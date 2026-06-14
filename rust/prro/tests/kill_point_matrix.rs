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
        let outcome = inline::run(&pool, &pool_secure, &stub, &sign_ctx, &fn_sign, &guard, &row)
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
    let shift_state: String =
        sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
            .bind(shift_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(shift_state, "REQUIRES_MANUAL_RECONCILIATION");

    // Ledger clean over the REAL chain incl. the REJECTED offline-origin
    // predecessor (M2-N2b).
    prro::db::invariant_scan::assert_clean(&pool).await;
}
