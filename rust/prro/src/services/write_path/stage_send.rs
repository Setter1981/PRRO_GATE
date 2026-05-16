//! Stage 4 — send (Pattern B with SENDING marker).
//!
//! W7.3 lands the pure-Rust pre-flight surface: typed errors,
//! `build_send_envelope`, and `transport_trace::OutcomeKind`-shaped
//! completion building.  The full 3-segment Pattern B worker step
//! (4-pre / 4a / 4b) lives in W7.4 in this same module.
//!
//! W10.2 wires `error_routing::route_send_result` into 4-a/4-b: the
//! W7 minimal `SendOutcome` / `classify_send_outcome` shim was dropped
//! wholesale and replaced with `WireDecision::{Sent, Routed(decision)}`
//! dispatch (freeze §3 + W0-3 §2 + §2.1).  4-pre source-state CAS is
//! extended from `Signed → Sending` to
//! `(Signed | ErrorRetryable | OfflineLocalAck) → Sending` per HIGH 3
//! §4.2 (Pattern B retry-path edge) + M3b W9a (Pattern C drain-path
//! edge: the W9 backlog drain replays offline-acked docs through this
//! same 4-pre/4a/4b ladder).
//!
//! # Caller obligation — retry-loop policy (R-W10.2-review HIGH 1)
//!
//! 4-pre CAS allowlist `(Signed | ErrorRetryable | OfflineLocalAck)
//! → Sending` makes [`run`] willing to re-attempt any doc in
//! `ErrorRetryable`, regardless of which `RetryClass` put it there;
//! `OfflineLocalAck` source is reserved for the W9 backlog drain
//! caller (M3b W9b).  The routing fn is
//! pure; the policy of "which retry classes warrant another wire
//! send" lives one layer up (worker dispatcher, W11+).  Callers MUST
//! respect this table:
//!
//! | RetryClass of last attempt | re-invoke `run` |
//! |---|---|
//! | `TransientRetry` (Transport / Server-3)          | YES |
//! | `FnConfigError` (-13/-14)                        | NO — operator |
//! | `WrapperBug`                                     | NO — code fix |
//! | `ProbeRequired` (Decode / -2/-15 close-shift)    | NO — W9 probe |
//! | `MacRecovery` (-12)                              | NO — W10.4 orchestrator |
//! | `OperatorEscalation` (-6)                        | NO — operator |
//!
//! Calling [`run`] repeatedly on a non-`TransientRetry` `ErrorRetryable`
//! doc would produce an unbounded crash-loop: same envelope, same
//! server reply, same `ErrorRetryable` landing.  **This hazard is
//! mitigated at the boot dispatcher layer** by
//! [`crate::services::reconciliation::boot_phase::dispatch_error_retryable_by_class`]
//! (M3a hardening pass 1, PR #38), which reads
//! [`crate::db::repositories::transport_trace::last_attempt_retry_class_for`]
//! (durable column added in migration 012; encoded by
//! `RetryClass::as_str`, decoded via `RetryClass::from_wire_str`) and
//! routes only `TransientRetry` docs back to [`run`].  Non-transient
//! classes are CAS'd to `RequiresManualReconciliation` instead of
//! re-invoking the wire send.  Additionally,
//! [`crate::db::repositories::transport_trace::attempts_used`] is
//! gated against `MAX_BOOT_ATTEMPTS = 5` (hardening pass 2, PR #40)
//! before the `TransientRetry` → wire arm is entered, so even valid
//! transient retries cannot loop indefinitely on a doc that keeps
//! failing every boot.
//!
//! **Caller obligation today:** if you bypass the boot dispatcher and
//! invoke [`run`] directly (e.g. ops scripts, ad-hoc test harness),
//! you MUST still respect the `RetryClass` table above — or replicate
//! the dispatcher's class/budget guard.  Production callers go
//! through `App::reconcile_pending_with`, which uses the dispatcher
//! and therefore inherits both guards.  See freeze §4.2.
//!
//! Anchored on:
//!   - W7 design freeze §4.3 (envelope builder)
//!   - W10 design freeze §3 (`route_send_result`, `RoutingDecision`,
//!     `AuditEvent`, `RetryClass`)
//!   - ADR-M3-A2 (Z-allocation by `wire_artifact_kind`)
//!   - ADR-M3-A6 (DpsError routing — full table)
//!   - ADR-M3-A9 step 5-6 (Pattern B retry-path: ErrorRetryable → Sending)
//!   - Sprint-7-proven Python contract `dps_fiscal_server.py:91-102, 190`
//!     for `DocType -> DpsCheckType` and the `SHIFT_OPEN -> local_number = 0`
//!     override.

use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
use chrono_tz::Europe::Kiev;
use prost::Message as _;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::db::models::enums::{DocState, DocType, Severity};
use crate::db::models::ids::DocumentId;
use crate::db::repositories::fiscal_documents::SendInputs;
use crate::db::repositories::transport_trace::OutcomeKind;
use crate::db::repositories::{
    audit_log, document_files,
    document_files::DocumentFileKind,
    fiscal_documents::{self as fd, TransitionOutcome},
    node_state,
    transport_trace::{self, AttemptCompletion, NewAttempt},
};
use crate::db::tx::with_immediate;
use crate::transports::dps::channel::DpsChannel;
use crate::transports::dps::dto::{CheckEnvelope, DpsCheckType};
use crate::transports::dps::error::DpsError;
use crate::transports::dps::gen;

use super::error_routing::{
    route_send_result, AuditEvent, RetryClass, RoutingDecision, WireDecision,
};
use super::mac_recovery::{self, MacRecoveryOutcome};
use super::stage_sign::{derive_wire_artifact_kind, SignError, SigningContext, WireArtifactKind};

// ─── Errors ──────────────────────────────────────────────────────────

/// Stage-4 typed error surface.  Three pre-flight variants (W7.3) +
/// six worker-step variants (W7.4) cover both pure-Rust fail-closed
/// conditions BEFORE any side effect AND the post-marker
/// state-invariant breaches that surface as caller-visible errors.
///
/// **Stage-progression vs error.**  `StateConflict` and
/// `DocumentMissing` are NOT errors; they are non-failure outcomes
/// returned via [`StageSendOutcome`].  `StageSendError` is reserved
/// for conditions that mean "stage 4 cannot complete as designed and
/// the caller (or W9 reconciliation) needs to know specifically why".
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum StageSendError {
    /// `derive_wire_artifact_kind` rejected the doc type.  W7 envelope
    /// builder is happy-path-only (SHIFT_OPEN/SELL/RETURN/SHIFT_CLOSE/
    /// Z_REPORT); SERVICE_IN/SERVICE_OUT/CASH_WITHDRAWAL/X_REPORT are
    /// not signable by W4 / not sendable by W7 and surface here BEFORE
    /// any side effect (CAS, trace, audit, wire).
    #[error("stage 4 unsupported doc type: {doc_type:?}")]
    UnsupportedDocType { doc_type: DocType },

    /// `inputs.lnd` exceeds `i32::MAX` and cannot fit
    /// `CheckEnvelope.local_number` (DPS proto type).  In practice
    /// unreachable (lnd grows from 1; even at 1k docs/day it takes
    /// ~5.8M years to hit i32::MAX), but kept as a typed fail-mode
    /// rather than a silent narrowing.
    #[error("stage 4 lnd out of i32 range: {lnd}")]
    LndOutOfRangeI32 { lnd: i64 },

    /// **M3b W9a (2026-05-16):** the doc is in `OfflineLocalAck` but
    /// `fiscal_documents.offline_fiscal_no` is NULL — a W7a invariant
    /// breach.  W7a's `transition_to_offline_local_ack_tx` writes
    /// `offline_fiscal_no = consumed code_lnd` atomically with the
    /// state flip; the only way to observe `OfflineLocalAck` with
    /// NULL `offline_fiscal_no` is a raw-SQL bypass or a future
    /// schema migration regression.  Surfaced BEFORE 4-pre CAS so
    /// no `Sending` marker is written without the data needed to
    /// populate `CheckEnvelope.id_offline` (DPS wire contract:
    /// `id_offline = offline_fiscal_no.to_string()`).
    #[error(
        "stage 4 W9 drain: doc {document_id:?} is in OfflineLocalAck but offline_fiscal_no is NULL \
         (W7a invariant breach: transition_to_offline_local_ack_tx writes both atomically)"
    )]
    OfflineFiscalNoMissing { document_id: DocumentId },

    /// **M3b W9a Round 2 LOW #1 fix (2026-05-16):** the doc is in
    /// `OfflineLocalAck` and `fiscal_documents.offline_fiscal_no` is
    /// present but non-positive (`<= 0`).  W7a writes
    /// `offline_fiscal_no = consumed code_lnd`, and `offline_codes`
    /// carries a schema CHECK `code_lnd > 0` — so the producer path
    /// guarantees a positive value.  `fiscal_documents.offline_fiscal_no`
    /// itself has no CHECK (`migrations/002_fiscal_documents.sql:25`),
    /// so a raw-SQL bypass or future schema regression could leak
    /// `0` / negative.  Surfaced BEFORE 4-pre CAS for the same
    /// reason as `OfflineFiscalNoMissing`: a `Sending` marker on a
    /// row whose `id_offline` would stringify to `"0"` would
    /// mis-identify the receipt to DPS.  Forensically split from
    /// `OfflineFiscalNoMissing` (NULL = column not written;
    /// `<= 0` = column written with invalid payload) so audit logs
    /// distinguish the two producer-side bug classes.
    #[error(
        "stage 4 W9 drain: doc {document_id:?} is in OfflineLocalAck but offline_fiscal_no is \
         non-positive ({observed}); offline_codes CHECK code_lnd > 0 forbids this on the W7a \
         producer path"
    )]
    OfflineFiscalNoNonPositive {
        document_id: DocumentId,
        observed: i64,
    },

    /// `inputs.business_ts` could not be parsed as UTC ISO-8601, or the
    /// Kyiv-local components could not be re-interpreted as UTC for
    /// the DPS Kyiv-local-as-epoch shape.
    #[error("stage 4 timestamp conversion: {detail}")]
    TimestampConversion { detail: String },

    /// `document_files::SignedXml` was not present for a doc whose
    /// fiscal_documents row is in `Signed` state.  State invariant
    /// breach: stage 3 must INSERT SIGNED_XML inside the same
    /// `with_immediate` envelope as the CAS `Prepared → Signed`.
    /// Surfacing as a typed error lets the dispatcher escalate
    /// (operator inspection / W9 forensics) rather than retry blindly.
    #[error("stage 4 SIGNED_XML missing for doc {document_id:?} despite SIGNED state")]
    SignedArtifactMissing { document_id: DocumentId },

    /// DPS responded `Ok(CheckAck)` but `CheckAck.id` is empty.  The
    /// transport_trace `OK ⇒ server_fiscal_no NOT NULL AND length > 0`
    /// CHECK (migration 010, W7.1 fix-up) would catch this at 4-b
    /// persist; this guard surfaces the malformed wire response BEFORE
    /// 4-b runs.  The doc remains in `Sending` and W9 reconciliation
    /// will move it to `ErrorRetryable` on the next boot.
    #[error("stage 4 DPS returned OK with empty CheckAck.id for doc {document_id:?}")]
    EmptyServerFiscalNo { document_id: DocumentId },

    /// 4-b post-wire CAS `Sending → {Sent|Rejected|ErrorRetryable}`
    /// returned a non-`Applied` outcome.  Impossible under M3a W5's
    /// single-writer-per-FN invariant (see ADR-M3-A10) + the 4-pre
    /// marker we just committed: the doc was in `Sending` after our
    /// 4-pre tx, no other writer can mutate it, so the post-wire CAS
    /// cannot miss.  Typed error for forensics if it ever happens.
    #[error("stage 4 post-wire CAS Sending->{target:?} on doc {document_id:?}: {observed:?}")]
    PostWireCasFailed {
        document_id: DocumentId,
        target: DocState,
        observed: TransitionOutcome,
    },

    /// `mark_submission_attempted_tx` returned `false` in 4-pre AFTER
    /// the CAS `Signed → Sending` succeeded.  Also impossible under
    /// the single-writer-per-FN invariant (see ADR-M3-A10): CAS
    /// Applied means the row exists for the duration of the same
    /// `with_immediate` envelope.
    #[error(
        "stage 4 mark_submission_attempted_tx returned 0 for doc {document_id:?} after CAS Applied"
    )]
    MarkSubmissionAttemptedMissing { document_id: DocumentId },

    /// W10.3 — `node_state::set_mode_blocked_tx` returned `false` in
    /// 4-b: the FN row is missing.  W5 acquire upserts the row before
    /// stage 1, so a missing FN at 4-b time is a structural breach
    /// (mirror of `StageFinalizeError::SeedUpdateMissing`).  Surfacing
    /// as typed error lets the dispatcher escalate (operator inspection
    /// / W9 forensics) rather than silently leave the FN unblocked
    /// after a -11 — which would let the next document hit the same
    /// 168-hour limit and burst the fleet of Rejected docs.
    #[error("stage 4 set_mode_blocked_tx returned 0 for fn {fn_id} after CAS Applied")]
    NodeStateMissingForBlock {
        fn_id: String,
        document_id: DocumentId,
    },

    /// `set_server_fiscal_no_tx` returned `false` in 4-b AFTER the
    /// CAS `Sending → Sent` succeeded.  Same invariant breach class
    /// as `PostWireCasFailed`.
    #[error(
        "stage 4 set_server_fiscal_no_tx returned 0 for doc {document_id:?} after CAS Applied"
    )]
    SetServerFiscalNoMissing { document_id: DocumentId },

    /// `transport_trace::complete_tx` returned `rows_affected == 0`
    /// in 4-b.  Per W7.1 docstring contract: the row was either
    /// missing (4-pre allocator bug — should have INSERTed it) or
    /// already complete (caller bug — `complete_tx` invoked twice
    /// for the same `(doc, attempt_no)`).  Both are non-recoverable
    /// mid-stage and surface as a typed error rather than silent
    /// success.
    #[error("stage 4 trace complete_tx returned 0 for doc {document_id:?} attempt {attempt_no}")]
    TraceMissingAtComplete {
        document_id: DocumentId,
        attempt_no: i32,
    },

    /// W10.4 step 2d — `stage_send::run` was invoked with
    /// `sign_ctx = None` but the wire `-12` decision requires invoking
    /// the MAC recovery orchestrator.  Production code paths MUST pass
    /// `Some(&SigningContext)` so this never fires in production; it
    /// surfaces in test contexts that exercise `-12` without a stub
    /// crypto provider, OR in any future caller that forgets the
    /// argument.  Surfacing as a typed error keeps recovery semantics
    /// auditable rather than silently treating `-12` as a normal
    /// `ErrorRetryable` doc.
    #[error("stage 4 MAC recovery dispatch needs SigningContext (doc {document_id:?})")]
    MacRecoveryContextMissing { document_id: DocumentId },

    /// W10.4 step 2c — MAC recovery orchestrator could not find the
    /// `(doc_id, kind)` artifact row in `document_files` during the
    /// MR-PERSIST Pre-PERSIST assertion.  W6 stage-3 invariant
    /// breach: PAYLOAD_XML + SIGNED_XML are INSERTed before stage 4
    /// runs, so a missing row at MR-PERSIST time means the doc is
    /// structurally broken.  Surfacing as a typed error rolls back
    /// the entire MR-PERSIST envelope before `replace_tx` (which
    /// would otherwise silently INSERT) gets invoked
    /// (R-W10.4-step2a-review LOW 3 close).
    #[error("MAC recovery artifact missing for doc {document_id:?}: {kind:?}")]
    MacRecoveryArtifactMissing {
        document_id: DocumentId,
        kind: crate::db::repositories::document_files::DocumentFileKind,
    },

    /// W10.4 step 2c — MAC recovery orchestrator could not find the
    /// `fiscal_documents` row when reading recovery inputs.  The doc
    /// must be present for stage 4 to have invoked recovery; absence
    /// indicates a race with delete (offline reconciliation, manual
    /// operator action) that the orchestrator surfaces typed for W9.
    #[error("MAC recovery: fiscal_documents row missing for doc {document_id:?}")]
    DocumentMissingForRecovery { document_id: DocumentId },

    /// W10.4 step 2c — MAC recovery orchestrator could not find the
    /// `fiscal_number_config` row for the doc's `fiscal_number`.  This
    /// would be a config-table inconsistency: the doc exists but the
    /// FN it points to has no config row.  Possible causes: operator
    /// removed the FN config while a doc was in flight, or a manual
    /// DB edit.  Surfacing typed lets W9 pick this up distinctly from
    /// the routine `DocumentMissingForRecovery`.
    #[error("MAC recovery: fn_config missing for fn {fn_id} (doc {document_id:?})")]
    FnConfigMissingForRecovery {
        fn_id: String,
        document_id: DocumentId,
    },

    /// W10.4 step 2c — MAC recovery orchestrator's MR-NO-TX re-sign
    /// step (`stage_sign::re_sign_after_mac_recovery`) failed.  Wraps
    /// the underlying [`crate::services::write_path::stage_sign::SignError`]
    /// so the caller routes failure forensically (typically:
    /// `PayloadSchema` / `TimestampConversion` / `Range` / `Crypto`
    /// from the re-sign helper).
    #[error("MAC recovery re-sign failed: {0}")]
    MacRecoverySignFailed(#[source] crate::services::write_path::stage_sign::SignError),

    /// Pass-through DB error from any helper.  Distinct variant so
    /// callers can route DB issues separately from state-invariant
    /// breaches.
    #[error("stage 4 db: {0}")]
    Db(#[source] sqlx::Error),

    /// Catch-all for non-sqlx, non-typed `anyhow::Error` chains
    /// surfacing from `with_immediate` closures.  Cause chain
    /// preserved.  Production callers should never see this; if they
    /// do, an upstream helper is leaking a non-typed error.
    #[error("stage 4 internal: {0}")]
    Internal(#[source] anyhow::Error),
}

// ─── Envelope builder ────────────────────────────────────────────────

/// Map `WireArtifactKind` (canonical inner taxonomy from W6) to the
/// DPS wire `check_type` enum.  Sprint-7-proven Python contract
/// (`dps_fiscal_server.py:91-102`):
///   - `SHIFT_OPEN` → `SERVICECHK`
///   - `SELL` / `RETURN` → `CHK`
///   - `Z_REPORT` → `ZREPORT` (also covers `SHIFT_CLOSE` after
///     `derive_wire_artifact_kind` boundary mapping)
fn wire_artifact_to_check_type(k: WireArtifactKind) -> DpsCheckType {
    match k {
        WireArtifactKind::ShiftOpen => DpsCheckType::ServiceChk,
        WireArtifactKind::Sell | WireArtifactKind::Return => DpsCheckType::Chk,
        WireArtifactKind::ZReport => DpsCheckType::ZReport,
    }
}

/// Build the wire-bound `CheckEnvelope` for stage 4-pre.  Pure
/// function: takes `SendInputs` (read in 4-pre BEFORE the CAS
/// `Signed → Sending`) and the SIGNED-XML CMS bytes (read from
/// `document_files`), returns either a ready-to-send envelope or a
/// typed fail-closed error.
///
/// **Fail-closed semantics.**  All three error variants
/// (`UnsupportedDocType`, `LndOutOfRangeI32`, `TimestampConversion`)
/// are observed BEFORE the 4-pre CAS in W7.4: the caller returns the
/// error directly without writing a `Sending` marker, without
/// allocating a `transport_trace` row, without an audit entry, and
/// crucially without invoking `send_chk`.
///
/// **`local_number` override.**  Sprint-7-proven Python contract
/// (`dps_fiscal_server.py:190`): `SHIFT_OPEN` always sends
/// `local_number = 0` regardless of `inputs.lnd`.  All other kinds
/// pass `inputs.lnd` through after a checked `i32::try_from`.
///
/// **`id_offline` / `id_cancel`.**  `id_offline` is set from
/// `inputs.offline_fiscal_no` (stringified) when present — this
/// covers the M3b W9 backlog drain replay path where the doc was
/// originally staged offline (W7a writes `offline_fiscal_no =
/// consumed code_lnd`).  For pure-online M3a docs `offline_fiscal_no`
/// is NULL and `id_offline` stays empty (DPS interprets empty as
/// "online", per the proven Sprint-7 Python contract
/// `dps_fiscal_server.py:196`).  `id_cancel` stays empty in W9a;
/// the cancel slice is future work.  The W7a invariant
/// "OfflineLocalAck implies offline_fiscal_no IS NOT NULL" is
/// enforced upstream of this call by `run_one_attempt`'s 4-pre
/// closure — a typed `StageSendError::OfflineFiscalNoMissing`
/// surfaces BEFORE any CAS or wire side effect.
pub fn build_send_envelope(
    inputs: &SendInputs,
    signed_payload: Vec<u8>,
) -> Result<CheckEnvelope, StageSendError> {
    let kind = derive_wire_artifact_kind(inputs.doc_type).map_err(|err| match err {
        SignError::UnsupportedDocType { doc_type } => {
            StageSendError::UnsupportedDocType { doc_type }
        }
        // `derive_wire_artifact_kind` only emits `UnsupportedDocType`
        // today; this arm is defensive against future additions to the
        // (`#[non_exhaustive]`) `SignError`.  Surfacing a typed
        // `UnsupportedDocType` keeps the fail-closed posture even if a
        // new variant is introduced without an explicit mapping here.
        _ => StageSendError::UnsupportedDocType {
            doc_type: inputs.doc_type,
        },
    })?;

    let check_type = wire_artifact_to_check_type(kind);

    let local_number: i32 = match kind {
        WireArtifactKind::ShiftOpen => 0,
        _ => i32::try_from(inputs.lnd)
            .map_err(|_| StageSendError::LndOutOfRangeI32 { lnd: inputs.lnd })?,
    };

    let date_time = kyiv_local_epoch(&inputs.business_ts)?;

    // M3b W9a (2026-05-16): `id_offline` carries the offline-acquired
    // fiscal-no for W9 backlog-drain replays.  W7a writes
    // `fiscal_documents.offline_fiscal_no = consumed code_lnd` at the
    // Signed → OfflineLocalAck transition; the wire contract
    // (docs/superpowers/specs/2026-05-04-m2-w0-1-dps-wire.md:116)
    // requires `id_offline = offline_fiscal_no.to_string()`.  For
    // M3a online docs this is `NULL` and the empty string maps to
    // DPS-interpreted "online" per the Sprint-7 Python contract.
    let id_offline = inputs
        .offline_fiscal_no
        .map(|n| n.to_string())
        .unwrap_or_default();

    Ok(CheckEnvelope {
        rro_fn: inputs.fiscal_number.clone(),
        date_time,
        check_sign: signed_payload,
        local_number,
        check_type,
        id_offline,
        id_cancel: String::new(),
    })
}

/// Convert UTC ISO-8601 `business_ts` to the DPS Kyiv-local-as-epoch
/// shape.  See `transports/dps/dto.rs:35-42` for the protocol-side
/// rationale.  Mirrors the Sprint-7-proven Python helper
/// `_kyiv_local_epoch` in `dps_fiscal_server.py:55-81` — chrono-tz
/// handles Europe/Kiev DST transitions; manual offset is a footgun.
fn kyiv_local_epoch(business_ts: &str) -> Result<i64, StageSendError> {
    let dt: DateTime<Utc> =
        business_ts
            .parse::<DateTime<Utc>>()
            .map_err(|e| StageSendError::TimestampConversion {
                detail: format!("parse {business_ts:?}: {e}"),
            })?;
    let kyiv = dt.with_timezone(&Kiev);
    // Re-interpret the Kyiv-local digits as if they were UTC — the
    // resulting epoch is the value DPS expects on the wire.
    let fake = Utc
        .with_ymd_and_hms(
            kyiv.year(),
            kyiv.month(),
            kyiv.day(),
            kyiv.hour(),
            kyiv.minute(),
            kyiv.second(),
        )
        .single()
        .ok_or_else(|| StageSendError::TimestampConversion {
            detail: format!(
                "ambiguous Kyiv-local components for {business_ts:?}: \
                 Y={} M={} D={} h={} m={} s={}",
                kyiv.year(),
                kyiv.month(),
                kyiv.day(),
                kyiv.hour(),
                kyiv.minute(),
                kyiv.second(),
            ),
        })?;
    Ok(fake.timestamp())
}

// ─── Stage outcome (worker dispatcher contract) ─────────────────────

/// Top-level outcome of [`run`].  Four variants cover the full Pattern
/// B stage 4 surface as observed by the worker dispatcher (W10.2):
///
///   - `Sent` — wire `send_chk` returned `Ok(CheckAck)` and 4-b CAS
///     `Sending → Sent` committed.  `attempt_no` correlates with the
///     `transport_trace` row.
///   - `Routed` — wire `send_chk` returned `Err(DpsError)` and 4-b CAS
///     `Sending → decision.target_state` committed.  `decision`
///     carries the full W10 routing surface (`retry_class`,
///     `audit_event`, `node_mode_flip`, `probe_hint`,
///     `mac_recovery_hint`); the worker dispatcher and W9
///     reconciliation read it to decide next-tick behaviour.
///   - `StateConflict` — 4-pre CAS
///     `(Signed | ErrorRetryable | OfflineLocalAck) → Sending`
///     missed: the doc was outside the allowlist (e.g. `Sent` from a
///     prior worker, or transitioned to a non-Signed state by
///     reconciliation).  Stage 4 did NOT call `send_chk`.  Idempotent
///     re-entry — the dispatcher should NOT treat this as failure.
///   - `DocumentMissing` — the `(doc_id)` row was not present at 4-pre
///     read.  Race with a delete (offline reconciliation, manual
///     operator action).  Stage 4 did NOT call `send_chk`.
///
/// **W7 → W10 collapse.**  W7 split the failure surface into
/// `Rejected { code, message }` + `Retryable { reason }`.  W10
/// collapses both into `Routed { decision }`: callers wanting the
/// status code / message read them via the routing decision +
/// the optional `wire_status_code` / `wire_error_message` fields
/// alongside `decision`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageSendOutcome {
    Sent {
        server_fiscal_no: String,
        attempt_no: i32,
    },
    Routed {
        decision: RoutingDecision,
        attempt_no: i32,
        /// Optional server status code from the wire reply
        /// (`Server { code, .. }` / `Authorization { code, .. }`).
        /// `None` for non-status-coded variants (Transport / Decode /
        /// NotFound / Internal / QueryNotSupported / ServerFiscalIdMismatch).
        wire_status_code: Option<i32>,
        /// Truncated wire error_message; useful for tests + W9
        /// forensics.  Truncation cap matches `transport_trace`
        /// CHECK (≤ 512 chars).  `None` only when the wire variant
        /// has no message at all.
        wire_error_message: Option<String>,
    },
    StateConflict {
        observed: DocState,
    },
    DocumentMissing,
}

// ─── Worker step (Pattern B 3-segment, 2 locks) ──────────────────────

/// Intermediate result of the 4-pre `with_immediate` envelope.
/// Distinguishes early-return paths (no CAS, no marker, no trace, no
/// audit, no wire) from the happy path that has already committed
/// the `SENDING` marker, allocated the trace row, and written the
/// `STAGE_SEND_INTENT_MARKED` audit entry.
enum PreOutcome {
    /// CAS `(Signed | ErrorRetryable | OfflineLocalAck) → Sending`
    /// applied; trace row allocated; audit written.  Wire send is the
    /// next step (4a, no lock).  `doc_type` is propagated out of the
    /// closure so 4-a can pass it to `route_send_result`;
    /// `fiscal_number` is propagated out so 4-b can call
    /// `node_state::set_mode_blocked_tx` for the W10.3 `-11` flip
    /// without a separate read.
    Marked {
        envelope: CheckEnvelope,
        attempt_no: i32,
        doc_type: DocType,
        fiscal_number: String,
    },
    /// `fetch_send_inputs_tx` returned `None` OR CAS returned
    /// `NotFound`.  No side effects.
    DocumentMissing,
    /// `document_files::SignedXml` was missing.  Surfaced as
    /// `StageSendError::SignedArtifactMissing` after the closure
    /// returns; we route via `PreOutcome` here so the closure body
    /// stays read-only on this branch.
    SignedArtifactMissing,
    /// `build_send_envelope` rejected the inputs (unsupported doc
    /// type / lnd overflow / business_ts parse).  Routed back as
    /// the typed error; no side effects.
    EnvelopeBuildFailed(StageSendError),
    /// W10.2: 4-pre source-state CAS rejected.  Either `inputs.state`
    /// was outside `{Signed, ErrorRetryable, OfflineLocalAck}`
    /// (we never attempt the CAS), OR the CAS `(observed_source) →
    /// Sending` returned `Conflict`.  No marker, no trace, no
    /// audit, no wire.  `OfflineLocalAck` joined the allowed set
    /// in M3b W9a — it is the source state of the W9 Pattern C
    /// drain (offline-acked doc replays through the wire-send
    /// ladder on return-online).
    StateConflict { observed: DocState },
}

/// SHA-256 over the **full** prost-encoded `gen::Check` proto bytes
/// (rro_fn, date_time, check_sign, local_number, check_type,
/// id_offline, id_cancel).  Per W7 design freeze §3 + W7.1 fix-up:
/// hashing only `check_sign` would miss drift in non-CMS fields
/// between retries; the trace's forensic value depends on the hash
/// covering every byte that goes on the wire.
fn compute_envelope_hash(envelope: &CheckEnvelope) -> [u8; 32] {
    let proto: gen::Check = envelope.clone().into();
    let bytes = proto.encode_to_vec();
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    hasher.finalize().into()
}

/// Local-private wall-clock helper for `wire_call_*` strings, which
/// bracket the `send_chk` invocation (4a — outside any tx, so DB
/// `CURRENT_TIMESTAMP` is unreachable).  Format mirrors SQLite's
/// `CURRENT_TIMESTAMP` shape (`'YYYY-MM-DD HH:MM:SS'`) so all four
/// timing TEXT columns on `transport_trace` are uniform.  Per W7
/// freeze decision #4: kept local-private to stage_send.rs rather
/// than expanded into a `WorkerContext` clock seam.
fn now_db_format() -> String {
    Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Bridge `anyhow::Error` from `with_immediate` closures back to
/// typed `StageSendError`.  Thin wrapper over the shared
/// [`super::types::bridge_anyhow_to`] (R-W10.4-senior-review LOW 1
/// close — deduplicated from three modules to one shared helper).
fn bridge_anyhow(e: anyhow::Error) -> StageSendError {
    super::types::bridge_anyhow_to(e, StageSendError::Db, StageSendError::Internal)
}

/// W10.2: derive the `transport_trace.outcome_kind` CHECK-list value
/// from a `WireDecision` + the wire error variant tag.  Mapping uses
/// the W7-frozen 5-value CHECK (`OK / REJECTED / RETRYABLE_TRANSPORT /
/// RETRYABLE_SERVER / RETRYABLE_AUTH_FN`); broader categories from the
/// W10 routing surface (WrapperBug / ProbeRequired / MacRecovery /
/// OperatorEscalation) all fold into `RETRYABLE_SERVER` because each
/// originates from a server response — not a transport-level fault.
///
/// `wire_kind` is the `&'static str` tag returned by
/// [`extract_wire_forensics`]; it disambiguates `TransientRetry` from
/// `Transport` (→ `RETRYABLE_TRANSPORT`) vs from `Server` (→
/// `RETRYABLE_SERVER`).
///
/// **W10.4 update.**  `MacRecovery` now folds to its own
/// `RETRYABLE_MAC_HASH_MISMATCH` (migration 013 extends the CHECK
/// list).  W10.5 may add finer kinds for `ProbeRequired` /
/// `OperatorEscalation` if forensic value outweighs migration cost.
fn wire_decision_to_outcome_kind(decision: &WireDecision, wire_kind: &str) -> OutcomeKind {
    match decision {
        WireDecision::Sent { .. } => OutcomeKind::Ok,
        WireDecision::Routed(d) => match d.retry_class {
            RetryClass::TerminalReject => OutcomeKind::Rejected,
            RetryClass::TransientRetry => match wire_kind {
                "Transport" => OutcomeKind::RetryableTransport,
                _ => OutcomeKind::RetryableServer,
            },
            RetryClass::FnConfigError => OutcomeKind::RetryableAuthFn,
            RetryClass::MacRecovery => OutcomeKind::RetryableMacHashMismatch,
            RetryClass::WrapperBug | RetryClass::ProbeRequired | RetryClass::OperatorEscalation => {
                OutcomeKind::RetryableServer
            }
        },
    }
}

/// W10.2: extract `(server_status_code, error_kind, error_message)`
/// from the raw wire `DpsError` for `transport_trace::complete_tx`
/// AND for the public `StageSendOutcome::Routed` surface.  Status code
/// present for `Server` and `Authorization` variants; `error_kind` is
/// the variant tag (stable, used by the outcome-kind decision fn).
fn extract_wire_forensics(err: &DpsError) -> (Option<i32>, &'static str, String) {
    use crate::transports::dps::error::AuthorizationKind;
    match err {
        DpsError::Transport(msg) => (None, "Transport", msg.clone()),
        DpsError::Server { code, message } => (Some(*code), "Server", message.clone()),
        DpsError::Authorization {
            code,
            kind,
            message,
        } => {
            let kind_str = match kind {
                AuthorizationKind::DocumentReject => "AuthorizationDocumentReject",
                AuthorizationKind::FiscalNumberNotRegistered => "AuthorizationFnNotRegistered",
            };
            (Some(*code), kind_str, message.clone())
        }
        DpsError::Decode(msg) => (None, "Decode", msg.clone()),
        DpsError::NotFound => (None, "NotFound", String::new()),
        DpsError::ServerFiscalIdMismatch {
            expected_id,
            actual_id,
        } => (
            None,
            "ServerFiscalIdMismatch",
            format!("expected={expected_id} actual={actual_id}"),
        ),
        DpsError::QueryNotSupported(q) => (None, "QueryNotSupported", q.to_string()),
        DpsError::Internal(msg) => (None, "Internal", msg.clone()),
    }
}

/// Build the `AttemptCompletion` payload for `transport_trace::complete_tx`.
/// W10.2: takes a `WireDecision` plus the pre-extracted forensics
/// tuple `(status_code, kind, message)` so the closure body can
/// compose the trace row deterministically.
fn build_attempt_completion(
    decision: &WireDecision,
    forensics: Option<&(Option<i32>, &'static str, String)>,
    wire_call_started_at: String,
    wire_call_finished_at: String,
) -> AttemptCompletion {
    let server_fiscal_no = match decision {
        WireDecision::Sent { server_fiscal_no } => Some(server_fiscal_no.clone()),
        _ => None,
    };
    let (server_status_code, error_kind, error_message) = match (decision, forensics) {
        (WireDecision::Sent { .. }, _) => (None, None, None),
        (WireDecision::Routed(_), Some((code, kind, message))) => {
            let message_opt = if message.is_empty() {
                None
            } else {
                Some(message.clone())
            };
            (*code, Some((*kind).to_string()), message_opt)
        }
        // Routed without forensics is a programming bug — every
        // Routed arm comes from a DpsError.  Surface defensively
        // rather than panic in case of unforeseen path.
        (WireDecision::Routed(_), None) => (None, Some("UnknownRouted".to_string()), None),
    };
    // W10.2 review fix-up + migration 012: durable encoding of
    // RetryClass on the routed arm.  Sent arm leaves NULL.
    let retry_class = match decision {
        WireDecision::Sent { .. } => None,
        WireDecision::Routed(d) => Some(d.retry_class.as_str().to_string()),
    };
    let kind_for_outcome = match forensics {
        Some((_, k, _)) => *k,
        None => {
            // R-W10.2-review LOW 4 close: Routed-without-forensics is
            // a programming bug — every Routed arm comes from `Err(_)`,
            // which extract_wire_forensics handles exhaustively.  Fail
            // fast in dev/CI; defensive fallback in release.
            debug_assert!(
                matches!(decision, WireDecision::Sent { .. }),
                "WireDecision::Routed reached build_attempt_completion without forensics"
            );
            "Transport"
        }
    };
    AttemptCompletion {
        wire_call_started_at,
        wire_call_finished_at,
        outcome_kind: wire_decision_to_outcome_kind(decision, kind_for_outcome),
        server_fiscal_no,
        server_status_code,
        error_kind,
        error_message: error_message.map(|m| truncate_msg(&m)),
        retry_class,
    }
}

/// `transport_trace.error_message` has CHECK length <= 512.  Truncate
/// upstream so an oversized DPS message doesn't trip the CHECK at
/// 4-b commit time and roll back the entire 4-b tx.  Char-boundary
/// safe (UTF-8 codepoint integrity preserved).
///
/// `pub(super)` since W10.4 step 2c — `mac_recovery::emit_hash_not_extractable_audit`
/// reuses the same byte-bounded truncation for audit_log payload
/// hygiene (R-W10.4-step2c-review LOW 4 close).
pub(super) fn truncate_msg(s: &str) -> String {
    if s.len() <= 512 {
        s.to_string()
    } else {
        // Char-boundary safe: take chars until we have <= 512 bytes.
        let mut out = String::with_capacity(512);
        for c in s.chars() {
            if out.len() + c.len_utf8() > 512 {
                break;
            }
            out.push(c);
        }
        out
    }
}

/// Stage 4 worker step — Pattern B (3 segments, 2 locks).
///
/// **Lock topology.**
///   - **4-pre** (`with_immediate` #1): pre-CAS read of `SendInputs`,
///     read of `SignedXml` artifact, build of `CheckEnvelope`
///     (fail-closed on UnsupportedDocType / Lnd / TS), CAS
///     `Signed → Sending`, `submission_attempted_at` stamp,
///     `transport_trace::allocate_and_insert_tx` (records
///     `request_envelope_sha256` of the **full** prost-encoded
///     `gen::Check`), `STAGE_SEND_INTENT_MARKED` audit.  Commit
///     publishes the durable intent marker.
///   - **4a** (no lock): `dps_channel.send_chk(envelope).await`.
///     The W3 static scanner enforces that this call lives outside
///     any `with_immediate` closure; the runtime guard panics in
///     debug if violated.
///   - **4b** (`with_immediate` #2): post-wire CAS
///     `Sending → {Sent | decision.target_state}` (target derived
///     from `route_send_result` → `WireDecision`), conditional
///     `set_server_fiscal_no_tx` on the success branch (gated by
///     the EmptyServerFiscalNo guard run BEFORE 4-b),
///     `transport_trace::complete_tx` (UPDATE the row 4-pre
///     allocated; `rows_affected == 0` ⇒ typed error per the
///     append-then-complete contract from W7.1), `STAGE_SEND_RESULT`
///     audit.
///
/// **Carry-forward obligations honoured.**
///   - `complete_tx == 0` → `StageSendError::TraceMissingAtComplete`.
///   - `request_envelope_sha256` = SHA-256 of the prost-encoded
///     full `gen::Check` (not just `check_sign`).
///   - Unsupported doc type fail-closed BEFORE CAS, trace, audit, wire.
///   - EmptyServerFiscalNo guard between 4a and 4b — preempts the
///     migration-010 OK-CHECK that would otherwise roll back 4-b
///     (and lose the audit / trace completion writes alongside).
///   - W3 scanner static-rejects any `send_chk` call inside
///     `with_immediate`; this implementation places `send_chk`
///     between two strictly-separated `with_immediate` blocks.
pub async fn run(
    pool: &SqlitePool,
    dps_channel: &dyn DpsChannel,
    doc: DocumentId,
    sign_ctx: Option<&SigningContext>,
) -> Result<StageSendOutcome, StageSendError> {
    // W10.4 step 2d — MAC recovery dispatch loop bound.  At most ONE
    // re-entry per `run()` invocation; combined with the DDL
    // `mac_recovery_attempts CHECK IN (0, 1)` budget, infinite-loop
    // is unreachable.  Flag is reset on each fresh `run()` call.
    //
    // Architecture (R-W10.4-senior-review MED 3 + LOW 3 close):
    // `run` is now a thin loop wrapper; the 4-pre/4a/4b body lives
    // in `run_one_attempt` so loop body indentation stays canonical
    // and each attempt is independently reasoned about.
    let mut mac_recovery_invoked = false;

    loop {
        let outcome = run_one_attempt(pool, dps_channel, doc).await?;

        // MAC recovery dispatch only fires on the routed-MacRecovery
        // arm; everything else returns directly.
        let (decision, attempt_no, wire_msg) = match &outcome {
            StageSendOutcome::Routed {
                decision,
                attempt_no,
                wire_error_message,
                ..
            } if decision.retry_class == RetryClass::MacRecovery => {
                (decision.clone(), *attempt_no, wire_error_message.clone())
            }
            _ => return Ok(outcome),
        };

        if mac_recovery_invoked {
            // Second `-12` after a successful Resigned in the same
            // run() call.  Budget burnt by the first orchestrator
            // invocation; short-circuit with FAILED_REPEAT audit.
            return override_to_rejected_with_failed_repeat_audit(pool, doc, attempt_no, wire_msg)
                .await;
        }
        mac_recovery_invoked = true;

        let ctx = sign_ctx.ok_or(StageSendError::MacRecoveryContextMissing { document_id: doc })?;
        let hint = decision.mac_recovery_hint.clone().ok_or_else(|| {
            StageSendError::Internal(anyhow::anyhow!(
                "MacRecovery decision missing mac_recovery_hint for doc {doc:?}"
            ))
        })?;

        match mac_recovery::run_mac_recovery(pool, ctx, doc, &hint).await? {
            MacRecoveryOutcome::Resigned => continue,
            MacRecoveryOutcome::HashNotExtractable => {
                // Orchestrator already emitted MAC_RECOVERY_HASH_NOT_EXTRACTABLE.
                // Caller's job: CAS to Rejected without duplicate audit.
                return override_to_rejected_no_additional_audit(
                    pool,
                    doc,
                    attempt_no,
                    AuditEvent::MacRecoveryHashNotExtractable,
                    wire_msg,
                )
                .await;
            }
            MacRecoveryOutcome::CounterExhausted => {
                return override_to_rejected_with_failed_repeat_audit(
                    pool, doc, attempt_no, wire_msg,
                )
                .await;
            }
        }
    }
}

/// One pass of the 4-pre/4a/4b cycle.  Pure body — no MAC recovery
/// dispatch.  Caller (`run`) loops over this fn at most twice (initial
/// attempt + one Resigned re-entry) bounded by the `mac_recovery_invoked`
/// flag in the caller's scope.
///
/// Extracted from inline body in W10.4 step 2d follow-up
/// (R-W10.4-senior-review MED 3 close); body identical to W10.2/W10.3
/// integrated stage 4 worker step modulo the return-vs-loop-continue
/// dispatch which now lives entirely in `run`.
async fn run_one_attempt(
    pool: &SqlitePool,
    dps_channel: &dyn DpsChannel,
    doc: DocumentId,
) -> Result<StageSendOutcome, StageSendError> {
    // ── 4-pre ────────────────────────────────────────────────────────
    let pre = with_immediate(pool, move |tx| {
        Box::pin(async move {
            // Pre-CAS read.  `inputs.state` snapshot survives the
            // CAS for the StateConflict diagnostic.
            let inputs = match fd::fetch_send_inputs_tx(tx, doc).await? {
                Some(i) => i,
                None => return Ok::<_, anyhow::Error>(PreOutcome::DocumentMissing),
            };

            // Read SIGNED_XML ahead of CAS — if it's missing we
            // surface a state-invariant breach (typed error after
            // closure return) without touching state.
            let signed_payload =
                match document_files::get_tx(tx, doc, DocumentFileKind::SignedXml).await? {
                    Some(b) => b,
                    None => return Ok(PreOutcome::SignedArtifactMissing),
                };

            // M3b W9a invariant guard: an `OfflineLocalAck` row
            // MUST carry a positive `offline_fiscal_no` (W7a writes
            // this = consumed code_lnd atomically with the state
            // flip in `transition_to_offline_local_ack_tx`, and
            // `offline_codes.code_lnd` has a schema CHECK `> 0`).
            // Two failure modes are forensically split (R2 LOW #1):
            //   - NULL → `OfflineFiscalNoMissing` (column not
            //     written; raw-SQL bypass that skipped the column).
            //   - `<= 0` → `OfflineFiscalNoNonPositive` (column
            //     written with invalid payload; raw-SQL bypass that
            //     wrote a non-positive value, or a future schema
            //     regression that drops the producer-side CHECK).
            // Both surface BEFORE any CAS / wire side effect; the
            // envelope builder would otherwise stringify the
            // invalid value into `CheckEnvelope.id_offline` and
            // mis-identify the receipt to DPS.  Online states
            // (Signed / ErrorRetryable) leave `offline_fiscal_no`
            // NULL by design — envelope builder maps that to empty
            // `id_offline` per DPS contract.
            if inputs.state == DocState::OfflineLocalAck {
                match inputs.offline_fiscal_no {
                    None => {
                        return Ok(PreOutcome::EnvelopeBuildFailed(
                            StageSendError::OfflineFiscalNoMissing { document_id: doc },
                        ));
                    }
                    Some(n) if n <= 0 => {
                        return Ok(PreOutcome::EnvelopeBuildFailed(
                            StageSendError::OfflineFiscalNoNonPositive {
                                document_id: doc,
                                observed: n,
                            },
                        ));
                    }
                    Some(_) => {}
                }
            }

            // Build envelope BEFORE CAS — fail-closed on
            // UnsupportedDocType / Lnd / TS without writing SENDING.
            let envelope = match build_send_envelope(&inputs, signed_payload) {
                Ok(e) => e,
                Err(err) => return Ok(PreOutcome::EnvelopeBuildFailed(err)),
            };

            // W10.2 HIGH 3 §4.2 + M3b W9a widening (2026-05-16): 4-pre
            // source-state CAS accepts {Signed, ErrorRetryable,
            // OfflineLocalAck} → Sending.  Online path: Signed and
            // ErrorRetryable enter via M3a Pattern B (ADR-M3-A9 step
            // 5-6 — a routed failure in 4-b transitions Sending →
            // ErrorRetryable, and the next worker tick re-enters via
            // this CAS).  Offline-drain path: OfflineLocalAck enters
            // only via the W9b backlog drain caller; no current
            // boot-phase dispatcher routes OfflineLocalAck through
            // stage_send (it is treated as terminal by
            // `dispatch_pending_doc` until W9b wires the drain).  The
            // `(OfflineLocalAck, Sending)` edge was added to the
            // `allowed_transition` whitelist by M3b W6 (PR #55).  Any
            // other observed state is a structural rejection — no CAS
            // attempt, no wire call.
            let source_state = match inputs.state {
                DocState::Signed | DocState::ErrorRetryable | DocState::OfflineLocalAck => {
                    inputs.state
                }
                other => return Ok(PreOutcome::StateConflict { observed: other }),
            };
            match fd::transition_state(tx, doc, source_state, DocState::Sending).await? {
                TransitionOutcome::Applied => {}
                TransitionOutcome::Conflict => {
                    return Ok(PreOutcome::StateConflict {
                        observed: inputs.state,
                    });
                }
                TransitionOutcome::NotFound => return Ok(PreOutcome::DocumentMissing),
                TransitionOutcome::Forbidden => {
                    unreachable!(
                        "({source_state:?},Sending) is whitelisted in fiscal_documents::allowed_transition"
                    )
                }
            }

            // Stamp submission_attempted_at = CURRENT_TIMESTAMP.
            // After CAS Applied this row exists; a `false` here is a
            // structural breach — surface as typed error.
            if !fd::mark_submission_attempted_tx(tx, doc).await? {
                return Err(anyhow::Error::new(
                    StageSendError::MarkSubmissionAttemptedMissing { document_id: doc },
                ));
            }

            // request_envelope_sha256 = SHA-256(prost(gen::Check)).
            let envelope_hash = compute_envelope_hash(&envelope);

            // Allocate trace row (intent-only fields).
            let attempt_no = transport_trace::allocate_and_insert_tx(
                tx,
                doc,
                NewAttempt {
                    backend_profile_id: inputs.backend_profile_id.clone(),
                    transport_profile_id: inputs.transport_profile_id.clone(),
                    request_envelope_sha256: envelope_hash,
                },
            )
            .await?;

            // Audit STAGE_SEND_INTENT_MARKED.  Payload uses serde_json
            // for safe escaping of profile id strings.
            let payload = serde_json::json!({
                "attempt_no": attempt_no,
                "backend_profile_id": inputs.backend_profile_id,
                "transport_profile_id": inputs.transport_profile_id,
            })
            .to_string();
            audit_log::append_tx(
                tx,
                "fiscal_document",
                &format!("{doc:?}"),
                "STAGE_SEND_INTENT_MARKED",
                Severity::Info,
                None,
                Some(&payload),
            )
            .await?;

            Ok(PreOutcome::Marked {
                envelope,
                attempt_no,
                doc_type: inputs.doc_type,
                fiscal_number: inputs.fiscal_number,
            })
        })
    })
    .await
    .map_err(bridge_anyhow)?;

    let (envelope, attempt_no, doc_type, fiscal_number) = match pre {
        PreOutcome::Marked {
            envelope,
            attempt_no,
            doc_type,
            fiscal_number,
        } => (envelope, attempt_no, doc_type, fiscal_number),
        PreOutcome::DocumentMissing => return Ok(StageSendOutcome::DocumentMissing),
        PreOutcome::SignedArtifactMissing => {
            return Err(StageSendError::SignedArtifactMissing { document_id: doc })
        }
        PreOutcome::EnvelopeBuildFailed(e) => return Err(e),
        PreOutcome::StateConflict { observed } => {
            return Ok(StageSendOutcome::StateConflict { observed })
        }
    };

    // ── 4a — wire send OUTSIDE any lock ──────────────────────────────
    //
    // W3 static scanner enforces that `send_chk` is not reachable from
    // inside any `with_immediate` closure body; the runtime
    // task_local guard panics in debug if a foreign-IO call happens
    // inside a BEGIN IMMEDIATE scope.  This call site is at module
    // top level, between the two `with_immediate` blocks above and
    // below.
    let wire_call_started_at = now_db_format();
    let wire_result = dps_channel.send_chk(envelope.clone()).await;
    let wire_call_finished_at = now_db_format();

    // W10.2: dispatch on the typed routing surface.  `is_live_send=true`
    // — production stage 4 send (freeze §3.5; W9 reconciliation will
    // pass `false`).  Forensics (status_code / error_kind /
    // error_message) are extracted ONCE here so they're available BOTH
    // for the 4-b `transport_trace::complete_tx` row AND for the
    // public `StageSendOutcome::Routed { wire_status_code,
    // wire_error_message, .. }` surface tests rely on.
    let wire_forensics: Option<(Option<i32>, &'static str, String)> = match &wire_result {
        Ok(_) => None,
        Err(e) => Some(extract_wire_forensics(e)),
    };
    let wire_decision = route_send_result(wire_result, doc_type, true);

    // EmptyServerFiscalNo guard (LOW risk close from W7.3 review).
    // The transport_trace OK-CHECK would otherwise reject 4-b commit
    // and roll back the entire 4-b tx (losing the audit and
    // CAS-Sending->Sent in the process); catching here lets the
    // doc stay cleanly in `Sending` for W9 reconciliation.
    if let WireDecision::Sent { server_fiscal_no } = &wire_decision {
        if server_fiscal_no.is_empty() {
            return Err(StageSendError::EmptyServerFiscalNo { document_id: doc });
        }
    }

    // ── 4b ───────────────────────────────────────────────────────────
    //
    // W10.2 dispatch:
    //   - `WireDecision::Sent`            → CAS Sending → Sent +
    //                                        set_server_fiscal_no_tx +
    //                                        audit STAGE_SEND_RESULT.
    //   - `WireDecision::Routed(decision)` → CAS Sending →
    //                                        decision.target_state +
    //                                        audit decision.audit_event.
    //
    // **W10.3 honoured.**  `decision.node_mode_flip == Some(Blocked)`
    // is invoked inside this same 4-b `with_immediate` envelope —
    // `node_state.mode → BLOCKED` is atomic with the CAS
    // `Sending → Rejected` and the audit row.  Server-11 cannot leave
    // the FN unblocked.
    //
    // **W10.2 deferred:**
    //   - `decision.mac_recovery_hint` (Server-12) → W10.4 will
    //     orchestrate MAC re-sign + re-send BEFORE this 4-b commit.
    //   - `decision.probe_hint` → W9 reconciliation territory; never
    //     actioned in stage 4 (only surfaced in audit payload for
    //     forensic grep, per W10.2 review LOW/MED 3).
    let decision_for_closure = wire_decision.clone();
    let forensics_for_closure = wire_forensics.clone();
    let started_for_closure = wire_call_started_at;
    let finished_for_closure = wire_call_finished_at;
    // R-W10.3-review LOW 2 close: `fiscal_number` is moved directly
    // (single owner — the closure).  `wire_decision` and
    // `wire_forensics` are cloned because the post-closure return
    // block reads them; `fiscal_number` is not.
    with_immediate(pool, move |tx| {
        let decision = decision_for_closure;
        let forensics = forensics_for_closure;
        let started = started_for_closure;
        let finished = finished_for_closure;
        // `fiscal_number` captured directly via `move`; no rebind
        // (R-W10.3-review LOW 3 close — the previous self-rebind
        // `let fiscal_number = fiscal_number;` was a no-op).
        Box::pin(async move {
            let target = match &decision {
                WireDecision::Sent { .. } => DocState::Sent,
                WireDecision::Routed(d) => d.target_state,
            };

            // Post-wire CAS Sending -> target.  Single-writer +
            // 4-pre-committed marker: any non-Applied outcome here is
            // a structural breach (no other writer can mutate the
            // doc, the marker is durable).
            match fd::transition_state(tx, doc, DocState::Sending, target).await? {
                TransitionOutcome::Applied => {}
                observed => {
                    return Err(anyhow::Error::new(StageSendError::PostWireCasFailed {
                        document_id: doc,
                        target,
                        observed,
                    }));
                }
            }

            // server_fiscal_no UPDATE on success branch (Empty guard
            // ran before this closure; we know it's non-empty here).
            if let WireDecision::Sent { server_fiscal_no } = &decision {
                if !fd::set_server_fiscal_no_tx(tx, doc, server_fiscal_no).await? {
                    return Err(anyhow::Error::new(
                        StageSendError::SetServerFiscalNoMissing { document_id: doc },
                    ));
                }
            }

            // W10.3 — node_state.mode flip atomic with the CAS above.
            // Only Server-11 currently emits this flip; future routes
            // may emit it for additional NodeMode targets.  We restrict
            // to BLOCKED here (the only target the routing fn ever
            // emits per freeze §3); if `node_mode_flip` ever carries
            // a different NodeMode, the match falls through to a
            // skip — surfaced via debug_assert! so dev/CI catches it.
            if let WireDecision::Routed(d) = &decision {
                if let Some(target_mode) = d.node_mode_flip {
                    debug_assert_eq!(
                        target_mode,
                        crate::db::models::enums::NodeMode::Blocked,
                        "W10.3 only honours NodeMode::Blocked; routing fn must \
                         not emit other NodeMode targets without extending stage_send"
                    );
                    if target_mode == crate::db::models::enums::NodeMode::Blocked
                        && !node_state::set_mode_blocked_tx(tx, &fiscal_number).await?
                    {
                        return Err(anyhow::Error::new(
                            StageSendError::NodeStateMissingForBlock {
                                fn_id: fiscal_number.clone(),
                                document_id: doc,
                            },
                        ));
                    }
                }
            }

            // Complete trace row.  rows_affected == 0 ⇒ typed error
            // (W7.1 append-then-complete contract).
            let completion =
                build_attempt_completion(&decision, forensics.as_ref(), started, finished);
            let outcome_kind_str = completion.outcome_kind.as_str();
            let rows = transport_trace::complete_tx(tx, doc, attempt_no, completion).await?;
            if rows == 0 {
                return Err(anyhow::Error::new(StageSendError::TraceMissingAtComplete {
                    document_id: doc,
                    attempt_no,
                }));
            }

            // Audit event: success arm uses STAGE_SEND_RESULT (W7
            // contract); routed arm uses `decision.audit_event` per
            // freeze §3.4 closed enum.
            //
            // Payload composition (W10.2 LOW 1 + LOW/MED 3 + W10.3 LOW 1):
            //   - `attempt_no`, `outcome_kind` always present (W7).
            //   - `retry_class` on the routed arm — forensic grep
            //     dimension orthogonal to event_type.
            //   - `node_mode_flipped: "Blocked"` on Server-11 — durable
            //     evidence of the W10.3 flip; redundant with the
            //     `node_state.mode = 'BLOCKED'` row (DDL keeps SHOUTING
            //     case) but cheap, and lets audit-log forensics work
            //     without a join.
            //   - `probe_hint` reason on Decode/-2/-15 close-shift —
            //     surfaces the W9 last_chk-probe target without
            //     re-decoding the routing fn.
            let (event_type, severity) = match &decision {
                WireDecision::Sent { .. } => ("STAGE_SEND_RESULT", Severity::Info),
                WireDecision::Routed(d) => (d.audit_event.as_str(), d.audit_severity),
            };
            let mut payload_obj = serde_json::json!({
                "attempt_no": attempt_no,
                "outcome_kind": outcome_kind_str,
            });
            if let WireDecision::Routed(d) = &decision {
                // R-W10.3-review LOW 1 close: all three payload
                // discriminators use PascalCase consistently — matches
                // `RetryClass::as_str()` migration-012 wire form and the
                // Rust `Debug` form of `NodeMode` / `ProbeReason`.
                // Earlier draft uppercased `node_mode_flipped`; that
                // mirrored the DDL form ('BLOCKED') but broke
                // forensic-grep consistency across audit_log JSON
                // payloads.
                //
                // R-W10.3-review LOW 3 doc note: `node_mode_flipped`
                // encodes ROUTING INTENT (the `-11` decision asked
                // for BLOCKED), NOT a state transition observation.
                // SQLite UPDATE semantics report rows_affected=1 for
                // a matching row even when the value is unchanged, so
                // the second `-11` for the same already-BLOCKED FN
                // emits the same payload — by design, no CAS guard
                // (avoids a read-then-write race on a hot path).
                payload_obj["retry_class"] =
                    serde_json::Value::String(d.retry_class.as_str().to_string());
                if let Some(mode) = d.node_mode_flip {
                    payload_obj["node_mode_flipped"] =
                        serde_json::Value::String(format!("{mode:?}"));
                }
                if let Some(hint) = &d.probe_hint {
                    payload_obj["probe_hint"] =
                        serde_json::Value::String(format!("{:?}", hint.reason));
                }
            }
            let payload = payload_obj.to_string();
            audit_log::append_tx(
                tx,
                "fiscal_document",
                &format!("{doc:?}"),
                event_type,
                severity,
                None,
                Some(&payload),
            )
            .await?;

            Ok::<_, anyhow::Error>(())
        })
    })
    .await
    .map_err(bridge_anyhow)?;

    Ok(match wire_decision {
        WireDecision::Sent { server_fiscal_no } => StageSendOutcome::Sent {
            server_fiscal_no,
            attempt_no,
        },
        WireDecision::Routed(decision) => {
            let (wire_status_code, wire_error_message) = match &wire_forensics {
                Some((code, _kind, msg)) if !msg.is_empty() => (*code, Some(truncate_msg(msg))),
                Some((code, _kind, _empty)) => (*code, None),
                // Routed arm without source forensics is a programming
                // bug — Routed only reaches here from `Err(_)`.
                None => (None, None),
            };
            StageSendOutcome::Routed {
                decision,
                attempt_no,
                wire_status_code,
                wire_error_message,
            }
        }
    })
}

// ─── W10.4 step 2d — recovery-failure override helpers ───────────────

/// Synthetic `RoutingDecision` for the post-recovery override paths
/// (HashNotExtractable / CounterExhausted): doc transitions
/// `ErrorRetryable → Rejected`; the carried `audit_event` discriminates
/// which recovery-failure mode caused the override.
fn synthetic_rejected_decision(audit_event: AuditEvent) -> RoutingDecision {
    RoutingDecision {
        target_state: DocState::Rejected,
        retry_class: RetryClass::TerminalReject,
        audit_event,
        audit_severity: Severity::Error,
        node_mode_flip: None,
        probe_hint: None,
        mac_recovery_hint: None,
    }
}

/// Override path for `HashNotExtractable`: orchestrator already emitted
/// `MAC_RECOVERY_HASH_NOT_EXTRACTABLE`.  Caller's job is just to CAS
/// the doc out of `ErrorRetryable` into `Rejected`.  No audit row
/// (recovery layer + final state suffice for forensics).
///
/// `synthetic_decision_event` populates the surfaced
/// `StageSendOutcome::Routed.decision.audit_event`.  The helper does
/// NOT emit an audit row — naming reflects "which event the synthetic
/// decision will carry", not "what we'll write to audit_log"
/// (R-W10.4-step2d-review LOW 2 close — earlier `audit_event_for_outcome`
/// suggested writing).
///
/// `wire_error_message` carries the original `-12` wire message
/// through to the public `StageSendOutcome` surface so caller-side
/// forensics don't lose context (R-W10.4-step2d-review LOW 1 close).
async fn override_to_rejected_no_additional_audit(
    pool: &SqlitePool,
    doc: DocumentId,
    attempt_no: i32,
    synthetic_decision_event: AuditEvent,
    wire_error_message: Option<String>,
) -> Result<StageSendOutcome, StageSendError> {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            match fd::transition_state(tx, doc, DocState::ErrorRetryable, DocState::Rejected)
                .await?
            {
                TransitionOutcome::Applied => Ok::<_, anyhow::Error>(()),
                observed => Err(anyhow::Error::new(StageSendError::PostWireCasFailed {
                    document_id: doc,
                    target: DocState::Rejected,
                    observed,
                })),
            }
        })
    })
    .await
    .map_err(bridge_anyhow)?;
    Ok(StageSendOutcome::Routed {
        decision: synthetic_rejected_decision(synthetic_decision_event),
        attempt_no,
        // `-12` is the only Server status code that maps to
        // `RetryClass::MacRecovery` (W10.1 routing fn §2.1 row -12);
        // hardcoded value matches the contract.
        wire_status_code: Some(-12),
        wire_error_message,
    })
}

/// Override path for `CounterExhausted` AND for second `-12` after a
/// successful Resigned in the same `run()` call.  Both signal that the
/// recovery budget is spent; emit `MAC_RECOVERY_FAILED_REPEAT_HASH_MISMATCH`
/// audit + CAS to `Rejected` atomically.
///
/// `wire_error_message` (R-W10.4-step2d-review LOW 1 close) preserves
/// the wire `-12` message through to the public `StageSendOutcome`.
async fn override_to_rejected_with_failed_repeat_audit(
    pool: &SqlitePool,
    doc: DocumentId,
    attempt_no: i32,
    wire_error_message: Option<String>,
) -> Result<StageSendOutcome, StageSendError> {
    let payload = serde_json::json!({
        "attempt_no": attempt_no,
        "outcome_kind": "REJECTED",
        "retry_class": RetryClass::TerminalReject.as_str(),
    })
    .to_string();
    with_immediate(pool, move |tx| {
        let payload = payload.clone();
        Box::pin(async move {
            match fd::transition_state(tx, doc, DocState::ErrorRetryable, DocState::Rejected)
                .await?
            {
                TransitionOutcome::Applied => {}
                observed => {
                    return Err(anyhow::Error::new(StageSendError::PostWireCasFailed {
                        document_id: doc,
                        target: DocState::Rejected,
                        observed,
                    }));
                }
            }
            audit_log::append_tx(
                tx,
                "fiscal_document",
                &format!("{doc:?}"),
                AuditEvent::MacRecoveryFailedRepeatHashMismatch.as_str(),
                Severity::Error,
                None,
                Some(&payload),
            )
            .await?;
            Ok::<_, anyhow::Error>(())
        })
    })
    .await
    .map_err(bridge_anyhow)?;
    Ok(StageSendOutcome::Routed {
        decision: synthetic_rejected_decision(AuditEvent::MacRecoveryFailedRepeatHashMismatch),
        attempt_no,
        // See note in `override_to_rejected_no_additional_audit`:
        // `-12` is the only MacRecovery wire code.
        wire_status_code: Some(-12),
        wire_error_message,
    })
}

// ─── Unit tests for the pure surface ─────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::enums::{DocState, DocType};

    fn inputs(doc_type: DocType, lnd: i64, business_ts: &str) -> SendInputs {
        SendInputs {
            state: DocState::Signed,
            fiscal_number: "1234567890".into(),
            lnd,
            doc_type,
            business_ts: business_ts.into(),
            backend_profile_id: "b1".into(),
            transport_profile_id: "t1".into(),
            // M3b W9a: M3a online docs (Signed source) have NULL
            // offline_fiscal_no by design — envelope builder maps
            // that to empty id_offline per the Sprint-7-proven
            // contract.
            offline_fiscal_no: None,
        }
    }

    // ─── build_send_envelope ────────────────────────────────────────

    #[test]
    fn build_envelope_sell_passes_lnd_and_chk() {
        let env = build_send_envelope(
            &inputs(DocType::Sell, 42, "2026-05-09T12:34:56Z"),
            b"PAY".to_vec(),
        )
        .expect("SELL/lnd=42 must build");
        assert_eq!(env.rro_fn, "1234567890");
        assert_eq!(env.local_number, 42);
        assert_eq!(env.check_type, DpsCheckType::Chk);
        assert_eq!(env.check_sign, b"PAY");
        assert_eq!(env.id_offline, "");
        assert_eq!(env.id_cancel, "");
    }

    #[test]
    fn build_envelope_shift_open_overrides_local_number_to_zero() {
        let env = build_send_envelope(
            &inputs(DocType::ShiftOpen, 1234, "2026-05-09T12:34:56Z"),
            b"PAY".to_vec(),
        )
        .expect("SHIFT_OPEN must build");
        assert_eq!(
            env.local_number, 0,
            "SHIFT_OPEN must override local_number to 0 (Sprint-7-proven Python contract)"
        );
        assert_eq!(env.check_type, DpsCheckType::ServiceChk);
    }

    #[test]
    fn build_envelope_return_uses_chk_check_type() {
        let env = build_send_envelope(
            &inputs(DocType::Return, 7, "2026-05-09T12:34:56Z"),
            b"PAY".to_vec(),
        )
        .expect("RETURN must build");
        assert_eq!(env.check_type, DpsCheckType::Chk);
        assert_eq!(env.local_number, 7);
    }

    #[test]
    fn build_envelope_z_report_uses_zreport_check_type() {
        let env = build_send_envelope(
            &inputs(DocType::ZReport, 99, "2026-05-09T12:34:56Z"),
            b"PAY".to_vec(),
        )
        .expect("Z_REPORT must build");
        assert_eq!(env.check_type, DpsCheckType::ZReport);
        assert_eq!(env.local_number, 99);
    }

    #[test]
    fn build_envelope_shift_close_routes_through_zreport() {
        // SHIFT_CLOSE collapses to ZReport via derive_wire_artifact_kind
        // (ADR-M3-A2 boundary mapping).  Wire check_type therefore must
        // be ZREPORT, not ServiceChk or Chk.
        let env = build_send_envelope(
            &inputs(DocType::ShiftClose, 100, "2026-05-09T12:34:56Z"),
            b"PAY".to_vec(),
        )
        .expect("SHIFT_CLOSE must build via ZReport route");
        assert_eq!(env.check_type, DpsCheckType::ZReport);
        assert_eq!(env.local_number, 100);
    }

    #[test]
    fn build_envelope_unsupported_doc_type_fails_closed() {
        for dt in [
            DocType::ServiceIn,
            DocType::ServiceOut,
            DocType::CashWithdrawal,
            DocType::XReport,
        ] {
            let r = build_send_envelope(&inputs(dt, 1, "2026-05-09T12:34:56Z"), b"PAY".to_vec());
            match r {
                Err(StageSendError::UnsupportedDocType { doc_type }) => {
                    assert_eq!(doc_type, dt, "unsupported variant must echo back doc_type");
                }
                other => panic!("expected UnsupportedDocType for {dt:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn build_envelope_lnd_overflow_returns_typed_error() {
        let r = build_send_envelope(
            &inputs(DocType::Sell, (i32::MAX as i64) + 1, "2026-05-09T12:34:56Z"),
            b"PAY".to_vec(),
        );
        match r {
            Err(StageSendError::LndOutOfRangeI32 { lnd }) => {
                assert_eq!(lnd, (i32::MAX as i64) + 1);
            }
            other => panic!("expected LndOutOfRangeI32, got {other:?}"),
        }
    }

    #[test]
    fn build_envelope_shift_open_skips_lnd_overflow_check() {
        // SHIFT_OPEN always sends local_number = 0; an overflowing lnd
        // on the row must NOT prevent envelope construction (the
        // override happens BEFORE the i32::try_from).
        let env = build_send_envelope(
            &inputs(
                DocType::ShiftOpen,
                (i32::MAX as i64) + 1,
                "2026-05-09T12:34:56Z",
            ),
            b"PAY".to_vec(),
        )
        .expect("SHIFT_OPEN must build despite oversize lnd");
        assert_eq!(env.local_number, 0);
    }

    #[test]
    fn build_envelope_invalid_business_ts_returns_typed_error() {
        let r = build_send_envelope(
            &inputs(DocType::Sell, 1, "not-a-timestamp"),
            b"PAY".to_vec(),
        );
        match r {
            Err(StageSendError::TimestampConversion { detail }) => {
                assert!(
                    detail.contains("not-a-timestamp"),
                    "detail must reference offending input, got {detail}"
                );
            }
            other => panic!("expected TimestampConversion, got {other:?}"),
        }
    }

    #[test]
    fn build_envelope_kyiv_local_epoch_summer_dst_offset_3h() {
        // 2026-07-15T10:00:00Z is summer (DST active) in Kyiv: local
        // = 13:00 EEST (UTC+3).  Kyiv-local-as-epoch fakes 13:00 as
        // UTC, so the value is `2026-07-15T13:00:00Z.timestamp()`.
        let env = build_send_envelope(
            &inputs(DocType::Sell, 1, "2026-07-15T10:00:00Z"),
            b"PAY".to_vec(),
        )
        .expect("summer build must succeed");
        let expected = Utc
            .with_ymd_and_hms(2026, 7, 15, 13, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(env.date_time, expected, "summer DST offset must be +3h");
    }

    #[test]
    fn build_envelope_kyiv_local_epoch_winter_offset_2h() {
        // 2026-01-15T10:00:00Z is winter (no DST): local = 12:00 EET
        // (UTC+2).  Faked-as-UTC epoch is `2026-01-15T12:00:00Z.timestamp()`.
        let env = build_send_envelope(
            &inputs(DocType::Sell, 1, "2026-01-15T10:00:00Z"),
            b"PAY".to_vec(),
        )
        .expect("winter build must succeed");
        let expected = Utc
            .with_ymd_and_hms(2026, 1, 15, 12, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(env.date_time, expected, "winter offset must be +2h");
    }

    // ─── compute_envelope_hash ──────────────────────────────────────

    fn sample_envelope() -> CheckEnvelope {
        CheckEnvelope {
            rro_fn: "1234567890".into(),
            date_time: 1_700_000_000,
            check_sign: b"CMS".to_vec(),
            local_number: 7,
            check_type: DpsCheckType::Chk,
            id_offline: String::new(),
            id_cancel: String::new(),
        }
    }

    #[test]
    fn envelope_hash_is_deterministic() {
        let env = sample_envelope();
        let h1 = compute_envelope_hash(&env);
        let h2 = compute_envelope_hash(&env);
        assert_eq!(h1, h2, "hash must be deterministic for identical input");
        assert_eq!(h1.len(), 32);
        assert_ne!(h1, [0u8; 32], "hash must not be the zero vector");
    }

    #[test]
    fn envelope_hash_reflects_check_sign_drift() {
        let mut a = sample_envelope();
        let mut b = sample_envelope();
        b.check_sign = b"DIFFERENT".to_vec();
        assert_ne!(
            compute_envelope_hash(&a),
            compute_envelope_hash(&b),
            "hash must change when check_sign changes"
        );
        a.check_sign = b"DIFFERENT".to_vec();
        assert_eq!(
            compute_envelope_hash(&a),
            compute_envelope_hash(&b),
            "matching check_sign must yield matching hash"
        );
    }

    #[test]
    fn envelope_hash_reflects_non_cms_field_drift() {
        // Per W7.1 fix-up: hash must cover the FULL envelope, not
        // just check_sign.  Drift in date_time / local_number /
        // check_type / id_offline / id_cancel must change the hash.
        let baseline = compute_envelope_hash(&sample_envelope());

        let mut alt = sample_envelope();
        alt.date_time = 1_700_000_001;
        assert_ne!(
            compute_envelope_hash(&alt),
            baseline,
            "date_time drift must change hash"
        );

        let mut alt = sample_envelope();
        alt.local_number = 8;
        assert_ne!(
            compute_envelope_hash(&alt),
            baseline,
            "local_number drift must change hash"
        );

        let mut alt = sample_envelope();
        alt.check_type = DpsCheckType::ZReport;
        assert_ne!(
            compute_envelope_hash(&alt),
            baseline,
            "check_type drift must change hash"
        );

        let mut alt = sample_envelope();
        alt.id_offline = "ABC".into();
        assert_ne!(
            compute_envelope_hash(&alt),
            baseline,
            "id_offline drift must change hash"
        );

        let mut alt = sample_envelope();
        alt.id_cancel = "XYZ".into();
        assert_ne!(
            compute_envelope_hash(&alt),
            baseline,
            "id_cancel drift must change hash"
        );

        let mut alt = sample_envelope();
        alt.rro_fn = "9999999999".into();
        assert_ne!(
            compute_envelope_hash(&alt),
            baseline,
            "rro_fn drift must change hash"
        );
    }

    // ─── now_db_format ──────────────────────────────────────────────

    #[test]
    fn now_db_format_matches_sqlite_current_timestamp_shape() {
        let s = now_db_format();
        assert_eq!(
            s.len(),
            19,
            "expected 'YYYY-MM-DD HH:MM:SS' shape, got {s:?}"
        );
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[7..8], "-");
        assert_eq!(&s[10..11], " ");
        assert_eq!(&s[13..14], ":");
        assert_eq!(&s[16..17], ":");
    }

    // ─── truncate_msg ───────────────────────────────────────────────

    #[test]
    fn truncate_msg_passes_short_strings_unchanged() {
        assert_eq!(truncate_msg("hello"), "hello");
        assert_eq!(truncate_msg(""), "");
    }

    #[test]
    fn truncate_msg_caps_at_512_bytes() {
        let big = "x".repeat(1024);
        let out = truncate_msg(&big);
        assert!(out.len() <= 512, "truncated len = {}", out.len());
        assert_eq!(out.len(), 512);
    }

    #[test]
    fn truncate_msg_respects_utf8_boundaries() {
        // 4-byte char ('🦀' = U+1F980, 4 bytes UTF-8).  Build a string
        // that crosses 512 mid-codepoint.
        let prefix = "x".repeat(510);
        let s = format!("{prefix}🦀rest");
        let out = truncate_msg(&s);
        assert!(out.len() <= 512, "len = {}", out.len());
        // Must end on a valid UTF-8 boundary; last char must NOT be
        // a partial codepoint.
        assert!(out.is_char_boundary(out.len()));
    }

    // ─── extract_wire_forensics + outcome_kind mapping ──────────────

    #[test]
    fn extract_wire_forensics_carries_status_code_and_kind_per_variant() {
        use crate::transports::dps::error::AuthorizationKind;
        let cases: Vec<(DpsError, Option<i32>, &'static str)> = vec![
            (DpsError::Transport("TLS".into()), None, "Transport"),
            (
                DpsError::Server {
                    code: -3,
                    message: "ERROR_SAVE".into(),
                },
                Some(-3),
                "Server",
            ),
            (
                DpsError::Authorization {
                    code: -1,
                    kind: AuthorizationKind::DocumentReject,
                    message: "ERROR_VEREFY".into(),
                },
                Some(-1),
                "AuthorizationDocumentReject",
            ),
            (
                DpsError::Authorization {
                    code: -13,
                    kind: AuthorizationKind::FiscalNumberNotRegistered,
                    message: "ERROR_NOT_REGISTERED_RRO".into(),
                },
                Some(-13),
                "AuthorizationFnNotRegistered",
            ),
            (DpsError::Decode("status=0".into()), None, "Decode"),
            (DpsError::NotFound, None, "NotFound"),
            (
                DpsError::ServerFiscalIdMismatch {
                    expected_id: "A".into(),
                    actual_id: "B".into(),
                },
                None,
                "ServerFiscalIdMismatch",
            ),
            (
                DpsError::QueryNotSupported("ByLocalIdentity"),
                None,
                "QueryNotSupported",
            ),
            (DpsError::Internal("wrapper".into()), None, "Internal"),
        ];
        for (err, exp_code, exp_kind) in cases {
            let (code, kind, _msg) = extract_wire_forensics(&err);
            assert_eq!(code, exp_code, "{err:?}");
            assert_eq!(kind, exp_kind, "{err:?}");
        }
    }

    #[test]
    fn wire_decision_to_outcome_kind_maps_per_w10_table() {
        // Covers both W10.2 baseline mapping (5 retry classes folding
        // to W7-frozen kinds) and W10.4 split (MacRecovery →
        // RetryableMacHashMismatch via migration 013).
        use super::super::error_routing::{AuditEvent, RetryClass, RoutingDecision, WireDecision};
        // Sent → OK regardless of wire_kind.
        assert_eq!(
            wire_decision_to_outcome_kind(
                &WireDecision::Sent {
                    server_fiscal_no: "X".into()
                },
                "Transport"
            ),
            OutcomeKind::Ok
        );
        // Helper to build a routed decision quickly.
        let routed = |rc: RetryClass| {
            WireDecision::Routed(RoutingDecision {
                target_state: DocState::ErrorRetryable,
                retry_class: rc,
                audit_event: AuditEvent::StageSendResult,
                audit_severity: Severity::Warning,
                node_mode_flip: None,
                probe_hint: None,
                mac_recovery_hint: None,
            })
        };
        // TerminalReject → REJECTED.
        assert_eq!(
            wire_decision_to_outcome_kind(&routed(RetryClass::TerminalReject), "Server"),
            OutcomeKind::Rejected
        );
        // TransientRetry split: Transport → RETRYABLE_TRANSPORT,
        // anything else → RETRYABLE_SERVER.
        assert_eq!(
            wire_decision_to_outcome_kind(&routed(RetryClass::TransientRetry), "Transport"),
            OutcomeKind::RetryableTransport
        );
        assert_eq!(
            wire_decision_to_outcome_kind(&routed(RetryClass::TransientRetry), "Server"),
            OutcomeKind::RetryableServer
        );
        // FnConfigError → RETRYABLE_AUTH_FN.
        assert_eq!(
            wire_decision_to_outcome_kind(&routed(RetryClass::FnConfigError), "Authorization"),
            OutcomeKind::RetryableAuthFn
        );
        // WrapperBug / ProbeRequired / OperatorEscalation fold to
        // RETRYABLE_SERVER; MacRecovery has its own kind from W10.4
        // (migration 013 extends the CHECK list with
        // RETRYABLE_MAC_HASH_MISMATCH).
        for rc in [
            RetryClass::WrapperBug,
            RetryClass::ProbeRequired,
            RetryClass::OperatorEscalation,
        ] {
            assert_eq!(
                wire_decision_to_outcome_kind(&routed(rc), "Server"),
                OutcomeKind::RetryableServer,
                "{rc:?} should fold to RetryableServer"
            );
        }
        // W10.4 split: MacRecovery → RETRYABLE_MAC_HASH_MISMATCH.
        assert_eq!(
            wire_decision_to_outcome_kind(&routed(RetryClass::MacRecovery), "Server"),
            OutcomeKind::RetryableMacHashMismatch,
            "MacRecovery must fold to RETRYABLE_MAC_HASH_MISMATCH (W10.4 + migration 013)"
        );
    }
}
