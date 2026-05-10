//! Targeted verification for migration 010 (transport_trace) and the
//! repo helpers in `db::repositories::transport_trace`.
//!
//! Anchored on the W7 design freeze §3 + ADR-M3-A5 / A9 step 5-6.
//!
//! Seven fixtures cover the schema's load-bearing properties:
//!  1. table + indexes exist; column set matches the freeze DDL.
//!  2. INSERT in 4-pre + UPDATE in 4-b round-trip; attempt_no monotonic.
//!  3. partial-completion CHECK rejects mid-state rows.
//!  4. CASCADE delete: removing fiscal_documents row drops trace rows.
//!  5. partial index `ix_transport_trace_unfinished` is queryable for
//!     the App::boot recovery scan (W9 prerequisite).
//!  6. second `complete_tx` on an already-completed row is a no-op
//!     (`rows_affected == 0`); first outcome preserved.
//!  7. OK outcome without `server_fiscal_no` is rejected by CHECK
//!     constraint (forensic invariant).

use prro::db::models::ids::DocumentId;
use prro::db::repositories::transport_trace::{self, AttemptCompletion, NewAttempt, OutcomeKind};
use prro::db::tx::with_immediate;
use sqlx::SqlitePool;
use std::collections::HashSet;

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs migrations including 010");
    (dir, pool)
}

/// Seed an FN config + a minimal fiscal_documents row, return its id.
/// `doc_byte` doubles as the `lnd` to avoid colliding on the partial
/// UNIQUE `(fiscal_number, lnd)` index from W1 (`007_lnd_unique.sql`).
async fn seed_doc(pool: &SqlitePool, doc_byte: u8) -> DocumentId {
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
         VALUES (?, ?, '1234567890', ?, 'SELL', 'SIGNED', 'b1', 't1', 'ONLINE', \
            '2026-01-01T00:00:00Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(lnd)
    .bind(&sha)
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

async fn alloc(pool: &SqlitePool, doc: DocumentId, sha: [u8; 32]) -> i32 {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            let n = transport_trace::allocate_and_insert_tx(
                tx,
                doc,
                NewAttempt {
                    backend_profile_id: "b1".into(),
                    transport_profile_id: "t1".into(),
                    request_envelope_sha256: sha,
                },
            )
            .await?;
            Ok::<i32, anyhow::Error>(n)
        })
    })
    .await
    .expect("allocate_and_insert_tx")
}

async fn complete_ok(pool: &SqlitePool, doc: DocumentId, attempt_no: i32, server_id: &str) {
    let server_id = server_id.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            let n = transport_trace::complete_tx(
                tx,
                doc,
                attempt_no,
                AttemptCompletion {
                    wire_call_started_at: "2026-05-09T10:00:00Z".into(),
                    wire_call_finished_at: "2026-05-09T10:00:01Z".into(),
                    outcome_kind: OutcomeKind::Ok,
                    server_fiscal_no: Some(server_id),
                    server_status_code: None,
                    error_kind: None,
                    error_message: None,
                    retry_class: None,
                },
            )
            .await?;
            Ok::<u64, anyhow::Error>(n)
        })
    })
    .await
    .expect("complete_tx");
}

#[tokio::test]
async fn migration_010_creates_table_and_indexes() {
    let (_d, pool) = fresh_pool().await;

    let names: HashSet<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' ORDER BY 1")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .collect();
    assert!(
        names.contains("transport_trace"),
        "transport_trace table missing post-010; have {names:?}"
    );

    let indexes: HashSet<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='index' ORDER BY 1")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .collect();
    for idx in [
        "ix_transport_trace_started",
        "ix_transport_trace_unfinished",
    ] {
        assert!(
            indexes.contains(idx),
            "missing index {idx}; have {indexes:?}"
        );
    }

    let cols: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(transport_trace)")
            .fetch_all(&pool)
            .await
            .unwrap();
    let col_names: HashSet<String> = cols.iter().map(|c| c.1.clone()).collect();
    for col in [
        "document_id",
        "attempt_no",
        "started_at",
        "backend_profile_id",
        "transport_profile_id",
        "request_envelope_sha256",
        "completed_at",
        "wire_call_started_at",
        "wire_call_finished_at",
        "outcome_kind",
        "server_fiscal_no",
        "server_status_code",
        "error_kind",
        "error_message",
    ] {
        assert!(
            col_names.contains(col),
            "transport_trace missing {col}; have {col_names:?}"
        );
    }
}

#[tokio::test]
async fn allocate_then_complete_round_trip() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x11).await;

    let attempt_no = alloc(&pool, doc, [7u8; 32]).await;
    assert_eq!(attempt_no, 1, "first attempt_no must be 1");

    let rows = transport_trace::list_for_document(&pool, doc)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(
        rows[0].completed_at.is_none(),
        "pre-completion completed_at must be NULL"
    );
    assert!(
        rows[0].outcome_kind.is_none(),
        "pre-completion outcome_kind must be NULL"
    );
    assert_eq!(rows[0].request_envelope_sha256, [7u8; 32]);

    complete_ok(&pool, doc, attempt_no, "SERVER-1").await;

    let rows = transport_trace::list_for_document(&pool, doc)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].completed_at.is_some());
    assert_eq!(rows[0].outcome_kind.as_deref(), Some("OK"));
    assert_eq!(rows[0].server_fiscal_no.as_deref(), Some("SERVER-1"));

    let attempt2 = alloc(&pool, doc, [8u8; 32]).await;
    assert_eq!(attempt2, 2, "attempt_no must be monotonic per document");
}

#[tokio::test]
async fn partial_completion_check_rejects_mid_state_row() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x22).await;

    let _ = alloc(&pool, doc, [9u8; 32]).await;

    let err = sqlx::query(
        "UPDATE transport_trace SET completed_at = '2026-05-09T10:00:00Z' \
         WHERE document_id = ? AND attempt_no = 1",
    )
    .bind(doc)
    .execute(&pool)
    .await
    .expect_err("partial completion (only completed_at) must violate CHECK");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("check") || msg.contains("constraint"),
        "expected CHECK error, got: {msg}"
    );
}

#[tokio::test]
async fn fiscal_documents_cascade_drops_trace_rows() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x33).await;

    let _ = alloc(&pool, doc, [1u8; 32]).await;

    let before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM transport_trace WHERE document_id = ?")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, 1);

    sqlx::query("DELETE FROM fiscal_documents WHERE document_id = ?")
        .bind(doc)
        .execute(&pool)
        .await
        .expect("delete fiscal_documents row (CASCADE)");

    let after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM transport_trace WHERE document_id = ?")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after, 0, "transport_trace rows must cascade-delete");
}

#[tokio::test]
async fn partial_index_unfinished_is_queryable() {
    let (_d, pool) = fresh_pool().await;
    let doc_a = seed_doc(&pool, 0x44).await;
    let doc_b = seed_doc(&pool, 0x55).await;

    let _ = alloc(&pool, doc_a, [1u8; 32]).await;
    let attempt2 = alloc(&pool, doc_a, [2u8; 32]).await;
    complete_ok(&pool, doc_a, attempt2, "X").await;

    let attempt_b = alloc(&pool, doc_b, [3u8; 32]).await;
    // Custom completion with REJECTED kind — exercise the CHECK list.
    with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let n = transport_trace::complete_tx(
                tx,
                doc_b,
                attempt_b,
                AttemptCompletion {
                    wire_call_started_at: "2026-05-09T10:00:00Z".into(),
                    wire_call_finished_at: "2026-05-09T10:00:01Z".into(),
                    outcome_kind: OutcomeKind::Rejected,
                    server_fiscal_no: None,
                    server_status_code: Some(-1),
                    error_kind: Some("Authorization".into()),
                    error_message: Some("ERROR_VEREFY".into()),
                    retry_class: Some("TerminalReject".into()),
                },
            )
            .await?;
            Ok::<u64, anyhow::Error>(n)
        })
    })
    .await
    .expect("complete_tx Rejected");

    let unfinished: Vec<(Vec<u8>, i32)> = sqlx::query_as(
        "SELECT document_id, attempt_no FROM transport_trace \
         WHERE completed_at IS NULL ORDER BY started_at",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        unfinished.len(),
        1,
        "exactly one unfinished attempt expected"
    );
    assert_eq!(
        &unfinished[0].0[..],
        doc_a.as_bytes(),
        "unfinished doc must be doc_a"
    );
    assert_eq!(unfinished[0].1, 1);
}

#[tokio::test]
async fn second_completion_is_noop_first_outcome_preserved() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x66).await;

    let attempt_no = alloc(&pool, doc, [4u8; 32]).await;
    complete_ok(&pool, doc, attempt_no, "FIRST").await;

    // Second complete_tx with a *different* outcome — must report 0 rows
    // affected and leave the first outcome intact.
    let rows_affected = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let n = transport_trace::complete_tx(
                tx,
                doc,
                attempt_no,
                AttemptCompletion {
                    wire_call_started_at: "2026-05-09T11:00:00Z".into(),
                    wire_call_finished_at: "2026-05-09T11:00:01Z".into(),
                    outcome_kind: OutcomeKind::Rejected,
                    server_fiscal_no: None,
                    server_status_code: Some(-1),
                    error_kind: Some("Authorization".into()),
                    error_message: Some("ERROR_VEREFY".into()),
                    retry_class: Some("TerminalReject".into()),
                },
            )
            .await?;
            Ok::<u64, anyhow::Error>(n)
        })
    })
    .await
    .expect("complete_tx second invocation");

    assert_eq!(
        rows_affected, 0,
        "second complete_tx must be no-op (rows_affected == 0)"
    );

    // First outcome preserved — still OK + SERVER-id 'FIRST'.
    let rows = transport_trace::list_for_document(&pool, doc)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].outcome_kind.as_deref(), Some("OK"));
    assert_eq!(rows[0].server_fiscal_no.as_deref(), Some("FIRST"));
    assert!(
        rows[0].error_kind.is_none(),
        "Rejected fields from second call must NOT have been written"
    );
}

#[tokio::test]
async fn ok_outcome_without_server_fiscal_no_is_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x77).await;

    let attempt_no = alloc(&pool, doc, [5u8; 32]).await;

    // Try to complete OK without server_fiscal_no — must violate the
    // outcome-specific CHECK in migration 010.
    let res = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let n = transport_trace::complete_tx(
                tx,
                doc,
                attempt_no,
                AttemptCompletion {
                    wire_call_started_at: "2026-05-09T10:00:00Z".into(),
                    wire_call_finished_at: "2026-05-09T10:00:01Z".into(),
                    outcome_kind: OutcomeKind::Ok,
                    server_fiscal_no: None, // <-- violates CHECK
                    server_status_code: None,
                    error_kind: None,
                    error_message: None,
                    retry_class: None,
                },
            )
            .await?;
            Ok::<u64, anyhow::Error>(n)
        })
    })
    .await;

    let err = res.expect_err("OK without server_fiscal_no must violate CHECK");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("check") || msg.contains("constraint"),
        "expected CHECK error, got: {msg}"
    );

    // Empty string is also rejected (length > 0 part of the CHECK).
    let res_empty = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let n = transport_trace::complete_tx(
                tx,
                doc,
                attempt_no,
                AttemptCompletion {
                    wire_call_started_at: "2026-05-09T10:00:00Z".into(),
                    wire_call_finished_at: "2026-05-09T10:00:01Z".into(),
                    outcome_kind: OutcomeKind::Ok,
                    server_fiscal_no: Some(String::new()),
                    server_status_code: None,
                    error_kind: None,
                    error_message: None,
                    retry_class: None,
                },
            )
            .await?;
            Ok::<u64, anyhow::Error>(n)
        })
    })
    .await;
    let err_empty = res_empty.expect_err("OK with empty server_fiscal_no must violate CHECK");
    let msg_empty = err_empty.to_string().to_lowercase();
    assert!(
        msg_empty.contains("check") || msg_empty.contains("constraint"),
        "expected CHECK error for empty string, got: {msg_empty}"
    );

    // Sanity: row stayed unfinished.
    let rows = transport_trace::list_for_document(&pool, doc)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(
        rows[0].completed_at.is_none(),
        "row must remain unfinished after rejected completes"
    );
}
