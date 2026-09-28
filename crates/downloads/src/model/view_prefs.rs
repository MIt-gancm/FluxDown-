//! 下载页视图偏好（全局单套，设备本地，键 `desktop.downloads.view`）。

use std::cmp::Ordering;

use chrono::{Datelike, Local, NaiveDate, TimeZone};
use fluxdown_ui_theme::DensityTokens;
use gpui::Pixels;
use serde::{Deserialize, Serialize};

use super::{DownloadTaskView, TaskState};

pub(crate) const VIEW_PREFS_KEY: &str = "desktop.downloads.view";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ViewDensity {
    #[default]
    Comfortable,
    Compact,
}

impl ViewDensity {
    /// 任务行高：舒适双行取 `density.taskRow`（默认 44 = 13/18 正文 + 12/16 元信息 + 上下留白），
    /// 紧凑单行取 `density.taskRowCompact`（默认 30）。
    pub(crate) fn row_height(self, density: &DensityTokens) -> Pixels {
        match self {
            Self::Comfortable => density.task_row,
            Self::Compact => density.task_row_compact,
        }
    }

    /// 是否渲染第二行元信息（名称列的类别 · 域名、状态列的详情）。
    pub(crate) fn two_line(self) -> bool {
        self == Self::Comfortable
    }

    pub(crate) fn next(self) -> Self {
        match self {
            Self::Comfortable => Self::Compact,
            Self::Compact => Self::Comfortable,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ViewGroupBy {
    #[default]
    None,
    Status,
    Date,
    Type,
    Queue,
    Site,
    Group,
}

impl ViewGroupBy {
    const CYCLE: [Self; 7] = [
        Self::None,
        Self::Status,
        Self::Date,
        Self::Type,
        Self::Queue,
        Self::Site,
        Self::Group,
    ];

    pub(crate) fn next(self) -> Self {
        let ix = Self::CYCLE
            .iter()
            .position(|item| *item == self)
            .unwrap_or(0);
        Self::CYCLE[(ix + 1) % Self::CYCLE.len()]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ViewSortKey {
    #[default]
    Smart,
    Created,
    Name,
    Size,
    Progress,
    Speed,
}

impl ViewSortKey {
    const CYCLE: [Self; 6] = [
        Self::Smart,
        Self::Created,
        Self::Name,
        Self::Size,
        Self::Progress,
        Self::Speed,
    ];

    pub(crate) fn next(self) -> Self {
        let ix = Self::CYCLE
            .iter()
            .position(|item| *item == self)
            .unwrap_or(0);
        Self::CYCLE[(ix + 1) % Self::CYCLE.len()]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SortDir {
    Asc,
    #[default]
    Desc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DetailPlacement {
    #[default]
    Bottom,
    Right,
}

impl DetailPlacement {
    pub(crate) fn toggled(self) -> Self {
        match self {
            Self::Bottom => Self::Right,
            Self::Right => Self::Bottom,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct ColumnPref {
    /// 列键（`DownloadColumnKind::key`）。
    pub(crate) key: String,
    pub(crate) visible: bool,
    #[serde(default)]
    pub(crate) width: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ViewPrefs {
    pub(crate) density: ViewDensity,
    pub(crate) group_by: ViewGroupBy,
    pub(crate) sort_key: ViewSortKey,
    pub(crate) sort_dir: SortDir,
    /// 空 = 使用默认列集。
    pub(crate) columns: Vec<ColumnPref>,
    /// 文件名列拖过后的固定宽度；`None` = 吸收表格剩余宽度（默认）。
    pub(crate) file_name_width: Option<f32>,
    pub(crate) detail_placement: DetailPlacement,
    pub(crate) detail_open: bool,
    pub(crate) detail_size: f32,
    pub(crate) sidebar_width: f32,
    pub(crate) collapsed_groups: Vec<String>,
}

impl Default for ViewPrefs {
    fn default() -> Self {
        Self {
            density: ViewDensity::default(),
            group_by: ViewGroupBy::default(),
            sort_key: ViewSortKey::default(),
            sort_dir: SortDir::default(),
            columns: Vec::new(),
            file_name_width: None,
            detail_placement: DetailPlacement::default(),
            detail_open: false,
            detail_size: 260.,
            sidebar_width: 200.,
            collapsed_groups: Vec::new(),
        }
    }
}

impl ViewPrefs {
    pub(crate) fn from_value(value: &serde_json::Value) -> Self {
        serde_json::from_value(value.clone()).unwrap_or_default()
    }

    pub(crate) fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    pub(crate) fn cycle_density(&mut self) {
        self.density = self.density.next();
    }

    pub(crate) fn cycle_group_by(&mut self) {
        self.group_by = self.group_by.next();
    }

    pub(crate) fn cycle_sort(&mut self) {
        self.sort_key = self.sort_key.next();
    }

    pub(crate) fn is_group_collapsed(&self, key: &str) -> bool {
        self.collapsed_groups.iter().any(|item| item == key)
    }

    pub(crate) fn toggle_group_collapsed(&mut self, key: &str) {
        if let Some(ix) = self.collapsed_groups.iter().position(|item| item == key) {
            self.collapsed_groups.remove(ix);
        } else {
            self.collapsed_groups.push(key.to_owned());
        }
    }

    /// 排序比较：`Smart` = 状态优先级再创建时间倒序；其他键按 `sort_dir`。
    pub(crate) fn compare(&self, left: &DownloadTaskView, right: &DownloadTaskView) -> Ordering {
        let ordering = match self.sort_key {
            ViewSortKey::Smart => {
                return left
                    .state
                    .smart_rank()
                    .cmp(&right.state.smart_rank())
                    .then_with(|| right.created_at_secs.cmp(&left.created_at_secs))
                    .then_with(|| left.key.cmp(&right.key));
            }
            ViewSortKey::Created => left.created_at_secs.cmp(&right.created_at_secs),
            ViewSortKey::Name => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
            ViewSortKey::Size => left.size_bytes.cmp(&right.size_bytes),
            ViewSortKey::Progress => left
                .progress
                .partial_cmp(&right.progress)
                .unwrap_or(Ordering::Equal),
            ViewSortKey::Speed => left
                .speed_bytes_per_second
                .unwrap_or(0)
                .cmp(&right.speed_bytes_per_second.unwrap_or(0)),
        };
        let ordering = match self.sort_dir {
            SortDir::Asc => ordering,
            SortDir::Desc => ordering.reverse(),
        };
        ordering.then_with(|| left.key.cmp(&right.key))
    }
}

/// 日期分组桶（基于本地日期）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum DateBucket {
    Today,
    Yesterday,
    ThisWeek,
    ThisMonth,
    Older,
}

impl DateBucket {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Today => "today",
            Self::Yesterday => "yesterday",
            Self::ThisWeek => "this_week",
            Self::ThisMonth => "this_month",
            Self::Older => "older",
        }
    }

    pub(crate) fn of(created_at_secs: i64) -> Self {
        Self::relative_to(created_at_secs, Local::now().date_naive())
    }

    fn relative_to(created_at_secs: i64, today: NaiveDate) -> Self {
        let Some(created) = Local
            .timestamp_opt(created_at_secs, 0)
            .single()
            .map(|value| value.date_naive())
        else {
            return Self::Older;
        };
        if created >= today {
            Self::Today
        } else if created == today.pred_opt().unwrap_or(today) {
            Self::Yesterday
        } else if created.iso_week() == today.iso_week() && created.year() == today.year() {
            Self::ThisWeek
        } else if created.month() == today.month() && created.year() == today.year() {
            Self::ThisMonth
        } else {
            Self::Older
        }
    }
}

/// 状态分组键（稳定，用于折叠记忆）。
pub(crate) fn state_group_key(state: TaskState) -> &'static str {
    match state {
        TaskState::Pending => "pending",
        TaskState::Downloading => "downloading",
        TaskState::Paused => "paused",
        TaskState::Completed => "completed",
        TaskState::Failed => "failed",
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Local, NaiveDate, TimeZone};

    use super::{DateBucket, SortDir, ViewDensity, ViewGroupBy, ViewPrefs, ViewSortKey};
    use crate::model::DownloadTaskView;

    fn task(id: &str, status: i32, created: &str, size: i64) -> DownloadTaskView {
        let dto = serde_json::from_value::<fluxdown_protocol::TaskDto>(serde_json::json!({
            "taskId":id,"url":"https://example.com/x","fileName":format!("{id}.bin"),
            "saveDir":"/tmp","status":status,"downloadedBytes":0,"totalBytes":size,
            "errorMessage":"","createdAt":created,"proxyUrl":"","queueId":"main","checksum":""
        }))
        .expect("task");
        DownloadTaskView::local(&dto, None, false)
    }

    #[test]
    fn prefs_round_trip_and_tolerate_unknown_fields() {
        let mut prefs = ViewPrefs::default();
        prefs.density = ViewDensity::Compact;
        prefs.group_by = ViewGroupBy::Site;
        prefs.sort_key = ViewSortKey::Size;
        prefs.sort_dir = SortDir::Asc;
        prefs.collapsed_groups.push("x".to_owned());
        let value = prefs.to_value();
        assert_eq!(ViewPrefs::from_value(&value), prefs);
        assert_eq!(
            ViewPrefs::from_value(&serde_json::json!({"density":"compact","bogus":1})).density,
            ViewDensity::Compact
        );
        assert_eq!(
            ViewPrefs::from_value(&serde_json::json!("garbage")),
            ViewPrefs::default()
        );
    }

    #[test]
    fn smart_sort_ranks_active_first_then_newest() {
        let prefs = ViewPrefs::default();
        let mut rows = vec![
            task("done", 3, "50", 1),
            task("old-dl", 1, "10", 1),
            task("new-dl", 1, "20", 1),
            task("paused", 2, "30", 1),
            task("failed", 4, "40", 1),
            task("pending", 0, "5", 1),
        ];
        rows.sort_by(|a, b| prefs.compare(a, b));
        let order: Vec<&str> = rows.iter().map(|row| row.key.task_id()).collect();
        assert_eq!(
            order,
            ["new-dl", "old-dl", "pending", "paused", "failed", "done"]
        );
    }

    #[test]
    fn date_buckets_use_local_day_boundaries() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 18).expect("date"); // Wednesday
        let at = |y, m, d, h| {
            Local
                .with_ymd_and_hms(y, m, d, h, 0, 0)
                .single()
                .expect("ts")
                .timestamp()
        };
        assert_eq!(
            DateBucket::relative_to(at(2026, 3, 18, 0), today),
            DateBucket::Today
        );
        assert_eq!(
            DateBucket::relative_to(at(2026, 3, 17, 23), today),
            DateBucket::Yesterday
        );
        assert_eq!(
            DateBucket::relative_to(at(2026, 3, 16, 1), today),
            DateBucket::ThisWeek
        );
        assert_eq!(
            DateBucket::relative_to(at(2026, 3, 2, 1), today),
            DateBucket::ThisMonth
        );
        assert_eq!(
            DateBucket::relative_to(at(2026, 2, 27, 1), today),
            DateBucket::Older
        );
    }
}
