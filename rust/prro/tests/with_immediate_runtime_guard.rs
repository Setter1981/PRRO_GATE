//! W0-2 §9.1 cases 2 + 4 + 5 — runtime-guard tests for the
//! `IN_WITH_IMMEDIATE` task-local scope.
//!
//! - Case 2: indirect substrate call via helper-of-helper.  Static
//!   scan misses this (closure body literal is the helper, not a
//!   denylisted method); runtime guard catches it AT THE PUBLIC API
//!   ENTRY of the substrate method itself.
//! - Case 4: provider public-API entry positive control.  Direct
//!   substrate call inside a `with_immediate` closure panics with
//!   the substrate method's name in the message — proves the
//!   `debug_assert!` is the FIRST statement and fires before any
//!   internal dispatch.
//! - Case 5: substrate call OUTSIDE `with_immediate`.  Negative
//!   control — must NOT panic; the call returns a regular
//!   `CryptoError` (here `urls.is_empty()` -> `CertFetch`).
//!
//! `fetch_cert_by_ski(&[], ...)` is the runtime probe of choice:
//! - Inside `with_immediate`: the assert at line 1 of the method
//!   panics BEFORE reaching the `urls.is_empty()` early return.
//! - Outside `with_immediate`: the assert is a no-op, and the
//!   empty-urls branch returns `Err(CryptoError::CertFetch)` quickly
//!   without any network call.
//!
//! No provider-spy / `spawn_blocking_count` instrumentation is used:
//! the structural proof "guard is the first statement" is what makes
//! cases 2 and 4 deterministic.  The panic message naming the
//! substrate method is the assertion target.

use std::time::Duration;

use prro::crypto::{CryptoError, CryptoProvider, InProcessProvider};
use prro::db::{open_pool, tx::with_immediate};

async fn fresh_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("guard.db");
    std::mem::forget(dir);
    open_pool(&path).await.unwrap()
}

fn provider() -> InProcessProvider {
    // The runtime guard fires before any provider state is touched,
    // so the cheapest constructible provider works for the panic-
    // assertion tests.  `InProcessProvider` is a unit struct
    // (`pub struct InProcessProvider;`) — direct construction;
    // `::default()` would trip `clippy::default_constructed_unit_structs`.
    InProcessProvider
}

/// Helper used by case 2 to introduce an indirection layer between
/// the `with_immediate` closure body (which the static scan walks)
/// and the substrate call (which the runtime guard catches).  The
/// scanner sees `local_helper(provider).await` in the closure body —
/// `local_helper` is not on the denylist — and lets it through.  At
/// runtime, `local_helper` calls `fetch_cert_by_ski`, whose first
/// statement asserts against `IN_WITH_IMMEDIATE`.
async fn local_helper(p: &InProcessProvider) -> Result<(), CryptoError> {
    let _ = p
        .fetch_cert_by_ski(&[], &[0u8; 32], Duration::from_millis(100))
        .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[should_panic(expected = "foreign IO inside with_immediate: fetch_cert_by_ski")]
async fn case_2_indirect_substrate_call_via_helper_panics() {
    // Case 2 from W0-2 §9.1.  Static scan would miss this (closure
    // body literal is `local_helper(p).await`); the runtime guard at
    // the entry of `fetch_cert_by_ski` catches it.
    let pool = fresh_pool().await;
    let p = provider();

    let _ = with_immediate(&pool, move |_tx| {
        Box::pin(async move {
            local_helper(&p).await.ok();
            Ok::<_, anyhow::Error>(())
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[should_panic(expected = "foreign IO inside with_immediate: fetch_cert_by_ski")]
async fn case_4_direct_substrate_entry_positive_control() {
    // Case 4 from W0-2 §9.1.  Direct `fetch_cert_by_ski(...)` inside
    // the closure body — the static scan would also catch this at
    // compile time, but the runtime test proves the
    // `debug_assert!` is wired up correctly and that the panic
    // message is informative.
    let pool = fresh_pool().await;
    let p = provider();

    let _ = with_immediate(&pool, move |_tx| {
        Box::pin(async move {
            let _ = p
                .fetch_cert_by_ski(&[], &[0u8; 32], Duration::from_millis(100))
                .await;
            Ok::<_, anyhow::Error>(())
        })
    })
    .await;
}

#[tokio::test]
async fn case_5_substrate_call_outside_with_immediate_does_not_panic() {
    // Case 5 from W0-2 §9.1.  The same call shape as cases 2/4 but
    // OUTSIDE any `with_immediate(...)` envelope.  The task-local
    // is not in scope; `IN_WITH_IMMEDIATE.try_with(...)` returns
    // Err; `debug_assert!(... .is_err(), ...)` is a no-op; the call
    // proceeds to the `urls.is_empty()` branch and returns a
    // regular `CryptoError`.
    let p = provider();
    let result = p
        .fetch_cert_by_ski(&[], &[0u8; 32], Duration::from_millis(100))
        .await;
    assert!(
        matches!(result, Err(CryptoError::CertFetch { .. })),
        "outside with_immediate, fetch_cert_by_ski(&[], ...) must return \
         CryptoError::CertFetch; got {result:?}"
    );
}
