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
//! `Z_REPORT` is intentionally NOT handled here: the Z path needs payload
//! aggregation + a divergent dual-hash (RS-3 A1Z). The dispatcher (A2)
//! routes Z to that builder; a Z row reaching this non-Z builder is a
//! dispatch bug, surfaced as a diagnosable reject rather than a silent
//! mis-build.

use crate::db::models::enums::{DocType, Severity};
use crate::db::models::ids::{CashierId, DriverId};
use crate::db::repositories::audit_log;
use crate::db::repositories::ingress_inbox::{self, InboxRow};
use crate::db::tx::WriteTxConn;
use crate::services::write_path::types::{hex_encode_lower as hex_lower, CanonicalFiscalCommand};

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
    /// `Z_REPORT` reached the non-Z builder — a dispatcher bug. Z needs
    /// the A1Z aggregation / dual-hash builder. Distinct from
    /// `UnsupportedOperation` so a misroute is diagnosable in the audit.
    ZRequiresAggregationBuilder,
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
            BuildReject::ZRequiresAggregationBuilder => "Z_REQUIRES_AGGREGATION",
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
            BuildReject::ZRequiresAggregationBuilder => {
                write!(f, "Z_REPORT must be built by the A1Z aggregation builder")
            }
            BuildReject::InvalidIdentity { field } => write!(f, "malformed {field}"),
        }
    }
}

/// Map the wire `operation_type` to a non-Z [`DocType`]. `Z_REPORT` is
/// rejected with the dedicated [`BuildReject::ZRequiresAggregationBuilder`]
/// (routes to A1Z); any other unknown / non-pilot op is
/// [`BuildReject::UnsupportedOperation`].
fn map_non_z_doc_type(operation_type: &str) -> Result<DocType, BuildReject> {
    match operation_type {
        "SELL" => Ok(DocType::Sell),
        "RETURN" => Ok(DocType::Return),
        "SHIFT_OPEN" => Ok(DocType::ShiftOpen),
        "SHIFT_CLOSE" => Ok(DocType::ShiftClose),
        "Z_REPORT" => Err(BuildReject::ZRequiresAggregationBuilder),
        other => Err(BuildReject::UnsupportedOperation {
            operation_type: other.to_string(),
        }),
    }
}

/// Pure mapping + null-guard — NO IO. Returns the canonical command, or a
/// TERMINAL reject reason the caller hands to [`terminalize_rejected_tx`].
///
/// Call ONLY for non-Z rows; the dispatcher routes `Z_REPORT` to the A1Z
/// builder. For non-Z, `source_sha256 == payload_sha256_canonical` (one
/// payload, one hash; per D5).
pub fn build_canonical(row: &InboxRow) -> Result<CanonicalFiscalCommand, BuildReject> {
    let doc_type = map_non_z_doc_type(&row.operation_type)?;

    // Null-contract: driver_id REQUIRED (fail-closed) for every row.
    let driver_raw = row
        .driver_id
        .as_deref()
        .ok_or(BuildReject::MissingDriverId)?;
    let driver_id = DriverId::new(driver_raw)
        .map_err(|_| BuildReject::InvalidIdentity { field: "driver_id" })?;

    // business_ts REQUIRED for every row.
    let business_ts = row
        .business_ts
        .clone()
        .ok_or(BuildReject::MissingBusinessTs)?;

    // total_sum_kop REQUIRED for SELL / RETURN; carried through (None
    // expected) for SHIFT_OPEN / SHIFT_CLOSE.
    let total_sum_kop = match doc_type {
        DocType::Sell | DocType::Return => Some(
            row.total_sum_kop
                .ok_or(BuildReject::MissingTotalForSale { doc_type })?,
        ),
        _ => row.total_sum_kop,
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
        payload_sha256_canonical: row.payload_sha256_canonical,
        // RS-3 D5: non-Z — source and canonical hashes coincide.
        source_sha256: row.payload_sha256_canonical,
        signed_by_cashier_id,
        driver_id: Some(driver_id),
    })
}

/// Terminalize a rejected PROCESSING row INSIDE the caller's write-tx:
/// `mark_rejected_tx` (status → REJECTED) + an `INBOX_REJECTED` audit
/// carrying the machine code + reason. NO `fiscal_documents` row is
/// created — the row is a ledger non-event. Reaper-safe: REJECTED is
/// terminal, so the stale-PROCESSING reaper never re-drives it.
///
/// MUST run after `acquire_lease` has flipped the row to PROCESSING, so a
/// build reject does not leave a NEW/PROCESSING row the reaper would loop.
pub async fn terminalize_rejected_tx(
    tx: &mut WriteTxConn<'_>,
    row: &InboxRow,
    reject: &BuildReject,
) -> anyhow::Result<()> {
    let rid_hex = hex_lower(&row.request_id);
    ingress_inbox::mark_rejected_tx(tx, &row.request_id).await?;
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
