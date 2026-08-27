//! CS-3 Slice 6 — atomic `Sent + NotFound` escalation.
//!
//! RED-first teeth for `reconciliation::sent_not_found::sent_not_found_to_manual` (design §3.5).
//! A SENT-but-NotFound document escalates to manual reconciliation in one tx: `Sent -> RMR` + node
//! `STOP_MODE` + trace complete + Critical audit — all-or-nothing. INACTIVE — the boot / drain
//! producers are retargeted at the cutover (Slice 7).
//!
//! - `sn01` happy: doc → RMR, node → STOP_MODE, open trace completed, Critical audit appended;
//! - `sn02` a non-SENT document is refused (`DocNotSent`) — nothing mutated;
//! - `sn03` on a refusal the node mode is unchanged (atomic — no partial STOP);
//! - `sn04` a missing node_state row rolls the whole tx back (the doc stays SENT).

use prro::db::models::ids::DocumentId;
use prro::db::tx::with_immediate;
use prro::services::reconciliation::sent_not_found::{sent_not_found_to_manual, SentNotFoundError};
use sqlx::SqlitePool;

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations");
    (dir, pool)
}

/// Seed FN config + node_state + a fiscal document in the given state.
async fn seed_doc(
    pool: &SqlitePool,
    fscl: &str,
    doc_byte: u8,
    state: &str,
    with_node: bool,
) -> DocumentId {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fscl)
    .execute(pool)
    .await
    .unwrap();
    if with_node {
        sqlx::query(
            "INSERT OR IGNORE INTO node_state (fiscal_number, mode, shift_state, next_lnd) \
             VALUES (?, 'ONLINE', 'CREATED', 1)",
        )
        .bind(fscl)
        .execute(pool)
        .await
        .unwrap();
    }
    let doc_bytes = vec![doc_byte; 16];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, ?, 1, 'SELL', ?, 'b1', 't1', 'ONLINE', \
            '2026-07-17T12:34:56Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(vec![doc_byte ^ 0xFF; 16])
    .bind(fscl)
    .bind(state)
    .bind(vec![0u8; 32])
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

async fn escalate(pool: &SqlitePool, doc: DocumentId, fscl: &str) -> Result<(), anyhow::Error> {
    let fscl = fscl.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            sent_not_found_to_manual(tx, doc, &fscl)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
}

async fn read_doc_state(pool: &SqlitePool, doc_byte: u8) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id=?")
        .bind(vec![doc_byte; 16])
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn read_mode(pool: &SqlitePool, fscl: &str) -> Option<String> {
    sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number=?")
        .bind(fscl)
        .fetch_optional(pool)
        .await
        .unwrap()
}
async fn count_audit(pool: &SqlitePool, event: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type=?")
        .bind(event)
        .fetch_one(pool)
        .await
        .unwrap()
}
fn is_err(err: &anyhow::Error, pred: impl Fn(&SentNotFoundError) -> bool) -> bool {
    err.downcast_ref::<SentNotFoundError>().is_some_and(pred)
}

#[tokio::test]
async fn sn01_sent_not_found_escalates_atomically() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "6000000001";
    let doc = seed_doc(&pool, fscl, 0x11, "SENT", true).await;

    escalate(&pool, doc, fscl)
        .await
        .expect("SENT+NotFound escalates");

    // The three fiscal-critical writes committed together (the recovery-trace completion is the
    // producer's write with the probe's own data — see the helper doc).
    assert_eq!(
        read_doc_state(&pool, 0x11).await,
        "REQUIRES_MANUAL_RECONCILIATION"
    );
    assert_eq!(read_mode(&pool, fscl).await.as_deref(), Some("STOP_MODE"));
    assert_eq!(
        count_audit(&pool, "SENT_NOT_FOUND_ESCALATED_MANUAL").await,
        1
    );
    // bd PRRO_GATE-m6e — the Critical audit's entity_id IS the operator's pointer to the
    // escalated document: assert the exact lowercase hex of the doc id (retro-1rw survivors
    // `hex_lower -> "xyzzy"/""` proved nothing pinned it — a corrupted id would strand the
    // operator with an unlocatable RMR doc).
    let entity_id: String = sqlx::query_scalar(
        "SELECT entity_id FROM audit_log WHERE event_type='SENT_NOT_FOUND_ESCALATED_MANUAL'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        entity_id,
        "11".repeat(16),
        "audit entity_id must be the lowercase hex of the escalated doc id"
    );
}

#[tokio::test]
async fn sn02_non_sent_document_refused() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "6000000002";
    let doc = seed_doc(&pool, fscl, 0x11, "SIGNED", true).await; // not SENT
    let err = escalate(&pool, doc, fscl)
        .await
        .expect_err("only a SENT document escalates");
    assert!(is_err(&err, |e| matches!(
        e,
        SentNotFoundError::DocNotSent(_)
    )));
    assert_eq!(read_doc_state(&pool, 0x11).await, "SIGNED");
}

#[tokio::test]
async fn sn03_refusal_leaves_node_mode_unchanged() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "6000000003";
    let doc = seed_doc(&pool, fscl, 0x11, "SIGNED", true).await;
    assert_eq!(read_mode(&pool, fscl).await.as_deref(), Some("ONLINE"));
    let _ = escalate(&pool, doc, fscl).await.expect_err("refused");
    // The doc CAS failed first → the node mode was never touched (no partial STOP).
    assert_eq!(read_mode(&pool, fscl).await.as_deref(), Some("ONLINE"));
    assert_eq!(
        count_audit(&pool, "SENT_NOT_FOUND_ESCALATED_MANUAL").await,
        0
    );
}

#[tokio::test]
async fn sn04_missing_node_state_rolls_back() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "6000000004";
    let doc = seed_doc(&pool, fscl, 0x11, "SENT", false).await; // no node_state row
    let err = escalate(&pool, doc, fscl)
        .await
        .expect_err("missing node_state refuses");
    assert!(is_err(&err, |e| matches!(
        e,
        SentNotFoundError::NodeStateMissing
    )));
    // The doc CAS (step 1) is rolled back with the failed step 2 → doc stays SENT.
    assert_eq!(read_doc_state(&pool, 0x11).await, "SENT");
}
