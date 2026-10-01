//! 插件平台登录对话框：一次登录后凭据由引擎保存，后续插件请求自动复用。

use std::sync::Arc;
use std::time::Duration;

use fluxdown_protocol::{PluginAuthResponse, RpcErrorData};
use fluxdown_ui_components::{
    ControlExt as _, FluxIcon, dialog_scroll_body, field_error, form, form_field,
};
use fluxdown_ui_i18n::Translator;
use fluxdown_ui_theme::active_theme;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AppContext as _, ClipboardItem, Context, Entity, Image, ImageFormat, IntoElement,
    ParentElement, Render, SharedString, Styled, Subscription, Window, div, img, px,
};
use gpui_component::{
    Disableable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    v_flex,
};

use crate::controller::plugin_auth_call;
use crate::{ExtensionsPort, error_text, pages::Frame, ui};

pub struct PluginAuthDialog {
    translator: Entity<Translator>,
    port: Arc<dyn ExtensionsPort>,
    identity: String,
    site: Entity<InputState>,
    _site_subscription: Subscription,
    input: Entity<InputState>,
    session_id: String,
    auth_ref: String,
    status: String,
    challenge: Option<String>,
    challenge_type: Option<String>,
    message: Option<String>,
    busy: bool,
    poll_task_active: bool,
    logout_pending: bool,
}

impl PluginAuthDialog {
    pub fn new(
        translator: Entity<Translator>,
        port: Arc<dyn ExtensionsPort>,
        identity: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let site_placeholder = translator
            .read(cx)
            .text("pluginAuthSitePlaceholder")
            .to_owned();
        let input_placeholder = translator
            .read(cx)
            .text("pluginAuthInputPlaceholder")
            .to_owned();
        let site = cx.new(|cx| InputState::new(window, cx).placeholder(site_placeholder));
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(input_placeholder));
        let _site_subscription =
            cx.subscribe_in(&site, window, |this, input, event, window, cx| {
                if matches!(
                    event,
                    gpui_component::input::InputEvent::Blur
                        | gpui_component::input::InputEvent::PressEnter { .. }
                ) && !this.busy
                {
                    this.request_status(&input.read(cx).value(), window, cx);
                }
            });
        let dialog = Self {
            translator,
            port,
            identity,
            site,
            _site_subscription,
            input,
            session_id: String::new(),
            auth_ref: String::new(),
            status: String::new(),
            challenge: None,
            challenge_type: None,
            message: None,
            busy: true,
            poll_task_active: false,
            logout_pending: false,
        };
        let status_future =
            plugin_auth_call(&dialog.port, &dialog.identity, "status", "", "", "", "");
        cx.spawn_in(window, async move |this, cx| {
            let result = status_future.await;

            let Ok(()) = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                this.apply_result(result, false, window, cx);
                cx.notify();
            }) else {
                // 对话框或窗口已释放，停止回写异步结果。
                return;
            };
        })
        .detach();
        dialog
    }

    fn request_status(&mut self, site: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.busy = true;
        self.message = None;
        let future = plugin_auth_call(&self.port, &self.identity, "status", site, "", "", "");
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = future.await;

            let Ok(()) = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                this.apply_result(result, false, window, cx);
                cx.notify();
            }) else {
                // 对话框或窗口已释放，停止回写异步结果。
                return;
            };
        })
        .detach();
    }

    fn call(&mut self, action: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.message = None;
        let identity = self.identity.clone();
        let site = self.site.read(cx).value().to_string();
        let input = self.input.read(cx).value().to_string();
        let session_id = self.session_id.clone();
        let notify_success = matches!(action, "begin" | "poll");
        let future = plugin_auth_call(
            &self.port,
            &identity,
            action,
            &site,
            &self.auth_ref,
            &session_id,
            &input,
        );
        self.logout_pending = action == "logout";
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = future.await;

            let Ok(()) = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                this.apply_result(result, notify_success, window, cx);
                cx.notify();
            }) else {
                // 对话框或窗口已释放，停止回写异步结果。
                return;
            };
        })
        .detach();
    }

    /// 向引擎取消当前登录会话；无会话或忙碌时是 no-op。取消按钮与对话框被
    /// X / Esc / 点击遮罩关闭（不经过按钮，见 `open_plugin_auth` 里
    /// `Dialog::on_cancel` 的钩子）两条路径共用，保证任何关闭方式都不会
    /// 在引擎侧留下悬空的登录会话。
    pub(crate) fn cancel_session(&self, cx: &mut Context<Self>) {
        // 不看 `busy`：自动轮询每 2s 置一次 busy，若此时用户关闭对话框，
        // 仍必须通知引擎释放会话；cancel 是 fire-and-forget，与在途 poll 并存无害。
        if self.session_id.is_empty() {
            return;
        }
        let identity = self.identity.clone();
        let site = self.site.read(cx).value().to_string();
        let session_id = self.session_id.clone();
        let auth_ref = self.auth_ref.clone();
        let future = plugin_auth_call(
            &self.port,
            &identity,
            "cancel",
            &site,
            &auth_ref,
            &session_id,
            "",
        );
        cx.spawn(async move |_this, _cx| {
            if let Err(error) = future.await {
                // 对话框可能已关闭；取消会话失败只留诊断，不回写已释放的 UI。
                eprintln!(
                    "plugin authentication cancellation failed: {:?} ({:?})",
                    error.code, error.reason
                );
            }
        })
        .detach();
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_session(cx);
        window.close_dialog(cx);
    }

    fn apply_response(
        &mut self,
        response: PluginAuthResponse,
        notify_success: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // 无条件先取出该标志：不管这次响应的 `status` 是什么，都不能把它带到
        // 下一次非 logout 的响应里误清凭据（logout 返回 error 时尤其如此）。
        let was_logout = std::mem::take(&mut self.logout_pending);
        self.status = response.status.clone();
        self.session_id = response.session_id;
        self.auth_ref = response.auth_ref.unwrap_or_default();
        if self.status == "pending" {
            // pending 回包的 challenge/challengeType 均为可选字段：插件在同一
            // 会话的后续 poll 里可以不重复下发，缺失时保留上一帧，避免二维码
            // 或提示内容在轮询途中被空响应中途抹掉。
            if response.challenge.is_some() {
                self.challenge = response.challenge;
            }
            if response.challenge_type.is_some() {
                self.challenge_type = response.challenge_type;
            }
        } else {
            self.challenge = response.challenge;
            self.challenge_type = response.challenge_type;
        }
        self.message = (!response.message.is_empty()).then_some(response.message);
        if was_logout {
            // logout 无论成功失败，引擎都已无条件删除本地档案；客户端同步清空
            // 登录态，不依赖插件回包内容——`status` 在 logout 语境下表示“注销
            // 请求本身是否成功”，不代表“是否仍处于登录状态”。渲染侧改用
            // `auth_ref` 非空联合判定是否已登录，清空后自然回落到「开始登录」。
            self.auth_ref.clear();
            self.session_id.clear();
            self.challenge = None;
            self.challenge_type = None;
        }
        if self.status == "pending"
            && !self.session_id.is_empty()
            && self
                .challenge_type
                .as_deref()
                .is_some_and(|kind| kind.eq_ignore_ascii_case("qrcode"))
        {
            self.start_polling(window, cx);
        }
        if notify_success && response.status == "success" {
            window.push_notification(
                gpui_component::notification::Notification::success(
                    self.translator
                        .read(cx)
                        .text("pluginAuthSuccess")
                        .to_owned(),
                ),
                cx,
            );
        }
    }

    /// QR 登录每两秒轮询一次；实体销毁后 `update` 失败，任务自然退出。
    fn start_polling(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.poll_task_active {
            return;
        }
        self.poll_task_active = true;
        let port = self.port.clone();
        let identity = self.identity.clone();
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                let Ok(Some(future)) = this.update(cx, |this, cx| {
                    if this.busy
                        || this.session_id.is_empty()
                        || !this
                            .challenge_type
                            .as_deref()
                            .is_some_and(|kind| kind.eq_ignore_ascii_case("qrcode"))
                    {
                        this.poll_task_active = false;
                        return None;
                    }
                    this.busy = true;
                    cx.notify();
                    Some(plugin_auth_call(
                        &port,
                        &identity,
                        "poll",
                        this.site.read(cx).value().as_ref(),
                        &this.auth_ref,
                        &this.session_id,
                        this.input.read(cx).value().as_ref(),
                    ))
                }) else {
                    break;
                };
                let result = future.await;
                let Ok(()) = this.update_in(cx, |this, window, cx| {
                    this.busy = false;
                    this.apply_result(result, true, window, cx);
                    cx.notify();
                }) else {
                    break;
                };
            }
        })
        .detach();
    }

    fn apply_result(
        &mut self,
        result: Result<serde_json::Value, RpcErrorData>,
        notify_success: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(value) => match serde_json::from_value::<PluginAuthResponse>(value) {
                Ok(response) => self.apply_response(response, notify_success, window, cx),
                Err(_) => {
                    self.message = Some(
                        self.translator
                            .read(cx)
                            .text("pluginAuthInvalidResponse")
                            .to_owned(),
                    )
                }
            },
            Err(error) => {
                let text = error_text(self.translator.read(cx), &error);
                self.message = Some(
                    self.translator
                        .read(cx)
                        .text_with("pluginAuthFailed", &[("message", &text)]),
                );
            }
        }
    }

    /// 复制挑战原文到剪贴板；展示侧的截断（见 [`truncate_challenge_text`]）
    /// 不影响复制的完整内容。
    fn copy_challenge(
        &self,
        value: String,
        copied_label: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.write_to_clipboard(ClipboardItem::new_string(value));
        window.push_notification(
            gpui_component::notification::Notification::success(copied_label),
            cx,
        );
    }
}

impl Render for PluginAuthDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = active_theme(cx);
        let translator = self.translator.read(cx);
        let frame = Frame {
            translator,
            tokens: theme.tokens(),
            extended: theme.extended(),
            components: theme.components(),
            stale: false,
        };
        let tokens = frame.tokens;
        let begin = translator.text("pluginAuthBegin").to_owned();
        let site_label = SharedString::from(translator.text("pluginAuthSiteLabel").to_owned());
        let input_label = SharedString::from(translator.text("pluginAuthInputLabel").to_owned());
        let poll = translator.text("pluginAuthPoll").to_owned();
        let cancel = translator.text("cancel").to_owned();
        let logout_label = translator.text("pluginAuthLogout").to_owned();
        let copy_label = translator.text("apiServiceCopy").to_owned();
        let copied_label = translator.text("apiServiceCopied").to_owned();
        // `status` 在不同 action 下语义不同（logout 的 "success" 是“注销成功”而
        // 非“已登录”），故是否已登录额外联合 `auth_ref` 非空判定；logout 完成
        // 后 `apply_response` 已清空 `auth_ref`，这里自然回落到「开始登录」。
        let is_logged_in = self.status == "success" && !self.auth_ref.is_empty();
        let is_qrcode = self
            .challenge_type
            .as_deref()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("qrcode"));
        let session_pending = self.status == "pending" && !self.session_id.is_empty();
        // 状态色：已登录 success、进行中 primary；其余不显示状态行。
        let status = if is_logged_in {
            Some((
                FluxIcon::CircleCheck,
                frame.extended.colors.success,
                translator.text("pluginAuthSuccess").to_owned(),
            ))
        } else if session_pending {
            Some((
                FluxIcon::Clock,
                tokens.colors.primary,
                translator.text("pluginAuthPending").to_owned(),
            ))
        } else {
            None
        };
        let challenge = self.challenge.clone();
        let challenge_type_label = self.challenge_type.clone().unwrap_or_default();
        let body = v_flex()
            .w_full()
            .gap(tokens.spacing.lg)
            .child(
                form(cx)
                    .child(ui::meta_text(
                        translator.text("pluginAuthDescription").to_owned(),
                        frame,
                    ))
                    .child(form_field(
                        site_label,
                        Input::new(&self.site)
                            .control(cx)
                            .w_full()
                            .disabled(self.busy),
                        None,
                        cx,
                    ))
                    .child(form_field(
                        input_label,
                        Input::new(&self.input)
                            .control(cx)
                            .w_full()
                            .disabled(self.busy),
                        None,
                        cx,
                    )),
            )
            .when_some(status, |this, (icon, color, text)| {
                this.child(
                    ui::status_line(icon, color, text, frame)
                        .text_size(tokens.typography.sm.size)
                        .text_color(tokens.colors.foreground),
                )
            })
            .when_some(challenge, |this, value| {
                // 插件挑战不可信：`data:image/` 载荷尝试直接解码渲染成图片
                // （避免整段 base64 糊在对话框里）；解码失败或非图片挑战一律
                // 退化成截断文本 + 复制按钮，保证原文始终可以取出。
                let image = decode_data_image_challenge(&value);
                let fallback_text = image.is_none().then(|| truncate_challenge_text(&value));
                this.child(
                    v_flex()
                        .gap(tokens.spacing.xs)
                        .p(tokens.spacing.sm)
                        .rounded(tokens.radius.md)
                        .bg(tokens.colors.muted)
                        .child(ui::meta_text(challenge_type_label, frame))
                        .when_some(image, |this, image| {
                            this.child(
                                div()
                                    .w_full()
                                    .flex()
                                    .justify_center()
                                    .child(img(Arc::new(image)).size(px(200.))),
                            )
                        })
                        .when_some(fallback_text, |this, text| {
                            let copy_value = value.clone();
                            this.child(
                                ui::meta_text(text, frame).text_color(tokens.colors.foreground),
                            )
                            .child(
                                Button::new("plugin-auth-copy-challenge")
                                    .outline()
                                    .control(cx)
                                    .icon(FluxIcon::Copy)
                                    .label(copy_label)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.copy_challenge(
                                            copy_value.clone(),
                                            copied_label.clone(),
                                            window,
                                            cx,
                                        );
                                    })),
                            )
                        }),
                )
            });
        // 表单与挑战（二维码）可能高过窗口：正文滚动，错误行与底栏常驻可见。
        v_flex()
            .w_full()
            .min_h_0()
            .gap(tokens.spacing.lg)
            .child(dialog_scroll_body("plugin-auth-body", None, body, cx))
            .when_some(self.message.clone(), |this, message| {
                this.child(field_error(message, cx))
            })
            .child(
                // 底栏右对齐：取消(outline) 在左、主操作在右，统一控件高度。
                h_flex()
                    .w_full()
                    .pt(tokens.spacing.sm)
                    .justify_end()
                    .gap(tokens.spacing.sm)
                    .child(
                        Button::new("plugin-auth-cancel")
                            .outline()
                            .control(cx)
                            .label(cancel)
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, window, cx| this.cancel(window, cx))),
                    )
                    .child(
                        div()
                            .when(is_logged_in, |this| {
                                this.child(
                                    Button::new("plugin-auth-logout")
                                        .outline()
                                        .control(cx)
                                        .label(logout_label)
                                        .loading(self.busy)
                                        .disabled(self.busy)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.call("logout", window, cx);
                                        })),
                                )
                            })
                            .when(!is_logged_in && session_pending && !is_qrcode, |this| {
                                // 非二维码挑战没有自动轮询，提供手动「检查状态」。
                                this.child(
                                    Button::new("plugin-auth-poll")
                                        .primary()
                                        .control(cx)
                                        .label(poll)
                                        .loading(self.busy)
                                        .disabled(self.busy)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.call("poll", window, cx);
                                        })),
                                )
                            })
                            .when(!is_logged_in && !session_pending, |this| {
                                // 凡非 success（已登录）/ pending（会话进行中）
                                // 均视为未登录，统一显示「开始登录」。
                                this.child(
                                    Button::new("plugin-auth-begin")
                                        .primary()
                                        .control(cx)
                                        .label(begin)
                                        .loading(self.busy)
                                        .disabled(self.busy)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.call("begin", window, cx);
                                        })),
                                )
                            }),
                    ),
            )
    }
}

/// 单次渲染中，挑战文本超过该长度即截断显示（复制按钮始终给出完整原文）；
/// 插件返回的内容不可信，避免超长字符串拖垮布局或卡顿渲染。
const CHALLENGE_TEXT_LIMIT: usize = 512;

/// `data:image/...;base64,` 挑战超过该长度直接放弃图片解码，退化为文本 +
/// 复制按钮；常规二维码远小于此值，超限视为异常载荷。
const MAX_CHALLENGE_DATA_URL_LEN: usize = 256 * 1024;

/// 把 `data:image/<mime>[;charset=..];base64,<payload>` 形式的挑战解析成可
/// 直接交给 `img()` 渲染的 [`Image`]；非 `data:` 前缀、非 base64 编码、未知
/// 格式或超限一律返回 `None`——调用侧退回纯文本 + 复制按钮，不代表挑战本身
/// 无效，只是本端选择不预览。
fn decode_data_image_challenge(value: &str) -> Option<Image> {
    if value.len() > MAX_CHALLENGE_DATA_URL_LEN {
        return None;
    }
    let rest = value.strip_prefix("data:")?;
    let (meta, payload) = rest.split_once(',')?;
    let mut segments = meta.split(';');
    let mime = segments.next()?.to_ascii_lowercase();
    if !segments.any(|segment| segment.eq_ignore_ascii_case("base64")) {
        return None;
    }
    let format = ImageFormat::from_mime_type(&mime)?;
    let bytes = base64_decode(payload)?;
    if bytes.is_empty() {
        return None;
    }
    Some(Image::from_bytes(format, bytes))
}

/// 极简标准字母表 base64 解码（RFC 4648），仅用于上面的 data URL 挑战预览；
/// 沿用 `native/engine/src/proxy_config.rs::base64_encode` 的既有做法，为
/// 单处用途手写而不是为此新增 `base64` 依赖。
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    fn sextet(byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let cleaned: Vec<u8> = input
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    let end = cleaned
        .iter()
        .rposition(|&byte| byte != b'=')
        .map_or(0, |index| index + 1);
    let data = &cleaned[..end];
    if data.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(data.len() * 3 / 4 + 3);
    for chunk in data.chunks(4) {
        let mut buf = [0u8; 4];
        for (slot, &byte) in buf.iter_mut().zip(chunk) {
            *slot = sextet(byte)?;
        }
        out.push((buf[0] << 2) | (buf[1] >> 4));
        if chunk.len() > 2 {
            out.push((buf[1] << 4) | (buf[2] >> 2));
        }
        if chunk.len() > 3 {
            out.push((buf[2] << 6) | buf[3]);
        }
    }
    Some(out)
}

/// 挑战文本超过 [`CHALLENGE_TEXT_LIMIT`] 字符时截断并追加省略号；复制按钮
/// 始终复制未截断的原文。
fn truncate_challenge_text(value: &str) -> String {
    if value.chars().count() <= CHALLENGE_TEXT_LIMIT {
        return value.to_string();
    }
    let mut truncated: String = value.chars().take(CHALLENGE_TEXT_LIMIT).collect();
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::{base64_decode, decode_data_image_challenge, truncate_challenge_text};

    #[test]
    fn base64_decode_round_trips_known_vectors() {
        assert_eq!(base64_decode("aGVsbG8=").as_deref(), Some(&b"hello"[..]));
        assert_eq!(base64_decode("aGVsbG8").as_deref(), Some(&b"hello"[..]));
        assert_eq!(base64_decode("YQ==").as_deref(), Some(&b"a"[..]));
        assert_eq!(base64_decode("").as_deref(), Some(&b""[..]));
    }

    #[test]
    fn base64_decode_rejects_invalid_alphabet() {
        assert_eq!(base64_decode("not base64!"), None);
    }

    #[test]
    fn decode_data_image_challenge_accepts_known_mime() {
        let image = decode_data_image_challenge("data:image/png;base64,aGVsbG8=")
            .expect("valid png data url should decode");
        assert_eq!(image.format(), gpui::ImageFormat::Png);
        assert_eq!(image.bytes(), b"hello");
    }

    #[test]
    fn decode_data_image_challenge_rejects_non_image_text() {
        assert!(decode_data_image_challenge("plain-text-challenge").is_none());
        assert!(decode_data_image_challenge("https://example.com/login").is_none());
    }

    #[test]
    fn decode_data_image_challenge_rejects_unknown_format_and_non_base64() {
        assert!(decode_data_image_challenge("data:image/unknown;base64,aGVsbG8=").is_none());
        assert!(decode_data_image_challenge("data:image/png,aGVsbG8=").is_none());
    }

    #[test]
    fn truncate_challenge_text_keeps_short_text_unchanged() {
        assert_eq!(truncate_challenge_text("short"), "short");
    }

    #[test]
    fn truncate_challenge_text_truncates_long_text_on_char_boundaries() {
        let long = "码".repeat(600);
        let truncated = truncate_challenge_text(&long);
        assert_eq!(truncated.chars().count(), super::CHALLENGE_TEXT_LIMIT + 1);
        assert!(truncated.ends_with('…'));
    }
}
