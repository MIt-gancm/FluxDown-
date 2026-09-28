//! 任务实时传输状态与持久活动历史；不以分段数推算并发。

use serde::{Deserialize, Serialize};

/// 一个文件字节区间的进度，`active` 未知时不推断其传输状态。
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct TaskSegmentDto {
    pub index: i32,
    pub start_byte: i64,
    pub end_byte: i64,
    pub downloaded_bytes: i64,
    pub active: Option<bool>,
}

/// 任务最新采样；活跃传输不是物理 socket、配置上限或分段数。
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct TaskRuntimeDto {
    pub task_id: String,
    pub sampled_at_ms: i64,
    /// 源端分配的单调采样序号；0 仅供未提供序号的旧客户端回退。
    #[serde(default)]
    pub sample_sequence: u64,
    pub active_transfers: Option<u32>,
    pub connected_peers: Option<u32>,
    pub parallelism_limit: Option<u32>,
    pub total_bytes: i64,
    pub segments: Vec<TaskSegmentDto>,
}

/// 持久任务事件；ID 在同一个下载数据库内单调递增。
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct TaskActivityDto {
    pub id: i64,
    pub task_id: String,
    pub timestamp_ms: i64,
    pub kind: String,
    pub message: String,
    pub status: Option<i32>,
}

/// 默认返回最近一页；`before_id` 向前翻页，`after_id` 用于断线补齐，两者互斥。
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct TaskActivityQuery {
    pub task_id: String,
    #[serde(default)]
    pub before_id: Option<i64>,
    #[serde(default)]
    pub after_id: Option<i64>,
    /// 0 使用服务端默认值；服务端另设硬上限。
    #[serde(default)]
    pub limit: u32,
}

/// 页内按 ID 升序；保留范围用于标明历史被清理，不伪装成完整历史。
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct TaskActivityPage {
    pub entries: Vec<TaskActivityDto>,
    pub has_more: bool,
    pub oldest_id: Option<i64>,
    pub newest_id: Option<i64>,
    pub truncated: bool,
}
