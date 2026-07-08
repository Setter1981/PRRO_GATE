//! W7 acceptance — `stage_offline_ack` (Pattern C step 1).
//!
//! Covers the 6 W7 review axes (memory `m3b-w7-review-criteria`):
//!
//! 1. stage_finalize untouched — verified by leaving its test
//!    suite intact (separate file, not modified by W7).
//! 2. No DPS / network / crypto in offline branch — verified by
//!    construction (stage_offline_ack module imports zero
//!    substrate modules) and by I1's `IN_WITH_IMMEDIATE` runtime
//!    guard which would panic if a substrate call leaked.
//! 3. `acquire_code_tx` + transition in SAME `with_immediate` —
//!    verified by `applied_happy_path_atomically_consumes_code_and_transitions`
//!    + the rollback fixtures (`conflict_rolls_back_code_consumption`).
//! 4. Shift / node-mode refusal typed; doc + code untouched —
//!    covered by `refusal_*` fixtures.
//! 5. `offline_fiscal_no = code_lnd`, `offline_fiscal_date =
//!    consumed_at`, audit only on Applied — covered by
//!    `applied_*` + `refusal_*` audit assertions.
//! 6. No runtime use of `(OfflineLocalAck, Sending|Cancelled)` —
//!    verified by grep over `src/` in the PR review (not testable
//!    in isolation; pinned at review).

use prro::db::models::enums::{NodeMode, OfflineSessionState, ShiftState};
use prro::db::models::ids::{DocumentId, OfflineSessionId, ShiftId};
use prro::services::write_path::stage_offline_ack::{self, OfflineAckOutcome, RefusalReason};
use uuid::Uuid;

const FN: &str = "1234567890";

async fn fresh_pool() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs migrations");
    seed_fn(&pool, FN).await;
    (dir, pool)
}

async fn seed_fn(pool: &sqlx::SqlitePool, fn_id: &str) {
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fn_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a `node_state` row with the given mode + shift_state.
/// Mirrors what `app::boot` does on first startup.
async fn seed_node_state(
    pool: &sqlx::SqlitePool,
    fn_id: &str,
    mode: NodeMode,
    shift_state: ShiftState,
) {
    sqlx::query(
        "INSERT INTO node_state(fiscal_number, mode, shift_state, next_lnd) \
         VALUES (?, ?, ?, 1)",
    )
    .bind(fn_id)
    .bind(mode)
    .bind(shift_state)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed an offline_session row directly (bypassing the W5 service)
/// because tests need precise state control (some refusal cases
/// require OPENING / DRAINING states the service doesn't expose).
async fn seed_offline_session(
    pool: &sqlx::SqlitePool,
    fn_id: &str,
    session_id: OfflineSessionId,
    state: OfflineSessionState,
) {
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, ?, '2026-05-15T00:00:00Z')",
    )
    .bind(session_id)
    .bind(fn_id)
    .bind(state.as_str())
    .execute(pool)
    .await
    .unwrap();
}

/// Seed an unconsumed offline_codes row with a synthetic dps_code.
///
/// B8-1: `acquire_code_tx` now requires `dps_code IS NOT NULL`.  Drill rows
/// seeded for tests use synthetic codes of the form `"DRILL-{code_lnd}"` so
/// they are acquired correctly by the B8-updated path.
async fn seed_code(pool: &sqlx::SqlitePool, fn_id: &str, code_lnd: i64) {
    let dps_code = format!("DRILL-{code_lnd}");
    sqlx::query(
        "INSERT INTO offline_codes(fiscal_number, code_lnd, dps_code) \
         VALUES (?, ?, ?)",
    )
    .bind(fn_id)
    .bind(code_lnd)
    .bind(&dps_code)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a fiscal_documents row in SIGNED state.  Returns the
/// generated `DocumentId`.
async fn insert_signed_doc(pool: &sqlx::SqlitePool, fn_id: &str, lnd: i64) -> DocumentId {
    let doc_id = DocumentId::new();
    let req_id = Uuid::now_v7();
    let sha = vec![0u8; 32];
    // M2-01: a SIGNED doc carries `unsigned_xml_sha256` (set at stage_sign);
    // stage_offline_ack now reads it to advance the MAC chain seed.  `previous_hash`
    // is left NULL (genesis) to match the test's genesis node seed, so the
    // step-7b drift-assert (`ns_seed == previous_hash`) passes.  A distinct
    // per-lnd unsigned hash keeps multi-doc seeds non-colliding.
    let unsigned = vec![0xA0u8.wrapping_add(lnd as u8); 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, unsigned_xml_sha256) \
         VALUES (?, ?, ?, ?, 'SELL', 'SIGNED', 'b', 't', 'OFFLINE', \
            '2026-05-15T00:00:00Z', '{}', ?, ?)",
    )
    .bind(doc_id)
    .bind(req_id.as_bytes().to_vec())
    .bind(fn_id)
    .bind(lnd)
    .bind(&sha)
    .bind(&unsigned)
    .execute(pool)
    .await
    .unwrap();
    doc_id
}

/// B9: simulate `stage_sign`'s conditional stamp-at-sign on an already-inserted
/// SIGNED offline doc — the ONLY path that now legitimately reaches offline-ack
/// as an `Applied` (readback).  Consumes `code_lnd` by the doc (as acquire_code_tx
/// would at sign) AND stamps the doc's offline columns (offline_fiscal_no =
/// code_lnd, offline_dps_code = the code's dps_code, offline_session_id).  After
/// this, `stage_offline_ack::run` takes the readback branch → OFFLINE_LOCAL_ACK.
async fn mark_doc_stamped_at_sign(
    pool: &sqlx::SqlitePool,
    doc_id: DocumentId,
    fn_id: &str,
    code_lnd: i64,
    session_id: OfflineSessionId,
) {
    // Consume the code by the doc (mirrors acquire_code_tx at sign).
    sqlx::query(
        "UPDATE offline_codes SET consumed_at = CURRENT_TIMESTAMP, consumed_by_document_id = ? \
         WHERE fiscal_number = ? AND code_lnd = ?",
    )
    .bind(doc_id)
    .bind(fn_id)
    .bind(code_lnd)
    .execute(pool)
    .await
    .unwrap();
    // Stamp the doc's offline columns (mirrors stamp_offline_issuance_tx at sign).
    let dps_code: String = sqlx::query_scalar(
        "SELECT dps_code FROM offline_codes WHERE fiscal_number = ? AND code_lnd = ?",
    )
    .bind(fn_id)
    .bind(code_lnd)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE fiscal_documents SET offline_fiscal_no = ?, \
             offline_fiscal_date = CURRENT_TIMESTAMP, offline_session_id = ?, \
             offline_dps_code = ? WHERE document_id = ?",
    )
    .bind(code_lnd)
    .bind(session_id)
    .bind(&dps_code)
    .bind(doc_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn fetch_doc_state(pool: &sqlx::SqlitePool, doc_id: DocumentId) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(doc_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn fetch_consumed_count(pool: &sqlx::SqlitePool, fn_id: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ? AND consumed_at IS NOT NULL",
    )
    .bind(fn_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn fetch_audit_events(
    pool: &sqlx::SqlitePool,
    doc_id: DocumentId,
) -> Vec<(String, String, Option<String>)> {
    let doc_hex = doc_id
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    sqlx::query_as(
        "SELECT event_type, severity, event_payload_json FROM audit_log \
         WHERE entity_type = 'fiscal_document' AND entity_id = ? \
         ORDER BY audit_id ASC",
    )
    .bind(doc_hex)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Setup for the "happy" environment (Offline mode, Opened shift,
/// one OPEN session, one seeded code, one SIGNED doc).  Returns
/// the session_id and doc_id for assertion convenience.
async fn setup_offline_environment(pool: &sqlx::SqlitePool) -> (OfflineSessionId, DocumentId) {
    seed_node_state(pool, FN, NodeMode::Offline, ShiftState::Opened).await;
    // node_state also wants current_shift_id non-null; for W7 we
    // only check shift_state in node_state, so leaving NULL is OK
    // for these tests.
    let session_id = OfflineSessionId::new();
    seed_offline_session(pool, FN, session_id, OfflineSessionState::Open).await;
    seed_code(pool, FN, 42).await;
    let doc_id = insert_signed_doc(pool, FN, 1).await;
    // B9: the doc is stamped-at-sign (readback path — the only path that now
    // reaches offline-ack as Applied).  Consumes code 42 by the doc + stamps
    // its offline columns, so `stage_offline_ack::run` takes the readback CAS.
    mark_doc_stamped_at_sign(pool, doc_id, FN, 42, session_id).await;
    (session_id, doc_id)
}

// ─── Happy path ─────────────────────────────────────────────────────

#[tokio::test]
async fn applied_happy_path_atomically_consumes_code_and_transitions() {
    let (_d, pool) = fresh_pool().await;
    let (session_id, doc_id) = setup_offline_environment(&pool).await;

    let outcome = stage_offline_ack::run(&pool, doc_id, FN).await.unwrap();

    match outcome {
        OfflineAckOutcome::Applied {
            document_id,
            code_lnd,
            consumed_at,
            offline_session_id,
        } => {
            assert_eq!(document_id, doc_id);
            assert_eq!(code_lnd, 42);
            assert!(!consumed_at.is_empty());
            assert_eq!(offline_session_id, session_id);
        }
        other => panic!("expected Applied, got: {other:?}"),
    }

    // Doc state flipped.
    assert_eq!(fetch_doc_state(&pool, doc_id).await, "OFFLINE_LOCAL_ACK");

    // offline_fiscal_no / _date / offline_session_id populated on doc.
    let row: (Option<i64>, Option<String>, Option<Vec<u8>>) = sqlx::query_as(
        "SELECT offline_fiscal_no, offline_fiscal_date, offline_session_id \
         FROM fiscal_documents WHERE document_id = ?",
    )
    .bind(doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, Some(42), "offline_fiscal_no = code_lnd");
    assert!(row.1.is_some(), "offline_fiscal_date populated");
    assert_eq!(
        row.2.as_deref(),
        Some(&session_id.as_bytes()[..]),
        "offline_session_id = active session"
    );

    // Code consumed + linked to doc.
    assert_eq!(fetch_consumed_count(&pool, FN).await, 1);
    let consumed_by: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT consumed_by_document_id FROM offline_codes WHERE fiscal_number = ? AND code_lnd = 42",
    )
    .bind(FN)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(consumed_by.as_deref(), Some(&doc_id.as_bytes()[..]));

    // Audit emitted exactly once with the right shape.
    let events = fetch_audit_events(&pool, doc_id).await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "OFFLINE_LOCAL_ACK_APPLIED");
    assert_eq!(events[0].1, "INFO");
    let payload: serde_json::Value = serde_json::from_str(events[0].2.as_ref().unwrap()).unwrap();
    assert_eq!(payload["code_lnd"], 42);
    assert!(!payload["consumed_at"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn applied_path_works_with_going_offline_mode() {
    // GoingOffline is the transitional mode (operator-initiated
    // offline switch); the offline branch must accept it as well.
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN, NodeMode::GoingOffline, ShiftState::Opened).await;
    let session_id = OfflineSessionId::new();
    seed_offline_session(&pool, FN, session_id, OfflineSessionState::Open).await;
    seed_code(&pool, FN, 1).await;
    let doc_id = insert_signed_doc(&pool, FN, 1).await;
    mark_doc_stamped_at_sign(&pool, doc_id, FN, 1, session_id).await;

    let outcome = stage_offline_ack::run(&pool, doc_id, FN).await.unwrap();
    assert!(matches!(outcome, OfflineAckOutcome::Applied { .. }));
    assert_eq!(fetch_doc_state(&pool, doc_id).await, "OFFLINE_LOCAL_ACK");
}

// ─── Node-mode refusals ─────────────────────────────────────────────

#[tokio::test]
async fn refusal_node_online_leaves_doc_and_code_untouched() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN, NodeMode::Online, ShiftState::Opened).await;
    let session_id = OfflineSessionId::new();
    seed_offline_session(&pool, FN, session_id, OfflineSessionState::Open).await;
    seed_code(&pool, FN, 1).await;
    let doc_id = insert_signed_doc(&pool, FN, 1).await;

    let outcome = stage_offline_ack::run(&pool, doc_id, FN).await.unwrap();
    match outcome {
        OfflineAckOutcome::Refused(RefusalReason::NodeNotOffline { mode }) => {
            assert_eq!(mode, NodeMode::Online);
        }
        other => panic!("expected NodeNotOffline(Online), got: {other:?}"),
    }

    // Doc state unchanged.
    assert_eq!(fetch_doc_state(&pool, doc_id).await, "SIGNED");
    // Code untouched.
    assert_eq!(fetch_consumed_count(&pool, FN).await, 0);

    // Refusal audit emitted.
    let events = fetch_audit_events(&pool, doc_id).await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "OFFLINE_ACK_REFUSED");
    assert_eq!(events[0].1, "WARNING");
    let payload: serde_json::Value = serde_json::from_str(events[0].2.as_ref().unwrap()).unwrap();
    assert!(payload["reason"]
        .as_str()
        .unwrap()
        .contains("NodeNotOffline"));
}

#[tokio::test]
async fn refusal_node_blocked_returns_typed_node_not_offline() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN, NodeMode::Blocked, ShiftState::Opened).await;
    let session_id = OfflineSessionId::new();
    seed_offline_session(&pool, FN, session_id, OfflineSessionState::Open).await;
    seed_code(&pool, FN, 1).await;
    let doc_id = insert_signed_doc(&pool, FN, 1).await;
    let outcome = stage_offline_ack::run(&pool, doc_id, FN).await.unwrap();
    assert!(matches!(
        outcome,
        OfflineAckOutcome::Refused(RefusalReason::NodeNotOffline {
            mode: NodeMode::Blocked
        })
    ));
    assert_eq!(fetch_consumed_count(&pool, FN).await, 0);
}

#[tokio::test]
async fn refusal_node_going_online_returns_typed_node_not_offline() {
    // GoingOnline = W9 backlog-drain mode; offline ack must refuse.
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN, NodeMode::GoingOnline, ShiftState::Opened).await;
    let session_id = OfflineSessionId::new();
    seed_offline_session(&pool, FN, session_id, OfflineSessionState::Open).await;
    seed_code(&pool, FN, 1).await;
    let doc_id = insert_signed_doc(&pool, FN, 1).await;
    let outcome = stage_offline_ack::run(&pool, doc_id, FN).await.unwrap();
    assert!(matches!(
        outcome,
        OfflineAckOutcome::Refused(RefusalReason::NodeNotOffline {
            mode: NodeMode::GoingOnline
        })
    ));
    assert_eq!(fetch_consumed_count(&pool, FN).await, 0);
}

// ─── Shift refusals ─────────────────────────────────────────────────

#[tokio::test]
async fn refusal_shift_closed_returns_typed_shift_not_opened() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN, NodeMode::Offline, ShiftState::Closed).await;
    let session_id = OfflineSessionId::new();
    seed_offline_session(&pool, FN, session_id, OfflineSessionState::Open).await;
    seed_code(&pool, FN, 1).await;
    let doc_id = insert_signed_doc(&pool, FN, 1).await;
    let outcome = stage_offline_ack::run(&pool, doc_id, FN).await.unwrap();
    match outcome {
        OfflineAckOutcome::Refused(RefusalReason::ShiftNotOpened { current }) => {
            assert_eq!(current, ShiftState::Closed);
        }
        other => panic!("expected ShiftNotOpened(Closed), got: {other:?}"),
    }
    assert_eq!(fetch_doc_state(&pool, doc_id).await, "SIGNED");
    assert_eq!(fetch_consumed_count(&pool, FN).await, 0);
}

// ─── Session refusals ───────────────────────────────────────────────

#[tokio::test]
async fn refusal_no_open_session_returns_typed_no_active_session() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN, NodeMode::Offline, ShiftState::Opened).await;
    // No offline_session row at all.
    seed_code(&pool, FN, 1).await;
    let doc_id = insert_signed_doc(&pool, FN, 1).await;
    let outcome = stage_offline_ack::run(&pool, doc_id, FN).await.unwrap();
    assert!(matches!(
        outcome,
        OfflineAckOutcome::Refused(RefusalReason::NoActiveSession)
    ));
    assert_eq!(fetch_consumed_count(&pool, FN).await, 0);
}

#[tokio::test]
async fn refusal_draining_session_returns_no_active_session() {
    // DRAINING session is a W9 backlog-drain state; W7 must refuse.
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN, NodeMode::Offline, ShiftState::Opened).await;
    let session_id = OfflineSessionId::new();
    seed_offline_session(&pool, FN, session_id, OfflineSessionState::Draining).await;
    seed_code(&pool, FN, 1).await;
    let doc_id = insert_signed_doc(&pool, FN, 1).await;
    let outcome = stage_offline_ack::run(&pool, doc_id, FN).await.unwrap();
    assert!(matches!(
        outcome,
        OfflineAckOutcome::Refused(RefusalReason::NoActiveSession)
    ));
    assert_eq!(fetch_consumed_count(&pool, FN).await, 0);
}

// ─── Unstamped-at-sign → abort (B9 merge-blocker fix) ───────────────
//
// B9 replaced the offline-ack acquire fallback with an abort: a SIGNED offline
// doc that was NOT stamped at sign (`offline_dps_code IS NULL`) carries a BARE
// `<MAC>` and can never validly drain, so it MUST abort (SIGNED→Aborted) rather
// than acquire a code and become a drainable OFFLINE_LOCAL_ACK.  This is the
// successor to the former `empty_code_pool_returns_typed_code_pool_exhausted`
// test: offline-ack no longer acquires, so an unstamped doc aborts IN-ENVELOPE
// regardless of pool state (asserted here with an EMPTY pool; the with-codes
// case is `unstamped_signed_offline_doc_aborts_at_offline_ack_even_with_session
// _and_codes` in b9_stamp_at_sign.rs).

#[tokio::test]
async fn unstamped_signed_offline_doc_aborts_empty_pool() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN, NodeMode::Offline, ShiftState::Opened).await;
    let session_id = OfflineSessionId::new();
    seed_offline_session(&pool, FN, session_id, OfflineSessionState::Open).await;
    // No code seeded — but that no longer matters: an unstamped doc aborts
    // BEFORE any pool interaction (offline-ack never acquires post-B9).
    let doc_id = insert_signed_doc(&pool, FN, 1).await;

    let outcome = stage_offline_ack::run(&pool, doc_id, FN).await.unwrap();
    assert!(
        matches!(
            outcome,
            OfflineAckOutcome::Refused(RefusalReason::UnstampedBareMacAbort)
        ),
        "expected Refused(UnstampedBareMacAbort); got: {outcome:?}"
    );
    // Code pool untouched.
    assert_eq!(fetch_consumed_count(&pool, FN).await, 0);
    // The doc is aborted TERMINALLY in-envelope (#192-safe; never a drainable
    // bare-MAC OLA).
    assert_eq!(fetch_doc_state(&pool, doc_id).await, "ABORTED");
}

// ─── Idempotency / conflict rollback (criterion 4 + I4 + I5) ────────

#[tokio::test]
async fn second_run_after_applied_returns_err_and_does_not_double_consume() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN, NodeMode::Offline, ShiftState::Opened).await;
    let session_id = OfflineSessionId::new();
    seed_offline_session(&pool, FN, session_id, OfflineSessionState::Open).await;
    seed_code(&pool, FN, 1).await;
    seed_code(&pool, FN, 2).await;
    let doc_id = insert_signed_doc(&pool, FN, 1).await;
    // B9: doc stamped-at-sign with code 1 (readback path). code 2 is never used.
    mark_doc_stamped_at_sign(&pool, doc_id, FN, 1, session_id).await;

    // First run — Applied via readback, code_lnd=1 (consumed at sign).
    let first = stage_offline_ack::run(&pool, doc_id, FN).await.unwrap();
    assert!(matches!(
        first,
        OfflineAckOutcome::Applied { code_lnd: 1, .. }
    ));

    // Second run on the same doc.  Doc is now in OFFLINE_LOCAL_ACK,
    // not SIGNED.  Operator W7 Round 1 LOW/MED fix (2026-05-15):
    // the new pre-check in stage_offline_ack catches this BEFORE
    // touching `offline_codes` and surfaces typed
    // `Refused(DocStateConflict)`.  No code consumption attempted;
    // I5 preserved at the pre-check layer (in addition to W4
    // schema's UNIQUE — defence-in-depth).
    let outcome = stage_offline_ack::run(&pool, doc_id, FN).await.unwrap();
    match &outcome {
        OfflineAckOutcome::Refused(RefusalReason::DocStateConflict { observed_state }) => {
            assert_eq!(
                observed_state, "OFFLINE_LOCAL_ACK",
                "operator W7 Round 2 LOW-4: observed_state must carry the actual state for audit"
            );
        }
        other => panic!(
            "expected typed Refused(DocStateConflict {{ observed_state: ... }}); got: {other:?}"
        ),
    }
    // I5 invariant: code_lnd=2 must remain unconsumed.
    let consumed: i64 = fetch_consumed_count(&pool, FN).await;
    assert_eq!(
        consumed, 1,
        "only code_lnd=1 was legally consumed; code_lnd=2 must stay unconsumed (I5)"
    );
    // Refusal audit emitted.
    let events = fetch_audit_events(&pool, doc_id).await;
    assert_eq!(events.len(), 2, "Applied + Refused");
    assert_eq!(events[0].0, "OFFLINE_LOCAL_ACK_APPLIED");
    assert_eq!(events[1].0, "OFFLINE_ACK_REFUSED");
    // The originally-consumed code's link unchanged.
    let link: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT consumed_by_document_id FROM offline_codes WHERE fiscal_number = ? AND code_lnd = 1",
    )
    .bind(FN)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(link.as_deref(), Some(&doc_id.as_bytes()[..]));
}

// ─── Doc-not-found (programmer bug surface) ─────────────────────────

#[tokio::test]
async fn unknown_doc_id_returns_typed_refused_doc_not_found() {
    // Operator W7 Round 1 LOW/MED fix (2026-05-15): pre-check in
    // stage_offline_ack catches missing doc BEFORE touching
    // `offline_codes`.  Was previously surfacing as FK-driven Err
    // via acquire_code_tx; now structured as typed
    // `Refused(DocNotFound)`.  Code pool stays untouched.
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN, NodeMode::Offline, ShiftState::Opened).await;
    let session_id = OfflineSessionId::new();
    seed_offline_session(&pool, FN, session_id, OfflineSessionState::Open).await;
    seed_code(&pool, FN, 1).await;

    let unknown_doc_id = DocumentId::new();
    let outcome = stage_offline_ack::run(&pool, unknown_doc_id, FN)
        .await
        .unwrap();
    assert!(
        matches!(
            outcome,
            OfflineAckOutcome::Refused(RefusalReason::DocNotFound)
        ),
        "expected typed Refused(DocNotFound); got: {outcome:?}"
    );
    // Code stays unconsumed (pre-check catches before acquire).
    assert_eq!(fetch_consumed_count(&pool, FN).await, 0);
    // Refusal audit emitted.
    let events = fetch_audit_events(&pool, unknown_doc_id).await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "OFFLINE_ACK_REFUSED");
}

#[tokio::test]
async fn cross_fn_doc_and_code_attribution_is_rejected() {
    // Operator W7 Round 1 HIGH-2 fix (2026-05-15): caller MUST
    // pass the FN the doc belongs to.  Mismatched FN must NOT
    // result in fiscal-integrity breach (doc of FN A stamped
    // with code/session of FN B).  Pre-check + helper WHERE
    // clause both filter on fiscal_number; the typed
    // Refused(DocStateConflict) surfaces because the pre-check
    // catches the mismatch.
    let (_d, pool) = fresh_pool().await;
    const FN_A: &str = "1111111111";
    const FN_B: &str = "2222222222";
    seed_fn(&pool, FN_A).await;
    seed_fn(&pool, FN_B).await;
    seed_node_state(&pool, FN_A, NodeMode::Offline, ShiftState::Opened).await;
    seed_node_state(&pool, FN_B, NodeMode::Offline, ShiftState::Opened).await;
    let session_a = OfflineSessionId::new();
    let session_b = OfflineSessionId::new();
    seed_offline_session(&pool, FN_A, session_a, OfflineSessionState::Open).await;
    seed_offline_session(&pool, FN_B, session_b, OfflineSessionState::Open).await;
    seed_code(&pool, FN_A, 100).await;
    seed_code(&pool, FN_B, 200).await;
    // Doc belongs to FN_A.
    let doc_a = insert_signed_doc(&pool, FN_A, 1).await;

    // Call stage with FN_B — would attempt to bind FN_A's doc to
    // FN_B's code + session.  Must refuse with the distinct
    // `CrossFnMismatch` typed variant (operator W7 Round 2 LOW-4:
    // separate signal from `DocStateConflict` so audit can
    // distinguish caller-bug-FN from operational state race).
    let outcome = stage_offline_ack::run(&pool, doc_a, FN_B).await.unwrap();
    match &outcome {
        OfflineAckOutcome::Refused(RefusalReason::CrossFnMismatch {
            observed_fiscal_number,
        }) => {
            assert_eq!(
                observed_fiscal_number, FN_A,
                "observed_fiscal_number must carry the doc's actual FN for audit"
            );
        }
        other => panic!(
            "cross-FN attribution MUST be refused as typed CrossFnMismatch (operator HIGH-2 + LOW-4); got: {other:?}"
        ),
    }
    // Neither FN's code pool touched.
    assert_eq!(
        fetch_consumed_count(&pool, FN_A).await,
        0,
        "FN_A code pool untouched"
    );
    assert_eq!(
        fetch_consumed_count(&pool, FN_B).await,
        0,
        "FN_B code pool untouched (the integrity-critical case)"
    );
    // Doc still in SIGNED.
    assert_eq!(fetch_doc_state(&pool, doc_a).await, "SIGNED");
    // Doc's offline columns remain NULL (no cross-FN stamping).
    let row: (Option<i64>, Option<String>, Option<Vec<u8>>) = sqlx::query_as(
        "SELECT offline_fiscal_no, offline_fiscal_date, offline_session_id \
         FROM fiscal_documents WHERE document_id = ?",
    )
    .bind(doc_a)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, None, "offline_fiscal_no must stay NULL");
    assert_eq!(row.1, None, "offline_fiscal_date must stay NULL");
    assert_eq!(row.2, None, "offline_session_id must stay NULL");
}

// ─── Sequential stage_offline_ack on two docs, same FN ─────────────
//
// M2-01 (Fable-locked, 2026-06-12): stage_offline_ack now ADVANCES the MAC
// chain seed per issued offline doc + asserts `ns_seed == doc.previous_hash`.
// Two SAME-FN offline-acks therefore MUST be serialised in sign-order (doc#2
// signs `prev = doc#1.unsigned` off the advanced seed) — which the per-FN write
// gate (A4 `FnWriteGate`) already guarantees in production.  The pre-M2-01
// version of this test raced two pre-signed docs (gate-bypassing) to stress the
// W5 code-pool CAS; post-M2-01 that race is invalid (the loser would hit the
// drift-assert).  Reworked to the real sequential chain; still verifies the W5
// atomic-acquire returns DISTINCT codes from one pool.  (Flagged for Fable: this
// is a test-semantics consequence of the locked seed-advance, not new behaviour.)
#[tokio::test]
async fn sequential_two_docs_acquire_distinct_codes() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN, NodeMode::Offline, ShiftState::Opened).await;
    let session_id = OfflineSessionId::new();
    seed_offline_session(&pool, FN, session_id, OfflineSessionState::Open).await;
    seed_code(&pool, FN, 1).await;
    seed_code(&pool, FN, 2).await;
    let doc_a = insert_signed_doc(&pool, FN, 1).await;
    let doc_b = insert_signed_doc(&pool, FN, 2).await;
    // B9: both docs stamped-at-sign with DISTINCT codes (readback path).  The
    // distinct-code guarantee now comes from the acquire-at-SIGN CAS; offline-ack
    // reads each doc's own stamp back (still verifies distinctness end-to-end).
    mark_doc_stamped_at_sign(&pool, doc_a, FN, 1, session_id).await;
    mark_doc_stamped_at_sign(&pool, doc_b, FN, 2, session_id).await;
    // doc#2 chains off doc#1 (its previous_hash = doc#1's unsigned hash =
    // 0xA0+1), matching the seed doc#1's offline-ack advances the chain to.
    sqlx::query("UPDATE fiscal_documents SET previous_hash = ? WHERE document_id = ?")
        .bind(vec![0xA1u8; 32])
        .bind(doc_b)
        .execute(&pool)
        .await
        .unwrap();

    let r1 = stage_offline_ack::run(&pool, doc_a, FN).await.unwrap();
    let r2 = stage_offline_ack::run(&pool, doc_b, FN).await.unwrap();

    let lnd1 = match r1 {
        OfflineAckOutcome::Applied { code_lnd, .. } => code_lnd,
        other => panic!("expected Applied for doc_a, got: {other:?}"),
    };
    let lnd2 = match r2 {
        OfflineAckOutcome::Applied { code_lnd, .. } => code_lnd,
        other => panic!("expected Applied for doc_b, got: {other:?}"),
    };
    assert_ne!(
        lnd1, lnd2,
        "sequential acquisition must return distinct codes"
    );
    let codes = {
        let mut v = vec![lnd1, lnd2];
        v.sort();
        v
    };
    assert_eq!(codes, vec![1, 2]);
    assert_eq!(fetch_consumed_count(&pool, FN).await, 2);
}

// ─── Named coverage for all refused node modes (W7 Round 2 LOW-3) ───
//
// Plan §Task 7 line 545 explicitly names `Blocked` / `StopMode` /
// `CryptoDegraded` / `GoingOnline` as refused modes.  The catch-all
// `!matches!(Offline | GoingOffline)` branch covers them
// structurally, but explicit named coverage per operator W7 Round 2
// LOW-3 pin protects against a future refactor that accidentally
// narrows the catch-all (e.g., `!= Offline` would silently let
// GoingOffline pass but also let nothing else through).  Single
// table-test exhausts all five non-offline modes.

#[tokio::test]
async fn all_non_offline_modes_refused_with_typed_node_not_offline() {
    // FN per iteration — partial UNIQUE ux_offline_active permits
    // at most one active session per FN, so reusing the same FN
    // across iterations would conflict.  Each iteration uses a
    // distinct 10-digit FN.
    let cases: &[(&str, NodeMode)] = &[
        ("1111111111", NodeMode::Online),
        ("2222222222", NodeMode::Blocked),
        ("3333333333", NodeMode::StopMode),
        ("4444444444", NodeMode::CryptoDegraded),
        ("5555555555", NodeMode::GoingOnline),
    ];
    for (fn_id, mode) in cases {
        let (_d, pool) = fresh_pool().await;
        // Override the FN from the default fresh_pool seed so each
        // test case is self-contained on its own DB.
        seed_fn(&pool, fn_id).await;
        seed_node_state(&pool, fn_id, *mode, ShiftState::Opened).await;
        let session_id = OfflineSessionId::new();
        seed_offline_session(&pool, fn_id, session_id, OfflineSessionState::Open).await;
        seed_code(&pool, fn_id, 1).await;
        let doc_id = insert_signed_doc(&pool, fn_id, 1).await;

        let outcome = stage_offline_ack::run(&pool, doc_id, fn_id).await.unwrap();
        match &outcome {
            OfflineAckOutcome::Refused(RefusalReason::NodeNotOffline { mode: observed }) => {
                assert_eq!(
                    observed, mode,
                    "NodeNotOffline must carry the observed mode for audit"
                );
            }
            other => panic!(
                "FN={fn_id} mode={mode:?}: expected Refused(NodeNotOffline{{mode: {mode:?}}}); got: {other:?}"
            ),
        }
        // Code untouched + doc still SIGNED + audit emitted.
        assert_eq!(
            fetch_consumed_count(&pool, fn_id).await,
            0,
            "FN={fn_id} mode={mode:?}: code pool must stay untouched"
        );
        assert_eq!(
            fetch_doc_state(&pool, doc_id).await,
            "SIGNED",
            "FN={fn_id} mode={mode:?}: doc must stay SIGNED"
        );
        let events = fetch_audit_events(&pool, doc_id).await;
        assert_eq!(
            events.len(),
            1,
            "FN={fn_id} mode={mode:?}: one refusal audit"
        );
        assert_eq!(events[0].0, "OFFLINE_ACK_REFUSED");
    }
}

// ─── W14a-2b Commit 6 §3.7 — doc_type-scoped shift-state widening ────

/// Seed a SIGNED doc with caller-chosen `doc_type` so the §3.7
/// widening matrix can be exercised across (regular fiscal | non-
/// fiscal) variants.
async fn insert_signed_doc_with_type(
    pool: &sqlx::SqlitePool,
    fn_id: &str,
    lnd: i64,
    doc_type: &str,
) -> DocumentId {
    let doc_id = DocumentId::new();
    let req_id = Uuid::now_v7();
    let sha = vec![0u8; 32];
    // M2-01: SIGNED docs carry unsigned_xml_sha256 (stage_offline_ack advances
    // the chain seed from it); previous_hash NULL (genesis) matches the genesis
    // node seed for the step-7b drift-assert.
    let unsigned = vec![0xA0u8.wrapping_add(lnd as u8); 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, unsigned_xml_sha256) \
         VALUES (?, ?, ?, ?, ?, 'SIGNED', 'b', 't', 'OFFLINE', \
            '2026-05-20T00:00:00Z', '{}', ?, ?)",
    )
    .bind(doc_id)
    .bind(req_id.as_bytes().to_vec())
    .bind(fn_id)
    .bind(lnd)
    .bind(doc_type)
    .bind(&sha)
    .bind(&unsigned)
    .execute(pool)
    .await
    .unwrap();
    doc_id
}

/// MED-C6-1 + §3.7 acceptance: SELL doc on `OpenedLocalPendingDrain`
/// shift in `Offline` mode → Applied (widened state allowed for
/// regular fiscal doc_type).  Pre-W14a-2b stage_offline_ack would
/// have refused with `ShiftNotOpened`.
#[tokio::test]
async fn applied_path_works_with_opened_local_pending_drain_for_sell() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(
        &pool,
        FN,
        NodeMode::Offline,
        ShiftState::OpenedLocalPendingDrain,
    )
    .await;
    let session_id = OfflineSessionId::new();
    seed_offline_session(&pool, FN, session_id, OfflineSessionState::Open).await;
    seed_code(&pool, FN, 42).await;
    let doc_id = insert_signed_doc_with_type(&pool, FN, 1, "SELL").await;
    mark_doc_stamped_at_sign(&pool, doc_id, FN, 42, session_id).await;

    let outcome = stage_offline_ack::run(&pool, doc_id, FN).await.unwrap();
    match outcome {
        OfflineAckOutcome::Applied { code_lnd, .. } => assert_eq!(code_lnd, 42),
        other => panic!("expected Applied on widened OpenedLocalPendingDrain, got {other:?}"),
    }
    assert_eq!(fetch_doc_state(&pool, doc_id).await, "OFFLINE_LOCAL_ACK");
    assert_eq!(fetch_consumed_count(&pool, FN).await, 1);
}

/// §3.7 doc-type-scoped widening: RETURN also gets widened path.
#[tokio::test]
async fn applied_path_works_with_opened_local_pending_drain_for_return() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(
        &pool,
        FN,
        NodeMode::Offline,
        ShiftState::OpenedLocalPendingDrain,
    )
    .await;
    let session_id = OfflineSessionId::new();
    seed_offline_session(&pool, FN, session_id, OfflineSessionState::Open).await;
    seed_code(&pool, FN, 43).await;
    let doc_id = insert_signed_doc_with_type(&pool, FN, 2, "RETURN").await;
    mark_doc_stamped_at_sign(&pool, doc_id, FN, 43, session_id).await;

    let outcome = stage_offline_ack::run(&pool, doc_id, FN).await.unwrap();
    assert!(matches!(outcome, OfflineAckOutcome::Applied { .. }));
    assert_eq!(fetch_doc_state(&pool, doc_id).await, "OFFLINE_LOCAL_ACK");
}

/// A′.3 PR-O3 edge-2 tail (numbering (ii)) — INVERTS the pre-O3
/// negative-pin: the offline SHIFT_OPEN doc, minted + driven to
/// `OpenedLocalPendingDrain` by stage_acquire (edge 2), reaches
/// stage_offline_ack and local-acks like a receipt (Signed →
/// OfflineLocalAck), consuming one offline code.  Step-3 is widened to
/// admit `ShiftOpen` specifically in `OpenedLocalPendingDrain` (the
/// state edge 2 leaves the shift in).  Still defence-in-depth:
/// stage_offline_ack reads doc_type + shift_state internally (a
/// `ShiftOpen` in any OTHER state falls through the `_` arm and is
/// still refused; SHIFT_CLOSE / Z_REPORT stay Opened-scoped until
/// slice 2).
#[tokio::test]
async fn applied_path_works_with_opened_local_pending_drain_for_shift_open() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(
        &pool,
        FN,
        NodeMode::Offline,
        ShiftState::OpenedLocalPendingDrain,
    )
    .await;
    let session_id = OfflineSessionId::new();
    seed_offline_session(&pool, FN, session_id, OfflineSessionState::Open).await;
    seed_code(&pool, FN, 44).await;
    let doc_id = insert_signed_doc_with_type(&pool, FN, 3, "SHIFT_OPEN").await;
    mark_doc_stamped_at_sign(&pool, doc_id, FN, 44, session_id).await;

    let outcome = stage_offline_ack::run(&pool, doc_id, FN).await.unwrap();
    match outcome {
        OfflineAckOutcome::Applied { code_lnd, .. } => assert_eq!(code_lnd, 44),
        other => panic!("offline SHIFT_OPEN in OLPD must local-ack (edge-2 tail), got {other:?}"),
    }
    // Local-ack'd like a receipt: doc flips OFFLINE_LOCAL_ACK, one code consumed.
    assert_eq!(fetch_doc_state(&pool, doc_id).await, "OFFLINE_LOCAL_ACK");
    assert_eq!(fetch_consumed_count(&pool, FN).await, 1);
}

/// A′.3 PR-O3 slice 2 (edge-9 tail, numbering (ii)): the offline local-Z doc
/// (Z_REPORT / SHIFT_CLOSE), minted + driven to `ClosingLocalPendingDrain` by
/// stage_acquire (edge 9/7), reaches stage_offline_ack and local-acks like a
/// receipt (Signed → OfflineLocalAck), consuming one offline code.  Step-3 is
/// widened to admit ShiftClose | ZReport specifically in
/// `ClosingLocalPendingDrain` (the state edge 9/7 leaves the shift in).
#[tokio::test]
async fn applied_path_works_with_closing_local_pending_drain_for_z_report() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(
        &pool,
        FN,
        NodeMode::Offline,
        ShiftState::ClosingLocalPendingDrain,
    )
    .await;
    let session_id = OfflineSessionId::new();
    seed_offline_session(&pool, FN, session_id, OfflineSessionState::Open).await;
    seed_code(&pool, FN, 55).await;
    let doc_id = insert_signed_doc_with_type(&pool, FN, 7, "Z_REPORT").await;
    mark_doc_stamped_at_sign(&pool, doc_id, FN, 55, session_id).await;

    let outcome = stage_offline_ack::run(&pool, doc_id, FN).await.unwrap();
    match outcome {
        OfflineAckOutcome::Applied { code_lnd, .. } => assert_eq!(code_lnd, 55),
        other => panic!("offline Z_REPORT in CLPD must local-ack (edge-9 tail), got {other:?}"),
    }
    assert_eq!(fetch_doc_state(&pool, doc_id).await, "OFFLINE_LOCAL_ACK");
    assert_eq!(fetch_consumed_count(&pool, FN).await, 1);
}

/// §3.7 invariant: a REGULAR fiscal doc (SELL here) on
/// `ClosingLocalPendingDrain` STILL refused — the §3.7 regular-doc widening
/// covers only `OpenedLocalPendingDrain`, not the closing-drain state; CLPD
/// remains the post-local-close sale lockout per PR #62 §W10.  (A′.3 PR-O3
/// widens ShiftClose / Z_REPORT — the closing doc itself — into CLPD, but
/// NOT regular sale-flavour docs; see
/// `applied_path_works_with_closing_local_pending_drain_for_z_report`.)
#[tokio::test]
async fn refusal_sell_on_closing_local_pending_drain_returns_shift_not_opened() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(
        &pool,
        FN,
        NodeMode::Offline,
        ShiftState::ClosingLocalPendingDrain,
    )
    .await;
    let session_id = OfflineSessionId::new();
    seed_offline_session(&pool, FN, session_id, OfflineSessionState::Open).await;
    seed_code(&pool, FN, 45).await;
    let doc_id = insert_signed_doc_with_type(&pool, FN, 4, "SELL").await;

    let outcome = stage_offline_ack::run(&pool, doc_id, FN).await.unwrap();
    assert!(matches!(
        outcome,
        OfflineAckOutcome::Refused(RefusalReason::ShiftNotOpened {
            current: ShiftState::ClosingLocalPendingDrain
        })
    ));
    assert_eq!(fetch_doc_state(&pool, doc_id).await, "SIGNED");
    assert_eq!(fetch_consumed_count(&pool, FN).await, 0);
}

// ─── Helper: silence "unused" warnings on imports kept for ergonomic
//     coverage of related types in case future tests need them.

#[allow(dead_code)]
fn _ensure_imports_referenced(_: ShiftId) {}
