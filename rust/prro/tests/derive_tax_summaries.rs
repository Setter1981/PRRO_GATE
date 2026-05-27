//! W4-Z1 piece 7 — `derive_tax_summaries` pure helper.
//!
//! Operator architectural decision: aggregation lives in conversion
//! layer (NOT in adapters), implemented as a PURE helper:
//!   - input: `&[CheckItem]` (xml types) + `&HashMap<i64,
//!     ResolvedTaxGroup>` (caller-passed snapshot, no DB reads).
//!   - output: `Result<Vec<TaxGroupSummary>, CalcTaxError>`.
//!
//! Reasons:
//!   1. tax_summaries are derived data, not wire data — adapter must
//!      not transcribe them (would create two sources of truth that
//!      can diverge).
//!   2. Adapter stays thin; ФСКО tax math centralized.
//!   3. check_payload_from has full context; smallest blast radius.
//!   4. Tests stay direct: items + tax-group map → summaries.
//!
//! Contract details (Python `_build_e_element:514-530`):
//!   - Group items by `tax_group_1`.
//!   - For each group, lookup tax_groups; on miss → SKIP (per check
//!     <E><TX>; Z-report short-form is a separate path).
//!   - On hit → call `calc_tax(group_sum, txpr, dtpr, txal)` and
//!     emit one TaxGroupSummary.
//!   - Items with `tax_group_1 == None` are NOT aggregated.

use std::collections::HashMap;

use prro::services::write_path::tax_summary::{derive_check_tax_summaries, ResolvedTaxGroup};
use prro::xml::{CheckItem, TaxGroupSummary};

fn item_with_group(code: &str, sum: i64, tax_group: Option<i64>) -> CheckItem {
    CheckItem {
        code: code.into(),
        name: "Item".into(),
        price: sum,
        quantity: 1000,
        sum,
        tax_group_1: tax_group,
        ..Default::default()
    }
}

fn vat_20_group(tx: i64) -> ResolvedTaxGroup {
    ResolvedTaxGroup {
        tx,
        txpr: 20.0,
        dtpr: 0.0,
        txal: 0,
        txty: 0,
    }
}

// ─── Empty / no tax_group_1 items ─────────────────────────────────

#[test]
fn empty_items_yields_empty_summaries() {
    let summaries = derive_check_tax_summaries(&[], &HashMap::new()).expect("ok");
    assert!(summaries.is_empty());
}

#[test]
fn items_without_tax_group_1_are_ignored() {
    let items = vec![item_with_group("ART-1", 1000, None)];
    let summaries = derive_check_tax_summaries(&items, &HashMap::new()).expect("ok");
    assert!(summaries.is_empty());
}

// ─── Aggregation: group sums by tax_group_1 ───────────────────────

#[test]
fn items_in_same_group_sum_aggregated_into_single_summary() {
    let items = vec![
        item_with_group("ART-1", 1000, Some(1)),
        item_with_group("ART-2", 2000, Some(1)),
        item_with_group("ART-3", 3000, Some(1)),
    ];
    let mut groups = HashMap::new();
    groups.insert(1_i64, vat_20_group(1));

    let summaries = derive_check_tax_summaries(&items, &groups).expect("ok");
    assert_eq!(summaries.len(), 1);
    let s = &summaries[0];
    assert_eq!(s.tx, 1);
    // group_sum = 6000, calc_tax(6000, 20.0, 0.0, 0) = 6000*20/120 = 1000.
    assert_eq!(s.txsm, 1000);
    assert_eq!(s.dtsm, 0);
    assert_eq!(s.txal, 0);
    assert_eq!(s.txpr, "20.00");
    assert_eq!(s.dtpr, "0.00");
}

#[test]
fn items_in_different_groups_yield_one_summary_each() {
    let items = vec![
        item_with_group("ART-1", 6000, Some(1)),  // 20% VAT group
        item_with_group("ART-2", 1400, Some(2)),  // 7% VAT group
    ];
    let mut groups = HashMap::new();
    groups.insert(1_i64, vat_20_group(1));
    groups.insert(
        2_i64,
        ResolvedTaxGroup { tx: 2, txpr: 7.0, dtpr: 0.0, txal: 0, txty: 0 },
    );

    let summaries = derive_check_tax_summaries(&items, &groups).expect("ok");
    assert_eq!(summaries.len(), 2);
    let by_tx: HashMap<i64, &TaxGroupSummary> = summaries.iter().map(|s| (s.tx, s)).collect();
    // 6000 * 20 / 120 = 1000
    assert_eq!(by_tx[&1].txsm, 1000);
    // 1400 * 7 / 107 = 91.589 → banker's 92
    assert_eq!(by_tx[&2].txsm, 92);
}

// ─── Unknown tax group: SKIP per Python `_build_e_element:516-518` ─

#[test]
fn unknown_tax_group_is_skipped_no_summary_emitted() {
    let items = vec![
        item_with_group("ART-1", 1000, Some(1)),  // known
        item_with_group("ART-2", 2000, Some(99)), // unknown
    ];
    let mut groups = HashMap::new();
    groups.insert(1_i64, vat_20_group(1));
    // tax_group 99 NOT in map.

    let summaries = derive_check_tax_summaries(&items, &groups).expect("ok");
    assert_eq!(summaries.len(), 1, "only known group emits TX summary");
    assert_eq!(summaries[0].tx, 1);
}

// ─── Calc_tax error propagates ────────────────────────────────────

#[test]
fn calc_tax_error_propagates_as_typed_result() {
    let items = vec![item_with_group("ART-1", 1000, Some(1))];
    let mut groups = HashMap::new();
    groups.insert(
        1_i64,
        ResolvedTaxGroup {
            tx: 1,
            txpr: -100.0, // invalid → InvalidRate
            dtpr: 0.0,
            txal: 0,
            txty: 0,
        },
    );
    let err = derive_check_tax_summaries(&items, &groups).expect_err("calc_tax fails");
    use prro::xml::CalcTaxError;
    assert!(matches!(err, CalcTaxError::InvalidRate { .. }));
}

#[test]
fn calc_tax_unsupported_txal_propagates() {
    let items = vec![item_with_group("ART-1", 1000, Some(1))];
    let mut groups = HashMap::new();
    groups.insert(
        1_i64,
        ResolvedTaxGroup { tx: 1, txpr: 20.0, dtpr: 0.0, txal: 3, txty: 0 },
    );
    let err = derive_check_tax_summaries(&items, &groups).expect_err("TXAL=3 not supported");
    use prro::xml::CalcTaxError;
    assert!(matches!(err, CalcTaxError::UnsupportedAlgorithm(3)));
}

// ─── Ordering preserved (lex-by-stringified TX per Python parity) ─

#[test]
fn summaries_emit_in_input_order_emitter_handles_sort() {
    // derive_tax_summaries returns in deterministic order (by tx
    // value ascending, since aggregation uses BTreeMap or sorted
    // iteration).  Final wire sort is emitter's responsibility
    // (lex-by-stringified per audit1 fix).
    let items = vec![
        item_with_group("ART-2", 2000, Some(2)),
        item_with_group("ART-10", 10000, Some(10)),
        item_with_group("ART-1", 1000, Some(1)),
    ];
    let mut groups = HashMap::new();
    groups.insert(1_i64, vat_20_group(1));
    groups.insert(2_i64, vat_20_group(2));
    groups.insert(10_i64, vat_20_group(10));

    let summaries = derive_check_tax_summaries(&items, &groups).expect("ok");
    assert_eq!(summaries.len(), 3);
    // Helper returns by numeric tx; emitter will lex-sort.
    let tx_values: Vec<i64> = summaries.iter().map(|s| s.tx).collect();
    assert_eq!(tx_values, vec![1, 2, 10]);
}
