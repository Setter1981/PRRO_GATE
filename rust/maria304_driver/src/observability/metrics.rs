//! Lightweight in-process metrics — per-session + per-FN counters.
//!
//! M8 ships only the data model; the Prometheus / HTTP exporter
//! lands in M10 admin.  Consumers read via [`SessionMetrics::snapshot`]
//! and can ship the values to whatever sink (Prometheus, logs, a
//! custom UI) they have in place.

use std::sync::atomic::{AtomicU64, Ordering};

/// Counters for a single session lifetime.
#[derive(Debug, Default)]
pub struct SessionMetrics {
    /// Frames received from the client (bytes decoded into a
    /// well-formed frame).
    pub inbound_frames: AtomicU64,
    /// Frames emitted to the client.
    pub outbound_frames: AtomicU64,
    /// Receipts closed successfully (COMP acked).
    pub receipts_acked: AtomicU64,
    /// Receipts cancelled (CANC).
    pub receipts_cancelled: AtomicU64,
    /// Bridge submissions that returned an error.
    pub bridge_errors: AtomicU64,
    /// Frames that failed CRC / length / start-byte checks.
    pub frame_errors: AtomicU64,
}

/// Snapshot — plain u64 values for export.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MetricsSnapshot {
    pub inbound_frames: u64,
    pub outbound_frames: u64,
    pub receipts_acked: u64,
    pub receipts_cancelled: u64,
    pub bridge_errors: u64,
    pub frame_errors: u64,
}

impl SessionMetrics {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_inbound_frame(&self) {
        self.inbound_frames.fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_outbound_frame(&self) {
        self.outbound_frames.fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_receipt_acked(&self) {
        self.receipts_acked.fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_receipt_cancelled(&self) {
        self.receipts_cancelled.fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_bridge_error(&self) {
        self.bridge_errors.fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_frame_error(&self) {
        self.frame_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Read all counters.  Values are consistent per-counter but not
    /// across counters — exporters that need cross-counter atomicity
    /// should sample twice and use the later value.
    #[must_use]
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            inbound_frames: self.inbound_frames.load(Ordering::Relaxed),
            outbound_frames: self.outbound_frames.load(Ordering::Relaxed),
            receipts_acked: self.receipts_acked.load(Ordering::Relaxed),
            receipts_cancelled: self.receipts_cancelled.load(Ordering::Relaxed),
            bridge_errors: self.bridge_errors.load(Ordering::Relaxed),
            frame_errors: self.frame_errors.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_snapshot_is_all_zero() {
        let m = SessionMetrics::new();
        assert_eq!(m.snapshot(), MetricsSnapshot::default());
    }

    #[test]
    fn record_methods_increment_counters_monotonically() {
        let m = SessionMetrics::new();
        for _ in 0..5 {
            m.record_inbound_frame();
        }
        for _ in 0..3 {
            m.record_outbound_frame();
        }
        m.record_receipt_acked();
        m.record_receipt_acked();
        m.record_receipt_cancelled();
        m.record_bridge_error();
        m.record_frame_error();
        m.record_frame_error();

        let s = m.snapshot();
        assert_eq!(s.inbound_frames, 5);
        assert_eq!(s.outbound_frames, 3);
        assert_eq!(s.receipts_acked, 2);
        assert_eq!(s.receipts_cancelled, 1);
        assert_eq!(s.bridge_errors, 1);
        assert_eq!(s.frame_errors, 2);
    }

    #[test]
    fn snapshot_is_send_sync_safe_for_concurrent_readers() {
        use std::sync::Arc;
        use std::thread;

        let m = Arc::new(SessionMetrics::new());
        let n_writers: u64 = 8;
        let per_writer: u64 = 1000;

        let handles: Vec<_> = (0..n_writers)
            .map(|_| {
                let m = Arc::clone(&m);
                thread::spawn(move || {
                    for _ in 0..per_writer {
                        m.record_inbound_frame();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(m.snapshot().inbound_frames, n_writers * per_writer);
    }

    #[test]
    fn snapshot_is_copy_and_can_be_compared() {
        // Snapshot derives Copy, Default, PartialEq so callers can
        // cheaply diff two readings for rate calculation.
        let m = SessionMetrics::new();
        let a = m.snapshot();
        let b = m.snapshot();
        assert_eq!(a, b);
    }
}
