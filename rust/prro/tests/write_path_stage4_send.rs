//! W7.5 — stage 4 send integration fixtures.
//!
//! In-memory `StubDpsChannel` (no tonic mock server — those live in
//! `dps_channel_smoke.rs`).  Each fixture exercises a single
//! `stage_send::run` invocation and asserts the full Pattern B shape:
//!
//!   1. `happy_sent` — full Sent path: `Signed → Sending → Sent`,
//!      `server_fiscal_no` persisted, trace OK + audit pair.
//!   2. `terminal_reject` — `Authorization{DocumentReject, code=-1}`
//!      → `Sending → Rejected`, trace REJECTED, no `server_fiscal_no`.
//!   3. `transport_retryable` — `Transport(...)` →
//!      `Sending → ErrorRetryable`, trace RETRYABLE_TRANSPORT,
//!      `submission_attempted_at` preserved.
//!   4. `pattern_b_ordering` — spy callback inside `send_chk` reads
//!      `fiscal_documents.state` via a fresh connection and asserts
//!      `'SENDING'` (Pattern B durable-marker proof).  Post-run
//!      asserts `'SENT'`.  Wall-clock-independent.
//!   5. `rerun_on_sent_state_conflict` — pre-seed doc as `Sent`,
//!      expect `StageSendOutcome::StateConflict { observed: Sent }`
//!      with **0** `send_chk` invocations (counter on stub).
//!   6. `whitelist_signed_sending_regression` — closes W7.4 F4: a
//!      future migration that drops `(Signed, Sending)` from the
//!      whitelist would make the `unreachable!` in 4-pre fire in
//!      production; this test catches it at CI.

use std::sync::{Arc, Mutex};

use sqlx::SqlitePool;

use prro::db::models::enums::DocState;
use prro::db::models::ids::DocumentId;
use prro::db::repositories::fiscal_documents::allowed_transition;
use prro::db::repositories::transport_trace;
use prro::db::types::DbDocumentId;
use prro::services::write_path::error_routing::RetryClass;
use prro::services::write_path::stage_send::{self, StageSendError, StageSendOutcome};
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{CheckAck, CheckEnvelope, CheckSignBlob};
use prro::transports::dps::error::{AuthorizationKind, DpsError};

mod common;
use common::StubDpsChannel;

// ─── Test seed helpers ───────────────────────────────────────────────

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations");
    (dir, pool)
}

/// Seed an FN config + a SIGNED `fiscal_documents` row + the
/// SIGNED_XML `document_files` artifact.  Mirrors the post-W6 W7
/// hand-off shape.  `lnd` parameterised so callers can avoid the
/// `(fiscal_number, lnd)` partial UNIQUE index.
async fn seed_signed_doc_with_xml(
    pool: &SqlitePool,
    doc_byte: u8,
    doc_type: &str,
    lnd: i64,
    state: &str,
) -> DocumentId {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(pool)
    .await
    .unwrap();

    // W14a-2b Commit 5: non-bypass doc types need a matching shift +
    // signer attribution so signer_guard at stage_send 4-pre passes
    // (signer == opening cashier).  Bypass doc types (SHIFT_OPEN /
    // SHIFT_CLOSE / Z_REPORT) skip signer enforcement.
    let bypass = matches!(doc_type, "SHIFT_OPEN" | "SHIFT_CLOSE" | "Z_REPORT");
    let (shift_id_bytes, cashier): (Option<Vec<u8>>, Option<&'static str>) = if bypass {
        (None, None)
    } else {
        // RS-3 C2 (migration 023): at most ONE active shift per FN.  All
        // non-bypass docs for this FN share ONE OPENED shift (fixed id) —
        // INSERT OR IGNORE de-dups, and the uq index forbids a second active
        // shift per FN.  The per-doc shift identity is irrelevant here (these
        // tests assert doc-state transitions, not shift linkage).
        let shift_bytes = vec![0x77u8; 16];
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
    // A.3: online-origin docs advance the chain seed at the Sending→Sent CAS, so
    // stage_send now reads node_state + this doc's unsigned_xml_sha256.  Seed a
    // GENESIS node_state (last_known = NULL) so the drift-assert passes
    // (ns.seed == doc.previous_hash == NULL) and the advance target exists.
    let unsigned = vec![doc_byte; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, shift_id, lnd, \
            doc_type, state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, unsigned_xml_sha256, signed_by_cashier_id) \
         VALUES (?, ?, '1234567890', ?, ?, ?, ?, 'b1', 't1', 'ONLINE', \
            '2026-05-09T12:34:56Z', '{}', ?, ?, ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(shift_id_bytes.as_deref())
    .bind(lnd)
    .bind(doc_type)
    .bind(state)
    .bind(&sha)
    .bind(&unsigned)
    .bind(cashier)
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
    sqlx::query(
        "INSERT OR IGNORE INTO node_state \
            (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
             backend_profile_id, transport_profile_id, last_known_unsigned_xml_sha256) \
         VALUES ('1234567890', 'ONLINE', 'OPENED', ?, 1, 'b1', 't1', NULL)",
    )
    .bind(shift_id_bytes.as_deref())
    .execute(pool)
    .await
    .expect("seed node_state (genesis seed)");
    sqlx::query(
        "INSERT INTO document_files(document_id, kind, content) \
         VALUES (?, 'SIGNED_XML', ?)",
    )
    .bind(&doc_bytes)
    .bind(b"FAKE-CMS-SIGNED-PAYLOAD".to_vec())
    .execute(pool)
    .await
    .expect("seed document_files SIGNED_XML");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

async fn read_doc_state(pool: &SqlitePool, doc: DocumentId) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(DbDocumentId(doc))
        .fetch_one(pool)
        .await
        .expect("read state")
}

async fn read_server_fiscal_no(pool: &SqlitePool, doc: DocumentId) -> Option<String> {
    sqlx::query_scalar("SELECT server_fiscal_no FROM fiscal_documents WHERE document_id = ?")
        .bind(DbDocumentId(doc))
        .fetch_one(pool)
        .await
        .expect("read server_fiscal_no")
}

async fn read_submission_attempted_at(pool: &SqlitePool, doc: DocumentId) -> Option<String> {
    sqlx::query_scalar("SELECT submission_attempted_at FROM fiscal_documents WHERE document_id = ?")
        .bind(DbDocumentId(doc))
        .fetch_one(pool)
        .await
        .expect("read submission_attempted_at")
}

async fn read_audit_event_types(pool: &SqlitePool, doc: DocumentId) -> Vec<String> {
    let entity_id = format!("{doc:?}");
    sqlx::query_scalar(
        "SELECT event_type FROM audit_log \
         WHERE entity_type = 'fiscal_document' AND entity_id = ? \
         ORDER BY audit_id",
    )
    .bind(&entity_id)
    .fetch_all(pool)
    .await
    .expect("read audit_log")
}

fn ack(id: &str) -> CheckAck {
    CheckAck {
        id: id.into(),
        id_sign: vec![],
        data_sign: vec![],
    }
}

// ─── Fixture 1 — happy_sent ──────────────────────────────────────────

#[tokio::test]
async fn happy_sent_full_pattern_b_round_trip() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_doc_with_xml(&pool, 0x11, "SELL", 1, "SIGNED").await;
    let stub = StubDpsChannel::new(Ok(ack("DPS-FN-7777")));

    let outcome = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect("happy path must succeed");

    // Outcome shape.
    let attempt_no = match outcome {
        StageSendOutcome::Sent {
            server_fiscal_no,
            attempt_no,
        } => {
            assert_eq!(server_fiscal_no, "DPS-FN-7777");
            attempt_no
        }
        other => panic!("expected Sent, got {other:?}"),
    };
    assert_eq!(attempt_no, 1);
    assert_eq!(stub.call_count(), 1);

    // Persisted state.
    assert_eq!(read_doc_state(&pool, doc).await, "SENT");
    assert_eq!(
        read_server_fiscal_no(&pool, doc).await.as_deref(),
        Some("DPS-FN-7777")
    );
    assert!(
        read_submission_attempted_at(&pool, doc).await.is_some(),
        "submission_attempted_at must be set in 4-pre"
    );

    // Trace shape.
    let traces = transport_trace::list_for_document(&pool, doc)
        .await
        .unwrap();
    assert_eq!(traces.len(), 1);
    let t = &traces[0];
    assert_eq!(t.attempt_no, 1);
    assert_eq!(t.outcome_kind.as_deref(), Some("OK"));
    assert_eq!(t.server_fiscal_no.as_deref(), Some("DPS-FN-7777"));
    assert!(t.completed_at.is_some());
    assert!(t.wire_call_started_at.is_some());
    assert!(t.wire_call_finished_at.is_some());
    assert_ne!(
        t.request_envelope_sha256, [0u8; 32],
        "envelope hash must be computed and persisted"
    );

    // Audit pair.
    assert_eq!(
        read_audit_event_types(&pool, doc).await,
        vec!["STAGE_SEND_INTENT_MARKED", "STAGE_SEND_RESULT"]
    );
}

// ─── Fixture 2 — terminal_reject ─────────────────────────────────────

#[tokio::test]
async fn terminal_reject_routes_to_rejected_no_server_fiscal_no() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_doc_with_xml(&pool, 0x22, "SELL", 1, "SIGNED").await;
    let stub = StubDpsChannel::new(Err(DpsError::Authorization {
        code: -1,
        kind: AuthorizationKind::DocumentReject,
        message: "ERROR_VEREFY".into(),
    }));

    let outcome = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect("rejected wire response is a successful stage_send outcome");

    match outcome {
        StageSendOutcome::Routed {
            decision,
            attempt_no,
            wire_status_code,
            wire_error_message,
        } => {
            assert_eq!(decision.retry_class, RetryClass::TerminalReject);
            assert_eq!(decision.target_state, DocState::Rejected);
            assert_eq!(wire_status_code, Some(-1));
            assert_eq!(wire_error_message.as_deref(), Some("ERROR_VEREFY"));
            assert_eq!(attempt_no, 1);
        }
        other => panic!("expected Routed terminal-reject, got {other:?}"),
    }
    assert_eq!(stub.call_count(), 1);

    assert_eq!(read_doc_state(&pool, doc).await, "REJECTED");
    assert!(
        read_server_fiscal_no(&pool, doc).await.is_none(),
        "rejected path must not write server_fiscal_no"
    );

    let traces = transport_trace::list_for_document(&pool, doc)
        .await
        .unwrap();
    let t = &traces[0];
    assert_eq!(t.outcome_kind.as_deref(), Some("REJECTED"));
    assert_eq!(t.server_status_code, Some(-1));
    assert_eq!(t.error_kind.as_deref(), Some("AuthorizationDocumentReject"));
    assert_eq!(t.error_message.as_deref(), Some("ERROR_VEREFY"));
    assert!(t.server_fiscal_no.is_none());
}

// ─── Fixture 3 — transport_retryable ─────────────────────────────────

#[tokio::test]
async fn transport_error_holds_stop_preserves_attempt_at() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_doc_with_xml(&pool, 0x33, "SELL", 1, "SIGNED").await;
    let stub = StubDpsChannel::new(Err(DpsError::Transport("TLS reset".into())));

    let outcome = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect("transport error is a successful stage_send outcome");

    match outcome {
        StageSendOutcome::Routed {
            decision,
            attempt_no,
            wire_status_code,
            wire_error_message,
        } => {
            assert_eq!(decision.retry_class, RetryClass::TransientRetry);
            assert_eq!(decision.target_state, DocState::ErrorRetryable);
            assert_eq!(wire_status_code, None);
            assert_eq!(wire_error_message.as_deref(), Some("TLS reset"));
            assert_eq!(attempt_no, 1);
        }
        other => panic!("expected Routed transient-retry (Transport), got {other:?}"),
    }
    assert_eq!(stub.call_count(), 1);

    // CS-3 S7-1: a Transport error is CLASSIFIED retryable (the trace records
    // RETRYABLE_TRANSPORT below), but the composed path no longer RESTS it in
    // ErrorRetryable for auto-redrive.  A wire failure after CALL_STARTED is
    // AMBIGUOUS (the send may have reached DPS), so it is a recorded HOLD (the
    // SubmittedUnknown treatment): the doc rests SENDING under a PENDING_APPLY
    // reservation and the node halts to STOP_MODE — a conscious liveness trade for
    // P2 double-issue safety.  Boot / operator resolves the hold; no auto re-wire.
    assert_eq!(
        read_doc_state(&pool, doc).await,
        "SENDING",
        "transport → HELD (SENDING), NOT auto-retryable ErrorRetryable"
    );
    let mode: String =
        sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = '1234567890'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        mode, "STOP_MODE",
        "transport-after-CALL_STARTED halts the node (ambiguous SubmittedUnknown)"
    );
    let apply_state: Option<String> = sqlx::query_scalar(
        "SELECT apply_state FROM delivery_reservation WHERE fiscal_number = '1234567890'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        apply_state.as_deref(),
        Some("PENDING_APPLY"),
        "transport HELD reservation"
    );
    assert!(
        read_submission_attempted_at(&pool, doc).await.is_some(),
        "submission_attempted_at must persist on the held path (forensics + boot input)"
    );

    // The trace still records the transport CLASSIFICATION (RETRYABLE_TRANSPORT) — the
    // wire outcome is retryable-typed even though the composed apply HOLDs it under STOP.
    let traces = transport_trace::list_for_document(&pool, doc)
        .await
        .unwrap();
    let t = &traces[0];
    assert_eq!(t.outcome_kind.as_deref(), Some("RETRYABLE_TRANSPORT"));
    assert_eq!(t.error_kind.as_deref(), Some("Transport"));
    assert_eq!(t.error_message.as_deref(), Some("TLS reset"));
    assert!(t.server_fiscal_no.is_none());
    assert!(t.server_status_code.is_none());
}

// ─── Fixture 3b — terminal Server reject (-5 ERROR_TYPE) ─────────────
//
// W10.2 review MED 2 close.  Anchors the §2.1 sub-table for terminal
// Server-class errors that are NOT Authorization{DocumentReject}.  -5
// is the canonical "M3 builder bug" reject — invalid `check_type` —
// which the W10 routing fn fail-closes to TerminalReject + CRITICAL.
// Mirrors the §2.1 row-5 + §3.4 audit-event contract.

#[tokio::test]
async fn terminal_server_minus_5_routes_to_rejected_critical_audit() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_doc_with_xml(&pool, 0xB1, "SELL", 1, "SIGNED").await;
    let stub = StubDpsChannel::new(Err(DpsError::Server {
        code: -5,
        message: "ERROR_TYPE".into(),
    }));

    let outcome = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect("Server -5 is a successful stage_send outcome (terminal route)");

    match outcome {
        StageSendOutcome::Routed {
            decision,
            wire_status_code,
            wire_error_message,
            ..
        } => {
            assert_eq!(decision.retry_class, RetryClass::TerminalReject);
            assert_eq!(decision.target_state, DocState::Rejected);
            assert_eq!(wire_status_code, Some(-5));
            assert_eq!(wire_error_message.as_deref(), Some("ERROR_TYPE"));
        }
        other => panic!("expected Routed terminal-reject for -5, got {other:?}"),
    }
    assert_eq!(stub.call_count(), 1);
    assert_eq!(read_doc_state(&pool, doc).await, "REJECTED");

    // Trace forensics: REJECTED outcome_kind, retry_class durably
    // persisted (W10.2 review MED 1 close — migration 012).
    let traces = transport_trace::list_for_document(&pool, doc)
        .await
        .unwrap();
    let t = &traces[0];
    assert_eq!(t.outcome_kind.as_deref(), Some("REJECTED"));
    assert_eq!(t.error_kind.as_deref(), Some("Server"));
    assert_eq!(t.server_status_code, Some(-5));
    assert_eq!(t.retry_class.as_deref(), Some("TerminalReject"));

    // Audit event: STAGE_SEND_REJECTED (per §3.4 closed enum), Critical
    // severity (per §2.1 row -5 — M3 builder bug).
    assert_eq!(
        read_audit_event_types(&pool, doc).await,
        vec!["STAGE_SEND_INTENT_MARKED", "STAGE_SEND_REJECTED"]
    );
}

// ─── Fixture 3c — Authorization{FnNotRegistered} (-13) ───────────────
//
// W10.2 review MED 2 close.  -13/-14 carry an Authorization variant
// that is per-FN, not per-doc: doc lands in ErrorRetryable with
// retry_class=FnConfigError and audit STAGE_SEND_FN_NOT_REGISTERED.
// W9 chains via RequiresManualReconciliation.

#[tokio::test]
async fn fn_config_minus_13_routes_to_error_retryable_with_fn_config_class() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_doc_with_xml(&pool, 0xB2, "SELL", 1, "SIGNED").await;
    let stub = StubDpsChannel::new(Err(DpsError::Authorization {
        code: -13,
        kind: AuthorizationKind::FiscalNumberNotRegistered,
        message: "ERROR_NOT_REGISTERED_RRO".into(),
    }));

    let outcome = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect("Authorization -13 is a successful stage_send outcome");

    match outcome {
        StageSendOutcome::Routed {
            decision,
            wire_status_code,
            wire_error_message,
            ..
        } => {
            assert_eq!(decision.retry_class, RetryClass::FnConfigError);
            assert_eq!(decision.target_state, DocState::ErrorRetryable);
            assert_eq!(wire_status_code, Some(-13));
            assert_eq!(
                wire_error_message.as_deref(),
                Some("ERROR_NOT_REGISTERED_RRO")
            );
        }
        other => panic!("expected Routed FnConfigError for -13, got {other:?}"),
    }
    assert_eq!(read_doc_state(&pool, doc).await, "ERROR_RETRYABLE");

    let traces = transport_trace::list_for_document(&pool, doc)
        .await
        .unwrap();
    let t = &traces[0];
    assert_eq!(t.outcome_kind.as_deref(), Some("RETRYABLE_AUTH_FN"));
    assert_eq!(
        t.error_kind.as_deref(),
        Some("AuthorizationFnNotRegistered")
    );
    assert_eq!(t.server_status_code, Some(-13));
    assert_eq!(t.retry_class.as_deref(), Some("FnConfigError"));

    // R-W10.2-review HIGH 1 close: durable retry_class enables
    // dispatcher-level retry-loop gate.  Read via the public helper
    // and verify it surfaces the right RetryClass — this is the
    // contract that keeps a future worker dispatcher from auto-retrying
    // an FN-config doc into a crash-loop.
    let last_rc = transport_trace::last_attempt_retry_class_for(&pool, doc)
        .await
        .unwrap();
    assert_eq!(last_rc, Some(RetryClass::FnConfigError));

    assert_eq!(
        read_audit_event_types(&pool, doc).await,
        vec!["STAGE_SEND_INTENT_MARKED", "STAGE_SEND_FN_NOT_REGISTERED"]
    );
}

// ─── Fixture 3d — Decode (proto status=0) → ProbeRequired ────────────
//
// W10.2 review MED 2 close.  Decode is the W3 pre-classifier shape for
// `status=0` UNKNOWN proto-default: needs a W9 `last_chk` probe to
// disambiguate.  Routing fn emits target=ErrorRetryable +
// retry_class=ProbeRequired + probe_hint=DecodeUnknown.

#[tokio::test]
async fn decode_status_zero_routes_to_probe_required_with_decode_unknown_hint() {
    use prro::services::write_path::error_routing::ProbeReason;
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_doc_with_xml(&pool, 0xB3, "SELL", 1, "SIGNED").await;
    let stub = StubDpsChannel::new(Err(DpsError::Decode("status=0 UNKNOWN".into())));

    let outcome = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect("Decode is a successful stage_send outcome (probe-required route)");

    match outcome {
        StageSendOutcome::Routed {
            decision,
            wire_status_code,
            wire_error_message,
            ..
        } => {
            assert_eq!(decision.retry_class, RetryClass::ProbeRequired);
            assert_eq!(decision.target_state, DocState::ErrorRetryable);
            assert_eq!(wire_status_code, None);
            assert_eq!(wire_error_message.as_deref(), Some("status=0 UNKNOWN"));
            assert_eq!(
                decision.probe_hint.as_ref().map(|h| h.reason),
                Some(ProbeReason::DecodeUnknown),
                "probe_hint must carry DecodeUnknown reason for W9 probe"
            );
        }
        other => panic!("expected Routed ProbeRequired for Decode, got {other:?}"),
    }
    // CS-3 S7-1: Decode status=0 is CLASSIFIED ProbeRequired (the routing echo above + the trace
    // RETRYABLE_SERVER row below), but the composed path HOLDs it: the doc rests SENDING under a
    // PENDING_APPLY reservation (node_effect ProbeRequired) and the node halts to STOP_MODE,
    // awaiting a W9 last_chk probe / operator — NOT an auto-retryable ErrorRetryable.
    assert_eq!(
        read_doc_state(&pool, doc).await,
        "SENDING",
        "Decode → HELD (SENDING), NOT auto-retryable ErrorRetryable"
    );
    let mode: String =
        sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = '1234567890'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        mode, "STOP_MODE",
        "ProbeRequired halts the node pending the last_chk probe"
    );
    let (apply_state, node_effect): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT apply_state, node_effect FROM delivery_reservation WHERE fiscal_number = '1234567890'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        apply_state.as_deref(),
        Some("PENDING_APPLY"),
        "Decode HELD reservation"
    );
    assert_eq!(
        node_effect.as_deref(),
        Some("ProbeRequired"),
        "Decode node-effect"
    );

    let traces = transport_trace::list_for_document(&pool, doc)
        .await
        .unwrap();
    let t = &traces[0];
    // ProbeRequired folds to RETRYABLE_SERVER per W10.2 best-effort
    // mapping (existing CHECK list).  W10.5 may add a finer kind.
    assert_eq!(t.outcome_kind.as_deref(), Some("RETRYABLE_SERVER"));
    assert_eq!(t.error_kind.as_deref(), Some("Decode"));
    assert_eq!(t.retry_class.as_deref(), Some("ProbeRequired"));
    let last_rc = transport_trace::last_attempt_retry_class_for(&pool, doc)
        .await
        .unwrap();
    assert_eq!(last_rc, Some(RetryClass::ProbeRequired));

    assert_eq!(
        read_audit_event_types(&pool, doc).await,
        vec!["STAGE_SEND_INTENT_MARKED", "STAGE_SEND_DECODE_UNKNOWN"]
    );

    // R-W10.3-review MED 1 close: pin the audit-payload extension
    // for `probe_hint`.  freeze §4.5 + W10.3 commit msg promise the
    // routed arm carries `probe_hint:"<ProbeReason>"` for Decode /
    // -2 / -15 close-shift; without this assert the payload-extension
    // branch could silently regress.
    let last_payload = sqlx::query_scalar::<_, Option<String>>(
        "SELECT event_payload_json FROM audit_log \
         WHERE entity_type = 'fiscal_document' AND entity_id = ? \
         ORDER BY audit_id DESC LIMIT 1",
    )
    .bind(format!("{doc:?}"))
    .fetch_one(&pool)
    .await
    .unwrap()
    .expect("audit payload must be present");
    assert!(
        last_payload.contains("\"probe_hint\":\"DecodeUnknown\""),
        "audit payload must carry probe_hint=DecodeUnknown evidence: {last_payload}"
    );
    assert!(
        last_payload.contains("\"retry_class\":\"ProbeRequired\""),
        "audit payload must carry retry_class: {last_payload}"
    );
}

// ─── Fixture 3e — Server -11 ERROR_OFFLINE_168 → node BLOCKED ────────
//
// W10.3 close.  -11 routes to TerminalReject + target=Rejected AND
// flips `node_state.mode → BLOCKED` atomic with the doc-state CAS.
// Pins:
//   - decision.retry_class == TerminalReject
//   - decision.target_state == Rejected
//   - decision.node_mode_flip == Some(NodeMode::Blocked)
//   - post-run `node_state.mode == 'BLOCKED'`
//   - audit STAGE_SEND_NODE_BLOCKED (Critical)
//   - audit payload carries `node_mode_flipped: "Blocked"` (PascalCase
//     per R-W10.3-review LOW 1 — JSON consistency with retry_class
//     and probe_hint)
//   - durable retry_class on trace row

#[tokio::test]
async fn server_minus_11_routes_to_rejected_and_flips_node_to_blocked() {
    use prro::db::repositories::node_state;
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_doc_with_xml(&pool, 0xB4, "SELL", 1, "SIGNED").await;

    // Pre-condition: node_state row must exist for the fn (W5 acquire
    // upserts it; the seed helper above already INSERTed
    // fiscal_number_config but not node_state).  Use upsert_initial
    // mirroring W5's behaviour.
    node_state::upsert_initial(
        &pool,
        "1234567890",
        prro::db::models::enums::NodeMode::Online,
        prro::db::models::enums::ShiftState::Closed,
        1,
    )
    .await
    .expect("seed node_state row");

    let stub = StubDpsChannel::new(Err(DpsError::Server {
        code: -11,
        message: "ERROR_OFFLINE_168".into(),
    }));

    let outcome = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect("Server -11 is a successful stage_send outcome (terminal + flip)");

    // Outcome shape — TerminalReject + node_mode_flip evidence.
    match outcome {
        StageSendOutcome::Routed {
            decision,
            wire_status_code,
            ..
        } => {
            assert_eq!(decision.retry_class, RetryClass::TerminalReject);
            assert_eq!(decision.target_state, DocState::Rejected);
            assert_eq!(
                decision.node_mode_flip,
                Some(prro::db::models::enums::NodeMode::Blocked),
                "Server -11 routing fn must emit node_mode_flip=Blocked"
            );
            assert_eq!(wire_status_code, Some(-11));
        }
        other => panic!("expected Routed terminal-reject for -11, got {other:?}"),
    }

    // Doc state: REJECTED.
    assert_eq!(read_doc_state(&pool, doc).await, "REJECTED");

    // **W10.3 atomic flip proof.**  node_state.mode is BLOCKED.
    let row = node_state::get(&pool, "1234567890")
        .await
        .unwrap()
        .expect("node_state row must exist");
    assert_eq!(
        row.mode,
        prro::db::models::enums::NodeMode::Blocked,
        "Server -11 MUST flip node_state.mode → BLOCKED atomic with the CAS"
    );

    // Audit pair + payload evidence.
    assert_eq!(
        read_audit_event_types(&pool, doc).await,
        vec!["STAGE_SEND_INTENT_MARKED", "STAGE_SEND_NODE_BLOCKED"]
    );
    let last_payload = sqlx::query_scalar::<_, Option<String>>(
        "SELECT event_payload_json FROM audit_log \
         WHERE entity_type = 'fiscal_document' AND entity_id = ? \
         ORDER BY audit_id DESC LIMIT 1",
    )
    .bind(format!("{doc:?}"))
    .fetch_one(&pool)
    .await
    .unwrap()
    .expect("audit payload must be present");
    assert!(
        last_payload.contains("\"node_mode_flipped\":\"Blocked\""),
        "audit payload must carry node_mode_flipped=Blocked (PascalCase per LOW 1): {last_payload}"
    );
    assert!(
        last_payload.contains("\"retry_class\":\"TerminalReject\""),
        "audit payload must carry retry_class: {last_payload}"
    );
}

// ─── Fixture 3f — missing node_state row → pre-wire P3 refusal (zero wire)
//
// W10.3 structural-breach proof, relocated by CS-3 S7-1 (§2.1). A missing
// `node_state` row (W5 acquire MUST upsert it before stage 1) is now caught by
// the PRE-WIRE P3 online-predecessor guard inside the authorize tx — it refuses
// with a typed `Internal("P3: node_state row missing")` and ZERO wire, rolling
// the whole authorize tx back atomically (nothing minted). The queued -11 is
// never reached — the breach is caught before the wire, not at a post-wire 4-b.

#[tokio::test]
async fn missing_node_state_refused_pre_wire_p3_zero_wire() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_doc_with_xml(&pool, 0xB5, "SELL", 1, "SIGNED").await;
    // **Intentionally NO `node_state::upsert_initial`** — simulate
    // structural breach (W5 invariant violated upstream).  The shared
    // seed helper now seeds a genesis node_state (for the A.3 online
    // advance path); this test's whole point is a MISSING row, so
    // delete it back out after seeding.
    sqlx::query("DELETE FROM node_state WHERE fiscal_number = '1234567890'")
        .execute(&pool)
        .await
        .expect("remove node_state to simulate structural breach");

    let stub = StubDpsChannel::new(Err(DpsError::Server {
        code: -11,
        message: "ERROR_OFFLINE_168".into(),
    }));

    let err = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect_err("a missing node_state row is refused pre-wire by the P3 guard");
    match err {
        StageSendError::Internal(e) => assert!(
            e.to_string().contains("P3: node_state row missing"),
            "expected the pre-wire P3 missing-node_state refusal, got: {e}"
        ),
        other => panic!("expected Internal(P3 node_state missing), got {other:?}"),
    }

    // KEY: the P3 guard runs PRE-WIRE inside the authorize tx (the SAME BEGIN IMMEDIATE as the
    // reservation insert + CALL_STARTED marker), so its failure rolls the WHOLE authorize tx
    // back BEFORE any send — send_chk NEVER fires (the queued -11 is never even reached).
    assert_eq!(
        stub.call_count(),
        0,
        "pre-wire P3 refuses with ZERO wire (before the -11)"
    );

    // The authorize rollback is atomic: NO CALL_STARTED marker, NO trace, NO intent audit — the
    // doc simply stays in its SIGNED source state for boot to re-attempt once the FN is repaired.
    // (Pre-cutover the post-wire 4-b guard left a SENDING marker + a 4-pre trace + intent audit;
    // relocating the check pre-wire makes the refusal mint nothing.)
    assert_eq!(read_doc_state(&pool, doc).await, "SIGNED");
    let traces = transport_trace::list_for_document(&pool, doc)
        .await
        .unwrap();
    assert_eq!(
        traces.len(),
        0,
        "authorize rolled back — no trace row minted"
    );
    assert!(
        read_audit_event_types(&pool, doc).await.is_empty(),
        "authorize rolled back — even the intent-marker audit is gone"
    );

    // node_state row is genuinely missing (we never seeded it) — the refused authorize left no
    // half-applied row behind.
    let node_row = prro::db::repositories::node_state::get(&pool, "1234567890")
        .await
        .unwrap();
    assert!(
        node_row.is_none(),
        "the refused authorize must NOT leave a partial node_state row"
    );
}

// ─── Fixture 4 — pattern_b_ordering (spy) ────────────────────────────

#[tokio::test]
async fn pattern_b_ordering_spy_observes_committed_sending_before_send_chk() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_doc_with_xml(&pool, 0x44, "SELL", 1, "SIGNED").await;

    // Spy infrastructure — captures the state observed inside send_chk.
    let observed_state: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    // Pool clone moved into the spy callback.  Spy uses a synchronous
    // SQLite read via `block_on` on a fresh tokio handle so the read
    // does not entangle the outer test runtime.  We use the existing
    // pool — WAL mode means the read sees all committed writes.
    let pool_for_spy = pool.clone();
    let observed_for_spy = Arc::clone(&observed_state);

    let stub = StubDpsChannel::with_spy(
        Ok(ack("DPS-FN-PATTERN-B")),
        Box::new(move || {
            // The spy runs INSIDE send_chk (4a, no lock).  At this
            // point 4-pre tx has committed; reading the state via a
            // fresh acquire MUST see SENDING.
            let pool = pool_for_spy.clone();
            let observed = Arc::clone(&observed_for_spy);
            // We're inside an async fn (send_chk) but `Fn() + Send +
            // Sync` is non-async.  Spawn a blocking thread that
            // creates its own runtime to do the DB read.  Use
            // `current_thread` flavour — single SELECT does not need
            // a multi-thread scheduler and instantiation is ~10× cheaper
            // than the default `Runtime::new()`.  WAL + fresh acquire
            // through the cloned pool sees the latest committed
            // snapshot, which is the post-4-pre-commit state.
            let handle = std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("spy runtime");
                rt.block_on(async move {
                    let row: String = sqlx::query_scalar(
                        "SELECT state FROM fiscal_documents WHERE document_id = ?",
                    )
                    .bind(DbDocumentId(doc))
                    .fetch_one(&pool)
                    .await
                    .expect("spy SELECT state");
                    *observed.lock().unwrap() = Some(row);
                });
            });
            handle.join().expect("spy thread join");
        }),
    );

    let outcome = stage_send::run(&pool, &stub, doc, None).await.unwrap();
    assert!(
        matches!(outcome, StageSendOutcome::Sent { .. }),
        "spy fixture expects happy path, got {outcome:?}"
    );

    // Pattern B proof: spy observed SENDING during the wire call.
    let spy_observation = observed_state.lock().unwrap().clone();
    assert_eq!(
        spy_observation.as_deref(),
        Some("SENDING"),
        "spy MUST observe SENDING during send_chk; got {spy_observation:?}"
    );

    // Post-run state: SENT (4-b CAS committed after spy returned).
    assert_eq!(read_doc_state(&pool, doc).await, "SENT");
}

// ─── Fixture 5 — rerun_on_sent_state_conflict ────────────────────────

#[tokio::test]
async fn rerun_on_sent_state_conflict_short_circuits_with_zero_wire_calls() {
    let (_d, pool) = fresh_pool().await;
    // Seed in SENT state — simulates a doc already-fiscalised by a
    // prior worker (or boot recovery).
    let doc = seed_signed_doc_with_xml(&pool, 0x55, "SELL", 1, "SENT").await;

    // Stub configured but should NOT be called.  call_count proves it.
    let stub = StubDpsChannel::new(Ok(ack("SHOULD-NEVER-BE-USED")));

    let outcome = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect("rerun on SENT is a successful idempotent re-entry, not an error");

    match outcome {
        StageSendOutcome::StateConflict { observed } => {
            assert_eq!(observed, DocState::Sent);
        }
        other => panic!("expected StateConflict, got {other:?}"),
    }

    assert_eq!(
        stub.call_count(),
        0,
        "rerun on SENT MUST NOT invoke send_chk (idempotency guard)"
    );

    // Post-run state preserved: still SENT.
    assert_eq!(read_doc_state(&pool, doc).await, "SENT");
    // No trace row should have been allocated (4-pre returned before
    // transport_trace::allocate_and_insert_tx).
    let traces = transport_trace::list_for_document(&pool, doc)
        .await
        .unwrap();
    assert_eq!(
        traces.len(),
        0,
        "StateConflict must not allocate a trace row"
    );
    // No audit entries from stage 4 either.
    assert_eq!(
        read_audit_event_types(&pool, doc).await,
        Vec::<String>::new()
    );
}

// ─── Fixture 6 — whitelist regression for 4-pre source states ───────

#[test]
fn whitelist_4pre_source_states_regression_guard() {
    // W7.4 F4 + W10.2 HIGH 3 §4.2 + M3b W9a widening (2026-05-16):
    // stage_send.rs 4-pre relies on `(Signed, Sending)`,
    // `(ErrorRetryable, Sending)`, and `(OfflineLocalAck, Sending)`
    // all being whitelisted; a future migration that drops any of
    // these entries would make the `unreachable!` in the Forbidden
    // arm fire in production.  This sub-millisecond test catches the
    // regression at CI before it ever ships.
    assert!(
        allowed_transition(DocState::Signed, DocState::Sending),
        "(Signed, Sending) MUST stay in the allowed_transition whitelist; \
         stage_send::run depends on this for the 4-pre CAS (W7 happy path)"
    );
    assert!(
        allowed_transition(DocState::ErrorRetryable, DocState::Sending),
        "(ErrorRetryable, Sending) MUST stay in the allowed_transition whitelist; \
         stage_send::run depends on this for the 4-pre CAS (W10.2 retry path)"
    );
    assert!(
        allowed_transition(DocState::OfflineLocalAck, DocState::Sending),
        "(OfflineLocalAck, Sending) MUST stay in the allowed_transition whitelist; \
         stage_send::run depends on this for the 4-pre CAS (M3b W9a offline-drain path; \
         edge added by W6 in PR #55)"
    );
    // W10.4 step 2d: MAC recovery failure overrides terminate the
    // doc directly from `ErrorRetryable` → `Rejected` without a
    // fresh wire send (HashNotExtractable / CounterExhausted /
    // second-`-12` short-circuit).  Pin against future allowlist
    // tightening.
    assert!(
        allowed_transition(DocState::ErrorRetryable, DocState::Rejected),
        "(ErrorRetryable, Rejected) MUST stay in the allowed_transition whitelist; \
         stage_send::run override-helpers depend on this for the W10.4 \
         MAC-recovery-failure terminal path"
    );
}

// ─── M3b W9a helper: realistic W7a-acked offline state ──────────────

/// Seeds a fiscal_documents row in `OFFLINE_LOCAL_ACK` state with all
/// W7a-required offline columns populated, plus the associated
/// `offline_sessions` row in `OPEN` and the `offline_codes` row
/// flagged as consumed by this document.  Mirrors what W7a's
/// `transition_to_offline_local_ack_tx` produces, without going
/// through the full `stage_offline_ack::run` path (which would
/// require additional node_state/shift seeding).
///
/// W7a invariant: an `OFFLINE_LOCAL_ACK` row MUST carry
/// `offline_fiscal_no = consumed code_lnd`,
/// `offline_fiscal_date = consumed_at`, and
/// `offline_session_id = the open session's id`.  Synthetic seeds
/// that skip these columns (the W9a Round 1 review L1 finding)
/// produce a state that W7a guarantees cannot exist in production
/// and would mask real bugs in the `id_offline` plumbing.
async fn seed_w7a_offline_local_ack(pool: &SqlitePool, doc_byte: u8, code_lnd: i64) -> DocumentId {
    let doc = seed_signed_doc_with_xml(pool, doc_byte, "SELL", 1, "OFFLINE_LOCAL_ACK").await;
    let session_id = vec![doc_byte ^ 0x55; 16];
    let consumed_at = "2026-05-16T00:00:01Z";
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, '1234567890', 'OPEN', '2026-05-16T00:00:00Z')",
    )
    .bind(&session_id)
    .execute(pool)
    .await
    .expect("seed offline_session");
    sqlx::query(
        "INSERT INTO offline_codes(fiscal_number, code_lnd, consumed_at, consumed_by_document_id) \
         VALUES ('1234567890', ?, ?, ?)",
    )
    .bind(code_lnd)
    .bind(consumed_at)
    .bind(DbDocumentId(doc))
    .execute(pool)
    .await
    .expect("seed offline_code (consumed by this doc)");
    // B8: also stamp offline_dps_code so the fail-closed drain guard passes.
    let dps_code = format!("DRILL-W9A-{code_lnd}");
    sqlx::query(
        "UPDATE fiscal_documents \
         SET offline_fiscal_no = ?, offline_fiscal_date = ?, offline_session_id = ?, \
             offline_dps_code = ? \
         WHERE document_id = ?",
    )
    .bind(code_lnd)
    .bind(consumed_at)
    .bind(&session_id)
    .bind(&dps_code)
    .bind(DbDocumentId(doc))
    .execute(pool)
    .await
    .expect("backfill W7a columns on fiscal_documents");
    doc
}

// ─── Fixture 6b' — M3b W9a Pattern C drain: OfflineLocalAck → Sending ─

#[tokio::test]
async fn w9a_offline_local_ack_to_sending_drives_through_4_pre() {
    // M3b W9a: stage_send::run's 4-pre source-state CAS now accepts
    // `OfflineLocalAck` as a third allowed source, alongside `Signed`
    // and `ErrorRetryable`.  The new edge is the entry to W9 Pattern C
    // drain: an offline-acked doc replays through the wire-send ladder
    // on return-online.  This fixture seeds a realistic W7a state
    // (offline_fiscal_no + offline_fiscal_date + offline_session_id +
    // a consumed offline_codes row + the associated OPEN
    // offline_sessions row), runs stage_send::run, and asserts:
    //   - the 4-pre CAS Applied (no StateConflict) — the new source
    //     state is in the allowlist;
    //   - the (OfflineLocalAck, Sending) whitelist edge (added in W6)
    //     was consumed via fiscal_documents::transition_state (NOT a
    //     raw UPDATE bypass);
    //   - wire send_chk WAS called;
    //   - 4-b CAS Sending → Sent committed.
    //
    // Round 1 L1 fix: seed shape now matches W7a invariants exactly,
    // so the OfflineFiscalNoMissing guard does NOT fire (offline
    // columns are populated as W7a would have produced them).
    //
    // Scope note: W9a is transport-ladder widening ONLY.  W9b will
    // wire the actual drain caller (App::drain_offline_backlog_with).
    // This fixture proves the seam is ready, not that it is wired.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_w7a_offline_local_ack(&pool, 0xC0, 42).await;
    let stub = StubDpsChannel::new(Ok(ack("DPS-FN-W9A-DRAIN")));

    let outcome = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect("drain from OfflineLocalAck must succeed");

    match outcome {
        StageSendOutcome::Sent {
            server_fiscal_no,
            attempt_no: _,
        } => {
            assert_eq!(server_fiscal_no, "DPS-FN-W9A-DRAIN");
        }
        other => panic!("expected Sent on W9a drain-path, got {other:?}"),
    }
    assert_eq!(
        stub.call_count(),
        1,
        "W9a drain-path must invoke send_chk exactly once"
    );
    assert_eq!(read_doc_state(&pool, doc).await, "SENT");

    // Pattern B intent-marker pair audit landed at the OfflineLocalAck
    // → Sending edge — same audit pair shape as the Signed and
    // ErrorRetryable paths.  No new audit event types in W9a.
    assert_eq!(
        read_audit_event_types(&pool, doc).await,
        vec!["STAGE_SEND_INTENT_MARKED", "STAGE_SEND_RESULT"]
    );
}

// ─── Fixture 6b'' — M3b W9a: id_offline carries offline_fiscal_no ────

/// Recording DpsChannel that captures every `send_chk` envelope.
/// Used by the wire-correctness fixture to assert that
/// `CheckEnvelope.id_offline` is populated from
/// `fiscal_documents.offline_fiscal_no` for the W9 drain replay path
/// and left empty for the M3a online path.
struct EnvelopeRecorder {
    response: Result<CheckAck, DpsError>,
    captured: std::sync::Mutex<Vec<CheckEnvelope>>,
}

impl EnvelopeRecorder {
    fn new(response: Result<CheckAck, DpsError>) -> Self {
        Self {
            response,
            captured: std::sync::Mutex::new(Vec::new()),
        }
    }
    fn first(&self) -> CheckEnvelope {
        self.captured
            .lock()
            .unwrap()
            .first()
            .cloned()
            .expect("send_chk never called")
    }
}

#[async_trait::async_trait]
impl DpsChannel for EnvelopeRecorder {
    async fn send_chk(&self, envelope: CheckEnvelope) -> Result<CheckAck, DpsError> {
        self.captured.lock().unwrap().push(envelope);
        match &self.response {
            Ok(a) => Ok(a.clone()),
            Err(_) => Err(DpsError::Transport("recorder: primed error".into())),
        }
    }
    async fn send_chk_observed(
        &self,
        envelope: CheckEnvelope,
    ) -> (
        Result<CheckAck, DpsError>,
        prro::transports::dps::raw_reply::RawSendObservation,
    ) {
        prro::transports::dps::dto::scripted_observation(self.send_chk(envelope).await)
    }
    async fn last_chk(&self, _: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        unreachable!("EnvelopeRecorder: last_chk not exercised")
    }
    async fn ping(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        unreachable!("EnvelopeRecorder: ping not exercised")
    }
    async fn status_rro(
        &self,
        _: &CheckSignBlob,
    ) -> Result<prro::transports::dps::dto::StatusSnapshot, DpsError> {
        unreachable!("EnvelopeRecorder: status_rro not exercised")
    }
    async fn info_rro(
        &self,
        _: &CheckSignBlob,
    ) -> Result<prro::transports::dps::dto::RroInfo, DpsError> {
        unreachable!("EnvelopeRecorder: info_rro not exercised")
    }
    async fn ask_offline_codes(
        &self,
        _: prro::transports::dps::dto::CheckEnvelope,
    ) -> Result<prro::transports::dps::dto::OfflineCodesResponse, DpsError> {
        unreachable!("EnvelopeRecorder: ask_offline_codes not exercised")
    }
}

#[tokio::test]
async fn w9a_offline_drain_wire_envelope_carries_id_offline_stringified() {
    // M3b W9a wire-contract fixture (Round 1 HIGH fix): when
    // stage_send::run drains an `OfflineLocalAck` doc, the
    // `CheckEnvelope` sent over the wire MUST carry
    // `id_offline = offline_fiscal_no.to_string()` per
    // `docs/superpowers/specs/2026-05-04-m2-w0-1-dps-wire.md:116`.
    // The Sprint-7 proven Python contract (`dps_fiscal_server.py:196`)
    // makes empty `id_offline` mean "online"; a drain that sends
    // empty `id_offline` would mis-identify the receipt to DPS.
    //
    // Setup: seed W7a state with `offline_fiscal_no = 42`, drive
    // stage_send::run, then assert the recorded envelope has
    // `id_offline == "42"`.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_w7a_offline_local_ack(&pool, 0xC1, 42).await;
    let recorder = EnvelopeRecorder::new(Ok(ack("DPS-FN-W9A-WIRE")));

    let outcome = stage_send::run(&pool, &recorder, doc, None)
        .await
        .expect("drain wire-envelope test must succeed");
    assert!(
        matches!(outcome, StageSendOutcome::Sent { .. }),
        "expected Sent, got {outcome:?}"
    );

    let env = recorder.first();
    // B8-3: id_offline now comes from offline_dps_code (the opaque DPS string),
    // not from offline_fiscal_no.to_string().  seed_w7a_offline_local_ack stamps
    // dps_code = format!("DRILL-W9A-{code_lnd}") so for code_lnd=42 we expect "DRILL-W9A-42".
    assert_eq!(
        env.id_offline, "DRILL-W9A-42",
        "id_offline must equal offline_dps_code (B8 DPS wire contract)"
    );
    assert_eq!(env.id_cancel, "", "id_cancel stays empty in W9a");
    assert_eq!(env.rro_fn, "1234567890");
}

#[tokio::test]
async fn w9a_online_signed_wire_envelope_id_offline_is_empty() {
    // Negative-side proof of the same Round 1 HIGH fix: an M3a
    // online doc (Signed source, NULL offline_fiscal_no) must emit
    // `id_offline = ""` so DPS treats it as a live online receipt.
    // Without this assertion the wire-contract test above could
    // pass via a "always populate id_offline" bug that breaks M3a.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_doc_with_xml(&pool, 0xC2, "SELL", 1, "SIGNED").await;
    let recorder = EnvelopeRecorder::new(Ok(ack("DPS-FN-W9A-ONLINE")));

    let outcome = stage_send::run(&pool, &recorder, doc, None)
        .await
        .expect("online wire-envelope test must succeed");
    assert!(
        matches!(outcome, StageSendOutcome::Sent { .. }),
        "expected Sent on online path, got {outcome:?}"
    );

    let env = recorder.first();
    assert_eq!(
        env.id_offline, "",
        "M3a online path must keep id_offline empty (Sprint-7 contract)"
    );
}

#[tokio::test]
async fn w9a_offline_local_ack_with_null_offline_fiscal_no_surfaces_typed_error() {
    // Round 1 HIGH guard: if the OfflineLocalAck row is observed
    // with NULL `offline_fiscal_no` (raw-SQL bypass or schema
    // regression — W7a guarantees this cannot happen organically),
    // stage_send::run must surface `OfflineFiscalNoMissing` BEFORE
    // any CAS / wire side effect.  Proven by seeding the unhealthy
    // synthetic state (the old W9a Round 0 seed shape) and asserting
    // the typed error.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_doc_with_xml(&pool, 0xC3, "SELL", 1, "OFFLINE_LOCAL_ACK").await;
    // NB: no W7a backfill — offline_fiscal_no stays NULL.
    let stub = StubDpsChannel::new(Ok(ack("SHOULD-NEVER-BE-USED")));

    let err = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect_err("NULL offline_fiscal_no on OfflineLocalAck must fail");
    assert!(
        matches!(err, stage_send::StageSendError::OfflineFiscalNoMissing { document_id } if document_id == doc),
        "expected OfflineFiscalNoMissing typed error, got {err:?}"
    );
    assert_eq!(
        stub.call_count(),
        0,
        "no wire call before W7a invariant guard fires"
    );
    assert_eq!(
        read_doc_state(&pool, doc).await,
        "OFFLINE_LOCAL_ACK",
        "no state mutation before the guard fires"
    );
}

// ─── Fixture 6b''' — M3b W9a R2 LOW #1: offline_fiscal_no <= 0 ────────

#[tokio::test]
async fn w9a_offline_local_ack_with_non_positive_offline_fiscal_no_surfaces_typed_error() {
    // M3b W9a Round 2 LOW #1 fix: producer-side W7a writes
    // `offline_fiscal_no = consumed code_lnd`, and `offline_codes`
    // carries a schema CHECK `code_lnd > 0` — so a positive value
    // is guaranteed by the W7a path.  `fiscal_documents.offline_fiscal_no`
    // itself has no CHECK (migrations/002_fiscal_documents.sql:25),
    // so a raw-SQL bypass or future schema regression could leak `0`
    // / negative.  Stage_send::run must surface
    // `OfflineFiscalNoNonPositive` BEFORE any CAS / wire side
    // effect, distinguishing it from the NULL case
    // (`OfflineFiscalNoMissing`) for forensic clarity.
    //
    // Parametrised across the realistic non-positive set
    // (0 + negative) to prove the guard handles `<= 0` not just
    // `== 0`.
    let (_d, pool) = fresh_pool().await;
    let cases: Vec<(u8, i64)> = vec![(0xC4, 0), (0xC5, -1)];
    for (byte_seed, bad_offline_fiscal_no) in cases {
        let doc = seed_signed_doc_with_xml(
            &pool,
            byte_seed,
            "SELL",
            byte_seed as i64,
            "OFFLINE_LOCAL_ACK",
        )
        .await;
        // Backfill offline_fiscal_no = 0 (or -1) directly via raw
        // UPDATE.  This is the producer-side invariant breach the
        // guard exists to catch — the row is otherwise W7a-shaped
        // (state is OFFLINE_LOCAL_ACK) but the column carries an
        // invalid payload.
        sqlx::query("UPDATE fiscal_documents SET offline_fiscal_no = ? WHERE document_id = ?")
            .bind(bad_offline_fiscal_no)
            .bind(DbDocumentId(doc))
            .execute(&pool)
            .await
            .expect("backfill non-positive offline_fiscal_no");
        let stub = StubDpsChannel::new(Ok(ack("SHOULD-NEVER-BE-USED")));

        let err = stage_send::run(&pool, &stub, doc, None)
            .await
            .expect_err("non-positive offline_fiscal_no on OfflineLocalAck must fail");
        match err {
            stage_send::StageSendError::OfflineFiscalNoNonPositive {
                document_id,
                observed,
            } => {
                assert_eq!(
                    document_id, doc,
                    "case n={bad_offline_fiscal_no}: document_id"
                );
                assert_eq!(
                    observed, bad_offline_fiscal_no,
                    "case n={bad_offline_fiscal_no}: observed"
                );
            }
            other => panic!(
                "case n={bad_offline_fiscal_no}: expected OfflineFiscalNoNonPositive, got {other:?}"
            ),
        }
        assert_eq!(
            stub.call_count(),
            0,
            "case n={bad_offline_fiscal_no}: no wire call before W7a invariant guard fires"
        );
        assert_eq!(
            read_doc_state(&pool, doc).await,
            "OFFLINE_LOCAL_ACK",
            "case n={bad_offline_fiscal_no}: no state mutation before the guard fires"
        );
    }
}

// ─── Fixture 6c — non-{Signed, ErrorRetryable, OfflineLocalAck} short-circuits ────────

#[tokio::test]
async fn rerun_on_non_allowlisted_states_short_circuits_with_zero_wire_calls() {
    // W10.2 HIGH 3 §4.2 + M3b W9a widening + CS-3 S7-1 (R2): the 4-pre
    // source allowlist now accepts ONLY Signed | OfflineLocalAck.  R2
    // DROPPED ErrorRetryable (an ErrorRetryable doc has already consumed
    // one wire under an active CALL_STARTED reservation; re-seeding it
    // would wire a SECOND time — R6 has the reconciliation layer escalate
    // such a doc to RequiresManualReconciliation + STOP instead of
    // re-wiring).  Any non-allowlisted source (Prepared / ErrorRetryable /
    // Kvt1 / Kvt2 / Sending / Rejected / etc.) MUST yield StateConflict +
    // zero wire calls.
    //
    // R-W10.2-review LOW 3 close: parametrise across the realistic ban
    // set rather than pin a single state.  Each (state_str, expected
    // DocState) tuple proves the allowlist.  byte_seed is per-case so
    // the (fiscal_number, lnd) partial UNIQUE index is not violated
    // when we batch them into one test.
    //
    // States covered:
    //   - PREPARED: pre-stage-3 leakage scenario (worker bug bypassing
    //     stage 3).
    //   - ERROR_RETRYABLE: R2 moved this OUT of the allowlist — stage_send
    //     no longer re-wires a retried doc (the retired W10.2 retry path);
    //     the reconciliation layer (R6) escalates it, never a 2nd wire.
    //   - SENDING: boot-recovery scenario — a crashed prior worker left
    //     the marker; W9 must own this case, NOT a fresh stage 4.
    //   - SENT, KVT1, KVT2: idempotent re-entry on already-fiscalised
    //     docs (covered partly by fixture 5 for SENT; this widens it).
    //   - REJECTED: terminal state — never re-attempted.
    let (_d, pool) = fresh_pool().await;
    let cases: Vec<(u8, &'static str, DocState)> = vec![
        (0xC2, "PREPARED", DocState::Prepared),
        (0xC1, "ERROR_RETRYABLE", DocState::ErrorRetryable),
        (0xC3, "SENDING", DocState::Sending),
        (0xC4, "KVT1", DocState::Kvt1),
        (0xC5, "KVT2", DocState::Kvt2),
        (0xC6, "REJECTED", DocState::Rejected),
    ];
    for (byte_seed, state_str, expected) in cases {
        // Each case gets its own fiscal_number-distinguished doc via
        // a unique lnd seed; SQLite UNIQUE(fiscal_number, lnd) WHERE
        // lnd IS NOT NULL is otherwise tripped on batch re-seed.
        let lnd = byte_seed as i64;
        let doc = seed_signed_doc_with_xml(&pool, byte_seed, "SELL", lnd, state_str).await;
        let stub = StubDpsChannel::new(Ok(ack("SHOULD-NEVER-BE-USED")));

        let outcome = stage_send::run(&pool, &stub, doc, None)
            .await
            .unwrap_or_else(|e| {
                panic!("rerun on {state_str} should be Ok(StateConflict), got Err({e:?})")
            });

        match outcome {
            StageSendOutcome::StateConflict { observed } => {
                assert_eq!(
                    observed, expected,
                    "{state_str}: observed must echo source state"
                );
            }
            other => panic!("{state_str}: expected StateConflict, got {other:?}"),
        }
        assert_eq!(
            stub.call_count(),
            0,
            "{state_str}: MUST NOT invoke send_chk on non-allowlisted state"
        );
        assert_eq!(
            read_doc_state(&pool, doc).await,
            state_str,
            "{state_str}: state must remain unchanged"
        );
    }
}

// ─── Fixture 7 — empty server_fiscal_no surfaces typed error ────────

#[tokio::test]
async fn empty_server_fiscal_no_routes_to_typed_error_no_4b_persist() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_doc_with_xml(&pool, 0x77, "SELL", 1, "SIGNED").await;
    // DPS responded OK but with empty CheckAck.id — this is a wire
    // contract violation that the W7.4 EmptyServerFiscalNo guard
    // catches BEFORE 4-b CAS persists.  The doc stays in SENDING
    // (4-pre committed the marker) for W9 to convert to ErrorRetryable
    // on the next boot.
    let stub = StubDpsChannel::new(Ok(ack("")));

    // CS-3 S7-1 (FIX-B2) CONTRACT CHANGE: an empty CheckAck.id is no longer a typed
    // `EmptyServerFiscalNo` error — it is the typed `OkButNoFiscalNumber` ApplyPlan leaf: a
    // `ProbeRequired` HELD. `run` returns `Ok(Routed{ProbeRequired})`; the doc rests in SENDING under a
    // PENDING_APPLY reservation (`apply_recorded_outcome` → `HeldNotAutoRelease`, swallowed), awaiting a
    // last_chk probe / operator — NOT auto-redrive, NOT a typed error.
    let outcome = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect("empty CheckAck.id is now a HELD probe leaf, not an error");
    match outcome {
        StageSendOutcome::Routed { decision, .. } => assert_eq!(
            decision.retry_class,
            RetryClass::ProbeRequired,
            "empty-SFN routes to the OkButNoFiscalNumber ProbeRequired HELD"
        ),
        other => panic!("expected Routed(ProbeRequired) for empty-SFN, got {other:?}"),
    }
    assert_eq!(stub.call_count(), 1, "exactly one wire call");

    // Doc HELD in SENDING (apply returned HeldNotAutoRelease — no doc CAS), no server_fiscal_no.
    assert_eq!(read_doc_state(&pool, doc).await, "SENDING");
    assert!(read_server_fiscal_no(&pool, doc).await.is_none());

    // The record transaction COMPLETED the trace with the probe outcome (no longer a crash-window
    // NULL): the composed path records + completes the attempt rather than early-erroring out of 4-pre.
    let traces = transport_trace::list_for_document(&pool, doc)
        .await
        .unwrap();
    assert_eq!(traces.len(), 1, "trace allocated + completed");
    assert!(
        traces[0].completed_at.is_some(),
        "trace.completed_at set — the record tx completed the attempt"
    );
    assert!(traces[0].outcome_kind.is_some());
}

// ─── Fixture 8 — bogus DocumentId surfaces DocumentMissing ───────────

#[tokio::test]
async fn document_missing_returns_outcome_with_zero_wire_calls() {
    let (_d, pool) = fresh_pool().await;
    // No seed at all — the doc id below points at a row that does
    // not exist.  fetch_send_inputs_tx returns None → PreOutcome::
    // DocumentMissing → StageSendOutcome::DocumentMissing.
    let bogus = DocumentId::from_bytes([0xAAu8; 16]);
    let stub = StubDpsChannel::new(Ok(ack("UNUSED")));

    let outcome = stage_send::run(&pool, &stub, bogus, None)
        .await
        .expect("DocumentMissing is a successful idempotent outcome, not an error");
    assert_eq!(outcome, StageSendOutcome::DocumentMissing);

    assert_eq!(
        stub.call_count(),
        0,
        "missing doc must NOT invoke send_chk (pre-CAS read returns None)"
    );
}

// ─── Bonus: SIGNED_XML missing surfaces typed error ──────────────────

#[tokio::test]
async fn signed_xml_missing_surfaces_typed_error_no_state_mutation() {
    let (_d, pool) = fresh_pool().await;
    // Seed doc but NOT the document_files row — simulate a state
    // invariant breach from upstream.
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();
    // W14a-2b Commit 5: signer_guard at 4-pre runs BEFORE the
    // SIGNED_XML read; seed a matching shift + signer so the test
    // reaches its intended SignedArtifactMissing path (would otherwise
    // surface ShiftMissingForFiscalDoc first).
    let shift_bytes = vec![0xE6u8; 16];
    sqlx::query(
        "INSERT INTO shifts(shift_id, fiscal_number, serial, state, open_mode, \
            cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, '1234567890', 1, 'OPENED', 'ONLINE', 0, 'test-cashier')",
    )
    .bind(&shift_bytes)
    .execute(&pool)
    .await
    .unwrap();
    let doc_bytes = vec![0x66u8; 16];
    let req_bytes = vec![0x99u8; 16];
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, shift_id, lnd, \
            doc_type, state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, signed_by_cashier_id) \
         VALUES (?, ?, '1234567890', ?, 1, 'SELL', 'SIGNED', 'b1', 't1', 'ONLINE', \
            '2026-05-09T12:34:56Z', '{}', ?, 'test-cashier')",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(&shift_bytes)
    .bind(&sha)
    .execute(&pool)
    .await
    .unwrap();
    let doc = DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap());

    let stub = StubDpsChannel::new(Ok(ack("UNUSED")));
    let err = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect_err("missing SIGNED_XML must surface as typed error");
    match err {
        StageSendError::SignedArtifactMissing { document_id } => {
            assert_eq!(document_id, doc);
        }
        other => panic!("expected SignedArtifactMissing, got {other:?}"),
    }

    assert_eq!(
        stub.call_count(),
        0,
        "stage_send must NOT invoke send_chk when SIGNED_XML is missing"
    );
    // No state mutation: doc still in SIGNED.
    assert_eq!(read_doc_state(&pool, doc).await, "SIGNED");
    // No trace row.
    let traces = transport_trace::list_for_document(&pool, doc)
        .await
        .unwrap();
    assert_eq!(traces.len(), 0);
}

// ─── CS-3 S7-1 (§2.1 / S7-P3-4) — pre-wire online predecessor equality ───────
//
// `variant_p_mac_recovery_skips_drift_assert_and_advances_seed` (A.3 Variant P) was
// RETIRED with the MAC orchestrator (R3). The old POST-wire drift-assert carried a
// `mac_recovery_attempts < 1` skip so a re-anchored recovered doc would not false-fail.
// With re-sign deleted there are NO recovered docs, so the skip is meaningless and the
// P3 equality gate moved PRE-WIRE (authorize tx, §2.1): it enforces `node_state.seed ==
// doc.previous_hash` for every online-origin doc BEFORE any wire, unconditionally — it
// cannot false-fail because no doc's `previous_hash` is ever re-anchored anymore.

/// S7-1 §2.1 (P3) — an online-origin doc whose node chain-seed has drifted off its
/// `previous_hash` is a chain fork: the PRE-WIRE predecessor-equality gate refuses to
/// authorize, so the whole `BEGIN IMMEDIATE` rolls back with ZERO wire (no reservation
/// minted, no `CALL_STARTED` marker, seed NOT advanced). This is the migration of the
/// former POST-wire `non_recovery_send_drift_fails_closed`: the refusal moved to the ONLY
/// safe side of the wire, so the KEY new tooth is `call_count() == 0` (the incumbent
/// fired the wire first, then rolled 4-b back). Revert-canary: delete the §2.1 `ensure!`
/// (`stage_send.rs:1629`) and this drift reaches the wire + `Sent` → both `call_count`
/// and the `expect_err` RED.
#[tokio::test]
async fn online_predecessor_drift_refuses_authorize_zero_wire() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_doc_with_xml(&pool, 0x5B, "SELL", 0x5B, "SIGNED").await;
    // node seed drifted off doc.previous_hash (NULL / genesis) — a chain fork.
    let drifted = [0xD2u8; 32];
    sqlx::query(
        "UPDATE node_state SET last_known_unsigned_xml_sha256 = ? \
         WHERE fiscal_number = '1234567890'",
    )
    .bind(&drifted[..])
    .execute(&pool)
    .await
    .unwrap();

    let stub = StubDpsChannel::new(Ok(ack("DPS-FN-SHOULD-NEVER-WIRE")));
    let err = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect_err("online predecessor drift must refuse authorize (fail closed)");
    match err {
        stage_send::StageSendError::Internal(e) => assert!(
            e.to_string().contains("predecessor drift"),
            "expected pre-wire P3 predecessor-drift refusal, got: {e}"
        ),
        other => panic!("expected Internal(predecessor drift), got {other:?}"),
    }

    // KEY pre-wire tooth: the refusal is BEFORE the wire — the authorize tx rolls back,
    // so send_chk is NEVER invoked (the retired POST-wire check fired the wire first).
    assert_eq!(
        stub.call_count(),
        0,
        "pre-wire P3 refuses with ZERO wire (authorize tx rolled back before send)"
    );

    // Fails closed: no CALL_STARTED / issuance (doc NOT Sent), seed NOT advanced.
    assert_ne!(
        read_doc_state(&pool, doc).await,
        "SENT",
        "a drift fork must NOT commit the Sent CAS"
    );
    let seed: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number = '1234567890'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        seed.as_deref(),
        Some(drifted.as_slice()),
        "seed must NOT advance on a refused authorize"
    );
}

// ─── CS-3 S7-1 (R3 / S7-P3-2) — `-12` divergence: recorded HELD, no 2nd wire ──
//
// The inline MAC-recovery orchestrator loop is DELETED (R3, `stage_send.rs:1232-1237`;
// retires the live `-12 Resigned => continue` per CS3_REMEDIATION_DESIGN §hold-table
// row 8). A DPS `-12` (ERROR_BAD_HASH_PREV) is now a recorded HOLD: the composed
// `run()` fires exactly ONE wire, the RECORD tx writes `MacReseedPending` (which flips
// the node to STOP_MODE), and `apply_recorded_outcome` returns the EXPECTED
// `HeldNotAutoRelease` — the doc rests SENDING (no doc CAS, no issuance) under a
// `PENDING_APPLY` reservation. NO re-sign, NO `continue`, NO second wire. Chain repair
// + operator completion release the hold later. This composed-run pin is the twin of
// the record-boundary pin `record_outcome::rc05_bad_hash_prev_held_stop_mode`.
//
// Revert-canary: if `record_outcome`'s `-12 → MacReseedPending` node-halt regressed to a
// plain retryable (no STOP), the `STOP_MODE` assertion REDs; if R3 were reverted (loop
// restored), `call_count() == 1` REDs (a 2nd wire would fire). Both are load-bearing.
#[tokio::test]
async fn minus_12_bad_hash_prev_records_held_stop_no_second_wire() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_doc_with_xml(&pool, 0xC9, "SELL", 0xC9, "SIGNED").await;
    let stub = StubDpsChannel::new(Err(DpsError::Server {
        code: -12,
        message: "ERROR_BAD_HASH_PREV: store \
                  deadbeef0123456789abcdef0123456789abcdef0123456789abcdef01234567 server-side"
            .into(),
    }));

    let outcome = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect("a -12 is a recorded HELD, not an error");

    // The returned outcome echoes the ROUTING decision (ErrorRetryable / MacRecovery)
    // with attempt_no == 1 — the raw routing echo, NOT the persisted state; the HELD is
    // realized by record/apply. attempt_no == 1 proves there was NO second attempt.
    match outcome {
        StageSendOutcome::Routed {
            decision,
            attempt_no,
            ..
        } => {
            assert_eq!(
                decision.retry_class,
                RetryClass::MacRecovery,
                "-12 routes to the MacRecovery class"
            );
            assert_eq!(
                decision.target_state,
                DocState::ErrorRetryable,
                "routing echo is ErrorRetryable; the HELD is realized by record+apply, not here"
            );
            assert_eq!(attempt_no, 1, "no re-sign, no second attempt");
        }
        other => panic!("expected Routed(MacRecovery) for -12, got {other:?}"),
    }

    // Exactly ONE wire call — the retired orchestrator would have fired a 2nd.
    assert_eq!(
        stub.call_count(),
        1,
        "exactly one wire send; the MAC re-sign retry is retired"
    );

    // Doc HELD in SENDING (apply returned HeldNotAutoRelease — no doc CAS, no issuance).
    assert_eq!(read_doc_state(&pool, doc).await, "SENDING");
    assert!(
        read_server_fiscal_no(&pool, doc).await.is_none(),
        "a HELD -12 issues nothing"
    );

    // Node halted: `MacReseedPending` flips the node to STOP_MODE in the record tx.
    let mode: String =
        sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = '1234567890'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        mode, "STOP_MODE",
        "-12 halts the node pending MAC reseed / operator completion"
    );

    // Reservation rests OUTCOME_OBSERVED + PENDING_APPLY carrying the BadHashPrev evidence
    // + MacReseedPending node-effect (the record-boundary contract, mirrors rc05).
    let (apply_state, evidence_text, node_effect): (
        Option<String>,
        Option<String>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT apply_state, evidence_text, node_effect FROM delivery_reservation \
         WHERE fiscal_number = '1234567890'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(apply_state.as_deref(), Some("PENDING_APPLY"));
    assert_eq!(evidence_text.as_deref(), Some("BadHashPrev"));
    assert_eq!(node_effect.as_deref(), Some("MacReseedPending"));

    // The record tx allocated + COMPLETED the single attempt trace (no crash-window NULL),
    // and NONE of the retired MAC-orchestrator audit events appear.
    let traces = transport_trace::list_for_document(&pool, doc)
        .await
        .unwrap();
    assert_eq!(traces.len(), 1, "exactly one attempt trace");
    assert!(
        traces[0].completed_at.is_some(),
        "trace.completed_at set — the record tx completed the attempt"
    );
    let events = read_audit_event_types(&pool, doc).await;
    for retired in [
        "MAC_RECOVERY_RESIGNED",
        "MAC_RECOVERY_FAILED_REPEAT_HASH_MISMATCH",
        "MAC_RECOVERY_HASH_NOT_EXTRACTABLE",
    ] {
        assert!(
            !events.contains(&retired.to_string()),
            "retired orchestrator event {retired} must NOT appear: {events:?}"
        );
    }
}
