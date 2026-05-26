//! W2 PR-B piece 5 — `db::repositories::operators` CRUD roundtrip + typed errors.
//!
//! Per `docs/superpowers/plans/2026-05-25-m4-ingress-plan.md` §3 W2
//! "Repository test" bullet:
//!   "operators::insert Created + duplicate active cashier-on-FN Conflict."
//!
//! Plus list_all + find_by_fiscal_number happy-path coverage.

use prro::db::open_secure_pool;
use prro::db::repositories::operators::{
    self as repo, NewOperator, OperatorsRepoError,
};
use sqlx::SqlitePool;

async fn fresh_secure_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = open_secure_pool(&dir.path().join("secure.db"))
        .await
        .expect("open_secure_pool");
    (dir, pool)
}

fn sample(operator_id: &str, fn_id: &str) -> NewOperator {
    NewOperator {
        operator_id: operator_id.to_string(),
        fiscal_number: fn_id.to_string(),
        name: "Test Cashier".to_string(),
        key_path: "/var/keys/cashier.dat".to_string(),
        key_pass_enc: vec![0xCA, 0xFE, 0xBA, 0xBE],
    }
}

#[tokio::test]
async fn insert_and_find_by_fiscal_number_roundtrip() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("OP-001", "4000000001"))
        .await
        .expect("insert succeeds on empty table");

    let row = repo::find_by_fiscal_number(&pool, "4000000001")
        .await
        .expect("find_by_fiscal_number query")
        .expect("row present for inserted FN");
    assert_eq!(row.operator_id, "OP-001");
    assert_eq!(row.fiscal_number, "4000000001");
    assert_eq!(row.name, "Test Cashier");
    assert_eq!(row.key_path, "/var/keys/cashier.dat");
    assert_eq!(row.key_pass_enc, vec![0xCA, 0xFE, 0xBA, 0xBE]);
    assert!(row.is_active, "newly inserted row is active by default");
}

#[tokio::test]
async fn find_by_fiscal_number_missing_returns_none() {
    let (_dir, pool) = fresh_secure_pool().await;
    let result = repo::find_by_fiscal_number(&pool, "4000000099")
        .await
        .expect("query succeeds even when no row");
    assert!(result.is_none(), "missing FN must return None, not error");
}

#[tokio::test]
async fn duplicate_active_cashier_on_same_fn_returns_typed_conflict() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("OP-A", "4000000001"))
        .await
        .expect("first insert");

    // Second active row on the same FN must be caught by the partial
    // unique index and surface as a typed Conflict, NOT raw sqlx error.
    let err = repo::insert(&pool, &sample("OP-B", "4000000001"))
        .await
        .expect_err("second active insert must conflict");

    match err {
        OperatorsRepoError::DuplicateActive(fn_id) => {
            assert_eq!(fn_id, "4000000001");
        }
        other => panic!("expected DuplicateActive, got: {other:?}"),
    }
}

#[tokio::test]
async fn list_all_returns_all_inserted_rows() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("OP-A", "4000000001"))
        .await
        .unwrap();
    repo::insert(&pool, &sample("OP-B", "4000000002"))
        .await
        .unwrap();

    let mut rows = repo::list_all(&pool).await.expect("list_all");
    rows.sort_by(|a, b| a.operator_id.cmp(&b.operator_id));

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].operator_id, "OP-A");
    assert_eq!(rows[0].fiscal_number, "4000000001");
    assert_eq!(rows[1].operator_id, "OP-B");
    assert_eq!(rows[1].fiscal_number, "4000000002");
}

#[tokio::test]
async fn list_all_on_empty_returns_empty_vec() {
    let (_dir, pool) = fresh_secure_pool().await;
    let rows = repo::list_all(&pool).await.expect("list_all on empty");
    assert!(rows.is_empty());
}
