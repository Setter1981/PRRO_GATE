//! W4-Z1 piece 4 — `<L N=... NM=...>` header/footer text lines.
//!
//! Per spec §2 + Python `dps_xml.py:172-178` (header) + `:181-182,
//! 311-313` (footer).  Operator-confirmed pilot need ("друк
//! нефіскальних строк теж потрібен").

use prro::xml::{
    build_canonical_xml, CanonicalDoc, CheckItem, CheckPayload, CheckPayment, DocumentHeader,
};

fn header() -> DocumentHeader {
    DocumentHeader::with_defaults("4538765845", "TN-12345", 0_u32, "20260527100000", "")
}

fn build_sell(
    header_lines: Vec<String>,
    items: Vec<CheckItem>,
    footer_lines: Vec<String>,
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
        header_lines,
        footer_lines,
        ..Default::default()
    });
    let bytes = build_canonical_xml(&doc).expect("build");
    bytes.iter().map(|&b| b as char).collect()
}

fn item() -> CheckItem {
    CheckItem {
        code: "ART-1".into(),
        name: "Test".into(),
        price: 1000,
        quantity: 1000,
        sum: 1000,
        ..Default::default()
    }
}

#[test]
fn check_with_no_text_lines_emits_no_l_elements() {
    let xml = build_sell(vec![], vec![item()], vec![]);
    assert!(!xml.contains("<L "));
}

#[test]
fn check_with_header_lines_emits_l_before_items() {
    let xml = build_sell(
        vec![
            "ТОВ Магазинчик".to_string(),
            "м. Київ, вул. Дніпровська 33".to_string(),
        ],
        vec![item()],
        vec![],
    );
    // Each header line → <L N=... NM="...">
    // Header lines come BEFORE first <P>.
    let l1_pos = xml.find(r#"<L "#).expect("first L");
    let p_pos = xml.find("<P ").expect("P present");
    assert!(l1_pos < p_pos, "header <L> must precede <P>");
    // Both header lines emitted
    assert!(xml.matches("<L ").count() == 2, "2 header L emitted");
}

#[test]
fn check_with_footer_lines_emits_l_after_payments_before_e() {
    let xml = build_sell(
        vec![],
        vec![item()],
        vec!["Дякуємо за покупку!".to_string()],
    );
    let m_pos = xml.find("<M ").expect("M present");
    let l_pos = xml.find("<L ").expect("L present");
    let e_pos = xml.find("<E ").expect("E present");
    assert!(m_pos < l_pos, "footer <L> must follow <M>");
    assert!(l_pos < e_pos, "footer <L> must precede <E>");
}

#[test]
fn header_and_footer_share_continuous_n_numbering_with_items() {
    // Per Python: single item_no counter increments across L/P/M/L.
    let xml = build_sell(
        vec!["Header1".to_string()],
        vec![item()],
        vec!["Footer1".to_string()],
    );
    // N=1 → header L, N=2 → P item, N=3 → M payment, N=4 → footer L
    assert!(xml.contains(r#"<L N="1" NM="Header1""#));
    assert!(xml.contains(r#"<P C="ART-1" N="2""#));
    assert!(xml.contains(r#"<M N="3""#));
    assert!(xml.contains(r#"<L N="4" NM="Footer1""#));
}

#[test]
fn empty_and_whitespace_lines_are_skipped_and_do_not_consume_n() {
    // Per Python `:173-178`: `line = line.strip(); if line: emit; n += 1`.
    // Empty + whitespace-only lines MUST be skipped AND must NOT
    // advance the item_no counter — otherwise per-piece counter
    // drift breaks every subsequent <P>/<M>/<L> N attribute.
    let xml = build_sell(
        vec!["".to_string(), "  ".to_string(), "Header1".to_string()],
        vec![item()],
        vec!["".to_string(), "\t".to_string(), "Footer1".to_string()],
    );
    // Only ONE header L emitted (Header1 at N=1).
    assert!(
        xml.contains(r#"<L N="1" NM="Header1""#),
        "header empties skipped, Header1 at N=1: {xml}"
    );
    // P item gets N=2 (NOT N=4 — empties did not consume slots).
    assert!(
        xml.contains(r#"<P C="ART-1" N="2""#),
        "P item must be N=2 after empty-skip: {xml}"
    );
    // M at N=3.
    assert!(xml.contains(r#"<M N="3""#), "payment at N=3: {xml}");
    // Footer Footer1 at N=4.
    assert!(
        xml.contains(r#"<L N="4" NM="Footer1""#),
        "footer empties skipped, Footer1 at N=4: {xml}"
    );
    // Sanity: no empty NM="" attribute emitted.
    assert!(!xml.contains(r#"NM="""#), "no empty NM emitted");
    assert!(!xml.contains(r#"NM="  ""#), "no whitespace-only NM emitted");
}

#[test]
fn cyrillic_text_lines_emit_through_cp1251() {
    // Verify cp1251 encoding works on Ukrainian text.
    let xml = build_sell(
        vec!["Дякуємо!".to_string()],
        vec![item()],
        vec!["До побачення".to_string()],
    );
    // Output is cp1251 bytes (we treat them as raw chars in the
    // String above — Unicode escaping the cp1251 → utf-8 here would
    // distort, but the SHAPE assertion still works: `NM="..."` must
    // be present and bracketed by `<L N=...`.
    assert!(xml.contains(r#"<L N="1" NM=""#));
    assert!(xml.contains(r#"</L>"#));
}
