//! 注册对话框：邮箱 + 密码 + 可选昵称；完成后转入邮箱验证码步骤，
//! 与 Flutter `_showRegisterDialog` / 注册验证屏幕语义对齐。

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

use crate::{AccountCommand, AccountPort, error_text, t, t_with};

pub(crate) struct RegisterDialog {
    translator: Entity<Translator>,
    port: Arc<dyn AccountPort>,
    email_input: Entity<InputState>,
    password_input: Entity<InputState>,
    nickname_input: Entity<InputState>,
    code_input: Entity<InputState>,
    verification_required: bool,
    busy: bool,
    error: Option<SharedString>,
}

/// 打开注册对话框；聚焦邮箱输入框。
pub(crate) fn open(
    translator: Entity<Translator>,
    port: Arc<dyn AccountPort>,
    window: &mut Window,
    cx: &mut App,
) {
    let title = t(translator.read(cx), "accountRegisterDialogTitle");
    let view = cx.new(|cx| RegisterDialog::new(translator, port, window, cx));
    let email_input = view.read(cx).email_input.clone();
    window.open_dialog(cx, move |dialog, _, cx| {
        let view = view.clone();
        dialog
            .title(fluxdown_ui_components::dialog_title(title.clone(), cx))
            .w(px(520.))
            .content(move |content, _, _| content.child(view.clone()))
    });
    email_input.update(cx, |input, cx| input.focus(window, cx));
}

impl RegisterDialog {
    fn new(
        translator: Entity<Translator>,
        port: Arc<dyn AccountPort>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let email_placeholder = t(translator.read(cx), "accountEmailPlaceholder");
        let password_placeholder = t(translator.read(cx), "accountPasswordHint");
        let nickname_placeholder = t(translator.read(cx), "accountNicknamePlaceholder");
        let code_placeholder = t(translator.read(cx), "accountCodePlaceholder");
        Self {
            translator,
            port,
            email_input: cx.new(|cx| InputState::new(window, cx).placeholder(email_placeholder)),
            password_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder(password_placeholder)
            }),
            nickname_input: cx
                .new(|cx| InputState::new(window, cx).placeholder(nickname_placeholder)),
            code_input: cx.new(|cx| InputState::new(window, cx).placeholder(code_placeholder)),
            verification_required: false,
            busy: false,
            error: None,
        }
    }

    fn t(&self, key: &str, cx: &App) -> SharedString {
        t(self.translator.read(cx), key)
    }

    fn email(&self, cx: &App) -> String {
        self.email_input.read(cx).value().trim().to_owned()
    }

    fn submit_register(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let email = self.email(cx);
        let password = self.password_input.read(cx).value().to_string();
        let nickname = self.nickname_input.read(cx).value().trim().to_owned();
        if email.is_empty() || password.is_empty() {
            return;
        }
        let mut params = serde_json::json!({"email": email, "password": password});
        if !nickname.is_empty() {
            params["nickname"] = serde_json::Value::String(nickname);
        }
        self.run(
            fluxdown_protocol::method::AGENT_AUTH_REGISTER,
            params,
            window,
            cx,
        );
    }

    fn submit_verify(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let email = self.email(cx);
        let code = self.code_input.read(cx).value().trim().to_owned();
        if code.is_empty() {
            return;
        }
        let params = serde_json::json!({"email": email, "code": code});
        self.run(
            fluxdown_protocol::method::AGENT_AUTH_REGISTER_VERIFY,
            params,
            window,
            cx,
        );
    }

    fn run(
        &mut self,
        method: &'static str,
        params: serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
            self.t("accountVerifySubmit", cx)
        } else {
            self.t("accountRegister", cx)
        };
        // 底栏右对齐：取消(outline) 在左、主操作(primary) 在右，统一控件高度。
        h_flex()
            .w_full()
            .pt(active_theme(cx).tokens().spacing.sm)
            .justify_end()
            .gap(active_theme(cx).tokens().spacing.sm)
            .child(
                Button::new("account-register-cancel")
                    .outline()
                    .label(self.t("cancel", cx))
                    .control(cx)
                    .disabled(self.busy)
                    .on_click(|_, window, cx| window.close_dialog(cx)),
            )
            .child(
                Button::new("account-register-submit")
                    .primary()
                    .label(submit_label)
                    .control(cx)
                    .loading(self.busy)
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        if this.verification_required {
                            this.submit_verify(window, cx);
                        } else {
                            this.submit_register(window, cx);
                        }
                    })),
            )
    }
}

impl Render for RegisterDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = active_theme(cx).tokens().clone();
        let body = if self.verification_required {
            let subtitle = t_with(
                self.translator.read(cx),
                "accountRegisterVerifySubtitle",
                &[("email", &self.email(cx))],
            );
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
                                .child(self.t("accountRegisterVerifyTitle", cx)),
                        )
                        .child(field_hint(subtitle, cx)),
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
                    self.t("accountEmailPlaceholder", cx),
                    Input::new(&self.email_input).control(cx).w_full(),
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
                    Some(self.t("accountPasswordHint", cx)),
                    cx,
                ))
                .child(form_field(
                    self.t("accountFieldNickname", cx),
                    Input::new(&self.nickname_input).control(cx).w_full(),
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
