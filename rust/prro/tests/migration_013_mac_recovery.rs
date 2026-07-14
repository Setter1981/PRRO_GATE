//! Targeted verification for migration 013 DDL contracts (W10.4
//! step 1).
//!
//! Covers the three contracts the migration installs:
//!
//!   1. `fiscal_documents.mac_recovery_attempts INTEGER NOT NULL
//!      DEFAULT 0` — every freshly INSERTed doc starts with counter
//!      = 0 (the claim helper bumps to 1 inside MR-PERSIST).
//!   2. `CHECK (mac_recovery_attempts IN (0, 1))` — DDL-enforced
//!      single-bit budget per W0-3 §2.1 row -12.  Helper-level CAS
//!      guard provides defence-in-depth on top of this.
//!   3. `transport_trace.outcome_kind` CHECK list extended with
//!      `'RETRYABLE_MAC_HASH_MISMATCH'` via the migration-008-style
//!      table rebuild.  W7-frozen 5-value list (010) preserved;
//!      retry_class column from 012 preserved through the rebuild.
//!
//! Anchored on freeze §4.4 + R-W10.4-step1-review MED 1 close.

use prro::db::models::ids::DocumentId;
use prro::db::types::DbDocumentId;
use sqlx::SqlitePool;

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs migrations including 013");
    (dir, pool)
}

/// Seed FN config + a SIGNED fiscal_documents row.  Mirrors the
/// minimal seed used elsewhere; we deliberately do NOT touch
/// `mac_recovery_attempts` here so the DEFAULT applies.
async fn seed_doc(pool: &SqlitePool, doc_byte: u8) -> DocumentId {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(pool)
    .await
    .unwrap();
    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let sha = vec![0u8; 32];
    let lnd = doc_byte as i64;
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, '1234567890', ?, 'SELL', 'SIGNED', 'b1', 't1', 'ONLINE', \
            '2026-05-09T12:34:56Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(lnd)
    .bind(&sha)
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

// ─── (1) DEFAULT 0 on freshly INSERTed doc ───────────────────────────

#[tokio::test]
async fn mac_recovery_attempts_defaults_to_zero_on_insert() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x01).await;

    let counter: i64 = sqlx::query_scalar(
        "SELECT mac_recovery_attempts FROM fiscal_documents WHERE document_id = ?",
    )
    .bind(DbDocumentId(doc))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        counter, 0,
        "migration 013 must set DEFAULT 0 on the new column"
    );
}

// ─── (2) CHECK rejects values outside (0, 1) ─────────────────────────

#[tokio::test]
async fn mac_recovery_attempts_check_rejects_value_2() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x02).await;

    // Try to bump counter to 2 — must violate CHECK.
    let res =
        sqlx::query("UPDATE fiscal_documents SET mac_recovery_attempts = 2 WHERE document_id = ?")
            .bind(DbDocumentId(doc))
            .execute(&pool)
            .await;
    let err = res.expect_err("CHECK (mac_recovery_attempts IN (0, 1)) must reject 2");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("check") || msg.contains("constraint"),
        "expected CHECK-class error, got: {msg}"
    );

    // Negative also rejected (closed set IS (0, 1), -1 not in it).
    let res =
        sqlx::query("UPDATE fiscal_documents SET mac_recovery_attempts = -1 WHERE document_id = ?")
            .bind(DbDocumentId(doc))
            .execute(&pool)
            .await;
    let err = res.expect_err("CHECK must reject -1");
    let msg = err.to_string().to_lowercase();
    assert!(msg.contains("check") || msg.contains("constraint"));

    // Counter remained at 0 (rejected updates roll back).
    let counter: i64 = sqlx::query_scalar(
        "SELECT mac_recovery_attempts FROM fiscal_documents WHERE document_id = ?",
    )
    .bind(DbDocumentId(doc))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counter, 0, "rejected UPDATEs must NOT mutate the row");
}

// ─── (3) outcome_kind CHECK accepts the new wire string ──────────────

#[tokio::test]
async fn outcome_kind_check_accepts_retryable_mac_hash_mismatch() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x03).await;
    let sha = vec![0u8; 32];

    // Allocate a trace row (intent-only; outcome_kind NULL initially).
    sqlx::query(
        "INSERT INTO transport_trace(document_id, attempt_no, backend_profile_id, \
            transport_profile_id, request_envelope_sha256) \
         VALUES (?, 1, 'b1', 't1', ?)",
    )
    .bind(DbDocumentId(doc))
    .bind(&sha)
    .execute(&pool)
    .await
    .expect("alloc trace row");

    // Complete it with the new outcome_kind — must pass the extended
    // CHECK list installed by migration 013 (B).
    let res = sqlx::query(
        "UPDATE transport_trace SET \
            completed_at = '2026-05-09 10:00:01', \
            wire_call_started_at = '2026-05-09 10:00:00', \
            wire_call_finished_at = '2026-05-09 10:00:01', \
            outcome_kind = 'RETRYABLE_MAC_HASH_MISMATCH', \
            error_kind = 'Server', \
            error_message = 'ERROR_BAD_HASH_PREV', \
            server_status_code = -12, \
            retry_class = 'MacRecovery' \
         WHERE document_id = ? AND attempt_no = 1",
    )
    .bind(DbDocumentId(doc))
    .execute(&pool)
    .await;
    assert!(
        res.is_ok(),
        "outcome_kind = 'RETRYABLE_MAC_HASH_MISMATCH' MUST pass the extended CHECK: {res:?}"
    );

    // Sanity: an unknown value is still rejected (CHECK is closed).
    let res = sqlx::query(
        "UPDATE transport_trace SET outcome_kind = 'UNKNOWN_GARBAGE' \
         WHERE document_id = ? AND attempt_no = 1",
    )
    .bind(DbDocumentId(doc))
    .execute(&pool)
    .await;
    let err = res.expect_err("unknown outcome_kind must violate CHECK");
    let msg = err.to_string().to_lowercase();
    assert!(msg.contains("check") || msg.contains("constraint"));
}

// ─── (4) W7 + 012 contracts preserved through the rebuild ────────────
//
// Spot-check: a row with the W7-frozen kind ('OK') still works, and
// the retry_class column from migration 012 is still present + writable.

#[tokio::test]
async fn migration_010_w7_contracts_survive_the_013_rebuild() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x04).await;
    let sha = vec![0u8; 32];

    sqlx::query(
        "INSERT INTO transport_trace(document_id, attempt_no, backend_profile_id, \
            transport_profile_id, request_envelope_sha256) \
         VALUES (?, 1, 'b1', 't1', ?)",
    )
    .bind(DbDocumentId(doc))
    .bind(&sha)
    .execute(&pool)
    .await
    .expect("alloc trace row");

    // Complete with W7-frozen 'OK' + W10.2 retry_class column.
    sqlx::query(
        "UPDATE transport_trace SET \
            completed_at = '2026-05-09 10:00:01', \
            wire_call_started_at = '2026-05-09 10:00:00', \
            wire_call_finished_at = '2026-05-09 10:00:01', \
            outcome_kind = 'OK', \
            server_fiscal_no = 'DPS-FN-7777', \
            retry_class = NULL \
         WHERE document_id = ? AND attempt_no = 1",
    )
    .bind(DbDocumentId(doc))
    .execute(&pool)
    .await
    .expect("W7 'OK' completion must still work post-013 rebuild");

    // OK ⇒ server_fiscal_no NOT NULL AND length > 0 — the migration-010
    // outcome-specific invariant survives the rebuild.
    let res = sqlx::query(
        "INSERT INTO transport_trace(document_id, attempt_no, backend_profile_id, \
            transport_profile_id, request_envelope_sha256, completed_at, \
            wire_call_started_at, wire_call_finished_at, outcome_kind, server_fiscal_no) \
         VALUES (?, 2, 'b1', 't1', ?, '2026-05-09 10:00:01', \
            '2026-05-09 10:00:00', '2026-05-09 10:00:01', 'OK', NULL)",
    )
    .bind(DbDocumentId(doc))
    .bind(&sha)
    .execute(&pool)
    .await;
    let err = res.expect_err("OK without server_fiscal_no MUST still violate CHECK post-rebuild");
    let msg = err.to_string().to_lowercase();
    assert!(msg.contains("check") || msg.contains("constraint"));
}
