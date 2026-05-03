//! One verification path for all M1 migrations: apply via `sqlx::migrate!`,
//! assert table/index set, and prove STRICT typing rejects bad inserts.
//!
//! Runs against the bundled libsqlite3-sys (NOT the system `sqlite3` CLI),
//! which is the runtime SQLite the gateway will actually use.

use std::collections::HashSet;

async fn fresh_pool() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs migrations");
    (dir, pool)
}

#[tokio::test]
async fn migration_001_creates_core_tables() {
    let (_d, pool) = fresh_pool().await;
    let names: HashSet<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table' ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .collect();
    for t in ["fiscal_number_config", "shifts", "node_state", "audit_log"] {
        assert!(names.contains(t), "missing table {t}; have {names:?}");
    }
}

#[tokio::test]
async fn migration_002_fiscal_documents_carries_both_hash_columns() {
    let (_d, pool) = fresh_pool().await;
    // PRAGMA table_info returns rows: (cid, name, type, notnull, dflt, pk).
    let cols: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(fiscal_documents)")
            .fetch_all(&pool)
            .await
            .unwrap();
    let names: HashSet<String> = cols.iter().map(|c| c.1.clone()).collect();
    for col in [
        "payload_sha256_canonical",
        "unsigned_xml_sha256",
        "submission_attempted_at",
    ] {
        assert!(names.contains(col), "fiscal_documents missing {col}; have {names:?}");
    }
}

#[tokio::test]
async fn migration_002_ingress_inbox_has_unique_idempotency_index() {
    let (_d, pool) = fresh_pool().await;
    // Confirm the unique index exists.
    let idx: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='index' AND name='ux_inbox_fn_idem'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(idx.as_deref(), Some("ux_inbox_fn_idem"));

    // Behavioural confirmation: duplicate (fn, idem_key) is rejected.
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let req1 = vec![1u8; 16];
    let req2 = vec![2u8; 16];
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical) \
         VALUES (?, '1234567890', 'REST', 'sell', 'idem-1', '{}', ?)",
    )
    .bind(&req1)
    .bind(&sha)
    .execute(&pool)
    .await
    .unwrap();

    let dup_err = sqlx::query(
        "INSERT INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical) \
         VALUES (?, '1234567890', 'REST', 'sell', 'idem-1', '{}', ?)",
    )
    .bind(&req2)
    .bind(&sha)
    .execute(&pool)
    .await
    .expect_err("duplicate (fn, idem_key) must violate UNIQUE");
    let msg = dup_err.to_string().to_lowercase();
    assert!(
        msg.contains("unique") || msg.contains("constraint"),
        "expected UNIQUE constraint error, got: {msg}"
    );
}

#[tokio::test]
async fn migration_001_strict_typing_rejects_text_in_int_column() {
    let (_d, pool) = fresh_pool().await;
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .expect("baseline insert");

    let err = sqlx::query(
        "UPDATE fiscal_number_config SET tsp_enabled = 'abc' WHERE fiscal_number = '1234567890'",
    )
    .execute(&pool)
    .await
    .expect_err("STRICT must reject TEXT in INTEGER column");

    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("integer") || msg.contains("strict") || msg.contains("type"),
        "expected STRICT/type-mismatch error, got: {msg}"
    );
}
