//! RP-R4-1b (registered sqlx-trait break — trybuild compile-FAIL).
//!
//! Spec `docs/superpowers/specs/2026-07-15-cs1r-remediation-spec.md` §1 R4.5
//! RP-R4-1b: a `trybuild` `compile_fail` fixture proving the legacy domain
//! types **no longer** satisfy sqlx's `Type<Sqlite>` / `Encode` / `Decode`.
//! This PINS the *registered* CS-1 source-API break (the enums/ids lost their
//! `sqlx::{Type,Encode,Decode}` impls — the mapping moved store-side onto the
//! `prro::db::types::Db*` wrappers) — it is NOT a restoration.
//!
//! Mirrors the `write_tx_conn_compile_fail.rs` style. Each fixture under
//! `tests/rp_r4_1b_sqlx_break_compile_fail/` must NOT compile; `trybuild`
//! asserts the failure against a captured `.stderr`.
//!
//! Teeth (spec §1, RP-R4-1b row): if a legacy `.bind(DocState::Prepared)` /
//! `needs::<DocState>()` were to compile again (i.e. someone re-added the sqlx
//! impls to the domain type), this compile-fail test flips GREEN→RED (the
//! fixture would then compile, and trybuild reports the missing expected error).

#[test]
fn legacy_types_no_longer_satisfy_sqlx_traits() {
    let t = trybuild::TestCases::new();
    // `T: sqlx::Type<Sqlite>` monomorphised on the legacy enum path — the
    // registered CS-1 break means the bound is unsatisfied (E0277).
    t.compile_fail("tests/rp_r4_1b_sqlx_break_compile_fail/enum_not_sqlx_type.rs");
    // `T: sqlx::Type<Sqlite>` on a legacy UUID-BLOB id — same break.
    t.compile_fail("tests/rp_r4_1b_sqlx_break_compile_fail/blob_id_not_sqlx_type.rs");
    // A legacy `.bind(DocState::…)` via `Encode` — unsatisfied bind bound.
    t.compile_fail("tests/rp_r4_1b_sqlx_break_compile_fail/enum_not_encodable.rs");
}
