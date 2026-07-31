//! Repository for `audit_log`.
//!
//! Append-only log per spec §4 — no FK to entities, so audit rows can
//! outlive the entities they reference (intentional: legal trail).
//! `entity_type` + `entity_id` are loose TEXT search keys.
//!
//! Repo policy:
//! - `append` runtime-bound `sqlx::query()`; returns the auto-incremented
//!   `audit_id` (sqlite ROWID) so caller can correlate.
//! - `list_for_entity` uses `sqlx::query!` for compile-time decode of the
//!   `severity: Severity` enum.  Ordered DESC by `audit_id` — the AUTOINCREMENT
//!   PK is monotonic and not subject to the `created_at` second-granularity
//!   issue, so it gives stable reverse-chronological order.

use crate::db::models::enums::Severity;
use crate::db::tx::WriteTxConn;
use crate::db::types::DbSeverity;
use sqlx::SqlitePool;

/// Hard cap on `list_for_entity` to prevent unbounded queries from admin UI /
/// API.  SQLite treats negative LIMIT as "no limit", so passing a negative
/// value is forbidden at the type system level (`limit: u32`); this constant
/// caps the upper end as well so a stray `u32::MAX` cannot drag the entire
/// audit log into memory.
pub const MAX_LIST_LIMIT: u32 = 10_000;

#[derive(Debug, Clone, PartialEq)]
pub struct AuditEntry {
    pub audit_id: i64,
    pub entity_type: String,
    pub entity_id: String,
    pub event_type: String,
    pub severity: Severity,
    pub actor: Option<String>,
    pub event_payload_json: Option<String>,
    pub created_at: String,
}

pub async fn append(
    pool: &SqlitePool,
    entity_type: &str,
    entity_id: &str,
    event_type: &str,
    severity: Severity,
    actor: Option<&str>,
    payload_json: Option<&str>,
) -> sqlx::Result<i64> {
    let res = sqlx::query(
        "INSERT INTO audit_log (entity_type, entity_id, event_type, severity, actor, event_payload_json) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(event_type)
    .bind(severity.as_str())
    .bind(actor)
    .bind(payload_json)
    .execute(pool)
    .await?;
    Ok(res.last_insert_rowid())
}

/// W5 — same INSERT as [`append`] but driven through
/// `&mut WriteTxConn<'_>` so stage 1 can append audit rows inside
/// the same `with_immediate` envelope as the lease / lnd-allocate /
/// INSERT PREPARED writes.  Pool-version is preserved for non-tx
/// call sites (post-stage events, ingress shells, etc.).
pub async fn append_tx(
    tx: &mut WriteTxConn<'_>,
    entity_type: &str,
    entity_id: &str,
    event_type: &str,
    severity: Severity,
    actor: Option<&str>,
    payload_json: Option<&str>,
) -> sqlx::Result<i64> {
    let res = sqlx::query(
        "INSERT INTO audit_log (entity_type, entity_id, event_type, severity, actor, event_payload_json) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(event_type)
    .bind(severity.as_str())
    .bind(actor)
    .bind(payload_json)
    .execute(&mut **tx)
    .await?;
    Ok(res.last_insert_rowid())
}

pub async fn list_for_entity(
    pool: &SqlitePool,
    entity_type: &str,
    entity_id: &str,
    limit: u32,
) -> sqlx::Result<Vec<AuditEntry>> {
    let bounded: i64 = limit.min(MAX_LIST_LIMIT).into();
    let rows = sqlx::query!(
        r#"SELECT audit_id           as "audit_id!",
                  entity_type, entity_id, event_type,
                  severity            as "severity: DbSeverity",
                  actor, event_payload_json, created_at
           FROM audit_log
           WHERE entity_type = ? AND entity_id = ?
           ORDER BY audit_id DESC
           LIMIT ?"#,
        entity_type,
        entity_id,
        bounded
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| AuditEntry {
            audit_id: r.audit_id,
            entity_type: r.entity_type,
            entity_id: r.entity_id,
            event_type: r.event_type,
            severity: r.severity.0,
            actor: r.actor,
            event_payload_json: r.event_payload_json,
            created_at: r.created_at,
        })
        .collect())
}
