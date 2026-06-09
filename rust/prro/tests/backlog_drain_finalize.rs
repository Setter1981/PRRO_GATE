//! W9b Commit 6 — finalize branch (`OFFLINE_DRAIN_COMPLETED` /
//! `OFFLINE_DRAIN_PARTIAL` + `Eligible` path CAS node mode + session).
//!
//! Acceptance for spec §2.4 + amendment 2026-05-21:
//!
//!   1. Pre-W12 steady state: every successful per-doc advance is
//!      `DeferredKvt1` → `finalize_eligibility()` returns
//!      `NotEligible{DocsDeferredAtKvt1}` → `OFFLINE_DRAIN_PARTIAL`
//!      audit fires; node_state stays `GoingOnline`; session stays
//!      `Draining`; summary.finalized == false.
//!   2. Per-doc failure → `OFFLINE_DRAIN_PARTIAL` with
//!      `PerDocFailuresPresent` reason.
//!   3. Halt-escalate (manual-recon-class on pending-drain shift)
//!      exits early via `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` and
//!      does NOT fire `OFFLINE_DRAIN_PARTIAL` (distinct exit path).
//!   4. Skip paths (`SKIPPED_NOT_GOING_ONLINE` / `SKIPPED_EMPTY_
//!      BACKLOG`) return BEFORE the finalize evaluation; no PARTIAL
//!      audit emitted.
//!
//! Eligible-arm full-contract test lives in the inline
//! `#[cfg(test)] mod eligible_arm_tests` inside `backlog_drain.rs`
//! (MED-C6-3 fix, 2026-05-21).  Pre-W12 the C5 stub
//! `apply_w12_confirmation` always returns `DeferredKvt1` so the
//! public `drain()` entry cannot naturally reach the Eligible
//! arm; the inline test constructs a `DrainSummary` with `Acked`
//! outcomes manually via the public C2 API and calls
//! `commit_finalize_envelope` directly (crate-internal access).
//! The W12 PR will add a public-entry full-flow Eligible test
//! once the lastChk evidence path makes real Ack proof
//! constructible end-to-end.

mod common;

use std::sync::Arc;

use prro::db::models::enums::{NodeMode, OfflineSessionState, ShiftState};
use prro::db::models::ids::{DocumentId, OfflineSessionId, ShiftId};
use prro::services::offline_sync::backlog_drain;
use prro::services::reconciliation::runtime::RuntimeView;
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::dto::{CheckAck, CheckSignBlob};
use prro::transports::dps::error::{AuthorizationKind, DpsError};
use sqlx::SqlitePool;
use uuid::Uuid;

use common::{det_signing_ctx, StubDpsChannel};

const FN: &str = "1234567890";
const CASHIER_OK: &str = "test-cashier";

// ─── Fixture helpers ─────────────────────────────────────────────────

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("w9b_c6.db"))
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

#[allow(clippy::too_many_arguments)]
async fn seed_doc(
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

async fn read_node_mode(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_session_state(pool: &SqlitePool, session_id: OfflineSessionId) -> String {
    sqlx::query_scalar("SELECT state FROM offline_sessions WHERE offline_session_id = ?")
        .bind(session_id)
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

async fn audit_latest_payload(pool: &SqlitePool, event_type: &str) -> Option<serde_json::Value> {
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT event_payload_json FROM audit_log WHERE event_type = ? \
         ORDER BY audit_id DESC LIMIT 1",
    )
    .bind(event_type)
    .fetch_optional(pool)
    .await
    .unwrap();
    raw.and_then(|s| serde_json::from_str(&s).ok())
}

fn ack(id: &str) -> CheckAck {
    CheckAck {
        id: id.into(),
        id_sign: vec![],
        data_sign: vec![],
    }
}

fn fn_sign() -> CheckSignBlob {
    CheckSignBlob(vec![0xAB, 0xCD])
}

struct DepsCarriers {
    dps: Arc<StubDpsChannel>,
    signing_ctx: SigningContext,
    fn_sign: CheckSignBlob,
}

fn carriers(responses: Vec<Result<CheckAck, DpsError>>) -> DepsCarriers {
    DepsCarriers {
        dps: Arc::new(StubDpsChannel::with_queue(responses)),
        signing_ctx: det_signing_ctx(),
        fn_sign: fn_sign(),
    }
}

/// **M3b W12 Commit 4b.3 (2026-05-22)** — DepsCarriers builder
/// seeding both `send_chk` + `last_chk` queues for SentFresh path.
fn carriers_with_last_chk(
    send_chk: Vec<Result<CheckAck, DpsError>>,
    last_chk: Vec<Result<CheckAck, DpsError>>,
) -> DepsCarriers {
    DepsCarriers {
        dps: Arc::new(StubDpsChannel::with_queue(send_chk).with_last_chk_queue(last_chk)),
        signing_ctx: det_signing_ctx(),
        fn_sign: fn_sign(),
    }
}

/// **M3b W12 Commit 4b.3 (2026-05-22)** — non-empty `data_sign`
/// CheckAck for lastChk Acked path.
fn last_chk_ack(id: &str, data_sign: Vec<u8>) -> CheckAck {
    CheckAck {
        id: id.into(),
        id_sign: vec![],
        data_sign,
    }
}

fn view_for<'a>(carriers: &'a DepsCarriers) -> RuntimeView<'a> {
    RuntimeView {
        dps: carriers.dps.as_ref(),
        signing_ctx: &carriers.signing_ctx,
        fn_sign: &carriers.fn_sign,
    }
}

// ─── Test 1: pre-W12 partial steady-state (DocsDeferredAtKvt1) ───────

#[tokio::test]
async fn c6_eligible_completed_when_all_docs_reach_ack_via_w12() {
    // **M3b W12 Commit 4b.3 (2026-05-22)** — refactored from pre-W12
    // `c6_pre_w12_partial_when_all_advances_are_deferred_kvt1`.  Post
    // W12 wiring, OFFLINE_LOCAL_ACK docs advance through SentFresh
    // chain (Envelope 1a + Envelope 2) to Ack; `finalize_eligibility`
    // flips Eligible → COMPLETED audit + node Online + session Closed.
    // Pre-W12 DeferredKvt1 path is structurally unreachable.
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let doc_a = seed_doc(
        &pool,
        1,
        100,
        session_id,
        shift_id,
        "OFFLINE_LOCAL_ACK",
        None,
    )
    .await;
    let doc_b = seed_doc(
        &pool,
        2,
        101,
        session_id,
        shift_id,
        "OFFLINE_LOCAL_ACK",
        None,
    )
    .await;
    // W12 chain bootstrap — 2-doc chain.
    common::init_chain_seed(&pool, FN, common::chain_anchor(0x00))
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc_a,
        common::chain_anchor(0x00),
        common::chain_anchor(0x01),
    )
    .await
    .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc_b,
        common::chain_anchor(0x01),
        common::chain_anchor(0x02),
    )
    .await
    .unwrap();

    let c = carriers_with_last_chk(
        vec![Ok(ack("A")), Ok(ack("B"))],
        vec![
            Ok(last_chk_ack("A", vec![0xAAu8; 32])),
            Ok(last_chk_ack("B", vec![0xBBu8; 32])),
        ],
    );
    let view = view_for(&c);

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    // Both docs advanced to Ack via W12 SentFresh chain →
    // finalize_eligibility == Eligible → COMPLETED.
    assert_eq!(summary.backlog_size_before(), 2);
    assert_eq!(summary.advanced_to_ack(), 2);
    assert_eq!(summary.advanced_to_kvt1(), 0, "no DeferredKvt1 post-W12");
    assert!(
        summary.finalized(),
        "W12 SentFresh-Acked path enables Eligible finalize"
    );

    // COMPLETED audit fires (not PARTIAL).
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_COMPLETED").await, 1);
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_PARTIAL").await, 0);
    let payload = audit_latest_payload(&pool, "OFFLINE_DRAIN_COMPLETED")
        .await
        .unwrap();
    assert_eq!(payload["outcome"], "COMPLETED");
    assert_eq!(payload["advanced_to_ack"], 2);
    assert_eq!(payload["advanced_to_kvt1"], 0);
    assert_eq!(payload["finalized"], true);
    assert_eq!(payload["entry_reason"], "normal_eligible");

    // W12 per-doc audit chain — Envelope 1a + Envelope 2.
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await, 2);
    assert_eq!(audit_count(&pool, "STAGE_FINALIZE_ACK").await, 2);

    // Node + session closed via Eligible-arm finalize envelope.
    assert_eq!(read_node_mode(&pool).await, "ONLINE");
    assert_eq!(read_session_state(&pool, session_id).await, "CLOSED");
}

// ─── Test 2: per-doc failure → PARTIAL with PerDocFailuresPresent ────

#[tokio::test]
async fn c6_partial_when_per_doc_failures_present() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let _doc = seed_doc(
        &pool,
        1,
        100,
        session_id,
        shift_id,
        "OFFLINE_LOCAL_ACK",
        None,
    )
    .await;

    // TerminalReject → per-doc failure on plain Opened shift (sibling
    // continues; no halt because shift is NOT pending-drain).
    let c = carriers(vec![Err(DpsError::Authorization {
        code: -1,
        kind: AuthorizationKind::DocumentReject,
        message: "reject".into(),
    })]);
    let view = view_for(&c);

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(summary.per_doc_failures().len(), 1);
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_PARTIAL").await, 1);
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_COMPLETED").await, 0);
    let payload = audit_latest_payload(&pool, "OFFLINE_DRAIN_PARTIAL")
        .await
        .unwrap();
    // `PerDocFailuresPresent` takes precedence over the DeferredKvt1
    // reason — the finalize_eligibility decision tree returns the
    // failure reason first (per C2 implementation order).
    assert_eq!(
        payload["not_eligible_reason"]["kind"],
        "PerDocFailuresPresent"
    );
    assert_eq!(payload["not_eligible_reason"]["count"], 1);
    assert_eq!(payload["per_doc_failures"].as_array().unwrap().len(), 1);
}

// ─── Test 3: halt-escalate path does NOT fire PARTIAL ────────────────

#[tokio::test]
async fn c6_halt_escalate_path_does_not_fire_partial() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(
        &pool,
        NodeMode::GoingOnline,
        ShiftState::OpenedLocalPendingDrain,
    )
    .await;
    // Need a pending-drain shift + current_shift_id wired.
    let shift_id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts(shift_id, fiscal_number, serial, state, \
            open_mode, cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED_LOCAL_PENDING_DRAIN', 'OFFLINE', 0, ?)",
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
    let _doc = seed_doc(
        &pool,
        1,
        100,
        session_id,
        shift_id,
        "OFFLINE_LOCAL_ACK",
        None,
    )
    .await;

    // TerminalReject on pending-drain shift → manual-recon class →
    // halt-escalate (NOT PARTIAL).
    let c = carriers(vec![Err(DpsError::Authorization {
        code: -1,
        kind: AuthorizationKind::DocumentReject,
        message: "halt_trigger".into(),
    })]);
    let view = view_for(&c);

    let _summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    // Halt-escalate audit fired; finalize branch (PARTIAL/COMPLETED)
    // was NOT reached because drain returned early.
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await,
        1
    );
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_PARTIAL").await,
        0,
        "halt-escalate path MUST NOT fire PARTIAL — distinct exit"
    );
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_COMPLETED").await, 0);
}

// ─── Test 4: skip paths do NOT fire PARTIAL ──────────────────────────

#[tokio::test]
async fn c6_skip_paths_do_not_fire_partial() {
    // (a) SKIPPED_NOT_GOING_ONLINE
    {
        let (_d, pool) = fresh_pool().await;
        seed_node_state(&pool, NodeMode::Offline, ShiftState::Opened).await;
        let c = carriers(vec![]);
        let view = view_for(&c);
        let _summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
            .await
            .unwrap();
        assert_eq!(
            audit_count(&pool, "OFFLINE_DRAIN_SKIPPED_NOT_GOING_ONLINE").await,
            1
        );
        assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_PARTIAL").await, 0);
        assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_COMPLETED").await, 0);
    }

    // (b) SKIPPED_EMPTY_BACKLOG (no active session — distinct C5 path)
    {
        let (_d, pool) = fresh_pool().await;
        seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
        let c = carriers(vec![]);
        let view = view_for(&c);
        let _summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
            .await
            .unwrap();
        assert_eq!(
            audit_count(&pool, "OFFLINE_DRAIN_SKIPPED_EMPTY_BACKLOG").await,
            1
        );
        assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_PARTIAL").await, 0);
        assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_COMPLETED").await, 0);
    }
}

// ─── M3b W12 Commit 3 Δ (MED-W12C3-01) — drain-finalize crash-recovery ──
//
// Reviewer-flagged MED finding 2026-05-22.  Post-Commit-3, drain itself
// commits `stage_finalize::run` Kvt2 → Ack durably for the W12 KVT2
// cohort entry path.  If the process crashes BETWEEN that envelope
// commit AND the `finalize_drain` 5-write closure, the next drain
// tick sees an empty cohort (doc now ACK, excluded by SELECT IN
// filter) — but node/session/shift remain in pre-finalize states.
//
// Fix (Commit 3 Δ): in the empty-cohort branch, when session_state is
// `Draining` AND `is_session_drain_completable(session_id)` proves
// all session docs are in `ACK`, route through `commit_finalize_
// envelope` with `FinalizeEntry::CrashRecovery` (distinct
// `OFFLINE_DRAIN_RECOVERED_FINALIZE` audit).  Otherwise → existing
// empty-backlog skip path.

/// Helper — seed a doc DIRECTLY in `ACK` terminal state.  Used to
/// simulate the post-crash scenario where stage_finalize::run
/// already committed Kvt2 → Ack durably in a prior drain tick.
#[allow(clippy::too_many_arguments)]
async fn seed_doc_in_ack(
    pool: &SqlitePool,
    lnd: i64,
    code_lnd: i64,
    session_id: OfflineSessionId,
    shift_id: ShiftId,
    server_fiscal_no: &str,
) -> DocumentId {
    seed_doc(
        pool,
        lnd,
        code_lnd,
        session_id,
        shift_id,
        "ACK",
        Some(server_fiscal_no),
    )
    .await
}

#[tokio::test]
async fn w12_crash_recovery_finalizes_session_when_all_docs_already_ack() {
    let (_d, pool) = fresh_pool().await;
    // Seed: pending-drain shift state (real crash scenario — offline
    // writes during pending-drain).  node_state.mode=GoingOnline,
    // shift_state=OpenedLocalPendingDrain, session=Draining (prior
    // tick mid-pass transition committed before crash).
    seed_node_state(
        &pool,
        NodeMode::GoingOnline,
        ShiftState::OpenedLocalPendingDrain,
    )
    .await;
    let shift_id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts(shift_id, fiscal_number, serial, state, \
            open_mode, cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED_LOCAL_PENDING_DRAIN', 'OFFLINE', 0, ?)",
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
    let session_id = seed_offline_session(&pool, OfflineSessionState::Draining).await;

    // Seed doc already in ACK — simulates stage_finalize::run committed
    // Kvt2 → Ack durably in prior tick, then process crashed before
    // reaching finalize_drain.
    let _doc = seed_doc_in_ack(&pool, 1, 100, session_id, shift_id, "DPS-FN-RECOVERY").await;

    // No DPS calls expected — recovery path is pool-only.
    let c = carriers(vec![]);
    let view = view_for(&c);

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    // Summary shape: 0 in-flight this tick (work landed in prior
    // crashed tick), finalized=true because recovery branch
    // converged.
    assert_eq!(summary.backlog_size_before(), 0);
    assert_eq!(summary.advanced_to_ack(), 0);
    assert!(
        summary.finalized(),
        "recovery branch must set finalized=true via commit_finalize_envelope"
    );

    // DB state — full finalize closure.
    assert_eq!(read_node_mode(&pool).await, "ONLINE");
    assert_eq!(read_session_state(&pool, session_id).await, "CLOSED");
    let shift_state_after: String =
        sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
            .bind(shift_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        shift_state_after, "OPENED",
        "OpenedLocalPendingDrain → Opened (edge 5) per pending-drain ladder"
    );

    // Audit emission — distinct RECOVERED_FINALIZE, NOT regular COMPLETED.
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_RECOVERED_FINALIZE").await,
        1,
        "crash-recovery branch emits distinct audit event"
    );
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_COMPLETED").await,
        0,
        "regular COMPLETED audit MUST NOT fire on recovery path"
    );
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_SKIPPED_EMPTY_BACKLOG").await,
        0,
        "skip path MUST NOT fire when recovery branch takes over"
    );
    let recovery_payload = audit_latest_payload(&pool, "OFFLINE_DRAIN_RECOVERED_FINALIZE")
        .await
        .unwrap();
    assert_eq!(recovery_payload["entry_reason"], "crash_recovery");
    assert_eq!(recovery_payload["finalized"], true);
    assert_eq!(recovery_payload["backlog_size_before"], 0);
    assert_eq!(recovery_payload["advanced_to_ack"], 0);

    // OFFLINE_SESSION_CLOSED audit also fires per existing envelope.
    assert_eq!(audit_count(&pool, "OFFLINE_SESSION_CLOSED").await, 1);
}

#[tokio::test]
async fn w12_crash_recovery_skips_when_session_has_non_ack_terminal_docs() {
    // Conservative predicate: REJECTED/MANUAL terminal docs DO NOT
    // count as "completable" — session must NOT auto-finalize unless
    // ALL docs are in ACK.
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Draining).await;

    let _ack_doc = seed_doc_in_ack(&pool, 1, 100, session_id, shift_id, "DPS-FN-NEGATIVE-1").await;
    // Seed second doc in REJECTED terminal state — predicate must
    // reject the whole session as non-completable.
    let _rejected_doc = seed_doc(
        &pool,
        2,
        101,
        session_id,
        shift_id,
        "REJECTED",
        Some("DPS-FN-NEGATIVE-2"),
    )
    .await;

    let c = carriers(vec![]);
    let view = view_for(&c);

    let _summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    // Skip path fires — recovery branch refused due to non-ACK
    // terminal doc.  Node/session/shift untouched.
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_SKIPPED_EMPTY_BACKLOG").await,
        1,
    );
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_RECOVERED_FINALIZE").await,
        0
    );
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_COMPLETED").await, 0);
    assert_eq!(read_node_mode(&pool).await, "GOING_ONLINE");
    assert_eq!(read_session_state(&pool, session_id).await, "DRAINING");
}

#[tokio::test]
async fn w12_crash_recovery_skips_when_session_open_with_zero_docs() {
    // Open session + empty docs — genuinely fresh session, NOT a
    // crash-recovery case.  Existing skip path must fire.
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let _shift_id = seed_open_shift(&pool).await;
    let _session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;

    let c = carriers(vec![]);
    let view = view_for(&c);

    let _summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_SKIPPED_EMPTY_BACKLOG").await,
        1
    );
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_RECOVERED_FINALIZE").await,
        0
    );
}

#[tokio::test]
async fn w12_crash_recovery_skips_when_session_draining_with_zero_docs() {
    // Draining session + zero docs — degenerate edge case
    // (predicate requires total > 0).  Skip path must fire to
    // avoid auto-finalizing a session that never had docs.
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let _shift_id = seed_open_shift(&pool).await;
    let _session_id = seed_offline_session(&pool, OfflineSessionState::Draining).await;

    let c = carriers(vec![]);
    let view = view_for(&c);

    let _summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_SKIPPED_EMPTY_BACKLOG").await,
        1
    );
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_RECOVERED_FINALIZE").await,
        0
    );
}
