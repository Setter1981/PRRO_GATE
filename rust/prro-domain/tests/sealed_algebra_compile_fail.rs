//! CS-3 Bridge-0 R2 sealing canary (trybuild) — the illegal-state teeth.
//!
//! Same shape as `purity_gate_compile_fail.rs`: a `trybuild` `compile_fail` fixture
//! proves that the sealed delivery algebra (`SentAccepted`, `Kvt1Raw`,
//! `ClassifiedOutcome`, `ObservedOutcomeV1`) CANNOT be constructed with illegal states
//! from outside `prro-domain`, because their fields are private and the only
//! constructors (`observe`, `new`, `classify`, `record`) validate their invariants.
//!
//! The checkpoint external audit's finding was "types not sealed — illegal states
//! constructible". This canary is the direct teeth: if any field were made `pub`, the
//! fixture would compile and this test would FAIL. Verified empirically by reverting a
//! seal → the fixture compiles → this test goes RED.

#[test]
fn sealed_delivery_types_cannot_be_constructed_externally() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/sealed_compile_fail/construct_sealed_types.rs");
}
