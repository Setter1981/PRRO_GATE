//! CS-3 Slice E Pin 5 — directed teeth for the `UnknownStatus → ProbeRequired` leaf (§6).
//!
//! The invariant fuzzer CANNOT reach this leaf: `WireResponse` has no unknown-status op, `wire_to_result`
//! hands the model an already-collapsed `Result<CheckAck, DpsError>`, and a faithful adapter turns an
//! `Indeterminate(-4)` into `NoResponse` (`dto.rs`). So this is the precise directed path instead: a REAL
//! `gen::CheckResponse{ status: -4 / -17 }` fed through the PRODUCTION `observe_check_reply` decode (via
//! the `scripted_raw_observation` test-support helper — NOT `scripted_observation`, which degrades), then
//! driven end-to-end through `stage_send::run`.
//!
//! Asserts the whole chain for the `UnknownStatus` leaf: durable reservation axes
//! (`SUBMITTED_UNKNOWN / PARSED_DPS_ENVELOPE / ProbeRequired / ProbeRequired`, `evidence_kind=UnknownStatus`
//! plus the raw code), the doc HELD (SENDING, no ErrorRetryable re-drive, no Rejected), EXACTLY ONE wire, no
//! `last_chk` auto-probe, and the returned `SubmittedUnknown` probe.
//!
//! Revert-canary: revert the classifier flip (`routing_for_indeterminate(UnknownStatus) → TransientRetry`,
//! prro-domain `mod.rs`) and these tests RED — `record_outcome` then writes `TransientRetry`, which
//! migration 038's fresh-arm matrix REJECTS (`CALL_STARTED ⇒ ProbeRequired only`), so `stage_send::run`
//! errors out before the axes ever match.

use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use sqlx::SqlitePool;

use prro::db::models::ids::DocumentId;
use prro::db::types::DbDocumentId;
use prro::services::write_path::error_routing::{ProbeReason, RetryClass};
use prro::services::write_path::stage_send::{self, StageSendOutcome};
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{
    self, CheckAck, CheckEnvelope, CheckSignBlob, OfflineCodesResponse, RroInfo, StatusSnapshot,
};
use prro::transports::dps::error::DpsError;
use prro::transports::dps::gen;
use prro::transports::dps::raw_reply::RawSendObservation;

// ─── Raw-status mock: returns the REAL decode of a raw CheckResponse ─────────────

/// A `DpsChannel` whose single `send_chk_observed` returns the production decode of a raw
/// `gen::CheckResponse{status}` (via [`dto::scripted_raw_observation`]) — the ONLY way to exercise the
/// `UnknownStatus` leaf (a `-4`/`-17` code the faithful-legacy adapter cannot rebuild). Counts wires;
/// `last_chk` is `unreachable!` so an unexpected auto-probe during the send is caught as a panic.
struct RawStatusDps {
    status: i32,
    send_calls: AtomicUsize,
}

impl RawStatusDps {
    fn new(status: i32) -> Self {
        Self {
            status,
            send_calls: AtomicUsize::new(0),
        }
    }
    fn resp(&self) -> gen::CheckResponse {
        gen::CheckResponse {
            id: String::new(),
            status: self.status,
            id_sign: Vec::new(),
            data_sign: Vec::new(),
            error_message: String::new(),
        }
    }
    fn wires(&self) -> usize {
        self.send_calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl DpsChannel for RawStatusDps {
    async fn send_chk(&self, _envelope: CheckEnvelope) -> Result<CheckAck, DpsError> {
        dto::scripted_raw_observation(self.resp()).0
    }
    async fn send_chk_observed(
        &self,
        _envelope: CheckEnvelope,
    ) -> (Result<CheckAck, DpsError>, RawSendObservation) {
        self.send_calls.fetch_add(1, Ordering::SeqCst);
        dto::scripted_raw_observation(self.resp())
    }
    async fn last_chk(&self, _: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        unreachable!("held UnknownStatus must NOT auto-probe (last_chk) during the send")
    }
    async fn ping(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        unreachable!("stub: ping not exercised")
    }
    async fn status_rro(&self, _: &CheckSignBlob) -> Result<StatusSnapshot, DpsError> {
        unreachable!("stub: status_rro not exercised")
    }
    async fn info_rro(&self, _: &CheckSignBlob) -> Result<RroInfo, DpsError> {
        unreachable!("stub: info_rro not exercised")
    }
    async fn ask_offline_codes(&self, _: CheckEnvelope) -> Result<OfflineCodesResponse, DpsError> {
        unreachable!("stub: ask_offline_codes not exercised")
    }
}

// ─── Pool / seed helpers (SELL, mirrors write_path_dps_error_routing's seed shape) ──

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

/// Seed FN config + node_state + shift + a SIGNED SELL doc (+ PAYLOAD_XML / SIGNED_XML), ready for
/// `stage_send::run`. `lnd = doc_byte` dodges the partial UNIQUE `(fiscal_number, lnd)` index.
async fn seed_signed_sell(pool: &SqlitePool, doc_byte: u8) -> DocumentId {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO node_state (fiscal_number, mode, shift_state, next_lnd) \
         VALUES ('1234567890', 'ONLINE', 'OPENED', 1)",
    )
    .execute(pool)
    .await
    .unwrap();
    let shift_bytes = vec![doc_byte ^ 0x80; 16];
    sqlx::query(
        "INSERT OR IGNORE INTO shifts(shift_id, fiscal_number, serial, state, \
            open_mode, cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, '1234567890', 1, 'OPENED', 'ONLINE', 0, 'test-cashier')",
    )
    .bind(&shift_bytes)
    .execute(pool)
    .await
    .expect("seed shift");

    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let sha = vec![0u8; 32];
    let lnd = doc_byte as i64;
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, shift_id, lnd, \
            doc_type, state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, total_sum_kop, signed_by_cashier_id) \
         VALUES (?, ?, '1234567890', ?, ?, 'SELL', 'SIGNED', 'b1', 't1', 'ONLINE', \
            '2026-05-09T12:34:56Z', ?, ?, 1500, 'test-cashier')",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(&shift_bytes)
    .bind(lnd)
    .bind(SELL_PAYLOAD_JSON)
    .bind(&sha)
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

async fn read_state(pool: &SqlitePool, doc: DocumentId) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(DbDocumentId(doc))
        .fetch_one(pool)
        .await
        .unwrap()
}

type Axes = (String, String, Option<String>, String, String, Option<i64>);

/// (submission_certainty, response_provenance, routing_class, node_effect, evidence_kind, evidence_code)
/// for the doc's reservation.
async fn read_reservation_axes(pool: &SqlitePool, doc: DocumentId) -> Axes {
    sqlx::query_as(
        "SELECT submission_certainty, response_provenance, routing_class, node_effect, \
                evidence_kind, evidence_code \
         FROM delivery_reservation WHERE document_id = ?",
    )
    .bind(DbDocumentId(doc))
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The durable HOLD markers: (reservation `apply_state`, `node_state.mode`, whether the FN fence
/// pointer `active_delivery_reservation_id` is still SET). `1234567890` is the seed FN.
async fn read_hold_markers(pool: &SqlitePool, doc: DocumentId) -> (String, String, bool) {
    let apply_state: String =
        sqlx::query_scalar("SELECT apply_state FROM delivery_reservation WHERE document_id = ?")
            .bind(DbDocumentId(doc))
            .fetch_one(pool)
            .await
            .unwrap();
    let (mode, fence): (String, Option<Vec<u8>>) = sqlx::query_as(
        "SELECT mode, active_delivery_reservation_id FROM node_state WHERE fiscal_number = '1234567890'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    (apply_state, mode, fence.is_some())
}

/// The shared body: a raw `status` (`-4` or `-17`) drives the `UnknownStatus → ProbeRequired` leaf.
async fn assert_unknown_status_holds_probe(status: i32, doc_byte: u8) {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed_sell(&pool, doc_byte).await;
    let dps = RawStatusDps::new(status);

    let outcome = stage_send::run(&pool, &dps, doc, None)
        .await
        .expect("held UnknownStatus is NOT an error (FIX-C): it rests PENDING_APPLY under STOP");

    // EXACTLY ONE wire; no auto-resend, no auto-probe (last_chk is unreachable! in the mock).
    assert_eq!(
        dps.wires(),
        1,
        "status={status}: exactly one send_chk_observed"
    );

    // Durable reservation axes ARE the persisted classifier output — the whole decode→classify→record
    // chain, verified end-to-end.
    let (certainty, provenance, routing, node_effect, evidence_kind, evidence_code) =
        read_reservation_axes(&pool, doc).await;
    assert_eq!(certainty, "SUBMITTED_UNKNOWN", "status={status}");
    assert_eq!(provenance, "PARSED_DPS_ENVELOPE", "status={status}");
    assert_eq!(
        routing.as_deref(),
        Some("ProbeRequired"),
        "status={status}: UnknownStatus HOLDS (Pin 3 flip), not TransientRetry"
    );
    assert_eq!(node_effect, "ProbeRequired", "status={status}");
    assert_eq!(evidence_kind, "UnknownStatus", "status={status}");
    assert_eq!(
        evidence_code,
        Some(i64::from(status)),
        "status={status}: raw code preserved"
    );

    // STOP/fence held — the doc rests SENDING (NOT ErrorRetryable re-drive, NOT Rejected) AND the
    // three durable hold markers are set: the reservation stays PENDING_APPLY (apply did NOT release
    // it — `record_outcome`/`apply_recorded_outcome` HeldNotAutoRelease), the node is halted STOP_MODE
    // (record owns the SubmittedUnknown halt, `delivery_reservation.rs` set_mode_stop_mode_tx), and the
    // FN fence pointer is NOT cleared (the held reservation is still the node's active one).
    assert_eq!(
        read_state(&pool, doc).await,
        "SENDING",
        "status={status}: held SENDING under STOP"
    );
    let (apply_state, node_mode, fence_set) = read_hold_markers(&pool, doc).await;
    assert_eq!(
        apply_state, "PENDING_APPLY",
        "status={status}: reservation HELD PENDING_APPLY (apply did not auto-release)"
    );
    assert_eq!(
        node_mode, "STOP_MODE",
        "status={status}: node halted STOP_MODE (SubmittedUnknown hold)"
    );
    assert!(
        fence_set,
        "status={status}: the FN fence pointer (active_delivery_reservation_id) is NOT cleared"
    );

    // The returned outcome carries the ProbeRequired routing + the SubmittedUnknown probe reason.
    match outcome {
        StageSendOutcome::Routed { decision, .. } => {
            assert_eq!(
                decision.retry_class,
                RetryClass::ProbeRequired,
                "status={status}"
            );
            assert_eq!(
                decision.probe_hint.as_ref().map(|h| h.reason),
                Some(ProbeReason::SubmittedUnknown),
                "status={status}: SubmittedUnknown probe (activated by Pin 3)"
            );
        }
        other => panic!("status={status}: expected Routed probe, got {other:?}"),
    }
}

/// `-4` (ERROR_UNKNOWN) — inside the proto enum but outside the named reject/accept set → UnknownStatus.
#[tokio::test]
async fn unknown_status_minus_4_holds_probe_required_single_wire() {
    assert_unknown_status_holds_probe(-4, 0xD4).await;
}

/// `-17` — outside the proto enum entirely (tonic decode fails) → still the UnknownStatus leaf.
#[tokio::test]
async fn unknown_status_minus_17_holds_probe_required_single_wire() {
    assert_unknown_status_holds_probe(-17, 0xE7).await;
}
