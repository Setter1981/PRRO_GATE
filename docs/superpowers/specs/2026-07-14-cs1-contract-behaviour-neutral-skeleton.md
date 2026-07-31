# CS-1 Contract — Fiscal-Runtime & Persisted-Representation Compatible Crate Skeleton (rev 5, CONTRACT-READY)

**Status: ✅ CONTRACT-READY (rev 5). 2026-07-15.** **CS-1R re-scope (spec
`2026-07-15-cs1r-remediation-spec.md` §1 R4).** The *global* "behaviour-neutral" claim is **withdrawn
and replaced** by the honest scope: this work is **"fiscal-runtime and persisted-representation
compatible; a deliberate *unsupported* Rust source-API refactor."** "Behaviour-neutral" is retained
**only** where it strictly means storage/serde bytes (SQLite affinity, TEXT literals, 16-byte BLOB,
`#[serde(rename)]` output — all byte-identical; see §2). The Rust *source* API is NOT neutral: the
8 TEXT enums + 6 UUID-BLOB ids + `CashierId` lost their public `sqlx::{Type,Encode,Decode}` impls
(they moved store-side onto `prro::db::types::Db*` wrappers), and new public methods appeared
(`from_sql_str`, `CashierId::from_persisted_unchecked`). The **Source-API-break register (§11)** names
this verbatim; **`publish = false`** on `prro` + `prro-domain` (R4.2) makes "no *supported* external
Rust API" honest. See §11 for the register + the RATIONAL-trade-off statement + the RP-R4-1a/1b/1c
facade pins.

Rev 4 applied the two final mechanical fixes: RP-CS1-5 CashierId decode
= **empty-SILENT / oversize-WARN** + `#[serde(transparent)]` unchanged; and the RP-CS1-2 command
matrix is now **literal & executable** (harness-scoped live-dps `--test live_dps_extended_smoke`,
`FUZZ_CASES` capstone nightly, `--features prro/test-support --locked` inventory). Ready for the
implementer (dual-session).
Rev 2 integrated CS1-V1…V5; **rev 3** closes the three narrow residuals: CashierId
silent-empty-decode + `from_persisted_unchecked` hydration & DriverId has no sqlx wrapper (§2/§3);
manifest tightened — explicit `CashierIdError/DriverIdError`, **no `FiscalCommand` alias**,
`InboxStatus` stays in `prro`, `Severity`→domain (§2/§3); and RP-CS1-2 now pins the **literal**
command matrix + the full **179-file** SHA inventory + machine-readable `nextest list` name/status
diff (§7). Open questions resolved (§10). Baseline: `origin/main f2c17b1` (code `8ec99ca`).
Dual-session: architect authors this + RED-pins; implementer writes test-first. **Storage/serde-byte
neutral** — module location, crate boundaries, re-exports, and the sqlx-mapping relocation change,
but the persisted bytes and fiscal runtime do NOT. The **Rust source API is deliberately, and
unsupportedly, refactored** (§11).

---

## 1 · Scope
Workspace skeleton + move the **pure** model out of `prro` behind a compatibility facade so all
existing tests compile & pass unchanged. **Deferrals (CS1-V2):** `SubmissionEvidence`,
`TransitionPlan`, and the transition oracle **do not exist in code** — do NOT invent them here; the
oracle API moves in **CS-2**, `TransitionPlan` freezes with **spec #6 / CS-4**. The three
`*-contract` crates are created as **empty dependency skeletons** (crate boundaries + dependency
arrows only) until **specs #3–5** define their ports.

## 2 · Representation matrix (REPLACES "every enum/id is TEXT" — CS1-V1, this is the closure of RP-CS1-5)
The relocation must be a **storage non-event** — byte-identical. Normative per type:

| type | kind | SQLite affinity | encoding | NULL | malformed decode | unknown value | serde |
|---|---|---|---|---|---|---|---|
| `DocumentId`,`RequestId`,`ShiftId`,`OperatorId`,`PrinterId`,`OfflineSessionId` | UUID newtype (`Uuid`) | **BLOB** | 16 raw bytes (`as_bytes().to_vec()` / decode `[u8;16]`) | `Option<T>` ⇒ SQL `NULL` (e.g. `node_state.rs` `Option<ShiftId>`) | **length≠16 ⇒ decode error** (no truncation/pad) | n/a | `Uuid` serde unchanged |
| `CashierId` | TEXT String newtype | **TEXT** | `String` (`as_str`) | `Option` ⇒ `NULL` | decode is legacy-tolerant: **empty ⇒ accepted SILENTLY; `>MAX_LEN` ⇒ accepted WITH warning** (ids.rs:243-265). Strict `new()` still rejects Empty/TooLong. **Add `CashierId::from_persisted_unchecked` (a.k.a. `hydrate_from_store`)** — the private field + strict `new()` cannot build legacy values; the warning stays **store-side** in `DbCashierId` | n/a | rename unchanged |
| `DriverId` | TEXT String newtype | TEXT | `String` — **NO `sqlx::Type/Encode/Decode` in baseline** (ids.rs:135-180); bound as **raw `String`** at the repo boundary → **do NOT create a `DbDriverId` wrapper**; keep the raw-String DB boundary + strict `DriverId::new()` on ingress | `Option` ⇒ NULL | strict `new()` | n/a | unchanged |
| `DocState`(14),`OfflineSessionState`(5),`ShiftState`(9),`NodeMode`(7),`Protocol`(6),`DocType`(12),`Severity` | `#[sqlx(type_name=TEXT)]` enum | TEXT | UPPER_SNAKE literal via `#[sqlx(rename=$sql)]`+`as_str()` | `Option` ⇒ NULL | n/a | **unknown literal ⇒ decode error** (closed set) | `#[serde(rename=$sql)]` **byte-identical** |
| `InboxStatus` | `#[sqlx(TEXT)]` enum | TEXT | as above | — | — | decode error | unchanged — **STAYS in `prro`** (persistence-lifecycle, baseline-unused except decl `enums.rs:136-142`); its domain-vs-store home is decided in **spec #3**, not CS-1 |
| `FiscalMode` | TEXT enum | TEXT | **lowercase `test` / `prod`** (NOT upper) | — | — | unknown ⇒ decode error | serde `test`/`prod` |

**RP-CS1-5 (rev 2):** a per-type conformance test — for every UUID-id: `typeof(col)='blob'`,
`length=16`, exact hex round-trip, **length≠16 rejected**; `CashierId`: constructor rejects
Empty/TooLong **AND** decode still hydrates legacy **empty SILENTLY and `>MAX_LEN` WITH a warning** (ids.rs:243-265) — and `CashierId` serde is `#[serde(transparent)]`, **unchanged** (not a rename); every enum:
`from_str(as_str(v))==v` for all variants, the stored TEXT equals the pre-move literal (a
`*_locked.rs`-style table), unknown literal → error, `Option` → NULL not empty; **serde output
byte-identical** for all. `FiscalMode` lowercase pinned explicitly.

## 3 · Symbol / ownership manifest (CS1-V2 — the move list is CLOSED, no authoring)
| old symbol / path | owner (baseline) | new home | compatibility |
|---|---|---|---|
| `db::models::enums::{DocState,OfflineSessionState,ShiftState,NodeMode,Protocol,DocType,FiscalMode,Severity}` | prro | `prro-domain` (pure, `as_str/from_str`) | `prro::db::models::enums` = shim `pub use prro_domain::{…}` (explicit, per-symbol) — **`InboxStatus` NOT moved** (stays `prro`, §2) |
| `db::models::ids::{DocumentId, RequestId, ShiftId, OperatorId, PrinterId, OfflineSessionId, CashierId, DriverId}` **+ `CashierIdError`, `DriverIdError`** (ids.rs:119-143) | prro | `prro-domain` (pure; **no sqlx**) | `prro::db::models::ids` = explicit per-symbol shim |
| `services::write_path::types::CanonicalFiscalCommand` | prro | `prro-domain` — **moved under the SAME name** | `prro::services::write_path::types::CanonicalFiscalCommand` = shim. **NO `FiscalCommand` alias in CS-1** — only the existing `CanonicalFiscalCommand` |
| the sqlx `Type/Encode/Decode` impls (exist ONLY on the UUID-BLOB ids `+` `CashierId`; **`DriverId` has none**) | prro | new `prro::db::types` wrappers `DbDocState`/`DbDocumentId`/`DbCashierId`/… (§4); `DriverId` keeps its raw-`String` boundary — **no wrapper** | stays in `prro` until CS-7 |
| `SubmissionEvidence`, `TransitionPlan`, oracle | **DO NOT EXIST** | — | **deferred** (oracle→CS-2, TransitionPlan→spec#6/CS-4) — NOT in CS-1 |
| real pure value candidates (e.g. `TaxResolutionSnapshot`, BPS/kopeck value types) | prro (services/xml) | list explicitly here **before** moving | move only if named; never "looks pure" |

**Stays adapter/format (CS1-V2 open-q4):** `admin_w4_z0` (CLI/store use-case), `doctor` verdicts
(diagnostic read-model), `xml` structs (protocol-specific `<DAT>`/`<MAC>`) — **not** domain.

## 4 · sqlx decoupling (adopted mechanism — CS1-V1 open-q1)
Domain types are sqlx-free. All TEXT/BLOB/legacy mapping is centralised in **one `prro::db::types`
module** as store-side wrappers (`DbDocState`, `DbDocumentId`, `DbCashierId`, …) implementing
`Type/Encode/Decode`, mapping to/from the pure `prro_domain` types. (Not scattered `String`/`Vec<u8>`
per-repo conversions.) This module **moves wholesale into `prro-store-sqlite` at CS-7**. Orphan rule
respected: the wrappers are `prro`-local, so `impl sqlx::* for DbX` is legal.

## 5 · Facade = explicit compatibility shims (CS1-V3 — NOT `pub use prro_domain::*`)
The old modules **remain** as explicit re-export shims preserving **every** legacy path — verified
callers include `prro::db::models::enums::*`, `prro::db::models::ids::*`,
`prro::services::write_path::types::CanonicalFiscalCommand` (e.g. `tests/a3_final_binding_flip.rs:34-35`).
**Existing test/`use` imports are NOT edited.**

## 6 · Explicitly NOT in CS-1
No coordinator (CS-4); no typed-delivery **semantic** change / double-issue fix (CS-3); no store
extraction (CS-7); no adapter migration (CS-6); no transition-table data (CS-2); no `NodeMode`→axes
split (CS-5); no invented ports / oracle / TransitionPlan.

## 7 · RED-pins (rev 2 — executable gates, CS1-V4 + CS1-V5)

> **SUPERSEDED-IN-PART by CS-1R R1/R2 (2026-07-15..16). READ THIS FIRST.** The
> RP-CS1 gates below describe the *original* CS-1 skeleton design. Where this text
> conflicts with the shipped gates, the SHIPPED gates win:
> - **RP-CS1-1 purity is now ALLOWLIST-based, not the 7-name denylist.** The
>   authoritative gate is `rust/prro-domain/tests/purity_gate.rs` (R2): a pinned
>   `--all-features` cargo-metadata **transitive-closure manifest**
>   (`purity-closure.lock`, node+edge set-equality) + an **exact direct-dep
>   allowlist** `{serde, thiserror, uuid}` pinned by FULL RECORD
>   (name/rename/req/source/kind/target/default-features/features — CS-1R2 A3a),
>   NOT merely a 7-name blocklist. The 7 names survive only as a fast, legible
>   `FORBIDDEN` smoke set layered on top.
> - **The §7(c) `--workspace` command matrix is superseded by the R1 package-scoped
>   member×dimension matrix** (`rust-prro.yml` build job runs per-package build+test
>   for all 13 members; `fmt-clippy.yml`; the CS-1R R1.2 inventory gate). The
>   inventory snapshot is the two literal profile commands in
>   `scripts/cs1r/mint_manifests.sh`, pinned to `nextest@0.9.137`.
> - **RP-CS1-6 CI-wiring** is realised by CS-1R R1.3 and, as of **CS-1R4**, hardened
>   by REMOVING the path-detector entirely: the required `x86_64-unknown-linux-gnu`
>   context runs UNCONDITIONALLY on every push + PR (no `changes` dispatch job to
>   defeat), and `rust/prro/tests/cs1r4_integrity_job_coverage.rs` asserts the
>   integrity job covers every workspace member (build∪test) + the required
>   `test-support` feature. (The old `scripts/cs1r/manifest_detector_paths.py` +
>   `scripts/cs1r/rust_change_paths.py` detector-consistency machinery was deleted —
>   there is no detector left to keep consistent.)
> - **RP-CS1-4 testkit-absence** is enforced by
>   `rp_cs1_4_contract_dag.rs::prro_testkit_absent_from_production_closures` (A3b).

- **RP-CS1-1 (structural purity — PRIMARY = cargo-metadata, canary = trybuild):** a test parses
  `cargo metadata` and asserts `prro-domain`'s **normal + build + target** dependency edges contain
  none of `sqlx / tonic / tokio / axum / prost / hyper / reqwest` (catches alias / build-dep /
  target-dep evasion that a `use`-level trybuild misses). A `write_tx_conn_compile_fail.rs`-style
  trybuild stays as a fast canary. *(Superseded — see banner: the shipped gate is
  the R2 allowlist + closure manifest, not this denylist.)*
- **RP-CS1-2 (equivalence — inventory, not a pass-count):**
  **(a) Source inventory** — a locked manifest of the SHA of **every** `.rs` under `tests/`
  recursively (**all 179**, not only the 167 top-level targets) + `src/**` test modules; a
  behaviour-neutral PR may change these only by path relocation, never assertion edits.
  **(b) Test-name/status inventory** — a machine-readable snapshot
  (`cargo nextest list --workspace --features prro/test-support --message-format json --locked` on
  `f2c17b1`) of every test's name + `ignore` status; the post-move diff is
  **additions-only** — no test deleted, renamed, `cfg`-gated off, or newly `#[ignore]`d.
  **(c) Literal command matrix** (the pinned, CI-enforced set; verify each before freeze):
  - production build — `cargo build --workspace --locked`
  - unit+integration — `cargo nextest run --workspace --features prro/test-support --locked`
  - live-DPS **compile-only** (never executes; **harness-scoped**, exactly as CI `rust-prro.yml:118`) —
    `cargo test -p prro --features live-dps --test live_dps_extended_smoke --no-run --locked`
  - format — `cargo fmt --all -- --check`
  - lint — `cargo clippy --workspace --all-targets --features prro/test-support -- -D warnings`
    (plus `-p prro_crypto`/`-p prro_escpos` scopes as today)
  - nightly (capstone large-N fuzzer; knob is **`FUZZ_CASES`**, **never `PROPTEST_CASES`** — a guard
    step fails the job if the latter is set, `fuzzer-nightly.yml`) —
    `FUZZ_CASES=<N> cargo nextest run -p prro --features test-support --locked -E 'test(/^harness_(online|offline)_seeded$/)'`
    plus the mutation diff-gate `scripts/mutation/run.sh diff`.
- **RP-CS1-3 (facade completeness):** a compile-only test that references the **full list** of legacy
  paths (§5), not just `prro::…` — it must resolve unchanged.
- **RP-CS1-4 (contract-crate DAG):** cargo-metadata pin: `prro-domain` + the three `*-contract`
  crates carry no adapter deps; contracts never depend on each other; `prro`/`prro-engine` may depend
  on contracts. `prro-testkit`: **absent from every production package's normal/build dependency
  graph** (correct invariant — `cargo build --workspace` still compiles it as a member; that's fine).
- **RP-CS1-5 (representation matrix conformance):** §2 table, per type. The closure of CS1-V1.
- **RP-CS1-6 (CI wiring):** the CI **path-detector, push-paths, fmt/clippy package list, and nightly**
  are updated to include every new crate (baseline knows only `rust/prro`/crypto/escpos,
  `rust-prro.yml:14-22,75`, `-p prro` at `:108-126`; `fmt-clippy.yml:45-58`) — else a
  `prro-domain`-only change gets a **skipped** build. CI runs workspace + feature-matrix, excludes
  manual live-dps.

## 8 · PR breakdown (each one reviewable, workspace-green, behaviour-neutral)
1. **CS-1a:** workspace scaffolding + empty `prro-domain` + `prro-testkit` (`publish=false`) + CI
   updated (RP-CS1-1, RP-CS1-4, RP-CS1-6).
2. **CS-1b (TEXT enums):** move the `#[sqlx(type_name=TEXT)]` enums to `prro-domain` (pure); relocate
   their mapping to `prro::db::types`; enum shims (RP-CS1-5 enums, RP-CS1-3).
3. **CS-1b′ (BLOB + legacy IDs):** move the UUID-BLOB ids + `CashierId`/`DriverId` to `prro-domain`;
   `Db*` wrappers for the 16-byte-BLOB + legacy-tolerant-TEXT mapping (RP-CS1-5 ids).
4. **CS-1c:** move `CanonicalFiscalCommand` (**same name**) + any manifest-listed pure value types;
   shims.
5. **CS-1d:** the three empty `*-contract` crate boundaries (no ports yet).
Every PR keeps the RP-CS1-2 inventory additions-only.

## 9 · Dependency policy for `prro-domain` (CS1-V2 open-q2)
Allow `uuid` (keep `now_v7`/`new_v5`/serde unchanged, ids.rs:15-24). `chrono` only if a moved type
truly needs it, **without a clock** (the oracle must never call `now()`). **No `rust_decimal`** —
absent in baseline; money stays integer kopecks / BPS. Forbid all I/O/runtime crates (RP-CS1-1).

## 10 · Resolved (review answers)
1. **UUID-BLOB set = exactly** `{DocumentId, RequestId, ShiftId, OperatorId, PrinterId,
   OfflineSessionId}` (ids.rs:68-73); **TEXT-shaped = `{CashierId, DriverId}`**, but a typed
   sqlx-TEXT mapping exists **only on `CashierId`** (`DriverId` = raw String, §2/§3).
2. **`Severity` → `prro-domain`** (protocol-independent classification, used across services).
   **`InboxStatus` stays in `prro`** (persistence-lifecycle, baseline-unused-but-decl); its
   domain-vs-store home is decided in **spec #3**, not CS-1.
3. **Order `CS-1b` → `CS-1b′` confirmed** (homogeneous TEXT-enums + the facade/wrapper pattern first,
   then the riskier BLOB-ids + legacy hydration); `CanonicalFiscalCommand` (CS-1c) after both.

## 11 · Source-API-break register (CS-1R rev 5 — normative)

**Framing.** §1–§10 correctly establish that CS-1 is a **storage/serde-byte non-event** (SQLite
affinity, TEXT literals, 16-byte BLOB, `#[serde(rename)]` output — byte-identical; RP-CS1-5). The
prior *global* "behaviour-neutral" label over-claimed: the **Rust source API is not neutral**. This
register names the break honestly. It does NOT roll the split back — it re-scopes the claim.

**11.1 Removed public trait impls (the break).** The following impls existed on the domain types in
the CS-1 baseline (`f2c17b1`) and are **removed** at CS-1 head (`f2628ba`) — they now live store-side
on the `prro`-local `prro::db::types::Db*` wrappers (orphan-rule legal there), NOT on the pure
domain types:

| type set | removed impls (exact trait signatures) |
|---|---|
| 8 TEXT enums (`DocState`, `DocType`, `FiscalMode`, `NodeMode`, `OfflineSessionState`, `Protocol`, `Severity`, `ShiftState`) | `impl sqlx::Type<Sqlite>`, `impl<'q> sqlx::Encode<'q, Sqlite>`, `impl<'r> sqlx::Decode<'r, Sqlite>` |
| 6 UUID-BLOB ids (`DocumentId`, `RequestId`, `ShiftId`, `OperatorId`, `PrinterId`, `OfflineSessionId`) | `impl sqlx::Type<Sqlite>`, `impl<'q> sqlx::Encode<'q, Sqlite>`, `impl<'r> sqlx::Decode<'r, Sqlite>` |
| `CashierId` | `impl sqlx::Type<Sqlite>`, `impl<'q> sqlx::Encode<'q, Sqlite>`, `impl<'r> sqlx::Decode<'r, Sqlite>` |
| `DriverId` | **none removed** — `DriverId` had NO sqlx impls in the baseline (raw-`String` DB boundary, §2/§3). Recorded so the register is exhaustive: 8 enums + 6 BLOB ids + `CashierId` broke; `DriverId` did not. |

**Consequence (compile-observable, RP-R4-1b pins it):** `.bind(DocState::Prepared)`, a
`T: sqlx::Type<Sqlite>` / `T: sqlx::Encode<'q, Sqlite>` bound monomorphised on any legacy path
(`prro::db::models::DocState`, `…::DocumentId`, …), or a `query_as::<Db*>`-free decode of the raw
domain type **no longer compiles (E0277)**. Callers wrap: `.bind(DbDocState(DocState::Prepared))`.

**11.2 New public surface (added by CS-1).**
- **`from_sql_str(&str) -> Option<Self>`** on all **8 TEXT enums** — the pure parse half the store-side
  `Db*::decode` delegates to (exact-literal closed-set match; unknown ⇒ `None`).
- **`CashierId::from_persisted_unchecked(String) -> Self`** — hydrates a legacy/oversize persisted
  value bypassing strict `new()` (the private field + strict `new()` cannot rebuild pre-W14a-2a
  empties or oversize drift; the store-side `DbCashierId::decode` calls it).

**11.3 Changed defining-crate / type identity.** Every legacy path
(`prro::db::models::enums::DocState`, `prro::db::models::DocState`, `…::ids::DocumentId`, …) now
resolves to a **`prro_domain`** type (re-exported through the explicit `prro::db::models` facade),
NOT a `prro`-local one. Even where the *name* is unchanged, the **defining crate** — hence the
trait-impl surface reachable under the orphan rule — changed. External code that relied on `impl`ing
its own traits for these types from a third crate, or on the `prro`-crate identity, is affected.

**11.4 Trade-off statement (the break was RATIONAL, not impossible).** The orphan rule forbids
`impl sqlx::Type for prro_domain::DocState` *from `prro`*, but source-API compatibility **was
reachable** by worse means, all considered and rejected:
- **(a) `prro`-local compat/shim types** re-exposing the sqlx surface — duplicates the type identity,
  re-introduces the `prro`↔store coupling CS-1 exists to sever.
- **(b) duplicate-with-conversion** (a second, sqlx-bearing copy of each type + `From`/`Into`) —
  double the maintenance surface, two "sources of truth" for every variant/literal.
- **(c) a `sqlx` feature *in* `prro-domain`** — re-pollutes the pure crate with a DB/runtime
  dependency, defeating the RP-CS1-1 purity gate (the whole point of the split).
All three are strictly worse than the `Db*` store-side wrappers (§4). Dropping the source-API is
therefore a **deliberate trade-off**, correctly named here — **not** an impossibility to be undone.

**11.5 `publish = false` (R4.2).** `prro` **and** `prro-domain` now carry `publish = false` in their
`[package]` (baseline had neither; only `prro-testkit` + the 3 contract crates set it). The honest
statement is **"no *supported* external Rust API"** — NOT "no external consumers" (that claim was
false). The crates are workspace-internal; the source-API refactor is unsupported for outside
consumers.

**11.6 Facade = closed legacy surface (RP-R4-1a/1b/1c).** The `prro::db::models::mod.rs`
`pub use enums::*; pub use ids::*;` globs are replaced by an **explicit per-symbol legacy
export-list** (no widening). Three pins guard it (spec §1 R4.5):
- **RP-R4-1a** — per-type retained-surface compile-manifest, both paths (nested
  `prro::db::models::enums::DocState` + short `prro::db::models::DocState`); the surface is
  **NON-uniform** (enums: `Copy`+`Hash`+serde+`as_str`+`from_sql_str`; BLOB ids:
  `Copy`+`Hash`+`serde(transparent)`+`new`/`from_bytes`/`as_bytes`/`Default`, no public `now_v7`;
  `CashierId`: no `Copy`, has `from_persisted_unchecked`/`as_str`/`into_inner`/`Display`/`FromStr`;
  `DriverId`: no `Copy`/`Hash`/`Serialize`/`Deserialize`).
- **RP-R4-1b** — `trybuild` compile-FAIL fixtures proving the legacy types no longer satisfy
  `sqlx::Type<Sqlite>` / `Encode` (pins §11.1, not a restoration).
- **RP-R4-1c** — an **AST parse** (`syn`) of `db/models/mod.rs` asserting **no glob** + **exact
  set-equality** of the `pub use` set to the pinned legacy list (the only guard that catches
  *widening*).
Teeth: remove a legacy export → RP-R4-1a/1c RED; restore a glob or add an extra export → RP-R4-1c
RED; a legacy `.bind`/`Type<Sqlite>` compiling again → RP-R4-1b RED.

## 12 · CI command-matrix conformance — the LITERAL per-member table (CS-1R R1.3)

This **amends and makes literal** the RP-CS1-2(c) command matrix and RP-CS1-6 CI-wiring item above:
it is the normative, package-scoped (NOT literal `--workspace`) `member × {build, test, fmt, clippy}`
table the CI workflows MUST satisfy. `✅` = a leg present on the line cited; `➕` = a leg CS-1R adds
(literal command given). 12 workspace members (`xtask` is a dev tool, out of the product matrix).

| member | build | test | fmt | clippy |
|---|---|---|---|---|
| `prro` | ✅ `rust-prro.yml` build step | ✅ `nextest run -p prro --features test-support` (+ live-dps build step) | ✅ `fmt-clippy.yml` fmt list | ✅ `fmt-clippy.yml` clippy `-p prro` |
| `prro-domain` | ✅ via the `prro-domain … contract` nextest leg | ✅ same leg | ✅ fmt list | ✅ clippy `prro-domain` |
| `prro-testkit` | ✅ via that leg | ✅ that leg | ✅ fmt list | ✅ clippy `prro-testkit` |
| `prro-ingress-contract` | ✅ via that leg | ✅ that leg | ✅ fmt list | ✅ clippy contract crates |
| `prro-dps-contract` | ✅ via that leg | ✅ that leg | ✅ fmt list | ✅ clippy contract crates |
| `prro-fleet-contract` | ✅ via that leg | ✅ that leg | ✅ fmt list | ✅ clippy contract crates |
| `prro_crypto` | ✅ via the `prro_crypto` nextest leg | ✅ that leg | ✅ fmt list | ✅ clippy `prro_crypto` |
| `prro_crypto_v2` | ➕ `cargo build -p prro_crypto_v2 --locked` | ➕ `cargo nextest run -p prro_crypto_v2 --locked` | ➕ fmt list | ➕ `cargo clippy -p prro_crypto_v2 --all-targets --no-deps -- -D warnings` (pre-existing findings **GRANDFATHERED** via crate-root `#![allow]`, CT-sensitive; per-finding cleanup deferred) |
| `prro_sidecar` | ➕ `cargo build -p prro_sidecar --locked` | ➕ `cargo nextest run -p prro_sidecar --locked` | ➕ fmt list | ➕ `cargo clippy -p prro_sidecar --all-targets --no-deps -- -D warnings` (CLEAN) |
| `maria304_driver` | ➕ `cargo build -p maria304_driver --locked` | ➕ `cargo nextest run -p maria304_driver --locked` | ➕ fmt list | ➕ `cargo clippy -p maria304_driver --all-targets --no-deps -- -D warnings` (CLEAN — `too_many_lines` scoped-`#[allow]` + trailing-comma fix) |
| `prro_escpos` | ➕ `cargo build -p prro_escpos --locked` | ➕ `cargo nextest run -p prro_escpos --locked` | ✅ fmt list | ✅ clippy `prro_escpos` |
| `prro_escpos_daemon` | ➕ `cargo build -p prro_escpos_daemon --locked` | ➕ `cargo nextest run -p prro_escpos_daemon --locked` | ➕ fmt list | ➕ `cargo clippy -p prro_escpos_daemon --all-targets --no-deps -- -D warnings` (CLEAN — test-only `byte str` fixed `&[b'\n']`→`b"\n"`) |

All 12 members carry at least one test target (verified: `prro_crypto_v2` 21 src test-files;
`prro_sidecar` 1 integration-test dir + 10 src; `maria304_driver` 10 integration-test dirs + 27 src;
`prro_escpos` / `prro_escpos_daemon` 1 integration-test dir each) — so **no cell is a zero-tests
exclusion**; every member gets **build + test + fmt** (all verified green: 790 tests pass; fmt clean).

**Clippy cell — every member now carries a clippy leg (no exclusions remain).** The three cells
formerly documented as R1.3 exclusions (`prro_crypto_v2`, `maria304_driver`, `prro_escpos_daemon`)
were closed in the CS-1R lint-debt pass (2026-07-16):

* `prro_crypto_v2` — a **CT-sensitive clean-room DSTU-4145 core**. Its pre-existing findings are
  **GRANDFATHERED** via a single crate-root `#![allow(...)]` block in `src/lib.rs` (attributes-only,
  **zero compiled-behaviour change** — `git diff` is lib.rs `+N/-0`). This deliberately lints **future**
  crypto code while a proper per-finding, CT-aware cleanup is deferred. Constant-time crypto is **never**
  `clippy --fix`-ed / autofixed (an autofixed loop or cast can silently break a CT property).
* `maria304_driver` — fixed CLEAN: a scoped `#[allow(clippy::too_many_lines)]` on `dispatch_prepare`
  (cohesive protocol dispatch) plus a trailing-comma test-fmt fix.
* `prro_escpos_daemon` — fixed CLEAN: a test-only byte-slice lint (`&[b'\n']` → `b"\n"`,
  behaviour-identical).

`prro_sidecar` was verified CLEAN and joined the gate in R1.3. All 12 product members now satisfy
**build + test + fmt + clippy** — the §12 matrix has no silent blank and no remaining exclusion. Any
future member that cannot satisfy a cell must be replaced by an **explicit documented exclusion with
rationale**, never a silent blank.

**Inventory gate (CS-1R R1.2).** CI wires `scripts/cs1r/inventory_gate.sh --pr <base>` (three-way:
live `nextest list` == committed manifest per profile, additions-only vs base, every new source test
file present in `docs/cs1r/inventory/source_files.sha256` in the same PR). `cargo-nextest` is pinned
to **0.9.137** (was `install-action@nextest` = latest); `cargo` is already `1.95.0` via
`rust-toolchain.toml`.

**Mutation-diff required + no-op companion.** `mutation-diff.yml` is **path-filtered** (`prro/src/**`
+ mutants config), so making its `mutation diff-gate` check REQUIRED in branch protection would hang
an out-of-path PR forever on a never-reported status. CS-1R adds an **always-run no-op companion**
(same job name, reports instantly green when no in-path file changed) so the check can be made
required without stalling docs-only PRs. **Making the check required is a repo-admin branch-protection
action — it cannot be done from workflow YAML and MUST be performed by the operator.**

**RED-pins:** RP-R1-1 (delete/rename/`#[ignore]` a test OR add a test absent from the manifest → the
inventory gate is RED); RP-R1-3 (drop a member from this table OR point a leg at a wrong command → the
CI matrix diverges from the workflows).
