//! CS-3 Bridge-0.1 (3.1) — bypass-impossibility canary for the sealed delivery algebra.
//!
//! Correction 2 (Bridge-0.1): the raw_code × doc_type MATRIX correctness is a runtime
//! property test (`rp4b_31_opaque_algebra`); THIS trybuild canary proves the complementary
//! half — the sealed algebra cannot be BYPASSED. `SendOutcome` / `SendIndeterminate` wrap a
//! PRIVATE inner enum and `UnknownStatusCode` has a PRIVATE field, so no external code can
//! hand-construct an illegal outcome (the checkpoint's `UnknownStatus("-1")` /
//! `CloseAmbiguous`+`Sell` states). The sole constructor is `SendOutcome::from_dps_status`.
//!
//! TEETH (proven empirically during Bridge-0.1): make any of the three inner fields `pub`
//! → the fixture compiles → this canary goes RED.

#[test]
fn opaque_algebra_bypass_compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/opaque_algebra_compile_fail/bypass_sealed_algebra.rs");
}
