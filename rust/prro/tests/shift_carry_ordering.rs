//! bd `PRRO_GATE-seb` — the opening cash carry must come from the MOST-RECENTLY-CLOSED
//! shift.
//!
//! Production never writes `shifts.serial` (the only prod INSERTs omit the column, no
//! `UPDATE … SET serial` exists, and the only writers are test fixtures), so every
//! production shift row has `serial = NULL`. `ORDER BY serial DESC LIMIT 1` over CLOSED
//! shifts was therefore a TOTAL TIE and SQLite returned an arbitrary row — empirically
//! the FIRST-inserted one, i.e. the OLDEST closed shift. The carry feeds a new shift's
//! opening cash via `stage_acquire`, so from the third shift onward every shift opened
//! with a stale balance.
//!
//! These tests seed shifts EXACTLY as production does — `serial` left NULL, `closed_at`
//! stamped like the real close path (`CURRENT_TIMESTAMP`, second-granular) — so they
//! bite the real defect rather than a fixture artefact.

use prro::db::models::ids::ShiftId;
use prro::db::open_pool;
use prro::db::types::DbShiftId;
use sqlx::SqlitePool;

const FN: &str = "4000000001";

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = open_pool(&dir.path().join("carry.db")).await.unwrap();
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode, \
            offline_enabled, national_check_enabled, tsp_enabled, min_offline_codes, \
            max_offline_codes) VALUES (?, '12345678', 'test', 1, 0, 0, 0, 0)",
    )
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    (dir, pool)
}

/// Insert a CLOSED shift the way production does: **no `serial`**, `closed_at` stamped.
async fn seed_closed_shift(pool: &SqlitePool, cash_kop: i64, closed_at: &str) -> ShiftId {
    let id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, cash_balance_kop, \
            opened_by_cashier_id, closed_at) \
         VALUES (?, ?, 'CLOSED', 'ONLINE', ?, 'cashier', ?)",
    )
    .bind(DbShiftId(id))
    .bind(FN)
    .bind(cash_kop)
    .bind(closed_at)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// The core defect: with two CLOSED shifts the carry must be the LATER one's balance.
/// Inserted oldest-first so a tie resolved by insertion order returns the WRONG (older)
/// row — which is exactly what `ORDER BY serial DESC` did while every `serial` is NULL.
#[tokio::test]
async fn opening_carry_takes_the_most_recently_closed_shift() {
    let (_d, pool) = fresh_pool().await;
    seed_closed_shift(&pool, 100, "2026-07-01 10:00:00").await;
    seed_closed_shift(&pool, 200, "2026-07-02 10:00:00").await;

    let carry = prro::services::cash_ledger::opening_carry_for_fn(&pool, FN)
        .await
        .unwrap();
    assert_eq!(
        carry, 200,
        "the opening carry must come from the MOST-RECENTLY-CLOSED shift (closed_at \
         2026-07-02, balance 200), not from an arbitrary tied row"
    );
}

/// Insertion order must not decide the winner: the SAME ledger with the NEWER shift
/// inserted FIRST must still yield the newer balance. Guards against a fix that merely
/// flips the tie-break direction instead of ordering by a real key.
#[tokio::test]
async fn opening_carry_is_insertion_order_independent() {
    let (_d, pool) = fresh_pool().await;
    seed_closed_shift(&pool, 200, "2026-07-02 10:00:00").await;
    seed_closed_shift(&pool, 100, "2026-07-01 10:00:00").await;

    let carry = prro::services::cash_ledger::opening_carry_for_fn(&pool, FN)
        .await
        .unwrap();
    assert_eq!(
        carry, 200,
        "the newest close wins regardless of the order rows were inserted"
    );
}

/// `closed_at` is `CURRENT_TIMESTAMP` in production — second-granular — so two shifts
/// CAN close within the same second. The final tie-break must still be deterministic
/// and pick the row inserted last (the later close).
#[tokio::test]
async fn opening_carry_breaks_a_same_second_close_tie_deterministically() {
    let (_d, pool) = fresh_pool().await;
    seed_closed_shift(&pool, 100, "2026-07-01 10:00:00").await;
    seed_closed_shift(&pool, 200, "2026-07-01 10:00:00").await; // same second, later row

    let carry = prro::services::cash_ledger::opening_carry_for_fn(&pool, FN)
        .await
        .unwrap();
    assert_eq!(
        carry, 200,
        "same-second closes must resolve to the LAST-appended row (rowid tie-break), \
         not to an arbitrary one"
    );
}

/// No closed shift → carry is 0 (first shift ever for the FN).
#[tokio::test]
async fn opening_carry_is_zero_without_a_closed_shift() {
    let (_d, pool) = fresh_pool().await;
    let carry = prro::services::cash_ledger::opening_carry_for_fn(&pool, FN)
        .await
        .unwrap();
    assert_eq!(carry, 0);
}
