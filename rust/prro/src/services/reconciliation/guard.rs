//! M3b W2 — module-level enforcement of ADR-M3-A10 global-single-writer
//! invariant via a lock-token type.
//!
//! ## Why a token
//!
//! M3a hardening pass 2 added `App::reconcile_pending_inner`'s
//! `tokio::sync::Mutex<()>` ("recon mutex") to structurally enforce
//! ADR-M3-A10.  But the mutex only protects callers that go through
//! `App::reconcile_pending` / `reconcile_pending_with`.  A direct call
//! into `boot_phase::run_boot_reconciliation` from elsewhere (e.g. a
//! test, ops script, or future scheduler that bypasses `App`) silently
//! skips the mutex — and any such bypass breaks the W0b scoped-YES
//! verdict's hard precondition for W9/W12 correctness ("no same-FN send
//! may interleave between `stage_send(doc_i)` and `lastChk(fn_sign)`",
//! see `docs/superpowers/specs/2026-05-14-m3b-w0b-w12-gate-decision.md`).
//!
//! W2 closes the bypass by making `run_boot_reconciliation` require a
//! `&ReconcileGuard<'_>` as its first parameter.  The token is
//! non-`Clone` and (by default `!Send` on its production variant; see
//! below) cannot be smuggled out of the call site that minted it.
//!
//! ## Constructors
//!
//! - `ReconcileGuard::from_app_mutex(MutexGuard<'a, ()>)` is the
//!   **only** production constructor.  Crate-private visibility means
//!   only `App::reconcile_pending_inner` (the App-mutex holder) can
//!   mint a production token.  The token's lifetime is bound to the
//!   `MutexGuard`, so dropping the token releases the mutex.
//!
//! - `ReconcileGuard::for_integration_test_only()` is the test seam.
//!   `#[doc(hidden)]` + the explicit `_for_integration_test_only`
//!   naming + this doc-comment are the signal that production callers
//!   MUST NOT use it.  It exists because the M3a-era integration
//!   tests in `tests/app_boot_reconciliation.rs` +
//!   `tests/boot_phase_w9_helpers.rs` exercise specific dispatch
//!   branches by calling `run_boot_reconciliation` directly; refactoring
//!   each of those tests to go through `App::reconcile_pending_with`
//!   would require building a full `App` (config + boot pipeline +
//!   quick_check + migrations) and matching against the aggregated
//!   `ReconciliationSummary` rather than per-FN `BranchOutcome` —
//!   well beyond the < 0.5 day boundary in plan §Task 2 OQ2.
//!
//! Both constructors return a `ReconcileGuard` of the same nominal
//! type; the only behavioural difference is the lock-holding inner.
//! `run_boot_reconciliation` does not look at the inner — the token's
//! presence in the call signature is what enforces discipline.

/// Lock-token proving the caller holds the App-scoped reconcile mutex.
///
/// Required parameter for
/// [`super::boot_phase::run_boot_reconciliation`].  Non-`Clone` by
/// construction; the production variant ties its lifetime to a
/// [`tokio::sync::MutexGuard`] so the mutex is released when the
/// token drops.
///
/// See module-level docs for the constructor contract.
pub struct ReconcileGuard<'a> {
    _inner: ReconcileGuardKind<'a>,
}

enum ReconcileGuardKind<'a> {
    /// Production variant — wraps the App's `MutexGuard<'_, ()>`.
    /// Crate-private; only `App::reconcile_pending_inner` mints this.
    /// The `MutexGuard` is held for RAII drop semantics (mutex
    /// released when the token drops) — Rust's lint doesn't see it
    /// as "read", but its presence is the whole point.
    Production(#[allow(dead_code)] tokio::sync::MutexGuard<'a, ()>),
    /// Test-only variant — no lock held.  Constructed via
    /// `ReconcileGuard::for_integration_test_only()`, which is
    /// `#[doc(hidden)] pub` and named explicitly to discourage
    /// production use.
    TestOnly,
}

impl<'a> ReconcileGuard<'a> {
    /// Production constructor.  Crate-private; only callable by
    /// `App::reconcile_pending_inner` after acquiring the App recon
    /// mutex.  Token's lifetime ties to the `MutexGuard`, so dropping
    /// the token releases the mutex.
    pub(crate) fn from_app_mutex(guard: tokio::sync::MutexGuard<'a, ()>) -> Self {
        Self {
            _inner: ReconcileGuardKind::Production(guard),
        }
    }
}

impl ReconcileGuard<'static> {
    /// **TEST SEAM — DO NOT CALL FROM PRODUCTION CODE.**
    ///
    /// Constructs a `ReconcileGuard<'static>` that holds no lock.
    /// Exists exclusively for integration tests in `tests/` that
    /// exercise `run_boot_reconciliation` for white-box dispatch
    /// fixtures.  Production callers MUST go through
    /// `App::reconcile_pending_with`, which mints the token via
    /// [`Self::from_app_mutex`] inside the App mutex critical section.
    ///
    /// Naming is intentionally awkward (`_for_integration_test_only`)
    /// + `#[doc(hidden)]` to discourage IDE autocomplete from
    /// surfacing this in production callers.
    #[doc(hidden)]
    pub fn for_integration_test_only() -> Self {
        Self {
            _inner: ReconcileGuardKind::TestOnly,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sanity: the test-seam constructor compiles + produces a token
    /// usable as a `&ReconcileGuard<'_>` borrow target.
    #[test]
    fn for_integration_test_only_compiles_and_borrows() {
        let token = ReconcileGuard::for_integration_test_only();
        let _: &ReconcileGuard<'_> = &token;
    }

    /// Sanity: `from_app_mutex` is crate-visible (this test lives
    /// inside the crate, so it can call the `pub(crate)` constructor).
    #[tokio::test]
    async fn from_app_mutex_compiles_inside_crate() {
        let mutex: tokio::sync::Mutex<()> = tokio::sync::Mutex::new(());
        let guard = mutex.lock().await;
        let _token = ReconcileGuard::from_app_mutex(guard);
    }
}
