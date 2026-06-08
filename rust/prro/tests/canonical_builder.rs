//! RS-3 A1 — coverage for the InboxRow → CanonicalFiscalCommand builder
//! (`runtime::ingress::canonical_builder`).
//!
//! - the happy-path map uses a NON-IDENTITY fixture (every field a distinct
//!   value) so a field-swap bug is caught, not masked by coincidence;
//! - the null-contract reject matrix (missing driver_id / business_ts /
//!   SELL-no-total, Z misroute, unknown op, malformed id);
//! - terminalize_rejected_tx flips the inbox to REJECTED + audits +
//!   creates NO fiscal_documents row (reaper-safe).

use prro::db::models::enums::DocType;
use prro::db::models::enums::Protocol;
use prro::db::models::ids::{CashierId, DriverId};
use prro::db::open_pool;
use prro::db::repositories::ingress_inbox::InboxRow;
use prro::db::tx::with_immediate;
use prro::runtime::ingress::canonical_builder::{
    build_canonical, terminalize_rejected_tx, BuildReject,
};
use sha2::{Digest, Sha256};

const FN: &str = "1234567890";
const SELL_PAYLOAD: &str = r#"{"items":[{"name":"X","sum_kop":2500}]}"#;

/// sha256 over the payload bytes — reproduces the ingest hashing so the
/// row's `payload_sha256_canonical` is a VALID hash of its `payload_json`
/// (build_canonical now verifies this, RS-3 A1 review HIGH-2).
fn sha256(s: &str) -> [u8; 32] {
    Sha256::digest(s.as_bytes()).into()
}

/// A fully-populated non-Z SELL row with DISTINCT field values + a valid
/// payload hash.
fn sell_row() -> InboxRow {
    InboxRow {
        request_id: [0xAB; 16],
        fiscal_number: FN.to_string(),
        protocol: Protocol::Rest,
        operation_type: "SELL".to_string(),
        idempotency_key: "idem-key-7".to_string(),
        status: "PROCESSING".to_string(),
        payload_json: SELL_PAYLOAD.to_string(),
        payload_sha256_canonical: sha256(SELL_PAYLOAD),
        correlation_id: Some("corr-99".to_string()),
        received_at: "2026-06-08T09:00:00Z".to_string(),
        signed_by_cashier_id: Some("cashier-vasya".to_string()),
        driver_id: Some("maria304".to_string()),
        business_ts: Some("2026-06-08T08:59:30Z".to_string()),
        total_sum_kop: Some(2500),
    }
}

#[test]
fn builds_sell_from_non_identity_row() {
    let row = sell_row();
    let cmd = build_canonical(&row).expect("a fully-populated SELL row must build");

    // Every field maps to its OWN source — no coincidental equality.
    assert_eq!(cmd.doc_type, DocType::Sell);
    assert_eq!(cmd.business_ts, "2026-06-08T08:59:30Z");
    assert_eq!(cmd.total_sum_kop, Some(2500));
    assert_eq!(cmd.payload_json, row.payload_json);
    // The canonical hash is the RECOMPUTED sha256 over payload_json (HIGH-2),
    // anchored to the known payload; source coincides with it for non-Z (D5).
    assert_eq!(cmd.payload_sha256_canonical, sha256(SELL_PAYLOAD));
    assert_eq!(cmd.source_sha256, sha256(SELL_PAYLOAD));
    assert_eq!(cmd.source_sha256, row.payload_sha256_canonical);
    assert_eq!(cmd.driver_id, Some(DriverId::new("maria304").unwrap()));
    assert_eq!(
        cmd.signed_by_cashier_id,
        Some(CashierId::new("cashier-vasya").unwrap())
    );
}

#[test]
fn maps_each_non_z_operation_to_its_doc_type() {
    // Non-Z = SELL / RETURN / SHIFT_OPEN. SHIFT_CLOSE is Z-class (aggregates
    // into ZReportJson) and routes to A1Z — see rejects_shift_close_as_z_class.
    for (op, expected) in [
        ("SELL", DocType::Sell),
        ("RETURN", DocType::Return),
        ("SHIFT_OPEN", DocType::ShiftOpen),
    ] {
        let mut row = sell_row();
        row.operation_type = op.to_string();
        // SHIFT_OPEN legitimately carries no total.
        if matches!(expected, DocType::ShiftOpen) {
            row.total_sum_kop = None;
        }
        let cmd = build_canonical(&row).unwrap_or_else(|e| panic!("{op} must build: {e}"));
        assert_eq!(cmd.doc_type, expected, "{op}");
    }
}

#[test]
fn shift_open_without_total_is_ok() {
    let mut row = sell_row();
    row.operation_type = "SHIFT_OPEN".to_string();
    row.total_sum_kop = None;
    let cmd = build_canonical(&row).expect("SHIFT_OPEN needs no total");
    assert_eq!(cmd.doc_type, DocType::ShiftOpen);
    assert_eq!(cmd.total_sum_kop, None);
}

/// `build_canonical` returns the reject (Ok variant is not `PartialEq`, so
/// match rather than `assert_eq!` on the whole `Result`).
fn reject_of(row: &InboxRow) -> BuildReject {
    build_canonical(row).expect_err("expected a build reject")
}

#[test]
fn rejects_missing_driver_id() {
    let mut row = sell_row();
    row.driver_id = None;
    assert_eq!(reject_of(&row), BuildReject::MissingDriverId);
}

#[test]
fn rejects_missing_business_ts() {
    let mut row = sell_row();
    row.business_ts = None;
    assert_eq!(reject_of(&row), BuildReject::MissingBusinessTs);
}

#[test]
fn rejects_sell_without_total() {
    let mut row = sell_row();
    row.total_sum_kop = None;
    assert_eq!(
        reject_of(&row),
        BuildReject::MissingTotalForSale {
            doc_type: DocType::Sell
        }
    );
}

#[test]
fn rejects_return_without_total() {
    let mut row = sell_row();
    row.operation_type = "RETURN".to_string();
    row.total_sum_kop = None;
    assert_eq!(
        reject_of(&row),
        BuildReject::MissingTotalForSale {
            doc_type: DocType::Return
        }
    );
}

#[test]
fn rejects_z_report_as_z_class() {
    let mut row = sell_row();
    row.operation_type = "Z_REPORT".to_string();
    // Distinct from UnsupportedOperation so a dispatch bug is diagnosable.
    assert_eq!(
        reject_of(&row),
        BuildReject::ZClassRequiresAggregationBuilder {
            operation_type: "Z_REPORT".to_string()
        }
    );
}

#[test]
fn rejects_shift_close_as_z_class() {
    // RS-3 A1 review HIGH-1: SHIFT_CLOSE is Z-class (aggregates the shift's
    // receipts into ZReportJson — is_z_class / convert / stage_sign). It must
    // route to A1Z, NOT build as a plain non-Z doc.
    let mut row = sell_row();
    row.operation_type = "SHIFT_CLOSE".to_string();
    assert_eq!(
        reject_of(&row),
        BuildReject::ZClassRequiresAggregationBuilder {
            operation_type: "SHIFT_CLOSE".to_string()
        }
    );
}

#[test]
fn rejects_payload_hash_mismatch() {
    // RS-3 A1 review HIGH-2: a row whose persisted hash does not match a
    // fresh sha256 over payload_json is corrupted/tampered → terminal reject,
    // BEFORE any stage_acquire work or fiscal-doc persist.
    let mut row = sell_row();
    row.payload_sha256_canonical = [0xFF; 32]; // does not match SELL_PAYLOAD
    assert_eq!(reject_of(&row), BuildReject::PayloadHashMismatch);
}

#[test]
fn rejects_unknown_operation() {
    let mut row = sell_row();
    row.operation_type = "FROBNICATE".to_string();
    assert_eq!(
        reject_of(&row),
        BuildReject::UnsupportedOperation {
            operation_type: "FROBNICATE".to_string()
        }
    );
}

#[test]
fn rejects_malformed_driver_id() {
    let mut row = sell_row();
    row.driver_id = Some(String::new()); // empty → DriverId::new fails
    assert_eq!(
        reject_of(&row),
        BuildReject::InvalidIdentity { field: "driver_id" }
    );
}

#[test]
fn reject_codes_are_stable() {
    // All 6 codes feed both the audit payload and the seam HTTP mapping —
    // pin every arm so a typo can't ship silently.
    assert_eq!(BuildReject::MissingDriverId.code(), "MISSING_DRIVER_ID");
    assert_eq!(BuildReject::MissingBusinessTs.code(), "MISSING_BUSINESS_TS");
    assert_eq!(
        BuildReject::MissingTotalForSale {
            doc_type: DocType::Sell
        }
        .code(),
        "MISSING_TOTAL_FOR_SALE"
    );
    assert_eq!(
        BuildReject::UnsupportedOperation {
            operation_type: "X".into()
        }
        .code(),
        "UNSUPPORTED_OPERATION"
    );
    assert_eq!(
        BuildReject::ZClassRequiresAggregationBuilder {
            operation_type: "SHIFT_CLOSE".into()
        }
        .code(),
        "Z_CLASS_REQUIRES_AGGREGATION"
    );
    assert_eq!(
        BuildReject::PayloadHashMismatch.code(),
        "PAYLOAD_HASH_MISMATCH"
    );
    assert_eq!(
        BuildReject::InvalidIdentity { field: "driver_id" }.code(),
        "INVALID_IDENTITY"
    );
}

#[test]
fn rejects_blank_business_ts() {
    // present-but-blank must reject, not pass into the command as "".
    let mut row = sell_row();
    row.business_ts = Some("   ".to_string());
    assert_eq!(reject_of(&row), BuildReject::MissingBusinessTs);
}

#[test]
fn rejects_malformed_cashier_id() {
    let mut row = sell_row();
    row.signed_by_cashier_id = Some(String::new()); // empty → CashierId::new fails
    assert_eq!(
        reject_of(&row),
        BuildReject::InvalidIdentity {
            field: "signed_by_cashier_id"
        }
    );
}

#[test]
fn builds_sell_with_no_cashier() {
    // signed_by_cashier_id is legitimately None — must build, not reject.
    let mut row = sell_row();
    row.signed_by_cashier_id = None;
    let cmd = build_canonical(&row).expect("a SELL with no cashier must build");
    assert_eq!(cmd.signed_by_cashier_id, None);
}

#[test]
fn shift_open_stray_total_is_normalized_to_none() {
    // A SHIFT op carrying a total is a contract anomaly — canonicalize it to
    // the documented "no total" shape rather than laundering it forward.
    let mut row = sell_row();
    row.operation_type = "SHIFT_OPEN".to_string();
    row.total_sum_kop = Some(999);
    let cmd = build_canonical(&row).expect("SHIFT_OPEN builds");
    assert_eq!(cmd.total_sum_kop, None);
}

#[test]
fn builds_sell_with_zero_total() {
    // total is REQUIRED-present for SELL, but presence is the contract — a
    // zero total builds (pins current behavior; no positivity guard in A1).
    let mut row = sell_row();
    row.total_sum_kop = Some(0);
    let cmd = build_canonical(&row).expect("SELL with zero total builds");
    assert_eq!(cmd.total_sum_kop, Some(0));
}

// ─── terminalize_rejected_tx — DB integration ───────────────────────────

async fn fresh_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    std::mem::forget(dir);
    let pool = open_pool(&path).await.unwrap();
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    pool
}

async fn seed_processing_inbox(pool: &sqlx::SqlitePool, request_id: &[u8; 16]) {
    sqlx::query(
        "INSERT INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical, status) \
         VALUES (?, ?, 'REST', 'SELL', ?, '{}', ?, 'PROCESSING')",
    )
    .bind(&request_id[..])
    .bind(FN)
    .bind(format!("idem-{:02x}", request_id[0]))
    .bind(&[0u8; 32][..])
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn terminalize_marks_rejected_audits_and_creates_no_fiscal_doc() {
    let pool = fresh_pool().await;
    let request_id = [0x5Au8; 16];
    seed_processing_inbox(&pool, &request_id).await;

    let mut row = sell_row();
    row.request_id = request_id;
    let reject = BuildReject::MissingDriverId;

    with_immediate(&pool, move |tx| {
        Box::pin(async move { terminalize_rejected_tx(tx, &row, &reject).await })
    })
    .await
    .unwrap();

    // (1) inbox flipped to REJECTED (terminal → reaper-safe).
    let status: String =
        sqlx::query_scalar("SELECT status FROM ingress_inbox WHERE request_id = ?")
            .bind(&request_id[..])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "REJECTED");

    // (2) exactly one INBOX_REJECTED audit, and its PAYLOAD actually carries
    // the machine code + context (not just a row count).
    let rid_hex: String = request_id.iter().map(|b| format!("{b:02x}")).collect();
    let payloads: Vec<String> = sqlx::query_scalar(
        "SELECT event_payload_json FROM audit_log WHERE entity_type = 'ingress_inbox' \
         AND entity_id = ? AND event_type = 'INBOX_REJECTED'",
    )
    .bind(&rid_hex)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(payloads.len(), 1, "exactly one reject audit");
    assert!(
        payloads[0].contains("MISSING_DRIVER_ID"),
        "audit payload must carry the reject code: {}",
        payloads[0]
    );
    assert!(payloads[0].contains("\"operation_type\":\"SELL\""));

    // (3) NO fiscal_documents row — a reject is a ledger non-event.
    let doc_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(doc_count, 0);
}

#[tokio::test]
async fn terminalize_persists_data_carrying_variant_in_audit() {
    // A reject that interpolates data (operation_type) must land that data in
    // the persisted audit payload — exercises the Display/code DB path that
    // the no-data MissingDriverId variant does not.
    let pool = fresh_pool().await;
    let request_id = [0x6Bu8; 16];
    seed_processing_inbox(&pool, &request_id).await;

    let mut row = sell_row();
    row.request_id = request_id;
    row.operation_type = "FROBNICATE".to_string();
    let reject = BuildReject::UnsupportedOperation {
        operation_type: "FROBNICATE".to_string(),
    };

    with_immediate(&pool, move |tx| {
        Box::pin(async move { terminalize_rejected_tx(tx, &row, &reject).await })
    })
    .await
    .unwrap();

    let rid_hex: String = request_id.iter().map(|b| format!("{b:02x}")).collect();
    let payload: String = sqlx::query_scalar(
        "SELECT event_payload_json FROM audit_log WHERE entity_type = 'ingress_inbox' \
         AND entity_id = ? AND event_type = 'INBOX_REJECTED'",
    )
    .bind(&rid_hex)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(payload.contains("UNSUPPORTED_OPERATION"), "{payload}");
    assert!(payload.contains("FROBNICATE"), "{payload}");
}

#[tokio::test]
async fn terminalize_refuses_to_clobber_a_non_processing_row() {
    // RS-3 A1 review (L2/L4 convergence): terminalize on a row that is NOT
    // PROCESSING (here a terminal DONE row) must ERROR and leave the row
    // untouched — never silently flip a completed-ledger row to REJECTED.
    let pool = fresh_pool().await;
    let request_id = [0x7Cu8; 16];
    seed_processing_inbox(&pool, &request_id).await;
    // Drive it to terminal DONE (as a successful fiscalization would).
    sqlx::query("UPDATE ingress_inbox SET status = 'DONE' WHERE request_id = ?")
        .bind(&request_id[..])
        .execute(&pool)
        .await
        .unwrap();

    let mut row = sell_row();
    row.request_id = request_id;
    let reject = BuildReject::MissingDriverId;

    let res = with_immediate(&pool, move |tx| {
        Box::pin(async move { terminalize_rejected_tx(tx, &row, &reject).await })
    })
    .await;
    assert!(res.is_err(), "terminalize on a DONE row must error");

    // The DONE row is untouched (the guarded update matched 0 rows; the
    // erroring envelope rolled back), and NO reject audit was appended.
    let status: String =
        sqlx::query_scalar("SELECT status FROM ingress_inbox WHERE request_id = ?")
            .bind(&request_id[..])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "DONE", "DONE must not be clobbered to REJECTED");

    let rid_hex: String = request_id.iter().map(|b| format!("{b:02x}")).collect();
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE entity_id = ? AND event_type = 'INBOX_REJECTED'",
    )
    .bind(&rid_hex)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 0, "no reject audit on a refused terminalize");
}
