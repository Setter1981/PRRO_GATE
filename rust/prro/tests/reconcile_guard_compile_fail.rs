//! M3b W2 — compile-fail proof for the `ReconcileGuard` enforcement.
//!
//! Each fixture under `tests/reconcile_guard_compile_fail/` is a small
//! Rust file that **must NOT compile** if the W2 token enforcement is
//! correctly applied to `boot_phase::run_boot_reconciliation`.
//! `trybuild` drives the per-fixture rustc invocations and asserts
//! the failure.
//!
//! Mirrors the `write_tx_conn_compile_fail.rs` pattern from W0-2
//! §9.2 (ADR-M3-A4 / PRRO_GATE-k99 structural seal).

#[test]
fn reconcile_guard_enforcement_compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/reconcile_guard_compile_fail/missing_guard.rs");
}
