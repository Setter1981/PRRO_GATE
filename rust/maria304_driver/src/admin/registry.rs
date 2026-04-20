//! Shared registry of all active FN listeners.
//!
//! The main process registers each `FnListener` here on startup so
//! the admin HTTP server can introspect.  Exclusion gate + metrics
//! are behind `Arc` — concurrent readers are always safe.

use std::sync::Arc;

use serde::Serialize;
use tokio::sync::RwLock;

use crate::listener::ConnectionGate;
use crate::observability::{MetricsSnapshot, SessionMetrics};

/// One entry per FN — reflects the configured listener plus live state.
#[derive(Debug, Clone)]
pub struct RegisteredFn {
    pub fiscal_number: String,
    pub bind: String,
    pub gate: Arc<ConnectionGate>,
    pub metrics: Arc<SessionMetrics>,
}

/// Admin-side JSON view of a single FN.
#[derive(Debug, Clone, Serialize)]
pub struct FnSnapshot {
    pub fiscal_number: String,
    pub bind: String,
    pub connection_active: bool,
    pub metrics: MetricsSummary,
}

/// Aggregate metrics summary across all FNs.
#[derive(Debug, Clone, Copy, Serialize, Default)]
pub struct MetricsSummary {
    pub inbound_frames: u64,
    pub outbound_frames: u64,
    pub receipts_acked: u64,
    pub receipts_cancelled: u64,
    pub bridge_errors: u64,
    pub frame_errors: u64,
}

impl From<MetricsSnapshot> for MetricsSummary {
    fn from(s: MetricsSnapshot) -> Self {
        Self {
            inbound_frames: s.inbound_frames,
            outbound_frames: s.outbound_frames,
            receipts_acked: s.receipts_acked,
            receipts_cancelled: s.receipts_cancelled,
            bridge_errors: s.bridge_errors,
            frame_errors: s.frame_errors,
        }
    }
}

/// Admin-side JSON view of an active session.  Populated by the
/// listener via `register_session` on accept and cleared on drop.
#[derive(Debug, Clone, Serialize)]
pub struct SessionSnapshot {
    pub fiscal_number: String,
    pub session_uuid: String,
    pub peer: String,
    pub cashier_id: Option<String>,
    pub receipt_open: bool,
}

/// Global registry.
#[derive(Debug, Default)]
pub struct Registry {
    fns: RwLock<Vec<RegisteredFn>>,
    sessions: RwLock<Vec<SessionSnapshot>>,
}

impl Registry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a listener.  Call once per FN at startup.
    pub async fn register_fn(&self, entry: RegisteredFn) {
        self.fns.write().await.push(entry);
    }

    /// Capture the current list of FNs as admin-friendly snapshots.
    pub async fn snapshot_fns(&self) -> Vec<FnSnapshot> {
        self.fns
            .read()
            .await
            .iter()
            .map(|e| FnSnapshot {
                fiscal_number: e.fiscal_number.clone(),
                bind: e.bind.clone(),
                connection_active: e.gate.is_active(),
                metrics: e.metrics.snapshot().into(),
            })
            .collect()
    }

    /// Aggregate every FN's metrics counters.
    pub async fn aggregate_metrics(&self) -> MetricsSummary {
        let mut total = MetricsSummary::default();
        for e in self.fns.read().await.iter() {
            let s = e.metrics.snapshot();
            total.inbound_frames += s.inbound_frames;
            total.outbound_frames += s.outbound_frames;
            total.receipts_acked += s.receipts_acked;
            total.receipts_cancelled += s.receipts_cancelled;
            total.bridge_errors += s.bridge_errors;
            total.frame_errors += s.frame_errors;
        }
        total
    }

    /// Add a session to the registry.  Session lifecycle matches
    /// `run_connection` — listener calls this on accept.
    pub async fn register_session(&self, s: SessionSnapshot) {
        self.sessions.write().await.push(s);
    }

    /// Remove the session with the given uuid.  No-op if absent.
    pub async fn drop_session(&self, session_uuid: &str) {
        self.sessions
            .write()
            .await
            .retain(|s| s.session_uuid != session_uuid);
    }

    /// List active sessions.
    pub async fn snapshot_sessions(&self) -> Vec<SessionSnapshot> {
        self.sessions.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_fn() -> RegisteredFn {
        RegisteredFn {
            fiscal_number: "FN-TEST".to_string(),
            bind: "127.0.0.1:9100".to_string(),
            gate: Arc::new(ConnectionGate::new()),
            metrics: Arc::new(SessionMetrics::new()),
        }
    }

    #[tokio::test]
    async fn empty_registry_returns_empty_snapshots() {
        let r = Registry::new();
        assert!(r.snapshot_fns().await.is_empty());
        assert!(r.snapshot_sessions().await.is_empty());
        assert_eq!(r.aggregate_metrics().await.inbound_frames, 0);
    }

    #[tokio::test]
    async fn registered_fn_surfaces_in_snapshot_with_live_gate_state() {
        let r = Registry::new();
        let entry = sample_fn();
        let gate = entry.gate.clone();
        r.register_fn(entry).await;

        let snap = r.snapshot_fns().await;
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].fiscal_number, "FN-TEST");
        assert!(!snap[0].connection_active);

        // Flip the gate — next snapshot reflects it.
        assert!(gate.try_acquire());
        let snap = r.snapshot_fns().await;
        assert!(snap[0].connection_active);
    }

    #[tokio::test]
    async fn aggregate_metrics_sums_every_fn_counter() {
        let r = Registry::new();
        let a = sample_fn();
        let b = sample_fn();
        // Increment counters on each FN independently.
        a.metrics.record_inbound_frame();
        a.metrics.record_receipts_acked_dummy(2);
        b.metrics.record_inbound_frame();
        b.metrics.record_bridge_error();

        r.register_fn(a).await;
        r.register_fn(b).await;

        let agg = r.aggregate_metrics().await;
        assert_eq!(agg.inbound_frames, 2);
        assert_eq!(agg.receipts_acked, 2);
        assert_eq!(agg.bridge_errors, 1);
    }

    #[tokio::test]
    async fn register_and_drop_session_roundtrip() {
        let r = Registry::new();
        r.register_session(SessionSnapshot {
            fiscal_number: "F".to_string(),
            session_uuid: "u".to_string(),
            peer: "127.0.0.1:1234".to_string(),
            cashier_id: None,
            receipt_open: false,
        })
        .await;
        assert_eq!(r.snapshot_sessions().await.len(), 1);
        r.drop_session("u").await;
        assert!(r.snapshot_sessions().await.is_empty());
    }

    // Convenience — the test above uses `record_receipts_acked_dummy`
    // which is a shim for the real API.  Wired here so SessionMetrics
    // doesn't need an oddball "×N" record method just for this test.
    trait RecordN {
        fn record_receipts_acked_dummy(&self, n: u64);
    }
    impl RecordN for SessionMetrics {
        fn record_receipts_acked_dummy(&self, n: u64) {
            for _ in 0..n {
                self.record_receipt_acked();
            }
        }
    }
}
