//! W4-Z0 piece 3 — `db::repositories::payment_methods` CRUD + typed errors.
//!
//! Per `docs/superpowers/specs/2026-05-26-w4-z0-config-storage-spec.md`
//! §1.2 + §3.2.  Pattern mirrors `tax_groups` repo (piece 2).

use prro::db::open_secure_pool;
use prro::db::repositories::payment_methods::{
    self as repo, NewPaymentMethod, PaymentMethodsRepoError,
};
use sqlx::SqlitePool;

async fn fresh_secure_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = open_secure_pool(&dir.path().join("secure.db"))
        .await
        .expect("open_secure_pool");
    (dir, pool)
}

fn sample(fn_id: &str, pay_index: i64, name: &str, iscash: bool) -> NewPaymentMethod {
    NewPaymentMethod {
        fn_id: fn_id.to_string(),
        pay_index,
        name: name.to_string(),
        iscash,
    }
}

#[tokio::test]
async fn insert_and_find_roundtrip() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("4000000001", 1, "Готівка", true))
        .await
        .expect("insert");

    let row = repo::find(&pool, "4000000001", 1)
        .await
        .expect("find query")
        .expect("row present");
    assert_eq!(row.fn_id, "4000000001");
    assert_eq!(row.pay_index, 1);
    assert_eq!(row.name, "Готівка");
    assert!(row.iscash);
    assert!(row.is_active);
}

#[tokio::test]
async fn find_missing_returns_none() {
    let (_dir, pool) = fresh_secure_pool().await;
    let result = repo::find(&pool, "4000000099", 1)
        .await
        .expect("query");
    assert!(result.is_none());
}

#[tokio::test]
async fn find_by_name_returns_active_row() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("4000000001", 1, "Готівка", true))
        .await
        .unwrap();
    repo::insert(&pool, &sample("4000000001", 2, "Картка", false))
        .await
        .unwrap();

    let row = repo::find_by_name(&pool, "4000000001", "Картка")
        .await
        .expect("query")
        .expect("active row present");
    assert_eq!(row.pay_index, 2);
    assert!(!row.iscash);
}

#[tokio::test]
async fn duplicate_pay_index_per_fn_returns_typed_conflict() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("4000000001", 1, "Готівка", true))
        .await
        .unwrap();

    let err = repo::insert(&pool, &sample("4000000001", 1, "Картка", false))
        .await
        .expect_err("second insert with same pay_index must conflict");

    match err {
        PaymentMethodsRepoError::DuplicatePayIndex { fn_id, pay_index } => {
            assert_eq!(fn_id, "4000000001");
            assert_eq!(pay_index, 1);
        }
        other => panic!("expected DuplicatePayIndex, got: {other:?}"),
    }
}

#[tokio::test]
async fn duplicate_active_name_per_fn_returns_typed_conflict() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("4000000001", 1, "Готівка", true))
        .await
        .unwrap();

    let err = repo::insert(&pool, &sample("4000000001", 2, "Готівка", false))
        .await
        .expect_err("second active row with same name must conflict");

    match err {
        PaymentMethodsRepoError::DuplicateActiveName { fn_id, name } => {
            assert_eq!(fn_id, "4000000001");
            assert_eq!(name, "Готівка");
        }
        other => panic!("expected DuplicateActiveName, got: {other:?}"),
    }
}

#[tokio::test]
async fn soft_delete_then_reinsert_same_name_succeeds() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("4000000001", 1, "Готівка", true))
        .await
        .unwrap();
    repo::soft_delete(&pool, "4000000001", 1)
        .await
        .expect("soft-delete");

    repo::insert(&pool, &sample("4000000001", 2, "Готівка", true))
        .await
        .expect("re-insert after soft-delete with same name");

    let active = repo::find_by_name(&pool, "4000000001", "Готівка")
        .await
        .unwrap()
        .expect("active row present");
    assert_eq!(active.pay_index, 2);
}

#[tokio::test]
async fn update_changes_name_and_iscash() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("4000000001", 2, "Картка", false))
        .await
        .unwrap();
    repo::update(&pool, "4000000001", 2, "Visa", false)
        .await
        .expect("update");

    let row = repo::find(&pool, "4000000001", 2)
        .await
        .unwrap()
        .expect("row present");
    assert_eq!(row.name, "Visa");
    assert!(!row.iscash);
}

#[tokio::test]
async fn update_on_missing_returns_not_found() {
    let (_dir, pool) = fresh_secure_pool().await;
    let err = repo::update(&pool, "4000000099", 1, "Visa", false)
        .await
        .expect_err("must surface NotFound");

    match err {
        PaymentMethodsRepoError::NotFound { fn_id, pay_index } => {
            assert_eq!(fn_id, "4000000099");
            assert_eq!(pay_index, 1);
        }
        other => panic!("expected NotFound, got: {other:?}"),
    }
}

#[tokio::test]
async fn list_active_for_fn_returns_only_active_rows_sorted() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("4000000001", 1, "Готівка", true))
        .await
        .unwrap();
    repo::insert(&pool, &sample("4000000001", 2, "Картка", false))
        .await
        .unwrap();
    repo::insert(&pool, &sample("4000000001", 3, "Кредит", false))
        .await
        .unwrap();
    repo::soft_delete(&pool, "4000000001", 2).await.unwrap();

    repo::insert(&pool, &sample("4000000099", 1, "Готівка", true))
        .await
        .unwrap();

    let rows = repo::list_active_for_fn(&pool, "4000000001")
        .await
        .expect("list");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].pay_index, 1);
    assert_eq!(rows[1].pay_index, 3);
}

#[tokio::test]
async fn soft_delete_missing_row_returns_not_found() {
    let (_dir, pool) = fresh_secure_pool().await;
    let err = repo::soft_delete(&pool, "4000000099", 1)
        .await
        .expect_err("must surface NotFound");

    assert!(matches!(
        err,
        PaymentMethodsRepoError::NotFound { fn_id, pay_index }
            if fn_id == "4000000099" && pay_index == 1
    ));
}
