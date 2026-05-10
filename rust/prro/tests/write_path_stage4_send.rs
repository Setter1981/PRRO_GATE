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

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sqlx::SqlitePool;

use prro::crypto::errors::CryptoError;
use prro::crypto::provider::{
    CertDer, CryptoProvider, DstuVerifyResult, SignCmsRequest, SignedCmsBytes,
};
use prro::crypto::session::SigningSession;
use prro::db::models::enums::DocState;
use prro::db::models::ids::DocumentId;
use prro::db::repositories::fiscal_documents::allowed_transition;
use prro::db::repositories::transport_trace;
use prro::services::write_path::error_routing::RetryClass;
use prro::services::write_path::stage_send::{self, StageSendError, StageSendOutcome};
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{CheckAck, CheckEnvelope, CheckSignBlob, RroInfo, StatusSnapshot};
use prro::transports::dps::error::{AuthorizationKind, DpsError};

// ─── In-memory stub DpsChannel ───────────────────────────────────────

/// Lightweight stub: scripted response queue + call counter +
/// optional spy callback fired BEFORE the response is returned.
///
/// W10.4 step 2d MED 1 close: queue-based to support multi-attempt
/// scenarios (e.g. MAC recovery's two-attempt sequence: -12 then OK).
/// Single-response constructors (`new` / `with_spy`) push exactly one
/// element into the queue for backwards compat with the W7.5
/// fixtures.
struct StubDpsChannel {
    responses: Mutex<VecDeque<Result<CheckAck, DpsError>>>,
    send_chk_calls: AtomicUsize,
    /// Spy hook for `pattern_b_ordering` — reads `fiscal_documents.state`
    /// from a fresh connection and stashes it in a slot.  Fires
    /// inside `send_chk` BEFORE the response is returned.
    on_send_chk: Option<Box<dyn Fn() + Send + Sync>>,
}

impl StubDpsChannel {
    fn new(response: Result<CheckAck, DpsError>) -> Self {
        let mut q = VecDeque::with_capacity(1);
        q.push_back(response);
        Self {
            responses: Mutex::new(q),
            send_chk_calls: AtomicUsize::new(0),
            on_send_chk: None,
        }
    }

    fn with_queue(responses: Vec<Result<CheckAck, DpsError>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            send_chk_calls: AtomicUsize::new(0),
            on_send_chk: None,
        }
    }

    fn with_spy(response: Result<CheckAck, DpsError>, spy: Box<dyn Fn() + Send + Sync>) -> Self {
        let mut q = VecDeque::with_capacity(1);
        q.push_back(response);
        Self {
            responses: Mutex::new(q),
            send_chk_calls: AtomicUsize::new(0),
            on_send_chk: Some(spy),
        }
    }

    fn call_count(&self) -> usize {
        self.send_chk_calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl DpsChannel for StubDpsChannel {
    async fn send_chk(&self, _envelope: CheckEnvelope) -> Result<CheckAck, DpsError> {
        self.send_chk_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(spy) = &self.on_send_chk {
            spy();
        }
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("StubDpsChannel response queue empty (caller forgot to enqueue)")
    }

    async fn last_chk(&self, _: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        unreachable!("W7.5 stub: last_chk not exercised")
    }

    async fn ping(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        unreachable!("W7.5 stub: ping not exercised")
    }

    async fn status_rro(&self, _: &CheckSignBlob) -> Result<StatusSnapshot, DpsError> {
        unreachable!("W7.5 stub: status_rro not exercised")
    }

    async fn info_rro(&self, _: &CheckSignBlob) -> Result<RroInfo, DpsError> {
        unreachable!("W7.5 stub: info_rro not exercised")
    }
}

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
    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, '1234567890', ?, ?, ?, 'b1', 't1', 'ONLINE', \
            '2026-05-09T12:34:56Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(lnd)
    .bind(doc_type)
    .bind(state)
    .bind(&sha)
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
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
        .bind(doc)
        .fetch_one(pool)
        .await
        .expect("read state")
}

async fn read_server_fiscal_no(pool: &SqlitePool, doc: DocumentId) -> Option<String> {
    sqlx::query_scalar("SELECT server_fiscal_no FROM fiscal_documents WHERE document_id = ?")
        .bind(doc)
        .fetch_one(pool)
        .await
        .expect("read server_fiscal_no")
}

async fn read_submission_attempted_at(pool: &SqlitePool, doc: DocumentId) -> Option<String> {
    sqlx::query_scalar("SELECT submission_attempted_at FROM fiscal_documents WHERE document_id = ?")
        .bind(doc)
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
async fn transport_retryable_routes_to_error_retryable_preserves_attempt_at() {
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

    assert_eq!(read_doc_state(&pool, doc).await, "ERROR_RETRYABLE");
    assert!(
        read_submission_attempted_at(&pool, doc).await.is_some(),
        "submission_attempted_at must persist on retryable path \
         (forensics + W9 input)"
    );

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
    assert_eq!(read_doc_state(&pool, doc).await, "ERROR_RETRYABLE");

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

// ─── Fixture 3f — Server -11 with missing node_state row → typed error
//
// W10.3 structural-breach proof.  If `node_state` row is missing at
// 4-b time (W5 acquire MUST upsert it before stage 1), the typed
// `NodeStateMissingForBlock` surfaces and the entire 4-b tx rolls
// back — doc stays in SENDING for W9 reconciliation.

#[tokio::test]
async fn server_minus_11_with_missing_node_state_surfaces_typed_error() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_doc_with_xml(&pool, 0xB5, "SELL", 1, "SIGNED").await;
    // **Intentionally NO `node_state::upsert_initial`** — simulate
    // structural breach (W5 invariant violated upstream).

    let stub = StubDpsChannel::new(Err(DpsError::Server {
        code: -11,
        message: "ERROR_OFFLINE_168".into(),
    }));

    let err = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect_err("missing node_state at 4-b must surface typed error");
    match err {
        StageSendError::NodeStateMissingForBlock { fn_id, document_id } => {
            assert_eq!(fn_id, "1234567890");
            assert_eq!(document_id, doc);
        }
        other => panic!("expected NodeStateMissingForBlock, got {other:?}"),
    }

    // Doc state stays in SENDING (4-b tx rolled back).  W9 will pick
    // it up on the next boot.
    assert_eq!(read_doc_state(&pool, doc).await, "SENDING");

    // R-W10.3-review LOW 4 close: 4-pre commits SHOULD persist
    // (CAS doc-state, trace alloc, intent-marker audit), but 4-b
    // SHOULD roll back (trace.completed_at NULL, no result audit
    // row).  This contract is what makes W9 reconciliation safe to
    // pick up Sending docs without losing forensic continuity.
    let traces = transport_trace::list_for_document(&pool, doc)
        .await
        .unwrap();
    assert_eq!(
        traces.len(),
        1,
        "trace allocated in 4-pre persists across 4-b rollback (1 row)"
    );
    assert!(
        traces[0].completed_at.is_none(),
        "4-b rolled back — trace.completed_at must remain NULL"
    );
    assert!(
        traces[0].outcome_kind.is_none(),
        "4-b rolled back — outcome_kind must not be set"
    );
    assert!(
        traces[0].retry_class.is_none(),
        "4-b rolled back — retry_class must not be set"
    );

    // Audit: only the intent marker, no result/blocked entry (4-b
    // rolled back swallowed it).
    assert_eq!(
        read_audit_event_types(&pool, doc).await,
        vec!["STAGE_SEND_INTENT_MARKED"],
        "4-b rollback must drop the result audit row"
    );

    // node_state row is genuinely missing (we never seeded it).  Pin
    // that the failed flip didn't leave a half-applied row behind.
    let node_row = prro::db::repositories::node_state::get(&pool, "1234567890")
        .await
        .unwrap();
    assert!(
        node_row.is_none(),
        "missing-FN scenario must NOT leave a partial node_state row \
         after the 4-b rollback"
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
                    .bind(doc)
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
    // W7.4 F4 + W10.2 HIGH 3 §4.2 close: stage_send.rs 4-pre relies on
    // both `(Signed, Sending)` and `(ErrorRetryable, Sending)` being
    // whitelisted; a future migration that drops either entry would
    // make the `unreachable!` in the Forbidden arm fire in production.
    // This sub-millisecond test catches the regression at CI before it
    // ever ships.
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

// ─── Fixture 6b — Pattern B retry-path: ErrorRetryable → Sending ─────

#[tokio::test]
async fn retry_path_error_retryable_to_sending_drives_through_4_pre() {
    // W10.2 HIGH 3 §4.2: a doc previously routed to ErrorRetryable
    // (e.g. Transport timeout on attempt #1) MUST be picked up by
    // stage_send::run on the next worker tick via the 4-pre CAS
    // (ErrorRetryable, Sending).  This fixture seeds in
    // `ERROR_RETRYABLE`, runs stage_send::run, and asserts:
    //   - attempt_no == 1 (fresh trace row, allocator counts attempts
    //     per-document irrespective of source state)
    //   - the 4-pre CAS Applied (no StateConflict)
    //   - wire send_chk WAS called
    //   - 4-b CAS Sending → Sent committed
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_doc_with_xml(&pool, 0xC1, "SELL", 1, "ERROR_RETRYABLE").await;
    let stub = StubDpsChannel::new(Ok(ack("DPS-FN-RETRY-OK")));

    let outcome = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect("retry from ErrorRetryable must succeed");

    match outcome {
        StageSendOutcome::Sent {
            server_fiscal_no,
            attempt_no: _,
        } => {
            assert_eq!(server_fiscal_no, "DPS-FN-RETRY-OK");
        }
        other => panic!("expected Sent on retry-path, got {other:?}"),
    }
    assert_eq!(
        stub.call_count(),
        1,
        "retry-path must invoke send_chk exactly once"
    );
    assert_eq!(read_doc_state(&pool, doc).await, "SENT");
}

// ─── Fixture 6c — non-{Signed, ErrorRetryable} short-circuits ────────

#[tokio::test]
async fn rerun_on_non_allowlisted_states_short_circuits_with_zero_wire_calls() {
    // W10.2 HIGH 3 §4.2: 4-pre CAS only accepts Signed | ErrorRetryable.
    // Any other source state (Prepared / Kvt1 / Kvt2 / Sending / Rejected
    // / etc.) MUST yield StateConflict + zero wire calls.
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
    //   - SENDING: boot-recovery scenario — a crashed prior worker left
    //     the marker; W9 must own this case, NOT a fresh stage 4.
    //   - SENT, KVT1, KVT2: idempotent re-entry on already-fiscalised
    //     docs (covered partly by fixture 5 for SENT; this widens it).
    //   - REJECTED: terminal state — never re-attempted.
    let (_d, pool) = fresh_pool().await;
    let cases: Vec<(u8, &'static str, DocState)> = vec![
        (0xC2, "PREPARED", DocState::Prepared),
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

    let err = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect_err("empty CheckAck.id must surface as typed error");
    match err {
        StageSendError::EmptyServerFiscalNo { document_id } => {
            assert_eq!(document_id, doc);
        }
        other => panic!("expected EmptyServerFiscalNo, got {other:?}"),
    }
    assert_eq!(
        stub.call_count(),
        1,
        "wire call DID happen — guard runs after"
    );

    // Doc state still SENDING (4-b never ran).
    assert_eq!(read_doc_state(&pool, doc).await, "SENDING");
    // server_fiscal_no NOT written (4-b skipped).
    assert!(read_server_fiscal_no(&pool, doc).await.is_none());

    // Trace row WAS allocated in 4-pre — completed_at remains NULL
    // (forensic crash-window shape).  W9 reconciliation will read
    // this as an unfinished attempt.
    let traces = transport_trace::list_for_document(&pool, doc)
        .await
        .unwrap();
    assert_eq!(traces.len(), 1, "trace allocated in 4-pre persists");
    assert!(
        traces[0].completed_at.is_none(),
        "trace.completed_at NULL — crash-window forensic marker"
    );
    assert!(traces[0].outcome_kind.is_none());

    // Audit: only the intent marker, no result entry.
    assert_eq!(
        read_audit_event_types(&pool, doc).await,
        vec!["STAGE_SEND_INTENT_MARKED"]
    );
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
    let doc_bytes = vec![0x66u8; 16];
    let req_bytes = vec![0x99u8; 16];
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, '1234567890', 1, 'SELL', 'SIGNED', 'b1', 't1', 'ONLINE', \
            '2026-05-09T12:34:56Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
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

// ─── W10.4 step 2d MED 1 close — MAC recovery dispatch smoke fixture ─

/// Deterministic crypto stub for the MAC recovery smoke test.
/// `re_sign_after_mac_recovery` calls `provider.sign_cms_detached`
/// once during the orchestrator's MR-NO-TX step; this stub returns
/// a fixed CMS byte string so the fixture can assert on the post-
/// MR-PERSIST `document_files.SIGNED_XML` content.
struct DetCrypto;

#[async_trait]
impl CryptoProvider for DetCrypto {
    async fn sign_cms_detached(
        &self,
        _: SignCmsRequest<'_>,
    ) -> Result<SignedCmsBytes, CryptoError> {
        Ok(SignedCmsBytes(b"RECOVERED-CMS-V2".to_vec()))
    }
    async fn verify_dstu(
        &self,
        _: &[u8],
        _: &[u8],
        _: &[u8],
    ) -> Result<DstuVerifyResult, CryptoError> {
        unimplemented!("not exercised");
    }
    async fn unwrap_envelope(
        &self,
        _: &[u8],
        _: &[u8],
        _: &SigningSession,
    ) -> Result<Vec<u8>, CryptoError> {
        unimplemented!("not exercised");
    }
    async fn fetch_cert_by_ski(
        &self,
        _: &[String],
        _: &[u8; 32],
        _: std::time::Duration,
    ) -> Result<CertDer, CryptoError> {
        unimplemented!("not exercised");
    }
}

/// Seed an `ERROR_RETRYABLE` doc with PAYLOAD_XML + SIGNED_XML
/// pre-INSERTed (mirrors post-attempt-#1 4-b commit shape per freeze
/// §4.4.1).  `previous_hash` set to `0xAA × 32` so the MAC_RECOVERY_RESIGNED
/// audit's `old_previous_hash_hex` field is observable.
async fn seed_error_retryable_doc_with_artifacts(pool: &SqlitePool, doc_byte: u8) -> DocumentId {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(pool)
    .await
    .unwrap();
    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let sha = vec![0u8; 32];
    let lnd = doc_byte as i64;
    let prev_hash = [0xAAu8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, total_sum_kop, previous_hash) \
         VALUES (?, ?, '1234567890', ?, 'SELL', 'ERROR_RETRYABLE', 'b1', 't1', 'ONLINE', \
            '2026-05-09T12:34:56Z', \
            '{\"items\":[{\"code\":\"p-1\",\"name\":\"Item A\",\"price_kop\":1500,\
              \"quantity_thousandths\":1000,\"sum_kop\":1500}],\
              \"payments\":[{\"name\":\"Cash\",\"sum_kop\":1500,\"type_code\":\"0\"}]}', \
            ?, 1500, ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(lnd)
    .bind(&sha)
    .bind(&prev_hash[..])
    .execute(pool)
    .await
    .expect("seed fiscal_documents (ERROR_RETRYABLE)");
    sqlx::query(
        "INSERT INTO document_files(document_id, kind, content) \
         VALUES (?, 'PAYLOAD_XML', ?), (?, 'SIGNED_XML', ?)",
    )
    .bind(&doc_bytes)
    .bind(b"OLD-PAYLOAD-XML".to_vec())
    .bind(&doc_bytes)
    .bind(b"OLD-SIGNED-CMS".to_vec())
    .execute(pool)
    .await
    .expect("seed document_files");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

#[tokio::test]
async fn mac_recovery_resigned_drives_attempt_2_through_loop_to_sent() {
    // R-W10.4-step2d-review MED 1 close — end-to-end smoke fixture
    // for the loop wrap + sign_ctx propagation + attempt_no increment
    // + trace/audit chain.  Pins the contract that:
    //   1. Server -12 on attempt #1 → orchestrator runs → Resigned →
    //      `continue 'attempt` re-enters 4-pre/4a/4b.
    //   2. Attempt #2 reads the new SIGNED_XML (replaced by orchestrator's
    //      MR-PERSIST), wire send returns OK, doc lands in `Sent` with
    //      attempt_no = 2.
    //   3. Two transport_trace rows materialise: attempt_no=1 with
    //      outcome=RETRYABLE_MAC_HASH_MISMATCH, attempt_no=2 with
    //      outcome=OK.
    //   4. Audit chain documents the full forensic story:
    //      STAGE_SEND_INTENT_MARKED (×2, one per attempt) +
    //      STAGE_SEND_MAC_HASH_MISMATCH (attempt #1 4-b) +
    //      MAC_RECOVERY_RESIGNED (orchestrator MR-PERSIST) +
    //      STAGE_SEND_RESULT (attempt #2 4-b).
    //   5. `mac_recovery_attempts` counter = 1 post-recovery.
    //   6. Stub `send_chk` invoked exactly twice.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_error_retryable_doc_with_artifacts(&pool, 0xC9).await;

    // Stub: queue [Err(-12), Ok(CheckAck)] for attempts #1 and #2.
    let stub = StubDpsChannel::with_queue(vec![
        Err(prro::transports::dps::error::DpsError::Server {
            code: -12,
            message:
                "ERROR_BAD_HASH_PREV: store deadbeef0123456789abcdef0123456789abcdef0123456789abcdef01234567 server-side"
                    .into(),
        }),
        Ok(CheckAck {
            id: "DPS-FN-RECOVERED".into(),
            id_sign: vec![],
            data_sign: vec![],
        }),
    ]);

    // SigningContext for the recovery orchestrator.
    let sign_ctx = SigningContext {
        provider: Arc::new(DetCrypto) as Arc<dyn CryptoProvider>,
        session: SigningSession::new_for_test("operator-1".into(), [0u8; 32], vec![]),
        profile: prro_crypto::cms::profile::CmsProfile::Dstu4145WithGost34311Pb,
    };

    let outcome = stage_send::run(&pool, &stub, doc, Some(&sign_ctx))
        .await
        .expect("end-to-end Resigned-then-Sent must succeed");

    // Outcome shape: Sent with attempt_no=2.
    match outcome {
        StageSendOutcome::Sent {
            server_fiscal_no,
            attempt_no,
        } => {
            assert_eq!(server_fiscal_no, "DPS-FN-RECOVERED");
            assert_eq!(attempt_no, 2, "attempt_no must increment to 2 on re-entry");
        }
        other => panic!("expected Sent (recovered), got {other:?}"),
    }

    // Stub call count: exactly 2 wire sends.
    assert_eq!(stub.call_count(), 2, "send_chk invoked once per attempt");

    // Doc state: SENT.
    assert_eq!(read_doc_state(&pool, doc).await, "SENT");

    // mac_recovery_attempts counter = 1.
    let counter: i64 = sqlx::query_scalar(
        "SELECT mac_recovery_attempts FROM fiscal_documents WHERE document_id = ?",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counter, 1, "single-bit budget burnt by orchestrator");

    // Two trace rows: attempt #1 RETRYABLE_MAC_HASH_MISMATCH, #2 OK.
    let traces = transport_trace::list_for_document(&pool, doc)
        .await
        .unwrap();
    assert_eq!(traces.len(), 2, "one trace row per attempt");
    assert_eq!(traces[0].attempt_no, 1);
    assert_eq!(
        traces[0].outcome_kind.as_deref(),
        Some("RETRYABLE_MAC_HASH_MISMATCH")
    );
    assert_eq!(traces[0].retry_class.as_deref(), Some("MacRecovery"));
    assert_eq!(traces[1].attempt_no, 2);
    assert_eq!(traces[1].outcome_kind.as_deref(), Some("OK"));
    assert_eq!(
        traces[1].server_fiscal_no.as_deref(),
        Some("DPS-FN-RECOVERED")
    );

    // Audit chain.
    let events = read_audit_event_types(&pool, doc).await;
    assert_eq!(
        events,
        vec![
            "STAGE_SEND_INTENT_MARKED",     // attempt #1 4-pre
            "STAGE_SEND_MAC_HASH_MISMATCH", // attempt #1 4-b (routed)
            "MAC_RECOVERY_RESIGNED",        // orchestrator MR-PERSIST
            "STAGE_SEND_INTENT_MARKED",     // attempt #2 4-pre
            "STAGE_SEND_RESULT",            // attempt #2 4-b (Sent)
        ],
        "full forensic audit chain for Resigned end-to-end"
    );

    // SIGNED_XML replaced by orchestrator's MR-PERSIST.
    let signed: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT content FROM document_files WHERE document_id = ? AND kind = 'SIGNED_XML'",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        signed.as_deref(),
        Some(b"RECOVERED-CMS-V2".as_slice()),
        "SIGNED_XML must reflect the orchestrator's re-signed bytes"
    );
}
