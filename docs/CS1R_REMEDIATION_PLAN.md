# CS-1R — Remediation plan (external audit NO-GO → legitimately close CS-1)

**Status: 🔒 PLAN-CONFIRMED (rev 3 — external reviewer confirmed PLAN-READY; rev 3 folds the three final
refinements, all verified against code).** Grounded on `origin/main` `197337d`. **Correct CS-1 range =
`f2c17b1..f2628ba`** (79 modified + 5 added `rust/prro/tests/*.rs`; the 6th added test
`migration_032_delivery_reservation.rs` is CS-2 `9ce76c2`, NOT CS-1).

**Framing.** The external NO-GO is correct. The deficiency is **proof rigor + honest scoping + gate
soundness**, NOT a runtime/storage regression (SQLite bytes, serde, enum literals, UUID-BLOB-16 byte-identical
— independently diff-verified; fiscal behaviour intact). Remediation narrows the claim, adds the missing
gates/pins, and audits the churn — it does **not** roll back the split. **RED-first note:** R2/R3/R1 are
RED-first; **R4 is docs/ADR and carries no RED-pin of its own** (its verification is the reviewed contract diff).

---

## R4 · Re-scope the claim (decide FIRST — frames R1/R3) — BLOCKER-of-claim · docs, no RED-pin

**Problem (grounded).** The contract (`docs/superpowers/specs/2026-07-14-cs1-contract-behaviour-neutral-skeleton.md`)
calls the work "behaviour-neutral", but the moved domain types **lost their public `sqlx::{Type,Encode,Decode}`**
(now on `prro`-local `Db*` newtypes, `prro/src/db/types.rs`); `.bind(DocState::X)` / `T: Type<Sqlite>` no
longer compiles. New public methods appeared (`from_sql_str`, `CashierId::from_persisted_unchecked`).

**Why the break was accepted — a RATIONAL TRADE-OFF, not an impossibility (corrected).** The orphan rule
forbids `impl sqlx::Type for prro_domain::DocState` *from `prro`*, but compatibility was **reachable** by worse
means: (a) keep compatibility shim-types in `prro`; (b) duplicate the types with conversion; (c) enable a
`sqlx` feature *in `prro-domain`* (re-pollutes the pure crate). All are worse than the `Db*` wrapper, so
dropping the source-API is a deliberate trade-off — **that is what must be named honestly**, not undone.

**Decision.**
- **Term → contract rev 5:** replace "behaviour-neutral" with
  **"fiscal-runtime and persisted-representation compatible; deliberate *unsupported* Rust source-API refactor"**.
- **Add `publish = false` to `prro` AND `prro-domain`** — verified ABSENT today
  (`197337d:rust/prro/Cargo.toml` and `…/prro-domain/Cargo.toml` have no `publish` key; only testkit + the 3
  contract crates set it). Then the honest statement is **"no *supported* external Rust API"** (not "no
  external consumers" — that claim was false).
- **Source-API-break register** (in the contract) must list: (1) removed `sqlx::{Type,Encode,Decode}` on the 8
  TEXT enums + UUID-BLOB ids + `CashierId`; (2) **new public surface** `from_sql_str`,
  `CashierId::from_persisted_unchecked`; (3) **changed defining-crate / type identity** (path resolves to a
  `prro_domain` type, not a `prro`-local one) — even where the name is unchanged.
- **Facade both-paths (prior finding, folded):** RP-CS1-3 must compile **both** the nested path
  (`prro::db::models::enums::DocState`) **and** the short path (`prro::db::models::DocState`). Replace the
  `db/models/mod.rs` `pub use enums::*/ids::*` glob with an **explicit legacy export-list**, or add a source
  gate that pins the exact legacy path set. (Ties R4 to the R-facade nit → make it a real closed legacy surface.)

**Alternatives considered.** Re-add legacy impls (worse: (a)/(b)/(c) above). Rejected.

---

## R2 · Gate soundness (with R3, can run parallel after R4) — BLOCKER/MAJOR

**Problem (grounded, `rust/prro-domain/tests/purity_gate.rs` + `rp_cs1_4_contract_dag.rs`).**
- **M1** denylist-not-allowlist (`purity_gate.rs:20-22`).
- **M2** no `--all-features` (`:31-33`).
- **M3** dedup-by-name-not-PackageId (`:96` `HashSet<String>`, `:126-127`) — two versions share a name; the
  safe version prunes the second version's (possibly forbidden) subtree. Same defect in the DAG gate.
- **Honesty:** `ids.rs:34` `Uuid::now_v7()` → clock + `getrandom` (syscall). The crate is not "I/O-free / no
  clock"; and Cargo metadata **cannot** see `std::fs/net/process` at all — so a dep allowlist bounds
  *dependency* capability, not *syscall* capability. Name that limit explicitly.

**Decision — an extended denylist ALONE is insufficient; a closure manifest is required.**
1. **Exact DIRECT allowlist by resolved identity** — `prro-domain`'s direct non-dev deps pinned to exactly
   `{uuid, serde, thiserror}` keyed on **resolved package id / source / dependency-kind / enabled features**,
   and **build-dependencies forbidden** (empty build-dep set). A new direct dep of any name fails.
2. **Pinned TRANSITIVE-closure manifest** (committed; reviewer-SIGNED as sufficient — no separate hand-written
   full allowlist needed) — the manifest fixes each node's **identity / source / version / features / kind /
   target**, is **reviewed wholesale once**, and **any change requires review** (a *vetted* dep gaining a new
   transitive capability trips the gate). **`getrandom` AND `libc` must be present and consciously accepted**
   (via `uuid` v7), documented as deliberate capabilities.
3. **`--all-features --locked`** on the `cargo metadata` invocation (both gate files).
4. **Traverse by PackageId** — dedup `reached` on `dep_id`; match name only for the allow/deny test; resolve
   workspace-root by id. Fix both files.
5. **Update every contradictory text:** `prro-domain/Cargo.toml:5`, `src/lib.rs:5-12`, purity-gate test names +
   assert messages → **"no DB/network/async-runtime/RPC/HTTP adapter dependencies; clock/random permitted only
   for UUID generation"** (not "I/O-free / no clock").
6. **Contract-crates' empty allowlist is PHASED** — marked so it may change *only via the corresponding port
   spec* (ingress/dps/fleet), not silently.

**Honest scope limit (stated, not hidden):** this proves *dependency-graph* purity + a pinned closure, not
absence of `std::{fs,net,process}` syscalls (invisible to Cargo). Syscall-level sandboxing is out of scope for
CS-1R; the closure manifest + allowlist is the enforceable bound.

**RED-pin / teeth (empirical, revert→RED):**
- **RP-R2-a:** add `rusqlite` as a direct dep → direct-allowlist RED; remove → GREEN.
- **RP-R2-b:** add `sqlx = { optional = true }` behind a feature → `--all-features` gate RED (GREEN today).
- **RP-R2-c:** walker refactored to take a metadata-JSON fixture; a two-same-name-versions fixture where only
  the *second* reaches `sqlx` → by-name walker FALSE-PASSES (proves M3), PackageId walker RED.
- **RP-R2-d:** add a `getrandom`-sibling capability crate not in the closure manifest → manifest gate RED
  (proves the transitive-closure pin bites).

This also removes the **Spec #5A ↔ gate inconsistency** (#5A already names rusqlite forbidden + "under all features").

---

## R3 · Observable-equivalence pins (with R2, parallel after R4) — MAJOR

**Problem (grounded).** Decode-error text drift (`types.rs:84-89`: `"unknown … literal in TEXT column: …"` vs
old sqlx `"invalid value … for enum …"`; `rp_cs1_5_db_enum_roundtrip.rs` asserts only `is_err()`).
CashierId warn **tracing target** drift (`types.rs:238-246`: module `prro::db::types` vs old
`prro::db::models::ids`, no explicit `target:`). No `Option<T> ↔ NULL` pin.

**Decision (reviewer-resolved).**
- **Decode message → MATCH-OLD-VERBATIM.** Restore the exact old sqlx-derive decode text (it is one
  format-string — "heavier" does not apply). Rationale: pin-new+ADR would mint another intentional
  runtime-diagnostic exception and make "runtime-compatible" false. So there is **no** allowed diagnostic
  drift.
- **Tracing target → restore** the original `target: "prro::db::models::ids"` explicitly on the warn.
- **`RP-CS1-5` +=** a **table-driven** exact decode-error assertion across **all 8** `Db*` wrappers; the warn
  `target`/message/fields capture; an `Option<Enum> ↔ NULL` round-trip pin.

**RED-pin / teeth.** Change the decode text or the warn target → the exact-string / target assertions go RED.

---

## R1 · RP-CS1-2 provenance + additions-only gate + CI-matrix conformance (final PR) — BLOCKER

**Problem (grounded).** The contract **requires** RP-CS1-2 (`…cs1-contract…:84-91, 127`): a locked SHA
manifest of every test `.rs`, a machine-readable `nextest list … --locked` snapshot, and **additions-only**
enforcement; **plus RP-CS1-2(c) a literal command matrix**. **None is implemented** — no artifact, and the CI
is **package-scoped, not the contract's workspace matrix** (verified: `rust-prro.yml:122` `cargo build -p prro`
not `--workspace`; test legs `:138/:147/:160` selective; `fmt-clippy.yml:47/53/60/66/73` all `-p`-scoped).
**79 modified + 5 added** test files across CS-1, and the diffs include **non-`.bind` transformations**
(verified: `use …DbShiftId`, `.bind(DbShiftId(id))`, `.bind(id.map(DbShiftId))`, `query_as::<Db*>`,
`.map(|w| w.0)`, tuple remaps — 551 added lines match). So "0 assert-line churn" is **necessary, not
sufficient**.

**Decision — a rigorous one-time provenance audit + a forward gate + an explicit CI-matrix resolution.**
- **(a) Per-hunk provenance audit of all 79 modified files** against a **CLOSED transformation whitelist**
  (import add; `.bind(x)` → `.bind(DbX(x))` / `.map(DbX)`; `query_as::<Db*>` + `.map(|w| w.0)` unwrap; pure
  formatting). **Every** non-trivial hunk classified; **anything outside the whitelist is flagged for manual
  ruling.** Separately **prove invariance** of: SQL text, bind ORDER, fixtures/setup, control flow, and
  expected values (the assertions' RHS). Store the artifact with **base/head SHA** + per-hunk classification.
- **(b) Forward additions-only gate — baseline minted at the FINAL CS-1R head (rev-3 refinement).** A static
  `197337d` snapshot would NOT protect the tests R2/R3 themselves add. So: mint the SHA manifest + the
  `nextest list` snapshot at the **final CS-1R head (after R2/R3 land)**; thereafter **every PR must
  synchronously update the manifest, and its diff may ONLY add tests — never delete / rename / `#[ignore]` /
  `cfg`-gate-off**. The retro-snapshots at `f2c17b1` and `f2628ba` stay **separate, immutable provenance
  artifacts** (audit record of the CS-1 churn), distinct from the live forward baseline. **Pin Cargo + nextest
  versions**; normalize the JSON to `{package, target, test_name, ignored}`; compare **≥ the `test-support`
  and the `live-dps`/all-feature compile profiles** (not one profile).
- **(c) Source inventory scope:** **all** `rust/**/tests/**/*.rs` + trybuild fixtures + **all
  `rust/**/src/**/*.rs` that carry `#[test]`/`#[cfg(test)]` modules**. `nextest list --workspace` covers
  *compiled* unit/integration names but **does not replace** the source-file SHA inventory.
- **(d) RP-CS1-2(c) literal command matrix — package-scoped ACCEPTED (reviewer-signed), but coverage is NOT
  complete today (rev-3 refinement).** Amend the contract to a **package-scoped matrix** (not literal
  `--workspace`), but this REQUIRES a proven **package × {build, test, fmt, clippy, features} coverage matrix**:
  verified gaps — **`prro_crypto_v2`, `prro_sidecar`, `maria304_driver`, `prro_escpos_daemon` have NO CI leg at
  all; `prro_escpos` has fmt/clippy but NO build/test leg** (workspace = 12 members; rust-prro.yml legs cover 7:
  prro, prro-domain, the 3 contracts, prro-testkit, prro_crypto). **Add the missing legs, OR give each
  uncovered member an explicit separate gate-reference** — the amended contract must enumerate every
  member × dimension. **Also verify the mutation-diff gate is genuinely merge-required in *branch protection*** —
  `mutation-diff.yml` itself does NOT guarantee it (its own header: "To make it a HARD merge gate, add the
  check to the repo's required status checks (branch protection)"). Treat branch-protection confirmation as a
  checklist item, not an assumption.

**RED-pin / teeth.** Delete / rename / `#[ignore]` an existing test → additions-only gate RED. A non-whitelist
hunk in the audit tool → audit RED (manual ruling required).

**Risk.** `nextest list` name stability across feature profiles; the normalization must be deterministic.

---

## Sequencing
1. **R4** (docs/ADR; no RED-pin) — frames R1/R3 wording; decide first.
2. **R2 ∥ R3** (independent, RED-first; both test/gate + one `target:` line) — parallel.
3. **R1** (final provenance/gate + CI-matrix PR) — depends on nothing but lands last as the closing record.

No hot zone touched (write-path / transports / migrations / reconciliation untouched); prod-code delta is
limited to R3's decode-string + one `tracing::warn!` `target:`.

## Resolved by review — all closed (PLAN-CONFIRMED)
- ✅ R3 decode message = **match-old-verbatim**. ✅ Term = **"fiscal-runtime & persisted-representation
  compatible; deliberate unsupported Rust source-API refactor"**. ✅ Add `publish=false` to prro+prro-domain.
  ✅ R1 range/counts corrected (79M/5A, `f2c17b1..f2628ba`). ✅ R4 has no RED-pin.
- ✅ **R1(d):** package-scoped matrix ACCEPTED (no literal `--workspace`) — but MUST add missing CI legs
  (`prro_crypto_v2` / `prro_sidecar` / `maria304_driver` / `prro_escpos_daemon` / `prro_escpos` test-leg) or
  explicit gate-refs, prove the full member × dimension matrix, and confirm mutation-diff is required in branch
  protection.
- ✅ **R2:** direct-allowlist + pinned transitive-closure manifest (identity/source/version/features/kind/
  target; wholesale-reviewed; change-requires-review; getrandom+libc consciously accepted) — reviewer-SIGNED.
- ✅ **R1(b):** forward baseline minted at the FINAL CS-1R head; per-PR additions-only manifest; retro-snapshots
  immutable + separate.

---
*Grounded/verified: prro/Cargo.toml + prro-domain/Cargo.toml (no publish=false) · f2c17b1..f2628ba = 79M/5A ·
migration_032 = 9ce76c2 (CS-2) · 551 non-bind test transforms · rust-prro.yml:122 + fmt-clippy.yml:47-73
package-scoped · purity_gate.rs:20-33,96,126-127 · types.rs:84-89,238-246 · ids.rs:34 · cs1-contract:84-91,127.*
