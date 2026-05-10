//! Targeted verification for the W7.2 helpers on
//! `db::repositories::fiscal_documents`:
//!   - `fetch_send_inputs_tx` (4-pre read)
//!   - `mark_submission_attempted_tx` (4-pre UPDATE)
//!   - `set_server_fiscal_no_tx` (4-b UPDATE)
//!
//! Anchored on W7 design freeze §4.2.  The helpers are dumb seams: no
//! state machine logic lives here — that lives in stage_send (W7.4)
//! around them.  These tests pin the contract surface only.

use prro::db::models::enums::{DocState, DocType};
use prro::db::models::ids::DocumentId;
use prro::db::repositories::fiscal_documents;
use prro::db::tx::with_immediate;
use sqlx::SqlitePool;

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations");
    (dir, pool)
}

/// Internal: seed an FN config + a fiscal_documents row in the given
/// `state` with a fixed `lnd`.  Body shared by `seed_signed_doc`
/// (default "SIGNED") and the W10.4 claim-helper fixtures (which
/// need "ERROR_RETRYABLE" pre-state).
async fn seed_doc(
    pool: &SqlitePool,
    doc_byte: u8,
    doc_type: &str,
    lnd: i64,
    state: &str,
) -> DocumentId {
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
         VALUES (?, ?, '1234567890', ?, ?, ?, 'b1', 't1', 'ONLINE', \
            '2026-05-09T12:34:56Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(lnd)
    .bind(doc_type)
    .bind(state)
    .bind(&sha)
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

/// Thin wrapper for the W7 hand-off shape: a doc that has cleared
/// stage 3 sign (state=SIGNED) and is ready for stage 4 send.
async fn seed_signed_doc(pool: &SqlitePool, doc_byte: u8, doc_type: &str, lnd: i64) -> DocumentId {
    seed_doc(pool, doc_byte, doc_type, lnd, "SIGNED").await
}

#[tokio::test]
async fn fetch_send_inputs_returns_minimal_field_set() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_doc(&pool, 0x11, "SELL", 42).await;

    let inputs = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let r = fiscal_documents::fetch_send_inputs_tx(tx, doc).await?;
            Ok::<_, anyhow::Error>(r)
        })
    })
    .await
    .expect("fetch_send_inputs_tx")
    .expect("row must be present for seeded doc");

    assert_eq!(
        inputs.state,
        DocState::Signed,
        "pre-CAS state must be SIGNED"
    );
    assert_eq!(inputs.fiscal_number, "1234567890");
    assert_eq!(inputs.lnd, 42);
    assert_eq!(inputs.doc_type, DocType::Sell);
    assert_eq!(inputs.business_ts, "2026-05-09T12:34:56Z");
    assert_eq!(inputs.backend_profile_id, "b1");
    assert_eq!(inputs.transport_profile_id, "t1");
}

#[tokio::test]
async fn fetch_send_inputs_returns_none_for_missing_row() {
    let (_d, pool) = fresh_pool().await;
    let bogus = DocumentId::from_bytes([0xEEu8; 16]);

    let result = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let r = fiscal_documents::fetch_send_inputs_tx(tx, bogus).await?;
            Ok::<_, anyhow::Error>(r)
        })
    })
    .await
    .expect("fetch_send_inputs_tx");

    assert!(result.is_none(), "missing row must surface as None");
}

#[tokio::test]
async fn fetch_send_inputs_works_for_all_doc_types() {
    let (_d, pool) = fresh_pool().await;

    // Each variant of DocType used by stage 4 has a distinct doc_byte/lnd.
    for (byte, lnd, dt_str, expected) in [
        (0x21u8, 1i64, "SHIFT_OPEN", DocType::ShiftOpen),
        (0x22, 2, "SELL", DocType::Sell),
        (0x23, 3, "RETURN", DocType::Return),
        (0x24, 4, "SHIFT_CLOSE", DocType::ShiftClose),
        (0x25, 5, "Z_REPORT", DocType::ZReport),
    ] {
        let doc = seed_signed_doc(&pool, byte, dt_str, lnd).await;
        let inputs = with_immediate(&pool, move |tx| {
            Box::pin(async move {
                let r = fiscal_documents::fetch_send_inputs_tx(tx, doc).await?;
                Ok::<_, anyhow::Error>(r)
            })
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            inputs.doc_type, expected,
            "doc_type round-trip for {dt_str}"
        );
        assert_eq!(inputs.lnd, lnd);
    }
}

#[tokio::test]
async fn mark_submission_attempted_updates_existing_row() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_doc(&pool, 0x33, "SELL", 1).await;

    // Pre-condition: column starts NULL on a freshly seeded SIGNED row.
    let pre: Option<String> = sqlx::query_scalar(
        "SELECT submission_attempted_at FROM fiscal_documents WHERE document_id = ?",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(pre.is_none(), "submission_attempted_at must start NULL");

    let updated = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let b = fiscal_documents::mark_submission_attempted_tx(tx, doc).await?;
            Ok::<bool, anyhow::Error>(b)
        })
    })
    .await
    .expect("mark_submission_attempted_tx");
    assert!(updated, "existing row must report updated=true");

    // Persisted value visible outside the tx; format matches the
    // SQLite CURRENT_TIMESTAMP shape (`YYYY-MM-DD HH:MM:SS`).
    let v: Option<String> = sqlx::query_scalar(
        "SELECT submission_attempted_at FROM fiscal_documents WHERE document_id = ?",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .unwrap();
    let ts = v.expect("submission_attempted_at must be non-NULL after mark");
    // Loose shape check: `YYYY-MM-DD HH:MM:SS` is 19 chars, with
    // dashes / spaces in fixed positions.
    assert_eq!(
        ts.len(),
        19,
        "expected SQLite-flavoured timestamp, got {ts:?}"
    );
    assert_eq!(&ts[4..5], "-");
    assert_eq!(&ts[7..8], "-");
    assert_eq!(&ts[10..11], " ");
    assert_eq!(&ts[13..14], ":");
    assert_eq!(&ts[16..17], ":");
}

#[tokio::test]
async fn mark_submission_attempted_idempotent_preserves_first_stamp() {
    // R-W10.4-senior-review MED 1 close: the column promise is
    // "first submission attempt time".  W10.4 step 2d retry path
    // re-runs 4-pre (and thus mark_submission_attempted_tx) on
    // attempt #2; this fixture pins that the helper does NOT
    // overwrite the existing value on the second call.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_doc(&pool, 0xF1, "SELL", 99).await;

    // First call: column starts NULL, gets stamped.
    let first = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            fiscal_documents::mark_submission_attempted_tx(tx, doc)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("first stamp");
    assert!(first, "first stamp on existing row must return true");

    // Force the timestamp into a known sentinel value so the
    // second-call assertion is wall-clock-independent.
    sqlx::query(
        "UPDATE fiscal_documents SET submission_attempted_at = '2020-01-01 00:00:00' \
         WHERE document_id = ?",
    )
    .bind(doc)
    .execute(&pool)
    .await
    .unwrap();

    // Second call: must NOT overwrite the existing timestamp.
    let second = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            fiscal_documents::mark_submission_attempted_tx(tx, doc)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("second stamp");
    assert!(
        second,
        "row exists, helper must report success even when no UPDATE happens"
    );

    let post: Option<String> = sqlx::query_scalar(
        "SELECT submission_attempted_at FROM fiscal_documents WHERE document_id = ?",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        post.as_deref(),
        Some("2020-01-01 00:00:00"),
        "second helper call must preserve first-attempt timestamp \
         (idempotency contract — first-attempt-time semantic)"
    );
}

#[tokio::test]
async fn mark_submission_attempted_returns_false_for_missing_row() {
    let (_d, pool) = fresh_pool().await;
    let bogus = DocumentId::from_bytes([0xCCu8; 16]);

    let updated = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let b = fiscal_documents::mark_submission_attempted_tx(tx, bogus).await?;
            Ok::<bool, anyhow::Error>(b)
        })
    })
    .await
    .expect("mark_submission_attempted_tx");
    assert!(
        !updated,
        "missing row must report updated=false (not silent ignore)"
    );
}

#[tokio::test]
async fn set_server_fiscal_no_updates_existing_row() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_doc(&pool, 0x44, "SELL", 1).await;

    let updated = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let b = fiscal_documents::set_server_fiscal_no_tx(tx, doc, "DPS-FISCAL-7777").await?;
            Ok::<bool, anyhow::Error>(b)
        })
    })
    .await
    .expect("set_server_fiscal_no_tx");
    assert!(updated, "existing row must report updated=true");

    let v: Option<String> =
        sqlx::query_scalar("SELECT server_fiscal_no FROM fiscal_documents WHERE document_id = ?")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(v.as_deref(), Some("DPS-FISCAL-7777"));
}

#[tokio::test]
async fn set_server_fiscal_no_returns_false_for_missing_row() {
    let (_d, pool) = fresh_pool().await;
    let bogus = DocumentId::from_bytes([0xDDu8; 16]);

    let updated = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let b = fiscal_documents::set_server_fiscal_no_tx(tx, bogus, "X").await?;
            Ok::<bool, anyhow::Error>(b)
        })
    })
    .await
    .expect("set_server_fiscal_no_tx");
    assert!(
        !updated,
        "missing row must report updated=false (not silent ignore)"
    );
}

// ─── W10.4 — mac_recovery_claim_counter_tx ───────────────────────────

/// Thin wrapper: seed a doc directly in `ERROR_RETRYABLE` with
/// `mac_recovery_attempts = 0` (DDL DEFAULT).  Mirrors the
/// post-attempt-#1 4-b commit shape per freeze §4.4.1.
async fn seed_error_retryable_doc(pool: &SqlitePool, doc_byte: u8, lnd: i64) -> DocumentId {
    seed_doc(pool, doc_byte, "SELL", lnd, "ERROR_RETRYABLE").await
}

async fn read_mac_recovery_attempts(pool: &SqlitePool, doc: DocumentId) -> i64 {
    sqlx::query_scalar("SELECT mac_recovery_attempts FROM fiscal_documents WHERE document_id = ?")
        .bind(doc)
        .fetch_one(pool)
        .await
        .expect("read mac_recovery_attempts")
}

#[tokio::test]
async fn mac_recovery_claim_succeeds_on_error_retryable_with_zero_counter() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_error_retryable_doc(&pool, 0xE1, 1).await;
    assert_eq!(read_mac_recovery_attempts(&pool, doc).await, 0);

    let claimed = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let b = fiscal_documents::mac_recovery_claim_counter_tx(tx, doc).await?;
            Ok::<bool, anyhow::Error>(b)
        })
    })
    .await
    .expect("claim_counter_tx");
    assert!(
        claimed,
        "ERROR_RETRYABLE + counter=0 MUST allow the single-bit claim"
    );
    assert_eq!(
        read_mac_recovery_attempts(&pool, doc).await,
        1,
        "claim must bump counter 0 → 1"
    );
}

#[tokio::test]
async fn mac_recovery_claim_is_single_bit_second_call_returns_false() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_error_retryable_doc(&pool, 0xE2, 2).await;

    // First claim succeeds.
    let first = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            fiscal_documents::mac_recovery_claim_counter_tx(tx, doc)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("first claim_counter_tx");
    assert!(first);

    // Second claim fails — counter already 1.
    let second = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            fiscal_documents::mac_recovery_claim_counter_tx(tx, doc)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("second claim_counter_tx");
    assert!(
        !second,
        "single-bit budget: second claim MUST fail (counter already 1)"
    );
    assert_eq!(read_mac_recovery_attempts(&pool, doc).await, 1);
}

#[tokio::test]
async fn mac_recovery_claim_fails_on_non_error_retryable_state() {
    // Seed in SIGNED state — shouldn't go through MAC recovery.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_doc(&pool, 0xE3, "SELL", 3).await;

    let claimed = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            fiscal_documents::mac_recovery_claim_counter_tx(tx, doc)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("claim_counter_tx");
    assert!(
        !claimed,
        "non-ERROR_RETRYABLE state MUST NOT pass the CAS guard"
    );
    assert_eq!(
        read_mac_recovery_attempts(&pool, doc).await,
        0,
        "failed claim must NOT bump the counter"
    );
}

#[tokio::test]
async fn mac_recovery_claim_fails_on_missing_row() {
    let (_d, pool) = fresh_pool().await;
    let bogus = DocumentId::from_bytes([0xCCu8; 16]);

    let claimed = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            fiscal_documents::mac_recovery_claim_counter_tx(tx, bogus)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("claim_counter_tx");
    assert!(
        !claimed,
        "missing row MUST report claimed=false (not silent ignore)"
    );
}
