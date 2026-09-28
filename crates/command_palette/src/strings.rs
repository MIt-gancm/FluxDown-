//! 命令面板的类型化文案。

use fluxdown_ui_i18n::Translator;
use gpui::SharedString;

#[derive(Clone)]
pub(crate) struct PaletteStrings {
    pub(crate) placeholder: SharedString,
    pub(crate) no_results: SharedString,
}

impl PaletteStrings {
    pub(crate) fn new(translator: &Translator) -> Self {
        let text = |key: &str| SharedString::from(translator.text(key).to_owned());
        Self {
            placeholder: text("commandPalettePlaceholder"),
            no_results: text("commandPaletteNoResults"),
        }
    }
}
