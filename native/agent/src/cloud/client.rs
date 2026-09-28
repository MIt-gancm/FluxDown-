//! FluxCloud HTTPS 客户端、并发 401 单飞刷新与一次重放。
//!
//! 服务地址：`default_base_url` 由启动环境固定；仅调试构建允许经
//! [`CloudClient::set_endpoint`] 覆盖（持久化在 agent 私有状态），与 Flutter
//! `CloudApiConfig` 同一策略——正式包锁定默认地址，避免残留覆盖值指向失效地址。

use std::sync::{Arc, RwLock};
use std::time::Duration;

use fluxdown_protocol::{AgentEvent, CloudEndpointDto};
use reqwest::{Method, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::Mutex;

use super::models::{AuthResponse, CloudErrorBody, RefreshRequest};
use crate::event_hub::AgentEventHub;
use crate::state::{AgentState, CloudCredentials, StateStore};

/// 是否允许运行期覆盖 FluxCloud 地址；与 Flutter `kDebugMode` 门控一致。
const ENDPOINT_EDITABLE: bool = cfg!(debug_assertions);

#[derive(Clone)]
pub struct CloudClient {
    default_base_url: String,
    base_url: Arc<RwLock<String>>,
    http: reqwest::Client,
    stream_http: reqwest::Client,
    state: Arc<Mutex<AgentState>>,
    store: Arc<StateStore>,
    refresh: Arc<Mutex<()>>,
    /// 会话被清除（退出 / 刷新令牌被拒 / 远端撤销）时投影 `SessionChanged(None)`，
    /// 让 UI 与 agent 私有状态永不脱节。
    events: Option<AgentEventHub>,
}

impl CloudClient {
    pub fn new(
        base_url: String,
        state: Arc<Mutex<AgentState>>,
        store: Arc<StateStore>,
    ) -> Result<Self, CloudError> {
        validate_base_url(&base_url)?;
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(15))
            .pool_idle_timeout(Duration::from_secs(15))
            .build()
            .map_err(|error| CloudError::transport(error.to_string()))?;
        let stream_http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .map_err(|error| CloudError::transport(error.to_string()))?;
        let default_base_url = normalize_base_url(&base_url);
        Ok(Self {
            base_url: Arc::new(RwLock::new(default_base_url.clone())),
            default_base_url,
            http,
            stream_http,
            state,
            store,
            refresh: Arc::new(Mutex::new(())),
            events: None,
        })
    }

    /// 接入事件枢纽；生产运行链路必须调用，否则会话清除不会通知 UI。
    #[must_use]
    pub fn with_events(mut self, events: AgentEventHub) -> Self {
        self.events = Some(events);
        self
    }

    /// 启动时套用已持久化的地址覆盖；正式构建或覆盖值非法时保持默认地址。
    pub async fn restore_endpoint_override(&self) {
        if !ENDPOINT_EDITABLE {
            return;
        }
        let Some(base_url) = self.state.lock().await.cloud_base_url_override.clone() else {
            return;
        };
        match validate_base_url(&base_url) {
            Ok(()) => {
                self.swap_base_url(normalize_base_url(&base_url));
                tracing::info!(base_url = %base_url, "using FluxCloud endpoint override");
            }
            Err(error) => {
                tracing::warn!(base_url = %base_url, %error, "ignoring invalid FluxCloud endpoint override");
            }
        }
    }

    /// 当前生效地址、构建期默认地址与是否可改。
    pub fn endpoint(&self) -> CloudEndpointDto {
        CloudEndpointDto {
            base_url: self.current_base_url(),
            default_base_url: self.default_base_url.clone(),
            editable: ENDPOINT_EDITABLE,
        }
    }

    /// 覆盖（空串 = 恢复默认）FluxCloud 地址并持久化；后续请求立即使用新地址。
    /// 正式构建拒绝调用。
    pub async fn set_endpoint(&self, base_url: &str) -> Result<CloudEndpointDto, CloudError> {
        if !ENDPOINT_EDITABLE {
            return Err(CloudError::unsupported());
        }
        let trimmed = base_url.trim();
        let next = if trimmed.is_empty() {
            None
        } else {
            validate_base_url(trimmed)?;
            Some(normalize_base_url(trimmed)).filter(|url| *url != self.default_base_url)
        };
        {
            let mut state = self.state.lock().await;
            state.cloud_base_url_override.clone_from(&next);
            self.store
                .save(&state)
                .await
                .map_err(|error| CloudError::transport(error.to_string()))?;
        }
        self.swap_base_url(next.unwrap_or_else(|| self.default_base_url.clone()));
        Ok(self.endpoint())
    }

    fn current_base_url(&self) -> String {
        self.base_url
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn swap_base_url(&self, base_url: String) {
        *self
            .base_url
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = base_url;
    }

    /// 本机设备身份 `(device_id, device_name, platform)`，供认证请求体与请求头共用。
    pub(crate) async fn device_identity(&self) -> (String, String, String) {
        let state = self.state.lock().await;
        (
            state.device_id.clone(),
            state.device_name.clone(),
            state.platform.clone(),
        )
    }

    /// 无登录端点调用。
    pub async fn public<P: Serialize, R: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&P>,
    ) -> Result<R, CloudError> {
        let body = body
            .map(serde_json::to_value)
            .transpose()
            .map_err(|error| CloudError::invalid(error.to_string()))?;
        let response = self.send_once(method, path, body, None).await?;
        decode(response).await
    }

    /// 登录端点调用；401 共享一次刷新并只重放一次原请求。
    pub async fn authenticated<P: Serialize, R: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&P>,
    ) -> Result<R, CloudError> {
        let body = body
            .map(serde_json::to_value)
            .transpose()
            .map_err(|error| CloudError::invalid(error.to_string()))?;
        let attempted = self.access_token().await?;
        let response = self
            .send_once(method.clone(), path, body.clone(), Some(&attempted))
            .await?;
        if response.status() != StatusCode::UNAUTHORIZED {
            return decode(response).await;
        }
        let replay_token = self.refreshed_access_token(&attempted).await?;
        let replay = self
            .send_once(method, path, body, Some(&replay_token))
            .await?;
        decode(replay).await
    }

    /// 401 后取可重放的 access token：单飞刷新，其他调用方已轮换过则直接复用新令牌。
    /// 普通请求与 SSE 必须共用这一入口，否则并发刷新会用同一枚 refresh token 撞上服务端的一次性轮换。
    async fn refreshed_access_token(&self, attempted: &str) -> Result<String, CloudError> {
        let _guard = self.refresh.lock().await;
        let current = self.access_token().await?;
        if current != attempted {
            return Ok(current);
        }
        self.refresh_session().await
    }

    /// 显式登录/注册成功后，先原子持久化令牌轮换再返回无令牌会话。
    pub(crate) async fn persist_auth(
        &self,
        auth: AuthResponse,
    ) -> Result<fluxdown_protocol::AgentSessionDto, CloudError> {
        let session = auth.session();
        let mut state = self.state.lock().await;
        state.credentials = Some(CloudCredentials {
            access_token: auth.access_token,
            refresh_token: auth.refresh_token,
            expires_at_unix: now_unix().saturating_add(auth.expires_in),
            session: Some(session.clone()),
        });
        self.store
            .save(&state)
            .await
            .map_err(|error| CloudError::transport(error.to_string()))?;
        Ok(session)
    }

    /// 资料修改成功后更新无令牌会话并原子持久化。
    pub(crate) async fn persist_profile(
        &self,
        profile: fluxdown_protocol::CloudProfile,
    ) -> Result<fluxdown_protocol::AgentSessionDto, CloudError> {
        let mut state = self.state.lock().await;
        let session = state
            .credentials
            .as_mut()
            .and_then(|credentials| credentials.session.as_mut())
            .ok_or_else(CloudError::unauthorized)?;
        session.user = profile.user;
        session.entitlements = profile.entitlements;
        session.current_plan = profile.current_plan;
        let updated = session.clone();
        self.store
            .save(&state)
            .await
            .map_err(|error| CloudError::transport(error.to_string()))?;
        Ok(updated)
    }

    /// 清除完整会话（显式退出 / 刷新令牌被拒 / 远端撤销）并投影 `SessionChanged(None)`。
    pub async fn clear_session(&self) -> Result<(), CloudError> {
        {
            let mut state = self.state.lock().await;
            state.credentials = None;
            self.store
                .save(&state)
                .await
                .map_err(|error| CloudError::transport(error.to_string()))?;
        }
        if let Some(events) = &self.events {
            events.publish(AgentEvent::SessionChanged(Box::new(None)));
        }
        Ok(())
    }

    /// 调用方必须持有 `self.refresh` 锁。
    async fn refresh_session(&self) -> Result<String, CloudError> {
        let refresh_token = {
            let state = self.state.lock().await;
            state
                .credentials
                .as_ref()
                .ok_or_else(CloudError::unauthorized)?
                .refresh_token
                .clone()
        };
        let response = self
            .send_once(
                Method::POST,
                "/api/v1/auth/refresh",
                Some(
                    serde_json::to_value(RefreshRequest {
                        refresh_token: &refresh_token,
                    })
                    .map_err(|error| CloudError::invalid(error.to_string()))?,
                ),
                None,
            )
            .await?;
        if matches!(
            response.status(),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        ) {
            return self.reject_refresh_token(&refresh_token).await;
        }
        let auth = decode::<AuthResponse>(response).await?;
        let access_token = auth.access_token.clone();
        self.persist_auth(auth).await?;
        Ok(access_token)
    }

    /// 服务端拒绝了 `rejected`：仅当它仍是当前凭证时才登出；期间凭证已被替换
    /// （并发刷新 / 重新登录）说明被拒的只是过期副本，沿用当前 access token。
    async fn reject_refresh_token(&self, rejected: &str) -> Result<String, CloudError> {
        let current = {
            let state = self.state.lock().await;
            state.credentials.as_ref().map(|credentials| {
                (
                    credentials.refresh_token.clone(),
                    credentials.access_token.clone(),
                )
            })
        };
        match current {
            Some((refresh, access)) if refresh != rejected && !access.is_empty() => Ok(access),
            _ => {
                self.clear_session().await?;
                Err(CloudError::unauthorized())
            }
        }
    }

    async fn access_token(&self) -> Result<String, CloudError> {
        self.state
            .lock()
            .await
            .credentials
            .as_ref()
            .map(|credentials| credentials.access_token.clone())
            .filter(|token| !token.is_empty())
            .ok_or_else(CloudError::unauthorized)
    }

    pub(crate) async fn refresh_token(&self) -> Result<String, CloudError> {
        let state = self.state.lock().await;
        state
            .credentials
            .as_ref()
            .map(|credentials| credentials.refresh_token.clone())
            .filter(|token| !token.is_empty())
            .ok_or_else(CloudError::unauthorized)
    }

    pub(crate) async fn is_authenticated(&self) -> bool {
        self.state
            .lock()
            .await
            .credentials
            .as_ref()
            .is_some_and(|credentials| {
                !credentials.access_token.is_empty() && !credentials.refresh_token.is_empty()
            })
    }

    pub(crate) async fn authenticated_stream(
        &self,
        path: &str,
    ) -> Result<reqwest::Response, CloudError> {
        let access_token = self.access_token().await?;
        let response = self.send_stream_once(path, &access_token).await?;
        if response.status() != StatusCode::UNAUTHORIZED {
            return ensure_success(response).await;
        }
        let replay_token = self.refreshed_access_token(&access_token).await?;
        ensure_success(self.send_stream_once(path, &replay_token).await?).await
    }

    async fn send_stream_once(
        &self,
        path: &str,
        bearer: &str,
    ) -> Result<reqwest::Response, CloudError> {
        let (device_id, device_name, platform) = self.device_identity().await;
        self.stream_http
            .get(format!("{}{}", self.current_base_url(), path))
            .bearer_auth(bearer)
            .header("Accept", "text/event-stream")
            .header("X-FluxDown-Device-Id", device_id)
            .header("X-FluxDown-Device-Name", device_name)
            .header("X-FluxDown-Platform", platform)
            .header("X-FluxDown-Version", env!("CARGO_PKG_VERSION"))
            .send()
            .await
            .map_err(|error| CloudError::transport(error_chain(&error)))
    }

    async fn send_once(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        bearer: Option<&str>,
    ) -> Result<reqwest::Response, CloudError> {
        let (device_id, device_name, platform) = self.device_identity().await;
        let mut request = self
            .http
            .request(method, format!("{}{}", self.current_base_url(), path))
            .header("X-FluxDown-Device-Id", device_id)
            .header("X-FluxDown-Device-Name", device_name)
            .header("X-FluxDown-Platform", platform)
            .header("X-FluxDown-Version", env!("CARGO_PKG_VERSION"));
        if let Some(token) = bearer {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        request
            .send()
            .await
            .map_err(|error| CloudError::transport(error_chain(&error)))
    }
}

async fn ensure_success(response: reqwest::Response) -> Result<reqwest::Response, CloudError> {
    if response.status().is_success() {
        Ok(response)
    } else {
        Err(response_error(response).await)
    }
}

async fn response_error(response: reqwest::Response) -> CloudError {
    let status = response.status();
    let retryable = status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS;
    let body = response.json::<CloudErrorBody>().await.ok();
    CloudError {
        status: Some(status.as_u16()),
        code: body.as_ref().and_then(|body| body.code.clone()),
        message: body
            .and_then(|body| body.message)
            .unwrap_or_else(|| format!("FluxCloud HTTP {status}")),
        retryable,
    }
}

async fn decode<R: DeserializeOwned>(response: reqwest::Response) -> Result<R, CloudError> {
    let status = response.status();
    if status.is_success() {
        return response
            .json::<R>()
            .await
            .map_err(|error| CloudError::transport(error_chain(&error)));
    }
    Err(response_error(response).await)
}

/// reqwest 的 `Display`（含 `{:#}`）只输出最外层「error sending request for url」，
/// 超时 / 连接重置 / TLS / 代理等根因都在 `source()` 链里，必须逐层拼出。
fn error_chain(error: &reqwest::Error) -> String {
    let mut message = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

fn validate_base_url(base_url: &str) -> Result<(), CloudError> {
    let url =
        reqwest::Url::parse(base_url).map_err(|error| CloudError::invalid(error.to_string()))?;
    let loopback = url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    if url.scheme() == "https" || (loopback && url.scheme() == "http") {
        Ok(())
    } else {
        Err(CloudError::invalid(
            "FluxCloud URL must use HTTPS outside loopback".to_owned(),
        ))
    }
}

fn normalize_base_url(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_owned()
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs().min(i64::MAX as u64) as i64)
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct CloudError {
    pub status: Option<u16>,
    pub code: Option<String>,
    pub message: String,
    pub retryable: bool,
}

impl CloudError {
    fn unauthorized() -> Self {
        Self {
            status: Some(401),
            code: Some("unauthorized".to_owned()),
            message: "authentication required".to_owned(),
            retryable: false,
        }
    }

    fn unsupported() -> Self {
        Self {
            status: None,
            code: Some("unsupported".to_owned()),
            message: "FluxCloud endpoint is fixed in release builds".to_owned(),
            retryable: false,
        }
    }

    fn transport(message: String) -> Self {
        Self {
            status: None,
            code: None,
            message,
            retryable: true,
        }
    }

    fn invalid(message: String) -> Self {
        Self {
            status: None,
            code: Some("invalidArgument".to_owned()),
            message,
            retryable: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::Router;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode, header};
    use axum::response::{IntoResponse, Response};
    use axum::routing::{get, post};
    use serde_json::{Value, json};
    use tokio::sync::Mutex;

    use super::CloudClient;
    use crate::state::{AgentState, CloudCredentials, StateStore};

    #[derive(Clone)]
    struct MockState {
        refreshes: Arc<AtomicUsize>,
    }

    async fn protected(headers: HeaderMap) -> Response {
        if headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            == Some("Bearer new-access")
        {
            axum::Json(json!({ "ok": true })).into_response()
        } else {
            StatusCode::UNAUTHORIZED.into_response()
        }
    }

    async fn refresh(State(state): State<MockState>) -> Response {
        state.refreshes.fetch_add(1, Ordering::SeqCst);
        axum::Json(json!({
            "accessToken": "new-access",
            "refreshToken": "new-refresh",
            "expiresIn": 3600,
            "user": {
                "id": "u1",
                "email": "user@example.com",
                "nickname": "User",
                "plan": "free",
                "status": "active",
                "createdAt": ""
            },
            "entitlements": {},
            "device": {
                "id": "row1",
                "deviceId": "device1",
                "name": "Desktop",
                "createdAt": "",
                "lastSeenAt": ""
            }
        }))
        .into_response()
    }

    async fn reject_refresh() -> StatusCode {
        StatusCode::UNAUTHORIZED
    }

    #[tokio::test]
    async fn concurrent_unauthorized_requests_share_one_refresh_and_replay() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock cloud");
        let address = listener.local_addr().expect("mock address");
        let refreshes = Arc::new(AtomicUsize::new(0));
        let app = Router::new()
            .route("/api/v1/test", get(protected))
            .route("/api/v1/auth/refresh", post(refresh))
            .with_state(MockState {
                refreshes: refreshes.clone(),
            });
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let dir = std::env::temp_dir().join(format!(
            "fluxdown_cloud_test_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let store = Arc::new(StateStore::open(dir.clone()).await.expect("state store"));
        let state = Arc::new(Mutex::new(AgentState {
            device_id: "device1".to_owned(),
            device_name: "Desktop".to_owned(),
            platform: "linux".to_owned(),
            credentials: Some(CloudCredentials {
                access_token: "old-access".to_owned(),
                refresh_token: "refresh".to_owned(),
                expires_at_unix: 0,
                session: None,
            }),
            ..AgentState::default()
        }));
        {
            let state = state.lock().await;
            store.save(&state).await.expect("save state");
        }
        let client =
            CloudClient::new(format!("http://{address}"), state, store).expect("cloud client");
        let first =
            client.authenticated::<Value, Value>(reqwest::Method::GET, "/api/v1/test", None);
        let second =
            client.authenticated::<Value, Value>(reqwest::Method::GET, "/api/v1/test", None);
        let (first, second) = tokio::join!(first, second);
        assert_eq!(first.expect("first replay")["ok"], true);
        assert_eq!(second.expect("second replay")["ok"], true);
        assert_eq!(refreshes.load(Ordering::SeqCst), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn revoked_refresh_clears_credentials_in_memory_and_on_disk() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind revoked cloud mock");
        let address = listener.local_addr().expect("revoked cloud address");
        let app = Router::new()
            .route("/api/v1/test", get(protected))
            .route("/api/v1/auth/refresh", post(reject_refresh));
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let dir = std::env::temp_dir().join(format!(
            "fluxdown_cloud_revoked_test_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let store = Arc::new(
            StateStore::open(dir.clone())
                .await
                .expect("revoked state store"),
        );
        let state = Arc::new(Mutex::new(AgentState {
            device_id: "device1".to_owned(),
            credentials: Some(CloudCredentials {
                access_token: "revoked-access".to_owned(),
                refresh_token: "revoked-refresh".to_owned(),
                expires_at_unix: 0,
                session: None,
            }),
            ..AgentState::default()
        }));
        {
            let state = state.lock().await;
            store.save(&state).await.expect("save revoked state");
        }
        let client = CloudClient::new(format!("http://{address}"), state.clone(), store.clone())
            .expect("revoked cloud client");

        let error = client
            .authenticated::<Value, Value>(reqwest::Method::GET, "/api/v1/test", None)
            .await
            .expect_err("revoked refresh must fail");
        assert_eq!(error.status, Some(StatusCode::UNAUTHORIZED.as_u16()));
        assert!(state.lock().await.credentials.is_none());
        assert!(
            store
                .load()
                .await
                .expect("reload revoked state")
                .credentials
                .is_none()
        );
        drop(client);
        drop(state);
        drop(store);
        let _ = tokio::fs::remove_dir_all(dir).await;
    }

    async fn whoami(State(name): State<&'static str>) -> Response {
        axum::Json(json!({ "server": name })).into_response()
    }

    async fn spawn_named(name: &'static str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind named mock");
        let address = listener.local_addr().expect("named mock address");
        let app = Router::new()
            .route("/api/v1/whoami", get(whoami))
            .with_state(name);
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn endpoint_override_persists_routes_and_resets() {
        let default_url = spawn_named("default").await;
        let override_url = spawn_named("override").await;
        let dir = std::env::temp_dir().join(format!(
            "fluxdown_cloud_endpoint_test_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let store = Arc::new(StateStore::open(dir.clone()).await.expect("state store"));
        let state = Arc::new(Mutex::new(AgentState::default()));
        let client = CloudClient::new(default_url.clone(), state.clone(), store.clone())
            .expect("cloud client");

        let endpoint = client.endpoint();
        assert!(endpoint.editable, "tests run under debug_assertions");
        assert_eq!(endpoint.base_url, default_url);
        assert_eq!(endpoint.default_base_url, default_url);

        let set = client
            .set_endpoint(&format!("{override_url}/"))
            .await
            .expect("override accepted");
        assert_eq!(set.base_url, override_url);
        assert_eq!(set.default_base_url, default_url);
        let persisted = store.load().await.expect("reload state");
        assert_eq!(
            persisted.cloud_base_url_override.as_deref(),
            Some(override_url.as_str())
        );
        let who = client
            .public::<Value, Value>(reqwest::Method::GET, "/api/v1/whoami", None)
            .await
            .expect("override reachable");
        assert_eq!(who["server"], "override");

        let restored = CloudClient::new(default_url.clone(), state.clone(), store.clone())
            .expect("second client");
        restored.restore_endpoint_override().await;
        assert_eq!(restored.endpoint().base_url, override_url);

        let rejected = client
            .set_endpoint("http://example.com")
            .await
            .expect_err("non-loopback http must be rejected");
        assert_eq!(rejected.code.as_deref(), Some("invalidArgument"));
        assert_eq!(client.endpoint().base_url, override_url);

        let reset = client.set_endpoint("  ").await.expect("reset accepted");
        assert_eq!(reset.base_url, default_url);
        assert!(
            store
                .load()
                .await
                .expect("reload after reset")
                .cloud_base_url_override
                .is_none()
        );
        let who = client
            .public::<Value, Value>(reqwest::Method::GET, "/api/v1/whoami", None)
            .await
            .expect("default reachable");
        assert_eq!(who["server"], "default");

        drop(client);
        drop(restored);
        drop(state);
        drop(store);
        let _ = tokio::fs::remove_dir_all(dir).await;
    }

    /// 服务端一次性轮换：refresh token 用过即作废，复用返回 401。
    #[derive(Clone, Default)]
    struct RotatingCloud {
        inner: Arc<Mutex<(String, String, usize)>>,
    }

    fn bearer(headers: &HeaderMap) -> String {
        headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .unwrap_or_default()
            .to_owned()
    }

    async fn rotating_protected(
        State(cloud): State<RotatingCloud>,
        headers: HeaderMap,
    ) -> Response {
        if bearer(&headers) == cloud.inner.lock().await.0 {
            axum::Json(json!({ "ok": true })).into_response()
        } else {
            StatusCode::UNAUTHORIZED.into_response()
        }
    }

    async fn rotating_refresh(
        State(cloud): State<RotatingCloud>,
        axum::Json(body): axum::Json<Value>,
    ) -> Response {
        // 模拟真实往返延迟，放大并发刷新窗口。
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let mut inner = cloud.inner.lock().await;
        if body["refreshToken"].as_str() != Some(inner.1.as_str()) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        inner.2 += 1;
        inner.0 = format!("access-{}", inner.2);
        inner.1 = format!("refresh-{}", inner.2);
        axum::Json(json!({
            "accessToken": inner.0,
            "refreshToken": inner.1,
            "expiresIn": 900,
            "user": { "id": "u1", "email": "user@example.com" },
            "device": { "id": "row1", "deviceId": "device1" }
        }))
        .into_response()
    }

    /// 重启后 access 已过期：普通请求与 SSE 重连同时 401，必须只刷新一次且保住会话
    /// （曾因 SSE 绕过单飞锁，第二次刷新被拒后清空了刚轮换到手的新凭证）。
    #[tokio::test]
    async fn concurrent_request_and_stream_refresh_once_and_keep_session() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind rotating cloud");
        let address = listener.local_addr().expect("rotating address");
        let cloud = RotatingCloud::default();
        *cloud.inner.lock().await = ("access-0".to_owned(), "refresh-0".to_owned(), 0);
        let app = Router::new()
            .route("/api/v1/test", get(rotating_protected))
            .route("/api/v1/stream", get(rotating_protected))
            .route("/api/v1/auth/refresh", post(rotating_refresh))
            .with_state(cloud.clone());
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let dir = std::env::temp_dir().join(format!(
            "fluxdown_cloud_rotation_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let store = Arc::new(StateStore::open(dir.clone()).await.expect("state store"));
        let session = serde_json::from_value(json!({
            "user": { "id": "u1", "email": "user@example.com" },
            "device": { "id": "row1", "deviceId": "device1" }
        }))
        .expect("session dto");
        let state = Arc::new(Mutex::new(AgentState {
            device_id: "device1".to_owned(),
            credentials: Some(CloudCredentials {
                access_token: "expired-access".to_owned(),
                refresh_token: "refresh-0".to_owned(),
                expires_at_unix: 0,
                session: Some(session),
            }),
            ..AgentState::default()
        }));
        let client = CloudClient::new(format!("http://{address}"), state.clone(), store.clone())
            .expect("cloud client");

        let request =
            client.authenticated::<Value, Value>(reqwest::Method::GET, "/api/v1/test", None);
        let stream = client.authenticated_stream("/api/v1/stream");
        let (request, stream) = tokio::join!(request, stream);
        assert_eq!(request.expect("request replay")["ok"], true);
        assert!(stream.expect("stream replay").status().is_success());
        assert_eq!(cloud.inner.lock().await.2, 1, "exactly one rotation");
        let persisted = store
            .load()
            .await
            .expect("reload state")
            .credentials
            .expect("session survives");
        assert_eq!(persisted.refresh_token, "refresh-1");

        drop(client);
        drop(state);
        drop(store);
        let _ = tokio::fs::remove_dir_all(dir).await;
    }
}
