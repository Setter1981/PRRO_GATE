# Spec CS-1R — Remediation to legitimately close CS-1

**Status: 🔒 SPEC-READY rev 3 — external reviewer pre-committed "after these four fixes, SPEC-READY without
another full review"; rev 3 folds all four (getrandom 0.4.2 grounding, literal CI table + nextest commands,
dev-only shared walker, per-type retained surface), each verified against code.** Implements
`docs/CS1R_REMEDIATION_PLAN.md` (PLAN-CONFIRMED rev 3). Grounded on `origin/main` `197337d`; CS-1 range
`f2c17b1..f2628ba`. This spec is the **independent oracle** for R1-R4: each RED-pin is a test the implementer
writes test-first; each teeth line is an empirical revert→RED the reviewer runs. rev 2 closed the two
BLOCKER-class green-but-unsound gaps (R2 manifest/walker, R1 provenance/inventory); rev 3 closes the four
grounding defects.

**Verified pins (this repo):** toolchain **1.95.0** (`rust/rust-toolchain.toml`); **sqlx 0.8.6**;
**nextest is NOT currently pinned** (`rust-prro.yml:118` `taiki-e/install-action@nextest` = latest → R1 pins it);
**the lock holds THREE getrandom versions (0.2.17, 0.3.4, 0.4.2); `prro-domain → uuid 1.23.1 → getrandom
0.4.2`** (verified in `Cargo.lock`) — the closure manifest pins exactly **0.4.2** (a three-version case,
validating the node+edge / PackageId model). Clock for UUID v7 comes from `std::time` (no dependency); only
entropy is a dependency (`getrandom 0.4.2`).

**Non-goal / scope guard.** No hot zone touched. Prod-code delta limited to R2 (gate tests + Cargo.toml/lib.rs
text), R3 (`db/types.rs` decode string + one `tracing::warn!` `target:` + `RP-CS1-5`), R4 (contract + ADR +
`publish=false` + `db/models` facade), R1 (a provenance/inventory test-tool + CI). Storage/serde bytes already
byte-identical and MUST stay so.

---

## §1 · R4 — honest re-scope — R4.1-4 docs-only (no RED-pin); **R4.5 RED-first**

**R4.1 Term** → contract rev 5: the *global* "behaviour-neutral" claim → **"fiscal-runtime and
persisted-representation compatible; deliberate *unsupported* Rust source-API refactor"**. "behaviour-neutral"
may remain only where it means storage/serde bytes.

**R4.2 `publish = false`** added to `rust/prro/Cargo.toml` + `rust/prro-domain/Cargo.toml` `[package]`
(verified ABSENT). Statement becomes **"no *supported* external Rust API"**.

**R4.3 Source-API-break register** (normative). Verbatim, with **correct trait signatures**:
- **Removed public impls:** `sqlx::Type<Sqlite>`, `sqlx::Encode<'q, Sqlite>`, `sqlx::Decode<'r, Sqlite>` on the
  8 TEXT enums, the UUID-BLOB ids, and `CashierId`. (`DriverId` had none — recorded.)
- **New public surface:** `from_sql_str(&str) -> Option<Self>` (8 enums), `CashierId::from_persisted_unchecked(String) -> Self`.
- **Changed defining-crate / type identity:** legacy paths resolve to a `prro_domain` type; the defining crate
  (hence trait-impl surface) changed even where the name did not.

**R4.4 Trade-off statement.** The break was a rational trade-off, not an impossibility: (a) `prro`-local
compat types, (b) duplicate-with-conversion, (c) a `sqlx` feature *in* `prro-domain` — all reachable, all
rejected as worse than the `Db*` wrapper.

**R4.5 Facade both-paths + closed legacy surface + registered-break pins (RED-first).** Replace the
`prro/src/db/models/mod.rs` `pub use enums::*; pub use ids::*;` globs with an **explicit per-symbol legacy
export-list**. Pins:
- **RP-R4-1a (positive retained-surface compile-manifest — PER-TYPE matrix; surface is NOT uniform).** A
  compile test that, per legacy type, exercises **exactly that type's retained surface** (verified from
  `prro-domain/src/{enums,ids}.rs`), via both the nested (`prro::db::models::enums::DocState`) and short
  (`prro::db::models::DocState`) paths:
  - **8 TEXT enums** (`DocState`/`DocType`/`FiscalMode`/`NodeMode`/`OfflineSessionState`/`Protocol`/`Severity`/`ShiftState`):
    `Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize` + `as_str()` + `from_sql_str()`.
  - **6 BLOB ids** (`DocumentId`/`RequestId`/`ShiftId`/`OperatorId`/`PrinterId`/`OfflineSessionId`):
    `Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize` + `#[serde(transparent)]` +
    `new()` (**public ctor; `Uuid::now_v7()` is internal — there is NO public `now_v7`**) + `from_bytes` +
    `as_bytes` + `Default`; `ShiftId` additionally `deterministic_for_shift_open`.
  - **`CashierId`**: `Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize` + `#[serde(transparent)]`
    (**no `Copy`**) + `new()->Result` + `from_persisted_unchecked` + `as_str` + `into_inner` + `Display` +
    `FromStr`; `CashierIdError{Empty, TooLong}`.
  - **`DriverId`**: `Debug, Clone, PartialEq, Eq` (**no `Copy`, no `Hash`, no `Serialize`/`Deserialize`, no
    `#[serde(transparent)]`**) + `new()->Result` + `as_str` + `into_inner` + `Display`;
    `DriverIdError{Empty, TooLong}`.
  Proves names, paths AND the exact per-type surface — so dropping `Copy` from an enum, or *adding* `Hash` to
  `DriverId`, is RED.
- **RP-R4-1b (registered-break compile-FAIL):** a `trybuild` `compile_fail` fixture proving the legacy types
  **no longer** satisfy `Type<Sqlite>` / `Encode` / `Decode` (e.g. a `.bind(DocState::Prepared)` or a
  `fn needs<T: Type<Sqlite>>()` monomorphised on `DocState` fails to compile). Pins the *registered* break —
  not a restoration.
- **RP-R4-1c (no-glob / no-widening, verbatim mechanism):** an **AST parse** of `db/models/mod.rs` asserting
  **exact set-equality** of the explicit `pub use` set to a pinned legacy list (or a pinned `cargo-public-api`
  snapshot of `prro::db::models`). Positive path-compilation does NOT detect widening — this does.
- **Teeth (three mutations):** (1) remove a legacy export → RP-R4-1a/1c RED; (2) restore a `pub use *` glob →
  RP-R4-1c RED; (3) add an extra explicit export → RP-R4-1c RED.

---

## §2 · R2 — gate soundness (RED-first) — BLOCKER/MAJOR

A **single shared walker** (`fn metadata_graph(meta) -> Graph`) is used by **both** `purity_gate.rs` and
`rp_cs1_4_contract_dag.rs` **and** by the fixtures — so a fixture exercises the real gate code, never a
test-only copy. **Placement: a DEV-ONLY shared test-support module**, e.g. `tests/support/metadata_graph.rs`
included via `#[path = "support/metadata_graph.rs"] mod metadata_graph;` in both integration test files (it is
the *production gate implementation*, but it is test-tier code). It **MUST NOT** go into `prro-domain`'s `lib`:
that would make `serde_json` a **normal** dependency and break the R2.1 direct allowlist `{uuid, serde,
thiserror}` (serde_json stays a `[dev-dependencies]` entry). Every test/fixture referenced below routes through
this one module.

**R2.1 Direct-dependency allowlist (canonical node+edge records).** The gate asserts `prro-domain`'s **direct**
non-dev dependency set equals exactly **`{uuid, serde, thiserror}`**, matched on the **canonical node+edge
records of R2.2** (i.e. `package_id` + `name` + `version` + `source` + resolved `features`, and the edge's
`dep_kinds`), **not** on `name/source/kind` alone. **Direct build-dependencies MUST be empty.** Contract
crates: direct non-dev set == **∅**, **PHASED** (comment + gate message: changes only via the corresponding
port spec).

**R2.2 Pinned transitive-closure manifest — TWO set-equality tables (kind/target belong to the EDGE, not the
node).** Committed `rust/prro-domain/purity-closure.lock` with:
- **`nodes`:** `{package_id, name, version, source, enabled_features}` — one row per resolved package.
- **`edges`:** `{from_package_id, to_package_id, dependency_alias, dep_kinds:[{kind, target}]}` — one row per
  dependency edge.
The gate computes the live `--all-features` non-dev closure and asserts **set-equality on BOTH tables**. This
catches an edge **rewiring** (e.g. a `wasm`-only edge to an already-present package becoming a Linux-active
edge) that a node-only manifest would pass green. **`getrandom 0.4.2`** (`prro-domain → uuid 1.23.1 →
getrandom 0.4.2`, verified in the lock; the other 0.2.17/0.3.4 nodes are NOT in prro-domain's closure) is an
accepted node, annotated **"OS entropy for UUID v7 random bits"**; `libc` (if present as its syscall ABI)
annotated **"getrandom syscall ABI"**. (Clock is `std::time`, not a dependency — do not annotate a clock dep.) A
legitimate version bump MUST turn the manifest RED (correct signal for so small a closure). A **separate
explicit `cargo xtask update-purity-closure`** regenerates the manifest; **CI never auto-updates it**.

**R2.3 Exact metadata args — one-time flag + a PERMANENT unit-pin.** Both gates invoke
`cargo metadata --format-version 1 --all-features --locked`. A **permanent unit-pin** asserts the exact
argument vector the gate uses (so removing `--all-features` fails forever, not only under a one-time canary —
today's graph has no optional deps, so a bare removal would otherwise stay green).

**R2.4 PackageId traversal.** The shared walker dedups `reached` on **`PackageId` (`dep_id`)**, never the crate
name; the name is used only for the allow/deny/manifest match. Root resolved by **`workspace_members` id**
(not by name — guards against a foreign same-named package).

**R2.5 Honesty texts.** `prro-domain/Cargo.toml:5` + `src/lib.rs:5-12` + gate test names/messages: strike
"I/O-free / no clock"; state **"no DB / network / async-runtime / RPC / HTTP adapter dependencies; the only
capability dependency is `getrandom` (OS entropy for UUID v7); the v7 timestamp uses `std::time`, not a
dependency."**

**R2.6 Honest scope limit** (documented in the gate module): proves *dependency-graph* purity + a pinned
closure, NOT absence of `std::{fs,net,process}` syscalls (invisible to Cargo). Out of scope for CS-1R.

**RED-pins (each canary that mutates `Cargo.toml` MUST also refresh the temporary `Cargo.lock` — else it reds
only via `--locked` staleness, not the gate — AND assert the SPECIFIC gate message):**
- **RP-R2-a (direct allowlist):** add `rusqlite` direct dep (+ lock refresh) → the allowlist assertion fails with its message; remove → PASS.
- **RP-R2-b (all-features, empirical):** add `sqlx = { optional = true }` behind a feature (+ lock refresh) → gate fails; remove → PASS.
- **RP-R2-b2 (all-features, permanent):** the R2.3 unit-pin — delete `--all-features` from the arg vector → the arg-pin fails (permanent guard, no dep needed).
- **RP-R2-c (PackageId, via the SHARED production walker):** a fixture where the **safe same-name version is FIRST** and only the **second** version reaches `sqlx`. Assertions: (i) the production PackageId walker **reaches `sqlx`** and the **purity assertion REJECTS the fixture** (the *test* is GREEN asserting this); (ii) swapping the production `visited` from PackageId to name makes the test **RED** (the dangerous second version is skipped, purity wrongly passes). A **separate** fixture: a foreign package sharing `prro-domain`'s name → root-by-`workspace_members`-id selects the right root (RED if root is chosen by name).
- **RP-R2-d (closure manifest — mutate each axis separately):** perturb, one at a time, a **node** `version` / `source` / `enabled_features`, and an **edge** `kind` / `target` → the corresponding table's set-equality fails; and **removing the `getrandom`/`libc` acceptance annotation** → RED.
- **RP-R2-e (build-dep):** add a `[build-dependencies]` entry (+ lock refresh) → the empty-build-dep assertion fails.
- **RP-R2-f (contract phased):** add a direct dep to a `*-contract` crate → its ∅-allowlist fails.

Removes the **Spec #5A ↔ gate inconsistency** (#5A `spec5-fleet-telemetry.md:64,70`).

---

## §3 · R3 — observable-equivalence pins (RED-first) — narrow MAJOR

**R3.1 Decode message — MATCH-OLD-VERBATIM, frozen via a captured golden.** sqlx 0.8.6 renders
`invalid value {value:?} for enum {EnumIdent}` (depends only on the unknown value + enum name). The spec
**freezes**:
1. **One canonical unknown input** — the literal `__CS1R_UNKNOWN__` — for all 8 types.
2. **The compared error layer** — the **inner `BoxDynError` / `ColumnDecode.source`**, NOT the full
   `sqlx::Error` (which carries column name/index and would be brittle).
3. **A committed golden** `rust/prro/tests/golden/cs1r_decode_errors.json` — the **8 literal strings** captured
   by a **one-time probe at `f2c17b1`** (compile the baseline type, decode `__CS1R_UNKNOWN__`, record) + the
   **probe commit SHA**. The expected values are the golden file — **never computed by the same
   formatter/helper production uses** (avoids common-mode).
4. The current test **reproduces the same SQLite TEXT decode path** (decode from a real `sqlite::memory:` TEXT
   column), not a synthetic string compare.

`DbX::decode` (`db/types.rs:84-89`) is changed to render the frozen baseline format. The sqlx-macro source may
be cited as corroborating evidence, but the **golden is the oracle**, not the current implementation.

**R3.2 Tracing target — restore explicitly** — `target: "prro::db::models::ids"` on the `CashierId` warn
(`db/types.rs:238-246`); message + fields already verbatim.

**R3.3 RP-CS1-5 additions (table-driven, all 8 wrappers):** exact decode-error `Display` == golden (×8, via
the SQLite decode path); the warn `target`+message+fields captured via a `tracing` subscriber; an
**`Option<Enum> ↔ NULL`** round-trip pin **for all 8 wrappers** (not one representative).

**RED-pins:** RP-R3-1 (change any decode string → golden mismatch, ×8), RP-R3-2 (change `target:` → RED),
RP-R3-3 (break NULL mapping → RED, ×8).

---

## §4 · R1 — provenance audit + forward gate + CI matrix (final PR) — BLOCKER

**R1.1 Per-hunk provenance audit — MACHINE mechanism (syn-based), not a manual read.** A committed tool +
artifact:
- Parse both endpoints (`f2c17b1` and `f2628ba`) of each of the **79 modified files** via **`syn`**.
- **Normalize ONLY the allowed constructions:** `DbX(expr) → expr`; `x.map(DbX) → x`; `DbX → X` in a decode
  type position; the service `.map(|w| w.0)`. **After normalization the AST outside these nodes MUST be
  equal.** "Pure formatting" is accepted **only** when the rustfmt-normalized AST/tokens are equal.
- For **every sqlx chain** extract a signature `{file, enclosing_fn, occurrence, runtime_sql_literal_bytes,
  ordered_normalized_bind_expressions, fetch_mode}` and assert the ordered **bind-vector** is unchanged
  (the bind-ORDER pin), SQL literal bytes unchanged, `fetch_mode` unchanged.
- **`manual ruling` is NOT a waiver:** a hunk outside the whitelist is either accompanied by an attached
  equivalence proof, or it is **reverted / registered as an intentional drift** — never silently accepted.
- **Teeth (each → RED):** swap two same-type `.bind`; change a SQL literal; change a fixture value; change a
  control-flow condition; change an assertion RHS.

**R1.2 Forward additions-only gate — three-way control, profile in identity.** Minted at the **final CS-1R
head** (after R2/R3). The gate enforces **all three**:
1. **live `nextest list` == committed manifest** (no drift);
2. the PR's manifest diff vs base may **only add** records;
3. **every new test MUST appear in the manifest in the same PR** (else a new test could be omitted then later
   deleted unnoticed).
Identity row = **`{profile, package, target, test_name, ignored}`** — **profile is part of identity** (else
moving a test from `test-support` to `live-dps` keeps the union and passes green). The **two literal profile
commands** (verbatim):
```
# profile = "test-support"
cargo nextest list --workspace --features prro/test-support \
  --message-format json --locked --target x86_64-unknown-linux-gnu
# profile = "live-dps"
cargo nextest list --workspace --features prro/test-support,prro/live-dps \
  --message-format json --locked --target x86_64-unknown-linux-gnu
```
**Pin `cargo-nextest` to `0.9.137`** (currently UNPINNED — `install-action@nextest` = latest; pin via
`install-action@v2` with `tool: nextest@0.9.137` or the `version:` input); Cargo is already pinned to `1.95.0`
via `rust-toolchain.toml`. **Source selector** counts `#[test]`, `#[cfg(test)]`,
`#[tokio::test]`, `#[rstest]`, `proptest!`, and `#[path]` test modules — not only literal `#[test]`. Scope:
all `rust/**/tests/**/*.rs` + trybuild fixtures + all `rust/**/src/**/*.rs` carrying the above. The retro
snapshots at `f2c17b1` and `f2628ba` are committed **separate immutable provenance** artifacts.

**R1.3 CI command-matrix conformance — the LITERAL per-member table (this IS the amended contract matrix).**
Package-scoped (NOT literal `--workspace`). Current state verified from `rust-prro.yml` (build :122, live-dps
build :130, test legs :138/:147/:160) + `fmt-clippy.yml` (fmt :47; clippy :53/:60/:66/:73) + `mutation-diff.yml`
(prro/src). `✅` = covered today (line); `➕` = leg CS-1R MUST add (literal command given). 12 members:

| member | build | test | fmt | clippy |
|---|---|---|---|---|
| `prro` | ✅ :122 | ✅ :138 `nextest run -p prro --features test-support` (+ :130 live-dps build) | ✅ :47 | ✅ :53 |
| `prro-domain` | ✅ via :160 | ✅ :160 | ✅ :47 | ✅ :66 |
| `prro-testkit` | ✅ via :160 | ✅ :160 | ✅ :47 | ✅ :66 |
| `prro-ingress-contract` | ✅ via :160 | ✅ :160 | ✅ :47 | ✅ :73 |
| `prro-dps-contract` | ✅ via :160 | ✅ :160 | ✅ :47 | ✅ :73 |
| `prro-fleet-contract` | ✅ via :160 | ✅ :160 | ✅ :47 | ✅ :73 |
| `prro_crypto` | ✅ via :147 | ✅ :147 | ✅ :47 | ✅ :60 |
| `prro_crypto_v2` | ➕ `cargo build -p prro_crypto_v2 --locked` | ➕ `cargo nextest run -p prro_crypto_v2 --locked` | ➕ add to :47 list | ➕ `cargo clippy -p prro_crypto_v2 --all-targets --no-deps -- -D warnings` |
| `prro_sidecar` | ➕ `cargo build -p prro_sidecar --locked` | ➕ `cargo nextest run -p prro_sidecar --locked` | ➕ add to :47 | ➕ `cargo clippy -p prro_sidecar --all-targets --no-deps -- -D warnings` |
| `maria304_driver` | ➕ `cargo build -p maria304_driver --locked` | ➕ `cargo nextest run -p maria304_driver --locked` | ➕ add to :47 | ➕ `cargo clippy -p maria304_driver --all-targets --no-deps -- -D warnings` |
| `prro_escpos` | ➕ `cargo build -p prro_escpos --locked` | ➕ `cargo nextest run -p prro_escpos --locked` | ✅ :47 | ✅ :60 |
| `prro_escpos_daemon` | ➕ `cargo build -p prro_escpos_daemon --locked` | ➕ `cargo nextest run -p prro_escpos_daemon --locked` | ➕ add to :47 | ➕ `cargo clippy -p prro_escpos_daemon --all-targets --no-deps -- -D warnings` |

Any cell a member legitimately cannot satisfy (e.g. a leaf with zero tests) is replaced by an **explicit
documented exclusion with rationale**, not a silent blank. The amended contract embeds this table verbatim.
**Mutation-diff:** confirm it is required in **branch protection** (its YAML alone does not guarantee it);
because it is **path-filtered** (`prro/src/**` + mutants config), making it required demands an
**always-reporting / no-op companion job** on out-of-path PRs — else an out-of-path PR hangs forever on a
never-reported required status (the same skip-companion pattern already used for the main test workflow).

**RED-pins:** RP-R1-1 (delete/rename/`#[ignore]` a test, OR add a test absent from the manifest → gate RED);
RP-R1-2 (any of the 5 R1.1 teeth mutations → provenance RED); RP-R1-3 (drop a member from the matrix, OR point
a leg at a wrong command → matrix gate RED — it checks the real workflow commands, not just a table row).

---

## §5 · RED-pin battery + teeth protocol

| Pin | Proves | Teeth (revert→RED) |
|---|---|---|
| RP-R4-1a | retained traits/methods on both paths | remove a legacy export → RED |
| RP-R4-1b | registered sqlx-trait break | (compile-fail) — a legacy `.bind` compiling → RED |
| RP-R4-1c | no glob / no widening (AST set-equality) | restore glob OR add extra export → RED |
| RP-R2-a | direct allowlist catches un-named DB client | add `rusqlite` (+lock) → RED |
| RP-R2-b / b2 | `--all-features` (empirical + permanent arg-pin) | add optional `sqlx` (+lock) / drop `--all-features` arg → RED |
| RP-R2-c | PackageId traversal via SHARED walker | swap production visited ID→name → RED |
| RP-R2-d | node+edge closure manifest bites | mutate node ver/source/features OR edge kind/target OR drop acceptance → RED |
| RP-R2-e | build-deps forbidden | add build-dep (+lock) → RED |
| RP-R2-f | contract phased-∅ | add a direct dep to a contract crate → RED |
| RP-R3-1 | decode msg == golden (×8, inner error layer) | change a string → RED |
| RP-R3-2 | warn target restored | change `target:` → RED |
| RP-R3-3 | Option↔NULL (×8) | break NULL mapping → RED |
| RP-R1-1 | additions-only + live==manifest + new-test-present | delete/ignore OR omit-new-test → RED |
| RP-R1-2 | AST/query provenance | any of 5 mutations → RED |
| RP-R1-3 | CI member×dimension on real commands | drop member / wrong command → RED |

**Teeth discipline** (`feedback_quality_spec_bar` §4 + `project_real_teeth_roi_pr257`): the reviewer
EMPIRICALLY reverts each guard, confirms the named test RED, restores. RP-R2-c/d and RP-R1-1/2 specifically
kill the green-but-unsound failure mode this whole spec exists to close.

## §6 · Sequencing / PR breakdown
1. **R4** (contract rev 5 + ADR + `publish=false` + facade explicit-list + RP-R4-1a/b/c) — docs + facade.
2. **R2 ∥ R3** — independent RED-first PRs (shared walker + node+edge manifest + arg-pin; golden + observable pins).
3. **R1** (final) — syn provenance tool + forward three-way gate minted at this head + CI member×dim table + branch-protection.

Each PR workspace-green, RED-first, no hot-zone. R1 last (its forward baseline must include R2/R3 tests).

## §7 · Conformance checklist
- [ ] R4: term; `publish=false` (prro+prro-domain); register (correct trait sigs + new methods + type-identity); trade-off; facade explicit-list; RP-R4-1a/b/c green + 3 teeth.
- [ ] R2: shared production walker; direct allowlist on node+edge records + empty build-deps; two-table closure manifest (getrandom exact-version + libc accepted; annotations); `--all-features --locked` + permanent arg-pin; PackageId dedup + root-by-workspace-id; honesty texts; `update-purity-closure` xtask (CI never auto-updates); RP-R2-a..f green + teeth (canaries refresh lock + assert message).
- [ ] R3: golden captured at f2c17b1 (8 strings + SHA, inner error layer, canonical `__CS1R_UNKNOWN__`); decode renders golden; warn target restored; Option↔NULL ×8; RP-R3-1..3 green + teeth.
- [ ] R1: syn provenance tool (allowed-normalization + AST-equality + sqlx signature/bind-order); manual-ruling≠waiver; forward three-way gate (live==manifest, additions-only, new-test-present) with profile-in-identity; two literal profiles; nextest 0.9.137 pinned; extended source selector; CI member×dimension literal table (gaps filled); mutation-diff required + no-op companion; RP-R1-1..3 green + teeth.

---
*Oracle for task #36. Grounded/verified: rust-toolchain.toml=1.95.0 · sqlx 0.8.6 · nextest unpinned (install-action@nextest) → pin 0.9.137 · getrandom 0.2.17/0.3.4/0.4.2 in lock, prro-domain→uuid 1.23.1→getrandom 0.4.2 · per-type surface from enums.rs/ids.rs · CI legs rust-prro.yml:122/130/138/147/160 + fmt-clippy.yml:47-73 · plan rev 3.*
