//! W4-Z0 — migration 021 schema verification.
//!
//! Per `docs/superpowers/specs/2026-05-26-w4-z0-config-storage-spec.md`
//! §1 — verifies the 5 new tables (`tax_groups`, `payment_methods`,
//! `driver_tax_mapping`, `fn_integration_flags`, `fn_outgress_profile`)
//! have the expected columns, NOT NULL constraints, CHECK constraints,
//! and partial unique indices.
//!
//! Constraint behaviour (CHECK rejections, partial uniqueness under
//! `is_active = 1`) lives in dedicated fixtures landing alongside
//! repository implementations (one per table) — this file is
//! schema-shape only.

use sqlx::{Row, SqlitePool};

async fn fresh_secure_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_secure_pool(&dir.path().join("secure.db"))
        .await
        .expect("open_secure_pool runs migration 021");
    (dir, pool)
}

fn collect_columns(rows: Vec<sqlx::sqlite::SqliteRow>) -> std::collections::HashMap<String, (i64, bool)> {
    rows.into_iter()
        .map(|r| {
            let name: String = r.get("name");
            let notnull: i64 = r.get("notnull");
            let dflt: Option<String> = r.get("dflt_value");
            (name, (notnull, dflt.is_some()))
        })
        .collect()
}

#[tokio::test]
async fn migration_021_creates_tax_groups_table() {
    let (_dir, pool) = fresh_secure_pool().await;

    let rows = sqlx::query("PRAGMA table_info(tax_groups)")
        .fetch_all(&pool)
        .await
        .expect("PRAGMA table_info(tax_groups)");

    let cols = collect_columns(rows);

    for required in ["fn", "tx_num", "letter", "dtpr", "txpr", "txal", "txty", "is_active", "created_at"] {
        let (notnull, _) = cols
            .get(required)
            .unwrap_or_else(|| panic!("tax_groups column {required} missing"));
        assert_eq!(*notnull, 1, "tax_groups.{required} must be NOT NULL");
    }

    // Defaults expected on dtpr, txpr, txal, txty, is_active, created_at
    for with_default in ["dtpr", "txpr", "txal", "txty", "is_active", "created_at"] {
        assert!(
            cols[with_default].1,
            "tax_groups.{with_default} must have DEFAULT value"
        );
    }
}

#[tokio::test]
async fn migration_021_creates_tax_groups_partial_unique_letter_index() {
    let (_dir, pool) = fresh_secure_pool().await;

    let row = sqlx::query(
        "SELECT name, sql FROM sqlite_master \
         WHERE type = 'index' AND tbl_name = 'tax_groups' \
           AND name = 'idx_tax_groups_fn_letter'",
    )
    .fetch_one(&pool)
    .await
    .expect("idx_tax_groups_fn_letter must exist");

    let sql: Option<String> = row.get("sql");
    let sql = sql.unwrap_or_default();
    assert!(
        sql.contains("UNIQUE") && sql.contains("WHERE is_active"),
        "idx_tax_groups_fn_letter must be UNIQUE with WHERE is_active = 1, got: {sql}"
    );
}

#[tokio::test]
async fn migration_021_creates_payment_methods_table() {
    let (_dir, pool) = fresh_secure_pool().await;

    let rows = sqlx::query("PRAGMA table_info(payment_methods)")
        .fetch_all(&pool)
        .await
        .expect("PRAGMA table_info(payment_methods)");

    let cols = collect_columns(rows);

    for required in ["fn", "pay_index", "name", "iscash", "is_active", "created_at"] {
        let (notnull, _) = cols
            .get(required)
            .unwrap_or_else(|| panic!("payment_methods column {required} missing"));
        assert_eq!(*notnull, 1, "payment_methods.{required} must be NOT NULL");
    }
}

#[tokio::test]
async fn migration_021_creates_payment_methods_partial_unique_name_index() {
    let (_dir, pool) = fresh_secure_pool().await;

    let row = sqlx::query(
        "SELECT name, sql FROM sqlite_master \
         WHERE type = 'index' AND tbl_name = 'payment_methods' \
           AND name = 'idx_payment_methods_fn_name'",
    )
    .fetch_one(&pool)
    .await
    .expect("idx_payment_methods_fn_name must exist");

    let sql: Option<String> = row.get("sql");
    let sql = sql.unwrap_or_default();
    assert!(
        sql.contains("UNIQUE") && sql.contains("WHERE is_active"),
        "idx_payment_methods_fn_name must be UNIQUE with WHERE is_active = 1, got: {sql}"
    );
}

#[tokio::test]
async fn migration_021_creates_driver_tax_mapping_table() {
    let (_dir, pool) = fresh_secure_pool().await;

    let rows = sqlx::query("PRAGMA table_info(driver_tax_mapping)")
        .fetch_all(&pool)
        .await
        .expect("PRAGMA table_info(driver_tax_mapping)");

    let cols = collect_columns(rows);

    for required in ["driver_id", "driver_number", "canonical_tx_num", "is_active", "created_at"] {
        let (notnull, _) = cols
            .get(required)
            .unwrap_or_else(|| panic!("driver_tax_mapping column {required} missing"));
        assert_eq!(*notnull, 1, "driver_tax_mapping.{required} must be NOT NULL");
    }

    // driver_letter is nullable (optional audit field)
    let driver_letter = cols
        .get("driver_letter")
        .expect("driver_tax_mapping.driver_letter column missing");
    assert_eq!(driver_letter.0, 0, "driver_tax_mapping.driver_letter must be nullable");
}

#[tokio::test]
async fn migration_021_creates_fn_integration_flags_table() {
    let (_dir, pool) = fresh_secure_pool().await;

    let rows = sqlx::query("PRAGMA table_info(fn_integration_flags)")
        .fetch_all(&pool)
        .await
        .expect("PRAGMA table_info(fn_integration_flags)");

    let cols = collect_columns(rows);

    for required in ["fn", "flag_name", "flag_value", "created_at", "updated_at"] {
        let (notnull, _) = cols
            .get(required)
            .unwrap_or_else(|| panic!("fn_integration_flags column {required} missing"));
        assert_eq!(*notnull, 1, "fn_integration_flags.{required} must be NOT NULL");
    }
}

#[tokio::test]
async fn migration_021_creates_fn_outgress_profile_table() {
    let (_dir, pool) = fresh_secure_pool().await;

    let rows = sqlx::query("PRAGMA table_info(fn_outgress_profile)")
        .fetch_all(&pool)
        .await
        .expect("PRAGMA table_info(fn_outgress_profile)");

    let cols = collect_columns(rows);

    for required in ["fn", "profile", "updated_at"] {
        let (notnull, _) = cols
            .get(required)
            .unwrap_or_else(|| panic!("fn_outgress_profile column {required} missing"));
        assert_eq!(*notnull, 1, "fn_outgress_profile.{required} must be NOT NULL");
    }
}

#[tokio::test]
async fn migration_021_fn_outgress_profile_enforces_profile_check() {
    let (_dir, pool) = fresh_secure_pool().await;

    // Acceptable values: 'FSCO_ZZD', 'EVPZ_DPS'.
    sqlx::query(
        "INSERT INTO fn_outgress_profile (fn, profile) VALUES ('1234567890', 'FSCO_ZZD')",
    )
    .execute(&pool)
    .await
    .expect("FSCO_ZZD insert must succeed");

    sqlx::query(
        "INSERT INTO fn_outgress_profile (fn, profile) VALUES ('9876543210', 'EVPZ_DPS')",
    )
    .execute(&pool)
    .await
    .expect("EVPZ_DPS insert must succeed");

    // Non-acceptable value MUST be rejected by CHECK constraint.
    let invalid = sqlx::query(
        "INSERT INTO fn_outgress_profile (fn, profile) VALUES ('5555555555', 'BOGUS')",
    )
    .execute(&pool)
    .await;
    assert!(
        invalid.is_err(),
        "fn_outgress_profile.profile CHECK must reject non-enum values"
    );
}

#[tokio::test]
async fn migration_021_tax_groups_enforces_txal_range() {
    let (_dir, pool) = fresh_secure_pool().await;

    // Valid 0..=3
    for valid_txal in 0..=3 {
        sqlx::query("INSERT INTO tax_groups (fn, tx_num, letter, txal) VALUES (?, ?, ?, ?)")
            .bind(format!("100000000{valid_txal}"))
            .bind(1)
            .bind("А")
            .bind(valid_txal)
            .execute(&pool)
            .await
            .unwrap_or_else(|e| panic!("txal={valid_txal} must be accepted: {e}"));
    }

    // Invalid txal=4
    let invalid = sqlx::query("INSERT INTO tax_groups (fn, tx_num, letter, txal) VALUES (?, ?, ?, ?)")
        .bind("2000000000")
        .bind(1)
        .bind("А")
        .bind(4)
        .execute(&pool)
        .await;
    assert!(invalid.is_err(), "tax_groups.txal=4 must be rejected (range 0..=3)");
}

#[tokio::test]
async fn migration_021_recorded_with_version_21_in_sqlx_migrations() {
    let (_dir, pool) = fresh_secure_pool().await;

    let version: i64 = sqlx::query(
        "SELECT version FROM _sqlx_migrations WHERE version = 21",
    )
    .fetch_one(&pool)
    .await
    .expect("migration 021 row")
    .get(0);
    assert_eq!(version, 21);
}
