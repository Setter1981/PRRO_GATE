//! CS-3 gap 4a — the operator-completion orchestrator (design §3.4).
//!
//! The SOLE production caller of `delivery_reservation::complete_operator_pending`, and the sole
//! caller of the operator-only shift rollback edge `Opening -> Closed` (shifts §4.1 edge 16). Its
//! one production entry point is `admin::resolve_operator_pending` (the CLI
//! `prro admin resolve-operator-pending`); the sole-caller claim is machine-checked by
//! `tests/k3y_operator_completion_shift_projection.rs`, which drives that CLI seam and asserts the
//! projection + audit this module owns.
//!
//! **bd PRRO_GATE-k3y (2026-08-01):** this paragraph previously asserted that role while the admin
//! CLI called `complete_operator_pending` DIRECTLY, bypassing everything below. The claim read as
//! authoritative and was false; the bypass silently dropped the shift projection and the Critical
//! audit, and a later boot orphaned the DPS-accepted shift. If this module is ever again reachable
//! only from tests, the named test goes RED — the claim is no longer maintained by hand.
//!
//! It resolves a PENDING reservation held under STOP_MODE (a SubmittedUnknown / crashed send) with
//! a typed operator resolution, in the caller's ONE `BEGIN IMMEDIATE` (the admin path runs the
//! `status_rro` probe OUTSIDE the tx first — invariant #1):
//!
//!   1. for an ONLINE shift-family document (SHIFT_OPEN / SHIFT_CLOSE / Z_REPORT) it applies the
//!      shift-state projection via [`apply_shift_transition`] — the sole `node_state.shift_state`
//!      writer (a repository must NOT invert into the service layer). Accepted commits the forward
//!      edge (`Opening -> Opened` / `Closing -> Closed`); not-accepted commits the rollback
//!      (`Opening -> Closed` for a never-opened shift; `Closing -> Opened` for a not-closed one);
//!   2. it runs the repository core [`complete_operator_pending`] (document `Sending -> Sent`/`RMR`,
//!      SFN, origin-split seed, `APPLIED`, pointer clear, verified mode target);
//!   3. it appends a Critical audit row with the resolution + result.
//!
//! All of that is ONE transaction — a shift-projection or core failure rolls the whole thing back.
//!
//! Scope (gap 4a): ONLINE shift-family + regular documents. An OFFLINE-origin not-accepted needs
//! the OLPD/CLPD rollback + OLA cohort cleanup + predecessor-seed repair, which the core now
//! performs itself (`NotAcceptedOffline`, bd PRRO_GATE-2nk) or refuses via its fork guards
//! (`LaterSuccessorIssued` / `LaterSuccessorInFlight` / `LaterSuccessorInvalidState`); an
//! origin-mismatched resolution is refused with `OriginMismatch`. Every one of those is surfaced
//! here unchanged. (The names `ShiftFamilyNotSupported` / `OfflineCohortCleanupRequired` this
//! paragraph used to cite are gone from `CompletionError` — corrected under bd PRRO_GATE-k3y.)

use crate::db::models::enums::{DocType, Severity, ShiftState};
use crate::db::models::ids::DocumentId;
use crate::db::repositories::delivery_reservation::{
    complete_operator_pending, CompletionResult, OperatorResolution, ReservationId,
};
use crate::db::repositories::shifts::TransitionOutcome;
use crate::db::repositories::{audit_log, delivery_reservation};
use crate::db::tx::WriteTxConn;
use crate::db::types::{DbDocType, DbDocumentId, DbShiftId};
use crate::services::shift::transition::apply_shift_transition;

/// Why [`complete_operator_resolution`] could not complete (nothing is mutated on any of these —
/// the whole `BEGIN IMMEDIATE` rolls back).
#[derive(Debug, thiserror::Error)]
pub enum OperatorCompletionError {
    /// No reservation row for the id.
    #[error("reservation not found for operator completion")]
    ReservationNotFound,
    /// The reservation's immutable document row is gone.
    #[error("document row missing")]
    DocumentMissing,
    /// A shift-family document carries no `shift_id` (structural breach).
    #[error("shift-family document has no shift_id")]
    ShiftMissing,
    /// A `MacReseed` resolution on a shift-family document — the §3.4 matrix has no such cell
    /// (a `-12` chain-seed correction is not defined for SHIFT_OPEN / SHIFT_CLOSE / Z_REPORT).
    #[error("MacReseed is not a defined shift-family resolution")]
    MacReseedOnShiftFamily,
    /// The shift was not in the expected state for the resolution (the `apply_shift_transition`
    /// CAS produced a non-`Applied` outcome — a stale local projection).
    #[error("shift-state projection failed: {0:?}")]
    ShiftProjectionFailed(TransitionOutcome),
    /// The repository core refused (stale authority / hold / offline gap-4b boundary / …).
    #[error(transparent)]
    Completion(#[from] delivery_reservation::CompletionError),
    /// A lower-level SQLite error (incl. a shift transition / mirror structural failure).
    #[error("db: {0}")]
    Db(String),
}

/// Complete a PENDING operator resolution (CS-3 gap 4a, design §3.4). See the module docs.
///
/// The `resolution` is trusted administrative input (the admin facade validated the out-of-tx
/// `status_rro` snapshot + `open_shift` agreement before opening this tx).
pub async fn complete_operator_resolution(
    tx: &mut WriteTxConn<'_>,
    reservation_id: ReservationId,
    resolution: OperatorResolution,
) -> Result<CompletionResult, OperatorCompletionError> {
    // 1. The reservation's document + FN (the core re-reads the reservation; this is for the
    //    shift-projection decision, which the core intentionally does NOT make).
    let res: Option<(Vec<u8>, String)> = sqlx::query_as(
        "SELECT document_id, fiscal_number FROM delivery_reservation WHERE reservation_id = ?",
    )
    .bind(&reservation_id[..])
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| OperatorCompletionError::Db(e.to_string()))?;
    let (doc_id_blob, fiscal_number) = res.ok_or(OperatorCompletionError::ReservationNotFound)?;
    let doc_id_arr: [u8; 16] = doc_id_blob
        .as_slice()
        .try_into()
        .map_err(|_| OperatorCompletionError::Db("document_id != 16 bytes".into()))?;
    let doc_id = DocumentId::from_bytes(doc_id_arr);

    // 2. The document family + origin + shift.
    let doc: Option<(DbDocType, Option<DbShiftId>, Option<i64>)> = sqlx::query_as(
        "SELECT doc_type, shift_id, offline_fiscal_no FROM fiscal_documents WHERE document_id = ?",
    )
    .bind(DbDocumentId(doc_id))
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| OperatorCompletionError::Db(e.to_string()))?;
    let (doc_type, shift_id, offline_fiscal_no) =
        doc.ok_or(OperatorCompletionError::DocumentMissing)?;
    let doc_type = doc_type.0;
    let online = offline_fiscal_no.is_none();

    // 3. Shift-state projection (design §3.4): for a shift-family document, apply the shift edge
    //    FIRST in the same tx. The operator-only rollback edges 16/17/18 are reachable ONLY here
    //    (pinned by the sole-caller test). ONLINE not-accepted rolls Opening->Closed / Closing->
    //    Opened; OFFLINE not-accepted rolls OLPD->Closed / CLPD->OLPD. OFFLINE **Accepted** gets
    //    NO operator shift edge — the drain finalize owns OLPD->Opened / CLPD->Closed
    //    (`backlog_drain.rs`); `shift_projection` returns None so the shift stays pending-drain.
    if let Some((from, to)) = shift_projection(doc_type, &resolution, online)? {
        let shift_id = shift_id.ok_or(OperatorCompletionError::ShiftMissing)?.0;
        let outcome = apply_shift_transition(tx, &fiscal_number, shift_id, from, to)
            .await
            .map_err(|e| OperatorCompletionError::Db(e.to_string()))?;
        if outcome != TransitionOutcome::Applied {
            return Err(OperatorCompletionError::ShiftProjectionFailed(outcome));
        }
    }

    // 4. Repository core: document / SFN / seed / APPLIED / pointer / verified mode.
    let result = complete_operator_pending(tx, reservation_id, resolution.clone()).await?;

    // 5. Critical audit (durable operator evidence, design §3.4). P4 — itemise the cancelled OLA
    //    cohort (document_id + lnd) and the rewound predecessor seed, so the record proves exactly
    //    which locally-issued documents were cancelled and where the chain was rewound.
    let entity = hex_lower(&reservation_id);
    let cancelled: Vec<String> = result
        .cancelled_cohort
        .iter()
        .map(|(id, lnd)| {
            format!(
                "{{\"doc\":\"{}\",\"lnd\":{}}}",
                hex_lower(id.as_bytes()),
                lnd
            )
        })
        .collect();
    let rewound_seed = match &result.rewound_predecessor_seed {
        None => "null".to_string(),
        Some(None) => "\"genesis\"".to_string(),
        Some(Some(h)) => format!("\"{}\"", hex_lower(h)),
    };
    let payload = format!(
        "{{\"resolution\":\"{}\",\"applied\":{},\"seed_advanced\":{},\"mode_target\":\"{:?}\",\
         \"cancelled_cohort\":[{}],\"rewound_seed\":{}}}",
        resolution_tag(&resolution),
        result.applied,
        result.seed_advanced,
        result.mode_target,
        cancelled.join(","),
        rewound_seed,
    );
    audit_log::append_tx(
        tx,
        "delivery_reservation",
        &entity,
        "OPERATOR_COMPLETION",
        Severity::Critical,
        None,
        Some(&payload),
    )
    .await
    .map_err(|e| OperatorCompletionError::Db(e.to_string()))?;

    Ok(result)
}

/// The shift-state edge for a shift-family document + resolution + DB-derived origin (design §3.4).
/// Returns `None` for a non-shift-family document, for an OFFLINE **Accepted** shift-family doc
/// (the drain finalize owns the forward edge), and for an origin-mismatched resolution (the core's
/// origin cross-check is the authority — it fails those closed). `MacReseed` on any shift-family
/// document has no defined cell → `MacReseedOnShiftFamily`.
fn shift_projection(
    doc_type: DocType,
    resolution: &OperatorResolution,
    online: bool,
) -> Result<Option<(ShiftState, ShiftState)>, OperatorCompletionError> {
    use DocType::{ShiftClose, ShiftOpen, ZReport};
    use OperatorResolution as R;
    use ShiftState::*;
    let edge = match (doc_type, online, resolution) {
        // ── online shift-family (gap 4a) ──
        (ShiftOpen, true, R::Accepted { .. }) => (Opening, Opened),
        (ShiftOpen, true, R::NotAccepted) => (Opening, Closed),
        (ShiftClose | ZReport, true, R::Accepted { .. }) => (Closing, Closed),
        (ShiftClose | ZReport, true, R::NotAccepted) => (Closing, Opened),
        // ── offline shift-family NOT-accepted rollback (gap 4b) ──
        (ShiftOpen, false, R::NotAcceptedOffline) => (OpenedLocalPendingDrain, Closed),
        (ShiftClose | ZReport, false, R::NotAcceptedOffline) => {
            (ClosingLocalPendingDrain, OpenedLocalPendingDrain)
        }
        // offline shift-family Accepted: NO operator shift edge (drain finalize owns edge 5/13).
        (ShiftOpen | ShiftClose | ZReport, false, R::Accepted { .. }) => return Ok(None),
        // MacReseed on any shift-family document has no §3.4 cell.
        (ShiftOpen | ShiftClose | ZReport, _, R::MacReseed { .. }) => {
            return Err(OperatorCompletionError::MacReseedOnShiftFamily)
        }
        // Non-shift-family, or an origin-mismatched resolution — no shift edge; the core's origin
        // cross-check fails the mismatch closed.
        _ => return Ok(None),
    };
    Ok(Some(edge))
}

fn resolution_tag(r: &OperatorResolution) -> &'static str {
    match r {
        OperatorResolution::Accepted { .. } => "Accepted",
        OperatorResolution::NotAccepted => "NotAccepted",
        OperatorResolution::NotAcceptedOffline => "NotAcceptedOffline",
        OperatorResolution::MacReseed { .. } => "MacReseed",
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
