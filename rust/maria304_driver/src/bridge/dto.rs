//! Canonical envelope DTOs — wire contract between the Rust driver
//! and the Python PRRO Gateway.
//!
//! Fields intentionally mirror `docs/superpowers/plans/2026-04-20-
//! maria304-driver.md §5.2`.  Any schema drift must be reflected in
//! both this module and `src/prro_gateway/adapters/maria304_native.py`
//! (M7).  Serialisation is JSON via serde.

use serde::{Deserialize, Serialize};

use crate::protocol::error_codes::ErrorCode;

/// Top-level envelope posted to `/v1/ingress/maria304`.
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

/// Envelope response — what the Python gateway returns on 200 OK.
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

/// Which business operation the driver is submitting.
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

/// Per-receipt direction flag — set by `SetCurrentCheckDirection` or
/// implied by `OpenReturnCheck`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReceiptDirection {
    #[default]
    Sale,
    Return,
}

/// Receipt body — goods + payments + optional dual-tax + totals.
///
/// M4 populates `raw_frames` verbatim (one entry per accumulated
/// receipt-building command) and the structured `direction` / `totals`
/// / `dual_tax_mode` metadata.  The richer `goods` / `payments`
/// parsing is scoped to the Python adapter (§M7) where the canonical
/// schema lives; those vectors remain empty in the M4 envelope.
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
    /// Raw wire commands accumulated inside the receipt, in submission
    /// order.  The Python adapter does the rich parsing on its side
    /// (invariant DRV-1 — no fiscal business logic in Rust).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_frames: Vec<RawFrame>,
}

/// Verbatim wire-command body paired with its opcode.  Driver keeps
/// these around so the Python adapter can parse / audit them without
/// re-scanning TCP.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RawFrame {
    pub opcode: String,
    pub body: String,
}

/// One fiscal line (FISC/ARFI/FICD + friends).
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

/// Discount/markup attached to a fiscal line.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Discount {
    pub direction: DiscountDirection,
    pub name: String,
    pub amount_kopecks: u64,
}

/// Discount vs markup — typed bit the `FiscalLineEx` wire sign signals.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DiscountDirection {
    Discount,
    Markup,
}

/// A single payment method applied to the receipt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanonicalPayment {
    #[serde(rename = "type")]
    pub kind: PaymentKind,
    pub amount_kopecks: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acquirer_slip: Option<AcquirerSlip>,
}

/// Payment kind — the 4 Maria-standard codes.  Extra acquirer-
/// specific classification happens on the Python side.
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

/// ЕПЗ slip — sourced from `AddSlip` / `PSDt` wire command.
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

/// Dual-tax mode set via `NLPR`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct DualTaxMode {
    pub tax_group_1: u8,
    pub tax_group_2: u8,
}

/// Pre-computed totals — driver does not recompute; the Python side
/// cross-validates against the goods list.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Totals {
    pub sale_kopecks: u64,
    pub return_kopecks: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_command_serde_roundtrips() {
        let cmd = CanonicalCommand {
            schema_version: "1.0".to_string(),
            fiscal_number: "3001234567".to_string(),
            command_type: CommandType::Sell,
            idempotency_key: "maria304:fn:sess:1".to_string(),
            cashier_id: Some("csh1".to_string()),
            department: Some("1".to_string()),
            return_check_number: None,
            payload: ReceiptPayload {
                direction: ReceiptDirection::Sale,
                goods: vec![FiscalLine {
                    name: "Паляниця".to_string(),
                    quantity_milli: 1000,
                    price_kopecks: 2500,
                    tax_group_1: 1,
                    tax_group_2: 0,
                    ..Default::default()
                }],
                payments: vec![CanonicalPayment {
                    kind: PaymentKind::Cash,
                    amount_kopecks: 2500,
                    acquirer_slip: None,
                }],
                dual_tax_mode: None,
                totals: Totals { sale_kopecks: 2500, return_kopecks: 0 },
                raw_frames: Vec::new(),
            },
        };

        let json = serde_json::to_string(&cmd).unwrap();
        let parsed: CanonicalCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, cmd);
    }

    #[test]
    fn command_type_serializes_as_screaming_snake_case() {
        let json = serde_json::to_string(&CommandType::ServiceIn).unwrap();
        assert_eq!(json, "\"SERVICE_IN\"");
    }

    #[test]
    fn payment_kind_serializes_as_screaming_snake_case() {
        for (kind, expected) in [
            (PaymentKind::Cash, "CASH"),
            (PaymentKind::Cashless1, "CASHLESS_1"),
            (PaymentKind::Cashless2, "CASHLESS_2"),
            (PaymentKind::Cashless3, "CASHLESS_3"),
        ] {
            let got = serde_json::to_string(&kind).unwrap();
            assert_eq!(got, format!("\"{expected}\""));
        }
    }

    #[test]
    fn receipt_direction_default_is_sale() {
        assert_eq!(ReceiptDirection::default(), ReceiptDirection::Sale);
    }

    #[test]
    fn fiscal_line_with_all_optionals_serializes_every_field() {
        let line = FiscalLine {
            name: "Test".to_string(),
            uktzed: Some("1234567890".to_string()),
            quantity_milli: 2000,
            price_kopecks: 1500,
            tax_group_1: 1,
            tax_group_2: 2,
            article_code: Some(42),
            discount: Some(Discount {
                direction: DiscountDirection::Discount,
                name: "Акція".to_string(),
                amount_kopecks: 300,
            }),
            excise_stamps: vec!["STAMP001".to_string()],
            barcode: Some("4820001234567".to_string()),
        };
        let json = serde_json::to_string(&line).unwrap();
        assert!(json.contains("\"uktzed\":\"1234567890\""));
        assert!(json.contains("\"article_code\":42"));
        assert!(json.contains("\"excise_stamps\":[\"STAMP001\"]"));
        assert!(json.contains("\"direction\":\"DISCOUNT\""));
    }

    #[test]
    fn fiscal_line_omits_empty_optionals_from_json() {
        let line = FiscalLine {
            name: "Bare".to_string(),
            quantity_milli: 1000,
            price_kopecks: 100,
            tax_group_1: 1,
            ..Default::default()
        };
        let json = serde_json::to_string(&line).unwrap();
        assert!(!json.contains("uktzed"));
        assert!(!json.contains("article_code"));
        assert!(!json.contains("discount"));
        assert!(!json.contains("excise_stamps"));
        assert!(!json.contains("barcode"));
    }

    #[test]
    fn canonical_response_deserializes_minimal_shape() {
        let json = r#"{
            "ok": true,
            "document_id": "abc",
            "fiscal_id": "0000001234",
            "fiscal_ts": "2026-04-20T10:15:30+00:00",
            "document_state": "ACK"
        }"#;
        let parsed: CanonicalResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.ok);
        assert_eq!(parsed.fiscal_id, "0000001234");
        assert_eq!(parsed.sale_total_kopecks, 0); // serde default
    }
}

/// Classification of a COMP (fiscal document send) response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentOutcome {
    /// DPS accepted the document — `fiscal_id` is valid; close receipt, store hash.
    Accepted { fiscal_id: u64, sale: u64, ret: u64 },
    /// Terminal failure — DPS permanently rejected or signed with bad key.
    /// Receipt must be closed as rejected; caller cannot retry this document.
    Terminal(ErrorCode),
    /// Transient failure — DPS may not have seen this document.
    /// Leave receipt open so caller can retry COMP or issue CANC.
    Retryable(ErrorCode),
}

/// Classify a `CanonicalResponse` into an outcome for the COMP handler.
///
/// Rationale: closing a receipt must reflect actual DPS outcome, not just
/// whether the HTTP call succeeded (Invariant #4 — idempotency).
#[must_use]
pub fn classify_response(resp: &CanonicalResponse) -> DocumentOutcome {
    if resp.ok {
        match resp.fiscal_id.parse::<u64>() {
            Ok(id) if id > 0 => DocumentOutcome::Accepted {
                fiscal_id: id,
                sale: resp.sale_total_kopecks,
                ret:  resp.return_total_kopecks,
            },
            // ok=true but fiscal_id zero or unparseable — data-contract violation from
            // the gateway.  Close the receipt (cannot retry this document) and signal
            // SoftBlock so 1C retries the higher-level operation from scratch.
            _ => DocumentOutcome::Terminal(ErrorCode::SoftBlock),
        }
    } else {
        match resp.document_state.as_str() {
            // Known permanent rejections — DPS will not accept a re-send.
            "REJECTED" | "ERROR_SIGN" | "ERROR_FISCAL" =>
                DocumentOutcome::Terminal(ErrorCode::SoftLocked),
            // ERROR_SEND or unknown state: DPS may not have seen it → retryable.
            // Includes ERROR_SAVE (-3 in native DPS protocol) per backlog ADR.
            _ => DocumentOutcome::Retryable(ErrorCode::SoftBlock),
        }
    }
}

#[cfg(test)]
mod classify_tests {
    use super::*;

    fn resp(ok: bool, fiscal_id: &str, document_state: &str) -> CanonicalResponse {
        CanonicalResponse {
            ok,
            document_id:         "doc1".into(),
            fiscal_id:           fiscal_id.into(),
            fiscal_ts:           "2026-04-22T10:00:00Z".into(),
            document_state:      document_state.into(),
            sale_total_kopecks:  1000,
            return_total_kopecks: 0,
        }
    }

    #[test]
    fn accepted_when_ok_and_valid_fiscal_id() {
        let r = resp(true, "12345", "SENT");
        assert!(matches!(
            classify_response(&r),
            DocumentOutcome::Accepted { fiscal_id: 12345, .. }
        ));
    }

    #[test]
    fn accepted_carries_sale_totals() {
        let r = resp(true, "9", "SENT");
        match classify_response(&r) {
            DocumentOutcome::Accepted { sale, .. } => assert_eq!(sale, 1000),
            other => panic!("expected Accepted, got {other:?}"),
        }
    }

    #[test]
    fn terminal_when_ok_but_fiscal_id_zero() {
        let r = resp(true, "0", "SENT");
        assert!(matches!(classify_response(&r), DocumentOutcome::Terminal(_)));
    }

    #[test]
    fn terminal_when_ok_but_fiscal_id_empty() {
        let r = resp(true, "", "SENT");
        assert!(matches!(classify_response(&r), DocumentOutcome::Terminal(_)));
    }

    #[test]
    fn terminal_on_rejected() {
        let r = resp(false, "", "REJECTED");
        assert!(matches!(classify_response(&r), DocumentOutcome::Terminal(_)));
    }

    #[test]
    fn terminal_on_error_sign() {
        let r = resp(false, "", "ERROR_SIGN");
        assert!(matches!(classify_response(&r), DocumentOutcome::Terminal(_)));
    }

    #[test]
    fn terminal_on_error_fiscal() {
        let r = resp(false, "", "ERROR_FISCAL");
        assert!(matches!(classify_response(&r), DocumentOutcome::Terminal(_)));
    }

    #[test]
    fn retryable_on_error_send() {
        let r = resp(false, "", "ERROR_SEND");
        assert!(matches!(classify_response(&r), DocumentOutcome::Retryable(_)));
    }

    #[test]
    fn retryable_on_empty_document_state() {
        let r = resp(false, "", "");
        assert!(matches!(classify_response(&r), DocumentOutcome::Retryable(_)));
    }
}
