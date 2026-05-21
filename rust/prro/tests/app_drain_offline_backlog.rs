//! W9b Commit 7 — `App::drain_offline_backlog_with` integration tests.
//!
//! Acceptance for spec §2.1 (a) + §10 C7: App-owned entry that
//! acquires the App reconcile mutex (W2 enforcement per ADR-M3-A10)
//! and delegates to the pure-function `backlog_drain::drain`.
//!
//! Two integration tests:
//!
//!   1. `app_drain_skip_path_mode_not_going_online` — boots App,
//!      seeds FN with `node_state.mode = Offline`, calls
//!      `App::drain_offline_backlog_with`, asserts skip via
//!      `SKIPPED_NOT_GOING_ONLINE` audit + empty summary.
//!   2. `app_drain_partial_path_pre_w12_steady_state` — boots App,
//!      seeds GoingOnline + Open session + 2 OFFLINE_LOCAL_ACK docs,
//!      calls `App::drain_offline_backlog_with`, asserts pre-W12
//!      stub flow: per-doc loop advances both as DeferredKvt1 →
//!      finalize blocked → `OFFLINE_DRAIN_PARTIAL` audit; node stays
//!      GoingOnline, session stays Draining.
//!
//! Eligible-arm full-flow integration via the public entry is
//! **deferred to W12 PR** — pre-W12 the C5 stub
//! `apply_w12_confirmation` always returns `DeferredKvt1`.  The
//! Eligible-arm CAS chain + audit shape are covered by the inline
//! `eligible_arm_tests` in `backlog_drain.rs` (calls
//! `commit_finalize_envelope` directly via crate-internal access).

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
db_path = "{}"

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
    sqlx::query(
        "INSERT INTO fiscal_documents( \
            document_id, request_id, fiscal_number, shift_id, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, signed_by_cashier_id, \
            offline_session_id, offline_fiscal_no, offline_fiscal_date \
         ) VALUES ( \
            ?, ?, ?, ?, ?, 'SELL', 'OFFLINE_LOCAL_ACK', \
            'b', 't', 'OFFLINE', '2026-05-21T00:00:00Z', \
            '{}', ?, ?, \
            ?, ?, '2026-05-21T00:00:00Z' \
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

// ─── Test 2: partial path — full pre-W12 steady-state flow ──────────

#[tokio::test]
async fn app_drain_partial_path_pre_w12_steady_state() {
    let (_d, app, pool) = boot_app("c7_partial.db").await;
    seed_fn_config(&pool).await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let _doc_a = seed_offline_local_ack(&pool, 1, 100, session_id, shift_id).await;
    let _doc_b = seed_offline_local_ack(&pool, 2, 101, session_id, shift_id).await;

    let c = carriers(vec![Ok(ack("DPS-FN-A")), Ok(ack("DPS-FN-B"))]);
    let view = view_for(&c);

    let summary = app
        .drain_offline_backlog_with(FN, &view)
        .await
        .expect("App entry must return Ok on partial flow");

    // Both docs advanced to KVT1 via stub; finalize blocked by
    // DocsDeferredAtKvt1.
    assert_eq!(summary.backlog_size_before(), 2);
    assert_eq!(summary.advanced_to_kvt1(), 2);
    assert_eq!(summary.advanced_to_ack(), 0);
    assert!(
        !summary.finalized(),
        "pre-W12 stub MUST NOT finalize (operator-pinned invariant)"
    );

    // Full audit chain: STARTED + SESSION_DRAIN_STARTED + 2 DOC_ADVANCED
    // + PARTIAL.
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_STARTED").await, 1);
    assert_eq!(audit_count(&pool, "OFFLINE_SESSION_DRAIN_STARTED").await, 1);
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_ADVANCED").await, 2);
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_PARTIAL").await, 1);

    // Finalize did NOT fire — node + session stay pre-drain.
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_COMPLETED").await, 0);
    assert_eq!(audit_count(&pool, "OFFLINE_SESSION_CLOSED").await, 0);
    assert_eq!(read_node_mode(&pool).await, "GOING_ONLINE");
    assert_eq!(read_session_state(&pool, session_id).await, "DRAINING");

    // Wire stubs consumed.
    assert_eq!(c.dps.call_count(), 2);
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
