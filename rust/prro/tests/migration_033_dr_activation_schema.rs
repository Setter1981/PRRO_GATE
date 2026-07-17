//! Migration 033 (`delivery_reservation` activation schema + `node_state`
//! generation columns) — RED-first regression battery (CS-3 Slice B).
//!
//! 033 is **INACTIVE** (behaviour-neutral): it extends 032's empty
//! `delivery_reservation` table with four new columns (`authorized_generation`,
//! `apply_state`, `node_effect`, plus the `node_state` columns
//! `delivery_generation` + `active_delivery_reservation_id`), wires
//! immutability / transition triggers for the new fields, and installs a
//! fail-fast activation guard that aborts if the table is non-empty.
//!
//! Groups:
//! - `ns_*`  — `node_state` new columns (existence, types, defaults, CHECKs).
//! - `ag_*`  — `delivery_reservation.authorized_generation` (existence, CHECK >= 1,
//!              NULL-at-insert, set-once immutable, frozen once set).
//! - `as_*`  — `delivery_reservation.apply_state` (existence, CHECK vocabulary,
//!              NULL-at-insert, legal NULL→PENDING_APPLY→APPLIED, illegal jumps,
//!              frozen once APPLIED).
//! - `ne_*`  — `delivery_reservation.node_effect` (existence, CHECK vocabulary,
//!              NULL-at-insert, set at OUTCOME_OBSERVED, frozen once set).
//! - `fg_*`  — fail-fast guard (non-empty table aborts migration).
//! - `rg_*`  — 032 regression (all existing triggers/indexes still fire; column
//!              count correct; static call-graph pin still holds).

use prro::db::repositories::delivery_reservation::{self, NewReservation};
use prro::db::models::ids::DocumentId;
use prro::db::tx::with_immediate;
use sqlx::SqlitePool;
use std::collections::HashSet;

// ─────────────────────────── fixtures ───────────────────────────

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations including 033");
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

/// Advance reservation to CALL_STARTED.
async fn advance_to_cs(pool: &SqlitePool, res_byte: u8) {
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'CALL_STARTED', call_started_at = '2026-07-17T00:00:00Z' \
         WHERE reservation_id = ?",
    )
    .bind(&[res_byte; 16][..])
    .execute(pool)
    .await
    .expect("RN→CS legal");
}

/// Advance to OUTCOME_OBSERVED (SUBMITTED, PARSED_DPS_ENVELOPE, routing NULL = clean accept).
async fn advance_to_oo_clean(pool: &SqlitePool, res_byte: u8) {
    advance_to_cs(pool, res_byte).await;
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE' \
         WHERE reservation_id = ?",
    )
    .bind(&[res_byte; 16][..])
    .execute(pool)
    .await
    .expect("CS→OO clean accept legal");
}

fn err_has(err: &sqlx::Error, needle: &str) -> bool {
    err.to_string().to_lowercase().contains(needle)
}

// ══════════════════════ ns_* — node_state new columns ══════════════════════

#[tokio::test]
async fn ns01_delivery_generation_column_exists_with_correct_default() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN_A).await;

    // delivery_generation must exist (DEFAULT 0) and read back as 0 after insert.
    let val: i64 = sqlx::query_scalar(
        "SELECT delivery_generation FROM node_state WHERE fiscal_number = ?",
    )
    .bind(FN_A)
    .fetch_one(&pool)
    .await
    .expect("delivery_generation column must exist on node_state after migration 033");

    assert_eq!(val, 0, "delivery_generation DEFAULT must be 0");
}

#[tokio::test]
async fn ns02_delivery_generation_pragma_type_and_not_null() {
    let (_d, pool) = fresh_pool().await;
    // PRAGMA table_info: cid, name, type, notnull, dflt_value, pk
    let cols: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(node_state)")
            .fetch_all(&pool)
            .await
            .unwrap();

    let dg = cols
        .iter()
        .find(|c| c.1 == "delivery_generation")
        .expect("delivery_generation column must exist in PRAGMA table_info(node_state)");

    // STRICT table: type must be INTEGER
    assert_eq!(dg.2.to_uppercase(), "INTEGER", "delivery_generation type must be INTEGER");
    // NOT NULL
    assert_eq!(dg.3, 1, "delivery_generation must be NOT NULL");
    // DEFAULT 0
    let dflt = dg.4.as_deref().unwrap_or("");
    assert_eq!(dflt, "0", "delivery_generation DEFAULT must be 0");
}

#[tokio::test]
async fn ns03_delivery_generation_check_rejects_negative() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, FN_A).await;
    // delivery_generation CHECK (>= 0, i.e. NOT NULL DEFAULT 0; direct INSERT lets us
    // test the raw schema constraint).  We directly insert a row with a negative value.
    let err = sqlx::query(
        "INSERT INTO node_state \
             (fiscal_number, mode, shift_state, next_lnd, delivery_generation) \
         VALUES (?, 'ONLINE', 'CREATED', 1, -1)",
    )
    .bind(FN_A)
    .execute(&pool)
    .await
    .expect_err("delivery_generation < 0 must violate CHECK");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "expected CHECK/constraint error, got: {err}"
    );
}

#[tokio::test]
async fn ns04_active_delivery_reservation_id_column_exists_nullable_blob() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN_A).await;

    // Column must exist and default to NULL.
    let val: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT active_delivery_reservation_id FROM node_state WHERE fiscal_number = ?",
    )
    .bind(FN_A)
    .fetch_one(&pool)
    .await
    .expect("active_delivery_reservation_id column must exist on node_state after migration 033");

    assert!(val.is_none(), "active_delivery_reservation_id must default to NULL");
}

#[tokio::test]
async fn ns05_active_delivery_reservation_id_check_rejects_wrong_length() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, FN_A).await;

    // 8 bytes (not 16) must violate CHECK (length = 16).
    let err = sqlx::query(
        "INSERT INTO node_state \
             (fiscal_number, mode, shift_state, next_lnd, active_delivery_reservation_id) \
         VALUES (?, 'ONLINE', 'CREATED', 1, ?)",
    )
    .bind(FN_A)
    .bind(&[0x01u8; 8][..])
    .execute(&pool)
    .await
    .expect_err("active_delivery_reservation_id length != 16 must violate CHECK");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "expected CHECK/constraint error, got: {err}"
    );
}

#[tokio::test]
async fn ns06_active_delivery_reservation_id_accepts_null_and_16_bytes() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, FN_A).await;
    seed_fn(&pool, FN_B).await;

    // NULL is accepted (default).
    sqlx::query(
        "INSERT INTO node_state (fiscal_number, mode, shift_state, next_lnd) \
         VALUES (?, 'ONLINE', 'CREATED', 1)",
    )
    .bind(FN_A)
    .execute(&pool)
    .await
    .expect("NULL active_delivery_reservation_id must be accepted");

    // 16-byte blob is accepted.
    sqlx::query(
        "INSERT INTO node_state \
             (fiscal_number, mode, shift_state, next_lnd, active_delivery_reservation_id) \
         VALUES (?, 'ONLINE', 'CREATED', 1, ?)",
    )
    .bind(FN_B)
    .bind(&[0xABu8; 16][..])
    .execute(&pool)
    .await
    .expect("16-byte active_delivery_reservation_id must be accepted");

    let v: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT active_delivery_reservation_id FROM node_state WHERE fiscal_number = ?",
    )
    .bind(FN_B)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(v.as_deref(), Some([0xABu8; 16].as_slice()));
}

// ══════════════════════ ag_* — authorized_generation ══════════════════════

#[tokio::test]
async fn ag01_authorized_generation_column_exists_nullable() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();

    // Column must exist and be NULL at RESERVED_NOT_STARTED.
    let val: Option<i64> = sqlx::query_scalar(
        "SELECT authorized_generation FROM delivery_reservation WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .fetch_one(&pool)
    .await
    .expect("authorized_generation column must exist on delivery_reservation after migration 033");

    assert!(val.is_none(), "authorized_generation must be NULL at RESERVED_NOT_STARTED");
}

#[tokio::test]
async fn ag02_authorized_generation_check_rejects_zero() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    // Bypass the repo to test the raw CHECK: authorized_generation must be NULL or >= 1.
    // We insert RNS and then try to UPDATE to 0.
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    // authorized_generation = 0 must violate CHECK.
    let err = sqlx::query(
        "UPDATE delivery_reservation SET authorized_generation = 0 WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("authorized_generation = 0 must violate CHECK (NULL or >= 1)");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint") || err_has(&err, "immutable"),
        "expected CHECK/constraint/immutable error, got: {err}"
    );
}

#[tokio::test]
async fn ag03_authorized_generation_check_rejects_negative() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    let err = sqlx::query(
        "UPDATE delivery_reservation SET authorized_generation = -1 WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("authorized_generation < 0 must violate CHECK");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint") || err_has(&err, "immutable"),
        "expected CHECK/constraint/immutable error, got: {err}"
    );
}

#[tokio::test]
async fn ag04_authorized_generation_accepts_positive_at_call_started() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    // Advance to CALL_STARTED (legal state to set authorized_generation per spec §5).
    advance_to_cs(&pool, 0x01).await;
    // Now set authorized_generation to 1 (valid: NULL → value, at CALL_STARTED).
    sqlx::query(
        "UPDATE delivery_reservation SET authorized_generation = 1 WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("authorized_generation = 1 at CALL_STARTED must be accepted");

    let val: Option<i64> = sqlx::query_scalar(
        "SELECT authorized_generation FROM delivery_reservation WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(val, Some(1));
}

#[tokio::test]
async fn ag05_authorized_generation_immutable_once_set() {
    // Once authorized_generation is set (non-NULL), it must not be changeable.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;
    sqlx::query(
        "UPDATE delivery_reservation SET authorized_generation = 5 WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("initial set of authorized_generation to 5 must be accepted");

    // Attempt to mutate: 5 → 6.
    let err = sqlx::query(
        "UPDATE delivery_reservation SET authorized_generation = 6 WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("authorized_generation must be immutable once set (5 → 6 must ABORT)");
    assert!(
        err_has(&err, "immutable") || err_has(&err, "constraint"),
        "expected immutable/constraint error, got: {err}"
    );
}

#[tokio::test]
async fn ag06_authorized_generation_cannot_be_cleared_once_set() {
    // Once authorized_generation is set, setting it back to NULL must also be blocked.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;
    sqlx::query(
        "UPDATE delivery_reservation SET authorized_generation = 3 WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("setting authorized_generation to 3 must be accepted");

    // Clear it: 3 → NULL must be blocked.
    let err = sqlx::query(
        "UPDATE delivery_reservation SET authorized_generation = NULL WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("clearing authorized_generation (3 → NULL) must be blocked (immutable)");
    assert!(
        err_has(&err, "immutable") || err_has(&err, "constraint"),
        "expected immutable/constraint error, got: {err}"
    );
}

// ══════════════════════ as_* — apply_state ══════════════════════

#[tokio::test]
async fn as01_apply_state_column_exists_null_at_insert() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();

    let val: Option<String> = sqlx::query_scalar(
        "SELECT apply_state FROM delivery_reservation WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .fetch_one(&pool)
    .await
    .expect("apply_state column must exist on delivery_reservation after migration 033");

    assert!(val.is_none(), "apply_state must be NULL at RESERVED_NOT_STARTED");
}

#[tokio::test]
async fn as02_apply_state_check_rejects_bad_value() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_oo_clean(&pool, 0x01).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation SET apply_state = 'BOGUS_STATE' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("apply_state 'BOGUS_STATE' must be rejected fail-closed");
    // A bogus value is rejected fail-closed by whichever guard fires first: the
    // apply_state transition trigger (BEFORE UPDATE) catches NULL→'BOGUS_STATE' as an
    // illegal transition before the table CHECK vocabulary is even evaluated. Both are
    // valid rejections (the CHECK is defence-in-depth under the trigger).
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint")
            || err_has(&err, "illegal") || err_has(&err, "apply_state"),
        "expected a fail-closed rejection (apply_state CHECK vocabulary OR transition trigger), got: {err}"
    );
}

#[tokio::test]
async fn as03_apply_state_legal_null_to_pending_apply() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_oo_clean(&pool, 0x01).await;

    // NULL → PENDING_APPLY is the first legal step.
    sqlx::query(
        "UPDATE delivery_reservation SET apply_state = 'PENDING_APPLY' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("NULL→PENDING_APPLY must be a legal apply_state transition");

    let val: Option<String> = sqlx::query_scalar(
        "SELECT apply_state FROM delivery_reservation WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(val.as_deref(), Some("PENDING_APPLY"));
}

#[tokio::test]
async fn as04_apply_state_legal_pending_apply_to_applied() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_oo_clean(&pool, 0x01).await;

    sqlx::query(
        "UPDATE delivery_reservation SET apply_state = 'PENDING_APPLY' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("NULL→PENDING_APPLY");

    // PENDING_APPLY → APPLIED is the second legal step.
    sqlx::query(
        "UPDATE delivery_reservation SET apply_state = 'APPLIED' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("PENDING_APPLY→APPLIED must be a legal apply_state transition");

    let val: Option<String> = sqlx::query_scalar(
        "SELECT apply_state FROM delivery_reservation WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(val.as_deref(), Some("APPLIED"));
}

#[tokio::test]
async fn as05_apply_state_illegal_null_to_applied_jump() {
    // NULL → APPLIED skips PENDING_APPLY — must be rejected.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_oo_clean(&pool, 0x01).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation SET apply_state = 'APPLIED' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("NULL→APPLIED (skipping PENDING_APPLY) must be rejected by apply_state trigger");
    assert!(
        err_has(&err, "apply_state") || err_has(&err, "illegal") || err_has(&err, "constraint"),
        "expected apply_state/illegal/constraint error, got: {err}"
    );
}

#[tokio::test]
async fn as06_apply_state_illegal_applied_to_pending_apply_regress() {
    // APPLIED → PENDING_APPLY (regression) must be rejected.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_oo_clean(&pool, 0x01).await;

    sqlx::query(
        "UPDATE delivery_reservation SET apply_state = 'PENDING_APPLY' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("NULL→PENDING_APPLY");
    sqlx::query(
        "UPDATE delivery_reservation SET apply_state = 'APPLIED' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("PENDING_APPLY→APPLIED");

    // Now regress: APPLIED → PENDING_APPLY.
    let err = sqlx::query(
        "UPDATE delivery_reservation SET apply_state = 'PENDING_APPLY' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("APPLIED→PENDING_APPLY regress must be rejected by apply_state trigger");
    assert!(
        err_has(&err, "apply_state") || err_has(&err, "illegal") || err_has(&err, "constraint"),
        "expected apply_state/illegal/constraint error, got: {err}"
    );
}

#[tokio::test]
async fn as07_apply_state_illegal_applied_to_null_regress() {
    // APPLIED → NULL (clear) must be rejected.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_oo_clean(&pool, 0x01).await;

    sqlx::query(
        "UPDATE delivery_reservation SET apply_state = 'PENDING_APPLY' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("NULL→PENDING_APPLY");
    sqlx::query(
        "UPDATE delivery_reservation SET apply_state = 'APPLIED' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("PENDING_APPLY→APPLIED");

    let err = sqlx::query(
        "UPDATE delivery_reservation SET apply_state = NULL WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("APPLIED→NULL must be rejected by apply_state trigger");
    assert!(
        err_has(&err, "apply_state") || err_has(&err, "illegal") || err_has(&err, "constraint"),
        "expected apply_state/illegal/constraint error, got: {err}"
    );
}

#[tokio::test]
async fn as08_apply_state_illegal_pending_apply_to_null_regress() {
    // PENDING_APPLY → NULL must be rejected.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_oo_clean(&pool, 0x01).await;

    sqlx::query(
        "UPDATE delivery_reservation SET apply_state = 'PENDING_APPLY' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("NULL→PENDING_APPLY");

    let err = sqlx::query(
        "UPDATE delivery_reservation SET apply_state = NULL WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("PENDING_APPLY→NULL regress must be rejected by apply_state trigger");
    assert!(
        err_has(&err, "apply_state") || err_has(&err, "illegal") || err_has(&err, "constraint"),
        "expected apply_state/illegal/constraint error, got: {err}"
    );
}

// ══════════════════════ ne_* — node_effect ══════════════════════

#[tokio::test]
async fn ne01_node_effect_column_exists_null_at_insert() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();

    let val: Option<String> = sqlx::query_scalar(
        "SELECT node_effect FROM delivery_reservation WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .fetch_one(&pool)
    .await
    .expect("node_effect column must exist on delivery_reservation after migration 033");

    assert!(val.is_none(), "node_effect must be NULL at RESERVED_NOT_STARTED");
}

#[tokio::test]
async fn ne02_node_effect_check_rejects_bad_value() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_oo_clean(&pool, 0x01).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation SET node_effect = 'NOT_IN_VOCAB' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("node_effect 'NOT_IN_VOCAB' must violate CHECK vocabulary");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "expected CHECK/constraint error, got: {err}"
    );
}

#[tokio::test]
async fn ne03_node_effect_accepts_all_valid_vocabulary_values() {
    // Each of the valid node_effect values (grounded on error_routing.rs) must be accepted.
    // We use a fresh pool per value to avoid fence/immutability collisions.
    let valid_values = [
        "NodeBlocked",          // -11 ERROR_OFFLINE_168 → NodeMode::Blocked flip
        "MacReseedPending",     // -12 ERROR_BAD_HASH_PREV → mac_recovery orchestrator
        "ProbeRequired",        // Decode / -2 close-shift / -15 close-shift → last_chk probe
        "OperatorEscalation",   // -6 ERROR_NOT_PREV_ZREPORT → operator-recoverable
        "FnConfigError",        // -13/-14 NOT_REGISTERED → FnConfigError class
        "WrapperBug",           // Internal / NotFound / FiscalIdMismatch → WrapperBug
        "NoNodeEffect",         // TerminalReject (clean), TransientRetry (no node impact)
    ];
    for val in valid_values {
        let (_d, pool) = fresh_pool().await;
        let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
        insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
        advance_to_oo_clean(&pool, 0x01).await;
        sqlx::query(
            "UPDATE delivery_reservation SET node_effect = ? WHERE reservation_id = ?",
        )
        .bind(val)
        .bind(&[0x01u8; 16][..])
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("node_effect '{val}' must be accepted; got: {e}"));
    }
}

#[tokio::test]
async fn ne04_node_effect_immutable_once_set() {
    // node_effect must be frozen once set — value → different value is blocked.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_oo_clean(&pool, 0x01).await;

    sqlx::query(
        "UPDATE delivery_reservation SET node_effect = 'NodeBlocked' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("initial set of node_effect to NodeBlocked must be accepted");

    // Mutation: NodeBlocked → NoNodeEffect.
    let err = sqlx::query(
        "UPDATE delivery_reservation SET node_effect = 'NoNodeEffect' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("node_effect must be immutable once set (NodeBlocked → NoNodeEffect must ABORT)");
    assert!(
        err_has(&err, "immutable") || err_has(&err, "constraint"),
        "expected immutable/constraint error, got: {err}"
    );
}

#[tokio::test]
async fn ne05_node_effect_cannot_be_cleared_once_set() {
    // node_effect set → NULL must be blocked.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_oo_clean(&pool, 0x01).await;

    sqlx::query(
        "UPDATE delivery_reservation SET node_effect = 'MacReseedPending' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("setting node_effect to MacReseedPending must be accepted");

    let err = sqlx::query(
        "UPDATE delivery_reservation SET node_effect = NULL WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("node_effect → NULL (clearing) must be blocked (immutable)");
    assert!(
        err_has(&err, "immutable") || err_has(&err, "constraint"),
        "expected immutable/constraint error, got: {err}"
    );
}

// ══════════════════════ fg_* — fail-fast activation guard ══════════════════════

/// The fail-fast guard is a TEMP-table assert (mirroring migration 027) that
/// fires at migration apply time if `delivery_reservation` is non-empty.
/// Because sqlx runs migrations transactionally, we cannot replay 033 on a
/// live non-empty table from within the test — the guard runs at CREATE-DB
/// time.  Instead we test the guard mechanism via the equivalent raw SQL
/// (replicating what the migration does) so it can be verified independently.
#[tokio::test]
async fn fg01_fail_fast_guard_sql_rejects_nonempty_table() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();

    // The guard is: INSERT INTO _temp (count) SELECT COUNT(*) FROM delivery_reservation;
    // where _temp has a CHECK (count = 0).  Replicate it verbatim.  TEMP tables are
    // connection-scoped, so CREATE + INSERT + DROP MUST run on ONE acquired connection
    // (a bare `&pool` would spread them across pooled connections — the migration itself
    // runs on a single connection, so this mirrors the real guard faithfully).
    let mut conn = pool.acquire().await.expect("acquire single connection");
    sqlx::query("CREATE TEMP TABLE _fg01_guard (c INTEGER NOT NULL CHECK (c = 0))")
        .execute(&mut *conn)
        .await
        .expect("create temp guard table");

    let err = sqlx::query(
        "INSERT INTO _fg01_guard (c) SELECT COUNT(*) FROM delivery_reservation",
    )
    .execute(&mut *conn)
    .await
    .expect_err("guard must reject non-empty delivery_reservation (COUNT(*) = 1, violates CHECK = 0)");

    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "expected CHECK/constraint from guard, got: {err}"
    );

    sqlx::query("DROP TABLE _fg01_guard").execute(&mut *conn).await.ok();
}

#[tokio::test]
async fn fg02_fail_fast_guard_sql_accepts_empty_table() {
    let (_d, pool) = fresh_pool().await;
    // delivery_reservation is empty — guard must pass. Single acquired connection
    // (TEMP tables are connection-scoped; see fg01).
    let mut conn = pool.acquire().await.expect("acquire single connection");
    sqlx::query("CREATE TEMP TABLE _fg02_guard (c INTEGER NOT NULL CHECK (c = 0))")
        .execute(&mut *conn)
        .await
        .expect("create temp guard table");

    sqlx::query(
        "INSERT INTO _fg02_guard (c) SELECT COUNT(*) FROM delivery_reservation",
    )
    .execute(&mut *conn)
    .await
    .expect("guard must accept empty delivery_reservation");

    sqlx::query("DROP TABLE _fg02_guard").execute(&mut *conn).await.ok();
}

// ══════════════════════ rg_* — 032 regression ══════════════════════

#[tokio::test]
async fn rg01_column_count_is_now_20() {
    // 032 had 17 columns; 033 adds 3 (authorized_generation, apply_state, node_effect).
    let (_d, pool) = fresh_pool().await;
    let cols: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(delivery_reservation)")
            .fetch_all(&pool)
            .await
            .unwrap();
    let names: HashSet<String> = cols.iter().map(|c| c.1.clone()).collect();

    // All 032 columns still present.
    for col in [
        "reservation_id", "document_id", "fiscal_number", "attempt_no", "state",
        "call_started_at", "dps_protocol_id", "protocol_contract_version",
        "capability_profile_version", "endpoint_config_revision", "envelope_hash",
        "remote_correlation_id", "submission_certainty", "response_provenance",
        "routing_class", "created_at", "updated_at",
    ] {
        assert!(names.contains(col), "032 column {col} must still exist in 033; have {names:?}");
    }
    // All 033 new columns present.
    for col in ["authorized_generation", "apply_state", "node_effect"] {
        assert!(names.contains(col), "033 new column {col} must exist; have {names:?}");
    }
    assert_eq!(
        names.len(), 20,
        "total column count must be 20 (17 from 032 + 3 from 033); have {names:?}"
    );
}

#[tokio::test]
async fn rg02_032_triggers_still_present_and_fire() {
    // The 032 triggers must survive the 033 rebuild (name-exact match).
    let (_d, pool) = fresh_pool().await;
    let triggers: HashSet<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='trigger'")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .collect();

    for tg in [
        "delivery_reservation_insert_state",
        "delivery_reservation_no_replace",
        "delivery_reservation_transition",
        "delivery_reservation_immutable",
        "delivery_reservation_append_only",
        "delivery_reservation_updated_at",
    ] {
        assert!(
            triggers.contains(tg),
            "032 trigger {tg} must still exist after 033 migration; have {triggers:?}"
        );
    }
}

#[tokio::test]
async fn rg03_032_insert_state_trigger_still_fires() {
    // The insert_state trigger from 032 must still reject non-RNS inserts.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    let err = sqlx::query(
        "INSERT INTO delivery_reservation \
            (reservation_id, document_id, fiscal_number, attempt_no, state, call_started_at, \
             dps_protocol_id, protocol_contract_version, envelope_hash) \
         VALUES (?, ?, ?, 1, 'CALL_STARTED', '2026-07-17T00:00:00Z', 'FSCO_ZZD', 1, ?)",
    )
    .bind(&[0x01u8; 16][..])
    .bind(doc.as_bytes().to_vec())
    .bind(FN_A)
    .bind(&[0xABu8; 32][..])
    .execute(&pool)
    .await
    .expect_err("032 insert_state trigger must still block non-RNS insert in 033");
    assert!(
        err_has(&err, "reserved_not_started") || err_has(&err, "constraint"),
        "{err}"
    );
}

#[tokio::test]
async fn rg04_032_append_only_trigger_still_fires() {
    // append_only trigger from 032 must still reject DELETEs.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_oo_clean(&pool, 0x01).await;

    let err = sqlx::query("DELETE FROM delivery_reservation WHERE reservation_id = ?")
        .bind(&[0x01u8; 16][..])
        .execute(&pool)
        .await
        .expect_err("032 append_only trigger must still block DELETE in 033");
    assert!(
        err_has(&err, "append-only") || err_has(&err, "constraint"),
        "{err}"
    );
}

#[tokio::test]
async fn rg05_032_immutable_trigger_still_blocks_binding_mutation() {
    // 032 immutable trigger still blocks mutating protocol-binding fields.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();

    let err = sqlx::query(
        "UPDATE delivery_reservation SET dps_protocol_id = 'EVPZ_DPS' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("032 immutable trigger must still block dps_protocol_id mutation in 033");
    assert!(
        err_has(&err, "immutable") || err_has(&err, "constraint"),
        "{err}"
    );
}

#[tokio::test]
async fn rg06_032_fence_still_blocks_second_reservation_while_rns() {
    // 032 fence (ux_reservation_active + collision-guard) still holds under 033.
    let (_d, pool) = fresh_pool().await;
    let doc1 = seed_doc(&pool, FN_A, 0x11, 1).await;
    let doc2 = seed_doc(&pool, FN_A, 0x22, 2).await;
    insert_res(&pool, new_res(0x01, doc1, FN_A)).await.unwrap();

    let err = insert_res(&pool, new_res(0x02, doc2, FN_A))
        .await
        .expect_err("032 fence must still block 2nd reservation on fenced FN in 033");
    let m = err.to_string().to_lowercase();
    assert!(
        m.contains("collision") || m.contains("unique") || m.contains("constraint"),
        "{m}"
    );
}

#[tokio::test]
async fn rg07_node_state_now_has_delivery_generation_and_pointer_columns() {
    // Both new node_state columns from 033 appear in PRAGMA table_info.
    let (_d, pool) = fresh_pool().await;
    let cols: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(node_state)")
            .fetch_all(&pool)
            .await
            .unwrap();
    let names: HashSet<String> = cols.iter().map(|c| c.1.clone()).collect();
    assert!(
        names.contains("delivery_generation"),
        "node_state must have delivery_generation after 033; have {names:?}"
    );
    assert!(
        names.contains("active_delivery_reservation_id"),
        "node_state must have active_delivery_reservation_id after 033; have {names:?}"
    );
}

#[tokio::test]
async fn rg08_no_production_caller_static_pin() {
    // Static call-graph pin: delivery_reservation must still have NO production
    // caller in src/ (mirrors migration_032 p03 — identical predicate).
    use std::process::Command;
    let manifest = env!("CARGO_MANIFEST_DIR");
    let src = format!("{manifest}/src");
    let out = Command::new("grep")
        .args(["-rn", "delivery_reservation", &src])
        .output()
        .expect("grep runs");
    let text = String::from_utf8_lossy(&out.stdout);
    let mut offenders = Vec::new();
    for line in text.lines() {
        let path = line.split(':').next().unwrap_or("");
        let is_repo_file = path.ends_with("repositories/delivery_reservation.rs");
        let is_mod_decl = path.ends_with("repositories/mod.rs");
        if is_repo_file || is_mod_decl {
            continue;
        }
        offenders.push(line.to_string());
    }
    assert!(
        offenders.is_empty(),
        "delivery_reservation must have NO production caller in src/ after 033; found: {offenders:#?}"
    );
}

// ══════════════════════ lc_* — lifecycle CHECKs (field ↔ state) ══════════════════════
// The three 033 outcome/generation fields must be structurally tied to the call
// state (mirroring 032's `call_started_at IS NULL at RNS` and `remote_correlation_id
// IS NULL OR state='OUTCOME_OBSERVED'` patterns), so an illegal-lifecycle write is
// unrepresentable — not merely governed by immutability/transition triggers.

#[tokio::test]
async fn lc01_apply_state_cannot_be_set_before_outcome_observed() {
    // apply_state is the record-then-apply boundary; it may exist ONLY at OUTCOME_OBSERVED.
    // At RESERVED_NOT_STARTED, setting it must be rejected (NULL→PENDING_APPLY is a legal
    // apply_state *transition*, so ONLY the field↔state CHECK catches this).
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();

    let err = sqlx::query(
        "UPDATE delivery_reservation SET apply_state = 'PENDING_APPLY' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("apply_state set at RESERVED_NOT_STARTED must be rejected (field↔state CHECK)");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "expected a CHECK/constraint (apply_state only at OUTCOME_OBSERVED), got: {err}"
    );
}

#[tokio::test]
async fn lc02_node_effect_cannot_be_set_before_outcome_observed() {
    // node_effect mirrors remote_correlation_id: only at OUTCOME_OBSERVED.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await; // CALL_STARTED — still not OUTCOME_OBSERVED

    let err = sqlx::query(
        "UPDATE delivery_reservation SET node_effect = 'NodeBlocked' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("node_effect set before OUTCOME_OBSERVED must be rejected (field↔state CHECK)");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "expected a CHECK/constraint (node_effect only at OUTCOME_OBSERVED), got: {err}"
    );
}

#[tokio::test]
async fn lc03_authorized_generation_cannot_be_set_at_reserved_not_started() {
    // authorized_generation is the RN→CS snapshot; it must be NULL at RESERVED_NOT_STARTED
    // (mirrors 032's `call_started_at IS NULL at RNS`).
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();

    let err = sqlx::query(
        "UPDATE delivery_reservation SET authorized_generation = 1 WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("authorized_generation set at RESERVED_NOT_STARTED must be rejected (field↔state CHECK)");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "expected a CHECK/constraint (authorized_generation NULL at RNS), got: {err}"
    );
}
