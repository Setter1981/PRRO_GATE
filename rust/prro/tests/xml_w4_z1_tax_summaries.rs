//! W4-Z1 piece 5 — `<TX>` tax-group summaries inside `<E>` +
//! `calc_tax` TXAL formulas (0/1/2).
//!
//! Mirror of Python `dps_xml.py:_calc_tax:536-563` and
//! `_build_e_element:514-530`.  Pinning tests for:
//!   - `calc_tax` arithmetic across the three supported TXAL modes,
//!   - `<TX>` child element emission inside `<E>` with alphabetically
//!     sorted attrs (DTPR/DTSM/TX/TXAL/TXPR/TXSM/TXTY),
//!   - back-compat: empty `tax_summaries` => no `<TX>` emitted,
//!   - TXAL=3 returns (0,0) per operator-confirmed deferral.

use prro::xml::{
    build_canonical_xml, calc_tax, CalcTaxError, CanonicalDoc, CheckItem, CheckPayload,
    CheckPayment, DocumentHeader, TaxGroupSummary,
};

fn calc_tax_ok(g: i64, txpr: f64, dtpr: f64, txal: i64) -> (i64, i64) {
    calc_tax(g, txpr, dtpr, txal).expect("supported TXAL")
}

fn header() -> DocumentHeader {
    DocumentHeader::with_defaults("4538765845", "TN-12345", 0_u32, "20260527100000", "")
}

fn build_sell(tax_summaries: Vec<TaxGroupSummary>) -> String {
    let doc = CanonicalDoc::Sell(CheckPayload {
        header: header(),
        local_number: 1,
        items: vec![CheckItem {
            code: "ART-1".into(),
            name: "Test".into(),
            price: 12000,
            quantity: 1000,
            sum: 12000,
            ..Default::default()
        }],
        payments: vec![CheckPayment {
            name: "CASH".into(),
            sum: 12000,
            type_code: "0".into(),
            ..Default::default()
        }],
        total_sum: 12000,
        tax_summaries,
        ..Default::default()
    });
    let bytes = build_canonical_xml(&doc).expect("build");
    bytes.iter().map(|&b| b as char).collect()
}

// ─── calc_tax: TXAL=0 (VAT-included) ──────────────────────────────

#[test]
fn calc_tax_txal_0_vat_included_20_percent() {
    // 12000 * 20 / 120 = 2000
    let (txsm, dtsm) = calc_tax_ok(12000, 20.0, 0.0, 0);
    assert_eq!(txsm, 2000, "VAT-included 20% on 12000 kop = 2000 kop");
    assert_eq!(dtsm, 0, "TXAL=0 always emits dtsm=0");
}

#[test]
fn calc_tax_txal_0_with_zero_rate() {
    let (txsm, dtsm) = calc_tax_ok(10000, 0.0, 0.0, 0);
    assert_eq!(txsm, 0);
    assert_eq!(dtsm, 0);
}

// ─── calc_tax: TXAL=1 (excise pre-VAT) ────────────────────────────

#[test]
fn calc_tax_txal_1_excise_pre_vat() {
    // group=10000, dtpr=5%, txpr=20%
    //   dtsm = 10000 * 5 / 100 = 500
    //   txsm = (10000 + 500) * 20 / 120 = 10500 * 20 / 120 = 1750
    let (txsm, dtsm) = calc_tax_ok(10000, 20.0, 5.0, 1);
    assert_eq!(dtsm, 500, "TXAL=1 dtsm = group * dtpr / 100");
    assert_eq!(txsm, 1750, "TXAL=1 txsm = (group + dtsm) * txpr / (100 + txpr)");
}

// ─── calc_tax: TXAL=2 (excise post-VAT) ───────────────────────────

#[test]
fn calc_tax_txal_2_excise_post_vat() {
    // group=10500, dtpr=5%, txpr=20%
    //   dtsm = 10500 * 5 / 105 = 500
    //   txsm = (10500 - 500) * 20 / 120 = 10000 * 20 / 120 = 1666.67 → 1667 (banker's: .67 rounds up)
    let (txsm, dtsm) = calc_tax_ok(10500, 20.0, 5.0, 2);
    assert_eq!(dtsm, 500, "TXAL=2 dtsm = group * dtpr / (100 + dtpr)");
    // round-half-to-even: 1666.666... → 1667 (not a .5 boundary)
    assert_eq!(txsm, 1667);
}

// ─── calc_tax: TXAL=3 — operator-deferred ─────────────────────────

#[test]
fn calc_tax_txal_3_returns_unsupported_algorithm_error() {
    // Operator-confirmed: TXAL=3 (per-unit excise) "не потрібен" —
    // but silent (0,0) before would drop excise sums; caller (piece
    // 7+) MUST react.  Surface via typed error.
    let err = calc_tax(10000, 20.0, 5.0, 3).expect_err("TXAL=3 unsupported");
    assert!(matches!(err, CalcTaxError::UnsupportedAlgorithm(3)));
}

#[test]
fn calc_tax_nan_and_inf_rates_are_guarded_no_panic() {
    // AUDIT2-CRIT-1 (B) defensive: corrupted config could yield
    // NaN/Inf rates.  Without a guard, NaN floor + saturating
    // `as i64` could land on i64::MAX in the banker's-half branch
    // and `MAX + 1` would overflow-panic in debug.  Empirically
    // current code doesn't hit that path (NaN comparisons + guards
    // route around), but pin the contract: NaN/Inf returns the
    // typed `RateNotFinite` error rather than silent 0 or panic.
    let err = calc_tax(10000, f64::NAN, 0.0, 0).expect_err("NaN txpr");
    assert!(matches!(err, CalcTaxError::RateNotFinite { .. }));

    let err = calc_tax(10000, f64::INFINITY, 0.0, 0).expect_err("Inf txpr");
    assert!(matches!(err, CalcTaxError::RateNotFinite { .. }));

    let err = calc_tax(10000, 20.0, f64::NAN, 1).expect_err("NaN dtpr");
    assert!(matches!(err, CalcTaxError::RateNotFinite { .. }));
}

#[test]
fn calc_tax_unknown_txal_also_errors() {
    // Forward-compat: any unknown txal returns typed error (NOT
    // silent zero).  Caller obligated to audit_log + decide policy.
    let err = calc_tax(10000, 20.0, 5.0, 99).expect_err("unknown txal");
    assert!(matches!(err, CalcTaxError::UnsupportedAlgorithm(99)));
}

// ─── calc_tax: banker's rounding at .5 boundaries ─────────────────
//
// Per Python `round()` semantics (banker's = round-half-to-even).
// Rust `f64::round()` is round-half-AWAY-from-zero and would
// produce DIFFERENT results at half-boundaries — fails byte-equiv
// with Python oracle for ANY payload landing on `.5`.

#[test]
fn calc_tax_txal_0_banker_half_to_even_at_boundary() {
    // 15 * 20 / 120 = 2.5 → Python banker's → 2 (even).
    // Rust f64::round would yield 3 (half-away-from-zero) — wrong.
    let (txsm, _) = calc_tax_ok(15, 20.0, 0.0, 0);
    assert_eq!(txsm, 2, "2.5 must round to 2 (even), not 3");

    // 27 * 20 / 120 = 4.5 → Python → 4 (even).
    let (txsm, _) = calc_tax_ok(27, 20.0, 0.0, 0);
    assert_eq!(txsm, 4, "4.5 must round to 4 (even), not 5");

    // 45 * 20 / 120 = 7.5 → Python → 8 (even).
    let (txsm, _) = calc_tax_ok(45, 20.0, 0.0, 0);
    assert_eq!(txsm, 8, "7.5 must round to 8 (even)");

    // 75 * 20 / 120 = 12.5 → Python → 12 (even).
    let (txsm, _) = calc_tax_ok(75, 20.0, 0.0, 0);
    assert_eq!(txsm, 12, "12.5 must round to 12 (even), not 13");
}

#[test]
fn calc_tax_txal_0_non_boundary_unchanged() {
    // Sanity: non-boundary values still round to nearest.
    let (txsm, _) = calc_tax_ok(10, 20.0, 0.0, 0);
    // 10 * 20 / 120 = 1.666... → 2.
    assert_eq!(txsm, 2);

    let (txsm, _) = calc_tax_ok(20, 20.0, 0.0, 0);
    // 20 * 20 / 120 = 3.333... → 3.
    assert_eq!(txsm, 3);
}

// ─── Emission: empty tax_summaries → no <TX> ──────────────────────

#[test]
fn empty_tax_summaries_emits_no_tx_inside_e() {
    let xml = build_sell(vec![]);
    assert!(!xml.contains("<TX "), "empty tax_summaries must not emit <TX>");
    // <E> still closes properly.
    assert!(xml.contains("</E>"));
}

// ─── Emission: <TX> children inside <E> with alphabetical attrs ───

#[test]
fn tax_summary_emits_tx_child_inside_e_with_alphabetical_attrs() {
    let xml = build_sell(vec![TaxGroupSummary {
        tx: 1,
        txpr: "20.00".into(),
        txsm: 2000,
        dtpr: "0.00".into(),
        dtsm: 0,
        txal: 0,
        txty: 0,
    }]);
    // Attrs alphabetical: DTPR, DTSM, TX, TXAL, TXPR, TXSM, TXTY.
    assert!(
        xml.contains(r#"<TX DTPR="0.00" DTSM="0" TX="1" TXAL="0" TXPR="20.00" TXSM="2000" TXTY="0"></TX>"#),
        "TX attrs must be alphabetical: got {xml}"
    );
    // TX is INSIDE <E>...</E>, not after.
    let e_open = xml.find("<E ").expect("E opens");
    let tx_pos = xml.find("<TX ").expect("TX present");
    let e_close = xml.find("</E>").expect("E closes");
    assert!(e_open < tx_pos && tx_pos < e_close,
        "<TX> must be a child of <E>, not a sibling: {xml}");
}

#[test]
fn multiple_tax_summaries_emit_sorted_by_stringified_tx() {
    // Python `_build_e_element:515` iterates
    // `sorted(group_sums.items())` where keys are STRINGS.  Emitter
    // must sort by `tx.to_string()` (lex) — NOT numeric — for byte
    // parity.  Caller MAY pass any order; emitter normalizes.
    let xml = build_sell(vec![
        TaxGroupSummary {
            tx: 2,
            txpr: "20.00".into(),
            txsm: 1000,
            dtpr: "0.00".into(),
            dtsm: 0,
            txal: 0,
            txty: 0,
        },
        TaxGroupSummary {
            tx: 1,
            txpr: "7.00".into(),
            txsm: 700,
            dtpr: "0.00".into(),
            dtsm: 0,
            txal: 0,
            txty: 0,
        },
    ]);
    let pos_tx1 = xml.find(r#"TX="1""#).expect("TX=1 present");
    let pos_tx2 = xml.find(r#"TX="2""#).expect("TX=2 present");
    assert!(pos_tx1 < pos_tx2, "TX=1 must precede TX=2 (sorted)");
}

#[test]
fn tax_summaries_stringified_sort_handles_dual_digit_tx() {
    // Python lex sort: "1" < "10" < "2".  Numeric sort would give
    // 1, 2, 10 — wrong.  Real tax_id range (per
    // feedback_tax_groups_real memory) includes TX up to 12, so
    // pinning this is load-bearing for piece 8 byte-equivalence.
    let xml = build_sell(vec![
        TaxGroupSummary {
            tx: 2,
            txpr: "0".into(),
            txsm: 0,
            dtpr: "0".into(),
            dtsm: 0,
            txal: 0,
            txty: 0,
        },
        TaxGroupSummary {
            tx: 10,
            txpr: "0".into(),
            txsm: 0,
            dtpr: "0".into(),
            dtsm: 0,
            txal: 0,
            txty: 0,
        },
        TaxGroupSummary {
            tx: 1,
            txpr: "0".into(),
            txsm: 0,
            dtpr: "0".into(),
            dtsm: 0,
            txal: 0,
            txty: 0,
        },
    ]);
    let pos_1 = xml.find(r#"TX="1""#).expect("TX=1 present");
    let pos_10 = xml.find(r#"TX="10""#).expect("TX=10 present");
    let pos_2 = xml.find(r#"TX="2""#).expect("TX=2 present");
    assert!(pos_1 < pos_10, "TX=1 before TX=10 (lex)");
    assert!(pos_10 < pos_2, "TX=10 before TX=2 (lex) — NOT numeric");
}
