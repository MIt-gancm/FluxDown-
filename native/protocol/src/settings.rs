//! Flutter 云同步目录的唯一 wire 键、所有权与 daemon 映射。

use serde_json::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingOwner {
    Daemon,
    Agent,
    Preferences,
    Excluded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettingSpec {
    pub key: &'static str,
    pub owner: SettingOwner,
    pub storage_key: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingValueKind {
    Boolean,
    Integer,
    Float,
    String,
}

macro_rules! spec {
    ($key:literal, $owner:ident) => {
        SettingSpec {
            key: $key,
            owner: SettingOwner::$owner,
            storage_key: $key,
        }
    };
    ($key:literal, $owner:ident, $storage:literal) => {
        SettingSpec {
            key: $key,
            owner: SettingOwner::$owner,
            storage_key: $storage,
        }
    };
}

/// 与 `lib/src/services/cloud/sync_catalog.dart` 对应的键，外加 GPUI 专属的 `ui.show_activity_*` 与
/// `custom_categories`（自定义分类，推送时剥离各设备不同的 `saveDir`；Flutter 拉到未知键会忽略），共 55 个。
pub const SYNC_SETTING_SPECS: &[SettingSpec] = &[
    spec!("appearance.theme_mode", Preferences),
    spec!("appearance.dark_theme", Preferences),
    spec!("appearance.light_theme", Preferences),
    spec!("appearance.color_scheme", Preferences),
    spec!("appearance.custom_color", Preferences),
    spec!("general.locale", Preferences),
    spec!("general.update_channel", Preferences),
    spec!("general.auto_check_update", Preferences),
    spec!("general.clipboard_watch", Preferences),
    spec!("general.floating_ball_enabled", Preferences),
    spec!("general.floating_ball_active_only", Preferences),
    spec!("ui.show_sidebar_status", Preferences),
    spec!("ui.show_sidebar_queues", Preferences),
    spec!("ui.show_sidebar_category", Preferences),
    spec!("ui.show_sidebar_rss", Preferences),
    spec!("ui.show_activity_rss", Preferences),
    spec!("ui.show_activity_webhooks", Preferences),
    spec!("ui.show_activity_theme", Preferences),
    spec!("ui.show_titlebar_pause_all", Preferences),
    spec!("ui.show_titlebar_resume_all", Preferences),
    spec!("ui.show_titlebar_settings", Preferences),
    spec!("ui.show_titlebar_theme", Preferences),
    spec!(
        "download.max_concurrent_tasks",
        Daemon,
        "max_concurrent_tasks"
    ),
    spec!("download.default_segments", Daemon, "default_segments"),
    spec!(
        "download.auto_max_connections",
        Daemon,
        "auto_max_connections"
    ),
    spec!("download.cdn_multi_enabled", Daemon, "cdn_multi_enabled"),
    spec!("download.cdn_max_nodes", Daemon, "cdn_max_nodes"),
    spec!("download.speed_limit_bytes", Daemon, "speed_limit_bytes"),
    spec!("download.max_auto_retries", Daemon, "max_auto_retries"),
    spec!(
        "download.auto_retry_delay_secs",
        Daemon,
        "auto_retry_delay_secs"
    ),
    spec!(
        "download.auto_resume_on_start",
        Daemon,
        "auto_resume_on_start"
    ),
    spec!("download.remember_last_save_dir", Preferences),
    spec!("download.use_server_time", Daemon, "use_server_time"),
    spec!("download.global_user_agent", Daemon, "global_user_agent"),
    spec!("download.notify_on_complete", Agent),
    spec!("download.silent_download", Agent),
    spec!("download.keep_awake", Agent),
    spec!("bt.enable_dht", Daemon, "bt_enable_dht"),
    spec!("bt.enable_upnp", Daemon, "bt_enable_upnp"),
    spec!("bt.custom_trackers", Daemon, "bt_custom_trackers"),
    spec!("bt.tracker_sub_enabled", Daemon, "bt_tracker_sub_enabled"),
    spec!("bt.tracker_sub_urls", Daemon, "bt_tracker_sub_urls"),
    spec!("bt.seed_ratio_limit", Daemon, "bt_seed_ratio_limit"),
    spec!(
        "bt.seed_post_ratio_limit",
        Daemon,
        "bt_seed_post_ratio_limit"
    ),
    spec!(
        "bt.seed_time_limit_minutes",
        Daemon,
        "bt_seed_time_limit_minutes"
    ),
    spec!(
        "bt.seed_inactive_time_limit_minutes",
        Daemon,
        "bt_seed_inactive_time_limit_minutes"
    ),
    spec!("bt.seed_limit_operator", Daemon, "bt_seed_limit_operator"),
    spec!("bt.seed_then_action", Daemon, "bt_seed_then_action"),
    spec!("bt.seed_max_active", Daemon, "bt_seed_max_active"),
    spec!("ed2k.enable_kad", Daemon, "ed2k_enable_kad"),
    spec!("ed2k.enable_upnp", Daemon, "ed2k_enable_upnp"),
    spec!("ed2k.server_list", Daemon, "ed2k_server_list"),
    spec!("ed2k.server_sub_enabled", Daemon, "ed2k_server_sub_enabled"),
    spec!("ed2k.server_sub_urls", Daemon, "ed2k_server_sub_urls"),
    spec!("custom_categories", Preferences),
];

#[must_use]
pub fn setting_spec(key: &str) -> Option<&'static SettingSpec> {
    SYNC_SETTING_SPECS.iter().find(|spec| spec.key == key)
}

#[must_use]
pub fn setting_value_kind(key: &str) -> SettingValueKind {
    if boolean_key(key) {
        SettingValueKind::Boolean
    } else if integer_key(key) {
        SettingValueKind::Integer
    } else if float_key(key) {
        SettingValueKind::Float
    } else {
        SettingValueKind::String
    }
}

pub fn validate_value(key: &str, value: &Value) -> Result<(), String> {
    if key == "custom_categories" {
        return validate_custom_categories(value);
    }
    if boolean_key(key) {
        return value
            .as_bool()
            .map(|_| ())
            .ok_or_else(|| format!("{key} must be boolean"));
    }
    if integer_key(key) {
        let value = value
            .as_i64()
            .ok_or_else(|| format!("{key} must be integer"))?;
        let (minimum, maximum) = integer_range(key);
        return (minimum..=maximum)
            .contains(&value)
            .then_some(())
            .ok_or_else(|| format!("{key} must be between {minimum} and {maximum}"));
    }
    if float_key(key) {
        return value
            .as_f64()
            .filter(|value| value.is_finite() && *value >= 0.0)
            .map(|_| ())
            .ok_or_else(|| format!("{key} must be a non-negative number"));
    }
    let value = value
        .as_str()
        .ok_or_else(|| format!("{key} must be string"))?;
    match key {
        "appearance.theme_mode" if !matches!(value, "system" | "light" | "dark") => {
            Err(format!("{key} has unknown value"))
        }
        "general.update_channel" if !matches!(value, "stable" | "frontier") => {
            Err(format!("{key} has unknown value"))
        }
        "bt.seed_limit_operator" if !matches!(value, "or" | "and") => {
            Err(format!("{key} has unknown value"))
        }
        "bt.seed_then_action" if !matches!(value, "stop" | "delete" | "delete_files") => {
            Err(format!("{key} has unknown value"))
        }
        _ => Ok(()),
    }
}

pub fn value_to_daemon_config(spec: &SettingSpec, value: &Value) -> Result<String, String> {
    validate_value(spec.key, value)?;
    match value {
        Value::Bool(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        Value::String(value) => Ok(value.clone()),
        _ => Err(format!("{} has unsupported config value", spec.key)),
    }
}

pub fn daemon_config_to_value(spec: &SettingSpec, value: &str) -> Result<Value, String> {
    if boolean_key(spec.key) {
        return match value {
            "true" | "1" => Ok(Value::Bool(true)),
            "false" | "0" => Ok(Value::Bool(false)),
            _ => Err(format!("{} has invalid boolean config", spec.key)),
        };
    }
    if integer_key(spec.key) {
        return value
            .parse::<i64>()
            .map(Value::from)
            .map_err(|error| error.to_string());
    }
    if float_key(spec.key) {
        let value = value.parse::<f64>().map_err(|error| error.to_string())?;
        return serde_json::Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| format!("{} has invalid float config", spec.key));
    }
    Ok(Value::String(value.to_owned()))
}

/// `custom_categories` 的线上形态：分类对象数组，或与 Flutter 偏好同形的 JSON 数组字符串。
fn validate_custom_categories(value: &Value) -> Result<(), String> {
    let parsed;
    let list = match value {
        Value::Array(list) => list,
        Value::String(text) => {
            parsed = serde_json::from_str::<Value>(text)
                .map_err(|error| format!("custom_categories is not valid JSON: {error}"))?;
            parsed
                .as_array()
                .ok_or_else(|| "custom_categories must be a JSON array".to_owned())?
        }
        _ => return Err("custom_categories must be an array".to_owned()),
    };
    if list.iter().all(Value::is_object) {
        Ok(())
    } else {
        Err("custom_categories entries must be objects".to_owned())
    }
}

fn boolean_key(key: &str) -> bool {
    matches!(
        key,
        "general.auto_check_update"
            | "general.clipboard_watch"
            | "general.floating_ball_enabled"
            | "general.floating_ball_active_only"
            | "ui.show_sidebar_status"
            | "ui.show_sidebar_queues"
            | "ui.show_sidebar_category"
            | "ui.show_sidebar_rss"
            | "ui.show_activity_rss"
            | "ui.show_activity_webhooks"
            | "ui.show_activity_theme"
            | "ui.show_titlebar_pause_all"
            | "ui.show_titlebar_resume_all"
            | "ui.show_titlebar_settings"
            | "ui.show_titlebar_theme"
            | "download.cdn_multi_enabled"
            | "download.auto_resume_on_start"
            | "download.remember_last_save_dir"
            | "download.use_server_time"
            | "download.notify_on_complete"
            | "download.silent_download"
            | "download.keep_awake"
            | "bt.enable_dht"
            | "bt.enable_upnp"
            | "bt.tracker_sub_enabled"
            | "ed2k.enable_kad"
            | "ed2k.enable_upnp"
            | "ed2k.server_sub_enabled"
    )
}

fn integer_key(key: &str) -> bool {
    matches!(
        key,
        "appearance.custom_color"
            | "download.max_concurrent_tasks"
            | "download.default_segments"
            | "download.auto_max_connections"
            | "download.cdn_max_nodes"
            | "download.speed_limit_bytes"
            | "download.max_auto_retries"
            | "download.auto_retry_delay_secs"
            | "bt.seed_time_limit_minutes"
            | "bt.seed_inactive_time_limit_minutes"
            | "bt.seed_max_active"
    )
}

fn float_key(key: &str) -> bool {
    matches!(key, "bt.seed_ratio_limit" | "bt.seed_post_ratio_limit")
}

fn integer_range(key: &str) -> (i64, i64) {
    match key {
        // Flutter `Color.toARGB32()`：无符号 32 位 ARGB。
        "appearance.custom_color" => (0, i64::from(u32::MAX)),
        "download.max_concurrent_tasks" => (1, 1024),
        "download.default_segments" => (0, 64),
        "download.auto_max_connections" => (0, 128),
        "download.cdn_max_nodes" => (0, 8),
        "download.max_auto_retries" => (-1, 20),
        "download.auto_retry_delay_secs" => (0, 86_400),
        _ => (0, i64::MAX),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::json;

    use super::{SYNC_SETTING_SPECS, SettingOwner, setting_spec, validate_value};

    #[test]
    fn catalog_has_exact_unique_flutter_count_and_namespaced_daemon_mapping() {
        assert_eq!(SYNC_SETTING_SPECS.len(), 55);
        assert_eq!(
            SYNC_SETTING_SPECS
                .iter()
                .map(|spec| spec.key)
                .collect::<HashSet<_>>()
                .len(),
            55
        );
        let spec = setting_spec("download.max_concurrent_tasks").expect("download spec");
        assert_eq!(spec.owner, SettingOwner::Daemon);
        assert_eq!(spec.storage_key, "max_concurrent_tasks");
        assert!(setting_spec("ui.show_sidebar_status").is_some());
        // 侧栏「设备区」显隐是设备本地偏好，永不入目录。
        assert!(setting_spec("ui.show_sidebar_devices").is_none());
    }

    #[test]
    fn validation_preserves_zero_auto_and_negative_infinite_retry_semantics() {
        assert!(validate_value("download.auto_max_connections", &json!(0)).is_ok());
        assert!(validate_value("download.max_auto_retries", &json!(-1)).is_ok());
        assert!(validate_value("bt.seed_then_action", &json!("delete_files")).is_ok());
        assert!(validate_value("bt.seed_then_action", &json!("remove")).is_err());
    }

    #[test]
    fn custom_color_accepts_flutter_argb32_integer_only() {
        assert!(validate_value("appearance.custom_color", &json!(0xFF11_2233_u32)).is_ok());
        assert!(validate_value("appearance.custom_color", &json!(u32::MAX)).is_ok());
        assert!(validate_value("appearance.custom_color", &json!(-1)).is_err());
        assert!(validate_value("appearance.custom_color", &json!("ff112233")).is_err());
    }

    #[test]
    fn custom_categories_accepts_object_array_or_flutter_json_string_only() {
        assert!(validate_value("custom_categories", &json!([{"id": "a"}])).is_ok());
        assert!(validate_value("custom_categories", &json!([])).is_ok());
        assert!(validate_value("custom_categories", &json!(r#"[{"id":"a"}]"#)).is_ok());
        assert!(validate_value("custom_categories", &json!("not json")).is_err());
        assert!(validate_value("custom_categories", &json!(r#"{"id":"a"}"#)).is_err());
        assert!(validate_value("custom_categories", &json!([1, 2])).is_err());
        assert!(validate_value("custom_categories", &json!(true)).is_err());
    }
}
