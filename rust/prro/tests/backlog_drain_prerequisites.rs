//! W9b Commit 3 — `backlog_drain::drain` prerequisites skeleton.
//!
//! Acceptance for the spec §2.2 prerequisites: mode-check, backlog
//! read, offline-session transition Open → Draining, audit emit.
//! **No per-doc loop yet** — C4 wires that.
//!
//! Tests in order of skip-precedence + happy-path + drift:
//!
//!   1. `c3_skips_when_mode_not_going_online`
//!   2. `c3_skips_when_backlog_empty`
//!   3. `c3_transitions_session_open_to_draining_and_emits_started`
//!   4. `c3_idempotent_when_session_already_draining`
//!   5. `c3_returns_internal_when_backlog_without_active_session`
//!
//! Wire / DPS surface is NOT exercised — `_deps` is unused in C3.
//! The C3 skeleton must NEVER touch the network or crypto (I1), and
//! these tests confirm that by construction: `StubDpsChannel` is
//! built so every method panics; if drain ever reached for the
//! wire the test would fail loudly.

mod common;

use std::sync::Arc;

use prro::db::models::enums::{NodeMode, OfflineSessionState, ShiftState};
use prro::db::models::ids::{DocumentId, OfflineSessionId};
use prro::services::offline_sync::backlog_drain;
use prro::services::reconciliation::runtime::RuntimeView;
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::dto::CheckSignBlob;
use sqlx::SqlitePool;
use uuid::Uuid;

use common::{det_signing_ctx, StubDpsChannel};

const FN: &str = "1234567890";

// ─── Fixture helpers ─────────────────────────────────────────────────

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("w9b_c3.db"))
        .await
        .expect("open_pool runs migrations");
    seed_fn_config(&pool, FN).await;
    (dir, pool)
}

async fn seed_fn_config(pool: &SqlitePool, fn_id: &str) {
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fn_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_node_state(pool: &SqlitePool, fn_id: &str, mode: NodeMode, shift: ShiftState) {
    sqlx::query(
        "INSERT INTO node_state(fiscal_number, mode, shift_state, next_lnd) \
         VALUES (?, ?, ?, 1)",
    )
    .bind(fn_id)
    .bind(mode)
    .bind(shift)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_offline_session(
    pool: &SqlitePool,
    fn_id: &str,
    session_id: OfflineSessionId,
    state: OfflineSessionState,
) {
    seed_offline_session_with_closed_at(pool, fn_id, session_id, state, None).await
}

/// Widened seed for tests that need terminal states (CLOSED / ABORTED).
/// Bypasses the W5 transition whitelist intentionally — used to set up
/// a "stale terminal session" fixture that the runtime would never
/// produce via the service layer.  Production drift defends via FK
/// ON DELETE RESTRICT + W4 partial UNIQUE.
async fn seed_offline_session_with_closed_at(
    pool: &SqlitePool,
    fn_id: &str,
    session_id: OfflineSessionId,
    state: OfflineSessionState,
    closed_at: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at, closed_at) \
         VALUES (?, ?, ?, '2026-05-20T00:00:00Z', ?)",
    )
    .bind(session_id)
    .bind(fn_id)
    .bind(state.as_str())
    .bind(closed_at)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a fiscal_documents row directly in OFFLINE_LOCAL_ACK state.
/// Avoids running the full W7 stage_offline_ack flow — C3 tests only
/// need the drain helper to OBSERVE these rows via the C1 list helper.
async fn seed_offline_local_ack_doc(
    pool: &SqlitePool,
    fn_id: &str,
    lnd: i64,
    code_lnd: i64,
    offline_session_id: OfflineSessionId,
) -> DocumentId {
    let doc_id = DocumentId::new();
    let req_id = Uuid::now_v7();
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents( \
            document_id, request_id, fiscal_number, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, \
            offline_session_id, offline_fiscal_no, offline_fiscal_date \
         ) VALUES ( \
            ?, ?, ?, ?, 'SELL', 'OFFLINE_LOCAL_ACK', \
            'b', 't', 'OFFLINE', '2026-05-20T00:00:00Z', \
            '{}', ?, \
            ?, ?, '2026-05-20T00:00:00Z' \
         )",
    )
    .bind(doc_id)
    .bind(req_id.as_bytes().to_vec())
    .bind(fn_id)
    .bind(lnd)
    .bind(&sha)
    .bind(offline_session_id)
    .bind(code_lnd)
    .execute(pool)
    .await
    .unwrap();
    doc_id
}

async fn fetch_session_state(pool: &SqlitePool, session_id: OfflineSessionId) -> String {
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

fn fn_sign() -> CheckSignBlob {
    CheckSignBlob(vec![0xAB, 0xCD])
}

/// Build a `RuntimeView` over throwaway stub deps.  C3 NEVER uses
/// `_deps`, so the stubs' panic-on-call methods double as a runtime
/// guard against accidental wire/crypto reach (I1).
struct DepsCarriers {
    _dps: Arc<StubDpsChannel>,
    signing_ctx: SigningContext,
    fn_sign: CheckSignBlob,
}

fn build_deps_carriers() -> DepsCarriers {
    // Empty queue: any `send_chk` call panics with "queue empty" —
    // C3 must never reach for this, hence the stub doubles as a
    // negative scanner.
    let dps = Arc::new(StubDpsChannel::with_queue(Vec::new()));
    DepsCarriers {
        _dps: dps,
        signing_ctx: det_signing_ctx(),
        fn_sign: fn_sign(),
    }
}

fn view_for<'a>(carriers: &'a DepsCarriers) -> RuntimeView<'a> {
    RuntimeView {
        dps: carriers._dps.as_ref(),
        signing_ctx: &carriers.signing_ctx,
        fn_sign: &carriers.fn_sign,
    }
}

// ─── Test 1: mode-check skip ─────────────────────────────────────────

#[tokio::test]
async fn c3_skips_when_mode_not_going_online() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN, NodeMode::Offline, ShiftState::Opened).await;

    let carriers = build_deps_carriers();
    let view = view_for(&carriers);
    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(summary.fiscal_number(), FN);
    assert_eq!(summary.backlog_size_before(), 0);
    assert_eq!(summary.advanced_to_ack(), 0);
    assert_eq!(summary.advanced_to_kvt1(), 0);
    assert!(summary.per_doc_failures().is_empty());
    assert!(!summary.finalized());

    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_SKIPPED_NOT_GOING_ONLINE").await,
        1,
        "SKIPPED_NOT_GOING_ONLINE audit must fire exactly once"
    );
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_STARTED").await,
        0,
        "STARTED must NOT fire on prerequisite skip"
    );
    assert_eq!(
        audit_count(&pool, "OFFLINE_SESSION_DRAIN_STARTED").await,
        0,
        "session transition must NOT fire on prerequisite skip"
    );

    // Payload sanity: current_mode echoed for forensics.
    let payload = audit_latest_payload(&pool, "OFFLINE_DRAIN_SKIPPED_NOT_GOING_ONLINE")
        .await
        .expect("payload present");
    assert_eq!(payload["current_mode"], "OFFLINE");
    assert_eq!(payload["fiscal_number"], FN);
}

// ─── Test 2: empty-backlog skip ──────────────────────────────────────

#[tokio::test]
async fn c3_skips_when_backlog_empty() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN, NodeMode::GoingOnline, ShiftState::Opened).await;

    // No OFFLINE_LOCAL_ACK docs seeded.  No active session either —
    // empty-backlog check fires before the session read so this is a
    // valid prerequisite-skip path even without a session row.

    let carriers = build_deps_carriers();
    let view = view_for(&carriers);
    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(summary.backlog_size_before(), 0);
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_SKIPPED_EMPTY_BACKLOG").await,
        1
    );
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_SKIPPED_NOT_GOING_ONLINE").await,
        0,
        "wrong skip class must not fire"
    );
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_STARTED").await, 0);
    assert_eq!(audit_count(&pool, "OFFLINE_SESSION_DRAIN_STARTED").await, 0);
}

// ─── Test 3: happy path — Open → Draining + STARTED audit ────────────

#[tokio::test]
async fn c3_transitions_session_open_to_draining_and_emits_started() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN, NodeMode::GoingOnline, ShiftState::Opened).await;
    let session_id = OfflineSessionId::new();
    seed_offline_session(&pool, FN, session_id, OfflineSessionState::Open).await;
    let _doc_a = seed_offline_local_ack_doc(&pool, FN, 1, 100, session_id).await;
    let _doc_b = seed_offline_local_ack_doc(&pool, FN, 2, 101, session_id).await;

    let carriers = build_deps_carriers();
    let view = view_for(&carriers);
    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    // C3 reports the backlog size but does NOT mutate per-doc counters
    // (per-doc loop deferred to C4); finalized stays false.
    assert_eq!(summary.backlog_size_before(), 2);
    assert_eq!(summary.advanced_to_ack(), 0);
    assert_eq!(summary.advanced_to_kvt1(), 0);
    assert!(!summary.finalized());

    // Session transitioned to DRAINING.
    assert_eq!(
        fetch_session_state(&pool, session_id).await,
        "DRAINING",
        "Open → Draining CAS must have applied"
    );

    // Both audit events emitted, exactly once each.
    assert_eq!(audit_count(&pool, "OFFLINE_SESSION_DRAIN_STARTED").await, 1);
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_STARTED").await, 1);

    let session_hex_expected: String = session_id
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();

    // STARTED payload sanity: backlog_size + session_id present.
    let drain_payload = audit_latest_payload(&pool, "OFFLINE_DRAIN_STARTED")
        .await
        .expect("started payload present");
    assert_eq!(drain_payload["fiscal_number"], FN);
    assert_eq!(drain_payload["backlog_size"], 2);
    assert_eq!(drain_payload["session_id"], session_hex_expected);

    // W5 transition payload sanity: pins the in-tx audit emitted
    // alongside the Open→Draining CAS in `backlog_drain.rs` step 3.
    // Locks the payload schema (`offline_session_id` / `from` / `to`
    // / `reason_abort: null`) so a future drift in the inline emit
    // breaks this test instead of silently shipping a malformed row.
    let session_payload = audit_latest_payload(&pool, "OFFLINE_SESSION_DRAIN_STARTED")
        .await
        .expect("session transition payload present");
    assert_eq!(
        session_payload["offline_session_id"], session_hex_expected,
        "offline_session_id must match the seeded session"
    );
    assert_eq!(session_payload["from"], "OPEN");
    assert_eq!(session_payload["to"], "DRAINING");
    assert!(
        session_payload["reason_abort"].is_null(),
        "reason_abort must be null on a non-Aborted transition; got {:?}",
        session_payload["reason_abort"]
    );
}

// ─── Test 4: re-entry — already DRAINING ─────────────────────────────

#[tokio::test]
async fn c3_idempotent_when_session_already_draining() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN, NodeMode::GoingOnline, ShiftState::Opened).await;
    let session_id = OfflineSessionId::new();
    seed_offline_session(&pool, FN, session_id, OfflineSessionState::Draining).await;
    let _doc = seed_offline_local_ack_doc(&pool, FN, 1, 200, session_id).await;

    let carriers = build_deps_carriers();
    let view = view_for(&carriers);
    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(summary.backlog_size_before(), 1);

    // Session state unchanged (was already DRAINING).
    assert_eq!(fetch_session_state(&pool, session_id).await, "DRAINING");

    // Open→Draining transition audit does NOT fire on re-entry —
    // session was already DRAINING, no CAS attempted.
    assert_eq!(
        audit_count(&pool, "OFFLINE_SESSION_DRAIN_STARTED").await,
        0,
        "OFFLINE_SESSION_DRAIN_STARTED must NOT fire when session was already DRAINING"
    );
    // But the drain itself still emits OFFLINE_DRAIN_STARTED — that
    // event signals the orchestrator entry, not the session transition.
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_STARTED").await, 1);
}

// ─── Test 5: no active session → drain skips with empty-backlog ──────

/// Updated for HIGH-C5-1 (2026-05-21): the walker now scopes to
/// `offline_session_id = ?`, so a stale CLOSED session + doc
/// referencing it cannot leak into the cohort.  Drain skips with
/// SKIPPED_EMPTY_BACKLOG (reason=`"no_active_offline_session"`) +
/// returns Ok(empty summary).  Old C3 contract (Internal error)
/// no longer applies — the new contract is "no active session =
/// no offline cohort = safe empty skip".
#[tokio::test]
async fn c3_drain_skips_when_only_closed_session_exists() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN, NodeMode::GoingOnline, ShiftState::Opened).await;
    let stale_session = OfflineSessionId::new();
    seed_offline_session_with_closed_at(
        &pool,
        FN,
        stale_session,
        OfflineSessionState::Closed,
        Some("2026-05-20T01:00:00Z"),
    )
    .await;
    // Doc references the CLOSED session.  Pre HIGH-C5-1 this caused
    // BootError::Internal; post-fix the walker filters by
    // active_session_id which is absent, so the doc is NOT in the
    // cohort.  Doc remains in OFFLINE_LOCAL_ACK; M3a boot_phase or a
    // future operator-initiated reconciliation handles it.
    let _doc = seed_offline_local_ack_doc(&pool, FN, 1, 300, stale_session).await;

    let carriers = build_deps_carriers();
    let view = view_for(&carriers);
    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .expect("no-active-session path must be Ok(empty summary), not Internal");

    assert_eq!(summary.backlog_size_before(), 0);
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_SKIPPED_EMPTY_BACKLOG").await,
        1,
        "EMPTY_BACKLOG audit must fire — distinguishes safe skip from drift"
    );
    let payload = audit_latest_payload(&pool, "OFFLINE_DRAIN_SKIPPED_EMPTY_BACKLOG")
        .await
        .expect("EMPTY_BACKLOG payload present");
    assert_eq!(
        payload["reason"], "no_active_offline_session",
        "audit payload must carry the distinct reason field for forensic clarity"
    );
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_STARTED").await, 0);
    assert_eq!(audit_count(&pool, "OFFLINE_SESSION_DRAIN_STARTED").await, 0);
}
