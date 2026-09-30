//! 事件驱动刷新的合并：任务进度 / 运行态事件频繁（每个活跃任务约 500ms 一条，session 一批
//! 最多数百条），每条都重算可见行与侧栏计数是 O(事件数 × 任务数)。
//!
//! 事件只把视图标记为「过期」，由本闸门决定何时刷新：
//! - 同一批事件（同一轮 effect 周期）内只刷新一次，排在该批全部事件之后；
//! - 相邻两次刷新至少间隔 [`REFRESH_INTERVAL`]（≈30Hz），冷却期内的变化折叠成一次尾沿刷新；
//! - 行集合 / 下标布局变化（增删任务）时，旧的行 ID 可能指向别的任务，不等冷却，照样在
//!   本批事件之后立即刷新。

use std::time::{Duration, Instant};

/// 相邻两次事件驱动刷新的最小间隔。
pub(crate) const REFRESH_INTERVAL: Duration = Duration::from_millis(33);

/// 一次「视图过期」标记的处理方式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RefreshPlan {
    /// 已有刷新在排队，本次折叠进去。
    Nothing,
    /// 在当前 effect 周期末尾刷新（排在本批全部事件之后）。
    Deferred,
    /// 冷却中：给定延迟后尾沿刷新。
    After(Duration),
}

/// 到达的刷新触发来源。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RefreshTrigger {
    Deferred,
    Timer,
}

#[derive(Debug, Default)]
pub(crate) struct RefreshGate {
    dirty: bool,
    deferred_armed: bool,
    timer_armed: bool,
    last: Option<Instant>,
}

impl RefreshGate {
    /// 事件使视图过期。`structural`：行集合 / 下标布局变了。
    pub(crate) fn mark(&mut self, structural: bool, now: Instant) -> RefreshPlan {
        self.dirty = true;
        if self.deferred_armed {
            return RefreshPlan::Nothing;
        }
        let elapsed = self.last.map(|last| now.saturating_duration_since(last));
        let cooled = elapsed.is_none_or(|elapsed| elapsed >= REFRESH_INTERVAL);
        if structural || cooled {
            self.deferred_armed = true;
            return RefreshPlan::Deferred;
        }
        if self.timer_armed {
            return RefreshPlan::Nothing;
        }
        self.timer_armed = true;
        RefreshPlan::After(REFRESH_INTERVAL.saturating_sub(elapsed.unwrap_or_default()))
    }

    /// 排队的刷新到达。返回 `true` 表示视图仍过期、现在应刷新（已记为最近一次刷新）。
    pub(crate) fn take(&mut self, trigger: RefreshTrigger, now: Instant) -> bool {
        match trigger {
            RefreshTrigger::Deferred => self.deferred_armed = false,
            RefreshTrigger::Timer => self.timer_armed = false,
        }
        if !self.dirty {
            return false;
        }
        self.dirty = false;
        self.last = Some(now);
        true
    }

    /// 不经闸门的即时刷新（快照替换）：视图已是最新，排队中的刷新到达时不再重复。
    pub(crate) fn flushed(&mut self, now: Instant) {
        self.dirty = false;
        self.last = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{REFRESH_INTERVAL, RefreshGate, RefreshPlan, RefreshTrigger};

    #[test]
    fn events_in_one_batch_share_a_single_deferred_refresh() {
        let t0 = Instant::now();
        let mut gate = RefreshGate::default();
        assert_eq!(gate.mark(false, t0), RefreshPlan::Deferred);
        for _ in 0..255 {
            assert_eq!(gate.mark(false, t0), RefreshPlan::Nothing);
        }
        assert!(gate.take(RefreshTrigger::Deferred, t0));
        // 已刷新：再没有新事件就不会重复刷新。
        assert!(!gate.take(RefreshTrigger::Deferred, t0));
    }

    #[test]
    fn content_changes_inside_the_interval_collapse_into_one_trailing_refresh() {
        let t0 = Instant::now();
        let mut gate = RefreshGate::default();
        assert_eq!(gate.mark(false, t0), RefreshPlan::Deferred);
        assert!(gate.take(RefreshTrigger::Deferred, t0));

        let t1 = t0 + Duration::from_millis(10);
        assert_eq!(
            gate.mark(false, t1),
            RefreshPlan::After(REFRESH_INTERVAL - Duration::from_millis(10))
        );
        assert_eq!(gate.mark(false, t1), RefreshPlan::Nothing);
        let t2 = t0 + REFRESH_INTERVAL;
        assert!(gate.take(RefreshTrigger::Timer, t2));

        // 间隔过后的变化重新走批末尾刷新。
        let t3 = t2 + REFRESH_INTERVAL;
        assert_eq!(gate.mark(false, t3), RefreshPlan::Deferred);
    }

    #[test]
    fn structural_change_refreshes_without_waiting_for_the_interval() {
        let t0 = Instant::now();
        let mut gate = RefreshGate::default();
        gate.flushed(t0);
        let t1 = t0 + Duration::from_millis(5);
        assert_eq!(gate.mark(true, t1), RefreshPlan::Deferred);
        assert!(gate.take(RefreshTrigger::Deferred, t1));
    }

    #[test]
    fn structural_change_queued_behind_a_timer_is_not_refreshed_twice() {
        let t0 = Instant::now();
        let mut gate = RefreshGate::default();
        gate.flushed(t0);
        let t1 = t0 + Duration::from_millis(5);
        assert!(matches!(gate.mark(false, t1), RefreshPlan::After(_)));
        assert_eq!(gate.mark(true, t1), RefreshPlan::Deferred);
        assert!(gate.take(RefreshTrigger::Deferred, t1));
        // 先前排队的尾沿刷新到达时视图已是最新。
        assert!(!gate.take(RefreshTrigger::Timer, t0 + REFRESH_INTERVAL));
    }

    #[test]
    fn immediate_refresh_clears_pending_work() {
        let t0 = Instant::now();
        let mut gate = RefreshGate::default();
        assert_eq!(gate.mark(false, t0), RefreshPlan::Deferred);
        gate.flushed(t0);
        assert!(!gate.take(RefreshTrigger::Deferred, t0));
    }
}
