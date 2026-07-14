//! A′.3 PR-O1 — node-mode setters for the operator offline seam (B1).
//!
//! `set_mode_offline_tx` (ONLINE→OFFLINE) and `set_mode_going_online_tx`
//! (OFFLINE|GOING_OFFLINE→GOING_ONLINE) are guarded single-statement CAS
//! UPDATEs mirroring `set_mode_blocked_tx`'s shape. They are MODE-ONLY —
//! `shift_state` is never touched (Frozen invariant #3: a GO_OFFLINE is a
//! node-mode flip, NOT a channel switch; it is legal across an open shift,
//! which is what keeps the online-preopen decoupling pin valid).

use prro::db::models::enums::{NodeMode, ShiftState};
use prro::db::repositories::node_state;
use prro::db::tx::with_immediate;

const FN: &str = "1234567890";

async fn fresh_pool() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs migrations");
    sqlx::query("INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) VALUES (?, '12345678', 'test')")
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();
    (dir, pool)
}

async fn seed_node_state(pool: &sqlx::SqlitePool, mode: NodeMode, shift_state: ShiftState) {
    sqlx::query(
        "INSERT INTO node_state(fiscal_number, mode, shift_state, next_lnd) VALUES (?, ?, ?, 1)",
    )
    .bind(FN)
    .bind(mode.as_str())
    .bind(shift_state.as_str())
    .execute(pool)
    .await
    .unwrap();
}

async fn read_mode(pool: &sqlx::SqlitePool) -> String {
    sqlx::query_scalar::<_, String>("SELECT mode FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_shift_state(pool: &sqlx::SqlitePool) -> String {
    sqlx::query_scalar::<_, String>("SELECT shift_state FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Core of RP-O1-1 / RP-O1-11: ONLINE→OFFLINE flips mode ONLY; shift_state
/// (an OPEN shift) is left intact.
#[tokio::test]
async fn set_mode_offline_tx_flips_online_to_offline_leaving_shift_state() {
    let (_dir, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::Online, ShiftState::Opened).await;

    let applied = with_immediate(&pool, |tx| {
        Box::pin(async move {
            Ok::<bool, anyhow::Error>(node_state::set_mode_offline_tx(tx, FN).await?)
        })
    })
    .await
    .unwrap();

    assert!(applied, "CAS must apply from ONLINE");
    assert_eq!(read_mode(&pool).await, "OFFLINE");
    assert_eq!(
        read_shift_state(&pool).await,
        "OPENED",
        "GO_OFFLINE must NOT touch shift_state (Frozen #3)"
    );
}

/// The ONLINE guard bites: a non-ONLINE node is not flipped (rows==0).
#[tokio::test]
async fn set_mode_offline_tx_guard_refuses_non_online() {
    let (_dir, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::Blocked, ShiftState::Opened).await;

    let applied = with_immediate(&pool, |tx| {
        Box::pin(async move {
            Ok::<bool, anyhow::Error>(node_state::set_mode_offline_tx(tx, FN).await?)
        })
    })
    .await
    .unwrap();

    assert!(!applied, "CAS must NOT apply from a non-ONLINE mode");
    assert_eq!(read_mode(&pool).await, "BLOCKED");
}

/// GO_ONLINE recovery seam: OFFLINE→GOING_ONLINE.
#[tokio::test]
async fn set_mode_going_online_tx_flips_offline_to_going_online() {
    let (_dir, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::Offline, ShiftState::Opened).await;

    let applied = with_immediate(&pool, |tx| {
        Box::pin(async move {
            Ok::<bool, anyhow::Error>(node_state::set_mode_going_online_tx(tx, FN).await?)
        })
    })
    .await
    .unwrap();

    assert!(applied);
    assert_eq!(read_mode(&pool).await, "GOING_ONLINE");
}

/// GO_ONLINE also accepts the GOING_OFFLINE transient.
#[tokio::test]
async fn set_mode_going_online_tx_accepts_going_offline() {
    let (_dir, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::GoingOffline, ShiftState::Opened).await;

    let applied = with_immediate(&pool, |tx| {
        Box::pin(async move {
            Ok::<bool, anyhow::Error>(node_state::set_mode_going_online_tx(tx, FN).await?)
        })
    })
    .await
    .unwrap();

    assert!(applied);
    assert_eq!(read_mode(&pool).await, "GOING_ONLINE");
}

/// GO_ONLINE guard bites on an ONLINE node (nothing to recover).
#[tokio::test]
async fn set_mode_going_online_tx_guard_refuses_online() {
    let (_dir, pool) = fresh_pool().await;
    seed_node_state(&pool, NodeMode::Online, ShiftState::Opened).await;

    let applied = with_immediate(&pool, |tx| {
        Box::pin(async move {
            Ok::<bool, anyhow::Error>(node_state::set_mode_going_online_tx(tx, FN).await?)
        })
    })
    .await
    .unwrap();

    assert!(!applied, "GO_ONLINE must not apply from ONLINE");
    assert_eq!(read_mode(&pool).await, "ONLINE");
}
