//! Kill-point matrix — executable crash-equivalence proof (audit pass-2, item 2).
//!
//! Spec: `docs/superpowers/plans/2026-06-11-kill-point-matrix-spec.md`
//! (SPEC LOCKED — architect Fable 5).  Implementer: Opus 4.8.
//!
//! Each `K`-test commits the write-path at one envelope boundary of the
//! online/offline ladder, then runs the *same-pool* standard recovery
//! (`boot_phase::run_boot_reconciliation` or `backlog_drain::drain`) and
//! asserts convergence WITHOUT double fiscalisation (exactly one `send_chk`
//! per check, exactly one `lnd`) and without ledger-invariant drift
//! (`db::invariant_scan`).
//!
//! **Crash-equivalence (spec §2, NOT to be changed):** under
//! `synchronous=FULL` a committed `with_immediate` survives kill -9; an
//! uncommitted one rolls back.  So "process died between envelopes k and
//! k+1" is byte-for-byte equal to "stages 1..k ran, k+1 never started".
//! Two construction mechanisms:
//!   - **Stage composition** (K5 manual CAS + the `inline::run` Hold path):
//!     deterministic, no future-drop, no timing.
//!   - **Drop-injection** (K3, K4): the only "mid-wire" points are awaits on
//!     the DPS stub.  The stub parks on a `tokio::sync::oneshot`; the test
//!     drops the `inline::run` future the instant a coordination oneshot
//!     confirms the await was reached (spec §2 explicitly permits dropping
//!     the `inline::run` future).  No sleep / no timing.
//!
//! Counters (`send_calls` / `last_calls`) are `Arc<AtomicUsize>` SHARED
//! across the phase-1 (pre-crash) and phase-2 (recovery) stubs, so the
//! "exactly one `send_chk`" assertions are counted THROUGH the restart.

mod common;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;

use prro::db::models::enums::{
    DocState, FiscalMode, NodeMode, OfflineSessionState, Protocol, ShiftState,
};
use prro::db::models::ids::{DocumentId, OfflineSessionId, RequestId, ShiftId};
use prro::db::repositories::document_files::{self, DocumentFileKind};
use prro::db::repositories::fiscal_documents::{self, TransitionOutcome};
use prro::db::repositories::ingress_inbox::{self as inbox, InboxRow, NewInboxEntry};
use prro::db::repositories::{fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig};
use prro::db::tx::with_immediate;
use prro::db::{open_pool, open_secure_pool};
use prro::runtime::ingress::canonical_builder::build_canonical;
use prro::services::offline_sync::backlog_drain;
use prro::services::reconciliation::boot_phase::{self};
use prro::services::reconciliation::{ReconcileGuard, RuntimeView};
use prro::services::write_path::inline;
use prro::services::write_path::types::WorkerProcessResult;
use prro::services::write_path::{stage_acquire, stage_sign};
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{CheckAck, CheckEnvelope, CheckSignBlob, RroInfo, StatusSnapshot};
use prro::transports::dps::error::DpsError;
use sqlx::SqlitePool;

use common::det_signing_ctx;

// ─── Constants (mirror tests/write_path_inline.rs base fixture) ─────────────

const FN: &str = "4000000001";
const CASHIER: &str = "test-cashier";
const DRIVER: &str = "drv-test";
const SERVER_FISCAL_NO: &str = "DPS-FN-ONLINE-1";
const SELL_PAYLOAD: &str = r#"{"items":[{"code":"item-1","name":"Test item","price_kop":15000,"quantity_thousandths":1000,"sum_kop":15000}],"payments":[{"name":"Cash","sum_kop":15000,"type_code":"0"}]}"#;
const TOTAL_KOP: i64 = 15000;

// ─── DPS stub: dual-queue + shared AtomicUsize counters + oneshot hang ───────

/// Kill-point DPS stub.  Two scripted response queues (`send_chk` /
/// `last_chk`), call counters held behind `Arc<AtomicUsize>` so they survive
/// the simulated restart (shared between the phase-1 and phase-2 stubs), and
/// an optional per-method "hang" mode for drop-injection (K3/K4): when armed,
/// the method increments its counter, fires `reached` (so the test knows the
/// wire await was entered AND the prior committed envelope is durable), then
/// awaits `block` — which the test never resolves, so the await parks until
/// the surrounding future is dropped (the "crash").
struct KpStub {
    send_q: Mutex<VecDeque<Result<CheckAck, DpsError>>>,
    last_q: Mutex<VecDeque<Result<CheckAck, DpsError>>>,
    send_calls: Arc<AtomicUsize>,
    last_calls: Arc<AtomicUsize>,
    send_reached: Mutex<Option<oneshot::Sender<()>>>,
    send_block: Mutex<Option<oneshot::Receiver<()>>>,
    last_reached: Mutex<Option<oneshot::Sender<()>>>,
    last_block: Mutex<Option<oneshot::Receiver<()>>>,
}

impl KpStub {
    fn new(send_calls: Arc<AtomicUsize>, last_calls: Arc<AtomicUsize>) -> Self {
        Self {
            send_q: Mutex::new(VecDeque::new()),
            last_q: Mutex::new(VecDeque::new()),
            send_calls,
            last_calls,
            send_reached: Mutex::new(None),
            send_block: Mutex::new(None),
            last_reached: Mutex::new(None),
            last_block: Mutex::new(None),
        }
    }

    fn push_send(&self, r: Result<CheckAck, DpsError>) {
        self.send_q.lock().unwrap().push_back(r);
    }

    fn push_last(&self, r: Result<CheckAck, DpsError>) {
        self.last_q.lock().unwrap().push_back(r);
    }

    /// Arm the `send_chk` await to hang.  `reached` fires when the await is
    /// entered (Sending already committed by the 4-pre envelope); `block`
    /// never resolves, so the await parks until the future is dropped.
    fn hang_send(&self, reached: oneshot::Sender<()>, block: oneshot::Receiver<()>) {
        *self.send_reached.lock().unwrap() = Some(reached);
        *self.send_block.lock().unwrap() = Some(block);
    }

    /// Arm the `last_chk` await to hang (Sent already committed when reached).
    fn hang_last(&self, reached: oneshot::Sender<()>, block: oneshot::Receiver<()>) {
        *self.last_reached.lock().unwrap() = Some(reached);
        *self.last_block.lock().unwrap() = Some(block);
    }
}

#[async_trait]
impl DpsChannel for KpStub {
    async fn send_chk(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        self.send_calls.fetch_add(1, Ordering::SeqCst);
        let block = self.send_block.lock().unwrap().take();
        if let Some(block) = block {
            if let Some(reached) = self.send_reached.lock().unwrap().take() {
                let _ = reached.send(());
            }
            // Park until the surrounding future is dropped (the "crash").
            let _ = block.await;
        }
        self.send_q
            .lock()
            .unwrap()
            .pop_front()
            .expect("KpStub.send_chk: empty queue")
    }

    async fn last_chk(&self, _: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        self.last_calls.fetch_add(1, Ordering::SeqCst);
        let block = self.last_block.lock().unwrap().take();
        if let Some(block) = block {
            if let Some(reached) = self.last_reached.lock().unwrap().take() {
                let _ = reached.send(());
            }
            let _ = block.await;
        }
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

// ─── Pool + fixture seed helpers (mirror tests/write_path_inline.rs) ─────────

async fn fresh_pool() -> SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kpm.db");
    std::mem::forget(dir);
    open_pool(&path).await.unwrap()
}

async fn fresh_secure_pool() -> SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kpm-secure.db");
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

async fn set_node_mode(pool: &SqlitePool, mode: NodeMode) {
    sqlx::query("UPDATE node_state SET mode = ? WHERE fiscal_number = ?")
        .bind(mode)
        .bind(FN)
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

async fn seed_offline_code(pool: &SqlitePool, code_lnd: i64) {
    sqlx::query("INSERT INTO offline_codes(fiscal_number, code_lnd) VALUES (?, ?)")
        .bind(FN)
        .bind(code_lnd)
        .execute(pool)
        .await
        .unwrap();
}

async fn seed_inbox_sell(pool: &SqlitePool) -> InboxRow {
    let req_id = RequestId::new();
    let request_id: [u8; 16] = *req_id.as_bytes();
    let payload_sha256_canonical: [u8; 32] = Sha256::digest(SELL_PAYLOAD.as_bytes()).into();
    let idempotency_key = "idem-kpm-SELL".to_string();
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

// ─── Read helpers ───────────────────────────────────────────────────────────

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

async fn count_doc_rows(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn count_consumed_offline_codes(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes \
         WHERE fiscal_number = ? AND consumed_at IS NOT NULL",
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn recon_guard() -> ReconcileGuard<'static> {
    ReconcileGuard::for_integration_test_only()
}

fn fn_sign_blob() -> CheckSignBlob {
    CheckSignBlob(vec![0xAB, 0xCD])
}

// ════════════════════════════════════════════════════════════════════════════
// K6 — OFFLINE_LOCAL_ACK committed, drain not yet run.
//
// Build:    offline fixture (node Offline, OPEN session, 1 code) + full
//           `inline::run` — terminates by itself at OFFLINE_LOCAL_ACK.
// Recovery: go-online (node → GoingOnline) + `backlog_drain::drain` with a
//           mock DPS (send Ok + lastChk Match).
// Locked:   doc → ACK, inbox → DONE, offline code consumed exactly once,
//           `send_chk` (by drain) == 1, `assert_clean`.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn k6_offline_local_ack_drains_to_ack() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_offline(&pool, shift_id).await;
    seed_open_offline_session(&pool).await;
    seed_offline_code(&pool, 1).await;
    let row = seed_inbox_sell(&pool).await;

    // ── Phase 1: full inline::run on the offline node — terminates at
    //    OFFLINE_LOCAL_ACK.  DPS is never called on the offline branch.
    let send_calls = Arc::new(AtomicUsize::new(0));
    let last_calls = Arc::new(AtomicUsize::new(0));
    let phase1 = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;

    let outcome = inline::run(
        &pool,
        &pool_secure,
        &phase1,
        &sign_ctx,
        &fn_sign,
        &guard,
        &row,
    )
    .await
    .expect("offline SELL must land at OFFLINE_LOCAL_ACK (success, not error)");
    assert_eq!(outcome.document_state, DocState::OfflineLocalAck);
    assert_eq!(read_doc_state(&pool, FN).await, "OFFLINE_LOCAL_ACK");
    assert_eq!(
        read_inbox_status(&pool, &row.request_id).await,
        "PROCESSING"
    );
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        0,
        "offline branch must NOT touch the wire"
    );
    drop(guard);

    // ── "Restart" + go-online: flip node to GoingOnline so the drain loop
    //    owns this FN's reconciliation (session stays OPEN).
    set_node_mode(&pool, NodeMode::GoingOnline).await;

    // ── Phase 2: drain with a mock DPS — send Ok (carries KVT1 data_sign) +
    //    lastChk Match.  Counters are SHARED with phase 1.
    let phase2 = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    phase2.push_send(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF])));
    phase2.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF])));
    let sign_ctx2 = det_signing_ctx();
    let fn_sign2 = fn_sign_blob();
    let view = RuntimeView {
        dps: &phase2,
        signing_ctx: &sign_ctx2,
        fn_sign: &fn_sign2,
    };

    let summary = backlog_drain::drain(&recon_guard(), &pool, &view, FN)
        .await
        .expect("drain must succeed");
    assert_eq!(summary.backlog_size_before(), 1, "exactly one backlog doc");

    // ── Locked assertions.
    assert_eq!(
        read_doc_state(&pool, FN).await,
        "ACK",
        "drain → terminal ACK"
    );
    assert_eq!(
        read_inbox_status(&pool, &row.request_id).await,
        "DONE",
        "inbox terminalised on drain ACK"
    );
    assert_eq!(
        count_consumed_offline_codes(&pool).await,
        1,
        "offline code consumed exactly once (phase 1)"
    );
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        1,
        "exactly one send_chk across both phases (drain's wire submit)"
    );
    assert_eq!(count_doc_rows(&pool).await, 1, "exactly one ledger row");
    prro::db::invariant_scan::assert_clean(&pool).await;
}

async fn read_doc_id(pool: &SqlitePool) -> DocumentId {
    sqlx::query_scalar("SELECT document_id FROM fiscal_documents WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

// ════════════════════════════════════════════════════════════════════════════
// K5 — KVT2 committed (Envelope 1a done), finalize not yet run.
//
// Build:    full online `inline::run` with lastChk = empty data_sign → the
//           online-confirm Hold branch rests the doc at SENT (202).  Then
//           mirror production Envelope-1a by hand: CAS Sent→Kvt1→Kvt2 via the
//           repository `transition_state` whitelist + `Kvt1Raw` replace, all
//           inside ONE `with_immediate` (state-construction, not a prod call).
// Recovery: boot (deps None — the Kvt2 arm is ctx-free).
// Locked:   Kvt2 arm (:2719) → stage_finalize → doc ACK, inbox DONE,
//           `send_chk` == 1, full `assert_clean`.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn k5_kvt2_committed_finalizes_to_ack() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_online(&pool, shift_id).await;
    let row = seed_inbox_sell(&pool).await;

    // ── Phase 1: online inline::run, lastChk Hold (empty data_sign) — doc
    //    rests at SENT, send_chk fires exactly once.
    let send_calls = Arc::new(AtomicUsize::new(0));
    let last_calls = Arc::new(AtomicUsize::new(0));
    let phase1 = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    phase1.push_send(Ok(ack(SERVER_FISCAL_NO, vec![]))); // data_sign discarded by stage_send
    phase1.push_last(Ok(ack(SERVER_FISCAL_NO, vec![]))); // empty → online_confirm Hold → SENT
    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;

    let outcome = inline::run(
        &pool,
        &pool_secure,
        &phase1,
        &sign_ctx,
        &fn_sign,
        &guard,
        &row,
    )
    .await
    .expect("online SELL with Hold lastChk returns Ok(Sent), a 202");
    assert_eq!(outcome.document_state, DocState::Sent, "Hold rests at SENT");
    assert_eq!(read_doc_state(&pool, FN).await, "SENT");
    assert_eq!(send_calls.load(Ordering::SeqCst), 1, "one send in phase 1");
    drop(guard);

    // ── Manual Envelope-1a mirror: CAS Sent→Kvt1→Kvt2 + persist Kvt1Raw, one
    //    short immediate transaction.  This is state-construction of the
    //    "KVT2 committed, finalize not run" crash point — NOT a production call.
    let doc_id = read_doc_id(&pool).await;
    let kvt1_bytes = vec![0xDE, 0xAD, 0xBE, 0xEF];
    with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let o1 = fiscal_documents::transition_state(tx, doc_id, DocState::Sent, DocState::Kvt1)
                .await
                .map_err(anyhow::Error::from)?;
            assert!(
                matches!(o1, TransitionOutcome::Applied),
                "Sent→Kvt1 applied"
            );
            let o2 = fiscal_documents::transition_state(tx, doc_id, DocState::Kvt1, DocState::Kvt2)
                .await
                .map_err(anyhow::Error::from)?;
            assert!(
                matches!(o2, TransitionOutcome::Applied),
                "Kvt1→Kvt2 applied"
            );
            document_files::replace_tx(tx, doc_id, DocumentFileKind::Kvt1Raw, &kvt1_bytes).await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await
    .expect("manual Sent→Kvt1→Kvt2 + Kvt1Raw envelope");
    assert_eq!(read_doc_state(&pool, FN).await, "KVT2", "K5 crash point");

    // ── Phase 2: boot recovery — Kvt2 arm is ctx-free, deps None.
    boot_phase::run_boot_reconciliation(&recon_guard(), &pool, FN, None)
        .await
        .expect("boot reconciliation must succeed");

    // ── Locked assertions.
    assert_eq!(
        read_doc_state(&pool, FN).await,
        "ACK",
        "Kvt2 arm finalizes to ACK"
    );
    assert_eq!(
        read_inbox_status(&pool, &row.request_id).await,
        "DONE",
        "inbox terminalised on finalize ACK"
    );
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        1,
        "exactly one send_chk across both phases (no resend in recovery)"
    );
    assert_eq!(count_doc_rows(&pool).await, 1, "exactly one ledger row");
    prro::db::invariant_scan::assert_clean(&pool).await;
}

// ════════════════════════════════════════════════════════════════════════════
// K4 — SENT committed, online confirm (lastChk) never began.
//
// Build:    full online `inline::run`; send_chk returns Ok (commits
//           Sending→Sent + server_fiscal_no), then lastChk PARKS on a oneshot.
//           When the lastChk await is reached, Sent is already durable, so the
//           test drops the `inline::run` future (drop-injection, spec §2/§4).
// Recovery: boot with deps whose lastChk now answers Match (id==sfn, non-empty
//           data_sign) → SENT-probe (:2822) → advance Sent→Kvt1, then the Kvt1
//           arm is a passive hold (:2676).
// Locked:   doc rests at KVT1, `send_chk` total == 1, `last_chk` ≥ 1, scan
//           clean.  The final online-Kvt1 ACK requires the ops-loop / B1
//           (known OCF-5) — we assert resting KVT1 and do NOT force ACK.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn k4_sent_committed_probe_holds_at_kvt1() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_online(&pool, shift_id).await;
    let row = seed_inbox_sell(&pool).await;

    let send_calls = Arc::new(AtomicUsize::new(0));
    let last_calls = Arc::new(AtomicUsize::new(0));

    // ── Phase 1: send_chk Ok → Sent committed; lastChk hangs → drop.
    let phase1 = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    phase1.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    let (reached_tx, reached_rx) = oneshot::channel::<()>();
    let (block_tx, block_rx) = oneshot::channel::<()>();
    phase1.hang_last(reached_tx, block_rx);

    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;

    {
        let mut fut = Box::pin(inline::run(
            &pool,
            &pool_secure,
            &phase1,
            &sign_ctx,
            &fn_sign,
            &guard,
            &row,
        ));
        tokio::select! {
            _ = &mut fut => panic!("inline::run must hang on lastChk, not complete"),
            _ = reached_rx => { /* lastChk await reached ⇒ Sending→Sent already committed */ }
        }
        drop(fut); // cancel the parked lastChk await — this is the "crash"
    }
    let _keep_block_tx = block_tx; // keep the block sender alive until after the drop
    drop(guard);

    // Post-crash committed state: SENT, server_fiscal_no persisted, one send.
    assert_eq!(read_doc_state(&pool, FN).await, "SENT", "K4 crash point");
    assert_eq!(send_calls.load(Ordering::SeqCst), 1, "one send in phase 1");

    // ── Phase 2: boot with deps; lastChk now answers Match.
    let phase2 = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    phase2.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF])));
    let sign_ctx2 = det_signing_ctx();
    let fn_sign2 = fn_sign_blob();
    let view = RuntimeView {
        dps: &phase2,
        signing_ctx: &sign_ctx2,
        fn_sign: &fn_sign2,
    };
    boot_phase::run_boot_reconciliation(&recon_guard(), &pool, FN, Some(&view))
        .await
        .expect("boot reconciliation must succeed");

    // ── Locked assertions: resting KVT1, exactly one send, ≥1 lastChk.
    assert_eq!(
        read_doc_state(&pool, FN).await,
        "KVT1",
        "SENT-probe Match advances to KVT1 and passively holds (no forced ACK — OCF-5/B1)"
    );
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        1,
        "exactly one send_chk across both phases — probe must NOT resend"
    );
    assert!(
        last_calls.load(Ordering::SeqCst) >= 1,
        "at least one lastChk (the recovery probe)"
    );
    assert_eq!(count_doc_rows(&pool).await, 1, "exactly one ledger row");
    prro::db::invariant_scan::assert_clean(&pool).await;
}

// ════════════════════════════════════════════════════════════════════════════
// K3 (critical) — SENDING committed, wire submit in flight.
//
// Build:    full online `inline::run`; the 4-pre envelope commits Sending,
//           then send_chk PARKS on a oneshot.  When the send_chk await is
//           reached, SENDING is durable, so the test drops the `inline::run`
//           future (drop-injection).  The wire result is indeterminate.
// Recovery: boot — the Sending arm (:2672) is ctx-free
//           (`resume_sending_to_error_retryable`): CAS Sending→ErrorRetryable,
//           NO resend ("DPS does not deduplicate; re-sending would be a
//           duplicate-document hazard").
// Locked:   doc → ERROR_RETRYABLE; `send_chk` total == 1 and NEVER 2 — the
//           auto-resend is FORBIDDEN (incomplete trace ⇒ ER-guard must hold
//           HoldIndeterminate); scan clean.  Resolving the hung ER is a
//           probe/B1 concern (feed to spec B1), not an auto-resend.
//
// NB: the scan runs ONLY after recovery — a SENDING doc at rest is itself a
// StuckSending violation that recovery is obligated to clear.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn k3_sending_committed_resumes_to_error_retryable_without_resend() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_online(&pool, shift_id).await;
    let row = seed_inbox_sell(&pool).await;

    let send_calls = Arc::new(AtomicUsize::new(0));
    let last_calls = Arc::new(AtomicUsize::new(0));

    // ── Phase 1: send_chk parks after Sending is committed → drop.
    let phase1 = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    let (reached_tx, reached_rx) = oneshot::channel::<()>();
    let (block_tx, block_rx) = oneshot::channel::<()>();
    phase1.hang_send(reached_tx, block_rx);

    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;

    {
        let mut fut = Box::pin(inline::run(
            &pool,
            &pool_secure,
            &phase1,
            &sign_ctx,
            &fn_sign,
            &guard,
            &row,
        ));
        tokio::select! {
            _ = &mut fut => panic!("inline::run must hang on send_chk, not complete"),
            _ = reached_rx => { /* send_chk await reached ⇒ Sending already committed */ }
        }
        drop(fut); // cancel the parked send_chk await — the "crash" mid-wire
    }
    let _keep_block_tx = block_tx;
    drop(guard);

    // Post-crash committed state: SENDING (Pattern B intent marker), one send.
    assert_eq!(read_doc_state(&pool, FN).await, "SENDING", "K3 crash point");
    assert_eq!(send_calls.load(Ordering::SeqCst), 1, "one send in phase 1");

    // ── Phase 2: boot recovery — Sending arm is ctx-free, deps None.
    boot_phase::run_boot_reconciliation(&recon_guard(), &pool, FN, None)
        .await
        .expect("boot reconciliation must succeed");

    // ── Locked assertions.
    assert_eq!(
        read_doc_state(&pool, FN).await,
        "ERROR_RETRYABLE",
        "Sending arm downgrades to ERROR_RETRYABLE (HoldIndeterminate, no resend)"
    );
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        1,
        "send_chk total == 1 and NEVER 2 — auto-resend is forbidden"
    );
    assert_eq!(count_doc_rows(&pool).await, 1, "exactly one ledger row");
    // Now-clean: ERROR_RETRYABLE is a legal resting state; the StuckSending
    // violation that existed at the crash point has been cleared by recovery.
    prro::db::invariant_scan::assert_clean(&pool).await;
}

// ════════════════════════════════════════════════════════════════════════════
// K1 — PREPARED committed, nothing past acquire.
//
// Build:    manual `stage_acquire::run` only (mirrors inline.rs acquire) →
//           doc PREPARED, inbox PROCESSING.
// Recovery: boot with deps Some on the Online node.
//
// **FINDING / architect ruling (2026-06-11).**  The locked matrix asked for
// `send_chk == 0`, but boot's Prepared arm (`dispatch_prepared_via_chain`,
// boot_phase.rs:2570 sign + :2595 stage_send) with deps=Some on an Online node
// re-drives the never-sent doc to its FIRST wire submit — `send_chk == 1` and
// the doc advances PREPARED→SIGNED→SENT.  Confirmed by code AND empirically.
// Architect accepted the actual behaviour (option A): boot-recon with deps is
// the W9 standard re-driver for pre-wire states; PREPARED/SIGNED never touched
// the wire so re-drive is duplicate-free.  The real invariant is "exactly one
// send per check TOTAL" (phase-1=0 + recovery=1 = 1), NOT "zero sends on boot".
// The Pattern-B "no resend without probe" rule applies only to SENDING-committed
// docs (that is K3).  The locked `==0` is corrected to `==1` under this ruling.
// (A second boot would SENT-probe → KVT1, as K4.)
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn k1_prepared_boot_redrives_to_sent_exactly_once() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_online(&pool, shift_id).await;
    let row = seed_inbox_sell(&pool).await;

    // ── Phase 1: manual acquire only → PREPARED.
    let command = build_canonical(&row).expect("build_canonical");
    let driver_id = row.driver_id.clone().expect("driver_id present");
    let acq = stage_acquire::run(&pool, &pool_secure, &driver_id, row.request_id, command)
        .await
        .expect("stage_acquire must succeed");
    match acq {
        WorkerProcessResult::Proceed(_) | WorkerProcessResult::Resumed(_) => {}
        other => panic!("unexpected acquire outcome: {other:?}"),
    }
    assert_eq!(
        read_doc_state(&pool, FN).await,
        "PREPARED",
        "K1 crash point"
    );
    assert_eq!(
        read_inbox_status(&pool, &row.request_id).await,
        "PROCESSING"
    );
    assert_eq!(count_doc_rows(&pool).await, 1, "exactly one ledger row");

    // ── Phase 2: boot with deps Some on the Online node.
    let send_calls = Arc::new(AtomicUsize::new(0));
    let last_calls = Arc::new(AtomicUsize::new(0));
    let stub = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    stub.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    boot_phase::run_boot_reconciliation(&recon_guard(), &pool, FN, Some(&view))
        .await
        .expect("boot reconciliation must succeed");

    // ── After tick 1: doc at SENT, exactly one send so far.
    assert_eq!(
        read_doc_state(&pool, FN).await,
        "SENT",
        "boot tick 1: Prepared arm signs + sends → SENT"
    );
    assert_eq!(send_calls.load(Ordering::SeqCst), 1, "one send on tick 1");
    assert_eq!(
        last_calls.load(Ordering::SeqCst),
        0,
        "no lastChk on tick 1 — dispatch stops at SENT"
    );

    // ── Idempotency pin (architect ruling): a SECOND boot tick on the same
    //    pool — deps Some, lastChk → Match.  The Sent arm PROBES (no resend)
    //    and advances Sent→KVT1 (as K4).  send_chk MUST stay == 1: this turns
    //    "one send so far" into "exactly one send EVER".  Counters are shared.
    let stub2 = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    stub2.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF])));
    let sign_ctx2 = det_signing_ctx();
    let fn_sign2 = fn_sign_blob();
    let view2 = RuntimeView {
        dps: &stub2,
        signing_ctx: &sign_ctx2,
        fn_sign: &fn_sign2,
    };
    boot_phase::run_boot_reconciliation(&recon_guard(), &pool, FN, Some(&view2))
        .await
        .expect("second boot reconciliation must succeed");

    // ── Final: exactly one send EVER, doc probe-advanced to KVT1.
    assert_eq!(
        read_doc_state(&pool, FN).await,
        "KVT1",
        "tick 2: SENT-probe Match → KVT1 (no resend)"
    );
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        1,
        "exactly one send_chk EVER across both boot ticks"
    );
    assert!(
        last_calls.load(Ordering::SeqCst) >= 1,
        "tick 2 issued the lastChk probe"
    );
    assert_eq!(
        count_doc_rows(&pool).await,
        1,
        "still exactly one ledger row"
    );
    prro::db::invariant_scan::assert_clean(&pool).await;
}

// ════════════════════════════════════════════════════════════════════════════
// K2 — SIGNED committed, nothing past sign.
//
// Build:    manual acquire + `stage_sign::run` → doc SIGNED.
// Recovery: boot with deps Some on the Online node — Signed arm (:2766
//           dispatch_post_sign → :2768 stage_send) re-drives to SENT.
// FINDING:  same architect ruling A as K1 — `send_chk == 1` (not the locked
//           `==0`); doc → SENT.  phase-1=0 + recovery=1 = exactly one send.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn k2_signed_boot_redrives_to_sent_exactly_once() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_online(&pool, shift_id).await;
    let row = seed_inbox_sell(&pool).await;

    // ── Phase 1: manual acquire + sign → SIGNED.
    let command = build_canonical(&row).expect("build_canonical");
    let driver_id = row.driver_id.clone().expect("driver_id present");
    let acq = stage_acquire::run(&pool, &pool_secure, &driver_id, row.request_id, command)
        .await
        .expect("stage_acquire must succeed");
    let ctx = match acq {
        WorkerProcessResult::Proceed(c) | WorkerProcessResult::Resumed(c) => c,
        other => panic!("unexpected acquire outcome: {other:?}"),
    };
    let sign_ctx0 = det_signing_ctx();
    stage_sign::run(&pool, &sign_ctx0, ctx)
        .await
        .expect("stage_sign must succeed");
    assert_eq!(read_doc_state(&pool, FN).await, "SIGNED", "K2 crash point");
    assert_eq!(count_doc_rows(&pool).await, 1, "exactly one ledger row");

    // ── Phase 2: boot with deps Some on the Online node.
    let send_calls = Arc::new(AtomicUsize::new(0));
    let last_calls = Arc::new(AtomicUsize::new(0));
    let stub = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    stub.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    boot_phase::run_boot_reconciliation(&recon_guard(), &pool, FN, Some(&view))
        .await
        .expect("boot reconciliation must succeed");

    // ── After tick 1: doc at SENT, exactly one send so far.
    assert_eq!(
        read_doc_state(&pool, FN).await,
        "SENT",
        "boot tick 1: Signed arm sends → SENT"
    );
    assert_eq!(send_calls.load(Ordering::SeqCst), 1, "one send on tick 1");
    assert_eq!(
        last_calls.load(Ordering::SeqCst),
        0,
        "no lastChk on tick 1 — dispatch stops at SENT"
    );

    // ── Idempotency pin (architect ruling): a SECOND boot tick on the same
    //    pool — deps Some, lastChk → Match.  The Sent arm PROBES (no resend)
    //    and advances Sent→KVT1 (as K4).  send_chk MUST stay == 1: this turns
    //    "one send so far" into "exactly one send EVER".  Counters are shared.
    let stub2 = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    stub2.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF])));
    let sign_ctx2 = det_signing_ctx();
    let fn_sign2 = fn_sign_blob();
    let view2 = RuntimeView {
        dps: &stub2,
        signing_ctx: &sign_ctx2,
        fn_sign: &fn_sign2,
    };
    boot_phase::run_boot_reconciliation(&recon_guard(), &pool, FN, Some(&view2))
        .await
        .expect("second boot reconciliation must succeed");

    // ── Final: exactly one send EVER, doc probe-advanced to KVT1.
    assert_eq!(
        read_doc_state(&pool, FN).await,
        "KVT1",
        "tick 2: SENT-probe Match → KVT1 (no resend)"
    );
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        1,
        "exactly one send_chk EVER across both boot ticks"
    );
    assert!(
        last_calls.load(Ordering::SeqCst) >= 1,
        "tick 2 issued the lastChk probe"
    );
    assert_eq!(
        count_doc_rows(&pool).await,
        1,
        "still exactly one ledger row"
    );
    prro::db::invariant_scan::assert_clean(&pool).await;
}
