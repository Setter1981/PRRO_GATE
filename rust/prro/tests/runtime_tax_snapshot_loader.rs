//! W4-Z2a piece 5 — `runtime/tax_snapshot::load_for_fn_driver` loader.
//!
//! Reads `tax_groups::list_active_for_fn` from secure pool, validates
//! rate round-trip to bps, returns TaxResolutionSnapshot ready for
//! `signing_config_snapshots::insert_or_get_id`.
//!
//! `driver_id` is the forensic key disambiguator (per locked design
//! audit-lineage requirement); the snapshot PAYLOAD currently captures
//! per-FN tax_groups only.  driver_tax_mapping is upstream/adapter
//! concern (item.tax_group_1 already translated to canonical by
//! conversion layer before stage_sign).

use prro::db::open_secure_pool;
use prro::db::repositories::tax_groups::{self, NewTaxGroup};
use prro::runtime::tax_snapshot::{self, LoadSnapshotError};

async fn fresh_secure_pool() -> sqlx::SqlitePool {
    let tmpdir = tempfile::tempdir().expect("tempdir");
    let path = tmpdir.path().join("secure.db");
    let pool = open_secure_pool(&path).await.expect("open_secure_pool");
    std::mem::forget(tmpdir);
    pool
}

async fn seed_two_tax_groups(pool: &sqlx::SqlitePool, fn_id: &str) {
    tax_groups::insert(
        pool,
        &NewTaxGroup {
            fn_id: fn_id.to_string(),
            tx_num: 1,
            letter: "А".to_string(),
            dtpr: 0.0,
            txpr: 20.0,
            txal: 0,
            txty: 0,
        },
    )
    .await
    .expect("insert tx=1");
    tax_groups::insert(
        pool,
        &NewTaxGroup {
            fn_id: fn_id.to_string(),
            tx_num: 2,
            letter: "В".to_string(),
            dtpr: 0.0,
            txpr: 7.0,
            txal: 0,
            txty: 0,
        },
    )
    .await
    .expect("insert tx=2");
}

#[tokio::test]
async fn empty_fn_yields_empty_snapshot_no_error() {
    // FN with no tax_groups configured → snapshot with empty groups.
    // Downstream: TaxMappingNotWired guard fires IF items reference
    // tax_group_1; absent-tax-group docs pass through unaffected.
    let pool = fresh_secure_pool().await;
    let snapshot = tax_snapshot::load_for_fn_driver(&pool, "1234567890", "maria304")
        .await
        .expect("empty config is valid");
    assert!(snapshot.groups.is_empty());
}

#[tokio::test]
async fn two_groups_round_trip_to_bps_correctly() {
    let pool = fresh_secure_pool().await;
    seed_two_tax_groups(&pool, "1234567890").await;
    let snapshot = tax_snapshot::load_for_fn_driver(&pool, "1234567890", "maria304")
        .await
        .expect("two-group load");
    assert_eq!(snapshot.groups.len(), 2);
    let g1 = snapshot.groups.iter().find(|g| g.tx == 1).expect("tx=1");
    assert_eq!(g1.txpr_bps, 2000); // 20.00% → 2000 bps
    assert_eq!(g1.dtpr_bps, 0);
    let g2 = snapshot.groups.iter().find(|g| g.tx == 2).expect("tx=2");
    assert_eq!(g2.txpr_bps, 700); // 7.00% → 700 bps
}

#[tokio::test]
async fn different_fn_distinct_snapshots() {
    let pool = fresh_secure_pool().await;
    seed_two_tax_groups(&pool, "FN-A").await;
    tax_groups::insert(
        &pool,
        &NewTaxGroup {
            fn_id: "FN-B".to_string(),
            tx_num: 1,
            letter: "А".to_string(),
            dtpr: 0.0,
            txpr: 14.0, // different rate
            txal: 0,
            txty: 0,
        },
    )
    .await
    .unwrap();

    let snap_a = tax_snapshot::load_for_fn_driver(&pool, "FN-A", "maria304")
        .await.unwrap();
    let snap_b = tax_snapshot::load_for_fn_driver(&pool, "FN-B", "maria304")
        .await.unwrap();
    assert_ne!(snap_a.sha256(), snap_b.sha256(),
        "distinct FNs → distinct snapshots");
}

#[tokio::test]
async fn snapshot_includes_only_active_groups() {
    let pool = fresh_secure_pool().await;
    seed_two_tax_groups(&pool, "1234567890").await;
    // Soft-delete tx=1
    tax_groups::soft_delete(&pool, "1234567890", 1).await.unwrap();

    let snap = tax_snapshot::load_for_fn_driver(&pool, "1234567890", "maria304")
        .await.unwrap();
    assert_eq!(snap.groups.len(), 1, "soft-deleted group excluded");
    assert_eq!(snap.groups[0].tx, 2);
}

#[tokio::test]
async fn invalid_rate_in_db_surfaces_typed_snapshot_build_error() {
    // Edge case: somehow a rate stored as e.g. 20.005 (cannot
    // round-trip to 2dp).  Loader propagates SnapshotBuild error.
    let pool = fresh_secure_pool().await;
    tax_groups::insert(
        &pool,
        &NewTaxGroup {
            fn_id: "1234567890".to_string(),
            tx_num: 1,
            letter: "А".to_string(),
            dtpr: 0.0,
            txpr: 20.005, // not round-trippable
            txal: 0,
            txty: 0,
        },
    )
    .await
    .unwrap();
    let err = tax_snapshot::load_for_fn_driver(&pool, "1234567890", "maria304")
        .await
        .expect_err("non-round-trip rate must surface");
    assert!(matches!(err, LoadSnapshotError::SnapshotBuild { .. }),
        "expected SnapshotBuild, got {err:?}");
}
