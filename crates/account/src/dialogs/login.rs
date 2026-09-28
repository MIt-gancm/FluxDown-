//! 登录对话框：账号 + 密码；设备验证码在需要时替换为独立的验证步骤，
//! 与 Flutter `_showLoginDialog` / 新设备验证屏幕语义对齐。

use std::sync::Arc;

use fluxdown_ui_components::{ControlExt as _, field_error, field_hint, form, form_field};
use fluxdown_ui_i18n::Translator;
use fluxdown_ui_theme::active_theme;
use gpui::{
    App, AppContext as _, ClickEvent, Context, Entity, FontWeight, IntoElement, ParentElement,
    Render, SharedString, Styled, Window, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    Disableable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputContentType, InputState},
    v_flex,
};

use crate::{AccountCommand, AccountPort, error_text, t};

pub(crate) struct LoginDialog {
    translator: Entity<Translator>,
    port: Arc<dyn AccountPort>,
    account_input: Entity<InputState>,
    password_input: Entity<InputState>,
    code_input: Entity<InputState>,
    verification_required: bool,
    busy: bool,
    error: Option<SharedString>,
}

/// 打开登录对话框；聚焦账号输入框。
pub(crate) fn open(
    translator: Entity<Translator>,
    port: Arc<dyn AccountPort>,
    window: &mut Window,
    cx: &mut App,
) {
    let title = t(translator.read(cx), "accountLoginDialogTitle");
    let view = cx.new(|cx| LoginDialog::new(translator, port, window, cx));
    let account_input = view.read(cx).account_input.clone();
    window.open_dialog(cx, move |dialog, _, cx| {
        let view = view.clone();
        dialog
            .title(fluxdown_ui_components::dialog_title(title.clone(), cx))
            .w(px(520.))
            .content(move |content, _, _| content.child(view.clone()))
    });
    account_input.update(cx, |input, cx| input.focus(window, cx));
}

impl LoginDialog {
    fn new(
        translator: Entity<Translator>,
        port: Arc<dyn AccountPort>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let account_placeholder = t(translator.read(cx), "accountLoginAccountPlaceholder");
        let password_placeholder = t(translator.read(cx), "accountPasswordPlaceholder");
        let code_placeholder = t(translator.read(cx), "accountCodePlaceholder");
        Self {
            translator,
            port,
            account_input: cx
                .new(|cx| InputState::new(window, cx).placeholder(account_placeholder)),
            password_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder(password_placeholder)
            }),
            code_input: cx.new(|cx| InputState::new(window, cx).placeholder(code_placeholder)),
            verification_required: false,
            busy: false,
            error: None,
        }
    }

    fn t(&self, key: &str, cx: &App) -> SharedString {
        t(self.translator.read(cx), key)
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let verify = self.verification_required;
        let account = self.account_input.read(cx).value().trim().to_owned();
        let password = self.password_input.read(cx).value().to_string();
        let code = self.code_input.read(cx).value().trim().to_owned();
        if account.is_empty() || password.is_empty() || (verify && code.is_empty()) {
            return;
        }
        let params = if verify {
            serde_json::json!({"account": account, "password": password, "code": code})
        } else {
            serde_json::json!({"account": account, "password": password})
        };
        let method = if verify {
            fluxdown_protocol::method::AGENT_AUTH_LOGIN_VERIFY
        } else {
            fluxdown_protocol::method::AGENT_AUTH_LOGIN
        };
        self.busy = true;
        self.error = None;
        cx.notify();
        let future = self.port.execute(AccountCommand::Auth { method, params });
        cx.spawn_in(window, async move |this, cx| {
            let result = future.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(value) => {
                        match serde_json::from_value::<fluxdown_protocol::AgentLoginResult>(value) {
                            Ok(fluxdown_protocol::AgentLoginResult::Ok { .. }) => {
                                window.close_dialog(cx);
                            }
                            Ok(
                                fluxdown_protocol::AgentLoginResult::DeviceVerificationRequired {
                                    ..
                                },
                            ) => {
                                this.verification_required = true;
                                this.code_input
                                    .update(cx, |input, cx| input.focus(window, cx));
                            }
                            Err(_) => {
                                this.error = Some(this.t("accountErrorUnknown", cx));
                            }
                        }
                    }
                    Err(error) => {
                        this.error = Some(error_text(this.translator.read(cx), &error));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let submit_label = if self.verification_required {
            self.t("confirm", cx)
        } else {
            self.t("accountLogin", cx)
        };
        // 底栏右对齐：取消(outline) 在左、主操作(primary) 在右，统一控件高度。
        h_flex()
            .w_full()
            .pt(active_theme(cx).tokens().spacing.sm)
            .justify_end()
            .gap(active_theme(cx).tokens().spacing.sm)
            .child(
                Button::new("account-login-cancel")
                    .outline()
                    .label(self.t("cancel", cx))
                    .control(cx)
                    .disabled(self.busy)
                    .on_click(|_, window, cx| window.close_dialog(cx)),
            )
            .child(
                Button::new("account-login-submit")
                    .primary()
                    .label(submit_label)
                    .control(cx)
                    .loading(self.busy)
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.submit(window, cx);
                    })),
            )
    }
}

impl Render for LoginDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = active_theme(cx).tokens().clone();
        let body = if self.verification_required {
            form(cx)
                .child(
                    v_flex()
                        .gap(tokens.spacing.xxs)
                        .child(
                            div()
                                .text_size(tokens.typography.sm.size)
                                .line_height(tokens.typography.sm.line_height)
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(tokens.colors.foreground)
                                .child(self.t("accountDeviceVerifyTitle", cx)),
                        )
                        .child(field_hint(
                            self.t("accountDeviceVerifySubtitleGeneric", cx),
                            cx,
                        )),
                )
                .child(form_field(
                    self.t("accountFieldCode", cx),
                    Input::new(&self.code_input).control(cx).w_full(),
                    None,
                    cx,
                ))
        } else {
            form(cx)
                .child(form_field(
                    self.t("accountFieldAccount", cx),
                    Input::new(&self.account_input).control(cx).w_full(),
                    None,
                    cx,
                ))
                .child(form_field(
                    self.t("accountPasswordPlaceholder", cx),
                    Input::new(&self.password_input)
                        .control(cx)
                        .w_full()
                        .content_type(InputContentType::Password)
                        .mask_toggle(),
                    None,
                    cx,
                ))
        };
        v_flex()
            .w_full()
            .gap(tokens.spacing.lg)
            .child(body)
            .when_some(self.error.clone(), |column, error| {
                column.child(field_error(error, cx))
            })
            .child(self.render_footer(cx))
    }
}
