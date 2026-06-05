//! RS-2 piece-2a — wire→signer payload conversion.
//!
//! Closes the `dto.rs` "Known scope gap": [`to_canonical_fiscal_command`]
//! populates `payload_json` with the **wire** `ReceiptPayload` shape, but
//! `services/write_path/stage_sign::parse_payload` (`deny_unknown_fields`)
//! expects a DIFFERENT internal shape (`CheckJson`/`ShiftOpenJson`/
//! `ZReportJson`).  This module produces the signer-ready shape and
//! recomputes `payload_sha256_canonical` over the CONVERTED payload
//! (RS-2 §0.4 H5 — `stage_acquire`/`stage_sign`/drift-checks all consume
//! the converted JSON, so the persisted hash must be over it).
//!
//! Field mapping is operator-locked (2026-06-05, see plan §0.4 / the
//! `project-rs2-convert-mapping` note):
//!   - `code` ← `article_code.to_string()`; `None` → typed error (no
//!     line-index fallback — that substitutes the product code).
//!   - `sum_kop` = `price_kopecks * quantity_milli / 1000` via **checked**
//!     arithmetic; a non-/1000-divisible product is a typed error (NO
//!     silent floor — rounding policy is deferred).
//!   - `quantity_thousandths` ← `quantity_milli` (1:1).
//!   - `tax_group_1` ← raw wire `u8` (faithful pass-through; `stage_sign`
//!     does the driver→canonical TX translation; `0` = звільнено valid).
//!   - `tax_group_2` (secondary `TX1=`) ← 3-way FAIL-CLOSED matrix:
//!     dual-tax active → emit raw; no dual + `0` → omit; no dual +
//!     non-zero → typed `SecondaryTaxRequiresDualTaxMode` (NOT a silent
//!     drop). Do NOT revert to an unconditional pass-through.
//!   - payments (D1 frozen slots): kind → candidate `pay_index`
//!     (Cash=1, Cashless1=2, …); `type_code = pay_index-1`; `name` from
//!     the per-FN `payment_methods` row; missing / inactive / `iscash`
//!     mismatch → typed error (no fallback).
//!   - `discount` → at most one `adjustment` (zero/absent omitted).
//!   - `ShiftOpenJson.opening_sum_kop` = 0 (no wire source).
//!
//! `ShiftClose` / `ZReport` need the ledger (sell/return counts +
//! per-form sums since shift open) and land in **piece-2b**; here they
//! return [`ConvertError::ZReportDeferredToPiece2b`].
//!
//! [`to_canonical_fiscal_command`]: super::dto::to_canonical_fiscal_command

use super::dto::{
    canonical_json_bytes, CanonicalCommand, CanonicalPayment, CommandType, DiscountDirection,
    FiscalLine, PaymentKind,
};
use crate::db::repositories::payment_methods;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use thiserror::Error;

/// The signer-ready payload + its canonical hash (over the CONVERTED
/// shape, §0.4 H5).  Replaces the wire-shape `payload_json` /
/// `payload_sha256_canonical` that `to_canonical_fiscal_command`
/// produced, before the inbox insert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvertedPayload {
    pub payload_json: String,
    pub payload_sha256_canonical: [u8; 32],
}

#[derive(Debug, Error)]
pub enum ConvertError {
    /// `article_code` absent — the signer's `code` is required and we do
    /// NOT fabricate one (operator decision: no line-index fallback).
    #[error("item {item_name:?}: missing article_code (signer 'code' is required; no fallback)")]
    MissingItemCode { item_name: String },

    /// `price_kopecks * quantity_milli` is not divisible by 1000 — we do
    /// NOT silently floor (operator decision: typed error until a
    /// rounding policy is agreed).
    #[error(
        "item {item_name:?}: price_kopecks {price_kopecks} * quantity_milli {quantity_milli} \
         is not divisible by 1000 (no silent floor — rounding policy deferred)"
    )]
    SumNotDivisible {
        item_name: String,
        price_kopecks: u64,
        quantity_milli: u64,
    },

    /// The wire carries a non-zero secondary tax group on an item but
    /// `dual_tax_mode` is absent — fail-closed rather than silently drop
    /// the secondary tax (which would sign a single-tax payload).
    #[error(
        "item {item_name:?}: secondary tax_group_2 {tax_group_2} present without dual_tax_mode — \
         secondary tax requires dual-tax mode (fail-closed, not dropped)"
    )]
    SecondaryTaxRequiresDualTaxMode { item_name: String, tax_group_2: u8 },

    /// A `u64` wire amount overflows the signer's `i64` field.
    #[error("value {value} overflows i64 (context: {context})")]
    ValueOverflow { value: u64, context: &'static str },

    /// No `payment_methods` row for the candidate `(fn, pay_index)`.
    #[error("fn {fiscal_number}: no payment method at pay_index {pay_index} (D1 frozen slots)")]
    MissingPaymentMethod {
        fiscal_number: String,
        pay_index: i64,
    },

    /// The candidate slot exists but is inactive.
    #[error("fn {fiscal_number}: payment method at pay_index {pay_index} is inactive")]
    InactivePaymentMethod {
        fiscal_number: String,
        pay_index: i64,
    },

    /// The candidate slot's `iscash` disagrees with the wire payment kind
    /// — the FN has a non-standard pay-form layout RS-2 does not support
    /// under the D1 frozen-slot pilot policy (CF2).
    #[error(
        "fn {fiscal_number}: pay_index {pay_index} iscash={slot_iscash} disagrees with wire kind \
         (cash={kind_is_cash}) — non-standard pay-form layout (D1 frozen slots)"
    )]
    PaymentSlotKindMismatch {
        fiscal_number: String,
        pay_index: i64,
        slot_iscash: bool,
        kind_is_cash: bool,
    },

    /// The payment carries an `acquirer_slip` (EPZ requisites) whose
    /// `AcquirerSlip → EPZ` attribute mapping is an open spec question
    /// (W4-Z1 wire-shape §Q1).  Fail-closed rather than silently drop
    /// the slip (which would also defeat the converted-payload hash).
    #[error(
        "fn {fiscal_number}: payment at pay_index {pay_index} carries an acquirer_slip; the \
         AcquirerSlip→EPZ mapping is an open spec question (W4-Z1 §Q1) — fail-closed, not dropped"
    )]
    AcquirerSlipMappingDeferred {
        fiscal_number: String,
        pay_index: i64,
    },

    /// `ShiftClose` / `ZReport` need the ledger aggregation — piece-2b.
    #[error("ZReport/ShiftClose conversion is deferred to RS-2 piece-2b")]
    ZReportDeferredToPiece2b,

    /// A non-signable command reached the converter (the ingress
    /// command policy should reject these earlier; defensive).
    #[error("command_type {0:?} is not signable and must not reach convert")]
    NotSignable(CommandType),

    #[error("payment_methods lookup failed: {0}")]
    PaymentLookup(#[from] payment_methods::PaymentMethodsRepoError),

    #[error("canonical JSON serialisation failed: {0}")]
    Serialise(#[from] serde_json::Error),
}

// ─── Signer-shape output structs (mirror stage_sign's private types) ──
// Field names + `skip_serializing_if` chosen so the SORTED output of
// `canonical_json_bytes` parses through `parse_payload`
// (`deny_unknown_fields`) — proven by the test-support validator.

#[derive(Serialize)]
struct ShiftOpenOut {
    opening_sum_kop: i64,
}

#[derive(Serialize)]
struct CheckOut {
    items: Vec<CheckItemOut>,
    payments: Vec<CheckPaymentOut>,
}

#[derive(Serialize)]
struct CheckItemOut {
    code: String,
    name: String,
    price_kop: i64,
    quantity_thousandths: i64,
    sum_kop: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    barcode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    uktzed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tax_group_1: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tax_group_2: Option<i64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    excise_stamps: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    adjustments: Vec<LineAdjustmentOut>,
}

#[derive(Serialize)]
struct LineAdjustmentOut {
    kind: &'static str, // "discount" | "surcharge"
    sum: i64,
    mode: &'static str, // "value"
}

#[derive(Serialize)]
struct CheckPaymentOut {
    name: String,
    sum_kop: i64,
    type_code: String,
}

fn to_i64(value: u64, context: &'static str) -> Result<i64, ConvertError> {
    i64::try_from(value).map_err(|_| ConvertError::ValueOverflow { value, context })
}

/// `sum_kop = price_kopecks * quantity_milli / 1000`, checked + no silent
/// floor (operator guard).
fn item_sum_kop(line: &FiscalLine) -> Result<i64, ConvertError> {
    let product =
        line.price_kopecks
            .checked_mul(line.quantity_milli)
            .ok_or(ConvertError::ValueOverflow {
                value: line.price_kopecks,
                context: "price_kopecks * quantity_milli",
            })?;
    if product % 1000 != 0 {
        return Err(ConvertError::SumNotDivisible {
            item_name: line.name.clone(),
            price_kopecks: line.price_kopecks,
            quantity_milli: line.quantity_milli,
        });
    }
    to_i64(product / 1000, "sum_kop")
}

fn convert_item(line: &FiscalLine, dual_tax_active: bool) -> Result<CheckItemOut, ConvertError> {
    let code = match line.article_code {
        Some(x) => x.to_string(),
        None => {
            return Err(ConvertError::MissingItemCode {
                item_name: line.name.clone(),
            })
        }
    };

    let adjustments = match &line.discount {
        Some(d) if d.amount_kopecks != 0 => {
            let kind = match d.direction {
                DiscountDirection::Discount => "discount",
                DiscountDirection::Markup => "surcharge",
            };
            vec![LineAdjustmentOut {
                kind,
                sum: to_i64(d.amount_kopecks, "discount.amount_kopecks")?,
                mode: "value",
            }]
        }
        // None or zero-amount → omit (no hash-noise; operator decision).
        _ => Vec::new(),
    };

    // Secondary tax (`TX1=`) — 3-way FAIL-CLOSED matrix (spec
    // `2026-05-26-w4-z1-dps-xml-wire-shape.md` §`TX1=`: "when
    // dual_tax_mode = Some"):
    //   - dual active       → emit raw (incl. 0, valid under dual-tax);
    //   - no dual, tg2 == 0  → omit `TX1` (ordinary single-tax check);
    //   - no dual, tg2 != 0  → typed error: secondary tax data without
    //     dual-tax mode must NOT be silently dropped (that would sign a
    //     single-tax payload and lose fiscal tax data — a fail-open).
    let tax_group_2 = if dual_tax_active {
        Some(i64::from(line.tax_group_2))
    } else if line.tax_group_2 != 0 {
        return Err(ConvertError::SecondaryTaxRequiresDualTaxMode {
            item_name: line.name.clone(),
            tax_group_2: line.tax_group_2,
        });
    } else {
        None
    };

    Ok(CheckItemOut {
        code,
        name: line.name.clone(),
        price_kop: to_i64(line.price_kopecks, "price_kopecks")?,
        quantity_thousandths: to_i64(line.quantity_milli, "quantity_milli")?,
        sum_kop: item_sum_kop(line)?,
        barcode: line.barcode.clone(),
        uktzed: line.uktzed.clone(),
        // Primary tax (`TX=`) always present; raw pass-through —
        // stage_sign does the driver→canonical TX translation, and `0`
        // is a valid group (звільнено).
        tax_group_1: Some(i64::from(line.tax_group_1)),
        tax_group_2,
        excise_stamps: line.excise_stamps.clone(),
        adjustments,
    })
}

/// Cash=1, Cashless1=2, Cashless2=3, Cashless3=4 (D1 frozen slots).
/// Returns `(pay_index, kind_is_cash)`.
fn payment_slot(kind: PaymentKind) -> (i64, bool) {
    match kind {
        PaymentKind::Cash => (1, true),
        PaymentKind::Cashless1 => (2, false),
        PaymentKind::Cashless2 => (3, false),
        PaymentKind::Cashless3 => (4, false),
    }
}

async fn convert_payment(
    p: &CanonicalPayment,
    fiscal_number: &str,
    secure_pool: &SqlitePool,
) -> Result<CheckPaymentOut, ConvertError> {
    let (pay_index, kind_is_cash) = payment_slot(p.kind);
    let row = payment_methods::find(secure_pool, fiscal_number, pay_index)
        .await?
        .ok_or_else(|| ConvertError::MissingPaymentMethod {
            fiscal_number: fiscal_number.to_string(),
            pay_index,
        })?;
    if !row.is_active {
        return Err(ConvertError::InactivePaymentMethod {
            fiscal_number: fiscal_number.to_string(),
            pay_index,
        });
    }
    if row.iscash != kind_is_cash {
        return Err(ConvertError::PaymentSlotKindMismatch {
            fiscal_number: fiscal_number.to_string(),
            pay_index,
            slot_iscash: row.iscash,
            kind_is_cash,
        });
    }
    // A card/acquirer slip carries EPZ requisites (terminal/RRN/PAN/
    // payment-system). The signer's CheckPaymentJson supports the EPZ
    // attrs (PA/PB/…), but the `AcquirerSlip → EPZ` field correspondence
    // is an OPEN spec question (W4-Z1 wire-shape §Q1: "PA source —
    // mapping ambiguity, defer; needs operator clarification"). Until it
    // is approved we FAIL CLOSED rather than silently drop the slip data
    // (which would also collapse two slip-distinct wire payloads to the
    // same converted hash). EPZ mapping is a tracked follow-up.
    if p.acquirer_slip.is_some() {
        return Err(ConvertError::AcquirerSlipMappingDeferred {
            fiscal_number: fiscal_number.to_string(),
            pay_index,
        });
    }
    Ok(CheckPaymentOut {
        name: row.name,
        sum_kop: to_i64(p.amount_kopecks, "payment.amount_kopecks")?,
        type_code: (pay_index - 1).to_string(),
    })
}

fn finalize<T: Serialize>(out: &T) -> Result<ConvertedPayload, ConvertError> {
    let bytes = canonical_json_bytes(out)?;
    let payload_sha256_canonical: [u8; 32] = Sha256::digest(&bytes).into();
    let payload_json =
        String::from_utf8(bytes).expect("canonical JSON is always valid UTF-8 (serde_json output)");
    Ok(ConvertedPayload {
        payload_json,
        payload_sha256_canonical,
    })
}

/// Convert a wire [`CanonicalCommand`] into the signer-ready payload +
/// recomputed canonical hash.  `secure_pool` is the secure DB pool that
/// holds `payment_methods`.  `ShiftClose`/`ZReport` are deferred to
/// piece-2b (typed error, never a silent wrong payload).
pub async fn convert_to_signer_payload(
    cmd: &CanonicalCommand,
    fiscal_number: &str,
    secure_pool: &SqlitePool,
) -> Result<ConvertedPayload, ConvertError> {
    match cmd.command_type {
        CommandType::ShiftOpen => finalize(&ShiftOpenOut { opening_sum_kop: 0 }),
        CommandType::Sell | CommandType::Return => {
            let dual_tax_active = cmd.payload.dual_tax_mode.is_some();
            let items = cmd
                .payload
                .goods
                .iter()
                .map(|l| convert_item(l, dual_tax_active))
                .collect::<Result<Vec<_>, _>>()?;
            let mut payments = Vec::with_capacity(cmd.payload.payments.len());
            for p in &cmd.payload.payments {
                payments.push(convert_payment(p, fiscal_number, secure_pool).await?);
            }
            finalize(&CheckOut { items, payments })
        }
        CommandType::ShiftClose | CommandType::ZReport => {
            Err(ConvertError::ZReportDeferredToPiece2b)
        }
        other => Err(ConvertError::NotSignable(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::enums::DocType;
    use crate::services::write_path::stage_sign::{
        derive_wire_artifact_kind, validate_signer_payload_shape_for_testing,
    };

    fn line(
        article_code: Option<u64>,
        name: &str,
        price_kopecks: u64,
        quantity_milli: u64,
    ) -> FiscalLine {
        FiscalLine {
            name: name.to_string(),
            uktzed: None,
            quantity_milli,
            price_kopecks,
            tax_group_1: 1,
            tax_group_2: 0,
            article_code,
            discount: None,
            excise_stamps: Vec::new(),
            barcode: None,
        }
    }

    #[test]
    fn item_sum_kop_checked_and_no_silent_floor() {
        // 150.00 * 2.000 = 300.00 → 30000 kop.
        let ok = line(Some(7), "X", 15000, 2000);
        assert_eq!(item_sum_kop(&ok).unwrap(), 30000);

        // price 100 kop * 1 milli = 100; 100 % 1000 != 0 → typed error.
        let bad = line(Some(7), "X", 100, 1);
        assert!(matches!(
            item_sum_kop(&bad),
            Err(ConvertError::SumNotDivisible { .. })
        ));
    }

    #[test]
    fn item_missing_article_code_is_typed_error() {
        let l = line(None, "NoCode", 15000, 1000);
        assert!(matches!(
            convert_item(&l, false),
            Err(ConvertError::MissingItemCode { .. })
        ));
    }

    #[test]
    fn item_maps_code_quantity_tax_and_omits_zero_discount() {
        let l = line(Some(42), "Bread", 15000, 1000);
        let out = convert_item(&l, false).unwrap();
        assert_eq!(out.code, "42");
        assert_eq!(out.price_kop, 15000);
        assert_eq!(out.quantity_thousandths, 1000);
        assert_eq!(out.sum_kop, 15000);
        assert_eq!(out.tax_group_1, Some(1));
        // Single-tax receipt (no dual_tax_mode) → no secondary TX1.
        assert_eq!(out.tax_group_2, None, "single-tax must NOT emit TX1");
        assert!(
            out.adjustments.is_empty(),
            "absent discount → no adjustment"
        );
    }

    #[test]
    fn secondary_tax_is_fail_closed_three_way_matrix() {
        let mut l = line(Some(42), "Bread", 15000, 1000);

        // (1) dual active → emit raw secondary group (incl. 0).
        l.tax_group_2 = 3;
        assert_eq!(convert_item(&l, true).unwrap().tax_group_2, Some(3));
        l.tax_group_2 = 0;
        assert_eq!(convert_item(&l, true).unwrap().tax_group_2, Some(0));

        // (2) no dual + tg2 == 0 → omit TX1 (ordinary single-tax check).
        assert_eq!(convert_item(&l, false).unwrap().tax_group_2, None);

        // (3) no dual + tg2 != 0 → typed error (NOT silently dropped).
        l.tax_group_2 = 3;
        assert!(matches!(
            convert_item(&l, false),
            Err(ConvertError::SecondaryTaxRequiresDualTaxMode { tax_group_2: 3, .. })
        ));
    }

    #[test]
    fn item_nonzero_discount_maps_to_single_value_adjustment() {
        let mut l = line(Some(42), "Bread", 15000, 1000);
        l.discount = Some(super::super::dto::Discount {
            direction: DiscountDirection::Discount,
            name: "promo".to_string(),
            amount_kopecks: 500,
        });
        let out = convert_item(&l, false).unwrap();
        assert_eq!(out.adjustments.len(), 1);
        assert_eq!(out.adjustments[0].kind, "discount");
        assert_eq!(out.adjustments[0].sum, 500);
        assert_eq!(out.adjustments[0].mode, "value");

        // Markup → surcharge.
        l.discount = Some(super::super::dto::Discount {
            direction: DiscountDirection::Markup,
            name: "fee".to_string(),
            amount_kopecks: 300,
        });
        assert_eq!(
            convert_item(&l, false).unwrap().adjustments[0].kind,
            "surcharge"
        );

        // Zero-amount → omitted (no hash-noise).
        l.discount = Some(super::super::dto::Discount {
            direction: DiscountDirection::Discount,
            name: "zero".to_string(),
            amount_kopecks: 0,
        });
        assert!(convert_item(&l, false).unwrap().adjustments.is_empty());
    }

    #[test]
    fn payment_slot_indices_are_frozen() {
        assert_eq!(payment_slot(PaymentKind::Cash), (1, true));
        assert_eq!(payment_slot(PaymentKind::Cashless1), (2, false));
        assert_eq!(payment_slot(PaymentKind::Cashless2), (3, false));
        assert_eq!(payment_slot(PaymentKind::Cashless3), (4, false));
    }

    /// A converted SELL CheckJson (items only, no payments) must parse
    /// through the signer's private `parse_payload` (deny_unknown_fields)
    /// via the test-support validator — proves shape parity end-to-end.
    #[test]
    fn converted_check_items_parse_through_signer() {
        let out = CheckOut {
            items: vec![convert_item(&line(Some(42), "Bread", 15000, 1000), false).unwrap()],
            payments: Vec::new(),
        };
        let conv = finalize(&out).unwrap();
        let kind = derive_wire_artifact_kind(DocType::Sell).unwrap();
        validate_signer_payload_shape_for_testing(kind, &conv.payload_json, Some(15000))
            .expect("converted CheckJson must parse through stage_sign");
    }

    /// A converted ShiftOpen `{opening_sum_kop:0}` parses through the
    /// signer.
    #[test]
    fn converted_shift_open_parses_through_signer() {
        let conv = finalize(&ShiftOpenOut { opening_sum_kop: 0 }).unwrap();
        let kind = derive_wire_artifact_kind(DocType::ShiftOpen).unwrap();
        validate_signer_payload_shape_for_testing(kind, &conv.payload_json, None)
            .expect("converted ShiftOpenJson must parse through stage_sign");
        assert_eq!(conv.payload_json, r#"{"opening_sum_kop":0}"#);
    }
}
