//! 邮箱验证码步骤的纯状态：有效期倒计时、重发冷却与「验证后替换设备」提示。
//!
//! 登录（设备验证 / 邮箱验证码）与注册验证共用；UI 每秒 `tick` 一次并在
//! [`CodeChallenge::is_idle`] 时停止计时器。

use std::time::Duration;

use gpui::{Context, Window};

/// 云端对同一邮箱重发验证码的限频窗口；窗口内重发只会返回原验证步骤，不会真的发信。
pub(crate) const RESEND_COOLDOWN_SECS: u64 = 60;

/// 一次已发出的验证码。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CodeChallenge {
    /// 验证码剩余有效秒数；0 = 已过期，需要重新发送。
    pub ttl_remaining: u64,
    /// 距离允许重发的剩余秒数；0 = 现在可以重发。
    pub resend_remaining: u64,
    /// 验证成功后会替换（登出）最久未用的设备。
    pub will_replace_devices: bool,
}

impl CodeChallenge {
    #[must_use]
    pub(crate) fn new(ttl_seconds: u64, will_replace_devices: bool) -> Self {
        Self {
            ttl_remaining: ttl_seconds,
            // 验证码有效期短于限频窗口时，过期即可重发。
            resend_remaining: RESEND_COOLDOWN_SECS.min(ttl_seconds),
            will_replace_devices,
        }
    }

    /// 过去 1 秒；返回显示是否发生变化（用于决定是否重绘）。
    pub(crate) fn tick(&mut self) -> bool {
        let before = *self;
        self.ttl_remaining = self.ttl_remaining.saturating_sub(1);
        self.resend_remaining = self.resend_remaining.saturating_sub(1);
        *self != before
    }

    #[must_use]
    pub(crate) fn is_expired(&self) -> bool {
        self.ttl_remaining == 0
    }

    #[must_use]
    pub(crate) fn can_resend(&self) -> bool {
        self.resend_remaining == 0
    }

    /// 两个计时都已走完：不再需要计时器。
    #[must_use]
    pub(crate) fn is_idle(&self) -> bool {
        self.ttl_remaining == 0 && self.resend_remaining == 0
    }
}

/// 每秒回调 `tick`（返回 `false` 结束）；视图销毁时自动停止。
/// 调用方用「代际」自行作废被新一轮验证码取代的旧计时器。
pub(crate) fn spawn_ticker<V: 'static>(
    window: &mut Window,
    cx: &mut Context<V>,
    tick: impl Fn(&mut V, &mut Window, &mut Context<V>) -> bool + 'static,
) {
    cx.spawn_in(window, async move |this, cx| {
        loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            match this.update_in(cx, |this, window, cx| tick(this, window, cx)) {
                Ok(true) => {}
                Ok(false) | Err(_) => break,
            }
        }
    })
    .detach();
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn countdown_runs_ttl_and_resend_independently_until_idle() {
        let mut challenge = CodeChallenge::new(90, true);
        assert!(challenge.will_replace_devices);
        assert!(!challenge.can_resend());
        for _ in 0..RESEND_COOLDOWN_SECS {
            assert!(challenge.tick());
        }
        assert!(challenge.can_resend());
        assert!(!challenge.is_expired());
        assert_eq!(challenge.ttl_remaining, 90 - RESEND_COOLDOWN_SECS);
        for _ in 0..30 {
            assert!(challenge.tick());
        }
        assert!(challenge.is_expired());
        assert!(challenge.is_idle());
        // 走完后不再变化，计时器据此停止。
        assert!(!challenge.tick());
    }

    #[test]
    fn short_ttl_allows_resend_at_expiry() {
        let mut challenge = CodeChallenge::new(20, false);
        assert_eq!(challenge.resend_remaining, 20);
        for _ in 0..20 {
            challenge.tick();
        }
        assert!(challenge.is_expired());
        assert!(challenge.can_resend());
    }

    #[test]
    fn zero_ttl_is_expired_and_resendable_immediately() {
        let challenge = CodeChallenge::new(0, false);
        assert!(challenge.is_expired());
        assert!(challenge.can_resend());
        assert!(challenge.is_idle());
    }

    #[test]
    fn replacing_the_challenge_restarts_both_timers_and_notice() {
        let mut challenge = CodeChallenge::new(300, false);
        for _ in 0..100 {
            challenge.tick();
        }
        challenge = CodeChallenge::new(300, true);
        assert_eq!(challenge.ttl_remaining, 300);
        assert_eq!(challenge.resend_remaining, RESEND_COOLDOWN_SECS);
        assert!(challenge.will_replace_devices);
    }
}
