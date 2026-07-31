//! Migration 034 (`delivery_reservation` authority integrity) — RED-first
//! regression battery (CS-3 Bridge-0 R4).
//!
//! Each of the four auditor-identified holes from 033 is now rejected by a
//! new trigger installed in 034.  Run this battery BEFORE writing 034 to
//! confirm RED (the illegal writes succeed without the triggers).  After 034
//! is written every test in this file must be GREEN.
//!
//! Holes closed:
//!   H1 — `node_state.delivery_generation` ABA hazard: monotone not enforced
//!         (033 trigger only checks >= 0, not NEW >= OLD).
//!   H2 — `authorized_generation` ↔ `call_started_at` not paired: a CALL_STARTED
//!         row with `authorized_generation = NULL` is allowed by 033.
//!   H3 — At OUTCOME_OBSERVED, `apply_state = NULL` and/or `node_effect = NULL`
//!         are allowed (partially-written authority).
//!   H4 — A clean-accept OO row (`routing_class IS NULL`) can be given
//!         `node_effect = 'NodeBlocked'` (semantic inconsistency).
//!
//! Test groups:
//!   `h1_*` — monotonic `delivery_generation` on `node_state`
//!   `h2_*` — paired `authorized_generation` ↔ `call_started_at`
//!   `h3_*` — OO completeness (`apply_state` + `node_effect` non-NULL)
//!   `h4_*` — clean-accept ⇒ NoNodeEffect constraint
//!   `rg_*` — 034 does NOT break any legal write (regression safety)

use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{self, NewReservation};
use prro::db::tx::with_immediate;
use sqlx::SqlitePool;

// ─────────────────────────── fixtures ───────────────────────────

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations including 034");
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

/// Advance to CALL_STARTED WITH both paired fields (legal under 034).
/// Sets `call_started_at` AND `authorized_generation = 1` atomically.
async fn advance_to_cs_paired(pool: &SqlitePool, res_byte: u8) {
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'CALL_STARTED', \
             call_started_at = '2026-07-17T00:00:00Z', \
             authorized_generation = 1 \
         WHERE reservation_id = ?",
    )
    .bind(&[res_byte; 16][..])
    .execute(pool)
    .await
    .expect("RN→CS with paired authorized_generation must be legal under 034");
}

/// Advance to OUTCOME_OBSERVED (clean accept) WITH all required 034 fields.
/// Commits apply_state = 'PENDING_APPLY', node_effect = 'NoNodeEffect' at OO.
async fn advance_to_oo_clean(pool: &SqlitePool, res_byte: u8) {
    advance_to_cs_paired(pool, res_byte).await;
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NoNodeEffect', \
             evidence_kind = 'Accepted', \
             evidence_text = '4000000001', \
             remote_correlation_id = '4000000001' \
         WHERE reservation_id = ?",
    )
    .bind(&[res_byte; 16][..])
    .execute(pool)
    .await
    .expect("CS→OO clean accept with PENDING_APPLY + NoNodeEffect must be legal under 034");
}

fn err_has(err: &sqlx::Error, needle: &str) -> bool {
    err.to_string().to_lowercase().contains(needle)
}

// ══════════════════════ h1_* — monotonic delivery_generation ══════════════════════
// H1: 033 only enforced >= 0; 034 adds a trigger that rejects NEW < OLD.

#[tokio::test]
async fn h1_01_delivery_generation_decrease_rejected() {
    // Advance delivery_generation to 5, then attempt to decrease to 3.
    // Under 033 this succeeded (only >= 0 was checked).
    // Under 034 this must ABORT (not monotonic).
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN_A).await;

    // Advance to 5.
    sqlx::query("UPDATE node_state SET delivery_generation = 5 WHERE fiscal_number = ?")
        .bind(FN_A)
        .execute(&pool)
        .await
        .expect("delivery_generation = 5 must be accepted");

    // Attempt decrease: 5 → 3.
    let err = sqlx::query("UPDATE node_state SET delivery_generation = 3 WHERE fiscal_number = ?")
        .bind(FN_A)
        .execute(&pool)
        .await
        .expect_err("delivery_generation decrease (5→3) must be ABORT under 034 monotone trigger");
    assert!(
        err_has(&err, "monoton")
            || err_has(&err, "delivery_generation")
            || err_has(&err, "constraint")
            || err_has(&err, "abort"),
        "expected monotone/delivery_generation/constraint/abort error, got: {err}"
    );
}

#[tokio::test]
async fn h1_02_delivery_generation_decrease_to_zero_rejected() {
    // Advance to 2, then attempt to decrease to 0.
    // 0 is >= 0 (passes 033 check) but IS a decrease — must be rejected by 034.
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN_A).await;

    sqlx::query("UPDATE node_state SET delivery_generation = 2 WHERE fiscal_number = ?")
        .bind(FN_A)
        .execute(&pool)
        .await
        .expect("delivery_generation = 2 must be accepted");

    let err = sqlx::query("UPDATE node_state SET delivery_generation = 0 WHERE fiscal_number = ?")
        .bind(FN_A)
        .execute(&pool)
        .await
        .expect_err("delivery_generation decrease (2→0) must be ABORT — ABA hazard");
    assert!(
        err_has(&err, "monoton")
            || err_has(&err, "delivery_generation")
            || err_has(&err, "constraint")
            || err_has(&err, "abort"),
        "expected monotone/delivery_generation/constraint error, got: {err}"
    );
}

#[tokio::test]
async fn h1_03_delivery_generation_same_value_rejected() {
    // Same-value update (5→5) is a no-op for the fence but the trigger must
    // enforce STRICT monotone (NEW < OLD is false for equal values, so this
    // depends on the trigger predicate).  The spec says "cannot be DECREASED"
    // — equal is allowed (the trigger condition is NEW < OLD, not NEW <= OLD).
    // Verify that same-value is NOT rejected.
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN_A).await;

    sqlx::query("UPDATE node_state SET delivery_generation = 5 WHERE fiscal_number = ?")
        .bind(FN_A)
        .execute(&pool)
        .await
        .expect("delivery_generation = 5 must be accepted");

    // 5 → 5 (same, no decrease): must NOT be rejected.
    sqlx::query("UPDATE node_state SET delivery_generation = 5 WHERE fiscal_number = ?")
        .bind(FN_A)
        .execute(&pool)
        .await
        .expect("delivery_generation same-value update (5→5) must be accepted (not a decrease)");
}

#[tokio::test]
async fn h1_04_delivery_generation_increase_accepted() {
    // Forward advance must still be accepted.
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN_A).await;

    sqlx::query("UPDATE node_state SET delivery_generation = 3 WHERE fiscal_number = ?")
        .bind(FN_A)
        .execute(&pool)
        .await
        .expect("delivery_generation = 3 must be accepted");

    sqlx::query("UPDATE node_state SET delivery_generation = 7 WHERE fiscal_number = ?")
        .bind(FN_A)
        .execute(&pool)
        .await
        .expect("delivery_generation forward advance (3→7) must be accepted");

    let val: i64 =
        sqlx::query_scalar("SELECT delivery_generation FROM node_state WHERE fiscal_number = ?")
            .bind(FN_A)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(val, 7);
}

// ══════════════════════ h2_* — authorized_generation ↔ call_started_at pairing ══════════════════════
// H2: 034 adds a trigger that rejects when (call_started_at IS NULL) <> (authorized_generation IS NULL).
// The legal invariant: both NULL at RNS, both non-NULL at CS (set together at RN→CS).

#[tokio::test]
async fn h2_01_cs_without_authorized_generation_rejected() {
    // The 033 advance_to_cs helper set call_started_at but NOT authorized_generation.
    // Under 033 this succeeded.  Under 034 this must ABORT.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();

    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'CALL_STARTED', call_started_at = '2026-07-17T00:00:00Z' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err(
        "CALL_STARTED with call_started_at set but authorized_generation=NULL must ABORT under 034",
    );
    assert!(
        err_has(&err, "authorized_generation")
            || err_has(&err, "call_started_at")
            || err_has(&err, "pairing")
            || err_has(&err, "constraint")
            || err_has(&err, "abort"),
        "expected pairing/authorized_generation/constraint/abort error, got: {err}"
    );
}

#[tokio::test]
async fn h2_02_authorized_generation_without_call_started_at_rejected() {
    // Setting authorized_generation at RNS (no call_started_at) is already blocked
    // by lc03 (field↔state CHECK: authorized_generation NULL at RNS).
    // This test confirms the pairing trigger also catches the symmetric case:
    // if somehow authorized_generation is set without call_started_at being set
    // (e.g. a direct UPDATE to authorized_generation alone on an RNS row).
    // The field↔state CHECK on RNS (state='RESERVED_NOT_STARTED' OR authorized_generation IS NULL)
    // catches this first, so both guards cooperate.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();

    // At RNS: trying to set authorized_generation without call_started_at.
    // Blocked by either the 033 field↔state CHECK or the 034 pairing trigger.
    let err = sqlx::query(
        "UPDATE delivery_reservation SET authorized_generation = 1 WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err(
        "authorized_generation set without call_started_at must be rejected (pairing or state CHECK)",
    );
    assert!(
        err_has(&err, "check")
            || err_has(&err, "constraint")
            || err_has(&err, "pairing")
            || err_has(&err, "authorized_generation")
            || err_has(&err, "abort"),
        "expected check/constraint/pairing/authorized_generation/abort error, got: {err}"
    );
}

#[tokio::test]
async fn h2_03_paired_cs_transition_accepted() {
    // Setting both call_started_at AND authorized_generation together at RN→CS must be accepted.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();

    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'CALL_STARTED', \
             call_started_at = '2026-07-17T00:00:00Z', \
             authorized_generation = 1 \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("RN→CS with both call_started_at + authorized_generation must be accepted by 034");

    let (csa, ag): (Option<String>, Option<i64>) = sqlx::query_as(
        "SELECT call_started_at, authorized_generation FROM delivery_reservation WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(csa.is_some(), "call_started_at must be set");
    assert_eq!(ag, Some(1), "authorized_generation must be 1");
}

// ══════════════════════ h3_* — OO completeness ══════════════════════
// H3: At OUTCOME_OBSERVED, apply_state=NULL and/or node_effect=NULL must be rejected.

#[tokio::test]
async fn h3_01_oo_with_apply_state_null_rejected() {
    // 033 advance_to_oo_clean set OO without apply_state or node_effect.
    // Under 033 this succeeded.  Under 034 this must ABORT.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs_paired(&pool, 0x01).await;

    // OO transition WITHOUT apply_state set (NULL) — must ABORT.
    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             node_effect = 'NoNodeEffect' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("OO with apply_state=NULL must ABORT under 034 OO-completeness trigger");
    assert!(
        err_has(&err, "apply_state")
            || err_has(&err, "node_effect")
            || err_has(&err, "outcome_observed")
            || err_has(&err, "constraint")
            || err_has(&err, "abort"),
        "expected apply_state/node_effect/outcome_observed/constraint/abort error, got: {err}"
    );
}

#[tokio::test]
async fn h3_02_oo_with_node_effect_null_rejected() {
    // OO transition WITHOUT node_effect (NULL) — must ABORT.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs_paired(&pool, 0x01).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             apply_state = 'PENDING_APPLY' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("OO with node_effect=NULL must ABORT under 034 OO-completeness trigger");
    assert!(
        err_has(&err, "apply_state")
            || err_has(&err, "node_effect")
            || err_has(&err, "outcome_observed")
            || err_has(&err, "constraint")
            || err_has(&err, "abort"),
        "expected apply_state/node_effect/outcome_observed/constraint/abort error, got: {err}"
    );
}

#[tokio::test]
async fn h3_03_oo_with_both_null_rejected() {
    // OO with both apply_state and node_effect NULL — must ABORT.
    // This is the exact case the 033 advance_to_oo_clean helper produced.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs_paired(&pool, 0x01).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("OO with both apply_state=NULL and node_effect=NULL must ABORT under 034");
    assert!(
        err_has(&err, "apply_state")
            || err_has(&err, "node_effect")
            || err_has(&err, "outcome_observed")
            || err_has(&err, "constraint")
            || err_has(&err, "abort"),
        "expected apply_state/node_effect/outcome_observed/constraint/abort error, got: {err}"
    );
}

#[tokio::test]
async fn h3_04_oo_with_both_set_accepted() {
    // OO with BOTH apply_state='PENDING_APPLY' AND node_effect='NoNodeEffect' must be accepted.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_oo_clean(&pool, 0x01).await; // uses the 034-conformant helper

    let (astate, neffect): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT apply_state, node_effect FROM delivery_reservation WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(astate.as_deref(), Some("PENDING_APPLY"));
    assert_eq!(neffect.as_deref(), Some("NoNodeEffect"));
}

#[tokio::test]
async fn h3_05_oo_terminal_reject_with_both_set_accepted() {
    // OO with a TerminalReject outcome (routing_class NOT NULL) + both apply_state and
    // node_effect set must be accepted.  Verifies the completeness trigger fires only
    // when both fields are absent, not when both are present on a non-clean-accept path.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs_paired(&pool, 0x01).await;

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
             evidence_digest = X'0000000000000000000000000000000000000000000000000000000000000000' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect(
        "OO with TerminalReject + apply_state=PENDING_APPLY + node_effect=NoNodeEffect must be accepted",
    );
}

// ══════════════════════ h4_* — clean-accept ⇒ NoNodeEffect ══════════════════════
// H4: A clean-accept OO row (routing_class IS NULL) must have node_effect = 'NoNodeEffect'.
// node_effect = 'NodeBlocked' (or any non-NoNodeEffect) on a clean accept is semantic nonsense.

#[tokio::test]
async fn h4_01_clean_accept_with_node_blocked_rejected() {
    // routing_class IS NULL (clean accept) + node_effect = 'NodeBlocked' must ABORT.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs_paired(&pool, 0x01).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'NodeBlocked' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err(
        "clean-accept OO (routing_class=NULL) with node_effect='NodeBlocked' must ABORT under 034",
    );
    assert!(
        err_has(&err, "clean")
            || err_has(&err, "node_effect")
            || err_has(&err, "nonodeeffect")
            || err_has(&err, "constraint")
            || err_has(&err, "abort")
            || err_has(&err, "evidence"),
        "expected clean/node_effect/nonodeeffect/constraint/abort/evidence error, got: {err}"
    );
}

#[tokio::test]
async fn h4_02_clean_accept_with_mac_reseed_pending_rejected() {
    // routing_class IS NULL + node_effect = 'MacReseedPending' must ABORT.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs_paired(&pool, 0x01).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'MacReseedPending' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("clean-accept OO with node_effect='MacReseedPending' must ABORT under 034");
    assert!(
        err_has(&err, "clean")
            || err_has(&err, "node_effect")
            || err_has(&err, "nonodeeffect")
            || err_has(&err, "constraint")
            || err_has(&err, "abort")
            || err_has(&err, "evidence"),
        "expected clean/node_effect/nonodeeffect/constraint/abort/evidence error, got: {err}"
    );
}

#[tokio::test]
async fn h4_03_clean_accept_with_no_node_effect_accepted() {
    // routing_class IS NULL + node_effect = 'NoNodeEffect' is the ONLY valid combo.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_oo_clean(&pool, 0x01).await; // routing_class=NULL + NoNodeEffect

    let (rc, ne): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT routing_class, node_effect FROM delivery_reservation WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(rc.is_none(), "clean accept must have routing_class=NULL");
    assert_eq!(ne.as_deref(), Some("NoNodeEffect"));
}

#[tokio::test]
async fn h4_04_non_null_routing_class_accepts_node_blocked() {
    // routing_class IS NOT NULL (a reject/degraded outcome) + node_effect='NodeBlocked' is valid.
    // The h4 constraint only fires when routing_class IS NULL (clean accept).
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs_paired(&pool, 0x01).await;

    // Offline168 (-11) is the real classifier output that yields this pairing:
    // (SUBMITTED, PARSED_DPS_ENVELOPE, TerminalReject, NodeBlocked). Because routing_class
    // is NON-NULL, the H4 clean-accept trigger does NOT fire, so NodeBlocked is valid here.
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
             evidence_digest = X'0000000000000000000000000000000000000000000000000000000000000000' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("routing_class=TerminalReject (non-NULL) + node_effect=NodeBlocked must be accepted");
}

#[tokio::test]
async fn h4_05_submitted_unknown_with_routing_class_and_probe_required_accepted() {
    // SUBMITTED_UNKNOWN at OO: the 033 CHECK forces routing_class IS NOT NULL
    // (CHECK: routing_class IS NOT NULL OR state <> OO OR submission_certainty = SUBMITTED).
    // So for SUBMITTED_UNKNOWN, routing_class must be set — use DrainChainSettleRetry
    // (the only SUBMITTED_UNKNOWN + PARSED_DPS_ENVELOPE routing class from error_routing.rs).
    // The H4 trigger only fires when routing_class IS NULL, so this path is UNAFFECTED.
    // NOTE: SUBMITTED_UNKNOWN typically uses NO_RESPONSE but DrainChainSettleRetry requires
    // PARSED_DPS_ENVELOPE. Use ProbeRequired + AUTHENTICATED_PEER for the test instead.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs_paired(&pool, 0x01).await;

    // SUBMITTED_UNKNOWN + routing_class=ProbeRequired + AUTHENTICATED_PEER + ProbeRequired node_effect
    // routing_class IS NOT NULL → H4 trigger does NOT fire.
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', \
             submission_certainty = 'SUBMITTED_UNKNOWN', \
             response_provenance = 'AUTHENTICATED_PEER', \
             routing_class = 'ProbeRequired', \
             apply_state = 'PENDING_APPLY', \
             node_effect = 'ProbeRequired', \
             evidence_kind = 'RemoteAuthStatus', \
             evidence_digest = X'0000000000000000000000000000000000000000000000000000000000000000' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect(
        "SUBMITTED_UNKNOWN + routing_class=ProbeRequired (non-NULL) + ProbeRequired node_effect \
         must be accepted (H4 trigger scoped to routing_class IS NULL only)",
    );
}

// ══════════════════════ rg_* — 034 regression safety ══════════════════════
// Confirm that legal writes are not broken by the 034 triggers.

#[tokio::test]
async fn rg01_full_lifecycle_rns_cs_oo_pending_applied() {
    // Full lifecycle: RNS → CS (paired) → OO (complete) → PENDING_APPLY → APPLIED.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_oo_clean(&pool, 0x01).await;

    // PENDING_APPLY → APPLIED
    sqlx::query("UPDATE delivery_reservation SET apply_state = 'APPLIED' WHERE reservation_id = ?")
        .bind(&[0x01u8; 16][..])
        .execute(&pool)
        .await
        .expect("PENDING_APPLY→APPLIED must be accepted");

    let astate: Option<String> =
        sqlx::query_scalar("SELECT apply_state FROM delivery_reservation WHERE reservation_id = ?")
            .bind(&[0x01u8; 16][..])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(astate.as_deref(), Some("APPLIED"));
}

#[tokio::test]
async fn rg02_delivery_generation_advance_sequence() {
    // node_state.delivery_generation advances monotonically: 0 → 1 → 5 → 100.
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, FN_A).await;

    for val in [1i64, 5, 100] {
        sqlx::query("UPDATE node_state SET delivery_generation = ? WHERE fiscal_number = ?")
            .bind(val)
            .bind(FN_A)
            .execute(&pool)
            .await
            .unwrap_or_else(|e| {
                panic!("delivery_generation advance to {val} must be accepted: {e}")
            });
    }

    let final_val: i64 =
        sqlx::query_scalar("SELECT delivery_generation FROM node_state WHERE fiscal_number = ?")
            .bind(FN_A)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(final_val, 100);
}

#[tokio::test]
async fn rg03_034_trigger_names_installed() {
    // Verify all four 034 triggers are present in sqlite_master.
    let (_d, pool) = fresh_pool().await;
    let triggers: std::collections::HashSet<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='trigger'")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .collect();

    for tg in [
        "node_state_delivery_generation_monotone",
        "delivery_reservation_cs_pairing_insert",
        "delivery_reservation_cs_pairing_update",
        "delivery_reservation_oo_completeness",
        "delivery_reservation_clean_accept_node_effect",
    ] {
        assert!(
            triggers.contains(tg),
            "034 trigger {tg} must exist in sqlite_master; have {triggers:?}"
        );
    }
}

#[tokio::test]
async fn rg04_033_triggers_still_present() {
    // 034 must NOT disturb the 033 trigger set.
    let (_d, pool) = fresh_pool().await;
    let triggers: std::collections::HashSet<String> =
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
        "delivery_reservation_apply_state_transition",
        "node_state_active_reservation_id_check",
        "node_state_delivery_generation_check",
    ] {
        assert!(
            triggers.contains(tg),
            "033 trigger {tg} must still exist after 034; have {triggers:?}"
        );
    }
}
