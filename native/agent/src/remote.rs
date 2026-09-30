//! 跨设备任务全量投影、本地执行路由、绑定评分与缺失宽限。
//!
//! 角色：
//! - 发起端：`dispatch` 经 FluxCloud 下发任务，`command` 控制远端任务；
//! - 执行端：SSE 收到 `task.dispatch` 后逐条接单建 daemon 任务，并上报状态 / 进度；
//!   收到 `task.command` 后映射到本机 daemon。
//!
//! 执行端的「云端任务 → 本机 daemon 任务」绑定持久化在 `AgentState::remote_bindings`，
//! 重启后不会为同一个云端任务重复建任务。

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fluxdown_protocol::{
    AgentEvent, CloudDevice, CreateTaskRequest, DaemonCreateTaskParams, DaemonEvent, ErrorReason,
    RemoteCommandAction, RemoteCommandParams, RemoteDispatchParams, RemoteDispatchResult,
    RemoteTaskDto, RemoteTaskStatus, RpcErrorData, ServiceEvent, WsServerMsg,
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::sync::{Mutex, broadcast};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::cloud::{CloudApi, CloudError};
use crate::daemon_client::DaemonClient;
use crate::event_hub::AgentEventHub;
use crate::link::{resolve_receive_dir, valid_file_name};
use crate::state::{AgentState, StateStore};

const MISSING_GRACE_ROUNDS: u8 = 3;
const SSE_STABLE: Duration = Duration::from_secs(60);
const SSE_IDLE_TIMEOUT: Duration = Duration::from_secs(75);
const REPORT_INTERVAL: Duration = Duration::from_secs(1);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
/// 周期性重试接单：daemon 暂时不可用时任务保持 pending，就绪后自动接上。
const ACCEPT_RETRY_INTERVAL: Duration = Duration::from_secs(5);
const RETRY_DELAYS: [Duration; 3] = [
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(60),
];
/// `resync` 后立即重连前的最短停顿，避免云端持续 lag 时空转。
const RESYNC_PAUSE: Duration = Duration::from_secs(1);
/// 已处理命令 id 的去重窗口（FIFO）。
const CONFIRMED_COMMAND_CAPACITY: usize = 512;
/// SSE 单行上限；超过说明流已损坏。
const MAX_SSE_LINE_BYTES: usize = 1024 * 1024;
/// 失败上报里的错误文本上限。
const MAX_REPORTED_ERROR_CHARS: usize = 512;

/// 一次 SSE 会话结束的原因。
#[derive(Debug, Eq, PartialEq)]
enum SseEnd {
    Cancelled,
    /// 云端广播滞后：需要重新全量拉取后重连。
    Resync,
    /// 登录会话已结束（登出 / 撤销）。
    SessionEnded,
}

/// 有界 FIFO 去重集合。
#[derive(Default)]
struct BoundedIds {
    order: VecDeque<String>,
    set: HashSet<String>,
}

impl BoundedIds {
    fn contains(&self, id: &str) -> bool {
        self.set.contains(id)
    }

    fn insert(&mut self, id: String) {
        if !self.set.insert(id.clone()) {
            return;
        }
        self.order.push_back(id);
        while self.order.len() > CONFIRMED_COMMAND_CAPACITY {
            if let Some(oldest) = self.order.pop_front() {
                self.set.remove(&oldest);
            }
        }
    }
}

/// 只属于当前登录会话的运行期状态；登出时整体丢弃。
#[derive(Default)]
struct Runtime {
    missing_rounds: HashMap<String, u8>,
    local_missing_rounds: HashMap<String, u8>,
    reported_statuses: HashMap<String, RemoteTaskStatus>,
    confirmed_commands: BoundedIds,
    /// daemon 任务的实时速度（快照不携带，取自 `TaskProgress` 事件）。
    local_speeds: HashMap<String, i64>,
    /// 本进程内建立绑定的时刻。快照请求发出之后才建立的绑定不能凭「快照里没有」判定记录已删除
    /// （任务可能在该快照之后才下发）；没有条目 = 启动前持久化的旧绑定。
    bound_at: HashMap<String, Instant>,
}

pub struct RemoteTaskService {
    cloud: CloudApi,
    daemon: Arc<DaemonClient>,
    events: AgentEventHub,
    state: Arc<Mutex<AgentState>>,
    store: Arc<StateStore>,
    sse_idle_timeout: Duration,
    runtime: Mutex<Runtime>,
}

impl RemoteTaskService {
    #[must_use]
    pub fn new(
        cloud: CloudApi,
        daemon: Arc<DaemonClient>,
        events: AgentEventHub,
        state: Arc<Mutex<AgentState>>,
        store: Arc<StateStore>,
    ) -> Self {
        Self {
            cloud,
            daemon,
            events,
            state,
            store,
            sse_idle_timeout: SSE_IDLE_TIMEOUT,
            runtime: Mutex::new(Runtime::default()),
        }
    }

    pub async fn run(self: Arc<Self>, cancel: CancellationToken) {
        let mut retry_attempt = 0_usize;
        let (mut session_events, _) = self.events.subscribe_and_snapshot();
        loop {
            if cancel.is_cancelled() {
                return;
            }
            if !self.cloud.is_authenticated().await {
                *self.runtime.lock().await = Runtime::default();
                // 未登录：最多等 30s，或在 `SessionChanged(Some)`（登录成功）时立即醒来，
                // 让设备名册 / 远程任务在登录后马上就位，而不是等下一轮。
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(Duration::from_secs(30)) => {},
                    _ = wait_for_session(&mut session_events) => {},
                }
                continue;
            }
            if let Err(error) = self.refresh_snapshot().await {
                tracing::warn!(error = %error, "remote task snapshot refresh failed");
                let delay = RETRY_DELAYS[retry_attempt.min(RETRY_DELAYS.len() - 1)];
                retry_attempt = retry_attempt.saturating_add(1);
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(delay) => {},
                }
                continue;
            }
            // 受信任设备名册随会话建立即刻投影（启动带凭证 / 登录），UI 不需要手动「重试」。
            if let Err(error) = self.refresh_devices().await {
                tracing::warn!(error = %error, "cloud device roster refresh failed");
            }
            self.rebuild_bindings().await;
            self.accept_pending_dispatches().await;
            let device_id = self.local_device_id().await;
            match self.cloud.remote_events(&device_id).await {
                Ok(response) => {
                    let connected_at = std::time::Instant::now();
                    if let Err(error) = self.cloud.ping_presence().await {
                        tracing::warn!(error = %error, "initial remote presence heartbeat failed");
                    }
                    match self
                        .consume_events(response, &cancel, &mut session_events)
                        .await
                    {
                        Ok(SseEnd::Cancelled) => return,
                        Ok(SseEnd::SessionEnded) => continue,
                        Ok(SseEnd::Resync) => {
                            tracing::info!("remote task SSE asked for resync; reloading snapshot");
                            tokio::select! {
                                _ = cancel.cancelled() => return,
                                _ = tokio::time::sleep(RESYNC_PAUSE) => {},
                            }
                            continue;
                        }
                        Err(error) => {
                            tracing::warn!(error = %error, "remote task SSE disconnected");
                            // 只有连接稳定存活过才清退避；秒断的 SSE 继续按档位放慢重连。
                            if connected_at.elapsed() >= SSE_STABLE {
                                retry_attempt = 0;
                            }
                        }
                    }
                }
                Err(error) => tracing::warn!(error = %error, "remote task SSE connect failed"),
            }
            let delay = RETRY_DELAYS[retry_attempt.min(RETRY_DELAYS.len() - 1)];
            retry_attempt = retry_attempt.saturating_add(1);
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = tokio::time::sleep(delay) => {},
            }
        }
    }

    async fn consume_events(
        &self,
        response: reqwest::Response,
        cancel: &CancellationToken,
        agent_events: &mut broadcast::Receiver<fluxdown_protocol::EventFrame>,
    ) -> Result<SseEnd, RemoteError> {
        let stream_uid = self.cloud.current_user_id().await;
        let mut stream = response.bytes_stream();
        let mut buffer = Vec::<u8>::new();
        let mut report_tick = tokio::time::interval(REPORT_INTERVAL);
        let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
        let mut accept_retry = tokio::time::interval(ACCEPT_RETRY_INTERVAL);
        report_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        accept_retry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // 空闲看门狗：截止时间放在循环外，只有收到流数据才顺延；周期性定时器不会重置它，
        // 半开连接（合盖唤醒 / NAT 静默断流）才能被识别。
        let idle = tokio::time::sleep(self.sse_idle_timeout);
        tokio::pin!(idle);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Ok(SseEnd::Cancelled),
                () = &mut idle => {
                    return Err(RemoteError::Protocol("remote SSE idle timeout".to_owned()));
                }
                _ = report_tick.tick() => {
                    if let Err(error) = self.report_local_progress().await {
                        tracing::warn!(error = %error, "remote progress report failed");
                    }
                }
                _ = accept_retry.tick() => {
                    self.accept_pending_dispatches().await;
                }
                _ = heartbeat.tick() => {
                    if let Err(error) = self.cloud.ping_presence().await {
                        tracing::warn!(error = %error, "remote presence heartbeat failed");
                    }
                }
                frame = agent_events.recv() => match frame {
                    Ok(frame) => {
                        if self.observe_agent_event(&frame.event, stream_uid.as_deref()).await {
                            return Ok(SseEnd::SessionEnded);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        if !self.cloud.is_authenticated().await {
                            return Ok(SseEnd::SessionEnded);
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return Err(RemoteError::Protocol("agent event hub closed".to_owned()));
                    }
                },
                chunk = stream.next() => {
                    let chunk = chunk
                        .ok_or_else(|| RemoteError::Protocol("remote SSE disconnected".to_owned()))?
                        .map_err(|error| RemoteError::Protocol(format!("remote SSE read failed: {error:#}")))?;
                    idle.as_mut().reset(tokio::time::Instant::now() + self.sse_idle_timeout);
                    buffer.extend_from_slice(&chunk);
                    if buffer.len() > MAX_SSE_LINE_BYTES && !buffer.contains(&b'\n') {
                        return Err(RemoteError::Protocol("remote SSE line too long".to_owned()));
                    }
                    while let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
                        let line = buffer.drain(..=newline).collect::<Vec<_>>();
                        if let Some(end) = self.handle_sse_line(&line).await {
                            return Ok(end);
                        }
                    }
                }
            }
        }
    }

    /// 观察 agent 事件：跟踪 daemon 任务的实时速度；返回 `true` 表示会话已结束或换成了别的账号。
    async fn observe_agent_event(&self, event: &ServiceEvent, stream_uid: Option<&str>) -> bool {
        let ServiceEvent::Agent(event) = event else {
            return false;
        };
        match event {
            AgentEvent::SessionChanged(session) => match &**session {
                None => true,
                Some(session) => stream_uid.is_some_and(|uid| uid != session.user.id),
            },
            AgentEvent::Daemon(DaemonEvent::Engine(WsServerMsg::TaskProgress {
                task_id,
                status,
                speed,
                ..
            })) => {
                let mut runtime = self.runtime.lock().await;
                if matches!(*status, 1 | 5) {
                    runtime.local_speeds.insert(task_id.clone(), *speed);
                } else {
                    runtime.local_speeds.remove(task_id);
                }
                false
            }
            AgentEvent::DaemonConnectionChanged(true) => {
                self.runtime.lock().await.local_missing_rounds.clear();
                false
            }
            AgentEvent::Daemon(DaemonEvent::TaskDeleted { task_id }) => {
                self.runtime.lock().await.local_speeds.remove(task_id);
                false
            }
            _ => false,
        }
    }

    /// 单行 SSE 数据的容错处理：坏行 / 单个事件处理失败只记录，不拆整条流。
    async fn handle_sse_line(&self, line: &[u8]) -> Option<SseEnd> {
        let line = match std::str::from_utf8(line) {
            Ok(line) => line.trim(),
            Err(error) => {
                tracing::warn!(error = %error, "remote SSE line is not UTF-8; skipped");
                return None;
            }
        };
        let payload = line.strip_prefix("data:")?.trim();
        let event = match serde_json::from_str::<Value>(payload) {
            Ok(event) => event,
            Err(error) => {
                tracing::warn!(error = %error, "remote SSE event is not JSON; skipped");
                return None;
            }
        };
        let kind = event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        match self.apply_remote_event(event).await {
            Ok(end) => end,
            Err(error) => {
                tracing::warn!(event = %kind, error = %error, "remote SSE event handling failed");
                None
            }
        }
    }

    async fn apply_remote_event(&self, event: Value) -> Result<Option<SseEnd>, RemoteError> {
        match event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "task.dispatch" | "task.status" => {
                let task = serde_json::from_value::<RemoteTaskDto>(event)?;
                let canceled_here = task.status == RemoteTaskStatus::Canceled
                    && task.to_device == self.local_device_id().await;
                let remote_id = task.id.clone();
                self.upsert_task(task).await?;
                if canceled_here {
                    self.discard_local_task(&remote_id).await;
                }
                self.rebuild_bindings().await;
                self.accept_pending_dispatches().await;
            }
            "task.progress" => {
                let items = event
                    .get("items")
                    .and_then(Value::as_array)
                    .map_or(&[][..], Vec::as_slice);
                // 进度只合并进内存投影，不落盘（约 1 次 / 秒）。
                let mut tasks = self.tasks().await;
                if apply_progress_items(&mut tasks, items) {
                    self.replace_tasks(tasks, false).await?;
                }
            }
            "task.command" => {
                let target = event
                    .get("toDevice")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if target == self.local_device_id().await {
                    let command = IncomingCommand::from_event(&event)?;
                    self.execute_local_command(command).await?;
                }
            }
            "task.removed" => {
                // 记录被删除（任意状态）：从投影移除；本设备正在执行它时删除本机任务。
                if let Some(id) = event.get("taskId").and_then(Value::as_str) {
                    let mut tasks = self.tasks().await;
                    let before = tasks.len();
                    tasks.retain(|task| task.id != id);
                    if tasks.len() != before {
                        self.replace_tasks(tasks, true).await?;
                    }
                    self.discard_local_task(id).await;
                }
            }
            "presence" | "device.updated" => {
                if let Err(error) = self.refresh_devices().await {
                    tracing::warn!(error = %error, "cloud device roster refresh failed");
                }
            }
            "session.revoked" => {
                let target = event.get("deviceId").and_then(Value::as_str);
                let local = self.local_device_id().await;
                if target.is_none_or(|target| target == local) {
                    // 指向本设备（被删除 / 被替换）= 设备不再受信任；未指明设备（管理员撤销全部会话等）
                    // = 会话过期。`revoke_session` 先发 `SessionRevoked` 再投影 `SessionChanged(None)`。
                    let reason = if target.is_some() {
                        ErrorReason::DeviceUntrusted
                    } else {
                        ErrorReason::SessionExpired
                    };
                    if let Err(error) = self.cloud.revoke_session(reason).await {
                        tracing::warn!(error = %error, "clearing revoked session failed");
                    }
                }
            }
            "resync" => return Ok(Some(SseEnd::Resync)),
            _ => {}
        }
        Ok(None)
    }

    /// 替换任务投影；`persist` 为 `false` 时只更新内存并发布事件。
    async fn replace_tasks(
        &self,
        tasks: Vec<RemoteTaskDto>,
        persist: bool,
    ) -> Result<(), RemoteError> {
        self.state.lock().await.remote_tasks.clone_from(&tasks);
        if persist {
            self.store.persist(&self.state).await?;
        }
        self.events.publish(AgentEvent::RemoteTasksChanged(tasks));
        Ok(())
    }

    async fn upsert_task(&self, task: RemoteTaskDto) -> Result<(), RemoteError> {
        let mut tasks = self.tasks().await;
        if let Some(existing) = tasks.iter_mut().find(|existing| existing.id == task.id) {
            existing.clone_from(&task);
        } else {
            tasks.push(task);
        }
        self.replace_tasks(tasks, true).await
    }

    async fn binding(&self, remote_id: &str) -> Option<String> {
        self.state
            .lock()
            .await
            .remote_bindings
            .get(remote_id)
            .cloned()
    }

    async fn set_binding(&self, remote_id: &str, local_id: &str) {
        self.state
            .lock()
            .await
            .remote_bindings
            .insert(remote_id.to_owned(), local_id.to_owned());
        self.runtime
            .lock()
            .await
            .bound_at
            .insert(remote_id.to_owned(), Instant::now());
        if let Err(error) = self.store.persist(&self.state).await {
            tracing::warn!(error = %error, "persisting remote task binding failed");
        }
    }

    async fn remove_binding(&self, remote_id: &str) {
        let removed = self
            .state
            .lock()
            .await
            .remote_bindings
            .remove(remote_id)
            .is_some();
        self.runtime.lock().await.bound_at.remove(remote_id);
        if removed && let Err(error) = self.store.persist(&self.state).await {
            tracing::warn!(error = %error, "persisting remote task binding removal failed");
        }
    }

    /// 已有绑定，或按 URL / 文件名重建出的绑定（未接单 / 已终态的任务不会被重建）。
    async fn resolve_binding(&self, remote_id: &str) -> Option<String> {
        if let Some(local_id) = self.binding(remote_id).await {
            return Some(local_id);
        }
        self.rebuild_bindings().await;
        self.binding(remote_id).await
    }

    /// 云端已取消 / 删除本设备执行的任务：删除绑定的本机 daemon 任务并解除绑定。
    ///
    /// 保留已下载文件——目标离线期间发起端的 `deleteFiles` 选择送不到这里；本机已完成的任务不动。
    /// daemon 暂时不可用时保留绑定，由下一次状态上报（404 / 409）或快照对照重试。
    async fn discard_local_task(&self, remote_id: &str) {
        let Some(local_id) = self.binding(remote_id).await else {
            return;
        };
        // daemon 状态 3 = 已完成。
        let completed = daemon_tasks(&self.events).is_some_and(|tasks| {
            tasks
                .iter()
                .any(|task| task.task_id == local_id && task.status == 3)
        });
        if !completed {
            tracing::info!(task = %remote_id, local = %local_id, "remote task was canceled or deleted in FluxCloud; removing the local task");
            if let Err(error) = self
                .daemon_task_call(
                    fluxdown_protocol::method::DAEMON_TASK_DELETE,
                    &local_id,
                    false,
                )
                .await
            {
                if error.code == fluxdown_protocol::ApplicationErrorCode::Unavailable {
                    tracing::warn!(task = %remote_id, "daemon is unavailable; will retry removing the local task");
                    return;
                }
                tracing::warn!(task = %remote_id, error = ?error, "removing the local task of a canceled remote task failed");
            }
        }
        self.remove_binding(remote_id).await;
    }

    async fn rebuild_bindings(&self) {
        let local_device = self.local_device_id().await;
        let Some(local_tasks) = daemon_tasks(&self.events) else {
            return;
        };
        let remote_tasks = self.tasks().await;
        let mut newly_bound = Vec::new();
        {
            let mut state = self.state.lock().await;
            let mut claimed = state
                .remote_bindings
                .values()
                .cloned()
                .collect::<HashSet<_>>();
            for remote in &remote_tasks {
                if remote.to_device != local_device
                    || remote.status == RemoteTaskStatus::Pending
                    || remote.status == RemoteTaskStatus::Unknown
                    || remote.status.is_terminal()
                    || state.remote_bindings.contains_key(&remote.id)
                {
                    continue;
                }
                if let Some(local) = local_tasks
                    .iter()
                    .filter(|task| !claimed.contains(&task.task_id))
                    .filter_map(|task| {
                        let score = Self::rebind_score(remote, task);
                        (score >= 11).then_some((score, task))
                    })
                    .max_by_key(|(score, _)| *score)
                    .map(|(_, task)| task)
                {
                    state
                        .remote_bindings
                        .insert(remote.id.clone(), local.task_id.clone());
                    claimed.insert(local.task_id.clone());
                    newly_bound.push(remote.id.clone());
                }
            }
        }
        if newly_bound.is_empty() {
            return;
        }
        {
            let now = Instant::now();
            let mut runtime = self.runtime.lock().await;
            for remote_id in newly_bound {
                runtime.bound_at.insert(remote_id, now);
            }
        }
        if let Err(error) = self.store.persist(&self.state).await {
            tracing::warn!(error = %error, "persisting rebuilt remote task bindings failed");
        }
    }

    /// 接单：逐条容错，任何一条失败都不影响后面的任务，也不中断 SSE。
    async fn accept_pending_dispatches(&self) {
        let local_device = self.local_device_id().await;
        let pending = self
            .tasks()
            .await
            .into_iter()
            .filter(|task| {
                task.to_device == local_device && task.status == RemoteTaskStatus::Pending
            })
            .collect::<Vec<_>>();
        for task in pending {
            if self.binding(&task.id).await.is_some() {
                continue;
            }
            self.accept_one(&task).await;
        }
    }

    async fn accept_one(&self, task: &RemoteTaskDto) {
        if task.url.trim().is_empty() {
            self.report_accept_failure(task, "dispatched task has no URL")
                .await;
            return;
        }
        let requested_dir = accept_save_dir(task.save_dir.as_deref());
        if requested_dir.is_none()
            && task
                .save_dir
                .as_deref()
                .is_some_and(|dir| !dir.trim().is_empty())
        {
            tracing::info!(
                task = %task.id,
                save_dir = ?task.save_dir,
                "dispatched save directory is not usable on this device; using the default directory"
            );
        }
        let created =
            create_with_directory_fallback(requested_dir, |dir| self.create_local_task(task, dir))
                .await;
        match created {
            Ok(local_id) => {
                // 先持久化绑定再上报：上报失败 / 进程重启都不会重复建任务。
                self.set_binding(&task.id, &local_id).await;
                match self
                    .cloud
                    .report_remote_status(&task.id, &json!({"status": "accepted"}))
                    .await
                {
                    Ok(_) => {
                        self.runtime
                            .lock()
                            .await
                            .reported_statuses
                            .insert(task.id.clone(), RemoteTaskStatus::Accepted);
                    }
                    // 接单期间记录已被删除：删除刚建的本机任务。
                    Err(error) if error.status == Some(404) => {
                        tracing::info!(task = %task.id, "accepted task no longer exists in FluxCloud");
                        self.discard_local_task(&task.id).await;
                    }
                    // 接单期间已被取消。
                    Err(error) if error.status == Some(409) => {
                        self.resolve_status_conflict(&task.id).await;
                    }
                    // 其余失败：下一轮进度上报会带上真实状态。
                    Err(error) => {
                        tracing::warn!(task = %task.id, error = %error, "reporting accepted status failed");
                    }
                }
            }
            Err(failure) if failure.transient => {
                // daemon 暂时不可用（启动中 / 重连）：不是任务本身的问题，保持 pending，稍后重试。
                tracing::info!(task = %task.id, error = %failure.message, "daemon is unavailable; will retry accepting the task");
            }
            Err(failure) => {
                tracing::warn!(task = %task.id, error = %failure.message, "accepting dispatched task failed");
                self.report_accept_failure(task, &failure.message).await;
            }
        }
    }

    async fn create_local_task(
        &self,
        task: &RemoteTaskDto,
        save_dir: Option<String>,
    ) -> Result<String, CreateFailure> {
        let mut request = json!({ "url": task.url });
        let file_name = task.file_name.trim();
        if !file_name.is_empty() {
            if !valid_file_name(file_name) {
                return Err(CreateFailure::permanent(
                    "dispatched file name is not a plain file name".to_owned(),
                ));
            }
            request["fileName"] = json!(file_name);
        }
        if let Some(dir) = &save_dir {
            // daemon 不校验目录：先确认本机确实能用（能创建 / 已存在），否则交给上层去掉目录重试。
            tokio::fs::create_dir_all(dir).await.map_err(|error| {
                CreateFailure::permanent(format!("save directory `{dir}` is unavailable: {error}"))
            })?;
            request["saveDir"] = json!(dir);
        }
        let request = serde_json::from_value::<CreateTaskRequest>(request).map_err(|error| {
            CreateFailure::permanent(format!("invalid dispatched task: {error}"))
        })?;
        let result = self
            .daemon
            .call_detailed::<DaemonCreateTaskParams, Value>(
                fluxdown_protocol::method::DAEMON_TASK_CREATE,
                Some(DaemonCreateTaskParams {
                    request,
                    torrent_blob_id: None,
                    unattended: true,
                    hint_file_size: None,
                }),
            )
            .await
            .map_err(|error| CreateFailure {
                transient: error.data.as_ref().is_some_and(|data| {
                    data.code == fluxdown_protocol::ApplicationErrorCode::Unavailable
                }),
                message: error.message,
            })?;
        result
            .get("taskId")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| CreateFailure::permanent("daemon returned no taskId".to_owned()))
    }

    /// 接单失败：向云端上报 `failed` + 错误，让发起端看到原因而不是永远 pending。
    async fn report_accept_failure(&self, task: &RemoteTaskDto, message: &str) {
        let error: String = message.chars().take(MAX_REPORTED_ERROR_CHARS).collect();
        match self
            .cloud
            .report_remote_status(&task.id, &json!({"status": "failed", "error": error}))
            .await
        {
            Ok(_) => {
                self.runtime
                    .lock()
                    .await
                    .reported_statuses
                    .insert(task.id.clone(), RemoteTaskStatus::Failed);
            }
            Err(report_error) => {
                tracing::warn!(task = %task.id, error = %report_error, "reporting failed accept status failed");
            }
        }
    }

    async fn report_local_progress(&self) -> Result<(), RemoteError> {
        // daemon 冷启动时快照为空：此时判定「本机任务消失」会把仍在下载的任务误报为 failed。
        let Some(local_tasks) = daemon_tasks(&self.events) else {
            return Ok(());
        };
        let bindings = self.state.lock().await.remote_bindings.clone();
        let mut progress = Vec::new();
        for (remote_id, local_id) in bindings {
            let Some(task) = local_tasks.iter().find(|task| task.task_id == local_id) else {
                let expired = {
                    let mut runtime = self.runtime.lock().await;
                    let count = runtime
                        .local_missing_rounds
                        .entry(remote_id.clone())
                        .or_default();
                    *count = count.saturating_add(1);
                    *count >= MISSING_GRACE_ROUNDS
                };
                if expired {
                    self.report_status_if_changed(
                        &remote_id,
                        RemoteTaskStatus::Failed,
                        Some("local task disappeared"),
                        None,
                    )
                    .await?;
                    self.remove_binding(&remote_id).await;
                }
                continue;
            };
            self.runtime
                .lock()
                .await
                .local_missing_rounds
                .remove(&remote_id);
            let status = local_status(task.status);
            self.report_status_if_changed(
                &remote_id,
                status,
                (!task.error_message.is_empty()).then_some(task.error_message.as_str()),
                Some(task),
            )
            .await?;
            if matches!(task.status, 1 | 5) {
                let speed = self
                    .runtime
                    .lock()
                    .await
                    .local_speeds
                    .get(&task.task_id)
                    .copied()
                    .unwrap_or(0);
                progress.push(progress_item(
                    &remote_id,
                    task.downloaded_bytes,
                    task.total_bytes,
                    speed,
                ));
            }
        }
        if !progress.is_empty() {
            self.cloud
                .report_remote_progress(&json!({"items": progress}))
                .await?;
        }
        Ok(())
    }

    async fn report_status_if_changed(
        &self,
        remote_id: &str,
        status: RemoteTaskStatus,
        error: Option<&str>,
        task: Option<&fluxdown_protocol::TaskDto>,
    ) -> Result<(), RemoteError> {
        if self.runtime.lock().await.reported_statuses.get(remote_id) == Some(&status) {
            return Ok(());
        }
        let body = json!({
            "status": remote_status_wire(status),
            "totalBytes": task.map(|task| task.total_bytes),
            "fileName": task.map(|task| task.file_name.clone()),
            "error": error,
        });
        match self.cloud.report_remote_status(remote_id, &body).await {
            Ok(_) => {
                self.runtime
                    .lock()
                    .await
                    .reported_statuses
                    .insert(remote_id.to_owned(), status);
                if status.is_terminal() {
                    self.remove_binding(remote_id).await;
                }
                Ok(())
            }
            Err(error) if error.status == Some(409) => {
                self.resolve_status_conflict(remote_id).await;
                Ok(())
            }
            Err(error) if error.status == Some(404) => {
                // 云端已删除该任务（离线期间被删除等）：删除本机任务并解除绑定。
                tracing::info!(task = %remote_id, "remote task no longer exists in FluxCloud");
                self.discard_local_task(remote_id).await;
                Ok(())
            }
            Err(error) if error.status == Some(403) => {
                // 本设备不是它的目标：解除绑定，不再上报。
                tracing::info!(task = %remote_id, "remote task no longer accepts reports from this device");
                self.remove_binding(remote_id).await;
                Ok(())
            }
            Err(error) => Err(RemoteError::Cloud(error)),
        }
    }

    /// 上报状态遭遇 409：云端已经把任务终态化（例如被发起端取消）。云端为 `canceled` 或记录已
    /// 不存在时删除本机任务（保留已下载文件），其余终态只解除绑定。
    async fn resolve_status_conflict(&self, remote_id: &str) {
        if let Err(error) = self.refresh_snapshot().await {
            tracing::warn!(task = %remote_id, error = %error, "refreshing tasks after a status conflict failed");
        }
        let cloud_status = self
            .tasks()
            .await
            .into_iter()
            .find(|task| task.id == remote_id)
            .map(|task| task.status);
        if matches!(cloud_status, Some(RemoteTaskStatus::Canceled) | None) {
            self.discard_local_task(remote_id).await;
        } else {
            self.remove_binding(remote_id).await;
        }
        // 之后不再重复上报同一状态。
        self.runtime.lock().await.reported_statuses.insert(
            remote_id.to_owned(),
            cloud_status.unwrap_or(RemoteTaskStatus::Canceled),
        );
    }

    /// 首次或重连后用完整 `/tasks/remote` 快照替换投影。
    ///
    /// 执行端同时据快照收敛本机任务：云端已 `canceled`，或快照请求发出前就已绑定、快照里却没有
    /// （记录已被删除；云端快照从不截断未终态任务）的，删除本机任务——目标离线期间被取消 / 删除的
    /// 任务在重连时收敛，不会继续在后台下载。
    pub async fn refresh_snapshot(&self) -> Result<Vec<RemoteTaskDto>, RemoteError> {
        let requested_at = Instant::now();
        let value = self.cloud.remote_tasks().await?;
        let tasks = parse_task_list(&value);
        let (stale, deleted) = self.stale_bindings(&tasks, requested_at).await;
        for remote_id in &stale {
            self.discard_local_task(remote_id).await;
        }
        let mut merged = self.apply_missing_grace(tasks).await;
        merged.retain(|task| !deleted.contains(&task.id));
        let bound = {
            let mut state = self.state.lock().await;
            state.remote_tasks.clone_from(&merged);
            // 已终态 / 已被云端删除的任务不再需要绑定；本机任务删除失败（daemon 暂不可用）的保留重试。
            let live = merged
                .iter()
                .filter(|task| !task.status.is_terminal())
                .map(|task| task.id.as_str())
                .chain(stale.iter().map(String::as_str))
                .collect::<HashSet<_>>();
            state
                .remote_bindings
                .retain(|remote_id, _| live.contains(remote_id.as_str()));
            state
                .remote_bindings
                .keys()
                .cloned()
                .collect::<HashSet<_>>()
        };
        self.runtime
            .lock()
            .await
            .bound_at
            .retain(|remote_id, _| bound.contains(remote_id));
        self.store.persist(&self.state).await?;
        self.events
            .publish(AgentEvent::RemoteTasksChanged(merged.clone()));
        Ok(merged)
    }

    /// 快照对照本机绑定：返回（需要删除本机任务的绑定，其中记录已被删除的那部分）。
    /// 快照请求发出之后才建立的绑定不凭缺失判定——任务可能在该快照之后才下发。
    async fn stale_bindings(
        &self,
        snapshot: &[RemoteTaskDto],
        requested_at: Instant,
    ) -> (Vec<String>, HashSet<String>) {
        let bound = self
            .state
            .lock()
            .await
            .remote_bindings
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let runtime = self.runtime.lock().await;
        let mut stale = Vec::new();
        let mut deleted = HashSet::new();
        for remote_id in bound {
            match snapshot.iter().find(|task| task.id == remote_id) {
                Some(task) if task.status == RemoteTaskStatus::Canceled => stale.push(remote_id),
                Some(_) => {}
                None if runtime
                    .bound_at
                    .get(&remote_id)
                    .is_none_or(|bound_at| *bound_at < requested_at) =>
                {
                    deleted.insert(remote_id.clone());
                    stale.push(remote_id);
                }
                None => {}
            }
        }
        (stale, deleted)
    }

    pub async fn local_device_id(&self) -> String {
        self.state.lock().await.device_id.clone()
    }

    /// 拉取受信任设备名册并投影 `CloudDevicesChanged`（会话建立后与 presence 事件共用）。
    pub async fn refresh_devices(&self) -> Result<(), RemoteError> {
        let value = self.cloud.devices(&self.local_device_id().await).await?;
        let devices = parse_device_list(&value);
        self.events
            .publish(AgentEvent::CloudDevicesChanged(devices));
        Ok(())
    }

    pub async fn tasks(&self) -> Vec<RemoteTaskDto> {
        self.state.lock().await.remote_tasks.clone()
    }

    /// 经 FluxCloud 把下载下发到本账号的另一台设备。
    ///
    /// `save_dir` 省略 = 目标设备的默认目录；给出时必须是目标设备路径风格下的绝对路径。
    pub async fn dispatch(
        &self,
        params: RemoteDispatchParams,
    ) -> Result<RemoteDispatchResult, RemoteError> {
        let local_device = self.local_device_id().await;
        if params.to_device == local_device {
            return Err(RemoteError::InvalidArgument {
                field: "toDevice",
                reason: None,
                message: "cannot dispatch a download to this device".to_owned(),
            });
        }
        if params.url.trim().is_empty() {
            return Err(RemoteError::InvalidArgument {
                field: "url",
                reason: None,
                message: "url is empty".to_owned(),
            });
        }
        let save_dir = params
            .save_dir
            .as_deref()
            .map(str::trim)
            .filter(|dir| !dir.is_empty())
            .map(str::to_owned);
        if let Some(dir) = &save_dir {
            let target = self.events.inspect(|snapshot| {
                snapshot
                    .cloud_devices
                    .iter()
                    .find(|device| device.device_id == params.to_device)
                    .cloned()
            });
            validate_target_save_dir(target.as_ref(), dir)?;
        }
        let file_name = params
            .file_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned);
        let value = self
            .cloud
            .dispatch_remote(&json!({
                "deviceId": local_device,
                "toDevice": params.to_device,
                "url": params.url,
                "fileName": file_name,
                "saveDir": save_dir,
            }))
            .await?;
        let task_value = value.get("task").cloned().unwrap_or(value);
        let task = serde_json::from_value::<RemoteTaskDto>(task_value)?;
        // 立即进入投影：UI 不必等云端推送。
        if let Err(error) = self.upsert_task(task.clone()).await {
            tracing::warn!(error = %error, "projecting the dispatched task failed");
        }
        Ok(RemoteDispatchResult { task })
    }

    /// 远程任务控制。
    ///
    /// - pause / resume：目标是本机时直接映射到 daemon；否则经云端转发（目标离线时云端 409）。
    /// - cancel / delete：目标是本机且已接单时先处理本机任务（delete 按 `delete_files`），再交给云端
    ///   落库。云端不依赖目标在线判定：cancel 置 `canceled`，delete 删除任何状态的记录；执行端据
    ///   持久态收敛本机任务。delete 成功后立即从投影移除该行。
    /// - `command_id` 缺省生成唯一值，同一动作可重复下发；相同 id 只执行一次。
    pub async fn command(&self, params: RemoteCommandParams) -> Result<(), RemoteError> {
        let command_id = params
            .command_id
            .clone()
            .filter(|id| !id.trim().is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        if self
            .runtime
            .lock()
            .await
            .confirmed_commands
            .contains(&command_id)
        {
            return Ok(());
        }
        let task = self
            .tasks()
            .await
            .into_iter()
            .find(|task| task.id == params.task_id)
            .ok_or_else(|| RemoteError::NotFound(params.task_id.clone()))?;
        if task.status == RemoteTaskStatus::Unknown {
            return Err(RemoteError::TaskUnavailable(task.id));
        }
        let local_target = task.to_device == self.local_device_id().await;
        let local_command = IncomingCommand {
            task_id: task.id.clone(),
            action: params.action,
            command_id: command_id.clone(),
            delete_files: params.delete_files,
        };
        match params.action {
            RemoteCommandAction::Pause | RemoteCommandAction::Resume if local_target => {
                return self.execute_local_command(local_command).await;
            }
            // 同一个 command id 已记入去重窗口：云端回显给本机的 `task.command` 不会再执行一次。
            RemoteCommandAction::Cancel | RemoteCommandAction::Delete if local_target => {
                self.execute_local_command(local_command).await?;
            }
            _ => {}
        }
        let sent = self
            .cloud
            .command_remote(
                &task.id,
                &json!({
                    "action": action_wire(params.action),
                    "commandId": command_id,
                    "deleteFiles": params.delete_files,
                }),
            )
            .await;
        match sent {
            Ok(_) => {}
            // 记录已被别的设备删除：删除的目标已达成。
            Err(error)
                if params.action == RemoteCommandAction::Delete && error.status == Some(404) => {}
            Err(error) => return Err(RemoteError::Cloud(error)),
        }
        if params.action == RemoteCommandAction::Delete {
            let mut tasks = self.tasks().await;
            tasks.retain(|existing| existing.id != task.id);
            self.replace_tasks(tasks, true).await?;
        }
        self.runtime
            .lock()
            .await
            .confirmed_commands
            .insert(command_id);
        Ok(())
    }

    /// 执行端：把命令映射到本机 daemon；命令 id 只在执行成功后记入去重窗口。
    /// cancel / delete 遇到没有本机任务的远程任务（尚未接单 / 已处理过）直接成功：
    /// 云端记录由指令发起方落库，这里没有要删的东西。
    async fn execute_local_command(&self, command: IncomingCommand) -> Result<(), RemoteError> {
        if self
            .runtime
            .lock()
            .await
            .confirmed_commands
            .contains(&command.command_id)
        {
            return Ok(());
        }
        let finalizes = matches!(
            command.action,
            RemoteCommandAction::Cancel | RemoteCommandAction::Delete
        );
        match self.resolve_binding(&command.task_id).await {
            Some(local_id) => {
                let (method, delete_files) = match command.action {
                    RemoteCommandAction::Pause => {
                        (fluxdown_protocol::method::DAEMON_TASK_PAUSE, false)
                    }
                    RemoteCommandAction::Resume => {
                        (fluxdown_protocol::method::DAEMON_TASK_RESUME, false)
                    }
                    RemoteCommandAction::Cancel => {
                        (fluxdown_protocol::method::DAEMON_TASK_DELETE, false)
                    }
                    RemoteCommandAction::Delete => (
                        fluxdown_protocol::method::DAEMON_TASK_DELETE,
                        command.delete_files,
                    ),
                };
                self.daemon_task_call(method, &local_id, delete_files)
                    .await
                    .map_err(RemoteError::Daemon)?;
                if finalizes {
                    // 先解绑再上报：记录已被删除时上报得到 404，不会再去删一次本机任务。
                    self.remove_binding(&command.task_id).await;
                    if let Err(error) = self
                        .report_status_if_changed(
                            &command.task_id,
                            RemoteTaskStatus::Canceled,
                            None,
                            None,
                        )
                        .await
                    {
                        tracing::warn!(task = %command.task_id, error = %error, "reporting canceled status failed");
                    }
                }
            }
            None if finalizes => {
                tracing::debug!(task = %command.task_id, "no local task to remove for this remote task");
            }
            None => {
                return Err(RemoteError::Protocol(format!(
                    "no local task is bound to remote task {}",
                    command.task_id
                )));
            }
        }
        self.runtime
            .lock()
            .await
            .confirmed_commands
            .insert(command.command_id);
        Ok(())
    }

    async fn daemon_task_call(
        &self,
        method: &str,
        local_id: &str,
        delete_files: bool,
    ) -> Result<Value, RpcErrorData> {
        let params = if method == fluxdown_protocol::method::DAEMON_TASK_DELETE {
            json!({ "taskId": local_id, "deleteFiles": delete_files })
        } else {
            json!({ "taskId": local_id })
        };
        self.daemon.call(method, Some(params)).await
    }

    /// URL 为主键，文件名与保存目录提供稳定加分。
    #[must_use]
    pub fn rebind_score(remote: &RemoteTaskDto, local: &fluxdown_protocol::TaskDto) -> i32 {
        if remote.url != local.url && remote.url != local.origin_url {
            return -1;
        }
        let mut score = 10;
        if !remote.file_name.is_empty() && remote.file_name == local.file_name {
            score += 4;
        }
        if remote
            .save_dir
            .as_deref()
            .is_some_and(|dir| dir == local.save_dir)
        {
            score += 2;
        }
        score
    }

    async fn apply_missing_grace(&self, incoming: Vec<RemoteTaskDto>) -> Vec<RemoteTaskDto> {
        let previous = self.state.lock().await.remote_tasks.clone();
        let incoming_ids = incoming
            .iter()
            .map(|task| task.id.clone())
            .collect::<HashSet<_>>();
        let mut expired = Vec::new();
        let mut merged = incoming;
        {
            let mut runtime = self.runtime.lock().await;
            for task in previous {
                if incoming_ids.contains(task.id.as_str()) || task.status.is_terminal() {
                    runtime.missing_rounds.remove(&task.id);
                    continue;
                }
                let round = runtime.missing_rounds.entry(task.id.clone()).or_default();
                *round = round.saturating_add(1);
                if *round < MISSING_GRACE_ROUNDS {
                    merged.push(task);
                } else {
                    runtime.missing_rounds.remove(&task.id);
                    expired.push(task.id);
                }
            }
        }
        if !expired.is_empty() {
            let mut state = self.state.lock().await;
            for id in expired {
                state.remote_bindings.remove(&id);
            }
        }
        merged
    }
}

/// 云端下发的命令（SSE `task.command` 或本机直接控制）。
struct IncomingCommand {
    task_id: String,
    action: RemoteCommandAction,
    command_id: String,
    delete_files: bool,
}

impl IncomingCommand {
    /// `commandId` 缺省时生成唯一值——不能用「任务:动作」作幂等键，否则同一动作第二次会被吞掉。
    fn from_event(event: &Value) -> Result<Self, RemoteError> {
        let task_id = event
            .get("taskId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if task_id.is_empty() {
            return Err(RemoteError::Protocol(
                "task.command without taskId".to_owned(),
            ));
        }
        let action = match event
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "pause" => RemoteCommandAction::Pause,
            "resume" => RemoteCommandAction::Resume,
            "cancel" => RemoteCommandAction::Cancel,
            "delete" => RemoteCommandAction::Delete,
            other => return Err(RemoteError::InvalidAction(other.to_owned())),
        };
        let command_id = event
            .get("commandId")
            .and_then(Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .map_or_else(|| Uuid::new_v4().to_string(), str::to_owned);
        Ok(Self {
            task_id: task_id.to_owned(),
            action,
            command_id,
            delete_files: event
                .get("deleteFiles")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }
}

/// 接单时使用的保存目录：与局域网互联入口共用 [`resolve_receive_dir`]（本机风格绝对路径，
/// 拒绝 `..`、UNC 与设备路径）；其余（缺省 / 对端风格 / 相对路径）一律用本机默认目录。
fn accept_save_dir(requested: Option<&str>) -> Option<String> {
    resolve_receive_dir(requested?)
}

/// 建任务失败；`transient` = daemon 暂时不可用（不是任务本身的问题，不应上报为失败）。
#[derive(Debug, Eq, PartialEq)]
struct CreateFailure {
    message: String,
    transient: bool,
}

impl CreateFailure {
    fn permanent(message: String) -> Self {
        Self {
            message,
            transient: false,
        }
    }
}

/// 带目录建任务；目录类失败时去掉目录重试，仍失败返回最后一次的错误。
async fn create_with_directory_fallback<F, Fut>(
    directory: Option<String>,
    mut create: F,
) -> Result<String, CreateFailure>
where
    F: FnMut(Option<String>) -> Fut,
    Fut: Future<Output = Result<String, CreateFailure>>,
{
    match create(directory.clone()).await {
        Ok(id) => Ok(id),
        Err(error) if directory.is_some() && !error.transient => {
            tracing::warn!(error = %error.message, "creating the task in the requested directory failed; retrying with the default directory");
            create(None).await
        }
        Err(error) => Err(error),
    }
}

/// 发起端校验：保存目录必须是目标设备路径风格下的绝对路径。
fn validate_target_save_dir(
    target: Option<&CloudDevice>,
    save_dir: &str,
) -> Result<(), RemoteError> {
    let Some(style) = target.and_then(CloudDevice::effective_path_style) else {
        // 目标未上报风格且平台未知：无法判断，交给云端与执行端兜底。
        return Ok(());
    };
    if style.is_absolute(save_dir) {
        Ok(())
    } else {
        Err(RemoteError::InvalidArgument {
            field: "saveDir",
            reason: Some(ErrorReason::SaveDirUnavailable),
            message: format!("saveDir must be an absolute {style:?} path on the target device"),
        })
    }
}

/// 把 `task.progress` 的 `items[]` 合并进任务列表；返回是否有任务变化。
/// 云端字段是 `taskId`（不是 `id`）；终态任务不被进度覆盖。
fn apply_progress_items(tasks: &mut [RemoteTaskDto], items: &[Value]) -> bool {
    let mut changed = false;
    for item in items {
        let Some(id) = item.get("taskId").and_then(Value::as_str) else {
            continue;
        };
        let Some(task) = tasks.iter_mut().find(|task| task.id == id) else {
            continue;
        };
        if task.status.is_terminal() || task.status == RemoteTaskStatus::Unknown {
            continue;
        }
        if task.status != RemoteTaskStatus::Downloading {
            task.status = RemoteTaskStatus::Downloading;
        }
        if let Some(value) = item.get("downloadedBytes").and_then(Value::as_i64) {
            task.downloaded_bytes = value.max(0);
        }
        if let Some(value) = item.get("speed").and_then(Value::as_i64) {
            task.speed = value.max(0);
        }
        if let Some(value) = item.get("progress").and_then(Value::as_f64) {
            task.progress = clamp_progress(value);
        }
        if let Some(value) = item.get("totalBytes").and_then(Value::as_i64) {
            task.total_bytes = Some(value.max(0));
        }
        changed = true;
    }
    changed
}

fn clamp_progress(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// 进度上报条目；`progress` 夹到 [0,1]（云端校验 `0..=1`，越界会拒绝整批）。
fn progress_item(remote_id: &str, downloaded: i64, total: i64, speed: i64) -> Value {
    let progress = if total > 0 {
        clamp_progress(downloaded as f64 / total as f64)
    } else {
        0.0
    };
    json!({
        "taskId": remote_id,
        "downloadedBytes": downloaded.max(0),
        "speed": speed.max(0),
        "progress": progress,
    })
}

/// daemon 任务快照；daemon 未连接时为 `None`——此时空列表不代表「任务不存在」。
fn daemon_tasks(events: &AgentEventHub) -> Option<Vec<fluxdown_protocol::TaskDto>> {
    match events.snapshot().body {
        fluxdown_protocol::SnapshotBody::Agent(snapshot) if snapshot.daemon_connected => {
            Some(snapshot.daemon.tasks)
        }
        _ => None,
    }
}

/// daemon 任务状态 → 云端状态：0 排队 → accepted，1 下载中 / 5 准备中 → downloading。
fn local_status(status: i32) -> RemoteTaskStatus {
    match status {
        1 | 5 => RemoteTaskStatus::Downloading,
        2 => RemoteTaskStatus::Paused,
        3 => RemoteTaskStatus::Completed,
        4 => RemoteTaskStatus::Failed,
        _ => RemoteTaskStatus::Accepted,
    }
}

fn remote_status_wire(status: RemoteTaskStatus) -> &'static str {
    match status {
        RemoteTaskStatus::Pending | RemoteTaskStatus::Unknown => "pending",
        RemoteTaskStatus::Accepted => "accepted",
        RemoteTaskStatus::Downloading => "downloading",
        RemoteTaskStatus::Paused => "paused",
        RemoteTaskStatus::Completed => "completed",
        RemoteTaskStatus::Failed => "failed",
        RemoteTaskStatus::Canceled => "canceled",
    }
}

fn action_wire(action: RemoteCommandAction) -> &'static str {
    match action {
        RemoteCommandAction::Pause => "pause",
        RemoteCommandAction::Resume => "resume",
        RemoteCommandAction::Cancel => "cancel",
        RemoteCommandAction::Delete => "delete",
    }
}

trait RemoteStatusExt {
    fn is_terminal(&self) -> bool;
}

impl RemoteStatusExt for RemoteTaskStatus {
    fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Canceled)
    }
}

/// 逐条解析：单条畸形任务只被跳过并记录，不让整份快照（以及设备名册 / 接单 / SSE）停摆。
fn parse_task_list(value: &Value) -> Vec<RemoteTaskDto> {
    parse_list(value, &["tasks", "value"], "remote task")
}

fn parse_device_list(value: &Value) -> Vec<CloudDevice> {
    parse_list(value, &["devices", "value"], "cloud device")
}

fn parse_list<T: serde::de::DeserializeOwned>(value: &Value, keys: &[&str], what: &str) -> Vec<T> {
    let list = keys
        .iter()
        .find_map(|key| value.get(*key))
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    list.iter()
        .filter_map(|item| match serde_json::from_value::<T>(item.clone()) {
            Ok(parsed) => Some(parsed),
            Err(error) => {
                tracing::warn!(error = %error, item = %item, "skipping malformed {what}");
                None
            }
        })
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error(transparent)]
    Cloud(#[from] CloudError),
    #[error("daemon remote command failed: {0:?}")]
    Daemon(RpcErrorData),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    State(#[from] crate::state::StateError),
    #[error("invalid remote action: {0}")]
    InvalidAction(String),
    #[error("remote task protocol error: {0}")]
    Protocol(String),
    #[error("remote task not found: {0}")]
    NotFound(String),
    /// 云端给出了本端不认识的任务状态，不可控制。
    #[error("remote task {0} is in a state this agent cannot control")]
    TaskUnavailable(String),
    #[error("invalid {field}: {message}")]
    InvalidArgument {
        field: &'static str,
        reason: Option<ErrorReason>,
        message: String,
    },
}

/// 等到下一条 `SessionChanged(Some)`；接收端 lag 时也返回，由调用方重新检查登录态。
async fn wait_for_session(events: &mut broadcast::Receiver<fluxdown_protocol::EventFrame>) {
    loop {
        match events.recv().await {
            Ok(frame) => {
                if let fluxdown_protocol::ServiceEvent::Agent(AgentEvent::SessionChanged(session)) =
                    &frame.event
                    && session.is_some()
                {
                    return;
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => return,
            Err(broadcast::error::RecvError::Closed) => std::future::pending::<()>().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::Router;
    use axum::body::Body;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode, header};
    use axum::response::IntoResponse;
    use axum::routing::{get, post};
    use fluxdown_protocol::{
        AgentEvent, CloudDevice, PathStyle, RemoteCommandAction, RemoteCommandParams,
        RemoteDispatchParams, RemoteTaskDto, RemoteTaskStatus, TaskDto,
    };
    use serde_json::{Value, json};
    use tokio::sync::Mutex;
    use tokio_util::sync::CancellationToken;

    use super::{
        BoundedIds, CreateFailure, IncomingCommand, MISSING_GRACE_ROUNDS, RemoteError,
        RemoteTaskService, SseEnd, accept_save_dir, apply_progress_items,
        create_with_directory_fallback, local_status, parse_task_list, progress_item,
        validate_target_save_dir,
    };
    use crate::state::{AgentState, CloudCredentials, StateStore};

    fn remote_task(value: Value) -> RemoteTaskDto {
        serde_json::from_value(value).expect("remote task")
    }

    #[test]
    fn rebind_score_requires_url_and_rewards_filename_and_directory() {
        let remote = serde_json::from_value::<RemoteTaskDto>(json!({
            "id": "r1", "url": "https://example.com/a", "fileName": "a.bin",
            "saveDir": "/tmp", "status": "pending"
        }))
        .expect("remote");
        let local = serde_json::from_value::<TaskDto>(json!({
            "taskId":"t1","url":"https://example.com/a","fileName":"a.bin",
            "saveDir":"/tmp","status":1,"downloadedBytes":0,"totalBytes":0,
            "errorMessage":"","createdAt":"1","proxyUrl":"","queueId":"main","checksum":""
        }))
        .expect("local");
        assert_eq!(RemoteTaskService::rebind_score(&remote, &local), 16);
        let unrelated = RemoteTaskDto {
            url: "https://other".to_owned(),
            status: RemoteTaskStatus::Pending,
            ..remote
        };
        assert_eq!(RemoteTaskService::rebind_score(&unrelated, &local), -1);
    }

    #[test]
    fn progress_events_read_task_id_clamp_values_and_skip_terminal_tasks() {
        let mut tasks = vec![
            remote_task(json!({"id": "r1", "status": "accepted"})),
            remote_task(json!({"id": "r2", "status": "completed", "progress": 1.0})),
        ];
        let items = [
            json!({"taskId": "r1", "downloadedBytes": 50, "speed": 7, "progress": 1.7}),
            // 旧字段名 `id` 不再被识别。
            json!({"id": "r1", "downloadedBytes": 999}),
            json!({"taskId": "r2", "downloadedBytes": 5, "progress": 0.1}),
            json!({"taskId": "missing", "downloadedBytes": 5}),
        ];
        assert!(apply_progress_items(&mut tasks, &items));
        assert_eq!(tasks[0].status, RemoteTaskStatus::Downloading);
        assert_eq!(tasks[0].downloaded_bytes, 50);
        assert_eq!(tasks[0].speed, 7);
        assert!(
            (tasks[0].progress - 1.0).abs() < f64::EPSILON,
            "progress is clamped to 1"
        );
        assert_eq!(tasks[1].status, RemoteTaskStatus::Completed);
        assert!((tasks[1].progress - 1.0).abs() < f64::EPSILON);

        assert!(!apply_progress_items(
            &mut tasks,
            &[json!({"taskId": "nope"})]
        ));
        assert!(!apply_progress_items(&mut tasks, &[json!({"id": "r1"})]));
    }

    #[test]
    fn reported_progress_is_clamped_and_never_negative() {
        let item = progress_item("r1", 200, 100, 5);
        assert!((item["progress"].as_f64().expect("progress") - 1.0).abs() < f64::EPSILON);
        let empty = progress_item("r1", -5, 0, -1);
        assert_eq!(empty["downloadedBytes"], 0);
        assert_eq!(empty["speed"], 0);
        assert!((empty["progress"].as_f64().expect("progress")).abs() < f64::EPSILON);
    }

    #[test]
    fn local_status_maps_preparing_to_downloading_and_queued_to_accepted() {
        assert_eq!(local_status(0), RemoteTaskStatus::Accepted);
        assert_eq!(local_status(1), RemoteTaskStatus::Downloading);
        assert_eq!(local_status(5), RemoteTaskStatus::Downloading);
        assert_eq!(local_status(2), RemoteTaskStatus::Paused);
        assert_eq!(local_status(3), RemoteTaskStatus::Completed);
        assert_eq!(local_status(4), RemoteTaskStatus::Failed);
    }

    #[test]
    fn malformed_task_entries_are_skipped_and_null_file_names_tolerated() {
        let value = json!({"tasks": [
            {"id": "ok", "fileName": null, "status": "pending"},
            {"noId": true},
            {"id": "future", "status": "brand-new-status"},
        ]});
        let tasks = parse_task_list(&value);
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].file_name, "");
        assert_eq!(tasks[1].status, RemoteTaskStatus::Unknown);
    }

    #[test]
    fn accept_directory_must_be_an_absolute_local_style_path_without_parent_segments() {
        assert_eq!(accept_save_dir(None), None);
        assert_eq!(accept_save_dir(Some("  ")), None);
        assert_eq!(accept_save_dir(Some("relative/dir")), None);
        assert_eq!(accept_save_dir(Some("/data/../etc")), None);
        if cfg!(windows) {
            assert_eq!(
                accept_save_dir(Some(r"D:\Downloads")),
                Some(r"D:\Downloads".to_owned())
            );
            assert_eq!(accept_save_dir(Some("/mnt/data")), None);
        } else {
            assert_eq!(
                accept_save_dir(Some("/mnt/data")),
                Some("/mnt/data".to_owned())
            );
            assert_eq!(accept_save_dir(Some(r"D:\Downloads")), None);
        }
    }

    #[test]
    fn accept_directory_rejects_unc_and_device_paths() {
        assert_eq!(accept_save_dir(Some(r"\\host\share\dl")), None);
        assert_eq!(accept_save_dir(Some(r"\\?\C:\dl")), None);
        assert_eq!(accept_save_dir(Some(r"\\.\pipe\x")), None);
        assert_eq!(accept_save_dir(Some("//host/share")), None);
    }

    #[tokio::test]
    async fn bound_tasks_are_not_failed_while_the_daemon_snapshot_is_unavailable() {
        let harness = Harness::new("cold_start", |router| router).await;
        bind(&harness, "r1", "l1").await;
        for _ in 0..(MISSING_GRACE_ROUNDS + 2) {
            harness
                .service
                .report_local_progress()
                .await
                .expect("progress round");
        }
        assert_eq!(bound_remote_ids(&harness).await, ["r1".to_owned()]);
        harness.finish().await;
    }

    #[tokio::test]
    async fn directory_failure_retries_without_directory_and_reports_the_last_error() {
        // 带目录失败 → 去掉目录重试成功。
        let attempts = Arc::new(Mutex::new(Vec::<Option<String>>::new()));
        let seen = attempts.clone();
        let created = create_with_directory_fallback(Some("/bad".to_owned()), move |dir| {
            let seen = seen.clone();
            async move {
                seen.lock().await.push(dir.clone());
                match dir {
                    Some(_) => Err(CreateFailure::permanent(
                        "cannot create directory".to_owned(),
                    )),
                    None => Ok("local-1".to_owned()),
                }
            }
        })
        .await;
        assert_eq!(created, Ok("local-1".to_owned()));
        assert_eq!(*attempts.lock().await, vec![Some("/bad".to_owned()), None]);

        // 不带目录也失败 → 返回最后一次错误（调用方据此上报 failed）。
        let failed = create_with_directory_fallback(Some("/bad".to_owned()), |dir| async move {
            Err::<String, _>(CreateFailure::permanent(format!("failed with {dir:?}")))
        })
        .await;
        assert_eq!(
            failed,
            Err(CreateFailure::permanent("failed with None".to_owned()))
        );

        // daemon 暂时不可用是瞬时错误：不会拿去掉目录再试一遍。
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let transient = create_with_directory_fallback(Some("/bad".to_owned()), move |_| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err::<String, _>(CreateFailure {
                    message: "daemon unavailable".to_owned(),
                    transient: true,
                })
            }
        })
        .await;
        assert!(transient.is_err_and(|failure| failure.transient));
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // 没有目录时不做多余重试。
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let failed = create_with_directory_fallback(None, move |_| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err::<String, _>(CreateFailure::permanent("no".to_owned()))
            }
        })
        .await;
        assert!(failed.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    fn device(value: Value) -> CloudDevice {
        serde_json::from_value(value).expect("cloud device")
    }

    #[test]
    fn dispatch_directory_is_validated_against_the_target_path_style() {
        let windows = device(json!({"id": "1", "deviceId": "w", "platform": "windows"}));
        assert!(validate_target_save_dir(Some(&windows), r"D:\Downloads").is_ok());
        let rejected = validate_target_save_dir(Some(&windows), "/mnt/data");
        assert!(matches!(
            rejected,
            Err(RemoteError::InvalidArgument {
                field: "saveDir",
                reason: Some(_),
                ..
            })
        ));

        // 目标自报风格优先于平台推断。
        let posix_reported = device(
            json!({"id": "2", "deviceId": "p", "platform": "windows", "pathStyle": "posix"}),
        );
        assert_eq!(
            posix_reported.effective_path_style(),
            Some(PathStyle::Posix)
        );
        assert!(validate_target_save_dir(Some(&posix_reported), "/srv/dl").is_ok());
        assert!(validate_target_save_dir(Some(&posix_reported), r"C:\dl").is_err());

        // 风格未知：放行，由执行端兜底。
        let unknown = device(json!({"id": "3", "deviceId": "u", "platform": "web"}));
        assert!(validate_target_save_dir(Some(&unknown), "anything").is_ok());
        assert!(validate_target_save_dir(None, "anything").is_ok());
    }

    #[test]
    fn command_id_window_is_bounded() {
        let mut ids = BoundedIds::default();
        for index in 0..(super::CONFIRMED_COMMAND_CAPACITY + 50) {
            ids.insert(format!("c{index}"));
        }
        assert!(!ids.contains("c0"));
        assert!(ids.contains(&format!("c{}", super::CONFIRMED_COMMAND_CAPACITY + 49)));
        assert_eq!(ids.order.len(), super::CONFIRMED_COMMAND_CAPACITY);
    }

    #[test]
    fn incoming_command_without_command_id_gets_a_unique_id_per_event() {
        let event = json!({"type": "task.command", "taskId": "r1", "action": "pause"});
        let first = IncomingCommand::from_event(&event).expect("first");
        let second = IncomingCommand::from_event(&event).expect("second");
        assert_ne!(first.command_id, second.command_id);
        assert!(!first.delete_files);
        let delete = IncomingCommand::from_event(
            &json!({"taskId": "r1", "action": "delete", "commandId": "c9", "deleteFiles": true}),
        )
        .expect("delete");
        assert_eq!(delete.command_id, "c9");
        assert!(delete.delete_files);
        assert_eq!(delete.action, RemoteCommandAction::Delete);
        assert!(
            IncomingCommand::from_event(&json!({"taskId": "r1", "action": "explode"})).is_err()
        );
    }

    #[derive(Default)]
    struct RemoteMockState {
        snapshots: AtomicUsize,
        events: AtomicUsize,
        presence: AtomicUsize,
        commands: Mutex<Vec<Value>>,
        statuses: Mutex<Vec<(String, Value)>>,
    }

    async fn mock_remote_snapshot(State(state): State<Arc<RemoteMockState>>) -> impl IntoResponse {
        state.snapshots.fetch_add(1, Ordering::SeqCst);
        axum::Json(json!({"tasks": []}))
    }

    async fn mock_remote_events(
        State(state): State<Arc<RemoteMockState>>,
        headers: HeaderMap,
    ) -> impl IntoResponse {
        assert_eq!(
            headers
                .get(header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer access")
        );
        state.events.fetch_add(1, Ordering::SeqCst);
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/event-stream")],
            "data: {\"type\":\"noop\"}\n\n",
        )
    }

    async fn mock_presence(State(state): State<Arc<RemoteMockState>>) -> impl IntoResponse {
        state.presence.fetch_add(1, Ordering::SeqCst);
        StatusCode::NO_CONTENT
    }

    async fn mock_command(
        State(state): State<Arc<RemoteMockState>>,
        axum::extract::Path(id): axum::extract::Path<String>,
        axum::Json(mut body): axum::Json<Value>,
    ) -> impl IntoResponse {
        body["taskId"] = json!(id);
        state.commands.lock().await.push(body);
        StatusCode::NO_CONTENT
    }

    async fn mock_status(
        State(state): State<Arc<RemoteMockState>>,
        axum::extract::Path(id): axum::extract::Path<String>,
        axum::Json(body): axum::Json<Value>,
    ) -> impl IntoResponse {
        state.statuses.lock().await.push((id, body));
        StatusCode::NO_CONTENT
    }

    type DaemonCalls = Arc<Mutex<Vec<(String, Option<Value>)>>>;

    struct Harness {
        service: Arc<RemoteTaskService>,
        state: Arc<Mutex<AgentState>>,
        store: Arc<StateStore>,
        mock: Arc<RemoteMockState>,
        /// 发给 daemon 的调用（仅 [`Harness::with_daemon`]；其余 harness 的 daemon 永远未连接）。
        daemon_calls: DaemonCalls,
        dir: std::path::PathBuf,
    }

    impl Harness {
        async fn new(
            label: &str,
            app: impl FnOnce(Router<Arc<RemoteMockState>>) -> Router<Arc<RemoteMockState>>,
        ) -> Self {
            Self::build(label, None, false, app).await
        }

        /// daemon 已连接：调用被记录并成功返回。
        async fn with_daemon(
            label: &str,
            app: impl FnOnce(Router<Arc<RemoteMockState>>) -> Router<Arc<RemoteMockState>>,
        ) -> Self {
            Self::build(label, None, true, app).await
        }

        async fn with_idle_timeout(
            label: &str,
            idle_timeout: Option<std::time::Duration>,
            app: impl FnOnce(Router<Arc<RemoteMockState>>) -> Router<Arc<RemoteMockState>>,
        ) -> Self {
            Self::build(label, idle_timeout, false, app).await
        }

        async fn build(
            label: &str,
            idle_timeout: Option<std::time::Duration>,
            daemon_connected: bool,
            app: impl FnOnce(Router<Arc<RemoteMockState>>) -> Router<Arc<RemoteMockState>>,
        ) -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind remote mock");
            let address = listener.local_addr().expect("remote mock address");
            let mock = Arc::new(RemoteMockState::default());
            let router = app(Router::new()).with_state(mock.clone());
            tokio::spawn(async move {
                let _ = axum::serve(listener, router).await;
            });
            let dir = std::env::temp_dir().join(format!(
                "fluxdown_remote_{label}_{}_{}",
                std::process::id(),
                uuid::Uuid::new_v4()
            ));
            let store = Arc::new(
                StateStore::open(dir.clone())
                    .await
                    .expect("remote state store"),
            );
            let initial = AgentState {
                device_id: "device-1".to_owned(),
                credentials: Some(CloudCredentials {
                    access_token: "access".to_owned(),
                    refresh_token: "refresh".to_owned(),
                    expires_at_unix: i64::MAX,
                    session: None,
                }),
                ..Default::default()
            };
            store.save(&initial).await.expect("save remote state");
            let state = Arc::new(Mutex::new(initial));
            let events =
                crate::event_hub::AgentEventHub::new(fluxdown_protocol::AgentSnapshot::default());
            let cloud = crate::cloud::CloudApi::new(
                crate::cloud::CloudClient::new(
                    format!("http://{address}"),
                    state.clone(),
                    store.clone(),
                )
                .expect("remote cloud client")
                .with_events(events.clone()),
            );
            let (daemon, daemon_calls) = if daemon_connected {
                crate::daemon_client::DaemonClient::recording()
            } else {
                (
                    crate::daemon_client::DaemonClient::disconnected(),
                    DaemonCalls::default(),
                )
            };
            let mut service = RemoteTaskService::new(
                cloud,
                Arc::new(daemon),
                events,
                state.clone(),
                store.clone(),
            );
            if let Some(idle_timeout) = idle_timeout {
                service.sse_idle_timeout = idle_timeout;
            }
            let service = Arc::new(service);
            Self {
                service,
                state,
                store,
                mock,
                daemon_calls,
                dir,
            }
        }

        async fn finish(self) {
            let Self {
                service,
                state,
                store,
                dir,
                ..
            } = self;
            drop(service);
            drop(state);
            drop(store);
            let _ = tokio::fs::remove_dir_all(dir).await;
        }
    }

    #[tokio::test]
    async fn worker_reconnects_sse_and_renews_presence_after_disconnect() {
        let harness = Harness::new("worker", |router| {
            router
                .route("/api/v1/tasks/remote", get(mock_remote_snapshot))
                .route("/api/v1/tasks/events", get(mock_remote_events))
                .route("/api/v1/tasks/presence", post(mock_presence))
        })
        .await;
        let cancel = CancellationToken::new();
        let worker = tokio::spawn(harness.service.clone().run(cancel.clone()));
        let mock = harness.mock.clone();
        tokio::time::timeout(std::time::Duration::from_secs(8), async {
            while mock.snapshots.load(Ordering::SeqCst) < 2
                || mock.events.load(Ordering::SeqCst) < 2
                || mock.presence.load(Ordering::SeqCst) < 2
            {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("remote worker reconnected SSE and heartbeat");
        cancel.cancel();
        worker.await.expect("join remote worker");
        harness.finish().await;
    }

    /// 只发响应头、之后一直沉默的 SSE：模拟合盖唤醒 / NAT 静默断流的半开连接。
    async fn silent_events() -> impl IntoResponse {
        let body = Body::from_stream(futures_util::stream::pending::<
            Result<axum::body::Bytes, std::convert::Infallible>,
        >());
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/event-stream")],
            body,
        )
    }

    #[tokio::test]
    async fn silent_sse_hits_the_idle_watchdog_even_while_periodic_timers_fire() {
        // 空闲阈值大于 1s 的进度上报周期：旧实现每个 tick 都会重建超时而永不触发。
        let harness = Harness::with_idle_timeout(
            "idle",
            Some(std::time::Duration::from_millis(1500)),
            |router| {
                router
                    .route("/api/v1/tasks/events", get(silent_events))
                    .route("/api/v1/tasks/presence", post(mock_presence))
            },
        )
        .await;
        let service = harness.service.clone();
        let response = service
            .cloud
            .remote_events("device-1")
            .await
            .expect("connect silent SSE");
        let cancel = CancellationToken::new();
        let (mut events, _) = service.events.subscribe_and_snapshot();
        let started = std::time::Instant::now();
        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(6),
            service.consume_events(response, &cancel, &mut events),
        )
        .await
        .expect("watchdog must end the stream before the test deadline");
        match outcome {
            Err(RemoteError::Protocol(message)) => assert!(message.contains("idle timeout")),
            other => panic!("expected idle timeout, got {other:?}"),
        }
        assert!(started.elapsed() >= std::time::Duration::from_millis(1400));
        drop(service);
        harness.finish().await;
    }

    #[tokio::test]
    async fn resync_event_ends_the_stream_for_a_full_reload() {
        async fn resync_events() -> impl IntoResponse {
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "text/event-stream")],
                "data: {\"type\":\"resync\"}\n\n",
            )
        }
        let harness = Harness::new("resync", |router| {
            router.route("/api/v1/tasks/events", get(resync_events))
        })
        .await;
        let response = harness
            .service
            .cloud
            .remote_events("device-1")
            .await
            .expect("connect SSE");
        let cancel = CancellationToken::new();
        let (mut events, _) = harness.service.events.subscribe_and_snapshot();
        let end = harness
            .service
            .consume_events(response, &cancel, &mut events)
            .await
            .expect("resync is a clean end");
        assert_eq!(end, SseEnd::Resync);
        harness.finish().await;
    }

    #[tokio::test]
    async fn malformed_sse_lines_do_not_tear_down_the_stream() {
        async fn noisy_events() -> impl IntoResponse {
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "text/event-stream")],
                "data: not-json\n\ndata: {\"type\":\"task.status\",\"nope\":1}\n\ndata: {\"type\":\"resync\"}\n\n",
            )
        }
        let harness = Harness::new("noisy", |router| {
            router.route("/api/v1/tasks/events", get(noisy_events))
        })
        .await;
        let response = harness
            .service
            .cloud
            .remote_events("device-1")
            .await
            .expect("connect SSE");
        let cancel = CancellationToken::new();
        let (mut events, _) = harness.service.events.subscribe_and_snapshot();
        let end = harness
            .service
            .consume_events(response, &cancel, &mut events)
            .await
            .expect("bad events are skipped");
        assert_eq!(end, SseEnd::Resync);
        harness.finish().await;
    }

    /// 依次取出已发布的 agent 事件（只保留会话相关的，便于断言顺序）。
    fn session_events(
        receiver: &mut tokio::sync::broadcast::Receiver<fluxdown_protocol::EventFrame>,
    ) -> Vec<String> {
        let mut seen = Vec::new();
        while let Ok(frame) = receiver.try_recv() {
            if let fluxdown_protocol::ServiceEvent::Agent(event) = frame.event {
                match event {
                    fluxdown_protocol::AgentEvent::SessionRevoked(reason) => {
                        seen.push(format!("revoked:{reason:?}"));
                    }
                    fluxdown_protocol::AgentEvent::SessionChanged(session) => {
                        seen.push(format!("session:{}", session.is_some()));
                    }
                    _ => {}
                }
            }
        }
        seen
    }

    #[tokio::test]
    async fn session_revoked_for_this_device_notifies_device_untrusted_before_the_session_clears() {
        let harness = Harness::new("revoked_local", command_mock).await;
        let (mut receiver, _) = harness.service.events.subscribe_and_snapshot();
        harness
            .service
            .apply_remote_event(json!({"type": "session.revoked", "deviceId": "device-1"}))
            .await
            .expect("handle session.revoked");
        assert_eq!(
            session_events(&mut receiver),
            ["revoked:DeviceUntrusted", "session:false"]
        );
        assert!(harness.state.lock().await.credentials.is_none());
        harness.finish().await;
    }

    #[tokio::test]
    async fn session_revoked_without_a_device_means_session_expired_and_other_devices_are_ignored()
    {
        let harness = Harness::new("revoked_all", command_mock).await;
        let (mut receiver, _) = harness.service.events.subscribe_and_snapshot();
        harness
            .service
            .apply_remote_event(json!({"type": "session.revoked", "deviceId": "someone-else"}))
            .await
            .expect("foreign device revoked");
        assert!(session_events(&mut receiver).is_empty());
        assert!(harness.state.lock().await.credentials.is_some());
        harness
            .service
            .apply_remote_event(json!({"type": "session.revoked"}))
            .await
            .expect("revoke all");
        assert_eq!(
            session_events(&mut receiver),
            ["revoked:SessionExpired", "session:false"]
        );
        // 已经登出后再收到撤销：不重复通知。
        harness
            .service
            .apply_remote_event(json!({"type": "session.revoked"}))
            .await
            .expect("revoke again");
        assert!(!session_events(&mut receiver).contains(&"revoked:SessionExpired".to_owned()));
        harness.finish().await;
    }

    fn command_mock(router: Router<Arc<RemoteMockState>>) -> Router<Arc<RemoteMockState>> {
        router
            .route("/api/v1/tasks/{id}/command", post(mock_command))
            .route("/api/v1/tasks/{id}/status", post(mock_status))
    }

    async fn seed_remote_task(harness: &Harness, status: &str) {
        harness
            .state
            .lock()
            .await
            .remote_tasks
            .push(remote_task(json!({
                "id": "r1", "toDevice": "device-2", "fromDevice": "device-1", "status": status
            })));
    }

    #[tokio::test]
    async fn repeating_the_same_action_reaches_the_cloud_every_time_but_a_reused_command_id_only_once()
     {
        let harness = Harness::new("commands", command_mock).await;
        seed_remote_task(&harness, "downloading").await;
        let pause = |command_id: Option<&str>| RemoteCommandParams {
            task_id: "r1".to_owned(),
            action: RemoteCommandAction::Pause,
            command_id: command_id.map(str::to_owned),
            delete_files: false,
        };
        // pause → pause：缺省 commandId 时每次都是新命令。
        harness
            .service
            .command(pause(None))
            .await
            .expect("first pause");
        harness
            .service
            .command(pause(None))
            .await
            .expect("second pause");
        assert_eq!(harness.mock.commands.lock().await.len(), 2);
        let commands = harness.mock.commands.lock().await.clone();
        assert_ne!(commands[0]["commandId"], commands[1]["commandId"]);
        assert_eq!(commands[0]["action"], "pause");

        // 显式 commandId 幂等：同一个 id 只下发一次。
        harness
            .service
            .command(pause(Some("fixed")))
            .await
            .expect("fixed once");
        harness
            .service
            .command(pause(Some("fixed")))
            .await
            .expect("fixed again");
        assert_eq!(harness.mock.commands.lock().await.len(), 3);
        harness.finish().await;
    }

    fn local_task(task_id: &str, status: i32) -> TaskDto {
        serde_json::from_value(json!({
            "taskId": task_id, "url": "https://example.com/a", "fileName": "a.bin",
            "saveDir": "/tmp", "status": status, "downloadedBytes": 0, "totalBytes": 0,
            "errorMessage": "", "createdAt": "1", "proxyUrl": "", "queueId": "main", "checksum": ""
        }))
        .expect("local task")
    }

    fn seed_daemon_tasks(harness: &Harness, tasks: Vec<TaskDto>) {
        harness
            .service
            .events
            .replace_daemon_snapshot(fluxdown_protocol::DaemonSnapshot {
                tasks,
                ..Default::default()
            });
        harness
            .service
            .events
            .publish(AgentEvent::DaemonConnectionChanged(true));
    }

    async fn bind(harness: &Harness, remote_id: &str, local_id: &str) {
        harness
            .state
            .lock()
            .await
            .remote_bindings
            .insert(remote_id.to_owned(), local_id.to_owned());
    }

    async fn bound_remote_ids(harness: &Harness) -> Vec<String> {
        let mut ids = harness
            .state
            .lock()
            .await
            .remote_bindings
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        ids.sort();
        ids
    }

    /// daemon 收到的删除调用：(本机任务 id, deleteFiles)，按任务 id 排序。
    async fn daemon_deletes(harness: &Harness) -> Vec<(String, bool)> {
        let mut deletes = harness
            .daemon_calls
            .lock()
            .await
            .iter()
            .filter(|(method, _)| method == fluxdown_protocol::method::DAEMON_TASK_DELETE)
            .map(|(_, params)| {
                let params = params.clone().unwrap_or_default();
                (
                    params["taskId"].as_str().unwrap_or_default().to_owned(),
                    params["deleteFiles"].as_bool().unwrap_or_default(),
                )
            })
            .collect::<Vec<_>>();
        deletes.sort();
        deletes
    }

    fn delete_params(command_id: Option<&str>) -> RemoteCommandParams {
        RemoteCommandParams {
            task_id: "r1".to_owned(),
            action: RemoteCommandAction::Delete,
            command_id: command_id.map(str::to_owned),
            delete_files: true,
        }
    }

    async fn not_found() -> impl IntoResponse {
        (
            StatusCode::NOT_FOUND,
            axum::Json(json!({"code": "not_found", "message": "gone"})),
        )
    }

    #[tokio::test]
    async fn delete_goes_through_the_cloud_command_for_any_status_and_drops_the_row_at_once() {
        let harness = Harness::new("delete", command_mock).await;
        for status in ["completed", "pending"] {
            seed_remote_task(&harness, status).await;
            harness
                .service
                .command(delete_params(None))
                .await
                .expect(status);
            assert!(harness.service.tasks().await.is_empty(), "{status}");
        }
        let commands = harness.mock.commands.lock().await.clone();
        assert_eq!(commands.len(), 2);
        assert!(
            commands
                .iter()
                .all(|command| command["action"] == "delete" && command["deleteFiles"] == true)
        );
        harness.finish().await;
    }

    #[tokio::test]
    async fn deleting_a_record_another_device_already_removed_still_succeeds() {
        let harness = Harness::new("delete_gone", |router| {
            router.route("/api/v1/tasks/{id}/command", post(not_found))
        })
        .await;
        seed_remote_task(&harness, "downloading").await;
        let pause = harness
            .service
            .command(RemoteCommandParams {
                action: RemoteCommandAction::Pause,
                delete_files: false,
                ..delete_params(None)
            })
            .await;
        assert!(matches!(pause, Err(RemoteError::Cloud(_))));
        harness
            .service
            .command(delete_params(None))
            .await
            .expect("an already deleted record counts as deleted");
        assert!(harness.service.tasks().await.is_empty());
        harness.finish().await;
    }

    #[tokio::test]
    async fn deleting_a_task_this_device_runs_honours_delete_files_and_ignores_the_echoed_command()
    {
        let harness = Harness::with_daemon("local_delete", command_mock).await;
        seed_daemon_tasks(&harness, vec![local_task("l1", 1)]);
        harness
            .state
            .lock()
            .await
            .remote_tasks
            .push(remote_task(json!({
                "id": "r1", "toDevice": "device-1", "fromDevice": "device-2", "status": "downloading"
            })));
        bind(&harness, "r1", "l1").await;

        harness
            .service
            .command(delete_params(Some("c1")))
            .await
            .expect("delete own task");
        assert_eq!(daemon_deletes(&harness).await, [("l1".to_owned(), true)]);
        let commands = harness.mock.commands.lock().await.clone();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0]["action"], "delete");
        let statuses = harness.mock.statuses.lock().await.clone();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].1["status"], "canceled");
        assert!(harness.service.tasks().await.is_empty());
        assert!(bound_remote_ids(&harness).await.is_empty());

        // 云端把同一条指令回显给本机：去重，不再触碰 daemon。
        harness
            .service
            .apply_remote_event(json!({
                "type": "task.command", "taskId": "r1", "toDevice": "device-1",
                "action": "delete", "commandId": "c1", "deleteFiles": true
            }))
            .await
            .expect("echoed command");
        assert_eq!(daemon_deletes(&harness).await.len(), 1);
        harness.finish().await;
    }

    #[tokio::test]
    async fn a_cancel_for_a_task_not_yet_accepted_succeeds_without_touching_the_daemon() {
        let harness = Harness::with_daemon("pending_cancel", command_mock).await;
        harness
            .state
            .lock()
            .await
            .remote_tasks
            .push(remote_task(json!({
                "id": "r1", "toDevice": "device-1", "fromDevice": "device-2",
                "url": "https://example.com/a", "status": "pending"
            })));
        seed_daemon_tasks(&harness, vec![local_task("unrelated", 1)]);
        harness
            .service
            .apply_remote_event(json!({
                "type": "task.command", "taskId": "r1", "toDevice": "device-1", "action": "cancel"
            }))
            .await
            .expect("nothing to cancel locally");
        assert!(harness.daemon_calls.lock().await.is_empty());
        harness.finish().await;
    }

    #[tokio::test]
    async fn a_task_this_device_runs_is_removed_locally_when_the_cloud_cancels_or_deletes_it() {
        let harness = Harness::with_daemon("executor_events", command_mock).await;
        seed_daemon_tasks(
            &harness,
            vec![
                local_task("l1", 1),
                local_task("l2", 2),
                local_task("l3", 3),
            ],
        );
        bind(&harness, "r1", "l1").await;
        bind(&harness, "r2", "l2").await;
        bind(&harness, "r3", "l3").await;

        for event in [
            json!({"type": "task.status", "id": "r1", "toDevice": "device-1",
                   "fromDevice": "device-2", "status": "canceled"}),
            json!({"type": "task.removed", "taskId": "r2"}),
            // 本机已下载完成的任务被删除记录：文件与本机任务都保留。
            json!({"type": "task.removed", "taskId": "r3"}),
        ] {
            harness
                .service
                .apply_remote_event(event)
                .await
                .expect("apply event");
        }
        assert_eq!(
            daemon_deletes(&harness).await,
            [("l1".to_owned(), false), ("l2".to_owned(), false)]
        );
        assert!(bound_remote_ids(&harness).await.is_empty());
        harness.finish().await;
    }

    #[tokio::test]
    async fn a_status_report_rejected_with_404_removes_the_local_task() {
        let harness = Harness::with_daemon("report_gone", |router| {
            router.route("/api/v1/tasks/{id}/status", post(not_found))
        })
        .await;
        seed_daemon_tasks(&harness, vec![local_task("l1", 1)]);
        bind(&harness, "r1", "l1").await;
        harness
            .service
            .report_status_if_changed("r1", RemoteTaskStatus::Downloading, None, None)
            .await
            .expect("a deleted record is not a report failure");
        assert_eq!(daemon_deletes(&harness).await, [("l1".to_owned(), false)]);
        assert!(bound_remote_ids(&harness).await.is_empty());
        harness.finish().await;
    }

    #[tokio::test]
    async fn reconnect_snapshot_removes_local_tasks_canceled_or_deleted_while_offline() {
        async fn snapshot() -> impl IntoResponse {
            axum::Json(json!({"tasks": [
                {"id": "r-canceled", "toDevice": "device-1", "fromDevice": "device-2", "status": "canceled"},
                {"id": "r-live", "toDevice": "device-1", "fromDevice": "device-2", "status": "downloading"}
            ]}))
        }
        let harness = Harness::with_daemon("executor_snapshot", |router| {
            router.route("/api/v1/tasks/remote", get(snapshot))
        })
        .await;
        seed_daemon_tasks(
            &harness,
            vec![
                local_task("l1", 1),
                local_task("l2", 1),
                local_task("l3", 1),
                local_task("l4", 0),
            ],
        );
        {
            let mut state = harness.state.lock().await;
            for id in ["r-deleted", "r-fresh"] {
                state.remote_tasks.push(remote_task(json!({
                    "id": id, "toDevice": "device-1", "fromDevice": "device-2", "status": "downloading"
                })));
            }
        }
        bind(&harness, "r-canceled", "l1").await;
        bind(&harness, "r-deleted", "l2").await;
        bind(&harness, "r-live", "l3").await;
        bind(&harness, "r-fresh", "l4").await;
        // r-fresh 在快照请求发出之后才下发并接单：快照里没有它不代表记录被删除。
        harness.service.runtime.lock().await.bound_at.insert(
            "r-fresh".to_owned(),
            std::time::Instant::now() + std::time::Duration::from_secs(3600),
        );

        let merged = harness
            .service
            .refresh_snapshot()
            .await
            .expect("refresh snapshot");
        assert_eq!(
            daemon_deletes(&harness).await,
            [("l1".to_owned(), false), ("l2".to_owned(), false)]
        );
        assert_eq!(bound_remote_ids(&harness).await, ["r-fresh", "r-live"]);
        let mut ids = merged
            .iter()
            .map(|task| task.id.as_str())
            .collect::<Vec<_>>();
        ids.sort_unstable();
        assert_eq!(ids, ["r-canceled", "r-fresh", "r-live"]);
        harness.finish().await;
    }

    #[tokio::test]
    async fn unknown_status_tasks_cannot_be_controlled() {
        let harness = Harness::new("unknown_status", command_mock).await;
        seed_remote_task(&harness, "brand-new-status").await;
        let result = harness
            .service
            .command(RemoteCommandParams {
                task_id: "r1".to_owned(),
                action: RemoteCommandAction::Pause,
                command_id: None,
                delete_files: false,
            })
            .await;
        assert!(matches!(result, Err(RemoteError::TaskUnavailable(_))));
        assert!(harness.mock.commands.lock().await.is_empty());
        harness.finish().await;
    }

    #[tokio::test]
    async fn accept_failure_is_reported_to_the_cloud_as_failed_with_the_error() {
        let harness = Harness::new("accept_failure", command_mock).await;
        let task = remote_task(json!({
            "id": "r1", "toDevice": "device-1", "fromDevice": "device-2",
            "url": "https://example.com/a.bin", "status": "pending"
        }));
        harness
            .service
            .report_accept_failure(&task, "save directory `/x` is unavailable: denied")
            .await;
        let statuses = harness.mock.statuses.lock().await.clone();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].0, "r1");
        assert_eq!(statuses[0].1["status"], "failed");
        assert!(
            statuses[0].1["error"]
                .as_str()
                .is_some_and(|error| error.contains("unavailable"))
        );
        harness.finish().await;
    }

    #[tokio::test]
    async fn an_unavailable_daemon_leaves_the_dispatch_pending_instead_of_failing_it() {
        let harness = Harness::new("accept_transient", command_mock).await;
        let task = remote_task(json!({
            "id": "r1", "toDevice": "device-1", "fromDevice": "device-2",
            "url": "https://example.com/a.bin", "status": "pending"
        }));
        harness.state.lock().await.remote_tasks.push(task);
        // 测试里的 daemon 客户端永远未连接。
        harness.service.accept_pending_dispatches().await;
        assert!(
            harness.mock.statuses.lock().await.is_empty(),
            "no failed report for a transient error"
        );
        assert!(harness.state.lock().await.remote_bindings.is_empty());
        harness.finish().await;
    }

    #[tokio::test]
    async fn a_persisted_binding_prevents_accepting_the_same_task_twice() {
        let harness = Harness::new("binding", command_mock).await;
        let task = remote_task(json!({
            "id": "r1", "toDevice": "device-1", "fromDevice": "device-2",
            "url": "https://example.com/a.bin", "status": "pending"
        }));
        {
            let mut state = harness.state.lock().await;
            state.remote_tasks.push(task);
            state
                .remote_bindings
                .insert("r1".to_owned(), "local-1".to_owned());
        }
        harness.service.accept_pending_dispatches().await;
        assert!(
            harness.mock.statuses.lock().await.is_empty(),
            "an already accepted task must not be created or reported again"
        );
        harness.finish().await;
    }

    #[tokio::test]
    async fn dispatch_rejects_the_local_device_and_foreign_style_directories_before_calling_the_cloud()
     {
        let harness = Harness::new("dispatch", command_mock).await;
        let to_self = harness
            .service
            .dispatch(RemoteDispatchParams {
                to_device: "device-1".to_owned(),
                url: "https://example.com/a".to_owned(),
                file_name: None,
                save_dir: None,
            })
            .await;
        assert!(matches!(
            to_self,
            Err(RemoteError::InvalidArgument {
                field: "toDevice",
                ..
            })
        ));

        harness
            .service
            .events
            .publish(fluxdown_protocol::AgentEvent::CloudDevicesChanged(vec![
                device(json!({"id": "1", "deviceId": "win", "platform": "windows"})),
            ]));
        let wrong_style = harness
            .service
            .dispatch(RemoteDispatchParams {
                to_device: "win".to_owned(),
                url: "https://example.com/a".to_owned(),
                file_name: None,
                save_dir: Some("/mnt/data".to_owned()),
            })
            .await;
        assert!(matches!(
            wrong_style,
            Err(RemoteError::InvalidArgument {
                field: "saveDir",
                reason: Some(_),
                ..
            })
        ));
        harness.finish().await;
    }
}
