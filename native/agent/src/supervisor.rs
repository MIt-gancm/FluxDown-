//! agent 对同级 `fluxdownd` 的单飞启动与异步回收。

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Mutex;

/// daemon 启动错误。
#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("could not locate sibling fluxdownd: {0}")]
    Locate(#[from] std::io::Error),
    #[error("failed to spawn fluxdownd: {0}")]
    Spawn(String),
}

#[derive(Default)]
struct SupervisorState {
    generation: u64,
    running: bool,
    reapers: Vec<tokio::task::JoinHandle<()>>,
}

/// 只在连接拒绝路径调用的 daemon 单飞启动器。
pub struct DaemonSupervisor {
    state: Arc<Mutex<SupervisorState>>,
    bind_addr: SocketAddr,
    /// 完全退出流程中置位：此后连接拒绝不再拉起 daemon。
    stopped: AtomicBool,
    /// 追加给 daemon 子进程的环境变量（如 server 模式生效的演示 URL）。
    extra_env: Vec<(String, String)>,
    /// daemon 子进程 stderr 的落盘文件；未设置或打开失败时丢弃。
    stderr_log: Option<PathBuf>,
    /// stderr 日志打开失败只告警一次。
    stderr_log_warned: AtomicBool,
    /// 连续快速异常退出次数（崩溃循环判定）。
    crash_streak: Arc<std::sync::atomic::AtomicU32>,
}

impl DaemonSupervisor {
    #[must_use]
    pub fn new(bind_addr: SocketAddr) -> Self {
        Self {
            state: Arc::new(Mutex::new(SupervisorState::default())),
            bind_addr,
            stopped: AtomicBool::new(false),
            extra_env: Vec::new(),
            stderr_log: None,
            stderr_log_warned: AtomicBool::new(false),
            crash_streak: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        }
    }

    /// 给拉起的 daemon 追加环境变量（覆盖继承的同名变量）。
    #[must_use]
    pub fn with_extra_env(mut self, env: Vec<(String, String)>) -> Self {
        self.extra_env = env;
        self
    }

    /// 把 daemon 的 stderr 追加到该文件（启动失败、panic 的唯一证据）；已超过 1 MiB 则截断重写。
    #[must_use]
    pub fn with_stderr_log(mut self, path: PathBuf) -> Self {
        self.stderr_log = Some(path);
        self
    }

    /// 永久停止监管（不可恢复）：随后的 [`Self::ensure_running`] 均为空操作。
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
    }

    /// 启动同级 daemon；短时间内并发/重复调用只产生一个子进程。
    ///
    /// 返回本进程所监管、仍存活的 daemon 子进程代际（刚拉起或早先拉起）；已停止监管时为
    /// `None`。代际让调用方区分「同一个子进程仍在初始化」与「子进程已退出又被重新拉起」。
    pub async fn ensure_running(&self) -> Result<Option<u64>, SupervisorError> {
        let mut state = self.state.lock().await;
        state.reapers.retain(|task| !task.is_finished());
        if self.stopped.load(Ordering::Acquire) {
            return Ok(None);
        }
        if state.running {
            return Ok(Some(state.generation));
        }
        let executable = daemon_executable()?;
        let mut command = std::process::Command::new(&executable);
        command
            .env("FLUXDOWN_DAEMON_BIND", self.bind_addr.to_string())
            .envs(self.extra_env.iter().cloned())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(self.open_stderr_log());
        detach_background_process(&mut command);
        let mut child = tokio::process::Command::from(command)
            .spawn()
            .map_err(|error| SupervisorError::Spawn(format!("{error:#}")))?;
        let spawned_at = std::time::Instant::now();
        state.generation = state.generation.saturating_add(1);
        let generation = state.generation;
        tracing::info!(
            executable = %executable.display(),
            generation,
            pid = child.id(),
            "spawned fluxdownd"
        );
        state.running = true;
        let supervisor_state = self.state.clone();
        let streak = self.crash_streak.clone();
        state.reapers.push(tokio::spawn(async move {
            let mut success = false;
            match child.wait().await {
                Ok(status) if status.success() => {
                    success = true;
                    tracing::info!(%status, generation, "supervised fluxdownd exited");
                }
                Ok(status) => tracing::warn!(
                    %status,
                    generation,
                    uptime_secs = spawned_at.elapsed().as_secs_f64(),
                    "supervised fluxdownd exited abnormally; see fluxdownd.stderr.log"
                ),
                Err(error) => tracing::warn!(error = %error, "failed to reap fluxdownd"),
            }
            let previous = streak.load(Ordering::Acquire);
            streak.store(
                next_crash_streak(previous, success, spawned_at.elapsed()),
                Ordering::Release,
            );
            let mut state = supervisor_state.lock().await;
            if state.generation == generation {
                state.running = false;
            }
        }));
        Ok(Some(generation))
    }

    /// 子进程是否处于「启动即崩溃」循环（连续快速异常退出）；调用方据此拉长重拉间隔。
    #[must_use]
    pub fn in_crash_loop(&self) -> bool {
        self.crash_streak.load(Ordering::Acquire) >= CRASH_LOOP_THRESHOLD
    }

    /// 已成功连上 daemon：之前的快速失败不再算数。
    pub fn clear_crash_streak(&self) {
        self.crash_streak.store(0, Ordering::Release);
    }
}

/// 连续快速异常退出达到该次数即视为崩溃循环。
const CRASH_LOOP_THRESHOLD: u32 = 3;
/// 存活不足该时长的异常退出计入崩溃循环。
const CRASH_FAST_EXIT: std::time::Duration = std::time::Duration::from_secs(30);

fn next_crash_streak(previous: u32, success: bool, uptime: std::time::Duration) -> u32 {
    if success || uptime >= CRASH_FAST_EXIT {
        0
    } else {
        previous.saturating_add(1)
    }
}

impl DaemonSupervisor {
    fn open_stderr_log(&self) -> Stdio {
        let Some(path) = &self.stderr_log else {
            return Stdio::null();
        };
        let mut options = std::fs::OpenOptions::new();
        options.create(true);
        let oversized = std::fs::metadata(path).is_ok_and(|meta| meta.len() > STDERR_LOG_MAX_BYTES);
        if oversized {
            options.write(true).truncate(true);
        } else {
            options.append(true);
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match options.open(path) {
            Ok(file) => Stdio::from(file),
            Err(error) => {
                if !self.stderr_log_warned.swap(true, Ordering::AcqRel) {
                    tracing::warn!(path = %path.display(), error = %error, "cannot open fluxdownd stderr log; discarding daemon stderr");
                }
                Stdio::null()
            }
        }
    }
}

/// daemon stderr 日志超过该大小时在下次拉起前截断。
const STDERR_LOG_MAX_BYTES: u64 = 1024 * 1024;

fn daemon_executable() -> Result<PathBuf, std::io::Error> {
    if let Some(path) = std::env::var_os("FLUXDOWN_DAEMON_BIN") {
        return Ok(PathBuf::from(path));
    }
    let current = std::env::current_exe()?;
    let name = if cfg!(windows) {
        "fluxdownd.exe"
    } else {
        "fluxdownd"
    };
    Ok(current.with_file_name(name))
}

/// 后台服务与拉起者解耦：Windows 不弹控制台窗；Unix 进入独立进程组，终端里对拉起者的
/// Ctrl-C（SIGINT 发给前台进程组）不会连带终止常驻 daemon。
#[cfg(windows)]
fn detach_background_process(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000);
}

#[cfg(unix)]
fn detach_background_process(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(any(windows, unix)))]
fn detach_background_process(_command: &mut std::process::Command) {}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn fast_failures_accumulate_and_healthy_run_resets() {
        let mut streak = 0;
        for _ in 0..CRASH_LOOP_THRESHOLD {
            streak = next_crash_streak(streak, false, Duration::from_secs(1));
        }
        assert_eq!(streak, CRASH_LOOP_THRESHOLD);
        assert_eq!(next_crash_streak(streak, false, Duration::from_secs(60)), 0);
        assert_eq!(next_crash_streak(streak, true, Duration::from_secs(1)), 0);
    }
}
