//! 稳定的本机服务应用错误契约。

use serde::{Deserialize, Serialize};

/// JSON-RPC 解析错误。
pub const PARSE_ERROR_CODE: i32 = -32700;
/// JSON-RPC 无效请求错误。
pub const INVALID_REQUEST_CODE: i32 = -32600;
/// JSON-RPC 方法不存在错误。
pub const METHOD_NOT_FOUND_CODE: i32 = -32601;
/// JSON-RPC 参数无效错误。
pub const INVALID_PARAMS_CODE: i32 = -32602;
/// JSON-RPC 内部错误。
pub const INTERNAL_ERROR_CODE: i32 = -32603;
/// FluxDown 应用错误使用的 JSON-RPC code。
pub const APPLICATION_ERROR_CODE: i32 = -32000;

/// 稳定应用错误码。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum ApplicationErrorCode {
    ProtocolIncompatible,
    Unauthorized,
    InvalidArgument,
    NotFound,
    Conflict,
    Unavailable,
    Timeout,
    Cancelled,
    Unsupported,
    Internal,
}

/// 比 [`ApplicationErrorCode`] 更细的稳定失败原因，供客户端给出可操作的本地化文案。
///
/// 同一个错误码可能对应性质完全不同的失败（如「市场镜像失效」与「本机服务断开」
/// 都属 `unavailable`），客户端优先按原因展示，缺失时再回退到按码的通用文案。
/// 未识别的原因解析为 [`ErrorReason::Unknown`]，新增变体不破坏旧客户端。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum ErrorReason {
    /// 无法连接插件市场索引源（网络 / 代理 / DNS）。
    MarketUnreachable,
    /// 插件市场索引内容无法解析或超出体积上限。
    MarketIndexInvalid,
    /// 插件市场索引序号低于本地已见高水位（可能被回滚或篡改）。
    MarketIndexRollback,
    /// 市场索引中没有该插件。
    PluginNotInMarket,
    /// 该插件版本已被发布者撤回。
    PluginYanked,
    /// 插件包的全部下载地址均不可用或内容校验不符。
    PluginDownloadFailed,
    /// 插件包超过体积上限。
    PluginPackageTooLarge,
    /// 插件包 / 插件目录未通过 manifest、脚本或版本门槛校验。
    PluginPackageInvalid,
    /// 对端发送了本端不认识的原因。
    #[serde(other)]
    Unknown,
}

/// FluxDown 应用错误的机器可读详情。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct RpcErrorData {
    pub code: ApplicationErrorCode,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<ErrorReason>,
}

impl RpcErrorData {
    /// 创建不带字段或版本上下文的错误详情。
    #[must_use]
    pub const fn new(code: ApplicationErrorCode, retryable: bool) -> Self {
        Self {
            code,
            retryable,
            field: None,
            revision: None,
            reason: None,
        }
    }

    /// 附加细分失败原因。
    #[must_use]
    pub const fn with_reason(mut self, reason: ErrorReason) -> Self {
        self.reason = Some(reason);
        self
    }
}

/// JSON-RPC 2.0 错误对象。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RpcErrorObject {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<RpcErrorData>,
}

impl RpcErrorObject {
    /// 创建带稳定 FluxDown 应用错误详情的错误对象。
    #[must_use]
    pub fn application(message: impl Into<String>, data: RpcErrorData) -> Self {
        Self {
            code: APPLICATION_ERROR_CODE,
            message: message.into(),
            data: Some(data),
        }
    }

    /// 创建不携带应用数据的 JSON 解析错误。
    #[must_use]
    pub fn parse_error(message: impl Into<String>) -> Self {
        Self {
            code: PARSE_ERROR_CODE,
            message: message.into(),
            data: None,
        }
    }

    /// 创建不携带应用数据的无效请求错误。
    #[must_use]
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: INVALID_REQUEST_CODE,
            message: message.into(),
            data: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{ApplicationErrorCode, ErrorReason, RpcErrorData};

    #[test]
    fn reason_is_optional_and_tolerates_unknown_values() {
        let legacy: RpcErrorData =
            serde_json::from_value(json!({ "code": "unavailable", "retryable": true }))
                .expect("error data without reason stays valid");
        assert_eq!(legacy.reason, None);

        let future: RpcErrorData = serde_json::from_value(json!({
            "code": "unavailable",
            "retryable": true,
            "reason": "someFutureReason"
        }))
        .expect("unknown reason must not reject the whole error");
        assert_eq!(future.reason, Some(ErrorReason::Unknown));

        let data = RpcErrorData::new(ApplicationErrorCode::Unavailable, true)
            .with_reason(ErrorReason::PluginDownloadFailed);
        assert_eq!(
            serde_json::to_value(&data).expect("serialize"),
            json!({ "code": "unavailable", "retryable": true, "reason": "pluginDownloadFailed" })
        );
    }
}
