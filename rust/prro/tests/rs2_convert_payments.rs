//! RS-2 piece-2a — integration test for the `convert.rs` payment mapping
//! (D1 frozen slots, repository-backed via `payment_methods`).
//!
//! Proves the DB-backed half of `convert_to_signer_payload`: `name` comes
//! from the per-FN `payment_methods` row, `type_code = pay_index-1`, and
//! the D1 fail-closed arms (missing / `iscash` mismatch) fire.  The pure
//! item-conversion + shape-parity is covered by the in-module unit tests.

use prro::db::open_secure_pool;
use prro::db::repositories::payment_methods::{insert, soft_delete, NewPaymentMethod};
use prro::runtime::ingress::convert::{convert_to_signer_payload, ConvertError};
use prro::runtime::ingress::dto::CanonicalCommand;
use sqlx::SqlitePool;

const FN: &str = "4000000001";

async fn fresh_secure_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = open_secure_pool(&dir.path().join("secure.db"))
        .await
        .expect("open_secure_pool");
    (dir, pool)
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
    let (_dir, pool) = fresh_secure_pool().await;
    insert(&pool, &pm(1, "Готівка", true)).await.unwrap();
    insert(&pool, &pm(2, "Картка", false)).await.unwrap();

    let conv = convert_to_signer_payload(&sell_with_cash_and_cashless1(), FN, &pool)
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
    let (_dir, pool) = fresh_secure_pool().await;
    // Seed only the cash slot; CASHLESS_1 (pay_index 2) is absent.
    insert(&pool, &pm(1, "Готівка", true)).await.unwrap();

    let err = convert_to_signer_payload(&sell_with_cash_and_cashless1(), FN, &pool)
        .await
        .expect_err("absent pay_index 2 must be a typed error");
    assert!(
        matches!(err, ConvertError::MissingPaymentMethod { pay_index: 2, .. }),
        "got {err:?}"
    );
}

#[tokio::test]
async fn inactive_payment_slot_is_typed_error() {
    let (_dir, pool) = fresh_secure_pool().await;
    insert(&pool, &pm(1, "Готівка", true)).await.unwrap();
    insert(&pool, &pm(2, "Картка", false)).await.unwrap();
    // Deactivate the cash slot → a CASH wire payment finds it inactive.
    soft_delete(&pool, FN, 1).await.unwrap();

    let err = convert_to_signer_payload(&sell_with_cash_and_cashless1(), FN, &pool)
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
    let (_dir, pool) = fresh_secure_pool().await;
    insert(&pool, &pm(2, "Картка", false)).await.unwrap();

    let err = convert_to_signer_payload(&sell_with_cashless_slip(), FN, &pool)
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
    let (_dir, pool) = fresh_secure_pool().await;
    // No payment_methods seeded — the item-tax error fires before the
    // payment lookup, proving fail-closed at the orchestrator level.
    let err = convert_to_signer_payload(&sell_with_secondary_tax_no_dual(), FN, &pool)
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

#[tokio::test]
async fn iscash_mismatch_at_frozen_slot_is_typed_error() {
    let (_dir, pool) = fresh_secure_pool().await;
    // Misconfigured FN: pay_index 1 is NON-cash → a CASH wire payment
    // looking up slot 1 finds iscash=false → fail-closed (D1).
    insert(&pool, &pm(1, "Картка", false)).await.unwrap();

    let err = convert_to_signer_payload(&sell_with_cash_and_cashless1(), FN, &pool)
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
