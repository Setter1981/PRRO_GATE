//! M3b W14a-2a — senior cashier close seam coverage (PR #66 R2 MED-4).
//!
//! Per spec §16.9 + PR #66 R1 M3 outcome-enum refactor.  Covers:
//! - Applied path (2 source states: Opened + ClosingLocalPendingDrain).
//! - RefusedNotClosable path (7 disallowed source states).
//! - RefusedCashierNotRegistered path (cashier_certs missing).
//!
//! Per spec §8 + Round 7 §8.1 forensic-traceability invariant: all
//! 3 paths emit audit_log rows (Info for Applied, Warning x2 for
//! refused), committed via Ok-return contract through `with_immediate`.

use prro::db::models::enums::{FiscalMode, ShiftState};
use prro::db::models::ids::{CashierId, DocumentId, ShiftId};
use prro::db::open_pool;
use prro::db::repositories::fiscal_number_config::{self as fn_repo, NewFnConfig};
use prro::db::repositories::shifts::{self, SeniorCloseError, SeniorCloseOutcome};
use prro::db::tx::with_immediate;

const ALL_NON_CLOSABLE_STATES: [ShiftState; 7] = [
    ShiftState::Created,
    ShiftState::Opening,
    ShiftState::OpenedLocalPendingDrain,
    ShiftState::Closing,
    ShiftState::Closed,
    ShiftState::RequiresManualReconciliation,
    ShiftState::Error,
];

const EVIDENCE: &str = r#"{"operator_id":"senior-007","reason_code":"end_of_day_close","free_text":"shift handover","timestamp_utc":"2026-05-17T20:00:00Z"}"#;

async fn fresh_with_fn() -> (sqlx::SqlitePool, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    std::mem::forget(dir);
    let pool = open_pool(&path).await.unwrap();
    fn_repo::insert(
        &pool,
        &NewFnConfig {
            fiscal_number: "9000090000".into(),
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
    (pool, "9000090000".to_string())
}

async fn seed_op_cert(pool: &sqlx::SqlitePool, fn_id: &str, ski_hex: &str) {
    sqlx::query(
        "INSERT INTO operator_certs(ski_hex, fiscal_number, cert_fingerprint, cert_der, \
            fetched_at, source, active) \
         VALUES (?, ?, 'fp', X'AB', '2026-05-17T00:00:00Z', 'manual', 0)",
    )
    .bind(ski_hex)
    .bind(fn_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_cashier_cert_binding(pool: &sqlx::SqlitePool, fn_id: &str, cashier_id: &str) {
    let primary_ski = "a".repeat(64);
    let deferred_ski = "b".repeat(64);
    seed_op_cert(pool, fn_id, &primary_ski).await;
    seed_op_cert(pool, fn_id, &deferred_ski).await;
    sqlx::query(
        "INSERT OR IGNORE INTO cashier_certs(cashier_id, fiscal_number, primary_cert_ski_hex, deferred_cert_ski_hex) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(cashier_id)
    .bind(fn_id)
    .bind(&primary_ski)
    .bind(&deferred_ski)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_shift_in_state(pool: &sqlx::SqlitePool, fn_id: &str, state: ShiftState) -> ShiftId {
    let id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, cash_balance_kop, \
            opened_by_cashier_id) \
         VALUES (?, ?, ?, 'ONLINE', 0, 'cashier-vasya')",
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

async fn shift_id_hex(pool: &sqlx::SqlitePool, id: ShiftId) -> String {
    sqlx::query_scalar("SELECT lower(hex(shift_id)) FROM shifts WHERE shift_id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

// ─── (1) Applied from Opened — happy path ───────────────────────────

#[tokio::test]
async fn senior_close_applied_from_opened() {
    let (pool, fn_id) = fresh_with_fn().await;
    let senior = CashierId::new("senior-cashier-1").unwrap();
    seed_cashier_cert_binding(&pool, &fn_id, senior.as_str()).await;
    let shift_id = seed_shift_in_state(&pool, &fn_id, ShiftState::Opened).await;
    let z_doc_id = DocumentId::new();

    let outcome = with_immediate(&pool, {
        let senior = senior.clone();
        move |tx| {
            Box::pin(async move {
                let o = shifts::senior_cashier_close_shift_with_audit(
                    tx, shift_id, &senior, z_doc_id, EVIDENCE,
                )
                .await
                .map_err(|e| anyhow::anyhow!("senior_close: {e}"))?;
                anyhow::Ok(o)
            })
        }
    })
    .await
    .unwrap();

    assert!(matches!(outcome, SeniorCloseOutcome::Applied));
    let row = shifts::get(&pool, shift_id).await.unwrap().unwrap();
    assert_eq!(row.state, ShiftState::Closed);
    assert_eq!(
        count_audit_events(
            &pool,
            &shift_id_hex(&pool, shift_id).await,
            "SHIFT_CLOSED_BY_SENIOR_CASHIER"
        )
        .await,
        1
    );
    assert_eq!(
        count_audit_events(
            &pool,
            &shift_id_hex(&pool, shift_id).await,
            "SHIFT_SENIOR_CLOSE_REFUSED"
        )
        .await,
        0
    );
}

// ─── (2) Applied from ClosingLocalPendingDrain — drain-finalisation path ─

#[tokio::test]
async fn senior_close_applied_from_closing_local_pending_drain() {
    let (pool, fn_id) = fresh_with_fn().await;
    let senior = CashierId::new("senior-cashier-2").unwrap();
    seed_cashier_cert_binding(&pool, &fn_id, senior.as_str()).await;
    let shift_id = seed_shift_in_state(&pool, &fn_id, ShiftState::ClosingLocalPendingDrain).await;
    let z_doc_id = DocumentId::new();

    let outcome = with_immediate(&pool, {
        let senior = senior.clone();
        move |tx| {
            Box::pin(async move {
                let o = shifts::senior_cashier_close_shift_with_audit(
                    tx, shift_id, &senior, z_doc_id, EVIDENCE,
                )
                .await
                .map_err(|e| anyhow::anyhow!("senior_close: {e}"))?;
                anyhow::Ok(o)
            })
        }
    })
    .await
    .unwrap();

    assert!(matches!(outcome, SeniorCloseOutcome::Applied));
    assert_eq!(
        shifts::get(&pool, shift_id).await.unwrap().unwrap().state,
        ShiftState::Closed
    );
}

// ─── (3) RefusedNotClosable from each of 7 non-allowed source states ────

#[tokio::test]
async fn senior_close_refused_not_closable_for_all_7_disallowed_sources() {
    let (pool, fn_id) = fresh_with_fn().await;
    let senior = CashierId::new("senior-cashier-3").unwrap();
    seed_cashier_cert_binding(&pool, &fn_id, senior.as_str()).await;

    for from in ALL_NON_CLOSABLE_STATES {
        let shift_id = seed_shift_in_state(&pool, &fn_id, from).await;
        let z_doc_id = DocumentId::new();

        let outcome = with_immediate(&pool, {
            let senior = senior.clone();
            move |tx| {
                Box::pin(async move {
                    let o = shifts::senior_cashier_close_shift_with_audit(
                        tx, shift_id, &senior, z_doc_id, EVIDENCE,
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!("senior_close: {e}"))?;
                    anyhow::Ok(o)
                })
            }
        })
        .await
        .unwrap();

        assert!(
            matches!(outcome, SeniorCloseOutcome::RefusedNotClosable { current_state } if current_state == from),
            "({from:?}) expected RefusedNotClosable, got {outcome:?}"
        );
        let row = shifts::get(&pool, shift_id).await.unwrap().unwrap();
        assert_eq!(row.state, from, "({from:?}) state must remain unchanged");
        // Forensic audit row MUST be committed via Ok-return contract.
        let refused = count_audit_events(
            &pool,
            &shift_id_hex(&pool, shift_id).await,
            "SHIFT_SENIOR_CLOSE_REFUSED",
        )
        .await;
        assert_eq!(
            refused, 1,
            "({from:?}) must emit SHIFT_SENIOR_CLOSE_REFUSED Warning audit \
             (forensic, committed via Ok-return contract per spec §8 + Round 7 §8.1)"
        );
        assert_eq!(
            count_audit_events(
                &pool,
                &shift_id_hex(&pool, shift_id).await,
                "SHIFT_CLOSED_BY_SENIOR_CASHIER"
            )
            .await,
            0,
            "({from:?}) refused path must NOT emit success audit"
        );
    }
}

// ─── (4) RefusedCashierNotRegistered — senior not in cashier_certs ──────

#[tokio::test]
async fn senior_close_refused_cashier_not_registered() {
    let (pool, fn_id) = fresh_with_fn().await;
    // Intentionally NO seed_cashier_cert_binding — senior is unknown.
    let senior = CashierId::new("senior-not-registered").unwrap();
    let shift_id = seed_shift_in_state(&pool, &fn_id, ShiftState::Opened).await;
    let z_doc_id = DocumentId::new();

    let outcome = with_immediate(&pool, {
        let senior = senior.clone();
        move |tx| {
            Box::pin(async move {
                let o = shifts::senior_cashier_close_shift_with_audit(
                    tx, shift_id, &senior, z_doc_id, EVIDENCE,
                )
                .await
                .map_err(|e| anyhow::anyhow!("senior_close: {e}"))?;
                anyhow::Ok(o)
            })
        }
    })
    .await
    .unwrap();

    assert!(matches!(
        outcome,
        SeniorCloseOutcome::RefusedCashierNotRegistered { .. }
    ));
    // State unchanged — was Opened, must stay Opened.
    assert_eq!(
        shifts::get(&pool, shift_id).await.unwrap().unwrap().state,
        ShiftState::Opened
    );
    // Forensic audit emitted.
    assert_eq!(
        count_audit_events(
            &pool,
            &shift_id_hex(&pool, shift_id).await,
            "SHIFT_SENIOR_CLOSE_REFUSED"
        )
        .await,
        1
    );
}

// ─── PR #66 R3 LOW-R3-3: error path tests ────────────────────────────

#[tokio::test]
async fn senior_close_error_invalid_evidence_json() {
    let (pool, fn_id) = fresh_with_fn().await;
    let senior = CashierId::new("senior-err-1").unwrap();
    seed_cashier_cert_binding(&pool, &fn_id, senior.as_str()).await;
    let shift_id = seed_shift_in_state(&pool, &fn_id, ShiftState::Opened).await;
    let z_doc_id = DocumentId::new();
    let hex_at_start = shift_id_hex(&pool, shift_id).await;

    // Invalid JSON evidence — must return Err(InvalidEvidenceJson)
    // BEFORE any state mutation or audit emission.
    let result: Result<SeniorCloseOutcome, SeniorCloseError> = {
        let senior = senior.clone();
        let res: anyhow::Result<Result<SeniorCloseOutcome, SeniorCloseError>> =
            with_immediate(&pool, move |tx| {
                Box::pin(async move {
                    // Capture the typed error inside the closure.
                    let r = shifts::senior_cashier_close_shift_with_audit(
                        tx,
                        shift_id,
                        &senior,
                        z_doc_id,
                        "not-json-at-all",
                    )
                    .await;
                    anyhow::Ok(r)
                })
            })
            .await;
        res.unwrap()
    };

    assert!(
        matches!(result, Err(SeniorCloseError::InvalidEvidenceJson(_))),
        "expected InvalidEvidenceJson error, got {result:?}"
    );

    // State must be unchanged + no audit emitted (validation rejects
    // BEFORE any DB write).
    assert_eq!(
        shifts::get(&pool, shift_id).await.unwrap().unwrap().state,
        ShiftState::Opened
    );
    assert_eq!(
        count_audit_events(&pool, &hex_at_start, "SHIFT_CLOSED_BY_SENIOR_CASHIER").await,
        0
    );
    assert_eq!(
        count_audit_events(&pool, &hex_at_start, "SHIFT_SENIOR_CLOSE_REFUSED").await,
        0
    );
}

#[tokio::test]
async fn senior_close_error_shift_not_found() {
    let (pool, _fn_id) = fresh_with_fn().await;
    let senior = CashierId::new("senior-err-2").unwrap();
    // Intentionally NOT seeded — shift_id is stale.
    let stale_id = ShiftId::new();
    let z_doc_id = DocumentId::new();

    let result: Result<SeniorCloseOutcome, SeniorCloseError> = {
        let senior = senior.clone();
        let res: anyhow::Result<Result<SeniorCloseOutcome, SeniorCloseError>> =
            with_immediate(&pool, move |tx| {
                Box::pin(async move {
                    let r = shifts::senior_cashier_close_shift_with_audit(
                        tx, stale_id, &senior, z_doc_id, EVIDENCE,
                    )
                    .await;
                    anyhow::Ok(r)
                })
            })
            .await;
        res.unwrap()
    };

    assert!(
        matches!(result, Err(SeniorCloseError::ShiftNotFound { .. })),
        "expected ShiftNotFound error, got {result:?}"
    );
}
