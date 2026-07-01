//! 全局文件日志 — 与 Dart 端 LogService 写入同一目录/文件，按日期分文件。
//!
//! - 日志目录：由 `data_dir::resolve_data_dir()` 决定，加 `/logs` 后缀
//!   - Linux: `~/.local/share/fluxdown/logs/`
//!   - macOS: `~/Library/Application Support/fluxdown/logs/`
//!   - Windows 便携版: `<exe_dir>/logs/`
//!   - Windows 安装版: `%LOCALAPPDATA%/FluxDown/logs/`
//! - 文件名：`fluxdown_YYYY-MM-DD.log`，分卷为 `fluxdown_YYYY-MM-DD.N.log`（与 Dart 端完全一致）
//! - 两端都以 append 模式写入，POSIX `O_APPEND` 保证单次 write 原子性
//! - 启动时自动清理 7 天前的日志文件
//!
//! ## 自动分割与清理（与 Dart 端 log_service.dart 协议一致）
//! - 单文件超过 2MB 自动分割到 `fluxdown_YYYY-MM-DD.N.log` 分卷；
//! - 日志总大小超过上限（默认 10MB，可通过 `set_max_total_bytes` 由设置覆盖）时
//!   按（日期, 分卷序号）从最旧开始删除；
//! - 清理只做目录遍历 + metadata，不读文件内容，内存占用极小。
//!
//! ## 用法
//! ```ignore
//! // 初始化（Rust runtime 启动时调用一次）
//! crate::logger::init();
//!
//! // 普通日志
//! log_info!("[module] some message: {}", value);
//!
//! // 错误日志（立即刷盘）
//! log_error!("[module] failed: {}", err);
//! ```

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use chrono::Local;

static LOGGER: OnceLock<AppLogger> = OnceLock::new();

/// 日志保留天数
const LOG_RETENTION_DAYS: u64 = 7;

/// 单个日志文件大小上限，超过则自动分割到新分卷
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// 日志目录总大小默认上限（可由设置覆盖）
const DEFAULT_MAX_TOTAL_BYTES: u64 = 10 * 1024 * 1024;

/// 距上次 stat 实际文件大小的写入字节阈值。
/// Dart/Rust 两端写同一文件，自身计数会低估，需周期性校准。
const SIZE_CHECK_INTERVAL_BYTES: u64 = 64 * 1024;

struct LogState {
    date_tag: String,
    /// 当前日期内的分卷序号（0 = 无序号的首个文件）
    part: u32,
    file: Option<File>,
    /// 当前文件大小估算（打开时 stat 初始化 + 自身写入累加，周期性校准）
    approx_size: u64,
    /// 距上次 stat 校准以来自身写入的字节数
    bytes_since_stat: u64,
}

struct AppLogger {
    log_dir: PathBuf,
    max_total_bytes: AtomicU64,
    state: Mutex<LogState>,
}

impl AppLogger {
    fn new(log_dir: PathBuf) -> Self {
        fs::create_dir_all(&log_dir).ok();
        Self {
            log_dir,
            max_total_bytes: AtomicU64::new(DEFAULT_MAX_TOTAL_BYTES),
            state: Mutex::new(LogState {
                date_tag: String::new(),
                part: 0,
                file: None,
                approx_size: 0,
                bytes_since_stat: 0,
            }),
        }
    }

    // ── 内部写入 ──

    /// 写入一行日志，自动按日期切换文件、按大小分割。`flush` 为 true 时立即刷盘。
    fn write_impl(&self, message: &str, flush: bool) {
        let now = Local::now();
        let date_tag = now.format("%Y-%m-%d").to_string();
        let ts = now.format("%H:%M:%S%.3f").to_string();
        let line = format!("{ts} {message}\n");

        let mut state = match self.state.lock() {
            Ok(s) => s,
            Err(poisoned) => poisoned.into_inner(),
        };

        self.ensure_file(&mut state, &date_tag);

        if let Some(ref mut f) = state.file {
            let _ = f.write_all(line.as_bytes());
            if flush {
                let _ = f.flush();
            }
        }
        state.approx_size += line.len() as u64;
        state.bytes_since_stat += line.len() as u64;
        self.maybe_roll_by_size(&mut state);
    }

    /// 确保日志文件已打开且日期匹配，否则切换到新文件。
    fn ensure_file(&self, state: &mut LogState, date_tag: &str) {
        if state.date_tag == date_tag && state.file.is_some() {
            return;
        }
        // 关闭旧文件（如有）
        if let Some(ref mut old) = state.file {
            let _ = old.flush();
        }
        state.file = None;
        state.date_tag = date_tag.to_string();
        state.part = self.scan_active_part(date_tag);
        self.open_current_file(state);
    }

    /// 打开 (date_tag, part) 对应的日志文件（append 模式），并 stat 初始化大小估算。
    fn open_current_file(&self, state: &mut LogState) {
        let path = self.file_path(&state.date_tag, state.part);
        if let Ok(f) = OpenOptions::new().create(true).append(true).open(&path) {
            state.approx_size = f.metadata().map(|m| m.len()).unwrap_or(0);
            state.bytes_since_stat = 0;
            state.file = Some(f);
        }
    }

    fn file_path(&self, date_tag: &str, part: u32) -> PathBuf {
        let name = if part == 0 {
            format!("fluxdown_{date_tag}.log")
        } else {
            format!("fluxdown_{date_tag}.{part}.log")
        };
        self.log_dir.join(name)
    }

    /// 找到 `date_tag` 当天已有的最大分卷序号；若该分卷已写满则返回下一个序号。
    /// Dart 端可能已创建更高序号的分卷，两端通过该扫描收敛到同一文件。
    fn scan_active_part(&self, date_tag: &str) -> u32 {
        let mut max_part: Option<u32> = None;
        if let Ok(entries) = fs::read_dir(&self.log_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_str().unwrap_or("");
                if let Some((d, part)) = parse_log_name(name)
                    && d == date_tag
                {
                    max_part = Some(max_part.map_or(part, |m| m.max(part)));
                }
            }
        }
        let Some(max_part) = max_part else {
            return 0;
        };
        let size = fs::metadata(self.file_path(date_tag, max_part))
            .map(|m| m.len())
            .unwrap_or(0);
        if size >= MAX_FILE_BYTES {
            max_part + 1
        } else {
            max_part
        }
    }

    /// 大小检查与自动分割：自身写入量达到阈值时 stat 一次实际大小校准，
    /// 超过单文件上限则切换到新分卷并触发总量清理。
    fn maybe_roll_by_size(&self, state: &mut LogState) {
        if state.bytes_since_stat >= SIZE_CHECK_INTERVAL_BYTES {
            state.bytes_since_stat = 0;
            if let Some(ref f) = state.file
                && let Ok(meta) = f.metadata()
            {
                state.approx_size = meta.len();
            }
        }
        if state.approx_size < MAX_FILE_BYTES || state.date_tag.is_empty() {
            return;
        }

        if let Some(ref mut old) = state.file {
            let _ = old.flush();
        }
        state.file = None;
        // 防御：保证分卷序号单调递增，避免重新打开已写满的文件
        let next = self.scan_active_part(&state.date_tag);
        state.part = next.max(state.part + 1);
        self.open_current_file(state);
        self.enforce_total_size(state);
    }

    /// 写入启动 header
    fn write_session_header(&self) {
        let now = Local::now();
        let header = format!(
            "\n====== Rust runtime log session started at {} ======\n  pid: {}\n  exe: {}\n\n",
            now.format("%Y-%m-%d %H:%M:%S"),
            std::process::id(),
            std::env::current_exe()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
        );

        let mut state = match self.state.lock() {
            Ok(s) => s,
            Err(poisoned) => poisoned.into_inner(),
        };

        let date_tag = now.format("%Y-%m-%d").to_string();
        self.ensure_file(&mut state, &date_tag);

        if let Some(ref mut f) = state.file {
            let _ = f.write_all(header.as_bytes());
            let _ = f.flush();
        }
        state.approx_size += header.len() as u64;
    }

    /// 清理超过 `max_days` 天的 `fluxdown_*.log` 文件
    fn cleanup_old_logs(&self, max_days: u64) {
        let cutoff = SystemTime::now() - Duration::from_secs(max_days * 86400);
        let entries = match fs::read_dir(&self.log_dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with("fluxdown_") || !name.ends_with(".log") {
                continue;
            }
            if let Ok(meta) = fs::metadata(&path)
                && let Ok(modified) = meta.modified()
                && modified < cutoff
            {
                let _ = fs::remove_file(&path);
            }
        }
    }

    /// 总大小超量清理：按（日期, 分卷序号）从最旧开始删除，
    /// 直到总大小回到上限内。当前活跃文件不删除。
    fn enforce_total_size(&self, state: &LogState) {
        let max_total = self.max_total_bytes.load(Ordering::Relaxed);
        let entries = match fs::read_dir(&self.log_dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        // (date, part, path, size)
        let mut files: Vec<(String, u32, PathBuf, u64)> = Vec::new();
        let mut total: u64 = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let Some((date, part)) = parse_log_name(name) else {
                continue;
            };
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            total += size;
            files.push((date.to_string(), part, path, size));
        }
        if total <= max_total {
            return;
        }

        files.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        let active = self.file_path(&state.date_tag, state.part);
        for (_, _, path, size) in files {
            if total <= max_total {
                break;
            }
            if path == active {
                continue;
            }
            // 删除失败（如被另一端持有句柄）不影响其他文件
            if fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(size);
            }
        }
    }
}

/// 解析日志文件名 `fluxdown_YYYY-MM-DD.log` / `fluxdown_YYYY-MM-DD.N.log`，
/// 返回 (日期, 分卷序号)。非日志文件返回 None。
fn parse_log_name(name: &str) -> Option<(&str, u32)> {
    let rest = name.strip_prefix("fluxdown_")?.strip_suffix(".log")?;
    let (date, part) = match rest.split_once('.') {
        Some((d, p)) => (d, p.parse::<u32>().ok()?),
        None => (rest, 0),
    };
    // 校验日期格式 YYYY-MM-DD（避免误删其他 fluxdown_*.log 命名的文件）
    if date.len() != 10 {
        return None;
    }
    let bytes = date.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        let ok = if i == 4 || i == 7 {
            *b == b'-'
        } else {
            b.is_ascii_digit()
        };
        if !ok {
            return None;
        }
    }
    Some((date, part))
}

// ══════════════════════════════════════════════════
//  公开 API
// ══════════════════════════════════════════════════

/// 初始化全局日志。应在 Rust runtime 启动时调用一次。
///
/// 自动清理 7 天前的日志文件、执行总量超量清理，并写入 session header。
pub fn init() {
    let log_dir = resolve_log_dir();
    let logger = AppLogger::new(log_dir);
    logger.cleanup_old_logs(LOG_RETENTION_DAYS);
    if LOGGER.set(logger).is_ok()
        && let Some(l) = LOGGER.get()
    {
        l.write_session_header();
        let state = match l.state.lock() {
            Ok(s) => s,
            Err(poisoned) => poisoned.into_inner(),
        };
        l.enforce_total_size(&state);
    }
}

/// 设置日志目录总大小上限（字节），由设置项 `log_max_size_mb` 驱动。
/// 立即执行一次超量清理。低于 1MB 的值会被忽略。
pub fn set_max_total_bytes(bytes: u64) {
    let Some(logger) = LOGGER.get() else {
        return;
    };
    if bytes < 1024 * 1024 {
        return;
    }
    logger.max_total_bytes.store(bytes, Ordering::Relaxed);
    let state = match logger.state.lock() {
        Ok(s) => s,
        Err(poisoned) => poisoned.into_inner(),
    };
    logger.enforce_total_size(&state);
}

/// 写入一条日志（缓冲写入，由 OS 按需刷盘）。
#[inline]
pub fn write(message: &str) {
    if let Some(logger) = LOGGER.get() {
        logger.write_impl(message, false);
    }
    #[cfg(debug_assertions)]
    eprintln!("{message}");
}

/// 写入一条错误日志（立即刷盘，确保崩溃前不丢失）。
#[inline]
#[allow(dead_code)]
pub fn write_error(message: &str) {
    if let Some(logger) = LOGGER.get() {
        logger.write_impl(message, true);
    }
    #[cfg(debug_assertions)]
    eprintln!("{message}");
}

// ══════════════════════════════════════════════════
//  路径解析 — 委托 data_dir 模块，与 Dart 端 platform_utils 一致
// ══════════════════════════════════════════════════

fn resolve_log_dir() -> PathBuf {
    crate::data_dir::resolve_data_dir(None)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("logs")
}

// ══════════════════════════════════════════════════
//  宏 — 直接替换 rinf::debug_print!
//
//  `#[macro_export]` 把宏放到 crate 根路径(`fluxdown_engine::log_info!`),
//  下方 `pub use` 把它们重新导出回 `logger` 模块路径,使得
//  `fluxdown_engine::logger::log_info!` 与 hub 侧历史用法
//  `crate::logger::log_info!`(经 hub 的 `pub use` shim 转发)保持一致。
//  宏体内必须用 `$crate` 而非 `crate`——`crate::` 在 `macro_rules!` 里按
//  *调用点* 所在 crate 解析,只有 `$crate` 才会不论调用点在哪个 crate,
//  始终指回定义宏的 `fluxdown_engine`。
// ══════════════════════════════════════════════════

/// 记录普通日志，格式同 `format!()`。
///
/// ```ignore
/// log_info!("[actor] task created: id={}", id);
/// ```
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::logger::write(&format!($($arg)*))
    };
}

/// 记录错误日志（立即刷盘），格式同 `format!()`。
///
/// ```ignore
/// log_error!("[actor] database open failed: {}", e);
/// ```
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::logger::write_error(&format!($($arg)*))
    };
}

#[allow(unused_imports)]
pub use crate::log_error;
#[allow(unused_imports)]
pub use crate::log_info;

#[cfg(test)]
mod tests {
    use super::parse_log_name;

    #[test]
    fn parse_plain_daily_file() {
        assert_eq!(
            parse_log_name("fluxdown_2026-06-10.log"),
            Some(("2026-06-10", 0))
        );
    }

    #[test]
    fn parse_part_file() {
        assert_eq!(
            parse_log_name("fluxdown_2026-06-10.3.log"),
            Some(("2026-06-10", 3))
        );
    }

    #[test]
    fn reject_non_log_names() {
        assert_eq!(parse_log_name("fluxdown_logs.zip"), None);
        assert_eq!(parse_log_name("fluxdown_backup.log"), None);
        assert_eq!(parse_log_name("fluxdown_2026-06-10.abc.log"), None);
        assert_eq!(parse_log_name("other_2026-06-10.log"), None);
    }
}
