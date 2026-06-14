//! B1 v1 — online-convergence tick acceptance (audit pass-2, item 3, spec §5).
//!
//! Proves the runtime tick converges resting online `SENT`/`KVT1` docs to `ACK`
//! by REUSING the boot Sent-arm + drain Kvt1Reentry confirm path, with a strict
//! SELECT-first contract (an empty / non-Online tick issues zero wire) and
//! exactly one `send_chk` per check counted THROUGH construction + tick.
//!
//! Fixtures mirror `tests/kill_point_matrix.rs`: real stage composition builds
//! `SENT` (`inline::run` lastChk-Hold) and `KVT1` (manual Envelope-1a CAS), not
//! raw doc INSERTs; counters are `Arc<AtomicUsize>` shared via a single stub.

mod common;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use prro::config::AppConfig;
use prro::db::models::enums::{DocState, FiscalMode, NodeMode, Protocol, ShiftState};
use prro::db::models::ids::{DocumentId, RequestId, ShiftId};
use prro::db::repositories::document_files::{self, DocumentFileKind};
use prro::db::repositories::fiscal_documents::{self, TransitionOutcome};
use prro::db::repositories::ingress_inbox::{self as inbox, InboxRow, NewInboxEntry};
use prro::db::repositories::{fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig};
use prro::db::tx::with_immediate;
use prro::db::{open_pool, open_secure_pool};
use prro::services::reconciliation::online_convergence::run_tick_for_fn;
use prro::services::reconciliation::RuntimeView;
use prro::services::write_path::inline;
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{CheckAck, CheckEnvelope, CheckSignBlob, RroInfo, StatusSnapshot};
use prro::transports::dps::error::DpsError;
use prro::App;
use sqlx::SqlitePool;

use common::det_signing_ctx;

const FN: &str = "4000000001";
const CASHIER: &str = "test-cashier";
const DRIVER: &str = "drv-test";
const SERVER_FISCAL_NO: &str = "DPS-FN-ONLINE-1";
const NEWER_SERVER_FISCAL_NO: &str = "DPS-FN-ONLINE-2";
const SELL_PAYLOAD: &str = r#"{"items":[{"code":"item-1","name":"Test item","price_kop":15000,"quantity_thousandths":1000,"sum_kop":15000}],"payments":[{"name":"Cash","sum_kop":15000,"type_code":"0"}]}"#;
const TOTAL_KOP: i64 = 15000;

// ─── DPS stub: dual-queue + shared AtomicUsize counters (no hang needed) ─────

struct KpStub {
    send_q: Mutex<VecDeque<Result<CheckAck, DpsError>>>,
    last_q: Mutex<VecDeque<Result<CheckAck, DpsError>>>,
    send_calls: Arc<AtomicUsize>,
    last_calls: Arc<AtomicUsize>,
}

impl KpStub {
    fn new(send_calls: Arc<AtomicUsize>, last_calls: Arc<AtomicUsize>) -> Self {
        Self {
            send_q: Mutex::new(VecDeque::new()),
            last_q: Mutex::new(VecDeque::new()),
            send_calls,
            last_calls,
        }
    }
    fn push_send(&self, r: Result<CheckAck, DpsError>) {
        self.send_q.lock().unwrap().push_back(r);
    }
    fn push_last(&self, r: Result<CheckAck, DpsError>) {
        self.last_q.lock().unwrap().push_back(r);
    }
}

#[async_trait]
impl DpsChannel for KpStub {
    async fn send_chk(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        self.send_calls.fetch_add(1, Ordering::SeqCst);
        self.send_q
            .lock()
            .unwrap()
            .pop_front()
            .expect("KpStub.send_chk: empty queue (tick must NOT send)")
    }
    async fn last_chk(&self, _: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        self.last_calls.fetch_add(1, Ordering::SeqCst);
        self.last_q
            .lock()
            .unwrap()
            .pop_front()
            .expect("KpStub.last_chk: empty queue")
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

fn fn_sign_blob() -> CheckSignBlob {
    CheckSignBlob(vec![0xAB, 0xCD])
}

// ─── Pool + fixture seeds (mirror tests/kill_point_matrix.rs) ────────────────

async fn fresh_pool() -> SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("oct.db");
    std::mem::forget(dir);
    open_pool(&path).await.unwrap()
}

async fn fresh_secure_pool() -> SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("oct-secure.db");
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

async fn seed_node_state(pool: &SqlitePool, mode: NodeMode, shift_id: ShiftId) {
    sqlx::query(
        "INSERT INTO node_state \
         (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
          backend_profile_id, transport_profile_id) \
         VALUES (?, ?, ?, ?, 1, 'b', 't')",
    )
    .bind(FN)
    .bind(mode)
    .bind(ShiftState::Opened)
    .bind(shift_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn set_node_mode(pool: &SqlitePool, mode: NodeMode) {
    sqlx::query("UPDATE node_state SET mode = ? WHERE fiscal_number = ?")
        .bind(mode)
        .bind(FN)
        .execute(pool)
        .await
        .unwrap();
}

async fn seed_inbox_sell(pool: &SqlitePool) -> InboxRow {
    let req_id = RequestId::new();
    let request_id: [u8; 16] = *req_id.as_bytes();
    let payload_sha256_canonical: [u8; 32] = Sha256::digest(SELL_PAYLOAD.as_bytes()).into();
    let idempotency_key = "idem-oct-SELL".to_string();
    inbox::insert(
        pool,
        &NewInboxEntry {
            request_id,
            fiscal_number: FN.into(),
            protocol: Protocol::Rest,
            operation_type: "SELL".into(),
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
        operation_type: "SELL".into(),
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

async fn read_inbox_status(pool: &SqlitePool, request_id: &[u8; 16]) -> String {
    sqlx::query_scalar("SELECT status FROM ingress_inbox WHERE request_id = ?")
        .bind(&request_id[..])
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_doc_id(pool: &SqlitePool) -> DocumentId {
    sqlx::query_scalar("SELECT document_id FROM fiscal_documents WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn count_doc_rows(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Read a specific doc's state by `lnd` — needed once a test seeds MORE than
/// one row for the FN (`read_doc_state` does an unqualified `fetch_one`).
async fn read_doc_state_by_lnd(pool: &SqlitePool, lnd: i64) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE fiscal_number = ? AND lnd = ?")
        .bind(FN)
        .bind(lnd)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn count_audit_events(pool: &SqlitePool, event_type: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type = ?")
        .bind(event_type)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Seed a NEWER submitted doc (`lnd` > the resting head) in terminal `ACK`
/// with a distinct `server_fiscal_no` — the supersession candidate that
/// `submitted_above_lnd` returns.  Raw INSERT (not a pipeline run): the test
/// only needs a row satisfying the supersession query (lnd > head, state IN
/// SENT/KVT1/KVT2/ACK, non-empty sfn).  Well-formed (real shift_id) so the
/// `assert_clean` invariant scan tolerates it.
async fn seed_newer_submitted_ack(pool: &SqlitePool, shift_id: ShiftId, lnd: i64, sfn: &str) {
    let doc_id = DocumentId::new();
    let req_id = RequestId::new();
    let payload_sha: [u8; 32] = Sha256::digest(b"newer-submitted-doc").into();
    sqlx::query(
        "INSERT INTO fiscal_documents \
            (document_id, request_id, fiscal_number, shift_id, lnd, doc_type, state, \
             backend_profile_id, transport_profile_id, fs_mode, business_ts, \
             payload_json, payload_sha256_canonical, server_fiscal_no, server_fiscal_date) \
         VALUES (?, ?, ?, ?, ?, 'SELL', 'ACK', 'b', 't', 'ONLINE', \
             '2026-06-09T12:00:01Z', '{}', ?, ?, '2026-06-09T12:00:01Z')",
    )
    .bind(&doc_id.as_bytes()[..])
    .bind(&req_id.as_bytes()[..])
    .bind(FN)
    .bind(&shift_id.as_bytes()[..])
    .bind(lnd)
    .bind(&payload_sha[..])
    .bind(sfn)
    .execute(pool)
    .await
    .unwrap();
    // ACK ⇒ persisted KVT1_RAW evidence (invariant_scan 3b / HIGH-C5-2) —
    // make the fixture well-formed so `assert_clean` tolerates it.
    sqlx::query(
        "INSERT INTO document_files (document_id, kind, content) VALUES (?, 'KVT1_RAW', ?)",
    )
    .bind(&doc_id.as_bytes()[..])
    .bind(&b"kvt1-raw-fixture"[..])
    .execute(pool)
    .await
    .unwrap();
}

/// Build a resting `SENT` doc via the real stage chain: `inline::run` with a
/// lastChk-Hold (empty data_sign) rests the doc at `SENT` (a 202).  Consumes
/// `send_q[0]` (send Ok → one send) + `last_q[0]` (empty → Hold).
async fn build_resting_sent(pool: &SqlitePool, pool_secure: &SqlitePool, stub: &KpStub) {
    let row = seed_inbox_sell(pool).await;
    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;
    let outcome = inline::run(pool, pool_secure, stub, &sign_ctx, &fn_sign, &guard, &row)
        .await
        .expect("inline::run with Hold lastChk returns Ok(Sent)");
    assert_eq!(outcome.document_state, DocState::Sent);
    assert_eq!(read_doc_state(pool, FN).await, "SENT");
}

/// Manually advance `Sent → Kvt1` + persist `Kvt1Raw` (mirror of production
/// Envelope-1a, stopping at `KVT1`) — state-construction, not a prod call.
async fn manual_advance_sent_to_kvt1(pool: &SqlitePool) {
    let doc_id = read_doc_id(pool).await;
    let kvt1_bytes = vec![0xDE, 0xAD, 0xBE, 0xEF];
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            let o = fiscal_documents::transition_state(tx, doc_id, DocState::Sent, DocState::Kvt1)
                .await
                .map_err(anyhow::Error::from)?;
            assert!(matches!(o, TransitionOutcome::Applied), "Sent→Kvt1 applied");
            document_files::replace_tx(tx, doc_id, DocumentFileKind::Kvt1Raw, &kvt1_bytes).await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await
    .expect("manual Sent→Kvt1 + Kvt1Raw envelope");
    assert_eq!(read_doc_state(pool, FN).await, "KVT1");
}

fn seed_counters() -> (Arc<AtomicUsize>, Arc<AtomicUsize>) {
    (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)))
}

// ════════════════════════════════════════════════════════════════════════════
// Test 1 — tick converges a resting SENT doc all the way to ACK (cascade).
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tick_converges_resting_sent_to_ack() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, NodeMode::Online, shift_id).await;

    let (send_calls, last_calls) = seed_counters();
    let stub = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    // Construction: send Ok (→SENT) + lastChk Hold (empty → rests at SENT).
    stub.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![]))); // Hold
                                                       // Tick cascade: SENT-arm probe Match (→KVT1) + KVT1 confirm Match (→ACK).
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF])));
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF])));

    build_resting_sent(&pool, &pool_secure, &stub).await;
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        1,
        "one send in construction"
    );

    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    let summary = run_tick_for_fn(&pool, &view, FN).await.expect("tick ok");

    assert_eq!(
        read_doc_state(&pool, FN).await,
        "ACK",
        "SENT cascades to ACK"
    );
    assert_eq!(
        read_inbox_status(&pool, &doc_request_id(&pool).await).await,
        "DONE"
    );
    assert_eq!(summary.advanced_sent_to_kvt1, 1);
    assert_eq!(summary.acked_from_kvt1, 1);
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        1,
        "exactly one send_chk total — the tick does NOT send"
    );
    assert_eq!(count_doc_rows(&pool).await, 1);
    prro::db::invariant_scan::assert_clean(&pool).await;
}

async fn doc_request_id(pool: &SqlitePool) -> [u8; 16] {
    let v: Vec<u8> =
        sqlx::query_scalar("SELECT request_id FROM ingress_inbox WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(pool)
            .await
            .unwrap();
    v.as_slice().try_into().unwrap()
}

// ════════════════════════════════════════════════════════════════════════════
// Test 2 — tick converges a resting KVT1 doc to ACK.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tick_converges_resting_kvt1_to_ack() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, NodeMode::Online, shift_id).await;

    let (send_calls, last_calls) = seed_counters();
    let stub = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    stub.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![]))); // Hold → SENT
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF]))); // KVT1 confirm Match

    build_resting_sent(&pool, &pool_secure, &stub).await;
    manual_advance_sent_to_kvt1(&pool).await;
    assert_eq!(send_calls.load(Ordering::SeqCst), 1);

    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    let summary = run_tick_for_fn(&pool, &view, FN).await.expect("tick ok");

    assert_eq!(read_doc_state(&pool, FN).await, "ACK", "KVT1 → ACK");
    assert_eq!(
        summary.advanced_sent_to_kvt1, 0,
        "no SENT step (already KVT1)"
    );
    assert_eq!(summary.acked_from_kvt1, 1);
    assert_eq!(send_calls.load(Ordering::SeqCst), 1, "tick does NOT send");
    prro::db::invariant_scan::assert_clean(&pool).await;
}

// ════════════════════════════════════════════════════════════════════════════
// Test 2b — AUD-L5-1: a resting KVT1 whose DPS `last_chk` tip was SUPERSEDED by
// a NEWER submitted doc is HELD (benign), NOT falsely terminalised as
// StructuralDrift.  Online-tick consumer policy = HOLD (no chain-head, unlike
// the offline drain which escalates Manual — see kill_point_matrix).
//
// On main this FAILS: the superseded exception is fetched ONLY for SentReplay
// (kvt2_confirm fetch-gate), so the Kvt1Reentry tick gets an empty newer-set →
// superseded=false → StructuralDrift (Severity::Error) + summary.errors==1.
// AUD-L5-1 widens the fetch to Kvt1Reentry and routes it to a benign hold.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tick_holds_superseded_resting_kvt1_not_structural_drift() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, NodeMode::Online, shift_id).await;

    let (send_calls, last_calls) = seed_counters();
    let stub = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    stub.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![]))); // Hold → SENT
                                                       // KVT1 confirm: DPS reports a DIFFERENT tip — a newer submitted doc
                                                       // became the last_chk tip → ServerFiscalIdMismatch (actual = newer sfn).
    stub.push_last(Err(DpsError::ServerFiscalIdMismatch {
        expected_id: SERVER_FISCAL_NO.to_string(),
        actual_id: NEWER_SERVER_FISCAL_NO.to_string(),
    }));

    // Resting head: lnd=1, sfn=SERVER_FISCAL_NO.
    build_resting_sent(&pool, &pool_secure, &stub).await;
    manual_advance_sent_to_kvt1(&pool).await;
    // Newer submitted doc (lnd=2) whose sfn is the DPS tip → the resting KVT1's
    // mismatch is a benign SUPERSESSION, not structural drift.
    seed_newer_submitted_ack(&pool, shift_id, 2, NEWER_SERVER_FISCAL_NO).await;
    assert_eq!(send_calls.load(Ordering::SeqCst), 1);

    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    let summary = run_tick_for_fn(&pool, &view, FN)
        .await
        .expect("tick ok — supersession is a benign hold, not an error");

    // Desired (AUD-L5-1): benign HOLD — doc stays KVT1, counted as a superseded
    // hold, NO per-doc error, NO structural-drift audit, exactly one
    // TIP_SUPERSEDED (Warning) audit.
    assert_eq!(
        read_doc_state_by_lnd(&pool, 1).await,
        "KVT1",
        "superseded KVT1 stays KVT1 (held, no CAS)"
    );
    assert_eq!(summary.acked_from_kvt1, 0, "not advanced");
    assert_eq!(
        summary.errors, 0,
        "supersession is benign — not a per-doc error"
    );
    assert_eq!(
        summary.superseded_held_kvt1, 1,
        "counted as a distinct superseded hold"
    );
    assert_eq!(
        count_audit_events(&pool, "KVT2_CONFIRM_STRUCTURAL_DRIFT").await,
        0,
        "superseded tip must NOT be falsely terminalised as structural drift"
    );
    assert_eq!(
        count_audit_events(&pool, "TIP_SUPERSEDED").await,
        1,
        "exactly one benign-supersession audit"
    );
    assert_eq!(send_calls.load(Ordering::SeqCst), 1, "tick does NOT send");
    prro::db::invariant_scan::assert_clean(&pool).await;
}

// ════════════════════════════════════════════════════════════════════════════
// Test 3 — SELECT-first: a clean FN issues ZERO wire calls.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tick_zero_wire_when_nothing_resting() {
    let pool = fresh_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, NodeMode::Online, shift_id).await;

    let (send_calls, last_calls) = seed_counters();
    let stub = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    let summary = run_tick_for_fn(&pool, &view, FN).await.expect("tick ok");

    assert_eq!(summary.scanned, 0, "no resting docs");
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        0,
        "SELECT-first: zero send"
    );
    assert_eq!(
        last_calls.load(Ordering::SeqCst),
        0,
        "SELECT-first: zero lastChk"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// Test 4 — mode-guard: a non-Online FN is skipped with zero wire, doc untouched.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tick_skips_non_online_mode() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, NodeMode::Online, shift_id).await;

    let (send_calls, last_calls) = seed_counters();
    let stub = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    stub.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![])));
    build_resting_sent(&pool, &pool_secure, &stub).await;
    manual_advance_sent_to_kvt1(&pool).await;

    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();

    for mode in [NodeMode::Offline, NodeMode::GoingOnline] {
        set_node_mode(&pool, mode).await;
        let view = RuntimeView {
            dps: &stub,
            signing_ctx: &sign_ctx,
            fn_sign: &fn_sign,
        };
        let summary = run_tick_for_fn(&pool, &view, FN).await.expect("tick ok");
        assert!(
            summary.mode_skipped,
            "non-Online FN must be skipped: {mode:?}"
        );
        assert_eq!(read_doc_state(&pool, FN).await, "KVT1", "doc untouched");
    }
    // Only the construction touched the wire; both skipped ticks added nothing.
    assert_eq!(send_calls.load(Ordering::SeqCst), 1);
    assert_eq!(last_calls.load(Ordering::SeqCst), 1);
}

// ════════════════════════════════════════════════════════════════════════════
// Test 5 — KVT1 + lastChk Match with EMPTY data_sign → Hold, doc stays KVT1.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tick_hold_on_empty_data_sign() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, NodeMode::Online, shift_id).await;

    let (send_calls, last_calls) = seed_counters();
    let stub = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    stub.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![]))); // construction Hold → SENT
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![]))); // tick confirm: EMPTY → Hold

    build_resting_sent(&pool, &pool_secure, &stub).await;
    manual_advance_sent_to_kvt1(&pool).await;

    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    let summary = run_tick_for_fn(&pool, &view, FN).await.expect("tick ok");

    assert_eq!(
        read_doc_state(&pool, FN).await,
        "KVT1",
        "empty data_sign → Hold, stays KVT1"
    );
    assert_eq!(summary.held_kvt1, 1);
    assert_eq!(summary.acked_from_kvt1, 0);
    prro::db::invariant_scan::assert_clean(&pool).await;
}

// ════════════════════════════════════════════════════════════════════════════
// Test 6 — idempotent: a second tick after convergence is a zero-wire no-op.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tick_idempotent_after_convergence() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, NodeMode::Online, shift_id).await;

    let (send_calls, last_calls) = seed_counters();
    let stub = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    stub.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![])));
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF])));
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF])));

    build_resting_sent(&pool, &pool_secure, &stub).await;

    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    // First tick → ACK.
    run_tick_for_fn(&pool, &view, FN).await.expect("tick 1 ok");
    assert_eq!(read_doc_state(&pool, FN).await, "ACK");
    let send_after_1 = send_calls.load(Ordering::SeqCst);
    let last_after_1 = last_calls.load(Ordering::SeqCst);

    // Second tick → nothing resting (doc is terminal ACK), zero new wire.
    let summary2 = run_tick_for_fn(&pool, &view, FN).await.expect("tick 2 ok");
    assert_eq!(summary2.scanned, 0, "ACK is terminal — not resting");
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        send_after_1,
        "no new send"
    );
    assert_eq!(
        last_calls.load(Ordering::SeqCst),
        last_after_1,
        "no new lastChk"
    );
    assert_eq!(read_doc_state(&pool, FN).await, "ACK", "state unchanged");
    prro::db::invariant_scan::assert_clean(&pool).await;
}

// ════════════════════════════════════════════════════════════════════════════
// Test 7 — App::converge_online_for_fn serialises on the A4 per-FN gate.
// ════════════════════════════════════════════════════════════════════════════

async fn boot_app() -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir
        .path()
        .join("a.db")
        .display()
        .to_string()
        .replace('\\', "/");
    let toml_text = format!(
        r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "{db_path}"
secure_db_path = "{db_path}_secure"

[admin_ui]
enabled = false
listen  = "127.0.0.1:8443"
"#
    );
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    let app = App::boot(cfg).await.unwrap();
    (dir, app)
}

#[tokio::test]
async fn tick_serialises_on_fn_gate() {
    use std::time::Duration;
    use tokio::time::timeout;

    let (_dir, app) = boot_app().await;

    // Stub + view are never touched — with no node_state row for this FN the
    // tick returns early (the point of the test is the gate, not convergence).
    let (send_calls, last_calls) = seed_counters();
    let stub = KpStub::new(send_calls, last_calls);
    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };

    // Externally hold the per-FN gate via a clone (shared via Arc<Inner>).
    let clone = app.clone();
    let held = clone.acquire_fn_gate(FN).await;

    // converge must block on the SAME FN gate → pending under a short timeout.
    let mut fut = Box::pin(app.converge_online_for_fn(FN, &view));
    assert!(
        timeout(Duration::from_millis(200), &mut fut).await.is_err(),
        "converge_online_for_fn must serialise on the held A4 fn-gate (expected pending)"
    );

    // Release the gate → converge acquires it and completes.
    drop(held);
    let summary = timeout(Duration::from_millis(500), &mut fut)
        .await
        .expect("converge must complete once the gate is released")
        .expect("tick ok");
    assert_eq!(
        summary.scanned, 0,
        "no node_state for this FN → nothing scanned"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// Test 10 (architect review fix) — probe Mismatch escalates to manual recon
// and the tick COUNTS the non-convergence.
//
// `dispatch_sent_via_probe` on `ProbeOutcome::Mismatch` CASes the doc to
// `REQUIRES_MANUAL_RECONCILIATION` — a state OUTSIDE `list_pending_for_fn`'s
// filter, so the post-arm re-read finds nothing.  The original else-branch
// treated that as a defensive race and returned WITHOUT counting: the operator
// log would show `scanned=1` with every outcome counter at zero.  Pins the
// fix: the vanished-from-pending doc is recorded as `sent_not_converged`.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tick_counts_mismatch_escalation_as_not_converged() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, NodeMode::Online, shift_id).await;

    let (send_calls, last_calls) = seed_counters();
    let stub = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    // Construction: send Ok (→SENT) + lastChk Hold (empty → rests at SENT).
    stub.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![]))); // Hold
                                                       // Tick: probe answers a DIFFERENT id → Mismatch → manual escalation.
    stub.push_last(Ok(ack("DPS-FN-SOMEONE-ELSE", vec![0xDE, 0xAD])));

    build_resting_sent(&pool, &pool_secure, &stub).await;

    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    let summary = run_tick_for_fn(&pool, &view, FN).await.expect("tick ok");

    assert_eq!(
        read_doc_state(&pool, FN).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "Mismatch escalates via the reused boot arm"
    );
    assert_eq!(
        summary.sent_not_converged, 1,
        "the escalated doc must be COUNTED, not silently dropped from the summary"
    );
    assert_eq!(summary.advanced_sent_to_kvt1, 0);
    assert_eq!(summary.acked_from_kvt1, 0);
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        1,
        "exactly one send_chk total — Mismatch never resends"
    );
    assert_eq!(count_doc_rows(&pool).await, 1);
    prro::db::invariant_scan::assert_clean(&pool).await;
}
