//! W4-Z2a piece 3 — signing_config_snapshots repository.
//!
//! Per locked design: `insert_or_get_id` race-safe via UNIQUE
//! constraint + INSERT OR IGNORE; `get_by_id` with SHA256
//! verify-on-read (typed error on mismatch).
//!
//! These tests use in-memory main pool (no secure pool needed —
//! snapshot table lives in main per locked design).

use prro::db::open_pool;
use prro::db::repositories::signing_config_snapshots::{self, SigningConfigSnapshotsRepoError};
use prro::services::write_path::tax_summary::{ResolvedTaxGroupBps, TaxResolutionSnapshot};

async fn fresh_pool() -> sqlx::SqlitePool {
    let tmpdir = tempfile::tempdir().expect("tempdir");
    let path = tmpdir.path().join("main.db");
    let pool = open_pool(&path).await.expect("open_pool runs migrations");
    // Leak tempdir intentionally: pool keeps file open; tempdir's
    // Drop would race with sqlx's connection finalization in fast
    // test cycles.  Real fixture util in tests/common/ handles this,
    // but for unit-scope smoke we accept the leak.
    std::mem::forget(tmpdir);
    pool
}

fn fixture_snapshot() -> TaxResolutionSnapshot {
    TaxResolutionSnapshot::new(vec![
        ResolvedTaxGroupBps {
            tx: 1,
            txpr_bps: 2000,
            dtpr_bps: 0,
            txal: 0,
            txty: 0,
        },
        ResolvedTaxGroupBps {
            tx: 2,
            txpr_bps: 700,
            dtpr_bps: 0,
            txal: 0,
            txty: 0,
        },
    ])
}

// ─── insert_or_get_id: round-trip ─────────────────────────────────

#[tokio::test]
async fn insert_or_get_id_inserts_new_snapshot() {
    let pool = fresh_pool().await;
    let snapshot = fixture_snapshot();
    let id = signing_config_snapshots::insert_or_get_id(&pool, "1234567890", "maria304", &snapshot)
        .await
        .expect("insert ok");
    assert!(id > 0, "valid autoincrement id, got {id}");
}

#[tokio::test]
async fn insert_or_get_id_idempotent_for_same_content() {
    // Race-safe + dedupe contract: two callers with identical
    // (fn, driver_id, snapshot) get the SAME id.  No second row.
    let pool = fresh_pool().await;
    let snapshot = fixture_snapshot();
    let id_a =
        signing_config_snapshots::insert_or_get_id(&pool, "1234567890", "maria304", &snapshot)
            .await
            .unwrap();
    let id_b =
        signing_config_snapshots::insert_or_get_id(&pool, "1234567890", "maria304", &snapshot)
            .await
            .unwrap();
    assert_eq!(id_a, id_b, "same content → same id");

    // Confirm only one row in table.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM signing_config_snapshots")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn insert_or_get_id_distinct_for_different_fn() {
    let pool = fresh_pool().await;
    let snapshot = fixture_snapshot();
    let id_a = signing_config_snapshots::insert_or_get_id(&pool, "FN-A", "maria304", &snapshot)
        .await
        .unwrap();
    let id_b = signing_config_snapshots::insert_or_get_id(&pool, "FN-B", "maria304", &snapshot)
        .await
        .unwrap();
    assert_ne!(id_a, id_b);
}

#[tokio::test]
async fn insert_or_get_id_distinct_for_different_driver() {
    let pool = fresh_pool().await;
    let snapshot = fixture_snapshot();
    let id_a = signing_config_snapshots::insert_or_get_id(&pool, "FN-A", "maria304", &snapshot)
        .await
        .unwrap();
    let id_b = signing_config_snapshots::insert_or_get_id(&pool, "FN-A", "webcheck", &snapshot)
        .await
        .unwrap();
    assert_ne!(id_a, id_b, "different driver_id → different snapshot id");
}

#[tokio::test]
async fn insert_or_get_id_distinct_for_different_payload() {
    let pool = fresh_pool().await;
    let snap_v1 = TaxResolutionSnapshot::new(vec![ResolvedTaxGroupBps {
        tx: 1,
        txpr_bps: 2000,
        dtpr_bps: 0,
        txal: 0,
        txty: 0,
    }]);
    let snap_v2 = TaxResolutionSnapshot::new(vec![
        ResolvedTaxGroupBps {
            tx: 1,
            txpr_bps: 1800,
            dtpr_bps: 0,
            txal: 0,
            txty: 0,
        }, // 20%→18%
    ]);
    let id_a = signing_config_snapshots::insert_or_get_id(&pool, "FN-A", "maria304", &snap_v1)
        .await
        .unwrap();
    let id_b = signing_config_snapshots::insert_or_get_id(&pool, "FN-A", "maria304", &snap_v2)
        .await
        .unwrap();
    assert_ne!(id_a, id_b, "config change → new snapshot row");
}

// ─── get_by_id: round-trip + SHA256 verify ────────────────────────

#[tokio::test]
async fn get_by_id_round_trips_snapshot_bytes() {
    let pool = fresh_pool().await;
    let snapshot = fixture_snapshot();
    let id = signing_config_snapshots::insert_or_get_id(&pool, "1234567890", "maria304", &snapshot)
        .await
        .unwrap();
    let loaded = signing_config_snapshots::get_by_id(&pool, id)
        .await
        .unwrap();
    assert_eq!(loaded.kind(), snapshot.kind());
    assert_eq!(loaded.groups(), snapshot.groups());
    assert_eq!(loaded.sha256(), snapshot.sha256());
}

#[tokio::test]
async fn get_by_id_not_found_returns_typed_error() {
    let pool = fresh_pool().await;
    let err = signing_config_snapshots::get_by_id(&pool, 999_999_i64)
        .await
        .expect_err("missing id");
    assert!(matches!(
        err,
        SigningConfigSnapshotsRepoError::NotFound { id: 999_999 }
    ));
}

#[tokio::test]
async fn get_by_id_sha256_mismatch_returns_typed_error() {
    // Defensive integrity check: if someone (or disk corruption)
    // mutates payload_json without updating payload_sha256,
    // get_by_id must fail-closed.  Test simulates by direct UPDATE.
    let pool = fresh_pool().await;
    let snapshot = fixture_snapshot();
    let id = signing_config_snapshots::insert_or_get_id(&pool, "1234567890", "maria304", &snapshot)
        .await
        .unwrap();
    // Corrupt payload (mutate without updating hash).
    sqlx::query("UPDATE signing_config_snapshots SET payload_json = ? WHERE id = ?")
        .bind(r#"{"driver_mapping":[],"groups":[{"dtpr_bps":0,"tx":999,"txal":0,"txpr_bps":1234,"txty":0}],"kind":"check_tax_mapping_v1"}"#)
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();

    let err = signing_config_snapshots::get_by_id(&pool, id)
        .await
        .expect_err("hash mismatch detected");
    assert!(
        matches!(
            err,
            SigningConfigSnapshotsRepoError::ChecksumMismatch { .. }
        ),
        "expected ChecksumMismatch, got {err:?}"
    );
}

// ─── insert validation: kind format ───────────────────────────────

// ─── insert_or_get_id_tx: direct test (mid-review IMP-3) ──────────

#[tokio::test]
async fn insert_or_get_id_tx_inserts_inside_with_immediate_envelope() {
    use prro::db::tx::with_immediate;
    let pool = fresh_pool().await;
    let snapshot = fixture_snapshot();
    let id = with_immediate(&pool, |tx| {
        let snapshot = snapshot.clone();
        Box::pin(async move {
            signing_config_snapshots::insert_or_get_id_tx(tx, "1234567890", "maria304", &snapshot)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("with_immediate succeeds");
    assert!(id > 0);
    // Verify row landed.
    let loaded = signing_config_snapshots::get_by_id(&pool, id)
        .await
        .unwrap();
    assert_eq!(loaded.groups(), snapshot.groups());
}

#[tokio::test]
async fn insert_or_get_id_tx_idempotent_across_two_envelopes() {
    use prro::db::tx::with_immediate;
    let pool = fresh_pool().await;
    let snapshot = fixture_snapshot();
    let id_a = with_immediate(&pool, |tx| {
        let snapshot = snapshot.clone();
        Box::pin(async move {
            signing_config_snapshots::insert_or_get_id_tx(tx, "FN-A", "maria304", &snapshot)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .unwrap();
    let id_b = with_immediate(&pool, |tx| {
        let snapshot = snapshot.clone();
        Box::pin(async move {
            signing_config_snapshots::insert_or_get_id_tx(tx, "FN-A", "maria304", &snapshot)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .unwrap();
    assert_eq!(id_a, id_b, "tx-variant idempotent across envelopes");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM signing_config_snapshots")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn insert_or_get_id_persists_kind_field() {
    // kind future-proof — must round-trip.
    let pool = fresh_pool().await;
    let snapshot = fixture_snapshot();
    let id = signing_config_snapshots::insert_or_get_id(&pool, "FN-A", "maria304", &snapshot)
        .await
        .unwrap();
    let loaded = signing_config_snapshots::get_by_id(&pool, id)
        .await
        .unwrap();
    assert_eq!(loaded.kind(), TaxResolutionSnapshot::KIND_V1);
}
