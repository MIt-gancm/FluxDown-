//! 队列管理窗口：单例辅助窗口，列表 + 表单编辑队列。

use std::sync::Arc;

use fluxdown_ui_downloads::QueueManagerView;
use fluxdown_ui_shell::{AuxiliaryWindowView, auxiliary_window_options};
use gpui::{App, AppContext as _, Bounds, WindowBounds, px, size};
use gpui_component::Root;

use crate::{
    app::Desktop,
    downloads_port::AgentDownloadsPort,
    session::attach,
    windows::{WindowKey, WindowRegistry},
};

const QUEUE_MANAGER_WINDOW_SIZE: gpui::Size<gpui::Pixels> = size(px(720.), px(560.));
const QUEUE_MANAGER_WINDOW_MIN_SIZE: gpui::Size<gpui::Pixels> = size(px(640.), px(480.));

/// 打开或聚焦队列管理窗口。
pub fn open(cx: &mut App) {
    let desktop = Desktop::global(cx);
    let translator = desktop.translator.clone();
    let session = desktop.session.clone();
    let client = desktop.client.clone();
    let title = translator.read(cx).text("manageQueueAction").to_owned();
    let mut options = auxiliary_window_options(title);
    options.window_bounds = Some(WindowBounds::Windowed(Bounds::centered(
        None,
        QUEUE_MANAGER_WINDOW_SIZE,
        cx,
    )));
    options.window_min_size = Some(QUEUE_MANAGER_WINDOW_MIN_SIZE);

    WindowRegistry::open_or_focus(cx, WindowKey::QueueManager, options, move |window, cx| {
        let port = Arc::new(AgentDownloadsPort::new(client.clone()));
        let queue_manager =
            cx.new(|cx| QueueManagerView::new(translator.clone(), port, window, cx));
        attach(&session, &queue_manager, cx);
        let window_view = cx.new(|cx| {
            AuxiliaryWindowView::new(translator, "manageQueueAction", queue_manager.into(), cx)
        });
        cx.new(|cx| Root::new(window_view, window, cx))
    });
}
