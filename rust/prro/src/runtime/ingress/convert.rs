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
//! `ShiftClose` / `ZReport` (piece-2b) derive their summary from the
//! ledger: [`aggregate_zreport`] over the current shift's issued
//! (`ACK` / `OFFLINE_LOCAL_ACK`) `SELL` / `RETURN` receipts read from
//! `main_pool` (`fiscal_documents` + `node_state.current_shift_id`),
//! grouped by `(type_code, name)` — SELL→`sum_in_kop`, RETURN→`sum_out_kop`.
//!
//! [`to_canonical_fiscal_command`]: super::dto::to_canonical_fiscal_command

use super::dto::{
    canonical_json_bytes, CanonicalCommand, CanonicalPayment, CommandType, DiscountDirection,
    FiscalLine, PaymentKind,
};
use crate::db::models::enums::DocType;
use crate::db::models::ids::ShiftId;
use crate::db::repositories::{
    fiscal_documents, node_state, payment_methods, signing_config_snapshots,
};
use crate::db::types::DbShiftId;
use crate::services::write_path::tax_summary::{ResolvedTaxGroup, TaxResolutionSnapshot};
use crate::xml::{calc_tax, CalcTaxError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::collections::{BTreeMap, HashMap};
use thiserror::Error;

/// The signer-ready payload + its canonical hash (over the CONVERTED
/// shape, §0.4 H5).  Replaces the wire-shape `payload_json` /
/// `payload_sha256_canonical` that `to_canonical_fiscal_command`
/// produced, before the inbox insert.
///
/// **Idempotency note (review MEDIUM-2, DECIDED — convert→insert is wired
/// at piece-5a `handler.rs`):** the hash is over the CONVERTED payload, and
/// `CheckPaymentJson.name` is sourced from the editable `payment_methods`
/// row, so the inbox replay/conflict key depends on that name.  An operator
/// renaming a payment slot between a POS submit and its retry flips a
/// legitimate retry from `Replay` to `Conflict`.  Accepted decision: keep
/// the honest converted-payload hash (the drift checks in
/// `stage_acquire`/`boot_phase` require the hash to be over what was
/// persisted), label such a conflict `config_drift: true`, and audit it —
/// the client re-submits under a fresh idempotency_key.  Freezing the slot
/// *name* under the D1 admin-guard was NOT chosen (the D1 guard freezes slot
/// KIND, not name — name churn is benign for the pilot).
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

    /// A SELL/RETURN with no goods — a fiscal receipt must have ≥1 item.
    #[error("SELL/RETURN with empty goods — a fiscal receipt must carry at least one item")]
    EmptyGoods,

    /// A line with `quantity_milli == 0` — fiscally meaningless; fail
    /// closed rather than sign a zero-quantity line.
    #[error("item {item_name:?}: quantity_milli is 0 (zero-quantity line is not fiscalizable)")]
    ZeroQuantityLine { item_name: String },

    /// The wire carries non-empty `raw_frames` on a Signable command.
    /// `raw_frames` is the M5 carrier for check-level discounts / header
    /// & footer text / service-movement amounts that the structured DTO
    /// does not capture (spec `2026-05-26-w4-z1-dps-xml-wire-shape.md`).
    /// Until that mapping lands we FAIL CLOSED — same posture as
    /// `acquirer_slip` — rather than silently sign away the frames (which
    /// would also collapse frame-distinct wire payloads to one hash).
    #[error(
        "Signable command carries {count} raw_frame(s); the raw_frames→signer mapping is not yet \
         implemented (M5) — fail-closed, not silently dropped"
    )]
    RawFramesNotSupported { count: usize },

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

    /// The command carries a non-null `return_check_number` (the original
    /// receipt a RETURN references).  The compact `<C T=>` wire dialect the
    /// gateway ships does NOT carry ORDERRETNUM — neither the 4-year Python
    /// production serializer nor the WebCheck capture emit it (the verbose
    /// `check01.xsd` has it optional, but that format is not what we send;
    /// corroborated by the RT-3 red-team adjudication in
    /// `docs/reviews/redteam-2026-06-12-adjudication.md`).
    /// So there is no wire slot to honor the field; we FAIL CLOSED — same
    /// posture as `raw_frames` / `acquirer_slip` — rather than silently drop
    /// it (fail-open), which would leave the client falsely believing the
    /// return is linked to the original until a tax audit.  Emitting it is a
    /// FUTURE live-verified enhancement (verbose format / cashback class), NOT
    /// data loss — no deployed client depends on drop semantics, so the typed
    /// 422 is cleanly reversible into an emit path if DPS/law later requires
    /// the link.
    #[error(
        "command carries a return_check_number; the compact <C T=> dialect does not carry \
         ORDERRETNUM — fail-closed (a future live-verified enhancement), not silently dropped"
    )]
    ReturnCheckNumberNotSupported,

    /// **INV-21** — RETURN would drive cash-on-hand below zero.
    /// Fail-closed before inbox insert (row-less), HTTP 422, code CASH_INSUFFICIENT.
    ///
    /// `cash_on_hand_kop` = current cash-on-hand for the FN's open shift;
    /// `return_cash_kop`  = cash leg of the RETURN being attempted.
    #[error(
        "INV-21: RETURN would drive cash below zero — \
         cash_on_hand {cash_on_hand_kop} kop < return {return_cash_kop} kop (fail-closed)"
    )]
    CashInsufficient {
        cash_on_hand_kop: i64,
        return_cash_kop: i64,
    },

    /// EPZ (видача готівки за ЕПЗ) — the card payment-form index is `< 2`.
    /// EPZ is a CARD operation (WebCheck `ClassFiscal.cs:1377` — «Тип
    /// paymentid не может быть меньше 2», errCode 94); a cash form (index 1)
    /// is not a valid EPZ leg.  Fail-closed at ingress (HTTP 422).
    #[error(
        "EPZ: paymentid {payment_form_index} < 2 — EPZ requires a card payment form \
         (errCode-94 analog; cash forms are not EPZ legs)"
    )]
    EpzPaymentIdTooLow { payment_form_index: u8 },

    /// EPZ with a missing / malformed card leg — an EPZ must carry EXACTLY one
    /// card `CanonicalPayment` (with an `acquirer_slip`) whose amount is the
    /// cash-out sum.  Fail-closed (HTTP 422).
    #[error("EPZ: expected exactly one card payment leg with an acquirer_slip; got {count}")]
    EpzMalformedCardLeg { count: usize },

    /// **L5 G1** — the SELL's CASH portion (`type_code == "0"` legs) exceeds the
    /// legal cash cap (49 999.99 UAH = 4 999 999 kop; WebCheck `DopNal`/
    /// `AllowableCash` clamp, `All.cs:875-886`).  Caps the CASH leg sum, NOT the
    /// receipt total (a card-heavy receipt may exceed 50 000 legally).
    /// Fail-closed pre-inbox (row-less, HTTP 422).
    #[error(
        "L5 G1: cash legs Σ {cash_kop} kop exceed the cash cap {cap_kop} kop \
         (49 999.99 UAH) — fail-closed pre-inbox"
    )]
    CashCapExceeded { cash_kop: i64, cap_kop: i64 },

    /// **L5 G2** — a good resolves to `item_sum_kop == 0` (a zero-price line).
    /// Distinct from `ZeroQuantityLine` (quantity may be non-zero here — a
    /// zero-PRICE good).  A fiscal line must carry a positive amount.
    /// Fail-closed pre-inbox (row-less, HTTP 422).
    #[error("L5 G2: item {item_name:?} has a zero line sum (zero-price line is not fiscalizable)")]
    ZeroPriceLine { item_name: String },

    /// **L5 G3** — a declared payment leg carries `sum_kop == 0`.  A zero-value
    /// payment is malformed input (a real payment leg has a positive amount).
    /// Fail-closed pre-inbox (row-less, HTTP 422).
    #[error("L5 G3: payment leg #{pay_index} has a zero sum (zero-value payment is not valid)")]
    ZeroPaymentAmount { pay_index: usize },

    /// **L5 G4** — a SELL whose declared payments (when present) sum to LESS than
    /// its goods total (an underpaid receipt).  SELL-only (a RETURN is a refund;
    /// underpayment semantics do not apply).  Fires only when ≥1 payment leg is
    /// present — a SELL with NO payment legs is the pre-existing "cash implied"
    /// shape convert already tolerates.  Fail-closed pre-inbox (row-less,
    /// HTTP 422); `stage_sign`'s later total cross-check is defense-in-depth.
    #[error(
        "L5 G4: SELL paid {paid_kop} kop < goods {goods_kop} kop (underpayment) — fail-closed"
    )]
    UnderpaymentRefused { goods_kop: i64, paid_kop: i64 },

    /// `ZReport` / `ShiftClose` with no open shift — a Z closes the open
    /// shift; closing when none is open is a state-machine breach.
    #[error(
        "fn {fiscal_number}: ZReport/ShiftClose with no open shift \
         (no node_state row, or current_shift_id is None)"
    )]
    NoOpenShiftForZReport { fiscal_number: String },

    /// A stored issued-receipt payment carries a negative `sum_kop`.
    /// Impossible on the normal pipeline (piece-2a maps from a `u64`
    /// `amount_kopecks`), so this signals ledger corruption — halt the Z
    /// rather than emit a negative-turnover fiscal report.
    #[error(
        "negative stored payment sum {sum_kop} (type_code {type_code}, name {name:?}) \
         — ledger corruption"
    )]
    NegativeStoredPaymentSum {
        type_code: String,
        name: String,
        sum_kop: i64,
    },

    /// A Z-report per-payment-form turnover sum overflows i64.
    #[error("ZReport turnover sum overflow (type_code {type_code}, name {name:?})")]
    ZReportSumOverflow { type_code: String, name: String },

    /// A shift-receipt ledger row had an unexpected doc_type — the query
    /// filters to SELL/RETURN, so this is defensive (should not occur).
    #[error("unexpected shift-receipt doc_type {0:?} (expected SELL/RETURN)")]
    UnexpectedShiftReceiptDocType(DocType),

    /// A read-side DB error (node_state / ledger) while building a Z.
    #[error("ZReport ledger read failed: {0}")]
    LedgerRead(sqlx::Error),

    /// A non-signable command reached the converter (the ingress
    /// command policy should reject these earlier; defensive).
    #[error("command_type {0:?} is not signable and must not reach convert")]
    NotSignable(CommandType),

    #[error("payment_methods lookup failed: {0}")]
    PaymentLookup(#[from] payment_methods::PaymentMethodsRepoError),

    #[error("canonical JSON serialisation failed: {0}")]
    Serialise(#[from] serde_json::Error),

    /// A Z-report per-tax-group turnover sum overflows i64 (W4-Z2 TXS).
    #[error("ZReport TXS turnover overflow (tax group {tx})")]
    ZReportTaxSumOverflow { tx: i64 },

    /// The SAME canonical tax group resolved to CONFLICTING rates across the
    /// shift's receipts (a mid-shift tax-config change) — the Z would mix rate
    /// regimes.  Fail-closed → manual reconciliation (never auto-emit an
    /// ambiguous Z).  Rare per operator empirics (config changes between shifts).
    #[error(
        "ZReport TXS: canonical tax group {tx} resolved to conflicting rates across shift receipts \
         (mid-shift tax-config drift) — fail-closed to manual reconciliation"
    )]
    TaxSnapshotDriftInShift { tx: i64 },

    /// `calc_tax` failed while computing a Z TXS tax sum (unsupported algorithm
    /// / non-finite rate).  Fail-closed rather than sign a wrong tax total.
    #[error("ZReport TXS tax calculation failed: {0}")]
    TaxCalc(#[source] CalcTaxError),

    /// A receipt's pinned `signing_config_snapshots` row could not be loaded
    /// (missing / checksum mismatch / unsupported kind) while aggregating TXS.
    #[error("ZReport TXS: failed to load signing_config_snapshot id={id}: {detail}")]
    SnapshotLoad { id: i64, detail: String },
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

// ─── ZReport (piece-2b) — ledger-derived shift summary ───────────────

/// L3 — one service-io row in the Z JSON (`<IO NM SMI SMO T="0">`).
#[derive(Serialize)]
struct ZReportServiceIoOut {
    name: String,
    sum_in_kop: i64,
    sum_out_kop: i64,
}

#[derive(Serialize)]
struct ZReportOut {
    payments: Vec<ZReportPaymentOut>,
    // W4-Z2 (PR-Z1) — per-tax-group `<TXS>` turnover.  Empty → OMITTED from
    // the payload (absent-when-empty is the DPS-accepted, spec-sanctioned form
    // per `2026-05-26-w4-z1-dps-xml-wire-shape.md:329`); keeps the pre-W4-Z2
    // payments-only payload byte-identical for the payment-only aggregation
    // unit tests.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tax_summaries: Vec<ZReportTaxSumOut>,
    /// L3 — service cash-in/out `<IO>` rows.  Empty → OMITTED (absent-when-empty
    /// is the DPS-accepted form — a shift with no service ops emits no `<IO>`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    service_sums: Vec<ZReportServiceIoOut>,
    /// EPZ — `<EPZ EPC EPCS='0' EPSM>` card-advance totals.  `None` → OMITTED
    /// (a shift with no EPZ ops emits no `<EPZ>`).  STOP-S2: populated here so a
    /// live Z close reports card-advance turnover in the SAME PR as the ingress
    /// relaxation.
    #[serde(skip_serializing_if = "Option::is_none")]
    epz: Option<ZReportEpzOut>,
    sell_count: u32,
    return_count: u32,
}

/// EPZ Z-section totals (`<EPZ EPC EPCS EPSM>`).  Mirrors `xml::ZReportEpzTotals`.
#[derive(Serialize)]
struct ZReportEpzOut {
    /// `<EPZ EPC=>` — count of EPZ operations in the shift.
    epc: i64,
    /// `<EPZ EPCS=>` — hardcoded 0 (byte-parity, WebCheck `FormDate.cs:436`).
    epcs: i64,
    /// `<EPZ EPSM=>` — total EPZ sum (kopecks).
    epsm: i64,
}

#[derive(Serialize)]
struct ZReportPaymentOut {
    name: String,
    sum_in_kop: i64,
    sum_out_kop: i64,
    type_code: String,
}

/// W4-Z2 (PR-Z1) — one aggregated `<TXS>` tax-group row in the signer JSON.
/// Mirrors `xml::ZReportTaxSummary` MINUS `ts_prefix` (the Z's date is a
/// document property, stamped at `stage_sign::build_canonical_doc` from the
/// header — not carried in the aggregated body).  `tx_short_form=true` leaves
/// the rate fields empty (Python `_build_z_report:457-458` fallback).
#[derive(Serialize)]
struct ZReportTaxSumOut {
    tx: i64,
    tx_short_form: bool,
    txpr: String,
    txal: i64,
    txty: i64,
    dtpr: String,
    smi: i64,
    smo: i64,
    txi: i64,
    txo: i64,
}

/// Minimal view of a stored (converted) `CheckJson` — ONLY the `items` the Z
/// TXS aggregation sums.  NOT `deny_unknown_fields` (ignores `payments` + the
/// rest).  `items` is REQUIRED (piece-2a always emits it): a stored receipt
/// payload that lacks it is corrupt — fail closed rather than silently emit a
/// TXS-less Z (same posture as `StoredCheckPayments`).
#[derive(Deserialize)]
struct StoredCheckItems {
    items: Vec<StoredItem>,
}

#[derive(Deserialize)]
struct StoredItem {
    /// Raw driver-side tax group (`tax_group_1` on the converted item).
    /// Absent → the line carries no tax group → no `<TXS>` contribution.
    #[serde(default)]
    tax_group_1: Option<i64>,
    sum_kop: i64,
}

/// Minimal view of a stored (converted) `CheckJson` — ONLY the payments
/// the Z aggregates.  NOT `deny_unknown_fields`: it deliberately ignores
/// `items` + every other field.  But `payments` is **REQUIRED** (no
/// `#[serde(default)]`): piece-2a always emits a `payments` key, so a
/// stored issued-receipt payload that lacks it is corrupt/wrong-shape —
/// fail closed with a typed parse error rather than silently treat it as
/// zero-turnover (which would underreport the Z).  A parse failure HALTS
/// the Z (deliberately stricter than the Python parity, which silently
/// skips a bad row).
#[derive(Deserialize)]
struct StoredCheckPayments {
    payments: Vec<StoredPayment>,
}

#[derive(Deserialize)]
struct StoredPayment {
    name: String,
    sum_kop: i64,
    type_code: String,
}

/// Aggregate a shift's issued `SELL`/`RETURN` receipts into the signer's
/// ZReport shape.  Parses each stored converted `CheckJson`'s payments,
/// groups by `(type_code, name)` (operator decision; `BTreeMap` →
/// deterministic payment-array order), routes a `SELL` payment's `sum_kop`
/// to `sum_in_kop` and a `RETURN`'s to `sum_out_kop` (both positive — the
/// DPS `<M SMI/SMO>` contract), counts the docs, and does NOT synthesize
/// zero-valued rows.  Pure (no I/O) — unit-testable without a DB.
fn aggregate_zreport(receipts: &[(DocType, String)]) -> Result<ZReportOut, ConvertError> {
    // (type_code, name) → (sum_in_kop, sum_out_kop)
    let mut groups: BTreeMap<(String, String), (i64, i64)> = BTreeMap::new();
    let mut sell_count: u32 = 0;
    let mut return_count: u32 = 0;

    for (doc_type, payload_json) in receipts {
        let is_sell = match doc_type {
            DocType::Sell => {
                sell_count += 1;
                true
            }
            DocType::Return => {
                return_count += 1;
                false
            }
            other => return Err(ConvertError::UnexpectedShiftReceiptDocType(*other)),
        };
        let parsed: StoredCheckPayments =
            serde_json::from_str(payload_json).map_err(ConvertError::Serialise)?;
        for p in parsed.payments {
            if p.sum_kop < 0 {
                return Err(ConvertError::NegativeStoredPaymentSum {
                    type_code: p.type_code,
                    name: p.name,
                    sum_kop: p.sum_kop,
                });
            }
            let key = (p.type_code, p.name);
            let entry = groups.entry(key.clone()).or_insert((0, 0));
            let target = if is_sell { &mut entry.0 } else { &mut entry.1 };
            *target =
                target
                    .checked_add(p.sum_kop)
                    .ok_or_else(|| ConvertError::ZReportSumOverflow {
                        type_code: key.0.clone(),
                        name: key.1.clone(),
                    })?;
        }
    }

    let payments = groups
        .into_iter()
        .map(
            |((type_code, name), (sum_in_kop, sum_out_kop))| ZReportPaymentOut {
                name,
                sum_in_kop,
                sum_out_kop,
                type_code,
            },
        )
        .collect();

    Ok(ZReportOut {
        payments,
        // TXS is derived separately (`derive_z_report_tax_summaries`) by the
        // shift-aggregation caller, which has the per-doc tax snapshots; the
        // pure payments aggregator leaves it empty.
        tax_summaries: Vec::new(),
        // service_sums is populated by aggregate_z_payload_for_shift after
        // this call (it reads service docs separately via aggregate_shift_service_io).
        service_sums: Vec::new(),
        // epz is populated by aggregate_z_payload_for_shift (reads EPZ docs
        // separately via aggregate_shift_epz).
        epz: None,
        sell_count,
        return_count,
    })
}

/// PR-Z1 (W4-Z2) — per-tax-group turnover (`<TXS>`) for the Z, aggregated from
/// each issued receipt's stored `items` + its pinned tax snapshot.  PURE (no
/// I/O — snapshots are pre-loaded by the caller; invariant #1).  Each item's
/// `sum_kop` accumulates into its canonical tax group's SMI (SELL) / SMO
/// (RETURN); the group's rate resolves via the receipt's OWN snapshot
/// (`resolve_driver_number` → `to_calc_map`), so TXI/TXO use the SAME rate the
/// receipt was signed with.  Full-form when the rate is known; short-form
/// fallback (SMI/SMO/TX only) for an unresolved group (Python
/// `_build_z_report:457-458`).  Mid-shift config drift (the SAME canonical tx
/// resolving to CONFLICTING rates) is fail-closed → manual reconciliation.
fn derive_z_report_tax_summaries(
    receipts: &[(DocType, String, Option<i64>)],
    snapshots: &HashMap<i64, TaxResolutionSnapshot>,
) -> Result<Vec<ZReportTaxSumOut>, ConvertError> {
    struct Accum {
        smi: i64,
        smo: i64,
        /// The group's established resolution: `Some(rate)` = a configured
        /// group (full-form), `None` = unresolved (short-form).  EVERY item
        /// mapping to this canonical tx must match it (see the mixing guard).
        rate: Option<ResolvedTaxGroup>,
        /// Whether the first item has fixed this group's resolution.
        established: bool,
    }
    let mut groups: BTreeMap<i64, Accum> = BTreeMap::new();

    for (doc_type, payload_json, snap_id) in receipts {
        let is_sell = match doc_type {
            DocType::Sell => true,
            DocType::Return => false,
            other => return Err(ConvertError::UnexpectedShiftReceiptDocType(*other)),
        };
        let parsed: StoredCheckItems =
            serde_json::from_str(payload_json).map_err(ConvertError::Serialise)?;
        let snapshot = (*snap_id).and_then(|id| snapshots.get(&id));
        let calc_map = snapshot.map(|s| s.to_calc_map());

        for item in parsed.items {
            let Some(driver_tx) = item.tax_group_1 else {
                // No tax group on the line → no <TXS> contribution (Python skip).
                continue;
            };
            // Resolve driver → canonical tx (identity when there is no snapshot
            // or no driver mapping); rate is Some only when the group is a
            // configured member of the receipt's snapshot.
            let canonical_tx = snapshot
                .and_then(|s| s.resolve_driver_number(driver_tx))
                .unwrap_or(driver_tx);
            let rate = calc_map
                .as_ref()
                .and_then(|m| m.get(&canonical_tx).cloned());

            let accum = groups.entry(canonical_tx).or_insert(Accum {
                smi: 0,
                smo: 0,
                rate: None,
                established: false,
            });
            // Every item mapping to this canonical tx MUST resolve to the SAME
            // rate — including the `None` (unresolved / short-form) vs `Some`
            // (configured) distinction.  A divergence means the shift's receipts
            // disagree on this group: a mid-shift tax-config change, OR a
            // NULL-snapshot (pre-W4-Z2a) receipt mixed with a pinned one.  Either
            // way the TXS row would be ambiguous — a rate applied to turnover it
            // was not signed under — so fail-closed to manual reconciliation,
            // never a silently-wrong Z.
            if !accum.established {
                accum.rate = rate;
                accum.established = true;
            } else if accum.rate != rate {
                return Err(ConvertError::TaxSnapshotDriftInShift { tx: canonical_tx });
            }
            let target = if is_sell {
                &mut accum.smi
            } else {
                &mut accum.smo
            };
            *target = target
                .checked_add(item.sum_kop)
                .ok_or(ConvertError::ZReportTaxSumOverflow { tx: canonical_tx })?;
        }
    }

    let mut out = Vec::with_capacity(groups.len());
    for (tx, accum) in groups {
        match accum.rate {
            Some(g) => {
                // Full-form: TXI/TXO via calc_tax (0 when the side is empty —
                // Python `:450-451`).
                let txi = if accum.smi != 0 {
                    calc_tax(accum.smi, g.txpr, g.dtpr, g.txal)
                        .map_err(ConvertError::TaxCalc)?
                        .0
                } else {
                    0
                };
                let txo = if accum.smo != 0 {
                    calc_tax(accum.smo, g.txpr, g.dtpr, g.txal)
                        .map_err(ConvertError::TaxCalc)?
                        .0
                } else {
                    0
                };
                out.push(ZReportTaxSumOut {
                    tx,
                    tx_short_form: false,
                    txpr: format!("{:.2}", g.txpr),
                    txal: g.txal,
                    txty: g.txty,
                    dtpr: format!("{:.2}", g.dtpr),
                    smi: accum.smi,
                    smo: accum.smo,
                    txi,
                    txo,
                });
            }
            // Short-form fallback: unconfigured group → SMI/SMO/TX only.
            None => out.push(ZReportTaxSumOut {
                tx,
                tx_short_form: true,
                txpr: String::new(),
                txal: 0,
                txty: 0,
                dtpr: String::new(),
                smi: accum.smi,
                smo: accum.smo,
                txi: 0,
                txo: 0,
            }),
        }
    }
    Ok(out)
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

    if line.quantity_milli == 0 {
        return Err(ConvertError::ZeroQuantityLine {
            item_name: line.name.clone(),
        });
    }

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
/// recomputed canonical hash.  `secure_pool` holds `payment_methods`
/// (SELL/RETURN payment names); `main_pool` holds the ledger +
/// `node_state` for the `ZReport`/`ShiftClose` shift aggregation.
/// All DB access is read-only and outside any write transaction
/// (invariant #1).
pub async fn convert_to_signer_payload(
    cmd: &CanonicalCommand,
    fiscal_number: &str,
    main_pool: &SqlitePool,
    secure_pool: &SqlitePool,
) -> Result<ConvertedPayload, ConvertError> {
    // `raw_frames` carries M5-scope fiscal data (check-level discounts /
    // header & footer / service amounts) the structured DTO does not capture
    // — fail closed for EVERY converted doc-type if present, rather than sign
    // it away (same posture as acquirer_slip).  Hoisted ABOVE the match: a
    // SHIFT_OPEN finalizes to a fixed `{opening_sum_kop:0}` and would otherwise
    // silently DROP raw_frames, collapsing two distinct submissions to the same
    // converted payload + hash (an idempotency-key content collision).
    if !cmd.payload.raw_frames.is_empty() {
        return Err(ConvertError::RawFramesNotSupported {
            count: cmd.payload.raw_frames.len(),
        });
    }
    // `return_check_number` (the original-receipt link a RETURN references)
    // has no slot in the compact `<C T=>` wire dialect we ship (ORDERRETNUM is
    // never emitted by the Python-prod / WebCheck reference serializers).
    // Fail closed for every convert-routed doc-type if present — like
    // `raw_frames`, hoisted ABOVE the match — rather than accept-and-drop it
    // (fail-open).  Z-class (ShiftClose/ZReport) is routed AROUND convert at
    // ingress (`is_z_class`) and is intentionally out of scope: the field is
    // envelope-only (never serialized into any stored/wire payload — only
    // `cmd.payload` is) and semantically inapplicable to a Z (no original
    // receipt), so it cannot leak regardless.  See
    // `ConvertError::ReturnCheckNumberNotSupported` for the full ground-truth.
    if cmd.return_check_number.is_some() {
        return Err(ConvertError::ReturnCheckNumberNotSupported);
    }
    match cmd.command_type {
        CommandType::ShiftOpen => finalize(&ShiftOpenOut { opening_sum_kop: 0 }),
        CommandType::Sell | CommandType::Return => {
            if cmd.payload.goods.is_empty() {
                return Err(ConvertError::EmptyGoods);
            }
            // `payload.direction` is intentionally NOT emitted — Sell vs
            // Return is carried by `command_type` (→ `WireArtifactKind`),
            // so the inner CheckJson is direction-agnostic (no data lost).
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

            // ── L5 G2 — ZeroPriceLine (a good with item_sum_kop == 0) ──────────
            // Pure in-memory scan over the converted items (SELL + RETURN).
            // Distinct from ZeroQuantityLine (a zero-PRICE good keeps qty > 0).
            if let Some(zero) = items.iter().find(|it| it.sum_kop == 0) {
                return Err(ConvertError::ZeroPriceLine {
                    item_name: zero.name.clone(),
                });
            }

            // ── L5 G3 — ZeroPaymentAmount (a declared payment leg == 0) ────────
            // Pure in-memory scan over the converted payments (SELL + RETURN).
            if let Some((idx, _)) = payments.iter().enumerate().find(|(_, p)| p.sum_kop == 0) {
                return Err(ConvertError::ZeroPaymentAmount { pay_index: idx });
            }

            // ── L5 G1 — CashCapExceeded (SELL cash legs Σ > 4_999_999 kop) ─────
            // Caps the CASH portion (WebCheck DopNal/AllowableCash, All.cs:875-886),
            // NOT the receipt total.  SELL-only (a RETURN pays cash OUT).  Pure
            // in-memory sum over the type_code=="0" legs.
            const CASH_CAP_KOP: i64 = 4_999_999;
            if cmd.command_type == CommandType::Sell {
                let cash_kop: i64 = payments
                    .iter()
                    .filter(|p| p.type_code == "0")
                    .map(|p| p.sum_kop)
                    .fold(0i64, |a, b| a.saturating_add(b));
                if cash_kop > CASH_CAP_KOP {
                    return Err(ConvertError::CashCapExceeded {
                        cash_kop,
                        cap_kop: CASH_CAP_KOP,
                    });
                }
            }

            // ── L5 G4 — UnderpaymentRefused (SELL Σpayments < Σgoods) ──────────
            // SELL-only (a RETURN is a refund; underpayment semantics don't apply).
            // Fires only when ≥1 payment leg is declared — a SELL with NO payment
            // legs is the pre-existing "cash implied" shape convert tolerates.
            // Pure in-memory sums.  `stage_sign`'s later total cross-check stays
            // as defense-in-depth (L5 adds the earlier fail-closed gate).
            if cmd.command_type == CommandType::Sell && !payments.is_empty() {
                let goods_kop: i64 = items
                    .iter()
                    .map(|it| it.sum_kop)
                    .fold(0i64, |a, b| a.saturating_add(b));
                let paid_kop: i64 = payments
                    .iter()
                    .map(|p| p.sum_kop)
                    .fold(0i64, |a, b| a.saturating_add(b));
                if paid_kop < goods_kop {
                    return Err(ConvertError::UnderpaymentRefused {
                        goods_kop,
                        paid_kop,
                    });
                }
            }

            // ── INV-21 guard — L1 cash-on-hand floor (pre-inbox, row-less) ─────
            // RETURN only: refuse if the cash leg exceeds current cash-on-hand.
            // SELL is excluded (cash in = always safe for this invariant).
            // Guard runs AFTER payment conversion so `type_code` is available.
            // Uses `main_pool` (invariant #1: no write-tx; pure pool read).
            if cmd.command_type == CommandType::Return {
                // Sum the cash legs of this RETURN (type_code == "0", D1 frozen).
                let return_cash_kop: i64 = payments
                    .iter()
                    .filter(|p| p.type_code == "0" && p.sum_kop > 0)
                    .map(|p| p.sum_kop)
                    .fold(0i64, |a, b| a.saturating_add(b));

                if return_cash_kop > 0 {
                    // Read cash-on-hand for the open shift.
                    let cash_on_hand_kop =
                        crate::services::cash_ledger::cash_on_hand_for_fn(main_pool, fiscal_number)
                            .await
                            .map_err(ConvertError::LedgerRead)?;
                    if cash_on_hand_kop < return_cash_kop {
                        return Err(ConvertError::CashInsufficient {
                            cash_on_hand_kop,
                            return_cash_kop,
                        });
                    }
                }
            }

            finalize(&CheckOut { items, payments })
        }
        // RS-3 A1Z: the Z-class aggregation lives in `aggregate_z_payload`
        // (extracted so the write-path Z-builder reuses the SAME ledger
        // read + aggregation — one Z-conversion owner, no parallel
        // aggregator). This ingress arm stays the (currently sole, non-Z-only
        // dispatched) caller for parity until the dispatcher routes Z here.
        CommandType::ShiftClose | CommandType::ZReport => {
            aggregate_z_payload(main_pool, fiscal_number).await
        }
        // L3 — service cash-in (службове внесення) / cash-out (службова видача).
        // Amount from `payload.totals.sale_kopecks` (ServiceIn) /
        // `payload.totals.return_kopecks` (ServiceOut).
        // Name is the constant label used in the Z `<IO>` section.
        // Schema_version carries invariant #7.
        CommandType::ServiceIn | CommandType::ServiceOut => {
            let (amount_kop, name) = if cmd.command_type == CommandType::ServiceIn {
                (
                    cmd.payload.totals.sale_kopecks as i64,
                    "SERVICE_IN".to_string(),
                )
            } else {
                (
                    cmd.payload.totals.return_kopecks as i64,
                    "SERVICE_OUT".to_string(),
                )
            };

            // ── INV-21 guard-3b — ServiceOut over cash-on-hand is refused ──
            // Pre-inbox, row-less.  Mirrors guard-3a (RETURN cash leg).
            // Guard #1 (invariant #1): pure pool read outside any write-tx.
            if cmd.command_type == CommandType::ServiceOut && amount_kop > 0 {
                let cash_on_hand_kop =
                    crate::services::cash_ledger::cash_on_hand_for_fn(main_pool, fiscal_number)
                        .await
                        .map_err(ConvertError::LedgerRead)?;
                if cash_on_hand_kop < amount_kop {
                    return Err(ConvertError::CashInsufficient {
                        cash_on_hand_kop,
                        return_cash_kop: amount_kop,
                    });
                }
            }

            #[derive(serde::Serialize)]
            struct ServiceIoOut {
                schema_version: &'static str,
                amount_kop: i64,
                name: String,
            }
            finalize(&ServiceIoOut {
                schema_version: "1.0",
                amount_kop,
                name,
            })
        }
        // EPZ — видача готівки за ЕПЗ (cash advance against a card).  The
        // cash-out sum + card requisites ride on the SINGLE card payment leg
        // (`CanonicalPayment.acquirer_slip`).  DPS wire = compact `<C T='8'>`
        // (`stage_sign` builds it); NO tax on the good; the cash-out is a
        // LEDGER effect, not a `<payments>` cash line.
        CommandType::CashAdvanceEpz => {
            // Exactly one card payment leg carrying an acquirer_slip.
            let slip_legs: Vec<&super::dto::CanonicalPayment> = cmd
                .payload
                .payments
                .iter()
                .filter(|p| p.acquirer_slip.is_some())
                .collect();
            if cmd.payload.payments.len() != 1 || slip_legs.len() != 1 {
                return Err(ConvertError::EpzMalformedCardLeg {
                    count: cmd.payload.payments.len(),
                });
            }
            let leg = slip_legs[0];
            let slip = leg.acquirer_slip.as_ref().expect("filtered to Some above");
            // paymentid ≥ 2 (card form only; errCode-94 analog).
            if slip.payment_form_index < 2 {
                return Err(ConvertError::EpzPaymentIdTooLow {
                    payment_form_index: slip.payment_form_index,
                });
            }
            let sum_kop = to_i64(leg.amount_kopecks, "epz.amount_kopecks")?;

            // ── INV-21 guard-3c — EPZ over cash-on-hand is refused ──
            // Pre-inbox, row-less (WebCheck `ClassFiscal.cs:1385-1391`, errCode
            // 47).  Mirrors guard-3a (RETURN) / guard-3b (ServiceOut).
            // Invariant #1: pure pool read outside any write-tx.
            if sum_kop > 0 {
                let cash_on_hand_kop =
                    crate::services::cash_ledger::cash_on_hand_for_fn(main_pool, fiscal_number)
                        .await
                        .map_err(ConvertError::LedgerRead)?;
                if cash_on_hand_kop < sum_kop {
                    return Err(ConvertError::CashInsufficient {
                        cash_on_hand_kop,
                        return_cash_kop: sum_kop,
                    });
                }
            }

            // Resolve the card payment-form display name from `payment_methods`
            // (INV-6: carry the full card requisites, not a summary).
            let pay_index = slip.payment_form_index as i64;
            let pm = payment_methods::find(secure_pool, fiscal_number, pay_index)
                .await?
                .ok_or_else(|| ConvertError::MissingPaymentMethod {
                    fiscal_number: fiscal_number.to_string(),
                    pay_index,
                })?;

            #[derive(serde::Serialize)]
            struct EpzOut {
                schema_version: &'static str,
                sum_kop: i64,
                /// Fixed good code (`<P C='0'>`, WebCheck `code='0'`).
                code: String,
                /// Fixed cash-advance good label.
                name: String,
                /// Card payment-form index (`paymentid` ≥ 2 → `<M T='0'>`).
                paymentid: i64,
                /// Card payment-form display name (`<M NM=>`).
                pay_name: String,
                // Card / acquirer requisites (`<M PA..RRN>`).
                pa: String,
                pb: String,
                pc: String,
                pd: String,
                pe: String,
                psnm: String,
                rrn: String,
            }
            finalize(&EpzOut {
                schema_version: "1.0",
                sum_kop,
                code: "0".to_string(),
                name:
                    "ОПЕРАЦІЯ З ВИДАЧІ ГОТІВКОВИХ КОШТІВ ДЕРЖАТЕЛЮ ЕЛЕКТРОННОГО ПЛАТІЖНОГО ЗАСОБУ"
                        .to_string(),
                paymentid: pay_index,
                pay_name: pm.name,
                // WebCheck EPZ `<L>`/`<M>` slip attrs (ClassFiscal.cs:1395-1396):
                //   PA=acquirer, PB=terminal, PC=op-type, PD=masked PAN,
                //   PE=approval code, PSNM=payment-system, RRN=reference.
                pa: slip.merchant_id.clone(),
                pb: slip.terminal_id.clone(),
                pc: slip.operation_type.clone(),
                pd: slip.pan.clone(),
                pe: slip.approval_code.clone(),
                psnm: slip.payment_system.clone(),
                rrn: slip.transaction_code.clone(),
            })
        }
        other => Err(ConvertError::NotSignable(other)),
    }
}

/// RS-3 A1Z — aggregate a GIVEN shift into a signer-ready `ZReportJson`
/// `ConvertedPayload` (payload_json + its sha256).
///
/// Aggregates the shift's issued (ACK / OFFLINE_LOCAL_ACK) SELL/RETURN
/// receipts from the ledger (`fiscal_documents`). Reads ONLY (no write-tx,
/// invariant #1). The returned `payload_sha256_canonical` is the hash of the
/// AGGREGATED body — distinct from the wire-intent hash the inbox carries
/// (D5 dual-hash).
///
/// Takes an EXPLICIT `shift_id` (review MEDIUM): the RS-3 write-path (A2) MUST
/// pass the SAME shift_id it already passed to `quiesce_shift_before_z`, so a
/// `current_shift_id` mutation between quiesce and aggregate can't make the Z
/// aggregate a DIFFERENT shift than the one it quiesced. Use this on the
/// write-path; [`aggregate_z_payload`] (which re-reads `current_shift_id`) is
/// for the ingest Z arm / utilities only.
pub async fn aggregate_z_payload_for_shift(
    main_pool: &SqlitePool,
    fiscal_number: &str,
    shift_id: ShiftId,
) -> Result<ConvertedPayload, ConvertError> {
    let receipts = fiscal_documents::list_shift_issued_receipts(main_pool, fiscal_number, shift_id)
        .await
        .map_err(ConvertError::LedgerRead)?;
    // Load each receipt's pinned tax snapshot once (dedup by id).  DB reads are
    // pool-bound, OUTSIDE any write-tx — the pure aggregators do no I/O
    // (invariant #1).
    let mut snapshots: HashMap<i64, TaxResolutionSnapshot> = HashMap::new();
    for (_, _, snap_id) in &receipts {
        if let Some(id) = snap_id {
            if !snapshots.contains_key(id) {
                let snap = signing_config_snapshots::get_by_id(main_pool, *id)
                    .await
                    .map_err(|e| ConvertError::SnapshotLoad {
                        id: *id,
                        detail: e.to_string(),
                    })?;
                snapshots.insert(*id, snap);
            }
        }
    }
    let tax_summaries = derive_z_report_tax_summaries(&receipts, &snapshots)?;
    // Payments reuse the pure payments-only aggregator; MOVE the payloads out
    // (no clone) now that the snapshot ids are consumed above.
    let two: Vec<(DocType, String)> = receipts.into_iter().map(|(d, p, _)| (d, p)).collect();
    let mut out = aggregate_zreport(&two)?;
    out.tax_summaries = tax_summaries;

    // L3 — aggregate service-in/out docs into Z `<IO>` rows.
    // Reads SERVICE_IN/OUT ACK docs for this shift, accumulates by name.
    // Invariant #1: pool-bound SELECT, no write-tx, no network.
    let (svc_in_kop, svc_out_kop) = crate::services::cash_ledger::aggregate_shift_service_io(
        main_pool,
        fiscal_number,
        shift_id,
    )
    .await
    .map_err(ConvertError::LedgerRead)?;
    // Only emit rows for non-zero totals (absent-when-empty, mirrors Python parity).
    if svc_in_kop > 0 {
        out.service_sums.push(ZReportServiceIoOut {
            name: "SERVICE_IN".to_string(),
            sum_in_kop: svc_in_kop,
            sum_out_kop: 0,
        });
    }
    if svc_out_kop > 0 {
        out.service_sums.push(ZReportServiceIoOut {
            name: "SERVICE_OUT".to_string(),
            sum_in_kop: 0,
            sum_out_kop: svc_out_kop,
        });
    }

    // EPZ — aggregate card-advance docs into the Z `<EPZ EPC EPCS='0' EPSM>`
    // section (STOP-S2).  Count the issued EPZ docs (EPC) + their total sum
    // (EPSM); EPCS is hardcoded 0 for byte-parity (WebCheck FormDate.cs:436).
    // Invariant #1: pool-bound SELECTs, no write-tx, no network.
    let epz_sum_kop =
        crate::services::cash_ledger::aggregate_shift_epz(main_pool, fiscal_number, shift_id)
            .await
            .map_err(ConvertError::LedgerRead)?;
    if epz_sum_kop > 0 {
        // bd PRRO_GATE-a6n: the EPC count MUST select the same turnover set as
        // `aggregate_shift_epz` above, or the Z `<EPZ>` count and sum disagree.
        // Both now go through the single `counted_in_turnover` predicate — the
        // count filters in Rust for the same reason the sum does (the predicate
        // reads three columns and reuses the `is_issued` SSOT; re-spelling it as
        // a SQL literal is the drift this change removes).
        let epz_rows: Vec<(String, Option<i64>, Option<String>)> = sqlx::query_as(
            "SELECT state, offline_fiscal_no, server_fiscal_no FROM fiscal_documents \
             WHERE fiscal_number = ? AND shift_id = ? \
               AND doc_type = 'CASH_ADVANCE_EPZ'",
        )
        .bind(fiscal_number)
        .bind(DbShiftId(shift_id))
        .fetch_all(main_pool)
        .await
        .map_err(ConvertError::LedgerRead)?;
        let epz_count: i64 = epz_rows
            .iter()
            .filter(|(state, ofn, sfn)| {
                fiscal_documents::counted_in_turnover(state, *ofn, sfn.as_deref())
            })
            .count() as i64;
        out.epz = Some(ZReportEpzOut {
            epc: epz_count,
            epcs: 0,
            epsm: epz_sum_kop,
        });
    }

    finalize(&out)
}

/// RS-3 A1Z — resolve the FN's open shift (`node_state.current_shift_id`) then
/// aggregate it. For the INGEST Z arm (no pre-resolved shift) + tests /
/// utilities. The write-path (A2) MUST use [`aggregate_z_payload_for_shift`]
/// with the quiesced shift_id instead, to avoid re-reading `current_shift_id`.
pub async fn aggregate_z_payload(
    main_pool: &SqlitePool,
    fiscal_number: &str,
) -> Result<ConvertedPayload, ConvertError> {
    let shift_id = node_state::get(main_pool, fiscal_number)
        .await
        .map_err(ConvertError::LedgerRead)?
        .and_then(|ns| ns.current_shift_id)
        .ok_or_else(|| ConvertError::NoOpenShiftForZReport {
            fiscal_number: fiscal_number.to_string(),
        })?;
    aggregate_z_payload_for_shift(main_pool, fiscal_number, shift_id).await
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

    #[test]
    fn zero_quantity_line_is_typed_error() {
        let l = line(Some(42), "Bread", 15000, 0);
        assert!(matches!(
            convert_item(&l, false),
            Err(ConvertError::ZeroQuantityLine { .. })
        ));
    }

    /// A maximal item (dual-tax secondary group + a discount adjustment +
    /// barcode/uktzed/excise_stamps) must ALSO parse through the signer —
    /// proves the rich optional fields + adjustment kind/mode strings
    /// match `parse_payload` under `deny_unknown_fields`, not just the
    /// minimal shape.
    #[test]
    fn maximal_item_parses_through_signer() {
        let mut l = line(Some(42), "Bread", 15000, 1000);
        l.tax_group_2 = 2;
        l.barcode = Some("4820000000001".to_string());
        l.uktzed = Some("1905310000".to_string());
        l.excise_stamps = vec!["AB1234567".to_string()];
        l.discount = Some(super::super::dto::Discount {
            direction: DiscountDirection::Discount,
            name: "promo".to_string(),
            amount_kopecks: 500,
        });
        let out = CheckOut {
            // dual_tax_active = true so the secondary TX1 is emitted.
            items: vec![convert_item(&l, true).unwrap()],
            payments: Vec::new(),
        };
        let conv = finalize(&out).unwrap();
        let kind = derive_wire_artifact_kind(DocType::Sell).unwrap();
        validate_signer_payload_shape_for_testing(kind, &conv.payload_json, Some(15000))
            .expect("maximal converted CheckJson must parse through stage_sign");
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

    // ─── piece-2b: aggregate_zreport (pure) ───────────────────────

    fn pay(name: &str, sum_kop: i64, type_code: &str) -> String {
        format!(r#"{{"name":"{name}","sum_kop":{sum_kop},"type_code":"{type_code}"}}"#)
    }
    fn stored_check(payments: &[String]) -> String {
        format!(r#"{{"items":[],"payments":[{}]}}"#, payments.join(","))
    }

    #[test]
    fn aggregate_groups_sell_in_return_out() {
        let receipts = vec![
            (DocType::Sell, stored_check(&[pay("Готівка", 10000, "0")])),
            (DocType::Return, stored_check(&[pay("Готівка", 3000, "0")])),
        ];
        let z = aggregate_zreport(&receipts).unwrap();
        assert_eq!(z.sell_count, 1);
        assert_eq!(z.return_count, 1);
        assert_eq!(z.payments.len(), 1);
        assert_eq!(z.payments[0].type_code, "0");
        assert_eq!(z.payments[0].name, "Готівка");
        assert_eq!(z.payments[0].sum_in_kop, 10000);
        assert_eq!(z.payments[0].sum_out_kop, 3000);
    }

    #[test]
    fn aggregate_distinct_names_same_type_code_split_into_two_rows() {
        let receipts = vec![(
            DocType::Sell,
            stored_check(&[pay("AcqA", 5000, "1"), pay("AcqB", 7000, "1")]),
        )];
        let z = aggregate_zreport(&receipts).unwrap();
        assert_eq!(
            z.payments.len(),
            2,
            "distinct names under one type_code → 2 rows"
        );
        // BTreeMap order: ("1","AcqA") before ("1","AcqB").
        assert_eq!(z.payments[0].name, "AcqA");
        assert_eq!(z.payments[1].name, "AcqB");
    }

    #[test]
    fn aggregate_accumulates_same_group_across_receipts() {
        let receipts = vec![
            (DocType::Sell, stored_check(&[pay("Готівка", 10000, "0")])),
            (DocType::Sell, stored_check(&[pay("Готівка", 2500, "0")])),
        ];
        let z = aggregate_zreport(&receipts).unwrap();
        assert_eq!(z.sell_count, 2);
        assert_eq!(z.payments.len(), 1);
        assert_eq!(z.payments[0].sum_in_kop, 12500);
        assert_eq!(z.payments[0].sum_out_kop, 0);
    }

    #[test]
    fn aggregate_empty_shift_is_zero_no_synthesized_rows() {
        let z = aggregate_zreport(&[]).unwrap();
        assert_eq!(z.sell_count, 0);
        assert_eq!(z.return_count, 0);
        assert!(
            z.payments.is_empty(),
            "no zero-valued payment rows synthesized"
        );
    }

    #[test]
    fn aggregate_malformed_stored_payload_is_typed_error() {
        let receipts = vec![(DocType::Sell, "{not json".to_string())];
        assert!(matches!(
            aggregate_zreport(&receipts),
            Err(ConvertError::Serialise(_))
        ));
    }

    #[test]
    fn aggregate_negative_stored_payment_sum_is_typed_error() {
        // Impossible on the normal pipeline (sum_kop maps from a u64) →
        // a negative stored sum signals ledger corruption; halt the Z.
        let receipts = vec![(DocType::Sell, stored_check(&[pay("Готівка", -5, "0")]))];
        assert!(matches!(
            aggregate_zreport(&receipts),
            Err(ConvertError::NegativeStoredPaymentSum { sum_kop: -5, .. })
        ));
    }

    #[test]
    fn aggregate_missing_payments_field_is_typed_error() {
        // A stored payload lacking the `payments` key (corrupt / wrong
        // shape) must HALT the Z, not be silently treated as zero
        // turnover (which would underreport). `payments` is required.
        let receipts = vec![(DocType::Sell, r#"{"items":[]}"#.to_string())];
        assert!(matches!(
            aggregate_zreport(&receipts),
            Err(ConvertError::Serialise(_))
        ));
    }

    #[test]
    fn aggregate_unexpected_doc_type_is_typed_error() {
        let receipts = vec![(DocType::ShiftOpen, stored_check(&[]))];
        assert!(matches!(
            aggregate_zreport(&receipts),
            Err(ConvertError::UnexpectedShiftReceiptDocType(
                DocType::ShiftOpen
            ))
        ));
    }

    /// The aggregated ZReportJson must parse through the signer's private
    /// `parse_payload` (deny_unknown_fields).
    #[test]
    fn aggregated_zreport_parses_through_signer() {
        let receipts = vec![
            (DocType::Sell, stored_check(&[pay("Готівка", 10000, "0")])),
            (DocType::Return, stored_check(&[pay("Готівка", 3000, "0")])),
        ];
        let conv = finalize(&aggregate_zreport(&receipts).unwrap()).unwrap();
        let kind = derive_wire_artifact_kind(DocType::ZReport).unwrap();
        validate_signer_payload_shape_for_testing(kind, &conv.payload_json, None)
            .expect("aggregated ZReportJson must parse through stage_sign");
    }
}
