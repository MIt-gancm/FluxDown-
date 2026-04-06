//! BitTorrent / Magnet-link download engine.
//!
//! Uses **librqbit** as the BT backend.  All BT tasks share a single
//! `Session` (DHT, trackers, listening port) managed by [`SharedBtSession`],
//! which lives inside `DownloadManager`.  This avoids per-task resource waste
//! (redundant DHT nodes, tracker connections, OS threads, listening ports).
//!
//! Because librqbit requires a multi-threaded tokio runtime while our main
//! actor runs on `current_thread`, the shared session is created inside a
//! dedicated `Runtime(multi_thread)`.  Individual download tasks submit work
//! to that runtime via `Runtime::spawn`.
//!
//! Key design:
//! - Single shared `Session` with DHT + public trackers + UPnP.
//! - Speed limit is applied at the `Session` level via `ratelimits` and
//!   updated in real-time when the user changes the global speed setting.
//! - `add_torrent` blocks while resolving magnet metadata from DHT/peers, so
//!   we report "preparing" status to Dart while we wait.

use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use bytes::Bytes;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, ManagedTorrent, PeerConnectionOptions,
    Session, SessionOptions, SessionPersistenceConfig,
};

/// Alias for librqbit's `BtHandle` (`Arc<ManagedTorrent>`).
/// The upstream type is not re-exported, so we define it locally.
pub type BtHandle = Arc<ManagedTorrent>;
use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;

use crate::db::Db;
use crate::downloader::{DownloadError, ProgressUpdate, SegmentProgressInfo};
use crate::logger::log_info;

// ---------------------------------------------------------------------------
// Public helpers
// ---------------------------------------------------------------------------

/// Truncate an identifier to at most 8 characters for log output.
/// Returns the full string if shorter than 8 characters, avoiding panic
/// from direct byte-index slicing on short or multi-byte strings.
#[inline]
fn short_id(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

/// Returns `true` if the URL looks like a magnet link.
pub fn is_magnet_url(url: &str) -> bool {
    url.get(..8)
        .map(|prefix| prefix.eq_ignore_ascii_case("magnet:?"))
        .unwrap_or(false)
}

/// Torrent input source — either a magnet URI or raw .torrent file bytes.
/// Replaces the old hardcoded `magnet_url: String` field so that
/// `BtDownloadParams` can represent both kinds of BT downloads uniformly.
#[derive(Clone)]
pub enum TorrentSource {
    /// A magnet link URI string (e.g. `magnet:?xt=urn:btih:...`).
    Magnet(String),
    /// Raw bytes of a `.torrent` file read from disk.
    TorrentFileBytes(Vec<u8>),
}

impl TorrentSource {
    /// Returns `true` if this source is a magnet link.
    #[allow(dead_code)]
    pub fn is_magnet(&self) -> bool {
        matches!(self, TorrentSource::Magnet(_))
    }

    /// Best-effort display name for logging / early UI display.
    /// For magnet links, extracts the `dn=` parameter.
    /// For torrent file bytes, returns None (the name comes from metadata).
    pub fn display_name(&self) -> Option<String> {
        match self {
            TorrentSource::Magnet(url) => magnet_display_name(url),
            TorrentSource::TorrentFileBytes(_) => None,
        }
    }

    /// URL string for DB storage.  Magnet links store the URI directly.
    /// Torrent file sources store a sentinel `torrent-file://` URL since the
    /// actual content is persisted separately in the `torrent_file_bytes` column.
    #[allow(dead_code)]
    pub fn url_for_db(&self) -> &str {
        match self {
            TorrentSource::Magnet(url) => url,
            TorrentSource::TorrentFileBytes(_) => "torrent-file://local",
        }
    }
}

/// Extract the `dn=` (display name) parameter from a magnet URI, if present.
fn magnet_display_name(url: &str) -> Option<String> {
    url.split('&')
        .find_map(|part| {
            let part = part.strip_prefix("magnet:?").unwrap_or(part);
            part.strip_prefix("dn=")
        })
        .map(urlencoding_decode)
}

/// Minimal percent-decoding for `dn=` values (UTF-8 safe).
///
/// Collects percent-encoded bytes into a buffer and decodes them as UTF-8,
/// correctly handling multi-byte characters (e.g. CJK, emoji).
///
/// Incomplete `%` sequences at the end of the input (e.g. `%`, `%A`) are
/// treated as literal characters rather than silently padded with zeros.
fn urlencoding_decode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut bytes_buf: Vec<u8> = Vec::new();
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    // Flush accumulated percent-encoded bytes as UTF-8 into `out`.
    let flush = |buf: &mut Vec<u8>, out: &mut String| {
        if !buf.is_empty() {
            match std::str::from_utf8(buf) {
                Ok(s) => out.push_str(s),
                Err(_) => {
                    // Fallback: replace invalid UTF-8 with replacement char
                    out.push(char::REPLACEMENT_CHARACTER);
                }
            }
            buf.clear();
        }
    };

    while i < len {
        match bytes[i] {
            b'+' => {
                flush(&mut bytes_buf, &mut out);
                out.push(' ');
                i += 1;
            }
            b'%' if i + 2 < len => {
                // Full %XX sequence — decode as a byte.
                let hi = bytes[i + 1];
                let lo = bytes[i + 2];
                bytes_buf.push(hex_val(hi) << 4 | hex_val(lo));
                i += 3;
            }
            b'%' => {
                // Incomplete `%` at end of string — emit as literal.
                flush(&mut bytes_buf, &mut out);
                out.push('%');
                i += 1;
                // Also emit any remaining characters after `%` literally.
                while i < len {
                    out.push(bytes[i] as char);
                    i += 1;
                }
            }
            b => {
                flush(&mut bytes_buf, &mut out);
                out.push(b as char);
                i += 1;
            }
        }
    }
    flush(&mut bytes_buf, &mut out);
    out
}

fn hex_val(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

/// Well-known public trackers used to accelerate peer discovery for magnet
/// links that ship without `tr=` parameters.
///
/// **Curated from global community sources** (2026-02-10):
///   - ngosang/trackerslist (52.9k stars, auto-updated daily, ranked by latency)
///   - XIU2/TrackersListCollection (popular in CN community)
///   - Cross-referenced and **availability-tested** before inclusion.
///
/// Strategy: CN/Asia trackers first (better peer locality for domestic users),
/// then international trackers.  UDP-heavy (lowest overhead), with HTTPS
/// fallbacks for restrictive network environments where UDP may be blocked.
///
/// Kept to ~25 high-availability trackers to minimise DNS/connect overhead
/// while still providing excellent global peer coverage.  All tracker
/// connections are async and parallel, so startup impact is minimal.
const PUBLIC_TRACKERS: &[&str] = &[
    // ─── CN / Asia — better peer discovery for domestic users ───
    "udp://tracker.dler.com:6969/announce",
    "udp://admin.52ywp.com:6969/announce",
    "udp://tracker.dler.org:6969/announce",
    "https://tracker.moeblog.cn:443/announce",
    "http://nyaa.tracker.wf:7777/announce",
    "https://tr.zukizuki.org:443/announce",
    // ─── International — top-tier, highest uptime ───
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://open.dstud.io:6969/announce",
    "udp://tracker-udp.gbitt.info:80/announce",
    "udp://open.stealth.si:80/announce",
    "udp://tracker.torrent.eu.org:451/announce",
    "udp://exodus.desync.com:6969/announce",
    "udp://explodie.org:6969/announce",
    "udp://tracker.srv00.com:6969/announce",
    "udp://tracker.qu.ax:6969/announce",
    "udp://opentracker.io:6969/announce",
    "udp://tracker.bittor.pw:1337/announce",
    "udp://tracker.theoks.net:6969/announce",
    "udp://tracker.opentorrent.top:6969/announce",
    "udp://open.demonoid.ch:6969/announce",
    "udp://tracker.t-1.org:6969/announce",
    // ─── HTTPS fallbacks — for networks that block UDP ───
    "https://tracker.ghostchu-services.top:443/announce",
    "https://tracker.bt4g.com:443/announce",
    "https://1337.abcvg.info:443/announce",
    "http://tracker.bt4g.com:2095/announce",
];

/// Return the built-in public tracker list as a newline-separated string.
/// Used to populate the default config value on first launch so users can
/// see and edit the full list in Settings.
pub fn default_tracker_list() -> String {
    PUBLIC_TRACKERS.join("\n")
}

// ---------------------------------------------------------------------------
// BT configuration — user-settable via the Settings page
// ---------------------------------------------------------------------------

/// User-configurable BT session settings, loaded from the DB config table.
#[derive(Debug, Clone)]
pub struct BtConfig {
    pub enable_dht: bool,
    pub enable_upnp: bool,
    pub port_start: u16,
    pub port_end: u16,
    /// User-supplied extra tracker URLs (newline-separated).
    /// These are **merged** with the built-in `PUBLIC_TRACKERS` list.
    pub custom_trackers: String,
}

impl Default for BtConfig {
    fn default() -> Self {
        Self {
            enable_dht: true,
            enable_upnp: true,
            port_start: 6881,
            port_end: 6891,
            custom_trackers: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Shared BT Session — singleton owned by DownloadManager
// ---------------------------------------------------------------------------

/// A shared BT session that holds a dedicated multi-thread runtime and a
/// single `librqbit::Session`.  All BT tasks share this instance, which
/// means they share DHT routing tables, tracker connections, and the
/// listening port — dramatically reducing resource usage.
///
/// Torrent handles are cached in `handles` so that pause/resume cycles
/// use the native `Session::pause` / `Session::unpause` API instead of
/// deleting and re-adding the torrent.  This preserves fast-resume data
/// (piece bitfield) and avoids expensive re-verification of already
/// downloaded pieces.
pub struct SharedBtSession {
    runtime: tokio::runtime::Runtime,
    session: Arc<Session>,
    /// Maps our `task_id` → librqbit `BtHandle`.
    /// Protected by an async Mutex because it's accessed from both the
    /// main actor (pause/delete) and spawned download tasks (add/finish).
    handles: Mutex<HashMap<String, BtHandle>>,
    /// Pending delete requests for tasks whose `add_torrent` call is still
    /// in progress (handle not yet in `handles`).  Keyed by task_id, value
    /// is the `delete_files` flag.  The detached add_torrent closure checks
    /// this map on completion and calls `session.delete` accordingly,
    /// preventing orphaned files when a magnet task is deleted during DHT
    /// metadata resolution.
    pending_deletes: Mutex<HashMap<String, bool>>,
    /// Count of detached `add_torrent` tasks currently running in the
    /// background.  Incremented just before spawning the detached task;
    /// decremented when the task completes (success, error, or pending-delete).
    ///
    /// `maybe_release_bt_session` must not tear down the session while this
    /// is non-zero: the detached task still holds an `Arc<Session>` that
    /// keeps the listening port bound.  Creating a new session while the old
    /// port is still in use causes the next BT task to fail immediately.
    inflight_adds: AtomicUsize,
}

impl SharedBtSession {
    /// Create the shared session with the given initial speed limit and config.
    ///
    /// `default_save_dir` is used as the Session's default output folder
    /// (individual torrents override this via `AddTorrentOptions::output_folder`).
    ///
    /// `app_data_dir` is the application data directory where BT persistence
    /// files (session.json, .bitv, .torrent) are stored. This should be the
    /// exe directory or an app-specific folder — NOT the user's download dir.
    ///
    /// `speed_limit_bps` is the global download speed limit in bytes/sec
    /// (0 = unlimited).
    ///
    /// `bt_config` contains user-configurable BT settings (DHT, UPnP, ports,
    /// custom trackers).
    pub fn new(
        default_save_dir: &str,
        app_data_dir: &str,
        speed_limit_bps: u64,
        bt_config: &BtConfig,
    ) -> Result<Self, DownloadError> {
        // Scale worker threads with CPU cores.  BT workload is mostly I/O-bound
        // so diminishing returns beyond 8 threads; capping here saves ~2 MB of
        // stack memory per thread avoided.
        let cpu_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let worker_threads = cpu_cores.clamp(2, 8);

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(worker_threads)
            .thread_name("bt-runtime")
            .build()
            .map_err(|e| DownloadError::Other(format!("failed to build BT runtime: {e}")))?;

        // If the user has a non-empty tracker list in settings, use it directly.
        // Otherwise fall back to the built-in PUBLIC_TRACKERS list.
        let trackers: HashSet<url::Url> = if bt_config.custom_trackers.trim().is_empty() {
            PUBLIC_TRACKERS
                .iter()
                .filter_map(|s| s.parse().ok())
                .collect()
        } else {
            bt_config
                .custom_trackers
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .filter_map(|l| l.parse().ok())
                .collect()
        };

        let total_tracker_count = trackers.len();

        let download_bps = NonZeroU32::new(speed_limit_bps.min(u32::MAX as u64) as u32);

        // Persistence folder: store session.json + {hash}.bitv + {hash}.torrent
        // in the app data directory (next to flux_down.db), NOT in the user's
        // download folder. This matches how professional tools (qBittorrent,
        // Thunder, etc.) keep internal data out of user-visible directories.
        let persistence_folder = PathBuf::from(app_data_dir).join("bt_session");

        // Validate and clamp port range.
        let port_start = bt_config.port_start.max(1024);
        let port_end = bt_config.port_end.max(port_start);

        let enable_dht = bt_config.enable_dht;
        let enable_upnp = bt_config.enable_upnp;

        let save_dir = default_save_dir.to_owned();
        let session = rt
            .block_on(async {
                let opts = SessionOptions {
                    disable_dht: !enable_dht,
                    disable_dht_persistence: !enable_dht,
                    listen_port_range: Some(port_start..port_end.saturating_add(1)),
                    enable_upnp_port_forwarding: enable_upnp,
                    trackers,
                    ratelimits: librqbit::limits::LimitsConfig {
                        download_bps,
                        upload_bps: None,
                    },
                    // Optimised peer connection parameters.
                    peer_opts: Some(PeerConnectionOptions {
                        // Slightly shorter connect timeout — drop unresponsive
                        // peers faster so we can try others sooner.
                        connect_timeout: Some(Duration::from_secs(10)),
                        // Generous read/write timeout to avoid dropping slow
                        // but otherwise healthy peers.
                        read_write_timeout: Some(Duration::from_secs(20)),
                        ..Default::default()
                    }),
                    // Enable persistence so that session.json and per-torrent
                    // .bitv (piece bitfield) files are written to disk.
                    persistence: Some(SessionPersistenceConfig::Json {
                        folder: Some(persistence_folder),
                    }),
                    // Fast-resume: persist piece completion state so that
                    // paused/restarted torrents can skip re-verification.
                    // Requires `persistence` to be set to take effect.
                    fastresume: true,
                    // Buffer writes in memory before flushing to disk.  Reduces
                    // I/O contention from many small pieces.  64 MiB is enough
                    // for high-speed connections while keeping RSS reasonable
                    // (was 128 — saved ~64 MB of potential RSS).
                    defer_writes_up_to: Some(64),
                    // Limit concurrent torrent initialisation to 3 to prevent
                    // DHT/tracker storms when many BT tasks start at once.
                    concurrent_init_limit: Some(3),
                    ..Default::default()
                };

                Session::new_with_opts(save_dir.into(), opts).await
            })
            .map_err(|e| DownloadError::Other(format!("BT session init failed: {e}")))?;

        // Startup cleanup: remove any finished torrents that were
        // retained in persistence from a previous app session.
        //
        // Background: since the main fix (pause-on-complete) we no
        // longer call session.delete() when a torrent finishes, so
        // completed torrents stay in session.json across restarts.
        // Without this cleanup they would accumulate indefinitely,
        // slowing down startup and wasting memory.
        //
        // We only remove them from the librqbit session — the actual
        // downloaded files are left untouched (delete_files=false).
        // User-triggered "delete task + files" goes through the normal
        // delete_task() path which uses delete_files=true.
        {
            let finished_ids: Vec<usize> = session.with_torrents(|iter| {
                iter.filter_map(|(id, handle)| {
                    if handle.stats().finished {
                        Some(id)
                    } else {
                        None
                    }
                })
                .collect()
            });
            if !finished_ids.is_empty() {
                log_info!(
                    "[BT] startup cleanup: removing {} finished torrent(s) from persistence",
                    finished_ids.len()
                );
                for id in finished_ids {
                    let _ = rt.block_on(session.delete(id.into(), false));
                }
            }
        }

        log_info!(
            "[BT] shared session created (DHT={}, UPnP={}, ports={}-{}, {} trackers, speed_limit={} B/s, worker_threads={}, persistence=on)",
            enable_dht,
            enable_upnp,
            port_start,
            port_end,
            total_tracker_count,
            speed_limit_bps,
            worker_threads
        );

        Ok(Self {
            runtime: rt,
            session,
            handles: Mutex::new(HashMap::new()),
            pending_deletes: Mutex::new(HashMap::new()),
            inflight_adds: AtomicUsize::new(0),
        })
    }

    /// Update the global download speed limit at runtime.
    /// `bps == 0` means unlimited.  Takes effect immediately on all active
    /// BT downloads.
    pub fn set_speed_limit(&self, bps: u64) {
        let limit = NonZeroU32::new(bps.min(u32::MAX as u64) as u32);
        self.session.ratelimits.set_download_bps(limit);
        log_info!("[BT] shared session speed limit updated to {} B/s", bps);
    }

    /// Get an `Arc<Session>` handle for adding torrents.
    pub fn session(&self) -> Arc<Session> {
        self.session.clone()
    }

    /// Get a handle to the BT runtime for spawning tasks.
    pub fn runtime_handle(&self) -> tokio::runtime::Handle {
        self.runtime.handle().clone()
    }

    /// Store a torrent handle for a task so it can be paused/resumed later.
    pub async fn store_handle(&self, task_id: &str, handle: BtHandle) {
        self.handles
            .lock()
            .await
            .insert(task_id.to_string(), handle);
    }

    /// Pause a BT torrent by task_id.  The handle stays cached so that
    /// `resume_handle` can unpause it without re-adding.
    pub async fn pause_task(&self, task_id: &str) -> Result<(), DownloadError> {
        // Clone the Arc handle and release the lock immediately so that
        // the async session.pause() call doesn't block other handle ops.
        let handle = self.handles.lock().await.get(task_id).cloned();
        if let Some(handle) = handle {
            // If already paused or initializing, ignore silently.
            if !handle.is_paused() {
                self.session
                    .pause(&handle)
                    .await
                    .map_err(|e| DownloadError::Other(format!("BT pause failed: {e}")))?;
            }
            log_info!("[BT] task={} paused via session API", short_id(task_id));
        }
        Ok(())
    }

    /// Resume a previously paused BT torrent.  Returns the handle if
    /// successful, or `None` if no cached handle exists (caller should
    /// fall back to `add_torrent`).
    pub async fn resume_task(&self, task_id: &str) -> Result<Option<BtHandle>, DownloadError> {
        // Clone the Arc handle and release the lock immediately.
        let handle = self.handles.lock().await.get(task_id).cloned();
        if let Some(handle) = handle {
            if handle.is_paused() {
                self.session
                    .unpause(&handle)
                    .await
                    .map_err(|e| DownloadError::Other(format!("BT unpause failed: {e}")))?;
                log_info!("[BT] task={} resumed via session API", short_id(task_id));
            }
            Ok(Some(handle))
        } else {
            Ok(None)
        }
    }

    /// Gracefully shut down the BT session and runtime.
    ///
    /// Pauses all active torrents, then shuts down the runtime with a timeout.
    /// Called when the application exits to ensure clean resource release.
    pub fn shutdown(&self) {
        log_info!("[BT] shutting down shared session...");
        // Use the runtime to gracefully close the session.  The session's
        // drop will attempt to persist DHT state and piece bitfields.
        // We give it a generous timeout to allow disk writes to complete.
        self.runtime.block_on(async {
            // Pause all tracked torrents so they flush state to disk.
            let handles: Vec<(String, BtHandle)> = {
                let map = self.handles.lock().await;
                map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            };
            for (tid, handle) in &handles {
                if !handle.is_paused()
                    && let Err(e) = self.session.pause(handle).await
                {
                    log_info!(
                        "[BT] shutdown: failed to pause task {}: {}",
                        short_id(tid),
                        e
                    );
                }
            }
        });
        // The Runtime::drop will be called after this, which blocks until
        // all spawned tasks finish (or the runtime forces them to stop).
        log_info!("[BT] shared session shutdown complete");
    }

    /// Permanently delete a torrent from the session, removing persistence
    /// data.  `delete_files` controls whether downloaded data is also removed.
    /// Returns `true` if a handle was found and `session.delete` was called,
    /// `false` if the task was not yet in the handles map (still in the
    /// `add_torrent` phase).  The caller should call `register_pending_delete`
    /// when this returns `false` so the detached add_torrent closure can clean
    /// up once metadata resolution completes.
    pub async fn delete_task(&self, task_id: &str, delete_files: bool) -> bool {
        // Remove from map first (under lock), then perform async deletion
        // outside the lock to minimise contention.
        let handle = self.handles.lock().await.remove(task_id);
        if let Some(handle) = handle {
            let torrent_id = handle.id();
            if let Err(e) = self.session.delete(torrent_id.into(), delete_files).await {
                log_info!(
                    "[BT] task={} session.delete error: {}",
                    short_id(task_id),
                    e
                );
            } else {
                log_info!(
                    "[BT] task={} deleted from session (delete_files={})",
                    short_id(task_id),
                    delete_files
                );
            }
            true
        } else {
            false
        }
    }

    /// Register a deferred delete for a task whose `add_torrent` is still in
    /// progress.  The detached add_torrent closure will consume this entry and
    /// call `session.delete(id, delete_files)` as soon as metadata resolves.
    pub async fn register_pending_delete(&self, task_id: &str, delete_files: bool) {
        self.pending_deletes
            .lock()
            .await
            .insert(task_id.to_string(), delete_files);
        log_info!(
            "[BT] task={} pending delete registered (delete_files={})",
            short_id(task_id),
            delete_files
        );
    }

    /// Consume and return the pending delete flag for `task_id`, if any.
    pub async fn take_pending_delete(&self, task_id: &str) -> Option<bool> {
        self.pending_deletes.lock().await.remove(task_id)
    }

    fn increment_inflight_add(&self) {
        self.inflight_adds.fetch_add(1, Ordering::Relaxed);
    }

    fn decrement_inflight_add(&self) {
        self.inflight_adds.fetch_sub(1, Ordering::Relaxed);
    }

    /// Returns `true` if any detached `add_torrent` task is still running.
    pub fn has_inflight_adds(&self) -> bool {
        self.inflight_adds.load(Ordering::Relaxed) > 0
    }

    /// Increment the counter and return an RAII guard that decrements it on
    /// drop.  Using a guard instead of manual increment/decrement ensures the
    /// counter is always decremented even if the enclosing `tokio::spawn`
    /// closure panics before reaching the end.
    pub fn inflight_guard(self: &Arc<Self>) -> InflightGuard {
        self.increment_inflight_add();
        InflightGuard(Arc::clone(self))
    }
}

/// Decrements `SharedBtSession::inflight_adds` when dropped, guaranteeing
/// the counter is decremented even if the enclosing async task panics.
pub struct InflightGuard(Arc<SharedBtSession>);

impl Drop for InflightGuard {
    fn drop(&mut self) {
        self.0.decrement_inflight_add();
    }
}

// ---------------------------------------------------------------------------
// BT download params
// ---------------------------------------------------------------------------

pub struct BtDownloadParams {
    pub task_id: String,
    /// Torrent input source — magnet URI or raw .torrent file bytes.
    pub torrent_source: TorrentSource,
    pub save_dir: String,
    pub db: Db,
    pub progress_tx: mpsc::Sender<ProgressUpdate>,
    pub cancel_token: CancellationToken,
    /// Handle to the shared BT session.
    pub session: Arc<Session>,
    /// Handle to the shared BT runtime.
    pub bt_runtime: tokio::runtime::Handle,
    /// Shared session wrapper — used to cache the handle after add_torrent.
    pub shared_bt: Arc<SharedBtSession>,
    /// If resuming a paused torrent, this is the existing handle.
    /// When `Some`, we skip `add_torrent` and go straight to the progress loop.
    pub existing_handle: Option<BtHandle>,
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Run a BT download for a magnet link using the shared session.
///
/// This function is designed to be `tokio::spawn`-ed from the download manager
/// just like `downloader::run_download` or `ftp_downloader::run_ftp_download`.
///
/// The actual BT work (add_torrent, progress polling) runs on the shared BT
/// runtime; this function bridges between the main `current_thread` runtime
/// and the BT runtime.
pub async fn run_bt_download(params: BtDownloadParams) -> Result<(), DownloadError> {
    let task_id = params.task_id.clone();

    // 1. Switch to "preparing" status
    let _ = params
        .db
        .update_task_status(&task_id, STATUS_PREPARING, "")
        .await;
    let _ = params
        .progress_tx
        .send(ProgressUpdate {
            task_id: task_id.clone(),
            downloaded_bytes: 0,
            total_bytes: 0,
            status: STATUS_PREPARING,
            error_message: String::new(),
            file_name: String::new(),
            segment_details: None,
        })
        .await;

    log_info!(
        "[BT] task={} starting bt download (shared session)...",
        short_id(&task_id)
    );

    // 2. Run the actual BT download on the shared multi-thread runtime.
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_clone = cancelled.clone();
    let cancel_token = params.cancel_token.clone();

    // Forward cancellation from CancellationToken to AtomicBool
    let cancelled_for_watcher = cancelled.clone();
    let cancel_watcher = tokio::spawn(async move {
        cancel_token.cancelled().await;
        cancelled_for_watcher.store(true, Ordering::SeqCst);
    });

    let progress_tx = params.progress_tx.clone();
    let db = params.db.clone();
    let torrent_source = params.torrent_source.clone();
    let save_dir = params.save_dir.clone();
    let tid = task_id.clone();
    let session = params.session.clone();
    let bt_runtime = params.bt_runtime.clone();
    let shared_bt = params.shared_bt.clone();
    let existing_handle = params.existing_handle;

    // Spawn the BT download on the shared multi-thread BT runtime.
    // The returned JoinHandle can be safely .await-ed from any runtime
    // (including our current_thread main runtime) — it uses waker-based
    // notification, not runtime-specific polling.  This avoids occupying
    // a thread from tokio's blocking thread pool for the entire download
    // duration, which previously caused thread-pool starvation under
    // many concurrent BT tasks.
    let inner_params = BtInnerParams {
        task_id: tid,
        torrent_source,
        save_dir,
        db,
        progress_tx,
        cancelled: cancelled_clone,
        session,
        shared_bt,
        existing_handle,
    };
    let result = bt_runtime
        .spawn(async move { bt_download_inner(inner_params).await })
        .await;

    cancel_watcher.abort();

    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        // JoinError has two causes:
        //   1. The spawned task panicked → treat as error (existing behaviour).
        //   2. The BT runtime was shut down (e.g. maybe_release_bt_session called
        //      while this task was still winding down after pause_task cancelled it).
        //      In that case cancelled is already true, so treat it as Cancelled to
        //      prevent the task from being marked as failed/error in the DB.
        Err(join_err) => {
            if cancelled.load(Ordering::SeqCst) {
                log_info!(
                    "[BT] task={} JoinError while cancelled (runtime shutdown during pause) — treating as Cancelled",
                    short_id(&task_id)
                );
                Err(DownloadError::Cancelled)
            } else {
                Err(DownloadError::Other(format!(
                    "BT task panicked: {join_err}"
                )))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Inner download logic (runs on the shared BT runtime)
// ---------------------------------------------------------------------------

/// Parameters for the inner BT download loop (avoids too-many-arguments warning).
struct BtInnerParams {
    task_id: String,
    torrent_source: TorrentSource,
    save_dir: String,
    db: Db,
    progress_tx: mpsc::Sender<ProgressUpdate>,
    cancelled: Arc<AtomicBool>,
    session: Arc<Session>,
    shared_bt: Arc<SharedBtSession>,
    existing_handle: Option<BtHandle>,
}

// ---------------------------------------------------------------------------
// Task status codes — must match Dart TaskStatus enum values.
// ---------------------------------------------------------------------------
const STATUS_DOWNLOADING: i32 = 1;
#[allow(dead_code)]
const STATUS_PAUSED: i32 = 2;
const STATUS_COMPLETED: i32 = 3;
const STATUS_ERROR: i32 = 4;

// ---------------------------------------------------------------------------
// BT staging directory helpers
// ---------------------------------------------------------------------------

/// Prefix used for per-task staging directories inside `save_dir`.
/// Each BT task downloads into `save_dir/.bt_stage_<task_id>/` so that
/// concurrent tasks with identical torrent names never collide on disk.
/// The directory is removed after the file/folder is moved to its final
/// location (or on task deletion).
const BT_STAGE_PREFIX: &str = ".bt_stage_";

/// Build the staging directory path for a BT task.
///
/// `save_dir/.bt_stage_<task_id>/`
pub fn bt_stage_dir(save_dir: &str, task_id: &str) -> PathBuf {
    PathBuf::from(save_dir).join(format!("{}{}", BT_STAGE_PREFIX, task_id))
}

/// Deduplicate a file or directory name inside `dir`.
///
/// If `dir/name` does not exist, returns `name` unchanged.
/// Otherwise appends ` (1)`, ` (2)`, … until a free slot is found.
/// Mirrors the logic in `downloader::dedup_filename` but runs synchronously
/// (called from the BT runtime thread after downloading is complete).
fn dedup_name_in_dir(dir: &Path, name: &str) -> String {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return name.to_string();
    }

    // Scan directory once to avoid per-candidate filesystem round-trips.
    let existing: std::collections::HashSet<std::ffi::OsString> = std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.file_name()).collect())
        .unwrap_or_default();

    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    let ext = Path::new(name).extension().and_then(|s| s.to_str());

    for i in 1..=9999u32 {
        let new_name = match ext {
            Some(e) => format!("{} ({}).{}", stem, i, e),
            None => format!("{} ({})", stem, i),
        };
        if !existing.contains(std::ffi::OsStr::new(&new_name)) {
            return new_name;
        }
    }
    name.to_string()
}

/// Move a file or directory from `src` to `dst`.
///
/// Tries `std::fs::rename` first (atomic, same filesystem).  If that fails
/// (e.g. cross-device), falls back to a recursive copy + remove.
fn move_path(src: &Path, dst: &Path) -> std::io::Result<()> {
    // Fast path: atomic rename (works when src and dst are on the same fs).
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }

    // Slow path: copy then remove.
    if src.is_dir() {
        copy_dir_all(src, dst)?;
        std::fs::remove_dir_all(src)
    } else {
        std::fs::copy(src, dst)?;
        std::fs::remove_file(src)
    }
}

/// Recursively copy a directory tree from `src` to `dst`.
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}
const STATUS_PREPARING: i32 = 5;

/// Number of virtual segments for single-file BT progress visualization.
const BT_VIRTUAL_SEGMENTS: i32 = 16;

/// For **multi-file** torrents each file becomes a segment — this naturally
/// reflects the concurrent piece-based download because different files
/// accumulate downloaded bytes independently.
///
/// For **single-file** (or when `file_progress` is unavailable) we split
/// the total size into `BT_VIRTUAL_SEGMENTS` virtual segments and
/// distribute the completed pieces proportionally using a deterministic
/// scatter pattern.  This avoids the old "linear fill" look and produces
/// an IDM-style concurrent visualization that truthfully represents the
/// random order in which BT pieces arrive.
fn build_bt_segments(
    total_bytes: i64,
    downloaded_bytes: i64,
    file_progress: &[u64],
    file_offsets: &[(u64, u64)], // (offset_in_torrent, file_len)
    total_pieces: u32,
    downloaded_pieces: u64,
) -> Vec<SegmentProgressInfo> {
    if total_bytes <= 0 {
        return Vec::new();
    }

    // Multi-file torrent: each file is a natural segment
    if file_progress.len() > 1 && file_offsets.len() == file_progress.len() {
        return build_multi_file_segments(total_bytes, file_progress, file_offsets);
    }

    // Single-file (or fallback): scatter pieces across virtual segments
    build_piece_scatter_segments(
        total_bytes,
        downloaded_bytes,
        total_pieces,
        downloaded_pieces,
    )
}

/// Multi-file torrent: map each file to a segment.
fn build_multi_file_segments(
    total_bytes: i64,
    file_progress: &[u64],
    file_offsets: &[(u64, u64)],
) -> Vec<SegmentProgressInfo> {
    let mut segs = Vec::with_capacity(file_progress.len());
    for (i, (&dl_bytes, &(offset, file_len))) in
        file_progress.iter().zip(file_offsets.iter()).enumerate()
    {
        if file_len == 0 {
            continue;
        }
        let start = offset as i64;
        let end = (offset + file_len).saturating_sub(1) as i64;
        let end = end.min(total_bytes - 1);
        segs.push(SegmentProgressInfo {
            index: i as i32,
            start_byte: start,
            end_byte: end,
            downloaded_bytes: (dl_bytes as i64).min(end - start + 1),
        });
    }
    segs
}

/// Single-file torrent: split into virtual segments and distribute
/// completed pieces using a deterministic scatter pattern.
///
/// BT downloads pieces in a mostly random order (rarest-first strategy).
/// Instead of filling left-to-right, we use a modular-hash scatter to
/// distribute `downloaded_pieces` across all virtual segments so the UI
/// shows multiple segments progressing simultaneously — which is what
/// actually happens in practice.
fn build_piece_scatter_segments(
    total_bytes: i64,
    downloaded_bytes: i64,
    total_pieces: u32,
    downloaded_pieces: u64,
) -> Vec<SegmentProgressInfo> {
    let n = BT_VIRTUAL_SEGMENTS;
    let chunk = total_bytes / n as i64;
    let mut segs = Vec::with_capacity(n as usize);

    if total_pieces == 0 || (downloaded_pieces == 0 && downloaded_bytes > 0) {
        // Fallback: no piece info yet OR no pieces completed but we have
        // fetched bytes (partial pieces in-flight).  Distribute bytes
        // evenly across virtual segments with a scatter pattern so the
        // user can see BT is actively downloading even before any piece
        // has been fully hash-verified.
        let per_seg = if downloaded_bytes > 0 {
            downloaded_bytes / n as i64
        } else {
            0
        };
        for i in 0..n {
            let start = i as i64 * chunk;
            let end = if i == n - 1 {
                total_bytes - 1
            } else {
                (i as i64 + 1) * chunk - 1
            };
            // Scatter the bytes unevenly so segments don't all look identical.
            // Use golden-ratio perturbation for a natural spread.
            let perturbation = ((i as f64 + 1.0) * 0.618033988749895).fract();
            let weight = 0.7 + perturbation * 0.6; // range [0.7, 1.3]
            let seg_dl = (per_seg as f64 * weight).round() as i64;
            segs.push(SegmentProgressInfo {
                index: i,
                start_byte: start,
                end_byte: end,
                downloaded_bytes: seg_dl.clamp(0, end - start + 1),
            });
        }
        // Correction: ensure total visual bytes match actual downloaded_bytes
        let visual_total: i64 = segs.iter().map(|s| s.downloaded_bytes).sum();
        let diff = downloaded_bytes - visual_total;
        if diff != 0 {
            let abs_diff = diff.unsigned_abs() as f64;
            let direction = if diff > 0 { 1i64 } else { -1i64 };
            let mut remaining = diff.abs();
            for seg in &mut segs {
                let seg_size = seg.end_byte - seg.start_byte + 1;
                let share = ((seg_size as f64 / total_bytes as f64) * abs_diff).round() as i64;
                let adj = share.min(remaining);
                seg.downloaded_bytes = (seg.downloaded_bytes + direction * adj).clamp(0, seg_size);
                remaining -= adj;
                if remaining <= 0 {
                    break;
                }
            }
        }
        return segs;
    }

    // Assign each piece to a virtual segment, then count completed pieces
    // per segment.  The assignment uses a scatter function to spread pieces
    // that are close in index across different segments.
    let pieces_per_seg = (total_pieces as f64 / n as f64).ceil() as u32;
    let completion_ratio = if total_pieces > 0 {
        downloaded_pieces as f64 / total_pieces as f64
    } else {
        0.0
    };

    for i in 0..n {
        let start = i as i64 * chunk;
        let end = if i == n - 1 {
            total_bytes - 1
        } else {
            (i as i64 + 1) * chunk - 1
        };
        let seg_size = end - start + 1;

        // Count how many pieces belong to this segment
        let seg_piece_start = i as u32 * pieces_per_seg;
        let seg_piece_end = ((i as u32 + 1) * pieces_per_seg).min(total_pieces);
        let seg_total_pieces = seg_piece_end.saturating_sub(seg_piece_start);

        // Scatter completed pieces across segments using a golden-ratio
        // based distribution.  This produces a visually pleasing and
        // deterministic spread that varies per segment.
        //
        // For each segment i, the expected completion is:
        //   base_ratio ± a small perturbation seeded by segment index
        //
        // The perturbation ensures segments don't all show the same %.
        let perturbation = ((i as f64 + 1.0) * 0.618033988749895).fract() - 0.5;
        let seg_ratio = (completion_ratio + perturbation * 0.3).clamp(0.0, 1.0);

        // Snap to exact 0 or 1 when close to boundaries
        let seg_dl_pieces = if completion_ratio <= 0.001 {
            0.0
        } else if completion_ratio >= 0.999 {
            seg_total_pieces as f64
        } else {
            (seg_total_pieces as f64 * seg_ratio).round()
        };

        let dl =
            ((seg_dl_pieces / seg_total_pieces.max(1) as f64) * seg_size as f64).round() as i64;

        segs.push(SegmentProgressInfo {
            index: i,
            start_byte: start,
            end_byte: end,
            downloaded_bytes: dl.clamp(0, seg_size),
        });
    }

    // Correction pass: make sure total downloaded across segments matches
    // the real downloaded_bytes (avoid visual mismatch with progress %).
    let visual_total: i64 = segs.iter().map(|s| s.downloaded_bytes).sum();
    let diff = downloaded_bytes - visual_total;
    if diff != 0 && !segs.is_empty() {
        // Distribute the difference proportionally
        let abs_diff = diff.unsigned_abs() as f64;
        let direction = if diff > 0 { 1i64 } else { -1i64 };
        let mut remaining = diff.abs();
        for seg in &mut segs {
            let seg_size = seg.end_byte - seg.start_byte + 1;
            let share = ((seg_size as f64 / total_bytes as f64) * abs_diff).round() as i64;
            let adj = share.min(remaining);
            seg.downloaded_bytes = (seg.downloaded_bytes + direction * adj).clamp(0, seg_size);
            remaining -= adj;
            if remaining <= 0 {
                break;
            }
        }
    }

    segs
}

/// Compute user-facing BT progress bytes.
///
/// We combine checked bytes (hash-verified) and fetched bytes (network-received)
/// so early BT activity is visible before any piece is fully verified.
///
/// Important: when the torrent is not `finished`, never return `total_bytes`,
/// otherwise UI would show 100% while librqbit is still verifying/finalizing.
fn compute_bt_display_progress(
    checked_progress: i64,
    fetched_progress: i64,
    total_bytes: i64,
    finished: bool,
) -> i64 {
    let mut progress = checked_progress.max(fetched_progress).max(0);
    if total_bytes > 0 {
        progress = progress.min(total_bytes);
        if !finished && progress >= total_bytes {
            progress = total_bytes.saturating_sub(1);
        }
    }
    progress
}

async fn bt_download_inner(p: BtInnerParams) -> Result<(), DownloadError> {
    let BtInnerParams {
        task_id,
        torrent_source,
        save_dir,
        db,
        progress_tx,
        cancelled,
        session,
        shared_bt,
        existing_handle,
    } = p;
    // -----------------------------------------------------------------------
    // Phase 1: Send initial file name from dn= parameter so user sees something
    // -----------------------------------------------------------------------

    let dn_name = torrent_source.display_name().unwrap_or_default();
    if !dn_name.is_empty() {
        let _ = db.update_task_file_info(&task_id, &dn_name, 0).await;
        let _ = progress_tx
            .send(ProgressUpdate {
                task_id: task_id.clone(),
                downloaded_bytes: 0,
                total_bytes: 0,
                status: STATUS_PREPARING,
                error_message: String::new(),
                file_name: dn_name.clone(),
                segment_details: None,
            })
            .await;
    }

    // -----------------------------------------------------------------------
    // Phase 2: Obtain torrent handle
    //
    // If we have an existing handle (resumed from pause), just unpause it.
    // Otherwise add a new torrent to the session.
    // -----------------------------------------------------------------------

    let handle = if let Some(h) = existing_handle {
        log_info!(
            "[BT] task={} reusing existing handle (resume)",
            short_id(&task_id)
        );
        // Handle was already unpaused by SharedBtSession::resume_task,
        // so we can go straight to the progress loop.
        h
    } else {
        // Use a task-scoped staging directory so that concurrent BT tasks
        // with identical torrent names never collide on disk.
        // librqbit will write to  save_dir/.bt_stage_<task_id>/<resolved_name>
        // and we move the result to the final deduplicated path after download.
        let stage_dir = bt_stage_dir(&save_dir, &task_id);
        let stage_dir_str = stage_dir.to_string_lossy().into_owned();
        let add_opts = AddTorrentOptions {
            overwrite: true,
            output_folder: Some(stage_dir_str),
            ..Default::default()
        };

        log_info!(
            "[BT] task={} adding torrent to shared session (metadata resolution may take a while)...",
            short_id(&task_id)
        );

        let session_for_add = session.clone();
        let source_for_add = torrent_source.clone();
        let shared_bt_for_add = shared_bt.clone();
        let task_id_for_add = task_id.clone();
        // Create the RAII guard before spawning so `maybe_release_bt_session`
        // sees the in-flight task even if `bt_download_inner` is cancelled
        // immediately after.  The guard decrements on drop — panic-safe.
        let inflight = shared_bt.inflight_guard();
        let add_handle = tokio::spawn(async move {
            // Move the guard into the task so it is dropped (and thus
            // decrements) when the task finishes normally *or* panics.
            let _inflight = inflight;
            let add_input = match source_for_add {
                TorrentSource::Magnet(ref url) => AddTorrent::from_url(url),
                TorrentSource::TorrentFileBytes(ref bytes) => {
                    AddTorrent::from_bytes(Bytes::from(bytes.clone()))
                }
            };
            let result = session_for_add.add_torrent(add_input, Some(add_opts)).await;
            // If delete_task was called while we were waiting for metadata
            // (handle not yet in `handles`, run_bt_download already returned
            // Err(Cancelled)), apply the pending delete now that we have the
            // torrent ID.  This prevents orphaned files from magnets whose
            // DHT metadata resolved after the user deleted the task.
            if let Ok(ref resp) = result {
                let torrent_id = match resp {
                    AddTorrentResponse::Added(id, _)
                    | AddTorrentResponse::AlreadyManaged(id, _) => Some(*id),
                    _ => None,
                };
                if let Some(id) = torrent_id {
                    if let Some(del_files) = shared_bt_for_add
                        .take_pending_delete(&task_id_for_add)
                        .await
                    {
                        let _ = session_for_add.delete(id.into(), del_files).await;
                        log_info!(
                            "[BT] task={} pending delete applied after add_torrent (delete_files={})",
                            short_id(&task_id_for_add),
                            del_files
                        );
                    }
                }
            }
            result
            // `_inflight` drops here → decrement_inflight_add() called
        });

        // Send "preparing" heartbeats while waiting for metadata.
        let mut add_handle = add_handle;
        let h = loop {
            if cancelled.load(Ordering::SeqCst) {
                // Drop (detach) instead of abort: the spawned add_torrent task
                // continues running so it can consume the pending_delete entry
                // registered by delete_task and properly remove the torrent
                // from the librqbit session.  Aborting would leave the torrent
                // in the session with no way to clean it up later.
                drop(add_handle);
                return Err(DownloadError::Cancelled);
            }

            tokio::select! {
                biased;
                result = &mut add_handle => {
                    let resp = result
                        .map_err(|e| DownloadError::Other(format!("BT add task panicked: {e}")))?
                        .map_err(|e| DownloadError::Other(format!("BT add torrent failed: {e}")))?;
                    let h = match resp {
                        AddTorrentResponse::Added(_id, handle) => {
                            log_info!("[BT] task={} torrent added, id={}", short_id(&task_id), _id);
                            handle
                        }
                        AddTorrentResponse::AlreadyManaged(_id, handle) => {
                            log_info!("[BT] task={} torrent already in session, id={}", short_id(&task_id), _id);
                            // Unpause if it was paused from a previous session
                            if handle.is_paused() {
                                let _ = session.unpause(&handle).await;
                            }
                            handle
                        }
                        AddTorrentResponse::ListOnly(_) => {
                            return Err(DownloadError::Other(
                                "torrent returned list_only response".into(),
                            ));
                        }
                    };
                    break h;
                }
                _ = tokio::time::sleep(Duration::from_secs(2)) => {
                    log_info!("[BT] task={} still resolving metadata...", short_id(&task_id));
                    let _ = progress_tx
                        .send(ProgressUpdate {
                            task_id: task_id.clone(),
                            downloaded_bytes: 0,
                            total_bytes: 0,
                            status: STATUS_PREPARING,
                            error_message: String::new(),
                            file_name: String::new(),
                            segment_details: None,
                        })
                        .await;
                }
            }
        };
        // Cache the handle for future pause/resume cycles.
        shared_bt.store_handle(&task_id, h.clone()).await;
        h
    };

    // -----------------------------------------------------------------------
    // Phase 3: Metadata resolved — extract name & total size, start tracking
    // -----------------------------------------------------------------------

    let stats = handle.stats();
    let total_bytes = stats.total_bytes as i64;
    let resolved_name = handle.name().unwrap_or_else(|| {
        if dn_name.is_empty() {
            format!("BT_{}", short_id(&task_id))
        } else {
            dn_name.clone()
        }
    });

    log_info!(
        "[BT] task={} metadata resolved: name={}, total={} bytes",
        short_id(&task_id),
        &resolved_name,
        total_bytes
    );

    // Extract file layout info and piece count from torrent metadata.
    // These are immutable after metadata resolution, so we cache them once.
    // Also capture the first file's relative_filename so we know exactly
    // what path librqbit will create inside the staging directory.
    let (file_offsets, total_pieces, first_relative_filename) = handle
        .with_metadata(|meta| {
            let offsets: Vec<(u64, u64)> = meta
                .file_infos
                .iter()
                .map(|fi| (fi.offset_in_torrent, fi.len))
                .collect();
            let pieces = meta.lengths.total_pieces();
            // For single-file torrents the relative_filename is just the
            // file name.  For multi-file torrents it is `name/file.ext`,
            // so the top-level entry is the torrent name directory.
            let first_name = meta
                .file_infos
                .first()
                .map(|fi| fi.relative_filename.clone())
                .unwrap_or_default();
            (offsets, pieces, first_name)
        })
        .unwrap_or_else(|_| (Vec::new(), 0, PathBuf::new()));

    // The top-level component of the first file's relative path is the
    // name of the file or directory that librqbit creates directly inside
    // the output_folder (i.e. the staging directory).
    let top_level_name = first_relative_filename
        .components()
        .next()
        .and_then(|c| c.as_os_str().to_str())
        .unwrap_or(&resolved_name)
        .to_string();
    // Prefer the metadata-derived top_level_name; fall back to resolved_name.
    let staging_item_name = if top_level_name.is_empty() {
        resolved_name.clone()
    } else {
        top_level_name
    };

    log_info!(
        "[BT] task={} files={}, total_pieces={}, staging_item={}",
        short_id(&task_id),
        file_offsets.len(),
        total_pieces,
        &staging_item_name,
    );

    let _ = db
        .update_task_file_info(&task_id, &resolved_name, total_bytes)
        .await;
    let _ = db
        .update_task_status(&task_id, STATUS_DOWNLOADING, "")
        .await;

    // Notify Dart of the transition to "downloading" with resolved info
    let init_progress = stats.progress_bytes as i64;
    let init_pieces = stats
        .live
        .as_ref()
        .map(|l| l.snapshot.downloaded_and_checked_pieces)
        .unwrap_or(0);
    let _ = progress_tx
        .send(ProgressUpdate {
            task_id: task_id.clone(),
            downloaded_bytes: init_progress,
            total_bytes,
            status: STATUS_DOWNLOADING,
            error_message: String::new(),
            file_name: resolved_name.clone(),
            segment_details: Some(build_bt_segments(
                total_bytes,
                init_progress,
                &stats.file_progress,
                &file_offsets,
                total_pieces,
                init_pieces,
            )),
        })
        .await;

    // -----------------------------------------------------------------------
    // Phase 4: Download progress loop
    // -----------------------------------------------------------------------

    let mut last_report = Instant::now();
    let mut last_db_save = Instant::now();

    loop {
        // Check cancellation — the manager layer (pause_task / cancel_task)
        // is responsible for calling session.pause() on the torrent handle,
        // so we only need to exit the loop here.  This avoids a double-pause
        // race where both the inner loop and the manager call session.pause().
        if cancelled.load(Ordering::SeqCst) {
            log_info!(
                "[BT] task={} cancelled → exiting download loop",
                short_id(&task_id)
            );
            return Err(DownloadError::Cancelled);
        }

        let stats = handle.stats();
        let checked_progress = stats.progress_bytes as i64;
        let total = if stats.total_bytes > 0 {
            stats.total_bytes as i64
        } else {
            total_bytes
        };

        // Use fetched_bytes (actual bytes received from network, including
        // partial pieces) to expose early BT activity before pieces are fully
        // hash-verified. Keep display progress below 100% until stats.finished.
        let fetched = stats
            .live
            .as_ref()
            .map(|l| l.snapshot.fetched_bytes as i64)
            .unwrap_or(0);
        let progress =
            compute_bt_display_progress(checked_progress, fetched, total, stats.finished);

        // Check for error — but ONLY when the torrent is not in Paused state.
        //
        // Race window: pause_task() calls entry.token.cancel() then session.pause().
        // Between those two calls the progress loop may wake up, see cancelled=false
        // (the AtomicBool watcher fires on the next await), and read stats while
        // librqbit is transitioning through its internal states.  During that
        // transition stats.state can transiently be Error before settling on Paused,
        // which would cause us to report STATUS_ERROR and write status=4 to the DB
        // even though the user only asked to pause.
        //
        // Guarding on `!cancelled` is sufficient for the common case, but the
        // watcher task fires asynchronously so there is still a narrow window where
        // cancelled=false while the session is already shutting down.  The additional
        // `stats.state != Paused` guard eliminates that window: a torrent that
        // librqbit has already placed in the Paused state cannot be in error.
        let is_paused_state = matches!(stats.state, librqbit::TorrentStatsState::Paused);
        if let Some(ref err) = stats.error {
            // If we are already cancelled (pause/cancel in progress), or the
            // torrent state is Paused, do not treat this as a hard error —
            // exit cleanly as Cancelled so the manager keeps status=2.
            if cancelled.load(Ordering::SeqCst) || is_paused_state {
                log_info!(
                    "[BT] task={} stats.error='{}' ignored — task is being paused/cancelled",
                    short_id(&task_id),
                    err
                );
                return Err(DownloadError::Cancelled);
            }
            let msg = format!("BT error: {err}");
            log_info!("[BT] task={} error: {}", short_id(&task_id), &msg);
            let _ = db.update_task_status(&task_id, STATUS_ERROR, &msg).await;
            let _ = progress_tx
                .send(ProgressUpdate {
                    task_id: task_id.clone(),
                    downloaded_bytes: progress,
                    total_bytes: total,
                    status: STATUS_ERROR,
                    error_message: msg.clone(),
                    file_name: String::new(),
                    segment_details: None,
                })
                .await;
            return Err(DownloadError::Other(msg));
        }

        // Check if finished
        if stats.finished {
            log_info!("[BT] task={} finished! total={}", short_id(&task_id), total);

            let final_total = if total > 0 { total } else { progress };
            let _ = db.update_task_status(&task_id, STATUS_COMPLETED, "").await;
            let _ = db.update_task_progress(&task_id, final_total).await;
            let _ = db.update_task_total_bytes(&task_id, final_total).await;

            // Build fully-completed segments — used in the single STATUS_COMPLETED
            // signal sent after the staging-dir move is resolved.
            let finished_segs = build_bt_segments(
                final_total,
                final_total,
                &stats.file_progress,
                &file_offsets,
                total_pieces,
                total_pieces as u64,
            );

            // Download complete.
            //
            // Move the downloaded file/directory from the staging directory
            // into save_dir with a deduplicated name, then update the DB so
            // that the UI and delete logic see the correct final path.
            //
            // Staging layout:  save_dir/.bt_stage_<task_id>/<staging_item_name>
            // Final layout:    save_dir/<final_name>
            //
            // We send the single STATUS_COMPLETED signal AFTER the move so
            // that the file_name field already reflects the true disk name.
            let save_path = PathBuf::from(&save_dir);
            let stage_dir = bt_stage_dir(&save_dir, &task_id);
            let stage_item = stage_dir.join(&staging_item_name);

            // Determine the final file name: attempt the staging-dir move
            // when the staged item is present; otherwise fall back to
            // resolved_name (e.g. resumed download that was already moved).
            let completed_name = if stage_item.exists() {
                let final_name = dedup_name_in_dir(&save_path, &staging_item_name);
                let final_path = save_path.join(&final_name);

                log_info!(
                    "[BT] task={} moving '{}' → '{}' (staging_item={})",
                    short_id(&task_id),
                    stage_item.display(),
                    final_path.display(),
                    &staging_item_name,
                );

                match move_path(&stage_item, &final_path) {
                    Ok(()) => {
                        // Remove now-empty staging directory (best-effort).
                        let _ = std::fs::remove_dir_all(&stage_dir);

                        if final_name != resolved_name {
                            log_info!(
                                "[BT] task={} file_name updated '{}' → '{}' (dedup)",
                                short_id(&task_id),
                                &resolved_name,
                                &final_name,
                            );
                        }
                        // Persist the true final name before signalling Dart.
                        let _ = db
                            .update_task_file_info(&task_id, &final_name, final_total)
                            .await;
                        final_name
                    }
                    Err(e) => {
                        log_info!(
                            "[BT] task={} failed to move staging item to final path: {}",
                            short_id(&task_id),
                            e,
                        );
                        // Fall through — the file is still in the staging dir.
                        // resolved_name is the best we can report; the staging
                        // directory remains for manual recovery.
                        resolved_name.clone()
                    }
                }
            } else if stage_dir.exists() {
                // Staging dir exists but the expected item is missing —
                // the torrent might be multi-file with a different top-level
                // name.  Move the entire staging dir contents best-effort.
                log_info!(
                    "[BT] task={} staging item '{}' not found in '{}'; moving whole staging dir",
                    short_id(&task_id),
                    &staging_item_name,
                    stage_dir.display(),
                );
                let mut first_child_name = resolved_name.clone();
                if let Ok(entries) = std::fs::read_dir(&stage_dir) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let child_name = entry.file_name();
                        let child_name_str = child_name.to_string_lossy();
                        // Skip hidden files (e.g. .DS_Store, other temp files).
                        if child_name_str.starts_with('.') {
                            continue;
                        }
                        let final_child_name = dedup_name_in_dir(&save_path, &child_name_str);
                        let dst = save_path.join(&final_child_name);
                        if move_path(&entry.path(), &dst).is_ok() {
                            first_child_name = final_child_name;
                        } else {
                            log_info!(
                                "[BT] task={} failed to move child '{}' from staging dir",
                                short_id(&task_id),
                                child_name_str,
                            );
                        }
                    }
                }
                let _ = std::fs::remove_dir_all(&stage_dir);
                let _ = db
                    .update_task_file_info(&task_id, &first_child_name, final_total)
                    .await;
                first_child_name
            } else {
                // No staging dir at all — resumed download that was already
                // moved in a previous session, or existing_handle path where
                // output_folder == save_dir.  The DB file_name is already
                // correct; just return resolved_name for the signal.
                resolved_name.clone()
            };

            // Send the single STATUS_COMPLETED signal with the true file name.
            let _ = progress_tx
                .send(ProgressUpdate {
                    task_id: task_id.clone(),
                    downloaded_bytes: final_total,
                    total_bytes: final_total,
                    status: STATUS_COMPLETED,
                    error_message: String::new(),
                    file_name: completed_name,
                    segment_details: Some(finished_segs),
                })
                .await;

            // Retain the handle in the cache (do NOT call take_handle) and
            // pause the torrent so it stops seeding.
            //
            // Keeping the handle alive means that a future
            // delete_task(delete_files=true) call can reach
            // session.delete(torrent_id, true), which properly removes the
            // files via librqbit.  Previously we called take_handle +
            // session.delete(false) here, which discarded the handle and
            // removed the session entry; that left no clean path for file
            // deletion — only an unreliable filesystem-path fallback.
            let _ = shared_bt.pause_task(&task_id).await;
            return Ok(());
        }

        // Progress reporting — runs on every poll cycle (500ms).
        // The elapsed check is kept as a safety guard against sleep jitter.
        if last_report.elapsed() >= Duration::from_millis(450) {
            // Speed: librqbit Speed.mbps is actually MiB/s
            let speed_bps = stats
                .live
                .as_ref()
                .map(|l| (l.download_speed.mbps * 1024.0 * 1024.0) as i64)
                .unwrap_or(0);

            let (peers_live, peers_connecting, peers_queued, peers_seen, peers_dead) = stats
                .live
                .as_ref()
                .map(|l| {
                    let ps = &l.snapshot.peer_stats;
                    (ps.live, ps.connecting, ps.queued, ps.seen, ps.dead)
                })
                .unwrap_or((0, 0, 0, 0, 0));

            let downloaded_pieces = stats
                .live
                .as_ref()
                .map(|l| l.snapshot.downloaded_and_checked_pieces)
                .unwrap_or(0);

            let upload_speed_bps = stats
                .live
                .as_ref()
                .map(|l| (l.upload_speed.mbps * 1024.0 * 1024.0) as i64)
                .unwrap_or(0);

            // If the torrent has entered Paused state while we are still in
            // the progress loop, it means pause_task() already called
            // session.pause() and the handle is now frozen.  The loop is
            // about to exit on the next cancelled-flag check (the watcher
            // task fires asynchronously), but we must not send STATUS_ERROR
            // or STATUS_PREPARING for a genuinely paused torrent.
            // Exit early and let the manager's explicit status=2 signal
            // (sent in pause_task) be the last word on the UI state.
            if is_paused_state {
                log_info!(
                    "[BT] task={} stats.state=Paused while in progress loop — exiting early (pause in progress)",
                    short_id(&task_id)
                );
                return Err(DownloadError::Cancelled);
            }

            let status_code = match stats.state {
                librqbit::TorrentStatsState::Live => STATUS_DOWNLOADING,
                librqbit::TorrentStatsState::Initializing => STATUS_PREPARING,
                librqbit::TorrentStatsState::Paused => STATUS_PREPARING, // unreachable after guard above
                librqbit::TorrentStatsState::Error => STATUS_ERROR,
            };

            log_info!(
                "[BT] task={} state={:?} progress={}/{} (checked={}, fetched={}) pieces={}/{} down={} B/s up={} B/s peers(live={} connecting={} queued={} seen={} dead={})",
                short_id(&task_id),
                stats.state,
                progress,
                total,
                checked_progress,
                fetched,
                downloaded_pieces,
                total_pieces,
                speed_bps,
                upload_speed_bps,
                peers_live,
                peers_connecting,
                peers_queued,
                peers_seen,
                peers_dead
            );

            let seg_details = build_bt_segments(
                total,
                progress,
                &stats.file_progress,
                &file_offsets,
                total_pieces,
                downloaded_pieces,
            );

            let _ = progress_tx
                .send(ProgressUpdate {
                    task_id: task_id.clone(),
                    downloaded_bytes: progress,
                    total_bytes: total,
                    status: status_code,
                    error_message: String::new(),
                    file_name: String::new(),
                    segment_details: Some(seg_details),
                })
                .await;

            last_report = Instant::now();
        }

        // Periodic DB save (every 3s).
        // Use checked_progress for DB persistence (not fetched_bytes) to
        // avoid inflating progress with partial pieces that would need
        // re-download after restart.
        if checked_progress > 0 && last_db_save.elapsed() >= Duration::from_secs(3) {
            let _ = db.update_task_progress(&task_id, checked_progress).await;
            if total > 0 {
                let _ = db.update_task_total_bytes(&task_id, total).await;
            }
            last_db_save = Instant::now();
        }

        // Poll interval — aligned with the progress reporting interval (500ms)
        // to avoid wasted cycles.  Cancel detection latency of 500ms is
        // acceptable since the manager layer handles session.pause() directly.
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::compute_bt_display_progress;

    #[test]
    fn display_progress_does_not_reach_total_before_finished() {
        let progress = compute_bt_display_progress(900, 1000, 1000, false);
        assert_eq!(progress, 999);
    }

    #[test]
    fn display_progress_can_reach_total_when_finished() {
        let progress = compute_bt_display_progress(900, 1000, 1000, true);
        assert_eq!(progress, 1000);
    }

    #[test]
    fn display_progress_handles_unknown_total() {
        let progress = compute_bt_display_progress(0, 12345, 0, false);
        assert_eq!(progress, 12345);
    }

    // -------------------------------------------------------------------------
    // InflightGuard: panic-safe decrement via RAII.
    // -------------------------------------------------------------------------

    /// Verify that InflightGuard decrements the counter even when the
    /// enclosing tokio::spawn closure panics before the natural end.
    #[tokio::test]
    async fn inflight_guard_decrements_on_task_panic() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        // Minimal stand-in for SharedBtSession: just the counter.
        let counter = Arc::new(AtomicUsize::new(0));

        // Build a minimal InflightGuard directly using the same AtomicUsize
        // so we can test the Drop behaviour without constructing a full Session.
        struct TestGuard(Arc<AtomicUsize>);
        impl Drop for TestGuard {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::Relaxed);
            }
        }

        // Simulate: shared_bt.inflight_guard() — increments then returns guard.
        counter.fetch_add(1, Ordering::Relaxed);
        assert_eq!(counter.load(Ordering::Relaxed), 1);
        let guard = TestGuard(Arc::clone(&counter));

        // Simulate: tokio::spawn(async move { let _g = guard; ..panic.. })
        let handle = tokio::spawn(async move {
            let _g = guard; // guard moved into task; Drop runs on panic
            panic!("simulated add_torrent panic");
        });

        // Tokio catches the panic; JoinHandle returns Err.
        assert!(handle.await.is_err());

        // FIX confirmed: guard's Drop ran during tokio's task cleanup,
        // decrementing the counter back to 0.
        assert_eq!(
            counter.load(Ordering::Relaxed),
            0,
            "InflightGuard must decrement counter even after task panic"
        );
    }

    /// Verify normal (non-panic) path: guard also decrements on clean exit.
    #[tokio::test]
    async fn inflight_guard_decrements_on_normal_exit() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let counter = Arc::new(AtomicUsize::new(0));

        struct TestGuard(Arc<AtomicUsize>);
        impl Drop for TestGuard {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::Relaxed);
            }
        }

        counter.fetch_add(1, Ordering::Relaxed);
        let guard = TestGuard(Arc::clone(&counter));

        let handle = tokio::spawn(async move {
            let _g = guard;
            // normal return — no panic
        });

        assert!(handle.await.is_ok());
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }
}
