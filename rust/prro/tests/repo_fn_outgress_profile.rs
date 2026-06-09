//! W4-Z0 piece 6 — `db::repositories::fn_outgress_profile`.
//!
//! Per spec §1.5 + §3.5.  Per-FN outgress protocol selection.
//! Pilot: every FN defaults to `FSCO_ZZD` at bootstrap; operator
//! switches to `EVPZ_DPS` post-pilot when W4-Y series ships.

use std::str::FromStr;

use prro::db::open_secure_pool;
use prro::db::repositories::fn_outgress_profile::{
    self as repo, FnOutgressProfileRepoError, OutgressProfile,
};
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

    repo::set_profile(&pool, "4000000001", OutgressProfile::FscoZzd)
        .await
        .expect("set");

    let profile = repo::get_profile(&pool, "4000000001")
        .await
        .expect("query")
        .expect("profile present");
    assert_eq!(profile, OutgressProfile::FscoZzd);
}

#[tokio::test]
async fn get_missing_returns_none() {
    let (_dir, pool) = fresh_secure_pool().await;
    let result = repo::get_profile(&pool, "4000000099").await.expect("query");
    assert!(result.is_none());
}

#[tokio::test]
async fn set_profile_is_upsert() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::set_profile(&pool, "4000000001", OutgressProfile::FscoZzd)
        .await
        .unwrap();
    repo::set_profile(&pool, "4000000001", OutgressProfile::EvpzDps)
        .await
        .expect("upsert");

    let profile = repo::get_profile(&pool, "4000000001")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(profile, OutgressProfile::EvpzDps);
}

#[tokio::test]
async fn invalid_profile_string_in_db_surfaces_as_parse_error() {
    let (_dir, _pool) = fresh_secure_pool().await;

    // Direct INSERT must be rejected by DB-side CHECK (verified in
    // migration_021 test); we cannot trigger a "parse error" via the
    // repo API directly.  This test pins the symmetric expectation:
    // `OutgressProfile::from_str` rejects unknown values.
    assert!(repo::OutgressProfile::from_str("UNKNOWN").is_err());
    assert!(repo::OutgressProfile::from_str("").is_err());
    assert_eq!(
        repo::OutgressProfile::from_str("FSCO_ZZD").unwrap(),
        OutgressProfile::FscoZzd
    );
    assert_eq!(
        repo::OutgressProfile::from_str("EVPZ_DPS").unwrap(),
        OutgressProfile::EvpzDps
    );
}

#[tokio::test]
async fn list_profiles_returns_all() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::set_profile(&pool, "4000000001", OutgressProfile::FscoZzd)
        .await
        .unwrap();
    repo::set_profile(&pool, "4000000002", OutgressProfile::EvpzDps)
        .await
        .unwrap();

    let all = repo::list_profiles(&pool).await.expect("list");
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn delete_profile_removes_row() {
    let (_dir, pool) = fresh_secure_pool().await;

    repo::set_profile(&pool, "4000000001", OutgressProfile::FscoZzd)
        .await
        .unwrap();
    repo::delete_profile(&pool, "4000000001")
        .await
        .expect("delete");

    let profile = repo::get_profile(&pool, "4000000001").await.unwrap();
    assert!(profile.is_none());
}

#[tokio::test]
async fn delete_missing_profile_returns_not_found() {
    let (_dir, pool) = fresh_secure_pool().await;
    let err = repo::delete_profile(&pool, "4000000099")
        .await
        .expect_err("must surface NotFound");

    assert!(matches!(
        err,
        FnOutgressProfileRepoError::NotFound { fn_id } if fn_id == "4000000099"
    ));
}
