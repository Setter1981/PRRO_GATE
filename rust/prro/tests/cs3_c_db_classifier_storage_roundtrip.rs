//! CS-3 Slice C-DB — C-pure classifier ↔ migration 033 storage roundtrip.
//!
//! **Purpose:** prove that every `classify(evidence, doc_type)` output is
//! storable in the real 033 `delivery_reservation` schema — and that the schema
//! is tight (a selection of CHECK-forbidden combos the classifier never emits
//! are rejected).
//!
//! # Structure
//! - `st_*` — **storability**: for every `ClassifiedOutcome` emitted by the
//!   normative graph, build a `delivery_reservation` row at `OUTCOME_OBSERVED`
//!   carrying `(submission_certainty, response_provenance, routing_class,
//!   node_effect)` and assert the INSERT+advance succeeds.
//! - `tg_*` — **tightness**: CHECK-forbidden combos that the classifier never
//!   emits are rejected by 033, proving the schema is not merely permissive.
//! - `ne_*` — **`routing_for_reject` node_effect storability**: every
//!   `(routing, node_effect)` pair from `routing_for_reject` can be stored
//!   at `OUTCOME_OBSERVED` under 033.
//!
//! # Column mapping (C-pure → 033)
//! | C-pure field           | 033 column              |
//! |------------------------|-------------------------|
//! | `SubmissionCertainty`  | `submission_certainty`  |
//! | `ResponseProvenance`   | `response_provenance`   |
//! | `ActiveRetryClass`     | `routing_class`         |
//! | `NodeEffect`           | `node_effect`           |
//!
//! Wire strings are byte-identical: verified in the C-pure test
//! `active_retry_class_wire_strings_match_error_routing` (prro-domain).

#![allow(dead_code)]

use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{self, NewReservation};
use prro::db::tx::with_immediate;
use prro_domain::delivery::{
    classify, AuthorizedGeneration, BoundedText, DpsProtocolBinding, DpsProtocolId, EnvelopeHash,
    NoResponseCause, ObservedOutcomeV1, PositiveGeneration, PreflightRefusal,
    ProtocolContractVersion, RawDpsStatus, RawResponseDigest, RemoteStatusEvidence, SendOutcome,
    SendResponse, SubmissionEvidence,
};
use prro_domain::enums::DocType;
use sqlx::SqlitePool;

// ─────────────────────────── fixtures (mirrors migration_033_dr_activation_schema.rs) ───

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations including 033");
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

/// Seed a fiscal_documents row and return its DocumentId.
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

/// Advance to CALL_STARTED (with authorized_generation set — the RN→CS snapshot).
/// Used for SUBMITTED_UNKNOWN and SUBMITTED paths which require a CS step.
async fn advance_to_cs(pool: &SqlitePool, res_byte: u8) {
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
    .expect("RN→CS with authorized_generation");
}

/// Advance to OUTCOME_OBSERVED via the NOT_SUBMITTED path (RNS→OO directly).
/// No CS step — authorized_generation stays NULL. call_started_at must stay NULL.
async fn advance_rns_to_oo_not_submitted(
    pool: &SqlitePool,
    res_byte: u8,
    routing_class: &str,
    node_effect: &str,
) {
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', apply_state = 'PENDING_APPLY', \
             submission_certainty = 'NOT_SUBMITTED', \
             response_provenance = 'NO_RESPONSE', \
             routing_class = ?, \
             node_effect = ? \
         WHERE reservation_id = ?",
    )
    .bind(routing_class)
    .bind(node_effect)
    .bind(&[res_byte; 16][..])
    .execute(pool)
    .await
    .unwrap_or_else(|e| {
        panic!("RNS→OO NOT_SUBMITTED routing={routing_class} node_effect={node_effect}: {e}")
    });
}

/// Advance to OUTCOME_OBSERVED via CALL_STARTED first (for SUBMITTED_UNKNOWN/SUBMITTED).
/// `routing_class` is None for the clean-accept case (SUBMITTED + NULL routing).
async fn advance_cs_to_oo(
    pool: &SqlitePool,
    res_byte: u8,
    certainty: &str,
    provenance: &str,
    routing_class: Option<&str>,
    node_effect: &str,
) {
    advance_to_cs(pool, res_byte).await;
    match routing_class {
        Some(rc) => {
            sqlx::query(
                "UPDATE delivery_reservation \
                 SET state = 'OUTCOME_OBSERVED', apply_state = 'PENDING_APPLY', \
                     submission_certainty = ?, \
                     response_provenance = ?, \
                     routing_class = ?, \
                     node_effect = ? \
                 WHERE reservation_id = ?",
            )
            .bind(certainty)
            .bind(provenance)
            .bind(rc)
            .bind(node_effect)
            .bind(&[res_byte; 16][..])
            .execute(pool)
            .await
            .unwrap_or_else(|e| {
                panic!(
                    "CS→OO certainty={certainty} provenance={provenance} \
                     routing={rc} node_effect={node_effect}: {e}"
                )
            });
        }
        None => {
            // Clean accept: routing_class stays NULL (the schema allows this
            // for certainty=SUBMITTED per CHECK `:106`).
            sqlx::query(
                "UPDATE delivery_reservation \
                 SET state = 'OUTCOME_OBSERVED', apply_state = 'PENDING_APPLY', \
                     submission_certainty = ?, \
                     response_provenance = ?, \
                     node_effect = ? \
                 WHERE reservation_id = ?",
            )
            .bind(certainty)
            .bind(provenance)
            .bind(node_effect)
            .bind(&[res_byte; 16][..])
            .execute(pool)
            .await
            .unwrap_or_else(|e| {
                panic!(
                    "CS→OO (clean accept) certainty={certainty} provenance={provenance} \
                     node_effect={node_effect}: {e}"
                )
            });
        }
    }
}

fn err_has(err: &sqlx::Error, needle: &str) -> bool {
    err.to_string().to_lowercase().contains(needle)
}

// ─────────────────────────────────────────────────────────────────────────────
// The normative graph rows (from rp4b_2_classify_graph_pin.rs §2 table).
// Represented as flat structs for the roundtrip driver.
// ─────────────────────────────────────────────────────────────────────────────

fn binding() -> DpsProtocolBinding {
    DpsProtocolBinding {
        protocol_id: DpsProtocolId::FscoZzd,
        contract_version: ProtocolContractVersion(1),
        capability_profile_version: None,
        endpoint_config_revision: None,
    }
}
fn ev_hash() -> EnvelopeHash {
    EnvelopeHash([0u8; 32])
}
fn ev_digest() -> RawResponseDigest {
    RawResponseDigest([0u8; 32])
}
fn not_started_preflight() -> SubmissionEvidence {
    SubmissionEvidence::NotStarted {
        reason: PreflightRefusal::PreconditionFailed(BoundedText::from_truncating("guard failed")),
        binding: binding(),
        envelope_hash: ev_hash(),
    }
}
fn not_started_signing() -> SubmissionEvidence {
    SubmissionEvidence::NotStarted {
        reason: PreflightRefusal::SigningFailed(BoundedText::from_truncating("sign error")),
        binding: binding(),
        envelope_hash: ev_hash(),
    }
}
fn started(response: SendResponse) -> SubmissionEvidence {
    SubmissionEvidence::Started {
        response,
        binding: binding(),
        envelope_hash: ev_hash(),
    }
}

/// Build a `Started{Parsed}` evidence from a raw DPS status via the SOLE sealed
/// constructor. `dt` MUST equal the row's classify doc_type (the `-2/-15` close split
/// happens inside `from_dps_status`).
fn parsed(status: RawDpsStatus<'_>, dt: DocType) -> SubmissionEvidence {
    started(SendResponse::Parsed(SendOutcome::from_dps_status(
        status,
        dt,
        ev_digest(),
    )))
}

/// Mapped ClassifiedOutcome from the normative graph (§2 table).
/// `routing` = None for the clean-accept row.
struct GraphRow {
    label: &'static str,
    evidence: SubmissionEvidence,
    doc_type: DocType,
    /// submission_certainty wire string
    certainty: &'static str,
    /// response_provenance wire string
    provenance: &'static str,
    /// routing_class wire string (None = NULL = clean accept)
    routing: Option<&'static str>,
    /// node_effect wire string (from routing_for_reject or NoNodeEffect for non-reject paths)
    node_effect: &'static str,
}

/// Derive the node_effect for each normative-graph row.
/// - NotStarted/PreconditionFailed → TransientRetry → NoNodeEffect
/// - NotStarted/SigningFailed → WrapperBug → WrapperBug
/// - Started/NoResponse(*) → TransientRetry → NoNodeEffect
/// - Started/RemoteStatus(*) → ProbeRequired → ProbeRequired
/// - Started/Parsed(Accepted) → None routing → NoNodeEffect
/// - Started/Parsed(Rejected(*)) → from routing_for_reject (see mod.rs)
/// - Started/Parsed(Indeterminate(*)) → ProbeRequired or TransientRetry
///   For indeterminate, the node_effect is by routing class:
///   TransientRetry → NoNodeEffect, ProbeRequired → ProbeRequired.
///
/// The routing→node_effect derivation for non-reject routing classes:
/// - TransientRetry → NoNodeEffect
/// - WrapperBug → WrapperBug
/// - ProbeRequired → ProbeRequired
fn normative_graph_rows() -> Vec<GraphRow> {
    vec![
        // ── NotStarted ──────────────────────────────────────────────────────────
        GraphRow {
            label: "NotStarted/PreconditionFailed → NOT_SUBMITTED/NO_RESPONSE/TransientRetry",
            evidence: not_started_preflight(),
            doc_type: DocType::Sell,
            certainty: "NOT_SUBMITTED",
            provenance: "NO_RESPONSE",
            routing: Some("TransientRetry"),
            node_effect: "NoNodeEffect",
        },
        GraphRow {
            label: "NotStarted/SigningFailed → NOT_SUBMITTED/NO_RESPONSE/WrapperBug",
            evidence: not_started_signing(),
            doc_type: DocType::Sell,
            certainty: "NOT_SUBMITTED",
            provenance: "NO_RESPONSE",
            routing: Some("WrapperBug"),
            node_effect: "WrapperBug",
        },
        // ── Started/NoResponse ──────────────────────────────────────────────────
        GraphRow {
            label: "Started/NoResponse(Timeout) → SUBMITTED_UNKNOWN/NO_RESPONSE/TransientRetry",
            evidence: started(SendResponse::NoResponse(NoResponseCause::Timeout)),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED_UNKNOWN",
            provenance: "NO_RESPONSE",
            routing: Some("TransientRetry"),
            node_effect: "NoNodeEffect",
        },
        GraphRow {
            label: "Started/NoResponse(Cancelled) → SUBMITTED_UNKNOWN/NO_RESPONSE/TransientRetry",
            evidence: started(SendResponse::NoResponse(NoResponseCause::Cancelled)),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED_UNKNOWN",
            provenance: "NO_RESPONSE",
            routing: Some("TransientRetry"),
            node_effect: "NoNodeEffect",
        },
        GraphRow {
            label: "Started/NoResponse(LocalHandshake) → SUBMITTED_UNKNOWN/NO_RESPONSE/TransientRetry",
            evidence: started(SendResponse::NoResponse(NoResponseCause::LocalHandshakeFailure)),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED_UNKNOWN",
            provenance: "NO_RESPONSE",
            routing: Some("TransientRetry"),
            node_effect: "NoNodeEffect",
        },
        GraphRow {
            label: "Started/NoResponse(Crashed) → SUBMITTED_UNKNOWN/NO_RESPONSE/TransientRetry (RP4B-1)",
            evidence: started(SendResponse::NoResponse(NoResponseCause::CrashedBeforeObservation)),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED_UNKNOWN",
            provenance: "NO_RESPONSE",
            routing: Some("TransientRetry"),
            node_effect: "NoNodeEffect",
        },
        // ── Started/RemoteStatus ────────────────────────────────────────────────
        GraphRow {
            label: "Started/RemoteStatus(Garbage) → SUBMITTED_UNKNOWN/AUTHENTICATED_PEER/ProbeRequired",
            evidence: started(SendResponse::RemoteStatus(RemoteStatusEvidence::AuthenticatedPeerGarbage(ev_digest()))),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED_UNKNOWN",
            provenance: "AUTHENTICATED_PEER",
            routing: Some("ProbeRequired"),
            node_effect: "ProbeRequired",
        },
        GraphRow {
            label: "Started/RemoteStatus(AuthStatus) → SUBMITTED_UNKNOWN/AUTHENTICATED_PEER/ProbeRequired",
            evidence: started(SendResponse::RemoteStatus(RemoteStatusEvidence::RemoteAuthStatus(ev_digest()))),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED_UNKNOWN",
            provenance: "AUTHENTICATED_PEER",
            routing: Some("ProbeRequired"),
            node_effect: "ProbeRequired",
        },
        // ── Started/Parsed(Accepted) ─────────────────────────────────────────────
        GraphRow {
            label: "Started/Parsed(Accepted) → SUBMITTED/PARSED_DPS_ENVELOPE/NULL",
            evidence: parsed(RawDpsStatus::Ok { fiscal_id: "DPS-123" }, DocType::Sell),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED",
            provenance: "PARSED_DPS_ENVELOPE",
            routing: None,
            node_effect: "NoNodeEffect",
        },
        // ── Started/Parsed(Rejected) — named codes via from_dps_status ─────────
        // -1 Verify → TerminalReject / NoNodeEffect
        GraphRow {
            label: "Rejected(Verify/-1) → SUBMITTED/PARSED_DPS_ENVELOPE/TerminalReject/NoNodeEffect",
            evidence: parsed(RawDpsStatus::Error(-1), DocType::Sell),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED",
            provenance: "PARSED_DPS_ENVELOPE",
            routing: Some("TerminalReject"),
            node_effect: "NoNodeEffect",
        },
        // -5 Type → TerminalReject / NoNodeEffect
        GraphRow {
            label: "Rejected(Type/-5) → SUBMITTED/PARSED_DPS_ENVELOPE/TerminalReject/NoNodeEffect",
            evidence: parsed(RawDpsStatus::Error(-5), DocType::Sell),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED",
            provenance: "PARSED_DPS_ENVELOPE",
            routing: Some("TerminalReject"),
            node_effect: "NoNodeEffect",
        },
        // -6 NotPrevZReport → OperatorEscalation / OperatorEscalation
        GraphRow {
            label: "Rejected(NotPrevZReport/-6) → SUBMITTED/PARSED_DPS_ENVELOPE/OperatorEscalation/OperatorEscalation",
            evidence: parsed(RawDpsStatus::Error(-6), DocType::Sell),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED",
            provenance: "PARSED_DPS_ENVELOPE",
            routing: Some("OperatorEscalation"),
            node_effect: "OperatorEscalation",
        },
        // -7 Xml → TerminalReject / NoNodeEffect
        GraphRow {
            label: "Rejected(Xml/-7) → SUBMITTED/PARSED_DPS_ENVELOPE/TerminalReject/NoNodeEffect",
            evidence: parsed(RawDpsStatus::Error(-7), DocType::Sell),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED",
            provenance: "PARSED_DPS_ENVELOPE",
            routing: Some("TerminalReject"),
            node_effect: "NoNodeEffect",
        },
        // -8 XmlDate → TerminalReject / NoNodeEffect
        GraphRow {
            label: "Rejected(XmlDate/-8) → SUBMITTED/PARSED_DPS_ENVELOPE/TerminalReject/NoNodeEffect",
            evidence: parsed(RawDpsStatus::Error(-8), DocType::Sell),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED",
            provenance: "PARSED_DPS_ENVELOPE",
            routing: Some("TerminalReject"),
            node_effect: "NoNodeEffect",
        },
        // -9 XmlChk → TerminalReject / NoNodeEffect
        GraphRow {
            label: "Rejected(XmlChk/-9) → SUBMITTED/PARSED_DPS_ENVELOPE/TerminalReject/NoNodeEffect",
            evidence: parsed(RawDpsStatus::Error(-9), DocType::Sell),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED",
            provenance: "PARSED_DPS_ENVELOPE",
            routing: Some("TerminalReject"),
            node_effect: "NoNodeEffect",
        },
        // -10 XmlZReport → TerminalReject / NoNodeEffect
        GraphRow {
            label: "Rejected(XmlZReport/-10) → SUBMITTED/PARSED_DPS_ENVELOPE/TerminalReject/NoNodeEffect",
            evidence: parsed(RawDpsStatus::Error(-10), DocType::Sell),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED",
            provenance: "PARSED_DPS_ENVELOPE",
            routing: Some("TerminalReject"),
            node_effect: "NoNodeEffect",
        },
        // -11 Offline168 → TerminalReject / NodeBlocked
        GraphRow {
            label: "Rejected(Offline168/-11) → SUBMITTED/PARSED_DPS_ENVELOPE/TerminalReject/NodeBlocked",
            evidence: parsed(RawDpsStatus::Error(-11), DocType::Sell),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED",
            provenance: "PARSED_DPS_ENVELOPE",
            routing: Some("TerminalReject"),
            node_effect: "NodeBlocked",
        },
        // -12 BadHashPrev → MacRecovery / MacReseedPending
        GraphRow {
            label: "Rejected(BadHashPrev/-12) → SUBMITTED/PARSED_DPS_ENVELOPE/MacRecovery/MacReseedPending",
            evidence: parsed(RawDpsStatus::Error(-12), DocType::Sell),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED",
            provenance: "PARSED_DPS_ENVELOPE",
            routing: Some("MacRecovery"),
            node_effect: "MacReseedPending",
        },
        // -13 NotRegisteredRro → FnConfigError / FnConfigError
        GraphRow {
            label: "Rejected(NotRegisteredRro/-13) → SUBMITTED/PARSED_DPS_ENVELOPE/FnConfigError/FnConfigError",
            evidence: parsed(RawDpsStatus::Error(-13), DocType::Sell),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED",
            provenance: "PARSED_DPS_ENVELOPE",
            routing: Some("FnConfigError"),
            node_effect: "FnConfigError",
        },
        // -14 NotRegisteredSigner → FnConfigError / FnConfigError
        GraphRow {
            label: "Rejected(NotRegisteredSigner/-14) → SUBMITTED/PARSED_DPS_ENVELOPE/FnConfigError/FnConfigError",
            evidence: parsed(RawDpsStatus::Error(-14), DocType::Sell),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED",
            provenance: "PARSED_DPS_ENVELOPE",
            routing: Some("FnConfigError"),
            node_effect: "FnConfigError",
        },
        // -16 OfflineId → TerminalReject / NoNodeEffect
        GraphRow {
            label: "Rejected(OfflineId/-16) → SUBMITTED/PARSED_DPS_ENVELOPE/TerminalReject/NoNodeEffect",
            evidence: parsed(RawDpsStatus::Error(-16), DocType::Sell),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED",
            provenance: "PARSED_DPS_ENVELOPE",
            routing: Some("TerminalReject"),
            node_effect: "NoNodeEffect",
        },
        // -2 on a non-close doc (§12 R1) → Rejected(Close) → TerminalReject / NoNodeEffect
        GraphRow {
            label: "Rejected(Close/Sell §12R1) → SUBMITTED/PARSED_DPS_ENVELOPE/TerminalReject/NoNodeEffect",
            evidence: parsed(RawDpsStatus::Error(-2), DocType::Sell),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED",
            provenance: "PARSED_DPS_ENVELOPE",
            routing: Some("TerminalReject"),
            node_effect: "NoNodeEffect",
        },
        // ── Started/Parsed(Indeterminate) ──────────────────────────────────────
        // -4 UnknownStatus → SUBMITTED_UNKNOWN / PARSED_DPS_ENVELOPE / TransientRetry
        GraphRow {
            label: "Indeterminate(Unknown/-4) → SUBMITTED_UNKNOWN/PARSED_DPS_ENVELOPE/TransientRetry",
            evidence: parsed(RawDpsStatus::Error(-4), DocType::Sell),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED_UNKNOWN",
            provenance: "PARSED_DPS_ENVELOPE",
            routing: Some("TransientRetry"),
            node_effect: "NoNodeEffect",
        },
        // -3 SaveError → SUBMITTED_UNKNOWN / PARSED_DPS_ENVELOPE / TransientRetry
        GraphRow {
            label: "Indeterminate(SaveError/-3) → SUBMITTED_UNKNOWN/PARSED_DPS_ENVELOPE/TransientRetry",
            evidence: parsed(RawDpsStatus::Error(-3), DocType::Sell),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED_UNKNOWN",
            provenance: "PARSED_DPS_ENVELOPE",
            routing: Some("TransientRetry"),
            node_effect: "NoNodeEffect",
        },
        // -2 on Z_REPORT (§12 R1) → CloseAmbiguous → SUBMITTED_UNKNOWN / PARSED / ProbeRequired
        GraphRow {
            label: "Indeterminate(CloseAmbiguous/ZReport) → SUBMITTED_UNKNOWN/PARSED_DPS_ENVELOPE/ProbeRequired",
            evidence: parsed(RawDpsStatus::Error(-2), DocType::ZReport),
            doc_type: DocType::ZReport,
            certainty: "SUBMITTED_UNKNOWN",
            provenance: "PARSED_DPS_ENVELOPE",
            routing: Some("ProbeRequired"),
            node_effect: "ProbeRequired",
        },
        // -2 on ShiftClose (§12 R1) → CloseAmbiguous → SUBMITTED_UNKNOWN / PARSED / ProbeRequired
        GraphRow {
            label: "Indeterminate(CloseAmbiguous/ShiftClose) → SUBMITTED_UNKNOWN/PARSED_DPS_ENVELOPE/ProbeRequired",
            evidence: parsed(RawDpsStatus::Error(-2), DocType::ShiftClose),
            doc_type: DocType::ShiftClose,
            certainty: "SUBMITTED_UNKNOWN",
            provenance: "PARSED_DPS_ENVELOPE",
            routing: Some("ProbeRequired"),
            node_effect: "ProbeRequired",
        },
        // OK + empty id → OkButNoFiscalNumber → SUBMITTED_UNKNOWN / PARSED / ProbeRequired
        GraphRow {
            label: "Indeterminate(OkButNoFiscalNumber) → SUBMITTED_UNKNOWN/PARSED_DPS_ENVELOPE/ProbeRequired",
            evidence: parsed(RawDpsStatus::Ok { fiscal_id: "" }, DocType::Sell),
            doc_type: DocType::Sell,
            certainty: "SUBMITTED_UNKNOWN",
            provenance: "PARSED_DPS_ENVELOPE",
            routing: Some("ProbeRequired"),
            node_effect: "ProbeRequired",
        },
    ]
}

// ═══════════════════════════════════════════════════════════════════════════════
// st_* — storability: every classifier output must be 033-storable
// ═══════════════════════════════════════════════════════════════════════════════

/// Drive the full normative graph: for each row, open a fresh DB, build a
/// reservation, advance to OUTCOME_OBSERVED carrying the classifier's output,
/// and assert the advance succeeds (no CHECK/trigger ABORT).
///
/// Two code paths mirror the two real transition edges:
/// - NOT_SUBMITTED → `RNS → OO` directly (no CALL_STARTED step, no authorized_generation).
/// - SUBMITTED_UNKNOWN / SUBMITTED → `RNS → CS → OO`.
///
/// Duplicate rows (same triple — e.g. many Rejected variants map to
/// TerminalReject/NoNodeEffect) are each exercised individually; each fresh DB
/// uses a fresh FN to avoid the fence blocking a second reservation.
#[tokio::test]
async fn st01_every_classifier_output_is_033_storable() {
    // R5: this pin DRIVES the real `classify()` (not a hand-copied output table), and the
    // normative strings stay an INDEPENDENT cross-check.
    // Bridge-0.1 (finding-2): the durable row is now minted THROUGH
    // `ObservedOutcomeV1::record` — the checkpoint found st01 wrote the classify output
    // STRAIGHT to SQL, so `record()`'s NOT_SUBMITTED totality (NotStarted → NULL generation)
    // was never exercised end-to-end. Routing storage through `record` removes that bypass.
    let rows = normative_graph_rows();
    let mut failures: Vec<String> = Vec::new();

    for (idx, row) in rows.iter().enumerate() {
        // Each iteration gets a fresh pool and unique identifiers.
        let (_dir, pool) = fresh_pool().await;
        let doc_byte = (idx as u8).wrapping_add(0x10);
        let res_byte = (idx as u8).wrapping_add(0xA0);
        let lnd = (idx as i64) + 1;

        // ── Drive the REAL sealed classifier and read its output via the getters. ──
        let classified = classify(&row.evidence, row.doc_type);
        let got_cert = classified.certainty().as_str();
        let got_prov = classified.provenance().as_str();
        let got_routing: Option<&str> = classified.routing().map(|r| r.as_str());
        let got_ne = classified.node_effect().as_str();

        // Cross-check the sealed classifier output against the INDEPENDENTLY encoded
        // normative graph (§2). A classify regression / node_effect drift fails HERE,
        // before it can be silently stored — this is the teeth (revert classify → RED).
        if got_cert != row.certainty
            || got_prov != row.provenance
            || got_routing != row.routing
            || got_ne != row.node_effect
        {
            failures.push(format!(
                "\n  CLASSIFY DRIFT [{}]:\n    classify  =(certainty={got_cert}, provenance={got_prov}, routing={got_routing:?}, node_effect={got_ne})\n    normative =(certainty={}, provenance={}, routing={:?}, node_effect={})",
                row.label, row.certainty, row.provenance, row.routing, row.node_effect
            ));
            continue;
        }

        let doc = seed_doc(&pool, FN_A, doc_byte, lnd).await;
        if let Err(e) = insert_res(&pool, new_res(res_byte, doc, FN_A)).await {
            failures.push(format!("\n  FAIL [{}] INSERT failed: {e}", row.label));
            continue;
        }

        // ── Bridge-0.1: mint the durable record THROUGH ObservedOutcomeV1::record. ──
        // The generation witness is derived from the evidence discriminant: the engine/
        // store mints it from durable CALL_STARTED state, and here the evidence IS that
        // state (NotStarted evidence ⇒ no CALL_STARTED ⇒ NotStarted witness ⇒ NULL column).
        let generation = match &row.evidence {
            SubmissionEvidence::NotStarted { .. } => AuthorizedGeneration::NotStarted,
            SubmissionEvidence::Started { .. } => {
                AuthorizedGeneration::Started(PositiveGeneration::new(1).unwrap())
            }
        };
        // record() is TOTAL over legal outcomes and enforces the generation↔certainty
        // invariant. This is the call the checkpoint found bypassed.
        let record = match ObservedOutcomeV1::record(&classified, None, generation) {
            Ok(r) => r,
            Err(e) => {
                failures.push(format!("\n  RECORD MINT [{}] failed: {e:?}", row.label));
                continue;
            }
        };
        let rec_cert = record.certainty().as_str();
        let rec_prov = record.provenance().as_str();
        let rec_routing: Option<&str> = record.routing().map(|r| r.as_str());
        let rec_ne = record.node_effect().as_str();
        // The nullable authorized_generation column comes STRAIGHT from the record witness:
        // NotStarted → None → NULL (the NOT_SUBMITTED totality the checkpoint flagged).
        let rec_gen: Option<i64> = record.authorized_generation().as_db_value();

        // Store the record's fields. A NULL generation ⇒ NOT_SUBMITTED ⇒ RNS → OO directly;
        // a Some generation ⇒ RNS → CS(authorized_generation) → OO.
        let result: Result<(), String> = if rec_gen.is_none() {
            let rc = rec_routing.expect("NOT_SUBMITTED always has a routing class");
            sqlx::query(
                "UPDATE delivery_reservation \
                 SET state = 'OUTCOME_OBSERVED', apply_state = 'PENDING_APPLY', \
                     submission_certainty = ?, \
                     response_provenance = ?, \
                     routing_class = ?, \
                     node_effect = ? \
                 WHERE reservation_id = ?",
            )
            .bind(rec_cert)
            .bind(rec_prov)
            .bind(rc)
            .bind(rec_ne)
            .bind(&[res_byte; 16][..])
            .execute(&pool)
            .await
            .map(|_| ())
            .map_err(|e| format!("RNS→OO NOT_SUBMITTED failed: {e}"))
        } else {
            let cs = sqlx::query(
                "UPDATE delivery_reservation \
                 SET state = 'CALL_STARTED', \
                     call_started_at = '2026-07-17T00:00:00Z', \
                     authorized_generation = ? \
                 WHERE reservation_id = ?",
            )
            .bind(rec_gen)
            .bind(&[res_byte; 16][..])
            .execute(&pool)
            .await;
            match cs {
                Err(e) => Err(format!("RNS→CS failed: {e}")),
                Ok(_) => {
                    let oo = match rec_routing {
                        Some(rc) => {
                            sqlx::query(
                                "UPDATE delivery_reservation \
                                 SET state = 'OUTCOME_OBSERVED', apply_state = 'PENDING_APPLY', \
                                     submission_certainty = ?, \
                                     response_provenance = ?, \
                                     routing_class = ?, \
                                     node_effect = ? \
                                 WHERE reservation_id = ?",
                            )
                            .bind(rec_cert)
                            .bind(rec_prov)
                            .bind(rc)
                            .bind(rec_ne)
                            .bind(&[res_byte; 16][..])
                            .execute(&pool)
                            .await
                        }
                        None => {
                            // Clean accept: routing_class NULL.
                            sqlx::query(
                                "UPDATE delivery_reservation \
                                 SET state = 'OUTCOME_OBSERVED', apply_state = 'PENDING_APPLY', \
                                     submission_certainty = ?, \
                                     response_provenance = ?, \
                                     node_effect = ? \
                                 WHERE reservation_id = ?",
                            )
                            .bind(rec_cert)
                            .bind(rec_prov)
                            .bind(rec_ne)
                            .bind(&[res_byte; 16][..])
                            .execute(&pool)
                            .await
                        }
                    };
                    oo.map(|_| ()).map_err(|e| format!("CS→OO failed: {e}"))
                }
            }
        };

        if let Err(msg) = result {
            failures.push(format!("\n  FAIL [{}]: {msg}", row.label));
            continue;
        }

        // Roundtrip: read the stored columns (incl. authorized_generation) back and assert
        // they equal the record we minted — proves classify → record → 033 store → read is
        // lossless, and that record()'s NOT_SUBMITTED → NULL generation survives end-to-end.
        let stored: (String, String, Option<String>, Option<String>, Option<i64>) = sqlx::query_as(
            "SELECT submission_certainty, response_provenance, routing_class, node_effect, \
                        authorized_generation \
                 FROM delivery_reservation WHERE reservation_id = ?",
        )
        .bind(&[res_byte; 16][..])
        .fetch_one(&pool)
        .await
        .expect("read back stored OUTCOME_OBSERVED row");
        if stored.0 != rec_cert
            || stored.1 != rec_prov
            || stored.2.as_deref() != rec_routing
            || stored.3.as_deref() != Some(rec_ne)
            || stored.4 != rec_gen
        {
            failures.push(format!(
                "\n  ROUNDTRIP DRIFT [{}]: stored={stored:?} != record(certainty={rec_cert}, provenance={rec_prov}, routing={rec_routing:?}, node_effect={rec_ne}, generation={rec_gen:?})",
                row.label
            ));
        }
        // Totality witness: a NOT_SUBMITTED record carries NO generation (NotStarted → NULL)
        // end-to-end — the completeness the checkpoint flagged as unexercised (finding-2).
        if rec_cert == "NOT_SUBMITTED" && stored.4.is_some() {
            failures.push(format!(
                "\n  TOTALITY [{}]: NOT_SUBMITTED must store NULL authorized_generation, got {:?}",
                row.label, stored.4
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "st01: classify()→033 storage roundtrip failures (classify drift, unstorable output, \
         or lossy roundtrip):{}\n",
        failures.join("")
    );
}

/// Verify the OUTCOME_OBSERVED rows round-trip: after st01 succeeds, read back
/// the stored columns and confirm they match what was written.
#[tokio::test]
async fn st02_stored_columns_read_back_correctly() {
    // Pick a representative subset: one per unique (certainty, provenance, routing) triple.
    let cases = [
        // NOT_SUBMITTED / NO_RESPONSE / TransientRetry
        (
            "NOT_SUBMITTED",
            "NO_RESPONSE",
            Some("TransientRetry"),
            "NoNodeEffect",
        ),
        // NOT_SUBMITTED / NO_RESPONSE / WrapperBug
        (
            "NOT_SUBMITTED",
            "NO_RESPONSE",
            Some("WrapperBug"),
            "WrapperBug",
        ),
        // SUBMITTED_UNKNOWN / NO_RESPONSE / TransientRetry
        (
            "SUBMITTED_UNKNOWN",
            "NO_RESPONSE",
            Some("TransientRetry"),
            "NoNodeEffect",
        ),
        // SUBMITTED_UNKNOWN / AUTHENTICATED_PEER / ProbeRequired
        (
            "SUBMITTED_UNKNOWN",
            "AUTHENTICATED_PEER",
            Some("ProbeRequired"),
            "ProbeRequired",
        ),
        // SUBMITTED_UNKNOWN / PARSED_DPS_ENVELOPE / TransientRetry
        (
            "SUBMITTED_UNKNOWN",
            "PARSED_DPS_ENVELOPE",
            Some("TransientRetry"),
            "NoNodeEffect",
        ),
        // SUBMITTED_UNKNOWN / PARSED_DPS_ENVELOPE / ProbeRequired
        (
            "SUBMITTED_UNKNOWN",
            "PARSED_DPS_ENVELOPE",
            Some("ProbeRequired"),
            "ProbeRequired",
        ),
        // SUBMITTED / PARSED_DPS_ENVELOPE / NULL (clean accept)
        ("SUBMITTED", "PARSED_DPS_ENVELOPE", None, "NoNodeEffect"),
        // SUBMITTED / PARSED_DPS_ENVELOPE / TerminalReject / NodeBlocked (-11)
        (
            "SUBMITTED",
            "PARSED_DPS_ENVELOPE",
            Some("TerminalReject"),
            "NodeBlocked",
        ),
        // SUBMITTED / PARSED_DPS_ENVELOPE / MacRecovery / MacReseedPending (-12)
        (
            "SUBMITTED",
            "PARSED_DPS_ENVELOPE",
            Some("MacRecovery"),
            "MacReseedPending",
        ),
        // SUBMITTED / PARSED_DPS_ENVELOPE / FnConfigError / FnConfigError (-13/-14)
        (
            "SUBMITTED",
            "PARSED_DPS_ENVELOPE",
            Some("FnConfigError"),
            "FnConfigError",
        ),
        // SUBMITTED / PARSED_DPS_ENVELOPE / OperatorEscalation / OperatorEscalation (-6)
        (
            "SUBMITTED",
            "PARSED_DPS_ENVELOPE",
            Some("OperatorEscalation"),
            "OperatorEscalation",
        ),
    ];

    for (idx, (certainty, provenance, routing, node_effect)) in cases.iter().enumerate() {
        let (_dir, pool) = fresh_pool().await;
        let doc_byte = (idx as u8).wrapping_add(0x20);
        let res_byte = (idx as u8).wrapping_add(0xC0);
        let lnd = (idx as i64) + 100;

        let doc = seed_doc(&pool, FN_A, doc_byte, lnd).await;
        insert_res(&pool, new_res(res_byte, doc, FN_A))
            .await
            .unwrap_or_else(|e| panic!("insert_res {idx}: {e}"));

        // Advance to OO via the correct path.
        if *certainty == "NOT_SUBMITTED" {
            let rc = routing.expect("NOT_SUBMITTED always has routing");
            sqlx::query(
                "UPDATE delivery_reservation \
                 SET state = 'OUTCOME_OBSERVED', apply_state = 'PENDING_APPLY', \
                     submission_certainty = ?, \
                     response_provenance = 'NO_RESPONSE', \
                     routing_class = ?, \
                     node_effect = ? \
                 WHERE reservation_id = ?",
            )
            .bind(certainty)
            .bind(rc)
            .bind(node_effect)
            .bind(&[res_byte; 16][..])
            .execute(&pool)
            .await
            .unwrap_or_else(|e| panic!("RNS→OO {idx}: {e}"));
        } else {
            sqlx::query(
                "UPDATE delivery_reservation \
                 SET state = 'CALL_STARTED', \
                     call_started_at = '2026-07-17T00:00:00Z', \
                     authorized_generation = 1 \
                 WHERE reservation_id = ?",
            )
            .bind(&[res_byte; 16][..])
            .execute(&pool)
            .await
            .unwrap_or_else(|e| panic!("RNS→CS {idx}: {e}"));

            match routing {
                Some(rc) => {
                    sqlx::query(
                        "UPDATE delivery_reservation \
                         SET state = 'OUTCOME_OBSERVED', apply_state = 'PENDING_APPLY', \
                             submission_certainty = ?, \
                             response_provenance = ?, \
                             routing_class = ?, \
                             node_effect = ? \
                         WHERE reservation_id = ?",
                    )
                    .bind(certainty)
                    .bind(provenance)
                    .bind(rc)
                    .bind(node_effect)
                    .bind(&[res_byte; 16][..])
                    .execute(&pool)
                    .await
                    .unwrap_or_else(|e| panic!("CS→OO with routing {idx}: {e}"));
                }
                None => {
                    sqlx::query(
                        "UPDATE delivery_reservation \
                         SET state = 'OUTCOME_OBSERVED', apply_state = 'PENDING_APPLY', \
                             submission_certainty = ?, \
                             response_provenance = ?, \
                             node_effect = ? \
                         WHERE reservation_id = ?",
                    )
                    .bind(certainty)
                    .bind(provenance)
                    .bind(node_effect)
                    .bind(&[res_byte; 16][..])
                    .execute(&pool)
                    .await
                    .unwrap_or_else(|e| panic!("CS→OO clean accept {idx}: {e}"));
                }
            }
        }

        // Read back and verify.
        let (sc, rp, rc, ne): (String, String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT submission_certainty, response_provenance, \
                         routing_class, node_effect \
                 FROM delivery_reservation WHERE reservation_id = ?",
        )
        .bind(&[res_byte; 16][..])
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|e| panic!("read-back {idx}: {e}"));

        assert_eq!(
            sc.as_str(),
            *certainty,
            "st02[{idx}]: submission_certainty mismatch"
        );
        assert_eq!(
            rp.as_str(),
            *provenance,
            "st02[{idx}]: response_provenance mismatch"
        );
        assert_eq!(
            rc.as_deref(),
            *routing,
            "st02[{idx}]: routing_class mismatch (certainty={certainty} provenance={provenance})"
        );
        assert_eq!(
            ne.as_deref(),
            Some(*node_effect),
            "st02[{idx}]: node_effect mismatch"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// tg_* — tightness: CHECK-forbidden combos that the classifier never emits
//         are rejected by 033 (proving the schema is not merely permissive).
// ═══════════════════════════════════════════════════════════════════════════════

/// `SUBMITTED + NO_RESPONSE` is structurally forbidden by 033:
/// `CHECK (submission_certainty <> 'SUBMITTED' OR response_provenance = 'PARSED_DPS_ENVELOPE')`.
/// The classifier NEVER emits this combo (D3: Submitted requires ParsedDpsEnvelope).
#[tokio::test]
async fn tg01_submitted_plus_no_response_rejected() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x01, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', apply_state = 'PENDING_APPLY', \
             submission_certainty = 'SUBMITTED', \
             response_provenance = 'NO_RESPONSE', \
             routing_class = 'TerminalReject', \
             node_effect = 'NoNodeEffect' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("SUBMITTED + NO_RESPONSE must be rejected by 033 CHECK");

    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "tg01: expected CHECK/constraint for SUBMITTED+NO_RESPONSE; got: {err}"
    );
}

/// `SUBMITTED_UNKNOWN + NULL routing` without being clean-accept is forbidden.
/// The classifier always emits a routing class for SUBMITTED_UNKNOWN rows;
/// 033 enforces this via:
/// `CHECK (routing_class IS NOT NULL OR state <> 'OUTCOME_OBSERVED' OR submission_certainty = 'SUBMITTED')`.
#[tokio::test]
async fn tg02_submitted_unknown_null_routing_rejected() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x02, 2).await;
    insert_res(&pool, new_res(0x02, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x02).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', apply_state = 'PENDING_APPLY', \
             submission_certainty = 'SUBMITTED_UNKNOWN', \
             response_provenance = 'NO_RESPONSE', \
             node_effect = 'NoNodeEffect' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x02u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("SUBMITTED_UNKNOWN + NULL routing must be rejected by 033 CHECK");

    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "tg02: expected CHECK/constraint for SUBMITTED_UNKNOWN+NULL routing; got: {err}"
    );
}

/// `TerminalReject + SUBMITTED_UNKNOWN` is forbidden (response-derived classes
/// require `SUBMITTED`). 033 enforces via:
/// `CHECK (routing_class NOT IN ('TerminalReject',...) OR (certainty='SUBMITTED' AND provenance='PARSED_DPS_ENVELOPE'))`.
/// The classifier NEVER emits this (DpsReject always yields Submitted).
#[tokio::test]
async fn tg03_terminal_reject_with_submitted_unknown_rejected() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x03, 3).await;
    insert_res(&pool, new_res(0x03, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x03).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', apply_state = 'PENDING_APPLY', \
             submission_certainty = 'SUBMITTED_UNKNOWN', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             routing_class = 'TerminalReject', \
             node_effect = 'NoNodeEffect' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x03u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("TerminalReject + SUBMITTED_UNKNOWN must be rejected by 033 CHECK");

    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "tg03: expected CHECK/constraint for TerminalReject+SUBMITTED_UNKNOWN; got: {err}"
    );
}

/// `ProbeRequired + NO_RESPONSE` is forbidden by 033:
/// `CHECK (routing_class <> 'ProbeRequired' OR response_provenance <> 'NO_RESPONSE')`.
/// The classifier never emits ProbeRequired with NO_RESPONSE: NoResponse causes
/// TransientRetry; ProbeRequired comes from RemoteStatus (AuthenticatedPeer) or
/// Indeterminate (ParsedDpsEnvelope).
#[tokio::test]
async fn tg04_probe_required_with_no_response_rejected() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x04, 4).await;
    insert_res(&pool, new_res(0x04, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x04).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', apply_state = 'PENDING_APPLY', \
             submission_certainty = 'SUBMITTED_UNKNOWN', \
             response_provenance = 'NO_RESPONSE', \
             routing_class = 'ProbeRequired', \
             node_effect = 'ProbeRequired' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x04u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("ProbeRequired + NO_RESPONSE must be rejected by 033 CHECK");

    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "tg04: expected CHECK/constraint for ProbeRequired+NO_RESPONSE; got: {err}"
    );
}

/// `MacRecovery + SUBMITTED_UNKNOWN` is forbidden (MacRecovery is a
/// response-derived class requiring `certainty=SUBMITTED + provenance=PARSED_DPS_ENVELOPE`).
/// 033 enforces via the `routing_class NOT IN ('MacRecovery',...)` CHECK.
/// The classifier only emits MacRecovery from `Rejected(BadHashPrev)` which
/// always yields `Submitted`.
#[tokio::test]
async fn tg05_mac_recovery_with_submitted_unknown_rejected() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x05, 5).await;
    insert_res(&pool, new_res(0x05, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x05).await;

    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', apply_state = 'PENDING_APPLY', \
             submission_certainty = 'SUBMITTED_UNKNOWN', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             routing_class = 'MacRecovery', \
             node_effect = 'MacReseedPending' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x05u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("MacRecovery + SUBMITTED_UNKNOWN must be rejected by 033 CHECK");

    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "tg05: expected CHECK/constraint for MacRecovery+SUBMITTED_UNKNOWN; got: {err}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// ne_* — routing_for_reject node_effect storability
//         Every (routing, node_effect) pair from routing_for_reject is 033-storable at OO.
// ═══════════════════════════════════════════════════════════════════════════════

/// Enumerate all `routing_for_reject` outputs and verify each `node_effect` is
/// accepted by the 033 `node_effect` vocabulary CHECK.
///
/// `routing_for_reject` pairs (from mod.rs, exhaustive):
/// - Verify    → (TerminalReject, NoNodeEffect)
/// - Type      → (TerminalReject, NoNodeEffect)
/// - NotPrevZReport → (OperatorEscalation, OperatorEscalation)
/// - Xml       → (TerminalReject, NoNodeEffect)
/// - XmlDate   → (TerminalReject, NoNodeEffect)
/// - XmlChk    → (TerminalReject, NoNodeEffect)
/// - XmlZReport → (TerminalReject, NoNodeEffect)
/// - Offline168 → (TerminalReject, NodeBlocked)
/// - BadHashPrev → (MacRecovery, MacReseedPending)
/// - NotRegisteredRro → (FnConfigError, FnConfigError)
/// - NotRegisteredSigner → (FnConfigError, FnConfigError)
/// - OfflineId → (TerminalReject, NoNodeEffect)
/// - Close     → (TerminalReject, NoNodeEffect)
#[tokio::test]
async fn ne01_routing_for_reject_node_effects_all_033_storable() {
    // All distinct (routing_class, node_effect) pairs from routing_for_reject.
    let pairs: &[(&str, &str)] = &[
        ("TerminalReject", "NoNodeEffect"),
        ("OperatorEscalation", "OperatorEscalation"),
        ("TerminalReject", "NodeBlocked"),
        ("MacRecovery", "MacReseedPending"),
        ("FnConfigError", "FnConfigError"),
    ];

    for (idx, (routing_class, node_effect)) in pairs.iter().enumerate() {
        let (_dir, pool) = fresh_pool().await;
        let doc_byte = (idx as u8).wrapping_add(0x50);
        let res_byte = (idx as u8).wrapping_add(0xD0);
        let lnd = (idx as i64) + 200;

        let doc = seed_doc(&pool, FN_A, doc_byte, lnd).await;
        insert_res(&pool, new_res(res_byte, doc, FN_A))
            .await
            .unwrap_or_else(|e| panic!("ne01[{idx}] insert: {e}"));

        // All routing_for_reject paths are SUBMITTED + PARSED_DPS_ENVELOPE → CS path.
        sqlx::query(
            "UPDATE delivery_reservation \
             SET state = 'CALL_STARTED', \
                 call_started_at = '2026-07-17T00:00:00Z', \
                 authorized_generation = 1 \
             WHERE reservation_id = ?",
        )
        .bind(&[res_byte; 16][..])
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("ne01[{idx}] CS: {e}"));

        sqlx::query(
            "UPDATE delivery_reservation \
             SET state = 'OUTCOME_OBSERVED', apply_state = 'PENDING_APPLY', \
                 submission_certainty = 'SUBMITTED', \
                 response_provenance = 'PARSED_DPS_ENVELOPE', \
                 routing_class = ?, \
                 node_effect = ? \
             WHERE reservation_id = ?",
        )
        .bind(routing_class)
        .bind(node_effect)
        .bind(&[res_byte; 16][..])
        .execute(&pool)
        .await
        .unwrap_or_else(|e| {
            panic!("ne01[{idx}] OO routing={routing_class} node_effect={node_effect}: {e}")
        });

        // Read back the node_effect to confirm it round-trips.
        let stored_ne: Option<String> = sqlx::query_scalar(
            "SELECT node_effect FROM delivery_reservation WHERE reservation_id = ?",
        )
        .bind(&[res_byte; 16][..])
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|e| panic!("ne01[{idx}] read-back: {e}"));

        assert_eq!(
            stored_ne.as_deref(),
            Some(*node_effect),
            "ne01[{idx}]: node_effect read-back mismatch for routing={routing_class}"
        );
    }
}
