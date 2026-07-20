//! M3b W14a-2a — exhaustive 9×9 = 81 (from, to) matrix scanner test
//! for [`shifts::transition_state`].
//!
//! Per spec `docs/superpowers/specs/2026-05-17-m3b-shift-state-expansion.md`
//! §4.1: drift-guard for the 15-edge whitelist.  Any addition / removal
//! of a whitelist edge MUST update both the production
//! `allowed_transition` function body AND `ALLOWED_EDGES` below.
//!
//! Test enumerates EVERY (from, to) pair across all 9 ShiftState
//! variants:
//! - 16 cells → expect `TransitionOutcome::Applied` (state actually changed).
//! - 65 cells → expect `TransitionOutcome::Forbidden` (state UNCHANGED;
//!   no DB write attempted before the typed return).
//!
//! Spec §11 acceptance #1: locked-count test (`ALLOWED_EDGES.len() == 16`).

use prro::db::models::enums::{FiscalMode, ShiftState};
use prro::db::models::ids::ShiftId;
use prro::db::open_pool;
use prro::db::repositories::fiscal_number_config::{self as fn_repo, NewFnConfig};
use prro::db::repositories::shifts::{self, TransitionOutcome};
use prro::db::tx::with_immediate;
use prro::db::types::DbShiftId;

const ALL_STATES: [ShiftState; 9] = [
    ShiftState::Created,
    ShiftState::Opening,
    ShiftState::OpenedLocalPendingDrain,
    ShiftState::Opened,
    ShiftState::ClosingLocalPendingDrain,
    ShiftState::Closing,
    ShiftState::Closed,
    ShiftState::RequiresManualReconciliation,
    ShiftState::Error,
];

/// Spec §4.1 — 15 allowed (from, to) edges.  Numbered per spec.
const ALLOWED_EDGES: [(ShiftState, ShiftState); 16] = [
    (ShiftState::Created, ShiftState::Opening), // 1
    (ShiftState::Created, ShiftState::OpenedLocalPendingDrain), // 2
    (ShiftState::Opening, ShiftState::Opened),  // 3
    (
        ShiftState::Opening,
        ShiftState::RequiresManualReconciliation,
    ), // 4
    (ShiftState::OpenedLocalPendingDrain, ShiftState::Opened), // 5
    (
        ShiftState::OpenedLocalPendingDrain,
        ShiftState::RequiresManualReconciliation,
    ), // 6
    (
        ShiftState::OpenedLocalPendingDrain,
        ShiftState::ClosingLocalPendingDrain,
    ), // 7
    (ShiftState::Opened, ShiftState::Closing),  // 8
    (ShiftState::Opened, ShiftState::ClosingLocalPendingDrain), // 9
    (ShiftState::Closing, ShiftState::Closed),  // 10
    (ShiftState::Closing, ShiftState::Opened),  // 11
    (
        ShiftState::Closing,
        ShiftState::RequiresManualReconciliation,
    ), // 12
    (ShiftState::ClosingLocalPendingDrain, ShiftState::Closed), // 13
    (
        ShiftState::ClosingLocalPendingDrain,
        ShiftState::RequiresManualReconciliation,
    ), // 14
    (ShiftState::Opened, ShiftState::RequiresManualReconciliation), // 15 (M2-N2a: strict-sequential drain-reject on a plain Opened shift)
    (ShiftState::Opening, ShiftState::Closed), // 16 (CS-3 gap 4a: operator-completion rollback of a not-accepted online SHIFT_OPEN)
];

/// Drift-guard: spec §11 acceptance #1 locks the edge count.  Any
/// addition / removal of a whitelist edge MUST update spec §4.1, this
/// constant, AND `allowed_transition` body simultaneously.  (Was 14;
/// M2-N2a added edge 15 `Opened → RequiresManualReconciliation`; CS-3 gap 4a
/// added edge 16 `Opening → Closed` for online-SHIFT_OPEN operator rollback.)
#[test]
fn locked_edge_count_is_16() {
    assert_eq!(
        ALLOWED_EDGES.len(),
        16,
        "spec §11 acceptance #1: whitelist edge count is drift-guarded (CS-3 gap 4a → 16)"
    );
}

async fn fresh_with_fn() -> (sqlx::SqlitePool, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    std::mem::forget(dir);
    let pool = open_pool(&path).await.unwrap();
    fn_repo::insert(
        &pool,
        &NewFnConfig {
            fiscal_number: "9000080000".into(),
            tax_number: "12345678".into(),
            vat_payer_inn: None,
            fiscal_mode: FiscalMode::Test,
            org_name: None,
            point_name: None,
            org_address: None,
            tsp_enabled: false,
            offline_enabled: true,
            national_check_enabled: false,
            min_offline_codes: 0,
            max_offline_codes: 0,
        },
    )
    .await
    .unwrap();
    (pool, "9000080000".to_string())
}

/// Seed a shift row directly in the desired `state` (bypassing
/// `transition_state` for test setup so we can exercise transitions
/// from any of the 9 variants).
async fn seed_shift_in_state(pool: &sqlx::SqlitePool, fn_id: &str, state: ShiftState) -> ShiftId {
    let id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, cash_balance_kop, \
            opened_by_cashier_id) \
         VALUES (?, ?, ?, 'ONLINE', 0, 'test-cashier')",
    )
    .bind(DbShiftId(id))
    .bind(fn_id)
    .bind(state.as_str())
    .execute(pool)
    .await
    .unwrap();
    id
}

#[tokio::test]
async fn whitelist_matrix_16_allowed_65_forbidden_via_transition_state() {
    let (pool, fn_id) = fresh_with_fn().await;

    let mut applied_count = 0usize;
    let mut forbidden_count = 0usize;

    for from in ALL_STATES {
        for to in ALL_STATES {
            let shift_id = seed_shift_in_state(&pool, &fn_id, from).await;

            let outcome = with_immediate(&pool, move |tx| {
                Box::pin(async move {
                    let o = shifts::transition_state(tx, shift_id, from, to).await?;
                    anyhow::Ok(o)
                })
            })
            .await
            .unwrap();

            let is_whitelisted = ALLOWED_EDGES.contains(&(from, to));
            let observed_state = shifts::get(&pool, shift_id).await.unwrap().unwrap().state;

            if is_whitelisted {
                assert_eq!(
                    outcome,
                    TransitionOutcome::Applied,
                    "({from:?} → {to:?}) is whitelisted; expected Applied, got {outcome:?}"
                );
                assert_eq!(
                    observed_state, to,
                    "({from:?} → {to:?}) Applied; state must equal `to`"
                );
                applied_count += 1;
            } else {
                assert!(
                    matches!(outcome, TransitionOutcome::Forbidden { .. }),
                    "({from:?} → {to:?}) is NOT whitelisted; expected Forbidden, got {outcome:?}"
                );
                assert_eq!(
                    observed_state, from,
                    "({from:?} → {to:?}) Forbidden; state must remain `from` (no DB write)"
                );
                forbidden_count += 1;
            }

            // RS-3 C2 (migration 023): the active-shift uniqueness index
            // forbids two ACTIVE shifts for one FN.  This matrix seeds 81
            // shifts for ONE FN to exhaustively probe `transition_state`, so
            // each seeded shift must be removed before the next is seeded —
            // this test isolates the transition whitelist, not per-FN
            // uniqueness (covered by `repo_shifts.rs`).
            sqlx::query("DELETE FROM shifts WHERE shift_id = ?")
                .bind(DbShiftId(shift_id))
                .execute(&pool)
                .await
                .unwrap();
        }
    }

    assert_eq!(
        applied_count, 16,
        "spec §4.1: 16 whitelist edges (CS-3 gap 4a added edge 16 Opening→Closed)"
    );
    assert_eq!(forbidden_count, 9 * 9 - 16, "9*9 - 16 = 65 forbidden pairs");
}

/// PR #66 R2 LOW-6: regression test for `TransitionOutcome::NotFound`
/// (renamed from `RowGone` in R2 MED-1).  Calls `transition_state` with
/// a never-seeded `ShiftId::new()` — the CAS-WHERE-state UPDATE matches
/// 0 rows AND the row doesn't exist → `NotFound` (was: opaque
/// `sqlx::Error::RowNotFound` pre-R1).
#[tokio::test]
async fn transition_state_returns_not_found_for_stale_shift_id() {
    use prro::db::tx::with_immediate;

    let (pool, _fn_id) = fresh_with_fn().await;
    let stale_id = ShiftId::new();

    let outcome = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let o =
                shifts::transition_state(tx, stale_id, ShiftState::Created, ShiftState::Opening)
                    .await?;
            anyhow::Ok(o)
        })
    })
    .await
    .unwrap();

    assert!(
        matches!(outcome, TransitionOutcome::NotFound),
        "stale ShiftId must surface as NotFound, got {outcome:?}"
    );
}

/// W14a-2b Commit 7 §1.6 (R7 carry-forward LOW): direct unit test
/// for `shifts::TransitionOutcome::Conflict` variant.  Mirrors the
/// `fiscal_documents` pattern at `repo_fiscal_documents_state_cas.rs:117`.
///
/// Scenario: caller passes a whitelist-allowed `from→to` pair, but the
/// observed `current_state` at CAS time has drifted (e.g. concurrent
/// admin path / force seam ran in parallel).  Whitelist check passes,
/// the CAS-WHERE-state UPDATE matches 0 rows, but the row still exists
/// → `Conflict { observed: <other_state> }`.
///
/// Locks the diagnostic re-read logic in `transition_state` against
/// future regression.
#[tokio::test]
async fn transition_state_returns_conflict_when_observed_state_drifted() {
    use prro::db::tx::with_immediate;

    let (pool, fn_id) = fresh_with_fn().await;
    let shift_id = seed_shift_in_state(&pool, &fn_id, ShiftState::Opened).await;

    let outcome = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            // Mutate state to Closing OUTSIDE the whitelist seam via
            // raw UPDATE (simulates a concurrent admin path that
            // bypassed the typed transition surface).
            sqlx::query("UPDATE shifts SET state = 'CLOSING' WHERE shift_id = ?")
                .bind(DbShiftId(shift_id))
                .execute(&mut **tx)
                .await?;
            // Whitelist-allowed transition (Opened → Closing edge 8)
            // BUT current state at UPDATE will be 'CLOSING', not
            // 'OPENED'.  CAS WHERE shift_id = ? AND state = 'OPENED'
            // → 0 rows.  Diagnostic re-read returns 'CLOSING' →
            // Conflict { observed: Closing }.
            let o = shifts::transition_state(tx, shift_id, ShiftState::Opened, ShiftState::Closing)
                .await?;
            anyhow::Ok(o)
        })
    })
    .await
    .unwrap();

    assert_eq!(
        outcome,
        TransitionOutcome::Conflict {
            observed: ShiftState::Closing
        },
        "post-drift transition_state MUST surface Conflict with observed=Closing"
    );
}
