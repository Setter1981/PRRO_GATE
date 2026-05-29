//! AUDIT2-IMP-1 — Python `_build_check` ordering: items_xml +
//! discounts_xml + payments_xml are SEPARATE buffers concatenated in
//! order, NOT interleaved.  Wire order:
//!   header_L | P1 P2 ... | D/S(per-item) D/S(check-level) | M | footer_L | E
//!
//! Per `dps_xml.py:155-159, :209, :244, :271, :321-324`.
//!
//! This pins the buffer-then-concat semantics so a future contributor
//! cannot revert to "emit D immediately after parent P" (which Rust
//! pre-fix did).

use prro::xml::{
    build_canonical_xml, AdjustmentMode, CanonicalDoc, CheckItem, CheckPayload, CheckPayment,
    DocumentHeader, LineAdjustment, LineAdjustmentKind,
};

fn header() -> DocumentHeader {
    DocumentHeader::with_defaults("4538765845", "TN-12345", 0_u32, "20260527100000", "")
}

fn build_sell_two_items_with_disc_on_first() -> String {
    let doc = CanonicalDoc::Sell(CheckPayload {
        header: header(),
        local_number: 1,
        items: vec![
            CheckItem {
                code: "ART-1".into(),
                name: "Item1".into(),
                price: 1000,
                quantity: 1000,
                sum: 1000,
                adjustments: vec![LineAdjustment {
                    kind: LineAdjustmentKind::Discount,
                    sum: 50,
                    mode: AdjustmentMode::Value,
                    percent: None,
                    name: None,
                    privilege: None,
                    tax_code: None,
                }],
                ..Default::default()
            },
            CheckItem {
                code: "ART-2".into(),
                name: "Item2".into(),
                price: 2000,
                quantity: 1000,
                sum: 2000,
                ..Default::default()
            },
        ],
        payments: vec![CheckPayment {
            name: "Готівка".into(),
            sum: 2950,
            type_code: "0".into(),
            ..Default::default()
        }],
        total_sum: 2950,
        ..Default::default()
    });
    let bytes = build_canonical_xml(&doc).expect("build");
    bytes.iter().map(|&b| b as char).collect()
}

#[test]
fn p_elements_emit_before_d_siblings_python_buffer_order() {
    // Python: items_xml = "P1, P3"; discounts_xml = "D2-NI2"
    //   (item_no inline: H=0(skip), P1@N=1 ic_no→2, D@N=2 NI=1 ic→3,
    //    P3@N=3 ic→4)  Actually: after fix, P1 at N=1, P2 at N=3
    //   (D consumed N=2 in counter even though emitted later).
    //   Wire: P(N=1), P(N=3), D(N=2,NI=1), M(N=4)
    let xml = build_sell_two_items_with_disc_on_first();
    let p1 = xml.find(r#"<P C="ART-1""#).expect("P1 present");
    let p2 = xml.find(r#"<P C="ART-2""#).expect("P2 present");
    let d = xml.find("<D ").expect("D present");
    let m = xml.find("<M ").expect("M present");
    assert!(p1 < p2, "P1 before P2 (items_xml accumulation order)");
    assert!(p2 < d,
        "Per-item D MUST come AFTER all <P> elements (Python items_xml + discounts_xml concat): wire = {xml}");
    assert!(d < m, "D before M (discounts_xml before payments_xml)");
}

#[test]
fn per_item_d_carries_n_value_from_inline_counter_increment() {
    // Python item_no inline-increments WHILE building inner-loop
    // discounts.  So the D's N attr is in counter-order even though
    // its position in the wire is later.
    //
    // Trace:  H=0 (no header), item_no starts at 1.
    //   Item1: P attr N=1, counter→2
    //     D for item1: attr N=2, NI=1, counter→3
    //   Item2: P attr N=3, counter→4
    //   M payment: attr N=4, counter→5
    //
    // Wire: P(N=1) P(N=3) D(N=2 NI=1) M(N=4)
    let xml = build_sell_two_items_with_disc_on_first();
    assert!(xml.contains(r#"<P C="ART-1" N="1""#), "ART-1 at N=1: {xml}");
    assert!(
        xml.contains(r#"<P C="ART-2" N="3""#),
        "ART-2 at N=3 (D consumed N=2): {xml}"
    );
    assert!(
        xml.contains(r#"N="2" NI="1""#),
        "D carries N=2 NI=1 (inline-incremented): {xml}"
    );
    assert!(xml.contains(r#"<M N="4""#), "M at N=4: {xml}");
}

#[test]
fn adjustments_vec_preserves_input_order_d_or_s_mixed() {
    // Python iterates `g.get('discounts') or []` preserving order.
    // Rust uses adjustments: Vec<LineAdjustment> with .kind picking
    // <D> vs <S>.  Input order [Surcharge, Discount] MUST emit
    // <S> before <D>.  (Pre-fix: discount: Option + surcharge:
    // Option always emitted D before S.)
    let doc = CanonicalDoc::Sell(CheckPayload {
        header: header(),
        local_number: 1,
        items: vec![CheckItem {
            code: "ART-1".into(),
            name: "Item1".into(),
            price: 1000,
            quantity: 1000,
            sum: 1000,
            adjustments: vec![
                LineAdjustment {
                    kind: LineAdjustmentKind::Surcharge,
                    sum: 100,
                    mode: AdjustmentMode::Value,
                    percent: None,
                    name: None,
                    privilege: None,
                    tax_code: None,
                },
                LineAdjustment {
                    kind: LineAdjustmentKind::Discount,
                    sum: 50,
                    mode: AdjustmentMode::Value,
                    percent: None,
                    name: None,
                    privilege: None,
                    tax_code: None,
                },
            ],
            ..Default::default()
        }],
        payments: vec![CheckPayment {
            name: "Готівка".into(),
            sum: 1050,
            type_code: "0".into(),
            ..Default::default()
        }],
        total_sum: 1050,
        ..Default::default()
    });
    let bytes = build_canonical_xml(&doc).expect("build");
    let xml: String = bytes.iter().map(|&b| b as char).collect();
    let s_pos = xml.find("<S ").expect("S present");
    let d_pos = xml.find("<D ").expect("D present");
    assert!(
        s_pos < d_pos,
        "S (Surcharge) MUST emit before D (Discount) when listed first: {xml}"
    );
}

#[test]
fn cross_piece_n_counter_full_pipeline() {
    // AUDIT2-MIN-2 (B): no integration test pins L→P→D→L→M→L sequence
    // counter.  Add ONE end-to-end test that exercises all 5 loops
    // sharing the item_no counter.
    //
    // Inputs:
    //   header lines: ["H1"]
    //   items: [g1 (no disc), g2 (with value disc 50)]
    //   check-level: 1 disc applies to [g1.N, g2.N]
    //   payments: [CASH]
    //   footer lines: ["F1"]
    //
    // Trace:
    //   H1 at N=1, counter→2
    //   P g1 at N=2, counter→3
    //   P g2 at N=3, counter→4
    //     D for g2 at N=4 NI=3, counter→5
    //   Check-level D at N=5, counter→6
    //   M payment at N=6, counter→7
    //   F1 at N=7, counter→8
    use prro::xml::{CheckLevelAdjustment, CheckLevelAdjustmentKind};
    let doc = CanonicalDoc::Sell(CheckPayload {
        header: header(),
        local_number: 1,
        items: vec![
            CheckItem {
                code: "ART-1".into(),
                name: "G1".into(),
                price: 1000,
                quantity: 1000,
                sum: 1000,
                ..Default::default()
            },
            CheckItem {
                code: "ART-2".into(),
                name: "G2".into(),
                price: 2000,
                quantity: 1000,
                sum: 2000,
                adjustments: vec![LineAdjustment {
                    kind: LineAdjustmentKind::Discount,
                    sum: 50,
                    mode: AdjustmentMode::Value,
                    percent: None,
                    name: None,
                    privilege: None,
                    tax_code: None,
                }],
                ..Default::default()
            },
        ],
        payments: vec![CheckPayment {
            name: "CASH".into(),
            sum: 2950,
            type_code: "0".into(),
            ..Default::default()
        }],
        total_sum: 2950,
        header_lines: vec!["H1".into()],
        footer_lines: vec!["F1".into()],
        check_level_adjustments: vec![CheckLevelAdjustment {
            kind: CheckLevelAdjustmentKind::Discount,
            sum: 100,
            mode: AdjustmentMode::Value,
            percent: None,
            name: None,
            applies_to_item_ns: vec![2, 3],
        }],
        ..Default::default()
    });
    let bytes = build_canonical_xml(&doc).expect("build");
    let xml: String = bytes.iter().map(|&b| b as char).collect();

    // N values (string-match each unique attr fragment):
    assert!(xml.contains(r#"<L N="1" NM="H1""#), "H1 at N=1: {xml}");
    assert!(xml.contains(r#"<P C="ART-1" N="2""#), "ART-1 at N=2: {xml}");
    assert!(xml.contains(r#"<P C="ART-2" N="3""#), "ART-2 at N=3: {xml}");
    // Per-item D for g2 at N=4 NI=3 (TR=0 implicit by per-item form).
    assert!(
        xml.contains(r#"N="4" NI="3""#),
        "per-item D N=4 NI=3: {xml}"
    );
    // Check-level D at N=5 (TR=1).
    assert!(xml.contains(r#"<D N="5""#), "check-level D at N=5: {xml}");
    assert!(xml.contains(r#"<M N="6""#), "M at N=6: {xml}");
    assert!(xml.contains(r#"<L N="7" NM="F1""#), "F1 at N=7: {xml}");
}
