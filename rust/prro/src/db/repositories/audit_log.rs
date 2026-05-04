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
use sqlx::SqlitePool;

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
    .bind(severity)
    .bind(actor)
    .bind(payload_json)
    .execute(pool)
    .await?;
    Ok(res.last_insert_rowid())
}

pub async fn list_for_entity(
    pool: &SqlitePool,
    entity_type: &str,
    entity_id: &str,
    limit: i64,
) -> sqlx::Result<Vec<AuditEntry>> {
    let rows = sqlx::query!(
        r#"SELECT audit_id, entity_type, entity_id, event_type,
                  severity as "severity: Severity",
                  actor, event_payload_json, created_at
           FROM audit_log
           WHERE entity_type = ? AND entity_id = ?
           ORDER BY audit_id DESC
           LIMIT ?"#,
        entity_type,
        entity_id,
        limit
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| AuditEntry {
            audit_id: r.audit_id.unwrap_or_default(),
            entity_type: r.entity_type,
            entity_id: r.entity_id,
            event_type: r.event_type,
            severity: r.severity,
            actor: r.actor,
            event_payload_json: r.event_payload_json,
            created_at: r.created_at,
        })
        .collect())
}
