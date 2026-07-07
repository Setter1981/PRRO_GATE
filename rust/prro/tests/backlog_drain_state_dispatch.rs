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
//!      - `KVT1` → `process_via_w12_only` →
//!        `kvt2_confirm::confirm_drain_doc(Kvt1Reentry, ...)` →
//!        Envelope 1b + Envelope 2 → ACK (M3b W12 Commit 5).
//!   3. SENT replay rediscovery via lastChk Match advances Sent→Kvt1
//!      with audit `replay_short_circuit=true` AND persists
//!      `KVT1_RAW = ack.data_sign` byte-for-byte (HIGH-C5-2).
//!   4. Empty-skip fires only when zero unfinished candidates exist
//!      (terminal-state docs do NOT keep drain running).
//!
//! **M3b W12 Commit 5 update (2026-05-22)**: Kvt1 dispatch refactored
//! to ACK-era; 4 new `w12_kvt1_reentry_*` integration fixtures added
//! covering NotFound→Drift / Mismatch→Drift / Transport→Hold /
//! ServerFiscalNoMissing→Drift (MED-W12C5-01 caller-level audit).
//!
//! Tests (5 original + 8 W9b ER-class-guard 2026-05-22 + 5 W12 4b
//! 2026-05-22 + 5 W12 5 2026-05-22):
//!
//!   1. `c5_sent_doc_lastchk_match_advances_to_kvt1_with_replay_flag`
//!   2. `c5_kvt1_doc_w12_reentry_advances_to_ack`
//!      (refactored 2026-05-22 W12 Commit 5: pre-W12 stub-locking
//!      assertions replaced by Kvt1Reentry chain → Envelope 1b +
//!      Envelope 2 → ACK; KVT1_RAW byte-for-byte + sha256 digest
//!      locked per LOW-W12C5-03).
//!   3. `c5_error_retryable_doc_re_driven_via_stage_send_to_ack_via_w12`
//!      (rewritten 2026-05-22 W12 Commit 4b: now seeds durable
//!      `TransientRetry` + under-budget attempts before asserting wire
//!      redrive, per W9b ER-class-guard caller obligation; ACK-era
//!      assertions).
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

use prro::db::models::enums::{DocState, NodeMode, OfflineSessionState, ShiftState};
use prro::db::models::ids::{DocumentId, OfflineSessionId, ShiftId};
use prro::services::offline_sync::backlog_drain;
use prro::services::reconciliation::runtime::RuntimeView;
use prro::services::write_path::kvt2_advance;
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{CheckAck, CheckEnvelope, CheckSignBlob, RroInfo, StatusSnapshot};
use prro::transports::dps::error::{AuthorizationKind, DpsError};
use prro::BootError;
use sha2::{Digest, Sha256};
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
    seed_doc_in_state_typed(
        pool,
        lnd,
        code_lnd,
        session_id,
        shift_id,
        state,
        server_fiscal_no,
        "SELL",
    )
    .await
}

/// As [`seed_doc_in_state`] but with a caller-chosen `doc_type` — added for the
/// NC-02 pin (a `SHIFT_OPEN` doc).  `seed_doc_in_state` delegates here with
/// `"SELL"`, so existing callers are unchanged.
#[allow(clippy::too_many_arguments)]
async fn seed_doc_in_state_typed(
    pool: &SqlitePool,
    lnd: i64,
    code_lnd: i64,
    session_id: OfflineSessionId,
    shift_id: ShiftId,
    state: &str,
    server_fiscal_no: Option<&str>,
    doc_type: &str,
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
            ?, ?, ?, ?, ?, ?, ?, \
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
    .bind(doc_type)
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

/// W9b ER-class-guard test helper (2026-05-22): seed COMPLETE
/// `transport_trace` SEND rows for `doc_id` so `last_attempt_retry_class_for`
/// + `attempts_used` return deterministic values during the drain tick.
///
/// M1-04 (architect ruling item 4, 2026-06-11): `attempts_used` is now
/// `COUNT(*) WHERE is_probe = 0`, no longer `MAX(attempt_no)`.  To seed a
/// budget of N used send attempts we must therefore insert N distinct send
/// rows (`attempt_no = 1..=attempt_no`), not one row stamped `attempt_no = N`.
/// All rows are SEND rows — `is_probe` is omitted so the migration-002 DEFAULT
/// 0 applies, i.e. every row counts toward the ER-redrive budget.  `MAX(attempt_no)`
/// is still `attempt_no`, so a subsequent `allocate_and_insert_tx` still gets
/// `attempt_no + 1`.
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
    for n in 1..=attempt_no {
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
        .bind(n)
        .bind(&sha)
        .bind(retry_class)
        .execute(pool)
        .await
        .unwrap();
    }
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

// ─── Test 1: SENT doc → lastChk Match → SentReplay full chain → ACK ──

/// **M3b W12 Commit 5b.2 (plan §412 production wiring, 2026-05-24)** —
/// refactored from pre-W12 `c5_sent_doc_lastchk_match_advances_to_kvt1_
/// with_replay_flag`.  Post-5b.2 production wiring
/// (`process_via_lastchk_replay` → `confirm_drain_doc(SentReplay)` →
/// Envelope 1c-pre + 1a-replay + Envelope 2 → ACK), the SENT-source
/// lastChk replay path now drives the doc all the way through to
/// terminal Ack via the bundled 5-write atomic envelope (trace.complete
/// OK + Kvt1Raw persist + Sent→Kvt1 CAS + Kvt1→Kvt2 CAS + audit з
/// replay_short_circuit=true).
#[tokio::test]
async fn c5b2_sent_replay_lastchk_match_advances_to_ack_with_replay_flag() {
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
    // W12 chain bootstrap — Envelope 2 (stage_finalize::run) needs
    // chain seed + finalize prereqs to reach terminal Ack.
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

    // lastChk Match with non-empty data_sign → REPLAY HIT → bundled
    // Envelope 1a-replay (5-write atomic) + Envelope 2 → ACK.
    // send_chk queue empty — drain MUST NOT wire-resend.
    let c = carriers(
        vec![],
        vec![Ok(ack("DPS-FN-SENT-A", vec![0xAA, 0xBB, 0xCC]))],
    );
    let view = view_for(&c);

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(summary.backlog_size_before(), 1);
    assert_eq!(
        summary.advanced_to_ack(),
        1,
        "SentReplay chain advances doc to Ack via 1a-replay + stage_finalize"
    );
    assert_eq!(summary.advanced_to_kvt1(), 0, "no DeferredKvt1 post-5b.2");
    assert_eq!(
        summary.advanced_via_lastchk_replay(),
        1,
        "replay flag MUST count this advance"
    );
    assert!(summary.per_doc_failures().is_empty());

    // Doc reaches terminal ACK via the W12 SentReplay chain.
    assert_eq!(read_doc_state(&pool, doc).await, "ACK");

    // Wire send was NOT invoked (drain used lastChk pre-flight only).
    assert_eq!(c.dps.send_chk_count(), 0, "no wire fall-through for SENT");
    assert_eq!(c.dps.last_chk_count(), 1);

    // Audit chain: KVT2_ADVANCED (Envelope 1a-replay) + STAGE_FINALIZE_ACK
    // (Envelope 2).  Pre-W12 OFFLINE_DRAIN_DOC_ADVANCED MUST NOT fire
    // (replaced by KVT2_ADVANCED + STAGE_FINALIZE_ACK chain).
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await,
        1,
        "Envelope 1a-replay emits KVT2_ADVANCED"
    );
    assert_eq!(audit_count(&pool, "STAGE_FINALIZE_ACK").await, 1);
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_ADVANCED").await, 0);

    let payload = audit_latest_payload(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED")
        .await
        .unwrap();
    assert_eq!(payload["from_state"], "SENT");
    assert_eq!(payload["to_state"], "KVT2");
    assert_eq!(payload["server_fiscal_no"], "DPS-FN-SENT-A");
    assert_eq!(
        payload["replay_short_circuit"], true,
        "SentReplay 1a-replay envelope MUST mark replay_short_circuit=true"
    );
    assert_eq!(payload["dispatch_via"], "kvt2_confirm");
    assert_eq!(payload["evidence_source"], "lastChk");
    // SentReplay-specific: trace_attempt_no threaded from Envelope 1c-pre
    // allocation into 1a-replay audit payload (plan §412).
    assert!(
        payload["trace_attempt_no"].is_i64(),
        "1a-replay payload MUST carry trace_attempt_no from 1c-pre allocation; \
         got: {payload:?}"
    );
}

// ─── Test 2: KVT1 doc → W12 Kvt1Reentry chain → ACK ─────────────────

/// **M3b W12 Commit 5 (2026-05-22)** — refactored from pre-W12
/// `c5_kvt1_doc_w12_only_no_db_mutation_records_deferred`.  Post W12
/// production wiring (`process_via_w12_only` → `confirm_drain_doc(
/// Kvt1Reentry, ...)` → Envelope 1b + Envelope 2 → ACK), the Kvt1
/// re-entry seam now drives the doc to terminal Ack via lastChk
/// evidence persistence (Kvt1Raw) + Kvt1→Kvt2 CAS + stage_finalize.
#[tokio::test]
async fn c5_kvt1_doc_w12_reentry_advances_to_ack() {
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
    // W12 chain bootstrap — single Kvt1 doc.
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

    // No send_chk (Kvt1 dispatch skips stage_send).
    // 1 last_chk Acked response — Kvt1Reentry chain.
    let c = carriers(vec![], vec![Ok(ack("DPS-FN-KVT1", vec![0xAAu8; 32]))]);
    let view = view_for(&c);

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(summary.backlog_size_before(), 1);
    assert_eq!(
        summary.advanced_to_ack(),
        1,
        "Kvt1Reentry chain advances doc to Ack"
    );
    assert_eq!(summary.advanced_to_kvt1(), 0, "no DeferredKvt1 post-W12");
    assert_eq!(summary.advanced_via_lastchk_replay(), 0);
    assert!(summary.per_doc_failures().is_empty());

    // Doc reaches ACK via Envelope 1b + Envelope 2.
    assert_eq!(read_doc_state(&pool, doc).await, "ACK");

    // No wire (no stage_send), 1 lastChk (Kvt1Reentry evidence).
    assert_eq!(c.dps.send_chk_count(), 0);
    assert_eq!(c.dps.last_chk_count(), 1);

    // Audit chain: KVT2_ADVANCED (Envelope 1b) + STAGE_FINALIZE_ACK
    // (Envelope 2).  Pre-W12 OFFLINE_DRAIN_DOC_ADVANCED MUST NOT
    // fire.
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await,
        1,
        "Envelope 1b emits KVT2_ADVANCED"
    );
    assert_eq!(audit_count(&pool, "STAGE_FINALIZE_ACK").await, 1);
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_ADVANCED").await, 0);

    let payload = audit_latest_payload(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED")
        .await
        .unwrap();
    assert_eq!(payload["from_state"], "KVT1");
    assert_eq!(payload["to_state"], "KVT2");
    assert_eq!(payload["dispatch_via"], "kvt2_confirm");
    assert_eq!(payload["evidence_source"], "lastChk");
    assert_eq!(payload["server_fiscal_no"], "DPS-FN-KVT1");
    // Kvt1Reentry has no attempt_no (no fresh stage_send this tick).
    assert!(
        payload.get("attempt_no").is_none() || payload["attempt_no"].is_null(),
        "Kvt1Reentry envelope 1b payload MUST NOT carry attempt_no \
         (no fresh wire attempt this tick); got: {payload:?}"
    );

    // **LOW-W12C5-03 fix (5 Δ, 2026-05-22)**: lock Envelope 1b
    // evidence-persistence contract end-to-end.  KVT1_RAW row in
    // document_files MUST equal lastChk.data_sign byte-for-byte
    // (HIGH-C5-2 forensic anchor); audit payload's
    // kvt1_raw_sha256_hex MUST equal SHA256 of those persisted
    // bytes (MED-W12C4A-A plan §62 audit-digest contract).
    let expected_data_sign = vec![0xAAu8; 32];
    let persisted_kvt1_raw: Vec<u8> = sqlx::query_scalar(
        "SELECT content FROM document_files WHERE document_id = ? AND kind = 'KVT1_RAW'",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .expect("Envelope 1b MUST persist KVT1_RAW row in document_files");
    assert_eq!(
        persisted_kvt1_raw, expected_data_sign,
        "KVT1_RAW persisted bytes MUST equal lastChk.data_sign byte-for-byte \
         (HIGH-C5-2 forensic anchor)"
    );
    let expected_digest_hex = format!("{:x}", Sha256::digest(&persisted_kvt1_raw));
    assert_eq!(
        payload["kvt1_raw_sha256_hex"], expected_digest_hex,
        "OFFLINE_DRAIN_KVT2_ADVANCED.kvt1_raw_sha256_hex MUST equal SHA256 \
         of persisted KVT1_RAW bytes (plan §62 audit-digest contract)"
    );
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
    // 5 SEND rows (is_probe=0) = MAX_BOOT_ATTEMPTS → attempts_used = 5 →
    // exhausted (M1-04: attempts_used is COUNT of send rows, not MAX(attempt_no)).
    let (doc, shift_id, _sess) = seed_er_with_class(&pool, "TransientRetry", 5).await;
    // Strict-sequential (architect ruling B, 2026-06-13): a terminal failure on a
    // plain Opened shift now escalates the FN shift to Manual via whitelist edge 15;
    // escalate_drain_to_manual reads node_state.current_shift_id, so backfill it.
    sqlx::query("UPDATE node_state SET current_shift_id = ? WHERE fiscal_number = ?")
        .bind(shift_id)
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();

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

    // Strict-sequential: terminal failure HALTS + escalates the FN shift to
    // Manual (plain Opened → whitelist edge 15) + Critical halt audit.
    let shift_state: String = sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
        .bind(shift_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(shift_state, "REQUIRES_MANUAL_RECONCILIATION");
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await,
        1
    );
}

#[tokio::test]
async fn er_guard_fn_config_error_no_wire_escalates_to_manual() {
    let (_d, pool) = fresh_pool().await;
    let (doc, shift_id, _sess) = seed_er_with_class(&pool, "FnConfigError", 1).await;
    // Strict-sequential: terminal failure escalates the FN shift to Manual
    // (edge 15) which reads node_state.current_shift_id; backfill it.
    sqlx::query("UPDATE node_state SET current_shift_id = ? WHERE fiscal_number = ?")
        .bind(shift_id)
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();

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

    // Strict-sequential: terminal failure HALTS + escalates the FN shift to
    // Manual (plain Opened → whitelist edge 15) + Critical halt audit.
    let shift_state: String = sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
        .bind(shift_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(shift_state, "REQUIRES_MANUAL_RECONCILIATION");
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await,
        1
    );
}

#[tokio::test]
async fn er_guard_wrapper_bug_no_wire_escalates_to_manual() {
    let (_d, pool) = fresh_pool().await;
    let (doc, shift_id, _sess) = seed_er_with_class(&pool, "WrapperBug", 1).await;
    // Strict-sequential: terminal failure escalates the FN shift to Manual
    // (edge 15) which reads node_state.current_shift_id; backfill it.
    sqlx::query("UPDATE node_state SET current_shift_id = ? WHERE fiscal_number = ?")
        .bind(shift_id)
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();

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

    // Strict-sequential: terminal failure HALTS + escalates the FN shift to
    // Manual (plain Opened → whitelist edge 15) + Critical halt audit.
    let shift_state: String = sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
        .bind(shift_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(shift_state, "REQUIRES_MANUAL_RECONCILIATION");
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await,
        1
    );
}

#[tokio::test]
async fn er_guard_operator_escalation_no_wire_escalates_to_manual() {
    let (_d, pool) = fresh_pool().await;
    let (doc, shift_id, _sess) = seed_er_with_class(&pool, "OperatorEscalation", 1).await;
    // Strict-sequential: terminal failure escalates the FN shift to Manual
    // (edge 15) which reads node_state.current_shift_id; backfill it.
    sqlx::query("UPDATE node_state SET current_shift_id = ? WHERE fiscal_number = ?")
        .bind(shift_id)
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();

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

    // Strict-sequential: terminal failure HALTS + escalates the FN shift to
    // Manual (plain Opened → whitelist edge 15) + Critical halt audit.
    let shift_state: String = sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
        .bind(shift_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(shift_state, "REQUIRES_MANUAL_RECONCILIATION");
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await,
        1
    );
}

#[tokio::test]
async fn er_guard_mac_recovery_no_wire_escalates_to_manual() {
    let (_d, pool) = fresh_pool().await;
    let (doc, shift_id, _sess) = seed_er_with_class(&pool, "MacRecovery", 1).await;
    // Strict-sequential: terminal failure escalates the FN shift to Manual
    // (edge 15) which reads node_state.current_shift_id; backfill it.
    sqlx::query("UPDATE node_state SET current_shift_id = ? WHERE fiscal_number = ?")
        .bind(shift_id)
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();

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

    // Strict-sequential: terminal failure HALTS + escalates the FN shift to
    // Manual (plain Opened → whitelist edge 15) + Critical halt audit.
    let shift_state: String = sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
        .bind(shift_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(shift_state, "REQUIRES_MANUAL_RECONCILIATION");
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await,
        1
    );
}

#[tokio::test]
async fn er_guard_terminal_reject_no_wire_escalates_critical_inconsistent() {
    let (_d, pool) = fresh_pool().await;
    let (doc, shift_id, _sess) = seed_er_with_class(&pool, "TerminalReject", 1).await;
    // Strict-sequential: terminal failure escalates the FN shift to Manual
    // (edge 15) which reads node_state.current_shift_id; backfill it.
    sqlx::query("UPDATE node_state SET current_shift_id = ? WHERE fiscal_number = ?")
        .bind(shift_id)
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();

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

    // Strict-sequential: terminal failure ALSO HALTS + escalates the FN shift to
    // Manual (plain Opened → whitelist edge 15) + Critical halt audit (in addition
    // to the structural-inconsistency / Critical ER-escalation envelope above).
    let shift_state: String = sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
        .bind(shift_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(shift_state, "REQUIRES_MANUAL_RECONCILIATION");
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await,
        1
    );
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

// ─── Test 4: SentReplay lastChk Mismatch → structural drift halt ─────

/// **M3b W12 Commit 5b.2 (plan §412 production wiring, 2026-05-24)** —
/// refactored from pre-W12 `c5_sent_doc_lastchk_mismatch_records_per_
/// doc_failure_no_wire_resend`.  Behavioral pivot: pre-W12 per-doc
/// failure (manual_recon=true, drain continued) → W12 structural
/// drift HALTS the FN drain via `BootError::Internal`.
///
/// Forensic chain: Envelope 1c-pre allocates trace row →
/// `evaluate_lastchk` classifies LastChkIdMismatch → Envelope 1c-drift
/// (bundled trace.complete RetryableServer + `KVT2_CONFIRM_STRUCTURAL
/// _DRIFT` audit at Severity::Error) → `BootError::Internal` halt.
/// Doc state UNCHANGED (Sent — drift envelope does not transition).
#[tokio::test]
async fn c5b2_sent_replay_lastchk_mismatch_halts_drain_with_structural_drift_audit() {
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

    // lastChk returns OK but ack.id differs → Mismatch routed to
    // Kvt2ConfirmOutcome::StructuralDrift { LastChkIdMismatch }.
    // Drain MUST halt with BootError::Internal AND emit drift audit
    // BEFORE the halt (MED-W12C5-01 caller-level forensic contract).
    let c = carriers(vec![], vec![Ok(ack("DPS-FN-DIFFERENT-DOC", vec![0xFF]))]);
    let view = view_for(&c);

    let result = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN).await;
    let err = result.expect_err(
        "SentReplay Mismatch MUST halt drain with BootError::Internal per W12 StructuralDrift",
    );
    match err {
        BootError::Internal(msg) => assert!(
            msg.contains("LastChkIdMismatch") && msg.contains("StructuralDrift")
                || msg.contains("structural drift"),
            "BootError::Internal message MUST reference LastChkIdMismatch / structural drift; \
             got: {msg}"
        ),
        other => panic!("drain Err MUST be Internal; got: {other:?}"),
    }

    // Doc state UNCHANGED — drift envelope does not CAS state.
    assert_eq!(read_doc_state(&pool, doc).await, "SENT");
    assert_eq!(
        c.dps.send_chk_count(),
        0,
        "MUST NOT wire-resend SENT doc on lastChk Mismatch"
    );
    assert_eq!(c.dps.last_chk_count(), 1);

    // Pre-W12 OFFLINE_DRAIN_DOC_FAILED MUST NOT fire (replaced by
    // KVT2_CONFIRM_STRUCTURAL_DRIFT at Severity::Error).
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_FAILED").await, 0);
    assert_eq!(audit_count(&pool, "KVT2_CONFIRM_STRUCTURAL_DRIFT").await, 1);

    let payload = audit_latest_payload(&pool, "KVT2_CONFIRM_STRUCTURAL_DRIFT")
        .await
        .unwrap();
    assert_eq!(payload["source"], "sent_replay");
    assert_eq!(payload["drift_reason"], "LASTCHK_ID_MISMATCH");
    // drift_reason_detail includes both observed + expected ids
    // (OBS-W12C5-1 fix — per-variant detail message).
    let detail = payload["drift_reason_detail"].as_str().unwrap();
    assert!(
        detail.contains("DPS-FN-DIFFERENT-DOC") && detail.contains("DPS-FN-EXPECTED"),
        "drift_reason_detail MUST contain both observed + expected ids; got: {detail}"
    );
    assert_eq!(payload["dispatch_via"], "kvt2_confirm");
    // SentReplay-specific: trace_attempt_no threaded from 1c-pre.
    assert!(
        payload["trace_attempt_no"].is_i64(),
        "1c-drift bundled payload MUST carry trace_attempt_no from 1c-pre allocation; \
         got: {payload:?}"
    );
}

// ─── Test 4b (SEAM-B-3): SentReplay Mismatch on a SUPERSEDED SENT doc ─
//     → held (no halt), drain CONTINUES to the tip, TIP_SUPERSEDED audit.

/// **M2-N1 reversal of SEAM-B-3 (architect ruling B, 2026-06-13)** — under
/// strict-sequential drain a SUPERSEDED predecessor STOPS the chain.  SEAM-B-3
/// originally let the drain sibling-continue past a superseded SENT doc to
/// drain the higher-`lnd` tip.  But M2-01 made offline-origin docs a STRICT
/// predecessor chain, so the tip's `previous_hash = hash(superseded doc)` —
/// sending it while the predecessor's ACK status is UNKNOWN is unsafe.  And a
/// superseded doc is non-self-resolving without B1-v2 doc-scoped confirmation,
/// so merely holding it would wedge.  Therefore: halt + escalate the FN to
/// `RequiresManualReconciliation` (plain `Opened` → whitelist edge 15).  The
/// `TIP_SUPERSEDED` audit is still emitted (forensic record of WHY).
///
/// Cohort = two SENT offline-origin docs in `lnd ASC` order:
///   - `doc_a` (lnd=1, sfn="DPS-FN-A")  — superseded (M2-N4: DPS tip "DPS-FN-B"
///     IS our newer submitted doc_b's sfn, so genuine supersession, not drift)
///   - `doc_b` (lnd=2, sfn="DPS-FN-B")  — the DPS lastChk tip
/// `doc_a` is processed FIRST → SupersededHeld → halt + Manual; `doc_b` is
/// NOT processed (stays SENT), proving the strict chain stops at the superseded
/// predecessor (the reverse of SEAM-B-3's old continue).
#[tokio::test]
async fn seam_b3_superseded_predecessor_halts_chain_and_escalates_manual() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    // Drain reads node_state.current_shift_id to identify the shift to escalate
    // (M2-N2a edge 15 for plain Opened).
    sqlx::query("UPDATE node_state SET current_shift_id = ? WHERE fiscal_number = ?")
        .bind(shift_id)
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;

    // doc_a: lnd=1 — the superseded SENT doc.
    let doc_a = seed_doc_in_state(
        &pool,
        1,
        100,
        session_id,
        shift_id,
        "SENT",
        Some("DPS-FN-A"),
    )
    .await;
    // doc_b: lnd=2 — the FN's newest submitted doc (the DPS lastChk tip).
    let doc_b = seed_doc_in_state(
        &pool,
        2,
        101,
        session_id,
        shift_id,
        "SENT",
        Some("DPS-FN-B"),
    )
    .await;

    // Genesis-consistent 2-doc chain so `invariant_scan::assert_clean` holds:
    //   doc_a (chain HEAD): previous_hash NULL, unsigned = H_a.
    //   doc_b: previous_hash = H_a, unsigned = H_b.
    //   node seed = H_b (both OFFLINE-origin ⇒ seed advanced past both at
    //   offline-ack, M2-01).  Both stay SENT after the halt — both still
    //   "issued" in the scan walk → ends at H_b, matching node seed.
    let h_a = common::chain_anchor(0x01);
    let h_b = common::chain_anchor(0x02);
    sqlx::query("UPDATE fiscal_documents SET unsigned_xml_sha256 = ? WHERE document_id = ?")
        .bind(&h_a[..])
        .bind(doc_a)
        .execute(&pool)
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(&pool, FN, doc_b, h_a, h_b)
        .await
        .unwrap();
    common::init_chain_seed(&pool, FN, h_b).await.unwrap();

    // lastChk for doc_a returns the tip id "DPS-FN-B" → Mismatch.  M2-N4:
    // "DPS-FN-B" IS doc_b's sfn (a newer submitted doc of ours) → genuine
    // supersession.  doc_b is NOT probed (drain halts at doc_a) — only one
    // lastChk response is supplied.
    let c = carriers(vec![], vec![Ok(ack("DPS-FN-B", vec![0xAA, 0xBB, 0xCC]))]);
    let view = view_for(&c);

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .expect("superseded predecessor halts + escalates Manual (no BootError)");

    // Strict-sequential: doc_a held in SENT; doc_b NOT processed (stays SENT).
    assert_eq!(
        read_doc_state(&pool, doc_a).await,
        "SENT",
        "superseded predecessor is held in SENT (no state change)"
    );
    assert_eq!(
        read_doc_state(&pool, doc_b).await,
        "SENT",
        "tip is NOT drained — the strict chain stops at the superseded predecessor"
    );

    // Mismatch was a genuine supersession, NOT structural drift.
    assert_eq!(audit_count(&pool, "KVT2_CONFIRM_STRUCTURAL_DRIFT").await, 0);
    // TIP_SUPERSEDED forensic audit still emitted (records WHY the FN halted).
    assert_eq!(audit_count(&pool, "TIP_SUPERSEDED").await, 1);
    let sup = audit_latest_payload(&pool, "TIP_SUPERSEDED").await.unwrap();
    assert_eq!(sup["source"], "sent_replay");
    assert_eq!(sup["dps_tip_id"], "DPS-FN-B");
    assert_eq!(sup["doc_lnd"], 1);
    assert_eq!(sup["max_submitted_lnd"], 2);

    // FN escalated to durable manual-recon on a plain Opened shift (edge 15).
    let shift_state: String = sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
        .bind(shift_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        shift_state, "REQUIRES_MANUAL_RECONCILIATION",
        "superseded predecessor (non-self-resolving) escalates plain Opened → Manual"
    );
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await,
        1
    );

    // No advance; doc_a recorded held-at-sent; only doc_a probed.
    assert_eq!(summary.advanced_to_ack(), 0, "nothing reaches Ack");
    assert_eq!(
        summary.held_at_sent(),
        1,
        "superseded doc recorded held-at-sent"
    );
    assert_eq!(c.dps.send_chk_count(), 0, "no wire-resend");
    assert_eq!(
        c.dps.last_chk_count(),
        1,
        "only doc_a probed; doc_b NOT reached (strict halt)"
    );

    // Ledger invariant clean (both SENT offline-origin docs are issued).
    prro::db::invariant_scan::assert_clean(&pool).await;
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

// ─── Test 7: SentReplay Match persists KVT1_RAW byte-for-byte ────────

/// HIGH-C5-2 (2026-05-21) + LOW-W12C5-03 (5 Δ) + **5b.2 (2026-05-24)**:
/// on lastChk REPLAY HIT, Envelope 1a-replay MUST persist `ack.data_sign`
/// into `document_files::Kvt1Raw` inside the same `with_immediate` as
/// the Sent→Kvt1 CAS + Kvt1→Kvt2 CAS + trace.complete + audit.  The
/// audit payload's `kvt1_raw_sha256_hex` MUST equal SHA256 of those
/// persisted bytes (plan §62 audit-digest contract).  Refactored from
/// pre-W12 fixture which only locked KVT1 stop-point; now locks full
/// SentReplay chain to terminal ACK + KVT1_RAW forensic anchor.
#[tokio::test]
async fn c5b2_sent_replay_lastchk_match_persists_kvt1_raw_byte_for_byte() {
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
    // W12 chain bootstrap — Envelope 2 reaches terminal ACK.
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

    let expected_data_sign: Vec<u8> = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x42, 0x13, 0x37];
    let c = carriers(
        vec![],
        vec![Ok(ack("DPS-FN-REPLAY", expected_data_sign.clone()))],
    );
    let view = view_for(&c);

    let _summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    // Doc reaches terminal ACK via SentReplay chain (Envelope 1a-replay
    // + Envelope 2).  KVT1_RAW persisted inside 1a-replay's
    // with_immediate per HIGH-C5-2 forensic anchor.
    assert_eq!(read_doc_state(&pool, doc).await, "ACK");
    let kvt1_raw: Vec<u8> = sqlx::query_scalar(
        "SELECT content FROM document_files WHERE document_id = ? AND kind = 'KVT1_RAW'",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .expect("KVT1_RAW row MUST exist after SentReplay 1a-replay envelope commit");
    assert_eq!(
        kvt1_raw, expected_data_sign,
        "KVT1_RAW MUST equal ack.data_sign byte-for-byte (HIGH-C5-2 forensic anchor)"
    );
    // Plan §62 audit-digest contract: 1a-replay audit payload's
    // kvt1_raw_sha256_hex MUST equal SHA256 of persisted KVT1_RAW.
    let payload = audit_latest_payload(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED")
        .await
        .unwrap();
    let expected_digest_hex = format!("{:x}", Sha256::digest(&kvt1_raw));
    assert_eq!(
        payload["kvt1_raw_sha256_hex"], expected_digest_hex,
        "OFFLINE_DRAIN_KVT2_ADVANCED.kvt1_raw_sha256_hex MUST equal SHA256 \
         of persisted KVT1_RAW bytes (plan §62 audit-digest contract)"
    );
}

// ─── Test 8: SentReplay lastChk NotFound → HoldFnDrain ErRedriveQueued ──

/// **M3b W12 Commit 5b.2 (plan §412 HIGH-C5-3 safe-redrive seam,
/// 2026-05-24)** — refactored from pre-W12 `c5_sent_doc_lastchk_not_
/// found_downgrades_to_error_retryable_non_manual`.  Behavioral pivot:
/// pre-W12 per-doc failure (Transport, manual_recon=false, drain
/// continued) → W12 `HoldFnDrain { ErRedriveQueued }` HALTS this-tick
/// drain (DocVerdict::HoldFnDrain consumer logic at backlog_drain
/// line 884 → break).
///
/// Forensic chain: Envelope 1c-pre allocates trace row → DPS NotFound
/// classified to `Kvt2ConfirmOutcome::SentNotFoundDowngrade` → Envelope
/// 1c-post (bundled: trace.complete RetryableTransport + Sent→ER CAS
/// + OFFLINE_DRAIN_DOC_FAILED audit) → `Ok(HoldFnDrain {
/// ErRedriveQueued })`.  Next tick: W9b ER-class-guard reads
/// `retry_class=TransientRetry` + attempts_used<MAX → bounded Pattern
/// B redrive through `stage_send::run`.
#[tokio::test]
async fn c5b2_sent_replay_lastchk_not_found_holds_fn_drain_er_redrive_queued() {
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

    // Doc downgraded to ER via Envelope 1c-post (Sent→ER CAS bundled
    // з trace.complete + audit atomically).
    assert_eq!(read_doc_state(&pool, doc).await, "ERROR_RETRYABLE");
    // W12 HoldFnDrain projection accounting: ErRedriveQueued counter
    // (NOT per_doc_failures — that's pre-W12 path).
    assert_eq!(
        summary.er_redrive_queued(),
        1,
        "SentNotFoundDowngrade MUST record ErRedriveQueued projection"
    );
    assert_eq!(
        summary.held_at_sent(),
        0,
        "NotFound is downgrade (Sent→ER), not Hold-at-Sent"
    );
    assert_eq!(summary.advanced_to_kvt1(), 0);
    assert_eq!(summary.advanced_to_ack(), 0);
    assert_eq!(c.dps.send_chk_count(), 0, "no wire-resend in this tick");
    assert_eq!(c.dps.last_chk_count(), 1);

    // OFFLINE_DRAIN_DOC_FAILED audit comes from Envelope 1c-post,
    // not the pre-W12 downgrade_sent_to_error_retryable_for_retry
    // path.  Payload shape differs: `dispatch_via=kvt2_confirm`,
    // `probe_outcome=NotFound`, `downgrade_to=ERROR_RETRYABLE`,
    // `manual_recon_class=false`, `failure_class=transport`.
    let payload = audit_latest_payload(&pool, "OFFLINE_DRAIN_DOC_FAILED")
        .await
        .unwrap();
    assert_eq!(payload["failure_class"], "transport");
    assert_eq!(payload["probe_outcome"], "NotFound");
    assert_eq!(payload["downgrade_to"], "ERROR_RETRYABLE");
    assert_eq!(payload["expected_server_fiscal_no"], "DPS-FN-REPLAY");
    assert_eq!(payload["manual_recon_class"], false);
    assert_eq!(payload["dispatch_via"], "kvt2_confirm");
    assert!(
        payload["trace_attempt_no"].is_i64(),
        "1c-post payload MUST carry trace_attempt_no from 1c-pre allocation; \
         got: {payload:?}"
    );
}

// ─── Test 8a: SentReplay lastChk Match + EMPTY data_sign → Hold ──────

/// **M3b W12 Commit 5b.2 Δ5 (closes external review MED-1, 2026-05-24)** —
/// Match-but-empty-data_sign behavioral pivot vs pre-W12:
/// pre-W12 path classified empty `ack.data_sign` as `failure_class=
/// Internal, manual_recon=true` (Failed; manual recon required).  W12
/// `classify_check_result` routes empty data_sign on a non-empty id
/// match into `Kvt2ConfirmOutcome::Hold { reason:
/// LastChkDataSignEmpty }` per kvt2_confirm.rs:188-192 — transient
/// class, doc stays at Sent, next-tick re-probes.
///
/// Forensic chain: Envelope 1c-pre allocates trace row → DPS returns
/// ack(id=expected, data_sign=empty) → classify_check_result emits
/// `Hold(LastChkDataSignEmpty)` (NOT Acked — empty data_sign means DPS
/// matched the id but didn't carry KVT2 evidence) → Envelope 1c-hold
/// bundled (trace.complete RetryableServer + KVT2_CONFIRM_HOLD audit
/// at Severity::Warning) → `Ok(HoldFnDrain { HeldAtSent })`.  Doc
/// state UNCHANGED per W0b state-unchanged contract; drain halts on
/// this doc this tick; pending-drain shifts do NOT escalate.
#[tokio::test]
async fn c5b2_sent_replay_lastchk_match_empty_data_sign_holds_drain() {
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
        Some("DPS-FN-MATCH-EMPTY"),
    )
    .await;

    // lastChk returns OK з matching id but EMPTY data_sign — routed
    // to Hold(LastChkDataSignEmpty), NOT Acked.  Drain halts з
    // HoldFnDrain { HeldAtSent }; doc state untouched.
    let c = carriers(vec![], vec![Ok(ack("DPS-FN-MATCH-EMPTY", vec![]))]);
    let view = view_for(&c);

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    // W0b state-unchanged contract: doc stays SENT, no CAS fired.
    assert_eq!(read_doc_state(&pool, doc).await, "SENT");
    // HoldFnDrain projection: HeldAtSent (NOT ErRedriveQueued — only
    // NotFound routes through 1c-post Sent→ER; empty data_sign stays
    // at Sent for next-tick re-probe).
    assert_eq!(
        summary.held_at_sent(),
        1,
        "Hold(LastChkDataSignEmpty) MUST record HeldAtSent projection"
    );
    assert_eq!(
        summary.er_redrive_queued(),
        0,
        "empty data_sign is Hold (doc stays Sent), not Sent→ER downgrade"
    );
    assert_eq!(summary.advanced_to_kvt1(), 0);
    assert_eq!(summary.advanced_to_ack(), 0);
    assert_eq!(c.dps.send_chk_count(), 0, "no wire-resend in this tick");
    assert_eq!(c.dps.last_chk_count(), 1);

    // Pre-W12 OFFLINE_DRAIN_DOC_FAILED MUST NOT fire (no per-doc
    // failure on transient Hold path); KVT2_CONFIRM_HOLD audit at
    // Severity::Warning emitted by Envelope 1c-hold bundled.
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_FAILED").await, 0);
    assert_eq!(audit_count(&pool, "KVT2_CONFIRM_HOLD").await, 1);

    let payload = audit_latest_payload(&pool, "KVT2_CONFIRM_HOLD")
        .await
        .unwrap();
    assert_eq!(payload["source"], "sent_replay");
    assert_eq!(payload["hold_reason"], "LASTCHK_DATA_SIGN_EMPTY");
    assert_eq!(payload["dispatch_via"], "kvt2_confirm");
    assert!(
        payload["trace_attempt_no"].is_i64(),
        "1c-hold bundled payload MUST carry trace_attempt_no from 1c-pre allocation; \
         got: {payload:?}"
    );
}

// ─── Phase 1 REC-5 Δ: SentFresh empty data_sign → HoldFnDrain ────────

/// **M3b W12 Post-Closure Hardening Phase 1 / REC-5 (2026-05-24)** —
/// closes empty-data_sign Hold path test gap for SentFresh source.
/// Symmetric counterpart to:
///   - `c5b2_sent_replay_lastchk_match_empty_data_sign_holds_drain`
///     (SentReplay path; Δ5 from 5b.2)
///   - `c6_kvt1_reentry_match_empty_data_sign_holds_drain` (Kvt1Reentry
///     path; this Δ).
///
/// Routing chain: cohort walker picks OFFLINE_LOCAL_ACK doc → stage_send::
/// run transitions Sending→Sent з server_fiscal_no stamp →
/// confirm_drain_doc(SentFresh, attempt_no=Some(_)) → DPS lastChk returns
/// `ack(id=matching, data_sign=vec![])` → `classify_check_result` routes
/// to `Hold(LastChkDataSignEmpty)` per kvt2_confirm.rs:188-192 (NOT Acked
/// — empty data_sign means DPS matched id without KVT2 evidence) →
/// SentFresh Hold arm (`commit_hold_envelope_1c_hold_light` from Commit 6)
/// → `Ok(ConfirmDrainOutcome::HoldFnDrain { HeldAtSent })` → caller
/// projects to `DocVerdict::HoldFnDrain { Transport, HeldAtSent }` →
/// drain orchestrator break per W0b state-unchanged contract.
#[tokio::test]
async fn c6_sent_fresh_match_empty_data_sign_holds_drain() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    // OFFLINE_LOCAL_ACK so stage_send::run picks it up + transitions Sending→Sent.
    let doc = seed_doc_in_state(
        &pool,
        1,
        100,
        session_id,
        shift_id,
        "OFFLINE_LOCAL_ACK",
        None,
    )
    .await;

    // stage_send::run succeeds (Sent з server_fiscal_no stamp); lastChk
    // returns OK з matching id but EMPTY data_sign → Hold(LastChkDataSignEmpty)
    // → SentFresh 1c-hold-light → HoldFnDrain { HeldAtSent }.
    let c = carriers(
        vec![Ok(ack("DPS-FN-FRESH-EMPTY", vec![0xCD]))],
        vec![Ok(ack("DPS-FN-FRESH-EMPTY", vec![]))],
    );
    let view = view_for(&c);

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    // Doc lands in Sent (stage_send transitioned it; 1c-hold-light
    // does NOT CAS state per W0b state-unchanged contract).
    assert_eq!(read_doc_state(&pool, doc).await, "SENT");
    // HoldFnDrain projection per MED-PR70-R7-02: SentFresh → HeldAtSent
    // (NOT HeldAtKvt1; doc is Sent post-stage_send, Hold doesn't advance).
    assert_eq!(
        summary.held_at_sent(),
        1,
        "SentFresh empty-data_sign Hold MUST project HeldAtSent"
    );
    assert_eq!(
        summary.held_at_kvt1(),
        0,
        "SentFresh empty-data_sign Hold MUST NOT project HeldAtKvt1"
    );
    assert_eq!(
        summary.er_redrive_queued(),
        0,
        "empty data_sign is Hold (doc stays Sent), not Sent→ER downgrade"
    );
    assert_eq!(summary.advanced_to_kvt1(), 0);
    assert_eq!(summary.advanced_to_ack(), 0);
    // 1 send_chk (stage_send reached wire); 1 last_chk (confirm_drain_doc).
    assert_eq!(c.dps.send_chk_count(), 1, "stage_send reached wire");
    assert_eq!(c.dps.last_chk_count(), 1, "confirm_drain_doc reached wire");

    // KVT2_CONFIRM_HOLD audit Severity::Warning з source=sent_fresh +
    // hold_reason=LASTCHK_DATA_SIGN_EMPTY.
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_FAILED").await, 0);
    assert_eq!(audit_count(&pool, "KVT2_CONFIRM_HOLD").await, 1);
    let payload = audit_latest_payload(&pool, "KVT2_CONFIRM_HOLD")
        .await
        .unwrap();
    assert_eq!(payload["source"], "sent_fresh");
    assert_eq!(payload["hold_reason"], "LASTCHK_DATA_SIGN_EMPTY");
    assert_eq!(payload["dispatch_via"], "kvt2_confirm");
}

// ─── Phase 1 REC-5 Δ: Kvt1Reentry empty data_sign → HoldFnDrain ──────

/// **M3b W12 Post-Closure Hardening Phase 1 / REC-5 (2026-05-24)** —
/// closes empty-data_sign Hold path test gap for Kvt1Reentry source.
/// Completes the empty-data_sign Hold coverage matrix:
///   - SentReplay: `c5b2_sent_replay_lastchk_match_empty_data_sign_holds_drain` (Δ5)
///   - SentFresh: `c6_sent_fresh_match_empty_data_sign_holds_drain` (this Δ)
///   - Kvt1Reentry: this test
///
/// Routing chain: cohort walker picks KVT1 doc з persisted
/// server_fiscal_no → `process_via_w12_only` → confirm_drain_doc(
/// Kvt1Reentry, attempt_no=None) → DPS lastChk returns `ack(id=matching,
/// data_sign=vec![])` → `classify_check_result` → `Hold(LastChkDataSignEmpty)`
/// → Kvt1Reentry Hold arm → `Ok(HoldFnDrain { HeldAtKvt1 })` per
/// MED-PR70-R7-02 projection matrix → caller projects to `DocVerdict::
/// HoldFnDrain { Transport, HeldAtKvt1 }` → drain orchestrator break.
#[tokio::test]
async fn c6_kvt1_reentry_match_empty_data_sign_holds_drain() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    // KVT1 з persisted server_fiscal_no (stage_send 4-b invariant —
    // Kvt1 ALWAYS implies prior tick stamped it).
    let doc = seed_doc_in_state(
        &pool,
        1,
        100,
        session_id,
        shift_id,
        "KVT1",
        Some("DPS-FN-KVT1-EMPTY"),
    )
    .await;
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

    // lastChk returns OK з matching id but EMPTY data_sign →
    // Hold(LastChkDataSignEmpty) → Kvt1Reentry 1c-hold-light →
    // HoldFnDrain { HeldAtKvt1 }.  No stage_send (Kvt1 dispatch skips).
    let c = carriers(vec![], vec![Ok(ack("DPS-FN-KVT1-EMPTY", vec![]))]);
    let view = view_for(&c);

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    // Doc state UNCHANGED at KVT1 per W0b.  1c-hold-light не CAS state.
    assert_eq!(read_doc_state(&pool, doc).await, "KVT1");
    // HoldFnDrain projection per MED-PR70-R7-02: Kvt1Reentry → HeldAtKvt1
    // (NOT HeldAtSent — doc stays at Kvt1 from prior tick's advance).
    assert_eq!(
        summary.held_at_kvt1(),
        1,
        "Kvt1Reentry empty-data_sign Hold MUST project HeldAtKvt1"
    );
    assert_eq!(
        summary.held_at_sent(),
        0,
        "Kvt1Reentry empty-data_sign Hold MUST NOT project HeldAtSent (doc Kvt1, не Sent)"
    );
    assert_eq!(summary.er_redrive_queued(), 0);
    assert_eq!(summary.advanced_to_kvt1(), 0);
    assert_eq!(summary.advanced_to_ack(), 0);
    // No wire send (Kvt1 dispatch skips stage_send); 1 last_chk.
    assert_eq!(c.dps.send_chk_count(), 0);
    assert_eq!(c.dps.last_chk_count(), 1);

    // KVT2_CONFIRM_HOLD audit з source=kvt1_reentry +
    // hold_reason=LASTCHK_DATA_SIGN_EMPTY.
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_FAILED").await, 0);
    assert_eq!(audit_count(&pool, "KVT2_CONFIRM_HOLD").await, 1);
    let payload = audit_latest_payload(&pool, "KVT2_CONFIRM_HOLD")
        .await
        .unwrap();
    assert_eq!(payload["source"], "kvt1_reentry");
    assert_eq!(payload["hold_reason"], "LASTCHK_DATA_SIGN_EMPTY");
    assert_eq!(payload["dispatch_via"], "kvt2_confirm");
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

// ─── M3b W12 Commit 5: Kvt1Reentry integration fixtures ─────────────
//
// Plan §411 acceptance for Commit 5: Kvt1Reentry end-to-end scenarios
// covering the source-context routing matrix for the `process_via_
// w12_only` → `confirm_drain_doc(Kvt1Reentry)` chain.
//
// 1. NotFound → Drift (DPS empty id) → BootError + drift audit.
// 2. Mismatch → Drift (DPS different id) → BootError + drift audit.
// 3. Transport → Hold → BootError + hold audit (HoldFnDrain
//    projection deferred to Commit 6).
// 4. ServerFiscalNoMissing → caller-level BootError (before DPS).

#[tokio::test]
async fn w12_kvt1_reentry_not_found_emits_drift_audit_and_halts_via_boot_error() {
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
        Some("DPS-FN-NOT-FOUND"),
    )
    .await;
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

    // DPS returns empty id → NotFound → StructuralDrift::
    // NotFoundOutsideSentReplay → Envelope 1c-drift-light audit
    // → BootError.
    let c = carriers(vec![], vec![Ok(ack("", vec![]))]);
    let view = view_for(&c);

    let err = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .expect_err("Kvt1Reentry NotFound MUST halt drain via BootError::Internal");
    let err_str = err.to_string();
    assert!(
        err_str.contains("structural drift") || err_str.contains("STRUCTURAL_DRIFT"),
        "BootError must mention structural drift; got: {err_str}"
    );

    // Doc state UNCHANGED at KVT1 — Envelope 1b never fired.
    assert_eq!(read_doc_state(&pool, doc).await, "KVT1");

    // Drift envelope audit landed BEFORE BootError.
    assert_eq!(audit_count(&pool, "KVT2_CONFIRM_STRUCTURAL_DRIFT").await, 1);
    let drift = audit_latest_payload(&pool, "KVT2_CONFIRM_STRUCTURAL_DRIFT")
        .await
        .unwrap();
    assert_eq!(drift["source"], "kvt1_reentry");
    assert_eq!(drift["drift_reason"], "NOT_FOUND_OUTSIDE_SENT_REPLAY");
    assert_eq!(drift["dispatch_via"], "kvt2_confirm");

    // No Envelope 1b / Envelope 2 fired.
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await, 0);
    assert_eq!(audit_count(&pool, "STAGE_FINALIZE_ACK").await, 0);
}

#[tokio::test]
async fn w12_kvt1_reentry_mismatch_emits_drift_audit_and_halts_via_boot_error() {
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
        Some("EXPECTED-KVT1"),
    )
    .await;
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

    // DPS returns different id → ServerFiscalIdMismatch →
    // StructuralDrift::LastChkIdMismatch → drift audit + BootError.
    let c = carriers(vec![], vec![Ok(ack("DIFFERENT-KVT1", vec![0xAAu8; 32]))]);
    let view = view_for(&c);

    let err = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .expect_err("Kvt1Reentry Mismatch MUST halt drain via BootError::Internal");
    let err_str = err.to_string();
    assert!(
        err_str.contains("structural drift") || err_str.contains("STRUCTURAL_DRIFT"),
        "BootError must mention structural drift; got: {err_str}"
    );

    assert_eq!(read_doc_state(&pool, doc).await, "KVT1");
    assert_eq!(audit_count(&pool, "KVT2_CONFIRM_STRUCTURAL_DRIFT").await, 1);
    let drift = audit_latest_payload(&pool, "KVT2_CONFIRM_STRUCTURAL_DRIFT")
        .await
        .unwrap();
    assert_eq!(drift["source"], "kvt1_reentry");
    assert_eq!(drift["drift_reason"], "LASTCHK_ID_MISMATCH");
    let detail = drift["drift_reason_detail"]
        .as_str()
        .expect("drift_reason_detail must be a string");
    assert!(
        detail.contains("DIFFERENT-KVT1") && detail.contains("EXPECTED-KVT1"),
        "drift detail must carry observed+expected pair; got: {detail}"
    );

    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await, 0);
    assert_eq!(audit_count(&pool, "STAGE_FINALIZE_ACK").await, 0);
}

#[tokio::test]
async fn w12_kvt1_reentry_dps_transport_holds_drain_and_projects_held_at_kvt1() {
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
        Some("DPS-FN-HOLD"),
    )
    .await;
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

    // DPS Transport error → Hold(DpsTransport) → Envelope
    // 1c-hold-light audit. Commit 6 projects Ok(HoldFnDrain { HeldAtKvt1 }).
    let c = carriers(
        vec![],
        vec![Err(DpsError::Transport(
            "simulated kvt1 lastChk timeout".into(),
        ))],
    );
    let view = view_for(&c);

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    // Doc state UNCHANGED at KVT1.
    assert_eq!(read_doc_state(&pool, doc).await, "KVT1");

    // HoldFnDrain projection: HeldAtKvt1 (NOT HeldAtSent) per MED-PR70-R7-02.
    assert_eq!(
        summary.held_at_kvt1(),
        1,
        "Kvt1Reentry Hold MUST project HeldAtKvt1"
    );
    assert_eq!(
        summary.held_at_sent(),
        0,
        "Kvt1Reentry Hold MUST NOT project HeldAtSent"
    );
    assert_eq!(summary.er_redrive_queued(), 0);

    assert_eq!(audit_count(&pool, "KVT2_CONFIRM_HOLD").await, 1);
    let hold = audit_latest_payload(&pool, "KVT2_CONFIRM_HOLD")
        .await
        .unwrap();
    assert_eq!(hold["source"], "kvt1_reentry");
    assert_eq!(hold["hold_reason"], "DPS_TRANSPORT");
    assert_eq!(hold["dispatch_via"], "kvt2_confirm");
    let detail = hold["hold_reason_detail"]
        .as_str()
        .expect("hold_reason_detail must be a string");
    assert!(
        detail.contains("simulated kvt1 lastChk timeout"),
        "hold detail must carry DPS error message; got: {detail}"
    );

    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await, 0);
    assert_eq!(audit_count(&pool, "KVT2_CONFIRM_STRUCTURAL_DRIFT").await, 0);
}

#[tokio::test]
async fn w12_kvt1_reentry_doc_without_server_fiscal_no_emits_drift_audit_and_halts() {
    // MED-PR70-R11-01 caller-level handoff: Kvt1 state implies
    // stage_send 4-b stamped server_fiscal_no.  Doc at Kvt1 with
    // NULL server_fiscal_no is a state-machine invariant breach
    // detected at `process_via_w12_only` entry BEFORE confirm_drain_doc
    // / DPS call.
    //
    // **MED-W12C5-01 fix (5 Δ, 2026-05-22)**: durable
    // KVT2_CONFIRM_STRUCTURAL_DRIFT audit emitted via
    // Envelope 1c-drift-light BEFORE BootError::Internal halt —
    // forensic operators see the structural breach in audit_log,
    // not just the error string.
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let doc = seed_doc_in_state(
        &pool, 1, 100, session_id, shift_id, "KVT1",
        // server_fiscal_no = None — invariant breach for KVT1 state.
        None,
    )
    .await;
    common::init_chain_seed(&pool, FN, common::chain_anchor(0x00))
        .await
        .unwrap();

    let c = carriers(vec![], vec![]);
    let view = view_for(&c);

    let err = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .expect_err("KVT1 without server_fiscal_no MUST halt with BootError::Internal");
    let err_str = err.to_string();
    assert!(
        err_str.contains("NULL server_fiscal_no") || err_str.contains("stamp invariant breach"),
        "BootError must mention server_fiscal_no invariant; got: {err_str}"
    );

    // Doc state unchanged at KVT1; no DPS call (caller-level fail
    // BEFORE confirm_drain_doc invocation).
    assert_eq!(read_doc_state(&pool, doc).await, "KVT1");
    assert_eq!(c.dps.send_chk_count(), 0);
    assert_eq!(c.dps.last_chk_count(), 0);

    // **MED-W12C5-01: durable forensic audit landed BEFORE BootError**:
    // KVT2_CONFIRM_STRUCTURAL_DRIFT (Severity::Error) with
    // drift_reason=SERVER_FISCAL_NO_MISSING + source=kvt1_reentry.
    assert_eq!(
        audit_count(&pool, "KVT2_CONFIRM_STRUCTURAL_DRIFT").await,
        1,
        "drift envelope MUST emit forensic audit BEFORE fail-loud"
    );
    let drift = audit_latest_payload(&pool, "KVT2_CONFIRM_STRUCTURAL_DRIFT")
        .await
        .unwrap();
    assert_eq!(drift["source"], "kvt1_reentry");
    assert_eq!(drift["drift_reason"], "SERVER_FISCAL_NO_MISSING");
    assert_eq!(drift["dispatch_via"], "kvt2_confirm");
    // **LOW-W12C5-Δ2-A fix (5 Δ3, 2026-05-22)**: lock per-variant
    // contextual `drift_reason_detail` content for ServerFiscalNoMissing
    // (OBS-W12C5-1 closure).  Future regression to `detail_message()`
    // that wipes the message OR reverts to literal variant-name would
    // be caught by this assertion.
    let detail = drift["drift_reason_detail"]
        .as_str()
        .expect("drift_reason_detail must be a string");
    assert!(
        detail.contains("Kvt1 with NULL server_fiscal_no")
            && detail.contains("stage_send 4-b stamp invariant breach"),
        "OBS-W12C5-1: contextual detail message MUST describe the \
         specific invariant breach (Kvt1 + NULL server_fiscal_no + \
         stage_send 4-b stamp); got: {detail}"
    );

    // No advance / hold audit on this caller-level structural drift.
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await, 0);
    assert_eq!(audit_count(&pool, "KVT2_CONFIRM_HOLD").await, 0);
}

// ─── Test 10: SentFresh Hold projection → HeldAtSent ─────────────────

/// **M3b W12 Commit 6 (MED-PR70-R7-02, 2026-05-24)** — Integration test
/// for SentFresh Hold projection.  When a doc at stage_send::run successfully
/// submits to the wire (state transitions OFFLINE_LOCAL_ACK → SENT) but
/// the subsequent W12 pre-flight confirm_drain_doc(SentFresh) encounters
/// a transient DPS error (e.g. Transport timeout), the drain MUST NOT
/// crash with BootError::Internal.
///
/// Instead, the doc is held at SENT (W0b state-unchanged contract),
/// and the drain projects `HeldAtSent` (summary.held_at_sent() == 1,
/// held_at_kvt1() == 0).  A transient KVT2_CONFIRM_HOLD audit row is
/// emitted with source=sent_fresh.
#[tokio::test]
async fn c6_sent_fresh_hold_emits_hold_audit_and_projects_held_at_sent() {
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
        "OFFLINE_LOCAL_ACK",
        None,
    )
    .await;

    // stage_send::run succeeds (returns Ok(Sent)), but confirm_drain_doc
    // encounters DpsError::Transport -> Hold(DpsTransport).
    // projects Ok(HoldFnDrain { HeldAtSent }).
    let c = carriers(
        vec![Ok(ack("DPS-FN-HOLD", vec![0xCD]))],
        vec![Err(DpsError::Transport(
            "simulated fresh lastChk timeout".into(),
        ))],
    );
    let view = view_for(&c);

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    // Doc remains in SENT (stage_send transitioned it, but Hold prevented Ack).
    assert_eq!(read_doc_state(&pool, doc).await, "SENT");

    // HoldFnDrain projection: HeldAtSent (NOT HeldAtKvt1) per MED-PR70-R7-02.
    assert_eq!(
        summary.held_at_sent(),
        1,
        "SentFresh Hold MUST project HeldAtSent"
    );
    assert_eq!(
        summary.held_at_kvt1(),
        0,
        "SentFresh Hold MUST NOT project HeldAtKvt1"
    );
    assert_eq!(summary.er_redrive_queued(), 0);

    assert_eq!(c.dps.send_chk_count(), 1, "stage_send reached wire");
    assert_eq!(c.dps.last_chk_count(), 1, "confirm_drain_doc reached wire");

    assert_eq!(audit_count(&pool, "KVT2_CONFIRM_HOLD").await, 1);
    let hold = audit_latest_payload(&pool, "KVT2_CONFIRM_HOLD")
        .await
        .unwrap();
    assert_eq!(hold["source"], "sent_fresh");
    assert_eq!(hold["hold_reason"], "DPS_TRANSPORT");
    assert_eq!(hold["dispatch_via"], "kvt2_confirm");
    let detail = hold["hold_reason_detail"]
        .as_str()
        .expect("hold_reason_detail must be a string");
    assert!(
        detail.contains("simulated fresh lastChk timeout"),
        "hold detail must carry DPS error message; got: {detail}"
    );

    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await, 0);
    assert_eq!(audit_count(&pool, "KVT2_CONFIRM_STRUCTURAL_DRIFT").await, 0);
}

// ─── Phase 2a.1 Commit 6.1.2 REC-1: Tier 1+2 trigger tests ───────────

/// Helper — read `consecutive_holds` counter from `fiscal_documents`.
async fn read_consecutive_holds(pool: &SqlitePool, doc_id: DocumentId) -> i64 {
    sqlx::query_scalar("SELECT consecutive_holds FROM fiscal_documents WHERE document_id = ?")
        .bind(doc_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Helper — read `node_state.mode` for FN.
async fn read_node_mode_state_dispatch(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Helper — single Kvt1Reentry Hold drain tick: doc stays Kvt1, counter +1.
async fn run_kvt1_hold_tick(pool: &SqlitePool) {
    let c = carriers(
        vec![],
        vec![Err(DpsError::Transport(
            "simulated kvt1 lastChk timeout".into(),
        ))],
    );
    let view = view_for(&c);
    let _ = backlog_drain::drain(&common::drain_test_guard(), pool, &view, FN)
        .await
        .unwrap();
}

/// **REC-1 Test 1 (Increment & Reset)**: послідовні Holds збільшують
/// counter +1 кожного тіку; перший Advance скидає в 0 atomically per
/// Envelope 1b reset semantics.
#[tokio::test]
async fn c612_consecutive_holds_increment_on_hold_reset_on_advance() {
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
        Some("DPS-FN-INCR"),
    )
    .await;
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

    // Initial: counter == 0 (DDL DEFAULT 0).
    assert_eq!(read_consecutive_holds(&pool, doc).await, 0);

    // 3 consecutive Hold ticks → counter == 3.
    for expected in 1..=3 {
        run_kvt1_hold_tick(&pool).await;
        assert_eq!(
            read_consecutive_holds(&pool, doc).await,
            expected,
            "after {expected} consecutive Hold ticks counter MUST equal {expected}"
        );
    }

    // Doc state UNCHANGED at KVT1 per W0b.
    assert_eq!(read_doc_state(&pool, doc).await, "KVT1");

    // Reset on first Advance: DPS lastChk returns ack з matching id +
    // KVT1 evidence → confirm_drain_doc(Kvt1Reentry, Acked) →
    // Envelope 1b (3-write) → counter resets to 0.
    let c = carriers(vec![], vec![Ok(ack("DPS-FN-INCR", vec![0xAAu8; 32]))]);
    let view = view_for(&c);
    let _ = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    // Doc reached ACK (via Envelope 1b + Envelope 2).
    assert_eq!(read_doc_state(&pool, doc).await, "ACK");
    // Counter reset to 0 by 1b's reset_consecutive_holds_tx call.
    assert_eq!(
        read_consecutive_holds(&pool, doc).await,
        0,
        "advance to Ack MUST atomically reset consecutive_holds via Envelope 1b"
    );

    // Tier 1 / Tier 2 audits MUST NOT fire (counter never reached threshold).
    assert_eq!(audit_count(&pool, "KVT2_CONFIRM_PROLONGED_HOLD").await, 0);
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_FN_STOP_MODE").await, 0);
}

/// **REC-1 Test 2 (Tier 1 Trigger)**: 10 consecutive Holds → first
/// `KVT2_CONFIRM_PROLONGED_HOLD` audit at Severity::Warning, no state
/// mutation.  Counter continues incrementing past 10 — each subsequent
/// tick re-fires Tier 1 audit (intended: operator sees ongoing
/// degradation signal per-tick).
#[tokio::test]
async fn c612_tier_1_prolonged_hold_audit_fires_at_10_consecutive_holds() {
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
        Some("DPS-FN-TIER1"),
    )
    .await;
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

    // 9 Hold ticks — counter == 9, NO Tier 1 audit.
    for _ in 0..9 {
        run_kvt1_hold_tick(&pool).await;
    }
    assert_eq!(read_consecutive_holds(&pool, doc).await, 9);
    assert_eq!(
        audit_count(&pool, "KVT2_CONFIRM_PROLONGED_HOLD").await,
        0,
        "Tier 1 MUST NOT fire at counter < 10"
    );

    // 10th Hold tick — counter == 10, Tier 1 audit fires.
    run_kvt1_hold_tick(&pool).await;
    assert_eq!(read_consecutive_holds(&pool, doc).await, 10);
    assert_eq!(
        audit_count(&pool, "KVT2_CONFIRM_PROLONGED_HOLD").await,
        1,
        "Tier 1 MUST fire at counter >= 10"
    );

    // Node state UNCHANGED (Tier 1 = audit-only).
    assert_eq!(read_node_mode_state_dispatch(&pool).await, "GOING_ONLINE");
    // Doc state UNCHANGED at KVT1.
    assert_eq!(read_doc_state(&pool, doc).await, "KVT1");
    // Tier 2 NOT fired.
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_FN_STOP_MODE").await, 0);

    // Audit payload structure verification.
    let payload = audit_latest_payload(&pool, "KVT2_CONFIRM_PROLONGED_HOLD")
        .await
        .unwrap();
    assert_eq!(payload["tier"], 1);
    assert_eq!(payload["tier_threshold"], 10);
    assert_eq!(payload["consecutive_holds"], 10);
    assert_eq!(payload["projection"], "HeldAtKvt1");
}

/// **REC-1 Test 3 (Tier 2 Trigger)**: 50 consecutive Holds → FN goes
/// to STOP_MODE atomically with Critical `OFFLINE_DRAIN_FN_STOP_MODE`
/// audit.  Tier 1 audit accumulated на 40 prior ticks (10..=49).
#[tokio::test]
async fn c612_tier_2_stop_mode_escalation_fires_at_50_consecutive_holds() {
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
        Some("DPS-FN-TIER2"),
    )
    .await;
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

    // 49 Hold ticks — counter == 49, Tier 1 firing з tick 10 onwards
    // (40 Tier-1 audits = ticks 10..=49).
    for _ in 0..49 {
        run_kvt1_hold_tick(&pool).await;
    }
    assert_eq!(read_consecutive_holds(&pool, doc).await, 49);
    assert_eq!(
        audit_count(&pool, "KVT2_CONFIRM_PROLONGED_HOLD").await,
        40,
        "Tier 1 fires every tick from 10 to 49 (40 ticks)"
    );
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_FN_STOP_MODE").await,
        0,
        "Tier 2 MUST NOT fire at counter < 50"
    );
    assert_eq!(read_node_mode_state_dispatch(&pool).await, "GOING_ONLINE");

    // 50th Hold tick — counter == 50, Tier 2 fires (NOT Tier 1; Tier
    // ordering checks Tier 2 first via if/else).
    run_kvt1_hold_tick(&pool).await;
    assert_eq!(read_consecutive_holds(&pool, doc).await, 50);
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_FN_STOP_MODE").await,
        1,
        "Tier 2 MUST fire at counter >= 50"
    );
    // Tier 1 NOT re-fired on this tick (Tier 2 took precedence).
    assert_eq!(
        audit_count(&pool, "KVT2_CONFIRM_PROLONGED_HOLD").await,
        40,
        "Tier 1 MUST NOT fire on Tier-2-triggering tick (mutually \
         exclusive per orchestrator if/else)"
    );

    // **CRITICAL ASSERTION**: FN now in STOP_MODE persistently.
    assert_eq!(read_node_mode_state_dispatch(&pool).await, "STOP_MODE");
    // Doc state UNCHANGED at KVT1 (STOP_MODE doesn't touch docs; only
    // gates new ingress).
    assert_eq!(read_doc_state(&pool, doc).await, "KVT1");

    // Tier 2 audit payload structure verification.
    let payload = audit_latest_payload(&pool, "OFFLINE_DRAIN_FN_STOP_MODE")
        .await
        .unwrap();
    assert_eq!(payload["tier"], 2);
    assert_eq!(payload["tier_threshold"], 50);
    assert_eq!(payload["consecutive_holds"], 50);
    assert_eq!(payload["node_mode_target"], "STOP_MODE");
    assert_eq!(payload["fiscal_number"], FN);
}

// ─── Phase 2b Commit 6.2 REC-6: granular FailureClass per Hold reason ──

/// Test fixture builder: seed Kvt1 doc з prereqs, run drain з one
/// specific DPS error, return the doc id for downstream assertions.
async fn drive_kvt1_reentry_hold_with_dps_error(
    pool: &SqlitePool,
    expected_id: &str,
    dps_err: DpsError,
) -> DocumentId {
    seed_node_state(pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(pool).await;
    let session_id = seed_offline_session(pool, OfflineSessionState::Open).await;
    let doc = seed_doc_in_state(
        pool,
        1,
        100,
        session_id,
        shift_id,
        "KVT1",
        Some(expected_id),
    )
    .await;
    common::init_chain_seed(pool, FN, common::chain_anchor(0x00))
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(
        pool,
        FN,
        doc,
        common::chain_anchor(0x00),
        common::chain_anchor(0x01),
    )
    .await
    .unwrap();
    let c = carriers(vec![], vec![Err(dps_err)]);
    let view = view_for(&c);
    let _ = backlog_drain::drain(&common::drain_test_guard(), pool, &view, FN)
        .await
        .unwrap();
    doc
}

/// **REC-6 integration Test 1**: DpsError::Server →
/// Kvt2ConfirmHoldReason::DpsServer → FailureClass::Server.
/// Audit payload `hold_reason="DPS_SERVER"`.
#[tokio::test]
async fn c62_granular_failure_class_server() {
    let (_d, pool) = fresh_pool().await;
    let _doc = drive_kvt1_reentry_hold_with_dps_error(
        &pool,
        "DPS-FN-SERVER",
        DpsError::Server {
            code: -5,
            message: "simulated server error".into(),
        },
    )
    .await;
    assert_eq!(audit_count(&pool, "KVT2_CONFIRM_HOLD").await, 1);
    let payload = audit_latest_payload(&pool, "KVT2_CONFIRM_HOLD")
        .await
        .unwrap();
    assert_eq!(payload["source"], "kvt1_reentry");
    assert_eq!(
        payload["hold_reason"], "DPS_SERVER",
        "DpsError::Server MUST map to hold_reason=DPS_SERVER"
    );
}

/// **REC-6 integration Test 2**: DpsError::Authorization →
/// Kvt2ConfirmHoldReason::DpsAuthorization → FailureClass::Authorization.
/// Audit payload `hold_reason="DPS_AUTHORIZATION"`.
#[tokio::test]
async fn c62_granular_failure_class_auth() {
    let (_d, pool) = fresh_pool().await;
    let _doc = drive_kvt1_reentry_hold_with_dps_error(
        &pool,
        "DPS-FN-AUTH",
        DpsError::Authorization {
            code: -1,
            kind: AuthorizationKind::DocumentReject,
            message: "simulated auth reject".into(),
        },
    )
    .await;
    assert_eq!(audit_count(&pool, "KVT2_CONFIRM_HOLD").await, 1);
    let payload = audit_latest_payload(&pool, "KVT2_CONFIRM_HOLD")
        .await
        .unwrap();
    assert_eq!(
        payload["hold_reason"], "DPS_AUTHORIZATION",
        "DpsError::Authorization MUST map to hold_reason=DPS_AUTHORIZATION"
    );
}

/// **REC-6 integration Test 3**: DpsError::Decode →
/// Kvt2ConfirmHoldReason::DpsDecode → FailureClass::Decode.
/// Audit payload `hold_reason="DPS_DECODE"`.
#[tokio::test]
async fn c62_granular_failure_class_decode() {
    let (_d, pool) = fresh_pool().await;
    let _doc = drive_kvt1_reentry_hold_with_dps_error(
        &pool,
        "DPS-FN-DECODE",
        DpsError::Decode("malformed protobuf".into()),
    )
    .await;
    assert_eq!(audit_count(&pool, "KVT2_CONFIRM_HOLD").await, 1);
    let payload = audit_latest_payload(&pool, "KVT2_CONFIRM_HOLD")
        .await
        .unwrap();
    assert_eq!(
        payload["hold_reason"], "DPS_DECODE",
        "DpsError::Decode MUST map to hold_reason=DPS_DECODE"
    );
}

/// **REC-6 integration Test 4**: Match + empty data_sign →
/// Kvt2ConfirmHoldReason::LastChkDataSignEmpty → FailureClass::Internal.
/// Audit payload `hold_reason="LASTCHK_DATA_SIGN_EMPTY"`.  Driven
/// з Ok(ack) (NOT Err) since this Hold variant only fires on
/// successful DPS response з empty data_sign.
#[tokio::test]
async fn c62_granular_failure_class_data_sign_empty() {
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
        Some("DPS-FN-EMPTY"),
    )
    .await;
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
    // Match + empty data_sign → Hold(LastChkDataSignEmpty).
    let c = carriers(vec![], vec![Ok(ack("DPS-FN-EMPTY", vec![]))]);
    let view = view_for(&c);
    let _ = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();
    assert_eq!(audit_count(&pool, "KVT2_CONFIRM_HOLD").await, 1);
    let payload = audit_latest_payload(&pool, "KVT2_CONFIRM_HOLD")
        .await
        .unwrap();
    assert_eq!(
        payload["hold_reason"], "LASTCHK_DATA_SIGN_EMPTY",
        "Match + empty data_sign MUST map to hold_reason=LASTCHK_DATA_SIGN_EMPTY"
    );
    // Doc state UNCHANGED at KVT1 per W0b.
    assert_eq!(read_doc_state(&pool, doc).await, "KVT1");
    // Counter incremented to 1.
    assert_eq!(read_consecutive_holds(&pool, doc).await, 1);
}

// ─── RS-3 A2.1a — runtime-neutral confirmer extraction guards ────────

/// **RS-3 A2.1a (2026-06-09) — MED-PR70-R11-01 non-identity guard
/// (the #1 silent-drift guard for the confirmer extraction).**
///
/// The `OFFLINE_DRAIN_KVT2_ADVANCED` audit `server_fiscal_no` MUST come
/// from the value the caller HANDS IN to
/// [`kvt2_advance::advance_to_ack`] (ultimately
/// `StageSendOutcome::Sent.server_fiscal_no`, stamped fresh this tick),
/// NOT from the doc's own persisted `fiscal_documents.server_fiscal_no`
/// cohort snapshot.  We seed those two to DIFFER and assert the audit
/// carries the hand-in value — a regression that re-read
/// `fiscal_documents.server_fiscal_no` (or the cohort snapshot) would
/// write the seeded-stale value and fail.
///
/// Also pins the `from_state` decoupling: the doc is at DB state `Sent`
/// (so the CAS is Sent→Kvt1→Kvt2), but we pass `doc_state_at_entry =
/// ErrorRetryable` (the cohort-walker snapshot label).  The audit
/// `from_state` MUST be the passed label, not the DB CAS state — matching
/// the pre-extraction LOW-PR70-R12-02 cohort-entry convention.
#[tokio::test]
async fn a2_1a_advance_to_ack_audit_server_fiscal_no_is_handin_not_persisted() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;

    // Persisted value = the WRONG value a regression would read; hand-in
    // value = the RIGHT (fresh) value.  They MUST differ.
    const PERSISTED_WRONG: &str = "DPS-FN-PERSISTED-WRONG";
    const HANDIN_RIGHT: &str = "DPS-FN-HANDIN-RIGHT";
    let doc = seed_doc_in_state(
        &pool,
        1,
        100,
        session_id,
        shift_id,
        "SENT",
        Some(PERSISTED_WRONG),
    )
    .await;
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

    let kvt1_raw = vec![0xAAu8; 32];
    let expected_digest_hex = format!("{:x}", Sha256::digest(&kvt1_raw));

    // Direct call into the extracted runtime-neutral confirmer: the
    // hand-in `server_fiscal_no` DIFFERS from the persisted column; the
    // entry label is the cohort snapshot (ERROR_RETRYABLE) while the DB
    // state is SENT (Sent-entry → Envelope 1a, CAS Sent→Kvt1→Kvt2).
    kvt2_advance::advance_to_ack(
        &pool,
        doc,
        kvt1_raw.clone(),
        HANDIN_RIGHT,
        DocState::ErrorRetryable,
        Some(7),
    )
    .await
    .expect("advance_to_ack drives Sent→Kvt1→Kvt2→Ack");

    // Doc reaches terminal ACK (Envelope 1a + Envelope 2).
    assert_eq!(read_doc_state(&pool, doc).await, "ACK");

    // Audit chain: KVT2_ADVANCED (Envelope 1a) + STAGE_FINALIZE_ACK
    // (Envelope 2).
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await, 1);
    assert_eq!(audit_count(&pool, "STAGE_FINALIZE_ACK").await, 1);

    let payload = audit_latest_payload(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED")
        .await
        .unwrap();
    // THE GUARD — audit server_fiscal_no == hand-in, NOT persisted.
    assert_eq!(
        payload["server_fiscal_no"], HANDIN_RIGHT,
        "audit server_fiscal_no MUST be the hand-in value \
         (StageSendOutcome::Sent), NOT fiscal_documents.server_fiscal_no \
         ({PERSISTED_WRONG}); got: {payload:?}"
    );
    assert_ne!(
        payload["server_fiscal_no"], PERSISTED_WRONG,
        "regression reading the persisted/cohort server_fiscal_no must fail"
    );
    // from_state decoupling — the passed cohort-snapshot label, NOT the
    // DB CAS state (SENT).
    assert_eq!(payload["from_state"], "ERROR_RETRYABLE");
    assert_eq!(payload["to_state"], "KVT2");
    assert_eq!(payload["dispatch_via"], "kvt2_confirm");
    assert_eq!(payload["evidence_source"], "lastChk");
    // attempt_no threaded through the Sent-entry envelope.
    assert_eq!(payload["attempt_no"].as_i64(), Some(7));
    // Kvt1Raw persisted byte-for-byte + sha256 audit cross-link.
    let persisted_kvt1_raw: Vec<u8> = sqlx::query_scalar(
        "SELECT content FROM document_files WHERE document_id = ? AND kind = 'KVT1_RAW'",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .expect("Envelope 1a MUST persist KVT1_RAW row");
    assert_eq!(persisted_kvt1_raw, kvt1_raw);
    assert_eq!(payload["kvt1_raw_sha256_hex"], expected_digest_hex);
}

/// **RS-3 A2.1a (2026-06-09)** — Kvt1-entry path: `advance_to_ack` with
/// `doc_state_at_entry = Kvt1` drives Envelope 1b (2-CAS, NO Sent→Kvt1)
/// + finalize to ACK, with the audit `from_state = KVT1` and NO
/// `attempt_no` field (no fresh wire attempt) — matching the pre-A2.1a
/// Kvt1Reentry envelope contract.
#[tokio::test]
async fn a2_1a_advance_to_ack_kvt1_entry_drives_2cas_to_ack() {
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
        Some("DPS-FN-KVT1-ENTRY"),
    )
    .await;
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

    let kvt1_raw = vec![0xBBu8; 16];
    kvt2_advance::advance_to_ack(
        &pool,
        doc,
        kvt1_raw.clone(),
        "DPS-FN-KVT1-ENTRY",
        DocState::Kvt1,
        // attempt_no ignored on the Kvt1-entry path; pass Some to prove it
        // is NOT leaked into the 1b audit payload.
        Some(9),
    )
    .await
    .expect("advance_to_ack Kvt1-entry drives Kvt1→Kvt2→Ack");

    assert_eq!(read_doc_state(&pool, doc).await, "ACK");
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await, 1);
    assert_eq!(audit_count(&pool, "STAGE_FINALIZE_ACK").await, 1);

    let payload = audit_latest_payload(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED")
        .await
        .unwrap();
    assert_eq!(payload["from_state"], "KVT1");
    assert_eq!(payload["to_state"], "KVT2");
    assert_eq!(payload["server_fiscal_no"], "DPS-FN-KVT1-ENTRY");
    assert_eq!(payload["dispatch_via"], "kvt2_confirm");
    // Kvt1-entry envelope OMITS the attempt_no key entirely (the signed
    // contract is key-absence, distinct from present-as-JSON-null — a
    // future regression emitting attempt_no:null must fail this).
    assert!(
        payload.get("attempt_no").is_none(),
        "Kvt1-entry (Envelope 1b) payload MUST OMIT the attempt_no key (not null); got: {payload:?}"
    );
}

/// **RS-3 A2.1a follow-up (TEST-3)** — Envelope 1a CAS-miss maps to
/// `ConfirmError::Database` (which the drain boundary maps to
/// `BootError::ReconciliationFailed`).  Pins the variant taxonomy the
/// future A2.1b online mapper depends on (Database = transient/recoverable
/// vs StructuralDrift = terminal breach).  Drives the 1a path
/// (`doc_state_at_entry=ErrorRetryable`, an allowed non-Kvt1 label) against
/// a doc whose real DB state is KVT2, so the `Sent→Kvt1` CAS produces a
/// non-Applied outcome → `ConfirmError::Database`; the envelope rolls back
/// atomically (no Kvt1Raw row, doc unchanged).
#[tokio::test]
async fn a2_1a_advance_to_ack_cas_miss_yields_database_error() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    // Doc is at KVT2 in the DB, but we drive the 1a (Sent-entry) path —
    // the hardcoded CAS Sent→Kvt1 cannot apply → non-Applied → Database.
    let doc = seed_doc_in_state(
        &pool,
        1,
        100,
        session_id,
        shift_id,
        "KVT2",
        Some("DPS-FN-CASMISS"),
    )
    .await;

    let result = kvt2_advance::advance_to_ack(
        &pool,
        doc,
        vec![0xCCu8; 8],
        "DPS-FN-CASMISS",
        DocState::ErrorRetryable, // allowed label → 1a path; not the real DB state
        Some(1),
    )
    .await;

    match result {
        Err(kvt2_advance::ConfirmError::Database { source }) => {
            let chain = format!("{source:#}");
            assert!(
                chain.contains("CAS Sent") && chain.contains("produced"),
                "Database source should describe the Sent→Kvt1 CAS-miss; got: {chain}"
            );
        }
        other => panic!("expected ConfirmError::Database, got {other:?}"),
    }
    // Envelope 1a rolled back atomically: doc unchanged, no Kvt1Raw, no advance audit.
    assert_eq!(read_doc_state(&pool, doc).await, "KVT2");
    let kvt1_raw_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM document_files WHERE document_id = ? AND kind = 'KVT1_RAW'",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        kvt1_raw_rows, 0,
        "CAS-miss rollback must leave no Kvt1Raw row"
    );
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await, 0);
}

/// **RS-3 A2.1a follow-up (TEST-4)** — `advance_to_ack`'s own empty-bytes
/// guard returns `ConfirmError::StructuralDrift` before any DB write.  This
/// is the runtime-neutral backstop the module docs treat as load-bearing
/// for A2.1b (the outer `confirm_drain_doc` guard is the drain backstop;
/// the online caller has only this one).
#[tokio::test]
async fn a2_1a_advance_to_ack_empty_bytes_yields_structural_drift() {
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
        Some("DPS-FN-EMPTY"),
    )
    .await;

    let result = kvt2_advance::advance_to_ack(
        &pool,
        doc,
        vec![], // empty evidence → StructuralDrift, no DB write
        "DPS-FN-EMPTY",
        DocState::Sent,
        Some(1),
    )
    .await;

    match result {
        Err(kvt2_advance::ConfirmError::StructuralDrift { detail }) => {
            assert!(
                detail.contains("empty kvt1_raw_bytes"),
                "detail should name the empty-bytes invariant; got: {detail}"
            );
        }
        other => panic!("expected ConfirmError::StructuralDrift, got {other:?}"),
    }
    // No state mutation, no Kvt1Raw row, no advance audit.
    assert_eq!(read_doc_state(&pool, doc).await, "SENT");
    let kvt1_raw_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM document_files WHERE document_id = ? AND kind = 'KVT1_RAW'",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(kvt1_raw_rows, 0);
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await, 0);
}

/// **RS-3 A2.1a follow-up (TEST-5)** — a `stage_finalize::run` typed error
/// reached THROUGH `advance_to_ack` maps to `ConfirmError::Infrastructure`
/// (distinct from the `process_via_w12_kvt2_advance` path the existing
/// `w12_kvt2_stage_finalize_typed_error_*` test covers).  Drives the 1b
/// (Kvt1-entry) path so the Kvt1→Kvt2 CAS commits, then `stage_finalize`
/// errors on a NULL `unsigned_xml_sha256` (no finalize prereqs seeded) →
/// `UnsignedXmlShaMissing` → Infrastructure; the finalize envelope rolls
/// back so the doc rests at KVT2 (the 1b advance is already committed).
#[tokio::test]
async fn a2_1a_advance_to_ack_finalize_error_yields_infrastructure() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    // KVT1 doc, but DELIBERATELY no seed_w12_finalize_prereqs → unsigned_xml_sha256 is NULL.
    let doc = seed_doc_in_state(
        &pool,
        1,
        100,
        session_id,
        shift_id,
        "KVT1",
        Some("DPS-FN-FINERR"),
    )
    .await;

    let result = kvt2_advance::advance_to_ack(
        &pool,
        doc,
        vec![0xAAu8; 16],
        "DPS-FN-FINERR",
        DocState::Kvt1, // 1b path: Kvt1→Kvt2 commits, then finalize errors
        None,
    )
    .await;

    match result {
        Err(kvt2_advance::ConfirmError::Infrastructure { source }) => {
            let chain = format!("{source:#}");
            assert!(
                chain.contains("unsigned_xml_sha256"),
                "Infrastructure source should be the stage_finalize UnsignedXmlShaMissing; got: {chain}"
            );
        }
        other => panic!("expected ConfirmError::Infrastructure, got {other:?}"),
    }
    // Envelope 1b committed (Kvt1→Kvt2); the finalize envelope rolled back,
    // so the doc rests at KVT2 (not Ack, not back to Kvt1).
    assert_eq!(read_doc_state(&pool, doc).await, "KVT2");
    // The 1b advance audit DID land (separate committed envelope); the
    // finalize ACK audit did NOT.
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await, 1);
    assert_eq!(audit_count(&pool, "STAGE_FINALIZE_ACK").await, 0);
}

// ────────────────────────────────────────────────────────────────────────────
// NC-02 (pin-only, #[ignore]) — the ER-redrive budget is doc_type- and
// wall-clock-BLIND: 5 TransientRetry attempts on a SHIFT_OPEN doc escalate to
// RequiresManualReconciliation, contradicting §16 (transport-class for shift
// open/close is UNBOUNDED).  This pins the FUTURE contract (a shift-doc
// TransientRetry must NOT prematurely go Manual) and FAILS today — the
// doc_type/wall-clock policy decision is DEFERRED to A2.2 (shift goes live) +
// M5 (ERROR_SAVE).  Run with `--ignored` to observe the current premature-Manual
// behaviour.  NO policy code is changed by this commit; see dossier NC-02.
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "NC-02: doc_type/wall-clock-aware ER budget deferred to A2.2/M5 (see dossier); pins the future unbounded-for-shift contract — RED today"]
async fn er_guard_shift_doc_transient_retry_must_not_prematurely_manual() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    // A SHIFT_OPEN doc in ERROR_RETRYABLE with 5 TransientRetry SEND attempts.
    let doc = seed_doc_in_state_typed(
        &pool,
        1,
        100,
        session_id,
        shift_id,
        "ERROR_RETRYABLE",
        None,
        "SHIFT_OPEN",
    )
    .await;
    seed_transport_trace_attempt(&pool, doc, 5, Some("TransientRetry")).await;

    let c = carriers(vec![], vec![]);
    let view = view_for(&c);
    backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    // DESIRED (§16): a shift-doc transport-class retry is UNBOUNDED — it must NOT
    // terminalise to Manual at the SELL budget.  Today the doc_type-blind budget
    // DOES escalate, so this assertion FAILS until A2.2/M5 lands the policy.
    assert_ne!(
        read_doc_state(&pool, doc).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "SHIFT_OPEN TransientRetry must not prematurely escalate to Manual \
         (§16: transport-class for shift open/close is unbounded)"
    );
}

// ─── M2-04 (Fable contract, 2026-06-12): ChainSeedMismatch → manual-recon ──
//
// Before M2-04, a stage_finalize ChainSeedMismatch in the KVT2 cohort arm
// (`process_via_w12_kvt2_advance`) mapped to a hard BootError::ReconciliationFailed
// → aborted the whole FN drain tick AND recurred every tick (doc stays KVT2,
// re-selected by the cohort) — a silent fail-loop that never escalated the shift.
// Contract: pattern-match ChainSeedMismatch → DocVerdict::Failed { manual_recon }
// + forensic audit; on a pending-drain shift the loop escalates the shift to
// RequiresManualReconciliation (§16.7).  All OTHER finalize errors stay hard
// ReconciliationFailed (pinned by
// w12_kvt2_stage_finalize_typed_error_routes_to_boot_error_reconciliation_failed).
//
// Reachability note: after M2-01 this arm is unreachable for offline-origin docs
// (finalize skips the chain guard); this test FORCES it by NULLing the cohort
// doc's offline_fiscal_no so finalize runs the online-origin guard, with the
// doc's previous_hash ([0xCD;32]) diverging from the genesis node seed (NULL).
#[tokio::test]
async fn w12_kvt2_chain_seed_mismatch_escalates_manual_recon_not_hard_abort() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(
        &pool,
        NodeMode::GoingOnline,
        ShiftState::OpenedLocalPendingDrain,
    )
    .await;
    // Shift directly in OPENED_LOCAL_PENDING_DRAIN (escalation precondition).
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

    // KVT2 cohort doc (previous_hash = [0xCD;32]); NULL offline_fiscal_no so
    // stage_finalize runs the online-origin chain guard → ChainSeedMismatch vs
    // the genesis node seed (NULL).
    let (doc, _req) =
        seed_kvt2_doc_for_stage_finalize(&pool, 1, 100, session_id, shift_id, "DPS-FN-KVT2-CSM")
            .await;
    sqlx::query("UPDATE fiscal_documents SET offline_fiscal_no = NULL WHERE document_id = ?")
        .bind(doc)
        .execute(&pool)
        .await
        .unwrap();
    // Force the divergence: reset the FN chain seed to genesis (NULL) so the
    // doc's previous_hash ([0xCD;32], set by the helper) no longer matches →
    // stage_finalize's online-origin guard raises ChainSeedMismatch{ expected:
    // Some([0xCD;32]), actual: None }.  (The helper had aligned the seed for the
    // happy-path ACK test.)
    sqlx::query(
        "UPDATE node_state SET last_known_unsigned_xml_sha256 = NULL WHERE fiscal_number = ?",
    )
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();

    // No DPS calls expected on the KVT2 finalize arm.
    let c = carriers(vec![], vec![]);
    let view = view_for(&c);

    // M2-04: the drain must NOT hard-abort — it returns Ok and escalates.
    backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .expect("M2-04: ChainSeedMismatch must NOT surface as BootError::ReconciliationFailed");

    // A.3 re-ground (design v3 §6 step 4; INV-08): finalize's sole
    // `ChainSeedMismatch` producer (the online ACK-time guard) was REMOVED (the
    // chain check moved to the SEND drift-assert).  The drain's w12 KVT2 recovery
    // arm therefore no longer detects this pre-existing chain divergence — it
    // converges the doc to ACK.  The load-bearing invariant this test protects —
    // the drain MUST NOT hard-abort (returns Ok, above) — still holds.  The breach
    // is surfaced by `invariant_scan` (detection moved; no recovery TRANSITION was
    // deleted — the escalation arms remain for scan/boot-detected breaks, PR-C).
    let shift_state: String = sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
        .bind(shift_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(
        shift_state, "REQUIRES_MANUAL_RECONCILIATION",
        "post-A.3: the drain's finalize arm no longer escalates a chain divergence (producer removed)"
    );

    // The KVT2 doc converges to ACK (finalize no longer guards / rolls back).
    assert_eq!(read_doc_state(&pool, doc).await, "ACK");

    // No finalize-triggered escalation audits.
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_FAILED").await, 0);
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await,
        0
    );

    // Detection MOVED to the scan: the chain divergence is flagged there.
    let violations = prro::db::invariant_scan::scan(&pool).await.unwrap();
    assert!(
        violations.iter().any(|v| matches!(
            v,
            prro::db::invariant_scan::Violation::ChainSeedMismatch { .. }
                | prro::db::invariant_scan::Violation::ChainBreak { .. }
        )),
        "invariant_scan surfaces the chain divergence (detection moved): {violations:?}"
    );
}

/// **M2-N4 (architect ruling, 2026-06-13) — DRAIN side** — a SentReplay
/// `ServerFiscalIdMismatch` is "superseded" (benign) ONLY if the DPS tip
/// `actual_id` is the `server_fiscal_no` of one of OUR newer submitted docs.
/// A FOREIGN/garbage tip — even when a newer submitted doc EXISTS (so the
/// lnd-only check would call it superseded) — is genuine structural drift →
/// `BootError::Internal` (NOT a benign SupersededHold).  Mirror of the boot-side
/// pin `m2_n4_boot_foreign_tip_is_manual_not_superseded`.
#[tokio::test]
async fn c5b2_sent_replay_foreign_tip_is_structural_drift_not_superseded() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    // doc_a (lnd=1) + a NEWER submitted doc_b (lnd=2) so the lnd-only check
    // would mislabel doc_a superseded.  DPS tip for doc_a's probe is FOREIGN.
    let doc_a = seed_doc_in_state(
        &pool,
        1,
        100,
        session_id,
        shift_id,
        "SENT",
        Some("DPS-FN-A"),
    )
    .await;
    let _doc_b = seed_doc_in_state(
        &pool,
        2,
        101,
        session_id,
        shift_id,
        "SENT",
        Some("DPS-FN-B"),
    )
    .await;

    // doc_a probe → "DPS-FN-FOREIGN" (NOT our doc_b's sfn) → genuine drift.
    // (doc_b is never probed — the drift halts the drain at doc_a.)
    let c = carriers(vec![], vec![Ok(ack("DPS-FN-FOREIGN", vec![0xFF]))]);
    let view = view_for(&c);

    let err = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .expect_err("foreign tip → structural drift → BootError (M2-N4)");
    match err {
        BootError::Internal(msg) => assert!(
            msg.contains("structural drift")
                || msg.contains("StructuralDrift")
                || msg.contains("LastChkIdMismatch"),
            "BootError MUST reference structural drift; got: {msg}"
        ),
        other => panic!("expected BootError::Internal (structural drift); got: {other:?}"),
    }

    // doc_a held in SENT (drift envelope does not CAS state); NOT superseded.
    assert_eq!(read_doc_state(&pool, doc_a).await, "SENT");
    assert_eq!(audit_count(&pool, "KVT2_CONFIRM_STRUCTURAL_DRIFT").await, 1);
    assert_eq!(
        audit_count(&pool, "TIP_SUPERSEDED").await,
        0,
        "a foreign tip is NOT a benign supersession (M2-N4)"
    );
}
