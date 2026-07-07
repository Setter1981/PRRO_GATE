//! RS-3 A1Z — the Z-class (`Z_REPORT` / `SHIFT_CLOSE`) write-path units.
//!
//! Unlike the pure non-Z [`build_canonical`](super::canonical_builder::build_canonical),
//! the Z path is ledger-touching + has a pre-aggregation QUIESCENCE step.
//! A1Z ships three units (the future A2 dispatcher orchestrates them):
//!   - [`super::convert::aggregate_z_payload`] — read the shift ledger →
//!     aggregated `ZReportJson` (done in part 1);
//!   - [`quiesce_shift_before_z`] — finalize inline-finalizable in-flight
//!     receipts so the aggregate is complete, or refuse;
//!   - [`build_z_canonical`] — build the D5 DUAL-HASH command from the inbox
//!     row + the aggregated payload.
//!
//! QUIESCENCE IS DELIBERATELY NARROW (operator-locked) — it is NOT a drain /
//! reaper / dispatcher. It finalizes ONLY `KVT2` docs (via
//! [`stage_finalize::run`], a short DB tx with NO network/crypto), then
//! REFUSES to aggregate while ANY other in-flight receipt remains. `SENT` /
//! `KVT1` need the runtime KVT2-confirm / lastChk path; `SENDING` must NOT be
//! re-pushed (duplicate risk) — those surface as `Pending`, never finalized
//! here. NO drain / B1 / DPS / send / sign / crypto runs inside quiescence,
//! so invariant #1 holds and the A1Z boundary stays sharp.

use crate::db::models::enums::{DocState, DocType};
use crate::db::models::ids::{CashierId, DocumentId, DriverId, ShiftId};
use crate::db::repositories::fiscal_documents;
use crate::db::repositories::ingress_inbox::InboxRow;
use crate::runtime::ingress::canonical_builder::BuildReject;
use crate::runtime::ingress::convert::ConvertedPayload;
use crate::services::write_path::stage_finalize;
use crate::services::write_path::types::CanonicalFiscalCommand;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

/// RS-3 A2 RELEASE GATE (wide-audit HIGH / AUDIT5-CRIT-2).  The Z surface is now
/// COMPLETE for this gateway's contour, so live Z dispatch is enabled (PR-Z2):
///   - **TXS** (tax-group turnover) landed in PR-Z1 — `derive_z_report_tax_summaries`
///     aggregates the shift's stored items into canonical tax groups.
///   - **IO** (service in/out) + **EPZ** (card-acquiring) are LEGITIMATELY ABSENT
///     for this contour, NOT under-reported: the gateway structurally cannot
///     accept the operations those sections report — `SERVICE_IN`/`SERVICE_OUT`
///     are classified `Unsupported` at ingress (`policy.rs` → HTTP 422, never
///     minted) and a slip-carrying payment fail-closes at `convert.rs`
///     (`AcquirerSlipMappingDeferred`, 422).  A 4-year Python prod emitted both
///     sections only on NON-EMPTY data → absent == accurate here.
///
/// ⚠️ COUPLING (STOP-S2): this completeness is CONDITIONAL on those two ingress
/// guards.  Enabling EITHER `SERVICE_IN/OUT` ingest OR `acquirer_slip` pass-through
/// WITHOUT building its Z-half (the IO / EPZ aggregation) re-opens the
/// under-reporting hazard — a live close-shift would then sign a Z that OMITS
/// real service / card turnover.  If either guard is relaxed, flip this back to
/// `false` OR extend the surface IN THE SAME CHANGE.  The coupling-pin
/// `z_surface_flip_is_coupled_to_ingress_guards` asserts both guards still hold;
/// it MUST break loudly if someone drops one without the IO/EPZ half.  The
/// tripwire `z_live_dispatch_is_gated_until_full_z_surface` pins this flip as the
/// deliberate release decision it is.
pub const FULL_Z_SURFACE_READY: bool = true;

/// Typed fail-closed error for a live Z dispatch attempted before the full Z
/// surface (TXS / IO / EPZ) exists. See [`FULL_Z_SURFACE_READY`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZSurfaceNotReady;

impl std::fmt::Display for ZSurfaceNotReady {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "live Z dispatch refused: the full Z report surface (TXS/IO/EPZ) is not yet \
             implemented (W4-Z2); the aggregate is summary-only"
        )
    }
}

/// A2 release gate: `Ok(())` once the full Z surface exists, else
/// `Err(ZSurfaceNotReady)`. The A2 dispatcher MUST call this before driving a
/// live Z_REPORT / SHIFT_CLOSE through aggregate/sign.
pub fn ensure_full_z_surface_ready() -> Result<(), ZSurfaceNotReady> {
    if FULL_Z_SURFACE_READY {
        Ok(())
    } else {
        Err(ZSurfaceNotReady)
    }
}

/// Outcome of the pre-aggregation quiescence pass for a shift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuiescenceOutcome {
    /// No blocking in-flight receipt remains — the ledger is complete and the
    /// Z may aggregate.
    Clear,
    /// In-flight receipts remain that A1Z must NOT finalize itself (they need
    /// the runtime KVT2-confirm / send path, or must not be re-pushed). The
    /// caller returns IN_PROGRESS / `Z_QUIESCENCE_PENDING` WITHOUT creating a
    /// Z doc; the Z retries once the runtime drives them issued.
    Pending {
        blocking: Vec<(DocumentId, DocState)>,
    },
}

/// Quiesce a shift before its Z aggregates: finalize the ONLY
/// inline-finalizable in-flight state (`KVT2`, via [`stage_finalize::run`] —
/// short DB tx, no IO), then re-read. If any blocking in-flight receipt
/// remains, return [`QuiescenceOutcome::Pending`] (the caller must NOT
/// aggregate / create the Z doc).
///
/// Pending receipts are read in MAC-chain order (`ORDER BY lnd`) so the KVT2
/// finalize pass respects the cross-doc hash chain. MUST run inside the
/// caller's ALREADY-HELD per-FN fiscalize gate (A4) — it does its own SHORT
/// transactions and MUST NOT call the drain loop / B1 reaper (which would
/// re-acquire the same gate → deadlock).
pub async fn quiesce_shift_before_z(
    pool: &SqlitePool,
    fiscal_number: &str,
    shift_id: ShiftId,
) -> anyhow::Result<QuiescenceOutcome> {
    // Pass 1 — inline-finalize ONLY the LEADING CONTIGUOUS run of KVT2 docs
    // (in lnd order). The cross-doc MAC chain advances strictly in lnd order:
    // a KVT2 doc can only finalize once every PRIOR doc is Acked (stage_finalize
    // chain-seed guard). So the first NON-KVT2 in-flight receipt blocks the
    // chain — every later receipt is chain-seed-blocked behind it. Stop there
    // (do NOT attempt to finalize a KVT2 sitting behind an earlier blocker —
    // that would turn a recoverable "Z not ready yet" into a hard error); the
    // refetch below then reports Pending.
    let pending = fiscal_documents::list_shift_pending_receipts_for_z_quiescence(
        pool,
        fiscal_number,
        shift_id,
    )
    .await?;
    for (doc_id, state) in &pending {
        if *state != DocState::Kvt2 {
            break;
        }
        // stage_finalize::run owns its own `with_immediate`; Kvt2 → Ack +
        // bookkeeping, no network/crypto. A non-`Acked` Ok-outcome (e.g. a
        // raced StateConflict) is caught by the refetch below; only a true
        // Err propagates (rolls back that doc's finalize tx).
        stage_finalize::run(pool, *doc_id)
            .await
            .map_err(|e| anyhow::anyhow!("z-quiescence finalize {doc_id:?}: {e}"))?;
    }
    // Pass 2 — re-read. Anything still in-flight blocks aggregation.
    let still = fiscal_documents::list_shift_pending_receipts_for_z_quiescence(
        pool,
        fiscal_number,
        shift_id,
    )
    .await?;
    if still.is_empty() {
        Ok(QuiescenceOutcome::Clear)
    } else {
        Ok(QuiescenceOutcome::Pending { blocking: still })
    }
}

/// Build the Z-class [`CanonicalFiscalCommand`] from the inbox row + the
/// aggregated payload ([`super::convert::aggregate_z_payload`]).
///
/// D5 DUAL-HASH:
///   - `source_sha256` = the inbox WIRE-intent hash (verified == a fresh
///     sha256 over the wire `payload_json`, same integrity gate as non-Z);
///   - `payload_json` = the AGGREGATED `ZReportJson` body;
///   - `payload_sha256_canonical` = `hash(aggregated)` (from the aggregation).
///
/// The two diverge for Z (that is the whole point of D5). The same
/// recovery-identity null-contract as non-Z applies (driver_id required,
/// business_ts required + trimmed; cashier optional). Z carries no wire total.
pub fn build_z_canonical(
    row: &InboxRow,
    aggregated: &ConvertedPayload,
) -> Result<CanonicalFiscalCommand, BuildReject> {
    let doc_type = match row.operation_type.as_str() {
        "Z_REPORT" => DocType::ZReport,
        "SHIFT_CLOSE" => DocType::ShiftClose,
        other => {
            return Err(BuildReject::UnsupportedOperation {
                operation_type: other.to_string(),
            })
        }
    };

    // Source-integrity gate: the WIRE payload must match the WIRE hash (the
    // inbox carries both).
    let computed_source: [u8; 32] = Sha256::digest(row.payload_json.as_bytes()).into();
    if computed_source != row.payload_sha256_canonical {
        return Err(BuildReject::PayloadHashMismatch);
    }

    // Aggregated-body integrity gate (review MEDIUM): the builder is the last
    // PURE boundary before stage 1 persists the document hash, so do NOT trust
    // the caller's `aggregated.payload_sha256_canonical` — recompute it over
    // `aggregated.payload_json` and reject a mismatch, so a bad / stale
    // ConvertedPayload can never persist payload_json=A with hash=B.
    let computed_aggregated: [u8; 32] = Sha256::digest(aggregated.payload_json.as_bytes()).into();
    if computed_aggregated != aggregated.payload_sha256_canonical {
        return Err(BuildReject::PayloadHashMismatch);
    }

    let driver_raw = row
        .driver_id
        .as_deref()
        .ok_or(BuildReject::MissingDriverId)?;
    let driver_id = DriverId::new(driver_raw)
        .map_err(|_| BuildReject::InvalidIdentity { field: "driver_id" })?;

    let business_ts = match row.business_ts.as_deref().map(str::trim) {
        Some(ts) if !ts.is_empty() => ts.to_string(),
        _ => return Err(BuildReject::MissingBusinessTs),
    };

    let signed_by_cashier_id = row
        .signed_by_cashier_id
        .as_deref()
        .map(CashierId::new)
        .transpose()
        .map_err(|_| BuildReject::InvalidIdentity {
            field: "signed_by_cashier_id",
        })?;

    Ok(CanonicalFiscalCommand {
        doc_type,
        business_ts,
        // Z closes a shift — no wire-declared total (the aggregate carries the
        // computed turnover in its payload).
        total_sum_kop: None,
        payload_json: aggregated.payload_json.clone(),
        // D5: canonical = the RECOMPUTED hash of the AGGREGATED body (verified
        // == the caller's above); source = wire hash.
        payload_sha256_canonical: computed_aggregated,
        source_sha256: row.payload_sha256_canonical,
        signed_by_cashier_id,
        driver_id: Some(driver_id),
    })
}
