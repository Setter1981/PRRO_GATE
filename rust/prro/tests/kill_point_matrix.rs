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
use prro::db::repositories::transport_trace::{self, NewAttempt};
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
use prro::transports::dps::error::{AuthorizationKind, DpsError};
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
//           arm is a passive hold (:2676).  After boot the doc rests at KVT1.
// Phase 3:  the B1 online-convergence tick (audit pass-2, item 3) is now the
//           runtime owner of resting online KVT1 — it converges KVT1 → ACK via
//           the reused Kvt1Reentry confirm path WITHOUT resending.  This closes
//           the OCF-5 hole this matrix originally surfaced ("no forced ACK").
// Locked:   after boot — doc KVT1, `send_chk` total == 1, `last_chk` ≥ 1, scan
//           clean.  After the B1 tick — doc ACK, `send_chk` STILL == 1, scan
//           clean (the tick issues only lastChk, never a resend).
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

    // ── Phase 3 (B1): the online-convergence tick OWNS the resting KVT1 and
    //    converges it to ACK — without resending.  Counters stay SHARED, so the
    //    "exactly one send_chk EVER" property is proven across crash + boot + tick.
    let phase3 = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    phase3.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF]))); // confirm Match
    let sign_ctx3 = det_signing_ctx();
    let fn_sign3 = fn_sign_blob();
    let view3 = RuntimeView {
        dps: &phase3,
        signing_ctx: &sign_ctx3,
        fn_sign: &fn_sign3,
    };
    let summary =
        prro::services::reconciliation::online_convergence::run_tick_for_fn(&pool, &view3, FN)
            .await
            .expect("B1 convergence tick must succeed");

    assert_eq!(
        read_doc_state(&pool, FN).await,
        "ACK",
        "B1 tick converges the resting KVT1 → ACK (closes the matrix-found OCF-5 hole)"
    );
    assert_eq!(summary.acked_from_kvt1, 1, "tick acked the resting KVT1");
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        1,
        "send_chk STILL == 1 across crash + boot + B1 tick — the tick never resends"
    );
    assert_eq!(
        count_doc_rows(&pool).await,
        1,
        "still exactly one ledger row"
    );
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

// ─── M1-02 reachability pin (architect ruling item 2(a), 2026-06-11) ─────────
//
// Question the dossier left open: is the "≥2 acked SENT docs on one FN"
// precondition of the M1-02 Mismatch→Manual escalation actually REACHABLE on a
// live online lane?  Drive two sequential online `inline::run` on ONE FN: the
// first rests SENT via the online-confirm Hold branch; the second is a fresh
// SELL on the same FN while doc1 is still SENT.  Record EXACTLY what the second
// does (acquire? lnd? MAC previous_hash? final state?) plus the `invariant_scan`
// verdict.  The recorded outcome decides the final severity of M1-02 and is
// transcribed verbatim into the PR body — whatever it turns out to be.
async fn seed_inbox_sell_keyed(pool: &SqlitePool, idem: &str) -> InboxRow {
    let req_id = RequestId::new();
    let request_id: [u8; 16] = *req_id.as_bytes();
    let payload_sha256_canonical: [u8; 32] = Sha256::digest(SELL_PAYLOAD.as_bytes()).into();
    inbox::insert(
        pool,
        &NewInboxEntry {
            request_id,
            fiscal_number: FN.into(),
            protocol: Protocol::Rest,
            operation_type: "SELL".into(),
            idempotency_key: idem.into(),
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
        idempotency_key: idem.into(),
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

#[tokio::test]
async fn m1_02_reachability_second_sell_while_first_rests_sent() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_online(&pool, shift_id).await;
    let gate = Arc::new(tokio::sync::Mutex::new(()));

    // ── Run #1: online SELL, lastChk Hold (empty data_sign) → doc1 rests SENT.
    let row1 = seed_inbox_sell(&pool).await;
    let stub1 = KpStub::new(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    stub1.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    stub1.push_last(Ok(ack(SERVER_FISCAL_NO, vec![])));
    let sign_ctx1 = det_signing_ctx();
    let fn_sign1 = fn_sign_blob();
    {
        let guard = gate.clone().lock_owned().await;
        let o1 = inline::run(
            &pool,
            &pool_secure,
            &stub1,
            &sign_ctx1,
            &fn_sign1,
            &guard,
            &row1,
        )
        .await
        .expect("run #1: online SELL with Hold lastChk returns Ok(Sent)");
        assert_eq!(o1.document_state, DocState::Sent, "doc1 rests at SENT");
    }

    // ── Run #2: a SECOND SELL on the SAME FN while doc1 is still SENT.  Distinct
    //    server_fiscal_no on the wire so a successful second send would create the
    //    "two SENT docs with distinct sfn" precondition M1-02 escalates on.
    let row2 = seed_inbox_sell_keyed(&pool, "idem-kpm-SELL-2").await;
    let stub2 = KpStub::new(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    stub2.push_send(Ok(ack("DPS-FN-ONLINE-2", vec![])));
    stub2.push_last(Ok(ack("DPS-FN-ONLINE-2", vec![])));
    let sign_ctx2 = det_signing_ctx();
    let fn_sign2 = fn_sign_blob();
    let result2 = {
        let guard = gate.clone().lock_owned().await;
        inline::run(
            &pool,
            &pool_secure,
            &stub2,
            &sign_ctx2,
            &fn_sign2,
            &guard,
            &row2,
        )
        .await
    };

    // ── Observe + record (emitted on every run as the PR-body evidence line).
    let all: Vec<(i64, String, Option<String>)> = sqlx::query_as(
        "SELECT lnd, state, server_fiscal_no FROM fiscal_documents \
         WHERE fiscal_number = ? ORDER BY lnd",
    )
    .bind(FN)
    .fetch_all(&pool)
    .await
    .unwrap();
    let violations = prro::db::invariant_scan::scan(&pool).await.unwrap();
    eprintln!(
        "M1-02 REACHABILITY: result2_ok={} result2_state={:?} docs={:?} invariant_violations={}",
        result2.is_ok(),
        result2.as_ref().map(|o| format!("{:?}", o.document_state)),
        all,
        violations.len()
    );
    for v in &violations {
        eprintln!("  violation: {v:?}");
    }

    // RECORDED OUTCOME (2026-06-11, --nocapture), pinned verbatim:
    //   result2_ok=true result2_state=Ok("Sent")
    //   docs=[(1,"SENT","DPS-FN-ONLINE-1"),(2,"SENT","DPS-FN-ONLINE-2")]
    //   invariant_violations=0
    // => The M1-02 precondition ("≥2 SENT docs on ONE FN with DISTINCT
    //    server_fiscal_no") IS reachable on a live online lane against a
    //    permissive DPS stub, and `invariant_scan` does NOT flag it.  M1-02 is
    //    therefore a REACHABLE condition, not a theoretical one — the boot/online
    //    Mismatch arm (and the item-2(b) lnd<max superseded softening) operate on
    //    a state the system can actually enter.  The escalation severity stands.
    let result2 = result2.expect("run #2: second SELL on the same FN also reaches SENT");
    assert_eq!(
        result2.document_state,
        DocState::Sent,
        "doc2 also rests at SENT"
    );
    assert_eq!(all.len(), 2, "two SELL docs issued on one FN");
    assert_eq!(
        all[0],
        (1, "SENT".to_string(), Some(SERVER_FISCAL_NO.to_string()))
    );
    assert_eq!(
        all[1],
        (2, "SENT".to_string(), Some("DPS-FN-ONLINE-2".to_string())),
        "doc2 carries a DISTINCT server_fiscal_no — the M1-02 precondition is met"
    );
    assert!(
        violations.is_empty(),
        "invariant_scan does NOT flag two SENT docs with distinct sfn: {violations:?}"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// K7 — boot SENT-probe: trace-alloc committed, CAS not yet run (M1-08).
//
// The ONLY multi-envelope boot window with durable PRE-CAS state:
// `dispatch_sent_via_probe` allocates a transport_trace recovery row in one
// `with_immediate` (Envelope 1), probes the wire, then CASes Sent→Kvt1 in a
// SECOND envelope.  A crash between Envelope 1 and the CAS leaves a durable
// ORPHAN probe row; the next boot re-runs the arm and allocates ANOTHER probe
// row — the "double-allocation".
//
// Build:    online inline::run, lastChk Hold (empty) → doc rests SENT (one
//           is_probe=0 send row, attempt_no=1).  Then hand-allocate ONE orphan
//           probe row (is_probe=1, attempt_no=2) via the SAME prod call the arm
//           uses — this IS the pre-CAS crash residue.
// Recovery: boot with a Match stub → the arm allocates a SECOND probe row
//           (is_probe=1, attempt_no=3) and CASes Sent→Kvt1.
// Locked (M1-08 + M1-04): doc converges to KVT1; ZERO send_chk in recovery
//           (the probe never resends — exactly one send EVER); the two probe
//           rows are BENIGN — `attempts_used` (the ER-redrive budget) counts
//           is_probe=0 rows ONLY, so it stays 1 despite MAX(attempt_no)=3 (the
//           budget-pollution harm the orphan double-alloc used to cause is gone
//           post-M1-04); scan clean.  No resume mechanics — the orphan is left
//           as an inert audit row.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn k7_sent_probe_alloc_orphan_is_benign_after_m1_04() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_online(&pool, shift_id).await;
    let row = seed_inbox_sell(&pool).await;

    // ── Phase 1: online SELL, lastChk Hold (empty) → doc rests SENT (one send).
    let send_calls = Arc::new(AtomicUsize::new(0));
    let last_calls = Arc::new(AtomicUsize::new(0));
    let phase1 = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    phase1.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    phase1.push_last(Ok(ack(SERVER_FISCAL_NO, vec![]))); // empty → Hold → SENT
    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    {
        let guard = gate.clone().lock_owned().await;
        inline::run(
            &pool,
            &pool_secure,
            &phase1,
            &sign_ctx,
            &fn_sign,
            &guard,
            &row,
        )
        .await
        .expect("phase 1 rests at SENT");
    }
    assert_eq!(read_doc_state(&pool, FN).await, "SENT", "K7 setup: SENT");
    assert_eq!(send_calls.load(Ordering::SeqCst), 1, "one send in phase 1");

    let doc_id = read_doc_id(&pool).await;
    assert_eq!(
        transport_trace::attempts_used(&pool, doc_id).await.unwrap(),
        1,
        "one send row (is_probe=0) so far"
    );

    // ── Pre-CAS crash residue: hand-allocate ONE orphan probe row (is_probe=1,
    //    attempt_no=2) via the EXACT prod call `dispatch_sent_via_probe` uses —
    //    simulating a probe whose trace-alloc envelope committed before a crash.
    let doc = fiscal_documents::list_pending_for_fn(&pool, FN)
        .await
        .unwrap()
        .into_iter()
        .find(|d| d.document_id == doc_id)
        .expect("SENT doc in pending list");
    let backend_id = doc.backend_profile_id.clone();
    let transport_id = doc.transport_profile_id.clone();
    let orphan_attempt = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            transport_trace::allocate_and_insert_tx(
                tx,
                doc_id,
                NewAttempt {
                    backend_profile_id: backend_id,
                    transport_profile_id: transport_id,
                    request_envelope_sha256: [0u8; 32],
                    is_probe: true,
                },
            )
            .await
            .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("orphan probe trace row");
    assert_eq!(orphan_attempt, 2, "orphan probe takes attempt_no=2");
    assert_eq!(
        transport_trace::attempts_used(&pool, doc_id).await.unwrap(),
        1,
        "orphan probe row excluded from the ER-redrive budget"
    );

    // ── Recovery: boot with a Match stub.  No push_send queued → a resend would
    //    be observable; asserted ZERO below.
    let phase2 = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    phase2.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF]))); // Match
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

    // ── Locked assertions.
    assert_eq!(
        read_doc_state(&pool, FN).await,
        "KVT1",
        "SENT-probe Match advances to KVT1 despite the orphan probe row"
    );
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        1,
        "ZERO send_chk in recovery — exactly one send EVER (the phase-1 original)"
    );
    // Two probe rows now exist (orphan attempt_no=2 + boot attempt_no=3): the
    // accepted-benign double-allocation (M1-08).
    let probe_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM transport_trace WHERE document_id = ? AND is_probe = 1",
    )
    .bind(doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        probe_rows, 2,
        "orphan + boot probe rows (benign double-allocation)"
    );
    // THE M1-08/M1-04 PIN: MAX(attempt_no)=3 but the ER-redrive budget counts
    // is_probe=0 rows ONLY, so it is UNPOLLUTED by the two probe rows.
    assert_eq!(
        transport_trace::attempts_used(&pool, doc_id).await.unwrap(),
        1,
        "attempts_used counts only the one real send — probe double-alloc is benign"
    );
    assert_eq!(count_doc_rows(&pool).await, 1, "exactly one ledger row");
    prro::db::invariant_scan::assert_clean(&pool).await;
}

// ════════════════════════════════════════════════════════════════════════════
// K4b (NC-01) — SENT-probe Match with a matching id but EMPTY data_sign.
//
// External-critic finding NC-01 (adjudicated 2026-06-11): `last_chk_probe::probe`
// Matches on `ack.id` alone and `advance_sent_to_kvt1_from_probe` persisted
// `ack.data_sign` into KVT1_RAW with NO `is_empty` guard — so the probe path
// could advance SENT→KVT1 persisting EMPTY forensic KVT1 bytes, diverging from
// the W12 classify contract (empty data_sign → Hold) and `advance_to_ack`'s own
// StructuralDrift-on-empty guard.
//
// Build:    online inline::run, lastChk Hold (empty) → doc rests SENT.
// Recovery: boot with a probe stub returning a MATCHING id + `vec![]` data_sign.
// Locked (the fix mirrors `advance_to_ack`): the advance fail-louds on empty, so
//   the doc HOLDS at SENT (no advance), NO KVT1_RAW row is persisted (let alone
//   an empty one), ZERO send in recovery, scan clean.  Next probe tick retries.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn k4b_sent_probe_matching_id_empty_data_sign_holds_at_sent() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_online(&pool, shift_id).await;
    let row = seed_inbox_sell(&pool).await;

    // ── Phase 1: online SELL, lastChk Hold (empty) → doc rests SENT.
    let send_calls = Arc::new(AtomicUsize::new(0));
    let last_calls = Arc::new(AtomicUsize::new(0));
    let phase1 = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    phase1.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    phase1.push_last(Ok(ack(SERVER_FISCAL_NO, vec![]))); // empty → online Hold → SENT
    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    {
        let guard = gate.clone().lock_owned().await;
        inline::run(
            &pool,
            &pool_secure,
            &phase1,
            &sign_ctx,
            &fn_sign,
            &guard,
            &row,
        )
        .await
        .expect("phase 1 rests at SENT");
    }
    assert_eq!(read_doc_state(&pool, FN).await, "SENT", "K4b setup: SENT");

    // ── Recovery: boot probe returns the MATCHING id but EMPTY data_sign.
    let phase2 = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    phase2.push_last(Ok(ack(SERVER_FISCAL_NO, vec![]))); // matching id, EMPTY data_sign
    let sign_ctx2 = det_signing_ctx();
    let fn_sign2 = fn_sign_blob();
    let view = RuntimeView {
        dps: &phase2,
        signing_ctx: &sign_ctx2,
        fn_sign: &fn_sign2,
    };
    boot_phase::run_boot_reconciliation(&recon_guard(), &pool, FN, Some(&view))
        .await
        .expect("boot reconciliation must succeed (empty probe holds, not errors fatally)");

    // ── Locked assertions (NC-01).
    assert_eq!(
        read_doc_state(&pool, FN).await,
        "SENT",
        "matching id + empty data_sign must NOT advance SENT→KVT1 (mirror advance_to_ack)"
    );
    let kvt1_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM document_files WHERE kind = 'KVT1_RAW'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        kvt1_rows, 0,
        "no KVT1_RAW persisted — empty forensic evidence is never written"
    );
    let empty_kvt1: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM document_files WHERE kind = 'KVT1_RAW' AND length(content) = 0",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(empty_kvt1, 0, "non-empty KVT1_RAW invariant holds");
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        1,
        "ZERO send in recovery — the probe never resends"
    );
    assert_eq!(count_doc_rows(&pool).await, 1, "exactly one ledger row");
    prro::db::invariant_scan::assert_clean(&pool).await;
}

// ════════════════════════════════════════════════════════════════════════════
// M2-01 (FT) pin — two REAL-path offline receipts in one session drain to ACK
// with a VALID per-doc MAC chain.  Pre-fix this was the FT: both receipts signed
// the same previous_hash, doc#2 hit ChainSeedMismatch and stuck at KVT2, drain
// hard-aborted.  Post-fix: offline-ack advances the seed per issued doc, so
// doc#2 chains off doc#1; drain's finalize skips the guard/advance for
// offline-origin docs; both reach ACK; invariant_scan is clean BEFORE and AFTER.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn m2_two_offline_receipts_real_chain_drains_both_to_ack() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_offline(&pool, shift_id).await;
    seed_open_offline_session(&pool).await;
    seed_offline_code(&pool, 1).await;
    seed_offline_code(&pool, 2).await;

    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let gate = Arc::new(tokio::sync::Mutex::new(()));

    // ── Two REAL offline receipts (inline::run on an offline node).
    let row1 = seed_inbox_sell(&pool).await;
    let stub1 = KpStub::new(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    {
        let g = gate.clone().lock_owned().await;
        let o = inline::run(&pool, &pool_secure, &stub1, &sign_ctx, &fn_sign, &g, &row1)
            .await
            .expect("offline receipt 1 → OFFLINE_LOCAL_ACK");
        assert_eq!(o.document_state, DocState::OfflineLocalAck);
    }
    let row2 = seed_inbox_sell_keyed(&pool, "idem-m2-pin-2").await;
    let stub2 = KpStub::new(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    {
        let g = gate.clone().lock_owned().await;
        let o = inline::run(&pool, &pool_secure, &stub2, &sign_ctx, &fn_sign, &g, &row2)
            .await
            .expect("offline receipt 2 → OFFLINE_LOCAL_ACK");
        assert_eq!(o.document_state, DocState::OfflineLocalAck);
    }

    // ── The chain is now REAL: doc#1.prev = genesis (None), doc#2.prev =
    //    doc#1.unsigned (distinct), and the node seed = doc#2.unsigned.
    async fn chain_col(pool: &SqlitePool, lnd: i64, col: &str) -> Option<Vec<u8>> {
        let q = format!("SELECT {col} FROM fiscal_documents WHERE fiscal_number = ? AND lnd = ?");
        sqlx::query_scalar(&q)
            .bind(FN)
            .bind(lnd)
            .fetch_one(pool)
            .await
            .unwrap()
    }
    let prev1 = chain_col(&pool, 1, "previous_hash").await;
    let prev2 = chain_col(&pool, 2, "previous_hash").await;
    let uns1 = chain_col(&pool, 1, "unsigned_xml_sha256").await;
    let uns2 = chain_col(&pool, 2, "unsigned_xml_sha256").await;
    assert!(prev1.is_none(), "doc#1 chains off genesis");
    assert_ne!(
        prev1, prev2,
        "M2-01: the two offline docs have DISTINCT previous_hash"
    );
    assert_eq!(prev2, uns1, "doc#2 chains off doc#1's unsigned_xml_sha256");
    let seed: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number = ?",
    )
    .bind(FN)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        seed, uns2,
        "node seed advanced to the last issued offline doc"
    );

    // ── invariant_scan is CLEAN during the offline window (closes M2-03).
    prro::db::invariant_scan::assert_clean(&pool).await;

    // ── Go online + drain.  Two send + two lastChk (one per doc).
    set_node_mode(&pool, NodeMode::GoingOnline).await;
    let send_calls = Arc::new(AtomicUsize::new(0));
    let last_calls = Arc::new(AtomicUsize::new(0));
    let phase = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    phase.push_send(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF])));
    phase.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF])));
    phase.push_send(Ok(ack("DPS-FN-ONLINE-2", vec![0xDE, 0xAD, 0xBE, 0xEF])));
    phase.push_last(Ok(ack("DPS-FN-ONLINE-2", vec![0xDE, 0xAD, 0xBE, 0xEF])));
    let sc2 = det_signing_ctx();
    let fs2 = fn_sign_blob();
    let view = RuntimeView {
        dps: &phase,
        signing_ctx: &sc2,
        fn_sign: &fs2,
    };
    let summary = backlog_drain::drain(&recon_guard(), &pool, &view, FN)
        .await
        .expect("drain must succeed (no ChainSeedMismatch post-fix)");
    assert_eq!(summary.backlog_size_before(), 2);

    // ── Both ACK, exactly two send_chk, scan clean AFTER drain.
    let states: Vec<(i64, String)> = sqlx::query_as(
        "SELECT lnd, state FROM fiscal_documents WHERE fiscal_number = ? ORDER BY lnd",
    )
    .bind(FN)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        states,
        vec![(1, "ACK".to_string()), (2, "ACK".to_string())],
        "both offline receipts drain to ACK"
    );
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        2,
        "exactly two send_chk (one per doc)"
    );
    prro::db::invariant_scan::assert_clean(&pool).await;
}

// ════════════════════════════════════════════════════════════════════════════
// M2-01 boundary pin — the online↔offline MAC chain is CONTINUOUS.  An online
// doc fiscalises at ACK (seed advances there); the first offline doc of the next
// session must chain off that last online ACK (the seed pointer is shared).
// Proves the online→offline boundary + scan-clean across it.  (The offline→online
// direction is symmetric — m2_two_offline_… leaves the seed at the last offline
// doc, which a subsequent online doc reads at sign.)
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn m2_online_offline_boundary_chain_continuous() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_online(&pool, shift_id).await;
    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let gate = Arc::new(tokio::sync::Mutex::new(()));

    async fn doc_id_by_req(pool: &SqlitePool, req: &[u8; 16]) -> DocumentId {
        sqlx::query_scalar("SELECT document_id FROM fiscal_documents WHERE request_id = ?")
            .bind(&req[..])
            .fetch_one(pool)
            .await
            .unwrap()
    }
    async fn state_by_id(pool: &SqlitePool, id: DocumentId) -> String {
        sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }
    async fn col_by_id(pool: &SqlitePool, id: DocumentId, col: &str) -> Option<Vec<u8>> {
        let q = format!("SELECT {col} FROM fiscal_documents WHERE document_id = ?");
        sqlx::query_scalar(&q)
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    // ── doc#0 ONLINE → ACK (inline Hold→SENT, manual Kvt1/Kvt2, boot finalize).
    let row0 = seed_inbox_sell(&pool).await;
    let stub0 = KpStub::new(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    stub0.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    stub0.push_last(Ok(ack(SERVER_FISCAL_NO, vec![]))); // empty → online Hold → SENT
    {
        let g = gate.clone().lock_owned().await;
        let o = inline::run(&pool, &pool_secure, &stub0, &sign_ctx, &fn_sign, &g, &row0)
            .await
            .unwrap();
        assert_eq!(o.document_state, DocState::Sent);
    }
    let doc0 = doc_id_by_req(&pool, &row0.request_id).await;
    let kvt1 = vec![0xDE, 0xAD, 0xBE, 0xEF];
    with_immediate(&pool, move |tx| {
        Box::pin(async move {
            fiscal_documents::transition_state(tx, doc0, DocState::Sent, DocState::Kvt1)
                .await
                .map_err(anyhow::Error::from)?;
            fiscal_documents::transition_state(tx, doc0, DocState::Kvt1, DocState::Kvt2)
                .await
                .map_err(anyhow::Error::from)?;
            document_files::replace_tx(tx, doc0, DocumentFileKind::Kvt1Raw, &kvt1).await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await
    .unwrap();
    boot_phase::run_boot_reconciliation(&recon_guard(), &pool, FN, None)
        .await
        .unwrap();
    assert_eq!(state_by_id(&pool, doc0).await, "ACK", "doc#0 online → ACK");
    let uns0 = col_by_id(&pool, doc0, "unsigned_xml_sha256").await;
    let seed0: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number = ?",
    )
    .bind(FN)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        seed0, uns0,
        "online ACK advanced the seed to doc#0.unsigned"
    );

    // ── Flip OFFLINE, emit doc#1 → OFFLINE_LOCAL_ACK; it must chain off doc#0.
    set_node_mode(&pool, NodeMode::Offline).await;
    seed_open_offline_session(&pool).await;
    seed_offline_code(&pool, 1).await;
    let row1 = seed_inbox_sell_keyed(&pool, "idem-m2-boundary-1").await;
    let stub1 = KpStub::new(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    {
        let g = gate.clone().lock_owned().await;
        let o = inline::run(&pool, &pool_secure, &stub1, &sign_ctx, &fn_sign, &g, &row1)
            .await
            .unwrap();
        assert_eq!(o.document_state, DocState::OfflineLocalAck);
    }
    let doc1 = doc_id_by_req(&pool, &row1.request_id).await;
    let prev1 = col_by_id(&pool, doc1, "previous_hash").await;
    assert_eq!(
        prev1, uns0,
        "online→offline boundary: the first offline doc chains off the last online ACK"
    );
    // Chain continuous across the boundary.
    prro::db::invariant_scan::assert_clean(&pool).await;
}

// ════════════════════════════════════════════════════════════════════════════
// M2-X1 (external-critic HIGH, adjudicated 2026-06-12): mac_recovery must REFUSE
// offline-origin docs.  W10.4 mac-recovery (DPS -12 ERROR_BAD_HASH_PREV) re-signs
// previous_hash + unsigned_xml_sha256 but does NOT advance node_state seed — it
// was designed for online docs (finalize advances after).  For an offline-origin
// doc the seed already advanced at offline-ack (M2-01), so a re-sign breaks the
// local chain (doc#2.prev goes stale; seed desyncs → invariant_scan ChainBreak).
//
// Pre-fix RED: drain doc#1 with -12 → mac_recovery re-signs → ChainBreak.
// Post-fix GREEN: mac_recovery refuses (no re-sign), the pending-drain shift
// escalates to RequiresManualReconciliation, the chain stays intact, scan clean,
// distinct MAC_RECOVERY_OFFLINE_ORIGIN_REFUSED audit.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn m2x1_mac_recovery_refuses_offline_origin_doc() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_offline(&pool, shift_id).await;
    seed_open_offline_session(&pool).await;
    seed_offline_code(&pool, 1).await;
    seed_offline_code(&pool, 2).await;

    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let gate = Arc::new(tokio::sync::Mutex::new(()));

    // Two REAL offline receipts (distinct previous_hash via M2-01).
    let row1 = seed_inbox_sell(&pool).await;
    {
        let g = gate.clone().lock_owned().await;
        let stub = KpStub::new(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let o = inline::run(&pool, &pool_secure, &stub, &sign_ctx, &fn_sign, &g, &row1)
            .await
            .unwrap();
        assert_eq!(o.document_state, DocState::OfflineLocalAck);
    }
    let row2 = seed_inbox_sell_keyed(&pool, "idem-m2x1-2").await;
    {
        let g = gate.clone().lock_owned().await;
        let stub = KpStub::new(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let o = inline::run(&pool, &pool_secure, &stub, &sign_ctx, &fn_sign, &g, &row2)
            .await
            .unwrap();
        assert_eq!(o.document_state, DocState::OfflineLocalAck);
    }

    async fn col1(pool: &SqlitePool, c: &str) -> Option<Vec<u8>> {
        let q = format!("SELECT {c} FROM fiscal_documents WHERE fiscal_number = ? AND lnd = 1");
        sqlx::query_scalar(&q)
            .bind(FN)
            .fetch_one(pool)
            .await
            .unwrap()
    }
    let doc1_unsigned_before = col1(&pool, "unsigned_xml_sha256").await;
    let doc1_prev_before = col1(&pool, "previous_hash").await;
    prro::db::invariant_scan::assert_clean(&pool).await; // chain clean pre-drain

    // Go online; flip the shift into pending-drain so a manual-recon failure
    // escalates (the §16.7 ladder).
    set_node_mode(&pool, NodeMode::GoingOnline).await;
    sqlx::query(
        "UPDATE node_state SET shift_state = 'OPENED_LOCAL_PENDING_DRAIN' WHERE fiscal_number = ?",
    )
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE shifts SET state = 'OPENED_LOCAL_PENDING_DRAIN' WHERE shift_id = ?")
        .bind(shift_id)
        .execute(&pool)
        .await
        .unwrap();

    // Drain: doc#1 (lnd=1) hits Server -12 ERROR_BAD_HASH_PREV (store <64 hex>).
    let msg = "ERROR_BAD_HASH_PREV: store \
               deadbeef0123456789abcdef0123456789abcdef0123456789abcdef01234567"
        .to_string();
    let phase = KpStub::new(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    phase.push_send(Err(DpsError::Server {
        code: -12,
        message: msg.clone(),
    }));
    phase.push_send(Err(DpsError::Server {
        code: -12,
        message: msg,
    }));
    let sc2 = det_signing_ctx();
    let fs2 = fn_sign_blob();
    let view = RuntimeView {
        dps: &phase,
        signing_ctx: &sc2,
        fn_sign: &fs2,
    };
    let _ = backlog_drain::drain(&recon_guard(), &pool, &view, FN).await;

    // ── (RED witness) chain must stay intact — pre-fix mac_recovery re-signs
    //    doc#1 → ChainBreak.
    let v = prro::db::invariant_scan::scan(&pool).await.unwrap();
    assert!(
        v.is_empty(),
        "M2-X1: invariant_scan must be clean after refusal (mac_recovery must NOT re-sign \
         an offline-origin doc): {v:?}"
    );
    // doc#1 NOT re-signed.
    assert_eq!(
        col1(&pool, "unsigned_xml_sha256").await,
        doc1_unsigned_before,
        "doc#1 unsigned_xml_sha256 unchanged (no re-sign)"
    );
    assert_eq!(
        col1(&pool, "previous_hash").await,
        doc1_prev_before,
        "doc#1 previous_hash unchanged (no re-sign)"
    );
    // No re-sign audit; one distinct refusal audit.
    let resigned: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE event_type = 'MAC_RECOVERY_RESIGNED'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        resigned, 0,
        "mac_recovery must NOT re-sign an offline-origin doc"
    );
    let refused: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE event_type = 'MAC_RECOVERY_OFFLINE_ORIGIN_REFUSED'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(refused, 1, "one MAC_RECOVERY_OFFLINE_ORIGIN_REFUSED audit");
    // Shift escalated to manual-recon.
    let shift_state: String = sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
        .bind(shift_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(shift_state, "REQUIRES_MANUAL_RECONCILIATION");
}

/// **M2-N1 FT pin — LITERAL real-inline path (architect ruling a, 2026-06-14).**
/// Three offline SELLs through the REAL `inline::run` path (NOT a hand-seeded
/// chain — the M2-02 class): each lands at OFFLINE_LOCAL_ACK with
/// `previous_hash = unsigned(prior issued doc)` built REALLY by M2-01's
/// seed-advance at offline-ack.  Then go-online (plain `Opened`) + drain with a
/// stub that ACKs doc1 and REJECTs doc2.  Under strict-sequential (M2-N1):
/// doc1 → ACK, doc2 → REJECTED (terminal), doc3 is NEVER sent (its predecessor
/// is non-ACK), the FN escalates to RequiresManualReconciliation via edge 15
/// (plain Opened, NOT a wedge), and `invariant_scan` is clean over the REAL
/// chain incl. the REJECTED offline-origin predecessor (M2-N2b).  This pins the
/// FT against a genuinely-reachable chain, not an aspirational hand-seed.
#[tokio::test]
async fn m2_n1_three_real_offline_sells_strict_drain_halts_on_reject() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_offline(&pool, shift_id).await; // current_shift_id set; shift_state Opened
    seed_open_offline_session(&pool).await;
    seed_offline_code(&pool, 1).await;
    seed_offline_code(&pool, 2).await;
    seed_offline_code(&pool, 3).await;

    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let send_calls = Arc::new(AtomicUsize::new(0));
    let last_calls = Arc::new(AtomicUsize::new(0));

    // ── Phase 1: 3 offline SELLs via the REAL inline path → OFFLINE_LOCAL_ACK.
    //    Distinct idempotency keys (the cross-harness fix); the chain is built
    //    really (no manual prev).  DPS is never touched on the offline branch.
    for (i, idem) in [(1u8, "idem-m2n1-1"), (2, "idem-m2n1-2"), (3, "idem-m2n1-3")] {
        let row = seed_inbox_sell_keyed(&pool, idem).await;
        let stub = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
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
        )
        .await
        .unwrap_or_else(|e| panic!("offline SELL {i} must land OFFLINE_LOCAL_ACK: {e:?}"));
        assert_eq!(
            outcome.document_state,
            DocState::OfflineLocalAck,
            "SELL {i} → OFFLINE_LOCAL_ACK"
        );
    }
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        0,
        "offline issuance must NOT touch the wire"
    );
    // The REAL chain is clean DURING the offline window (M2-01 seed-advance).
    prro::db::invariant_scan::assert_clean(&pool).await;

    // ── Restart + go-online: the drain owns this FN.  Shift stays plain Opened.
    set_node_mode(&pool, NodeMode::GoingOnline).await;

    // ── Phase 2: drain.  doc1 send OK + lastChk ACK → ACK; doc2 send →
    //    DocumentReject (terminal) → strict-sequential HALT + escalate Manual;
    //    doc3 is NEVER sent (no stub response provided for it).
    let phase2 = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    phase2.push_send(Ok(ack("SFN-1", vec![0xDE, 0xAD, 0xBE, 0xEF]))); // doc1 send
    phase2.push_send(Err(DpsError::Authorization {
        code: -1,
        kind: AuthorizationKind::DocumentReject,
        message: "reject_doc2".into(),
    })); // doc2 send
    phase2.push_last(Ok(ack("SFN-1", vec![0xDE, 0xAD, 0xBE, 0xEF]))); // doc1 lastChk Match
    let sign_ctx2 = det_signing_ctx();
    let fn_sign2 = fn_sign_blob();
    let view = RuntimeView {
        dps: &phase2,
        signing_ctx: &sign_ctx2,
        fn_sign: &fn_sign2,
    };
    backlog_drain::drain(&recon_guard(), &pool, &view, FN)
        .await
        .expect("strict drain returns Ok (escalates Manual, not BootError)");

    // ── FT pin (strict-sequential):
    let state_at = |lnd: i64| {
        let pool = &pool;
        async move {
            sqlx::query_scalar::<_, String>(
                "SELECT state FROM fiscal_documents WHERE fiscal_number = ? AND lnd = ?",
            )
            .bind(FN)
            .bind(lnd)
            .fetch_one(pool)
            .await
            .unwrap()
        }
    };
    assert_eq!(state_at(1).await, "ACK", "doc1 → ACK");
    assert_eq!(state_at(2).await, "REJECTED", "doc2 → REJECTED (terminal)");
    assert_eq!(
        state_at(3).await,
        "OFFLINE_LOCAL_ACK",
        "doc3 NOT sent — strict halt at the rejected predecessor"
    );
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        2,
        "only doc1 + doc2 reached the wire; doc3 never did"
    );

    // FN escalated to durable manual-recon via edge 15 (plain Opened, NOT wedged).
    let shift_state: String = sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
        .bind(shift_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(shift_state, "REQUIRES_MANUAL_RECONCILIATION");

    // Ledger clean over the REAL chain incl. the REJECTED offline-origin
    // predecessor (M2-N2b).
    prro::db::invariant_scan::assert_clean(&pool).await;
}

/// **K8 durability pin — idempotent re-tick of a halted strict drain.**
/// Companion to M2-N1 (strict-sequential drain).  A drain that halts +
/// escalates on a terminal reject must be a SAFE NO-OP when re-entered:
/// crash-recovery (boot / the next supervisor tick) re-runs
/// `backlog_drain::drain`, so a crash between the escalate-commit of doc2
/// and the loop's `return` must NOT, on the re-tick, re-send doc3, double-
/// escalate, or fork the ledger.
///
/// Tick 1 reproduces the M2-N1 precondition (doc1 ACK, doc2 REJECTED, doc3
/// held, FN escalated Manual via edge 15).  Tick 2 re-runs drain with an
/// EMPTY stub: if the halt is truly idempotent nothing reaches the wire
/// (the empty queue is never popped); if doc3 were wrongly re-sent the
/// empty `send_chk` queue panics — a loud RED that the re-tick is not
/// crash-safe.
///
/// **STATUS — regression pin AUD-K8-1 (HIGH), architect-confirmed
/// 2026-06-14; green since the manual-reconciliation re-entry guard landed
/// (#168, fix/aud-k8-1).**  Cross-tick companion to M2-N1.  Before the fix this
/// panicked on tick 2 at `KpStub.send_chk: empty queue` because the re-tick
/// re-sent doc3.  Root cause was that `escalate_drain_to_manual` leaves
/// `node_state.mode = GoingOnline` and the offline session `DRAINING` (it
/// mutates only the shift + the `node_state.shift_state` mirror), while
/// `drain()`'s entry gate (`backlog_drain.rs` step 1) checked ONLY
/// `mode == GoingOnline` and
/// `list_drain_candidates_for_fn_ordered_by_lnd` does not filter on shift
/// state.  The REJECTED doc2 had left the drain cohort, so the orphaned
/// successor doc3 became the cohort head and was re-sent.  Reachable in
/// prod via the supervisor `drain_tick` / boot re-drain of the
/// still-`GoingOnline` FN, NOT only crash-recovery — the escalation's
/// documented "durable operator surface, halts FN drain" contract was thus
/// not enforced.  The fix is the step-1b
/// `shift_state == RequiresManualReconciliation` re-entry guard next to the
/// mode-gate (`backlog_drain.rs`); this test is its regression pin.
#[tokio::test]
async fn k8_halted_strict_drain_is_idempotent_under_retick() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_offline(&pool, shift_id).await;
    seed_open_offline_session(&pool).await;
    seed_offline_code(&pool, 1).await;
    seed_offline_code(&pool, 2).await;
    seed_offline_code(&pool, 3).await;

    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let send_calls = Arc::new(AtomicUsize::new(0));
    let last_calls = Arc::new(AtomicUsize::new(0));

    // ── Phase 1: 3 offline SELLs via the REAL inline path → OFFLINE_LOCAL_ACK.
    for (i, idem) in [(1u8, "idem-k8-1"), (2, "idem-k8-2"), (3, "idem-k8-3")] {
        let row = seed_inbox_sell_keyed(&pool, idem).await;
        let stub = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
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
        )
        .await
        .unwrap_or_else(|e| panic!("offline SELL {i} must land OFFLINE_LOCAL_ACK: {e:?}"));
        assert_eq!(
            outcome.document_state,
            DocState::OfflineLocalAck,
            "SELL {i}"
        );
    }
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        0,
        "offline issuance must NOT touch the wire"
    );
    prro::db::invariant_scan::assert_clean(&pool).await;

    set_node_mode(&pool, NodeMode::GoingOnline).await;

    let state_at = |lnd: i64| {
        let pool = &pool;
        async move {
            sqlx::query_scalar::<_, String>(
                "SELECT state FROM fiscal_documents WHERE fiscal_number = ? AND lnd = ?",
            )
            .bind(FN)
            .bind(lnd)
            .fetch_one(pool)
            .await
            .unwrap()
        }
    };
    let shift_state = || {
        let pool = &pool;
        async move {
            sqlx::query_scalar::<_, String>("SELECT state FROM shifts WHERE shift_id = ?")
                .bind(shift_id)
                .fetch_one(pool)
                .await
                .unwrap()
        }
    };
    let halted_count = || {
        let pool = &pool;
        async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM audit_log \
                 WHERE event_type = 'OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL'",
            )
            .fetch_one(pool)
            .await
            .unwrap()
        }
    };

    // ── Tick 1: doc1 send OK + lastChk ACK; doc2 send → DocumentReject →
    //    strict-sequential HALT + escalate Manual; doc3 never sent.
    let tick1 = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    tick1.push_send(Ok(ack("SFN-1", vec![0xDE, 0xAD, 0xBE, 0xEF])));
    tick1.push_send(Err(DpsError::Authorization {
        code: -1,
        kind: AuthorizationKind::DocumentReject,
        message: "reject_doc2".into(),
    }));
    tick1.push_last(Ok(ack("SFN-1", vec![0xDE, 0xAD, 0xBE, 0xEF])));
    let sc1 = det_signing_ctx();
    let fs1 = fn_sign_blob();
    let view1 = RuntimeView {
        dps: &tick1,
        signing_ctx: &sc1,
        fn_sign: &fs1,
    };
    backlog_drain::drain(&recon_guard(), &pool, &view1, FN)
        .await
        .expect("tick 1 strict drain returns Ok (escalates Manual)");

    assert_eq!(state_at(1).await, "ACK", "tick 1: doc1 → ACK");
    assert_eq!(state_at(2).await, "REJECTED", "tick 1: doc2 → REJECTED");
    assert_eq!(state_at(3).await, "OFFLINE_LOCAL_ACK", "tick 1: doc3 held");
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        2,
        "tick 1: only doc1 + doc2 reached the wire"
    );
    assert_eq!(
        shift_state().await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "tick 1: FN escalated Manual (edge 15)"
    );
    assert_eq!(
        halted_count().await,
        1,
        "tick 1: exactly one escalation audit"
    );
    prro::db::invariant_scan::assert_clean(&pool).await;

    // ── Tick 2 (post-crash re-tick): drain AGAIN.  The escalate-commit left
    //    node mode = GoingOnline and the session = DRAINING (it touches only
    //    the shift + node_state.shift_state mirror), exactly the state a crash
    //    between escalate-commit and loop-return would leave.  An EMPTY stub
    //    guarantees any wrongful re-send of doc3 surfaces as a loud panic.
    let tick2 = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    let sc2 = det_signing_ctx();
    let fs2 = fn_sign_blob();
    let view2 = RuntimeView {
        dps: &tick2,
        signing_ctx: &sc2,
        fn_sign: &fs2,
    };
    backlog_drain::drain(&recon_guard(), &pool, &view2, FN)
        .await
        .expect("tick 2 re-tick drain returns Ok (idempotent no-op)");

    assert_eq!(state_at(1).await, "ACK", "re-tick: doc1 stable");
    assert_eq!(state_at(2).await, "REJECTED", "re-tick: doc2 stable");
    assert_eq!(
        state_at(3).await,
        "OFFLINE_LOCAL_ACK",
        "re-tick: doc3 must NOT be sent (strict halt persists)"
    );
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        2,
        "re-tick: no new wire send (still 2 total)"
    );
    assert_eq!(
        shift_state().await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "re-tick: shift stable"
    );
    assert_eq!(
        halted_count().await,
        1,
        "re-tick: NO double escalation (still exactly one audit)"
    );
    prro::db::invariant_scan::assert_clean(&pool).await;
}

/// **K9 durability pin — offline seed survives a restart; chain doesn't fork.**
/// Companion to M2-01 / AUD-L6-1 (offline seed-advance at OfflineLocalAck).
/// The seed-advance MAC commit on offline-ack runs under `synchronous=FULL`,
/// so a crash AFTER doc1's offline-ack but BEFORE doc2 is signed must leave
/// doc2 chaining off doc1's unsigned hash (the SURVIVING seed) — a real,
/// non-forked MAC chain across a process restart.  Models the restart by
/// closing and re-opening the pool on the same DB file.
#[tokio::test]
async fn k9_offline_seed_survives_restart_chain_not_forked() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("k9.db");
    std::mem::forget(dir);
    let pool = open_pool(&path).await.unwrap();
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_offline(&pool, shift_id).await;
    seed_open_offline_session(&pool).await;
    seed_offline_code(&pool, 1).await;
    seed_offline_code(&pool, 2).await;

    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();

    async fn chain_col(pool: &SqlitePool, lnd: i64, col: &str) -> Option<Vec<u8>> {
        let q = format!("SELECT {col} FROM fiscal_documents WHERE fiscal_number = ? AND lnd = ?");
        sqlx::query_scalar(&q)
            .bind(FN)
            .bind(lnd)
            .fetch_one(pool)
            .await
            .unwrap()
    }
    async fn node_seed(pool: &SqlitePool) -> Option<Vec<u8>> {
        sqlx::query_scalar(
            "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number = ?",
        )
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    // ── doc1 through the REAL inline path → OFFLINE_LOCAL_ACK.
    let row1 = seed_inbox_sell_keyed(&pool, "idem-k9-1").await;
    {
        let stub = KpStub::new(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let gate = Arc::new(tokio::sync::Mutex::new(()));
        let guard = gate.lock_owned().await;
        let o = inline::run(
            &pool,
            &pool_secure,
            &stub,
            &sign_ctx,
            &fn_sign,
            &guard,
            &row1,
        )
        .await
        .expect("doc1 → OFFLINE_LOCAL_ACK");
        assert_eq!(o.document_state, DocState::OfflineLocalAck);
    }

    // unsigned(doc1) == h1; the node seed advanced to it (M2-01).
    let h1 = chain_col(&pool, 1, "unsigned_xml_sha256").await;
    assert!(h1.is_some(), "doc1 carries an unsigned hash");
    assert_eq!(
        node_seed(&pool).await,
        h1,
        "seed advanced to unsigned(doc1) (M2-01)"
    );
    prro::db::invariant_scan::assert_clean(&pool).await;

    // ── "Crash + reboot": close and re-open the pool on the SAME DB file.
    //    Nothing is re-derived in memory; the committed seed must survive.
    pool.close().await;
    let pool = open_pool(&path).await.unwrap();
    assert_eq!(
        node_seed(&pool).await,
        h1,
        "seed durable across restart (synchronous=FULL commit)"
    );

    // ── doc2 through inline::run (its inbox row is still NEW).  It must chain
    //    off the SURVIVING seed = unsigned(doc1).
    let row2 = seed_inbox_sell_keyed(&pool, "idem-k9-2").await;
    {
        let stub = KpStub::new(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let gate = Arc::new(tokio::sync::Mutex::new(()));
        let guard = gate.lock_owned().await;
        let o = inline::run(
            &pool,
            &pool_secure,
            &stub,
            &sign_ctx,
            &fn_sign,
            &guard,
            &row2,
        )
        .await
        .expect("doc2 → OFFLINE_LOCAL_ACK");
        assert_eq!(o.document_state, DocState::OfflineLocalAck);
    }

    let prev1 = chain_col(&pool, 1, "previous_hash").await;
    let prev2 = chain_col(&pool, 2, "previous_hash").await;
    let uns2 = chain_col(&pool, 2, "unsigned_xml_sha256").await;
    assert!(prev1.is_none(), "doc1 is genesis (prev NULL)");
    assert_eq!(
        prev2, h1,
        "doc2 chains off the surviving unsigned(doc1) seed"
    );
    assert_ne!(
        prev1, prev2,
        "distinct previous_hash — a real chain, not a pre-M2-01 fork"
    );
    assert_eq!(
        node_seed(&pool).await,
        uns2,
        "seed advanced to unsigned(doc2)"
    );
    prro::db::invariant_scan::assert_clean(&pool).await;
}

// ════════════════════════════════════════════════════════════════════════════
// AUD-L5-1 (offline-drain consumer policy) — a resting KVT1 offline-cohort head
// whose DPS last_chk tip was SUPERSEDED by a newer submitted doc HALTS + escalates
// the FN to RequiresManualReconciliation (ruling B), mirroring SentReplay.
//
// This is the OFFLINE counterpart to the online-tick HOLD test
// (tests/online_convergence_tick.rs::tick_holds_superseded_resting_kvt1_…): the
// SAME SupersededHeld outcome, a DIFFERENT consumer policy — the offline drain
// has a chain-head (strict M2-01 predecessor chain), so a superseded head is
// non-self-resolving → escalate; the online tick has no chain-head → hold.
//
// RED (this commit, EDIT-A/B landed, EDIT-E not): confirm_drain_doc now returns
// SupersededHeld for Kvt1Reentry, but process_via_w12_only still fail-louds
// ("SupersededHeld is SentReplay-exclusive … cannot produce it") → drain returns
// Err.  GREEN (EDIT-E): the arm maps it to DocVerdict::SupersededHeld → the
// drain-loop ruling-B arm escalates Manual.
// ════════════════════════════════════════════════════════════════════════════

async fn read_doc_state_by_lnd(pool: &SqlitePool, lnd: i64) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE fiscal_number = ? AND lnd = ?")
        .bind(FN)
        .bind(lnd)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn doc_id_by_lnd(pool: &SqlitePool, lnd: i64) -> DocumentId {
    sqlx::query_scalar(
        "SELECT document_id FROM fiscal_documents WHERE fiscal_number = ? AND lnd = ?",
    )
    .bind(FN)
    .bind(lnd)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn read_shift_state_by_id(pool: &SqlitePool, shift_id: ShiftId) -> String {
    sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
        .bind(shift_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_node_shift_state(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT shift_state FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn count_audit_event(pool: &SqlitePool, event_type: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type = ?")
        .bind(event_type)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn stamp_server_fiscal_no(pool: &SqlitePool, lnd: i64, sfn: &str) {
    sqlx::query(
        "UPDATE fiscal_documents SET server_fiscal_no = ? WHERE fiscal_number = ? AND lnd = ?",
    )
    .bind(sfn)
    .bind(FN)
    .bind(lnd)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn drain_kvt1_reentry_superseded_escalates_manual() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_offline(&pool, shift_id).await;
    seed_open_offline_session(&pool).await;
    seed_offline_code(&pool, 1).await;
    seed_offline_code(&pool, 2).await;

    let send_calls = Arc::new(AtomicUsize::new(0));
    let last_calls = Arc::new(AtomicUsize::new(0));
    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();

    // ── Phase 1: two offline SELLs via the REAL inline path → OFFLINE_LOCAL_ACK
    //    (lnd 1 + 2, same OPEN session, codes 1 + 2 consumed; no wire).
    for (idem, n) in [("idem-l5-1-doc1", 1u8), ("idem-l5-1-doc2", 2)] {
        let row = seed_inbox_sell_keyed(&pool, idem).await;
        let stub = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
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
        )
        .await
        .unwrap_or_else(|e| panic!("offline SELL {n} must land OFFLINE_LOCAL_ACK: {e:?}"));
        assert_eq!(outcome.document_state, DocState::OfflineLocalAck);
        drop(guard);
    }
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        0,
        "offline branch issues no wire"
    );

    // ── Stage the cohort via the transition whitelist (mirror K5): doc1 (lnd=1)
    //    → KVT1, doc2 (lnd=2) → SENT.  doc2's server_fiscal_no is the DPS tip
    //    that supersedes doc1.
    let doc1_id = doc_id_by_lnd(&pool, 1).await;
    let doc2_id = doc_id_by_lnd(&pool, 2).await;
    let kvt1_bytes = vec![0xDE, 0xAD, 0xBE, 0xEF];
    with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let o1 = fiscal_documents::transition_state(
                tx,
                doc1_id,
                DocState::OfflineLocalAck,
                DocState::Sent,
            )
            .await
            .map_err(anyhow::Error::from)?;
            assert!(matches!(o1, TransitionOutcome::Applied), "doc1 OLA→Sent");
            let o2 =
                fiscal_documents::transition_state(tx, doc1_id, DocState::Sent, DocState::Kvt1)
                    .await
                    .map_err(anyhow::Error::from)?;
            assert!(matches!(o2, TransitionOutcome::Applied), "doc1 Sent→Kvt1");
            document_files::replace_tx(tx, doc1_id, DocumentFileKind::Kvt1Raw, &kvt1_bytes).await?;
            let o3 = fiscal_documents::transition_state(
                tx,
                doc2_id,
                DocState::OfflineLocalAck,
                DocState::Sent,
            )
            .await
            .map_err(anyhow::Error::from)?;
            assert!(matches!(o3, TransitionOutcome::Applied), "doc2 OLA→Sent");
            Ok::<(), anyhow::Error>(())
        })
    })
    .await
    .expect("stage doc1→KVT1 + doc2→SENT");
    stamp_server_fiscal_no(&pool, 1, SERVER_FISCAL_NO).await; // DPS-FN-ONLINE-1
    stamp_server_fiscal_no(&pool, 2, "DPS-FN-ONLINE-2").await;
    assert_eq!(read_doc_state_by_lnd(&pool, 1).await, "KVT1");
    assert_eq!(read_doc_state_by_lnd(&pool, 2).await, "SENT");

    // ── Go-online: the drain owns reconciliation (session stays OPEN).
    set_node_mode(&pool, NodeMode::GoingOnline).await;

    // ── Phase 2: drain.  Head = doc1 (KVT1) → Kvt1Reentry confirm → lastChk
    //    reports a DIFFERENT tip (doc2's sfn) → ServerFiscalIdMismatch.  doc2
    //    (lnd=2, SENT, sfn=DPS-FN-ONLINE-2) is the supersession candidate
    //    `submitted_above_lnd` returns.  Exactly one last_chk (drain halts at
    //    the head — doc2 is never processed).
    let phase2 = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    phase2.push_last(Err(DpsError::ServerFiscalIdMismatch {
        expected_id: SERVER_FISCAL_NO.to_string(),
        actual_id: "DPS-FN-ONLINE-2".to_string(),
    }));
    let sign_ctx2 = det_signing_ctx();
    let fn_sign2 = fn_sign_blob();
    let view = RuntimeView {
        dps: &phase2,
        signing_ctx: &sign_ctx2,
        fn_sign: &fn_sign2,
    };

    backlog_drain::drain(&recon_guard(), &pool, &view, FN)
        .await
        .expect("drain escalates Manual (ruling B) and returns Ok — not a fail-loud Err");

    // ── GREEN assertions: ruling-B escalate over the SupersededHeld outcome.
    assert_eq!(
        read_doc_state_by_lnd(&pool, 1).await,
        "KVT1",
        "doc1 held at KVT1 (no CAS)"
    );
    assert_eq!(
        read_doc_state_by_lnd(&pool, 2).await,
        "SENT",
        "doc2 not processed (halt at the cohort head)"
    );
    assert_eq!(
        read_shift_state_by_id(&pool, shift_id).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "FN shift escalated to Manual (ruling B)"
    );
    assert_eq!(
        read_node_shift_state(&pool).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "node_state.shift_state mirror"
    );
    assert_eq!(
        count_audit_event(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await,
        1,
        "exactly one escalation audit (Critical)"
    );
    assert_eq!(
        count_audit_event(&pool, "TIP_SUPERSEDED").await,
        1,
        "the Kvt1Reentry superseded-light audit (Warning)"
    );
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        0,
        "Kvt1Reentry issues no send"
    );
    prro::db::invariant_scan::assert_clean(&pool).await;
}

// ════════════════════════════════════════════════════════════════════════════
// AUD-L2-1b (boot-KVT2 consumer) — a stray online-origin KVT2 doc whose boot
// finalize hits a ChainSeedMismatch (tampered chain seed) ESCALATES the FN to
// Manual, instead of the Warning-only BOOT_KVT2_DISPATCH_FAILED (doc wedged at
// KVT2, shift untouched, no operator surface).
//
// On main this FAILS: the boot-KVT2 Err(e) arm stringifies EVERY stage_finalize
// error into one BOOT_KVT2_DISPATCH_FAILED (Severity::Warning) — a
// ChainSeedMismatch is indistinguishable and gets no escalation; the shift stays
// OPENED.  GREEN matches the typed StageFinalizeError::ChainSeedMismatch and
// escalates via escalate_fn_to_manual_recon
// (BOOT_KVT2_CHAIN_SEED_MISMATCH_ESCALATE_MANUAL).
//
// NB: no assert_clean — the tampered seed is a deliberate chain breach (exactly
// the escalation condition); invariant_scan rightly flags it.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn boot_kvt2_chain_seed_mismatch_escalates_manual() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_online(&pool, shift_id).await;
    let row = seed_inbox_sell(&pool).await;

    // ── Build an online-origin KVT2 doc (offline_fiscal_no NULL → stage_finalize's
    //    online-origin seed guard applies): inline::run Hold → SENT, then manual
    //    Sent→Kvt1→Kvt2 + Kvt1Raw (mirror K5's crash-point construction).
    let send_calls = Arc::new(AtomicUsize::new(0));
    let last_calls = Arc::new(AtomicUsize::new(0));
    let phase1 = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    phase1.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    phase1.push_last(Ok(ack(SERVER_FISCAL_NO, vec![]))); // empty → Hold → SENT
    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;
    inline::run(
        &pool,
        &pool_secure,
        &phase1,
        &sign_ctx,
        &fn_sign,
        &guard,
        &row,
    )
    .await
    .expect("online SELL Hold rests at SENT");
    drop(guard);

    let doc_id = read_doc_id(&pool).await;
    let kvt1_bytes = vec![0xDE, 0xAD, 0xBE, 0xEF];
    with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let o1 = fiscal_documents::transition_state(tx, doc_id, DocState::Sent, DocState::Kvt1)
                .await
                .map_err(anyhow::Error::from)?;
            assert!(matches!(o1, TransitionOutcome::Applied), "Sent→Kvt1");
            let o2 = fiscal_documents::transition_state(tx, doc_id, DocState::Kvt1, DocState::Kvt2)
                .await
                .map_err(anyhow::Error::from)?;
            assert!(matches!(o2, TransitionOutcome::Applied), "Kvt1→Kvt2");
            document_files::replace_tx(tx, doc_id, DocumentFileKind::Kvt1Raw, &kvt1_bytes).await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await
    .expect("manual Sent→Kvt1→Kvt2 + Kvt1Raw");
    assert_eq!(read_doc_state(&pool, FN).await, "KVT2");

    // Tamper the chain seed → the boot-KVT2 stage_finalize online-origin guard
    // breaks (ns.last_known_unsigned_xml_sha256 != doc.previous_hash).
    prro::db::repositories::node_state::seed_prevhash(&pool, FN, &[0xFF; 32])
        .await
        .expect("seed tamper");

    // ── Boot recovery — the Kvt2 dispatch arm runs stage_finalize → ChainSeedMismatch.
    boot_phase::run_boot_reconciliation(&recon_guard(), &pool, FN, None)
        .await
        .expect("boot returns Ok (escalates the FN, does not abort the boot)");

    // ── GREEN: the chain breach ESCALATES the FN to Manual with a Critical audit.
    assert_eq!(
        read_doc_state(&pool, FN).await,
        "KVT2",
        "doc stays KVT2 (finalize rolled back)"
    );
    assert_eq!(
        read_shift_state_by_id(&pool, shift_id).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "boot-KVT2 ChainSeedMismatch escalates the FN shift to Manual"
    );
    assert_eq!(
        read_node_shift_state(&pool).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "node_state.shift_state mirror"
    );
    assert_eq!(
        count_audit_event(&pool, "BOOT_KVT2_CHAIN_SEED_MISMATCH_ESCALATE_MANUAL").await,
        1,
        "exactly one Critical boot-KVT2 escalation audit"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// AUD-L2-1a — RED pin + A2.4 ACTIVATION PREREQUISITE (NOT fixed here).
//
// **STATUS — RED pin, #[ignore]'d; fix deferred to A2.4.**  The online inline
// lane has a chain SEED-FORK: the chain seed
// (`node_state.last_known_unsigned_xml_sha256`) is read as a doc's
// `previous_hash` at sign time (stage_sign) but ADVANCED only inside
// `stage_finalize` at the ACK transition (gated online-origin,
// `offline_fiscal_no.is_none()`).  Two online SELLs can BOTH rest at `SENT`
// (empty-data_sign lastChk Hold), so `stage_finalize` never runs for doc1 → the
// seed is never advanced → doc2 is signed against the SAME stale (pre-doc1)
// seed.  Result: `doc2.previous_hash == doc1.previous_hash` (both = the pre-doc1
// genesis seed), NOT `doc1.unsigned_xml_sha256` — a FORK, not a chain.  (The
// OFFLINE lane is correct — M2-01 advances the seed per-doc at offline-ack;
// cf. the M2-01 chain test which asserts `prev2 == uns1`.)
//
// This test asserts the DESIRED (fixed) chained property and therefore FAILS
// today.  The fix (move the seed advance to `stage_send`, or generalise the
// finalize-gate / handle rejected-after-SENT) is deferred to A2.4.  The inline
// lane is DORMANT in prod (`UnimplementedWritePath` is bound, NOT
// `InlineWritePath`), so the fork is NOT reachable from a live request yet —
// un-#[ignore]-ing this test is the GATE for flipping the prod binding (see the
// A2.4 ACTIVATION PREREQUISITE barrier at `runtime::supervisor` + `inline.rs`).
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "RED pin AUD-L2-1a: online-lane seed-fork; fix is an A2.4 prerequisite (do NOT activate InlineWritePath until resolved)"]
async fn m1_02_online_seed_fork_a24_prerequisite() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_online(&pool, shift_id).await;
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();

    // Two online SELLs on the SAME FN, both resting at SENT via empty-data_sign
    // Hold (mirror m1_02_reachability_second_sell_while_first_rests_sent).
    let row1 = seed_inbox_sell(&pool).await;
    let stub1 = KpStub::new(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    stub1.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    stub1.push_last(Ok(ack(SERVER_FISCAL_NO, vec![]))); // empty → Hold → SENT
    {
        let guard = gate.clone().lock_owned().await;
        inline::run(
            &pool,
            &pool_secure,
            &stub1,
            &sign_ctx,
            &fn_sign,
            &guard,
            &row1,
        )
        .await
        .expect("doc1 rests at SENT");
    }
    let row2 = seed_inbox_sell_keyed(&pool, "idem-l2-1a-2").await;
    let stub2 = KpStub::new(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    stub2.push_send(Ok(ack("DPS-FN-ONLINE-2", vec![])));
    stub2.push_last(Ok(ack("DPS-FN-ONLINE-2", vec![]))); // empty → Hold → SENT
    {
        let guard = gate.clone().lock_owned().await;
        inline::run(
            &pool,
            &pool_secure,
            &stub2,
            &sign_ctx,
            &fn_sign,
            &guard,
            &row2,
        )
        .await
        .expect("doc2 rests at SENT");
    }

    let doc1_unsigned: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT unsigned_xml_sha256 FROM fiscal_documents WHERE fiscal_number = ? AND lnd = 1",
    )
    .bind(FN)
    .fetch_one(&pool)
    .await
    .unwrap();
    let doc2_previous_hash: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT previous_hash FROM fiscal_documents WHERE fiscal_number = ? AND lnd = 2",
    )
    .bind(FN)
    .fetch_one(&pool)
    .await
    .unwrap();

    // DESIRED (A2.4 fixed): doc2 chains off doc1's unsigned hash.  FAILS today —
    // the seed never advanced (doc1 rests at SENT), so doc2.previous_hash is the
    // pre-doc1 genesis seed, not doc1.unsigned_xml_sha256.
    assert!(
        doc1_unsigned.is_some(),
        "doc1 must carry unsigned_xml_sha256"
    );
    assert_eq!(
        doc2_previous_hash, doc1_unsigned,
        "AUD-L2-1a: doc2.previous_hash must chain off doc1.unsigned_xml_sha256 \
         (online-lane seed-fork — fix is an A2.4 prerequisite)"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// AUD-L2-1b (boot-KVT2 escalation robustness — review MEDIUM follow-up).  A
// boot-KVT2 ChainSeedMismatch on an FN whose shift is CLOSED but whose
// node_state.current_shift_id still DANGLES (the locked post-close status quo —
// terminal_close_pins_current_shift_id_behavior: Closing→Closed leaves the
// pointer set) MUST NOT abort the boot.
//
// (Closed → RMR) is NOT a whitelisted shift edge, so a guard that checks ONLY
// current_shift_id.is_some() would call apply_shift_transition(Closed → RMR) →
// Forbidden → non-Applied → BootError::Internal, which `?`-propagates and ABORTS
// the whole-FN boot doc-loop (violating the W9.4 per-doc failure-containment
// invariant at boot_phase.rs).  The escalation must route a NON-escalatable
// from-state (NULL current_shift_id OR a non-whitelisted state like
// Closed/Created/Error) to the no-shift operator audit WITHOUT a CAS.
//
// On the pre-fix branch this FAILS (boot returns Err → .expect panics).
// NB: no assert_clean — the tampered seed is a deliberate chain breach.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn boot_kvt2_chain_seed_mismatch_closed_shift_no_abort() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_online(&pool, shift_id).await;
    let row = seed_inbox_sell(&pool).await;

    // Online-origin KVT2 doc (mirror K5).
    let send_calls = Arc::new(AtomicUsize::new(0));
    let last_calls = Arc::new(AtomicUsize::new(0));
    let phase1 = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    phase1.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    phase1.push_last(Ok(ack(SERVER_FISCAL_NO, vec![])));
    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let guard = Arc::new(tokio::sync::Mutex::new(())).lock_owned().await;
    inline::run(
        &pool,
        &pool_secure,
        &phase1,
        &sign_ctx,
        &fn_sign,
        &guard,
        &row,
    )
    .await
    .expect("online SELL Hold rests at SENT");
    drop(guard);
    let doc_id = read_doc_id(&pool).await;
    let kvt1_bytes = vec![0xDE, 0xAD, 0xBE, 0xEF];
    with_immediate(&pool, move |tx| {
        Box::pin(async move {
            fiscal_documents::transition_state(tx, doc_id, DocState::Sent, DocState::Kvt1)
                .await
                .map_err(anyhow::Error::from)?;
            fiscal_documents::transition_state(tx, doc_id, DocState::Kvt1, DocState::Kvt2)
                .await
                .map_err(anyhow::Error::from)?;
            document_files::replace_tx(tx, doc_id, DocumentFileKind::Kvt1Raw, &kvt1_bytes).await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await
    .expect("manual Sent→Kvt1→Kvt2 + Kvt1Raw");

    // Pin the dangling-pointer status quo: shift CLOSED, current_shift_id still set.
    sqlx::query("UPDATE shifts SET state = 'CLOSED' WHERE shift_id = ?")
        .bind(shift_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE node_state SET shift_state = 'CLOSED' WHERE fiscal_number = ?")
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();

    prro::db::repositories::node_state::seed_prevhash(&pool, FN, &[0xFF; 32])
        .await
        .expect("seed tamper");

    // Boot MUST NOT abort — a non-escalatable (Closed) shift routes to the
    // no-shift operator surface, not a Forbidden CAS → Internal → boot abort.
    boot_phase::run_boot_reconciliation(&recon_guard(), &pool, FN, None)
        .await
        .expect("boot must NOT abort on a closed-shift ChainSeedMismatch (W9.4 isolation)");

    assert_eq!(
        read_doc_state(&pool, FN).await,
        "KVT2",
        "doc stays KVT2 (no CAS)"
    );
    assert_eq!(
        read_shift_state_by_id(&pool, shift_id).await,
        "CLOSED",
        "no illegal Closed→RMR CAS"
    );
    assert_eq!(
        count_audit_event(&pool, "BOOT_KVT2_CHAIN_SEED_MISMATCH_NO_SHIFT").await,
        1,
        "non-escalatable shift → one no-shift operator audit (no CAS, no boot abort)"
    );
    assert_eq!(
        count_audit_event(&pool, "BOOT_KVT2_CHAIN_SEED_MISMATCH_ESCALATE_MANUAL").await,
        0,
        "no escalation audit — the shift is not RMR-escalatable"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// SW-1 (M1-M2 sweep) — boot must NOT re-drive a RMR-but-Online FN.
//
// AUD-K8-1's RMR re-entry guard lives only in the drain (backlog_drain.rs).
// run_boot_reconciliation has no equivalent, so an FN escalated to
// shift_state==RMR while mode stays ONLINE (exactly the state Batch C's
// convergence / boot-KVT2 escalate_fn_to_manual_recon creates — it CAS's the
// shift to RMR but leaves mode) falls past the mode-gates (branch (d) only
// catches GoingOnline; branch (f) only Blocked/StopMode/CryptoDegraded) into the
// per-doc dispatch → a halted FN's resting doc is RE-DRIVEN (real DPS wire).
//
// On main this FAILS: the SENT doc is probed (last_chk fired) + advanced.
// GREEN: a RMR short-circuit (mirror of AUD-K8-1) skips the FN — 0 wire, doc
// untouched.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn boot_skips_rmr_but_online_fn_no_redrive() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state_online(&pool, shift_id).await; // mode=Online, shift_state=Opened
    let row = seed_inbox_sell(&pool).await;

    let send_calls = Arc::new(AtomicUsize::new(0));
    let last_calls = Arc::new(AtomicUsize::new(0));

    // Build a resting SENT doc (inline::run lastChk-Hold).
    let phase1 = KpStub::new(Arc::clone(&send_calls), Arc::clone(&last_calls));
    phase1.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    phase1.push_last(Ok(ack(SERVER_FISCAL_NO, vec![]))); // empty → Hold → SENT
    let sign_ctx = det_signing_ctx();
    let fn_sign = fn_sign_blob();
    let guard = Arc::new(tokio::sync::Mutex::new(())).lock_owned().await;
    inline::run(
        &pool,
        &pool_secure,
        &phase1,
        &sign_ctx,
        &fn_sign,
        &guard,
        &row,
    )
    .await
    .expect("online SELL Hold rests at SENT");
    drop(guard);
    assert_eq!(read_doc_state(&pool, FN).await, "SENT");

    // The Batch-C escalation state: shift CAS'd to RMR, mode LEFT at Online.
    sqlx::query("UPDATE shifts SET state = 'REQUIRES_MANUAL_RECONCILIATION' WHERE shift_id = ?")
        .bind(shift_id)
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

    // Boot WITH deps + a stub that WOULD answer the SENT probe (Match → advance).
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
        .expect("boot reconciliation returns Ok (RMR FN skipped, not re-driven)");

    // GREEN: a halted (RMR) FN is NOT re-driven by boot — doc untouched, no
    // ADDITIONAL wire beyond construction.
    assert_eq!(
        read_doc_state(&pool, FN).await,
        "SENT",
        "RMR FN's resting doc must NOT be re-driven by boot"
    );
    assert_eq!(
        last_calls.load(Ordering::SeqCst),
        1,
        "only the construction Hold lastChk — boot does NOT re-probe a halted (RMR) FN"
    );
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        1,
        "only the construction send — boot does NOT resend a halted FN"
    );
    prro::db::invariant_scan::assert_clean(&pool).await;
}
