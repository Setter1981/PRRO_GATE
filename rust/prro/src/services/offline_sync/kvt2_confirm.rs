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
use crate::db::repositories::{audit_log, document_files, fiscal_documents, transport_trace};
use crate::db::tx::with_immediate;
use crate::services::offline_sync::backlog_drain::{
    FailureClass, HoldFnDrainProjection, AUDIT_ENTITY_DOC,
};
use crate::services::write_path::types::hex_encode_lower;
use crate::services::write_path::{kvt2_advance, stage_finalize};
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

impl Kvt2ConfirmSource {
    /// **M3b W12 Commit 4b.1 (plan §212 + §311, 2026-05-22)** —
    /// stable string identifier for the `source` field in
    /// `KVT2_CONFIRM_HOLD` / `KVT2_CONFIRM_STRUCTURAL_DRIFT` audit
    /// payloads.  Operator dashboards filter on these literals; do not
    /// rename without coordinating with the W12 audit-shape contract.
    pub fn audit_label(self) -> &'static str {
        match self {
            Self::SentFresh => "sent_fresh",
            Self::SentReplay => "sent_replay",
            Self::Kvt1Reentry => "kvt1_reentry",
        }
    }
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
    /// **SEAM-B-3 (architect-locked contract, 2026-06-13)** —
    /// SentReplay-exclusive non-fatal outcome.  A `ServerFiscalIdMismatch`
    /// on a SENT doc that is NOT the FN's newest submitted doc
    /// (`doc.lnd < max_submitted_lnd`): a NEWER submitted doc became the
    /// DPS `last_chk` tip.  The ACK status of THIS doc is UNKNOWN from
    /// `last_chk` — the probe only reports that a newer submitted doc is
    /// now the FN tip; this SENT doc may be acked-then-superseded OR never
    /// acked.  So we HOLD in SENT and do NOT conclude (mirrors boot's
    /// `dispatch_sent_via_probe` M1-02 arm, which holds rather than
    /// terminalises); resolution is deferred to B1-v2 doc-scoped
    /// confirmation.
    /// Caller (`confirm_drain_doc`) completes the recovery trace
    /// (`RetryableServer` / `LAST_CHK_TIP_SUPERSEDED`) + emits a
    /// `TIP_SUPERSEDED` audit, leaves the doc in SENT (NO state change),
    /// and projects to `ConfirmDrainOutcome::SupersededHeld` so the drain
    /// CONTINUES past it (does NOT `BootError::Internal`-halt the FN).
    /// `dps_tip_id` is the DPS tip's `server_fiscal_no` (the newer doc),
    /// threaded into the trace + audit for forensics.  The `superseded`
    /// verdict is computed by the caller BEFORE the classifier call (it
    /// reads `max_submitted_lnd` + `doc.lnd`), so the classifier stays
    /// pure.
    SupersededHold {
        dps_tip_id: String,
        sent_replay_trace_attempt_no: Option<i64>,
    },
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

impl Kvt2ConfirmHoldReason {
    /// **M3b W12 Commit 4b.1 (plan §211-212, 2026-05-22)** — stable
    /// string identifier for the `hold_reason` field in
    /// `KVT2_CONFIRM_HOLD` audit payloads.  Operator dashboards filter
    /// on these literals; do not rename without coordinating with the
    /// W12 audit-shape contract.  Detail string (transport/server/auth
    /// error message) goes into separate `hold_reason_detail` field.
    pub fn audit_label(&self) -> &'static str {
        match self {
            Self::DpsTransport(_) => "DPS_TRANSPORT",
            Self::DpsServer(_) => "DPS_SERVER",
            Self::DpsAuthorization(_) => "DPS_AUTHORIZATION",
            Self::DpsDecode(_) => "DPS_DECODE",
            Self::LastChkDataSignEmpty => "LASTCHK_DATA_SIGN_EMPTY",
        }
    }
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

impl Kvt2ConfirmStructuralReason {
    /// **M3b W12 Commit 4b.1 (plan §311, 2026-05-22)** — stable string
    /// identifier for the `drift_reason` field in
    /// `KVT2_CONFIRM_STRUCTURAL_DRIFT` audit payloads.  Operator
    /// dashboards filter on these literals; do not rename without
    /// coordinating with the W12 audit-shape contract.  Detail string
    /// (observed/expected ids, observed state) goes into separate
    /// `drift_reason_detail` field.
    pub fn audit_label(&self) -> &'static str {
        match self {
            Self::ServerFiscalNoMissing => "SERVER_FISCAL_NO_MISSING",
            Self::CasMissOnAdvance { .. } => "CAS_MISS_ON_ADVANCE",
            Self::LastChkIdMismatch { .. } => "LASTCHK_ID_MISMATCH",
            Self::NotFoundOutsideSentReplay { .. } => "NOT_FOUND_OUTSIDE_SENT_REPLAY",
        }
    }

    /// **M3b W12 Commit 5 Δ2 (OBS-W12C5-1, 2026-05-22)** — human-
    /// readable detail string for `drift_reason_detail` field в
    /// `KVT2_CONFIRM_STRUCTURAL_DRIFT` audit payload.  Variants із
    /// inner data fall back to `{self:?}` Debug rendering
    /// (preserves observed/expected ids for operator triage of
    /// `LastChkIdMismatch` etc).  Unit variants return contextual
    /// description instead of literal variant-name (which would
    /// duplicate `drift_reason` field — value-add zero).
    pub fn detail_message(&self) -> String {
        match self {
            Self::ServerFiscalNoMissing => "doc state at Kvt1 with NULL \
                 server_fiscal_no — stage_send 4-b stamp invariant \
                 breach; persisted Sent advance MUST always stamp \
                 server_fiscal_no before transitioning to Sent"
                .to_string(),
            // Variants із inner data: Debug renders observed +
            // expected fields — preserves operator triage signal.
            other => format!("{other:?}"),
        }
    }
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
///
/// **SEAM-B-3 (2026-06-13)** — `superseded` is the caller-computed
/// M1-02 verdict for the `ServerFiscalIdMismatch` arm: `true` when the
/// probed SENT doc is NOT the FN's newest submitted doc
/// (`doc.lnd < max_submitted_lnd`).  The caller reads the repo +
/// `doc.lnd` BEFORE calling this function so the classifier stays PURE.
/// Only the `ServerFiscalIdMismatch` arm reads it (all other arms ignore
/// it).  It is `true` only on the SentReplay path; every other source /
/// outcome passes `false` (boot-parity scope: superseded is a
/// SentReplay-exclusive exception).
pub fn classify_check_result(
    result: Result<CheckAck, DpsError>,
    source: Kvt2ConfirmSource,
    sent_replay_trace_attempt_no: Option<i64>,
    superseded: bool,
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
        }) => {
            if superseded {
                // **SEAM-B-3**: the probed SENT doc is NOT the FN's newest
                // submitted doc — a newer submitted doc became the DPS
                // last_chk tip.  The ACK status of THIS doc is UNKNOWN from
                // last_chk (it may be acked-then-superseded OR never acked);
                // last_chk only tells us a newer doc is now the tip.  So we
                // do NOT conclude — non-fatal: caller HOLDS it in SENT, emits
                // TIP_SUPERSEDED, and the drain continues (M1-02 parity:
                // boot holds, does not terminalise).
                Kvt2ConfirmOutcome::SupersededHold {
                    dps_tip_id: actual_id,
                    sent_replay_trace_attempt_no,
                }
            } else {
                // Mismatch on the ACTUAL newest doc (no newer submitted
                // doc) → genuine state-machine drift → fatal/escalates
                // (UNCHANGED pre-SEAM-B-3 behaviour).
                Kvt2ConfirmOutcome::StructuralDrift {
                    reason: Kvt2ConfirmStructuralReason::LastChkIdMismatch {
                        observed: actual_id,
                        expected: expected_id,
                    },
                    sent_replay_trace_attempt_no,
                }
            }
        }
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
    // **M2-N4 (architect ruling, 2026-06-13)** — the `server_fiscal_no`s of the
    // FN's submitted docs with `lnd > doc.lnd` (caller-fetched; see
    // `confirm_drain_doc`).  A `ServerFiscalIdMismatch` is "superseded" (NOT
    // structural drift) ONLY if the DPS tip `actual_id` is one of these — i.e.
    // a NEWER submitted doc of OURS became the tip.  A foreign/garbage tip is
    // genuine drift.  This subsumes the lnd-only SEAM-B-3 check (a non-empty
    // set ⟺ `doc.lnd < max_submitted_lnd`).  The membership test is done HERE
    // (post-DPS, where `actual_id` is known) so `classify_check_result` stays a
    // PURE `bool` consumer.  Empty for non-SentReplay sources.
    newer_submitted_sfns: &[String],
) -> Kvt2ConfirmOutcome {
    let result = dps
        .by_server_fiscal_no(fn_sign, expected_server_fiscal_no)
        .await;
    let superseded = match &result {
        Err(DpsError::ServerFiscalIdMismatch { actual_id, .. }) => {
            newer_submitted_sfns.iter().any(|sfn| sfn == actual_id)
        }
        _ => false,
    };
    classify_check_result(result, source, sent_replay_trace_attempt_no, superseded)
}

// ─── M3b W12 Commit 4 — confirm_drain_doc + Envelope 1a ─────────────

/// Drain-projected outcome of [`confirm_drain_doc`].
///
/// **Commit 4b production surface** wires only the `Advanced` variant
/// (SentFresh happy path; `process_via_stage_send` consumer).  Commits
/// 5 / 5b / 6 will extend this enum with `Hold` (HoldFnDrain
/// projection), `SentNotFoundDowngrade` (SentReplay safe-redrive), and
/// source-specific projections per plan §"Helper vs caller envelope
/// ownership".  Pre-Commit-6 callers observe non-Acked outcomes as
/// [`BootError::Internal`] (defensive fail-loud until HoldFnDrain
/// projection wiring lands in Commit 6).
///
/// **M3b W12 Commit 5 status (2026-05-22)**: production-wired for
/// SentFresh (process_via_stage_send) + Kvt1Reentry
/// (process_via_w12_only).  Hold and StructuralDrift contexts emit
/// forensic audit envelopes (`KVT2_CONFIRM_HOLD` /
/// `KVT2_CONFIRM_STRUCTURAL_DRIFT`) BEFORE `BootError::Internal`
/// halt per plan §311 MED-PR70-R12-01.  SentReplay remains
/// scope-guarded at `confirm_drain_doc` entry — returns
/// `BootError::Internal` until Commit 5b lands the Envelope 1c-pre
/// `transport_trace.allocate_and_insert_tx` + bundled
/// 1a-replay / 1c-post / 1c-hold / 1c-drift chain per plan §412.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmDrainOutcome {
    /// **M3b W12 Commit 5b.1 (plan §412, 2026-05-22)** — Hold path
    /// (transient evidence failure) OR SentNotFoundDowngrade path
    /// (HIGH-C5-3 safe-redrive).  Doc state is either:
    ///   - `Sent` (unchanged) for SentReplay Hold contexts
    ///     (Envelope 1c-hold-light or 1c-hold bundled emit only
    ///     audit, no CAS); projection = `HeldAtSent`.
    ///   - `Sent` (unchanged) for SentFresh Hold; projection =
    ///     `HeldAtSent`.
    ///   - `Kvt1` (unchanged) for Kvt1Reentry Hold; projection =
    ///     `HeldAtKvt1`.
    ///   - `ErrorRetryable` (advanced via Envelope 1c-post) for
    ///     SentReplay SentNotFoundDowngrade; projection =
    ///     `ErRedriveQueued` (HIGH-C5-3 safe-redrive — next-tick
    ///     ER cohort dispatch + bounded Pattern B redrive via
    ///     `stage_send::run`).
    ///
    /// Caller projects to `DocVerdict::HoldFnDrain { class,
    /// projection, consecutive_holds }`.  Post-Commit-6 wires
    /// SentFresh + Kvt1Reentry Hold paths; Commit 6.1.2 adds
    /// `consecutive_holds` field plumbing для drain-orchestrator
    /// Tier 1 (>= 10) + Tier 2 (>= 50) trigger checks per REC-1.
    ///
    /// `consecutive_holds` semantics:
    ///   - HeldAtSent / HeldAtKvt1 → post-increment value from
    ///     Envelope 1c-hold-light (SentFresh/Kvt1Reentry) OR 1c-hold
    ///     bundled (SentReplay) — guides Tier 1/2 trigger.
    ///   - ErRedriveQueued → 0 (Envelope 1c-post resets counter
    ///     atomically з Sent→ER CAS; ER class guard takes over via
    ///     transport_trace.retry_class budget rather than counter).
    HoldFnDrain {
        projection: HoldFnDrainProjection,
        consecutive_holds: i64,
        /// **Commit 6.2 / Phase 2b REC-6 (2026-05-24)** — granular
        /// `FailureClass` derived per Hold reason via
        /// [`map_hold_reason_to_class`]:
        ///   - `DpsTransport` → `FailureClass::Transport`
        ///   - `DpsServer` → `FailureClass::Server`
        ///   - `DpsAuthorization` → `FailureClass::Authorization`
        ///   - `DpsDecode` → `FailureClass::Decode`
        ///   - `LastChkDataSignEmpty` → `FailureClass::Internal`
        ///   - SentNotFoundDowngrade → `FailureClass::NotFound`
        ///     (not via helper — distinct outcome variant з direct
        ///     hardcoded mapping; rationale: DPS healthy + functioning
        ///     normally, NotFound is application-level negative result,
        ///     NOT transport/decode failure → avoid auto-offline
        ///     false-positives per memory `feedback_offline_transition_
        ///     strategy`).
        ///
        /// Replaces Commit 6.1.2 hardcoded `FailureClass::Transport`
        /// at all 3 caller sites — operator dashboards тепер можуть
        /// distinguish "network glitch" від "DPS internal error"
        /// vs "DPS auth issue" в hold counters.
        class: FailureClass,
    },
    /// Envelope 1a (Kvt1Raw persist + Sent→Kvt1 CAS + Kvt1→Kvt2 CAS +
    /// `OFFLINE_DRAIN_KVT2_ADVANCED` audit) committed atomically;
    /// Envelope 2 (`stage_finalize::run` Kvt2→Ack) committed.  Doc is
    /// now in terminal `Ack` state.  Caller updates `DrainSummary`
    /// with the Acked outcome.
    Advanced,
    /// **SEAM-B-3 (architect-locked contract, 2026-06-13)** —
    /// SentReplay-exclusive: the probed SENT doc is NOT the FN's newest
    /// submitted doc, so it is SUPERSEDED as the `last_chk` tip.  Its ACK
    /// status is UNKNOWN from `last_chk` (it may be acked-then-superseded
    /// OR never acked) — we HOLD, we do NOT conclude (M1-02 parity with
    /// boot's `dispatch_sent_via_probe`, which holds rather than
    /// terminalises).  `confirm_drain_doc` has already completed the
    /// recovery trace (`RetryableServer` / `LAST_CHK_TIP_SUPERSEDED`) +
    /// emitted a `TIP_SUPERSEDED` audit; the doc stays in SENT (NO state
    /// change, NO `consecutive_holds` increment).
    ///
    /// **Caller MUST sibling-continue, NOT stop.**  Unlike `HoldFnDrain`
    /// (which halts the FN drain at the held doc), the superseded doc has
    /// a LOWER `lnd` than the tip and is processed FIRST in the `lnd ASC`
    /// cohort — stopping here would strand the higher-`lnd` tip.  The
    /// drain consumer maps this to a continue-verdict that records the doc
    /// as held-at-sent (blocks finalize, conservative) and proceeds to the
    /// next doc.  Resolution of the superseded doc is deferred to the
    /// B1-v2 doc-scoped confirmation / monitoring (same as boot).
    SupersededHeld,
}

/// W12 high-level helper — orchestrates the full Sent-source W12
/// confirmation chain per plan §410 (Commit 4 wiring scope).
///
/// **Source-context support matrix** (post W12 Commit 5):
/// - [`Kvt2ConfirmSource::SentFresh`] → **production-wired** (Commit
///   4b).  Caller = `process_via_stage_send` after
///   `StageSendOutcome::Sent`.  `expected_server_fiscal_no` MUST be
///   sourced from the `StageSendOutcome::Sent` variant
///   (`&outcome.server_fiscal_no`), NOT from the pre-stage_send
///   cohort snapshot `doc.server_fiscal_no` (which is `None` at
///   cohort SELECT time per stage_send 4-b invariant —
///   MED-PR70-R11-01 handoff).  On Acked → Envelope 1a (3-CAS).
/// - [`Kvt2ConfirmSource::Kvt1Reentry`] → **production-wired**
///   (Commit 5).  Caller = `process_via_w12_only` on Kvt1 cohort
///   entry.  `expected_server_fiscal_no` MUST be sourced from
///   persisted `doc.server_fiscal_no` with caller-level
///   `BootError::Internal` fail-loud on None (state-machine
///   invariant breach: Kvt1 ALWAYS implies stage_send 4-b stamped
///   it).  On Acked → Envelope 1b (2-CAS — no Sent→Kvt1 because
///   doc was already at Kvt1).
/// - [`Kvt2ConfirmSource::SentReplay`] → deferred to Commit 5b
///   (`process_via_lastchk_replay` rewrite, Envelope 1c-pre /
///   1a-replay / 1c-post / 1c-hold / 1c-drift chain per plan §412).
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
/// **M3b W12 Commit 4b status (2026-05-22)**: production-wired for
/// SentFresh via `process_via_stage_send` Sent branch (replaced
/// pre-W12 `apply_w12_confirmation` call site).  Visibility narrowed
/// to `pub(in crate::services::offline_sync)` — restricts call sites
/// to sibling modules under `offline_sync`.  Kvt1Reentry / SentReplay
/// scope-guarded at entry until Commits 5 / 5b land their source-
/// specific envelope chains (Envelope 1b / 1c-pre).
// **B1 reuse (audit pass-2, 2026-06-11):** visibility raised
// `pub(in crate::services::offline_sync) → pub(crate)` so the runtime
// online-convergence tick (`services::reconciliation::online_convergence`)
// re-drives a *resting* online `KVT1` doc through THE SAME `Kvt1Reentry`
// confirm path — lastChk → `advance_to_ack(doc.state = Kvt1)` (runtime-neutral,
// see the RS-3 A2.1a note below) → ACK + inbox DONE; Hold → stays KVT1.  No
// copy, no behaviour change (mirrors spec §2).
pub(crate) async fn confirm_drain_doc(
    pool: &SqlitePool,
    dps: &dyn DpsChannel,
    doc: &fiscal_documents::DocumentRow,
    expected_server_fiscal_no: &str,
    fn_sign: &CheckSignBlob,
    source: Kvt2ConfirmSource,
    // **LOW-W12C4A-04 / LOW-W12C4A-D fix (4b.3, 2026-05-22)**:
    // `attempt_no` from `StageSendOutcome::Sent` threaded into
    // Envelope 1a audit payload to preserve operator-dashboard
    // continuity with the pre-W12 stub `attempt_no` field.  None
    // for sources that do not carry an attempt counter
    // (Kvt1Reentry has no fresh wire attempt; SentReplay's
    // attempt_no lives in `transport_trace`, NOT this surface).
    attempt_no: Option<i64>,
) -> Result<ConfirmDrainOutcome, BootError> {
    // **M3b W12 Commit 5b.1 scope (2026-05-22)**: SentFresh +
    // Kvt1Reentry + SentReplay all wired.  SentReplay path
    // additionally allocates Envelope 1c-pre `transport_trace`
    // recovery row BEFORE the DPS call, then routes outcomes through
    // bundled (vs light) completion envelopes that include
    // `transport_trace.complete_tx` atomically per plan §412.
    let doc_id = doc.document_id;
    let id_hex = hex_encode_lower(doc.document_id.as_bytes());
    let fiscal_number = doc.fiscal_number.clone();
    // For SentReplay: allocate recovery trace row + capture wire
    // timing around the DPS call.  Other sources skip this — their
    // light envelope variants do not have a trace row to complete.
    let (sent_replay_trace_attempt_no, sent_replay_wire_started): (Option<i32>, Option<String>) =
        match source {
            Kvt2ConfirmSource::SentReplay => {
                let attempt_no = commit_sent_replay_envelope_1c_pre(
                    pool,
                    &fiscal_number,
                    doc_id,
                    doc.backend_profile_id.clone(),
                    doc.transport_profile_id.clone(),
                )
                .await?;
                (Some(attempt_no), Some(iso8601_now()))
            }
            _ => (None, None),
        };
    // **M2-N4 (architect ruling, 2026-06-13; refines SEAM-B-3)**: fetch the
    // FN's submitted docs with `lnd > doc.lnd` (the only legitimate
    // supersession candidates) BEFORE the DPS call.  A SentReplay
    // `ServerFiscalIdMismatch` is "superseded" (NOT structural drift) ONLY if
    // the DPS tip `actual_id` equals the `server_fiscal_no` of one of these
    // newer submitted docs — checked inside `evaluate_lastchk` (post-DPS) so
    // `classify_check_result` stays a pure `bool` consumer.  A foreign/garbage
    // tip → drift.  `superseded_max_lnd` retains the max `lnd` of the newer set
    // for the TIP_SUPERSEDED audit payload (it is `Some` whenever superseded is
    // true, since a match implies a non-empty set).  Non-SentReplay sources
    // pass an empty set (boot-parity scope).
    let newer_submitted: Vec<(i64, String)> = match source {
        Kvt2ConfirmSource::SentReplay => {
            fiscal_documents::submitted_above_lnd(pool, &fiscal_number, doc.lnd)
                .await
                .map_err(BootError::Database)?
        }
        _ => Vec::new(),
    };
    let superseded_max_lnd: Option<i64> = newer_submitted.iter().map(|(lnd, _)| *lnd).max();
    let newer_submitted_sfns: Vec<String> =
        newer_submitted.into_iter().map(|(_, sfn)| sfn).collect();
    let outcome = evaluate_lastchk(
        dps,
        fn_sign,
        expected_server_fiscal_no,
        source,
        sent_replay_trace_attempt_no.map(i64::from),
        &newer_submitted_sfns,
    )
    .await;
    // Capture wire-finish timestamp AFTER DPS call returns (only
    // meaningful for SentReplay context with allocated trace).
    let sent_replay_wire_finished = sent_replay_wire_started.as_ref().map(|_| iso8601_now());
    match outcome {
        Kvt2ConfirmOutcome::Acked { kvt1_raw_bytes, .. } => {
            // **LOW-W12C4A-07 fix (4b.3, 2026-05-22)**: defensive
            // non-empty guard.  classify_check_result routes empty
            // data_sign to Hold(LastChkDataSignEmpty) per
            // kvt2_confirm.rs:188-192 — empty kvt1_raw_bytes
            // reaching Acked here means Commit 1 routing regression.
            // Fail-loud BEFORE persisting empty Kvt1Raw row that
            // would silently corrupt forensic evidence.
            // **RS-3 A2.1a**: this OUTER guard covers ALL three sources
            // (incl. SentReplay, which does NOT delegate to
            // `advance_to_ack`).  `advance_to_ack` carries its OWN
            // empty-bytes guard as the runtime-neutral backstop for the
            // A2.1b online caller — do NOT "dedup" this outer one away.
            if kvt1_raw_bytes.is_empty() {
                return Err(BootError::Internal(format!(
                    "confirm_drain_doc({source:?}): Acked outcome with empty \
                     kvt1_raw_bytes for doc {id_hex} — classify_check_result \
                     invariant violated (should route empty data_sign to \
                     Hold(LastChkDataSignEmpty); see kvt2_confirm.rs:188-192)"
                )));
            }
            // **RS-3 A2.1a (2026-06-09, plan §A2.1a decision a)**: the
            // Sent→Kvt1→Kvt2 CAS chain + Envelope 2 (stage_finalize
            // Kvt2→Ack) is now owned by the runtime-neutral confirmer
            // `write_path::kvt2_advance::advance_to_ack`, so the online
            // ladder (A2.1b) can reach ACK inline WITHOUT BootError /
            // drain source-routing.  SentFresh + Kvt1Reentry delegate to
            // it — their Envelope 1 is a pure CAS chain (1a / 1b), source-
            // selected inside advance_to_ack by `doc.state` (Kvt1 → 1b
            // 2-CAS; anything else → 1a 3-CAS, audit `from_state` = the
            // cohort-walker snapshot).  SentReplay keeps its bundled
            // Envelope 1a-replay HERE because it must complete the
            // `transport_trace` recovery row atomically with the CAS
            // (drain-specific lifecycle — plan §412; not runtime-neutral).
            match source {
                Kvt2ConfirmSource::SentFresh | Kvt2ConfirmSource::Kvt1Reentry => {
                    kvt2_advance::advance_to_ack(
                        pool,
                        doc_id,
                        kvt1_raw_bytes,
                        expected_server_fiscal_no,
                        doc.state,
                        attempt_no,
                    )
                    .await
                    .map_err(|err| map_confirm_error(err, &fiscal_number))?;
                    Ok(ConfirmDrainOutcome::Advanced)
                }
                Kvt2ConfirmSource::SentReplay => {
                    let trace_attempt_no = sent_replay_trace_attempt_no
                        .expect("SentReplay implies 1c-pre allocated trace_attempt_no");
                    let wire_started = sent_replay_wire_started
                        .clone()
                        .expect("SentReplay implies wire_started captured");
                    let wire_finished = sent_replay_wire_finished
                        .clone()
                        .expect("SentReplay implies wire_finished captured");
                    commit_sent_replay_envelope_1a_replay(
                        pool,
                        &fiscal_number,
                        doc_id,
                        &id_hex,
                        kvt1_raw_bytes,
                        expected_server_fiscal_no,
                        doc.state,
                        trace_attempt_no,
                        wire_started,
                        wire_finished,
                    )
                    .await?;
                    // Envelope 2: M3a stage_finalize::run owns its 5-write
                    // atomic envelope (Kvt2→Ack + chain seed + inbox DONE +
                    // outbox + STAGE_FINALIZE_ACK).  Acked/AlreadyAcked are
                    // both success-shapes for W12.  Kept inline (not via
                    // advance_to_ack) because the SentReplay Acked arm uses
                    // the bundled 1a-replay envelope above, not the pure 1a.
                    let finalize_outcome =
                        stage_finalize::run(pool, doc_id).await.map_err(|err| {
                            BootError::ReconciliationFailed {
                                fiscal_number: fiscal_number.clone(),
                                source: anyhow::Error::new(err),
                            }
                        })?;
                    match finalize_outcome {
                        stage_finalize::StageFinalizeOutcome::Acked { .. }
                        | stage_finalize::StageFinalizeOutcome::AlreadyAcked => {
                            Ok(ConfirmDrainOutcome::Advanced)
                        }
                        stage_finalize::StageFinalizeOutcome::StateConflict { observed } => {
                            Err(BootError::Internal(format!(
                                "confirm_drain_doc({source:?}): stage_finalize::run \
                                 StateConflict {{ observed: {observed} }} for doc \
                                 {id_hex} — concurrent writer past App reconcile mutex \
                                 (Envelope 1 CAS Kvt1→Kvt2 just committed; another \
                                 writer must have rolled state forward to Ack/etc \
                                 between Envelope 1 commit and Envelope 2 read)",
                                observed = observed.as_str(),
                            )))
                        }
                        stage_finalize::StageFinalizeOutcome::DocumentMissing => {
                            Err(BootError::Internal(format!(
                                "confirm_drain_doc({source:?}): stage_finalize::run \
                                 DocumentMissing for doc {id_hex} — row deleted between \
                                 Envelope 1 commit and Envelope 2 read (cannot happen \
                                 under single-writer App reconcile mutex)"
                            )))
                        }
                    }
                }
            }
        }
        Kvt2ConfirmOutcome::StructuralDrift { reason, .. } => {
            // **M3b W12 Commit 5b.1 (plan §311 + §412, 2026-05-22)**:
            // source-aware drift envelope.  SentFresh/Kvt1Reentry →
            // 1c-drift-light (audit-only).  SentReplay → 1c-drift
            // bundled (trace.complete + audit) — completes recovery
            // trace row allocated by 1c-pre BEFORE BootError fail-loud
            // per plan §412 MED-PR70-R6-01 trace lifecycle.
            match source {
                Kvt2ConfirmSource::SentReplay => {
                    let trace_attempt_no = sent_replay_trace_attempt_no
                        .expect("SentReplay implies 1c-pre allocated trace_attempt_no");
                    let wire_started = sent_replay_wire_started
                        .clone()
                        .expect("SentReplay implies wire_started captured");
                    let wire_finished = sent_replay_wire_finished
                        .clone()
                        .expect("SentReplay implies wire_finished captured");
                    commit_sent_replay_envelope_1c_drift(
                        pool,
                        &fiscal_number,
                        doc_id,
                        &id_hex,
                        trace_attempt_no,
                        wire_started,
                        wire_finished,
                        &reason,
                    )
                    .await?;
                }
                _ => {
                    commit_drift_envelope_1c_drift_light(
                        pool,
                        &fiscal_number,
                        &id_hex,
                        source,
                        &reason,
                    )
                    .await?;
                }
            }
            Err(BootError::Internal(format!(
                "confirm_drain_doc({source:?}): structural drift for doc {id_hex} \
                 — {reason:?}.  NotFound/Mismatch indicates state-machine drift \
                 (server_fiscal_no not recognized by DPS).  Halts FN drain per \
                 plan §410.  KVT2_CONFIRM_STRUCTURAL_DRIFT audit emitted prior \
                 to halt."
            )))
        }
        Kvt2ConfirmOutcome::Hold { reason, .. } => {
            // **M3b W12 Commit 5b.1 (plan §311 + §412, 2026-05-22)**:
            // source-aware hold envelope.  SentFresh/Kvt1Reentry →
            // 1c-hold-light (audit-only) + BootError::Internal
            // defensive marker (HoldFnDrain projection deferred to
            // Commit 6).  SentReplay → 1c-hold bundled
            // (trace.complete Retryable* + audit) + Ok(HoldFnDrain
            // { HeldAtSent }) per plan §412 (cannot defer because
            // HIGH-C5-3 contract requires HoldFnDrain projection for
            // SentReplay-specific safe-redrive accounting).
            match source {
                Kvt2ConfirmSource::SentReplay => {
                    let trace_attempt_no = sent_replay_trace_attempt_no
                        .expect("SentReplay implies 1c-pre allocated trace_attempt_no");
                    let wire_started = sent_replay_wire_started
                        .clone()
                        .expect("SentReplay implies wire_started captured");
                    let wire_finished = sent_replay_wire_finished
                        .clone()
                        .expect("SentReplay implies wire_finished captured");
                    let consecutive_holds = commit_sent_replay_envelope_1c_hold(
                        pool,
                        &fiscal_number,
                        doc_id,
                        &id_hex,
                        trace_attempt_no,
                        wire_started,
                        wire_finished,
                        &reason,
                    )
                    .await?;
                    Ok(ConfirmDrainOutcome::HoldFnDrain {
                        projection: HoldFnDrainProjection::HeldAtSent,
                        consecutive_holds,
                        class: map_hold_reason_to_class(&reason),
                    })
                }
                Kvt2ConfirmSource::SentFresh => {
                    let consecutive_holds = commit_hold_envelope_1c_hold_light(
                        pool,
                        &fiscal_number,
                        doc_id,
                        &id_hex,
                        source,
                        &reason,
                    )
                    .await?;
                    Ok(ConfirmDrainOutcome::HoldFnDrain {
                        projection: HoldFnDrainProjection::HeldAtSent,
                        consecutive_holds,
                        class: map_hold_reason_to_class(&reason),
                    })
                }
                Kvt2ConfirmSource::Kvt1Reentry => {
                    let consecutive_holds = commit_hold_envelope_1c_hold_light(
                        pool,
                        &fiscal_number,
                        doc_id,
                        &id_hex,
                        source,
                        &reason,
                    )
                    .await?;
                    Ok(ConfirmDrainOutcome::HoldFnDrain {
                        projection: HoldFnDrainProjection::HeldAtKvt1,
                        consecutive_holds,
                        class: map_hold_reason_to_class(&reason),
                    })
                }
            }
        }
        Kvt2ConfirmOutcome::SentNotFoundDowngrade { trace_attempt_no } => {
            // **M3b W12 Commit 5b.1 (plan §412 HIGH-C5-3 safe-redrive
            // contract, 2026-05-22)**: SentReplay-exclusive outcome
            // — DPS has zero history of `server_fiscal_no`, so doc
            // should re-send via Pattern B next tick (NOT poll
            // forever).  Commits Envelope 1c-post (trace.complete
            // TransientRetry + Sent→ER + audit) then returns
            // `HoldFnDrain { ErRedriveQueued }`.  Caller maps to
            // `DocVerdict::HoldFnDrain { ErRedriveQueued }` — next-
            // tick ER cohort dispatch via W9b ER-class-guard bounded
            // Pattern B redrive through `stage_send::run`.
            if !matches!(source, Kvt2ConfirmSource::SentReplay) {
                return Err(BootError::Internal(format!(
                    "confirm_drain_doc({source:?}): SentNotFoundDowngrade(attempt_no={trace_attempt_no}) \
                     is structurally unreachable for non-SentReplay per Commit 1 \
                     classify_check_result routing — NotFound + non-SentReplay must \
                     route to StructuralDrift::NotFoundOutsideSentReplay.  If this \
                     surfaces, Commit 1 routing has regressed."
                )));
            }
            // **LOW-W12C5B1-B fix (5b.1 Δ4, 2026-05-23)**: use our own
            // 1c-pre allocation (i32, infallible-narrow) instead of the
            // outcome's i64 echo + try_into.  Symmetric з Acked/Drift/
            // Hold arms (all already consume sent_replay_trace_attempt_no
            // directly).  The destructured `trace_attempt_no` from the
            // variant stays in scope for the non-SentReplay diagnostic
            // above; intentionally unused on the SentReplay production
            // path (the outcome merely echoes what we passed at line 545).
            let _ = trace_attempt_no;
            let trace_attempt_no_i32 = sent_replay_trace_attempt_no
                .expect("SentReplay implies 1c-pre allocated trace_attempt_no");
            let wire_started = sent_replay_wire_started
                .clone()
                .expect("SentReplay implies wire_started captured");
            let wire_finished = sent_replay_wire_finished
                .clone()
                .expect("SentReplay implies wire_finished captured");
            commit_sent_replay_envelope_1c_post(
                pool,
                &fiscal_number,
                doc_id,
                &id_hex,
                trace_attempt_no_i32,
                wire_started,
                wire_finished,
                expected_server_fiscal_no,
            )
            .await?;
            // **REC-1 Phase 2a.1 Commit 6.1.2 (2026-05-24)**:
            // ErRedriveQueued surface counter == 0 because Envelope
            // 1c-post atomically reset counter (Sent→ER advance =
            // any-advance-resets contract); ER class guard takes
            // over via transport_trace.retry_class budget.
            // **REC-6 Phase 2b Commit 6.2 (2026-05-24)**: class =
            // `FailureClass::NotFound` (semantically точно — DPS
            // healthy + functioning, application-level negative
            // result).  НЕ `Transport` бо це не network failure —
            // запобігає false-positive auto-offline transitions per
            // memory `feedback_offline_transition_strategy`.
            Ok(ConfirmDrainOutcome::HoldFnDrain {
                projection: HoldFnDrainProjection::ErRedriveQueued,
                consecutive_holds: 0,
                class: FailureClass::NotFound,
            })
        }
        Kvt2ConfirmOutcome::SupersededHold { dps_tip_id, .. } => {
            // **SEAM-B-3 (architect-locked contract, 2026-06-13)**:
            // SentReplay-exclusive non-fatal outcome.  A SENT doc that is
            // NOT the FN's newest submitted doc — a newer submitted doc
            // became the last_chk tip, so this doc is SUPERSEDED as the
            // tip.  Its ACK status is UNKNOWN from last_chk (acked-then-
            // superseded OR never acked); we HOLD, we do NOT conclude
            // (M1-02 parity with boot's `complete_probe_trace_tip_
            // superseded`, which holds rather than terminalises).  Complete
            // the recovery trace (RetryableServer / LAST_CHK_TIP_SUPERSEDED,
            // carrying the DPS tip id) + emit a TIP_SUPERSEDED audit; NO
            // doc-state change; NO consecutive_holds increment.  Return
            // `SupersededHeld` so the drain CONTINUES past this doc
            // (caller maps to a sibling-continue verdict) — do NOT
            // BootError-halt the FN drain.
            if !matches!(source, Kvt2ConfirmSource::SentReplay) {
                return Err(BootError::Internal(format!(
                    "confirm_drain_doc({source:?}): SupersededHold is structurally \
                     unreachable for non-SentReplay — `superseded` is computed true \
                     only on the SentReplay path (SEAM-B-3 verdict).  If this surfaces, \
                     classifier routing has regressed."
                )));
            }
            let trace_attempt_no = sent_replay_trace_attempt_no
                .expect("SentReplay implies 1c-pre allocated trace_attempt_no");
            let wire_started = sent_replay_wire_started
                .clone()
                .expect("SentReplay implies wire_started captured");
            let wire_finished = sent_replay_wire_finished
                .clone()
                .expect("SentReplay implies wire_finished captured");
            let max_submitted_lnd = superseded_max_lnd
                .expect("SupersededHold implies superseded verdict computed Some(max)");
            commit_sent_replay_envelope_1c_superseded(
                pool,
                &fiscal_number,
                doc_id,
                &id_hex,
                trace_attempt_no,
                wire_started,
                wire_finished,
                &dps_tip_id,
                doc.lnd,
                max_submitted_lnd,
            )
            .await?;
            Ok(ConfirmDrainOutcome::SupersededHeld)
        }
    }
}

/// **RS-3 A2.1a (2026-06-09, plan §A2.1a)** — map the runtime-neutral
/// confirmer's [`kvt2_advance::ConfirmError`] back to the drain's
/// [`BootError`] boundary, preserving the EXACT pre-extraction drain
/// semantics:
///   - `StructuralDrift` → [`BootError::Internal`] — fail-loud halt, as
///     the inline empty-`kvt1_raw_bytes` guard / `stage_finalize`
///     `StateConflict` | `DocumentMissing` arms produced before A2.1a.
///   - `Database` / `Infrastructure` → [`BootError::ReconciliationFailed`]
///     — the Envelope 1a/1b commit-failure (incl. CAS-miss) and
///     `stage_finalize::run` typed-error surfaces, both per-FN-attributed
///     recon-failed exactly as the inline code routed them.
///
/// (A2.1b will map the same `ConfirmError` to `FiscalError` at the online
/// worker's boundary instead.)
fn map_confirm_error(err: kvt2_advance::ConfirmError, fiscal_number: &str) -> BootError {
    match err {
        kvt2_advance::ConfirmError::StructuralDrift { detail } => BootError::Internal(detail),
        kvt2_advance::ConfirmError::Database { source }
        | kvt2_advance::ConfirmError::Infrastructure { source } => {
            BootError::ReconciliationFailed {
                fiscal_number: fiscal_number.to_string(),
                source,
            }
        }
    }
}

/// **M3b W12 Commit 4b.1 (plan §311 + §449, 2026-05-22)** —
/// Envelope 1c-hold-light: audit-only `with_immediate` emitting
/// `KVT2_CONFIRM_HOLD` (Severity::Warning) for SentFresh and
/// Kvt1Reentry Hold contexts.  Doc state UNCHANGED per W0b §97-102.
///
/// SentReplay context uses its own bundled Envelope 1c-hold (trace
/// completion + audit) — Commit 5b will land it.  This light variant
/// is for the two source contexts that do not allocate a recovery
/// trace row.
///
/// **Helper-heavy ownership** (plan §311 MED-PR70-R12-01 fix): caller
/// receives `Kvt2ConfirmOutcome::Hold` AFTER the audit row commits.
/// Forensic trail lands regardless of subsequent caller projection
/// (in 4b.1 helper still returns `BootError::Internal` defensive
/// marker until Commit 6 lands `HoldFnDrain` projection).
///
/// **Commit 4b status**: production-consumed via `confirm_drain_doc`
/// Hold arm; emits forensic KVT2_CONFIRM_HOLD audit BEFORE caller
/// observes `DocVerdict::HoldFnDrain { HeldAtSent | HeldAtKvt1 }`.
///
/// **Commit 6.1.2 (2026-05-24) — REC-1 Tier wiring**: now also
/// increments `fiscal_documents.consecutive_holds` atomically inside
/// the same `with_immediate` as the audit emission.  Returns the
/// post-increment counter value (i64) for caller-side Tier 1/2
/// trigger evaluation in drain orchestrator.
async fn commit_hold_envelope_1c_hold_light(
    pool: &SqlitePool,
    fiscal_number: &str,
    doc_id: DocumentId,
    id_hex: &str,
    source: Kvt2ConfirmSource,
    reason: &Kvt2ConfirmHoldReason,
) -> Result<i64, BootError> {
    // **LOW-W12C4B-03 fix (4b.3 Δ2, 2026-05-22)**: runtime guard
    // (not debug-only) — when Commits 5/5b/6 lift the SentFresh-only
    // scope guard at `confirm_drain_doc` entry, a routing bug that
    // sends SentReplay outcome through this light helper would
    // silently skip the recovery `transport_trace.complete_tx` row →
    // forensic gap.  Runtime `BootError::Internal` catches the
    // regression in release builds (not just dev/CI), and the audit
    // row is NOT emitted (which is correct — the bundled 1c-hold
    // envelope is the authoritative emission site for SentReplay).
    if matches!(source, Kvt2ConfirmSource::SentReplay) {
        return Err(BootError::Internal(format!(
            "commit_hold_envelope_1c_hold_light: SentReplay routed to \
             light variant for doc {id_hex} — SentReplay MUST use bundled \
             Envelope 1c-hold (trace.complete + audit, Commit 5b scope).  \
             Refusing to emit audit without trace.complete to avoid \
             forensic gap."
        )));
    }
    let payload = serde_json::json!({
        "document_id": id_hex,
        "source": source.audit_label(),
        "hold_reason": reason.audit_label(),
        "hold_reason_detail": format!("{reason:?}"),
        "dispatch_via": "kvt2_confirm",
    });
    let payload_owned = payload.to_string();
    let id_hex_owned = id_hex.to_string();
    let new_counter: i64 = with_immediate(pool, move |tx| {
        Box::pin(async move {
            // (a) **REC-1 Phase 2a.1 Commit 6.1.2 (2026-05-24)**:
            // increment consecutive_holds counter; new value drives
            // caller-side Tier 1 (>= 10) / Tier 2 (>= 50) triggers.
            let counter = fiscal_documents::increment_consecutive_holds_tx(tx, doc_id).await?;
            // (b) Forensic audit row.
            audit_log::append_tx(
                tx,
                AUDIT_ENTITY_DOC,
                &id_hex_owned,
                "KVT2_CONFIRM_HOLD",
                Severity::Warning,
                None,
                Some(&payload_owned),
            )
            .await?;
            Ok::<i64, anyhow::Error>(counter)
        })
    })
    .await
    .map_err(|err| BootError::ReconciliationFailed {
        fiscal_number: fiscal_number.to_string(),
        source: err,
    })?;
    Ok(new_counter)
}

/// **M3b W12 Commit 4b.1 (plan §311, 2026-05-22)** — Envelope
/// 1c-drift-light: audit-only `with_immediate` emitting
/// `KVT2_CONFIRM_STRUCTURAL_DRIFT` (Severity::Error) for SentFresh and
/// Kvt1Reentry StructuralDrift contexts.  Doc state UNCHANGED;
/// forensic trail preserved BEFORE caller's `BootError::Internal`
/// fail-loud propagation halts the FN drain.
///
/// SentReplay context uses its own bundled Envelope 1c-drift (trace
/// completion + audit) — Commit 5b will land it.  This light variant
/// is for the two source contexts that do not allocate a recovery
/// trace row.
///
/// `Severity::Error` (not Warning like Hold) because StructuralDrift
/// signals state-machine invariant breach that halts FN drain — more
/// severe than recoverable Hold.
///
/// **Commit 4b status**: production-consumed via `confirm_drain_doc`
/// StructuralDrift arm; emits forensic KVT2_CONFIRM_STRUCTURAL_DRIFT
/// audit BEFORE caller propagates `BootError::Internal` fail-loud.
pub(in crate::services::offline_sync) async fn commit_drift_envelope_1c_drift_light(
    pool: &SqlitePool,
    fiscal_number: &str,
    id_hex: &str,
    source: Kvt2ConfirmSource,
    reason: &Kvt2ConfirmStructuralReason,
) -> Result<(), BootError> {
    // **LOW-W12C4B-03 fix (4b.3 Δ2, 2026-05-22)**: runtime guard
    // (not debug-only) — same rationale as
    // `commit_hold_envelope_1c_hold_light`.  When Commits 5/5b/6 lift
    // the SentFresh-only scope guard at `confirm_drain_doc` entry, a
    // routing bug that sends SentReplay outcome through this light
    // helper would silently skip the recovery `transport_trace.
    // complete_tx` row.  Runtime `BootError::Internal` catches the
    // regression in release builds; audit row NOT emitted (bundled
    // 1c-drift envelope is authoritative emission site for SentReplay).
    if matches!(source, Kvt2ConfirmSource::SentReplay) {
        return Err(BootError::Internal(format!(
            "commit_drift_envelope_1c_drift_light: SentReplay routed to \
             light variant for doc {id_hex} — SentReplay MUST use bundled \
             Envelope 1c-drift (trace.complete + audit, Commit 5b scope).  \
             Refusing to emit audit without trace.complete to avoid \
             forensic gap."
        )));
    }
    let payload = serde_json::json!({
        "document_id": id_hex,
        "source": source.audit_label(),
        "drift_reason": reason.audit_label(),
        // **OBS-W12C5-1 fix (5 Δ2, 2026-05-22)**: per-variant
        // `detail_message()` replaces flat `Debug` format —
        // unit variants now carry contextual description
        // (ServerFiscalNoMissing) instead of duplicating
        // `drift_reason` field; data-bearing variants
        // (LastChkIdMismatch / CasMissOnAdvance / NotFoundOutside
        // SentReplay) fall back to Debug for observed/expected
        // operator-triage signal preserved.
        "drift_reason_detail": reason.detail_message(),
        "dispatch_via": "kvt2_confirm",
    });
    let payload_owned = payload.to_string();
    let id_hex_owned = id_hex.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            audit_log::append_tx(
                tx,
                AUDIT_ENTITY_DOC,
                &id_hex_owned,
                "KVT2_CONFIRM_STRUCTURAL_DRIFT",
                Severity::Error,
                None,
                Some(&payload_owned),
            )
            .await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await
    .map_err(|err| BootError::ReconciliationFailed {
        fiscal_number: fiscal_number.to_string(),
        source: err,
    })?;
    Ok(())
}

// ─── M3b W12 Commit 5b.1: SentReplay bundled envelopes ──────────────
//
// Plan §412 + §316-319 helper-heavy ownership: SentReplay context
// allocates a `transport_trace` recovery row in **Envelope 1c-pre**
// BEFORE the DPS call (so wire timing + outcome are bundled into a
// single forensic record), then commits one of 4 outcome-specific
// envelopes (each bundling `transport_trace::complete_tx` + outcome-
// specific writes + audit atomically):
//
//   - 1a-replay (Acked): trace.complete OK + Kvt1Raw + Sent→Kvt1 +
//     Kvt1→Kvt2 + audit (5 writes).
//   - 1c-post (NotFound): trace.complete TransientRetry + Sent→ER +
//     audit (3 writes).  HIGH-C5-3 safe-redrive seam.
//   - 1c-hold (Hold): trace.complete Retryable* + audit (2 writes).
//   - 1c-drift (StructuralDrift): trace.complete structural + audit
//     (2 writes).
//
// Differs from SentFresh / Kvt1Reentry **-light** variants
// (`commit_hold_envelope_1c_hold_light` / `commit_drift_envelope_1c
// _drift_light`) which do NOT have a trace row to complete (no
// 1c-pre allocation for those source contexts per plan §311).

/// **M3b W12 Commit 5b.1 (plan §412, 2026-05-22)** — Local ISO-8601
/// UTC timestamp renderer for `transport_trace` `wire_call_*` fields.
/// Duplicated від `boot_phase::iso8601_now` per "boot recovery is the
/// only ctx-needy reader" pattern + now W12 SentReplay is the second
/// reader.  Format matches W7 stage_send wire-time convention.
fn iso8601_now() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// **M3b W12 Post-Closure Hardening Phase 2b Commit 6.2 / REC-6
/// (2026-05-24)** — granular `FailureClass` mapping per `Kvt2ConfirmHoldReason`.
///
/// Replaces Commit 6.1.2 hardcoded `FailureClass::Transport` для всіх
/// Hold-class outcomes.  Operator dashboards тепер можуть distinguish:
///   - `DpsTransport` → `Transport` — network / TLS / DNS glitch.
///   - `DpsServer` → `Server` — DPS returned non-OK status code.
///   - `DpsAuthorization` → `Authorization` — DPS auth/config failure.
///   - `DpsDecode` → `Decode` — DPS response decode failed (contract
///     drift / malformed protobuf).
///   - `LastChkDataSignEmpty` → `Internal` — structural breach (DPS
///     matched id but returned empty data_sign; internal contract
///     violation, NOT server-side error).
///
/// Note: `SentNotFoundDowngrade` outcome (NotFound from DPS) is
/// dispatched separately в `confirm_drain_doc` з hardcoded
/// `FailureClass::NotFound` (NOT через this helper — distinct
/// outcome variant, distinct semantic).  Rationale: NotFound is
/// application-level negative result with healthy DPS — must NOT
/// trigger auto-offline false-positive per memory
/// `feedback_offline_transition_strategy`.
fn map_hold_reason_to_class(reason: &Kvt2ConfirmHoldReason) -> FailureClass {
    match reason {
        Kvt2ConfirmHoldReason::DpsTransport(_) => FailureClass::Transport,
        Kvt2ConfirmHoldReason::DpsServer(_) => FailureClass::Server,
        Kvt2ConfirmHoldReason::DpsAuthorization(_) => FailureClass::Authorization,
        Kvt2ConfirmHoldReason::DpsDecode(_) => FailureClass::Decode,
        Kvt2ConfirmHoldReason::LastChkDataSignEmpty => FailureClass::Internal,
    }
}

/// **M3b W12 Commit 5b.1 (plan §412 Envelope 1c-pre, 2026-05-22)** —
/// allocate `transport_trace` recovery row for SentReplay context
/// BEFORE the DPS lastChk call.  Single `with_immediate` envelope.
/// Returns the allocated `attempt_no` for threading into
/// outcome-specific completion envelopes.
///
/// `request_envelope_sha256 = [0u8; 32]` placeholder per
/// `boot_phase.rs:1521-1542` precedent: lastChk is a query, not a
/// wire submit; there is no envelope payload to hash.
///
/// **Commit 5b.1 status**: consumed by `confirm_drain_doc` SentReplay
/// branch (also added in 5b.1).  No production caller invokes the
/// SentReplay branch until 5b.2 wires `process_via_lastchk_replay`,
/// but the static call graph reaches this helper through the match
/// arm so dead_code does not fire.
async fn commit_sent_replay_envelope_1c_pre(
    pool: &SqlitePool,
    fiscal_number: &str,
    doc_id: DocumentId,
    backend_profile_id: String,
    transport_profile_id: String,
) -> Result<i32, BootError> {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            transport_trace::allocate_and_insert_tx(
                tx,
                doc_id,
                transport_trace::NewAttempt {
                    backend_profile_id,
                    transport_profile_id,
                    request_envelope_sha256: [0u8; 32],
                    // M1 item 4: a READ-ONLY KVT2-confirm last_chk probe —
                    // excluded from the ER-redrive send budget.
                    is_probe: true,
                },
            )
            .await
            .map_err(anyhow::Error::from)
        })
    })
    .await
    .map_err(|err| BootError::ReconciliationFailed {
        fiscal_number: fiscal_number.to_string(),
        source: err,
    })
}

/// **M3b W12 Commit 5b.1 (plan §412 Envelope 1a-replay, 2026-05-22)**
/// — SentReplay Acked path: 5-write atomic envelope bundling
/// `transport_trace::complete_tx` OK + Kvt1Raw persist + `Sent →
/// Kvt1` CAS + `Kvt1 → Kvt2` CAS + `OFFLINE_DRAIN_KVT2_ADVANCED`
/// audit з `replay_short_circuit=true` marker.
///
/// **Difference from SentFresh Envelope 1a**: also bundles
/// `transport_trace::complete_tx` для recovery trace row allocated
/// by Envelope 1c-pre.  `replay_short_circuit=true` payload field
/// distinguishes SentReplay-advance from SentFresh-advance for
/// operator dashboards.
///
/// `attempt_no` (caller's `sent_replay_trace_attempt_no` from
/// 1c-pre allocation) feeds `transport_trace.complete_tx` —
/// completes the row at the exact attempt allocated pre-DPS call.
#[allow(clippy::too_many_arguments)] // 9 args — symmetric з SentFresh 1a
async fn commit_sent_replay_envelope_1a_replay(
    pool: &SqlitePool,
    fiscal_number: &str,
    doc_id: DocumentId,
    id_hex: &str,
    kvt1_raw_bytes: Vec<u8>,
    server_fiscal_no: &str,
    doc_state_for_audit: DocState,
    trace_attempt_no: i32,
    wire_started: String,
    wire_finished: String,
) -> Result<(), BootError> {
    let kvt1_raw_sha256_hex = format!("{:x}", Sha256::digest(&kvt1_raw_bytes));
    let payload = serde_json::json!({
        "document_id": id_hex,
        "from_state": doc_state_for_audit.as_str(),
        "to_state": DocState::Kvt2.as_str(),
        "server_fiscal_no": server_fiscal_no,
        "dispatch_via": "kvt2_confirm",
        "evidence_source": "lastChk",
        "kvt1_raw_sha256_hex": kvt1_raw_sha256_hex,
        "replay_short_circuit": true,
        "trace_attempt_no": trace_attempt_no,
    });
    let payload_owned = payload.to_string();
    let id_hex_owned = id_hex.to_string();
    let fn_owned = fiscal_number.to_string();
    let server_fiscal_no_owned = server_fiscal_no.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            // (a) Complete recovery trace row з OK outcome.
            let trace_rows = transport_trace::complete_tx(
                tx,
                doc_id,
                trace_attempt_no,
                transport_trace::AttemptCompletion {
                    wire_call_started_at: wire_started,
                    wire_call_finished_at: wire_finished,
                    outcome_kind: transport_trace::OutcomeKind::Ok,
                    server_fiscal_no: Some(server_fiscal_no_owned),
                    server_status_code: None,
                    error_kind: None,
                    error_message: None,
                    retry_class: None,
                },
            )
            .await?;
            if trace_rows != 1 {
                return Err(anyhow::anyhow!(
                    "backlog_drain({fn_id}): Envelope 1a-replay trace.complete \
                     produced rows_affected={trace_rows} for doc {doc_hex} \
                     attempt_no={trace_attempt_no} — append-then-complete \
                     invariant breach (row missing OR already complete)",
                    fn_id = fn_owned,
                    doc_hex = id_hex_owned,
                ));
            }
            // (b) Persist Kvt1Raw byte-for-byte.
            document_files::replace_tx(
                tx,
                doc_id,
                document_files::DocumentFileKind::Kvt1Raw,
                &kvt1_raw_bytes,
            )
            .await?;
            // (c) CAS Sent → Kvt1.
            let sent_to_kvt1 =
                fiscal_documents::transition_state(tx, doc_id, DocState::Sent, DocState::Kvt1)
                    .await?;
            if sent_to_kvt1 != TransitionOutcome::Applied {
                return Err(anyhow::anyhow!(
                    "backlog_drain({fn_id}): Envelope 1a-replay CAS Sent→Kvt1 \
                     produced {outcome:?} for doc {doc_hex}",
                    fn_id = fn_owned,
                    outcome = sent_to_kvt1,
                    doc_hex = id_hex_owned,
                ));
            }
            // (d) CAS Kvt1 → Kvt2.
            let kvt1_to_kvt2 =
                fiscal_documents::transition_state(tx, doc_id, DocState::Kvt1, DocState::Kvt2)
                    .await?;
            if kvt1_to_kvt2 != TransitionOutcome::Applied {
                return Err(anyhow::anyhow!(
                    "backlog_drain({fn_id}): Envelope 1a-replay CAS Kvt1→Kvt2 \
                     produced {outcome:?} for doc {doc_hex}",
                    fn_id = fn_owned,
                    outcome = kvt1_to_kvt2,
                    doc_hex = id_hex_owned,
                ));
            }
            // (e) **REC-1 Phase 2a.1 (2026-05-24)**: reset consecutive_
            // holds counter — SentReplay advance clears any prior
            // Hold accumulation per Tier reset semantics.
            fiscal_documents::reset_consecutive_holds_tx(tx, doc_id).await?;
            // (f) Forensic audit row.
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
    .map_err(|err| BootError::ReconciliationFailed {
        fiscal_number: fiscal_number.to_string(),
        source: err,
    })?;
    Ok(())
}

/// **M3b W12 Commit 5b.1 (plan §412 Envelope 1c-post, 2026-05-22)**
/// — SentReplay NotFound path: 3-write atomic envelope bundling
/// `transport_trace::complete_tx` TransientRetry + `Sent →
/// ErrorRetryable` CAS + `OFFLINE_DRAIN_DOC_FAILED` audit (failure_
/// class="transport", manual_recon_class=false).  HIGH-C5-3 safe-
/// redrive seam: doc lands in ER cohort for next-tick W9b ER-class-
/// guard bounded Pattern B redrive via `stage_send::run`.
///
/// `retry_class=TransientRetry` enables next-tick `ErRedriveDecision::
/// Redrive` per W9b guard contract (durable last-attempt retry_class
/// must be TransientRetry + attempts_used < MAX_BOOT_ATTEMPTS).
#[allow(clippy::too_many_arguments)] // 8 args — bundled envelope shape
async fn commit_sent_replay_envelope_1c_post(
    pool: &SqlitePool,
    fiscal_number: &str,
    doc_id: DocumentId,
    id_hex: &str,
    trace_attempt_no: i32,
    wire_started: String,
    wire_finished: String,
    server_fiscal_no: &str,
) -> Result<(), BootError> {
    let payload = serde_json::json!({
        "document_id": id_hex,
        "failure_class": "transport",
        "probe_outcome": "NotFound",
        "expected_server_fiscal_no": server_fiscal_no,
        "manual_recon_class": false,
        "dispatch_via": "kvt2_confirm",
        "trace_attempt_no": trace_attempt_no,
        "downgrade_to": DocState::ErrorRetryable.as_str(),
    });
    let payload_owned = payload.to_string();
    let id_hex_owned = id_hex.to_string();
    let fn_owned = fiscal_number.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            // (a) Complete recovery trace row з TransientRetry
            // outcome — enables W9b ER guard next-tick redrive.
            let trace_rows = transport_trace::complete_tx(
                tx,
                doc_id,
                trace_attempt_no,
                transport_trace::AttemptCompletion {
                    wire_call_started_at: wire_started,
                    wire_call_finished_at: wire_finished,
                    outcome_kind: transport_trace::OutcomeKind::RetryableTransport,
                    server_fiscal_no: None,
                    server_status_code: None,
                    error_kind: Some("LASTCHK_NOT_FOUND".to_string()),
                    error_message: Some(
                        "DPS lastChk returned empty id (NotFound); \
                         doc downgraded to ErrorRetryable for next-tick \
                         Pattern B redrive (HIGH-C5-3 safe-redrive)"
                            .to_string(),
                    ),
                    retry_class: Some("TransientRetry".to_string()),
                },
            )
            .await?;
            if trace_rows != 1 {
                return Err(anyhow::anyhow!(
                    "backlog_drain({fn_id}): Envelope 1c-post trace.complete \
                     produced rows_affected={trace_rows} for doc {doc_hex} \
                     attempt_no={trace_attempt_no}",
                    fn_id = fn_owned,
                    doc_hex = id_hex_owned,
                ));
            }
            // (b) CAS Sent → ErrorRetryable.
            let sent_to_er = fiscal_documents::transition_state(
                tx,
                doc_id,
                DocState::Sent,
                DocState::ErrorRetryable,
            )
            .await?;
            if sent_to_er != TransitionOutcome::Applied {
                return Err(anyhow::anyhow!(
                    "backlog_drain({fn_id}): Envelope 1c-post CAS \
                     Sent→ErrorRetryable produced {outcome:?} for doc {doc_hex}",
                    fn_id = fn_owned,
                    outcome = sent_to_er,
                    doc_hex = id_hex_owned,
                ));
            }
            // (c) **REC-1 Phase 2a.1 (2026-05-24)**: reset consecutive_
            // holds counter — SentNotFoundDowngrade Sent→ER advance
            // clears any prior Hold accumulation per Tier reset semantics
            // (doc now in ER cohort awaiting W9b Pattern B redrive next
            // tick; previous Hold history irrelevant to new attempt budget).
            fiscal_documents::reset_consecutive_holds_tx(tx, doc_id).await?;
            // (d) Forensic audit row.
            audit_log::append_tx(
                tx,
                AUDIT_ENTITY_DOC,
                &id_hex_owned,
                "OFFLINE_DRAIN_DOC_FAILED",
                Severity::Warning,
                None,
                Some(&payload_owned),
            )
            .await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await
    .map_err(|err| BootError::ReconciliationFailed {
        fiscal_number: fiscal_number.to_string(),
        source: err,
    })?;
    Ok(())
}

/// **M3b W12 Commit 5b.1 (plan §412 Envelope 1c-hold, 2026-05-22)** —
/// SentReplay Hold path: 2-write atomic envelope bundling
/// `transport_trace::complete_tx` Retryable* + `KVT2_CONFIRM_HOLD`
/// audit.  Doc state UNCHANGED (Sent) per W0b §97-102.
///
/// `outcome_kind` mapped per Hold reason: DpsTransport →
/// RetryableTransport; DpsServer/DpsAuthorization/DpsDecode/
/// LastChkDataSignEmpty → RetryableServer.  `retry_class` is NOT
/// set because doc state stays Sent (no ER cohort entry); next-tick
/// SentReplay re-allocates a fresh recovery row.
///
/// **Commit 6.1.2 (2026-05-24) — REC-1 Tier wiring**: now also
/// increments `fiscal_documents.consecutive_holds` atomically inside
/// the same `with_immediate` as trace.complete + audit (3-write
/// bundled envelope).  Returns the post-increment counter value (i64)
/// for caller-side Tier 1/2 trigger evaluation in drain orchestrator.
#[allow(clippy::too_many_arguments)] // 8 args — bundled envelope shape
async fn commit_sent_replay_envelope_1c_hold(
    pool: &SqlitePool,
    fiscal_number: &str,
    doc_id: DocumentId,
    id_hex: &str,
    trace_attempt_no: i32,
    wire_started: String,
    wire_finished: String,
    reason: &Kvt2ConfirmHoldReason,
) -> Result<i64, BootError> {
    // Hold reason → trace outcome_kind mapping per plan §200-205:
    let outcome_kind = match reason {
        Kvt2ConfirmHoldReason::DpsTransport(_) => transport_trace::OutcomeKind::RetryableTransport,
        Kvt2ConfirmHoldReason::DpsServer(_)
        | Kvt2ConfirmHoldReason::DpsAuthorization(_)
        | Kvt2ConfirmHoldReason::DpsDecode(_)
        | Kvt2ConfirmHoldReason::LastChkDataSignEmpty => {
            transport_trace::OutcomeKind::RetryableServer
        }
    };
    let payload = serde_json::json!({
        "document_id": id_hex,
        "source": Kvt2ConfirmSource::SentReplay.audit_label(),
        "hold_reason": reason.audit_label(),
        "hold_reason_detail": format!("{reason:?}"),
        "dispatch_via": "kvt2_confirm",
        "trace_attempt_no": trace_attempt_no,
    });
    let payload_owned = payload.to_string();
    let id_hex_owned = id_hex.to_string();
    let fn_owned = fiscal_number.to_string();
    let error_label = reason.audit_label().to_string();
    let error_detail = format!("{reason:?}");
    let new_counter: i64 = with_immediate(pool, move |tx| {
        Box::pin(async move {
            // (a) Complete recovery trace row з Retryable* outcome.
            let trace_rows = transport_trace::complete_tx(
                tx,
                doc_id,
                trace_attempt_no,
                transport_trace::AttemptCompletion {
                    wire_call_started_at: wire_started,
                    wire_call_finished_at: wire_finished,
                    outcome_kind,
                    server_fiscal_no: None,
                    server_status_code: None,
                    error_kind: Some(error_label),
                    error_message: Some(error_detail),
                    // retry_class None: doc stays Sent (not ER); next-
                    // tick SentReplay re-allocates fresh trace row.
                    retry_class: None,
                },
            )
            .await?;
            if trace_rows != 1 {
                return Err(anyhow::anyhow!(
                    "backlog_drain({fn_id}): Envelope 1c-hold trace.complete \
                     produced rows_affected={trace_rows} for doc {doc_hex} \
                     attempt_no={trace_attempt_no}",
                    fn_id = fn_owned,
                    doc_hex = id_hex_owned,
                ));
            }
            // (b) **REC-1 Phase 2a.1 Commit 6.1.2 (2026-05-24)**:
            // increment consecutive_holds counter; new value drives
            // caller-side Tier 1 (>= 10) / Tier 2 (>= 50) triggers.
            let counter = fiscal_documents::increment_consecutive_holds_tx(tx, doc_id).await?;
            // (c) Forensic audit row.
            audit_log::append_tx(
                tx,
                AUDIT_ENTITY_DOC,
                &id_hex_owned,
                "KVT2_CONFIRM_HOLD",
                Severity::Warning,
                None,
                Some(&payload_owned),
            )
            .await?;
            Ok::<i64, anyhow::Error>(counter)
        })
    })
    .await
    .map_err(|err| BootError::ReconciliationFailed {
        fiscal_number: fiscal_number.to_string(),
        source: err,
    })?;
    Ok(new_counter)
}

/// **M3b W12 Commit 5b.1 (plan §412 Envelope 1c-drift, 2026-05-22)** —
/// SentReplay StructuralDrift path: 2-write atomic envelope bundling
/// `transport_trace::complete_tx` з outcome_kind=RetryableServer
/// (structural drift treated as recoverable from trace lifecycle
/// perspective — actual recovery via Manual / FN drain halt) +
/// `KVT2_CONFIRM_STRUCTURAL_DRIFT` audit (Severity::Error).
///
/// Per plan §412: SentReplay structurally drifts only via
/// `LastChkIdMismatch` (NotFoundOutsideSentReplay can NOT reach
/// SentReplay by definition of routing matrix).  Caller propagates
/// `BootError::Internal` after this envelope commits.
#[allow(clippy::too_many_arguments)] // 8 args — bundled envelope shape
async fn commit_sent_replay_envelope_1c_drift(
    pool: &SqlitePool,
    fiscal_number: &str,
    doc_id: DocumentId,
    id_hex: &str,
    trace_attempt_no: i32,
    wire_started: String,
    wire_finished: String,
    reason: &Kvt2ConfirmStructuralReason,
) -> Result<(), BootError> {
    let payload = serde_json::json!({
        "document_id": id_hex,
        "source": Kvt2ConfirmSource::SentReplay.audit_label(),
        "drift_reason": reason.audit_label(),
        "drift_reason_detail": reason.detail_message(),
        "dispatch_via": "kvt2_confirm",
        "trace_attempt_no": trace_attempt_no,
    });
    let payload_owned = payload.to_string();
    let id_hex_owned = id_hex.to_string();
    let fn_owned = fiscal_number.to_string();
    let error_label = format!("STRUCTURAL_DRIFT_{}", reason.audit_label());
    let error_detail = reason.detail_message();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            // (a) Complete recovery trace row з structural-drift
            // marker (RetryableServer outcome_kind — closest fit;
            // trace alone is not the recovery surface, audit row is
            // authoritative).
            let trace_rows = transport_trace::complete_tx(
                tx,
                doc_id,
                trace_attempt_no,
                transport_trace::AttemptCompletion {
                    wire_call_started_at: wire_started,
                    wire_call_finished_at: wire_finished,
                    outcome_kind: transport_trace::OutcomeKind::RetryableServer,
                    server_fiscal_no: None,
                    server_status_code: None,
                    error_kind: Some(error_label),
                    error_message: Some(error_detail),
                    retry_class: None,
                },
            )
            .await?;
            if trace_rows != 1 {
                return Err(anyhow::anyhow!(
                    "backlog_drain({fn_id}): Envelope 1c-drift trace.complete \
                     produced rows_affected={trace_rows} for doc {doc_hex} \
                     attempt_no={trace_attempt_no}",
                    fn_id = fn_owned,
                    doc_hex = id_hex_owned,
                ));
            }
            // (b) Forensic audit row.
            audit_log::append_tx(
                tx,
                AUDIT_ENTITY_DOC,
                &id_hex_owned,
                "KVT2_CONFIRM_STRUCTURAL_DRIFT",
                Severity::Error,
                None,
                Some(&payload_owned),
            )
            .await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await
    .map_err(|err| BootError::ReconciliationFailed {
        fiscal_number: fiscal_number.to_string(),
        source: err,
    })?;
    Ok(())
}

/// **SEAM-B-3 (architect-locked contract, 2026-06-13)** — SentReplay
/// superseded path: 2-write atomic envelope mirroring boot's
/// `complete_probe_trace_tip_superseded` for the offline-drain context.
/// Bundles `transport_trace::complete_tx` (`OutcomeKind::RetryableServer`,
/// carrying the DPS tip id + `LAST_CHK_TIP_SUPERSEDED` marker) + a
/// `TIP_SUPERSEDED` audit (`Severity::Warning`, boot-parity event/label).
/// NO doc-state CAS; NO `consecutive_holds` increment — the doc's ACK
/// status is UNKNOWN from last_chk (acked-then-superseded OR never acked),
/// so this is a HOLD (not a transient retry-hold, and NOT a conclusion).
/// The emitted `rationale` / `error_message` strings below are kept
/// verbatim from boot M1-02 for forensic parity across the boot + drain
/// `TIP_SUPERSEDED` surfaces (a maintainer should read the doc-comments,
/// not the legacy payload wording, as the authoritative semantics).
/// Caller returns `ConfirmDrainOutcome::SupersededHeld`; the drain
/// CONTINUES.
#[allow(clippy::too_many_arguments)] // 10 args — bundled envelope shape
async fn commit_sent_replay_envelope_1c_superseded(
    pool: &SqlitePool,
    fiscal_number: &str,
    doc_id: DocumentId,
    id_hex: &str,
    trace_attempt_no: i32,
    wire_started: String,
    wire_finished: String,
    dps_tip_id: &str,
    doc_lnd: i64,
    max_submitted_lnd: i64,
) -> Result<(), BootError> {
    let payload = serde_json::json!({
        "document_id": id_hex,
        "source": Kvt2ConfirmSource::SentReplay.audit_label(),
        "branch": "c-sent-mismatch-superseded",
        "dps_tip_id": dps_tip_id,
        "doc_lnd": doc_lnd,
        "max_submitted_lnd": max_submitted_lnd,
        "dispatch_via": "kvt2_confirm",
        "trace_attempt_no": trace_attempt_no,
        "rationale":
            "SENT doc's DPS ACK status is UNKNOWN from last_chk — a NEWER submitted doc is the \
             tip, so this doc may have been acked-then-superseded OR never acked; held in SENT, \
             NOT concluded; resolution deferred to B1-v2 doc-scoped confirmation (NOT structural \
             drift, NOT RequiresManualReconciliation)",
    });
    let payload_owned = payload.to_string();
    let id_hex_owned = id_hex.to_string();
    let fn_owned = fiscal_number.to_string();
    let tip_owned = dps_tip_id.to_string();
    let error_message = format!(
        "last_chk tip id={tip_owned} is a newer submitted doc (this doc lnd={doc_lnd} \
         < max submitted lnd={max_submitted_lnd}); this doc's DPS ACK status is UNKNOWN from \
         last_chk (may be acked-then-superseded OR never acked) — held in SENT, deferred to \
         B1-v2, NOT lost"
    );
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            // (a) Complete recovery trace row carrying the DPS tip id.
            let trace_rows = transport_trace::complete_tx(
                tx,
                doc_id,
                trace_attempt_no,
                transport_trace::AttemptCompletion {
                    wire_call_started_at: wire_started,
                    wire_call_finished_at: wire_finished,
                    outcome_kind: transport_trace::OutcomeKind::RetryableServer,
                    server_fiscal_no: Some(tip_owned),
                    server_status_code: None,
                    error_kind: Some("LAST_CHK_TIP_SUPERSEDED".to_string()),
                    error_message: Some(error_message),
                    // retry_class None: doc stays Sent (not ER); resolution
                    // is deferred to B1-v2 doc-scoped confirmation.
                    retry_class: None,
                },
            )
            .await?;
            if trace_rows != 1 {
                return Err(anyhow::anyhow!(
                    "backlog_drain({fn_id}): Envelope 1c-superseded trace.complete \
                     produced rows_affected={trace_rows} for doc {doc_hex} \
                     attempt_no={trace_attempt_no}",
                    fn_id = fn_owned,
                    doc_hex = id_hex_owned,
                ));
            }
            // (b) Forensic TIP_SUPERSEDED audit (boot-parity event/label).
            audit_log::append_tx(
                tx,
                AUDIT_ENTITY_DOC,
                &id_hex_owned,
                "TIP_SUPERSEDED",
                Severity::Warning,
                None,
                Some(&payload_owned),
            )
            .await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await
    .map_err(|err| BootError::ReconciliationFailed {
        fiscal_number: fiscal_number.to_string(),
        source: err,
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
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentFresh, None, false);
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
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentReplay, Some(7), false);
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
        let outcome = classify_check_result(result, Kvt2ConfirmSource::Kvt1Reentry, None, false);
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
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentReplay, Some(3), false);
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
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentFresh, None, false);
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
        let outcome = classify_check_result(result, Kvt2ConfirmSource::Kvt1Reentry, None, false);
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
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentReplay, Some(1), false);
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
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentFresh, None, false);
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
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentFresh, None, false);
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
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentFresh, None, false);
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
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentReplay, Some(9), false);
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
        let outcome = classify_check_result(result, Kvt2ConfirmSource::Kvt1Reentry, None, false);
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
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentReplay, Some(5), false);
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
        let outcome = classify_check_result(result, Kvt2ConfirmSource::SentFresh, None, false);
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
        let outcome = classify_check_result(result, Kvt2ConfirmSource::Kvt1Reentry, None, false);
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
        let _ = classify_check_result(result, Kvt2ConfirmSource::SentReplay, None, false);
    }

    // ─── REC-6 Phase 2b helper unit test ────────────────────────────

    /// **REC-6 unit test (Phase 2b Commit 6.2, 2026-05-24)** —
    /// `map_hold_reason_to_class` 5-branch coverage.  Direct
    /// verification без integration test overhead.
    #[test]
    fn map_hold_reason_to_class_covers_all_5_branches() {
        // DpsTransport → Transport (network/TLS/DNS glitch).
        assert_eq!(
            map_hold_reason_to_class(&Kvt2ConfirmHoldReason::DpsTransport("timeout".into())),
            FailureClass::Transport
        );
        // DpsServer → Server (DPS non-OK status code).
        assert_eq!(
            map_hold_reason_to_class(&Kvt2ConfirmHoldReason::DpsServer("status=-5".into())),
            FailureClass::Server
        );
        // DpsAuthorization → Authorization (DPS auth/config failure).
        assert_eq!(
            map_hold_reason_to_class(&Kvt2ConfirmHoldReason::DpsAuthorization(
                "DocumentReject(code=-1)".into()
            )),
            FailureClass::Authorization
        );
        // DpsDecode → Decode (response decode / contract drift).
        assert_eq!(
            map_hold_reason_to_class(&Kvt2ConfirmHoldReason::DpsDecode("malformed".into())),
            FailureClass::Decode
        );
        // LastChkDataSignEmpty → Internal (structural breach — DPS
        // matched id but empty data_sign payload).
        assert_eq!(
            map_hold_reason_to_class(&Kvt2ConfirmHoldReason::LastChkDataSignEmpty),
            FailureClass::Internal
        );
    }
}
