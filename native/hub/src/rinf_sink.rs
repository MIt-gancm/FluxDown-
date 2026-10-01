//! `EventSink` 实现 —— 把 `EngineEvent` 变体转发为具体 Dart 信号。
//!
//! 这是 hub 内 `.send_signal_to_dart()` 调用点的收敛，内容是搬移而非新写业务逻辑。

use fluxdown_engine::events::{EngineEvent, EventSink};
use rinf::RustSignal;

use crate::signals;

/// 桥接 `EngineEvent` 到 `hub::signals::*` 具体信号类型的 `EventSink` 实现。
#[derive(Default)]
pub struct RinfEventSink;

impl RinfEventSink {
    pub fn new() -> Self {
        Self
    }
}

impl EventSink for RinfEventSink {
    fn emit(&self, event: EngineEvent) {
        match event {
            EngineEvent::TaskProgress {
                task_id,
                status,
                downloaded_bytes,
                total_bytes,
                speed,
                upload_speed_bps,
                uploaded_bytes,
                seeding_status,
                seeding_message,
                seeding_time_secs,
                file_name,
                save_dir,
                url,
                error_message,
            } => {
                signals::TaskProgress {
                    task_id,
                    status,
                    downloaded_bytes,
                    total_bytes,
                    speed,
                    file_name,
                    save_dir,
                    url,
                    error_message,
                    upload_speed_bps,
                    uploaded_bytes,
                    seeding_status,
                    seeding_message,
                    seeding_time_secs,
                }
                .send_signal_to_dart();
            }
            EngineEvent::TasksSnapshot(tasks) => {
                signals::AllTasks {
                    tasks: tasks.into_iter().map(Into::into).collect(),
                }
                .send_signal_to_dart();
            }
            EngineEvent::SegmentProgress {
                task_id,
                total_bytes,
                segment_count,
                segments,
            } => {
                signals::SegmentProgress {
                    task_id,
                    total_bytes,
                    segment_count,
                    segments: segments.into_iter().map(Into::into).collect(),
                }
                .send_signal_to_dart();
            }
            EngineEvent::TaskMetaProbed {
                task_id,
                file_name,
                total_bytes,
            } => {
                signals::TaskMetaProbed {
                    task_id,
                    file_name,
                    total_bytes,
                }
                .send_signal_to_dart();
            }
            EngineEvent::QueuePositionsChanged(positions) => {
                signals::QueuePositionsUpdate {
                    positions: positions.into_iter().map(Into::into).collect(),
                }
                .send_signal_to_dart();
            }
            EngineEvent::QueuesChanged(queues) => {
                signals::AllQueues {
                    queues: queues.into_iter().map(Into::into).collect(),
                }
                .send_signal_to_dart();
            }
            EngineEvent::TaskQueueChanged { task_id, queue_id } => {
                signals::TaskQueueChanged { task_id, queue_id }.send_signal_to_dart();
            }
            EngineEvent::TaskRouteChanged { task_id, route } => {
                signals::TaskRouteChanged { task_id, route }.send_signal_to_dart();
            }
            EngineEvent::PriorityTaskChanged {
                priority_task_id,
                auto_paused_count,
            } => {
                signals::PriorityTaskChanged {
                    priority_task_id,
                    auto_paused_count,
                }
                .send_signal_to_dart();
            }
            EngineEvent::SegmentSplit {
                task_id,
                parent_index,
                parent_new_end,
                child_index,
                child_start,
                child_end,
                is_proactive,
                total_segments,
            } => {
                signals::SegmentSplitEvent {
                    task_id,
                    parent_index,
                    parent_new_end,
                    child_index,
                    child_start,
                    child_end,
                    is_proactive,
                    total_segments,
                }
                .send_signal_to_dart();
            }
            EngineEvent::FileMissingChanged(updates) => {
                signals::FileMissingChanged {
                    updates: updates
                        .into_iter()
                        .map(|(task_id, missing)| signals::FileMissingUpdate { task_id, missing })
                        .collect(),
                }
                .send_signal_to_dart();
            }

            // 任务组仅桌面端（GPUI/daemon）使用；移动端 Flutter 不消费分组快照。
            EngineEvent::GroupsChanged(_) => {}
            EngineEvent::TaskCdnEvent {
                task_id,
                kind,
                host,
                nodes,
                ip,
                reason,
                candidates,
                alive,
                cap,
                auto_cap,
            } => {
                signals::TaskCdnEvent {
                    task_id,
                    kind,
                    host,
                    nodes: nodes.into_iter().map(Into::into).collect(),
                    ip,
                    reason,
                    candidates,
                    alive,
                    cap,
                    auto_cap,
                }
                .send_signal_to_dart();
            }

            // 移动端 Dart 不消费的事件（BT 完成通知源 / 重复种子 / 解析预览 / RSS /
            // webhook / 插件）：引擎仍会发出，这里静默丢弃。
            EngineEvent::BtDataFinished { .. }
            | EngineEvent::PluginAutoDisabled { .. }
            | EngineEvent::PluginHookActivity { .. }
            | EngineEvent::DuplicateTorrentDetected { .. }
            | EngineEvent::ResolvePreviewReady { .. }
            | EngineEvent::RssSourcesChanged(_)
            | EngineEvent::RssItemsChanged { .. }
            | EngineEvent::RssFeedValidated { .. }
            | EngineEvent::WebhookDeliveriesChanged(_) => {}
            // Flutter 继续消费 TaskProgress / SegmentProgress；活动由 JournalSink 落库，
            // 这两种扩展通知无需重复投影，也不能对每帧同步写日志。
            EngineEvent::TaskRuntimeChanged(_) | EngineEvent::TaskActivityAdded(_) => {}
            // `#[non_exhaustive]`：未来新增变体默认丢弃并记录日志，而非编译失败。
            _ => {
                crate::logger::log_info!(
                    "[rinf-sink] unhandled EngineEvent variant (added after this match was written)"
                );
            }
        }
    }
}
