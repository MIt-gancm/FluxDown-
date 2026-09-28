//! 云功能分组：配置同步开关（真实读写 `agent.sync.*`）+ 多设备协同状态行
//! （无独立开关，登录后自动可用，展示当前在线设备数）。

use fluxdown_protocol::{CloudDevice, SyncStatusDto};
use fluxdown_ui_components::{FluxIcon, card, tabular_numbers};
use fluxdown_ui_i18n::Translator;
use fluxdown_ui_theme::{SemanticThemeTokens, active_theme};
use gpui::{
    App, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement as _, Styled, div, prelude::FluentBuilder as _,
};
use gpui_component::{
    Disableable as _, Icon, IconNamed, h_flex, switch::Switch, tooltip::Tooltip, v_flex,
};

use crate::view::AccountView;
use crate::{AccountCommand, t, t_with, ui};

pub(crate) fn render(
    translator: &Translator,
    tokens: &SemanticThemeTokens,
    logged_in: bool,
    sync: &SyncStatusDto,
    devices: &[CloudDevice],
    disabled: bool,
    cx: &mut Context<AccountView>,
) -> impl IntoElement {
    let heading = t(translator, "accountGroupCloudFeatures");
    let desc = t(translator, "accountCloudFeaturesDesc");

    div()
        .flex()
        .flex_col()
        .gap(tokens.spacing.sm)
        .child(ui::group_heading(heading, Some(desc), cx))
        .child(
            card(cx)
                .w_full()
                .flex()
                .flex_col()
                .child(config_sync_row(
                    translator, tokens, logged_in, sync, disabled, cx,
                ))
                .child(ui::row_divider(cx))
                .child(multi_device_row(translator, tokens, logged_in, devices, cx)),
        )
}

/// 行首图标块：`density.control` 见方的中性底 + `icon.lg` 图标。
fn row_icon(tokens: &SemanticThemeTokens, icon: impl IconNamed, cx: &App) -> impl IntoElement {
    div()
        .size(active_theme(cx).density().control)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(tokens.radius.md)
        .bg(tokens.colors.muted)
        .child(
            Icon::new(icon)
                .size(active_theme(cx).extended().icon.lg)
                .text_color(tokens.colors.muted_foreground),
        )
}

/// 行标题（sm MEDIUM）+ 说明（xs 二级文字）。
fn row_text(
    tokens: &SemanticThemeTokens,
    title: SharedString,
    subtitle: SharedString,
) -> impl IntoElement {
    v_flex()
        .flex_1()
        .min_w_0()
        .gap(tokens.spacing.xxs)
        .child(
            div()
                .text_size(tokens.typography.sm.size)
                .line_height(tokens.typography.sm.line_height)
                .font_weight(FontWeight::MEDIUM)
                .text_color(tokens.colors.foreground)
                .child(title),
        )
        .child(
            div()
                .text_size(tokens.typography.xs.size)
                .line_height(tokens.typography.xs.line_height)
                .text_color(tokens.colors.muted_foreground)
                .truncate()
                .child(subtitle),
        )
}

fn config_sync_row(
    translator: &Translator,
    tokens: &SemanticThemeTokens,
    logged_in: bool,
    sync: &SyncStatusDto,
    disabled: bool,
    cx: &mut Context<AccountView>,
) -> impl IntoElement {
    let title = t(translator, "cloudSyncTitle");
    let active = logged_in && sync.enabled;
    let subtitle: SharedString = if !active {
        t(translator, "cloudSyncDesc")
    } else if let Some(error) = sync.last_error.as_deref().filter(|error| !error.is_empty()) {
        t_with(translator, "cloudSyncStatusError", &[("reason", error)])
    } else if !sync.dirty_keys.is_empty() {
        t(translator, "cloudSyncStatusSyncing")
    } else {
        t(translator, "cloudSyncStatusSynced")
    };
    let login_required_label = t(translator, "cloudSyncLoginRequired");
    let checked = sync.enabled;
    let switch_disabled = disabled || !logged_in;

    let switch = Switch::new("account-sync-enable")
        .checked(checked)
        .disabled(switch_disabled)
        .on_click(cx.listener(move |view, checked: &bool, _, cx| {
            let method = if *checked {
                fluxdown_protocol::method::AGENT_SYNC_ENABLE
            } else {
                fluxdown_protocol::method::AGENT_SYNC_DISABLE
            };
            let future = view.controller.port().execute(AccountCommand::Sync {
                method,
                params: serde_json::json!({}),
            });
            view.spawn_action(future, cx);
        }));

    h_flex()
        .w_full()
        .items_center()
        .gap(tokens.spacing.md)
        .px(tokens.spacing.md)
        .py(tokens.spacing.sm)
        .child(row_icon(tokens, FluxIcon::RotateCw, cx))
        .child(row_text(tokens, title, subtitle))
        .child(if logged_in {
            switch.into_any_element()
        } else {
            div()
                .id("account-sync-login-required")
                .tooltip(move |window, cx| {
                    Tooltip::new(login_required_label.clone()).build(window, cx)
                })
                .child(switch)
                .into_any_element()
        })
}

fn multi_device_row(
    translator: &Translator,
    tokens: &SemanticThemeTokens,
    logged_in: bool,
    devices: &[CloudDevice],
    cx: &App,
) -> impl IntoElement {
    let title = t(translator, "multiDeviceTitle");
    let desc = t(translator, "multiDeviceDesc");
    let online_count = devices.iter().filter(|device| device.is_online).count();
    let caption = active_theme(cx).extended().caption;

    h_flex()
        .w_full()
        .items_center()
        .gap(tokens.spacing.md)
        .px(tokens.spacing.md)
        .py(tokens.spacing.sm)
        .child(row_icon(tokens, FluxIcon::Network, cx))
        .child(row_text(tokens, title, desc))
        .when(logged_in, |this| {
            this.child(
                div()
                    .px(tokens.spacing.sm)
                    .py(tokens.spacing.xxs)
                    .rounded(active_theme(cx).components().badge_radius)
                    .bg(tokens.colors.muted)
                    .text_size(caption.size)
                    .line_height(caption.line_height)
                    .font_features(tabular_numbers())
                    .text_color(tokens.colors.muted_foreground)
                    .child(t_with(
                        translator,
                        "devicesOnlineCount",
                        &[("count", &online_count.to_string())],
                    )),
            )
        })
}
