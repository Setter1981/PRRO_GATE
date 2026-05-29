//! W2 PR-A iter 4 — migration 020 schema verification.
//!
//! Per `docs/superpowers/plans/2026-05-25-m4-ingress-plan.md` §3 W2
//! "Tests" first bullet — verifies the `operators` table has the
//! expected columns, NOT NULL constraints, the partial unique index
//! (MED-PR90-01), and the lookup index.
//!
//! Constraint behaviour (CHECK rejections, partial uniqueness under
//! `is_active = 1`) lives in dedicated fixtures
//! `migration_020_fk_constraint.rs` (HIGH-PR90-01) and
//! `operators_multi_cashier_history.rs` (MED-PR90-01).  This file is
//! schema-shape only — proves the migration applied the right DDL.

use sqlx::{Row, SqlitePool};

async fn fresh_secure_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_secure_pool(&dir.path().join("secure.db"))
        .await
        .expect("open_secure_pool runs migration 020");
    (dir, pool)
}

#[tokio::test]
async fn migration_020_creates_operators_table_with_expected_columns() {
    let (_dir, pool) = fresh_secure_pool().await;

    let rows = sqlx::query("PRAGMA table_info(operators)")
        .fetch_all(&pool)
        .await
        .expect("PRAGMA table_info");

    // Column name -> (notnull, dflt_value present)
    let cols: std::collections::HashMap<String, (i64, bool)> = rows
        .into_iter()
        .map(|r| {
            let name: String = r.get("name");
            let notnull: i64 = r.get("notnull");
            let dflt: Option<String> = r.get("dflt_value");
            (name, (notnull, dflt.is_some()))
        })
        .collect();

    // Required columns + NOT NULL
    for required in [
        "operator_id",
        "fiscal_number",
        "name",
        "key_path",
        "key_pass_enc",
        "is_active",
        "created_at",
    ] {
        let (notnull, _dflt) = cols
            .get(required)
            .unwrap_or_else(|| panic!("column {required} missing"));
        assert_eq!(*notnull, 1, "{required} must be NOT NULL");
    }

    // is_active has a default (1)
    assert!(cols["is_active"].1, "is_active must have DEFAULT value");

    // created_at has a default (CURRENT_TIMESTAMP)
    assert!(cols["created_at"].1, "created_at must have DEFAULT value");
}

#[tokio::test]
async fn migration_020_creates_partial_active_unique_index() {
    let (_dir, pool) = fresh_secure_pool().await;

    let rows = sqlx::query(
        "SELECT name, sql FROM sqlite_master \
         WHERE type = 'index' AND tbl_name = 'operators'",
    )
    .fetch_all(&pool)
    .await
    .expect("query sqlite_master for operators indices");

    let mut has_active_uidx = false;
    let mut has_lookup_idx = false;

    for r in rows {
        let name: String = r.get("name");
        let sql: Option<String> = r.get("sql");
        let sql = sql.unwrap_or_default();
        if name == "operators_active_fn_uidx" {
            has_active_uidx = true;
            assert!(
                sql.contains("WHERE is_active = 1"),
                "operators_active_fn_uidx must be partial WHERE is_active = 1, got: {sql}"
            );
            assert!(
                sql.contains("UNIQUE"),
                "operators_active_fn_uidx must be UNIQUE, got: {sql}"
            );
        }
        if name == "operators_fiscal_number_idx" {
            has_lookup_idx = true;
        }
    }

    assert!(has_active_uidx, "operators_active_fn_uidx must exist");
    assert!(has_lookup_idx, "operators_fiscal_number_idx must exist");
}

#[tokio::test]
async fn migration_020_recorded_with_version_20_in_sqlx_migrations() {
    let (_dir, pool) = fresh_secure_pool().await;

    let version: i64 = sqlx::query("SELECT version FROM _sqlx_migrations WHERE version = 20")
        .fetch_one(&pool)
        .await
        .expect("migration 020 row")
        .get(0);
    assert_eq!(version, 20);
}
