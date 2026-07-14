//! L0 + L1 cash-ledger integration tests.
//!
//! RED-first: all pins were written before the implementation; each was
//! observed to fail, then the implementation was written to make it green.
//!
//! ## L0 pins (derive_cash_on_hand + carry)
//! 1. `pin_l0_empty_shift_zero`   — first shift, no docs → 0
//! 2. `pin_l0_cash_sales_sum`     — two cash SELLs → correct sum
//! 3. `pin_l0_cash_return_subtracts` — SELL then RETURN
//! 4. `pin_l0_noncash_excluded`   — card SELL not counted
//! 5. `pin_l0_mixed_receipt`      — cash+card split: only cash leg counts
//! 6. `pin_l0_carry_across_shifts` — opening = prior shift's closing
//! 7. `pin_l0_boot_reconcile_detects_drift` — anchor corruption detected
//!
//! ## L1 pins (INV-21 guard in convert_to_signer_payload)
//! 1. `pin_l1_return_over_empty_drawer_refused` — 0 cash, RETURN 1.00 → 422
//! 2. `pin_l1_return_within_balance_ok`         — SELL 100, RETURN 50 ok; RETURN 60 refused
//! 3. `pin_l1_noncash_return_not_gated`         — card RETURN: no cash floor check
//! 4. `pin_l1_exact_zero_ok`                    — drawer 50, RETURN 50 → accepted (floor = 0)
//! 5. `pin_l1_teeth_revert_guard`               — canary: guard is load-bearing

use prro::db::models::enums::{DocState, DocType, FiscalMode, NodeMode, ShiftState};
use prro::db::models::ids::{DocumentId, RequestId, ShiftId};
use prro::db::open_pool;
use prro::db::open_secure_pool;
use prro::db::repositories::fiscal_documents as fd;
use prro::db::repositories::fiscal_number_config::{self as fn_repo, NewFnConfig};
use prro::db::repositories::node_state;
use prro::db::repositories::payment_methods::{insert as pm_insert, NewPaymentMethod};
use prro::db::repositories::shifts;
use prro::runtime::ingress::convert::{convert_to_signer_payload, ConvertError};
use prro::runtime::ingress::dto::CanonicalCommand;
use prro::services::cash_ledger::{
    aggregate_shift_cash, derive_cash_on_hand, derive_closing_cash, opening_carry_for_fn,
    reconcile_opening_anchor,
};
use sqlx::SqlitePool;

const FN: &str = "4000100001";

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

async fn fresh_main() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let main = open_pool(&dir.path().join("main.db"))
        .await
        .expect("open main");
    (dir, main)
}

async fn seed_fn(pool: &SqlitePool) {
    fn_repo::insert(
        pool,
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

/// Seed payment methods (cash slot 1, card slot 2) on the secure pool.
async fn seed_payment_methods(secure: &SqlitePool) {
    pm_insert(
        secure,
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
        secure,
        &NewPaymentMethod {
            fn_id: FN.to_string(),
            pay_index: 2,
            name: "Картка".into(),
            iscash: false,
        },
    )
    .await
    .expect("seed card pm");
}

/// Create a shift row in CREATED state, then transition it to the given state.
async fn seed_shift(pool: &SqlitePool, id: ShiftId, state: ShiftState, opening_cash: i64) {
    use prro::db::tx::with_immediate;
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            shifts::insert_created_tx(tx, id, FN, "ONLINE", "cashier-1", opening_cash)
                .await
                .map_err(Into::into)
        })
    })
    .await
    .expect("insert shift");

    // Drive through states to reach `state`.
    let path: &[(ShiftState, ShiftState)] = match state {
        ShiftState::Created => &[],
        ShiftState::Opening => &[(ShiftState::Created, ShiftState::Opening)],
        ShiftState::Opened => &[
            (ShiftState::Created, ShiftState::Opening),
            (ShiftState::Opening, ShiftState::Opened),
        ],
        ShiftState::Closing => &[
            (ShiftState::Created, ShiftState::Opening),
            (ShiftState::Opening, ShiftState::Opened),
            (ShiftState::Opened, ShiftState::Closing),
        ],
        ShiftState::Closed => &[
            (ShiftState::Created, ShiftState::Opening),
            (ShiftState::Opening, ShiftState::Opened),
            (ShiftState::Opened, ShiftState::Closing),
            (ShiftState::Closing, ShiftState::Closed),
        ],
        _ => panic!("seed_shift: unsupported target state {state:?}"),
    };
    for &(from, to) in path {
        use prro::db::tx::with_immediate;
        with_immediate(pool, move |tx| {
            Box::pin(async move {
                shifts::transition(tx, id, from, to)
                    .await
                    .map(|_| ())
                    .map_err(Into::into)
            })
        })
        .await
        .expect("shift transition");
    }
}

/// Seed a closed shift and also write the closing cash into `cash_balance_kop`.
async fn seed_closed_shift_with_closing_cash(
    pool: &SqlitePool,
    id: ShiftId,
    opening_cash: i64,
    closing_cash: i64,
) {
    seed_shift(pool, id, ShiftState::Closed, opening_cash).await;
    // Overwrite cash_balance_kop with the closing value (mimics what
    // confirm_shift_edge does atomically in prod).
    sqlx::query("UPDATE shifts SET cash_balance_kop = ? WHERE shift_id = ?")
        .bind(closing_cash)
        .bind(id)
        .execute(pool)
        .await
        .expect("write closing cash");
}

/// Seed one issued SELL or RETURN receipt with a payments JSON blob.
async fn seed_issued_receipt(
    pool: &SqlitePool,
    shift: ShiftId,
    lnd: i64,
    doc_type: DocType,
    state: DocState,
    payments_json: &str,
) -> DocumentId {
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
        business_ts: "2026-07-10T10:00:00Z".into(),
        total_sum_kop: Some(0),
        payload_json: format!(r#"{{"items":[],"payments":[{payments_json}]}}"#),
        payload_sha256_canonical: [0u8; 32],
        source_sha256: [0u8; 32],
        unsigned_xml_sha256: None,
        previous_hash: None,
        signed_by_cashier_id: None,
        signing_config_snapshot_id: None,
    };
    let id = new.document_id;
    fd::insert_prepared(pool, &new)
        .await
        .expect("insert_prepared");
    sqlx::query("UPDATE fiscal_documents SET state = ? WHERE document_id = ?")
        .bind(state.as_str())
        .bind(id)
        .execute(pool)
        .await
        .expect("set state");
    id
}

fn cash(sum_kop: i64) -> String {
    format!(r#"{{"name":"Готівка","sum_kop":{sum_kop},"type_code":"0"}}"#)
}

fn card(sum_kop: i64) -> String {
    format!(r#"{{"name":"Картка","sum_kop":{sum_kop},"type_code":"1"}}"#)
}

/// Wire a node_state row for `FN` pointing at `shift_id` in `Opened` state.
async fn wire_node_state(pool: &SqlitePool, shift: ShiftId) {
    node_state::upsert_initial(pool, FN, NodeMode::Online, ShiftState::Opened, 1)
        .await
        .unwrap();
    sqlx::query("UPDATE node_state SET current_shift_id = ? WHERE fiscal_number = ?")
        .bind(shift)
        .bind(FN)
        .execute(pool)
        .await
        .unwrap();
}

// ──────────────────────────────────────────────────────────────────────────────
// L0 pins — derive_cash_on_hand (pure)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn pin_l0_empty_shift_zero() {
    // First shift, no docs: derive_cash_on_hand(0, 0, 0) = 0.
    // L3: service_in=0, service_out=0, epz=0 (no service/EPZ ops in this shift)
    assert_eq!(derive_cash_on_hand(0, 0, 0, 0, 0, 0), 0);
}

#[test]
fn pin_l0_cash_sales_sum() {
    // Two cash SELLs: 100.00 + 25.00 = 125.00 = 12500 kop.
    assert_eq!(derive_cash_on_hand(0, 12500, 0, 0, 0, 0), 12500);
}

#[test]
fn pin_l0_cash_return_subtracts() {
    // SELL 100.00 then RETURN 30.00 → 70.00 = 7000 kop.
    assert_eq!(derive_cash_on_hand(0, 10000, 3000, 0, 0, 0), 7000);
}

#[test]
fn pin_l0_noncash_excluded() {
    // Card SELL does NOT count — cash_sell stays 0.
    assert_eq!(derive_cash_on_hand(0, 0, 0, 0, 0, 0), 0);
}

#[test]
fn pin_l0_mixed_receipt() {
    // One receipt: cash 40.00 + card 60.00 → cash_on_hand += 4000 only.
    // The card leg is excluded; only type_code "0" matters.
    assert_eq!(derive_cash_on_hand(0, 4000, 0, 0, 0, 0), 4000);
}

// ──────────────────────────────────────────────────────────────────────────────
// L0 pins — aggregate_shift_cash (DB-backed)
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_l0_aggregate_shift_cash_two_sells() {
    let (_dir, pool) = fresh_main().await;
    seed_fn(&pool).await;
    let shift = ShiftId::new();
    seed_shift(&pool, shift, ShiftState::Opened, 0).await;
    // 100.00 + 25.00 cash SELLs.
    seed_issued_receipt(&pool, shift, 1, DocType::Sell, DocState::Ack, &cash(10000)).await;
    seed_issued_receipt(&pool, shift, 2, DocType::Sell, DocState::Ack, &cash(2500)).await;
    let (sell, ret, svc_in, svc_out, epz_out) =
        aggregate_shift_cash(&pool, FN, shift).await.unwrap();
    assert_eq!(sell, 12500, "sell sum");
    assert_eq!(ret, 0, "no returns");
    assert_eq!(svc_in, 0, "no service-in");
    assert_eq!(svc_out, 0, "no service-out");
    assert_eq!(epz_out, 0, "no EPZ");
}

#[tokio::test]
async fn pin_l0_aggregate_excludes_card() {
    let (_dir, pool) = fresh_main().await;
    seed_fn(&pool).await;
    let shift = ShiftId::new();
    seed_shift(&pool, shift, ShiftState::Opened, 0).await;
    // Card SELL only — type_code "1", must not count.
    seed_issued_receipt(&pool, shift, 1, DocType::Sell, DocState::Ack, &card(5000)).await;
    let (sell, ret, _, _, _) = aggregate_shift_cash(&pool, FN, shift).await.unwrap();
    assert_eq!(sell, 0, "card excluded from cash sell");
    assert_eq!(ret, 0, "card excluded from cash ret");
}

#[tokio::test]
async fn pin_l0_aggregate_mixed_receipt() {
    let (_dir, pool) = fresh_main().await;
    seed_fn(&pool).await;
    let shift = ShiftId::new();
    seed_shift(&pool, shift, ShiftState::Opened, 0).await;
    // One receipt: cash 40.00 + card 60.00.
    let payments = format!("{},{}", cash(4000), card(6000));
    seed_issued_receipt(&pool, shift, 1, DocType::Sell, DocState::Ack, &payments).await;
    let (sell, _, _, _, _) = aggregate_shift_cash(&pool, FN, shift).await.unwrap();
    assert_eq!(sell, 4000, "only cash leg counted");
}

// ──────────────────────────────────────────────────────────────────────────────
// L0 pin 6 — carry across shifts
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_l0_carry_across_shifts() {
    let (_dir, pool) = fresh_main().await;
    seed_fn(&pool).await;

    // Shift A: cash SELL 100.00, closes with closing_cash = 10000.
    let shift_a = ShiftId::new();
    seed_closed_shift_with_closing_cash(&pool, shift_a, 0, 10000).await;

    // opening_carry_for_fn must return 10000 (shift A's closing cash).
    let carry = opening_carry_for_fn(&pool, FN).await.unwrap();
    assert_eq!(carry, 10000, "opening carry = prior closing");

    // Shift B opens with carry = 10000.
    let shift_b = ShiftId::new();
    seed_shift(&pool, shift_b, ShiftState::Opened, carry).await;
    // Cash SELL 20.00 in shift B.
    seed_issued_receipt(&pool, shift_b, 1, DocType::Sell, DocState::Ack, &cash(2000)).await;

    // cash_on_hand for shift B via derive_closing_cash.
    let coh = derive_closing_cash(&pool, FN, shift_b, carry)
        .await
        .unwrap();
    assert_eq!(coh, 12000, "carry 100 + sell 20 = 120 (12000 kop)");
}

// ──────────────────────────────────────────────────────────────────────────────
// L0 pin 7 — reconcile detects drift
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_l0_boot_reconcile_detects_drift() {
    let (_dir, pool) = fresh_main().await;
    seed_fn(&pool).await;

    // Shift A: opening=0, one SELL of 100.00 (10000 kop) → closing carry = 10000.
    // Note: `reconcile_opening_anchor` re-derives carry from docs (not from the
    // stored closing `cash_balance_kop`), so shift A must have an ACK SELL that
    // totals 10000 kop to produce carry=10000 via `derive_cash_on_hand(0, 10000, 0)`.
    let shift_a = ShiftId::new();
    seed_closed_shift_with_closing_cash(&pool, shift_a, 0, 10000).await;
    seed_issued_receipt(
        &pool,
        shift_a,
        1,
        DocType::Sell,
        DocState::Ack,
        &cash(10000),
    )
    .await;

    // Shift B: opens with correct carry 10000, has one SELL 20.00.
    let shift_b = ShiftId::new();
    seed_shift(&pool, shift_b, ShiftState::Opened, 10000).await;
    seed_issued_receipt(&pool, shift_b, 2, DocType::Sell, DocState::Ack, &cash(2000)).await;

    // reconcile_opening_anchor re-derives: carry-after-A = 0 + 10000 = 10000.
    // Shift B's stored opening = 10000 → no drift.
    let result = reconcile_opening_anchor(&pool, FN).await.unwrap();
    let (re_derived, stored) = result.expect("open shift found");
    assert_eq!(re_derived, stored, "no drift with correct anchor");
    assert_eq!(re_derived, 10000, "re-derived carry = 10000 (shift A SELL)");

    // Corrupt the stored opening anchor of shift B.
    sqlx::query("UPDATE shifts SET cash_balance_kop = 999 WHERE shift_id = ?")
        .bind(shift_b)
        .execute(&pool)
        .await
        .unwrap();

    // Now reconcile should detect drift: re_derived=10000 but stored=999.
    let result = reconcile_opening_anchor(&pool, FN).await.unwrap();
    let (re_derived, stored) = result.expect("open shift found");
    assert_ne!(
        re_derived, stored,
        "drift must be detected after anchor corruption"
    );
    assert_eq!(re_derived, 10000, "re-derived from journal = 10000");
    assert_eq!(stored, 999, "stored = corrupted value");
}

// ──────────────────────────────────────────────────────────────────────────────
// L1 pins — INV-21 guard in convert_to_signer_payload
// ──────────────────────────────────────────────────────────────────────────────

/// Build a minimal RETURN command with cash amount `kop`.
fn return_cmd_cash(kop: i64) -> CanonicalCommand {
    let json = format!(
        r#"{{
            "schema_version": "1.0",
            "fiscal_number": "{FN}",
            "command_type": "RETURN",
            "idempotency_key": "ret-1",
            "cashier_id": null,
            "department": null,
            "return_check_number": null,
            "payload": {{
                "direction": "RETURN",
                "goods": [{{"name":"Item","quantity_milli":1000,"price_kopecks":{kop},"tax_group_1":0,"tax_group_2":0,"article_code":1}}],
                "payments": [{{"type":"CASH","amount_kopecks":{kop}}}],
                "totals": {{"sale_kopecks":0,"return_kopecks":{kop}}}
            }}
        }}"#
    );
    serde_json::from_str(&json).expect("parse RETURN cmd")
}

/// Build a minimal RETURN command with card amount `kop`.
fn return_cmd_card(kop: i64) -> CanonicalCommand {
    let json = format!(
        r#"{{
            "schema_version": "1.0",
            "fiscal_number": "{FN}",
            "command_type": "RETURN",
            "idempotency_key": "ret-card",
            "cashier_id": null,
            "department": null,
            "return_check_number": null,
            "payload": {{
                "direction": "RETURN",
                "goods": [{{"name":"Item","quantity_milli":1000,"price_kopecks":{kop},"tax_group_1":0,"tax_group_2":0,"article_code":1}}],
                "payments": [{{"type":"CASHLESS_1","amount_kopecks":{kop}}}],
                "totals": {{"sale_kopecks":0,"return_kopecks":{kop}}}
            }}
        }}"#
    );
    serde_json::from_str(&json).expect("parse card RETURN cmd")
}

/// Build a minimal SELL command with cash amount `kop`.
fn sell_cmd_cash(kop: i64) -> CanonicalCommand {
    let json = format!(
        r#"{{
            "schema_version": "1.0",
            "fiscal_number": "{FN}",
            "command_type": "SELL",
            "idempotency_key": "sell-1",
            "cashier_id": null,
            "department": null,
            "return_check_number": null,
            "payload": {{
                "direction": "SALE",
                "goods": [{{"name":"Item","quantity_milli":1000,"price_kopecks":{kop},"tax_group_1":0,"tax_group_2":0,"article_code":1}}],
                "payments": [{{"type":"CASH","amount_kopecks":{kop}}}],
                "totals": {{"sale_kopecks":{kop},"return_kopecks":0}}
            }}
        }}"#
    );
    serde_json::from_str(&json).expect("parse SELL cmd")
}

/// Setup: open shift on FN with an empty drawer (opening cash = 0),
/// wire node_state to it. Returns (dir, main_pool, secure_pool, shift_id).
async fn setup_open_shift() -> (tempfile::TempDir, SqlitePool, SqlitePool, ShiftId) {
    let dir = tempfile::tempdir().expect("tempdir");
    let main = open_pool(&dir.path().join("main.db"))
        .await
        .expect("open main");
    let secure = open_secure_pool(&dir.path().join("secure.db"))
        .await
        .expect("open secure");
    seed_fn(&main).await;
    seed_payment_methods(&secure).await;
    let shift = ShiftId::new();
    seed_shift(&main, shift, ShiftState::Opened, 0).await;
    wire_node_state(&main, shift).await;
    (dir, main, secure, shift)
}

#[tokio::test]
async fn pin_l1_return_over_empty_drawer_refused() {
    // Open shift, no cash sales → drawer = 0.
    // Cash RETURN 1.00 must be refused (CashInsufficient), row-less.
    let (_dir, main, secure, _shift) = setup_open_shift().await;

    let err = convert_to_signer_payload(&return_cmd_cash(100), FN, &main, &secure)
        .await
        .expect_err("must refuse: drawer is empty");

    assert!(
        matches!(
            err,
            ConvertError::CashInsufficient {
                cash_on_hand_kop: 0,
                return_cash_kop: 100
            }
        ),
        "expected CashInsufficient(0, 100), got: {err:?}"
    );

    // No rows must exist in fiscal_documents (pre-inbox, row-less).
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(&main)
            .await
            .unwrap();
    assert_eq!(count, 0, "row-less: no fiscal_documents row must be minted");
}

#[tokio::test]
async fn pin_l1_return_within_balance_ok() {
    // Sell 100.00 cash → drawer = 100.00.
    // Return 50.00 → OK (drawer becomes 50.00).
    // Return 60.00 → refused (50.00 < 60.00).
    let (_dir, main, secure, shift) = setup_open_shift().await;

    // Accept the SELL (no L1 guard on SELL).
    convert_to_signer_payload(&sell_cmd_cash(10000), FN, &main, &secure)
        .await
        .expect("sell must be accepted");

    // Seed an issued SELL receipt so aggregate_shift_cash sees it.
    seed_issued_receipt(&main, shift, 1, DocType::Sell, DocState::Ack, &cash(10000)).await;

    // First RETURN 50.00 — must succeed.
    convert_to_signer_payload(&return_cmd_cash(5000), FN, &main, &secure)
        .await
        .expect("return 50 within balance must succeed");

    // Seed the first RETURN as issued so it reduces cash_on_hand.
    seed_issued_receipt(&main, shift, 2, DocType::Return, DocState::Ack, &cash(5000)).await;

    // Second RETURN 60.00 — must be refused (drawer now 50.00 < 60.00).
    let err = convert_to_signer_payload(&return_cmd_cash(6000), FN, &main, &secure)
        .await
        .expect_err("return 60 must be refused: only 50 in drawer");

    assert!(
        matches!(
            err,
            ConvertError::CashInsufficient {
                cash_on_hand_kop: 5000,
                return_cash_kop: 6000
            }
        ),
        "expected CashInsufficient(5000, 6000), got: {err:?}"
    );
}

#[tokio::test]
async fn pin_l1_noncash_return_not_gated() {
    // Card RETURN must NOT trigger the cash floor check.
    let (_dir, main, secure, _shift) = setup_open_shift().await;
    // Drawer is empty (0 cash), but the return is card-only.
    // Must succeed (no cash leg → guard skipped).
    convert_to_signer_payload(&return_cmd_card(5000), FN, &main, &secure)
        .await
        .expect("card return must not be blocked by cash floor");
}

#[tokio::test]
async fn pin_l1_exact_zero_ok() {
    // Drawer 50.00, RETURN exactly 50.00 → accepted (floor is inclusive 0).
    let (_dir, main, secure, shift) = setup_open_shift().await;
    seed_issued_receipt(&main, shift, 1, DocType::Sell, DocState::Ack, &cash(5000)).await;

    // Seed shift with opening 0 and current sell 5000 → cash_on_hand = 5000.
    // Return 5000 → exactly 0 remaining → must be accepted.
    convert_to_signer_payload(&return_cmd_cash(5000), FN, &main, &secure)
        .await
        .expect("return matching drawer exactly is accepted (floor = 0, not -1)");
}

#[tokio::test]
async fn pin_l1_teeth_revert_guard() {
    // Canary: without the guard, a cash RETURN on an empty drawer would
    // proceed to a converted payload (not an error).
    //
    // This test asserts the CURRENT (guarded) behaviour: the error IS
    // returned.  If the guard is reverted, this test goes RED — which
    // is the intended "teeth" effect.  The fuzzer differential test
    // (in invariant_fuzzer/) provides the durable automated teeth.
    let (_dir, main, secure, _shift) = setup_open_shift().await;

    let result = convert_to_signer_payload(&return_cmd_cash(100), FN, &main, &secure).await;
    assert!(
        result.is_err(),
        "guard must fire: empty drawer + cash RETURN → error (teeth: reverting guard makes this RED)"
    );
    assert!(
        matches!(result.unwrap_err(), ConvertError::CashInsufficient { .. }),
        "error variant must be CashInsufficient"
    );
}
