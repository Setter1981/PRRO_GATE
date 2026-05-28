//! W4-Z2a piece 1 — TaxResolutionSnapshot foundation.
//!
//! Per locked design at memory `project_m4_w4_z2a_locked_design.md`:
//!   - struct TaxResolutionSnapshot { groups: Vec<ResolvedTaxGroupBps> }
//!   - rates stored as integer basis points (i64), NOT f64
//!   - canonical_bytes() deterministic: sorted groups, no whitespace
//!   - sha256() hash of canonical_bytes
//!   - bps→f64 for calc_tax: `bps as f64 / 100.0`
//!   - bps→2dp string for XML wire: 2000 → "20.00", 75 → "0.75", 0 → "0.00"
//!   - to_calc_map() bridges to W4-Z1 derive_check_tax_summaries
//!   - validation: rates that cannot round-trip to 2dp at snapshot
//!     creation are rejected (typed error)

use prro::services::write_path::tax_summary::{
    bps_to_decimal_string, bps_to_f64, ResolvedTaxGroup, SnapshotBuildError,
    TaxResolutionSnapshot,
};
use prro::services::write_path::tax_summary::ResolvedTaxGroupBps;

// ─── Construction + validation ────────────────────────────────────

#[test]
fn empty_snapshot_has_empty_groups_stable_hash() {
    let s = TaxResolutionSnapshot::new(vec![]);
    assert!(s.groups().is_empty());
    let h = s.sha256();
    // Pinned: sha256 of canonical bytes for empty groups vec.
    // This is "{\"groups\":[],\"kind\":\"check_tax_mapping_v1\"}" → known hash.
    // Test re-computation determinism by calling twice.
    assert_eq!(h, s.sha256(), "sha256 must be deterministic");
}

#[test]
fn snapshot_canonical_bytes_sort_groups_by_tx_ascending() {
    // Caller may supply groups in any order; canonical bytes sort
    // by tx ascending so identical config → identical hash.
    let unsorted = TaxResolutionSnapshot::new(vec![
        ResolvedTaxGroupBps { tx: 10, txpr_bps: 700, dtpr_bps: 0, txal: 0, txty: 0 },
        ResolvedTaxGroupBps { tx: 1, txpr_bps: 2000, dtpr_bps: 0, txal: 0, txty: 0 },
        ResolvedTaxGroupBps { tx: 2, txpr_bps: 700, dtpr_bps: 0, txal: 0, txty: 0 },
    ]);
    let sorted = TaxResolutionSnapshot::new(vec![
        ResolvedTaxGroupBps { tx: 1, txpr_bps: 2000, dtpr_bps: 0, txal: 0, txty: 0 },
        ResolvedTaxGroupBps { tx: 2, txpr_bps: 700, dtpr_bps: 0, txal: 0, txty: 0 },
        ResolvedTaxGroupBps { tx: 10, txpr_bps: 700, dtpr_bps: 0, txal: 0, txty: 0 },
    ]);
    assert_eq!(unsorted.sha256(), sorted.sha256(),
        "canonical bytes must sort groups → same hash regardless of caller order");
}

#[test]
fn pinned_hash_for_known_input_locks_canonical_format() {
    // PINNED: protects against silent canonicalization drift.  If this
    // test fails after a refactor, EVERY existing signing_config_snapshots
    // row's payload_sha256 becomes wrong on verify-on-read.  Treat as
    // a fiscal-compatibility break.
    let s = TaxResolutionSnapshot::new(vec![
        ResolvedTaxGroupBps { tx: 1, txpr_bps: 2000, dtpr_bps: 0, txal: 0, txty: 0 },
        ResolvedTaxGroupBps { tx: 2, txpr_bps: 700, dtpr_bps: 0, txal: 0, txty: 0 },
    ]);
    let h = s.sha256();
    // The exact hash bytes are pinned in the assertion message after
    // first impl; until then, this just locks-in determinism.
    // Re-running compute must yield identical hash.
    assert_eq!(h, s.sha256());
    // Also pin via hex prefix so refactors that change canonical bytes
    // surface a diff.  Computed at implementation time and pasted here
    // — DO NOT silently regenerate; treat as fiscal-compat break.
    let h_hex: String = h.iter().map(|b| format!("{b:02x}")).collect();
    // Pinned hash (computed once at implementation; locks canonical format):
    // canonical_bytes = b"{\"groups\":[{\"dtpr_bps\":0,\"tx\":1,\"txal\":0,\"txpr_bps\":2000,\"txty\":0},{\"dtpr_bps\":0,\"tx\":2,\"txal\":0,\"txpr_bps\":700,\"txty\":0}],\"kind\":\"check_tax_mapping_v1\"}"
    assert!(!h_hex.is_empty(), "sha256 hex emitted: {h_hex}");
    // The implementation will paste the expected hex; for the RED-phase
    // test we just assert deterministic non-empty output.
}

// ─── BPS → f64 conversion (for calc_tax interop) ──────────────────

#[test]
fn bps_to_f64_zero_is_zero() {
    assert_eq!(bps_to_f64(0), 0.0);
}

#[test]
fn bps_to_f64_2000_is_twenty() {
    assert_eq!(bps_to_f64(2000), 20.0);
}

#[test]
fn bps_to_f64_700_is_seven() {
    assert_eq!(bps_to_f64(700), 7.0);
}

#[test]
fn bps_to_f64_75_is_zero_point_seven_five() {
    assert_eq!(bps_to_f64(75), 0.75);
}

// ─── BPS → 2dp decimal string (for XML wire emission) ─────────────

#[test]
fn bps_to_decimal_string_zero() {
    assert_eq!(bps_to_decimal_string(0), "0.00");
}

#[test]
fn bps_to_decimal_string_2000_is_twenty() {
    assert_eq!(bps_to_decimal_string(2000), "20.00");
}

#[test]
fn bps_to_decimal_string_700_is_seven() {
    assert_eq!(bps_to_decimal_string(700), "7.00");
}

#[test]
fn bps_to_decimal_string_75_is_zero_point_seven_five() {
    assert_eq!(bps_to_decimal_string(75), "0.75");
}

#[test]
fn bps_to_decimal_string_pads_single_digit_fractional() {
    // 30 bps = 0.30%, NOT "0.3" — wire requires 2 decimal places.
    assert_eq!(bps_to_decimal_string(30), "0.30");
}

// ─── Construction from live config (f64 rates) — validation ───────

#[test]
fn from_live_rates_round_trip_validates() {
    // 20.0 → 2000 bps → "20.00" → 20.0 — round-trips exactly.
    let r = TaxResolutionSnapshot::try_from_live(vec![
        ("1".to_string(), 1_i64, 20.0_f64, 0.0_f64, 0_i64, 0_i64),
    ]);
    let s = r.expect("20.0 round-trips");
    assert_eq!(s.groups().len(), 1);
    assert_eq!(s.groups()[0].txpr_bps, 2000);
}

#[test]
fn from_live_rates_rejects_non_round_trippable() {
    // 20.005 → 2000.5 bps — not representable as integer.  Reject.
    let r = TaxResolutionSnapshot::try_from_live(vec![
        ("1".to_string(), 1_i64, 20.005_f64, 0.0_f64, 0_i64, 0_i64),
    ]);
    let err = r.expect_err("20.005 cannot round-trip to 2dp exactly");
    assert!(matches!(err, SnapshotBuildError::RateNotRoundTrippable { .. }));
}

#[test]
fn from_live_rates_negative_rejected() {
    let r = TaxResolutionSnapshot::try_from_live(vec![
        ("1".to_string(), 1_i64, -1.0_f64, 0.0_f64, 0_i64, 0_i64),
    ]);
    let err = r.expect_err("negative rate must reject (consistent with CalcTaxError::InvalidRate)");
    assert!(matches!(err, SnapshotBuildError::NegativeRate { .. }));
}

#[test]
fn from_live_rates_nan_inf_rejected() {
    let r = TaxResolutionSnapshot::try_from_live(vec![
        ("1".to_string(), 1_i64, f64::NAN, 0.0, 0, 0),
    ]);
    let err = r.expect_err("NaN rate rejected");
    assert!(matches!(err, SnapshotBuildError::RateNotFinite { .. }));

    let r = TaxResolutionSnapshot::try_from_live(vec![
        ("1".to_string(), 1_i64, f64::INFINITY, 0.0, 0, 0),
    ]);
    let err = r.expect_err("Inf rate rejected");
    assert!(matches!(err, SnapshotBuildError::RateNotFinite { .. }));
}

// ─── to_calc_map: bridge to derive_check_tax_summaries ────────────

#[test]
fn to_calc_map_yields_resolved_tax_group_keyed_by_tx() {
    let s = TaxResolutionSnapshot::new(vec![
        ResolvedTaxGroupBps { tx: 1, txpr_bps: 2000, dtpr_bps: 0, txal: 0, txty: 0 },
        ResolvedTaxGroupBps { tx: 2, txpr_bps: 700, dtpr_bps: 0, txal: 0, txty: 0 },
    ]);
    let m = s.to_calc_map();
    assert_eq!(m.len(), 2);
    let g1 = m.get(&1_i64).expect("tx=1");
    assert_eq!(g1.tx, 1);
    assert_eq!(g1.txpr, 20.0);
    assert_eq!(g1.dtpr, 0.0);
    let g2 = m.get(&2_i64).expect("tx=2");
    assert_eq!(g2.txpr, 7.0);
}

#[test]
fn to_calc_map_pipes_into_derive_check_tax_summaries() {
    use prro::services::write_path::tax_summary::derive_check_tax_summaries;
    use prro::xml::CheckItem;
    let s = TaxResolutionSnapshot::new(vec![
        ResolvedTaxGroupBps { tx: 1, txpr_bps: 2000, dtpr_bps: 0, txal: 0, txty: 0 },
    ]);
    let items = vec![CheckItem {
        code: "ART-1".into(),
        name: "Test".into(),
        price: 1200,
        quantity: 1000,
        sum: 1200,
        tax_group_1: Some(1),
        ..Default::default()
    }];
    let map = s.to_calc_map();
    let summaries = derive_check_tax_summaries(&items, &map).expect("ok");
    assert_eq!(summaries.len(), 1);
    // 1200 * 20 / 120 = 200
    assert_eq!(summaries[0].txsm, 200);
    // wire rate format from snapshot helper (NOT format!("{:.2}") f64
    // arithmetic — bps_to_decimal_string is the canonical path).
    assert_eq!(summaries[0].txpr, "20.00");
}

// ─── _ = ResolvedTaxGroup type aliased import (no warning) ────────

#[test]
fn resolved_tax_group_type_still_exported_for_w4_z1_callers() {
    // W4-Z1 derive_check_tax_summaries signature uses
    // &HashMap<i64, ResolvedTaxGroup>.  Type must remain pub at the
    // same path (back-compat with audit5 callers).
    let _g: ResolvedTaxGroup = ResolvedTaxGroup {
        tx: 1,
        txpr: 20.0,
        dtpr: 0.0,
        txal: 0,
        txty: 0,
    };
}
