//! Windows 便携版的 `flux_down.exe` 转发器。
//!
//! 旧 Flutter 便携版的自动更新器覆盖解压后固定重启 `<dir>\flux_down.exe`；
//! 这里原样带参拉起同目录的 `fluxdown-desktop.exe` 后立即退出，
//! 代替整份复制桌面端（Linux tar.gz 用同名 shell 脚本做同一件事）。

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::process::{Command, ExitCode};

const DESKTOP_EXE: &str = if cfg!(windows) {
    "fluxdown-desktop.exe"
} else {
    "fluxdown-desktop"
};

fn desktop_path() -> std::io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let dir = exe
        .parent()
        .ok_or_else(|| std::io::Error::other("executable has no parent directory"))?;
    Ok(dir.join(DESKTOP_EXE))
}

fn main() -> ExitCode {
    let spawned = desktop_path()
        .and_then(|path| Command::new(path).args(std::env::args_os().skip(1)).spawn());
    match spawned {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("flux_down: failed to launch {DESKTOP_EXE}: {e}");
            ExitCode::FAILURE
        }
    }
}
