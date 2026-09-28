//! 本机主题库在设置能力内的状态：app 启动时装入全部已导入主题并注册到主题 crate；
//! 外观分区经此导入、删除与导出。
//!
//! 文件存取经 [`ThemeLibrary`] 端口（app 以 `<data_dir>/themes/<id>.json` 实现），
//! 这里只持有解析结果与卡片预览色。

use std::{io, sync::Arc};

use fluxdown_ui_theme::{
    ACCENT_TOKEN_PATHS, BuiltinBase, BuiltinThemeId, ColorTokens, Diagnostic, DiagnosticKind,
    ResolveOptions, ThemeDocument, ThemeMode, ThemeParseError, ThemeSelection, TokenLayer,
    TokenValue, active_theme, color_hex, custom_theme, register_custom_theme, resolve,
    resolve_with, unregister_custom_theme,
};
use gpui::{App, Global, Hsla, SharedString};

use crate::port::ThemeLibrary;

/// 一个已导入并注册的主题。
#[derive(Clone)]
pub(crate) struct ImportedTheme {
    pub id: SharedString,
    /// `meta.name`，缺省为 id。
    pub name: SharedString,
    pub document: Arc<ThemeDocument>,
    preview_dark: ColorTokens,
    preview_light: ColorTokens,
}

impl ImportedTheme {
    fn new(id: String, document: ThemeDocument) -> Self {
        let name = document
            .meta
            .as_ref()
            .and_then(|meta| meta.name.as_deref())
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map_or_else(|| id.clone(), str::to_owned);
        let preview = |mode| resolve(&document, mode).0.to_theme(1.).base.colors;
        Self {
            preview_dark: preview(ThemeMode::Dark),
            preview_light: preview(ThemeMode::Light),
            id: SharedString::from(id),
            name: SharedString::from(name),
            document: Arc::new(document),
        }
    }

    /// 卡片预览色（纯文件解析，不含用户强调色，与内置卡片一致）。
    pub fn preview(&self, mode: ThemeMode) -> ColorTokens {
        if mode.is_dark() {
            self.preview_dark
        } else {
            self.preview_light
        }
    }

    pub fn available_in(&self, mode: ThemeMode) -> bool {
        theme_available_in(&self.document, mode)
    }
}

/// 主题能否放进 `mode` 槽位，满足任一即可：
/// 1. 文件含该模式层（`dark` / `light` 非空）；
/// 2. `extends` 为该模式外观的单模式预设（如 `builtin:nord` → 暗色）；
/// 3. 文件没有任何模式倾向：两个模式层都为空，且 `extends` 不是单模式预设
///    （缺省、`builtin:default` 或未知值）——只改共享 `tokens` 的主题两个槽位都可用。
pub(crate) fn theme_available_in(document: &ThemeDocument, mode: ThemeMode) -> bool {
    let has_layer = |mode| !document.layer(TokenLayer::for_mode(mode)).is_empty();
    let preset_mode = document
        .extends
        .as_deref()
        .and_then(BuiltinBase::parse)
        .and_then(BuiltinBase::preset)
        .map(BuiltinThemeId::appearance);
    if has_layer(mode) || preset_mode == Some(mode) {
        return true;
    }
    let other = if mode.is_dark() {
        ThemeMode::Light
    } else {
        ThemeMode::Dark
    };
    preset_mode.is_none() && !has_layer(other)
}

struct ThemeLibraryState {
    library: Arc<dyn ThemeLibrary>,
    themes: Vec<ImportedTheme>,
}

impl Global for ThemeLibraryState {}

/// 装配主题库：读取、解析并注册库内全部主题（之后应用的 `custom:<id>` 偏好即可命中；
/// 找不到的 id 由主题 crate 回退到该槽位的内置默认主题）。返回无法加载的条目说明，
/// 由调用方记录；单个文件失败不影响其余。
pub fn install_theme_library(library: Arc<dyn ThemeLibrary>, cx: &mut App) -> Vec<String> {
    let mut failures = Vec::new();
    let infos = library.list().unwrap_or_else(|error| {
        failures.push(format!("theme library: {error}"));
        Vec::new()
    });
    let mut themes = Vec::with_capacity(infos.len());
    for info in infos {
        let stored = match library.load(&info.id) {
            Ok(stored) => stored,
            Err(error) => {
                failures.push(format!("{}: {error}", info.id));
                continue;
            }
        };
        match ThemeDocument::parse(&stored.text) {
            Ok((document, _)) => themes.push(ImportedTheme::new(info.id, document)),
            Err(error) => failures.push(format!("{}: {error}", info.id)),
        }
    }
    for theme in &themes {
        register_custom_theme(theme.id.clone(), Arc::clone(&theme.document), cx);
    }
    cx.set_global(ThemeLibraryState { library, themes });
    failures
}

/// 主题库端口；app 未装配时为 `None`（导入 / 删除不可用）。
pub(crate) fn library(cx: &App) -> Option<Arc<dyn ThemeLibrary>> {
    cx.try_global::<ThemeLibraryState>()
        .map(|state| Arc::clone(&state.library))
}

/// 已导入的主题（库内 id 升序，本次运行新导入的追加在后）。
pub(crate) fn imported_themes(cx: &App) -> Vec<ImportedTheme> {
    cx.try_global::<ThemeLibraryState>()
        .map(|state| state.themes.clone())
        .unwrap_or_default()
}

/// 单个文件导入失败的原因。
#[derive(Debug)]
pub(crate) enum ImportError {
    Read(io::Error),
    Parse(ThemeParseError),
    Save(io::Error),
}

impl ImportError {
    /// 对应的 i18n 文案键。
    pub(crate) fn i18n_key(&self) -> &'static str {
        match self {
            Self::Read(_) => "themeImportReadFailed",
            Self::Parse(ThemeParseError::InvalidJson(_)) => "themeImportInvalidJson",
            Self::Parse(ThemeParseError::NotAnObject) => "themeImportNotObject",
            Self::Parse(ThemeParseError::UnsupportedFormat(_)) => "themeImportUnsupportedFormat",
            Self::Save(_) => "themeImportSaveFailed",
        }
    }

    /// 系统错误详情（读写失败时）；解析错误的原因已由文案键表达。
    pub(crate) fn io_detail(&self) -> Option<&io::Error> {
        match self {
            Self::Read(error) | Self::Save(error) => Some(error),
            Self::Parse(_) => None,
        }
    }
}

/// 一次成功的导入：已保存到库，尚未注册（注册须在主线程经 [`register_imported`]）。
pub(crate) struct ImportOutcome {
    pub theme: ImportedTheme,
    pub diagnostics: Vec<Diagnostic>,
}

/// 解析（含 v1 / Flutter 格式）并把原文原样存入库；可在后台线程执行。
pub(crate) fn import_text(
    library: &dyn ThemeLibrary,
    text: &str,
) -> Result<ImportOutcome, ImportError> {
    let (document, diagnostics) = ThemeDocument::parse(text).map_err(ImportError::Parse)?;
    let preferred = document.meta.as_ref().and_then(|meta| meta.id.as_deref());
    let id = library.save(text, preferred).map_err(ImportError::Save)?;
    Ok(ImportOutcome {
        theme: ImportedTheme::new(id, document),
        diagnostics,
    })
}

/// 注册新导入的主题并加入列表（同 id 替换）。
pub(crate) fn register_imported(theme: ImportedTheme, cx: &mut App) {
    register_custom_theme(theme.id.clone(), Arc::clone(&theme.document), cx);
    if cx.has_global::<ThemeLibraryState>() {
        let themes = &mut cx.global_mut::<ThemeLibraryState>().themes;
        themes.retain(|existing| existing.id != theme.id);
        themes.push(theme);
    }
}

/// 从库中删除并注销。调用方应先把引用该 id 的槽位改回内置主题并写偏好。
pub(crate) fn delete_theme(id: &str, cx: &mut App) -> io::Result<()> {
    let Some(library) = library(cx) else {
        return Ok(());
    };
    library.delete(id)?;
    if cx.has_global::<ThemeLibraryState>() {
        cx.global_mut::<ThemeLibraryState>()
            .themes
            .retain(|theme| theme.id.as_ref() != id);
    }
    unregister_custom_theme(id, cx);
    Ok(())
}

/// 诊断汇总的展示顺序与文案键。
pub(crate) const DIAGNOSTIC_LABELS: [(DiagnosticKind, &str); 7] = [
    (DiagnosticKind::Migrated, "themeDiagMigrated"),
    (DiagnosticKind::UnknownKey, "themeDiagUnknownKey"),
    (DiagnosticKind::InvalidValue, "themeDiagInvalidValue"),
    (DiagnosticKind::OutOfRange, "themeDiagOutOfRange"),
    (DiagnosticKind::NewerVersion, "themeDiagNewerVersion"),
    (DiagnosticKind::UnknownExtends, "themeDiagUnknownExtends"),
    (DiagnosticKind::RefCycle, "themeDiagRefCycle"),
];

/// 按 [`DIAGNOSTIC_LABELS`] 顺序统计每类诊断的条数（只含非零项）。
pub(crate) fn diagnostic_counts(diagnostics: &[Diagnostic]) -> Vec<(&'static str, usize)> {
    DIAGNOSTIC_LABELS
        .iter()
        .filter_map(|(kind, key)| {
            let count = diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.kind == *kind)
                .count();
            (count > 0).then_some((*key, count))
        })
        .collect()
}

/// 当前明暗模式正在生效的主题文件（导出用）。
///
/// 自定义主题原样返回（未知键保留）；内置主题（含自定义主题缺失时的回退）把用户强调色
/// 写入两个模式层，使导出文件在任何客户端里与当前所见一致。
pub(crate) fn export_document(cx: &App) -> ThemeDocument {
    let state = active_theme(cx);
    let mode = state.mode();
    let appearance = state.appearance();
    if let ThemeSelection::Custom(id) = appearance.theme(mode)
        && let Some(document) = custom_theme(id, cx)
    {
        return (*document).clone();
    }
    builtin_with_accent(appearance.builtin_theme(mode), appearance.accent())
}

pub(crate) fn builtin_with_accent(id: BuiltinThemeId, accent: Hsla) -> ThemeDocument {
    let mut document = ThemeDocument::builtin(id);
    let options = ResolveOptions {
        accent: Some(accent),
        ensure_primary_contrast: false,
    };
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let (values, _) = resolve_with(&document, mode, &options);
        for path in ACCENT_TOKEN_PATHS {
            if let Some(TokenValue::Color(color)) = values.get(path) {
                let hex = color_hex(*color);
                document.set_token(TokenLayer::for_mode(mode), path, hex.into());
            }
        }
    }
    document
}

/// 导出文件的建议文件名：`meta.name`（缺省 `meta.id`）清洗为小写 `a-z0-9-`。
pub(crate) fn export_file_name(document: &ThemeDocument) -> String {
    let source = document
        .meta
        .as_ref()
        .and_then(|meta| meta.name.as_deref().or(meta.id.as_deref()))
        .unwrap_or_default();
    let mut stem = String::with_capacity(source.len());
    for ch in source.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            stem.push(ch);
        } else if !stem.is_empty() && !stem.ends_with('-') {
            stem.push('-');
        }
    }
    let stem = stem.trim_end_matches('-');
    if stem.is_empty() {
        "fluxdown-theme.json".to_owned()
    } else {
        format!("{stem}.json")
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Mutex};

    use fluxdown_ui_theme::{ExportMode, argb_color, resolve};

    use super::*;
    use crate::port::{StoredTheme, ThemeInfo};

    /// 内存主题库：id 取 `preferred_id` 或 `theme`，冲突加 `-N`。
    #[derive(Default)]
    struct MemoryLibrary(Mutex<BTreeMap<String, String>>);

    impl ThemeLibrary for MemoryLibrary {
        fn list(&self) -> io::Result<Vec<ThemeInfo>> {
            let themes = self.0.lock().map_err(|_| io::ErrorKind::Other)?;
            Ok(themes
                .keys()
                .map(|id| ThemeInfo {
                    id: id.clone(),
                    modified: None,
                })
                .collect())
        }

        fn load(&self, id: &str) -> io::Result<StoredTheme> {
            let themes = self.0.lock().map_err(|_| io::ErrorKind::Other)?;
            let text = themes.get(id).cloned().ok_or(io::ErrorKind::NotFound)?;
            Ok(StoredTheme {
                info: ThemeInfo {
                    id: id.to_owned(),
                    modified: None,
                },
                text,
            })
        }

        fn save(&self, text: &str, preferred_id: Option<&str>) -> io::Result<String> {
            let mut themes = self.0.lock().map_err(|_| io::ErrorKind::Other)?;
            let base = preferred_id.unwrap_or("theme").to_owned();
            let mut id = base.clone();
            let mut n = 2;
            while themes.contains_key(&id) {
                id = format!("{base}-{n}");
                n += 1;
            }
            themes.insert(id.clone(), text.to_owned());
            Ok(id)
        }

        fn delete(&self, id: &str) -> io::Result<()> {
            let mut themes = self.0.lock().map_err(|_| io::ErrorKind::Other)?;
            themes.remove(id);
            Ok(())
        }
    }

    fn document(json: &str) -> Result<ThemeDocument, ThemeParseError> {
        ThemeDocument::parse(json).map(|(doc, _)| doc)
    }

    #[test]
    fn slot_availability_follows_mode_layers_and_extends() -> Result<(), Box<dyn std::error::Error>>
    {
        let dark_layer = document(r##"{"dark":{"colors":{"primary":"#ff0000"}}}"##)?;
        assert!(theme_available_in(&dark_layer, ThemeMode::Dark));
        assert!(!theme_available_in(&dark_layer, ThemeMode::Light));

        let nord = document(r#"{"extends":"builtin:nord"}"#)?;
        assert!(theme_available_in(&nord, ThemeMode::Dark));
        assert!(!theme_available_in(&nord, ThemeMode::Light));

        // 暗色预设 + 亮色层：亮色层让它也能进亮色槽位。
        let nord_with_light =
            document(r##"{"extends":"builtin:nord","light":{"colors":{"primary":"#ff0000"}}}"##)?;
        assert!(theme_available_in(&nord_with_light, ThemeMode::Dark));
        assert!(theme_available_in(&nord_with_light, ThemeMode::Light));

        // 只有共享 tokens、无模式倾向：两个槽位都可用。
        let shared = document(r#"{"tokens":{"radius":{"md":8}}}"#)?;
        assert!(theme_available_in(&shared, ThemeMode::Dark));
        assert!(theme_available_in(&shared, ThemeMode::Light));
        Ok(())
    }

    #[test]
    fn flutter_theme_imports_into_its_appearance_slot() -> Result<(), Box<dyn std::error::Error>> {
        let library = MemoryLibrary::default();
        let flutter =
            r#"{"name":"Ocean","appearance":"light","colors":{"accent":{"color":"FF0EA5E9"}}}"#;
        let outcome = import_text(&library, flutter).map_err(|error| format!("{error:?}"))?;
        assert!(outcome.theme.available_in(ThemeMode::Light));
        assert!(!outcome.theme.available_in(ThemeMode::Dark));
        assert_eq!(outcome.theme.name.as_ref(), "Ocean");
        assert_eq!(
            diagnostic_counts(&outcome.diagnostics),
            vec![("themeDiagMigrated", 1)]
        );
        // 原文原样入库（不是转换后的文件）。
        assert_eq!(library.load(&outcome.theme.id)?.text, flutter);
        Ok(())
    }

    #[test]
    fn import_reload_round_trip_keeps_unknown_keys_and_suffixes_conflicts()
    -> Result<(), Box<dyn std::error::Error>> {
        let library = MemoryLibrary::default();
        let text = r##"{
          "format": "fluxdown.gpui-theme",
          "schemaVersion": 2,
          "meta": { "id": "ocean", "name": "Ocean", "x-origin": "gallery" },
          "x-top": { "keep": true },
          "dark": { "colors": { "primary": "#0ea5e9", "x-glow": "#ffffff" } }
        }"##;
        let first = import_text(&library, text).map_err(|error| format!("{error:?}"))?;
        let second = import_text(&library, text).map_err(|error| format!("{error:?}"))?;
        assert_eq!(first.theme.id.as_ref(), "ocean");
        assert_eq!(second.theme.id.as_ref(), "ocean-2");
        let counts = diagnostic_counts(&first.diagnostics);
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0].0, "themeDiagUnknownKey");

        let reloaded = library.load("ocean")?;
        let (document, _) = ThemeDocument::parse(&reloaded.text)?;
        let exported: serde_json::Value =
            serde_json::from_str(&document.to_json_pretty(ExportMode::Diff))?;
        assert_eq!(exported["x-top"]["keep"], true);
        assert_eq!(exported["meta"]["x-origin"], "gallery");
        assert_eq!(exported["dark"]["colors"]["x-glow"], "#ffffff");
        Ok(())
    }

    #[test]
    fn unparseable_import_is_not_saved() -> Result<(), Box<dyn std::error::Error>> {
        let library = MemoryLibrary::default();
        let error = import_text(&library, "[1, 2]")
            .err()
            .ok_or("expected an error")?;
        assert_eq!(error.i18n_key(), "themeImportNotObject");
        let error = import_text(&library, "{")
            .err()
            .ok_or("expected an error")?;
        assert_eq!(error.i18n_key(), "themeImportInvalidJson");
        assert!(library.list()?.is_empty());
        Ok(())
    }

    #[test]
    fn builtin_export_carries_accent_in_both_modes() -> Result<(), Box<dyn std::error::Error>> {
        let rose = argb_color(0xFFF4_3F5E);
        let document = builtin_with_accent(BuiltinThemeId::Nord, rose);
        let reparsed = ThemeDocument::parse(&document.to_json_pretty(ExportMode::Diff))?.0;
        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            let (values, _) = resolve(&reparsed, mode);
            let Some(TokenValue::Color(primary)) = values.get("colors.primary") else {
                panic!("colors.primary missing in {mode:?}");
            };
            assert_eq!(color_hex(*primary), "#f43f5eff", "{mode:?}");
        }
        // Nord 预设固定的 primaryForeground 不随强调色改变。
        let (plain, _) = resolve(
            &ThemeDocument::builtin(BuiltinThemeId::Nord),
            ThemeMode::Dark,
        );
        let (values, _) = resolve(&reparsed, ThemeMode::Dark);
        assert_eq!(
            values.get("colors.primaryForeground"),
            plain.get("colors.primaryForeground")
        );
        Ok(())
    }

    #[test]
    fn builtin_export_pins_accent_even_when_equal_to_base() -> Result<(), Box<dyn std::error::Error>>
    {
        // 强调色等于基底默认值时 diff 导出曾把强调色路径全部精简掉，导入方的强调色随即覆盖。
        let blue = argb_color(0xFF3B_82F6);
        let rose = argb_color(0xFFF4_3F5E);
        let document = builtin_with_accent(BuiltinThemeId::DefaultDark, blue);
        let reparsed = ThemeDocument::parse(&document.to_json_pretty(ExportMode::Diff))?.0;
        let importer = ResolveOptions {
            accent: Some(rose),
            ensure_primary_contrast: false,
        };
        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            let (exported, _) = resolve(&document, mode);
            let (imported, _) = resolve_with(&reparsed, mode, &importer);
            for path in ACCENT_TOKEN_PATHS {
                assert_eq!(imported.get(path), exported.get(path), "{mode:?} {path}");
            }
        }
        Ok(())
    }

    #[test]
    fn export_file_name_is_sanitized() -> Result<(), Box<dyn std::error::Error>> {
        let mut document = ThemeDocument::builtin(BuiltinThemeId::MidnightBlue);
        assert_eq!(export_file_name(&document), "midnight-blue.json");
        document.meta = None;
        assert_eq!(export_file_name(&document), "fluxdown-theme.json");
        Ok(())
    }
}
