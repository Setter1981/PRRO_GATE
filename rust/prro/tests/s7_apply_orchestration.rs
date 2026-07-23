//! CS-3 S7-1 Q3 — apply-orchestration gap-1 proof tests.
//!
//! Exercises `apply_orchestration::apply_recorded_outcome` directly, proving:
//!
//! - `ao01`: an online `SHIFT_OPEN` Accepted apply fires the shift edge 3 (`Opening → Opened`)
//!   in the SAME tx as the doc-state CAS — `apply_outcome` alone does NOT fire this edge; the
//!   orchestration is what closes the gap.
//! - `ao02`: an online `SELL` Accepted apply releases the doc to `Sent` with no shift-edge attempt
//!   (SELL has no shift edge).
//! - `ao03`: a `-12`/MacReseedPending HOLD propagates through the anyhow boundary with
//!   `downcast_ref::<ApplyError>() == Some(HeldNotAutoRelease)`, proving FIX-C: the boot
//!   consumer's downcast still classifies HOLDs correctly after the anyhow wrapping.
//!
//! `ao01`-`ao03` exercise `apply_recorded_outcome` directly; `ao04` drives it through its live
//! production caller `reservation_boot_pass::run` (the crash-resume path — S7-1 B3 re-audit fix).

use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{self, ApplyError, NewReservation};
use prro::db::tx::with_immediate;
use prro::services::reconciliation::reservation_boot_pass;
use prro::services::reconciliation::ReconcileGuard;
use prro::services::write_path::apply_orchestration::apply_recorded_outcome;
use sqlx::SqlitePool;

const TS: &str = "2026-07-21T00:00:00Z";
const SEED: [u8; 32] = [0x77; 32];
const DIG: [u8; 32] = [0x5A; 32];

// The shift id used in edge tests (16 bytes, all 0x5A).
const SHIFT: [u8; 16] = [0x5a; 16];

// ─── helpers ──────────────────────────────────────────────────────────────────

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations");
    (dir, pool)
}

/// Seed FN config and node_state (in the given shift_state, with optional current_shift_id).
async fn seed_fn_node(
    pool: &SqlitePool,
    fscl: &str,
    shift_state: &str,
    current_shift_id: Option<&[u8]>,
) {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fscl)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO node_state \
         (fiscal_number, mode, shift_state, current_shift_id, next_lnd) \
         VALUES (?, 'ONLINE', ?, ?, 1)",
    )
    .bind(fscl)
    .bind(shift_state)
    .bind(current_shift_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a shifts row in the given state (opening `cash_balance_kop = 0`).
async fn seed_shift(pool: &SqlitePool, shift_id: &[u8], fscl: &str, state: &str) {
    seed_shift_with_cash(pool, shift_id, fscl, state, 0).await;
}

/// Seed a shifts row in the given state with an explicit opening `cash_balance_kop`
/// (the opening ANCHOR).  The CLOSE-edge tests (ao05/ao06) seed an ISSUED ACK SELL
/// on top (`seed_ack_sell_cash`) so the derived closing cash DIFFERS from this
/// anchor — making the `update_cash_balance_tx` write on the CLOSE path
/// load-bearing (deleting it leaves this anchor, so the assert bites).
async fn seed_shift_with_cash(
    pool: &SqlitePool,
    shift_id: &[u8],
    fscl: &str,
    state: &str,
    cash_balance_kop: i64,
) {
    sqlx::query(
        "INSERT INTO shifts(shift_id, fiscal_number, serial, state, open_mode, \
            cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, ?, 'ONLINE', ?, 'test-cashier')",
    )
    .bind(shift_id)
    .bind(fscl)
    .bind(state)
    .bind(cash_balance_kop)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a `SENDING` fiscal document with the given doc_type and optional shift_id.
/// Online-origin when `offline_fiscal_no` is None.
async fn seed_sending_doc(
    pool: &SqlitePool,
    fscl: &str,
    doc_byte: u8,
    doc_type: &str,
    shift_id_bytes: Option<&[u8]>,
    offline_fiscal_no: Option<i64>,
) -> DocumentId {
    let doc_bytes = vec![doc_byte; 16];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, shift_id, lnd, \
            doc_type, state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, unsigned_xml_sha256, offline_fiscal_no) \
         VALUES (?, ?, ?, ?, ?, ?, 'SENDING', 'b1', 't1', 'ONLINE', \
            '2026-07-21T12:00:00Z', '{}', ?, ?, ?)",
    )
    .bind(&doc_bytes)
    .bind(vec![doc_byte ^ 0xFF; 16])
    .bind(fscl)
    .bind(shift_id_bytes)
    .bind(doc_byte as i64)
    .bind(doc_type)
    .bind(vec![0u8; 32])
    .bind(&SEED[..])
    .bind(offline_fiscal_no)
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

/// Seed an ISSUED (`ACK`) `SELL` receipt bound to `shift_id` carrying a single
/// cash payment (`type_code = "0"`, `sum_kop`).  `derive_closing_cash` sums these
/// into `cash_sell_kop` — so an ACK SELL of `sum_kop` shifts the DERIVED closing
/// cash to `opening + sum_kop`, DISTINCT from the opening anchor.  That is what
/// makes the CLOSE-edge cash write (`update_cash_balance_tx`) load-bearing: with
/// no receipt, derived == opening and deleting the write leaves the value
/// unchanged (a false-positive); with this receipt, deleting the write leaves the
/// opening anchor instead of `opening + sum_kop`, so the assert bites.
async fn seed_ack_sell_cash(
    pool: &SqlitePool,
    fscl: &str,
    doc_byte: u8,
    shift_id_bytes: &[u8],
    sum_kop: i64,
) {
    let payload = format!(r#"{{"payments":[{{"type_code":"0","sum_kop":{sum_kop}}}]}}"#);
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, shift_id, lnd, \
            doc_type, state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical) \
         VALUES (?, ?, ?, ?, ?, 'SELL', 'ACK', 'b1', 't1', 'ONLINE', \
            '2026-07-21T11:00:00Z', ?, ?)",
    )
    .bind(vec![doc_byte; 16])
    .bind(vec![doc_byte ^ 0xFF; 16])
    .bind(fscl)
    .bind(shift_id_bytes)
    .bind(doc_byte as i64)
    .bind(&payload)
    .bind(vec![0u8; 32])
    .execute(pool)
    .await
    .expect("seed ACK SELL cash receipt");
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

/// Mint a reservation (CALL_STARTED) and set the node active pointer + generation.
async fn authorize(pool: &SqlitePool, row: NewReservation) {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            delivery_reservation::authorize_submission(tx, row, TS)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("authorize");
}

/// Move the reservation to OUTCOME_OBSERVED + PENDING_APPLY with Accepted evidence (non-empty SFN).
async fn record_accepted(pool: &SqlitePool, res_byte: u8, sfn: &str) {
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state='OUTCOME_OBSERVED', submission_certainty='SUBMITTED', \
             response_provenance='PARSED_DPS_ENVELOPE', routing_class=NULL, \
             apply_state='PENDING_APPLY', node_effect='NoNodeEffect', \
             evidence_kind='Accepted', evidence_text=?, evidence_digest=NULL, \
             remote_correlation_id=? \
         WHERE reservation_id=?",
    )
    .bind(sfn)
    .bind(sfn)
    .bind(&[res_byte; 16][..])
    .execute(pool)
    .await
    .expect("record Accepted PENDING_APPLY");
}

/// Move the reservation to OUTCOME_OBSERVED + PENDING_APPLY with MacReseedPending (-12) evidence.
async fn record_mac_reseed_pending(pool: &SqlitePool, res_byte: u8) {
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state='OUTCOME_OBSERVED', submission_certainty='SUBMITTED', \
             response_provenance='PARSED_DPS_ENVELOPE', routing_class='MacRecovery', \
             apply_state='PENDING_APPLY', node_effect='MacReseedPending', \
             evidence_kind='Rejected', evidence_text='BadHashPrev', evidence_digest=?, \
             remote_correlation_id=NULL \
         WHERE reservation_id=?",
    )
    .bind(&DIG[..])
    .bind(&[res_byte; 16][..])
    .execute(pool)
    .await
    .expect("record MacReseedPending PENDING_APPLY");
}

// ─── readers ──────────────────────────────────────────────────────────────────

async fn read_doc_state(pool: &SqlitePool, doc_byte: u8) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id=?")
        .bind(vec![doc_byte; 16])
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_sfn(pool: &SqlitePool, doc_byte: u8) -> Option<String> {
    sqlx::query_scalar("SELECT server_fiscal_no FROM fiscal_documents WHERE document_id=?")
        .bind(vec![doc_byte; 16])
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_shift_state(pool: &SqlitePool, shift_id: &[u8]) -> String {
    sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id=?")
        .bind(shift_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_node_shift_state(pool: &SqlitePool, fscl: &str) -> String {
    sqlx::query_scalar("SELECT shift_state FROM node_state WHERE fiscal_number=?")
        .bind(fscl)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_apply_state(pool: &SqlitePool, res_byte: u8) -> Option<String> {
    sqlx::query_scalar("SELECT apply_state FROM delivery_reservation WHERE reservation_id=?")
        .bind(&[res_byte; 16][..])
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Read `shifts.cash_balance_kop` (the closing-cash anchor written at the CLOSE edge).
async fn read_shift_cash(pool: &SqlitePool, shift_id: &[u8]) -> i64 {
    sqlx::query_scalar("SELECT cash_balance_kop FROM shifts WHERE shift_id=?")
        .bind(shift_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Count `SHIFT_CONFIRM_EDGE_*` forensic audit rows (NOOP / DRIFT). The HAPPY
/// `Applied` edge emits NONE, so this is 0 after a clean fire. A re-fire on a
/// second boot pass would hit `Conflict{observed=Closed}` → emit
/// `SHIFT_CONFIRM_EDGE_NOOP`, so a non-zero count is the double-fire tell.
async fn count_shift_confirm_edge_audits(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type LIKE 'SHIFT_CONFIRM_EDGE%'")
        .fetch_one(pool)
        .await
        .unwrap()
}

// ─── ao01 — online SHIFT_OPEN Accepted fires shift edge 3 (Opening → Opened) ──

/// Gap-1 proof: `apply_outcome` alone leaves the shift unadvanced; the
/// orchestration fires edge 3 in the SAME `BEGIN IMMEDIATE` as the doc CAS.
#[tokio::test]
async fn ao01_online_shift_open_accepted_fires_edge3_opening_to_opened() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "4001000001";

    // Seed fn_config + node_state FIRST (shifts.fiscal_number FKs fiscal_number_config), then the
    // shift row in Opening state; node_state.current_shift_id has no FK so ordering is fn→shift.
    seed_fn_node(&pool, fscl, "OPENING", Some(&SHIFT)).await;
    seed_shift(&pool, &SHIFT, fscl, "OPENING").await;

    // Seed a SENDING SHIFT_OPEN doc bound to the shift, online origin.
    let doc = seed_sending_doc(&pool, fscl, 0x11, "SHIFT_OPEN", Some(&SHIFT), None).await;

    // Authorize → CALL_STARTED.
    authorize(&pool, new_res(0x01, doc, fscl)).await;

    // Simulate the record-tx: move to OUTCOME_OBSERVED + PENDING_APPLY with Accepted evidence.
    record_accepted(&pool, 0x01, "DPS-SHIFT-001").await;

    // Call the orchestration.
    let res = apply_recorded_outcome(&pool, [0x01; 16])
        .await
        .expect("ao01: Accepted SHIFT_OPEN should resolve");

    // Doc advanced to Sent + SFN stamped.
    assert!(res.applied, "ao01: applied must be true");
    assert_eq!(
        res.server_fiscal_no.as_deref(),
        Some("DPS-SHIFT-001"),
        "ao01: SFN must be stamped"
    );
    assert_eq!(
        read_doc_state(&pool, 0x11).await,
        "SENT",
        "ao01: doc must be Sent"
    );
    assert_eq!(
        read_sfn(&pool, 0x11).await.as_deref(),
        Some("DPS-SHIFT-001"),
        "ao01: SFN in fiscal_documents"
    );

    // GAP-1 PROOF: shift edge fired — shift row advanced Opening → Opened.
    assert_eq!(
        read_shift_state(&pool, &SHIFT).await,
        "OPENED",
        "ao01: gap-1 proof — shift must advance to OPENED (apply_outcome alone would leave it OPENING)"
    );
    // Node state projection also advanced.
    assert_eq!(
        read_node_shift_state(&pool, fscl).await,
        "OPENED",
        "ao01: node_state.shift_state must reflect OPENED"
    );

    // Reservation is APPLIED.
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("APPLIED"),
        "ao01: reservation must be APPLIED"
    );
}

// ─── ao02 — online SELL Accepted fires NO shift edge ──────────────────────────

/// A SELL doc has no shift edge — the orchestration must not attempt one.
/// The doc reaches Sent and the SFN is stamped, but no shift transition is attempted.
#[tokio::test]
async fn ao02_online_sell_accepted_fires_no_shift_edge() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "4001000002";

    // fn_config + node_state FIRST (shifts.fiscal_number FK), then the shift row in Opened state
    // (a SELL happens inside an open shift).
    seed_fn_node(&pool, fscl, "OPENED", Some(&SHIFT)).await;
    seed_shift(&pool, &SHIFT, fscl, "OPENED").await;

    // Seed a SENDING SELL doc bound to the shift, online origin.
    let doc = seed_sending_doc(&pool, fscl, 0x22, "SELL", Some(&SHIFT), None).await;

    authorize(&pool, new_res(0x02, doc, fscl)).await;
    record_accepted(&pool, 0x02, "DPS-SELL-001").await;

    let res = apply_recorded_outcome(&pool, [0x02; 16])
        .await
        .expect("ao02: Accepted SELL should resolve");

    assert!(res.applied, "ao02: applied must be true");
    assert_eq!(
        res.server_fiscal_no.as_deref(),
        Some("DPS-SELL-001"),
        "ao02: SFN stamped"
    );
    assert_eq!(read_doc_state(&pool, 0x22).await, "SENT", "ao02: doc Sent");

    // Shift must remain OPENED — no edge was fired.
    assert_eq!(
        read_shift_state(&pool, &SHIFT).await,
        "OPENED",
        "ao02: shift must remain OPENED after a SELL (no shift edge)"
    );
    assert_eq!(
        read_node_shift_state(&pool, fscl).await,
        "OPENED",
        "ao02: node_state shift_state must remain OPENED"
    );
}

// ─── ao03 — MacReseedPending (-12) HOLD propagates through anyhow boundary ───

/// FIX-C proof: a HOLD (HeldNotAutoRelease) propagates as an anyhow error whose
/// `downcast_ref::<ApplyError>()` is `Some(HeldNotAutoRelease)` — the boot
/// consumer's downcast still classifies the HOLD correctly after the orchestration
/// wraps `apply_outcome`'s error via `map_err(anyhow::Error::from)`.
/// No shift edge is fired (res.applied is false on a HOLD — the apply returns before writes).
#[tokio::test]
async fn ao03_held_outcome_propagates_as_heldnotautorelease() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "4001000003";

    seed_fn_node(&pool, fscl, "OPENED", Some(&SHIFT)).await;
    // Shift not strictly needed for a SELL doc but seed it to make the FK clean.
    seed_shift(&pool, &SHIFT, fscl, "OPENED").await;

    // A SENDING online SELL doc (SHIFT_OPEN would also work — the HOLD gate fires before shift logic).
    let doc = seed_sending_doc(&pool, fscl, 0x33, "SELL", Some(&SHIFT), None).await;

    authorize(&pool, new_res(0x03, doc, fscl)).await;
    // Move to PENDING_APPLY with MacReseedPending (-12) evidence — this is a HOLD.
    record_mac_reseed_pending(&pool, 0x03).await;

    let err = apply_recorded_outcome(&pool, [0x03; 16])
        .await
        .expect_err("ao03: -12 HOLD must propagate as Err");

    // FIX-C: the downcast must classify it as HeldNotAutoRelease.
    let ae = err
        .downcast_ref::<ApplyError>()
        .expect("ao03: anyhow error must carry ApplyError as its source (FIX-C)");
    assert!(
        matches!(ae, ApplyError::HeldNotAutoRelease),
        "ao03: ApplyError must be HeldNotAutoRelease, got {ae:?}"
    );

    // Nothing mutated — doc still Sending, reservation still PENDING_APPLY.
    assert_eq!(
        read_doc_state(&pool, 0x33).await,
        "SENDING",
        "ao03: doc must remain Sending (HOLD — no apply write)"
    );
    assert_eq!(
        read_apply_state(&pool, 0x03).await.as_deref(),
        Some("PENDING_APPLY"),
        "ao03: reservation must remain PENDING_APPLY"
    );
    // Shift must remain OPENED — no edge was attempted.
    assert_eq!(
        read_shift_state(&pool, &SHIFT).await,
        "OPENED",
        "ao03: shift must remain OPENED (no edge on HOLD)"
    );
}

// ─── ao04 — the BOOT reservation pass fires the shift edge via orchestration (B3) ──

/// B3 regression tooth (re-audit CRITICAL/MAJOR — external F1 + internal B3). A crash between
/// PHASE-3 record(Accepted) and PHASE-4 apply leaves an online `SHIFT_OPEN` reservation resting
/// `OUTCOME_OBSERVED + PENDING_APPLY`. The boot reservation pass MUST resolve it THROUGH
/// `apply_orchestration::apply_recorded_outcome` (edge 3 `Opening → Opened` fires in the apply tx),
/// NOT the bare repo `apply_outcome` (which advances the doc/SFN/seed but SKIPS the shift edge,
/// stranding the shift in `OPENING` until branch-e2 destructively orphans a DPS-accepted shift).
///
/// Pre-B3 (`apply_one` called `delivery_reservation::apply_outcome` directly) this asserts RED:
/// doc reaches Sent but shift rests OPENING. Post-B3 (routed through the orchestration) → OPENED.
#[tokio::test]
async fn ao04_boot_pass_shift_open_accepted_fires_edge3_via_orchestration() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "4001000004";

    seed_fn_node(&pool, fscl, "OPENING", Some(&SHIFT)).await;
    seed_shift(&pool, &SHIFT, fscl, "OPENING").await;

    // Online SHIFT_OPEN doc, authorized then recorded Accepted (the record tx committed, then crash).
    let doc = seed_sending_doc(&pool, fscl, 0x44, "SHIFT_OPEN", Some(&SHIFT), None).await;
    authorize(&pool, new_res(0x04, doc, fscl)).await;
    record_accepted(&pool, 0x04, "DPS-SHIFT-004").await;

    // Drive the PRODUCTION boot reservation pass (the resume path after a crash).
    let guard = ReconcileGuard::for_integration_test_only();
    reservation_boot_pass::run(&guard, &pool)
        .await
        .expect("ao04: boot reservation pass");

    // Doc converged exactly as the live path would.
    assert_eq!(
        read_doc_state(&pool, 0x44).await,
        "SENT",
        "ao04: boot apply must advance the doc to Sent"
    );
    assert_eq!(
        read_sfn(&pool, 0x44).await.as_deref(),
        Some("DPS-SHIFT-004"),
        "ao04: SFN stamped by boot apply"
    );

    // B3 PROOF: the shift edge fired on the BOOT path (not just the live path).
    assert_eq!(
        read_shift_state(&pool, &SHIFT).await,
        "OPENED",
        "ao04 (B3): boot MUST fire edge 3 via apply_orchestration — bare apply_outcome leaves OPENING"
    );
    assert_eq!(
        read_node_shift_state(&pool, fscl).await,
        "OPENED",
        "ao04 (B3): node_state.shift_state must reflect OPENED after boot apply"
    );
    assert_eq!(
        read_apply_state(&pool, 0x04).await.as_deref(),
        Some("APPLIED"),
        "ao04: reservation APPLIED after boot pass"
    );
}

// ─── ao05 — boot pass fires shift CLOSE edge (Closing → Closed) for SHIFT_CLOSE ──

/// B3 completeness tooth (re-audit) — the SHIFT_CLOSE analogue of ao04. A crash
/// between record(Accepted) and apply leaves an online `SHIFT_CLOSE` reservation
/// resting `OUTCOME_OBSERVED + PENDING_APPLY`. The boot reservation pass MUST
/// resolve it THROUGH `apply_orchestration::apply_recorded_outcome`, firing the
/// online CLOSE edge (`Closing → Closed`, `edge10_close`) in the apply tx AND
/// persisting the derived closing cash — NOT the bare repo `apply_outcome`
/// (which advances the doc/SFN/seed but SKIPS the shift edge, stranding the
/// shift in `CLOSING`).
///
/// The closing-cash write is made LOAD-BEARING (re-audit MINOR — the old form
/// seeded opening == expected closing, so deleting `update_cash_balance_tx` was a
/// false-positive no-op): the shift carries an ISSUED (`ACK`) SELL of `20000 kop`
/// cash, so the INDEPENDENTLY-derived closing cash is `opening(12345) +
/// cash_sell(20000) = 32345` — DISTINCT from the opening anchor.  The CLOSE path
/// must OVERWRITE the opening sentinel with this derived value; deleting the write
/// leaves the shift at `12345` and the assert goes RED.
///
/// The SECOND boot pass is a NO-OP (multi-boot idempotency, re-audit MINOR):
/// the reservation is APPLIED so it is no longer listed; the shift stays CLOSED,
/// the closing cash is unchanged, and NO `SHIFT_CONFIRM_EDGE_*` audit row is
/// emitted (a re-fire would hit the NOOP/DRIFT branch and leave a row).
#[tokio::test]
async fn ao05_boot_pass_shift_close_accepted_fires_edge_closing_to_closed() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "4001000005";

    // Shift in CLOSING with a non-zero opening anchor.  A seeded ACK SELL of
    // 20000 kop cash moves the DERIVED closing cash to 12345 + 20000 = 32345,
    // DISTINCT from the opening anchor — so the CLOSE-edge cash write is
    // load-bearing (deleting it leaves 12345, not 32345).
    seed_fn_node(&pool, fscl, "CLOSING", Some(&SHIFT)).await;
    seed_shift_with_cash(&pool, &SHIFT, fscl, "CLOSING", 12345).await;
    // ISSUED cash SELL in the shift (lnd 0x5C, distinct from the close doc 0x55).
    seed_ack_sell_cash(&pool, fscl, 0x5c, &SHIFT, 20000).await;

    // Online SHIFT_CLOSE doc, authorized then recorded Accepted (record tx committed, then crash).
    let doc = seed_sending_doc(&pool, fscl, 0x55, "SHIFT_CLOSE", Some(&SHIFT), None).await;
    authorize(&pool, new_res(0x05, doc, fscl)).await;
    record_accepted(&pool, 0x05, "DPS-CLOSE-005").await;

    // ── First boot pass — the crash-resume ──────────────────────────────────
    let guard = ReconcileGuard::for_integration_test_only();
    reservation_boot_pass::run(&guard, &pool)
        .await
        .expect("ao05: first boot reservation pass");

    // Doc converged: Sent + SFN stamped.
    assert_eq!(
        read_doc_state(&pool, 0x55).await,
        "SENT",
        "ao05: boot apply must advance the SHIFT_CLOSE doc to Sent"
    );
    assert_eq!(
        read_sfn(&pool, 0x55).await.as_deref(),
        Some("DPS-CLOSE-005"),
        "ao05: SFN stamped by boot apply"
    );

    // B3 PROOF: the CLOSE edge fired on the BOOT path.
    assert_eq!(
        read_shift_state(&pool, &SHIFT).await,
        "CLOSED",
        "ao05 (B3): boot MUST fire the CLOSE edge via apply_orchestration — bare apply_outcome leaves CLOSING"
    );
    assert_eq!(
        read_node_shift_state(&pool, fscl).await,
        "CLOSED",
        "ao05 (B3): node_state.shift_state must reflect CLOSED after boot apply"
    );
    // Closing cash persisted: the INDEPENDENTLY-derived value (opening 12345 +
    // cash_sell 20000 = 32345), NOT the opening sentinel — this is the
    // load-bearing assertion (deleting update_cash_balance_tx leaves 12345).
    assert_eq!(
        read_shift_cash(&pool, &SHIFT).await,
        32345,
        "ao05: CLOSE path must persist the DERIVED closing cash (opening 12345 + \
         cash_sell 20000), overwriting the opening sentinel"
    );
    assert_eq!(
        read_apply_state(&pool, 0x05).await.as_deref(),
        Some("APPLIED"),
        "ao05: reservation APPLIED after boot pass"
    );
    // Happy Applied edge emits NO forensic audit row.
    assert_eq!(
        count_shift_confirm_edge_audits(&pool).await,
        0,
        "ao05: happy CLOSE edge emits no SHIFT_CONFIRM_EDGE audit row"
    );

    // ── Second boot pass — MUST be a NO-OP (multi-boot idempotency) ──────────
    reservation_boot_pass::run(&guard, &pool)
        .await
        .expect("ao05: second boot reservation pass (idempotent no-op)");

    assert_eq!(
        read_shift_state(&pool, &SHIFT).await,
        "CLOSED",
        "ao05: shift stays CLOSED after a second boot pass"
    );
    assert_eq!(
        read_node_shift_state(&pool, fscl).await,
        "CLOSED",
        "ao05: node_state.shift_state stays CLOSED after a second boot pass"
    );
    assert_eq!(
        read_apply_state(&pool, 0x05).await.as_deref(),
        Some("APPLIED"),
        "ao05: reservation stays APPLIED after a second boot pass"
    );
    assert_eq!(
        read_shift_cash(&pool, &SHIFT).await,
        32345,
        "ao05: closing cash unchanged after a second boot pass"
    );
    // NO second edge fire: a re-fire would emit a SHIFT_CONFIRM_EDGE_NOOP row.
    assert_eq!(
        count_shift_confirm_edge_audits(&pool).await,
        0,
        "ao05: second boot pass must not re-fire the CLOSE edge (no SHIFT_CONFIRM_EDGE audit)"
    );
}

// ─── ao06 — boot pass fires shift CLOSE edge for a Z_REPORT ─────────────────────

/// B3 completeness tooth (re-audit) — the Z_REPORT analogue. Per
/// `apply_orchestration::apply_recorded_outcome`, `Z_REPORT` and `SHIFT_CLOSE`
/// share the SAME online close edge (`Closing → Closed`, `edge10_close`) with
/// closing-cash derivation. This pins that a crash-resumed online `Z_REPORT`
/// reservation fires that close edge via the boot pass — not the bare
/// `apply_outcome` (which would strand the shift in `CLOSING`). Same
/// second-pass idempotency assertions as ao05.
#[tokio::test]
async fn ao06_boot_pass_z_report_accepted_fires_edge_closing_to_closed() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "4001000006";

    seed_fn_node(&pool, fscl, "CLOSING", Some(&SHIFT)).await;
    seed_shift_with_cash(&pool, &SHIFT, fscl, "CLOSING", 6789).await;
    // ISSUED cash SELL of 10000 kop → derived closing = 6789 + 10000 = 16789,
    // DISTINCT from the opening anchor (lnd 0x6C, distinct from the Z doc 0x66).
    seed_ack_sell_cash(&pool, fscl, 0x6c, &SHIFT, 10000).await;

    // Online Z_REPORT doc, authorized then recorded Accepted (record committed, then crash).
    let doc = seed_sending_doc(&pool, fscl, 0x66, "Z_REPORT", Some(&SHIFT), None).await;
    authorize(&pool, new_res(0x06, doc, fscl)).await;
    record_accepted(&pool, 0x06, "DPS-ZREP-006").await;

    // ── First boot pass ─────────────────────────────────────────────────────
    let guard = ReconcileGuard::for_integration_test_only();
    reservation_boot_pass::run(&guard, &pool)
        .await
        .expect("ao06: first boot reservation pass");

    assert_eq!(
        read_doc_state(&pool, 0x66).await,
        "SENT",
        "ao06: boot apply must advance the Z_REPORT doc to Sent"
    );
    assert_eq!(
        read_sfn(&pool, 0x66).await.as_deref(),
        Some("DPS-ZREP-006"),
        "ao06: SFN stamped by boot apply"
    );

    // B3 PROOF: the CLOSE edge fired for a Z_REPORT on the BOOT path.
    assert_eq!(
        read_shift_state(&pool, &SHIFT).await,
        "CLOSED",
        "ao06 (B3): boot MUST fire the CLOSE edge for a Z_REPORT — bare apply_outcome leaves CLOSING"
    );
    assert_eq!(
        read_node_shift_state(&pool, fscl).await,
        "CLOSED",
        "ao06 (B3): node_state.shift_state must reflect CLOSED after boot apply"
    );
    // Load-bearing: the DERIVED closing (opening 6789 + cash_sell 10000 = 16789),
    // NOT the opening sentinel — deleting update_cash_balance_tx leaves 6789.
    assert_eq!(
        read_shift_cash(&pool, &SHIFT).await,
        16789,
        "ao06: Z_REPORT close path must persist the DERIVED closing cash (opening \
         6789 + cash_sell 10000), overwriting the opening sentinel"
    );
    assert_eq!(
        read_apply_state(&pool, 0x06).await.as_deref(),
        Some("APPLIED"),
        "ao06: reservation APPLIED after boot pass"
    );
    assert_eq!(
        count_shift_confirm_edge_audits(&pool).await,
        0,
        "ao06: happy Z_REPORT close edge emits no SHIFT_CONFIRM_EDGE audit row"
    );

    // ── Second boot pass — MUST be a NO-OP (multi-boot idempotency) ──────────
    reservation_boot_pass::run(&guard, &pool)
        .await
        .expect("ao06: second boot reservation pass (idempotent no-op)");

    assert_eq!(
        read_shift_state(&pool, &SHIFT).await,
        "CLOSED",
        "ao06: shift stays CLOSED after a second boot pass"
    );
    assert_eq!(
        read_node_shift_state(&pool, fscl).await,
        "CLOSED",
        "ao06: node_state.shift_state stays CLOSED after a second boot pass"
    );
    assert_eq!(
        read_apply_state(&pool, 0x06).await.as_deref(),
        Some("APPLIED"),
        "ao06: reservation stays APPLIED after a second boot pass"
    );
    assert_eq!(
        read_shift_cash(&pool, &SHIFT).await,
        16789,
        "ao06: closing cash unchanged after a second boot pass"
    );
    assert_eq!(
        count_shift_confirm_edge_audits(&pool).await,
        0,
        "ao06: second boot pass must not re-fire the CLOSE edge (no SHIFT_CONFIRM_EDGE audit)"
    );
}
