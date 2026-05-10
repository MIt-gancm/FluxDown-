/// 在系统/用户指定的文件管理器中打开目录或定位文件。
///
/// 调用方传入：
/// - `path`        — 文件或目录的绝对路径（自动检测类型）
/// - `file_tpl`    — 用户自定义"定位文件"命令模板，空表示使用平台默认
/// - `dir_tpl`     — 用户自定义"打开目录"命令模板，空表示使用平台默认
///
/// 模板占位符：
/// - `{path}` — 完整文件路径（仅 reveal-file 场景有意义）
/// - `{dir}`  — 目录路径（文件 → 父目录，目录 → 自身）
/// 占位符在替换时会做平台 shell 转义，用户无需在模板中再加引号。
///
/// 平台默认行为（无模板时）：
/// | 平台    | 文件                                                   | 目录                          |
/// |---------|--------------------------------------------------------|-------------------------------|
/// | Windows | `explorer.exe /select,"path"`                          | `cmd /c start "" "dir"`       |
/// | macOS   | `open -R path`                                         | `open path`                   |
/// | Linux   | D-Bus `FileManager1.ShowItems`，失败 fallback xdg-open | `xdg-open dir`                |
pub fn reveal(path: &str, file_tpl: &str, dir_tpl: &str) {
    use std::path::Path;

    // 判定 file/dir：路径若不存在则按"末段是否含 . "猜测，与 Dart 端旧逻辑一致。
    let p = Path::new(path);
    let is_file = match std::fs::metadata(p) {
        Ok(m) => m.is_file(),
        Err(_) => p
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.contains('.'))
            .unwrap_or(false),
    };

    // 推算目录路径
    let dir: String = if is_file {
        p.parent()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string())
    } else {
        path.to_string()
    };

    // 优先走用户自定义模板
    let tpl = if is_file { file_tpl } else { dir_tpl };
    if !tpl.trim().is_empty() {
        if run_template(tpl, path, &dir) {
            return;
        }
        crate::logger::log_info!("[reveal] custom template failed, falling back to platform default");
    }

    // 平台默认
    if is_file {
        platform_reveal_file(path);
    } else {
        platform_open_dir(&dir);
    }
}

// ---------------------------------------------------------------------------
// 模板执行：占位符替换 + shell 解析
// ---------------------------------------------------------------------------
//
// 设计理由：
// 用户提供的命令是字符串（含空格、引号、管道等），最稳的执行方式是交给系统
// shell 解析。Windows 用 `cmd /c`，Unix 用 `sh -c`。占位符替换前对路径做
// 平台 shell 转义，用户在模板里写 `nautilus --select {path}` 即可，不需要
// 自己包引号。

fn run_template(tpl: &str, path: &str, dir: &str) -> bool {
    let cmdline = substitute(tpl, path, dir);
    crate::logger::log_info!("[reveal] running custom: {cmdline}");

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // raw_arg 把 `/c <cmdline>` 整段原样塞进 CreateProcessW，
        // 避免 Rust 的参数转义改写用户写好的引号。
        match std::process::Command::new("cmd.exe")
            .raw_arg(format!("/c {cmdline}"))
            .spawn()
        {
            Ok(_) => true,
            Err(e) => {
                crate::logger::log_info!("[reveal] cmd /c spawn failed: {e}");
                false
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        match std::process::Command::new("sh").arg("-c").arg(&cmdline).spawn() {
            Ok(_) => true,
            Err(e) => {
                crate::logger::log_info!("[reveal] sh -c spawn failed: {e}");
                false
            }
        }
    }
}

fn substitute(tpl: &str, path: &str, dir: &str) -> String {
    let path_q = shell_quote(path);
    let dir_q = shell_quote(dir);
    tpl.replace("{path}", &path_q).replace("{dir}", &dir_q)
}

#[cfg(target_os = "windows")]
fn shell_quote(s: &str) -> String {
    // cmd 引号规则：包在 "..." 中；内层 " 在 cmd 上下文里需写成 \"，
    // 同时为了对付 cmd 的 ^ & | < > 等元字符，整串再用 ^ 转义会破坏路径，
    // 所以最务实做法是禁止路径中出现 "（实际文件名也不允许 " 字符）。
    if s.contains('"') {
        // 极端兜底：替换为下划线避免命令注入
        let cleaned: String = s.chars().map(|c| if c == '"' { '_' } else { c }).collect();
        format!("\"{cleaned}\"")
    } else {
        format!("\"{s}\"")
    }
}

#[cfg(not(target_os = "windows"))]
fn shell_quote(s: &str) -> String {
    // POSIX 单引号转义：单引号本身写成 '\''
    let escaped = s.replace('\'', "'\\''");
    format!("'{escaped}'")
}

// ---------------------------------------------------------------------------
// 平台默认：reveal 文件（父目录 + 选中）
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn platform_reveal_file(path: &str) {
    use std::os::windows::process::CommandExt;
    // /select 是 Explorer 私有 verb，仅 explorer.exe 识别
    let arg = format!(r#"/select,"{}""#, path);
    if let Err(e) = std::process::Command::new("explorer.exe")
        .raw_arg(&arg)
        .spawn()
    {
        crate::logger::log_info!("[reveal] explorer /select failed: {e}");
    }
}

#[cfg(target_os = "macos")]
fn platform_reveal_file(path: &str) {
    if let Err(e) = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn()
    {
        crate::logger::log_info!("[reveal] open -R failed: {e}");
    }
}

#[cfg(target_os = "linux")]
fn platform_reveal_file(path: &str) {
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
        let dir = std::path::Path::new(path)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string());
        platform_open_dir(&dir);
    }
}

// ---------------------------------------------------------------------------
// 平台默认：打开目录（不选中）
// ---------------------------------------------------------------------------
//
// Windows: 用 `cmd /c start "" "dir"` 走 ShellExecute 关联，尊重用户在
// `HKCR\Folder\shell\open\command` 注册的默认 FM；直接 explorer.exe <dir>
// 会强制使用 Explorer。
// macOS: open <dir> 走 LaunchServices，尊重 `public.folder` 默认 handler。
// Linux: xdg-open 走 mimeapps.list 的 inode/directory 默认。

#[cfg(target_os = "windows")]
fn platform_open_dir(dir: &str) {
    use std::os::windows::process::CommandExt;
    // start 的第一个引号串是窗口标题，必须保留为空，否则 cmd 会把目录路径
    // 当成标题而打开新 cmd 窗口。
    let arg = format!(r#"/c start "" "{}""#, dir);
    if let Err(e) = std::process::Command::new("cmd.exe").raw_arg(&arg).spawn() {
        crate::logger::log_info!("[reveal] cmd /c start failed: {e}");
    }
}

#[cfg(target_os = "macos")]
fn platform_open_dir(dir: &str) {
    if let Err(e) = std::process::Command::new("open").arg(dir).spawn() {
        crate::logger::log_info!("[reveal] open dir failed: {e}");
    }
}

#[cfg(target_os = "linux")]
fn platform_open_dir(dir: &str) {
    if let Err(e) = std::process::Command::new("xdg-open").arg(dir).spawn() {
        crate::logger::log_info!("[reveal] xdg-open failed: {e}");
    }
}

#[cfg(target_os = "linux")]
fn path_to_file_uri(path: &str) -> String {
    let encoded: String = path
        .chars()
        .flat_map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.' | '~') {
                vec![c]
            } else {
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
