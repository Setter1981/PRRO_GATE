//! RP-CS1-1 canary fixture — MUST NOT COMPILE.
//!
//! `sqlx` is a forbidden crate (contract §9): it is not, and must never be, a
//! dependency of `prro-domain`. This fixture tries to reach it via
//! `extern crate` and rustc rejects it (E0463: can't find crate for `sqlx`).
//! If someone adds `sqlx` to `prro-domain/Cargo.toml`, this fixture would start
//! to compile and the driver test (`purity_gate_compile_fail`) would FAIL — the
//! fast twin of the cargo-metadata purity gate.

extern crate sqlx;

fn main() {}
