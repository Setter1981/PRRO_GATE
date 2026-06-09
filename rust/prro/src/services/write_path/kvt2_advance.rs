//! Runtime-neutral KVT2 confirmer — the `Sent → Kvt1 → Kvt2 → Ack`
//! advance, extracted from the W12 drain wrapper
//! (`services::offline_sync::kvt2_confirm::confirm_drain_doc`) per
//! **RS-3 A2.1a** (`docs/superpowers/plans/2026-06-09-rs3-a2-implementation.md`
//! §A2.1a, decision (a)).
//!
//! ## Why this exists
//!
//! The online happy-path ends at `Sent` (`stage_send.rs`); `stage_finalize::run`
//! starts at `Kvt2`.  The `Sent → Kvt1 → Kvt2` quittance lived ONLY inside
//! `kvt2_confirm`, behind the drain wrapper that leaks `BootError`
//! (`app::BootError` is boot-fatal) and the drain source-routing matrix
//! (SentFresh / SentReplay / Kvt1Reentry).  The inline online orchestrator
//! (A2.1b) must drive that same quittance to reach ACK inline, but it needs
//! a `FiscalError`/IN_PROGRESS shape, NOT boot-fatal semantics.
//!
//! [`advance_to_ack`] is that shared, runtime-neutral core:
//!   - **NO `BootError`** — failures surface as the local [`ConfirmError`]
//!     taxonomy; each caller maps it at its own boundary (the W12 drain
//!     orchestrator → `BootError`; the A2.1b online worker → `FiscalError`).
//!   - **NO drain source-routing** — the `transport_trace` recovery-row
//!     lifecycle (SentReplay's Envelope 1c-pre / 1a-replay bundling) is
//!     drain-specific and stays in `kvt2_confirm`.  This core owns ONLY the
//!     two *pure* CAS paths: Envelope 1a (Sent-entry) and Envelope 1b
//!     (Kvt1-entry).
//!
//! ## What it does (behaviour-preserving extraction)
//!
//! One atomic CAS-chain envelope, then `stage_finalize::run`:
//!   - `doc_state_at_entry == Kvt1` → **Envelope 1b** (2-CAS: persist
//!     `Kvt1Raw` + `Kvt1 → Kvt2` + reset-holds + audit).  No `Sent → Kvt1`.
//!   - otherwise → **Envelope 1a** (3-CAS: persist `Kvt1Raw` +
//!     `Sent → Kvt1` + `Kvt1 → Kvt2` + reset-holds + audit).  The *actual*
//!     DB CAS is always `Sent → Kvt1 → Kvt2`; `doc_state_at_entry` only
//!     labels the audit `from_state` (the cohort-walker snapshot for drain —
//!     `OFFLINE_LOCAL_ACK` / `ERROR_RETRYABLE` — or `Sent` for the online
//!     ladder).  This decoupling matches the pre-extraction drain contract
//!     (LOW-PR70-R12-02 cohort-entry convention).
//!   - then **Envelope 2** (`stage_finalize::run`, `Kvt2 → Ack`).
//!
//! ## I1 / invariant #1 preserved
//!
//! No DPS / crypto IO happens here: the canonical `Kvt1Raw` evidence
//! (`kvt1_raw_bytes`, the byte-for-byte lastChk `data_sign` per HIGH-C5-2)
//! is fetched by the caller OUTSIDE any `with_immediate`.  This module only
//! does short DB-only envelopes.

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::db::models::enums::{DocState, Severity};
use crate::db::models::ids::DocumentId;
use crate::db::repositories::fiscal_documents::TransitionOutcome;
use crate::db::repositories::{audit_log, document_files, fiscal_documents};
use crate::db::tx::with_immediate;
use crate::services::write_path::stage_finalize;
use crate::services::write_path::types::hex_encode_lower;

/// Audit `entity_type` for `fiscal_documents` rows.  Mirrors the
/// repo-wide contract (`offline_sync::backlog_drain::AUDIT_ENTITY_DOC`,
/// also hardcoded in `reconciliation::boot_phase`).  Defined locally so
/// this write-path module stays self-contained (no backwards dependency
/// on the higher-level `offline_sync` drain layer).
const AUDIT_ENTITY_DOC: &str = "fiscal_document";

/// Typed error surface for [`advance_to_ack`].  Runtime-neutral: NO
/// `BootError`, NO drain source-routing.  Each caller maps these at its
/// own boundary — the W12 drain orchestrator → `BootError`
/// (StructuralDrift → `Internal`; Database / Infrastructure →
/// `ReconciliationFailed`, preserving the pre-extraction drain semantics);
/// the A2.1b online worker → `FiscalError`.
///
/// The split preserves the *cause/detail* of each pre-extraction failure
/// so the drain boundary can reconstruct the exact `BootError` variant it
/// produced before A2.1a.
#[derive(Debug, thiserror::Error)]
pub enum ConfirmError {
    /// State-machine invariant breach during the advance: empty evidence
    /// bytes (a `classify_check_result` routing regression), or
    /// `stage_finalize::run` observing `StateConflict` / `DocumentMissing`
    /// after Envelope 1 just committed the `Kvt1 → Kvt2` CAS.  Fail-loud
    /// (drain → `BootError::Internal`; online → 500/Internal).  `detail`
    /// carries the human-readable forensic description.
    #[error("kvt2 advance structural drift: {detail}")]
    StructuralDrift { detail: String },

    /// Envelope 1a/1b commit failed — a sqlx error OR a CAS-miss
    /// (`Sent → Kvt1` / `Kvt1 → Kvt2` producing non-`Applied`).  Drain →
    /// `BootError::ReconciliationFailed` (same per-FN-attributed surface
    /// the inline envelope helpers produced before A2.1a).
    #[error("kvt2 advance envelope db failure: {source}")]
    Database {
        #[source]
        source: anyhow::Error,
    },

    /// `stage_finalize::run` (Envelope 2, `Kvt2 → Ack`) returned a typed
    /// [`stage_finalize::StageFinalizeError`].  Cause chain preserved.
    /// Drain → `BootError::ReconciliationFailed` (same as the inline
    /// `stage_finalize::run` error map before A2.1a).
    #[error("kvt2 advance finalize failure: {source}")]
    Infrastructure {
        #[source]
        source: anyhow::Error,
    },
}

/// Advance a doc through `Kvt2` to terminal `Ack`, driven by the canonical
/// `Kvt1Raw` evidence the caller already fetched from DPS lastChk.
///
/// See the module docs for the full contract.  In short:
///   1. `kvt1_raw_bytes` MUST be non-empty (byte-for-byte lastChk
///      `data_sign`); empty → [`ConfirmError::StructuralDrift`] (a routing
///      regression — empty `data_sign` must route to Hold upstream, never
///      reach this advance).
///   2. Envelope 1 — source-selected CAS chain to `Kvt2`:
///      `doc_state_at_entry == Kvt1` → Envelope 1b (2-CAS); otherwise →
///      Envelope 1a (3-CAS, actual CAS `Sent → Kvt1 → Kvt2`).
///   3. Envelope 2 — `stage_finalize::run` (`Kvt2 → Ack`).
///
/// `doc_state_at_entry` is documented as `Sent | Kvt1` for the online
/// ladder (A2.1b); the W12 drain caller passes the cohort-walker snapshot
/// (`OFFLINE_LOCAL_ACK` / `ERROR_RETRYABLE` / `Sent`) for the audit
/// `from_state`, which dispatches to Envelope 1a (anything not `Kvt1`).
///
/// `attempt_no` is threaded into the Envelope 1a audit payload
/// (operator-dashboard continuity with the wire attempt); it is ignored on
/// the Envelope 1b path (Kvt1 re-entry carries no fresh wire attempt).
pub async fn advance_to_ack(
    pool: &SqlitePool,
    doc_id: DocumentId,
    kvt1_raw_bytes: Vec<u8>,
    server_fiscal_no: &str,
    doc_state_at_entry: DocState,
    attempt_no: Option<i64>,
) -> Result<(), ConfirmError> {
    let id_hex = hex_encode_lower(doc_id.as_bytes());

    // Defensive non-empty guard.  `classify_check_result` routes empty
    // `data_sign` to Hold(LastChkDataSignEmpty), so a non-empty Acked
    // carrying empty bytes is an upstream routing regression — fail-loud
    // BEFORE persisting an empty `Kvt1Raw` row that would silently corrupt
    // forensic evidence.
    if kvt1_raw_bytes.is_empty() {
        return Err(ConfirmError::StructuralDrift {
            detail: format!(
                "advance_to_ack: Acked evidence with empty kvt1_raw_bytes for \
                 doc {id_hex} — evidence-routing invariant violated (empty \
                 data_sign must route to Hold upstream, never reach the \
                 Kvt2 advance)"
            ),
        });
    }

    // Envelope 1: source-selected atomic CAS chain to Kvt2.
    match doc_state_at_entry {
        DocState::Kvt1 => {
            commit_kvt1_entry_envelope(pool, doc_id, &id_hex, kvt1_raw_bytes, server_fiscal_no)
                .await?;
        }
        _ => {
            commit_sent_entry_envelope(
                pool,
                doc_id,
                &id_hex,
                kvt1_raw_bytes,
                server_fiscal_no,
                doc_state_at_entry,
                attempt_no,
            )
            .await?;
        }
    }

    // Envelope 2: M3a `stage_finalize::run` owns its 5-write atomic
    // envelope (Kvt2→Ack + chain seed advance + inbox DONE + outbox +
    // STAGE_FINALIZE_ACK).  Acked / AlreadyAcked are both success-shapes.
    let finalize_outcome =
        stage_finalize::run(pool, doc_id)
            .await
            .map_err(|err| ConfirmError::Infrastructure {
                source: anyhow::Error::new(err),
            })?;
    match finalize_outcome {
        stage_finalize::StageFinalizeOutcome::Acked { .. }
        | stage_finalize::StageFinalizeOutcome::AlreadyAcked => Ok(()),
        stage_finalize::StageFinalizeOutcome::StateConflict { observed } => {
            Err(ConfirmError::StructuralDrift {
                detail: format!(
                    "advance_to_ack: stage_finalize::run StateConflict \
                     {{ observed: {observed} }} for doc {id_hex} — Envelope 1 \
                     just committed Kvt1→Kvt2, so a different observed state \
                     means a concurrent writer rolled it forward between \
                     Envelope 1 commit and Envelope 2 read",
                    observed = observed.as_str(),
                ),
            })
        }
        stage_finalize::StageFinalizeOutcome::DocumentMissing => {
            Err(ConfirmError::StructuralDrift {
                detail: format!(
                    "advance_to_ack: stage_finalize::run DocumentMissing for \
                     doc {id_hex} — row vanished between Envelope 1 commit and \
                     Envelope 2 read (single-writer invariant breach)"
                ),
            })
        }
    }
}

/// Envelope 1a (Sent-entry) — one atomic `with_immediate`: persist
/// `Kvt1Raw`, CAS `Sent → Kvt1`, CAS `Kvt1 → Kvt2`, reset consecutive-holds,
/// then append the `OFFLINE_DRAIN_KVT2_ADVANCED` audit.
///
/// `doc_state_for_audit` labels the audit `from_state` — it is the
/// user-visible "from" (the cohort-walker snapshot for drain, e.g.
/// `OFFLINE_LOCAL_ACK` / `ERROR_RETRYABLE`, or `Sent` for the online
/// ladder), which may differ from the *actual* DB state (`Sent`) the CAS
/// transitions from.  `attempt_no` serializes to JSON `null` when `None`.
async fn commit_sent_entry_envelope(
    pool: &SqlitePool,
    doc_id: DocumentId,
    id_hex: &str,
    kvt1_raw_bytes: Vec<u8>,
    server_fiscal_no: &str,
    doc_state_for_audit: DocState,
    attempt_no: Option<i64>,
) -> Result<(), ConfirmError> {
    // SHA256 of the persisted Kvt1Raw evidence — audit-trail cross-link to
    // the `document_files.Kvt1Raw` blob (plan §62 audit-digest contract).
    // Computed BEFORE the move into the closure (bytes are consumed by
    // `document_files::replace_tx`).
    let kvt1_raw_sha256_hex = format!("{:x}", Sha256::digest(&kvt1_raw_bytes));
    let payload = serde_json::json!({
        "document_id": id_hex,
        "from_state": doc_state_for_audit.as_str(),
        "to_state": DocState::Kvt2.as_str(),
        "server_fiscal_no": server_fiscal_no,
        "dispatch_via": "kvt2_confirm",
        "evidence_source": "lastChk",
        "kvt1_raw_sha256_hex": kvt1_raw_sha256_hex,
        "attempt_no": attempt_no,
    });
    let payload_owned = payload.to_string();
    let id_hex_owned = id_hex.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            // (a) Persist Kvt1Raw evidence byte-for-byte.
            document_files::replace_tx(
                tx,
                doc_id,
                document_files::DocumentFileKind::Kvt1Raw,
                &kvt1_raw_bytes,
            )
            .await?;
            // (b) CAS Sent → Kvt1.  The doc reached Sent earlier this tick
            // (drain stage_send, or online stage_send) in its own envelope.
            let sent_to_kvt1 =
                fiscal_documents::transition_state(tx, doc_id, DocState::Sent, DocState::Kvt1)
                    .await?;
            if sent_to_kvt1 != TransitionOutcome::Applied {
                return Err(anyhow::anyhow!(
                    "advance_to_ack: Envelope 1a CAS Sent→Kvt1 produced \
                     {outcome:?} for doc {doc_hex} (expected Applied — \
                     single-writer invariant breach OR doc not at Sent)",
                    outcome = sent_to_kvt1,
                    doc_hex = id_hex_owned,
                ));
            }
            // (c) CAS Kvt1 → Kvt2.  W12 advance proof now persisted.
            let kvt1_to_kvt2 =
                fiscal_documents::transition_state(tx, doc_id, DocState::Kvt1, DocState::Kvt2)
                    .await?;
            if kvt1_to_kvt2 != TransitionOutcome::Applied {
                return Err(anyhow::anyhow!(
                    "advance_to_ack: Envelope 1a CAS Kvt1→Kvt2 produced \
                     {outcome:?} for doc {doc_hex} (just CAS'd to Kvt1 in same \
                     envelope — concurrent writer impossible)",
                    outcome = kvt1_to_kvt2,
                    doc_hex = id_hex_owned,
                ));
            }
            // (d) Reset consecutive_holds — advancing through Kvt2 clears
            // any prior Hold accumulation (REC-1 Tier reset semantics).
            fiscal_documents::reset_consecutive_holds_tx(tx, doc_id).await?;
            // (e) Forensic audit row.
            audit_log::append_tx(
                tx,
                AUDIT_ENTITY_DOC,
                &id_hex_owned,
                "OFFLINE_DRAIN_KVT2_ADVANCED",
                Severity::Info,
                None,
                Some(&payload_owned),
            )
            .await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await
    .map_err(|source| ConfirmError::Database { source })?;
    Ok(())
}

/// Envelope 1b (Kvt1-entry) — one atomic `with_immediate`: persist
/// `Kvt1Raw`, CAS `Kvt1 → Kvt2`, reset consecutive-holds, then append the
/// `OFFLINE_DRAIN_KVT2_ADVANCED` audit.
///
/// **Difference from Envelope 1a**: NO `Sent → Kvt1` CAS — the doc was
/// already at `Kvt1` at entry (boot recovery OR a prior-tick W12 Hold left
/// it there).  `Kvt1Raw` is re-persisted via INSERT-OR-REPLACE so the
/// freshly-fetched canonical bytes win.  The audit payload omits
/// `attempt_no` (no fresh wire attempt this entry — distinguishes
/// Kvt1-entry advance from Sent-entry advance for operator dashboards).
async fn commit_kvt1_entry_envelope(
    pool: &SqlitePool,
    doc_id: DocumentId,
    id_hex: &str,
    kvt1_raw_bytes: Vec<u8>,
    server_fiscal_no: &str,
) -> Result<(), ConfirmError> {
    let kvt1_raw_sha256_hex = format!("{:x}", Sha256::digest(&kvt1_raw_bytes));
    let payload = serde_json::json!({
        "document_id": id_hex,
        "from_state": DocState::Kvt1.as_str(),
        "to_state": DocState::Kvt2.as_str(),
        "server_fiscal_no": server_fiscal_no,
        "dispatch_via": "kvt2_confirm",
        "evidence_source": "lastChk",
        "kvt1_raw_sha256_hex": kvt1_raw_sha256_hex,
        // attempt_no intentionally absent — Kvt1-entry has no fresh wire
        // attempt counter.
    });
    let payload_owned = payload.to_string();
    let id_hex_owned = id_hex.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            // (a) Persist Kvt1Raw evidence (overwrite prior-tick bytes if
            // any; INSERT OR REPLACE).
            document_files::replace_tx(
                tx,
                doc_id,
                document_files::DocumentFileKind::Kvt1Raw,
                &kvt1_raw_bytes,
            )
            .await?;
            // (b) CAS Kvt1 → Kvt2.  No Sent → Kvt1 — doc was at Kvt1 already.
            let kvt1_to_kvt2 =
                fiscal_documents::transition_state(tx, doc_id, DocState::Kvt1, DocState::Kvt2)
                    .await?;
            if kvt1_to_kvt2 != TransitionOutcome::Applied {
                return Err(anyhow::anyhow!(
                    "advance_to_ack: Envelope 1b CAS Kvt1→Kvt2 produced \
                     {outcome:?} for doc {doc_hex} (concurrent writer OR doc \
                     state drifted off Kvt1 between entry and Envelope 1b)",
                    outcome = kvt1_to_kvt2,
                    doc_hex = id_hex_owned,
                ));
            }
            // (c) Reset consecutive_holds — advance clears prior Hold
            // accumulation (REC-1 Tier reset semantics).
            fiscal_documents::reset_consecutive_holds_tx(tx, doc_id).await?;
            // (d) Forensic audit row.
            audit_log::append_tx(
                tx,
                AUDIT_ENTITY_DOC,
                &id_hex_owned,
                "OFFLINE_DRAIN_KVT2_ADVANCED",
                Severity::Info,
                None,
                Some(&payload_owned),
            )
            .await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await
    .map_err(|source| ConfirmError::Database { source })?;
    Ok(())
}
