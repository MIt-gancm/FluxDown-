//! 账号与安全分组：分组标题 + 邮箱只读行。
//!
//! Flutter 侧可点邮箱打开修改邮箱对话框（`/me/email` 多步验证）；本能力尚未
//! 暴露对应端口方法，故此处保持只读展示，见交付报告的协议缺口清单。

use fluxdown_protocol::AgentSessionDto;
use fluxdown_ui_components::card;
use fluxdown_ui_i18n::Translator;
use fluxdown_ui_theme::SemanticThemeTokens;
use gpui::{Context, FontWeight, IntoElement, ParentElement, Styled, div};
use gpui_component::h_flex;

use crate::view::AccountView;
use crate::{t, ui};

pub(crate) fn render(
    translator: &Translator,
    tokens: &SemanticThemeTokens,
    session: &AgentSessionDto,
    cx: &mut Context<AccountView>,
) -> impl IntoElement {
    let heading = t(translator, "accountSecurityGroup");
    let desc = t(translator, "accountSecurityGroupDesc");
    let email_label = t(translator, "accountEmailPlaceholder");
    let body = &tokens.typography.sm;

    div()
        .flex()
        .flex_col()
        .gap(tokens.spacing.sm)
        .child(ui::group_heading(heading, Some(desc), cx))
        .child(
            card(cx).w_full().p(tokens.spacing.md).child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .text_size(body.size)
                    .line_height(body.line_height)
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(tokens.colors.foreground)
                            .child(email_label),
                    )
                    .child(
                        div()
                            .text_color(tokens.colors.muted_foreground)
                            .child(session.user.email.clone()),
                    ),
            ),
        )
}
