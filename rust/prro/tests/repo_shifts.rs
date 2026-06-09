use prro::db::models::enums::FiscalMode;
use prro::db::tx::with_immediate;
use prro::db::{
    models::{enums::ShiftState, ids::ShiftId},
    open_pool,
    repositories::{fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig, shifts},
};

/// Test-only helper that drives `shifts::transition` through the
/// production `with_immediate` envelope (per ADR-M3-A4 / W0-2 §4.4).
async fn tx_shift_transition(
    pool: &sqlx::SqlitePool,
    id: ShiftId,
    from: ShiftState,
    to: ShiftState,
) -> anyhow::Result<bool> {
    with_immediate(pool, move |tx| {
        Box::pin(async move { Ok(shifts::transition(tx, id, from, to).await?) })
    })
    .await
}

async fn fresh_with_fn() -> (sqlx::SqlitePool, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    std::mem::forget(dir);
    let pool = open_pool(&path).await.unwrap();
    fn_repo::insert(
        &pool,
        &NewFnConfig {
            fiscal_number: "4000000001".into(),
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
    (pool, "4000000001".to_string())
}

#[tokio::test]
async fn insert_created_then_get() {
    let (pool, fn_id) = fresh_with_fn().await;
    let id = ShiftId::new();
    shifts::insert_created(&pool, id, &fn_id, "ONLINE", "test-cashier")
        .await
        .unwrap();
    let row = shifts::get(&pool, id).await.unwrap().unwrap();
    assert_eq!(row.shift_id, id);
    assert_eq!(row.fiscal_number, fn_id);
    assert_eq!(row.state, ShiftState::Created);
    assert_eq!(row.cash_balance_kop, 0);
    assert!(row.serial.is_none());
}

#[tokio::test]
async fn allowed_transitions_succeed() {
    let (pool, fn_id) = fresh_with_fn().await;
    let id = ShiftId::new();
    shifts::insert_created(&pool, id, &fn_id, "ONLINE", "test-cashier")
        .await
        .unwrap();
    assert!(
        tx_shift_transition(&pool, id, ShiftState::Created, ShiftState::Opening)
            .await
            .unwrap()
    );
    assert!(
        tx_shift_transition(&pool, id, ShiftState::Opening, ShiftState::Opened)
            .await
            .unwrap()
    );
    assert_eq!(
        shifts::get(&pool, id).await.unwrap().unwrap().state,
        ShiftState::Opened
    );
}

#[tokio::test]
async fn forbidden_transitions_blocked_in_code() {
    // Code-level whitelist must short-circuit BEFORE touching the DB.
    let (pool, fn_id) = fresh_with_fn().await;
    let id = ShiftId::new();
    shifts::insert_created(&pool, id, &fn_id, "ONLINE", "test-cashier")
        .await
        .unwrap();
    let did_it = tx_shift_transition(&pool, id, ShiftState::Created, ShiftState::Closed)
        .await
        .unwrap();
    assert!(!did_it, "Created → Closed must be blocked by whitelist");
    assert_eq!(
        shifts::get(&pool, id).await.unwrap().unwrap().state,
        ShiftState::Created,
        "row state must remain unchanged after blocked transition"
    );
}

#[tokio::test]
async fn cas_blocks_when_state_diverged() {
    // Allowed transition Opening → Opened, but row is in Created — CAS must reject.
    let (pool, fn_id) = fresh_with_fn().await;
    let id = ShiftId::new();
    shifts::insert_created(&pool, id, &fn_id, "ONLINE", "test-cashier")
        .await
        .unwrap();
    let did_it = tx_shift_transition(&pool, id, ShiftState::Opening, ShiftState::Opened)
        .await
        .unwrap();
    assert!(!did_it, "CAS must reject when actual state ≠ expected from");
    assert_eq!(
        shifts::get(&pool, id).await.unwrap().unwrap().state,
        ShiftState::Created
    );
}

#[tokio::test]
async fn allowed_transition_table_matrix() {
    use ShiftState::*;
    // M3b W14a-2a: 14-edge whitelist per spec §4.1.
    // Allowed (must be true) — 14 edges.
    assert!(shifts::allowed_transition(Created, Opening)); // 1
    assert!(shifts::allowed_transition(Created, OpenedLocalPendingDrain)); // 2
    assert!(shifts::allowed_transition(Opening, Opened)); // 3
    assert!(shifts::allowed_transition(
        Opening,
        RequiresManualReconciliation
    )); // 4
    assert!(shifts::allowed_transition(OpenedLocalPendingDrain, Opened)); // 5
    assert!(shifts::allowed_transition(
        OpenedLocalPendingDrain,
        RequiresManualReconciliation
    )); // 6
    assert!(shifts::allowed_transition(
        OpenedLocalPendingDrain,
        ClosingLocalPendingDrain
    )); // 7
    assert!(shifts::allowed_transition(Opened, Closing)); // 8
    assert!(shifts::allowed_transition(Opened, ClosingLocalPendingDrain)); // 9
    assert!(shifts::allowed_transition(Closing, Closed)); // 10
    assert!(shifts::allowed_transition(Closing, Opened)); // 11 — DocumentReject rollback
    assert!(shifts::allowed_transition(
        Closing,
        RequiresManualReconciliation
    )); // 12
    assert!(shifts::allowed_transition(ClosingLocalPendingDrain, Closed)); // 13
    assert!(shifts::allowed_transition(
        ClosingLocalPendingDrain,
        RequiresManualReconciliation
    )); // 14

    // Forbidden (must be false) — sample of structural violations.
    // Per spec §4.4: Error is reachable ONLY via force_to_error_with_audit seam;
    // no whitelist edge ever lands on Error.
    assert!(!shifts::allowed_transition(Opening, Error)); // M3a edge REMOVED per §4.4
    assert!(!shifts::allowed_transition(Closing, Error)); // M3a edge REMOVED per §4.4
    assert!(!shifts::allowed_transition(Error, Closed)); // M3a recovery edge REMOVED — terminal force-seam state
    assert!(!shifts::allowed_transition(Closed, Opening)); // re-open final state
    assert!(!shifts::allowed_transition(Created, Opened)); // skipped Opening
    assert!(!shifts::allowed_transition(Created, Closed)); // skipped pipeline
    assert!(!shifts::allowed_transition(Opened, Created)); // backwards
    assert!(!shifts::allowed_transition(Closed, Closed)); // self-loop
    assert!(!shifts::allowed_transition(Error, Opened)); // recovery → not back to Opened (terminal)
                                                         // Forbidden: ClosingLocalPendingDrain → Opened (per §4.4 "No ClosingLocalPendingDrain → Opened")
    assert!(!shifts::allowed_transition(
        ClosingLocalPendingDrain,
        Opened
    ));
    // Forbidden: OpenedLocalPendingDrain → Closed directly (must route via ClosingLocalPendingDrain)
    assert!(!shifts::allowed_transition(OpenedLocalPendingDrain, Closed));
    // Forbidden: OpenedLocalPendingDrain → Opening (forward-only on open-side)
    assert!(!shifts::allowed_transition(
        OpenedLocalPendingDrain,
        Opening
    ));
}

/// RS-3 C2 (migration 023) — at most ONE active shift per fiscal_number.
/// A second active (CREATED) shift for the same FN is rejected by the
/// `uq_active_shift_per_fiscal` partial-unique index (the WL-1 foundation;
/// `insert_created` is a raw INSERT with no app guard, so the DB index IS the
/// enforcer).
#[tokio::test]
async fn second_active_shift_per_fiscal_is_rejected() {
    let (pool, fn_id) = fresh_with_fn().await;
    shifts::insert_created(&pool, ShiftId::new(), &fn_id, "ONLINE", "csh-1")
        .await
        .expect("first active shift OK");
    let err = shifts::insert_created(&pool, ShiftId::new(), &fn_id, "ONLINE", "csh-2")
        .await
        .expect_err("a second active shift for the same FN must be rejected by the uq index");
    assert!(
        format!("{err}").to_lowercase().contains("unique"),
        "expected a UNIQUE-constraint error, got: {err}"
    );
}

/// RS-3 C2 — the partial index covers ACTIVE states only: once the prior
/// shift leaves the active set (CLOSED — terminal, excluded), a new shift for
/// the same FN is allowed.
#[tokio::test]
async fn new_shift_allowed_after_prior_left_active_set() {
    let (pool, fn_id) = fresh_with_fn().await;
    let id1 = ShiftId::new();
    shifts::insert_created(&pool, id1, &fn_id, "ONLINE", "csh-1")
        .await
        .unwrap();
    // Drive id1 OUT of the active set (raw UPDATE to terminal CLOSED — the full
    // transition path is exercised elsewhere; here we only set the index
    // precondition).
    sqlx::query("UPDATE shifts SET state = 'CLOSED' WHERE shift_id = ?")
        .bind(id1)
        .execute(&pool)
        .await
        .unwrap();
    shifts::insert_created(&pool, ShiftId::new(), &fn_id, "ONLINE", "csh-2")
        .await
        .expect(
            "a new active shift is allowed once the prior is CLOSED (excluded from the uq index)",
        );
}
