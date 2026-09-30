//! 表头选择条：选中 ≥1 项时覆盖全选框右侧的列头，承载批量操作；取消选择后恢复列名。
//! 不遮挡任何任务行，紧挨刚点过的复选框，鼠标移动距离最短。

use fluxdown_ui_components::{
    FluxIcon, IconControlExt as _, tabular_numbers, toolbar_action_button,
};
use fluxdown_ui_theme::active_theme;
use gpui::{
    Anchor, AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement as _, Styled, Window, div, px,
};
use gpui_component::{
    Icon,
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    tooltip::Tooltip,
};

use crate::{
    components::task_table::{SELECTION_COLUMN_WIDTH, TABLE_HEADER_HEIGHT, ToolbarCommand},
    pages::downloads::DownloadView,
};

impl DownloadView {
    /// chrome 风格图标按钮 + 悬浮提示；`on_click` 在下载页上下文执行。
    pub(crate) fn icon_action(
        &self,
        id: &'static str,
        label: SharedString,
        icon: Icon,
        destructive: bool,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.icon_action_with_state(id, label, icon, destructive, false, on_click, cx)
    }

    /// [`Self::icon_action`] 的可禁用版本：`disabled` 时保留悬浮提示但不响应点击。
    #[allow(clippy::too_many_arguments)]
    fn icon_action_with_state(
        &self,
        id: &'static str,
        label: SharedString,
        icon: Icon,
        destructive: bool,
        disabled: bool,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tooltip_label = label.clone();
        div()
            .id(SharedString::from(format!("{id}-tooltip")))
            .flex_none()
            .tooltip(move |window, cx| Tooltip::new(tooltip_label.clone()).build(window, cx))
            .child(
                toolbar_action_button(id, label, icon, destructive, disabled, cx)
                    .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx))),
            )
            .into_any_element()
    }

    /// 删除入口：下拉区分「删除任务」与「删除任务和文件」（后者二次确认）。
    fn selection_delete_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = active_theme(cx);
        let tokens = theme.tokens();
        let radius = theme.components().button_radius;
        let destructive = tokens.colors.destructive;
        let icon_size = theme.extended().icon.md;
        let delete_task = self.strings.delete_task.clone();
        let delete_with_files = self.strings.delete_task_and_file.clone();
        let view = cx.weak_entity();
        div()
            .flex_none()
            .child(
                Button::new("download-selection-delete")
                    .ghost()
                    .control_icon(cx)
                    .rounded(radius)
                    .tooltip(self.strings.delete.clone())
                    .child(
                        Icon::new(FluxIcon::Trash2)
                            .size(icon_size)
                            .text_color(destructive),
                    )
                    .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| {
                        menu.item(
                            PopupMenuItem::new(delete_task.clone())
                                .icon(FluxIcon::Trash2)
                                .on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        let _ = view.update(cx, |this, cx| {
                                            this.execute_toolbar(ToolbarCommand::Delete, cx);
                                        });
                                    }
                                }),
                        )
                        .item(
                            PopupMenuItem::new(delete_with_files.clone())
                                .icon(FluxIcon::Trash2)
                                .on_click({
                                    let view = view.clone();
                                    move |_, window, cx| {
                                        let _ = view.update(cx, |this, cx| {
                                            this.delete_selected_with_files(window, cx);
                                        });
                                    }
                                }),
                        )
                    }),
            )
            .into_any_element()
    }

    /// 表头选择条（覆盖在表格容器顶部、选择列右侧）；无选中时返回 `None`。
    /// 覆盖期间列头的排序 / 拖宽被 `occlude` 拦下，全选框仍可用。
    pub(crate) fn render_selection_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let selection = self.table_state.read(cx).delegate().selection_summary();
        self.selection_summary.set(selection);
        // 详情面板已打开且只选中一项时，面板本身就在展示该任务，不再叠加选择条。
        let detail_shows_selection =
            selection.count == 1 && self.table_state.read(cx).delegate().prefs().detail_open;
        if selection.count == 0 || detail_shows_selection {
            return None;
        }

        let theme = active_theme(cx);
        let tokens = theme.tokens();
        let spacing = tokens.spacing;
        let surface = tokens.colors.surface;
        let foreground = tokens.colors.foreground;
        let text_size = tokens.typography.xs.size;
        let line_height = tokens.typography.xs.line_height;
        let hairline = theme.extended().colors.hairline;
        let stroke = theme.extended().stroke.thin;
        let icon_size = theme.extended().icon.md;
        let count_label = SharedString::from(
            self.translator
                .read(cx)
                .text_with("selectedCount", &[("n", &selection.count.to_string())]),
        );
        let clear_label =
            SharedString::from(self.translator.read(cx).text("deselectAll").to_owned());
        // 竖分隔与按钮同一垂直中线：高取 icon.lg（16），左右留白与按钮间距一致。
        let separator_height = theme.extended().icon.lg;
        let separator = move || {
            div()
                .flex_none()
                .w(stroke)
                .h(separator_height)
                .mx(spacing.xs)
                .bg(hairline)
        };

        let mut actions: Vec<AnyElement> = Vec::new();
        if selection.any_resumable {
            actions.push(self.icon_action(
                "download-selection-resume",
                self.strings.resume.clone(),
                Icon::new(FluxIcon::Play).size(icon_size),
                false,
                |this, _, cx| this.execute_toolbar(ToolbarCommand::Resume, cx),
                cx,
            ));
        }
        if selection.any_active {
            actions.push(self.icon_action(
                "download-selection-pause",
                self.strings.pause.clone(),
                Icon::new(FluxIcon::Pause).size(icon_size),
                false,
                |this, _, cx| this.execute_toolbar(ToolbarCommand::Pause, cx),
                cx,
            ));
        }
        if selection.any_local {
            actions.push(self.icon_action_with_state(
                "download-selection-open",
                self.strings.open_file.clone(),
                Icon::new(FluxIcon::ExternalLink).size(icon_size),
                false,
                !selection.all_openable,
                |this, _, cx| this.execute_toolbar(ToolbarCommand::Open, cx),
                cx,
            ));
            actions.push(self.icon_action(
                "download-selection-reveal",
                self.strings.open_folder.clone(),
                Icon::new(FluxIcon::FolderOpen).size(icon_size),
                false,
                |this, _, cx| this.execute_toolbar(ToolbarCommand::Reveal, cx),
                cx,
            ));
        }
        actions.push(self.selection_delete_menu(cx));
        let clear = self.icon_action(
            "download-selection-clear",
            clear_label,
            Icon::new(FluxIcon::X).size(icon_size),
            false,
            |this, _, cx| this.clear_table_selection(cx),
            cx,
        );

        Some(
            h_flex()
                .id("download-selection-bar")
                .occlude()
                .absolute()
                .top_0()
                .left(px(SELECTION_COLUMN_WIDTH))
                .right_0()
                // 留出表头底部分隔线。
                .h(px(TABLE_HEADER_HEIGHT) - stroke)
                .pl(spacing.sm)
                .gap(spacing.xxs)
                .items_center()
                .bg(surface)
                .child(
                    div()
                        .flex_none()
                        .whitespace_nowrap()
                        .text_size(text_size)
                        .line_height(line_height)
                        .font_features(tabular_numbers())
                        .text_color(foreground)
                        .child(count_label),
                )
                .child(separator())
                .children(actions)
                .child(separator())
                .child(clear)
                .into_any_element(),
        )
    }
}
