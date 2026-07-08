//! Dry-run bridge — accepts every submit, returns a synthetic ACK,
//! never touches the network.
//!
//! Operators flip the deployment mode to [`DeploymentMode::DryRun`]
//! during migration: 1C is pointed at the driver, wire traffic is
//! parsed and logged, but no canonical envelopes reach the Python
//! gateway and therefore nothing touches DPS or the archive.
//! Perfect for shaking out 1C-side integration bugs before the first
//! live receipt.

use std::sync::atomic::{AtomicU64, Ordering};

use super::{Bridge, BridgeError, CanonicalCommand, CanonicalResponse};

/// Deployment mode — selected from config at startup.
///
/// Three values per plan §12:
///  * [`DeploymentMode::Live`]   — production; Bridge talks to
///    the real Python gateway which submits to DPS.
///  * [`DeploymentMode::Shadow`] — Bridge still talks to Python, but
///    Python itself is in dry-run (no DPS, no archive).  Rust side
///    of shadow mode is identical to live — the mode flag travels
///    via `X-Maria-Shadow` header so Python can branch.
///  * [`DeploymentMode::DryRun`] — Rust never calls the bridge at
///    all.  [`DryRunBridge`] handles `COMP` locally with synthetic
///    `ACK`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeploymentMode {
    #[default]
    Live,
    Shadow,
    DryRun,
}

impl DeploymentMode {
    /// Parse from a config string — returns `None` on unknown value so
    /// startup validation can fail fast.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "live" => Some(Self::Live),
            "shadow" => Some(Self::Shadow),
            "dry-run" | "dry_run" | "dryrun" => Some(Self::DryRun),
            _ => None,
        }
    }
}

/// Bridge implementation for dry-run mode.
///
/// Every `submit` call:
///   * logs a `tracing::info!` event with the `command_type` and
///     `fiscal_number` so operators can audit what a live deployment
///     WOULD have sent.
///   * returns a synthetic `CanonicalResponse` with an auto-increment
///     `fiscal_id`.
///
/// No network I/O, no persistence — safe to run alongside a live
/// deployment without any risk of accidentally double-submitting.
#[derive(Debug, Default)]
pub struct DryRunBridge {
    next_fiscal_id: AtomicU64,
}

impl DryRunBridge {
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_fiscal_id: AtomicU64::new(1),
        }
    }

    /// How many times submit has been invoked since construction.
    #[must_use]
    pub fn call_count(&self) -> u64 {
        // Sequence starts at 1 and increments before use, so the next
        // ID is (calls + 1).  Fetch current and subtract.
        self.next_fiscal_id
            .load(Ordering::Relaxed)
            .saturating_sub(1)
    }
}

impl Bridge for DryRunBridge {
    fn submit(&self, command: &CanonicalCommand) -> Result<CanonicalResponse, BridgeError> {
        let n = self.next_fiscal_id.fetch_add(1, Ordering::Relaxed);
        tracing::info!(
            mode = "dry-run",
            fiscal_number = %command.fiscal_number,
            command_type = ?command.command_type,
            idempotency_key = %command.idempotency_key,
            "suppressed canonical submit (dry-run)",
        );
        let fiscal_id = format!("{n:010}");
        Ok(CanonicalResponse {
            ok: true,
            document_id: format!("dryrun-{fiscal_id}"),
            fiscal_id,
            fiscal_ts: "2026-04-20T00:00:00+00:00".to_string(),
            document_state: "DRY_RUN_ACK".to_string(),
            sale_total_kopecks: command.payload.totals.sale_kopecks,
            return_total_kopecks: command.payload.totals.return_kopecks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::dto::{CommandType, ReceiptPayload, Totals};

    fn sample(sale: u64) -> CanonicalCommand {
        CanonicalCommand {
            schema_version: "1.0".to_string(),
            fiscal_number: "F".to_string(),
            command_type: CommandType::Sell,
            idempotency_key: "k".to_string(),
            cashier_id: None,
            department: None,
            return_check_number: None,
            payload: ReceiptPayload {
                totals: Totals {
                    sale_kopecks: sale,
                    return_kopecks: 0,
                },
                ..Default::default()
            },
        }
    }

    #[test]
    fn deployment_mode_default_is_live() {
        assert_eq!(DeploymentMode::default(), DeploymentMode::Live);
    }

    #[test]
    fn deployment_mode_parse_accepts_case_and_separator_variants() {
        assert_eq!(DeploymentMode::parse("live"), Some(DeploymentMode::Live));
        assert_eq!(DeploymentMode::parse("LIVE"), Some(DeploymentMode::Live));
        assert_eq!(
            DeploymentMode::parse("shadow"),
            Some(DeploymentMode::Shadow)
        );
        assert_eq!(
            DeploymentMode::parse("dry-run"),
            Some(DeploymentMode::DryRun)
        );
        assert_eq!(
            DeploymentMode::parse("dry_run"),
            Some(DeploymentMode::DryRun)
        );
        assert_eq!(
            DeploymentMode::parse("DryRun"),
            Some(DeploymentMode::DryRun)
        );
    }

    #[test]
    fn deployment_mode_parse_rejects_nonsense() {
        assert_eq!(DeploymentMode::parse(""), None);
        assert_eq!(DeploymentMode::parse("production"), None);
        assert_eq!(DeploymentMode::parse("stub"), None);
    }

    #[test]
    fn dry_run_first_submit_returns_fiscal_id_0000000001() {
        let b = DryRunBridge::new();
        let r = b.submit(&sample(100)).unwrap();
        assert_eq!(r.fiscal_id, "0000000001");
        assert_eq!(r.document_state, "DRY_RUN_ACK");
    }

    #[test]
    fn dry_run_echoes_totals_from_envelope() {
        let b = DryRunBridge::new();
        let r = b.submit(&sample(42_000)).unwrap();
        assert_eq!(r.sale_total_kopecks, 42_000);
    }

    #[test]
    fn dry_run_fiscal_id_increments_per_call() {
        let b = DryRunBridge::new();
        let a = b.submit(&sample(0)).unwrap();
        let bb = b.submit(&sample(0)).unwrap();
        let c = b.submit(&sample(0)).unwrap();
        assert_eq!(a.fiscal_id, "0000000001");
        assert_eq!(bb.fiscal_id, "0000000002");
        assert_eq!(c.fiscal_id, "0000000003");
    }

    #[test]
    fn dry_run_call_count_tracks_submissions() {
        let b = DryRunBridge::new();
        assert_eq!(b.call_count(), 0);
        b.submit(&sample(0)).unwrap();
        b.submit(&sample(0)).unwrap();
        assert_eq!(b.call_count(), 2);
    }

    #[test]
    fn dry_run_bridge_is_send_sync_for_listener_sharing() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DryRunBridge>();
    }
}
