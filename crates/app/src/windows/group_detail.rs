//! 任务组详情窗口：FluxDown 辅助窗口标题栏；自建私有 `DownloadsController`，经 [`attach`] 订阅会话。

use std::sync::Arc;

use fluxdown_ui_downloads::GroupDetailView;
use fluxdown_ui_shell::{AuxiliaryWindowView, auxiliary_window_options};
use gpui::{App, AppContext as _, Bounds, WindowBounds, px, size};
use gpui_component::Root;

use crate::{
    app::Desktop,
    downloads_port::AgentDownloadsPort,
    session::attach,
    windows::{WindowKey, WindowRegistry},
};

const GROUP_WINDOW_SIZE: gpui::Size<gpui::Pixels> = size(px(640.), px(480.));
const GROUP_WINDOW_MIN_SIZE: gpui::Size<gpui::Pixels> = size(px(480.), px(320.));

/// 打开或聚焦任务组详情窗口。
pub fn open(cx: &mut App, group_id: String) {
    let desktop = Desktop::global(cx);
    let translator = desktop.translator.clone();
    let session = desktop.session.clone();
    let client = desktop.client.clone();
    let placeholder_title = translator.read(cx).text("groupDetailMembersTab").to_owned();

    let mut options = auxiliary_window_options(placeholder_title);
    options.window_min_size = Some(GROUP_WINDOW_MIN_SIZE);
    options.window_bounds = Some(WindowBounds::Windowed(Bounds::centered(
        None,
        GROUP_WINDOW_SIZE,
        cx,
    )));

    let key = WindowKey::GroupDetail(group_id.clone());
    WindowRegistry::open_or_focus(cx, key, options, move |window, cx| {
        let downloads_port = Arc::new(AgentDownloadsPort::new(client.clone()));
        let detail = cx.new(|cx| {
            GroupDetailView::new(
                translator.clone(),
                group_id.clone(),
                downloads_port,
                window,
                cx,
            )
        });
        attach(&session, &detail, cx);
        let window_view = cx.new(|cx| {
            AuxiliaryWindowView::new(translator, "groupDetailMembersTab", detail.into(), cx)
        });
        cx.new(|cx| Root::new(window_view, window, cx))
    });
}
