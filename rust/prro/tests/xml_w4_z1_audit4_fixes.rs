//! AUDIT4 round-4 fixes — 4 IMP findings (both auditors converged).
//!
//! - IMP-1: orphan `<D TR="1">` w/o `<NI>` when items empty +
//!   check-level present.  Python `:249` guards entire block with
//!   `if check_discounts and p_item_numbers:`.
//! - IMP-2: zero-skip Python uses SOURCE value (rate for percent),
//!   Rust uses RESOLVED sum.  Percent + zero base → Python emits
//!   `<D SM="0" PR="10.00">`, Rust skips.
//! - IMP-3: `safe_round` misuses RateNotFinite when arithmetic
//!   overflows.  Split into IntermediateOverflow variant.
//! - IMP-4: applies_to_item_ns override blindly trusts caller —
//!   `vec![999]` emits orphan `<NI NI="999">`.  Validate against
//!   tracked p_item_numbers; return typed error if mismatch.
//! - MIN: split CalcTaxError into semantic variants (input invalid
//!   vs intermediate overflow).

use prro::xml::{
    build_canonical_xml, AdjustmentMode, CalcTaxError, CanonicalDoc, CheckItem,
    CheckLevelAdjustment, CheckLevelAdjustmentKind, CheckPayload, CheckPayment,
    DocumentHeader, LineAdjustment, LineAdjustmentKind, XmlBuildError,
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

// ─── AUDIT4-IMP-1: empty items + check-level → no orphan D ────────

#[test]
fn check_level_adjustment_with_zero_items_is_skipped_no_orphan() {
    // Python `:249`: `if check_discounts and p_item_numbers:` —
    // SKIPS entire check-level block when no items.  Rust pre-fix
    // would emit `<D TR="1">` with no `<NI>` (orphan, DPS reject).
    let doc = CanonicalDoc::Sell(CheckPayload {
        header: header(),
        local_number: 1,
        items: vec![],  // empty!
        payments: vec![CheckPayment {
            name: "CASH".into(), sum: 0, type_code: "0".into(),
            ..Default::default()
        }],
        total_sum: 0,
        check_level_adjustments: vec![CheckLevelAdjustment {
            kind: CheckLevelAdjustmentKind::Discount,
            sum: 100,
            mode: AdjustmentMode::Value,
            percent: None, name: None,
            applies_to_item_ns: vec![],  // empty → auto-fill, but empty p_item_numbers
        }],
        ..Default::default()
    });
    let bytes = build_canonical_xml(&doc).expect("build");
    let xml: String = bytes.iter().map(|&b| b as char).collect();
    assert!(!xml.contains("<D "),
        "orphan check-level D must be skipped when items empty: {xml}");
    assert!(!xml.contains("<NI "),
        "no orphan NI: {xml}");
}

#[test]
fn check_level_with_override_subset_emits_even_with_zero_items() {
    // Edge: zero items + check-level with EXPLICIT override.  The
    // override is invalid (no items to reference), so we should
    // surface XmlBuildError::OrphanCheckLevelNi rather than emit
    // dangling NI.
    let doc = CanonicalDoc::Sell(CheckPayload {
        header: header(),
        local_number: 1,
        items: vec![],
        payments: vec![CheckPayment {
            name: "CASH".into(), sum: 0, type_code: "0".into(),
            ..Default::default()
        }],
        total_sum: 0,
        check_level_adjustments: vec![CheckLevelAdjustment {
            kind: CheckLevelAdjustmentKind::Discount,
            sum: 100,
            mode: AdjustmentMode::Value,
            percent: None, name: None,
            applies_to_item_ns: vec![1],  // refers to non-existent P
        }],
        ..Default::default()
    });
    let err = build_canonical_xml(&doc).expect_err("must reject orphan NI ref");
    assert!(matches!(err, XmlBuildError::OrphanCheckLevelNi { .. }),
        "expected OrphanCheckLevelNi, got {err:?}");
}

// ─── AUDIT4-IMP-2: percent-mode zero-skip parity ──────────────────

#[test]
fn percent_mode_adjustment_with_zero_base_emits_not_skipped() {
    // Python `:220`: `d_value = int(d.get('value', 0)); if d_value
    // == 0: continue` — for PERCENT mode `d_value` is the RATE
    // (e.g. 10), NOT the resolved sum.  So free gift (item.sum=0)
    // + 10% discount → Python emits `<D SM="0" PR="10.00">`.
    // Rust pre-fix `adj.sum == 0` would skip → divergent.
    let doc = CanonicalDoc::Sell(CheckPayload {
        header: header(),
        local_number: 1,
        items: vec![CheckItem {
            code: "FREE-GIFT".into(),
            name: "Free".into(),
            price: 0, quantity: 1000, sum: 0,  // base sum = 0
            adjustments: vec![LineAdjustment {
                kind: LineAdjustmentKind::Discount,
                sum: 0,  // resolved: 0 * 10 / 100 = 0
                mode: AdjustmentMode::Percent,
                percent: Some("10.00".to_string()),  // non-zero rate
                name: None, privilege: None, tax_code: None,
            }],
            ..Default::default()
        }],
        payments: vec![CheckPayment {
            name: "CASH".into(), sum: 0, type_code: "0".into(),
            ..Default::default()
        }],
        total_sum: 0,
        ..Default::default()
    });
    let bytes = build_canonical_xml(&doc).expect("build");
    let xml: String = bytes.iter().map(|&b| b as char).collect();
    assert!(xml.contains("<D "),
        "percent-mode adj with non-zero rate MUST emit even at sum=0: {xml}");
    assert!(xml.contains(r#"SM="0""#));
    assert!(xml.contains(r#"PR="10.00""#));
}

#[test]
fn percent_mode_adjustment_with_zero_rate_is_skipped() {
    // Python parity: if `value` (rate for percent) is zero → skip.
    let doc = CanonicalDoc::Sell(CheckPayload {
        header: header(),
        local_number: 1,
        items: vec![CheckItem {
            adjustments: vec![LineAdjustment {
                kind: LineAdjustmentKind::Discount,
                sum: 0,
                mode: AdjustmentMode::Percent,
                percent: Some("0.00".to_string()),  // zero rate
                name: None, privilege: None, tax_code: None,
            }],
            ..item("ART-1", 1000)
        }],
        payments: vec![CheckPayment {
            name: "CASH".into(), sum: 1000, type_code: "0".into(),
            ..Default::default()
        }],
        total_sum: 1000,
        ..Default::default()
    });
    let bytes = build_canonical_xml(&doc).expect("build");
    let xml: String = bytes.iter().map(|&b| b as char).collect();
    assert!(!xml.contains("<D "),
        "zero-rate percent-mode must skip: {xml}");
}

#[test]
fn value_mode_zero_sum_still_skipped() {
    // Regression: VALUE mode zero-sum skip still works (audit3 fix).
    let doc = CanonicalDoc::Sell(CheckPayload {
        header: header(),
        local_number: 1,
        items: vec![CheckItem {
            adjustments: vec![LineAdjustment {
                kind: LineAdjustmentKind::Discount,
                sum: 0,
                mode: AdjustmentMode::Value,
                percent: None, name: None,
                privilege: None, tax_code: None,
            }],
            ..item("ART-1", 1000)
        }],
        payments: vec![CheckPayment {
            name: "CASH".into(), sum: 1000, type_code: "0".into(),
            ..Default::default()
        }],
        total_sum: 1000,
        ..Default::default()
    });
    let bytes = build_canonical_xml(&doc).expect("build");
    let xml: String = bytes.iter().map(|&b| b as char).collect();
    assert!(!xml.contains("<D "), "value-mode zero-sum still skipped");
}

// ─── AUDIT4-IMP-3: split CalcTaxError semantics ───────────────────

#[test]
fn arithmetic_overflow_returns_intermediate_overflow_not_rate_invalid() {
    // Misleading: pre-fix `RateNotFinite { txpr=20.0, dtpr=0.0 }`
    // when rates are FINE but `g * txpr` overflows.  Split into
    // dedicated variant so piece-7 debugging is accurate.
    use prro::xml::calc_tax;
    let err = calc_tax(1_000_000_000, 1e300, 0.0, 0)
        .expect_err("arithmetic overflow");
    assert!(
        matches!(err, CalcTaxError::IntermediateOverflow { .. }),
        "expected IntermediateOverflow, got {err:?}"
    );
}

#[test]
fn negative_rate_returns_invalid_rate_not_overflow() {
    // After split: negative rates return InvalidRate (input-time
    // validation), NOT IntermediateOverflow.
    use prro::xml::calc_tax;
    let err = calc_tax(10000, -100.0, 0.0, 0)
        .expect_err("negative rate");
    assert!(
        matches!(err, CalcTaxError::InvalidRate { .. }),
        "expected InvalidRate, got {err:?}"
    );
}

#[test]
fn nan_input_returns_invalid_rate() {
    use prro::xml::calc_tax;
    let err = calc_tax(10000, f64::NAN, 0.0, 0).expect_err("NaN");
    assert!(matches!(err, CalcTaxError::InvalidRate { .. }));
}

// ─── AUDIT4-IMP-4: override validation ────────────────────────────

#[test]
fn override_subset_with_invalid_n_is_rejected() {
    // POS-precomputed override `vec![999]` doesn't match any actual
    // P-item N.  Pre-fix would emit orphan `<NI NI="999">`.
    // Must surface as typed XmlBuildError.
    let doc = CanonicalDoc::Sell(CheckPayload {
        header: header(),
        local_number: 1,
        items: vec![item("ART-1", 1000), item("ART-2", 2000)],
        payments: vec![CheckPayment {
            name: "CASH".into(), sum: 2900, type_code: "0".into(),
            ..Default::default()
        }],
        total_sum: 2900,
        check_level_adjustments: vec![CheckLevelAdjustment {
            kind: CheckLevelAdjustmentKind::Discount,
            sum: 100,
            mode: AdjustmentMode::Value,
            percent: None, name: None,
            applies_to_item_ns: vec![999],  // invalid
        }],
        ..Default::default()
    });
    let err = build_canonical_xml(&doc).expect_err("must reject");
    assert!(matches!(err, XmlBuildError::OrphanCheckLevelNi { .. }),
        "expected OrphanCheckLevelNi, got {err:?}");
}

#[test]
fn override_subset_with_valid_ns_succeeds() {
    // Sanity: valid override (subset of p_item_numbers) works.
    let doc = CanonicalDoc::Sell(CheckPayload {
        header: header(),
        local_number: 1,
        items: vec![item("ART-1", 1000), item("ART-2", 2000), item("ART-3", 3000)],
        payments: vec![CheckPayment {
            name: "CASH".into(), sum: 5900, type_code: "0".into(),
            ..Default::default()
        }],
        total_sum: 5900,
        check_level_adjustments: vec![CheckLevelAdjustment {
            kind: CheckLevelAdjustmentKind::Discount,
            sum: 100,
            mode: AdjustmentMode::Value,
            percent: None, name: None,
            applies_to_item_ns: vec![1, 3],  // valid subset of items at N=1,2,3
        }],
        ..Default::default()
    });
    let bytes = build_canonical_xml(&doc).expect("valid subset succeeds");
    let xml: String = bytes.iter().map(|&b| b as char).collect();
    assert!(xml.contains(r#"<NI NI="1""#));
    assert!(xml.contains(r#"<NI NI="3""#));
    assert!(!xml.contains(r#"<NI NI="2""#));
}
