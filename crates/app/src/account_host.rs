//! 账户状态的全局宿主：一个 [`AccountHost`] 订阅 agent 会话，供设置窗口的账户页、
//! 「添加设备」对话框共享；这里把它的一次性提示落到窗口上——
//! 入站局域网配对请求弹确认框，agent 的 `SessionRevoked` 给一次性通知。

use std::{collections::HashSet, sync::Arc};

use fluxdown_protocol::ErrorReason;
use fluxdown_ui_account::{AccountHost, AccountHostEvent, open_add_device, open_pairing_prompt};
use fluxdown_ui_i18n::Translator;
use gpui::{App, AppContext as _, Entity, Global, Window};
use gpui_component::{WindowExt as _, notification::Notification};

use crate::{
    account_port::AgentAccountPort,
    agent_client::AgentClient,
    app::Desktop,
    session::{AgentSession, attach},
    windows::{WindowRegistry, bring_to_front},
};

/// 已经弹出过确认框的入站请求（`session_id`），避免重连快照重复弹窗。
#[derive(Default)]
struct PromptedRequests(HashSet<String>);

impl Global for PromptedRequests {}

/// 创建并装配账户宿主：订阅会话，登记提示落点。
pub(crate) fn install(
    translator: &Entity<Translator>,
    session: &Entity<AgentSession>,
    client: &Arc<AgentClient>,
    cx: &mut App,
) -> Entity<AccountHost> {
    let port = Arc::new(AgentAccountPort::new(client.clone()));
    let host = cx.new(|cx| AccountHost::new(translator.clone(), port, cx));
    cx.set_global(PromptedRequests::default());
    cx.subscribe(&host, |host, event, cx| match event {
        AccountHostEvent::PairingRequested(id) => show_pairing_prompt(&host, id, cx),
        AccountHostEvent::SessionRevoked(reason) => notify_session_revoked(&host, *reason, cx),
        AccountHostEvent::SnapshotReplaced => {}
    })
    .detach();
    attach(session, &host, cx);
    host
}

/// 主窗口就绪后补弹窗口尚不存在时被跳过的提示：入站配对请求与会话撤销通知（各一次）。
pub(crate) fn replay_pending(cx: &mut App) {
    let host = Desktop::global(cx).account_host.clone();
    for id in host.read(cx).pending_pairing_requests() {
        show_pairing_prompt(&host, &id, cx);
    }
    if let Some(reason) = host.read(cx).pending_revocation() {
        notify_session_revoked(&host, reason, cx);
    }
}

fn show_pairing_prompt(host: &Entity<AccountHost>, id: &str, cx: &mut App) {
    let pending = host.read(cx).pending_pairing_requests();
    let prompted = cx.global_mut::<PromptedRequests>();
    prompted.0.retain(|shown| pending.contains(shown));
    if prompted.0.contains(id) {
        return;
    }
    let Some(handle) = WindowRegistry::overlay_window(cx) else {
        return;
    };
    let host = host.clone();
    let id = id.to_owned();
    let shown = handle
        .update(cx, |_, window, cx| {
            bring_to_front(window, cx);
            open_pairing_prompt(&host, &id, window, cx);
        })
        .is_ok();
    if shown {
        cx.global_mut::<PromptedRequests>().0.insert(id);
    }
}

/// 会话被撤销的一次性通知；没有承载窗口时原因留在宿主里，等主窗口打开后补弹。
fn notify_session_revoked(host: &Entity<AccountHost>, reason: ErrorReason, cx: &mut App) {
    let Some(handle) = WindowRegistry::overlay_window(cx) else {
        return;
    };
    let message = host.read(cx).session_revoked_message(reason, cx);
    let shown = handle
        .update(cx, |_, window, cx| {
            window.push_notification(Notification::warning(message), cx);
        })
        .is_ok();
    if shown {
        host.update(cx, |host, _| {
            host.take_pending_revocation();
        });
    }
}

/// 侧栏「添加设备」入口：在触发它的窗口里打开对话框。
pub(crate) fn open_add_device_dialog(window: &mut Window, cx: &mut App) {
    let host = Desktop::global(cx).account_host.clone();
    open_add_device(&host, window, cx);
}
