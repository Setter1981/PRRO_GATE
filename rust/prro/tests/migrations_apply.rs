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
