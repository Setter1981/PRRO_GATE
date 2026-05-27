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

use crate::xml::{calc_tax, CalcTaxError, CheckItem, TaxGroupSummary};

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

/// AUDIT5-IMP-3 (B) — deprecated alias for transitional builds.
/// Caller migration will land alongside W4-Z2 dispatcher work.
#[deprecated(note = "use derive_check_tax_summaries; Z-report needs a separate helper")]
pub fn derive_tax_summaries(
    items: &[CheckItem],
    tax_groups: &HashMap<i64, ResolvedTaxGroup>,
) -> Result<Vec<TaxGroupSummary>, CalcTaxError> {
    derive_check_tax_summaries(items, tax_groups)
}
