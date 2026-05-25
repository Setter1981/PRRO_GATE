//! W2 PR-A iter 6 — MED-PR90-01 multi-cashier-per-FN history.
//!
//! Per plan §3 W2 Acceptance MED-PR90-01:
//!   "schema supports multi-cashier-per-FN historical rows via partial
//!    unique index `WHERE is_active = 1`; pilot 1-cashier behaves
//!    identically, no schema rebuild needed for cashier rotation."
//!
//! Three contracts:
//!
//!   1. Two rows with the SAME fiscal_number — one `is_active = 0`
//!      (historical), one `is_active = 1` (current) — both land
//!      successfully.  The partial unique index `WHERE is_active = 1`
//!      sees only the active row and does not collide with the
//!      historical one.
//!   2. Inserting a SECOND `is_active = 1` row for the same FN fails:
//!      the partial unique index catches it.
//!   3. Any number of `is_active = 0` rows for the same FN are allowed
//!      (rotation history with no upper bound).

use sqlx::{Row, SqlitePool};

async fn fresh_secure_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_secure_pool(&dir.path().join("secure.db"))
        .await
        .expect("open_secure_pool");
    (dir, pool)
}

async fn insert(
    pool: &SqlitePool,
    operator_id: &str,
    fiscal_number: &str,
    is_active: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO operators \
            (operator_id, fiscal_number, name, key_path, key_pass_enc, is_active) \
         VALUES (?, ?, 'Cashier', '/tmp/k.dat', X'00', ?)",
    )
    .bind(operator_id)
    .bind(fiscal_number)
    .bind(is_active)
    .execute(pool)
    .await
    .map(|_| ())
}

#[tokio::test]
async fn historical_then_active_for_same_fn_both_succeed() {
    let (_dir, pool) = fresh_secure_pool().await;

    // Historical row (rotated-out cashier).
    insert(&pool, "OP-OLD", "4000000001", 0)
        .await
        .expect("historical is_active=0 row inserts");

    // Currently active cashier — same FN.
    insert(&pool, "OP-NEW", "4000000001", 1)
        .await
        .expect("current is_active=1 row inserts despite historical present");

    let total: i64 = sqlx::query("SELECT COUNT(*) FROM operators WHERE fiscal_number = '4000000001'")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    assert_eq!(total, 2, "both rows must persist");

    let active: i64 = sqlx::query(
        "SELECT COUNT(*) FROM operators WHERE fiscal_number = '4000000001' AND is_active = 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .get(0);
    assert_eq!(active, 1, "exactly one active row");
}

#[tokio::test]
async fn two_active_rows_for_same_fn_rejected_by_partial_unique() {
    let (_dir, pool) = fresh_secure_pool().await;

    insert(&pool, "OP-A", "4000000001", 1)
        .await
        .expect("first active row inserts");

    let err = insert(&pool, "OP-B", "4000000001", 1)
        .await
        .expect_err("second active row must be UNIQUE-rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("UNIQUE") || msg.contains("unique"),
        "expected UNIQUE constraint failure, got: {msg}"
    );
}

#[tokio::test]
async fn many_historical_rows_for_same_fn_allowed() {
    let (_dir, pool) = fresh_secure_pool().await;

    // Five rotations all historical (rotated out before any active landed).
    for i in 0..5 {
        let oid = format!("OP-OLD-{i}");
        insert(&pool, &oid, "4000000001", 0)
            .await
            .unwrap_or_else(|e| panic!("historical row {i} must insert: {e}"));
    }

    let count: i64 = sqlx::query(
        "SELECT COUNT(*) FROM operators WHERE fiscal_number = '4000000001' AND is_active = 0",
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .get(0);
    assert_eq!(count, 5, "five historical rows allowed for one FN");
}

#[tokio::test]
async fn different_fns_each_with_one_active_coexist() {
    let (_dir, pool) = fresh_secure_pool().await;

    insert(&pool, "OP-A", "4000000001", 1)
        .await
        .expect("FN-A active row");
    insert(&pool, "OP-B", "4000000002", 1)
        .await
        .expect("FN-B active row — different FN, partial index does not collide");

    let total: i64 = sqlx::query("SELECT COUNT(*) FROM operators WHERE is_active = 1")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    assert_eq!(total, 2, "one active per distinct FN");
}
