//! W12 — In-drain KVT2 confirmation via canonical DPS evidence.
//!
//! See: `docs/superpowers/plans/2026-05-22-m3b-w12-kvt2-confirm.md`.
//!
//! ## Commit 1 surface (this file)
//!
//! - Typed enums: [`Kvt2ConfirmSource`], [`Kvt2ConfirmOutcome`],
//!   [`Kvt2ConfirmHoldReason`], [`Kvt2ConfirmStructuralReason`].
//! - Pure evidence-routing function [`classify_check_result`] —
//!   maps `Result<CheckAck, DpsError>` from
//!   [`crate::transports::dps::channel::DpsChannel::by_server_fiscal_no`]
//!   into a [`Kvt2ConfirmOutcome`] variant per plan §"Source-context
//!   routing matrix".  No DB writes.  No DPS calls.
//! - Async helper [`evaluate_lastchk`] — orchestrates the canonical
//!   typed lookup + classification.  Calls
//!   `dps.by_server_fiscal_no(fn_sign, expected_server_fiscal_no)`
//!   OUTSIDE any `with_immediate` per I1.  No DB writes.
//!
//! ## Deferred to Commits 4 / 5 / 5b
//!
//! - Full `confirm_drain_doc(pool, dps, doc: &DocumentRow,
//!   expected_server_fiscal_no, fn_sign, source)` helper-heavy
//!   ownership (envelope commits per source × outcome) — plan
//!   §"Helper vs caller envelope ownership".
//! - Drain dispatcher rewires (`process_via_stage_send`,
//!   `process_via_w12_only`, `process_via_lastchk_replay`).
//! - `DrainSummary` triple-counter projection split.
//! - `DocVerdict::HoldFnDrain` + `HoldFnDrainProjection`.
//! - Cohort widening to include `DocState::Kvt2`.
//!
//! ## Routing matrix anchor (plan §Source-context routing matrix)
//!
//! Helper surface: `dps.by_server_fiscal_no(fn_sign, expected_id)`
//! per `channel.rs:53-69`.  Rows correspond to `Result<CheckAck,
//! DpsError>` variants the canonical helper produces.
//!
//! | Evidence outcome           | SentFresh       | SentReplay              | Kvt1Reentry     |
//! |---                         |---              |---                      |---              |
//! | Ok(ack) + non-empty signed | Acked           | Acked                   | Acked           |
//! | Ok(ack) + empty data_sign  | Hold            | Hold                    | Hold            |
//! | Err(NotFound)              | StructuralDrift | **SentNotFoundDowngrade** | StructuralDrift |
//! | Err(ServerFiscalIdMismatch)| StructuralDrift | StructuralDrift         | StructuralDrift |
//! | Err(Transport)             | Hold            | Hold                    | Hold            |
//! | Err(Server)                | Hold            | Hold                    | Hold            |
//! | Err(Authorization)         | Hold            | Hold                    | Hold            |
//! | Err(Decode)                | Hold            | Hold                    | Hold            |

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::app::BootError;
use crate::db::models::enums::{DocState, Severity};
use crate::db::models::ids::DocumentId;
use crate::db::repositories::fiscal_documents::TransitionOutcome;
use crate::db::repositories::{audit_log, document_files, fiscal_documents};
use crate::db::tx::with_immediate;
use crate::services::offline_sync::backlog_drain::AUDIT_ENTITY_DOC;
use crate::services::write_path::stage_finalize;
use crate::services::write_path::types::hex_encode_lower;
use crate::transports::dps::channel::DpsChannel;
use crate::transports::dps::dto::{CheckAck, CheckSignBlob};
use crate::transports::dps::error::DpsError;

/// Source-context for [`evaluate_lastchk`] invocation.  Identical
/// lastChk evidence outcomes route to context-specific verdicts per
/// the source-context routing matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kvt2ConfirmSource {
    /// Caller = `process_via_stage_send`, immediately after
    /// `StageSendOutcome::Sent`.  Doc just stamped server_fiscal_no
    /// this tick; caller passes `&outcome.server_fiscal_no` (NOT the
    /// pre-stage_send cohort `doc.server_fiscal_no`) per MED-PR70-R11-01.
    SentFresh,
    /// Caller = `process_via_lastchk_replay`, on persisted `Sent`
    /// cohort entry (crash recovery).  Caller passes
    /// `doc.server_fiscal_no.as_deref().ok_or(ServerFiscalNoMissing)?`.
    SentReplay,
    /// Caller = `process_via_w12_only`, on persisted `Kvt1` cohort
    /// entry (prior-tick Held).  Caller passes
    /// `doc.server_fiscal_no.as_deref().ok_or(ServerFiscalNoMissing)?`.
    Kvt1Reentry,
}

/// Helper outcome variants.  Caller projects to `DocVerdict` per plan
/// §"Helper vs caller envelope ownership".
///
/// `sent_replay_trace_attempt_no` is `Some(_)` only when source ==
/// [`Kvt2ConfirmSource::SentReplay`] — threads the Envelope 1c-pre
/// allocated trace row through to the post-outcome completion envelope.
/// Sent-fresh and Kvt1 re-entry contexts always carry `None` because
/// they do not allocate a W12-owned trace row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kvt2ConfirmOutcome {
    /// lastChk evidence matched (canonical id-match by
    /// `by_server_fiscal_no` + non-empty `data_sign`).  Caller invokes
    /// Envelope 1a / 1a-replay / 1b chain per source then Envelope 2
    /// (`stage_finalize::run`) for `Kvt2 → Ack`.  `kvt1_raw_bytes` is
    /// the persisted-byte-for-byte KVT1_RAW evidence (HIGH-C5-2).
    Acked {
        kvt1_raw_bytes: Vec<u8>,
        sent_replay_trace_attempt_no: Option<i64>,
    },
    /// Evidence-failure class.  Doc state UNCHANGED per W0b §97-102.
    /// Caller projects to `DocVerdict::HoldFnDrain { projection }`
    /// where projection per plan matrix: SentFresh / SentReplay →
    /// `HeldAtSent`; Kvt1Reentry → `HeldAtKvt1`.  Drain stops at this
    /// doc for this tick.
    Hold {
        reason: Kvt2ConfirmHoldReason,
        sent_replay_trace_attempt_no: Option<i64>,
    },
    /// Structural-invariant breach.  Caller propagates
    /// `BootError::Internal` to halt entire FN drain.  NOT per-doc
    /// Manual CAS.
    StructuralDrift {
        reason: Kvt2ConfirmStructuralReason,
        sent_replay_trace_attempt_no: Option<i64>,
    },
    /// HIGH-PR70-R4-01 safe-redrive (SentReplay arm exclusively).
    /// DPS has zero history of `server_fiscal_no`; recovery is resend
    /// via Pattern B, not poll forever.  Caller commits Envelope 1c-post
    /// (atomic trace.complete + Sent→ER + audit) then projects to
    /// `DocVerdict::HoldFnDrain { projection: ErRedriveQueued }`.
    /// Next tick: doc enters ER cohort → W9b ER class guard
    /// bounded-redrive via `stage_send::run`.
    SentNotFoundDowngrade { trace_attempt_no: i64 },
}

/// Hold-class reasons aligned with the actual `DpsChannel::last_chk
/// -> Result<CheckAck, DpsError>` surface per MED-PR70-R3-02.
/// `LastChkStatusNotOk` is **not** a variant: non-OK statuses are
/// decoded into typed `DpsError::*` upstream by the gRPC layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kvt2ConfirmHoldReason {
    /// `DpsError::Transport(_)` — network/transport blip.
    DpsTransport(String),
    /// `DpsError::Server { code, message }` — non-OK server status.
    DpsServer(String),
    /// `DpsError::Authorization { code, kind, message }` —
    /// operator-actionable at lastChk time; collapsed to Hold per
    /// W0b §97-102.
    DpsAuthorization(String),
    /// `DpsError::Decode(_)` — malformed lastChk response.
    DpsDecode(String),
    /// `Ok(CheckAck)` with id-match by helper but empty `data_sign`
    /// — can be a transient DPS-side bug (rare, can resolve on retry).
    LastChkDataSignEmpty,
}

/// Structural-drift reasons — system-level fail-loud as
/// `BootError::Internal`.  NOT per-doc Manual escalation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kvt2ConfirmStructuralReason {
    /// stage_send 4-b invariant breach: persisted Sent state without
    /// `server_fiscal_no`.  Surfaces when SentReplay / Kvt1Reentry
    /// caller cannot source the expected id from `doc.server_fiscal_no`
    /// (MED-PR70-R11-01 fail-loud BEFORE DPS call).
    ServerFiscalNoMissing,
    /// CAS produced non-Applied with state diverged from expected
    /// (concurrent writer past App reconcile mutex).
    CasMissOnAdvance {
        from: DocState,
        to: DocState,
        observed: DocState,
    },
    /// `DpsError::ServerFiscalIdMismatch` — non-empty differing
    /// `ack.id`.  Uniform structural drift across all 3 contexts.
    LastChkIdMismatch { observed: String, expected: String },
    /// `DpsError::NotFound` from a context with no safe-redrive
    /// interpretation (Sent-fresh or Kvt1 re-entry).  NotFound from
    /// these contexts indicates state-machine drift, not absent-history.
    NotFoundOutsideSentReplay { source: Kvt2ConfirmSource },
}

/// Pure evidence-routing function.  Maps the
/// `dps.by_server_fiscal_no(fn_sign, expected_id)` result + source
/// context into a [`Kvt2ConfirmOutcome`] variant per plan
/// §"Source-context routing matrix".
///
/// **No DB writes.  No DPS calls.  No envelope work.**
///
/// `sent_replay_trace_attempt_no` MUST be `Some(_)` when source ==
/// [`Kvt2ConfirmSource::SentReplay`]; the value is threaded through
/// to the outcome so the SentReplay post-outcome envelope (1a-replay
/// / 1c-post / 1c-hold / 1c-drift) can complete the exact trace row
/// allocated in Envelope 1c-pre.  Panics if SentReplay + NotFound
/// arrives with `None` — that violates the MED-PR70-R5-02 +
/// MED-PR70-R6-01 lifecycle contract.
pub fn classify_check_result(
    result: Result<CheckAck, DpsError>,
    source: Kvt2ConfirmSource,
    sent_replay_trace_attempt_no: Option<i64>,
) -> Kvt2ConfirmOutcome {
    match result {
        Ok(ack) => {
            // `by_server_fiscal_no` has already verified `ack.id ==
            // expected_id` per `channel.rs:60-69`.  We only need to
            // check `data_sign` non-empty for W0b §99 conformance.
            if ack.data_sign.is_empty() {
                Kvt2ConfirmOutcome::Hold {
                    reason: Kvt2ConfirmHoldReason::LastChkDataSignEmpty,
                    sent_replay_trace_attempt_no,
                }
            } else {
                Kvt2ConfirmOutcome::Acked {
                    kvt1_raw_bytes: ack.data_sign,
                    sent_replay_trace_attempt_no,
                }
            }
        }
        Err(DpsError::NotFound) => match source {
            Kvt2ConfirmSource::SentReplay => {
                let trace_attempt_no = sent_replay_trace_attempt_no.expect(
                    "SentReplay context requires pre-allocated trace attempt_no \
                     per MED-PR70-R5-02 + MED-PR70-R6-01 lifecycle contract",
                );
                Kvt2ConfirmOutcome::SentNotFoundDowngrade { trace_attempt_no }
            }
            Kvt2ConfirmSource::SentFresh | Kvt2ConfirmSource::Kvt1Reentry => {
                Kvt2ConfirmOutcome::StructuralDrift {
                    reason: Kvt2ConfirmStructuralReason::NotFoundOutsideSentReplay { source },
                    sent_replay_trace_attempt_no,
                }
            }
        },
        Err(DpsError::ServerFiscalIdMismatch {
            expected_id,
            actual_id,
        }) => Kvt2ConfirmOutcome::StructuralDrift {
            reason: Kvt2ConfirmStructuralReason::LastChkIdMismatch {
                observed: actual_id,
                expected: expected_id,
            },
            sent_replay_trace_attempt_no,
        },
        Err(DpsError::Transport(msg)) => Kvt2ConfirmOutcome::Hold {
            reason: Kvt2ConfirmHoldReason::DpsTransport(msg),
            sent_replay_trace_attempt_no,
        },
        Err(DpsError::Server { code, message }) => Kvt2ConfirmOutcome::Hold {
            reason: Kvt2ConfirmHoldReason::DpsServer(format!("status={code}: {message}")),
            sent_replay_trace_attempt_no,
        },
        Err(DpsError::Authorization {
            code,
            kind,
            message,
        }) => Kvt2ConfirmOutcome::Hold {
            reason: Kvt2ConfirmHoldReason::DpsAuthorization(format!(
                "{kind:?}(code={code}): {message}"
            )),
            sent_replay_trace_attempt_no,
        },
        Err(DpsError::Decode(msg)) => Kvt2ConfirmOutcome::Hold {
            reason: Kvt2ConfirmHoldReason::DpsDecode(msg),
            sent_replay_trace_attempt_no,
        },
        Err(other) => {
            // Defensive catch-all for `DpsError::QueryNotSupported` /
            // `DpsError::Internal` (and any future variant).  Lands as
            // Hold(DpsServer) with the Debug-formatted detail so
            // operator can triage without losing the typed signal.
            // Promoting any of these to structural drift is a separate
            // operator decision (out of W12 scope).
            Kvt2ConfirmOutcome::Hold {
                reason: Kvt2ConfirmHoldReason::DpsServer(format!("{other:?}")),
                sent_replay_trace_attempt_no,
            }
        }
    }
}

/// Async evidence-routing orchestrator.  Calls
/// `dps.by_server_fiscal_no(fn_sign, expected_server_fiscal_no)` —
/// the canonical typed lookup surface per `channel.rs:53-69` — and
/// classifies the result into a [`Kvt2ConfirmOutcome`] variant.
///
/// **No DB writes.  No envelope work.**  The DPS call sits OUTSIDE
/// any `with_immediate` per I1 (the underlying helper asserts this
/// via `assert_not_in_with_immediate` at `channel.rs:58`).
///
/// For [`Kvt2ConfirmSource::SentReplay`], caller MUST have allocated
/// a `transport_trace` recovery row before invoking this function
/// and pass its `attempt_no` in `sent_replay_trace_attempt_no`.
///
/// Full helper `confirm_drain_doc` (Commits 4 / 5 / 5b) wraps this
/// with the source-specific envelope chain.
pub async fn evaluate_lastchk(
    dps: &dyn DpsChannel,
    fn_sign: &CheckSignBlob,
    expected_server_fiscal_no: &str,
    source: Kvt2ConfirmSource,
    sent_replay_trace_attempt_no: Option<i64>,
) -> Kvt2ConfirmOutcome {
    let result = dps
        .by_server_fiscal_no(fn_sign, expected_server_fiscal_no)
        .await;
    classify_check_result(result, source, sent_replay_trace_attempt_no)
}

// ─── M3b W12 Commit 4 — confirm_drain_doc + Envelope 1a ─────────────

/// Drain-projected outcome of [`confirm_drain_doc`].
///
/// **Commit 4 surface** wires only the `Advanced` variant (SentFresh
/// happy path).  Commits 5 / 5b / 6 will extend this enum with `Hold`,
/// `SentNotFoundDowngrade`, and source-specific projections per plan
/// §"Helper vs caller envelope ownership".  Pre-Commit-6 callers
/// observe non-Acked outcomes as [`BootError::Internal`] (defensive
/// fail-loud until projection wiring lands).
///
/// **M3b W12 Commit 4a status (2026-05-22)**: foundation-only landing
/// — helper has NO production consumer yet (4a = library surface +
/// `#[cfg(test)]` proof only).  Commit 4b will wire the consumer at
/// `process_via_stage_send` and rebuild the pre-W12 stub-locking tests
/// against the new ACK-era acceptance shape.  Until 4b lands, `dead_
/// code` is expected — `#[allow(dead_code)]` on this enum + on
/// `confirm_drain_doc` + on the Envelope 1a writer is the explicit
/// "not-yet-wired" marker.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmDrainOutcome {
    /// Envelope 1a (Kvt1Raw persist + Sent→Kvt1 CAS + Kvt1→Kvt2 CAS +
    /// `OFFLINE_DRAIN_KVT2_ADVANCED` audit) committed atomically;
    /// Envelope 2 (`stage_finalize::run` Kvt2→Ack) committed.  Doc is
    /// now in terminal `Ack` state.  Caller updates `DrainSummary`
    /// with the Acked outcome.
    Advanced,
}

/// W12 high-level helper — orchestrates the full Sent-source W12
/// confirmation chain per plan §410 (Commit 4 wiring scope).
///
/// **Source-context support matrix** (4a = library-wired; production
/// consumer lands in 4b):
/// - [`Kvt2ConfirmSource::SentFresh`] → **library-wired** (this commit;
///   Commit 4b will replace `apply_w12_confirmation(Sent, ...)` in
///   `process_via_stage_send` with the call site).
///   Caller = `process_via_stage_send` after `StageSendOutcome::Sent`.
///   `expected_server_fiscal_no` MUST be sourced from the
///   `StageSendOutcome::Sent` variant (`&outcome.server_fiscal_no`),
///   NOT from the pre-stage_send cohort snapshot `doc.server_fiscal_no`
///   (which is `None` at cohort SELECT time per stage_send 4-b
///   invariant — MED-PR70-R11-01 handoff).
/// - [`Kvt2ConfirmSource::Kvt1Reentry`] → deferred to Commit 5
///   (`process_via_w12_only` rewrite, Envelope 1b chain).
/// - [`Kvt2ConfirmSource::SentReplay`] → deferred to Commit 5b
///   (`process_via_lastchk_replay` rewrite, Envelope 1c-pre / 1a-replay
///   / 1c-post / 1c-hold / 1c-drift chain).
///
/// **SentFresh happy path** (Commit 4):
///
/// 1. Call `evaluate_lastchk(dps, fn_sign, expected_id, SentFresh,
///    sent_replay_trace_attempt_no=None)` — DPS call sits OUTSIDE
///    `with_immediate` per I1.
/// 2. On `Kvt2ConfirmOutcome::Acked { kvt1_raw_bytes, .. }` commit
///    Envelope 1a atomically in ONE `with_immediate`: (a)
///    `document_files::replace_tx(Kvt1Raw)` persists `ack.data_sign`
///    byte-for-byte (HIGH-C5-2 forensic contract); (b) CAS
///    `Sent → Kvt1` (must produce `TransitionOutcome::Applied`; else
///    structural drift); (c) CAS `Kvt1 → Kvt2` (must produce Applied;
///    else structural drift); (d) `OFFLINE_DRAIN_KVT2_ADVANCED` audit
///    append.
/// 3. Then sequentially run Envelope 2: `stage_finalize::run(pool,
///    doc_id)` converges `Kvt2 → Ack` via M3a's own 5-write atomic
///    envelope.  Returned `Acked`/`AlreadyAcked` → `Advanced`; other
///    outcomes surface as `BootError`.  Returns
///    `Ok(ConfirmDrainOutcome::Advanced)`.
///
/// **Non-Acked outcomes (SentFresh)**:
/// - [`Kvt2ConfirmOutcome::StructuralDrift`] →
///   [`BootError::Internal`] per plan §410 (NotFound/Mismatch from
///   SentFresh = state-machine drift, NOT safe-redrive).
/// - [`Kvt2ConfirmOutcome::Hold`] → [`BootError::Internal`]
///   ("Commit 6 not yet wired" — Commit 6 will project to
///   `DocVerdict::HoldFnDrain { projection: HeldAtSent }`).
/// - [`Kvt2ConfirmOutcome::SentNotFoundDowngrade`] →
///   [`BootError::Internal`] (structurally unreachable for SentFresh
///   per `classify_check_result` routing matrix; defensive fail-loud
///   if it ever does fire = Commit 1 routing bug).
///
/// **I1 preserved**: the DPS call is OUTSIDE any `with_immediate`;
/// Envelope 1a is pool-only; Envelope 2 is owned by `stage_finalize::run`
/// per M3a W8 contract.  No nested envelopes.
///
/// **Commit 4a status**: this fn is defined but has no production
/// consumer yet (foundation-only checkpoint per split 4a/4b).
/// `process_via_stage_send` still routes through the pre-W12
/// `apply_w12_confirmation` stub.  Commit 4b will replace that
/// call site with `confirm_drain_doc(SentFresh, ...)` and rebuild
/// the 9 pre-W12 stub-locking tests against the W12 ACK-era
/// acceptance shape (per plan §410).  `#[allow(dead_code)]` is the
/// explicit "not-yet-wired" marker — remove in 4b.
#[allow(dead_code)]
pub async fn confirm_drain_doc(
    pool: &SqlitePool,
    dps: &dyn DpsChannel,
    doc: &fiscal_documents::DocumentRow,
    expected_server_fiscal_no: &str,
    fn_sign: &CheckSignBlob,
    source: Kvt2ConfirmSource,
) -> Result<ConfirmDrainOutcome, BootError> {
    // Commit 4 scope guard — Kvt1Reentry / SentReplay deferred.
    if !matches!(source, Kvt2ConfirmSource::SentFresh) {
        return Err(BootError::Internal(format!(
            "confirm_drain_doc: source {source:?} not yet wired (Commit 4 \
             = SentFresh only; Kvt1Reentry/SentReplay land in Commits 5/5b)"
        )));
    }
    let outcome = evaluate_lastchk(
        dps,
        fn_sign,
        expected_server_fiscal_no,
        source,
        /* sent_replay_trace_attempt_no */ None,
    )
    .await;
    let doc_id = doc.document_id;
    let id_hex = hex_encode_lower(doc.document_id.as_bytes());
    let fiscal_number = doc.fiscal_number.clone();
    match outcome {
        Kvt2ConfirmOutcome::Acked { kvt1_raw_bytes, .. } => {
            commit_sent_fresh_envelope_1a(
                pool,
                &fiscal_number,
                doc_id,
                &id_hex,
                kvt1_raw_bytes,
                expected_server_fiscal_no,
                doc.state,
            )
            .await?;
            // Envelope 2: M3a stage_finalize::run owns its 5-write
            // atomic envelope (Kvt2→Ack + chain seed + inbox DONE +
            // outbox + STAGE_FINALIZE_ACK).  Acked/AlreadyAcked are
            // both success-shapes for W12.
            let finalize_outcome = stage_finalize::run(pool, doc_id).await.map_err(|source| {
                BootError::ReconciliationFailed {
                    fiscal_number: fiscal_number.clone(),
                    source: anyhow::Error::new(source),
                }
            })?;
            match finalize_outcome {
                stage_finalize::StageFinalizeOutcome::Acked { .. }
                | stage_finalize::StageFinalizeOutcome::AlreadyAcked => {
                    Ok(ConfirmDrainOutcome::Advanced)
                }
                stage_finalize::StageFinalizeOutcome::StateConflict { observed } => {
                    Err(BootError::Internal(format!(
                        "confirm_drain_doc(SentFresh): stage_finalize::run \
                         StateConflict {{ observed: {observed} }} for doc \
                         {id_hex} — concurrent writer past App reconcile mutex \
                         (Envelope 1a CAS Kvt1→Kvt2 just committed; another \
                         writer must have rolled state forward to Ack/etc \
                         between Envelope 1a commit and Envelope 2 read)",
                        observed = observed.as_str(),
                    )))
                }
                stage_finalize::StageFinalizeOutcome::DocumentMissing => {
                    Err(BootError::Internal(format!(
                        "confirm_drain_doc(SentFresh): stage_finalize::run \
                         DocumentMissing for doc {id_hex} — row deleted between \
                         Envelope 1a commit and Envelope 2 read (cannot happen \
                         under single-writer App reconcile mutex)"
                    )))
                }
            }
        }
        Kvt2ConfirmOutcome::StructuralDrift { reason, .. } => Err(BootError::Internal(format!(
            "confirm_drain_doc(SentFresh): structural drift for doc {id_hex} \
             — {reason:?}.  NotFound/Mismatch from SentFresh context indicates \
             state-machine drift (server_fiscal_no just stamped by stage_send \
             4-b but DPS does not recognize it).  Halts FN drain per plan §410."
        ))),
        Kvt2ConfirmOutcome::Hold { reason, .. } => Err(BootError::Internal(format!(
            "confirm_drain_doc(SentFresh): Hold path not yet wired for doc \
             {id_hex} — reason={reason:?}.  Commit 6 will project to \
             DocVerdict::HoldFnDrain {{ projection: HeldAtSent }}."
        ))),
        Kvt2ConfirmOutcome::SentNotFoundDowngrade { trace_attempt_no } => {
            Err(BootError::Internal(format!(
                "confirm_drain_doc(SentFresh): SentNotFoundDowngrade(attempt_no={trace_attempt_no}) \
                 is structurally unreachable for SentFresh per Commit 1 \
                 classify_check_result routing — NotFound + SentFresh must \
                 route to StructuralDrift::NotFoundOutsideSentReplay.  If \
                 this surfaces, Commit 1 routing has regressed."
            )))
        }
    }
}

/// Envelope 1a (Commit 4) — atomic Sent→Kvt1 + Kvt1→Kvt2 + Kvt1Raw
/// persist + audit in ONE `with_immediate`.  Caller (`confirm_drain_doc`)
/// has already verified the source is `SentFresh` + outcome is `Acked`
/// + has the canonical `kvt1_raw_bytes` evidence in hand.
///
/// `doc_state_for_audit` is the cohort-walker snapshot state at drain
/// loop entry (OfflineLocalAck / ErrorRetryable for the SentFresh path
/// — stage_send::run advanced through Sending→Sent in its own
/// envelope; this audit shows the user-visible "from" state).
///
/// **Commit 4a status**: invoked only by `confirm_drain_doc`, which
/// itself has no production consumer in 4a.  `#[allow(dead_code)]`
/// is the "not-yet-wired" marker; remove in Commit 4b once
/// `process_via_stage_send` wires `confirm_drain_doc(SentFresh, ...)`.
#[allow(dead_code)]
async fn commit_sent_fresh_envelope_1a(
    pool: &SqlitePool,
    fiscal_number: &str,
    doc_id: DocumentId,
    id_hex: &str,
    kvt1_raw_bytes: Vec<u8>,
    server_fiscal_no: &str,
    doc_state_for_audit: DocState,
) -> Result<(), BootError> {
    // **MED-W12C4A-A fix (plan §62-65 pinned audit contract,
    // 2026-05-22)**: SHA256 digest of the persisted Kvt1Raw evidence
    // bytes — gives operator dashboards an audit-trail cross-link to
    // the `document_files.Kvt1Raw` blob.  Computed BEFORE the move
    // into `with_immediate` closure (kvt1_raw_bytes is consumed by
    // the inner `document_files::replace_tx` call).  Matches existing
    // audit-shape convention (cf. `stage_finalize.rs:338`
    // `unsigned_xml_sha256_hex`).
    let kvt1_raw_sha256_hex = format!("{:x}", Sha256::digest(&kvt1_raw_bytes));
    let payload = serde_json::json!({
        "document_id": id_hex,
        "from_state": doc_state_for_audit.as_str(),
        "to_state": DocState::Kvt2.as_str(),
        "server_fiscal_no": server_fiscal_no,
        // **MED-W12C4A-E fix (plan §64 pinned literal, 2026-05-22)**:
        // dispatch_via value aligned with plan-anchored
        // `"kvt2_confirm"` (was `"w12_sent_fresh"` in 4a foundation —
        // operator-dashboard filter mismatch).
        "dispatch_via": "kvt2_confirm",
        "evidence_source": "lastChk",
        "kvt1_raw_sha256_hex": kvt1_raw_sha256_hex,
    });
    let payload_owned = payload.to_string();
    let id_hex_owned = id_hex.to_string();
    let fn_owned = fiscal_number.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            // (a) Persist Kvt1Raw evidence byte-for-byte.
            document_files::replace_tx(
                tx,
                doc_id,
                document_files::DocumentFileKind::Kvt1Raw,
                &kvt1_raw_bytes,
            )
            .await?;
            // (b) CAS Sent → Kvt1.  stage_send::run committed
            // OfflineLocalAck/ErrorRetryable → Sending → Sent inside
            // its own envelope earlier this tick; we observe Sent here.
            let sent_to_kvt1 =
                fiscal_documents::transition_state(tx, doc_id, DocState::Sent, DocState::Kvt1)
                    .await?;
            if sent_to_kvt1 != TransitionOutcome::Applied {
                return Err(anyhow::anyhow!(
                    "backlog_drain({fn_id}): Envelope 1a CAS Sent→Kvt1 produced \
                     {outcome:?} for doc {doc_hex} (single-writer invariant \
                     breach — App reconcile mutex should prevent races)",
                    fn_id = fn_owned,
                    outcome = sent_to_kvt1,
                    doc_hex = id_hex_owned,
                ));
            }
            // (c) CAS Kvt1 → Kvt2.  W12 advance proof now persisted.
            let kvt1_to_kvt2 =
                fiscal_documents::transition_state(tx, doc_id, DocState::Kvt1, DocState::Kvt2)
                    .await?;
            if kvt1_to_kvt2 != TransitionOutcome::Applied {
                return Err(anyhow::anyhow!(
                    "backlog_drain({fn_id}): Envelope 1a CAS Kvt1→Kvt2 produced \
                     {outcome:?} for doc {doc_hex} (just CAS'd to Kvt1 in same \
                     envelope — concurrent writer impossible)",
                    fn_id = fn_owned,
                    outcome = kvt1_to_kvt2,
                    doc_hex = id_hex_owned,
                ));
            }
            // (d) Forensic audit row.
            audit_log::append_tx(
                tx,
                AUDIT_ENTITY_DOC,
                &id_hex_owned,
                "OFFLINE_DRAIN_KVT2_ADVANCED",
                Severity::Info,
                None,
                Some(&payload_owned),
            )
            .await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await
    .map_err(|source| BootError::ReconciliationFailed {
        fiscal_number: fiscal_number.to_string(),
        source,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transports::dps::error::AuthorizationKind;

    fn ack(id: &str, data_sign: Vec<u8>) -> CheckAck {
        CheckAck {
            id: id.to_string(),
            id_sign: vec![],
            data_sign,
        }
    }

    // ─── Acked happy path ────────────────────────────────────────────

    #[test]
    fn ok_with_data_sign_returns_acked_sent_fresh() {
        let result = Ok(ack("FN-001", vec![0xAA, 0xBB, 0xCC]));
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentFresh, None);
        match outcome {
            Kvt2ConfirmOutcome::Acked {
                kvt1_raw_bytes,
                sent_replay_trace_attempt_no,
            } => {
                assert_eq!(kvt1_raw_bytes, vec![0xAA, 0xBB, 0xCC]);
                assert_eq!(sent_replay_trace_attempt_no, None);
            }
            other => panic!("expected Acked, got {other:?}"),
        }
    }

    #[test]
    fn ok_with_data_sign_returns_acked_sent_replay_threads_trace_attempt_no() {
        let result = Ok(ack("FN-001", vec![0xDE, 0xAD]));
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentReplay, Some(7));
        match outcome {
            Kvt2ConfirmOutcome::Acked {
                kvt1_raw_bytes,
                sent_replay_trace_attempt_no,
            } => {
                assert_eq!(kvt1_raw_bytes, vec![0xDE, 0xAD]);
                assert_eq!(sent_replay_trace_attempt_no, Some(7));
            }
            other => panic!("expected Acked with trace_attempt_no=Some(7), got {other:?}"),
        }
    }

    #[test]
    fn ok_with_data_sign_returns_acked_kvt1_reentry() {
        let result = Ok(ack("FN-001", vec![0xFF]));
        let outcome = classify_check_result(result, Kvt2ConfirmSource::Kvt1Reentry, None);
        assert!(matches!(
            outcome,
            Kvt2ConfirmOutcome::Acked {
                sent_replay_trace_attempt_no: None,
                ..
            }
        ));
    }

    // ─── Hold variants per W0b §97-102 + R3-02 routing matrix ────────

    #[test]
    fn ok_with_empty_data_sign_returns_hold_data_sign_empty() {
        let result = Ok(ack("FN-001", vec![]));
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentReplay, Some(3));
        assert!(matches!(
            outcome,
            Kvt2ConfirmOutcome::Hold {
                reason: Kvt2ConfirmHoldReason::LastChkDataSignEmpty,
                sent_replay_trace_attempt_no: Some(3),
            }
        ));
    }

    #[test]
    fn err_transport_returns_hold_dps_transport() {
        let result = Err(DpsError::Transport("conn reset".into()));
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentFresh, None);
        match outcome {
            Kvt2ConfirmOutcome::Hold {
                reason: Kvt2ConfirmHoldReason::DpsTransport(msg),
                sent_replay_trace_attempt_no: None,
            } => assert_eq!(msg, "conn reset"),
            other => panic!("expected Hold::DpsTransport, got {other:?}"),
        }
    }

    #[test]
    fn err_server_returns_hold_dps_server_with_code_in_msg() {
        let result = Err(DpsError::Server {
            code: -42,
            message: "internal".into(),
        });
        let outcome = classify_check_result(result, Kvt2ConfirmSource::Kvt1Reentry, None);
        match outcome {
            Kvt2ConfirmOutcome::Hold {
                reason: Kvt2ConfirmHoldReason::DpsServer(msg),
                ..
            } => {
                assert!(msg.contains("-42"), "msg should carry status code: {msg}");
                assert!(msg.contains("internal"), "msg should carry message: {msg}");
            }
            other => panic!("expected Hold::DpsServer, got {other:?}"),
        }
    }

    #[test]
    fn err_authorization_returns_hold_dps_authorization() {
        let result = Err(DpsError::Authorization {
            code: -13,
            kind: AuthorizationKind::FiscalNumberNotRegistered,
            message: "FN not registered".into(),
        });
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentReplay, Some(1));
        assert!(matches!(
            outcome,
            Kvt2ConfirmOutcome::Hold {
                reason: Kvt2ConfirmHoldReason::DpsAuthorization(_),
                sent_replay_trace_attempt_no: Some(1),
            }
        ));
    }

    #[test]
    fn err_decode_returns_hold_dps_decode() {
        let result = Err(DpsError::Decode("invalid proto".into()));
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentFresh, None);
        match outcome {
            Kvt2ConfirmOutcome::Hold {
                reason: Kvt2ConfirmHoldReason::DpsDecode(msg),
                ..
            } => assert_eq!(msg, "invalid proto"),
            other => panic!("expected Hold::DpsDecode, got {other:?}"),
        }
    }

    #[test]
    fn err_other_variant_falls_back_to_hold_dps_server_defensively() {
        let result = Err(DpsError::QueryNotSupported("byLocalIdentity"));
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentFresh, None);
        assert!(matches!(
            outcome,
            Kvt2ConfirmOutcome::Hold {
                reason: Kvt2ConfirmHoldReason::DpsServer(_),
                ..
            }
        ));
    }

    // ─── StructuralDrift: ServerFiscalIdMismatch uniform across contexts ──

    #[test]
    fn err_mismatch_returns_structural_drift_sent_fresh() {
        let result = Err(DpsError::ServerFiscalIdMismatch {
            expected_id: "FN-A".into(),
            actual_id: "FN-B".into(),
        });
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentFresh, None);
        match outcome {
            Kvt2ConfirmOutcome::StructuralDrift {
                reason: Kvt2ConfirmStructuralReason::LastChkIdMismatch { observed, expected },
                ..
            } => {
                assert_eq!(observed, "FN-B");
                assert_eq!(expected, "FN-A");
            }
            other => panic!("expected StructuralDrift::LastChkIdMismatch, got {other:?}"),
        }
    }

    #[test]
    fn err_mismatch_returns_structural_drift_sent_replay() {
        let result = Err(DpsError::ServerFiscalIdMismatch {
            expected_id: "FN-A".into(),
            actual_id: "FN-B".into(),
        });
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentReplay, Some(9));
        assert!(matches!(
            outcome,
            Kvt2ConfirmOutcome::StructuralDrift {
                reason: Kvt2ConfirmStructuralReason::LastChkIdMismatch { .. },
                sent_replay_trace_attempt_no: Some(9),
            }
        ));
    }

    #[test]
    fn err_mismatch_returns_structural_drift_kvt1_reentry() {
        let result = Err(DpsError::ServerFiscalIdMismatch {
            expected_id: "FN-A".into(),
            actual_id: "FN-B".into(),
        });
        let outcome = classify_check_result(result, Kvt2ConfirmSource::Kvt1Reentry, None);
        assert!(matches!(
            outcome,
            Kvt2ConfirmOutcome::StructuralDrift {
                reason: Kvt2ConfirmStructuralReason::LastChkIdMismatch { .. },
                ..
            }
        ));
    }

    // ─── NotFound: context-discriminating per HIGH-PR70-R4-01 ────────

    #[test]
    fn err_not_found_sent_replay_returns_sent_not_found_downgrade() {
        let result = Err(DpsError::NotFound);
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentReplay, Some(5));
        assert!(matches!(
            outcome,
            Kvt2ConfirmOutcome::SentNotFoundDowngrade {
                trace_attempt_no: 5
            }
        ));
    }

    #[test]
    fn err_not_found_sent_fresh_returns_structural_drift() {
        let result = Err(DpsError::NotFound);
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentFresh, None);
        match outcome {
            Kvt2ConfirmOutcome::StructuralDrift {
                reason:
                    Kvt2ConfirmStructuralReason::NotFoundOutsideSentReplay {
                        source: Kvt2ConfirmSource::SentFresh,
                    },
                sent_replay_trace_attempt_no: None,
            } => {}
            other => panic!(
                "expected StructuralDrift::NotFoundOutsideSentReplay{{SentFresh}}, got {other:?}"
            ),
        }
    }

    #[test]
    fn err_not_found_kvt1_reentry_returns_structural_drift() {
        let result = Err(DpsError::NotFound);
        let outcome = classify_check_result(result, Kvt2ConfirmSource::Kvt1Reentry, None);
        assert!(matches!(
            outcome,
            Kvt2ConfirmOutcome::StructuralDrift {
                reason: Kvt2ConfirmStructuralReason::NotFoundOutsideSentReplay {
                    source: Kvt2ConfirmSource::Kvt1Reentry,
                },
                ..
            }
        ));
    }

    #[test]
    #[should_panic(expected = "SentReplay context requires pre-allocated trace attempt_no")]
    fn err_not_found_sent_replay_without_trace_attempt_no_panics() {
        // Contract violation: SentReplay + NotFound but caller failed
        // to allocate trace row pre-DPS-call.  Helper-heavy ownership
        // contract makes this structurally impossible in production,
        // but the panic guards against a future implementation bug
        // that bypasses Envelope 1c-pre.
        let result = Err(DpsError::NotFound);
        let _ = classify_check_result(result, Kvt2ConfirmSource::SentReplay, None);
    }
}
