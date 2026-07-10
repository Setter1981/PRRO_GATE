//! A′.1 pieces 0b+1 — the shift CREATE primitive.
//!
//! Covers the new `create_shift_tx` (transition-service home per architect
//! ruling 2026-07-05) + `insert_created_tx` (tx-bound) + the deterministic
//! `ShiftId` derivation + the partial-unique-index migration (026).
//!
//! Design pins verified here (teeth):
//!  - happy: CLOSED→CREATED CASes the projection + INSERTs the shifts row.
//!  - (c) the CAS refuses (fail-closed rollback) when shift_state != CLOSED.
//!  - (d) the derivation is deterministic; a re-inserted shift_id PK-collides.
//!  - (f) the partial-unique index rejects a 2nd open-state row for the FN.
//!  - (a) `apply_shift_transition(Closed→Created)` is Forbidden — create must not route through it (pin).
//!  - (e) an already-applied edge re-drive returns Conflict, not error (pin).

use prro::db::models::enums::ShiftState;
use prro::db::models::ids::{DocumentId, ShiftId};
use prro::db::open_pool;
use prro::db::repositories::shifts::{self, TransitionOutcome};
use prro::db::tx::with_immediate;
use prro::services::shift::transition as svc;

const FN: &str = "1234567890";
const CASHIER: &str = "test-cashier";

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

async fn seed_shift(pool: &sqlx::SqlitePool, id: ShiftId, state: ShiftState) {
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, cash_balance_kop, \
            opened_by_cashier_id) \
         VALUES (?, ?, ?, 'ONLINE', 0, 'seed')",
    )
    .bind(id)
    .bind(FN)
    .bind(state)
    .execute(pool)
    .await
    .unwrap();
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

// ── happy path ──────────────────────────────────────────────────────────
#[tokio::test]
async fn create_shift_tx_from_closed_dual_writes_shifts_and_projection() {
    let pool = fresh_pool_with_fn().await;
    seed_node_state(&pool, ShiftState::Closed, None).await;
    let shift_id = ShiftId::new();

    with_immediate(&pool, move |tx| {
        Box::pin(async move { svc::create_shift_tx(tx, FN, shift_id, "ONLINE", CASHIER, 0).await })
    })
    .await
    .unwrap();

    // shifts row minted as CREATED
    let row = shifts::get(&pool, shift_id)
        .await
        .unwrap()
        .expect("shifts row exists");
    assert_eq!(row.state, ShiftState::Created);
    assert_eq!(row.fiscal_number, FN);
    // node_state projection: CLOSED→CREATED + pointer set
    assert_eq!(read_node_shift_state(&pool).await, ShiftState::Created);
    assert_eq!(
        read_current_shift_id(&pool).await.as_deref(),
        Some(shift_id.as_bytes().as_slice()),
        "current_shift_id points at the new shift"
    );
}

// ── (c) CAS refuses when shift_state != CLOSED; whole envelope rolls back ─
#[tokio::test]
async fn create_shift_tx_refuses_and_rolls_back_when_not_closed() {
    let pool = fresh_pool_with_fn().await;
    let existing = ShiftId::new();
    seed_node_state(&pool, ShiftState::Opened, Some(existing)).await;
    let shift_id = ShiftId::new();

    let res = with_immediate(&pool, move |tx| {
        Box::pin(async move { svc::create_shift_tx(tx, FN, shift_id, "ONLINE", CASHIER, 0).await })
    })
    .await;

    assert!(
        res.is_err(),
        "create must fail-closed when shift_state != CLOSED"
    );
    // Rollback: neither the shifts INSERT nor the projection survived.
    assert!(
        shifts::get(&pool, shift_id).await.unwrap().is_none(),
        "shifts INSERT rolled back"
    );
    assert_eq!(
        read_node_shift_state(&pool).await,
        ShiftState::Opened,
        "projection untouched"
    );
    assert_eq!(
        read_current_shift_id(&pool).await.as_deref(),
        Some(existing.as_bytes().as_slice()),
        "pointer untouched"
    );
}

// ── (d1) deterministic derivation ───────────────────────────────────────
#[test]
fn deterministic_shift_id_is_stable_and_distinct() {
    let doc_a = DocumentId::new();
    let doc_b = DocumentId::new();
    assert_eq!(
        ShiftId::deterministic_for_shift_open(doc_a),
        ShiftId::deterministic_for_shift_open(doc_a),
        "same document_id → same shift_id (idempotent re-create collides on PK)"
    );
    assert_ne!(
        ShiftId::deterministic_for_shift_open(doc_a),
        ShiftId::deterministic_for_shift_open(doc_b),
        "distinct documents → distinct shift_ids"
    );
    assert_ne!(
        ShiftId::deterministic_for_shift_open(doc_a).as_bytes(),
        doc_a.as_bytes(),
        "derived id is namespaced, not equal to the document_id itself"
    );
}

// ── (d2) re-inserting the same shift_id PK-collides (fail-closed backstop) ─
#[tokio::test]
async fn insert_created_tx_duplicate_shift_id_is_pk_error() {
    let pool = fresh_pool_with_fn().await;
    let shift_id = ShiftId::new();

    with_immediate(&pool, move |tx| {
        Box::pin(async move {
            shifts::insert_created_tx(tx, shift_id, FN, "ONLINE", CASHIER, 0)
                .await
                .map_err(Into::into)
        })
    })
    .await
    .unwrap();

    let res = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            shifts::insert_created_tx(tx, shift_id, FN, "ONLINE", CASHIER, 0)
                .await
                .map_err(Into::into)
        })
    })
    .await;

    assert!(
        res.is_err(),
        "duplicate shift_id must collide on the shifts PK"
    );
}

// ── (f) partial-unique index rejects a 2nd open-state row for the FN ─────
#[tokio::test]
async fn partial_unique_index_rejects_second_open_state_row() {
    let pool = fresh_pool_with_fn().await;
    // Row #1: an already-open shift (Opened) for the FN.
    seed_shift(&pool, ShiftId::new(), ShiftState::Opened).await;
    // Row #2: a distinct shift_id in another open state (Created) for the SAME FN.
    let second = ShiftId::new();
    let res = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            shifts::insert_created_tx(tx, second, FN, "ONLINE", CASHIER, 0)
                .await
                .map_err(Into::into)
        })
    })
    .await;

    assert!(
        res.is_err(),
        "partial-unique index must reject a 2nd open-state shift for the FN"
    );
}

// ── (a) create must NOT route through apply_shift_transition (pin) ───────
#[tokio::test]
async fn apply_shift_transition_closed_to_created_is_forbidden() {
    let pool = fresh_pool_with_fn().await;
    seed_node_state(&pool, ShiftState::Closed, None).await;
    let shift_id = ShiftId::new();

    let outcome = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            svc::apply_shift_transition(tx, FN, shift_id, ShiftState::Closed, ShiftState::Created)
                .await
        })
    })
    .await
    .unwrap();

    assert!(
        matches!(outcome, TransitionOutcome::Forbidden { .. }),
        "(Closed→Created) is not a whitelist edge — create cannot ride apply_shift_transition"
    );
}

// ── (e) re-driven edge is a no-op Conflict, not an error (pin) ───────────
#[tokio::test]
async fn apply_shift_transition_redrive_is_conflict_not_error() {
    let pool = fresh_pool_with_fn().await;
    let shift_id = ShiftId::new();
    seed_shift(&pool, shift_id, ShiftState::Opened).await;
    seed_node_state(&pool, ShiftState::Opened, Some(shift_id)).await;

    // Re-drive of edge 3 (Opening→Opened) after the shift already reached Opened.
    let outcome = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            svc::apply_shift_transition(tx, FN, shift_id, ShiftState::Opening, ShiftState::Opened)
                .await
        })
    })
    .await
    .unwrap();

    assert!(
        matches!(
            outcome,
            TransitionOutcome::Conflict {
                observed: ShiftState::Opened
            }
        ),
        "an already-applied edge re-drive is a no-op Conflict, not an error"
    );
}
