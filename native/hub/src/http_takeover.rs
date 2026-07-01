//! 本地 HTTP 接管服务 —— 把浏览器油猴脚本（Tampermonkey / Violentmonkey）捕获的
//! 下载交给 FluxDown 桌面应用。
//!
//! ## 为什么需要它
//!
//! 油猴脚本运行在**页面上下文**，无法使用浏览器扩展专属的 Native Messaging、
//! `chrome.downloads` 或 Unix Domain Socket，唯一能与本机程序通信的通道是
//! `GM_xmlhttpRequest`（可跨域、可访问 `http://127.0.0.1`、绕过页面 CORS）。
//! 因此本模块提供一个轻量的本机 HTTP 端点，脚本通过它把下载请求 POST 过来。
//!
//! 收到的 [`DownloadRequest`] 被原样塞进与 Native Messaging **同一条 mpsc channel**
//! （`download_actor` 的 `native_msg_rx` 分支消费），从而 100% 复用既有的
//! 「缓存请求事务 → `ExternalDownloadRequest` 信号 → 快速下载弹框 → `create_task`」
//! 全链路，零额外 Dart 改动、与浏览器扩展完全一致的接管体验。
//!
//! ## 安全模型
//!
//! 本机 HTTP 端口比 Unix Socket 暴露面更大，因此采取多层防护：
//!
//! 1. **仅监听 `127.0.0.1`**（硬编码，永不监听 `0.0.0.0`），外网不可达。
//! 2. **自定义请求头门禁**：变更类端点 `/download`、`/download/batch` 要求请求带
//!    `X-FluxDown-Client` 头。恶意网页用 `fetch()` 跨域携带自定义头会触发
//!    CORS 预检（OPTIONS），而本服务**不返回** `Access-Control-Allow-Origin`，
//!    预检失败 → 浏览器拦截真实请求。油猴 `GM_xmlhttpRequest` 不受 CORS 约束、
//!    可自由设置该头，故脚本正常工作、恶意网页被挡。
//! 3. **JSON-RPC 合法性门禁**：aria2 兼容端点 `/jsonrpc` 不校验 `Content-Type`
//!    （与真实 aria2 行为一致），改以「请求体能否解析为合法 JSON-RPC」为准入门槛。
//!    放宽 `Content-Type` 是为兼容广泛存在的 aria2 风格脚本（如「网盘直链下载助手」
//!    panlinker），它们经 `GM_xmlhttpRequest` 发送但默认不带 `application/json` 头。
//!    代价是恶意网页可用 `text/plain` 简单请求绕过 CORS 预检直打 `/jsonrpc`，
//!    故 `/jsonrpc` 的纵深防御依赖第 4、5 条（可选 token + 最终确认弹框）；
//!    而变更类端点 `/download`、`/download/batch` 仍由第 2 条的自定义头门禁保护。
//! 4. **可选 token**（`local_server_token` 非空时启用）：请求需带匹配的
//!    `X-FluxDown-Token` 头，常量时间比较，作纵深防御。
//! 5. **最终安全网**：所有下载都会在 FluxDown 中弹出确认框，杜绝静默下载。

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use crate::logger::log_info;
use crate::native_messaging::DownloadRequest;

/// 请求体大小上限：4 MB（足够容纳批量 URL 列表）。
const MAX_BODY_SIZE: usize = 4 * 1024 * 1024;
/// 请求头部分大小上限：64 KB。
const MAX_HEADER_SIZE: usize = 64 * 1024;
/// 读取完整请求头的超时（防慢速攻击挂死连接）。
const HEADER_TIMEOUT: Duration = Duration::from_secs(15);
/// 油猴脚本必须携带的来源标识头（小写存储以便大小写不敏感匹配）。
const CLIENT_HEADER: &str = "x-fluxdown-client";
/// 可选鉴权 token 头。
const TOKEN_HEADER: &str = "x-fluxdown-token";

/// 本地 HTTP 接管服务配置，从 DB config 表加载。
#[derive(Debug, Clone)]
pub struct HttpTakeoverConfig {
    pub enabled: bool,
    pub port: u16,
    /// 空字符串 = 不鉴权。
    pub token: String,
}

impl HttpTakeoverConfig {
    pub fn from_config_map(map: &HashMap<String, String>) -> Self {
        Self {
            enabled: map
                .get("local_server_enabled")
                .map(|v| v == "true")
                .unwrap_or(true),
            port: map
                .get("local_server_port")
                .and_then(|v| v.parse().ok())
                .unwrap_or(17800),
            token: map.get("local_server_token").cloned().unwrap_or_default(),
        }
    }

    fn bind_addr(&self) -> SocketAddr {
        // 永远只监听本机回环地址。
        SocketAddr::from((Ipv4Addr::LOCALHOST, self.port))
    }
}

/// 启动本地 HTTP 接管服务。
///
/// 收到的下载请求通过 `tx` 发送到 `download_actor` 的 `native_msg_rx` 分支，
/// 与浏览器扩展走完全相同的处理路径。若 `config.enabled == false`，直接返回不监听。
pub fn spawn_http_takeover_server(tx: mpsc::Sender<DownloadRequest>, config: HttpTakeoverConfig) {
    if !config.enabled {
        log_info!("[http-takeover] disabled by config");
        return;
    }

    let addr = config.bind_addr();
    tokio::spawn(async move {
        let listener = match TcpListener::bind(addr).await {
            Ok(l) => {
                log_info!("[http-takeover] listening on http://{}", addr);
                l
            }
            Err(e) => {
                // 端口被占用等错误不影响主功能，仅本特性不可用。
                log_info!("[http-takeover] failed to bind {}: {}", addr, e);
                return;
            }
        };
        run_accept_loop(listener, tx, config).await;
    });
}

/// accept 循环：每个连接 spawn 一个独立任务处理。抽出以便集成测试。
async fn run_accept_loop(
    listener: TcpListener,
    tx: mpsc::Sender<DownloadRequest>,
    config: HttpTakeoverConfig,
) {
    loop {
        match listener.accept().await {
            Ok((stream, _peer)) => {
                let tx = tx.clone();
                let config = config.clone();
                tokio::spawn(async move {
                    handle_connection(stream, tx, config).await;
                });
            }
            Err(e) => {
                log_info!("[http-takeover] accept error: {}", e);
                // 短暂退避后继续。
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP 请求解析（手写最小 HTTP/1.1，避免引入 hyper/axum 重依赖）
// ---------------------------------------------------------------------------

/// 解析后的 HTTP 请求。
struct HttpRequest {
    method: String,
    /// 含 query 的原始路径，如 `/download?token=x`。
    path: String,
    /// 头部，key 全部小写。
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

/// 在 `haystack` 中查找子序列 `needle` 的起始下标。
fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// 解析请求行 + 头部文本，返回 (method, path, headers)。
///
/// 纯函数，便于单元测试。
fn parse_request_head(head: &str) -> Option<(String, String, HashMap<String, String>)> {
    let mut lines = head.split("\r\n");
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    // 第三段是 HTTP 版本，忽略。

    let mut headers = HashMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    Some((method, path, headers))
}

/// 去掉 query 部分，返回纯路径。
fn path_only(path: &str) -> &str {
    match path.split_once('?') {
        Some((p, _)) => p,
        None => path,
    }
}

/// 从 TcpStream 读取一个完整 HTTP 请求。
///
/// 返回 `Ok(None)` 表示连接在读到任何数据前就关闭了（无需回应）。
async fn read_request(stream: &mut TcpStream) -> std::io::Result<Option<HttpRequest>> {
    let mut buf: Vec<u8> = Vec::with_capacity(2048);
    let mut tmp = [0u8; 4096];

    // 1. 读到请求头结束标记 \r\n\r\n。
    let header_end = loop {
        if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
            break pos;
        }
        if buf.len() > MAX_HEADER_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "request header too large",
            ));
        }
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            if buf.is_empty() {
                return Ok(None); // 干净关闭
            }
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "connection closed before headers complete",
            ));
        }
        buf.extend_from_slice(&tmp[..n]);
    };

    let head_str = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let (method, path, headers) = parse_request_head(&head_str).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "malformed request line")
    })?;

    // 2. 按 Content-Length 读取请求体。
    let content_length = headers
        .get("content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    if content_length > MAX_BODY_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "request body too large",
        ));
    }

    let body_start = header_end + 4;
    let mut body: Vec<u8> = buf[body_start..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(content_length);

    Ok(Some(HttpRequest {
        method,
        path,
        headers,
        body,
    }))
}

// ---------------------------------------------------------------------------
// 响应写出
// ---------------------------------------------------------------------------

async fn write_response(stream: &mut TcpStream, status: u16, status_text: &str, json_body: &str) {
    let resp = format!(
        "HTTP/1.1 {status} {status_text}\r\n\
         Content-Type: application/json; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\
         \r\n\
         {json_body}",
        json_body.len()
    );
    let _ = stream.write_all(resp.as_bytes()).await;
    let _ = stream.flush().await;
}

/// 写一个 `{"success":bool,"message":...}` 形态的响应。
async fn write_result(
    stream: &mut TcpStream,
    status: u16,
    status_text: &str,
    success: bool,
    message: &str,
) {
    let body = json!({ "success": success, "message": message }).to_string();
    write_response(stream, status, status_text, &body).await;
}

/// 204 No Content（用于 OPTIONS 预检；故意不带 ACAO 头以拦截跨域网页）。
async fn write_no_content(stream: &mut TcpStream) {
    let resp = "HTTP/1.1 204 No Content\r\n\
                Allow: GET, POST, OPTIONS\r\n\
                Connection: close\r\n\r\n";
    let _ = stream.write_all(resp.as_bytes()).await;
    let _ = stream.flush().await;
}

// ---------------------------------------------------------------------------
// 鉴权
// ---------------------------------------------------------------------------

/// 常量时间字符串比较，防 timing attack。
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// 校验 token（若服务端配置了 token）。
fn check_token(headers: &HashMap<String, String>, config: &HttpTakeoverConfig) -> bool {
    if config.token.is_empty() {
        return true;
    }
    let provided = headers.get(TOKEN_HEADER).map(|s| s.as_str()).unwrap_or("");
    constant_time_eq(provided, &config.token)
}

/// 变更类端点门禁：要求 `X-FluxDown-Client` 头存在（挡跨域网页）+ token 校验。
fn check_takeover_auth(
    headers: &HashMap<String, String>,
    config: &HttpTakeoverConfig,
) -> Result<(), (u16, &'static str, &'static str)> {
    let has_client = headers
        .get(CLIENT_HEADER)
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    if !has_client {
        return Err((403, "Forbidden", "missing X-FluxDown-Client header"));
    }
    if !check_token(headers, config) {
        return Err((401, "Unauthorized", "invalid or missing token"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 连接处理 + 路由
// ---------------------------------------------------------------------------

async fn handle_connection(
    mut stream: TcpStream,
    tx: mpsc::Sender<DownloadRequest>,
    config: HttpTakeoverConfig,
) {
    let req = match tokio::time::timeout(HEADER_TIMEOUT, read_request(&mut stream)).await {
        Ok(Ok(Some(req))) => req,
        Ok(Ok(None)) => return, // 连接干净关闭
        Ok(Err(e)) => {
            log_info!("[http-takeover] read error: {}", e);
            write_result(&mut stream, 400, "Bad Request", false, "malformed request").await;
            return;
        }
        Err(_) => {
            log_info!("[http-takeover] header read timed out");
            return;
        }
    };

    let path = path_only(&req.path).to_string();
    match (req.method.as_str(), path.as_str()) {
        ("OPTIONS", _) => write_no_content(&mut stream).await,

        ("GET", "/ping") => {
            let body = json!({
                "success": true,
                "app": "FluxDown",
                "version": env!("CARGO_PKG_VERSION"),
                "message": "pong",
            })
            .to_string();
            write_response(&mut stream, 200, "OK", &body).await;
        }

        ("POST", "/download") => {
            if let Err((code, text, msg)) = check_takeover_auth(&req.headers, &config) {
                write_result(&mut stream, code, text, false, msg).await;
                return;
            }
            match serde_json::from_slice::<DownloadRequest>(&req.body) {
                Ok(dl) => {
                    log_info!("[http-takeover] /download url={}", dl.url);
                    handle_download(&mut stream, &tx, dl).await;
                }
                Err(e) => {
                    write_result(
                        &mut stream,
                        400,
                        "Bad Request",
                        false,
                        &format!("invalid download payload: {e}"),
                    )
                    .await;
                }
            }
        }

        ("POST", "/download/batch") => {
            if let Err((code, text, msg)) = check_takeover_auth(&req.headers, &config) {
                write_result(&mut stream, code, text, false, msg).await;
                return;
            }
            match parse_batch(&req.body) {
                Ok(dl) => {
                    let count = dl.url.split('\n').filter(|s| !s.trim().is_empty()).count();
                    log_info!("[http-takeover] /download/batch {} urls", count);
                    handle_download(&mut stream, &tx, dl).await;
                }
                Err(e) => {
                    write_result(&mut stream, 400, "Bad Request", false, &e).await;
                }
            }
        }

        // aria2 JSON-RPC 兼容垫片（让既有「发送到 aria2」类脚本也能用）。
        ("POST", "/jsonrpc") => {
            handle_jsonrpc(&mut stream, &tx, &config, &req).await;
        }

        _ => {
            write_result(&mut stream, 404, "Not Found", false, "unknown endpoint").await;
        }
    }
}

/// 把一个下载请求送进 channel 并回应。
async fn handle_download(
    stream: &mut TcpStream,
    tx: &mpsc::Sender<DownloadRequest>,
    dl: DownloadRequest,
) {
    if dl.url.trim().is_empty() {
        write_result(stream, 400, "Bad Request", false, "url is required").await;
        return;
    }
    match tx.send(dl).await {
        Ok(()) => write_result(stream, 200, "OK", true, "download accepted").await,
        Err(_) => {
            // actor 已退出（应用关闭中）。
            write_result(
                stream,
                503,
                "Service Unavailable",
                false,
                "app shutting down",
            )
            .await;
        }
    }
}

/// 解析批量下载请求体。
///
/// 支持两种形态：
/// - `{ "urls": ["u1","u2"], "referrer": "", "cookies": "", "headers": {}, "userAgent": "" }`
/// - `{ "items": [ { ...DownloadRequest }, ... ] }`（取各项 url，共享首个非空 cookies/referrer）
///
/// 统一合并为**单个** [`DownloadRequest`]，`url` 以换行符连接 —— 与 Dart 快速下载
/// 弹框「按换行拆分批量创建」的既有约定一致，用户只需确认一次。
fn parse_batch(body: &[u8]) -> Result<DownloadRequest, String> {
    let v: Value = serde_json::from_slice(body).map_err(|e| format!("invalid JSON: {e}"))?;

    // 形态 A：urls 数组 + 共享字段
    if let Some(urls) = v.get("urls").and_then(|u| u.as_array()) {
        let joined = urls
            .iter()
            .filter_map(|u| u.as_str())
            .filter(|s| !s.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if joined.is_empty() {
            return Err("urls is empty".to_string());
        }
        let headers = v
            .get("headers")
            .and_then(|h| h.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, val)| val.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect::<HashMap<String, String>>()
            })
            .filter(|m| !m.is_empty());
        return Ok(DownloadRequest {
            url: joined,
            filename: String::new(),
            referrer: str_field(&v, "referrer"),
            cookies: str_field(&v, "cookies"),
            headers,
            file_size: v.get("fileSize").and_then(|f| f.as_i64()),
            mime_type: None,
            method: None,
            body: None,
        });
    }

    // 形态 B：items 数组（每项是一个 DownloadRequest）
    if let Some(items) = v.get("items").and_then(|i| i.as_array()) {
        let parsed: Vec<DownloadRequest> = items
            .iter()
            .filter_map(|item| serde_json::from_value::<DownloadRequest>(item.clone()).ok())
            .collect();
        if parsed.is_empty() {
            return Err("items is empty or invalid".to_string());
        }
        let joined = parsed
            .iter()
            .map(|d| d.url.as_str())
            .filter(|s| !s.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if joined.is_empty() {
            return Err("no valid urls in items".to_string());
        }
        // 共享首个非空 cookies / referrer / headers。
        let cookies = parsed
            .iter()
            .map(|d| d.cookies.clone())
            .find(|c| !c.is_empty())
            .unwrap_or_default();
        let referrer = parsed
            .iter()
            .map(|d| d.referrer.clone())
            .find(|r| !r.is_empty())
            .unwrap_or_default();
        let headers = parsed
            .iter()
            .find_map(|d| d.headers.clone().filter(|h| !h.is_empty()));
        return Ok(DownloadRequest {
            url: joined,
            filename: String::new(),
            referrer,
            cookies,
            headers,
            file_size: None,
            mime_type: None,
            method: None,
            body: None,
        });
    }

    Err("expected `urls` array or `items` array".to_string())
}

fn str_field(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string()
}

// ---------------------------------------------------------------------------
// aria2 JSON-RPC 兼容垫片
// ---------------------------------------------------------------------------

/// 处理 `POST /jsonrpc`。aria2 JSON-RPC 兼容垫片，实现下载相关子集：
/// `aria2.addUri`、`aria2.getVersion`、`aria2.getGlobalStat`、
/// `system.multicall`、`system.listMethods`。
///
/// 同时支持**单个请求对象**与**顶层 JSON 数组批量**（gofile-enhanced 等脚本
/// 一次 POST 多个 JSON-RPC 对象的实际行为）。
///
/// 安全：不校验 `Content-Type`（与真实 aria2 一致，兼容不带 `application/json` 头的
/// aria2 风格脚本），改以请求体能否解析为合法 JSON-RPC 为准入门槛；并支持 aria2
/// 约定的 `token:xxx`（params[0]）或 `X-FluxDown-Token` 头鉴权。详见模块级「安全模型」。
async fn handle_jsonrpc(
    stream: &mut TcpStream,
    tx: &mpsc::Sender<DownloadRequest>,
    config: &HttpTakeoverConfig,
    req: &HttpRequest,
) {
    // 准入门槛：请求体须能解析为合法 JSON（解析失败即非 JSON-RPC 客户端，拒绝）。
    let parsed: Value = match serde_json::from_slice(&req.body) {
        Ok(v) => v,
        Err(e) => {
            let err = rpc_err(&Value::Null, -32700, &format!("parse error: {e}"));
            write_response(stream, 200, "OK", &err.to_string()).await;
            return;
        }
    };

    let response: Value = match parsed {
        // 顶层数组：逐个处理，返回等长结果数组。
        Value::Array(calls) => {
            let mut out = Vec::with_capacity(calls.len());
            for call in &calls {
                out.push(dispatch_rpc_call(call, tx, config, &req.headers).await);
            }
            Value::Array(out)
        }
        obj @ Value::Object(_) => dispatch_rpc_call(&obj, tx, config, &req.headers).await,
        _ => rpc_err(&Value::Null, -32600, "invalid request"),
    };

    write_response(stream, 200, "OK", &response.to_string()).await;
}

/// 校验单个 JSON-RPC 调用的 token（服务端配置了 token 时）。
/// 接受 `X-FluxDown-Token` 头或 aria2 约定的 `params[0] = "token:xxx"`。
fn jsonrpc_token_ok(
    params: &Value,
    headers: &HashMap<String, String>,
    config: &HttpTakeoverConfig,
) -> bool {
    if config.token.is_empty() {
        return true;
    }
    if check_token(headers, config) {
        return true;
    }
    params
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .and_then(|s| s.strip_prefix("token:"))
        .map(|t| constant_time_eq(t, &config.token))
        .unwrap_or(false)
}

/// 处理单个 JSON-RPC 调用，返回完整响应对象（含 `id` + `result`/`error`）。
async fn dispatch_rpc_call(
    call: &Value,
    tx: &mpsc::Sender<DownloadRequest>,
    config: &HttpTakeoverConfig,
    headers: &HashMap<String, String>,
) -> Value {
    let id = call.get("id").cloned().unwrap_or(Value::Null);
    let method = call.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = call.get("params").cloned().unwrap_or(Value::Array(vec![]));

    if !jsonrpc_token_ok(&params, headers, config) {
        return rpc_err(&id, 1, "Unauthorized: invalid token");
    }

    if method == "system.multicall" {
        return system_multicall(&id, &params, tx).await;
    }
    dispatch_method(method, &params, &id, tx).await
}

/// 派发单个 aria2 方法（不含 `system.multicall`，避免异步递归）。
async fn dispatch_method(
    method: &str,
    params: &Value,
    id: &Value,
    tx: &mpsc::Sender<DownloadRequest>,
) -> Value {
    match method {
        "aria2.addUri" => match aria2_add_uri_to_download_request(params) {
            Ok(dl) => {
                let gid = pseudo_gid(&dl.url);
                match tx.send(dl).await {
                    Ok(()) => rpc_ok(id, Value::String(gid)),
                    Err(_) => rpc_err(id, 1, "app shutting down"),
                }
            }
            Err(e) => rpc_err(id, -32602, &e),
        },
        // 返回一个真实存在的 aria2 版本号以通过各客户端的连通性/版本检测。
        "aria2.getVersion" => rpc_ok(
            id,
            json!({
                "version": "1.37.0",
                "enabledFeatures": [
                    "Async DNS", "BitTorrent", "Firefox3 Cookie", "GZip",
                    "HTTPS", "Message Digest", "Metalink", "XML-RPC"
                ],
            }),
        ),
        // 不暴露真实统计；返回占位以满足客户端的「连通性探测」。
        "aria2.getGlobalStat" => rpc_ok(
            id,
            json!({
                "downloadSpeed": "0", "uploadSpeed": "0",
                "numActive": "0", "numWaiting": "0", "numStopped": "0",
            }),
        ),
        "system.listMethods" => rpc_ok(
            id,
            json!([
                "aria2.addUri",
                "aria2.getVersion",
                "aria2.getGlobalStat",
                "system.multicall",
                "system.listMethods"
            ]),
        ),
        other => rpc_err(id, -32601, &format!("Method not found: {other}")),
    }
}

/// 实现 `system.multicall`：`params = [ [ {methodName, params}, ... ] ]`。
/// 每个子调用的成功结果按 aria2 约定包裹成单元素数组。
async fn system_multicall(id: &Value, params: &Value, tx: &mpsc::Sender<DownloadRequest>) -> Value {
    let calls = match params
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_array())
    {
        Some(c) => c,
        None => return rpc_err(id, -32602, "system.multicall expects an array of calls"),
    };

    let mut results = Vec::with_capacity(calls.len());
    for c in calls {
        let method = c.get("methodName").and_then(|m| m.as_str()).unwrap_or("");
        // 禁止嵌套 multicall（aria2 行为一致）。
        if method == "system.multicall" {
            results.push(json!({ "code": -32600, "message": "nested multicall not allowed" }));
            continue;
        }
        let inner_params = c.get("params").cloned().unwrap_or(Value::Array(vec![]));
        let resp = dispatch_method(method, &inner_params, &Value::Null, tx).await;
        if let Some(result) = resp.get("result") {
            results.push(json!([result]));
        } else {
            results.push(resp.get("error").cloned().unwrap_or(Value::Null));
        }
    }
    rpc_ok(id, Value::Array(results))
}

fn rpc_ok(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn rpc_err(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// 由稳定输入派生一个 16 字符十六进制占位 GID（无需随机源）。
fn pseudo_gid(seed: &str) -> String {
    // FNV-1a 64-bit hash → 16 hex chars，格式上贴近 aria2 的 GID。
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in seed.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// 把 `aria2.addUri` 的 params 翻译为 [`DownloadRequest`]。
///
/// `params = [ "token:xxx"?, [uris...], { options }? ]`
///
/// 支持的 options：`dir`(忽略，保存目录由弹框/全局决定)、`out`→filename、
/// `referer`/`referrer`→referrer、`header`(字符串数组)→Cookie/Referer/User-Agent/其它头。
fn aria2_add_uri_to_download_request(params: &Value) -> Result<DownloadRequest, String> {
    let arr = params.as_array().ok_or("params must be an array")?;

    // 跳过可能存在的 "token:xxx" 前缀参数。
    let mut idx = 0;
    if let Some(first) = arr.first().and_then(|v| v.as_str())
        && first.starts_with("token:")
    {
        idx = 1;
    }

    let uris = arr
        .get(idx)
        .and_then(|v| v.as_array())
        .ok_or("first param (after optional token) must be a uris array")?;
    let joined = uris
        .iter()
        .filter_map(|u| u.as_str())
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if joined.is_empty() {
        return Err("at least one URI is required".to_string());
    }

    let options = arr.get(idx + 1).and_then(|v| v.as_object());

    let mut filename = String::new();
    let mut referrer = String::new();
    let mut cookies = String::new();
    let mut extra_headers: HashMap<String, String> = HashMap::new();

    if let Some(opts) = options {
        if let Some(out) = opts.get("out").and_then(|v| v.as_str()) {
            filename = out.to_string();
        }
        if let Some(r) = opts
            .get("referer")
            .or_else(|| opts.get("referrer"))
            .and_then(|v| v.as_str())
        {
            referrer = r.to_string();
        }
        // aria2 的 header 是字符串数组，每项形如 "Name: value"。
        if let Some(headers) = opts.get("header").and_then(|v| v.as_array()) {
            for h in headers.iter().filter_map(|x| x.as_str()) {
                if let Some((name, value)) = h.split_once(':') {
                    let name = name.trim();
                    let value = value.trim();
                    match name.to_ascii_lowercase().as_str() {
                        "cookie" => cookies = value.to_string(),
                        "referer" | "referrer" => {
                            if referrer.is_empty() {
                                referrer = value.to_string();
                            }
                        }
                        _ => {
                            extra_headers.insert(name.to_string(), value.to_string());
                        }
                    }
                }
            }
        }
    }

    Ok(DownloadRequest {
        url: joined,
        filename,
        referrer,
        cookies,
        headers: if extra_headers.is_empty() {
            None
        } else {
            Some(extra_headers)
        },
        file_size: None,
        mime_type: None,
        method: None,
        body: None,
    })
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_from_empty_map() {
        let cfg = HttpTakeoverConfig::from_config_map(&HashMap::new());
        assert!(cfg.enabled); // 默认启用
        assert_eq!(cfg.port, 17800);
        assert!(cfg.token.is_empty());
        assert_eq!(cfg.bind_addr().ip().to_string(), "127.0.0.1");
    }

    #[test]
    fn config_from_full_map() {
        let mut m = HashMap::new();
        m.insert("local_server_enabled".to_string(), "false".to_string());
        m.insert("local_server_port".to_string(), "9999".to_string());
        m.insert("local_server_token".to_string(), "secret".to_string());
        let cfg = HttpTakeoverConfig::from_config_map(&m);
        assert!(!cfg.enabled);
        assert_eq!(cfg.port, 9999);
        assert_eq!(cfg.token, "secret");
    }

    #[test]
    fn parse_head_basic() {
        let head = "POST /download?x=1 HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 5\r\nX-FluxDown-Client: userscript";
        let (method, path, headers) = parse_request_head(head).unwrap();
        assert_eq!(method, "POST");
        assert_eq!(path, "/download?x=1");
        assert_eq!(headers.get("host").unwrap(), "127.0.0.1");
        assert_eq!(headers.get("content-length").unwrap(), "5");
        // 大小写不敏感
        assert_eq!(headers.get("x-fluxdown-client").unwrap(), "userscript");
    }

    #[test]
    fn path_only_strips_query() {
        assert_eq!(path_only("/download?token=abc"), "/download");
        assert_eq!(path_only("/ping"), "/ping");
    }

    #[test]
    fn find_subsequence_works() {
        assert_eq!(find_subsequence(b"abc\r\n\r\ndef", b"\r\n\r\n"), Some(3));
        assert_eq!(find_subsequence(b"abcdef", b"\r\n\r\n"), None);
    }

    #[test]
    fn constant_time_eq_correct() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "abcd"));
    }

    #[test]
    fn takeover_auth_requires_client_header() {
        let cfg = HttpTakeoverConfig {
            enabled: true,
            port: 17800,
            token: String::new(),
        };
        let mut headers = HashMap::new();
        // 缺 client 头 → 403
        assert!(check_takeover_auth(&headers, &cfg).is_err());
        headers.insert("x-fluxdown-client".to_string(), "userscript".to_string());
        assert!(check_takeover_auth(&headers, &cfg).is_ok());
    }

    #[test]
    fn takeover_auth_token_enforced() {
        let cfg = HttpTakeoverConfig {
            enabled: true,
            port: 17800,
            token: "s3cr3t".to_string(),
        };
        let mut headers = HashMap::new();
        headers.insert("x-fluxdown-client".to_string(), "userscript".to_string());
        // 有 client 头但缺 token → 401
        assert!(check_takeover_auth(&headers, &cfg).is_err());
        headers.insert("x-fluxdown-token".to_string(), "wrong".to_string());
        assert!(check_takeover_auth(&headers, &cfg).is_err());
        headers.insert("x-fluxdown-token".to_string(), "s3cr3t".to_string());
        assert!(check_takeover_auth(&headers, &cfg).is_ok());
    }

    #[test]
    fn parse_download_request_json() {
        let json = br#"{"url":"https://example.com/file.zip","filename":"file.zip","referrer":"https://example.com/","cookies":"a=b","fileSize":1024,"headers":{"Authorization":"Bearer x"}}"#;
        let dl: DownloadRequest = serde_json::from_slice(json).unwrap();
        assert_eq!(dl.url, "https://example.com/file.zip");
        assert_eq!(dl.filename, "file.zip");
        assert_eq!(dl.cookies, "a=b");
        assert_eq!(dl.file_size, Some(1024));
        assert_eq!(
            dl.headers.unwrap().get("Authorization").unwrap(),
            "Bearer x"
        );
    }

    #[test]
    fn batch_urls_form() {
        let body = br#"{"urls":["https://a.com/1.zip","https://b.com/2.zip"],"referrer":"https://p.com/","cookies":"s=1"}"#;
        let dl = parse_batch(body).unwrap();
        assert_eq!(dl.url, "https://a.com/1.zip\nhttps://b.com/2.zip");
        assert_eq!(dl.referrer, "https://p.com/");
        assert_eq!(dl.cookies, "s=1");
    }

    #[test]
    fn batch_items_form() {
        let body = br#"{"items":[{"url":"https://a.com/1.zip","cookies":"s=1"},{"url":"https://b.com/2.zip"}]}"#;
        let dl = parse_batch(body).unwrap();
        assert_eq!(dl.url, "https://a.com/1.zip\nhttps://b.com/2.zip");
        assert_eq!(dl.cookies, "s=1");
    }

    #[test]
    fn batch_rejects_empty() {
        assert!(parse_batch(br#"{"urls":[]}"#).is_err());
        assert!(parse_batch(br#"{}"#).is_err());
    }

    #[test]
    fn aria2_add_uri_basic() {
        let params = serde_json::json!([
            ["https://example.com/file.zip"],
            { "out": "renamed.zip", "header": ["Cookie: a=b", "Referer: https://example.com/", "User-Agent: UA/1.0"] }
        ]);
        let dl = aria2_add_uri_to_download_request(&params).unwrap();
        assert_eq!(dl.url, "https://example.com/file.zip");
        assert_eq!(dl.filename, "renamed.zip");
        assert_eq!(dl.cookies, "a=b");
        assert_eq!(dl.referrer, "https://example.com/");
        assert_eq!(dl.headers.unwrap().get("User-Agent").unwrap(), "UA/1.0");
    }

    #[test]
    fn aria2_add_uri_strips_token_param() {
        let params = serde_json::json!(["token:mysecret", ["https://example.com/file.zip"]]);
        let dl = aria2_add_uri_to_download_request(&params).unwrap();
        assert_eq!(dl.url, "https://example.com/file.zip");
    }

    #[test]
    fn aria2_add_uri_requires_uri() {
        let params = serde_json::json!([[]]);
        assert!(aria2_add_uri_to_download_request(&params).is_err());
    }

    #[test]
    fn pseudo_gid_is_stable_hex() {
        let g = pseudo_gid("https://example.com/file.zip");
        assert_eq!(g.len(), 16);
        assert!(g.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(g, pseudo_gid("https://example.com/file.zip")); // 稳定
    }

    #[test]
    fn rpc_ok_err_shape() {
        let ok = rpc_ok(&Value::String("qwer".into()), Value::String("gid".into()));
        assert_eq!(ok["jsonrpc"], "2.0");
        assert_eq!(ok["id"], "qwer");
        assert_eq!(ok["result"], "gid");
        let err = rpc_err(&Value::Null, -32601, "boom");
        assert_eq!(err["error"]["code"], -32601);
        assert_eq!(err["error"]["message"], "boom");
    }

    #[test]
    fn jsonrpc_token_via_param_or_header() {
        let cfg = HttpTakeoverConfig {
            enabled: true,
            port: 17800,
            token: "S".to_string(),
        };
        // 空 token 配置 → 永远放行
        let open = HttpTakeoverConfig {
            enabled: true,
            port: 17800,
            token: String::new(),
        };
        assert!(jsonrpc_token_ok(
            &Value::Array(vec![]),
            &HashMap::new(),
            &open
        ));
        // params[0] = "token:S"
        let params = serde_json::json!(["token:S", ["https://x/f.zip"]]);
        assert!(jsonrpc_token_ok(&params, &HashMap::new(), &cfg));
        // 错误 token
        let bad = serde_json::json!(["token:WRONG"]);
        assert!(!jsonrpc_token_ok(&bad, &HashMap::new(), &cfg));
        // header token
        let mut headers = HashMap::new();
        headers.insert("x-fluxdown-token".to_string(), "S".to_string());
        assert!(jsonrpc_token_ok(&Value::Array(vec![]), &headers, &cfg));
    }

    #[tokio::test]
    async fn dispatch_get_version() {
        let (tx, _rx) = mpsc::channel::<DownloadRequest>(1);
        let resp = dispatch_method("aria2.getVersion", &Value::Null, &Value::from(1), &tx).await;
        assert_eq!(resp["result"]["version"], "1.37.0");
    }

    #[tokio::test]
    async fn dispatch_add_uri_sends_to_channel() {
        let (tx, mut rx) = mpsc::channel::<DownloadRequest>(4);
        let params = serde_json::json!([["https://example.com/f.zip"], { "out": "f.zip" }]);
        let resp = dispatch_method("aria2.addUri", &params, &Value::from("id1"), &tx).await;
        assert!(resp["result"].is_string());
        let dl = rx.recv().await.unwrap();
        assert_eq!(dl.url, "https://example.com/f.zip");
        assert_eq!(dl.filename, "f.zip");
    }

    #[tokio::test]
    async fn multicall_dispatches_each() {
        let (tx, mut rx) = mpsc::channel::<DownloadRequest>(8);
        let params = serde_json::json!([[
            { "methodName": "aria2.addUri", "params": [["https://a/1.zip"]] },
            { "methodName": "aria2.getVersion", "params": [] }
        ]]);
        let resp = system_multicall(&Value::from(1), &params, &tx).await;
        let arr = resp["result"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        // 第一项：addUri 成功 → 单元素数组
        assert!(arr[0].is_array());
        // 第二项：getVersion 成功 → 单元素数组包 version 对象
        assert!(arr[1][0]["version"].is_string());
        let dl = rx.recv().await.unwrap();
        assert_eq!(dl.url, "https://a/1.zip");
    }

    #[tokio::test]
    async fn dispatch_unknown_method() {
        let (tx, _rx) = mpsc::channel::<DownloadRequest>(1);
        let resp = dispatch_method("aria2.removeXyz", &Value::Null, &Value::Null, &tx).await;
        assert_eq!(resp["error"]["code"], -32601);
    }

    // ---- 真实 TCP socket 集成测试 ----

    /// 启动一个临时服务器，返回 (实际地址字符串, 下载请求接收端)。
    async fn start_test_server(token: &str) -> (String, mpsc::Receiver<DownloadRequest>) {
        let (tx, rx) = mpsc::channel::<DownloadRequest>(8);
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let cfg = HttpTakeoverConfig {
            enabled: true,
            port: addr.port(),
            token: token.to_string(),
        };
        tokio::spawn(run_accept_loop(listener, tx, cfg));
        (addr.to_string(), rx)
    }

    /// 发送一个原始 HTTP 请求，返回完整响应文本。
    async fn raw_request(addr: &str, request: &str) -> String {
        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(request.as_bytes()).await.unwrap();
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).await.unwrap();
        String::from_utf8_lossy(&buf).into_owned()
    }

    fn post(path: &str, headers: &str, body: &str) -> String {
        format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    #[tokio::test]
    async fn integration_ping() {
        let (addr, _rx) = start_test_server("").await;
        let resp = raw_request(
            &addr,
            "GET /ping HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(resp.contains("200 OK"), "resp={resp}");
        assert!(resp.contains("\"pong\""));
        assert!(resp.contains("FluxDown"));
    }

    #[tokio::test]
    async fn integration_download_accepted() {
        let (addr, mut rx) = start_test_server("").await;
        let body = r#"{"url":"https://example.com/f.zip","filename":"f.zip","cookies":"a=b"}"#;
        let req = post(
            "/download",
            "X-FluxDown-Client: userscript\r\nContent-Type: application/json\r\n",
            body,
        );
        let resp = raw_request(&addr, &req).await;
        assert!(resp.contains("200 OK"), "resp={resp}");
        assert!(resp.contains("download accepted"));
        let dl = rx.recv().await.unwrap();
        assert_eq!(dl.url, "https://example.com/f.zip");
        assert_eq!(dl.filename, "f.zip");
        assert_eq!(dl.cookies, "a=b");
    }

    #[tokio::test]
    async fn integration_download_rejected_without_client_header() {
        let (addr, _rx) = start_test_server("").await;
        let body = r#"{"url":"https://evil.example/x.zip"}"#;
        // 缺少 X-FluxDown-Client（模拟恶意网页的简单跨域 POST）→ 403
        let req = post("/download", "Content-Type: application/json\r\n", body);
        let resp = raw_request(&addr, &req).await;
        assert!(resp.contains("403 Forbidden"), "resp={resp}");
    }

    #[tokio::test]
    async fn integration_download_rejected_bad_token() {
        let (addr, _rx) = start_test_server("S3CRET").await;
        let body = r#"{"url":"https://example.com/x.zip"}"#;
        let req = post(
            "/download",
            "X-FluxDown-Client: userscript\r\nX-FluxDown-Token: wrong\r\nContent-Type: application/json\r\n",
            body,
        );
        let resp = raw_request(&addr, &req).await;
        assert!(resp.contains("401 Unauthorized"), "resp={resp}");
    }

    #[tokio::test]
    async fn integration_jsonrpc_add_uri() {
        let (addr, mut rx) = start_test_server("").await;
        let body = r#"{"jsonrpc":"2.0","id":"1","method":"aria2.addUri","params":[["https://a.com/v.mp4"],{"out":"v.mp4","header":["Cookie: s=1","Referer: https://a.com/"]}]}"#;
        let req = post("/jsonrpc", "Content-Type: application/json\r\n", body);
        let resp = raw_request(&addr, &req).await;
        assert!(resp.contains("200 OK"), "resp={resp}");
        assert!(resp.contains("\"result\""));
        let dl = rx.recv().await.unwrap();
        assert_eq!(dl.url, "https://a.com/v.mp4");
        assert_eq!(dl.filename, "v.mp4");
        assert_eq!(dl.cookies, "s=1");
        assert_eq!(dl.referrer, "https://a.com/");
    }

    #[tokio::test]
    async fn integration_jsonrpc_batch_array() {
        // gofile-enhanced 形态：顶层 JSON 数组。
        let (addr, mut rx) = start_test_server("").await;
        let body = r#"[{"jsonrpc":"2.0","id":"a","method":"aria2.addUri","params":[["https://a/1.zip"]]},{"jsonrpc":"2.0","id":"b","method":"aria2.addUri","params":[["https://b/2.zip"]]}]"#;
        let req = post("/jsonrpc", "Content-Type: application/json\r\n", body);
        let resp = raw_request(&addr, &req).await;
        assert!(resp.contains("200 OK"), "resp={resp}");
        let r1 = rx.recv().await.unwrap();
        let r2 = rx.recv().await.unwrap();
        let mut urls = [r1.url, r2.url];
        urls.sort();
        assert_eq!(urls, ["https://a/1.zip", "https://b/2.zip"]);
    }

    #[tokio::test]
    async fn integration_jsonrpc_accepts_non_json_content_type() {
        // 兼容性回归：panlinker 等 aria2 风格脚本经 GM_xmlhttpRequest 发送时
        // 默认不带 `application/json` 头（常为 text/plain）。/jsonrpc 不应据此拒绝，
        // 只要请求体是合法 JSON-RPC 即正常处理（与真实 aria2 行为一致）。
        let (addr, mut rx) = start_test_server("").await;
        let body = r#"{"jsonrpc":"2.0","id":"1","method":"aria2.addUri","params":["token:",["https://a.com/v.mp4"],{"out":"v.mp4"}]}"#;
        let req = post(
            "/jsonrpc",
            "Content-Type: text/plain;charset=UTF-8\r\n",
            body,
        );
        let resp = raw_request(&addr, &req).await;
        assert!(resp.contains("200 OK"), "resp={resp}");
        assert!(resp.contains("\"result\""), "resp={resp}");
        let dl = rx.recv().await.unwrap();
        assert_eq!(dl.url, "https://a.com/v.mp4");
    }

    #[tokio::test]
    async fn integration_jsonrpc_rejects_non_json_body() {
        // 准入门槛仍在：非 JSON 请求体应被拒绝（-32700 解析错误）。
        let (addr, _rx) = start_test_server("").await;
        let req = post(
            "/jsonrpc",
            "Content-Type: text/plain\r\n",
            "not json at all",
        );
        let resp = raw_request(&addr, &req).await;
        assert!(resp.contains("\"error\""), "resp={resp}");
        assert!(resp.contains("-32700"), "resp={resp}");
    }

    #[tokio::test]
    async fn integration_batch_endpoint() {
        let (addr, mut rx) = start_test_server("").await;
        let body = r#"{"urls":["https://a/1.zip","https://b/2.zip"],"referrer":"https://p/"}"#;
        let req = post(
            "/download/batch",
            "X-FluxDown-Client: userscript\r\nContent-Type: application/json\r\n",
            body,
        );
        let resp = raw_request(&addr, &req).await;
        assert!(resp.contains("200 OK"), "resp={resp}");
        let dl = rx.recv().await.unwrap();
        assert_eq!(dl.url, "https://a/1.zip\nhttps://b/2.zip");
        assert_eq!(dl.referrer, "https://p/");
    }

    #[tokio::test]
    async fn integration_unknown_path_404() {
        let (addr, _rx) = start_test_server("").await;
        let resp = raw_request(
            &addr,
            "GET /nope HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(resp.contains("404 Not Found"), "resp={resp}");
    }
}
