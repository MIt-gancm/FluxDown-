//! 入站局域网配对确认框：对端输入了本机配对码后，双方核对 SAS，本机确认或拒绝。
//!
//! 由 app 在收到 `AccountHostEvent::PairingRequested` 时于某个窗口弹出；请求被对端取消 /
//! 过期 / 已被其他窗口处理（从快照消失）时自动关闭。必须明确选择：不响应 Esc 与点击遮罩。

use fluxdown_protocol::{LinkApproveParams, LinkPairingRequestDto, method};
use fluxdown_ui_components::{ControlExt as _, field_error, field_hint};
use fluxdown_ui_theme::active_theme;
use gpui::{
    App, AppContext as _, ClickEvent, Context, Entity, FontWeight, IntoElement, ParentElement,
    Render, SharedString, Styled, Window, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    Disableable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};

use crate::errors::{ErrorContext, error_text};
use crate::host::AccountHost;
use crate::link::{group_sas, now_unix_ms, seconds_until};
use crate::verification::spawn_ticker;
use crate::{AccountCommand, t, t_with};

struct PairingPrompt {
    host: Entity<AccountHost>,
    request: LinkPairingRequestDto,
    remaining: u64,
    busy: bool,
    /// 已向 agent 提交过决定（含超时自动拒绝），不再重复提交。
    answered: bool,
    error: Option<SharedString>,
}

/// 为入站请求 `session_id` 打开确认框；请求已经不在快照里则什么也不做。
pub fn open(host: &Entity<AccountHost>, session_id: &str, window: &mut Window, cx: &mut App) {
    let Some(request) = host.read(cx).pairing_request(session_id) else {
        return;
    };
    let title = t(host.read(cx).translator().read(cx), "incomingPairingTitle");
    let view = cx.new(|cx| PairingPrompt::new(host.clone(), request, window, cx));
    window.open_dialog(cx, move |dialog, _, cx| {
        let view = view.clone();
        dialog
            .title(fluxdown_ui_components::dialog_title(title.clone(), cx))
            .w(px(480.))
            .close_button(false)
            .overlay_closable(false)
            .keyboard(false)
            .content(move |content, _, _| content.child(view.clone()))
    });
}

impl PairingPrompt {
    fn new(
        host: Entity<AccountHost>,
        request: LinkPairingRequestDto,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let remaining = seconds_until(request.expires_at_unix_ms, now_unix_ms());
        let session_id = request.session_id.clone();
        // 请求从快照消失（对端取消 / 过期 / 已处理）→ 关闭确认框。
        cx.observe_in(&host, window, move |this, host, window, cx| {
            if host.read(cx).pairing_request(&session_id).is_none() {
                this.answered = true;
                window.close_dialog(cx);
            }
        })
        .detach();
        spawn_ticker(window, cx, |this: &mut Self, window, cx| {
            this.remaining = seconds_until(this.request.expires_at_unix_ms, now_unix_ms());
            // 到期仍无人处理：自动拒绝（agent 侧也会过期，这里让请求方立刻得到结果）。
            if this.remaining == 0 {
                this.respond(false, window, cx);
            }
            cx.notify();
            !this.answered
        });
        Self {
            host,
            request,
            remaining,
            busy: false,
            answered: false,
            error: None,
        }
    }

    fn t(&self, key: &str, cx: &App) -> SharedString {
        t(self.host.read(cx).translator().read(cx), key)
    }

    fn respond(&mut self, accept: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.answered {
            return;
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        let params = serde_json::to_value(LinkApproveParams {
            session_id: self.request.session_id.clone(),
            accept,
        })
        .unwrap_or_default();
        let future = self.host.read(cx).port().execute(AccountCommand::Link {
            method: method::AGENT_LINK_APPROVE,
            params,
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = future.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(_) => {
                        this.answered = true;
                        window.close_dialog(cx);
                    }
                    Err(error) => {
                        this.error = Some(error_text(
                            this.host.read(cx).translator().read(cx),
                            &error,
                            ErrorContext::Pairing,
                        ));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for PairingPrompt {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = active_theme(cx).tokens().clone();
        let extended = active_theme(cx).extended().clone();
        let translator = self.host.read(cx).translator().read(cx).clone();
        let from = t_with(
            &translator,
            "incomingPairingFrom",
            &[("device", &self.request.peer_name)],
        );
        let countdown = t_with(
            &translator,
            "incomingPairingCountdown",
            &[("seconds", &self.remaining.to_string())],
        );
        let platform = self.request.peer_platform.clone();
        v_flex()
            .w_full()
            .gap(tokens.spacing.lg)
            .child(
                v_flex()
                    .gap(tokens.spacing.xxs)
                    .child(
                        div()
                            .text_size(tokens.typography.sm.size)
                            .line_height(tokens.typography.sm.line_height)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(tokens.colors.foreground)
                            .child(from),
                    )
                    .when_some(platform, |column, platform| {
                        column.child(field_hint(platform, cx))
                    })
                    .child(field_hint(self.t("incomingPairingHint", cx), cx)),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .justify_center()
                    .py(tokens.spacing.md)
                    .rounded(tokens.radius.md)
                    .bg(tokens.colors.muted)
                    .text_size(extended.title.size * 2.)
                    .line_height(extended.title.line_height * 2.)
                    .font_weight(FontWeight::SEMIBOLD)
                    .font_features(fluxdown_ui_components::tabular_numbers())
                    .text_color(tokens.colors.foreground)
                    .child(group_sas(&self.request.sas)),
            )
            .child(field_hint(countdown, cx))
            .when_some(self.error.clone(), |column, error| {
                column.child(field_error(error, cx))
            })
            .child(
                h_flex()
                    .w_full()
                    .pt(tokens.spacing.sm)
                    .justify_end()
                    .gap(tokens.spacing.sm)
                    .child(
                        Button::new("incoming-pairing-reject")
                            .outline()
                            .label(self.t("incomingPairingReject", cx))
                            .control(cx)
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.respond(false, window, cx);
                            })),
                    )
                    .child(
                        Button::new("incoming-pairing-accept")
                            .primary()
                            .label(self.t("incomingPairingAccept", cx))
                            .control(cx)
                            .loading(self.busy)
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.respond(true, window, cx);
                            })),
                    ),
            )
    }
}
