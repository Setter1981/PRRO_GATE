//! W10.5 — canonical routing-dispatch coverage for `stage_send::run`.
//!
//! Pure-DB integration via stub `DpsChannel` (mirror of the W7.5
//! stub pattern); each fixture invokes `stage_send::run` end-to-end
//! and asserts:
//!   - post-tx `fiscal_documents.state`
//!   - `transport_trace[].outcome_kind` + `retry_class` (durable
//!     migration 012 column)
//!   - `audit_log` event_type chain + payload discriminators
//!   - (where applicable) `node_state.mode` flip
//!   - (where applicable) `probe_hint` / `mac_recovery_hint` payload
//!
//! Anchored on:
//!   - W10 design freeze §4.5 (this file enumeration).
//!   - W0-3 §2 main table (8 DpsError variants) + §2.1 sub-table
//!     (12 Server-routed status codes).
//!   - W0-3 §9.2 (21 routing fixtures spec).
//!
//! **Scope discipline.**  This file is the canonical routing-dispatch
//! coverage matrix.  Some fixtures overlap with `write_path_stage4_send.rs`
//! (which covers Pattern B mechanics + the W10.2/W10.3 control surface);
//! that overlap is intentional — `write_path_stage4_send` proves the
//! plumbing, `write_path_dps_error_routing` proves the routing decision
//! matrix.
//!
//! **Naming convention** (R-W10.5-review NIT):
//!   - `fx01..fx21` — routing decisions per W0-3 §9.2 numbered list
//!     (matches the spec's enumeration 1-by-1 для cross-reference).
//!
//! CS-3 S7-1 (R3): the former `mac_fx01..mac_fx03` MAC-recovery dispatch
//! fixtures — and `fx17` (the `-12` "routes to mac_recovery decision" smoke,
//! which drove the orchestrator's Resigned→2nd-attempt path) — were RETIRED with
//! the inline MAC orchestrator loop (`-12 Resigned => continue`, no 2nd wire).
//! The `-12` contract is now a
//! `MacReseedPending` HELD (node → STOP_MODE, doc rests SENDING under a
//! `PENDING_APPLY` reservation); see the composed-run pin
//! `write_path_stage4_send::minus_12_bad_hash_prev_records_held_stop_no_second_wire`
//! and the record-boundary pin `record_outcome::rc05_bad_hash_prev_held_stop_mode`.

use std::sync::{Arc, Mutex};

use sqlx::SqlitePool;

use prro::db::models::enums::{DocState, NodeMode};
use prro::db::models::ids::DocumentId;
use prro::db::repositories::{node_state, transport_trace};
use prro::db::types::DbDocumentId;
use prro::services::write_path::error_routing::{ProbeReason, RetryClass};
use prro::services::write_path::stage_send::{self, StageSendOutcome};
use prro::transports::dps::error::{AuthorizationKind, DpsError};

mod common;
use common::StubDpsChannel;

// ─── Pool / seed helpers ─────────────────────────────────────────────

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations");
    (dir, pool)
}

const SELL_PAYLOAD_JSON: &str = r#"{
    "items": [{"code": "p-1", "name": "Item A", "price_kop": 1500, "quantity_thousandths": 1000, "sum_kop": 1500}],
    "payments": [{"name": "Cash", "sum_kop": 1500, "type_code": "0"}]
}"#;

const Z_REPORT_PAYLOAD_JSON: &str = r#"{"payments":[{"name":"CASH","sum_in_kop":15000,"sum_out_kop":0,"type_code":"0"}],"sell_count":1,"return_count":0}"#;

/// Seed FN config + a doc in the given `state` with PAYLOAD_XML +
/// SIGNED_XML pre-INSERTed (mirrors W6 stage 3-PERSIST commit shape).
/// `lnd` is forced to `doc_byte` to dodge the partial UNIQUE
/// `(fiscal_number, lnd)` index.
async fn seed_doc(
    pool: &SqlitePool,
    doc_byte: u8,
    doc_type: &str,
    state: &str,
    payload_json: &str,
    total_sum_kop: Option<i64>,
) -> DocumentId {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(pool)
    .await
    .unwrap();

    // CS-3 S7-1 cutover: the composed send path requires a `node_state` row (P3 pre-wire chain-seed
    // equality + `authorize_submission`'s generation advance). Seed one with a NULL
    // `last_known_unsigned_xml_sha256` — it matches these docs' NULL `previous_hash` (a fresh online
    // chain), so P3 passes. Production always has a node_state row for an online FN.
    sqlx::query(
        "INSERT OR IGNORE INTO node_state (fiscal_number, mode, shift_state, next_lnd) \
         VALUES ('1234567890', 'ONLINE', 'OPENED', 1)",
    )
    .execute(pool)
    .await
    .unwrap();

    // W14a-2b Commit 5: non-bypass doc types need shift + signer
    // attribution for signer_guard at stage_send 4-pre.
    let bypass = matches!(doc_type, "SHIFT_OPEN" | "SHIFT_CLOSE" | "Z_REPORT");
    let (shift_id_bytes, cashier): (Option<Vec<u8>>, Option<&'static str>) = if bypass {
        (None, None)
    } else {
        let shift_byte = doc_byte ^ 0x80;
        let shift_bytes = vec![shift_byte; 16];
        sqlx::query(
            "INSERT OR IGNORE INTO shifts(shift_id, fiscal_number, serial, state, \
                open_mode, cash_balance_kop, opened_by_cashier_id) \
             VALUES (?, '1234567890', 1, 'OPENED', 'ONLINE', 0, 'test-cashier')",
        )
        .bind(&shift_bytes)
        .execute(pool)
        .await
        .expect("seed shift for non-bypass doc");
        (Some(shift_bytes), Some("test-cashier"))
    };

    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let sha = vec![0u8; 32];
    let lnd = doc_byte as i64;
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, shift_id, lnd, \
            doc_type, state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, total_sum_kop, signed_by_cashier_id) \
         VALUES (?, ?, '1234567890', ?, ?, ?, ?, 'b1', 't1', 'ONLINE', \
            '2026-05-09T12:34:56Z', ?, ?, ?, ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(shift_id_bytes.as_deref())
    .bind(lnd)
    .bind(doc_type)
    .bind(state)
    .bind(payload_json)
    .bind(&sha)
    .bind(total_sum_kop)
    .bind(cashier)
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
    sqlx::query(
        "INSERT INTO document_files(document_id, kind, content) \
         VALUES (?, 'PAYLOAD_XML', ?), (?, 'SIGNED_XML', ?)",
    )
    .bind(&doc_bytes)
    .bind(b"PAYLOAD".to_vec())
    .bind(&doc_bytes)
    .bind(b"SIGNED-CMS".to_vec())
    .execute(pool)
    .await
    .expect("seed document_files");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

async fn seed_signed_sell(pool: &SqlitePool, doc_byte: u8) -> DocumentId {
    seed_doc(
        pool,
        doc_byte,
        "SELL",
        "SIGNED",
        SELL_PAYLOAD_JSON,
        Some(1500),
    )
    .await
}

async fn seed_signed_zreport(pool: &SqlitePool, doc_byte: u8) -> DocumentId {
    seed_doc(
        pool,
        doc_byte,
        "Z_REPORT",
        "SIGNED",
        Z_REPORT_PAYLOAD_JSON,
        None,
    )
    .await
}

async fn seed_signed_shift_close(pool: &SqlitePool, doc_byte: u8) -> DocumentId {
    seed_doc(
        pool,
        doc_byte,
        "SHIFT_CLOSE",
        "SIGNED",
        Z_REPORT_PAYLOAD_JSON,
        None,
    )
    .await
}

// ─── Read helpers ────────────────────────────────────────────────────

async fn read_state(pool: &SqlitePool, doc: DocumentId) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(DbDocumentId(doc))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_audit_chain(
    pool: &SqlitePool,
    doc: DocumentId,
) -> Vec<(String, String, Option<String>)> {
    let entity_id = format!("{doc:?}");
    sqlx::query_as(
        "SELECT event_type, severity, event_payload_json FROM audit_log \
         WHERE entity_type = 'fiscal_document' AND entity_id = ? \
         ORDER BY audit_id",
    )
    .bind(&entity_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn last_trace(pool: &SqlitePool, doc: DocumentId) -> transport_trace::TraceRow {
    let traces = transport_trace::list_for_document(pool, doc).await.unwrap();
    traces.into_iter().last().expect("at least one trace row")
}

fn assert_audit_event(
    chain: &[(String, String, Option<String>)],
    event: &str,
    expected_severity: &str,
) -> String {
    let row = chain
        .iter()
        .find(|(e, _, _)| e == event)
        .unwrap_or_else(|| panic!("expected audit event {event} in chain {chain:?}"));
    assert_eq!(row.1, expected_severity, "{event} severity");
    row.2.clone().unwrap_or_default()
}

// ─── §2 main table fixtures (8 DpsError variants) ────────────────────

// Fixture 1 — Transport
#[tokio::test]
async fn fx01_transport_routes_to_error_retryable_with_transient_class() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_sell(&pool, 0x01).await;
    let stub = StubDpsChannel::new(Err(DpsError::Transport("TLS reset".into())));

    let outcome = stage_send::run(&pool, &stub, doc, None).await.unwrap();
    match outcome {
        StageSendOutcome::Routed { decision, .. } => {
            assert_eq!(decision.target_state, DocState::ErrorRetryable);
            assert_eq!(decision.retry_class, RetryClass::TransientRetry);
        }
        other => panic!("expected Routed transient, got {other:?}"),
    }
    // CS-3 S7-1: held SENDING (SubmittedUnknown), no ER redrive
    assert_eq!(read_state(&pool, doc).await, "SENDING");
    let trace = last_trace(&pool, doc).await;
    assert_eq!(trace.outcome_kind.as_deref(), Some("RETRYABLE_TRANSPORT"));
    assert_eq!(trace.retry_class.as_deref(), Some("TransientRetry"));
    let chain = read_audit_chain(&pool, doc).await;
    assert_audit_event(&chain, "STAGE_SEND_TRANSIENT_RETRY", "WARNING");
}

// Fixture 2 — Authorization{DocumentReject, -1}
#[tokio::test]
async fn fx02_authorization_document_reject_routes_to_terminal_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_sell(&pool, 0x02).await;
    let stub = StubDpsChannel::new(Err(DpsError::Authorization {
        code: -1,
        kind: AuthorizationKind::DocumentReject,
        message: "ERROR_VEREFY".into(),
    }));

    stage_send::run(&pool, &stub, doc, None).await.unwrap();
    assert_eq!(read_state(&pool, doc).await, "REJECTED");
    let trace = last_trace(&pool, doc).await;
    assert_eq!(trace.outcome_kind.as_deref(), Some("REJECTED"));
    assert_eq!(trace.retry_class.as_deref(), Some("TerminalReject"));
    assert_eq!(trace.server_status_code, Some(-1));
    let chain = read_audit_chain(&pool, doc).await;
    let payload = assert_audit_event(&chain, "STAGE_SEND_REJECTED", "ERROR");
    assert!(payload.contains("\"retry_class\":\"TerminalReject\""));
}

// Fixture 3 — Authorization{FnNotRegistered, -13}
#[tokio::test]
async fn fx03_authorization_fn_not_registered_minus_13_routes_to_fn_config_error() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_sell(&pool, 0x03).await;
    let stub = StubDpsChannel::new(Err(DpsError::Authorization {
        code: -13,
        kind: AuthorizationKind::FiscalNumberNotRegistered,
        message: "ERROR_NOT_REGISTERED_RRO".into(),
    }));

    stage_send::run(&pool, &stub, doc, None).await.unwrap();
    assert_eq!(read_state(&pool, doc).await, "ERROR_RETRYABLE");
    let trace = last_trace(&pool, doc).await;
    assert_eq!(trace.outcome_kind.as_deref(), Some("RETRYABLE_AUTH_FN"));
    assert_eq!(trace.retry_class.as_deref(), Some("FnConfigError"));
    assert_eq!(trace.server_status_code, Some(-13));
    let chain = read_audit_chain(&pool, doc).await;
    assert_audit_event(&chain, "STAGE_SEND_FN_NOT_REGISTERED", "ERROR");
}

// Fixture 4 — Authorization{FnNotRegistered, -14}
#[tokio::test]
async fn fx04_authorization_fn_not_registered_minus_14_routes_to_fn_config_error() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_sell(&pool, 0x04).await;
    let stub = StubDpsChannel::new(Err(DpsError::Authorization {
        code: -14,
        kind: AuthorizationKind::FiscalNumberNotRegistered,
        message: "ERROR_NOT_REGISTERED_SIGNER".into(),
    }));

    stage_send::run(&pool, &stub, doc, None).await.unwrap();
    let trace = last_trace(&pool, doc).await;
    assert_eq!(trace.retry_class.as_deref(), Some("FnConfigError"));
    assert_eq!(trace.server_status_code, Some(-14));
    let chain = read_audit_chain(&pool, doc).await;
    assert_audit_event(&chain, "STAGE_SEND_FN_NOT_REGISTERED", "ERROR");
}

// Fixture 5 — Decode (status=0) → ProbeRequired
#[tokio::test]
async fn fx05_decode_routes_to_probe_required_with_decode_unknown_hint() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_sell(&pool, 0x05).await;
    let stub = StubDpsChannel::new(Err(DpsError::Decode("status=0 UNKNOWN".into())));

    let outcome = stage_send::run(&pool, &stub, doc, None).await.unwrap();
    match outcome {
        StageSendOutcome::Routed { decision, .. } => {
            assert_eq!(decision.retry_class, RetryClass::ProbeRequired);
            assert_eq!(
                decision.probe_hint.as_ref().map(|h| h.reason),
                Some(ProbeReason::DecodeUnknown)
            );
        }
        other => panic!("expected Routed probe-required, got {other:?}"),
    }
    let chain = read_audit_chain(&pool, doc).await;
    let payload = assert_audit_event(&chain, "STAGE_SEND_DECODE_UNKNOWN", "WARNING");
    assert!(payload.contains("\"probe_hint\":\"DecodeUnknown\""));
}

// Fixture 6 — NotFound on live → WrapperBug (B2 close)
#[tokio::test]
async fn fx06_not_found_on_live_routes_to_wrapper_bug_critical() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_sell(&pool, 0x06).await;
    let stub = StubDpsChannel::new(Err(DpsError::NotFound));

    stage_send::run(&pool, &stub, doc, None).await.unwrap();
    // CS-3 S7-1: the composed HOLD contract supersedes the old B2-close — a wrapper-bug with no faithful DPS envelope rests SENDING-held (STOP); it is still surfaced by the CRITICAL WrapperBug audit below.
    let state = read_state(&pool, doc).await;
    assert_eq!(state, "SENDING");
    let trace = last_trace(&pool, doc).await;
    assert_eq!(trace.retry_class.as_deref(), Some("WrapperBug"));
    let chain = read_audit_chain(&pool, doc).await;
    assert_audit_event(&chain, "STAGE_SEND_WRAPPER_BUG", "CRITICAL");
}

// Fixture 7 — ServerFiscalIdMismatch
#[tokio::test]
async fn fx07_server_fiscal_id_mismatch_routes_to_wrapper_bug_distinct_event() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_sell(&pool, 0x07).await;
    let stub = StubDpsChannel::new(Err(DpsError::ServerFiscalIdMismatch {
        expected_id: "EXPECTED-FN".into(),
        actual_id: "WRONG-FN".into(),
    }));

    stage_send::run(&pool, &stub, doc, None).await.unwrap();
    // CS-3 S7-1: the composed HOLD contract supersedes the old B2-close — a wrapper-bug with no faithful DPS envelope rests SENDING-held (STOP); it is still surfaced by the CRITICAL WrapperBug audit below.
    assert_eq!(read_state(&pool, doc).await, "SENDING");
    let trace = last_trace(&pool, doc).await;
    assert_eq!(trace.retry_class.as_deref(), Some("WrapperBug"));
    let chain = read_audit_chain(&pool, doc).await;
    // PRRO_GATE-5js: distinct from generic WrapperBug for forensics.
    assert_audit_event(&chain, "STAGE_SEND_FISCAL_ID_MISMATCH", "CRITICAL");
}

// Fixture 8 — QueryNotSupported on live → WrapperBug (B2 close)
#[tokio::test]
async fn fx08_query_not_supported_on_live_routes_to_wrapper_bug_critical() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_sell(&pool, 0x08).await;
    let stub = StubDpsChannel::new(Err(DpsError::QueryNotSupported("ByLocalIdentity")));

    stage_send::run(&pool, &stub, doc, None).await.unwrap();
    // CS-3 S7-1: the composed HOLD contract supersedes the old B2-close — a wrapper-bug with no faithful DPS envelope rests SENDING-held (STOP); it is still surfaced by the CRITICAL WrapperBug audit below.
    let state = read_state(&pool, doc).await;
    assert_eq!(state, "SENDING");
    let trace = last_trace(&pool, doc).await;
    assert_eq!(trace.retry_class.as_deref(), Some("WrapperBug"));
    let chain = read_audit_chain(&pool, doc).await;
    assert_audit_event(&chain, "STAGE_SEND_WRAPPER_BUG", "CRITICAL");
}

// Fixture 9 — Internal
#[tokio::test]
async fn fx09_internal_routes_to_wrapper_bug_critical() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_sell(&pool, 0x09).await;
    let stub = StubDpsChannel::new(Err(DpsError::Internal("wrapper bug surfaced".into())));

    stage_send::run(&pool, &stub, doc, None).await.unwrap();
    // CS-3 S7-1: the composed HOLD contract supersedes the old B2-close — a wrapper-bug with no faithful DPS envelope rests SENDING-held (STOP); it is still surfaced by the CRITICAL WrapperBug audit below.
    assert_eq!(read_state(&pool, doc).await, "SENDING");
    let trace = last_trace(&pool, doc).await;
    assert_eq!(trace.retry_class.as_deref(), Some("WrapperBug"));
    let chain = read_audit_chain(&pool, doc).await;
    assert_audit_event(&chain, "STAGE_SEND_WRAPPER_BUG", "CRITICAL");
}

// ─── §2.1 sub-table fixtures (Server status codes) ───────────────────

// Fixture 10 — Server{-2} non-shift → terminal Rejected
#[tokio::test]
async fn fx10_server_minus_2_non_shift_routes_to_terminal_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_sell(&pool, 0x10).await;
    let stub = StubDpsChannel::new(Err(DpsError::Server {
        code: -2,
        message: "ERROR_CHECK".into(),
    }));

    stage_send::run(&pool, &stub, doc, None).await.unwrap();
    assert_eq!(read_state(&pool, doc).await, "REJECTED");
    let trace = last_trace(&pool, doc).await;
    assert_eq!(trace.retry_class.as_deref(), Some("TerminalReject"));
    assert_eq!(trace.server_status_code, Some(-2));
    let chain = read_audit_chain(&pool, doc).await;
    assert_audit_event(&chain, "STAGE_SEND_REJECTED", "CRITICAL");
}

// Fixture 11 — Server{-2} close-shift → ProbeRequired
#[tokio::test]
async fn fx11_server_minus_2_close_shift_routes_to_probe_required() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_zreport(&pool, 0x11).await;
    let stub = StubDpsChannel::new(Err(DpsError::Server {
        code: -2,
        message: "ERROR_CHECK".into(),
    }));

    let outcome = stage_send::run(&pool, &stub, doc, None).await.unwrap();
    match outcome {
        StageSendOutcome::Routed { decision, .. } => {
            assert_eq!(decision.retry_class, RetryClass::ProbeRequired);
            assert_eq!(
                decision.probe_hint.as_ref().map(|h| h.reason),
                Some(ProbeReason::CloseShiftProbe),
                "close-shift -2 must carry CloseShiftProbe probe hint"
            );
        }
        other => panic!("expected Routed probe, got {other:?}"),
    }
    // CS-3 S7-1: held SENDING (SubmittedUnknown), no ER redrive
    assert_eq!(read_state(&pool, doc).await, "SENDING");
    let chain = read_audit_chain(&pool, doc).await;
    let payload = assert_audit_event(&chain, "STAGE_SEND_PROBE_REQUIRED", "WARNING");
    assert!(payload.contains("\"probe_hint\":\"CloseShiftProbe\""));
}

// CS-3 Slice E (Pin 1) — RED-first: -2 and -15 close-shift collapse to ONE probe reason
// (CloseShiftProbe). Before the merge they carry distinct CloseShiftProbe / CloseShiftProbe;
// after, both must be EQUAL. Rename-agnostic — compares the two reasons, names no variant.
#[tokio::test]
async fn slice_e_close_shift_probe_unifies_minus_2_and_minus_15() {
    let (_d2, pool2) = fresh_pool().await;
    let doc2 = seed_signed_zreport(&pool2, 0xE2).await;
    let stub2 = StubDpsChannel::new(Err(DpsError::Server {
        code: -2,
        message: "ERROR_CHECK".into(),
    }));
    let r2 = match stage_send::run(&pool2, &stub2, doc2, None).await.unwrap() {
        StageSendOutcome::Routed { decision, .. } => {
            assert_eq!(decision.retry_class, RetryClass::ProbeRequired);
            decision.probe_hint.as_ref().map(|h| h.reason)
        }
        other => panic!("expected Routed probe for -2, got {other:?}"),
    };

    let (_d15, pool15) = fresh_pool().await;
    let doc15 = seed_signed_zreport(&pool15, 0xE5).await;
    let stub15 = StubDpsChannel::new(Err(DpsError::Server {
        code: -15,
        message: "ERROR_NOT_OPEN_SHIFT".into(),
    }));
    let r15 = match stage_send::run(&pool15, &stub15, doc15, None).await.unwrap() {
        StageSendOutcome::Routed { decision, .. } => {
            assert_eq!(decision.retry_class, RetryClass::ProbeRequired);
            decision.probe_hint.as_ref().map(|h| h.reason)
        }
        other => panic!("expected Routed probe for -15, got {other:?}"),
    };

    assert!(
        r2.is_some() && r2 == r15,
        "Slice E: -2 and -15 close-shift must collapse to one CloseShiftProbe reason; got {r2:?} vs {r15:?}"
    );
}

// Fixture 12 — Server{-3} → transient retry
#[tokio::test]
async fn fx12_server_minus_3_routes_to_transient_retry() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_sell(&pool, 0x12).await;
    let stub = StubDpsChannel::new(Err(DpsError::Server {
        code: -3,
        message: "ERROR_SAVE".into(),
    }));

    stage_send::run(&pool, &stub, doc, None).await.unwrap();
    // CS-3 S7-1: held SENDING (SubmittedUnknown), no ER redrive
    assert_eq!(read_state(&pool, doc).await, "SENDING");
    let trace = last_trace(&pool, doc).await;
    // -3 routes Server-class transient → RETRYABLE_SERVER (not Transport).
    assert_eq!(trace.outcome_kind.as_deref(), Some("RETRYABLE_SERVER"));
    assert_eq!(trace.retry_class.as_deref(), Some("TransientRetry"));
    let chain = read_audit_chain(&pool, doc).await;
    assert_audit_event(&chain, "STAGE_SEND_TRANSIENT_RETRY", "WARNING");
}

// Fixture 13 — Server{-5} → terminal Rejected
#[tokio::test]
async fn fx13_server_minus_5_routes_to_terminal_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_sell(&pool, 0x13).await;
    let stub = StubDpsChannel::new(Err(DpsError::Server {
        code: -5,
        message: "ERROR_TYPE".into(),
    }));

    stage_send::run(&pool, &stub, doc, None).await.unwrap();
    assert_eq!(read_state(&pool, doc).await, "REJECTED");
    let trace = last_trace(&pool, doc).await;
    assert_eq!(trace.retry_class.as_deref(), Some("TerminalReject"));
    let chain = read_audit_chain(&pool, doc).await;
    assert_audit_event(&chain, "STAGE_SEND_REJECTED", "CRITICAL");
}

// Fixture 14 — Server{-6} → operator escalation
#[tokio::test]
async fn fx14_server_minus_6_routes_to_operator_escalation() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_sell(&pool, 0x14).await;
    let stub = StubDpsChannel::new(Err(DpsError::Server {
        code: -6,
        message: "ERROR_NOT_PREV_ZREPORT".into(),
    }));

    stage_send::run(&pool, &stub, doc, None).await.unwrap();
    // CS-3 S7-1: held SENDING (SubmittedUnknown), no ER redrive
    assert_eq!(read_state(&pool, doc).await, "SENDING");
    let trace = last_trace(&pool, doc).await;
    assert_eq!(trace.retry_class.as_deref(), Some("OperatorEscalation"));
    let chain = read_audit_chain(&pool, doc).await;
    assert_audit_event(&chain, "STAGE_SEND_OPERATOR_ESCALATION", "ERROR");
}

// Fixture 15 — Server{-7..-10} XML-class → terminal Rejected
#[tokio::test]
async fn fx15_server_minus_7_to_minus_10_xml_class_routes_to_terminal() {
    for (i, code) in [-7, -8, -9, -10].iter().enumerate() {
        let (_d, pool) = fresh_pool().await;
        let doc_byte = 0x20 + i as u8;
        let doc = seed_signed_sell(&pool, doc_byte).await;
        let stub = StubDpsChannel::new(Err(DpsError::Server {
            code: *code,
            message: format!("ERROR_XML_{i}"),
        }));

        stage_send::run(&pool, &stub, doc, None).await.unwrap();
        assert_eq!(read_state(&pool, doc).await, "REJECTED", "code {code}");
        let trace = last_trace(&pool, doc).await;
        assert_eq!(
            trace.retry_class.as_deref(),
            Some("TerminalReject"),
            "code {code}"
        );
        let chain = read_audit_chain(&pool, doc).await;
        assert_audit_event(&chain, "STAGE_SEND_REJECTED", "CRITICAL");
    }
}

// Fixture 16 — Server{-11} → terminal Rejected + node BLOCKED flip
#[tokio::test]
async fn fx16_server_minus_11_routes_to_rejected_and_flips_node_blocked() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_sell(&pool, 0x16).await;
    // node_state row required for the W10.3 flip.
    node_state::upsert_initial(
        &pool,
        "1234567890",
        NodeMode::Online,
        prro::db::models::enums::ShiftState::Closed,
        1,
    )
    .await
    .unwrap();
    // R-W10.5-review LOW 4 close: pin pre-flip baseline so the
    // direction Online → Blocked is documented by the test, not
    // just by the upsert_initial argument.  A future refactor that
    // accidentally seeds Blocked would flip the post-assertion
    // semantically, but this pre-pin would catch it.
    let pre = node_state::get(&pool, "1234567890")
        .await
        .unwrap()
        .expect("node_state row must exist");
    assert_eq!(pre.mode, NodeMode::Online, "pre-flip baseline = Online");
    let stub = StubDpsChannel::new(Err(DpsError::Server {
        code: -11,
        message: "ERROR_OFFLINE_168".into(),
    }));

    stage_send::run(&pool, &stub, doc, None).await.unwrap();
    assert_eq!(read_state(&pool, doc).await, "REJECTED");
    let trace = last_trace(&pool, doc).await;
    assert_eq!(trace.retry_class.as_deref(), Some("TerminalReject"));
    let chain = read_audit_chain(&pool, doc).await;
    let payload = assert_audit_event(&chain, "STAGE_SEND_NODE_BLOCKED", "CRITICAL");
    assert!(
        payload.contains("\"node_mode_flipped\":\"Blocked\""),
        "payload must carry node_mode_flipped: {payload}"
    );
    let row = node_state::get(&pool, "1234567890").await.unwrap().unwrap();
    assert_eq!(
        row.mode,
        NodeMode::Blocked,
        "Server -11 MUST flip node_state.mode → BLOCKED"
    );
}

// Fixture 18 — Server{-15} non-shift → terminal Rejected
#[tokio::test]
async fn fx18_server_minus_15_non_shift_routes_to_terminal_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_sell(&pool, 0x18).await;
    let stub = StubDpsChannel::new(Err(DpsError::Server {
        code: -15,
        message: "ERROR_NOT_OPEN_SHIFT".into(),
    }));

    stage_send::run(&pool, &stub, doc, None).await.unwrap();
    assert_eq!(read_state(&pool, doc).await, "REJECTED");
    let trace = last_trace(&pool, doc).await;
    assert_eq!(trace.retry_class.as_deref(), Some("TerminalReject"));
    let chain = read_audit_chain(&pool, doc).await;
    assert_audit_event(&chain, "STAGE_SEND_REJECTED", "CRITICAL");
}

// Fixture 19 — Server{-15} close-shift → ProbeRequired
#[tokio::test]
async fn fx19_server_minus_15_close_shift_routes_to_probe_required() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_shift_close(&pool, 0x19).await;
    let stub = StubDpsChannel::new(Err(DpsError::Server {
        code: -15,
        message: "ERROR_NOT_OPEN_SHIFT".into(),
    }));

    let outcome = stage_send::run(&pool, &stub, doc, None).await.unwrap();
    match outcome {
        StageSendOutcome::Routed { decision, .. } => {
            assert_eq!(decision.retry_class, RetryClass::ProbeRequired);
            assert_eq!(
                decision.probe_hint.as_ref().map(|h| h.reason),
                Some(ProbeReason::CloseShiftProbe),
            );
        }
        other => panic!("expected Routed probe, got {other:?}"),
    }
    // CS-3 S7-1: held SENDING (SubmittedUnknown), no ER redrive
    assert_eq!(read_state(&pool, doc).await, "SENDING");
    let chain = read_audit_chain(&pool, doc).await;
    let payload = assert_audit_event(&chain, "STAGE_SEND_PROBE_REQUIRED", "WARNING");
    assert!(payload.contains("\"probe_hint\":\"CloseShiftProbe\""));
}

// Fixture 20 — Server{-16} M3a → terminal Rejected + alert
#[tokio::test]
async fn fx20_server_minus_16_m3a_routes_to_rejected_alert() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_sell(&pool, 0x1A).await;
    let stub = StubDpsChannel::new(Err(DpsError::Server {
        code: -16,
        message: "ERROR_OFFLINE_ID".into(),
    }));

    stage_send::run(&pool, &stub, doc, None).await.unwrap();
    assert_eq!(read_state(&pool, doc).await, "REJECTED");
    let trace = last_trace(&pool, doc).await;
    assert_eq!(trace.retry_class.as_deref(), Some("TerminalReject"));
    let chain = read_audit_chain(&pool, doc).await;
    assert_audit_event(&chain, "STAGE_SEND_REJECTED", "CRITICAL");
}

// ─── Fixture 21 — Pattern B retry-path proof ─────────────────────────

#[tokio::test]
async fn fx21_pattern_b_retry_path_spy_observes_sending_marker_then_error_retryable() {
    // R-W10.5: prove that M3a NEVER takes a direct `(ErrorRetryable, Sent)`
    // issue-forward hop (the edge is REMOVED entirely as of A.3 PR-B step 6) —
    // every wire send must pass through the `Sending` durable marker.  Spy fires inside
    // `send_chk` (4a, no lock) and reads the doc state via a fresh
    // pool acquire — must see SENDING (4-pre committed) before the
    // wire response routes to ErrorRetryable.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_sell(&pool, 0xD1).await;

    let observed: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let pool_for_spy = pool.clone();
    let observed_for_spy = Arc::clone(&observed);
    let stub = StubDpsChannel::with_spy(
        Err(DpsError::Transport("TLS reset for retry-path proof".into())),
        Box::new(move || {
            let pool = pool_for_spy.clone();
            let observed = Arc::clone(&observed_for_spy);
            let handle = std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                rt.block_on(async move {
                    let row: String = sqlx::query_scalar(
                        "SELECT state FROM fiscal_documents WHERE document_id = ?",
                    )
                    .bind(DbDocumentId(doc))
                    .fetch_one(&pool)
                    .await
                    .unwrap();
                    *observed.lock().unwrap() = Some(row);
                });
            });
            handle.join().unwrap();
        }),
    );

    stage_send::run(&pool, &stub, doc, None).await.unwrap();
    assert_eq!(
        observed.lock().unwrap().as_deref(),
        Some("SENDING"),
        "Pattern B contract: spy MUST observe SENDING during wire call"
    );
    // CS-3 S7-1: post-run the transient (Transport → SubmittedUnknown) is HELD in SENDING
    // (PENDING_APPLY + STOP), NOT routed to ErrorRetryable — the ER-redrive edge is retired
    // (R1/R2), so the Pattern-B SENDING marker now legitimately RESTS (held), exempt in
    // invariant_scan via the active PENDING_APPLY reservation witness.
    assert_eq!(read_state(&pool, doc).await, "SENDING");
    assert_eq!(stub.call_count(), 1);
}
