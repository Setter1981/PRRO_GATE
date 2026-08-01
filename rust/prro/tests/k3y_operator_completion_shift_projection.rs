//! bd PRRO_GATE-k3y — RED-first: an operator-Accepted ONLINE `SHIFT_OPEN` must leave the shift
//! OPEN, and a later boot must not orphan it.
//!
//! The claim under test (ticket k3y, triage 2026-08-01): the LIVE operator path is
//! `main.rs → admin::resolve_operator_pending` (`admin.rs:498`), which calls
//! `delivery_reservation::complete_operator_pending` DIRECTLY (`admin.rs:537`). It therefore never
//! reaches `services::reconciliation::operator_completion::complete_operator_resolution` — the
//! orchestrator that owns (1) the shift-state projection via `apply_shift_transition` and (2) the
//! Critical `OPERATOR_COMPLETION` audit. That orchestrator documents itself as "The SOLE production
//! caller" (`operator_completion.rs:3`) and has ZERO production callers.
//!
//! Consequence, in two steps:
//!   * `k3y_a` — right after the completion the document is issued (`SENT` + `server_fiscal_no` +
//!     seed advanced) while `shifts.state` stays `OPENING` and `node_state.shift_state` stays
//!     `Opening`. The register believes a shift is still opening while its opening document is
//!     confirmed by DPS.
//!   * `k3y_b` — the damaging step. `boot_phase` branch (e2) fires once the document reaches a
//!     TERMINAL state (`list_pending_for_fn`, `fiscal_documents.rs:754`, counts SENT/KVT1/KVT2 as
//!     pending, so (e2) stays quiet until `ACK`). Its orphan SELECT is
//!     `WHERE fiscal_number = ? AND state IN ('OPENING','CLOSING')` — nothing excludes a shift whose
//!     `SHIFT_OPEN` is ISSUED. `force_orphan_shift_to_error` then RAW-updates the shift to `ERROR`
//!     and `clear_active_shift_projection` resets the projection to `Closed`. Net: DPS holds the
//!     shift OPEN, we hold it ERROR/CLOSED — a shift-state divergence with the peer.
//!
//! `k3y_b` is what settles the one doubt the reading could not: whether the orphan SELECT really
//! matches an issued `SHIFT_OPEN`'s row. Both tests pin the contract regardless of which fix
//! direction is chosen (route the CLI through the orchestrator, or teach (e2) to skip issued
//! shifts) — they assert the OBSERVABLE end state, not the mechanism.
//!
//! The fuzzer cannot catch this today: its interpreter drives the same admin seam
//! (`invariant_fuzzer/interp.rs:1426`), so the model mirrors the bypass along with the defect.

use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{
    self, resume_crashed_reservation, NewReservation, OperatorResolution,
};
use prro::db::tx::with_immediate;
use prro::services::reconciliation::boot_phase::{self, BranchOutcome};
use prro::services::reconciliation::ReconcileGuard;
use sqlx::SqlitePool;

const TS: &str = "2026-08-01T00:00:00Z";
/// The document's `unsigned_xml_sha256` — the value an online `Accepted` advances the chain seed to.
const SEED: [u8; 32] = [0x77; 32];
/// The server fiscal number the operator attests DPS assigned.
const ATTESTED_SFN: &str = "4000162280";

fn recon_guard() -> ReconcileGuard<'static> {
    ReconcileGuard::for_integration_test_only()
}

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations");
    (dir, pool)
}

/// The realistic pre-state: an ONLINE `SHIFT_OPEN` mid-wire. `shifts.state = OPENING`,
/// `node_state.shift_state = OPENING` with `current_shift_id` pointing at it, and the document
/// `SENDING` (the ADR-M3-A9 intent marker written BEFORE the wire send).
async fn seed_opening_shift(
    pool: &SqlitePool,
    fscl: &str,
    doc_byte: u8,
    shift_byte: u8,
) -> DocumentId {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fscl)
    .execute(pool)
    .await
    .expect("seed fiscal_number_config");

    let shift_bytes = vec![shift_byte; 16];
    let doc_bytes = vec![doc_byte; 16];

    sqlx::query(
        "INSERT INTO shifts(shift_id, fiscal_number, state, open_mode, opened_by_cashier_id, \
            open_document_id) \
         VALUES (?, ?, 'OPENING', 'ONLINE', 'cashier-1', ?)",
    )
    .bind(&shift_bytes)
    .bind(fscl)
    .bind(&doc_bytes)
    .execute(pool)
    .await
    .expect("seed shifts");

    sqlx::query(
        "INSERT INTO node_state (fiscal_number, mode, shift_state, current_shift_id, next_lnd) \
         VALUES (?, 'ONLINE', 'OPENING', ?, 2)",
    )
    .bind(fscl)
    .bind(&shift_bytes)
    .execute(pool)
    .await
    .expect("seed node_state");

    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, shift_id, lnd, \
            doc_type, state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, unsigned_xml_sha256) \
         VALUES (?, ?, ?, ?, 1, 'SHIFT_OPEN', 'SENDING', 'b1', 't1', 'ONLINE', \
            '2026-08-01T09:00:00Z', '{}', ?, ?)",
    )
    .bind(&doc_bytes)
    .bind(vec![doc_byte ^ 0xFF; 16])
    .bind(fscl)
    .bind(&shift_bytes)
    .bind(vec![0u8; 32])
    .bind(&SEED[..])
    .execute(pool)
    .await
    .expect("seed fiscal_documents");

    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

/// Authorize (RN → CALL_STARTED) then boot-resume the crash → `OUTCOME_OBSERVED` +
/// `PENDING_APPLY` + node `STOP_MODE`. This is the held state an operator resolves.
async fn held_pending(pool: &SqlitePool, res_byte: u8, doc: DocumentId, fscl: &str) {
    let row = NewReservation {
        reservation_id: [res_byte; 16],
        document_id: doc,
        fiscal_number: fscl.to_string(),
        dps_protocol_id: "FSCO_ZZD".to_string(),
        protocol_contract_version: 1,
        capability_profile_version: None,
        endpoint_config_revision: None,
        envelope_hash: [0xAB; 32],
    };
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            delivery_reservation::authorize_submission(tx, row, TS)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("authorize");
    let fscl_owned = fscl.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            resume_crashed_reservation(tx, [res_byte; 16], &fscl_owned)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("resume to PENDING_APPLY + STOP_MODE");
}

async fn shift_state(pool: &SqlitePool, shift_byte: u8) -> String {
    sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
        .bind(vec![shift_byte; 16])
        .fetch_one(pool)
        .await
        .expect("shift row")
}

async fn node_shift_state(pool: &SqlitePool, fscl: &str) -> String {
    sqlx::query_scalar("SELECT shift_state FROM node_state WHERE fiscal_number = ?")
        .bind(fscl)
        .fetch_one(pool)
        .await
        .expect("node_state row")
}

async fn doc_state(pool: &SqlitePool, doc_byte: u8) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(vec![doc_byte; 16])
        .fetch_one(pool)
        .await
        .expect("document row")
}

async fn doc_sfn(pool: &SqlitePool, doc_byte: u8) -> Option<String> {
    sqlx::query_scalar("SELECT server_fiscal_no FROM fiscal_documents WHERE document_id = ?")
        .bind(vec![doc_byte; 16])
        .fetch_one(pool)
        .await
        .expect("document row")
}

async fn audit_count(pool: &SqlitePool, event_type: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type = ?")
        .bind(event_type)
        .fetch_one(pool)
        .await
        .expect("audit_log count")
}

/// Drive the REAL production operator seam and assert the completion itself succeeded — the
/// document IS issued. Shared by both tests so neither can pass on a completion that silently
/// refused.
async fn complete_accepted_and_assert_issued(pool: &SqlitePool, fscl: &str, doc_byte: u8) {
    let outcome = prro::admin::resolve_operator_pending(
        pool,
        fscl,
        [0x0A; 16],
        OperatorResolution::Accepted {
            fiscal_number: ATTESTED_SFN.into(),
        },
    )
    .await
    .expect("admin resolve completes");

    assert!(outcome.applied, "the operator resolution applied");
    assert!(
        outcome.seed_advanced,
        "an online Accepted advances the chain seed — this IS the issuance moment"
    );
    assert_eq!(
        doc_state(pool, doc_byte).await,
        "SENT",
        "Accepted issues the SHIFT_OPEN document"
    );
    assert_eq!(
        doc_sfn(pool, doc_byte).await.as_deref(),
        Some(ATTESTED_SFN),
        "the server fiscal number the operator attested is stamped"
    );
}

// ═══════════ k3y_a — the completion must commit the forward shift edge ═══════════

#[tokio::test]
async fn k3y_a_operator_accepted_shift_open_commits_the_forward_shift_edge() {
    let (_dir, pool) = fresh_pool().await;
    let fscl = "5000000901";
    let doc = seed_opening_shift(&pool, fscl, 0x1A, 0x5A).await;
    held_pending(&pool, 0x0A, doc, fscl).await;

    assert_eq!(shift_state(&pool, 0x5A).await, "OPENING", "precondition");
    assert_eq!(
        node_shift_state(&pool, fscl).await,
        "OPENING",
        "precondition"
    );

    complete_accepted_and_assert_issued(&pool, fscl, 0x1A).await;

    // The document is issued and DPS holds the shift OPEN. Local state must say the same.
    // §3.4 edge (ShiftOpen, online, Accepted) = Opening -> Opened.
    assert_eq!(
        shift_state(&pool, 0x5A).await,
        "OPENED",
        "k3y: an operator-Accepted SHIFT_OPEN must move the shift Opening -> Opened; \
         the CLI bypasses `complete_operator_resolution`, so the projection never runs"
    );
    assert_eq!(
        node_shift_state(&pool, fscl).await,
        "OPENED",
        "k3y: `node_state.shift_state` is the register's own view — it must not stay `Opening` \
         while the opening document is confirmed"
    );

    // The Critical operator-evidence audit the bypass also skips (operator_completion.rs:152-162).
    assert_eq!(
        audit_count(&pool, "OPERATOR_COMPLETION").await,
        1,
        "k3y: the Critical OPERATOR_COMPLETION audit is the durable operator evidence for a \
         manual fiscal decision — it must exist"
    );
}

// ═══════════ k3y_b — a later boot must not orphan the issued shift ═══════════

#[tokio::test]
async fn k3y_b_boot_must_not_orphan_a_shift_whose_open_document_is_issued() {
    let (_dir, pool) = fresh_pool().await;
    let fscl = "5000000902";
    let doc = seed_opening_shift(&pool, fscl, 0x1B, 0x5B).await;
    held_pending(&pool, 0x0A, doc, fscl).await;

    complete_accepted_and_assert_issued(&pool, fscl, 0x1B).await;

    // Advance the issued document to its terminal ACK, as the KVT1/KVT2 ladder does. This is the
    // trigger: `list_pending_for_fn` counts SENT/KVT1/KVT2 as pending, so branch (e2) is quiet
    // until here — it fires on the NEXT boot after the doc terminalises.
    sqlx::query("UPDATE fiscal_documents SET state = 'ACK' WHERE document_id = ?")
        .bind(vec![0x1B; 16])
        .execute(&pool)
        .await
        .expect("advance to ACK");

    let outcome = boot_phase::run_boot_reconciliation(&recon_guard(), &pool, fscl, None)
        .await
        .expect("boot reconciliation runs");

    assert!(
        !matches!(outcome, BranchOutcome::OrphanShiftResolved { .. }),
        "k3y: a shift whose SHIFT_OPEN is ISSUED (server_fiscal_no stamped, seed advanced) is NOT \
         an orphan — branch (e2) must not claim it; got {outcome:?}"
    );
    assert_eq!(
        shift_state(&pool, 0x5B).await,
        "OPENED",
        "k3y: boot marked a DPS-accepted shift ERROR. `force_orphan_shift_to_error` RAW-updates it, \
         bypassing the §4.1 transition whitelist — the shift is unrecoverable locally while DPS \
         holds it open"
    );
    assert_eq!(
        node_shift_state(&pool, fscl).await,
        "OPENED",
        "k3y: `clear_active_shift_projection` reset the register to Closed — local state says \
         'no shift', DPS says 'open'. That is a peer divergence, not a stale projection"
    );
    assert_eq!(
        audit_count(&pool, "SHIFT_BOOT_ORPHAN_ERROR").await,
        0,
        "k3y: the orphan SELECT (`state IN ('OPENING','CLOSING')`) matched an issued shift — this \
         assertion is the one that settles whether it really does"
    );
}
