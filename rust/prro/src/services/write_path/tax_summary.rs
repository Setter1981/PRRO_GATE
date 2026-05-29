//! W4-Z1 piece 7 — pure tax-summary aggregation helper.
//!
//! Operator architectural pin: aggregation lives in the conversion
//! layer (NOT in adapters), implemented as a PURE function with no
//! DB / network side effects.  Caller passes a pre-resolved
//! `HashMap<i64, ResolvedTaxGroup>` snapshot (loaded once per
//! receipt from `driver_tax_mapping` repo by piece-7 caller in
//! `check_payload_from`).
//!
//! Rationale:
//!   1. `tax_summaries` are DERIVED data, not WIRE data.  Adapter
//!      transcribes only what's on the wire (items + rates); the
//!      builder owns derivation so item.sum and TXSM/DTSM cannot
//!      diverge.
//!   2. Adapter stays thin — ФСКО tax math (`calc_tax`) is NOT
//!      smeared across Maria/REST/XML-RPC.
//!   3. Smallest blast radius: the helper is unit-testable in
//!      isolation; integration tests exercise check_payload_from.
//!
//! Behaviour per Python `_build_e_element:514-530`:
//!   - Group items by `tax_group_1`; items without a group are
//!     skipped (no <TX> contribution).
//!   - For each group, lookup `tax_groups`; on MISS → skip (per
//!     `:516-518 if group is None: continue`).  Note: this is
//!     check-level (<E><TX>) semantics.  Z-report (<TXS>) uses a
//!     fallback short-form for unknown groups — that path is a
//!     separate helper (not provided here).
//!   - On HIT → call `calc_tax(group_sum, txpr, dtpr, txal)` and
//!     emit one `TaxGroupSummary`.
//!
//! Ordering: returns summaries in numeric `tx` ascending.  The
//! emitter (`emit_check`) applies Python-parity lex-by-stringified
//! sort on top — both orderings are deterministic; the helper's
//! choice is irrelevant to wire output.

use std::collections::BTreeMap;
use std::collections::HashMap;

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::xml::{calc_tax, CalcTaxError, CheckItem, TaxGroupSummary};

// ─── W4-Z2a foundation: TaxResolutionSnapshot + bps helpers ────────

/// W4-Z2a piece 1 — per-receipt pinned tax-resolution snapshot.
///
/// Stored in `signing_config_snapshots` table (main pool) — append-only
/// ledger of every UNIQUE `(fn, driver_id, payload_sha256)` triple seen
/// by any signed receipt.  See [[project_m4_w4_z2a_locked_design]].
///
/// **Rates stored as integer basis points (i64)**, NOT f64.  This
/// removes float/locale/format drift from canonical hash + decouples
/// snapshot canonical bytes from XML wire format.  Conversion to
/// `f64` happens at calc_tax site; conversion to 2-decimal string
/// happens at XML emission site.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TaxResolutionSnapshot {
    /// Schema version — locks payload shape for future-proofing.
    /// "check_tax_mapping_v1" is the W4-Z2a launch shape.  Future
    /// EVPZ/Z-report extensions get distinct kinds.
    ///
    /// External mid-review IMP-3: field is `pub(crate)` so external
    /// callers cannot inject non-V1 / malformed JSON content via
    /// struct-literal syntax.  Constructors hardcode `KIND_V1`;
    /// `get_by_id` post-load validates via `is_v1()` and surfaces
    /// `UnsupportedKind` for any drift.
    pub(crate) kind: String,
    /// Resolved canonical tax-group rates, sorted ascending by `tx`
    /// in canonical bytes.
    pub(crate) groups: Vec<ResolvedTaxGroupBps>,
    /// External mid-review CRIT-1 — driver-side translation table.
    /// Each entry maps a driver-specific `tax_group_1` value (as
    /// received from W3 DTO `FiscalLine.tax_group_1: u8`) to a
    /// canonical TX number that lives in `groups` (and ultimately
    /// becomes the `<TX TX=...>` wire attribute).  Without this,
    /// adapters would carry the driver's local code straight into
    /// `<P TX="...">`, violating ФСКО canonical-TX requirement
    /// + W4 "adapters stay thin, conversion-layer owns translation"
    /// operator pin.
    ///
    /// Empty Vec means no driver-side translation is configured
    /// (1:1 identity mapping assumed — caller's driver_number IS
    /// the canonical tx_num).  This is the typical pilot config
    /// where `tax_groups.tx_num` matches the POS-side coding.
    pub(crate) driver_mapping: Vec<DriverNumberMapping>,
}

/// External mid-review CRIT-1 — driver→canonical translation entry.
/// Sourced from `driver_tax_mapping` repo (rows `is_active=1` for
/// the receipt's `driver_id`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DriverNumberMapping {
    /// `driver_tax_mapping.driver_number` — the value POS sends
    /// in `FiscalLine.tax_group_1` (and the JSON shim's
    /// `tax_group_1` field at stage_sign parse_payload time).
    pub driver_number: i64,
    /// `driver_tax_mapping.canonical_tx_num` — the canonical TX
    /// that resolves into `groups[].tx`, ends up on the wire as
    /// `<P TX="...">` and `<TX TX="...">`.
    pub canonical_tx_num: i64,
}

/// BPS-storage variant of [`ResolvedTaxGroup`].  Used for canonical
/// snapshot payload.  Converted to `ResolvedTaxGroup` (f64) via
/// [`TaxResolutionSnapshot::to_calc_map`] at calc_tax boundary.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResolvedTaxGroupBps {
    pub tx: i64,
    /// PDV rate in basis points (20.00% = 2000).
    pub txpr_bps: i64,
    /// Excise rate in basis points.
    pub dtpr_bps: i64,
    pub txal: i64,
    pub txty: i64,
}

impl TaxResolutionSnapshot {
    pub const KIND_V1: &'static str = "check_tax_mapping_v1";

    /// Construct from already-validated bps groups, with no driver
    /// mapping (1:1 identity assumption).  Used by tests and
    /// recovery placeholders.  Production stage_acquire uses
    /// [`try_from_live`] which loads driver_tax_mapping too.
    pub fn new(groups: Vec<ResolvedTaxGroupBps>) -> Self {
        Self {
            kind: Self::KIND_V1.to_string(),
            groups,
            driver_mapping: Vec::new(),
        }
    }

    /// W4-Z2a piece 14 — test / admin constructor with explicit
    /// `driver_mapping`.  Production `stage_acquire` uses
    /// [`try_from_live`] (loads both groups + mapping from
    /// `tax_groups` + `driver_tax_mapping` repos).  Exposed pub so
    /// regression tests and future admin CLIs can construct
    /// snapshots with non-identity driver mappings without
    /// reaching for the live-config loader path.
    pub fn with_driver_mapping(
        groups: Vec<ResolvedTaxGroupBps>,
        driver_mapping: Vec<DriverNumberMapping>,
    ) -> Self {
        Self {
            kind: Self::KIND_V1.to_string(),
            groups,
            driver_mapping,
        }
    }

    /// External mid-review IMP-3 — accessor since `kind` is now
    /// `pub(crate)`.  Returns the snapshot's schema-version tag.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// External mid-review IMP-3 — V1 validation called by
    /// `signing_config_snapshots::get_by_id` on every read.  Future
    /// schema bumps (kind_v2, evpz_v1) will need a distinct accessor
    /// + parsing path; today rejecting non-V1 is fail-loud.
    pub fn is_v1(&self) -> bool {
        self.kind == Self::KIND_V1
    }

    pub fn groups(&self) -> &[ResolvedTaxGroupBps] {
        &self.groups
    }

    /// External mid-review CRIT-1 — driver-side mappings used at
    /// `check_payload_from` translation step.
    pub fn driver_mapping(&self) -> &[DriverNumberMapping] {
        &self.driver_mapping
    }

    /// External mid-review CRIT-1 — resolve driver-side
    /// `tax_group_1` (as received in W3 DTO / JSON shim) to
    /// canonical tx_num.  Returns `None` if driver_mapping is
    /// empty (1:1 identity assumption — caller's tax_group_1 IS
    /// canonical) OR if the lookup misses (operator should treat
    /// miss as `RequiresManualReconciliation`).
    pub fn resolve_driver_number(&self, driver_number: i64) -> Option<i64> {
        if self.driver_mapping.is_empty() {
            // 1:1 identity: assume driver_number IS canonical tx_num.
            // (Pre-pilot common case where POS coding matches ФСКО.)
            return Some(driver_number);
        }
        self.driver_mapping
            .iter()
            .find(|m| m.driver_number == driver_number)
            .map(|m| m.canonical_tx_num)
    }

    /// Construct from live (REAL/f64) tax_groups config + driver
    /// mapping rows with strict round-trip validation.
    pub fn try_from_live(
        rows: Vec<(String, i64, f64, f64, i64, i64)>,
        driver_mappings: Vec<DriverNumberMapping>,
    ) -> Result<Self, SnapshotBuildError> {
        let mut groups = Vec::with_capacity(rows.len());
        for (_fn_id, tx, txpr, dtpr, txal, txty) in rows {
            let txpr_bps = validate_rate_to_bps(txpr, "txpr", tx)?;
            let dtpr_bps = validate_rate_to_bps(dtpr, "dtpr", tx)?;
            groups.push(ResolvedTaxGroupBps {
                tx,
                txpr_bps,
                dtpr_bps,
                txal,
                txty,
            });
        }
        Ok(Self {
            kind: Self::KIND_V1.to_string(),
            groups,
            driver_mapping: driver_mappings,
        })
    }

    /// Canonical bytes per locked design: deterministic JSON with
    /// groups + driver_mapping sorted ascending, sorted top-level
    /// keys (alphabetical: driver_mapping / groups / kind), no
    /// whitespace.  Any drift breaks `payload_sha256` verify-on-read
    /// for every existing snapshot row — treat modifications as
    /// fiscal-compat break.  Pinned hash test locks the format.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut sorted_groups: Vec<&ResolvedTaxGroupBps> = self.groups.iter().collect();
        sorted_groups.sort_by_key(|g| g.tx);
        let mut sorted_mapping: Vec<&DriverNumberMapping> = self.driver_mapping.iter().collect();
        sorted_mapping.sort_by_key(|m| m.driver_number);

        let mut out = Vec::with_capacity(384);
        // Top-level alphabetical: driver_mapping → groups → kind.
        out.extend_from_slice(b"{\"driver_mapping\":[");
        for (i, m) in sorted_mapping.iter().enumerate() {
            if i > 0 {
                out.push(b',');
            }
            out.extend_from_slice(b"{\"canonical_tx_num\":");
            out.extend_from_slice(m.canonical_tx_num.to_string().as_bytes());
            out.extend_from_slice(b",\"driver_number\":");
            out.extend_from_slice(m.driver_number.to_string().as_bytes());
            out.push(b'}');
        }
        out.extend_from_slice(b"],\"groups\":[");
        for (i, g) in sorted_groups.iter().enumerate() {
            if i > 0 {
                out.push(b',');
            }
            out.extend_from_slice(b"{\"dtpr_bps\":");
            out.extend_from_slice(g.dtpr_bps.to_string().as_bytes());
            out.extend_from_slice(b",\"tx\":");
            out.extend_from_slice(g.tx.to_string().as_bytes());
            out.extend_from_slice(b",\"txal\":");
            out.extend_from_slice(g.txal.to_string().as_bytes());
            out.extend_from_slice(b",\"txpr_bps\":");
            out.extend_from_slice(g.txpr_bps.to_string().as_bytes());
            out.extend_from_slice(b",\"txty\":");
            out.extend_from_slice(g.txty.to_string().as_bytes());
            out.push(b'}');
        }
        out.extend_from_slice(b"],\"kind\":\"");
        out.extend_from_slice(self.kind.as_bytes());
        out.extend_from_slice(b"\"}");
        out
    }

    /// SHA256 of [`canonical_bytes`].
    pub fn sha256(&self) -> [u8; 32] {
        let digest = Sha256::digest(self.canonical_bytes());
        let mut out = [0u8; 32];
        out.copy_from_slice(digest.as_slice());
        out
    }

    /// Bridge to W4-Z1 [`derive_check_tax_summaries`]: converts bps
    /// → f64 at the calc_tax boundary.  XML wire rate strings come
    /// from [`bps_to_decimal_string`] applied to the bps directly
    /// (NOT format!("{:.2}", f64) — avoids decimal-vs-float drift).
    pub fn to_calc_map(&self) -> HashMap<i64, ResolvedTaxGroup> {
        self.groups
            .iter()
            .map(|g| {
                (
                    g.tx,
                    ResolvedTaxGroup {
                        tx: g.tx,
                        txpr: bps_to_f64(g.txpr_bps),
                        dtpr: bps_to_f64(g.dtpr_bps),
                        txal: g.txal,
                        txty: g.txty,
                    },
                )
            })
            .collect()
    }
}

/// Convert basis points to f64 percentage (2000 → 20.0).
#[inline]
pub fn bps_to_f64(bps: i64) -> f64 {
    bps as f64 / 100.0
}

/// Convert basis points to exact 2-decimal-place wire string
/// (2000 → "20.00", 75 → "0.75", 0 → "0.00").  Used by XML emitter
/// for TXPR/DTPR attribute formatting — bypasses any float→string
/// formatting drift.
pub fn bps_to_decimal_string(bps: i64) -> String {
    let integer = bps / 100;
    let fractional = (bps % 100).abs();
    if bps < 0 {
        format!("-{}.{:02}", integer.abs(), fractional)
    } else {
        format!("{}.{:02}", integer, fractional)
    }
}

/// Validate an f64 rate (live config input) and convert to bps with
/// strict round-trip requirement.  Reasoning + rules pinned in
/// memory `project_m4_w4_z2a_locked_design.md`.
fn validate_rate_to_bps(
    rate: f64,
    field: &'static str,
    tx: i64,
) -> Result<i64, SnapshotBuildError> {
    if !rate.is_finite() {
        return Err(SnapshotBuildError::RateNotFinite { field, tx, rate });
    }
    if rate < 0.0 {
        return Err(SnapshotBuildError::NegativeRate { field, tx, rate });
    }
    // Multiply by 100 and check exact integer round-trip.
    let scaled = rate * 100.0;
    let rounded = scaled.round() as i64;
    // Tolerance: f64 representation error for `rate * 100.0` on common
    // 2dp values is ~1e-13 (e.g. 19.99 → 1998.9999999999998).
    // Using `f64::EPSILON` (~2.22e-16) rejects perfectly valid rates
    // like 19.99, 10.05, etc — see mid-review CRIT-1.  Use `1e-9` —
    // five orders of magnitude tighter than the 2dp resolution we
    // care about, but loose enough for IEEE-754 representation drift.
    const TWO_DP_TOLERANCE: f64 = 1e-9;
    if (scaled - rounded as f64).abs() > TWO_DP_TOLERANCE {
        return Err(SnapshotBuildError::RateNotRoundTrippable { field, tx, rate });
    }
    Ok(rounded)
}

/// Errors during [`TaxResolutionSnapshot::try_from_live`] validation.
#[derive(Debug, Error, PartialEq)]
pub enum SnapshotBuildError {
    #[error("snapshot build: {field} rate not finite (tx={tx}, rate={rate})")]
    RateNotFinite {
        field: &'static str,
        tx: i64,
        rate: f64,
    },
    #[error("snapshot build: {field} rate negative (tx={tx}, rate={rate})")]
    NegativeRate {
        field: &'static str,
        tx: i64,
        rate: f64,
    },
    #[error("snapshot build: {field} rate not round-trippable to 2dp (tx={tx}, rate={rate}) — admin config must use rates representable as 2 decimal places")]
    RateNotRoundTrippable {
        field: &'static str,
        tx: i64,
        rate: f64,
    },
}

// ─── End W4-Z2a foundation ─────────────────────────────────────────

/// Caller-resolved tax group snapshot (already translated through
/// `driver_tax_mapping`).  Fields mirror the ФСКО TX attributes.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTaxGroup {
    /// Canonical tax-group number (1..12 per ФСКО + 0 + -1 per
    /// `feedback_tax_groups_real` memory).
    pub tx: i64,
    /// PDV rate, as f64 (caller pre-validates finiteness).
    pub txpr: f64,
    /// Excise rate.
    pub dtpr: f64,
    /// Tax algorithm (0=VAT-included, 1=excise pre-VAT, 2=excise
    /// post-VAT, 3=per-unit excise [not supported pre-pilot]).
    pub txal: i64,
    /// Tax type (default 0 = standard).
    pub txty: i64,
}

/// AUDIT5-IMP-3 (B) — renamed from `derive_tax_summaries` to bind
/// the contract to CHECK-level `<E><TX>` semantics: unknown groups
/// are SKIPPED (no `<TX>` for that tax_id).  Z-report `<TXS>` uses
/// a DIFFERENT contract (short-form fallback with only `SMI/SMO/TX`
/// per Python `:444-458`) and MUST use a separate helper.
///
/// Aggregate items by `tax_group_1`, resolve each group via the
/// pre-passed map, and compute `TaxGroupSummary` entries.
///
/// Items with `tax_group_1 == None` are excluded from aggregation.
///
/// AUDIT5-CRIT-1 — fail-closed guard: when ANY item carries
/// `tax_group_1 == Some(_)` but `tax_groups` is empty (pre-W4-Z2
/// `driver_tax_mapping` wiring), return `TaxMappingNotWired`
/// rather than silently emit no `<TX>` (which would be a silent
/// production data loss invisible to byte goldens).
///
/// AUDIT5-IMP-1/3 — checked_add on per-group accumulation; surface
/// `AggregationOverflow` rather than panic / wrap.
pub fn derive_check_tax_summaries(
    items: &[CheckItem],
    tax_groups: &HashMap<i64, ResolvedTaxGroup>,
) -> Result<Vec<TaxGroupSummary>, CalcTaxError> {
    // BTreeMap → deterministic numeric-ascending iteration order.
    let mut group_sums: BTreeMap<i64, i64> = BTreeMap::new();
    for item in items {
        if let Some(tg) = item.tax_group_1 {
            let entry = group_sums.entry(tg).or_insert(0);
            *entry = entry
                .checked_add(item.sum)
                .ok_or(CalcTaxError::AggregationOverflow { tax_group: tg })?;
        }
    }

    // AUDIT5-CRIT-1 fail-closed: items reference tax_group_1 but
    // map is empty → silent <TX> drop hazard.  Surface up-front.
    if !group_sums.is_empty() && tax_groups.is_empty() {
        return Err(CalcTaxError::TaxMappingNotWired {
            referenced_groups: group_sums.keys().copied().collect(),
        });
    }

    let mut summaries = Vec::with_capacity(group_sums.len());
    for (tx, group_sum) in group_sums {
        let group = match tax_groups.get(&tx) {
            Some(g) => g,
            None => continue, // Python `:516-518` skip on miss
        };
        let (txsm, dtsm) = calc_tax(group_sum, group.txpr, group.dtpr, group.txal)?;
        summaries.push(TaxGroupSummary {
            tx: group.tx,
            txpr: format!("{:.2}", group.txpr),
            txsm,
            dtpr: format!("{:.2}", group.dtpr),
            dtsm,
            txal: group.txal,
            txty: group.txty,
        });
    }
    Ok(summaries)
}
