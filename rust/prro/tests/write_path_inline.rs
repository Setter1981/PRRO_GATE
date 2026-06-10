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

use prro::crypto::errors::{CryptoError, SignKind};
use prro::crypto::provider::{
    CertDer, CryptoProvider, DstuVerifyResult, SignCmsRequest, SignedCmsBytes,
};
use prro::crypto::session::SigningSession;
use prro::db::models::enums::{
    DocType, FiscalMode, NodeMode, OfflineSessionState, Protocol, ShiftState,
};
use prro::db::models::ids::{OfflineSessionId, RequestId, ShiftId};
use prro::db::repositories::ingress_inbox::{self as inbox, InboxRow, NewInboxEntry};
use prro::db::repositories::{fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig};
use prro::db::{open_pool, open_secure_pool};
use prro::runtime::ingress::seam::FiscalError;
use prro::services::write_path::inline;
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{CheckAck, CheckEnvelope, CheckSignBlob, RroInfo, StatusSnapshot};
use prro::transports::dps::error::{AuthorizationKind, DpsError};
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

// ─── Failing crypto provider (the SignFailure gate fixture) ─────────────

/// `sign_cms_detached` always errors with a crypto-class failure — drives the
/// `SignError::Crypto` → `FiscalError::SignFailure` gate arm.
struct FailingCrypto;

#[async_trait]
impl CryptoProvider for FailingCrypto {
    async fn sign_cms_detached(
        &self,
        _: SignCmsRequest<'_>,
    ) -> Result<SignedCmsBytes, CryptoError> {
        Err(CryptoError::CmsSign {
            reason: SignKind::BackendError,
        })
    }
    async fn verify_dstu(
        &self,
        _: &[u8],
        _: &[u8],
        _: &[u8],
    ) -> Result<DstuVerifyResult, CryptoError> {
        unreachable!("stub: verify_dstu not exercised");
    }
    async fn unwrap_envelope(
        &self,
        _: &[u8],
        _: &[u8],
        _: &SigningSession,
    ) -> Result<Vec<u8>, CryptoError> {
        unreachable!("stub: unwrap_envelope not exercised");
    }
    async fn fetch_cert_by_ski(
        &self,
        _: &[String],
        _: &[u8; 32],
        _: std::time::Duration,
    ) -> Result<CertDer, CryptoError> {
        unreachable!("stub: fetch_cert_by_ski not exercised");
    }
}

/// `SigningContext` over [`FailingCrypto`] — mirrors `common::det_signing_ctx`
/// but every sign attempt fails at the crypto boundary.
fn failing_signing_ctx() -> SigningContext {
    SigningContext {
        provider: Arc::new(FailingCrypto) as Arc<dyn CryptoProvider>,
        session: SigningSession::new_for_test("operator-1".into(), [0u8; 32], vec![]),
        profile: prro_crypto::cms::profile::CmsProfile::Dstu4145WithGost34311Pb,
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

/// Latest audit payload for an event type (None if absent / not JSON).
async fn audit_latest_payload(pool: &SqlitePool, event_type: &str) -> Option<serde_json::Value> {
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT event_payload_json FROM audit_log \
         WHERE event_type = ? ORDER BY audit_id DESC LIMIT 1",
    )
    .bind(event_type)
    .fetch_optional(pool)
    .await
    .unwrap();
    raw.and_then(|s| serde_json::from_str(&s).ok())
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
    // Review OCF-1 pin: first-pass↔replay parity — the inline ACK carries the
    // first_kvt1_at stamp (the Sent→Kvt1 CAS wrote it during the advance).
    assert!(
        outcome.fiscal_ts.is_some(),
        "inline ACK must carry fiscal_ts (first_kvt1_at) for replay parity"
    );
    assert_eq!(read_doc_state(&pool, FN).await, "ACK");
    // Review TA-7 pin: stage_finalize marked the inbox DONE in its envelope.
    assert_eq!(read_inbox_status(&pool, &row.request_id).await, "DONE");
    // Review TA-4 pin: the attempt_no GOTCHA capture (taken from
    // StageSendOutcome::Sent BEFORE classify_send_outcome drops it) reaches
    // the advance audit payload.
    let payload = audit_latest_payload(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED")
        .await
        .expect("the advance emits its audit");
    assert_eq!(payload["attempt_no"].as_i64(), Some(1));
    assert_eq!(payload["server_fiscal_no"], SERVER_FISCAL_NO);
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
    // Review TA-7 pin: the inbox stays PROCESSING (the in-flight intermediate
    // owned by drain/B1; replay resolves 202 via the ledger).
    assert_eq!(
        read_inbox_status(&pool, &row.request_id).await,
        "PROCESSING"
    );
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
    // Review TA-7 pin: the inbox stays PROCESSING until the drain later
    // finalizes the doc to ACK (replay reads OfflineLocalAck as ACCEPTED).
    assert_eq!(
        read_inbox_status(&pool, &row.request_id).await,
        "PROCESSING"
    );
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
    // Review TA-4 pin: the audit payload's code == the returned error's code
    // (audit ↔ error agree by construction via code_of).
    let payload = audit_latest_payload(&pool, "INLINE_SHIFT_OPEN_NOT_SUPPORTED")
        .await
        .expect("terminalise audit carries a payload");
    assert_eq!(payload["code"], "SHIFT_OPEN_NOT_SUPPORTED");
    assert_eq!(payload["operation_type"], "SHIFT_OPEN");
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

/// **A2.1b-core incr.5b — four-variant gate: SignFailure.** A crypto-class
/// sign failure (`SignError::Crypto`) → `FiscalError::SignFailure` (500); the
/// inline arm terminalises the inbox (REJECTED + `INLINE_SIGN_FAIL` audit).
/// The doc row stays at its pre-sign state (no SIGNED commit) — a write-path
/// artifact, not an issued receipt.
#[tokio::test]
async fn sign_crypto_failure_is_sign_failure_and_terminalises_inbox() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_online(&pool, shift_id).await;
    let row = seed_inbox_sell(&pool).await;

    let dps = DualStub::new(
        Ok(ack(SERVER_FISCAL_NO, vec![])),
        Ok(ack(SERVER_FISCAL_NO, vec![])),
    );
    let sign_ctx = failing_signing_ctx(); // crypto always errors
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;

    let err = inline::run(&pool, &pool_secure, &dps, &sign_ctx, &fn_sign, &guard, &row)
        .await
        .expect_err("a crypto sign failure is a terminal refusal");
    assert!(matches!(err, FiscalError::SignFailure { .. }));
    assert_eq!(read_inbox_status(&pool, &row.request_id).await, "REJECTED");
    assert_eq!(audit_count(&pool, "INLINE_SIGN_FAIL").await, 1);
}

/// **A2.1b-core incr.5b — four-variant gate: DpsRejected.** DPS hard-rejects
/// the receipt (`Authorization{DocumentReject}` → stage_send routes the doc to
/// terminal `Rejected`) → `FiscalError::DpsRejected` (422); the inline arm
/// terminalises the inbox (REJECTED + `INLINE_SEND_REJECT` audit). The
/// existing `fiscal_documents` row lands at REJECTED — an existing write-path
/// artifact (Q2), which replay reads as failure.
#[tokio::test]
async fn dps_hard_reject_is_dps_rejected_and_terminalises_inbox() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_online(&pool, shift_id).await;
    let row = seed_inbox_sell(&pool).await;

    // send_chk → DocumentReject (terminal); last_chk is never reached.
    let dps = DualStub::new(
        Err(DpsError::Authorization {
            code: -1,
            kind: AuthorizationKind::DocumentReject,
            message: "document rejected by DPS".into(),
        }),
        Ok(ack(SERVER_FISCAL_NO, vec![])),
    );
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;

    let err = inline::run(&pool, &pool_secure, &dps, &sign_ctx, &fn_sign, &guard, &row)
        .await
        .expect_err("a DPS hard reject is a terminal refusal");
    assert!(matches!(err, FiscalError::DpsRejected { .. }));
    assert_eq!(read_inbox_status(&pool, &row.request_id).await, "REJECTED");
    assert_eq!(audit_count(&pool, "INLINE_SEND_REJECT").await, 1);
    // Q2: the doc row exists (acquire+sign+send ran) and rests at terminal
    // REJECTED — a failure artifact, never an issued receipt.
    assert_eq!(read_doc_state(&pool, FN).await, "REJECTED");
}

/// **A2.1b-core incr.5b — four-variant gate: OfflineRefused (503).** A
/// `Blocked` node refuses fiscalization at the acquire guard →
/// `Rejected(NodeBlocked)` → `map_rejection` → `OfflineRefused{NODE_BLOCKED}`
/// (503). stage_acquire owns the terminalise (inbox REJECTED + its own audit;
/// no inline re-terminalise), and no fiscal_documents row is minted.
#[tokio::test]
async fn blocked_node_is_offline_refused_and_inbox_terminal() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    // Node BLOCKED (operator action required) — shift open is irrelevant.
    sqlx::query(
        "INSERT INTO node_state \
         (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
          backend_profile_id, transport_profile_id) \
         VALUES (?, ?, ?, ?, 1, 'b', 't')",
    )
    .bind(FN)
    .bind(NodeMode::Blocked)
    .bind(ShiftState::Opened)
    .bind(shift_id)
    .execute(&pool)
    .await
    .unwrap();
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
        .expect_err("a BLOCKED node refuses fiscalization");
    match err {
        FiscalError::OfflineRefused { code, .. } => assert_eq!(code, "NODE_BLOCKED"),
        other => panic!("expected OfflineRefused{{NODE_BLOCKED}}, got {other:?}"),
    }
    // Terminalised by stage_acquire (no inline re-terminalise), no doc row.
    assert_eq!(read_inbox_status(&pool, &row.request_id).await, "REJECTED");
    assert_eq!(inline_audit_count(&pool).await, 0);
    let docs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(docs, 0);
}

/// **Review TA-1 — confirm-Drift arm + the #8 audited divergence.** The send
/// succeeds (doc at `Sent`), but the inline lastChk says NotFound (empty id) —
/// the server does not recognise the id it just issued. `online_confirm` →
/// Drift → terminalise + `Internal{REPLAY_LEDGER_DRIFT}` (500). The doc STAYS
/// at `Sent` while the inbox is REJECTED — the intentional, AUDITED divergence
/// surface for B1/recon (invariant #8, not silent).
#[tokio::test]
async fn lastchk_drift_terminalises_inbox_and_leaves_doc_sent() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_online(&pool, shift_id).await;
    let row = seed_inbox_sell(&pool).await;

    // send OK; lastChk returns an EMPTY id → by_server_fiscal_no → NotFound → Drift.
    let dps = DualStub::new(Ok(ack(SERVER_FISCAL_NO, vec![])), Ok(ack("", vec![0x01])));
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;

    let err = inline::run(&pool, &pool_secure, &dps, &sign_ctx, &fn_sign, &guard, &row)
        .await
        .expect_err("a Sent-fresh NotFound is structural drift (500)");
    match err {
        FiscalError::Internal { code, .. } => assert_eq!(code, "REPLAY_LEDGER_DRIFT"),
        other => panic!("expected Internal{{REPLAY_LEDGER_DRIFT}}, got {other:?}"),
    }
    // The audited divergence: inbox REJECTED, doc still SENT (NOT rolled back,
    // NOT fake-ACKed) — B1/recon owns the convergence.
    assert_eq!(read_inbox_status(&pool, &row.request_id).await, "REJECTED");
    assert_eq!(audit_count(&pool, "INLINE_CONFIRM_DRIFT").await, 1);
    assert_eq!(read_doc_state(&pool, FN).await, "SENT");
}

/// **Review TA-2 — the Noop arm resolves from the ledger (decision e),
/// end-to-end.** The inbox row is already terminal (DONE — processed by a
/// prior pass) and the ledger holds the ACK doc: stage_acquire's peek finds no
/// NEW row → Noop → audit Critical (unexpected under A4) → the ledger truth is
/// returned, NEVER a blind terminalise or a 501.
#[tokio::test]
async fn noop_resolves_accepted_truth_from_ledger() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_online(&pool, shift_id).await;
    let row = seed_inbox_sell(&pool).await;
    // Simulate "already processed": inbox DONE + a terminal ACK ledger doc
    // for the SAME request_id.
    sqlx::query("UPDATE ingress_inbox SET status = 'DONE' WHERE request_id = ?")
        .bind(&row.request_id[..])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO fiscal_documents( \
            document_id, request_id, fiscal_number, shift_id, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, server_fiscal_no, first_kvt1_at, \
            total_sum_kop \
         ) VALUES (?, ?, ?, ?, 1, 'SELL', 'ACK', 'b', 't', 'ONLINE', \
            '2026-06-09T12:00:00Z', '{}', ?, ?, '2026-06-09T12:00:05Z', ?)",
    )
    .bind(prro::db::models::ids::DocumentId::new())
    .bind(&row.request_id[..])
    .bind(FN)
    .bind(shift_id)
    .bind(vec![0u8; 32])
    .bind(SERVER_FISCAL_NO)
    .bind(TOTAL_KOP)
    .execute(&pool)
    .await
    .unwrap();

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
        .expect("Noop must resolve the accepted ledger truth, not fail");
    assert_eq!(
        outcome.document_state,
        prro::db::models::enums::DocState::Ack
    );
    assert_eq!(outcome.fiscal_id.as_deref(), Some(SERVER_FISCAL_NO));
    assert!(
        outcome.fiscal_ts.is_some(),
        "ledger first_kvt1_at threads through"
    );
    // The unexpected-Noop observability pin (decision e).
    assert_eq!(audit_count(&pool, "INLINE_NOOP_UNEXPECTED").await, 1);
    // The DONE row is untouched (never blind-terminalise a foreign row).
    assert_eq!(read_inbox_status(&pool, &row.request_id).await, "DONE");
}

/// **Review TA-6 — BuildReject arm (pre-acquire).** A corrupted row (payload
/// hash mismatch) fails `build_canonical` BEFORE any stage: atomic pre-acquire
/// terminalise (lease+REJECT+audit, one tx) + `Internal{PAYLOAD_HASH_MISMATCH}`
/// (500); NO fiscal_documents row.
#[tokio::test]
async fn build_reject_terminalises_pre_acquire() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    // Row with a WRONG payload_sha256_canonical ([0u8;32] != sha256(payload)).
    let req_id = RequestId::new();
    let request_id: [u8; 16] = *req_id.as_bytes();
    inbox::insert(
        &pool,
        &NewInboxEntry {
            request_id,
            fiscal_number: FN.into(),
            protocol: Protocol::Rest,
            operation_type: DocType::Sell.as_str().into(),
            idempotency_key: "idem-a2-1b-bad-sha".into(),
            payload_json: SELL_PAYLOAD.into(),
            payload_sha256_canonical: [0u8; 32],
            correlation_id: None,
            signed_by_cashier_id: Some(CASHIER.into()),
            driver_id: Some(DRIVER.into()),
            business_ts: Some("2026-06-09T12:00:00Z".into()),
            total_sum_kop: Some(TOTAL_KOP),
        },
    )
    .await
    .unwrap();
    let row = InboxRow {
        request_id,
        fiscal_number: FN.into(),
        protocol: Protocol::Rest,
        operation_type: DocType::Sell.as_str().into(),
        idempotency_key: "idem-a2-1b-bad-sha".into(),
        status: "NEW".into(),
        payload_json: SELL_PAYLOAD.into(),
        payload_sha256_canonical: [0u8; 32],
        correlation_id: None,
        received_at: "2026-06-09T12:00:00Z".into(),
        signed_by_cashier_id: Some(CASHIER.into()),
        driver_id: Some(DRIVER.into()),
        business_ts: Some("2026-06-09T12:00:00Z".into()),
        total_sum_kop: Some(TOTAL_KOP),
    };

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
        .expect_err("a hash-mismatched row is a structural reject");
    match err {
        FiscalError::Internal { code, .. } => assert_eq!(code, "PAYLOAD_HASH_MISMATCH"),
        other => panic!("expected Internal{{PAYLOAD_HASH_MISMATCH}}, got {other:?}"),
    }
    assert_eq!(read_inbox_status(&pool, &request_id).await, "REJECTED");
    assert_eq!(audit_count(&pool, "INLINE_BUILD_REJECT").await, 1);
    let docs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(docs, 0);
}

/// **Review A24-4 — RETURN is in the signed scope and flows to ACK** exactly
/// like SELL (same CheckJson shape, total required).
#[tokio::test]
async fn online_return_reaches_ack() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_online(&pool, shift_id).await;
    let row = seed_inbox_op(&pool, DocType::Return.as_str()).await;

    let dps = DualStub::new(
        Ok(ack(SERVER_FISCAL_NO, vec![])),
        Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD])),
    );
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;

    let outcome = inline::run(&pool, &pool_secure, &dps, &sign_ctx, &fn_sign, &guard, &row)
        .await
        .expect("online RETURN must reach a terminal ACK");
    assert_eq!(
        outcome.document_state,
        prro::db::models::enums::DocState::Ack
    );
    assert_eq!(outcome.fiscal_id.as_deref(), Some(SERVER_FISCAL_NO));
    assert_eq!(read_doc_state(&pool, FN).await, "ACK");
}
