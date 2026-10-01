//! Doctor 动态探测（`daemon.diagnostics.probe`）：在实际执行下载的进程里真实写入各下载目录
//! 与数据目录、真实运行外部组件，验证下载链路拥有的权限，而不是只看配置。
//!
//! 只有真实操作能发现的失败：macOS 隐私授权（TCC）拒绝写「下载」「桌面」「文稿」、只读挂载、
//! NAS 容器 uid 与共享目录属主不符、网络盘失联、组件缺执行位 / 带隔离属性 / 被安全软件拦截。
//!
//! 写入探测在目标目录（不存在时为最近的已存在上级，下载时会自动创建该目录）里创建一个随机
//! 命名的临时文件，依次「创建 → 写入 → 落盘 → 读回校验 → 重命名 → 删除」，覆盖下载链路用到
//! 的全部文件操作；失败时尽力清理。探测只由用户在 Doctor 里手动触发，不是周期任务，
//! 不影响空闲静默。

use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use fluxdown_protocol::{
    ComponentProbeDto, StorageProbeDto, StorageProbeFailure, StorageProbeRole, StorageProbeTarget,
};

/// 组件路径解析结果：`(名称, 版本参数, 生效路径)`。
pub(crate) type ComponentPath = (&'static str, &'static str, PathBuf);

/// 单个目录的探测时限；失联的网络挂载会让文件调用长时间阻塞。
const DIR_PROBE_TIMEOUT: Duration = Duration::from_secs(10);
/// 组件运行时限；yt-dlp 单文件版首次运行要解包，安全软件扫描也会拖慢启动。
const COMPONENT_PROBE_TIMEOUT: Duration = Duration::from_secs(20);
/// 写入探测的数据量：足以穿过写缓存与配额检查，又不会在慢盘上拖慢 Doctor。
const PROBE_PAYLOAD_BYTES: usize = 64 * 1024;
const PROBE_PAYLOAD_BYTE: u8 = 0x5a;

/// 按路径去重（忽略首尾空白与末尾分隔符），保留首次出现的目标；空路径丢弃。
pub(crate) fn dedupe_targets(targets: Vec<StorageProbeTarget>) -> Vec<StorageProbeTarget> {
    let mut seen: Vec<String> = Vec::with_capacity(targets.len());
    targets
        .into_iter()
        .filter_map(|mut target| {
            let trimmed = target.path.trim();
            let key = trimmed.trim_end_matches(['/', '\\']);
            // 根目录（`/`、`C:\`）去掉分隔符后会变空或丢失根语义，按原样比较。
            let key = if key.is_empty() || key.ends_with(':') {
                trimmed
            } else {
                key
            };
            if key.is_empty() || seen.iter().any(|existing| existing == key) {
                return None;
            }
            seen.push(key.to_owned());
            target.path = trimmed.to_owned();
            Some(target)
        })
        .collect()
}

/// 并发探测全部目录，结果顺序与输入一致。
pub(crate) async fn probe_storage(targets: Vec<StorageProbeTarget>) -> Vec<StorageProbeDto> {
    futures_util::future::join_all(targets.into_iter().map(probe_target)).await
}

async fn probe_target(target: StorageProbeTarget) -> StorageProbeDto {
    let path = PathBuf::from(&target.path);
    let task = tokio::task::spawn_blocking(move || probe_dir(&path));
    let outcome = match tokio::time::timeout(DIR_PROBE_TIMEOUT, task).await {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(error)) => DirOutcome::failed(
            true,
            String::new(),
            "",
            StorageProbeFailure::Other,
            format!("probe task failed: {error}"),
        ),
        // 阻塞线程无法取消；它会在文件调用返回后自行结束并清理临时文件。
        Err(_) => DirOutcome::failed(
            true,
            String::new(),
            "",
            StorageProbeFailure::Timeout,
            format!("no response within {}s", DIR_PROBE_TIMEOUT.as_secs()),
        ),
    };
    StorageProbeDto {
        role: target.role,
        label: target.label,
        path: target.path,
        exists: outcome.exists,
        probed_dir: outcome.probed_dir,
        failed_step: outcome.failed_step.to_owned(),
        failure: outcome.failure,
        error: outcome.error,
        available_bytes: outcome.available_bytes,
        os_error: outcome.os_error,
    }
}

#[derive(Debug)]
struct DirOutcome {
    exists: bool,
    probed_dir: String,
    failed_step: &'static str,
    failure: Option<StorageProbeFailure>,
    error: String,
    available_bytes: Option<u64>,
    os_error: Option<i32>,
}

impl DirOutcome {
    fn failed(
        exists: bool,
        probed_dir: String,
        step: &'static str,
        failure: StorageProbeFailure,
        error: String,
    ) -> Self {
        Self {
            exists,
            probed_dir,
            failed_step: step,
            failure: Some(failure),
            error,
            available_bytes: None,
            os_error: None,
        }
    }
}

/// 阻塞探测一个目录：定位探测目录 → 写入往返 → 查询可用空间。
fn probe_dir(path: &Path) -> DirOutcome {
    let exists = path.exists();
    let Some(dir) = nearest_existing(path) else {
        return DirOutcome::failed(
            false,
            String::new(),
            "stat",
            StorageProbeFailure::Missing,
            "neither the directory nor any parent exists".to_owned(),
        );
    };
    let probed_dir = dir.display().to_string();
    if !dir.is_dir() {
        return DirOutcome::failed(
            exists,
            probed_dir,
            "stat",
            StorageProbeFailure::NotDirectory,
            format!("{} is not a directory", dir.display()),
        );
    }
    let available_bytes = fluxdown_engine::disk_space::available_space(dir);
    match write_round_trip(dir) {
        Ok(()) => DirOutcome {
            exists,
            probed_dir,
            failed_step: "",
            failure: None,
            error: String::new(),
            available_bytes,
            os_error: None,
        },
        // 目标不存在、在挂载容器里写入又被拒：卷没有挂载，而不是权限问题。容器本身可写
        // （如 Docker 把共享目录挂在 `/media/<name>`）时探测已经通过，不会走到这里。
        Err((_, error))
            if !exists
                && is_mount_container(dir)
                && matches!(
                    classify(&error),
                    StorageProbeFailure::PermissionDenied | StorageProbeFailure::ReadOnly
                ) =>
        {
            DirOutcome::failed(
                false,
                probed_dir,
                "stat",
                StorageProbeFailure::Missing,
                format!(
                    "the volume under {} is not mounted ({error})",
                    dir.display()
                ),
            )
        }
        Err((step, error)) => DirOutcome {
            exists,
            probed_dir,
            failed_step: step,
            failure: Some(classify(&error)),
            error: error.to_string(),
            available_bytes,
            os_error: error.raw_os_error(),
        },
    }
}

/// 路径本身或最近的已存在上级。
fn nearest_existing(path: &Path) -> Option<&Path> {
    path.ancestors()
        .find(|candidate| !candidate.as_os_str().is_empty() && candidate.exists())
}

/// 外置卷的挂载容器（macOS `/Volumes`、Linux `/media[/<user>]`、`/run/media[/<user>]`、`/mnt`）。
/// 目标不存在而最近的已存在上级恰是这些目录时，说明目标所在的卷没有挂载。
#[cfg(unix)]
fn is_mount_container(dir: &Path) -> bool {
    const CONTAINERS: [&str; 4] = ["/Volumes", "/media", "/run/media", "/mnt"];
    const PER_USER: [&str; 2] = ["/media", "/run/media"];
    CONTAINERS.iter().any(|root| dir == Path::new(root))
        || dir
            .parent()
            .is_some_and(|parent| PER_USER.iter().any(|root| parent == Path::new(root)))
}

/// Windows 卷以盘符出现：盘符不存在时 [`nearest_existing`] 已返回 `None`。
#[cfg(not(unix))]
fn is_mount_container(_dir: &Path) -> bool {
    false
}

/// 下载链路的全部文件操作走一遍；任一步失败返回 `(步骤, 错误)` 并清理临时文件。
fn write_round_trip(dir: &Path) -> Result<(), (&'static str, std::io::Error)> {
    let name = format!(
        ".fluxdown-doctor-{}-{}.tmp",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    );
    let staged = dir.join(&name);
    let renamed = dir.join(format!("{name}.done"));
    let result = round_trip_steps(&staged, &renamed);
    if result.is_err() {
        // 失败发生在哪一步不定：两个名字都可能不存在，只清理存在的那个。
        remove_probe_file(&staged);
        remove_probe_file(&renamed);
    }
    result
}

/// 尽力删除探测临时文件；文件本就不存在不算失败，其余失败只记日志（探测结论不受影响）。
fn remove_probe_file(path: &Path) {
    if let Err(error) = std::fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(path = %path.display(), error = %error, "could not remove doctor probe file");
    }
}

fn round_trip_steps(staged: &Path, renamed: &Path) -> Result<(), (&'static str, std::io::Error)> {
    let payload = [PROBE_PAYLOAD_BYTE; PROBE_PAYLOAD_BYTES];
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(staged)
        .map_err(|error| ("create", error))?;
    file.write_all(&payload).map_err(|error| ("write", error))?;
    file.sync_all().map_err(|error| ("sync", error))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|error| ("read", error))?;
    let mut read_back = Vec::with_capacity(PROBE_PAYLOAD_BYTES);
    file.read_to_end(&mut read_back)
        .map_err(|error| ("read", error))?;
    drop(file);
    if read_back.as_slice() != payload.as_slice() {
        return Err((
            "read",
            std::io::Error::other(format!(
                "read back {} bytes that differ from the {PROBE_PAYLOAD_BYTES} bytes written",
                read_back.len()
            )),
        ));
    }
    std::fs::rename(staged, renamed).map_err(|error| ("rename", error))?;
    std::fs::remove_file(renamed).map_err(|error| ("remove", error))
}

fn classify(error: &std::io::Error) -> StorageProbeFailure {
    use std::io::ErrorKind;
    match error.kind() {
        ErrorKind::PermissionDenied => StorageProbeFailure::PermissionDenied,
        ErrorKind::ReadOnlyFilesystem => StorageProbeFailure::ReadOnly,
        ErrorKind::StorageFull | ErrorKind::QuotaExceeded => StorageProbeFailure::NoSpace,
        ErrorKind::NotFound => StorageProbeFailure::Missing,
        ErrorKind::NotADirectory => StorageProbeFailure::NotDirectory,
        _ => StorageProbeFailure::Other,
    }
}

/// 外部组件的生效路径（与下载链路同一解析顺序：手动 → 托管 → 系统 PATH）。
pub(crate) async fn component_paths(
    db: &fluxdown_engine::db::Db,
    data_dir: &Path,
) -> Vec<ComponentPath> {
    use fluxdown_engine::components::{resolve_ffmpeg, resolve_ffprobe, resolve_ytdlp};
    let mut paths = Vec::with_capacity(3);
    if let Some(path) = resolve_ffmpeg(db, data_dir).await {
        paths.push(("ffmpeg", "-version", path));
    }
    if let Some(path) = resolve_ffprobe(db, data_dir).await {
        paths.push(("ffprobe", "-version", path));
    }
    if let Some(path) = resolve_ytdlp(db, data_dir).await {
        paths.push(("yt-dlp", "--version", path));
    }
    paths
}

/// 并发运行每个组件的版本命令。
pub(crate) async fn probe_components(
    components: Vec<ComponentPath>,
    data_dir: &Path,
) -> Vec<ComponentProbeDto> {
    futures_util::future::join_all(
        components
            .into_iter()
            .map(|component| probe_component(component, data_dir)),
    )
    .await
}

async fn probe_component((name, arg, path): ComponentPath, data_dir: &Path) -> ComponentProbeDto {
    let result =
        fluxdown_engine::components::exec_probe(&path, arg, Some(COMPONENT_PROBE_TIMEOUT)).await;
    let (output, error, permission_denied) = match result {
        Ok(output) => (output, String::new(), false),
        Err(error) => (String::new(), error.to_string(), error.permission_denied()),
    };
    ComponentProbeDto {
        name: name.to_owned(),
        managed: is_within(&path, data_dir),
        path: path.display().to_string(),
        output,
        error,
        permission_denied,
    }
}

/// `path` 解析符号链接后位于 `root` 之内（托管组件判定）。任一方无法解析时按不在内处理。
fn is_within(path: &Path, root: &Path) -> bool {
    match (std::fs::canonicalize(path), std::fs::canonicalize(root)) {
        (Ok(path), Ok(root)) => path != root && path.starts_with(&root),
        _ => false,
    }
}

/// [`fix_component`] 的失败原因。
#[derive(Debug, thiserror::Error)]
pub(crate) enum FixComponentError {
    /// 没有解析到该名称的组件。
    #[error("component not found: {0}")]
    NotFound(String),
    /// 生效路径不在 daemon 数据目录内（手动指定 / 系统 PATH），不替用户改别处的文件。
    #[error("component is not a managed install: {0}")]
    NotManaged(String),
    #[error("could not update permissions: {0}")]
    Io(#[from] std::io::Error),
}

/// 补上托管组件缺失的执行权限（只改 daemon 数据目录内、属于当前用户的文件），再重新探测。
/// noexec 挂载、安全软件拦截等改权限解决不了的情况由重新探测的结果如实反映。
pub(crate) async fn fix_component(
    components: Vec<ComponentPath>,
    data_dir: &Path,
    name: &str,
) -> Result<ComponentProbeDto, FixComponentError> {
    let component = components
        .into_iter()
        .find(|(candidate, _, _)| *candidate == name)
        .ok_or_else(|| FixComponentError::NotFound(name.to_owned()))?;
    if !is_within(&component.2, data_dir) {
        return Err(FixComponentError::NotManaged(
            component.2.display().to_string(),
        ));
    }
    let path = component.2.clone();
    let changed = tokio::task::spawn_blocking(move || ensure_executable(&path))
        .await
        .map_err(std::io::Error::other)??;
    if changed {
        tracing::info!(component = name, path = %component.2.display(), "restored execute permission");
    }
    Ok(probe_component(component, data_dir).await)
}

/// 所有者读 + 执行位与组 / 其他人的执行位都补齐；已具备时不写，返回是否改动。
#[cfg(unix)]
fn ensure_executable(path: &Path) -> std::io::Result<bool> {
    use std::os::unix::fs::PermissionsExt;

    let mode = std::fs::metadata(path)?.permissions().mode() & 0o7777;
    let wanted = mode | 0o511;
    if wanted == mode {
        return Ok(false);
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(wanted))?;
    Ok(true)
}

/// Windows 没有执行位：启动被拒来自 ACL / 安全软件，不在这里改动。
#[cfg(not(unix))]
fn ensure_executable(_path: &Path) -> std::io::Result<bool> {
    Ok(false)
}

/// daemon 自己持有的探测目录：默认保存目录、队列与 RSS 源的自定义目录、数据目录。
pub(crate) fn daemon_targets(
    snapshot: &fluxdown_protocol::DaemonSnapshot,
    data_dir: &Path,
) -> Vec<StorageProbeTarget> {
    let default_save_dir = snapshot
        .config
        .values
        .get("default_save_dir")
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(fluxdown_engine::user_dirs::download_dir_or_cwd);
    let mut targets = vec![StorageProbeTarget {
        role: StorageProbeRole::DefaultSaveDir,
        label: String::new(),
        path: default_save_dir,
    }];
    targets.extend(
        snapshot
            .queues
            .iter()
            .filter(|queue| !queue.default_save_dir.trim().is_empty())
            .map(|queue| StorageProbeTarget {
                role: StorageProbeRole::Queue,
                label: queue.name.clone(),
                path: queue.default_save_dir.clone(),
            }),
    );
    targets.extend(
        snapshot
            .rss_sources
            .iter()
            .filter(|source| source.enabled && !source.save_dir.trim().is_empty())
            .map(|source| StorageProbeTarget {
                role: StorageProbeRole::Rss,
                label: source.name.clone(),
                path: source.save_dir.clone(),
            }),
    );
    targets.push(StorageProbeTarget {
        role: StorageProbeRole::DataDir,
        label: String::new(),
        path: data_dir.display().to_string(),
    });
    targets
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use fluxdown_protocol::{StorageProbeFailure, StorageProbeRole, StorageProbeTarget};

    use super::{dedupe_targets, probe_dir};

    fn target(role: StorageProbeRole, path: &str) -> StorageProbeTarget {
        StorageProbeTarget {
            role,
            label: String::new(),
            path: path.to_owned(),
        }
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fluxdown-doctor-probe-{name}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entries(dir: &Path) -> usize {
        std::fs::read_dir(dir).unwrap().count()
    }

    #[test]
    fn dedupe_keeps_first_role_and_ignores_trailing_separators() {
        let targets = dedupe_targets(vec![
            target(StorageProbeRole::DefaultSaveDir, "/data/dl"),
            target(StorageProbeRole::Queue, "/data/dl/"),
            target(StorageProbeRole::Category, "  "),
            target(StorageProbeRole::Rss, " /data/rss "),
            target(StorageProbeRole::Category, "/"),
            target(StorageProbeRole::Category, "/"),
        ]);
        let kept: Vec<_> = targets
            .iter()
            .map(|target| (target.role, target.path.as_str()))
            .collect();
        assert_eq!(
            kept,
            vec![
                (StorageProbeRole::DefaultSaveDir, "/data/dl"),
                (StorageProbeRole::Rss, "/data/rss"),
                (StorageProbeRole::Category, "/"),
            ]
        );
    }

    #[test]
    fn writable_directory_passes_and_leaves_nothing_behind() {
        let dir = temp_dir("ok");
        let outcome = probe_dir(&dir);
        assert!(outcome.exists);
        assert_eq!(outcome.failure, None, "{}", outcome.error);
        assert!(outcome.available_bytes.is_some_and(|bytes| bytes > 0));
        assert_eq!(entries(&dir), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_directory_is_probed_on_nearest_existing_parent() {
        let dir = temp_dir("missing");
        let target = dir.join("a").join("b");
        let outcome = probe_dir(&target);
        assert!(!outcome.exists);
        assert_eq!(outcome.failure, None, "{}", outcome.error);
        assert_eq!(outcome.probed_dir, dir.display().to_string());
        assert!(!dir.join("a").exists(), "probe must not create the target");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn file_in_place_of_directory_is_reported() {
        let dir = temp_dir("file");
        let file = dir.join("not-a-dir");
        std::fs::write(&file, b"x").unwrap();
        let outcome = probe_dir(&file.join("child"));
        assert_eq!(outcome.failure, Some(StorageProbeFailure::NotDirectory));
        assert_eq!(outcome.failed_step, "stat");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn only_volume_mount_points_count_as_mount_containers() {
        use super::is_mount_container;

        for dir in [
            "/Volumes",
            "/media",
            "/mnt",
            "/run/media",
            "/media/zero",
            "/run/media/zero",
        ] {
            assert!(is_mount_container(Path::new(dir)), "{dir}");
        }
        for dir in ["/", "/mnt/data", "/media/zero/usb", "/Users/zero", "/tmp"] {
            assert!(!is_mount_container(Path::new(dir)), "{dir}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn read_only_directory_reports_permission_denied_at_create() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("readonly");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        // root 无视权限位：此时探测按预期通过，权限路径无从验证。
        let writable_anyway = std::fs::write(dir.join("root-check"), b"").is_ok();
        let outcome = probe_dir(&dir);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        if !writable_anyway {
            assert_eq!(outcome.failure, Some(StorageProbeFailure::PermissionDenied));
            assert_eq!(outcome.failed_step, "create");
            assert_eq!(outcome.os_error, Some(13));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 托管组件缺执行位：修复后能真实运行；数据目录外的组件不替用户改，未知名称报错。
    #[cfg(unix)]
    #[tokio::test]
    async fn fix_component_restores_execute_bit_of_managed_components_only() {
        use std::os::unix::fs::PermissionsExt;

        use super::{FixComponentError, fix_component};

        let data_dir = temp_dir("component-data");
        let outside = temp_dir("component-outside");
        let managed = data_dir.join("bin").join("yt-dlp");
        std::fs::create_dir_all(managed.parent().unwrap()).unwrap();
        std::fs::write(&managed, "#!/bin/sh\necho 2026.07.04\n").unwrap();
        std::fs::set_permissions(&managed, std::fs::Permissions::from_mode(0o644)).unwrap();
        let manual = outside.join("ffmpeg");
        std::fs::write(&manual, "#!/bin/sh\necho ffmpeg version 7.1\n").unwrap();
        let components = || {
            vec![
                ("yt-dlp", "--version", managed.clone()),
                ("ffmpeg", "-version", manual.clone()),
            ]
        };

        let probe = fix_component(components(), &data_dir, "yt-dlp")
            .await
            .unwrap();
        assert!(probe.managed);
        assert_eq!(probe.error, "");
        assert_eq!(probe.output, "2026.07.04");
        assert_eq!(
            std::fs::metadata(&managed).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert!(matches!(
            fix_component(components(), &data_dir, "ffmpeg").await,
            Err(FixComponentError::NotManaged(_))
        ));
        assert!(matches!(
            fix_component(components(), &data_dir, "ffprobe").await,
            Err(FixComponentError::NotFound(_))
        ));
        std::fs::remove_dir_all(&data_dir).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }
}
