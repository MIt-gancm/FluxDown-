//! 下载来源标记（Mark-of-the-Web）。
//!
//! 网络下载完成的文件需要带上「来自互联网」的系统标记，否则 SmartScreen、
//! Office 受保护视图、Gatekeeper 都把它当作本地可信文件：
//! - Windows：NTFS 备用数据流 `<file>:Zone.Identifier`（`ZoneId=3`）；
//! - macOS：`com.apple.quarantine` 扩展属性；
//! - 其它平台没有对应机制，空操作。
//!
//! 标记不改变文件内容，对 BT 做种 / 续传无影响。所有失败（FAT/exFAT 不支持
//! ADS、只读介质、`xattr` 缺失）只记日志，绝不影响任务完成状态。

use std::path::{Path, PathBuf};

use crate::logger::log_info;

/// 目录产物（BT 多文件）最多标记的文件数，防止超大种子拖住后台任务。
#[cfg(any(windows, target_os = "macos"))]
const MAX_MARKED_FILES: usize = 50_000;

/// 后台为 `path`（文件或目录）写入来源标记；立即返回，不阻塞调用方。
///
/// `host_url` 为下载源地址、`referrer` 为来源页（均可为空）。目录会被递归展开，
/// 符号链接不跟随。
pub fn spawn_mark_downloaded(path: PathBuf, host_url: String, referrer: String) {
    if !cfg!(any(windows, target_os = "macos")) {
        return;
    }
    tokio::spawn(async move {
        let files = match tokio::task::spawn_blocking(move || collect_files(&path)).await {
            Ok(files) => files,
            Err(e) => {
                log_info!("[motw] collect files failed: {e}");
                return;
            }
        };
        if files.is_empty() {
            return;
        }
        platform::apply(files, host_url, referrer).await;
    });
}

/// 展开目标：文件 → 自身；目录 → 其下全部普通文件（不跟随符号链接）。
#[cfg(any(windows, target_os = "macos", test))]
fn collect_files(path: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return out;
    };
    if meta.is_file() {
        out.push(path.to_path_buf());
        return out;
    }
    if !meta.is_dir() {
        return out;
    }
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if file_type.is_file() {
                out.push(entry.path());
                #[cfg(any(windows, target_os = "macos"))]
                if out.len() >= MAX_MARKED_FILES {
                    return out;
                }
            }
        }
    }
    out
}

#[cfg(not(any(windows, target_os = "macos", test)))]
fn collect_files(_path: &Path) -> Vec<PathBuf> {
    Vec::new()
}

/// 只接受 http(s)/ftp URL 写入标记，剥掉内嵌凭据与控制字符（ADS 内容按行解析，
/// 换行会注入额外键）。其它 scheme（`magnet:`、`torrent-file://local` 等）返回空。
#[cfg(any(windows, test))]
fn sanitize_zone_url(url: &str) -> String {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("ftp://"))
    {
        return String::new();
    }
    if url.chars().any(char::is_control) {
        return String::new();
    }
    let Some(scheme_end) = url.find("://") else {
        return String::new();
    };
    let (scheme, rest) = url.split_at(scheme_end + 3);
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(authority_end);
    let host = authority.rsplit('@').next().unwrap_or(authority);
    format!("{scheme}{host}{tail}")
}

/// `Zone.Identifier` 流内容：`ZoneId=3`（Internet），可带 ReferrerUrl / HostUrl。
#[cfg(any(windows, test))]
fn zone_identifier_content(host_url: &str, referrer: &str) -> String {
    let mut content = String::from("[ZoneTransfer]\r\nZoneId=3\r\n");
    let referrer = sanitize_zone_url(referrer);
    if !referrer.is_empty() {
        content.push_str("ReferrerUrl=");
        content.push_str(&referrer);
        content.push_str("\r\n");
    }
    let host = sanitize_zone_url(host_url);
    if !host.is_empty() {
        content.push_str("HostUrl=");
        content.push_str(&host);
        content.push_str("\r\n");
    }
    content
}

/// `com.apple.quarantine` 值：`<flags>;<十六进制秒>;<应用名>;`（UUID 段可省）。
#[cfg(any(target_os = "macos", test))]
fn quarantine_value(now_secs: u64) -> String {
    format!("0081;{now_secs:x};FluxDown;")
}

#[cfg(windows)]
mod platform {
    use super::{Path, PathBuf, log_info, zone_identifier_content};

    pub(super) async fn apply(files: Vec<PathBuf>, host_url: String, referrer: String) {
        let result = tokio::task::spawn_blocking(move || {
            let content = zone_identifier_content(&host_url, &referrer);
            for file in &files {
                if let Err(e) = write_zone_identifier(file, &content) {
                    log_info!("[motw] Zone.Identifier failed for {}: {e}", file.display());
                }
            }
        })
        .await;
        if let Err(e) = result {
            log_info!("[motw] mark task failed: {e}");
        }
    }

    fn write_zone_identifier(file: &Path, content: &str) -> std::io::Result<()> {
        let mut stream = file.as_os_str().to_owned();
        stream.push(":Zone.Identifier");
        std::fs::write(PathBuf::from(stream), content)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{PathBuf, log_info, quarantine_value};

    /// 单次 `xattr` 调用处理的文件数（避免超长命令行，又不为每个文件起一个进程）。
    const BATCH: usize = 200;

    pub(super) async fn apply(files: Vec<PathBuf>, _host_url: String, _referrer: String) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let value = quarantine_value(now);
        for chunk in files.chunks(BATCH) {
            let mut cmd = tokio::process::Command::new("/usr/bin/xattr");
            crate::proc::no_console_window(&mut cmd);
            cmd.arg("-w").arg("com.apple.quarantine").arg(&value);
            for file in chunk {
                cmd.arg(file);
            }
            cmd.stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true);
            match cmd.status().await {
                Ok(status) if status.success() => {}
                Ok(status) => log_info!("[motw] xattr exited with {status}"),
                Err(e) => log_info!("[motw] xattr spawn failed: {e}"),
            }
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    use super::PathBuf;

    pub(super) async fn apply(_files: Vec<PathBuf>, _host_url: String, _referrer: String) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_content_carries_sanitized_urls_and_no_injected_keys() {
        let content = zone_identifier_content(
            "https://user:secret@cdn.example.com/a/b.bin?x=1",
            "https://example.com/page",
        );
        assert_eq!(
            content,
            "[ZoneTransfer]\r\nZoneId=3\r\nReferrerUrl=https://example.com/page\r\n\
             HostUrl=https://cdn.example.com/a/b.bin?x=1\r\n"
        );

        // 控制字符（换行）整条丢弃，不能让调用方注入额外键。
        let injected = zone_identifier_content("https://a.test/x\r\nZoneId=0", "");
        assert_eq!(injected, "[ZoneTransfer]\r\nZoneId=3\r\n");
    }

    #[test]
    fn non_network_urls_are_not_written() {
        assert_eq!(sanitize_zone_url("magnet:?xt=urn:btih:abc"), "");
        assert_eq!(sanitize_zone_url("torrent-file://local"), "");
        assert_eq!(sanitize_zone_url(""), "");
    }

    #[test]
    fn quarantine_value_is_hex_timestamped() {
        assert_eq!(quarantine_value(255), "0081;ff;FluxDown;");
    }

    #[test]
    fn collect_files_expands_directories_and_skips_symlinks_free_paths() {
        let root = std::env::temp_dir().join(format!(
            "fluxdown_motw_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let nested = root.join("sub");
        assert!(std::fs::create_dir_all(&nested).is_ok());
        assert!(std::fs::write(root.join("a.bin"), b"a").is_ok());
        assert!(std::fs::write(nested.join("b.bin"), b"b").is_ok());

        let mut files = collect_files(&root);
        files.sort();
        assert_eq!(files, vec![root.join("a.bin"), nested.join("b.bin")]);
        assert_eq!(collect_files(&root.join("a.bin")), vec![root.join("a.bin")]);
        assert!(collect_files(&root.join("missing")).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
