//! M3b W14a-2a — scanner test enforcing spec §4.4 forbidden patterns
//! + §11 acceptance #3 two-tier contract.
//!
//! - **Tier (a)**: `ShiftState::Error` is reachable ONLY via the
//!   `force_to_error_with_audit` seam.  No whitelist edge in §4.1 ever
//!   lands on `Error`.  This test verifies that `transition_state` returns
//!   `TransitionOutcome::Forbidden` for EVERY `(from, Error)` pair across
//!   the 9-state ShiftState set.
//!
//! - **Tier (b)**: `ShiftState::RequiresManualReconciliation` is reachable
//!   ONLY through whitelist edges 4 / 6 / 12 / 14 (per spec §4.1) OR via
//!   the `force_to_manual_reconciliation_with_audit` seam.  This test
//!   verifies that `transition_state` permits Manual landing ONLY from
//!   the 4 expected sources and forbids it from all 5 others.

use prro::db::models::enums::{FiscalMode, ShiftState};
use prro::db::models::ids::ShiftId;
use prro::db::open_pool;
use prro::db::repositories::fiscal_number_config::{self as fn_repo, NewFnConfig};
use prro::db::repositories::shifts::{self, TransitionOutcome};
use prro::db::tx::with_immediate;

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

/// Spec §11 acceptance #3 tier (b): exactly these 4 source states are
/// allowed to whitelist-transition into RequiresManualReconciliation.
const MANUAL_ALLOWED_SOURCES: [ShiftState; 4] = [
    ShiftState::Opening,                  // edge 4
    ShiftState::OpenedLocalPendingDrain,  // edge 6
    ShiftState::Closing,                  // edge 12
    ShiftState::ClosingLocalPendingDrain, // edge 14
];

async fn fresh_with_fn() -> (sqlx::SqlitePool, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    std::mem::forget(dir);
    let pool = open_pool(&path).await.unwrap();
    fn_repo::insert(
        &pool,
        &NewFnConfig {
            fiscal_number: "9000081000".into(),
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
    (pool, "9000081000".to_string())
}

async fn seed_shift_in_state(pool: &sqlx::SqlitePool, fn_id: &str, state: ShiftState) -> ShiftId {
    let id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, cash_balance_kop, \
            opened_by_cashier_id) \
         VALUES (?, ?, ?, 'ONLINE', 0, 'test-cashier')",
    )
    .bind(id)
    .bind(fn_id)
    .bind(state)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// Tier (a) — Error is NEVER reachable via `transition_state`.  Spec
/// §4.4 forbidden patterns + §11 acceptance #3a.
#[tokio::test]
async fn tier_a_error_unreachable_via_whitelist_from_any_source() {
    let (pool, fn_id) = fresh_with_fn().await;
    for from in ALL_STATES {
        let shift_id = seed_shift_in_state(&pool, &fn_id, from).await;
        let outcome = with_immediate(&pool, move |tx| {
            Box::pin(async move {
                let o = shifts::transition_state(tx, shift_id, from, ShiftState::Error).await?;
                anyhow::Ok(o)
            })
        })
        .await
        .unwrap();
        assert!(
            matches!(outcome, TransitionOutcome::Forbidden { .. }),
            "tier (a) violation: ({from:?} → Error) MUST be Forbidden via transition_state; \
             got {outcome:?}.  Spec §4.4: Error is reachable ONLY via \
             force_to_error_with_audit seam."
        );
        // State must remain unchanged (no DB write attempted).
        let observed = shifts::get(&pool, shift_id).await.unwrap().unwrap().state;
        assert_eq!(observed, from, "tier (a): Forbidden must not mutate state");
    }
}

/// Tier (b) — RequiresManualReconciliation reachable via
/// `transition_state` ONLY from edges 4 / 6 / 12 / 14 sources.  Spec §11
/// acceptance #3b.
#[tokio::test]
async fn tier_b_manual_reachable_only_from_4_whitelist_sources() {
    let (pool, fn_id) = fresh_with_fn().await;
    for from in ALL_STATES {
        let shift_id = seed_shift_in_state(&pool, &fn_id, from).await;
        let outcome = with_immediate(&pool, move |tx| {
            Box::pin(async move {
                let o = shifts::transition_state(
                    tx,
                    shift_id,
                    from,
                    ShiftState::RequiresManualReconciliation,
                )
                .await?;
                anyhow::Ok(o)
            })
        })
        .await
        .unwrap();

        let observed = shifts::get(&pool, shift_id).await.unwrap().unwrap().state;
        if MANUAL_ALLOWED_SOURCES.contains(&from) {
            assert_eq!(
                outcome,
                TransitionOutcome::Applied,
                "tier (b): ({from:?} → Manual) is whitelisted edge; expected Applied"
            );
            assert_eq!(
                observed,
                ShiftState::RequiresManualReconciliation,
                "tier (b): Applied must mutate state to Manual"
            );
        } else {
            assert!(
                matches!(outcome, TransitionOutcome::Forbidden { .. }),
                "tier (b) violation: ({from:?} → Manual) is NOT whitelisted; \
                 got {outcome:?}.  Spec §11 acceptance #3b: Manual reachable \
                 via transition_state ONLY from edges 4/6/12/14 sources."
            );
            assert_eq!(observed, from, "tier (b): Forbidden must not mutate state");
        }
    }
}
