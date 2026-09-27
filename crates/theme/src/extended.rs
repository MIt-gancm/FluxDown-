//! FluxDown 自有的扩展语义 token。
//!
//! gpui-base 的 [`SemanticThemeTokens`](crate::SemanticThemeTokens) 是外部类型，不含桌面
//! 列表 UI 需要的状态色、三级文字色、细分隔线、标题/注释字号、图标尺寸、线宽与焦点环。
//! 这些值由注册表默认表达式从 Base token 派生，主题文件可逐项覆盖，并与 Base token
//! 一起按界面缩放。

use gpui::{Hsla, Pixels};

use crate::TextStyleToken;
use crate::builtin::relative_luminance;

/// 主色按钮文字的最低对比度（WCAG AA 正文）。
pub(crate) const MIN_PRIMARY_CONTRAST: f32 = 4.5;

/// 亮色模式下，若主色与其前景对比不足 AA，则逐步压暗主色直至达标。
///
/// 默认蓝 `#3B82F6` 配白字只有约 3.7:1，按钮文字发虚；压暗后（≈`#2563EB`）更沉稳。
/// 前景比主色更暗时不调整。
pub(crate) fn primary_with_contrast(primary: Hsla, foreground: Hsla) -> Hsla {
    let foreground = relative_luminance(foreground);
    let mut primary = primary;
    for _ in 0..40 {
        let background = relative_luminance(primary);
        let (light, dark) = if foreground > background {
            (foreground, background)
        } else {
            (background, foreground)
        };
        if (light + 0.05) / (dark + 0.05) >= MIN_PRIMARY_CONTRAST || foreground < background {
            break;
        }
        primary.l = (primary.l - 0.01).max(0.);
    }
    primary
}

/// 扩展颜色。派生色（`text_tertiary` 等）默认不透明，叠在选中/悬停底色上不会出现
/// 透明度叠加色差。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ExtendedColors {
    /// 成功（完成）状态色：只用于小面积图标或圆点，不做大面积填充。
    pub success: Hsla,
    /// 警示色：需要注意但不是错误的状态（例如关机倒计时）。
    pub warning: Hsla,
    /// 提示信息色。
    pub info: Hsla,
    /// 三级文字：计数、时间、分区标题、占位等最弱信息。
    pub text_tertiary: Hsla,
    /// 细分隔线：比 `border` 更弱，只用于保留下来的少数结构线。
    pub hairline: Hsla,
    /// 列表行 / 导航项悬停底色。
    pub row_hover: Hsla,
    /// 侧栏与顶栏等「退后」区域的底色（比内容区 `surface` 低一个层级）。
    pub chrome: Hsla,
    /// `chrome` 上导航项的悬停底色。
    pub nav_hover: Hsla,
    /// `chrome` 上导航项的选中底色（中性，不用强调色）。
    pub nav_selected: Hsla,
    /// 任务状态文字：下载中。
    pub status_downloading: Hsla,
    /// 任务状态文字：已完成。
    pub status_completed: Hsla,
    /// 任务状态文字：失败。
    pub status_failed: Hsla,
    /// 任务状态文字：已暂停。
    pub status_paused: Hsla,
    /// 任务状态文字：排队中。
    pub status_queued: Hsla,
    /// 进度条轨道。
    pub progress_track: Hsla,
    /// 进度条填充（下载中）。
    pub progress_fill: Hsla,
}

/// 图标尺寸阶梯：全应用只用这三档。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct IconSizes {
    /// 12px：折叠箭头、行内状态点旁的小图标。
    pub sm: Pixels,
    /// 14px：状态栏、按钮内图标、表格内操作。
    pub md: Pixels,
    /// 16px：导航项、文件类型、工具栏主图标。
    pub lg: Pixels,
}

/// 线宽（不随界面缩放）。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StrokeTokens {
    /// 1px：描边与分隔线。
    pub thin: Pixels,
    /// 2px：强调描边（选中框、勾选线）。
    pub strong: Pixels,
}

/// 焦点环（不随界面缩放）。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FocusRingTokens {
    /// 环本身的线宽。
    pub width: Pixels,
    /// 环与控件之间的留白。
    pub offset: Pixels,
}

/// 扩展 token 快照（已按界面缩放）。
#[derive(Debug, Clone, PartialEq)]
pub struct ExtendedTokens {
    pub colors: ExtendedColors,
    /// 11/14：计数、分区标题、状态栏。
    pub caption: TextStyleToken,
    /// 15/20 半粗：页面级标题。
    pub title: TextStyleToken,
    pub icon: IconSizes,
    pub stroke: StrokeTokens,
    pub focus_ring: FocusRingTokens,
}
