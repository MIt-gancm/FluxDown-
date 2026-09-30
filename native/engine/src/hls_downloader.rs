//! HLS (HTTP Live Streaming) download engine.
//!
//! Fetches M3U8 playlists, downloads all segments with bounded concurrency,
//! optionally decrypts AES-128-CBC encrypted segments, and merges them into a
//! single output file — MPEG-TS (`.ts`) for classic playlists, or a
//! fragmented MP4 (`.mp4`) for fMP4/CMAF playlists that carry an
//! EXT-X-MAP initialization segment (#682).
//!
//! Architecture:
//! - Master playlist → auto-select highest bandwidth variant
//! - Media playlist → bounded-concurrency segment download with cancellation.
//!   Each segment downloads + decrypts on its own task (permit-gated by a
//!   `Semaphore`); a single writer drains finished segments in `seg_idx` order
//!   so the on-disk byte stream matches the sequential implementation exactly.
//! - EXT-X-MAP (fMP4/CMAF) init segments are prefetched once per distinct
//!   `(uri, byte_range)` and prepended by the writer whenever the sticky map
//!   in effect changes (see `should_write_init`)
//! - AES-128-CBC decryption with shared key caching
//! - Progress reporting via ProgressUpdate channel (writer-side, so byte counts
//!   never double-count under concurrency)
//! - Per-segment retry with exponential backoff

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::Client;
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, OnceCell, OwnedSemaphorePermit, Semaphore, mpsc};

use crate::downloader::{
    DB_SAVE_INTERVAL_SECS, DownloadError, DownloadParams, ProgressUpdate, TEMP_EXT, dedup_filename,
    extract_from_url, sanitize_filename,
};
use crate::events::EventSink;
use crate::logger::log_info;
use crate::model::HlsQualityOption;
use crate::output;
use crate::selection::SelectionOutcome;
use crate::transfer_activity::{TaskRuntime, TransferTracker};

fn hls_runtime(task_id: &str, tracker: &TransferTracker, limit: u32) -> TaskRuntime {
    TaskRuntime {
        task_id: task_id.to_owned(),
        sampled_at_ms: chrono::Utc::now().timestamp_millis(),
        sample_sequence: crate::transfer_activity::next_sample_sequence(),
        active_transfers: Some(tracker.active()),
        connected_peers: None,
        parallelism_limit: Some(limit),
        total_bytes: 0,
        segments: Vec::new(),
        source_bytes: None,
    }
}

// ---------------------------------------------------------------------------
// Same-origin check for cookie safety
// ---------------------------------------------------------------------------

fn is_same_origin(base_url: &str, target_url: &str) -> bool {
    let base = match url::Url::parse(base_url) {
        Ok(u) => u,
        Err(_) => return false,
    };
    let target = match url::Url::parse(target_url) {
        Ok(u) => u,
        Err(_) => return false,
    };
    base.scheme() == target.scheme()
        && base.host_str() == target.host_str()
        && base.port_or_known_default() == target.port_or_known_default()
}

fn cookies_for_url<'a>(playlist_url: &str, target_url: &str, cookies: &'a str) -> &'a str {
    if cookies.is_empty() {
        return "";
    }
    if is_same_origin(playlist_url, target_url) {
        cookies
    } else {
        ""
    }
}

/// 清单主机之外的请求不得携带的凭据类头。Cookie 另有独立的同源过滤
/// (`cookies_for_url`),扩展捕获的 Authorization/Cookie 与站点 Basic 凭据
/// 同样只属于用户提供的清单源站,分片/密钥/子清单指向第三方主机时不能带过去。
fn is_credential_header(name: &str) -> bool {
    ["authorization", "cookie", "proxy-authorization"]
        .iter()
        .any(|h| name.eq_ignore_ascii_case(h))
}

/// 目标与清单不同源时剔除凭据类头,同源(或本就没有凭据头)时零拷贝借用。
pub(crate) fn extra_headers_for_origin<'a>(
    manifest_url: &str,
    target_url: &str,
    headers: &'a HashMap<String, String>,
) -> Cow<'a, HashMap<String, String>> {
    if !headers.keys().any(|k| is_credential_header(k)) || is_same_origin(manifest_url, target_url)
    {
        return Cow::Borrowed(headers);
    }
    Cow::Owned(
        headers
            .iter()
            .filter(|(k, _)| !is_credential_header(k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    )
}

/// 小请求(playlist / 密钥 / MPD)的整体超时:响应体只有几 KB~几 MB,30s 足够;
/// 段 body 不能用整体超时(单段可达 256MB),见 `SEGMENT_IDLE_TIMEOUT`。
pub(crate) const SMALL_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// 段请求等待响应头的上限。
pub(crate) const SEGMENT_HEADER_TIMEOUT: Duration = Duration::from_secs(30);
/// 段 body 逐 chunk 空闲上限:比 HTTP 路径的 5/10s 宽松,HLS 常见限速 CDN。
pub(crate) const SEGMENT_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// 拉取小响应体,并与取消、超时竞速;返回(重定向后的最终 URL, 响应体)。
/// 不在共享 client 上设全局 timeout,否则会误伤普通 HTTP 长下载。
pub(crate) async fn fetch_small(
    req: reqwest::RequestBuilder,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<(String, bytes::Bytes), DownloadError> {
    let work = async {
        let resp = req.send().await?.error_for_status()?;
        let final_url = resp.url().to_string();
        let body = resp.bytes().await?;
        Ok::<_, DownloadError>((final_url, body))
    };
    tokio::select! {
        _ = cancel.cancelled() => Err(DownloadError::Cancelled),
        r = tokio::time::timeout(SMALL_REQUEST_TIMEOUT, work) => match r {
            Ok(r) => r,
            Err(_) => Err(DownloadError::Other(format!(
                "request timed out after {}s",
                SMALL_REQUEST_TIMEOUT.as_secs()
            ))),
        },
    }
}

/// 一次 HLS 请求的公共上下文:Cookie 按播放列表同源过滤,凭据类头按用户
/// 原始清单源站过滤,Referer 对所有请求附加(防盗链 CDN 缺失即 403)。
#[derive(Clone, Copy)]
struct HlsRequestCtx<'a> {
    cookies: &'a str,
    /// Cookie 同源判定基准(实际列出该资源的 playlist URL)。
    cookie_base_url: &'a str,
    /// 凭据类请求头的同源判定基准(用户提交的原始清单 URL)。
    header_origin_url: &'a str,
    referrer: &'a str,
    extra_headers: &'a HashMap<String, String>,
}

impl HlsRequestCtx<'_> {
    fn get(&self, client: &Client, url: &str) -> reqwest::RequestBuilder {
        let mut req = client.get(url);
        let safe_cookies = cookies_for_url(self.cookie_base_url, url, self.cookies);
        if !safe_cookies.is_empty() {
            req = req.header("Cookie", safe_cookies);
        }
        // 放在 apply_extra_headers 之前:扩展捕获的真实 Referer 会覆盖默认值。
        if crate::downloader::is_valid_referrer(self.referrer) {
            req = req.header(reqwest::header::REFERER, self.referrer);
        }
        let headers = extra_headers_for_origin(self.header_origin_url, url, self.extra_headers);
        crate::downloader::apply_extra_headers(req, &headers)
    }
}

/// 随作用域结束(含 panic 展开)中止后台任务,避免采样器泄漏。
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl AbortOnDrop {
    fn abort(&self) {
        self.0.abort();
    }
}

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MAX_RETRIES: u32 = 3;
const RETRY_BASE_DELAY: std::time::Duration = std::time::Duration::from_secs(2);

/// Upper bound on concurrent segment downloads.
///
/// Capped at 16 to bound CDN per-IP connection pressure: HLS playlists often
/// have hundreds of tiny segments, and opening dozens of parallel connections
/// to a single streaming CDN risks tripping per-IP limits or throttling.
/// This ceiling is intentionally independent of `build_client`'s idle-pool
/// size (`pool_max_idle_per_host`, sized for the 64-segment HTTP path) —
/// the pool is large enough to keep every HLS connection warm regardless.
const MAX_HLS_CONCURRENCY: usize = 16;

/// Concurrency used when the user left the segment count on "auto"
/// (`segment_count <= 0`). Conservative enough to help every playlist without
/// hammering small CDNs.
const DEFAULT_HLS_CONCURRENCY: usize = 8;

/// Pick the number of segments to download in parallel.
///
/// Derived from the user-configured `segment_count` (the same knob the
/// multi-segment HTTP downloader uses), clamped to `[1, MAX_HLS_CONCURRENCY]`
/// and never exceeding the number of segments actually left to download.
/// `segment_count <= 0` means "auto" → `DEFAULT_HLS_CONCURRENCY`.
fn hls_concurrency(segment_count: i32, remaining_segments: usize) -> usize {
    let requested = if segment_count <= 0 {
        DEFAULT_HLS_CONCURRENCY
    } else {
        segment_count as usize
    };
    requested
        .clamp(1, MAX_HLS_CONCURRENCY)
        .min(remaining_segments.max(1))
}

pub(crate) fn force_ts_extension(name: &str) -> String {
    if let Some(dot_pos) = name.rfind('.') {
        format!("{}.ts", &name[..dot_pos])
    } else {
        format!("{}.ts", name)
    }
}

// ---------------------------------------------------------------------------
// HLS URL detection
// ---------------------------------------------------------------------------

/// Check if a URL points to an HLS manifest (`.m3u8` or `.m3u` extension).
/// Case-insensitive, ignores query parameters and fragments.
pub fn is_hls_url(url: &str) -> bool {
    let path = url.split('?').next().unwrap_or(url);
    let path = path.split('#').next().unwrap_or(path);
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".m3u8") || lower.ends_with(".m3u")
}

// ---------------------------------------------------------------------------
// HLS types
// ---------------------------------------------------------------------------

/// Parsed M3U8 content — either a master playlist or a media playlist.
#[allow(dead_code)]
pub enum M3u8Content {
    Master {
        variants: Vec<HlsVariant>,
    },
    Media {
        segments: Vec<HlsSegment>,
        total_duration: f32,
        media_sequence: u64,
        /// 播放列表中是否存在 EXT-X-MAP(fMP4/CMAF 初始化段)。`true` 时输出
        /// 是分片 MP4(ftyp+moov+[moof+mdat]*)而非 MPEG-TS,下游需相应改变
        /// 输出扩展名并跳过 TS→MP4 remux(#682)。
        has_map: bool,
        /// 播放列表是否为直播/未结束(无 EXT-X-ENDLIST 且非 VOD 类型)。
        live: bool,
    },
}

/// A variant stream from a master playlist.
pub struct HlsVariant {
    pub bandwidth: u64,
    pub resolution: Option<(u64, u64)>,
    pub uri: String,
    /// 该变体的音频组(`AUDIO="group"`)指向独立 URI 的 EXT-X-MEDIA 音轨时
    /// 的绝对 URI;`None` 表示音频已内嵌在变体流里。
    pub audio_uri: Option<String>,
}

/// A single segment from a media playlist.
#[allow(dead_code)]
pub struct HlsSegment {
    pub uri: String,
    pub duration: f32,
    pub key: Option<HlsKey>,
    /// EXT-X-BYTERANGE 子区间 `(offset, length)`(字节)。`None` 表示整段就是
    /// 整个 `uri` 资源;`Some` 表示该段只是 `uri` 的一个子区间,下载时必须发
    /// `Range: bytes=offset-(offset+length-1)` 头并要求 206,否则多个段会各自
    /// 下载整文件、拼出 N 份完整副本(巨量损坏 + 撑爆磁盘)。
    pub byte_range: Option<(u64, u64)>,
    /// 本段是否紧跟 EXT-X-DISCONTINUITY(与前一段存在不连续点)。当前仅解析
    /// 并保留该标志;隐式 IV 仍按 RFC 8216 用绝对 Media Sequence Number 计算
    /// (见 `compute_default_iv` 注释),不连续点不重置该序号。
    pub discontinuity: bool,
    /// 本段生效的 EXT-X-MAP 初始化段(fMP4/CMAF)。RFC 8216 §4.3.2.4 规定该
    /// tag 粘性生效直到下一个 EXT-X-MAP 或播放列表结束,故同一 `HlsMap` 会
    /// 被多个连续段共享(#682)。`None` 表示 MPEG-TS 段,无需初始化段。
    pub map: Option<HlsMap>,
}

/// Encryption key info for a segment.
pub struct HlsKey {
    pub method: HlsKeyMethod,
    pub uri: String,
    pub iv: Option<String>,
}

/// Key encryption method.
#[derive(Clone, PartialEq, Eq)]
pub enum HlsKeyMethod {
    Aes128,
    None,
}

/// fMP4/CMAF 初始化段(EXT-X-MAP)。包含 `ftyp`+`moov` box,须在其粘性覆盖
/// 的首个媒体段之前写入输出文件一次;之后的媒体段只含 `moof`+`mdat`。
#[derive(Clone, PartialEq, Eq)]
pub struct HlsMap {
    pub uri: String,
    /// EXT-X-MAP 的 BYTERANGE 属性 `(offset, length)`。省略 offset 时视为 0
    /// ——不同于 EXT-X-BYTERANGE 段,init 段通常是独立小文件,标准未定义跨
    /// 多次出现的隐式偏移累计。`None` 表示整个 `uri` 资源就是初始化段。
    pub byte_range: Option<(u64, u64)>,
}

/// 初始化段去重键 `(uri, byte_range)`:同一 key 的 init 段只下载 / 写入一次。
type MapKey = (String, Option<(u64, u64)>);

// ---------------------------------------------------------------------------
// URI resolution
// ---------------------------------------------------------------------------

/// Resolve a possibly-relative URI against a base URL.
/// If `uri` starts with `http://` or `https://`, return as-is.
/// Otherwise, strip the path component after the last `/` from `base_url`
/// and append `uri`.
/// Resolve a possibly-relative URI against a base URL using RFC 3986 rules.
fn resolve_uri(base_url: &str, uri: &str) -> String {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        return uri.to_string();
    }

    match url::Url::parse(base_url) {
        Ok(base) => match base.join(uri) {
            Ok(resolved) => resolved.to_string(),
            Err(_) => {
                // Fallback: simple concatenation
                if let Some(last_slash) = base_url.rfind('/') {
                    format!("{}/{}", &base_url[..last_slash], uri)
                } else {
                    uri.to_string()
                }
            }
        },
        Err(_) => {
            if let Some(last_slash) = base_url.rfind('/') {
                format!("{}/{}", &base_url[..last_slash], uri)
            } else {
                uri.to_string()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// M3U8 parsing
// ---------------------------------------------------------------------------

/// Fetch and parse an M3U8 playlist from the given URL.
async fn parse_m3u8(
    client: &Client,
    url: &str,
    ctx: &HlsRequestCtx<'_>,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<M3u8Content, DownloadError> {
    let req = ctx.get(client, url);
    // 相对 URI 必须以"最终检索到的资源 URL"为 base 解析(RFC 3986 §5.1)。
    // reqwest 默认跟随重定向(见 downloader.rs),播放列表被负载均衡/短链
    // 重定向时,请求 url 与实际返回内容的 URL 不同;若仍用请求前的 url 作
    // base,会把相对段/密钥 URI 拼到错误的主机/路径。无重定向时
    // base_url == url,行为不变。与同仓 downloader.rs 既定做法对齐。
    let (base_url, bytes) = fetch_small(req, cancel).await?;

    parse_m3u8_bytes(&base_url, &bytes)
}

/// 去掉 UTF-8 BOM 与前导空白:m3u8_rs 要求首行就是 `#EXTM3U`,带 BOM 的
/// 合法播放列表否则会被误判为非播放列表。
fn trim_playlist_prefix(bytes: &[u8]) -> &[u8] {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    &bytes[start..]
}

/// 选出变体音频组里带独立 URI 的音轨(优先 DEFAULT=YES)。
fn external_audio_uri(
    alternatives: &[m3u8_rs::AlternativeMedia],
    group: &str,
    base_url: &str,
) -> Option<String> {
    let mut candidates = alternatives.iter().filter(|a| {
        matches!(a.media_type, m3u8_rs::AlternativeMediaType::Audio)
            && a.group_id == group
            && a.uri.as_deref().is_some_and(|u| !u.is_empty())
    });
    let first = candidates.next()?;
    let chosen = if first.default {
        first
    } else {
        candidates.find(|a| a.default).unwrap_or(first)
    };
    chosen.uri.as_deref().map(|u| resolve_uri(base_url, u))
}

/// Parse already-fetched M3U8 bytes against a resolved base URL.
///
/// Split out from [`parse_m3u8`] purely for unit-testability: the HTTP fetch
/// above needs a live server/mock to exercise, but the playlist-parsing logic
/// (segment/key/map resolution, byte-range accounting) is pure and worth
/// testing directly (see `mod tests`).
fn parse_m3u8_bytes(base_url: &str, bytes: &[u8]) -> Result<M3u8Content, DownloadError> {
    let bytes = trim_playlist_prefix(bytes);
    // 解析失败（含非法内容、缺 #EXTM3U 头）一律归为 NotAnHlsPlaylist——而
    // 非泛化的 Other——使调用方（`run_hls_download`）能与"内容合法但下游
    // 步骤失败"的真实 HLS 错误区分开，把误判为 HLS 的普通文件退回普通 HTTP
    // 下载器，而不是当作终态失败上报。
    let (_remaining, playlist) = m3u8_rs::parse_playlist(bytes)
        .map_err(|e| DownloadError::NotAnHlsPlaylist(format!("M3U8 parse error: {}", e)))?;

    match playlist {
        m3u8_rs::Playlist::MasterPlaylist(master) => {
            // I-frame 变体(EXT-X-I-FRAME-STREAM-INF)只含关键帧,是快进预览流,
            // 不能当画质选项,更不能参与"最高码率"自动选择。
            let variants: Vec<HlsVariant> = master
                .variants
                .iter()
                .filter(|v| !v.is_i_frame)
                .map(|v| {
                    let resolution = v.resolution.as_ref().map(|r| (r.width, r.height));
                    HlsVariant {
                        bandwidth: v.bandwidth,
                        resolution,
                        uri: resolve_uri(base_url, &v.uri),
                        audio_uri: v
                            .audio
                            .as_deref()
                            .and_then(|g| external_audio_uri(&master.alternatives, g, base_url)),
                    }
                })
                .collect();

            if variants.is_empty() {
                return Err(DownloadError::Other(
                    "M3U8 master playlist has no variants".to_string(),
                ));
            }

            Ok(M3u8Content::Master { variants })
        }
        m3u8_rs::Playlist::MediaPlaylist(media) => {
            let media_sequence = media.media_sequence;
            let live = !media.end_list
                && !matches!(media.playlist_type, Some(m3u8_rs::MediaPlaylistType::Vod));
            let mut total_duration: f32 = 0.0;
            let mut current_key: Option<HlsKey> = None;
            // EXT-X-MAP(fMP4/CMAF 初始化段,#682)按 RFC 8216 §4.3.2.4 对其后
            // 所有段粘性生效,直到下一个 EXT-X-MAP 或播放列表结束。m3u8_rs 的
            // 解析器只把 `map` 挂在"标签出现的那一段"上、逐段清空(其
            // parser.rs 里 `map = None` 与 `encryption_key = None` 同步发生),
            // 与 `key` 字段同样是非粘性的——因此这里手动维护 `current_map`
            // 跨段传播,与下方 `current_key` 的处理方式完全一致。
            //
            // 已知限制:若 EXT-X-MAP 之前出现过 EXT-X-KEY(RFC 8216 §4.3.2.4/
            // §4.3.2.5 规定此时 init 段也应使用该密钥解密),m3u8_rs 的 `Map`
            // 结构不携带密钥/IV 信息,本实现无法据此解密——init 段字节按原样
            // (未解密)写入输出。现实中的 fMP4/CMAF 分发绝大多数场景下 init
            // 段本身未加密(仅媒体段加密),此限制不影响这些场景。
            let mut current_map: Option<HlsMap> = None;
            let mut segments: Vec<HlsSegment> = Vec::with_capacity(media.segments.len());
            // EXT-X-BYTERANGE 省略 @offset 时,offset = 同一 uri 上一子区间结束
            // 位置+1(按出现顺序累计)。键为已解析的绝对段 URI,值为该 uri 上
            // "下一个隐式子区间的起始 offset"(即上一子区间的 offset+length)。
            let mut byterange_next_offset: HashMap<String, u64> = HashMap::new();

            for seg in &media.segments {
                total_duration += seg.duration;

                if let Some(m) = &seg.map {
                    let map_byte_range = m
                        .byte_range
                        .as_ref()
                        .map(|br| (br.offset.unwrap_or(0), br.length));
                    current_map = Some(HlsMap {
                        uri: resolve_uri(base_url, &m.uri),
                        byte_range: map_byte_range,
                    });
                }

                if let Some(key) = &seg.key {
                    current_key = match &key.method {
                        &m3u8_rs::KeyMethod::AES128 => {
                            let key_uri = match key.uri.as_ref() {
                                Some(u) if !u.is_empty() => resolve_uri(base_url, u),
                                _ => {
                                    return Err(DownloadError::Other(
                                        "AES-128 KEY tag missing URI".to_string(),
                                    ));
                                }
                            };
                            Some(HlsKey {
                                method: HlsKeyMethod::Aes128,
                                uri: key_uri,
                                iv: key.iv.clone(),
                            })
                        }
                        &m3u8_rs::KeyMethod::None => Some(HlsKey {
                            method: HlsKeyMethod::None,
                            uri: String::new(),
                            iv: None,
                        }),
                        other => {
                            return Err(DownloadError::Other(format!(
                                "unsupported HLS encryption method: {:?}",
                                other
                            )));
                        }
                    };
                }

                let seg_key = current_key.as_ref().and_then(|k| {
                    if k.method == HlsKeyMethod::Aes128 {
                        Some(HlsKey {
                            method: HlsKeyMethod::Aes128,
                            uri: k.uri.clone(),
                            iv: k.iv.clone(),
                        })
                    } else {
                        None
                    }
                });

                let resolved_uri = resolve_uri(base_url, &seg.uri);

                // EXT-X-BYTERANGE 解析:同一 uri 的多个段共享底层大文件的不同
                // 子区间。@offset 缺省时按出现顺序在该 uri 上累计(上一子区间
                // 结束位置)。offset+length 可能溢出 u64 → checked_add 报错而非
                // 回绕(回绕会请求错误区间、拼出损坏数据)。
                let byte_range = match &seg.byte_range {
                    Some(br) => {
                        let offset = match br.offset {
                            Some(o) => o,
                            None => byterange_next_offset
                                .get(&resolved_uri)
                                .copied()
                                .unwrap_or(0),
                        };
                        let next = offset.checked_add(br.length).ok_or_else(|| {
                            DownloadError::Other(format!(
                                "EXT-X-BYTERANGE offset+length overflow (offset={}, length={})",
                                offset, br.length
                            ))
                        })?;
                        byterange_next_offset.insert(resolved_uri.clone(), next);
                        Some((offset, br.length))
                    }
                    None => None,
                };

                segments.push(HlsSegment {
                    uri: resolved_uri,
                    duration: seg.duration,
                    key: seg_key,
                    byte_range,
                    discontinuity: seg.discontinuity,
                    map: current_map.clone(),
                });
            }

            if segments.is_empty() {
                return Err(DownloadError::Other(
                    "M3U8 media playlist has no segments".to_string(),
                ));
            }

            let has_map = segments.iter().any(|s| s.map.is_some());

            Ok(M3u8Content::Media {
                segments,
                total_duration,
                media_sequence,
                has_map,
                live,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// AES-128-CBC decryption
// ---------------------------------------------------------------------------

use aes::Aes128;
use cbc::cipher::block_padding::{NoPadding, Pkcs7};
use cbc::cipher::{BlockDecryptMut, KeyIvInit};

type Aes128CbcDec = cbc::Decryptor<Aes128>;

/// AES block size in bytes (AES-128-CBC operates on 16-byte blocks).
const AES_BLOCK_SIZE: usize = 16;

/// Shared AES-128 key cache: key URI → single-flight cell.
///
/// 外层 `Mutex` 只保护内存 HashMap 的查找/插入,绝不跨网络 I/O 持有;并发段
/// 拿到同一个 `OnceCell` 后由它保证同一 URI 只发一次请求(首批 8~16 个段同时
/// 未命中缓存时不会对限流的密钥服务重复请求)。失败不会写入 cell,下次调用重试。
type KeyCache = Arc<Mutex<HashMap<String, Arc<OnceCell<Vec<u8>>>>>>;

/// Fetch an AES-128 key from the given URI, with caching, single-flight and retry.
async fn fetch_key(
    client: &Client,
    key_uri: &str,
    ctx: &HlsRequestCtx<'_>,
    key_cache: &KeyCache,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Vec<u8>, DownloadError> {
    let cell = key_cache
        .lock()
        .await
        .entry(key_uri.to_string())
        .or_default()
        .clone();
    cell.get_or_try_init(|| fetch_key_with_retry(client, key_uri, ctx, cancel))
        .await
        .cloned()
}

async fn fetch_key_with_retry(
    client: &Client,
    key_uri: &str,
    ctx: &HlsRequestCtx<'_>,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Vec<u8>, DownloadError> {
    let mut attempts = 0u32;
    loop {
        match fetch_key_once(client, key_uri, ctx, cancel).await {
            Ok(key) => return Ok(key),
            Err(DownloadError::Cancelled) => return Err(DownloadError::Cancelled),
            Err(e) => {
                attempts += 1;
                if attempts >= MAX_RETRIES {
                    return Err(DownloadError::Other(format!(
                        "HLS key fetch failed after {} attempts: {}",
                        MAX_RETRIES, e
                    )));
                }
                log_info!(
                    "[hls-download] key {} attempt {}/{} failed: {}",
                    key_uri,
                    attempts,
                    MAX_RETRIES,
                    e
                );
                let delay = RETRY_BASE_DELAY * 2u32.saturating_pow(attempts - 1);
                tokio::select! {
                    _ = cancel.cancelled() => return Err(DownloadError::Cancelled),
                    _ = tokio::time::sleep(delay) => {}
                }
            }
        }
    }
}

async fn fetch_key_once(
    client: &Client,
    key_uri: &str,
    ctx: &HlsRequestCtx<'_>,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Vec<u8>, DownloadError> {
    let (_, body) = fetch_small(ctx.get(client, key_uri), cancel).await?;
    if body.len() != 16 {
        return Err(DownloadError::Other(format!(
            "AES-128 key must be 16 bytes, got {} bytes from {}",
            body.len(),
            key_uri
        )));
    }
    Ok(body.to_vec())
}

/// Parse an IV hex string (e.g. "0x1234abcd...") into 16 bytes.
fn parse_iv_hex(iv_str: &str) -> Result<[u8; 16], DownloadError> {
    let hex = iv_str
        .strip_prefix("0x")
        .or_else(|| iv_str.strip_prefix("0X"))
        .unwrap_or(iv_str);

    // 必须先确认全是 ASCII hex:下面按字节下标切片,多字节字符会切到字符中间 panic。
    if hex.len() != 32 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(DownloadError::Other(format!(
            "IV hex string must be 32 hex chars, got {}: {}",
            hex.len(),
            iv_str
        )));
    }

    let mut iv = [0u8; 16];
    for i in 0..16 {
        iv[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
            .map_err(|e| DownloadError::Other(format!("invalid IV hex: {}", e)))?;
    }
    Ok(iv)
}

/// 续传检查点里记录的所选变体标识。bandwidth+resolution+去 query 的 path:
/// 变体 URI 常带会过期的 CDN 签名 token,续传时不能按完整 URI 匹配。
#[derive(Debug, Clone, PartialEq, Eq)]
struct VariantId {
    bandwidth: u64,
    resolution: Option<(u64, u64)>,
    path: String,
}

impl VariantId {
    fn of(v: &HlsVariant) -> Self {
        let path = match url::Url::parse(&v.uri) {
            Ok(u) => u.path().to_string(),
            Err(_) => v
                .uri
                .split(['?', '#'])
                .next()
                .unwrap_or_default()
                .to_string(),
        };
        Self {
            bandwidth: v.bandwidth,
            resolution: v.resolution,
            path,
        }
    }

    /// `bandwidth|WxH(无分辨率为 -)|path`;path 放最后,`|` 在 URL path 里会被转义。
    fn encode(&self) -> String {
        let res = match self.resolution {
            Some((w, h)) => format!("{w}x{h}"),
            None => "-".to_string(),
        };
        format!("{}|{}|{}", self.bandwidth, res, self.path)
    }

    fn decode(s: &str) -> Option<Self> {
        let mut parts = s.splitn(3, '|');
        let bandwidth = parts.next()?.parse().ok()?;
        let res = parts.next()?;
        let resolution = if res == "-" {
            None
        } else {
            let (w, h) = res.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        };
        let path = parts.next()?.to_string();
        Some(Self {
            bandwidth,
            resolution,
            path,
        })
    }
}

/// 在重新解析出的变体列表里找回上次所选变体:先完全匹配,再按
/// (bandwidth, resolution) 唯一匹配,最后按 path 唯一匹配;均不成立返回 `None`。
fn match_saved_variant(variants: &[HlsVariant], saved: &VariantId) -> Option<usize> {
    if let Some(i) = variants.iter().position(|v| VariantId::of(v) == *saved) {
        return Some(i);
    }
    let unique = |pred: &dyn Fn(&VariantId) -> bool| -> Option<usize> {
        let mut it = variants
            .iter()
            .enumerate()
            .filter(|(_, v)| pred(&VariantId::of(v)));
        let first = it.next()?.0;
        if it.next().is_some() {
            None
        } else {
            Some(first)
        }
    };
    unique(&|v| v.bandwidth == saved.bandwidth && v.resolution == saved.resolution)
        .or_else(|| unique(&|v| v.path == saved.path))
}

/// 续传检查点:`idx:byte_offset:media_sequence[:variant]`。
fn format_resume_checkpoint(
    idx: usize,
    bytes: i64,
    media_sequence: u64,
    variant: Option<&str>,
) -> String {
    match variant {
        Some(v) => format!("{idx}:{bytes}:{media_sequence}:{v}"),
        None => format!("{idx}:{bytes}:{media_sequence}"),
    }
}

/// Parse an HLS resume checkpoint string.
///
/// 支持的格式(向后兼容):
/// - `"idx:byte_offset:media_sequence[:variant]"`(当前)
/// - `"idx:byte_offset"`(旧,media_sequence 视为未知 → `None`)
/// - `"idx"`(更早,byte_offset 视为 0)
///
/// 返回 `(saved_idx, saved_bytes, saved_media_seq)`;无法解析 idx 时返回
/// `(0, 0, None)`(等同于不 resume)。
fn parse_resume_checkpoint(s: &str) -> (usize, i64, Option<u64>) {
    let mut parts = s.splitn(4, ':');
    let idx = parts.next().and_then(|p| p.parse().ok());
    let Some(idx) = idx else {
        return (0, 0, None);
    };
    let bytes = parts.next().and_then(|b| b.parse().ok()).unwrap_or(0i64);
    let media_seq = parts.next().and_then(|m| m.parse::<u64>().ok());
    (idx, bytes, media_seq)
}

/// 检查点里的所选变体;旧格式(无第四段)返回 `None`。
fn parse_checkpoint_variant(s: &str) -> Option<VariantId> {
    VariantId::decode(s.splitn(4, ':').nth(3)?)
}

/// 是否可以沿用磁盘上的前缀续传。`saved_idx` 超过新播放列表段数说明列表
/// 变短/换了变体,前缀无法与新列表对齐,必须放弃。
fn resume_is_usable(
    saved_idx: usize,
    saved_bytes: i64,
    file_size: i64,
    segment_count: usize,
    media_seq_changed: bool,
    abandon: bool,
) -> bool {
    saved_idx > 0
        && saved_idx <= segment_count
        && file_size > 0
        && saved_bytes > 0
        && !media_seq_changed
        && !abandon
}

/// Compute the default IV from media_sequence + segment_index.
/// IV = (media_sequence + segment_index) as 128-bit big-endian.
///
/// `sequence_number` 是该段的绝对 Media Sequence Number(RFC 8216 §5.2:无显式
/// IV 时以段的 Media Sequence Number 作 IV)。该序号在整个播放列表内单调递增,
/// **不**在 EXT-X-DISCONTINUITY 处重置——不连续点改变的是 Discontinuity
/// Sequence Number,而非 Media Sequence Number。因此跟踪到的 `discontinuity`
/// 标志不参与隐式 IV 计算;若在此处"重置"序号反而会让合规加密流解出乱码。
///
/// 用 `saturating_add` 而非 `+`:序号接近 `u64::MAX` 时无检查加法在 debug 下
/// panic、在 release 下回绕得到错误 IV;饱和到 `u64::MAX` 不 panic,且此规模
/// 的段索引在现实播放列表中不可能出现,饱和值不会影响真实解密。
fn compute_default_iv(media_sequence: u64, segment_index: usize) -> [u8; 16] {
    let sequence_number = media_sequence.saturating_add(segment_index as u64);
    let mut iv = [0u8; 16];
    // Write as 128-bit big-endian: lower 8 bytes at offset 8
    iv[8..16].copy_from_slice(&sequence_number.to_be_bytes());
    iv
}

/// Decrypt AES-128-CBC encrypted segment data in-place.
///
/// Returns the decrypted data (may be shorter than input due to PKCS7 padding removal).
///
/// RFC 8216 要求 AES-128-CBC 段使用 PKCS7 填充,故首选 Pkcs7 解密。但现实中
/// 存在两类合规变体:某些 CDN/编码器(尤其转封装管线)产出"无填充"的密文,
/// 其总长度可能不是 16 的倍数 —— 此时 Pkcs7 解密必然失败,但数据本身有效。
/// 因此:
/// - 当 `data.len() % 16 != 0`(段本身非块对齐,说明源省略了填充):用
///   NoPadding 解密前 `(len/16)*16` 字节,尾部不足一块的字节丢弃。
/// - 当 `data.len() % 16 == 0` 但 Pkcs7 失败:**不** fallback,保留报错。
///   对齐却解不开通常意味着密钥/IV 错误,fallback 会掩盖真实解密失败、
///   产出垃圾数据。
///
/// `seg_idx` 仅用于在出错时给出可诊断的段索引。
fn decrypt_segment(
    data: &mut [u8],
    key: &[u8],
    iv: &[u8; 16],
    seg_idx: usize,
) -> Result<Vec<u8>, DownloadError> {
    // 空输入短路:加密段下载到 0 字节时,走对齐分支调
    // `decrypt_padded_mut::<Pkcs7>(&mut [])` 会返回 UnpadError,导致整个下载被
    // 当作永久失败中止。空密文解密只能是空明文,直接返回 `Vec::new()`,放在
    // 取模 / PKCS7 逻辑之前。
    if data.is_empty() {
        return Ok(Vec::new());
    }

    let key_array: [u8; 16] = key
        .try_into()
        .map_err(|_| DownloadError::Other("AES key must be 16 bytes".to_string()))?;

    // 非块对齐:源省略了 PKCS7 填充。用 NoPadding 解密对齐前缀,丢弃尾部
    // 不足一块的残余字节(它们无法构成完整密文块)。
    if !data.len().is_multiple_of(AES_BLOCK_SIZE) {
        let aligned = (data.len() / AES_BLOCK_SIZE) * AES_BLOCK_SIZE;
        if aligned == 0 {
            return Err(DownloadError::Other(format!(
                "decrypt_segment: segment {} too short to decrypt ({} bytes, < one AES block)",
                seg_idx,
                data.len()
            )));
        }
        let decryptor = Aes128CbcDec::new_from_slices(&key_array, iv)
            .map_err(|e| DownloadError::Other(format!("AES init error: {}", e)))?;
        let decrypted = decryptor
            .decrypt_padded_mut::<NoPadding>(&mut data[..aligned])
            .map_err(|e| {
                DownloadError::Other(format!(
                    "decrypt_segment: segment {} NoPadding decrypt error: {}",
                    seg_idx, e
                ))
            })?;
        return Ok(decrypted.to_vec());
    }

    // 块对齐:按 RFC 8216 用 PKCS7 解密。失败不 fallback,直接报错(疑似
    // 密钥/IV 错误),避免掩盖真实解密失败。
    let decryptor = Aes128CbcDec::new_from_slices(&key_array, iv)
        .map_err(|e| DownloadError::Other(format!("AES init error: {}", e)))?;

    let decrypted = decryptor.decrypt_padded_mut::<Pkcs7>(data).map_err(|e| {
        DownloadError::Other(format!(
            "decrypt_segment: segment {} PKCS7 decrypt error (likely wrong key/IV): {}",
            seg_idx, e
        ))
    })?;

    Ok(decrypted.to_vec())
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// 执行一次 HLS 下载任务。
///
/// 返回值是调度层（`download_manager`）的退回协议：
/// - `None`：任务已在本函数内跑到终态（完成 / 取消 / 真实 HLS 失败），DB
///   状态与进度事件均已落定，调用方无需再做任何事。
/// - `Some(params)`：URL 仅因 `.m3u8`/`.m3u` 扩展名被误判为 HLS，实际内容
///   不是合法播放列表（[`DownloadError::NotAnHlsPlaylist`]）。只有用户原始
///   URL 的**首次**拉取（且没有续传检查点）会产生该错误；变体 playlist 与
///   已有检查点的续传已确认是 HLS，统一按真实失败上报。此时尚未创建任何
///   临时文件、写入任何字节，未消费的 `params`（含 `.ts` 归一化后的文件名）
///   原样交还——不落终态失败，由调用方把它交给普通 HTTP 下载器，并在此之前
///   把文件名恢复为 URL 派生的原始扩展名，逐字节保存该文件本身。
pub async fn run_hls_download(params: DownloadParams) -> Option<DownloadParams> {
    let task_id_log = params.task_id.clone();
    let result = run_hls_download_inner(&params).await;

    match result {
        Ok(total) => {
            log_info!(
                "[hls-download] task {} completed, total={} bytes",
                task_id_log,
                total
            );
            if let Err(db_error) = params.db.update_task_status(&params.task_id, 3, "").await {
                crate::logger::report_error("hls-download", "persist completion status", &db_error);
            }
            let _ = params
                .progress_tx
                .send(ProgressUpdate {
                    task_id: params.task_id,
                    downloaded_bytes: total,
                    total_bytes: total,
                    status: 3,
                    error_message: String::new(),
                    file_name: String::new(),
                    segment_details: None,
                    ..Default::default()
                })
                .await;
            None
        }
        Err(DownloadError::Cancelled) => {
            log_info!("[hls-download] task {} cancelled", task_id_log);
            None
        }
        Err(DownloadError::NotAnHlsPlaylist(msg)) => {
            // 尚未落任何终态、未写任何字节——原样交还 params，由调度层
            // 退回普通 HTTP 下载器。
            log_info!(
                "[hls-download] task {} url does not resolve to a valid HLS playlist ({}), \
                 falling back to plain HTTP download",
                task_id_log,
                msg
            );
            Some(params)
        }
        Err(e) => {
            let msg = e.to_string();
            crate::logger::report_error("hls-download", "run task", &e);
            if let Err(db_error) = params.db.update_task_status(&params.task_id, 4, &msg).await {
                crate::logger::report_error(
                    "hls-download",
                    "persist terminal error status",
                    &db_error,
                );
            }

            let (dl, total) = match params.db.load_task_by_id(&params.task_id).await {
                Ok(Some(t)) => (t.downloaded_bytes, t.total_bytes),
                other => {
                    crate::log_warn!(
                        "[hls-download] task {} warning: failed to read progress from DB: {:?}",
                        task_id_log,
                        other.err()
                    );
                    (0, 0)
                }
            };
            let _ = params
                .progress_tx
                .send(ProgressUpdate {
                    task_id: params.task_id,
                    downloaded_bytes: dl,
                    total_bytes: total,
                    status: 4,
                    error_message: msg,
                    file_name: String::new(),
                    segment_details: None,
                    ..Default::default()
                })
                .await;
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Variant selection
// ---------------------------------------------------------------------------

/// Timeout for waiting on user quality selection (seconds).
/// After this duration, the best quality is auto-selected.
const QUALITY_SELECTION_TIMEOUT_SECS: u64 = 60;

async fn select_variant(
    task_id: &str,
    variants: &[HlsVariant],
    selector: &dyn crate::selection::HostSelection,
    cancel_token: &tokio_util::sync::CancellationToken,
    unattended: bool,
) -> Result<usize, DownloadError> {
    let auto_select_best = || -> Result<usize, DownloadError> {
        let (best_idx, best) = variants
            .iter()
            .enumerate()
            .max_by_key(|(_, v)| v.bandwidth)
            .ok_or_else(|| DownloadError::Other("no variants in master playlist".to_string()))?;
        log_info!(
            "[hls-download] task {} auto-selected variant: bandwidth={}, resolution={:?}",
            task_id,
            best.bandwidth,
            best.resolution
        );
        Ok(best_idx)
    };

    // Skip the selector entirely when there is only one variant — no point
    // asking — or when the task was created unattended (RSS / silent takeover):
    // popping a dialog nobody is around to answer just delays the download by
    // the selection timeout.
    if variants.len() <= 1 || unattended {
        log_info!(
            "[hls-download] task {} skipping quality dialog ({} variant(s), unattended={})",
            task_id,
            variants.len(),
            unattended
        );
        return auto_select_best();
    }

    let options: Vec<HlsQualityOption> = variants
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let (w, h) = v.resolution.unwrap_or((0, 0));
            HlsQualityOption {
                index: i as i32,
                bandwidth: v.bandwidth as i64,
                width: w as i64,
                height: h as i64,
            }
        })
        .collect();

    log_info!(
        "[hls-download] task {} requesting quality selection ({} variants) via HostSelection (timeout={}s)",
        task_id,
        variants.len(),
        QUALITY_SELECTION_TIMEOUT_SECS
    );

    let timeout_duration = std::time::Duration::from_secs(QUALITY_SELECTION_TIMEOUT_SECS);

    tokio::select! {
        _ = cancel_token.cancelled() => {
            Err(DownloadError::Cancelled)
        }
        outcome = selector.select_hls_quality(task_id, &options, timeout_duration) => {
            let idx = match &outcome {
                SelectionOutcome::UserChose(idx) => {
                    log_info!(
                        "[hls-download] task {} user selected variant {}",
                        task_id, idx
                    );
                    *idx
                }
                SelectionOutcome::TimedOutDefaulted(idx) => {
                    log_info!(
                        "[hls-download] task {} quality selection timed out ({}s), defaulting to variant {}",
                        task_id, QUALITY_SELECTION_TIMEOUT_SECS, idx
                    );
                    *idx
                }
                SelectionOutcome::NoSelectorConfigured(idx) => {
                    log_info!(
                        "[hls-download] task {} no selector configured, defaulting to variant {}",
                        task_id, idx
                    );
                    *idx
                }
            };
            let variant = variants.get(idx as usize).ok_or_else(|| {
                DownloadError::Other(format!(
                    "invalid HLS quality index: {} (have {} variants)",
                    idx,
                    variants.len()
                ))
            })?;
            log_info!(
                "[hls-download] task {} using variant: bandwidth={}, resolution={:?}",
                task_id, variant.bandwidth, variant.resolution
            );
            Ok(idx as usize)
        }
    }
}

// ---------------------------------------------------------------------------
// Core logic
// ---------------------------------------------------------------------------

/// 判断写入 `seg_idx` 段字节前是否需要先(重新)写入其 EXT-X-MAP 初始化段
/// (#682)。
///
/// - 当前段没有 map(非 fMP4 段)→ 不需要。
/// - 有 map 且与磁盘上最后写入的不同(该 map 首次出现,或播放列表中途切换
///   了 init 段)→ 需要。
/// - 有 map 且与磁盘上最后写入的相同 → 已经在磁盘上,跳过(避免重复拼接
///   同一个 init 段)。
fn should_write_init(last_written: Option<&MapKey>, current: Option<&MapKey>) -> bool {
    match current {
        None => false,
        Some(c) => last_written != Some(c),
    }
}

async fn run_hls_download_inner(p: &DownloadParams) -> Result<i64, DownloadError> {
    log_info!("[hls-download] task {} starting, url={}", p.task_id, p.url);

    // Transition to status=5 (preparing)
    let _ = p.db.update_task_status(&p.task_id, 5, "").await;
    let _ = p
        .progress_tx
        .send(ProgressUpdate {
            task_id: p.task_id.clone(),
            downloaded_bytes: 0,
            total_bytes: 0,
            status: 5,
            error_message: String::new(),
            file_name: p.file_name.clone(),
            segment_details: None,
            ..Default::default()
        })
        .await;

    // 续传检查点先于解析读取:(1) 据此沿用上次所选变体、不再重复弹窗并保证
    // 续传前后是同一画质;(2) 已有检查点说明此 URL 早已确认是 HLS,之后解析
    // 失败必须按真实错误上报,不能退回普通 HTTP 下载把清单文本当文件存下。
    let resume_seg_key = format!("hls_resume_{}", p.task_id);
    let saved_checkpoint: Option<String> = if p.is_resume {
        p.db.get_config(&resume_seg_key).await.ok().flatten()
    } else {
        None
    };
    let (saved_idx, saved_bytes, saved_media_seq) = saved_checkpoint
        .as_deref()
        .map(parse_resume_checkpoint)
        .unwrap_or((0, 0, None));
    let saved_variant = saved_checkpoint
        .as_deref()
        .and_then(parse_checkpoint_variant);
    let confirmed_hls = saved_checkpoint.is_some();

    // Parse the M3U8 playlist
    let root_ctx = HlsRequestCtx {
        cookies: &p.cookies,
        cookie_base_url: &p.url,
        header_origin_url: &p.url,
        referrer: &p.referrer,
        extra_headers: &p.extra_headers,
    };
    let content = parse_m3u8(&p.client, &p.url, &root_ctx, &p.cancel_token)
        .await
        .map_err(|e| if confirmed_hls { reject_not_hls(e) } else { e })?;

    // media_playlist_url 是"实际列出 segment/key 的播放列表 URL"：
    // master→media 两级结构里是选中的 media playlist(selected_uri),
    // 直接 media 路径里就是 p.url 本身。段/密钥的同源 cookie 判定必须以
    // 它为基准——master 与 media playlist 经常跨主机(master 在主域、
    // media+segments+key 在 CDN),用 master URL 判同源会错误剥离 CDN
    // 鉴权 cookie 导致 403/401。直接 media 路径下 media_playlist_url==p.url,
    // 行为不变。
    let mut abandon_resume = false;
    let (segments, media_sequence, media_playlist_url, is_fmp4, chosen_variant, live) =
        match content {
            M3u8Content::Master { variants } => {
                let saved_match = saved_variant
                    .as_ref()
                    .and_then(|s| match_saved_variant(&variants, s));
                let variant_idx = match saved_match {
                    Some(i) => {
                        log_info!(
                            "[hls-download] task {} resuming with previously selected variant {}",
                            p.task_id,
                            i
                        );
                        i
                    }
                    None => {
                        // 已有进度但无法确认上次画质(旧检查点且需要用户选择,或已记录
                        // 的变体在新清单里找不到):磁盘前缀可能属于另一画质,拼接会
                        // 产出错位/不可解码的文件,只能全量重下。单变体与无人值守任务
                        // 的自动选择是确定的,旧检查点照常续传。
                        if saved_idx > 0
                            && (saved_variant.is_some() || (variants.len() > 1 && !p.unattended))
                        {
                            log_info!(
                                "[hls-download] task {} cannot identify the previously selected variant, \
                                 re-downloading from scratch",
                                p.task_id
                            );
                            abandon_resume = true;
                        }
                        select_variant(
                            &p.task_id,
                            &variants,
                            p.selector.as_ref(),
                            &p.cancel_token,
                            p.unattended,
                        )
                        .await?
                    }
                };

                if p.cancel_token.is_cancelled() {
                    return Err(DownloadError::Cancelled);
                }

                let Some(variant) = variants.get(variant_idx) else {
                    return Err(DownloadError::Other(format!(
                        "selected HLS variant {} out of range ({} variants)",
                        variant_idx,
                        variants.len()
                    )));
                };
                let selected_uri = variant.uri.clone();
                let chosen = VariantId::of(variant);

                if variant.audio_uri.is_some() && saved_idx == 0 {
                    crate::log_warn!(
                        "[hls-download] task {} variant uses a separate EXT-X-MEDIA audio rendition; \
                         only the video stream is downloaded",
                        p.task_id
                    );
                    if let Err(journal_error) = crate::task_activity::record(
                        &p.db,
                        p.sink.as_ref(),
                        &p.task_id,
                        "warning",
                        "该 HLS 流的音频是独立音轨（EXT-X-MEDIA），当前仅下载视频流，输出文件可能没有声音",
                        None,
                    )
                    .await
                    {
                        crate::log_error!(
                            "[task-activity] failed to persist warning: {}",
                            journal_error
                        );
                    }
                }

                // 所选 variant 可能指向与 p.url 不同源的 CDN:ctx 对 Cookie 与凭据类
                // 请求头都按同源过滤,不会把原站点的会话/鉴权令牌带给第三方。
                // 此时已确认是 HLS,内容不是播放列表按真实失败处理。
                let media_content =
                    parse_m3u8(&p.client, &selected_uri, &root_ctx, &p.cancel_token)
                        .await
                        .map_err(reject_not_hls)?;
                match media_content {
                    M3u8Content::Media {
                        segments,
                        total_duration: _,
                        media_sequence,
                        has_map,
                        live,
                    } => (
                        segments,
                        media_sequence,
                        selected_uri,
                        has_map,
                        Some(chosen),
                        live,
                    ),
                    M3u8Content::Master { .. } => {
                        return Err(DownloadError::Other(
                            "nested master playlist not supported".to_string(),
                        ));
                    }
                }
            }
            M3u8Content::Media {
                segments,
                total_duration: _,
                media_sequence,
                has_map,
                live,
            } => (segments, media_sequence, p.url.clone(), has_map, None, live),
        };

    if live {
        return Err(DownloadError::Other(
            "暂不支持直播或未结束的 HLS 流（播放列表缺少 EXT-X-ENDLIST）".to_string(),
        ));
    }

    let segment_count = segments.len();
    log_info!(
        "[hls-download] task {} found {} segments, media_sequence={}",
        p.task_id,
        segment_count,
        media_sequence
    );

    if segment_count == 0 {
        return Err(DownloadError::Other(
            "HLS playlist has no segments".to_string(),
        ));
    }

    // 文件名沿用 DownloadManager 预订的 `.ts`(dedup / 兄弟任务协调都按它做),
    // fMP4/CMAF 播放列表(存在 EXT-X-MAP)也不例外——其输出是分片 MP4 而非
    // MPEG-TS,但改扩展名统一放到完成后的 finalize(见 `remux_ts_to_mp4` 的
    // is_fmp4 分支:跳过 TS→MP4 转换,走同一套占名协议直接改名 `.mp4`)。#682
    let auto_name = if p.file_name.is_empty() {
        let url_name = extract_from_url(&p.url).unwrap_or_else(|| "download.ts".to_string());
        force_ts_extension(&url_name)
    } else {
        force_ts_extension(&sanitize_filename(&p.file_name))
    };

    let save_dir = PathBuf::from(&p.save_dir);
    // 文件名由 DownloadManager 在 do_start_task 同步段统一决策（含 dedup 和
    // 兄弟任务预订协调），HLS downloader 内不再做名称变更——保留
    // p.file_name 即可，仅当为空时（兜底）使用 URL 解析结果。
    let actual_name = auto_name.clone();

    // total_bytes is unknown for HLS until we download all segments
    p.db.update_task_file_info(&p.task_id, &actual_name, 0)
        .await?;

    // 早期取消检查：probe/解析完成后、创建文件之前检测 pause/delete，
    // 防止已取消的任务仍然在磁盘上创建临时文件。
    if p.cancel_token.is_cancelled() {
        return Err(DownloadError::Cancelled);
    }

    let _ = p.db.update_task_status(&p.task_id, 1, "").await;

    // Notify Dart: downloading started with file name
    let _ = p
        .progress_tx
        .send(ProgressUpdate {
            task_id: p.task_id.clone(),
            downloaded_bytes: 0,
            total_bytes: 0,
            status: 1,
            error_message: String::new(),
            file_name: actual_name.clone(),
            segment_details: None,
            ..Default::default()
        })
        .await;

    let dest_path = save_dir.join(&actual_name);
    let temp_path = PathBuf::from(format!("{}{}", dest_path.display(), TEMP_EXT));

    // Ensure parent directory exists
    output::ensure_parent(&temp_path).await?;

    // --- HLS resume support ---
    // On resume, check if we have a saved segment index from a previous run.
    // If so, skip already-downloaded segments and open the temp file in append mode.
    let (mut file, skip_segments, mut downloaded_bytes) = if p.is_resume {
        // 检查点(saved_*)已在解析前读出。当前格式
        // "idx:byte_offset:media_sequence[:variant]";向后兼容旧格式
        // "idx:byte_offset"(缺 media_sequence 视为未知)与更早的 "idx"
        // (缺 byte_offset 视为 0,不截断)。
        //
        // IV 计算(无显式 IV 的加密段)依赖 media_sequence。若服务器在两次
        // 抓取之间重写了 EXT-X-MEDIA-SEQUENCE(VOD 被 CDN 重新生成等),已
        // 跳过的段与新解析的 media_sequence 组合会让续传段用错 IV,解密出
        // 垃圾数据。检测到不一致时放弃 resume,走全量重下保证 IV 与首次一致。
        // 旧格式 checkpoint(saved_media_seq=None)无法判断服务器是否改写了
        // EXT-X-MEDIA-SEQUENCE。仅当播放列表确实含"AES-128 且无显式 IV"的段
        // (其 IV=compute_default_iv(media_sequence,idx),依赖 media_sequence)时,
        // media_sequence 漂移才会导致解密错位;此时对旧 checkpoint 保守放弃 resume
        // 全量重下。明文 / 显式 IV / 常量 media_sequence(VOD 通常恒为 0)等常见场景
        // 不受影响,继续 resume 以保留有效进度。
        let uses_computed_iv = segments.iter().any(|s| {
            s.key
                .as_ref()
                .is_some_and(|k| k.method == HlsKeyMethod::Aes128 && k.iv.is_none())
        });
        let media_seq_changed = match saved_media_seq {
            Some(prev) => prev != media_sequence,
            None => uses_computed_iv,
        };
        if media_seq_changed {
            log_info!(
                "[hls] task {} media_sequence changed across resume (saved={:?}, now={}), \
                 abandoning resume and re-downloading from scratch to keep IV consistent",
                p.task_id,
                saved_media_seq,
                media_sequence
            );
        }
        if saved_idx > segment_count {
            log_info!(
                "[hls] task {} saved segment index {} exceeds playlist length {}, \
                 abandoning resume and re-downloading from scratch",
                p.task_id,
                saved_idx,
                segment_count
            );
        }

        let file_size = tokio::fs::metadata(&temp_path)
            .await
            .map(|m| m.len() as i64)
            .unwrap_or(0);
        // 续传要求 saved_bytes > 0(三字段 checkpoint 记录的已完整落盘字节数)。
        // 早期版本只写 "idx"(无字节数)的旧 checkpoint 解析出 saved_bytes=0,此时
        // 无法确认磁盘上第 saved_idx 段是否完整——若上次硬崩在 write_all 中途,
        // file_size 会含残字节;旧逻辑用 file_size 当 safe_size 不截断,会把残字节当
        // 有效数据、后续段追加其后导致输出损坏。故对无字节偏移的旧 checkpoint 保守
        // 放弃 resume、全量重下(仅影响从早期版本升级、且恰好硬崩在段中途的遗留任务)。
        if resume_is_usable(
            saved_idx,
            saved_bytes,
            file_size,
            segment_count,
            media_seq_changed,
            abandon_resume,
        ) {
            // Truncate to the exact byte offset of the last fully-completed segment.
            // This removes any partially-written data from a crashed segment.
            let safe_size = saved_bytes.min(file_size);
            if safe_size < file_size {
                log_info!(
                    "[hls] task {} truncating temp file {} -> {} bytes (removing partial segment data)",
                    p.task_id,
                    file_size,
                    safe_size
                );
                let truncate_file = tokio::fs::OpenOptions::new()
                    .write(true)
                    .open(&temp_path)
                    .await?;
                truncate_file.set_len(safe_size as u64).await?;
                drop(truncate_file);
            }
            log_info!(
                "[hls] task {} resuming from segment {} (file size: {} bytes, safe: {} bytes)",
                p.task_id,
                saved_idx,
                file_size,
                safe_size
            );
            let f = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&temp_path)
                .await?;
            (f, saved_idx, safe_size)
        } else {
            (File::create(&temp_path).await?, 0, 0i64)
        }
    } else {
        // Clean up any stale resume marker from a previous run
        let _ = p.db.delete_config(&resume_seg_key).await;
        (File::create(&temp_path).await?, 0, 0i64)
    };

    // 所选变体随检查点一起持久化(与 hls_resume_<id> 同生命周期,任务删除时
    // 一并清理)。从头下载时先写一个零进度检查点,使首段落盘前的自动重试
    // 也能沿用同一画质而不是再次弹窗。
    let variant_tag: Option<String> = chosen_variant.as_ref().map(VariantId::encode);
    if skip_segments == 0
        && let Some(tag) = variant_tag.as_deref()
    {
        let _ =
            p.db.set_config(
                &resume_seg_key,
                &format_resume_checkpoint(0, 0, media_sequence, Some(tag)),
            )
            .await;
    }

    let key_cache: KeyCache = Arc::new(Mutex::new(HashMap::new()));
    let mut last_report = std::time::Instant::now();
    let mut last_db_save = std::time::Instant::now();

    // -----------------------------------------------------------------------
    // Bounded-concurrency segment download.
    //
    // Each remaining segment downloads + decrypts on its own task, gated by a
    // `Semaphore` so at most `concurrency` are in flight (and at most that many
    // decrypted buffers are buffered waiting to be written). A single writer —
    // this function — drains finished segments **strictly in `seg_idx` order**
    // via a `BTreeMap`, then runs the *same* speed-limit / write / rollback /
    // checkpoint / progress code the sequential implementation used. Keeping
    // all writes on one task guarantees:
    //   • the on-disk byte order is identical to the sequential version,
    //   • `downloaded_bytes` is accumulated exactly once per segment (no
    //     double-counting / loss under concurrency),
    //   • the speed limiter is consulted on the single write path,
    //   • the resume checkpoint advances monotonically by completed prefix.
    // Each task computes its IV from its own `seg_idx`, so AES-128-CBC
    // decryption stays correct regardless of completion order.
    // -----------------------------------------------------------------------
    let first_idx = skip_segments;
    let remaining = segment_count.saturating_sub(first_idx);
    let concurrency = hls_concurrency(p.segment_count, remaining);
    log_info!(
        "[hls-download] task {} downloading {} remaining segment(s) with concurrency {}",
        p.task_id,
        remaining,
        concurrency
    );

    // -----------------------------------------------------------------------
    // EXT-X-MAP(fMP4/CMAF 初始化段,#682)预取。
    //
    // init 段(ftyp+moov)与媒体段走不同的写入语义——同一个 init 段通常被
    // 多个连续媒体段共享,必须只写入一次而非每段重复拼接。为保持单一有序
    // writer 的现有并发模型不变,这里在生成媒体段下载任务【之前】,顺序
    // 预取本次运行(`first_idx..segment_count`,即尚未落盘的剩余段)会用到
    // 的每个不同 init 段字节,缓存进 `map_bytes`;写入时机与去重判断见下方
    // writer 循环里的 `should_write_init`。
    //
    // 只预取剩余段用到的 map(而非整份播放列表的所有 map):已经落盘的前缀
    // 段对应的 init 段字节已经在磁盘上,无需重新下载。
    //
    // 复用 `download_segment_with_retry`(与媒体段完全相同的重试/退避/
    // Range 请求路径),`seg_idx` 传 `usize::MAX` 仅用于失败时的日志诊断,
    // 不代表真实段序号。
    let tracker = TransferTracker::new();
    // Segments can stay in the body for longer than a completed writer chunk.
    // Sample independently, including while no byte geometry is available.
    let reported_bytes = Arc::new(AtomicI64::new(downloaded_bytes));
    let sample_tracker = tracker.clone();
    let sample_bytes = Arc::clone(&reported_bytes);
    let sample_tx = p.progress_tx.clone();
    let sample_task = p.task_id.clone();
    let sample_limit = concurrency as u32;
    let sampler = AbortOnDrop(tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(200));
        loop {
            ticker.tick().await;
            if sample_tx
                .send(ProgressUpdate {
                    task_id: sample_task.clone(),
                    downloaded_bytes: sample_bytes.load(Ordering::Relaxed),
                    total_bytes: 0,
                    status: 1,
                    runtime: Some(hls_runtime(&sample_task, &sample_tracker, sample_limit)),
                    ..Default::default()
                })
                .await
                .is_err()
            {
                break;
            }
        }
    }));
    let seg_ctx = HlsRequestCtx {
        cookies: &p.cookies,
        cookie_base_url: &media_playlist_url,
        header_origin_url: &p.url,
        referrer: &p.referrer,
        extra_headers: &p.extra_headers,
    };
    let mut map_bytes: HashMap<MapKey, Vec<u8>> = HashMap::new();
    if is_fmp4 {
        let mut seen: std::collections::HashSet<MapKey> = std::collections::HashSet::new();
        for seg in segments.iter().skip(first_idx) {
            let Some(m) = &seg.map else { continue };
            let map_key = (m.uri.clone(), m.byte_range);
            if !seen.insert(map_key.clone()) {
                continue;
            }
            let init_data = match download_segment_with_retry(
                &p.client,
                &m.uri,
                m.byte_range,
                &seg_ctx,
                &p.cancel_token,
                &p.task_id,
                usize::MAX,
                &tracker,
                &p.db,
                p.sink.as_ref(),
            )
            .await
            {
                Ok(data) => data,
                Err(e) => {
                    sampler.abort();
                    return Err(e);
                }
            };
            log_info!(
                "[hls-download] task {} fetched init segment {} ({} bytes)",
                p.task_id,
                m.uri,
                init_data.len()
            );
            map_bytes.insert(map_key, init_data);
        }
    }

    let semaphore = Arc::new(Semaphore::new(concurrency));
    // 许可由 dispatcher 按 seg_idx 顺序获取,并随下载结果一起送回 writer,写盘后
    // 才释放。因此"在途 + 已下载待写"的段总数 ≤ concurrency,队首段慢时乱序
    // 缓冲不会无界增长;又因为许可严格按 idx 递增发放,最小的未写段一定已持有
    // 许可,不会出现许可被后续段占满、队首段拿不到许可的死锁。
    // Channel 容量 == concurrency:未释放的许可数不超过它,结果入队从不阻塞。
    let (result_tx, mut result_rx) = mpsc::channel::<SegmentOutcome>(concurrency.max(1));

    let jobs: Vec<SegmentJob> = segments
        .iter()
        .enumerate()
        .skip(first_idx)
        .map(|(idx, segment)| SegmentJob {
            idx,
            uri: segment.uri.clone(),
            byte_range: segment.byte_range,
            // Extract only the encryption fields needed to decrypt this segment.
            key_info: segment.key.as_ref().and_then(|k| {
                if k.method == HlsKeyMethod::Aes128 && !k.uri.is_empty() {
                    Some((k.uri.clone(), k.iv.clone()))
                } else {
                    None
                }
            }),
        })
        .collect();
    let shared = Arc::new(SegmentShared {
        client: p.client.clone(),
        cookies: p.cookies.clone(),
        playlist_url: media_playlist_url.clone(),
        header_origin_url: p.url.clone(),
        referrer: p.referrer.clone(),
        extra_headers: p.extra_headers.clone(),
        cancel: p.cancel_token.clone(),
        task_id: p.task_id.clone(),
        key_cache: key_cache.clone(),
        media_sequence,
        tracker: tracker.clone(),
        db: p.db.clone(),
        sink: p.sink.clone(),
    });
    // dispatcher 持有唯一的发送端;它与其子任务全部结束后 channel 关闭,
    // writer 的 recv 循环才不会在末尾挂起。
    let dispatcher = tokio::spawn(dispatch_segments(jobs, shared, semaphore, result_tx));

    // In-order writer: buffer out-of-order completions and flush the contiguous
    // prefix starting at `next_to_write`.
    let mut pending: BTreeMap<usize, (Vec<u8>, OwnedSemaphorePermit)> = BTreeMap::new();
    let mut next_to_write = first_idx;
    let mut fatal_error: Option<DownloadError> = None;
    // EXT-X-MAP(#682):跟踪"磁盘上最后一次写入的 init 段"(uri, byte_range)。
    // 全新下载(first_idx==0)时磁盘上还没有任何 init 段 → `None`。断点续传
    // (first_idx>0)时没有把"历史 init 段身份"持久化进 checkpoint(见上方
    // 预取处的说明,故意不引入新持久化状态)——保守假设"上一个已完整落盘的
    // 段(segments[first_idx-1])的 map 就是磁盘上最后写入的 init 段"。这是
    // 稳妥的:map 字段在解析期已按 RFC 8216 粘性传播(parse_m3u8_bytes),同一
    // map 的连续段只在其首次出现时触发写入,与首次下载时 writer 的行为一致。
    let mut last_written_map: Option<MapKey> = if first_idx == 0 {
        None
    } else {
        segments
            .get(first_idx - 1)
            .and_then(|s| s.map.as_ref())
            .map(|m| (m.uri.clone(), m.byte_range))
    };

    'writer: while next_to_write < segment_count {
        // Writer-side cancellation check (mirrors the original between-segment
        // check). Cancel the token so producers abort, flush progress, exit.
        if p.cancel_token.is_cancelled() {
            fatal_error = Some(DownloadError::Cancelled);
            break;
        }

        // If the next segment isn't buffered yet, wait for more completions.
        while !pending.contains_key(&next_to_write) {
            match result_rx.recv().await {
                Some((idx, Ok(data), permit)) => {
                    pending.insert(idx, (data, permit));
                }
                Some((idx, Err(e), _permit)) => {
                    // A segment failed permanently. Cancel siblings and stop;
                    // the partial prefix already on disk is kept for resume.
                    log_info!(
                        "[hls-download] task {} segment {} failed: {}",
                        p.task_id,
                        idx,
                        e
                    );
                    p.cancel_token.cancel();
                    fatal_error = Some(e);
                    break 'writer;
                }
                None => {
                    // Channel closed before producing `next_to_write`: every
                    // worker exited without delivering it. Besides a user
                    // cancel this means a worker died (panic), which must
                    // surface as a failure instead of leaving the task in
                    // "downloading" with nothing running.
                    if fatal_error.is_none() {
                        fatal_error = Some(if p.cancel_token.is_cancelled() {
                            DownloadError::Cancelled
                        } else {
                            DownloadError::Other(format!(
                                "HLS segment workers exited unexpectedly before segment {}",
                                next_to_write
                            ))
                        });
                    }
                    break 'writer;
                }
            }
        }

        // Flush every contiguous segment we already have, in order.
        while let Some((output_data, _permit)) = pending.remove(&next_to_write) {
            let seg_idx = next_to_write;

            // Stop flushing promptly on cancellation. Segments already written
            // (the contiguous prefix) stay on disk with a matching checkpoint,
            // so a later resume continues cleanly; this in-memory segment and
            // the rest of `pending` are discarded (they will be re-downloaded).
            if p.cancel_token.is_cancelled() {
                drop(output_data);
                fatal_error = Some(DownloadError::Cancelled);
                break 'writer;
            }

            // EXT-X-MAP(#682):若本段生效的 init 段与磁盘上最后写入的不同
            // (首次出现,或播放列表中途切换了 init 段),需要在本段媒体字节
            // 之前先写入 init 字节。两者作为同一次"写入尝试"共享下面的
            // seg_start_pos 回退点与 downloaded_bytes 计数,任一部分失败都整体
            // 回退(避免只落盘半个 init 段或"init 有、媒体段无"的不一致状态)。
            let current_map_key = segments[seg_idx]
                .map
                .as_ref()
                .map(|m| (m.uri.clone(), m.byte_range));
            let init_chunk: Option<&[u8]> =
                if should_write_init(last_written_map.as_ref(), current_map_key.as_ref()) {
                    match current_map_key.as_ref().and_then(|k| map_bytes.get(k)) {
                        Some(data) => Some(data.as_slice()),
                        None => {
                            // 理论不可达:预取阶段已为 first_idx..segment_count 内
                            // 出现的每个不同 map 下载好字节;缺失说明预取逻辑与
                            // 这里的判定不一致(bug)。报错而非静默跳过 init 段——
                            // 跳过会产出不可解码的 fMP4 输出却标记任务完成。
                            fatal_error = Some(DownloadError::Other(format!(
                                "internal error: init segment for {:?} was not prefetched",
                                current_map_key
                            )));
                            break 'writer;
                        }
                    }
                } else {
                    None
                };

            // Apply speed limiter and write to file (init chunk, if any, then
            // the media segment bytes).
            //
            // seg_start_pos 是本次写入前文件的逻辑长度。resume 时文件已被
            // truncate 到恰好 safe_size(== 初始 downloaded_bytes),此后每次
            // 迭代以 append 方式精确追加写入的字节数,故文件磁盘长度始终等于
            // downloaded_bytes —— 用它作为出错回退点是准确的。
            let seg_start_pos = downloaded_bytes;
            let mut written_len: i64 = 0;
            let mut write_result: Result<(), std::io::Error> = Ok(());
            'chunks: for chunk in init_chunk
                .into_iter()
                .chain(std::iter::once(output_data.as_slice()))
            {
                let mut offset = 0usize;
                while offset < chunk.len() {
                    let remaining_bytes = (chunk.len() - offset) as u64;
                    let allowed = p.speed_limiter.consume(remaining_bytes).await;
                    let end = offset + allowed as usize;
                    if let Err(e) = file.write_all(&chunk[offset..end]).await {
                        write_result = Err(e);
                        break 'chunks;
                    }
                    offset = end;
                }
                written_len += chunk.len() as i64;
            }

            if let Err(e) = write_result {
                // 写入中途失败(常见:磁盘满 ENOSPC)。本次迭代(可能含 init 段
                // + 媒体段)已部分写入,先把文件回退到写入前的长度,避免残留
                // 半截数据污染后续 resume(与 dash_downloader 的
                // set_len(start_pos) 兜底一致)。回退失败仅记录日志,不掩盖
                // 原始写入错误。
                if let Err(trunc_err) = file.set_len(seg_start_pos as u64).await {
                    log_info!(
                        "[hls] task {} segment {} rollback set_len({}) failed: {}",
                        p.task_id,
                        seg_idx,
                        seg_start_pos,
                        trunc_err
                    );
                }
                p.cancel_token.cancel();
                // 磁盘空间不足(ENOSPC, errno 28 / ErrorKind::StorageFull)给出
                // 明确提示,便于用户区分"磁盘满"与普通 IO 错误。
                if e.kind() == std::io::ErrorKind::StorageFull || e.raw_os_error() == Some(28) {
                    fatal_error = Some(DownloadError::Other(
                        "磁盘空间不足，请清理磁盘后重试".to_string(),
                    ));
                } else {
                    fatal_error = Some(DownloadError::Io(e));
                }
                break 'writer;
            }

            if init_chunk.is_some() {
                last_written_map = current_map_key;
            }
            downloaded_bytes += written_len;
            reported_bytes.store(downloaded_bytes, Ordering::Relaxed);
            next_to_write += 1;

            // Save resume checkpoint for HLS resume support.
            // Format: "next_seg_idx:total_bytes_written:media_sequence" — on resume
            // we truncate to this byte offset to discard any partially-written
            // segment data,并比对 media_sequence 以保证续传段的 IV 计算与首次一致。
            // 因为按 seg_idx 顺序写盘,next_to_write 即"已完整落盘的连续前缀
            // 长度",检查点始终对应一段完整、可安全续传的字节边界。
            let _ =
                p.db.set_config(
                    &resume_seg_key,
                    &format_resume_checkpoint(
                        next_to_write,
                        downloaded_bytes,
                        media_sequence,
                        variant_tag.as_deref(),
                    ),
                )
                .await;

            // Progress reporting (every 200ms)
            if last_report.elapsed().as_millis() >= 200 {
                let _ = p
                    .progress_tx
                    .send(ProgressUpdate {
                        task_id: p.task_id.clone(),
                        downloaded_bytes,
                        total_bytes: 0, // unknown for HLS
                        status: 1,
                        error_message: String::new(),
                        file_name: String::new(),
                        segment_details: None,
                        runtime: Some(hls_runtime(&p.task_id, &tracker, concurrency as u32)),
                        ..Default::default()
                    })
                    .await;
                last_report = std::time::Instant::now();
            }

            // DB persistence (every DB_SAVE_INTERVAL_SECS)
            if last_db_save.elapsed().as_secs() >= DB_SAVE_INTERVAL_SECS {
                let _ =
                    p.db.update_task_progress(&p.task_id, downloaded_bytes)
                        .await;
                last_db_save = std::time::Instant::now();
            }

            log_info!(
                "[hls-download] task {} segment {}/{} done, {} bytes total",
                p.task_id,
                seg_idx + 1,
                segment_count,
                downloaded_bytes
            );
        }
    }

    // Ensure producers stop and are reaped before we touch the file further.
    // On error/cancel the token is already cancelled; either way drain the
    // handles so no task outlives this function.
    if fatal_error.is_some() {
        p.cancel_token.cancel();
    }
    // Drop the receiver FIRST: on the error/cancel path the writer stopped
    // recv'ing, so a producer parked in `tx.send().await` (bounded channel at
    // capacity) would block forever and hang the join below. Closing the
    // receiver makes those sends return `Err` immediately, letting every
    // producer unwind. On the success path the channel is already drained, so
    // this is a no-op.
    drop(result_rx);
    let _ = dispatcher.await;
    sampler.abort();

    if let Some(err) = fatal_error {
        // Persist whatever fully-written prefix we have so a later resume can
        // continue from there (matches the sequential cancel path).
        let _ = file.flush().await;
        let _ =
            p.db.update_task_progress(&p.task_id, downloaded_bytes)
                .await;
        return Err(err);
    }

    file.flush().await?;
    drop(file);

    // Save final progress
    let _ =
        p.db.update_task_progress(&p.task_id, downloaded_bytes)
            .await;

    // Clean up HLS resume marker on successful completion
    let _ = p.db.delete_config(&resume_seg_key).await;

    tokio::fs::rename(&temp_path, &dest_path)
        .await
        .map_err(|e| {
            DownloadError::Other(format!(
                "failed to rename {} -> {}: {}",
                temp_path.display(),
                dest_path.display(),
                e
            ))
        })?;

    log_info!(
        "[hls-download] task {} renamed {} -> {}",
        p.task_id,
        temp_path.display(),
        dest_path.display()
    );

    // 非 fMP4:TS→MP4 转换;fMP4/CMAF(#682):内容已是分片 MP4,只走占名协议
    // 把 `.ts` 改名成 `.mp4`。两者失败都保留 `.ts` 并按原名完成。
    if let Some(mp4_path) =
        remux_ts_to_mp4(&dest_path, &p.task_id, p.allow_overwrite, is_fmp4).await
    {
        let mp4_file_name = mp4_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("output.mp4")
            .to_string();
        let mp4_size = tokio::fs::metadata(&mp4_path)
            .await
            .ok()
            .and_then(|m| i64::try_from(m.len()).ok())
            .unwrap_or(downloaded_bytes);

        match p
            .db
            .update_task_file_info(&p.task_id, &mp4_file_name, mp4_size)
            .await
        {
            Ok(_) => {
                let _ = tokio::fs::remove_file(&dest_path).await;
                let _ = p
                    .progress_tx
                    .send(ProgressUpdate {
                        task_id: p.task_id.clone(),
                        downloaded_bytes: mp4_size,
                        total_bytes: mp4_size,
                        // remux 成功即完成,发 status=3(完成);外层 run_hls_download
                        // 还会再发一次 status=3,Dart 端有 oldStatus!=completed 守卫,
                        // 不会重复触发完成回调。
                        status: 3,
                        error_message: String::new(),
                        file_name: mp4_file_name,
                        segment_details: None,
                        ..Default::default()
                    })
                    .await;
                return Ok(mp4_size);
            }
            Err(e) => {
                log_info!(
                    "[hls] task {} DB update failed after remux: {}, removing orphan mp4 at {}",
                    p.task_id,
                    e,
                    mp4_path.display()
                );
                // DB update failed: the task record still points to the .ts file name.
                // delete_task uses the DB file_name to locate files, so the .mp4
                // would never be cleaned up. Remove it now to prevent a disk leak.
                let _ = tokio::fs::remove_file(&mp4_path).await;
            }
        }
    }

    Ok(downloaded_bytes)
}

// ---------------------------------------------------------------------------
// TS → MP4 remux (best-effort)
// ---------------------------------------------------------------------------

/// ts2mp4 会在内存里同时持有整份 TS 与多份中间产物(峰值为文件体积的数倍),
/// 上限收紧到 192MB 以避免低内存设备在收尾阶段 OOM;超限时保留 .ts。
const MAX_REMUX_BYTES: u64 = 192 * 1024 * 1024;

/// remux 需要 dest 卷至少还有 `file_len`(mp4 产物 ≈ ts 体积,仅重封装
/// 无转码)+ 安全余量——remux 期间 `.ts` 与 `.mp4` 并存,峰值 ≈ 2x。
/// `avail=None`(网络盘/权限/超时,无法探测)按放行处理:预检是优化,
/// 安全网是下方既有的写失败清理路径。
fn remux_space_ok(avail: Option<u64>, file_len: u64) -> bool {
    match avail {
        Some(a) => a >= file_len.saturating_add(crate::disk_space::PRECHECK_MARGIN),
        None => true,
    }
}

/// `allow_overwrite`（config `file_exists_behavior` == "overwrite"）：为
/// true 时,同名 `.mp4` 已作为普通最终文件存在不触发编号改名——保留原名,
/// 占名遇 AlreadyExists 时删除旧文件后重试一次;目录/删除失败仍走既有
/// 失败路径(保留 .ts)。
///
/// `is_fmp4`(#682):播放列表带 EXT-X-MAP,`.ts` 里装的已经是分片 MP4
/// (ftyp+moov+[moof+mdat]*),不做 TS→MP4 转换,跳过体积/空间预检,只按同一
/// 套 dedup + 原子占名协议把文件改名为 `.mp4`;失败同样保留 `.ts`。
async fn remux_ts_to_mp4(
    ts_path: &std::path::Path,
    task_id: &str,
    allow_overwrite: bool,
    is_fmp4: bool,
) -> Option<PathBuf> {
    let ext = ts_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if !ext.eq_ignore_ascii_case("ts") {
        return None;
    }

    let file_len = match tokio::fs::metadata(ts_path).await {
        Ok(m) => m.len(),
        Err(_) => return None,
    };
    if !is_fmp4 && file_len > MAX_REMUX_BYTES {
        log_info!(
            "[hls] task {} skipping TS→MP4 remux: file is {} bytes (limit {}), keeping .ts",
            task_id,
            file_len,
            MAX_REMUX_BYTES
        );
        return None;
    }

    let parent = ts_path.parent()?;

    // ENOSPC 预检:空间不足时跳过 remux,走既有"保留 .ts"降级路径。
    // fMP4 只改名,不占额外空间,跳过。
    let avail = if is_fmp4 {
        None
    } else {
        crate::disk_space::available_space_checked(parent.to_path_buf()).await
    };
    if !is_fmp4 && !remux_space_ok(avail, file_len) {
        log_info!(
            "[hls] task {} skipping TS→MP4 remux: insufficient disk space (avail={:?}, need {}+margin), keeping .ts",
            task_id,
            avail,
            file_len
        );
        return None;
    }
    let stem = ts_path.file_stem().and_then(|s| s.to_str())?;
    let desired_name = format!("{}.mp4", stem);
    let unique_name = dedup_filename(
        parent,
        &desired_name,
        &std::collections::HashSet::new(),
        &std::collections::HashSet::new(),
        allow_overwrite,
    )
    .await;
    let mp4_path = parent.join(&unique_name);

    let ts_owned = ts_path.to_owned();
    let mp4_owned = mp4_path.clone();
    let mp4_tmp = mp4_path.with_extension("mp4.tmp");
    let mp4_tmp_inner = mp4_tmp.clone();

    match tokio::task::spawn_blocking(move || -> Result<(), std::io::Error> {
        // fMP4:待改名的源就是 .ts 本身;否则转换产物先落 tmp。
        let src = if is_fmp4 {
            ts_owned.clone()
        } else {
            let ts_data = std::fs::read(&ts_owned)?;
            let mp4_data = ts2mp4::convert_ts_to_mp4(&ts_data)?;
            drop(ts_data);
            std::fs::write(&mp4_tmp_inner, &mp4_data)?;
            drop(mp4_data);
            mp4_tmp_inner.clone()
        };
        // 原子占名(同 `downloader::claim_rename` 协议,此处在阻塞线程内走
        // 同步 API):create_new 独占创建占位——dedup 与落盘之间若有并发
        // 写者(同名 HTTP/BT 任务完成)抢得该名,后到者得 AlreadyExists,
        // 决不覆盖;rename 覆盖的是自己的占位。失败清理占位与 tmp。
        if let Err(e) = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&mp4_owned)
            .map(drop)
        {
            // overwrite 模式:dedup 对已存在的普通 mp4 保留了原名,占名必然
            // AlreadyExists——删除旧文件后重试占名一次(覆盖旧文件)。目标是
            // 目录、删除失败或二次抢占时维持既有失败路径(保留 .ts)。
            let overwrote = allow_overwrite
                && e.kind() == std::io::ErrorKind::AlreadyExists
                && mp4_owned.is_file()
                && std::fs::remove_file(&mp4_owned).is_ok()
                && std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&mp4_owned)
                    .map(drop)
                    .is_ok();
            if !overwrote {
                if !is_fmp4 {
                    let _ = std::fs::remove_file(&mp4_tmp_inner);
                }
                return Err(e);
            }
            log_info!(
                "[hls] overwrote existing '{}' (file_exists_behavior=overwrite)",
                mp4_owned.display()
            );
        }
        if let Err(e) = std::fs::rename(&src, &mp4_owned) {
            // 只清自己的占位与 tmp,fMP4 的源 .ts 是下载数据本身,绝不删。
            let _ = std::fs::remove_file(&mp4_owned);
            if !is_fmp4 {
                let _ = std::fs::remove_file(&mp4_tmp_inner);
            }
            return Err(e);
        }
        Ok(())
    })
    .await
    {
        Ok(Ok(())) => {
            if is_fmp4 {
                log_info!("[hls] task {} fMP4 output renamed .ts -> .mp4", task_id);
            } else {
                log_info!("[hls] task {} remuxed TS -> MP4", task_id);
            }
            Some(mp4_path)
        }
        Ok(Err(e)) => {
            log_info!(
                "[hls] task {} MP4 remux failed: {}, keeping .ts",
                task_id,
                e
            );
            let _ = tokio::fs::remove_file(&mp4_tmp).await;
            None
        }
        Err(e) => {
            log_info!(
                "[hls] task {} MP4 remux join error: {}, keeping .ts",
                task_id,
                e
            );
            let _ = tokio::fs::remove_file(&mp4_tmp).await;
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Per-segment download + decrypt (concurrency unit)
// ---------------------------------------------------------------------------

/// (seg_idx, 下载+解密结果, 并发许可)。许可随结果送回 writer,写盘后才释放。
type SegmentOutcome = (usize, Result<Vec<u8>, DownloadError>, OwnedSemaphorePermit);

struct SegmentJob {
    idx: usize,
    uri: String,
    byte_range: Option<(u64, u64)>,
    /// `Some((key_uri, iv))` 仅用于带非空密钥 URI 的 AES-128 段。
    key_info: Option<(String, Option<String>)>,
}

/// 所有段任务共享的只读上下文与句柄。
struct SegmentShared {
    client: Client,
    cookies: String,
    playlist_url: String,
    header_origin_url: String,
    referrer: String,
    extra_headers: HashMap<String, String>,
    cancel: tokio_util::sync::CancellationToken,
    task_id: String,
    key_cache: KeyCache,
    media_sequence: u64,
    tracker: TransferTracker,
    db: crate::db::Db,
    sink: Arc<dyn EventSink>,
}

impl SegmentShared {
    fn ctx(&self) -> HlsRequestCtx<'_> {
        HlsRequestCtx {
            cookies: &self.cookies,
            cookie_base_url: &self.playlist_url,
            header_origin_url: &self.header_origin_url,
            referrer: &self.referrer,
            extra_headers: &self.extra_headers,
        }
    }
}

/// 按 seg_idx 顺序获取许可并派发段任务。必须由单一任务顺序 `acquire`:
/// 若各段任务自行抢许可,首次 poll 顺序不保证等于 idx 顺序,许可可能全被
/// 后续段占住而队首段拿不到,writer 永远等不到它。
async fn dispatch_segments(
    jobs: Vec<SegmentJob>,
    shared: Arc<SegmentShared>,
    semaphore: Arc<Semaphore>,
    tx: mpsc::Sender<SegmentOutcome>,
) {
    let mut children = tokio::task::JoinSet::new();
    for job in jobs {
        let permit = tokio::select! {
            _ = shared.cancel.cancelled() => break,
            acquired = Arc::clone(&semaphore).acquire_owned() => match acquired {
                Ok(permit) => permit,
                // Semaphore closed — runtime shutting down; nothing to send.
                Err(_) => break,
            },
        };
        // 及时回收已结束的子任务,避免长播放列表下 JoinSet 累积。
        while children.try_join_next().is_some() {}
        let child_shared = Arc::clone(&shared);
        let child_tx = tx.clone();
        children.spawn(async move {
            let outcome = if child_shared.cancel.is_cancelled() {
                Err(DownloadError::Cancelled)
            } else {
                download_and_decrypt_segment(&child_shared, &job).await
            };
            // Always emit a result for this index so the in-order writer never
            // blocks forever waiting on a task that failed.
            let _ = child_tx.send((job.idx, outcome, permit)).await;
        });
    }
    drop(tx);
    while children.join_next().await.is_some() {}
}

/// 把非 HLS 播放列表错误转为真实失败:已确认是 HLS 后不允许退回普通 HTTP 下载。
fn reject_not_hls(e: DownloadError) -> DownloadError {
    match e {
        DownloadError::NotAnHlsPlaylist(msg) => {
            DownloadError::Other(format!("invalid HLS playlist: {msg}"))
        }
        other => other,
    }
}

/// Download a single segment (with retry) and, if encrypted, decrypt it.
///
/// This is the unit of work each concurrent task runs. It is purely
/// download + decrypt: it performs no disk writes, progress reporting, or
/// checkpointing — those stay on the single ordered writer so byte counts and
/// on-disk order remain correct under concurrency.
///
/// The IV is computed from this segment's own `idx` (`compute_default_iv`)
/// when not explicitly provided, so AES-128-CBC stays correct regardless of
/// the order tasks complete in.
async fn download_and_decrypt_segment(
    shared: &SegmentShared,
    job: &SegmentJob,
) -> Result<Vec<u8>, DownloadError> {
    let ctx = shared.ctx();
    let seg_data = download_segment_with_retry(
        &shared.client,
        &job.uri,
        job.byte_range,
        &ctx,
        &shared.cancel,
        &shared.task_id,
        job.idx,
        &shared.tracker,
        &shared.db,
        shared.sink.as_ref(),
    )
    .await?;

    let Some((key_uri, iv_str)) = job.key_info.as_ref() else {
        return Ok(seg_data);
    };

    // Fetch key (shared single-flight cache across all concurrent tasks).
    let key_bytes = fetch_key(
        &shared.client,
        key_uri,
        &ctx,
        &shared.key_cache,
        &shared.cancel,
    )
    .await?;

    // Determine IV — explicit IV from the playlist, else derived from this
    // segment's own index so concurrency never changes the IV.
    let iv = match iv_str {
        Some(iv_hex) => parse_iv_hex(iv_hex)?,
        None => compute_default_iv(shared.media_sequence, job.idx),
    };

    let mut data_buf = seg_data;
    decrypt_segment(&mut data_buf, &key_bytes, &iv, job.idx)
}

// ---------------------------------------------------------------------------
// Per-segment download with retry
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn download_segment_with_retry(
    client: &Client,
    url: &str,
    byte_range: Option<(u64, u64)>,
    ctx: &HlsRequestCtx<'_>,
    cancel_token: &tokio_util::sync::CancellationToken,
    task_id: &str,
    seg_idx: usize,
    tracker: &TransferTracker,
    db: &crate::db::Db,
    sink: &dyn EventSink,
) -> Result<Vec<u8>, DownloadError> {
    let transport = SegmentTransport {
        client,
        ctx,
        cancel_token,
        tracker,
        header_timeout: SEGMENT_HEADER_TIMEOUT,
        idle_timeout: SEGMENT_IDLE_TIMEOUT,
    };
    let mut attempts = 0u32;

    loop {
        match download_segment_once(&transport, url, byte_range, seg_idx).await {
            Ok(data) => return Ok(data),
            Err(DownloadError::Cancelled) => return Err(DownloadError::Cancelled),
            Err(e) => {
                attempts += 1;
                if attempts >= MAX_RETRIES {
                    return Err(DownloadError::Other(format!(
                        "HLS segment {} failed after {} retries: {}",
                        seg_idx, MAX_RETRIES, e
                    )));
                }
                log_info!(
                    "[hls-download] task {} segment {} attempt {}/{} failed: {}",
                    task_id,
                    seg_idx,
                    attempts,
                    MAX_RETRIES,
                    e
                );
                if let Err(journal_error) = crate::task_activity::record(
                    db,
                    sink,
                    task_id,
                    "retry",
                    format!(
                        "HLS 段 {seg_idx} 第 {attempts}/{MAX_RETRIES} 次尝试失败，即将重试：{e}"
                    ),
                    None,
                )
                .await
                {
                    crate::log_error!("[task-activity] failed to persist retry: {}", journal_error);
                }
                let delay = RETRY_BASE_DELAY * 2u32.saturating_pow(attempts - 1);
                tokio::select! {
                    _ = cancel_token.cancelled() => return Err(DownloadError::Cancelled),
                    _ = tokio::time::sleep(delay) => {}
                }
            }
        }
    }
}

struct SegmentTransport<'a> {
    client: &'a Client,
    ctx: &'a HlsRequestCtx<'a>,
    cancel_token: &'a tokio_util::sync::CancellationToken,
    tracker: &'a TransferTracker,
    /// 等待响应头的上限;超时按 stalled 报错,复用段重试与自动重试白名单。
    header_timeout: Duration,
    /// body 逐 chunk 空闲上限(不是整段超时:单段可达 256MB)。
    idle_timeout: Duration,
}

async fn download_segment_once(
    transport: &SegmentTransport<'_>,
    url: &str,
    byte_range: Option<(u64, u64)>,
    seg_idx: usize,
) -> Result<Vec<u8>, DownloadError> {
    let mut req = transport.ctx.get(transport.client, url);

    // EXT-X-BYTERANGE:同一 uri 的多段是底层大文件的不同子区间,必须发
    // `Range: bytes=offset-(offset+length-1)` 头只取本段区间。否则每段都拉整
    // 文件,N 段拼成 N 份完整副本(巨量损坏 + 撑爆磁盘)。range_end 用
    // checked 运算防溢出(offset 已在解析期与 length 一起校验过,这里再保一道)。
    if let Some((offset, length)) = byte_range {
        if length == 0 {
            return Err(DownloadError::Other(
                "EXT-X-BYTERANGE length must be > 0".to_string(),
            ));
        }
        let range_end = match offset.checked_add(length).and_then(|e| e.checked_sub(1)) {
            Some(end) => end,
            None => {
                return Err(DownloadError::Other(format!(
                    "EXT-X-BYTERANGE range overflow (offset={}, length={})",
                    offset, length
                )));
            }
        };
        req = req.header("Range", format!("bytes={}-{}", offset, range_end));
    }

    let resp = tokio::select! {
        _ = transport.cancel_token.cancelled() => return Err(DownloadError::Cancelled),
        r = tokio::time::timeout(transport.header_timeout, req.send()) => match r {
            Ok(r) => r?.error_for_status()?,
            Err(_) => {
                return Err(DownloadError::Other(format!(
                    "HLS segment {} stalled: no response headers within {:?}",
                    seg_idx, transport.header_timeout
                )));
            }
        },
    };

    // ranged 请求(EXT-X-BYTERANGE)必须得到 206 Partial Content。若服务器忽略
    // Range 头返回 200 全量,则收到的是整个底层文件而非本段子区间;放行会把
    // 整文件当成本段拼进输出造成损坏。故对 ranged 请求强制要求 206,否则报错
    // (触发上层重试,仍失败则整任务失败,绝不静默产出损坏文件)。
    if byte_range.is_some() && resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(DownloadError::Other(format!(
            "EXT-X-BYTERANGE 请求未返回 206 Partial Content (got {}); \
             服务器不支持 Range,无法正确切分子区间",
            resp.status()
        )));
    }

    // Transparently decompress if the server returned compressed content.
    let encoding = crate::downloader::detect_content_encoding(resp.headers());
    // 声明的 body 字节数(EOF 后据此做截断校验)。
    // - 对 ranged 请求:用 EXT-X-BYTERANGE 声明的 length 作为期望长度,而非
    //   响应的 Content-Length(206 的 Content-Length 是子区间长度,正常应相等,
    //   但以播放列表声明为准更稳妥),且 ranged 子区间通常未压缩。
    // - 对普通请求:仅当响应"无 Content-Encoding"时 content_length 才等于实际
    //   写入字节数;压缩响应解压后 buf 长度必然 != content_length(压缩后的值),
    //   故只对未压缩响应启用,避免误伤合法压缩分段。
    let declared_len = match byte_range {
        // byte_range 长度只在【未压缩】时等于落盘字节数；若服务器对 206 仍压缩
        // （正常不会，因强制 Accept-Encoding: identity），解压后长度 != 请求长度，
        // 会误报截断。与下方 None 分支一致地在有 encoding 时跳过大小校验。
        Some((_, length)) => {
            if encoding.is_none() {
                Some(length)
            } else {
                None
            }
        }
        None => {
            if encoding.is_none() {
                resp.content_length()
            } else {
                None
            }
        }
    };
    let raw_stream = resp.bytes_stream();
    let mut stream = crate::downloader::maybe_decompress_stream(raw_stream, encoding);

    /// Maximum allowed size for a single HLS segment (256 MB).
    /// Prevents OOM if a malicious or misconfigured server sends an oversized segment.
    const MAX_SEGMENT_BYTES: usize = 256 * 1024 * 1024;

    let mut buf = Vec::new();
    loop {
        let chunk = {
            let _transfer = transport.tracker.start(seg_idx as i32);
            tokio::select! {
                _ = transport.cancel_token.cancelled() => return Err(DownloadError::Cancelled),
                c = tokio::time::timeout(transport.idle_timeout, stream.next()) => match c {
                    Ok(c) => c,
                    Err(_) => {
                        return Err(DownloadError::Other(format!(
                            "HLS segment {} stalled: no data for {:?}",
                            seg_idx, transport.idle_timeout
                        )));
                    }
                },
            }
        };
        let Some(chunk_result) = chunk else {
            break;
        };
        let chunk_data = chunk_result.map_err(DownloadError::Io)?;
        if buf.len() + chunk_data.len() > MAX_SEGMENT_BYTES {
            return Err(DownloadError::Other(format!(
                "HLS segment too large: exceeds {} MB limit",
                MAX_SEGMENT_BYTES / (1024 * 1024)
            )));
        }
        buf.extend_from_slice(&chunk_data);
    }

    // 完整性校验：当有期望长度(普通请求的 Content-Length / ranged 请求的
    // EXT-X-BYTERANGE length)时,EOF 后实际字节必须恰好等于该值。服务器在分段
    // 中途关闭连接(TCP RST / chunked 提前 EOF)会让 stream 返回 None 被当作正常
    // 结束,只写入部分字节;不校验会把截断分段静默 append 进输出造成缺帧/花屏,
    // 而任务被标记完成。对 ranged 请求,收到字节数也据此对齐到本子区间长度
    // (而非整文件)。返回 Err 触发上层 download_segment_with_retry 重试。
    if let Some(expected) = declared_len
        && buf.len() as u64 != expected
    {
        return Err(DownloadError::Other(format!(
            "HLS segment truncated: got {} bytes, expected {}",
            buf.len(),
            expected
        )));
    }

    Ok(buf)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_HLS_CONCURRENCY, DownloadError, M3u8Content, MAX_HLS_CONCURRENCY,
        compute_default_iv, decrypt_segment, hls_concurrency, is_hls_url, parse_iv_hex,
        parse_m3u8_bytes, parse_resume_checkpoint, remux_space_ok, resolve_uri, should_write_init,
    };
    use aes::Aes128;
    use cbc::cipher::block_padding::{NoPadding, Pkcs7};
    use cbc::cipher::{BlockEncryptMut, KeyIvInit};

    type Aes128CbcEnc = cbc::Encryptor<Aes128>;

    #[tokio::test]
    async fn body_activity_excludes_response_setup_and_ends_at_eof()
    -> Result<(), Box<dyn std::error::Error>> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let address = listener.local_addr()?;
        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await?;
            let mut request = [0u8; 1024];
            let _ = socket.read(&mut request).await?;
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\n")
                .await?;
            let _ = release_rx.await;
            socket.write_all(b"hello").await?;
            Ok::<(), std::io::Error>(())
        });
        let tracker = super::TransferTracker::new();
        let client = reqwest::Client::builder().no_proxy().build()?;
        let cancel = tokio_util::sync::CancellationToken::new();
        let url = format!("http://{address}/segment.ts");
        let download = tokio::spawn({
            let tracker = tracker.clone();
            async move {
                let headers = std::collections::HashMap::new();
                let ctx = super::HlsRequestCtx {
                    cookies: "",
                    cookie_base_url: &url,
                    header_origin_url: &url,
                    referrer: "",
                    extra_headers: &headers,
                };
                let transport = super::SegmentTransport {
                    client: &client,
                    ctx: &ctx,
                    cancel_token: &cancel,
                    tracker: &tracker,
                    header_timeout: std::time::Duration::from_secs(30),
                    idle_timeout: std::time::Duration::from_secs(30),
                };
                super::download_segment_once(&transport, &url, None, 2).await
            }
        });
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while tracker.active() != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        assert!(tracker.is_active(2));
        assert!(release_tx.send(()).is_ok());
        assert_eq!(download.await??, b"hello");
        server.await??;
        assert_eq!(tracker.active(), 0);
        Ok(())
    }

    /// PKCS7-encrypt `plaintext` with the given key/iv, returning ciphertext.
    /// 返回 `None` 时由调用方断言失败,避免在测试中使用 `unwrap`/`expect`。
    fn encrypt_pkcs7(plaintext: &[u8], key: &[u8; 16], iv: &[u8; 16]) -> Option<Vec<u8>> {
        let enc = Aes128CbcEnc::new_from_slices(key, iv).ok()?;
        // 输出缓冲需容纳 padding(最多多一整块)。
        let mut buf = vec![0u8; plaintext.len() + 16];
        let ct = enc
            .encrypt_padded_b2b_mut::<Pkcs7>(plaintext, &mut buf)
            .ok()?;
        Some(ct.to_vec())
    }

    // ---------------------------------------------------------------------
    // remux_space_ok — ENOSPC 预检阈值(remux 期间 .ts 与 .mp4 并存 ≈ 2x)。
    // ---------------------------------------------------------------------

    #[test]
    fn remux_space_ok_thresholds() {
        const MARGIN: u64 = crate::disk_space::PRECHECK_MARGIN;
        let file_len = 100 * 1024 * 1024u64;
        // 无法探测(网络盘/超时)→ 乐观放行。
        assert!(remux_space_ok(None, file_len));
        // 恰好够(file_len + margin)→ 放行。
        assert!(remux_space_ok(Some(file_len + MARGIN), file_len));
        // 差 1 字节 → 拒绝。
        assert!(!remux_space_ok(Some(file_len + MARGIN - 1), file_len));
        // 溢出安全:file_len 接近 u64::MAX 时 saturating_add 不回绕。
        assert!(!remux_space_ok(Some(u64::MAX - 1), u64::MAX));
    }

    /// No-padding encrypt (input must be block-aligned), returning ciphertext.
    fn encrypt_nopad(plaintext: &[u8], key: &[u8; 16], iv: &[u8; 16]) -> Option<Vec<u8>> {
        let enc = Aes128CbcEnc::new_from_slices(key, iv).ok()?;
        let mut buf = plaintext.to_vec();
        let len = buf.len();
        let ct = enc.encrypt_padded_mut::<NoPadding>(&mut buf, len).ok()?;
        Some(ct.to_vec())
    }

    #[test]
    fn test_is_hls_url_m3u8() {
        assert!(is_hls_url("https://example.com/stream.m3u8"));
        assert!(is_hls_url("https://example.com/stream.M3U8"));
        assert!(is_hls_url("https://example.com/stream.m3u8?token=abc"));
        assert!(is_hls_url("https://example.com/path/index.m3u8#fragment"));
    }

    #[test]
    fn test_is_hls_url_m3u() {
        assert!(is_hls_url("https://example.com/stream.m3u"));
        assert!(is_hls_url("https://example.com/stream.M3U"));
    }

    #[test]
    fn test_is_hls_url_not_hls() {
        assert!(!is_hls_url("https://example.com/video.mp4"));
        assert!(!is_hls_url("https://example.com/stream.mpd"));
        assert!(!is_hls_url("https://example.com/file.ts"));
    }

    #[test]
    fn test_parse_m3u8_bytes_rejects_non_playlist_content() {
        // 拿 .m3u8 扩展名装普通文件列表的场景（WebDAV/网盘把曲目清单恰好命
        // 名为 .m3u8，无 #EXTM3U 头）：分类必须是 NotAnHlsPlaylist 而非泛化
        // 的 Other，这样调用方（`run_hls_download`）才能把它与"内容合法但
        // 下游步骤失败"的真实 HLS 错误区分开，退回普通 HTTP 下载器而不是
        // 上报终态失败。
        let content = "01. \u{9727}\u{6708}\u{306f}\u{308b}\u{304b} - break time.flac\r\n\
                        02. \u{9727}\u{6708}\u{306f}\u{308b}\u{304b} - vocal off.flac\r\n"
            .as_bytes();
        let err = match parse_m3u8_bytes("https://example.com/list.m3u8", content) {
            Ok(_) => panic!("non-playlist content must not parse as a valid M3U8 playlist"),
            Err(e) => e,
        };
        assert!(
            matches!(err, DownloadError::NotAnHlsPlaylist(_)),
            "expected NotAnHlsPlaylist, got {err:?}"
        );
    }

    #[test]
    fn test_parse_m3u8_bytes_accepts_valid_playlist() {
        // 对照组：合法播放列表必须继续正常解析，不能被误分类为
        // NotAnHlsPlaylist。
        let playlist = "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:6\n\
                         #EXTINF:6.000,\nseg0.ts\n#EXT-X-ENDLIST\n";
        assert!(parse_m3u8_bytes("https://example.com/list.m3u8", playlist.as_bytes()).is_ok());
    }

    #[test]
    fn test_resolve_uri_absolute() {
        assert_eq!(
            resolve_uri(
                "https://cdn.example.com/live/master.m3u8",
                "https://other.com/seg.ts"
            ),
            "https://other.com/seg.ts"
        );
    }

    #[test]
    fn test_resolve_uri_relative() {
        assert_eq!(
            resolve_uri("https://cdn.example.com/live/master.m3u8", "segment0.ts"),
            "https://cdn.example.com/live/segment0.ts"
        );
    }

    #[test]
    fn test_resolve_uri_absolute_path() {
        assert_eq!(
            resolve_uri("https://cdn.example.com/live/master.m3u8", "/data/seg.ts"),
            "https://cdn.example.com/data/seg.ts"
        );
    }

    #[test]
    fn test_parse_iv_hex_with_prefix() {
        let iv = parse_iv_hex("0x00000000000000000000000000000001").unwrap_or([0; 16]);
        let mut expected = [0u8; 16];
        expected[15] = 1;
        assert_eq!(iv, expected);
    }

    #[test]
    fn test_parse_iv_hex_without_prefix() {
        let iv = parse_iv_hex("00000000000000000000000000000002").unwrap_or([0; 16]);
        let mut expected = [0u8; 16];
        expected[15] = 2;
        assert_eq!(iv, expected);
    }

    #[test]
    fn test_compute_default_iv() {
        let iv = compute_default_iv(0, 0);
        assert_eq!(iv, [0u8; 16]);

        let iv = compute_default_iv(0, 1);
        let mut expected = [0u8; 16];
        expected[15] = 1;
        assert_eq!(iv, expected);

        let iv = compute_default_iv(100, 5);
        let mut expected = [0u8; 16];
        let seq: u64 = 105;
        expected[8..16].copy_from_slice(&seq.to_be_bytes());
        assert_eq!(iv, expected);
    }

    #[test]
    fn test_is_hls_ftp_m3u8() {
        // FTP URL with .m3u8 extension — still detected as HLS
        assert!(is_hls_url("ftp://example.com/stream.m3u8"));
    }

    // --- F036: decrypt_segment padding handling ---

    #[test]
    fn test_decrypt_segment_pkcs7_roundtrip() {
        // 块对齐明文经 PKCS7 加密后,decrypt_segment 应正确解出原文。
        let key = [0x11u8; 16];
        let iv = [0x22u8; 16];
        let plaintext = b"hello world, hls!".to_vec(); // 17 bytes -> padded to 32
        let Some(mut ct) = encrypt_pkcs7(&plaintext, &key, &iv) else {
            panic!("test fixture encryption failed");
        };
        assert_eq!(ct.len() % 16, 0, "pkcs7 ciphertext must be block-aligned");
        let out = decrypt_segment(&mut ct, &key, &iv, 0);
        match out {
            Ok(decoded) => assert_eq!(decoded, plaintext),
            Err(e) => panic!("pkcs7 decrypt should succeed: {e}"),
        }
    }

    #[test]
    fn test_decrypt_segment_nopadding_when_unaligned() {
        // 源省略填充导致密文非块对齐:decrypt_segment 应走 NoPadding 解出
        // 对齐前缀,丢弃尾部不足一块的残余字节,而非整体失败(F036 核心)。
        let key = [0x33u8; 16];
        let iv = [0x44u8; 16];
        let plaintext = [0xABu8; 48]; // 3 blocks, no padding
        let Some(ct_aligned) = encrypt_nopad(&plaintext, &key, &iv) else {
            panic!("test fixture nopadding encryption failed");
        };
        // 追加 5 字节"残余",模拟非块对齐密文(总长 53)。
        let mut ct = ct_aligned.clone();
        ct.extend_from_slice(&[0x99u8; 5]);
        assert_ne!(ct.len() % 16, 0, "fixture must be unaligned");

        let out = decrypt_segment(&mut ct, &key, &iv, 7);
        match out {
            // 仅解出对齐前缀(48 字节),尾部 5 字节被丢弃。
            Ok(decoded) => assert_eq!(decoded, plaintext.to_vec()),
            Err(e) => panic!("nopadding fallback should succeed for unaligned data: {e}"),
        }
    }

    #[test]
    fn test_decrypt_segment_aligned_wrong_key_errors() {
        // 块对齐但 PKCS7 解密失败(错误密钥)时,必须报错而非 fallback,
        // 避免掩盖真实解密失败(F036 安全约束)。
        let key = [0x55u8; 16];
        let iv = [0x66u8; 16];
        let plaintext = b"some aligned data here padded".to_vec();
        let Some(mut ct) = encrypt_pkcs7(&plaintext, &key, &iv) else {
            panic!("test fixture encryption failed");
        };
        let wrong_key = [0x00u8; 16];
        // 用错误密钥解密块对齐数据:绝大多数情况下 PKCS7 校验失败。
        let out = decrypt_segment(&mut ct, &wrong_key, &iv, 3);
        // 不强制必然 Err(理论上极小概率出现"看似合法"的尾字节),但若 Err
        // 必须携带段索引以便诊断;此处主要验证不会 panic 且未走 NoPadding
        // 静默通过——只要是 Ok 也应解码为非原文。
        match out {
            Err(e) => assert!(
                e.to_string().contains("segment 3"),
                "error must carry segment index for diagnostics: {e}"
            ),
            Ok(decoded) => assert_ne!(decoded, plaintext),
        }
    }

    #[test]
    fn test_decrypt_segment_too_short_errors() {
        // 不足一个完整块(< 16 字节)无法解密,应返回携带段索引的错误。
        let key = [0x77u8; 16];
        let iv = [0x88u8; 16];
        let mut data = vec![0x01u8; 10];
        let out = decrypt_segment(&mut data, &key, &iv, 9);
        match out {
            Err(e) => assert!(e.to_string().contains("segment 9"), "got: {e}"),
            Ok(_) => panic!("data shorter than one AES block must error"),
        }
    }

    #[test]
    fn test_decrypt_segment_empty_is_ok_empty() {
        // BUG-HLS-EMPTY-SEGMENT-PKCS7:加密段下载到 0 字节时必须短路返回空,
        // 而非走 PKCS7 分支报 UnpadError 把整个下载当永久失败中止。
        let key = [0x12u8; 16];
        let iv = [0x34u8; 16];
        let mut data: Vec<u8> = Vec::new();
        match decrypt_segment(&mut data, &key, &iv, 0) {
            Ok(out) => assert!(out.is_empty(), "empty ciphertext must decrypt to empty"),
            Err(e) => panic!("empty input must not error: {e}"),
        }
    }

    // --- BUG-HLS-MEDIASEQ-OVERFLOW: saturating IV sequence ---

    #[test]
    fn test_compute_default_iv_saturates_no_overflow() {
        // media_sequence 接近 u64::MAX 时,序号加法必须饱和而非 panic/回绕。
        let iv = compute_default_iv(u64::MAX, 5);
        let mut expected = [0u8; 16];
        // 饱和到 u64::MAX → 低 8 字节全 0xFF。
        expected[8..16].copy_from_slice(&u64::MAX.to_be_bytes());
        assert_eq!(iv, expected);
    }

    // --- F040: resume checkpoint parsing (backward compatibility) ---

    #[test]
    fn test_parse_resume_checkpoint_three_fields() {
        assert_eq!(parse_resume_checkpoint("5:1024:42"), (5, 1024, Some(42)));
    }

    #[test]
    fn test_parse_resume_checkpoint_two_fields_legacy() {
        // 旧格式无 media_sequence -> None。
        assert_eq!(parse_resume_checkpoint("3:512"), (3, 512, None));
    }

    #[test]
    fn test_parse_resume_checkpoint_idx_only_legacy() {
        // 更早格式仅有 idx -> byte_offset 视为 0,media_sequence 未知。
        assert_eq!(parse_resume_checkpoint("7"), (7, 0, None));
    }

    #[test]
    fn test_parse_resume_checkpoint_garbage() {
        // 完全无法解析 -> (0, 0, None),等同于不 resume。
        assert_eq!(parse_resume_checkpoint("not-a-number"), (0, 0, None));
        assert_eq!(parse_resume_checkpoint(""), (0, 0, None));
    }

    // --- F016: relative URI resolution against (redirect-final) base ---

    #[test]
    fn test_resolve_uri_cross_host_base() {
        // media playlist 重定向到 CDN 后,相对段 URI 应拼到 CDN 主机。
        assert_eq!(
            resolve_uri("https://cdn.example.com/path/media.m3u8", "seg1.ts"),
            "https://cdn.example.com/path/seg1.ts"
        );
    }

    // --- #275: concurrency selection bounds ---

    #[test]
    fn test_hls_concurrency_auto_uses_default() {
        // segment_count <= 0 means "auto": fall back to DEFAULT, but never
        // exceed the number of remaining segments.
        assert_eq!(hls_concurrency(0, 100), DEFAULT_HLS_CONCURRENCY);
        assert_eq!(hls_concurrency(-1, 100), DEFAULT_HLS_CONCURRENCY);
        assert_eq!(hls_concurrency(0, 3), 3);
    }

    #[test]
    fn test_hls_concurrency_respects_user_value() {
        assert_eq!(hls_concurrency(4, 100), 4);
        assert_eq!(hls_concurrency(1, 100), 1);
    }

    #[test]
    fn test_hls_concurrency_clamped_to_max() {
        // Never exceed the connection-pool ceiling even if the user asks for more.
        assert_eq!(hls_concurrency(999, 100), MAX_HLS_CONCURRENCY);
        assert_eq!(hls_concurrency(i32::MAX, 100), MAX_HLS_CONCURRENCY);
    }

    #[test]
    fn test_hls_concurrency_never_below_one() {
        // Even with zero remaining (shouldn't happen — guarded earlier), the
        // semaphore must be created with at least one permit.
        assert_eq!(hls_concurrency(8, 0), 1);
        assert_eq!(hls_concurrency(0, 1), 1);
    }

    #[test]
    fn test_hls_concurrency_capped_by_remaining() {
        // Spawning more workers than segments left is wasteful; cap at remaining.
        assert_eq!(hls_concurrency(16, 5), 5);
        assert_eq!(hls_concurrency(8, 2), 2);
    }

    // --- #682: EXT-X-MAP (fMP4/CMAF init segment) support ---

    #[test]
    fn test_parse_m3u8_bytes_without_map_is_ts() {
        // Baseline regression: plain MPEG-TS media playlists (no EXT-X-MAP)
        // must keep parsing with `has_map=false` and no `map` on any segment.
        let base_url = "https://cdn.example.com/live/media.m3u8";
        let playlist = "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:6\n\
#EXT-X-MEDIA-SEQUENCE:0\n#EXTINF:6.000,\nseg0.ts\n#EXT-X-ENDLIST\n";
        let content = match parse_m3u8_bytes(base_url, playlist.as_bytes()) {
            Ok(c) => c,
            Err(e) => panic!("plain TS playlist must parse, got error: {e}"),
        };
        match content {
            M3u8Content::Media {
                segments, has_map, ..
            } => {
                assert!(!has_map, "no EXT-X-MAP present -> has_map must be false");
                assert!(segments[0].map.is_none());
            }
            M3u8Content::Master { .. } => panic!("expected media playlist"),
        }
    }

    #[test]
    fn test_parse_m3u8_bytes_ext_x_map_sticky_across_segments() {
        // EXT-X-MAP must no longer be rejected (#682) and must stick to every
        // segment that follows it (RFC 8216 §4.3.2.4), not just the segment the
        // tag is textually attached to.
        let base_url = "https://cdn.example.com/live/media.m3u8";
        let playlist = "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:6\n\
#EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-MAP:URI=\"init.mp4\"\n\
#EXTINF:6.000,\nseg0.m4s\n#EXTINF:6.000,\nseg1.m4s\n#EXT-X-ENDLIST\n";
        let content = match parse_m3u8_bytes(base_url, playlist.as_bytes()) {
            Ok(c) => c,
            Err(e) => panic!("EXT-X-MAP playlist must parse, got error: {e}"),
        };
        match content {
            M3u8Content::Media {
                segments, has_map, ..
            } => {
                assert!(has_map);
                assert_eq!(segments.len(), 2);
                let expected_uri = "https://cdn.example.com/live/init.mp4";
                for (i, seg) in segments.iter().enumerate() {
                    match &seg.map {
                        Some(m) => {
                            assert_eq!(m.uri, expected_uri, "segment {i} map uri mismatch");
                            assert_eq!(m.byte_range, None);
                        }
                        None => panic!("segment {i} must carry the sticky EXT-X-MAP"),
                    }
                }
                // Stickiness: both segments resolve to the identical map.
                assert!(
                    segments[0].map == segments[1].map,
                    "map must be identical across segments sharing one EXT-X-MAP tag"
                );
            }
            M3u8Content::Master { .. } => panic!("expected media playlist"),
        }
    }

    #[test]
    fn test_parse_m3u8_bytes_ext_x_map_byte_range() {
        // EXT-X-MAP's BYTERANGE attribute uses the same `<length>@<offset>`
        // grammar as EXT-X-BYTERANGE; unlike segment byte-ranges, a missing
        // offset means 0 (no cross-tag implicit chaining for init segments).
        let base_url = "https://cdn.example.com/live/media.m3u8";
        let playlist = "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:6\n\
#EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-MAP:URI=\"init.mp4\",BYTERANGE=\"800@100\"\n\
#EXTINF:6.000,\nseg0.m4s\n#EXT-X-ENDLIST\n";
        let content = match parse_m3u8_bytes(base_url, playlist.as_bytes()) {
            Ok(c) => c,
            Err(e) => panic!("EXT-X-MAP with BYTERANGE must parse, got error: {e}"),
        };
        match content {
            M3u8Content::Media { segments, .. } => match &segments[0].map {
                Some(m) => assert_eq!(m.byte_range, Some((100, 800)), "(offset, length)"),
                None => panic!("segment must carry map"),
            },
            M3u8Content::Master { .. } => panic!("expected media playlist"),
        }
    }

    // --- #682: writer-side init-segment prepend decision ---

    #[test]
    fn test_should_write_init() {
        let map_a = ("https://cdn.example.com/init.mp4".to_string(), None);
        let map_b = (
            "https://cdn.example.com/init2.mp4".to_string(),
            Some((0u64, 100u64)),
        );

        // MPEG-TS segment (no map in effect) never needs an init write.
        assert!(!should_write_init(None, None));
        assert!(!should_write_init(Some(&map_a), None));

        // First appearance of a map (nothing written to disk yet) -> write it.
        assert!(should_write_init(None, Some(&map_a)));

        // Same map as the one already on disk -> skip, avoid duplicating bytes.
        assert!(!should_write_init(Some(&map_a), Some(&map_a)));

        // Playlist switched to a different init segment -> write again.
        assert!(should_write_init(Some(&map_a), Some(&map_b)));
    }

    use super::{
        HlsRequestCtx, HlsVariant, SegmentTransport, VariantId, extra_headers_for_origin,
        format_resume_checkpoint, match_saved_variant, parse_checkpoint_variant, resume_is_usable,
    };

    fn variant(bandwidth: u64, resolution: Option<(u64, u64)>, uri: &str) -> HlsVariant {
        HlsVariant {
            bandwidth,
            resolution,
            uri: uri.to_string(),
            audio_uri: None,
        }
    }

    #[test]
    fn parse_iv_hex_rejects_multibyte_without_panic() {
        // 32 字节但含多字节字符:按字节切片会落在字符中间。
        let iv = format!("0中{}", "a".repeat(28));
        assert_eq!(iv.len(), 32);
        assert!(parse_iv_hex(&iv).is_err());
    }

    #[test]
    fn checkpoint_roundtrips_selected_variant_and_stays_backward_compatible() {
        let v = variant(
            2_000_000,
            Some((1280, 720)),
            "https://cdn.example.com/v/720/index.m3u8?token=abc",
        );
        let id = VariantId::of(&v);
        assert_eq!(id.path, "/v/720/index.m3u8");
        let cp = format_resume_checkpoint(5, 1024, 0, Some(&id.encode()));
        assert_eq!(parse_resume_checkpoint(&cp), (5, 1024, Some(0)));
        assert_eq!(parse_checkpoint_variant(&cp), Some(id));
        // 旧格式没有变体段。
        assert_eq!(parse_checkpoint_variant("5:1024:0"), None);
        assert_eq!(parse_checkpoint_variant("5:1024"), None);
        // 无分辨率的变体同样可往返。
        let no_res = VariantId::of(&variant(1, None, "https://a.example/x.m3u8"));
        assert_eq!(VariantId::decode(&no_res.encode()), Some(no_res));
    }

    #[test]
    fn match_saved_variant_survives_rotated_signature_token() {
        let saved = VariantId::of(&variant(
            2_000_000,
            Some((1280, 720)),
            "https://a.example/720/i.m3u8?token=old",
        ));
        let variants = [
            variant(
                800_000,
                Some((640, 360)),
                "https://a.example/360/i.m3u8?token=new",
            ),
            variant(
                2_000_000,
                Some((1280, 720)),
                "https://a.example/720/i.m3u8?token=new",
            ),
        ];
        assert_eq!(match_saved_variant(&variants, &saved), Some(1));
    }

    #[test]
    fn match_saved_variant_rejects_ambiguous_or_missing() {
        let variants = [
            variant(2_000_000, Some((1280, 720)), "https://a.example/a/x.m3u8"),
            variant(2_000_000, Some((1280, 720)), "https://a.example/b/y.m3u8"),
        ];
        // 画质相同无法区分、path 也对不上 -> 不能猜。
        let ambiguous = VariantId {
            bandwidth: 2_000_000,
            resolution: Some((1280, 720)),
            path: "/c/z.m3u8".to_string(),
        };
        assert_eq!(match_saved_variant(&variants, &ambiguous), None);
        // 画质变了但 path 唯一对应 -> 按 path 找回。
        let by_path = VariantId {
            bandwidth: 1,
            resolution: None,
            path: "/b/y.m3u8".to_string(),
        };
        assert_eq!(match_saved_variant(&variants, &by_path), Some(1));
        let missing = VariantId {
            bandwidth: 1,
            resolution: None,
            path: "/nope.m3u8".to_string(),
        };
        assert_eq!(match_saved_variant(&variants, &missing), None);
    }

    #[test]
    fn resume_is_usable_guards_prefix_validity() {
        assert!(resume_is_usable(3, 100, 120, 10, false, false));
        // 检查点段数超过新播放列表:前缀无法对齐。
        assert!(!resume_is_usable(11, 100, 120, 10, false, false));
        assert!(resume_is_usable(10, 100, 120, 10, false, false));
        assert!(!resume_is_usable(0, 100, 120, 10, false, false));
        assert!(!resume_is_usable(3, 0, 120, 10, false, false));
        assert!(!resume_is_usable(3, 100, 0, 10, false, false));
        assert!(!resume_is_usable(3, 100, 120, 10, true, false));
        assert!(!resume_is_usable(3, 100, 120, 10, false, true));
    }

    #[test]
    fn credential_headers_only_go_to_manifest_origin() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer t".to_string());
        headers.insert("cookie".to_string(), "a=b".to_string());
        headers.insert("Proxy-Authorization".to_string(), "Basic x".to_string());
        headers.insert("X-Token".to_string(), "keep".to_string());
        let manifest = "https://site.example/master.m3u8";

        let same = extra_headers_for_origin(manifest, "https://site.example/seg0.ts", &headers);
        assert_eq!(same.len(), 4);

        for other in [
            "https://cdn.example/seg0.ts",
            "http://site.example/seg0.ts",
            "https://site.example:8443/seg0.ts",
        ] {
            let filtered = extra_headers_for_origin(manifest, other, &headers);
            assert_eq!(filtered.len(), 1, "{other}");
            assert_eq!(filtered.get("X-Token").map(String::as_str), Some("keep"));
        }
    }

    #[test]
    fn playlist_with_bom_and_leading_whitespace_parses() {
        let playlist = b"\xEF\xBB\xBF\n  #EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXTINF:6.0,\nseg0.ts\n#EXT-X-ENDLIST\n";
        assert!(parse_m3u8_bytes("https://example.com/a.m3u8", playlist).is_ok());
    }

    #[test]
    fn media_playlist_live_flag() {
        let base = "https://example.com/a.m3u8";
        let live_of = |body: &str| match parse_m3u8_bytes(base, body.as_bytes()) {
            Ok(M3u8Content::Media { live, .. }) => live,
            _ => panic!("expected media playlist"),
        };
        let head = "#EXTM3U\n#EXT-X-TARGETDURATION:6\n";
        let seg = "#EXTINF:6.0,\nseg0.ts\n";
        assert!(live_of(&format!("{head}{seg}")));
        assert!(!live_of(&format!("{head}{seg}#EXT-X-ENDLIST\n")));
        assert!(!live_of(&format!("{head}#EXT-X-PLAYLIST-TYPE:VOD\n{seg}")));
        assert!(live_of(&format!("{head}#EXT-X-PLAYLIST-TYPE:EVENT\n{seg}")));
    }

    #[test]
    fn master_skips_iframe_variants_and_reports_external_audio() {
        let master = "#EXTM3U\n\
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"en\",DEFAULT=YES,URI=\"audio/en.m3u8\"\n\
#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360,AUDIO=\"aud\"\n\
v360.m3u8\n\
#EXT-X-I-FRAME-STREAM-INF:BANDWIDTH=9000000,RESOLUTION=1920x1080,URI=\"iframe.m3u8\"\n";
        match parse_m3u8_bytes("https://example.com/m.m3u8", master.as_bytes()) {
            Ok(M3u8Content::Master { variants }) => {
                assert_eq!(variants.len(), 1, "I-frame variant must be skipped");
                assert_eq!(variants[0].bandwidth, 800_000);
                assert_eq!(
                    variants[0].audio_uri.as_deref(),
                    Some("https://example.com/audio/en.m3u8")
                );
            }
            _ => panic!("expected master playlist"),
        }
    }

    /// 起一个只接受一次连接的服务器:`send_headers` 为 true 时先发响应头和部分
    /// body 再挂住,否则读完请求后什么都不发;返回地址与释放信号。
    async fn stalling_server(
        send_headers: bool,
    ) -> Result<(std::net::SocketAddr, tokio::sync::oneshot::Sender<()>), std::io::Error> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let address = listener.local_addr()?;
        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut request = [0u8; 1024];
            let _ = socket.read(&mut request).await;
            if send_headers {
                let _ = socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nhello",
                    )
                    .await;
            }
            let _ = release_rx.await;
        });
        Ok((address, release_tx))
    }

    async fn stalled_segment_error(
        send_headers: bool,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let (address, release_tx) = stalling_server(send_headers).await?;
        let client = reqwest::Client::builder().no_proxy().build()?;
        let tracker = super::TransferTracker::new();
        let cancel = tokio_util::sync::CancellationToken::new();
        let url = format!("http://{address}/segment.ts");
        let headers = std::collections::HashMap::new();
        let ctx = HlsRequestCtx {
            cookies: "",
            cookie_base_url: &url,
            header_origin_url: &url,
            referrer: "",
            extra_headers: &headers,
        };
        let transport = SegmentTransport {
            client: &client,
            ctx: &ctx,
            cancel_token: &cancel,
            tracker: &tracker,
            header_timeout: std::time::Duration::from_millis(200),
            idle_timeout: std::time::Duration::from_millis(200),
        };
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            super::download_segment_once(&transport, &url, None, 0),
        )
        .await?;
        let _ = release_tx.send(());
        match result {
            Err(DownloadError::Other(message)) => Ok(message),
            other => Err(format!("expected stalled error, got ok={}", other.is_ok()).into()),
        }
    }

    #[tokio::test]
    async fn segment_body_stall_is_reported_as_stalled() -> Result<(), Box<dyn std::error::Error>> {
        let message = stalled_segment_error(true).await?;
        assert!(message.contains("stalled"), "{message}");
        Ok(())
    }

    #[tokio::test]
    async fn segment_header_stall_is_reported_as_stalled() -> Result<(), Box<dyn std::error::Error>>
    {
        let message = stalled_segment_error(false).await?;
        assert!(message.contains("stalled"), "{message}");
        Ok(())
    }
}
