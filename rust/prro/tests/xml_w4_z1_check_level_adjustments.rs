//! W4-Z1 piece 3 — check-level `<D TR="1">` / `<S TR="1">` with
//! `<NI>` children naming affected items.
//!
//! Per spec §2 + Python `dps_xml.py:248-272`.

use prro::xml::{
    build_canonical_xml, AdjustmentMode, CanonicalDoc, CheckItem, CheckLevelAdjustment,
    CheckLevelAdjustmentKind, CheckPayload, CheckPayment, DocumentHeader,
};

fn header() -> DocumentHeader {
    DocumentHeader::with_defaults("4538765845", "TN-12345", 0_u32, "20260527100000", "")
}

fn build_check_with_adjustments(
    items: Vec<CheckItem>,
    adjustments: Vec<CheckLevelAdjustment>,
) -> String {
    let doc = CanonicalDoc::Sell(CheckPayload {
        header: header(),
        local_number: 1,
        items,
        payments: vec![CheckPayment {
            name: "Готівка".to_string(),
            sum: 1000,
            type_code: "0".to_string(),
            ..Default::default()
        }],
        total_sum: 1000,
        check_level_adjustments: adjustments,
        ..Default::default()
    });
    let bytes = build_canonical_xml(&doc).expect("build");
    bytes.iter().map(|&b| b as char).collect()
}

fn item(code: &str, sum: i64) -> CheckItem {
    CheckItem {
        code: code.into(),
        name: "Test".into(),
        price: sum,
        quantity: 1000,
        sum,
        ..Default::default()
    }
}

#[test]
fn check_with_no_adjustments_emits_no_check_level_d_or_s() {
    // Back-compat: empty Vec → no <D TR="1"> / <S TR="1"> emitted.
    let xml = build_check_with_adjustments(vec![item("ART-1", 1000)], vec![]);
    // Sanity: there's NO <D> or <S> at all (no per-item adj either).
    assert!(!xml.contains("<D "));
    assert!(!xml.contains("<S "));
}

#[test]
fn check_level_value_discount_emits_d_tr_1_with_ni_children() {
    let xml = build_check_with_adjustments(
        vec![item("ART-1", 500), item("ART-2", 500)],
        vec![CheckLevelAdjustment {
            kind: CheckLevelAdjustmentKind::Discount,
            sum: 50,
            mode: AdjustmentMode::Value,
            percent: None,
            name: Some("Знижка на всі товари".to_string()),
            applies_to_item_ns: vec![1, 2],
        }],
    );
    // <D N="3" SM="50" TR="1" TY="0" NM="..."> ... <NI NI="1"></NI><NI NI="2"></NI></D>
    assert!(xml.contains(r#"<D "#));
    assert!(xml.contains(r#"TR="1""#));
    assert!(xml.contains(r#"TY="0""#));
    assert!(xml.contains(r#"SM="50""#));
    assert!(xml.contains(r#"<NI NI="1"></NI>"#));
    assert!(xml.contains(r#"<NI NI="2"></NI>"#));
    // NI children must be INSIDE <D>...</D>, not after it.
    let d_open = xml.find("<D ").unwrap();
    let d_close = xml.find("</D>").unwrap();
    let ni_pos = xml.find(r#"<NI NI="1""#).unwrap();
    assert!(
        d_open < ni_pos && ni_pos < d_close,
        "NI must be a child of D, not a sibling: {xml}"
    );
}

#[test]
fn check_level_percent_discount_emits_pr_attr() {
    let xml = build_check_with_adjustments(
        vec![item("ART-1", 1000)],
        vec![CheckLevelAdjustment {
            kind: CheckLevelAdjustmentKind::Discount,
            sum: 100, // pre-resolved 10% of 1000
            mode: AdjustmentMode::Percent,
            percent: Some("10.00".to_string()),
            name: None,
            applies_to_item_ns: vec![1],
        }],
    );
    assert!(xml.contains(r#"TY="1""#));
    assert!(xml.contains(r#"PR="10.00""#));
}

#[test]
fn check_level_surcharge_emits_s_tr_1() {
    let xml = build_check_with_adjustments(
        vec![item("ART-1", 1000)],
        vec![CheckLevelAdjustment {
            kind: CheckLevelAdjustmentKind::Surcharge,
            sum: 50,
            mode: AdjustmentMode::Value,
            percent: None,
            name: Some("Націнка за доставку".to_string()),
            applies_to_item_ns: vec![1],
        }],
    );
    assert!(xml.contains(r#"<S "#));
    assert!(xml.contains(r#"TR="1""#));
    assert!(xml.contains(r#"</S>"#));
}

#[test]
fn check_level_adjustment_emits_before_payments() {
    // Per Python `dps_xml.py:248-272` ordering: check-level adjustments
    // come AFTER all <P> + per-item adjustments, BEFORE <M> payments.
    let xml = build_check_with_adjustments(
        vec![item("ART-1", 1000)],
        vec![CheckLevelAdjustment {
            kind: CheckLevelAdjustmentKind::Discount,
            sum: 50,
            mode: AdjustmentMode::Value,
            percent: None,
            name: None,
            applies_to_item_ns: vec![1],
        }],
    );
    let d_pos = xml.find("<D ").expect("check-level D present");
    let m_pos = xml.find("<M ").expect("payment present");
    assert!(d_pos < m_pos, "check-level <D> must precede <M>");
}
