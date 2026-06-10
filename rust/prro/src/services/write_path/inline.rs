//! RS-3 A2.1b-core — the inline `fiscalize` orchestrator (SELL/RETURN only).
//!
//! Lands DORMANT: the production binding (`UnimplementedWritePath` →
//! `InlineWritePath`) is flipped in A2.4, so nothing here is reachable from a
//! live request yet. See `docs/superpowers/plans/2026-06-09-rs3-a2-1b-core-impl.md`.
//!
//! ## Online `Sent → ACK` confirm (Q1 = option b, operator-signed)
//!
//! `stage_send::run` reaches `Sent { server_fiscal_no, attempt_no }` but
//! discards the DPS send-response `data_sign`; `kvt2_advance::advance_to_ack`
//! REQUIRES `kvt1_raw_bytes` (the lastChk `data_sign`).  So after `Sent` the
//! inline ladder performs an inline lastChk by `server_fiscal_no` via
//! [`online_confirm`] to recover the evidence, then drives `advance_to_ack`.
//!
//! [`online_confirm`] is a thin runtime-neutral wrapper that REUSES the drain's
//! pure classifier (`classify_check_result`) so the KVT1/hold/drift routing is
//! NOT duplicated — but it yields a 3-shape [`InlineConfirmOutcome`] (NO
//! `BootError`, NO drain source-routing leaking into the inline path).

#![allow(dead_code)] // consumed by the A2.4 binding; wired up incrementally.

use sqlx::SqlitePool;
use tokio::sync::OwnedMutexGuard;

use crate::db::models::enums::DocState;
use crate::db::repositories::ingress_inbox::InboxRow;
use crate::runtime::ingress::canonical_builder::build_canonical;
use crate::runtime::ingress::seam::{FiscalError, FiscalOutcome};
use crate::services::offline_sync::kvt2_confirm::{
    classify_check_result, Kvt2ConfirmOutcome, Kvt2ConfirmSource,
};
use crate::services::write_path::dispatch::{dispatch_post_sign, PostSignRoute};
use crate::services::write_path::inline_map::{classify_send_outcome, SendDisposition};
use crate::services::write_path::kvt2_advance::{advance_to_ack, ConfirmError};
use crate::services::write_path::stage_acquire;
use crate::services::write_path::stage_send::{self, StageSendOutcome};
use crate::services::write_path::stage_sign::{self, SigningContext};
use crate::services::write_path::types::WorkerProcessResult;
use crate::transports::dps::channel::DpsChannel;
use crate::transports::dps::dto::CheckSignBlob;

/// Outcome of the inline online `Sent → ACK` confirm step ([`online_confirm`]).
/// Runtime-neutral 3-shape projection of the drain's `Kvt2ConfirmOutcome`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineConfirmOutcome {
    /// lastChk matched with non-empty `data_sign` — the KVT1 evidence the
    /// caller feeds into `advance_to_ack(kvt1_raw_bytes = .0)`.
    Acked(Vec<u8>),
    /// Transient / no-KVT1-evidence (DPS transport/server/auth/decode error,
    /// or matched id with EMPTY `data_sign`).  The caller leaves the doc at
    /// `Sent` → `Ok(FiscalOutcome{document_state: Sent})` → 202 IN_PROGRESS;
    /// drain/B1 completes the ACK later.  NEVER a terminal failure.
    Hold,
    /// Structural drift — `NotFound` / `ServerFiscalIdMismatch` from a
    /// Sent-fresh online send (the server does not recognise the id we just
    /// got from it).  The caller terminalises the inbox + returns
    /// `FiscalError::Internal` (500).
    Drift,
}

/// Inline online confirm: lastChk by `server_fiscal_no` → recover the KVT1
/// `data_sign` evidence.  Reuses [`classify_check_result`] (`SentFresh`
/// context) so the KVT1/hold/drift classification is single-sourced.
///
/// **I1**: the only IO is `by_server_fiscal_no` (a `last_chk` wire call), which
/// `assert_not_in_with_immediate`s — it MUST be called OUTSIDE every
/// `with_immediate` envelope.  No DB writes here.
pub async fn online_confirm(
    dps: &dyn DpsChannel,
    fn_sign: &CheckSignBlob,
    server_fiscal_no: &str,
) -> InlineConfirmOutcome {
    let result = dps.by_server_fiscal_no(fn_sign, server_fiscal_no).await;
    match classify_check_result(result, Kvt2ConfirmSource::SentFresh, None) {
        Kvt2ConfirmOutcome::Acked { kvt1_raw_bytes, .. } => {
            InlineConfirmOutcome::Acked(kvt1_raw_bytes)
        }
        Kvt2ConfirmOutcome::Hold { .. } => InlineConfirmOutcome::Hold,
        Kvt2ConfirmOutcome::StructuralDrift { .. } => InlineConfirmOutcome::Drift,
        Kvt2ConfirmOutcome::SentNotFoundDowngrade { .. } => {
            // Structurally unreachable: `classify_check_result` only emits
            // `SentNotFoundDowngrade` for the `SentReplay` source (the
            // safe-redrive path); we always pass `SentFresh`.  If this ever
            // fires, the classifier routing regressed.
            unreachable!(
                "online_confirm: SentNotFoundDowngrade is SentReplay-exclusive; \
                 SentFresh cannot produce it (classify_check_result routing breach)"
            )
        }
    }
}

/// Inline `fiscalize` orchestrator (A2.1b-core, SELL/RETURN only) — chains
/// `build_canonical → stage_acquire → stage_sign → dispatch → (Online) send →
/// online_confirm → advance_to_ack → finalize` into a `FiscalOutcome`.
///
/// **DORMANT**: no production caller yet (the binding flip is A2.4). `fn_gate`
/// is the A4 per-FN gate proof — the A2.4 binding MUST hold
/// `App::acquire_fn_gate(&row.fiscal_number)` across this call (invariant #2 at
/// the runtime level; the DB lease CAS in `stage_acquire` is the durable
/// backstop). `run` opens NO `with_immediate` itself; every envelope is owned
/// by a reused stage, and the only IO (crypto sign, DPS send, inline lastChk)
/// sits strictly BETWEEN envelopes (invariant #1).
///
/// Non-happy arms are wired incrementally (see the TDD increments in the impl
/// spec); each lands with its own pinning test.
#[allow(clippy::too_many_arguments)] // individual deps keep `run` unit-testable
                                     // without `&App` (the binding bundles them).
pub async fn run(
    pool: &SqlitePool,
    pool_secure: &SqlitePool,
    dps: &dyn DpsChannel,
    sign_ctx: &SigningContext,
    fn_sign: &CheckSignBlob,
    _fn_gate: &OwnedMutexGuard<()>,
    row: &InboxRow,
) -> Result<FiscalOutcome, FiscalError> {
    let request_id = row.request_id;

    // A2.1b-core is SELL/RETURN only. Z_REPORT/SHIFT_CLOSE (Z-class) and
    // SHIFT_OPEN are fail-closed (incr.5), handled before build/acquire.
    // TODO(incr.5): is_z_class → ZSurfaceNotReady (lease+terminalise);
    //               SHIFT_OPEN → ShiftGuardRefused{SHIFT_OPEN_NOT_SUPPORTED}.

    let command = match build_canonical(row) {
        Ok(c) => c,
        Err(_reject) => {
            todo!("A2.1b-core incr.5: BuildReject arm (terminalise + map_build_reject)")
        }
    };
    // build_canonical validated driver_id is present + well-formed.
    let driver_id = row
        .driver_id
        .as_deref()
        .expect("build_canonical guarantees driver_id present");

    let acq = match stage_acquire::run(pool, pool_secure, driver_id, request_id, command).await {
        Ok(r) => r,
        Err(_e) => todo!("A2.1b-core incr.5: stage_acquire anyhow-error arm"),
    };
    let ctx = match acq {
        WorkerProcessResult::Proceed(ctx) | WorkerProcessResult::Resumed(ctx) => ctx,
        WorkerProcessResult::Noop => todo!("A2.1b-core incr.3: Noop replay-resolve"),
        WorkerProcessResult::Rejected { .. } => todo!("A2.1b-core incr.5: Rejected arm"),
    };
    let doc_id = ctx.document.document_id;
    let fiscal_number = ctx.document.fiscal_number.clone();

    match stage_sign::run(pool, sign_ctx, ctx).await {
        Ok(_signing) => {}
        Err(_e) => todo!("A2.1b-core incr.5: SignError arm"),
    }

    let route = match dispatch_post_sign(pool, doc_id, &fiscal_number).await {
        Ok(r) => r,
        Err(_e) => todo!("A2.1b-core incr.5: dispatch anyhow-error arm"),
    };
    match route {
        PostSignRoute::Refused(_reason) => {
            todo!("A2.1b-core incr.5: dispatcher Refused arm")
        }
        PostSignRoute::Offline { .. } => todo!("A2.1b-core incr.4: offline-ack arm"),
        PostSignRoute::Online { .. } => {
            let send = match stage_send::run(pool, dps, doc_id, Some(sign_ctx)).await {
                Ok(o) => o,
                Err(_e) => todo!("A2.1b-core incr.5: StageSendError arm"),
            };
            // GOTCHA (arch-planner): `classify_send_outcome` drops `attempt_no`
            // on the Proceed arm — capture it from `Sent` BEFORE classifying,
            // since `advance_to_ack`'s audit payload needs it.
            let sent_attempt_no = match &send {
                StageSendOutcome::Sent { attempt_no, .. } => Some(*attempt_no),
                _ => None,
            };
            match classify_send_outcome(send, request_id) {
                SendDisposition::Reject(_fe) => todo!("A2.1b-core incr.5: send Reject arm"),
                SendDisposition::InProgress => Ok(FiscalOutcome {
                    // Transient wire failure: the doc is persisted at
                    // `ErrorRetryable`; the ledger re-drives it via drain/B1.
                    // 202 IN_PROGRESS — NOT a terminal failure. No DPS id yet.
                    document_id: doc_id,
                    fiscal_id: None,
                    fiscal_ts: None,
                    document_state: DocState::ErrorRetryable,
                    report_xml: None,
                }),
                SendDisposition::ResolveReplay { .. } => {
                    todo!("A2.1b-core incr.3: send ResolveReplay arm")
                }
                SendDisposition::Proceed { server_fiscal_no } => {
                    let attempt_no = sent_attempt_no
                        .expect("Sent disposition implies a captured Sent attempt_no");
                    // Online Sent→ACK confirm (Q1 = b): inline lastChk recovers
                    // the KVT1 evidence stage_send discarded, then advance_to_ack.
                    match online_confirm(dps, fn_sign, &server_fiscal_no).await {
                        InlineConfirmOutcome::Acked(kvt1_raw_bytes) => {
                            match advance_to_ack(
                                pool,
                                doc_id,
                                kvt1_raw_bytes,
                                &server_fiscal_no,
                                DocState::Sent,
                                Some(i64::from(attempt_no)),
                            )
                            .await
                            {
                                Ok(()) => Ok(FiscalOutcome {
                                    document_id: doc_id,
                                    fiscal_id: Some(server_fiscal_no),
                                    fiscal_ts: None,
                                    document_state: DocState::Ack,
                                    report_xml: None,
                                }),
                                Err(ConfirmError::StructuralDrift { .. }) => {
                                    todo!("A2.1b-core incr.5: advance StructuralDrift arm")
                                }
                                Err(ConfirmError::Database { .. })
                                | Err(ConfirmError::Infrastructure { .. }) => {
                                    todo!("A2.1b-core incr.3: advance Database/Infra → ledger-resolve")
                                }
                            }
                        }
                        InlineConfirmOutcome::Hold => Ok(FiscalOutcome {
                            // Inline lastChk had no KVT1 evidence (transient /
                            // empty data_sign): leave the doc at `Sent` → 202
                            // IN_PROGRESS; drain/B1 completes the KVT2 confirm.
                            // NOT a terminal failure, NOT a fake ACK. The DPS
                            // id IS known (stamped by stage_send) — informational.
                            document_id: doc_id,
                            fiscal_id: Some(server_fiscal_no),
                            fiscal_ts: None,
                            document_state: DocState::Sent,
                            report_xml: None,
                        }),
                        InlineConfirmOutcome::Drift => {
                            todo!("A2.1b-core incr.5: confirm Drift arm")
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transports::dps::dto::{CheckAck, CheckEnvelope, RroInfo, StatusSnapshot};
    use crate::transports::dps::error::DpsError;
    use async_trait::async_trait;
    use std::sync::Mutex;

    const FN_SIGN: &[u8] = &[0xAB, 0xCD];
    const SERVER_FISCAL_NO: &str = "DPS-FN-ONLINE-1";

    /// Minimal `DpsChannel` stub: a single scripted `last_chk` reply (consumed
    /// once).  `online_confirm` → `by_server_fiscal_no` (default trait method)
    /// → `last_chk`.  Other RPCs are unreachable for this seam.
    struct StubLastChk(Mutex<Option<Result<CheckAck, DpsError>>>);

    impl StubLastChk {
        fn new(reply: Result<CheckAck, DpsError>) -> Self {
            Self(Mutex::new(Some(reply)))
        }
    }

    #[async_trait]
    impl DpsChannel for StubLastChk {
        async fn send_chk(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
            unreachable!("online_confirm never sends");
        }
        async fn last_chk(&self, _: &CheckSignBlob) -> Result<CheckAck, DpsError> {
            self.0
                .lock()
                .unwrap()
                .take()
                .expect("StubLastChk: last_chk called more than once")
        }
        async fn ping(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
            unreachable!("stub: ping not exercised");
        }
        async fn status_rro(&self, _: &CheckSignBlob) -> Result<StatusSnapshot, DpsError> {
            unreachable!("stub: status_rro not exercised");
        }
        async fn info_rro(&self, _: &CheckSignBlob) -> Result<RroInfo, DpsError> {
            unreachable!("stub: info_rro not exercised");
        }
    }

    fn ack(id: &str, data_sign: Vec<u8>) -> CheckAck {
        CheckAck {
            id: id.to_string(),
            id_sign: vec![],
            data_sign,
        }
    }

    fn fn_sign() -> CheckSignBlob {
        CheckSignBlob(FN_SIGN.to_vec())
    }

    /// Match + non-empty data_sign → Acked carrying the exact evidence bytes
    /// (which the caller feeds into advance_to_ack as kvt1_raw_bytes).
    #[tokio::test]
    async fn acked_returns_data_sign_evidence() {
        let dps = StubLastChk::new(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF])));
        let outcome = online_confirm(&dps, &fn_sign(), SERVER_FISCAL_NO).await;
        assert_eq!(
            outcome,
            InlineConfirmOutcome::Acked(vec![0xDE, 0xAD, 0xBE, 0xEF])
        );
    }

    /// Match but EMPTY data_sign → Hold (no KVT1 evidence → 202, NOT a fake ACK).
    #[tokio::test]
    async fn empty_data_sign_returns_hold() {
        let dps = StubLastChk::new(Ok(ack(SERVER_FISCAL_NO, vec![])));
        let outcome = online_confirm(&dps, &fn_sign(), SERVER_FISCAL_NO).await;
        assert_eq!(outcome, InlineConfirmOutcome::Hold);
    }

    /// Transport error → Hold (transient → 202, NOT terminal 500).
    #[tokio::test]
    async fn transport_error_returns_hold() {
        let dps = StubLastChk::new(Err(DpsError::Transport("conn reset".into())));
        let outcome = online_confirm(&dps, &fn_sign(), SERVER_FISCAL_NO).await;
        assert_eq!(outcome, InlineConfirmOutcome::Hold);
    }

    /// Empty id → NotFound → Drift (server does not recognise the id it just
    /// returned to us → structural breach → caller maps to Internal/500).
    #[tokio::test]
    async fn not_found_returns_drift() {
        let dps = StubLastChk::new(Ok(ack("", vec![0x01])));
        let outcome = online_confirm(&dps, &fn_sign(), SERVER_FISCAL_NO).await;
        assert_eq!(outcome, InlineConfirmOutcome::Drift);
    }

    /// Mismatched id → ServerFiscalIdMismatch → Drift.
    #[tokio::test]
    async fn id_mismatch_returns_drift() {
        let dps = StubLastChk::new(Ok(ack("DPS-FN-OTHER", vec![0x01])));
        let outcome = online_confirm(&dps, &fn_sign(), SERVER_FISCAL_NO).await;
        assert_eq!(outcome, InlineConfirmOutcome::Drift);
    }
}
