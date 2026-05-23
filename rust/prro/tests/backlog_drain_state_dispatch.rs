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
//! Tests (5 original + 8 W9b ER-class-guard 2026-05-22):
//!
//!   1. `c5_sent_doc_lastchk_match_advances_to_kvt1_with_replay_flag`
//!   2. `c5_kvt1_doc_w12_only_no_db_mutation_records_deferred`
//!   3. `c5_error_retryable_doc_re_driven_via_stage_send_to_kvt1`
//!      (rewritten 2026-05-22: now seeds durable
//!      `TransientRetry` + under-budget attempts before asserting wire
//!      redrive, per W9b ER-class-guard caller obligation).
//!   4. `c5_sent_doc_lastchk_mismatch_records_per_doc_failure_no_wire_resend`
//!   5. `c5_empty_skip_when_only_terminal_state_docs_exist`
//!   6. `er_guard_budget_exhausted_no_wire_escalates_to_manual`
//!   7. `er_guard_fn_config_error_no_wire_escalates_to_manual`
//!   8. `er_guard_wrapper_bug_no_wire_escalates_to_manual`
//!   9. `er_guard_operator_escalation_no_wire_escalates_to_manual`
//!  10. `er_guard_mac_recovery_no_wire_escalates_to_manual`
//!  11. `er_guard_terminal_reject_no_wire_escalates_critical_inconsistent`
//!  12. `er_guard_probe_required_no_wire_holds_in_er_sibling_continue`
//!  13. `er_guard_indeterminate_no_trace_no_wire_holds_in_er_sibling_continue`

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
use prro::BootError;
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

/// W9b ER-class-guard test helper (2026-05-22): seed a COMPLETE
/// `transport_trace` row for `doc_id` so `last_attempt_retry_class_for`
/// + `attempts_used` return deterministic values during the drain tick.
///
/// `retry_class = None` writes NULL — exercises the `HoldIndeterminate`
/// arm (alongside the "no row at all" sub-case).  The completion
/// columns are populated to satisfy the migration-013 self-consistency
/// CHECK (row is either fully incomplete or fully complete).
async fn seed_transport_trace_attempt(
    pool: &SqlitePool,
    doc_id: DocumentId,
    attempt_no: i32,
    retry_class: Option<&str>,
) {
    let sha = vec![0x42u8; 32];
    sqlx::query(
        "INSERT INTO transport_trace( \
            document_id, attempt_no, started_at, \
            backend_profile_id, transport_profile_id, request_envelope_sha256, \
            completed_at, wire_call_started_at, wire_call_finished_at, \
            outcome_kind, server_status_code, error_kind, error_message, retry_class \
         ) VALUES ( \
            ?, ?, '2026-05-22T00:00:00Z', 'b', 't', ?, \
            '2026-05-22T00:00:02Z', '2026-05-22T00:00:01Z', '2026-05-22T00:00:02Z', \
            'RETRYABLE_SERVER', -1, 'Server', 'seed-er-class-guard', ? \
         )",
    )
    .bind(doc_id)
    .bind(attempt_no)
    .bind(&sha)
    .bind(retry_class)
    .execute(pool)
    .await
    .unwrap();
}

async fn audit_count(pool: &SqlitePool, event_type: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type = ?")
        .bind(event_type)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn audit_latest_payload(pool: &SqlitePool, event_type: &str) -> Option<serde_json::Value> {
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

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

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

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

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
//
// W9b ER-class-guard 2026-05-22 rewrite: durable `TransientRetry` +
// under-budget attempts MUST be seeded in `transport_trace` before
// asserting wire redrive.  Previously, this test asserted re-drive
// against a plain ER doc with no trace history — locking the unsafe
// behavior fixed by HIGH-M3B-01 (the ER class guard now holds
// `HoldIndeterminate` for that input).

#[tokio::test]
async fn c5_error_retryable_doc_re_driven_via_stage_send_to_ack_via_w12() {
    // **M3b W12 Commit 4b.3 (2026-05-22)** — refactored from
    // pre-W12 `c5_error_retryable_doc_re_driven_via_stage_send_to_kvt1`.
    // ER → stage_send re-drive → Sent → confirm_drain_doc(SentFresh)
    // → Envelope 1a + Envelope 2 → ACK (full W12 chain).
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    // ERROR_RETRYABLE — stage_send 4-pre W9a source whitelist accepts.
    // server_fiscal_no NULL because 4-b never stamps it on transient
    // failure paths.
    let doc = seed_doc_in_state(&pool, 1, 100, session_id, shift_id, "ERROR_RETRYABLE", None).await;
    // W9b ER-class-guard authorization gate: durable TransientRetry
    // last-attempt + attempts_used = 1 < MAX_BOOT_ATTEMPTS (5).
    seed_transport_trace_attempt(&pool, doc, 1, Some("TransientRetry")).await;
    // W12 chain bootstrap — single doc.
    common::init_chain_seed(&pool, FN, common::chain_anchor(0x00))
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc,
        common::chain_anchor(0x00),
        common::chain_anchor(0x01),
    )
    .await
    .unwrap();

    let c = carriers(
        vec![Ok(ack("DPS-FN-RETRIED", vec![1, 2, 3]))],
        vec![Ok(ack("DPS-FN-RETRIED", vec![0xAA; 32]))],
    );
    let view = view_for(&c);

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(
        summary.advanced_to_ack(),
        1,
        "ER re-drive → W12 SentFresh chain → ACK"
    );
    assert_eq!(summary.advanced_to_kvt1(), 0, "no DeferredKvt1 post-W12");
    assert_eq!(
        summary.advanced_via_lastchk_replay(),
        0,
        "no replay flag — wire re-drive (SentFresh), not lastChk replay (SentReplay)"
    );
    assert_eq!(c.dps.send_chk_count(), 1, "stage_send re-drove the doc");
    assert_eq!(c.dps.last_chk_count(), 1, "confirm_drain_doc lastChk × 1");
    assert_eq!(read_doc_state(&pool, doc).await, "ACK");

    let payload = audit_latest_payload(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED")
        .await
        .unwrap();
    // from_state is the cohort-walker snapshot (ERROR_RETRYABLE) per
    // plan §65-70 LOW-PR70-R12-02 cohort-entry convention.
    assert_eq!(payload["from_state"], "ERROR_RETRYABLE");
    assert_eq!(payload["to_state"], "KVT2");
    assert_eq!(payload["dispatch_via"], "kvt2_confirm");
    assert_eq!(payload["evidence_source"], "lastChk");
}

// ─── W9b ER-class-guard 2026-05-22 negative-coverage matrix ──────────
//
// HIGH-M3B-01 fix: each non-redrive `ErRedriveDecision` arm MUST hold
// the doc out of `stage_send::run` (no wire re-drive) AND project the
// correct manual-recon / sibling-continue verdict.  These tests
// individually seed the durable `retry_class` (or absence thereof) and
// assert `send_chk_count == 0` + the appropriate state outcome +
// `OFFLINE_DRAIN_DOC_FAILED` payload shape.

// Helper for the 7 "ER + retry_class set" negative cases.
async fn seed_er_with_class(
    pool: &SqlitePool,
    retry_class: &str,
    attempt_no: i32,
) -> (DocumentId, ShiftId, OfflineSessionId) {
    seed_node_state(pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(pool).await;
    let session_id = seed_offline_session(pool, OfflineSessionState::Open).await;
    let doc = seed_doc_in_state(pool, 1, 100, session_id, shift_id, "ERROR_RETRYABLE", None).await;
    seed_transport_trace_attempt(pool, doc, attempt_no, Some(retry_class)).await;
    (doc, shift_id, session_id)
}

#[tokio::test]
async fn er_guard_budget_exhausted_no_wire_escalates_to_manual() {
    let (_d, pool) = fresh_pool().await;
    // attempt_no = MAX_BOOT_ATTEMPTS (5) → attempts_used = 5 → exhausted.
    let (doc, _shift, _sess) = seed_er_with_class(&pool, "TransientRetry", 5).await;

    let c = carriers(vec![], vec![]);
    let view = view_for(&c);
    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(
        c.dps.send_chk_count(),
        0,
        "budget exhausted MUST NOT re-drive via stage_send"
    );
    assert_eq!(c.dps.last_chk_count(), 0);
    assert_eq!(summary.advanced_to_kvt1(), 0);
    assert_eq!(summary.per_doc_failures().len(), 1);
    assert_eq!(
        read_doc_state(&pool, doc).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "doc CAS'd off ER per stage_send.rs:18 budget cap"
    );

    // Atomic CAS + audit envelope (er_class_guard helper).
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_ER_ESCALATED_TO_MANUAL").await,
        1
    );
    // Per-doc audit with full payload.
    let p = audit_latest_payload(&pool, "OFFLINE_DRAIN_DOC_FAILED")
        .await
        .unwrap();
    assert_eq!(p["failure_class"], "budget_exhausted");
    assert_eq!(p["retry_class"], "TransientRetry");
    assert_eq!(p["attempts_used"], 5);
    assert_eq!(p["max_boot_attempts"], 5);
    assert_eq!(p["manual_recon_class"], true);
    assert_eq!(p["dispatch_via"], "er_class_guard");
}

#[tokio::test]
async fn er_guard_fn_config_error_no_wire_escalates_to_manual() {
    let (_d, pool) = fresh_pool().await;
    let (doc, _shift, _sess) = seed_er_with_class(&pool, "FnConfigError", 1).await;

    let c = carriers(vec![], vec![]);
    let view = view_for(&c);
    backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(
        c.dps.send_chk_count(),
        0,
        "FnConfigError MUST NOT re-drive via stage_send"
    );
    assert_eq!(
        read_doc_state(&pool, doc).await,
        "REQUIRES_MANUAL_RECONCILIATION"
    );
    let p = audit_latest_payload(&pool, "OFFLINE_DRAIN_DOC_FAILED")
        .await
        .unwrap();
    assert_eq!(p["failure_class"], "authorization");
    assert_eq!(p["retry_class"], "FnConfigError");
    assert_eq!(p["manual_recon_class"], true);
    assert_eq!(p["dispatch_via"], "er_class_guard");
}

#[tokio::test]
async fn er_guard_wrapper_bug_no_wire_escalates_to_manual() {
    let (_d, pool) = fresh_pool().await;
    let (doc, _shift, _sess) = seed_er_with_class(&pool, "WrapperBug", 1).await;

    let c = carriers(vec![], vec![]);
    let view = view_for(&c);
    backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(c.dps.send_chk_count(), 0, "WrapperBug MUST NOT re-drive");
    assert_eq!(
        read_doc_state(&pool, doc).await,
        "REQUIRES_MANUAL_RECONCILIATION"
    );
    let p = audit_latest_payload(&pool, "OFFLINE_DRAIN_DOC_FAILED")
        .await
        .unwrap();
    assert_eq!(p["failure_class"], "internal");
    assert_eq!(p["retry_class"], "WrapperBug");
    assert_eq!(p["manual_recon_class"], true);
}

#[tokio::test]
async fn er_guard_operator_escalation_no_wire_escalates_to_manual() {
    let (_d, pool) = fresh_pool().await;
    let (doc, _shift, _sess) = seed_er_with_class(&pool, "OperatorEscalation", 1).await;

    let c = carriers(vec![], vec![]);
    let view = view_for(&c);
    backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(
        c.dps.send_chk_count(),
        0,
        "OperatorEscalation MUST NOT re-drive"
    );
    assert_eq!(
        read_doc_state(&pool, doc).await,
        "REQUIRES_MANUAL_RECONCILIATION"
    );
    let p = audit_latest_payload(&pool, "OFFLINE_DRAIN_DOC_FAILED")
        .await
        .unwrap();
    assert_eq!(p["failure_class"], "server");
    assert_eq!(p["retry_class"], "OperatorEscalation");
    assert_eq!(p["manual_recon_class"], true);
}

#[tokio::test]
async fn er_guard_mac_recovery_no_wire_escalates_to_manual() {
    let (_d, pool) = fresh_pool().await;
    let (doc, _shift, _sess) = seed_er_with_class(&pool, "MacRecovery", 1).await;

    let c = carriers(vec![], vec![]);
    let view = view_for(&c);
    backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(c.dps.send_chk_count(), 0, "MacRecovery MUST NOT re-drive");
    assert_eq!(
        read_doc_state(&pool, doc).await,
        "REQUIRES_MANUAL_RECONCILIATION"
    );
    let p = audit_latest_payload(&pool, "OFFLINE_DRAIN_DOC_FAILED")
        .await
        .unwrap();
    assert_eq!(p["failure_class"], "internal");
    assert_eq!(p["retry_class"], "MacRecovery");
    assert_eq!(p["manual_recon_class"], true);
}

#[tokio::test]
async fn er_guard_terminal_reject_no_wire_escalates_critical_inconsistent() {
    let (_d, pool) = fresh_pool().await;
    let (doc, _shift, _sess) = seed_er_with_class(&pool, "TerminalReject", 1).await;

    let c = carriers(vec![], vec![]);
    let view = view_for(&c);
    backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(
        c.dps.send_chk_count(),
        0,
        "TerminalReject MUST NOT re-drive (structural inconsistency)"
    );
    assert_eq!(
        read_doc_state(&pool, doc).await,
        "REQUIRES_MANUAL_RECONCILIATION"
    );
    // Atomic CAS + audit row tagged structural inconsistency.
    let p = audit_latest_payload(&pool, "OFFLINE_DRAIN_DOC_FAILED")
        .await
        .unwrap();
    assert_eq!(p["failure_class"], "wire_routing_terminal_reject");
    assert_eq!(p["retry_class"], "TerminalReject");
    assert_eq!(p["manual_recon_class"], true);
    assert_eq!(p["structural_inconsistency"], true);
    // Sanity: the inner CAS+audit envelope still committed under
    // Critical severity (operator dashboard signal).
    let sev: String = sqlx::query_scalar(
        "SELECT severity FROM audit_log \
         WHERE event_type = 'OFFLINE_DRAIN_ER_ESCALATED_TO_MANUAL' \
         ORDER BY audit_id DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(sev, "CRITICAL");
}

#[tokio::test]
async fn er_guard_probe_required_no_wire_holds_in_er_sibling_continue() {
    let (_d, pool) = fresh_pool().await;
    let (doc, _shift, _sess) = seed_er_with_class(&pool, "ProbeRequired", 1).await;

    let c = carriers(vec![], vec![]);
    let view = view_for(&c);
    backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(c.dps.send_chk_count(), 0, "ProbeRequired MUST NOT re-drive");
    // No CAS — doc stays in ER; sibling-continue.
    assert_eq!(read_doc_state(&pool, doc).await, "ERROR_RETRYABLE");
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_ER_ESCALATED_TO_MANUAL").await,
        0,
        "ProbeRequired hold MUST NOT emit escalation audit"
    );
    let p = audit_latest_payload(&pool, "OFFLINE_DRAIN_DOC_FAILED")
        .await
        .unwrap();
    assert_eq!(p["failure_class"], "wire_routing_probe_required");
    assert_eq!(p["retry_class"], "ProbeRequired");
    assert_eq!(
        p["manual_recon_class"], false,
        "ProbeRequired is hold, not manual-recon"
    );
    assert_eq!(p["hold_reason"], "probe_required");
}

#[tokio::test]
async fn er_guard_indeterminate_no_trace_no_wire_holds_in_er_sibling_continue() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    // No transport_trace row at all — `last_attempt_retry_class_for`
    // returns None → HoldIndeterminate arm.
    let doc = seed_doc_in_state(&pool, 1, 100, session_id, shift_id, "ERROR_RETRYABLE", None).await;

    let c = carriers(vec![], vec![]);
    let view = view_for(&c);
    backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(
        c.dps.send_chk_count(),
        0,
        "indeterminate retry_class MUST NOT re-drive"
    );
    assert_eq!(read_doc_state(&pool, doc).await, "ERROR_RETRYABLE");
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_ER_ESCALATED_TO_MANUAL").await,
        0
    );
    let p = audit_latest_payload(&pool, "OFFLINE_DRAIN_DOC_FAILED")
        .await
        .unwrap();
    assert_eq!(p["failure_class"], "retry_class_indeterminate");
    assert!(
        p["retry_class"].is_null(),
        "retry_class MUST be null in payload"
    );
    assert_eq!(
        p["manual_recon_class"], false,
        "indeterminate is hold per operator-pinned 2026-05-22 scope"
    );
    assert_eq!(p["hold_reason"], "retry_class_indeterminate");
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
    let c = carriers(vec![], vec![Ok(ack("DPS-FN-DIFFERENT-DOC", vec![0xFF]))]);
    let view = view_for(&c);

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

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
    let _doc_rej = seed_doc_in_state(&pool, 2, 101, session_id, shift_id, "REJECTED", None).await;

    let c = carriers(vec![], vec![]);
    let view = view_for(&c);

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

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
    // M3b W12 Commit 4b.3: chain seed for the offline doc (online
    // doc not in cohort → no W12 prereqs needed).
    common::init_chain_seed(&pool, FN, common::chain_anchor(0x00))
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        offline_doc,
        common::chain_anchor(0x00),
        common::chain_anchor(0x01),
    )
    .await
    .unwrap();

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

    let c = carriers(
        vec![Ok(ack("DPS-OFFLINE", vec![0xCD]))],
        vec![Ok(ack("DPS-OFFLINE", vec![0xAA; 32]))],
    );
    let view = view_for(&c);

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    // Cohort size 1 (offline doc only); online doc not visited.
    assert_eq!(
        summary.backlog_size_before(),
        1,
        "walker MUST filter by offline_session_id + fs_mode; online doc excluded"
    );
    assert_eq!(
        summary.advanced_to_ack(),
        1,
        "offline doc reaches Ack via W12 SentFresh chain"
    );
    assert_eq!(summary.advanced_to_kvt1(), 0, "no DeferredKvt1 post-W12");
    assert_eq!(c.dps.send_chk_count(), 1, "only offline doc reached wire");
    assert_eq!(c.dps.last_chk_count(), 1, "lastChk × 1 (offline doc only)");
    assert_eq!(read_doc_state(&pool, offline_doc).await, "ACK");
    // Online doc untouched (excluded from cohort by walker filter).
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

    let _summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

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

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

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

// ─── Test 9: KVT2 cohort widening (W12 Commit 3 reversal of MED-C5-4) ──
//
// The pre-W12 fixture `c5_kvt2_doc_excluded_from_cohort_pre_w12`
// asserted KVT2 was DEFERRED from the cohort SELECT IN list per
// MED-C5-4 — that defensive lock is **intentionally retired** by
// M3b W12 Commit 3, which re-adds KVT2 to the cohort and wires
// `process_via_w12_kvt2_advance` through `stage_finalize::run` for
// idempotent Kvt2→Ack advance.  See the new positive coverage at
// `w12_kvt2_cohort_entry_dispatches_to_stage_finalize_and_reaches_ack`
// below the W9b pending-drain halt section.

// ─── W9b ER-class-guard pending-drain halt proof ─────────────────────
//
// Spec amendment 2026-05-21 + LEGAL_INVARIANTS.md §INV-19: a manual-
// recon-class drain reject on a pending-drain shift halts the FN drain
// (CAS shift → RequiresManualReconciliation via edge 6, Critical
// `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` audit).  The W9b ER-class-
// guard 2026-05-22 fix introduces a NEW manual-recon-class verdict
// path (ER + non-transient durable retry_class) that MUST drive the
// halt ladder identically — without re-entering `stage_send::run`.
//
// Test pattern: pending-drain shift + ER doc + durable retry_class
// FnConfigError.  Drain MUST NOT wire-send; MUST CAS doc to manual;
// MUST CAS shift to manual; MUST emit halt audit.
#[tokio::test]
async fn er_guard_pending_drain_manual_class_halts_and_escalates_shift() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(
        &pool,
        NodeMode::GoingOnline,
        ShiftState::OpenedLocalPendingDrain,
    )
    .await;
    // Seed shift directly in OPENED_LOCAL_PENDING_DRAIN.
    let shift_id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts(shift_id, fiscal_number, serial, state, \
            open_mode, cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED_LOCAL_PENDING_DRAIN', 'ONLINE', 0, ?)",
    )
    .bind(shift_id)
    .bind(FN)
    .bind(CASHIER_OK)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE node_state SET current_shift_id = ? WHERE fiscal_number = ?")
        .bind(shift_id)
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;

    let doc = seed_doc_in_state(&pool, 1, 100, session_id, shift_id, "ERROR_RETRYABLE", None).await;
    seed_transport_trace_attempt(&pool, doc, 1, Some("FnConfigError")).await;

    // Empty DPS queues: any wire access = test crash.  Asserts the
    // halt happens BEFORE `stage_send::run` is entered.
    let c = carriers(vec![], vec![]);
    let view = view_for(&c);

    backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(
        c.dps.send_chk_count(),
        0,
        "ER-class-guard manual halt MUST NOT touch the wire"
    );

    // Doc CAS'd off ER into Manual via the ER class guard envelope.
    assert_eq!(
        read_doc_state(&pool, doc).await,
        "REQUIRES_MANUAL_RECONCILIATION"
    );

    // Shift CAS'd via edge 6 (pending-drain ladder).
    let shift_state: String = sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
        .bind(shift_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(shift_state, "REQUIRES_MANUAL_RECONCILIATION");

    // node_state.shift_state mirrors the shifts row inside the same tx
    // (HIGH-C4-5 load-bearing invariant).
    let node_shift: String =
        sqlx::query_scalar("SELECT shift_state FROM node_state WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(node_shift, "REQUIRES_MANUAL_RECONCILIATION");

    // Halt audit emitted exactly once + carries the ER-class-guard
    // failure_class.
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await,
        1
    );
    let halt = audit_latest_payload(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL")
        .await
        .unwrap();
    assert_eq!(halt["fiscal_number"], FN);
    assert_eq!(halt["failure_class"], "authorization");
    assert_eq!(halt["current_shift_state"], "OPENED_LOCAL_PENDING_DRAIN");
}

// ─── M3b W12 Commit 3: KVT2 cohort dispatch → stage_finalize → Ack ──
//
// Per plan §Phasing Commit 3 + §"Cohort widening" §14-15: KVT2 is
// re-added to the cohort SELECT IN list, and `process_via_w12_kvt2_advance`
// invokes `stage_finalize::run` for idempotent Kvt2→Ack advance.
// Test seeds a KVT2 doc with all stage_finalize::run preconditions
// (inbox PROCESSING row, chain seed match, KVT raw files), runs drain,
// asserts doc reaches Ack + summary records advanced_to_ack.

async fn seed_kvt2_doc_for_stage_finalize(
    pool: &SqlitePool,
    lnd: i64,
    code_lnd: i64,
    session_id: OfflineSessionId,
    shift_id: ShiftId,
    server_fiscal_no: &str,
) -> (DocumentId, [u8; 16]) {
    let doc_id = DocumentId::new();
    let req_id = Uuid::now_v7();
    let req_bytes = *req_id.as_bytes();
    let payload_sha = vec![0x77u8; 32];
    let unsigned_xml_sha: [u8; 32] = [0xABu8; 32];
    let previous_hash: [u8; 32] = [0xCDu8; 32];

    // ingress_inbox in PROCESSING — finalize marks it DONE.
    sqlx::query(
        "INSERT INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical, status) \
         VALUES (?, ?, 'REST', 'sell', ?, '{}', ?, 'PROCESSING')",
    )
    .bind(&req_bytes[..])
    .bind(FN)
    .bind(format!("idem-kvt2-{lnd}"))
    .bind(&payload_sha)
    .execute(pool)
    .await
    .unwrap();

    // KVT2 fiscal_documents row with chain hashes + server_fiscal_no.
    sqlx::query(
        "INSERT INTO fiscal_documents( \
            document_id, request_id, fiscal_number, shift_id, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, unsigned_xml_sha256, previous_hash, \
            signed_by_cashier_id, offline_session_id, offline_fiscal_no, \
            offline_fiscal_date, server_fiscal_no \
         ) VALUES ( \
            ?, ?, ?, ?, ?, 'SELL', 'KVT2', \
            'b', 't', 'OFFLINE', '2026-05-21T00:00:00Z', \
            '{}', ?, ?, ?, \
            ?, ?, ?, '2026-05-21T00:00:00Z', ? \
         )",
    )
    .bind(doc_id)
    .bind(&req_bytes[..])
    .bind(FN)
    .bind(shift_id)
    .bind(lnd)
    .bind(&payload_sha)
    .bind(&unsigned_xml_sha[..])
    .bind(&previous_hash[..])
    .bind(CASHIER_OK)
    .bind(session_id)
    .bind(code_lnd)
    .bind(server_fiscal_no)
    .execute(pool)
    .await
    .unwrap();

    // SIGNED_XML + KVT1_RAW + KVT2_RAW per finalize-helpers convention.
    for (kind, content) in [
        ("SIGNED_XML", b"FAKE-CMS".as_slice()),
        ("KVT1_RAW", b"FAKE-KVT1-PROTOBUF".as_slice()),
        ("KVT2_RAW", b"FAKE-KVT2-PROTOBUF".as_slice()),
    ] {
        sqlx::query(
            "INSERT INTO document_files(document_id, kind, content) \
             VALUES (?, ?, ?)",
        )
        .bind(doc_id)
        .bind(kind)
        .bind(content)
        .execute(pool)
        .await
        .unwrap();
    }

    // Chain seed pinned to doc.previous_hash so stage_finalize's W8 F2
    // chain-continuity guard passes.
    sqlx::query(
        "UPDATE node_state SET last_known_unsigned_xml_sha256 = ? \
         WHERE fiscal_number = ?",
    )
    .bind(&previous_hash[..])
    .bind(FN)
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

    (doc_id, req_bytes)
}

#[tokio::test]
async fn w12_kvt2_cohort_entry_dispatches_to_stage_finalize_and_reaches_ack() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;

    let (doc, req_id_bytes) =
        seed_kvt2_doc_for_stage_finalize(&pool, 1, 100, session_id, shift_id, "DPS-FN-KVT2-COHORT")
            .await;

    // No DPS calls — KVT2 dispatch goes through stage_finalize::run only.
    let c = carriers(vec![], vec![]);
    let view = view_for(&c);

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(c.dps.send_chk_count(), 0, "no wire call from KVT2 dispatch");
    assert_eq!(
        c.dps.last_chk_count(),
        0,
        "no lastChk call from KVT2 dispatch"
    );

    // KVT2 cohort entry advanced to Ack via stage_finalize::run.
    assert_eq!(read_doc_state(&pool, doc).await, "ACK");

    assert_eq!(summary.backlog_size_before(), 1);
    assert_eq!(summary.advanced_to_ack(), 1);
    assert_eq!(summary.advanced_to_kvt1(), 0);
    assert!(summary.per_doc_failures().is_empty());

    // Forensic audit: OFFLINE_DRAIN_DOC_ADVANCED with dispatch_via=
    // w12_kvt2_recovery + stage_finalize_outcome="Acked".
    let advanced = audit_latest_payload(&pool, "OFFLINE_DRAIN_DOC_ADVANCED")
        .await
        .unwrap();
    assert_eq!(advanced["from_state"], "KVT2");
    assert_eq!(advanced["to_state"], "ACK");
    assert_eq!(advanced["dispatch_via"], "w12_kvt2_recovery");
    assert_eq!(advanced["stage_finalize_outcome"], "Acked");

    // stage_finalize's STAGE_FINALIZE_ACK audit also emitted.
    assert_eq!(audit_count(&pool, "STAGE_FINALIZE_ACK").await, 1);

    // inbox row marked DONE by stage_finalize's W8 step 5.
    let inbox_status: String =
        sqlx::query_scalar("SELECT status FROM ingress_inbox WHERE request_id = ?")
            .bind(&req_id_bytes[..])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(inbox_status, "DONE");
}

// ─── M3b W12 Commit 3 amend — Err-path routing coverage ──────────────
//
// LOW-W12C3-01 close-out (post-narrow-review 2026-05-22):
//
// `process_via_w12_kvt2_advance` has 4 outcome arms after
// `stage_finalize::run` returns; the happy `Acked` arm is covered by
// `w12_kvt2_cohort_entry_dispatches_to_stage_finalize_and_reaches_ack`.
// The remaining 3 success-shape arms (`AlreadyAcked` / `StateConflict`
// / `DocumentMissing`) are **forensic-only concurrency-race outcomes**
// — they are not reachable via single-threaded `drain()` integration
// path because the cohort SELECT IN list filter requires `KVT2` state
// at SELECT time, after which `drain()` processes docs linearly with
// no concurrent writer in the test harness.  Their **generation** is
// already covered at the `stage_finalize::run` level in
// `write_path_stage5_finalize.rs`:
//   - `AlreadyAcked` → fixture `rerun_on_ack_is_idempotent_no_op`
//     (idempotent replay short-circuit) AND
//     `concurrent_finalize_yields_one_acked_and_one_already_acked`
//     (TRUE concurrency-race proof: two parallel `stage_finalize::run`
//     against the same doc yield exactly one Acked + one AlreadyAcked,
//     which is the production-realistic generation path for the
//     forensic outcome).
//   - `StateConflict` → fixture
//     `non_kvt2_state_short_circuits_no_seed_advance` (e.g. doc
//     observed as Kvt1 at stage_finalize CAS time).
//   - `DocumentMissing` → fixture
//     `document_missing_returns_outcome_not_error` (bogus doc id
//     race-with-delete proxy).
// Their **routing** in this helper is straight-line per-arm match
// (~10 lines each) and structurally mirrors the `Acked` arm.
//
// **Accepted-deferral on routing-projection coverage** (LOW-W12C3-02
// 2026-05-22): direct projection assertions for these three forensic
// outcomes (helper → `DocVerdict::Failed{manual_recon:true}` +
// `OFFLINE_DRAIN_DOC_FAILED` audit shape) are NOT covered by the
// current fixtures and are NOT structurally reachable via the
// public `drain()` entry in single-threaded tests.  Reaching them
// would require either (a) exposing `process_via_w12_kvt2_advance`
// as `pub(crate)` for a direct unit test (project anti-pattern —
// sibling helpers `process_via_stage_send` / `process_via_w12_only`
// are all private), OR (b) adding a `cfg(test)` projection-only
// seam.  Both routes were deliberately deferred 2026-05-22 to keep
// the Commit 3 production diff minimal.  Coverage will be added in
// a focused follow-up commit (a small `cfg(test)` seam exposing the
// routing-only projection logic, allowing 3 direct unit
// assertions) when the operator decides scope expansion is
// warranted.  Until then, **routing regressions for these arms
// would NOT be caught at integration test level** — caller (this
// helper) is the sole projection site, so a single-character bug
// in (e.g.) `FailureClass::StateConflict` vs `FailureClass::NotFound`
// for the wrong outcome would slip integration.  The structural
// mirroring to the well-tested `Acked` arm is the only current
// safeguard.
//
// What IS reproducible via `drain()` single-threaded is the
// `Err(StageFinalizeError)` arm — seeding a KVT2 doc with broken
// preconds forces `stage_finalize::run` into a typed-error path, and
// the helper's `map_err` must wrap it as
// `BootError::ReconciliationFailed { fiscal_number, source }` with
// per-FN attribution preserved (W9.4 cycle-2 MED-B convention).
// Fixture below covers that routing explicitly.

#[tokio::test]
async fn w12_kvt2_stage_finalize_typed_error_routes_to_boot_error_reconciliation_failed() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;

    // Seed KVT2 doc with full preconds, then NULL out
    // `unsigned_xml_sha256` to force `stage_finalize::run` into the
    // `UnsignedXmlShaMissing` typed-error path (proven path per
    // `write_path_stage5_finalize.rs::unsigned_xml_sha_missing_typed_error_full_rollback`).
    let (doc, _req_id_bytes) = seed_kvt2_doc_for_stage_finalize(
        &pool,
        1,
        100,
        session_id,
        shift_id,
        "DPS-FN-KVT2-TYPED-ERR",
    )
    .await;
    sqlx::query("UPDATE fiscal_documents SET unsigned_xml_sha256 = NULL WHERE document_id = ?")
        .bind(doc)
        .execute(&pool)
        .await
        .unwrap();

    // No DPS calls expected.
    let c = carriers(vec![], vec![]);
    let view = view_for(&c);

    let err = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .expect_err(
            "UnsignedXmlShaMissing typed error must surface as \
             BootError::ReconciliationFailed",
        );

    match err {
        BootError::ReconciliationFailed {
            fiscal_number: fn_tag,
            source,
        } => {
            assert_eq!(fn_tag, FN, "per-FN attribution preserved (MED-B W9.4)");
            let chain = format!("{source:#}");
            assert!(
                chain.contains("unsigned_xml_sha256"),
                "source chain should mention unsigned_xml_sha256 \
                 (got: {chain})",
            );
        }
        other => panic!("expected BootError::ReconciliationFailed, got {other:?}"),
    }

    // Err-path short-circuits BEFORE any audit emission in the
    // helper (Acked/AlreadyAcked paths emit OFFLINE_DRAIN_DOC_ADVANCED;
    // StateConflict/DocumentMissing paths emit OFFLINE_DRAIN_DOC_FAILED).
    // `stage_finalize::run` rolled back its envelope (W8.2 F5-bis
    // integrated rollback proof) — no STAGE_FINALIZE_ACK either.
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_DOC_ADVANCED").await,
        0,
        "Err path must NOT emit OFFLINE_DRAIN_DOC_ADVANCED"
    );
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_DOC_FAILED").await,
        0,
        "Err path must NOT emit OFFLINE_DRAIN_DOC_FAILED"
    );
    assert_eq!(
        audit_count(&pool, "STAGE_FINALIZE_ACK").await,
        0,
        "stage_finalize rollback proof: ACK audit must NOT exist"
    );

    // Doc state preserved as KVT2 (rollback proof).  Next drain tick
    // re-encounters the same broken-preconds doc → same Err →
    // operator observability via repeated BootError audit chain.
    assert_eq!(read_doc_state(&pool, doc).await, "KVT2");
}
