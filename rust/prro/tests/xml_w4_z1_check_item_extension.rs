//! W4-Z1 piece 1 — extended `xml::CheckItem` payload struct.
//!
//! Per spec `2026-05-26-w4-z1-dps-xml-wire-shape.md` §2 `<P>` per-item
//! attributes table + cross-checked against Python `dps_xml.py:184-209`.
//!
//! Coverage:
//!   * Minimal subset still works (back-compat) — no CD/CZD/TX/TX1
//!     emitted, no `<CA>` children.
//!   * Each optional attribute emits when populated.
//!   * `excise_stamps` produces `<CA CA='...'></CA>` children.
//!   * Per-item `discount` emits sibling `<D>` after `<P>`, with `NI=`
//!     pointing at the parent's `N`.
//!   * `tax_group_1 = Some(-1)` (не об'єкт ПДВ) emits literal `TX="-1"`.

use prro::xml::{
    build_canonical_xml, AdjustmentMode, CanonicalDoc, CheckItem, CheckPayload,
    CheckPayment, DocumentHeader, LineAdjustment,
};

fn dummy_header() -> DocumentHeader {
    DocumentHeader::with_defaults(
        "4538765845",
        "TN-12345",
        0_u32,
        "20260527100000",
        "",
    )
}

fn minimal_check(items: Vec<CheckItem>, payments: Vec<CheckPayment>) -> CheckPayload {
    CheckPayload {
        header: dummy_header(),
        local_number: 1,
        items,
        payments,
        total_sum: 0,
        ..Default::default()
    }
}

fn build_sell(items: Vec<CheckItem>, payments: Vec<CheckPayment>) -> String {
    let bytes = build_canonical_xml(&CanonicalDoc::Sell(minimal_check(items, payments)))
        .expect("build");
    // For ASCII attr assertions, cp1251 ≈ ASCII slice — convert via
    // cp1251 → utf-8 round-trip via a permissive lossy decode.
    bytes.iter().map(|&b| b as char).collect()
}

#[test]
fn minimal_check_item_still_emits_required_attrs_only() {
    // Back-compat: a CheckItem with only required fields populated
    // produces `<P C N NM PRC Q SM>` with NO optional attrs.
    let item = CheckItem {
        code: "ART-1".to_string(),
        name: "Test".to_string(),
        price: 100,
        quantity: 1000,
        sum: 100,
        ..Default::default()
    };
    let xml = build_sell(vec![item], vec![]);
    assert!(xml.contains(r#"<P C="ART-1" N="1" NM="Test" PRC="100" Q="1000" SM="100">"#));
    assert!(!xml.contains(r#"CD="#));
    assert!(!xml.contains(r#"CZD="#));
    assert!(!xml.contains(r#"TX="#));
    assert!(!xml.contains(r#"<CA"#));
}

#[test]
fn check_item_with_barcode_emits_cd_attr() {
    let item = CheckItem {
        code: "ART-1".to_string(),
        name: "Test".to_string(),
        price: 100,
        quantity: 1000,
        sum: 100,
        barcode: Some("4820000123456".to_string()),
        ..Default::default()
    };
    let xml = build_sell(vec![item], vec![]);
    assert!(xml.contains(r#"CD="4820000123456""#));
}

#[test]
fn check_item_with_uktzed_emits_czd_attr() {
    let item = CheckItem {
        code: "ART-1".to_string(),
        name: "Test".to_string(),
        price: 100,
        quantity: 1000,
        sum: 100,
        uktzed: Some("1905909000".to_string()),
        ..Default::default()
    };
    let xml = build_sell(vec![item], vec![]);
    assert!(xml.contains(r#"CZD="1905909000""#));
}

#[test]
fn check_item_with_tax_group_1_emits_tx_attr() {
    let item = CheckItem {
        code: "ART-1".to_string(),
        name: "Test".to_string(),
        price: 100,
        quantity: 1000,
        sum: 100,
        tax_group_1: Some(1),
        ..Default::default()
    };
    let xml = build_sell(vec![item], vec![]);
    assert!(xml.contains(r#"TX="1""#));
}

#[test]
fn check_item_with_dual_tax_emits_tx1_attr() {
    let item = CheckItem {
        code: "ART-1".to_string(),
        name: "Test".to_string(),
        price: 100,
        quantity: 1000,
        sum: 100,
        tax_group_1: Some(1),
        tax_group_2: Some(4),
        ..Default::default()
    };
    let xml = build_sell(vec![item], vec![]);
    assert!(xml.contains(r#"TX="1""#));
    assert!(xml.contains(r#"TX1="4""#));
}

/// Per `feedback_pdv_zero` memory: TX="-1" (не об'єкт ПДВ) is valid.
/// Pin that the i64 signed encoding makes it through to the wire.
#[test]
fn check_item_with_pdv_not_object_emits_tx_negative_one() {
    let item = CheckItem {
        code: "ART-1".to_string(),
        name: "Test".to_string(),
        price: 100,
        quantity: 1000,
        sum: 100,
        tax_group_1: Some(-1),
        ..Default::default()
    };
    let xml = build_sell(vec![item], vec![]);
    assert!(
        xml.contains(r#"TX="-1""#),
        "TX=\"-1\" must round-trip for PDV-not-object items; got: {xml}"
    );
}

#[test]
fn check_item_with_excise_stamps_emits_ca_children() {
    let item = CheckItem {
        code: "ART-1".to_string(),
        name: "Test".to_string(),
        price: 100,
        quantity: 1000,
        sum: 100,
        excise_stamps: vec![
            "UA1234567890".to_string(),
            "UA0987654321".to_string(),
        ],
        ..Default::default()
    };
    let xml = build_sell(vec![item], vec![]);
    assert!(xml.contains(r#"<CA CA="UA1234567890"></CA>"#));
    assert!(xml.contains(r#"<CA CA="UA0987654321"></CA>"#));
    // The stamps live INSIDE the <P> element.
    let p_open = xml.find("<P ").expect("P element present");
    let p_close = xml.find("</P>").expect("P close present");
    let stamp_pos = xml.find(r#"<CA CA="UA1234567890""#).unwrap();
    assert!(
        p_open < stamp_pos && stamp_pos < p_close,
        "stamps must be CHILDREN of <P>, not siblings"
    );
}

#[test]
fn check_item_with_per_item_discount_emits_d_sibling_with_ni() {
    let item = CheckItem {
        code: "ART-1".to_string(),
        name: "Test".to_string(),
        price: 1000,
        quantity: 1000,
        sum: 1000,
        discount: Some(LineAdjustment {
            sum: 100,
            mode: AdjustmentMode::Value,
            percent: None,
            name: Some("Знижка постійному клієнту".to_string()),
            privilege: None,
            tax_code: None,
        }),
        ..Default::default()
    };
    let xml = build_sell(vec![item], vec![]);
    // The discount's NI must reference the parent P's N=1.  The
    // discount itself gets N=2 (item_no after the <P>).
    assert!(xml.contains(r#"<D "#));
    assert!(xml.contains(r#"NI="1""#));
    assert!(xml.contains(r#"TR="0""#));
    assert!(xml.contains(r#"TY="0""#));
    assert!(xml.contains(r#"SM="100""#));
}

#[test]
fn check_item_with_per_item_percent_discount_emits_pr_attr() {
    let item = CheckItem {
        code: "ART-1".to_string(),
        name: "Test".to_string(),
        price: 1000,
        quantity: 1000,
        sum: 1000,
        discount: Some(LineAdjustment {
            sum: 100, // pre-resolved kopecks for 10% of 1000
            mode: AdjustmentMode::Percent,
            percent: Some("10.00".to_string()),
            name: None,
            privilege: None,
            tax_code: None,
        }),
        ..Default::default()
    };
    let xml = build_sell(vec![item], vec![]);
    assert!(xml.contains(r#"TY="1""#));
    assert!(xml.contains(r#"PR="10.00""#));
}

#[test]
fn check_item_with_per_item_surcharge_emits_s_sibling() {
    let item = CheckItem {
        code: "ART-1".to_string(),
        name: "Test".to_string(),
        price: 1000,
        quantity: 1000,
        sum: 1000,
        surcharge: Some(LineAdjustment {
            sum: 50,
            mode: AdjustmentMode::Value,
            percent: None,
            name: None,
            privilege: None,
            tax_code: None,
        }),
        ..Default::default()
    };
    let xml = build_sell(vec![item], vec![]);
    assert!(xml.contains(r#"<S "#));
    assert!(xml.contains(r#"NI="1""#));
    assert!(xml.contains(r#"SM="50""#));
}
