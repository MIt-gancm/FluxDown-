//! 下载能力的键盘 / 菜单动作。键位与菜单装配由 app 完成；本 crate 只定义动作并
//! 在 [`crate::DownloadView`] 根元素上注册处理器。

use gpui::actions;

actions!(
    downloads,
    [
        NewDownload,
        OpenTorrentFile,
        PauseSelected,
        ResumeSelected,
        TogglePauseSelected,
        DeleteSelected,
        DeleteSelectedWithFiles,
        SelectAllTasks,
        ClearSelection,
        PauseAll,
        ResumeAll,
        OpenSelected,
        RevealSelected,
        OpenSelectedInWindow,
        /// 在停靠详情面板中查看选中任务（右键「详情」）。
        ShowSelectedDetail,
        CopySelectedUrl,
        RenameSelected,
        RedownloadSelected,
        ToggleBoostSelected,
        FocusSearch,
        CycleDensity,
        CycleGroupBy,
        CycleSort,
        ToggleDetailPanel,
        OpenQueueManager,
        ClearFinished,
    ]
);

/// 下载页根元素的键位上下文名。
pub const KEY_CONTEXT: &str = "DownloadView";
