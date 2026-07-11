//! RS-3 A1 — `InboxRow` → `CanonicalFiscalCommand` builder (non-Z) + the
//! seam null-contract.
//!
//! The crash-recovery reaper (RS-3 B1) and the live write-path worker
//! (RS-3 A2) both reconstruct the canonical command FROM the persisted
//! `ingress_inbox` row — the row is the ONLY input the SEAM gets. This
//! module maps a non-Z row to a [`CanonicalFiscalCommand`] and enforces
//! the PROCESSING null-contract: the recovery-identity columns
//! (migrations 021/022) the write-path consumes MUST be present, else the
//! row is TERMINALIZED ([`terminalize_rejected_tx`]: status → REJECTED +
//! audit, NO `fiscal_documents` row) so the reaper never loops a
//! malformed / pre-migration legacy row forever.
//!
//! The Z-CLASS ops — `Z_REPORT` AND `SHIFT_CLOSE` (both aggregate the
//! shift's issued receipts into a ZReportJson; see `is_z_class` /
//! `convert` / `stage_sign::derive_wire_artifact_kind`) — are intentionally
//! NOT handled here: they need payload aggregation + a divergent dual-hash
//! (RS-3 A1Z). The dispatcher (A2) routes them to that builder; a Z-class
//! row reaching this non-Z builder is a dispatch bug, surfaced as a
//! diagnosable reject rather than a silent mis-build.

use crate::db::models::enums::{DocType, Severity};
use crate::db::models::ids::{CashierId, DriverId};
use crate::db::repositories::audit_log;
use crate::db::repositories::ingress_inbox::{self, InboxRow};
use crate::db::tx::WriteTxConn;
use crate::services::write_path::types::{hex_encode_lower as hex_lower, CanonicalFiscalCommand};
use sha2::{Digest, Sha256};

/// Why a PROCESSING inbox row cannot be turned into a fiscal command.
///
/// Every variant is TERMINAL: the caller `mark_rejected_tx`'s the row (no
/// `fiscal_documents` row) so the stale-PROCESSING reaper never re-drives
/// it. The reject is a ledger NON-event — it lands in `audit_log` only
/// (DB-vs-log separation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildReject {
    /// `driver_id` absent — a pre-021 legacy row, or a listener that
    /// failed to stamp it. RS-3 fails closed: the driver drives tax-group
    /// translation, so a doc built without it would mis-translate.
    MissingDriverId,
    /// `business_ts` absent — a pre-022 legacy row. Every fiscal command
    /// needs the receipt/document timestamp.
    MissingBusinessTs,
    /// `total_sum_kop` absent on a SELL / RETURN — the declared total the
    /// stage_sign sum cross-check needs.
    MissingTotalForSale { doc_type: DocType },
    /// `operation_type` is not a non-Z fiscal operation this builder maps
    /// (an unknown string, or a non-pilot op).
    UnsupportedOperation { operation_type: String },
    /// A Z-CLASS operation (`Z_REPORT` or `SHIFT_CLOSE` — both aggregate the
    /// shift's issued receipts into a ZReportJson, per `is_z_class` /
    /// `convert` / `stage_sign::derive_wire_artifact_kind`) reached the non-Z
    /// builder — a dispatcher bug. It needs the A1Z aggregation / dual-hash
    /// builder. Distinct from `UnsupportedOperation` so a misroute is
    /// diagnosable in the audit.
    ZClassRequiresAggregationBuilder { operation_type: String },
    /// The persisted `payload_sha256_canonical` does not match a fresh
    /// sha256 over `payload_json` — a corrupted / tampered row. Terminal:
    /// the row must not be fiscalized with a stale hash + altered payload.
    PayloadHashMismatch,
    /// A persisted identity column (`driver_id` / `signed_by_cashier_id`)
    /// is present but malformed (fails the id newtype's validation).
    InvalidIdentity { field: &'static str },
}

impl BuildReject {
    /// Stable machine code for the `audit_log` payload + the seam's HTTP
    /// error mapping.
    pub fn code(&self) -> &'static str {
        match self {
            BuildReject::MissingDriverId => "MISSING_DRIVER_ID",
            BuildReject::MissingBusinessTs => "MISSING_BUSINESS_TS",
            BuildReject::MissingTotalForSale { .. } => "MISSING_TOTAL_FOR_SALE",
            BuildReject::UnsupportedOperation { .. } => "UNSUPPORTED_OPERATION",
            BuildReject::ZClassRequiresAggregationBuilder { .. } => "Z_CLASS_REQUIRES_AGGREGATION",
            BuildReject::PayloadHashMismatch => "PAYLOAD_HASH_MISMATCH",
            BuildReject::InvalidIdentity { .. } => "INVALID_IDENTITY",
        }
    }
}

impl std::fmt::Display for BuildReject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildReject::MissingDriverId => write!(f, "driver_id is required (fail-closed)"),
            BuildReject::MissingBusinessTs => write!(f, "business_ts is required"),
            BuildReject::MissingTotalForSale { doc_type } => {
                write!(f, "total_sum_kop is required for {}", doc_type.as_str())
            }
            BuildReject::UnsupportedOperation { operation_type } => {
                write!(f, "unsupported operation_type `{operation_type}`")
            }
            BuildReject::ZClassRequiresAggregationBuilder { operation_type } => write!(
                f,
                "Z-class op `{operation_type}` must be built by the A1Z aggregation builder"
            ),
            BuildReject::PayloadHashMismatch => {
                write!(f, "payload_sha256_canonical does not match payload_json")
            }
            BuildReject::InvalidIdentity { field } => write!(f, "malformed {field}"),
        }
    }
}

/// Map the wire `operation_type` to a non-Z [`DocType`].
///
/// The Z-CLASS ops — `Z_REPORT` AND `SHIFT_CLOSE` — both aggregate the
/// shift's issued receipts into a ZReportJson (`is_z_class` in `handler`,
/// `convert::CommandType::ShiftClose | ZReport`, and
/// `stage_sign::derive_wire_artifact_kind` mapping `ShiftClose → ZReport`).
/// They are NOT mapped here; they route to A1Z via
/// [`BuildReject::ZClassRequiresAggregationBuilder`]. Any other unknown /
/// non-pilot op is [`BuildReject::UnsupportedOperation`].
fn map_non_z_doc_type(operation_type: &str) -> Result<DocType, BuildReject> {
    match operation_type {
        "SELL" => Ok(DocType::Sell),
        "RETURN" => Ok(DocType::Return),
        "SHIFT_OPEN" => Ok(DocType::ShiftOpen),
        // L3 — service cash-in/out are wired fiscal ops (Signable).
        "SERVICE_IN" => Ok(DocType::ServiceIn),
        "SERVICE_OUT" => Ok(DocType::ServiceOut),
        "Z_REPORT" | "SHIFT_CLOSE" => Err(BuildReject::ZClassRequiresAggregationBuilder {
            operation_type: operation_type.to_string(),
        }),
        other => Err(BuildReject::UnsupportedOperation {
            operation_type: other.to_string(),
        }),
    }
}

/// Pure mapping + null-guard — NO IO. Returns the canonical command, or a
/// TERMINAL reject reason the caller hands to [`terminalize_rejected_tx`].
///
/// Call ONLY for non-Z rows; the dispatcher routes the Z-CLASS ops
/// (`Z_REPORT` AND `SHIFT_CLOSE`) to the A1Z aggregation builder. For
/// non-Z, `source_sha256 == payload_sha256_canonical` (one payload, one
/// hash; per D5).
pub fn build_canonical(row: &InboxRow) -> Result<CanonicalFiscalCommand, BuildReject> {
    let doc_type = map_non_z_doc_type(&row.operation_type)?;

    // Row-integrity gate (plan §A1 + D5): the canonical hash MUST be a fresh
    // sha256 over `payload_json`, not the trusted persisted value — so a
    // corrupted row (payload altered, hash not) is rejected HERE rather than
    // sailing past the stage_acquire cross-check (which compares the
    // command's hash to the same persisted inbox hash). Reproduces the ingest
    // hashing exactly (`Sha256::digest` over the canonical JSON bytes;
    // payload_json IS those bytes — see convert.rs / dto.rs).
    let computed_payload_sha256: [u8; 32] = Sha256::digest(row.payload_json.as_bytes()).into();
    if computed_payload_sha256 != row.payload_sha256_canonical {
        return Err(BuildReject::PayloadHashMismatch);
    }

    // Null-contract: driver_id REQUIRED (fail-closed) for every row.
    let driver_raw = row
        .driver_id
        .as_deref()
        .ok_or(BuildReject::MissingDriverId)?;
    let driver_id = DriverId::new(driver_raw)
        .map_err(|_| BuildReject::InvalidIdentity { field: "driver_id" })?;

    // business_ts REQUIRED for every row — and NON-EMPTY: an empty /
    // whitespace timestamp is not a valid DPS document TS and would defeat
    // the early-terminalize design (it would surface far later, at sign).
    // Mirrors the persisted-row contract asserted at handler.rs (the seam
    // test requires `business_ts` non-empty).
    // Store the TRIMMED value (consistent with DriverId trim-at-construction)
    // so surrounding whitespace can't surface as a late RFC3339 parse failure
    // at sign — the early-terminalize design's whole point.
    let business_ts = match row.business_ts.as_deref().map(str::trim) {
        Some(ts) if !ts.is_empty() => ts.to_string(),
        _ => return Err(BuildReject::MissingBusinessTs),
    };

    // total_sum_kop REQUIRED for SELL / RETURN. SHIFT_OPEN / SHIFT_CLOSE
    // carry NO total per the inbox contract (`InboxRow` doc) — normalize
    // any stray persisted value to None so an anomalous row is canonicalized
    // to its documented shape rather than laundered forward.
    let total_sum_kop = match doc_type {
        DocType::Sell | DocType::Return => Some(
            row.total_sum_kop
                .ok_or(BuildReject::MissingTotalForSale { doc_type })?,
        ),
        _ => None,
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
        total_sum_kop,
        payload_json: row.payload_json.clone(),
        // The RECOMPUTED hash (verified == persisted above). source_sha256
        // is the inbox/source hash; for non-Z the two coincide (D5). A1Z
        // makes them diverge (canonical = hash of the aggregated Z body).
        payload_sha256_canonical: computed_payload_sha256,
        source_sha256: row.payload_sha256_canonical,
        signed_by_cashier_id,
        driver_id: Some(driver_id),
    })
}

/// Terminalize a rejected PROCESSING row INSIDE the caller's write-tx:
/// a `PROCESSING → REJECTED` guarded flip + an `INBOX_REJECTED` audit
/// carrying the machine code + reason. NO `fiscal_documents` row is
/// created — the row is a ledger non-event. Reaper-safe: REJECTED is
/// terminal (excluded from the pending-work index, never re-leased), so the
/// stale-PROCESSING reaper never re-drives it.
///
/// MUST run after `acquire_lease` has flipped the row to PROCESSING. The
/// flip is SOURCE-STATE-GUARDED ([`ingress_inbox::mark_rejected_if_processing_tx`]):
/// if the row is not PROCESSING (a double-drive / stale request_id /
/// concurrently-completed row) this returns `Err` and the audit is NOT
/// appended, so a terminal `DONE` row can never be clobbered to REJECTED.
pub async fn terminalize_rejected_tx(
    tx: &mut WriteTxConn<'_>,
    row: &InboxRow,
    reject: &BuildReject,
) -> anyhow::Result<()> {
    let rid_hex = hex_lower(&row.request_id);
    let marked = ingress_inbox::mark_rejected_if_processing_tx(tx, &row.request_id).await?;
    if !marked {
        return Err(anyhow::anyhow!(
            "terminalize_rejected_tx({rid_hex}): inbox row is not PROCESSING — refusing to \
             clobber a non-leased / already-terminal row (reject_code={})",
            reject.code()
        ));
    }
    let payload = serde_json::json!({
        "request_id": rid_hex,
        "fiscal_number": row.fiscal_number,
        "operation_type": row.operation_type,
        "reject_code": reject.code(),
        "reason": reject.to_string(),
    });
    audit_log::append_tx(
        tx,
        "ingress_inbox",
        &rid_hex,
        "INBOX_REJECTED",
        Severity::Warning,
        None,
        Some(&payload.to_string()),
    )
    .await?;
    Ok(())
}
