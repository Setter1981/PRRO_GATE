//! W9b Commit 7 — `App::drain_offline_backlog_with` integration tests.
//!
//! Acceptance for spec §2.1 (a) + §10 C7: App-owned entry that
//! acquires the App reconcile mutex (W2 enforcement per ADR-M3-A10)
//! and delegates to the pure-function `backlog_drain::drain`.
//!
//! **M3b W12 Commit 4b update (2026-05-22)**: SentFresh production-
//! wired via `process_via_stage_send` Sent branch.  App-owned
//! happy path now reaches the Eligible arm (per `c6_eligible_*`
//! fixtures' chain) end-to-end without bypassing public entry.
//!
//! Three integration tests:
//!
//!   1. `app_drain_skip_path_mode_not_going_online` — boots App,
//!      seeds FN with `node_state.mode = Offline`, calls
//!      `App::drain_offline_backlog_with`, asserts skip via
//!      `SKIPPED_NOT_GOING_ONLINE` audit + empty summary.
//!   2. `app_drain_eligible_path_via_w12_sent_fresh_steady_state`
//!      (post W12 Commit 4b refactor) — boots App, seeds
//!      GoingOnline + Open session + 2 OFFLINE_LOCAL_ACK docs +
//!      W12 chain prereqs + lastChk Acked queue, calls
//!      `App::drain_offline_backlog_with`, asserts SentFresh chain:
//!      per-doc Envelope 1a + Envelope 2 → ACK; finalize Eligible
//!      → `OFFLINE_DRAIN_COMPLETED` + `OFFLINE_SESSION_CLOSED`
//!      audits; node ONLINE; session CLOSED.
//!   3. `app_drain_concurrent_invocations_smoke_no_deadlock` —
//!      W9b NIT-C7-R2 concurrent invocation smoke (mutex
//!      serialization, no panic / no wedge under tokio::join).
//!
//! Eligible-arm CAS chain + audit shape now exercised end-to-end
//! through the public entry via test #2.  The inline
//! `eligible_arm_tests` in `backlog_drain.rs` provide complementary
//! direct-call coverage of `commit_finalize_envelope`.

mod common;

use std::sync::Arc;

use prro::config::AppConfig;
use prro::db::models::enums::{NodeMode, OfflineSessionState, ShiftState};
use prro::db::models::ids::{DocumentId, OfflineSessionId, ShiftId};
use prro::services::reconciliation::runtime::RuntimeView;
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::dto::{CheckAck, CheckSignBlob};
use prro::transports::dps::error::DpsError;
use sqlx::SqlitePool;
use uuid::Uuid;

use common::{det_signing_ctx, StubDpsChannel};

const FN: &str = "1234567890";
const CASHIER_OK: &str = "test-cashier";

// ─── Fixture helpers ─────────────────────────────────────────────────

async fn boot_app(db_filename: &str) -> (tempfile::TempDir, prro::App, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join(db_filename);
    let toml_text = format!(
        r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "{0}"
secure_db_path = "{0}_secure"

[admin_ui]
enabled = false
listen  = "127.0.0.1:8443"
"#,
        db_path.display().to_string().replace('\\', "/")
    );
    let cfg = AppConfig::from_toml(&toml_text).expect("config parse");
    let app = prro::App::boot(cfg).await.expect("App::boot");
    let pool = app.db().clone();
    (dir, app, pool)
}

async fn seed_fn_config(pool: &SqlitePool) {
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(FN)
    .execute(pool)
    .await
    .unwrap();
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

async fn seed_offline_local_ack(
    pool: &SqlitePool,
    lnd: i64,
    code_lnd: i64,
    session_id: OfflineSessionId,
    shift_id: ShiftId,
) -> DocumentId {
    let doc_id = DocumentId::new();
    let req_id = Uuid::now_v7();
    let sha = vec![0u8; 32];
    // B8: offline_dps_code must be set so fetch_send_inputs_tx returns non-NULL
    // and the fail-closed drain guard in stage_send passes.
    let dps_code = format!("DRAIN-{code_lnd}");
    sqlx::query(
        "INSERT INTO fiscal_documents( \
            document_id, request_id, fiscal_number, shift_id, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, signed_by_cashier_id, \
            offline_session_id, offline_fiscal_no, offline_fiscal_date, offline_dps_code \
         ) VALUES ( \
            ?, ?, ?, ?, ?, 'SELL', 'OFFLINE_LOCAL_ACK', \
            'b', 't', 'OFFLINE', '2026-05-21T00:00:00Z', \
            '{}', ?, ?, \
            ?, ?, '2026-05-21T00:00:00Z', ? \
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

async fn audit_count(pool: &SqlitePool, event_type: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type = ?")
        .bind(event_type)
        .fetch_one(pool)
        .await
        .unwrap()
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

/// **M3b W12 Commit 4b.3 Δ2 (2026-05-22)** — DepsCarriers builder
/// seeding both `send_chk` + `last_chk` queues.  Required for App
/// public-entry tests exercising the Sent-source W12 chain
/// (process_via_stage_send → confirm_drain_doc(SentFresh) → lastChk
/// → Envelope 1a + Envelope 2 → ACK).
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

/// **M3b W12 Commit 4b.3 Δ2 (2026-05-22)** — lastChk Acked response
/// with non-empty `data_sign` (classify_check_result routes empty
/// to Hold).
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

// ─── Test 1: skip path — App entry returns empty summary ─────────────

#[tokio::test]
async fn app_drain_skip_path_mode_not_going_online() {
    let (_d, app, pool) = boot_app("c7_skip.db").await;
    seed_fn_config(&pool).await;
    seed_node_state(&pool, NodeMode::Offline, ShiftState::Opened).await;

    let c = carriers(vec![]);
    let view = view_for(&c);

    let summary = app
        .drain_offline_backlog_with(FN, &view)
        .await
        .expect("App entry must return Ok on skip");

    assert_eq!(summary.backlog_size_before(), 0);
    assert!(summary.per_doc_failures().is_empty());
    assert!(!summary.finalized());
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_SKIPPED_NOT_GOING_ONLINE").await,
        1
    );
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_STARTED").await, 0);
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_PARTIAL").await, 0);
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_COMPLETED").await, 0);
    assert_eq!(c.dps.call_count(), 0);
}

// ─── Test 2: App-entry happy path — full W12 SentFresh steady-state ─

/// **M3b W12 Commit 4b.3 Δ2 (2026-05-22)** — refactored from
/// pre-W12 `app_drain_partial_path_pre_w12_steady_state`.  Post W12
/// production wiring (`process_via_stage_send` → `confirm_drain_doc(
/// SentFresh, ...)` → Envelope 1a + Envelope 2 → ACK), both docs
/// reach ACK; finalize_eligibility flips Eligible →
/// `OFFLINE_DRAIN_COMPLETED` audit; node Online + session Closed.
/// Pre-W12 DeferredKvt1 path is structurally unreachable.
#[tokio::test]
async fn app_drain_eligible_path_via_w12_sent_fresh_steady_state() {
    let (_d, app, pool) = boot_app("c7_eligible.db").await;
    seed_fn_config(&pool).await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let doc_a = seed_offline_local_ack(&pool, 1, 100, session_id, shift_id).await;
    let doc_b = seed_offline_local_ack(&pool, 2, 101, session_id, shift_id).await;
    // W12 chain bootstrap — anchor + per-doc finalize prereqs.
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

    // send_chk × 2 + last_chk × 2 (SentFresh confirm per doc).
    let c = carriers_with_last_chk(
        vec![Ok(ack("DPS-FN-A")), Ok(ack("DPS-FN-B"))],
        vec![
            Ok(last_chk_ack("DPS-FN-A", vec![0xAAu8; 64])),
            Ok(last_chk_ack("DPS-FN-B", vec![0xBBu8; 64])),
        ],
    );
    let view = view_for(&c);

    let summary = app
        .drain_offline_backlog_with(FN, &view)
        .await
        .expect("App entry must return Ok on happy flow");

    // Both docs reached Ack via W12 SentFresh chain →
    // finalize_eligibility == Eligible → COMPLETED.
    assert_eq!(summary.backlog_size_before(), 2);
    assert_eq!(summary.advanced_to_ack(), 2);
    assert_eq!(summary.advanced_to_kvt1(), 0, "no DeferredKvt1 post-W12");
    assert!(
        summary.finalized(),
        "W12 SentFresh-Acked path enables Eligible finalize"
    );

    // Full W12 audit chain at App entry:
    // STARTED + SESSION_DRAIN_STARTED + 2 KVT2_ADVANCED + 2
    // STAGE_FINALIZE_ACK + COMPLETED + SESSION_CLOSED.
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_STARTED").await, 1);
    assert_eq!(audit_count(&pool, "OFFLINE_SESSION_DRAIN_STARTED").await, 1);
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await, 2);
    assert_eq!(audit_count(&pool, "STAGE_FINALIZE_ACK").await, 2);
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_COMPLETED").await, 1);
    assert_eq!(audit_count(&pool, "OFFLINE_SESSION_CLOSED").await, 1);
    // Pre-W12 audit types MUST NOT fire post-W12 wiring.
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_DOC_ADVANCED").await,
        0,
        "pre-W12 stub audit MUST NOT fire post-W12 wiring"
    );
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_PARTIAL").await, 0);

    // Node + session closed via Eligible-arm finalize envelope.
    assert_eq!(read_node_mode(&pool).await, "ONLINE");
    assert_eq!(read_session_state(&pool, session_id).await, "CLOSED");

    // Wire stubs consumed — 2 send_chk (stage_send) + 2 lastChk
    // (confirm_drain_doc SentFresh).
    assert_eq!(c.dps.call_count(), 2);
    assert_eq!(c.dps.last_chk_call_count(), 2);
}

// ─── Test 3: concurrent invocation smoke (no-deadlock only) ──────────

/// NIT-C7-R2 smoke test (2026-05-21): two concurrent
/// `App::drain_offline_backlog_with` calls on the SAME `App`
/// complete without deadlock.  Both calls return `Ok` and BOTH
/// SKIPPED audit rows land — proving the App entry handles
/// concurrent invocation without panic or wedge.
///
/// **Scope of guarantee** (LOW-C7-R1 wording fix 2026-05-21): this
/// is a smoke / no-deadlock proof, NOT a serialization proof.  Two
/// skip-path calls would pass this test even without the App mutex
/// (they don't conflict on durable state).  Actual serialization
/// is enforced structurally by `self.inner.reconcile_mutex.lock()
/// .await` + `ReconcileGuard::from_app_mutex` in the App entry +
/// `&ReconcileGuard<'_>` requirement on the `backlog_drain::drain`
/// signature (NIT-C7-R1).  A blocking fixture that proves ordering
/// (e.g. via a spy hooked into the mutex critical section) belongs
/// in W12 PR when there's a heavier flow worth pinning.
#[tokio::test]
async fn app_drain_concurrent_invocations_smoke_no_deadlock() {
    let (_d, app, pool) = boot_app("c7_contention.db").await;
    seed_fn_config(&pool).await;
    seed_node_state(&pool, NodeMode::Offline, ShiftState::Opened).await;

    let c = carriers(vec![]);

    // Spawn 2 concurrent drain invocations via `tokio::join!`.  Both
    // race for the App reconcile mutex.  Without mutex, both would
    // run concurrently; with mutex, they serialise.  Either way, the
    // smoke test is: both complete in bounded time without panic.
    let app1 = app.clone();
    let app2 = app.clone();
    let view1 = view_for(&c);
    let view2 = view_for(&c);
    let fut1 = async move { app1.drain_offline_backlog_with(FN, &view1).await };
    let fut2 = async move { app2.drain_offline_backlog_with(FN, &view2).await };
    let (r1, r2) = tokio::join!(fut1, fut2);

    let s1 = r1.expect("first concurrent invocation MUST complete Ok");
    let s2 = r2.expect("second concurrent invocation MUST complete Ok");

    // Both hit the SKIPPED_NOT_GOING_ONLINE path (mode=Offline).
    assert_eq!(s1.backlog_size_before(), 0);
    assert_eq!(s2.backlog_size_before(), 0);

    // Audit count: 2 SKIPPED audits — one per invocation.  If the
    // mutex had failed and the invocations had panicked / deadlocked,
    // we'd see fewer than 2 here.
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_SKIPPED_NOT_GOING_ONLINE").await,
        2,
        "both concurrent invocations MUST emit their SKIPPED audit \
         (mutex serializes; neither call gets dropped)"
    );

    // Drop stub carriers to release the borrow held by view1/view2
    // (the views borrowed `&c` for the lifetime of the futures).
    drop(c);
}

// ─── Phase 4 REC-2: scheduled drain з per-FN exponential backoff ─────

/// **Phase 4 / REC-2 (2026-05-24)** — first scheduled invocation runs
/// drain (no prior backoff state).  Verifies `drain_offline_backlog_
/// scheduled` returns `Ran(summary)` для fresh FN.
#[tokio::test]
async fn rec2_scheduled_first_call_runs_drain_with_no_prior_backoff() {
    use prro::ScheduledDrainOutcome;
    let (_d, app, pool) = boot_app("rec2_first.db").await;
    seed_fn_config(&pool).await;
    seed_node_state(&pool, NodeMode::Offline, ShiftState::Opened).await;
    let c = carriers(vec![]);
    let view = view_for(&c);

    let outcome = app
        .drain_offline_backlog_scheduled(FN, &view)
        .await
        .expect("scheduled drain");

    match outcome {
        ScheduledDrainOutcome::Ran(_) => {}
        ScheduledDrainOutcome::SkippedBackoff { .. } => {
            panic!("first scheduled call MUST run drain (no prior backoff state)")
        }
    }
}

/// **Phase 4 / REC-2 (2026-05-24)** — після Hold outcome (here: skip
/// path via mode=Offline emits empty summary з 0 holds, BUT the
/// integration test uses a true Hold via Kvt1Reentry).  Backoff state
/// incremented; second invocation within window returns
/// `SkippedBackoff { next_eligible }`.
#[tokio::test]
async fn rec2_scheduled_after_hold_skips_within_backoff_window() {
    use prro::ScheduledDrainOutcome;
    let (_d, app, pool) = boot_app("rec2_hold.db").await;
    seed_fn_config(&pool).await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    // Seed KVT1 doc → drain dispatches Kvt1Reentry; з Transport DPS
    // err → Hold(DpsTransport) → HoldFnDrain { HeldAtKvt1 }.
    let doc_id = DocumentId::new();
    let req_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO fiscal_documents( \
            document_id, request_id, fiscal_number, shift_id, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, signed_by_cashier_id, \
            offline_session_id, offline_fiscal_no, offline_fiscal_date, \
            server_fiscal_no) \
         VALUES (?, ?, ?, ?, 100, 'SELL', 'KVT1', \
            'b1', 't1', 'OFFLINE', '2026-05-21T00:00:00Z', \
            '{}', ?, ?, ?, 100, '2026-05-21T00:00:00Z', ?)",
    )
    .bind(doc_id)
    .bind(req_id.as_bytes().to_vec())
    .bind(FN)
    .bind(shift_id)
    .bind(vec![0u8; 32])
    .bind(CASHIER_OK)
    .bind(session_id)
    .bind("DPS-FN-REC2")
    .execute(&pool)
    .await
    .unwrap();
    common::init_chain_seed(&pool, FN, common::chain_anchor(0x00))
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc_id,
        common::chain_anchor(0x00),
        common::chain_anchor(0x01),
    )
    .await
    .unwrap();

    // Carriers: lastChk returns Transport err → Hold.
    let c = carriers_with_last_chk(vec![], vec![Err(DpsError::Transport("simulated".into()))]);
    let view = view_for(&c);

    // Tick 1: drain runs + observes Hold → backoff state transitions.
    let outcome1 = app
        .drain_offline_backlog_scheduled(FN, &view)
        .await
        .expect("tick 1 scheduled drain");
    match outcome1 {
        ScheduledDrainOutcome::Ran(summary) => {
            assert_eq!(
                summary.held_at_kvt1(),
                1,
                "tick 1 MUST register Kvt1Reentry Hold"
            );
        }
        ScheduledDrainOutcome::SkippedBackoff { .. } => {
            panic!("tick 1 MUST run drain — no prior backoff state")
        }
    }

    // Tick 2 (immediate): within 30s backoff window → SkippedBackoff.
    let outcome2 = app
        .drain_offline_backlog_scheduled(FN, &view)
        .await
        .expect("tick 2 scheduled drain");
    match outcome2 {
        ScheduledDrainOutcome::Ran(_) => {
            panic!(
                "tick 2 within backoff window MUST be skipped (REC-2 \
                 exponential backoff filter — Hold from tick 1 should \
                 gate tick 2)"
            )
        }
        ScheduledDrainOutcome::SkippedBackoff { next_eligible: _ } => {
            // Success: backoff window honored.  next_eligible is
            // ~30s в future (first Hold → 2^1 * 30s = 60s window).
        }
    }
}

/// **FW-1 mutation teeth (2026-07-14)** — TWIN of
/// `rec2_scheduled_after_hold_skips_within_backoff_window` whose ONLY
/// hold projection is `HeldAtSent` (NOT `HeldAtKvt1`), so
/// `summary.held_at_kvt1() == 0` at the post-drain backoff decision.
///
/// Kills a cargo-mutants survivor at `app.rs` `drain_offline_backlog_
/// scheduled`: the `any_hold` expression
/// ```ignore
/// any_hold = held_at_kvt1() > 0 || held_at_sent() > 0 || er_redrive_queued() > 0
/// ```
/// The survivor rewrites the 2nd/3rd clauses to `held_at_sent() < 0
/// && er_redrive_queued() < 0`.  Both counters are `usize` (always
/// `>= 0`), so `< 0` is a constant `false`, collapsing the whole
/// expression to `any_hold = held_at_kvt1() > 0`.  The existing
/// KVT1-doc sibling still passes under that mutation (its
/// `held_at_kvt1() == 1` keeps the surviving first clause `true`), so
/// only a SENT-only cohort exposes it.
///
/// Cohort shape: a single `OFFLINE_LOCAL_ACK` doc that stage_send
/// advances to `Sent` (`send_chk` → `Ok(ack)`), then the SentFresh
/// KVT2-confirm lastChk returns a Transport error →
/// `Kvt2ConfirmOutcome::Hold(DpsTransport)` →
/// `HoldFnDrain { projection: HeldAtSent }` →
/// `summary.record_doc_held_at_sent()`.  Thus tick-1 summary has
/// `held_at_sent() == 1` AND `held_at_kvt1() == 0`.
///
///   - Correct code: `held_at_sent() > 0` clause keeps `any_hold ==
///     true` → `backoff::on_hold` → tick 2 within window =
///     `SkippedBackoff` (REC-2 backoff engaged).
///   - Mutated code: `any_hold` collapses to `held_at_kvt1() > 0` ==
///     `false` → `backoff::on_advance` (reset, immediate eligibility)
///     → tick 2 `Ran` → the tick-2 assertion below FAILS (teeth fire).
///
/// The wire-call storm this guards against: a SENT-only Hold that
/// never backs off means the supervisor re-hits DPS at full ~60s
/// cadence forever instead of exponential backoff (REC-2 operational
/// safety).
#[tokio::test]
async fn rec2_scheduled_after_held_at_sent_hold_skips_within_backoff_window() {
    use prro::ScheduledDrainOutcome;
    let (_d, app, pool) = boot_app("rec2_hold_sent.db").await;
    seed_fn_config(&pool).await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    // Seed ONE OFFLINE_LOCAL_ACK doc: stage_send advances it to Sent
    // (send_chk Ok), then the SentFresh lastChk confirm returns a
    // Transport error → HoldFnDrain { HeldAtSent }.  Crucially this
    // cohort has NO KVT1 doc, so held_at_kvt1() stays 0.
    let doc_id = seed_offline_local_ack(&pool, 1, 100, session_id, shift_id).await;
    common::init_chain_seed(&pool, FN, common::chain_anchor(0x00))
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc_id,
        common::chain_anchor(0x00),
        common::chain_anchor(0x01),
    )
    .await
    .unwrap();

    // Tick 1: send_chk → Ok(ack) (stage_send Sent), last_chk →
    // Transport err (SentFresh confirm Hold → HeldAtSent).
    //
    // Tick-2 spare responses: on CORRECT code tick 2 is SkippedBackoff
    // and never touches the wire, so these are simply left unconsumed
    // (harmless).  Under the MUTATION tick 2 RUNS the drain again (the
    // doc now rests at SENT → process_via_lastchk_replay → last_chk);
    // the spare Transport err lets that second drain complete and
    // return `Ran` so the explicit tick-2 `panic!` assertion fires
    // with a legible mutation-specific message (instead of an
    // empty-queue stub panic).
    let c = carriers_with_last_chk(
        vec![Ok(ack("DPS-FN-SENT"))],
        vec![
            Err(DpsError::Transport("simulated".into())),
            Err(DpsError::Transport("simulated-tick2-spare".into())),
        ],
    );
    let view = view_for(&c);

    // Tick 1: drain runs; SentFresh Hold registers HeldAtSent only.
    let outcome1 = app
        .drain_offline_backlog_scheduled(FN, &view)
        .await
        .expect("tick 1 scheduled drain");
    match outcome1 {
        ScheduledDrainOutcome::Ran(summary) => {
            assert_eq!(
                summary.held_at_sent(),
                1,
                "tick 1 MUST register a SentFresh HeldAtSent Hold"
            );
            assert_eq!(
                summary.held_at_kvt1(),
                0,
                "SENT-only cohort — held_at_kvt1 MUST be 0 so the mutation \
                 that collapses any_hold to held_at_kvt1()>0 is exposed"
            );
            assert_eq!(
                summary.er_redrive_queued(),
                0,
                "no ER redrive in this cohort"
            );
        }
        ScheduledDrainOutcome::SkippedBackoff { .. } => {
            panic!("tick 1 MUST run drain — no prior backoff state")
        }
    }

    // Tick 2 (immediate): correct code engaged backoff::on_hold from
    // the HeldAtSent Hold → within window = SkippedBackoff.  Under the
    // mutation any_hold collapsed to held_at_kvt1()>0 == false →
    // on_advance → this arm RUNS → panic → teeth fire.
    let outcome2 = app
        .drain_offline_backlog_scheduled(FN, &view)
        .await
        .expect("tick 2 scheduled drain");
    match outcome2 {
        ScheduledDrainOutcome::Ran(_) => {
            panic!(
                "tick 2 within backoff window MUST be skipped: a SENT-only \
                 HeldAtSent Hold on tick 1 MUST engage REC-2 backoff \
                 (any_hold via the held_at_sent()>0 clause). Ran here means \
                 any_hold collapsed to held_at_kvt1()>0 == false → \
                 backoff::on_advance → wire-call storm at full cadence."
            )
        }
        ScheduledDrainOutcome::SkippedBackoff { next_eligible: _ } => {
            // Success: HeldAtSent Hold engaged the backoff window.
        }
    }
}

/// **Polish cycle / GAP-3 (2026-05-25)** — locks REC-2 backoff
/// `on_advance` reset path through the `drain_offline_backlog_
/// scheduled` wrapper.  Validates that an empty-cohort drain
/// (no Hold outcomes) calls `backoff::on_advance` to initialize
/// state з consecutive_holds==0 + next_eligible==now → consecutive
/// invocations remain eligible.
///
/// This complements `rec2_scheduled_first_call_runs_drain_with_no_
/// prior_backoff` (which only verifies fresh-state behavior) by
/// proving that the post-drain state-update path для non-Hold
/// outcomes is observable via the scheduled wrapper's return shape.
#[tokio::test]
async fn rec2_scheduled_empty_cohort_keeps_fn_immediately_eligible_for_next_tick() {
    use prro::ScheduledDrainOutcome;
    let (_d, app, pool) = boot_app("rec2_empty.db").await;
    seed_fn_config(&pool).await;
    // Empty cohort: GoingOnline + NO offline session + NO docs →
    // drain hits `SKIPPED_NO_OFFLINE_SESSION` path, summary is
    // empty з 0 holds.
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let c = carriers(vec![]);
    let view = view_for(&c);

    // Tick 1: drain runs, returns Ran(empty summary).
    let tick1 = app
        .drain_offline_backlog_scheduled(FN, &view)
        .await
        .expect("tick 1 scheduled");
    match tick1 {
        ScheduledDrainOutcome::Ran(summary) => {
            // No holds, no advances — empty cohort.
            assert_eq!(summary.held_at_kvt1(), 0);
            assert_eq!(summary.held_at_sent(), 0);
            assert_eq!(summary.er_redrive_queued(), 0);
        }
        ScheduledDrainOutcome::SkippedBackoff { .. } => {
            panic!("tick 1 з fresh App MUST run drain")
        }
    }

    // Tick 2 immediate: backoff::on_advance from tick 1 set
    // next_eligible to ~now; tick 2 still eligible (no Hold so no
    // backoff growth).  Drain runs again.
    let tick2 = app
        .drain_offline_backlog_scheduled(FN, &view)
        .await
        .expect("tick 2 scheduled");
    match tick2 {
        ScheduledDrainOutcome::Ran(_) => {
            // Success: on_advance reset path keeps FN eligible
            // immediately when no Hold accumulates.
        }
        ScheduledDrainOutcome::SkippedBackoff { next_eligible } => {
            panic!(
                "tick 2 after empty-cohort tick 1 MUST be eligible \
                 (backoff::on_advance keeps next_eligible == now); \
                 got SkippedBackoff{{next_eligible={next_eligible:?}}}"
            )
        }
    }
}

// ─── Polish GAP-1: end-to-end Tier1→Tier2→AdminReset→Drain cycle ─────

/// **Polish cycle / GAP-1 (2026-05-25)** — full operator-workflow
/// integration: degradation → STOP_MODE → manual reset → recovery
/// drain.  Locks the complete REC-1 tier-degradation lifecycle as
/// single end-to-end fixture (regression protection if any transition
/// point breaks between releases).
///
/// Scenario:
///   1. Boot App, seed FN + Kvt1 doc + W12 chain prereqs.
///   2. Loop 50 drain ticks з DPS Transport err →
///      `consecutive_holds` counter increments tick-by-tick → Tier 1
///      audit at counter ≥ 10 → Tier 2 STOP_MODE CAS at counter = 50.
///   3. Verify Tier 2 triggered: node_state.mode = STOP_MODE +
///      OFFLINE_DRAIN_FN_STOP_MODE Critical audit emitted exactly 1×.
///   4. Operator-driven recovery via admin reset
///      (`prro::admin::reset_stop_mode`).  Verify: counter reset to 0,
///      mode → GOING_ONLINE, ADMIN_STOP_MODE_RESET Critical audit.
///   5. Switch DPS carrier to lastChk Acked response.
///   6. Drain again → Kvt1Reentry advances doc через Envelope 1b +
///      Envelope 2 → ACK.
///   7. Verify: doc state = ACK, OFFLINE_DRAIN_KVT2_ADVANCED + STAGE_
///      FINALIZE_ACK audits, fresh `consecutive_holds = 0`.
#[tokio::test]
async fn polish_tier_degradation_then_admin_reset_then_drain_succeeds_end_to_end() {
    let (_d, app, pool) = boot_app("polish_e2e.db").await;
    seed_fn_config(&pool).await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;

    // Seed Kvt1 doc (Kvt1Reentry dispatch). server_fiscal_no MUST
    // be present per stage_send 4-b invariant.
    let doc_id = DocumentId::new();
    let req_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO fiscal_documents( \
            document_id, request_id, fiscal_number, shift_id, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, signed_by_cashier_id, \
            offline_session_id, offline_fiscal_no, offline_fiscal_date, \
            server_fiscal_no) \
         VALUES (?, ?, ?, ?, 100, 'SELL', 'KVT1', \
            'b1', 't1', 'OFFLINE', '2026-05-21T00:00:00Z', \
            '{}', ?, ?, ?, 100, '2026-05-21T00:00:00Z', ?)",
    )
    .bind(doc_id)
    .bind(req_id.as_bytes().to_vec())
    .bind(FN)
    .bind(shift_id)
    .bind(vec![0u8; 32])
    .bind(CASHIER_OK)
    .bind(session_id)
    .bind("DPS-FN-E2E")
    .execute(&pool)
    .await
    .unwrap();
    common::init_chain_seed(&pool, FN, common::chain_anchor(0x00))
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc_id,
        common::chain_anchor(0x00),
        common::chain_anchor(0x01),
    )
    .await
    .unwrap();

    // ── Step 1-2: 50 hold ticks ──
    for _ in 0..50 {
        let c = carriers_with_last_chk(
            vec![],
            vec![Err(DpsError::Transport(
                "e2e simulated transport err".into(),
            ))],
        );
        let view = view_for(&c);
        let _ = app
            .drain_offline_backlog_with(FN, &view)
            .await
            .expect("hold tick");
        drop(c);
    }

    // ── Step 3: verify Tier 2 fired ──
    let counter: i64 =
        sqlx::query_scalar("SELECT consecutive_holds FROM fiscal_documents WHERE document_id = ?")
            .bind(doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(counter, 50, "counter MUST reach 50 after 50 hold ticks");
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_FN_STOP_MODE").await,
        1,
        "Tier 2 STOP_MODE escalation MUST fire exactly once at counter==50"
    );
    assert_eq!(
        read_node_mode(&pool).await,
        "STOP_MODE",
        "node_state.mode MUST be STOP_MODE post Tier 2"
    );
    // Tier 1 audits also accumulated (counter 10..=49 = 40 ticks).
    assert_eq!(
        audit_count(&pool, "KVT2_CONFIRM_PROLONGED_HOLD").await,
        40,
        "Tier 1 audits MUST fire on ticks 10..=49 (40 events)"
    );

    // ── Step 4: operator-driven admin reset ──
    let reset_outcome = prro::admin::reset_stop_mode(
        &pool,
        FN,
        "e2e test: DPS connectivity restored; operator verified",
    )
    .await
    .expect("admin reset MUST succeed when mode==STOP_MODE");
    assert_eq!(reset_outcome.fiscal_number, FN);
    assert_eq!(
        reset_outcome.docs_reset_count, 1,
        "1 held doc on FN MUST be reset to consecutive_holds=0"
    );
    assert_eq!(
        read_node_mode(&pool).await,
        "GOING_ONLINE",
        "admin reset MUST transition STOP_MODE → GOING_ONLINE"
    );
    let counter_post_reset: i64 =
        sqlx::query_scalar("SELECT consecutive_holds FROM fiscal_documents WHERE document_id = ?")
            .bind(doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        counter_post_reset, 0,
        "admin reset MUST clear consecutive_holds для all held docs on FN"
    );
    assert_eq!(
        audit_count(&pool, "ADMIN_STOP_MODE_RESET").await,
        1,
        "exactly 1 ADMIN_STOP_MODE_RESET Critical audit MUST emit per reset"
    );

    // Doc state UNCHANGED at KVT1 (admin reset doesn't touch doc state —
    // only counter + node mode).
    let doc_state_post_reset: String =
        sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
            .bind(doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(doc_state_post_reset, "KVT1");

    // ── Step 5-6: switch DPS to Acked + drain ──
    let c_recovery = carriers_with_last_chk(
        vec![],
        vec![Ok(last_chk_ack("DPS-FN-E2E", vec![0xAAu8; 64]))],
    );
    let view_recovery = view_for(&c_recovery);
    let summary_recovery = app
        .drain_offline_backlog_with(FN, &view_recovery)
        .await
        .expect("recovery drain");

    // ── Step 7: verify ACK reached ──
    assert_eq!(
        summary_recovery.advanced_to_ack(),
        1,
        "recovery drain MUST advance doc to ACK via Kvt1Reentry chain"
    );
    let doc_state_final: String =
        sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
            .bind(doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(doc_state_final, "ACK");
    assert!(
        audit_count(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await >= 1,
        "Envelope 1b OFFLINE_DRAIN_KVT2_ADVANCED MUST fire on recovery"
    );
    assert!(
        audit_count(&pool, "STAGE_FINALIZE_ACK").await >= 1,
        "Envelope 2 STAGE_FINALIZE_ACK MUST fire on recovery"
    );
    // Counter remains 0 (Envelope 1b reset_consecutive_holds_tx fires
    // atomically з advance per REC-1 6.1.1 contract).
    let counter_final: i64 =
        sqlx::query_scalar("SELECT consecutive_holds FROM fiscal_documents WHERE document_id = ?")
            .bind(doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(counter_final, 0);

    drop(c_recovery);
}
