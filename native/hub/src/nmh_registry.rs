//! Chrome Native Messaging Host (NMH) manifest generation and registry registration.
//!
//! Registers `com.fluxdown.nmh` for Chrome, Edge, and Firefox so that the
//! browser extension can use `chrome.runtime.connectNative("com.fluxdown.nmh")`
//! to communicate with the FluxDown desktop app via the NMH relay binary.
//!
//! Registry keys (all HKCU — no admin required):
//!   Chrome:  `HKCU\Software\Google\Chrome\NativeMessagingHosts\com.fluxdown.nmh`
//!   Edge:    `HKCU\Software\Microsoft\Edge\NativeMessagingHosts\com.fluxdown.nmh`
//!   Firefox: `HKCU\Software\Mozilla\NativeMessagingHosts\com.fluxdown.nmh`
//!
//! Each key's default value points to a JSON manifest file that describes the NMH.
//!
//! The two manifest JSON files live in `<data_dir>\nmh\` (installed:
//! `%LOCALAPPDATA%\FluxDown\nmh\`; portable: `<exe_dir>\portable_data\nmh\`),
//! never next to the exe: an all-users install under `Program Files` is not
//! writable by the unelevated app, so writing there fails before any registry
//! key is created and the extension can never connect. Keys and manifests sit
//! outside the Windows installer's [Registry]/[Files] tracking, so
//! `installer/windows/setup.iss` removes them explicitly on uninstall
//! (`CurUninstallStepChanged` + `[UninstallDelete]`) — keep both in sync.
//!
//! Several FluxDown installs (Flutter / GPUI, installed / dev builds) may share
//! one machine and therefore one registration entry point (the Unix launcher
//! script, the Windows HKCU keys). Startup self-heal ([`auto_register`]) follows
//! the ownership rule in [`may_take_over`] instead of "last launch wins", so two
//! installs never overwrite each other on every start; the explicit Doctor
//! repair ([`register`]) always points the registration at this install.
//! The path-based rules mirror `native/agent/src/nmh.rs::registry` — change both.
//! The GPUI agent additionally takes over a relay that fails a live ping against it
//! (older endpoint or framing); this host does not probe.

use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Edge Add-ons store extension ID. Edge ignores the manifest `key` field, so
/// its store build gets a different ID than Chrome and must be listed
/// explicitly (Chromium native messaging `allowed_origins` has no wildcard).
/// Without this, Edge store users get "Access to the specified native
/// messaging host is forbidden" → extension stuck on "未连接".
const EDGE_EXTENSION_ID: &str = "chrome-extension://nglkkjbogjghekbhhcnccnpfedjbdhhd/";
/// Windows `register_with` writes every registry key unconditionally, so even
/// browsers that are not installed must check out; Unix only writes manifests
/// for installed browsers.
const REGISTERS_EVERY_TARGET: bool = cfg!(target_os = "windows");
/// Path fragments of dev builds and temporary mounts (matched lowercase, `/`-separated).
const TRANSIENT_RELAY_MARKERS: [&str; 7] = [
    "/target/debug/",
    "/target/release/",
    "/build/macos/build/products/",
    "/build/linux/",
    "/build/windows/",
    "/.mount_",
    "/apptranslocation/",
];

/// 单个浏览器的 NMH 注册状态。
#[derive(Debug, Clone)]
pub struct NmhTarget {
    /// 展示名，如 `"Chrome"` / `"Firefox"` / `"Brave (Flatpak)"`。
    pub label: String,
    /// 注册位置：Windows = `HKCU\Software\...\com.fluxdown.nmh`；类 Unix = 清单文件绝对路径。
    pub location: String,
    /// 该浏览器是否安装（配置根目录存在）。false 时 Doctor 只报 `info`，不算故障。
    pub installed: bool,
    /// 清单完整且与当前生效的注册一致（中继归属由 [`NmhDiagnosis::relay_owner`] 单独给出）。
    pub ok: bool,
    /// `ok == false` 时的具体原因（英文技术描述，不翻译）；ok 时为空串。
    /// 例：`"registry key missing"` / `"manifest file missing: <path>"` /
    /// `"missing Edge origin"`。
    pub issue: String,
}

/// 注册当前指向的中继相对本安装的归属。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RelayOwner {
    /// 未注册：类 Unix 启动脚本不存在；Windows 三个注册表键都不存在。
    #[default]
    Missing,
    /// 指向本安装的中继。
    Current,
    /// 指向另一份仍存在的 FluxDown 中继：扩展可用，浏览器冷启动会拉起那份安装。
    OtherInstall,
    /// 指向的中继不存在或无法解析（安装被移除、AppImage 挂载点失效、脚本被改坏）。
    Broken,
}

/// NMH 注册整体诊断快照。
#[derive(Debug, Clone, Default)]
pub struct NmhDiagnosis {
    /// NMH 中继可执行文件绝对路径；空 = 未找到。
    pub exe_path: String,
    /// 未找到中继时的原因原文；找到时为空串。
    pub exe_error: String,
    /// Chromium 清单文件绝对路径（**期望**路径，可能尚未写出）；无法推导时为空串。
    pub chromium_manifest: String,
    /// Firefox 清单文件绝对路径（同上；Linux 与 chromium 同名不同目录时给第一个候选）。
    pub firefox_manifest: String,
    /// 注册入口：类 Unix 为启动脚本路径，Windows 为提供生效中继的注册表键。
    pub relay_location: String,
    /// 注册实际指向的中继；未注册或无法解析时为空。
    pub registered_relay: String,
    /// `registered_relay` 相对本安装的归属。
    pub relay_owner: RelayOwner,
    /// 每个浏览器一条。未找到中继时可为空 vec。
    pub targets: Vec<NmhTarget>,
}

/// [`auto_register`] 的结果。
#[derive(Debug, PartialEq, Eq)]
pub enum AutoRegisterOutcome {
    /// 注册完整，未改动任何文件。
    UpToDate,
    /// 已重写注册，指向给定中继（可能是保留下来的另一份安装）。
    Registered(PathBuf),
}

/// 清单声明的 `path`。
fn manifest_relay(manifest: &Value) -> Option<&str> {
    manifest.get("path").and_then(Value::as_str)
}

/// Chromium 清单是否放行 Edge 商店扩展。
fn manifest_allows_edge(manifest: &Value) -> bool {
    manifest
        .get("allowed_origins")
        .and_then(Value::as_array)
        .is_some_and(|origins| {
            origins
                .iter()
                .any(|origin| origin.as_str() == Some(EDGE_EXTENSION_ID))
        })
}

/// 开发构建（cargo / Flutter 产物）或临时挂载（AppImage、macOS App Translocation）里的中继。
fn is_transient_relay(path: &Path) -> bool {
    let normalized = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    TRANSIENT_RELAY_MARKERS
        .iter()
        .any(|marker| normalized.contains(marker))
}

/// 两个中继路径是否指向同一文件（Windows 不区分大小写；再以 canonicalize 兜底符号链接）。
fn same_relay(a: &Path, b: &Path) -> bool {
    let literal = if cfg!(target_os = "windows") {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    } else {
        a == b
    };
    literal
        || matches!(
            (std::fs::canonicalize(a), std::fs::canonicalize(b)),
            (Ok(a), Ok(b)) if a == b
        )
}

fn classify_relay(registered: &Path, current: &Path) -> RelayOwner {
    if same_relay(registered, current) {
        RelayOwner::Current
    } else if registered.is_file() {
        RelayOwner::OtherInstall
    } else {
        RelayOwner::Broken
    }
}

/// 启动自愈能否把注册改指向本安装：未注册、失效或已是本安装时总可以；另一份健康
/// 安装只在它是开发构建/临时路径、而本安装不是时才被接管，其余情况保持先到者。
fn may_take_over(owner: RelayOwner, registered: &Path, current: &Path) -> bool {
    match owner {
        RelayOwner::Missing | RelayOwner::Broken | RelayOwner::Current => true,
        RelayOwner::OtherInstall => is_transient_relay(registered) && !is_transient_relay(current),
    }
}

/// 显式修复：把全部注册改指向本安装的中继。
pub fn register() -> Result<(), io::Error> {
    inner::register_with(&inner::find_nmh_exe()?)
}

/// 启动自愈：注册缺失、失效或不完整时按归属规则重写，完好时不碰任何文件。
pub fn auto_register() -> Result<AutoRegisterOutcome, io::Error> {
    let diagnosis = inner::diagnose();
    if diagnosis.exe_path.is_empty() {
        return Err(io::Error::new(io::ErrorKind::NotFound, diagnosis.exe_error));
    }
    let current = PathBuf::from(&diagnosis.exe_path);
    let registered = PathBuf::from(&diagnosis.registered_relay);
    let relay = if may_take_over(diagnosis.relay_owner, &registered, &current) {
        current
    } else {
        registered.clone()
    };
    let complete = matches!(
        diagnosis.relay_owner,
        RelayOwner::Current | RelayOwner::OtherInstall
    ) && same_relay(&relay, &registered)
        && diagnosis
            .targets
            .iter()
            .all(|target| target.ok || !(target.installed || REGISTERS_EVERY_TARGET));
    if complete {
        return Ok(AutoRegisterOutcome::UpToDate);
    }
    inner::register_with(&relay)?;
    Ok(AutoRegisterOutcome::Registered(relay))
}

/// POSIX 单引号转义：`'` → `'\''`。
#[cfg(unix)]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// 启动脚本内容：`exec` 真实中继并转发浏览器追加的参数。
#[cfg(unix)]
fn wrapper_script(relay: &Path) -> String {
    format!(
        "#!/bin/sh\nexec {} \"$@\"\n",
        shell_quote(&relay.to_string_lossy())
    )
}

/// 启动脚本 `exec '<relay>' "$@"` 里的中继路径（兼容 `'\''` 转义）。
#[cfg(unix)]
fn parse_wrapper_relay(script: &str) -> Option<PathBuf> {
    let rest = script
        .lines()
        .find_map(|line| line.trim_start().strip_prefix("exec "))?;
    let mut relay = String::new();
    let mut chars = rest.trim_start().chars();
    loop {
        match chars.next() {
            Some('\'') => loop {
                match chars.next()? {
                    '\'' => break,
                    other => relay.push(other),
                }
            },
            Some('\\') => relay.push(chars.next()?),
            _ => break,
        }
    }
    (!relay.is_empty()).then(|| PathBuf::from(relay))
}

/// 启动脚本的归属与其指向的中继；不可执行的脚本浏览器拉不起来，按失效处理。
#[cfg(unix)]
fn diagnose_wrapper(wrapper: &Path, current: &Path) -> (RelayOwner, String) {
    use std::os::unix::fs::PermissionsExt;

    let script = match std::fs::read_to_string(wrapper) {
        Ok(script) => script,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return (RelayOwner::Missing, String::new());
        }
        Err(_) => return (RelayOwner::Broken, String::new()),
    };
    let Some(relay) = parse_wrapper_relay(&script) else {
        return (RelayOwner::Broken, String::new());
    };
    let executable =
        std::fs::metadata(wrapper).is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0);
    let owner = if executable {
        classify_relay(&relay, current)
    } else {
        RelayOwner::Broken
    };
    (owner, relay.to_string_lossy().into_owned())
}

/// 单个浏览器清单的问题：必须是指向启动脚本的合法 JSON；完好时为空串。
/// 中继归属由启动脚本单独判定，不在每个浏览器上重复报告。
#[cfg(unix)]
fn manifest_issue(manifest: &Path, wrapper_str: &str, require_edge_origin: bool) -> String {
    let location = manifest.to_string_lossy();
    match std::fs::read_to_string(manifest) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            format!("manifest file missing: {location}")
        }
        Err(e) => format!("manifest unreadable: {location}: {e:#}"),
        Ok(content) => match serde_json::from_str::<Value>(&content) {
            Err(e) => format!("manifest is not valid JSON: {location}: {e:#}"),
            Ok(json) if manifest_relay(&json) != Some(wrapper_str) => {
                format!("manifest does not point to the launcher script: {location}")
            }
            Ok(json) if require_edge_origin && !manifest_allows_edge(&json) => {
                format!("missing Edge origin in manifest: {location}")
            }
            Ok(_) => String::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{RelayOwner, classify_relay, is_transient_relay, may_take_over};

    const INSTALLED: &str = "/Applications/FluxDown.app/Contents/MacOS/fluxdown_nmh";
    const DEV: &str = "/Users/dev/FluxDown/build/macos/Build/Products/Debug/FluxDown.app/Contents/MacOS/fluxdown_nmh";

    #[test]
    fn healthy_other_install_is_only_taken_over_from_a_dev_build() {
        let other_installed = Path::new("/opt/fluxdown/fluxdown_nmh");
        assert!(!is_transient_relay(other_installed));
        assert!(is_transient_relay(Path::new(DEV)));
        assert!(!may_take_over(
            RelayOwner::OtherInstall,
            other_installed,
            Path::new(INSTALLED)
        ));
        assert!(!may_take_over(
            RelayOwner::OtherInstall,
            other_installed,
            Path::new(DEV)
        ));
        assert!(may_take_over(
            RelayOwner::OtherInstall,
            Path::new(DEV),
            Path::new(INSTALLED)
        ));
        for owner in [RelayOwner::Missing, RelayOwner::Broken, RelayOwner::Current] {
            assert!(may_take_over(owner, other_installed, Path::new(DEV)));
        }
    }

    #[test]
    fn classify_distinguishes_current_other_and_missing_relays() {
        let dir = std::env::temp_dir().join(format!(
            "fluxdown_hub_nmh_owner_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).ok();
        let current = dir.join("current_nmh");
        let other = dir.join("other_nmh");
        std::fs::write(&current, b"").ok();
        std::fs::write(&other, b"").ok();
        assert_eq!(classify_relay(&current, &current), RelayOwner::Current);
        assert_eq!(classify_relay(&other, &current), RelayOwner::OtherInstall);
        assert_eq!(
            classify_relay(&dir.join("removed_nmh"), &current),
            RelayOwner::Broken
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn wrapper_relay_round_trips_through_shell_quoting() {
        use super::{parse_wrapper_relay, wrapper_script};

        for relay in [INSTALLED, "/home/o'brien/My Apps/fluxdown_nmh"] {
            assert_eq!(
                parse_wrapper_relay(&wrapper_script(Path::new(relay))).as_deref(),
                Some(Path::new(relay))
            );
        }
        assert_eq!(parse_wrapper_relay("#!/bin/sh\necho hi\n"), None);
    }
}

#[cfg(target_os = "windows")]
mod inner {
    use super::{EDGE_EXTENSION_ID, NmhDiagnosis, NmhTarget, RelayOwner};
    use crate::logger::log_info;
    use serde::Serialize;
    use serde_json::Value;
    use std::io;
    use std::path::{Path, PathBuf};
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};

    const NMH_NAME: &str = "com.fluxdown.nmh";
    const NMH_DESCRIPTION: &str = "FluxDown Native Messaging Host";
    const NMH_EXE_NAME: &str = "fluxdown_nmh.exe";

    /// Manifest filename for Chrome/Edge (contains `allowed_origins`).
    const MANIFEST_FILENAME_CHROMIUM: &str = "com.fluxdown.nmh.json";
    /// Manifest filename for Firefox (contains `allowed_extensions`, NO `allowed_origins`).
    /// Firefox schema validation (NativeManifests.sys.mjs via Schemas.normalize) rejects any
    /// field not in its native_manifest.json schema. `allowed_origins` is Chrome-only and
    /// causes Firefox to report "No such native application" (Bugzilla #1361459).
    const MANIFEST_FILENAME_FIREFOX: &str = "com.fluxdown.nmh.firefox.json";

    /// Chrome extension ID — pinned via `key` in wxt.config.ts manifest.
    const CHROME_EXTENSION_ID: &str = "chrome-extension://meleenglfggcmcajknpeeeiobnpfmahc/";

    /// Firefox extension ID (matches `browser_specific_settings.gecko.id` in manifest).
    const FIREFOX_EXTENSION_ID: &str = "fluxdown@fluxdown.app";

    /// Chromium (Chrome/Edge) NMH manifest — uses `allowed_origins`.
    #[derive(Serialize)]
    struct NmhManifestChromium {
        name: String,
        description: String,
        path: String,
        #[serde(rename = "type")]
        host_type: String,
        allowed_origins: Vec<String>,
    }

    /// Firefox NMH manifest — uses `allowed_extensions` ONLY.
    /// Firefox schema (native_manifest.json) does not define `allowed_origins`;
    /// including it causes schema validation to fail with "No such native application".
    #[derive(Serialize)]
    struct NmhManifestFirefox {
        name: String,
        description: String,
        path: String,
        #[serde(rename = "type")]
        host_type: String,
        allowed_extensions: Vec<String>,
    }

    /// Strip `\\?\` UNC prefix from a path string (if present).
    fn strip_unc_prefix(s: &str) -> String {
        s.strip_prefix(r"\\?\").unwrap_or(s).to_string()
    }

    /// Find the NMH executable, searching multiple locations.
    ///
    /// Search order:
    /// 1. Same directory as the current app exe (production deployment)
    /// 2. Cargo workspace `target/debug/` (development — `flutter run`)
    /// 3. Cargo workspace `target/release/` (development — release build)
    pub(super) fn find_nmh_exe() -> Result<PathBuf, io::Error> {
        // 1. Next to current exe (production: NMH ships alongside the app)
        if let Ok(exe) = std::env::current_exe() {
            let canonical = std::fs::canonicalize(&exe).unwrap_or(exe);
            if let Some(dir) = canonical.parent() {
                let candidate = dir.join(NMH_EXE_NAME);
                if candidate.exists() {
                    log_info!(
                        "[nmh_registry] found NMH exe next to app: {}",
                        candidate.display()
                    );
                    return Ok(candidate);
                }
            }
        }

        // 2+3. Cargo workspace target directory (development)
        // CARGO_MANIFEST_DIR is baked in at compile time for the hub crate.
        // hub crate is at <workspace>/native/hub, so workspace root is 2 levels up.
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let workspace_root = Path::new(manifest_dir).parent().and_then(|p| p.parent());

        if let Some(ws) = workspace_root {
            for profile in &["debug", "release"] {
                let candidate = ws.join("target").join(profile).join(NMH_EXE_NAME);
                if candidate.exists() {
                    log_info!(
                        "[nmh_registry] found NMH exe in cargo target: {}",
                        candidate.display()
                    );
                    return Ok(candidate);
                }
            }
        }

        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "{} not found. Build it with: cargo build -p fluxdown_nmh",
                NMH_EXE_NAME
            ),
        ))
    }

    /// Per-user, always-writable directory holding the manifest JSON files.
    ///
    /// `%LOCALAPPDATA%\FluxDown\nmh` for installed builds, `<exe_dir>\portable_data\nmh`
    /// for portable ones (`fluxdown_engine::data_dir` decides). The install directory
    /// is deliberately not used: an all-users install lands in `Program Files`, which
    /// the unelevated app cannot write.
    fn manifest_dir() -> Result<PathBuf, io::Error> {
        fluxdown_engine::data_dir::resolve_data_dir(None)
            .map(|d| d.join("nmh"))
            .map_err(|e| io::Error::other(format!("data dir unavailable: {e:#}")))
    }

    /// Expected (UNC-stripped) manifest paths `(chromium, firefox)` for the current user.
    fn expected_manifest_paths() -> Result<(String, String), io::Error> {
        let dir = manifest_dir()?;
        Ok((
            strip_unc_prefix(&dir.join(MANIFEST_FILENAME_CHROMIUM).to_string_lossy()),
            strip_unc_prefix(&dir.join(MANIFEST_FILENAME_FIREFOX).to_string_lossy()),
        ))
    }

    /// Write two NMH manifest JSON files into [`manifest_dir`]:
    /// - Chromium manifest (Chrome/Edge): contains `allowed_origins`
    /// - Firefox manifest: contains `allowed_extensions` ONLY (no `allowed_origins`)
    ///
    /// Returns `(chromium_manifest_path, firefox_manifest_path)`.
    fn write_manifests(nmh_exe: &Path) -> Result<(PathBuf, PathBuf), io::Error> {
        let nmh_path_str = strip_unc_prefix(&nmh_exe.to_string_lossy());
        let dir = manifest_dir()?;
        std::fs::create_dir_all(&dir)?;

        // Chromium manifest (Chrome + Edge)
        let chromium = NmhManifestChromium {
            name: NMH_NAME.to_string(),
            description: NMH_DESCRIPTION.to_string(),
            path: nmh_path_str.clone(),
            host_type: "stdio".to_string(),
            allowed_origins: vec![
                CHROME_EXTENSION_ID.to_string(),
                EDGE_EXTENSION_ID.to_string(),
            ],
        };
        let chromium_json = serde_json::to_string_pretty(&chromium)
            .map_err(|e| io::Error::other(format!("JSON serialize error: {}", e)))?;
        let chromium_path = dir.join(MANIFEST_FILENAME_CHROMIUM);
        std::fs::write(&chromium_path, chromium_json)?;

        // Firefox manifest — NO `allowed_origins` field (Bugzilla #1361459)
        let firefox = NmhManifestFirefox {
            name: NMH_NAME.to_string(),
            description: NMH_DESCRIPTION.to_string(),
            path: nmh_path_str,
            host_type: "stdio".to_string(),
            allowed_extensions: vec![FIREFOX_EXTENSION_ID.to_string()],
        };
        let firefox_json = serde_json::to_string_pretty(&firefox)
            .map_err(|e| io::Error::other(format!("JSON serialize error: {}", e)))?;
        let firefox_path = dir.join(MANIFEST_FILENAME_FIREFOX);
        std::fs::write(&firefox_path, firefox_json)?;

        Ok((chromium_path, firefox_path))
    }

    /// Chromium-family registry paths on Windows.
    ///
    /// Brave, Vivaldi, Opera and most other Chromium forks fall back to reading
    /// Chrome's `Software\Google\Chrome\NativeMessagingHosts` registry key when
    /// their own key is absent (verified via KeePassXC source and Chromium
    /// source).  Only Chrome and Edge need dedicated keys.
    const CHROMIUM_REG_PATHS: &[&str] = &[
        r"Software\Google\Chrome\NativeMessagingHosts",
        r"Software\Microsoft\Edge\NativeMessagingHosts",
    ];

    /// Register each browser's registry key pointing to its dedicated manifest.
    /// Chrome and Edge use the Chromium manifest; Firefox uses the Firefox-only manifest.
    /// Other Chromium browsers (Brave, Vivaldi, Opera) fall back to Chrome's key.
    fn register_registry(chromium_manifest: &str, firefox_manifest: &str) -> Result<(), io::Error> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);

        for reg_path in CHROMIUM_REG_PATHS {
            let full_path = format!("{}\\{}", reg_path, NMH_NAME);
            let (key, _) = hkcu.create_subkey_with_flags(&full_path, KEY_WRITE)?;
            key.set_value("", &chromium_manifest)?;
            log_info!("[nmh_registry] registered at HKCU\\{}", full_path);
        }

        let firefox_reg = format!("{}\\{}", r"Software\Mozilla\NativeMessagingHosts", NMH_NAME);
        let (key, _) = hkcu.create_subkey_with_flags(&firefox_reg, KEY_WRITE)?;
        key.set_value("", &firefox_manifest)?;
        log_info!("[nmh_registry] registered at HKCU\\{}", firefox_reg);

        Ok(())
    }

    const FIREFOX_REG_PATH: &str = r"Software\Mozilla\NativeMessagingHosts";
    /// `(registry path, label, is Chromium manifest)`; the first key that resolves
    /// to a relay defines the active registration.
    const REG_TARGETS: [(&str, &str, bool); 3] = [
        (CHROMIUM_REG_PATHS[0], "Chrome", true),
        (CHROMIUM_REG_PATHS[1], "Edge", true),
        (FIREFOX_REG_PATH, "Firefox", false),
    ];

    /// What one registry key resolves to.
    enum Registration {
        KeyMissing(String),
        Invalid(String),
        Valid { manifest: String, json: Value },
    }

    /// `true` if `%VAR%\<rest…>` exists and is a directory.
    /// Used as the "browser installed" proxy on Windows (user-data root).
    fn env_dir_exists(var: &str, rest: &[&str]) -> bool {
        let Ok(base) = std::env::var(var) else {
            return false;
        };
        let mut path = PathBuf::from(base);
        for segment in rest {
            path.push(segment);
        }
        path.is_dir()
    }

    fn read_registration(hkcu: &RegKey, reg_path: &str) -> Registration {
        let full_path = format!("{reg_path}\\{NMH_NAME}");
        let Ok(key) = hkcu.open_subkey_with_flags(&full_path, KEY_READ) else {
            return Registration::KeyMissing(format!("registry key missing: HKCU\\{full_path}"));
        };
        let Ok(manifest) = key.get_value::<String, _>("") else {
            return Registration::Invalid(format!(
                "registry default value unreadable: HKCU\\{full_path}"
            ));
        };
        let content = match std::fs::read_to_string(&manifest) {
            Ok(content) => content,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Registration::Invalid(format!("manifest file missing: {manifest}"));
            }
            Err(e) => {
                return Registration::Invalid(format!("manifest unreadable: {manifest}: {e:#}"));
            }
        };
        match serde_json::from_str(&content) {
            Ok(json) => Registration::Valid { manifest, json },
            Err(e) => {
                Registration::Invalid(format!("manifest is not valid JSON: {manifest}: {e:#}"))
            }
        }
    }

    /// One key's problem relative to the active registration; empty when fine.
    /// With `owner_is_current` the manifest must also live in this install's data
    /// dir (migrates manifests older releases wrote next to the exe).
    fn target_issue(
        registration: &Registration,
        registered: &Path,
        owner_is_current: bool,
        expected_manifest: &str,
        require_edge_origin: bool,
    ) -> String {
        let (manifest, json) = match registration {
            Registration::KeyMissing(issue) | Registration::Invalid(issue) => {
                return issue.clone();
            }
            Registration::Valid { manifest, json } => (manifest, json),
        };
        let Some(relay) = super::manifest_relay(json) else {
            return format!("manifest has no relay path: {manifest}");
        };
        if !super::same_relay(Path::new(&strip_unc_prefix(relay)), registered) {
            return format!(
                "manifest points to a different relay than the active registration: {manifest}"
            );
        }
        if owner_is_current && !manifest.eq_ignore_ascii_case(expected_manifest) {
            return format!("registry points to unexpected manifest: {manifest}");
        }
        if require_edge_origin && !super::manifest_allows_edge(json) {
            return format!("missing Edge origin in manifest: {manifest}");
        }
        String::new()
    }

    /// Read-only snapshot of the NMH registration state for the Doctor page and
    /// startup self-heal. Never writes the registry, manifests or directories.
    pub fn diagnose() -> NmhDiagnosis {
        let mut diag = NmhDiagnosis::default();

        let nmh_exe = match find_nmh_exe() {
            Ok(p) => p,
            Err(e) => {
                diag.exe_error = format!("{e:#}");
                return diag;
            }
        };
        diag.exe_path = strip_unc_prefix(&nmh_exe.to_string_lossy());
        match expected_manifest_paths() {
            Ok((chromium, firefox)) => {
                diag.chromium_manifest = chromium;
                diag.firefox_manifest = firefox;
            }
            Err(e) => {
                diag.exe_error = format!("{e:#}");
                diag.exe_path.clear();
                return diag;
            }
        }

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let registrations = REG_TARGETS.map(|(reg_path, _, _)| read_registration(&hkcu, reg_path));
        let active =
            REG_TARGETS
                .iter()
                .zip(&registrations)
                .find_map(|((reg_path, _, _), registration)| match registration {
                    Registration::Valid { json, .. } => super::manifest_relay(json)
                        .map(|relay| (*reg_path, strip_unc_prefix(relay))),
                    _ => None,
                });
        match active {
            Some((reg_path, relay)) => {
                diag.relay_location = format!("HKCU\\{reg_path}\\{NMH_NAME}");
                diag.relay_owner =
                    super::classify_relay(Path::new(&relay), Path::new(&diag.exe_path));
                diag.registered_relay = relay;
            }
            None => {
                diag.relay_location = format!("HKCU\\{}\\{NMH_NAME}", CHROMIUM_REG_PATHS[0]);
                diag.relay_owner = if registrations
                    .iter()
                    .all(|registration| matches!(registration, Registration::KeyMissing(_)))
                {
                    RelayOwner::Missing
                } else {
                    RelayOwner::Broken
                };
            }
        }

        let registered = PathBuf::from(&diag.registered_relay);
        let owner_is_current = diag.relay_owner == RelayOwner::Current;
        let installed = [
            env_dir_exists("LOCALAPPDATA", &["Google", "Chrome", "User Data"]),
            env_dir_exists("LOCALAPPDATA", &["Microsoft", "Edge", "User Data"]),
            env_dir_exists("APPDATA", &["Mozilla", "Firefox"]),
        ];
        for (((reg_path, label, chromium), registration), installed) in
            REG_TARGETS.iter().zip(&registrations).zip(installed)
        {
            let expected_manifest = if *chromium {
                &diag.chromium_manifest
            } else {
                &diag.firefox_manifest
            };
            let issue = target_issue(
                registration,
                &registered,
                owner_is_current,
                expected_manifest,
                *chromium,
            );
            diag.targets.push(NmhTarget {
                label: (*label).to_string(),
                location: format!("HKCU\\{reg_path}\\{NMH_NAME}"),
                installed,
                ok: issue.is_empty(),
                issue,
            });
        }

        diag
    }

    /// Point every browser's registration at `relay`.
    ///
    /// Writes two separate manifest files:
    /// - Chromium manifest (Chrome/Edge): contains `allowed_origins`
    /// - Firefox manifest: contains `allowed_extensions` ONLY
    ///
    /// Idempotent.
    pub(super) fn register_with(relay: &Path) -> Result<(), io::Error> {
        let (chromium_path, firefox_path) = write_manifests(relay)?;
        let chromium_str = strip_unc_prefix(&chromium_path.to_string_lossy());
        let firefox_str = strip_unc_prefix(&firefox_path.to_string_lossy());
        let nmh_str = strip_unc_prefix(&relay.to_string_lossy());
        register_registry(&chromium_str, &firefox_str)?;
        remove_legacy_manifests(relay);
        log_info!(
            "[nmh_registry] NMH registered: relay={}, chromium_manifest={}, firefox_manifest={}",
            nmh_str,
            chromium_str,
            firefox_str,
        );
        Ok(())
    }

    /// Older releases wrote the manifests next to the NMH exe; delete them so a
    /// stale copy in the install directory cannot mislead anyone debugging the
    /// registration. Best-effort: on an all-users install this directory is
    /// read-only and the files simply stay (uninstall removes them).
    fn remove_legacy_manifests(nmh_exe: &Path) {
        if let Some(dir) = nmh_exe.parent() {
            let _ = std::fs::remove_file(dir.join(MANIFEST_FILENAME_CHROMIUM));
            let _ = std::fs::remove_file(dir.join(MANIFEST_FILENAME_FIREFOX));
        }
    }

    /// Remove NMH registration for all browsers and delete manifest files.
    #[allow(dead_code)]
    pub fn unregister() -> Result<(), io::Error> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);

        // Remove all Chromium-family browser registry keys
        for reg_path in CHROMIUM_REG_PATHS {
            match hkcu.open_subkey_with_flags(reg_path, KEY_WRITE) {
                Ok(parent) => {
                    let _ = parent.delete_subkey(NMH_NAME);
                }
                Err(_) => continue,
            }
        }
        // Remove Firefox registry key
        if let Ok(parent) =
            hkcu.open_subkey_with_flags(r"Software\Mozilla\NativeMessagingHosts", KEY_WRITE)
        {
            let _ = parent.delete_subkey(NMH_NAME);
        }

        // Remove both manifest files (best-effort; dir may never have been created).
        if let Ok(dir) = manifest_dir() {
            let _ = std::fs::remove_file(dir.join(MANIFEST_FILENAME_CHROMIUM));
            let _ = std::fs::remove_file(dir.join(MANIFEST_FILENAME_FIREFOX));
        }

        log_info!("[nmh_registry] NMH registration removed");
        Ok(())
    }

    #[cfg(test)]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    mod tests {
        use super::NMH_NAME;
        use crate::nmh_registry::{AutoRegisterOutcome, auto_register, register};
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};

        /// 本机注册表冒烟：Firefox 键被外部删除后启动自愈必须重写注册。
        ///
        /// 依赖真实 HKCU 注册表与已构建的 `fluxdown_nmh.exe`（`cargo build -p fluxdown_nmh`），
        /// 会改写本机 NMH 注册（指向 workspace target 目录），故标记 ignore，手动执行：
        /// `cargo test -p hub -- --ignored firefox_key_self_heal`
        #[test]
        #[ignore]
        fn firefox_key_self_heal() {
            // 基线：全量注册后一切匹配。
            register().expect("register");
            assert_eq!(
                auto_register().expect("auto register"),
                AutoRegisterOutcome::UpToDate
            );

            // 模拟外部删除 Firefox 键（杀毒/清理工具场景）。
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let parent = hkcu
                .open_subkey_with_flags(r"Software\Mozilla\NativeMessagingHosts", KEY_WRITE)
                .expect("open Mozilla NMH parent");
            parent.delete_subkey(NMH_NAME).expect("delete firefox key");

            // 缺失必须触发重注册，重写后恢复完好。
            assert!(matches!(
                auto_register().expect("self heal"),
                AutoRegisterOutcome::Registered(_)
            ));
            assert_eq!(
                auto_register().expect("auto register"),
                AutoRegisterOutcome::UpToDate
            );
        }
    }
}

// Linux: write NMH manifest files to XDG browser directories.
#[cfg(target_os = "linux")]
mod inner {
    use super::{EDGE_EXTENSION_ID, NmhDiagnosis, NmhTarget};
    use crate::logger::log_info;
    use serde::Serialize;
    use std::io;
    use std::path::{Path, PathBuf};

    const NMH_NAME: &str = "com.fluxdown.nmh";
    const NMH_DESCRIPTION: &str = "FluxDown Native Messaging Host";
    const NMH_EXE_NAME: &str = "fluxdown_nmh";
    /// Shell wrapper script registered in the NMH manifest.
    /// Provides a stable path even for AppImage builds where the real binary
    /// lives at a random FUSE mount point that changes on every launch.
    const NMH_WRAPPER_NAME: &str = "fluxdown_nmh.sh";
    const MANIFEST_FILENAME_CHROMIUM: &str = "com.fluxdown.nmh.json";
    const MANIFEST_FILENAME_FIREFOX: &str = "com.fluxdown.nmh.json";
    const CHROME_EXTENSION_ID: &str = "chrome-extension://meleenglfggcmcajknpeeeiobnpfmahc/";
    const FIREFOX_EXTENSION_ID: &str = "fluxdown@fluxdown.app";

    #[derive(Serialize)]
    struct NmhManifestChromium {
        name: String,
        description: String,
        path: String,
        #[serde(rename = "type")]
        host_type: String,
        allowed_origins: Vec<String>,
    }

    #[derive(Serialize)]
    struct NmhManifestFirefox {
        name: String,
        description: String,
        path: String,
        #[serde(rename = "type")]
        host_type: String,
        allowed_extensions: Vec<String>,
    }

    fn home_dir() -> Option<PathBuf> {
        std::env::var("HOME").ok().map(PathBuf::from)
    }

    /// All Chromium-family NMH manifest directories on Linux.
    ///
    /// Covers standard deb/rpm/tar.gz installs as well as Flatpak and Snap
    /// variants, which use isolated profile directories under ~/.var/app/ and
    /// ~/snap/ respectively.
    fn chromium_nmh_dirs() -> Vec<PathBuf> {
        let Some(home) = home_dir() else {
            return vec![];
        };
        let config = home.join(".config");
        let var_app = home.join(".var").join("app");
        let snap = home.join("snap");
        vec![
            // ── Standard deb/rpm/tar.gz installs ──
            config.join("google-chrome").join("NativeMessagingHosts"),
            config.join("chromium").join("NativeMessagingHosts"),
            config.join("microsoft-edge").join("NativeMessagingHosts"),
            // Brave Browser (verified via KeePassXC source)
            config
                .join("BraveSoftware")
                .join("Brave-Browser")
                .join("NativeMessagingHosts"),
            // Vivaldi (verified via KeePassXC source)
            config.join("vivaldi").join("NativeMessagingHosts"),
            // Thorium (Chromium fork; user-reported, #360)
            config.join("thorium").join("NativeMessagingHosts"),
            // Norton Neo (Chromium-based AI browser; user-reported, #623/#653).
            // Confirmed via /etc/neo/native-messaging-hosts embedded in the
            // official Linux .deb (vendor product dir name = "neo").
            config.join("neo").join("NativeMessagingHosts"),
            // ── Flatpak variants ──
            // Flatpak Chrome
            var_app
                .join("com.google.Chrome")
                .join("config")
                .join("google-chrome")
                .join("NativeMessagingHosts"),
            // Flatpak Chromium
            var_app
                .join("org.chromium.Chromium")
                .join("config")
                .join("chromium")
                .join("NativeMessagingHosts"),
            // Flatpak Edge
            var_app
                .join("com.microsoft.Edge")
                .join("config")
                .join("microsoft-edge")
                .join("NativeMessagingHosts"),
            // Flatpak Brave
            var_app
                .join("com.brave.Browser")
                .join("config")
                .join("BraveSoftware")
                .join("Brave-Browser")
                .join("NativeMessagingHosts"),
            // ── Snap variants ──
            // Snap Chromium
            snap.join("chromium")
                .join("common")
                .join(".config")
                .join("chromium")
                .join("NativeMessagingHosts"),
        ]
    }

    /// All Firefox-family NMH manifest directories on Linux.
    ///
    /// Returns multiple paths: standard location, Flatpak sandboxed variants,
    /// and Firefox-fork browsers (LibreWolf, Waterfox).
    /// Registration writes to every dir whose browser profile root exists;
    /// startup self-heal requires each such dir's manifest to exist and match
    /// (self-heals external deletion and browsers installed later, #159).
    fn firefox_nmh_dirs() -> Vec<PathBuf> {
        let Some(home) = home_dir() else {
            return vec![];
        };
        let var_app = home.join(".var").join("app");
        vec![
            // Standard Firefox
            home.join(".mozilla").join("native-messaging-hosts"),
            // Flatpak Firefox
            var_app
                .join("org.mozilla.firefox")
                .join(".mozilla")
                .join("native-messaging-hosts"),
            // LibreWolf (privacy-focused Firefox fork, verified via official FAQ)
            home.join(".librewolf").join("native-messaging-hosts"),
            // Zen Browser (Firefox fork, uses its own ~/.zen profile root, #313)
            home.join(".zen").join("native-messaging-hosts"),
            // Flatpak LibreWolf
            var_app
                .join("io.gitlab.librewolf-community")
                .join(".librewolf")
                .join("native-messaging-hosts"),
        ]
    }

    pub(super) fn find_nmh_exe() -> Result<PathBuf, io::Error> {
        // 1. Next to current exe (production deployment, including AppImage mount)
        if let Ok(exe) = std::env::current_exe() {
            let canonical = std::fs::canonicalize(&exe).unwrap_or(exe);
            if let Some(dir) = canonical.parent() {
                let candidate = dir.join(NMH_EXE_NAME);
                if candidate.exists() {
                    log_info!(
                        "[nmh_registry] found NMH exe next to app: {}",
                        candidate.display()
                    );
                    return Ok(candidate);
                }
            }
        }

        // 2. Cargo workspace target directory (development)
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let workspace_root = Path::new(manifest_dir).parent().and_then(|p| p.parent());

        if let Some(ws) = workspace_root {
            for profile in &["debug", "release"] {
                let candidate = ws.join("target").join(profile).join(NMH_EXE_NAME);
                if candidate.exists() {
                    log_info!(
                        "[nmh_registry] found NMH exe in cargo target: {}",
                        candidate.display()
                    );
                    return Ok(candidate);
                }
            }
        }

        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "{} not found. Build it with: cargo build -p fluxdown_nmh",
                NMH_EXE_NAME
            ),
        ))
    }

    /// Stable wrapper script path: ~/.local/share/fluxdown/fluxdown_nmh.sh
    ///
    /// NMH manifests always point to this wrapper rather than the real binary.
    /// This decouples the manifest from AppImage mount points (which change on
    /// every launch) and from Cargo target directories (which are dev-only).
    fn wrapper_path() -> Option<PathBuf> {
        home_dir().map(|h| {
            h.join(".local")
                .join("share")
                .join("fluxdown")
                .join(NMH_WRAPPER_NAME)
        })
    }

    /// Write the shell wrapper script that exec's `relay`.
    ///
    /// By registering a wrapper script instead of the binary directly, we
    /// provide a stable path even when the binary lives in a temporary AppImage
    /// mount point; self-heal rewrites it whenever the registered relay is gone.
    fn write_wrapper_script(relay: &Path) -> Result<PathBuf, io::Error> {
        let Some(wp) = wrapper_path() else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "cannot determine home directory for wrapper script",
            ));
        };
        if let Some(parent) = wp.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&wp, super::wrapper_script(relay))?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&wp, std::fs::Permissions::from_mode(0o755))?;
        Ok(wp)
    }

    fn write_chromium_manifest(wrapper: &Path, dir: &Path) -> Result<PathBuf, io::Error> {
        std::fs::create_dir_all(dir)?;
        let manifest = NmhManifestChromium {
            name: NMH_NAME.to_string(),
            description: NMH_DESCRIPTION.to_string(),
            path: wrapper.to_string_lossy().into_owned(),
            host_type: "stdio".to_string(),
            allowed_origins: vec![
                CHROME_EXTENSION_ID.to_string(),
                EDGE_EXTENSION_ID.to_string(),
            ],
        };
        let json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| io::Error::other(format!("JSON error: {}", e)))?;
        let path = dir.join(MANIFEST_FILENAME_CHROMIUM);
        std::fs::write(&path, json)?;
        Ok(path)
    }

    fn write_firefox_manifest(wrapper: &Path, dir: &Path) -> Result<PathBuf, io::Error> {
        std::fs::create_dir_all(dir)?;
        let manifest = NmhManifestFirefox {
            name: NMH_NAME.to_string(),
            description: NMH_DESCRIPTION.to_string(),
            path: wrapper.to_string_lossy().into_owned(),
            host_type: "stdio".to_string(),
            allowed_extensions: vec![FIREFOX_EXTENSION_ID.to_string()],
        };
        let json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| io::Error::other(format!("JSON error: {}", e)))?;
        let path = dir.join(MANIFEST_FILENAME_FIREFOX);
        std::fs::write(&path, json)?;
        Ok(path)
    }

    /// Proxy for "browser is installed": the NMH dir's parent is the browser's
    /// profile/config root (e.g. `~/.config/microsoft-edge`, `~/.mozilla`),
    /// which only exists once the browser has run at least once. Same heuristic
    /// as Bitwarden desktop. Scopes registration and self-heal to browsers actually
    /// present instead of spraying manifests into never-used dirs (#159).
    fn browser_installed(nmh_dir: &Path) -> bool {
        nmh_dir.parent().is_some_and(|p| p.is_dir())
    }

    /// True when any Firefox-family browser's profile root exists on disk.
    ///
    /// `~/.mozilla/native-messaging-hosts` is a compat dir some Firefox-fork
    /// builds (Zen, LibreWolf) read NMH manifests from instead of their own
    /// profile root (`~/.zen`, `~/.librewolf`), so it must count as
    /// "installed" whenever any fork is present, not only Firefox itself (#360).
    fn firefox_family_present(home: &Path) -> bool {
        let var_app = home.join(".var").join("app");
        home.join(".mozilla").is_dir()
            || home.join(".zen").is_dir()
            || home.join(".librewolf").is_dir()
            || var_app
                .join("org.mozilla.firefox")
                .join(".mozilla")
                .is_dir()
            || var_app
                .join("io.gitlab.librewolf-community")
                .join(".librewolf")
                .is_dir()
    }

    /// Firefox NMH dir "installed" check: the shared `.mozilla` compat dir
    /// falls back to [`firefox_family_present`]; every other fork dir keeps
    /// the plain profile-root check (#360).
    fn firefox_dir_installed(dir: &Path) -> bool {
        if browser_installed(dir) {
            return true;
        }
        let Some(home) = home_dir() else {
            return false;
        };
        let mozilla_compat = home.join(".mozilla").join("native-messaging-hosts");
        dir == mozilla_compat.as_path() && firefox_family_present(&home)
    }

    /// Human-readable browser name for an NMH manifest directory.
    ///
    /// The profile root (the NMH dir's parent) identifies the browser; Flatpak
    /// installs live under `~/.var/app/<app-id>/…` and Snap ones under
    /// `~/snap/<name>/…`, which the suffix keeps distinguishable.
    fn label_for_dir(dir: &Path) -> String {
        let flatpak = dir
            .components()
            .any(|c| c.as_os_str().to_str() == Some(".var"));
        let snap = !flatpak
            && dir
                .components()
                .any(|c| c.as_os_str().to_str() == Some("snap"));
        let root = dir
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let base = match root {
            "google-chrome" => "Chrome",
            "chromium" => "Chromium",
            "microsoft-edge" => "Edge",
            "Brave-Browser" => "Brave",
            "vivaldi" => "Vivaldi",
            "thorium" => "Thorium",
            "neo" => "Neo",
            ".mozilla" => "Firefox",
            ".librewolf" => "LibreWolf",
            ".zen" => "Zen Browser",
            "" => "Unknown browser",
            other => other,
        };
        if flatpak {
            format!("{} (Flatpak)", base)
        } else if snap {
            format!("{} (Snap)", base)
        } else {
            base.to_string()
        }
    }

    /// Read-only manifest check for one browser directory.
    fn diagnose_dir(
        dir: &Path,
        installed: bool,
        manifest_filename: &str,
        wrapper_str: &str,
        require_edge_origin: bool,
    ) -> NmhTarget {
        let manifest = dir.join(manifest_filename);
        let issue = super::manifest_issue(&manifest, wrapper_str, require_edge_origin);
        NmhTarget {
            label: label_for_dir(dir),
            location: manifest.to_string_lossy().into_owned(),
            installed,
            ok: issue.is_empty(),
            issue,
        }
    }

    /// Read-only snapshot of the NMH registration state for the Doctor page and
    /// startup self-heal. Never writes manifests, the wrapper script or any directory.
    pub fn diagnose() -> NmhDiagnosis {
        let mut diag = NmhDiagnosis::default();

        let chromium_dirs = chromium_nmh_dirs();
        let firefox_dirs = firefox_nmh_dirs();
        if let Some(first) = chromium_dirs.first() {
            diag.chromium_manifest = first
                .join(MANIFEST_FILENAME_CHROMIUM)
                .to_string_lossy()
                .into_owned();
        }
        if let Some(first) = firefox_dirs.first() {
            diag.firefox_manifest = first
                .join(MANIFEST_FILENAME_FIREFOX)
                .to_string_lossy()
                .into_owned();
        }

        let nmh_exe = match find_nmh_exe() {
            Ok(p) => p,
            Err(e) => {
                diag.exe_error = format!("{e:#}");
                return diag;
            }
        };
        diag.exe_path = nmh_exe.to_string_lossy().into_owned();

        // 无 HOME 时 wrapper 路径与上面两个目录列表同样为空，直接返回空快照。
        let Some(wp) = wrapper_path() else {
            return diag;
        };
        let wrapper_str = wp.to_string_lossy().into_owned();
        let (owner, relay) = super::diagnose_wrapper(&wp, &nmh_exe);
        diag.relay_owner = owner;
        diag.registered_relay = relay;
        diag.relay_location = wrapper_str.clone();

        for dir in &chromium_dirs {
            diag.targets.push(diagnose_dir(
                dir,
                browser_installed(dir),
                MANIFEST_FILENAME_CHROMIUM,
                &wrapper_str,
                true,
            ));
        }
        for dir in &firefox_dirs {
            diag.targets.push(diagnose_dir(
                dir,
                firefox_dir_installed(dir),
                MANIFEST_FILENAME_FIREFOX,
                &wrapper_str,
                false,
            ));
        }

        diag
    }

    /// Point the wrapper script at `relay` and write manifests for installed browsers.
    pub(super) fn register_with(relay: &Path) -> Result<(), io::Error> {
        // Write wrapper script first; manifests point to it.
        let wrapper = write_wrapper_script(relay)?;
        log_info!("[nmh_registry] NMH wrapper script: {}", wrapper.display());

        for dir in chromium_nmh_dirs() {
            if !browser_installed(&dir) {
                // 未安装（profile 根不存在）的浏览器不写清单，
                // 避免凭空创建其 profile / Flatpak / Snap 目录（#159）。
                continue;
            }
            match write_chromium_manifest(&wrapper, &dir) {
                Ok(path) => {
                    log_info!("[nmh_registry] Chromium manifest: {}", path.display());
                }
                Err(e) => {
                    log_info!(
                        "[nmh_registry] Chromium manifest error ({}): {}",
                        dir.display(),
                        e
                    );
                }
            }
        }

        for dir in firefox_nmh_dirs() {
            if !firefox_dir_installed(&dir) {
                continue;
            }
            match write_firefox_manifest(&wrapper, &dir) {
                Ok(path) => {
                    log_info!("[nmh_registry] Firefox manifest: {}", path.display());
                }
                Err(e) => {
                    log_info!(
                        "[nmh_registry] Firefox manifest error ({}): {}",
                        dir.display(),
                        e
                    );
                }
            }
        }

        log_info!(
            "[nmh_registry] NMH registered: relay={}, wrapper={}",
            relay.display(),
            wrapper.display()
        );
        Ok(())
    }

    #[allow(dead_code)]
    pub fn unregister() -> Result<(), io::Error> {
        for dir in chromium_nmh_dirs() {
            let _ = std::fs::remove_file(dir.join(MANIFEST_FILENAME_CHROMIUM));
        }
        for dir in firefox_nmh_dirs() {
            let _ = std::fs::remove_file(dir.join(MANIFEST_FILENAME_FIREFOX));
        }
        if let Some(wp) = wrapper_path() {
            let _ = std::fs::remove_file(wp);
        }
        log_info!("[nmh_registry] NMH registration removed");
        Ok(())
    }

    #[cfg(test)]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    mod tests {
        use super::firefox_family_present;
        use std::path::PathBuf;

        fn unique_test_home(tag: &str) -> PathBuf {
            std::env::temp_dir().join(format!(
                "fluxdown_nmh_test_{tag}_{}_{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or_default()
            ))
        }

        /// Zen-only 主机（没有 ~/.mozilla，只有 ~/.zen）也要判定为 Firefox 家族
        /// 已安装，否则 `.mozilla` 兼容目录永远拿不到清单，Zen 读取的正是
        /// 这个兼容目录（#360）。
        #[test]
        fn zen_only_home_counts_as_firefox_family_present() {
            let home = unique_test_home("zen_only");
            std::fs::create_dir_all(home.join(".zen")).expect("create .zen");
            assert!(!home.join(".mozilla").is_dir());
            assert!(firefox_family_present(&home));
            let _ = std::fs::remove_dir_all(&home);
        }

        #[test]
        fn empty_home_has_no_firefox_family() {
            let home = unique_test_home("empty");
            std::fs::create_dir_all(&home).expect("create home");
            assert!(!firefox_family_present(&home));
            let _ = std::fs::remove_dir_all(&home);
        }
    }
}

// macOS: write NMH manifest files to ~/Library/Application Support browser directories.
#[cfg(target_os = "macos")]
mod inner {
    use super::{EDGE_EXTENSION_ID, NmhDiagnosis, NmhTarget};
    use crate::logger::log_info;
    use serde::Serialize;
    use std::io;
    use std::path::{Path, PathBuf};

    const NMH_NAME: &str = "com.fluxdown.nmh";
    const NMH_DESCRIPTION: &str = "FluxDown Native Messaging Host";
    const NMH_EXE_NAME: &str = "fluxdown_nmh";
    /// Shell wrapper script name registered in NMH manifest.
    /// Chrome/Firefox spawn this shell (a system-signed binary) which then
    /// exec's the actual fluxdown_nmh binary, bypassing macOS AMFI's
    /// requirement that processes spawned by Hardened-Runtime apps must
    /// carry a trusted Developer ID signature (adhoc-only binaries are
    /// rejected with "Unrecoverable CT signature issue").
    const NMH_WRAPPER_NAME: &str = "fluxdown_nmh.sh";
    const MANIFEST_FILENAME: &str = "com.fluxdown.nmh.json";
    const CHROME_EXTENSION_ID: &str = "chrome-extension://meleenglfggcmcajknpeeeiobnpfmahc/";
    const FIREFOX_EXTENSION_ID: &str = "fluxdown@fluxdown.app";

    #[derive(Serialize)]
    struct NmhManifestChromium {
        name: String,
        description: String,
        path: String,
        #[serde(rename = "type")]
        host_type: String,
        allowed_origins: Vec<String>,
    }

    #[derive(Serialize)]
    struct NmhManifestFirefox {
        name: String,
        description: String,
        path: String,
        #[serde(rename = "type")]
        host_type: String,
        allowed_extensions: Vec<String>,
    }

    /// Returns the current user's home directory.
    ///
    /// Prefers `$HOME` but falls back to the passwd database via `getpwuid_r`
    /// so that the correct path is returned even when the process is launched
    /// by a system service (launchd) that may not set `$HOME`.
    fn home_dir() -> Option<PathBuf> {
        if let Ok(h) = std::env::var("HOME")
            && !h.is_empty()
        {
            return Some(PathBuf::from(h));
        }
        use std::ffi::CStr;
        let uid = unsafe { libc::getuid() };
        let buf_size = unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) };
        let buf_size = if buf_size > 0 {
            buf_size as usize
        } else {
            1024
        };
        let mut buf = vec![0i8; buf_size];
        let mut pwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        let ret = unsafe {
            libc::getpwuid_r(
                uid,
                pwd.as_mut_ptr(),
                buf.as_mut_ptr(),
                buf_size,
                &mut result,
            )
        };
        if ret == 0 && !result.is_null() {
            let pwd = unsafe { pwd.assume_init() };
            if !pwd.pw_dir.is_null() {
                let cstr = unsafe { CStr::from_ptr(pwd.pw_dir) };
                if let Ok(s) = cstr.to_str()
                    && !s.is_empty()
                {
                    return Some(PathBuf::from(s));
                }
            }
        }
        None
    }

    /// macOS Chromium-family NMH manifest directories.
    /// Ref: https://developer.chrome.com/docs/apps/nativeMessaging/#native-messaging-host-location-macos
    fn chromium_nmh_dirs() -> Vec<PathBuf> {
        let Some(home) = home_dir() else {
            return vec![];
        };
        let lib = home.join("Library").join("Application Support");
        vec![
            // Google Chrome (stable / beta / canary)
            lib.join("Google")
                .join("Chrome")
                .join("NativeMessagingHosts"),
            lib.join("Google")
                .join("Chrome Beta")
                .join("NativeMessagingHosts"),
            lib.join("Google")
                .join("Chrome Canary")
                .join("NativeMessagingHosts"),
            // Open-source Chromium
            lib.join("Chromium").join("NativeMessagingHosts"),
            // Microsoft Edge (stable / beta)
            lib.join("Microsoft Edge").join("NativeMessagingHosts"),
            lib.join("Microsoft Edge Beta").join("NativeMessagingHosts"),
            // Arc
            lib.join("Arc")
                .join("User Data")
                .join("NativeMessagingHosts"),
            // Brave Browser (verified via KeePassXC source)
            lib.join("BraveSoftware")
                .join("Brave-Browser")
                .join("NativeMessagingHosts"),
            // Vivaldi (verified via KeePassXC source)
            lib.join("Vivaldi").join("NativeMessagingHosts"),
            // Thorium (Chromium fork; user-reported, #360)
            lib.join("Thorium").join("NativeMessagingHosts"),
            // Norton Neo (Chromium-based AI browser; user-reported, #623/#653).
            // Confirmed via /Library/Application Support/Neo/NativeMessagingHosts
            // embedded in the official macOS .pkg (Info.plist CFBundleName = "Neo").
            lib.join("Neo").join("NativeMessagingHosts"),
        ]
    }

    /// macOS Firefox NMH manifest directory.
    fn firefox_nmh_dir() -> Option<PathBuf> {
        home_dir().map(|h| {
            h.join("Library")
                .join("Application Support")
                .join("Mozilla")
                .join("NativeMessagingHosts")
        })
    }

    pub(super) fn find_nmh_exe() -> Result<PathBuf, io::Error> {
        // 1. Next to current exe (production: inside .app bundle Contents/MacOS/)
        if let Ok(exe) = std::env::current_exe() {
            let canonical = std::fs::canonicalize(&exe).unwrap_or(exe);
            if let Some(dir) = canonical.parent() {
                let candidate = dir.join(NMH_EXE_NAME);
                if candidate.exists() {
                    log_info!(
                        "[nmh_registry] found NMH exe next to app: {}",
                        candidate.display()
                    );
                    return Ok(candidate);
                }
            }
        }

        // 2. Cargo workspace target directory (development)
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let workspace_root = Path::new(manifest_dir).parent().and_then(|p| p.parent());

        if let Some(ws) = workspace_root {
            for profile in &["debug", "release"] {
                let candidate = ws.join("target").join(profile).join(NMH_EXE_NAME);
                if candidate.exists() {
                    log_info!(
                        "[nmh_registry] found NMH exe in cargo target: {}",
                        candidate.display()
                    );
                    return Ok(candidate);
                }
            }
        }

        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "{} not found. Build it with: cargo build -p fluxdown_nmh",
                NMH_EXE_NAME
            ),
        ))
    }

    /// Write a shell wrapper script that exec's the real NMH binary.
    ///
    /// macOS AMFI rejects adhoc-signed (non-Developer-ID) binaries when they
    /// are spawned by Hardened Runtime processes such as Chrome or Firefox.
    /// `/bin/sh` is a system binary with an Apple-signed certificate and is
    /// always permitted. By registering the *shell script* as the NMH path,
    /// the browser spawns `/bin/sh`, which in turn exec's `fluxdown_nmh`.
    /// The shell inherits the NMH stdin/stdout pipe and transparently relays
    /// it to the binary — zero overhead, no extra process.
    fn write_wrapper_script(relay: &Path) -> Result<PathBuf, io::Error> {
        let Some(home) = home_dir() else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "cannot determine home directory",
            ));
        };
        let dir = home
            .join("Library")
            .join("Application Support")
            .join("fluxdown");
        std::fs::create_dir_all(&dir)?;
        let script_path = dir.join(NMH_WRAPPER_NAME);
        // `exec` replaces the shell with the relay (no extra process); "$@"
        // forwards any arguments Chrome may add.
        let script = super::wrapper_script(relay);
        std::fs::write(&script_path, script)?;
        // The script must be executable.
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))?;
        Ok(script_path)
    }

    fn write_chromium_manifest(wrapper: &Path, dir: &Path) -> Result<PathBuf, io::Error> {
        std::fs::create_dir_all(dir)?;
        let manifest = NmhManifestChromium {
            name: NMH_NAME.to_string(),
            description: NMH_DESCRIPTION.to_string(),
            path: wrapper.to_string_lossy().into_owned(),
            host_type: "stdio".to_string(),
            allowed_origins: vec![
                CHROME_EXTENSION_ID.to_string(),
                EDGE_EXTENSION_ID.to_string(),
            ],
        };
        let json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| io::Error::other(format!("JSON error: {}", e)))?;
        let path = dir.join(MANIFEST_FILENAME);
        std::fs::write(&path, json)?;
        Ok(path)
    }

    fn write_firefox_manifest(wrapper: &Path, dir: &Path) -> Result<PathBuf, io::Error> {
        std::fs::create_dir_all(dir)?;
        let manifest = NmhManifestFirefox {
            name: NMH_NAME.to_string(),
            description: NMH_DESCRIPTION.to_string(),
            path: wrapper.to_string_lossy().into_owned(),
            host_type: "stdio".to_string(),
            allowed_extensions: vec![FIREFOX_EXTENSION_ID.to_string()],
        };
        let json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| io::Error::other(format!("JSON error: {}", e)))?;
        let path = dir.join(MANIFEST_FILENAME);
        std::fs::write(&path, json)?;
        Ok(path)
    }

    /// Proxy for "browser is installed": the NMH dir's parent is the browser's
    /// profile/user-data root (e.g. `~/Library/Application Support/Microsoft Edge`),
    /// which only exists once the browser has run at least once. Same heuristic
    /// as Bitwarden desktop. Scopes registration and self-heal to browsers actually
    /// present instead of spraying manifests into never-used dirs (#159).
    fn browser_installed(nmh_dir: &Path) -> bool {
        nmh_dir.parent().is_some_and(|p| p.is_dir())
    }

    /// Firefox reads NMH manifests from `…/Mozilla/NativeMessagingHosts`, but its
    /// actual profile root is `…/Application Support/Firefox` — the `Mozilla` dir
    /// is not guaranteed to exist on a machine with Firefox installed. Treat
    /// either directory as evidence of an install.
    fn firefox_installed() -> bool {
        home_dir().is_some_and(|h| {
            let lib = h.join("Library").join("Application Support");
            lib.join("Firefox").is_dir() || lib.join("Mozilla").is_dir()
        })
    }

    /// `~/Library/Application Support/fluxdown/fluxdown_nmh.sh`.
    fn wrapper_path() -> Option<PathBuf> {
        home_dir().map(|h| {
            h.join("Library")
                .join("Application Support")
                .join("fluxdown")
                .join(NMH_WRAPPER_NAME)
        })
    }

    /// Human-readable browser name for an NMH manifest directory.
    ///
    /// The profile root (the NMH dir's parent) identifies the browser, except
    /// for Arc which nests its manifests under `Arc/User Data/`.
    fn label_for_dir(dir: &Path) -> String {
        // 闭包返回借用自参数的 &str 会触发生命周期推断报错，用具名 fn。
        fn dir_name(p: Option<&Path>) -> &str {
            p.and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or_default()
        }
        let parent = dir.parent();
        let mut root = dir_name(parent);
        if root == "User Data" {
            root = dir_name(parent.and_then(|p| p.parent()));
        }
        match root {
            "Chrome" => "Chrome",
            "Chrome Beta" => "Chrome Beta",
            "Chrome Canary" => "Chrome Canary",
            "Chromium" => "Chromium",
            "Microsoft Edge" => "Edge",
            "Microsoft Edge Beta" => "Edge Beta",
            "Arc" => "Arc",
            "Brave-Browser" => "Brave",
            "Vivaldi" => "Vivaldi",
            "Thorium" => "Thorium",
            "Neo" => "Neo",
            "Mozilla" => "Firefox",
            "" => "Unknown browser",
            other => other,
        }
        .to_string()
    }

    /// Read-only manifest check for one browser directory.
    fn diagnose_dir(
        dir: &Path,
        installed: bool,
        wrapper_str: &str,
        require_edge_origin: bool,
    ) -> NmhTarget {
        let manifest = dir.join(MANIFEST_FILENAME);
        let issue = super::manifest_issue(&manifest, wrapper_str, require_edge_origin);
        NmhTarget {
            label: label_for_dir(dir),
            location: manifest.to_string_lossy().into_owned(),
            installed,
            ok: issue.is_empty(),
            issue,
        }
    }

    /// Read-only snapshot of the NMH registration state for the Doctor page and
    /// startup self-heal. Never writes manifests, the wrapper script or any directory.
    pub fn diagnose() -> NmhDiagnosis {
        let mut diag = NmhDiagnosis::default();

        let chromium_dirs = chromium_nmh_dirs();
        let firefox_dir = firefox_nmh_dir();
        if let Some(first) = chromium_dirs.first() {
            diag.chromium_manifest = first.join(MANIFEST_FILENAME).to_string_lossy().into_owned();
        }
        if let Some(dir) = &firefox_dir {
            diag.firefox_manifest = dir.join(MANIFEST_FILENAME).to_string_lossy().into_owned();
        }

        let nmh_exe = match find_nmh_exe() {
            Ok(p) => p,
            Err(e) => {
                diag.exe_error = format!("{e:#}");
                return diag;
            }
        };
        diag.exe_path = nmh_exe.to_string_lossy().into_owned();

        // 无 home 时上面的目录列表同样为空，直接返回空快照。
        let Some(wp) = wrapper_path() else {
            return diag;
        };
        let wrapper_str = wp.to_string_lossy().into_owned();
        let (owner, relay) = super::diagnose_wrapper(&wp, &nmh_exe);
        diag.relay_owner = owner;
        diag.registered_relay = relay;
        diag.relay_location = wrapper_str.clone();

        for dir in &chromium_dirs {
            diag.targets.push(diagnose_dir(
                dir,
                browser_installed(dir),
                &wrapper_str,
                true,
            ));
        }
        if let Some(dir) = &firefox_dir {
            diag.targets
                .push(diagnose_dir(dir, firefox_installed(), &wrapper_str, false));
        }

        diag
    }

    /// Point the wrapper script at `relay` and write manifests for installed browsers.
    pub(super) fn register_with(relay: &Path) -> Result<(), io::Error> {
        // Write the shell wrapper script first; manifests point to it.
        let wrapper = write_wrapper_script(relay)?;
        log_info!("[nmh_registry] NMH wrapper script: {}", wrapper.display());

        for dir in chromium_nmh_dirs() {
            if !browser_installed(&dir) {
                // 未安装（profile 根不存在）的浏览器不写清单，
                // 避免凭空创建其 profile 目录（#159 修复建议）。
                continue;
            }
            match write_chromium_manifest(&wrapper, &dir) {
                Ok(path) => {
                    log_info!("[nmh_registry] Chromium manifest: {}", path.display());
                }
                Err(e) => {
                    log_info!(
                        "[nmh_registry] Chromium manifest error ({}): {}",
                        dir.display(),
                        e
                    );
                }
            }
        }

        if firefox_installed()
            && let Some(dir) = firefox_nmh_dir()
        {
            match write_firefox_manifest(&wrapper, &dir) {
                Ok(path) => {
                    log_info!("[nmh_registry] Firefox manifest: {}", path.display());
                }
                Err(e) => {
                    log_info!("[nmh_registry] Firefox manifest error: {}", e);
                }
            }
        }

        log_info!(
            "[nmh_registry] NMH registered: relay={}, wrapper={}",
            relay.display(),
            wrapper.display()
        );
        Ok(())
    }

    #[allow(dead_code)]
    pub fn unregister() -> Result<(), io::Error> {
        for dir in chromium_nmh_dirs() {
            let _ = std::fs::remove_file(dir.join(MANIFEST_FILENAME));
        }
        if let Some(dir) = firefox_nmh_dir() {
            let _ = std::fs::remove_file(dir.join(MANIFEST_FILENAME));
        }
        // Remove wrapper script.
        if let Some(home) = home_dir() {
            let wrapper = home
                .join("Library")
                .join("Application Support")
                .join("fluxdown")
                .join(NMH_WRAPPER_NAME);
            let _ = std::fs::remove_file(wrapper);
        }
        log_info!("[nmh_registry] NMH registration removed");
        Ok(())
    }
}

// All other non-Windows, non-Linux, non-macOS platforms — no-op.
#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
mod inner {
    use std::io;
    use std::path::{Path, PathBuf};

    pub(super) fn find_nmh_exe() -> Result<PathBuf, io::Error> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "unsupported platform",
        ))
    }

    pub fn diagnose() -> super::NmhDiagnosis {
        super::NmhDiagnosis {
            exe_error: "unsupported platform".into(),
            ..super::NmhDiagnosis::default()
        }
    }

    pub(super) fn register_with(_relay: &Path) -> Result<(), io::Error> {
        Ok(())
    }

    #[allow(dead_code)]
    pub fn unregister() -> Result<(), io::Error> {
        Ok(())
    }
}

#[allow(unused_imports)]
pub use inner::{diagnose, unregister};
