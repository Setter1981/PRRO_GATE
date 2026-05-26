//! W3 — ingress DTOs (wire contract with `maria304_driver`).
//!
//! Mirrors `rust/maria304_driver/src/bridge/dto.rs` field-by-field.
//! The two crates do NOT share a DTO crate by design (per plan §3 W3
//! Trade-off): `prro` is the system-of-record and must not depend on
//! a driver-binary crate, and reverse dep (driver → prro) would pull
//! the entire DB layer.  The wire contract is guarded by parity
//! fixtures in `tests/ingress_dto_parity.rs` — rename in either side
//! breaks the test.

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

    let total_sum_kop = match cmd.command_type {
        CommandType::Sell => Some(cmd.payload.totals.sale_kopecks as i64),
        CommandType::Return => Some(cmd.payload.totals.return_kopecks as i64),
        _ => None,
    };

    let business_ts = chrono::Utc::now().to_rfc3339();

    let payload_json_bytes = canonical_json_bytes(cmd)?;
    let payload_json = String::from_utf8(payload_json_bytes.clone())
        .expect("canonical JSON is always valid UTF-8 (serde_json output)");
    let payload_sha256_canonical: [u8; 32] = Sha256::digest(&payload_json_bytes).into();

    let signed_by_cashier_id = cmd
        .cashier_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(CashierId::new)
        .transpose()?;

    Ok(CanonicalFiscalCommand {
        doc_type,
        business_ts,
        total_sum_kop,
        payload_json,
        payload_sha256_canonical,
        signed_by_cashier_id,
    })
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
