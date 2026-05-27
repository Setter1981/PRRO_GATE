//! W3 — ingress DTOs (wire contract with `maria304_driver`).
//!
//! Mirrors `rust/maria304_driver/src/bridge/dto.rs` field-by-field.
//! The two crates do NOT share a DTO crate by design (per plan §3 W3
//! Trade-off): `prro` is the system-of-record and must not depend on
//! a driver-binary crate, and reverse dep (driver → prro) would pull
//! the entire DB layer.  The wire contract is guarded by parity
//! fixtures in `tests/ingress_dto_parity.rs` — rename in either side
//! breaks the test.
//!
//! # Known scope gap — `payload_json` is wire-shape, NOT stage_sign-ready
//!
//! `to_canonical_fiscal_command` populates `CanonicalFiscalCommand.
//! payload_json` with the **driver-wire-shape canonical JSON of
//! `cmd.payload`** (i.e. the `ReceiptPayload` struct as serialised by
//! serde with BTreeMap-sorted keys).  But `services/write_path/
//! stage_sign.rs::parse_payload` (the consumer at signing time)
//! uses `#[serde(deny_unknown_fields)]` and expects a DIFFERENT
//! internal canonical shape:
//!
//!   - `CheckJson  { items[]: { code, name, price_kop,
//!     quantity_thousandths, sum_kop }, payments[]: { name, sum_kop,
//!     type_code } }`        — for SELL / RETURN.
//!   - `ZReportJson { payments[]: { name, sum_in_kop, sum_out_kop,
//!     type_code }, sell_count, return_count }`
//!                            — for SHIFT_CLOSE / Z_REPORT.
//!   - `ShiftOpenJson { opening_sum_kop }`
//!                            — for SHIFT_OPEN.
//!
//! The W3 DTO emits `payload.goods[].price_kopecks /
//! quantity_milli / tax_group_1 / ...` + `payments[].type:
//! PaymentKind enum`.  Fields do not align by name OR by data —
//! e.g. `ZReportJson.sell_count` does not exist anywhere in the W3
//! DTO (the driver does not emit shift counters; they are derived
//! from the ledger).
//!
//! Consequence — until a conversion layer lands, the first real
//! receipt flowing through `to_canonical_fiscal_command` →
//! `ingress_inbox::insert` → `stage_acquire` → `stage_sign` will
//! fail with `SignError::PayloadSchema` at the parse_payload step.
//! The fixture parity tests in `tests/ingress_dto_parity.rs` do
//! NOT exercise this signing-time consumption — they assert the
//! DTO→CanonicalFiscalCommand mapping shape only.
//!
//! Resolution: the conversion is deferred to W4 (per-FN supervisor)
//! or a new conversion-stage worklet between mapper and stage_acquire
//! — see plan §3 W3 acceptance note + W4 Algorithm step 0
//! addendum.  The DTO→CheckJson/ZReportJson/ShiftOpenJson conversion
//! requires ledger lookups for ZReport (sell_count, return_count
//! derived from rows since `shift_open_at_business_ts`) — that is
//! repository-touching code, not pure DTO transformation, so it
//! does not belong in W3.
//!
//! See [`tests/ingress_dto_parity.rs::
//! mapped_payload_json_is_wire_shape_not_stage_sign_ready`]
//! (`#[ignore]`'d to document the gap loudly without breaking CI).

use crate::db::models::enums::DocType;
use crate::db::models::ids::{CashierId, CashierIdError};
use crate::services::write_path::types::CanonicalFiscalCommand;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const SCHEMA_VERSION: &str = "1.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanonicalCommand {
    pub schema_version: String,
    pub fiscal_number: String,
    pub command_type: CommandType,
    pub idempotency_key: String,
    pub cashier_id: Option<String>,
    pub department: Option<String>,
    pub return_check_number: Option<String>,
    pub payload: ReceiptPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanonicalResponse {
    pub ok: bool,
    pub document_id: String,
    pub fiscal_id: String,
    pub fiscal_ts: String,
    pub document_state: String,
    #[serde(default)]
    pub sale_total_kopecks: u64,
    #[serde(default)]
    pub return_total_kopecks: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CommandType {
    Sell,
    Return,
    ShiftOpen,
    ShiftClose,
    XReport,
    ZReport,
    ServiceIn,
    ServiceOut,
    CashWithdrawal,
    PeriodicReport,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReceiptDirection {
    #[default]
    Sale,
    Return,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ReceiptPayload {
    #[serde(default)]
    pub direction: ReceiptDirection,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub goods: Vec<FiscalLine>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub payments: Vec<CanonicalPayment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dual_tax_mode: Option<DualTaxMode>,
    pub totals: Totals,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_frames: Vec<RawFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RawFrame {
    pub opcode: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FiscalLine {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uktzed: Option<String>,
    pub quantity_milli: u64,
    pub price_kopecks: u64,
    pub tax_group_1: u8,
    pub tax_group_2: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub article_code: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discount: Option<Discount>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excise_stamps: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub barcode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Discount {
    pub direction: DiscountDirection,
    pub name: String,
    pub amount_kopecks: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DiscountDirection {
    Discount,
    Markup,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanonicalPayment {
    #[serde(rename = "type")]
    pub kind: PaymentKind,
    pub amount_kopecks: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acquirer_slip: Option<AcquirerSlip>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PaymentKind {
    #[serde(rename = "CASH")]
    Cash,
    #[serde(rename = "CASHLESS_1")]
    Cashless1,
    #[serde(rename = "CASHLESS_2")]
    Cashless2,
    #[serde(rename = "CASHLESS_3")]
    Cashless3,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AcquirerSlip {
    pub payment_form_index: u8,
    pub merchant_id: String,
    pub terminal_id: String,
    pub operation_type: String,
    pub pan: String,
    pub approval_code: String,
    pub payment_system: String,
    pub transaction_code: String,
    pub fee_kopecks: u64,
    pub cashier_signature_placeholder: bool,
    pub cardholder_signature_placeholder: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct DualTaxMode {
    pub tax_group_1: u8,
    pub tax_group_2: u8,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Totals {
    pub sale_kopecks: u64,
    pub return_kopecks: u64,
}

#[derive(Debug, Error)]
pub enum MappingError {
    /// W4-Z0 piece 9 — wire `fiscal_number` does NOT match the
    /// listener's configured FN (per `ops/config.yaml`).  Listener
    /// rejects the receipt before any write-path work begins.
    #[error("listener FN mismatch: wire={wire_fn:?} listener={listener_fn:?}")]
    FnConfigMismatch {
        wire_fn: String,
        listener_fn: String,
    },

    #[error("schema_version mismatch: expected {expected}, got {actual}")]
    SchemaVersionMismatch {
        expected: &'static str,
        actual: String,
    },
    #[error("unsupported command_type for fiscal pipeline: {0:?}")]
    UnsupportedCommandType(CommandType),
    #[error("invalid cashier_id: {0}")]
    InvalidCashierId(#[from] CashierIdError),
    #[error("canonical JSON serialisation failed: {0}")]
    CanonicalSerialise(#[from] serde_json::Error),
}

pub fn to_canonical_fiscal_command(
    cmd: &CanonicalCommand,
) -> Result<CanonicalFiscalCommand, MappingError> {
    if cmd.schema_version != SCHEMA_VERSION {
        return Err(MappingError::SchemaVersionMismatch {
            expected: SCHEMA_VERSION,
            actual: cmd.schema_version.clone(),
        });
    }

    let doc_type = match cmd.command_type {
        CommandType::Sell => DocType::Sell,
        CommandType::Return => DocType::Return,
        CommandType::ShiftOpen => DocType::ShiftOpen,
        CommandType::ShiftClose => DocType::ShiftClose,
        CommandType::XReport => DocType::XReport,
        CommandType::ZReport => DocType::ZReport,
        CommandType::ServiceIn => DocType::ServiceIn,
        CommandType::ServiceOut => DocType::ServiceOut,
        CommandType::CashWithdrawal => DocType::CashWithdrawal,
        CommandType::PeriodicReport => {
            return Err(MappingError::UnsupportedCommandType(CommandType::PeriodicReport));
        }
    };

    // total_sum_kop derivation — explicit per variant so the None
    // arms are intentional rather than the residue of a wildcard.
    // Self-review finding #1 (2026-05-26): driver `Totals` struct only
    // carries `sale_kopecks` + `return_kopecks`.  Cash-movement ops
    // (ServiceIn / ServiceOut / CashWithdrawal) are MONETARY per UA
    // fiscal protocol but the driver does not emit their amounts via
    // `Totals` — the source data lives in `raw_frames` and is parsed
    // by M5.  Until then `None` is the truthful answer: we don't
    // know the cash amount at this layer.  `mac_recovery.rs` accepts
    // NULL `total_sum_kop` (column is nullable) — the MAC chain
    // input is `total_sum_kop OR 0` so a NULL doesn't break
    // determinism, only obscures the cash subtotal in reporting
    // queries.  SHIFT_OPEN / SHIFT_CLOSE / reports legitimately have
    // no monetary total.
    let total_sum_kop = match cmd.command_type {
        CommandType::Sell => Some(cmd.payload.totals.sale_kopecks as i64),
        CommandType::Return => Some(cmd.payload.totals.return_kopecks as i64),
        CommandType::ServiceIn
        | CommandType::ServiceOut
        | CommandType::CashWithdrawal => None, // M5: parse from raw_frames
        CommandType::ShiftOpen
        | CommandType::ShiftClose
        | CommandType::XReport
        | CommandType::ZReport => None, // no monetary total by design
        CommandType::PeriodicReport => unreachable!(
            "PeriodicReport rejected by command_type match arm above; \
             control flow does not reach total_sum_kop derivation"
        ),
    };

    // business_ts at mapping time.  Wall-clock (`Utc::now()`) here
    // means a retried `to_canonical_fiscal_command` call for the same
    // logical fiscal event produces a DIFFERENT business_ts on each
    // invocation.  `payload_sha256_canonical` is intentionally stable
    // across retries (it hashes `cmd.payload` only — see below); but
    // the persisted `fiscal_documents.business_ts` row differs between
    // attempts, which the MAC-chain recovery path (`mac_recovery.rs`
    // §business_ts read) treats as a fresh document.  Pre-pilot risk
    // is bounded because `ingress_inbox::insert`'s idempotency_key
    // probe blocks duplicate processing BEFORE business_ts lands.
    // M5 plumbs the driver-supplied business clock through
    // `raw_frames` parsing; this placeholder is the W3-scope contract.
    let business_ts = chrono::Utc::now().to_rfc3339();

    // Hash scope: `cmd.payload` ONLY — NOT the full envelope.  The
    // field name `payload_sha256_canonical` promises payload-scope,
    // and integrity-check semantics require that two retries with the
    // same fiscal goods/payments but different `idempotency_key` /
    // `cashier_id` / `department` produce the SAME hash (so an
    // operator can detect payload tampering independently of routing
    // metadata drift).  Envelope-level uniqueness is the
    // `idempotency_key` field's job, not the hash's.
    let payload_json_bytes = canonical_json_bytes(&cmd.payload)?;
    let payload_json = String::from_utf8(payload_json_bytes.clone())
        .expect("canonical JSON is always valid UTF-8 (serde_json output)");
    let payload_sha256_canonical: [u8; 32] = Sha256::digest(&payload_json_bytes).into();

    // `cashier_id` mapping — explicit empty-vs-absent distinction.
    // Self-review finding #3 (Round 2 audit, 2026-05-26):
    //
    //   `cashier_id: None`        → `signed_by_cashier_id = None`
    //                                (operator intentionally omitted
    //                                cashier id from envelope).
    //   `cashier_id: Some(s)` where `s` non-empty
    //                              → `Some(CashierId::new(s)?)`.
    //   `cashier_id: Some("")`    → typed `InvalidCashierId(
    //                                CashierIdError::Empty)` —
    //                                empty string is MALFORMED input,
    //                                NOT semantic-absent.  Previously
    //                                `.filter(|s| !s.is_empty())`
    //                                normalised it to `None` which
    //                                (a) hid wire-bug from operator
    //                                audit_log and (b) shifted the
    //                                failure surface to the signer
    //                                (where the precise context is
    //                                gone).  Drop the filter.
    let signed_by_cashier_id = cmd
        .cashier_id
        .as_deref()
        .map(CashierId::new)
        .transpose()?;

    Ok(CanonicalFiscalCommand {
        doc_type,
        business_ts,
        total_sum_kop,
        payload_json,
        payload_sha256_canonical,
        signed_by_cashier_id,
        driver_id: None, // W4-Z0 piece 9: set via `to_canonical_fiscal_command_with_context`
    })
}

/// W4-Z0 piece 9 — listener-stamped variant.  Validates wire
/// `fiscal_number` matches the listener's configured FN, then maps
/// + stamps the driver_id into the canonical command.  Listener
/// catches the cross-FN typo at the source instead of letting the
/// boot-time reconcile audit catch it hours later.
pub fn to_canonical_fiscal_command_with_context(
    cmd: &CanonicalCommand,
    listener_driver_id: crate::db::models::ids::DriverId,
    listener_fn: &str,
) -> Result<CanonicalFiscalCommand, MappingError> {
    if cmd.fiscal_number != listener_fn {
        return Err(MappingError::FnConfigMismatch {
            wire_fn: cmd.fiscal_number.clone(),
            listener_fn: listener_fn.to_string(),
        });
    }
    let mut canonical = to_canonical_fiscal_command(cmd)?;
    canonical.driver_id = Some(listener_driver_id);
    Ok(canonical)
}

pub fn canonical_json_bytes<T: Serialize>(v: &T) -> Result<Vec<u8>, serde_json::Error> {
    fn sort_recursive(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::Object(map) => {
                let original = std::mem::take(map);
                let mut sorted: std::collections::BTreeMap<String, serde_json::Value> =
                    std::collections::BTreeMap::new();
                for (k, mut child) in original {
                    sort_recursive(&mut child);
                    sorted.insert(k, child);
                }
                *map = serde_json::Map::from_iter(sorted);
            }
            serde_json::Value::Array(arr) => {
                for item in arr {
                    sort_recursive(item);
                }
            }
            _ => {}
        }
    }
    let mut value = serde_json::to_value(v)?;
    sort_recursive(&mut value);
    serde_json::to_vec(&value)
}
