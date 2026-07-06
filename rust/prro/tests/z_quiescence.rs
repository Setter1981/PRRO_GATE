//! RS-3 A1Z — DB-bound coverage for the pre-aggregation quiescence
//! (`z_builder::quiesce_shift_before_z`) + the issued-receipt aggregate
//! boundary. Pins the operator-locked quiescence contract:
//!   - KVT2 is finalized inline (→ Ack) and then counted by the aggregate;
//!   - SIGNED / SENT / KVT1 (and PREPARED/ENCRYPTED/SENDING/ERROR_RETRYABLE)
//!     are blocking → Pending, the Z is NOT created;
//!   - ACK / OFFLINE_LOCAL_ACK are issued (non-blocking, counted);
//!   - REJECTED / CANCELLED are terminal-non-issued (non-blocking, excluded).

use prro::db::models::ids::{DocumentId, ShiftId};
use prro::db::open_pool;
use prro::db::repositories::fiscal_documents;
use prro::runtime::ingress::convert::{aggregate_z_payload, aggregate_z_payload_for_shift};
use prro::runtime::ingress::z_builder::{quiesce_shift_before_z, QuiescenceOutcome};

const FN: &str = "1234567890";

async fn fresh_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    std::mem::forget(dir);
    let pool = open_pool(&path).await.unwrap();
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    pool
}

async fn seed_open_shift(pool: &sqlx::SqlitePool) -> ShiftId {
    let shift_id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, serial, state, open_mode, \
            cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED', 'ONLINE', 0, 'cashier')",
    )
    .bind(shift_id)
    .bind(FN)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO node_state (fiscal_number, mode, shift_state, current_shift_id, \
            next_lnd, backend_profile_id, transport_profile_id) \
         VALUES (?, 'ONLINE', 'OPENED', ?, 1, 'b', 't')",
    )
    .bind(FN)
    .bind(shift_id)
    .execute(pool)
    .await
    .unwrap();
    shift_id
}

/// Minimal SELL receipt in `state` (non-finalize states only — no chain).
async fn seed_receipt(
    pool: &sqlx::SqlitePool,
    shift_id: ShiftId,
    lnd: i64,
    state: &str,
) -> DocumentId {
    let doc_id = DocumentId::new();
    let req = vec![lnd as u8; 16];
    sqlx::query(
        "INSERT INTO fiscal_documents( \
            document_id, request_id, fiscal_number, shift_id, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical \
         ) VALUES (?, ?, ?, ?, ?, 'SELL', ?, 'b', 't', 'ONLINE', '2026-06-08T00:00:00Z', '{}', ?)",
    )
    .bind(doc_id)
    .bind(req)
    .bind(FN)
    .bind(shift_id)
    .bind(lnd)
    .bind(state)
    .bind(&[0u8; 32][..])
    .execute(pool)
    .await
    .unwrap();
    doc_id
}

/// Seed a SELL doc in `state` with explicit D3 discriminator fields
/// (`server_fiscal_no` / `offline_fiscal_no`) — needed for the C10 quiescence
/// disjunct which keys on the online-issued-unconfirmed signature.
async fn seed_receipt_d3(
    pool: &sqlx::SqlitePool,
    shift_id: ShiftId,
    lnd: i64,
    state: &str,
    server_fiscal_no: Option<&str>,
    offline_fiscal_no: Option<i64>,
) -> DocumentId {
    let doc_id = DocumentId::new();
    let req = vec![lnd as u8; 16];
    sqlx::query(
        "INSERT INTO fiscal_documents( \
            document_id, request_id, fiscal_number, shift_id, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, server_fiscal_no, offline_fiscal_no \
         ) VALUES (?, ?, ?, ?, ?, 'SELL', ?, 'b', 't', 'ONLINE', '2026-06-08T00:00:00Z', \
            '{}', ?, ?, ?)",
    )
    .bind(doc_id)
    .bind(req)
    .bind(FN)
    .bind(shift_id)
    .bind(lnd)
    .bind(state)
    .bind(&[0u8; 32][..])
    .bind(server_fiscal_no)
    .bind(offline_fiscal_no)
    .execute(pool)
    .await
    .unwrap();
    doc_id
}

async fn quiesce(pool: &sqlx::SqlitePool, shift_id: ShiftId) -> QuiescenceOutcome {
    quiesce_shift_before_z(pool, FN, shift_id).await.unwrap()
}

async fn issued_count(pool: &sqlx::SqlitePool, shift_id: ShiftId) -> usize {
    fiscal_documents::list_shift_issued_receipts(pool, FN, shift_id)
        .await
        .unwrap()
        .len()
}

#[tokio::test]
async fn aggregate_for_shift_uses_the_explicit_shift_not_current() {
    // wide-audit MEDIUM: aggregate_z_payload_for_shift must aggregate the GIVEN
    // shift, NOT re-read node_state.current_shift_id (so a transition between
    // quiesce and aggregate can't switch the shift). Proof: clear
    // current_shift_id → the wrapper (which reads it) fails, but _for_shift with
    // the explicit shift still succeeds.
    let pool = fresh_pool().await;
    let shift = seed_open_shift(&pool).await;
    sqlx::query("UPDATE node_state SET current_shift_id = NULL WHERE fiscal_number = ?")
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();

    // The wrapper re-reads current_shift_id (now NULL) → no open shift error.
    assert!(
        aggregate_z_payload(&pool, FN).await.is_err(),
        "wrapper must read current_shift_id (NULL → error)"
    );
    // _for_shift ignores current_shift_id and aggregates the explicit shift.
    assert!(
        aggregate_z_payload_for_shift(&pool, FN, shift)
            .await
            .is_ok(),
        "_for_shift must use the explicit shift_id, not current_shift_id"
    );
}

#[tokio::test]
async fn clear_when_only_issued_receipts() {
    let pool = fresh_pool().await;
    let shift = seed_open_shift(&pool).await;
    seed_receipt(&pool, shift, 1, "ACK").await;
    seed_receipt(&pool, shift, 2, "OFFLINE_LOCAL_ACK").await;

    assert_eq!(quiesce(&pool, shift).await, QuiescenceOutcome::Clear);
    // Both issued receipts are counted by the aggregate boundary.
    assert_eq!(issued_count(&pool, shift).await, 2);
}

#[tokio::test]
async fn pending_for_every_non_kvt2_blocking_state() {
    // All 7 non-KVT2 in-flight states (KVT2 is the only inline-finalizable one)
    // must block aggregation → Pending. (KVT2-only / KVT2-leading are pinned by
    // the kvt2_* tests below.)
    for blocking_state in [
        "PREPARED",
        "SIGNED",
        "ENCRYPTED",
        "SENDING",
        "SENT",
        "KVT1",
        "ERROR_RETRYABLE",
    ] {
        let pool = fresh_pool().await;
        let shift = seed_open_shift(&pool).await;
        seed_receipt(&pool, shift, 1, "ACK").await;
        seed_receipt(&pool, shift, 2, blocking_state).await;

        let outcome = quiesce(&pool, shift).await;
        assert!(
            matches!(outcome, QuiescenceOutcome::Pending { .. }),
            "{blocking_state} must block aggregation, got {outcome:?}"
        );
    }
}

#[tokio::test]
async fn rejected_and_cancelled_do_not_block() {
    let pool = fresh_pool().await;
    let shift = seed_open_shift(&pool).await;
    seed_receipt(&pool, shift, 1, "ACK").await;
    seed_receipt(&pool, shift, 2, "REJECTED").await;
    seed_receipt(&pool, shift, 3, "CANCELLED").await;

    assert_eq!(quiesce(&pool, shift).await, QuiescenceOutcome::Clear);
    // Only the issued ACK is aggregated; REJECTED/CANCELLED are excluded.
    assert_eq!(issued_count(&pool, shift).await, 1);
}

/// C10 (design v3 §9.3 ruling): a POST-SENT online doc that escalated to
/// REQUIRES_MANUAL_RECONCILIATION is ISSUED-BUT-UNCONFIRMED (lnd consumed,
/// server_fiscal_no stamped, seed advanced at SEND) — DPS may still be holding
/// it, so Z-quiescence MUST BLOCK on it (→ Pending) rather than close the shift
/// over it. RMR is NOT in the coarse in-flight set, so pre-C10 this doc silently
/// did NOT block; the new disjunct (offline_fiscal_no NULL AND server_fiscal_no
/// set) admits exactly this cohort.
#[tokio::test]
async fn post_sent_online_rmr_blocks_quiescence() {
    let pool = fresh_pool().await;
    let shift = seed_open_shift(&pool).await;
    seed_receipt(&pool, shift, 1, "ACK").await;
    // online-origin (offline_fiscal_no NULL), issued-unconfirmed (sfn set), RMR.
    seed_receipt_d3(
        &pool,
        shift,
        2,
        "REQUIRES_MANUAL_RECONCILIATION",
        Some("70000002"),
        None,
    )
    .await;

    let outcome = quiesce(&pool, shift).await;
    assert!(
        matches!(outcome, QuiescenceOutcome::Pending { .. }),
        "post-SENT online-issued-unconfirmed RMR must block Z-quiescence, got {outcome:?}"
    );
}

/// C10 NARROWNESS (paired control): the disjunct is guarded by
/// `offline_fiscal_no IS NULL` — an OFFLINE-origin RMR (offline_fiscal_no set)
/// is NOT online-issued-unconfirmed and must NOT block Z-quiescence via this
/// disjunct (its manual-recon is owned by the drain path, not the Z boundary).
/// Proves the C10 widening did not over-block all RMR.
#[tokio::test]
async fn offline_origin_rmr_does_not_block_quiescence() {
    let pool = fresh_pool().await;
    let shift = seed_open_shift(&pool).await;
    seed_receipt(&pool, shift, 1, "ACK").await;
    // offline-origin (offline_fiscal_no set) RMR → excluded by the disjunct guard.
    seed_receipt_d3(
        &pool,
        shift,
        2,
        "REQUIRES_MANUAL_RECONCILIATION",
        Some("70000003"),
        Some(9001),
    )
    .await;

    assert_eq!(
        quiesce(&pool, shift).await,
        QuiescenceOutcome::Clear,
        "offline-origin RMR must NOT block via the online-issued-unconfirmed disjunct"
    );
}

#[tokio::test]
async fn offline_local_ack_is_non_blocking_and_counted() {
    let pool = fresh_pool().await;
    let shift = seed_open_shift(&pool).await;
    seed_receipt(&pool, shift, 1, "OFFLINE_LOCAL_ACK").await;

    assert_eq!(quiesce(&pool, shift).await, QuiescenceOutcome::Clear);
    assert_eq!(issued_count(&pool, shift).await, 1);
}

/// Seed a FINALIZABLE KVT2 SELL doc + the node_state chain seed + the inbox
/// row stage_finalize::run requires (unsigned_xml_sha256 present;
/// node_state.last_known == this doc's previous_hash; a PROCESSING inbox row
/// for mark_done).
async fn seed_kvt2_finalizable(pool: &sqlx::SqlitePool, shift_id: ShiftId, lnd: i64) -> DocumentId {
    let prev = [0xBBu8; 32];
    let seed = [0xAAu8; 32];
    // node_state chain seed must equal this doc's previous_hash.
    sqlx::query("UPDATE node_state SET last_known_unsigned_xml_sha256 = ? WHERE fiscal_number = ?")
        .bind(&prev[..])
        .bind(FN)
        .execute(pool)
        .await
        .unwrap();
    let doc_id = DocumentId::new();
    let req = vec![0x9Au8; 16];
    sqlx::query(
        "INSERT INTO fiscal_documents( \
            document_id, request_id, fiscal_number, shift_id, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, unsigned_xml_sha256, previous_hash \
         ) VALUES (?, ?, ?, ?, ?, 'SELL', 'KVT2', 'b', 't', 'ONLINE', \
            '2026-06-08T00:00:00Z', '{}', ?, ?, ?)",
    )
    .bind(doc_id)
    .bind(&req)
    .bind(FN)
    .bind(shift_id)
    .bind(lnd)
    .bind(&[0u8; 32][..])
    .bind(&seed[..])
    .bind(&prev[..])
    .execute(pool)
    .await
    .unwrap();
    // stage_finalize marks the inbox row DONE — it must exist in PROCESSING.
    sqlx::query(
        "INSERT INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical, status) \
         VALUES (?, ?, 'REST', 'SELL', 'idem-kvt2', '{}', ?, 'PROCESSING')",
    )
    .bind(&req)
    .bind(FN)
    .bind(&[0u8; 32][..])
    .execute(pool)
    .await
    .unwrap();
    doc_id
}

#[tokio::test]
async fn kvt2_is_finalized_inline_then_clear_and_counted() {
    // The CORE quiescence path: KVT2 is the ONLY inline-finalizable state.
    // After quiescence the doc is Ack (issued) and counts in the aggregate.
    let pool = fresh_pool().await;
    let shift = seed_open_shift(&pool).await;
    let kvt2_doc = seed_kvt2_finalizable(&pool, shift, 1).await;

    // Before: not issued (KVT2 is in-flight).
    assert_eq!(issued_count(&pool, shift).await, 0);

    assert_eq!(quiesce(&pool, shift).await, QuiescenceOutcome::Clear);

    // The KVT2 doc was finalized to Ack (issued) and now counts.
    let state: String =
        sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
            .bind(kvt2_doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, "ACK", "KVT2 must be finalized to ACK by quiescence");
    assert_eq!(issued_count(&pool, shift).await, 1);
}

#[tokio::test]
async fn kvt2_finalized_but_other_pending_blocks() {
    // KVT2 finalizes, but a co-resident SENT receipt still blocks — quiescence
    // must report Pending (the Z is not created until SENT reaches issued).
    let pool = fresh_pool().await;
    let shift = seed_open_shift(&pool).await;
    let kvt2_doc = seed_kvt2_finalizable(&pool, shift, 1).await;
    seed_receipt(&pool, shift, 2, "SENT").await;

    let outcome = quiesce(&pool, shift).await;
    assert!(
        matches!(outcome, QuiescenceOutcome::Pending { .. }),
        "a co-resident SENT receipt must block, got {outcome:?}"
    );
    // The KVT2 was finalized inline (it leads the pending set; the SENT comes
    // AFTER it in lnd order, so the chain isn't blocked ahead of the KVT2).
    let state: String =
        sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
            .bind(kvt2_doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, "ACK");
}

#[tokio::test]
async fn kvt2_behind_an_earlier_blocker_is_not_finalized() {
    // review MEDIUM: a KVT2 sitting BEHIND an earlier non-finalizable receipt
    // (SIGNED at a lower lnd) must NOT be force-finalized — its MAC chain is
    // blocked behind the SIGNED doc, so stage_finalize would chain-seed-mismatch
    // and turn a recoverable "Z not ready" into a hard error. Quiescence must
    // stop at the first non-KVT2 and report Pending, leaving the KVT2 as KVT2.
    let pool = fresh_pool().await;
    let shift = seed_open_shift(&pool).await;
    seed_receipt(&pool, shift, 1, "SIGNED").await; // earlier blocker
    let kvt2_doc = seed_kvt2_finalizable(&pool, shift, 2).await; // behind it

    let outcome = quiesce(&pool, shift).await;
    assert!(
        matches!(outcome, QuiescenceOutcome::Pending { .. }),
        "an earlier SIGNED blocker must yield Pending, got {outcome:?}"
    );
    // The KVT2 was NOT finalized (it sits behind the SIGNED doc in the chain).
    let state: String =
        sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
            .bind(kvt2_doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        state, "KVT2",
        "a KVT2 behind an earlier blocker must stay KVT2, not be force-finalized"
    );
}
