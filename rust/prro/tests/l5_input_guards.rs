//! L5 — fail-closed pre-inbox input guards (pre-pilot hardening).
//!
//! RED-first: these V0 pins were written BEFORE the four new `ConvertError`
//! variants existed; each was observed to fail (variant absent / guard not
//! firing), then the implementation in `convert.rs` was written to GREEN.
//!
//! All four guards mirror the existing pre-inbox row-less 422 pattern
//! (`CashInsufficient` / `EpzPaymentIdTooLow`): a fail-closed refusal in
//! `convert_to_signer_payload` BEFORE any inbox / fiscal_documents row is
//! minted (audit_log only), mapped to HTTP 422 via `convert_error_code` +
//! `http_status_for_error_code`.
//!
//! ## Pins (contract §V0)
//! - G1 `CashCapExceeded`   — cash legs (type_code=="0") Σ > 4_999_999 kop (SELL)
//! - G2 `ZeroPriceLine`     — a good with item_sum_kop == 0 (zero price)
//! - G3 `ZeroPaymentAmount` — a payment with sum_kop == 0
//! - G4 `UnderpaymentRefused` — SELL Σpayments < Σgoods (payments present)

use prro::db::models::enums::{FiscalMode, NodeMode, ShiftState};
use prro::db::models::ids::ShiftId;
use prro::db::open_pool;
use prro::db::open_secure_pool;
use prro::db::repositories::fiscal_number_config::{self as fn_repo, NewFnConfig};
use prro::db::repositories::node_state;
use prro::db::repositories::payment_methods::{insert as pm_insert, NewPaymentMethod};
use prro::db::repositories::shifts;
use prro::db::types::DbShiftId;
use prro::runtime::ingress::convert::{convert_to_signer_payload, ConvertError};
use prro::runtime::ingress::dto::CanonicalCommand;
use sqlx::SqlitePool;

const FN: &str = "4000100005";

// ──────────────────────────────────────────────────────────────────────────────
// Helpers (self-contained; mirror l0_l1_cash_ledger.rs / l3_service_io.rs)
// ──────────────────────────────────────────────────────────────────────────────

async fn setup_open_shift() -> (tempfile::TempDir, SqlitePool, SqlitePool, ShiftId) {
    let dir = tempfile::tempdir().expect("tempdir");
    let main = open_pool(&dir.path().join("main.db"))
        .await
        .expect("open main");
    let secure = open_secure_pool(&dir.path().join("secure.db"))
        .await
        .expect("open secure");

    fn_repo::insert(
        &main,
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

    // Cash slot 1 (iscash), card slot 2 (non-cash) — the D1 frozen layout.
    pm_insert(
        &secure,
        &NewPaymentMethod {
            fn_id: FN.to_string(),
            pay_index: 1,
            name: "Готівка".into(),
            iscash: true,
        },
    )
    .await
    .expect("seed cash pm");
    pm_insert(
        &secure,
        &NewPaymentMethod {
            fn_id: FN.to_string(),
            pay_index: 2,
            name: "Картка".into(),
            iscash: false,
        },
    )
    .await
    .expect("seed card pm");

    let shift = ShiftId::new();
    use prro::db::tx::with_immediate;
    with_immediate(&main, move |tx| {
        Box::pin(async move {
            shifts::insert_created_tx(tx, shift, FN, "ONLINE", "cashier-1", 0)
                .await
                .map_err(Into::into)
        })
    })
    .await
    .expect("insert shift");
    for (from, to) in [
        (ShiftState::Created, ShiftState::Opening),
        (ShiftState::Opening, ShiftState::Opened),
    ] {
        with_immediate(&main, move |tx| {
            Box::pin(async move {
                shifts::transition(tx, shift, from, to)
                    .await
                    .map(|_| ())
                    .map_err(Into::into)
            })
        })
        .await
        .expect("shift transition");
    }

    node_state::upsert_initial(&main, FN, NodeMode::Online, ShiftState::Opened, 1)
        .await
        .unwrap();
    sqlx::query("UPDATE node_state SET current_shift_id = ? WHERE fiscal_number = ?")
        .bind(DbShiftId(shift))
        .bind(FN)
        .execute(&main)
        .await
        .unwrap();

    (dir, main, secure, shift)
}

/// Count all fiscal_documents rows for FN — used to assert row-less refusal.
async fn doc_count(main: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(main)
        .await
        .unwrap()
}

/// Build a SELL command with one good (price/qty) + a chosen payment list.
/// `good_price_kop` is the per-unit price; qty is 1.000 so item_sum == price.
/// `payments_json` is the raw JSON array body of the `payments` field.
fn sell_cmd(
    idem: &str,
    good_price_kop: i64,
    payments_json: &str,
    total_sale_kop: i64,
) -> CanonicalCommand {
    let json = format!(
        r#"{{
            "schema_version": "1.0",
            "fiscal_number": "{FN}",
            "command_type": "SELL",
            "idempotency_key": "{idem}",
            "cashier_id": null,
            "department": null,
            "return_check_number": null,
            "payload": {{
                "direction": "SALE",
                "goods": [{{"name":"Item","quantity_milli":1000,"price_kopecks":{good_price_kop},"tax_group_1":0,"tax_group_2":0,"article_code":1}}],
                "payments": {payments_json},
                "totals": {{"sale_kopecks":{total_sale_kop},"return_kopecks":0}}
            }}
        }}"#
    );
    serde_json::from_str(&json).expect("parse SELL cmd")
}

// ──────────────────────────────────────────────────────────────────────────────
// G1 — CashCapExceeded (cash legs Σ > 4_999_999 kop, SELL)
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn g1_cash_over_cap_refused_pre_inbox() {
    // A single cash payment of 5_000_000 kop (50 000.00 UAH) → over the cap.
    let (_dir, main, secure, _shift) = setup_open_shift().await;

    let cmd = sell_cmd(
        "g1-over",
        5_000_000,
        r#"[{"type":"CASH","amount_kopecks":5000000}]"#,
        5_000_000,
    );
    let err = convert_to_signer_payload(&cmd, FN, &main, &secure)
        .await
        .expect_err("cash Σ over cap must be refused by G1");

    assert!(
        matches!(
            err,
            ConvertError::CashCapExceeded {
                cash_kop: 5_000_000,
                cap_kop: 4_999_999
            }
        ),
        "expected CashCapExceeded(5_000_000, 4_999_999), got: {err:?}"
    );
    assert_eq!(doc_count(&main).await, 0, "G1 must be row-less (pre-inbox)");
}

#[tokio::test]
async fn g1_cash_at_cap_boundary_ok() {
    // Exactly 4_999_999 kop → the boundary is inclusive-PASS (> is the trigger).
    let (_dir, main, secure, _shift) = setup_open_shift().await;

    let cmd = sell_cmd(
        "g1-at-cap",
        4_999_999,
        r#"[{"type":"CASH","amount_kopecks":4999999}]"#,
        4_999_999,
    );
    convert_to_signer_payload(&cmd, FN, &main, &secure)
        .await
        .expect("cash Σ exactly at cap (4_999_999) must PASS");
}

// ──────────────────────────────────────────────────────────────────────────────
// G2 — ZeroPriceLine (a good with item_sum_kop == 0)
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn g2_zero_price_line_refused_pre_inbox() {
    // price 0, qty 1.000 → item_sum_kop == 0 (a zero-price line).  Distinct
    // from ZeroQuantityLine (qty is non-zero here).
    let (_dir, main, secure, _shift) = setup_open_shift().await;

    let cmd = sell_cmd(
        "g2-zero-price",
        0,
        r#"[{"type":"CASH","amount_kopecks":0}]"#,
        0,
    );
    let err = convert_to_signer_payload(&cmd, FN, &main, &secure)
        .await
        .expect_err("zero-price good must be refused by G2");

    assert!(
        matches!(err, ConvertError::ZeroPriceLine { .. }),
        "expected ZeroPriceLine, got: {err:?}"
    );
    assert_eq!(doc_count(&main).await, 0, "G2 must be row-less (pre-inbox)");
}

// ──────────────────────────────────────────────────────────────────────────────
// G3 — ZeroPaymentAmount (a payment with sum_kop == 0)
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn g3_zero_payment_amount_refused_pre_inbox() {
    // A good priced 100.00, but a payment of amount 0 → a zero-value payment
    // leg (G3).  (Underpayment G4 would ALSO fire here, but G3 is checked on
    // the per-payment scan; assert the zero-payment variant explicitly by
    // using a card leg that covers the good + a zero cash leg.)
    let (_dir, main, secure, _shift) = setup_open_shift().await;

    let cmd = sell_cmd(
        "g3-zero-pay",
        10000,
        r#"[{"type":"CASHLESS_1","amount_kopecks":10000},{"type":"CASH","amount_kopecks":0}]"#,
        10000,
    );
    let err = convert_to_signer_payload(&cmd, FN, &main, &secure)
        .await
        .expect_err("zero-amount payment leg must be refused by G3");

    assert!(
        matches!(err, ConvertError::ZeroPaymentAmount { .. }),
        "expected ZeroPaymentAmount, got: {err:?}"
    );
    assert_eq!(doc_count(&main).await, 0, "G3 must be row-less (pre-inbox)");
}

// ──────────────────────────────────────────────────────────────────────────────
// G4 — UnderpaymentRefused (SELL Σpayments < Σgoods, payments present)
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn g4_underpayment_refused_pre_inbox() {
    // goods total 1000 kop, one cash payment of 900 kop → underpaid by 100.
    let (_dir, main, secure, _shift) = setup_open_shift().await;

    let cmd = sell_cmd(
        "g4-under",
        1000,
        r#"[{"type":"CASH","amount_kopecks":900}]"#,
        1000,
    );
    let err = convert_to_signer_payload(&cmd, FN, &main, &secure)
        .await
        .expect_err("underpaid SELL must be refused by G4");

    assert!(
        matches!(
            err,
            ConvertError::UnderpaymentRefused {
                goods_kop: 1000,
                paid_kop: 900
            }
        ),
        "expected UnderpaymentRefused(1000, 900), got: {err:?}"
    );
    assert_eq!(doc_count(&main).await, 0, "G4 must be row-less (pre-inbox)");
}

#[tokio::test]
async fn g4_exact_payment_ok() {
    // goods total 1000 kop, cash payment 1000 kop → exact match must PASS.
    let (_dir, main, secure, _shift) = setup_open_shift().await;

    let cmd = sell_cmd(
        "g4-exact",
        1000,
        r#"[{"type":"CASH","amount_kopecks":1000}]"#,
        1000,
    );
    convert_to_signer_payload(&cmd, FN, &main, &secure)
        .await
        .expect("exact payment (1000 == 1000) must PASS");
}

#[tokio::test]
async fn g4_empty_payments_not_gated() {
    // A SELL with NO payments declared is the pre-existing "cash implied" shape
    // that convert already tolerates — G4 must NOT fire (you cannot "underpay"
    // when no payment leg was declared).  Preserves the frozen suite
    // (handler.rs `sell_cmd` uses `"payments":[]`).
    let (_dir, main, secure, _shift) = setup_open_shift().await;

    let cmd = sell_cmd("g4-empty", 1000, r#"[]"#, 1000);
    convert_to_signer_payload(&cmd, FN, &main, &secure)
        .await
        .expect("SELL with no payment legs must NOT be gated by G4");
}
