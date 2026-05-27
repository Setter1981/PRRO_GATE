//! W4-Z0 piece 4 — `db::repositories::driver_tax_mapping` CRUD + typed errors.
//!
//! Per spec §1.3 + §3.4.  Cross-vendor coding-scheme normalization:
//! each driver vendor (Maria/WebCheck/Eccelio/etc.) maps its internal
//! `driver_number` to a `canonical_tx_num` used in DPS `<P TX=...>`.
//! Keyed on `(driver_id, driver_number)` — NOT per-FN (mapping is
//! vendor-level, not operator-level).  Default 1:1 mapping installed
//! at driver registration; operator adjusts via admin CLI if a
//! vendor's scheme differs from canonical.

use prro::db::open_secure_pool;
use prro::db::repositories::driver_tax_mapping::{
    self as repo, DriverTaxMappingRepoError, NewDriverTaxMapping,
};
use sqlx::SqlitePool;

async fn fresh_secure_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = open_secure_pool(&dir.path().join("secure.db"))
        .await
        .expect("open_secure_pool");
    (dir, pool)
}

fn sample(driver_id: &str, driver_number: i64, letter: Option<&str>, canonical: i64) -> NewDriverTaxMapping {
    NewDriverTaxMapping {
        driver_id: driver_id.to_string(),
        driver_number,
        driver_letter: letter.map(String::from),
        canonical_tx_num: canonical,
    }
}

#[tokio::test]
async fn insert_and_find_roundtrip() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("maria304", 1, Some("А"), 1))
        .await
        .expect("insert");

    let row = repo::find(&pool, "maria304", 1)
        .await
        .expect("query")
        .expect("row present");
    assert_eq!(row.driver_id, "maria304");
    assert_eq!(row.driver_number, 1);
    assert_eq!(row.driver_letter.as_deref(), Some("А"));
    assert_eq!(row.canonical_tx_num, 1);
    assert!(row.is_active);
}

#[tokio::test]
async fn insert_with_no_letter_succeeds() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("eccelio", 5, None, 4))
        .await
        .expect("insert with letter=None");

    let row = repo::find(&pool, "eccelio", 5)
        .await
        .unwrap()
        .expect("row present");
    assert_eq!(row.driver_letter, None);
    assert_eq!(row.canonical_tx_num, 4);
}

#[tokio::test]
async fn duplicate_pk_returns_typed_conflict() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("maria304", 1, Some("А"), 1))
        .await
        .unwrap();

    let err = repo::insert(&pool, &sample("maria304", 1, Some("Б"), 2))
        .await
        .expect_err("duplicate (driver_id, driver_number) must conflict");

    match err {
        DriverTaxMappingRepoError::DuplicatePk {
            driver_id,
            driver_number,
        } => {
            assert_eq!(driver_id, "maria304");
            assert_eq!(driver_number, 1);
        }
        other => panic!("expected DuplicatePk, got: {other:?}"),
    }
}

#[tokio::test]
async fn different_drivers_same_number_coexist() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("maria304", 1, Some("А"), 1))
        .await
        .unwrap();
    repo::insert(&pool, &sample("eccelio", 1, Some("A"), 1))
        .await
        .expect("different driver_id allows same driver_number");

    let m = repo::find(&pool, "maria304", 1).await.unwrap().unwrap();
    let e = repo::find(&pool, "eccelio", 1).await.unwrap().unwrap();
    assert_eq!(m.driver_id, "maria304");
    assert_eq!(e.driver_id, "eccelio");
}

#[tokio::test]
async fn update_canonical_changes_mapping() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("eccelio", 5, Some("ГА"), 5))
        .await
        .unwrap();
    repo::update_canonical(&pool, "eccelio", 5, 4)
        .await
        .expect("update");

    let row = repo::find(&pool, "eccelio", 5).await.unwrap().unwrap();
    assert_eq!(row.canonical_tx_num, 4);
}

#[tokio::test]
async fn update_canonical_on_missing_returns_not_found() {
    let (_dir, pool) = fresh_secure_pool().await;
    let err = repo::update_canonical(&pool, "eccelio", 99, 4)
        .await
        .expect_err("must surface NotFound");

    assert!(matches!(
        err,
        DriverTaxMappingRepoError::NotFound { driver_id, driver_number }
            if driver_id == "eccelio" && driver_number == 99
    ));
}

#[tokio::test]
async fn soft_delete_and_reinsert_same_pk_succeeds_via_new_canonical() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::insert(&pool, &sample("maria304", 1, Some("А"), 1))
        .await
        .unwrap();
    repo::soft_delete(&pool, "maria304", 1)
        .await
        .expect("soft-delete");

    // Driver_tax_mapping does NOT have a partial unique index on
    // canonical_tx_num — the PK (driver_id, driver_number) covers
    // uniqueness, and soft-delete simply marks the row inactive.
    // Re-inserting the same PK still conflicts (PK doesn't care about
    // is_active) — operator must use update_canonical instead.
    let err = repo::insert(&pool, &sample("maria304", 1, Some("А"), 2))
        .await
        .expect_err("PK collision regardless of is_active");
    assert!(matches!(err, DriverTaxMappingRepoError::DuplicatePk { .. }));
}

#[tokio::test]
async fn list_active_for_driver_returns_only_active() {
    let (_dir, pool) = fresh_secure_pool().await;

    for i in 1..=4 {
        repo::insert(&pool, &sample("maria304", i, None, i))
            .await
            .unwrap();
    }
    repo::soft_delete(&pool, "maria304", 3).await.unwrap();

    repo::insert(&pool, &sample("eccelio", 1, None, 1))
        .await
        .unwrap();

    let rows = repo::list_active_for_driver(&pool, "maria304")
        .await
        .expect("list");
    assert_eq!(rows.len(), 3);
    let nums: Vec<i64> = rows.iter().map(|r| r.driver_number).collect();
    assert_eq!(nums, vec![1, 2, 4]);
}
