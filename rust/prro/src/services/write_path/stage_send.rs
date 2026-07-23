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

use chrono::{DateTime, Utc};
use chrono_tz::Europe::Kiev;
use sqlx::SqlitePool;

use crate::db::models::enums::{DocState, DocType, Severity, ShiftState};
use crate::db::models::ids::{DocumentId, ShiftId};
use crate::db::repositories::fiscal_documents::SendInputs;
use crate::db::repositories::transport_trace::OutcomeKind;
use crate::db::repositories::{
    audit_log, document_files,
    document_files::DocumentFileKind,
    fiscal_documents::{self as fd, TransitionOutcome},
    node_state, shifts,
    transport_trace::{self, AttemptCompletion, NewAttempt},
};
use crate::db::tx::with_immediate;
use crate::db::types::DbDocumentId;
use crate::services::shift::transition;
use crate::services::write_path::signer_guard::{self, SignerCashierMismatch};
use crate::transports::dps::channel::DpsChannel;
use crate::transports::dps::dto::{CheckEnvelope, DpsCheckType};
use crate::transports::dps::error::DpsError;

use super::error_routing::{
    route_dps_error, AuditEvent, ProbeHint, ProbeReason, RetryClass, RoutingDecision, WireDecision,
};
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

    /// **STOP-O3-1 fix (v7) — wire-correctness fail-closed.**  A doc ACQUIRED
    /// OFFLINE (`fs_mode = 'OFFLINE'`) that is NOT yet offline-acked
    /// (`offline_fiscal_no IS NULL`, so state ∈ {Signed, ErrorRetryable}) must
    /// NEVER be sent to DPS online.  Closes the GO_ONLINE mode-race: an offline
    /// SHIFT_OPEN / SHIFT_CLOSE / Z_REPORT / SELL committed its acquire edge +
    /// signed under `mode = Offline`, then the mode flipped to `Online` before
    /// `dispatch_post_sign` (which routes by NODE mode, not doc `fs_mode`) →
    /// the doc would otherwise be sent with an empty `id_offline`, wrongly
    /// issuing to DPS a receipt that was never offline-numbered.  Refused
    /// BEFORE any CAS / `Sending` marker / wire — the doc stays put for the
    /// offline lane (offline-ack on return-offline, or the orphan-recovery of
    /// fix (b) / boot SW-2).  Drain-owned offline docs are EXCLUDED (they are
    /// `OfflineLocalAck` with `offline_fiscal_no = Some`); online docs are
    /// `fs_mode = 'ONLINE'`.
    #[error(
        "stage 4 (v7): doc {document_id:?} was acquired OFFLINE (fs_mode=OFFLINE) and is not yet \
         offline-acked (offline_fiscal_no NULL) — refusing the online send (GO_ONLINE mode-race); \
         the doc stays put for the offline lane / orphan recovery"
    )]
    OfflineDocRoutedOnline { document_id: DocumentId },

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

    /// B8-3: `fiscal_documents.offline_dps_code` is NULL for a doc in
    /// `OfflineLocalAck`.  The stamp is written atomically with the
    /// `offline_fiscal_no` at the `Signed → OfflineLocalAck` CAS
    /// (`transition_to_offline_local_ack_tx`); a NULL here is a
    /// producer-side invariant breach (raw-SQL bypass or a pre-B8 doc
    /// that was offline-acked before migration 029 landed).  Surfaced
    /// fail-closed BEFORE any CAS / wire side-effect so the doc stays
    /// in `OfflineLocalAck` for operator inspection; emitting the
    /// integer ordinal string silently mis-identifies the receipt to DPS.
    #[error(
        "stage 4 W9 drain: doc {document_id:?} is in OfflineLocalAck but offline_dps_code is \
         NULL; the opaque DPS code must be stamped atomically with offline_fiscal_no at W7a \
         (transition_to_offline_local_ack_tx)"
    )]
    OfflineDpsCodeMissing { document_id: DocumentId },

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

    /// W4-Z2a piece 9 — MR-NO-TX snapshot reload failed.  Locked
    /// rule #9: MAC recovery uses the persisted snapshot keyed by
    /// `fiscal_documents.signing_config_snapshot_id`, NEVER the
    /// live config.  Surfaces `signing_config_snapshots::get_by_id`
    /// failures forensically: `NotFound` (orphan FK — corrupted
    /// ledger), `ChecksumMismatch` (storage tamper or unfinished
    /// write), `DeserializeFailed` / `UnsupportedKind` (schema
    /// drift).  All catastrophic; orchestrator MUST NOT proceed
    /// to re-sign with an empty / fresh-config fallback (would
    /// silently break MAC chain).
    #[error("MAC recovery snapshot reload failed: {0}")]
    MacRecoverySnapshotReloadFailed(
        #[source]
        crate::db::repositories::signing_config_snapshots::SigningConfigSnapshotsRepoError,
    ),

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
        // B10 — offline-session boundary docs are service receipts, same
        // typCheck as SHIFT_OPEN (ground truth: WebCheck `TypToPROTO` maps
        // typ 9/10 → PROTO 3 = ServiceChk).
        WireArtifactKind::OfflineSessionBegin | WireArtifactKind::OfflineSessionEnd => {
            DpsCheckType::ServiceChk
        }
        // L3 — service cash-in/out: DPS SubmitCheck code 3 = ServiceChk.
        // WebCheck `DealCheck` sends to the same endpoint with `typCheck=3`.
        WireArtifactKind::ServiceIn | WireArtifactKind::ServiceOut => DpsCheckType::ServiceChk,
        // EPZ — видача готівки за ЕПЗ is a FISCAL receipt (`<C T='8'>`, has a
        // good + fiscal number), sent via the CHK path like SELL/RETURN
        // (WebCheck `EPZtoCash` → `FiscalReceipt`, not the DealCheck service path).
        WireArtifactKind::CashAdvanceEpz => DpsCheckType::Chk,
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

    // WebCheck's drain path parses the already-signed XML's `<TS>` and
    // passes that 14-digit value straight through as `Check.date`. The
    // ordinary online path instead uses the Kyiv-local-as-epoch convention.
    // Mixing the two representations makes DPS reject the document with -8
    // (`ERROR_XML_DATE`) before it can evaluate the MAC chain.
    //
    // `offline_fiscal_no.is_some()` is the durable discriminator for an
    // offline-origin document on the drain. The two boundary doc types use
    // the same raw-TS rule even when END is minted as an online issuance.
    // DPS `-8` FIX (live-proven 2026-07-12; WebCheck-verified): `Check.date_time`
    // MUST equal the signed `<TS>` = `kyiv_comp_date` (Kyiv-local `YYYYMMDDHHMMSS`
    // int) for EVERY doc — online AND offline. WebCheck sends exactly this
    // (`dd = СurrentCompDate()`) as `Check.date` for all check types (CHK / Z /
    // SERVICE). The old ONLINE `kyiv_local_epoch` produced a Kyiv-wall-clock-as-
    // UTC epoch = +3h ahead of DPS's UTC clock → envelope ≠ `<TS>` → DPS `-8`
    // "дата не відповідає Check.date" on online shift-close. B10 fixed only the
    // offline branch; this aligns online → the same encoding.
    // docs/DPS_MINUS8_DATE_AND_SHIFT_RECOVERY.md
    let date_time = kyiv_comp_date(&inputs.business_ts)?;

    // M3b W9a (2026-05-16): `id_offline` carries the offline-acquired
    // fiscal-no for W9 backlog-drain replays.  W7a writes
    // `fiscal_documents.offline_fiscal_no = consumed code_lnd` at the
    // Signed → OfflineLocalAck transition; the wire contract
    // (docs/superpowers/specs/2026-05-04-m2-w0-1-dps-wire.md:116)
    // requires `id_offline = offline_fiscal_no.to_string()`.  For
    // M3a online docs this is `NULL` and the empty string maps to
    // DPS-interpreted "online" per the Sprint-7 Python contract.
    // B8-3: render id_offline from the opaque DPS code string, not the integer
    // ordinal.  For offline-origin docs the guard above guarantees
    // `offline_dps_code` is `Some`; for online docs it is `None` → empty
    // string (DPS interprets empty as "online" per Sprint-7 contract).
    let id_offline = inputs.offline_dps_code.clone().unwrap_or_default();

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

/// Convert UTC ISO-8601 `business_ts` to the DPS `Check.date` / signed `<TS>`
/// form: Kyiv-local `YYYYMMDDHHMMSS` parsed as an integer.
///
/// This is the SINGLE encoding for BOTH the envelope `Check.date_time` and the
/// XML `<TS>` — WebCheck sends exactly this (`dd = СurrentCompDate()`) as
/// `Check.date` for every check type (CHK / Z / SERVICE), online and offline.
/// A prior online `kyiv_local_epoch` (Kyiv-wall-clock re-interpreted as a UTC
/// epoch) was WRONG: it read +3h into the future vs the DPS UTC clock →
/// envelope ≠ `<TS>` → DPS `-8` "дата не відповідає Check.date"; removed
/// 2026-07-12 (docs/DPS_MINUS8_DATE_AND_SHIFT_RECOVERY.md).
fn kyiv_comp_date(business_ts: &str) -> Result<i64, StageSendError> {
    let dt: DateTime<Utc> =
        business_ts
            .parse::<DateTime<Utc>>()
            .map_err(|e| StageSendError::TimestampConversion {
                detail: format!("parse {business_ts:?}: {e}"),
            })?;
    let kyiv = dt.with_timezone(&Kiev);
    kyiv.format("%Y%m%d%H%M%S")
        .to_string()
        .parse::<i64>()
        .map_err(|e| StageSendError::TimestampConversion {
            detail: format!("format Kyiv comp-date for {business_ts:?}: {e}"),
        })
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
    /// W14a-2b Commit 5 — signer-guard refusal at 4-pre BEFORE
    /// envelope/CAS/trace/wire.  `SIGNER_CASHIER_MISMATCH` Warning
    /// audit row has been committed to the same `with_immediate` tx
    /// (preserves forensic trail per spec §2.5 + W14a-2a Round 7 §8.1
    /// Ok-return contract).  No state mutation; subsequent retry
    /// without fixing the signer id re-triggers the same refusal.
    SignerRefused(SignerCashierMismatch),
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
    ///
    /// **CS-3 S7-1:** the authorize-tx ALSO minted the sealed [`Authorization`] (reservation insert +
    /// generation advance + RN→CALL_STARTED) in the SAME tx, so `auth` is carried out to gate the
    /// wire. Only the fields the wire/record/apply path needs survive — `shift_id` /
    /// `offline_fiscal_no` / `previous_hash` / `unsigned_xml_sha256` / `mac_recovery_attempts` /
    /// `fiscal_number` are re-read from durable state by the apply orchestration (Q3), not threaded.
    Marked {
        auth: crate::db::repositories::delivery_reservation::Authorization,
        envelope: CheckEnvelope,
        attempt_no: i32,
        doc_type: DocType,
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
    /// W14a-2b Commit 5 — signer_guard refused inside the 4-pre tx.
    /// `SIGNER_CASHIER_MISMATCH` Warning audit has been emitted within
    /// the same `with_immediate` envelope BEFORE this `Ok` return so
    /// the commit preserves the forensic record (spec §2.5 + W14a-2a
    /// Round 7 §8.1 Ok-return contract).  No CAS, no trace, no marker,
    /// no wire.
    SignerRefused(SignerCashierMismatch),
    /// STOP-O3-1 fix (v7) — an OFFLINE-acquired, not-yet-offline-acked doc was
    /// routed to stage_send by the GO_ONLINE mode-race.  Read-only refusal
    /// branch (no CAS, no trace, no marker, no wire); surfaced as the typed
    /// [`StageSendError::OfflineDocRoutedOnline`] after the closure returns.
    OfflineDocRoutedOnline,
}

/// SHA-256 over the **full** prost-encoded `gen::Check` proto bytes
/// (rro_fn, date_time, check_sign, local_number, check_type,
/// id_offline, id_cancel).  Per W7 design freeze §3 + W7.1 fix-up:
/// hashing only `check_sign` would miss drift in non-CMS fields
/// between retries; the trace's forensic value depends on the hash
/// covering every byte that goes on the wire.
/// W14a-2b Commit 5 — emit `SIGNER_CASHIER_MISMATCH` Warning audit
/// inside the surrounding `with_immediate` 4-pre tx.
///
/// Per spec §2.5:
/// - Severity = `Warning` (refused BEFORE wire — no fiscal commitment
///   leaked).
/// - Actor = `attempted_signer_id` for `Mismatch` (forensic "who
///   tried" query); `None` for the 4 structural-bug variants (no
///   operator identity OR root cause is caller bug).
/// - Payload includes variant tag + variant-specific fields per
///   spec §2.5 table.
async fn emit_signer_cashier_mismatch_audit_tx(
    tx: &mut crate::db::tx::WriteTxConn<'_>,
    doc: DocumentId,
    inputs: &SendInputs,
    mismatch: &SignerCashierMismatch,
) -> Result<(), sqlx::Error> {
    let doc_hex = format!("{doc:?}");
    let (variant_tag, actor, mut payload_obj): (&'static str, Option<String>, serde_json::Value) =
        match mismatch {
            SignerCashierMismatch::SignerIdMissing { document_id } => (
                "SignerIdMissing",
                None,
                serde_json::json!({
                    "fiscal_number": inputs.fiscal_number,
                    "document_id": format!("{document_id:?}"),
                    "doc_type": inputs.doc_type.as_str(),
                }),
            ),
            SignerCashierMismatch::ShiftMissingForFiscalDoc {
                document_id,
                doc_type,
            } => (
                "ShiftMissingForFiscalDoc",
                None,
                serde_json::json!({
                    "fiscal_number": inputs.fiscal_number,
                    "document_id": format!("{document_id:?}"),
                    "doc_type": doc_type.as_str(),
                }),
            ),
            SignerCashierMismatch::ShiftIdMismatch {
                document_id,
                doc_type,
                expected_shift_id,
                supplied_shift_id,
            } => (
                "ShiftIdMismatch",
                None,
                serde_json::json!({
                    "fiscal_number": inputs.fiscal_number,
                    "document_id": format!("{document_id:?}"),
                    "doc_type": doc_type.as_str(),
                    "expected_shift_id": expected_shift_id.map(|s| format!("{s:?}")),
                    "supplied_shift_id": format!("{supplied_shift_id:?}"),
                }),
            ),
            SignerCashierMismatch::CrossFnMismatch {
                document_id,
                inputs_fiscal_number,
                shift_fiscal_number,
            } => (
                "CrossFnMismatch",
                None,
                serde_json::json!({
                    "document_id": format!("{document_id:?}"),
                    "inputs_fiscal_number": inputs_fiscal_number,
                    "shift_fiscal_number": shift_fiscal_number,
                }),
            ),
            SignerCashierMismatch::Mismatch {
                shift_id,
                document_id,
                expected_cashier_id,
                attempted_signer_id,
                doc_type,
            } => (
                "Mismatch",
                Some(attempted_signer_id.as_str().to_string()),
                serde_json::json!({
                    "fiscal_number": inputs.fiscal_number,
                    "document_id": format!("{document_id:?}"),
                    "doc_type": doc_type.as_str(),
                    "shift_id": format!("{shift_id:?}"),
                    "expected_cashier_id": expected_cashier_id.as_str(),
                    "attempted_signer_id": attempted_signer_id.as_str(),
                }),
            ),
        };
    if let serde_json::Value::Object(map) = &mut payload_obj {
        map.insert(
            "variant".to_string(),
            serde_json::Value::String(variant_tag.into()),
        );
        map.insert(
            "refused_at_stage".to_string(),
            serde_json::Value::String("stage_send_pre".into()),
        );
    }
    audit_log::append_tx(
        tx,
        "fiscal_document",
        &doc_hex,
        "SIGNER_CASHIER_MISMATCH",
        Severity::Warning,
        actor.as_deref(),
        Some(&payload_obj.to_string()),
    )
    .await?;
    Ok(())
}

fn compute_envelope_hash(envelope: &CheckEnvelope) -> [u8; 32] {
    // CS-3 S7-1 (B1): single source — delegate to the canonical `CheckEnvelope::wire_hash`
    // (SHA-256 over the full prost `gen::Check`). The token/reservation `envelope_hash`, the
    // `submit_authorized` rebind check and this trace `request_envelope_sha256` are now the SAME value.
    envelope.wire_hash()
}

/// CS-3 S7-1 (composition impl-design §6-Q1): the FIXED production DPS protocol binding — the single
/// source for BOTH the reservation/token binding (`build_new_reservation`) and the `submit_authorized`
/// `port_binding` (AO-2 echoes them against each other, so the happy path always matches). `cpv`/`ecr`
/// are provably always `None` and `protocol_id`/`contract` are fixed `FSCO_ZZD`/`1` today; this
/// graduates to a per-FN config read only at CS-6 (`EvpzDps`), which has no live wire path yet.
fn production_dps_binding() -> prro_domain::delivery::DpsProtocolBinding {
    prro_domain::delivery::DpsProtocolBinding {
        protocol_id: prro_domain::delivery::DpsProtocolId::FscoZzd,
        contract_version: prro_domain::delivery::ProtocolContractVersion(1),
        capability_profile_version: None,
        endpoint_config_revision: None,
    }
}

/// CS-3 S7-1 (composition impl-design §6-Q5): build the `NewReservation` the authorize-tx mints.
/// `envelope_hash` is the FULL wire hash (B1 — `CheckEnvelope::wire_hash`), IDENTICAL to what
/// `submit_authorized` rebind-checks and to the trace `request_envelope_sha256`. `reservation_id` is a
/// fresh time-ordered UUID (unique per attempt; the DDL `UNIQUE(document_id, attempt_no)` + the
/// `no_replace` trigger are the fail-closed backstop). The binding fields come from
/// [`production_dps_binding`] so token == port by construction (AO-2 is a rebind/tamper guard).
fn build_new_reservation(
    doc: DocumentId,
    inputs: &SendInputs,
    envelope: &CheckEnvelope,
) -> crate::db::repositories::delivery_reservation::NewReservation {
    let b = production_dps_binding();
    crate::db::repositories::delivery_reservation::NewReservation {
        reservation_id: uuid::Uuid::now_v7().into_bytes(),
        document_id: doc,
        fiscal_number: inputs.fiscal_number.clone(),
        dps_protocol_id: b.protocol_id.as_str().to_string(),
        protocol_contract_version: i64::from(b.contract_version.0),
        capability_profile_version: b.capability_profile_version.map(|v| i64::from(v.0)),
        endpoint_config_revision: b.endpoint_config_revision.map(|v| i64::from(v.0)),
        envelope_hash: envelope.wire_hash(),
    }
}

/// CS-3 S7-1 (composition impl-design §6-Q2.2): build the two record arguments from the post-wire
/// evidence — the TARGET authority (`classify` + `EvidenceDiscriminant`) that `record_outcome`
/// persists. Post-wire the reservation has crossed `CALL_STARTED` (a `Started` generation witness); a
/// `NotSubmitted` certainty (a pre-wire refusal) never reaches record, so the `Started` pairing is
/// total here. `remote_correlation_id` is `None` for the live path (matches legacy 4-b).
fn build_record_args(
    obs: &crate::db::repositories::delivery_reservation::AttemptObservation,
) -> anyhow::Result<(
    prro_domain::delivery::evidence::EvidenceDiscriminant,
    prro_domain::delivery::ObservedOutcomeV1,
    prro_domain::delivery::ClassifiedOutcome,
)> {
    let classified = prro_domain::delivery::classify(obs.evidence());
    let disc = prro_domain::delivery::evidence::EvidenceDiscriminant::from_evidence(obs.evidence());
    // migration-036 evidence matrix (arm 5): a clean `Accepted` REQUIRES
    // `remote_correlation_id == evidence_text` (the accepted fiscal number F); a NULL correlation on
    // an `Accepted` trips the matrix trigger and rolls the record tx back. Every other outcome carries
    // a NULL correlation. Derive it from the discriminant's own columns so it can never drift from
    // `evidence_text`.
    let cols = disc.to_columns();
    let correlation = if cols.kind == "Accepted" {
        cols.text
            .as_deref()
            .map(prro_domain::delivery::BoundedText::from_truncating)
    } else {
        None
    };
    let generation = prro_domain::delivery::PositiveGeneration::new(obs.authorized_generation())
        .ok_or_else(|| {
            anyhow::anyhow!("build_record_args: authorized_generation must be >= 1 post-wire")
        })?;
    let outcome = prro_domain::delivery::ObservedOutcomeV1::record(
        &classified,
        correlation,
        prro_domain::delivery::AuthorizedGeneration::Started(generation),
    )
    .map_err(|e| anyhow::anyhow!("build_record_args: ObservedOutcomeV1::record: {e:?}"))?;
    // F3: return `classified` too — the projection of the durable authority onto the WireDecision
    // (trace/audit/return/drain) is built from this SAME classifier, so they cannot drift.
    Ok((disc, outcome, classified))
}

/// CS-3 Slice E (Pin 2 / Track A — the SINGLE routing authority). Project the sealed evidence leaf +
/// the classifier onto the `WireDecision` that drives trace + audit + the returned outcome + drain
/// failure-class. **TOTAL** over `EvidenceDiscriminantKind` — a new leaf breaks the build (no `_`
/// catch-all). Replaces the former `project_decision_from_evidence`, which special-cased the two
/// leaves whose projection diverged from a naive legacy pass-through; those are now STRUCTURAL leaves.
///
/// **Boundary (rev4 §3.1, `docs/CS3_SLICE_E_PIN2_LEAF_TUPLE_TABLE.md`):** `target_state`,
/// `retry_class`, `node_mode_flip`, `probe_hint.reason`, and the happy `Sent{server_fiscal_no}` (SFN
/// from `disc::Accepted`, NOT the legacy `CheckAck`) come from `classified` + `disc` ONLY — never the
/// collapsed legacy `DpsError`. `route_dps_error` is thereby demoted to the raw legacy-vs-classifier
/// drift-pin (`grpc.rs`) + the forensic overlay (`extract_wire_forensics`); it no longer authors the
/// wire decision.
///
/// **Diagnostic overlay (`route_dps_error` demoted) — `wrapper_overlay`.** The message / DPS
/// status-code reach the trace via the SEPARATE, untouched `wire_forensics`
/// (`extract_wire_forensics(&legacy)`) path. `mac_recovery_hint` is `None` on every leaf: the
/// projected decision's hint is DEAD (the W10.4 MAC orchestrator sources the `-12` hash from the
/// durable evidence — `mac_recovery.rs`, not this projection). ONE diagnostic CANNOT be rebuilt from
/// classifier+disc: a WRAPPER-side bug (`Internal` / `NotFound` / `QueryNotSupported` /
/// `ServerFiscalIdMismatch`) collapses into the `NoResponse` leaf (evidence carries no wrapper shape)
/// AND mints an empty `WireDiagnostics::default()` (the absence path), yet the S7-1 contract keeps it
/// SENDING-held with a CRITICAL `WrapperBug` audit for operator visibility
/// (`write_path_dps_error_routing.rs` fx06–fx09). Its ONLY witness is the legacy `route_dps_error`
/// result — passed as `wrapper_overlay` (the demoted `route_dps_error`) and consulted EXCLUSIVELY on
/// the `NoResponse` leaf to preserve that `WrapperBug` decision verbatim (behaviour-neutral). Every
/// other leaf ignores the overlay — routing is classifier+disc-authoritative.
fn wire_decision_from(
    disc: &prro_domain::delivery::evidence::EvidenceDiscriminant,
    classified: &prro_domain::delivery::ClassifiedOutcome,
    wrapper_overlay: Option<&RoutingDecision>,
) -> WireDecision {
    use prro_domain::delivery::evidence::EvidenceDiscriminantKind as K;
    use prro_domain::delivery::{ActiveRetryClass, DpsReject, NodeEffect};

    // node_mode_flip is a pure function of the classifier's node_effect (only Offline168 → Blocked).
    let node_mode_flip = match classified.node_effect() {
        NodeEffect::NodeBlocked => Some(crate::db::models::enums::NodeMode::Blocked),
        _ => None,
    };

    // Per-leaf authority tuple: (target_state, retry_class, audit_event, audit_severity, probe_reason).
    // The happy `Accepted` arm early-returns `Sent{sfn}` (routing() is None there). Every other leaf is
    // `Routed`. See the leaf→tuple table doc for the full derivation + behaviour-neutrality proof.
    type Tuple = (
        DocState,
        RetryClass,
        AuditEvent,
        Severity,
        Option<ProbeReason>,
    );
    let (target_state, retry_class, audit_event, audit_severity, probe_reason): Tuple =
        match disc.kind() {
            K::Accepted(fiscal_no) => {
                return WireDecision::Sent {
                    server_fiscal_no: fiscal_no.to_string(),
                }
            }
            // Definitive DPS verdict — audit_event + severity per verdict (authority-side, from disc).
            K::Rejected(verdict, _digest) => match verdict {
                // -1 ERROR_VEREFY — document-level reject, ERROR severity (operator-visible, not a bug).
                DpsReject::Verify => (
                    DocState::Rejected,
                    RetryClass::TerminalReject,
                    AuditEvent::StageSendRejected,
                    Severity::Error,
                    None,
                ),
                // -11 ERROR_OFFLINE_168 — terminal + node BLOCKED flip (via node_effect above).
                DpsReject::Offline168 => (
                    DocState::Rejected,
                    RetryClass::TerminalReject,
                    AuditEvent::StageSendNodeBlocked,
                    Severity::Critical,
                    None,
                ),
                // -12 ERROR_BAD_HASH_PREV — MAC recovery class.
                DpsReject::BadHashPrev => (
                    DocState::ErrorRetryable,
                    RetryClass::MacRecovery,
                    AuditEvent::StageSendMacHashMismatch,
                    Severity::Warning,
                    None,
                ),
                // -6 ERROR_NOT_PREV_ZREPORT — operator-recoverable.
                DpsReject::NotPrevZReport => (
                    DocState::ErrorRetryable,
                    RetryClass::OperatorEscalation,
                    AuditEvent::StageSendOperatorEscalation,
                    Severity::Error,
                    None,
                ),
                // -13/-14 ERROR_NOT_REGISTERED_* — FN-config error.
                DpsReject::NotRegisteredRro | DpsReject::NotRegisteredSigner => (
                    DocState::ErrorRetryable,
                    RetryClass::FnConfigError,
                    AuditEvent::StageSendFnNotRegistered,
                    Severity::Error,
                    None,
                ),
                // -5/-7..-10 XML-class, -16 OfflineId, `Close` (=-2/-15 on non-close) — terminal, CRITICAL
                // (builder/adapter/ingress bug worth an operator alert).
                DpsReject::Type
                | DpsReject::Xml
                | DpsReject::XmlDate
                | DpsReject::XmlChk
                | DpsReject::XmlZReport
                | DpsReject::OfflineId
                | DpsReject::Close => (
                    DocState::Rejected,
                    RetryClass::TerminalReject,
                    AuditEvent::StageSendRejected,
                    Severity::Critical,
                    None,
                ),
            },
            // -2/-15 on a close/Z doc — close-shift ambiguity, HELD for a probe.
            K::CloseAmbiguous(_digest) => (
                DocState::ErrorRetryable,
                RetryClass::ProbeRequired,
                AuditEvent::StageSendProbeRequired,
                Severity::Warning,
                Some(ProbeReason::CloseShiftProbe),
            ),
            // status==0 proto-default decode — HELD for a probe.
            K::MissingStatus(_digest) => (
                DocState::ErrorRetryable,
                RetryClass::ProbeRequired,
                AuditEvent::StageSendDecodeUnknown,
                Severity::Warning,
                Some(ProbeReason::DecodeUnknown),
            ),
            // DPS OK with an empty fiscal id — no issuance, HELD for a probe.
            K::OkButNoFiscalNumber(_digest) => (
                DocState::ErrorRetryable,
                RetryClass::ProbeRequired,
                AuditEvent::StageSendProbeRequired,
                Severity::Warning,
                Some(ProbeReason::OkButNoFiscalNumber),
            ),
            // TLS-proven authenticated non-DPS peer — HELD for a probe. (Former F3 forward divergence:
            // legacy `RemoteStatus → TransientRetry`, durable `classify → ProbeRequired`; now structural.)
            K::RemoteAuthStatus(_digest) => (
                DocState::ErrorRetryable,
                RetryClass::ProbeRequired,
                AuditEvent::StageSendProbeRequired,
                Severity::Warning,
                Some(ProbeReason::RemoteStatus),
            ),
            // Parsed envelope, unnamed non-zero status code (`-4`/`-17`/`-99`). Pin 2: classifier routes
            // TransientRetry (unifying the two legacy sources — the former F3 reverse divergence). Pin 3
            // flips `routing_for_indeterminate(UnknownStatus) → ProbeRequired`, activating the dormant
            // SubmittedUnknown probe here with NO projection change.
            K::UnknownStatus(_code, _digest) => match classified.routing() {
                Some(ActiveRetryClass::ProbeRequired) => (
                    DocState::ErrorRetryable,
                    RetryClass::ProbeRequired,
                    AuditEvent::StageSendProbeRequired,
                    Severity::Warning,
                    Some(ProbeReason::SubmittedUnknown),
                ),
                _ => (
                    DocState::ErrorRetryable,
                    RetryClass::TransientRetry,
                    AuditEvent::StageSendTransientRetry,
                    Severity::Warning,
                    None,
                ),
            },
            // -3 ERROR_SAVE — transient retry.
            K::SaveError(_digest) => (
                DocState::ErrorRetryable,
                RetryClass::TransientRetry,
                AuditEvent::StageSendTransientRetry,
                Severity::Warning,
                None,
            ),
            // Genuine local absence / transport failure — the classifier collapses EVERY wire failure
            // (incl. wrapper-side bugs) to NoResponse → TransientRetry. A wrapper-bug's CRITICAL
            // WrapperBug diagnostic cannot be rebuilt from the classifier (evidence lost the shape), so
            // it rides the demoted `route_dps_error` overlay — preserved VERBATIM (S7-1 SENDING-held
            // wrapper contract; `write_path_dps_error_routing.rs` fx06–fx09, incl. the distinct
            // FiscalIdMismatch audit). Transport (the reachable case) has a non-WrapperBug overlay and
            // falls through to the classifier's TransientRetry.
            K::NoResponse(_cause) => {
                if let Some(ov) = wrapper_overlay {
                    if ov.retry_class == RetryClass::WrapperBug {
                        return WireDecision::Routed(ov.clone());
                    }
                }
                (
                    DocState::ErrorRetryable,
                    RetryClass::TransientRetry,
                    AuditEvent::StageSendTransientRetry,
                    Severity::Warning,
                    None,
                )
            }
            // NotStarted preflight leaves — UNREACHABLE post-wire (`submit_authorized` returns only
            // `Started`); handled for totality. SigningFailed classifies WrapperBug (CRITICAL — this is
            // where the classifier's WrapperBug class is authored); PreconditionFailed → TransientRetry.
            K::SigningFailed => (
                DocState::ErrorRetryable,
                RetryClass::WrapperBug,
                AuditEvent::StageSendWrapperBug,
                Severity::Critical,
                None,
            ),
            K::PreconditionFailed => (
                DocState::ErrorRetryable,
                RetryClass::TransientRetry,
                AuditEvent::StageSendTransientRetry,
                Severity::Warning,
                None,
            ),
        };

    WireDecision::Routed(RoutingDecision {
        target_state,
        retry_class,
        audit_event,
        audit_severity,
        node_mode_flip,
        probe_hint: probe_reason.map(|reason| ProbeHint { reason }),
        mac_recovery_hint: None,
    })
}

/// CS-3 S7-1 (composition impl-design §6-Q4): build the public `StageSendOutcome` from the TARGET
/// `WireDecision` (`target_wire_decision(route_send_result(...))`) — RELOCATED verbatim from the legacy
/// 4-b return block. Driving the return from the TARGET (not the raw legacy) is FIX-B2: an empty-SFN
/// `Sent{""}` has already been mapped to a `Routed(OkButNoFiscalNumber)` HELD upstream.
fn stage_send_outcome_from(
    decision: WireDecision,
    attempt_no: i32,
    forensics: Option<&(Option<i32>, &'static str, String)>,
) -> StageSendOutcome {
    match decision {
        WireDecision::Sent { server_fiscal_no } => StageSendOutcome::Sent {
            server_fiscal_no,
            attempt_no,
        },
        WireDecision::Routed(decision) => {
            let (wire_status_code, wire_error_message) = match forensics {
                Some((code, _kind, msg)) if !msg.is_empty() => (*code, Some(truncate_msg(msg))),
                Some((code, _kind, _empty)) => (*code, None),
                None => (None, None),
            };
            StageSendOutcome::Routed {
                decision,
                attempt_no,
                wire_status_code,
                wire_error_message,
            }
        }
    }
}

/// CS-3 S7-1 (composition impl-design §6-Q2.3): the outcome audit — RELOCATED verbatim from the legacy
/// 4-b audit block (`STAGE_SEND_RESULT` on the `Sent` arm, `decision.audit_event` on the routed arm,
/// same payload). Driven by the TARGET `WireDecision` (FIX-B2). Emitted inside the record transaction.
async fn append_stage_send_result_audit(
    tx: &mut crate::db::tx::WriteTxConn<'_>,
    doc: DocumentId,
    decision: &WireDecision,
    attempt_no: i32,
    outcome_kind_str: &str,
) -> anyhow::Result<()> {
    let (event_type, severity) = match decision {
        WireDecision::Sent { .. } => ("STAGE_SEND_RESULT", Severity::Info),
        WireDecision::Routed(d) => (d.audit_event.as_str(), d.audit_severity),
    };
    let mut payload_obj = serde_json::json!({
        "attempt_no": attempt_no,
        "outcome_kind": outcome_kind_str,
    });
    if let WireDecision::Routed(d) = decision {
        payload_obj["retry_class"] = serde_json::Value::String(d.retry_class.as_str().to_string());
        if let Some(mode) = d.node_mode_flip {
            payload_obj["node_mode_flipped"] = serde_json::Value::String(format!("{mode:?}"));
        }
        if let Some(hint) = &d.probe_hint {
            payload_obj["probe_hint"] = serde_json::Value::String(format!("{:?}", hint.reason));
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
    Ok(())
}

/// CS-3 S7-1 (composition impl-design §6-Q2.3): the RECORD transaction — one `BEGIN IMMEDIATE` that
/// atomically (1) commits the TARGET record via the repo `record_outcome` (axes + `PENDING_APPLY` +
/// early STOP/BLOCKED), (2) completes the `transport_trace`, (3) appends the outcome audit. The doc
/// target CAS / SFN / seed / shift edges MOVE to the APPLY orchestration (Q3); record owns
/// evidence + trace + audit + safety-halt. Consumes `obs` by value (moved into the closure so
/// `record_outcome` can borrow it); the caller captures `reservation_id` before the move.
#[allow(clippy::too_many_arguments)]
async fn record_transaction(
    pool: &SqlitePool,
    obs: crate::db::repositories::delivery_reservation::AttemptObservation,
    outcome: prro_domain::delivery::ObservedOutcomeV1,
    disc: prro_domain::delivery::evidence::EvidenceDiscriminant,
    decision: WireDecision,
    forensics: Option<(Option<i32>, &'static str, String)>,
    started: String,
    finished: String,
    doc: DocumentId,
    attempt_no: i32,
) -> Result<(), StageSendError> {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            // 1. TARGET record: authority CAS + axes + PENDING_APPLY + early STOP/BLOCKED (repo).
            crate::db::repositories::delivery_reservation::record_outcome(
                tx, &obs, &outcome, &disc,
            )
            .await
            .map_err(anyhow::Error::new)?;
            // 2. complete the existing transport_trace (verbatim relocation from legacy 4-b).
            let completion =
                build_attempt_completion(&decision, forensics.as_ref(), started, finished);
            let outcome_kind_str = completion.outcome_kind.as_str().to_string();
            let rows = transport_trace::complete_tx(tx, doc, attempt_no, completion).await?;
            if rows == 0 {
                return Err(anyhow::Error::new(StageSendError::TraceMissingAtComplete {
                    document_id: doc,
                    attempt_no,
                }));
            }
            // 3. outcome audit — TARGET decision (verbatim relocation from legacy 4-b).
            append_stage_send_result_audit(tx, doc, &decision, attempt_no, &outcome_kind_str)
                .await?;
            Ok::<_, anyhow::Error>(())
        })
    })
    .await
    .map_err(bridge_anyhow)
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
            RetryClass::WrapperBug
            | RetryClass::ProbeRequired
            | RetryClass::OperatorEscalation
            // Historical persisted tag only; current routing never emits it.
            | RetryClass::DrainChainSettleRetry => {
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
        // CS-3 Slice A′ (RA: forensic tuple is a COMPATIBILITY PROJECTION kept identical to `Transport`; the slice is not behaviour-neutral overall): `Unauthenticated` / `PermissionDenied` now
        // surface as `RemoteStatus` instead of `Transport`.  The persisted
        // `transport_trace.error_kind` / `outcome_kind` are CHECK-constrained, so we keep
        // the forensic tuple IDENTICAL to `Transport` (no schema drift); the in-memory
        // `DpsError::RemoteStatus` type is what the CS-3 classifier (slice E) consumes.
        DpsError::RemoteStatus { message, .. } => (None, "Transport", message.clone()),
        // CS-3 Slice A (RA: forensic tuple is a COMPATIBILITY PROJECTION kept identical to `Transport`; the slice is not behaviour-neutral overall): a parsed `-4` now surfaces as `Indeterminate`
        // instead of `Transport`. The persisted `transport_trace.error_kind` /
        // `outcome_kind` (via wire_decision_to_outcome_kind's "Transport" arm) are
        // CHECK-constrained, so we keep the forensic tuple IDENTICAL to `Transport`
        // (no schema drift); `-4`'s distinctness lives in the in-memory `DpsError` type,
        // which the CS-3 classifier consumes. Forensic distinctness lands with slice E.
        DpsError::Indeterminate { message, .. } => (None, "Transport", message.clone()),
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
    // CS-3 S7-1 (FIX-B2): the empty-SFN case maps a wire-OK `Sent{""}` to `Routed(OkButNoFiscalNumber)`
    // — a LEGITIMATE Routed WITHOUT forensics (the wire returned OK, so there is no error to extract).
    // Detect it so the trace records clean (no-error) fields and the "Routed ⟺ forensics" invariant is
    // not violated for this one non-error routed leaf.
    let is_ok_no_fiscal = matches!(
        decision,
        WireDecision::Routed(d)
            if matches!(
                d.probe_hint.as_ref().map(|h| h.reason),
                Some(ProbeReason::OkButNoFiscalNumber)
            )
    );
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
        // FIX-B2: the mapped-OK `OkButNoFiscalNumber` routed leaf — no wire error, clean forensic fields.
        (WireDecision::Routed(_), None) if is_ok_no_fiscal => (None, None, None),
        // Any OTHER Routed without forensics is a programming bug — every real routed arm comes from a
        // DpsError.  Surface defensively rather than panic in case of an unforeseen path.
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
                matches!(decision, WireDecision::Sent { .. }) || is_ok_no_fiscal,
                "WireDecision::Routed (non-OkButNoFiscalNumber) reached build_attempt_completion \
                 without forensics"
            );
            // Unused by `wire_decision_to_outcome_kind` except on `TransientRetry`; the
            // `OkButNoFiscalNumber` leaf is `ProbeRequired`, so this fallback never affects it.
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
    _sign_ctx: Option<&SigningContext>,
) -> Result<StageSendOutcome, StageSendError> {
    // CS-3 S7-1 (R3): the MAC-recovery loop is DELETED. A `-12` routed leaf is now a recorded HOLD —
    // `run_one_attempt` records `MacReseedPending` (which sets STOP in `record_outcome`) and
    // `apply_recorded_outcome` returns the EXPECTED `HeldNotAutoRelease`; there is NO re-sign, NO
    // `continue`, and NO second wire. `_sign_ctx` is unused (the envelope is pre-signed at stage_sign);
    // the parameter is kept for zero caller edits — all 7 callers pass it (S7-3 cleanup removes it).
    run_one_attempt(pool, dps_channel, doc).await
}

/// A′.1 piece 2 — POST-WIRE shift confirm edge (3: `Opening → Opened`, 10:
/// `Closing → Closed`).  DPS already accepted the SENT commit, so this MUST
/// NOT roll it back: on any non-`Applied` outcome the Sent commit STANDS and
/// the drift is audited (Info for the benign already-at-target no-op, CRITICAL
/// for real drift) — repair belongs to boot orphan-resolution / manual-recon.
/// The shift row + `node_state` projection move atomically via the transition
/// service (sole-writer discipline).  A projection-mirror failure on an
/// APPLIED shift is a structural breach unreachable under the FN single-writer
/// lease and is the ONLY path that propagates an error (fail-closed) — every
/// reachable non-Applied outcome returns `Ok` (no rollback).
///
/// `closing_cash_kop` — when `to == Closed`, the computed closing cash-on-hand
/// to persist in `shifts.cash_balance_kop` (L0 carry anchor for the next shift).
/// For all other transitions this value is ignored (`None` is fine; `Some(0)` also safe).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn confirm_shift_edge(
    tx: &mut crate::db::tx::WriteTxConn<'_>,
    fiscal_number: &str,
    shift_id: Option<ShiftId>,
    doc: DocumentId,
    from: ShiftState,
    to: ShiftState,
    edge: &str,
    closing_cash_kop: Option<i64>,
) -> anyhow::Result<()> {
    // `shift_id == None` on an online SHIFT_OPEN / Z / SHIFT_CLOSE is
    // impossible after piece 3 (edge 1/8 bind it at acquire).  Fail closed —
    // CRITICAL audit, NO panic, NO rollback (the Sent commit stands).
    let Some(shift_id) = shift_id else {
        emit_shift_confirm_audit(
            tx,
            doc,
            "SHIFT_CONFIRM_EDGE_DRIFT",
            Severity::Critical,
            edge,
            "shift_id_none",
        )
        .await?;
        return Ok(());
    };
    match transition::apply_shift_transition(tx, fiscal_number, shift_id, from, to).await? {
        shifts::TransitionOutcome::Applied => {
            // L0 — persist closing cash at the CLOSE edge (Closing → Closed or
            // ClosingLocalPendingDrain → Closed), atomically in this envelope
            // (invariant #2).  The value becomes the next shift's opening carry.
            if to == ShiftState::Closed {
                if let Some(kop) = closing_cash_kop {
                    shifts::update_cash_balance_tx(tx, shift_id, kop).await?;
                }
            }
            Ok(())
        }
        shifts::TransitionOutcome::Conflict { observed } if observed == to => {
            // Benign idempotent no-op — the shift is already at the target.
            // Unreachable by trace: this block runs on a FRESH `Sending → Sent`
            // Applied CAS, and a re-drive of a Sent doc short-circuits at the
            // 4-pre allowlist (`StateConflict`), so 4-b never re-fires the
            // edge.  Audited Info defensively.
            emit_shift_confirm_audit(
                tx,
                doc,
                "SHIFT_CONFIRM_EDGE_NOOP",
                Severity::Info,
                edge,
                &format!("already_at_target={observed:?}"),
            )
            .await
        }
        other => {
            // Real drift (`Conflict{other}` / `Forbidden` / `NotFound`) — e.g.
            // a boot-orphan quarantined the shift to `Error` and the doc then
            // re-sent.  Sent stands; recovery owns the shift row.
            emit_shift_confirm_audit(
                tx,
                doc,
                "SHIFT_CONFIRM_EDGE_DRIFT",
                Severity::Critical,
                edge,
                &format!("expected_from={from:?} observed={other:?}"),
            )
            .await
        }
    }
}

/// A′.1 piece 2 — forensic audit for a post-wire shift-confirm-edge outcome,
/// inside the 4-b `with_immediate` tx.  Emitting here (not rolling back)
/// preserves the record while the Sent commit stands.
async fn emit_shift_confirm_audit(
    tx: &mut crate::db::tx::WriteTxConn<'_>,
    doc: DocumentId,
    event: &str,
    severity: Severity,
    edge: &str,
    detail: &str,
) -> anyhow::Result<()> {
    let payload = serde_json::json!({ "edge": edge, "detail": detail }).to_string();
    audit_log::append_tx(
        tx,
        "fiscal_document",
        &format!("{doc:?}"),
        event,
        severity,
        None,
        Some(&payload),
    )
    .await?;
    Ok(())
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

            // ── W14a-2b Commit 5 MED-C5-1 fix (2026-05-20) ───────────
            //
            // Source-state allowlist gate FIRST — before signer guard,
            // SIGNED_XML read, envelope build, CAS.  Stale terminal /
            // out-of-band docs (SENT / KVT1 / Ack / Cancelled / etc.)
            // must surface as `StateConflict { observed }` regardless
            // of their shift_id / signed_by_cashier_id attribution.
            // Pre-fix ordering ran signer_guard first → forensic
            // noise: a stale SENT doc with NULL shift_id produced
            // `SIGNER_CASHIER_MISMATCH(ShiftMissingForFiscalDoc)`
            // instead of the correct StateConflict surface.
            //
            // CAS itself still fires later (after envelope build), but
            // the allowlist filter here is the structural gate.
            // CS-3 S7-1 (R2): `ErrorRetryable` is DROPPED from the source allowlist. Only a fresh
            // `Signed` / `OfflineLocalAck` may seed a `Sending` — an `ErrorRetryable` doc has already
            // consumed exactly one wire (all ER producers are post-wire routes), so re-seeding it here
            // would be a SECOND wire for an already-`CALL_STARTED` doc. It now surfaces as
            // `StateConflict`; the R6 convergence retires the redrive callers so it never bricks.
            if !matches!(
                inputs.state,
                DocState::Signed | DocState::OfflineLocalAck,
            ) {
                return Ok(PreOutcome::StateConflict {
                    observed: inputs.state,
                });
            }

            // ── STOP-O3-1 fix (v7) — wire-correctness fail-closed ────
            //
            // Refuse the online send of an OFFLINE-acquired doc that is
            // NOT yet offline-acked (the GO_ONLINE mode-race).  Only
            // possible when `offline_fiscal_no IS NULL` — drain-owned
            // offline docs carry `Some` and are excluded, online docs
            // are `fs_mode='ONLINE'` — so the extra read is skipped on
            // the hot path.  Read-only branch: no CAS / marker / wire;
            // the doc stays put for the offline lane / orphan recovery
            // (fix (b) / boot SW-2).  Placed AFTER the state-allowlist
            // and BEFORE the signer guard / envelope build / CAS so the
            // advance-at-SEND core is untouched.  SCOPED to the online
            // acquire-then-race states {Signed, ErrorRetryable}: an
            // `OfflineLocalAck` doc is the DRAIN's territory — its
            // NULL-offline_fiscal_no breach keeps the existing
            // `OfflineFiscalNoMissing` disposition below (unchanged).
            if matches!(inputs.state, DocState::Signed | DocState::ErrorRetryable)
                && inputs.offline_fiscal_no.is_none()
            {
                let fs_mode: Option<String> =
                    sqlx::query_scalar("SELECT fs_mode FROM fiscal_documents WHERE document_id = ?")
                        .bind(DbDocumentId(doc))
                        .fetch_one(&mut **tx)
                        .await?;
                if fs_mode.as_deref() == Some("OFFLINE") {
                    return Ok(PreOutcome::OfflineDocRoutedOnline);
                }
            }

            // ── W14a-2b Commit 5: signer-cashier enforcement ─────────
            //
            // Per spec §2.3 + §16.8: validate that
            // `inputs.signed_by_cashier_id == shift.opened_by_cashier_id`
            // (with full structural prerequisites — shift binding,
            // cross-FN, shift_id mismatch) BEFORE SIGNED_XML / envelope /
            // CAS / trace / `STAGE_SEND_INTENT_MARKED` / wire send.
            //
            // Ok-return contract (spec §2.5 + W14a-2a Round 7 §8.1):
            // refusal surfaces as `Ok(PreOutcome::SignerRefused)` — NOT
            // `Err` — so the surrounding `with_immediate` tx COMMITS
            // the `SIGNER_CASHIER_MISMATCH` audit row.  An `Err` return
            // would roll back the audit alongside any aborted state
            // mutation.
            //
            // ShiftClose / ZReport / ShiftOpen bypass per
            // `signer_guard` module contract (§16.9 senior-cashier seam
            // owns ShiftClose/ZReport validation; ShiftOpen has no
            // shift row pre-finalize).
            let shift_for_signer: Option<shifts::ShiftRow> = match inputs.shift_id {
                Some(sid) => shifts::get_tx(tx, sid).await?,
                None => None,
            };
            if let Err(mismatch) = signer_guard::enforce_signer_cashier_match(
                &inputs,
                shift_for_signer.as_ref(),
            ) {
                emit_signer_cashier_mismatch_audit_tx(tx, doc, &inputs, &mismatch).await?;
                return Ok(PreOutcome::SignerRefused(mismatch));
            }

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
                // B8-3: fail-closed guard — the opaque DPS code must be
                // present.  A NULL here is a producer-side breach (pre-B8
                // doc or raw-SQL bypass); emitting the integer ordinal
                // string would silently mis-identify the receipt to DPS.
                if inputs.offline_dps_code.is_none() {
                    return Ok(PreOutcome::EnvelopeBuildFailed(
                        StageSendError::OfflineDpsCodeMissing { document_id: doc },
                    ));
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
            // `allowed_transition` whitelist by M3b W6 (PR #55).
            //
            // MED-C5-1 fix (2026-05-20): the structural allowlist
            // filter now sits at the TOP of the 4-pre body so signer
            // guard / envelope build / CAS never run on stale terminal
            // docs (SENT / KVT1 / etc.).  At this point `inputs.state`
            // is one of the 3 allowed values by construction; the
            // `unreachable!()` arm guards against future filter drift.
            let source_state = match inputs.state {
                // CS-3 S7-1 (R2): ErrorRetryable dropped — kept in sync with the top allowlist so the
                // `unreachable!` stays honest (an ER doc surfaced as `StateConflict` above).
                DocState::Signed | DocState::OfflineLocalAck => inputs.state,
                other => unreachable!(
                    "S7-1 R2 invariant: top-of-4-pre allowlist already filtered \
                     non-{{Signed,OfflineLocalAck}}; got {other:?}"
                ),
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
                    // M1 item 4: a real `send_chk` submit, not a probe.
                    is_probe: false,
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

            // ── S7-1 §2.1 (P3): online-origin pre-wire predecessor equality ──
            // The FN chain seed MUST equal this doc's `previous_hash` BEFORE we mint the reservation —
            // moving the incumbent post-wire drift check to the ONLY safe side of the wire, in the
            // SAME `BEGIN IMMEDIATE` as the reservation insert + CALL_STARTED marker. Online-origin
            // only (offline follows the offline chain/cohort rules). A mismatch refuses with ZERO wire
            // (the whole authorize tx rolls back, so no reservation is minted).
            if inputs.offline_fiscal_no.is_none() {
                let ns = node_state::get_tx(tx, &inputs.fiscal_number)
                    .await?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "stage_send P3: node_state row missing for FN {} (doc {doc:?})",
                            inputs.fiscal_number
                        )
                    })?;
                anyhow::ensure!(
                    ns.last_known_unsigned_xml_sha256.as_ref().map(|h| h.as_slice())
                        == inputs.previous_hash.as_deref(),
                    "stage_send P3: online predecessor drift for doc {doc:?} — node seed != \
                     doc.previous_hash; refusing authorize (zero wire)"
                );
            }

            // ── S7-1: mint the delivery reservation in THIS tx (P2 Layer 2/3) ──
            // `authorize_submission` does the call-once `NOT EXISTS` + `RESERVED_NOT_STARTED` insert +
            // generation advance + `RN→CALL_STARTED` CAS, and returns the sealed non-`Clone`
            // `Authorization` that gates the sole wire. `envelope_hash` = the FULL wire hash (B1).
            let auth = crate::db::repositories::delivery_reservation::authorize_submission(
                tx,
                build_new_reservation(doc, &inputs, &envelope),
                &now_db_format(),
            )
            .await
            .map_err(anyhow::Error::new)?;

            Ok(PreOutcome::Marked {
                auth,
                envelope,
                attempt_no,
                doc_type: inputs.doc_type,
            })
        })
    })
    .await
    .map_err(bridge_anyhow)?;

    let (auth, envelope, attempt_no, doc_type) = match pre {
        PreOutcome::Marked {
            auth,
            envelope,
            attempt_no,
            doc_type,
        } => (auth, envelope, attempt_no, doc_type),
        PreOutcome::DocumentMissing => return Ok(StageSendOutcome::DocumentMissing),
        PreOutcome::SignedArtifactMissing => {
            return Err(StageSendError::SignedArtifactMissing { document_id: doc })
        }
        PreOutcome::EnvelopeBuildFailed(e) => return Err(e),
        // STOP-O3-1 fix (v7) — fail-closed: no wire, doc left untouched.
        PreOutcome::OfflineDocRoutedOnline => {
            return Err(StageSendError::OfflineDocRoutedOnline { document_id: doc })
        }
        PreOutcome::StateConflict { observed } => {
            return Ok(StageSendOutcome::StateConflict { observed })
        }
        PreOutcome::SignerRefused(mismatch) => {
            return Ok(StageSendOutcome::SignerRefused(mismatch))
        }
    };

    // ── PHASE 2 — WIRE (outside any lock): `submit_authorized` is the SOLE `send_chk_observed` ──
    // The wire RELOCATES here from the deleted legacy 4a: `submit_authorized` consumes the sealed
    // non-`Clone` `Authorization` by value, echoes the full binding + envelope hash (AO-2 + rebind),
    // fires EXACTLY ONE `send_chk_observed`, and surfaces the byte-identical legacy `Result` for the
    // drift-pin + trace/audit routing. P2 Layer 3: no wire-capable value survives the call.
    let port_binding = production_dps_binding();
    let wire_call_started_at = now_db_format();
    let (obs, legacy) = match super::submit::submit_authorized(
        dps_channel,
        &port_binding,
        auth,
        envelope,
        doc_type,
    )
    .await
    {
        Ok(v) => v,
        // Unreachable by construction (fixed binding + the SAME envelope moved to the wire). Fail
        // closed if it ever fires — the reservation is already `CALL_STARTED`, so boot normalizes it
        // (`resume_crashed_reservation`) without a re-wire (D-FIX-D; the STOP hardening is a follow-up).
        Err(refused) => {
            return Err(StageSendError::Internal(anyhow::anyhow!(
            "submit_authorized refused for doc {doc:?} (unreachable, fixed binding): {refused:?}"
        )))
        }
    };
    let wire_call_finished_at = now_db_format();

    // ── PHASE 3 setup — the TARGET authority (FIX-B2) ──
    // Forensics from the byte-identical legacy result (for the trace row + the public Routed surface).
    let wire_forensics: Option<(Option<i32>, &'static str, String)> = match &legacy {
        Ok(_) => None,
        Err(e) => Some(extract_wire_forensics(e)),
    };
    // Build the TARGET record arguments (classify axes + the lossless evidence discriminant + the
    // classifier). `record_outcome` persists these — the DURABLE authority.
    let (disc, outcome, classified) = build_record_args(&obs).map_err(StageSendError::Internal)?;
    // The demoted `route_dps_error` — the diagnostic overlay. Consulted by `wire_decision_from` ONLY to
    // preserve a wrapper-side bug's CRITICAL WrapperBug decision on the collapsed `NoResponse` leaf
    // (evidence + `WireDiagnostics` cannot rebuild `Internal`/`NotFound`/`QueryNotSupported`/
    // `ServerFiscalIdMismatch`). Derived from the byte-identical legacy result; NOT the wire authority.
    let wrapper_overlay = match &legacy {
        Err(e) => Some(route_dps_error(e, doc_type, true)),
        Ok(_) => None,
    };
    // CS-3 Slice E (Pin 2 — Track A): the `decision` that drives trace + audit + the returned outcome +
    // drain failure-class is the TOTAL projection of the DURABLE authority (the sealed evidence leaf +
    // the SAME classifier that built the record), so those observations can never disagree with the
    // durable record. The legacy `Result` remains ONLY for the forensic overlay (`wire_forensics`,
    // above), the wrapper-bug diagnostic overlay (above), and the raw legacy-vs-classifier drift-pin
    // (`grpc.rs`); it no longer AUTHORS the decision (`route_dps_error` / `target_wire_decision`
    // demoted — the empty-SFN `Sent{""}` → `OkButNoFiscalNumber` HELD leaf that `target_wire_decision`
    // used to reconcile is now the structural `OkButNoFiscalNumber` evidence leaf).
    let decision = wire_decision_from(&disc, &classified, wrapper_overlay.as_ref());
    // Capture the reservation id BEFORE `obs` is moved into the record transaction.
    let reservation_id = obs.reservation_id();

    // ── PHASE 3 — RECORD tx: record_outcome (axes + PENDING_APPLY + early STOP/BLOCKED) + trace + audit.
    record_transaction(
        pool,
        obs,
        outcome,
        disc,
        decision.clone(),
        wire_forensics.clone(),
        wire_call_started_at,
        wire_call_finished_at,
        doc,
        attempt_no,
    )
    .await?;

    // ── PHASE 4 — APPLY orchestration (shared live + boot): doc CAS / SFN / seed / shift + APPLIED.
    // A `HeldNotAutoRelease` is the EXPECTED result for a `-12`/`-6`/SubmittedUnknown routed leaf: the
    // doc rests `PENDING_APPLY` under STOP (set in record), and the outcome is surfaced below — it is
    // NOT an error (FIX-C). Real structural / DB errors propagate.
    match super::apply_orchestration::apply_recorded_outcome(pool, reservation_id).await {
        Ok(_) => {}
        Err(e) => match e
            .downcast_ref::<crate::db::repositories::delivery_reservation::ApplyError>()
        {
            Some(crate::db::repositories::delivery_reservation::ApplyError::HeldNotAutoRelease) => {
            }
            _ => return Err(StageSendError::Internal(e)),
        },
    }

    // The returned outcome is derived from the TARGET decision (FIX-B2), not the raw legacy.
    Ok(stage_send_outcome_from(
        decision,
        attempt_no,
        wire_forensics.as_ref(),
    ))
}

// ─── Unit tests for the pure surface ─────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::enums::{DocState, DocType};

    // ─── Pin 2 (Track A total projection) — the wire_decision_from authority tuple ───
    //
    // The single central-change guard (rev4 §4-step-2): `wire_decision_from` derives the wire
    // `WireDecision` from the sealed evidence leaf + the classifier — NEVER from the collapsed legacy
    // `DpsError`. This SUBSUMES the two former F3 pins
    // (`f3_remote_status_*` / `f3_unknown_nonzero_*`), which special-cased the two leaves whose
    // projection diverges from a naive legacy pass-through; here they are STRUCTURAL leaves of the
    // total function. Full-tuple asserts lock every `RoutingDecision` field so a silent drift in the
    // otherwise-unguarded central change reds. See `docs/CS3_SLICE_E_PIN2_LEAF_TUPLE_TABLE.md`.
    #[cfg(test)]
    fn pin_started(
        response: prro_domain::delivery::SendResponse,
    ) -> prro_domain::delivery::SubmissionEvidence {
        use prro_domain::delivery::{
            DpsProtocolBinding, DpsProtocolId, EnvelopeHash, ProtocolContractVersion,
            SubmissionEvidence,
        };
        SubmissionEvidence::Started {
            response,
            binding: DpsProtocolBinding {
                protocol_id: DpsProtocolId::FscoZzd,
                contract_version: ProtocolContractVersion(1),
                capability_profile_version: None,
                endpoint_config_revision: None,
            },
            envelope_hash: EnvelopeHash([0u8; 32]),
        }
    }

    #[test]
    fn wire_decision_from_locks_authority_tuple() {
        use crate::transports::dps::error::DpsError;
        use prro_domain::delivery::evidence::EvidenceDiscriminant;
        use prro_domain::delivery::{
            classify, ActiveRetryClass, DecodedResponseDigest, GrpcStatusDigest, NoResponseCause,
            NonOkStatusCode, RemoteStatusEvidence, SendOutcome, SendResponse,
        };

        // ── Divergent leaf A — RemoteAuthStatus → ProbeRequired/RemoteStatus ──
        // Durable authority: classify(RemoteAuthStatus) = ProbeRequired. Legacy `DpsError::RemoteStatus`
        // routes the COMPAT TransientRetry; the projection must reflect the DURABLE ProbeRequired.
        let ev_ra = pin_started(SendResponse::remote_status(
            RemoteStatusEvidence::RemoteAuthStatus(GrpcStatusDigest::from_transport_digest(
                [0xCD; 32],
            )),
        ));
        let cls_ra = classify(&ev_ra);
        assert_eq!(cls_ra.routing(), Some(ActiveRetryClass::ProbeRequired));
        let disc_ra = EvidenceDiscriminant::from_evidence(&ev_ra);
        let dec_ra = wire_decision_from(&disc_ra, &cls_ra, None);
        let d_ra = match &dec_ra {
            WireDecision::Routed(d) => d,
            WireDecision::Sent { .. } => panic!("RemoteAuthStatus must be Routed"),
        };
        assert_eq!(d_ra.target_state, DocState::ErrorRetryable);
        assert_eq!(d_ra.retry_class, RetryClass::ProbeRequired);
        assert_eq!(d_ra.audit_event, AuditEvent::StageSendProbeRequired);
        assert_eq!(d_ra.audit_severity, Severity::Warning);
        assert!(d_ra.node_mode_flip.is_none());
        assert_eq!(
            d_ra.probe_hint.as_ref().map(|h| h.reason),
            Some(ProbeReason::RemoteStatus),
            "RemoteAuthStatus carries the distinct RemoteStatus probe reason"
        );
        assert!(d_ra.mac_recovery_hint.is_none());
        // End-to-end: trace retry_class + returned outcome both agree with the DURABLE ProbeRequired.
        let fx_ra = (None, "Transport", "gateway".to_string());
        assert_eq!(
            build_attempt_completion(&dec_ra, Some(&fx_ra), "t0".into(), "t1".into())
                .retry_class
                .as_deref(),
            Some("ProbeRequired")
        );
        match stage_send_outcome_from(dec_ra, 1, Some(&fx_ra)) {
            StageSendOutcome::Routed { decision, .. } => {
                assert_eq!(decision.retry_class, RetryClass::ProbeRequired)
            }
            _ => panic!("returned outcome must be Routed"),
        }

        // ── Divergent leaf B — UnknownStatus (-17) → TransientRetry (Pin 2; ProbeRequired at Pin 3) ──
        // The total projection UNIFIES the two legacy sources for this leaf (`-17` decode→ProbeRequired
        // and `-4` indeterminate→TransientRetry) under the single durable `UnknownStatus → TransientRetry`.
        let ev_us = pin_started(SendResponse::parsed(SendOutcome::from_server_code(
            NonOkStatusCode::from_transport(-17).unwrap(),
            DocType::Sell,
            DecodedResponseDigest::from_transport_digest([0xAB; 32]),
        )));
        let cls_us = classify(&ev_us);
        assert_eq!(
            cls_us.routing(),
            Some(ActiveRetryClass::TransientRetry),
            "Pin 2: classifier still routes UnknownStatus → TransientRetry (Pin 3 flips it)"
        );
        let disc_us = EvidenceDiscriminant::from_evidence(&ev_us);
        let dec_us = wire_decision_from(&disc_us, &cls_us, None);
        let d_us = match &dec_us {
            WireDecision::Routed(d) => d,
            WireDecision::Sent { .. } => panic!("UnknownStatus must be Routed"),
        };
        assert_eq!(d_us.target_state, DocState::ErrorRetryable);
        assert_eq!(d_us.retry_class, RetryClass::TransientRetry);
        assert_eq!(d_us.audit_event, AuditEvent::StageSendTransientRetry);
        assert_eq!(d_us.audit_severity, Severity::Warning);
        assert!(d_us.node_mode_flip.is_none());
        assert!(
            d_us.probe_hint.is_none(),
            "Pin 2: UnknownStatus carries NO probe (TransientRetry); Pin 3 activates SubmittedUnknown"
        );
        assert!(d_us.mac_recovery_hint.is_none());
        let fx_us = (None, "Decode", "status=-17 unknown non-zero".to_string());
        assert_eq!(
            build_attempt_completion(&dec_us, Some(&fx_us), "t0".into(), "t1".into())
                .retry_class
                .as_deref(),
            Some("TransientRetry")
        );
        match stage_send_outcome_from(dec_us, 1, Some(&fx_us)) {
            StageSendOutcome::Routed { decision, .. } => {
                assert_eq!(decision.retry_class, RetryClass::TransientRetry)
            }
            _ => panic!("returned outcome must be Routed"),
        }

        // ── Reject with a node-mode flip — Offline168 (-11) → TerminalReject + Blocked ──
        // Locks the node_mode_flip source (classifier `node_effect==NodeBlocked → Some(Blocked)`) and
        // the per-verdict Critical severity (a TerminalReject leaf), both authority-side (disc+cls).
        let ev_ob = pin_started(SendResponse::parsed(SendOutcome::from_server_code(
            NonOkStatusCode::from_transport(-11).unwrap(),
            DocType::Sell,
            DecodedResponseDigest::from_transport_digest([0x11; 32]),
        )));
        let cls_ob = classify(&ev_ob);
        let disc_ob = EvidenceDiscriminant::from_evidence(&ev_ob);
        let d_ob = match wire_decision_from(&disc_ob, &cls_ob, None) {
            WireDecision::Routed(d) => d,
            WireDecision::Sent { .. } => panic!("Offline168 must be Routed"),
        };
        assert_eq!(d_ob.target_state, DocState::Rejected);
        assert_eq!(d_ob.retry_class, RetryClass::TerminalReject);
        assert_eq!(d_ob.audit_event, AuditEvent::StageSendNodeBlocked);
        assert_eq!(d_ob.audit_severity, Severity::Critical);
        assert_eq!(
            d_ob.node_mode_flip,
            Some(crate::db::models::enums::NodeMode::Blocked)
        );
        assert!(d_ob.probe_hint.is_none());

        // ── NoResponse leaf + WrapperBug overlay — the demoted route_dps_error is preserved VERBATIM ──
        // The classifier collapses a wrapper-side bug (`Internal`/…) into NoResponse → TransientRetry;
        // its CRITICAL WrapperBug diagnostic cannot be rebuilt from evidence, so it rides the overlay
        // (S7-1 SENDING-held wrapper contract; integration fx06–fx09). A NON-wrapper overlay (Transport)
        // does NOT apply — the leaf keeps the classifier's TransientRetry.
        let ev_nr = pin_started(SendResponse::no_response(
            NoResponseCause::CallFailedWithoutTrustedDpsEnvelope,
        ));
        let cls_nr = classify(&ev_nr);
        assert_eq!(cls_nr.routing(), Some(ActiveRetryClass::TransientRetry));
        let disc_nr = EvidenceDiscriminant::from_evidence(&ev_nr);
        let wrapper = route_dps_error(
            &DpsError::Internal("wrapper bug".into()),
            DocType::Sell,
            true,
        );
        assert_eq!(wrapper.retry_class, RetryClass::WrapperBug);
        let d_nr = match wire_decision_from(&disc_nr, &cls_nr, Some(&wrapper)) {
            WireDecision::Routed(d) => d,
            WireDecision::Sent { .. } => panic!("NoResponse must be Routed"),
        };
        assert_eq!(
            d_nr.retry_class,
            RetryClass::WrapperBug,
            "wrapper-bug overlay preserved VERBATIM on the collapsed NoResponse leaf"
        );
        assert_eq!(d_nr.audit_event, AuditEvent::StageSendWrapperBug);
        assert_eq!(d_nr.audit_severity, Severity::Critical);
        // A ServerFiscalIdMismatch overlay preserves its DISTINCT forensic audit event.
        let mismatch = route_dps_error(
            &DpsError::ServerFiscalIdMismatch {
                expected_id: "A".into(),
                actual_id: "B".into(),
            },
            DocType::Sell,
            true,
        );
        match wire_decision_from(&disc_nr, &cls_nr, Some(&mismatch)) {
            WireDecision::Routed(d) => {
                assert_eq!(d.retry_class, RetryClass::WrapperBug);
                assert_eq!(d.audit_event, AuditEvent::StageSendFiscalIdMismatch);
            }
            WireDecision::Sent { .. } => panic!("must be Routed"),
        }
        // Transport overlay → NOT preserved; the leaf keeps the classifier's TransientRetry.
        let transport = route_dps_error(&DpsError::Transport("reset".into()), DocType::Sell, true);
        match wire_decision_from(&disc_nr, &cls_nr, Some(&transport)) {
            WireDecision::Routed(d) => assert_eq!(
                d.retry_class,
                RetryClass::TransientRetry,
                "Transport overlay is non-WrapperBug → classifier TransientRetry"
            ),
            WireDecision::Sent { .. } => panic!("must be Routed"),
        }

        // (The happy `Accepted → Sent{fiscal_no}` arm — SFN threaded from `disc::Accepted`, not the
        // legacy `CheckAck` — is covered by `route_send_result_ok_arm_yields_sent` +
        // `s7_target_wire_decision_maps_empty_sfn_to_held_not_sent`.)
    }

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
            // W14a-2b Commit 1: SendInputs gained document_id /
            // shift_id / signed_by_cashier_id; plumbing-only here —
            // helper is not yet consuming them.
            document_id: crate::db::models::ids::DocumentId::new(),
            shift_id: None,
            signed_by_cashier_id: None,
            // A.3 — chain fields; envelope-build tests don't exercise the advance.
            previous_hash: None,
            unsigned_xml_sha256: None,
            mac_recovery_attempts: 0,
            // B8-3: online test docs have no offline_dps_code.
            offline_dps_code: None,
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

    // ─── B10: offline-session boundary docs (DocType 9/10) ────────────
    #[test]
    fn build_envelope_offline_session_begin_uses_servicechk_and_passes_lnd() {
        // DocType=9 BEGIN → typCheck ServiceChk(3), same as SHIFT_OPEN;
        // BUT (unlike SHIFT_OPEN) it does NOT force local_number to 0 —
        // it is an ordinary lnd-numbered offline doc.  It also carries an
        // offline code → non-empty id_offline (proven separately).
        let mut i = inputs(DocType::OfflineSessionBegin, 7, "2026-05-09T12:34:56Z");
        i.offline_dps_code = Some("BEGIN-CODE".into());
        let env = build_send_envelope(&i, b"PAY".to_vec()).expect("BEGIN must build");
        assert_eq!(
            env.check_type,
            DpsCheckType::ServiceChk,
            "OFFLINE_SESSION_BEGIN must map to ServiceChk(3)"
        );
        assert_eq!(
            env.local_number, 7,
            "BEGIN keeps its lnd (NOT forced to 0 like SHIFT_OPEN)"
        );
        assert_eq!(
            env.id_offline, "BEGIN-CODE",
            "BEGIN is an offline doc → id_offline carries its DPS code"
        );
    }

    #[test]
    fn build_envelope_offline_session_end_uses_servicechk_and_passes_lnd() {
        let mut i = inputs(DocType::OfflineSessionEnd, 8, "2026-05-09T12:34:56Z");
        i.offline_dps_code = Some("END-CODE".into());
        let env = build_send_envelope(&i, b"PAY".to_vec()).expect("END must build");
        assert_eq!(
            env.check_type,
            DpsCheckType::ServiceChk,
            "OFFLINE_SESSION_END must map to ServiceChk(3)"
        );
        assert_eq!(env.local_number, 8, "END keeps its lnd");
        assert_eq!(env.id_offline, "END-CODE");
    }

    #[test]
    fn build_envelope_offline_drain_uses_raw_signed_ts_across_kyiv_dst() {
        for (business_ts, expected) in [
            ("2026-07-15T10:00:00Z", 20_260_715_130_000_i64),
            ("2026-01-15T10:00:00Z", 20_260_115_120_000_i64),
        ] {
            let mut i = inputs(DocType::Sell, 9, business_ts);
            i.offline_fiscal_no = Some(42);
            i.offline_dps_code = Some("OFFLINE-CODE".into());
            let env = build_send_envelope(&i, b"PAY".to_vec())
                .expect("offline drain envelope must build");
            assert_eq!(
                env.date_time, expected,
                "offline {business_ts} must preserve signed XML <TS>"
            );
        }
    }

    #[test]
    fn build_envelope_offline_boundaries_use_raw_signed_ts_even_when_end_is_online_shaped() {
        for doc_type in [DocType::OfflineSessionBegin, DocType::OfflineSessionEnd] {
            let env = build_send_envelope(
                &inputs(doc_type, 9, "2026-05-09T12:34:56Z"),
                b"PAY".to_vec(),
            )
            .expect("offline boundary envelope must build");
            assert_eq!(
                env.date_time, 20_260_509_153_456,
                "{doc_type:?} must preserve signed XML <TS>"
            );
        }
    }

    #[test]
    fn build_envelope_unsupported_doc_type_fails_closed() {
        // ServiceIn/Out are wired (L3); only CashWithdrawal/XReport stay unsupported.
        for dt in [DocType::CashWithdrawal, DocType::XReport] {
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

    // DPS `-8` FIX (live-proven 2026-07-12, docs/DPS_MINUS8_DATE_AND_SHIFT_RECOVERY.md):
    // the ONLINE envelope `Check.date_time` MUST equal the signed `<TS>`
    // (`kyiv_comp_date` = Kyiv-local `YYYYMMDDHHMMSS` int) — NOT a Unix epoch.
    // The old `kyiv_local_epoch` produced a Kyiv-wall-clock-as-UTC epoch = +3h
    // future vs the DPS UTC clock → envelope-date ≠ `<TS>` → DPS `-8`
    // "дата не відповідає Check.date" on online shift-close. WebCheck sends the
    // 14-digit `<TS>` int for `Check.date` (offline AND online). These pins
    // encode the correct (comp_date) contract; they RED against the pre-fix
    // epoch code.
    #[test]
    fn build_envelope_online_date_time_equals_ts_summer_dst() {
        // 2026-07-15T10:00:00Z → Kyiv 13:00:00 (EEST, UTC+3) → <TS> 20260715130000.
        let env = build_send_envelope(
            &inputs(DocType::Sell, 1, "2026-07-15T10:00:00Z"),
            b"PAY".to_vec(),
        )
        .expect("summer build must succeed");
        assert_eq!(
            env.date_time, 20_260_715_130_000,
            "online date_time must equal the <TS> YYYYMMDDHHMMSS (comp_date), NOT an epoch"
        );
    }

    #[test]
    fn build_envelope_online_date_time_equals_ts_winter() {
        // 2026-01-15T10:00:00Z → Kyiv 12:00:00 (EET, UTC+2) → <TS> 20260115120000.
        let env = build_send_envelope(
            &inputs(DocType::Sell, 1, "2026-01-15T10:00:00Z"),
            b"PAY".to_vec(),
        )
        .expect("winter build must succeed");
        assert_eq!(
            env.date_time, 20_260_115_120_000,
            "online date_time must equal the <TS> YYYYMMDDHHMMSS (comp_date), NOT an epoch"
        );
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
    fn extract_wire_forensics_remote_status_and_indeterminate_project_to_transport_ra() {
        // RA all-consumers pin (compatibility projection): R1 `RemoteStatus` and slice-A
        // `Indeterminate` keep the SAME forensic tuple as `Transport` — (None, "Transport",
        // message) — because `transport_trace.error_kind`/`outcome_kind` are CHECK-constrained.
        // This is a compatibility projection, not an ideal classification.
        let rs = extract_wire_forensics(&DpsError::RemoteStatus {
            code: "Unauthenticated".into(),
            message: "creds rejected".into(),
            digest: prro_domain::delivery::GrpcStatusDigest::from_transport_digest([0xAB; 32]),
        });
        assert_eq!(rs, (None, "Transport", "creds rejected".to_string()));
        let ind = extract_wire_forensics(&DpsError::Indeterminate {
            code: -4,
            message: "server unknown".into(),
            digest: prro_domain::delivery::DecodedResponseDigest::from_transport_digest([0xAB; 32]),
        });
        assert_eq!(ind, (None, "Transport", "server unknown".to_string()));
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
