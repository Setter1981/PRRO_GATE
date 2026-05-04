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
use crate::db::tx::with_immediate;
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
            .fetch_optional(&mut *conn)
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
            .execute(&mut *conn)
            .await?;

            // Read back received_at (server-side default) so the caller sees
            // the canonical row as persisted.
            let received_at: String = sqlx::query_scalar(
                "SELECT received_at FROM ingress_inbox WHERE fiscal_number = ? AND idempotency_key = ?",
            )
            .bind(&n.fiscal_number)
            .bind(&n.idempotency_key)
            .fetch_one(&mut *conn)
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
