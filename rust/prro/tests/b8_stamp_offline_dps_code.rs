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

async fn fetch_state(pool: &sqlx::SqlitePool, doc_id: DocumentId) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(doc_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// B8-2 pin (B9-updated): the opaque `offline_dps_code` reaches the doc row and
/// SURVIVES into OFFLINE_LOCAL_ACK.  Post-B9 the stamp happens at SIGN (so the
/// code is in the signed bytes as `<MAC ID>`), and offline-ack takes the
/// READBACK path → OLA carrying the same code.  Here we simulate the
/// stamp-at-sign (consume code 10 by the doc + stamp its offline columns), then
/// assert offline-ack readback → Applied with the code preserved.
#[tokio::test]
async fn offline_dps_code_stamped_at_offline_local_ack() {
    let (_d, pool) = fresh_pool().await;
    seed_node_offline(&pool).await;
    let session_id = seed_open_session(&pool).await;
    seed_real_code(&pool, 10, "STAMP-TEST-CODE").await;
    let doc_id = insert_signed_doc(&pool, 1).await;
    // Simulate stamp-at-sign: consume the code by the doc + stamp offline columns.
    sqlx::query(
        "UPDATE offline_codes SET consumed_at = CURRENT_TIMESTAMP, consumed_by_document_id = ? \
         WHERE fiscal_number = ? AND code_lnd = 10",
    )
    .bind(doc_id)
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE fiscal_documents SET offline_fiscal_no = 10, \
             offline_fiscal_date = CURRENT_TIMESTAMP, offline_session_id = ?, \
             offline_dps_code = 'STAMP-TEST-CODE' WHERE document_id = ?",
    )
    .bind(session_id)
    .bind(doc_id)
    .execute(&pool)
    .await
    .unwrap();

    let outcome = stage_offline_ack::run(&pool, doc_id, FN)
        .await
        .expect("stage_offline_ack::run must not error");

    assert!(
        matches!(outcome, OfflineAckOutcome::Applied { .. }),
        "expected Applied (readback), got: {outcome:?}"
    );
    assert_eq!(
        fetch_state(&pool, doc_id).await,
        "OFFLINE_LOCAL_ACK",
        "readback path advances SIGNED→OFFLINE_LOCAL_ACK"
    );

    // B8 core: the opaque dps_code is on the doc row (stamped at sign, survives OLA).
    let offline_dps_code: Option<String> =
        sqlx::query_scalar("SELECT offline_dps_code FROM fiscal_documents WHERE document_id = ?")
            .bind(doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        offline_dps_code.as_deref(),
        Some("STAMP-TEST-CODE"),
        "offline_dps_code must be present on the OLA doc, got: {offline_dps_code:?}"
    );
    let offline_fiscal_no: Option<i64> =
        sqlx::query_scalar("SELECT offline_fiscal_no FROM fiscal_documents WHERE document_id = ?")
            .bind(doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        offline_fiscal_no,
        Some(10),
        "offline_fiscal_no = code_lnd=10"
    );
}

/// B9 (was B8-2 exhaustion arc): post-B9 an UNSTAMPED SIGNED offline doc reaching
/// offline-ack ABORTS (SIGNED→Aborted) — it can never validly drain (bare
/// `<MAC>`).  Offline-ack no longer acquires, so pool state is irrelevant; the
/// abort fires regardless.  (The former `CodePoolExhausted` arc is now handled
/// at SIGN — see tests/b9_stamp_at_sign.rs.)
#[tokio::test]
async fn unstamped_signed_offline_doc_aborts_at_offline_ack() {
    use prro::services::write_path::stage_offline_ack::RefusalReason;

    let (_d, pool) = fresh_pool().await;
    seed_node_offline(&pool).await;
    seed_open_session(&pool).await;
    // No codes seeded — but pool state no longer matters; unstamped → abort.
    let doc_id = insert_signed_doc(&pool, 1).await;

    let outcome = stage_offline_ack::run(&pool, doc_id, FN)
        .await
        .expect("run must not error");

    assert!(
        matches!(
            outcome,
            OfflineAckOutcome::Refused(RefusalReason::UnstampedBareMacAbort)
        ),
        "expected Refused(UnstampedBareMacAbort), got: {outcome:?}"
    );
    assert_eq!(
        fetch_state(&pool, doc_id).await,
        "ABORTED",
        "unstamped bare-MAC doc aborts terminally (never a drainable OLA)"
    );
}
