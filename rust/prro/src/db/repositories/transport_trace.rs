//! Repository for `transport_trace`.
//!
//! Schema: `rust/prro/migrations/010_transport_trace.sql` — append-then-update
//! per W7 design freeze §3 (Option A).  Two write entry points:
//!
//!   - [`allocate_and_insert_tx`] runs in the **stage 4-pre** lock.  It
//!     allocates a monotonic `attempt_no` per `document_id` and INSERTs
//!     a row with intent data (timing/start, profiles, request envelope
//!     hash).  Completion columns are left NULL.
//!   - [`complete_tx`] runs in the **stage 4-b** lock.  It UPDATEs the
//!     same row with wire timing, outcome classification, and any
//!     server-side identifiers / error data.
//!
//! Crash-window forensics: a row whose `completed_at IS NULL` is the
//! durable signal that an attempt did not complete cleanly.  W9
//! App::boot reconciliation reads `transport_trace` for forensic
//! correlation but never re-issues the wire `send_chk` for those
//! attempts (Pattern B safety; see ADR-M3-A9 step 5-6 + the W7 freeze).
//!
//! Repo policy:
//! - INSERT, UPDATE, scalar SELECT use runtime-bound `sqlx::query` /
//!   `query_scalar`.  This mirrors `document_files.rs` and avoids
//!   blocking on `cargo sqlx prepare` regeneration for the W7 PR.  The
//!   bd follow-up `PRRO_GATE-9qd.1.1` already tracks promotion to
//!   `query!` macros across the new repos.
//! - Errors bubble up unmodified; FK / CHECK / PK violations surface as
//!   `sqlx::Error`.

use crate::db::models::ids::DocumentId;
use crate::db::tx::WriteTxConn;

/// Closed set of W7-frozen + W10.4 extension outcome classifications.
/// Mirrors the CHECK list in migration 010 + 013.
///
/// **W10.4 addition:** `RetryableMacHashMismatch` for the Server-12
/// first-attempt trace row.  Migration 013 extends the
/// `transport_trace.outcome_kind` CHECK list to include the new wire
/// string via a SQLite table-rebuild (per migration 008 precedent).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutcomeKind {
    Ok,
    Rejected,
    RetryableTransport,
    RetryableServer,
    RetryableAuthFn,
    /// W10.4 — Server `-12` ERROR_BAD_HASH_PREV first attempt.
    /// `MacRecovery` `RetryClass` folds here; the orchestrator
    /// completes the recovery cycle out of band of stage 4-b.
    RetryableMacHashMismatch,
}

impl OutcomeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Rejected => "REJECTED",
            Self::RetryableTransport => "RETRYABLE_TRANSPORT",
            Self::RetryableServer => "RETRYABLE_SERVER",
            Self::RetryableAuthFn => "RETRYABLE_AUTH_FN",
            Self::RetryableMacHashMismatch => "RETRYABLE_MAC_HASH_MISMATCH",
        }
    }
}

/// Inputs needed at 4-pre time.  `request_envelope_sha256` is the
/// SHA-256 of the **full** `CheckEnvelope` wire form (all fields:
/// `rro_fn`, `date_time`, `check_sign`, `local_number`, `check_type`,
/// `id_offline`, `id_cancel`).  Hashing only `check_sign` would miss
/// drift in `date_time`/`local_number`/`check_type`/`id_offline`/
/// `id_cancel` — a bug-introduced mismatch between two retry attempts
/// would not be detectable.  The canonicalisation (concrete byte form
/// being hashed) is the caller's contract; W7.4 `stage_send` uses the
/// prost-encoded `gen::Check` proto bytes since that captures every
/// wire field exactly as DPS sees them.
#[derive(Clone, Debug)]
pub struct NewAttempt {
    pub backend_profile_id: String,
    pub transport_profile_id: String,
    pub request_envelope_sha256: [u8; 32],
}

/// Inputs needed at 4-b time.  `wire_call_started_at` and
/// `wire_call_finished_at` are ISO-8601 strings captured around the
/// `dps_channel.send_chk(envelope).await` call (no clock seam in
/// `WorkerContext` per W7 freeze decision #4 — caller renders these
/// strings from any reasonable source, default helper is local-private
/// to `stage_send.rs`).  `completed_at` is NOT a parameter — the
/// repo sets it to `CURRENT_TIMESTAMP` on UPDATE.
#[derive(Clone, Debug)]
pub struct AttemptCompletion {
    pub wire_call_started_at: String,
    pub wire_call_finished_at: String,
    pub outcome_kind: OutcomeKind,
    pub server_fiscal_no: Option<String>,
    pub server_status_code: Option<i32>,
    pub error_kind: Option<String>,
    pub error_message: Option<String>,
    /// W10.2 review fix-up + migration 012: durable encoding of the
    /// W10 routing `RetryClass` for the routed arm.  `None` on the
    /// `WireDecision::Sent` happy path (success has no retry class)
    /// AND on pre-migration-012 rows (NULL).  Callers (worker
    /// dispatcher in W11+) read this via
    /// [`last_attempt_retry_class_for`] to decide whether a doc
    /// currently in `ErrorRetryable` warrants another wire send.
    pub retry_class: Option<String>,
}

/// Allocate the next `attempt_no` for `doc_id` and INSERT the trace
/// row with intent-only fields.  Completion columns stay NULL until
/// [`complete_tx`] is called from 4-b.  Returns the allocated
/// `attempt_no` (1-based, monotonic per `doc_id`).
///
/// Must be called inside the same `with_immediate` envelope as the
/// CAS `Signed/Encrypted/ErrorRetryable -> Sending` and the
/// `submission_attempted_at` UPDATE — the BEGIN IMMEDIATE serialises
/// the `MAX(attempt_no)+1` read against any concurrent stage_send for
/// the same document, so two callers cannot allocate the same number.
pub async fn allocate_and_insert_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
    init: NewAttempt,
) -> sqlx::Result<i32> {
    let next: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(attempt_no), 0) + 1 \
         FROM transport_trace WHERE document_id = ?",
    )
    .bind(doc_id)
    .fetch_one(&mut **tx)
    .await?;
    let next_i32: i32 = next.try_into().map_err(|_| {
        sqlx::Error::Decode(
            format!("transport_trace.attempt_no overflow: {next} for document_id").into(),
        )
    })?;
    sqlx::query(
        "INSERT INTO transport_trace ( \
            document_id, attempt_no, \
            backend_profile_id, transport_profile_id, \
            request_envelope_sha256 \
         ) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(doc_id)
    .bind(next_i32)
    .bind(&init.backend_profile_id)
    .bind(&init.transport_profile_id)
    .bind(&init.request_envelope_sha256[..])
    .execute(&mut **tx)
    .await?;
    Ok(next_i32)
}

/// Mark an existing trace row complete.  The CHECK constraint in
/// migration 010 enforces "all-or-none" on the four completion
/// columns; this UPDATE writes them as a unit.
///
/// `completed_at` is set to `CURRENT_TIMESTAMP` (DB clock per W7 freeze
/// decision #4).  Caller passes `wire_call_*` ISO-8601 strings captured
/// around the `send_chk` invocation.
///
/// **Append-then-complete invariant.**  The `WHERE` clause includes
/// `completed_at IS NULL` so a second call against an already-completed
/// row is a no-op (`rows_affected == 0`) — never an in-place rewrite.
/// The forensic record of the first completion is preserved.  Callers
/// MUST treat `rows_affected == 0` as either:
///   - the `(doc_id, attempt_no)` pair is missing (allocator bug — the
///     row should have been INSERTed by `allocate_and_insert_tx` in
///     4-pre), or
///   - the row is already complete (caller bug — `complete_tx` was
///     invoked twice for the same attempt).
///
/// Both cases are non-recoverable mid-stage and should surface as a
/// stage_send error rather than silent success.  `rows_affected == 1`
/// is the only success shape.
pub async fn complete_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
    attempt_no: i32,
    completion: AttemptCompletion,
) -> sqlx::Result<u64> {
    let res = sqlx::query(
        "UPDATE transport_trace SET \
            completed_at = CURRENT_TIMESTAMP, \
            wire_call_started_at = ?, \
            wire_call_finished_at = ?, \
            outcome_kind = ?, \
            server_fiscal_no = ?, \
            server_status_code = ?, \
            error_kind = ?, \
            error_message = ?, \
            retry_class = ? \
         WHERE document_id = ? AND attempt_no = ? AND completed_at IS NULL",
    )
    .bind(&completion.wire_call_started_at)
    .bind(&completion.wire_call_finished_at)
    .bind(completion.outcome_kind.as_str())
    .bind(completion.server_fiscal_no.as_deref())
    .bind(completion.server_status_code)
    .bind(completion.error_kind.as_deref())
    .bind(completion.error_message.as_deref())
    .bind(completion.retry_class.as_deref())
    .bind(doc_id)
    .bind(attempt_no)
    .execute(&mut **tx)
    .await?;
    Ok(res.rows_affected())
}

/// Forensic read used by tests + (eventually) W9 reconciliation.
/// Returns rows for `doc_id` ordered by `attempt_no` ascending.
#[derive(Clone, Debug)]
pub struct TraceRow {
    pub attempt_no: i32,
    pub started_at: String,
    pub backend_profile_id: String,
    pub transport_profile_id: String,
    pub request_envelope_sha256: [u8; 32],
    pub completed_at: Option<String>,
    pub wire_call_started_at: Option<String>,
    pub wire_call_finished_at: Option<String>,
    pub outcome_kind: Option<String>,
    pub server_fiscal_no: Option<String>,
    pub server_status_code: Option<i32>,
    pub error_kind: Option<String>,
    pub error_message: Option<String>,
    /// W10.2 review fix-up + migration 012.  `None` on success path
    /// AND on pre-012 rows.  Decode via [`crate::services::write_path::error_routing::RetryClass::from_wire_str`].
    pub retry_class: Option<String>,
}

/// Wire shape of a `transport_trace` row when read via `query_as`
/// runtime-bound — 13 columns in the order the SELECT below emits
/// them.  Type alias keeps the call site narrow (clippy
/// `clippy::type_complexity` was tripping on the inline tuple) and
/// makes the SELECT-vs-decode boundary explicit.  Promotion to
/// `query_as!` with `#[derive(FromRow)]` is tracked under bd
/// `PRRO_GATE-9qd.1.1`.
type TraceRowTuple = (
    i32,            // attempt_no
    String,         // started_at
    String,         // backend_profile_id
    String,         // transport_profile_id
    Vec<u8>,        // request_envelope_sha256 (decoded to [u8; 32] below)
    Option<String>, // completed_at
    Option<String>, // wire_call_started_at
    Option<String>, // wire_call_finished_at
    Option<String>, // outcome_kind
    Option<String>, // server_fiscal_no
    Option<i32>,    // server_status_code
    Option<String>, // error_kind
    Option<String>, // error_message
    Option<String>, // retry_class (W10.2 review fix-up + migration 012)
);

pub async fn list_for_document(
    pool: &sqlx::SqlitePool,
    doc_id: DocumentId,
) -> sqlx::Result<Vec<TraceRow>> {
    let rows: Vec<TraceRowTuple> = sqlx::query_as(
        "SELECT attempt_no, started_at, backend_profile_id, transport_profile_id, \
                request_envelope_sha256, completed_at, wire_call_started_at, \
                wire_call_finished_at, outcome_kind, server_fiscal_no, \
                server_status_code, error_kind, error_message, retry_class \
         FROM transport_trace WHERE document_id = ? ORDER BY attempt_no",
    )
    .bind(doc_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|r| {
            let sha: [u8; 32] = r.4.as_slice().try_into().map_err(|_| {
                sqlx::Error::Decode(
                    format!(
                        "transport_trace.request_envelope_sha256: expected 32 bytes, got {}",
                        r.4.len()
                    )
                    .into(),
                )
            })?;
            Ok(TraceRow {
                attempt_no: r.0,
                started_at: r.1,
                backend_profile_id: r.2,
                transport_profile_id: r.3,
                request_envelope_sha256: sha,
                completed_at: r.5,
                wire_call_started_at: r.6,
                wire_call_finished_at: r.7,
                outcome_kind: r.8,
                server_fiscal_no: r.9,
                server_status_code: r.10,
                error_kind: r.11,
                error_message: r.12,
                retry_class: r.13,
            })
        })
        .collect()
}

/// W10.2 review fix-up: read the durable `retry_class` of the most
/// recent (highest `attempt_no`) **completed** trace row for `doc_id`.
/// Used by the worker dispatcher (W11+) and ops scripts to decide
/// whether a doc currently in `ErrorRetryable` warrants another wire
/// send per the table in `stage_send.rs` module docstring + freeze §4.2.
///
/// Returns:
///   - `Ok(None)`  — no completed attempt yet (4-pre allocated but
///     4-b never committed) OR the doc has no trace rows at all OR the
///     row is pre-migration-012 (NULL `retry_class`).  Caller should
///     treat as "indeterminate; do NOT auto-retry".
///   - `Ok(Some(rc))` — last completed attempt's retry class.  Caller
///     filters per the docstring contract: only `TransientRetry` is
///     auto-retryable; the other six classes need higher-layer
///     orchestration.
///
/// Reads outside any tx — pool-bound; safe to call from the dispatcher
/// scheduling loop without entering a write lock.  WAL semantics: the
/// row is visible after the 4-b `with_immediate` envelope commits.
pub async fn last_attempt_retry_class_for(
    pool: &sqlx::SqlitePool,
    doc_id: DocumentId,
) -> sqlx::Result<Option<crate::services::write_path::error_routing::RetryClass>> {
    let opt: Option<Option<String>> = sqlx::query_scalar(
        "SELECT retry_class FROM transport_trace \
         WHERE document_id = ? AND completed_at IS NOT NULL \
         ORDER BY attempt_no DESC LIMIT 1",
    )
    .bind(doc_id)
    .fetch_optional(pool)
    .await?;
    Ok(opt
        .flatten()
        .as_deref()
        .and_then(crate::services::write_path::error_routing::RetryClass::from_wire_str))
}

/// W9.2 (per freeze §4.0, HIGH 2 fix) — durable per-doc wire-attempt
/// counter for boot-recovery budget decisions.
///
/// Returns `COALESCE(MAX(attempt_no), 0)` across all `transport_trace`
/// rows for `doc_id` (no `completed_at` filter — both in-flight and
/// completed attempts count toward the budget; a crash mid-send still
/// "burned" a slot per Pattern B safety contract).  Always non-NULL
/// `i64` because `COALESCE` guarantees a fallback `0`.
///
/// Used by `services::reconciliation::boot_phase::run_boot_reconciliation`
/// (W9.3) for the §4.8 ERROR_RETRYABLE pre-check:
///   `attempts_used(doc_id) >= MAX_BOOT_ATTEMPTS  →  escalate to
///    RequiresManualReconciliation`.
///
/// Pool-bound (read-only); safe to call from boot or operational loops
/// without entering a write lock.
pub async fn attempts_used(pool: &sqlx::SqlitePool, doc_id: DocumentId) -> sqlx::Result<i64> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(attempt_no), 0) FROM transport_trace WHERE document_id = ?",
    )
    .bind(doc_id)
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// W9.2 (per freeze §4.5, HIGH 8 + HIGH 9 fix — amended for schema
/// reality) — complete an in-flight `transport_trace` row via boot
/// recovery.
///
/// **Schema constraint awareness.**  Migration 010 enforces all-or-
/// none on `(completed_at, wire_call_started_at, wire_call_finished_at,
/// outcome_kind)`: in-flight rows have all four NULL, completed rows
/// have all four NOT NULL.  No legitimate intermediate "in-flight
/// with wire times set" state exists.
///
/// Earlier freeze v7 claim "preserves original wire times" was based
/// on a misread: the row that's in-flight at boot has NEVER recorded
/// wire times (the original `send_chk` either never returned or
/// crashed before reaching `complete_tx`).  Therefore recovery MUST
/// supply its own wire times — the `last_chk_probe` call's times are
/// the truthful record of when this code path interacted with DPS.
///
/// **What this helper writes** (recovery completion):
///   - `completed_at = CURRENT_TIMESTAMP`
///   - `outcome_kind = 'OK'` (doc IS protocol-acknowledged per match)
///   - `server_fiscal_no` (from `CheckAck.id`)
///   - `wire_call_started_at` (caller-supplied, from probe)
///   - `wire_call_finished_at` (caller-supplied, from probe)
///   - `retry_class = NULL` (recovery flipped a retryable in-flight
///     to OK; classification N/A)
///
/// **What this helper does NOT write** — `server_status_code` /
/// `error_kind` / `error_message` stay at their DB default (NULL on
/// fresh row).  The audit row (`BOOT_LAST_CHK_MATCH_KVT1`) records
/// the recovery context separately.
///
/// **Why a recovery-specific helper vs reusing `complete_tx`.**
/// `complete_tx` requires the full `AttemptCompletion` struct
/// including outcome-classification fields (`error_kind`,
/// `error_message`, `server_status_code`).  Recovery callers don't
/// have those — they have only the probe's identity match.  Forcing
/// callers to fabricate `AttemptCompletion::default()` would invite
/// drift.  Distinct helper keeps the recovery-completion contract
/// narrow and obvious.
///
/// **Idempotency.**  `WHERE completed_at IS NULL` makes a second call
/// against an already-completed row a no-op (`rows_affected = 0`).
/// Caller treats `0` as either:
///   - the `(doc_id, attempt_no)` pair is missing (caller bug — the
///     orchestrator should have verified the in-flight row exists),
///     or
///   - the row was already completed by a parallel boot tick (race
///     under non-single-writer scenarios — should not occur under
///     M3a's single-writer-per-FN invariant).
///
/// `rows_affected = 1` is the only success shape.
pub async fn complete_via_recovery_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
    attempt_no: i32,
    server_fiscal_no: &str,
    wire_call_started_at: &str,
    wire_call_finished_at: &str,
) -> sqlx::Result<u64> {
    let res = sqlx::query(
        "UPDATE transport_trace SET \
            completed_at = CURRENT_TIMESTAMP, \
            outcome_kind = 'OK', \
            server_fiscal_no = ?, \
            wire_call_started_at = ?, \
            wire_call_finished_at = ?, \
            retry_class = NULL \
         WHERE document_id = ? AND attempt_no = ? AND completed_at IS NULL",
    )
    .bind(server_fiscal_no)
    .bind(wire_call_started_at)
    .bind(wire_call_finished_at)
    .bind(doc_id)
    .bind(attempt_no)
    .execute(&mut **tx)
    .await?;
    Ok(res.rows_affected())
}
