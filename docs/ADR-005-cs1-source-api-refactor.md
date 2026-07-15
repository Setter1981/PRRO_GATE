# ADR-005: CS-1 Source-API Refactor — deliberate unsupported Rust API break, no supported external API

**Status:** Accepted (CS-1R remediation, rev 5)
**Date:** 2026-07-15
**Authors:** CS-1R remediation (architect + implementer, dual-session)
**Supersedes:** the *global* "behaviour-neutral" label on CS-1
(`docs/superpowers/specs/2026-07-14-cs1-contract-behaviour-neutral-skeleton.md`, now rev 5)
**Related:** CS-1R spec `docs/superpowers/specs/2026-07-15-cs1r-remediation-spec.md` §1 (R4);
`docs/CS1R_REMEDIATION_PLAN.md` §R4

---

## Context

CS-1 (crate-split skeleton, range `f2c17b1..f2628ba`) moved the pure domain model — the 8 TEXT-affinity
state/protocol enums (`DocState`, `DocType`, `FiscalMode`, `NodeMode`, `OfflineSessionState`,
`Protocol`, `Severity`, `ShiftState`), the 6 UUID-BLOB ids (`DocumentId`, `RequestId`, `ShiftId`,
`OperatorId`, `PrinterId`, `OfflineSessionId`), and `CashierId` / `DriverId` — out of `prro` into the
sqlx-free `prro-domain` crate, behind an explicit compatibility facade at `prro::db::models`.

The relocation is a **storage / serde-byte non-event**: SQLite affinity (TEXT / 16-byte BLOB), the
UPPER_SNAKE (and lowercase `test`/`prod`) TEXT literals, and `#[serde(rename)]` output are all
byte-identical (independently diff-verified; RP-CS1-5). The fiscal runtime is unchanged.

**But the Rust *source* API is not neutral.** The orphan rule forbids `impl sqlx::Type for
prro_domain::DocState` from `prro`, so the sqlx `Type`/`Encode`/`Decode` impls were moved off the
domain types onto `prro`-local `prro::db::types::Db*` store-side wrappers. Consequently:

- `.bind(DocState::Prepared)`, a `T: sqlx::Type<Sqlite>` / `T: sqlx::Encode<'q, Sqlite>` bound
  monomorphised on any legacy path, no longer compiles (E0277).
- New public methods appeared: `from_sql_str(&str) -> Option<Self>` (8 enums),
  `CashierId::from_persisted_unchecked(String) -> Self`.
- Every legacy path now resolves to a `prro_domain` type — the defining crate (hence the trait-impl
  surface reachable under the orphan rule) changed, even where the name did not.

Labelling this "behaviour-neutral" globally over-claimed. The external audit correctly flagged it.

## Decision

1. **Name the break honestly.** The claim is re-scoped to **"fiscal-runtime and
   persisted-representation compatible; a deliberate *unsupported* Rust source-API refactor."**
   "Behaviour-neutral" is retained only where it strictly means storage/serde bytes. The contract
   (rev 5) carries a normative **Source-API-break register** (§11): removed impls with exact trait
   signatures (`sqlx::Type<Sqlite>`, `sqlx::Encode<'q, Sqlite>`, `sqlx::Decode<'r, Sqlite>`), the new
   public surface, and the changed defining-crate / type identity.

2. **Accept the break as a rational trade-off, not undo it.** Source-API compatibility *was*
   reachable, by strictly worse means, all rejected:
   - **(a)** `prro`-local compat/shim types re-exposing sqlx — re-introduces the coupling CS-1 exists
     to sever;
   - **(b)** duplicate-with-conversion (a second sqlx-bearing copy + `From`/`Into`) — two sources of
     truth per type;
   - **(c)** a `sqlx` feature *in* `prro-domain` — re-pollutes the pure crate, defeats the RP-CS1-1
     purity gate.
   The `Db*` store-side wrapper (contract §4) is better than all three.

3. **Declare "no *supported* external Rust API."** `prro` and `prro-domain` gain `publish = false`
   (neither had it in the baseline). The honest statement is "no *supported* external Rust API" — not
   "no external consumers" (which would be false). The crates are workspace-internal; the source-API
   refactor is unsupported for outside consumers.

4. **Close the facade as an explicit, pinned surface.** The `prro::db::models::mod.rs`
   `pub use enums::*; pub use ids::*;` globs are replaced by an explicit per-symbol legacy
   export-list. Three RED-first pins guard it (CS-1R §1 R4.5): **RP-R4-1a** (per-type retained-surface
   compile-manifest, both nested and short paths, non-uniform surface), **RP-R4-1b** (`trybuild`
   compile-FAIL proving the legacy types no longer satisfy sqlx — pins the break, not a restoration),
   and **RP-R4-1c** (a `syn` AST parse asserting no glob + exact set-equality of the `pub use` set to
   the pinned legacy list — the only guard that catches *widening*).

## Consequences

- **Positive.** The purity boundary is real and enforced; the claim now matches the code, so CS-1 can
  be legitimately closed. The facade surface is pinned and cannot silently widen. External-consumer
  expectations are correctly set to "unsupported".
- **Cost.** Internal callers bind through `Db*` wrappers (`.bind(DbDocState(DocState::Prepared))`)
  instead of the raw enum — a known, contained ergonomics cost, isolated to `prro::db`.
- **Scope limit.** This ADR governs the *Rust source API* and the persisted-byte compatibility claim.
  It does not address syscall-level capability (invisible to Cargo) or the later `prro-store-sqlite`
  extraction (CS-7), which will move `db::types` wholesale.
