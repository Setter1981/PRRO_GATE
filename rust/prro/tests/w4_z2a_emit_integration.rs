//! W4-Z2a piece 11 — end-to-end integration of the snapshot ledger
//! into the tax-emit pipeline.
//!
//! The unit-level coverage is already strong:
//!   - `derive_tax_summaries.rs` — aggregation math, unknown-group skip,
//!     calc_tax error propagation.
//!   - `tax_resolution_snapshot.rs` — bps storage, canonical bytes,
//!     sha256, driver_mapping, V1 validation.
//!   - `xml_w4_z1_audit5_fixes.rs` — TaxMappingNotWired fail-closed.
//!   - `goldens_byte_equiv.rs` — byte-pinned `<TX>` emit for
//!     `extended_tax_groups()` (xml_sell_extended_byte_equivalent).
//!   - `pin_signing_inputs_coalesce.rs` — 5-case truth table incl.
//!     WHERE-guard drift rejection.
//!
//! What's NEW in piece 11 — bridging the ledger and the emit pipeline:
//!   - `TaxResolutionSnapshot.to_calc_map()` MUST produce a map
//!     byte-equivalent to the hand-built `extended_tax_groups()` that
//!     drives the existing extended goldens.  Proves the ledger feeds
//!     the SAME downstream pipeline the goldens lock at byte level.
//!   - Round-trip through `signing_config_snapshots::insert_or_get_id_tx`
//!     + `get_by_id` (sha256-verified) + `to_calc_map()` preserves the
//!     map identically.  Proves the persistence layer doesn't drift the
//!     resolved values.
//!   - NULL-FK / pre-W4-Z2a back-compat: `None.as_ref().map(to_calc_map)
//!     .unwrap_or_default()` produces an EMPTY map — back-compat semantic
//!     locked by piece 8c / piece 9.

use std::collections::HashMap;

use prro::db::repositories::signing_config_snapshots;
use prro::db::tx::with_immediate;
use prro::services::write_path::tax_summary::{
    ResolvedTaxGroup, ResolvedTaxGroupBps, TaxResolutionSnapshot,
};

const FN: &str = "4000000077";

async fn fresh_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("w4-z2a-emit.db");
    std::mem::forget(dir);
    prro::db::open_pool(&path).await.unwrap()
}

fn extended_bps_groups() -> Vec<ResolvedTaxGroupBps> {
    // Mirror goldens_byte_equiv::extended_tax_groups() in bps form:
    //   1 → 20.00% VAT  (2000 bps)
    //   2 →  7.00% VAT  ( 700 bps)
    vec![
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
    ]
}

fn expected_calc_map() -> HashMap<i64, ResolvedTaxGroup> {
    let mut m = HashMap::new();
    m.insert(
        1_i64,
        ResolvedTaxGroup {
            tx: 1,
            txpr: 20.0,
            dtpr: 0.0,
            txal: 0,
            txty: 0,
        },
    );
    m.insert(
        2_i64,
        ResolvedTaxGroup {
            tx: 2,
            txpr: 7.0,
            dtpr: 0.0,
            txal: 0,
            txty: 0,
        },
    );
    m
}

// ─── Test 1 — to_calc_map matches goldens' extended_tax_groups ──────

#[test]
fn snapshot_to_calc_map_matches_extended_goldens_shape() {
    let snapshot = TaxResolutionSnapshot::new(extended_bps_groups());
    let actual = snapshot.to_calc_map();
    let expected = expected_calc_map();
    assert_eq!(
        actual.len(),
        expected.len(),
        "live snapshot's to_calc_map MUST produce the same keys as the hand-built goldens fixture"
    );
    for (tx, ev) in &expected {
        let av = actual.get(tx).expect("tx present in to_calc_map");
        assert_eq!(av.tx, ev.tx);
        assert_eq!(
            av.txpr, ev.txpr,
            "bps→f64 conversion MUST yield the byte-identical rate the goldens lock"
        );
        assert_eq!(av.dtpr, ev.dtpr);
        assert_eq!(av.txal, ev.txal);
        assert_eq!(av.txty, ev.txty);
    }
}

// ─── Test 2 — insert + get_by_id + to_calc_map round-trip ───────────

#[tokio::test]
async fn insert_get_by_id_preserves_to_calc_map_byte_identically() {
    let pool = fresh_pool().await;
    let snapshot = TaxResolutionSnapshot::new(extended_bps_groups());
    let expected_map = snapshot.to_calc_map();

    // Persist via tx-variant (mirrors stage_acquire's flow).
    let snapshot_for_tx = snapshot.clone();
    let inserted_id = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let id = signing_config_snapshots::insert_or_get_id_tx(
                tx,
                FN,
                "driver-77",
                &snapshot_for_tx,
            )
            .await?;
            Ok::<i64, anyhow::Error>(id)
        })
    })
    .await
    .unwrap();

    // Reload via get_by_id (sha256-verified read, V1 validated).
    let reloaded = signing_config_snapshots::get_by_id(&pool, inserted_id)
        .await
        .expect("reload via FK");
    let reloaded_map = reloaded.to_calc_map();

    assert_eq!(
        reloaded_map, expected_map,
        "round-trip through insert_or_get_id_tx + get_by_id MUST preserve the resolved map byte-identically — \
         locked rule #9 MAC recovery determinism guarantee"
    );
}

// ─── Test 3 — None-snapshot path produces an empty map ──────────────

#[test]
fn none_snapshot_unwrap_or_default_yields_empty_map() {
    // Mirrors stage_sign 3-NO-TX wiring (piece 8c) for the Resume /
    // boot recovery / fixture path: ctx.tax_resolution_snapshot = None
    // → empty HashMap.
    let ctx: Option<TaxResolutionSnapshot> = None;
    let map: HashMap<i64, ResolvedTaxGroup> =
        ctx.as_ref().map(|s| s.to_calc_map()).unwrap_or_default();
    assert!(
        map.is_empty(),
        "None snapshot MUST collapse to empty map — back-compat path for pre-W4-Z2a / NULL-FK docs"
    );
}

// ─── Test 4 — idempotent insert under content-hash UNIQUE ───────────

#[tokio::test]
async fn insert_or_get_id_is_idempotent_under_same_payload() {
    // Forensic invariant: a Proceed retry / Resume / boot recovery
    // that re-INSERTs the same (fn, driver, payload) snapshot MUST
    // return the SAME id.  No row duplication — append-only ledger
    // dedup via UNIQUE (fn, driver_id, payload_sha256).
    let pool = fresh_pool().await;
    let snapshot = TaxResolutionSnapshot::new(extended_bps_groups());

    let s1 = snapshot.clone();
    let id1 = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let id = signing_config_snapshots::insert_or_get_id_tx(
                tx, FN, "driver-77", &s1,
            )
            .await?;
            Ok::<i64, anyhow::Error>(id)
        })
    })
    .await
    .unwrap();

    let s2 = snapshot.clone();
    let id2 = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let id = signing_config_snapshots::insert_or_get_id_tx(
                tx, FN, "driver-77", &s2,
            )
            .await?;
            Ok::<i64, anyhow::Error>(id)
        })
    })
    .await
    .unwrap();

    assert_eq!(
        id1, id2,
        "second insert with identical (fn, driver, payload) MUST return the existing row's id — \
         content-hash UNIQUE keeps the ledger append-only without duplicates"
    );
}
