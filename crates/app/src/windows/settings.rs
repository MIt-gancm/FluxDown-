//! 设置窗口：账户 / 扩展视图各自 attach 会话；边界持久化。

use std::sync::Arc;

use fluxdown_ui_account::AccountView;
use fluxdown_ui_extensions::ExtensionsView;
use fluxdown_ui_i18n::keys;
use fluxdown_ui_settings::{SettingsContentSlots, SettingsView};
use fluxdown_ui_shell::{AuxiliaryWindowView, auxiliary_window_options};
use gpui::{App, AppContext as _, px, size};
use gpui_component::Root;

use crate::{
    app::Desktop,
    capability_ports::AgentExtensionsPort,
    session::attach,
    windows::{WindowKey, WindowRegistry},
};

const SETTINGS_WINDOW_SIZE: gpui::Size<gpui::Pixels> = size(px(1240.), px(760.));

pub fn open(cx: &mut App) {
    let desktop = Desktop::global(cx);
    let translator = desktop.translator.clone();
    let session = desktop.session.clone();
    let client = desktop.client.clone();
    let settings_store = desktop.settings_store.clone();
    let account_host = desktop.account_host.clone();
    let title = translator.read(cx).text(keys::SETTINGS).to_owned();
    let stored = Desktop::pref(cx, "desktop.window.settings");
    let mut options = auxiliary_window_options(title);
    options.window_bounds = Some(WindowRegistry::restore_bounds(
        &WindowKey::Settings,
        stored.as_ref(),
        SETTINGS_WINDOW_SIZE,
        cx,
    ));
    options.window_min_size = Some(size(px(1000.), px(600.)));

    WindowRegistry::open_or_focus(cx, WindowKey::Settings, options, move |window, cx| {
        let extensions_port = Arc::new(AgentExtensionsPort::new(client.clone()));
        let account =
            cx.new(|cx| AccountView::new(translator.clone(), account_host.clone(), window, cx));
        let extensions = cx.new(|cx| ExtensionsView::new(translator.clone(), extensions_port, cx));
        let settings = cx.new(|cx| {
            SettingsView::new(
                translator.clone(),
                settings_store,
                SettingsContentSlots {
                    account: Some(account.clone().into()),
                    extensions: Some(extensions.clone().into()),
                    activity_bar: crate::activity::toggles(),
                },
                window,
                cx,
            )
        });
        Desktop::global_mut(cx).settings_view = Some(settings.downgrade());
        attach(&session, &extensions, cx);
        let window_view =
            cx.new(|cx| AuxiliaryWindowView::new(translator, keys::SETTINGS, settings.into(), cx));
        let root = cx.new(|cx| Root::new(window_view, window, cx));
        WindowRegistry::persist_bounds(&WindowKey::Settings, client, &root, window, cx);
        root
    });
}
