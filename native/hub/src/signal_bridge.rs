//! `engine::model::*` ↔ `hub::signals::*` 类型转换。
//!
//! orphan rule 决定了不能跨 crate 共享同一 derive 类型(`fluxdown_engine` 不
//! 知道、也不能依赖 `rinf`),因此这里为每个引擎领域类型手写一个到对应
//! Dart 信号 DTO 的 `From` 实现——这是标准 repository-pattern 边界收口做法,
//! 内容是搬移字段而非新写业务逻辑。

use fluxdown_engine::model;

use crate::signals;

impl From<model::TaskInfo> for signals::TaskInfo {
    fn from(t: model::TaskInfo) -> Self {
        Self {
            task_id: t.task_id,
            url: t.url,
            file_name: t.file_name,
            save_dir: t.save_dir,
            status: t.status,
            downloaded_bytes: t.downloaded_bytes,
            total_bytes: t.total_bytes,
            error_message: t.error_message,
            created_at: t.created_at,
            proxy_url: t.proxy_url,
            queue_id: t.queue_id,
            checksum: t.checksum,
            ignore_tls_errors: t.ignore_tls_errors,
            file_missing: t.file_missing,
            completed_at: t.completed_at,
            segments: t.segments,
            queue_order: t.queue_order,
            uploaded_bytes: t.uploaded_bytes,
            uploaded_at_completion: t.uploaded_at_completion,
            seeding_status: t.seeding_status,
            seeding_message: t.seeding_message,
            seeding_time_secs: t.seeding_time_secs,
            seed_ratio_limit_milli: t.seed_ratio_limit_milli,
            seed_post_ratio_limit_milli: t.seed_post_ratio_limit_milli,
            seed_time_limit_minutes: t.seed_time_limit_minutes,
            seed_inactive_time_limit_minutes: t.seed_inactive_time_limit_minutes,
            seed_upload_limit_bps: t.seed_upload_limit_bps,
            referrer: t.referrer,
            group_id: t.group_id,
            rss_source_id: t.rss_source_id,
            origin_url: t.origin_url,
            auto_route: t.auto_route,
        }
    }
}

impl From<model::QueueInfo> for signals::QueueInfo {
    fn from(q: model::QueueInfo) -> Self {
        Self {
            queue_id: q.queue_id,
            name: q.name,
            speed_limit_kbps: q.speed_limit_kbps,
            upload_limit_kbps: q.upload_limit_kbps,
            max_concurrent: q.max_concurrent,
            default_save_dir: q.default_save_dir,
            position: q.position,
            default_segments: q.default_segments,
            default_user_agent: q.default_user_agent,
            is_running: q.is_running,
            schedule_enabled: q.schedule_enabled,
            schedule_start: q.schedule_start,
            schedule_stop: q.schedule_stop,
            schedule_days: q.schedule_days,
        }
    }
}

impl From<model::QueuePosition> for signals::QueuePosition {
    fn from(p: model::QueuePosition) -> Self {
        Self {
            task_id: p.task_id,
            position: p.position,
        }
    }
}

impl From<model::SegmentDetail> for signals::SegmentDetail {
    fn from(s: model::SegmentDetail) -> Self {
        Self {
            index: s.index,
            start_byte: s.start_byte,
            end_byte: s.end_byte,
            downloaded_bytes: s.downloaded_bytes,
        }
    }
}

impl From<model::CdnNodeInfo> for signals::CdnNodeDetail {
    fn from(n: model::CdnNodeInfo) -> Self {
        Self {
            ip: n.ip,
            origin: n.origin,
            bytes: n.bytes,
            ewma_bps: n.ewma_bps,
            active: n.active,
        }
    }
}

impl From<model::BtFileEntry> for signals::BtFileEntry {
    fn from(f: model::BtFileEntry) -> Self {
        Self {
            index: f.index,
            path: f.path,
            size: f.size,
        }
    }
}

impl From<model::HlsQualityOption> for signals::HlsQualityOption {
    fn from(o: model::HlsQualityOption) -> Self {
        Self {
            index: o.index,
            bandwidth: o.bandwidth,
            width: o.width,
            height: o.height,
        }
    }
}

impl From<model::ResolveVariantOption> for signals::ResolveVariantOption {
    fn from(o: model::ResolveVariantOption) -> Self {
        Self {
            index: o.index,
            label: o.label,
            container: o.container,
            bandwidth: o.bandwidth,
            width: o.width,
            height: o.height,
            total_bytes: o.total_bytes,
        }
    }
}
