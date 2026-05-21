//! W9b Commit 5 — per-state dispatch + `lastChk` pre-flight +
//! widened drain cohort.
//!
//! Acceptance for spec amendment 2026-05-21 (HIGH-C4-1 + HIGH-C4-8
//! resolution + HIGH-C5-1 session scoping + MED-C5-4 KVT2 deferral)
//! + spec §2.3 Step A:
//!
//!   1. Walker scans the unfinished cohort scoped to the active
//!      offline session: `OFFLINE_LOCAL_ACK | SENT | KVT1 |
//!      ERROR_RETRYABLE` with `offline_session_id = ?` AND
//!      `fs_mode = 'OFFLINE'`.  `KVT2` deferred to W12 PR.
//!   2. Per-state dispatch by `doc.state`:
//!      - `OFFLINE_LOCAL_ACK` / `ERROR_RETRYABLE` → `stage_send::run`.
//!      - `SENT` → `lastChk` pre-flight (no wire fall-through;
//!        NotFound downgrades to `ErrorRetryable` for next-tick
//!        Pattern B re-drive per HIGH-C5-3).
//!      - `KVT1` → `apply_w12_confirmation` stub only.
//!   3. SENT replay rediscovery via lastChk Match advances Sent→Kvt1
//!      with audit `replay_short_circuit=true` AND persists
//!      `KVT1_RAW = ack.data_sign` byte-for-byte (HIGH-C5-2).
//!   4. Empty-skip fires only when zero unfinished candidates exist
//!      (terminal-state docs do NOT keep drain running).
//!
//! Tests (5):
//!
//!   1. `c5_sent_doc_lastchk_match_advances_to_kvt1_with_replay_flag`
//!   2. `c5_kvt1_doc_w12_only_no_db_mutation_records_deferred`
//!   3. `c5_error_retryable_doc_re_driven_via_stage_send_to_kvt1`
//!   4. `c5_sent_doc_lastchk_mismatch_records_per_doc_failure_no_wire_resend`
//!   5. `c5_empty_skip_when_only_terminal_state_docs_exist`

mod common;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

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

// ─── Local DPS stub: dual-queue for send_chk + last_chk ──────────────

/// W9b C5 test stub: scripted response queues for BOTH `send_chk`
/// and `last_chk` paths.  Distinct from the common `StubDpsChannel`
/// which `unreachable!()`s on `last_chk` — promoted to common module
/// when a third test file needs the dual-queue surface.
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

    fn last_chk_count(&self) -> usize {
        self.last_chk_calls.load(Ordering::SeqCst)
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
}

fn ack(id: &str, data_sign: Vec<u8>) -> CheckAck {
    CheckAck {
        id: id.into(),
        id_sign: vec![],
        data_sign,
    }
}

fn fn_sign() -> CheckSignBlob {
    CheckSignBlob(vec![0xAB, 0xCD])
}

// ─── Fixture helpers ─────────────────────────────────────────────────

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("w9b_c5.db"))
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

async fn seed_open_shift(pool: &SqlitePool) -> ShiftId {
    let shift_id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts(shift_id, fiscal_number, serial, state, \
            open_mode, cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED', 'ONLINE', 0, ?)",
    )
    .bind(shift_id)
    .bind(FN)
    .bind(CASHIER_OK)
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

/// Seed a fully-formed doc in arbitrary persisted state — covers the
/// W7a invariants for OFFLINE_LOCAL_ACK source AND supports SENT /
/// KVT1 / KVT2 / ERROR_RETRYABLE seeds for the C5 walker cohort tests.
///
/// `server_fiscal_no` is parametrised so SENT/KVT1/KVT2 docs (which
/// must have it NOT NULL by stage_send 4-b invariant) seed correctly,
/// while OFFLINE_LOCAL_ACK / ERROR_RETRYABLE leave it `None`.
#[allow(clippy::too_many_arguments)]
async fn seed_doc_in_state(
    pool: &SqlitePool,
    lnd: i64,
    code_lnd: i64,
    session_id: OfflineSessionId,
    shift_id: ShiftId,
    state: &str,
    server_fiscal_no: Option<&str>,
) -> DocumentId {
    let doc_id = DocumentId::new();
    let req_id = Uuid::now_v7();
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents( \
            document_id, request_id, fiscal_number, shift_id, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, signed_by_cashier_id, \
            offline_session_id, offline_fiscal_no, offline_fiscal_date, \
            server_fiscal_no \
         ) VALUES ( \
            ?, ?, ?, ?, ?, 'SELL', ?, \
            'b', 't', 'OFFLINE', '2026-05-21T00:00:00Z', \
            '{}', ?, ?, \
            ?, ?, '2026-05-21T00:00:00Z', \
            ? \
         )",
    )
    .bind(doc_id)
    .bind(req_id.as_bytes().to_vec())
    .bind(FN)
    .bind(shift_id)
    .bind(lnd)
    .bind(state)
    .bind(&sha)
    .bind(CASHIER_OK)
    .bind(session_id)
    .bind(code_lnd)
    .bind(server_fiscal_no)
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

async fn read_doc_state(pool: &SqlitePool, doc_id: DocumentId) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(doc_id)
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

async fn audit_latest_payload(
    pool: &SqlitePool,
    event_type: &str,
) -> Option<serde_json::Value> {
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT event_payload_json FROM audit_log \
         WHERE event_type = ? \
         ORDER BY audit_id DESC LIMIT 1",
    )
    .bind(event_type)
    .fetch_optional(pool)
    .await
    .unwrap();
    raw.and_then(|s| serde_json::from_str(&s).ok())
}

struct DepsCarriers {
    dps: Arc<DualQueueStub>,
    signing_ctx: SigningContext,
    fn_sign: CheckSignBlob,
}

fn carriers(
    send_chk: Vec<Result<CheckAck, DpsError>>,
    last_chk: Vec<Result<CheckAck, DpsError>>,
) -> DepsCarriers {
    DepsCarriers {
        dps: Arc::new(DualQueueStub::new(send_chk, last_chk)),
        signing_ctx: det_signing_ctx(),
        fn_sign: fn_sign(),
    }
}

fn view_for<'a>(carriers: &'a DepsCarriers) -> RuntimeView<'a> {
    RuntimeView {
        dps: carriers.dps.as_ref(),
        signing_ctx: &carriers.signing_ctx,
        fn_sign: &carriers.fn_sign,
    }
}

// ─── Test 1: SENT doc → lastChk Match → Kvt1 with replay flag ────────

#[tokio::test]
async fn c5_sent_doc_lastchk_match_advances_to_kvt1_with_replay_flag() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    // SENT doc — stage_send 4-b invariant requires server_fiscal_no NOT NULL.
    let doc = seed_doc_in_state(
        &pool,
        1,
        100,
        session_id,
        shift_id,
        "SENT",
        Some("DPS-FN-SENT-A"),
    )
    .await;

    // lastChk Match with non-empty data_sign → REPLAY HIT → advance
    // Sent → Kvt1.  send_chk queue empty — drain MUST NOT wire-resend.
    let c = carriers(
        vec![],
        vec![Ok(ack("DPS-FN-SENT-A", vec![0xAA, 0xBB, 0xCC]))],
    );
    let view = view_for(&c);

    let summary = backlog_drain::drain(&pool, &view, FN).await.unwrap();

    assert_eq!(summary.backlog_size_before(), 1);
    assert_eq!(summary.advanced_to_kvt1(), 1);
    assert_eq!(
        summary.advanced_via_lastchk_replay(),
        1,
        "replay flag MUST count this advance"
    );
    assert!(summary.per_doc_failures().is_empty());

    // Doc advanced to KVT1 via the C5 stub (Sent → Kvt1 CAS).
    assert_eq!(read_doc_state(&pool, doc).await, "KVT1");

    // Wire send was NOT invoked (drain used lastChk pre-flight only).
    assert_eq!(c.dps.send_chk_count(), 0, "no wire fall-through for SENT");
    assert_eq!(c.dps.last_chk_count(), 1);

    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_ADVANCED").await, 1);
    let payload = audit_latest_payload(&pool, "OFFLINE_DRAIN_DOC_ADVANCED")
        .await
        .unwrap();
    assert_eq!(payload["from_state"], "SENT");
    assert_eq!(payload["to_state"], "KVT1");
    assert_eq!(
        payload["replay_short_circuit"], true,
        "replay flag MUST be true on lastChk Match path"
    );
    assert_eq!(payload["dispatch_via"], "lastchk_replay");
    assert_eq!(payload["w12_status"], "DeferredKvt1");
}

// ─── Test 2: KVT1 doc → W12-only path, no DB mutation ────────────────

#[tokio::test]
async fn c5_kvt1_doc_w12_only_no_db_mutation_records_deferred() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let doc = seed_doc_in_state(
        &pool,
        1,
        100,
        session_id,
        shift_id,
        "KVT1",
        Some("DPS-FN-KVT1"),
    )
    .await;

    // No DPS calls expected — KVT1 dispatch goes through
    // apply_w12_confirmation stub only.
    let c = carriers(vec![], vec![]);
    let view = view_for(&c);

    let summary = backlog_drain::drain(&pool, &view, FN).await.unwrap();

    assert_eq!(summary.backlog_size_before(), 1);
    assert_eq!(summary.advanced_to_kvt1(), 1);
    assert_eq!(summary.advanced_via_lastchk_replay(), 0);
    assert!(summary.per_doc_failures().is_empty());

    // Pre-W12 stub: no DB mutation; doc stays in KVT1.
    assert_eq!(read_doc_state(&pool, doc).await, "KVT1");

    // No wire, no lastChk.
    assert_eq!(c.dps.send_chk_count(), 0);
    assert_eq!(c.dps.last_chk_count(), 0);

    let payload = audit_latest_payload(&pool, "OFFLINE_DRAIN_DOC_ADVANCED")
        .await
        .unwrap();
    assert_eq!(payload["from_state"], "KVT1");
    assert_eq!(payload["to_state"], "KVT1");
    assert_eq!(payload["dispatch_via"], "w12_only");
    assert_eq!(payload["w12_status"], "DeferredKvt1");
}

// ─── Test 3: ERROR_RETRYABLE doc → re-drive via stage_send → KVT1 ────

#[tokio::test]
async fn c5_error_retryable_doc_re_driven_via_stage_send_to_kvt1() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    // ERROR_RETRYABLE — stage_send 4-pre W9a source whitelist accepts.
    // server_fiscal_no NULL because 4-b never stamps it on transient
    // failure paths.
    let doc = seed_doc_in_state(
        &pool,
        1,
        100,
        session_id,
        shift_id,
        "ERROR_RETRYABLE",
        None,
    )
    .await;

    let c = carriers(vec![Ok(ack("DPS-FN-RETRIED", vec![1, 2, 3]))], vec![]);
    let view = view_for(&c);

    let summary = backlog_drain::drain(&pool, &view, FN).await.unwrap();

    assert_eq!(summary.advanced_to_kvt1(), 1);
    assert_eq!(
        summary.advanced_via_lastchk_replay(),
        0,
        "no replay flag — wire re-drive, not lastChk replay"
    );
    assert_eq!(c.dps.send_chk_count(), 1, "stage_send re-drove the doc");
    assert_eq!(c.dps.last_chk_count(), 0);
    assert_eq!(read_doc_state(&pool, doc).await, "KVT1");

    let payload = audit_latest_payload(&pool, "OFFLINE_DRAIN_DOC_ADVANCED")
        .await
        .unwrap();
    assert_eq!(payload["from_state"], "ERROR_RETRYABLE");
    assert_eq!(payload["to_state"], "KVT1");
    assert_eq!(payload["dispatch_via"], "stage_send");
    assert_eq!(payload["replay_short_circuit"], false);
}

// ─── Test 4: SENT doc → lastChk Mismatch → per-doc failure (no wire) ─

#[tokio::test]
async fn c5_sent_doc_lastchk_mismatch_records_per_doc_failure_no_wire_resend() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let doc = seed_doc_in_state(
        &pool,
        1,
        100,
        session_id,
        shift_id,
        "SENT",
        Some("DPS-FN-EXPECTED"),
    )
    .await;

    // lastChk returns OK but ack.id differs → Mismatch.  Drain MUST
    // NOT wire-resend (would double-fiscalize on the SENT-source
    // 4-pre source whitelist failure path).
    let c = carriers(
        vec![],
        vec![Ok(ack("DPS-FN-DIFFERENT-DOC", vec![0xFF]))],
    );
    let view = view_for(&c);

    let summary = backlog_drain::drain(&pool, &view, FN).await.unwrap();

    assert_eq!(summary.advanced_to_kvt1(), 0);
    assert_eq!(summary.per_doc_failures().len(), 1);
    assert_eq!(summary.per_doc_failures()[0].1, "internal");

    // Doc stays in SENT (no fallthrough wire call).
    assert_eq!(read_doc_state(&pool, doc).await, "SENT");
    assert_eq!(
        c.dps.send_chk_count(),
        0,
        "MUST NOT wire-resend SENT doc on lastChk Mismatch"
    );
    assert_eq!(c.dps.last_chk_count(), 1);

    let payload = audit_latest_payload(&pool, "OFFLINE_DRAIN_DOC_FAILED")
        .await
        .unwrap();
    assert_eq!(payload["failure_class"], "internal");
    assert_eq!(payload["probe_outcome"], "Mismatch");
    assert_eq!(payload["expected_server_fiscal_no"], "DPS-FN-EXPECTED");
    assert_eq!(payload["actual_server_fiscal_no"], "DPS-FN-DIFFERENT-DOC");
    assert_eq!(payload["manual_recon_class"], true);
    assert_eq!(payload["dispatch_via"], "lastchk_replay");
}

// ─── Test 5: empty-skip when only terminal-state docs exist ──────────

#[tokio::test]
async fn c5_empty_skip_when_only_terminal_state_docs_exist() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    // Seed terminal-state docs only.  The C5 walker SELECT filter
    // excludes ACK / REJECTED / CANCELLED / REQUIRES_MANUAL_RECONCILIATION.
    let _doc_ack = seed_doc_in_state(
        &pool,
        1,
        100,
        session_id,
        shift_id,
        "ACK",
        Some("DPS-FN-ACK"),
    )
    .await;
    let _doc_rej = seed_doc_in_state(
        &pool,
        2,
        101,
        session_id,
        shift_id,
        "REJECTED",
        None,
    )
    .await;

    let c = carriers(vec![], vec![]);
    let view = view_for(&c);

    let summary = backlog_drain::drain(&pool, &view, FN).await.unwrap();

    // Empty-backlog skip fires even though 2 docs exist for the FN —
    // both are in terminal states which the walker SELECT excludes.
    assert_eq!(summary.backlog_size_before(), 0);
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_SKIPPED_EMPTY_BACKLOG").await,
        1,
        "SKIPPED_EMPTY_BACKLOG MUST fire — terminal docs don't keep drain running"
    );
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_STARTED").await, 0);
    assert_eq!(c.dps.send_chk_count(), 0);
    assert_eq!(c.dps.last_chk_count(), 0);
}

// ─── Test 6: walker scope — online cross-session doc excluded ────────

/// HIGH-C5-1 (2026-05-21): the walker now filters by
/// `offline_session_id = active_session_id` AND `fs_mode = 'OFFLINE'`.
/// An online doc of the same FN (offline_session_id NULL, fs_mode
/// ONLINE) MUST NOT appear in the cohort.  Without this filter,
/// the widened cohort would capture online M3a-territory docs.
#[tokio::test]
async fn c5_walker_scope_excludes_online_cross_session_docs() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;

    // Offline doc in the active session — IS in cohort.
    let offline_doc = seed_doc_in_state(
        &pool,
        1,
        100,
        session_id,
        shift_id,
        "OFFLINE_LOCAL_ACK",
        None,
    )
    .await;

    // Online SENT doc of same FN — fs_mode=ONLINE, offline_session_id
    // NULL.  MUST NOT appear in cohort.
    let online_doc = DocumentId::new();
    let req_id = Uuid::now_v7();
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents( \
            document_id, request_id, fiscal_number, shift_id, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, signed_by_cashier_id, \
            server_fiscal_no \
         ) VALUES ( \
            ?, ?, ?, ?, ?, 'SELL', 'SENT', \
            'b', 't', 'ONLINE', '2026-05-21T00:00:00Z', \
            '{}', ?, ?, ?)",
    )
    .bind(online_doc)
    .bind(req_id.as_bytes().to_vec())
    .bind(FN)
    .bind(shift_id)
    .bind(2_i64)
    .bind(&sha)
    .bind(CASHIER_OK)
    .bind("ONLINE-SENT-NOT-IN-COHORT")
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO document_files(document_id, kind, content) VALUES (?, 'SIGNED_XML', ?)",
    )
    .bind(online_doc)
    .bind(b"FAKE".to_vec())
    .execute(&pool)
    .await
    .unwrap();

    let c = carriers(vec![Ok(ack("DPS-OFFLINE", vec![0xCD]))], vec![]);
    let view = view_for(&c);

    let summary = backlog_drain::drain(&pool, &view, FN).await.unwrap();

    // Cohort size 1 (offline doc only); online doc not visited.
    assert_eq!(
        summary.backlog_size_before(),
        1,
        "walker MUST filter by offline_session_id + fs_mode; online doc excluded"
    );
    assert_eq!(summary.advanced_to_kvt1(), 1);
    assert_eq!(c.dps.send_chk_count(), 1, "only offline doc reached wire");
    assert_eq!(read_doc_state(&pool, offline_doc).await, "KVT1");
    // Online doc untouched.
    assert_eq!(read_doc_state(&pool, online_doc).await, "SENT");
}

// ─── Test 7: lastChk Match persists KVT1_RAW byte-for-byte ───────────

/// HIGH-C5-2 (2026-05-21): on lastChk REPLAY HIT, the helper MUST
/// persist `ack.data_sign` into `document_files::Kvt1Raw` inside the
/// same `with_immediate` envelope as the Sent→Kvt1 CAS + audit.
/// Matches M3a `boot_phase::advance_sent_to_kvt1_from_probe` evidence
/// contract (forensic KVT1_RAW per legal-trail requirements).
#[tokio::test]
async fn c5_lastchk_match_persists_kvt1_raw_byte_for_byte() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let doc = seed_doc_in_state(
        &pool,
        1,
        100,
        session_id,
        shift_id,
        "SENT",
        Some("DPS-FN-REPLAY"),
    )
    .await;

    let expected_data_sign: Vec<u8> = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x42, 0x13, 0x37];
    let c = carriers(
        vec![],
        vec![Ok(ack("DPS-FN-REPLAY", expected_data_sign.clone()))],
    );
    let view = view_for(&c);

    let _summary = backlog_drain::drain(&pool, &view, FN).await.unwrap();

    assert_eq!(read_doc_state(&pool, doc).await, "KVT1");
    // KVT1_RAW byte-for-byte equality.
    let kvt1_raw: Vec<u8> = sqlx::query_scalar(
        "SELECT content FROM document_files WHERE document_id = ? AND kind = 'KVT1_RAW'",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .expect("KVT1_RAW row MUST exist after lastChk Match");
    assert_eq!(
        kvt1_raw, expected_data_sign,
        "KVT1_RAW MUST equal ack.data_sign byte-for-byte"
    );
}

// ─── Test 8: lastChk NotFound → ER for retry, non-manual ─────────────

/// HIGH-C5-3 (2026-05-21): on lastChk NotFound (DPS has no record
/// for the FN_sign), the SENT doc downgrades to `ERROR_RETRYABLE`
/// for safe Pattern B re-drive next tick.  Non-manual class — does
/// NOT escalate pending-drain shifts.
#[tokio::test]
async fn c5_sent_doc_lastchk_not_found_downgrades_to_error_retryable_non_manual() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let doc = seed_doc_in_state(
        &pool,
        1,
        100,
        session_id,
        shift_id,
        "SENT",
        Some("DPS-FN-REPLAY"),
    )
    .await;

    let c = carriers(vec![], vec![Err(DpsError::NotFound)]);
    let view = view_for(&c);

    let summary = backlog_drain::drain(&pool, &view, FN).await.unwrap();

    // Per HIGH-C5-3: doc downgrades to ER for retry (no manual recon).
    assert_eq!(read_doc_state(&pool, doc).await, "ERROR_RETRYABLE");
    assert_eq!(
        summary.per_doc_failures()[0].1,
        "transport",
        "NotFound classified as transport (non-manual; retry budget retained)"
    );
    assert_eq!(c.dps.send_chk_count(), 0, "no wire-resend in this tick");
    assert_eq!(c.dps.last_chk_count(), 1);

    let payload = audit_latest_payload(&pool, "OFFLINE_DRAIN_DOC_FAILED")
        .await
        .unwrap();
    assert_eq!(payload["probe_outcome"], "NotFound");
    assert_eq!(payload["downgrade_target_state"], "ERROR_RETRYABLE");
    assert_eq!(
        payload["manual_recon_class"], false,
        "NotFound retains retry budget — MUST NOT be manual class"
    );
}

// ─── Test 9: KVT2 doc excluded from cohort (MED-C5-4 defensive) ──────

/// MED-C5-4 (2026-05-21): KVT2 docs are explicitly deferred to the
/// W12 PR.  The cohort walker SQL filter excludes `KVT2` from
/// `state IN (...)`.  Defensive coverage so a future refactor that
/// re-adds KVT2 to the SELECT without also reviving the dispatcher
/// arm + apply_w12_confirmation Kvt2 branch will break this test.
#[tokio::test]
async fn c5_kvt2_doc_excluded_from_cohort_pre_w12() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let kvt2_doc = seed_doc_in_state(
        &pool,
        1,
        100,
        session_id,
        shift_id,
        "KVT2",
        Some("DPS-FN-KVT2-PRE-W12"),
    )
    .await;

    let c = carriers(vec![], vec![]);
    let view = view_for(&c);

    let summary = backlog_drain::drain(&pool, &view, FN).await.unwrap();

    // Walker SQL filter excludes KVT2 → cohort empty → SKIPPED.
    assert_eq!(
        summary.backlog_size_before(),
        0,
        "KVT2 docs MUST NOT appear in pre-W12 drain cohort"
    );
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_SKIPPED_EMPTY_BACKLOG").await,
        1
    );
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_ADVANCED").await, 0);
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_FAILED").await, 0);

    // KVT2 doc state unchanged.
    assert_eq!(read_doc_state(&pool, kvt2_doc).await, "KVT2");
    assert_eq!(c.dps.send_chk_count(), 0);
    assert_eq!(c.dps.last_chk_count(), 0);
}
