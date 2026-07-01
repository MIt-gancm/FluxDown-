//! Application data directory resolution.
//!
//! Determines where FluxDown stores persistent data (database, logs, NMH manifests).
//!
//! ## Strategy
//!
//! | Platform        | Mode      | Directory                                      |
//! |-----------------|-----------|-------------------------------------------------|
//! | Windows         | Portable  | `<exe_dir>/`  (data travels with the app)       |
//! | Windows         | Installed | `%LOCALAPPDATA%\FluxDown\`                      |
//! | Linux           | —         | `$XDG_DATA_HOME/fluxdown/`                      |
//! | macOS           | —         | `~/Library/Application Support/fluxdown/`        |
//!
//! ### Portable detection (Windows only)
//!
//! A `portable` marker file next to the executable signals portable mode.
//! This is consistent with the existing check in `updater.rs` and the Dart-side
//! `_isPortableMode()` in `windows_toast_helper.dart`.

use std::path::{Path, PathBuf};

/// Marker file name — a zero-byte file placed next to the exe by the portable
/// ZIP distribution.  Matches `updater::PORTABLE_MARKER` and the Dart-side
/// `_portableMarker` constant.
#[cfg(target_os = "windows")]
const PORTABLE_MARKER: &str = "portable";

/// Errors that can occur while resolving the application data directory.
#[derive(Debug, thiserror::Error)]
pub enum DataDirError {
    /// Failed to create the resolved directory (or one of its ancestors).
    #[error("failed to create data directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Resolve the application data directory (for DB, logs, NMH manifests, etc.).
///
/// `explicit` overrides auto-detection when set (e.g. a CLI `--data-dir` flag
/// or a Server per-tenant directory); pass `None` to fall back to the
/// platform-specific auto-detection below (portable marker / `LOCALAPPDATA` /
/// XDG / macOS Application Support).
///
/// The returned path is guaranteed to exist (created if necessary).
///
/// # Examples
///
/// ```
/// use fluxdown_engine::data_dir::resolve_data_dir;
///
/// // Auto-detect the platform data directory.
/// let dir = resolve_data_dir(None).expect("data dir should be creatable");
/// assert!(dir.is_absolute() || dir.as_os_str() == ".");
/// ```
pub fn resolve_data_dir(explicit: Option<&Path>) -> Result<PathBuf, DataDirError> {
    let dir = match explicit {
        Some(path) => path.to_path_buf(),
        None => resolve_data_dir_inner(),
    };
    std::fs::create_dir_all(&dir).map_err(|source| DataDirError::CreateDir {
        path: dir.clone(),
        source,
    })?;
    Ok(dir)
}

fn resolve_data_dir_inner() -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        let base = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                PathBuf::from(home).join(".local").join("share")
            });
        base.join("fluxdown")
    }

    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("fluxdown")
    }

    #[cfg(target_os = "windows")]
    {
        if is_portable() {
            // Portable mode: data lives next to the exe.
            return exe_dir();
        }
        // Installed mode: use %LOCALAPPDATA%\FluxDown (always user-writable).
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(local).join("FluxDown");
        }
        // Fallback: %APPDATA%\FluxDown
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("FluxDown");
        }
        // Last resort: exe directory (may fail on write, but better than ".").
        exe_dir()
    }

    // Catch-all for other platforms (e.g. Android/iOS stubs) — should never
    // be reached in practice.
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        exe_dir()
    }
}

/// Windows portable detection: `portable` marker file exists next to the exe.
#[cfg(target_os = "windows")]
fn is_portable() -> bool {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        return dir.join(PORTABLE_MARKER).exists();
    }
    false
}

/// Returns the exe's parent directory, falling back to CWD or ".".
#[allow(dead_code)]
fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}
