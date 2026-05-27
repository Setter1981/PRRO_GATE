//! W4-Z0 piece 2 — `db::repositories::tax_groups` CRUD + typed errors.
//!
//! Per `docs/superpowers/specs/2026-05-26-w4-z0-config-storage-spec.md`
//! §1.1 + §3.1.  The repository must:
//!
//!   * Insert new tax-group rows with full attribute set
//!   * Reject duplicate active letter per FN via typed
//!     `DuplicateActiveLetter` (partial unique idx
//!     `idx_tax_groups_fn_letter` WHERE is_active=1)
//!   * Reject duplicate tx_num per FN via typed `DuplicateTxNum`
//!     (PRIMARY KEY (fn, tx_num))
//!   * Allow soft-delete + re-insert with the same letter
//!   * Update rates (dtpr/txpr/txal/txty) on existing row
//!   * Find by (fn, tx_num) and (fn, letter active-only)
//!   * List all active rows per FN

use prro::db::open_secure_pool;
use prro::db::repositories::tax_groups::{self as repo, NewTaxGroup, TaxGroupsRepoError};
use sqlx::SqlitePool;

async fn fresh_secure_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = open_secure_pool(&dir.path().join("secure.db"))
        .await
        .expect("open_secure_pool");
    (dir, pool)
}

fn sample(fn_id: &str, tx_num: i64, letter: &str) -> NewTaxGroup {
    NewTaxGroup {
        fn_id: fn_id.to_string(),
        tx_num,
        letter: letter.to_string(),
        dtpr: 0.0,
        txpr: 20.0,
        txal: 0,
        txty: 0,
    }
}

#[tokio::test]
async fn insert_and_find_by_tx_num_roundtrip() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("4000000001", 1, "А"))
        .await
        .expect("insert");

    let row = repo::find(&pool, "4000000001", 1)
        .await
        .expect("find query")
        .expect("row present");
    assert_eq!(row.fn_id, "4000000001");
    assert_eq!(row.tx_num, 1);
    assert_eq!(row.letter, "А");
    assert_eq!(row.dtpr, 0.0);
    assert_eq!(row.txpr, 20.0);
    assert_eq!(row.txal, 0);
    assert_eq!(row.txty, 0);
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
async fn find_by_letter_returns_active_row() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("4000000001", 1, "А"))
        .await
        .expect("insert А");
    repo::insert(&pool, &sample("4000000001", 4, "ГА"))
        .await
        .expect("insert ГА");

    let row_a = repo::find_by_letter(&pool, "4000000001", "А")
        .await
        .expect("query А")
        .expect("active row present");
    assert_eq!(row_a.tx_num, 1);

    let row_ga = repo::find_by_letter(&pool, "4000000001", "ГА")
        .await
        .expect("query ГА")
        .expect("active row present");
    assert_eq!(row_ga.tx_num, 4);
}

#[tokio::test]
async fn duplicate_tx_num_per_fn_returns_typed_conflict() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("4000000001", 1, "А"))
        .await
        .expect("first insert");

    let err = repo::insert(&pool, &sample("4000000001", 1, "Б"))
        .await
        .expect_err("second insert with same tx_num must conflict");

    match err {
        TaxGroupsRepoError::DuplicateTxNum { fn_id, tx_num } => {
            assert_eq!(fn_id, "4000000001");
            assert_eq!(tx_num, 1);
        }
        other => panic!("expected DuplicateTxNum, got: {other:?}"),
    }
}

#[tokio::test]
async fn duplicate_active_letter_per_fn_returns_typed_conflict() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("4000000001", 1, "А"))
        .await
        .expect("first insert (tx_num=1, letter=А)");

    // Different tx_num but same letter — caught by partial unique idx
    // on (fn, letter) WHERE is_active=1.
    let err = repo::insert(&pool, &sample("4000000001", 2, "А"))
        .await
        .expect_err("second active row with same letter must conflict");

    match err {
        TaxGroupsRepoError::DuplicateActiveLetter { fn_id, letter } => {
            assert_eq!(fn_id, "4000000001");
            assert_eq!(letter, "А");
        }
        other => panic!("expected DuplicateActiveLetter, got: {other:?}"),
    }
}

#[tokio::test]
async fn soft_delete_then_reinsert_same_letter_succeeds() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("4000000001", 1, "А"))
        .await
        .expect("first insert");
    repo::soft_delete(&pool, "4000000001", 1)
        .await
        .expect("soft-delete");

    // After soft-delete the partial unique index no longer covers
    // (fn=4000000001, letter=А), so re-inserting with a fresh
    // tx_num must succeed.
    repo::insert(&pool, &sample("4000000001", 2, "А"))
        .await
        .expect("re-insert with same letter after soft-delete");

    // Active row is the new one (tx_num=2)
    let active = repo::find_by_letter(&pool, "4000000001", "А")
        .await
        .expect("query")
        .expect("active row present");
    assert_eq!(active.tx_num, 2);

    // Old soft-deleted row still findable by (fn, tx_num=1)
    let old = repo::find(&pool, "4000000001", 1)
        .await
        .expect("query")
        .expect("soft-deleted row still present");
    assert!(!old.is_active, "old row must be is_active=false");
}

#[tokio::test]
async fn update_rates_changes_dtpr_txpr_txal_txty() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("4000000001", 4, "ГА"))
        .await
        .expect("insert ГА (default 0/20/0/0)");

    repo::update_rates(&pool, "4000000001", 4, 5.0, 20.0, 2, 0)
        .await
        .expect("update rates to excise-bearing");

    let row = repo::find(&pool, "4000000001", 4)
        .await
        .expect("query")
        .expect("row present");
    assert_eq!(row.dtpr, 5.0);
    assert_eq!(row.txpr, 20.0);
    assert_eq!(row.txal, 2);
    assert_eq!(row.txty, 0);
}

#[tokio::test]
async fn update_rates_on_missing_row_returns_not_found() {
    let (_dir, pool) = fresh_secure_pool().await;
    let err = repo::update_rates(&pool, "4000000099", 1, 5.0, 20.0, 2, 0)
        .await
        .expect_err("must surface NotFound");

    match err {
        TaxGroupsRepoError::NotFound { fn_id, tx_num } => {
            assert_eq!(fn_id, "4000000099");
            assert_eq!(tx_num, 1);
        }
        other => panic!("expected NotFound, got: {other:?}"),
    }
}

#[tokio::test]
async fn list_active_for_fn_returns_only_active_rows() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("4000000001", 1, "А")).await.unwrap();
    repo::insert(&pool, &sample("4000000001", 2, "Б")).await.unwrap();
    repo::insert(&pool, &sample("4000000001", 3, "В")).await.unwrap();
    repo::soft_delete(&pool, "4000000001", 2).await.unwrap();

    // Other FN noise — must not appear
    repo::insert(&pool, &sample("4000000099", 1, "А")).await.unwrap();

    let rows = repo::list_active_for_fn(&pool, "4000000001")
        .await
        .expect("list query");
    assert_eq!(rows.len(), 2, "soft-deleted row excluded, other FN excluded");
    let tx_nums: Vec<i64> = rows.iter().map(|r| r.tx_num).collect();
    assert!(tx_nums.contains(&1));
    assert!(tx_nums.contains(&3));
    assert!(!tx_nums.contains(&2));
}

#[tokio::test]
async fn soft_delete_missing_row_returns_not_found() {
    let (_dir, pool) = fresh_secure_pool().await;
    let err = repo::soft_delete(&pool, "4000000099", 1)
        .await
        .expect_err("must surface NotFound");

    match err {
        TaxGroupsRepoError::NotFound { fn_id, tx_num } => {
            assert_eq!(fn_id, "4000000099");
            assert_eq!(tx_num, 1);
        }
        other => panic!("expected NotFound, got: {other:?}"),
    }
}
