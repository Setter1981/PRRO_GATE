//! Migration 035 (`delivery_reservation` call-once + durable evidence union) — RED-first battery.
//!
//! **RED-first discipline:** these tests were authored BEFORE migration 035 existed.
//! Without 035: the evidence columns, `ux_delivery_document_ever_started`,
//! the §3.1 rebuilt fence, and the matrix/immutability triggers are absent.
//! Several tests are RED without 035 (the intended-rejection inserts succeed
//! or the column reference errors).  After 035 is applied, all tests are GREEN.
//!
//! ## Test groups
//!
//! ### `m1_*` — SQL matrix tightness (§4.2 fail-closed evidence matrix)
//! For representative leaves, assert:
//! - a correctly-formed row is accepted;
//! - a row missing a required payload is rejected;
//! - a row carrying a forbidden payload is rejected;
//! - a row with swapped routing or node-effect is rejected;
//! - an explicit NULL-bypass case (a NULL where the matrix requires a value
//!   must be rejected, proving the `COALESCE(CASE…)` form closes the bypass).
//!
//! Leaves covered: `Accepted`, `Rejected{Verify → TerminalReject/NoNodeEffect}`,
//! `Rejected{Offline168 → TerminalReject/NodeBlocked}`, `Rejected{BadHashPrev}`,
//! `Rejected{NotPrevZReport}`, `Rejected{NotRegisteredRro}`, `NoResponse`,
//! `UnknownStatus`, `SaveError`, `CloseAmbiguous`, `MissingStatus`,
//! `OkButNoFiscalNumber`, `PreconditionFailed`, `RemoteAuthStatus`.
//!
//! ### `m2_*` — INACTIVE-safety (§1 fail-fast guard)
//! - Empty post-034 DB migrates 035 cleanly.
//! - DB with one `delivery_reservation` row aborts 035 transactionally
//!   (new columns/indexes/triggers absent afterwards).
//!
//! The guard is tested via the equivalent raw SQL (replicating what the
//! migration does) on a single acquired connection (TEMP tables are
//! connection-scoped).
//!
//! ### `m3_*` — Fence consistency (§3.1 predicate)
//! - `ux_reservation_active` and `delivery_reservation_no_replace` both use
//!   the §3.1 predicate (structural/string compare).
//! - The PENDING fence holds a fenced FN; APPLIED releases it.
//! - The §3.1 predicate is present in the trigger body (content check).
//!
//! ### `m4_*` — Evidence immutability (§7 trigger)
//! - Mutation of any evidence column after OUTCOME_OBSERVED is rejected.
//!
//! ### `m5_*` — Call-once index (`ux_delivery_document_ever_started`)
//! - After a reservation for doc A crosses CALL_STARTED, inserting a new RN
//!   for doc A is rejected.
//! - A doc B (different document_id) can still be reserved.
//!
//! ### `m6_*` — apply_state-based fence (§3.1 predicate replaces old cert/routing fence)
//! - A PENDING_APPLY row holds the FN fence; APPLIED releases it.
//! - Under the old 032/033 predicate, an APPLIED row with routing_class set
//!   would still hold the fence.  The new §3.1 predicate releases it.
//!
//! ### `rg_*` — Regression (034 + 033 triggers preserved; column count correct)

use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{self, NewReservation};
use prro::db::tx::with_immediate;
use sqlx::SqlitePool;
use std::collections::HashSet;

// ─────────────────────────────────────── fixtures ───────────────────────────

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations including 035");
    (dir, pool)
}

const FN_A: &str = "1234567890";
const FN_B: &str = "9876543210";

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

async fn seed_node_state(pool: &SqlitePool, fscl: &str) {
    seed_fn(pool, fscl).await;
    sqlx::query(
        "INSERT OR IGNORE INTO node_state \
             (fiscal_number, mode, shift_state, next_lnd) \
         VALUES (?, 'ONLINE', 'CREATED', 1)",
    )
    .bind(fscl)
    .execute(pool)
    .await
    .expect("seed node_state");
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
            '2026-07-19T12:34:56Z', '{}', ?)",
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

/// Advance to CALL_STARTED with both paired fields (034-conformant).
async fn advance_to_cs(pool: &SqlitePool, res_byte: u8) {
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'CALL_STARTED', \
             call_started_at = '2026-07-19T00:00:00Z', \
             authorized_generation = 1 \
         WHERE reservation_id = ?",
    )
    .bind(&[res_byte; 16][..])
    .execute(pool)
    .await
    .expect("RN→CS transition must be legal");
}

/// Write an Accepted OO row (the canonical happy-path).
async fn advance_to_oo_accepted(pool: &SqlitePool, res_byte: u8, fiscal_number_f: &str) {
    advance_to_cs(pool, res_byte).await;
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NoNodeEffect', \
             evidence_kind = 'Accepted', \
             evidence_text = ?, \
             remote_correlation_id = ? \
         WHERE reservation_id = ?",
    )
    .bind(fiscal_number_f)
    .bind(fiscal_number_f)
    .bind(&[res_byte; 16][..])
    .execute(pool)
    .await
    .expect("Accepted OO row must be accepted by matrix trigger");
}

/// Mark a PENDING_APPLY row as APPLIED (fence release).
async fn mark_applied(pool: &SqlitePool, res_byte: u8) {
    sqlx::query("UPDATE delivery_reservation SET apply_state = 'APPLIED' WHERE reservation_id = ?")
        .bind(&[res_byte; 16][..])
        .execute(pool)
        .await
        .expect("PENDING_APPLY → APPLIED must be legal");
}

fn err_has(err: &sqlx::Error, needle: &str) -> bool {
    err.to_string()
        .to_lowercase()
        .contains(&needle.to_lowercase())
}

// ══════════════════════ m1_* — SQL matrix tightness ══════════════════════

// ── m1_01: Accepted — correct row accepted ──
#[tokio::test]
async fn m1_01_accepted_correct_row_accepted() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    // Should accept a correctly-formed Accepted row.
    advance_to_oo_accepted(&pool, 0x01, "F1234567890").await;
    let kind: Option<String> = sqlx::query_scalar(
        "SELECT evidence_kind FROM delivery_reservation WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(kind.as_deref(), Some("Accepted"));
}

// ── m1_02: Accepted — empty evidence_text (empty fiscal number) rejected ──
#[tokio::test]
async fn m1_02_accepted_empty_fiscal_number_rejected() {
    // Without 035: the matrix trigger does not exist → this INSERT succeeds (RED).
    // With 035: the trigger fires because evidence_text is empty (length 0 fails
    //           `length(evidence_text) > 0` check).
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NoNodeEffect', \
             evidence_kind = 'Accepted', \
             evidence_text = '', \
             remote_correlation_id = '' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("Accepted with empty evidence_text must be rejected (NULL-bypass / empty-F check)");
    assert!(
        err_has(&err, "matrix")
            || err_has(&err, "leaf")
            || err_has(&err, "fail-closed")
            || err_has(&err, "constraint")
            || err_has(&err, "abort"),
        "expected matrix/leaf/fail-closed/constraint/abort error, got: {err}"
    );
}

// ── m1_03: Accepted — NULL evidence_text (NULL-bypass test) ──
#[tokio::test]
async fn m1_03_accepted_null_evidence_text_rejected() {
    // This is the explicit NULL-bypass case: evidence_text=NULL, evidence_kind='Accepted'.
    // Without 035: the matrix trigger absent → succeeds (RED).
    // With 035: COALESCE(CASE…ELSE 0 END, 0) <> 1 catches it because `evidence_text IS NOT NULL`
    //           evaluates to FALSE when evidence_text is NULL → CASE returns 0 → trigger fires.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NoNodeEffect', \
             evidence_kind = 'Accepted' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("Accepted with NULL evidence_text must be rejected (NULL-bypass proof)");
    assert!(
        err_has(&err, "matrix")
            || err_has(&err, "leaf")
            || err_has(&err, "fail-closed")
            || err_has(&err, "constraint")
            || err_has(&err, "abort"),
        "expected matrix/leaf/fail-closed/constraint/abort error, got: {err}"
    );
}

// ── m1_04: Accepted — swapped routing (routing_class='TerminalReject' instead of NULL) rejected ──
#[tokio::test]
async fn m1_04_accepted_with_routing_class_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             routing_class = 'TerminalReject', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NoNodeEffect', \
             evidence_kind = 'Accepted', \
             evidence_text = 'F9999', \
             remote_correlation_id = 'F9999' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("Accepted with routing_class set (swapped routing) must be rejected");
    assert!(
        err_has(&err, "matrix")
            || err_has(&err, "leaf")
            || err_has(&err, "fail-closed")
            || err_has(&err, "constraint")
            || err_has(&err, "abort"),
        "expected matrix/leaf/fail-closed/constraint/abort error, got: {err}"
    );
}

// ── m1_05: Accepted — mismatched remote_correlation_id rejected ──
#[tokio::test]
async fn m1_05_accepted_rcid_mismatch_rejected() {
    // For Accepted, remote_correlation_id must equal evidence_text (same fiscal number).
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NoNodeEffect', \
             evidence_kind = 'Accepted', \
             evidence_text = 'F_CORRECT', \
             remote_correlation_id = 'F_DIFFERENT' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("Accepted with rcid != evidence_text must be rejected");
    assert!(
        err_has(&err, "matrix")
            || err_has(&err, "leaf")
            || err_has(&err, "fail-closed")
            || err_has(&err, "constraint")
            || err_has(&err, "abort"),
        "expected matrix/leaf/fail-closed/constraint/abort error, got: {err}"
    );
}

// ── m1_06: Rejected{Verify} — correct row accepted ──
#[tokio::test]
async fn m1_06_rejected_verify_correct_row_accepted() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             routing_class = 'TerminalReject', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NoNodeEffect', \
             evidence_kind = 'Rejected', \
             evidence_text = 'Verify', \
             evidence_digest = ? \
         WHERE reservation_id = ?",
    )
    .bind(&[0xCDu8; 32][..])
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("Rejected{Verify → TerminalReject/NoNodeEffect} must be accepted");
}

// ── m1_07: Rejected{Verify} — missing digest rejected ──
#[tokio::test]
async fn m1_07_rejected_verify_missing_digest_rejected() {
    // Without 035: no matrix trigger → succeeds (RED).
    // With 035: evidence_digest IS NULL fails `length(evidence_digest) = 32`.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             routing_class = 'TerminalReject', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NoNodeEffect', \
             evidence_kind = 'Rejected', \
             evidence_text = 'Verify' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("Rejected{Verify} with NULL digest must be rejected (required payload missing)");
    assert!(
        err_has(&err, "matrix")
            || err_has(&err, "leaf")
            || err_has(&err, "fail-closed")
            || err_has(&err, "constraint")
            || err_has(&err, "abort"),
        "expected matrix/leaf/fail-closed/constraint/abort error, got: {err}"
    );
}

// ── m1_08: Rejected{Verify} — wrong digest length rejected ──
#[tokio::test]
async fn m1_08_rejected_verify_short_digest_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             routing_class = 'TerminalReject', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NoNodeEffect', \
             evidence_kind = 'Rejected', \
             evidence_text = 'Verify', \
             evidence_digest = ? \
         WHERE reservation_id = ?",
    )
    .bind(&[0xCDu8; 16][..]) // 16 bytes, not 32
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("Rejected{Verify} with 16-byte digest must be rejected (requires 32B)");
    // Both the column-level CHECK and the matrix trigger guard digest length.
    assert!(
        err_has(&err, "matrix")
            || err_has(&err, "leaf")
            || err_has(&err, "fail-closed")
            || err_has(&err, "constraint")
            || err_has(&err, "check")
            || err_has(&err, "abort"),
        "expected matrix/leaf/fail-closed/constraint/check/abort error, got: {err}"
    );
}

// ── m1_09: Rejected{Offline168} — correct row (TerminalReject/NodeBlocked) accepted ──
#[tokio::test]
async fn m1_09_rejected_offline168_correct_row_accepted() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             routing_class = 'TerminalReject', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NodeBlocked', \
             evidence_kind = 'Rejected', \
             evidence_text = 'Offline168', \
             evidence_digest = ? \
         WHERE reservation_id = ?",
    )
    .bind(&[0xCDu8; 32][..])
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("Rejected{Offline168 → TerminalReject/NodeBlocked} must be accepted");
}

// ── m1_10: Rejected{Offline168} — wrong node_effect (NoNodeEffect instead of NodeBlocked) rejected ──
#[tokio::test]
async fn m1_10_rejected_offline168_wrong_node_effect_rejected() {
    // This is the revert-canary test: removing the NodeBlocked requirement from the
    // matrix trigger's Offline168 branch would allow this to succeed.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             routing_class = 'TerminalReject', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NoNodeEffect', \
             evidence_kind = 'Rejected', \
             evidence_text = 'Offline168', \
             evidence_digest = ? \
         WHERE reservation_id = ?",
    )
    .bind(&[0xCDu8; 32][..])
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err(
        "Rejected{Offline168} with node_effect=NoNodeEffect (should be NodeBlocked) must be rejected \
         [REVERT-CANARY: removing NodeBlocked requirement from Offline168 branch makes this RED]"
    );
    assert!(
        err_has(&err, "matrix")
            || err_has(&err, "leaf")
            || err_has(&err, "fail-closed")
            || err_has(&err, "constraint")
            || err_has(&err, "abort"),
        "expected matrix/leaf/fail-closed/constraint/abort error, got: {err}"
    );
}

// ── m1_11: Rejected{BadHashPrev} — correct row (MacRecovery/MacReseedPending) accepted ──
#[tokio::test]
async fn m1_11_rejected_badhashprev_correct_row_accepted() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             routing_class = 'MacRecovery', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'MacReseedPending', \
             evidence_kind = 'Rejected', \
             evidence_text = 'BadHashPrev', \
             evidence_digest = ? \
         WHERE reservation_id = ?",
    )
    .bind(&[0xCDu8; 32][..])
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("Rejected{BadHashPrev → MacRecovery/MacReseedPending} must be accepted");
}

// ── m1_12: Rejected{NotPrevZReport} — correct row (OperatorEscalation×2) accepted ──
#[tokio::test]
async fn m1_12_rejected_notprevzreport_correct_row_accepted() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             routing_class = 'OperatorEscalation', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'OperatorEscalation', \
             evidence_kind = 'Rejected', \
             evidence_text = 'NotPrevZReport', \
             evidence_digest = ? \
         WHERE reservation_id = ?",
    )
    .bind(&[0xCDu8; 32][..])
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("Rejected{NotPrevZReport → OperatorEscalation/OperatorEscalation} must be accepted");
}

// ── m1_13: Rejected{NotRegisteredRro} — correct row (FnConfigError×2) accepted ──
#[tokio::test]
async fn m1_13_rejected_notregisteredrro_correct_row_accepted() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             routing_class = 'FnConfigError', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'FnConfigError', \
             evidence_kind = 'Rejected', \
             evidence_text = 'NotRegisteredRro', \
             evidence_digest = ? \
         WHERE reservation_id = ?",
    )
    .bind(&[0xCDu8; 32][..])
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("Rejected{NotRegisteredRro → FnConfigError/FnConfigError} must be accepted");
}

// ── m1_14: NoResponse — correct row accepted ──
#[tokio::test]
async fn m1_14_noresponse_correct_row_accepted() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED_UNKNOWN', \
             response_provenance = 'NO_RESPONSE', \
             routing_class = 'TransientRetry', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NoNodeEffect', \
             evidence_kind = 'NoResponse', \
             evidence_text = 'Timeout' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("NoResponse{Timeout} must be accepted");
}

// ── m1_15: NoResponse — NULL evidence_text (NULL-bypass case) rejected ──
#[tokio::test]
async fn m1_15_noresponse_null_cause_rejected() {
    // This is the key NULL-bypass test: evidence_text=NULL on NoResponse.
    // Without 035: no matrix trigger → succeeds (RED; proves `WHEN NOT(pred)` form
    //               would be bypassed if we had used it).
    // With 035: COALESCE(CASE…) form closes the bypass → rejected.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED_UNKNOWN', \
             response_provenance = 'NO_RESPONSE', \
             routing_class = 'TransientRetry', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NoNodeEffect', \
             evidence_kind = 'NoResponse' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err(
        "NoResponse with NULL evidence_text must be rejected (NULL-bypass proof via COALESCE)",
    );
    assert!(
        err_has(&err, "matrix")
            || err_has(&err, "leaf")
            || err_has(&err, "fail-closed")
            || err_has(&err, "constraint")
            || err_has(&err, "abort"),
        "expected matrix/leaf/fail-closed/constraint/abort error, got: {err}"
    );
}

// ── m1_16: UnknownStatus — correct row (code + digest) accepted ──
#[tokio::test]
async fn m1_16_unknownstatus_correct_row_accepted() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED_UNKNOWN', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             routing_class = 'TransientRetry', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NoNodeEffect', \
             evidence_kind = 'UnknownStatus', \
             evidence_code = -999, \
             evidence_digest = ? \
         WHERE reservation_id = ?",
    )
    .bind(&[0xCDu8; 32][..])
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("UnknownStatus{code=-999, digest 32B} must be accepted");
}

// ── m1_17: UnknownStatus — missing code rejected ──
#[tokio::test]
async fn m1_17_unknownstatus_missing_code_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED_UNKNOWN', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             routing_class = 'TransientRetry', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NoNodeEffect', \
             evidence_kind = 'UnknownStatus', \
             evidence_digest = ? \
         WHERE reservation_id = ?",
    )
    .bind(&[0xCDu8; 32][..])
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("UnknownStatus with NULL evidence_code must be rejected");
    assert!(
        err_has(&err, "matrix")
            || err_has(&err, "leaf")
            || err_has(&err, "fail-closed")
            || err_has(&err, "constraint")
            || err_has(&err, "abort"),
        "expected matrix/leaf/fail-closed/constraint/abort error, got: {err}"
    );
}

// ── m1_18: SaveError — correct row (digest only) accepted ──
#[tokio::test]
async fn m1_18_saveerror_correct_row_accepted() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED_UNKNOWN', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             routing_class = 'TransientRetry', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NoNodeEffect', \
             evidence_kind = 'SaveError', \
             evidence_digest = ? \
         WHERE reservation_id = ?",
    )
    .bind(&[0xCDu8; 32][..])
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("SaveError{digest 32B} must be accepted");
}

// ── m1_19: NoResponse — forbidden payload (digest set) rejected ──
#[tokio::test]
async fn m1_19_noresponse_forbidden_digest_rejected() {
    // NoResponse requires evidence_digest IS NULL.  Setting a digest is a forbidden payload.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED_UNKNOWN', \
             response_provenance = 'NO_RESPONSE', \
             routing_class = 'TransientRetry', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NoNodeEffect', \
             evidence_kind = 'NoResponse', \
             evidence_text = 'Timeout', \
             evidence_digest = ? \
         WHERE reservation_id = ?",
    )
    .bind(&[0xCDu8; 32][..])
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("NoResponse with evidence_digest set (forbidden payload) must be rejected");
    assert!(
        err_has(&err, "matrix")
            || err_has(&err, "leaf")
            || err_has(&err, "fail-closed")
            || err_has(&err, "constraint")
            || err_has(&err, "abort"),
        "expected matrix/leaf/fail-closed/constraint/abort error, got: {err}"
    );
}

// ── m1_20: PreconditionFailed — correct row (all NULL payload) accepted ──
#[tokio::test]
async fn m1_20_preconditionfailed_all_null_payload_accepted() {
    // PreconditionFailed: NOT_SUBMITTED / NO_RESPONSE / TransientRetry / NoNodeEffect / all NULL.
    // This leaf can be set on an RNS→OO direct transition (not via CALL_STARTED).
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();

    // Transition directly RNS→OO with NOT_SUBMITTED (legal per 033 transition trigger).
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'NOT_SUBMITTED', \
             response_provenance = 'NO_RESPONSE', \
             routing_class = 'TransientRetry', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NoNodeEffect', \
             evidence_kind = 'PreconditionFailed' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("PreconditionFailed with all-NULL payload must be accepted");
}

// ── m1_21: RemoteAuthStatus — correct row (digest 32B) accepted ──
#[tokio::test]
async fn m1_21_remoteauthstatus_correct_row_accepted() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED_UNKNOWN', \
             response_provenance = 'AUTHENTICATED_PEER', \
             routing_class = 'ProbeRequired', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'ProbeRequired', \
             evidence_kind = 'RemoteAuthStatus', \
             evidence_digest = ? \
         WHERE reservation_id = ?",
    )
    .bind(&[0xCDu8; 32][..])
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("RemoteAuthStatus{digest 32B} must be accepted");
}

// ── m1_22: CloseAmbiguous — correct row accepted ──
#[tokio::test]
async fn m1_22_closeambiguous_correct_row_accepted() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED_UNKNOWN', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             routing_class = 'ProbeRequired', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'ProbeRequired', \
             evidence_kind = 'CloseAmbiguous', \
             evidence_digest = ? \
         WHERE reservation_id = ?",
    )
    .bind(&[0xCDu8; 32][..])
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("CloseAmbiguous{digest 32B} must be accepted");
}

// ── m1_23: MissingStatus — correct row accepted ──
#[tokio::test]
async fn m1_23_missingstatus_correct_row_accepted() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED_UNKNOWN', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             routing_class = 'ProbeRequired', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'ProbeRequired', \
             evidence_kind = 'MissingStatus', \
             evidence_digest = ? \
         WHERE reservation_id = ?",
    )
    .bind(&[0xCDu8; 32][..])
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("MissingStatus{digest 32B} must be accepted");
}

// ── m1_24: OkButNoFiscalNumber — correct row accepted ──
#[tokio::test]
async fn m1_24_okbutnofiscalnumber_correct_row_accepted() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED_UNKNOWN', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             routing_class = 'ProbeRequired', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'ProbeRequired', \
             evidence_kind = 'OkButNoFiscalNumber', \
             evidence_digest = ? \
         WHERE reservation_id = ?",
    )
    .bind(&[0xCDu8; 32][..])
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("OkButNoFiscalNumber{digest 32B} must be accepted");
}

// ── m1_25: NULL evidence_kind (no leaf) at OUTCOME_OBSERVED rejected ──
#[tokio::test]
async fn m1_25_null_evidence_kind_at_oo_rejected() {
    // NULL evidence_kind at OO means no leaf matches → COALESCE returns 0 → trigger fires.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NoNodeEffect' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("OO with NULL evidence_kind must be rejected (no leaf matches, fail-closed)");
    // Note: the 034 OO-completeness trigger fires first for routing_class IS NULL + NoNodeEffect,
    // but the evidence matrix trigger also catches it.  Either error is valid here.
    assert!(
        err_has(&err, "matrix")
            || err_has(&err, "leaf")
            || err_has(&err, "fail-closed")
            || err_has(&err, "clean")
            || err_has(&err, "constraint")
            || err_has(&err, "abort"),
        "expected matrix/leaf/fail-closed/clean/constraint/abort error, got: {err}"
    );
}

// ══════════════════════ m2_* — INACTIVE-safety ══════════════════════

/// Tests the fail-fast guard mechanism via the equivalent raw SQL on a single
/// connection (TEMP tables are connection-scoped; the real migration runs on one
/// connection too).  We cannot replay migration 035 on a live populated table
/// from within a test (sqlx applies each migration once), so we replicate the
/// guard logic directly.
#[tokio::test]
async fn m2_01_guard_accepts_empty_table() {
    let (_d, pool) = fresh_pool().await;
    // delivery_reservation is empty after a fresh pool (no rows inserted).
    let mut conn = pool.acquire().await.expect("acquire connection");
    sqlx::query("CREATE TEMP TABLE _m2_01_guard (c INTEGER NOT NULL CHECK (c = 0))")
        .execute(&mut *conn)
        .await
        .expect("create temp guard table");
    sqlx::query("INSERT INTO _m2_01_guard (c) SELECT COUNT(*) FROM delivery_reservation")
        .execute(&mut *conn)
        .await
        .expect("guard must accept empty delivery_reservation");
    sqlx::query("DROP TABLE _m2_01_guard")
        .execute(&mut *conn)
        .await
        .ok();
}

#[tokio::test]
async fn m2_02_guard_rejects_nonempty_table() {
    // Without 035: the guard does not exist during migration → N/A (the migration
    // hasn't run yet).  This test replicates the guard SQL to verify the mechanism.
    // With 035: same mechanism is used.  This test catches a guard regression.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();

    let mut conn = pool.acquire().await.expect("acquire connection");
    sqlx::query("CREATE TEMP TABLE _m2_02_guard (c INTEGER NOT NULL CHECK (c = 0))")
        .execute(&mut *conn)
        .await
        .expect("create temp guard table");

    let err = sqlx::query("INSERT INTO _m2_02_guard (c) SELECT COUNT(*) FROM delivery_reservation")
        .execute(&mut *conn)
        .await
        .expect_err("guard must reject non-empty delivery_reservation");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "expected CHECK/constraint from guard, got: {err}"
    );

    sqlx::query("DROP TABLE _m2_02_guard")
        .execute(&mut *conn)
        .await
        .ok();
}

/// Verify that after 035 is applied, the four evidence columns exist.
#[tokio::test]
async fn m2_03_evidence_columns_exist() {
    let (_d, pool) = fresh_pool().await;
    let cols: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(delivery_reservation)")
            .fetch_all(&pool)
            .await
            .unwrap();
    let names: HashSet<String> = cols.iter().map(|c| c.1.clone()).collect();

    for col in [
        "evidence_kind",
        "evidence_text",
        "evidence_code",
        "evidence_digest",
    ] {
        assert!(
            names.contains(col),
            "035 column {col} must exist in delivery_reservation; have {names:?}"
        );
    }
    // Column count: 033 had 20 columns; 035 adds 4 → 24.
    assert_eq!(
        cols.len(),
        24,
        "delivery_reservation must have 24 columns after 035; got {}",
        cols.len()
    );
}

// ══════════════════════ m3_* — Fence consistency ══════════════════════

/// The §3.1 predicate must appear in `delivery_reservation_no_replace` trigger body.
#[tokio::test]
async fn m3_01_no_replace_trigger_contains_section_31_predicate() {
    let (_d, pool) = fresh_pool().await;
    let sql: Option<String> = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type='trigger' \
         AND name='delivery_reservation_no_replace'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    let sql = sql.expect("delivery_reservation_no_replace trigger must exist");

    // The §3.1 predicate (core part) must appear in the trigger body.
    assert!(
        sql.contains("state IN ('RESERVED_NOT_STARTED','CALL_STARTED')")
            || sql.contains("RESERVED_NOT_STARTED") && sql.contains("CALL_STARTED"),
        "no_replace trigger must contain RESERVED_NOT_STARTED/CALL_STARTED states: {sql}"
    );
    assert!(
        sql.contains("PENDING_APPLY"),
        "no_replace trigger must contain PENDING_APPLY (§3.1 predicate): {sql}"
    );
}

/// The §3.1 predicate must appear in `ux_reservation_active` index definition.
#[tokio::test]
async fn m3_02_ux_reservation_active_contains_section_31_predicate() {
    let (_d, pool) = fresh_pool().await;
    let sql: Option<String> = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type='index' \
         AND name='ux_reservation_active'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    let sql = sql.expect("ux_reservation_active index must exist");

    assert!(
        sql.contains("PENDING_APPLY"),
        "ux_reservation_active must use PENDING_APPLY (§3.1 predicate): {sql}"
    );
}

/// PENDING_APPLY row holds the fence; APPLIED releases it.
/// Under the §3.1 predicate, a second FN-A reservation is rejected while
/// the first is PENDING_APPLY, but succeeds after it moves to APPLIED.
#[tokio::test]
async fn m3_03_pending_apply_holds_fence_applied_releases() {
    let (_d, pool) = fresh_pool().await;
    let doc_a = seed_doc(&pool, FN_A, 0x10, 1).await;
    let doc_b = seed_doc(&pool, FN_A, 0x11, 2).await;

    // Reservation for doc_a: advance to Accepted/PENDING_APPLY.
    insert_res(&pool, new_res(0x01, doc_a, FN_A)).await.unwrap();
    advance_to_oo_accepted(&pool, 0x01, "F_ACCEPTED").await;

    // At PENDING_APPLY: inserting a new reservation for the same FN must fail.
    let err = insert_res(&pool, new_res(0x02, doc_b, FN_A)).await;
    assert!(
        err.is_err(),
        "inserting a new reservation while FN-A is PENDING_APPLY must be rejected by the fence"
    );

    // Mark the first reservation APPLIED — releases the fence.
    mark_applied(&pool, 0x01).await;

    // Now a new reservation for doc_b on FN_A must succeed.
    insert_res(&pool, new_res(0x02, doc_b, FN_A))
        .await
        .expect("after APPLIED, a new reservation for FN_A doc_b must be accepted");
}

/// Under the OLD 032/033 fence predicate (submission_certainty + routing_class based),
/// a SUBMITTED/TerminalReject APPLIED row would NOT be released (routing_class NOT NULL
/// kept it fenced).  Under §3.1 (apply_state = 'PENDING_APPLY' only), an APPLIED row
/// releases regardless of routing_class.  This test verifies the new behaviour.
#[tokio::test]
async fn m3_04_applied_terminal_reject_releases_fence() {
    let (_d, pool) = fresh_pool().await;
    let doc_a = seed_doc(&pool, FN_A, 0x10, 1).await;
    let doc_b = seed_doc(&pool, FN_A, 0x11, 2).await;

    // Reservation for doc_a: advance to Rejected{Verify}/PENDING_APPLY.
    insert_res(&pool, new_res(0x01, doc_a, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             routing_class = 'TerminalReject', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NoNodeEffect', \
             evidence_kind = 'Rejected', \
             evidence_text = 'Verify', \
             evidence_digest = ? \
         WHERE reservation_id = ?",
    )
    .bind(&[0xCDu8; 32][..])
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("Rejected{Verify} PENDING_APPLY must be accepted");

    // Still PENDING_APPLY: fence held.
    let err = insert_res(&pool, new_res(0x02, doc_b, FN_A)).await;
    assert!(
        err.is_err(),
        "PENDING_APPLY TerminalReject must hold the fence"
    );

    // Mark APPLIED.
    mark_applied(&pool, 0x01).await;

    // Under §3.1: APPLIED releases even when routing_class = 'TerminalReject'.
    // (Under old 032/033 fence this would still be held — the §3.1 change.)
    insert_res(&pool, new_res(0x02, doc_b, FN_A))
        .await
        .expect("after APPLIED, TerminalReject row must release the fence (§3.1)");
}

// ══════════════════════ m4_* — Evidence immutability ══════════════════════

/// After OUTCOME_OBSERVED, evidence_kind cannot be mutated.
#[tokio::test]
async fn m4_01_evidence_kind_immutable_after_oo() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_oo_accepted(&pool, 0x01, "F9999").await;

    let err = sqlx::query(
        "UPDATE delivery_reservation SET evidence_kind = 'Rejected' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("evidence_kind must be immutable after OUTCOME_OBSERVED");
    assert!(
        err_has(&err, "immutable")
            || err_has(&err, "evidence")
            || err_has(&err, "constraint")
            || err_has(&err, "abort"),
        "expected immutable/evidence/constraint/abort error, got: {err}"
    );
}

/// After OUTCOME_OBSERVED, evidence_text cannot be mutated.
#[tokio::test]
async fn m4_02_evidence_text_immutable_after_oo() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_oo_accepted(&pool, 0x01, "F9999").await;

    let err = sqlx::query(
        "UPDATE delivery_reservation SET evidence_text = 'TAMPERED', remote_correlation_id = 'TAMPERED' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("evidence_text must be immutable after OUTCOME_OBSERVED");
    assert!(
        err_has(&err, "immutable")
            || err_has(&err, "evidence")
            || err_has(&err, "constraint")
            || err_has(&err, "abort"),
        "expected immutable/evidence/constraint/abort error, got: {err}"
    );
}

/// After OUTCOME_OBSERVED, evidence_digest cannot be mutated.
#[tokio::test]
async fn m4_03_evidence_digest_immutable_after_oo() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;
    // Use Rejected{Verify} which has a digest.
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             routing_class = 'TerminalReject', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NoNodeEffect', \
             evidence_kind = 'Rejected', \
             evidence_text = 'Verify', \
             evidence_digest = ? \
         WHERE reservation_id = ?",
    )
    .bind(&[0xCDu8; 32][..])
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("Rejected{Verify} must be accepted");

    let err =
        sqlx::query("UPDATE delivery_reservation SET evidence_digest = ? WHERE reservation_id = ?")
            .bind(&[0xFFu8; 32][..])
            .bind(&[0x01u8; 16][..])
            .execute(&pool)
            .await
            .expect_err("evidence_digest must be immutable after OUTCOME_OBSERVED");
    assert!(
        err_has(&err, "immutable")
            || err_has(&err, "evidence")
            || err_has(&err, "constraint")
            || err_has(&err, "abort"),
        "expected immutable/evidence/constraint/abort error, got: {err}"
    );
}

/// Evidence columns are mutable (nullable) before OUTCOME_OBSERVED (RNS/CS state).
/// They should be NULL before OO — the evidence-kind column-level CHECK allows NULL at RNS/CS.
#[tokio::test]
async fn m4_04_evidence_null_before_oo_accepted() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();

    // At RNS: all four evidence columns must be NULL.
    let (ek, et, ec, ed): (Option<String>, Option<String>, Option<i64>, Option<Vec<u8>>) =
        sqlx::query_as(
            "SELECT evidence_kind, evidence_text, evidence_code, evidence_digest \
             FROM delivery_reservation WHERE reservation_id = ?",
        )
        .bind(&[0x01u8; 16][..])
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(ek.is_none(), "evidence_kind must be NULL at RNS");
    assert!(et.is_none(), "evidence_text must be NULL at RNS");
    assert!(ec.is_none(), "evidence_code must be NULL at RNS");
    assert!(ed.is_none(), "evidence_digest must be NULL at RNS");
}

// ══════════════════════ m5_* — Call-once index ══════════════════════

/// After doc A crosses CALL_STARTED, inserting a new RN for doc A is rejected.
#[tokio::test]
async fn m5_01_new_rn_after_call_started_rejected() {
    // Without 035: `ux_delivery_document_ever_started` does not exist →
    //   the INSERT of a second RN for the same document_id succeeds (RED).
    // With 035: the `delivery_reservation_no_replace` trigger (historical clause)
    //   rejects the INSERT.  The `ux_delivery_document_ever_started` index also
    //   prevents a concurrent INSERT-OR-REPLACE from slipping through.
    let (_d, pool) = fresh_pool().await;
    let doc_a = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc_a, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    // Mark the first reservation APPLIED (fence released for FN_A).
    advance_to_oo_accepted(&pool, 0x01, "F_ACCEPTED").await;
    mark_applied(&pool, 0x01).await;

    // Now try to insert a new RN for the same document_id = doc_a on FN_A.
    // The historical-document-started clause in delivery_reservation_no_replace must reject this.
    let doc_a2 = doc_a; // same document_id
    let err = insert_res(&pool, new_res(0x02, doc_a2, FN_A)).await;
    assert!(
        err.is_err(),
        "inserting a new RN for doc_a (which already crossed CALL_STARTED) must be rejected"
    );
    let err = err.unwrap_err();
    let s = err.to_string().to_lowercase();
    assert!(
        s.contains("ever-started")
            || s.contains("document")
            || s.contains("collision")
            || s.contains("constraint")
            || s.contains("abort"),
        "expected ever-started/document/collision/constraint/abort error, got: {err}"
    );
}

/// A different document B (different document_id) can still be reserved even after doc A started.
#[tokio::test]
async fn m5_02_different_document_can_still_reserve() {
    let (_d, pool) = fresh_pool().await;
    let doc_a = seed_doc(&pool, FN_A, 0x10, 1).await;
    let doc_b = seed_doc(&pool, FN_A, 0x11, 2).await;

    insert_res(&pool, new_res(0x01, doc_a, FN_A)).await.unwrap();
    advance_to_oo_accepted(&pool, 0x01, "F_ACCEPTED").await;
    mark_applied(&pool, 0x01).await;

    // doc_b has a different document_id — should be reservable.
    insert_res(&pool, new_res(0x02, doc_b, FN_A))
        .await
        .expect("doc_b (different document_id) must be reservable after doc_a APPLIED");
}

/// The `ux_delivery_document_ever_started` index must exist in sqlite_master.
#[tokio::test]
async fn m5_03_ux_delivery_document_ever_started_exists() {
    let (_d, pool) = fresh_pool().await;
    let exists: bool = sqlx::query_scalar(
        "SELECT COUNT(*) > 0 FROM sqlite_master \
         WHERE type='index' AND name='ux_delivery_document_ever_started'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        exists,
        "ux_delivery_document_ever_started index must exist after 035"
    );
}

// ══════════════════════ m6_* — apply_state-based fence ══════════════════════

/// Under §3.1, a RESERVED_NOT_STARTED row holds the fence.
#[tokio::test]
async fn m6_01_reserved_not_started_holds_fence() {
    let (_d, pool) = fresh_pool().await;
    let doc_a = seed_doc(&pool, FN_A, 0x10, 1).await;
    let doc_b = seed_doc(&pool, FN_A, 0x11, 2).await;

    insert_res(&pool, new_res(0x01, doc_a, FN_A)).await.unwrap();
    // At RNS: fence held.
    let err = insert_res(&pool, new_res(0x02, doc_b, FN_A)).await;
    assert!(err.is_err(), "RNS row must hold the fence for FN_A");
}

/// Under §3.1, a CALL_STARTED row holds the fence.
#[tokio::test]
async fn m6_02_call_started_holds_fence() {
    let (_d, pool) = fresh_pool().await;
    let doc_a = seed_doc(&pool, FN_A, 0x10, 1).await;
    let doc_b = seed_doc(&pool, FN_A, 0x11, 2).await;

    insert_res(&pool, new_res(0x01, doc_a, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;
    let err = insert_res(&pool, new_res(0x02, doc_b, FN_A)).await;
    assert!(
        err.is_err(),
        "CALL_STARTED row must hold the fence for FN_A"
    );
}

// ══════════════════════ rg_* — Regression (034 + 033 triggers preserved) ══════════════════════

/// All 034, 033, and 035 triggers and indexes must be present in sqlite_master.
#[tokio::test]
async fn rg01_all_expected_objects_in_sqlite_master() {
    let (_d, pool) = fresh_pool().await;
    let triggers: HashSet<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='trigger'")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .collect();
    let indexes: HashSet<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='index'")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .collect();

    // 035 triggers
    for tg in [
        "delivery_reservation_evidence_matrix_insert",
        "delivery_reservation_evidence_matrix_update",
        "delivery_reservation_evidence_immutable",
    ] {
        assert!(
            triggers.contains(tg),
            "035 trigger {tg} must exist; have {triggers:?}"
        );
    }

    // 035 index
    assert!(
        indexes.contains("ux_delivery_document_ever_started"),
        "035 index ux_delivery_document_ever_started must exist; have {indexes:?}"
    );

    // 034 triggers (must survive 035 rebuild)
    for tg in [
        "delivery_reservation_clean_accept_node_effect",
        "delivery_reservation_oo_completeness",
        "delivery_reservation_cs_pairing_update",
        "delivery_reservation_cs_pairing_insert",
        "node_state_delivery_generation_monotone",
    ] {
        assert!(
            triggers.contains(tg),
            "034 trigger {tg} must still exist after 035; have {triggers:?}"
        );
    }

    // 033 triggers (must survive 035 rebuild)
    for tg in [
        "delivery_reservation_insert_state",
        "delivery_reservation_no_replace",
        "delivery_reservation_transition",
        "delivery_reservation_immutable",
        "delivery_reservation_append_only",
        "delivery_reservation_updated_at",
        "delivery_reservation_apply_state_transition",
        "node_state_active_reservation_id_check",
        "node_state_delivery_generation_check",
    ] {
        assert!(
            triggers.contains(tg),
            "033 trigger {tg} must still exist after 035; have {triggers:?}"
        );
    }

    // Rebuilt indexes with §3.1 predicate
    for idx in ["ux_reservation_active", "ix_reservation_call_started"] {
        assert!(
            indexes.contains(idx),
            "index {idx} must exist after 035; have {indexes:?}"
        );
    }
}

/// 034 triggers still fire correctly after 035 rebuild.
#[tokio::test]
async fn rg02_034_triggers_still_bite_after_035() {
    // H1: monotone delivery_generation still enforced.
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN_A).await;
    sqlx::query("UPDATE node_state SET delivery_generation = 5 WHERE fiscal_number = ?")
        .bind(FN_A)
        .execute(&pool)
        .await
        .expect("delivery_generation = 5 must be accepted");
    let err = sqlx::query("UPDATE node_state SET delivery_generation = 3 WHERE fiscal_number = ?")
        .bind(FN_A)
        .execute(&pool)
        .await
        .expect_err(
            "delivery_generation decrease (5→3) must be ABORT — 034 still enforced after 035",
        );
    assert!(
        err_has(&err, "monoton")
            || err_has(&err, "delivery_generation")
            || err_has(&err, "constraint")
            || err_has(&err, "abort"),
        "expected monotone/delivery_generation/constraint/abort error, got: {err}"
    );
}

/// apply_state monotone still enforced (033 trigger) after 035 rebuild.
#[tokio::test]
async fn rg03_033_apply_state_monotone_still_enforced() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x10, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_oo_accepted(&pool, 0x01, "F9999").await;
    mark_applied(&pool, 0x01).await;

    // Attempt to roll back APPLIED → PENDING_APPLY: must be rejected.
    let err = sqlx::query(
        "UPDATE delivery_reservation SET apply_state = 'PENDING_APPLY' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("APPLIED → PENDING_APPLY rollback must be rejected (033 monotone trigger)");
    assert!(
        err_has(&err, "apply_state")
            || err_has(&err, "monoton")
            || err_has(&err, "illegal")
            || err_has(&err, "constraint")
            || err_has(&err, "abort"),
        "expected apply_state/monotone/illegal/constraint/abort error, got: {err}"
    );
}

/// FN_B reservation unaffected when FN_A is fenced.
#[tokio::test]
async fn rg04_fn_b_unaffected_when_fn_a_fenced() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, FN_B).await;
    let doc_a = seed_doc(&pool, FN_A, 0x10, 1).await;
    let doc_b = seed_doc(&pool, FN_B, 0x20, 1).await;

    insert_res(&pool, new_res(0x01, doc_a, FN_A)).await.unwrap();
    // FN_A is fenced (RNS). FN_B must still be reservable.
    insert_res(&pool, new_res(0x02, doc_b, FN_B))
        .await
        .expect("FN_B must be reservable while FN_A is fenced");
}
