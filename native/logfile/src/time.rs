//! 零依赖 UTC 时间戳：`YYYY-MM-DDTHH:MM:SS.mmmZ`。
//!
//! 桌面与 agent 的日志跨进程、跨时区对照，统一用带 `Z` 的 UTC，避免本地时区与夏令时歧义。

use std::time::{SystemTime, UNIX_EPOCH};

/// 把 `time` 格式化为毫秒精度的 RFC 3339 UTC 时间戳；早于 1970 的时间按纪元处理。
#[must_use]
pub fn utc_timestamp(time: SystemTime) -> String {
    let since_epoch = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since_epoch.as_secs();
    let millis = since_epoch.subsec_millis();
    let days = i64::try_from(secs / 86_400).unwrap_or(i64::MAX);
    let seconds_of_day = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        seconds_of_day / 3_600,
        seconds_of_day / 60 % 60,
        seconds_of_day % 60,
    )
}

/// 自 1970-01-01 起的天数 → 公历 (年, 月, 日)（Howard Hinnant 的 civil_from_days）。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn formats_epoch_leap_day_and_millis() {
        assert_eq!(utc_timestamp(UNIX_EPOCH), "1970-01-01T00:00:00.000Z");
        // 2024-02-29T23:59:59.999Z：闰日 + 日界前最后一毫秒。
        let leap = UNIX_EPOCH + Duration::from_millis(1_709_251_199_999);
        assert_eq!(utc_timestamp(leap), "2024-02-29T23:59:59.999Z");
        let next = leap + Duration::from_millis(1);
        assert_eq!(utc_timestamp(next), "2024-03-01T00:00:00.000Z");
    }
}
