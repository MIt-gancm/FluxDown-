//! 由真实文件字节区间投影的进度条。窄列中将多个区间聚合到像素，
//! 绝不把总下载字节均摊到分段或将分段数当作活跃连接数。

use fluxdown_protocol::TaskRuntimeDto;
use gpui::{
    AnyElement, Hsla, IntoElement as _, ParentElement as _, Pixels, Styled as _, div, px, relative,
};

/// 「正在写入但尚无已完成字节」的像素用主色的这一透明度绘制：与已完成部分同色系
/// 但明显更浅，表达「活跃连接所在位置」而不冒充已下载进度。
const ACTIVE_SEGMENT_ALPHA: f32 = 0.4;
/// 相邻分段只有在投影宽度至少这么多像素时才留 1px 细缝；更窄时细缝会吞掉
/// 分段本身，改为按覆盖率叠加。
const MIN_SEGMENT_WIDTH_FOR_GAP: f64 = 5.;
/// 像素覆盖率达到 `1 - FULL_COVERAGE_EPSILON` 即视为已完成（吸收浮点累加误差）。
const FULL_COVERAGE_EPSILON: f32 = 1e-5;
/// 进度条最大像素宽度：防御异常列宽导致逐像素节点数失控。
const MAX_BAR_WIDTH: f32 = 4096.;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Pixel {
    completed: f32,
    active: bool,
    gap: bool,
}

/// 所有坐标限制在 [0, width)；把不可见的小分段按像素合并，节点数至多与宽度成正比。
fn pixels(runtime: &TaskRuntimeDto, width: usize) -> Vec<Pixel> {
    let mut result = vec![Pixel::default(); width];
    let total = runtime.total_bytes.max(0) as u64;
    if total == 0 || width == 0 {
        return result;
    }
    let scale = width as f64 / total as f64;
    for segment in &runtime.segments {
        // Wire range 为 [start_byte, end_byte]（含末字节），下载量属于本段。
        let start = segment.start_byte.max(0) as u64;
        let end = (segment.end_byte.max(0) as u64)
            .saturating_add(1)
            .min(total);
        if end <= start || start >= total {
            continue;
        }
        let done = start
            .saturating_add(segment.downloaded_bytes.max(0) as u64)
            .min(end);
        let left = start as f64 * scale;
        let right = end as f64 * scale;
        let filled = done as f64 * scale;
        let first = (left.floor() as usize).min(width);
        let last = (right.ceil() as usize).min(width);
        for (offset, pixel) in result[first..last].iter_mut().enumerate() {
            let x = (first + offset) as f64;
            pixel.completed += (filled.min(x + 1.) - left.max(x)).clamp(0., 1.) as f32;
            if segment.active == Some(true) && x < right && x + 1. > filled {
                pixel.active = true;
            }
        }
        // 只有边界能在足够宽的相邻段间看清时才留 1px 细缝；
        // 像素密度高时按覆盖率叠加，不因缝隙吞掉整个小分段。
        if right - left >= MIN_SEGMENT_WIDTH_FOR_GAP && end < total {
            let boundary = right.floor() as usize;
            if boundary < width {
                result[boundary].gap = true;
            }
        }
    }
    for pixel in &mut result {
        pixel.completed = pixel.completed.min(1.);
    }
    result
}

/// 主列表与详情共用的真实分段进度条；无分段事实时退回任务总进度。
/// `width` 是可用像素宽度，按可见列重新投影而非按段数分配宽度；`height` / `radius`
/// 由调用方按主题（`components.progress.*`）给出。
pub(crate) fn render_segment_progress(
    runtime: Option<&TaskRuntimeDto>,
    fallback_progress: f32,
    width: f32,
    height: Pixels,
    radius: Pixels,
    color: Hsla,
    muted: Hsla,
) -> AnyElement {
    let width = width.max(0.).floor().min(MAX_BAR_WIDTH) as usize;
    let mut track = div()
        .relative()
        .flex_none()
        .w(px(width as f32))
        .h(height.max(px(1.)))
        .overflow_hidden()
        .rounded(radius)
        .bg(muted);
    if let Some(runtime) =
        runtime.filter(|runtime| runtime.total_bytes > 0 && !runtime.segments.is_empty())
    {
        let mut active_color = color;
        active_color.a *= ACTIVE_SEGMENT_ALPHA;
        let columns = pixels(runtime, width);
        let mut x = 0;
        while x < columns.len() {
            let pixel = columns[x];
            if pixel.gap {
                x += 1;
                continue;
            }
            if pixel.completed >= 1. - FULL_COVERAGE_EPSILON {
                let left = x;
                while x < columns.len()
                    && !columns[x].gap
                    && columns[x].completed >= 1. - FULL_COVERAGE_EPSILON
                {
                    x += 1;
                }
                track = track.child(
                    div()
                        .absolute()
                        .left(px(left as f32))
                        .top_0()
                        .w(px((x - left) as f32))
                        .h_full()
                        .bg(color),
                );
                continue;
            }
            if pixel.active && pixel.completed <= 0. {
                let left = x;
                while x < columns.len()
                    && !columns[x].gap
                    && columns[x].active
                    && columns[x].completed <= 0.
                {
                    x += 1;
                }
                track = track.child(
                    div()
                        .absolute()
                        .left(px(left as f32))
                        .top_0()
                        .w(px((x - left) as f32))
                        .h_full()
                        .bg(active_color),
                );
                continue;
            }
            if pixel.active {
                track = track.child(
                    div()
                        .absolute()
                        .left(px(x as f32 + pixel.completed))
                        .top_0()
                        .w(px(1. - pixel.completed))
                        .h_full()
                        .bg(active_color),
                );
            }
            if pixel.completed > 0. {
                track = track.child(
                    div()
                        .absolute()
                        .left(px(x as f32))
                        .top_0()
                        .w(px(pixel.completed))
                        .h_full()
                        .bg(color),
                );
            }
            x += 1;
        }
    } else {
        track = track.child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .h_full()
                .w(relative(fallback_progress.clamp(0., 1.)))
                .bg(color),
        );
    }
    track.into_any_element()
}

#[cfg(test)]
mod tests {
    use fluxdown_protocol::{TaskRuntimeDto, TaskSegmentDto};

    use super::pixels;

    fn segment(start: i64, end: i64, downloaded: i64, active: bool) -> TaskSegmentDto {
        TaskSegmentDto {
            start_byte: start,
            end_byte: end,
            downloaded_bytes: downloaded,
            active: Some(active),
            ..Default::default()
        }
    }

    #[test]
    fn uneven_ranges_preserve_their_actual_bytes_and_active_state() {
        let runtime = TaskRuntimeDto {
            total_bytes: 100,
            segments: vec![segment(0, 9, 10, false), segment(10, 99, 20, true)],
            ..Default::default()
        };
        let columns = pixels(&runtime, 100);
        assert_eq!(columns[5].completed, 1.);
        assert_eq!(columns[15].completed, 1.);
        assert_eq!(columns[50].completed, 0.);
        assert!(columns[50].active);
        assert!(!columns[5].active);
    }

    #[test]
    fn narrow_bar_aggregates_thousands_of_ranges_without_overflow() {
        let runtime = TaskRuntimeDto {
            total_bytes: 10_000,
            segments: (0..10_000)
                .map(|i| segment(i, i, i % 2, i % 3 == 0))
                .collect(),
            ..Default::default()
        };
        let columns = pixels(&runtime, 5);
        assert_eq!(columns.len(), 5);
        assert!(
            columns
                .iter()
                .all(|column| (column.completed - 0.5).abs() < 0.001)
        );
        assert!(columns.iter().all(|column| !column.gap && column.active));
    }

    #[test]
    fn malformed_ranges_are_clipped_to_file_and_segment_boundaries() {
        let runtime = TaskRuntimeDto {
            total_bytes: 10,
            segments: vec![segment(-5, 2, 999, false), segment(9, 1000, 999, false)],
            ..Default::default()
        };
        let columns = pixels(&runtime, 10);
        assert_eq!(columns[2].completed, 1.);
        assert_eq!(columns[5].completed, 0.);
        assert_eq!(columns[9].completed, 1.);
        assert!(pixels(&runtime, 0).is_empty());
    }
}
