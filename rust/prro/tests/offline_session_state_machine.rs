//! W5 acceptance — OfflineSession state machine.
//!
//! Covers the 4 W5 review axes:
//!
//! 1. **State machine discipline**: whitelist enforcement +
//!    `TransitionOutcome::Forbidden` for off-whitelist edges +
//!    `Conflict` / `NotFound` for CAS misses.
//! 2. **Tx discipline**: every mutating call goes through
//!    `with_immediate` (BEGIN IMMEDIATE envelope).  Compile-fail
//!    coverage for the seal already exists at
//!    `tests/write_tx_conn_compile_fail/`; this file's tests
//!    exercise the runtime path.
//! 3. (Code-pool axis covered in `offline_session_code_pool.rs`.)
//! 4. **Typed-error surface**: `AnotherSessionActive` recovered via
//!    `anyhow::Error::downcast_ref::<OfflineSessionError>()`;
//!    `MissingReasonAbort` ditto.
//!
//! Plus W4-compatibility column-population coverage:
//!   - `drained_at` stamped ONLY on `Open → Draining` (operator pin);
//!   - `closed_at` stamped ONLY on `Draining → Closed`, NOT on any
//!     ABORTED-exit (memory `m3b-w5-review-criteria`);
//!   - `reason_abort` required + populated on every ABORTED-exit.

use prro::db::models::enums::OfflineSessionState;
use prro::db::models::ids::OfflineSessionId;
use prro::db::repositories::fiscal_documents::TransitionOutcome;
use prro::db::repositories::offline_sessions::{self, NewOpeningSession, OfflineSessionError};
use prro::db::tx::with_immediate;
use prro::services::offline_session::OfflineSessionService;
use std::sync::Arc;

const FN: &str = "1234567890";

async fn fresh_pool() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs migrations");
    seed_fn(&pool, FN).await;
    (dir, pool)
}

async fn seed_fn(pool: &sqlx::SqlitePool, fiscal_number: &str) {
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fiscal_number)
    .execute(pool)
    .await
    .unwrap();
}

/// Drive `transition_state` from outside `with_immediate` via the
/// explicit envelope (mirrors production composition).
async fn transition_via_envelope(
    pool: &sqlx::SqlitePool,
    session_id: OfflineSessionId,
    from: OfflineSessionState,
    to: OfflineSessionState,
    reason_abort: Option<&str>,
) -> Result<TransitionOutcome, anyhow::Error> {
    let reason_owned = reason_abort.map(|s| s.to_string());
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            offline_sessions::transition_state(tx, session_id, from, to, reason_owned.as_deref())
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
}

async fn insert_opening_via_envelope(
    pool: &sqlx::SqlitePool,
    session_id: OfflineSessionId,
    fiscal_number: &str,
) -> Result<(), anyhow::Error> {
    let fn_owned = fiscal_number.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            offline_sessions::insert_opening(
                tx,
                &NewOpeningSession {
                    offline_session_id: session_id,
                    fiscal_number: &fn_owned,
                    opened_at: "2026-05-15T00:00:00Z",
                },
            )
            .await
            .map_err(anyhow::Error::from)
        })
    })
    .await
}

// ─── Axis 1 — whitelist + TransitionOutcome surface ─────────────────

#[tokio::test]
async fn whitelist_accepts_all_six_legal_edges() {
    let (_d, pool) = fresh_pool().await;
    // Exercise each of the 6 legal edges per `allowed_transition`.
    // Each edge needs its own session — and since the partial
    // UNIQUE `ux_offline_active` permits at most one
    // OPENING/OPEN/DRAINING per FN, every test session needs a
    // distinct FN.  FN CHECK constraint: length=10 + digits-only
    // (per migration 001).
    type Edge = (
        OfflineSessionState,
        OfflineSessionState,
        Option<&'static str>,
    );
    let cases: &[(&str, &[Edge])] = &[
        (
            "1111111111",
            &[(
                OfflineSessionState::Opening,
                OfflineSessionState::Open,
                None,
            )],
        ),
        (
            "2222222222",
            &[(
                OfflineSessionState::Opening,
                OfflineSessionState::Aborted,
                Some("OPENING-aborted"),
            )],
        ),
        (
            "3333333333",
            &[
                (
                    OfflineSessionState::Opening,
                    OfflineSessionState::Open,
                    None,
                ),
                (
                    OfflineSessionState::Open,
                    OfflineSessionState::Aborted,
                    Some("OPEN-aborted"),
                ),
            ],
        ),
        (
            "4444444444",
            &[
                (
                    OfflineSessionState::Opening,
                    OfflineSessionState::Open,
                    None,
                ),
                (
                    OfflineSessionState::Open,
                    OfflineSessionState::Draining,
                    None,
                ),
                (
                    OfflineSessionState::Draining,
                    OfflineSessionState::Closed,
                    None,
                ),
            ],
        ),
        (
            "5555555555",
            &[
                (
                    OfflineSessionState::Opening,
                    OfflineSessionState::Open,
                    None,
                ),
                (
                    OfflineSessionState::Open,
                    OfflineSessionState::Draining,
                    None,
                ),
                (
                    OfflineSessionState::Draining,
                    OfflineSessionState::Aborted,
                    Some("DRAIN failed"),
                ),
            ],
        ),
    ];
    for (fn_id, edge_chain) in cases {
        seed_fn(&pool, fn_id).await;
        let sid = OfflineSessionId::new();
        insert_opening_via_envelope(&pool, sid, fn_id)
            .await
            .unwrap();
        for (from, to, reason) in *edge_chain {
            let outcome = transition_via_envelope(&pool, sid, *from, *to, *reason)
                .await
                .unwrap_or_else(|e| {
                    panic!("edge {from:?}→{to:?} on FN={fn_id} surfaced error: {e:?}")
                });
            assert_eq!(
                outcome,
                TransitionOutcome::Applied,
                "edge {from:?}→{to:?} on FN={fn_id} must Apply"
            );
        }
    }
}

#[tokio::test]
async fn whitelist_rejects_forbidden_edges_via_transition_outcome() {
    let (_d, pool) = fresh_pool().await;
    let sid = OfflineSessionId::new();
    insert_opening_via_envelope(&pool, sid, FN).await.unwrap();
    // OPENING → CLOSED (skip OPEN/DRAINING) — forbidden.
    let outcome = transition_via_envelope(
        &pool,
        sid,
        OfflineSessionState::Opening,
        OfflineSessionState::Closed,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        outcome,
        TransitionOutcome::Forbidden,
        "Opening→Closed must Forbidden"
    );
    // CLOSED → OPEN — forbidden (no reset).
    let outcome = transition_via_envelope(
        &pool,
        sid,
        OfflineSessionState::Closed,
        OfflineSessionState::Open,
        None,
    )
    .await
    .unwrap();
    assert_eq!(outcome, TransitionOutcome::Forbidden);
    // ABORTED → DRAINING — forbidden (no restart).
    let outcome = transition_via_envelope(
        &pool,
        sid,
        OfflineSessionState::Aborted,
        OfflineSessionState::Draining,
        Some("ignored"),
    )
    .await
    .unwrap();
    assert_eq!(outcome, TransitionOutcome::Forbidden);
}

#[tokio::test]
async fn transition_returns_not_found_for_unknown_session() {
    let (_d, pool) = fresh_pool().await;
    let unknown = OfflineSessionId::new();
    let outcome = transition_via_envelope(
        &pool,
        unknown,
        OfflineSessionState::Opening,
        OfflineSessionState::Open,
        None,
    )
    .await
    .unwrap();
    assert_eq!(outcome, TransitionOutcome::NotFound);
}

#[tokio::test]
async fn transition_returns_conflict_when_state_diverged() {
    let (_d, pool) = fresh_pool().await;
    let sid = OfflineSessionId::new();
    insert_opening_via_envelope(&pool, sid, FN).await.unwrap();
    // Promote to OPEN.
    transition_via_envelope(
        &pool,
        sid,
        OfflineSessionState::Opening,
        OfflineSessionState::Open,
        None,
    )
    .await
    .unwrap();
    // Now try Opening→Open again — row exists but state diverged.
    let outcome = transition_via_envelope(
        &pool,
        sid,
        OfflineSessionState::Opening,
        OfflineSessionState::Open,
        None,
    )
    .await
    .unwrap();
    assert_eq!(outcome, TransitionOutcome::Conflict);
}

// ─── Axis 2 — column-population semantics on the right transition ──

#[tokio::test]
async fn draining_entry_stamps_drained_at_only_on_open_to_draining() {
    let (_d, pool) = fresh_pool().await;
    let sid = OfflineSessionId::new();
    insert_opening_via_envelope(&pool, sid, FN).await.unwrap();
    transition_via_envelope(
        &pool,
        sid,
        OfflineSessionState::Opening,
        OfflineSessionState::Open,
        None,
    )
    .await
    .unwrap();
    // drained_at must be NULL before DRAINING entry.
    let pre: Option<String> =
        sqlx::query_scalar("SELECT drained_at FROM offline_sessions WHERE offline_session_id = ?")
            .bind(sid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(pre.is_none(), "drained_at must be NULL pre-DRAINING entry");
    transition_via_envelope(
        &pool,
        sid,
        OfflineSessionState::Open,
        OfflineSessionState::Draining,
        None,
    )
    .await
    .unwrap();
    let post: Option<String> =
        sqlx::query_scalar("SELECT drained_at FROM offline_sessions WHERE offline_session_id = ?")
            .bind(sid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        post.is_some(),
        "drained_at must be stamped on DRAINING entry"
    );
}

#[tokio::test]
async fn closed_entry_stamps_closed_at_only_on_draining_to_closed() {
    let (_d, pool) = fresh_pool().await;
    let sid = OfflineSessionId::new();
    insert_opening_via_envelope(&pool, sid, FN).await.unwrap();
    transition_via_envelope(
        &pool,
        sid,
        OfflineSessionState::Opening,
        OfflineSessionState::Open,
        None,
    )
    .await
    .unwrap();
    transition_via_envelope(
        &pool,
        sid,
        OfflineSessionState::Open,
        OfflineSessionState::Draining,
        None,
    )
    .await
    .unwrap();
    let pre: Option<String> =
        sqlx::query_scalar("SELECT closed_at FROM offline_sessions WHERE offline_session_id = ?")
            .bind(sid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(pre.is_none(), "closed_at must be NULL pre-CLOSED entry");
    transition_via_envelope(
        &pool,
        sid,
        OfflineSessionState::Draining,
        OfflineSessionState::Closed,
        None,
    )
    .await
    .unwrap();
    let post: Option<String> =
        sqlx::query_scalar("SELECT closed_at FROM offline_sessions WHERE offline_session_id = ?")
            .bind(sid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(post.is_some(), "closed_at must be stamped on CLOSED entry");
}

#[tokio::test]
async fn aborted_exit_does_not_stamp_closed_at_only_reason_abort() {
    let (_d, pool) = fresh_pool().await;
    let sid = OfflineSessionId::new();
    insert_opening_via_envelope(&pool, sid, FN).await.unwrap();
    transition_via_envelope(
        &pool,
        sid,
        OfflineSessionState::Opening,
        OfflineSessionState::Open,
        None,
    )
    .await
    .unwrap();
    // Open → Aborted with reason.
    let outcome = transition_via_envelope(
        &pool,
        sid,
        OfflineSessionState::Open,
        OfflineSessionState::Aborted,
        Some("operator manual abort"),
    )
    .await
    .unwrap();
    assert_eq!(outcome, TransitionOutcome::Applied);
    // closed_at MUST remain NULL — ABORTED ≠ CLOSED.
    let closed_at: Option<String> =
        sqlx::query_scalar("SELECT closed_at FROM offline_sessions WHERE offline_session_id = ?")
            .bind(sid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        closed_at.is_none(),
        "closed_at must remain NULL on ABORTED exit — got {closed_at:?}"
    );
    // reason_abort MUST be populated.
    let reason: Option<String> = sqlx::query_scalar(
        "SELECT reason_abort FROM offline_sessions WHERE offline_session_id = ?",
    )
    .bind(sid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(reason.as_deref(), Some("operator manual abort"));
    // state actually changed to ABORTED.
    let state: String =
        sqlx::query_scalar("SELECT state FROM offline_sessions WHERE offline_session_id = ?")
            .bind(sid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, "ABORTED");
}

// ─── Axis 4 — typed-error surface ───────────────────────────────────

#[tokio::test]
async fn aborted_transition_without_reason_returns_typed_missing_reason_abort() {
    let (_d, pool) = fresh_pool().await;
    let sid = OfflineSessionId::new();
    insert_opening_via_envelope(&pool, sid, FN).await.unwrap();
    // None reason.
    let err = transition_via_envelope(
        &pool,
        sid,
        OfflineSessionState::Opening,
        OfflineSessionState::Aborted,
        None,
    )
    .await
    .unwrap_err();
    let typed = err.downcast_ref::<OfflineSessionError>();
    assert!(
        matches!(typed, Some(OfflineSessionError::MissingReasonAbort)),
        "expected MissingReasonAbort for None reason; got: {err:?}"
    );
    // Empty-string reason — same outcome.
    let err = transition_via_envelope(
        &pool,
        sid,
        OfflineSessionState::Opening,
        OfflineSessionState::Aborted,
        Some(""),
    )
    .await
    .unwrap_err();
    let typed = err.downcast_ref::<OfflineSessionError>();
    assert!(
        matches!(typed, Some(OfflineSessionError::MissingReasonAbort)),
        "expected MissingReasonAbort for empty-string reason; got: {err:?}"
    );
}

#[tokio::test]
async fn second_active_session_on_same_fn_surfaces_typed_another_session_active() {
    let (_d, pool) = fresh_pool().await;
    let sid1 = OfflineSessionId::new();
    insert_opening_via_envelope(&pool, sid1, FN).await.unwrap();
    // Second OPENING on same FN — partial UNIQUE ux_offline_active fires.
    let sid2 = OfflineSessionId::new();
    let err = insert_opening_via_envelope(&pool, sid2, FN)
        .await
        .unwrap_err();
    let typed = err.downcast_ref::<OfflineSessionError>();
    match typed {
        Some(OfflineSessionError::AnotherSessionActive { fiscal_number }) => {
            assert_eq!(fiscal_number, FN);
        }
        other => panic!("expected AnotherSessionActive, got: {other:?} (full err: {err:?})"),
    }
}

#[tokio::test]
async fn closed_session_does_not_block_new_active_session_on_same_fn() {
    let (_d, pool) = fresh_pool().await;
    let sid1 = OfflineSessionId::new();
    insert_opening_via_envelope(&pool, sid1, FN).await.unwrap();
    transition_via_envelope(
        &pool,
        sid1,
        OfflineSessionState::Opening,
        OfflineSessionState::Open,
        None,
    )
    .await
    .unwrap();
    transition_via_envelope(
        &pool,
        sid1,
        OfflineSessionState::Open,
        OfflineSessionState::Draining,
        None,
    )
    .await
    .unwrap();
    transition_via_envelope(
        &pool,
        sid1,
        OfflineSessionState::Draining,
        OfflineSessionState::Closed,
        None,
    )
    .await
    .unwrap();
    // First session is CLOSED — new session on same FN must succeed
    // (partial UNIQUE excludes CLOSED/ABORTED per W4 design).
    let sid2 = OfflineSessionId::new();
    insert_opening_via_envelope(&pool, sid2, FN)
        .await
        .expect("CLOSED session must not block a new OPENING on same FN");
}

#[tokio::test]
async fn aborted_session_does_not_block_new_active_session_on_same_fn() {
    let (_d, pool) = fresh_pool().await;
    let sid1 = OfflineSessionId::new();
    insert_opening_via_envelope(&pool, sid1, FN).await.unwrap();
    transition_via_envelope(
        &pool,
        sid1,
        OfflineSessionState::Opening,
        OfflineSessionState::Aborted,
        Some("test"),
    )
    .await
    .unwrap();
    let sid2 = OfflineSessionId::new();
    insert_opening_via_envelope(&pool, sid2, FN)
        .await
        .expect("ABORTED session must not block new OPENING on same FN");
}

// ─── Service-layer composition + audit ──────────────────────────────

#[tokio::test]
async fn service_open_session_inserts_promotes_open_and_emits_audit() {
    let (_d, pool) = fresh_pool().await;
    let svc = OfflineSessionService::new(Arc::new(pool.clone()));
    let sid = svc.open_session(FN).await.unwrap();
    // State is OPEN (open_session does INSERT + Opening→Open in one tx).
    let state: String =
        sqlx::query_scalar("SELECT state FROM offline_sessions WHERE offline_session_id = ?")
            .bind(sid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, "OPEN");
    // Audit row emitted for OFFLINE_SESSION_OPENED.
    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log \
         WHERE entity_type = 'offline_session' AND event_type = 'OFFLINE_SESSION_OPENED'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        event_count, 1,
        "exactly one OFFLINE_SESSION_OPENED audit row"
    );
}

#[tokio::test]
async fn service_drain_close_emit_audit_chain() {
    let (_d, pool) = fresh_pool().await;
    let svc = OfflineSessionService::new(Arc::new(pool.clone()));
    let sid = svc.open_session(FN).await.unwrap();
    let out = svc.start_drain(sid).await.unwrap();
    assert_eq!(out, TransitionOutcome::Applied);
    let out = svc.close_session(sid).await.unwrap();
    assert_eq!(out, TransitionOutcome::Applied);
    // Three audit rows in sequence: OPENED, DRAIN_STARTED, CLOSED.
    let events: Vec<String> = sqlx::query_scalar(
        "SELECT event_type FROM audit_log \
         WHERE entity_type = 'offline_session' \
         ORDER BY audit_id ASC",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        events,
        vec![
            "OFFLINE_SESSION_OPENED",
            "OFFLINE_SESSION_DRAIN_STARTED",
            "OFFLINE_SESSION_CLOSED"
        ]
    );
}

#[tokio::test]
async fn service_abort_session_emits_audit_with_reason() {
    let (_d, pool) = fresh_pool().await;
    let svc = OfflineSessionService::new(Arc::new(pool.clone()));
    let sid = svc.open_session(FN).await.unwrap();
    let out = svc
        .abort_session(sid, OfflineSessionState::Open, "shift cutoff")
        .await
        .unwrap();
    assert_eq!(out, TransitionOutcome::Applied);
    // Audit row with WARNING severity + payload carrying from/to.
    let row: (String, String, Option<String>) = sqlx::query_as(
        "SELECT event_type, severity, event_payload_json FROM audit_log \
         WHERE entity_type = 'offline_session' \
           AND event_type = 'OFFLINE_SESSION_ABORTED'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "OFFLINE_SESSION_ABORTED");
    assert_eq!(row.1, "WARNING");
    let payload = row.2.expect("audit payload must be present");
    assert!(
        payload.contains("OPEN"),
        "payload must mention from-state: {payload}"
    );
    assert!(
        payload.contains("ABORTED"),
        "payload must mention to-state: {payload}"
    );
}

#[tokio::test]
async fn service_open_session_second_active_returns_typed_another_session_active() {
    let (_d, pool) = fresh_pool().await;
    let svc = OfflineSessionService::new(Arc::new(pool.clone()));
    let _sid1 = svc.open_session(FN).await.unwrap();
    let err = svc.open_session(FN).await.unwrap_err();
    let typed = err.downcast_ref::<OfflineSessionError>();
    match typed {
        Some(OfflineSessionError::AnotherSessionActive { fiscal_number }) => {
            assert_eq!(fiscal_number, FN);
        }
        other => panic!(
            "expected typed AnotherSessionActive after service::open_session race; \
             got: {other:?} (full err: {err:?})"
        ),
    }
}
