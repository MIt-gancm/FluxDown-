//! 任务详情「来源构成」：把已下载字节拆成源站 / CDN / 代理 / 多网卡 / P2P 几段。
//!
//! 与 Web SPA 的同名算法逐条一致（同一组用例）：计数器只覆盖加速路径，
//! 源站 = 已下载 − 加速路径之和；采样偏差导致加速之和超过已下载时按比例缩放。

use super::TaskProtocol;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceKind {
    Origin,
    Cdn,
    Proxy,
    Nic,
    P2p,
}

impl SourceKind {
    /// 图例名称的 i18n key。
    pub(crate) fn i18n_key(self) -> &'static str {
        match self {
            Self::Origin => "sourceOrigin",
            Self::Cdn => "sourceCdn",
            Self::Proxy => "sourceProxy",
            Self::Nic => "sourceNic",
            Self::P2p => "sourceP2p",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceSlice {
    pub(crate) kind: SourceKind,
    pub(crate) bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceComposition {
    pub(crate) downloaded: u64,
    pub(crate) slices: Vec<SourceSlice>,
    /// 缩放后的加速路径字节之和（P2P 恒为 0）。
    pub(crate) accelerated_bytes: u64,
    pub(crate) p2p: bool,
}

impl SourceComposition {
    /// 尚无已下载字节：界面显示空态而不是图表。
    pub(crate) fn is_empty(&self) -> bool {
        self.downloaded == 0
    }

    /// `bytes` 占已下载的比例；已下载为 0 时为 0。
    pub(crate) fn fraction(&self, bytes: u64) -> f64 {
        if self.downloaded == 0 {
            0.
        } else {
            bytes as f64 / self.downloaded as f64
        }
    }

    pub(crate) fn accelerated_share(&self) -> f64 {
        self.fraction(self.accelerated_bytes)
    }
}

/// 负数按 0；`downloaded` 与各计数器都在此归一。
fn clamp(value: i64) -> u64 {
    value.max(0) as u64
}

pub(crate) fn compose(
    protocol: TaskProtocol,
    downloaded: i64,
    cdn: i64,
    proxy: i64,
    nic: i64,
) -> SourceComposition {
    let downloaded = clamp(downloaded);
    if matches!(protocol, TaskProtocol::Bt | TaskProtocol::Ed2k) {
        return SourceComposition {
            downloaded,
            slices: vec![SourceSlice {
                kind: SourceKind::P2p,
                bytes: downloaded,
            }],
            accelerated_bytes: 0,
            p2p: true,
        };
    }
    let (cdn, proxy, nic) = (clamp(cdn), clamp(proxy), clamp(nic));
    let accel = u128::from(cdn) + u128::from(proxy) + u128::from(nic);
    let scale = |bytes: u64| -> u64 {
        if accel > u128::from(downloaded) {
            // bytes ≤ accel，结果 ≤ downloaded，不会溢出 u64。
            (u128::from(bytes) * u128::from(downloaded) / accel) as u64
        } else {
            bytes
        }
    };
    let (cdn, proxy, nic) = (scale(cdn), scale(proxy), scale(nic));
    let accelerated_bytes = cdn + proxy + nic;
    let origin = downloaded - accelerated_bytes;

    let is_http = protocol == TaskProtocol::Http;
    let mut slices = vec![SourceSlice {
        kind: SourceKind::Origin,
        bytes: origin,
    }];
    for (kind, bytes) in [
        (SourceKind::Cdn, cdn),
        (SourceKind::Proxy, proxy),
        (SourceKind::Nic, nic),
    ] {
        if is_http || bytes > 0 {
            slices.push(SourceSlice { kind, bytes });
        }
    }
    SourceComposition {
        downloaded,
        slices,
        accelerated_bytes,
        p2p: false,
    }
}

/// 一位小数百分比；恰为 0 显示「0%」，不足 0.1% 的正数显示「<0.1%」。
pub(crate) fn format_percent(fraction: f64) -> String {
    if fraction <= 0. {
        "0%".to_owned()
    } else if fraction < 0.001 {
        "<0.1%".to_owned()
    } else {
        // JS toFixed(1) 对恰好半档向上取整，Rust 的 {:.1} 是就近偶数，先手动 round 对齐 web。
        format!("{:.1}%", (fraction * 1000.).round() / 10.)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(c: &SourceComposition) -> Vec<(SourceKind, u64)> {
        c.slices.iter().map(|s| (s.kind, s.bytes)).collect()
    }

    #[test]
    fn format_percent_rounds_half_up_like_web() {
        assert_eq!(format_percent(0.0625), "6.3%");
        assert_eq!(format_percent(0.3125), "31.3%");
        assert_eq!(format_percent(0.0004), "<0.1%");
    }

    #[test]
    fn http_without_accel_is_all_origin() {
        let c = compose(TaskProtocol::Http, 1000, 0, 0, 0);
        assert_eq!(
            rows(&c),
            [
                (SourceKind::Origin, 1000),
                (SourceKind::Cdn, 0),
                (SourceKind::Proxy, 0),
                (SourceKind::Nic, 0),
            ]
        );
        assert_eq!(c.accelerated_share(), 0.);
    }

    #[test]
    fn accel_within_downloaded_subtracts_from_origin() {
        let c = compose(TaskProtocol::Http, 1000, 250, 125, 0);
        assert_eq!(
            rows(&c),
            [
                (SourceKind::Origin, 625),
                (SourceKind::Cdn, 250),
                (SourceKind::Proxy, 125),
                (SourceKind::Nic, 0),
            ]
        );
        assert_eq!(format_percent(c.accelerated_share()), "37.5%");
    }

    #[test]
    fn overshoot_scales_down_and_sums_to_downloaded() {
        let c = compose(TaskProtocol::Http, 1000, 900, 300, 0);
        assert_eq!(
            rows(&c),
            [
                (SourceKind::Origin, 0),
                (SourceKind::Cdn, 750),
                (SourceKind::Proxy, 250),
                (SourceKind::Nic, 0),
            ]
        );
        assert_eq!(c.slices.iter().map(|s| s.bytes).sum::<u64>(), 1000);
    }

    #[test]
    fn p2p_protocols_are_a_single_p2p_slice() {
        for protocol in [TaskProtocol::Bt, TaskProtocol::Ed2k] {
            let c = compose(protocol, 500, 10, 10, 10);
            assert_eq!(rows(&c), [(SourceKind::P2p, 500)]);
            assert!(c.p2p);
            assert_eq!(c.accelerated_share(), 0.);
        }
    }

    #[test]
    fn negative_inputs_clamp_to_zero() {
        let c = compose(TaskProtocol::Http, -5, -1, -2, -3);
        assert!(c.is_empty());
        let c = compose(TaskProtocol::Http, 100, -50, 40, -1);
        assert_eq!(
            rows(&c),
            [
                (SourceKind::Origin, 60),
                (SourceKind::Cdn, 0),
                (SourceKind::Proxy, 40),
                (SourceKind::Nic, 0),
            ]
        );
    }

    #[test]
    fn ftp_lists_only_origin_when_no_accel_bytes() {
        let c = compose(TaskProtocol::Ftp, 100, 0, 0, 0);
        assert_eq!(rows(&c), [(SourceKind::Origin, 100)]);
    }

    #[test]
    fn hls_lists_accel_rows_that_have_bytes() {
        let c = compose(TaskProtocol::Hls, 100, 0, 0, 10);
        assert_eq!(rows(&c), [(SourceKind::Origin, 90), (SourceKind::Nic, 10)]);
    }

    #[test]
    fn zero_downloaded_is_empty() {
        let c = compose(TaskProtocol::Http, 0, 5, 0, 0);
        assert!(c.is_empty());
        assert_eq!(c.accelerated_share(), 0.);
    }

    #[test]
    fn percent_formatting() {
        assert_eq!(format_percent(0.), "0%");
        assert_eq!(format_percent(0.0004), "<0.1%");
        assert_eq!(format_percent(0.875), "87.5%");
    }
}
