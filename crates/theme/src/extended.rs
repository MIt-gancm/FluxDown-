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

/// 相对亮度的黑白分界：背景亮于它时与黑色的对比度更高（√(1.05 × 0.05) − 0.05）。
const CONTRAST_PIVOT: f32 = 0.179_13;
/// 对比度派生每步调整的 HSL 亮度。
const CONTRAST_STEP: f32 = 0.01;

/// WCAG 对比度（参数为相对亮度，顺序无关）。
fn contrast_ratio(a: f32, b: f32) -> f32 {
    let (light, dark) = if a > b { (a, b) } else { (b, a) };
    (light + 0.05) / (dark + 0.05)
}

/// 注册表 `contrast` 表达式：保持色相 / 饱和度 / alpha，沿亮度远离 `against`（背景亮于
/// [`CONTRAST_PIVOT`] 则压暗，否则提亮），直到对比度 ≥ `min` 或亮度到头；已达标原样返回。
/// 网站端 `withMinContrast`（`website-v2/src/lib/gpui-theme/value.ts`）逐步 f32 镜像本函数。
pub(crate) fn with_min_contrast(color: Hsla, against: Hsla, min: f32) -> Hsla {
    let background = relative_luminance(against);
    let darken = background > CONTRAST_PIVOT;
    let mut color = color;
    loop {
        if contrast_ratio(relative_luminance(color), background) >= min {
            return color;
        }
        let next = if darken {
            (color.l - CONTRAST_STEP).max(0.)
        } else {
            (color.l + CONTRAST_STEP).min(1.)
        };
        if next == color.l {
            return color;
        }
        color.l = next;
    }
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
    /// `chrome` 上导航项的选中底色（中性，强调色只落在选中图标上）。
    pub nav_selected: Hsla,
    /// 选中导航项的文字色（默认正文色）。
    pub nav_selected_foreground: Hsla,
    /// 选中导航项的图标色：强调色，与 `nav_selected` 对比度 ≥ 3:1。
    pub nav_selected_icon: Hsla,
    /// 强调色文字：链接、生效中的状态、活跃计数；与 `surface` 对比度 ≥ 4.5:1。
    pub accent_text: Hsla,
    /// 输入框光标。
    pub caret: Hsla,
    /// 文本选区底色。
    pub text_selection: Hsla,
    /// 拖拽指示线。
    pub drag_border: Hsla,
    /// 拖放目标区底色。
    pub drop_target: Hsla,
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

#[cfg(test)]
mod tests {
    use gpui::{Hsla, rgb};

    use super::{contrast_ratio, with_min_contrast};
    use crate::builtin::relative_luminance;
    use crate::{AccentScheme, BuiltinThemeId, ResolveOptions, ThemeDocument, resolve_with};

    fn ratio(a: Hsla, b: Hsla) -> f32 {
        contrast_ratio(relative_luminance(a), relative_luminance(b))
    }

    #[test]
    fn compliant_color_is_returned_unchanged() {
        let navy = Hsla::from(rgb(0x1E3A8A));
        let white = Hsla::from(rgb(0xFFFFFF));
        assert!(ratio(navy, white) >= 4.5);
        assert_eq!(with_min_contrast(navy, white, 4.5), navy);
    }

    #[test]
    fn moves_only_lightness_away_from_the_background() {
        let yellow = Hsla::from(rgb(0xFACC15)).alpha(0.8);
        let on_light = with_min_contrast(yellow, Hsla::from(rgb(0xFFFFFF)), 4.5);
        assert!(on_light.l < yellow.l);
        assert_eq!(
            (on_light.h, on_light.s, on_light.a),
            (yellow.h, yellow.s, yellow.a)
        );
        assert!(ratio(on_light, Hsla::from(rgb(0xFFFFFF))) >= 4.5);

        let navy = Hsla::from(rgb(0x1E3A8A));
        let dark = Hsla::from(rgb(0x1C1C1E));
        let on_dark = with_min_contrast(navy, dark, 4.5);
        assert!(on_dark.l > navy.l);
        assert!(ratio(on_dark, dark) >= 4.5);
    }

    /// 任意强调色（含浅黄 / 浅青 / 近黑这类原色不可读的选择）× 全部内置主题：
    /// 强调色文字与内容底色 ≥ 4.5:1，选中导航图标与选中底色 ≥ 3:1。
    #[test]
    fn accent_derivatives_stay_readable_for_any_accent() {
        let extremes = [0xFFFA_CC15, 0xFF67_E8F9, 0xFFF5_F5F5, 0xFF11_1111];
        let accents = AccentScheme::ALL
            .map(|scheme| scheme.color(0xFFEF_4444))
            .into_iter()
            .chain(extremes.map(|argb| AccentScheme::Custom.color(argb)));
        for accent in accents {
            for id in BuiltinThemeId::ALL {
                let options = ResolveOptions {
                    accent: Some(accent),
                    ensure_primary_contrast: true,
                };
                let (values, _) =
                    resolve_with(&ThemeDocument::builtin(id), id.appearance(), &options);
                let theme = values.to_theme(1.);
                let colors = theme.extended.colors;
                assert!(
                    ratio(colors.accent_text, theme.base.colors.surface) >= 4.5,
                    "{id:?} {accent:?}"
                );
                assert!(
                    ratio(colors.nav_selected_icon, colors.nav_selected) >= 3.,
                    "{id:?} {accent:?}"
                );
            }
        }
    }
}
