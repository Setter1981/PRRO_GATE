//! Repository for `outbox`.
//!
//! Schema: `rust/prro/migrations/011_outbox.sql` — cross-process
//! publishing seam for documents that have reached terminal Ack.
//! Per the W8 design freeze §3.1, the only INSERT site is
//! `stage_finalize::run`, inside the same `with_immediate` envelope
//! as the CAS `Kvt2 → Ack`.  Cross-process publishing (PENDING →
//! PUBLISHED transition) is post-M3a and not represented in this
//! repo.
//!
//! Repo policy:
//! - INSERT runtime-bound (`sqlx::query`) — content is BLOB/INT/TEXT
//!   without the schema-decode complexity that motivates `query!`
//!   for SELECT paths.
//! - Errors bubble up unmodified; PK/FK/CHECK violations surface as
//!   `sqlx::Error` for the caller to route.
//! - Tx-only: no pool-bound enqueue helper exists in M3a — the W8
//!   freeze constrains the INSERT to live inside the finalize
//!   `with_immediate` envelope.  A pool-bound helper would invite
//!   the bug class "outbox enqueued but state not advanced to Ack",
//!   which is exactly what migration 011 + finalize atomicity rule
//!   out by construction.

use crate::db::models::ids::DocumentId;
use crate::db::tx::WriteTxConn;

/// Inputs for `enqueue_document_tx`.  Caller (`stage_finalize::run`)
/// supplies the FN, the lnd as `sequence_no` (monotonic per FN),
/// and the canonical payload sha256 from `fiscal_documents.
/// payload_sha256_canonical`.  `document_id`, `enqueued_at` (DB
/// clock), `status` ('PENDING'), and `published_at` (NULL) are
/// either passed separately or default-set by the schema.
#[derive(Clone, Debug)]
pub struct NewOutboxRow {
    pub fiscal_number: String,
    pub sequence_no: i64,
    pub payload_sha256: [u8; 32],
}

/// Enqueue a document for cross-process publishing.  INSERT only —
/// the row starts as `status='PENDING'` with `published_at IS NULL`
/// per the schema defaults; `enqueued_at` is set to
/// `CURRENT_TIMESTAMP` by the DDL DEFAULT.
///
/// **Atomicity contract.**  Caller MUST invoke this inside the same
/// `with_immediate` envelope as the CAS `Kvt2 → Ack` on
/// `fiscal_documents`.  Any rollback of the enclosing tx (including
/// a failed audit insert downstream) rolls back the outbox row —
/// "Ack without outbox" is structurally impossible.
///
/// **Single-row contract.**  The `outbox` PRIMARY KEY is
/// `document_id`.  A duplicate INSERT (caller bug — `stage_finalize`
/// invoked twice for the same doc) returns a sqlx UNIQUE-violation
/// error.  In practice the CAS in `stage_finalize` short-circuits
/// rerun-on-Ack BEFORE this INSERT runs; the PK is defence in depth.
pub async fn enqueue_document_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
    row: NewOutboxRow,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO outbox (document_id, fiscal_number, sequence_no, payload_sha256) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(doc_id)
    .bind(&row.fiscal_number)
    .bind(row.sequence_no)
    .bind(&row.payload_sha256[..])
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Read-side helper used by tests + (eventually) the post-M3a
/// publisher worker.  Returns rows for `doc_id` (zero or one given
/// the `document_id` PK).
#[derive(Clone, Debug)]
pub struct OutboxRow {
    pub document_id: DocumentId,
    pub fiscal_number: String,
    pub sequence_no: i64,
    pub payload_sha256: [u8; 32],
    pub enqueued_at: String,
    pub status: String,
    pub published_at: Option<String>,
}

/// Wire shape of an `outbox` row when read via `query_as` runtime-bound
/// — 7 columns in the order the SELECT below emits them.  Type alias
/// keeps the call site narrow (clippy `clippy::type_complexity` was
/// tripping on the inline tuple).  Mirrors the W7.1
/// `transport_trace::TraceRowTuple` pattern.  Promotion to
/// `query_as!` + `#[derive(FromRow)]` is tracked under bd
/// `PRRO_GATE-9qd.1.1`.
type OutboxRowTuple = (
    Vec<u8>,        // document_id (decoded to [u8; 16] below)
    String,         // fiscal_number
    i64,            // sequence_no
    Vec<u8>,        // payload_sha256 (decoded to [u8; 32] below)
    String,         // enqueued_at
    String,         // status
    Option<String>, // published_at
);

pub async fn get_for_document(
    pool: &sqlx::SqlitePool,
    doc_id: DocumentId,
) -> sqlx::Result<Option<OutboxRow>> {
    let row: Option<OutboxRowTuple> = sqlx::query_as(
        "SELECT document_id, fiscal_number, sequence_no, payload_sha256, \
                    enqueued_at, status, published_at \
             FROM outbox WHERE document_id = ?",
    )
    .bind(doc_id)
    .fetch_optional(pool)
    .await?;
    let Some(r) = row else { return Ok(None) };
    let doc_id_arr: [u8; 16] = r.0.as_slice().try_into().map_err(|_| {
        sqlx::Error::Decode(
            format!("outbox.document_id: expected 16 bytes, got {}", r.0.len()).into(),
        )
    })?;
    let sha_arr: [u8; 32] = r.3.as_slice().try_into().map_err(|_| {
        sqlx::Error::Decode(
            format!(
                "outbox.payload_sha256: expected 32 bytes, got {}",
                r.3.len()
            )
            .into(),
        )
    })?;
    Ok(Some(OutboxRow {
        document_id: DocumentId::from_bytes(doc_id_arr),
        fiscal_number: r.1,
        sequence_no: r.2,
        payload_sha256: sha_arr,
        enqueued_at: r.4,
        status: r.5,
        published_at: r.6,
    }))
}
