//! agent 私有状态的独占锁、权限与原子持久化。

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fluxdown_protocol::{
    AgentPreferencesDto, AgentSessionDto, GatewayStatusDto, RemoteTaskDto, SyncStatusDto,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use uuid::Uuid;

/// FluxCloud 令牌只存在 agent 私有状态与云传输层。
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at_unix: i64,
    pub session: Option<AgentSessionDto>,
}

/// 单个配置同步键的私有持久化状态。
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedSyncEntry {
    pub value: serde_json::Value,
    pub version: u64,
    pub dirty: bool,
    pub deleted: bool,
}

/// 单个账号的同步水位与条目；账号切换 / 登出时暂存，重新登录同一账号时恢复。
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct AccountSyncData {
    pub revision: u64,
    pub entries: BTreeMap<String, PersistedSyncEntry>,
    pub pulled: bool,
}

/// 暂存账号数上限；超出时丢弃最先的账号，避免状态文件无限增长。
const SYNC_STASH_LIMIT: usize = 8;

/// agent 可恢复状态；不包含 daemon 下载快照或捕获 header/cookie。
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct AgentState {
    pub device_id: String,
    pub device_name: String,
    pub platform: String,
    pub credentials: Option<CloudCredentials>,
    pub sync: SyncStatusDto,
    pub preferences: AgentPreferencesDto,
    /// 当前账号的同步条目（水位在 `sync.revision`）；账号切换由 [`AgentState::bind_account`] 隔离。
    pub sync_entries: BTreeMap<String, PersistedSyncEntry>,
    pub sync_pulled: bool,
    /// `sync_entries` / `sync.revision` / `remote_*` 所属账号；`None` = 未登录。
    pub account_uid: Option<String>,
    /// 其他账号暂存的同步数据（按账号隔离水位与脏键）。
    pub sync_stash: BTreeMap<String, AccountSyncData>,
    /// 用户曾显式关闭同步：登录后不再自动开启。
    pub sync_user_disabled: bool,
    pub gateway: GatewayStatusDto,
    pub gateway_user_token: String,
    pub link_identity: Option<serde_json::Value>,
    pub linked_devices: Vec<serde_json::Value>,
    pub remote_tasks: Vec<RemoteTaskDto>,
    /// 已接单的远程任务 → 本机 daemon 任务；持久化以防重启后重复建任务，并恢复进度绑定。
    pub remote_bindings: BTreeMap<String, String>,
    pub link_migration_revision: Option<u64>,
    pub gateway_migration_revision: Option<u64>,
    pub analytics_install_reported: bool,
    pub analytics_last_active_day: u64,
    /// 匿名统计专用随机 ID；刻意与 FluxCloud `device_id` 分离，统计无法关联到账号 / 设备。
    pub analytics_id: String,
    /// 调试构建下用户覆盖的 FluxCloud 地址；正式构建启动时忽略（锁定固定地址）。
    pub cloud_base_url_override: Option<String>,
}

impl AgentState {
    /// 把账号维度的状态（同步水位 / 脏键 / 远程任务与绑定）切换到 `uid`；返回是否发生了切换。
    ///
    /// 旧账号的同步数据暂存，重新登录同一账号时恢复；未登录期间产生的本地编辑（无所属账号）
    /// 被首个登录账号接管。远程任务与接单绑定属于账号，切换即清空。
    pub fn bind_account(&mut self, uid: Option<&str>) -> bool {
        if self.account_uid.as_deref() == uid {
            return false;
        }
        let current = AccountSyncData {
            revision: self.sync.revision,
            entries: std::mem::take(&mut self.sync_entries),
            pulled: self.sync_pulled,
        };
        let next = match (self.account_uid.take(), uid) {
            (Some(previous), _) => {
                self.sync_stash.insert(previous, current);
                while self.sync_stash.len() > SYNC_STASH_LIMIT {
                    let Some(oldest) = self.sync_stash.keys().next().cloned() else {
                        break;
                    };
                    self.sync_stash.remove(&oldest);
                }
                uid.and_then(|uid| self.sync_stash.remove(uid))
                    .unwrap_or_default()
            }
            // 未登录期间的本地编辑：新账号有暂存则恢复暂存，否则接管这些编辑（水位从 0 开始）。
            (None, Some(uid)) => self.sync_stash.remove(uid).unwrap_or(AccountSyncData {
                revision: 0,
                entries: current.entries,
                pulled: false,
            }),
            (None, None) => AccountSyncData::default(),
        };
        self.sync.revision = next.revision;
        self.sync_entries = next.entries;
        self.sync_pulled = next.pulled;
        self.account_uid = uid.map(str::to_owned);
        self.remote_tasks.clear();
        self.remote_bindings.clear();
        self.refresh_sync_projection();
        self.reset_sync_runtime();
        true
    }

    /// 升级前的状态没有 `account_uid`：已登录时把现有同步数据归属当前会话账号。
    pub fn adopt_session_account(&mut self) {
        if self.account_uid.is_none()
            && let Some(uid) = self
                .credentials
                .as_ref()
                .and_then(|credentials| credentials.session.as_ref())
                .map(|session| session.user.id.clone())
        {
            self.account_uid = Some(uid);
        }
    }

    /// 由 `sync_entries` 重新推导投影的脏键列表。
    pub fn refresh_sync_projection(&mut self) {
        self.sync.dirty_keys = self
            .sync_entries
            .iter()
            .filter(|(_, entry)| entry.dirty)
            .map(|(key, _)| key.clone())
            .collect();
    }

    /// 同步的运行期字段不随进程 / 账号存活。
    pub fn reset_sync_runtime(&mut self) {
        self.sync.connected = false;
        self.sync.halted = false;
        self.sync.last_error = None;
        self.sync.last_error_reason = None;
        self.sync.last_synced_at_unix_ms = None;
    }
}

/// 私有状态存储错误。
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("agent state is already locked")]
    Locked,
    #[error("agent state I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("agent state JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("agent state ACL setup failed: {0}")]
    Acl(String),
}

/// 损坏状态文件的隔离备份保留个数。
const CORRUPT_BACKUP_LIMIT: usize = 3;

/// 持有 agent 独占锁的状态存储。
///
/// 写盘按「序列化时取得的代际」排序：并发调用方各自在持有状态锁时序列化，写盘阶段只需
/// 串行化文件操作，较旧的快照永远不会覆盖较新的快照，慢盘 / ACL 也不会拖住状态锁。
pub struct StateStore {
    data_dir: PathBuf,
    state_path: PathBuf,
    _lock: File,
    /// 下一个快照代际。
    generation: AtomicU64,
    /// 已落盘的最新代际；同时充当写盘互斥锁。
    written: Mutex<u64>,
    /// 数据目录已一次性设置为仅当前用户可访问（Windows ACL 可继承），保存时不再逐文件 spawn 子进程。
    acl_dir_ready: bool,
}

/// Windows 上锁争用是 `ERROR_LOCK_VIOLATION`，std 不把它映射为 `WouldBlock`。
fn is_lock_contended(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::WouldBlock
        || error
            .raw_os_error()
            .is_some_and(|code| fs2::lock_contended_error().raw_os_error() == Some(code))
}

impl StateStore {
    /// 打开状态目录并获取 `<data-dir>/agent.lock`。
    pub async fn open(data_dir: PathBuf) -> Result<Self, StateError> {
        tokio::fs::create_dir_all(&data_dir).await?;
        set_private_dir_permissions(&data_dir).await?;
        let lock_path = data_dir.join("agent.lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        lock.try_lock_exclusive().map_err(|error| {
            if is_lock_contended(&error) {
                StateError::Locked
            } else {
                StateError::Io(error)
            }
        })?;
        let acl_dir_ready = match apply_windows_dir_acl(&data_dir).await {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(error = %error, "agent data directory ACL setup failed; falling back to per-file ACL");
                false
            }
        };
        remove_stale_temp_files(&data_dir).await;
        Ok(Self {
            state_path: data_dir.join("agent-state.json"),
            data_dir,
            _lock: lock,
            generation: AtomicU64::new(1),
            written: Mutex::new(0),
            acl_dir_ready,
        })
    }

    /// 读取状态；文件不存在时返回默认状态。
    ///
    /// 文件损坏（JSON 无法解析）时不让 agent 启动失败：把坏文件隔离为
    /// `agent-state.corrupt-<毫秒时间戳>.json` 备份，并以默认状态启动。
    pub async fn load(&self) -> Result<AgentState, StateError> {
        let bytes = match tokio::fs::read(&self.state_path).await {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(AgentState::default());
            }
            Err(error) => return Err(StateError::Io(error)),
        };
        match serde_json::from_slice::<AgentState>(&bytes) {
            Ok(mut state) => {
                state.reset_sync_runtime();
                Ok(state)
            }
            Err(error) => {
                let backup = self.quarantine_corrupt_state().await?;
                tracing::error!(
                    error = %error,
                    backup = %backup.display(),
                    "agent state file is corrupt; started with default state (backup kept)"
                );
                Ok(AgentState::default())
            }
        }
    }

    async fn quarantine_corrupt_state(&self) -> Result<PathBuf, StateError> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis());
        let backup = self
            .data_dir
            .join(format!("agent-state.corrupt-{stamp:020}.json"));
        tokio::fs::rename(&self.state_path, &backup).await?;
        if let Ok(mut entries) = tokio::fs::read_dir(&self.data_dir).await {
            let mut backups = Vec::new();
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with("agent-state.corrupt-") && name.ends_with(".json") {
                    backups.push(entry.path());
                }
            }
            backups.sort();
            let surplus = backups.len().saturating_sub(CORRUPT_BACKUP_LIMIT);
            for stale in backups.into_iter().take(surplus) {
                if let Err(error) = tokio::fs::remove_file(&stale).await {
                    tracing::warn!(path = %stale.display(), error = %error, "could not prune old corrupt agent state backup");
                }
            }
        }
        Ok(backup)
    }

    /// temp-write + fsync + atomic rename 持久化完整状态。
    ///
    /// 调用方可以持有状态锁调用（序列化在第一个 await 之前完成）；更推荐用 [`StateStore::persist`]
    /// 在写盘期间释放状态锁。
    pub async fn save(&self, state: &AgentState) -> Result<(), StateError> {
        let bytes = serde_json::to_vec(state)?;
        let generation = self.generation.fetch_add(1, Ordering::SeqCst);
        self.write_snapshot(generation, &bytes).await
    }

    /// 持有状态锁只做序列化，写盘期间不占用状态锁。
    pub async fn persist(&self, state: &Mutex<AgentState>) -> Result<(), StateError> {
        let (bytes, generation) = {
            let state = state.lock().await;
            let bytes = serde_json::to_vec(&*state)?;
            (bytes, self.generation.fetch_add(1, Ordering::SeqCst))
        };
        self.write_snapshot(generation, &bytes).await
    }

    async fn write_snapshot(&self, generation: u64, bytes: &[u8]) -> Result<(), StateError> {
        let mut written = self.written.lock().await;
        if generation < *written {
            // 更新的快照已经落盘。
            return Ok(());
        }
        let temp = self
            .data_dir
            .join(format!(".agent-state.{}.tmp", Uuid::new_v4()));
        let result = self.write_temp_then_rename(&temp, bytes).await;
        if result.is_err() {
            let _ = tokio::fs::remove_file(&temp).await;
        } else {
            *written = generation;
        }
        result
    }

    async fn write_temp_then_rename(&self, temp: &Path, bytes: &[u8]) -> Result<(), StateError> {
        let mut file = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(temp)
            .await?;
        set_private_file_permissions(temp).await?;
        if !self.acl_dir_ready {
            apply_windows_acl(temp).await?;
        }
        file.write_all(bytes).await?;
        file.sync_all().await?;
        drop(file);
        tokio::fs::rename(temp, &self.state_path).await?;
        sync_parent(&self.data_dir).await?;
        Ok(())
    }

    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
}

/// 清理上次崩溃遗留的临时状态文件（调用方已持有独占锁，不会误删并发写入）。
async fn remove_stale_temp_files(dir: &Path) {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(".agent-state.")
            && name.ends_with(".tmp")
            && let Err(error) = tokio::fs::remove_file(entry.path()).await
        {
            tracing::warn!(file = %name, error = %error, "could not remove stale agent state temp file");
        }
    }
}

#[cfg(unix)]
pub(crate) async fn set_private_dir_permissions(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).await
}

#[cfg(not(unix))]
pub(crate) async fn set_private_dir_permissions(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

#[cfg(unix)]
pub(crate) async fn set_private_file_permissions(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await
}

#[cfg(not(unix))]
pub(crate) async fn set_private_file_permissions(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

#[cfg(unix)]
async fn sync_parent(path: &Path) -> Result<(), std::io::Error> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
async fn sync_parent(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

/// 当前用户 SID；进程内只查询一次（`whoami` 子进程开销大）。
#[cfg(windows)]
async fn current_user_sid() -> Result<String, StateError> {
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
    .map_err(|error| StateError::Acl(error.to_string()))??;
    if !output.status.success() {
        return Err(StateError::Acl("whoami /user failed".to_owned()));
    }
    let text =
        String::from_utf8(output.stdout).map_err(|error| StateError::Acl(error.to_string()))?;
    let sid = text
        .split(',')
        .nth(1)
        .map(|value| value.trim().trim_matches('"'))
        .filter(|value| value.starts_with("S-1-"))
        .ok_or_else(|| StateError::Acl("could not parse current SID".to_owned()))?
        .to_owned();
    Ok(SID.get_or_init(|| sid).clone())
}

#[cfg(windows)]
async fn icacls_grant_full(path: &Path, inheritable: bool) -> Result<(), StateError> {
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
            .status()
    })
    .await
    .map_err(|error| StateError::Acl(error.to_string()))??;
    if status.success() {
        Ok(())
    } else {
        Err(StateError::Acl("icacls failed".to_owned()))
    }
}

/// 单个文件仅当前用户可访问。
#[cfg(windows)]
pub(crate) async fn apply_windows_acl(path: &Path) -> Result<(), StateError> {
    icacls_grant_full(path, false).await
}

/// 数据目录仅当前用户可访问，且 ACL 可继承：其后在目录内新建的文件（状态临时文件）
/// 自动继承，无需每次保存都 spawn `whoami` / `icacls`。
#[cfg(windows)]
pub(crate) async fn apply_windows_dir_acl(path: &Path) -> Result<(), StateError> {
    icacls_grant_full(path, true).await
}

#[cfg(not(windows))]
pub(crate) async fn apply_windows_acl(_path: &Path) -> Result<(), StateError> {
    Ok(())
}

#[cfg(not(windows))]
pub(crate) async fn apply_windows_dir_acl(_path: &Path) -> Result<(), StateError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::Mutex;

    use super::{AgentState, PersistedSyncEntry, StateError, StateStore};

    #[test]
    fn platform_lock_contention_error_is_recognized() {
        assert!(super::is_lock_contended(&fs2::lock_contended_error()));
        assert!(super::is_lock_contended(&std::io::Error::from(
            std::io::ErrorKind::WouldBlock
        )));
        assert!(!super::is_lock_contended(&std::io::Error::from(
            std::io::ErrorKind::PermissionDenied
        )));
    }

    fn temp_dir(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "fluxdown_agent_{label}_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ))
    }

    fn dirty_entry(value: &str) -> PersistedSyncEntry {
        PersistedSyncEntry {
            value: serde_json::json!(value),
            version: 1,
            dirty: true,
            deleted: false,
        }
    }

    #[test]
    fn account_switch_isolates_sync_watermark_dirty_keys_and_remote_state() {
        let mut state = AgentState::default();
        // 未登录期间的本地编辑被首个登录账号接管，水位从 0 开始。
        state
            .sync_entries
            .insert("appearance.theme_mode".to_owned(), dirty_entry("dark"));
        state.sync.revision = 9;
        assert!(state.bind_account(Some("user-a")));
        assert_eq!(state.sync.revision, 0);
        assert!(state.sync_entries["appearance.theme_mode"].dirty);
        assert_eq!(state.sync.dirty_keys, ["appearance.theme_mode"]);

        state.sync.revision = 40;
        state
            .remote_tasks
            .push(serde_json::from_value(serde_json::json!({"id": "r1"})).expect("remote task"));
        state
            .remote_bindings
            .insert("r1".to_owned(), "t1".to_owned());

        // 切到账号 B：A 的脏键与水位不外泄，远程任务 / 绑定清空。
        assert!(state.bind_account(Some("user-b")));
        assert_eq!(state.sync.revision, 0);
        assert!(state.sync_entries.is_empty());
        assert!(state.sync.dirty_keys.is_empty());
        assert!(state.remote_tasks.is_empty());
        assert!(state.remote_bindings.is_empty());

        // 登出后再登录 A：恢复 A 的水位与脏键。
        state.sync.revision = 5;
        assert!(state.bind_account(None));
        assert_eq!(state.account_uid, None);
        assert!(state.bind_account(Some("user-a")));
        assert_eq!(state.sync.revision, 40);
        assert!(state.sync_entries["appearance.theme_mode"].dirty);

        // 相同账号再次绑定是空操作。
        assert!(!state.bind_account(Some("user-a")));
    }

    #[test]
    fn stash_is_bounded() {
        let mut state = AgentState::default();
        for index in 0..20 {
            state.bind_account(Some(&format!("user-{index:02}")));
        }
        assert!(state.sync_stash.len() <= super::SYNC_STASH_LIMIT);
    }

    #[tokio::test]
    async fn corrupt_state_is_quarantined_and_agent_starts_from_default() {
        let dir = temp_dir("state_corrupt");
        let store = StateStore::open(dir.clone()).await.expect("open store");
        tokio::fs::write(dir.join("agent-state.json"), b"{ not json")
            .await
            .expect("write corrupt state");
        let loaded = store
            .load()
            .await
            .expect("corrupt state must not fail load");
        assert!(loaded.device_id.is_empty());
        assert!(!dir.join("agent-state.json").exists());
        let backups = std::fs::read_dir(&dir)
            .expect("list dir")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("agent-state.corrupt-")
            })
            .count();
        assert_eq!(backups, 1);
        // 之后可以正常保存并读回。
        let state = AgentState {
            device_id: "fresh".to_owned(),
            ..AgentState::default()
        };
        store.save(&state).await.expect("save after recovery");
        assert_eq!(store.load().await.expect("reload").device_id, "fresh");
        drop(store);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn concurrent_persists_never_let_an_older_snapshot_win() {
        let dir = temp_dir("state_persist_order");
        let store = Arc::new(StateStore::open(dir.clone()).await.expect("open store"));
        let state = Arc::new(Mutex::new(AgentState::default()));
        let mut tasks = Vec::new();
        for index in 0..16 {
            let store = store.clone();
            let state = state.clone();
            tasks.push(tokio::spawn(async move {
                state.lock().await.device_name = format!("name-{index}");
                store.persist(&state).await.expect("persist");
            }));
        }
        for task in tasks {
            task.await.expect("join persist task");
        }
        let in_memory = state.lock().await.device_name.clone();
        assert_eq!(store.load().await.expect("load").device_name, in_memory);
        let temps = std::fs::read_dir(&dir)
            .expect("list dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(temps, 0, "no temp files may leak");
        drop(store);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn state_is_atomic_private_and_exclusively_locked() {
        let dir = std::env::temp_dir().join(format!(
            "fluxdown_agent_state_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let store = StateStore::open(dir.clone()).await.expect("open store");
        assert!(matches!(
            StateStore::open(dir.clone()).await,
            Err(StateError::Locked)
        ));
        let state = AgentState {
            device_id: "device-1".to_owned(),
            device_name: "Desktop".to_owned(),
            platform: "linux".to_owned(),
            ..AgentState::default()
        };
        store.save(&state).await.expect("save");
        let loaded = store.load().await.expect("load");
        assert_eq!(loaded.device_id, "device-1");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join("agent-state.json"))
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        drop(store);
        let reopened = StateStore::open(dir.clone()).await.expect("reopen");
        drop(reopened);
        let _ = std::fs::remove_dir_all(dir);
    }
}
