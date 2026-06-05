//! RS-2 piece-2a — integration test for the `convert.rs` payment mapping
//! (D1 frozen slots, repository-backed via `payment_methods`).
//!
//! Proves the DB-backed half of `convert_to_signer_payload`: `name` comes
//! from the per-FN `payment_methods` row, `type_code = pay_index-1`, and
//! the D1 fail-closed arms (missing / `iscash` mismatch) fire.  The pure
//! item-conversion + shape-parity is covered by the in-module unit tests.

use prro::db::models::enums::{DocState, DocType, FiscalMode, NodeMode, ShiftState};
use prro::db::models::ids::{DocumentId, RequestId, ShiftId};
use prro::db::repositories::fiscal_documents as fd;
use prro::db::repositories::fiscal_number_config::{self as fn_repo, NewFnConfig};
use prro::db::repositories::node_state;
use prro::db::repositories::payment_methods::{insert, soft_delete, NewPaymentMethod};
use prro::db::repositories::shifts;
use prro::db::{open_pool, open_secure_pool};
use prro::runtime::ingress::convert::{convert_to_signer_payload, ConvertError};
use prro::runtime::ingress::dto::CanonicalCommand;
use sqlx::SqlitePool;

const FN: &str = "4000000001";

/// Opens both pools (main + secure). For the payment-mapping tests the
/// main pool is unused by convert (only the ZReport arm reads it), but
/// the signature requires it.
async fn fresh_pools() -> (tempfile::TempDir, SqlitePool, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let main = open_pool(&dir.path().join("main.db"))
        .await
        .expect("open_pool");
    let secure = open_secure_pool(&dir.path().join("secure.db"))
        .await
        .expect("open_secure_pool");
    (dir, main, secure)
}

fn pm(pay_index: i64, name: &str, iscash: bool) -> NewPaymentMethod {
    NewPaymentMethod {
        fn_id: FN.to_string(),
        pay_index,
        name: name.to_string(),
        iscash,
    }
}

/// A SELL with one CASH + one CASHLESS_1 payment.
fn sell_with_cash_and_cashless1() -> CanonicalCommand {
    let json = r#"{
        "schema_version": "1.0",
        "fiscal_number": "4000000001",
        "command_type": "SELL",
        "idempotency_key": "idem-1",
        "cashier_id": null,
        "department": null,
        "return_check_number": null,
        "payload": {
            "direction": "SALE",
            "goods": [
                {"name":"Bread","quantity_milli":1000,"price_kopecks":15000,
                 "tax_group_1":1,"tax_group_2":0,"article_code":42}
            ],
            "payments": [
                {"type":"CASH","amount_kopecks":10000},
                {"type":"CASHLESS_1","amount_kopecks":5000}
            ],
            "totals": {"sale_kopecks":15000,"return_kopecks":0}
        }
    }"#;
    serde_json::from_str(json).expect("parse SELL fixture")
}

#[tokio::test]
async fn payments_take_name_from_table_and_type_code_from_slot() {
    let (_dir, main, pool) = fresh_pools().await;
    insert(&pool, &pm(1, "Готівка", true)).await.unwrap();
    insert(&pool, &pm(2, "Картка", false)).await.unwrap();

    let conv = convert_to_signer_payload(&sell_with_cash_and_cashless1(), FN, &main, &pool)
        .await
        .expect("convert must succeed with seeded slots");

    // name from the per-FN table; type_code = pay_index - 1.
    assert!(
        conv.payload_json.contains(r#""name":"Готівка""#),
        "{}",
        conv.payload_json
    );
    assert!(conv.payload_json.contains(r#""type_code":"0""#));
    assert!(conv.payload_json.contains(r#""name":"Картка""#));
    assert!(conv.payload_json.contains(r#""type_code":"1""#));
    // wire-shape field names must NOT survive into the signer payload.
    assert!(!conv.payload_json.contains("price_kopecks"));
    assert!(!conv.payload_json.contains("amount_kopecks"));
}

#[tokio::test]
async fn missing_payment_slot_is_typed_error() {
    let (_dir, main, pool) = fresh_pools().await;
    // Seed only the cash slot; CASHLESS_1 (pay_index 2) is absent.
    insert(&pool, &pm(1, "Готівка", true)).await.unwrap();

    let err = convert_to_signer_payload(&sell_with_cash_and_cashless1(), FN, &main, &pool)
        .await
        .expect_err("absent pay_index 2 must be a typed error");
    assert!(
        matches!(err, ConvertError::MissingPaymentMethod { pay_index: 2, .. }),
        "got {err:?}"
    );
}

#[tokio::test]
async fn inactive_payment_slot_is_typed_error() {
    let (_dir, main, pool) = fresh_pools().await;
    insert(&pool, &pm(1, "Готівка", true)).await.unwrap();
    insert(&pool, &pm(2, "Картка", false)).await.unwrap();
    // Deactivate the cash slot → a CASH wire payment finds it inactive.
    soft_delete(&pool, FN, 1).await.unwrap();

    let err = convert_to_signer_payload(&sell_with_cash_and_cashless1(), FN, &main, &pool)
        .await
        .expect_err("inactive pay_index 1 must be a typed error");
    assert!(
        matches!(
            err,
            ConvertError::InactivePaymentMethod { pay_index: 1, .. }
        ),
        "got {err:?}"
    );
}

/// A SELL whose CASHLESS_1 payment carries an `acquirer_slip` (EPZ).
fn sell_with_cashless_slip() -> CanonicalCommand {
    let json = r#"{
        "schema_version": "1.0",
        "fiscal_number": "4000000001",
        "command_type": "SELL",
        "idempotency_key": "idem-slip",
        "cashier_id": null,
        "department": null,
        "return_check_number": null,
        "payload": {
            "direction": "SALE",
            "goods": [
                {"name":"Bread","quantity_milli":1000,"price_kopecks":15000,
                 "tax_group_1":1,"tax_group_2":0,"article_code":42}
            ],
            "payments": [
                {"type":"CASHLESS_1","amount_kopecks":15000,
                 "acquirer_slip":{
                    "payment_form_index":1,"merchant_id":"M1","terminal_id":"T1",
                    "operation_type":"sale","pan":"****1234","approval_code":"OK123",
                    "payment_system":"VISA","transaction_code":"TX1","fee_kopecks":0,
                    "cashier_signature_placeholder":false,
                    "cardholder_signature_placeholder":false}}
            ],
            "totals": {"sale_kopecks":15000,"return_kopecks":0}
        }
    }"#;
    serde_json::from_str(json).expect("parse SELL-with-slip fixture")
}

#[tokio::test]
async fn acquirer_slip_fails_closed_until_epz_mapping_lands() {
    let (_dir, main, pool) = fresh_pools().await;
    insert(&pool, &pm(2, "Картка", false)).await.unwrap();

    let err = convert_to_signer_payload(&sell_with_cashless_slip(), FN, &main, &pool)
        .await
        .expect_err("acquirer_slip must fail closed, not silently drop");
    assert!(
        matches!(
            err,
            ConvertError::AcquirerSlipMappingDeferred { pay_index: 2, .. }
        ),
        "got {err:?}"
    );
}

/// A SELL whose item carries a non-zero secondary tax_group_2 but the
/// payload has NO `dual_tax_mode`.
fn sell_with_secondary_tax_no_dual() -> CanonicalCommand {
    let json = r#"{
        "schema_version": "1.0",
        "fiscal_number": "4000000001",
        "command_type": "SELL",
        "idempotency_key": "idem-tax2",
        "cashier_id": null,
        "department": null,
        "return_check_number": null,
        "payload": {
            "direction": "SALE",
            "goods": [
                {"name":"Bread","quantity_milli":1000,"price_kopecks":15000,
                 "tax_group_1":1,"tax_group_2":3,"article_code":42}
            ],
            "payments": [{"type":"CASH","amount_kopecks":15000}],
            "totals": {"sale_kopecks":15000,"return_kopecks":0}
        }
    }"#;
    serde_json::from_str(json).expect("parse SELL secondary-tax fixture")
}

#[tokio::test]
async fn secondary_tax_without_dual_mode_fails_closed_through_orchestrator() {
    let (_dir, main, pool) = fresh_pools().await;
    // No payment_methods seeded — the item-tax error fires before the
    // payment lookup, proving fail-closed at the orchestrator level.
    let err = convert_to_signer_payload(&sell_with_secondary_tax_no_dual(), FN, &main, &pool)
        .await
        .expect_err("secondary tax without dual_tax_mode must fail closed");
    assert!(
        matches!(
            err,
            ConvertError::SecondaryTaxRequiresDualTaxMode { tax_group_2: 3, .. }
        ),
        "got {err:?}"
    );
}

/// A SELL carrying a non-empty `raw_frames` (M5-scope fiscal data).
fn sell_with_raw_frames() -> CanonicalCommand {
    let json = r#"{
        "schema_version": "1.0",
        "fiscal_number": "4000000001",
        "command_type": "SELL",
        "idempotency_key": "idem-frames",
        "cashier_id": null,
        "department": null,
        "return_check_number": null,
        "payload": {
            "direction": "SALE",
            "goods": [
                {"name":"Bread","quantity_milli":1000,"price_kopecks":15000,
                 "tax_group_1":1,"tax_group_2":0,"article_code":42}
            ],
            "payments": [{"type":"CASH","amount_kopecks":15000}],
            "totals": {"sale_kopecks":15000,"return_kopecks":0},
            "raw_frames": [{"opcode":"DISC","body":"check-level discount"}]
        }
    }"#;
    serde_json::from_str(json).expect("parse SELL raw_frames fixture")
}

#[tokio::test]
async fn raw_frames_fail_closed_through_orchestrator() {
    let (_dir, main, pool) = fresh_pools().await;
    // Fires before the payment lookup → no payment_methods needed.
    let err = convert_to_signer_payload(&sell_with_raw_frames(), FN, &main, &pool)
        .await
        .expect_err("non-empty raw_frames must fail closed, not be dropped");
    assert!(
        matches!(err, ConvertError::RawFramesNotSupported { count: 1 }),
        "got {err:?}"
    );
}

/// A SELL with no goods.
fn sell_with_no_goods() -> CanonicalCommand {
    let json = r#"{
        "schema_version": "1.0",
        "fiscal_number": "4000000001",
        "command_type": "SELL",
        "idempotency_key": "idem-empty",
        "cashier_id": null,
        "department": null,
        "return_check_number": null,
        "payload": {
            "direction": "SALE",
            "goods": [],
            "payments": [{"type":"CASH","amount_kopecks":0}],
            "totals": {"sale_kopecks":0,"return_kopecks":0}
        }
    }"#;
    serde_json::from_str(json).expect("parse SELL empty-goods fixture")
}

#[tokio::test]
async fn empty_goods_sell_is_typed_error() {
    let (_dir, main, pool) = fresh_pools().await;
    let err = convert_to_signer_payload(&sell_with_no_goods(), FN, &main, &pool)
        .await
        .expect_err("SELL with empty goods must be a typed error");
    assert!(matches!(err, ConvertError::EmptyGoods), "got {err:?}");
}

#[tokio::test]
async fn iscash_mismatch_at_frozen_slot_is_typed_error() {
    let (_dir, main, pool) = fresh_pools().await;
    // Misconfigured FN: pay_index 1 is NON-cash → a CASH wire payment
    // looking up slot 1 finds iscash=false → fail-closed (D1).
    insert(&pool, &pm(1, "Картка", false)).await.unwrap();

    let err = convert_to_signer_payload(&sell_with_cash_and_cashless1(), FN, &main, &pool)
        .await
        .expect_err("iscash mismatch at the frozen cash slot must fail closed");
    assert!(
        matches!(
            err,
            ConvertError::PaymentSlotKindMismatch {
                pay_index: 1,
                slot_iscash: false,
                kind_is_cash: true,
                ..
            }
        ),
        "got {err:?}"
    );
}

// ─── piece-2b — ZReport ledger aggregation (full convert path) ───────

/// `insert_prepared` FKs `fiscal_number` → `fiscal_number_config`; seed it.
async fn seed_fn_config(main: &SqlitePool) {
    fn_repo::insert(
        main,
        &NewFnConfig {
            fiscal_number: FN.to_string(),
            tax_number: "12345678".into(),
            vat_payer_inn: None,
            fiscal_mode: FiscalMode::Test,
            org_name: None,
            point_name: None,
            org_address: None,
            tsp_enabled: false,
            offline_enabled: true,
            national_check_enabled: false,
            min_offline_codes: 0,
            max_offline_codes: 0,
        },
    )
    .await
    .expect("seed fn_config");
}

/// Seed one issued (terminal-state) SELL/RETURN receipt in `shift` whose
/// stored converted-CheckJson payload carries `payments_json`.
async fn seed_issued_receipt(
    main: &SqlitePool,
    shift: ShiftId,
    lnd: i64,
    doc_type: DocType,
    state: DocState,
    payments_json: &str,
) {
    let new = fd::NewDocument {
        document_id: DocumentId::new(),
        request_id: RequestId::new(),
        fiscal_number: FN.to_string(),
        shift_id: Some(shift),
        offline_session_id: None,
        lnd,
        doc_type,
        backend_profile_id: "b".into(),
        transport_profile_id: "t".into(),
        fs_mode: "ONLINE",
        business_ts: "2026-06-05T12:00:00Z".into(),
        total_sum_kop: Some(0),
        payload_json: format!(r#"{{"items":[],"payments":[{payments_json}]}}"#),
        payload_sha256_canonical: [0u8; 32],
        unsigned_xml_sha256: None,
        previous_hash: None,
        signed_by_cashier_id: None,
        signing_config_snapshot_id: None,
    };
    let id = new.document_id;
    fd::insert_prepared(main, &new)
        .await
        .expect("insert_prepared receipt");
    sqlx::query("UPDATE fiscal_documents SET state = ? WHERE document_id = ?")
        .bind(state)
        .bind(id)
        .execute(main)
        .await
        .expect("set terminal state");
}

fn z_report_cmd(idem: &str) -> CanonicalCommand {
    let json = format!(
        r#"{{"schema_version":"1.0","fiscal_number":"4000000001",
            "command_type":"Z_REPORT","idempotency_key":"{idem}",
            "cashier_id":null,"department":null,"return_check_number":null,
            "payload":{{"totals":{{"sale_kopecks":0,"return_kopecks":0}}}}}}"#
    );
    serde_json::from_str(&json).expect("parse Z_REPORT")
}

#[tokio::test]
async fn zreport_aggregates_only_issued_shift_receipts() {
    let (_dir, main, secure) = fresh_pools().await;
    seed_fn_config(&main).await;
    let shift = ShiftId::new();
    // fiscal_documents.shift_id FKs shifts(shift_id) — seed the shift row.
    shifts::insert_created(&main, shift, FN, "ONLINE", "cashier-1")
        .await
        .expect("seed shift");
    node_state::upsert_initial(&main, FN, NodeMode::Online, ShiftState::Opened, 1)
        .await
        .unwrap();
    sqlx::query("UPDATE node_state SET current_shift_id = ? WHERE fiscal_number = ?")
        .bind(shift)
        .bind(FN)
        .execute(&main)
        .await
        .unwrap();

    let cash = r#"{"name":"Готівка","sum_kop":10000,"type_code":"0"}"#;
    seed_issued_receipt(&main, shift, 1, DocType::Sell, DocState::Ack, cash).await;
    seed_issued_receipt(
        &main,
        shift,
        2,
        DocType::Sell,
        DocState::OfflineLocalAck,
        r#"{"name":"Готівка","sum_kop":2500,"type_code":"0"}"#,
    )
    .await;
    seed_issued_receipt(
        &main,
        shift,
        3,
        DocType::Return,
        DocState::Ack,
        r#"{"name":"Готівка","sum_kop":3000,"type_code":"0"}"#,
    )
    .await;
    // A non-terminal SELL (Sending) MUST be excluded from the Z.
    seed_issued_receipt(
        &main,
        shift,
        4,
        DocType::Sell,
        DocState::Sending,
        r#"{"name":"Готівка","sum_kop":99999,"type_code":"0"}"#,
    )
    .await;

    let conv = convert_to_signer_payload(&z_report_cmd("z1"), FN, &main, &secure)
        .await
        .expect("ZReport convert");

    assert!(
        conv.payload_json.contains(r#""sell_count":2"#),
        "{}",
        conv.payload_json
    );
    assert!(conv.payload_json.contains(r#""return_count":1"#));
    assert!(conv.payload_json.contains(r#""sum_in_kop":12500"#));
    assert!(conv.payload_json.contains(r#""sum_out_kop":3000"#));
    // The excluded in-flight doc's amount must NOT leak into the Z.
    assert!(!conv.payload_json.contains("99999"));
}

#[tokio::test]
async fn zreport_with_no_open_shift_is_typed_error() {
    let (_dir, main, secure) = fresh_pools().await;
    seed_fn_config(&main).await;
    node_state::upsert_initial(&main, FN, NodeMode::Online, ShiftState::Closed, 1)
        .await
        .unwrap();
    // current_shift_id left NULL.
    let err = convert_to_signer_payload(&z_report_cmd("z2"), FN, &main, &secure)
        .await
        .expect_err("Z with no open shift must be a typed error");
    assert!(
        matches!(err, ConvertError::NoOpenShiftForZReport { .. }),
        "got {err:?}"
    );
}
