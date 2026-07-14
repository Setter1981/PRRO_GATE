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

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use sha2::{Digest, Sha256};

use prro::config::AppConfig;
use prro::db::models::enums::{DocState, FiscalMode, NodeMode, Protocol, ShiftState};
use prro::db::models::ids::{DocumentId, RequestId, ShiftId};
use prro::db::repositories::document_files::{self, DocumentFileKind};
use prro::db::repositories::fiscal_documents::{self, TransitionOutcome};
use prro::db::repositories::ingress_inbox::{self as inbox, InboxRow, NewInboxEntry};
use prro::db::repositories::node_state;
use prro::db::repositories::{fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig};
use prro::db::tx::with_immediate;
use prro::db::types::{DbDocumentId, DbShiftId};
use prro::db::{open_pool, open_secure_pool};
use prro::services::reconciliation::online_convergence::run_tick_for_fn;
use prro::services::reconciliation::RuntimeView;
use prro::services::write_path::inline;
use prro::transports::dps::dto::{CheckAck, CheckSignBlob};
use prro::transports::dps::error::DpsError;
use prro::App;
use sqlx::SqlitePool;

use common::det_signing_ctx;
use common::scripted_dps::ScriptedDps;

const FN: &str = "4000000001";
const CASHIER: &str = "test-cashier";
const DRIVER: &str = "drv-test";
const SERVER_FISCAL_NO: &str = "DPS-FN-ONLINE-1";
const NEWER_SERVER_FISCAL_NO: &str = "DPS-FN-ONLINE-2";
const SELL_PAYLOAD: &str = r#"{"items":[{"code":"item-1","name":"Test item","price_kop":15000,"quantity_thousandths":1000,"sum_kop":15000}],"payments":[{"name":"Cash","sum_kop":15000,"type_code":"0"}]}"#;
const TOTAL_KOP: i64 = 15000;

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
    .bind(DbShiftId(shift_id))
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
    .bind(mode.as_str())
    .bind(ShiftState::Opened.as_str())
    .bind(DbShiftId(shift_id))
    .execute(pool)
    .await
    .unwrap();
}

async fn set_node_mode(pool: &SqlitePool, mode: NodeMode) {
    sqlx::query("UPDATE node_state SET mode = ? WHERE fiscal_number = ?")
        .bind(mode.as_str())
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
    sqlx::query_scalar::<_, DbDocumentId>(
        "SELECT document_id FROM fiscal_documents WHERE fiscal_number = ?",
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .map(|w| w.0)
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

async fn read_shift_state(pool: &SqlitePool, shift_id: ShiftId) -> String {
    sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
        .bind(DbShiftId(shift_id))
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
async fn build_resting_sent(pool: &SqlitePool, pool_secure: &SqlitePool, stub: &ScriptedDps) {
    let row = seed_inbox_sell(pool).await;
    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;
    let outcome = inline::run(
        pool,
        pool_secure,
        stub,
        &sign_ctx,
        &fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await
    .expect("inline::run with Hold lastChk returns Ok(Sent)");
    assert_eq!(outcome.document_state, DocState::Sent);
    assert_eq!(read_doc_state(pool, FN).await, "SENT");
}

/// Manually advance `Sent → Kvt1` + persist `Kvt1Raw` (mirror of production
/// Envelope-1a, stopping at `KVT1`) — state-construction, not a prod call.
async fn manual_advance_sent_to_kvt1(pool: &SqlitePool) {
    let doc_id = read_doc_id(pool).await;
    let kvt1_bytes = vec![0xDE; 64];
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
    let stub = ScriptedDps::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    // Construction: send Ok (→SENT) + lastChk Hold (empty → rests at SENT).
    stub.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![]))); // Hold
                                                       // Tick cascade: SENT-arm probe Match (→KVT1) + KVT1 confirm Match (→ACK).
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE; 64])));
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE; 64])));

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
    let stub = ScriptedDps::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    stub.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![]))); // Hold → SENT
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE; 64]))); // KVT1 confirm Match

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
    let stub = ScriptedDps::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
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
    let stub = ScriptedDps::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
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
    let stub = ScriptedDps::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
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
    let stub = ScriptedDps::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
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
// Test 5b — KVT1 + lastChk Match with SHORT (sub-signature) data_sign → Hold.
// RISK 1 harden: a byzantine-but-alive DPS returning a non-empty but implausibly
// short data_sign (shorter than any real DSTU signature) must NOT be accepted as
// a KVT1 quittance — those bytes are not evidence. Mirrors the empty-hold guard.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tick_hold_on_short_data_sign() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, NodeMode::Online, shift_id).await;

    let (send_calls, last_calls) = seed_counters();
    let stub = ScriptedDps::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    stub.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![]))); // construction Hold → SENT
                                                       // tick confirm: a non-empty but IMPLAUSIBLY SHORT data_sign (4 bytes ≪ any
                                                       // real DSTU signature) → must Hold, not advance (RISK 1 fail-closed harden).
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF])));

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
        "short (sub-signature) data_sign → Hold, stays KVT1 (RISK 1 harden)"
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
    let stub = ScriptedDps::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    stub.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![])));
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE; 64])));
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE; 64])));

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
    let stub = ScriptedDps::new(send_calls, last_calls);
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
    let stub = ScriptedDps::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    // Construction: send Ok (→SENT) + lastChk Hold (empty → rests at SENT).
    stub.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![]))); // Hold
                                                       // Tick: probe answers a DIFFERENT id → Mismatch → manual escalation.
    stub.push_last(Ok(ack("DPS-FN-SOMEONE-ELSE", vec![0xDE; 64])));

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

// ════════════════════════════════════════════════════════════════════════════
// AUD-L2-1b (convergence consumer) — a resting online KVT1 whose stage_finalize
// hits a ChainSeedMismatch (tampered node_state seed) ESCALATES the FN to Manual,
// instead of being lost as a generic Infrastructure error + a silent per-doc
// isolation skip (summary.errors++, shift untouched).
//
// On main this FAILS: advance_to_ack blanket-maps EVERY StageFinalizeError into
// ConfirmError::Infrastructure → map_confirm_error → BootError::ReconciliationFailed
// → online_convergence per-doc isolation (summary.errors += 1), so a chain-integrity
// breach gets NO operator surface and the shift stays Opened.  AUD-L2-1b threads a
// typed ConfirmError::ChainSeedMismatch → ConfirmDrainOutcome::ChainSeedMismatch →
// the convergence arm escalates via the shared escalate_fn_to_manual_recon.
//
// NB: no assert_clean — the tampered seed is a deliberate chain-integrity breach
// (exactly the condition under escalation); invariant_scan rightly flags it.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tick_chain_seed_mismatch_escalates_manual() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, NodeMode::Online, shift_id).await;

    let (send_calls, last_calls) = seed_counters();
    let stub = ScriptedDps::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    stub.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![]))); // Hold → SENT
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE; 64]))); // KVT1 confirm Match

    build_resting_sent(&pool, &pool_secure, &stub).await;
    manual_advance_sent_to_kvt1(&pool).await; // online-origin KVT1 (offline_fiscal_no NULL)

    // Tamper the chain seed AFTER the doc was signed (its previous_hash is the
    // pre-doc genesis seed) so stage_finalize's online-origin seed guard breaks
    // (ns.last_known_unsigned_xml_sha256 != doc.previous_hash) on the ACK step.
    node_state::seed_prevhash(&pool, FN, &[0xFF; 32])
        .await
        .expect("seed tamper");

    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    let summary = run_tick_for_fn(&pool, &view, FN)
        .await
        .expect("tick returns Ok (per-doc isolation does not abort the tick)");

    // A.3 re-ground (design v3 §6 step 4; INV-08): the sole finalize producer of
    // `ChainSeedMismatch` (the online ACK-time chain guard) was REMOVED — the
    // chain check moved EARLIER, to the SEND drift-assert.  This tamper corrupts
    // `node_state.seed` AFTER the doc already crossed SEND, so it is NOT a
    // convergence-detectable breach anymore; the convergence tick converges the
    // doc to ACK WITHOUT firing a ChainSeedMismatch escalation.  The breach is
    // instead surfaced by the `invariant_scan` detector (PR-C wires
    // scan/boot-detected breaks → the RETAINED escalation arm; no recovery
    // TRANSITION was deleted — only the producer narrowed).
    assert_ne!(
        read_shift_state(&pool, shift_id).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "post-A.3: a POST-SEND seed tamper is a scan/boot breach, not a convergence escalation"
    );
    assert_eq!(
        count_audit_events(&pool, "CONVERGE_CHAIN_SEED_MISMATCH_ESCALATE_MANUAL").await,
        0,
        "finalize no longer produces ChainSeedMismatch → no convergence escalation here"
    );
    assert_eq!(summary.errors, 0, "per-doc isolation: the tick returns Ok");
    assert_eq!(send_calls.load(Ordering::SeqCst), 1, "tick does NOT send");
    // Detection MOVED to the scan: the tampered chain breach is flagged there.
    let violations = prro::db::invariant_scan::scan(&pool).await.unwrap();
    assert!(
        violations.iter().any(|v| matches!(
            v,
            prro::db::invariant_scan::Violation::ChainSeedMismatch { .. }
        )),
        "the invariant_scan detector surfaces the tampered chain-seed breach (detection moved)"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// SW-2 (M1-M2 sweep) — the convergence tick must NOT re-probe a RMR-but-Online FN.
//
// run_tick_for_fn has no RMR guard (AUD-K8-1 lives only in the drain), so an FN
// escalated to shift_state==RMR while mode stays ONLINE (the Batch-C convergence
// / boot-KVT2 escalation state) passes the mode-gate and re-probes its resting
// SENT/KVT1 siblings every tick — wire-traffic on a halted FN.
//
// On main this FAILS: the resting SENT doc is probed (extra last_chk) + advanced.
// GREEN: a RMR short-circuit after the mode-gate skips the FN — 0 re-probe.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tick_skips_rmr_but_online_fn_no_reprobe() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, NodeMode::Online, shift_id).await;

    let (send_calls, last_calls) = seed_counters();
    let stub = ScriptedDps::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    stub.push_send(Ok(ack(SERVER_FISCAL_NO, vec![]))); // construction send
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![]))); // construction Hold → SENT
                                                       // probe responses the tick WOULD consume if it (wrongly) ran —
                                                       // enough for the SENT→KVT1→ACK cascade so main fails on the
                                                       // assertion below, NOT on an empty-queue stub panic.
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE; 64]))); // SENT-probe Match
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE; 64]))); // KVT1-confirm Match

    build_resting_sent(&pool, &pool_secure, &stub).await; // doc rests SENT
    assert_eq!(
        last_calls.load(Ordering::SeqCst),
        1,
        "one construction Hold probe"
    );

    // The Batch-C escalation state: shift CAS'd to RMR, mode LEFT at Online.
    sqlx::query("UPDATE shifts SET state = 'REQUIRES_MANUAL_RECONCILIATION' WHERE shift_id = ?")
        .bind(DbShiftId(shift_id))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE node_state SET shift_state = 'REQUIRES_MANUAL_RECONCILIATION' WHERE fiscal_number = ?",
    )
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();

    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    run_tick_for_fn(&pool, &view, FN)
        .await
        .expect("tick returns Ok (RMR FN skipped)");

    // GREEN: a halted (RMR) FN is NOT re-probed — the doc stays SENT, last_chk
    // count stays at the single construction Hold (no tick re-probe).
    assert_eq!(
        read_doc_state(&pool, FN).await,
        "SENT",
        "RMR FN's resting doc must NOT be re-probed/advanced by the tick"
    );
    assert_eq!(
        last_calls.load(Ordering::SeqCst),
        1,
        "no re-probe on a halted (RMR) FN — only the construction Hold probe"
    );
    assert_eq!(send_calls.load(Ordering::SeqCst), 1, "tick does NOT send");
}

// ════════════════════════════════════════════════════════════════════════════
// A.3 PR-C — resolver: extend the tick to the ER (pre-SENT) cohort.
//
// An online ErrorRetryable doc is a NON-ISSUED blocker for the D5 gate (it
// gates every successor on the FN until it converges).  Without a runtime
// re-driver the gate would be an FN-wide stall until reboot.  The tick now
// routes the ER cohort through the EXISTING `er_redrive_policy`:
//   - Redrive (TransientRetry, attempts<MAX) → stage_send::run → Sent (issued)
//     ⇒ the gate opens;
//   - HoldIndeterminate (no durable retry_class) → fail-closed HOLD: the FN
//     stays gated (doc unchanged) + an audit that escalates Warning→CRITICAL
//     after N ticks (operator surface — ambiguous-wire is a manual-recon
//     family, NOT a spin).
// ════════════════════════════════════════════════════════════════════════════

/// Seed a resting online-origin ERROR_RETRYABLE doc with NO transport_trace
/// (⇒ `evaluate_er_redrive` returns `HoldIndeterminate`).  Raw INSERT.
async fn seed_online_er_doc(pool: &SqlitePool, lnd: i64) -> DocumentId {
    let doc_id = DocumentId::new();
    let req_id = RequestId::new();
    let sha: [u8; 32] = Sha256::digest(b"er-doc-indeterminate").into();
    sqlx::query(
        "INSERT INTO fiscal_documents \
            (document_id, request_id, fiscal_number, lnd, doc_type, state, \
             backend_profile_id, transport_profile_id, fs_mode, business_ts, \
             payload_json, payload_sha256_canonical) \
         VALUES (?, ?, ?, ?, 'SELL', 'ERROR_RETRYABLE', 'b', 't', 'ONLINE', \
             '2026-06-09T12:00:00Z', '{}', ?)",
    )
    .bind(&doc_id.as_bytes()[..])
    .bind(&req_id.as_bytes()[..])
    .bind(FN)
    .bind(lnd)
    .bind(&sha[..])
    .execute(pool)
    .await
    .unwrap();
    doc_id
}

async fn event_has_severity(pool: &SqlitePool, event_type: &str, severity: &str) -> bool {
    let n: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type = ? AND severity = ?")
            .bind(event_type)
            .bind(severity)
            .fetch_one(pool)
            .await
            .unwrap();
    n > 0
}

// (g) Redrive: an ER doc (TransientRetry) is driven to Sent by the tick →
//     the doc becomes issued → the D5 gate opens.
#[tokio::test]
async fn tick_er_redrive_advances_to_sent_and_ungates_fn() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, NodeMode::Online, shift_id).await;

    let (send_calls, last_calls) = seed_counters();
    let stub = ScriptedDps::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    // Construction: the sell's send fails transiently → the doc lands in
    // ErrorRetryable with a completed transport_trace(TransientRetry, 1).
    stub.push_send(Err(DpsError::Transport("transient-construction".into())));
    // Tick redrive: stage_send re-sends → Ok → Sent (sfn stamped ⇒ issued).
    stub.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));

    let row = seed_inbox_sell(&pool).await;
    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    {
        let gate = Arc::new(tokio::sync::Mutex::new(()));
        let guard = gate.lock_owned().await;
        let outcome = inline::run(
            &pool,
            &pool_secure,
            &stub,
            &sign_ctx,
            &fn_sign,
            &guard,
            &row,
            prro::services::time_budget::system_gate(),
        )
        .await
        .expect("inline::run with a transient send → Ok(InProgress)");
        assert_eq!(
            outcome.document_state,
            DocState::ErrorRetryable,
            "construction: transient send lands ErrorRetryable"
        );
    }
    assert_eq!(read_doc_state(&pool, FN).await, "ERROR_RETRYABLE");

    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    run_tick_for_fn(&pool, &view, FN).await.expect("tick ok");

    assert_eq!(
        read_doc_state(&pool, FN).await,
        "SENT",
        "resolver drives ER→Sent (issued) ⇒ the D5 gate opens"
    );
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        2,
        "one construction fail + one redrive send"
    );
}

// (h) HoldIndeterminate: no durable retry_class → fail-closed HOLD; the FN
//     stays gated (doc unchanged) and the audit escalates Warning→CRITICAL
//     after N ticks.
#[tokio::test]
async fn tick_er_hold_indeterminate_keeps_fn_gated_and_escalates_after_n_ticks() {
    let pool = fresh_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, NodeMode::Online, shift_id).await;
    // A resting online-ER doc with NO transport_trace → HoldIndeterminate.
    let _doc = seed_online_er_doc(&pool, 1).await;

    let (send_calls, last_calls) = seed_counters();
    let stub = ScriptedDps::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };

    // Ticks 1..=2 → Warning; the doc stays ErrorRetryable (FN gated), zero wire.
    for _ in 0..2 {
        run_tick_for_fn(&pool, &view, FN).await.expect("tick ok");
    }
    assert_eq!(
        read_doc_state(&pool, FN).await,
        "ERROR_RETRYABLE",
        "HoldIndeterminate keeps the FN gated (no state change)"
    );
    assert_eq!(
        count_audit_events(&pool, "CONVERGE_ER_HOLD_INDETERMINATE").await,
        2
    );
    assert!(
        !event_has_severity(&pool, "CONVERGE_ER_HOLD_INDETERMINATE", "CRITICAL").await,
        "the first 2 ticks are Warning, not CRITICAL"
    );

    // Tick 3 (N=3) → CRITICAL operator surface; still gated (hold).
    run_tick_for_fn(&pool, &view, FN).await.expect("tick ok");
    assert_eq!(
        read_doc_state(&pool, FN).await,
        "ERROR_RETRYABLE",
        "still gated"
    );
    assert_eq!(
        count_audit_events(&pool, "CONVERGE_ER_HOLD_INDETERMINATE").await,
        3
    );
    assert!(
        event_has_severity(&pool, "CONVERGE_ER_HOLD_INDETERMINATE", "CRITICAL").await,
        "the Nth (=3) tick escalates to CRITICAL"
    );
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        0,
        "HoldIndeterminate issues zero wire"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// T1 (RULING 1 / PRRO_GATE-eid) — BOUNDED superseded-hold for shift-lifecycle
// docs → RequiresManualReconciliation.
//
// A resting online SHIFT_OPEN / Z_REPORT / SHIFT_CLOSE doc whose KVT1 confirm
// keeps returning `SupersededHeld` wedges the shift in `Opening`/`Closing`
// (can neither open nor close, compounds the shift time-limit).  Today the
// online tick HOLDS it FOREVER (AUD-L5-1: hold + `superseded_held_kvt1` counter,
// no bound).  T1 gives it a BOUNDED hold: after `SUPERSEDED_SHIFT_HOLD_TICKS`
// (= 5) consecutive superseded-held ticks for the SAME doc, escalate the FN to
// `RequiresManualReconciliation` via the EXISTING `escalate_fn_to_manual_recon`
// CAS (the ChainSeedMismatch seam) — doc state untouched, a dedicated
// `CONVERGE_SUPERSEDED_SHIFT_BOUND_ESCALATE_MANUAL` audit with {document_id,
// doc_type, held_ticks}.  Receipt docs (SELL/RETURN) are byte-unchanged: an
// unbounded benign hold (a held receipt does not wedge the shift).
//
// Durability: the tick-count is AUDIT-DERIVED (count the per-doc
// `CONVERGE_SUPERSEDED_SHIFT_HELD` rows) — crash-safe by construction, no schema
// churn.  This mirrors the existing `count_converge_indeterminate_audits`.
//
// On main (pre-T1) pin 1 / pin 4 / the teeth pin FAIL RED: the shift-doc holds
// forever (`shift_state` stays Opened, zero bound audits).  Pins 2 / 3 pass on
// main too (they assert NON-escalation), but they lock the reset rule and the
// receipt-arm no-op against a future over-broad bound.
// ════════════════════════════════════════════════════════════════════════════

/// Re-type the single resting doc for the FN to a shift-lifecycle `doc_type`
/// (raw UPDATE).  `confirm_drain_doc` / the superseded verdict do NOT branch on
/// `doc_type` (verified: zero `doc_type` reads in kvt2_confirm.rs), so flipping
/// it leaves the `SupersededHeld` outcome identical — only the online-tick
/// consumer arm (T1) branches on it.  Reuses the proven Test-2b KVT1 fixture
/// (`build_resting_sent` + `manual_advance_sent_to_kvt1`) verbatim.
async fn flip_doc_type(pool: &SqlitePool, lnd: i64, doc_type: &str) {
    sqlx::query("UPDATE fiscal_documents SET doc_type = ? WHERE fiscal_number = ? AND lnd = ?")
        .bind(doc_type)
        .bind(FN)
        .bind(lnd)
        .execute(pool)
        .await
        .unwrap();
}

/// Push ONE superseded-tick `last_chk` response (DPS reports a DIFFERENT tip —
/// the newer submitted doc — so `confirm_drain_doc` returns `SupersededHeld`).
fn push_superseded_tick(stub: &ScriptedDps) {
    stub.push_last(Err(DpsError::ServerFiscalIdMismatch {
        expected_id: SERVER_FISCAL_NO.to_string(),
        actual_id: NEWER_SERVER_FISCAL_NO.to_string(),
    }));
}

/// Build a resting KVT1 doc (Test-2b fixture) re-typed to `doc_type`, plus the
/// newer submitted ACK doc (lnd=2) that makes the mismatch a benign SUPERSESSION
/// rather than structural drift.  The construction consumes push_send + one
/// push_last(Hold); the caller enqueues the per-tick superseded responses.
async fn build_resting_kvt1_superseded(
    pool: &SqlitePool,
    pool_secure: &SqlitePool,
    stub: &ScriptedDps,
    shift_id: ShiftId,
    doc_type: &str,
) {
    stub.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![]))); // Hold → SENT
    build_resting_sent(pool, pool_secure, stub).await;
    manual_advance_sent_to_kvt1(pool).await; // lnd=1 rests at KVT1
    flip_doc_type(pool, 1, doc_type).await;
    // Newer submitted doc (lnd=2) whose sfn is the DPS tip → supersession.
    seed_newer_submitted_ack(pool, shift_id, 2, NEWER_SERVER_FISCAL_NO).await;
}

const BOUND_ESCALATE_EVENT: &str = "CONVERGE_SUPERSEDED_SHIFT_BOUND_ESCALATE_MANUAL";
/// The bound value under test — must equal `SUPERSEDED_SHIFT_HOLD_TICKS`.
const N: usize = 5;

// ── Pin 1 — SHIFT_OPEN superseded ×N → FN escalated to RMR + dedicated audit.
//    (RED on main: the shift-doc holds forever, shift stays Opened, 0 audits.)
#[tokio::test]
async fn t1_shift_open_superseded_n_ticks_escalates_manual() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, NodeMode::Online, shift_id).await;

    let (send_calls, last_calls) = seed_counters();
    let stub = ScriptedDps::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    build_resting_kvt1_superseded(&pool, &pool_secure, &stub, shift_id, "SHIFT_OPEN").await;
    // N superseded ticks.
    for _ in 0..N {
        push_superseded_tick(&stub);
    }

    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };

    // Ticks 1..N-1: still held, NOT yet escalated (below the bound).
    for i in 1..N {
        run_tick_for_fn(&pool, &view, FN).await.expect("tick ok");
        assert_ne!(
            read_shift_state(&pool, shift_id).await,
            "REQUIRES_MANUAL_RECONCILIATION",
            "must NOT escalate before the bound (tick {i} of {N})"
        );
    }
    // Tick N: the bound fires → FN escalated to RMR.
    let summary = run_tick_for_fn(&pool, &view, FN).await.expect("tick N ok");

    assert_eq!(
        read_shift_state(&pool, shift_id).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "the Nth (=5) consecutive superseded-held tick escalates the FN to RMR"
    );
    assert_eq!(
        read_doc_state_by_lnd(&pool, 1).await,
        "KVT1",
        "the escalation leaves the doc state untouched (rests at KVT1)"
    );
    assert_eq!(
        count_audit_events(&pool, BOUND_ESCALATE_EVENT).await,
        1,
        "exactly one dedicated bound-escalate audit"
    );
    assert_eq!(
        summary.superseded_held_kvt1, 1,
        "the Nth tick still counts the superseded hold (the escalation is on top)"
    );
    assert_eq!(send_calls.load(Ordering::SeqCst), 1, "tick does NOT send");
}

// ── Pin 2 — SHIFT_OPEN superseded ×(N-1) then a confirm-SUCCESS → NO escalation,
//    the shift converges (doc → ACK).  Locks the RESET rule: recovery obsoletes
//    the counter (a successful confirm on a later tick must not leave a wedge).
#[tokio::test]
async fn t1_shift_open_recovers_before_bound_no_escalation() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, NodeMode::Online, shift_id).await;

    let (send_calls, last_calls) = seed_counters();
    let stub = ScriptedDps::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    build_resting_kvt1_superseded(&pool, &pool_secure, &stub, shift_id, "SHIFT_OPEN").await;
    // N-1 superseded ticks, then a confirm Match (KVT1 → ACK).  The success tick
    // returns the doc's OWN sfn as the DPS tip (+ non-empty data_sign) → the
    // superseded verdict no longer applies and the confirm ADVANCES to ACK.
    for _ in 0..(N - 1) {
        push_superseded_tick(&stub);
    }
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE; 64])));

    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    // N-1 superseded ticks — held, not escalated.
    for _ in 0..(N - 1) {
        run_tick_for_fn(&pool, &view, FN).await.expect("tick ok");
    }
    // The confirm-success tick.
    run_tick_for_fn(&pool, &view, FN)
        .await
        .expect("confirm-success tick ok");

    assert_eq!(
        read_doc_state_by_lnd(&pool, 1).await,
        "ACK",
        "a confirm-success on a later tick converges the shift doc"
    );
    assert_ne!(
        read_shift_state(&pool, shift_id).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "recovery before the bound must NOT escalate (reset rule)"
    );
    assert_eq!(
        count_audit_events(&pool, BOUND_ESCALATE_EVENT).await,
        0,
        "no bound-escalate audit when the doc recovers before N"
    );
}

// ── Pin 3 — SELL superseded ×(N+5) → still a benign UNBOUNDED hold: no
//    escalation, doc stays KVT1, no bound audit.  The receipt arm is byte-
//    unchanged (AUD-L5-1 stands verbatim for receipts).
#[tokio::test]
async fn t1_sell_superseded_unbounded_hold_no_escalation() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, NodeMode::Online, shift_id).await;

    let (send_calls, last_calls) = seed_counters();
    let stub = ScriptedDps::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    // doc_type stays "SELL" (the default) — the resting doc is a receipt.
    build_resting_kvt1_superseded(&pool, &pool_secure, &stub, shift_id, "SELL").await;
    let ticks = N + 5;
    for _ in 0..ticks {
        push_superseded_tick(&stub);
    }

    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    for _ in 0..ticks {
        let summary = run_tick_for_fn(&pool, &view, FN).await.expect("tick ok");
        assert_eq!(
            summary.superseded_held_kvt1, 1,
            "receipt: benign hold each tick"
        );
        assert_eq!(summary.errors, 0, "receipt: supersession is not an error");
    }

    assert_eq!(
        read_doc_state_by_lnd(&pool, 1).await,
        "KVT1",
        "a superseded RECEIPT holds forever (unbounded), stays KVT1"
    );
    assert_ne!(
        read_shift_state(&pool, shift_id).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "a held receipt does NOT wedge the shift — never escalate"
    );
    assert_eq!(
        count_audit_events(&pool, BOUND_ESCALATE_EVENT).await,
        0,
        "no bound-escalate audit for a receipt (the receipt arm is untouched)"
    );
}

// ── Pin 4 — crash/reboot between held ticks: the AUDIT-DERIVED counter survives
//    (it lives in the DB, not in memory), so the bound still fires at N TOTAL.
//    "Reboot" = drop the ScriptedDps + RuntimeView (in-memory) and rebuild them
//    against the SAME pool (the durable store).  The tick is stateless.
#[tokio::test]
async fn t1_bound_survives_reboot_fires_at_n_total() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, NodeMode::Online, shift_id).await;

    // ── Pre-crash instance: build the fixture + tick N-2 times, then "crash".
    {
        let (send_calls, last_calls) = seed_counters();
        let stub = ScriptedDps::new(send_calls, last_calls);
        build_resting_kvt1_superseded(&pool, &pool_secure, &stub, shift_id, "Z_REPORT").await;
        for _ in 0..(N - 2) {
            push_superseded_tick(&stub);
        }
        let sign_ctx = det_signing_ctx();
        let fn_sign = fn_sign_blob();
        let view = RuntimeView {
            dps: &stub,
            signing_ctx: &sign_ctx,
            fn_sign: &fn_sign,
        };
        for _ in 0..(N - 2) {
            run_tick_for_fn(&pool, &view, FN)
                .await
                .expect("pre-crash tick ok");
        }
        assert_ne!(
            read_shift_state(&pool, shift_id).await,
            "REQUIRES_MANUAL_RECONCILIATION",
            "N-2 ticks: below the bound, not escalated"
        );
        // stub + view drop here — the in-memory tick context is gone (the crash).
    }

    // ── Post-reboot instance: brand-new stub/view, SAME pool.  2 more ticks
    //    ((N-2) + 2 = N total) must fire the bound — the counter was durable.
    let (send_calls, last_calls) = seed_counters();
    let stub = ScriptedDps::new(send_calls, last_calls);
    for _ in 0..2 {
        push_superseded_tick(&stub);
    }
    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    // Tick N-1: still below the bound.
    run_tick_for_fn(&pool, &view, FN)
        .await
        .expect("post-reboot tick N-1 ok");
    assert_ne!(
        read_shift_state(&pool, shift_id).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "tick N-1 after reboot: still below the bound"
    );
    // Tick N (total): the bound fires despite the reboot.
    run_tick_for_fn(&pool, &view, FN)
        .await
        .expect("post-reboot tick N ok");
    assert_eq!(
        read_shift_state(&pool, shift_id).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "the bound fires at N TOTAL ticks — the audit-derived counter survived the reboot"
    );
    assert_eq!(
        count_audit_events(&pool, BOUND_ESCALATE_EVENT).await,
        1,
        "exactly one bound-escalate audit across the reboot"
    );
}

// ── Pin 5 (🦷 teeth) — the canary: byte-identical assertion to pin 1.  Revert
//    the bound (restore the unbounded hold in online_convergence.rs's
//    `SupersededHeld` arm) → this pin REDs (the shift-doc holds forever, so it
//    never reaches RMR).  Kept as a standing test so a future regression that
//    silently drops the bound is caught by CI.
#[tokio::test]
async fn t1_teeth_bound_reverted_would_red() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, NodeMode::Online, shift_id).await;

    let (send_calls, last_calls) = seed_counters();
    let stub = ScriptedDps::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    build_resting_kvt1_superseded(&pool, &pool_secure, &stub, shift_id, "SHIFT_CLOSE").await;
    for _ in 0..N {
        push_superseded_tick(&stub);
    }

    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    for _ in 0..N {
        run_tick_for_fn(&pool, &view, FN).await.expect("tick ok");
    }

    // The teeth assertion: WITH the bound, N ticks escalate.  Reverting the
    // bound leaves the shift Opened forever → this REDs.
    assert_eq!(
        read_shift_state(&pool, shift_id).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "teeth: reverting the bound leaves the shift-doc held forever (this REDs)"
    );
    assert_eq!(
        count_audit_events(&pool, BOUND_ESCALATE_EVENT).await,
        1,
        "teeth: the dedicated bound audit is the regression tripwire"
    );
}
