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

use sqlx::SqlitePool;

use prro::db::models::enums::DocState;

use crate::interp::{ObservedDoc, RealOutcome};
use crate::model::{ExpectedOutcome, Mutation};

/// The op classification the differential dispatches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpClass {
    /// A deterministic fiscal mutation — differential-matched against the model.
    PredictableMutating,
    /// A typed refusal / idempotent no-op — assert NO fiscal issuance.
    ExpectedNoMutation,
    /// A fault / recovery op — deferred to Task 5 (bounded postcond + re-sync).
    FaultOrRecovery,
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
        ExpectedOutcome::Fault => OpClass::FaultOrRecovery,
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
        OpClass::ExpectedNoMutation => match real {
            // A typed refusal is the expected shape.
            RealOutcome::Refused(_) => Ok(()),
            // An idempotent recovery no-op (e.g. GoOnlineWithoutBacklog) — the
            // ledger-unchanged assertion is the test's (via ctx).
            RealOutcome::Recovered { .. } => Ok(()),
            // A replay-resolve (DuplicateIdemKey) returns the EXISTING doc; the
            // "no NEW issuance" assertion (no seed/code advance) is the test's.
            RealOutcome::Doc(_) => Ok(()),
            RealOutcome::Crashed { .. } => Err(Divergence(
                "ExpectedNoMutation but the real op crashed".to_string(),
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
    let active: Option<String> = sqlx::query_scalar::<_, String>(
        "SELECT lower(hex(offline_session_id)) FROM offline_sessions \
         WHERE state IN ('OPEN', 'DRAINING') LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Divergence(format!("active-session query failed: {e}")))?;

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
