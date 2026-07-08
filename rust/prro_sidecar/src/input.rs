//! Canonical-command serde structs — minimal subset for XML build + license check.
//!
//! Unknown fields must not break us (Python-side additions use `#[serde(flatten)]`
//! on the `other` map). `schema_version` is required — invariant (7).

use serde::{Deserialize, Serialize};

// ── Operation types supported by the sidecar ─────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OperationType {
    ShiftOpen,
    ShiftClose,
    Sell,
    Return,
    ServiceIn,
    ServiceOut,
    CashWithdrawal,
    ZReport,
    // Recognized by the sidecar so deserialization succeeds; rejected at the handler
    // with a clean 400 "operation not supported". Python write_path must not route
    // these here, but graceful rejection is better than a serde parse error.
    XReport,
    GoOffline,
    GoOnline,
}

impl OperationType {
    /// Returns false for op types that the sidecar recognizes but does not execute.
    /// The HTTP handler should return 400 before touching any DB or crypto.
    ///
    /// `ShiftClose` is recognized (so deserialization succeeds) but is NOT
    /// supported yet: the SHIFT_CLOSE XML (T="101") builder requires closing
    /// totals from the shift summary which is not yet wired in Phase 5.
    pub fn is_sidecar_supported(&self) -> bool {
        !matches!(
            self,
            Self::ShiftClose | Self::XReport | Self::GoOffline | Self::GoOnline
        )
    }
}

// ── Top-level envelope ────────────────────────────────────────────────────────

/// Canonical fiscal command posted by Python write_path to POST /fiscal/send.
///
/// `schema_version` is required: missing it is a hard 400 (invariant 7).
/// `payload` keeps the full Python payload as-is for audit + xml_builder access.
/// Unknown envelope fields land in `other` for forward-compatibility.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CanonicalCommand {
    pub schema_version: String,
    pub request_id: String,
    pub idempotency_key: String,
    pub operation_type: OperationType,
    pub fiscal_number: String,
    pub business_ts: String, // ISO-8601 UTC
    pub payload: serde_json::Value,
    pub payload_sha256: String,
    #[serde(flatten)]
    pub other: serde_json::Map<String, serde_json::Value>,
}

impl CanonicalCommand {
    /// Typed view of `payload["receipt"]` — for SELL / RETURN / SHIFT_* ops.
    ///
    /// Each call clones and re-parses the receipt sub-value (`serde_json::from_value`
    /// requires ownership). Callers that need the receipt more than once should bind
    /// the result to a local variable rather than calling this method repeatedly.
    /// `has_card_rrn()` avoids this cost by navigating raw JSON directly.
    pub fn receipt(&self) -> Option<Receipt> {
        self.payload
            .get("receipt")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Typed view of `payload["z_report_data"]` — for Z_REPORT.
    pub fn z_report_data(&self) -> Option<ZReportData> {
        self.payload
            .get("z_report_data")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// `payload["service_sum"]` in kopecks — for SERVICE_IN / SERVICE_OUT.
    pub fn service_sum(&self) -> Option<i64> {
        self.payload.get("service_sum").and_then(|v| v.as_i64())
    }

    /// `payload["cash_withdrawal_sum"]` in kopecks — for CASH_WITHDRAWAL.
    pub fn cash_withdrawal_sum(&self) -> Option<i64> {
        self.payload
            .get("cash_withdrawal_sum")
            .and_then(|v| v.as_i64())
    }

    /// True if any payment in the receipt has a non-empty RRN.
    /// Drives `fn_config.national_check_enabled` <L> tag injection.
    ///
    /// Navigates the raw JSON directly (JSON Pointer RFC 6901) to avoid cloning
    /// and re-parsing the entire receipt sub-tree.
    pub fn has_card_rrn(&self) -> bool {
        self.payload
            .pointer("/receipt/payments")
            .and_then(|v| v.as_array())
            .is_some_and(|payments| {
                payments.iter().any(|p| {
                    p.get("rrn")
                        .and_then(|r| r.as_str())
                        .is_some_and(|s| !s.is_empty())
                })
            })
    }
}

// ── Receipt ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Receipt {
    pub header: Option<String>,
    pub footer: Option<String>,
    #[serde(default)]
    pub goods: Vec<Good>,
    #[serde(default)]
    pub payments: Vec<Payment>,
    pub totals: Option<ReceiptTotals>,
    #[serde(default)]
    pub discounts: Vec<Discount>,
    pub rounding: Option<i64>, // kopecks rounding adjustment
}

// ── Good (receipt item) ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Good {
    pub code: Option<String>,
    pub name: String,
    pub price: i64,    // kopecks
    pub quantity: i64, // thousandths (e.g. 1000 = 1 unit)
    pub sum: i64,      // kopecks
    pub barcode: Option<String>,
    pub uktzed: Option<String>,
    pub tax_id: Option<i64>,
    pub tax_id_2: Option<i64>,
    #[serde(default)]
    pub excise_barcodes: Vec<String>,
    #[serde(default)]
    pub discounts: Vec<Discount>,
}

// ── Payment ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Payment {
    /// dps_xml.py reads `p.get('type', 'CASH')` — accept both JSON keys.
    #[serde(rename = "type", alias = "payment_type", default = "cash_payment_type")]
    pub payment_type: String,
    pub amount: i64, // kopecks, always non-negative
    pub rrn: Option<String>,
    pub payment_system: Option<String>,
    pub bank_name: Option<String>,
    pub terminal: Option<String>,
    pub label: Option<String>,
    pub card_mask: Option<String>,
    pub auth_code: Option<String>,
    pub commission: Option<i64>,
}

fn cash_payment_type() -> String {
    "CASH".to_string()
}

// ── Discount ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Discount {
    pub value: i64,
    #[serde(
        rename = "type",
        alias = "discount_type",
        default = "discount_type_default"
    )]
    pub discount_type: String, // "DISCOUNT" | "EXTRA_CHARGE"
    #[serde(
        rename = "mode",
        alias = "discount_mode",
        default = "discount_mode_default"
    )]
    pub discount_mode: String, // "VALUE" | "PERCENT"
    pub name: Option<String>,
    pub privilege: Option<String>,
    pub tax_code: Option<String>,
}

fn discount_type_default() -> String {
    "DISCOUNT".to_string()
}
fn discount_mode_default() -> String {
    "VALUE".to_string()
}

// ── Totals ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ReceiptTotals {
    pub total_sum: Option<i64>,
    pub round_sum: Option<i64>,
    pub discounts_sum: Option<i64>,
    pub extra_charge_sum: Option<i64>,
}

// ── Z-report data ─────────────────────────────────────────────────────────────

/// Aggregated Z-report payload from Python write_path.
/// Sub-fields are kept as raw JSON Values — Phase 2 xml_builder navigates them.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ZReportData {
    /// dict[tax_id → {smi, smo}] — per-group sales in/out totals
    pub tax_sums: Option<serde_json::Value>,
    /// dict[payment_type → {smi, smo}] — payment totals
    pub payment_sums: Option<serde_json::Value>,
    /// dict[service_type → {smi, smo}] — service in/out
    pub service_sums: Option<serde_json::Value>,
    /// {ni, no} — sell/return check counts
    pub check_count: Option<serde_json::Value>,
    /// {epc, epcs, epsm} — cash withdrawal summaries (optional)
    pub epz_sums: Option<serde_json::Value>,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sell_json() -> &'static str {
        r#"{
            "schema_version": "1.0",
            "request_id":     "req-1",
            "idempotency_key":"idem-1",
            "operation_type": "SELL",
            "fiscal_number":  "3001234567",
            "business_ts":    "2026-04-19T12:00:00Z",
            "payload_sha256": "abc",
            "payload": {
                "receipt": {
                    "goods": [
                        {"name":"Кава","price":5000,"quantity":1000,"sum":5000}
                    ],
                    "payments": [
                        {"type":"CASH","amount":5000}
                    ],
                    "totals": {"total_sum": 5000}
                }
            }
        }"#
    }

    #[test]
    fn deserialize_sell() {
        let cmd: CanonicalCommand = serde_json::from_str(make_sell_json()).unwrap();
        assert_eq!(cmd.operation_type, OperationType::Sell);
        assert_eq!(cmd.fiscal_number, "3001234567");
        let r = cmd.receipt().unwrap();
        assert_eq!(r.goods.len(), 1);
        assert_eq!(r.goods[0].name, "Кава");
        assert_eq!(r.payments[0].payment_type, "CASH");
        assert_eq!(r.totals.unwrap().total_sum, Some(5000));
    }

    #[test]
    fn missing_schema_version_fails() {
        let json = r#"{"request_id":"r","idempotency_key":"k","operation_type":"SELL","fiscal_number":"fn","business_ts":"ts","payload":{},"payload_sha256":"x"}"#;
        let result: Result<CanonicalCommand, _> = serde_json::from_str(json);
        assert!(result.is_err(), "schema_version is required");
    }

    #[test]
    fn payment_type_alias() {
        let json = r#"{"payment_type":"CARD","amount":1000}"#;
        let p: Payment = serde_json::from_str(json).unwrap();
        assert_eq!(p.payment_type, "CARD");
    }

    #[test]
    fn has_card_rrn_true() {
        let json = serde_json::json!({
            "schema_version":"1.0","request_id":"r","idempotency_key":"k",
            "operation_type":"SELL","fiscal_number":"fn","business_ts":"ts",
            "payload_sha256":"x",
            "payload":{"receipt":{"goods":[{"name":"X","price":1,"quantity":1000,"sum":1}],
                "payments":[{"type":"CARD","amount":1,"rrn":"123456789012"}]}}
        });
        let cmd: CanonicalCommand = serde_json::from_value(json).unwrap();
        assert!(cmd.has_card_rrn());
    }

    #[test]
    fn z_report_deserialize() {
        let json = serde_json::json!({
            "schema_version":"1.0","request_id":"r","idempotency_key":"k",
            "operation_type":"Z_REPORT","fiscal_number":"fn","business_ts":"ts",
            "payload_sha256":"x",
            "payload":{"z_report_data":{"check_count":{"ni":5,"no":1}}}
        });
        let cmd: CanonicalCommand = serde_json::from_value(json).unwrap();
        let zr = cmd.z_report_data().unwrap();
        assert!(zr.check_count.is_some());
    }

    #[test]
    fn unknown_fields_ignored() {
        let mut json: serde_json::Value = serde_json::from_str(make_sell_json()).unwrap();
        json["future_field_v3"] = serde_json::json!("some_value");
        let cmd: CanonicalCommand = serde_json::from_value(json).unwrap();
        assert_eq!(cmd.operation_type, OperationType::Sell);
    }

    #[test]
    fn unsupported_op_types_deserialize_but_rejected() {
        for op in ["SHIFT_CLOSE", "X_REPORT", "GO_OFFLINE", "GO_ONLINE"] {
            let json = serde_json::json!({
                "schema_version":"1.0","request_id":"r","idempotency_key":"k",
                "operation_type": op, "fiscal_number":"fn","business_ts":"ts",
                "payload_sha256":"x","payload":{}
            });
            let cmd: CanonicalCommand =
                serde_json::from_value(json).unwrap_or_else(|e| panic!("{op} should parse: {e}"));
            assert!(
                !cmd.operation_type.is_sidecar_supported(),
                "{op} must not be sidecar-supported"
            );
        }
    }

    #[test]
    fn supported_op_types_accepted() {
        // SHIFT_CLOSE / X_REPORT / GO_OFFLINE / GO_ONLINE are not yet implemented (Phase 5)
        for op in [
            "SHIFT_OPEN",
            "SELL",
            "RETURN",
            "SERVICE_IN",
            "SERVICE_OUT",
            "CASH_WITHDRAWAL",
            "Z_REPORT",
        ] {
            let json = serde_json::json!({
                "schema_version":"1.0","request_id":"r","idempotency_key":"k",
                "operation_type": op, "fiscal_number":"fn","business_ts":"ts",
                "payload_sha256":"x","payload":{}
            });
            let cmd: CanonicalCommand =
                serde_json::from_value(json).unwrap_or_else(|e| panic!("{op} should parse: {e}"));
            assert!(
                cmd.operation_type.is_sidecar_supported(),
                "{op} must be sidecar-supported"
            );
        }
    }

    #[test]
    fn has_card_rrn_uses_raw_json_no_double_parse() {
        let base = |rrn: serde_json::Value| {
            serde_json::json!({
                "schema_version":"1.0","request_id":"r","idempotency_key":"k",
                "operation_type":"SELL","fiscal_number":"fn","business_ts":"ts",
                "payload_sha256":"x",
                "payload":{"receipt":{"payments":[{"type":"CASH","amount":100,"rrn": rrn}]}}
            })
        };
        let cmd_empty: CanonicalCommand =
            serde_json::from_value(base(serde_json::json!(""))).unwrap();
        assert!(!cmd_empty.has_card_rrn(), "empty rrn → false");

        let cmd_null: CanonicalCommand =
            serde_json::from_value(base(serde_json::json!(null))).unwrap();
        assert!(!cmd_null.has_card_rrn(), "null rrn → false");

        let cmd_real: CanonicalCommand =
            serde_json::from_value(base(serde_json::json!("000123456789"))).unwrap();
        assert!(cmd_real.has_card_rrn(), "non-empty rrn → true");
    }

    // ── helper ────────────────────────────────────────────────────────────────

    fn cmd_with_payload(payload: serde_json::Value) -> CanonicalCommand {
        serde_json::from_value(serde_json::json!({
            "schema_version":"1.0","request_id":"r","idempotency_key":"k",
            "operation_type":"SELL","fiscal_number":"fn","business_ts":"ts",
            "payload_sha256":"x","payload": payload
        }))
        .unwrap()
    }

    // ── receipt() ─────────────────────────────────────────────────────────────

    #[test]
    fn receipt_absent_returns_none() {
        let cmd = cmd_with_payload(serde_json::json!({}));
        assert!(cmd.receipt().is_none());
    }

    #[test]
    fn receipt_malformed_type_returns_none() {
        // "receipt" is a number, not an object — serde fails silently via .ok()
        let cmd = cmd_with_payload(serde_json::json!({"receipt": 42}));
        assert!(cmd.receipt().is_none());
    }

    #[test]
    fn receipt_minimal_fields() {
        // Receipt with only goods/payments/totals absent; defaults kick in.
        let cmd = cmd_with_payload(serde_json::json!({"receipt": {}}));
        let r = cmd.receipt().unwrap();
        assert!(r.goods.is_empty());
        assert!(r.payments.is_empty());
        assert!(r.discounts.is_empty());
        assert!(r.totals.is_none());
        assert!(r.header.is_none());
        assert!(r.rounding.is_none());
    }

    // ── z_report_data() ───────────────────────────────────────────────────────

    #[test]
    fn z_report_data_absent_returns_none() {
        let cmd = cmd_with_payload(serde_json::json!({}));
        assert!(cmd.z_report_data().is_none());
    }

    #[test]
    fn z_report_data_empty_object_gives_all_none() {
        let cmd = cmd_with_payload(serde_json::json!({"z_report_data": {}}));
        let zr = cmd.z_report_data().unwrap();
        assert!(zr.tax_sums.is_none());
        assert!(zr.payment_sums.is_none());
        assert!(zr.service_sums.is_none());
        assert!(zr.check_count.is_none());
        assert!(zr.epz_sums.is_none());
    }

    // ── service_sum() / cash_withdrawal_sum() ─────────────────────────────────

    #[test]
    fn service_sum_present_and_absent() {
        let cmd = cmd_with_payload(serde_json::json!({"service_sum": 75000}));
        assert_eq!(cmd.service_sum(), Some(75000));

        let cmd2 = cmd_with_payload(serde_json::json!({}));
        assert_eq!(cmd2.service_sum(), None);
    }

    #[test]
    fn cash_withdrawal_sum_present_and_absent() {
        let cmd = cmd_with_payload(serde_json::json!({"cash_withdrawal_sum": 100000}));
        assert_eq!(cmd.cash_withdrawal_sum(), Some(100000));

        let cmd2 = cmd_with_payload(serde_json::json!({}));
        assert_eq!(cmd2.cash_withdrawal_sum(), None);
    }

    // ── has_card_rrn() edge cases ─────────────────────────────────────────────

    #[test]
    fn has_card_rrn_false_when_no_receipt_key() {
        let cmd = cmd_with_payload(serde_json::json!({}));
        assert!(!cmd.has_card_rrn());
    }

    #[test]
    fn has_card_rrn_false_when_payments_empty() {
        let cmd = cmd_with_payload(serde_json::json!({"receipt": {"payments": []}}));
        assert!(!cmd.has_card_rrn());
    }

    #[test]
    fn has_card_rrn_true_when_any_payment_has_rrn() {
        // First payment: CASH, no RRN. Second payment: CARD with RRN.
        // has_card_rrn must return true if ANY payment has a non-empty RRN.
        let cmd = cmd_with_payload(serde_json::json!({
            "receipt": {
                "payments": [
                    {"type":"CASH","amount":3000},
                    {"type":"CARD","amount":2000,"rrn":"000123456789"}
                ]
            }
        }));
        assert!(cmd.has_card_rrn());
    }

    // ── Good ─────────────────────────────────────────────────────────────────

    #[test]
    fn good_optional_fields_absent() {
        let json = r#"{"name":"Молоко","price":3000,"quantity":1000,"sum":3000}"#;
        let g: Good = serde_json::from_str(json).unwrap();
        assert_eq!(g.name, "Молоко");
        assert_eq!(g.price, 3000);
        assert!(g.code.is_none());
        assert!(g.barcode.is_none());
        assert!(g.uktzed.is_none());
        assert!(g.tax_id.is_none());
        assert!(g.tax_id_2.is_none());
        assert!(g.excise_barcodes.is_empty());
        assert!(g.discounts.is_empty());
    }

    // ── Discount ─────────────────────────────────────────────────────────────

    #[test]
    fn discount_serde_defaults() {
        // Only "value" field — discount_type and discount_mode must default.
        let json = r#"{"value": 500}"#;
        let d: Discount = serde_json::from_str(json).unwrap();
        assert_eq!(d.value, 500);
        assert_eq!(d.discount_type, "DISCOUNT");
        assert_eq!(d.discount_mode, "VALUE");
        assert!(d.name.is_none());
        assert!(d.privilege.is_none());
        assert!(d.tax_code.is_none());
    }

    #[test]
    fn discount_serde_aliases() {
        // Python may send "discount_type"/"discount_mode" instead of "type"/"mode".
        let json = r#"{"value":200,"discount_type":"EXTRA_CHARGE","discount_mode":"PERCENT"}"#;
        let d: Discount = serde_json::from_str(json).unwrap();
        assert_eq!(d.discount_type, "EXTRA_CHARGE");
        assert_eq!(d.discount_mode, "PERCENT");
    }

    #[test]
    fn discount_canonical_keys() {
        // Canonical serde uses rename "type"/"mode".
        let json = r#"{"value":100,"type":"EXTRA_CHARGE","mode":"VALUE"}"#;
        let d: Discount = serde_json::from_str(json).unwrap();
        assert_eq!(d.discount_type, "EXTRA_CHARGE");
        assert_eq!(d.discount_mode, "VALUE");
    }

    // ── OperationType serialization ───────────────────────────────────────────

    #[test]
    fn operation_type_serializes_screaming_snake() {
        let cases = [
            (OperationType::ShiftOpen, "\"SHIFT_OPEN\""),
            (OperationType::ShiftClose, "\"SHIFT_CLOSE\""),
            (OperationType::Sell, "\"SELL\""),
            (OperationType::Return, "\"RETURN\""),
            (OperationType::ServiceIn, "\"SERVICE_IN\""),
            (OperationType::ServiceOut, "\"SERVICE_OUT\""),
            (OperationType::CashWithdrawal, "\"CASH_WITHDRAWAL\""),
            (OperationType::ZReport, "\"Z_REPORT\""),
            (OperationType::XReport, "\"X_REPORT\""),
            (OperationType::GoOffline, "\"GO_OFFLINE\""),
            (OperationType::GoOnline, "\"GO_ONLINE\""),
        ];
        for (op, expected) in cases {
            let serialized = serde_json::to_string(&op).unwrap();
            assert_eq!(serialized, expected, "wrong serialization for {op:?}");
        }
    }

    // ── Payment default ───────────────────────────────────────────────────────

    #[test]
    fn payment_type_defaults_to_cash_when_key_absent() {
        // Neither "type" nor "payment_type" present → default "CASH"
        let json = r#"{"amount": 5000}"#;
        let p: Payment = serde_json::from_str(json).unwrap();
        assert_eq!(p.payment_type, "CASH");
        assert!(p.rrn.is_none());
        assert!(p.commission.is_none());
    }

    // ── New evidential tests ──────────────────────────────────────────────────

    /// Truly unknown operation_type strings not in the enum must fail deserialization.
    #[test]
    fn unknown_operation_type_fails_deserialization() {
        let json = serde_json::json!({
            "schema_version":"1.0","request_id":"r","idempotency_key":"k",
            "operation_type":"UNKNOWN_FISCAL_OP",
            "fiscal_number":"fn","business_ts":"ts",
            "payload_sha256":"x","payload":{}
        });
        let result = serde_json::from_value::<CanonicalCommand>(json);
        assert!(
            result.is_err(),
            "unknown operation_type must fail deserialization"
        );
    }

    /// amount=0 is accepted at the deserialization layer (domain validation is downstream).
    #[test]
    fn payment_amount_zero_accepted() {
        let json = serde_json::json!({
            "schema_version":"1.0","request_id":"r","idempotency_key":"k",
            "operation_type":"SELL","fiscal_number":"fn","business_ts":"ts",
            "payload_sha256":"x",
            "payload":{"receipt":{"payments":[{"type":"CASH","amount":0}]}}
        });
        let cmd: CanonicalCommand =
            serde_json::from_value(json).expect("amount=0 must deserialize");
        let receipt = cmd.receipt().unwrap();
        assert_eq!(receipt.payments[0].amount, 0);
    }

    /// Negative amounts are accepted at this layer — domain validation is downstream
    /// (DPS returns ERROR_XML on fiscally invalid amounts).
    #[test]
    fn payment_amount_negative_accepted_no_domain_validation_at_deserialize() {
        let json = serde_json::json!({
            "schema_version":"1.0","request_id":"r","idempotency_key":"k",
            "operation_type":"SELL","fiscal_number":"fn","business_ts":"ts",
            "payload_sha256":"x",
            "payload":{"receipt":{"payments":[{"type":"CASH","amount":-5000}]}}
        });
        let cmd: CanonicalCommand = serde_json::from_value(json)
            .expect("negative amount must deserialize (domain validation is downstream)");
        let receipt = cmd.receipt().unwrap();
        assert_eq!(receipt.payments[0].amount, -5000);
    }

    /// Missing required `payload` field must fail deserialization.
    #[test]
    fn missing_payload_field_fails_deserialization() {
        let json = serde_json::json!({
            "schema_version":"1.0","request_id":"r","idempotency_key":"k",
            "operation_type":"SELL","fiscal_number":"fn","business_ts":"ts",
            "payload_sha256":"x"
            // no "payload" key
        });
        let result = serde_json::from_value::<CanonicalCommand>(json);
        assert!(
            result.is_err(),
            "missing payload field must fail deserialization"
        );
    }

    /// Receipt with empty goods/payments/discounts arrays must deserialize correctly.
    #[test]
    fn receipt_with_empty_goods_and_payments_deserializes() {
        let json = serde_json::json!({
            "schema_version":"1.0","request_id":"r","idempotency_key":"k",
            "operation_type":"SELL","fiscal_number":"fn","business_ts":"ts",
            "payload_sha256":"x",
            "payload":{"receipt":{"goods":[],"payments":[],"discounts":[]}}
        });
        let cmd: CanonicalCommand =
            serde_json::from_value(json).expect("empty arrays must deserialize");
        let receipt = cmd.receipt().unwrap();
        assert!(receipt.goods.is_empty(), "goods must be empty");
        assert!(receipt.payments.is_empty(), "payments must be empty");
        assert!(receipt.discounts.is_empty(), "discounts must be empty");
    }

    /// RRN as a JSON number (not string) must return false from has_card_rrn.
    /// The method uses `.as_str()` on the raw JSON value — a number returns None.
    #[test]
    fn has_card_rrn_with_numeric_rrn_value_returns_false() {
        let json = serde_json::json!({
            "schema_version":"1.0","request_id":"r","idempotency_key":"k",
            "operation_type":"SELL","fiscal_number":"fn","business_ts":"ts",
            "payload_sha256":"x",
            "payload":{"receipt":{"payments":[{"type":"CARD","amount":100,"rrn":123456}]}}
        });
        // payload is raw serde_json::Value — deserializes fine even with numeric rrn
        let cmd: CanonicalCommand = serde_json::from_value(json)
            .expect("numeric rrn must deserialize at the envelope level");
        assert!(
            !cmd.has_card_rrn(),
            "numeric RRN value (not string) must return false — .as_str() returns None"
        );
    }

    /// operation_type must be SCREAMING_SNAKE_CASE; lowercase must fail.
    #[test]
    fn lowercase_operation_type_fails_deserialization() {
        let json = serde_json::json!({
            "schema_version":"1.0","request_id":"r","idempotency_key":"k",
            "operation_type":"sell",   // lowercase — must not match
            "fiscal_number":"fn","business_ts":"ts",
            "payload_sha256":"x","payload":{}
        });
        let result = serde_json::from_value::<CanonicalCommand>(json);
        assert!(
            result.is_err(),
            "lowercase operation_type must fail (SCREAMING_SNAKE_CASE required)"
        );
    }

    /// Good struct: only required fields present → all optional fields are None/empty.
    /// Required: name, price, quantity, sum. Optional: code, barcode, uktzed, tax_id,
    /// tax_id_2, excise_barcodes (default []), discounts (default []).
    #[test]
    fn good_required_fields_only_deserializes_with_correct_defaults() {
        let json = r#"{"name":"Хліб","price":2500,"quantity":2000,"sum":5000}"#;
        let g: Good = serde_json::from_str(json).unwrap();
        assert_eq!(g.name, "Хліб");
        assert_eq!(g.price, 2500);
        assert_eq!(g.quantity, 2000);
        assert_eq!(g.sum, 5000);
        assert!(g.code.is_none(), "code must default to None");
        assert!(g.barcode.is_none(), "barcode must default to None");
        assert!(g.uktzed.is_none(), "uktzed must default to None");
        assert!(g.tax_id.is_none(), "tax_id must default to None");
        assert!(g.tax_id_2.is_none(), "tax_id_2 must default to None");
        assert!(
            g.excise_barcodes.is_empty(),
            "excise_barcodes must default to []"
        );
        assert!(g.discounts.is_empty(), "discounts must default to []");
    }

    /// Z_REPORT deserialization succeeds even with no sub-fields.
    #[test]
    fn z_report_data_all_fields_none_when_payload_absent() {
        let json = serde_json::json!({
            "schema_version":"1.0","request_id":"r","idempotency_key":"k",
            "operation_type":"Z_REPORT","fiscal_number":"fn","business_ts":"ts",
            "payload_sha256":"x","payload":{}
        });
        let cmd: CanonicalCommand = serde_json::from_value(json).unwrap();
        assert!(
            cmd.z_report_data().is_none(),
            "missing z_report_data → None"
        );
    }

    /// Missing fiscal_number field must fail deserialization.
    #[test]
    fn missing_fiscal_number_fails_deserialization() {
        let json = serde_json::json!({
            "schema_version":"1.0","request_id":"r","idempotency_key":"k",
            "operation_type":"SELL","business_ts":"ts",
            "payload_sha256":"x","payload":{}
            // fiscal_number absent
        });
        let result = serde_json::from_value::<CanonicalCommand>(json);
        assert!(
            result.is_err(),
            "missing fiscal_number must fail deserialization"
        );
    }
}
