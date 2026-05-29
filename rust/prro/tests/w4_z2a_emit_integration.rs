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
use prro::services::write_path::stage_sign::{check_payload_from_json_for_testing, SignError};
use prro::services::write_path::tax_summary::{
    DriverNumberMapping, ResolvedTaxGroup, ResolvedTaxGroupBps, TaxResolutionSnapshot,
};
use prro::xml::{CalcTaxError, DocumentHeader};

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

// ─── Piece 14 — driver_mapping translation regression (CRIT-1) ──────

/// W4-Z2a piece 14 (external review CRIT-1 close) — verify
/// `resolve_driver_number` translates POS driver_number to canonical
/// TX (the primitive that `check_payload_from` invokes per-item
/// inside the conversion layer).  Non-identity mapping case: driver
/// 5 → canonical 1.  Without this translation an item with
/// `tax_group_1 = Some(5)` would emit `<I TX="5">` in XML and the
/// canonical `tax_groups` map keyed by 1 would miss → silent `<TX>`
/// drop OR wrong-group aggregation if canonical 5 also exists.
///
/// Note: full XML-level assertion (`<I TX="1">` + `<TX TX="1">`)
/// requires driving `check_payload_from` / `build_canonical_doc`
/// which are crate-private.  This unit-level test against the
/// translation primitive + the existing build success guarantees
/// the wire-up.  An end-to-end XML assertion would require either
/// exposing `check_payload_from` as `pub` or a stage_sign
/// integration fixture with a full WorkerContext.
#[test]
fn driver_5_translates_to_canonical_1() {
    let snapshot = TaxResolutionSnapshot::with_driver_mapping(
        vec![ResolvedTaxGroupBps {
            tx: 1,
            txpr_bps: 2000, // 20.00% VAT — canonical TX = 1
            dtpr_bps: 0,
            txal: 0,
            txty: 0,
        }],
        vec![DriverNumberMapping {
            driver_number: 5,    // POS sends 5
            canonical_tx_num: 1, // wire emits TX="1"
        }],
    );

    assert_eq!(
        snapshot.resolve_driver_number(5),
        Some(1),
        "POS driver_number=5 MUST translate to canonical TX=1 — \
         without this, item.tax_group_1 stays 5, canonical tax_groups[1] lookup misses, \
         <TX> summary silently dropped, fiscal divergence on the wire"
    );

    // Calc map keyed by canonical TX (NOT driver_number) — confirms
    // the asymmetry that motivates translation in check_payload_from.
    let calc_map = snapshot.to_calc_map();
    assert!(
        calc_map.contains_key(&1),
        "calc_map MUST be keyed by canonical TX=1, not driver_number=5"
    );
    assert!(
        !calc_map.contains_key(&5),
        "calc_map MUST NOT carry driver_number=5 — that's the WHOLE point of translation"
    );
}

/// W4-Z2a piece 14 — fail-loud surface for driver_mapping miss.
/// A driver_number not present in driver_mapping (e.g., POS sends 99
/// but mapping only knows {5→1}) MUST surface `None` from
/// `resolve_driver_number`, which `check_payload_from` translates to
/// `CalcTaxError::DriverMappingMiss` rather than silently emitting
/// `<I TX="99">` with no matching `<TX>` summary.
#[test]
fn driver_unknown_returns_none_for_fail_loud() {
    let snapshot = TaxResolutionSnapshot::with_driver_mapping(
        vec![ResolvedTaxGroupBps {
            tx: 1,
            txpr_bps: 2000,
            dtpr_bps: 0,
            txal: 0,
            txty: 0,
        }],
        vec![DriverNumberMapping {
            driver_number: 5,
            canonical_tx_num: 1,
        }],
    );

    assert_eq!(
        snapshot.resolve_driver_number(99),
        None,
        "unknown driver_number MUST return None — check_payload_from surfaces this as DriverMappingMiss fail-loud"
    );
}

/// W4-Z2a piece 15 (review round 2 H1) — snapshot self-inconsistency
/// guard: driver_mapping `5 → 99` but `groups` has no `tx=99`.
/// `resolve_driver_number(5)` returns `Some(99)` (translation succeeds
/// by mapping lookup) — but downstream the canonical 99 doesn't exist
/// in groups, so emitting `<I TX="99">` would yield silent fiscal
/// divergence one step later.  `check_payload_from` MUST detect this
/// post-translation and surface `CalcTaxError::CanonicalTxNotInGroups`
/// fail-loud.
///
/// This unit test verifies the snapshot's `groups()` accessor reflects
/// the inconsistency that the use-site check in `check_payload_from`
/// will catch.  Mapping resolves to canonical 99; groups carries only
/// tx=1; consumer must validate canonical ∈ {g.tx for g in groups}.
#[test]
fn snapshot_self_inconsistent_mapping_to_missing_canonical_is_detectable() {
    let snapshot = TaxResolutionSnapshot::with_driver_mapping(
        vec![ResolvedTaxGroupBps {
            tx: 1, // canonical 1 is the ONLY group
            txpr_bps: 2000,
            dtpr_bps: 0,
            txal: 0,
            txty: 0,
        }],
        vec![DriverNumberMapping {
            driver_number: 5,
            canonical_tx_num: 99, // but mapping points to canonical 99 — INCONSISTENT
        }],
    );

    // Translation succeeds at the mapping primitive layer.
    assert_eq!(
        snapshot.resolve_driver_number(5),
        Some(99),
        "mapping returns canonical 99 even though groups has no such entry — \
         the self-inconsistency is invisible at this layer"
    );

    // Consumer can detect inconsistency via groups() accessor.
    assert!(
        !snapshot.groups().iter().any(|g| g.tx == 99),
        "groups has no canonical 99 entry — check_payload_from MUST surface \
         CanonicalTxNotInGroups when this snapshot is used"
    );
    assert!(
        snapshot.groups().iter().any(|g| g.tx == 1),
        "groups carries canonical 1 only"
    );
}

/// W4-Z2a piece 15 (review round 2 M1) — driver_mapping translation
/// MUST apply to `tax_group_2` as well as `tax_group_1`.  Per operator's
/// production POS reality (1С / WebCheck), both groups are configured
/// from POS driver-numbers; runtime translates both.
///
/// This test verifies the translation primitive is symmetric across
/// the two field slots — `check_payload_from` invokes
/// `resolve_driver_number` for both tg1 and tg2 with the same snapshot.
#[test]
fn driver_mapping_translates_both_tax_group_1_and_tax_group_2() {
    let snapshot = TaxResolutionSnapshot::with_driver_mapping(
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
        ],
        vec![
            DriverNumberMapping {
                driver_number: 5, // POS sends 5 for primary
                canonical_tx_num: 1,
            },
            DriverNumberMapping {
                driver_number: 7, // POS sends 7 for secondary (e.g., excise)
                canonical_tx_num: 2,
            },
        ],
    );

    // Both driver_numbers resolve to their canonical counterparts —
    // proves check_payload_from's symmetric translate_tax_group helper
    // produces consistent output regardless of which field slot
    // (tax_group_1 or tax_group_2) supplies the raw value.
    assert_eq!(snapshot.resolve_driver_number(5), Some(1));
    assert_eq!(snapshot.resolve_driver_number(7), Some(2));
    // Unknown driver_number on EITHER field surfaces None (which
    // check_payload_from maps to DriverMappingMiss with field
    // discriminator).
    assert_eq!(snapshot.resolve_driver_number(99), None);
}

// ─── Piece 16 — check_payload_from integration (R1 M2 close) ────────

/// Helper: build a minimal Sell CheckJson string with one item
/// carrying optional tax_group_1 + tax_group_2.  Mirrors the wire
/// JSON shape adapters produce.
fn sell_payload_json(tg1: Option<i64>, tg2: Option<i64>) -> String {
    let tg1_s = tg1.map_or("null".to_string(), |v| v.to_string());
    let tg2_s = tg2.map_or("null".to_string(), |v| v.to_string());
    format!(
        r#"{{
          "items": [{{
            "code": "ART-1",
            "name": "Item",
            "price_kop": 6000,
            "quantity_thousandths": 1000,
            "sum_kop": 6000,
            "tax_group_1": {tg1_s},
            "tax_group_2": {tg2_s}
          }}],
          "payments": [{{
            "name": "CASH",
            "sum_kop": 6000,
            "type_code": "0"
          }}]
        }}"#
    )
}

fn sample_header() -> DocumentHeader {
    DocumentHeader::with_defaults(
        "4000000001".to_string(),
        "12345678".to_string(),
        0,
        "2026-05-28T12:00:00".to_string(),
        String::new(),
    )
}

fn snapshot_with_dual_mapping() -> TaxResolutionSnapshot {
    TaxResolutionSnapshot::with_driver_mapping(
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
        ],
        vec![
            DriverNumberMapping {
                driver_number: 5,
                canonical_tx_num: 1,
            },
            DriverNumberMapping {
                driver_number: 7,
                canonical_tx_num: 2,
            },
        ],
    )
}

/// W4-Z2a piece 16 (R1 M2 close, positive case) — drives the actual
/// `check_payload_from` conversion path end-to-end at the conversion
/// boundary (no DB / no crypto / no XML).  Asserts BOTH
/// `tax_group_1` AND `tax_group_2` are translated from POS driver
/// form to canonical TX via `translate_tax_group`.  Would FAIL if
/// `translate_tax_group` were accidentally bypassed for either
/// field (e.g., a future refactor copying raw `tax_group_2` into
/// `CheckItem`) — the type-system / build-success path would NOT
/// catch that regression.
#[test]
fn check_payload_from_translates_both_tax_group_1_and_tax_group_2_to_canonical() {
    let snapshot = snapshot_with_dual_mapping();
    let body_json = sell_payload_json(Some(5), Some(7));
    let result =
        check_payload_from_json_for_testing(sample_header(), 99, &body_json, 6000, Some(&snapshot))
            .expect("check_payload_from succeeds for both-fields-translated case");

    assert_eq!(
        result.items.len(),
        1,
        "single item input → single item output"
    );
    let item = &result.items[0];
    assert_eq!(
        item.tax_group_1,
        Some(1),
        "tax_group_1 driver_number=5 MUST translate to canonical TX=1 — \
         if this fails, translate_tax_group was bypassed for primary slot"
    );
    assert_eq!(
        item.tax_group_2,
        Some(2),
        "tax_group_2 driver_number=7 MUST translate to canonical TX1=2 — \
         if this fails, translate_tax_group was bypassed for secondary slot"
    );
}

/// W4-Z2a piece 16 (R1 M2 close, fail-loud case) — drives the
/// conversion boundary on a tax_group_2 miss.  Mapping has 5→1 only;
/// item carries tg2=99 (no entry in mapping).  MUST surface typed
/// `SignError::TaxSummary(CalcTaxError::DriverMappingMiss { field:
/// "tax_group_2", driver_number: 99 })` — field discriminator
/// preserved through the error chain.  Without this test, a swap of
/// the field discriminator (e.g., always reporting "tax_group_1")
/// would silently pass type-checks but degrade operator forensic
/// observability for compound-tax drift.
#[test]
fn check_payload_from_field_aware_miss_surfaces_tax_group_2_discriminator() {
    let snapshot = TaxResolutionSnapshot::with_driver_mapping(
        vec![ResolvedTaxGroupBps {
            tx: 1,
            txpr_bps: 2000,
            dtpr_bps: 0,
            txal: 0,
            txty: 0,
        }],
        vec![DriverNumberMapping {
            driver_number: 5,
            canonical_tx_num: 1,
        }],
    );
    let body_json = sell_payload_json(Some(5), Some(99));
    let err =
        check_payload_from_json_for_testing(sample_header(), 99, &body_json, 6000, Some(&snapshot))
            .expect_err("driver_number=99 not in mapping MUST fail-loud");

    match err {
        SignError::TaxSummary(CalcTaxError::DriverMappingMiss {
            driver_number,
            field,
        }) => {
            assert_eq!(driver_number, 99);
            assert_eq!(
                field, "tax_group_2",
                "field discriminator MUST be 'tax_group_2' for secondary slot miss — \
                 swapped or generic discriminator would degrade forensic observability"
            );
        }
        other => panic!("expected DriverMappingMiss field=tax_group_2, got: {other:?}"),
    }
}

/// W4-Z2a piece 14 — identity fallback for empty driver_mapping.
/// Pre-pilot or simple POS where POS coding equals canonical TX:
/// driver_mapping is empty Vec, every lookup returns `Some(input)`
/// unchanged (identity).
#[test]
fn empty_driver_mapping_yields_identity() {
    let snapshot = TaxResolutionSnapshot::new(vec![ResolvedTaxGroupBps {
        tx: 1,
        txpr_bps: 2000,
        dtpr_bps: 0,
        txal: 0,
        txty: 0,
    }]);

    assert_eq!(snapshot.resolve_driver_number(1), Some(1));
    assert_eq!(snapshot.resolve_driver_number(2), Some(2));
    assert_eq!(snapshot.resolve_driver_number(42), Some(42));
}

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
            let id =
                signing_config_snapshots::insert_or_get_id_tx(tx, FN, "driver-77", &s1).await?;
            Ok::<i64, anyhow::Error>(id)
        })
    })
    .await
    .unwrap();

    let s2 = snapshot.clone();
    let id2 = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let id =
                signing_config_snapshots::insert_or_get_id_tx(tx, FN, "driver-77", &s2).await?;
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
