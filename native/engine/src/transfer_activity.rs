//! Transport-independent observations of in-flight body transfers.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// 0 is reserved for legacy snapshots with no source observation.
static NEXT_SAMPLE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Allocate at the observation site, never when delivering an older queued sample.
pub fn next_sample_sequence() -> u64 {
    NEXT_SAMPLE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

/// One file byte range; `active = None` means this protocol has no observation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TaskSegment {
    pub index: i32,
    pub start_byte: i64,
    pub end_byte: i64,
    pub downloaded_bytes: i64,
    pub active: Option<bool>,
}

/// A sampled task runtime, independent from the daemon wire DTO.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TaskRuntime {
    pub task_id: String,
    pub sampled_at_ms: i64,
    pub sample_sequence: u64,
    pub active_transfers: Option<u32>,
    pub connected_peers: Option<u32>,
    pub parallelism_limit: Option<u32>,
    pub total_bytes: i64,
    pub segments: Vec<TaskSegment>,
}

/// Count body reads, not worker slots, pending permits, or retry backoff.
/// Multiple workers may share a segment index while a split is being reconciled.
///
/// ```
/// use fluxdown_engine::transfer_activity::TransferTracker;
/// let tracker = TransferTracker::new();
/// let transfer = tracker.start(0);
/// assert_eq!(tracker.active(), 1);
/// drop(transfer);
/// assert_eq!(tracker.active(), 0);
/// ```
#[derive(Clone, Default)]
pub struct TransferTracker {
    inner: Arc<Mutex<HashMap<i32, u32>>>,
}

impl TransferTracker {
    /// Create a tracker with no body reads in flight.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enter a body read; retain the guard only while reading the transport body.
    pub fn start(&self, index: i32) -> TransferGuard {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        *inner.entry(index).or_default() += 1;
        TransferGuard {
            tracker: self.clone(),
            index,
        }
    }

    /// Return the number of body reads currently in flight.
    pub fn active(&self) -> u32 {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .copied()
            .sum()
    }
    /// Capture the count and active segment indexes under one lock.
    pub fn snapshot(&self) -> (u32, HashSet<i32>) {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        (
            inner.values().copied().sum(),
            inner.keys().copied().collect(),
        )
    }

    /// Report whether at least one body read currently owns this segment index.
    pub fn is_active(&self, index: i32) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(&index)
    }
}

/// Dropping on error, cancellation, retry, or ordinary completion ends the read.
pub struct TransferGuard {
    tracker: TransferTracker,
    index: i32,
}

impl Drop for TransferGuard {
    fn drop(&mut self) {
        let mut inner = self.tracker.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(n) = inner.get_mut(&self.index) {
            *n -= 1;
            if *n == 0 {
                inner.remove(&self.index);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::TransferTracker;

    #[test]
    fn overlapping_reads_same_range_and_retry_do_not_lose_each_other() {
        let tracker = TransferTracker::new();
        let first = tracker.start(4);
        let second = tracker.clone().start(4);
        let other = tracker.start(7);
        assert_eq!(tracker.active(), 3);
        drop(first);
        assert_eq!(tracker.active(), 2);
        assert!(tracker.is_active(4));
        drop(second);
        assert!(!tracker.is_active(4));
        drop(other);
        assert_eq!(tracker.active(), 0);
        let retry = tracker.start(4);
        assert_eq!(tracker.active(), 1);
        drop(retry);
        assert_eq!(tracker.active(), 0);
    }

    #[tokio::test]
    async fn cancellation_drops_guard_without_stale_activity() {
        let tracker = TransferTracker::new();
        let worker = tracker.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let _read = worker.start(3);
            let _ = tx.send(());
            std::future::pending::<()>().await;
        });
        rx.await.expect("worker began body read");
        assert_eq!(tracker.active(), 1);
        task.abort();
        let _ = task.await;
        assert_eq!(tracker.active(), 0);
        assert!(!tracker.is_active(3));
    }
}
