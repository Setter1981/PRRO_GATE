//! Repository for `fiscal_documents`.
//!
//! Repo policy:
//! - SELECT decode through `sqlx::query!` (compile-time schema verification).
//! - INSERT and UPDATE runtime-bound (param types enforced by `Encode<Sqlite>`).
//! - State transitions go through CAS UPDATE + a code-level `allowed_transition`
//!   whitelist, returning a `TransitionOutcome` enum that distinguishes:
//!     * Applied  — CAS hit, row now in `to`;
//!     * Forbidden — whitelist rejected the (from, to) pair (no DB call);
//!     * Conflict — row exists but its state != `from` (caller should reload);
//!     * NotFound — no row with `document_id`.
//!
//!   This split is what write_path retry logic (M3) needs to decide between
//!   "reload + retry" (Conflict) and "escalate" (NotFound).
//!
//! Known limitation (deferred to M3 — see bd-issue tracked for write_path):
//! `transition_state` disambiguates Conflict vs NotFound with a *separate*
//! `SELECT 1` after a CAS-miss.  A delete or insert in the gap between the
//! UPDATE and the SELECT could swap the two outcomes.  In practice this is
//! benign — `fiscal_documents` is legally append-only-like (no production
//! delete path) and M3 write_path is expected to call `transition_state`
//! inside its own `db::tx::with_immediate` envelope as part of a compound
//! op (transition + audit_log.append + node_state.update), naturally
//! making the disambiguation atomic.  If a code path needs strict atomic
//! disambiguation in M1, wrap the call site in `with_immediate` directly.

use crate::db::models::{
    enums::{DocState, DocType},
    ids::{DocumentId, OfflineSessionId, RequestId, ShiftId},
};
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct NewDocument {
    pub document_id: DocumentId,
    pub request_id: RequestId,
    pub fiscal_number: String,
    pub shift_id: Option<ShiftId>,
    pub offline_session_id: Option<OfflineSessionId>,
    pub lnd: i64,
    pub doc_type: DocType,
    pub backend_profile_id: String,
    pub transport_profile_id: String,
    pub fs_mode: &'static str, // "ONLINE" | "OFFLINE"
    pub business_ts: String,   // ISO-8601
    pub total_sum_kop: Option<i64>,
    pub payload_json: String,
    pub payload_sha256_canonical: [u8; 32],
    pub unsigned_xml_sha256: Option<[u8; 32]>,
    pub previous_hash: Option<[u8; 32]>,
}

#[derive(Debug, Clone)]
pub struct DocumentRow {
    pub document_id: DocumentId,
    pub fiscal_number: String,
    pub lnd: i64,
    pub state: DocState,
    pub doc_type: DocType,
    pub server_fiscal_no: Option<String>,
    pub submission_attempted_at: Option<String>,
}

/// Outcome of a state transition attempt.  Replaces a bare `bool` so write_path
/// retry logic can tell "row diverged, reload" apart from "row vanished, escalate".
///
/// Precedence: `Forbidden` is decided in code BEFORE the DB is consulted, so a
/// forbidden transition for a non-existent `document_id` returns `Forbidden`,
/// not `NotFound`.  This is by design — `Forbidden` indicates a caller bug
/// (asked for a transition not in the whitelist) and should be surfaced
/// regardless of row existence.  `NotFound` is reserved for the case where
/// the requested transition WOULD have been allowed but the row is missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionOutcome {
    Applied,
    Forbidden,
    Conflict,
    NotFound,
}

pub fn allowed_transition(from: DocState, to: DocState) -> bool {
    use DocState::*;
    matches!(
        (from, to),
        (Prepared, Signed)
            | (Prepared, Rejected)
            | (Signed, Encrypted)
            | (Signed, ErrorRetryable)
            | (Signed, OfflineLocalAck)
            | (Encrypted, Sent)
            | (Encrypted, ErrorRetryable)
            | (Sent, Kvt1)
            | (Sent, ErrorRetryable)
            | (Sent, Rejected)
            | (Kvt1, Kvt2)
            | (Kvt1, ErrorRetryable)
            | (Kvt2, Ack)
            | (OfflineLocalAck, Sent)
            | (ErrorRetryable, Sent)
            | (ErrorRetryable, Kvt1)
            | (ErrorRetryable, RequiresManualReconciliation)
            // Pattern B (ADR-M3-A5 / A9 step 3): Sending is the
            // intent-marker for stage 4 send.  The 7 additions below
            // wire it in.  M3a DPS code MUST NOT use the legacy
            // (ErrorRetryable, Sent) entry for wire send; retries go
            // through (ErrorRetryable, Sending) and then on through
            // a fresh wire call.
            | (Signed, Sending)
            | (Encrypted, Sending)
            | (Sending, Sent)
            | (Sending, Kvt1)
            | (Sending, ErrorRetryable)
            | (Sending, Rejected)
            | (ErrorRetryable, Sending)
    )
}

pub async fn insert_prepared(pool: &SqlitePool, n: &NewDocument) -> sqlx::Result<()> {
    sqlx::query(
        r#"INSERT INTO fiscal_documents (
             document_id, request_id, fiscal_number, shift_id, offline_session_id,
             lnd, doc_type, state, backend_profile_id, transport_profile_id,
             fs_mode, business_ts, total_sum_kop, payload_json,
             payload_sha256_canonical, unsigned_xml_sha256, previous_hash
           ) VALUES (?, ?, ?, ?, ?, ?, ?, 'PREPARED', ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(n.document_id)
    .bind(n.request_id)
    .bind(&n.fiscal_number)
    .bind(n.shift_id)
    .bind(n.offline_session_id)
    .bind(n.lnd)
    .bind(n.doc_type)
    .bind(&n.backend_profile_id)
    .bind(&n.transport_profile_id)
    .bind(n.fs_mode)
    .bind(&n.business_ts)
    .bind(n.total_sum_kop)
    .bind(&n.payload_json)
    .bind(&n.payload_sha256_canonical[..])
    .bind(n.unsigned_xml_sha256.as_ref().map(|b| &b[..]))
    .bind(n.previous_hash.as_ref().map(|b| &b[..]))
    .execute(pool)
    .await?;
    Ok(())
}

/// Atomic state transition.  See [`TransitionOutcome`] for the four possible
/// results.  The whitelist short-circuits forbidden moves before any DB call;
/// a successful CAS returns Applied; a missed CAS triggers a follow-up
/// existence check to disambiguate Conflict from NotFound.
pub async fn transition_state(
    pool: &SqlitePool,
    id: DocumentId,
    from: DocState,
    to: DocState,
) -> sqlx::Result<TransitionOutcome> {
    if !allowed_transition(from, to) {
        return Ok(TransitionOutcome::Forbidden);
    }
    let res =
        sqlx::query("UPDATE fiscal_documents SET state = ? WHERE document_id = ? AND state = ?")
            .bind(to)
            .bind(id)
            .bind(from)
            .execute(pool)
            .await?;
    if res.rows_affected() == 1 {
        return Ok(TransitionOutcome::Applied);
    }
    // CAS missed — disambiguate row-missing vs state-diverged.  One extra
    // round-trip on the unhappy path; the happy path stays single-statement.
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM fiscal_documents WHERE document_id = ? LIMIT 1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(if exists.is_some() {
        TransitionOutcome::Conflict
    } else {
        TransitionOutcome::NotFound
    })
}

/// Returns documents in non-final, non-handed-off states for the given FN,
/// in deterministic order suitable for fiscal-chain recovery.
///
/// Pending set (8 states, per ADR-M3-A8):
/// - PREPARED, SIGNED, ENCRYPTED, SENDING, SENT, KVT1, KVT2, ERROR_RETRYABLE
///
/// SENDING is pending (ADR-M3-A9 step 2): a crash between the CAS
/// Signed/Encrypted -> Sending and the wire send leaves the document in
/// SENDING with unknown wire-state; the App::boot recovery rule transitions
/// it to ERROR_RETRYABLE without invoking send_chk, because DPS does not
/// deduplicate and a re-send could fiscalise the same canonical receipt
/// twice.
///
/// KVT2 IS pending: a crash between persisting KVT2 and transitioning to ACK
/// would otherwise strand the document.  ACK is the only true terminal-success
/// state.
///
/// Excluded:
/// - ACK / REJECTED / CANCELLED — terminal.
/// - OFFLINE_LOCAL_ACK — handed off to offline_sync_service (separate worker).
/// - REQUIRES_MANUAL_RECONCILIATION — operator-driven flow.
///
/// Ordering: `(lnd, created_at, document_id)`.  `created_at` alone is
/// second-granular in SQLite (`CURRENT_TIMESTAMP`), so multiple docs created
/// within one second would otherwise have unstable order.  `lnd` is the
/// Local Numerator of Document — strictly monotonic per FN — so it is the
/// authoritative chain-recovery key; the other two are tiebreakers.
pub async fn list_pending_for_fn(pool: &SqlitePool, fn_id: &str) -> sqlx::Result<Vec<DocumentRow>> {
    let rows = sqlx::query!(
        r#"SELECT document_id    as "document_id: DocumentId",
                  fiscal_number,
                  lnd,
                  state           as "state: DocState",
                  doc_type        as "doc_type: DocType",
                  server_fiscal_no,
                  submission_attempted_at
           FROM fiscal_documents
           WHERE fiscal_number = ?
             AND state IN ('PREPARED','SIGNED','ENCRYPTED','SENDING','SENT','KVT1','KVT2','ERROR_RETRYABLE')
           ORDER BY lnd, created_at, document_id"#,
        fn_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| DocumentRow {
            document_id: r.document_id,
            fiscal_number: r.fiscal_number,
            lnd: r.lnd,
            state: r.state,
            doc_type: r.doc_type,
            server_fiscal_no: r.server_fiscal_no,
            submission_attempted_at: r.submission_attempted_at,
        })
        .collect())
}
