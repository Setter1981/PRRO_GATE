//! RP-CS1-1 fast canary (trybuild), twinned with the PRIMARY cargo-metadata
//! purity gate in `purity_gate.rs`.
//!
//! Same shape as `prro/tests/write_tx_conn_compile_fail.rs`: a `trybuild`
//! `compile_fail` fixture proves a forbidden crate (contract §9) cannot be
//! reached from `prro-domain`. `forbidden_sqlx_extern.rs` does
//! `extern crate sqlx;` — rustc rejects it (E0463) because `sqlx` is not a
//! dependency of `prro-domain`. If `sqlx` were added, the fixture would compile
//! and THIS test would fail. The metadata gate remains the authoritative check
//! (it also catches alias / build-dep / target-dep evasion a `use`-site misses).

#[test]
fn prro_domain_forbidden_crate_compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/purity_compile_fail/forbidden_sqlx_extern.rs");
}
