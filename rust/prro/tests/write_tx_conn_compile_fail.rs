//! W0-2 §9.2 — compile-fail proofs for the `WriteTxConn<'_>` seal.
//!
//! Each fixture under `tests/write_tx_conn_compile_fail/` is a small
//! Rust file that **must NOT compile** if the seal is correctly
//! applied.  `trybuild` drives the per-fixture rustc invocations and
//! asserts the failure.  These tests are the structural proof that
//! ADR-M3-A4 / W0-2 §4.4 PRRO_GATE-k99 closure holds:
//!
//! 1. `raw_sqlite_connection.rs` — caller passes `&mut SqliteConnection`
//!    to `transition_state`; rustc rejects (E0308: mismatched types).
//! 2. `new_outside_db_tx.rs` — caller calls `WriteTxConn::new` from
//!    outside `db::tx`; rustc rejects (E0603: function `new` is
//!    private).
//! 3. `struct_literal_seal_field.rs` — caller fabricates a
//!    `WriteTxConn` via struct-literal syntax, naming the private
//!    `_seal: ()` field; rustc rejects (E0451: field is private).
//! 4. `new_for_test_outside_db_tx.rs` — caller calls
//!    `WriteTxConn::new_for_test` from a non-`db::tx` module
//!    (the `pub(super)` + `cfg(test)` gating means the symbol is
//!    not visible to other crates' integration tests).
//!
//! Case 4 from §9.2 ("valid usage compiles + runs") is implicitly
//! covered by every other test in the suite that drives
//! `transition_state` through a `with_immediate` envelope; it does
//! not need a dedicated trybuild fixture.  Case 5's inside-module
//! half is `db::tx::tests::new_for_test_visible_inside_db_tx` in
//! `src/db/tx.rs`.

#[test]
fn write_tx_conn_seal_compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/write_tx_conn_compile_fail/raw_sqlite_connection.rs");
    t.compile_fail("tests/write_tx_conn_compile_fail/new_outside_db_tx.rs");
    t.compile_fail("tests/write_tx_conn_compile_fail/struct_literal_seal_field.rs");
    t.compile_fail("tests/write_tx_conn_compile_fail/new_for_test_outside_db_tx.rs");
}
