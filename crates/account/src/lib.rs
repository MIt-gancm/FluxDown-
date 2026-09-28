//! GPUI 账户、认证、设备、订单、配置同步与推介 capability。
//!
//! 页面结构与 `lib/src/pages/settings_page.dart` 的 `_AccountContent` 对齐：
//! 顶部居中的卡片列——未登录 hero / 已登录 profile、账号与安全、设备、云功能。

mod assets;
mod controller;
mod dialogs;
mod pages;
mod ui;
mod view;

use std::future::Future;
use std::pin::Pin;

use fluxdown_protocol::ApplicationErrorCode;
use fluxdown_ui_i18n::Translator;
use gpui::SharedString;

pub use assets::{AccountAssets, CLOUD_ICON_PATH, CROWN_ICON_PATH};
pub use controller::AccountController;
pub use view::AccountView;

pub type PortFuture<T> =
    Pin<Box<dyn Future<Output = Result<T, fluxdown_protocol::RpcErrorData>> + Send + 'static>>;

pub enum AccountCommand {
    Auth {
        method: &'static str,
        params: serde_json::Value,
    },
    Profile {
        method: &'static str,
        params: serde_json::Value,
    },
    Device {
        method: &'static str,
        params: serde_json::Value,
    },
    Plan {
        method: &'static str,
        params: serde_json::Value,
    },
    Order {
        method: &'static str,
        params: serde_json::Value,
    },
    Referral {
        method: &'static str,
        params: serde_json::Value,
    },
    /// 配置同步开关与状态（`agent.sync.*`）。
    Sync {
        method: &'static str,
        params: serde_json::Value,
    },
    /// FluxCloud 服务地址读取/覆盖（`agent.cloud.endpoint*`，仅调试构建可改）。
    CloudEndpoint {
        method: &'static str,
        params: serde_json::Value,
    },
}

pub trait AccountPort: Send + Sync {
    fn execute(&self, command: AccountCommand) -> PortFuture<serde_json::Value>;
}

/// 查表并转为可克隆的 `SharedString`；本 crate 内部统一走这层小转换。
pub(crate) fn t(translator: &Translator, key: &str) -> SharedString {
    SharedString::from(translator.text(key).to_owned())
}

/// 查表并做 `{name}` 占位插值。
pub(crate) fn t_with(
    translator: &Translator,
    key: &str,
    arguments: &[(&str, &str)],
) -> SharedString {
    SharedString::from(translator.text_with(key, arguments))
}

/// agent 端口只回传错误码（服务端 message 不透传），按码映射通用文案；
/// 与 `fluxdown_ui_extensions::error_text` 同一约定。
pub(crate) fn error_text(
    translator: &Translator,
    error: &fluxdown_protocol::RpcErrorData,
) -> SharedString {
    let key = match error.code {
        ApplicationErrorCode::Unavailable | ApplicationErrorCode::Timeout => {
            "localServiceDisconnected"
        }
        ApplicationErrorCode::InvalidArgument | ApplicationErrorCode::NotFound => {
            "localServiceInvalidArgument"
        }
        ApplicationErrorCode::Conflict => "localServiceConflict",
        ApplicationErrorCode::Unsupported => "settingsUnsupportedOnPlatform",
        ApplicationErrorCode::ProtocolIncompatible
        | ApplicationErrorCode::Unauthorized
        | ApplicationErrorCode::Cancelled
        | ApplicationErrorCode::Internal => "localServiceActionFailed",
    };
    t(translator, key)
}
