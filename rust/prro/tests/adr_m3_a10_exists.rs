//! Smoke test pinning ADR-M3-A10 existence.
//!
//! ADR-M3-A10 codifies the M3a global-single-writer invariant and the
//! carry-forward obligations for any future multi-worker slice.  It is
//! cross-referenced from `mac_recovery.rs`, `stage_send.rs`,
//! `stage_finalize.rs`, `stage_acquire.rs`, `boot_phase.rs`, and
//! `transport_trace.rs`.  If the ADR file is moved or deleted, those
//! cross-references rot silently.
//!
//! This test makes the deletion loud, but at RUNTIME rather than compile time.
//! The ADR lives in `../../docs/` (a SIBLING of the `rust/` cargo workspace), so a
//! compile-time `include_str!` reaching outside the workspace broke `cargo-mutants
//! -j` — that tool copies only the `rust/` workspace into each parallel build
//! sandbox, the `docs/` sibling is absent, and the baseline build failed, forcing
//! every mutation run to be serial (`--in-place`). Reading at runtime keeps the
//! anti-drift guarantee for the REAL tree (where `docs/` is present: a moved or
//! deleted ADR still fails loudly) while letting the mutants copy build + run
//! (where `docs/` is absent: the check gracefully skips — cargo-mutants is
//! measuring CODE mutants, not this fixture).

use std::path::Path;

/// Path to the surrounding `docs/` tree (a sibling of the `rust/` workspace),
/// resolved from the crate dir at compile time (string only — no file access).
const DOCS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs");
const ADR_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/superpowers/specs/2026-05-12-adr-m3-a10-global-single-writer.md"
);

/// Load the ADR at RUNTIME.
/// - `docs/` absent (a `cargo-mutants -j` copy sandbox that cloned only `rust/`)
///   → `None`, and callers skip: we do not want to fail the mutants baseline build
///   over a fixture that is not being mutated.
/// - `docs/` present but the ADR file missing (a REAL move/delete) → panic loudly:
///   this is exactly the rot the smoke test exists to catch.
fn load_adr_m3_a10() -> Option<String> {
    if !Path::new(DOCS_DIR).exists() {
        eprintln!("adr_m3_a10: `docs/` tree absent (cargo-mutants sandbox?) — skipping");
        return None;
    }
    Some(std::fs::read_to_string(ADR_PATH).unwrap_or_else(|e| {
        panic!(
            "ADR-M3-A10 missing though `docs/` is present ({ADR_PATH}): {e} — \
             global-single-writer cross-references now rot silently"
        )
    }))
}

#[test]
fn adr_m3_a10_file_is_present_and_non_empty() {
    let Some(adr) = load_adr_m3_a10() else { return };
    assert!(
        !adr.is_empty(),
        "ADR-M3-A10 file is empty — global-single-writer contract has no spec anchor"
    );
}

#[test]
fn adr_m3_a10_declares_status_accepted() {
    let Some(adr) = load_adr_m3_a10() else { return };
    assert!(
        adr.contains("**Status:** ACCEPTED"),
        "ADR-M3-A10 must carry `**Status:** ACCEPTED` header — found:\n{}",
        adr.lines().take(10).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn adr_m3_a10_names_carry_forward_obligations() {
    let Some(adr) = load_adr_m3_a10() else { return };
    for required in &[
        "FN-scope exclusion primitive",
        "Lock-leak recovery",
        "Contention metrics",
        "Docstring sweep",
    ] {
        assert!(
            adr.contains(required),
            "ADR-M3-A10 must enumerate carry-forward obligation: {required:?}"
        );
    }
}
