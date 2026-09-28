//! 未登录 hero 卡片：accent 圆形图标 + 标题 + 说明 + 登录/注册按钮。

use std::sync::Arc;

use fluxdown_ui_components::{ButtonVariant, FluxIcon, button, card};
use fluxdown_ui_i18n::Translator;
use fluxdown_ui_theme::{SemanticThemeTokens, active_theme};
use gpui::{
    Context, Entity, IntoElement, ParentElement, SharedString, Styled, div,
    prelude::FluentBuilder as _,
};
use gpui_component::{Icon, h_flex};

use crate::view::AccountView;
use crate::{AccountPort, dialogs, t};

pub(crate) fn render(
    translator: &Translator,
    translator_entity: &Entity<Translator>,
    tokens: &SemanticThemeTokens,
    port: Arc<dyn AccountPort>,
    disabled: bool,
    last_error: Option<SharedString>,
    cx: &mut Context<AccountView>,
) -> impl IntoElement {
    let title = t(translator, "accountLoginDialogTitle");
    let subtitle = t(translator, "accountHeroSubtitle");
    let login_label = t(translator, "accountLogin");
    let register_label = t(translator, "accountRegister");
    let login_translator = translator_entity.clone();
    let login_port = port.clone();
    let register_translator = translator_entity.clone();
    let register_port = port;

    let extended = active_theme(cx).extended().clone();
    // 与空状态同一插图尺度：图标 = 2×lg（32），圆底 = 3.5×lg（56），随界面缩放。
    let badge_icon = extended.icon.lg * 2.;
    let badge = extended.icon.lg * 3.5;

    card(cx)
        .w_full()
        .p(tokens.spacing.xl)
        .flex()
        .flex_col()
        .items_center()
        .child(
            div()
                .size(badge)
                .flex()
                .items_center()
                .justify_center()
                .rounded(tokens.radius.full)
                .bg(tokens.colors.accent)
                .child(
                    Icon::new(FluxIcon::CircleUser)
                        .size(badge_icon)
                        .text_color(tokens.colors.accent_foreground),
                ),
        )
        .child(
            div()
                .mt(tokens.spacing.lg)
                .text_size(extended.title.size)
                .line_height(extended.title.line_height)
                .font_weight(extended.title.weight)
                .text_color(tokens.colors.foreground)
                .child(title),
        )
        .child(
            div()
                .mt(tokens.spacing.xs)
                .text_size(tokens.typography.xs.size)
                .line_height(tokens.typography.xs.line_height)
                .text_color(tokens.colors.muted_foreground)
                .text_center()
                .child(subtitle),
        )
        .child(
            h_flex()
                .mt(tokens.spacing.lg)
                .gap(tokens.spacing.sm)
                .child(
                    button(
                        "account-hero-login",
                        login_label,
                        ButtonVariant::Primary,
                        cx,
                    )
                    .disabled(disabled)
                    .on_click(move |_, window, cx| {
                        dialogs::login::open(
                            login_translator.clone(),
                            login_port.clone(),
                            window,
                            cx,
                        );
                    }),
                )
                .child(
                    button(
                        "account-hero-register",
                        register_label,
                        ButtonVariant::Secondary,
                        cx,
                    )
                    .disabled(disabled)
                    .on_click(move |_, window, cx| {
                        dialogs::register::open(
                            register_translator.clone(),
                            register_port.clone(),
                            window,
                            cx,
                        );
                    }),
                ),
        )
        .when_some(last_error, |this, error| {
            this.child(
                div()
                    .mt(tokens.spacing.md)
                    .text_size(tokens.typography.xs.size)
                    .text_color(tokens.colors.destructive)
                    .child(error),
            )
        })
}
