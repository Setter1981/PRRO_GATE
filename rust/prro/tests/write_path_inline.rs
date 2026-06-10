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

use prro::db::models::enums::{
    DocType, FiscalMode, NodeMode, OfflineSessionState, Protocol, ShiftState,
};
use prro::db::models::ids::{OfflineSessionId, RequestId, ShiftId};
use prro::db::repositories::ingress_inbox::{self as inbox, InboxRow, NewInboxEntry};
use prro::db::repositories::{fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig};
use prro::db::{open_pool, open_secure_pool};
use prro::runtime::ingress::seam::FiscalError;
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
    seed_inbox_op(pool, DocType::Sell.as_str()).await
}

/// Seed a NEW inbox row with the given `operation_type` + return the matching
/// `InboxRow`. The SELL payload is reused verbatim — it is never parsed on the
/// fail-closed Z/SHIFT_OPEN paths (they return before build_canonical), and
/// `payload_sha256_canonical = sha256(payload)` keeps the SELL path's gates happy.
async fn seed_inbox_op(pool: &SqlitePool, operation_type: &str) -> InboxRow {
    let req_id = RequestId::new();
    let request_id: [u8; 16] = *req_id.as_bytes();
    let payload_sha256_canonical: [u8; 32] = Sha256::digest(SELL_PAYLOAD.as_bytes()).into();
    let idempotency_key = format!("idem-a2-1b-{operation_type}");
    inbox::insert(
        pool,
        &NewInboxEntry {
            request_id,
            fiscal_number: FN.into(),
            protocol: Protocol::Rest,
            operation_type: operation_type.into(),
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
        operation_type: operation_type.into(),
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

/// Read the inbox row's status for the seeded FN (one row per test pool).
async fn read_inbox_status(pool: &SqlitePool, request_id: &[u8; 16]) -> String {
    sqlx::query_scalar("SELECT status FROM ingress_inbox WHERE request_id = ?")
        .bind(&request_id[..])
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn audit_count(pool: &SqlitePool, event_type: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type = ?")
        .bind(event_type)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Count audit rows whose event_type starts with `INLINE_` (the inline
/// orchestrator's own terminalise/fail-closed audits).
async fn inline_audit_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type LIKE 'INLINE_%'")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// node_state with an arbitrary mode + shift_state (for the refusal fixtures).
async fn seed_node_state(pool: &SqlitePool, mode: NodeMode, shift_state: ShiftState) {
    sqlx::query(
        "INSERT INTO node_state \
         (fiscal_number, mode, shift_state, next_lnd, backend_profile_id, transport_profile_id) \
         VALUES (?, ?, ?, 1, 'b', 't')",
    )
    .bind(FN)
    .bind(mode)
    .bind(shift_state)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_node_state_offline(pool: &SqlitePool, shift_id: ShiftId) {
    sqlx::query(
        "INSERT INTO node_state \
         (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
          backend_profile_id, transport_profile_id) \
         VALUES (?, ?, ?, ?, 1, 'b', 't')",
    )
    .bind(FN)
    .bind(NodeMode::Offline)
    .bind(ShiftState::Opened)
    .bind(shift_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_open_offline_session(pool: &SqlitePool) -> OfflineSessionId {
    let session_id = OfflineSessionId::new();
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, ?, '2026-06-09T00:00:00Z')",
    )
    .bind(session_id)
    .bind(FN)
    .bind(OfflineSessionState::Open.as_str())
    .execute(pool)
    .await
    .unwrap();
    session_id
}

/// Seed one AVAILABLE offline code (consumed_at NULL by default).
async fn seed_offline_code(pool: &SqlitePool, code_lnd: i64) {
    sqlx::query("INSERT INTO offline_codes(fiscal_number, code_lnd) VALUES (?, ?)")
        .bind(FN)
        .bind(code_lnd)
        .execute(pool)
        .await
        .unwrap();
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

/// **A2.1b-core incr.3 — transient send → 202.** A transient wire failure
/// routes the doc to `ErrorRetryable`; the orchestrator returns
/// `Ok(FiscalOutcome{document_state: ErrorRetryable, fiscal_id: None})` → 202
/// IN_PROGRESS (NOT a terminal `Err`/500). Drain/B1 re-drives later.
#[tokio::test]
async fn online_sell_transient_send_returns_in_progress() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_online(&pool, shift_id).await;
    let row = seed_inbox_sell(&pool).await;

    // send_chk transient failure → stage_send routes to ErrorRetryable.
    let dps = DualStub::new(
        Err(DpsError::Transport("net blip".into())),
        Ok(ack(SERVER_FISCAL_NO, vec![0x01])), // unused on this path
    );
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;

    let outcome = inline::run(&pool, &pool_secure, &dps, &sign_ctx, &fn_sign, &guard, &row)
        .await
        .expect("transient send is IN_PROGRESS (202), never a terminal Err");

    assert_eq!(
        outcome.document_state,
        prro::db::models::enums::DocState::ErrorRetryable
    );
    assert_eq!(outcome.fiscal_id, None);
    assert_eq!(read_doc_state(&pool, FN).await, "ERROR_RETRYABLE");
}

/// **A2.1b-core incr.3 — inline lastChk Hold → 202 Sent.** The wire send
/// succeeded (doc at `Sent`), but the inline lastChk is transient (no KVT1
/// evidence yet) → `online_confirm` Hold → `Ok(FiscalOutcome{document_state:
/// Sent})` → 202 IN_PROGRESS. The doc is NOT advanced and NOT fake-ACK'd; the
/// drain/B1 completes the KVT2 confirm later.
#[tokio::test]
async fn online_sell_lastchk_hold_returns_sent_in_progress() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_online(&pool, shift_id).await;
    let row = seed_inbox_sell(&pool).await;

    // send_chk succeeds (→ Sent), but last_chk is transient → online_confirm Hold.
    let dps = DualStub::new(
        Ok(ack(SERVER_FISCAL_NO, vec![])),
        Err(DpsError::Transport("lastChk blip".into())),
    );
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;

    let outcome = inline::run(&pool, &pool_secure, &dps, &sign_ctx, &fn_sign, &guard, &row)
        .await
        .expect("Hold leaves the doc at Sent → 202, never a terminal Err");

    assert_eq!(
        outcome.document_state,
        prro::db::models::enums::DocState::Sent
    );
    // server_fiscal_no IS known (stage_send stamped it) — informational on 202.
    assert_eq!(outcome.fiscal_id.as_deref(), Some(SERVER_FISCAL_NO));
    // Doc stays Sent — NOT advanced, NOT fake-ACK'd.
    assert_eq!(read_doc_state(&pool, FN).await, "SENT");
}

/// **A2.1b-core incr.4 — offline-local-ack is a SUCCESS (200, not Err).** With
/// the node Offline + an open session + an available code, dispatch routes the
/// signed doc to `stage_offline_ack`, which acquires a code and lands it at
/// `OFFLINE_LOCAL_ACK`. The orchestrator returns
/// `Ok(FiscalOutcome{document_state: OfflineLocalAck, fiscal_id: None})` — NOT
/// a `FiscalError` (a transport/ambiguous-DPS auto-offline is success, per the
/// seam contract).
#[tokio::test]
async fn offline_sell_is_offline_local_ack_success() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_offline(&pool, shift_id).await;
    seed_open_offline_session(&pool).await;
    seed_offline_code(&pool, 1).await;
    let row = seed_inbox_sell(&pool).await;

    // DPS is never called on the offline path (dispatch terminates at offline-ack).
    let dps = DualStub::new(
        Ok(ack(SERVER_FISCAL_NO, vec![])),
        Ok(ack(SERVER_FISCAL_NO, vec![])),
    );
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;

    let outcome = inline::run(&pool, &pool_secure, &dps, &sign_ctx, &fn_sign, &guard, &row)
        .await
        .expect("offline auto-ack is a SUCCESS (200), never a FiscalError");

    assert_eq!(
        outcome.document_state,
        prro::db::models::enums::DocState::OfflineLocalAck
    );
    assert_eq!(outcome.fiscal_id, None);
    assert_eq!(read_doc_state(&pool, FN).await, "OFFLINE_LOCAL_ACK");
}

/// **A2.1b-core incr.5 — Z-class is fail-closed (501) + terminalises the inbox,
/// NO fiscal_documents.** A Z_REPORT is out of the SELL/RETURN core: the
/// orchestrator fail-closes BEFORE build/acquire with `ZSurfaceNotReady`,
/// leasing+REJECTing the inbox atomically (so replay reads it as failure, not
/// 202-forever) and minting no ledger row.
#[tokio::test]
async fn z_report_is_fail_closed_and_terminalises_inbox() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let row = seed_inbox_op(&pool, "Z_REPORT").await;

    let dps = DualStub::new(
        Ok(ack(SERVER_FISCAL_NO, vec![])),
        Ok(ack(SERVER_FISCAL_NO, vec![])),
    );
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;

    let err = inline::run(&pool, &pool_secure, &dps, &sign_ctx, &fn_sign, &guard, &row)
        .await
        .expect_err("Z-class is fail-closed (501), not a success");
    assert!(matches!(err, FiscalError::ZSurfaceNotReady { .. }));
    // Inbox terminal + audited (replay reads REJECTED as failure, not 202).
    assert_eq!(read_inbox_status(&pool, &row.request_id).await, "REJECTED");
    assert_eq!(audit_count(&pool, "INLINE_Z_SURFACE_NOT_READY").await, 1);
    // NO fiscal_documents minted for a refusal (Q2).
    let docs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(docs, 0, "fail-closed must mint NO fiscal_documents");
}

/// **A2.1b-core incr.5 — SHIFT_OPEN is fail-closed (422) + terminalises the
/// inbox.** SHIFT_OPEN is out of the SELL/RETURN core (A2.2 owns it):
/// `ShiftGuardRefused{SHIFT_OPEN_NOT_SUPPORTED}` (422), inbox REJECTED+audited,
/// no fiscal_documents. The 422 mapping is pinned by inline_map's round-trip test.
#[tokio::test]
async fn shift_open_is_fail_closed_and_terminalises_inbox() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let row = seed_inbox_op(&pool, "SHIFT_OPEN").await;

    let dps = DualStub::new(
        Ok(ack(SERVER_FISCAL_NO, vec![])),
        Ok(ack(SERVER_FISCAL_NO, vec![])),
    );
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;

    let err = inline::run(&pool, &pool_secure, &dps, &sign_ctx, &fn_sign, &guard, &row)
        .await
        .expect_err("SHIFT_OPEN is fail-closed (422) in the SELL/RETURN core");
    match err {
        FiscalError::ShiftGuardRefused { code, .. } => {
            assert_eq!(code, "SHIFT_OPEN_NOT_SUPPORTED");
        }
        other => panic!("expected ShiftGuardRefused{{SHIFT_OPEN_NOT_SUPPORTED}}, got {other:?}"),
    }
    assert_eq!(read_inbox_status(&pool, &row.request_id).await, "REJECTED");
    assert_eq!(
        audit_count(&pool, "INLINE_SHIFT_OPEN_NOT_SUPPORTED").await,
        1
    );
}

/// **A2.1b-core incr.5b — stage_acquire Rejected: map ONLY, NO double-terminalise
/// (CORRECTION 5).** A SELL with no open shift → stage_acquire guard →
/// `Rejected(ShiftNotOpen)`. stage_acquire ALREADY terminalised the inbox
/// (REJECTED + its own audit); the inline arm maps to `ShiftNotOpen` (422) and
/// must NOT emit a second terminalise/audit. Also a four-variant gate row
/// (ShiftNotOpen → inbox non-NEW + audited).
#[tokio::test]
async fn acquire_rejected_maps_only_no_double_terminalise() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    // Node Online but shift CLOSED → a SELL has no open shift → Rejected.
    seed_node_state(&pool, NodeMode::Online, ShiftState::Closed).await;
    let row = seed_inbox_sell(&pool).await;

    let dps = DualStub::new(
        Ok(ack(SERVER_FISCAL_NO, vec![])),
        Ok(ack(SERVER_FISCAL_NO, vec![])),
    );
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;

    let err = inline::run(&pool, &pool_secure, &dps, &sign_ctx, &fn_sign, &guard, &row)
        .await
        .expect_err("a SELL with no open shift is refused");
    assert!(matches!(err, FiscalError::ShiftNotOpen { .. }));
    // Inbox terminal (by stage_acquire). No fiscal_documents (guard-refused).
    assert_eq!(read_inbox_status(&pool, &row.request_id).await, "REJECTED");
    let docs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(docs, 0);
    // CORRECTION 5: the inline arm did NOT re-terminalise — NO INLINE_* audit
    // (stage_acquire owns the single REJECTED + audit).
    assert_eq!(
        inline_audit_count(&pool).await,
        0,
        "stage_acquire Rejected must NOT trigger a second (inline) terminalise/audit"
    );
}

/// **A2.1b-core incr.5b — offline refusal is granular (CORRECTION 4): a
/// NoActiveSession is a STRUCTURAL breach → Internal/500, NOT a blanket 503,
/// and terminalises the inbox.** Node Offline (dispatch routes offline) but no
/// open offline session → stage_offline_ack `Refused(NoActiveSession)` →
/// `map_offline_refusal` → Internal/OFFLINE_NO_ACTIVE_SESSION. Exercises the
/// post-acquire `terminalise_inbox`.
#[tokio::test]
async fn offline_no_session_is_internal_500_and_terminalises_inbox() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_offline(&pool, shift_id).await; // node Offline → dispatch offline
                                                    // NO open offline session seeded → stage_offline_ack refuses NoActiveSession.
    let row = seed_inbox_sell(&pool).await;

    let dps = DualStub::new(
        Ok(ack(SERVER_FISCAL_NO, vec![])),
        Ok(ack(SERVER_FISCAL_NO, vec![])),
    );
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;

    let err = inline::run(&pool, &pool_secure, &dps, &sign_ctx, &fn_sign, &guard, &row)
        .await
        .expect_err("offline-ack with no session is a structural refusal");
    match err {
        FiscalError::Internal { code, .. } => assert_eq!(code, "OFFLINE_NO_ACTIVE_SESSION"),
        other => panic!("expected Internal{{OFFLINE_NO_ACTIVE_SESSION}}, got {other:?}"),
    }
    assert_eq!(read_inbox_status(&pool, &row.request_id).await, "REJECTED");
    assert_eq!(audit_count(&pool, "INLINE_OFFLINE_REFUSED").await, 1);
}
