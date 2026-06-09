//! RS-3 A2.1b-core — inline `fiscalize` orchestrator integration tests.
//!
//! Drives `services::write_path::inline::run` end-to-end against a freshly
//! migrated pool. The first fixture is the ONLINE-ACK happy path (SELL):
//! build_canonical → stage_acquire → stage_sign → dispatch(Online) →
//! stage_send → online_confirm → advance_to_ack → FiscalOutcome{Ack}.

mod common;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use prro::db::models::enums::{DocType, FiscalMode, NodeMode, Protocol, ShiftState};
use prro::db::models::ids::{RequestId, ShiftId};
use prro::db::repositories::ingress_inbox::{self as inbox, InboxRow, NewInboxEntry};
use prro::db::repositories::{fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig};
use prro::db::{open_pool, open_secure_pool};
use prro::services::write_path::inline;
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{CheckAck, CheckEnvelope, CheckSignBlob, RroInfo, StatusSnapshot};
use prro::transports::dps::error::DpsError;
use sqlx::SqlitePool;

use common::det_signing_ctx;

const FN: &str = "4000000001";
const CASHIER: &str = "test-cashier";
const DRIVER: &str = "drv-test";
const SERVER_FISCAL_NO: &str = "DPS-FN-ONLINE-1";

/// Minimal valid SELL `CheckJson` (one item, one payment; no tax_group_1 so
/// `derive_check_tax_summaries` short-circuits without TaxMappingNotWired).
const SELL_PAYLOAD: &str = r#"{"items":[{"code":"item-1","name":"Test item","price_kop":15000,"quantity_thousandths":1000,"sum_kop":15000}],"payments":[{"name":"Cash","sum_kop":15000,"type_code":"0"}]}"#;
const TOTAL_KOP: i64 = 15000;

// ─── Dual-queue DPS stub: send_chk then last_chk ────────────────────────

struct DualStub {
    send_chk: Mutex<Option<Result<CheckAck, DpsError>>>,
    last_chk: Mutex<Option<Result<CheckAck, DpsError>>>,
}

impl DualStub {
    fn new(send: Result<CheckAck, DpsError>, last: Result<CheckAck, DpsError>) -> Self {
        Self {
            send_chk: Mutex::new(Some(send)),
            last_chk: Mutex::new(Some(last)),
        }
    }
}

#[async_trait]
impl DpsChannel for DualStub {
    async fn send_chk(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        self.send_chk
            .lock()
            .unwrap()
            .take()
            .expect("DualStub.send_chk: called more than once")
    }
    async fn last_chk(&self, _: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        self.last_chk
            .lock()
            .unwrap()
            .take()
            .expect("DualStub.last_chk: called more than once")
    }
    async fn ping(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        unreachable!("stub: ping not exercised");
    }
    async fn status_rro(&self, _: &CheckSignBlob) -> Result<StatusSnapshot, DpsError> {
        unreachable!("stub: status_rro not exercised");
    }
    async fn info_rro(&self, _: &CheckSignBlob) -> Result<RroInfo, DpsError> {
        unreachable!("stub: info_rro not exercised");
    }
}

fn ack(id: &str, data_sign: Vec<u8>) -> CheckAck {
    CheckAck {
        id: id.to_string(),
        id_sign: vec![],
        data_sign,
    }
}

// ─── Fixture ────────────────────────────────────────────────────────────

async fn fresh_pool() -> SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a2_1b.db");
    std::mem::forget(dir);
    open_pool(&path).await.unwrap()
}

async fn fresh_secure_pool() -> SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a2_1b-secure.db");
    std::mem::forget(dir);
    open_secure_pool(&path).await.unwrap()
}

async fn seed_fn_config(pool: &SqlitePool) {
    fn_repo::insert(
        pool,
        &NewFnConfig {
            fiscal_number: FN.into(),
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
    .unwrap();
}

async fn seed_node_state_online(pool: &SqlitePool, shift_id: ShiftId) {
    sqlx::query(
        "INSERT INTO node_state \
         (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
          backend_profile_id, transport_profile_id) \
         VALUES (?, ?, ?, ?, 1, 'b', 't')",
    )
    .bind(FN)
    .bind(NodeMode::Online)
    .bind(ShiftState::Opened)
    .bind(shift_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_open_shift(pool: &SqlitePool) -> ShiftId {
    let shift_id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, serial, state, open_mode, \
            cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED', 'ONLINE', 0, ?)",
    )
    .bind(shift_id)
    .bind(FN)
    .bind(CASHIER)
    .execute(pool)
    .await
    .unwrap();
    shift_id
}

/// Seed a NEW SELL inbox row + return the matching [`InboxRow`] to hand to
/// `inline::run`. The `payload_sha256_canonical` is `sha256(payload_json)` so
/// both `build_canonical`'s integrity gate and `stage_acquire`'s cross-check
/// pass.
async fn seed_inbox_sell(pool: &SqlitePool) -> InboxRow {
    let req_id = RequestId::new();
    let request_id: [u8; 16] = *req_id.as_bytes();
    let payload_sha256_canonical: [u8; 32] = Sha256::digest(SELL_PAYLOAD.as_bytes()).into();
    let idempotency_key = "idem-a2-1b-online-sell".to_string();
    inbox::insert(
        pool,
        &NewInboxEntry {
            request_id,
            fiscal_number: FN.into(),
            protocol: Protocol::Rest,
            operation_type: DocType::Sell.as_str().into(),
            idempotency_key: idempotency_key.clone(),
            payload_json: SELL_PAYLOAD.into(),
            payload_sha256_canonical,
            correlation_id: None,
            signed_by_cashier_id: Some(CASHIER.into()),
            driver_id: Some(DRIVER.into()),
            business_ts: Some("2026-06-09T12:00:00Z".into()),
            total_sum_kop: Some(TOTAL_KOP),
        },
    )
    .await
    .unwrap();
    InboxRow {
        request_id,
        fiscal_number: FN.into(),
        protocol: Protocol::Rest,
        operation_type: DocType::Sell.as_str().into(),
        idempotency_key,
        status: "NEW".into(),
        payload_json: SELL_PAYLOAD.into(),
        payload_sha256_canonical,
        correlation_id: None,
        received_at: "2026-06-09T12:00:00Z".into(),
        signed_by_cashier_id: Some(CASHIER.into()),
        driver_id: Some(DRIVER.into()),
        business_ts: Some("2026-06-09T12:00:00Z".into()),
        total_sum_kop: Some(TOTAL_KOP),
    }
}

async fn read_doc_state(pool: &SqlitePool, fiscal_number: &str) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE fiscal_number = ?")
        .bind(fiscal_number)
        .fetch_one(pool)
        .await
        .unwrap()
}

// ─── Tests ──────────────────────────────────────────────────────────────

/// **A2.1b-core happy path** — an online SELL drives the full inline chain to
/// terminal ACK: `FiscalOutcome{document_state: Ack, fiscal_id: Some}`.
#[tokio::test]
async fn online_sell_reaches_ack() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_online(&pool, shift_id).await;
    let row = seed_inbox_sell(&pool).await;

    let dps = DualStub::new(
        Ok(ack(SERVER_FISCAL_NO, vec![])), // send_chk: data_sign discarded by stage_send
        Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF])), // last_chk: KVT1 evidence
    );
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);

    // A4 gate proof — the binding (A2.4) holds App::acquire_fn_gate; the test
    // holds a standalone guard for the duration of the call.
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;

    let outcome = inline::run(&pool, &pool_secure, &dps, &sign_ctx, &fn_sign, &guard, &row)
        .await
        .expect("online SELL must reach a terminal ACK FiscalOutcome");

    assert_eq!(
        outcome.document_state,
        prro::db::models::enums::DocState::Ack
    );
    assert_eq!(outcome.fiscal_id.as_deref(), Some(SERVER_FISCAL_NO));
    assert_eq!(read_doc_state(&pool, FN).await, "ACK");
}
