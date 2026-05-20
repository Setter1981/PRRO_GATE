//! W9b Commit 4 — `backlog_drain::drain` per-doc loop + Manual-
//! escalation on pending-drain shift reject.
//!
//! Acceptance:
//!   - Spec §2.3 Step B + §2.5: invoke `stage_send::run` per doc in
//!     strict `lnd ASC`; route outcomes via private helpers; audit
//!     `OFFLINE_DRAIN_DOC_ADVANCED` / `_DOC_FAILED`.
//!   - Spec amendment 2026-05-21 (C4 senior review): inline
//!     `Sent → Kvt1` via W12 stub so DB state, counters, and audit
//!     all say KVT1 in C4 isolation.  Sibling-continue applies
//!     ONLY to non-pending-drain shifts; pending-drain reject
//!     halts the drain + transitions shift to
//!     `RequiresManualReconciliation` per `LEGAL_INVARIANTS.md`
//!     §INV-19 + `m3b-shift-state-expansion.md` §6.3.
//!
//! **C5 blocker** (deferred from C4): SENT rediscovery on restart —
//! the walker is OFFLINE_LOCAL_ACK-only here; C5 widens it to
//! `OFFLINE_LOCAL_ACK | SENT | KVT1 | KVT2` for restart-safe drain.
//! See module-doc on `backlog_drain.rs` for the full known-gaps list.
//!
//! Tests (7):
//!
//!   1. `c4_happy_path_two_docs_advance_to_kvt1_and_emit_doc_advanced`
//!   2. `c4_routed_terminal_reject_records_wire_routing_failure_class`
//!   3. `c4_signer_refused_records_signer_refused_class_and_sibling_continues`
//!   4. `c4_processes_backlog_in_lnd_asc_order`
//!   5. `c4_accounting_advanced_plus_failures_equals_backlog`
//!   6. `c4_pending_drain_shift_reject_halts_and_transitions_shift_to_manual`
//!   7. `c4_pending_drain_shift_transient_retry_sibling_continues_no_halt`

mod common;

use std::sync::Arc;

use prro::db::models::enums::{NodeMode, OfflineSessionState, ShiftState};
use prro::db::models::ids::{DocumentId, OfflineSessionId, ShiftId};
use prro::services::offline_sync::backlog_drain;
use prro::services::reconciliation::runtime::RuntimeView;
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::dto::{CheckAck, CheckSignBlob};
use prro::transports::dps::error::{AuthorizationKind, DpsError};
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
    sqlx::query(
        "INSERT INTO fiscal_documents( \
            document_id, request_id, fiscal_number, shift_id, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, signed_by_cashier_id, \
            offline_session_id, offline_fiscal_no, offline_fiscal_date \
         ) VALUES ( \
            ?, ?, ?, ?, ?, 'SELL', 'OFFLINE_LOCAL_ACK', \
            'b', 't', 'OFFLINE', '2026-05-20T00:00:00Z', \
            '{}', ?, ?, \
            ?, ?, '2026-05-20T00:00:00Z' \
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

async fn audit_payloads_for(
    pool: &SqlitePool,
    event_type: &str,
) -> Vec<serde_json::Value> {
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

fn view_for<'a>(carriers: &'a DepsCarriers) -> RuntimeView<'a> {
    RuntimeView {
        dps: carriers.dps.as_ref(),
        signing_ctx: &carriers.signing_ctx,
        fn_sign: &carriers.fn_sign,
    }
}

// ─── Test 1: happy path — 2 docs advance ─────────────────────────────

#[tokio::test]
async fn c4_happy_path_two_docs_advance_to_kvt1_and_emit_doc_advanced() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool, CASHIER_OK).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    let doc_a =
        seed_complete_offline_local_ack(&pool, 1, 100, session_id, shift_id, CASHIER_OK).await;
    let doc_b =
        seed_complete_offline_local_ack(&pool, 2, 101, session_id, shift_id, CASHIER_OK).await;

    // 2 Ok wire responses — stage_send Ok→Sent twice; C4 inline
    // W12 stub then advances each Sent → Kvt1.
    let carriers = carriers_with_responses(vec![Ok(ack("DPS-FN-A")), Ok(ack("DPS-FN-B"))]);
    let view = view_for(&carriers);

    let summary = backlog_drain::drain(&pool, &view, FN).await.unwrap();

    assert_eq!(summary.backlog_size_before(), 2);
    assert_eq!(
        summary.advanced_to_kvt1(),
        2,
        "both docs counted as DeferredKvt1 (post-inline-stub)"
    );
    assert_eq!(summary.advanced_to_ack(), 0, "no Ack pre-W12");
    assert!(summary.per_doc_failures().is_empty());
    assert!(!summary.finalized(), "no finalize branch in C4");

    // Both docs reached KVT1 (stage_send post-wire CAS Sending→Sent
    // followed by the C4 inline W12 stub CAS Sent→Kvt1).  This is
    // the MED-C4-3 fix: counter, audit to_state, and persisted DB
    // state all say KVT1 in a single C4 flow.
    assert_eq!(read_doc_state(&pool, doc_a).await, "KVT1");
    assert_eq!(read_doc_state(&pool, doc_b).await, "KVT1");

    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_ADVANCED").await, 2);
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_FAILED").await, 0);
    assert_eq!(carriers.dps.call_count(), 2, "send_chk called once per doc");

    // Payload sanity on the first ADVANCED row — to_state=KVT1 + the
    // typed w12_status=DeferredKvt1 marker.
    let advanced_payloads = audit_payloads_for(&pool, "OFFLINE_DRAIN_DOC_ADVANCED").await;
    assert_eq!(advanced_payloads.len(), 2);
    assert_eq!(advanced_payloads[0]["from_state"], "OFFLINE_LOCAL_ACK");
    assert_eq!(advanced_payloads[0]["to_state"], "KVT1");
    assert_eq!(advanced_payloads[0]["w12_status"], "DeferredKvt1");
    assert_eq!(advanced_payloads[0]["replay_short_circuit"], false);
    assert_eq!(advanced_payloads[0]["server_fiscal_no"], "DPS-FN-A");
}

// ─── Test 2: routed terminal reject ──────────────────────────────────

#[tokio::test]
async fn c4_routed_terminal_reject_records_wire_routing_failure_class() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool, CASHIER_OK).await;
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

    let summary = backlog_drain::drain(&pool, &view, FN).await.unwrap();

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
    assert_eq!(failed_payloads[0]["failure_class"], "wire_routing_terminal_reject");
    assert_eq!(failed_payloads[0]["retry_class"], "TerminalReject");
}

// ─── Test 3: signer refused; sibling continues ───────────────────────

#[tokio::test]
async fn c4_signer_refused_records_signer_refused_class_and_sibling_continues() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool, CASHIER_OK).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    // Doc A: cashier mismatch → SignerRefused.  Doc B: cashier matches.
    let doc_a =
        seed_complete_offline_local_ack(&pool, 1, 100, session_id, shift_id, CASHIER_OTHER).await;
    let doc_b =
        seed_complete_offline_local_ack(&pool, 2, 101, session_id, shift_id, CASHIER_OK).await;

    // Only doc B should reach the wire (signer guard rejects doc A
    // BEFORE any 4-pre side effect).
    let carriers = carriers_with_responses(vec![Ok(ack("DPS-FN-B-ONLY"))]);
    let view = view_for(&carriers);

    let summary = backlog_drain::drain(&pool, &view, FN).await.unwrap();

    assert_eq!(summary.backlog_size_before(), 2);
    assert_eq!(summary.advanced_to_kvt1(), 1, "only doc B advances");
    assert_eq!(summary.per_doc_failures().len(), 1, "doc A in failures");
    assert_eq!(summary.per_doc_failures()[0].0, doc_a);
    assert_eq!(summary.per_doc_failures()[0].1, "signer_refused");

    // Doc A stays at OFFLINE_LOCAL_ACK (signer_guard runs BEFORE CAS).
    assert_eq!(read_doc_state(&pool, doc_a).await, "OFFLINE_LOCAL_ACK");
    // Doc B advanced to KVT1 (Sent → Kvt1 via C4 inline W12 stub).
    assert_eq!(read_doc_state(&pool, doc_b).await, "KVT1");

    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_ADVANCED").await, 1);
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

    // 3 OK responses — assigned in stub-queue order, which equals the
    // drain processing order.  Each id is distinct so we can pin
    // which doc consumed which response.
    let carriers = carriers_with_responses(vec![
        Ok(ack("LND-2")),
        Ok(ack("LND-5")),
        Ok(ack("LND-8")),
    ]);
    let view = view_for(&carriers);

    let _summary = backlog_drain::drain(&pool, &view, FN).await.unwrap();

    // Verify ordering via audit row sequence — audit_log.audit_id is
    // AUTOINCREMENT, so ASC order == chronological emit order.  Each
    // payload's server_fiscal_no pins which doc consumed which slot.
    let advanced = audit_payloads_for(&pool, "OFFLINE_DRAIN_DOC_ADVANCED").await;
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

#[tokio::test]
async fn c4_accounting_advanced_plus_failures_equals_backlog() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;
    let shift_id = seed_open_shift(&pool, CASHIER_OK).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;
    // 3 docs total: 2 successes (Ok responses) + 1 failure
    // (Authorization{DocumentReject}).  Accounting must add up to 3.
    let doc_a =
        seed_complete_offline_local_ack(&pool, 1, 100, session_id, shift_id, CASHIER_OK).await;
    let doc_b =
        seed_complete_offline_local_ack(&pool, 2, 101, session_id, shift_id, CASHIER_OK).await;
    let doc_c =
        seed_complete_offline_local_ack(&pool, 3, 102, session_id, shift_id, CASHIER_OK).await;

    let carriers = carriers_with_responses(vec![
        Ok(ack("OK-A")),
        Err(DpsError::Authorization {
            code: -1,
            kind: AuthorizationKind::DocumentReject,
            message: "reject_B".into(),
        }),
        Ok(ack("OK-C")),
    ]);
    let view = view_for(&carriers);

    let summary = backlog_drain::drain(&pool, &view, FN).await.unwrap();

    assert_eq!(summary.backlog_size_before(), 3);
    assert_eq!(summary.advanced_to_kvt1(), 2, "doc A + doc C");
    assert_eq!(summary.advanced_to_ack(), 0);
    assert_eq!(summary.per_doc_failures().len(), 1);

    // Accounting invariant — advanced + failures = backlog (no doc
    // silently dropped).
    let advanced = summary.advanced_to_ack() + summary.advanced_to_kvt1();
    let failures = summary.per_doc_failures().len();
    assert_eq!(
        advanced + failures,
        summary.backlog_size_before(),
        "every backlog doc must be accounted for in summary"
    );

    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_ADVANCED").await, 2);
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_FAILED").await, 1);

    // Sibling-continue contract: every doc was visited even though
    // the middle one failed.
    assert_eq!(carriers.dps.call_count(), 3, "all 3 docs reached the wire");

    // DB-state pinning: shift_state=Opened (non-pending-drain), so
    // sibling-continue applies.  Doc A + C: stage_send Sent → C4
    // inline W12 stub → KVT1.  Doc B: Authorization{DocumentReject}
    // → RetryClass::TerminalReject → stage_send 4-b CAS Sending →
    // Rejected.
    assert_eq!(read_doc_state(&pool, doc_a).await, "KVT1");
    assert_eq!(read_doc_state(&pool, doc_b).await, "REJECTED");
    assert_eq!(read_doc_state(&pool, doc_c).await, "KVT1");

    // No manual-escalation audit fires on a non-pending-drain shift.
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await,
        0,
        "OFFLINE_LOCAL_ACK on plain Opened shift must NOT escalate to Manual"
    );
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
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::OpenedLocalPendingDrain).await;
    let shift_id =
        seed_shift_with_state(&pool, CASHIER_OK, "OPENED_LOCAL_PENDING_DRAIN").await;
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

    let carriers = carriers_with_responses(vec![
        Ok(ack("OK-A")),
        Err(DpsError::Authorization {
            code: -1,
            kind: AuthorizationKind::DocumentReject,
            message: "halt_trigger".into(),
        }),
        // Doc C response NOT consumed (loop halts at B).  Stub queue
        // depth check below pins this.
        Ok(ack("UNREACHED-C")),
    ]);
    let view = view_for(&carriers);

    let summary = backlog_drain::drain(&pool, &view, FN).await.unwrap();

    // Drain halted after doc B: doc A advanced, doc B failed, doc C
    // never visited.
    assert_eq!(summary.backlog_size_before(), 3);
    assert_eq!(summary.advanced_to_kvt1(), 1, "only doc A advanced");
    assert_eq!(summary.advanced_to_ack(), 0);
    assert_eq!(summary.per_doc_failures().len(), 1, "doc B in failures");
    assert_eq!(summary.per_doc_failures()[0].0, doc_b);
    assert_eq!(
        summary.per_doc_failures()[0].1,
        "wire_routing_terminal_reject"
    );

    // Per-doc DB states.
    assert_eq!(read_doc_state(&pool, doc_a).await, "KVT1");
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
    // exactly once + per-doc audit chain preserved.
    assert_eq!(
        audit_count(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await,
        1
    );
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_ADVANCED").await, 1);
    assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_DOC_FAILED").await, 1);

    // Escalation payload sanity — pins forensic fields.
    let halt_payloads = audit_payloads_for(&pool, "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL").await;
    assert_eq!(halt_payloads.len(), 1);
    assert_eq!(halt_payloads[0]["fiscal_number"], FN);
    assert_eq!(halt_payloads[0]["failure_class"], "wire_routing_terminal_reject");
    assert_eq!(
        halt_payloads[0]["current_shift_state"],
        "OPENED_LOCAL_PENDING_DRAIN"
    );
    assert_eq!(
        halt_payloads[0]["halt_position"],
        1,
        "doc B is at 0-based index 1 in the backlog"
    );
}

// ─── Test 7: transient retry on pending-drain shift — NO halt ────────

/// MED-C4-6 fix: TransientRetry (Transport / Server-3) on a pending-
/// drain shift must NOT trigger Manual escalation.  Sibling continues;
/// shift stays in `OpenedLocalPendingDrain` for the next-tick retry
/// (spec §3.5: Manual is last resort; transient outcomes retain retry
/// budget).
#[tokio::test]
async fn c4_pending_drain_shift_transient_retry_sibling_continues_no_halt() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::OpenedLocalPendingDrain).await;
    let shift_id =
        seed_shift_with_state(&pool, CASHIER_OK, "OPENED_LOCAL_PENDING_DRAIN").await;
    set_node_current_shift(&pool, shift_id).await;
    let session_id = seed_offline_session(&pool, OfflineSessionState::Open).await;

    let doc_a =
        seed_complete_offline_local_ack(&pool, 1, 100, session_id, shift_id, CASHIER_OK).await;
    let doc_b =
        seed_complete_offline_local_ack(&pool, 2, 101, session_id, shift_id, CASHIER_OK).await;
    let doc_c =
        seed_complete_offline_local_ack(&pool, 3, 102, session_id, shift_id, CASHIER_OK).await;

    // Doc A: Ok wire reply → KVT1.  Doc B: Transport error →
    // RetryClass::TransientRetry → doc → ErrorRetryable.  Doc C: Ok
    // wire reply → KVT1.  Sibling-continue MUST apply; no halt; no
    // shift escalation.
    let carriers = carriers_with_responses(vec![
        Ok(ack("OK-A")),
        Err(DpsError::Transport("simulated link flap".into())),
        Ok(ack("OK-C")),
    ]);
    let view = view_for(&carriers);

    let summary = backlog_drain::drain(&pool, &view, FN).await.unwrap();

    assert_eq!(summary.backlog_size_before(), 3);
    assert_eq!(
        summary.advanced_to_kvt1(),
        2,
        "doc A and doc C both reached KVT1; doc B was transient retry (NOT halt)"
    );
    assert_eq!(summary.per_doc_failures().len(), 1);
    assert_eq!(
        summary.per_doc_failures()[0].1,
        "wire_routing_transient_retry"
    );
    // All 3 docs visited the wire — sibling-continue contract holds
    // even on pending-drain shift, because Transport is non-manual.
    assert_eq!(carriers.dps.call_count(), 3);

    // DB states.
    assert_eq!(read_doc_state(&pool, doc_a).await, "KVT1");
    assert_eq!(read_doc_state(&pool, doc_b).await, "ERROR_RETRYABLE");
    assert_eq!(read_doc_state(&pool, doc_c).await, "KVT1");

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
        failed_payloads[0]["retry_class"],
        "TransientRetry",
        "wire error_routing routed Transport → TransientRetry"
    );
    assert_eq!(
        failed_payloads[0]["manual_recon_class"],
        false,
        "TransientRetry is NOT manual-recon class — operator dashboards filter on this"
    );
}
