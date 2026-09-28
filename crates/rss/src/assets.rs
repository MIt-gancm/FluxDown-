use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

/// RSS 能力在活动栏使用的图标路径。
pub const RSS_ICON_PATH: &str = "fluxdown/icons/rss.svg";
const RSS_ICON: &[u8] = include_bytes!("../assets/rss.svg");

/// RSS 能力拥有的嵌入资源。
pub struct RssAssets;

impl AssetSource for RssAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok((path == RSS_ICON_PATH).then_some(Cow::Borrowed(RSS_ICON)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(if RSS_ICON_PATH.starts_with(path) {
            vec![RSS_ICON_PATH.into()]
        } else {
            Vec::new()
        })
    }
}
