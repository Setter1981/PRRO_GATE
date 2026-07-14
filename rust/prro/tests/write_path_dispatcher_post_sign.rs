//! W7b acceptance — post-sign dispatcher integration test
//! (`services::write_path::dispatch::dispatch_post_sign`).
//!
//! Covers the 6 W7b review axes (memory `m3b-w7b-review-criteria`):
//!
//! 1. **Wiring after stage_sign**: test seeds a doc in `Signed`
//!    state (the post-sign substrate) and drives it through the
//!    dispatcher directly.  Full stage 1+2+3 chain is exercised
//!    by separate M3a tests; this file tests the dispatcher in
//!    isolation per plan §Task 7 line 555.
//! 2. **Online → caller continues to stage_send**: dispatcher
//!    returns `PostSignRoute::Online`; test verifies the caller's
//!    contract (no offline stage side effect; would-be-stage_send
//!    is the caller's responsibility).
//! 3. **Offline | GoingOffline → stage_offline_ack, pipeline
//!    terminates**: dispatcher invokes stage_offline_ack and
//!    returns its outcome.  Pipeline termination is verified via
//!    `transport_trace` absence (criterion 6).
//! 4. **4 non-routable modes typed refusal**: separate test per
//!    mode; doc state unchanged; `WRITE_PATH_DISPATCH_REFUSED`
//!    audit row asserted.
//! 5. **Integration test file required** per plan §Task 7 line
//!    555.  This file IS that test.
//! 6. **stage_send / stage_finalize NOT called on offline
//!    branch**: verified DB-side via `transport_trace` row count
//!    == 0 after offline dispatch + audit-side via absence of
//!    `STAGE_SEND_*` events.

use prro::db::models::enums::{NodeMode, OfflineSessionState, ShiftState};
use prro::db::models::ids::{DocumentId, OfflineSessionId};
use prro::services::write_path::dispatch::{self, DispatcherRefusalReason, PostSignRoute};
use prro::services::write_path::stage_offline_ack::OfflineAckOutcome;
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
    .bind(mode.as_str())
    .bind(shift_state.as_str())
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_offline_session(
    pool: &sqlx::SqlitePool,
    fn_id: &str,
    session_id: OfflineSessionId,
    state: OfflineSessionState,
) {
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, ?, '2026-05-16T00:00:00Z')",
    )
    .bind(session_id)
    .bind(fn_id)
    .bind(state.as_str())
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_code(pool: &sqlx::SqlitePool, fn_id: &str, code_lnd: i64) {
    sqlx::query(
        "INSERT INTO offline_codes(fiscal_number, code_lnd, dps_code) \
         VALUES (?, ?, ?)",
    )
    .bind(fn_id)
    .bind(code_lnd)
    .bind(format!("DRILL-{code_lnd}"))
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_signed_doc(pool: &sqlx::SqlitePool, fn_id: &str, lnd: i64) -> DocumentId {
    let doc_id = DocumentId::new();
    let req_id = Uuid::now_v7();
    let sha = vec![0u8; 32];
    // M2-01: a SIGNED doc carries `unsigned_xml_sha256`; stage_offline_ack reads
    // it to advance the MAC chain seed.  `previous_hash` stays NULL (genesis) to
    // match the genesis node seed so the step-7b drift-assert passes.
    let unsigned = vec![0xA0u8.wrapping_add(lnd as u8); 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, unsigned_xml_sha256) \
         VALUES (?, ?, ?, ?, 'SELL', 'SIGNED', 'b', 't', 'OFFLINE', \
            '2026-05-16T00:00:00Z', '{}', ?, ?)",
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

/// B9: simulate `stage_sign`'s stamp-at-sign on an already-SIGNED offline doc —
/// the only path that now reaches offline-ack as `Applied` (readback).  Consumes
/// `code_lnd` by the doc + stamps its offline columns, so `stage_offline_ack`
/// takes the readback CAS → OFFLINE_LOCAL_ACK.
async fn stamp_doc_at_sign(
    pool: &sqlx::SqlitePool,
    doc_id: DocumentId,
    fn_id: &str,
    code_lnd: i64,
    session_id: OfflineSessionId,
) {
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

async fn fetch_transport_trace_count(pool: &sqlx::SqlitePool, doc_id: DocumentId) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM transport_trace WHERE document_id = ?")
        .bind(doc_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn fetch_audit_events(pool: &sqlx::SqlitePool, doc_id: DocumentId) -> Vec<(String, String)> {
    let doc_hex = doc_id
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    sqlx::query_as(
        "SELECT event_type, severity FROM audit_log \
         WHERE entity_type = 'fiscal_document' AND entity_id = ? \
         ORDER BY audit_id ASC",
    )
    .bind(doc_hex)
    .fetch_all(pool)
    .await
    .unwrap()
}

// ─── Online branch ──────────────────────────────────────────────────

#[tokio::test]
async fn online_mode_routes_to_online_no_side_effects() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN, NodeMode::Online, ShiftState::Opened).await;
    let doc_id = insert_signed_doc(&pool, FN, 1).await;

    let route = dispatch::dispatch_post_sign(&pool, doc_id, FN)
        .await
        .unwrap();
    match route {
        PostSignRoute::Online { mode } => assert_eq!(mode, NodeMode::Online),
        other => panic!("expected Online route, got: {other:?}"),
    }

    // Dispatcher does NOT advance doc on Online — caller's
    // responsibility to invoke stage_send.  Doc stays SIGNED here.
    assert_eq!(fetch_doc_state(&pool, doc_id).await, "SIGNED");
    // No transport_trace (caller hasn't invoked stage_send in this
    // test).
    assert_eq!(fetch_transport_trace_count(&pool, doc_id).await, 0);
    // No audit yet (dispatcher emits audit only on refusal).
    let events = fetch_audit_events(&pool, doc_id).await;
    assert!(
        events.is_empty(),
        "dispatcher emits no audit on Online route"
    );
}

// ─── Offline branch ─────────────────────────────────────────────────

#[tokio::test]
async fn offline_mode_routes_to_stage_offline_ack_applied() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN, NodeMode::Offline, ShiftState::Opened).await;
    let session_id = OfflineSessionId::new();
    seed_offline_session(&pool, FN, session_id, OfflineSessionState::Open).await;
    seed_code(&pool, FN, 1).await;
    let doc_id = insert_signed_doc(&pool, FN, 1).await;
    stamp_doc_at_sign(&pool, doc_id, FN, 1, session_id).await;

    let route = dispatch::dispatch_post_sign(&pool, doc_id, FN)
        .await
        .unwrap();
    match route {
        PostSignRoute::Offline {
            mode,
            outcome: OfflineAckOutcome::Applied { code_lnd, .. },
        } => {
            assert_eq!(mode, NodeMode::Offline);
            assert_eq!(code_lnd, 1);
        }
        other => panic!("expected Offline {{ Applied }}, got: {other:?}"),
    }

    // Pipeline terminated at stage_offline_ack — doc in OFFLINE_LOCAL_ACK.
    assert_eq!(fetch_doc_state(&pool, doc_id).await, "OFFLINE_LOCAL_ACK");
    // Criterion 6: stage_send NOT called — transport_trace must be empty.
    assert_eq!(
        fetch_transport_trace_count(&pool, doc_id).await,
        0,
        "stage_send must NOT be invoked on offline branch — transport_trace must be empty"
    );
    // Audit chain: OFFLINE_LOCAL_ACK_APPLIED only (no STAGE_SEND_*).
    let events = fetch_audit_events(&pool, doc_id).await;
    let event_types: Vec<String> = events.iter().map(|(t, _)| t.clone()).collect();
    assert!(
        event_types.contains(&"OFFLINE_LOCAL_ACK_APPLIED".to_string()),
        "expected OFFLINE_LOCAL_ACK_APPLIED audit; got: {event_types:?}"
    );
    for event_type in &event_types {
        assert!(
            !event_type.starts_with("STAGE_SEND_"),
            "no STAGE_SEND_* event must appear on offline branch; saw: {event_type}"
        );
        assert!(
            !event_type.starts_with("STAGE_FINALIZE_"),
            "no STAGE_FINALIZE_* event must appear on offline branch; saw: {event_type}"
        );
    }
}

#[tokio::test]
async fn going_offline_mode_routes_to_stage_offline_ack_applied() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN, NodeMode::GoingOffline, ShiftState::Opened).await;
    let session_id = OfflineSessionId::new();
    seed_offline_session(&pool, FN, session_id, OfflineSessionState::Open).await;
    seed_code(&pool, FN, 7).await;
    let doc_id = insert_signed_doc(&pool, FN, 1).await;
    stamp_doc_at_sign(&pool, doc_id, FN, 7, session_id).await;

    let route = dispatch::dispatch_post_sign(&pool, doc_id, FN)
        .await
        .unwrap();
    assert!(matches!(
        route,
        PostSignRoute::Offline {
            mode: NodeMode::GoingOffline,
            outcome: OfflineAckOutcome::Applied { .. },
        }
    ));
    assert_eq!(fetch_doc_state(&pool, doc_id).await, "OFFLINE_LOCAL_ACK");
    assert_eq!(fetch_transport_trace_count(&pool, doc_id).await, 0);
}

// ─── Dispatcher refusal: 4 non-routable modes ───────────────────────

async fn assert_dispatcher_refusal(
    fn_id: &str,
    mode: NodeMode,
    expected_reason: DispatcherRefusalReason,
) {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, fn_id).await;
    seed_node_state(&pool, fn_id, mode, ShiftState::Opened).await;
    let doc_id = insert_signed_doc(&pool, fn_id, 1).await;

    let route = dispatch::dispatch_post_sign(&pool, doc_id, fn_id)
        .await
        .unwrap();
    match route {
        PostSignRoute::Refused(reason) => {
            assert_eq!(
                reason, expected_reason,
                "FN={fn_id} mode={mode:?}: expected {expected_reason:?}, got {reason:?}"
            );
        }
        other => panic!("FN={fn_id} mode={mode:?}: expected Refused, got: {other:?}"),
    }
    // Doc state UNCHANGED.
    assert_eq!(
        fetch_doc_state(&pool, doc_id).await,
        "SIGNED",
        "doc state must stay SIGNED on dispatcher refusal"
    );
    // No transport_trace (stage_send not invoked).
    assert_eq!(fetch_transport_trace_count(&pool, doc_id).await, 0);
    // WRITE_PATH_DISPATCH_REFUSED audit row exists.
    let events = fetch_audit_events(&pool, doc_id).await;
    let mut saw_refused_audit = false;
    for (event_type, severity) in &events {
        if event_type == "WRITE_PATH_DISPATCH_REFUSED" {
            assert_eq!(severity, "WARNING");
            saw_refused_audit = true;
        }
        assert!(
            !event_type.starts_with("STAGE_SEND_"),
            "no STAGE_SEND_* event on refusal; saw: {event_type}"
        );
        assert!(
            event_type != "OFFLINE_LOCAL_ACK_APPLIED",
            "no offline-stage emission on refusal"
        );
    }
    assert!(
        saw_refused_audit,
        "WRITE_PATH_DISPATCH_REFUSED audit must be emitted"
    );
}

#[tokio::test]
async fn blocked_mode_returns_typed_node_blocked_refusal() {
    assert_dispatcher_refusal(
        "1111111111",
        NodeMode::Blocked,
        DispatcherRefusalReason::NodeBlocked,
    )
    .await;
}

#[tokio::test]
async fn stop_mode_returns_typed_node_stop_mode_refusal() {
    assert_dispatcher_refusal(
        "2222222222",
        NodeMode::StopMode,
        DispatcherRefusalReason::NodeStopMode,
    )
    .await;
}

#[tokio::test]
async fn crypto_degraded_mode_returns_typed_node_crypto_degraded_refusal() {
    assert_dispatcher_refusal(
        "3333333333",
        NodeMode::CryptoDegraded,
        DispatcherRefusalReason::NodeCryptoDegraded,
    )
    .await;
}

#[tokio::test]
async fn going_online_mode_returns_typed_node_going_online_refusal() {
    assert_dispatcher_refusal(
        "4444444444",
        NodeMode::GoingOnline,
        DispatcherRefusalReason::NodeGoingOnline,
    )
    .await;
}

// ─── Edge: node_state row missing ───────────────────────────────────

#[tokio::test]
async fn missing_node_state_returns_blocked_refusal() {
    // node_state row missing — dispatcher treats as NodeBlocked
    // refusal (safest fallback; operator must clear by bootstrapping
    // the FN's node_state).
    let (_d, pool) = fresh_pool().await;
    // Skip seed_node_state — leave the row missing.
    let doc_id = insert_signed_doc(&pool, FN, 1).await;

    let route = dispatch::dispatch_post_sign(&pool, doc_id, FN)
        .await
        .unwrap();
    assert!(matches!(
        route,
        PostSignRoute::Refused(DispatcherRefusalReason::NodeBlocked)
    ));
    assert_eq!(fetch_doc_state(&pool, doc_id).await, "SIGNED");
    // Audit emitted.
    let events = fetch_audit_events(&pool, doc_id).await;
    assert!(
        events
            .iter()
            .any(|(t, _)| t == "WRITE_PATH_DISPATCH_REFUSED"),
        "WRITE_PATH_DISPATCH_REFUSED audit must be emitted"
    );
}

// ─── Criterion 6 verification via dedicated absence check ───────────

#[tokio::test]
async fn offline_branch_does_not_create_transport_trace_or_send_audit() {
    // This test is a dedicated belt-and-braces for criterion 6:
    // even in the happy offline path, NO stage_send / stage_finalize
    // artifact must appear in DB.
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN, NodeMode::Offline, ShiftState::Opened).await;
    let session_id = OfflineSessionId::new();
    seed_offline_session(&pool, FN, session_id, OfflineSessionState::Open).await;
    seed_code(&pool, FN, 1).await;
    let doc_id = insert_signed_doc(&pool, FN, 1).await;
    stamp_doc_at_sign(&pool, doc_id, FN, 1, session_id).await;

    let route = dispatch::dispatch_post_sign(&pool, doc_id, FN)
        .await
        .unwrap();
    assert!(matches!(
        route,
        PostSignRoute::Offline {
            outcome: OfflineAckOutcome::Applied { .. },
            ..
        }
    ));

    // Belt-and-braces: transport_trace absent.
    let tt_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transport_trace")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        tt_count, 0,
        "transport_trace must have zero rows after offline dispatch"
    );

    // Audit table contains the OFFLINE_LOCAL_ACK_APPLIED event but
    // NO online-ladder audit events.  Filter excludes the offline
    // event itself explicitly (substring "ACK" alone would match
    // "OFFLINE_LOCAL_ACK_APPLIED" incorrectly).
    let events = fetch_audit_events(&pool, doc_id).await;
    let online_traces: Vec<&String> = events
        .iter()
        .map(|(t, _)| t)
        .filter(|t| {
            t.starts_with("STAGE_SEND_")
                || t.starts_with("STAGE_FINALIZE_")
                || t.contains("KVT1")
                || t.contains("KVT2")
                || t == &"DOC_ACK"
                || t == &"FINALIZE_ACK"
        })
        .collect();
    assert!(
        online_traces.is_empty(),
        "no online-ladder audit events must appear on offline branch; saw: {online_traces:?}"
    );
    // Belt-and-braces: the only event must be OFFLINE_LOCAL_ACK_APPLIED.
    let event_types: Vec<String> = events.iter().map(|(t, _)| t.clone()).collect();
    assert_eq!(
        event_types,
        vec!["OFFLINE_LOCAL_ACK_APPLIED".to_string()],
        "exactly one OFFLINE_LOCAL_ACK_APPLIED event expected; got: {event_types:?}"
    );
}
