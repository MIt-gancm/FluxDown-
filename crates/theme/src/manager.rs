use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use gpui::{App, Global, SharedString, Window};
use gpui_component::{Theme as ComponentTheme, ThemeMode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    AppearancePreferences, BuiltinThemeId, ComponentTokens, DensityTokens, Diagnostic,
    ExtendedTokens, ResolveOptions, ResolvedTheme, SemanticThemeTokens, ThemeDocument,
    ThemeSelection, normalize_ui_scale_percent, resolve_with,
};

/// 用户主题偏好；`System` 在每次安装时解析当前系统明暗模式。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemePreference {
    fn resolve(self, cx: &App) -> ThemeMode {
        match self {
            Self::System => cx.window_appearance().into(),
            Self::Light => ThemeMode::Light,
            Self::Dark => ThemeMode::Dark,
        }
    }
}

/// 亮暗两个槽位各自使用的主题文件。
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeDocuments {
    pub dark: Arc<ThemeDocument>,
    pub light: Arc<ThemeDocument>,
}

impl ThemeDocuments {
    /// 同一文件同时用于两个槽位。
    #[must_use]
    pub fn single(document: Arc<ThemeDocument>) -> Self {
        Self {
            dark: Arc::clone(&document),
            light: document,
        }
    }

    #[must_use]
    pub fn get(&self, mode: ThemeMode) -> &Arc<ThemeDocument> {
        match mode {
            ThemeMode::Dark => &self.dark,
            ThemeMode::Light => &self.light,
        }
    }
}

/// 当前应用主题的完整快照。
///
/// gpui-component 的 legacy `Theme` 无法持久保存自定义 spacing/shadow；本全局状态
/// 因此保留完整解析结果，并在每次切换后重新投影到 `gpui_base::Theme`。
#[derive(Clone)]
pub struct FluxThemeState {
    documents: ThemeDocuments,
    appearance: AppearancePreferences,
    mode: ThemeMode,
    theme: ResolvedTheme,
    diagnostics: Arc<[Diagnostic]>,
}

impl Global for FluxThemeState {}

impl FluxThemeState {
    /// 当前明暗模式正在使用的主题文件（未缩放）。
    pub fn document(&self) -> &Arc<ThemeDocument> {
        self.documents.get(self.mode)
    }

    /// 两个槽位的主题文件。
    pub fn documents(&self) -> &ThemeDocuments {
        &self.documents
    }

    /// 用户当前的外观选项（主题槽位、强调色、缩放、明暗偏好）。
    pub fn appearance(&self) -> &AppearancePreferences {
        &self.appearance
    }

    /// 用户选择的明暗偏好。
    pub fn preference(&self) -> ThemePreference {
        self.appearance.theme_mode
    }

    /// 已解析的实际明暗模式。
    pub fn mode(&self) -> ThemeMode {
        self.mode
    }

    /// 界面缩放百分比（80 ~ 150）。
    pub fn ui_scale_percent(&self) -> u16 {
        self.appearance.ui_scale_percent
    }

    /// 完整运行时主题（已按界面缩放）。
    pub fn resolved(&self) -> &ResolvedTheme {
        &self.theme
    }

    /// 当前完整 Base token（已按界面缩放）；应用自有组件应只从这里取值。
    pub fn tokens(&self) -> &SemanticThemeTokens {
        &self.theme.base
    }

    /// FluxDown 扩展 token（扩展/状态/进度色、caption/title、图标、线宽、焦点环；已按界面缩放）。
    pub fn extended(&self) -> &ExtendedTokens {
        &self.theme.extended
    }

    /// 控件高度阶梯（已按界面缩放，标题栏除外）。
    pub fn density(&self) -> &DensityTokens {
        &self.theme.density
    }

    /// 组件级 token（已按界面缩放）。
    pub fn components(&self) -> &ComponentTokens {
        &self.theme.components
    }

    /// 当前模式主题文件的解析诊断。
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// 本机主题库中已加载的自定义主题（`custom:<id>` → 文件）。主题库的存取由调用方
/// 实现，本 crate 只保存已解析的文件。
#[derive(Default)]
struct CustomThemes(HashMap<SharedString, Arc<ThemeDocument>>);

impl Global for CustomThemes {}

/// 提供（或替换）一个自定义主题；当前槽位正在使用该 id 时立即重新安装。
pub fn register_custom_theme(
    id: impl Into<SharedString>,
    document: Arc<ThemeDocument>,
    cx: &mut App,
) {
    let id = id.into();
    cx.default_global::<CustomThemes>()
        .0
        .insert(id.clone(), document);
    reinstall_if_selected(&id, cx);
}

/// 移除一个自定义主题；当前槽位正在使用该 id 时回退到内置默认主题。
pub fn unregister_custom_theme(id: &str, cx: &mut App) {
    let removed = cx
        .try_global::<CustomThemes>()
        .is_some_and(|themes| themes.0.contains_key(id));
    if removed {
        cx.global_mut::<CustomThemes>().0.remove(id);
        reinstall_if_selected(id, cx);
    }
}

/// 已提供的自定义主题。
pub fn custom_theme(id: &str, cx: &App) -> Option<Arc<ThemeDocument>> {
    cx.try_global::<CustomThemes>()
        .and_then(|themes| themes.0.get(id).cloned())
}

fn reinstall_if_selected(id: &str, cx: &mut App) {
    let Some(state) = cx.try_global::<FluxThemeState>() else {
        return;
    };
    let appearance = state.appearance.clone();
    let selected = [&appearance.dark_theme, &appearance.light_theme]
        .into_iter()
        .any(|selection| {
            selection
                .custom_id()
                .is_some_and(|custom| custom.as_ref() == id)
        });
    if selected {
        let documents = selection_documents(&appearance, cx);
        install(documents, appearance, None, cx);
    }
}

/// 槽位选择 → 主题文件；未提供的自定义主题回退到该槽位的内置默认主题。
fn selection_documents(appearance: &AppearancePreferences, cx: &App) -> ThemeDocuments {
    let document = |mode: ThemeMode| match appearance.theme(mode) {
        ThemeSelection::Builtin(id) => Arc::new(ThemeDocument::builtin(*id)),
        ThemeSelection::Custom(id) => custom_theme(id, cx)
            .unwrap_or_else(|| Arc::new(ThemeDocument::builtin(BuiltinThemeId::default_for(mode)))),
    };
    ThemeDocuments {
        dark: document(ThemeMode::Dark),
        light: document(ThemeMode::Light),
    }
}

/// 在 `gpui_component::init` 后安装与 Flutter 客户端一致的默认主题。
pub fn init(cx: &mut App) {
    let appearance = AppearancePreferences::default();
    let documents = selection_documents(&appearance, cx);
    install(documents, appearance, None, cx);
}

/// 返回当前完整主题状态。
///
/// 调用方必须先执行 [`init`] 或 [`install_document`]；桌面 shell 在创建任何 view 前
/// 建立这一不变量。
pub fn active_theme(cx: &App) -> &FluxThemeState {
    cx.global::<FluxThemeState>()
}

/// 把一个主题文件同时装入亮暗两个槽位并同步到 gpui-component 与 gpui-base。
///
/// 其余外观选项（强调色、缩放）沿用当前状态；未初始化时取默认值。之后的
/// [`set_appearance`] 在主题槽位与强调色不变时继续沿用该文件。
pub fn install_document(
    document: Arc<ThemeDocument>,
    preference: ThemePreference,
    window: Option<&mut Window>,
    cx: &mut App,
) {
    let mut appearance = cx
        .try_global::<FluxThemeState>()
        .map_or_else(AppearancePreferences::default, |state| {
            state.appearance.clone()
        });
    appearance.theme_mode = preference;
    install(ThemeDocuments::single(document), appearance, window, cx);
}

/// 应用一组外观选项。主题槽位/强调色未变时沿用当前文件（含通过
/// [`install_document`] 装入的文件），否则按槽位选择重新取文件。
pub fn set_appearance(
    appearance: AppearancePreferences,
    window: Option<&mut Window>,
    cx: &mut App,
) {
    let documents = match cx.try_global::<FluxThemeState>() {
        Some(state) if state.appearance.same_palette(&appearance) => state.documents.clone(),
        _ => selection_documents(&appearance, cx),
    };
    install(documents, appearance, window, cx);
}

/// 偏好快照 → 外观。读取 `appearance.theme_mode` / `appearance.dark_theme` /
/// `appearance.light_theme` / `appearance.color_scheme` / `appearance.custom_color` /
/// `ui_scale`；与当前状态一致时不做任何事，可在每次快照/偏好事件上幂等调用。
pub fn apply_appearance_preferences(values: &BTreeMap<String, Value>, cx: &mut App) {
    let appearance = AppearancePreferences::from_values(values);
    if cx
        .try_global::<FluxThemeState>()
        .is_some_and(|state| state.appearance == appearance)
    {
        return;
    }
    set_appearance(appearance, None, cx);
}

/// 保留当前主题定义，仅切换明暗偏好。
pub fn set_theme_preference(
    preference: ThemePreference,
    window: Option<&mut Window>,
    cx: &mut App,
) {
    let mut appearance = active_theme(cx).appearance.clone();
    appearance.theme_mode = preference;
    set_appearance(appearance, window, cx);
}

/// 设置界面缩放百分比（限制到 80 ~ 150，按 10 取整）。
///
/// GPUI 只有逐窗口的 rem 尺寸；gpui-component 的 `Root` 每帧把
/// `Theme::font_size` 写入窗口 rem，因此这里通过缩放排版/间距/圆角 token
/// 驱动所有窗口，无需逐窗口调用 `set_rem_size`。
pub fn set_ui_scale(percent: u16, cx: &mut App) {
    let mut appearance = active_theme(cx).appearance.clone();
    appearance.ui_scale_percent = normalize_ui_scale_percent(percent);
    set_appearance(appearance, None, cx);
}

/// 在亮/暗两种显式模式间切换。
pub fn toggle_theme(window: &mut Window, cx: &mut App) {
    let preference = if active_theme(cx).mode().is_dark() {
        ThemePreference::Light
    } else {
        ThemePreference::Dark
    };
    set_theme_preference(preference, Some(window), cx);
}

/// 系统外观变化时刷新 `System` 偏好；显式亮/暗偏好保持不变。
pub fn sync_system_theme(window: &mut Window, cx: &mut App) {
    if active_theme(cx).preference() == ThemePreference::System {
        set_theme_preference(ThemePreference::System, Some(window), cx);
    }
}

fn install(
    documents: ThemeDocuments,
    appearance: AppearancePreferences,
    window: Option<&mut Window>,
    cx: &mut App,
) {
    let mode = appearance.theme_mode.resolve(cx);
    let options = ResolveOptions {
        accent: Some(appearance.accent()),
        ensure_primary_contrast: true,
    };
    let (values, diagnostics) = resolve_with(documents.get(mode), mode, &options);
    let theme = values.to_theme(appearance.ui_scale());
    let tokens = &theme.base;
    let extended = &theme.extended;

    ComponentTheme::change(mode, None, cx);
    {
        let component_theme = ComponentTheme::global_mut(cx);
        component_theme.apply_semantic_tokens(tokens);
        component_theme.focus_ring = false;
        // gpui-component 的对话框、输入框、Root 等取 `background`；FluxDown 的内容面统一是
        // `surface`（chrome 区由各页显式着色），这里对齐，避免对话框发灰与白色内容区不一致。
        component_theme.background = tokens.colors.surface;
        component_theme.tokens.background = tokens.colors.surface.into();
        // gpui-component 的 `text_base` 取 `font_size`（默认映射到 md=16px），
        // 对桌面密度偏大；与 Flutter 桌面端 13px 正文基线对齐取 sm。
        component_theme.font_size = tokens.typography.sm.size;
        component_theme.title_bar = extended.colors.chrome;
        component_theme.title_bar_border = extended.colors.hairline;
        // gpui-component 的 Sidebar / Settings 侧栏只读 sidebar_* 系列，legacy
        // `apply_semantic_tokens` 不会同步它们；不映射就会留在库默认的黑/白。
        component_theme.sidebar = tokens.colors.surface;
        component_theme.sidebar_foreground = tokens.colors.surface_foreground;
        component_theme.sidebar_border = tokens.colors.border;
        component_theme.sidebar_accent = tokens.colors.accent;
        component_theme.sidebar_accent_foreground = tokens.colors.accent_foreground;
        component_theme.sidebar_primary = tokens.colors.primary;
        component_theme.sidebar_primary_foreground = tokens.colors.primary_foreground;
        component_theme.tokens.sidebar = tokens.colors.surface.into();
        component_theme.tokens.sidebar_foreground = tokens.colors.surface_foreground.into();
        component_theme.tokens.sidebar_border = tokens.colors.border.into();
        component_theme.tokens.sidebar_accent = tokens.colors.accent.into();
        component_theme.tokens.sidebar_accent_foreground = tokens.colors.accent_foreground.into();
        component_theme.tokens.sidebar_primary = tokens.colors.primary.into();
        component_theme.tokens.sidebar_primary_foreground = tokens.colors.primary_foreground.into();
        // legacy Button/Link 系列同样不在 apply_semantic_tokens 内；不同步则 gpui-component
        // 的 `Button::primary()` 永远是库默认黑/白，不跟随强调色。
        let colors = tokens.colors;
        let primary_hover = shift_toward_contrast(colors.primary, 0.08);
        let primary_active = shift_toward_contrast(colors.primary, 0.13);
        let secondary_hover = shift_toward_contrast(colors.secondary, 0.05);
        let secondary_active = shift_toward_contrast(colors.secondary, 0.09);
        let danger_hover = shift_toward_contrast(colors.destructive, 0.08);
        let danger_active = shift_toward_contrast(colors.destructive, 0.13);
        component_theme.primary_hover = primary_hover;
        component_theme.primary_active = primary_active;
        component_theme.secondary_hover = secondary_hover;
        component_theme.secondary_active = secondary_active;
        component_theme.danger_hover = danger_hover;
        component_theme.danger_active = danger_active;
        component_theme.link = colors.primary;
        component_theme.button_primary = colors.primary;
        component_theme.button_primary_foreground = colors.primary_foreground;
        component_theme.button_primary_hover = primary_hover;
        component_theme.button_primary_active = primary_active;
        component_theme.button_secondary = colors.secondary;
        component_theme.button_secondary_foreground = colors.secondary_foreground;
        component_theme.button_secondary_hover = secondary_hover;
        component_theme.button_secondary_active = secondary_active;
        component_theme.button_danger = colors.destructive;
        component_theme.button_danger_foreground = colors.destructive_foreground;
        component_theme.button_danger_hover = danger_hover;
        component_theme.button_danger_active = danger_active;
        component_theme.tokens.primary_hover = primary_hover.into();
        component_theme.tokens.primary_active = primary_active.into();
        component_theme.tokens.secondary_hover = secondary_hover.into();
        component_theme.tokens.secondary_active = secondary_active.into();
        component_theme.tokens.danger_hover = danger_hover.into();
        component_theme.tokens.danger_active = danger_active.into();
        component_theme.tokens.link = colors.primary.into();
        component_theme.tokens.button_primary = colors.primary.into();
        component_theme.tokens.button_primary_foreground = colors.primary_foreground.into();
        component_theme.tokens.button_primary_hover = primary_hover.into();
        component_theme.tokens.button_primary_active = primary_active.into();
        component_theme.tokens.button_secondary = colors.secondary.into();
        component_theme.tokens.button_secondary_foreground = colors.secondary_foreground.into();
        component_theme.tokens.button_secondary_hover = secondary_hover.into();
        component_theme.tokens.button_secondary_active = secondary_active.into();
        component_theme.tokens.button_danger = colors.destructive.into();
        component_theme.tokens.button_danger_foreground = colors.destructive_foreground.into();
        component_theme.tokens.button_danger_hover = danger_hover.into();
        component_theme.tokens.button_danger_active = danger_active.into();
        // 下载列表不画网格：行分隔线与表头竖线都取透明，只靠悬停 / 选中底色
        // 区分行；表头与内容同底色。列宽拖拽柄在悬停表头时仍按 `border` 显示。
        let no_line = tokens.colors.surface.opacity(0.);
        component_theme.table = tokens.colors.surface;
        component_theme.table_active = tokens.colors.accent;
        component_theme.table_active_border = tokens.colors.primary;
        component_theme.table_even = tokens.colors.surface;
        component_theme.table_head = tokens.colors.surface;
        component_theme.table_head_foreground = extended.colors.text_tertiary;
        component_theme.table_hover = extended.colors.row_hover;
        component_theme.table_row_border = no_line;
        component_theme.tokens.table = tokens.colors.surface.into();
        component_theme.tokens.table_active = tokens.colors.accent.into();
        component_theme.tokens.table_even = tokens.colors.surface.into();
        component_theme.tokens.table_head = tokens.colors.surface.into();
        component_theme.tokens.table_hover.background = extended.colors.row_hover.into();
    }
    ComponentTheme::sync_base(cx);
    {
        let base_theme = gpui_base::Theme::global_mut(cx);
        base_theme.tokens = tokens.clone();
        // 面板分隔把手（侧栏|内容、详情面板）与其他结构线一致用 hairline。
        base_theme.resizable.handle = extended.colors.hairline;
    }
    cx.set_global(FluxThemeState {
        documents,
        appearance,
        mode,
        theme,
        diagnostics: diagnostics.into(),
    });

    for handle in cx.windows() {
        let _ = handle.update(cx, |_, window, _| window.refresh());
    }
    if let Some(window) = window {
        window.refresh();
    }
}

/// 向对比方向偏移亮度：亮色变暗、暗色变亮（hover / active 派生）。
fn shift_toward_contrast(color: gpui::Hsla, amount: f32) -> gpui::Hsla {
    let delta = if color.l >= 0.5 { -amount } else { amount };
    gpui::Hsla {
        l: (color.l + delta).clamp(0., 1.),
        ..color
    }
}
