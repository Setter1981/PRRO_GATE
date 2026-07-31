//! S7-P2-2 (design §8) — compile-fail proofs for the sealed `Authorization` token (P2 Layer 3).
//!
//! The double-issue guarantee P2 ("no wire-capable value survives one `submit_authorized` call")
//! rests on `Authorization` being **non-`Clone`** and consumed by value: a clonable token could be
//! duplicated and fed to `submit_authorized` TWICE → two `send_chk_observed` wires for one document
//! (Layers 1/2 stop a second *authorize*, but only Layer 3 stops a second *submit* of a cloned
//! token). Each fixture under `tests/s7_p2_2_authorization_seal_compile_fail/` MUST NOT compile;
//! `trybuild` drives the per-fixture rustc invocation and asserts the failure. Mirrors the
//! `write_tx_conn_compile_fail.rs` / `reconcile_guard_compile_fail.rs` seal-pin pattern.
//!
//! - `clone_authorization.rs` — `.clone()` on the token (E0599). A `#[derive(Clone)]` regression
//!   reopens the double-submit vector; this pin turns that into a compile error.
//! - `construct_authorization.rs` — struct-literal from an external crate names a private field
//!   (E0451). The ONLY mint path is `authorize_submission`.
//!
//! (The design's "`AttemptObservation` cannot call the wire" clause is trivially structural — the
//! type carries no channel and the `s7_p2_2_sole_seam.rs` scan already pins `send_chk_observed` to
//! its single site inside `submit_authorized` — so no dedicated fixture is added, per the
//! anti-duplication bias.)

#[test]
fn s7_p2_2_authorization_seal_compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/s7_p2_2_authorization_seal_compile_fail/clone_authorization.rs");
    t.compile_fail("tests/s7_p2_2_authorization_seal_compile_fail/construct_authorization.rs");
}
