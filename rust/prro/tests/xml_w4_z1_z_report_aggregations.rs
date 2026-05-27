//! W4-Z1 piece 6 — Z_REPORT extended aggregations: `<TXS>` per tax
//! group, `<IO>` service in/out, `<EPZ>` electronic-payment-instrument
//! totals.
//!
//! Per Python `dps_xml.py:_build_z_report:417-498`.  Python `<Z>`
//! ordering is fixed: TXS → M → IO → NC → EPZ.  We mirror exactly.
//! All four optional sections are back-compat: empty Vec / None →
//! no emission, preserving the existing minimal goldens.

use prro::xml::{
    build_canonical_xml, CanonicalDoc, DocumentHeader, ZReportCheckCount, ZReportEpzTotals,
    ZReportPayload, ZReportPaymentSum, ZReportServiceSum, ZReportTaxSummary,
};

fn header() -> DocumentHeader {
    DocumentHeader::with_defaults("4538765845", "TN-12345", 7_u32, "20260527100000", "")
}

fn build_z(
    tax_summaries: Vec<ZReportTaxSummary>,
    payments: Vec<ZReportPaymentSum>,
    service_sums: Vec<ZReportServiceSum>,
    epz: Option<ZReportEpzTotals>,
) -> String {
    let doc = CanonicalDoc::ZReport(ZReportPayload {
        header: header(),
        local_number: 100,
        tax_summaries,
        payments,
        service_sums,
        check_count: ZReportCheckCount {
            sell_count: 5,
            return_count: 1,
        },
        epz,
    });
    let bytes = build_canonical_xml(&doc).expect("build");
    bytes.iter().map(|&b| b as char).collect()
}

// ─── Empty/None — no new sections emitted ─────────────────────────

#[test]
fn empty_optional_sections_emit_no_txs_io_epz() {
    let xml = build_z(
        vec![],
        vec![ZReportPaymentSum {
            name: "CASH".into(),
            sum_in: 5000,
            sum_out: 0,
            type_code: "0".into(),
        }],
        vec![],
        None,
    );
    assert!(!xml.contains("<TXS "), "no TXS when tax_summaries empty");
    assert!(!xml.contains("<IO "), "no IO when service_sums empty");
    assert!(!xml.contains("<EPZ "), "no EPZ when None");
    // Existing M + NC still present.
    assert!(xml.contains("<M "));
    assert!(xml.contains("<NC "));
}

// ─── <TXS> — full form with tax_rate + algorithm ──────────────────

#[test]
fn txs_full_form_emits_all_attrs_alphabetical() {
    let xml = build_z(
        vec![ZReportTaxSummary {
            tx: 1,
            tx_short_form: false,
            txpr: "20.00".into(),
            txal: 0,
            txty: 0,
            dtpr: "0.00".into(),
            smi: 12000,
            smo: 0,
            txi: 2000,
            txo: 0,
            ts_prefix: "20260527".into(),
        }],
        vec![],
        vec![],
        None,
    );
    // Attrs alphabetical: DTPR DTSM(no) SMI SMO TS TX TXAL TXI TXO TXPR TXTY
    // Per Python `:452-456`: DTPR, SMI, SMO, TS, TX, TXAL, TXI, TXO, TXPR, TXTY.
    assert!(
        xml.contains(r#"<TXS DTPR="0.00" SMI="12000" SMO="0" TS="20260527" TX="1" TXAL="0" TXI="2000" TXO="0" TXPR="20.00" TXTY="0"></TXS>"#),
        "TXS full form must emit all 10 attrs alphabetically: got {xml}"
    );
}

#[test]
fn txs_short_form_emits_only_smi_smo_tx() {
    // Python fallback when tax_group is not found (`:458`):
    //   _tag('TXS', {'SMI': smi, 'SMO': smo, 'TX': tax_id})
    let xml = build_z(
        vec![ZReportTaxSummary {
            tx: 9,
            tx_short_form: true,
            txpr: "".into(),
            txal: 0,
            txty: 0,
            dtpr: "".into(),
            smi: 5000,
            smo: 200,
            txi: 0,
            txo: 0,
            ts_prefix: "".into(),
        }],
        vec![],
        vec![],
        None,
    );
    assert!(
        xml.contains(r#"<TXS SMI="5000" SMO="200" TX="9"></TXS>"#),
        "TXS short form: SMI/SMO/TX only: got {xml}"
    );
    // Must NOT carry the full-form attrs.
    assert!(!xml.contains(r#"TXPR="""#));
    assert!(!xml.contains(r#"TXAL="0""#));
}

// ─── <IO> — service in/out summaries ──────────────────────────────

#[test]
fn io_emits_one_element_per_service_sum_sorted_by_name() {
    let xml = build_z(
        vec![],
        vec![],
        vec![
            ZReportServiceSum {
                name: "SERVICE_OUT".into(),
                sum_in: 0,
                sum_out: 3000,
            },
            ZReportServiceSum {
                name: "SERVICE_IN".into(),
                sum_in: 5000,
                sum_out: 0,
            },
        ],
        None,
    );
    // Python iterates `sorted(service_sums.keys())` → SERVICE_IN first.
    let in_pos = xml.find(r#"NM="SERVICE_IN""#).expect("SERVICE_IN");
    let out_pos = xml.find(r#"NM="SERVICE_OUT""#).expect("SERVICE_OUT");
    assert!(in_pos < out_pos, "IO sorted alphabetically by name");
    // Per Python `:477`: NM, SMI, SMO, T="0".
    assert!(xml.contains(r#"<IO NM="SERVICE_IN" SMI="5000" SMO="0" T="0"></IO>"#));
    assert!(xml.contains(r#"<IO NM="SERVICE_OUT" SMI="0" SMO="3000" T="0"></IO>"#));
}

// ─── <EPZ> — electronic-payment-instrument totals ─────────────────

#[test]
fn epz_emits_single_element_with_three_attrs() {
    let xml = build_z(
        vec![],
        vec![],
        vec![],
        Some(ZReportEpzTotals {
            epc: 3,    // count of operations
            epcs: 2,   // count of successful operations
            epsm: 1500, // total sum (kopecks)
        }),
    );
    // Per Python `:487-491`: EPC, EPCS, EPSM (already alphabetical).
    assert!(
        xml.contains(r#"<EPZ EPC="3" EPCS="2" EPSM="1500"></EPZ>"#),
        "EPZ shape: got {xml}"
    );
}

// ─── Ordering: TXS → M → IO → NC → EPZ ────────────────────────────

#[test]
fn z_report_section_ordering_matches_python() {
    let xml = build_z(
        vec![ZReportTaxSummary {
            tx: 1,
            tx_short_form: true,
            txpr: "".into(),
            txal: 0,
            txty: 0,
            dtpr: "".into(),
            smi: 100,
            smo: 0,
            txi: 0,
            txo: 0,
            ts_prefix: "".into(),
        }],
        vec![ZReportPaymentSum {
            name: "CASH".into(),
            sum_in: 100,
            sum_out: 0,
            type_code: "0".into(),
        }],
        vec![ZReportServiceSum {
            name: "SERVICE_IN".into(),
            sum_in: 50,
            sum_out: 0,
        }],
        Some(ZReportEpzTotals {
            epc: 1,
            epcs: 1,
            epsm: 100,
        }),
    );
    let txs = xml.find("<TXS ").expect("TXS");
    let m = xml.find("<M ").expect("M");
    let io = xml.find("<IO ").expect("IO");
    let nc = xml.find("<NC ").expect("NC");
    let epz = xml.find("<EPZ ").expect("EPZ");
    assert!(txs < m, "TXS before M");
    assert!(m < io, "M before IO");
    assert!(io < nc, "IO before NC");
    assert!(nc < epz, "NC before EPZ");
}
