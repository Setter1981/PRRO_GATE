# CS-1 Contract — Behaviour-Neutral Crate Skeleton (rev 4, CONTRACT-READY)

**Status: ✅ CONTRACT-READY (rev 4). 2026-07-14.** External audit cleared all findings
(CS1-V1…V5, §10 resolved). Rev 4 applied the two final mechanical fixes: RP-CS1-5 CashierId decode
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
Dual-session: architect authors this + RED-pins; implementer writes test-first. **Zero behaviour
change** — only module location, crate boundaries, re-exports, and the sqlx-mapping relocation.

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
- **RP-CS1-1 (structural purity — PRIMARY = cargo-metadata, canary = trybuild):** a test parses
  `cargo metadata` and asserts `prro-domain`'s **normal + build + target** dependency edges contain
  none of `sqlx / tonic / tokio / axum / prost / hyper / reqwest` (catches alias / build-dep /
  target-dep evasion that a `use`-level trybuild misses). A `write_tx_conn_compile_fail.rs`-style
  trybuild stays as a fast canary.
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
