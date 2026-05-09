//! Repository for `ingress_inbox`.
//!
//! Implements spec §6.2 / decision #31 — the most subtle correctness gate
//! in M1: idempotent ingress with three distinct outcomes.
//!
//! Behaviour of `insert(NewInboxEntry)`:
//! - **Created** — first time `(fiscal_number, idempotency_key)` is seen;
//!   the row is persisted with `status='NEW'` and the populated row is
//!   returned.
//! - **Replay** — the same `(fn, idem_key)` is already on file AND the
//!   submitted `payload_sha256_canonical` matches the stored one; the
//!   existing row is returned.  Caller MUST treat this as success
//!   (idempotent retry of the same logical request) and MUST NOT
//!   re-process.
//! - **Conflict** — same `(fn, idem_key)` but a different payload hash;
//!   returns both hashes so the caller can audit the discrepancy.
//!   This is NEVER a silent replay — the gateway refuses to reuse an
//!   idempotency key for a different payload.
//!
//! All branches run inside a single `db::tx::with_immediate` transaction
//! so the probe-then-insert is atomic against concurrent ingress
//! attempts on the same key.  This is the canonical use-site for the
//! `with_immediate` primitive in M1.

use crate::db::models::enums::Protocol;
use crate::db::tx::{with_immediate, WriteTxConn};
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct InboxRow {
    pub request_id: [u8; 16],
    pub fiscal_number: String,
    pub protocol: Protocol,
    pub operation_type: String,
    pub idempotency_key: String,
    pub status: String,
    pub payload_json: String,
    pub payload_sha256_canonical: [u8; 32],
    pub correlation_id: Option<String>,
    pub received_at: String,
}

#[derive(Debug, Clone)]
pub struct NewInboxEntry {
    pub request_id: [u8; 16],
    pub fiscal_number: String,
    pub protocol: Protocol,
    pub operation_type: String,
    pub idempotency_key: String,
    pub payload_json: String,
    pub payload_sha256_canonical: [u8; 32],
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone)]
pub enum InboxInsertOutcome {
    Created(InboxRow),
    Replay(InboxRow),
    Conflict {
        existing_payload_hash: [u8; 32],
        submitted_payload_hash: [u8; 32],
    },
}

pub async fn insert(pool: &SqlitePool, n: &NewInboxEntry) -> anyhow::Result<InboxInsertOutcome> {
    let n = n.clone();
    with_immediate(pool, move |conn| {
        Box::pin(async move {
            // Step 1: probe by (fn, idem_key) inside the RESERVED-locked tx.
            let existing = sqlx::query!(
                r#"SELECT request_id      as "request_id: Vec<u8>",
                          fiscal_number,
                          protocol         as "protocol: Protocol",
                          operation_type,
                          idempotency_key,
                          status,
                          payload_json,
                          payload_sha256_canonical as "payload_sha256_canonical: Vec<u8>",
                          correlation_id,
                          received_at
                   FROM ingress_inbox
                   WHERE fiscal_number = ? AND idempotency_key = ?"#,
                n.fiscal_number,
                n.idempotency_key
            )
            .fetch_optional(&mut **conn)
            .await?;

            if let Some(r) = existing {
                let existing_hash: [u8; 32] = r
                    .payload_sha256_canonical
                    .as_slice()
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("bad sha256 length in inbox row"))?;
                let request_id: [u8; 16] = r
                    .request_id
                    .as_slice()
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("bad request_id length in inbox row"))?;
                if existing_hash == n.payload_sha256_canonical {
                    return Ok(InboxInsertOutcome::Replay(InboxRow {
                        request_id,
                        fiscal_number: r.fiscal_number,
                        protocol: r.protocol,
                        operation_type: r.operation_type,
                        idempotency_key: r.idempotency_key,
                        status: r.status,
                        payload_json: r.payload_json,
                        payload_sha256_canonical: existing_hash,
                        correlation_id: r.correlation_id,
                        received_at: r.received_at,
                    }));
                } else {
                    return Ok(InboxInsertOutcome::Conflict {
                        existing_payload_hash: existing_hash,
                        submitted_payload_hash: n.payload_sha256_canonical,
                    });
                }
            }

            // Step 2: no existing row — insert.
            sqlx::query(
                "INSERT INTO ingress_inbox (
                     request_id, fiscal_number, protocol, operation_type,
                     idempotency_key, payload_json, payload_sha256_canonical, correlation_id
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&n.request_id[..])
            .bind(&n.fiscal_number)
            .bind(n.protocol)
            .bind(&n.operation_type)
            .bind(&n.idempotency_key)
            .bind(&n.payload_json)
            .bind(&n.payload_sha256_canonical[..])
            .bind(n.correlation_id.as_deref())
            .execute(&mut **conn)
            .await?;

            // Read back received_at (server-side default) so the caller sees
            // the canonical row as persisted.
            let received_at: String = sqlx::query_scalar(
                "SELECT received_at FROM ingress_inbox WHERE fiscal_number = ? AND idempotency_key = ?",
            )
            .bind(&n.fiscal_number)
            .bind(&n.idempotency_key)
            .fetch_one(&mut **conn)
            .await?;

            Ok(InboxInsertOutcome::Created(InboxRow {
                request_id: n.request_id,
                fiscal_number: n.fiscal_number.clone(),
                protocol: n.protocol,
                operation_type: n.operation_type.clone(),
                idempotency_key: n.idempotency_key.clone(),
                status: "NEW".to_string(),
                payload_json: n.payload_json.clone(),
                payload_sha256_canonical: n.payload_sha256_canonical,
                correlation_id: n.correlation_id.clone(),
                received_at,
            }))
        })
    })
    .await
}

/// W5 / W0-1 §3.1 stage 1 — atomically claim an inbox row for
/// processing.  CAS `status = 'NEW' → 'PROCESSING'` keyed on
/// `request_id`.  Returns:
///   - `Some(row)` — lease acquired; row state was `NEW`, now
///     `PROCESSING`.
///   - `None` — lease miss; row was already `PROCESSING` /
///     `DONE` / `REJECTED` / `ERROR` (another worker has it,
///     or processing is complete).  Caller maps to
///     `WorkerProcessResult::Noop`.
///
/// Orthogonal to `insert`: this method takes an existing row and
/// flips its status atomically inside the worker's
/// `with_immediate` envelope.  Replay / conflict detection at
/// ingress time is handled by `insert` against UNIQUE
/// `(fiscal_number, idempotency_key)` and is NOT involved here.
pub async fn acquire_lease(
    tx: &mut WriteTxConn<'_>,
    request_id: &[u8; 16],
) -> sqlx::Result<Option<InboxRow>> {
    let req_slice: &[u8] = request_id;
    let row = sqlx::query!(
        r#"UPDATE ingress_inbox
           SET status = 'PROCESSING', processed_at = CURRENT_TIMESTAMP
           WHERE request_id = ? AND status = 'NEW'
           RETURNING request_id      as "request_id: Vec<u8>",
                     fiscal_number,
                     protocol         as "protocol: Protocol",
                     operation_type,
                     idempotency_key,
                     status,
                     payload_json,
                     payload_sha256_canonical as "payload_sha256_canonical: Vec<u8>",
                     correlation_id,
                     received_at"#,
        req_slice
    )
    .fetch_optional(&mut **tx)
    .await?;
    let Some(r) = row else { return Ok(None) };
    let req_array: [u8; 16] = r
        .request_id
        .as_slice()
        .try_into()
        .map_err(|_| sqlx::Error::Decode("inbox.request_id length != 16".into()))?;
    let sha_array: [u8; 32] = r
        .payload_sha256_canonical
        .as_slice()
        .try_into()
        .map_err(|_| sqlx::Error::Decode("inbox.payload_sha256_canonical length != 32".into()))?;
    Ok(Some(InboxRow {
        request_id: req_array,
        fiscal_number: r.fiscal_number,
        protocol: r.protocol,
        operation_type: r.operation_type,
        idempotency_key: r.idempotency_key,
        status: r.status,
        payload_json: r.payload_json,
        payload_sha256_canonical: sha_array,
        correlation_id: r.correlation_id,
        received_at: r.received_at,
    }))
}

/// W5 stage 1 reject path — finalise an inbox row to terminal
/// `REJECTED` status without going through `transition_state`.
/// Used when a guard fails (NodeMode != Online, shift state
/// mismatch, missing profile binding, invalid payload).  No
/// `fiscal_documents` row is created and no `lnd` is allocated;
/// the inbox row carries the rejection.
pub async fn mark_rejected_tx(tx: &mut WriteTxConn<'_>, request_id: &[u8; 16]) -> sqlx::Result<()> {
    let req_slice: &[u8] = request_id;
    sqlx::query(
        "UPDATE ingress_inbox \
         SET status = 'REJECTED', processed_at = CURRENT_TIMESTAMP \
         WHERE request_id = ?",
    )
    .bind(req_slice)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// W8 stage 5 finalize — finalise the inbox row to terminal `DONE`
/// status **inside** the same `with_immediate` envelope as the CAS
/// `Kvt2 → Ack` on `fiscal_documents`.  Tx-bound; mirror of
/// [`mark_rejected_tx`] but with `status = 'DONE'`.
///
/// **Atomicity contract.**  Caller (`stage_finalize::run`) MUST
/// invoke this after the CAS Applied AND the seed advance AND
/// before / alongside the outbox INSERT + audit row.  All five
/// writes commit atomically; partial commits are impossible by
/// construction.
///
/// Returns `true` if the inbox row existed and was updated; `false`
/// indicates a missing `request_id`, which is a state-invariant
/// breach (the inbox row is the very thing that drives the worker;
/// it must exist for stage 5 to run).  Caller MUST treat `false` as
/// a typed stage error (`StageFinalizeError::InboxDoneMissing`),
/// not silent ignore.
pub async fn mark_done_tx(tx: &mut WriteTxConn<'_>, request_id: &[u8; 16]) -> sqlx::Result<bool> {
    let req_slice: &[u8] = request_id;
    let res = sqlx::query(
        "UPDATE ingress_inbox \
         SET status = 'DONE', processed_at = CURRENT_TIMESTAMP \
         WHERE request_id = ?",
    )
    .bind(req_slice)
    .execute(&mut **tx)
    .await?;
    Ok(res.rows_affected() == 1)
}
