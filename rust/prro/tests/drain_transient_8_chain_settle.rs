//! B10 — offline backlog drain transient `-8` (DPS chain-settle latency).
//!
//! Root cause (live-verified, dossier `docs/B10_DRAIN_TRANSIENT_8_DOSSIER.md`):
//! the offline backlog drain intermittently draws DPS wire reject `-8`
//! (`ERROR_XML_DATE`) on a doc that chains off a just-accepted predecessor.
//! This is a TRANSIENT chain-settle latency (DPS `lastChk` read-model surfaces
//! a tip BEFORE its write-model will accept that tip as a valid `previous_hash`
//! for the next doc), NOT a format error.  The byte-identical form ACCEPTED
//! after a wait; only TIME changed.
//!
//! The bug: routing lumped `-8` into the terminal `-5 | -7 | -8 | -9 | -10`
//! arm → an offline-origin `-8` on drain became `DocVerdict::Failed {
//! manual_recon: true }` → the shift wedged into RequiresManualReconciliation
//! on a mere DPS settle latency.
//!
//! The fix (Seam A1, forward-only): an OFFLINE-ORIGIN `-8` routes to a
//! retryable class (`DrainChainSettleRetry`) so the doc goes
//! `Sending → ErrorRetryable` (a normal forward edge), is re-driven under a
//! bounded budget, and escalates to RMR ONLY on budget exhaustion.

mod common;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;

use prro::db::models::enums::{NodeMode, OfflineSessionState, ShiftState};
use prro::db::models::ids::{DocumentId, OfflineSessionId, ShiftId};
use prro::services::offline_sync::backlog_drain;
use prro::services::reconciliation::runtime::RuntimeView;
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{CheckAck, CheckEnvelope, CheckSignBlob, RroInfo, StatusSnapshot};
use prro::transports::dps::error::DpsError;
use sqlx::SqlitePool;
use uuid::Uuid;

use common::det_signing_ctx;

const FN: &str = "1234567890";
const CASHIER_OK: &str = "test-cashier";

// ─── Dual-queue DPS stub (send_chk + last_chk) ───────────────────────

struct DualQueueStub {
    send_chk_q: Mutex<VecDeque<Result<CheckAck, DpsError>>>,
    last_chk_q: Mutex<VecDeque<Result<CheckAck, DpsError>>>,
    send_chk_calls: AtomicUsize,
    last_chk_calls: AtomicUsize,
}

impl DualQueueStub {
    fn new(
        send_chk: Vec<Result<CheckAck, DpsError>>,
        last_chk: Vec<Result<CheckAck, DpsError>>,
    ) -> Self {
        Self {
            send_chk_q: Mutex::new(send_chk.into()),
            last_chk_q: Mutex::new(last_chk.into()),
            send_chk_calls: AtomicUsize::new(0),
            last_chk_calls: AtomicUsize::new(0),
        }
    }

    fn send_chk_count(&self) -> usize {
        self.send_chk_calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl DpsChannel for DualQueueStub {
    async fn send_chk(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        self.send_chk_calls.fetch_add(1, Ordering::SeqCst);
        self.send_chk_q
            .lock()
            .unwrap()
            .pop_front()
            .expect("DualQueueStub.send_chk: empty queue")
    }

    async fn last_chk(&self, _: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        self.last_chk_calls.fetch_add(1, Ordering::SeqCst);
        self.last_chk_q
            .lock()
            .unwrap()
            .pop_front()
            .expect("DualQueueStub.last_chk: empty queue")
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

    async fn ask_offline_codes(
        &self,
        _: prro::transports::dps::dto::CheckEnvelope,
    ) -> Result<
        prro::transports::dps::dto::OfflineCodesResponse,
        prro::transports::dps::error::DpsError,
    > {
        unreachable!("stub: ask_offline_codes not exercised");
    }
}

fn fn_sign() -> CheckSignBlob {
    CheckSignBlob(vec![0xAB, 0xCD])
}

struct DepsCarriers {
    dps: std::sync::Arc<DualQueueStub>,
    signing_ctx: SigningContext,
    fn_sign: CheckSignBlob,
}

fn carriers(
    send_chk: Vec<Result<CheckAck, DpsError>>,
    last_chk: Vec<Result<CheckAck, DpsError>>,
) -> DepsCarriers {
    DepsCarriers {
        dps: std::sync::Arc::new(DualQueueStub::new(send_chk, last_chk)),
        signing_ctx: det_signing_ctx(),
        fn_sign: fn_sign(),
    }
}

fn view_for(c: &DepsCarriers) -> RuntimeView<'_> {
    RuntimeView {
        dps: c.dps.as_ref(),
        signing_ctx: &c.signing_ctx,
        fn_sign: &c.fn_sign,
    }
}

fn server_reject(code: i32, message: &str) -> Result<CheckAck, DpsError> {
    Err(DpsError::Server {
        code,
        message: message.into(),
    })
}

// ─── Fixtures ────────────────────────────────────────────────────────

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("drain_transient_8.db"))
        .await
        .expect("open_pool runs migrations");
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    (dir, pool)
}

async fn seed_node_state(pool: &SqlitePool, mode: NodeMode, shift: ShiftState) {
    sqlx::query(
        "INSERT INTO node_state(fiscal_number, mode, shift_state, next_lnd) \
         VALUES (?, ?, ?, 100)",
    )
    .bind(FN)
    .bind(mode)
    .bind(shift)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a shift in `OPENED_LOCAL_PENDING_DRAIN` (the pending-drain state the
/// backlog cohort operates on) + wire `node_state.current_shift_id` so
/// `escalate_drain_to_manual` can find the shift.
async fn seed_pending_drain_shift(pool: &SqlitePool) -> ShiftId {
    let shift_id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts(shift_id, fiscal_number, serial, state, \
            open_mode, cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED_LOCAL_PENDING_DRAIN', 'OFFLINE', 0, ?)",
    )
    .bind(shift_id)
    .bind(FN)
    .bind(CASHIER_OK)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE node_state SET current_shift_id = ? WHERE fiscal_number = ?")
        .bind(shift_id)
        .bind(FN)
        .execute(pool)
        .await
        .unwrap();
    shift_id
}

async fn seed_offline_session(pool: &SqlitePool, state: OfflineSessionState) -> OfflineSessionId {
    let session_id = OfflineSessionId::new();
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, ?, '2026-05-21T00:00:00Z')",
    )
    .bind(session_id)
    .bind(FN)
    .bind(state.as_str())
    .execute(pool)
    .await
    .unwrap();
    session_id
}

/// Seed a fully-formed OFFLINE_LOCAL_ACK doc (offline_fiscal_no +
/// offline_dps_code + persisted SIGNED_XML + consumed offline code) ready
/// for the drain to pick up and wire-send.
#[allow(clippy::too_many_arguments)]
async fn seed_offline_local_ack_doc(
    pool: &SqlitePool,
    lnd: i64,
    code_lnd: i64,
    session_id: OfflineSessionId,
    shift_id: ShiftId,
) -> DocumentId {
    let doc_id = DocumentId::new();
    let req_id = Uuid::now_v7();
    let sha = vec![0u8; 32];
    let dps_code = format!("DRAIN-8-{code_lnd}");
    sqlx::query(
        "INSERT INTO fiscal_documents( \
            document_id, request_id, fiscal_number, shift_id, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, signed_by_cashier_id, \
            offline_session_id, offline_fiscal_no, offline_fiscal_date, \
            offline_dps_code, server_fiscal_no \
         ) VALUES ( \
            ?, ?, ?, ?, ?, 'SELL', 'OFFLINE_LOCAL_ACK', \
            'b', 't', 'OFFLINE', '2026-05-21T00:00:00Z', \
            '{}', ?, ?, \
            ?, ?, '2026-05-21T00:00:00Z', \
            ?, NULL \
         )",
    )
    .bind(doc_id)
    .bind(req_id.as_bytes().to_vec())
    .bind(FN)
    .bind(shift_id)
    .bind(lnd)
    .bind(&sha)
    .bind(CASHIER_OK)
    .bind(session_id)
    .bind(code_lnd)
    .bind(&dps_code)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO document_files(document_id, kind, content) \
         VALUES (?, 'SIGNED_XML', ?)",
    )
    .bind(doc_id)
    .bind(b"FAKE-CMS".to_vec())
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO offline_codes(fiscal_number, code_lnd, consumed_at, consumed_by_document_id) \
         VALUES (?, ?, '2026-05-21T00:00:01Z', ?)",
    )
    .bind(FN)
    .bind(code_lnd)
    .bind(doc_id)
    .execute(pool)
    .await
    .unwrap();

    doc_id
}

/// Seed an ERROR_RETRYABLE doc + N complete SEND transport_trace rows
/// carrying `retry_class`, so `attempts_used` = N on the next drain tick.
async fn seed_error_retryable_doc_with_trace(
    pool: &SqlitePool,
    session_id: OfflineSessionId,
    shift_id: ShiftId,
    lnd: i64,
    code_lnd: i64,
    attempts: i32,
    retry_class: &str,
) -> DocumentId {
    let doc = seed_offline_local_ack_doc(pool, lnd, code_lnd, session_id, shift_id).await;
    // Flip to ERROR_RETRYABLE (the ER cohort entry state for a re-driven doc).
    sqlx::query("UPDATE fiscal_documents SET state = 'ERROR_RETRYABLE' WHERE document_id = ?")
        .bind(doc)
        .execute(pool)
        .await
        .unwrap();
    let sha = vec![0x42u8; 32];
    for n in 1..=attempts {
        sqlx::query(
            "INSERT INTO transport_trace( \
                document_id, attempt_no, started_at, \
                backend_profile_id, transport_profile_id, request_envelope_sha256, \
                completed_at, wire_call_started_at, wire_call_finished_at, \
                outcome_kind, server_status_code, error_kind, error_message, retry_class \
             ) VALUES ( \
                ?, ?, '2026-05-22T00:00:00Z', 'b', 't', ?, \
                '2026-05-22T00:00:02Z', '2026-05-22T00:00:01Z', '2026-05-22T00:00:02Z', \
                'RETRYABLE_SERVER', -8, 'Server', 'ERROR_XML_DATE', ? \
             )",
        )
        .bind(doc)
        .bind(n)
        .bind(&sha)
        .bind(retry_class)
        .execute(pool)
        .await
        .unwrap();
    }
    doc
}

async fn read_doc_state(pool: &SqlitePool, doc_id: DocumentId) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(doc_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_shift_state(pool: &SqlitePool, shift_id: ShiftId) -> String {
    sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
        .bind(shift_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn consumed_code_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ? AND consumed_at IS NOT NULL",
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn doc_offline_fiscal_no(pool: &SqlitePool, doc_id: DocumentId) -> Option<i64> {
    sqlx::query_scalar("SELECT offline_fiscal_no FROM fiscal_documents WHERE document_id = ?")
        .bind(doc_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn doc_server_fiscal_no(pool: &SqlitePool, doc_id: DocumentId) -> Option<String> {
    sqlx::query_scalar("SELECT server_fiscal_no FROM fiscal_documents WHERE document_id = ?")
        .bind(doc_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn node_seed_sha(pool: &SqlitePool) -> Option<Vec<u8>> {
    sqlx::query_scalar(
        "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number = ?",
    )
    .bind(FN)
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

// ─── Pin #1: offline-origin drain `-8` → ErrorRetryable, NOT RMR ─────

/// PIN #1 (core RED): an offline-origin `-8` on drain must NOT wedge the
/// shift into RequiresManualReconciliation.  The doc goes
/// `OFFLINE_LOCAL_ACK → Sending → ErrorRetryable` and stays in the drain
/// cohort; the shift stays `OPENED_LOCAL_PENDING_DRAIN`.  (Today this
/// escalates to Manual — that is the bug.)
#[tokio::test]
async fn pin1_offline_drain_minus_8_does_not_escalate_to_manual() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(
        &pool,
        NodeMode::GoingOnline,
        ShiftState::OpenedLocalPendingDrain,
    )
    .await;
    let shift_id = seed_pending_drain_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let doc = seed_offline_local_ack_doc(&pool, 1, 100, session_id, shift_id).await;

    // DPS returns transient -8 on the drain send.
    let c = carriers(vec![server_reject(-8, "ERROR_XML_DATE")], vec![]);
    let view = view_for(&c);
    backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    // Forward-only: doc rests in ERROR_RETRYABLE (re-drivable), NOT Rejected.
    assert_eq!(
        read_doc_state(&pool, doc).await,
        "ERROR_RETRYABLE",
        "offline -8 must go Sending->ErrorRetryable (transient chain-settle), never Rejected"
    );
    // The shift must NOT be wedged into Manual on a mere settle latency.
    assert_eq!(
        read_shift_state(&pool, shift_id).await,
        "OPENED_LOCAL_PENDING_DRAIN",
        "a transient -8 must NOT escalate the shift to RequiresManualReconciliation"
    );
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await,
        0,
        "no manual-recon escalation on a transient -8"
    );
    // The reject reached the wire once.
    assert_eq!(c.dps.send_chk_count(), 1);
}

// ─── Pin #7 (D2): offline `-8` never advances seed / stamps sfn ─────

/// PIN #7: a rejected offline `-8` doc must NOT advance the online chain
/// seed and must NOT stamp `server_fiscal_no` (advance-at-SEND is the
/// `WireDecision::Sent` arm ONLY; a reject can never issue).  offline-origin
/// docs skip the seed advance entirely.
#[tokio::test]
async fn pin7_offline_drain_minus_8_does_not_advance_seed_or_stamp_sfn() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(
        &pool,
        NodeMode::GoingOnline,
        ShiftState::OpenedLocalPendingDrain,
    )
    .await;
    let shift_id = seed_pending_drain_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let doc = seed_offline_local_ack_doc(&pool, 1, 100, session_id, shift_id).await;

    let seed_before = node_seed_sha(&pool).await;

    let c = carriers(vec![server_reject(-8, "ERROR_XML_DATE")], vec![]);
    let view = view_for(&c);
    backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(
        node_seed_sha(&pool).await,
        seed_before,
        "D2: offline -8 reject must NOT advance node_state chain seed"
    );
    assert_eq!(
        doc_server_fiscal_no(&pool, doc).await,
        None,
        "D2: offline -8 reject must NOT stamp server_fiscal_no (never issued online)"
    );
    // offline_fiscal_no (the offline issuance identity) is untouched.
    assert_eq!(doc_offline_fiscal_no(&pool, doc).await, Some(100));
}

// ─── Pin #4: idempotent re-drive re-sends persisted bytes ───────────

/// PIN #4 (idempotency, INV-4): a `-8` + re-drive next tick MUST re-send the
/// PERSISTED signed bytes — no re-sign — so NO new offline code / lnd is
/// drawn.  A re-sign would draw a fresh offline number → double-issue.
#[tokio::test]
async fn pin4_redrive_after_minus_8_consumes_no_new_offline_code() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(
        &pool,
        NodeMode::GoingOnline,
        ShiftState::OpenedLocalPendingDrain,
    )
    .await;
    let shift_id = seed_pending_drain_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let doc = seed_offline_local_ack_doc(&pool, 1, 100, session_id, shift_id).await;

    let codes_before = consumed_code_count(&pool).await;
    let offline_no_before = doc_offline_fiscal_no(&pool, doc).await;

    // Tick 1: -8 → ErrorRetryable.
    let c1 = carriers(vec![server_reject(-8, "ERROR_XML_DATE")], vec![]);
    backlog_drain::drain(&common::drain_test_guard(), &pool, &view_for(&c1), FN)
        .await
        .unwrap();
    assert_eq!(read_doc_state(&pool, doc).await, "ERROR_RETRYABLE");

    // Tick 2: -8 again → re-drive re-sends stored bytes; still ErrorRetryable.
    let c2 = carriers(vec![server_reject(-8, "ERROR_XML_DATE")], vec![]);
    backlog_drain::drain(&common::drain_test_guard(), &pool, &view_for(&c2), FN)
        .await
        .unwrap();
    assert_eq!(
        c2.dps.send_chk_count(),
        1,
        "tick 2 re-drove the ErrorRetryable doc through the wire"
    );

    // No new offline code drawn; the doc's offline identity is unchanged.
    assert_eq!(
        consumed_code_count(&pool).await,
        codes_before,
        "re-drive must NOT consume a new offline code (INV-4: re-send stored bytes, no re-sign)"
    );
    assert_eq!(
        doc_offline_fiscal_no(&pool, doc).await,
        offline_no_before,
        "re-drive must NOT re-mint the offline_fiscal_no"
    );
}

// ─── Pin #5: bounded → RMR at budget; ACK within budget → finalize ──

/// PIN #5a: an ERROR_RETRYABLE doc carrying `DrainChainSettleRetry` is
/// re-driven (Redrive) while under its budget — a `-8` this tick keeps it in
/// ERROR_RETRYABLE, NO escalation.
#[tokio::test]
async fn pin5a_drain_chain_settle_under_budget_redrives_no_escalate() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(
        &pool,
        NodeMode::GoingOnline,
        ShiftState::OpenedLocalPendingDrain,
    )
    .await;
    let shift_id = seed_pending_drain_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    // 3 prior DrainChainSettleRetry attempts — under the WebCheck-scale budget.
    let doc = seed_error_retryable_doc_with_trace(
        &pool,
        session_id,
        shift_id,
        1,
        100,
        3,
        "DrainChainSettleRetry",
    )
    .await;

    // Still -8 → re-drive keeps it ErrorRetryable (under budget), no escalate.
    let c = carriers(vec![server_reject(-8, "ERROR_XML_DATE")], vec![]);
    let view = view_for(&c);
    backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(
        c.dps.send_chk_count(),
        1,
        "under budget: the doc is RE-DRIVEN through stage_send"
    );
    assert_eq!(read_doc_state(&pool, doc).await, "ERROR_RETRYABLE");
    assert_eq!(
        read_shift_state(&pool, shift_id).await,
        "OPENED_LOCAL_PENDING_DRAIN"
    );
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await,
        0
    );
}

/// PIN #5b: once the drain-`-8` budget is exhausted, the ER-class guard
/// escalates to RequiresManualReconciliation (NO wire re-drive) — the doc
/// can't chain-settle forever.
#[tokio::test]
async fn pin5b_drain_chain_settle_budget_exhausted_escalates_to_manual() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(
        &pool,
        NodeMode::GoingOnline,
        ShiftState::OpenedLocalPendingDrain,
    )
    .await;
    let shift_id = seed_pending_drain_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    // Seed AT the budget cap (the dedicated drain-`-8` budget).
    let doc = seed_error_retryable_doc_with_trace(
        &pool,
        session_id,
        shift_id,
        1,
        100,
        backlog_drain::MAX_DRAIN_CHAIN_SETTLE_ATTEMPTS as i32,
        "DrainChainSettleRetry",
    )
    .await;

    // No wire responses queued — budget-exhausted MUST NOT re-drive.
    let c = carriers(vec![], vec![]);
    let view = view_for(&c);
    backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(
        c.dps.send_chk_count(),
        0,
        "budget exhausted MUST NOT re-drive through the wire"
    );
    assert_eq!(
        read_doc_state(&pool, doc).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "at budget, the doc CAS'd off ER to Manual (chain-settle cannot be infinite)"
    );
    assert_eq!(
        read_shift_state(&pool, shift_id).await,
        "REQUIRES_MANUAL_RECONCILIATION"
    );
}

// ─── Pin #6: strict-sequential — a held `-8` blocks its successors ──

/// PIN #6 (strict-sequential, AUD-K8-1): when the first backlog doc draws a
/// transient `-8` and holds in ERROR_RETRYABLE, the drain HALTS this tick —
/// the successor doc (`lnd+1`) is NOT sent (no out-of-order issuance).
#[tokio::test]
async fn pin6_held_minus_8_blocks_successor_this_tick() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(
        &pool,
        NodeMode::GoingOnline,
        ShiftState::OpenedLocalPendingDrain,
    )
    .await;
    let shift_id = seed_pending_drain_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let doc1 = seed_offline_local_ack_doc(&pool, 1, 100, session_id, shift_id).await;
    let doc2 = seed_offline_local_ack_doc(&pool, 2, 101, session_id, shift_id).await;

    // Only ONE send response queued — if the drain tried to send doc2 after
    // doc1's -8, the empty-queue stub would panic.  It must NOT.
    let c = carriers(vec![server_reject(-8, "ERROR_XML_DATE")], vec![]);
    let view = view_for(&c);
    backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(
        c.dps.send_chk_count(),
        1,
        "strict-sequential: exactly ONE wire send — the successor is blocked by the held -8"
    );
    assert_eq!(read_doc_state(&pool, doc1).await, "ERROR_RETRYABLE");
    assert_eq!(
        read_doc_state(&pool, doc2).await,
        "OFFLINE_LOCAL_ACK",
        "the successor stays un-sent (no lnd+1 issuance past a held predecessor)"
    );
}

// ─── Pin #3 (integration): offline `-5` on drain still escalates ────

/// PIN #3 (integration mirror of the pure-fn pin): a genuine terminal XML
/// error (`-5`) on an offline-origin drain doc STILL escalates to Manual —
/// only `-8` is the chain-settle transient; scope is narrow.
#[tokio::test]
async fn pin3_offline_drain_minus_5_still_escalates_to_manual() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(
        &pool,
        NodeMode::GoingOnline,
        ShiftState::OpenedLocalPendingDrain,
    )
    .await;
    let shift_id = seed_pending_drain_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let doc = seed_offline_local_ack_doc(&pool, 1, 100, session_id, shift_id).await;

    let c = carriers(vec![server_reject(-5, "ERROR_TYPE")], vec![]);
    let view = view_for(&c);
    backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(
        read_doc_state(&pool, doc).await,
        "REJECTED",
        "offline -5 is a true terminal format error → Rejected (unchanged)"
    );
    assert_eq!(
        read_shift_state(&pool, shift_id).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "a terminal -5 STILL escalates the shift (only -8 is transient)"
    );
}
