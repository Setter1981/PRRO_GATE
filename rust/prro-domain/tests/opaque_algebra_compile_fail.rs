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

/// Bridge-0.1 (3.1b) — doc_type single-source canary. `classify` takes ONLY the sealed
/// evidence; the `-2/-15` close split is consumed once, at `from_dps_status`. Passing a
/// second, independent `doc_type` to `classify` (the re-binding vector: a Sell-built
/// outcome re-classified as ZReport) is an arity error, so it cannot be written.
///
/// TEETH: re-add a `doc_type` parameter to `classify` → the fixture compiles → canary RED.
#[test]
fn doc_type_cannot_be_rebound_at_classify_compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/opaque_algebra_compile_fail/rebind_doc_type.rs");
}
