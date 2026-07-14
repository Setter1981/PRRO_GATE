//! RS-3 inbox-reaper — stale-inbox sweep gate tests.
//!
//! D-R1 (terminalise-only) · D-R2 (inbox+audit ONLY, zero doc writes) · D-R3
//! (SSOT classify via the replay classifiers).  Block 1 drives `sweep(pool,
//! None)` (boot semantics — no age-gate) over each doc-state class; Block 2
//! adds the age-gate (f/g).
//!
//! RED-first: against the `sweep` NotYet stub these all fail (no terminalise);
//! GREEN once the real sweep is wired.

use std::time::Duration;

use sha2::{Digest, Sha256};

use prro::db::models::ids::{DocumentId, RequestId};
use prro::db::open_pool;
use prro::db::repositories::ingress_inbox::{self as inbox, InboxInsertOutcome, NewInboxEntry};
use prro::db::types::DbDocumentId;
use prro::runtime::ingress::replay::{resolve_replay, ReplayResolution};
use prro::services::reconciliation::inbox_reaper::{sweep, REAPER_STALE_THRESHOLD};
use sqlx::SqlitePool;

const FN: &str = "4000000001";

async fn fresh_pool() -> SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reaper.db");
    std::mem::forget(dir);
    open_pool(&path).await.unwrap()
}

async fn seed_fn(pool: &SqlitePool) {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config (fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(FN)
    .execute(pool)
    .await
    .unwrap();
}

fn hex16(b: &[u8; 16]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Insert a NEW inbox row (via the real repo) and, if `status != NEW`, CAS it to
/// the chosen status (stamping `processed_at`, as `acquire_lease` would).
async fn seed_inbox(pool: &SqlitePool, status: &str) -> [u8; 16] {
    let rid: [u8; 16] = *RequestId::new().as_bytes();
    inbox::insert(
        pool,
        &NewInboxEntry {
            request_id: rid,
            fiscal_number: FN.into(),
            protocol: prro::db::models::enums::Protocol::Rest,
            operation_type: "SELL".into(),
            idempotency_key: format!("idem-{}", hex16(&rid)),
            payload_json: "{}".into(),
            payload_sha256_canonical: Sha256::digest(b"{}").into(),
            correlation_id: None,
            signed_by_cashier_id: None,
            driver_id: None,
            business_ts: None,
            total_sum_kop: None,
        },
    )
    .await
    .unwrap();
    if status != "NEW" {
        sqlx::query(
            "UPDATE ingress_inbox SET status = ?, processed_at = CURRENT_TIMESTAMP \
             WHERE request_id = ?",
        )
        .bind(status)
        .bind(&rid[..])
        .execute(pool)
        .await
        .unwrap();
    }
    rid
}

/// Insert a fiscal_documents row at `state` for `rid` (so the reaper's
/// `terminal_outcome_by_request_id` join finds it).
async fn seed_doc(pool: &SqlitePool, rid: &[u8; 16], state: &str, lnd: i64) {
    let doc_id = DocumentId::new();
    sqlx::query(
        "INSERT INTO fiscal_documents (\
            document_id, request_id, fiscal_number, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            total_sum_kop, payload_json, payload_sha256_canonical, server_fiscal_no\
         ) VALUES (?, ?, ?, ?, 'SELL', ?, 'b', 't', 'ONLINE', \
            '2026-06-09T12:00:00Z', 15000, '{}', ?, ?)",
    )
    .bind(DbDocumentId(doc_id))
    .bind(&rid[..])
    .bind(FN)
    .bind(lnd)
    .bind(state)
    .bind(&[0u8; 32][..])
    .bind(if state == "ACK" || state == "SENT" {
        Some("D-1")
    } else {
        None
    })
    .execute(pool)
    .await
    .unwrap();
}

async fn read_inbox_status(pool: &SqlitePool, rid: &[u8; 16]) -> String {
    sqlx::query_scalar("SELECT status FROM ingress_inbox WHERE request_id = ?")
        .bind(&rid[..])
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn audit_severity(pool: &SqlitePool, event_type: &str) -> Option<String> {
    sqlx::query_scalar("SELECT severity FROM audit_log WHERE event_type = ? LIMIT 1")
        .bind(event_type)
        .fetch_optional(pool)
        .await
        .unwrap()
}

// ════════════════════════════════════════════════════════════════════════
// (a) PROCESSING + no doc → REJECTED + INBOX_REAPER_NO_DOC_TERMINALISED(Critical)
// ════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn reap_processing_no_doc_terminalises_rejected_critical() {
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    let rid = seed_inbox(&pool, "PROCESSING").await;

    let summary = sweep(&pool, None).await.expect("sweep ok");

    assert_eq!(read_inbox_status(&pool, &rid).await, "REJECTED");
    assert_eq!(summary.no_doc_terminalised, 1);
    assert_eq!(
        audit_severity(&pool, "INBOX_REAPER_NO_DOC_TERMINALISED")
            .await
            .as_deref(),
        Some("CRITICAL"),
        "the no-doc terminalise is the ruling-2/finding-1 CRITICAL operator surface"
    );
}

// ════════════════════════════════════════════════════════════════════════
// (b) PROCESSING + accepted (ACK) doc → DONE (accounting convergence)
// ════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn reap_processing_accepted_doc_converges_done() {
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    let rid = seed_inbox(&pool, "PROCESSING").await;
    seed_doc(&pool, &rid, "ACK", 1).await;

    let summary = sweep(&pool, None).await.expect("sweep ok");

    assert_eq!(read_inbox_status(&pool, &rid).await, "DONE");
    assert_eq!(summary.accepted_converged_done, 1);
}

// ════════════════════════════════════════════════════════════════════════
// (c) PROCESSING + terminally-failed (REJECTED) doc → REJECTED
// ════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn reap_processing_failed_doc_terminalises_rejected() {
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    let rid = seed_inbox(&pool, "PROCESSING").await;
    seed_doc(&pool, &rid, "REJECTED", 1).await;

    let summary = sweep(&pool, None).await.expect("sweep ok");

    assert_eq!(read_inbox_status(&pool, &rid).await, "REJECTED");
    assert_eq!(summary.failed_terminalised, 1);
}

// ════════════════════════════════════════════════════════════════════════
// (d) PROCESSING + in-flight (SENT) doc → UNTOUCHED  [load-bearing negative + teeth]
// ════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn reap_processing_in_flight_doc_is_untouched() {
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    let rid = seed_inbox(&pool, "PROCESSING").await;
    seed_doc(&pool, &rid, "SENT", 1).await;

    let summary = sweep(&pool, None).await.expect("sweep ok");

    assert_eq!(
        read_inbox_status(&pool, &rid).await,
        "PROCESSING",
        "recon owns an in-flight doc; the reaper must NOT terminalise it (would lie a Sent receipt is failed)"
    );
    assert_eq!(summary.in_flight_skipped, 1);
    assert_eq!(summary.no_doc_terminalised, 0);
    assert_eq!(summary.failed_terminalised, 0);
}

// ════════════════════════════════════════════════════════════════════════
// (e) NEW + no doc → REJECTED  (finding-1: pre-lease binding failure = NEW-forever)
// ════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn reap_new_no_doc_terminalises_rejected() {
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    let rid = seed_inbox(&pool, "NEW").await;

    let summary = sweep(&pool, None).await.expect("sweep ok");

    assert_eq!(read_inbox_status(&pool, &rid).await, "REJECTED");
    assert_eq!(summary.no_doc_terminalised, 1);
}

// ════════════════════════════════════════════════════════════════════════
// (h) replay AFTER reap → honest terminal INBOX_REJECTED (never 202-forever)
// ════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn replay_after_reap_is_terminal_inbox_rejected() {
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    let rid = seed_inbox(&pool, "PROCESSING").await;
    let idem = format!("idem-{}", hex16(&rid));

    sweep(&pool, None).await.expect("sweep ok");
    assert_eq!(read_inbox_status(&pool, &rid).await, "REJECTED");

    // A client re-submits the SAME idempotency key → dedups to Replay(row) with
    // the reaped REJECTED status → resolve_replay short-circuits to a terminal
    // Failed(INBOX_REJECTED), NOT InProgress/202-forever.
    let replay = inbox::insert(
        &pool,
        &NewInboxEntry {
            request_id: *RequestId::new().as_bytes(),
            fiscal_number: FN.into(),
            protocol: prro::db::models::enums::Protocol::Rest,
            operation_type: "SELL".into(),
            idempotency_key: idem,
            payload_json: "{}".into(),
            payload_sha256_canonical: Sha256::digest(b"{}").into(),
            correlation_id: None,
            signed_by_cashier_id: None,
            driver_id: None,
            business_ts: None,
            total_sum_kop: None,
        },
    )
    .await
    .unwrap();
    let row = match replay {
        InboxInsertOutcome::Replay(row) => row,
        other => panic!("expected Replay after reap, got {other:?}"),
    };
    assert_eq!(row.status, "REJECTED");
    match resolve_replay(&row, &pool).await.unwrap() {
        ReplayResolution::Failed(e) => assert_eq!(e.error_code, "INBOX_REJECTED"),
        other => panic!("replay after reap must be terminal Failed(INBOX_REJECTED), got {other:?}"),
    }
}

/// Force a row's lease/arrival timestamps to an OLD value so the age-gate sees
/// it as stale (both columns, so the `COALESCE` anchor is old for NEW or
/// PROCESSING).
async fn age_row(pool: &SqlitePool, rid: &[u8; 16]) {
    sqlx::query(
        "UPDATE ingress_inbox SET received_at = '2020-01-01 00:00:00', \
             processed_at = '2020-01-01 00:00:00' WHERE request_id = ?",
    )
    .bind(&rid[..])
    .execute(pool)
    .await
    .unwrap();
}

// ════════════════════════════════════════════════════════════════════════
// (f) FRESH PROCESSING (age < threshold) → UNTOUCHED  [anti-race load-bearing]
// ════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn runtime_sweep_leaves_fresh_processing_untouched() {
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    // Just-leased (processed_at = now) with no doc yet — the acquire→sign window
    // of a LIVE fiscalize.  The age-gate must NOT reap it.
    let rid = seed_inbox(&pool, "PROCESSING").await;

    let summary = sweep(&pool, Some(REAPER_STALE_THRESHOLD))
        .await
        .expect("sweep ok");

    assert_eq!(
        read_inbox_status(&pool, &rid).await,
        "PROCESSING",
        "a fresh PROCESSING row is a live fiscalize mid-flight — NEVER reap it"
    );
    assert_eq!(
        summary.scanned, 0,
        "the age-gate excludes fresh rows from the candidate set"
    );
    assert_eq!(summary.no_doc_terminalised, 0);
}

// ════════════════════════════════════════════════════════════════════════
// (g) AGED PROCESSING + no doc → REJECTED
// ════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn runtime_sweep_reaps_aged_processing_no_doc() {
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    let rid = seed_inbox(&pool, "PROCESSING").await;
    age_row(&pool, &rid).await;

    let summary = sweep(&pool, Some(REAPER_STALE_THRESHOLD))
        .await
        .expect("sweep ok");

    assert_eq!(read_inbox_status(&pool, &rid).await, "REJECTED");
    assert_eq!(summary.no_doc_terminalised, 1);
}

// Keep the threshold const referenced so a Block-2 rename surfaces here too.
#[test]
fn threshold_is_15_min() {
    assert_eq!(REAPER_STALE_THRESHOLD, Duration::from_secs(15 * 60));
}
