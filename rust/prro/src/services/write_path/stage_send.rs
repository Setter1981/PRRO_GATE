//! Stage 4 — send (Pattern B with SENDING marker).
//!
//! W7.3 lands the pure-Rust pre-flight surface: typed errors,
//! `build_send_envelope`, `classify_send_outcome`, and
//! `SendOutcome::trace_kind`.  The full 3-segment Pattern B worker
//! step (4-pre / 4a / 4b) lives in W7.4 in this same module.
//!
//! Anchored on:
//!   - W7 design freeze §4.3 (envelope builder; classify minimal table)
//!   - ADR-M3-A2 (Z-allocation by `wire_artifact_kind`)
//!   - ADR-M3-A6 (DpsError routing scaffold; full table is W10)
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
    transport_trace::{self, AttemptCompletion, NewAttempt},
};
use crate::db::tx::with_immediate;
use crate::transports::dps::channel::DpsChannel;
use crate::transports::dps::dto::{CheckAck, CheckEnvelope, DpsCheckType};
use crate::transports::dps::error::{AuthorizationKind, DpsError};
use crate::transports::dps::gen;

use super::stage_sign::{derive_wire_artifact_kind, SignError, WireArtifactKind};

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
    /// returned a non-`Applied` outcome.  Impossible under M3a W5
    /// single-writer-per-FN + the 4-pre marker we just committed:
    /// the doc was in `Sending` after our 4-pre tx, no other writer
    /// can mutate it, so the post-wire CAS cannot miss.  Typed error
    /// for forensics if it ever happens.
    #[error("stage 4 post-wire CAS Sending->{target:?} on doc {document_id:?}: {observed:?}")]
    PostWireCasFailed {
        document_id: DocumentId,
        target: DocState,
        observed: TransitionOutcome,
    },

    /// `mark_submission_attempted_tx` returned `false` in 4-pre AFTER
    /// the CAS `Signed → Sending` succeeded.  Also impossible under
    /// the single-writer invariant: CAS Applied means the row exists
    /// for the duration of the same `with_immediate` envelope.
    #[error("stage 4 mark_submission_attempted_tx returned 0 for doc {document_id:?} after CAS Applied")]
    MarkSubmissionAttemptedMissing { document_id: DocumentId },

    /// `set_server_fiscal_no_tx` returned `false` in 4-b AFTER the
    /// CAS `Sending → Sent` succeeded.  Same invariant breach class
    /// as `PostWireCasFailed`.
    #[error("stage 4 set_server_fiscal_no_tx returned 0 for doc {document_id:?} after CAS Applied")]
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
/// **`id_offline` / `id_cancel`.**  Empty strings for the W7
/// happy path — DPS interprets empty `id_offline` as "online" and
/// empty `id_cancel` as "not a cancellation".  Offline wiring lands
/// in W11; the cancel slice is future work.
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
        _ => i32::try_from(inputs.lnd).map_err(|_| StageSendError::LndOutOfRangeI32 {
            lnd: inputs.lnd,
        })?,
    };

    let date_time = kyiv_local_epoch(&inputs.business_ts)?;

    Ok(CheckEnvelope {
        rro_fn: inputs.fiscal_number.clone(),
        date_time,
        check_sign: signed_payload,
        local_number,
        check_type,
        id_offline: String::new(),
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

// ─── classify_send_outcome ───────────────────────────────────────────

/// W7-minimal outcome of the wire `send_chk` call.  Carries just
/// enough to drive (a) the 4-b CAS target state, (b) the
/// `transport_trace::OutcomeKind` mapping, and (c) the
/// `set_server_fiscal_no_tx` write on the success branch.
///
/// **No `Kvt1` variant in W7.**  Inline KVT1 piggyback is W8
/// territory; W7 fixtures intentionally do not exercise that path.
///
/// **No `StateConflict` variant here.**  StateConflict is observed
/// from `transition_state(Signed, Sending)` in 4-pre, BEFORE the
/// wire call.  Classify only sees post-wire results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendOutcome {
    /// DPS returned OK; `server_fiscal_no` is the assigned fiscal id
    /// (`CheckAck.id`).  4-b transitions `Sending → Sent` and writes
    /// `server_fiscal_no` to `fiscal_documents`.
    Sent { server_fiscal_no: String },
    /// Terminal per-document reject (W7 minimal: only
    /// `Authorization{DocumentReject}`).  4-b transitions
    /// `Sending → Rejected`.
    Rejected { code: i32, message: String },
    /// Transient failure — 4-b transitions `Sending → ErrorRetryable`
    /// and the worker re-enters via `(ErrorRetryable, Sending)` next
    /// tick (per ADR-M3-A9 step 5-6 retry path).
    Retryable { reason: RetryableReason },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryableReason {
    /// gRPC transport-level failure (TCP/TLS/DNS/per-call deadline).
    Transport(String),
    /// DPS replied with a non-OK application status code.  W7 is
    /// conservative: ALL `DpsError::Server` shapes route here.  W10
    /// dispatch table will split per `code` (e.g. ERROR_NOT_OPEN_SHIFT
    /// vs ERROR_BAD_HASH_PREV vs ...) into terminal vs transient.
    Server { code: i32, message: String },
    /// `Authorization{FiscalNumberNotRegistered}` (codes -13/-14).
    /// Per ADR-M3-A6 prereq + W0-3 §2.1 this routes through
    /// `ErrorRetryable → RequiresManualReconciliation` once the W10
    /// dispatch table lands; W7 stops at `ErrorRetryable`.
    AuthorizationFnNotRegistered { code: i32, message: String },
}

impl SendOutcome {
    /// Map a post-wire outcome to the `transport_trace.outcome_kind`
    /// CHECK-list value persisted by `complete_tx` in 4-b.
    pub fn trace_kind(&self) -> OutcomeKind {
        match self {
            SendOutcome::Sent { .. } => OutcomeKind::Ok,
            SendOutcome::Rejected { .. } => OutcomeKind::Rejected,
            SendOutcome::Retryable {
                reason: RetryableReason::Transport(_),
            } => OutcomeKind::RetryableTransport,
            SendOutcome::Retryable {
                reason: RetryableReason::Server { .. },
            } => OutcomeKind::RetryableServer,
            SendOutcome::Retryable {
                reason: RetryableReason::AuthorizationFnNotRegistered { .. },
            } => OutcomeKind::RetryableAuthFn,
        }
    }
}

/// Classify the result of `dps_channel.send_chk(envelope).await` into
/// a typed `SendOutcome`.  Pure function — no I/O.
///
/// **W7 minimal table.**  Per W7 freeze §4.3:
/// - `Ok(ack)`                                                 → `Sent { server_fiscal_no = ack.id }`
/// - `Err(Transport(..))`                                      → `Retryable::Transport`
/// - `Err(Server { .. })`                                      → `Retryable::Server` (W7 conservative; W10 splits)
/// - `Err(Authorization { DocumentReject, .. })`               → `Rejected`
/// - `Err(Authorization { FiscalNumberNotRegistered, .. })`    → `Retryable::AuthorizationFnNotRegistered`
/// - `Err(other shapes)` (`Decode`, `NotFound`, `ServerFiscalIdMismatch`,
///   `QueryNotSupported`, `Internal`) → `Retryable::Transport`.  These
///   shapes are not part of the documented `send_chk` success/failure
///   contract; conservatively retrying is safer than classifying as
///   terminal reject.  W10 may refine.
pub fn classify_send_outcome(r: Result<CheckAck, DpsError>) -> SendOutcome {
    match r {
        Ok(ack) => SendOutcome::Sent {
            server_fiscal_no: ack.id,
        },
        Err(DpsError::Transport(msg)) => SendOutcome::Retryable {
            reason: RetryableReason::Transport(msg),
        },
        Err(DpsError::Server { code, message }) => SendOutcome::Retryable {
            reason: RetryableReason::Server { code, message },
        },
        Err(DpsError::Authorization {
            code,
            kind: AuthorizationKind::DocumentReject,
            message,
        }) => SendOutcome::Rejected { code, message },
        Err(DpsError::Authorization {
            code,
            kind: AuthorizationKind::FiscalNumberNotRegistered,
            message,
        }) => SendOutcome::Retryable {
            reason: RetryableReason::AuthorizationFnNotRegistered { code, message },
        },
        Err(other) => SendOutcome::Retryable {
            reason: RetryableReason::Transport(format!(
                "unexpected DpsError on send_chk: {other}"
            )),
        },
    }
}

// ─── Stage outcome (worker dispatcher contract) ─────────────────────

/// Top-level outcome of [`run`].  Five variants cover the full Pattern
/// B stage 4 surface as observed by the worker dispatcher:
///
///   - `Sent` / `Rejected` / `Retryable` — wire `send_chk` returned and
///     4-b CAS persisted the result.  `attempt_no` correlates with the
///     `transport_trace` row.
///   - `StateConflict` — 4-pre CAS `Signed → Sending` missed: the doc
///     was already past `Signed` (e.g. `Sent` from a prior worker, or
///     transitioned to a non-Signed state by reconciliation).  Stage
///     4 did NOT call `send_chk`.  Idempotent re-entry — the
///     dispatcher should NOT treat this as failure.
///   - `DocumentMissing` — the `(doc_id)` row was not present at 4-pre
///     read.  Race with a delete (offline reconciliation, manual
///     operator action).  Stage 4 did NOT call `send_chk`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageSendOutcome {
    Sent {
        server_fiscal_no: String,
        attempt_no: i32,
    },
    Rejected {
        code: i32,
        message: String,
        attempt_no: i32,
    },
    Retryable {
        reason: RetryableReason,
        attempt_no: i32,
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
    /// CAS `Signed → Sending` applied; trace row allocated; audit
    /// written.  Wire send is the next step (4a, no lock).
    Marked {
        envelope: CheckEnvelope,
        attempt_no: i32,
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
    /// CAS `Signed → Sending` returned `Conflict`: the doc was not
    /// in `Signed` at the time of the CAS.  No marker, no trace,
    /// no audit, no wire.
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
/// typed `StageSendError`.  Mirrors stage_sign's `bridge_anyhow`
/// pattern: typed errors thrown via `anyhow::Error::new(StageSendError::...)`
/// inside closures round-trip cleanly; raw `sqlx::Error` becomes
/// `StageSendError::Db`; everything else surfaces as `Internal`
/// preserving the cause chain.
fn bridge_anyhow(e: anyhow::Error) -> StageSendError {
    match e.downcast::<StageSendError>() {
        Ok(typed) => typed,
        Err(rest) => match rest.downcast::<sqlx::Error>() {
            Ok(sqlx_err) => StageSendError::Db(sqlx_err),
            Err(other) => StageSendError::Internal(other),
        },
    }
}

/// Build the `AttemptCompletion` payload for `transport_trace::complete_tx`.
/// The `outcome_kind` ↔ `error_kind` shape mirrors the W7-frozen
/// CHECK list in migration 010; kind / message present iff the
/// outcome is non-OK.
fn build_attempt_completion(
    outcome: &SendOutcome,
    wire_call_started_at: String,
    wire_call_finished_at: String,
) -> AttemptCompletion {
    let server_fiscal_no = match outcome {
        SendOutcome::Sent { server_fiscal_no } => Some(server_fiscal_no.clone()),
        _ => None,
    };
    let server_status_code = match outcome {
        SendOutcome::Rejected { code, .. } => Some(*code),
        SendOutcome::Retryable {
            reason: RetryableReason::Server { code, .. },
        } => Some(*code),
        SendOutcome::Retryable {
            reason: RetryableReason::AuthorizationFnNotRegistered { code, .. },
        } => Some(*code),
        _ => None,
    };
    let (error_kind, error_message) = match outcome {
        SendOutcome::Sent { .. } => (None, None),
        SendOutcome::Rejected { message, .. } => {
            (Some("AuthorizationDocumentReject".into()), Some(message.clone()))
        }
        SendOutcome::Retryable {
            reason: RetryableReason::Transport(msg),
        } => (Some("Transport".into()), Some(msg.clone())),
        SendOutcome::Retryable {
            reason: RetryableReason::Server { message, .. },
        } => (Some("Server".into()), Some(message.clone())),
        SendOutcome::Retryable {
            reason: RetryableReason::AuthorizationFnNotRegistered { message, .. },
        } => (
            Some("AuthorizationFnNotRegistered".into()),
            Some(truncate_msg(message)),
        ),
    };
    AttemptCompletion {
        wire_call_started_at,
        wire_call_finished_at,
        outcome_kind: outcome.trace_kind(),
        server_fiscal_no,
        server_status_code,
        error_kind,
        error_message: error_message.map(|m| truncate_msg(&m)),
    }
}

/// transport_trace.error_message has CHECK length <= 512.  Truncate
/// upstream so an oversized DPS message doesn't trip the CHECK at
/// 4-b commit time and roll back the entire 4-b tx.
fn truncate_msg(s: &str) -> String {
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
///     `Sending → {Sent | Rejected | ErrorRetryable}` (target
///     derived from `classify_send_outcome`), conditional
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

            // Build envelope BEFORE CAS — fail-closed on
            // UnsupportedDocType / Lnd / TS without writing SENDING.
            let envelope = match build_send_envelope(&inputs, signed_payload) {
                Ok(e) => e,
                Err(err) => return Ok(PreOutcome::EnvelopeBuildFailed(err)),
            };

            // CAS Signed -> Sending.  Whitelist guarantees `Forbidden`
            // is unreachable for this transition.
            match fd::transition_state(tx, doc, DocState::Signed, DocState::Sending).await? {
                TransitionOutcome::Applied => {}
                TransitionOutcome::Conflict => {
                    return Ok(PreOutcome::StateConflict {
                        observed: inputs.state,
                    });
                }
                TransitionOutcome::NotFound => return Ok(PreOutcome::DocumentMissing),
                TransitionOutcome::Forbidden => {
                    unreachable!("(Signed,Sending) is whitelisted in fiscal_documents::allowed_transition")
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
            })
        })
    })
    .await
    .map_err(bridge_anyhow)?;

    let (envelope, attempt_no) = match pre {
        PreOutcome::Marked {
            envelope,
            attempt_no,
        } => (envelope, attempt_no),
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
    let outcome = classify_send_outcome(wire_result);

    // EmptyServerFiscalNo guard (LOW risk close from W7.3 review).
    // The transport_trace OK-CHECK would otherwise reject 4-b commit
    // and roll back the entire 4-b tx (losing the audit and
    // CAS-Sending->Sent in the process); catching here lets the
    // doc stay cleanly in `Sending` for W9 reconciliation.
    if let SendOutcome::Sent { server_fiscal_no } = &outcome {
        if server_fiscal_no.is_empty() {
            return Err(StageSendError::EmptyServerFiscalNo { document_id: doc });
        }
    }

    // ── 4b ───────────────────────────────────────────────────────────
    let outcome_for_closure = outcome.clone();
    let started_for_closure = wire_call_started_at;
    let finished_for_closure = wire_call_finished_at;
    with_immediate(pool, move |tx| {
        let outcome = outcome_for_closure;
        let started = started_for_closure;
        let finished = finished_for_closure;
        Box::pin(async move {
            let target = match &outcome {
                SendOutcome::Sent { .. } => DocState::Sent,
                SendOutcome::Rejected { .. } => DocState::Rejected,
                SendOutcome::Retryable { .. } => DocState::ErrorRetryable,
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
            if let SendOutcome::Sent { server_fiscal_no } = &outcome {
                if !fd::set_server_fiscal_no_tx(tx, doc, server_fiscal_no).await? {
                    return Err(anyhow::Error::new(
                        StageSendError::SetServerFiscalNoMissing { document_id: doc },
                    ));
                }
            }

            // Complete trace row.  rows_affected == 0 ⇒ typed error
            // (W7.1 append-then-complete contract).
            let completion = build_attempt_completion(&outcome, started, finished);
            let rows = transport_trace::complete_tx(tx, doc, attempt_no, completion).await?;
            if rows == 0 {
                return Err(anyhow::Error::new(
                    StageSendError::TraceMissingAtComplete {
                        document_id: doc,
                        attempt_no,
                    },
                ));
            }

            // Audit STAGE_SEND_RESULT.
            let payload = serde_json::json!({
                "attempt_no": attempt_no,
                "outcome_kind": outcome.trace_kind().as_str(),
            })
            .to_string();
            audit_log::append_tx(
                tx,
                "fiscal_document",
                &format!("{doc:?}"),
                "STAGE_SEND_RESULT",
                Severity::Info,
                None,
                Some(&payload),
            )
            .await?;

            Ok::<_, anyhow::Error>(())
        })
    })
    .await
    .map_err(bridge_anyhow)?;

    Ok(match outcome {
        SendOutcome::Sent { server_fiscal_no } => StageSendOutcome::Sent {
            server_fiscal_no,
            attempt_no,
        },
        SendOutcome::Rejected { code, message } => StageSendOutcome::Rejected {
            code,
            message,
            attempt_no,
        },
        SendOutcome::Retryable { reason } => StageSendOutcome::Retryable {
            reason,
            attempt_no,
        },
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
        }
    }

    // ─── build_send_envelope ────────────────────────────────────────

    #[test]
    fn build_envelope_sell_passes_lnd_and_chk() {
        let env = build_send_envelope(&inputs(DocType::Sell, 42, "2026-05-09T12:34:56Z"), b"PAY".to_vec())
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
            &inputs(DocType::ShiftOpen, (i32::MAX as i64) + 1, "2026-05-09T12:34:56Z"),
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
        let env =
            build_send_envelope(&inputs(DocType::Sell, 1, "2026-07-15T10:00:00Z"), b"PAY".to_vec())
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
        let env =
            build_send_envelope(&inputs(DocType::Sell, 1, "2026-01-15T10:00:00Z"), b"PAY".to_vec())
                .expect("winter build must succeed");
        let expected = Utc
            .with_ymd_and_hms(2026, 1, 15, 12, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(env.date_time, expected, "winter offset must be +2h");
    }

    // ─── classify_send_outcome ──────────────────────────────────────

    fn ack(id: &str) -> CheckAck {
        CheckAck {
            id: id.into(),
            id_sign: vec![],
            data_sign: vec![],
        }
    }

    #[test]
    fn classify_ok_yields_sent_with_server_fiscal_no() {
        let out = classify_send_outcome(Ok(ack("DPS-FN-1")));
        assert_eq!(
            out,
            SendOutcome::Sent {
                server_fiscal_no: "DPS-FN-1".into()
            }
        );
        assert_eq!(out.trace_kind(), OutcomeKind::Ok);
    }

    #[test]
    fn classify_transport_yields_retryable_transport() {
        let out = classify_send_outcome(Err(DpsError::Transport("TLS reset".into())));
        let SendOutcome::Retryable {
            reason: RetryableReason::Transport(msg),
        } = out.clone()
        else {
            panic!("expected Retryable::Transport, got {out:?}");
        };
        assert_eq!(msg, "TLS reset");
        assert_eq!(out.trace_kind(), OutcomeKind::RetryableTransport);
    }

    #[test]
    fn classify_server_yields_retryable_server_w7_conservative() {
        let out = classify_send_outcome(Err(DpsError::Server {
            code: -7,
            message: "ERROR_NOT_OPEN_SHIFT".into(),
        }));
        let SendOutcome::Retryable {
            reason: RetryableReason::Server { code, message },
        } = out.clone()
        else {
            panic!("expected Retryable::Server, got {out:?}");
        };
        assert_eq!(code, -7);
        assert_eq!(message, "ERROR_NOT_OPEN_SHIFT");
        assert_eq!(out.trace_kind(), OutcomeKind::RetryableServer);
    }

    #[test]
    fn classify_authorization_document_reject_yields_terminal_rejected() {
        let out = classify_send_outcome(Err(DpsError::Authorization {
            code: -1,
            kind: AuthorizationKind::DocumentReject,
            message: "ERROR_VEREFY".into(),
        }));
        assert_eq!(
            out,
            SendOutcome::Rejected {
                code: -1,
                message: "ERROR_VEREFY".into()
            }
        );
        assert_eq!(out.trace_kind(), OutcomeKind::Rejected);
    }

    #[test]
    fn classify_authorization_fn_not_registered_yields_retryable_auth_fn() {
        let out = classify_send_outcome(Err(DpsError::Authorization {
            code: -13,
            kind: AuthorizationKind::FiscalNumberNotRegistered,
            message: "ERROR_NOT_REGISTERED_RRO".into(),
        }));
        let SendOutcome::Retryable {
            reason: RetryableReason::AuthorizationFnNotRegistered { code, message },
        } = out.clone()
        else {
            panic!("expected Retryable::AuthorizationFnNotRegistered, got {out:?}");
        };
        assert_eq!(code, -13);
        assert_eq!(message, "ERROR_NOT_REGISTERED_RRO");
        assert_eq!(out.trace_kind(), OutcomeKind::RetryableAuthFn);
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
        assert_eq!(s.len(), 19, "expected 'YYYY-MM-DD HH:MM:SS' shape, got {s:?}");
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

    // ─── classify_send_outcome ──────────────────────────────────────

    #[test]
    fn classify_unexpected_dps_error_shapes_fall_back_to_retryable_transport() {
        // None of these are part of the documented send_chk
        // success/failure contract; conservative classifier routes
        // them all to Retryable::Transport so the doc lands in
        // ErrorRetryable rather than terminal Rejected.
        for err in [
            DpsError::Decode("malformed".into()),
            DpsError::NotFound,
            DpsError::ServerFiscalIdMismatch {
                expected_id: "A".into(),
                actual_id: "B".into(),
            },
            DpsError::QueryNotSupported("ByLocalIdentity"),
            DpsError::Internal("wrapper bug".into()),
        ] {
            let out = classify_send_outcome(Err(err));
            assert_eq!(
                out.trace_kind(),
                OutcomeKind::RetryableTransport,
                "expected RetryableTransport for unexpected DpsError shape: {out:?}"
            );
        }
    }
}
