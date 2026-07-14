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

use prro::db::models::enums::FiscalMode;
use prro::db::models::enums::ShiftState;
use prro::db::models::ids::ShiftId;
use prro::db::open_pool;
use prro::db::repositories::fiscal_number_config::{self as fn_repo, NewFnConfig};
use prro::db::repositories::shifts;
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

async fn count_audit_events(pool: &sqlx::SqlitePool, shift_id_hex: &str, event_type: &str) -> i64 {
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
        let critical_audits =
            count_audit_events(&pool, &shift_id_hex, "SHIFT_FORCE_TO_ERROR").await;
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
            assert_eq!(
                critical_audits, 1,
                "({from:?}) must emit SHIFT_FORCE_TO_ERROR once"
            );
            assert_eq!(
                refused_audits, 0,
                "({from:?}) Applied must NOT emit refused audit"
            );
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

        // RS-3 C2 (migration 023): one active shift per FN — drop the seed
        // before the next `from` state is seeded for the same FN.  This
        // matrix isolates the force-seam source guard, not per-FN uniqueness.
        sqlx::query("DELETE FROM shifts WHERE shift_id = ?")
            .bind(shift_id)
            .execute(&pool)
            .await
            .unwrap();
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
                let o = shifts::force_to_manual_reconciliation_with_audit(
                    tx,
                    shift_id,
                    Some("op-007"),
                    EVIDENCE,
                )
                .await
                .map_err(|e| anyhow::anyhow!("force-seam: {e}"))?;
                anyhow::Ok(o)
            })
        })
        .await
        .unwrap();

        let observed_state = shifts::get(&pool, shift_id).await.unwrap().unwrap().state;
        let critical_audits =
            count_audit_events(&pool, &shift_id_hex, "SHIFT_FORCE_TO_MANUAL_RECONCILIATION").await;
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
            assert_eq!(
                critical_audits, 1,
                "({from:?}) must emit FORCE_TO_MANUAL once"
            );
            assert_eq!(
                refused_audits, 0,
                "({from:?}) Applied must NOT emit refused audit"
            );
        } else {
            assert!(
                matches!(outcome, ForceSeamOutcome::ForbiddenSource { .. }),
                "({from:?}, force_to_manual) forbidden; expected ForbiddenSource, got {outcome:?}"
            );
            assert_eq!(
                observed_state, from,
                "({from:?}, force_to_manual) Forbidden must NOT mutate state"
            );
            assert_eq!(
                critical_audits, 0,
                "({from:?}) Forbidden must NOT emit FORCE_TO_MANUAL"
            );
            assert_eq!(
                refused_audits, 1,
                "({from:?}) Forbidden must emit SHIFT_FORCE_SEAM_REFUSED once (forensic, \
                 committed via Ok-return contract per spec §8 + Round 7 §8.1)"
            );
        }

        // RS-3 C2 (migration 023): one active shift per FN — drop the seed
        // before the next `from` state is seeded for the same FN.  This
        // matrix isolates the force-seam source guard, not per-FN uniqueness.
        sqlx::query("DELETE FROM shifts WHERE shift_id = ?")
            .bind(shift_id)
            .execute(&pool)
            .await
            .unwrap();
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

// ─── PR #66 R4 L-R4-1 (revised R5): TxIsolationViolation tests ──────
//
// We cannot synthetically trigger CAS-WHERE-state mismatch via integration
// tests: BEGIN IMMEDIATE serialises writers inside the `with_immediate`
// envelope (the only legal way to acquire `&mut WriteTxConn<'_>`), and
// production code holds the WriteTxConn for the entire seam call —
// there's no observable point where another writer can race in.
//
// Earlier R4 attempt at a synthetic-race test was deleted in R5 per
// reviewer M-R5-1 (the test body had collected "thinking aloud"
// comments + a tautological assert that didn't actually exercise the
// TxIsolationViolation path).  What ships instead:
//
// - **Type-system regression** below — locks variant shape + ensures
//   the enum variants remain constructible + matchable.  Catches
//   accidental rename / field removal in future refactors.
// - **Production correctness** of the graceful path itself is verified
//   structurally: the source code at `shifts.rs` shows all 3 sites
//   (force_to_error, force_to_manual, senior_close) wire the
//   `if rows_affected != 1` branch to `emit_cas_isolation_violation_audit`
//   helper which returns the outcome.  Helper unit-testing inside
//   shifts.rs is a future improvement; would require setting up a
//   pool + with_immediate inside `#[cfg(test)] mod` (boilerplate
//   duplicating integration test fixtures).

/// Type-system regression: confirms TxIsolationViolation variant exists,
/// derives Debug/Clone/Eq, and the documented fields are accessible.
/// PR #66 R4 L-R4-1 — pure structural test, no DB interaction.
#[test]
fn tx_isolation_violation_variant_is_constructible_and_matchable() {
    use prro::db::repositories::shifts::ForceSeamOutcome;
    let v = ForceSeamOutcome::TxIsolationViolation {
        observed_state_at_update: Some(ShiftState::Closing),
        attempted_seam: "force_to_error_with_audit",
        rows_affected: 0,
    };
    assert!(matches!(
        v,
        ForceSeamOutcome::TxIsolationViolation {
            observed_state_at_update: Some(ShiftState::Closing),
            attempted_seam: "force_to_error_with_audit",
            rows_affected: 0,
        }
    ));
    // Also exercise the SeniorCloseOutcome counterpart.
    use prro::db::repositories::shifts::SeniorCloseOutcome;
    let s = SeniorCloseOutcome::TxIsolationViolation {
        observed_state_at_update: None,
        rows_affected: 0,
    };
    assert!(matches!(
        s,
        SeniorCloseOutcome::TxIsolationViolation {
            observed_state_at_update: None,
            rows_affected: 0,
        }
    ));
}

/// FW-1 mutation A2 — `validate_evidence_json` body → `Ok(())` (shifts.rs:475).
/// Both force seams call `validate_evidence_json(evidence_json)?` as their FIRST
/// line (shifts.rs:540 / :671), before the source-state check and the shift
/// lookup. The existing 9-case guards always pass the VALID `EVIDENCE` const, so
/// the malformed / oversized paths are never exercised → the mutation (which
/// disables both the 8 KiB cap and the JSON-shape check) survives. This feeds
/// invalid + oversized evidence to both seams on an allowed source (Opened) and
/// pins: reject with `InvalidEvidenceJson`, no state change, no forensic audit.
#[tokio::test]
async fn force_seams_reject_malformed_and_oversized_evidence() {
    let (pool, fn_id) = fresh_with_fn().await;
    // One Opened shift, REUSED across all cases: a rejected force never mutates
    // state, so the shift stays a valid Opened source — and one FN cannot hold
    // two active shifts (UNIQUE(shifts.fiscal_number)), so re-seeding per case
    // would spuriously fail.
    let shift_id = seed_shift_in_state(&pool, &fn_id, ShiftState::Opened).await;
    let shift_id_hex: String =
        sqlx::query_scalar("SELECT lower(hex(shift_id)) FROM shifts WHERE shift_id = ?")
            .bind(shift_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    // valid JSON but over the 8 KiB cap → must still be rejected (size guard).
    let oversized = format!(
        "{{\"x\":\"{}\"}}",
        "a".repeat(shifts::FORCE_SEAM_EVIDENCE_MAX_BYTES)
    );
    for bad in ["not-json-at-all", oversized.as_str()] {
        for seam in ["error", "manual"] {
            let bad_s = bad.to_string();
            let res = with_immediate(&pool, move |tx| {
                Box::pin(async move {
                    let o = if seam == "error" {
                        shifts::force_to_error_with_audit(tx, shift_id, Some("op-007"), &bad_s).await
                    } else {
                        shifts::force_to_manual_reconciliation_with_audit(
                            tx,
                            shift_id,
                            Some("op-007"),
                            &bad_s,
                        )
                        .await
                    };
                    anyhow::Ok(o)
                })
            })
            .await
            .unwrap();
            // Rejected with the typed evidence error before any state change.
            // Mutation (validate → Ok(())) lets it proceed → Ok(Applied/ForbiddenSource).
            assert!(
                matches!(res, Err(shifts::ForceSeamError::InvalidEvidenceJson(_))),
                "seam={seam} bad-evidence must be InvalidEvidenceJson, got {res:?}"
            );
        }
    }
    // No case mutated the shift and no forensic audit fired — validation precedes
    // the source check and every DB write.
    let st = shifts::get(&pool, shift_id).await.unwrap().unwrap().state;
    assert_eq!(
        st,
        ShiftState::Opened,
        "state must not change on rejected evidence"
    );
    assert_eq!(
        count_audit_events(&pool, &shift_id_hex, "SHIFT_FORCE_TO_ERROR").await,
        0,
        "rejected evidence must not emit SHIFT_FORCE_TO_ERROR"
    );
    assert_eq!(
        count_audit_events(&pool, &shift_id_hex, "SHIFT_FORCE_TO_MANUAL_RECONCILIATION").await,
        0,
        "rejected evidence must not emit SHIFT_FORCE_TO_MANUAL_RECONCILIATION"
    );
}

/// FW-1 mutation A3 — delete `!` in `actor_id.filter(|s| !s.is_empty())`
/// (shifts.rs:674, force_to_manual). Flips "keep non-empty actor, drop empty"
/// into "keep only empty" ⇒ a real operator id like `Some("op-007")` normalizes
/// to `None` ⇒ every emitted audit records `actor = NULL`, destroying the
/// non-repudiable "who forced the shift into manual reconciliation" trail. The
/// mutation leaves outcome / state / audit COUNT identical (that is why the
/// 9-case test misses it — it never reads `audit_log.actor`). This pins the
/// stored actor column directly.
#[tokio::test]
async fn force_to_manual_records_operator_actor_in_audit() {
    let (pool, fn_id) = fresh_with_fn().await;
    // Opened is an allowed source for force_to_manual → the success audit fires.
    let shift_id = seed_shift_in_state(&pool, &fn_id, ShiftState::Opened).await;
    let shift_id_hex: String =
        sqlx::query_scalar("SELECT lower(hex(shift_id)) FROM shifts WHERE shift_id = ?")
            .bind(shift_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let outcome = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let o = shifts::force_to_manual_reconciliation_with_audit(
                tx,
                shift_id,
                Some("op-007"),
                EVIDENCE,
            )
            .await
            .map_err(|e| anyhow::anyhow!("force-seam: {e}"))?;
            anyhow::Ok(o)
        })
    })
    .await
    .unwrap();
    assert!(
        matches!(outcome, shifts::ForceSeamOutcome::Applied),
        "Opened is an allowed source; expected Applied, got {outcome:?}"
    );

    // Oracle: the Critical success audit must carry the operator id verbatim.
    // Under the mutation `filter(|s| s.is_empty())` drops "op-007" → actor = NULL.
    let actor: Option<String> = sqlx::query_scalar(
        "SELECT actor FROM audit_log WHERE entity_type = 'shift' AND entity_id = ? \
         AND event_type = 'SHIFT_FORCE_TO_MANUAL_RECONCILIATION'",
    )
    .bind(&shift_id_hex)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        actor.as_deref(),
        Some("op-007"),
        "force-to-manual audit must record the operator actor for forensic attribution"
    );
}
