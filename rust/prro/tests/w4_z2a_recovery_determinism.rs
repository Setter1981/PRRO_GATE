//! W4-Z2a piece 12 — config-change-after-pin determinism.
//!
//! Locked rule #3 (append-only) + rule #9 (MAC recovery uses
//! persisted snapshot, NEVER current config) together guarantee
//! byte-identical re-sign for a doc whose live tax config was
//! mutated AFTER its snapshot was pinned.
//!
//! These tests prove the architectural invariants at the ledger
//! layer (the only layer where "config change" could conceivably
//! leak into a re-sign):
//!
//! T1. Append-only — admin "config update" between two stage_acquire
//!     runs creates a NEW snapshot row.  The OLD row stays untouched.
//!     `get_by_id(old_id)` still returns the old data.  Proven via
//!     sha256 verify-on-read.
//!
//! T2. Byte-identity at the resolved-map layer — `to_calc_map()` of
//!     the OLD snapshot is byte-equivalent to itself across the
//!     mutation event.  No mutable state in the snapshot pipeline.
//!
//! T3. MAC recovery FK reload semantic — `get_by_id` keyed on the
//!     persisted `signing_config_snapshot_id` returns the OLD
//!     snapshot regardless of how many newer snapshots exist for
//!     the same (fn, driver).  Proves
//!     `mac_recovery::run_mac_recovery` path's reload (piece 9) is
//!     deterministic across admin mutations.
//!
//! Together T1-T3 are the structural proof of "config mutated
//! AFTER pin → byte-identical re-sign via persisted FK".  The
//! actual byte-equivalent unsigned_xml comparison is locked by the
//! existing `goldens_byte_equiv::xml_sell_extended_byte_equivalent`
//! test — these tests prove the bridge between the persisted FK
//! and that golden bytes-path is drift-free.

use prro::db::repositories::signing_config_snapshots;
use prro::db::tx::with_immediate;
use prro::services::write_path::tax_summary::{ResolvedTaxGroupBps, TaxResolutionSnapshot};

const FN: &str = "4000000088";

async fn fresh_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("w4-z2a-recovery.db");
    std::mem::forget(dir);
    prro::db::open_pool(&path).await.unwrap()
}

fn snapshot_at_20_percent() -> TaxResolutionSnapshot {
    TaxResolutionSnapshot::new(vec![ResolvedTaxGroupBps {
        tx: 1,
        txpr_bps: 2000, // 20.00%
        dtpr_bps: 0,
        txal: 0,
        txty: 0,
    }])
}

fn snapshot_at_19_percent() -> TaxResolutionSnapshot {
    // Admin-mutated rate (Ukrainian VAT reform scenario simulated
    // for the deterministic test).  Same fn + driver, different
    // payload → different content hash → different row id.
    TaxResolutionSnapshot::new(vec![ResolvedTaxGroupBps {
        tx: 1,
        txpr_bps: 1900, // 19.00%
        dtpr_bps: 0,
        txal: 0,
        txty: 0,
    }])
}

async fn insert(pool: &sqlx::SqlitePool, snapshot: TaxResolutionSnapshot) -> i64 {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            let id = signing_config_snapshots::insert_or_get_id_tx(tx, FN, "driver-88", &snapshot)
                .await?;
            Ok::<i64, anyhow::Error>(id)
        })
    })
    .await
    .unwrap()
}

// ─── T1 — append-only under admin mutation ──────────────────────────

#[tokio::test]
async fn admin_config_change_creates_new_row_old_row_unchanged() {
    let pool = fresh_pool().await;

    let old_id = insert(&pool, snapshot_at_20_percent()).await;
    // Admin "changes config" — second insert lands as a new row
    // because content hash differs.
    let new_id = insert(&pool, snapshot_at_19_percent()).await;

    assert_ne!(
        old_id, new_id,
        "different payload MUST yield different row id under content-hash UNIQUE"
    );

    // The OLD row stays untouched — sha256 verify-on-read confirms.
    let reloaded_old = signing_config_snapshots::get_by_id(&pool, old_id)
        .await
        .expect("old row still resolvable post-mutation");
    let groups = reloaded_old.groups();
    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0].txpr_bps, 2000,
        "OLD snapshot's rate MUST stay at 20.00% — append-only ledger forbids in-place edit"
    );

    // The NEW row carries the new rate.
    let reloaded_new = signing_config_snapshots::get_by_id(&pool, new_id)
        .await
        .expect("new row resolvable");
    assert_eq!(reloaded_new.groups()[0].txpr_bps, 1900);
}

// ─── T2 — byte-identity at resolved-map layer ──────────────────────

#[tokio::test]
async fn to_calc_map_byte_identical_across_admin_mutation() {
    let pool = fresh_pool().await;

    let old_id = insert(&pool, snapshot_at_20_percent()).await;
    let map_pre_mutation = signing_config_snapshots::get_by_id(&pool, old_id)
        .await
        .unwrap()
        .to_calc_map();

    // Admin mutates: new snapshot inserted alongside (same fn+driver).
    let _new_id = insert(&pool, snapshot_at_19_percent()).await;

    // Reload OLD snapshot post-mutation → resolved map MUST be
    // byte-identical (no mutable state in the ledger / pipeline).
    let map_post_mutation = signing_config_snapshots::get_by_id(&pool, old_id)
        .await
        .unwrap()
        .to_calc_map();

    assert_eq!(
        map_pre_mutation, map_post_mutation,
        "OLD snapshot's to_calc_map MUST yield byte-identical resolved map across an admin-mutation event — \
         byte-identical re-sign guarantee for the MAC recovery + Resume paths"
    );
}

// ─── T3 — MAC recovery reload semantic ─────────────────────────────

#[tokio::test]
async fn mac_recovery_reload_returns_pinned_snapshot_not_latest() {
    let pool = fresh_pool().await;

    // Pin doc to OLD snapshot.
    let pinned_id = insert(&pool, snapshot_at_20_percent()).await;
    // Several admin mutations create newer rows for same fn+driver.
    let _newer_id_1 = insert(&pool, snapshot_at_19_percent()).await;
    let _newer_id_2 = insert(
        &pool,
        TaxResolutionSnapshot::new(vec![ResolvedTaxGroupBps {
            tx: 1,
            txpr_bps: 700,
            dtpr_bps: 0,
            txal: 0,
            txty: 0,
        }]),
    )
    .await;

    // MAC recovery semantic: orchestrator reads `doc.signing_config_
    // snapshot_id` → `get_by_id(pinned_id)` → resolves to OLD data
    // regardless of how many newer rows exist for the same (fn,
    // driver).  Locked rule #9 enforced at this layer.
    let reloaded = signing_config_snapshots::get_by_id(&pool, pinned_id)
        .await
        .expect("pinned id still resolves");

    assert_eq!(
        reloaded.groups()[0].txpr_bps,
        2000,
        "MAC recovery reload via pinned FK MUST return OLD snapshot's data — NEVER latest config"
    );
}
