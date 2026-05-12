//! W9.2 — `services::reconciliation::boot_phase` per-DocState helpers
//! integration tests.
//!
//! Three helpers shipped in W9.2 (per freeze §4.4 + §4.5 + §4.6):
//!   - `resume_sending_to_error_retryable` (§4.4)
//!   - `advance_sent_to_kvt1_from_probe`   (§4.5)
//!   - `passive_hold_kvt1`                 (§4.6)
//!
//! Each test exercises a single helper against a real `SqlitePool`
//! with minimal seed setup.  Tests are isolated by `tempfile::tempdir`.

use prro::db::models::ids::DocumentId;
use prro::db::repositories::transport_trace;
use prro::db::tx::with_immediate;
use prro::services::reconciliation::boot_phase;
use prro::transports::dps::dto::CheckAck;
use sqlx::SqlitePool;

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool");
    (dir, pool)
}

/// Seed an FN config + a fiscal_documents row in the requested state.
async fn seed_doc_in_state(pool: &SqlitePool, doc_byte: u8, state: &str) -> DocumentId {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(pool)
    .await
    .unwrap();
    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let sha = vec![0u8; 32];
    let lnd = doc_byte as i64;
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, '1234567890', ?, 'SELL', ?, 'b1', 't1', 'ONLINE', \
            '2026-01-01T00:00:00Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(lnd)
    .bind(state)
    .bind(&sha)
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

async fn read_state(pool: &SqlitePool, doc: DocumentId) -> String {
    let s: String = sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(doc)
        .fetch_one(pool)
        .await
        .unwrap();
    s
}

async fn audit_count(pool: &SqlitePool, event_type: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type = ?")
        .bind(event_type)
        .fetch_one(pool)
        .await
        .unwrap()
}

// ─── resume_sending_to_error_retryable ─────────────────────────────────

#[tokio::test]
async fn resume_sending_applies_when_doc_is_sending() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xA1, "SENDING").await;
    let applied = boot_phase::resume_sending_to_error_retryable(&pool, doc)
        .await
        .expect("helper must not fail on in-state doc");
    assert!(applied, "applied = true when CAS rows_affected == 1");
    assert_eq!(read_state(&pool, doc).await, "ERROR_RETRYABLE");
    assert_eq!(
        audit_count(&pool, "BOOT_RESUME_SENDING_TO_ERROR_RETRYABLE").await,
        1
    );
}

#[tokio::test]
async fn resume_sending_noop_when_doc_not_in_sending() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xA2, "SIGNED").await;
    let applied = boot_phase::resume_sending_to_error_retryable(&pool, doc)
        .await
        .expect("no-op must Ok(false), not Err");
    assert!(!applied, "applied = false when CAS rows_affected == 0");
    assert_eq!(read_state(&pool, doc).await, "SIGNED");
    assert_eq!(
        audit_count(&pool, "BOOT_RESUME_SENDING_TO_ERROR_RETRYABLE").await,
        0
    );
}

#[tokio::test]
async fn resume_sending_idempotent_second_call_is_noop() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xA3, "SENDING").await;
    let first = boot_phase::resume_sending_to_error_retryable(&pool, doc)
        .await
        .unwrap();
    let second = boot_phase::resume_sending_to_error_retryable(&pool, doc)
        .await
        .unwrap();
    assert!(first, "first call applies");
    assert!(!second, "second call no-ops");
    assert_eq!(read_state(&pool, doc).await, "ERROR_RETRYABLE");
    assert_eq!(
        audit_count(&pool, "BOOT_RESUME_SENDING_TO_ERROR_RETRYABLE").await,
        1
    );
}

// ─── advance_sent_to_kvt1_from_probe ───────────────────────────────────

async fn alloc_inflight_trace(pool: &SqlitePool, doc: DocumentId) -> i32 {
    use prro::db::repositories::transport_trace::NewAttempt;
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            let n = transport_trace::allocate_and_insert_tx(
                tx,
                doc,
                NewAttempt {
                    backend_profile_id: "b1".into(),
                    transport_profile_id: "t1".into(),
                    request_envelope_sha256: [0u8; 32],
                },
            )
            .await?;
            Ok::<i32, anyhow::Error>(n)
        })
    })
    .await
    .unwrap()
}

fn fake_ack(id: &str, data_sign: &[u8]) -> CheckAck {
    CheckAck {
        id: id.into(),
        id_sign: vec![],
        data_sign: data_sign.to_vec(),
    }
}

#[tokio::test]
async fn advance_sent_to_kvt1_applies_full_envelope() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xB1, "SENT").await;
    let attempt_no = alloc_inflight_trace(&pool, doc).await;
    let ack = fake_ack("SRV-FISCAL-12345", &[0x11, 0x22, 0x33]);

    let ok = boot_phase::advance_sent_to_kvt1_from_probe(
        &pool,
        doc,
        attempt_no,
        &ack,
        "2026-05-11T09:00:00Z",
        "2026-05-11T09:00:01Z",
    )
    .await
    .expect("helper must succeed on Sent + in-flight trace");
    assert!(ok, "CAS applied → Ok(true)");

    // (1) State advanced.
    assert_eq!(read_state(&pool, doc).await, "KVT1");

    // (2) KVT1_RAW persisted.
    let kvt1_raw: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT content FROM document_files WHERE document_id = ? AND kind = 'KVT1_RAW'",
    )
    .bind(doc)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(kvt1_raw.as_deref(), Some(&[0x11, 0x22, 0x33][..]));

    // (3) Trace completed with OK outcome + server_fiscal_no.
    let (completed_at, outcome, server_id): (Option<String>, Option<String>, Option<String>) =
        sqlx::query_as(
            "SELECT completed_at, outcome_kind, server_fiscal_no FROM transport_trace \
             WHERE document_id = ? AND attempt_no = ?",
        )
        .bind(doc)
        .bind(attempt_no)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(completed_at.is_some());
    assert_eq!(outcome.as_deref(), Some("OK"));
    assert_eq!(server_id.as_deref(), Some("SRV-FISCAL-12345"));

    // (4) Audit row.
    assert_eq!(audit_count(&pool, "BOOT_LAST_CHK_MATCH_KVT1").await, 1);
}

#[tokio::test]
async fn advance_sent_to_kvt1_returns_false_when_doc_not_in_sent() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xB2, "SIGNED").await;
    let ack = fake_ack("SRV-X", &[]);
    let ok = boot_phase::advance_sent_to_kvt1_from_probe(
        &pool,
        doc,
        1, // attempt_no doesn't matter — CAS bails first
        &ack,
        "2026-05-11T09:00:00Z",
        "2026-05-11T09:00:01Z",
    )
    .await
    .expect("CAS no-op must Ok(false), not Err");
    assert!(!ok);
    assert_eq!(read_state(&pool, doc).await, "SIGNED");
    assert_eq!(audit_count(&pool, "BOOT_LAST_CHK_MATCH_KVT1").await, 0);
    // No KVT1_RAW persisted on bail.
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_files WHERE document_id = ?")
        .bind(doc)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn advance_sent_to_kvt1_idempotent_on_repeat_call() {
    // MED 1 fix (freeze §10.3 row d): second invocation on an
    // already-advanced doc must no-op (CAS Sent→Kvt1 sees doc in
    // Kvt1 → rows_affected=0 → Ok(false)).  No duplicate audit, no
    // duplicate KVT1_RAW write, no duplicate trace completion.
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xB4, "SENT").await;
    let attempt_no = alloc_inflight_trace(&pool, doc).await;
    let ack = fake_ack("SRV-FISCAL-IDEMP", &[0xAB]);

    // First call applies the full envelope.
    let r1 = boot_phase::advance_sent_to_kvt1_from_probe(
        &pool,
        doc,
        attempt_no,
        &ack,
        "2026-05-11T09:00:00Z",
        "2026-05-11T09:00:01Z",
    )
    .await
    .expect("first call must succeed");
    assert!(r1, "first call applies → Ok(true)");
    assert_eq!(read_state(&pool, doc).await, "KVT1");

    // Second call: doc is now in Kvt1, CAS Sent→Kvt1 no-ops.
    let r2 = boot_phase::advance_sent_to_kvt1_from_probe(
        &pool,
        doc,
        attempt_no,
        &ack,
        "2026-05-11T09:00:02Z",
        "2026-05-11T09:00:03Z",
    )
    .await
    .expect("second call must Ok(false), not Err");
    assert!(!r2, "second call no-ops → Ok(false)");

    // Forensic invariants: no duplicates.
    assert_eq!(
        audit_count(&pool, "BOOT_LAST_CHK_MATCH_KVT1").await,
        1,
        "single audit row across the two calls"
    );
    // document_files Kvt1Raw: single row (replace_tx is INSERT OR
    // REPLACE but second call bails at CAS before touching it).
    let n_kvt1_raw: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM document_files WHERE document_id = ? AND kind = 'KVT1_RAW'",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n_kvt1_raw, 1, "single KVT1_RAW row");
    // Trace row stays completed with the FIRST call's wire times
    // (second call bails before reaching complete_via_recovery_tx).
    let wire_started: Option<String> = sqlx::query_scalar(
        "SELECT wire_call_started_at FROM transport_trace WHERE document_id = ? AND attempt_no = ?",
    )
    .bind(doc)
    .bind(attempt_no)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        wire_started.as_deref(),
        Some("2026-05-11T09:00:00Z"),
        "first call's wire times preserved (second call no-ops)"
    );
}

#[tokio::test]
async fn advance_sent_to_kvt1_errors_when_trace_row_missing() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xB3, "SENT").await;
    // NO transport_trace row allocated — recovery helper requires one.
    let ack = fake_ack("SRV-X", &[1]);
    let result = boot_phase::advance_sent_to_kvt1_from_probe(
        &pool,
        doc,
        1, // no row matches
        &ack,
        "2026-05-11T09:00:00Z",
        "2026-05-11T09:00:01Z",
    )
    .await;
    assert!(
        result.is_err(),
        "missing trace row → envelope rolls back, anyhow::Err"
    );
    // State unchanged (transaction rolled back).
    assert_eq!(read_state(&pool, doc).await, "SENT");
    assert_eq!(audit_count(&pool, "BOOT_LAST_CHK_MATCH_KVT1").await, 0);
}

// ─── passive_hold_kvt1 ─────────────────────────────────────────────────

#[tokio::test]
async fn passive_hold_kvt1_emits_audit_only() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xC1, "KVT1").await;
    boot_phase::passive_hold_kvt1(&pool, doc)
        .await
        .expect("passive_hold_kvt1 must succeed");
    assert_eq!(read_state(&pool, doc).await, "KVT1", "state unchanged");
    assert_eq!(audit_count(&pool, "BOOT_KVT1_HOLD_DEFERRED").await, 1);
    // No transport_trace row created.
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transport_trace WHERE document_id = ?")
        .bind(doc)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0, "no transport_trace row created by passive hold");
    // No document_files row created.
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_files WHERE document_id = ?")
        .bind(doc)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0, "no document_files row created");
}

#[tokio::test]
async fn passive_hold_kvt1_idempotent_each_call_emits_one_audit() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xC2, "KVT1").await;
    boot_phase::passive_hold_kvt1(&pool, doc).await.unwrap();
    boot_phase::passive_hold_kvt1(&pool, doc).await.unwrap();
    assert_eq!(audit_count(&pool, "BOOT_KVT1_HOLD_DEFERRED").await, 2);
    assert_eq!(read_state(&pool, doc).await, "KVT1");
}

// ─── run_boot_reconciliation stub ──────────────────────────────────────

#[tokio::test]
async fn run_boot_reconciliation_stub_returns_ok() {
    let (_dir, pool) = fresh_pool().await;
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();
    boot_phase::run_boot_reconciliation(&pool, "1234567890", None)
        .await
        .expect("W9.2 stub returns Ok(()) — W9.3 wires dispatch");
}
