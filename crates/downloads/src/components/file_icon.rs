//! 系统文件图标：经 `agent.platform.fileIcon` 取系统文件管理器（资源管理器 / Finder / GTK
//! 图标主题）为文件显示的彩色图标，替代按类型的 Lucide 图标。
//!
//! - 走 GPUI 资产缓存（[`Window::use_asset`]）去重：同一「扩展名 + 物理像素」只取一次；图标
//!   内嵌在文件里的类型（[`FILE_ICON_PER_FILE_EXTENSIONS`]）在本机产物存在时按文件取，键里带
//!   完成时间，同名重新下载后换新图标。
//! - 取完后 GPUI 通知发起渲染的视图重绘。取不到（agent 不支持 / 失败）的结果同样缓存，本会话
//!   内不再重试，调用方一直显示回退图标。

use std::{future::Future, sync::Arc};

use fluxdown_protocol::{FILE_ICON_PER_FILE_EXTENSIONS, PlatformFileIconParams};
use gpui::{
    AnyElement, App, Asset, Global, Image, ImageFormat, IntoElement, Pixels, SharedString,
    Styled as _, Window, img, prelude::FluentBuilder as _,
};

use crate::{
    controller::{DownloadsCommand, DownloadsPort, DownloadsResult},
    model::DownloadTaskView,
};

/// 已完成但文件已不在下载目录的行：图标与次要色文件名一起退淡。
const MISSING_FILE_OPACITY: f32 = 0.5;

/// 取图标用的端口（下载页创建时注入，所有下载窗口共用同一个 agent 连接）。
struct FileIconPort(Arc<dyn DownloadsPort>);

impl Global for FileIconPort {}

/// 注入取图标用的端口；已注入时不覆盖。
pub(crate) fn install_port(port: &Arc<dyn DownloadsPort>, cx: &mut App) {
    if !cx.has_global::<FileIconPort>() {
        cx.set_global(FileIconPort(Arc::clone(port)));
    }
}

/// 一次渲染时的系统图标状态。
pub(crate) enum SystemFileIcon {
    /// 请求中：调用方留空，避免先闪一下回退图标再换成系统图标。
    Loading,
    Ready(AnyElement),
    /// 没有可用的系统图标（无文件名 / agent 不支持 / 提取失败）：显示回退图标。
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct FileIconKey {
    /// 小写、不带点；空 = 无扩展名。
    extension: SharedString,
    /// 按文件取时的本机路径；`None` = 按扩展名取。
    path: Option<SharedString>,
    /// 按文件取时的完成时间：同路径重新下载后换键。
    revision: i64,
    /// 物理像素边长。
    size: u32,
}

impl FileIconKey {
    fn for_task(task: &DownloadTaskView, size: u32) -> Option<Self> {
        if task.name.is_empty() {
            return None;
        }
        let extension = task.file_extension.clone();
        let path = (task.has_local_file()
            && FILE_ICON_PER_FILE_EXTENSIONS.contains(&extension.as_ref()))
        .then(|| task.local_file_path())
        .flatten()
        .map(|path| SharedString::from(path.to_string_lossy().into_owned()));
        let revision = if path.is_some() {
            task.completed_at_secs
        } else {
            0
        };
        Some(Self {
            extension,
            path,
            revision,
            size,
        })
    }
}

enum FileIconAsset {}

impl Asset for FileIconAsset {
    type Source = FileIconKey;
    type Output = Option<Arc<Image>>;

    fn load(
        source: Self::Source,
        cx: &mut App,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        let port = cx
            .try_global::<FileIconPort>()
            .map(|port| Arc::clone(&port.0));
        async move {
            let params = PlatformFileIconParams {
                extension: source.extension.to_string(),
                path: source.path.map(|path| path.to_string()),
                size: source.size,
            };
            match port?.execute(DownloadsCommand::FileIcon(params)).await {
                Ok(DownloadsResult::FileIcon(png)) if !png.is_empty() => {
                    Some(Arc::new(Image::from_bytes(ImageFormat::Png, png)))
                }
                _ => None,
            }
        }
    }
}

/// 任务的系统图标，`size` 为逻辑边长（按窗口缩放换算成物理像素请求，高分屏不糊）。
pub(crate) fn system_file_icon(
    task: &DownloadTaskView,
    size: Pixels,
    window: &mut Window,
    cx: &mut App,
) -> SystemFileIcon {
    let physical = (f32::from(size) * window.scale_factor()).ceil() as u32;
    let Some(key) = FileIconKey::for_task(task, physical) else {
        return SystemFileIcon::Unavailable;
    };
    match window.use_asset::<FileIconAsset>(&key, cx) {
        None => SystemFileIcon::Loading,
        Some(None) => SystemFileIcon::Unavailable,
        Some(Some(image)) => SystemFileIcon::Ready(
            img(image)
                .flex_none()
                .size(size)
                .when(task.is_file_missing(), |this| {
                    this.opacity(MISSING_FILE_OPACITY)
                })
                .into_any_element(),
        ),
    }
}
