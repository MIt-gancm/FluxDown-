//! 云设备列表的展示规则：排序、搜索、折叠数量、名称校验与时间格式。

use fluxdown_protocol::CloudDevice;

/// 设备卡片默认展示的设备数；更多设备经「管理全部」对话框查看。
pub(crate) const VISIBLE_LIMIT: usize = 5;
/// 设备名长度上限（与 agent / 云端一致，按字符计）。
pub(crate) const NAME_MAX_CHARS: usize = 64;

/// 本机在前，其后在线设备，最后离线；同组按名称（不区分大小写）排序。
pub(crate) fn sorted(devices: &[CloudDevice]) -> Vec<CloudDevice> {
    let mut list = devices.to_vec();
    list.sort_by(|a, b| {
        b.is_current
            .cmp(&a.is_current)
            .then(b.is_online.cmp(&a.is_online))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    list
}

/// 名称 / 平台 / 版本任一包含查询词（不区分大小写）；空查询匹配全部。
pub(crate) fn matches_query(device: &CloudDevice, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    [
        Some(device.name.as_str()),
        device.platform.as_deref(),
        device.app_version.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|field| field.to_lowercase().contains(&query))
}

pub(crate) fn filtered(devices: &[CloudDevice], query: &str) -> Vec<CloudDevice> {
    sorted(devices)
        .into_iter()
        .filter(|device| matches_query(device, query))
        .collect()
}

/// 设备卡片可见的设备与被折叠的数量。
pub(crate) fn summarize(devices: &[CloudDevice]) -> (Vec<CloudDevice>, usize) {
    let mut list = sorted(devices);
    let hidden = list.len().saturating_sub(VISIBLE_LIMIT);
    list.truncate(VISIBLE_LIMIT);
    (list, hidden)
}

/// 规整重命名输入：去首尾空白后必须是 1–64 个字符。
pub(crate) fn normalize_device_name(raw: &str) -> Option<String> {
    let name = raw.trim();
    let count = name.chars().count();
    (1..=NAME_MAX_CHARS)
        .contains(&count)
        .then(|| name.to_owned())
}

/// 平台名 → 已有文案键；未识别的平台原样显示。
pub(crate) fn platform_label_key(platform: &str) -> Option<&'static str> {
    match platform.trim().to_ascii_lowercase().as_str() {
        "windows" | "win32" => Some("accountDevicePlatformWindows"),
        "macos" | "darwin" => Some("accountDevicePlatformMacos"),
        "linux" => Some("accountDevicePlatformLinux"),
        "android" => Some("accountDevicePlatformAndroid"),
        "ios" => Some("accountDevicePlatformIos"),
        "web" => Some("accountDevicePlatformWeb"),
        _ => None,
    }
}

/// ISO-8601 → `YYYY-MM-DD HH:MM`；无法识别时原样返回（空串保持为空）。
///
/// 带 `Z` 或 `±HH:MM` 偏移的时间按偏移换算到 UTC 并标注 `UTC`，避免把非 UTC 的偏移值
/// 当作同一时区直接截断；没有时区信息的值只截断。账户 crate 没有时区数据库，
/// 无法换算到本地时区，所以明确标注基准时区。
pub(crate) fn format_timestamp(raw: &str) -> String {
    let raw = raw.trim();
    let bytes = raw.as_bytes();
    let looks_iso = bytes.len() >= 16
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && matches!(bytes[10], b'T' | b' ')
        && bytes[13] == b':';
    if !looks_iso {
        return raw.to_owned();
    }
    if let Some(utc) = to_utc_minutes(raw) {
        return utc;
    }
    raw.get(..16)
        .map_or_else(|| raw.to_owned(), |head| head.replacen('T', " ", 1))
}

/// 解析带时区的 ISO-8601，返回 `YYYY-MM-DD HH:MM UTC`；无时区或解析失败返回 `None`。
fn to_utc_minutes(raw: &str) -> Option<String> {
    let field = |range: std::ops::Range<usize>| -> Option<i64> { raw.get(range)?.parse().ok() };
    let (year, month, day) = (field(0..4)?, field(5..7)?, field(8..10)?);
    let (hour, minute) = (field(11..13)?, field(14..16)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    // 跳过可选的秒与小数部分，定位时区后缀。
    let rest = raw.get(16..)?;
    let zone_start = rest.find(['Z', 'z', '+', '-'])?;
    let zone = &rest[zone_start..];
    let offset_minutes = if zone.eq_ignore_ascii_case("z") {
        0
    } else {
        let sign = if zone.starts_with('-') { -1 } else { 1 };
        let digits = zone.get(1..)?;
        let (oh, om) = match digits.len() {
            5 if digits.as_bytes()[2] == b':' => (digits.get(..2)?, digits.get(3..)?),
            4 => (digits.get(..2)?, digits.get(2..)?),
            _ => return None,
        };
        let (oh, om): (i64, i64) = (oh.parse().ok()?, om.parse().ok()?);
        if oh > 23 || om > 59 {
            return None;
        }
        sign * (oh * 60 + om)
    };
    let local_minutes = days_from_civil(year, month, day) * 1440 + hour * 60 + minute;
    let utc = local_minutes - offset_minutes;
    let days = utc.div_euclid(1440);
    let in_day = utc.rem_euclid(1440);
    let (y, m, d) = civil_from_days(days);
    Some(format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        in_day / 60,
        in_day % 60
    ))
}

/// 公历日期 → 自 1970-01-01 起的天数（Howard Hinnant 算法）。
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn device(name: &str, current: bool, online: bool) -> CloudDevice {
        CloudDevice {
            id: name.to_owned(),
            device_id: name.to_owned(),
            name: name.to_owned(),
            platform: Some("macos".to_owned()),
            created_at: String::new(),
            last_seen_at: String::new(),
            last_ip: None,
            app_version: Some("1.2.3".to_owned()),
            is_online: online,
            is_current: current,
            default_save_dir: None,
            path_style: None,
        }
    }

    #[test]
    fn current_device_first_then_online_then_name() {
        let list = sorted(&[
            device("zeta", false, true),
            device("alpha", false, false),
            device("Beta", false, true),
            device("me", true, false),
        ]);
        let names: Vec<_> = list.iter().map(|device| device.name.as_str()).collect();
        assert_eq!(names, ["me", "Beta", "zeta", "alpha"]);
    }

    #[test]
    fn search_matches_name_platform_and_version_case_insensitively() {
        let mut win = device("Office PC", false, true);
        win.platform = Some("windows".to_owned());
        let devices = [win, device("MacBook", false, false)];
        assert_eq!(filtered(&devices, "OFFICE").len(), 1);
        assert_eq!(filtered(&devices, "  windows ").len(), 1);
        assert_eq!(filtered(&devices, "1.2.3").len(), 2);
        assert!(filtered(&devices, "linux").is_empty());
        assert_eq!(filtered(&devices, "").len(), 2);
    }

    #[test]
    fn card_collapses_beyond_the_limit_and_reports_hidden_count() {
        let devices: Vec<_> = (0..8)
            .map(|index| device(&format!("d{index}"), index == 7, false))
            .collect();
        let (visible, hidden) = summarize(&devices);
        assert_eq!(visible.len(), VISIBLE_LIMIT);
        assert_eq!(hidden, 3);
        // 本机始终可见。
        assert_eq!(visible[0].name, "d7");
        let (visible, hidden) = summarize(&devices[..2]);
        assert_eq!((visible.len(), hidden), (2, 0));
    }

    #[test]
    fn rename_input_is_trimmed_and_bounded_by_characters() {
        assert_eq!(
            normalize_device_name("  Living room  "),
            Some("Living room".to_owned())
        );
        assert_eq!(normalize_device_name("   "), None);
        assert_eq!(normalize_device_name(""), None);
        assert!(normalize_device_name(&"名".repeat(NAME_MAX_CHARS)).is_some());
        assert_eq!(
            normalize_device_name(&"名".repeat(NAME_MAX_CHARS + 1)),
            None
        );
    }

    #[test]
    fn timestamps_are_shortened_only_when_iso() {
        assert_eq!(
            format_timestamp("2026-09-29T06:37:48Z"),
            "2026-09-29 06:37 UTC"
        );
        assert_eq!(format_timestamp("2026-09-29 06:37:48"), "2026-09-29 06:37");
        assert_eq!(
            format_timestamp("2026-09-29T06:37:48+08:00"),
            "2026-09-28 22:37 UTC"
        );
        assert_eq!(
            format_timestamp("2026-12-31T23:30:00.123-05:30"),
            "2027-01-01 05:00 UTC"
        );
        assert_eq!(format_timestamp("yesterday"), "yesterday");
        assert_eq!(format_timestamp(""), "");
    }

    #[test]
    fn platform_labels_cover_known_platforms_only() {
        assert_eq!(
            platform_label_key("Windows"),
            Some("accountDevicePlatformWindows")
        );
        assert_eq!(
            platform_label_key("darwin"),
            Some("accountDevicePlatformMacos")
        );
        assert_eq!(platform_label_key("freebsd"), None);
    }
}
