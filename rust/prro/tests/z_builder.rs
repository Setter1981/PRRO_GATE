//! RS-3 A1Z — pure-builder coverage for `build_z_canonical` (the D5
//! dual-hash Z-class command builder). Quiescence + stage_acquire/boot
//! adapt are covered by their own (DB-bound) suites.

use prro::db::models::enums::{DocType, Protocol};
use prro::db::models::ids::DriverId;
use prro::db::repositories::ingress_inbox::InboxRow;
use prro::runtime::ingress::canonical_builder::BuildReject;
use prro::runtime::ingress::convert::ConvertedPayload;
use prro::runtime::ingress::z_builder::build_z_canonical;
use sha2::{Digest, Sha256};

const FN: &str = "1234567890";
const WIRE_PAYLOAD: &str = r#"{"command":"Z_REPORT"}"#;
const AGG_PAYLOAD: &str = r#"{"turnover":{"sell_count":2,"groups":[]}}"#;

fn sha256(s: &str) -> [u8; 32] {
    Sha256::digest(s.as_bytes()).into()
}

/// A Z inbox row carrying the WIRE intent + its (valid) wire hash.
fn z_row(op: &str) -> InboxRow {
    InboxRow {
        request_id: [0xCD; 16],
        fiscal_number: FN.to_string(),
        protocol: Protocol::Rest,
        operation_type: op.to_string(),
        idempotency_key: "z-idem".to_string(),
        status: "PROCESSING".to_string(),
        payload_json: WIRE_PAYLOAD.to_string(),
        payload_sha256_canonical: sha256(WIRE_PAYLOAD),
        correlation_id: None,
        received_at: "2026-06-08T20:00:00Z".to_string(),
        signed_by_cashier_id: Some("cashier-z".to_string()),
        driver_id: Some("maria304".to_string()),
        business_ts: Some("2026-06-08T19:59:00Z".to_string()),
        total_sum_kop: None,
    }
}

fn aggregated() -> ConvertedPayload {
    ConvertedPayload {
        payload_json: AGG_PAYLOAD.to_string(),
        payload_sha256_canonical: sha256(AGG_PAYLOAD),
    }
}

#[test]
fn builds_z_report_with_divergent_dual_hash() {
    let cmd = build_z_canonical(&z_row("Z_REPORT"), &aggregated()).expect("Z_REPORT builds");

    assert_eq!(cmd.doc_type, DocType::ZReport);
    // payload = AGGREGATED body; canonical hash = hash(aggregated).
    assert_eq!(cmd.payload_json, AGG_PAYLOAD);
    assert_eq!(cmd.payload_sha256_canonical, sha256(AGG_PAYLOAD));
    // source = WIRE hash. THE D5 POINT: source and canonical DIVERGE for Z.
    assert_eq!(cmd.source_sha256, sha256(WIRE_PAYLOAD));
    assert_ne!(
        cmd.source_sha256, cmd.payload_sha256_canonical,
        "Z must have a divergent source vs canonical hash (D5)"
    );
    assert_eq!(cmd.total_sum_kop, None);
    assert_eq!(cmd.driver_id, Some(DriverId::new("maria304").unwrap()));
}

#[test]
fn builds_shift_close_as_z_doc_type() {
    let cmd = build_z_canonical(&z_row("SHIFT_CLOSE"), &aggregated()).expect("SHIFT_CLOSE builds");
    assert_eq!(cmd.doc_type, DocType::ShiftClose);
    assert_eq!(cmd.payload_json, AGG_PAYLOAD);
    assert_ne!(cmd.source_sha256, cmd.payload_sha256_canonical);
}

#[test]
fn rejects_wire_source_hash_mismatch() {
    // A tampered WIRE row (payload_json changed, wire hash not) is rejected —
    // the source integrity gate, independent of the aggregated body.
    let mut row = z_row("Z_REPORT");
    row.payload_sha256_canonical = [0xFF; 32];
    assert_eq!(
        build_z_canonical(&row, &aggregated()).unwrap_err(),
        BuildReject::PayloadHashMismatch
    );
}

#[test]
fn rejects_missing_driver_id() {
    let mut row = z_row("Z_REPORT");
    row.driver_id = None;
    assert_eq!(
        build_z_canonical(&row, &aggregated()).unwrap_err(),
        BuildReject::MissingDriverId
    );
}

#[test]
fn rejects_blank_business_ts() {
    let mut row = z_row("Z_REPORT");
    row.business_ts = Some("  ".to_string());
    assert_eq!(
        build_z_canonical(&row, &aggregated()).unwrap_err(),
        BuildReject::MissingBusinessTs
    );
}

#[test]
fn rejects_non_z_operation() {
    // build_z_canonical is the Z path; a non-Z op here is a dispatch bug.
    let mut row = z_row("SELL");
    row.operation_type = "SELL".to_string();
    assert!(matches!(
        build_z_canonical(&row, &aggregated()).unwrap_err(),
        BuildReject::UnsupportedOperation { .. }
    ));
}
