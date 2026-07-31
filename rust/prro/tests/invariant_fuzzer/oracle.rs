//! Oracle layer 1 — the differential (Task 4).
//!
//! Each op is run through BOTH the reference model (`RefModel::apply`) and the
//! interpreter (`run_op`); the oracle asserts they agree, per the op's
//! CLASSIFICATION:
//!   - `PredictableMutating` → `check_differential` asserts the real ledger
//!     effect matches the model's prediction;
//!   - `ExpectedNoMutation`  → the real op is a typed refusal / no-op (no fiscal
//!     issuance);
//!   - `FaultOrRecovery`     → deferred to Task 5 (the fault oracle owns it).
//!
//! Two load-bearing rules (plan Task 4 "Two constraints"):
//!   1. classification is DERIVED from the model's `ExpectedOutcome` (which
//!      already encodes precondition + outcome type), NOT hard-coded per `Op`
//!      variant — so an out-of-precondition SELL (model → `NoMutation`) is
//!      correctly `ExpectedNoMutation`, never `PredictableMutating`.
//!   2. the seed differential is STRUCTURAL, not byte-equal: the model's seed is
//!      a synthetic per-lnd value, so we assert the seed ADVANCED iff the model
//!      says it did (at the lane-correct point), and that the real doc chains to
//!      the prior REAL tip — never `model.seed_after == real.seed` byte-for-byte.
//!
//! `check_differential` is a PURE function returning `Result` (never panics) so
//! both a match and a mismatch are testable.  The DB is read by the test via
//! `FuzzCtx` accessors, not inside the comparison.

use std::collections::BTreeMap;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use prro::db::invariant_scan::Violation;
use prro::db::models::enums::{DocState, ShiftState};
use prro::services::reconciliation::online_convergence::TickSummary;

use crate::interp::{ObservedDoc, ObservedHeld, ObservedRelease, RealOutcome};
use crate::model::{ExpectedOutcome, HeldWitness, Mutation, ReleasedWitness};

/// The op classification the differential dispatches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpClass {
    /// A deterministic fiscal mutation — differential-matched against the model.
    PredictableMutating,
    /// A TRUE no-op — assert the ledger is ENTIRELY unchanged (no row, no lnd).
    ExpectedNoMutation,
    /// B2 — a refusal that mints a legal NON-ISSUED row (online-reject Rejected /
    /// offline-ack Aborted): assert ≤1 new non-issued row, no issuance (seed +
    /// codes unchanged).  Distinct from `ExpectedNoMutation` (which forbids ANY
    /// new row), so a non-issued row is ACCEPTED here but a leaked row is caught
    /// there.
    ExpectedNoIssuanceRow,
    /// A fault / recovery op — deferred to Task 5 (bounded postcond + re-sync).
    FaultOrRecovery,
    /// CS-3 operator-completion (1b) — a release/refuse op. `run_harness` owns the comparison
    /// (`check_release_witness` + the doc/seed/mode transition), NOT the per-doc differential.
    Release,
    /// bd `PRRO_GATE-hpc` — a T=112 replenish: mutates durable state (code pool + chain seed) while
    /// minting NO document and allocating NO `lnd`.  Its ledger assertions live in `run_harness`
    /// (narrow and asserted, not a blanket exemption from the no-mutation invariants).
    Replenish,
}

/// A differential mismatch — carries a human-readable reason (the fuzzer reports
/// it; the test asserts on `is_err()`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence(pub String);

/// Classify an op FROM the model's predicted outcome (constraint #1): the model
/// already encoded precondition + outcome type, so we never hard-code per `Op`
/// variant (an out-of-precondition SELL → `NoMutation` → `ExpectedNoMutation`).
pub fn classify(expected: &ExpectedOutcome) -> OpClass {
    match expected {
        ExpectedOutcome::Mutated(_) => OpClass::PredictableMutating,
        ExpectedOutcome::NoMutation => OpClass::ExpectedNoMutation,
        ExpectedOutcome::NoIssuanceRow => OpClass::ExpectedNoIssuanceRow,
        ExpectedOutcome::Fault => OpClass::FaultOrRecovery,
        ExpectedOutcome::Release(_) => OpClass::Release,
        ExpectedOutcome::Replenish { .. } => OpClass::Replenish,
    }
}

/// Assert the real outcome agrees with the model's prediction for one op.
///
/// `prior_tip_real` is the REAL MAC seed BEFORE this op (read by the test via
/// `ctx.read_seed()`), used for the structural seed check.
///
/// For `PredictableMutating` ops whose real outcome is `Recovered` (drain /
/// go-online), the per-doc ledger is not on the outcome — the ledger-delta is
/// checked separately by [`check_ledger_delta`] (the test reads the real ledger
/// via `ctx.read_ledger()`).  Here such ops pass (the ledger-delta is the test's
/// next assertion).
pub fn check_differential(
    real: &RealOutcome,
    expected: &ExpectedOutcome,
    prior_tip_real: Option<&[u8]>,
) -> Result<(), Divergence> {
    match classify(expected) {
        OpClass::FaultOrRecovery => Ok(()), // Task 5 owns fault/recovery
        OpClass::Release => Ok(()),         // 1b: run_harness owns the release comparison
        OpClass::PredictableMutating => {
            let m = match expected {
                ExpectedOutcome::Mutated(m) => m,
                _ => unreachable!("classify maps Mutated ⇒ PredictableMutating"),
            };
            match real {
                RealOutcome::Doc(doc) => check_doc_against_mutation(doc, m, prior_tip_real),
                // drain / go-online: ledger-delta is checked by check_ledger_delta.
                RealOutcome::Recovered { .. } => Ok(()),
                other => Err(Divergence(format!(
                    "PredictableMutating: expected a Doc/Recovered real outcome, got {other:?}"
                ))),
            }
        }
        // Both no-issuance classes share the permissive differential shape; the
        // ledger assertions (zero rows vs ≤1 non-issued row, no seed/code) are the
        // harness's, split by class.
        // bd hpc — the ONLY doc-less mutating class.  The shape check is here; the ledger
        // assertions (seed moved iff granted, next_lnd UNCHANGED, no doc row, codes grew iff granted)
        // are the harness's, so a wrong-direction outcome cannot pass silently.
        OpClass::Replenish => {
            let granted = matches!(expected, ExpectedOutcome::Replenish { granted: true });
            match (granted, real) {
                (true, RealOutcome::Replenished { .. }) => Ok(()),
                (false, RealOutcome::Refused(_)) => Ok(()),
                (g, other) => Err(Divergence(format!(
                    "Replenish: model granted={g} but real outcome was {other:?}"
                ))),
            }
        }
        OpClass::ExpectedNoMutation | OpClass::ExpectedNoIssuanceRow => match real {
            // A typed refusal is the expected shape (online-reject / offline-ack).
            RealOutcome::Refused(_) => Ok(()),
            // An idempotent recovery no-op (e.g. GoOnlineWithoutBacklog) — the
            // ledger-unchanged assertion is the test's (via ctx).
            RealOutcome::Recovered { .. } => Ok(()),
            // A replay-resolve (DuplicateIdemKey) returns the EXISTING doc; the
            // "no NEW issuance" assertion (no seed/code advance) is the test's.
            RealOutcome::Doc(_) => Ok(()),
            // L6 — an X-report read is inherently a no-op for the differential
            // (no doc, no lnd, no seed, no code).  The turnover-snapshot equality
            // is the harness's extra assertion (`check_x_report_turnover`).
            RealOutcome::XReport { .. } => Ok(()),
            RealOutcome::Crashed { .. } => Err(Divergence(
                "ExpectedNoMutation/NoIssuanceRow but the real op crashed".to_string(),
            )),
            // CS-3 operator-completion — a release is checked by the SEPARATE `check_release_witness`
            // pass (like `check_ledger_delta`), not the per-doc differential.  Inert here (the release
            // op is directed-only this increment; the generative call site + prediction land later).
            RealOutcome::Released(_) => Ok(()),
            // bd hpc — a replenish mutates the seed + code pool, so it can NEVER be the real shape of
            // a no-mutation class.  Fail loudly rather than silently tolerating a class mix-up.
            RealOutcome::Replenished { .. } => Err(Divergence(
                "ExpectedNoMutation/NoIssuanceRow but the real op was a T=112 replenish (which \
                 advances the seed and grows the code pool)"
                    .to_string(),
            )),
        },
    }
}

/// Compare a real `Doc` outcome against the model's single-doc `Mutation`:
/// lnd, doc-state, code-consumption, and the STRUCTURAL seed (constraint #2).
fn check_doc_against_mutation(
    doc: &ObservedDoc,
    m: &Mutation,
    prior_tip_real: Option<&[u8]>,
) -> Result<(), Divergence> {
    if doc.lnd != m.lnd {
        return Err(Divergence(format!(
            "lnd mismatch: real {} != model {}",
            doc.lnd, m.lnd
        )));
    }
    if doc.doc_state != m.doc_state {
        return Err(Divergence(format!(
            "doc_state mismatch: real {:?} != model {:?}",
            doc.doc_state, m.doc_state
        )));
    }
    if doc.code_consumed != m.code_consumed {
        return Err(Divergence(format!(
            "code_consumed mismatch: real {:?} != model {:?}",
            doc.code_consumed, m.code_consumed
        )));
    }
    if let Some(expected_shift) = m.shift_state_after {
        if doc.shift_state_after != expected_shift {
            return Err(Divergence(format!(
                "shift_state mismatch: real {:?} != model {:?}",
                doc.shift_state_after, expected_shift
            )));
        }
    }

    // Structural seed (constraint #2):
    // (a) the seed ADVANCED this op iff the model says it advanced.  The model
    // compares its own Option<[u8;32]> tips (synthetic); the real side compares
    // byte-slices — we NEVER compare model bytes to real bytes (constraint #2).
    let real_advanced = doc.seed_after.as_deref() != prior_tip_real;
    let model_advanced = m.seed_after != m.previous_hash;
    if real_advanced != model_advanced {
        return Err(Divergence(format!(
            "seed-advance mismatch: real_advanced={real_advanced} model_advanced={model_advanced}"
        )));
    }
    // (b) chain-continuity: the real doc's previous_hash IS the prior real tip.
    if doc.previous_hash.as_deref() != prior_tip_real {
        return Err(Divergence(format!(
            "chain-continuity: real previous_hash {:?} != prior real tip {:?}",
            doc.previous_hash, prior_tip_real
        )));
    }
    Ok(())
}

/// CS-3 Slice E — assert the REAL durable delivery-axis witness (read from `delivery_reservation` +
/// `node_state`) matches the model's INDEPENDENT [`HeldWitness`] prediction for a HELD online
/// outcome.  `observed == None` while the model predicted a witness is itself a divergence: the
/// contract says a held outcome MUST persist a reservation row.  Every axis compares as DB-TEXT, so a
/// persisted-classifier regression (e.g. `ProbeRequired` → `TransientRetry`) surfaces as a plain
/// mismatch — the fuzzer's independent oracle for the Slice E routing contract.
pub fn check_held_witness(
    observed: Option<&ObservedHeld>,
    expected: &HeldWitness,
) -> Result<(), Divergence> {
    let o = observed.ok_or_else(|| {
        Divergence(format!(
            "held-witness: model predicted a held reservation ({expected:?}) but the doc has NO \
             delivery_reservation row (a held outcome MUST reserve)"
        ))
    })?;
    let string_axes: [(&str, &str, &str); 7] = [
        (
            "submission_certainty",
            o.submission_certainty.as_str(),
            expected.submission_certainty,
        ),
        (
            "response_provenance",
            o.response_provenance.as_str(),
            expected.response_provenance,
        ),
        (
            "routing_class",
            o.routing_class.as_deref().unwrap_or("<NULL>"),
            expected.routing_class,
        ),
        ("node_effect", o.node_effect.as_str(), expected.node_effect),
        (
            "evidence_kind",
            o.evidence_kind.as_str(),
            expected.evidence_kind,
        ),
        ("apply_state", o.apply_state.as_str(), expected.apply_state),
        ("node_mode", o.node_mode.as_str(), expected.node_mode),
    ];
    for (axis, real, exp) in string_axes {
        if real != exp {
            return Err(Divergence(format!(
                "held-witness {axis} mismatch: real {real:?} != model {exp:?}"
            )));
        }
    }
    if o.evidence_code != expected.evidence_code {
        return Err(Divergence(format!(
            "held-witness evidence_code mismatch: real {:?} != model {:?}",
            o.evidence_code, expected.evidence_code
        )));
    }
    if o.fence_held != expected.fence_held {
        return Err(Divergence(format!(
            "held-witness fence_held mismatch: real {} != model {}",
            o.fence_held, expected.fence_held
        )));
    }
    Ok(())
}

/// CS-3 operator-completion — assert the REAL durable witness after a legal completion matches the
/// model's INDEPENDENT [`ReleasedWitness`].  The load-bearing anti-BRICK checks are the last two:
/// `apply_state == "APPLIED"`, `fence_held == false`, and — always — `node_mode != "STOP_MODE"`.  A
/// completion that leaves the reservation `PENDING_APPLY`, the fence set, or the node in `STOP_MODE`
/// is an eternal BRICK and REDs here.
pub fn check_release_witness(
    observed: &ObservedRelease,
    expected: &ReleasedWitness,
) -> Result<(), Divergence> {
    let axes: [(&str, &str, &str); 3] = [
        (
            "apply_state",
            observed.apply_state.as_str(),
            expected.apply_state,
        ),
        ("node_mode", observed.node_mode.as_str(), expected.node_mode),
        ("doc_state", observed.doc_state.as_str(), expected.doc_state),
    ];
    for (axis, real, exp) in axes {
        if real != exp {
            return Err(Divergence(format!(
                "release-witness {axis} mismatch: real {real:?} != model {exp:?}"
            )));
        }
    }
    if observed.fence_held != expected.fence_held {
        return Err(Divergence(format!(
            "release-witness fence_held mismatch: real {} != model {}",
            observed.fence_held, expected.fence_held
        )));
    }
    // The unconditional anti-BRICK invariant — independent of the model tuple: a released
    // reservation can NEVER rest bricked (node halted STOP_MODE) nor with the fence still held.
    if observed.node_mode == "STOP_MODE" || observed.fence_held {
        return Err(Divergence(format!(
            "release-witness BRICK: released reservation still halted/fenced (mode={:?}, fence_held={})",
            observed.node_mode, observed.fence_held
        )));
    }
    Ok(())
}

/// Teeth helper for shift-aware model predictions.  It is intentionally tiny:
/// the real fuzzer path checks this through [`check_doc_against_mutation`], and
/// directed canaries can call it with synthetic states to prove the oracle goes
/// RED on shift drift without constructing a whole DB.
pub fn check_shift_state(real: ShiftState, expected: Option<ShiftState>) -> Result<(), Divergence> {
    match expected {
        Some(expected) if real != expected => Err(Divergence(format!(
            "shift_state mismatch: real {real:?} != model {expected:?}"
        ))),
        _ => Ok(()),
    }
}

/// Ledger-delta for `Recovered` (drain / go-online) ops: the real ledger
/// (lnd → state, read by the test via `ctx.read_ledger()`) must equal the
/// model's predicted ledger (`model.docs`).
pub fn check_ledger_delta(
    model_docs: &BTreeMap<i64, DocState>,
    real_ledger: &BTreeMap<i64, DocState>,
) -> Result<(), Divergence> {
    if model_docs != real_ledger {
        return Err(Divergence(format!(
            "ledger mismatch: model {model_docs:?} != real {real_ledger:?}"
        )));
    }
    Ok(())
}

/// L0/INV-21 cash-on-hand oracle.
///
/// Independent re-derive of the cash-on-hand for `fiscal_number`'s open shift
/// from the ledger, then compare to the model's `cash_on_hand` accumulator.
///
/// **Differential contract:**
/// - Model `cash_on_hand` must equal the real derived value after every op.
/// - If they diverge, a prod-side cash accounting bug or an L1-guard drift is
///   detected (e.g. the L1 guard being reverted would let a cash RETURN issue,
///   making real `cash_on_hand` go negative while the model stays at 0).
///
/// **Teeth (durable):** reverting the L1 guard in `convert.rs` makes this
/// oracle RED: the model refuses the cash RETURN (NoMutation, cash stays at
/// prior value) while prod accepts it (cash decrements) → divergence caught.
///
/// Called AFTER every op in the proptest harness (same cadence as
/// `check_ledger_delta`).
pub async fn check_cash_on_hand(
    pool: &SqlitePool,
    fiscal_number: &str,
    model_cash: i64,
) -> Result<(), Divergence> {
    // Re-derive cash using the prod cash_ledger (INDEPENDENT model is the
    // RefModel accumulator above; this reads the actual DB to cross-check).
    let real_cash = prro::services::cash_ledger::cash_on_hand_for_fn(pool, fiscal_number)
        .await
        .map_err(|e| Divergence(format!("cash oracle: cash_on_hand_for_fn failed: {e}")))?;
    if real_cash != model_cash {
        return Err(Divergence(format!(
            "cash oracle: real cash_on_hand {real_cash} != model {model_cash} \
             (INV-21 differential breach — cash ledger diverged)"
        )));
    }
    Ok(())
}

/// L6 — X-report turnover-snapshot oracle.
///
/// The X-report is a SIDE-EFFECT-FREE read, so the differential (no doc / lnd /
/// seed / code) already passes.  This adds the "turnover snapshot == model
/// totals" check: the `cash_on_hand_kop` the real X-report returned must equal
/// the model's independently-tracked `cash_on_hand`.
///
/// **Independence:** `model_cash` is the RefModel's own accumulator (built from
/// first principles per op — SELL `+`, RETURN `−`, ServiceIn `+`, ServiceOut
/// `−`, EPZ `−`); `real_cash` is what the prod `handle_x_report` derived from the
/// durable ledger via `cash_on_hand_for_fn`.  A divergence signals a prod X-report
/// aggregation bug (or a side-effect that changed the ledger under it).
///
/// **Teeth:** reverting the side-effect-free property (making X consume an lnd /
/// write a row) is caught by the harness's six-point no-mutation set FIRST; this
/// oracle additionally catches a turnover that silently under/over-reports.
pub fn check_x_report_turnover(real_cash: i64, model_cash: i64) -> Result<(), Divergence> {
    if real_cash != model_cash {
        return Err(Divergence(format!(
            "x-report oracle: real snapshot cash_on_hand {real_cash} != model {model_cash} \
             (turnover snapshot diverged from the model totals)"
        )));
    }
    Ok(())
}

/// Z aggregation oracle — independent from `runtime::ingress::convert`.
///
/// Reads the latest Z/close-shift document and the issued SELL/RETURN receipts
/// in that same shift, then recomputes the signer-ready Z payload shape from
/// persisted receipt payloads + persisted signing snapshots.  This catches the
/// fiscally expensive class where the state machine is correct but Z turnover /
/// tax totals are silently wrong.
pub async fn check_latest_z_aggregation(pool: &SqlitePool) -> Result<(), Divergence> {
    let z_payload: Option<String> = sqlx::query_scalar(
        "SELECT payload_json \
         FROM fiscal_documents \
         WHERE doc_type IN ('Z_REPORT','SHIFT_CLOSE') \
         ORDER BY lnd DESC \
         LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Divergence(format!("Z oracle: latest Z query failed: {e}")))?;
    let z_payload = z_payload.ok_or_else(|| {
        Divergence("Z oracle: no Z_REPORT/SHIFT_CLOSE document found".to_string())
    })?;

    let receipt_rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "WITH latest_z AS ( \
             SELECT shift_id \
             FROM fiscal_documents \
             WHERE doc_type IN ('Z_REPORT','SHIFT_CLOSE') \
             ORDER BY lnd DESC \
             LIMIT 1 \
         ) \
         SELECT d.doc_type, d.payload_json, s.payload_json \
         FROM fiscal_documents d \
         JOIN latest_z z ON d.shift_id = z.shift_id \
         LEFT JOIN signing_config_snapshots s ON s.id = d.signing_config_snapshot_id \
         WHERE d.doc_type IN ('SELL','RETURN') \
           AND d.state IN ('ACK','OFFLINE_LOCAL_ACK') \
         ORDER BY d.lnd",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Divergence(format!("Z oracle: receipt query failed: {e}")))?;

    check_z_payload_against_receipts(&z_payload, &receipt_rows)
}

#[derive(Debug, Clone)]
struct ReceiptForZ {
    doc_type: String,
    payload_json: String,
    snapshot_json: Option<String>,
}

impl From<&(String, String, Option<String>)> for ReceiptForZ {
    fn from(row: &(String, String, Option<String>)) -> Self {
        Self {
            doc_type: row.0.clone(),
            payload_json: row.1.clone(),
            snapshot_json: row.2.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct ZPayload {
    payments: Vec<ZPayment>,
    #[serde(default)]
    tax_summaries: Vec<ZTaxSummary>,
    sell_count: u32,
    return_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
struct ZPayment {
    name: String,
    sum_in_kop: i64,
    sum_out_kop: i64,
    type_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
struct ZTaxSummary {
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

#[derive(Debug, Deserialize)]
struct StoredCheckForZ {
    items: Vec<StoredItemForZ>,
    payments: Vec<StoredPaymentForZ>,
}

#[derive(Debug, Deserialize)]
struct StoredItemForZ {
    #[serde(default)]
    tax_group_1: Option<i64>,
    sum_kop: i64,
}

#[derive(Debug, Deserialize)]
struct StoredPaymentForZ {
    name: String,
    sum_kop: i64,
    type_code: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SnapshotForZ {
    #[serde(default)]
    driver_mapping: Vec<DriverMappingForZ>,
    groups: Vec<TaxGroupForZ>,
    kind: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DriverMappingForZ {
    driver_number: i64,
    canonical_tx_num: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct TaxGroupForZ {
    tx: i64,
    txpr_bps: i64,
    dtpr_bps: i64,
    txal: i64,
    txty: i64,
}

struct TaxAccumForZ {
    smi: i64,
    smo: i64,
    rate: Option<TaxGroupForZ>,
    established: bool,
}

fn check_z_payload_against_receipts(
    z_payload: &str,
    rows: &[(String, String, Option<String>)],
) -> Result<(), Divergence> {
    let receipts: Vec<ReceiptForZ> = rows.iter().map(ReceiptForZ::from).collect();
    let expected = expected_z_payload(&receipts)?;
    let real: ZPayload = serde_json::from_str(z_payload)
        .map_err(|e| Divergence(format!("Z oracle: Z payload JSON parse failed: {e}")))?;
    if real != expected {
        return Err(Divergence(format!(
            "Z oracle: aggregated payload mismatch: real {real:?} != expected {expected:?}"
        )));
    }
    Ok(())
}

fn expected_z_payload(receipts: &[ReceiptForZ]) -> Result<ZPayload, Divergence> {
    let mut payments: BTreeMap<(String, String), (i64, i64)> = BTreeMap::new();
    let mut tax: BTreeMap<i64, TaxAccumForZ> = BTreeMap::new();
    let mut sell_count = 0_u32;
    let mut return_count = 0_u32;

    for receipt in receipts {
        let is_sell = match receipt.doc_type.as_str() {
            "SELL" => {
                sell_count += 1;
                true
            }
            "RETURN" => {
                return_count += 1;
                false
            }
            other => {
                return Err(Divergence(format!(
                    "Z oracle: unexpected shift receipt doc_type {other:?}"
                )))
            }
        };
        let parsed: StoredCheckForZ = serde_json::from_str(&receipt.payload_json).map_err(|e| {
            Divergence(format!(
                "Z oracle: stored receipt payload parse failed for {}: {e}",
                receipt.doc_type
            ))
        })?;
        for payment in parsed.payments {
            if payment.sum_kop < 0 {
                return Err(Divergence(format!(
                    "Z oracle: negative stored payment {} for type_code={} name={:?}",
                    payment.sum_kop, payment.type_code, payment.name
                )));
            }
            let key = (payment.type_code, payment.name);
            let entry = payments.entry(key.clone()).or_insert((0, 0));
            let target = if is_sell { &mut entry.0 } else { &mut entry.1 };
            *target = target.checked_add(payment.sum_kop).ok_or_else(|| {
                Divergence(format!(
                    "Z oracle: payment sum overflow for type_code={} name={:?}",
                    key.0, key.1
                ))
            })?;
        }

        let snapshot = match &receipt.snapshot_json {
            Some(raw) => {
                let snapshot: SnapshotForZ = serde_json::from_str(raw).map_err(|e| {
                    Divergence(format!("Z oracle: signing snapshot parse failed: {e}"))
                })?;
                if snapshot.kind != "check_tax_mapping_v1" {
                    return Err(Divergence(format!(
                        "Z oracle: unsupported signing snapshot kind {:?}",
                        snapshot.kind
                    )));
                }
                Some(snapshot)
            }
            None => None,
        };

        for item in parsed.items {
            let Some(driver_tx) = item.tax_group_1 else {
                continue;
            };
            let canonical_tx = snapshot
                .as_ref()
                .and_then(|s| resolve_driver_tx_for_z(s, driver_tx))
                .unwrap_or(driver_tx);
            let rate = snapshot
                .as_ref()
                .and_then(|s| s.groups.iter().find(|g| g.tx == canonical_tx).cloned());
            let accum = tax.entry(canonical_tx).or_insert(TaxAccumForZ {
                smi: 0,
                smo: 0,
                rate: None,
                established: false,
            });
            if !accum.established {
                accum.rate = rate;
                accum.established = true;
            } else if accum.rate != rate {
                return Err(Divergence(format!(
                    "Z oracle: tax snapshot drift for tx={canonical_tx}"
                )));
            }
            let target = if is_sell {
                &mut accum.smi
            } else {
                &mut accum.smo
            };
            *target = target.checked_add(item.sum_kop).ok_or_else(|| {
                Divergence(format!("Z oracle: tax sum overflow for tx={canonical_tx}"))
            })?;
        }
    }

    let payments = payments
        .into_iter()
        .map(|((type_code, name), (sum_in_kop, sum_out_kop))| ZPayment {
            name,
            sum_in_kop,
            sum_out_kop,
            type_code,
        })
        .collect();
    let tax_summaries = tax
        .into_iter()
        .map(|(tx, accum)| z_tax_summary(tx, accum))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ZPayload {
        payments,
        tax_summaries,
        sell_count,
        return_count,
    })
}

fn resolve_driver_tx_for_z(snapshot: &SnapshotForZ, driver_tx: i64) -> Option<i64> {
    snapshot
        .driver_mapping
        .iter()
        .find(|m| m.driver_number == driver_tx)
        .map(|m| m.canonical_tx_num)
}

fn z_tax_summary(tx: i64, accum: TaxAccumForZ) -> Result<ZTaxSummary, Divergence> {
    match accum.rate {
        Some(rate) => {
            let txi = if accum.smi == 0 {
                0
            } else {
                calc_tax_included_for_z(accum.smi, &rate)?.0
            };
            let txo = if accum.smo == 0 {
                0
            } else {
                calc_tax_included_for_z(accum.smo, &rate)?.0
            };
            Ok(ZTaxSummary {
                tx,
                tx_short_form: false,
                txpr: format_bps(rate.txpr_bps),
                txal: rate.txal,
                txty: rate.txty,
                dtpr: format_bps(rate.dtpr_bps),
                smi: accum.smi,
                smo: accum.smo,
                txi,
                txo,
            })
        }
        None => Ok(ZTaxSummary {
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

fn calc_tax_included_for_z(sum_kop: i64, rate: &TaxGroupForZ) -> Result<(i64, i64), Divergence> {
    let txpr = rate.txpr_bps as f64 / 100.0;
    let dtpr = rate.dtpr_bps as f64 / 100.0;
    if txpr < 0.0 || dtpr < 0.0 {
        return Err(Divergence(format!(
            "Z oracle: negative tax rate txpr_bps={} dtpr_bps={}",
            rate.txpr_bps, rate.dtpr_bps
        )));
    }
    let g = sum_kop as f64;
    match rate.txal {
        0 => {
            let tx = if txpr > 0.0 {
                bankers_round_for_z(g * txpr / (100.0 + txpr))
            } else {
                0
            };
            Ok((tx, 0))
        }
        1 => {
            let dt = if dtpr > 0.0 {
                bankers_round_for_z(g * dtpr / 100.0)
            } else {
                0
            };
            let tx = if txpr > 0.0 {
                bankers_round_for_z((g + dt as f64) * txpr / (100.0 + txpr))
            } else {
                0
            };
            Ok((tx, dt))
        }
        2 => {
            let dt = if dtpr > 0.0 {
                bankers_round_for_z(g * dtpr / (100.0 + dtpr))
            } else {
                0
            };
            let tx = if txpr > 0.0 {
                bankers_round_for_z((g - dt as f64) * txpr / (100.0 + txpr))
            } else {
                0
            };
            Ok((tx, dt))
        }
        other => Err(Divergence(format!(
            "Z oracle: unsupported TXAL {other} for tx={}",
            rate.tx
        ))),
    }
}

fn bankers_round_for_z(x: f64) -> i64 {
    let floor = x.floor();
    let diff = x - floor;
    if diff < 0.5 {
        floor as i64
    } else if diff > 0.5 {
        (floor + 1.0) as i64
    } else {
        let f = floor as i64;
        if f % 2 == 0 {
            f
        } else {
            f + 1
        }
    }
}

fn format_bps(bps: i64) -> String {
    format!("{}.{:02}", bps / 100, (bps % 100).abs())
}

// ── Layer 2 — quiescent-boundary scan ───────────────────────────────────────

/// Run the REAL ledger invariant scanner at a QUIESCENT boundary — after a
/// completed op or after `Reboot` / recovery, NEVER mid-crash (a committed
/// `SENDING` wire-in-flight is a legal transient that the scanner would
/// false-positive on as `StuckSending`, spec §7.2).  Wraps
/// `prro::db::invariant_scan::assert_clean` (panics on a real violation) so the
/// fuzzer does NOT re-implement the invariants (chain / Mirror-1 / Mirror-3).
pub async fn assert_clean(pool: &SqlitePool) {
    prro::db::invariant_scan::assert_clean(pool).await;
}

/// O5 — the `ArtifactNoResend` terminal scan filter.
///
/// A forced-mode no-session `GoingOnline` artifact legitimately carries a
/// DEFERRED online-origin `SENDING` doc (boot reconciliation defers it to the W9
/// drain) — which is WHY the harness previously SKIPPED the scan there: a full
/// `assert_clean` would false-flag that `SENDING` as `StuckSending`.  Skipping
/// the WHOLE scan was a false-negative (a chain break / leaked pre-send doc /
/// duplicate lnd in that terminal slipped past).  This closes it: run `scan()`
/// but EXCUSE only the `StuckSending` variant; EVERY other violation stays
/// fatal.  The filter is VARIANT-SPECIFIC by design — `ChainBreak` /
/// `ChainSeedMismatch` / `DuplicateLnd` / session-desync carry no `document_id`,
/// so a bare id-filter would wrongly suppress them; matching the variant does not.
pub fn filter_artifact_violations(violations: Vec<Violation>) -> Result<(), Divergence> {
    let fatal: Vec<Violation> = violations
        .into_iter()
        .filter(|v| !matches!(v, Violation::StuckSending { .. }))
        .collect();
    if fatal.is_empty() {
        Ok(())
    } else {
        Err(Divergence(format!(
            "O5: ArtifactNoResend terminal has non-StuckSending violation(s) (only a \
             deferred online-origin SENDING transient is excused here): {fatal:?}"
        )))
    }
}

/// O3 — DB-integrity: every signed doc's stored MAC hash must equal the sha256
/// of its OWN persisted `PAYLOAD_XML`.  `stage_sign` computes
/// `unsigned_xml_sha256 = sha256(unsigned_xml)` and persists the SAME bytes as
/// `document_files(PAYLOAD_XML)` (`stage_sign.rs:431-441/709`), so for a clean
/// doc they match exactly.  The chain oracle is only REFERENTIAL (it trusts the
/// stored hash and checks chain-continuity); this catches a stored hash that
/// does NOT match its own stored payload — corruption / a mis-wired persist the
/// referential oracle is blind to.
///
/// SCOPE: this is the achievable DB-integrity SUBSET.  CANONICAL-TRUTH (recompute
/// the canonical XML from the doc and compare) needs a callable seam
/// canonicaliser (`stage_sign`'s builder is private) → deferred to WebCheck.
pub async fn check_payload_hash_integrity(pool: &SqlitePool) -> Result<(), Divergence> {
    let rows: Vec<(String, Vec<u8>, Option<Vec<u8>>)> = sqlx::query_as(
        "SELECT lower(hex(df.document_id)), df.content, fd.unsigned_xml_sha256 \
         FROM document_files df \
         JOIN fiscal_documents fd ON fd.document_id = df.document_id \
         WHERE df.kind = 'PAYLOAD_XML'",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Divergence(format!("O3 payload-hash query failed: {e}")))?;
    for (doc_hex, payload, stored_hash) in rows {
        let Some(stored) = stored_hash else {
            return Err(Divergence(format!(
                "O3: doc {doc_hex} has a persisted PAYLOAD_XML but a NULL unsigned_xml_sha256"
            )));
        };
        let computed = Sha256::digest(&payload);
        if computed.as_slice() != stored.as_slice() {
            return Err(Divergence(format!(
                "O3: doc {doc_hex} stored unsigned_xml_sha256 {stored:02x?} != \
                 sha256(PAYLOAD_XML) {:02x?}",
                computed.as_slice()
            )));
        }
    }
    Ok(())
}

/// O1 — the online-convergence postcondition (PURE; unit-tested on its decision
/// table).  After an Ack/Match-loaded `settle_convergence_tick`, a doc that the
/// tick neither held, superseded, errored, nor escalated MUST have converged
/// (left no doc resting at `SENT`/`KVT1`).  Online docs converge only on boot /
/// this tick, and the referential scan never flags `SENT`/`KVT1` — so a
/// Match-able doc left stuck after a deterministic tick is the false-negative
/// this closes.  A LEGITIMATE non-convergence (KVT1 hold / SENT transport-hold /
/// supersession / per-doc error / RMR escalation) is EXCUSED — the tick reports
/// it, so this never false-positives on a doc that legitimately stays put (CP2).
pub fn assert_online_convergence(
    summary: &TickSummary,
    resting_after: usize,
) -> Result<(), Divergence> {
    let legit_nonconverge = summary.sent_not_converged
        + summary.held_kvt1
        + summary.superseded_held_kvt1
        + summary.errors
        + summary.chain_seed_mismatch_escalated;
    if summary.scanned > 0 && legit_nonconverge == 0 && resting_after > 0 {
        return Err(Divergence(format!(
            "O1: an Ack/Match-loaded convergence tick scanned {} doc(s) with no hold / \
             supersession / error / escalation, yet left {resting_after} resting at \
             SENT/KVT1 — a Match-able doc failed to converge (the blind spot the \
             referential scan never caught)",
            summary.scanned
        )));
    }
    Ok(())
}

// ── Layer 3 — bounded kill-point postconditions (spec §9) ───────────────────

/// `Crash(Send)` + `Reboot` (kill-matrix K3): recovery routes the committed
/// `SENDING` doc to `ERROR_RETRYABLE` with NO second `send_chk` — DPS does not
/// deduplicate, so a blind resend would double-fiscalise.
pub fn assert_crash_send_recovery(
    recovered_state: DocState,
    send_calls_before_reboot: usize,
    send_calls_after_reboot: usize,
) -> Result<(), Divergence> {
    if recovered_state != DocState::ErrorRetryable {
        return Err(Divergence(format!(
            "Crash(Send) recovery: expected ERROR_RETRYABLE, got {recovered_state:?}"
        )));
    }
    if send_calls_after_reboot != send_calls_before_reboot {
        return Err(Divergence(format!(
            "Crash(Send) recovery RESENT: send_chk {send_calls_before_reboot} -> {send_calls_after_reboot}"
        )));
    }
    Ok(())
}

/// `Crash(Kvt1)` + `Reboot` (kill-matrix K4): SENT-before-confirm recovery takes
/// the PROBE path (a `last_chk`) and does NOT resend — `send_chk` unchanged AND
/// `last_chk` advanced.
pub fn assert_probe_recovery_no_resend(
    send_calls_before: usize,
    send_calls_after: usize,
    last_calls_before: usize,
    last_calls_after: usize,
) -> Result<(), Divergence> {
    if send_calls_after != send_calls_before {
        return Err(Divergence(format!(
            "probe recovery RESENT: send_chk {send_calls_before} -> {send_calls_after}"
        )));
    }
    if last_calls_after <= last_calls_before {
        return Err(Divergence(format!(
            "probe recovery did not probe: last_chk {last_calls_before} -> {last_calls_after}"
        )));
    }
    Ok(())
}

/// A3 — the UNIVERSAL crash-recovery postcondition (no exact terminal): a reboot
/// that resolves a committed crash transient must NOT re-send — `send_chk`
/// unchanged across the reboot (DPS does not dedup; a blind resend would
/// double-fiscalise).  This is the property-harness bounded postcond: under
/// composition the exact terminal (ERROR_RETRYABLE / probe → KVT1 / ACK /
/// manual) varies, but no-resend is invariant.  The exact terminals stay pinned
/// in the directed K3/K4 tests (`assert_crash_send_recovery` /
/// `assert_probe_recovery_no_resend`).
pub fn assert_no_resend(
    send_calls_before: usize,
    send_calls_after: usize,
) -> Result<(), Divergence> {
    if send_calls_after != send_calls_before {
        return Err(Divergence(format!(
            "crash recovery RESENT: send_chk {send_calls_before} -> {send_calls_after}"
        )));
    }
    Ok(())
}

// ── The 5th class — mirror-drift checks (Task 6) ────────────────────────────

/// Assert the load-bearing mirrors at a quiescent boundary.
///
/// Mirror-1 (`shifts.state` ↔ `node_state.shift_state`, #177), Mirror-3
/// (`inbox` ↔ ledger, check-5), the chain, and check-6d (cohort doc with a NULL
/// session) are already in the REAL scanner — we SURFACE any of its violations
/// as a `Divergence` (we do NOT re-implement them).
///
/// Mirror-2 (`offline_session` ↔ `drain_cohort`) is the EXACT predicate the
/// scanner does NOT contain: every drain-cohort doc must point at the ACTIVE
/// (OPEN / DRAINING) session.  check-6d only catches a NULL session; this also
/// catches a non-null but MISMATCHED (stale / foreign) session.  The predicate
/// is over DOCS — an empty active session is LEGAL (it is NOT "every session
/// must have docs"), so a session with zero cohort docs never false-positives.
pub async fn check_mirrors(pool: &SqlitePool) -> Result<(), Divergence> {
    // Mirror-1 / Mirror-3 / chain / check-6d — surface the real scanner.
    let violations = prro::db::invariant_scan::scan(pool)
        .await
        .map_err(|e| Divergence(format!("invariant_scan query failed: {e}")))?;
    if !violations.is_empty() {
        return Err(Divergence(format!(
            "invariant_scan (Mirror-1 / Mirror-3 / chain / check-6d): {violations:?}"
        )));
    }

    // Mirror-2 — the active (OPEN / DRAINING) session for this FN (the cohort the
    // drain scopes to; mirrors `current_open_or_draining_session`).
    //
    // X2: fetch ALL active sessions with a deterministic `ORDER BY` and guard the
    // single-active-session invariant.  `ux_offline_active` (partial unique index
    // on OPENING/OPEN/DRAINING) guarantees ≤1 on a clean DB, so this never fires
    // in a normal run — it is a defense-in-depth sentinel surfacing a >1-active
    // breach (e.g. a schema-guard regression) the bare `LIMIT 1` would have
    // silently MASKED by picking an arbitrary row.
    let active_ids: Vec<String> = sqlx::query_scalar::<_, String>(
        "SELECT lower(hex(offline_session_id)) FROM offline_sessions \
         WHERE state IN ('OPEN', 'DRAINING') ORDER BY opened_at, offline_session_id",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Divergence(format!("active-session query failed: {e}")))?;
    if active_ids.len() > 1 {
        return Err(Divergence(format!(
            "X2: multiple active OPEN/DRAINING offline sessions (single-active-session \
             invariant breach): {active_ids:?}"
        )));
    }
    let active: Option<String> = active_ids.into_iter().next();

    // Every drain-cohort doc (offline-origin, in a non-terminal cohort state —
    // mirrors `list_drain_candidates_for_fn_ordered_by_lnd`).
    let cohort: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT lower(hex(document_id)), \
                CASE WHEN offline_session_id IS NULL THEN NULL \
                     ELSE lower(hex(offline_session_id)) END \
         FROM fiscal_documents \
         WHERE offline_fiscal_no IS NOT NULL \
           AND state IN ('OFFLINE_LOCAL_ACK', 'SENT', 'KVT1', 'ERROR_RETRYABLE', 'KVT2')",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Divergence(format!("cohort query failed: {e}")))?;

    for (doc_hex, doc_session) in cohort {
        match (doc_session, &active) {
            // NULL session — invisible to the session-scoped cohort.
            (None, _) => {
                return Err(Divergence(format!(
                    "Mirror-2: drain-cohort doc {doc_hex} has a NULL offline_session_id \
                     (invisible to the cohort)"
                )));
            }
            // Points at the active session — clean.
            (Some(d), Some(a)) if &d == a => {}
            // Non-null but MISMATCHED (the gap check-6d misses).
            (Some(d), Some(a)) => {
                return Err(Divergence(format!(
                    "Mirror-2: drain-cohort doc {doc_hex} session {d} != active session {a}"
                )));
            }
            // A cohort doc with no active OPEN/DRAINING session — orphaned cohort.
            (Some(d), None) => {
                return Err(Divergence(format!(
                    "Mirror-2: drain-cohort doc {doc_hex} references session {d} \
                     but no OPEN/DRAINING session is active"
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taxable_receipt_rows() -> Vec<(String, String, Option<String>)> {
        vec![(
            "SELL".to_string(),
            r#"{"items":[{"tax_group_1":1,"sum_kop":15000}],"payments":[{"name":"Cash","sum_kop":15000,"type_code":"0"}]}"#
                .to_string(),
            Some(
                r#"{"driver_mapping":[],"groups":[{"dtpr_bps":0,"tx":1,"txal":0,"txpr_bps":2000,"txty":0}],"kind":"check_tax_mapping_v1"}"#
                    .to_string(),
            ),
        )]
    }

    #[test]
    fn z_aggregation_oracle_accepts_independently_computed_taxable_payload() {
        let z = r#"{"payments":[{"name":"Cash","sum_in_kop":15000,"sum_out_kop":0,"type_code":"0"}],"tax_summaries":[{"tx":1,"tx_short_form":false,"txpr":"20.00","txal":0,"txty":0,"dtpr":"0.00","smi":15000,"smo":0,"txi":2500,"txo":0}],"sell_count":1,"return_count":0}"#;
        check_z_payload_against_receipts(z, &taxable_receipt_rows())
            .expect("matching taxable Z payload must pass");
    }

    #[test]
    fn z_aggregation_oracle_catches_payment_drift() {
        let z = r#"{"payments":[{"name":"Cash","sum_in_kop":14999,"sum_out_kop":0,"type_code":"0"}],"tax_summaries":[{"tx":1,"tx_short_form":false,"txpr":"20.00","txal":0,"txty":0,"dtpr":"0.00","smi":15000,"smo":0,"txi":2500,"txo":0}],"sell_count":1,"return_count":0}"#;
        assert!(
            check_z_payload_against_receipts(z, &taxable_receipt_rows()).is_err(),
            "wrong Z payment turnover must fail the oracle"
        );
    }

    #[test]
    fn z_aggregation_oracle_catches_tax_drift() {
        let z = r#"{"payments":[{"name":"Cash","sum_in_kop":15000,"sum_out_kop":0,"type_code":"0"}],"tax_summaries":[{"tx":1,"tx_short_form":false,"txpr":"20.00","txal":0,"txty":0,"dtpr":"0.00","smi":15000,"smo":0,"txi":2499,"txo":0}],"sell_count":1,"return_count":0}"#;
        assert!(
            check_z_payload_against_receipts(z, &taxable_receipt_rows()).is_err(),
            "wrong Z TXS amount must fail the oracle"
        );
    }
}
