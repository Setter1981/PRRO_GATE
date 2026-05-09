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
//!   "reload + retry" (Conflict) and "escalate" (NotFound).  Per ADR-M3-A4
//!   and W0-2 §4.4 (M3a W2), `transition_state` takes
//!   `&mut WriteTxConn<'_>` rather than a pool: the disambiguation
//!   `SELECT` and the CAS `UPDATE` run on the same connection inside the
//!   same `with_immediate` BEGIN IMMEDIATE envelope, so the
//!   Conflict-vs-NotFound result is atomic by construction —
//!   PRRO_GATE-k99 closed structurally.

use crate::db::models::{
    enums::{DocState, DocType},
    ids::{DocumentId, OfflineSessionId, RequestId, ShiftId},
};
use crate::db::tx::WriteTxConn;
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
    /// W5 — read along the doc to make resume path use PERSISTED
    /// profile bindings (not the current `node_state.*_profile_id`,
    /// which can drift between original PREPARED and a later resume
    /// pickup).  Schema-NOT-NULL on `fiscal_documents`, so always
    /// present for a successfully-inserted doc.
    pub backend_profile_id: String,
    pub transport_profile_id: String,
    /// W6 — previous-doc unsigned-XML SHA256 (raw 32 bytes, NOT hex).
    /// Pinned in stage 3-PRE atomically with `signing_inputs_pinned_at`.
    /// Hex-encoded only at the XML builder boundary (`<MAC>` attr).
    /// `None` for a doc whose chain seed was unset at pin time
    /// (genuine first-after-bootstrap), distinguishable from "not
    /// pinned yet" via [`signing_inputs_pinned_at`].
    pub previous_hash: Option<[u8; 32]>,
    /// W6 — Z-report counter persisted on the doc; `Some(N)` only for
    /// `wire_artifact_kind == ZReport` (DocType::ShiftClose or
    /// DocType::ZReport after boundary mapping).  Pinned in stage
    /// 3-PRE; retry observes the persisted value and reuses it.
    pub z_report_number: Option<i64>,
    /// W6 — sha256 of the canonical unsigned XML.  Updated NULL→hash
    /// in stage 3-PERSIST (was set NULL by W5 INSERT PREPARED).
    pub unsigned_xml_sha256: Option<[u8; 32]>,
    /// W6 — pin-once flag (ISO8601 timestamp).  `None` = stage 3-PRE
    /// has not pinned signing inputs yet.  `Some(_)` = pinned; the
    /// `previous_hash` and `z_report_number` columns are now
    /// authoritative for retry.
    pub signing_inputs_pinned_at: Option<String>,
}

/// W6 — decode a length-32 BLOB into a fixed `[u8; 32]`.  Fail-closed:
/// any non-NULL value whose length is not 32 surfaces as a
/// `sqlx::Error::Decode` rather than a silent truncation.  Schema
/// CHECK clauses (`previous_hash`, `unsigned_xml_sha256`) make this
/// path unreachable in production, but a corrupted or hand-edited DB
/// is then operator-visible at row load time.
fn decode_blob32(
    raw: Option<Vec<u8>>,
    column: &'static str,
) -> Result<Option<[u8; 32]>, sqlx::Error> {
    match raw {
        None => Ok(None),
        Some(v) => {
            let arr: [u8; 32] = v.as_slice().try_into().map_err(|_| {
                sqlx::Error::Decode(
                    format!(
                        "fiscal_documents.{column}: expected 32 bytes, got {}",
                        v.len()
                    )
                    .into(),
                )
            })?;
            Ok(Some(arr))
        }
    }
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
///
/// Per ADR-M3-A4 / W0-2 §4.4 (M3a W2), takes `&mut WriteTxConn<'_>` —
/// callers obtain it from a `with_immediate` closure, which guarantees
/// the CAS UPDATE and the disambiguation SELECT run on the same
/// connection inside the same BEGIN IMMEDIATE envelope.
pub async fn transition_state(
    tx: &mut WriteTxConn<'_>,
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
            .execute(&mut **tx)
            .await?;
    if res.rows_affected() == 1 {
        return Ok(TransitionOutcome::Applied);
    }
    // CAS missed — disambiguate row-missing vs state-diverged.  Same
    // connection inside the same BEGIN IMMEDIATE tx (via WriteTxConn
    // Deref), so this SELECT cannot interleave with another writer's
    // INSERT/DELETE.
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM fiscal_documents WHERE document_id = ? LIMIT 1")
            .bind(id)
            .fetch_optional(&mut **tx)
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
                  submission_attempted_at,
                  backend_profile_id,
                  transport_profile_id,
                  previous_hash         as "previous_hash: Vec<u8>",
                  z_report_number,
                  unsigned_xml_sha256   as "unsigned_xml_sha256: Vec<u8>",
                  signing_inputs_pinned_at
           FROM fiscal_documents
           WHERE fiscal_number = ?
             AND state IN ('PREPARED','SIGNED','ENCRYPTED','SENDING','SENT','KVT1','KVT2','ERROR_RETRYABLE')
           ORDER BY lnd, created_at, document_id"#,
        fn_id
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|r| {
            Ok(DocumentRow {
                document_id: r.document_id,
                fiscal_number: r.fiscal_number,
                lnd: r.lnd,
                state: r.state,
                doc_type: r.doc_type,
                server_fiscal_no: r.server_fiscal_no,
                submission_attempted_at: r.submission_attempted_at,
                backend_profile_id: r.backend_profile_id,
                transport_profile_id: r.transport_profile_id,
                previous_hash: decode_blob32(r.previous_hash, "previous_hash")?,
                z_report_number: r.z_report_number,
                unsigned_xml_sha256: decode_blob32(r.unsigned_xml_sha256, "unsigned_xml_sha256")?,
                signing_inputs_pinned_at: r.signing_inputs_pinned_at,
            })
        })
        .collect()
}

/// W5 / W0-1 §3.1 stage 1 — resume-detect lookup by `request_id`
/// inside the worker's `with_immediate` envelope.
///
/// **Filtered to pending (resumable) states only**: PREPARED, SIGNED,
/// ENCRYPTED, SENDING, SENT, KVT1, KVT2, ERROR_RETRYABLE.  Mirrors
/// the pending list of `list_pending_for_fn`.  Terminal-success
/// (ACK), terminal-fail (REJECTED, CANCELLED, REQUIRES_MANUAL_
/// RECONCILIATION), and offline-handoff (OFFLINE_LOCAL_ACK) rows
/// MUST NOT be returned — they would otherwise drive
/// `WorkerProcessResult::Resumed` for a document whose flow is
/// already concluded.  Terminal coexistence with a `NEW` inbox row
/// for the same `request_id` is detected separately by
/// [`exists_terminal_by_request_id_tx`] and surfaces as a guard
/// rejection; this method's job is strictly the resume path.
pub async fn get_pending_by_request_id_tx(
    tx: &mut WriteTxConn<'_>,
    request_id: &RequestId,
) -> sqlx::Result<Option<DocumentRow>> {
    let row = sqlx::query!(
        r#"SELECT document_id    as "document_id: DocumentId",
                  fiscal_number,
                  lnd,
                  state           as "state: DocState",
                  doc_type        as "doc_type: DocType",
                  server_fiscal_no,
                  submission_attempted_at,
                  backend_profile_id,
                  transport_profile_id,
                  previous_hash         as "previous_hash: Vec<u8>",
                  z_report_number,
                  unsigned_xml_sha256   as "unsigned_xml_sha256: Vec<u8>",
                  signing_inputs_pinned_at
           FROM fiscal_documents
           WHERE request_id = ?
             AND state IN ('PREPARED','SIGNED','ENCRYPTED','SENDING','SENT','KVT1','KVT2','ERROR_RETRYABLE')"#,
        request_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    let Some(r) = row else { return Ok(None) };
    Ok(Some(DocumentRow {
        document_id: r.document_id,
        fiscal_number: r.fiscal_number,
        lnd: r.lnd,
        state: r.state,
        doc_type: r.doc_type,
        server_fiscal_no: r.server_fiscal_no,
        submission_attempted_at: r.submission_attempted_at,
        backend_profile_id: r.backend_profile_id,
        transport_profile_id: r.transport_profile_id,
        previous_hash: decode_blob32(r.previous_hash, "previous_hash")?,
        z_report_number: r.z_report_number,
        unsigned_xml_sha256: decode_blob32(r.unsigned_xml_sha256, "unsigned_xml_sha256")?,
        signing_inputs_pinned_at: r.signing_inputs_pinned_at,
    }))
}

/// W5 / W0-1 §3.1 stage 1 — companion to
/// [`get_pending_by_request_id_tx`].  Returns `true` when a
/// fiscal_documents row with the given `request_id` exists in a
/// terminal state: ACK, REJECTED, CANCELLED, OFFLINE_LOCAL_ACK,
/// REQUIRES_MANUAL_RECONCILIATION.
///
/// Stage 1 calls this AFTER a clean pending miss to detect the
/// "terminal-doc + NEW-inbox-for-same-request_id" invariant breach
/// — coexistence here means a previous run reached terminal but a
/// fresh ingress row was admitted under the same `request_id`,
/// which the worker MUST refuse rather than INSERT a duplicate
/// PREPARED.
pub async fn exists_terminal_by_request_id_tx(
    tx: &mut WriteTxConn<'_>,
    request_id: &RequestId,
) -> sqlx::Result<bool> {
    let row: Option<i64> = sqlx::query_scalar(
        r#"SELECT 1 FROM fiscal_documents
            WHERE request_id = ?
              AND state IN ('ACK','REJECTED','CANCELLED','OFFLINE_LOCAL_ACK','REQUIRES_MANUAL_RECONCILIATION')
            LIMIT 1"#,
    )
    .bind(request_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.is_some())
}

/// W5 / W0-1 §3.1 stage 1 — same INSERT as [`insert_prepared`] but
/// driven from inside a `with_immediate` envelope through
/// `&mut WriteTxConn<'_>`.  The pool-version is preserved for
/// pre-W5 call sites (admin / migration tooling); W5 stage 1 uses
/// this one so the lease CAS, lnd allocate, INSERT PREPARED, and
/// audit append all run on the same connection inside the same
/// BEGIN IMMEDIATE transaction.
pub async fn insert_prepared_tx(tx: &mut WriteTxConn<'_>, n: &NewDocument) -> sqlx::Result<()> {
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
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// W6 stage 3-PRE — pin status + state snapshot of a doc, atomic with
/// the rest of the 3-PRE write tx.  Returns `None` if the row is
/// missing.  `state` is included so the caller can early-fail with
/// `SignError::StateConflict` BEFORE pin or Z allocation, preventing
/// stale-WorkerContext from advancing `next_z_report_number` for a doc
/// whose flow already concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedSigningInputs {
    pub state: DocState,
    /// `signing_inputs_pinned_at IS NOT NULL` — disambiguates "not
    /// pinned yet" from "pinned with empty/None previous_hash"
    /// (genuine first-doc-after-bootstrap edge case).
    pub is_pinned: bool,
    pub previous_hash: Option<[u8; 32]>,
    pub z_report_number: Option<i64>,
}

/// W6 stage 3-PRE — read state + pin status atomically inside the
/// `with_immediate` envelope.  See [`PinnedSigningInputs`] for shape.
pub async fn get_signing_inputs_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
) -> sqlx::Result<Option<PinnedSigningInputs>> {
    let row = sqlx::query!(
        r#"SELECT state                    as "state: DocState",
                  previous_hash            as "previous_hash: Vec<u8>",
                  z_report_number,
                  signing_inputs_pinned_at
           FROM fiscal_documents WHERE document_id = ?"#,
        doc_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    let Some(r) = row else { return Ok(None) };
    Ok(Some(PinnedSigningInputs {
        state: r.state,
        is_pinned: r.signing_inputs_pinned_at.is_some(),
        previous_hash: decode_blob32(r.previous_hash, "previous_hash")?,
        z_report_number: r.z_report_number,
    }))
}

/// W6 stage 3-PRE — pin signing inputs onto the doc atomically.  One
/// UPDATE writes `previous_hash`, `z_report_number`, AND
/// `signing_inputs_pinned_at = CURRENT_TIMESTAMP`.
///
/// **Pin-once + state-gate guard**: WHERE-guarded on
/// `state = 'PREPARED' AND signing_inputs_pinned_at IS NULL`.  Returns
/// `rows_affected` so the caller can distinguish:
/// - `1` — pin happened (this caller is authoritative)
/// - `0` — either state moved (concurrent finalize/reject) OR row
///   was already pinned by an earlier 3-PRE re-entry; caller already
///   has [`get_signing_inputs_tx`] read in the same tx and acts on
///   that truth (re-fetch + StateConflict OR reuse-branch).
///
/// Idempotent under concurrent re-entry: a second pin attempt finds
/// `signing_inputs_pinned_at IS NOT NULL`, UPDATE matches 0 rows,
/// caller falls through to reuse.
pub async fn pin_signing_inputs_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
    previous_hash: Option<&[u8; 32]>,
    z_report_number: Option<i64>,
) -> sqlx::Result<u64> {
    let res = sqlx::query(
        "UPDATE fiscal_documents \
         SET previous_hash            = ?, \
             z_report_number          = ?, \
             signing_inputs_pinned_at = CURRENT_TIMESTAMP \
         WHERE document_id = ? \
           AND state = 'PREPARED' \
           AND signing_inputs_pinned_at IS NULL",
    )
    .bind(previous_hash.map(|h| &h[..]))
    .bind(z_report_number)
    .bind(doc_id)
    .execute(&mut **tx)
    .await?;
    Ok(res.rows_affected())
}

/// W6 stage 3-PERSIST — UPDATE `unsigned_xml_sha256` from NULL to the
/// canonical sha256 of the unsigned XML.  W5 INSERT PREPARED leaves
/// it NULL; this is the canonical write site.  Returns `true` if the
/// row exists and was updated.
pub async fn update_unsigned_xml_sha256_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
    hash: &[u8; 32],
) -> sqlx::Result<bool> {
    let res =
        sqlx::query("UPDATE fiscal_documents SET unsigned_xml_sha256 = ? WHERE document_id = ?")
            .bind(&hash[..])
            .bind(doc_id)
            .execute(&mut **tx)
            .await?;
    Ok(res.rows_affected() == 1)
}
