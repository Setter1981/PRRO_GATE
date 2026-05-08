//! Single source of truth for write transactions.
//!
//! `pool.begin()` opens BEGIN DEFERRED; nesting BEGIN IMMEDIATE inside
//! is a SQLite error.  This helper acquires a raw connection and
//! issues `BEGIN IMMEDIATE` directly, ensuring writers contend on the
//! RESERVED lock from the very first statement (spec decision #39).
//!
//! M3a W2 — sealed `WriteTxConn<'_>` newtype (per ADR-M3-A4 / W0-2
//! §4.4 option (b)) gates every transactional repository helper
//! behind `with_immediate`.  `transition_state` and `shifts::transition`
//! take `&mut WriteTxConn<'_>` instead of `&SqlitePool`; the only
//! way to obtain a `WriteTxConn` is from inside `with_immediate`'s
//! closure (or from `new_for_test`, gated behind `cfg(test)` and
//! `pub(super)`).  This is what closes PRRO_GATE-k99 by construction:
//! a caller cannot call `transition_state` without the surrounding
//! BEGIN IMMEDIATE / COMMIT envelope, so the unhappy-path
//! disambiguation SELECT runs through the same connection inside
//! the same transaction and the CAS-vs-SELECT race vanishes.

use std::ops::{Deref, DerefMut};

use futures::future::BoxFuture;
use sqlx::{SqliteConnection, SqlitePool};

/// Sealed handle to a SQLite connection that is **inside** a
/// `with_immediate` BEGIN IMMEDIATE transaction.
///
/// The `_seal: ()` private field forbids struct-literal construction
/// from outside `db::tx`; the constructor `new` is module-private
/// (deliberately NOT `pub(crate)`, which would let any in-crate caller
/// fabricate a `WriteTxConn` and bypass the BEGIN IMMEDIATE envelope).
/// Only [`with_immediate`] (production) and [`WriteTxConn::new_for_test`]
/// (test-only) can produce one.
///
/// `Deref` / `DerefMut` to `SqliteConnection` keeps inline
/// `sqlx::query(…).execute(&mut **tx)` ergonomic — one extra deref
/// through the newtype, no rewrites of statement bodies.
pub struct WriteTxConn<'a> {
    inner: &'a mut SqliteConnection,
    _seal: (),
}

impl<'a> WriteTxConn<'a> {
    fn new(inner: &'a mut SqliteConnection) -> Self {
        Self { inner, _seal: () }
    }

    /// Test-only constructor for unit tests of helpers that take
    /// `&mut WriteTxConn<'_>`.  `pub(super)` keeps it visible only
    /// inside `db::tx` (and submodules thereof); other test crates
    /// must drive helpers through `with_immediate` like production.
    #[cfg(test)]
    pub(super) fn new_for_test(inner: &'a mut SqliteConnection) -> Self {
        Self { inner, _seal: () }
    }
}

impl<'a> Deref for WriteTxConn<'a> {
    type Target = SqliteConnection;
    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl<'a> DerefMut for WriteTxConn<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
    }
}

pub async fn with_immediate<R, F>(pool: &SqlitePool, f: F) -> anyhow::Result<R>
where
    F: for<'c> FnOnce(&'c mut WriteTxConn<'c>) -> BoxFuture<'c, anyhow::Result<R>> + Send,
    R: Send,
{
    let mut conn = pool.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    // Explicit inner scope so the `WriteTxConn` borrow of `conn` is
    // released BEFORE we issue COMMIT or ROLLBACK on `conn`.  Without
    // this scope the closure's BoxFuture keeps the borrow alive past
    // the `match`'s control-flow split, and the COMMIT/ROLLBACK calls
    // on `&mut *conn` would either fail to borrow (compile error) or
    // run while a stale `WriteTxConn` reference is still considered
    // live (soundness hazard via DerefMut).
    let result = {
        let mut wt = WriteTxConn::new(&mut conn);
        f(&mut wt).await
    };
    match result {
        Ok(r) => match sqlx::query("COMMIT").execute(&mut *conn).await {
            Ok(_) => Ok(r),
            Err(commit_err) => {
                // COMMIT can fail (e.g. deferred FK / disk error).  Without
                // an explicit ROLLBACK the connection would return to the
                // pool with the transaction still open, poisoning the next
                // acquire.  Best-effort rollback; surface the COMMIT error.
                let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                Err(commit_err.into())
            }
        },
        Err(e) => {
            // Closure failed; best-effort rollback, surface the original.
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    //! W0-2 §9.2 case 5 inside-module half: prove that
    //! `WriteTxConn::new_for_test` IS reachable inside `db::tx`.
    //! The outside-module half (compile-fail when called from a
    //! different module) is the trybuild fixture
    //! `tests/write_tx_conn_compile_fail/new_for_test_outside_db_tx.rs`.

    use super::*;

    #[tokio::test]
    async fn new_for_test_visible_inside_db_tx() {
        let dir = tempfile::tempdir().unwrap();
        let pool = crate::db::open_pool(&dir.path().join("t.db"))
            .await
            .unwrap();
        let mut conn = pool.acquire().await.unwrap();
        // The fact that this line compiles is the proof — `new_for_test`
        // is visible here (inside `db::tx`) but blocked elsewhere by
        // `pub(super) + cfg(test)`.
        // PoolConnection<Sqlite> auto-derefs to SqliteConnection via
        // its DerefMut impl, so `&mut conn` coerces to the
        // `&'a mut SqliteConnection` parameter without an explicit
        // `*conn` (clippy::explicit_auto_deref).
        let _wt = WriteTxConn::new_for_test(&mut conn);
    }
}
