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
use prro::db::types::DbShiftId;
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
    .bind(DbShiftId(id))
    .bind(FN)
    .bind(state.as_str())
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
    .bind(shift_state.as_str())
    .bind(current.map(DbShiftId))
    .execute(pool)
    .await
    .unwrap();
}

async fn read_shift_state(pool: &sqlx::SqlitePool, id: ShiftId) -> ShiftState {
    sqlx::query_scalar::<_, prro::db::types::DbShiftState>(
        r#"SELECT state FROM shifts WHERE shift_id = ?"#,
    )
    .bind(DbShiftId(id))
    .fetch_one(pool)
    .await
    .unwrap()
    .0
}

async fn read_node_shift_state(pool: &sqlx::SqlitePool) -> ShiftState {
    sqlx::query_scalar::<_, prro::db::types::DbShiftState>(
        r#"SELECT shift_state FROM node_state WHERE fiscal_number = ?"#,
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .unwrap()
    .0
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
    // Projection must NOT be touched when the shift did not move — neither
    // shift_state nor the current_shift_id pointer.
    assert_eq!(read_shift_state(&pool, shift_id).await, ShiftState::Opened);
    assert_eq!(read_node_shift_state(&pool).await, ShiftState::Opened);
    assert_eq!(
        read_current_shift_id(&pool).await,
        Some(shift_id.as_bytes().to_vec()),
        "Forbidden must leave the current_shift_id pointer intact"
    );
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

    // The primary CAS that ran inside the envelope MUST be rolled back —
    // the shift stays Opened, no half-applied dual-write.
    assert_eq!(read_shift_state(&pool, shift_id).await, ShiftState::Opened);
    assert_eq!(read_node_shift_state(&pool).await, ShiftState::Opened);

    // Distinguish a genuine rollback from a partial commit (the dangerous
    // failure mode the mirror guards against): repair the projection pointer
    // and retry the SAME edge. It can only succeed if the shift is still
    // Opened — had the failed call left a half-applied `Closing`, this retry
    // would CAS-miss (Conflict), not Applied.
    sqlx::query("UPDATE node_state SET current_shift_id = ? WHERE fiscal_number = ?")
        .bind(DbShiftId(shift_id))
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();
    let retry = apply(&pool, shift_id, ShiftState::Opened, ShiftState::Closing)
        .await
        .unwrap();
    assert_eq!(
        retry,
        TransitionOutcome::Applied,
        "retry after repair must succeed — proving the drifted call left the shift in Opened, \
         not a half-applied Closing"
    );
    assert_eq!(read_shift_state(&pool, shift_id).await, ShiftState::Closing);
    assert_eq!(read_node_shift_state(&pool).await, ShiftState::Closing);
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

#[tokio::test]
async fn conflict_outcome_leaves_both_tables_untouched() {
    // The shift row has drifted to Closing (e.g. a concurrent admin path),
    // but the caller still asks for the whitelisted Opened → Closing edge.
    // The primary CAS (WHERE state = 'OPENED') matches 0 rows; the row
    // exists → Conflict. apply_shift_transition returns Ok(Conflict) — NOT an
    // Err — so the envelope does NOT roll back; the projection must stay put.
    let pool = fresh_pool_with_fn().await;
    let shift_id = ShiftId::new();
    seed_shift(&pool, shift_id, ShiftState::Closing).await;
    seed_node_state(&pool, ShiftState::Opened, Some(shift_id)).await;

    let outcome = apply(&pool, shift_id, ShiftState::Opened, ShiftState::Closing)
        .await
        .unwrap();

    assert_eq!(
        outcome,
        TransitionOutcome::Conflict {
            observed: ShiftState::Closing
        }
    );
    // Mirror is skipped on non-Applied — projection untouched.
    assert_eq!(read_shift_state(&pool, shift_id).await, ShiftState::Closing);
    assert_eq!(read_node_shift_state(&pool).await, ShiftState::Opened);
}

#[tokio::test]
async fn not_found_outcome_is_ok_not_err() {
    // A stale / never-seeded shift_id: the CAS matches 0 rows AND the row
    // does not exist → NotFound. Returned as Ok(NotFound), no rollback,
    // projection untouched.
    let pool = fresh_pool_with_fn().await;
    let stale = ShiftId::new();
    seed_node_state(&pool, ShiftState::Opened, None).await;

    let outcome = apply(&pool, stale, ShiftState::Opened, ShiftState::Closing)
        .await
        .unwrap();

    assert_eq!(outcome, TransitionOutcome::NotFound);
    assert_eq!(read_node_shift_state(&pool).await, ShiftState::Opened);
}

#[tokio::test]
async fn second_distinct_edge_also_dual_writes() {
    // A second, distinct whitelisted edge (Opened → ClosingLocalPendingDrain)
    // to confirm the mirror's from/to binding is generic, not hardcoded to
    // the Opened→Closing pair the happy-path test uses.
    let pool = fresh_pool_with_fn().await;
    let shift_id = ShiftId::new();
    seed_shift(&pool, shift_id, ShiftState::Opened).await;
    seed_node_state(&pool, ShiftState::Opened, Some(shift_id)).await;

    let outcome = apply(
        &pool,
        shift_id,
        ShiftState::Opened,
        ShiftState::ClosingLocalPendingDrain,
    )
    .await
    .unwrap();

    assert_eq!(outcome, TransitionOutcome::Applied);
    assert_eq!(
        read_shift_state(&pool, shift_id).await,
        ShiftState::ClosingLocalPendingDrain
    );
    assert_eq!(
        read_node_shift_state(&pool).await,
        ShiftState::ClosingLocalPendingDrain
    );
}

#[tokio::test]
async fn mirror_shift_state_arm_drift_also_rolls_back() {
    // The mirror CAS guards on BOTH current_shift_id AND shift_state = from.
    // Here the pointer is correct but the projection's shift_state has drifted
    // away from `from` (concurrent writer smuggled a different projection
    // state). The mirror must still find 0 rows → Err → rollback. Complements
    // projection_drift_rolls_back_the_whole_envelope (which breaks the
    // current_shift_id arm).
    let pool = fresh_pool_with_fn().await;
    let shift_id = ShiftId::new();
    seed_shift(&pool, shift_id, ShiftState::Opened).await;
    // Pointer matches, but projection shift_state is Closing, not Opened.
    seed_node_state(&pool, ShiftState::Closing, Some(shift_id)).await;

    let res = apply(&pool, shift_id, ShiftState::Opened, ShiftState::Closing).await;
    assert!(res.is_err(), "shift_state-arm drift must surface as Err");
    // Primary CAS rolled back — shift still Opened.
    assert_eq!(read_shift_state(&pool, shift_id).await, ShiftState::Opened);
}

#[tokio::test]
async fn terminal_close_pins_current_shift_id_behavior() {
    // PIN the CURRENT C1 behavior on a terminal close (Closing → Closed): the
    // mirror updates shift_state ONLY, so current_shift_id is left pointing at
    // the now-Closed shift. Whether a normal close should ALSO clear the
    // pointer (as orphan resolution does) is deferred to the RS-3 A-pieces
    // (stage_acquire intent-edge / ACK-edge wiring) — this test locks the
    // status quo so that decision is made deliberately, not by drift.
    let pool = fresh_pool_with_fn().await;
    let shift_id = ShiftId::new();
    seed_shift(&pool, shift_id, ShiftState::Closing).await;
    seed_node_state(&pool, ShiftState::Closing, Some(shift_id)).await;

    let outcome = apply(&pool, shift_id, ShiftState::Closing, ShiftState::Closed)
        .await
        .unwrap();

    assert_eq!(outcome, TransitionOutcome::Applied);
    assert_eq!(read_shift_state(&pool, shift_id).await, ShiftState::Closed);
    assert_eq!(read_node_shift_state(&pool).await, ShiftState::Closed);
    assert_eq!(
        read_current_shift_id(&pool).await,
        Some(shift_id.as_bytes().to_vec()),
        "C1 leaves current_shift_id at the closed shift on a normal close \
         (pointer lifecycle on close is an RS-3 A-piece decision)"
    );
}
