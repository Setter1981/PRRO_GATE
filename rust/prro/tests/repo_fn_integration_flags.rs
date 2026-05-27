//! W4-Z0 piece 5 — `db::repositories::fn_integration_flags`.
//!
//! Per spec §1.4 + §3.3.  Per-FN key-value flag store:
//! `useecheckmegovua` (Національний чек) + future flags.  Upsert
//! semantics: setting an existing flag overwrites + bumps updated_at;
//! setting a missing flag inserts a new row.

use prro::db::open_secure_pool;
use prro::db::repositories::fn_integration_flags::{self as repo, FnIntegrationFlagsRepoError};
use sqlx::SqlitePool;

async fn fresh_secure_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = open_secure_pool(&dir.path().join("secure.db"))
        .await
        .expect("open_secure_pool");
    (dir, pool)
}

#[tokio::test]
async fn set_and_get_roundtrip() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::set_flag(&pool, "4000000001", "useecheckmegovua", "1")
        .await
        .expect("set");

    let value = repo::get_flag(&pool, "4000000001", "useecheckmegovua")
        .await
        .expect("query")
        .expect("flag present");
    assert_eq!(value, "1");
}

#[tokio::test]
async fn get_missing_returns_none() {
    let (_dir, pool) = fresh_secure_pool().await;
    let value = repo::get_flag(&pool, "4000000099", "useecheckmegovua")
        .await
        .expect("query");
    assert!(value.is_none());
}

#[tokio::test]
async fn set_flag_is_upsert() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::set_flag(&pool, "4000000001", "useecheckmegovua", "0")
        .await
        .unwrap();
    repo::set_flag(&pool, "4000000001", "useecheckmegovua", "1")
        .await
        .expect("upsert (overwrite)");

    let value = repo::get_flag(&pool, "4000000001", "useecheckmegovua")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(value, "1");
}

#[tokio::test]
async fn delete_flag_removes_row() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::set_flag(&pool, "4000000001", "useecheckmegovua", "1")
        .await
        .unwrap();
    repo::delete_flag(&pool, "4000000001", "useecheckmegovua")
        .await
        .expect("delete");

    let value = repo::get_flag(&pool, "4000000001", "useecheckmegovua")
        .await
        .unwrap();
    assert!(value.is_none(), "flag must be gone after delete");
}

#[tokio::test]
async fn delete_missing_flag_returns_not_found() {
    let (_dir, pool) = fresh_secure_pool().await;
    let err = repo::delete_flag(&pool, "4000000099", "useecheckmegovua")
        .await
        .expect_err("must surface NotFound");

    assert!(matches!(
        err,
        FnIntegrationFlagsRepoError::NotFound { fn_id, flag_name }
            if fn_id == "4000000099" && flag_name == "useecheckmegovua"
    ));
}

#[tokio::test]
async fn list_flags_for_fn_returns_all_set_flags() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::set_flag(&pool, "4000000001", "useecheckmegovua", "1")
        .await
        .unwrap();
    repo::set_flag(&pool, "4000000001", "some_future_flag", "value")
        .await
        .unwrap();

    repo::set_flag(&pool, "4000000099", "useecheckmegovua", "0")
        .await
        .unwrap();

    let flags = repo::list_flags_for_fn(&pool, "4000000001")
        .await
        .expect("list");
    assert_eq!(flags.len(), 2);
    let names: Vec<String> = flags.iter().map(|f| f.flag_name.clone()).collect();
    assert!(names.contains(&"useecheckmegovua".to_string()));
    assert!(names.contains(&"some_future_flag".to_string()));
}
