//! **M3b W12 Post-Closure Hardening Phase 1 / REC-4 (2026-05-24)** —
//! crash-replay regression for `stage_finalize::run` idempotency.
//!
//! W12 two-phase commit on successful KVT2 evidence chain:
//!   1. Envelope 1 (`confirm_drain_doc` 1a/1b/1a-replay): Sent/Kvt1 →
//!      Kvt2 + Kvt1Raw persist + audit.
//!   2. Envelope 2 (`stage_finalize::run`): Kvt2 → Ack + chain seed
//!      advance + inbox DONE + outbox row + STAGE_FINALIZE_ACK audit.
//!
//! Crash window: process exits between Envelope 1 commit and Envelope 2
//! commit (doc stuck в Kvt2 state).  Next-tick cohort selector re-routes
//! Kvt2 doc через `process_via_w12_kvt2_advance` → re-runs
//! `stage_finalize::run`.  Critical contract: re-invocation MUST return
//! `StageFinalizeOutcome::AlreadyAcked` без duplicating any of the 5
//! side-effects from the first successful invocation.
//!
//! This regression test runs `stage_finalize::run` twice on the same
//! Kvt2 doc + асserts no duplicate rows in any of the 5 affected
//! tables (fiscal_documents state, node_state chain seed, ingress_inbox
//! status, outbox, audit_log).

mod common;

use prro::db::models::enums::{NodeMode, OfflineSessionState, ShiftState};
use prro::db::models::ids::{DocumentId, OfflineSessionId, ShiftId};
use prro::services::write_path::stage_finalize::{self, StageFinalizeOutcome};
use sqlx::SqlitePool;
use uuid::Uuid;

const FN: &str = "1234567890";
const CASHIER_OK: &str = "test-cashier";

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("stage_finalize_idemp.db"))
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
    .bind(mode.as_str())
    .bind(shift.as_str())
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

/// Seed a Kvt2-state doc з ALL prereqs for stage_finalize::run to succeed
/// (SIGNED_XML, KVT1_RAW, offline_codes, ingress_inbox, chain anchors).
async fn seed_kvt2_doc(
    pool: &SqlitePool,
    session_id: OfflineSessionId,
    shift_id: ShiftId,
) -> DocumentId {
    let doc_id = DocumentId::new();
    let req_id = Uuid::now_v7();
    let payload_sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents( \
            document_id, request_id, fiscal_number, shift_id, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, signed_by_cashier_id, \
            offline_session_id, offline_fiscal_no, offline_fiscal_date, \
            server_fiscal_no, unsigned_xml_sha256, previous_hash, signing_inputs_pinned_at \
         ) VALUES ( \
            ?, ?, ?, ?, 100, 'SELL', 'KVT2', \
            'b', 't', 'OFFLINE', '2026-05-21T00:00:00Z', \
            '{}', ?, ?, \
            ?, 100, '2026-05-21T00:00:00Z', \
            'DPS-FN-IDEMP', ?, ?, '2026-05-21T00:00:00Z' \
         )",
    )
    .bind(doc_id)
    .bind(req_id.as_bytes().to_vec())
    .bind(FN)
    .bind(shift_id)
    .bind(&payload_sha)
    .bind(CASHIER_OK)
    .bind(session_id)
    .bind(&common::chain_anchor(0x01)[..])
    .bind(&common::chain_anchor(0x00)[..])
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
        "INSERT INTO document_files(document_id, kind, content) \
         VALUES (?, 'KVT1_RAW', ?)",
    )
    .bind(doc_id)
    .bind(vec![0xAAu8; 32])
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO offline_codes(fiscal_number, code_lnd, consumed_at, consumed_by_document_id) \
         VALUES (?, 100, '2026-05-21T00:00:01Z', ?)",
    )
    .bind(FN)
    .bind(doc_id)
    .execute(pool)
    .await
    .unwrap();
    let idempotency_key = format!("idem-rec4-{}", req_id);
    sqlx::query(
        "INSERT INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical, status) \
         VALUES (?, ?, 'REST', 'sell', ?, '{}', ?, 'PROCESSING')",
    )
    .bind(req_id.as_bytes().to_vec())
    .bind(FN)
    .bind(&idempotency_key)
    .bind(&payload_sha)
    .execute(pool)
    .await
    .unwrap();
    common::init_chain_seed(pool, FN, common::chain_anchor(0x00))
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

async fn read_doc_state(pool: &SqlitePool, doc_id: DocumentId) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(doc_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_chain_seed(pool: &SqlitePool) -> Vec<u8> {
    sqlx::query_scalar(
        "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number = ?",
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn read_inbox_status(pool: &SqlitePool, doc_id: DocumentId) -> String {
    sqlx::query_scalar(
        "SELECT s.status FROM ingress_inbox s \
         JOIN fiscal_documents d ON d.request_id = s.request_id \
         WHERE d.document_id = ?",
    )
    .bind(doc_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn outbox_row_count(pool: &SqlitePool, doc_id: DocumentId) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM outbox WHERE document_id = ?")
        .bind(doc_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// **REC-4 regression**: crash-replay idempotency contract for
/// `stage_finalize::run`.
///
/// Validates that re-running `stage_finalize::run` on a doc that already
/// reached Ack (simulating crash-recovery between Envelope 1 commit and
/// Envelope 2 commit, then re-execution post-restart) returns
/// `StageFinalizeOutcome::AlreadyAcked` WITHOUT duplicating any of the
/// 5 atomic side-effects:
///   1. fiscal_documents.state stays Ack (no double CAS).
///   2. node_state.last_known_unsigned_xml_sha256 stays at the value
///      written by the first invocation (no double advance).
///   3. ingress_inbox.status stays DONE (no double mark).
///   4. outbox row count stays 1 (no duplicate row).
///   5. STAGE_FINALIZE_ACK audit count stays 1 (no duplicate audit).
#[tokio::test]
async fn stage_finalize_double_invocation_idempotent_after_simulated_crash() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let doc = seed_kvt2_doc(&pool, session_id, shift_id).await;

    // First invocation — full 5-write Envelope 2 commits atomically.
    let outcome_1 = stage_finalize::run(&pool, doc).await.unwrap();
    assert!(
        matches!(outcome_1, StageFinalizeOutcome::Acked { .. }),
        "first invocation MUST return Acked; got: {outcome_1:?}"
    );

    // Snapshot post-first-invocation state for invariant comparison.
    assert_eq!(read_doc_state(&pool, doc).await, "ACK");
    let chain_seed_after_first = read_chain_seed(&pool).await;
    assert_eq!(read_inbox_status(&pool, doc).await, "DONE");
    assert_eq!(outbox_row_count(&pool, doc).await, 1);
    assert_eq!(audit_count(&pool, "STAGE_FINALIZE_ACK").await, 1);

    // Second invocation — simulating crash-replay after process restart.
    // MUST return AlreadyAcked + leave ALL 5 side-effect tables exactly
    // as they were post-first-invocation.
    let outcome_2 = stage_finalize::run(&pool, doc).await.unwrap();
    assert!(
        matches!(outcome_2, StageFinalizeOutcome::AlreadyAcked),
        "re-invocation MUST return AlreadyAcked (idempotent crash-replay); got: {outcome_2:?}"
    );

    // Invariant 1: fiscal_documents.state stays Ack (no double CAS).
    assert_eq!(read_doc_state(&pool, doc).await, "ACK");
    // Invariant 2: node_state chain seed unchanged (no double advance).
    assert_eq!(
        read_chain_seed(&pool).await,
        chain_seed_after_first,
        "chain seed MUST NOT advance on AlreadyAcked re-invocation"
    );
    // Invariant 3: ingress_inbox.status stays DONE (no double mark).
    assert_eq!(read_inbox_status(&pool, doc).await, "DONE");
    // Invariant 4: outbox row count stays 1 (no duplicate row).
    assert_eq!(
        outbox_row_count(&pool, doc).await,
        1,
        "outbox MUST NOT gain a duplicate row on AlreadyAcked"
    );
    // Invariant 5: STAGE_FINALIZE_ACK audit count stays 1 (no duplicate audit).
    assert_eq!(
        audit_count(&pool, "STAGE_FINALIZE_ACK").await,
        1,
        "STAGE_FINALIZE_ACK audit MUST NOT duplicate on AlreadyAcked"
    );
}
