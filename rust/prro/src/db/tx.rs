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

tokio::task_local! {
    /// Marker scope active while a `with_immediate` BEGIN IMMEDIATE
    /// closure body is being polled.  Per ADR-M3-A3 / W0-2 §3.6
    /// (M3a W3 hybrid enforcement), every M2 substrate public-API
    /// entry in `prro::crypto::*` and `prro::transports::dps::*`
    /// `debug_assert!` that this scope is NOT active — calling
    /// substrate from inside the BEGIN IMMEDIATE envelope violates
    /// PRRO invariant #1 ("no network or crypto inside long SQLite
    /// write transactions").
    ///
    /// `tokio::task_local!` (NOT `thread_local!`) because tokio's
    /// multi-threaded runtime migrates polled futures across worker
    /// threads after `.await`; per-task storage stays visible across
    /// the migration whereas a `thread_local!` set on thread T1 is
    /// invisible on T2 where the future resumes.
    ///
    /// What this does NOT cover, and how the W3 static scan
    /// complements it: `tokio::task::spawn_blocking(closure)` runs
    /// `closure` on a blocking-pool thread without async polling
    /// context.  Tokio does not propagate task-local table into the
    /// blocking-pool thread; the closure body runs as if the scope
    /// were never entered.  The static scan in
    /// `tests/with_immediate_no_foreign_io.rs` therefore catches
    /// literal `spawn_blocking` / `block_in_place` call expressions
    /// inside `with_immediate` closure bodies — the runtime guard
    /// here cannot.
    pub(crate) static IN_WITH_IMMEDIATE: ();
}

/// Runtime guard helper for M2 substrate public-API entry points.
/// Substrate methods (`crypto::*`, `transports::dps::*`) call this as
/// their first body line; in debug / test builds it `debug_assert!`s
/// that we are NOT inside a `with_immediate` BEGIN IMMEDIATE scope.
/// In release builds the assert compiles to a no-op.
///
/// The `method` parameter names the offending entry in the panic
/// message so reviewers reading a CI failure see exactly which
/// substrate call leaked into a transaction without grepping the
/// stack frame addresses.
#[inline]
pub(crate) fn assert_not_in_with_immediate(method: &'static str) {
    debug_assert!(
        IN_WITH_IMMEDIATE.try_with(|_| ()).is_err(),
        "foreign IO inside with_immediate: {method}"
    );
}

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
    // Wrap the closure body in `IN_WITH_IMMEDIATE.scope(...)` so the
    // M3a W3 hybrid runtime guard (per ADR-M3-A3 / W0-2 §3.6) sees the
    // marker scope active throughout the closure's polling.  Substrate
    // public-API methods called from inside the closure (directly or
    // via helper-of-helper indirection) `debug_assert!` against this
    // scope at their entry and panic in debug / test builds.
    //
    // The `WriteTxConn::new(&mut conn)` lives inside the scope so the
    // borrow ends at the scope close (before the COMMIT/ROLLBACK
    // statements below) — same lifetime hygiene as the W2 inner-block
    // shape it replaces.
    let result = IN_WITH_IMMEDIATE
        .scope((), async {
            let mut wt = WriteTxConn::new(&mut conn);
            f(&mut wt).await
        })
        .await;
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
