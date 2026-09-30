//! 数据目录 / 凭据文件的私有权限收紧。
//!
//! Unix 用 0700/0600；Windows 无安全的 std 封装，改用 `icacls` 子进程把 ACL 收紧为仅当前用户。
//! 默认 `%LOCALAPPDATA%` 已是用户私有，便携模式（exe 同级目录）与自定义数据目录则继承父目录 ACL。

use std::path::Path;

/// 启动时收紧数据目录：其后在目录内创建的 DB、token、blob 目录自动继承。
pub async fn secure_data_dir(path: &Path) -> Result<(), std::io::Error> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).await
    }
    #[cfg(not(unix))]
    {
        restrict_to_current_user(path, true).await
    }
}

/// 仅当前用户可访问 `path`；`inheritable` 为真时对目录内新建项可继承。非 Windows 为空操作。
#[cfg(windows)]
pub async fn restrict_to_current_user(
    path: &Path,
    inheritable: bool,
) -> Result<(), std::io::Error> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let sid = current_user_sid().await?;
    let grant = if inheritable {
        format!("*{sid}:(OI)(CI)(F)")
    } else {
        format!("*{sid}:(F)")
    };
    let path = path.to_owned();
    let status = tokio::task::spawn_blocking(move || {
        std::process::Command::new("icacls")
            .arg(path)
            .args(["/inheritance:r", "/grant:r", &grant])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(std::process::Stdio::null())
            .status()
    })
    .await
    .map_err(std::io::Error::other)??;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("icacls failed"))
    }
}

#[cfg(not(windows))]
pub async fn restrict_to_current_user(
    _path: &Path,
    _inheritable: bool,
) -> Result<(), std::io::Error> {
    Ok(())
}

#[cfg(windows)]
async fn current_user_sid() -> Result<String, std::io::Error> {
    use std::os::windows::process::CommandExt;
    static SID: std::sync::OnceLock<String> = std::sync::OnceLock::new();

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    if let Some(sid) = SID.get() {
        return Ok(sid.clone());
    }
    let output = tokio::task::spawn_blocking(|| {
        std::process::Command::new("whoami")
            .args(["/user", "/fo", "csv", "/nh"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
    })
    .await
    .map_err(std::io::Error::other)??;
    if !output.status.success() {
        return Err(std::io::Error::other("whoami /user failed"));
    }
    let text = String::from_utf8(output.stdout).map_err(std::io::Error::other)?;
    let sid = parse_whoami_sid(&text)
        .ok_or_else(|| std::io::Error::other("could not parse current SID"))?;
    Ok(SID.get_or_init(|| sid).clone())
}

#[cfg(any(windows, test))]
fn parse_whoami_sid(text: &str) -> Option<String> {
    text.split(',')
        .nth(1)
        .map(|value| value.trim().trim_matches('"'))
        .filter(|value| value.starts_with("S-1-"))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::parse_whoami_sid;

    #[test]
    fn parses_sid_from_whoami_csv() {
        assert_eq!(
            parse_whoami_sid("\"pc\\user\",\"S-1-5-21-1-2-3-1001\"\r\n").as_deref(),
            Some("S-1-5-21-1-2-3-1001")
        );
        assert_eq!(parse_whoami_sid("garbage"), None);
    }
}
