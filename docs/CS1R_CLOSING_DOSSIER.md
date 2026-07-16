# CS-1R closing dossier — verification-of-remediation (GO-gate)

**Purpose.** The external auditor issued a **NO-GO** on CS-1 ("not provably behaviour-neutral"). CS-1R has now
landed. This is a **verification-of-remediation checklist** (not a re-audit): each original NO-GO finding →
the merged fix → the teeth evidence (each **empirically re-verified by the architect**, revert guard → named
test RED). The ask: does this close the NO-GO → **CS-1 GO**?

**Merged on `origin/main` (all squash-merged, required checks green):**
| slice | PR | SHA | scope |
|---|---|---|---|
| R4 | #300 | `6fb5f6a` | honest re-scope + facade explicit-list + registered-break pins |
| R2 | #301 | `5394c21` | gate soundness (allowlist + node/edge manifest + PackageId + --all-features) |
| R3 | #302 | `207d1f8` | observable-equivalence pins (match-old decode golden + warn target) |
| oracle | #303 | `32166cc` | (bonus) the moat caught + we fixed an oracle bug found mid-remediation |
| R1 | #304 | `60bc305` | RP-CS1-2 (provenance audit + forward inventory gate) + CI matrix |

All test/gate/docs/CI only — **no hot zone touched** (write_path / reconciliation / transports / adapters /
repositories / migrations untouched); storage/serde bytes byte-identical. **CS-1R2 A4 narrowing:** the
_runtime SQL_ is NOT byte-identical — 3 test-side runtime `query_scalar` column-aliases were cleaned
(`col as "alias: Type"` → `col`), catalogued in `RUNTIME_SQL_DELTAS`; the fiscal result + persisted
representation are unchanged (the aliases only named a read's output column).

---

## §1 · NO-GO findings → fixes → teeth (architect-verified)

| # | Original NO-GO finding | Severity | Fix (PR) | Teeth — architect-verified empirically |
|---|---|---|---|---|
| 1 | **§7.10 source-API break** — legacy types lost public `sqlx::{Type,Encode,Decode}`; term "behaviour-neutral" overclaimed | BLOCKER | **R4**: contract rev 5 term → "fiscal-runtime & persisted-representation compatible; deliberate *unsupported* Rust source-API refactor" + Source-API-break register (correct trait sigs + new methods + type-identity) + `publish=false` on prro+prro-domain + ADR-005 | **RP-R4-1b** trybuild proves legacy types no longer satisfy sqlx (the registered break); **RP-R4-1a** per-type surface: removal→E0204, **addition→E0283** (static_assertions, both dirs); ✅ verified |
| 2 | **RP-CS1-2 never implemented** — contract-required test-inventory additions-only gate absent; 79 modified test files unaudited | BLOCKER | **R1**: syn per-hunk provenance audit (**78 pure-whitelist + 1 manual-ruling + 0 drift**) + forward three-way inventory gate (live==manifest / additions-only / new-test-present) minted at final head + source SHA manifest | **RP-R1-2** mutate a SQL/assertion literal in a CS-1 test → provenance live-drift RED ✅; **RP-R1-1** `#[ignore]` a test → inventory gate **real exit=1** (confirmed non-zero, not just a printed ❌) ✅ verified |
| 3 | **M1 denylist-not-allowlist** — 7-name FORBIDDEN denylist ≠ purity; `rusqlite` etc. slip through | BLOCKER | **R2**: direct-dependency **allowlist** `{uuid,serde,thiserror}` (name-agnostic) + empty build-deps | **RP-R2-a** add a forbidden direct dep → allowlist RED; **RP-R2-e** build-dep → RED |
| 4 | **M2 no `--all-features`** — optional feature-gated forbidden dep invisible | BLOCKER | **R2**: `cargo metadata --all-features --locked` + **permanent arg-pin** | **RP-R2-b2** delete `--all-features` from the pinned arg vector → arg-pin RED ✅ verified |
| 5 | **M3 walker dedup-by-name (the walker bug)** — two same-name versions; the dangerous 2nd pruned | MAJOR | **R2**: shared production walker dedups on **PackageId**; root by `workspace_members` id; + **node+edge** closure manifest (kind/target on the edge) | **RP-R2-c** mutate the production walker PackageId→name → it misses `tokio` off the 2nd sqlx version → RED ✅ verified; **RP-R2-d** perturb any manifest node/edge axis → set-equality RED ✅ verified |
| 6 | **§7.5 decode-message drift** — `unknown … literal` ≠ old sqlx text; RP-CS1-5 only `is_err()` | MAJOR | **R3**: `DbX::decode` restored to **match-old-verbatim** (`invalid value {v:?} for enum {E}`) frozen via a golden captured empirically at `f2c17b1` (inner `ColumnDecode.source`) | **RP-R3-1** mutate the decode string → golden-match RED (×8) ✅ verified |
| 7 | **§7.5 tracing-target drift** — `CashierId` warn target moved to `prro::db::types` | MAJOR | **R3**: explicit `target: "prro::db::models::ids"` restored | **RP-R3-2** change `target:` → RED |
| 8 | **§7.4 facade completeness / "never `pub use *`" leaf-only** | MAJOR/MINOR | **R4**: `db/models/mod.rs` globs → **explicit per-symbol legacy export-list** | **RP-R4-1c** restore a glob OR add an extra export → AST set-equality RED ✅ verified |
| 9 | **"no clock / I/O-free" purity claim false** (`uuid` v7 → getrandom) | (honesty) | **R2**: texts → "no DB/network/async-runtime/RPC/HTTP adapter deps; getrandom 0.4.2 = OS entropy for UUID v7, clock via std::time"; getrandom+libc annotated accepted in the closure manifest | — (documented) |
| — | **Spec #5A ↔ gate inconsistency** (#5A names rusqlite forbidden + "under all features") | — | Closed by R2 (the CS-1 gate now does both) | — |

---

## §2 · Honest carryovers (full transparency — flagged, not hidden)

1. ~~**Clippy for 3 crates = DOCUMENTED EXCLUSION**~~ **RESOLVED (#305, `0744203`).** The last three CS-1R R1.3-excluded crates now carry REQUIRED clippy legs in `fmt-clippy.yml`: `prro_crypto_v2` (pre-existing findings GRANDFATHERED via a crate-root `#![allow(...)]` — attributes-only, zero behaviour change; a per-finding CT-aware cleanup is the sole remaining lint-debt backlog item), `maria304_driver` (fixed: scoped `#[allow(too_many_lines)]` + fmt), `prro_escpos_daemon` (fixed cleanly). Every member×dimension clippy cell is now filled — no silent clippy blank.
2. ~~**`mutation diff-gate` is NOT yet required**~~ **RESOLVED — the mutation diff-gate is now a REQUIRED check** (`mutation-diff.yml`; the unfiltered-trigger + in-workflow `changes` detector make it report on every PR, and the branch-protection required flag has been set by the operator). New survivors vs `docs/mutation/baseline/survivors.txt` fail the PR.
3. **R2 closure manifest is verbose (~64 nodes)** because `--all-features` pulls `getrandom`'s WASI/`wit-bindgen` backends (never compiled on the target) → unrelated wasm-backend version bumps will RED the gate; `cargo xtask update-purity-closure` is the intended re-mint path.
4. **RP-R2-a canary uses `sqlx` (not the spec's `rusqlite`)** — `rusqlite` cannot resolve (collides with the workspace's `libsqlite3-sys` `links="sqlite3"`, present 4×). Assertion identical (allowlist is name-set membership).
5. **RP-R3-3 (Option↔NULL) NULL-arm** delegates to sqlx's blanket `Option` impl → it is a regression sentinel, not a revert-prod-→RED pin; the `Some(v)` encode-content arm bites hard.
6. **1 provenance manual-ruling** (`live_dps_extended_smoke.rs`, tuple-decode `.0`) — carries an equivalence argument + full sqlx-signature pin, not a silent waiver. The `invariant_fuzzer/model.rs` post-CS-1 oracle-fix is a documented POST_CS1_CARVEOUT.
7. **CI cost:** the main test leg went ~7m → ~15m (fetch-depth:0 for the provenance git-blob access + 5 new crate legs + provenance/inventory tests).

---

## §3 · Operator / admin actions (cannot be done from code)
1. **Add `mutation diff-gate` to `main`'s required status checks** (branch protection). The YAML is now safe for it.
2. **Schedule the lint-debt PR** for the 3 clippy-excluded crates (or accept the documented exclusion).

---

## §4 · Question for the auditor
Does the above **close the NO-GO** (CS-1 is now legitimately "fiscal-runtime & persisted-representation
compatible; deliberate unsupported source-API refactor", with the missing gates/pins delivered + teeth-proven)
→ **CS-1 GO**? Or is there a residual point (in §1 or a carryover in §2) that must close first? This is a
verification pass — every §1 teeth was re-verified by the architect (revert→RED); challenge any you doubt.
