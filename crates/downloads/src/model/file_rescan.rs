//! 文件跟踪重扫节流：主窗口获焦会频繁触发重扫，最小间隔内的请求折叠成一次尾沿补发，
//! 而不是直接丢弃——否则「获焦 → 切出去删文件 → 再获焦」会被吞掉，要等 daemon 的
//! 5 分钟定时扫描才能发现。

use std::time::{Duration, Instant};

/// 两次获焦重扫的最小间隔。
pub(crate) const RESCAN_MIN_INTERVAL: Duration = Duration::from_secs(10);

/// 一次重扫请求的处理方式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RescanDecision {
    /// 立即重扫。
    Now,
    /// 冷却中：在给定延迟后补发一次尾沿重扫。
    After(Duration),
    /// 冷却中且已有尾沿补发排队，本次折叠进去。
    Coalesced,
}

#[derive(Debug, Default)]
pub(crate) struct RescanThrottle {
    last: Option<Instant>,
    trailing_pending: bool,
}

impl RescanThrottle {
    /// 获焦等可合并的重扫请求。返回 [`RescanDecision::Now`] 时已记为最近一次重扫。
    pub(crate) fn request(&mut self, now: Instant) -> RescanDecision {
        match self.last.map(|last| now.saturating_duration_since(last)) {
            Some(elapsed) if elapsed < RESCAN_MIN_INTERVAL => {
                if self.trailing_pending {
                    RescanDecision::Coalesced
                } else {
                    self.trailing_pending = true;
                    RescanDecision::After(RESCAN_MIN_INTERVAL - elapsed)
                }
            }
            _ => {
                self.last = Some(now);
                RescanDecision::Now
            }
        }
    }

    /// 尾沿补发执行时调用。
    pub(crate) fn trailing_fired(&mut self, now: Instant) {
        self.trailing_pending = false;
        self.last = Some(now);
    }

    /// 不经节流的即时重扫（打开 / 拖出时发现文件已不在）也计入间隔。
    pub(crate) fn record_immediate(&mut self, now: Instant) {
        self.last = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{RESCAN_MIN_INTERVAL, RescanDecision, RescanThrottle};

    #[test]
    fn requests_inside_cooldown_collapse_into_one_trailing_rescan() {
        let start = Instant::now();
        let mut throttle = RescanThrottle::default();
        assert_eq!(throttle.request(start), RescanDecision::Now);

        let second = start + Duration::from_secs(3);
        assert_eq!(
            throttle.request(second),
            RescanDecision::After(RESCAN_MIN_INTERVAL - Duration::from_secs(3))
        );
        // 尾沿已排队：后续获焦不再重复排队。
        assert_eq!(
            throttle.request(start + Duration::from_secs(5)),
            RescanDecision::Coalesced
        );

        let fired = start + RESCAN_MIN_INTERVAL;
        throttle.trailing_fired(fired);
        // 尾沿执行后重新计冷却，并允许再次排队。
        assert!(matches!(
            throttle.request(fired + Duration::from_secs(1)),
            RescanDecision::After(_)
        ));
        assert_eq!(
            throttle.request(fired + Duration::from_secs(1)),
            RescanDecision::Coalesced
        );
    }

    #[test]
    fn immediate_rescan_restarts_cooldown_and_expiry_allows_rescan() {
        let start = Instant::now();
        let mut throttle = RescanThrottle::default();
        throttle.record_immediate(start);
        assert!(matches!(
            throttle.request(start + Duration::from_secs(1)),
            RescanDecision::After(_)
        ));

        let mut throttle = RescanThrottle::default();
        assert_eq!(throttle.request(start), RescanDecision::Now);
        assert_eq!(
            throttle.request(start + RESCAN_MIN_INTERVAL),
            RescanDecision::Now
        );
    }
}
