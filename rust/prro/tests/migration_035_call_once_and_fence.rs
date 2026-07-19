//! Migration 035 — per-document lifetime call-once + narrowed §3.1 active-fence.
//!
//! RED-first teeth for the CS-3 D/E Slice 1 schema (design §2 + §3.1). INACTIVE:
//! the migration only adds an index and replaces the two fence DB objects; no
//! caller is wired. Evidence (columns + matrix) is deferred to Slice 2.
//!
//! Groups:
//! - `co_*` — P2 lifetime call-once (the ux_delivery_document_ever_started index +
//!   the no_replace historical-document-started clause).
//! - `fn_*` — P3 narrowed §3.1 fence (PENDING_APPLY holds, APPLIED / definitive
//!   resolution releases; SubmittedUnknown / routed-Submitted are NO LONGER
//!   index-held — they are held by STOP_MODE in Slice 5).
//! - `fc_*` — fence-consistency (the §3.1 predicate is byte-identical in the index
//!   and the no_replace trigger).
//! - `ia_*` — INACTIVE-safety (the empty-table activation guard).

use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{self, NewReservation};
use prro::db::tx::with_immediate;
use sqlx::SqlitePool;

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations including 035");
    (dir, pool)
}

const FN_A: &str = "1234567890";

async fn seed_fn(pool: &SqlitePool, fscl: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fscl)
    .execute(pool)
    .await
    .expect("seed fiscal_number_config");
}

async fn seed_doc(pool: &SqlitePool, fscl: &str, doc_byte: u8, lnd: i64) -> DocumentId {
    seed_fn(pool, fscl).await;
    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, ?, ?, 'SELL', 'SENDING', 'b1', 't1', 'ONLINE', \
            '2026-07-17T12:34:56Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(fscl)
    .bind(lnd)
    .bind(&sha)
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

fn new_res(res_byte: u8, doc: DocumentId, fscl: &str) -> NewReservation {
    NewReservation {
        reservation_id: [res_byte; 16],
        document_id: doc,
        fiscal_number: fscl.to_string(),
        dps_protocol_id: "FSCO_ZZD".to_string(),
        protocol_contract_version: 1,
        capability_profile_version: None,
        endpoint_config_revision: None,
        envelope_hash: [0xAB; 32],
    }
}

async fn insert_res(pool: &SqlitePool, row: NewReservation) -> anyhow::Result<i64> {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            delivery_reservation::insert(tx, row)
                .await
                .map_err(Into::into)
        })
    })
    .await
}

async fn mark_call_started(pool: &SqlitePool, res_byte: u8) {
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'CALL_STARTED', call_started_at = '2026-07-17T00:00:00Z', \
             authorized_generation = 1 \
         WHERE reservation_id = ?",
    )
    .bind(&[res_byte; 16][..])
    .execute(pool)
    .await
    .expect("RN→CS");
}

// ─────────────────────────── co_* — lifetime call-once ───────────────────────

#[tokio::test]
async fn co01_index_exists() {
    let (_d, pool) = fresh_pool().await;
    let sql: Option<String> = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type='index' AND name='ux_delivery_document_ever_started'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    let sql = sql.expect("ux_delivery_document_ever_started must exist");
    assert!(
        sql.contains("call_started_at IS NOT NULL"),
        "must be the partial call-once index: {sql}"
    );
}

#[tokio::test]
async fn co02_second_reservation_after_call_started_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    mark_call_started(&pool, 0x01).await;
    // A fresh RN for a document that ever started a wire is refused (no_replace
    // historical-document-started clause). This is the P2 lifetime guard.
    let err = insert_res(&pool, new_res(0x02, doc, FN_A))
        .await
        .expect_err("a 2nd reservation for a started document must be rejected");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("document-ever-started") || msg.contains("forbidden") || msg.contains("abort"),
        "expected the call-once rejection; got: {err}"
    );
}

#[tokio::test]
async fn co03_second_reservation_for_unstarted_doc_after_release_ok() {
    // A doc whose first reservation NEVER started (safe NotSubmitted, call_started_at
    // NULL) may get a fresh attempt once the fence releases.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    // RN → OO(NOT_SUBMITTED) → APPLIED: never started, fence released.
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state='OUTCOME_OBSERVED', submission_certainty='NOT_SUBMITTED', \
             response_provenance='NO_RESPONSE', routing_class='TransientRetry', \
             apply_state='PENDING_APPLY', node_effect='NoNodeEffect' WHERE reservation_id=?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE delivery_reservation SET apply_state='APPLIED' WHERE reservation_id=?")
        .bind(&[0x01u8; 16][..])
        .execute(&pool)
        .await
        .unwrap();
    let a2 = insert_res(&pool, new_res(0x02, doc, FN_A))
        .await
        .expect("a never-started document gets a fresh attempt after release");
    assert_eq!(a2, 2, "attempt_no increments");
}

// ─────────────────────────── fn_* — narrowed §3.1 fence ──────────────────────

#[tokio::test]
async fn fn01_pending_apply_holds_applied_releases() {
    let (_d, pool) = fresh_pool().await;
    let doc1 = seed_doc(&pool, FN_A, 0x11, 1).await;
    let doc2 = seed_doc(&pool, FN_A, 0x22, 2).await;
    insert_res(&pool, new_res(0x01, doc1, FN_A)).await.unwrap();
    mark_call_started(&pool, 0x01).await;
    // OUTCOME_OBSERVED + PENDING_APPLY HOLDS the fence.
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state='OUTCOME_OBSERVED', submission_certainty='SUBMITTED', \
             response_provenance='PARSED_DPS_ENVELOPE', apply_state='PENDING_APPLY', \
             node_effect='NoNodeEffect' WHERE reservation_id=?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .unwrap();
    insert_res(&pool, new_res(0x02, doc2, FN_A))
        .await
        .expect_err("PENDING_APPLY holds the FN fence — a new doc is blocked");
    // APPLIED releases.
    sqlx::query("UPDATE delivery_reservation SET apply_state='APPLIED' WHERE reservation_id=?")
        .bind(&[0x01u8; 16][..])
        .execute(&pool)
        .await
        .unwrap();
    insert_res(&pool, new_res(0x02, doc2, FN_A))
        .await
        .expect("APPLIED releases the fence — the new doc is accepted");
    let active = delivery_reservation::get_active_for_fn(&pool, FN_A)
        .await
        .unwrap()
        .expect("the new reservation is active");
    assert_eq!(active.reservation_id, [0x02; 16]);
}

#[tokio::test]
async fn fn02_submitted_unknown_no_longer_index_held() {
    // §3.1 change from the 032/033 fence: a SubmittedUnknown at APPLIED is NOT held
    // by the SQL fence (it is held by STOP_MODE in Slice 5). get_active_for_fn (§3.1)
    // returns None for it.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    mark_call_started(&pool, 0x01).await;
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state='OUTCOME_OBSERVED', submission_certainty='SUBMITTED_UNKNOWN', \
             response_provenance='NO_RESPONSE', routing_class='TransientRetry', \
             apply_state='PENDING_APPLY', node_effect='NoNodeEffect' WHERE reservation_id=?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE delivery_reservation SET apply_state='APPLIED' WHERE reservation_id=?")
        .bind(&[0x01u8; 16][..])
        .execute(&pool)
        .await
        .unwrap();
    let active = delivery_reservation::get_active_for_fn(&pool, FN_A)
        .await
        .unwrap();
    assert!(
        active.is_none(),
        "§3.1: a SubmittedUnknown at APPLIED is not SQL-fence-held (STOP_MODE holds it, Slice 5)"
    );
}

// ─────────────────────────── fc_* — fence consistency ────────────────────────

#[tokio::test]
async fn fc01_section_31_predicate_byte_identical_in_index_and_trigger() {
    let (_d, pool) = fresh_pool().await;
    let idx: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type='index' AND name='ux_reservation_active'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let trg: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type='trigger' AND name='delivery_reservation_no_replace'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let needle = "state IN ('RESERVED_NOT_STARTED','CALL_STARTED')";
    let pending = "state = 'OUTCOME_OBSERVED' AND apply_state = 'PENDING_APPLY'";
    for (what, sql) in [("index", &idx), ("trigger", &trg)] {
        assert!(sql.contains(needle), "{what} missing §3.1 head: {sql}");
        assert!(
            sql.contains(pending),
            "{what} missing §3.1 PENDING_APPLY clause: {sql}"
        );
    }
    // The old 032/033 cert/routing disjunct must be GONE from both.
    for (what, sql) in [("index", &idx), ("trigger", &trg)] {
        assert!(
            !sql.contains("submission_certainty = 'SUBMITTED_UNKNOWN'"),
            "{what} still carries the retired cert/routing fence: {sql}"
        );
    }
}

// ─────────────────────────── ia_* — INACTIVE-safety ──────────────────────────

#[tokio::test]
async fn ia01_activation_guard_rejects_nonempty_table() {
    // Replicates the migration-035 activation guard against a DB that already has a
    // delivery_reservation row: the CHECK (row_count = 0) must abort. (open_pool runs
    // migrations before any row exists, so the guard is exercised via its raw SQL.)
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    let mut conn = pool.acquire().await.unwrap();
    sqlx::query("DROP TABLE IF EXISTS _m035_guard_probe")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TEMP TABLE _m035_guard_probe (row_count INTEGER NOT NULL CHECK (row_count = 0))",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    let err = sqlx::query(
        "INSERT INTO _m035_guard_probe (row_count) SELECT COUNT(*) FROM delivery_reservation",
    )
    .execute(&mut *conn)
    .await
    .expect_err("the guard must reject a non-empty delivery_reservation");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("check") || msg.contains("constraint"),
        "expected a CHECK failure; got: {err}"
    );
}
