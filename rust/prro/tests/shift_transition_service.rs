//! RS-3 C1 — unit/integration coverage for the shift transition-service
//! (`services::shift::transition`).
//!
//! Asserts the dual-write biconditional (shifts primary ⇔ node_state
//! projection) on every edge, the structural-drift rollback when the
//! projection mirror can't find its row, and the asymmetric boot
//! orphan-quarantine path (shift → ERROR while the projection resets to
//! CLOSED + clears current_shift_id).

use prro::db::models::enums::ShiftState;
use prro::db::models::ids::ShiftId;
use prro::db::open_pool;
use prro::db::repositories::shifts::TransitionOutcome;
use prro::db::tx::with_immediate;
use prro::services::shift::transition as svc;

const FN: &str = "1234567890";

async fn fresh_pool_with_fn() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    std::mem::forget(dir);
    let pool = open_pool(&path).await.unwrap();
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    pool
}

async fn seed_shift(pool: &sqlx::SqlitePool, id: ShiftId, state: ShiftState) {
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, cash_balance_kop, \
            opened_by_cashier_id) \
         VALUES (?, ?, ?, 'ONLINE', 0, 'test-cashier')",
    )
    .bind(id)
    .bind(FN)
    .bind(state)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_node_state(
    pool: &sqlx::SqlitePool,
    shift_state: ShiftState,
    current: Option<ShiftId>,
) {
    sqlx::query(
        "INSERT INTO node_state (fiscal_number, mode, shift_state, current_shift_id, \
            next_lnd, backend_profile_id, transport_profile_id) \
         VALUES (?, 'ONLINE', ?, ?, 1, 'b1', 't1')",
    )
    .bind(FN)
    .bind(shift_state)
    .bind(current)
    .execute(pool)
    .await
    .unwrap();
}

async fn read_shift_state(pool: &sqlx::SqlitePool, id: ShiftId) -> ShiftState {
    sqlx::query_scalar(r#"SELECT state as "state: ShiftState" FROM shifts WHERE shift_id = ?"#)
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_node_shift_state(pool: &sqlx::SqlitePool) -> ShiftState {
    sqlx::query_scalar(
        r#"SELECT shift_state as "s: ShiftState" FROM node_state WHERE fiscal_number = ?"#,
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn read_current_shift_id(pool: &sqlx::SqlitePool) -> Option<Vec<u8>> {
    sqlx::query_scalar("SELECT current_shift_id FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Helper: run one `apply_shift_transition` inside a `with_immediate`
/// envelope and surface the outcome (or the rollback error).
async fn apply(
    pool: &sqlx::SqlitePool,
    shift_id: ShiftId,
    from: ShiftState,
    to: ShiftState,
) -> anyhow::Result<TransitionOutcome> {
    with_immediate(pool, move |tx| {
        Box::pin(async move { svc::apply_shift_transition(tx, FN, shift_id, from, to).await })
    })
    .await
}

#[tokio::test]
async fn applied_edge_dual_writes_both_tables() {
    let pool = fresh_pool_with_fn().await;
    let shift_id = ShiftId::new();
    seed_shift(&pool, shift_id, ShiftState::Opened).await;
    seed_node_state(&pool, ShiftState::Opened, Some(shift_id)).await;

    let outcome = apply(&pool, shift_id, ShiftState::Opened, ShiftState::Closing)
        .await
        .unwrap();

    assert_eq!(outcome, TransitionOutcome::Applied);
    // Primary AND projection both moved — the dual-write biconditional.
    assert_eq!(read_shift_state(&pool, shift_id).await, ShiftState::Closing);
    assert_eq!(read_node_shift_state(&pool).await, ShiftState::Closing);
}

#[tokio::test]
async fn forbidden_edge_touches_neither_table() {
    let pool = fresh_pool_with_fn().await;
    let shift_id = ShiftId::new();
    seed_shift(&pool, shift_id, ShiftState::Opened).await;
    seed_node_state(&pool, ShiftState::Opened, Some(shift_id)).await;

    // Opened → Created is not a whitelist edge.
    let outcome = apply(&pool, shift_id, ShiftState::Opened, ShiftState::Created)
        .await
        .unwrap();

    assert!(matches!(outcome, TransitionOutcome::Forbidden { .. }));
    // Projection must NOT be touched when the shift did not move.
    assert_eq!(read_shift_state(&pool, shift_id).await, ShiftState::Opened);
    assert_eq!(read_node_shift_state(&pool).await, ShiftState::Opened);
}

#[tokio::test]
async fn projection_drift_rolls_back_the_whole_envelope() {
    let pool = fresh_pool_with_fn().await;
    let shift_id = ShiftId::new();
    let other = ShiftId::new();
    seed_shift(&pool, shift_id, ShiftState::Opened).await;
    // node_state.current_shift_id points at a DIFFERENT shift → the mirror
    // CAS (WHERE current_shift_id = shift_id) matches 0 rows even though the
    // primary shift CAS succeeds → structural-drift Err → rollback.
    seed_node_state(&pool, ShiftState::Opened, Some(other)).await;

    let res = apply(&pool, shift_id, ShiftState::Opened, ShiftState::Closing).await;
    assert!(res.is_err(), "mirror drift must surface as Err");

    // The primary CAS that succeeded inside the envelope MUST be rolled
    // back — the shift stays Opened, no half-applied dual-write.
    assert_eq!(read_shift_state(&pool, shift_id).await, ShiftState::Opened);
    assert_eq!(read_node_shift_state(&pool).await, ShiftState::Opened);
}

#[tokio::test]
async fn orphan_quarantine_is_asymmetric_shift_error_projection_closed() {
    let pool = fresh_pool_with_fn().await;
    let shift_id = ShiftId::new();
    seed_shift(&pool, shift_id, ShiftState::Opening).await;
    seed_node_state(&pool, ShiftState::Opening, Some(shift_id)).await;

    with_immediate(&pool, move |tx| {
        Box::pin(async move {
            svc::force_orphan_shift_to_error(tx, shift_id).await?;
            svc::clear_active_shift_projection(tx, FN).await?;
            anyhow::Ok(())
        })
    })
    .await
    .unwrap();

    // Shift row quarantined to ERROR; projection reset to "no active shift".
    assert_eq!(read_shift_state(&pool, shift_id).await, ShiftState::Error);
    assert_eq!(read_node_shift_state(&pool).await, ShiftState::Closed);
    assert!(
        read_current_shift_id(&pool).await.is_none(),
        "current_shift_id must be cleared on orphan resolution"
    );
}

#[tokio::test]
async fn clear_projection_repairs_dangling_pointer_with_no_backing_shift() {
    // node_state says OPENING but there is no backing shift row at all —
    // the clear must still reset it (runs even with zero orphan rows).
    let pool = fresh_pool_with_fn().await;
    seed_node_state(&pool, ShiftState::Opening, None).await;

    with_immediate(&pool, move |tx| {
        Box::pin(async move { svc::clear_active_shift_projection(tx, FN).await })
    })
    .await
    .unwrap();

    assert_eq!(read_node_shift_state(&pool).await, ShiftState::Closed);
}

#[tokio::test]
async fn clear_projection_is_noop_outside_opening_closing() {
    // A projection already in OPENED must be left untouched (the guard is
    // scoped to the orphan source states OPENING/CLOSING).
    let pool = fresh_pool_with_fn().await;
    let shift_id = ShiftId::new();
    seed_shift(&pool, shift_id, ShiftState::Opened).await;
    seed_node_state(&pool, ShiftState::Opened, Some(shift_id)).await;

    with_immediate(&pool, move |tx| {
        Box::pin(async move { svc::clear_active_shift_projection(tx, FN).await })
    })
    .await
    .unwrap();

    assert_eq!(read_node_shift_state(&pool).await, ShiftState::Opened);
    assert_eq!(
        read_current_shift_id(&pool).await,
        Some(shift_id.as_bytes().to_vec()),
        "non-orphan projection + pointer must be left intact"
    );
}
