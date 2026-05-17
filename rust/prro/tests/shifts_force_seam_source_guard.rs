//! M3b W14a-2a — 18-case force-seam source-state guard matrix.
//!
//! Per spec §4.5 (Round 6 A-H1 force-seam source-state restriction) +
//! §11 acceptance #4b.  Enumerates 9 ShiftState variants × 2 force
//! seams = 18 (current_state, seam) invocations.  Each verifies the
//! exact contract:
//!
//! - Allowed source → seam returns `Ok(())`, state transitions to
//!   target (Error or RequiresManualReconciliation), Critical audit
//!   emitted (`SHIFT_FORCE_TO_ERROR` / `SHIFT_FORCE_TO_MANUAL_RECONCILIATION`).
//! - Forbidden source → seam returns `ForceSeamError::ForbiddenSource`,
//!   state UNCHANGED, Warning audit emitted (`SHIFT_FORCE_SEAM_REFUSED`)
//!   with full evidence_json envelope for forensic traceability per §8.

use prro::db::models::enums::ShiftState;
use prro::db::models::enums::FiscalMode;
use prro::db::models::ids::ShiftId;
use prro::db::repositories::fiscal_number_config::{self as fn_repo, NewFnConfig};
use prro::db::repositories::shifts;
use prro::db::tx::with_immediate;
use prro::db::open_pool;

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

/// Spec §4.5 allowed sources for `force_to_error_with_audit`.
fn force_to_error_allows(state: ShiftState) -> bool {
    use ShiftState::*;
    matches!(
        state,
        Opening
            | OpenedLocalPendingDrain
            | Opened
            | Closing
            | ClosingLocalPendingDrain
            | RequiresManualReconciliation
    )
}

/// Spec §4.5 allowed sources for `force_to_manual_reconciliation_with_audit`.
fn force_to_manual_allows(state: ShiftState) -> bool {
    use ShiftState::*;
    matches!(
        state,
        Opening | OpenedLocalPendingDrain | Opened | Closing | ClosingLocalPendingDrain
    )
}

async fn fresh_with_fn() -> (sqlx::SqlitePool, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    std::mem::forget(dir);
    let pool = open_pool(&path).await.unwrap();
    fn_repo::insert(
        &pool,
        &NewFnConfig {
            fiscal_number: "9000082000".into(),
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
    (pool, "9000082000".to_string())
}

async fn seed_shift_in_state(
    pool: &sqlx::SqlitePool,
    fn_id: &str,
    state: ShiftState,
) -> ShiftId {
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

async fn count_audit_events(
    pool: &sqlx::SqlitePool,
    shift_id_hex: &str,
    event_type: &str,
) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE entity_type = 'shift' \
         AND entity_id = ? AND event_type = ?",
    )
    .bind(shift_id_hex)
    .bind(event_type)
    .fetch_one(pool)
    .await
    .unwrap()
}

const EVIDENCE: &str = r#"{"operator_id":"op-007","reason_code":"manual_recon_request","free_text":"test","timestamp_utc":"2026-05-17T12:00:00Z"}"#;

/// 9 cases: force_to_error_with_audit × each ShiftState.
/// 6 allowed → Applied + SHIFT_FORCE_TO_ERROR Critical audit.
/// 3 forbidden → ForbiddenSource + SHIFT_FORCE_SEAM_REFUSED Warning audit.
#[tokio::test]
async fn force_to_error_with_audit_source_guard_9_cases() {
    let (pool, fn_id) = fresh_with_fn().await;
    for from in ALL_STATES {
        let shift_id = seed_shift_in_state(&pool, &fn_id, from).await;
        let shift_id_hex: String =
            sqlx::query_scalar("SELECT lower(hex(shift_id)) FROM shifts WHERE shift_id = ?")
                .bind(shift_id)
                .fetch_one(&pool)
                .await
                .unwrap();

        let outcome = with_immediate(&pool, move |tx| {
            Box::pin(async move {
                let o = shifts::force_to_error_with_audit(tx, shift_id, Some("op-007"), EVIDENCE)
                    .await
                    .map_err(|e| anyhow::anyhow!("force-seam: {e}"))?;
                anyhow::Ok(o)
            })
        })
        .await
        .unwrap();

        let observed_state = shifts::get(&pool, shift_id).await.unwrap().unwrap().state;
        let critical_audits = count_audit_events(&pool, &shift_id_hex, "SHIFT_FORCE_TO_ERROR").await;
        let refused_audits =
            count_audit_events(&pool, &shift_id_hex, "SHIFT_FORCE_SEAM_REFUSED").await;

        use prro::db::repositories::shifts::ForceSeamOutcome;
        if force_to_error_allows(from) {
            assert!(
                matches!(outcome, ForceSeamOutcome::Applied),
                "({from:?}, force_to_error) allowed; expected Applied, got {outcome:?}"
            );
            assert_eq!(
                observed_state,
                ShiftState::Error,
                "({from:?}, force_to_error) Applied must transition to Error"
            );
            assert_eq!(critical_audits, 1, "({from:?}) must emit SHIFT_FORCE_TO_ERROR once");
            assert_eq!(refused_audits, 0, "({from:?}) Applied must NOT emit refused audit");
        } else {
            assert!(
                matches!(outcome, ForceSeamOutcome::ForbiddenSource { .. }),
                "({from:?}, force_to_error) forbidden; expected ForbiddenSource, got {outcome:?}"
            );
            assert_eq!(
                observed_state, from,
                "({from:?}, force_to_error) Forbidden must NOT mutate state"
            );
            assert_eq!(
                critical_audits, 0,
                "({from:?}) Forbidden must NOT emit SHIFT_FORCE_TO_ERROR"
            );
            assert_eq!(
                refused_audits, 1,
                "({from:?}) Forbidden must emit SHIFT_FORCE_SEAM_REFUSED once (forensic, \
                 committed via Ok-return contract per spec §8 + Round 7 §8.1)"
            );
        }
    }
}

/// 9 cases: force_to_manual_reconciliation_with_audit × each ShiftState.
/// 5 allowed → Applied + SHIFT_FORCE_TO_MANUAL_RECONCILIATION Critical audit.
/// 4 forbidden → ForbiddenSource + SHIFT_FORCE_SEAM_REFUSED Warning audit.
#[tokio::test]
async fn force_to_manual_reconciliation_with_audit_source_guard_9_cases() {
    let (pool, fn_id) = fresh_with_fn().await;
    for from in ALL_STATES {
        let shift_id = seed_shift_in_state(&pool, &fn_id, from).await;
        let shift_id_hex: String =
            sqlx::query_scalar("SELECT lower(hex(shift_id)) FROM shifts WHERE shift_id = ?")
                .bind(shift_id)
                .fetch_one(&pool)
                .await
                .unwrap();

        let outcome = with_immediate(&pool, move |tx| {
            Box::pin(async move {
                let o = shifts::force_to_manual_reconciliation_with_audit(tx, shift_id, Some("op-007"), EVIDENCE)
                    .await
                    .map_err(|e| anyhow::anyhow!("force-seam: {e}"))?;
                anyhow::Ok(o)
            })
        })
        .await
        .unwrap();

        let observed_state = shifts::get(&pool, shift_id).await.unwrap().unwrap().state;
        let critical_audits = count_audit_events(
            &pool,
            &shift_id_hex,
            "SHIFT_FORCE_TO_MANUAL_RECONCILIATION",
        )
        .await;
        let refused_audits =
            count_audit_events(&pool, &shift_id_hex, "SHIFT_FORCE_SEAM_REFUSED").await;

        use prro::db::repositories::shifts::ForceSeamOutcome;
        if force_to_manual_allows(from) {
            assert!(
                matches!(outcome, ForceSeamOutcome::Applied),
                "({from:?}, force_to_manual) allowed; expected Applied, got {outcome:?}"
            );
            assert_eq!(
                observed_state,
                ShiftState::RequiresManualReconciliation,
                "({from:?}, force_to_manual) Applied must transition to Manual"
            );
            assert_eq!(critical_audits, 1, "({from:?}) must emit FORCE_TO_MANUAL once");
            assert_eq!(refused_audits, 0, "({from:?}) Applied must NOT emit refused audit");
        } else {
            assert!(
                matches!(outcome, ForceSeamOutcome::ForbiddenSource { .. }),
                "({from:?}, force_to_manual) forbidden; expected ForbiddenSource, got {outcome:?}"
            );
            assert_eq!(
                observed_state, from,
                "({from:?}, force_to_manual) Forbidden must NOT mutate state"
            );
            assert_eq!(critical_audits, 0, "({from:?}) Forbidden must NOT emit FORCE_TO_MANUAL");
            assert_eq!(
                refused_audits, 1,
                "({from:?}) Forbidden must emit SHIFT_FORCE_SEAM_REFUSED once (forensic, \
                 committed via Ok-return contract per spec §8 + Round 7 §8.1)"
            );
        }
    }
}

/// Drift-guard: count of cases matches spec (9 × 2 = 18).
#[test]
fn locked_case_count_is_18() {
    assert_eq!(
        ALL_STATES.len() * 2,
        18,
        "spec §4.5 source-guard matrix is 9 states × 2 seams = 18"
    );
}
