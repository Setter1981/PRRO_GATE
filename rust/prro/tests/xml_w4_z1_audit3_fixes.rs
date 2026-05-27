//! AUDIT3 round-3 fixes — 1 CRIT + 2 IMP findings.
//!
//! Two independent auditors converged on:
//!  - CRIT-1: calc_tax intermediate Inf/NaN through arithmetic
//!    (finite txpr=-100 → 100+txpr=0 → div by 0; or finite huge
//!    txpr → multiplication overflow).  Round-2 is_finite guard
//!    only protects INPUT, not intermediate result.
//!  - IMP-1 (A): Python skips zero-sum adjustments WITHOUT
//!    incrementing item_no (`dps_xml.py:220-222, :254-256`); Rust
//!    pre-fix emitted `<D SM="0">` and consumed N.
//!  - IMP-1 (B): check-level <NI> children — Python auto-tracks
//!    `p_item_numbers` over the items loop and iterates ALL items;
//!    Rust forces caller to predict every N value via
//!    `applies_to_item_ns`.  Per operator clarification: POS may
//!    legitimately precompute a SUBSET (e.g. "discount only on
//!    alcohol items"), so we keep the field as an OPTIONAL OVERRIDE
//!    — empty Vec → auto-fill with all p_item_numbers (Python
//!    parity), non-empty → use caller's subset as-is.

use prro::xml::{
    build_canonical_xml, AdjustmentMode, CalcTaxError, CanonicalDoc, CheckItem,
    CheckLevelAdjustment, CheckLevelAdjustmentKind, CheckPayload, CheckPayment,
    DocumentHeader, LineAdjustment, LineAdjustmentKind,
};

fn header() -> DocumentHeader {
    DocumentHeader::with_defaults("4538765845", "TN-12345", 0_u32, "20260527100000", "")
}

fn item(code: &str, sum: i64) -> CheckItem {
    CheckItem {
        code: code.into(),
        name: "Item".into(),
        price: sum,
        quantity: 1000,
        sum,
        ..Default::default()
    }
}

fn build_sell_xml(payload: CheckPayload) -> String {
    let bytes = build_canonical_xml(&CanonicalDoc::Sell(payload)).expect("build");
    bytes.iter().map(|&b| b as char).collect()
}

// ─── AUDIT3-CRIT-1: intermediate Inf/NaN through arithmetic ───────
//
// Both auditors converged: input-only is_finite check is insufficient.
// Examples that bypass round-2 guard:
//   txpr = -100.0 → 100 + txpr = 0.0 → g/0 = ±Inf → banker's panic
//   txpr = 1e300  → g * txpr could overflow to Inf
//   dtpr = -100.0 (TXAL=2) → 100 + dtpr = 0.0 → same hazard

#[test]
fn calc_tax_txpr_minus_100_yields_typed_error_not_panic() {
    use prro::xml::calc_tax;
    // Round-2 guard only checks input finite-ness; -100.0 is finite.
    // Intermediate 100+(-100) = 0 → divide-by-zero → Inf.  Must
    // surface as typed error.
    let err = calc_tax(10000, -100.0, 0.0, 0)
        .expect_err("txpr=-100 must error, not panic");
    assert!(matches!(err, CalcTaxError::InvalidRate { .. }));
}

#[test]
fn calc_tax_dtpr_minus_100_for_txal_2_yields_typed_error() {
    use prro::xml::calc_tax;
    // TXAL=2 formula uses (100.0 + dtpr).  dtpr=-100 → 0 →
    // intermediate Inf.
    let err = calc_tax(10000, 20.0, -100.0, 2)
        .expect_err("dtpr=-100 with TXAL=2 must error");
    assert!(matches!(err, CalcTaxError::InvalidRate { .. }));
}

#[test]
fn calc_tax_huge_finite_txpr_overflow_yields_typed_error() {
    use prro::xml::calc_tax;
    // txpr=1e300 is finite, but g * txpr overflows to Inf for any
    // non-trivial group_sum.  Intermediate Inf must surface as
    // typed error rather than silent saturation or panic.
    let err = calc_tax(1_000_000_000, 1e300, 0.0, 0)
        .expect_err("huge txpr arithmetic overflow must error");
    // Post-AUDIT4 split: arithmetic overflow → IntermediateOverflow
    // (rates were finite + non-negative, intermediate broke).
    assert!(matches!(err, CalcTaxError::IntermediateOverflow { .. }));
}

// ─── AUDIT3-IMP-1 (A): zero-sum adjustment filter ─────────────────

#[test]
fn per_item_zero_sum_adjustment_is_skipped_no_counter_advance() {
    // Python `:220-222`: `if d_value == 0: continue`.  Skipped
    // BEFORE item_no increment.  Rust pre-fix emitted `<D SM="0">`
    // and consumed N → subsequent items got wrong N.
    //
    // Setup: item1 with [zero-disc, real-disc-50], item2.
    // Python trace:
    //   P1@N=1, ic→2
    //     zero-disc → continue (no emit, no ic++)
    //     real-disc@N=2 NI=1, ic→3
    //   P2@N=3, ic→4
    //   M@N=4
    let payload = CheckPayload {
        header: header(),
        local_number: 1,
        items: vec![
            CheckItem {
                adjustments: vec![
                    LineAdjustment {
                        kind: LineAdjustmentKind::Discount,
                        sum: 0,  // zero — must be skipped per Python
                        mode: AdjustmentMode::Value,
                        percent: None, name: None,
                        privilege: None, tax_code: None,
                    },
                    LineAdjustment {
                        kind: LineAdjustmentKind::Discount,
                        sum: 50,
                        mode: AdjustmentMode::Value,
                        percent: None, name: None,
                        privilege: None, tax_code: None,
                    },
                ],
                ..item("ART-1", 1000)
            },
            item("ART-2", 2000),
        ],
        payments: vec![CheckPayment {
            name: "CASH".into(), sum: 2950, type_code: "0".into(),
            ..Default::default()
        }],
        total_sum: 2950,
        ..Default::default()
    };
    let xml = build_sell_xml(payload);
    // Sanity: only ONE <D> emitted (zero-sum filtered).
    assert_eq!(xml.matches("<D ").count(), 1,
        "exactly one D emitted (zero filtered): {xml}");
    // No <D SM="0"> in wire.
    assert!(!xml.contains(r#"SM="0""#) || !xml.contains("<D"),
        "no <D SM=0> emitted: {xml}");
    // Counter integrity: P2 at N=3 (NOT N=4 — zero did not consume).
    assert!(xml.contains(r#"<P C="ART-2" N="3""#),
        "ART-2 at N=3 (zero adj skipped): {xml}");
    // M at N=4.
    assert!(xml.contains(r#"<M N="4""#), "M at N=4: {xml}");
}

#[test]
fn check_level_zero_sum_adjustment_is_skipped_no_counter_advance() {
    // Python `:254-256`: same skip-without-increment for check-level.
    let payload = CheckPayload {
        header: header(),
        local_number: 1,
        items: vec![item("ART-1", 1000)],
        payments: vec![CheckPayment {
            name: "CASH".into(), sum: 1000, type_code: "0".into(),
            ..Default::default()
        }],
        total_sum: 1000,
        check_level_adjustments: vec![
            CheckLevelAdjustment {
                kind: CheckLevelAdjustmentKind::Discount,
                sum: 0,  // zero → skip
                mode: AdjustmentMode::Value,
                percent: None, name: None,
                applies_to_item_ns: vec![],
            },
        ],
        ..Default::default()
    };
    let xml = build_sell_xml(payload);
    // No <D> emitted.
    assert!(!xml.contains("<D "),
        "zero check-level D must be skipped: {xml}");
    // M at N=2 (NOT N=3).
    assert!(xml.contains(r#"<M N="2""#),
        "M at N=2 (zero check-level did not consume): {xml}");
}

// ─── AUDIT3-IMP-1 (B) MODIFIED: auto-track p_item_numbers ─────────

#[test]
fn check_level_adjustment_empty_applies_auto_fills_all_p_item_numbers() {
    // Python `:260`: `for n in p_item_numbers` — ALL items, always.
    // Per operator clarification: POS may sometimes precompute a
    // SUBSET (e.g. discount only on alcohol).  Compromise:
    //   - applies_to_item_ns EMPTY → auto-fill with ALL tracked
    //     p_item_numbers (Python parity).
    //   - applies_to_item_ns NON-EMPTY → use as-is (POS subset).
    //
    // Setup: 3 items, check-level discount with EMPTY applies.
    let payload = CheckPayload {
        header: header(),
        local_number: 1,
        items: vec![
            item("ART-1", 1000),
            item("ART-2", 2000),
            item("ART-3", 3000),
        ],
        payments: vec![CheckPayment {
            name: "CASH".into(), sum: 5900, type_code: "0".into(),
            ..Default::default()
        }],
        total_sum: 5900,
        check_level_adjustments: vec![
            CheckLevelAdjustment {
                kind: CheckLevelAdjustmentKind::Discount,
                sum: 100,
                mode: AdjustmentMode::Value,
                percent: None, name: None,
                applies_to_item_ns: vec![],  // empty → auto-fill ALL
            },
        ],
        ..Default::default()
    };
    let xml = build_sell_xml(payload);
    // 3 items → 3 <NI> children inside check-level <D>.
    assert_eq!(xml.matches("<NI ").count(), 3,
        "3 NI children auto-filled from p_item_numbers: {xml}");
    // NI values must be 1, 2, 3 (items' N values).
    assert!(xml.contains(r#"<NI NI="1""#));
    assert!(xml.contains(r#"<NI NI="2""#));
    assert!(xml.contains(r#"<NI NI="3""#));
}

#[test]
fn check_level_adjustment_subset_applies_uses_caller_values() {
    // POS-precomputed subset: discount only on items 1 and 3.
    let payload = CheckPayload {
        header: header(),
        local_number: 1,
        items: vec![
            item("ART-1", 1000),
            item("ART-2", 2000),
            item("ART-3", 3000),
        ],
        payments: vec![CheckPayment {
            name: "CASH".into(), sum: 5900, type_code: "0".into(),
            ..Default::default()
        }],
        total_sum: 5900,
        check_level_adjustments: vec![
            CheckLevelAdjustment {
                kind: CheckLevelAdjustmentKind::Discount,
                sum: 100,
                mode: AdjustmentMode::Value,
                percent: None, name: None,
                applies_to_item_ns: vec![1, 3],  // POS subset
            },
        ],
        ..Default::default()
    };
    let xml = build_sell_xml(payload);
    assert_eq!(xml.matches("<NI ").count(), 2,
        "2 NI children from caller subset: {xml}");
    assert!(xml.contains(r#"<NI NI="1""#));
    assert!(xml.contains(r#"<NI NI="3""#));
    assert!(!xml.contains(r#"<NI NI="2""#),
        "NI=2 NOT emitted (not in subset): {xml}");
}
