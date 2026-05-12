//! Smoke test pinning ADR-M3-A10 existence.
//!
//! ADR-M3-A10 codifies the M3a global-single-writer invariant and the
//! carry-forward obligations for any future multi-worker slice.  It is
//! cross-referenced from `mac_recovery.rs`, `stage_send.rs`,
//! `stage_finalize.rs`, `stage_acquire.rs`, `boot_phase.rs`, and
//! `transport_trace.rs`.  If the ADR file is moved or deleted, those
//! cross-references rot silently.
//!
//! This test makes the deletion loud: `include_str!` is resolved at
//! compile time, so removing or renaming the ADR file fails the build.

const ADR_M3_A10: &str =
    include_str!("../../../docs/superpowers/specs/2026-05-12-adr-m3-a10-global-single-writer.md");

#[test]
fn adr_m3_a10_file_is_present_and_non_empty() {
    assert!(
        !ADR_M3_A10.is_empty(),
        "ADR-M3-A10 file is empty — global-single-writer contract has no spec anchor"
    );
}

#[test]
fn adr_m3_a10_declares_status_accepted() {
    assert!(
        ADR_M3_A10.contains("**Status:** ACCEPTED"),
        "ADR-M3-A10 must carry `**Status:** ACCEPTED` header — found:\n{}",
        ADR_M3_A10.lines().take(10).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn adr_m3_a10_names_carry_forward_obligations() {
    for required in &[
        "FN-scope exclusion primitive",
        "Lock-leak recovery",
        "Contention metrics",
        "Docstring sweep",
    ] {
        assert!(
            ADR_M3_A10.contains(required),
            "ADR-M3-A10 must enumerate carry-forward obligation: {required:?}"
        );
    }
}
