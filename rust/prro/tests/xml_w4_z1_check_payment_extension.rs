//! W4-Z1 piece 2 — extended `xml::CheckPayment` with EPZ + RM + SMP.
//!
//! Per spec `2026-05-26-w4-z1-dps-xml-wire-shape.md` §2 `<M>` element
//! cross-checked against Python `dps_xml.py:281-307` + WebCheck
//! `StringXML.cs:663 DopTegE()`.

use prro::xml::{
    build_canonical_xml, CanonicalDoc, CheckItem, CheckPayload, CheckPayment, DocumentHeader,
};

fn dummy_header() -> DocumentHeader {
    DocumentHeader::with_defaults("4538765845", "TN-12345", 0_u32, "20260527100000", "")
}

fn build_sell_with_payment(payment: CheckPayment) -> String {
    let item = CheckItem {
        code: "ART-1".to_string(),
        name: "Test".to_string(),
        price: 1000,
        quantity: 1000,
        sum: 1000,
        ..Default::default()
    };
    let doc = CanonicalDoc::Sell(CheckPayload {
        header: dummy_header(),
        local_number: 1,
        items: vec![item],
        payments: vec![payment],
        total_sum: 1000,
    });
    let bytes = build_canonical_xml(&doc).expect("build");
    bytes.iter().map(|&b| b as char).collect()
}

#[test]
fn minimal_cash_payment_emits_required_attrs_only() {
    // Back-compat: minimal payment with no optional attrs.
    let xml = build_sell_with_payment(CheckPayment {
        name: "Готівка".to_string(),
        sum: 1000,
        type_code: "0".to_string(),
        ..Default::default()
    });
    // N, NM, SM, T — alphabetical
    assert!(xml.contains(r#"<M N="2" NM="#));
    assert!(xml.contains(r#"SM="1000""#));
    assert!(xml.contains(r#"T="0""#));
    assert!(!xml.contains(r#"PA="#));
    assert!(!xml.contains(r#"RRN="#));
    assert!(!xml.contains(r#"RM="#));
    assert!(!xml.contains(r#"SMP="#));
}

#[test]
fn cashless_payment_with_full_epz_slip_emits_all_attrs() {
    let xml = build_sell_with_payment(CheckPayment {
        name: "Visa".to_string(),
        sum: 1000,
        type_code: "1".to_string(),
        pa: Some("PrivatBank".to_string()),
        pb: Some("TERM-42".to_string()),
        pc: Some("PURCHASE".to_string()),
        pd: Some("************4242".to_string()),
        pe: Some("AUTH-12345".to_string()),
        pf: Some(50),
        psnm: Some("Visa".to_string()),
        rrn: Some("RRN-987654321".to_string()),
        ..Default::default()
    });
    assert!(xml.contains(r#"PA="PrivatBank""#));
    assert!(xml.contains(r#"PB="TERM-42""#));
    assert!(xml.contains(r#"PC="PURCHASE""#));
    assert!(xml.contains(r#"PD="************4242""#));
    assert!(xml.contains(r#"PE="AUTH-12345""#));
    assert!(xml.contains(r#"PF="50""#));
    assert!(xml.contains(r#"PSNM="Visa""#));
    assert!(xml.contains(r#"RRN="RRN-987654321""#));
}

#[test]
fn cash_payment_with_change_emits_rm_attr() {
    let xml = build_sell_with_payment(CheckPayment {
        name: "Готівка".to_string(),
        sum: 1050, // customer paid 10.50 for 10.00 item
        type_code: "0".to_string(),
        rm: Some(50), // 50 kopecks change
        ..Default::default()
    });
    assert!(xml.contains(r#"RM="50""#));
}

#[test]
fn cash_payment_with_rounding_emits_smp_attr() {
    let xml = build_sell_with_payment(CheckPayment {
        name: "Готівка".to_string(),
        sum: 1000,
        type_code: "0".to_string(),
        smp: Some(2), // 2 kopecks rounded off
        ..Default::default()
    });
    assert!(xml.contains(r#"SMP="2""#));
}

#[test]
fn payment_with_zero_commission_pf_still_emits_if_some_zero() {
    // Field is Option<i64>; Some(0) means "explicitly 0" — emit.
    // None means "not present" — omit.  Caller (W4-Z1 piece 7)
    // decides whether 0 commission should be Some(0) or None.
    let xml = build_sell_with_payment(CheckPayment {
        name: "Visa".to_string(),
        sum: 1000,
        type_code: "1".to_string(),
        pf: Some(0),
        ..Default::default()
    });
    assert!(xml.contains(r#"PF="0""#));
}

#[test]
fn payment_attrs_in_alphabetical_order() {
    // tag_attrs sorts alphabetically — verify ordering for a multi-
    // attr cashless payment (PA, PB, ..., PSNM, RRN, RM, SMP, T).
    let xml = build_sell_with_payment(CheckPayment {
        name: "Кредит".to_string(),
        sum: 1000,
        type_code: "2".to_string(),
        pa: Some("Bank".to_string()),
        pb: Some("T1".to_string()),
        rrn: Some("R1".to_string()),
        psnm: Some("S1".to_string()),
        ..Default::default()
    });
    // Find <M ...> and verify the attrs are in alpha order
    let m_start = xml.find("<M ").expect("M element");
    let m_end = xml[m_start..].find('>').expect("M close-bracket") + m_start;
    let m_attrs_str = &xml[m_start..=m_end];
    // Sequence: N < NM < PA < PB < PSNM < RRN < SM < T
    let pos_n = m_attrs_str.find(r#"N=""#).unwrap();
    let pos_nm = m_attrs_str.find(r#"NM=""#).unwrap();
    let pos_pa = m_attrs_str.find(r#"PA=""#).unwrap();
    let pos_pb = m_attrs_str.find(r#"PB=""#).unwrap();
    let pos_psnm = m_attrs_str.find(r#"PSNM=""#).unwrap();
    let pos_rrn = m_attrs_str.find(r#"RRN=""#).unwrap();
    let pos_sm = m_attrs_str.find(r#"SM=""#).unwrap();
    let pos_t = m_attrs_str.find(r#"T=""#).unwrap();
    assert!(pos_n < pos_nm);
    assert!(pos_nm < pos_pa);
    assert!(pos_pa < pos_pb);
    assert!(pos_pb < pos_psnm);
    assert!(pos_psnm < pos_rrn);
    assert!(pos_rrn < pos_sm);
    assert!(pos_sm < pos_t);
}
