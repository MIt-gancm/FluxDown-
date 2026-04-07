/// 在系统文件管理器中打开父目录并选中文件。
/// 仅在 Dart 侧确认 `FileSystemEntity.type(path) == file` 后调用。
///
/// | 平台    | 实现                                              |
/// |---------|---------------------------------------------------|
/// | Windows | `explorer.exe /select,"path"` via `raw_arg()`     |
/// | macOS   | `open -R path`                                    |
/// | Linux   | D-Bus `org.freedesktop.FileManager1.ShowItems`，  |
/// |         | 失败则 fallback 到 `xdg-open` 打开父目录          |
pub fn reveal(path: &str) {
    #[cfg(target_os = "windows")]
    reveal_windows(path);

    #[cfg(target_os = "macos")]
    reveal_macos(path);

    #[cfg(target_os = "linux")]
    reveal_linux(path);
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------
//
// `CommandExt::raw_arg()` 将字符串原样写入 `CreateProcessW` 命令行，
// 不经过 Rust/Dart 的参数引号转义逻辑。Explorer 自己解析命令行，
// 正确识别 `/select,"含空格/CJK 路径"` 并高亮选中文件。
#[cfg(target_os = "windows")]
fn reveal_windows(path: &str) {
    use std::os::windows::process::CommandExt;
    let arg = format!(r#"/select,"{}""#, path);
    if let Err(e) = std::process::Command::new("explorer.exe")
        .raw_arg(&arg)
        .spawn()
    {
        crate::logger::log_info!("[reveal] Windows: explorer /select failed: {e}");
    }
}

// ---------------------------------------------------------------------------
// macOS
// ---------------------------------------------------------------------------
//
// `open -R path` 相当于 Finder 的「显示原身」：打开父目录并选中文件。
// path 作为独立参数传入，Rust 自动处理引号，无转义问题。
#[cfg(target_os = "macos")]
fn reveal_macos(path: &str) {
    if let Err(e) = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn()
    {
        crate::logger::log_info!("[reveal] macOS: open -R failed: {e}");
    }
}

// ---------------------------------------------------------------------------
// Linux
// ---------------------------------------------------------------------------
//
// 优先通过 D-Bus `org.freedesktop.FileManager1.ShowItems` 请求文件管理器
// 打开父目录并选中文件。Nautilus、Dolphin、Thunar、Nemo 等主流 FM 均实现
// 此接口（XDG 规范）。失败（FM 未运行 / 不支持此接口）时 fallback 到
// `xdg-open` 打开父目录（无选中）。
#[cfg(target_os = "linux")]
fn reveal_linux(path: &str) {
    // 将文件系统路径转为 file:// URI（空格等特殊字符 percent-encode）
    let uri = path_to_file_uri(path);

    let ok = std::process::Command::new("dbus-send")
        .args([
            "--session",
            "--dest=org.freedesktop.FileManager1",
            "--type=method_call",
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1.ShowItems",
            &format!("array:string:{uri}"),
            "string:",
        ])
        .spawn()
        .map(|mut c| c.wait().map(|s| s.success()).unwrap_or(false))
        .unwrap_or(false);

    if !ok {
        // fallback: 打开父目录（无文件选中）
        let dir = std::path::Path::new(path)
            .parent()
            .map(|p| path_to_file_uri(&p.to_string_lossy()))
            .unwrap_or_else(|| path_to_file_uri(path));
        if let Err(e) = std::process::Command::new("xdg-open").arg(&dir).spawn() {
            crate::logger::log_info!("[reveal] Linux: xdg-open fallback failed: {e}");
        }
    }
}

/// 将文件系统路径转换为 `file://` URI（percent-encode 非 ASCII 及保留字符）。
#[cfg(target_os = "linux")]
fn path_to_file_uri(path: &str) -> String {
    let encoded: String = path
        .chars()
        .flat_map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.' | '~') {
                vec![c]
            } else {
                // percent-encode: 每个字节写成 %XX
                c.to_string()
                    .as_bytes()
                    .iter()
                    .flat_map(|b| format!("%{b:02X}").chars().collect::<Vec<_>>())
                    .collect()
            }
        })
        .collect();
    format!("file://{encoded}")
}
