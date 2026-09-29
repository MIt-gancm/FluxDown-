use std::rc::Rc;

use fluxdown_ui_components::{activity_button as activity_bar_button, nav_icon_color};
use fluxdown_ui_i18n::Translator;
use fluxdown_ui_theme::active_theme;
use gpui::{
    AnyElement, AnyView, App, Context, Div, Entity, FontWeight, InteractiveElement as _,
    IntoElement, MouseButton, ParentElement, Render, SharedString, StatefulInteractiveElement as _,
    Styled, Window, div, img, px,
};
use gpui_component::{Icon, TitleBar, h_flex, menu::AppMenuBar, tooltip::Tooltip, v_flex};

use crate::assets::APP_LOGO_PATH;

/// shell 路由的稳定标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteId(&'static str);

impl RouteId {
    /// 创建稳定路由标识。
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }
}

/// 由应用装配并注入 shell 的路由元数据与页面内容。
pub struct ShellRoute {
    id: RouteId,
    button_id: &'static str,
    tooltip_id: &'static str,
    label_key: &'static str,
    icon: Icon,
    view: AnyView,
    /// 路由活跃时渲染在统一顶栏中的插槽（靠右内容由插槽自己排布）。
    title_bar: Option<AnyView>,
    /// 可选路由参与活动栏「是否整体渲染」的判定；固定路由（如下载）不参与。
    optional: bool,
    visible: bool,
}

impl ShellRoute {
    /// 创建一条 shell 路由；默认固定显示（非可选、可见）。
    pub fn new(
        id: RouteId,
        button_id: &'static str,
        tooltip_id: &'static str,
        label_key: &'static str,
        icon: Icon,
        view: AnyView,
    ) -> Self {
        Self {
            id,
            button_id,
            tooltip_id,
            label_key,
            icon,
            view,
            title_bar: None,
            optional: false,
            visible: true,
        }
    }

    /// 标记该路由是否为「可选」：活动栏在没有任何可见可选项时整体收起。
    pub fn optional(mut self, optional: bool) -> Self {
        self.optional = optional;
        self
    }

    /// 为该路由挂载统一顶栏插槽：路由活跃时 shell 在标题栏中渲染它。
    ///
    /// 插槽内可交互元素须自行拦截左键 `mouse_down` 冒泡，空白处保持窗口拖拽 / 双击。
    pub fn with_title_bar(mut self, view: impl Into<AnyView>) -> Self {
        self.title_bar = Some(view.into());
        self
    }
}

type ShellActionHandler = Rc<dyn Fn(&mut Window, &mut App)>;
type ShellActionIcon = Rc<dyn Fn(&App) -> Icon>;

/// 由应用装配并注入 shell 的窗口级活动栏动作。
pub struct ShellAction {
    button_id: &'static str,
    tooltip_id: &'static str,
    label_key: &'static str,
    icon: ShellActionIcon,
    handler: ShellActionHandler,
    /// 可选动作参与活动栏「是否整体渲染」的判定；固定动作（如设置）不参与。
    optional: bool,
    visible: bool,
}

impl ShellAction {
    /// 创建不切换主内容路由的活动栏动作；默认固定显示（非可选、可见）。
    pub fn new(
        button_id: &'static str,
        tooltip_id: &'static str,
        label_key: &'static str,
        icon: Icon,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self::with_dynamic_icon(
            button_id,
            tooltip_id,
            label_key,
            move |_cx| icon.clone(),
            handler,
        )
    }

    /// 创建图标随运行时状态变化的活动栏动作（例如随主题模式在日/月间切换）。
    pub fn with_dynamic_icon(
        button_id: &'static str,
        tooltip_id: &'static str,
        label_key: &'static str,
        icon: impl Fn(&App) -> Icon + 'static,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            button_id,
            tooltip_id,
            label_key,
            icon: Rc::new(icon),
            handler: Rc::new(handler),
            optional: false,
            visible: true,
        }
    }

    /// 标记该动作是否为「可选」：活动栏在没有任何可见可选项时整体收起。
    pub fn optional(mut self, optional: bool) -> Self {
        self.optional = optional;
        self
    }
}

/// 使用 FluxDown 自定义标题栏承载任意能力页面的辅助窗口。
pub struct AuxiliaryWindowView {
    _translator: Entity<Translator>,
    title: SharedString,
    /// 宿主设置的动态标题（如任务文件名）；存在时优先于按语言刷新的默认标题。
    title_override: Option<SharedString>,
    content: AnyView,
}

impl AuxiliaryWindowView {
    /// 创建跟随共享语言状态更新标题的辅助窗口 chrome。
    pub fn new(
        translator: Entity<Translator>,
        title_key: &'static str,
        content: AnyView,
        cx: &mut Context<Self>,
    ) -> Self {
        let title = SharedString::from(translator.read(cx).text(title_key).to_owned());
        cx.observe(&translator, move |this, translator, cx| {
            this.title = SharedString::from(translator.read(cx).text(title_key).to_owned());
            cx.notify();
        })
        .detach();
        Self {
            _translator: translator,
            title,
            title_override: None,
            content,
        }
    }

    /// 以动态文本覆盖标题（传 `None` 恢复按语言键显示的默认标题）。
    pub fn set_title(&mut self, title: Option<SharedString>, cx: &mut Context<Self>) {
        if self.title_override != title {
            self.title_override = title;
            cx.notify();
        }
    }

    /// 辅助窗口标题栏：与主窗口统一顶栏同为 chrome 底 + hairline 底线；标题 sm MEDIUM，
    /// 三平台一致左对齐（macOS 紧随交通灯，Windows/Linux 自窗口左缘起）。
    fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = active_theme(cx);
        let tokens = theme.tokens();
        let extended = theme.extended().colors;
        let spacing = tokens.spacing;
        let typography = tokens.typography.clone();
        let title_bar = TitleBar::new();
        #[cfg(not(target_os = "macos"))]
        let title_bar = title_bar.pl(spacing.sm);

        // 显式 `.bg` 覆盖 gpui-component 默认渐变；`.h` 经 refine_style 覆盖默认 34px。
        title_bar
            .h(theme.density().title_bar)
            .bg(extended.chrome)
            .border_color(extended.hairline)
            .child(
                h_flex()
                    .size_full()
                    .min_w_0()
                    .items_center()
                    .pl(spacing.sm)
                    .pr(spacing.md)
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(typography.sm.size)
                            .line_height(typography.sm.line_height)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(tokens.colors.foreground)
                            .child(
                                self.title_override
                                    .clone()
                                    .unwrap_or_else(|| self.title.clone()),
                            ),
                    ),
            )
    }
}

impl Render for AuxiliaryWindowView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = active_theme(cx).tokens().colors;
        v_flex()
            .size_full()
            .relative()
            .bg(colors.surface)
            .text_color(colors.foreground)
            .child(self.render_title_bar(cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .bg(colors.surface)
                    .child(self.content.clone()),
            )
    }
}

/// 活动栏轨宽度：32px 按钮居中 + 左右各 8px 留白。
const ACTIVITY_RAIL_WIDTH: gpui::Pixels = px(48.);
/// 活动栏每个按钮位所占的行高（等于按钮自身高度，纵向间距由 `gap` 提供）。
const ACTIVITY_TILE_HEIGHT: gpui::Pixels = px(32.);
/// 活动栏按钮尺寸。
const ACTIVITY_BUTTON_SIZE: gpui::Pixels = px(32.);
/// 活动栏图标比 `extended.icon.lg` 大的余量（图标 = lg + 2，随 lg 缩放）。
const ACTIVITY_ICON_EXTRA: gpui::Pixels = px(2.);
/// GPUI 窗口外壳：只负责窗口 chrome、活动栏、路由与内容槽位。
pub struct ShellView {
    translator: Entity<Translator>,
    active_route: Option<RouteId>,
    routes: Vec<ShellRoute>,
    actions: Vec<ShellAction>,
    /// Windows / Linux 标题栏内的应用菜单；macOS 走原生菜单不渲染。
    menu_bar: Option<Entity<AppMenuBar>>,
}

impl ShellView {
    /// 创建 shell，并使用传入顺序的首条路由作为初始页面。
    pub fn new(
        translator: Entity<Translator>,
        routes: Vec<ShellRoute>,
        actions: Vec<ShellAction>,
        menu_bar: Option<Entity<AppMenuBar>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let active_route = routes.first().map(|route| route.id);
        cx.observe(&translator, |_, _, cx| cx.notify()).detach();
        Self {
            translator,
            active_route,
            routes,
            actions,
            menu_bar,
        }
    }

    /// 切换到指定路由（宿主导航入口）。
    pub fn navigate(&mut self, route: RouteId, cx: &mut Context<Self>) {
        if self.active_route != Some(route) {
            self.active_route = Some(route);
            cx.notify();
        }
    }

    /// 按活动栏按钮 id 设置可选路由或动作的可见性（宿主偏好回流入口）；
    /// 隐藏当前活跃路由时自动切到首条可见路由。
    pub fn set_entry_visible(&mut self, button_id: &str, visible: bool, cx: &mut Context<Self>) {
        if let Some(route) = self.routes.iter_mut().find(|r| r.button_id == button_id) {
            if route.visible == visible {
                return;
            }
            route.visible = visible;
            let id = route.id;
            if !visible && self.active_route == Some(id) {
                self.active_route = self.routes.iter().find(|r| r.visible).map(|r| r.id);
            }
            cx.notify();
            return;
        }
        let Some(action) = self.actions.iter_mut().find(|a| a.button_id == button_id) else {
            return;
        };
        if action.visible == visible {
            return;
        }
        action.visible = visible;
        cx.notify();
    }

    /// 统一顶栏：macOS 交通灯 / Windows·Linux logo + 应用菜单，其后是当前活跃路由的插槽；
    /// Windows·Linux 的窗口按钮由 `TitleBar` 自带。
    fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = active_theme(cx);
        let spacing = theme.tokens().spacing;
        let extended = theme.extended().colors;
        let is_macos = cfg!(target_os = "macos");
        // macOS 走原生菜单且交通灯占据左侧，不渲染 logo 与应用内菜单。
        let leading = (!is_macos).then(|| {
            h_flex()
                .h_full()
                .flex_none()
                .items_center()
                .gap(spacing.sm)
                .child(img(APP_LOGO_PATH).size(px(16.)))
                .children(self.menu_bar.clone().map(|menu_bar| {
                    h_flex()
                        .h_full()
                        .items_center()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(menu_bar)
                }))
        });
        let slot = self.active_title_bar();
        let title_bar = TitleBar::new();
        #[cfg(not(target_os = "macos"))]
        let title_bar = title_bar.pl(spacing.sm);

        // 显式 `.bg` 覆盖 gpui-component 默认渐变；`.h` 经 refine_style 覆盖默认 34px。
        title_bar
            .h(theme.density().title_bar)
            .bg(extended.chrome)
            .border_color(extended.hairline)
            .child(
                h_flex()
                    .size_full()
                    .min_w_0()
                    .items_center()
                    .gap(spacing.sm)
                    .pr(if is_macos { spacing.md } else { spacing.sm })
                    .children(leading)
                    .child(
                        h_flex()
                            .h_full()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .children(slot),
                    ),
            )
    }

    /// 当前活跃路由挂载的顶栏插槽。
    fn active_title_bar(&self) -> Option<AnyView> {
        self.active_route
            .and_then(|active| self.routes.iter().find(|route| route.id == active))
            .and_then(|route| route.title_bar.clone())
    }

    fn route_button(&self, route: &ShellRoute, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.active_route == Some(route.id);
        let icon_size = active_theme(cx).extended().icon.lg + ACTIVITY_ICON_EXTRA;
        let label = SharedString::from(self.translator.read(cx).text(route.label_key).to_owned());
        let tooltip_label = label.clone();
        let route_id = route.id;

        div()
            .id(route.tooltip_id)
            .w(ACTIVITY_RAIL_WIDTH)
            .h(ACTIVITY_TILE_HEIGHT)
            .flex()
            .items_center()
            .justify_center()
            .tooltip(move |window, cx| Tooltip::new(tooltip_label.clone()).build(window, cx))
            .child(
                activity_bar_button(
                    route.button_id,
                    label,
                    route
                        .icon
                        .clone()
                        .size(icon_size)
                        .text_color(nav_icon_color(selected, cx)),
                    selected,
                    ACTIVITY_BUTTON_SIZE,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.active_route != Some(route_id) {
                        this.active_route = Some(route_id);
                        cx.notify();
                    }
                })),
            )
            .into_any_element()
    }

    fn route_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let gap = active_theme(cx).tokens().spacing.xs;
        v_flex().gap(gap).children(
            self.routes
                .iter()
                .filter(|route| route.visible)
                .map(|route| self.route_button(route, cx)),
        )
    }

    fn action_button(&self, action: &ShellAction, cx: &mut Context<Self>) -> AnyElement {
        let label = SharedString::from(self.translator.read(cx).text(action.label_key).to_owned());
        let tooltip_label = label.clone();
        let handler = Rc::clone(&action.handler);
        let icon = (action.icon)(cx);
        let theme = active_theme(cx);
        let icon_size = theme.extended().icon.lg + ACTIVITY_ICON_EXTRA;
        let icon_color = theme.tokens().colors.muted_foreground;

        div()
            .id(action.tooltip_id)
            .w(ACTIVITY_RAIL_WIDTH)
            .h(ACTIVITY_TILE_HEIGHT)
            .flex()
            .items_center()
            .justify_center()
            .tooltip(move |window, cx| Tooltip::new(tooltip_label.clone()).build(window, cx))
            .child(
                activity_bar_button(
                    action.button_id,
                    label,
                    icon.size(icon_size).text_color(icon_color),
                    false,
                    ACTIVITY_BUTTON_SIZE,
                    cx,
                )
                .on_click(move |_, window, cx| handler(window, cx)),
            )
            .into_any_element()
    }

    fn action_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let gap = active_theme(cx).tokens().spacing.xs;
        v_flex().gap(gap).children(
            self.actions
                .iter()
                .filter(|action| action.visible)
                .map(|action| self.action_button(action, cx)),
        )
    }

    /// 是否存在至少一个可见的「可选」路由或动作；活动栏仅在此为真时渲染。
    fn has_visible_optional_item(&self) -> bool {
        self.routes
            .iter()
            .any(|route| route.optional && route.visible)
            || self
                .actions
                .iter()
                .any(|action| action.optional && action.visible)
    }

    fn render_activity_bar(&self, cx: &mut Context<Self>) -> Option<Div> {
        if !self.has_visible_optional_item() {
            return None;
        }
        let theme = active_theme(cx);
        let spacing = theme.tokens().spacing;
        let extended = theme.extended();
        // 与侧栏同为 chrome 底，靠右侧 hairline 结构线分界（与侧栏|内容的 resizable 把手同色同粗）。
        Some(
            v_flex()
                .h_full()
                .w(ACTIVITY_RAIL_WIDTH)
                .flex_none()
                .justify_between()
                .bg(extended.colors.chrome)
                .border_r(extended.stroke.thin)
                .border_color(extended.colors.hairline)
                .pt(spacing.sm)
                .pb(spacing.sm)
                .child(self.route_buttons(cx))
                .child(self.action_buttons(cx)),
        )
    }

    fn active_content(&self) -> AnyElement {
        self.active_route
            .and_then(|active| self.routes.iter().find(|route| route.id == active))
            .map_or_else(
                || div().into_any_element(),
                |route| route.view.clone().into_any_element(),
            )
    }
}

impl Render for ShellView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = active_theme(cx).tokens().colors;
        v_flex()
            .size_full()
            .relative()
            .bg(colors.background)
            .text_color(colors.foreground)
            .child(self.render_title_bar(cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_stretch()
                    .children(self.render_activity_bar(cx))
                    .child(
                        div()
                            .h_full()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .child(self.active_content()),
                    ),
            )
    }
}
