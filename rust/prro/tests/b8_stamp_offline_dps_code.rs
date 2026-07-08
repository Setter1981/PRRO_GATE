//! B8-2 RED pins: after Signed→OfflineLocalAck, `fiscal_documents.offline_dps_code`
//! must be stamped with the acquired `dps_code` in the SAME CAS UPDATE.
//!
//! RED before migration 029 + `transition_to_offline_local_ack_tx` change;
//! GREEN after.

use prro::db::models::ids::{DocumentId, OfflineSessionId};
use prro::services::write_path::stage_offline_ack::{self, OfflineAckOutcome};
use uuid::Uuid;

const FN: &str = "8880000001";

async fn fresh_pool() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("b8_2.db"))
        .await
        .expect("open_pool");
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '88000001', 'test')",
    )
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    (dir, pool)
}

async fn seed_node_offline(pool: &sqlx::SqlitePool) {
    sqlx::query(
        "INSERT INTO node_state(fiscal_number, mode, shift_state, next_lnd) \
         VALUES (?, 'OFFLINE', 'OPENED', 1)",
    )
    .bind(FN)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_open_session(pool: &sqlx::SqlitePool) -> OfflineSessionId {
    let sid = OfflineSessionId::new();
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, 'OPEN', '2026-07-08T10:00:00Z')",
    )
    .bind(sid)
    .bind(FN)
    .execute(pool)
    .await
    .unwrap();
    sid
}

async fn seed_real_code(pool: &sqlx::SqlitePool, code_lnd: i64, dps_code: &str) {
    sqlx::query("INSERT INTO offline_codes(fiscal_number, code_lnd, dps_code) VALUES (?, ?, ?)")
        .bind(FN)
        .bind(code_lnd)
        .bind(dps_code)
        .execute(pool)
        .await
        .unwrap();
}

async fn insert_signed_doc(pool: &sqlx::SqlitePool, lnd: i64) -> DocumentId {
    let doc_id = DocumentId::new();
    let req_id = Uuid::now_v7();
    let sha = vec![0u8; 32];
    let unsigned = vec![0xA0u8.wrapping_add(lnd as u8); 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, unsigned_xml_sha256) \
         VALUES (?, ?, ?, ?, 'SELL', 'SIGNED', 'b', 't', 'OFFLINE', \
            '2026-07-08T10:00:00Z', '{}', ?, ?)",
    )
    .bind(doc_id)
    .bind(req_id.as_bytes().to_vec())
    .bind(FN)
    .bind(lnd)
    .bind(&sha)
    .bind(&unsigned)
    .execute(pool)
    .await
    .unwrap();
    doc_id
}

/// B8-2 pin: after stage_offline_ack::run Applied, `offline_dps_code` on the
/// doc row equals the acquired dps_code string.
///
/// Before migration 029 + stamp impl: column absent → RED.
/// After: column present and stamped → GREEN.
#[tokio::test]
async fn offline_dps_code_stamped_at_offline_local_ack() {
    let (_d, pool) = fresh_pool().await;
    seed_node_offline(&pool).await;
    seed_open_session(&pool).await;
    seed_real_code(&pool, 10, "STAMP-TEST-CODE").await;
    let doc_id = insert_signed_doc(&pool, 1).await;

    let outcome = stage_offline_ack::run(&pool, doc_id, FN)
        .await
        .expect("stage_offline_ack::run must not error");

    assert!(
        matches!(outcome, OfflineAckOutcome::Applied { .. }),
        "expected Applied, got: {outcome:?}"
    );

    // B8-2 core assertion: offline_dps_code must be stamped in the SAME CAS.
    let offline_dps_code: Option<String> =
        sqlx::query_scalar("SELECT offline_dps_code FROM fiscal_documents WHERE document_id = ?")
            .bind(doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(
        offline_dps_code.as_deref(),
        Some("STAMP-TEST-CODE"),
        "offline_dps_code must be stamped at OfflineLocalAck CAS, got: {offline_dps_code:?}"
    );

    // Also confirm offline_fiscal_no is still correctly set (regression guard).
    let offline_fiscal_no: Option<i64> =
        sqlx::query_scalar("SELECT offline_fiscal_no FROM fiscal_documents WHERE document_id = ?")
            .bind(doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        offline_fiscal_no,
        Some(10),
        "offline_fiscal_no must still be code_lnd=10"
    );
}

/// B8-2 exhaustion arc: CodePoolExhausted still fires correctly after B8-1+B8-2
/// changes (regression guard — the existing exhaustion refusal arc must be
/// unaffected by adding offline_dps_code to the stamp path).
#[tokio::test]
async fn exhaustion_refusal_arc_unaffected_by_b8_2() {
    use prro::services::write_path::stage_offline_ack::RefusalReason;

    let (_d, pool) = fresh_pool().await;
    seed_node_offline(&pool).await;
    seed_open_session(&pool).await;
    // No codes seeded → CodePoolExhausted.
    let doc_id = insert_signed_doc(&pool, 1).await;

    let outcome = stage_offline_ack::run(&pool, doc_id, FN)
        .await
        .expect("run must not error on exhaustion");

    assert!(
        matches!(
            outcome,
            OfflineAckOutcome::Refused(RefusalReason::CodePoolExhausted)
        ),
        "expected Refused(CodePoolExhausted), got: {outcome:?}"
    );
}
