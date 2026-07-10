//! Minimal canonical-XML builder for the 4 W4 doc types.
//!
//! Hand-written byte-stream builder targeting byte-identity against
//! the Python serializer at `src/prro_gateway/serializers/dps_xml.py`.
//! No XML crate dependency: a generic XML serializer would force us to
//! fight its attribute-ordering / namespace-emission / escaping
//! defaults to match the Python `_tag` helper exactly, and the
//! contract is per-tag byte-equivalence, not "pretty enough XML".
//!
//! W4 scope (revision 2026-05-06): four canonical-XML doc types per
//! ADR-M2-3 + W4 plan:
//!
//! - `ShiftOpen` — `<C T="108">` + `DI=0` + `<O>` + `<E>`.
//! - `Sell` — `<C T="0">` containing `<P>` items + `<M>` payments +
//!   closing `<E FN N NO SM TS>`.
//! - `Return` — `<C T="1">` with the same body shape as `Sell`.
//! - `ZReport` — `<Z NO="...">` containing `<M>` per-payment-type
//!   totals + `<NC NI NO>` check counts.  Per WebCheck + DPS reality
//!   (`docs/webcheck_reverse/WEBCHECK_ANALYSIS.md:77`,
//!   `WebCheck/CreateDB.cs:624` doctype='80', DPS
//!   `Check.Type::ZREPORT=2` in `fiscal_server.proto:24`), `ZReport`
//!   IS the close-shift wire artifact.  There is no separate
//!   `ShiftClose` variant; do NOT add one.
//!
//! C1 (this commit) lands typed payload structs + the canonical-XML
//! builder + unit tests.  Unit-level acceptance only — attribute
//! alphabetical ordering, cp1251 byte mapping, escaping,
//! deterministic output.  Byte-equivalence against the
//! Python-captured goldens lands in C3.
//!
//! W4-Z1 (2026-05-27) covers the full ФСКО Table 23 surface for the
//! four signable doc types (ShiftOpen, Sell, Return, ZReport):
//!
//!   * <P> extras — barcode CD, UKTZED CZD, TX/TX1 tax groups, <CA>
//!     excise children, per-item <D>/<S> via `adjustments: Vec`.
//!   * <M> extras — EPZ slip attrs (PA/PB/PC/PD/PE/PF/PSNM/RRN) +
//!     cash-only RM/SMP.
//!   * Check-level <D TR="1"> / <S TR="1"> with auto-tracked
//!     `p_item_numbers` driving <NI> children (caller may override
//!     with an explicit subset for POS-precomputed cases).
//!   * <L N=... NM=...> header + footer text lines (strip + skip +
//!     no-counter-increment-on-empty per Python).
//!   * <TX> tax-group children inside <E> + `calc_tax` per ФСКО
//!     TXAL formulas (banker's rounding, lex-by-stringified TX
//!     sort, typed `CalcTaxError` for unsupported TXAL + invalid
//!     rates including NaN/Inf/negative + intermediate overflow).
//!   * Z-report <TXS> / <IO> / <EPZ> aggregations with section
//!     ordering TXS → M → IO → NC → EPZ.
//!
//! **emit_check pattern** mirrors Python `_build_check` exactly:
//! buffer accumulation (`items_buf` / `discounts_buf` /
//! `payments_buf` / `footer_buf`) flushed in concat order
//! `header + items + discounts + payments + footer + total`.
//! Per-item D/S land in `discounts_buf` AFTER all <P> elements;
//! the item_no counter increments INLINE so a D's N attr can be
//! lower than a subsequent P's N (non-monotonic wire-position-vs-N
//! is intentional and Python-identical).
//!
//! **Back-compat invariant**: all expansion is additive.  Empty
//! `Vec` / `None` / `Default::default()` produces the same bytes as
//! the W4 minimal subset.  The four byte-equivalence goldens
//! (xml/shift_open.bin, xml/sell.bin, xml/return.bin,
//! xml/z_report.bin) remain authoritative; pieces 1-6 do not
//! regenerate them.
//!
//! **Caller obligations** (piece 7+ conversion layer):
//!   * `Option<String>` attrs MUST be `None` when absent; `Some("")`
//!     emits `KEY=""` and diverges from Python's `if v:` skip.
//!   * `header_lines` / `footer_lines` MAY contain raw `\n`-split
//!     strings; emitter strips + skips empties.
//!   * `TaxGroupSummary` / `ZReportTaxSummary` callers populate
//!     `txsm` / `dtsm` via `calc_tax(…)?` and must handle
//!     `CalcTaxError::UnsupportedAlgorithm` (TXAL=3) +
//!     `InvalidRate` (corrupted config: NaN/Inf/negative) +
//!     `IntermediateOverflow` (arithmetic blow-up) +
//!     `AggregationOverflow` (i64 sum saturation) +
//!     `TaxMappingNotWired` (empty tax_groups stub).
//!   * `CheckItem.adjustments` is a `Vec` preserving caller order
//!     (Python iterates `g['discounts']` insertion-order).  Mixed
//!     `[Surcharge, Discount]` emits `<S>` before `<D>`.

pub mod cp1251;

// ─── Public typed payloads ────────────────────────────────────────────

/// Common header fields shared by every W4 doc type.  Carved out so
/// per-doc payloads don't repeat the device / FN / TN / TS shape.
#[derive(Debug, Clone)]
pub struct DocumentHeader {
    /// `<DAT FN=...>`.  Operator's fiscal number.
    pub fiscal_number: String,
    /// `<DAT TN=...>`.  Tax number (TIN/EDRPOU).
    pub tax_number: String,
    /// `<DAT ZN=...>`.  Z-report counter for the FN.
    pub z_number: u32,
    /// `<TS>` content.  Pre-formatted Kyiv-local `YYYYMMDDHHMMSS`
    /// string — the builder does NOT do timestamp formatting itself
    /// (that lives in the caller; see `dto::CheckEnvelope.date_time`
    /// commentary about Kyiv-local-as-epoch in `transports::dps`).
    pub ts_str: String,
    /// `<MAC>` content.  Hex-encoded previous-document hash.  Empty
    /// string for first-after-bootstrap.
    pub previous_hash: String,
    /// B9 — offline `<MAC ID='...'>` attribute value.  `None` for ONLINE
    /// docs → bare `<MAC>{previous_hash}</MAC>` (proven live; the goldens
    /// are all online).  `Some(dps_code)` for OFFLINE docs (`fs_mode =
    /// 'OFFLINE'`) → `<MAC ID='{dps_code}'>{previous_hash}</MAC>`, where
    /// `dps_code` is the opaque DPS-issued offline code (== the wire
    /// `id_offline`).  Live DPS rejects an offline drain whose `<MAC>` is
    /// bare with `-9 "not ID in MAC"`.  Set by `stage_sign` for the
    /// offline branch ONLY (branch on `fs_mode`, NOT doc-type) so the ID
    /// lands in the SIGNED bytes → flows into `unsigned_xml_sha256` → the
    /// next doc's `previous_hash` (WebCheck parity: the hash covers the
    /// ID attribute).
    pub mac_id: Option<String>,
    /// `<RQ NDv=...>`.  Default `"ПРО_каса"` (cp1251-encoded on
    /// output).  Held as `String` so callers can override per-pilot.
    pub device_name: String,
    /// `<RQ PrV=...>`.  Default `"1.1"`.
    pub device_version: String,
}

impl DocumentHeader {
    /// Default device-name + version mirror the Python serializer's
    /// kwargs: `device_name='ПРО_каса'`, `device_version='1.1'`.
    pub fn with_defaults(
        fiscal_number: impl Into<String>,
        tax_number: impl Into<String>,
        z_number: u32,
        ts_str: impl Into<String>,
        previous_hash: impl Into<String>,
    ) -> Self {
        Self {
            fiscal_number: fiscal_number.into(),
            tax_number: tax_number.into(),
            z_number,
            ts_str: ts_str.into(),
            previous_hash: previous_hash.into(),
            // B9 — default ONLINE shape (bare `<MAC>`); stage_sign sets
            // `Some(dps_code)` on the offline branch AFTER `with_defaults`.
            mac_id: None,
            device_name: "ПРО_каса".into(),
            device_version: "1.1".into(),
        }
    }
}

/// SHIFT_OPEN — service receipt with `<C T="108">` and DI=0.
#[derive(Debug, Clone)]
pub struct ShiftOpenPayload {
    pub header: DocumentHeader,
    /// `<O SM=...>`.  Opening cash sum (kopecks).
    pub opening_sum: i64,
}

/// B10 — offline-session boundary service receipt (`<C T="109">` BEGIN /
/// `<C T="110">` END).  Byte-pinned to WebCheck
/// `SendingOfflineChecks.cs::EndOfflineXML`: an EMPTY `<C>` element (no
/// `<O>` opening-sum, no `<E>` closing, no `<P>` goods), followed by
/// `<TS>`.  Modelled on the SHIFT_OPEN(108) service-receipt HEADER shape
/// but carries no body.  DI = the doc's local number.  Offline doc →
/// `header.mac_id = Some(dps_code)` (B9 `<MAC ID>`).
#[derive(Debug, Clone)]
pub struct OfflineSessionBoundaryPayload {
    pub header: DocumentHeader,
    /// `<DAT DI=...>`.  The per-FN local document number of the boundary
    /// doc (unlike SHIFT_OPEN which forces DI=0 — these are ordinary
    /// lnd-numbered offline docs).
    pub local_number: u32,
}

/// SELL or RETURN check.  The CanonicalDoc variant picks
/// `<C T="0">` (Sell) vs `<C T="1">` (Return); body shape identical.
#[derive(Debug, Clone)]
pub struct CheckPayload {
    pub header: DocumentHeader,
    /// Per-FN local document number (`<DAT DI=...>`).
    pub local_number: u32,
    /// Line items emitted as `<P>` elements.
    pub items: Vec<CheckItem>,
    /// Payments emitted as `<M>` elements after items.
    pub payments: Vec<CheckPayment>,
    /// `<E SM=...>` total (kopecks).  The closing `<E>` element
    /// always emits `FN / N / NO / SM / TS` per ФСКО Table 23.
    pub total_sum: i64,

    /// W4-Z1 piece 3 — Check-level discounts / surcharges.  Emitted
    /// AFTER all `<P>` items + per-item `<D>` / `<S>` siblings, but
    /// BEFORE `<M>` payments.  Each carries `TR="1"` (check-level
    /// flag, vs `TR="0"` for per-item) + a list of `<NI>` children
    /// naming each affected item's `N` sequence number.
    ///
    /// Per Python `dps_xml.py:248-272` + spec §2 `<D>` / `<S>`
    /// check-level form.
    #[doc(hidden)] // field is pub but defaults to empty for back-compat
    pub check_level_adjustments: Vec<CheckLevelAdjustment>,

    /// W4-Z1 piece 4 — Header text lines emitted as `<L N=... NM=...>`
    /// BEFORE the first `<P>` item.  Per Python `dps_xml.py:172-178`.
    pub header_lines: Vec<String>,

    /// W4-Z1 piece 4 — Footer text lines emitted as `<L N=... NM=...>`
    /// AFTER all payments + check-level adjustments, BEFORE closing
    /// `<E>`.  Per Python `dps_xml.py:181-182, 311-313`.
    pub footer_lines: Vec<String>,

    /// W4-Z1 piece 5 — Per-tax-group summaries emitted as `<TX>`
    /// CHILD elements of the closing `<E>`.  Each carries the
    /// resolved TXSM (tax sum) and DTSM (excise sum) per ФСКО
    /// TXAL formulas (Python `dps_xml.py:_calc_tax:536-563` +
    /// spec §3).  Empty Vec → `<E .../>` is self-closing (back-
    /// compat with existing minimal goldens).
    ///
    /// Caller (W4-Z1 piece 7 conversion layer) groups item.sum by
    /// tax_group_1 and pre-resolves `txsm` + `dtsm` via `_calc_tax`
    /// helper (provided here as `calc_tax(group_sum, rate, dtpr,
    /// txal)`).
    pub tax_summaries: Vec<TaxGroupSummary>,
}

/// Per-tax-group `<TX>` summary inside `<E>` (and inside `<TXS>`
/// for Z-reports — see ZReportTaxSummary).  Mirrors Python
/// `dps_xml.py:526-530`.
#[derive(Debug, Clone)]
pub struct TaxGroupSummary {
    /// `<TX TX=...>`.  Canonical tax group number (1..12 per
    /// `driver_tax_mapping` resolution).
    pub tx: i64,
    /// `<TX TXPR=...>`.  PDV rate, formatted as `"{:.2}"`.
    pub txpr: String,
    /// `<TX TXSM=...>`.  Tax sum (kopecks) computed via _calc_tax.
    pub txsm: i64,
    /// `<TX DTPR=...>`.  Excise rate, formatted as `"{:.2}"`.
    pub dtpr: String,
    /// `<TX DTSM=...>`.  Excise sum (kopecks) computed via _calc_tax.
    pub dtsm: i64,
    /// `<TX TXAL=...>`.  Tax algorithm (0/1/2 per ФСКО + spec §3).
    pub txal: i64,
    /// `<TX TXTY=...>`.  Tax type (default 0 = standard).
    pub txty: i64,
}

/// W4-Z1 piece 5 — error surfaced when an unsupported tax algorithm
/// is requested.  Pre-pilot scope ships TXAL 0/1/2 only; TXAL=3
/// (per-unit excise) is deferred and returns `UnsupportedAlgorithm`.
/// Caller obligation (piece 7+): audit_log warn + decide policy
/// (reject doc / fall back to 0 with explicit audit / surface to
/// operator).  Silent (0,0) drops excise sums and would be an
/// invariant violation.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum CalcTaxError {
    #[error("calc_tax: unsupported TXAL={0} (only 0/1/2 supported in pre-pilot scope)")]
    UnsupportedAlgorithm(i64),
    /// AUDIT4-MIN-1 split — INPUT validation failure: rate is NaN
    /// / ±Inf / negative.  Caller config is corrupted.
    #[error("calc_tax: invalid input rate (txpr={txpr}, dtpr={dtpr})")]
    InvalidRate { txpr: f64, dtpr: f64 },
    /// AUDIT4-IMP-3 — Rates were valid (finite + non-negative) but
    /// the intermediate computation overflowed to ±Inf or NaN.
    /// Distinct from InvalidRate so piece-7 operator debugging
    /// doesn't get misdirected to rate config.
    #[error("calc_tax: arithmetic intermediate overflow (txpr={txpr}, dtpr={dtpr})")]
    IntermediateOverflow { txpr: f64, dtpr: f64 },
    /// Deprecated alias retained for transitional builds.  Prefer
    /// InvalidRate or IntermediateOverflow.  Will be removed once
    /// piece-7 caller switches to the split variants.
    #[deprecated(note = "use InvalidRate or IntermediateOverflow; will be removed post-W4-Z2")]
    #[error("calc_tax: rate not finite (txpr={txpr}, dtpr={dtpr})")]
    RateNotFinite { txpr: f64, dtpr: f64 },
    /// AUDIT5-IMP-1/3 — i64 overflow during per-group sum
    /// accumulation.  Realistic only with maliciously huge items,
    /// but pre-fix would panic in debug + silently wrap in release.
    /// Caller (stage_sign) MUST audit_log + reject the doc; never
    /// retry.
    #[error("derive_check_tax_summaries: aggregation overflow for tax_group_1={tax_group}")]
    AggregationOverflow { tax_group: i64 },
    /// AUDIT5-CRIT-1 — pre-W4-Z2 transition guard.  Live
    /// stage_sign currently passes an EMPTY `tax_groups` map (until
    /// W4-Z2 wires the `driver_tax_mapping` repo).  When an item
    /// carries `tax_group_1 == Some(_)` but the map is empty, the
    /// pre-fix path silently dropped all `<TX>` children — visible
    /// as missing tax info on the wire.  Fail-closed instead so
    /// the gap surfaces at integration time rather than in pilot.
    #[error(
        "derive_check_tax_summaries: items reference tax_group_1 but tax_groups map is empty \
         (driver_tax_mapping not wired yet — W4-Z2)"
    )]
    TaxMappingNotWired { referenced_groups: Vec<i64> },
    /// W4-Z2a piece 14 + 15 (external review CRIT + M1) — POS sent
    /// `tax_group_<field> = driver_number` but the snapshot's
    /// `driver_mapping` (non-empty) has no entry for that
    /// driver_number → canonical_tx_num lookup MISS.  Fail-loud
    /// here so we don't emit `<I TX="<driver_number>">` + drop
    /// `<TX>` summary (silent fiscal divergence).  Operator
    /// inspects via audit_log + reject doc — NEVER retry, NEVER
    /// route to Manual reconciliation (per
    /// `feedback_manual_recon_catastrophe` empirics).
    ///
    /// `field` discriminator: "tax_group_1" (primary) vs
    /// "tax_group_2" (secondary, compound-tax line).  Both groups
    /// are translated through the same driver_mapping; field-aware
    /// surface tells operator which item slot drifted.
    #[error(
        "driver_tax_mapping miss: POS sent {field}={driver_number} \
         but snapshot's driver_mapping has no entry — config drift / new driver"
    )]
    DriverMappingMiss {
        driver_number: i64,
        field: &'static str,
    },
    /// W4-Z2a piece 15 (external review round 2 H1) — snapshot
    /// self-inconsistency: `driver_mapping` translates
    /// `driver_number → canonical_tx_num` but `groups` has no
    /// entry for that `canonical_tx_num`.  Without this guard,
    /// `<I TX="<canonical_tx_num>">` would emit and
    /// `derive_check_tax_summaries` would silently skip the
    /// missing canonical group → silent fiscal divergence one
    /// step later (same class as the original piece-14 CRIT).
    ///
    /// Should not happen with admin-validated snapshots
    /// (`try_from_live` enforces); defensive guard at
    /// `check_payload_from` after translation catches manual SQL
    /// drift / corrupted snapshots loaded via `get_by_id`.
    #[error(
        "snapshot self-inconsistency: {field} driver_number={driver_number} \
         maps to canonical_tx_num={canonical_tx_num} but tax_groups has no such entry"
    )]
    CanonicalTxNotInGroups {
        driver_number: i64,
        canonical_tx_num: i64,
        field: &'static str,
    },
}

/// W4-Z1 piece 5 — `_calc_tax` per ФСКО TXAL formulas.  Mirror of
/// Python `dps_xml.py:536-563` exactly.
///
/// Returns `(txsm, dtsm)` in kopecks for a given group total + rates
/// + algorithm.  Caller computes `group_sum` as the sum of
///   `item.sum` for items with this `tax_group_1`.
///
/// Algorithms:
///   * TXAL=0 — VAT-included: txsm = group_sum * txpr / (100 + txpr).
///     dtsm = 0.
///   * TXAL=1 — Excise pre-VAT: dtsm = group_sum * dtpr / 100;
///     txsm = (group_sum + dtsm) * txpr / (100 + txpr).
///   * TXAL=2 — Excise post-VAT: dtsm = group_sum * dtpr / (100 + dtpr);
///     txsm = (group_sum - dtsm) * txpr / (100 + txpr).
///   * TXAL=3 — Absolute (per-unit) excise.  Returns
///     `Err(UnsupportedAlgorithm(3))` (operator-confirmed
///     deferred).  Caller MUST audit_log + decide policy.
pub fn calc_tax(
    group_sum: i64,
    txpr: f64,
    dtpr: f64,
    txal: i64,
) -> Result<(i64, i64), CalcTaxError> {
    fn round_half_to_even(x: f64) -> i64 {
        // Python `round()` is banker's rounding (round-half-to-even).
        // Rust's `f64::round()` is half-AWAY-from-zero — would diverge
        // from the Python oracle on any `.5`-boundary value (e.g.
        // 12.5 → Python 12, Rust 13).  Implement banker's explicitly
        // so kopeck integer math matches Python byte-for-byte.
        let floor = x.floor();
        let diff = x - floor;
        if diff < 0.5 {
            floor as i64
        } else if diff > 0.5 {
            (floor + 1.0) as i64
        } else {
            // diff == 0.5 → pick the even neighbour.
            let f = floor as i64;
            if f % 2 == 0 {
                f
            } else {
                f + 1
            }
        }
    }

    // INPUT validation — AUDIT2-CRIT-1 + AUDIT3-CRIT-1 + AUDIT4-MIN-1.
    // Reject NaN / ±Inf / negative rates as InvalidRate (corrupted
    // config), distinct from intermediate-arithmetic failures.
    if !txpr.is_finite() || !dtpr.is_finite() || txpr < 0.0 || dtpr < 0.0 {
        return Err(CalcTaxError::InvalidRate { txpr, dtpr });
    }

    // AUDIT3-CRIT-1 + AUDIT4-IMP-3: post-arithmetic finite check.
    // Rates can be valid yet produce non-finite intermediates:
    //   - txpr=1e300 → g*txpr → Inf (multiplication overflow)
    // Surface as IntermediateOverflow (NOT InvalidRate) so the
    // operator-facing error message accurately points at the
    // arithmetic rather than the config.
    let safe_round = |x: f64| -> Result<i64, CalcTaxError> {
        if !x.is_finite() {
            Err(CalcTaxError::IntermediateOverflow { txpr, dtpr })
        } else {
            Ok(round_half_to_even(x))
        }
    };

    let g = group_sum as f64;
    match txal {
        0 => {
            let txsm = if txpr > 0.0 {
                safe_round(g * txpr / (100.0 + txpr))?
            } else {
                0
            };
            Ok((txsm, 0))
        }
        1 => {
            let dtsm = if dtpr > 0.0 {
                safe_round(g * dtpr / 100.0)?
            } else {
                0
            };
            let txsm = if txpr > 0.0 {
                safe_round((g + dtsm as f64) * txpr / (100.0 + txpr))?
            } else {
                0
            };
            Ok((txsm, dtsm))
        }
        2 => {
            let dtsm = if dtpr > 0.0 {
                safe_round(g * dtpr / (100.0 + dtpr))?
            } else {
                0
            };
            let txsm = if txpr > 0.0 {
                safe_round((g - dtsm as f64) * txpr / (100.0 + txpr))?
            } else {
                0
            };
            Ok((txsm, dtsm))
        }
        other => Err(CalcTaxError::UnsupportedAlgorithm(other)),
    }
}

/// Check-level discount or surcharge.  Distinct from per-item
/// `LineAdjustment` because of the `TR="1"` flag + `<NI>` child
/// elements naming affected items.
///
/// Same `Option<String>` empty-vs-`None` contract as
/// [`LineAdjustment`]: caller MUST pass `None` for absent attrs.
#[derive(Debug, Clone)]
pub struct CheckLevelAdjustment {
    /// `D` for discount, `S` for surcharge.  Per Python
    /// `:259 xml_tag = 'D' if d_type != 'EXTRA_CHARGE' else 'S'`.
    pub kind: CheckLevelAdjustmentKind,
    /// `SM=` resolved sum in kopecks.  Caller pre-computes for
    /// percent mode: `round(goods_total * value / 100)`.
    pub sum: i64,
    /// `TY=` mode (Value / Percent).
    pub mode: AdjustmentMode,
    /// `PR=` percent value (only emitted when mode = Percent).
    pub percent: Option<String>,
    /// `NM=` operator-readable name.
    pub name: Option<String>,
    /// `<NI NI=...>` children: each item-N this adjustment applies
    /// to.  Caller populates with the parent `<P>` `N` values.
    pub applies_to_item_ns: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckLevelAdjustmentKind {
    /// Emits `<D TR="1">` ... `</D>`.  Default discount.
    Discount,
    /// Emits `<S TR="1">` ... `</S>`.  Surcharge / extra charge.
    Surcharge,
}

impl Default for CheckPayload {
    fn default() -> Self {
        Self {
            header: DocumentHeader::with_defaults("", "", 0, "", ""),
            local_number: 0,
            items: Vec::new(),
            payments: Vec::new(),
            total_sum: 0,
            check_level_adjustments: Vec::new(),
            header_lines: Vec::new(),
            footer_lines: Vec::new(),
            tax_summaries: Vec::new(),
        }
    }
}

/// Single line item inside a SELL/RETURN check.  Mirrors Python
/// `_build_check`'s `<P>` element.
///
/// W4-Z1 (2026-05-27) extended the W4 minimal subset (C/N/NM/PRC/Q/SM)
/// with the optional attributes the Python `dps_xml.py` serializer
/// emits + the operator-confirmed pilot requirements per spec §2
/// (excise marks, UKTZED, barcode, TX/TX1 tax groups, per-item
/// discount/surcharge).  Empty `None` / `Vec::new()` → attribute /
/// child element NOT emitted (back-compat with existing minimal
/// goldens).
///
/// **Optional-attr semantics**: caller MUST pass `None` for absent
/// string attrs.  `Some("")` emits `KEY=""`, which diverges from
/// Python's `if value:` skip semantics and would surface as a
/// piece-8 byte-equivalence break.
#[derive(Debug, Clone, Default)]
pub struct CheckItem {
    /// `<P C=...>`.  Item code (article SKU).
    pub code: String,
    /// `<P NM=...>`.  Item name.
    pub name: String,
    /// `<P PRC=...>`.  Per-unit price (kopecks).
    pub price: i64,
    /// `<P Q=...>`.  Quantity (thousandths, per Python).
    pub quantity: i64,
    /// `<P SM=...>`.  Line total (kopecks).
    pub sum: i64,

    // ─── W4-Z1 optional attributes ─────────────────────────────────
    /// `<P CD=...>`.  Barcode.  Per Python `dps_xml.py:197`:
    /// `p_attrs['CD'] = barcode`.  Omit if `None`.
    pub barcode: Option<String>,

    /// `<P CZD=...>`.  УКТЗЕД (HS) code.  Per Python `:200`:
    /// `p_attrs['CZD'] = uktzed`.  Omit if `None`.  NB this is NOT
    /// the same as `CD` (which is barcode).
    pub uktzed: Option<String>,

    /// `<P TX=...>`.  Primary tax group code.  `Some(0)` = звільнено
    /// (exempt), `Some(-1)` = не об'єкт ПДВ (not-VAT-object per
    /// `feedback_pdv_zero` memory), `Some(1..)` = regular tax group.
    /// `None` → attribute omitted (W4 refund variant per WebCheck
    /// `StringXML.cs:957` opertyp=-8 branch).  `i64` so the field can
    /// carry -1 even though canonical W3 DTO uses `u8` (W4-Z1 piece
    /// 7 conversion layer maps DTO→CheckItem with sentinel handling).
    pub tax_group_1: Option<i64>,

    /// `<P TX1=...>`.  Secondary tax group (dual-tax mode per
    /// Python `:204`).  Rare; only emit when operator explicitly
    /// configured the line as dual-tax.
    pub tax_group_2: Option<i64>,

    /// Excise marks (DSTU 9095-04 acc-codes).  Per WebCheck
    /// `StringXML.cs:1547 AAAAA()`: each stamp becomes a
    /// `<CA CA='{stamp}'></CA>` child of `<P>`.  Empty → `<P .../>`
    /// self-closing form (no children).  Per Python `:205-206`:
    /// `ca_xml = ''.join(_tag('CA', {'CA': m}) for m in excise_marks)`.
    pub excise_stamps: Vec<String>,

    /// Per-item adjustments (discount / surcharge).  Per Python
    /// `dps_xml.py:217-244`: caller's `g.get('discounts') or []` is
    /// a list iterated in insertion order; each entry can be a
    /// discount (`<D>`) OR a surcharge (`<S>`) via `kind`.  Wire
    /// preserves caller order — `[Surcharge, Discount]` emits
    /// `<S>` BEFORE `<D>`.
    ///
    /// AUDIT2-IMP-1 fix: previously two `Option` slots (discount,
    /// surcharge) which (a) lost multiplicity (no two discs on one
    /// item) and (b) hardcoded D-before-S wire order.  Now a
    /// `Vec` preserves both.
    pub adjustments: Vec<LineAdjustment>,
}

/// Per-line adjustment (discount or surcharge) — shared shape for
/// `<D>` and `<S>` sibling elements after `<P>`.  Per Python
/// `dps_xml.py:226-244`.
///
/// **Optional-attr semantics** (apply to all `Option<String>` fields
/// below).  Caller MUST normalize absent/empty values to `None`.
/// Passing `Some("")` emits `KEY=""` which diverges from Python's
/// `_tag` + caller-side `if d.get('name'):` skip semantics (Python
/// omits the attr entirely).  Empty-string drift is silent at
/// unit-test level and surfaces as a piece-8 byte-equivalence break.
#[derive(Debug, Clone)]
pub struct LineAdjustment {
    /// `<D>` (Discount) or `<S>` (Surcharge).  Per Python
    /// `dps_xml.py:225`: `xml_tag = 'D' if d_type != 'EXTRA_CHARGE'
    /// else 'S'`.
    pub kind: LineAdjustmentKind,
    /// Sum in kopecks.  For percent-mode this is the resolved
    /// `round(item.sum * pr / 100)`; caller resolves percent at
    /// construction time (xml/ stays format-only).
    pub sum: i64,
    /// `TY=` mode flag.  `0` = VALUE (default), `1` = PERCENT.
    pub mode: AdjustmentMode,
    /// `PR=` percent value, formatted as `"{:.2}"`.  Only emitted
    /// when `mode == AdjustmentMode::Percent`.
    pub percent: Option<String>,
    /// `NM=` operator-readable name (e.g. "Знижка постійному
    /// клієнту").  Omit if empty.
    pub name: Option<String>,
    /// `DN=` privilege code.  Omit if empty.
    pub privilege: Option<String>,
    /// `TX=` tax code.  Omit if empty.  Drives ПДВ-зачот calculations.
    pub tax_code: Option<String>,
}

/// Per-item-adjustment kind selector.  Drives `<D>` vs `<S>` tag
/// name in `emit_line_adjustment`.  Per Python `:225`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineAdjustmentKind {
    /// Emits `<D N=... NI=... TR="0" ...>`.  Default per-item form.
    Discount,
    /// Emits `<S N=... NI=... TR="0" ...>`.  Per Python `EXTRA_CHARGE`.
    Surcharge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdjustmentMode {
    /// `TY="0"` — fixed-sum mode.  `sum` carries the value.
    Value,
    /// `TY="1"` — percent mode.  `percent` is the rate, `sum` is the
    /// resolved kopeck amount (caller pre-computes from item sum ×
    /// rate / 100).
    Percent,
}

/// Single payment inside a SELL/RETURN check.  Mirrors Python's
/// `<M>` shape.
///
/// W4-Z1 (2026-05-27) extended the minimal subset (N/NM/SM/T) with
/// the EPZ acquirer-slip attributes (Python `dps_xml.py:289-301`,
/// WebCheck `DopTegE()`) + cash-only `RM` (change) + `SMP` (rounding).
/// Empty `None` → attribute NOT emitted.  Cash payments (T="0") may
/// carry RM/SMP; non-cash payments (T="1"+) carry EPZ slip attrs.
///
/// **Optional-attr semantics**: caller MUST normalize empty/absent
/// string fields to `None`.  `Some("")` emits `KEY=""` and diverges
/// from Python (which skips falsy values).  Drift is invisible at
/// unit-test level; surfaces at piece-8 byte-equivalence.
#[derive(Debug, Clone, Default)]
pub struct CheckPayment {
    /// `<M NM=...>`.  Payment-type display name (e.g. `"Готівка"`).
    pub name: String,
    /// `<M SM=...>`.  Amount (kopecks).
    pub sum: i64,
    /// `<M T=...>`.  Type code per WebCheck PayForms ordering:
    /// `"0"` = first cash slot (Готівка), `"1"+` = subsequent
    /// cashless slots (Картка / Кредит / Сертифікат / custom).
    pub type_code: String,

    // ─── W4-Z1: EPZ acquirer-slip attributes ─────────────────────
    //
    // Per Python `dps_xml.py:289-300` + WebCheck `StringXML.cs:663
    // DopTegE()`.  Only emit when populated; non-cash payments
    // typically carry several of these, cash payments carry none.
    /// `<M PA=...>`.  Payment system / acquirer name (or first
    /// EPZ slot per WebCheck `tegD.PA`).
    pub pa: Option<String>,
    /// `<M PB=...>`.  Terminal ID.
    pub pb: Option<String>,
    /// `<M PC=...>`.  Operation type / human label.
    pub pc: Option<String>,
    /// `<M PD=...>`.  Card mask (last 4 digits + masked PAN).
    pub pd: Option<String>,
    /// `<M PE=...>`.  Approval / authorisation code from acquirer.
    pub pe: Option<String>,
    /// `<M PF=...>`.  Commission / fee (kopecks).  Per Python
    /// `:300` only emitted when commission > 0.
    pub pf: Option<i64>,
    /// `<M PSNM=...>`.  Payment-system name (e.g. "Visa"/"Mastercard").
    pub psnm: Option<String>,
    /// `<M RRN=...>`.  Retrieval Reference Number (acquirer
    /// transaction ID).
    pub rrn: Option<String>,

    // ─── W4-Z1: Cash-only attrs ───────────────────────────────────
    /// `<M RM=...>`.  Change returned to customer (kopecks).  Per
    /// Python `:301-303` emitted ONLY on the FIRST cash payment
    /// (T="0") and ONLY when total paid > total due.  Caller
    /// (W4-Z1 piece 7) resolves the "first cash" semantics.
    pub rm: Option<i64>,
    /// `<M SMP=...>`.  Rounding adjustment (kopecks).  Per Python
    /// `:304-306` emitted ONLY on the FIRST cash payment when
    /// `receipt.rounding != 0`.  Carries the rounded-off amount
    /// (positive = customer overpaid by N kopecks rounded to
    /// hryvnia; negative = underpaid).
    pub smp: Option<i64>,
}

/// Z_REPORT — shift-close fiscal document.  Per WebCheck + DPS
/// reality this DOUBLES as the CloseShift wire artifact; do NOT add
/// a separate `ShiftClosePayload`.
///
/// W4 first-round subset: `<Z NO=...>` containing `<M>` per-payment-
/// type sums and a single `<NC NI NO>` check-count footer.  Optional
/// `<TXS>` / `<IO>` / `<EPZ>` sections from the Python serializer
/// are deliberately omitted; C2 fixtures will be designed against
/// this subset.
#[derive(Debug, Clone)]
pub struct ZReportPayload {
    pub header: DocumentHeader,
    pub local_number: u32,
    /// W4-Z1 piece 6 — `<TXS>` per-tax-group inflow/outflow.
    /// Emitted BEFORE `<M>` payments (Python `:435-458`).  Empty Vec
    /// → no `<TXS>` emission (back-compat with minimal goldens).
    pub tax_summaries: Vec<ZReportTaxSummary>,
    /// Per-payment-type aggregate sums.  Each entry emits one
    /// `<M NM SMI SMO T>` element.
    pub payments: Vec<ZReportPaymentSum>,
    /// W4-Z1 piece 6 — `<IO>` service in/out summaries.  Emitted
    /// AFTER `<M>` payments, BEFORE `<NC>` (Python `:470-477`).
    pub service_sums: Vec<ZReportServiceSum>,
    /// `<NC NI=... NO=...>` check counts.
    pub check_count: ZReportCheckCount,
    /// W4-Z1 piece 6 — `<EPZ>` electronic-payment-instrument totals.
    /// Emitted LAST inside `<Z>` (Python `:484-491`).  None → no
    /// emission.
    pub epz: Option<ZReportEpzTotals>,
}

/// W4-Z1 piece 6 — per-tax-group inflow/outflow summary inside
/// `<Z>`.  Mirrors Python `:435-458`.  Two emission modes:
///
///   * `tx_short_form = false` — full attrs (DTPR/SMI/SMO/TS/TX/TXAL
///     /TXI/TXO/TXPR/TXTY) when the tax_groups lookup resolved.
///   * `tx_short_form = true` — fallback when tax_group not found:
///     only SMI/SMO/TX are emitted.
///
/// Caller (W4-Z1 piece 7 conversion layer) populates `txi` / `txo`
/// via `calc_tax(smi/smo, txpr, dtpr, txal)` for the full form, OR
/// flips `tx_short_form` and leaves rate fields empty for fallback.
#[derive(Debug, Clone)]
pub struct ZReportTaxSummary {
    /// `<TXS TX=...>`.  Canonical tax group number.
    pub tx: i64,
    /// When true, emit only `SMI/SMO/TX`; ignore rate fields.
    pub tx_short_form: bool,
    /// `<TXS TXPR=...>`.  PDV rate, pre-formatted `"{:.2}"`.
    pub txpr: String,
    /// `<TXS TXAL=...>`.
    pub txal: i64,
    /// `<TXS TXTY=...>`.
    pub txty: i64,
    /// `<TXS DTPR=...>`.  Excise rate, pre-formatted `"{:.2}"`.
    pub dtpr: String,
    /// `<TXS SMI=...>`.  Inflow group sum (kopecks).
    pub smi: i64,
    /// `<TXS SMO=...>`.  Outflow group sum (kopecks).
    pub smo: i64,
    /// `<TXS TXI=...>`.  Inflow tax sum, pre-computed via `calc_tax`.
    pub txi: i64,
    /// `<TXS TXO=...>`.  Outflow tax sum, pre-computed via `calc_tax`.
    pub txo: i64,
    /// `<TXS TS=...>`.  Date prefix (`ts_str[:8]` = `YYYYMMDD`).
    pub ts_prefix: String,
}

/// W4-Z1 piece 6 — service in/out summary inside `<Z>`.  Mirrors
/// Python `:470-477`.  Emits `<IO NM SMI SMO T="0">`.
#[derive(Debug, Clone)]
pub struct ZReportServiceSum {
    /// `<IO NM=...>`.  Service-type name (e.g. `"SERVICE_IN"`,
    /// `"SERVICE_OUT"`).
    pub name: String,
    /// `<IO SMI=...>`.
    pub sum_in: i64,
    /// `<IO SMO=...>`.
    pub sum_out: i64,
}

/// W4-Z1 piece 6 — electronic-payment-instrument totals inside
/// `<Z>`.  Mirrors Python `:484-491`.  Single element with three
/// attrs.
#[derive(Debug, Clone)]
pub struct ZReportEpzTotals {
    /// `<EPZ EPC=...>`.  Count of EPI operations.
    pub epc: i64,
    /// `<EPZ EPCS=...>`.  Count of successful EPI operations.
    pub epcs: i64,
    /// `<EPZ EPSM=...>`.  Total sum across EPI operations (kopecks).
    pub epsm: i64,
}

#[derive(Debug, Clone)]
pub struct ZReportPaymentSum {
    /// `<M NM=...>`.  Payment-type name (`"CASH"`, `"CARD"`, ...).
    pub name: String,
    /// `<M SMI=...>`.  Inflow sum (kopecks).
    pub sum_in: i64,
    /// `<M SMO=...>`.  Outflow sum (kopecks).
    pub sum_out: i64,
    /// `<M T=...>`.  `"0"` cash, `"2"` non-cash.
    pub type_code: String,
}

#[derive(Debug, Clone)]
pub struct ZReportCheckCount {
    /// `<NC NI=...>`.  Sell-receipt count.
    pub sell_count: u32,
    /// `<NC NO=...>`.  Return-receipt count.
    pub return_count: u32,
}

/// Top-level discriminated wrapper consumed by `build_canonical_xml`.
#[derive(Debug, Clone)]
pub enum CanonicalDoc {
    ShiftOpen(ShiftOpenPayload),
    Sell(CheckPayload),
    Return(CheckPayload),
    /// CloseShift wire artifact (DPS Check.Type::ZREPORT=2; WebCheck
    /// CreateDB.cs:624 doctype='80').  No separate `ShiftClose`
    /// variant.
    ZReport(ZReportPayload),
    /// B10 — offline-session BEGIN (`<C T="109">`, verAPI=2).  Opens the
    /// DPS-side offline window at drain; sent FIRST.
    OfflineSessionBegin(OfflineSessionBoundaryPayload),
    /// B10 — offline-session END (`<C T="110">`).  Closes the DPS-side
    /// offline window at drain; sent LAST.
    OfflineSessionEnd(OfflineSessionBoundaryPayload),
}

// ─── Build entry point ────────────────────────────────────────────────

/// Errors a builder run can surface.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum XmlBuildError {
    #[error("cp1251: cannot encode character {0:?} (U+{1:04X}) in payload field")]
    Cp1251Unmappable(char, u32),
    /// AUDIT4-IMP-4 — `CheckLevelAdjustment.applies_to_item_ns`
    /// override referenced an N value that does not correspond to
    /// any emitted `<P>` element.  Pre-fix would emit `<NI NI="X">`
    /// dangling reference and DPS would reject.  Caller MUST
    /// validate the subset against the actual items vector before
    /// passing to the builder.
    #[error(
        "check-level adjustment refers to N={referenced_n} but only items {tracked:?} were emitted"
    )]
    OrphanCheckLevelNi {
        referenced_n: u32,
        tracked: Vec<u32>,
    },
}

/// Build the canonical wire XML for a `CanonicalDoc`.  Output is a
/// cp1251-encoded byte stream ready for signing / wire submission.
pub fn build_canonical_xml(doc: &CanonicalDoc) -> Result<Vec<u8>, XmlBuildError> {
    let mut out = String::new();
    match doc {
        CanonicalDoc::ShiftOpen(p) => emit_shift_open(p, &mut out),
        CanonicalDoc::Sell(p) => emit_check(p, "0", &mut out)?,
        CanonicalDoc::Return(p) => emit_check(p, "1", &mut out)?,
        CanonicalDoc::ZReport(p) => emit_z_report(p, &mut out),
        CanonicalDoc::OfflineSessionBegin(p) => emit_offline_session_boundary(p, "109", &mut out),
        CanonicalDoc::OfflineSessionEnd(p) => emit_offline_session_boundary(p, "110", &mut out),
    }
    cp1251::encode(&out)
}

// ─── Per-doc emitters (literal port of Python `_build_*` helpers) ─────

fn emit_shift_open(p: &ShiftOpenPayload, out: &mut String) {
    let h = &p.header;
    open_rq(out, h);
    open_dat(out, h, "0");
    tag_attrs(out, "C", &[("T", "108")]);
    let opening = p.opening_sum.to_string();
    tag_attrs(out, "O", &[("N", "1"), ("SM", &opening), ("T", "0")]);
    close(out, "O");
    tag_attrs(out, "E", &[("N", "2")]);
    close(out, "E");
    close(out, "C");
    tag_text(out, "TS", &h.ts_str);
    close(out, "DAT");
    emit_mac(out, h);
    close(out, "RQ");
}

/// B10 — offline-session BEGIN (`c_type="109"`) / END (`c_type="110"`)
/// service receipt.  Byte-pinned to WebCheck
/// `SendingOfflineChecks.cs::EndOfflineXML`: an EMPTY `<C T='...'></C>`
/// element (NO `<O>`, NO `<E>`, NO `<P>` goods) between `<DAT>` and
/// `<TS>`.  DI = `local_number` (unlike SHIFT_OPEN's forced DI=0 — these
/// are ordinary lnd-numbered offline docs).  `<MAC>` carries the ID
/// attribute when `header.mac_id` is set (B9 offline path).
fn emit_offline_session_boundary(
    p: &OfflineSessionBoundaryPayload,
    c_type: &str,
    out: &mut String,
) {
    let h = &p.header;
    open_rq(out, h);
    let di = p.local_number.to_string();
    open_dat(out, h, &di);
    tag_attrs(out, "C", &[("T", c_type)]);
    close(out, "C");
    tag_text(out, "TS", &h.ts_str);
    close(out, "DAT");
    emit_mac(out, h);
    close(out, "RQ");
}

fn emit_check(p: &CheckPayload, c_type: &str, out: &mut String) -> Result<(), XmlBuildError> {
    let h = &p.header;
    open_rq(out, h);
    let di = p.local_number.to_string();
    open_dat(out, h, &di);
    tag_attrs(out, "C", &[("T", c_type)]);

    // AUDIT2-IMP-1 — Python `_build_check:155-159, :209, :244, :271,
    // :321-324` accumulates SEPARATE buffers and concats in order:
    //
    //   header_xml + items_xml + discounts_xml + payments_xml
    //                + footer_xml + total_xml
    //
    // Per-item D/S go into discounts_xml (NOT inline after the
    // parent P), and the item_no counter increments INLINE inside
    // the items loop while populating both buffers.  Wire wire-
    // ordering: P P D D ... NOT P D P D ...  Mirror exactly for
    // byte parity.
    let mut item_no: u32 = 1;
    let mut items_buf = String::new();
    let mut discounts_buf = String::new();
    let mut payments_buf = String::new();
    let mut footer_buf = String::new();
    // AUDIT3-IMP-1 (B): track every emitted item's N for auto-fill
    // of check-level <NI> children (Python `p_item_numbers` mirror).
    let mut p_item_numbers: Vec<u32> = Vec::new();

    // header_xml — emitted directly into `out` (Python `:172-178`).
    // Python strips + skips empties + does NOT increment item_no on
    // skip.  Mirror exactly — caller may pass raw `\n`-split slices.
    for line in &p.header_lines {
        let stripped = line.trim();
        if stripped.is_empty() {
            continue;
        }
        let n = item_no.to_string();
        tag_attrs(out, "L", &[("N", &n), ("NM", stripped)]);
        close(out, "L");
        item_no += 1;
    }

    // items_xml + per-item discounts_xml — Python `:184-245`.
    // Counter increments inline; <P> goes to items_buf, D/S goes to
    // discounts_buf in caller order.
    for it in &p.items {
        let p_item_n = item_no;
        let n = p_item_n.to_string();
        let prc = it.price.to_string();
        let q = it.quantity.to_string();
        let sm = it.sum.to_string();

        // Build `<P>` attribute list — alphabetical (C, CD, CZD, N,
        // NM, PRC, Q, SM, TX, TX1).  `tag_attrs` sorts; we build in
        // alpha for readability + a tiny perf win.
        let tx_str = it.tax_group_1.map(|v| v.to_string());
        let tx1_str = it.tax_group_2.map(|v| v.to_string());

        let mut p_attrs: Vec<(&str, &str)> = Vec::with_capacity(10);
        p_attrs.push(("C", &it.code));
        if let Some(barcode) = &it.barcode {
            p_attrs.push(("CD", barcode));
        }
        if let Some(uktzed) = &it.uktzed {
            p_attrs.push(("CZD", uktzed));
        }
        p_attrs.push(("N", &n));
        p_attrs.push(("NM", &it.name));
        p_attrs.push(("PRC", &prc));
        p_attrs.push(("Q", &q));
        p_attrs.push(("SM", &sm));
        if let Some(tx) = tx_str.as_deref() {
            p_attrs.push(("TX", tx));
        }
        if let Some(tx1) = tx1_str.as_deref() {
            p_attrs.push(("TX1", tx1));
        }
        tag_attrs(&mut items_buf, "P", &p_attrs);

        // `<CA>` excise stamp children INSIDE <P>...</P>.  Python
        // `:206` ca_xml = join over excise_marks.
        for stamp in &it.excise_stamps {
            tag_attrs(&mut items_buf, "CA", &[("CA", stamp)]);
            close(&mut items_buf, "CA");
        }
        close(&mut items_buf, "P");
        item_no += 1;

        // Per-item D/S go to discounts_buf in caller order.  Python
        // `:217-245`: iterates `g.get('discounts') or []` preserving
        // input order, picks `<D>` or `<S>` per `d_type`.  The
        // counter increments INLINE — the D's N attr is one more
        // than the parent P's N.
        //
        // AUDIT3-IMP-1 (A) + AUDIT4-IMP-2 — Python `:220` reads
        // `d_value` and skips on `d_value == 0`.  For PERCENT mode
        // `d_value` is the RATE (not the resolved sum), so a free
        // gift (item.sum=0) with 10% disc → Python emits
        // `<D SM="0" PR="10.00">`.  Rust mirrors:
        //   - VALUE mode: skip when `adj.sum == 0`.
        //   - PERCENT mode: skip only when the rate string parses
        //     to 0.0 (or is absent).  Resolved zero-sum from a
        //     non-zero rate is NOT skipped.
        for adj in &it.adjustments {
            if adjustment_is_zero(adj.mode, adj.sum, adj.percent.as_deref()) {
                continue;
            }
            let tag_name = match adj.kind {
                LineAdjustmentKind::Discount => "D",
                LineAdjustmentKind::Surcharge => "S",
            };
            emit_line_adjustment(&mut discounts_buf, tag_name, adj, item_no, p_item_n);
            item_no += 1;
        }

        // AUDIT3-IMP-1 (B): track p_item_numbers for check-level
        // auto-fill (Python `:208 p_item_numbers.append(item_no)`
        // BEFORE inner discount loop — so ALL items get tracked).
        // We push AFTER the P emit + AFTER the per-item disc loop,
        // since we captured p_item_n at top of iteration.
        p_item_numbers.push(p_item_n);
    }

    // Check-level adjustments — Python `:248-272`.  They APPEND to
    // the same `discounts_xml` buffer (per `:271 discounts_xml +=
    // _tag(...)`), so they come after per-item D/S in wire order.
    //
    // AUDIT3-IMP-1 (A): Python `:254-256` zero-skip without
    // counter increment — mirror.
    //
    // AUDIT3-IMP-1 (B) + operator clarification: `applies_to_item_ns`
    // is an OPTIONAL OVERRIDE.  Empty Vec → auto-fill with
    // `p_item_numbers` (Python parity, `:260` `for n in
    // p_item_numbers`).  Non-empty Vec → use caller-supplied subset
    // (POS-precomputed real-world case, e.g. "discount only on
    // alcohol items").
    for cla in &p.check_level_adjustments {
        // AUDIT4-IMP-2: percent-mode parity (see per-item branch).
        if adjustment_is_zero(cla.mode, cla.sum, cla.percent.as_deref()) {
            continue;
        }
        // AUDIT4-IMP-1 — Python `:249 if check_discounts and
        // p_item_numbers:` skips ENTIRE check-level block when no
        // items.  Auto-fill source is empty → would emit orphan
        // `<D TR="1">` w/o `<NI>` children → DPS reject.  Skip.
        let auto_fill = cla.applies_to_item_ns.is_empty();
        if auto_fill && p_item_numbers.is_empty() {
            continue;
        }
        // AUDIT4-IMP-4 — override validation: every N in
        // applies_to_item_ns must reference a tracked p_item_number.
        // Surface as typed error rather than emit orphan `<NI>`.
        if !auto_fill {
            for &n in &cla.applies_to_item_ns {
                if !p_item_numbers.contains(&n) {
                    return Err(XmlBuildError::OrphanCheckLevelNi {
                        referenced_n: n,
                        tracked: p_item_numbers.clone(),
                    });
                }
            }
        }
        let self_n = item_no;
        let self_n_str = self_n.to_string();
        let sm_str = cla.sum.to_string();
        let (ty_str, percent_str) = match cla.mode {
            AdjustmentMode::Value => ("0", None),
            AdjustmentMode::Percent => ("1", cla.percent.as_deref()),
        };
        let tag_name = match cla.kind {
            CheckLevelAdjustmentKind::Discount => "D",
            CheckLevelAdjustmentKind::Surcharge => "S",
        };

        let mut attrs: Vec<(&str, &str)> = Vec::with_capacity(6);
        attrs.push(("N", &self_n_str));
        attrs.push(("SM", &sm_str));
        attrs.push(("TR", "1")); // check-level flag
        attrs.push(("TY", ty_str));
        if let Some(pr) = percent_str {
            attrs.push(("PR", pr));
        }
        if let Some(name) = cla.name.as_deref() {
            attrs.push(("NM", name));
        }
        tag_attrs(&mut discounts_buf, tag_name, &attrs);
        // `<NI NI="...">` children: empty caller subset → auto-fill
        // with all tracked p_item_numbers (Python parity); non-empty
        // → use as-is (POS subset).
        let ni_source: &[u32] = if cla.applies_to_item_ns.is_empty() {
            &p_item_numbers
        } else {
            &cla.applies_to_item_ns
        };
        for &item_n in ni_source {
            let item_n_str = item_n.to_string();
            tag_attrs(&mut discounts_buf, "NI", &[("NI", &item_n_str)]);
            close(&mut discounts_buf, "NI");
        }
        close(&mut discounts_buf, tag_name);
        item_no += 1;
    }

    // payments_xml — Python `:281-308`.
    for pay in &p.payments {
        let n = item_no.to_string();
        let sm = pay.sum.to_string();
        // Owned String buffers for any numeric optional attrs so the
        // &str borrow lives through the tag_attrs call.
        let pf_str = pay.pf.map(|v| v.to_string());
        let rm_str = pay.rm.map(|v| v.to_string());
        let smp_str = pay.smp.map(|v| v.to_string());

        let mut m_attrs: Vec<(&str, &str)> = Vec::with_capacity(12);
        m_attrs.push(("N", &n));
        m_attrs.push(("NM", &pay.name));
        m_attrs.push(("SM", &sm));
        m_attrs.push(("T", &pay.type_code));
        if let Some(v) = pay.pa.as_deref() {
            m_attrs.push(("PA", v));
        }
        if let Some(v) = pay.pb.as_deref() {
            m_attrs.push(("PB", v));
        }
        if let Some(v) = pay.pc.as_deref() {
            m_attrs.push(("PC", v));
        }
        if let Some(v) = pay.pd.as_deref() {
            m_attrs.push(("PD", v));
        }
        if let Some(v) = pay.pe.as_deref() {
            m_attrs.push(("PE", v));
        }
        if let Some(v) = pf_str.as_deref() {
            m_attrs.push(("PF", v));
        }
        if let Some(v) = pay.psnm.as_deref() {
            m_attrs.push(("PSNM", v));
        }
        if let Some(v) = pay.rrn.as_deref() {
            m_attrs.push(("RRN", v));
        }
        if let Some(v) = rm_str.as_deref() {
            m_attrs.push(("RM", v));
        }
        if let Some(v) = smp_str.as_deref() {
            m_attrs.push(("SMP", v));
        }
        tag_attrs(&mut payments_buf, "M", &m_attrs);
        close(&mut payments_buf, "M");
        item_no += 1;
    }

    // footer_xml — Python `:311-313`.  Same strip+skip+no-increment
    // contract as header_lines.
    for line in &p.footer_lines {
        let stripped = line.trim();
        if stripped.is_empty() {
            continue;
        }
        let n = item_no.to_string();
        tag_attrs(&mut footer_buf, "L", &[("N", &n), ("NM", stripped)]);
        close(&mut footer_buf, "L");
        item_no += 1;
    }

    // Python `:321-324` concat order: header is already in `out`;
    // now items + discounts + payments + footer.
    out.push_str(&items_buf);
    out.push_str(&discounts_buf);
    out.push_str(&payments_buf);
    out.push_str(&footer_buf);

    // Closing <E FN N NO SM TS> per ФСКО Table 23 / Python
    // `_build_e_element`.  W4-Z1 piece 5: when `tax_summaries` is
    // non-empty, emit `<TX>` CHILD elements inside `<E>` before close.
    let e_n = item_no.to_string();
    let e_no = p.local_number.to_string();
    let e_sm = p.total_sum.to_string();
    tag_attrs(
        out,
        "E",
        &[
            ("FN", &h.fiscal_number),
            ("N", &e_n),
            ("NO", &e_no),
            ("SM", &e_sm),
            ("TS", &h.ts_str),
        ],
    );
    // <TX> tax-group children inside <E> (Python `_build_e_element:514-530`).
    // Python sorts by STRINGIFIED key (`sorted(group_sums.items())`,
    // keys are str at `:214`), so "1" < "10" < "2" lex order — NOT
    // numeric.  Mirror exactly for byte parity.
    let mut sorted_tx: Vec<&TaxGroupSummary> = p.tax_summaries.iter().collect();
    sorted_tx.sort_by_key(|t| t.tx.to_string());
    for ts in sorted_tx {
        let tx_str = ts.tx.to_string();
        let txsm_str = ts.txsm.to_string();
        let dtsm_str = ts.dtsm.to_string();
        let txal_str = ts.txal.to_string();
        let txty_str = ts.txty.to_string();
        tag_attrs(
            out,
            "TX",
            &[
                ("DTPR", &ts.dtpr),
                ("DTSM", &dtsm_str),
                ("TX", &tx_str),
                ("TXAL", &txal_str),
                ("TXPR", &ts.txpr),
                ("TXSM", &txsm_str),
                ("TXTY", &txty_str),
            ],
        );
        close(out, "TX");
    }
    close(out, "E");
    close(out, "C");
    tag_text(out, "TS", &h.ts_str);
    close(out, "DAT");
    emit_mac(out, h);
    close(out, "RQ");
    Ok(())
}

/// AUDIT4-IMP-2 helper — replicates Python `:220` zero-skip
/// semantics across both per-item LineAdjustment and check-level
/// CheckLevelAdjustment.  Python skips on `d_value == 0` where
/// d_value is the SOURCE rate (for percent mode) or value (for
/// value mode).  Rust must mirror:
///
///   - VALUE mode: skip when resolved sum is 0.
///   - PERCENT mode: skip only when the rate STRING parses to
///     0.0 (or is absent).  A non-zero rate producing a zero
///     resolved sum (free gift + 10%) MUST emit, per Python.
fn adjustment_is_zero(mode: AdjustmentMode, sum: i64, percent: Option<&str>) -> bool {
    match mode {
        AdjustmentMode::Value => sum == 0,
        AdjustmentMode::Percent => percent
            .and_then(|s| s.trim().parse::<f64>().ok())
            .map(|r| r == 0.0)
            .unwrap_or(true),
    }
}

fn emit_z_report(p: &ZReportPayload, out: &mut String) {
    let h = &p.header;
    open_rq(out, h);
    let di = p.local_number.to_string();
    open_dat(out, h, &di);
    let zn = h.z_number.to_string();
    tag_attrs(out, "Z", &[("NO", &zn)]);

    // W4-Z1 piece 6: <TXS> per-tax-group inflow/outflow.  Python
    // `_build_z_report:438` iterates `sorted(tax_sums.keys())` where
    // keys are STRINGS at `:436` — lex sort: "1" < "10" < "2".
    // Numeric sort would diverge; mirror Python exactly for byte
    // parity.
    let mut sorted_txs: Vec<&ZReportTaxSummary> = p.tax_summaries.iter().collect();
    sorted_txs.sort_by_key(|t| t.tx.to_string());
    for ts in sorted_txs {
        let tx_str = ts.tx.to_string();
        let smi_str = ts.smi.to_string();
        let smo_str = ts.smo.to_string();
        if ts.tx_short_form {
            // Fallback form (Python `:458`): SMI, SMO, TX only.
            tag_attrs(
                out,
                "TXS",
                &[("SMI", &smi_str), ("SMO", &smo_str), ("TX", &tx_str)],
            );
        } else {
            // Full form (Python `:452-456`): DTPR, SMI, SMO, TS, TX,
            // TXAL, TXI, TXO, TXPR, TXTY (alphabetical).
            let txal_str = ts.txal.to_string();
            let txi_str = ts.txi.to_string();
            let txo_str = ts.txo.to_string();
            let txty_str = ts.txty.to_string();
            tag_attrs(
                out,
                "TXS",
                &[
                    ("DTPR", &ts.dtpr),
                    ("SMI", &smi_str),
                    ("SMO", &smo_str),
                    ("TS", &ts.ts_prefix),
                    ("TX", &tx_str),
                    ("TXAL", &txal_str),
                    ("TXI", &txi_str),
                    ("TXO", &txo_str),
                    ("TXPR", &ts.txpr),
                    ("TXTY", &txty_str),
                ],
            );
        }
        close(out, "TXS");
    }

    // Z body — per-payment-type <M NM SMI SMO T>.  Python iterates
    // `sorted(payment_sums.keys())`; we mirror that by sorting the
    // caller-supplied vec by `name` so the wire output is
    // deterministic regardless of caller insertion order.
    let mut sorted_payments: Vec<&ZReportPaymentSum> = p.payments.iter().collect();
    sorted_payments.sort_by(|a, b| a.name.cmp(&b.name));
    for pay in sorted_payments {
        let smi = pay.sum_in.to_string();
        let smo = pay.sum_out.to_string();
        tag_attrs(
            out,
            "M",
            &[
                ("NM", &pay.name),
                ("SMI", &smi),
                ("SMO", &smo),
                ("T", &pay.type_code),
            ],
        );
        close(out, "M");
    }

    // W4-Z1 piece 6: <IO> service in/out summaries.  Python `:470-477`
    // — NM, SMI, SMO, T="0" alphabetical.  Sort by name ascending.
    let mut sorted_io: Vec<&ZReportServiceSum> = p.service_sums.iter().collect();
    sorted_io.sort_by(|a, b| a.name.cmp(&b.name));
    for io in sorted_io {
        let smi = io.sum_in.to_string();
        let smo = io.sum_out.to_string();
        tag_attrs(
            out,
            "IO",
            &[("NM", &io.name), ("SMI", &smi), ("SMO", &smo), ("T", "0")],
        );
        close(out, "IO");
    }

    // Z body — <NC NI NO>.  Always emitted in W4 first-round shape
    // (Python emits if `check_count` is a dict, which it always is
    // in our typed payload).
    let ni = p.check_count.sell_count.to_string();
    let no = p.check_count.return_count.to_string();
    tag_attrs(out, "NC", &[("NI", &ni), ("NO", &no)]);
    close(out, "NC");

    // W4-Z1 piece 6: <EPZ> electronic-payment-instrument totals.
    // Python `:484-491` — EPC, EPCS, EPSM (already alphabetical).
    if let Some(epz) = &p.epz {
        let epc = epz.epc.to_string();
        let epcs = epz.epcs.to_string();
        let epsm = epz.epsm.to_string();
        tag_attrs(
            out,
            "EPZ",
            &[("EPC", &epc), ("EPCS", &epcs), ("EPSM", &epsm)],
        );
        close(out, "EPZ");
    }

    close(out, "Z");
    tag_text(out, "TS", &h.ts_str);
    close(out, "DAT");
    emit_mac(out, h);
    close(out, "RQ");
}

// ─── Tag-emission primitives (mirror Python _tag/_xml_escape) ─────────

/// Emit `<RQ NDv="..." PrV="..." V="1">`.  Attribute order is
/// alphabetical (per `_tag` in Python), which for `NDv / PrV / V` is
/// already lex-correct (`N < P < V` in ASCII).
fn open_rq(out: &mut String, h: &DocumentHeader) {
    tag_attrs(
        out,
        "RQ",
        &[
            ("NDv", &h.device_name),
            ("PrV", &h.device_version),
            ("V", "1"),
        ],
    );
}

/// Emit `<DAT DI="..." FN="..." TN="..." V="1" ZN="...">`.  DI is
/// caller-supplied: `"0"` for SHIFT_OPEN, the local_number for
/// SELL/RETURN/Z_REPORT.  Order is alphabetical.
fn open_dat(out: &mut String, h: &DocumentHeader, di: &str) {
    let zn = h.z_number.to_string();
    tag_attrs(
        out,
        "DAT",
        &[
            ("DI", di),
            ("FN", &h.fiscal_number),
            ("TN", &h.tax_number),
            ("V", "1"),
            ("ZN", &zn),
        ],
    );
}

/// Open a tag with attributes, sorted alphabetically by name.
/// Mirrors the Python `_tag` helper's `sorted(attrs.items())`.
/// Caller is responsible for the matching `close()` — Python never
/// emits self-closing tags here, and neither do we.
fn tag_attrs(out: &mut String, name: &str, attrs: &[(&str, &str)]) {
    let mut sorted: Vec<(&str, &str)> = attrs.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    out.push('<');
    out.push_str(name);
    for (k, v) in sorted {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        push_escaped(out, v);
        out.push('"');
    }
    out.push('>');
}

/// W4-Z1: emit a `<D>` (discount) or `<S>` (surcharge) sibling
/// element after a `<P>` line.  Per Python `dps_xml.py:217-244`
/// per-item-adjustment branch + spec §2 `<D>`/`<S>` shape.
///
/// `parent_n` — the sibling `<P>`'s `N` (sequence number) referenced
/// via `NI`.  `self_n` — this adjustment's own `N`.
fn emit_line_adjustment(
    out: &mut String,
    tag_name: &str, // "D" or "S"
    adj: &LineAdjustment,
    self_n: u32,
    parent_n: u32,
) {
    let self_n_str = self_n.to_string();
    let parent_n_str = parent_n.to_string();
    let sm_str = adj.sum.to_string();
    let (ty_str, percent_str) = match adj.mode {
        AdjustmentMode::Value => ("0", None),
        AdjustmentMode::Percent => ("1", adj.percent.as_deref()),
    };

    let mut attrs: Vec<(&str, &str)> = Vec::with_capacity(8);
    attrs.push(("N", &self_n_str));
    attrs.push(("NI", &parent_n_str));
    attrs.push(("SM", &sm_str));
    attrs.push(("TR", "0")); // per-item flag (TR=1 is check-level form)
    attrs.push(("TY", ty_str));
    if let Some(pr) = percent_str {
        attrs.push(("PR", pr));
    }
    if let Some(name) = adj.name.as_deref() {
        attrs.push(("NM", name));
    }
    if let Some(dn) = adj.privilege.as_deref() {
        attrs.push(("DN", dn));
    }
    if let Some(tx) = adj.tax_code.as_deref() {
        attrs.push(("TX", tx));
    }
    tag_attrs(out, tag_name, &attrs);
    close(out, tag_name);
}

/// Emit `<name>content</name>`.  Used for `<TS>` and `<MAC>`.
///
/// **Content is NOT XML-escaped** — mirrors Python `_tag` exactly:
/// the Python helper f-strings `{open_tag}{content}` directly, so
/// `&` / `<` / `>` in a hex-MAC or a YYYYMMDDHHMMSS timestamp are
/// passed through verbatim.  In practice TS is always digits and
/// MAC is always hex, so the question is academic — but we match
/// the oracle's behaviour for byte-equivalence.  If a future text
/// content needs escaping, callers should pre-escape and emit via
/// `tag_attrs` instead.
fn tag_text(out: &mut String, name: &str, content: &str) {
    out.push('<');
    out.push_str(name);
    out.push('>');
    out.push_str(content);
    out.push_str("</");
    out.push_str(name);
    out.push('>');
}

/// B9 — emit the closing `<MAC>` element.
///
/// - ONLINE (`h.mac_id == None`): bare `<MAC>{previous_hash}</MAC>`,
///   byte-identical to the pre-B9 `tag_text(out, "MAC", &h.previous_hash)`
///   (proven live; the goldens are all online).
/// - OFFLINE (`h.mac_id == Some(dps_code)`): `<MAC ID='{dps_code}'>{previous_hash}</MAC>`.
///
/// The single-quoted `ID` attribute mirrors the DPS/WebCheck ground-truth
/// (`NumbersOfflineUse.cs:97` / `StringXML.cs:1438`).  Content (the hash)
/// stays RAW — matching `tag_text`'s Python-`_tag` no-escape contract; the
/// `dps_code` is an opaque base64url-shaped token with no XML-special
/// chars, so it is emitted verbatim too (WebCheck does the same — the
/// signed hash must cover the exact bytes DPS reads back).
fn emit_mac(out: &mut String, h: &DocumentHeader) {
    match &h.mac_id {
        None => tag_text(out, "MAC", &h.previous_hash),
        Some(id) => {
            out.push_str("<MAC ID='");
            out.push_str(id);
            out.push_str("'>");
            out.push_str(&h.previous_hash);
            out.push_str("</MAC>");
        }
    }
}

/// Emit `</name>`.
fn close(out: &mut String, name: &str) {
    out.push_str("</");
    out.push_str(name);
    out.push('>');
}

/// Mirror of Python `_xml_escape`:
///   `&` → `&amp;`, `"` → `&quot;`, `<` → `&lt;`, `>` → `&gt;`.
/// Note: `'` (apostrophe) is NOT escaped — Python doesn't escape it
/// either; this is a documented difference from generic XML
/// canonicalisation and a place a generic XML crate would diverge.
fn push_escaped(out: &mut String, s: &str) {
    let mut buf = [0u8; 4];
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push_str(c.encode_utf8(&mut buf)),
        }
    }
}

// ─── Unit tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> DocumentHeader {
        DocumentHeader::with_defaults("1234567890", "12345678", 7, "20260506120000", "deadbeef")
    }

    fn ascii_header() -> DocumentHeader {
        let mut h = header();
        // Override the cyrillic device name so the XML is pure-ASCII
        // for tests that assert string-shape via `from_utf8`.
        h.device_name = "ASCII_RRO".into();
        h
    }

    fn render_ascii(doc: &CanonicalDoc) -> String {
        let bytes = build_canonical_xml(doc).expect("build ok");
        String::from_utf8(bytes).expect("ASCII fixture must round-trip via UTF-8")
    }

    fn one_check_item() -> CheckItem {
        CheckItem {
            code: "ART-1".into(),
            name: "Apple".into(),
            price: 1500,
            quantity: 1000,
            sum: 1500,
            ..Default::default()
        }
    }

    fn one_cash_payment() -> CheckPayment {
        CheckPayment {
            name: "CASH".into(),
            sum: 1500,
            type_code: "0".into(),
            ..Default::default()
        }
    }

    // ─── Attribute alphabetical ordering ──────────────────────────

    #[test]
    fn attributes_emitted_alphabetically_by_name() {
        let mut out = String::new();
        tag_attrs(&mut out, "T", &[("Z", "z"), ("A", "a"), ("M", "m")]);
        assert_eq!(out, r#"<T A="a" M="m" Z="z">"#);
    }

    #[test]
    fn dat_attrs_alphabetical_di_fn_tn_v_zn() {
        let doc = CanonicalDoc::ShiftOpen(ShiftOpenPayload {
            header: ascii_header(),
            opening_sum: 0,
        });
        let s = render_ascii(&doc);
        assert!(
            s.contains(r#"<DAT DI="0" FN="1234567890" TN="12345678" V="1" ZN="7">"#),
            "DAT attrs out of alphabetical order: {s}"
        );
    }

    #[test]
    fn rq_attrs_alphabetical_ndv_prv_v() {
        let doc = CanonicalDoc::ShiftOpen(ShiftOpenPayload {
            header: ascii_header(),
            opening_sum: 0,
        });
        let s = render_ascii(&doc);
        assert!(
            s.starts_with(r#"<RQ NDv="ASCII_RRO" PrV="1.1" V="1">"#),
            "RQ attrs out of order: {}",
            &s[..40.min(s.len())]
        );
    }

    // ─── Escaping ─────────────────────────────────────────────────

    #[test]
    fn attribute_values_escape_amp_quote_lt_gt_but_not_apostrophe() {
        let mut h = ascii_header();
        h.device_name = r#"<a&b>"c'd"#.into();
        let doc = CanonicalDoc::ShiftOpen(ShiftOpenPayload {
            header: h,
            opening_sum: 0,
        });
        let s = render_ascii(&doc);
        assert!(
            s.contains(r#"NDv="&lt;a&amp;b&gt;&quot;c'd""#),
            "attribute escape mismatch: {s}"
        );
        assert!(s.contains("c'd"), "apostrophe must NOT be escaped");
    }

    #[test]
    fn text_content_is_not_escaped_in_ts_or_mac() {
        // Python `_tag` interpolates `content` raw — any `&` or `<`
        // appearing in a TS / MAC string lands verbatim on the wire.
        // Practically TS is digits and MAC is hex, but the contract
        // matters because byte-equivalence depends on it.
        let mut h = ascii_header();
        h.previous_hash = "<dead&beef>".into();
        h.ts_str = "AB&CD".into(); // synthetic — TS is normally numeric
        let doc = CanonicalDoc::ShiftOpen(ShiftOpenPayload {
            header: h,
            opening_sum: 0,
        });
        let s = render_ascii(&doc);
        // Both raw — no escape.
        assert!(
            s.contains("<MAC><dead&beef></MAC>"),
            "MAC content must pass through raw: {s}"
        );
        assert!(
            s.contains("<TS>AB&CD</TS>"),
            "TS content must pass through raw: {s}"
        );
    }

    // ─── B9: offline `<MAC ID=...>` (signed-bytes id_offline) ─────
    //
    // Live DPS rejects an offline drain whose `<MAC>` carries no `ID`
    // attribute with `-9 "not ID in MAC"` (proven 2026-07-08,
    // `live_smoke_11`).  Offline receipts MUST emit
    // `<MAC ID='{offline_dps_code}'>{hash}</MAC>` in the SIGNED bytes
    // (WebCheck ground-truth: `NumbersOfflineUse.cs:97` /
    // `StringXML.cs:1438` — the opaque DPS code, single-quoted).  ONLINE
    // docs keep the bare `<MAC>` (proven live) — this is the regression
    // pin.  Branch on `mac_id.is_some()` (which stage_sign sets only for
    // `fs_mode='OFFLINE'`), NOT on doc-type.

    #[test]
    fn offline_shift_open_emits_mac_id_attribute() {
        let mut h = ascii_header();
        h.previous_hash = "deadbeef".into();
        h.mac_id = Some("eYme-jhnkWQ".into());
        let doc = CanonicalDoc::ShiftOpen(ShiftOpenPayload {
            header: h,
            opening_sum: 0,
        });
        let s = render_ascii(&doc);
        assert!(
            s.contains("<MAC ID='eYme-jhnkWQ'>deadbeef</MAC>"),
            "offline SHIFT_OPEN MAC must carry the opaque id_offline: {s}"
        );
    }

    #[test]
    fn offline_sell_emits_mac_id_attribute() {
        let mut h = ascii_header();
        h.previous_hash = "deadbeef".into();
        h.mac_id = Some("eYme-jhnkWQ".into());
        let doc = CanonicalDoc::Sell(CheckPayload {
            header: h,
            local_number: 1,
            items: vec![one_check_item()],
            payments: vec![one_cash_payment()],
            total_sum: 1500,
            check_level_adjustments: vec![],
            header_lines: vec![],
            footer_lines: vec![],
            tax_summaries: vec![],
        });
        let s = render_ascii(&doc);
        assert!(
            s.contains("<MAC ID='eYme-jhnkWQ'>deadbeef</MAC>"),
            "offline SELL MAC must carry the opaque id_offline: {s}"
        );
    }

    #[test]
    fn offline_z_report_emits_mac_id_attribute() {
        let mut h = ascii_header();
        h.previous_hash = "deadbeef".into();
        h.mac_id = Some("eYme-jhnkWQ".into());
        let doc = CanonicalDoc::ZReport(ZReportPayload {
            header: h,
            local_number: 1,
            tax_summaries: vec![],
            payments: vec![],
            service_sums: vec![],
            check_count: ZReportCheckCount {
                sell_count: 0,
                return_count: 0,
            },
            epz: None,
        });
        let s = render_ascii(&doc);
        assert!(
            s.contains("<MAC ID='eYme-jhnkWQ'>deadbeef</MAC>"),
            "offline Z_REPORT MAC must carry the opaque id_offline: {s}"
        );
    }

    #[test]
    fn online_mac_stays_bare_regression_pin() {
        // ONLINE (mac_id == None) MUST render the bare `<MAC>` — proven
        // live; the goldens are all online.  This is the online-unaffected
        // regression pin: an accidental attribute here would break every
        // online receipt's signed bytes.
        let mut h = ascii_header();
        h.previous_hash = "deadbeef".into();
        assert!(h.mac_id.is_none(), "with_defaults MUST leave mac_id None");
        let doc = CanonicalDoc::Sell(CheckPayload {
            header: h,
            local_number: 1,
            items: vec![one_check_item()],
            payments: vec![one_cash_payment()],
            total_sum: 1500,
            check_level_adjustments: vec![],
            header_lines: vec![],
            footer_lines: vec![],
            tax_summaries: vec![],
        });
        let s = render_ascii(&doc);
        assert!(
            s.contains("<MAC>deadbeef</MAC>"),
            "online MAC must stay bare (no ID attribute): {s}"
        );
        assert!(
            !s.contains("<MAC ID="),
            "online MAC must NOT carry an ID attribute: {s}"
        );
    }

    // ─── Per-doc shape invariants ─────────────────────────────────

    #[test]
    fn shift_open_uses_c_t_108_and_di_zero() {
        let doc = CanonicalDoc::ShiftOpen(ShiftOpenPayload {
            header: ascii_header(),
            opening_sum: 1234,
        });
        let s = render_ascii(&doc);
        assert!(s.contains(r#"<C T="108">"#), "SHIFT_OPEN must be C T=108");
        assert!(s.contains(r#"DI="0""#), "SHIFT_OPEN must force DI=0");
        assert!(
            s.contains(r#"<O N="1" SM="1234" T="0">"#),
            "SHIFT_OPEN <O> shape mismatch: {s}"
        );
        assert!(s.contains(r#"<E N="2">"#), "SHIFT_OPEN <E> missing");
    }

    #[test]
    fn sell_uses_c_t_0_with_p_m_e_body_in_python_shape() {
        let doc = CanonicalDoc::Sell(CheckPayload {
            header: ascii_header(),
            local_number: 42,
            items: vec![one_check_item()],
            payments: vec![one_cash_payment()],
            total_sum: 1500,
            check_level_adjustments: Vec::new(),
            header_lines: Vec::new(),
            footer_lines: Vec::new(),
            tax_summaries: Vec::new(),
        });
        let s = render_ascii(&doc);
        assert!(s.contains(r#"<C T="0">"#), "SELL must be C T=0");
        assert!(s.contains(r#"DI="42""#), "SELL DI must equal local_number");
        // Per-item attrs alphabetical: C, N, NM, PRC, Q, SM
        assert!(
            s.contains(r#"<P C="ART-1" N="1" NM="Apple" PRC="1500" Q="1000" SM="1500">"#),
            "SELL <P> shape mismatch: {s}"
        );
        // Per-payment attrs alphabetical: N, NM, SM, T (item_no=2 because item_no=1 was the P)
        assert!(
            s.contains(r#"<M N="2" NM="CASH" SM="1500" T="0">"#),
            "SELL <M> shape mismatch: {s}"
        );
        // Closing <E FN N NO SM TS> with N=3 (after one P + one M)
        assert!(
            s.contains(r#"<E FN="1234567890" N="3" NO="42" SM="1500" TS="20260506120000">"#),
            "SELL <E> shape mismatch: {s}"
        );
    }

    #[test]
    fn return_uses_c_t_1_with_same_body_shape_as_sell() {
        let doc = CanonicalDoc::Return(CheckPayload {
            header: ascii_header(),
            local_number: 13,
            items: vec![one_check_item()],
            payments: vec![one_cash_payment()],
            total_sum: 1500,
            check_level_adjustments: Vec::new(),
            header_lines: Vec::new(),
            footer_lines: Vec::new(),
            tax_summaries: Vec::new(),
        });
        let s = render_ascii(&doc);
        assert!(s.contains(r#"<C T="1">"#), "RETURN must be C T=1");
        assert!(s.contains(r#"DI="13""#));
        // Body shape identical to SELL.
        assert!(s.contains(r#"<P C="ART-1" N="1" NM="Apple" PRC="1500" Q="1000" SM="1500">"#));
        assert!(s.contains(r#"<M N="2" NM="CASH" SM="1500" T="0">"#));
    }

    #[test]
    fn z_report_uses_z_no_with_m_and_nc_body_and_doubles_as_close_shift() {
        // This test pins the contract that `ZReport` IS the
        // close-shift wire artifact (DPS Check.Type::ZREPORT=2;
        // WebCheck CreateDB.cs:624 doctype='80').  A future
        // contributor adding a separate ShiftClose variant will
        // need to update this test, which is the gate.
        let doc = CanonicalDoc::ZReport(ZReportPayload {
            header: ascii_header(),
            local_number: 100,
            tax_summaries: Vec::new(),
            payments: vec![ZReportPaymentSum {
                name: "CASH".into(),
                sum_in: 5000,
                sum_out: 0,
                type_code: "0".into(),
            }],
            service_sums: Vec::new(),
            check_count: ZReportCheckCount {
                sell_count: 17,
                return_count: 2,
            },
            epz: None,
        });
        let s = render_ascii(&doc);
        // <Z NO="..."> not <Z DC NO SM>.
        assert!(s.contains(r#"<Z NO="7">"#), "Z open shape mismatch: {s}");
        // <M NM SMI SMO T> alphabetical.
        assert!(
            s.contains(r#"<M NM="CASH" SMI="5000" SMO="0" T="0">"#),
            "Z<M> shape mismatch: {s}"
        );
        // <NC NI NO>.
        assert!(
            s.contains(r#"<NC NI="17" NO="2">"#),
            "Z<NC> shape mismatch: {s}"
        );
        // Z_REPORT must NOT contain a <C T=...> wrapper.
        assert!(!s.contains("<C "), "Z_REPORT must not emit a <C> tag: {s}");
    }

    // ─── B10: offline-session boundary docs (DocType 9/10) ────────────
    //
    // Byte-pinned to WebCheck `SendingOfflineChecks.cs::EndOfflineXML`
    // (`docs/webcheck_reverse_v2/WebCheck/SendingOfflineChecks.cs:290`):
    //   `<DAT ... DI='{idCh}' ...><C T='110'></C><TS>{CCD}</TS></DAT>...`
    // An EMPTY `<C>` element — NO `<O>` (unlike SHIFT_OPEN 108), NO `<E>`,
    // NO `<P>` goods.  BEGIN uses `<C T="109">`, END uses `<C T="110">`
    // (verAPI=2 wire form).  Modelled on SHIFT_OPEN's service-receipt
    // header shape (open_rq + open_dat + <C> + <TS> + <MAC>).

    #[test]
    fn offline_session_begin_uses_c_t_109_empty_service_receipt() {
        let doc = CanonicalDoc::OfflineSessionBegin(OfflineSessionBoundaryPayload {
            header: ascii_header(),
            local_number: 5,
        });
        let s = render_ascii(&doc);
        assert!(
            s.contains(r#"<C T="109">"#),
            "OFFLINE_SESSION_BEGIN must be C T=109: {s}"
        );
        assert!(s.contains(r#"DI="5""#), "BEGIN DI must equal local_number");
        // Empty <C> — closes immediately with no children (WebCheck shape).
        assert!(
            s.contains(r#"<C T="109"></C>"#),
            "BEGIN <C> must be EMPTY (no <O>/<E>/<P> children): {s}"
        );
        // Service receipt: NO goods <P>, NO opening <O>, NO closing <E>.
        assert!(!s.contains("<P "), "BEGIN must not emit goods <P>: {s}");
        assert!(!s.contains("<O "), "BEGIN must not emit <O>: {s}");
        assert!(!s.contains("<E "), "BEGIN must not emit <E>: {s}");
        // <TS> present after the <C>.
        assert!(
            s.contains("<TS>20260506120000</TS>"),
            "BEGIN <TS> missing: {s}"
        );
    }

    #[test]
    fn offline_session_end_uses_c_t_110_empty_service_receipt() {
        let doc = CanonicalDoc::OfflineSessionEnd(OfflineSessionBoundaryPayload {
            header: ascii_header(),
            local_number: 9,
        });
        let s = render_ascii(&doc);
        assert!(
            s.contains(r#"<C T="110">"#),
            "OFFLINE_SESSION_END must be C T=110: {s}"
        );
        assert!(s.contains(r#"DI="9""#), "END DI must equal local_number");
        assert!(
            s.contains(r#"<C T="110"></C>"#),
            "END <C> must be EMPTY (no <O>/<E>/<P> children): {s}"
        );
        assert!(!s.contains("<P "), "END must not emit goods <P>: {s}");
        assert!(!s.contains("<O "), "END must not emit <O>: {s}");
        assert!(!s.contains("<E "), "END must not emit <E>: {s}");
        assert!(
            s.contains("<TS>20260506120000</TS>"),
            "END <TS> missing: {s}"
        );
    }

    #[test]
    fn offline_session_boundary_carries_mac_id_when_offline() {
        // B9: an offline doc's header.mac_id = Some(dps_code) → <MAC ID='..'>.
        // The boundary docs are offline docs (they carry a pool code), so
        // their signed XML must carry the ID'd MAC just like an offline SELL.
        let mut h = ascii_header();
        h.previous_hash = "cafe".into();
        h.mac_id = Some("OFFLINE-CODE-9".into());
        let doc = CanonicalDoc::OfflineSessionBegin(OfflineSessionBoundaryPayload {
            header: h,
            local_number: 1,
        });
        let s = render_ascii(&doc);
        assert!(
            s.contains("<MAC ID='OFFLINE-CODE-9'>cafe</MAC>"),
            "offline boundary doc must carry <MAC ID='...'>: {s}"
        );
    }

    #[test]
    fn z_report_payments_emit_in_sorted_name_order() {
        // Python iterates `sorted(payment_sums.keys())`; Rust must
        // match regardless of caller insertion order.  Pass in
        // CARD, then CASH; expect CASH < CARD lex order ⇒ CARD
        // emits first (because `'CARD' < 'CASH'`).
        let doc = CanonicalDoc::ZReport(ZReportPayload {
            header: ascii_header(),
            local_number: 1,
            tax_summaries: Vec::new(),
            payments: vec![
                ZReportPaymentSum {
                    name: "CASH".into(),
                    sum_in: 2,
                    sum_out: 0,
                    type_code: "0".into(),
                },
                ZReportPaymentSum {
                    name: "CARD".into(),
                    sum_in: 1,
                    sum_out: 0,
                    type_code: "2".into(),
                },
            ],
            service_sums: Vec::new(),
            check_count: ZReportCheckCount {
                sell_count: 0,
                return_count: 0,
            },
            epz: None,
        });
        let s = render_ascii(&doc);
        let card_idx = s.find(r#"NM="CARD""#).expect("CARD present");
        let cash_idx = s.find(r#"NM="CASH""#).expect("CASH present");
        assert!(card_idx < cash_idx, "CARD < CASH lex order: {s}");
    }

    #[test]
    fn check_items_emit_in_input_order_with_alphabetised_attrs() {
        let doc = CanonicalDoc::Sell(CheckPayload {
            header: ascii_header(),
            local_number: 1,
            items: vec![
                CheckItem {
                    code: "CODE-1".into(),
                    name: "Apple".into(),
                    price: 100,
                    quantity: 1000,
                    sum: 100,
                    ..Default::default()
                },
                CheckItem {
                    code: "CODE-2".into(),
                    name: "Banana".into(),
                    price: 100,
                    quantity: 1000,
                    sum: 100,
                    ..Default::default()
                },
            ],
            payments: vec![one_cash_payment()],
            total_sum: 200,
            check_level_adjustments: Vec::new(),
            header_lines: Vec::new(),
            footer_lines: Vec::new(),
            tax_summaries: Vec::new(),
        });
        let s = render_ascii(&doc);
        let idx1 = s.find("CODE-1").expect("first item present");
        let idx2 = s.find("CODE-2").expect("second item present");
        assert!(idx1 < idx2, "items must emit in input order");
    }

    // ─── Determinism ──────────────────────────────────────────────

    #[test]
    fn build_is_deterministic_for_same_input() {
        let doc = CanonicalDoc::Sell(CheckPayload {
            header: ascii_header(),
            local_number: 1,
            items: vec![one_check_item()],
            payments: vec![one_cash_payment()],
            total_sum: 1500,
            check_level_adjustments: Vec::new(),
            header_lines: Vec::new(),
            footer_lines: Vec::new(),
            tax_summaries: Vec::new(),
        });
        let a = build_canonical_xml(&doc).unwrap();
        let b = build_canonical_xml(&doc).unwrap();
        assert_eq!(a, b);
    }

    // ─── cp1251 byte mapping at the builder boundary ──────────────

    #[test]
    fn default_device_name_encodes_to_known_cp1251_bytes() {
        let doc = CanonicalDoc::ShiftOpen(ShiftOpenPayload {
            header: header(),
            opening_sum: 0,
        });
        let bytes = build_canonical_xml(&doc).expect("default cyrillic must encode");
        let expected: &[u8] = &[0xCF, 0xD0, 0xCE, 0x5F, 0xEA, 0xE0, 0xF1, 0xE0];
        let _ = bytes
            .windows(expected.len())
            .position(|w| w == expected)
            .expect("ПРО_каса bytes must appear in cp1251 output");
    }

    #[test]
    fn ukrainian_name_encodes_via_cp1251() {
        // Item name with Ukrainian glyphs must round-trip; this is
        // the guard that catches cp1251 coverage drift.
        let doc = CanonicalDoc::Sell(CheckPayload {
            header: ascii_header(),
            local_number: 1,
            items: vec![CheckItem {
                code: "ART".into(),
                name: "Їжа".into(), // Ї (0xAF) + ж (0xE6) + а (0xE0)
                price: 100,
                quantity: 1000,
                sum: 100,
                ..Default::default()
            }],
            payments: vec![one_cash_payment()],
            total_sum: 100,
            check_level_adjustments: Vec::new(),
            header_lines: Vec::new(),
            footer_lines: Vec::new(),
            tax_summaries: Vec::new(),
        });
        let bytes = build_canonical_xml(&doc).expect("ukrainian must encode");
        // Find the cp1251 bytes for "Їжа" inside the wire output.
        let needle: &[u8] = &[0xAF, 0xE6, 0xE0];
        assert!(
            bytes.windows(needle.len()).any(|w| w == needle),
            "Ukrainian name must round-trip through cp1251"
        );
    }

    #[test]
    fn unmappable_char_in_device_name_returns_typed_error() {
        let mut h = ascii_header();
        h.device_name = "RRO😀".into();
        let doc = CanonicalDoc::ShiftOpen(ShiftOpenPayload {
            header: h,
            opening_sum: 0,
        });
        let err = build_canonical_xml(&doc).expect_err("emoji must be unmappable");
        assert!(
            matches!(err, XmlBuildError::Cp1251Unmappable(c, _) if c == '😀'),
            "expected typed Cp1251Unmappable for emoji, got {err:?}"
        );
    }
}
