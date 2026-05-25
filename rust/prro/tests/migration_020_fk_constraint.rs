//! W2 PR-A iter 5 — HIGH-PR90-01 CHECK enforcement on `operators`.
//!
//! Per plan §3 W2 Tests / Acceptance HIGH-PR90-01:
//!   "migration 020 enforces 10-digit numeric `fiscal_number` CHECK +
//!    FK to fiscal_number_config(fiscal_number) ON DELETE RESTRICT.
//!    Test `migration_020_fk_constraint.rs` proves both rejections."
//!
//! ## FK gap
//!
//! The cross-DB FK from `operators.fiscal_number` (in `secure.db`) to
//! `fiscal_number_config.fiscal_number` (in `prro.db`) **cannot be
//! enforced by SQLite** — foreign keys do not cross database files.
//! The W2 plan acknowledges this and pushes the FK semantics to two
//! runtime compensating checks (CLI pre-INSERT + boot registry build).
//! See `migrations_secure/020_operators.sql` doc-block "Cross-DB
//! foreign-key gap" for the full rationale.
//!
//! This fixture therefore covers only the **CHECK constraint half**
//! of HIGH-PR90-01: 11-digit, 9-digit, non-numeric, and empty
//! `fiscal_number` are all rejected at the DB layer.  The FK half
//! lands its tests in W2 PR-B alongside `BindingsRegistry::build_from_db`
//! (file `operator_orphan_fn_audit.rs`) and the admin CLI pre-check.

use sqlx::SqlitePool;

async fn fresh_secure_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_secure_pool(&dir.path().join("secure.db"))
        .await
        .expect("open_secure_pool");
    (dir, pool)
}

async fn try_insert(pool: &SqlitePool, fiscal_number: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO operators \
            (operator_id, fiscal_number, name, key_path, key_pass_enc, is_active) \
         VALUES ('OP-001', ?, 'Test Cashier', '/tmp/k.dat', X'00', 1)",
    )
    .bind(fiscal_number)
    .execute(pool)
    .await
    .map(|_| ())
}

#[tokio::test]
async fn rejects_eleven_digit_fiscal_number() {
    let (_dir, pool) = fresh_secure_pool().await;
    let err = try_insert(&pool, "12345678901")
        .await
        .expect_err("11-digit FN must be CHECK-rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("CHECK") || msg.contains("constraint"),
        "expected CHECK constraint failure, got: {msg}"
    );
}

#[tokio::test]
async fn rejects_nine_digit_fiscal_number() {
    let (_dir, pool) = fresh_secure_pool().await;
    let err = try_insert(&pool, "123456789")
        .await
        .expect_err("9-digit FN must be CHECK-rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("CHECK") || msg.contains("constraint"),
        "expected CHECK constraint failure, got: {msg}"
    );
}

#[tokio::test]
async fn rejects_non_numeric_fiscal_number() {
    let (_dir, pool) = fresh_secure_pool().await;
    let err = try_insert(&pool, "ABCDEFGHIJ")
        .await
        .expect_err("non-numeric FN must be CHECK-rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("CHECK") || msg.contains("constraint"),
        "expected CHECK constraint failure, got: {msg}"
    );
}

#[tokio::test]
async fn rejects_fiscal_number_with_letter_in_middle() {
    let (_dir, pool) = fresh_secure_pool().await;
    let err = try_insert(&pool, "12345X7890")
        .await
        .expect_err("mixed alphanumeric FN must be CHECK-rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("CHECK") || msg.contains("constraint"),
        "expected CHECK constraint failure, got: {msg}"
    );
}

#[tokio::test]
async fn accepts_valid_ten_digit_fiscal_number() {
    let (_dir, pool) = fresh_secure_pool().await;
    try_insert(&pool, "4000000001")
        .await
        .expect("valid 10-digit numeric FN must be accepted");
}

#[tokio::test]
async fn rejects_is_active_value_outside_zero_one() {
    let (_dir, pool) = fresh_secure_pool().await;
    let err = sqlx::query(
        "INSERT INTO operators \
            (operator_id, fiscal_number, name, key_path, key_pass_enc, is_active) \
         VALUES ('OP-002', '4000000001', 'Test', '/tmp/k.dat', X'00', 2)",
    )
    .execute(&pool)
    .await
    .expect_err("is_active=2 must be CHECK-rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("CHECK") || msg.contains("constraint"),
        "expected CHECK constraint failure, got: {msg}"
    );
}
