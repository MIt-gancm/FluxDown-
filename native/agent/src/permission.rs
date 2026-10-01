//! Doctor 权限修复的执行层。
//!
//! 原则：**提权只做「释放」**——把被 root / 管理员占住的路径还给当前用户（改回属主，或给
//! 当前用户加一条可继承的 ACL），真正的修复（重写文件、注册）回到普通权限完成。修复后的
//! 状态全部属于用户，之后的自动注册、下载都不再需要提权。提权进程绝不创建 FluxDown 的
//! 文件或注册项，否则又会留下 root / 管理员所有的产物。
//!
//! - Linux：`pkexec`（polkit 认证对话框）；macOS：`osascript … with administrator privileges`；
//!   Windows：PowerShell `Start-Process -Verb RunAs`（UAC）。
//! - 命令是固定模板，路径只作为独立参数传入，不拼进 shell / PowerShell 源码。
//! - 只在用户点「修复」时调用；headless（`--server`）宿主不提权。

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 等用户在授权对话框里操作的上限；超时视为失败并结束提权进程。
const ELEVATION_TIMEOUT: Duration = Duration::from_secs(300);
/// 查询当前用户身份（`id` / `whoami`）的上限。
const IDENTITY_TIMEOUT: Duration = Duration::from_secs(10);

/// `chown` 释放脚本：`$1` = chown 程序，`$2` = 属主，其后成对出现 `R|N <路径>`
/// （`R` 递归、`N` 只改该路径）。路径经 argv 传入，脚本源码固定。规划后到授权前，同一用户的
/// 其它进程可能把目录换成指向系统文件的符号链接：遇到符号链接直接失败，且一律 `-h`（GNU /
/// BSD 都支持）不解引用最后一级路径，`-R` 默认 `-P` 也不跟随子项链接。
pub(crate) const CHOWN_SCRIPT: &str = "chown_bin=$1; owner=$2; shift 2; \
while [ $# -ge 2 ]; do \
if [ -L \"$2\" ]; then echo \"refusing symbolic link: $2\" >&2; exit 1; fi; \
if [ \"$1\" = R ]; then \"$chown_bin\" -R -h -- \"$owner\" \"$2\" || exit 1; \
else \"$chown_bin\" -h -- \"$owner\" \"$2\" || exit 1; fi; \
shift 2; done";

/// macOS：把 argv 逐个 `quoted form of` 后以管理员身份执行（`administrator` 为 false 时
/// 不提权，供测试验证引用规则）。
fn applescript_lines(administrator: bool) -> [String; 7] {
    [
        "on run argv".to_owned(),
        "set cmd to \"\"".to_owned(),
        "repeat with a in argv".to_owned(),
        "set cmd to cmd & quoted form of (a as text) & \" \"".to_owned(),
        "end repeat".to_owned(),
        if administrator {
            "do shell script cmd with administrator privileges".to_owned()
        } else {
            "do shell script cmd".to_owned()
        },
        "end run".to_owned(),
    ]
}

/// Windows：在普通权限的 PowerShell 里以 UAC（`ShellExecute` 的 `runas` 动词）拉起提权进程并
/// 等待其退出码。程序与参数行经环境变量传入（不进脚本源码）。直接用 .NET `Process.Start`
/// 而不是 `Start-Process`：后者把 `Win32Exception` 改包成不带内层异常的
/// `InvalidOperationException`，识别不出用户取消；`Process.Start` 抛出的
/// `MethodInvocationException` 内层保留 `Win32Exception(1223 = ERROR_CANCELLED)`。
const POWERSHELL_SCRIPT: &str = "$ErrorActionPreference = 'Stop'; \
try { $si = New-Object System.Diagnostics.ProcessStartInfo; \
$si.FileName = $env:FLUXDOWN_ELEVATE_PROGRAM; $si.Arguments = $env:FLUXDOWN_ELEVATE_ARGS; \
$si.Verb = 'runas'; $si.UseShellExecute = $true; \
$si.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden; \
$p = [System.Diagnostics.Process]::Start($si); if ($null -eq $p) { exit 1 }; \
$p.WaitForExit(); exit $p.ExitCode } \
catch { $e = $_.Exception; while ($e) { \
if ($e -is [System.ComponentModel.Win32Exception] -and $e.NativeErrorCode -eq 1223) { exit 1223 }; \
$e = $e.InnerException }; exit 1 }";
const POWERSHELL_PROGRAM_ENV: &str = "FLUXDOWN_ELEVATE_PROGRAM";
const POWERSHELL_ARGS_ENV: &str = "FLUXDOWN_ELEVATE_ARGS";
/// Windows `ERROR_CANCELLED`：用户在 UAC 对话框点了「否」。
const WINDOWS_ERROR_CANCELLED: i32 = 1223;

/// 权限修复失败的原因。
#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    /// 用户取消了授权对话框，没有做任何更改。
    #[error("administrator authorization was cancelled")]
    Cancelled,
    /// 无法请求管理员授权（缺 `pkexec` / polkit 认证代理、无桌面会话等）。
    #[error("cannot request administrator authorization: {0}")]
    ElevationUnavailable(String),
    /// FluxDown 自身以 root 运行：「还给当前用户」没有意义。
    #[error("FluxDown is running as root")]
    RunningElevated,
    /// 目标不在自动修复范围内（系统目录、家目录本身、非权限类错误）。
    #[error("not repairable automatically: {0}")]
    NotApplicable(String),
    #[error("permission repair failed: {0}")]
    Failed(String),
    /// 命令在时限内没有结束（授权对话框无人处理、大目录传播 ACL）；已被结束。
    #[error("permission repair timed out: {0}")]
    TimedOut(String),
}

/// 修复动作适用的平台规则（测试可构造任一平台）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Linux,
    MacOs,
    Windows,
}

impl Os {
    pub const CURRENT: Self = if cfg!(windows) {
        Self::Windows
    } else if cfg!(target_os = "macos") {
        Self::MacOs
    } else {
        Self::Linux
    };
}

/// 一次需要管理员授权的释放操作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseOp {
    /// 类 Unix：把路径的属主改回 `uid`；`(路径, 是否递归)`，递归只用于 FluxDown 自有目录。
    Chown {
        uid: u32,
        entries: Vec<(PathBuf, bool)>,
    },
    /// 给当前用户（Linux uid / macOS 用户名 / Windows `*SID`）加一条可继承的读写权限，
    /// 不改属主——用于不属于当前用户的共享目录。
    Grant { principal: String, dir: PathBuf },
}

/// 一条外部命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub env: Vec<(&'static str, OsString)>,
}

impl Invocation {
    fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
        }
    }

    fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }
}

/// 释放操作需要的外部程序（运行期按平台定位，测试直接构造）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tools {
    pub sh: Option<PathBuf>,
    pub chown: Option<PathBuf>,
    pub setfacl: Option<PathBuf>,
    pub chmod: Option<PathBuf>,
    pub icacls: Option<PathBuf>,
    pub pkexec: Option<PathBuf>,
    pub osascript: Option<PathBuf>,
    pub powershell: Option<PathBuf>,
}

impl Tools {
    /// 按平台的固定安装位置定位（不经 `PATH`，避免被同名程序劫持）。
    #[must_use]
    pub fn locate(os: Os) -> Self {
        let find = |candidates: &[&str]| {
            candidates
                .iter()
                .map(PathBuf::from)
                .find(|candidate| candidate.is_file())
        };
        match os {
            Os::Linux => Self {
                sh: find(&["/bin/sh", "/usr/bin/sh"]),
                chown: find(&["/usr/bin/chown", "/bin/chown"]),
                setfacl: find(&["/usr/bin/setfacl", "/bin/setfacl"]),
                pkexec: find(&["/usr/bin/pkexec", "/bin/pkexec"]),
                ..Self::default()
            },
            Os::MacOs => Self {
                sh: find(&["/bin/sh"]),
                chown: find(&["/usr/sbin/chown"]),
                chmod: find(&["/bin/chmod"]),
                osascript: find(&["/usr/bin/osascript"]),
                ..Self::default()
            },
            Os::Windows => {
                let system32 = std::env::var_os("SystemRoot")
                    .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from)
                    .join("System32");
                let existing = |path: PathBuf| path.is_file().then_some(path);
                Self {
                    icacls: existing(system32.join("icacls.exe")),
                    powershell: existing(
                        system32
                            .join("WindowsPowerShell")
                            .join("v1.0")
                            .join("powershell.exe"),
                    ),
                    ..Self::default()
                }
            }
        }
    }
}

fn missing(tool: &str) -> PermissionError {
    PermissionError::ElevationUnavailable(format!("{tool} is not installed"))
}

/// 释放操作本身的命令（尚未提权）。
pub fn release_invocation(
    op: &ReleaseOp,
    os: Os,
    tools: &Tools,
) -> Result<Invocation, PermissionError> {
    match (op, os) {
        (ReleaseOp::Chown { uid, entries }, Os::Linux | Os::MacOs) => {
            let sh = tools.sh.clone().ok_or_else(|| missing("sh"))?;
            let chown = tools.chown.clone().ok_or_else(|| missing("chown"))?;
            let mut invocation = Invocation::new(sh)
                .arg("-c")
                .arg(CHOWN_SCRIPT)
                .arg("fluxdown-release")
                .arg(chown)
                .arg(uid.to_string());
            for (path, recursive) in entries {
                invocation = invocation
                    .arg(if *recursive { "R" } else { "N" })
                    .arg(path.as_os_str());
            }
            Ok(invocation)
        }
        (ReleaseOp::Grant { principal, dir }, Os::Linux) => {
            let setfacl = tools.setfacl.clone().ok_or_else(|| missing("setfacl"))?;
            // `-P`：不跟随符号链接，符号链接参数直接跳过（随后的重新探测会如实报告未修复）。
            Ok(Invocation::new(setfacl)
                .arg("-P")
                .arg("-m")
                .arg(format!("u:{principal}:rwx,d:u:{principal}:rwx"))
                .arg("--")
                .arg(dir.as_os_str()))
        }
        (ReleaseOp::Grant { principal, dir }, Os::MacOs) => {
            let chmod = tools.chmod.clone().ok_or_else(|| missing("chmod"))?;
            // `-h`：路径若被换成符号链接，只改链接本身。
            Ok(Invocation::new(chmod)
                .arg("-h")
                .arg("+a")
                .arg(format!(
                    "user:{principal} allow list,add_file,search,add_subdirectory,delete_child,\
                     readattr,writeattr,readextattr,writeextattr,readsecurity,\
                     file_inherit,directory_inherit"
                ))
                .arg(dir.as_os_str()))
        }
        (ReleaseOp::Grant { principal, dir }, Os::Windows) => {
            let icacls = tools.icacls.clone().ok_or_else(|| missing("icacls"))?;
            // `/L`：路径若被换成符号链接 / 联接点，只作用于链接本身。
            Ok(Invocation::new(icacls)
                .arg(dir.as_os_str())
                .arg("/grant")
                .arg(format!("{principal}:(OI)(CI)M"))
                .arg("/L"))
        }
        (ReleaseOp::Chown { .. }, Os::Windows) => Err(PermissionError::NotApplicable(
            "ownership is not changed on Windows; access is granted instead".to_owned(),
        )),
    }
}

/// 套上平台的提权外壳。
pub fn elevated_invocation(
    tool: Invocation,
    os: Os,
    tools: &Tools,
) -> Result<Invocation, PermissionError> {
    match os {
        Os::Linux => {
            let pkexec = tools.pkexec.clone().ok_or_else(|| missing("pkexec"))?;
            let mut invocation = Invocation::new(pkexec).arg(tool.program.into_os_string());
            invocation.args.extend(tool.args);
            Ok(invocation)
        }
        Os::MacOs => {
            let osascript = tools
                .osascript
                .clone()
                .ok_or_else(|| missing("osascript"))?;
            let mut invocation = Invocation::new(osascript);
            for line in applescript_lines(true) {
                invocation = invocation.arg("-e").arg(line);
            }
            invocation = invocation.arg(tool.program.into_os_string());
            invocation.args.extend(tool.args);
            Ok(invocation)
        }
        Os::Windows => {
            let powershell = tools
                .powershell
                .clone()
                .ok_or_else(|| missing("powershell"))?;
            let command_line = windows_command_line(&tool.args);
            let mut invocation = Invocation::new(powershell)
                .arg("-NoProfile")
                .arg("-NonInteractive")
                .arg("-ExecutionPolicy")
                .arg("Bypass")
                .arg("-Command")
                .arg(POWERSHELL_SCRIPT);
            invocation.env = vec![
                (POWERSHELL_PROGRAM_ENV, tool.program.into_os_string()),
                (POWERSHELL_ARGS_ENV, OsString::from(command_line)),
            ];
            Ok(invocation)
        }
    }
}

/// 按 `CommandLineToArgvW` 规则拼 Windows 命令行（`Start-Process -ArgumentList` 原样使用）：
/// 含空白或引号的参数加引号，引号前与结尾的反斜杠成倍转义。
#[must_use]
pub fn windows_command_line(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| quote_windows_arg(&arg.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn quote_windows_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '\u{b}', '"']) {
        return arg.to_owned();
    }
    let mut quoted = String::with_capacity(arg.len() + 2);
    quoted.push('"');
    let mut backslashes = 0_usize;
    for ch in arg.chars() {
        match ch {
            '\\' => backslashes += 1,
            '"' => {
                quoted.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            }
            other => {
                quoted.extend(std::iter::repeat_n('\\', backslashes));
                quoted.push(other);
                backslashes = 0;
            }
        }
    }
    quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
    quoted.push('"');
    quoted
}

/// 提权外壳的退出状态 → 结果。`code` 为 `None` 表示被信号结束。
pub fn classify_elevation_exit(
    os: Os,
    code: Option<i32>,
    stderr: &str,
) -> Result<(), PermissionError> {
    let stderr = stderr.trim();
    let failed = || {
        PermissionError::Failed(match code {
            Some(code) if stderr.is_empty() => format!("exit code {code}"),
            Some(code) => format!("exit code {code}: {stderr}"),
            None => "terminated by a signal".to_owned(),
        })
    };
    match (os, code) {
        (_, Some(0)) => Ok(()),
        // pkexec(1)：126 = 用户关闭了认证对话框；127 = 未获授权 / 没有认证代理。
        (Os::Linux, Some(126)) => Err(PermissionError::Cancelled),
        (Os::Linux, Some(127)) => Err(PermissionError::ElevationUnavailable(
            if stderr.is_empty() {
                "pkexec could not obtain authorization".to_owned()
            } else {
                stderr.to_owned()
            },
        )),
        // AppleScript 错误 -128 = 用户点了「取消」。
        (Os::MacOs, Some(_)) if stderr.contains("(-128)") => Err(PermissionError::Cancelled),
        (Os::Windows, Some(WINDOWS_ERROR_CANCELLED)) => Err(PermissionError::Cancelled),
        _ => Err(failed()),
    }
}

/// 以普通权限运行命令（Windows 属主可直接改 DACL 时无需 UAC）；成功返回 `Ok`。
pub async fn run_as_user(invocation: Invocation, timeout: Duration) -> Result<(), PermissionError> {
    let output = run(invocation, timeout).await?;
    if output.status.success() {
        Ok(())
    } else {
        Err(PermissionError::Failed(format!(
            "{}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

/// 弹出系统授权对话框执行释放操作；用户取消、无法授权与命令失败分别返回。
pub async fn run_elevated(op: &ReleaseOp) -> Result<(), PermissionError> {
    let tools = Tools::locate(Os::CURRENT);
    let tool = release_invocation(op, Os::CURRENT, &tools)?;
    let invocation = elevated_invocation(tool, Os::CURRENT, &tools)?;
    tracing::info!(op = ?op, "requesting administrator authorization for permission repair");
    let output = run(invocation, ELEVATION_TIMEOUT).await?;
    let result = classify_elevation_exit(
        Os::CURRENT,
        output.status.code(),
        &String::from_utf8_lossy(&output.stderr),
    );
    match &result {
        Ok(()) => tracing::info!("permission repair authorized and applied"),
        Err(error) => tracing::warn!(error = %error, "elevated permission repair did not complete"),
    }
    result
}

/// 释放：Windows 先以普通权限加 ACL（目录属主本就能改 DACL），被拒再走 UAC；超时（大目录传播
/// 继承项）不改走 UAC，直接报告——ACL 可能已部分写入，再提权只会重复同一操作。类 Unix 直接提权。
pub async fn release(op: &ReleaseOp) -> Result<(), PermissionError> {
    if Os::CURRENT == Os::Windows {
        let tools = Tools::locate(Os::CURRENT);
        if let Ok(tool) = release_invocation(op, Os::CURRENT, &tools) {
            match run_as_user(tool, ELEVATION_TIMEOUT).await {
                Ok(()) => return Ok(()),
                Err(error @ PermissionError::TimedOut(_)) => return Err(error),
                Err(error) => {
                    tracing::info!(error = %error, "granting access without elevation failed; requesting UAC");
                }
            }
        }
    }
    run_elevated(op).await
}

async fn run(
    invocation: Invocation,
    timeout: Duration,
) -> Result<std::process::Output, PermissionError> {
    let mut command = tokio::process::Command::new(&invocation.program);
    command
        .args(&invocation.args)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    for (key, value) in &invocation.env {
        command.env(key, value);
    }
    #[cfg(windows)]
    {
        /// `CREATE_NO_WINDOW`：PowerShell / icacls 是控制台程序，不闪黑窗。
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    match tokio::time::timeout(timeout, command.output()).await {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(PermissionError::ElevationUnavailable(format!(
                "{}: {error}",
                invocation.program.display()
            )))
        }
        Ok(Err(error)) => Err(PermissionError::Failed(format!(
            "{}: {error}",
            invocation.program.display()
        ))),
        Err(_) => Err(PermissionError::TimedOut(format!(
            "{}: no result within {}s",
            invocation.program.display(),
            timeout.as_secs()
        ))),
    }
}

// ─────────────────────────── 当前用户身份 ───────────────────────────

/// 本进程的有效 uid：新建文件的属主即有效 uid（std 不提供 `geteuid`，不为此引入 FFI）。
#[cfg(unix)]
pub fn effective_uid() -> std::io::Result<u32> {
    use std::os::unix::fs::MetadataExt;
    static UID: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    if let Some(uid) = UID.get() {
        return Ok(*uid);
    }
    let probe = std::env::temp_dir().join(format!(
        ".fluxdown-uid-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)?;
    let uid = file.metadata().map(|metadata| metadata.uid());
    drop(file);
    remove_file_quietly(&probe);
    let uid = uid?;
    Ok(*UID.get_or_init(|| uid))
}

/// 尽力删除临时文件：文件已不存在不算失败，其余失败只记日志（调用方的结论不受影响）。
#[cfg(unix)]
pub(crate) fn remove_file_quietly(path: &Path) {
    if let Err(error) = std::fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(path = %path.display(), error = %error, "could not remove temporary file");
    }
}

/// 路径的属主 uid；读不到元数据时为 `None`。
#[cfg(unix)]
#[must_use]
pub fn owner_uid(path: &Path) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(path)
        .ok()
        .map(|metadata| metadata.uid())
}

/// ACL 授权对象：Linux 用数字 uid，macOS 用用户名（`chmod +a` 只认名字），Windows 用 `*SID`。
pub async fn current_principal() -> Result<String, PermissionError> {
    match Os::CURRENT {
        Os::Linux => {
            #[cfg(unix)]
            {
                effective_uid()
                    .map(|uid| uid.to_string())
                    .map_err(|error| PermissionError::Failed(format!("current uid: {error}")))
            }
            #[cfg(not(unix))]
            {
                Err(PermissionError::NotApplicable("uid".to_owned()))
            }
        }
        Os::MacOs => {
            let output = run(Invocation::new("/usr/bin/id").arg("-un"), IDENTITY_TIMEOUT).await?;
            let name = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if output.status.success() && !name.is_empty() {
                Ok(name)
            } else {
                Err(PermissionError::Failed(
                    "could not determine the user name".to_owned(),
                ))
            }
        }
        Os::Windows => {
            let system32 = std::env::var_os("SystemRoot")
                .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from)
                .join("System32");
            let output = run(
                Invocation::new(system32.join("whoami.exe"))
                    .arg("/user")
                    .arg("/fo")
                    .arg("csv")
                    .arg("/nh"),
                IDENTITY_TIMEOUT,
            )
            .await?;
            parse_whoami_sid(&String::from_utf8_lossy(&output.stdout))
                .map(|sid| format!("*{sid}"))
                .ok_or_else(|| {
                    PermissionError::Failed("could not determine the user SID".to_owned())
                })
        }
    }
}

/// `whoami /user /fo csv /nh` 输出 `"域\\用户","S-1-5-21-…"`，取 SID。
#[must_use]
pub fn parse_whoami_sid(output: &str) -> Option<String> {
    output
        .lines()
        .filter_map(|line| line.trim().rsplit_once(','))
        .map(|(_, sid)| sid.trim().trim_matches('"').to_owned())
        .find(|sid| sid.starts_with("S-1-"))
}

// ─────────────────────────── 目录修复规划 ───────────────────────────

/// 目录相对当前用户的归属范围，决定释放方式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirScope {
    /// FluxDown 自有目录（数据目录等）之内：递归改回属主。
    Owned,
    /// 家目录之内：只改该目录本身的属主（不递归进用户的下载内容）。
    Home,
    /// 家目录之外的共享目录：不抢属主，只给当前用户加 ACL。
    Shared,
    /// 根目录、家目录本身及其上级、系统目录：不自动修改。
    Refused,
}

/// 只读的分类依据。路径应已规范化（解析符号链接与 `..`）。
#[derive(Debug, Clone, Default)]
pub struct ScopeRules {
    pub home: Option<PathBuf>,
    /// FluxDown 自有目录（daemon / agent 数据目录、NMH 目录）。
    pub owned_roots: Vec<PathBuf>,
    /// 整棵子树都拒绝的系统目录。
    pub refused_trees: Vec<PathBuf>,
    /// `refused_trees` 中仍允许的子树（如 Linux `/run/media` 下的可移动磁盘）。
    pub allowed_trees: Vec<PathBuf>,
    /// 只拒绝目录本身（如 `Program Files` 根），其子目录允许。
    pub refused_exact: Vec<PathBuf>,
    /// 其直接子目录也是挂载容器（udisks 的 `/media/<user>`、`/run/media/<user>`）：子目录本身
    /// 拒绝，更深的卷目录按其它规则判定。
    pub container_parents: Vec<PathBuf>,
    /// Windows 路径不区分大小写。
    pub case_insensitive: bool,
}

impl ScopeRules {
    /// 当前平台的系统目录规则。
    #[must_use]
    pub fn for_current_os(home: Option<PathBuf>, owned_roots: Vec<PathBuf>) -> Self {
        let paths = |items: &[&str]| items.iter().map(PathBuf::from).collect::<Vec<_>>();
        let (refused_trees, allowed_trees, refused_exact, container_parents) = match Os::CURRENT {
            Os::Linux => (
                paths(&[
                    "/bin",
                    "/boot",
                    "/dev",
                    "/etc",
                    "/lib",
                    "/lib32",
                    "/lib64",
                    "/libx32",
                    "/lost+found",
                    "/nix",
                    "/opt",
                    "/proc",
                    "/root",
                    "/run",
                    "/sbin",
                    "/snap",
                    "/sys",
                    "/usr",
                    "/var",
                ]),
                // ostree（Fedora Atomic）上 `/mnt`、`/srv` 是指向 `/var/mnt`、`/var/srv` 的链接。
                paths(&["/run/media", "/var/mnt", "/var/srv"]),
                // 挂载容器本身不改，其中的卷（`/mnt/data`、`/media/<user>/<卷>`）按共享目录处理。
                paths(&[
                    "/mnt",
                    "/media",
                    "/srv",
                    "/run/media",
                    "/var/mnt",
                    "/var/srv",
                ]),
                paths(&["/media", "/run/media"]),
            ),
            Os::MacOs => (
                paths(&[
                    "/System",
                    "/Library",
                    "/Applications",
                    "/bin",
                    "/sbin",
                    "/usr",
                    "/etc",
                    "/private",
                    "/dev",
                    "/cores",
                    "/opt",
                    "/nix",
                ]),
                Vec::new(),
                paths(&["/Volumes"]),
                Vec::new(),
            ),
            Os::Windows => {
                let env = |key: &str| std::env::var_os(key).map(PathBuf::from);
                (
                    env("SystemRoot").into_iter().collect(),
                    Vec::new(),
                    ["ProgramFiles", "ProgramFiles(x86)", "ProgramData"]
                        .iter()
                        .filter_map(|key| env(key))
                        .collect(),
                    Vec::new(),
                )
            }
        };
        Self {
            home,
            owned_roots,
            refused_trees,
            allowed_trees,
            refused_exact,
            container_parents,
            case_insensitive: Os::CURRENT == Os::Windows,
        }
    }

    fn key(&self, path: &Path) -> PathBuf {
        if self.case_insensitive {
            PathBuf::from(path.to_string_lossy().to_lowercase())
        } else {
            path.to_path_buf()
        }
    }

    /// 目录的归属范围。自有目录本身若是根目录、家目录的上级或系统目录（配置错误的数据目录），
    /// 不按自有目录处理，避免递归改动整棵系统目录。家目录内的目录先于系统目录规则判定：
    /// 部分发行版把家目录放在系统树下（Fedora Atomic 的 `/var/home`）。
    #[must_use]
    pub fn classify(&self, dir: &Path) -> DirScope {
        let dir = self.key(dir);
        let home = self.home.as_deref().map(|home| self.key(home));
        let keys = |paths: &[PathBuf]| paths.iter().map(|path| self.key(path)).collect::<Vec<_>>();
        let (refused_trees, allowed_trees, refused_exact, container_parents) = (
            keys(&self.refused_trees),
            keys(&self.allowed_trees),
            keys(&self.refused_exact),
            keys(&self.container_parents),
        );
        let refused = |path: &Path| {
            path.parent().is_none_or(|parent| {
                container_parents
                    .iter()
                    .any(|container| container.as_path() == parent)
            }) || home.as_deref().is_some_and(|home| home.starts_with(path))
                || refused_exact.iter().any(|exact| exact.starts_with(path))
                || refused_trees.iter().any(|tree| tree.starts_with(path))
        };
        let in_refused_tree = refused_trees.iter().any(|tree| dir.starts_with(tree))
            && !allowed_trees.iter().any(|tree| dir.starts_with(tree));
        if refused(&dir) {
            return DirScope::Refused;
        }
        let owned = keys(&self.owned_roots)
            .into_iter()
            .filter(|root| !refused(root))
            .any(|root| dir.starts_with(&root));
        if owned {
            return DirScope::Owned;
        }
        if home.as_deref().is_some_and(|home| dir.starts_with(home)) {
            return DirScope::Home;
        }
        if in_refused_tree {
            DirScope::Refused
        } else {
            DirScope::Shared
        }
    }
}

/// 目录写入被拒时的修复方式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirPlan {
    /// 目录属于当前用户，只是缺少属主权限位：普通权限补上 `u+rwx`。
    ChmodOwner(PathBuf),
    /// 需要管理员授权的释放操作。
    Release(ReleaseOp),
    /// macOS 隐私授权（TCC）拒绝：只能由用户在系统设置里放行。
    PrivacySettings,
    /// 不在自动修复范围内。
    NotApplicable(&'static str),
}

/// 规划所需的事实。
#[derive(Debug, Clone)]
pub struct DirFacts {
    pub os: Os,
    /// 探测失败时的系统错误码。
    pub os_error: Option<i32>,
    /// 规范化后的探测目录。
    pub dir: PathBuf,
    pub scope: DirScope,
    /// 类 Unix：目录属主与本进程有效 uid。
    pub owner: Option<u32>,
    pub self_uid: Option<u32>,
    /// ACL 授权对象（仅 `Shared` / Windows 需要）。
    pub principal: Option<String>,
}

/// `EACCES`：权限位 / ACL 不允许（改属主或加 ACL 可解）。
pub const EACCES: i32 = 13;
/// `EPERM`：macOS 上是隐私授权 / SIP 拒绝，Linux 上多为不可变属性等，改属主无效。
pub const EPERM: i32 = 1;
/// Windows `ERROR_ACCESS_DENIED`。
pub const ERROR_ACCESS_DENIED: i32 = 5;

/// 按「先普通权限、再释放、拒绝越界」规划一次目录修复。
#[must_use]
pub fn plan_dir_repair(facts: &DirFacts) -> DirPlan {
    if facts.scope == DirScope::Refused {
        return DirPlan::NotApplicable("system or home directory");
    }
    match facts.os {
        Os::Windows => {
            if facts.os_error != Some(ERROR_ACCESS_DENIED) {
                return DirPlan::NotApplicable("not an access-denied error");
            }
            match &facts.principal {
                Some(principal) => DirPlan::Release(ReleaseOp::Grant {
                    principal: principal.clone(),
                    dir: facts.dir.clone(),
                }),
                None => DirPlan::NotApplicable("current user unknown"),
            }
        }
        Os::MacOs if facts.os_error == Some(EPERM) => DirPlan::PrivacySettings,
        Os::Linux | Os::MacOs => {
            if facts.os_error != Some(EACCES) {
                return DirPlan::NotApplicable("not a permission-bits error");
            }
            let Some(uid) = facts.self_uid else {
                return DirPlan::NotApplicable("current user unknown");
            };
            if facts.owner == Some(uid) {
                return DirPlan::ChmodOwner(facts.dir.clone());
            }
            match facts.scope {
                DirScope::Owned => DirPlan::Release(ReleaseOp::Chown {
                    uid,
                    entries: vec![(facts.dir.clone(), true)],
                }),
                DirScope::Home => DirPlan::Release(ReleaseOp::Chown {
                    uid,
                    entries: vec![(facts.dir.clone(), false)],
                }),
                DirScope::Shared => match &facts.principal {
                    Some(principal) => DirPlan::Release(ReleaseOp::Grant {
                        principal: principal.clone(),
                        dir: facts.dir.clone(),
                    }),
                    None => DirPlan::NotApplicable("current user unknown"),
                },
                DirScope::Refused => DirPlan::NotApplicable("system or home directory"),
            }
        }
    }
}

/// 属于当前用户但缺少属主权限位的目录：补上 `u+rwx`（不改组 / 其他人的位）。
#[cfg(unix)]
pub fn chmod_owner_rwx(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(dir)?.permissions().mode() & 0o7777;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode | 0o700))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    use super::{
        CHOWN_SCRIPT, DirFacts, DirPlan, DirScope, Invocation, Os, PermissionError, ReleaseOp,
        ScopeRules, Tools, classify_elevation_exit, elevated_invocation, parse_whoami_sid,
        plan_dir_repair, release_invocation, windows_command_line,
    };

    fn rules() -> ScopeRules {
        ScopeRules {
            home: Some(PathBuf::from("/home/zero")),
            owned_roots: vec![PathBuf::from("/home/zero/.local/share/FluxDown")],
            refused_trees: vec![PathBuf::from("/usr"), PathBuf::from("/run")],
            allowed_trees: vec![PathBuf::from("/run/media")],
            refused_exact: vec![PathBuf::from("/opt/apps"), PathBuf::from("/media")],
            container_parents: vec![PathBuf::from("/media")],
            case_insensitive: false,
        }
    }

    #[test]
    fn scope_separates_owned_home_shared_and_refused_dirs() {
        let rules = rules();
        let scope = |dir: &str| rules.classify(Path::new(dir));
        assert_eq!(scope("/home/zero/.local/share/FluxDown"), DirScope::Owned);
        assert_eq!(
            scope("/home/zero/.local/share/FluxDown/bt"),
            DirScope::Owned
        );
        assert_eq!(scope("/home/zero/Downloads"), DirScope::Home);
        assert_eq!(scope("/srv/downloads"), DirScope::Shared);
        assert_eq!(scope("/run/media/zero/usb/dl"), DirScope::Shared);
        assert_eq!(scope("/opt/apps/dl"), DirScope::Shared);
        assert_eq!(scope("/media/zero/usb"), DirScope::Shared);
        for refused in [
            "/",
            "/home",
            "/home/zero",
            "/usr/local/dl",
            "/run/user/1000",
            "/opt/apps",
            "/media",
            "/media/zero",
        ] {
            assert_eq!(scope(refused), DirScope::Refused, "{refused}");
        }
    }

    #[test]
    fn misconfigured_owned_roots_never_widen_recursive_scope() {
        let mut rules = rules();
        rules.owned_roots = vec![
            PathBuf::from("/"),
            PathBuf::from("/home"),
            PathBuf::from("/usr"),
            PathBuf::from("/var/lib/fluxdown"),
        ];
        rules.refused_trees.push(PathBuf::from("/var"));
        let scope = |dir: &str| rules.classify(Path::new(dir));
        assert_eq!(scope("/etc/ssh"), DirScope::Shared);
        assert_eq!(scope("/usr/lib"), DirScope::Refused);
        assert_eq!(scope("/home/other"), DirScope::Shared);
        assert_eq!(scope("/var/lib/fluxdown/db"), DirScope::Owned);
        assert_eq!(scope("/var/log"), DirScope::Refused);
    }

    /// Fedora Atomic 等把家目录放在 `/var/home`：家目录内仍按家目录处理，家目录外的 `/var` 照旧拒绝。
    #[test]
    fn home_inside_a_system_tree_stays_repairable() {
        let rules = ScopeRules {
            home: Some(PathBuf::from("/var/home/zero")),
            refused_trees: vec![PathBuf::from("/var")],
            ..ScopeRules::default()
        };
        let scope = |dir: &str| rules.classify(Path::new(dir));
        assert_eq!(scope("/var/home/zero/Downloads"), DirScope::Home);
        assert_eq!(scope("/var/home/zero"), DirScope::Refused);
        assert_eq!(scope("/var/lib/data"), DirScope::Refused);
    }

    /// 大小写不敏感比较（Windows 规则）；用 `/` 分隔，保证在任意平台按组件解析。
    #[test]
    fn case_insensitive_scope_matches_regardless_of_case() {
        let rules = ScopeRules {
            home: Some(PathBuf::from("/Users/Zero")),
            owned_roots: vec![PathBuf::from("/Users/Zero/AppData/Local/FluxDown")],
            refused_trees: vec![PathBuf::from("/WINDOWS")],
            allowed_trees: Vec::new(),
            refused_exact: vec![PathBuf::from("/Program Files")],
            container_parents: Vec::new(),
            case_insensitive: true,
        };
        let scope = |dir: &str| rules.classify(Path::new(dir));
        assert_eq!(
            scope("/users/zero/appdata/local/fluxdown/nmh"),
            DirScope::Owned
        );
        assert_eq!(scope("/users/ZERO"), DirScope::Refused);
        assert_eq!(scope("/Windows/Temp"), DirScope::Refused);
        assert_eq!(scope("/program files"), DirScope::Refused);
        assert_eq!(scope("/Program Files/App/dl"), DirScope::Shared);
    }

    /// 软件安装树与挂载容器拒绝，挂载容器里的卷按共享目录处理。
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_rules_refuse_install_trees_and_mount_containers() {
        let rules = ScopeRules::for_current_os(Some(PathBuf::from("/home/zero")), Vec::new());
        let scope = |dir: &str| rules.classify(Path::new(dir));
        for refused in [
            "/opt/app/data",
            "/snap/core",
            "/nix/store/x",
            "/lost+found/1",
            "/mnt",
            "/media",
            "/media/zero",
            "/srv",
            "/run/media",
            "/run/media/zero",
            "/var/mnt",
            "/var/opt/app",
        ] {
            assert_eq!(scope(refused), DirScope::Refused, "{refused}");
        }
        for shared in [
            "/mnt/data",
            "/media/zero/usb",
            "/srv/dl",
            "/run/media/zero/usb",
            "/var/mnt/data",
            "/var/srv/dl",
        ] {
            assert_eq!(scope(shared), DirScope::Shared, "{shared}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_rules_refuse_install_trees_and_volume_root() {
        let rules = ScopeRules::for_current_os(Some(PathBuf::from("/Users/zero")), Vec::new());
        let scope = |dir: &str| rules.classify(Path::new(dir));
        for refused in ["/opt/homebrew/var", "/nix/store/x", "/Volumes"] {
            assert_eq!(scope(refused), DirScope::Refused, "{refused}");
        }
        assert_eq!(scope("/Volumes/Data/dl"), DirScope::Shared);
    }

    fn facts(os: Os, os_error: i32, scope: DirScope, owner: u32) -> DirFacts {
        DirFacts {
            os,
            os_error: Some(os_error),
            dir: PathBuf::from("/d"),
            scope,
            owner: Some(owner),
            self_uid: Some(1000),
            principal: Some("1000".to_owned()),
        }
    }

    #[test]
    fn unix_plan_restores_ownership_without_reaching_into_user_content() {
        assert_eq!(
            plan_dir_repair(&facts(Os::Linux, 13, DirScope::Home, 1000)),
            DirPlan::ChmodOwner(PathBuf::from("/d"))
        );
        assert_eq!(
            plan_dir_repair(&facts(Os::Linux, 13, DirScope::Owned, 0)),
            DirPlan::Release(ReleaseOp::Chown {
                uid: 1000,
                entries: vec![(PathBuf::from("/d"), true)]
            })
        );
        assert_eq!(
            plan_dir_repair(&facts(Os::Linux, 13, DirScope::Home, 0)),
            DirPlan::Release(ReleaseOp::Chown {
                uid: 1000,
                entries: vec![(PathBuf::from("/d"), false)]
            })
        );
        assert_eq!(
            plan_dir_repair(&facts(Os::Linux, 13, DirScope::Shared, 0)),
            DirPlan::Release(ReleaseOp::Grant {
                principal: "1000".to_owned(),
                dir: PathBuf::from("/d")
            })
        );
        assert!(matches!(
            plan_dir_repair(&facts(Os::Linux, 1, DirScope::Home, 0)),
            DirPlan::NotApplicable(_)
        ));
        assert!(matches!(
            plan_dir_repair(&facts(Os::Linux, 13, DirScope::Refused, 0)),
            DirPlan::NotApplicable(_)
        ));
        assert_eq!(
            plan_dir_repair(&facts(Os::MacOs, 1, DirScope::Home, 0)),
            DirPlan::PrivacySettings
        );
    }

    #[test]
    fn windows_plan_grants_access_only_for_access_denied() {
        let mut windows = facts(Os::Windows, 5, DirScope::Home, 0);
        windows.principal = Some("*S-1-5-21-1".to_owned());
        assert_eq!(
            plan_dir_repair(&windows),
            DirPlan::Release(ReleaseOp::Grant {
                principal: "*S-1-5-21-1".to_owned(),
                dir: PathBuf::from("/d")
            })
        );
        windows.os_error = Some(32);
        assert!(matches!(
            plan_dir_repair(&windows),
            DirPlan::NotApplicable(_)
        ));
    }

    fn tools() -> Tools {
        Tools {
            sh: Some(PathBuf::from("/bin/sh")),
            chown: Some(PathBuf::from("/usr/bin/chown")),
            setfacl: Some(PathBuf::from("/usr/bin/setfacl")),
            chmod: Some(PathBuf::from("/bin/chmod")),
            icacls: Some(PathBuf::from(r"C:\Windows\System32\icacls.exe")),
            pkexec: Some(PathBuf::from("/usr/bin/pkexec")),
            osascript: Some(PathBuf::from("/usr/bin/osascript")),
            powershell: Some(PathBuf::from("powershell.exe")),
        }
    }

    fn strings(invocation: &Invocation) -> Vec<String> {
        invocation
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn linux_release_runs_fixed_tools_under_pkexec_with_paths_as_arguments()
    -> Result<(), PermissionError> {
        let tools = tools();
        let grant = ReleaseOp::Grant {
            principal: "1000".to_owned(),
            dir: PathBuf::from("/srv/dl; rm -rf ~"),
        };
        let wrapped = elevated_invocation(
            release_invocation(&grant, Os::Linux, &tools)?,
            Os::Linux,
            &tools,
        )?;
        assert_eq!(wrapped.program, PathBuf::from("/usr/bin/pkexec"));
        assert_eq!(
            strings(&wrapped),
            [
                "/usr/bin/setfacl",
                "-P",
                "-m",
                "u:1000:rwx,d:u:1000:rwx",
                "--",
                "/srv/dl; rm -rf ~"
            ]
        );
        let chown = ReleaseOp::Chown {
            uid: 1000,
            entries: vec![(PathBuf::from("/a"), true), (PathBuf::from("/b c"), false)],
        };
        let wrapped = elevated_invocation(
            release_invocation(&chown, Os::Linux, &tools)?,
            Os::Linux,
            &tools,
        )?;
        assert_eq!(
            strings(&wrapped),
            [
                "/bin/sh",
                "-c",
                CHOWN_SCRIPT,
                "fluxdown-release",
                "/usr/bin/chown",
                "1000",
                "R",
                "/a",
                "N",
                "/b c"
            ]
        );
        let no_pkexec = Tools {
            pkexec: None,
            ..tools
        };
        assert!(matches!(
            elevated_invocation(
                release_invocation(&chown, Os::Linux, &no_pkexec)?,
                Os::Linux,
                &no_pkexec
            ),
            Err(PermissionError::ElevationUnavailable(_))
        ));
        Ok(())
    }

    #[test]
    fn windows_release_passes_icacls_command_line_through_environment()
    -> Result<(), PermissionError> {
        let tools = tools();
        let grant = ReleaseOp::Grant {
            principal: "*S-1-5-21-1".to_owned(),
            dir: PathBuf::from(r"C:\Users\A B\Down$(x)'loads\"),
        };
        let wrapped = elevated_invocation(
            release_invocation(&grant, Os::Windows, &tools)?,
            Os::Windows,
            &tools,
        )?;
        assert!(strings(&wrapped).iter().all(|arg| !arg.contains("Down")));
        let env: Vec<_> = wrapped
            .env
            .iter()
            .map(|(key, value)| (*key, value.to_string_lossy().into_owned()))
            .collect();
        assert_eq!(
            env,
            [
                (
                    "FLUXDOWN_ELEVATE_PROGRAM",
                    r"C:\Windows\System32\icacls.exe".to_owned()
                ),
                (
                    "FLUXDOWN_ELEVATE_ARGS",
                    r#""C:\Users\A B\Down$(x)'loads\\" /grant *S-1-5-21-1:(OI)(CI)M /L"#.to_owned()
                ),
            ]
        );
        assert!(matches!(
            release_invocation(
                &ReleaseOp::Chown {
                    uid: 1,
                    entries: Vec::new()
                },
                Os::Windows,
                &tools
            ),
            Err(PermissionError::NotApplicable(_))
        ));
        Ok(())
    }

    #[test]
    fn windows_command_line_follows_command_line_to_argv_rules() {
        let line = |args: &[&str]| {
            windows_command_line(&args.iter().map(OsString::from).collect::<Vec<_>>())
        };
        assert_eq!(line(&["a", "b"]), "a b");
        assert_eq!(line(&["a b"]), r#""a b""#);
        assert_eq!(line(&[r"C:\"]), r"C:\");
        assert_eq!(line(&[r"C:\a b\"]), r#""C:\a b\\""#);
        assert_eq!(line(&[r#"say "hi""#]), r#""say \"hi\"""#);
        assert_eq!(line(&[""]), r#""""#);
    }

    #[test]
    fn elevation_exit_codes_distinguish_cancel_unavailable_and_failure() {
        assert!(classify_elevation_exit(Os::Linux, Some(0), "").is_ok());
        assert!(matches!(
            classify_elevation_exit(Os::Linux, Some(126), ""),
            Err(PermissionError::Cancelled)
        ));
        assert!(matches!(
            classify_elevation_exit(Os::Linux, Some(127), "No authentication agent found."),
            Err(PermissionError::ElevationUnavailable(detail)) if detail.contains("agent")
        ));
        assert!(matches!(
            classify_elevation_exit(Os::Linux, Some(1), "chown: invalid user"),
            Err(PermissionError::Failed(detail)) if detail.contains("invalid user")
        ));
        assert!(matches!(
            classify_elevation_exit(Os::MacOs, Some(1), "execution error: User canceled. (-128)"),
            Err(PermissionError::Cancelled)
        ));
        assert!(matches!(
            classify_elevation_exit(Os::MacOs, Some(1), "execution error: chown: x (1)"),
            Err(PermissionError::Failed(_))
        ));
        assert!(matches!(
            classify_elevation_exit(Os::Windows, Some(1223), ""),
            Err(PermissionError::Cancelled)
        ));
        assert!(matches!(
            classify_elevation_exit(Os::Windows, None, ""),
            Err(PermissionError::Failed(_))
        ));
    }

    #[test]
    fn whoami_sid_is_parsed_from_csv() {
        assert_eq!(
            parse_whoami_sid("\"desktop-1\\zero\",\"S-1-5-21-111-222-1001\"\r\n").as_deref(),
            Some("S-1-5-21-111-222-1001")
        );
        assert_eq!(parse_whoami_sid("garbage"), None);
    }

    /// 释放脚本真实执行：改回自己的 uid 不需要 root，可验证参数成对解析与递归开关。
    #[cfg(unix)]
    #[tokio::test]
    async fn chown_script_releases_each_entry_with_its_recursion_flag()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir =
            std::env::temp_dir().join(format!("fluxdown-chown-{}", uuid::Uuid::new_v4().simple()));
        let spaced = dir.join("with space");
        std::fs::create_dir_all(spaced.join("child"))?;
        let uid = super::effective_uid()?;
        let tools = Tools::locate(Os::CURRENT);
        let invocation = release_invocation(
            &ReleaseOp::Chown {
                uid,
                entries: vec![(spaced.clone(), true), (dir.clone(), false)],
            },
            Os::CURRENT,
            &tools,
        )?;
        super::run_as_user(invocation, super::ELEVATION_TIMEOUT).await?;
        let missing = release_invocation(
            &ReleaseOp::Chown {
                uid,
                entries: vec![(dir.join("missing"), false)],
            },
            Os::CURRENT,
            &tools,
        )?;
        assert!(
            super::run_as_user(missing, super::ELEVATION_TIMEOUT)
                .await
                .is_err()
        );
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    /// macOS 引用规则真实执行（不提权的同一段 AppleScript）：特殊字符原样到达命令。
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn applescript_quotes_every_argument() -> Result<(), Box<dyn std::error::Error>> {
        let mut command = tokio::process::Command::new("/usr/bin/osascript");
        for line in super::applescript_lines(false) {
            command.arg("-e").arg(line);
        }
        let tricky = "a'b \"c\" $(d) `e`; f";
        let output = command.arg("/bin/echo").arg(tricky).output().await?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), tricky);
        Ok(())
    }

    /// macOS ACL 授权命令真实执行：自己拥有的目录无需提权即可加 ACL。
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn macos_grant_command_is_accepted_by_chmod() -> Result<(), Box<dyn std::error::Error>> {
        let dir =
            std::env::temp_dir().join(format!("fluxdown-acl-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir)?;
        let principal = super::current_principal().await?;
        let invocation = release_invocation(
            &ReleaseOp::Grant {
                principal,
                dir: dir.clone(),
            },
            Os::MacOs,
            &Tools::locate(Os::MacOs),
        )?;
        super::run_as_user(invocation, super::ELEVATION_TIMEOUT).await?;
        let listing = std::process::Command::new("/bin/ls")
            .arg("-led")
            .arg(&dir)
            .output()?;
        assert!(String::from_utf8_lossy(&listing.stdout).contains("allow"));
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
