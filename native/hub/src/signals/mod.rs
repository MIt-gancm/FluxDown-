use rinf::{DartSignal, RustSignal, SignalPiece};
use serde::{Deserialize, Serialize};

// ========== Dart → Rust signals ==========

/// Create a new download task
#[derive(Deserialize, DartSignal)]
pub struct CreateTask {
    pub url: String,
    pub save_dir: String,
    pub file_name: String, // empty = auto detect from server
    pub segments: i32,     // 0 = auto (default 8)
    #[serde(default)]
    pub cookies: String, // browser cookies for authenticated downloads
    /// Raw .torrent file bytes (base64-decoded by Dart before sending).
    /// When non-empty, this takes priority over `url` for BT downloads.
    #[serde(default)]
    pub torrent_file_bytes: Vec<u8>,
    /// Per-task proxy URL override (e.g. "socks5://user:pass@host:port").
    /// Empty = use global proxy setting.
    #[serde(default)]
    pub proxy_url: String,
    /// Per-task user-agent override. Empty = use global UA setting.
    #[serde(default)]
    pub user_agent: String,
    /// Named queue ID to assign this task to. Empty = default queue.
    #[serde(default)]
    pub queue_id: String,
    /// Checksum spec for integrity verification after download.
    /// Format: "algo=hexhash", e.g. "sha-256=abc123..." or "md5=d41d8c...".
    /// Empty = skip verification.
    #[serde(default)]
    pub checksum: String,
    /// Ignore HTTPS certificate errors for this task. Secure default: false.
    #[serde(default)]
    pub ignore_tls_errors: bool,
    /// Custom HTTP request headers (key/value) for this task.
    /// Cookie is handled separately via `cookies`; do not mix the two.
    /// Empty = no extra headers.
    #[serde(default)]
    pub extra_headers: std::collections::HashMap<String, String>,
    /// Pre-selected file indices for BT downloads (from the new-download dialog).
    /// When non-empty, Phase 3.5 will use these instead of waiting for a
    /// second file-selection dialog.
    /// Special value [-1] = user cancelled = task should abort immediately.
    /// Empty = no pre-selection (show the dialog after metadata resolves).
    #[serde(default)]
    pub selected_file_indices: Vec<i32>,
    /// 稍后下载：true = 建任务后不启动（paused 落库），待「启动队列」
    /// 按序恢复或用户手动恢复。
    #[serde(default)]
    pub start_paused: bool,
    /// HTTP Basic 认证用户名（空 = 未提供；非空时引擎注入
    /// `Authorization: Basic` 头，覆盖 extra_headers 里的同名头）。
    #[serde(default)]
    pub http_user: String,
    /// HTTP Basic 认证密码（仅 `http_user` 非空时有意义）。
    #[serde(default)]
    pub http_password: String,
    /// 为此网站保存凭据（按 host[:port] 存入 config，后续同站点任务
    /// 未显式提供凭据时自动套用）。
    #[serde(default)]
    pub save_site_auth: bool,
}

/// Control an existing task (pause/resume/cancel/delete/restart)
#[derive(Deserialize, DartSignal)]
pub struct ControlTask {
    pub task_id: String,
    /// 0=pause, 1=resume, 2=cancel, 3=delete(+files), 4=delete(record only),
    /// 5=restart（「重新下载」：丢弃磁盘产物与进度后从零重下；BT 不支持，
    /// 引擎侧直接忽略）。
    pub action: i32,
}

/// Batch control multiple tasks at once (pause/resume/delete).
/// Replaces N individual ControlTask IPC calls with a single signal.
#[derive(Deserialize, DartSignal)]
pub struct BatchControlTask {
    pub task_ids: Vec<String>,
    pub action: i32, // 0=pause, 1=resume, 3=delete(+files), 4=delete(record only)
}

/// Request all persisted tasks (sent on app startup)
#[derive(Deserialize, DartSignal)]
pub struct RequestAllTasks {}

// ========== Rust → Dart signals ==========

/// Task progress update — sent periodically during download
#[derive(Serialize, RustSignal)]
pub struct TaskProgress {
    pub task_id: String,
    pub status: i32, // 0=pending, 1=downloading, 2=paused, 3=completed, 4=error, 5=preparing
    pub downloaded_bytes: i64,
    pub total_bytes: i64,
    pub speed: i64, // bytes per second
    pub file_name: String,
    pub save_dir: String,
    pub url: String,
    pub error_message: String, // empty if no error
    /// 实时上传速率（字节/秒）。仅 BT 任务非零，其余协议恒 0。
    #[serde(default)]
    pub upload_speed_bps: i64,
    /// 已上传字节数（BT 做种）。仅 BT 任务有意义，默认 0。
    #[serde(default)]
    pub uploaded_bytes: i64,
    /// Seeding status: 0=none, 1=active seeding, 2=ratio reached,
    /// 3=time reached, 4=user stopped, 5=task deleted, 6=session released,
    /// 7=inactive time reached, 8=queued for a seeding slot.
    #[serde(default)]
    pub seeding_status: i32,
    /// BT 做种状态的辅助说明（如停止原因）。无错误/未做种时为空。
    #[serde(default)]
    pub seeding_message: String,
    /// 累计做种秒数（发帧时刻；排队/暂停不计）。仅 BT 任务有意义。
    #[serde(default)]
    pub seeding_time_secs: i64,
}

/// Response to RequestAllTasks — all persisted tasks
#[derive(Serialize, RustSignal)]
pub struct AllTasks {
    pub tasks: Vec<TaskInfo>,
}

/// Segment-level progress for download visualization (IDM-style)
#[derive(Serialize, RustSignal)]
pub struct SegmentProgress {
    pub task_id: String,
    pub total_bytes: i64,
    /// Number of segments (1 = single-thread download)
    pub segment_count: i32,
    pub segments: Vec<SegmentDetail>,
}

/// Per-segment byte range and progress
#[derive(Serialize, Deserialize, SignalPiece)]
pub struct SegmentDetail {
    pub index: i32,
    pub start_byte: i64,
    pub end_byte: i64,
    pub downloaded_bytes: i64,
}

// ========== External download signals (browser extension → app) ==========

/// Dart → Rust: user confirmed the external download request.
#[derive(Deserialize, DartSignal)]
pub struct ConfirmExternalDownload {
    pub url: String,
    pub save_dir: String,
    pub file_name: String, // empty = auto detect
    pub segments: i32,     // 0 = auto
    #[serde(default)]
    pub cookies: String, // browser cookies for authenticated downloads
    /// HTTP Referer header value captured by the browser extension.
    /// Empty = do not send Referer (e.g. manually added downloads).
    #[serde(default)]
    pub referrer: String,
    /// File size hint from the browser extension (bytes). 0 = unknown.
    /// When > 0, the downloader skips the probe phase (HEAD + Range:0-0)
    /// and uses this value as total_bytes directly.  This is critical for
    /// one-time CDN URLs (e.g. Lanzou) where extra probe requests would
    /// consume the URL token before the actual download begins.
    #[serde(default)]
    pub hint_file_size: i64,
    /// Per-task proxy URL override.
    /// Empty = use global proxy setting.
    #[serde(default)]
    pub proxy_url: String,
    /// Per-task user-agent override. Empty = use global UA setting.
    #[serde(default)]
    pub user_agent: String,
    /// Named queue ID. Empty = default queue.
    #[serde(default)]
    pub queue_id: String,
    /// Ignore HTTPS certificate errors for this confirmed task.
    #[serde(default)]
    pub ignore_tls_errors: bool,
    /// 音频轨 URL（通用「视频轨+音频轨」离散下载对语义）。
    /// 空 = 普通单 URL 下载；非空 = url 是视频轨，本字段是音频轨，
    /// create_task 尾参按此非空/空转换为 Some/None。
    #[serde(default)]
    pub audio_url: String,
    /// 用户在快速下载表单里手填的自定义请求头。与 Rust 侧按 URL 缓存的
    /// 浏览器捕获请求头合并，同名以用户手填值覆盖。
    #[serde(default)]
    pub extra_headers: std::collections::HashMap<String, String>,
    /// 稍后下载：true = 建任务后不启动（paused 落库）。
    #[serde(default)]
    pub start_paused: bool,
    /// HTTP Basic 认证用户名（快速下载表单手填；空 = 未提供）。
    #[serde(default)]
    pub http_user: String,
    /// HTTP Basic 认证密码（仅 `http_user` 非空时有意义）。
    #[serde(default)]
    pub http_password: String,
    /// 为此网站保存凭据。
    #[serde(default)]
    pub save_site_auth: bool,
    /// 无人值守创建（免打扰下载 + 「跳过二次选择」子开关）：跳过 BT 文件/
    /// HLS·DASH 画质/插件变体选择弹窗，直接按默认开始。用户点确认框的
    /// 路径恒 false（人在场，弹窗有意义）。
    #[serde(default)]
    pub unattended: bool,
}

// ========== Config signals ==========

/// Save a single config entry (Dart → Rust)
#[derive(Deserialize, DartSignal)]
pub struct SaveConfig {
    pub key: String,
    pub value: String,
}

/// Request all config entries (Dart → Rust, sent on app startup)
#[derive(Deserialize, DartSignal)]
pub struct RequestConfig {}

/// All config entries loaded from DB (Rust → Dart)
#[derive(Serialize, RustSignal)]
pub struct ConfigLoaded {
    pub entries: Vec<ConfigEntry>,
}

/// Single config key-value pair
#[derive(Serialize, Deserialize, SignalPiece)]
pub struct ConfigEntry {
    pub key: String,
    pub value: String,
}

/// Nested task info piece
#[derive(Clone, Serialize, Deserialize, SignalPiece)]
pub struct TaskInfo {
    pub task_id: String,
    pub url: String,
    pub file_name: String,
    pub save_dir: String,
    pub status: i32, // 0=pending, 1=downloading, 2=paused, 3=completed, 4=error, 5=preparing
    pub downloaded_bytes: i64,
    pub total_bytes: i64,
    pub error_message: String,
    pub created_at: String, // Unix seconds timestamp
    /// Per-task proxy URL (empty = global proxy).
    pub proxy_url: String,
    /// Named queue ID (empty = default queue).
    #[serde(default)]
    pub queue_id: String,
    /// Checksum spec for integrity verification (empty = skip).
    #[serde(default)]
    pub checksum: String,
    /// Whether this task explicitly accepts invalid HTTPS certificates.
    #[serde(default)]
    pub ignore_tls_errors: bool,
    /// 文件跟踪：completed 任务的目标文件是否已丢失（被删除/移动）。默认 false。
    #[serde(default)]
    pub file_missing: bool,
    /// 任务结束时间，Unix seconds 时间戳（空 = 尚未完成）。
    /// 记录下载真正完成（status→3）的时刻，不含插件 hook 后处理耗时。
    #[serde(default)]
    pub completed_at: String,
    /// 配置的分段（线程）数。0 = 自动。供 UI 展示与「改线程数」编辑。
    #[serde(default)]
    pub segments: i32,
    /// 队列内启动顺序（越小越先启动）。0 = 未显式排序（按创建时间）。
    #[serde(default)]
    pub queue_order: i32,
    /// Cumulative uploaded bytes for BT tasks (0 for other protocols).
    #[serde(default)]
    pub uploaded_bytes: i64,
    /// Uploaded bytes observed when the download completed (post-completion
    /// ratio baseline, 0 for non-BT tasks).
    #[serde(default)]
    pub uploaded_at_completion: i64,
    /// Seeding status: 0=none, 1=active seeding, 2=ratio reached,
    /// 3=time reached, 4=user stopped, 5=task deleted, 6=session released,
    /// 7=inactive time reached, 8=queued for a seeding slot.
    #[serde(default)]
    pub seeding_status: i32,
    /// Reason/description when seeding stopped (e.g. "ratio 1.5 reached").
    #[serde(default)]
    pub seeding_message: String,
    /// 累计做种秒数（活跃做种期间累加；排队/暂停不计）。
    #[serde(default)]
    pub seeding_time_secs: i64,
    /// 任务级总分享率上限（千分比：1500 = 1.5）。-2 = 跟随全局，
    /// -1 = 不限制，>=0 = 自定义（0 视同不限制）。字段恒由引擎侧
    /// `TaskInfo` 填充，`default` 仅为反序列化兜底。
    #[serde(default)]
    pub seed_ratio_limit_milli: i64,
    /// 任务级做种后分享率上限（千分比）。哨兵语义同上。
    #[serde(default)]
    pub seed_post_ratio_limit_milli: i64,
    /// 任务级做种时长上限（分钟）。哨兵语义同上。
    #[serde(default)]
    pub seed_time_limit_minutes: i64,
    /// 任务级不活跃做种时长上限（分钟）。哨兵语义同上。
    #[serde(default)]
    pub seed_inactive_time_limit_minutes: i64,
    /// 任务级做种上传限速（B/s，0 = 无单任务限制）。add 时烘焙生效，
    /// live 句柄不热改。
    #[serde(default)]
    pub seed_upload_limit_bps: i64,
    /// Source page URL captured by the browser extension (empty = none).
    #[serde(default)]
    pub referrer: String,
    /// 所属任务组 ID（空 = 不属于任何组）。
    #[serde(default)]
    pub group_id: String,
    /// 由哪条 RSS 订阅自动创建（空 = 非 RSS 来源），任务详情「来源」行用。
    #[serde(default)]
    pub rss_source_id: String,
    /// 展示用原始来源链接（空 = 用 `url`）。`.torrent` 任务的 `url` 是本地哨兵。
    #[serde(default)]
    pub origin_url: String,
    /// `ProxyMode::Auto` 的任务级最终链路（空 = 非 Auto 模式）。wire 标签：
    /// `direct` / `direct:sampled` / `direct:pinned` / `direct:failover` /
    /// `proxy:cached` / `proxy:sampled` / `proxy:failover`（代理类标签带
    /// 候选来源后缀 `:system`/`:manual`）。
    #[serde(default)]
    pub auto_route: String,
}

/// `ProxyMode::Auto` 任务的链路决策落定/变更（Rust → Dart）。启动基线与
/// 运行中热切换都会发送，Dart 侧按 task_id 原位更新详情面板「链路」行。
#[derive(Serialize, RustSignal)]
pub struct TaskRouteChanged {
    pub task_id: String,
    /// wire 标签同 `TaskInfo.auto_route`；恒非空。
    pub route: String,
}

/// 文件跟踪：一批已完成任务的「文件已丢失」标志变化（Rust → Dart）。
/// 只携带发生变化的任务，Dart 侧按 task_id 定向更新，避免整表重建导致活跃
/// 下载 UI 闪烁。
#[derive(Serialize, RustSignal)]
pub struct FileMissingChanged {
    pub updates: Vec<FileMissingUpdate>,
}

/// 文件跟踪：单个任务的「文件已丢失」标志更新（true=丢失，false=恢复存在）。
#[derive(Serialize, Deserialize, SignalPiece)]
pub struct FileMissingUpdate {
    pub task_id: String,
    pub missing: bool,
}

/// 文件跟踪：请求引擎重扫所有已完成任务的文件是否仍在（Dart → Rust）。
/// 桌面窗口聚焦 / 移动端回前台时发送，触发一次即时扫描。
#[derive(Deserialize, DartSignal)]
pub struct RescanFiles {}

/// Notification that a dynamic segment split occurred (IDM-style coordinator).
/// Sent in real-time so the Dart UI can animate the split transition.
#[derive(Serialize, RustSignal)]
pub struct SegmentSplitEvent {
    pub task_id: String,
    /// Index of the parent segment that was shrunk.
    pub parent_index: i32,
    /// New end_byte of the parent after the split.
    pub parent_new_end: i64,
    /// Index of the newly created child segment.
    pub child_index: i32,
    /// Start byte of the new child segment (= split point).
    pub child_start: i64,
    /// End byte of the new child segment (= parent's old end).
    pub child_end: i64,
    /// Whether this was a proactive split (true) or reactive/on-demand (false).
    pub is_proactive: bool,
    /// Current total number of segments after the split.
    pub total_segments: i32,
}

/// 多 CDN 并发下载的节点级活动事件（Rust → Dart，详情面板「日志」Tab）。
/// 语义与字段约定见 `fluxdown_engine::events::EngineEvent::TaskCdnEvent`。
#[derive(Serialize, RustSignal)]
pub struct TaskCdnEvent {
    pub task_id: String,
    /// "pool" | "kick" | "breaker" | "fallback" | "leases" | "summary"
    pub kind: String,
    /// 钉定目标 host。
    pub host: String,
    /// pool/leases/summary 的节点清单；其余事件为空。
    pub nodes: Vec<CdnNodeDetail>,
    /// kick：被踢节点 IP；其余为空串。
    pub ip: String,
    /// kick："validator"|"fail"|"build"；fallback："few"|"error"。
    pub reason: String,
    /// pool/fallback：去重候选 IP 总数；kick(fail)：连续失败次数。
    pub candidates: i32,
    /// pool/fallback：connect 预筛存活数。
    pub alive: i32,
    /// pool：本次生效的钉定节点数上限。
    pub cap: i32,
    /// pool：上限是否为自动档推导。
    pub auto_cap: bool,
}

/// 多 CDN 单节点描述（`TaskCdnEvent` 载荷）。
#[derive(Serialize, Deserialize, SignalPiece)]
pub struct CdnNodeDetail {
    /// 节点 IP；SYS 兜底节点为 "SYS"。
    pub ip: String,
    /// 候选来源："sys" / "doh:<端点IP>" / "ecs:<端点IP>"；SYS 为空串。
    pub origin: String,
    /// 本任务经该节点下载的字节数（summary 有效，其余 0）。
    pub bytes: i64,
    /// EWMA 吞吐（B/s）：pool = 健康度先验（0 = 无先验）；summary = 实测。
    pub ewma_bps: i64,
    /// 当前未归还的段租约数（leases 快照有效，其余 0）。
    pub active: i32,
}

// ========== Auto-update signals ==========

/// Check for application updates (Dart → Rust)
#[derive(Deserialize, DartSignal)]
pub struct CheckForUpdate {
    pub current_version: String,
    /// Update channel: "stable" (default) or "frontier" (includes prereleases).
    pub channel: String,
}

/// Update check result (Rust → Dart)
#[derive(Serialize, RustSignal)]
pub struct UpdateCheckResult {
    pub has_update: bool,
    pub latest_version: String,
    pub current_version: String,
    pub download_url: String,
    pub file_size: i64,
    pub published_at: String,
    pub error_message: String,
}

/// Start downloading an update (Dart → Rust)
#[derive(Deserialize, DartSignal)]
pub struct DownloadUpdate {
    pub url: String,
    pub version: String,
    /// Known file size from the check phase (bytes). Avoids relying on HEAD
    /// probes that may fail through API proxies / CDN redirects.
    pub file_size: i64,
}

/// Update download progress (Rust → Dart)
#[derive(Serialize, RustSignal)]
pub struct UpdateDownloadProgress {
    pub version: String,
    pub downloaded_bytes: i64,
    pub total_bytes: i64,
    pub speed: i64,
    /// 0=downloading, 1=completed, 2=error
    pub status: i32,
    pub installer_path: String,
    pub error_message: String,
    /// Number of concurrent download segments (0 = single-threaded fallback).
    pub segments: i32,
    /// Number of segments currently actively downloading.
    pub active_segments: i32,
}

// ========== HLS quality selection signals ==========

/// HLS master playlist parsed — send available quality options to Dart (Rust → Dart).
/// Dart should display a selection dialog and respond with [SelectHlsQuality].
#[derive(Serialize, RustSignal)]
pub struct HlsQualityOptions {
    pub task_id: String,
    pub options: Vec<HlsQualityOption>,
}

#[derive(Serialize, Deserialize, SignalPiece)]
pub struct HlsQualityOption {
    pub index: i32,
    pub bandwidth: i64,
    pub width: i64,
    pub height: i64,
}

/// User selected an HLS quality variant (Dart → Rust).
#[derive(Deserialize, DartSignal)]
pub struct SelectHlsQuality {
    pub task_id: String,
    /// Index of the selected variant (from [HlsQualityOption.index]).
    pub selected_index: i32,
}

// ========== Plugin resolve variant selection signals ==========

/// 插件 resolve 返回多个可选变体（画质/格式）— 发送给 Dart 供用户选择
/// (Rust → Dart)。Dart 应展示一个选择对话框并回复 [SelectResolveVariant]。
#[derive(Serialize, RustSignal)]
pub struct ResolveVariantSelectionRequest {
    pub task_id: String,
    /// 插件按自身偏好排序的默认变体索引（用户未选择/超时时回退）。
    pub default_index: i32,
    pub options: Vec<ResolveVariantOption>,
}

/// 插件 resolve 返回的单个可选变体（画质/格式）。
#[derive(Serialize, Deserialize, SignalPiece)]
pub struct ResolveVariantOption {
    pub index: i32,
    pub label: String,
    pub container: String,
    pub bandwidth: i64,
    pub width: i64,
    pub height: i64,
    pub total_bytes: i64,
}

/// User selected a plugin resolve variant (Dart → Rust).
#[derive(Deserialize, DartSignal)]
pub struct SelectResolveVariant {
    pub task_id: String,
    /// Index of the selected variant (from [ResolveVariantOption.index]).
    pub selected_index: i32,
}

// ========== Queue / meta-probe signals ==========

/// 队列任务探测到元数据 (Rust → Dart)
#[derive(Serialize, RustSignal)]
pub struct TaskMetaProbed {
    pub task_id: String,
    pub file_name: String, // 空 = 无法探测
    pub total_bytes: i64,  // 0 = 未知
}

/// 队列位置批量更新 (Rust → Dart) — 每次队列变化时广播
#[derive(Serialize, RustSignal)]
pub struct QueuePositionsUpdate {
    pub positions: Vec<QueuePosition>,
}

/// 单个任务的队列位置
#[derive(Serialize, Deserialize, SignalPiece)]
pub struct QueuePosition {
    pub task_id: String,
    pub position: i32, // 1-based，0 = 不在队列
}

// ========== Named queue management signals ==========

/// Move a task to a different queue (Dart → Rust)
#[derive(Deserialize, DartSignal)]
pub struct MoveTaskToQueue {
    pub task_id: String,
    /// Target queue ID. Empty string = move to the builtin main queue.
    pub queue_id: String,
}

/// Request all named queues (Dart → Rust, sent on app startup)
#[derive(Deserialize, DartSignal)]
pub struct RequestAllQueues {}

/// All named queues — sent on startup and after any queue change (Rust → Dart)
#[derive(Serialize, RustSignal)]
pub struct AllQueues {
    pub queues: Vec<QueueInfo>,
}

/// Single task moved to another queue (Rust → Dart)
#[derive(Serialize, RustSignal)]
pub struct TaskQueueChanged {
    pub task_id: String,
    pub queue_id: String,
}

// ========== Priority (Boost) download signals ==========

/// Set the priority download task — the selected task gets exclusive bandwidth
/// while all other active downloads are auto-paused (Dart → Rust).
/// Send `task_id = ""` to cancel boost mode.
#[derive(Deserialize, DartSignal)]
pub struct SetPriorityTask {
    /// ID of the task to boost. Empty string = cancel boost mode.
    pub task_id: String,
}

/// Notifies Dart that the boost-mode priority task has changed (Rust → Dart).
#[derive(Serialize, RustSignal)]
pub struct PriorityTaskChanged {
    /// ID of the current priority task. Empty string = boost mode inactive.
    pub priority_task_id: String,
    /// Number of tasks that were automatically paused to free bandwidth.
    pub auto_paused_count: i32,
}

/// Named queue metadata
#[derive(Serialize, Deserialize, SignalPiece)]
pub struct QueueInfo {
    pub queue_id: String,
    pub name: String,
    /// Speed limit in KB/s. 0 = no limit.
    pub speed_limit_kbps: i64,
    /// Upload speed limit in KB/s. 0 = no limit.
    #[serde(default)]
    pub upload_limit_kbps: i64,
    /// Max simultaneous tasks in this queue. 0 = use global setting.
    pub max_concurrent: i32,
    /// Default save directory. Empty = use global default.
    pub default_save_dir: String,
    /// Display order (lower = higher up).
    pub position: i32,
    /// Default segment count for new tasks. 0 = auto (global segment advisor).
    pub default_segments: i32,
    /// Default user-agent for tasks in this queue. Empty = inherit global UA.
    #[serde(default)]
    pub default_user_agent: String,
    /// 队列运行状态：停止的队列不自动启动其中任务。
    /// （QueueInfo 仅 Rust→Dart 序列化，字段恒由引擎填充。）
    #[serde(default)]
    pub is_running: bool,
    /// 定时计划是否启用。
    #[serde(default)]
    pub schedule_enabled: bool,
    /// 每日定时启动时间 "HH:MM"（空 = 不定时启动）。
    #[serde(default)]
    pub schedule_start: String,
    /// 每日定时停止时间 "HH:MM"（空 = 不定时停止）。
    #[serde(default)]
    pub schedule_stop: String,
    /// 定时生效星期位掩码：bit0=周一 … bit6=周日。
    #[serde(default)]
    pub schedule_days: i32,
}

// ========== BT file selection signals ==========

/// BT torrent metadata resolved — send file list to Dart for user selection (Rust → Dart).
/// Dart should display a file selection dialog and respond with [SelectBtFiles].
#[derive(Serialize, RustSignal)]
pub struct BtFilesInfo {
    pub task_id: String,
    /// Total size of all files in the torrent (bytes).
    pub total_bytes: i64,
    /// List of files in the torrent.
    pub files: Vec<BtFileEntry>,
}

/// A single file entry in a BT torrent.
#[derive(Serialize, Deserialize, SignalPiece)]
pub struct BtFileEntry {
    /// Zero-based file index within the torrent.
    pub index: i32,
    /// Relative path of the file inside the torrent (e.g. "folder/sub/file.mp4").
    pub path: String,
    /// File size in bytes.
    pub size: i64,
}

/// User selected which BT files to download (Dart → Rust).
#[derive(Deserialize, DartSignal)]
pub struct SelectBtFiles {
    pub task_id: String,
    /// Indices of files the user wants to download (from [BtFileEntry.index]).
    /// Empty = download all files (should not happen in practice).
    pub selected_indices: Vec<i32>,
}
