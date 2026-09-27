//! 设备分组：标题 + 刷新按钮 + 受信任设备列表（名称 + 平台 + 当前设备标记）。

use fluxdown_protocol::CloudDevice;
use fluxdown_ui_components::{ButtonVariant, button, card};
use fluxdown_ui_i18n::Translator;
use fluxdown_ui_theme::{ExtendedTokens, SemanticThemeTokens, active_theme};
use gpui::{
    App, ClickEvent, Context, FontWeight, IntoElement, ParentElement, Styled, div,
    prelude::FluentBuilder as _,
};
use gpui_component::{Sizable as _, h_flex, spinner::Spinner};

use crate::view::AccountView;
use crate::{t, ui};

pub(crate) fn render(
    translator: &Translator,
    tokens: &SemanticThemeTokens,
    devices: &[CloudDevice],
    disabled: bool,
    refreshing: bool,
    cx: &mut Context<AccountView>,
) -> impl IntoElement {
    let title = t(translator, "accountDevicesTitle");
    let retry_label = t(translator, "accountDevicesRetry");
    let empty_label = t(translator, "accountDevicesEmpty");
    let current_label = t(translator, "accountDeviceCurrent");
    let extended = active_theme(cx).extended().clone();

    div()
        .flex()
        .flex_col()
        .gap(tokens.spacing.sm)
        .child(
            h_flex()
                .w_full()
                .justify_between()
                .items_center()
                .child(ui::group_heading(title, None, cx))
                .child(
                    button(
                        "account-devices-refresh",
                        retry_label,
                        ButtonVariant::Secondary,
                        cx,
                    )
                    .gap(tokens.spacing.xs)
                    .disabled(disabled || refreshing)
                    .when(refreshing, |this| {
                        this.child(
                            Spinner::new()
                                .with_size(extended.icon.sm)
                                .color(tokens.colors.muted_foreground),
                        )
                    })
                    .on_click(cx.listener(
                        move |view, _: &ClickEvent, window: &mut gpui::Window, cx| {
                            view.refresh_devices(window, cx);
                        },
                    )),
                ),
        )
        .child(card(cx).w_full().map(|card| {
            if devices.is_empty() {
                card.p(tokens.spacing.lg).flex().justify_center().child(
                    div()
                        .text_size(tokens.typography.xs.size)
                        .line_height(tokens.typography.xs.line_height)
                        .text_color(tokens.colors.muted_foreground)
                        .child(empty_label),
                )
            } else {
                card.flex()
                    .flex_col()
                    .children(devices.iter().enumerate().map(|(index, device)| {
                        device_row(
                            tokens,
                            &extended,
                            device,
                            current_label.clone(),
                            index > 0,
                            cx,
                        )
                    }))
            }
        }))
}

fn device_row(
    tokens: &SemanticThemeTokens,
    extended: &ExtendedTokens,
    device: &CloudDevice,
    current_label: gpui::SharedString,
    with_divider: bool,
    cx: &App,
) -> impl IntoElement {
    let badge_radius = active_theme(cx).components().badge_radius;
    div()
        .w_full()
        .when(with_divider, |this| this.child(ui::row_divider(cx)))
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .px(tokens.spacing.md)
                .py(tokens.spacing.sm)
                .child(
                    h_flex()
                        .items_center()
                        .gap(tokens.spacing.sm)
                        .child(
                            div()
                                .text_size(tokens.typography.sm.size)
                                .line_height(tokens.typography.sm.line_height)
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(tokens.colors.foreground)
                                .child(device.name.clone()),
                        )
                        .when(device.is_current, |this| {
                            this.child(
                                div()
                                    .px(tokens.spacing.xs)
                                    .py(tokens.spacing.xxs)
                                    .rounded(badge_radius)
                                    .bg(tokens.colors.accent)
                                    .text_size(extended.caption.size)
                                    .line_height(extended.caption.line_height)
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(tokens.colors.accent_foreground)
                                    .child(current_label),
                            )
                        }),
                )
                .child(
                    div()
                        .text_size(tokens.typography.xs.size)
                        .line_height(tokens.typography.xs.line_height)
                        .text_color(tokens.colors.muted_foreground)
                        .child(device.platform.clone().unwrap_or_default()),
                ),
        )
}
