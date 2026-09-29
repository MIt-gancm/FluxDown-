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
pub(crate) fn format_timestamp(raw: &str) -> String {
    let raw = raw.trim();
    let bytes = raw.as_bytes();
    let looks_iso = bytes.len() >= 16
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && matches!(bytes[10], b'T' | b' ')
        && bytes[13] == b':';
    if looks_iso {
        raw.get(..16)
            .map_or_else(|| raw.to_owned(), |head| head.replacen('T', " ", 1))
    } else {
        raw.to_owned()
    }
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
        assert_eq!(format_timestamp("2026-09-29T06:37:48Z"), "2026-09-29 06:37");
        assert_eq!(format_timestamp("2026-09-29 06:37:48"), "2026-09-29 06:37");
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
