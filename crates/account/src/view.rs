//! 账户页顶层视图：装配 hero / profile / 账号与安全 / 设备 / 云功能卡片，
//! 以及调试构建独有的 FluxCloud 服务器地址卡片。

use std::sync::Arc;
use std::time::Duration;

use fluxdown_protocol::{AgentSnapshot, CloudEndpointDto, ServiceEvent, method};
use fluxdown_ui_i18n::Translator;
use fluxdown_ui_theme::active_theme;
use gpui::{
    AppContext as _, ClipboardItem, Context, Entity, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window, div, px,
};
use gpui_component::WindowExt as _;
use gpui_component::input::{InputEvent, InputState};
use gpui_component::notification::Notification;

use crate::controller::AccountController;
use crate::pages;
use crate::{AccountCommand, AccountPort, PortFuture};

/// 顶部居中卡片列的最大宽度（与 Flutter `_AccountContent` 的 760 对齐）。
const CONTENT_MAX_WIDTH: f32 = 760.;

/// 服务器地址设置只在调试构建出现（对齐 Flutter 的 `kDebugMode` 门控）；
/// 正式包既不显示也不向 agent 查询。
const SERVER_ADDRESS_VISIBLE: bool = cfg!(debug_assertions);

pub struct AccountView {
    translator: Entity<Translator>,
    pub(crate) controller: AccountController,
    last_error: Option<SharedString>,
    origin_id_copied: bool,
    /// agent 回报的 FluxCloud 地址；`None` 表示未加载或正式构建不查询。
    endpoint: Option<CloudEndpointDto>,
    endpoint_input: Entity<InputState>,
    /// 输入框上次同步到的地址；渲染期发现 `endpoint.base_url` 变化时回写输入框。
    endpoint_synced: SharedString,
    endpoint_busy: bool,
    /// profile 卡片「刷新云端信息」在途：按钮转菊花并拒绝重入。
    cloud_refreshing: bool,
    /// 设备卡片「重试」在途：同上，两者互不影响。
    devices_refreshing: bool,
    _endpoint_subscription: Subscription,
}

impl AccountView {
    pub fn new(
        translator: Entity<Translator>,
        port: Arc<dyn AccountPort>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&translator, |_, _, cx| cx.notify()).detach();
        let endpoint_input = cx.new(|cx| InputState::new(window, cx));
        let _endpoint_subscription =
            cx.subscribe_in(&endpoint_input, window, |this, input, event, window, cx| {
                if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    let value = input.read(cx).value().trim().to_owned();
                    this.commit_endpoint_input(value, window, cx);
                }
            });
        let mut this = Self {
            controller: AccountController::new(port),
            translator,
            last_error: None,
            origin_id_copied: false,
            endpoint: None,
            endpoint_input,
            endpoint_synced: SharedString::default(),
            endpoint_busy: false,
            cloud_refreshing: false,
            devices_refreshing: false,
            _endpoint_subscription,
        };
        this.load_endpoint(cx);
        this
    }

    pub fn replace_snapshot(&mut self, snapshot: &AgentSnapshot, cx: &mut Context<Self>) {
        self.last_error = None;
        self.controller.replace_snapshot(snapshot);
        self.load_endpoint(cx);
        cx.notify();
    }

    pub fn apply_event(&mut self, event: &ServiceEvent, cx: &mut Context<Self>) {
        self.controller.apply_event(event);
        cx.notify();
    }

    pub fn mark_stale(&mut self, cx: &mut Context<Self>) {
        self.last_error = Some(crate::t(
            self.translator.read(cx),
            "localServiceDisconnected",
        ));
        self.controller.mark_stale();
        cx.notify();
    }

    /// 通用「发起命令 → 失败提示」；成功结果由快照/事件驱动的重渲染呈现。
    pub(crate) fn spawn_action(
        &mut self,
        future: PortFuture<serde_json::Value>,
        cx: &mut Context<Self>,
    ) {
        self.last_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = future.await;
            let _ = this.update(cx, |this, cx| {
                if let Err(error) = result {
                    this.last_error = Some(crate::error_text(this.translator.read(cx), &error));
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 复制 Origin ID 到剪贴板；2 秒内胶囊文案切换为「已复制」。
    pub(crate) fn copy_origin_id(&mut self, origin_id: i64, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(origin_id.to_string()));
        self.origin_id_copied = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            let _ = this.update(cx, |this, cx| {
                this.origin_id_copied = false;
                cx.notify();
            });
        })
        .detach();
    }

    /// 手动刷新云端全量信息：资料/套餐/能力 + 设备名册（Flutter `_refreshCloudInfo`）。
    /// 在途期间按钮转菊花并拒绝重入；完成后以 toast 告知成功/失败，
    /// 数据本身经会话/设备事件回流重渲染。
    pub(crate) fn refresh_cloud_info(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.cloud_refreshing {
            return;
        }
        let port = self.controller.port();
        let profile = port.execute(AccountCommand::Auth {
            method: method::AGENT_AUTH_REFRESH_PROFILE,
            params: serde_json::json!({}),
        });
        let devices = port.execute(AccountCommand::Device {
            method: method::AGENT_DEVICE_LIST,
            params: serde_json::json!({}),
        });
        self.cloud_refreshing = true;
        self.last_error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = match profile.await {
                Ok(_) => devices.await,
                Err(error) => Err(error),
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.cloud_refreshing = false;
                this.notify_refresh_result(result.map(|_| ()), window, cx);
            });
        })
        .detach();
    }

    /// 设备卡片「重试」：只重拉受信任设备名册，同样带在途态与结果 toast。
    pub(crate) fn refresh_devices(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.devices_refreshing {
            return;
        }
        let future = self.controller.port().execute(AccountCommand::Device {
            method: method::AGENT_DEVICE_LIST,
            params: serde_json::json!({}),
        });
        self.devices_refreshing = true;
        self.last_error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = future.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.devices_refreshing = false;
                this.notify_refresh_result(result.map(|_| ()), window, cx);
            });
        })
        .detach();
    }

    /// 刷新类操作的统一结果反馈：成功 toast「云端信息已刷新」，失败 toast 错误文案
    /// 并同时落到卡片内错误行（错误需要可停留阅读）。
    fn notify_refresh_result(
        &mut self,
        result: Result<(), fluxdown_protocol::RpcErrorData>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let translator = self.translator.read(cx);
        match result {
            Ok(()) => window.push_notification(
                Notification::success(crate::t(translator, "accountCloudRefreshDone")),
                cx,
            ),
            Err(error) => {
                let message = crate::error_text(translator, &error);
                self.last_error = Some(message.clone());
                window.push_notification(Notification::error(message), cx);
            }
        }
        cx.notify();
    }

    /// 调试构建下向 agent 读取当前 FluxCloud 地址；正式构建直接跳过。
    fn load_endpoint(&mut self, cx: &mut Context<Self>) {
        if !SERVER_ADDRESS_VISIBLE {
            return;
        }
        let future = self
            .controller
            .port()
            .execute(AccountCommand::CloudEndpoint {
                method: method::AGENT_CLOUD_ENDPOINT_GET,
                params: serde_json::json!({}),
            });
        cx.spawn(async move |this, cx| {
            let Ok(value) = future.await else {
                return;
            };
            let Ok(endpoint) = serde_json::from_value::<CloudEndpointDto>(value) else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.endpoint = Some(endpoint);
                cx.notify();
            });
        })
        .detach();
    }

    /// 输入框失焦/回车：空值或未变化时回滚显示，否则提交覆盖。
    fn commit_endpoint_input(
        &mut self,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(endpoint) = &self.endpoint else {
            return;
        };
        if value.is_empty() || value == endpoint.base_url {
            let current = SharedString::from(endpoint.base_url.clone());
            self.endpoint_input
                .update(cx, |input, cx| input.set_value(current, window, cx));
            return;
        }
        self.set_endpoint(value, window, cx);
    }

    /// 提交 `agent.cloud.endpointSet`（空串 = 恢复默认）；成功/失败均以通知提示，
    /// 失败时把输入框回滚到当前生效地址。
    pub(crate) fn set_endpoint(
        &mut self,
        base_url: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.endpoint_busy {
            return;
        }
        self.endpoint_busy = true;
        cx.notify();
        let future = self
            .controller
            .port()
            .execute(AccountCommand::CloudEndpoint {
                method: method::AGENT_CLOUD_ENDPOINT_SET,
                params: serde_json::json!({ "baseUrl": base_url }),
            });
        cx.spawn_in(window, async move |this, cx| {
            let result = future.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.endpoint_busy = false;
                let translator = this.translator.read(cx);
                match result.and_then(|value| {
                    serde_json::from_value::<CloudEndpointDto>(value).map_err(|_| {
                        fluxdown_protocol::RpcErrorData::new(
                            fluxdown_protocol::ApplicationErrorCode::Internal,
                            false,
                        )
                    })
                }) {
                    Ok(endpoint) => {
                        window.push_notification(
                            Notification::success(crate::t(
                                translator,
                                "accountServerAddressSaved",
                            )),
                            cx,
                        );
                        this.endpoint = Some(endpoint);
                    }
                    Err(error) => {
                        let message = if error.code
                            == fluxdown_protocol::ApplicationErrorCode::InvalidArgument
                        {
                            crate::t(translator, "accountServerAddressInvalid")
                        } else {
                            crate::error_text(translator, &error)
                        };
                        window.push_notification(Notification::error(message), cx);
                        if let Some(endpoint) = &this.endpoint {
                            let current = SharedString::from(endpoint.base_url.clone());
                            this.endpoint_input
                                .update(cx, |input, cx| input.set_value(current, window, cx));
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 渲染前把 agent 回报的地址回写到输入框（仅在生效地址变化时）。
    fn sync_endpoint_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(endpoint) = &self.endpoint else {
            return;
        };
        if self.endpoint_synced.as_ref() == endpoint.base_url {
            return;
        }
        self.endpoint_synced = SharedString::from(endpoint.base_url.clone());
        let value = self.endpoint_synced.clone();
        self.endpoint_input
            .update(cx, |input, cx| input.set_value(value, window, cx));
    }
}

impl Render for AccountView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_endpoint_input(window, cx);
        let translator = self.translator.read(cx).clone();
        let tokens = active_theme(cx).tokens().clone();
        let disabled = self.controller.is_stale();
        let session = self.controller.session().cloned();
        let devices = self.controller.devices().to_vec();
        let sync = self.controller.sync_status().clone();
        let last_error = self.last_error.clone();
        let origin_id_copied = self.origin_id_copied;
        let port = self.controller.port();
        let server_address = self
            .endpoint
            .as_ref()
            .filter(|endpoint| SERVER_ADDRESS_VISIBLE && endpoint.editable)
            .cloned();

        let mut column = div()
            .flex()
            .flex_col()
            .w_full()
            .max_w(px(CONTENT_MAX_WIDTH))
            .gap(tokens.spacing.xl);

        column = match &session {
            None => column.child(pages::hero::render(
                &translator,
                &self.translator,
                &tokens,
                port.clone(),
                disabled,
                last_error,
                cx,
            )),
            Some(session) => column.child(pages::profile::render(
                &translator,
                &tokens,
                session,
                pages::profile::ProfileState {
                    copied: origin_id_copied,
                    disabled,
                    refreshing: self.cloud_refreshing,
                    last_error,
                },
                cx,
            )),
        };

        let logged_in = session.is_some();
        if let Some(session) = session {
            column = column
                .child(pages::security::render(&translator, &tokens, &session, cx))
                .child(pages::devices::render(
                    &translator,
                    &tokens,
                    &devices,
                    disabled,
                    self.devices_refreshing,
                    cx,
                ));
        }
        column = column.child(pages::cloud_features::render(
            &translator,
            &tokens,
            logged_in,
            &sync,
            &devices,
            disabled,
            cx,
        ));
        if let Some(endpoint) = server_address {
            column = column.child(pages::server_address::render(
                &translator,
                &tokens,
                &endpoint,
                &self.endpoint_input,
                disabled || self.endpoint_busy,
                cx,
            ));
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .child(column)
    }
}
