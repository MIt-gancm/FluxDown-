//! 主窗口活动栏注册表：活动栏条目的唯一登记处。
//!
//! 主窗口按 [`ActivityEntry::ALL`] 注册按钮，偏好回流按它切换可见性，设置页「通用 → 活动栏」
//! 按它生成开关——新增可选入口只需在这里加变体并给出 [`ActivityEntry::toggle`]，
//! 开关自动出现在设置页，无需另行维护。

use std::collections::BTreeMap;

use fluxdown_ui_settings::ActivityBarToggle;
use fluxdown_ui_shell::{RouteId, ShellView};
use gpui::Context;

/// 主窗口路由标识：主窗口装配与命令面板导航共用。
pub(crate) const DOWNLOADS_ROUTE: RouteId = RouteId::new("downloads");
pub(crate) const RSS_ROUTE: RouteId = RouteId::new("rss");
pub(crate) const WEBHOOKS_ROUTE: RouteId = RouteId::new("webhooks");

/// 活动栏条目，按自上而下顺序（路由在上，动作在下）。
///
/// [`Self::ALL`] 是唯一构造点：新增变体未登记进 `ALL` 会触发 `dead_code`（`clippy -D warnings` 拦截），
/// 而 [`Self::button_id`] / [`Self::toggle`] 与主窗口装配的穷尽 `match` 迫使补齐元数据与内容。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActivityEntry {
    Downloads,
    Rss,
    Webhooks,
    Theme,
    Settings,
}

impl ActivityEntry {
    pub(crate) const ALL: [Self; 5] = [
        Self::Downloads,
        Self::Rss,
        Self::Webhooks,
        Self::Theme,
        Self::Settings,
    ];

    /// 活动栏按钮 id：shell 以它定位路由 / 动作。
    pub(crate) const fn button_id(self) -> &'static str {
        match self {
            Self::Downloads => "activity-downloads",
            Self::Rss => "activity-rss",
            Self::Webhooks => "activity-webhooks",
            Self::Theme => "activity-theme",
            Self::Settings => "activity-settings",
        }
    }

    /// 活动栏按钮与命令面板共用的标题文案键。
    pub(crate) const fn label_key(self) -> &'static str {
        match self {
            Self::Downloads => fluxdown_ui_i18n::keys::MOBILE_NAV_DOWNLOADS,
            Self::Rss => "sidebarRss",
            Self::Webhooks => "webhookNavTitle",
            Self::Theme => "activityThemeToggle",
            Self::Settings => fluxdown_ui_i18n::keys::SETTINGS,
        }
    }

    /// 路由条目的 shell 路由标识；动作条目为 `None`。
    pub(crate) const fn route(self) -> Option<RouteId> {
        match self {
            Self::Downloads => Some(DOWNLOADS_ROUTE),
            Self::Rss => Some(RSS_ROUTE),
            Self::Webhooks => Some(WEBHOOKS_ROUTE),
            Self::Theme | Self::Settings => None,
        }
    }

    /// 可选入口的可见性开关（偏好缺省视为显示）；`None` = 固定显示，不参与活动栏整体收起判定。
    pub(crate) const fn toggle(self) -> Option<ActivityBarToggle> {
        match self {
            Self::Downloads | Self::Settings => None,
            Self::Rss => Some(ActivityBarToggle {
                pref_key: "ui.show_activity_rss",
                title_key: "showActivityRss",
                desc_key: "showActivityRssDesc",
            }),
            Self::Webhooks => Some(ActivityBarToggle {
                pref_key: "ui.show_activity_webhooks",
                title_key: "showActivityWebhooks",
                desc_key: "showActivityWebhooksDesc",
            }),
            Self::Theme => Some(ActivityBarToggle {
                pref_key: "ui.show_activity_theme",
                title_key: "showActivityTheme",
                desc_key: "showActivityThemeDesc",
            }),
        }
    }
}

/// 设置页「活动栏」分区的开关行，与活动栏同序。
pub(crate) fn toggles() -> Vec<ActivityBarToggle> {
    ActivityEntry::ALL
        .into_iter()
        .filter_map(ActivityEntry::toggle)
        .collect()
}

/// 偏好 → 活动栏可选入口可见性；幂等，窗口创建与每次偏好快照/事件都走这里。
pub(crate) fn apply_visibility(
    shell: &mut ShellView,
    preferences: &BTreeMap<String, serde_json::Value>,
    cx: &mut Context<ShellView>,
) {
    for entry in ActivityEntry::ALL {
        let Some(toggle) = entry.toggle() else {
            continue;
        };
        let visible = preferences
            .get(toggle.pref_key)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        shell.set_entry_visible(entry.button_id(), visible, cx);
    }
}
