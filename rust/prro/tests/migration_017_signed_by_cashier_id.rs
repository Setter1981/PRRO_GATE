//! W14a-2b Commit 7 — targeted verification for migration 017
//! (`signed_by_cashier_id` column on `fiscal_documents`).
//!
//! NOTE (2026-06 baseline squash): the incremental upgrade-from-016 test
//! (`migration_017_true_upgrade_from_016_preserves_existing_rows`, which
//! replayed versions <17 then 017) was removed — that step is unreachable
//! post-squash by construction (pre-squash chain + tests at git ref
//! 5c6b00a3a9895fd634c322d02dc6c3d925dfcc4b).  The idempotent-runner contract
//! test was kept and adapted (recorded-count bound `>= 17` → `>= 1`, since the
//! chain is now a single baseline migration; the no-op invariant is unchanged).
//!
//! Three contracts:
//!
//!   1. Fresh-apply contract: a brand-new DB lands at the post-017
//!      schema → column exists with NULL default + no constraint
//!      block on existing-NULL writes.
//!   2. Upgrade-from-016 contract: existing fiscal_documents rows
//!      pre-017 survive the ALTER TABLE ADD COLUMN cleanly +
//!      receive NULL for the new field (no DDL-level back-fill).
//!   3. Idempotent runner contract: sqlx `migrate run` re-execution
//!      against an already-017 DB is a no-op (recorded-checksum
//!      gate prevents a second ALTER, which would fail with
//!      "duplicate column").
//!
//! Anchored on spec
//! `docs/superpowers/specs/2026-05-19-w14a-2b-signer-channel.md` §2.2 +
//! Commit 1 plumbing.  Mirrors the structure of
//! `migration_013_mac_recovery.rs` / `migration_010_transport_trace.rs`.

use sqlx::{Row, SqlitePool};

/// Open a fresh pool driven through the full migration set (1..=017).
async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations including 017");
    (dir, pool)
}

/// Query SQLite's `PRAGMA table_info(...)` to discover whether a
/// column exists by name on a given table.
async fn column_exists(pool: &SqlitePool, table: &str, column: &str) -> bool {
    let q = format!("PRAGMA table_info({table})");
    let rows = sqlx::query(&q).fetch_all(pool).await.unwrap();
    rows.iter()
        .any(|r| r.try_get::<String, _>("name").unwrap_or_default() == column)
}

// ─── Contract 1: fresh-apply ─────────────────────────────────────────

/// Fresh DB → column exists, default NULL on new rows, no CHECK
/// constraint blocks NULL writes (column was added NULLABLE per spec
/// §2.2 — no back-fill source for pre-W14a-2b ledger rows).
#[tokio::test]
async fn migration_017_fresh_apply_adds_nullable_signed_by_cashier_id_column() {
    let (_d, pool) = fresh_pool().await;

    assert!(
        column_exists(&pool, "fiscal_documents", "signed_by_cashier_id").await,
        "post-017 fresh apply: fiscal_documents.signed_by_cashier_id MUST exist",
    );

    // Insert without specifying signed_by_cashier_id → default NULL.
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let doc_bytes = vec![0xAAu8; 16];
    let req_bytes = vec![0x55u8; 16];
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, '1234567890', 1, 'SELL', 'SIGNED', 'b1', 't1', 'ONLINE', \
            '2026-05-20T12:00:00Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(&sha)
    .execute(&pool)
    .await
    .expect("post-017 INSERT without signed_by_cashier_id must succeed (NULLABLE)");

    let value: Option<String> = sqlx::query_scalar(
        "SELECT signed_by_cashier_id FROM fiscal_documents WHERE document_id = ?",
    )
    .bind(&doc_bytes)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        value.is_none(),
        "post-017 fresh apply: signed_by_cashier_id MUST default to NULL when omitted",
    );
}

// ─── Contract 3: idempotent runner re-run ────────────────────────────

/// Calling `migrate run` against a fully-migrated DB MUST be a no-op
/// — the recorded `_sqlx_migrations` row blocks a second `ALTER TABLE
/// ADD COLUMN`, which would otherwise fail with "duplicate column"
/// per SQLite's strict ALTER semantics.
///
/// NIT-clarification (per operator C1 senior review):
/// `ALTER TABLE ADD COLUMN` itself is NOT idempotent in SQLite — the
/// idempotency comes from the migration runner's checksum gate.  This
/// test exercises THAT, not raw ALTER re-apply.
#[tokio::test]
async fn migration_017_runner_rerun_is_noop_via_recorded_checksum() {
    let (_d, pool) = fresh_pool().await;

    // Count migration runner records before re-run.
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    // 2026-06 baseline squash: the chain collapsed to a single `001_baseline`
    // migration, so the recorded count is now `>= 1` (was `>= 17` pre-squash).
    // This test's contract is the idempotent-rerun checksum gate, NOT the chain
    // length — only the count bound is adapted; the no-op assertion below is the
    // real invariant.
    assert!(
        before >= 1,
        "fresh pool MUST record at least the baseline migration (got {before})",
    );

    // Re-run the migrator.  This MUST be a no-op (no panic, no
    // additional rows in `_sqlx_migrations`).
    let migrator = sqlx::migrate::Migrator::new(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations"
    )))
    .await
    .expect("load migrations");
    migrator
        .run(&pool)
        .await
        .expect("migrator re-run MUST be a no-op (recorded checksum gate)");

    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        before, after,
        "runner re-run MUST NOT add a duplicate 017 record (got before={before}, after={after})",
    );

    // Sanity: column still exists post re-run.
    assert!(
        column_exists(&pool, "fiscal_documents", "signed_by_cashier_id").await,
        "column MUST persist through runner re-run (no raw ALTER re-apply)",
    );
}
