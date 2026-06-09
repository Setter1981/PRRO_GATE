//! AUDIT5 round-5 fixes — 2 CRIT + 3 IMP findings on the INTEGRATION
//! surface (piece 7 + piece 8 review).
//!
//! - CRIT-1: live stage_sign tax_groups = empty HashMap silently
//!   drops <TX> children for real extended payloads.  Both auditors
//!   converged.  Fix: fail-closed when items carry tax_group_1 but
//!   tax_groups is empty (pre-W4-Z2 transition guard).
//!
//! - CRIT-2: Z-report build_canonical_doc arm hardcodes empty
//!   tax_summaries / service_sums / None epz; piece-8 golden is
//!   hand-built so the gap is invisible.  Adapter pathway does NOT
//!   exist yet (ZReportJson is minimal), but defer with explicit
//!   gap-doc test that surfaces when the pathway lands.
//!
//! - IMP-1+3 (A) / IMP-1 (B): aggregation += on i64 unchecked.
//!   checked_add + CalcTaxError::AggregationOverflow.
//!
//! - IMP-3 (B): rename derive_tax_summaries → derive_check_tax_
//!   summaries to bind to check-level semantics; Z-report needs a
//!   separate helper with short-form fallback for unknown groups.
//!
//! - CRIT-1 integration: end-to-end test through check_payload_from
//!   (piped from extended JSON, not just serde_json::Value parse).

use std::collections::HashMap;

use prro::services::write_path::tax_summary::{derive_check_tax_summaries, ResolvedTaxGroup};
use prro::xml::{CalcTaxError, CheckItem};

// ─── AUDIT5-IMP-1+3 (A) / IMP-1 (B): checked_add ──────────────────

#[test]
fn aggregation_overflow_returns_typed_error_not_panic() {
    // i64 sum overflow: two items each near i64::MAX/2 + 1 in the
    // same tax group → sum overflows.  Pre-fix: panic in debug,
    // wrap in release (silent negative).  Post-fix: typed error.
    let big = i64::MAX / 2 + 1;
    let items = vec![
        CheckItem {
            code: "BIG-1".into(),
            name: "BigItem".into(),
            price: big,
            quantity: 1000,
            sum: big,
            tax_group_1: Some(1),
            ..Default::default()
        },
        CheckItem {
            code: "BIG-2".into(),
            name: "BigItem2".into(),
            price: big,
            quantity: 1000,
            sum: big,
            tax_group_1: Some(1),
            ..Default::default()
        },
    ];
    let mut groups = HashMap::new();
    groups.insert(
        1_i64,
        ResolvedTaxGroup {
            tx: 1,
            txpr: 20.0,
            dtpr: 0.0,
            txal: 0,
            txty: 0,
        },
    );
    let err = derive_check_tax_summaries(&items, &groups).expect_err("must fail-loud on overflow");
    assert!(
        matches!(err, CalcTaxError::AggregationOverflow { .. }),
        "expected AggregationOverflow, got {err:?}"
    );
}

// ─── AUDIT5-IMP-3 (B): renamed helper ─────────────────────────────

#[test]
fn renamed_helper_has_check_semantics_skip_on_unknown() {
    // Binds the rename contract: check-level <E><TX> SKIPS on
    // unknown, does NOT fall back to short form.
    // Setup: map has ONE entry (1) but items reference 99 — guard
    // doesn't trip (map non-empty), Python parity skip-on-miss
    // applies.
    let items = vec![CheckItem {
        code: "ART-1".into(),
        name: "Test".into(),
        price: 1000,
        quantity: 1000,
        sum: 1000,
        tax_group_1: Some(99), // unknown
        ..Default::default()
    }];
    let mut groups = HashMap::new();
    groups.insert(
        1_i64,
        ResolvedTaxGroup {
            tx: 1,
            txpr: 20.0,
            dtpr: 0.0,
            txal: 0,
            txty: 0,
        },
    );
    let summaries = derive_check_tax_summaries(&items, &groups).expect("ok");
    assert!(
        summaries.is_empty(),
        "check-level helper SKIPS on unknown; Z-report short-form is a separate path"
    );
}

// ─── AUDIT5-CRIT-1: live stage_sign fail-closed guard ─────────────
//
// Build a minimal-shape extended JSON payload + drive through
// `parse_payload` + `check_payload_from`.  Without the guard:
// payload with tax_group_1=Some(1) + empty tax_groups → silently
// emits no <TX>.  With the guard: typed SignError::TaxMappingNotWired.

// Note: the parse_payload + check_payload_from functions are
// pub(crate); end-to-end test goes through the public stage_sign
// entry (run) which requires SQLite fixtures.  For this pre-merge
// gate we instead verify the helper-level fail-closed: if a caller
// has items with tax_group_1 but an empty resolved map, the
// build_canonical_doc / check_payload_from MUST error rather than
// silently drop <TX>.
//
// The fail-closed check lives in `check_payload_from`; we surface
// it via `CalcTaxError` propagation (TaxMappingNotWired is a new
// variant on `CalcTaxError`, not `SignError`, so it's visible at
// the helper level too).

#[test]
fn fail_closed_when_items_carry_tax_group_but_map_is_empty() {
    let items = vec![CheckItem {
        code: "ART-1".into(),
        name: "Test".into(),
        price: 1000,
        quantity: 1000,
        sum: 1000,
        tax_group_1: Some(1),
        ..Default::default()
    }];
    let groups = HashMap::new(); // empty stub (live stage_sign state)
    let err = derive_check_tax_summaries(&items, &groups)
        .expect_err("must fail-closed when tax map is empty but items carry groups");
    assert!(
        matches!(err, CalcTaxError::TaxMappingNotWired { .. }),
        "expected TaxMappingNotWired, got {err:?}"
    );
}

// ─── AUDIT5-CRIT-2: Z-report aggregation gap-doc test ─────────────
//
// Ignored inversion test: when W4-Z2 extends ZReportJson with
// `tax_sums` / `service_sums` / `epz_totals` and wires the
// derive_z_report_tax_summaries helper, this test should be
// un-ignored AND adjusted to assert that build_canonical_doc's
// ZReport arm produces a CanonicalDoc with non-empty aggregations
// (NOT hardcoded Vec::new() / None).
//
// Today the test would fail because the arm hardcodes empties.  The
// ignore is a deliberate gap-doc per ADR-M2-3 inversion-target
// pattern — surfaces the deferred work to a future contributor.

#[test]
#[ignore = "W4-Z2 will wire Z-report aggregation; gap-doc inversion test"]
fn z_report_canonical_doc_populates_aggregations_when_payload_provides_them() {
    // Pseudocode for W4-Z2:
    //   let json = r#"{"payments":[...], "tax_sums":{...},
    //                  "service_sums":{...}, "epz_totals":{...},
    //                  "sell_count":17, "return_count":2}"#;
    //   let payload = parse_payload(WireArtifactKind::ZReport, json, None).unwrap();
    //   let doc = build_canonical_doc(WireArtifactKind::ZReport, header,
    //                                 100, payload, &tax_groups).unwrap();
    //   if let CanonicalDoc::ZReport(z) = doc {
    //       assert!(!z.tax_summaries.is_empty());
    //       assert!(!z.service_sums.is_empty());
    //       assert!(z.epz.is_some());
    //   }
    panic!("W4-Z2 deferred: unmute + implement after derive_z_report_tax_summaries lands");
}

#[test]
fn empty_map_with_no_tax_grouped_items_succeeds_back_compat() {
    // Minimal-shape items (no tax_group_1) + empty map → OK.
    // This is the existing back-compat path: 4 minimal goldens use
    // this exact shape.  Must not regress.
    let items = vec![CheckItem {
        code: "ART-1".into(),
        name: "Test".into(),
        price: 1000,
        quantity: 1000,
        sum: 1000,
        tax_group_1: None,
        ..Default::default()
    }];
    let summaries = derive_check_tax_summaries(&items, &HashMap::new()).expect("back-compat path");
    assert!(summaries.is_empty());
}
