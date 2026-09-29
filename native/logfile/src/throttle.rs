//! 重复日志限流：同一条消息（数字归一后）在一个窗口内只落前 [`BURST`] 条，其余计数，
//! 窗口结束后以一行摘要补记被压掉的次数。
//!
//! 目的：渲染失败、设备丢失、重连这类每帧 / 每 100ms 触发的循环不会把轮转日志冲掉，
//! 启动头与首次出错的上下文始终留在文件里；同时循环本身仍以摘要形式可见（次数 + 时长）。

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{Duration, Instant};

use crate::Level;

/// 同一消息每个窗口内原样落盘的条数。
pub(crate) const BURST: u32 = 10;
/// 限流窗口。
pub(crate) const WINDOW: Duration = Duration::from_secs(60);
/// 同时跟踪的不同消息上限；超出时淘汰窗口最早的一条（先补记其摘要）。
const MAX_KEYS: usize = 256;
/// 过期窗口的清扫间隔：摘要最迟在窗口结束后这么久补记。
const SWEEP_INTERVAL: Duration = Duration::from_secs(1);
/// 摘要里引用原消息的最大字节数。
const SAMPLE_BYTES: usize = 240;

/// 一次提交的处理结果。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    /// 原样写入；`last_before_suppression` 为 true 表示这是窗口内最后一条放行的重复。
    Write { last_before_suppression: bool },
    /// 计数但不写。
    Suppress,
}

/// 窗口结束时需要补记的摘要。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Summary {
    pub level: Level,
    pub suppressed: u64,
    pub span: Duration,
    pub sample: String,
}

struct Entry {
    window_start: Instant,
    emitted: u32,
    suppressed: u64,
    last_seen: Instant,
    level: Level,
    sample: String,
}

impl Entry {
    fn summary(&self) -> Option<Summary> {
        (self.suppressed > 0).then(|| Summary {
            level: self.level,
            suppressed: self.suppressed,
            span: self.last_seen.saturating_duration_since(self.window_start),
            sample: self.sample.clone(),
        })
    }
}

pub(crate) struct Throttle {
    entries: HashMap<u64, Entry>,
    last_sweep: Option<Instant>,
}

impl Throttle {
    pub(crate) fn new() -> Self {
        Self {
            entries: HashMap::new(),
            last_sweep: None,
        }
    }

    /// 判定一条消息；到期窗口的摘要追加到 `summaries`（须先于本条写出）。
    pub(crate) fn submit(
        &mut self,
        level: Level,
        text: &str,
        now: Instant,
        summaries: &mut Vec<Summary>,
    ) -> Decision {
        if self
            .last_sweep
            .is_none_or(|last| now.saturating_duration_since(last) >= SWEEP_INTERVAL)
        {
            self.sweep(now, summaries);
        }
        let key = message_key(level, text);
        if let Some(entry) = self.entries.get_mut(&key) {
            if now.saturating_duration_since(entry.window_start) >= WINDOW {
                summaries.extend(entry.summary());
                *entry = new_entry(level, text, now);
                return Decision::Write {
                    last_before_suppression: false,
                };
            }
            entry.last_seen = now;
            if entry.emitted < BURST {
                entry.emitted += 1;
                return Decision::Write {
                    last_before_suppression: entry.emitted == BURST,
                };
            }
            entry.suppressed += 1;
            return Decision::Suppress;
        }
        if self.entries.len() >= MAX_KEYS {
            self.evict_oldest(summaries);
        }
        self.entries.insert(key, new_entry(level, text, now));
        Decision::Write {
            last_before_suppression: false,
        }
    }

    /// 补记全部待定摘要并清空状态（进程退出 / panic 前调用）。
    pub(crate) fn drain(&mut self, summaries: &mut Vec<Summary>) {
        summaries.extend(
            self.entries
                .drain()
                .filter_map(|(_, entry)| entry.summary()),
        );
    }

    fn sweep(&mut self, now: Instant, summaries: &mut Vec<Summary>) {
        self.last_sweep = Some(now);
        self.entries.retain(|_, entry| {
            if now.saturating_duration_since(entry.window_start) < WINDOW {
                return true;
            }
            summaries.extend(entry.summary());
            false
        });
    }

    fn evict_oldest(&mut self, summaries: &mut Vec<Summary>) {
        let oldest = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.window_start)
            .map(|(key, _)| *key);
        if let Some(entry) = oldest.and_then(|key| self.entries.remove(&key)) {
            summaries.extend(entry.summary());
        }
    }
}

fn new_entry(level: Level, text: &str, now: Instant) -> Entry {
    Entry {
        window_start: now,
        emitted: 1,
        suppressed: 0,
        last_seen: now,
        level,
        sample: sample(text),
    }
}

/// 限流键：级别 + 文本，连续的 ASCII 数字折叠为一个占位，让「第 N 次重试」「耗时 N ms」
/// 「错误码 0x…」这类只有数字不同的循环消息归入同一键。
fn message_key(level: Level, text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    level.hash(&mut hasher);
    let mut in_digits = false;
    for byte in text.bytes() {
        if byte.is_ascii_digit() {
            if !in_digits {
                b'#'.hash(&mut hasher);
            }
            in_digits = true;
        } else {
            in_digits = false;
            byte.hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn sample(text: &str) -> String {
    let first_line = text.lines().next().unwrap_or_default();
    if first_line.len() <= SAMPLE_BYTES {
        return first_line.to_owned();
    }
    let mut end = SAMPLE_BYTES;
    while !first_line.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &first_line[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(last: bool) -> Decision {
        Decision::Write {
            last_before_suppression: last,
        }
    }

    #[test]
    fn repeats_beyond_burst_are_counted_and_summarized_after_window() {
        let mut throttle = Throttle::new();
        let start = Instant::now();
        let mut summaries = Vec::new();
        for index in 0..BURST {
            let decision = throttle.submit(Level::Error, "draw failed", start, &mut summaries);
            assert_eq!(decision, write(index + 1 == BURST));
        }
        for _ in 0..5 {
            let at = start + Duration::from_secs(10);
            let decision = throttle.submit(Level::Error, "draw failed", at, &mut summaries);
            assert_eq!(decision, Decision::Suppress);
        }
        assert!(summaries.is_empty());

        // 窗口结束后的任意一条日志都会先补记摘要。
        let later = start + WINDOW + Duration::from_secs(1);
        let decision = throttle.submit(Level::Info, "unrelated", later, &mut summaries);
        assert_eq!(decision, write(false));
        assert_eq!(
            summaries,
            vec![Summary {
                level: Level::Error,
                suppressed: 5,
                span: Duration::from_secs(10),
                sample: "draw failed".to_owned(),
            }]
        );
        // 摘要只补记一次，同一消息重新开窗后照常放行。
        summaries.clear();
        let decision = throttle.submit(Level::Error, "draw failed", later, &mut summaries);
        assert_eq!(decision, write(false));
        assert!(summaries.is_empty());
    }

    #[test]
    fn messages_differing_only_in_numbers_share_a_key() {
        let mut throttle = Throttle::new();
        let now = Instant::now();
        let mut summaries = Vec::new();
        for attempt in 0..BURST {
            let text = format!("connect attempt {attempt} failed after {}ms", attempt * 7);
            throttle.submit(Level::Warn, &text, now, &mut summaries);
        }
        let decision = throttle.submit(
            Level::Warn,
            "connect attempt 99 failed after 3ms",
            now,
            &mut summaries,
        );
        assert_eq!(decision, Decision::Suppress);
        // 级别不同或文字不同则互不影响。
        let decision = throttle.submit(
            Level::Error,
            "connect attempt 99 failed after 3ms",
            now,
            &mut summaries,
        );
        assert_eq!(decision, write(false));
        let decision = throttle.submit(
            Level::Warn,
            "listen attempt 1 failed after 3ms",
            now,
            &mut summaries,
        );
        assert_eq!(decision, write(false));
    }

    #[test]
    fn drain_reports_pending_suppressions() {
        let mut throttle = Throttle::new();
        let now = Instant::now();
        let mut summaries = Vec::new();
        for _ in 0..BURST + 3 {
            throttle.submit(Level::Warn, "device lost", now, &mut summaries);
        }
        throttle.submit(Level::Info, "quiet", now, &mut summaries);
        throttle.drain(&mut summaries);
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].suppressed, 3);
    }

    #[test]
    fn key_capacity_evicts_oldest_window_with_its_summary() {
        let mut throttle = Throttle::new();
        let start = Instant::now();
        let mut summaries = Vec::new();
        for _ in 0..BURST + 2 {
            throttle.submit(Level::Error, "oldest loop", start, &mut summaries);
        }
        for index in 1..MAX_KEYS {
            let text = format!("distinct message {}", "x".repeat(index));
            throttle.submit(
                Level::Info,
                &text,
                start + Duration::from_millis(1),
                &mut summaries,
            );
        }
        assert!(summaries.is_empty());
        throttle.submit(
            Level::Info,
            "one more",
            start + Duration::from_millis(2),
            &mut summaries,
        );
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].sample, "oldest loop");
        assert_eq!(summaries[0].suppressed, 2);
    }
}
