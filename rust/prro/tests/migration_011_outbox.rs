//! Targeted verification for migration 011 (outbox) and the repo
//! helpers in `db::repositories::outbox`.
//!
//! Anchored on the W8 design freeze §3.1 + ADR-M3-A8.
//!
//! Eight fixtures cover the schema's load-bearing properties:
//!  1. table + indexes exist; column set matches the freeze DDL.
//!  2. enqueue_document_tx INSERT round-trip; defaults applied.
//!  3. PRIMARY KEY (document_id) rejects duplicate INSERT.
//!  4. status CHECK rejects unknown values.
//!  5. status / published_at all-or-none CHECK rejects mid-state rows.
//!  6. partial index `ix_outbox_pending` is queryable for the
//!     post-M3a publisher worker.
//!  7. FK ON DELETE RESTRICT blocks `fiscal_documents` row deletion
//!     while a PENDING outbox row references it.
//!  8. `get_for_document` returns `None` for a missing doc id.

use prro::db::models::ids::DocumentId;
use prro::db::repositories::outbox::{self, NewOutboxRow};
use prro::db::tx::with_immediate;
use sqlx::SqlitePool;
use std::collections::HashSet;

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations including 011");
    (dir, pool)
}

/// Seed an FN config + a fiscal_documents row in `Kvt2` state (the
/// finalize entry condition).  Mirrors the W7.5 helper but seeds at
/// Kvt2 instead of Signed.  `lnd` parameterised to dodge the
/// `(fiscal_number, lnd)` partial UNIQUE.
async fn seed_kvt2_doc(pool: &SqlitePool, doc_byte: u8, lnd: i64) -> DocumentId {
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
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, '1234567890', ?, 'SELL', 'KVT2', 'b1', 't1', 'ONLINE', \
            '2026-05-09T12:34:56Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(lnd)
    .bind(&sha)
    .execute(pool)
    .await
    .expect("seed fiscal_documents Kvt2");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

async fn enqueue(pool: &SqlitePool, doc: DocumentId, lnd: i64, sha: [u8; 32]) {
    let fn_id = "1234567890".to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            outbox::enqueue_document_tx(
                tx,
                doc,
                NewOutboxRow {
                    fiscal_number: fn_id,
                    sequence_no: lnd,
                    payload_sha256: sha,
                },
            )
            .await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await
    .expect("enqueue_document_tx");
}

#[tokio::test]
async fn migration_011_creates_table_and_indexes() {
    let (_d, pool) = fresh_pool().await;

    let names: HashSet<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' ORDER BY 1")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .collect();
    assert!(
        names.contains("outbox"),
        "outbox table missing post-011; have {names:?}"
    );

    let indexes: HashSet<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='index' ORDER BY 1")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .collect();
    assert!(
        indexes.contains("ix_outbox_pending"),
        "ix_outbox_pending partial index missing; have {indexes:?}"
    );

    let cols: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(outbox)")
            .fetch_all(&pool)
            .await
            .unwrap();
    let col_names: HashSet<String> = cols.iter().map(|c| c.1.clone()).collect();
    for col in [
        "document_id",
        "fiscal_number",
        "sequence_no",
        "payload_sha256",
        "enqueued_at",
        "status",
        "published_at",
    ] {
        assert!(
            col_names.contains(col),
            "outbox missing {col}; have {col_names:?}"
        );
    }
}

#[tokio::test]
async fn enqueue_document_tx_inserts_with_defaults() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_kvt2_doc(&pool, 0x11, 1).await;
    enqueue(&pool, doc, 1, [7u8; 32]).await;

    let row = outbox::get_for_document(&pool, doc)
        .await
        .unwrap()
        .expect("outbox row must exist");
    assert_eq!(row.document_id, doc);
    assert_eq!(row.fiscal_number, "1234567890");
    assert_eq!(row.sequence_no, 1);
    assert_eq!(row.payload_sha256, [7u8; 32]);
    assert_eq!(
        row.status, "PENDING",
        "default status must be PENDING per DDL"
    );
    assert!(row.published_at.is_none(), "published_at must default NULL");
    assert!(
        !row.enqueued_at.is_empty(),
        "enqueued_at must be set by DEFAULT CURRENT_TIMESTAMP"
    );
}

#[tokio::test]
async fn enqueue_twice_for_same_doc_violates_pk() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_kvt2_doc(&pool, 0x22, 1).await;
    enqueue(&pool, doc, 1, [1u8; 32]).await;

    // Second enqueue for the same document_id MUST violate PK.  This is
    // defence-in-depth: stage_finalize::run is supposed to short-circuit
    // rerun-on-Ack BEFORE the outbox INSERT, but if that ever regresses,
    // the PK catches it as a hard sqlx error.
    let fn_id = "1234567890".to_string();
    let res: Result<(), anyhow::Error> = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            outbox::enqueue_document_tx(
                tx,
                doc,
                NewOutboxRow {
                    fiscal_number: fn_id,
                    sequence_no: 1,
                    payload_sha256: [2u8; 32],
                },
            )
            .await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await;

    let err = res.expect_err("duplicate INSERT must violate PK");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("unique") || msg.contains("constraint"),
        "expected UNIQUE / constraint error, got: {msg}"
    );
}

#[tokio::test]
async fn unknown_status_violates_check() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_kvt2_doc(&pool, 0x33, 1).await;
    let doc_bytes = doc.as_bytes().to_vec();
    let sha = vec![0u8; 32];
    let err = sqlx::query(
        "INSERT INTO outbox(document_id, fiscal_number, sequence_no, payload_sha256, status) \
         VALUES (?, '1234567890', 1, ?, 'NOT_A_STATUS')",
    )
    .bind(&doc_bytes)
    .bind(&sha)
    .execute(&pool)
    .await
    .expect_err("unknown status must violate CHECK");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("check") || msg.contains("constraint"),
        "expected CHECK error, got: {msg}"
    );
}

#[tokio::test]
async fn published_status_without_published_at_violates_check() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_kvt2_doc(&pool, 0x44, 1).await;
    enqueue(&pool, doc, 1, [3u8; 32]).await;

    // Try to flip status='PUBLISHED' WITHOUT setting published_at.
    // The all-or-none CHECK must reject.
    let err = sqlx::query("UPDATE outbox SET status = 'PUBLISHED' WHERE document_id = ?")
        .bind(doc)
        .execute(&pool)
        .await
        .expect_err("PUBLISHED without published_at must violate all-or-none CHECK");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("check") || msg.contains("constraint"),
        "expected CHECK error, got: {msg}"
    );

    // Flipping both atomically is allowed (mirrors the future
    // publisher worker's single-statement update).
    sqlx::query(
        "UPDATE outbox SET status = 'PUBLISHED', published_at = '2026-05-09 13:00:00' \
         WHERE document_id = ?",
    )
    .bind(doc)
    .execute(&pool)
    .await
    .expect("atomic PUBLISHED+published_at must succeed");

    let row = outbox::get_for_document(&pool, doc).await.unwrap().unwrap();
    assert_eq!(row.status, "PUBLISHED");
    assert_eq!(row.published_at.as_deref(), Some("2026-05-09 13:00:00"));
}

#[tokio::test]
async fn fk_restrict_blocks_doc_delete_with_pending_outbox() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_kvt2_doc(&pool, 0x77, 1).await;
    enqueue(&pool, doc, 1, [1u8; 32]).await;

    let err = sqlx::query("DELETE FROM fiscal_documents WHERE document_id = ?")
        .bind(doc)
        .execute(&pool)
        .await
        .expect_err("ON DELETE RESTRICT must block while outbox row references doc");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("foreign key"),
        "expected FK constraint error, got: {msg}"
    );
}

#[tokio::test]
async fn get_for_document_returns_none_for_missing_row() {
    let (_d, pool) = fresh_pool().await;
    let bogus = DocumentId::from_bytes([0xEEu8; 16]);
    assert!(outbox::get_for_document(&pool, bogus)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn pending_partial_index_filters_published_rows() {
    let (_d, pool) = fresh_pool().await;
    let doc_a = seed_kvt2_doc(&pool, 0x55, 1).await;
    let doc_b = seed_kvt2_doc(&pool, 0x66, 2).await;
    enqueue(&pool, doc_a, 1, [1u8; 32]).await;
    enqueue(&pool, doc_b, 2, [2u8; 32]).await;

    // Flip doc_b to PUBLISHED.
    sqlx::query(
        "UPDATE outbox SET status = 'PUBLISHED', published_at = '2026-05-09 13:00:00' \
         WHERE document_id = ?",
    )
    .bind(doc_b)
    .execute(&pool)
    .await
    .unwrap();

    // Query PENDING via the partial-index predicate.
    let pending: Vec<Vec<u8>> = sqlx::query_scalar(
        "SELECT document_id FROM outbox WHERE status = 'PENDING' ORDER BY enqueued_at",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(pending.len(), 1, "exactly one PENDING row expected");
    assert_eq!(pending[0], doc_a.as_bytes(), "PENDING row must be doc_a");
}
