//! W14a-2b §2.3 — sign-time cashier enforcement helper.
//!
//! Per spec `docs/superpowers/specs/2026-05-19-w14a-2b-signer-channel.md`
//! §1.4 + §16.8.  Pure-function helper called from `stage_send` 4-pre tx
//! (Commit 5) BEFORE envelope build / CAS / trace / audit / wire send.
//!
//! **Pre-wire refusal contract.**  The helper produces a typed
//! [`SignerCashierMismatch`] that the caller surfaces as
//! `Ok(PreOutcome::SignerRefused(_))` so the surrounding
//! `with_immediate` tx commits — preserving the audit row per spec §8 +
//! W14a-2a Round 7 §8.1 forensic-traceability convention.  This module
//! does NOT emit the audit itself (audit emit lands in `stage_send::run`
//! inside the same 4-pre tx — Commit 5).
//!
//! **Bypass set.**  Three doc types bypass this helper:
//!
//! - `DocType::ShiftClose` / `DocType::ZReport` — per spec §16.9, those
//!   may be signed by senior cashier via W14a-2a's
//!   `senior_cashier_close_shift_with_audit` seam, which has its own
//!   runtime validation against `cashier_certs(cashier_id,
//!   fiscal_number)`.  This helper trusts that layer.
//!
//! - `DocType::ShiftOpen` — at stage_send time for SHIFT_OPEN there is
//!   no shift row to compare against (stage_acquire allows ShiftOpen
//!   only from `ShiftState::Closed`, resolves `active_shift = None`,
//!   inserts the doc with `shift_id = NULL`).  The shift row is
//!   created during stage_finalize after DPS Ack — the signer's
//!   `signed_by_cashier_id` BECOMES `shifts.opened_by_cashier_id` by
//!   construction.  Validating before creation is semantically empty;
//!   bypassing here keeps the helper consistent with the actual
//!   stage_acquire surface (MED-C3-2 resolution, 2026-05-19).
//!
//! **Drain-time consumer (W9b).**  The helper signature is shaped so
//! W9b's offline backlog drain can invoke it on `SendInputs` loaded
//! from a persisted `OFFLINE_LOCAL_ACK` doc + `ShiftRow` looked up by
//! the doc's `shift_id`.  No state mutation on refusal — caller
//! decides whether to advance the doc to `Cancelled` / leave it
//! pending / escalate.

use crate::db::models::enums::DocType;
use crate::db::models::ids::{CashierId, DocumentId, ShiftId};
use crate::db::repositories::fiscal_documents::SendInputs;
use crate::db::repositories::shifts::ShiftRow;

/// W14a-2b §2.4 — sign-time cashier-mismatch refusal taxonomy.
///
/// All variants surface as `Ok(PreOutcome::SignerRefused(_))` from the
/// stage_send 4-pre tx (Commit 5).  Caller emits the
/// `SIGNER_CASHIER_MISMATCH` Warning audit + returns to the dispatcher
/// without invoking wire send.
///
/// `#[non_exhaustive]` per W14a-2a outcome-enum convention — supports
/// future variants (e.g. shift-state-specific refusal classes) without
/// breaking downstream pattern-match callers in audit / metrics layers.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SignerCashierMismatch {
    /// `SendInputs.signed_by_cashier_id` is `None`.  The ingress adapter
    /// (or boot-snapshot reconstruction) failed to attribute the signer
    /// at PREPARED insert time.  Caller bug: ingress MUST populate this
    /// field for fiscal docs that require signer enforcement.
    ///
    /// Distinguished from [`Self::ShiftMissingForFiscalDoc`] because the
    /// remediation is different (fix ingress vs fix shift resolution).
    #[error("stage send: signer id missing for document {document_id:?}")]
    SignerIdMissing { document_id: DocumentId },

    /// Non-close fiscal doc (Sell / Return / ServiceIn / ServiceOut /
    /// CashWithdrawal / XReport) has no resolvable shift_id on its
    /// `SendInputs`.  Structural caller bug: `stage_acquire`'s shift-
    /// invariant guard should have refused this doc at acquire time.
    /// Surfaced as Ok refusal so the audit row commits — the operator
    /// gets forensic visibility into the dispatcher mistake.
    #[error("stage send: non-close fiscal doc {document_id:?} of type {doc_type:?} has no resolvable shift_id")]
    ShiftMissingForFiscalDoc {
        document_id: DocumentId,
        doc_type: DocType,
    },

    /// `SendInputs.shift_id != Some(shift.shift_id)`.  Structural caller
    /// bug: someone loaded a `ShiftRow` that is NOT the document's
    /// persisted shift binding.  Defends against the case where a
    /// future caller (W9b drain time) loads "any same-FN shift" instead
    /// of the doc's `shift_id` from `fiscal_documents` — cashier-id
    /// equality would otherwise pass tautologically if the FN happens
    /// to have a single active cashier.
    ///
    /// Covers both:
    /// - `inputs.shift_id == None` (non-bypass doc has no persisted
    ///   shift binding); and
    /// - `inputs.shift_id == Some(X)` but `shift.shift_id == Y` with
    ///   `X != Y` (wrong shift row supplied).
    ///
    /// MED-C3-1 hardening (operator-flagged 2026-05-19) — locks the
    /// invariant that signer enforcement always validates AGAINST THE
    /// DOCUMENT'S OWN SHIFT, never a sibling shift.
    #[error(
        "stage send: shift_id mismatch for document {document_id:?} of type \
         {doc_type:?} — expected {expected_shift_id:?}, supplied \
         {supplied_shift_id:?}"
    )]
    ShiftIdMismatch {
        document_id: DocumentId,
        doc_type: DocType,
        expected_shift_id: Option<ShiftId>,
        supplied_shift_id: ShiftId,
    },

    /// `SendInputs.fiscal_number != ShiftRow.fiscal_number`.  Structural
    /// caller bug: someone loaded a `SendInputs` and a `ShiftRow` that
    /// belong to different FNs and tried to validate them as a pair.
    /// Distinct from `Mismatch` because cashier-id coincidence across
    /// FNs (e.g. same operator name registered under both FNs) would
    /// otherwise let the equality check pass while the binding is
    /// semantically wrong.
    ///
    /// Currently unreachable through stage_acquire's active-shift
    /// resolution (single-FN binding), but the helper enforces it as
    /// defence-in-depth so W9b's drain-time consumer (loads both
    /// `SendInputs` and `ShiftRow` independently from persisted ledger
    /// rows) cannot silently produce a cross-FN attribution.
    ///
    /// NIT-C3-2 hardening (operator-approved 2026-05-19) — adds checked
    /// boundary before the cashier-id equality arm.
    #[error(
        "stage send: cross-FN binding for document {document_id:?} — inputs FN \
         {inputs_fiscal_number}, shift FN {shift_fiscal_number}"
    )]
    CrossFnMismatch {
        document_id: DocumentId,
        inputs_fiscal_number: String,
        shift_fiscal_number: String,
    },

    /// `SendInputs.signed_by_cashier_id != ShiftRow.opened_by_cashier_id`.
    /// Operator / UI attribution error OR signer-pipeline misconfiguration.
    /// Surfaced as Ok refusal + Warning audit.  Retry without fixing the
    /// signer id will re-trigger the same refusal — operator must reissue
    /// with the correct cashier.
    #[error(
        "stage send: signer {attempted_signer_id} does not match opening cashier \
         {expected_cashier_id} on shift {shift_id:?} for document {document_id:?} \
         of type {doc_type:?}"
    )]
    Mismatch {
        shift_id: ShiftId,
        document_id: DocumentId,
        expected_cashier_id: CashierId,
        attempted_signer_id: CashierId,
        doc_type: DocType,
    },
}

/// W14a-2b §2.3 — sign-time signer-cashier check.
///
/// Returns `Ok(())` when:
/// - `inputs.doc_type` is in the bypass set
///   (`ShiftClose` / `ZReport` / `ShiftOpen`), OR
/// - `inputs.shift_id == Some(shift.shift_id)` AND
///   `inputs.fiscal_number == shift.fiscal_number` AND
///   `inputs.signed_by_cashier_id == Some(shift.opened_by_cashier_id)`.
///
/// Returns `Err(SignerCashierMismatch::*)` for the 5 refusal classes
/// (decision order — forensic precedence):
/// 1. `ShiftMissingForFiscalDoc` — `shift` arg is `None`, OR
///    `inputs.shift_id` is `None`.  Non-close doc has no resolvable
///    shift binding for signer validation.
/// 2. `ShiftIdMismatch` — `inputs.shift_id` is `Some(X)` and
///    `shift.shift_id == Y` with `X != Y` (caller loaded the wrong
///    shift row).
/// 3. `CrossFnMismatch` — `inputs.fiscal_number != shift.fiscal_number`.
///    Defence-in-depth: protects W9b drain-time consumer from cross-FN
///    attribution coincidence.
/// 4. `SignerIdMissing` — `inputs.signed_by_cashier_id` is `None`.
/// 5. `Mismatch` — signer id differs from the shift's opening cashier.
///
/// Order rationale: structural caller bugs (wrong shift / cross-FN)
/// surface BEFORE attribution bugs (missing signer / wrong cashier);
/// upstream issues report first so the operator sees the root cause
/// rather than a derived symptom.
///
/// **Pure function** — no DB I/O, no audit emit.  Caller wraps the
/// result into `PreOutcome::SignerRefused(_)` and emits the audit row
/// inside its own `with_immediate` tx (Commit 5 stage_send wiring).
pub fn enforce_signer_cashier_match(
    inputs: &SendInputs,
    shift: Option<&ShiftRow>,
) -> Result<(), SignerCashierMismatch> {
    // Bypass set — see module-level docstring for full rationale.
    // ShiftClose / ZReport: §16.9 senior-cashier seam owns validation.
    // ShiftOpen: no shift row exists yet at stage_send time; signer
    //   becomes opened_by_cashier_id post-finalize by construction
    //   (MED-C3-2 resolution).
    if matches!(
        inputs.doc_type,
        DocType::ShiftClose | DocType::ZReport | DocType::ShiftOpen,
    ) {
        return Ok(());
    }

    // (1a) Non-close fiscal docs MUST have a resolvable shift row.
    let shift = shift.ok_or(SignerCashierMismatch::ShiftMissingForFiscalDoc {
        document_id: inputs.document_id,
        doc_type: inputs.doc_type,
    })?;

    // (1b) Non-close fiscal docs MUST have a persisted shift_id on
    // SendInputs.  Folded into ShiftMissingForFiscalDoc — semantically:
    // no resolvable binding for signer validation.  MED-C3-1 invariant.
    let inputs_shift_id = inputs
        .shift_id
        .ok_or(SignerCashierMismatch::ShiftMissingForFiscalDoc {
            document_id: inputs.document_id,
            doc_type: inputs.doc_type,
        })?;

    // (2) Shift row MUST be the document's own shift, NOT a sibling
    // same-FN shift.  MED-C3-1: locks the invariant that signer
    // enforcement always validates against the document's persisted
    // shift_id.  Without this check, a future caller could pass
    // "any open shift on this FN" and pass equality tautologically
    // if the cashier happens to match.
    if inputs_shift_id != shift.shift_id {
        return Err(SignerCashierMismatch::ShiftIdMismatch {
            document_id: inputs.document_id,
            doc_type: inputs.doc_type,
            expected_shift_id: Some(inputs_shift_id),
            supplied_shift_id: shift.shift_id,
        });
    }

    // (3) Cross-FN defence-in-depth.  Currently unreachable through
    // stage_acquire (single-FN binding via active-shift resolution),
    // but W9b drain time loads SendInputs + ShiftRow independently
    // from persisted ledger rows — prevents silent cross-FN attribution
    // via same-cashier-id coincidence.
    if inputs.fiscal_number != shift.fiscal_number {
        return Err(SignerCashierMismatch::CrossFnMismatch {
            document_id: inputs.document_id,
            inputs_fiscal_number: inputs.fiscal_number.clone(),
            shift_fiscal_number: shift.fiscal_number.clone(),
        });
    }

    // (4) Signer attribution required for non-bypass fiscal docs.
    let attempted = inputs
        .signed_by_cashier_id
        .as_ref()
        .ok_or(SignerCashierMismatch::SignerIdMissing {
            document_id: inputs.document_id,
        })?;

    // (5) Equality check via `&str` to avoid double-clone'ing CashierId.
    if attempted.as_str() != shift.opened_by_cashier_id.as_str() {
        return Err(SignerCashierMismatch::Mismatch {
            shift_id: shift.shift_id,
            document_id: inputs.document_id,
            expected_cashier_id: shift.opened_by_cashier_id.clone(),
            attempted_signer_id: attempted.clone(),
            doc_type: inputs.doc_type,
        });
    }

    Ok(())
}
