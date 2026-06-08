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

const FN: &str = "1234567890";

/// A fully-populated non-Z SELL row with DISTINCT field values.
fn sell_row() -> InboxRow {
    InboxRow {
        request_id: [0xAB; 16],
        fiscal_number: FN.to_string(),
        protocol: Protocol::Rest,
        operation_type: "SELL".to_string(),
        idempotency_key: "idem-key-7".to_string(),
        status: "PROCESSING".to_string(),
        payload_json: r#"{"items":[{"name":"X","sum_kop":2500}]}"#.to_string(),
        payload_sha256_canonical: [0x42; 32],
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
    assert_eq!(cmd.payload_sha256_canonical, [0x42; 32]);
    // D5: non-Z → source hash coincides with the canonical (inbox) hash.
    assert_eq!(cmd.source_sha256, row.payload_sha256_canonical);
    assert_eq!(cmd.driver_id, Some(DriverId::new("maria304").unwrap()));
    assert_eq!(
        cmd.signed_by_cashier_id,
        Some(CashierId::new("cashier-vasya").unwrap())
    );
}

#[test]
fn maps_each_non_z_operation_to_its_doc_type() {
    for (op, expected) in [
        ("SELL", DocType::Sell),
        ("RETURN", DocType::Return),
        ("SHIFT_OPEN", DocType::ShiftOpen),
        ("SHIFT_CLOSE", DocType::ShiftClose),
    ] {
        let mut row = sell_row();
        row.operation_type = op.to_string();
        // SHIFT_* legitimately carry no total.
        if matches!(expected, DocType::ShiftOpen | DocType::ShiftClose) {
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
fn rejects_z_report_with_dedicated_variant() {
    let mut row = sell_row();
    row.operation_type = "Z_REPORT".to_string();
    // Distinct from UnsupportedOperation so a dispatch bug is diagnosable.
    assert_eq!(reject_of(&row), BuildReject::ZRequiresAggregationBuilder);
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
    assert_eq!(BuildReject::MissingDriverId.code(), "MISSING_DRIVER_ID");
    assert_eq!(BuildReject::MissingBusinessTs.code(), "MISSING_BUSINESS_TS");
    assert_eq!(
        BuildReject::ZRequiresAggregationBuilder.code(),
        "Z_REQUIRES_AGGREGATION"
    );
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

    // (2) an INBOX_REJECTED audit carrying the machine code exists.
    let rid_hex: String = request_id.iter().map(|b| format!("{b:02x}")).collect();
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE entity_type = 'ingress_inbox' \
         AND entity_id = ? AND event_type = 'INBOX_REJECTED'",
    )
    .bind(&rid_hex)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 1);

    // (3) NO fiscal_documents row — a reject is a ledger non-event.
    let doc_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(doc_count, 0);
}
