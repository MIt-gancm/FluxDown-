//! 详情面板的持久活动游标：历史页与实时事件按源端 ID 合并。

use std::collections::BTreeMap;

use fluxdown_protocol::{TaskActivityDto, TaskActivityPage, TaskActivityQuery};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fetch {
    Latest,
    After,
    Before,
}

#[derive(Clone, Debug)]
pub(super) struct Ticket {
    generation: u64,
    serial: u64,
    fetch: Fetch,
    pub query: TaskActivityQuery,
}

#[derive(Default)]
pub(super) struct ActivityFeed {
    task_id: String,
    generation: u64,
    serial: u64,
    entries: BTreeMap<i64, TaskActivityDto>,
    cursor: i64,
    loaded: bool,
    has_older: bool,
    oldest_retained: Option<i64>,
    newest_retained: Option<i64>,
    truncated: bool,
    journal_gap: bool,
    pending: Option<Fetch>,
    in_flight: Option<Ticket>,
    failed: Option<Fetch>,
    /// 事件可能先于页 RPC 的结果到达；页的游标只能由查询结果推进。
    live_during_fetch: i64,
}

impl ActivityFeed {
    pub fn new(task_id: String) -> Self {
        Self {
            task_id,
            pending: Some(Fetch::Latest),
            ..Self::default()
        }
    }

    pub fn switch_task(&mut self, task_id: String) {
        let generation = self.generation.wrapping_add(1);
        *self = Self::new(task_id);
        self.generation = generation;
    }

    pub fn entries(&self) -> impl DoubleEndedIterator<Item = &TaskActivityDto> {
        self.entries.values()
    }

    pub fn is_loading(&self) -> bool {
        self.in_flight.is_some()
    }

    pub fn loaded(&self) -> bool {
        self.loaded
    }

    pub fn failed(&self) -> bool {
        self.failed.is_some()
    }

    pub fn has_older(&self) -> bool {
        self.loaded && self.has_older
    }

    pub fn retained_range(&self) -> (Option<i64>, Option<i64>, bool) {
        (self.oldest_retained, self.newest_retained, self.truncated)
    }

    pub fn has_journal_gap(&self) -> bool {
        self.journal_gap
    }

    pub fn suspend(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.in_flight = None;
        self.live_during_fetch = 0;
    }

    pub fn reconnect(&mut self) {
        self.suspend();
        if self.loaded {
            self.pending = Some(Fetch::After);
        } else {
            self.pending = Some(Fetch::Latest);
        }
        self.failed = None;
    }

    pub fn load_older(&mut self) {
        if self.has_older && self.in_flight.is_none() && self.pending.is_none() {
            self.pending = Some(Fetch::Before);
            self.failed = None;
        }
    }

    pub fn retry(&mut self) {
        if let Some(fetch) = self.failed.take() {
            self.pending = Some(fetch);
        }
    }

    pub fn add(&mut self, entry: TaskActivityDto) -> bool {
        if entry.task_id != self.task_id || entry.id <= 0 {
            return false;
        }
        let id = entry.id;
        self.journal_gap |= entry.kind == "journal_overflow";
        let changed = self.entries.insert(id, entry).is_none();
        if self
            .in_flight
            .as_ref()
            .is_some_and(|ticket| ticket.fetch != Fetch::Before)
        {
            self.live_during_fetch = self.live_during_fetch.max(id);
        }
        changed
    }

    pub fn begin(&mut self) -> Option<Ticket> {
        if self.in_flight.is_some() || self.failed.is_some() {
            return None;
        }
        let fetch = self.pending.take()?;
        let before_id = (fetch == Fetch::Before)
            .then(|| self.entries.first_key_value().map(|(&id, _)| id))
            .flatten();
        if fetch == Fetch::Before && before_id.is_none() {
            self.has_older = false;
            return None;
        }
        self.serial = self.serial.wrapping_add(1);
        let ticket = Ticket {
            generation: self.generation,
            serial: self.serial,
            fetch,
            query: TaskActivityQuery {
                task_id: self.task_id.clone(),
                before_id,
                after_id: (fetch == Fetch::After).then_some(self.cursor),
                limit: 0,
            },
        };
        self.live_during_fetch = 0;
        self.in_flight = Some(ticket.clone());
        Some(ticket)
    }

    /// 只接受本任务本世代的结果；查询期间到达的实时事件永不被页覆盖。
    pub fn finish(&mut self, ticket: &Ticket, page: Option<TaskActivityPage>) -> bool {
        if !self.in_flight.as_ref().is_some_and(|current| {
            current.generation == ticket.generation
                && current.serial == ticket.serial
                && current.query.task_id == ticket.query.task_id
        }) {
            return false;
        }
        self.in_flight = None;
        let Some(page) = page else {
            self.failed = Some(ticket.fetch);
            return true;
        };
        self.failed = None;
        if ticket.fetch == Fetch::After
            && self.cursor > 0
            && page.newest_id.is_none_or(|newest| newest < self.cursor)
        {
            // 数据库被重建或该任务的保留记录全部过期；旧游标不属于新历史。
            let task_id = self.task_id.clone();
            self.switch_task(task_id);
            return true;
        }
        self.oldest_retained = page.oldest_id;
        self.newest_retained = page.newest_id;
        self.truncated |= page.truncated;
        if let Some(oldest) = self.oldest_retained {
            if self
                .entries
                .first_key_value()
                .is_some_and(|(&id, _)| id < oldest)
            {
                self.truncated = true;
            }
            self.entries = self.entries.split_off(&oldest);
        }
        let last_page_id = page.entries.last().map(|entry| entry.id).unwrap_or(0);
        for entry in page.entries {
            if entry.task_id == self.task_id && entry.id > 0 {
                self.journal_gap |= entry.kind == "journal_overflow";
                self.entries.insert(entry.id, entry);
            }
        }
        match ticket.fetch {
            Fetch::Latest => {
                self.loaded = true;
                self.has_older = page.has_more;
                self.cursor = self.cursor.max(last_page_id);
                if self.live_during_fetch > self.cursor {
                    self.pending = Some(Fetch::After);
                }
            }
            Fetch::After => {
                // 不能以实时事件的最大 ID 推进游标：中间尚未取回的记录会被跳过。
                self.cursor = self.cursor.max(last_page_id);
                if page.has_more && last_page_id <= ticket.query.after_id.unwrap_or_default() {
                    self.failed = Some(Fetch::After);
                } else if page.has_more || self.live_during_fetch > self.cursor {
                    self.pending = Some(Fetch::After);
                }
            }
            Fetch::Before => self.has_older = page.has_more,
        }
        self.live_during_fetch = 0;
        true
    }
}

#[cfg(test)]
mod tests {
    use fluxdown_protocol::{TaskActivityDto, TaskActivityPage};

    use super::ActivityFeed;

    fn entry(id: i64, task_id: &str) -> TaskActivityDto {
        TaskActivityDto {
            id,
            task_id: task_id.into(),
            ..Default::default()
        }
    }

    fn page(task_id: &str, ids: &[i64], has_more: bool) -> TaskActivityPage {
        TaskActivityPage {
            entries: ids.iter().map(|id| entry(*id, task_id)).collect(),
            has_more,
            oldest_id: Some(1),
            newest_id: ids.last().copied(),
            ..Default::default()
        }
    }

    #[test]
    fn latest_older_and_event_merge_by_id_without_reordering() {
        let mut feed = ActivityFeed::new("a".into());
        let latest = feed.begin().unwrap();
        assert_eq!(latest.query.before_id, None);
        feed.add(entry(8, "a"));
        feed.finish(&latest, Some(page("a", &[5, 6, 7], true)));
        let catchup = feed.begin().unwrap();
        assert_eq!(catchup.query.after_id, Some(7));
        feed.finish(&catchup, Some(page("a", &[8], false)));
        feed.load_older();
        let older = feed.begin().unwrap();
        assert_eq!(older.query.before_id, Some(5));
        feed.add(entry(8, "a"));
        feed.finish(&older, Some(page("a", &[1, 3, 5], false)));
        assert_eq!(
            feed.entries().map(|entry| entry.id).collect::<Vec<_>>(),
            [1, 3, 5, 6, 7, 8]
        );
        assert!(!feed.has_older());
    }

    #[test]
    fn live_event_during_catchup_does_not_skip_missing_records() {
        let mut feed = ActivityFeed::new("a".into());
        let latest = feed.begin().unwrap();
        feed.finish(&latest, Some(page("a", &[10], false)));
        feed.reconnect();
        let first = feed.begin().unwrap();
        assert_eq!(first.query.after_id, Some(10));
        feed.add(entry(14, "a"));
        feed.finish(&first, Some(page("a", &[11, 12], true)));
        let second = feed.begin().unwrap();
        assert_eq!(second.query.after_id, Some(12));
        feed.finish(&second, Some(page("a", &[13, 14], false)));
        assert!(feed.begin().is_none());
        assert_eq!(
            feed.entries().map(|entry| entry.id).collect::<Vec<_>>(),
            [10, 11, 12, 13, 14]
        );
    }

    #[test]
    fn task_switch_discards_late_result_and_wrong_task_event() {
        let mut feed = ActivityFeed::new("a".into());
        let old = feed.begin().unwrap();
        feed.switch_task("b".into());
        assert!(!feed.add(entry(20, "a")));
        assert!(!feed.finish(&old, Some(page("a", &[20], false))));
        let next = feed.begin().unwrap();
        assert_eq!(next.query.task_id, "b");
        feed.finish(&next, Some(page("b", &[1], false)));
        assert_eq!(
            feed.entries().map(|entry| entry.id).collect::<Vec<_>>(),
            [1]
        );
    }

    #[test]
    fn error_requires_explicit_retry_and_preserves_events() {
        let mut feed = ActivityFeed::new("a".into());
        let latest = feed.begin().unwrap();
        feed.add(entry(2, "a"));
        feed.finish(&latest, None);
        assert!(feed.failed());
        assert!(feed.begin().is_none());
        feed.retry();
        let retry = feed.begin().unwrap();
        feed.finish(&retry, Some(page("a", &[1, 2], false)));
        assert_eq!(
            feed.entries().map(|entry| entry.id).collect::<Vec<_>>(),
            [1, 2]
        );
    }

    #[test]
    fn reconnect_detects_restarted_database_epoch() {
        let mut feed = ActivityFeed::new("a".into());
        let latest = feed.begin().unwrap();
        feed.finish(&latest, Some(page("a", &[40], false)));
        feed.reconnect();
        let after = feed.begin().unwrap();
        assert_eq!(after.query.after_id, Some(40));
        feed.finish(&after, Some(page("a", &[2], false)));
        let latest = feed.begin().unwrap();
        assert_eq!(latest.query.after_id, None);
        feed.finish(&latest, Some(page("a", &[2], false)));
        assert_eq!(
            feed.entries().map(|entry| entry.id).collect::<Vec<_>>(),
            [2]
        );
    }

    #[test]
    fn event_during_empty_catchup_page_requeries_without_polling_progress() {
        let mut feed = ActivityFeed::new("a".into());
        let latest = feed.begin().unwrap();
        feed.finish(&latest, Some(page("a", &[10], false)));
        feed.add(entry(11, "a"));
        assert!(feed.begin().is_none());
        feed.reconnect();
        let first = feed.begin().unwrap();
        feed.add(entry(12, "a"));
        let mut empty = page("a", &[], false);
        empty.newest_id = Some(10);
        feed.finish(&first, Some(empty));
        let second = feed.begin().unwrap();
        assert_eq!(second.query.after_id, Some(10));
        feed.finish(&second, Some(page("a", &[11, 12], false)));
        assert_eq!(
            feed.entries().map(|entry| entry.id).collect::<Vec<_>>(),
            [10, 11, 12]
        );
        assert!(feed.begin().is_none());
    }

    #[test]
    fn retention_prunes_expired_cached_entries_and_reports_range() {
        let mut feed = ActivityFeed::new("a".into());
        let latest = feed.begin().unwrap();
        feed.finish(&latest, Some(page("a", &[10, 20], false)));
        feed.reconnect();
        let after = feed.begin().unwrap();
        let mut retained = page("a", &[50, 51], false);
        retained.oldest_id = Some(50);
        retained.truncated = true;
        feed.finish(&after, Some(retained));
        assert_eq!(
            feed.entries().map(|entry| entry.id).collect::<Vec<_>>(),
            [50, 51]
        );
        assert_eq!(feed.retained_range(), (Some(50), Some(51), true));
        feed.add(TaskActivityDto {
            kind: "journal_overflow".into(),
            ..entry(52, "a")
        });
        assert!(feed.has_journal_gap());
        feed.reconnect();
        let next = feed.begin().unwrap();
        let mut later = page("a", &[52], false);
        later.oldest_id = Some(50);
        feed.finish(&next, Some(later));
        assert_eq!(feed.retained_range(), (Some(50), Some(52), true));
    }

    #[test]
    fn reconnect_during_old_request_retries_on_new_snapshot() {
        let mut feed = ActivityFeed::new("a".into());
        let old = feed.begin().unwrap();
        feed.reconnect();
        let fresh = feed.begin().unwrap();
        assert!(!feed.finish(&old, Some(page("a", &[99], false))));
        assert!(!feed.failed());
        feed.finish(&fresh, Some(page("a", &[1], false)));
        assert_eq!(
            feed.entries().map(|entry| entry.id).collect::<Vec<_>>(),
            [1]
        );
    }
}
