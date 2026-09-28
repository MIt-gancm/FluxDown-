//! 条目使用记录：次数 + 最近使用时间 → 频次 / 时效加权（frecency）。
//!
//! 常用条目在空查询时排在最前，有查询时在匹配分上叠加有上限的加权，避免高频但弱匹配的
//! 条目压过精确命中。

use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 最多保留的记录数；超出时淘汰权重最低的条目。
const MAX_ENTRIES: usize = 200;
/// 时效半衰期（天）：一个月没用，权重减半。
const HALF_LIFE_DAYS: f64 = 30.;
/// 时效衰减下限：长期高频的条目即使久未使用仍保留一部分加权。
const MIN_DECAY: f64 = 0.25;
const SECS_PER_DAY: f64 = 86_400.;
/// 加权换算成匹配分的系数与上限（一个查询字符约 16 分）。
const BOOST_SCALE: f64 = 18.;
const MAX_BOOST: f64 = 60.;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageEntry {
    count: u32,
    /// Unix 秒。
    last_used: u64,
}

/// 全部条目的使用记录（设备本地偏好，按条目 id 索引）。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UsageStats {
    entries: BTreeMap<String, UsageEntry>,
}

impl UsageStats {
    /// 从偏好值恢复；缺失、类型不符的整体或单条记录一律忽略。
    #[must_use]
    pub fn from_value(value: Option<&Value>) -> Self {
        let entries = value
            .and_then(Value::as_object)
            .map(|object| {
                object
                    .iter()
                    .filter_map(|(id, entry)| {
                        serde_json::from_value::<UsageEntry>(entry.clone())
                            .ok()
                            .map(|entry| (id.clone(), entry))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { entries }
    }

    /// 序列化为偏好值：`{ "<id>": { "count": n, "lastUsed": unixSecs } }`。
    #[must_use]
    pub fn to_value(&self) -> Value {
        serde_json::to_value(&self.entries).unwrap_or(Value::Null)
    }

    /// 记一次使用；超出容量时淘汰权重最低的其他条目。
    pub fn record(&mut self, id: &str, now: u64) {
        let entry = self.entries.entry(id.to_owned()).or_insert(UsageEntry {
            count: 0,
            last_used: now,
        });
        entry.count = entry.count.saturating_add(1);
        entry.last_used = entry.last_used.max(now);
        while self.entries.len() > MAX_ENTRIES {
            let weakest = self
                .entries
                .iter()
                .filter(|(key, _)| key.as_str() != id)
                .min_by(|(_, a), (_, b)| weight(**a, now).total_cmp(&weight(**b, now)))
                .map(|(key, _)| key.clone());
            match weakest {
                Some(key) => {
                    self.entries.remove(&key);
                }
                None => break,
            }
        }
    }

    /// 叠加到匹配分上的加权；从未使用为 0。
    #[must_use]
    pub(crate) fn boost(&self, id: &str, now: u64) -> i32 {
        let Some(entry) = self.entries.get(id) else {
            return 0;
        };
        // 值域 [0, MAX_BOOST]，转换不会截断。
        (weight(*entry, now) * BOOST_SCALE).round().min(MAX_BOOST) as i32
    }

    /// 当前 Unix 秒（系统时钟早于纪元时视为 0）。
    #[must_use]
    pub fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs())
    }
}

/// `ln(1 + 次数) × 时效衰减`：次数边际递减，久未使用按半衰期衰减到下限。
fn weight(entry: UsageEntry, now: u64) -> f64 {
    let age_days = now.saturating_sub(entry.last_used) as f64 / SECS_PER_DAY;
    let decay = 0.5_f64.powf(age_days / HALF_LIFE_DAYS).max(MIN_DECAY);
    f64::from(entry.count).ln_1p() * decay
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{MAX_ENTRIES, UsageStats};

    const DAY: u64 = 86_400;
    const NOW: u64 = 1_800_000_000;

    #[test]
    fn more_uses_rank_higher_and_boost_is_capped() {
        let mut stats = UsageStats::default();
        stats.record("once", NOW);
        for _ in 0..5 {
            stats.record("often", NOW);
        }
        for _ in 0..10_000 {
            stats.record("always", NOW);
        }
        assert_eq!(stats.boost("never", NOW), 0);
        assert!(stats.boost("once", NOW) > 0);
        assert!(stats.boost("often", NOW) > stats.boost("once", NOW));
        assert_eq!(stats.boost("always", NOW), 60);
    }

    #[test]
    fn stale_usage_decays_but_keeps_a_floor() {
        let mut stats = UsageStats::default();
        for _ in 0..5 {
            stats.record("old", NOW - 400 * DAY);
            stats.record("recent", NOW);
        }
        let old = stats.boost("old", NOW);
        assert!(old > 0);
        assert!(stats.boost("recent", NOW) > old);
    }

    #[test]
    fn roundtrips_through_preference_value_and_skips_bad_entries() {
        let mut stats = UsageStats::default();
        stats.record("cmd.pause_all", NOW);
        stats.record("cmd.pause_all", NOW + 1);
        let value = stats.to_value();
        assert_eq!(
            value,
            json!({ "cmd.pause_all": { "count": 2, "lastUsed": NOW + 1 } })
        );
        assert_eq!(UsageStats::from_value(Some(&value)), stats);

        let mixed = json!({
            "cmd.pause_all": { "count": 2, "lastUsed": NOW + 1 },
            "broken": "x",
        });
        assert_eq!(UsageStats::from_value(Some(&mixed)), stats);
        assert_eq!(
            UsageStats::from_value(Some(&json!([1, 2]))),
            UsageStats::default()
        );
        assert_eq!(UsageStats::from_value(None), UsageStats::default());
    }

    #[test]
    fn capacity_evicts_weakest_but_never_the_entry_just_recorded() {
        let mut stats = UsageStats::default();
        for index in 0..MAX_ENTRIES {
            stats.record(&format!("strong-{index}"), NOW);
            stats.record(&format!("strong-{index}"), NOW);
        }
        stats.record("fresh", NOW - 1000 * DAY);
        assert_eq!(stats.entries.len(), MAX_ENTRIES);
        assert!(stats.boost("fresh", NOW) > 0);
    }
}
