//! coordinator 侧的多路径调度状态：连接窗口采样 → 路径估计、冷路径探索、
//! 完成时间抢占、`ProxyMode::Auto` 主导链路标签与跨任务先验回写。
//!
//! 判据全部来自 [`crate::path_scheduler`]；路径选择本身在
//! [`crate::cdn::NodePool::lease_for`]。本模块只在每个完整 ramp 窗口被
//! 驱动一次，不引入新的事件循环分支。

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use super::{LiveSegment, SegState};
use crate::auto_proxy::{AutoProxyCtx, RoutePath, route};
use crate::cdn::NodePool;
use crate::db::Db;
use crate::events::{EngineEvent, EventSink};
use crate::logger::{log_error, log_info};
use crate::path_scheduler::{MIN_SAMPLE_WINDOWS, completion_secs, should_preempt, window_rate};

/// 剩余字节低于此值不再探索冷路径——探索连接来不及进入稳态。
const EXPLORE_MIN_REMAINING: i64 = 4 * 1024 * 1024;

/// 抢占后抑制 ramp 扩容评估/劣化收缩的窗口数：交接期的吞吐波动不是
/// 连接额度的责任，不得触发回滚或写域名连接上限。
const MIGRATION_QUIET_TICKS: u32 = 2;

/// 单个在途连接的窗口跟踪。
struct Track {
    seg_index: i32,
    node_id: usize,
    /// 上个窗口边界时该段的已下字节。
    last_bytes: i64,
    /// 已跨越的完整窗口数（1 = 首个完整窗口，慢启动预热）。
    windows: u32,
    /// 最近一个完整窗口的字节增量。
    last_delta: i64,
    /// 最近一个完整窗口的稳态速率（预热窗/限速窗为 `None`）。
    last_rate: Option<f64>,
    preempted: bool,
}

/// 本窗口的守卫。
pub(super) struct TickGuards {
    /// 可采样（未限速）：限速窗口的速率反映限速器而非链路。
    pub sampling: bool,
    /// 可改道（Range 已验证、非串行、非连接敏感、非重连敌对、未限速）。
    pub may_reroute: bool,
    /// 任务剩余字节。
    pub remaining_total: i64,
    /// 不可抢占的段（开放式首段生命线）。
    pub protected_seg: Option<i32>,
}

/// 本窗口的调度产出。
pub(super) struct TickReport {
    /// 本窗发起的抢占数。
    pub preempted: usize,
    /// 需要为冷路径额外放出 1 个探索 worker。
    pub explore_worker_wanted: bool,
}

/// `ProxyMode::Auto` 的主导链路标签发布器。
struct RouteLabeler {
    ctx: Arc<AutoProxyCtx>,
    published: &'static str,
    pinned: bool,
}

impl RouteLabeler {
    /// 按主导路径（窗口累计字节最多）推导标签。主导路径仍是起飞路径时
    /// 保留 manager 写入的更具体标签（failover/cached），仅在备选路径被
    /// 实测过时把 `direct` 升级为 `direct:sampled`（或 validator 不一致时
    /// `direct:pinned`）。
    fn desired(&self, route_bytes: &[(RoutePath, u64)], explored: bool) -> &'static str {
        let start = self.ctx.start_route;
        let dominant = route_bytes
            .iter()
            .filter(|(_, bytes)| *bytes > 0)
            .max_by(|a, b| a.1.cmp(&b.1).then((a.0 == start).cmp(&(b.0 == start))))
            .map_or(start, |(route, _)| *route);
        if dominant != start {
            return dominant.sampled_label();
        }
        if start == RoutePath::Direct && self.ctx.start_label == route::DIRECT {
            if self.pinned {
                return route::DIRECT_PINNED;
            }
            if explored {
                return route::DIRECT_SAMPLED;
            }
        }
        self.ctx.start_label
    }
}

/// coordinator 侧多路径状态（每次 `run_coordinated_download` 一份）。
pub(super) struct Multipath {
    tracks: HashMap<u64, Track>,
    labeler: Option<RouteLabeler>,
    quiet_ticks: u32,
}

impl Multipath {
    /// 构造并（Auto 任务）为每条备选路径构建 client 挂入节点池。client
    /// 构建失败的路径跳过（与该路径不存在等价）。
    pub(super) fn new(ctx: Option<Arc<AutoProxyCtx>>, nodes: &NodePool, task_id: &str) -> Self {
        let labeler = ctx.map(|ctx| {
            let mut alternates = Vec::with_capacity(ctx.alternates.len());
            for alt in &ctx.alternates {
                match crate::downloader::build_client_with_tls_policy(
                    &alt.config,
                    &ctx.user_agent,
                    false,
                ) {
                    Ok(client) => alternates.push((alt.route, client, alt.prior_bps)),
                    Err(e) => log_error!(
                        "[multipath] task {} {:?} 路径 client 构建失败，跳过: {e}",
                        task_id,
                        alt.route
                    ),
                }
            }
            log_info!(
                "[multipath] task {} host {} 起飞 {:?}（先验 {:?}），备选路径 {:?}",
                task_id,
                ctx.host,
                ctx.start_route,
                ctx.start_prior_bps,
                alternates
                    .iter()
                    .map(|(route, _, prior)| (*route, *prior))
                    .collect::<Vec<_>>()
            );
            nodes.add_paths(ctx.start_route, ctx.start_prior_bps, alternates);
            RouteLabeler {
                published: ctx.start_label,
                ctx,
                pinned: false,
            }
        });
        Self {
            tracks: HashMap::new(),
            labeler,
            quiet_ticks: 0,
        }
    }

    /// 完整 ramp 窗口驱动：采样 → 路径估计 → 探索开关 → 完成时间抢占。
    ///
    /// `segments` 须已与共享进度同步。每个活动段的 `rate_bps` 被重写为
    /// 持有连接本窗稳态速率（未知为 `None`），供拆分挑选与均衡切分。
    pub(super) fn on_tick(
        &mut self,
        nodes: &NodePool,
        segments: &mut BTreeMap<i32, LiveSegment>,
        window: Duration,
        guards: &TickGuards,
        task_id: &str,
    ) -> TickReport {
        let live = nodes.live_conns();
        let mut samples: Vec<(usize, f64)> = Vec::new();
        for seg in segments.values_mut() {
            seg.rate_bps = None;
        }
        let mut next_tracks: HashMap<u64, Track> = HashMap::with_capacity(live.len());
        for conn in &live {
            let Some(seg) = segments.get_mut(&conn.seg_index) else {
                continue;
            };
            let current = seg.downloaded_bytes;
            let mut track = match self.tracks.remove(&conn.lease_id) {
                Some(mut track) => {
                    let delta = (current - track.last_bytes).max(0);
                    track.last_bytes = current;
                    track.last_delta = delta;
                    track.windows = track.windows.saturating_add(1);
                    track.last_rate = (guards.sampling && track.windows >= MIN_SAMPLE_WINDOWS)
                        .then(|| window_rate(delta, window));
                    track
                }
                // 首次见到：窗口起点未知，只建立基线。
                None => Track {
                    seg_index: conn.seg_index,
                    node_id: conn.node_id,
                    last_bytes: current,
                    last_delta: 0,
                    windows: 0,
                    last_rate: None,
                    preempted: false,
                },
            };
            if let Some(rate) = track.last_rate {
                samples.push((track.node_id, rate));
                if seg.state == SegState::Active {
                    seg.rate_bps = Some(rate);
                }
            }
            track.seg_index = conn.seg_index;
            next_tracks.insert(conn.lease_id, track);
        }
        self.tracks = next_tracks;
        nodes.observe_window(&samples);

        let explore = guards.may_reroute && guards.remaining_total >= EXPLORE_MIN_REMAINING;
        nodes.set_explore(explore);

        let preempted = if guards.may_reroute && nodes.is_multipath() {
            self.preempt_stragglers(nodes, segments, guards, task_id, live.len())
        } else {
            0
        };
        if preempted > 0 {
            self.quiet_ticks = MIGRATION_QUIET_TICKS;
        }
        TickReport {
            preempted,
            explore_worker_wanted: explore && nodes.has_idle_cold_path(),
        }
    }

    /// 完成时间抢占：按预计完成时间从晚到早，至多 `max(1, 在途/4)` 条。
    fn preempt_stragglers(
        &mut self,
        nodes: &NodePool,
        segments: &BTreeMap<i32, LiveSegment>,
        guards: &TickGuards,
        task_id: &str,
        live_count: usize,
    ) -> usize {
        let Some(best) = nodes.best_measured_rate() else {
            return 0;
        };
        let mut candidates: Vec<(f64, u64, i32, i64, f64)> = Vec::new();
        for (&lease_id, track) in &self.tracks {
            if track.preempted || guards.protected_seg == Some(track.seg_index) {
                continue;
            }
            let Some(seg) = segments.get(&track.seg_index) else {
                continue;
            };
            if seg.state != SegState::Active {
                continue;
            }
            // 稳态样本，或首个完整窗口即零字节（停滞无需等慢启动）。
            let rate = match track.last_rate {
                Some(rate) => rate,
                None if track.windows >= 1 && guards.sampling && track.last_delta == 0 => 0.0,
                None => continue,
            };
            let remaining = seg.remaining();
            if should_preempt(remaining, rate, best) {
                candidates.push((
                    completion_secs(remaining, rate),
                    lease_id,
                    track.seg_index,
                    remaining,
                    rate,
                ));
            }
        }
        candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
        let budget = (live_count / 4).max(1);
        let mut done = 0;
        let mut stalled_nodes: Vec<(usize, f64)> = Vec::new();
        for (_, lease_id, seg_index, remaining, rate) in candidates.into_iter().take(budget) {
            if nodes.preempt(lease_id) {
                if let Some(track) = self.tracks.get_mut(&lease_id) {
                    track.preempted = true;
                    // 整窗零字节的停滞连接：以 0 速率计入该路径实证，冷路径不再
                    // 被反复探索，已知路径跌出竞争集。
                    if rate == 0.0 {
                        stalled_nodes.push((track.node_id, 0.0));
                    }
                }
                done += 1;
                log_info!(
                    "[multipath] task {} seg {} 抢占：剩余 {} B，本连接 {:.0} B/s，最优路径 {:.0} B/s",
                    task_id,
                    seg_index,
                    remaining,
                    rate,
                    best
                );
            }
        }
        if !stalled_nodes.is_empty() {
            nodes.observe_window(&stalled_nodes);
        }
        done
    }

    /// 本窗是否处于交接期（抑制 ramp 扩容评估与劣化收缩）。每窗调用一次。
    pub(super) fn take_quiet_tick(&mut self) -> bool {
        if self.quiet_ticks > 0 {
            self.quiet_ticks -= 1;
            true
        } else {
            false
        }
    }

    /// 备选路径是否承载过流量（为真时连接规模观察混有代理连接，不得学习
    /// 为源站域名连接上限/起步提示）。
    pub(super) fn alternates_used(&self, nodes: &NodePool) -> bool {
        self.labeler.is_some() && nodes.alternates_explored()
    }

    /// 处理代理路径 validator 踢除（记 NoSwitch）并按主导路径（已结束租约
    /// 回报 + 在途租约进度的累计传输字节）发布链路标签。
    pub(super) async fn publish_route(
        &mut self,
        nodes: &NodePool,
        segments: &BTreeMap<i32, LiveSegment>,
        db: &Db,
        sink: &dyn EventSink,
        task_id: &str,
    ) {
        let Some(labeler) = self.labeler.as_mut() else {
            return;
        };
        for kicked in nodes.take_validator_kicks() {
            log_info!(
                "[multipath] task {} host {} {:?} 路径 validator 不一致，已踢除并记录 NoSwitch",
                task_id,
                labeler.ctx.host,
                kicked
            );
            crate::route_health::record_no_switch(&labeler.ctx.host, db);
            labeler.pinned = true;
        }
        let in_flight: Vec<(usize, u64)> = nodes
            .live_conns()
            .iter()
            .filter_map(|conn| {
                let seg = segments.get(&conn.seg_index)?;
                Some((
                    conn.node_id,
                    (seg.downloaded_bytes - conn.start_downloaded).max(0) as u64,
                ))
            })
            .collect();
        let label = labeler.desired(&nodes.route_bytes(&in_flight), nodes.alternates_explored());
        if label == labeler.published {
            return;
        }
        labeler.published = label;
        log_info!("[multipath] task {} 主导链路: {}", task_id, label);
        if let Err(e) = db.set_task_auto_route(task_id, label).await {
            log_error!("[multipath] task {} 路由落库失败: {e:#}", task_id);
        }
        sink.emit(EngineEvent::TaskRouteChanged {
            task_id: task_id.to_string(),
            route: label.to_string(),
        });
    }

    /// 任务结束：把已实测的各路径单连接估计写入跨任务折扣先验。
    pub(super) fn record_priors(&self, nodes: &NodePool, db: &Db) {
        let Some(labeler) = self.labeler.as_ref() else {
            return;
        };
        for (route, bps) in nodes.path_estimates() {
            crate::route_health::record_path_rate(&labeler.ctx.host, route, bps, db);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::auto_proxy::CandidateSource;

    const MANUAL: RoutePath = RoutePath::Proxy(CandidateSource::ManualFields);

    fn labeler(start: RoutePath, start_label: &'static str) -> RouteLabeler {
        RouteLabeler {
            ctx: Arc::new(AutoProxyCtx {
                host: "h".to_string(),
                user_agent: String::new(),
                start_route: start,
                start_prior_bps: None,
                start_label,
                alternates: Vec::new(),
            }),
            published: start_label,
            pinned: false,
        }
    }

    #[test]
    fn label_follows_dominant_path() {
        let l = labeler(RoutePath::Direct, route::DIRECT);
        assert_eq!(l.desired(&[], false), route::DIRECT);
        assert_eq!(
            l.desired(&[(RoutePath::Direct, 10), (MANUAL, 0)], true),
            route::DIRECT_SAMPLED
        );
        assert_eq!(
            l.desired(&[(RoutePath::Direct, 10), (MANUAL, 30)], true),
            route::PROXY_SAMPLED_MANUAL
        );
    }

    #[test]
    fn start_specific_labels_survive_while_start_path_dominates() {
        let failover = labeler(RoutePath::Direct, route::DIRECT_FAILOVER);
        assert_eq!(
            failover.desired(&[(RoutePath::Direct, 10), (MANUAL, 1)], true),
            route::DIRECT_FAILOVER
        );
        let cached = labeler(MANUAL, route::PROXY_CACHED_MANUAL);
        assert_eq!(
            cached.desired(&[(MANUAL, 10), (RoutePath::Direct, 1)], true),
            route::PROXY_CACHED_MANUAL
        );
        assert_eq!(
            cached.desired(&[(MANUAL, 1), (RoutePath::Direct, 10)], true),
            route::DIRECT_SAMPLED
        );
    }

    #[test]
    fn validator_kick_pins_direct_label() {
        let mut l = labeler(RoutePath::Direct, route::DIRECT);
        l.pinned = true;
        assert_eq!(
            l.desired(&[(RoutePath::Direct, 10), (MANUAL, 1)], true),
            route::DIRECT_PINNED
        );
    }
}
