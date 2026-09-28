//! agent 自有状态与 daemon 缓存的原子投影和事件序列。

use std::sync::{Arc, Mutex, MutexGuard};

use fluxdown_protocol::{
    AgentEvent, AgentSnapshot, DaemonEvent, DaemonSnapshot, EventFrame, ServiceEvent, Snapshot,
    SnapshotBody, apply_agent_event,
};
use tokio::sync::broadcast;
use uuid::Uuid;

struct AgentEventState {
    epoch: String,
    sequence: u64,
    snapshot: AgentSnapshot,
}

/// agent 物化投影的唯一同步边界。
#[derive(Clone)]
pub struct AgentEventHub {
    state: Arc<Mutex<AgentEventState>>,
    events: broadcast::Sender<EventFrame>,
}

impl AgentEventHub {
    #[must_use]
    pub fn new(snapshot: AgentSnapshot) -> Self {
        let (events, _) = broadcast::channel(1024);
        Self {
            state: Arc::new(Mutex::new(AgentEventState {
                epoch: Uuid::new_v4().to_string(),
                sequence: 0,
                snapshot,
            })),
            events,
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        let state = lock_or_recover(&self.state);
        Snapshot {
            epoch: state.epoch.clone(),
            sequence: state.sequence,
            body: SnapshotBody::Agent(Box::new(state.snapshot.clone())),
        }
    }

    #[must_use]
    pub fn subscribe_and_snapshot(&self) -> (broadcast::Receiver<EventFrame>, Snapshot) {
        let receiver = self.events.subscribe();
        (receiver, self.snapshot())
    }

    /// 在临界区内只读访问当前投影。高频事件路径用它按需提取少量字段，
    /// 避免每帧克隆整份 `AgentSnapshot`。闭包内不得再调用本 hub（非重入锁）。
    pub fn inspect<R>(&self, read: impl FnOnce(&AgentSnapshot) -> R) -> R {
        let state = lock_or_recover(&self.state);
        read(&state.snapshot)
    }

    /// daemon 增量先更新缓存，再发布 agent sequence。
    pub fn apply_daemon_event(&self, event: DaemonEvent) -> EventFrame {
        self.publish(AgentEvent::Daemon(event))
    }

    /// daemon 重连快照原子替换并发布替换事件。
    pub fn replace_daemon_snapshot(&self, snapshot: DaemonSnapshot) -> EventFrame {
        self.publish(AgentEvent::DaemonSnapshotReplaced(snapshot))
    }

    pub fn publish(&self, event: AgentEvent) -> EventFrame {
        let mut state = lock_or_recover(&self.state);
        apply_agent_event(&mut state.snapshot, &event);
        state.sequence = state.sequence.saturating_add(1);
        let frame = EventFrame {
            epoch: state.epoch.clone(),
            sequence: state.sequence,
            event: ServiceEvent::Agent(event),
        };
        let _ = self.events.send(frame.clone());
        frame
    }
}

fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use fluxdown_protocol::{
        AgentSnapshot, DaemonEvent, DaemonSnapshot, SnapshotBody, TaskDto, WsServerMsg,
    };

    use super::AgentEventHub;

    #[test]
    fn daemon_snapshot_then_delta_share_agent_sequence_and_preserve_metadata()
    -> Result<(), serde_json::Error> {
        let task = serde_json::from_value::<TaskDto>(serde_json::json!({
            "taskId": "task-1",
            "url": "https://example.com/file",
            "fileName": "file.bin",
            "saveDir": "/tmp",
            "status": 0,
            "downloadedBytes": 0,
            "totalBytes": 100,
            "errorMessage": "",
            "createdAt": "1",
            "proxyUrl": "",
            "queueId": "main",
            "checksum": ""
        }))?;
        let hub = AgentEventHub::new(AgentSnapshot::default());
        let replaced = hub.replace_daemon_snapshot(DaemonSnapshot {
            tasks: vec![task],
            ..DaemonSnapshot::default()
        });
        let delta = hub.apply_daemon_event(DaemonEvent::Engine(WsServerMsg::TaskProgress {
            task_id: "task-1".to_owned(),
            status: 5,
            downloaded_bytes: 1,
            total_bytes: 100,
            speed: 0,
            file_name: String::new(),
            save_dir: String::new(),
            upload_speed: 0,
            url: String::new(),
            error_message: String::new(),
            uploaded_bytes: 0,
            seeding_status: 0,
            seeding_message: String::new(),
            seeding_time_secs: 0,
        }));
        assert_eq!(delta.sequence, replaced.sequence + 1);
        assert_eq!(delta.epoch, replaced.epoch);
        let SnapshotBody::Agent(snapshot) = hub.snapshot().body else {
            panic!("agent hub returned daemon root snapshot");
        };
        assert_eq!(snapshot.daemon.tasks[0].url, "https://example.com/file");
        assert_eq!(snapshot.daemon.tasks[0].save_dir, "/tmp");
        assert_eq!(snapshot.daemon.tasks[0].status, 5);
        Ok(())
    }
}
