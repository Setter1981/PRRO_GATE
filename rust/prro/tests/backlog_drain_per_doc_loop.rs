//! W9b Commit 4 — `backlog_drain::drain` per-doc loop + Manual-
//! escalation on pending-drain shift reject.
//!
//! **M3b W12 Commit 4b.3 update (2026-05-22)**: W9b C4 inline
//! `Sent → Kvt1` stub replaced by W12 SentFresh chain
//! (`process_via_stage_send` → `kvt2_confirm::confirm_drain_doc(
//! SentFresh, ...)` → Envelope 1a + Envelope 2 → ACK).  The 6
//! pre-W12 c4_* stub-locking fixtures were refactored to W12
//! ACK-era assertions; 3 new `w12_sent_fresh_*` integration
//! fixtures added (NotFound→Drift / Mismatch→Drift / Transport→Hold).
//!
//! Acceptance:
//!   - Spec §2.3 Step B + §2.5: invoke `stage_send::run` per doc in
//!     strict `lnd ASC`; route outcomes via private helpers.  Post
//!     W12: success path emits `OFFLINE_DRAIN_KVT2_ADVANCED` +
//!     `STAGE_FINALIZE_ACK` (Envelope 1a + Envelope 2); pre-W12
//!     `OFFLINE_DRAIN_DOC_ADVANCED` no longer fires from SentFresh
//!     path.  `OFFLINE_DRAIN_DOC_FAILED` unchanged for stage_send
//!     failure paths.  Sibling-continue applies ONLY to
//!     non-pending-drain shifts; pending-drain reject halts the
//!     drain + transitions shift to `RequiresManualReconciliation`
//!     per `LEGAL_INVARIANTS.md` §INV-19 +
//!     `m3b-shift-state-expansion.md` §6.3.
//!   - W12 SentFresh non-Acked paths emit `KVT2_CONFIRM_HOLD`
//!     (Severity::Warning) or `KVT2_CONFIRM_STRUCTURAL_DRIFT`
//!     (Severity::Error) audit-only envelope BEFORE BootError::
//!     Internal fail-loud per plan §311 MED-PR70-R12-01.
//!
//! Tests (10):
//!
//!   1. `c4_happy_path_two_docs_advance_to_ack_via_w12` (renamed)
//!   2. `c4_routed_terminal_reject_records_wire_routing_failure_class`
//!   3. `c4_signer_refused_records_signer_refused_class_and_escalates_manual`
//!   4. `c4_processes_backlog_in_lnd_asc_order`
//!   5. `c4_accounting_advanced_plus_failures_equals_backlog`
//!   6. `c4_pending_drain_shift_reject_halts_and_transitions_shift_to_manual`
//!   7. `c4_pending_drain_shift_transient_retry_halts_chain_this_tick_no_manual`
//!   8. `w12_sent_fresh_not_found_emits_drift_audit_and_halts_via_boot_error`
//!   9. `w12_sent_fresh_mismatch_emits_drift_audit_and_halts_via_boot_error`
//!  10. `w12_sent_fresh_dps_transport_holds_drain_and_projects_held_at_sent`
//!      (refactored 2026-05-24 W12 Commit 6: replaces BootError-halt
//!      assertion з HoldFnDrain projection + held_at_sent==1 per
//!      MED-PR70-R7-02 projection matrix)

mod common;

use std::sync::Arc;

use prro::db::models::enums::{NodeMode, OfflineSessionState, ShiftState};
use prro::db::models::ids::{DocumentId, OfflineSessionId, ShiftId};
use prro::services::offline_sync::backlog_drain;
use prro::services::reconciliation::runtime::RuntimeView;
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::dto::{CheckAck, CheckSignBlob};
use prro::transports::dps::error::{AuthorizationKind, DpsError};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use uuid::Uuid;

use common::{det_signing_ctx, StubDpsChannel};

const FN: &str = "1234567890";
const TAX_NUMBER: &str = "12345678";
const CASHIER_OK: &str = "test-cashier";
const CASHIER_OTHER: &str = "different-cashier";

// ─── Fixture builders ────────────────────────────────────────────────

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("w9b_c4.db"))
        .await
        .expect("open_pool runs migrations");
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, ?, 'test')",
    )
    .bind(FN)
    .bind(TAX_NUMBER)
    .execute(&pool)
    .await
    .unwrap();
    (dir, pool)
}

async fn seed_node_state(pool: &SqlitePool, mode: NodeMode, shift: ShiftState) {
    sqlx::query(
        "INSERT INTO node_state(fiscal_number, mode, shift_state, next_lnd) \
         VALUES (?, ?, ?, 100)",
    )
    .bind(FN)
    .bind(mode)
    .bind(shift)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_open_shift(pool: &SqlitePool, cashier_id: &str) -> ShiftId {
    seed_shift_with_state(pool, cashier_id, "OPENED").await
}

/// W14a-1 widening: tests can seed shifts in OPENED_LOCAL_PENDING_DRAIN
/// or CLOSING_LOCAL_PENDING_DRAIN to exercise the pending-drain halt
/// path (spec amendment 2026-05-21 + LEGAL_INVARIANTS §INV-19).
async fn seed_shift_with_state(pool: &SqlitePool, cashier_id: &str, state: &str) -> ShiftId {
    let shift_id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts(shift_id, fiscal_number, serial, state, \
            open_mode, cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, ?, 'OFFLINE', 0, ?)",
    )
    .bind(shift_id)
    .bind(FN)
    .bind(state)
    .bind(cashier_id)
    .execute(pool)
    .await
    .unwrap();
    shift_id
}

/// W9b prereq: drain reads `node_state.current_shift_id` to identify
/// the shift to escalate on pending-drain reject.  Tests that exercise
/// the halt path need to backfill this column after seeding the shift.
async fn set_node_current_shift(pool: &SqlitePool, shift_id: ShiftId) {
    sqlx::query("UPDATE node_state SET current_shift_id = ? WHERE fiscal_number = ?")
        .bind(shift_id)
        .bind(FN)
        .execute(pool)
        .await
        .unwrap();
}

async fn read_shift_state(pool: &SqlitePool, shift_id: ShiftId) -> String {
    sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
        .bind(shift_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_node_shift_state(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT shift_state FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_offline_session(pool: &SqlitePool, state: OfflineSessionState) -> OfflineSessionId {
    let session_id = OfflineSessionId::new();
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, ?, '2026-05-20T00:00:00Z')",
    )
    .bind(session_id)
    .bind(FN)
    .bind(state.as_str())
    .execute(pool)
    .await
    .unwrap();
    session_id
}

/// Seed a fully-formed OFFLINE_LOCAL_ACK doc — covers the W7a
/// invariants, a SIGNED_XML artifact, and a consumed offline_codes
/// row.  Returns the document_id for the caller to assert against.
///
/// `signer_cashier` is the cashier id stamped on the doc; the test
/// passes either `CASHIER_OK` (matches the shift's opener → signer
/// guard passes) or `CASHIER_OTHER` (mismatch → SignerRefused).
async fn seed_complete_offline_local_ack(
    pool: &SqlitePool,
    lnd: i64,
    code_lnd: i64,
    session_id: OfflineSessionId,
    shift_id: ShiftId,
    signer_cashier: &str,
) -> DocumentId {
    let doc_id = DocumentId::new();
    let req_id = Uuid::now_v7();
    let sha = vec![0u8; 32];
    // B8: offline_dps_code required so fail-closed drain guard passes.
    let dps_code = format!("DRAIN-LOOP-{code_lnd}");
    sqlx::query(
        "INSERT INTO fiscal_documents( \
            document_id, request_id, fiscal_number, shift_id, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, signed_by_cashier_id, \
            offline_session_id, offline_fiscal_no, offline_fiscal_date, offline_dps_code \
         ) VALUES ( \
            ?, ?, ?, ?, ?, 'SELL', 'OFFLINE_LOCAL_ACK', \
            'b', 't', 'OFFLINE', '2026-05-20T00:00:00Z', \
            '{}', ?, ?, \
            ?, ?, '2026-05-20T00:00:00Z', ? \
         )",
    )
    .bind(doc_id)
    .bind(req_id.as_bytes().to_vec())
    .bind(FN)
    .bind(shift_id)
    .bind(lnd)
    .bind(&sha)
    .bind(signer_cashier)
    .bind(session_id)
    .bind(code_lnd)
    .bind(&dps_code)
    .execute(pool)
    .await
    .unwrap();

    // SIGNED_XML artifact (stage_send 4-pre reads this).
    sqlx::query(
        "INSERT INTO document_files(document_id, kind, content) \
         VALUES (?, 'SIGNED_XML', ?)",
    )
    .bind(doc_id)
    .bind(b"FAKE-CMS-SIGNED-PAYLOAD".to_vec())
    .execute(pool)
    .await
    .unwrap();

    // Consumed offline_codes row (W7a marker; not strictly needed by
    // stage_send drain path but mirrors production shape and avoids
    // any future N+1 read surprises).
    sqlx::query(
        "INSERT INTO offline_codes(fiscal_number, code_lnd, consumed_at, consumed_by_document_id) \
         VALUES (?, ?, '2026-05-20T00:00:01Z', ?)",
    )
    .bind(FN)
    .bind(code_lnd)
    .bind(doc_id)
    .execute(pool)
    .await
    .unwrap();

    doc_id
}

async fn read_doc_state(pool: &SqlitePool, doc_id: DocumentId) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(doc_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn audit_count(pool: &SqlitePool, event_type: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type = ?")
        .bind(event_type)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn audit_payloads_for(pool: &SqlitePool, event_type: &str) -> Vec<serde_json::Value> {
    let raw: Vec<Option<String>> = sqlx::query_scalar(
        "SELECT event_payload_json FROM audit_log \
         WHERE event_type = ? \
         ORDER BY audit_id ASC",
    )
    .bind(event_type)
    .fetch_all(pool)
    .await
    .unwrap();
    let raw: Vec<String> = raw.into_iter().flatten().collect();
    raw.into_iter()
        .map(|s| serde_json::from_str(&s).unwrap())
        .collect()
}

fn fn_sign() -> CheckSignBlob {
    CheckSignBlob(vec![0xAB, 0xCD])
}

fn ack(id: &str) -> CheckAck {
    CheckAck {
        id: id.into(),
        id_sign: vec![],
        data_sign: vec![],
    }
}

/// **M3b W12 Commit 4b.3 (2026-05-22)** — lastChk-style response with
/// non-empty `data_sign` evidence (required for `Acked` path per
/// `classify_check_result`; empty bytes route to `Hold(LastChkData
/// SignEmpty)`).  Use deterministic `[0xAB; 32]` bytes per fixture
/// (operator dashboards see `kvt1_raw_sha256_hex` in audit so the
/// exact bytes are operator-visible cross-correlation anchor).
fn last_chk_ack(id: &str, data_sign: Vec<u8>) -> CheckAck {
    CheckAck {
        id: id.into(),
        id_sign: vec![],
        data_sign,
    }
}

/// **M3b W12 Commit 4b.3 (2026-05-22)** — deterministic non-empty
/// KVT1_RAW evidence bytes for lastChk Acked path tests.  Length 32
/// matches real DPS protobuf KVT1_RAW shape closely enough for
/// SHA256 digest assertions in `OFFLINE_DRAIN_KVT2_ADVANCED`
/// audit payload.
fn kvt1_raw_bytes_for(server_fiscal_no: &str) -> Vec<u8> {
    // Discriminator-derived 32-byte pattern: first 4 bytes from
    // server_fiscal_no hash-like seed, rest 0xAB padding.  Each FN
    // gets distinguishable bytes so dashboards can cross-correlate
    // `kvt1_raw_sha256_hex` per-doc.
    let mut bytes = vec![0xABu8; 32];
    for (i, b) in server_fiscal_no.bytes().take(4).enumerate() {
        bytes[i] = b;
    }
    bytes
}

struct DepsCarriers {
    dps: Arc<StubDpsChannel>,
    signing_ctx: SigningContext,
    fn_sign: CheckSignBlob,
}

fn carriers_with_responses(responses: Vec<Result<CheckAck, DpsError>>) -> DepsCarriers {
    DepsCarriers {
        dps: Arc::new(StubDpsChannel::with_queue(responses)),
        signing_ctx: det_signing_ctx(),
        fn_sign: fn_sign(),
    }
}

/// **M3b W12 Commit 4b.3 (2026-05-22)** — DepsCarriers builder that
/// seeds BOTH `send_chk` and `last_chk` response queues.  Required
/// for Sent-source W12 happy-path tests: each doc that reaches
/// `StageSendOutcome::Sent` triggers a downstream lastChk DPS call
/// via `confirm_drain_doc(SentFresh, ...)`.  Tests should queue one
/// `last_chk_ack(server_fiscal_no, kvt1_raw_bytes)` per Sent doc.
fn carriers_with_responses_and_last_chk(
    send_chk: Vec<Result<CheckAck, DpsError>>,
    last_chk: Vec<Result<CheckAck, DpsError>>,
) -> DepsCarriers {
    DepsCarriers {
        dps: Arc::new(StubDpsChannel::with_queue(send_chk).with_last_chk_queue(last_chk)),
        signing_ctx: det_signing_ctx(),
        fn_sign: fn_sign(),
    }
}

fn view_for<'a>(carriers: &'a DepsCarriers) -> RuntimeView<'a> {
    RuntimeView {
        dps: carriers.dps.as_ref(),
        signing_ctx: &carriers.signing_ctx,
        fn_sign: &carriers.fn_sign,
    }
}

// ─── Test 1: happy path — 2 docs advance ─────────────────────────────

#[tokio::test]
async fn c4_happy_path_two_docs_advance_to_ack_via_w12() {
    // **M3b W12 Commit 4b.3 (2026-05-22)** — refactored from
    // pre-W12 stub-locking `c4_happy_path_two_docs_advance_to_kvt1_and_
    // emit_doc_advanced`.  Post-Sent path now flows through
    // `kvt2_confirm::confirm_drain_doc(SentFresh, ...)` →
    // Envelope 1a (Kvt1Raw + Sent→Kvt1 + Kvt1→Kvt2 + KVT2_ADVANCED
    // audit) → Envelope 2 (`stage_finalize::run` Kvt2→Ack).
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool, CASHIER_OK).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let doc_a =
        seed_complete_offline_local_ack(&pool, 1, 100, session_id, shift_id, CASHIER_OK).await;
    let doc_b =
        seed_complete_offline_local_ack(&pool, 2, 101, session_id, shift_id, CASHIER_OK).await;
    // M2-01 (Fable-locked fix-order): seed the REAL same-session offline
    // chain.  The real sign→offline-ack path does NOT advance the MAC seed
    // until M2-01 lands, so BOTH offline docs of one session sign the SAME
    // `previous_hash` (the session-entry seed) — NOT a per-doc chain.  Both
    // docs therefore carry `prev = chain_anchor(0x00)`; their
    // `unsigned_xml_sha256` differ (0x01, 0x02).  Against pre-M2-01 code this
    // is RED (doc B fails `stage_finalize` ChainSeedMismatch once doc A's
    // finalize advances the seed); post-fix `stage_finalize` skips the
    // chain-continuity guard for offline-origin docs, so both reach ACK.
    common::init_chain_seed(&pool, FN, common::chain_anchor(0x00))
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc_a,
        common::chain_anchor(0x00),
        common::chain_anchor(0x01),
    )
    .await
    .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc_b,
        common::chain_anchor(0x00),
        common::chain_anchor(0x02),
    )
    .await
    .unwrap();

    // send_chk × 2 (stage_send Ok→Sent for each doc) + last_chk × 2
    // (confirm_drain_doc(SentFresh) lookup for each doc).
    let carriers = carriers_with_responses_and_last_chk(
        vec![Ok(ack("DPS-FN-A")), Ok(ack("DPS-FN-B"))],
        vec![
            Ok(last_chk_ack("DPS-FN-A", kvt1_raw_bytes_for("DPS-FN-A"))),
            Ok(last_chk_ack("DPS-FN-B", kvt1_raw_bytes_for("DPS-FN-B"))),
        ],
    );
    let view = view_for(&carriers);

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(summary.backlog_size_before(), 2);
    assert_eq!(
        summary.advanced_to_ack(),
        2,
        "W12 SentFresh → both docs reach Ack via Envelope 1a + Envelope 2"
    );
    assert_eq!(summary.advanced_to_kvt1(), 0, "no DeferredKvt1 post-W12");
    assert!(summary.per_doc_failures().is_empty());

    // Both docs reached ACK (stage_send Sent → confirm_drain_doc
    // SentFresh-Acked → Envelope 1a Sent→Kvt1→Kvt2 → Envelope 2
    // stage_finalize Kvt2→Ack).
    assert_eq!(read_doc_state(&pool, doc_a).await, "ACK");
    assert_eq!(read_doc_state(&pool, doc_b).await, "ACK");

    // W12 audit chain — per doc: 1 OFFLINE_DRAIN_KVT2_ADVANCED
    // (Envelope 1a) + 1 STAGE_FINALIZE_ACK (Envelope 2).  Pre-W12
    // OFFLINE_DRAIN_DOC_ADVANCED no longer fires from the SentFresh
    // path.
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await,
        2,
        "Envelope 1a fires once per doc"
    );
    assert_eq!(
        audit_count(&pool, "STAGE_FINALIZE_ACK").await,
        2,
        "Envelope 2 stage_finalize::run fires once per doc"
    );
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_DOC_ADVANCED").await,
        0,
        "pre-W12 stub audit MUST NOT fire post-W12 wiring"
    );
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_FAILED").await, 0);
    assert_eq!(carriers.dps.call_count(), 2, "send_chk × 2");
    assert_eq!(
        carriers.dps.last_chk_call_count(),
        2,
        "lastChk × 2 (confirm_drain_doc SentFresh per doc)"
    );

    // Envelope 1a payload sanity — plan §62-65 pinned shape.
    let advanced_payloads = audit_payloads_for(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await;
    assert_eq!(advanced_payloads.len(), 2);
    assert_eq!(advanced_payloads[0]["from_state"], "OFFLINE_LOCAL_ACK");
    assert_eq!(advanced_payloads[0]["to_state"], "KVT2");
    assert_eq!(advanced_payloads[0]["dispatch_via"], "kvt2_confirm");
    assert_eq!(advanced_payloads[0]["evidence_source"], "lastChk");
    assert_eq!(advanced_payloads[0]["server_fiscal_no"], "DPS-FN-A");
    // **LOW-W12C4B3-B fix (4b.3 Δ, 2026-05-22)**: assert digest
    // value matches actual `kvt1_raw_bytes_for("DPS-FN-A")` bytes
    // rather than just `is_string()` presence check.  Locks the
    // plan-pinned MED-W12C4A-A audit-digest contract end-to-end
    // (computation algorithm + byte ordering + hex format).
    let expected_a_hex = format!("{:x}", Sha256::digest(kvt1_raw_bytes_for("DPS-FN-A")));
    let expected_b_hex = format!("{:x}", Sha256::digest(kvt1_raw_bytes_for("DPS-FN-B")));
    assert_eq!(advanced_payloads[0]["kvt1_raw_sha256_hex"], expected_a_hex);
    assert_eq!(advanced_payloads[1]["kvt1_raw_sha256_hex"], expected_b_hex);
    // attempt_no = 1 (first stage_send wire attempt per
    // mark_submission_attempted_tx counter contract).
    assert_eq!(advanced_payloads[0]["attempt_no"], 1);
    assert_eq!(advanced_payloads[1]["attempt_no"], 1);
}

// ─── Test 2: routed terminal reject ──────────────────────────────────

#[tokio::test]
async fn c4_routed_terminal_reject_records_wire_routing_failure_class() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool, CASHIER_OK).await;
    // Strict-sequential offline drain (architect ruling B, 2026-06-13): a
    // terminal reject now HALTS + escalates Manual even on a PLAIN Opened shift
    // (whitelist edge 15).  escalate_drain_to_manual reads
    // node_state.current_shift_id, so backfill it after seeding the shift.
    set_node_current_shift(&pool, shift_id).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let _doc =
        seed_complete_offline_local_ack(&pool, 1, 100, session_id, shift_id, CASHIER_OK).await;

    // Authorization{DocumentReject} → RetryClass::TerminalReject →
    // FailureClass::WireRoutingTerminalReject.
    let carriers = carriers_with_responses(vec![Err(DpsError::Authorization {
        code: -1,
        kind: AuthorizationKind::DocumentReject,
        message: "signature_invalid".into(),
    })]);
    let view = view_for(&carriers);

    // drain() STILL returns Ok(summary): escalate_drain_to_manual is Ok.
    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(summary.backlog_size_before(), 1);
    assert_eq!(summary.advanced_to_kvt1(), 0);
    assert_eq!(summary.per_doc_failures().len(), 1);
    assert_eq!(
        summary.per_doc_failures()[0].1,
        "wire_routing_terminal_reject",
        "Authorization{{DocumentReject}} must map via RetryClass::TerminalReject"
    );

    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_ADVANCED").await, 0);
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_FAILED").await, 1);

    let failed_payloads = audit_payloads_for(&pool, "OFFLINE_DRAIN_DOC_FAILED").await;
    assert_eq!(
        failed_payloads[0]["failure_class"],
        "wire_routing_terminal_reject"
    );
    assert_eq!(failed_payloads[0]["retry_class"], "TerminalReject");

    // Strict-sequential: terminal reject HALTS + escalates the FN shift to
    // RequiresManualReconciliation even on a plain Opened shift (edge 15), and
    // emits the Critical halt audit exactly once.
    assert_eq!(
        read_shift_state(&pool, shift_id).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "terminal reject MUST escalate even a plain Opened shift → Manual"
    );
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await,
        1,
        "the halt emits the Critical escalation audit"
    );
}

// ─── Test 3: signer refused; terminal HALT + escalate Manual ─────────

#[tokio::test]
async fn c4_signer_refused_records_signer_refused_class_and_escalates_manual() {
    // Strict-sequential offline drain (architect ruling B, 2026-06-13):
    // SignerRefused is a TERMINAL (manual_recon:true) failure.  The old
    // "sibling_continues" behavior is now WRONG — doc A's terminal failure
    // HALTS the chain (doc B with higher lnd is NOT processed) and escalates
    // the FN shift to RequiresManualReconciliation even on a plain Opened shift
    // (whitelist edge 15).
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool, CASHIER_OK).await;
    // escalate_drain_to_manual reads node_state.current_shift_id; backfill it.
    set_node_current_shift(&pool, shift_id).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    // Doc A: cashier mismatch → SignerRefused (terminal).  Doc B: cashier
    // matches but is NOT processed — the chain halts at doc A.
    let doc_a =
        seed_complete_offline_local_ack(&pool, 1, 100, session_id, shift_id, CASHIER_OTHER).await;
    let doc_b =
        seed_complete_offline_local_ack(&pool, 2, 101, session_id, shift_id, CASHIER_OK).await;
    // M3b W12 Commit 4b.3 chain seed for doc B is retained but unused: doc A's
    // terminal halt means doc B never reaches the wire / finalize.
    common::init_chain_seed(&pool, FN, common::chain_anchor(0x00))
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc_b,
        common::chain_anchor(0x00),
        common::chain_anchor(0x01),
    )
    .await
    .unwrap();

    // Doc A is rejected by the signer guard BEFORE any wire call, then the
    // chain HALTS — so NO wire response is consumed (doc B never reaches the
    // wire).  Spare responses are queued only to keep the stub from
    // underflowing if the loop unexpectedly continues.
    let carriers = carriers_with_responses_and_last_chk(
        vec![Ok(ack("DPS-FN-B-UNREACHED"))],
        vec![Ok(last_chk_ack(
            "DPS-FN-B-UNREACHED",
            kvt1_raw_bytes_for("DPS-FN-B"),
        ))],
    );
    let view = view_for(&carriers);

    // drain() STILL returns Ok(summary): escalate_drain_to_manual is Ok.
    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(summary.backlog_size_before(), 2);
    assert_eq!(
        summary.advanced_to_ack(),
        0,
        "doc A terminal-halts the chain; doc B is NOT processed"
    );
    assert_eq!(summary.per_doc_failures().len(), 1, "doc A in failures");
    assert_eq!(summary.per_doc_failures()[0].0, doc_a);
    assert_eq!(summary.per_doc_failures()[0].1, "signer_refused");

    // Doc A stays at OFFLINE_LOCAL_ACK (signer_guard runs BEFORE CAS).
    assert_eq!(read_doc_state(&pool, doc_a).await, "OFFLINE_LOCAL_ACK");
    // Doc B is NOT processed — stays durable OFFLINE_LOCAL_ACK (chain halted).
    assert_eq!(
        read_doc_state(&pool, doc_b).await,
        "OFFLINE_LOCAL_ACK",
        "doc B MUST NOT be processed — chain halted at doc A's terminal failure"
    );

    // No doc advanced: the chain halted at doc A before any wire send.
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await, 0);
    assert_eq!(audit_count(&pool, "STAGE_FINALIZE_ACK").await, 0);
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_FAILED").await, 1);

    let failed_payloads = audit_payloads_for(&pool, "OFFLINE_DRAIN_DOC_FAILED").await;
    assert_eq!(failed_payloads[0]["failure_class"], "signer_refused");
    // mismatch_detail echoes the SignerCashierMismatch Display string;
    // exact wording is owned by `signer_guard.rs` (thiserror format).
    // Lock only the structural property: non-empty string present.
    let detail = failed_payloads[0]["mismatch_detail"]
        .as_str()
        .expect("mismatch_detail must be a string");
    assert!(
        !detail.is_empty(),
        "mismatch_detail must carry the signer guard's Display string (got empty)"
    );

    // Strict-sequential: terminal SignerRefused HALTS + escalates the FN shift
    // to RequiresManualReconciliation even on a plain Opened shift (edge 15),
    // emitting the Critical halt audit exactly once.
    assert_eq!(
        read_shift_state(&pool, shift_id).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "terminal SignerRefused MUST escalate even a plain Opened shift → Manual"
    );
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await,
        1,
        "the halt emits the Critical escalation audit"
    );
}

// ─── Test 4: strict lnd ASC processing order ─────────────────────────

#[tokio::test]
async fn c4_processes_backlog_in_lnd_asc_order() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool, CASHIER_OK).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    // Insert lnd=5 FIRST, then lnd=2, then lnd=8.  The C1 helper
    // returns rows in `lnd ASC` order regardless of insertion order.
    let doc_lnd5 =
        seed_complete_offline_local_ack(&pool, 5, 100, session_id, shift_id, CASHIER_OK).await;
    let doc_lnd2 =
        seed_complete_offline_local_ack(&pool, 2, 101, session_id, shift_id, CASHIER_OK).await;
    let doc_lnd8 =
        seed_complete_offline_local_ack(&pool, 8, 102, session_id, shift_id, CASHIER_OK).await;
    // M3b W12 Commit 4b.3: chain seed in lnd-ASC drain order.
    // Processing: lnd=2 → lnd=5 → lnd=8.
    common::init_chain_seed(&pool, FN, common::chain_anchor(0x00))
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc_lnd2,
        common::chain_anchor(0x00),
        common::chain_anchor(0x01),
    )
    .await
    .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc_lnd5,
        common::chain_anchor(0x01),
        common::chain_anchor(0x02),
    )
    .await
    .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc_lnd8,
        common::chain_anchor(0x02),
        common::chain_anchor(0x03),
    )
    .await
    .unwrap();

    // 3 OK send_chk + 3 lastChk (Acked path for each doc).  Queue
    // order = drain processing order = lnd-ASC.
    let carriers = carriers_with_responses_and_last_chk(
        vec![Ok(ack("LND-2")), Ok(ack("LND-5")), Ok(ack("LND-8"))],
        vec![
            Ok(last_chk_ack("LND-2", kvt1_raw_bytes_for("LND-2"))),
            Ok(last_chk_ack("LND-5", kvt1_raw_bytes_for("LND-5"))),
            Ok(last_chk_ack("LND-8", kvt1_raw_bytes_for("LND-8"))),
        ],
    );
    let view = view_for(&carriers);

    let _summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    // Verify ordering via audit row sequence — audit_log.audit_id is
    // AUTOINCREMENT, so ASC order == chronological emit order.  Each
    // KVT2_ADVANCED payload's server_fiscal_no pins which doc
    // consumed which slot.
    let advanced = audit_payloads_for(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await;
    assert_eq!(advanced.len(), 3);
    assert_eq!(advanced[0]["server_fiscal_no"], "LND-2");
    assert_eq!(advanced[1]["server_fiscal_no"], "LND-5");
    assert_eq!(advanced[2]["server_fiscal_no"], "LND-8");

    // Cross-pin via document_id hex — the first ADVANCED audit must
    // reference doc_lnd2 (the lowest lnd), not doc_lnd5 (first inserted).
    let id2_hex: String = doc_lnd2
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let id5_hex: String = doc_lnd5
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let id8_hex: String = doc_lnd8
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(advanced[0]["document_id"], id2_hex);
    assert_eq!(advanced[1]["document_id"], id5_hex);
    assert_eq!(advanced[2]["document_id"], id8_hex);
}

// ─── Test 5: accounting invariant ────────────────────────────────────

/// **M2-N1 / M2-N2a (architect ruling B, 2026-06-13)** — strict-sequential
/// offline-origin drain.  REBUILT from the pre-M2-01 sibling-continue pin
/// (`c4_accounting_advanced_plus_failures_equals_backlog`).  M2-01 made
/// offline-origin docs a STRICT predecessor chain (each signs
/// `previous_hash = unsigned(prior issued doc)`), so the drain MUST NOT send
/// `lnd+1` past a non-ACK predecessor.  A TERMINAL reject of doc B
/// (non-self-resolving) HALTS the chain — doc C (whose predecessor is B) is
/// NOT sent and stays `OFFLINE_LOCAL_ACK` (durable) — and escalates the FN
/// to `RequiresManualReconciliation` even on a PLAIN `Opened` shift
/// (whitelist edge 15), so there is an operator surface (no silent
/// GoingOnline/Draining wedge).  Pins the FT (M2-N1) + the escalation
/// (M2-N2a) + scan-clean over a REJECTED offline-origin predecessor (M2-N2b).
#[tokio::test]
async fn strict_sequential_terminal_reject_halts_chain_and_escalates_manual() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool, CASHIER_OK).await;
    // Drain reads node_state.current_shift_id to identify the shift to
    // escalate (M2-N2a edge 15 for plain Opened).
    set_node_current_shift(&pool, shift_id).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    // 3 offline-origin docs forming a strict M2-01 chain A→B→C.
    let doc_a =
        seed_complete_offline_local_ack(&pool, 1, 100, session_id, shift_id, CASHIER_OK).await;
    let doc_b =
        seed_complete_offline_local_ack(&pool, 2, 101, session_id, shift_id, CASHIER_OK).await;
    let doc_c =
        seed_complete_offline_local_ack(&pool, 3, 102, session_id, shift_id, CASHIER_OK).await;
    // Real strict chain: A(prev=G,unsigned=H_A) → B(prev=H_A,unsigned=H_B)
    // → C(prev=H_B,unsigned=H_C).  node seed = H_C — all 3 reached
    // OFFLINE_LOCAL_ACK (issued offline ⇒ seed advanced past them at
    // offline-ack; finalize does NOT re-advance offline-origin docs, M2-01).
    common::init_chain_seed(&pool, FN, common::chain_anchor(0x03))
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc_a,
        common::chain_anchor(0x00),
        common::chain_anchor(0x01),
    )
    .await
    .unwrap();
    // doc_a is the chain HEAD — reset previous_hash to NULL (genesis) so the
    // invariant_scan walk's first row matches its `None` start (the inbox row +
    // unsigned hash from seed_w12_finalize_prereqs are kept for the ACK advance).
    sqlx::query("UPDATE fiscal_documents SET previous_hash = NULL WHERE document_id = ?")
        .bind(doc_a)
        .execute(&pool)
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc_b,
        common::chain_anchor(0x01),
        common::chain_anchor(0x02),
    )
    .await
    .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc_c,
        common::chain_anchor(0x02),
        common::chain_anchor(0x03),
    )
    .await
    .unwrap();

    // A → send OK + lastChk ACK → ACK.  B → DocumentReject (terminal,
    // non-self-resolving).  A 3rd send response for C is provided so the
    // stub never underflows — under strict-sequential C is NOT sent, so the
    // `call_count == 2` assertion is the load-bearing pin.
    let carriers = carriers_with_responses_and_last_chk(
        vec![
            Ok(ack("OK-A")),
            Err(DpsError::Authorization {
                code: -1,
                kind: AuthorizationKind::DocumentReject,
                message: "reject_B".into(),
            }),
            Ok(ack("OK-C")),
        ],
        // A's lastChk + a spare C lastChk so the CURRENT (buggy) sibling-
        // continue code reaches the assertions (sends C → lastChk C) instead
        // of panicking on stub underflow — RED then fails cleanly on the
        // `doc_c == OFFLINE_LOCAL_ACK` / `call_count == 2` pins.  Under the
        // GREEN strict-sequential code C is never sent, so the spare is unused.
        vec![
            Ok(last_chk_ack("OK-A", kvt1_raw_bytes_for("OK-A"))),
            Ok(last_chk_ack("OK-C", kvt1_raw_bytes_for("OK-C"))),
        ],
    );
    let view = view_for(&carriers);

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    // Strict-sequential: A acked, B rejected (terminal) → HALT, C NOT sent.
    assert_eq!(read_doc_state(&pool, doc_a).await, "ACK");
    assert_eq!(read_doc_state(&pool, doc_b).await, "REJECTED");
    assert_eq!(
        read_doc_state(&pool, doc_c).await,
        "OFFLINE_LOCAL_ACK",
        "C's predecessor B was rejected → C MUST NOT be sent (strict chain)"
    );

    // Wire: only A + B reached send_chk; C never did.
    assert_eq!(
        carriers.dps.call_count(),
        2,
        "doc C must NOT be sent (strict halt at rejected predecessor)"
    );
    assert_eq!(
        carriers.dps.last_chk_call_count(),
        1,
        "lastChk only for A (B rejected at send; C never sent)"
    );

    // Summary accounting: A advanced, B failed; C neither advanced nor failed
    // (held back, still OFFLINE_LOCAL_ACK).
    assert_eq!(summary.advanced_to_ack(), 1, "only A reaches Ack");
    assert_eq!(summary.per_doc_failures().len(), 1, "only B failed");

    // FN escalated to durable manual-recon on a PLAIN Opened shift (edge 15).
    assert_eq!(
        read_shift_state(&pool, shift_id).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "terminal reject MUST escalate even a plain Opened shift → Manual"
    );
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await,
        1,
        "the halt emits the Critical escalation audit"
    );

    // Ledger invariant clean: a REJECTED offline-origin predecessor in the
    // chain must NOT trip a false ChainBreak at its successor (M2-N2b).
    prro::db::invariant_scan::assert_clean(&pool).await;
}

// ─── Test 6: pending-drain shift reject halts drain + escalates ──────

/// Spec amendment 2026-05-21 + `LEGAL_INVARIANTS.md` §INV-19 + spec
/// §6.3: drain reject on a shift currently in
/// `OpenedLocalPendingDrain` / `ClosingLocalPendingDrain` lands the
/// shift in `RequiresManualReconciliation` (edges 6 / 14), emits
/// Critical `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` audit, and halts
/// the FN drain.  Subsequent backlog docs are NOT visited.
#[tokio::test]
async fn c4_pending_drain_shift_reject_halts_and_transitions_shift_to_manual() {
    let (_d, pool) = fresh_pool().await;
    // Seed node_state with shift_state=OPENED_LOCAL_PENDING_DRAIN +
    // a linked shift row in the same state.  This is the operator-
    // confirmed pending-drain configuration where any drain reject
    // = Manual.
    seed_node_state(
        &pool,
        NodeMode::GoingOnline,
        ShiftState::OpenedLocalPendingDrain,
    )
    .await;
    let shift_id = seed_shift_with_state(&pool, CASHIER_OK, "OPENED_LOCAL_PENDING_DRAIN").await;
    set_node_current_shift(&pool, shift_id).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;

    // 3-doc backlog.  Doc A: stage_send Sent + KVT1 advance (sets
    // server_fiscal_no so we can pin "halted BEFORE this doc was
    // visited" through the wire stub queue depth).  Doc B: rejected
    // — triggers halt.  Doc C: would have been processed but loop
    // halts at doc B.
    let doc_a =
        seed_complete_offline_local_ack(&pool, 1, 100, session_id, shift_id, CASHIER_OK).await;
    let doc_b =
        seed_complete_offline_local_ack(&pool, 2, 101, session_id, shift_id, CASHIER_OK).await;
    let doc_c =
        seed_complete_offline_local_ack(&pool, 3, 102, session_id, shift_id, CASHIER_OK).await;
    // M3b W12 Commit 4b.3: chain seed for A only (B halts after
    // stage_send failure; C never processed).
    common::init_chain_seed(&pool, FN, common::chain_anchor(0x00))
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc_a,
        common::chain_anchor(0x00),
        common::chain_anchor(0x01),
    )
    .await
    .unwrap();

    let carriers = carriers_with_responses_and_last_chk(
        vec![
            Ok(ack("OK-A")),
            Err(DpsError::Authorization {
                code: -1,
                kind: AuthorizationKind::DocumentReject,
                message: "halt_trigger".into(),
            }),
            // Doc C response NOT consumed (loop halts at B).  Stub queue
            // depth check below pins this.
            Ok(ack("UNREACHED-C")),
        ],
        vec![Ok(last_chk_ack("OK-A", kvt1_raw_bytes_for("OK-A")))],
    );
    let view = view_for(&carriers);

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    // Drain halted after doc B: doc A advanced (Ack via W12), doc B
    // failed, doc C never visited.
    assert_eq!(summary.backlog_size_before(), 3);
    assert_eq!(summary.advanced_to_ack(), 1, "only doc A reached Ack");
    assert_eq!(summary.advanced_to_kvt1(), 0, "no DeferredKvt1 post-W12");
    assert_eq!(summary.per_doc_failures().len(), 1, "doc B in failures");
    assert_eq!(summary.per_doc_failures()[0].0, doc_b);
    assert_eq!(
        summary.per_doc_failures()[0].1,
        "wire_routing_terminal_reject"
    );

    // Per-doc DB states.
    assert_eq!(read_doc_state(&pool, doc_a).await, "ACK");
    assert_eq!(read_doc_state(&pool, doc_b).await, "REJECTED");
    assert_eq!(
        read_doc_state(&pool, doc_c).await,
        "OFFLINE_LOCAL_ACK",
        "doc C MUST NOT have been processed — drain halted at doc B"
    );
    assert_eq!(
        carriers.dps.call_count(),
        2,
        "drain halted at doc B; doc C wire response was NOT consumed"
    );

    // Shift transitioned to RequiresManualReconciliation via edge 6.
    assert_eq!(
        read_shift_state(&pool, shift_id).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "halt path MUST CAS shift through whitelist edge 6"
    );
    // HIGH-C4-5 fix: node_state.shift_state mirrors shifts.state in the
    // same with_immediate envelope per m3b-shift-state-expansion §5
    // load-bearing invariant.  Without this, downstream readers
    // (stage_acquire / boot_phase / W12) would diverge from the
    // authoritative shifts row.
    assert_eq!(
        read_node_shift_state(&pool).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "node_state.shift_state MUST mirror shifts.state inside the same tx"
    );

    // Critical OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL audit emitted
    // exactly once + per-doc audit chain preserved.  Doc A's
    // advance now emits 1 KVT2_ADVANCED (Envelope 1a) + 1
    // STAGE_FINALIZE_ACK (Envelope 2) via W12 SentFresh chain.
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await,
        1
    );
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await, 1);
    assert_eq!(audit_count(&pool, "STAGE_FINALIZE_ACK").await, 1);
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_FAILED").await, 1);

    // Escalation payload sanity — pins forensic fields.
    let halt_payloads = audit_payloads_for(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await;
    assert_eq!(halt_payloads.len(), 1);
    assert_eq!(halt_payloads[0]["fiscal_number"], FN);
    assert_eq!(
        halt_payloads[0]["failure_class"],
        "wire_routing_terminal_reject"
    );
    assert_eq!(
        halt_payloads[0]["current_shift_state"],
        "OPENED_LOCAL_PENDING_DRAIN"
    );
    assert_eq!(
        halt_payloads[0]["halt_position"], 1,
        "doc B is at 0-based index 1 in the backlog"
    );
}

// ─── Test 7: transient retry — chain HALTS this tick, NO Manual ──────

/// Strict-sequential offline drain (architect ruling B, 2026-06-13):
/// TransientRetry (Transport / Server-3) is a `manual_recon:false` failure.
/// The chain HALTS THIS TICK at the transient doc — there is NO sibling-
/// continue, so higher-lnd docs are NOT processed this tick — but there is
/// NO Manual escalation and NO escalation audit; the held docs retry on the
/// next tick.  The shift stays in `OpenedLocalPendingDrain` (spec §3.5:
/// transient outcomes retain retry budget).
#[tokio::test]
async fn c4_pending_drain_shift_transient_retry_halts_chain_this_tick_no_manual() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(
        &pool,
        NodeMode::GoingOnline,
        ShiftState::OpenedLocalPendingDrain,
    )
    .await;
    let shift_id = seed_shift_with_state(&pool, CASHIER_OK, "OPENED_LOCAL_PENDING_DRAIN").await;
    set_node_current_shift(&pool, shift_id).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;

    let doc_a =
        seed_complete_offline_local_ack(&pool, 1, 100, session_id, shift_id, CASHIER_OK).await;
    let doc_b =
        seed_complete_offline_local_ack(&pool, 2, 101, session_id, shift_id, CASHIER_OK).await;
    let doc_c =
        seed_complete_offline_local_ack(&pool, 3, 102, session_id, shift_id, CASHIER_OK).await;
    // M3b W12 Commit 4b.3 chain seed for A only is load-bearing; the C seed is
    // retained but unused: doc B's transient halt means doc C is not processed
    // this tick.
    common::init_chain_seed(&pool, FN, common::chain_anchor(0x00))
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc_a,
        common::chain_anchor(0x00),
        common::chain_anchor(0x01),
    )
    .await
    .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc_c,
        common::chain_anchor(0x01),
        common::chain_anchor(0x02),
    )
    .await
    .unwrap();

    // Doc A: Ok wire reply → W12 SentFresh → ACK.  Doc B: Transport
    // error → RetryClass::TransientRetry → doc → ErrorRetryable, and the
    // chain HALTS this tick.  Doc C's wire response is queued but NOT
    // consumed (loop halts at B).
    let carriers = carriers_with_responses_and_last_chk(
        vec![
            Ok(ack("OK-A")),
            Err(DpsError::Transport("simulated link flap".into())),
            Ok(ack("UNREACHED-C")),
        ],
        vec![Ok(last_chk_ack("OK-A", kvt1_raw_bytes_for("OK-A")))],
    );
    let view = view_for(&carriers);

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(summary.backlog_size_before(), 3);
    assert_eq!(
        summary.advanced_to_ack(),
        1,
        "only doc A reached Ack; doc B transient HALTS the chain this tick (no sibling-continue)"
    );
    assert_eq!(summary.advanced_to_kvt1(), 0, "no DeferredKvt1 post-W12");
    assert_eq!(summary.per_doc_failures().len(), 1);
    assert_eq!(
        summary.per_doc_failures()[0].1,
        "wire_routing_transient_retry"
    );
    // Only A + B visited the wire; doc C was NOT processed this tick (chain
    // halted at the transient doc B).
    assert_eq!(
        carriers.dps.call_count(),
        2,
        "doc C must NOT be sent — chain halted at transient doc B this tick"
    );
    assert_eq!(
        carriers.dps.last_chk_call_count(),
        1,
        "lastChk only for A (B never reaches Sent; C never processed)"
    );

    // DB states.
    assert_eq!(read_doc_state(&pool, doc_a).await, "ACK");
    assert_eq!(read_doc_state(&pool, doc_b).await, "ERROR_RETRYABLE");
    assert_eq!(
        read_doc_state(&pool, doc_c).await,
        "OFFLINE_LOCAL_ACK",
        "doc C MUST NOT be processed this tick — held behind transient doc B"
    );

    // Shift + node_state mirror MUST stay in OPENED_LOCAL_PENDING_DRAIN
    // — transient retry does NOT escalate per spec §3.5.
    assert_eq!(
        read_shift_state(&pool, shift_id).await,
        "OPENED_LOCAL_PENDING_DRAIN"
    );
    assert_eq!(
        read_node_shift_state(&pool).await,
        "OPENED_LOCAL_PENDING_DRAIN"
    );

    // No manual-escalation audit emitted.
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await,
        0,
        "TransientRetry MUST NOT escalate to Manual even on pending-drain shift"
    );

    // The DOC_FAILED audit row's `manual_recon_class` flag is FALSE
    // for TransientRetry — locks the routing decision in the audit
    // payload for operator forensics.
    let failed_payloads = audit_payloads_for(&pool, "OFFLINE_DRAIN_DOC_FAILED").await;
    assert_eq!(failed_payloads.len(), 1);
    assert_eq!(
        failed_payloads[0]["retry_class"], "TransientRetry",
        "wire error_routing routed Transport → TransientRetry"
    );
    assert_eq!(
        failed_payloads[0]["manual_recon_class"], false,
        "TransientRetry is NOT manual-recon class — operator dashboards filter on this"
    );
}

// ─── M3b W12 Commit 4b.3: SentFresh integration fixtures ────────────
//
// Plan §410 acceptance for Commit 4 wiring: 3 SentFresh end-to-end
// scenarios proving the Sent-source W12 chain (`process_via_stage_send`
// → `confirm_drain_doc(SentFresh)` → Envelope 1a + Envelope 2 → ACK,
// OR Envelope 1c-hold-light/1c-drift-light audit + BootError on
// non-Acked outcomes).
//
// 1. Acked — full Sent→ACK chain with audit shape lock.
// 2. NotFound from DPS → StructuralDrift via SentFresh source-context
//    matrix (plan §classify_check_result NotFound + SentFresh →
//    StructuralDrift::NotFoundOutsideSentReplay) → Envelope
//    1c-drift-light KVT2_CONFIRM_STRUCTURAL_DRIFT audit + BootError::
//    Internal halts FN drain per plan §410.
// 3. Mismatch (DPS returns ack.id != expected) → StructuralDrift::
//    LastChkIdMismatch → same envelope chain + halt.
//
// Acked-path coverage complements the refactored c4_happy_path; these
// fixtures focus on the unique error-path projection contracts not
// otherwise exercised at integration level.

#[tokio::test]
async fn w12_sent_fresh_not_found_emits_drift_audit_and_halts_via_boot_error() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool, CASHIER_OK).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let doc =
        seed_complete_offline_local_ack(&pool, 1, 100, session_id, shift_id, CASHIER_OK).await;
    common::init_chain_seed(&pool, FN, common::chain_anchor(0x00))
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc,
        common::chain_anchor(0x00),
        common::chain_anchor(0x01),
    )
    .await
    .unwrap();

    // stage_send succeeds (Sent), then lastChk returns empty id →
    // canonical `by_server_fiscal_no` maps to DpsError::NotFound →
    // classify_check_result(NotFound, SentFresh) → StructuralDrift::
    // NotFoundOutsideSentReplay.  Helper emits Envelope 1c-drift-light
    // KVT2_CONFIRM_STRUCTURAL_DRIFT audit BEFORE BootError::Internal.
    let carriers = carriers_with_responses_and_last_chk(
        vec![Ok(ack("DPS-FRESH-NOT-FOUND"))],
        vec![Ok(last_chk_ack("", vec![]))],
    );
    let view = view_for(&carriers);

    let err = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .expect_err("SentFresh NotFound MUST halt drain via BootError::Internal");

    // BootError::Internal per plan §410.
    let err_str = err.to_string();
    assert!(
        err_str.contains("structural drift") || err_str.contains("STRUCTURAL_DRIFT"),
        "BootError must mention structural drift; got: {err_str}"
    );

    // Doc state UNCHANGED (Envelope 1a never committed; drift envelope
    // is audit-only).  stage_send committed Sending → Sent already.
    assert_eq!(read_doc_state(&pool, doc).await, "SENT");

    // Envelope 1c-drift-light fired — KVT2_CONFIRM_STRUCTURAL_DRIFT
    // audit row landed BEFORE BootError propagation.
    assert_eq!(
        audit_count(&pool, "KVT2_CONFIRM_STRUCTURAL_DRIFT").await,
        1,
        "drift envelope MUST emit forensic audit BEFORE fail-loud"
    );
    let drift_payloads = audit_payloads_for(&pool, "KVT2_CONFIRM_STRUCTURAL_DRIFT").await;
    assert_eq!(drift_payloads[0]["source"], "sent_fresh");
    assert_eq!(
        drift_payloads[0]["drift_reason"],
        "NOT_FOUND_OUTSIDE_SENT_REPLAY"
    );
    assert_eq!(drift_payloads[0]["dispatch_via"], "kvt2_confirm");

    // Envelope 1a (Kvt1Raw + CAS chain) MUST NOT have fired.
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await,
        0,
        "Envelope 1a MUST NOT fire on StructuralDrift path"
    );
    assert_eq!(audit_count(&pool, "STAGE_FINALIZE_ACK").await, 0);
}

#[tokio::test]
async fn w12_sent_fresh_mismatch_emits_drift_audit_and_halts_via_boot_error() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool, CASHIER_OK).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let doc =
        seed_complete_offline_local_ack(&pool, 1, 100, session_id, shift_id, CASHIER_OK).await;
    common::init_chain_seed(&pool, FN, common::chain_anchor(0x00))
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc,
        common::chain_anchor(0x00),
        common::chain_anchor(0x01),
    )
    .await
    .unwrap();

    // stage_send returns Sent with server_fiscal_no="EXPECTED-A".
    // lastChk returns ack.id="DIFFERENT-B" → canonical
    // by_server_fiscal_no maps to ServerFiscalIdMismatch →
    // classify_check_result → StructuralDrift::LastChkIdMismatch.
    let carriers = carriers_with_responses_and_last_chk(
        vec![Ok(ack("EXPECTED-A"))],
        vec![Ok(last_chk_ack("DIFFERENT-B", vec![0xAAu8; 32]))],
    );
    let view = view_for(&carriers);

    let err = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .expect_err("SentFresh Mismatch MUST halt drain via BootError::Internal");

    let err_str = err.to_string();
    assert!(
        err_str.contains("structural drift") || err_str.contains("STRUCTURAL_DRIFT"),
        "BootError must mention structural drift; got: {err_str}"
    );

    // Doc state unchanged (Sent post-stage_send).
    assert_eq!(read_doc_state(&pool, doc).await, "SENT");

    // Envelope 1c-drift-light forensic audit.
    assert_eq!(audit_count(&pool, "KVT2_CONFIRM_STRUCTURAL_DRIFT").await, 1);
    let drift_payloads = audit_payloads_for(&pool, "KVT2_CONFIRM_STRUCTURAL_DRIFT").await;
    assert_eq!(drift_payloads[0]["source"], "sent_fresh");
    assert_eq!(drift_payloads[0]["drift_reason"], "LASTCHK_ID_MISMATCH");
    // Detail string carries the typed Debug rendering of the reason —
    // operators can grep the literal mismatch values for triage.
    let detail = drift_payloads[0]["drift_reason_detail"]
        .as_str()
        .expect("drift_reason_detail must be a string");
    assert!(
        detail.contains("DIFFERENT-B") && detail.contains("EXPECTED-A"),
        "drift_reason_detail must carry the observed/expected pair; got: {detail}"
    );

    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await, 0);
    assert_eq!(audit_count(&pool, "STAGE_FINALIZE_ACK").await, 0);
}

#[tokio::test]
async fn w12_sent_fresh_dps_transport_holds_drain_and_projects_held_at_sent() {
    // **M3b W12 Commit 6 (plan §413 production wiring, 2026-05-24)** —
    // refactored from `w12_sent_fresh_dps_transport_emits_hold_audit_
    // and_halts_via_boot_error`.  Pre-Commit-6: SentFresh Hold path
    // halt-ed FN drain via defensive `BootError::Internal` (5b.1
    // foundation).  Post-Commit-6: confirm_drain_doc Hold arm returns
    // `Ok(ConfirmDrainOutcome::HoldFnDrain { HeldAtSent })` per
    // MED-PR70-R7-02 projection matrix; `process_via_stage_send`
    // forwards as `Ok(DocVerdict::HoldFnDrain { Transport, HeldAtSent })`
    // → drain orchestrator halts on this doc (break), records
    // `held_at_sent` counter, returns successful summary.  Pending-
    // drain shifts do NOT escalate on HoldFnDrain (only manual-recon-
    // class Failed escalates per backlog_drain consumer logic).
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool, CASHIER_OK).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let doc =
        seed_complete_offline_local_ack(&pool, 1, 100, session_id, shift_id, CASHIER_OK).await;
    common::init_chain_seed(&pool, FN, common::chain_anchor(0x00))
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc,
        common::chain_anchor(0x00),
        common::chain_anchor(0x01),
    )
    .await
    .unwrap();

    // stage_send succeeds; lastChk fails with Transport error →
    // classify_check_result → Hold(DpsTransport) → Commit-6 HoldFnDrain.
    let carriers = carriers_with_responses_and_last_chk(
        vec![Ok(ack("DPS-FRESH-HOLD"))],
        vec![Err(DpsError::Transport("simulated lastChk timeout".into()))],
    );
    let view = view_for(&carriers);

    let summary = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    // Doc state unchanged (Sent — Envelope 1a never committed; 1c-hold
    // does not CAS state per W0b state-unchanged contract).
    assert_eq!(read_doc_state(&pool, doc).await, "SENT");

    // HoldFnDrain projection per MED-PR70-R7-02: SentFresh → HeldAtSent
    // (NOT HeldAtKvt1; doc is Sent post-stage_send, hold doesn't advance).
    assert_eq!(
        summary.held_at_sent(),
        1,
        "SentFresh Hold MUST project HeldAtSent"
    );
    assert_eq!(
        summary.held_at_kvt1(),
        0,
        "SentFresh Hold MUST NOT project HeldAtKvt1 (doc is Sent, not Kvt1)"
    );
    assert_eq!(summary.er_redrive_queued(), 0);

    // Envelope 1c-hold-light forensic audit fired (Severity::Warning
    // per plan §449); audit chain unchanged from 4b.1 foundation.
    assert_eq!(audit_count(&pool, "KVT2_CONFIRM_HOLD").await, 1);
    let hold_payloads = audit_payloads_for(&pool, "KVT2_CONFIRM_HOLD").await;
    assert_eq!(hold_payloads[0]["source"], "sent_fresh");
    assert_eq!(hold_payloads[0]["hold_reason"], "DPS_TRANSPORT");
    assert_eq!(hold_payloads[0]["dispatch_via"], "kvt2_confirm");
    let detail = hold_payloads[0]["hold_reason_detail"]
        .as_str()
        .expect("hold_reason_detail must be a string");
    assert!(
        detail.contains("simulated lastChk timeout"),
        "hold_reason_detail must carry the DPS error message; got: {detail}"
    );

    // No advance / drift audit on Hold path.
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_KVT2_ADVANCED").await, 0);
    assert_eq!(audit_count(&pool, "KVT2_CONFIRM_STRUCTURAL_DRIFT").await, 0);
}

/// **M2-N1 / ruling B (architect, 2026-06-13)** — a TRANSIENT failure on an
/// offline-origin predecessor HALTS the chain for THIS tick (does NOT escalate
/// Manual, does NOT send the successor), and the NEXT tick RETRIES it and drains
/// the chain.  The ruling-B pin distinguishing transient (retry-budget
/// preserved, no premature Manual) from terminal (escalate).  Two-tick test:
///   - tick 1: doc_a send → Transport error → ERROR_RETRYABLE → HALT; doc_b NOT
///     sent; shift stays Opened; NO escalation audit.
///   - tick 2: doc_a re-drives (ER class-guard) → ACK; doc_b then → ACK.
#[tokio::test]
async fn strict_sequential_transient_halts_this_tick_then_retries_next_tick() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool, CASHIER_OK).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let doc_a =
        seed_complete_offline_local_ack(&pool, 1, 100, session_id, shift_id, CASHIER_OK).await;
    let doc_b =
        seed_complete_offline_local_ack(&pool, 2, 101, session_id, shift_id, CASHIER_OK).await;
    // Strict chain A→B (A genesis head, prev NULL) so both can reach ACK on tick 2.
    common::init_chain_seed(&pool, FN, common::chain_anchor(0x02))
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc_a,
        common::chain_anchor(0x00),
        common::chain_anchor(0x01),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE fiscal_documents SET previous_hash = NULL WHERE document_id = ?")
        .bind(doc_a)
        .execute(&pool)
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc_b,
        common::chain_anchor(0x01),
        common::chain_anchor(0x02),
    )
    .await
    .unwrap();

    // ── Tick 1: doc_a send → Transport error → ERROR_RETRYABLE → HALT. ──
    let c1 = carriers_with_responses_and_last_chk(
        vec![Err(DpsError::Transport("tick1 link flap".into()))],
        vec![],
    );
    let v1 = view_for(&c1);
    let _ = backlog_drain::drain(&common::drain_test_guard(), &pool, &v1, FN)
        .await
        .unwrap();

    assert_eq!(
        read_doc_state(&pool, doc_a).await,
        "ERROR_RETRYABLE",
        "transient send failure parks doc_a in ERROR_RETRYABLE"
    );
    assert_eq!(
        read_doc_state(&pool, doc_b).await,
        "OFFLINE_LOCAL_ACK",
        "doc_b NOT sent — chain halts at the transient predecessor"
    );
    assert_eq!(
        c1.dps.call_count(),
        1,
        "only doc_a reached the wire on tick 1"
    );
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await,
        0,
        "transient halt does NOT escalate to Manual"
    );
    assert_eq!(
        read_shift_state(&pool, shift_id).await,
        "OPENED",
        "shift stays Opened on a transient halt (no escalation)"
    );

    // ── Tick 2: doc_a re-drives → ACK, then doc_b → ACK. ──
    let c2 = carriers_with_responses_and_last_chk(
        vec![Ok(ack("OK-A")), Ok(ack("OK-B"))],
        vec![
            Ok(last_chk_ack("OK-A", kvt1_raw_bytes_for("OK-A"))),
            Ok(last_chk_ack("OK-B", kvt1_raw_bytes_for("OK-B"))),
        ],
    );
    let v2 = view_for(&c2);
    let _ = backlog_drain::drain(&common::drain_test_guard(), &pool, &v2, FN)
        .await
        .unwrap();

    assert_eq!(
        read_doc_state(&pool, doc_a).await,
        "ACK",
        "next tick retries doc_a → ACK"
    );
    assert_eq!(
        read_doc_state(&pool, doc_b).await,
        "ACK",
        "the chain drains past the now-acked predecessor on the retry tick"
    );
}

/// **H-M2-1 (architect ruling, 2026-06-13) — DPS ERROR_BAD_HASH_PREV cascade
/// contract pin (does NOT gate the fix).**  Whether DPS cascade-rejects the
/// SUCCESSORS of a rejected predecessor (ERROR_BAD_HASH_PREV) is DPS-behaviour-
/// dependent and UNKNOWN from our corpus (zero rejected-offline-backlog
/// empirics).  The strict-sequential stop (M2-N1) makes the question MOOT for
/// the gateway: a successor `B` (whose `previous_hash = hash(A)`) is NEVER wired
/// after predecessor `A` is rejected, so DPS never evaluates B's prev-hash —
/// the fix is safe under BOTH possible DPS behaviours (cascade-reject OR
/// accept-broken-chain).  This test pins that invariant (B is never sent).
///
/// **LIVE TEST-CAMPAIGN NOTE:** confirm the real ДПС's ERROR_BAD_HASH_PREV
/// cascade behaviour against a live ПРРО (issue A+B offline, force-reject A on
/// drain, observe whether DPS would have rejected B) — to validate the
/// stop-on-non-ACK assumption empirically.  Until then the gateway never
/// exposes the cascade surface.
#[tokio::test]
async fn h_m2_1_rejected_predecessor_never_wires_successor_bad_hash_prev_moot() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool, CASHIER_OK).await;
    set_node_current_shift(&pool, shift_id).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    // Strict chain A→B: B.previous_hash = hash(A).  A genesis head (prev NULL).
    let doc_a =
        seed_complete_offline_local_ack(&pool, 1, 100, session_id, shift_id, CASHIER_OK).await;
    let doc_b =
        seed_complete_offline_local_ack(&pool, 2, 101, session_id, shift_id, CASHIER_OK).await;
    common::init_chain_seed(&pool, FN, common::chain_anchor(0x02))
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc_a,
        common::chain_anchor(0x00),
        common::chain_anchor(0x01),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE fiscal_documents SET previous_hash = NULL WHERE document_id = ?")
        .bind(doc_a)
        .execute(&pool)
        .await
        .unwrap();
    common::seed_w12_finalize_prereqs(
        &pool,
        FN,
        doc_b,
        common::chain_anchor(0x01),
        common::chain_anchor(0x02),
    )
    .await
    .unwrap();

    // A → DocumentReject (terminal).  A spare B send response is provided so the
    // stub never underflows — under strict-sequential B is NEVER wired.
    let carriers = carriers_with_responses_and_last_chk(
        vec![
            Err(DpsError::Authorization {
                code: -1,
                kind: AuthorizationKind::DocumentReject,
                message: "reject_A".into(),
            }),
            Ok(ack("UNREACHED-B")),
        ],
        vec![],
    );
    let view = view_for(&carriers);

    let _ = backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)
        .await
        .unwrap();

    assert_eq!(read_doc_state(&pool, doc_a).await, "REJECTED");
    // The load-bearing H-M2-1 pin: B (successor of the rejected predecessor) is
    // NEVER wired, so DPS never sees a bad-hash-prev chain.
    assert_eq!(
        read_doc_state(&pool, doc_b).await,
        "OFFLINE_LOCAL_ACK",
        "successor of a rejected predecessor MUST NOT be wired (ERROR_BAD_HASH_PREV moot)"
    );
    assert_eq!(
        carriers.dps.call_count(),
        1,
        "only the rejected predecessor reached the wire; the successor never did"
    );
    // FN escalated (terminal reject); ledger clean.
    assert_eq!(
        read_shift_state(&pool, shift_id).await,
        "REQUIRES_MANUAL_RECONCILIATION"
    );
    prro::db::invariant_scan::assert_clean(&pool).await;
}
