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
    build_canonical_xml, calc_tax, CanonicalDoc, CheckItem, CheckPayload, CheckPayment,
    DocumentHeader, TaxGroupSummary,
};

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
    let (txsm, dtsm) = calc_tax(12000, 20.0, 0.0, 0);
    assert_eq!(txsm, 2000, "VAT-included 20% on 12000 kop = 2000 kop");
    assert_eq!(dtsm, 0, "TXAL=0 always emits dtsm=0");
}

#[test]
fn calc_tax_txal_0_with_zero_rate() {
    let (txsm, dtsm) = calc_tax(10000, 0.0, 0.0, 0);
    assert_eq!(txsm, 0);
    assert_eq!(dtsm, 0);
}

// ─── calc_tax: TXAL=1 (excise pre-VAT) ────────────────────────────

#[test]
fn calc_tax_txal_1_excise_pre_vat() {
    // group=10000, dtpr=5%, txpr=20%
    //   dtsm = 10000 * 5 / 100 = 500
    //   txsm = (10000 + 500) * 20 / 120 = 10500 * 20 / 120 = 1750
    let (txsm, dtsm) = calc_tax(10000, 20.0, 5.0, 1);
    assert_eq!(dtsm, 500, "TXAL=1 dtsm = group * dtpr / 100");
    assert_eq!(txsm, 1750, "TXAL=1 txsm = (group + dtsm) * txpr / (100 + txpr)");
}

// ─── calc_tax: TXAL=2 (excise post-VAT) ───────────────────────────

#[test]
fn calc_tax_txal_2_excise_post_vat() {
    // group=10500, dtpr=5%, txpr=20%
    //   dtsm = 10500 * 5 / 105 = 500
    //   txsm = (10500 - 500) * 20 / 120 = 10000 * 20 / 120 = 1666.67 → 1667 (banker's: .67 rounds up)
    let (txsm, dtsm) = calc_tax(10500, 20.0, 5.0, 2);
    assert_eq!(dtsm, 500, "TXAL=2 dtsm = group * dtpr / (100 + dtpr)");
    // round-half-to-even: 1666.666... → 1667 (not a .5 boundary)
    assert_eq!(txsm, 1667);
}

// ─── calc_tax: TXAL=3 — operator-deferred ─────────────────────────

#[test]
fn calc_tax_txal_3_returns_zero_per_operator_deferral() {
    // Operator-confirmed: TXAL=3 "не потрібен" — caller should
    // audit_log warn if hit.  We return (0,0) for forward-compat.
    let (txsm, dtsm) = calc_tax(10000, 20.0, 5.0, 3);
    assert_eq!(txsm, 0);
    assert_eq!(dtsm, 0);
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
fn multiple_tax_summaries_emit_in_caller_order() {
    // Caller is responsible for ordering (typically by TX number);
    // we do NOT sort.
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
    let pos_tx2 = xml.find(r#"TX="2""#).expect("TX=2 present");
    let pos_tx1 = xml.find(r#"TX="1""#).expect("TX=1 present");
    assert!(pos_tx2 < pos_tx1, "caller ordering must be preserved (TX=2 before TX=1)");
}
