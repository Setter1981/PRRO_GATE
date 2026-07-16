# CS-1 test-provenance audit — machine artifact

**Spec:** `docs/superpowers/specs/2026-07-15-cs1r-remediation-spec.md` §4 **R1.1**.
**Tool (machine mechanism):** `rust/prro/tests/cs1_test_provenance.rs` +
`rust/prro/tests/support/cs1_provenance.rs` (syn-based). This markdown is the
committed **artifact** the tool's classification is recorded in; the tool is the
**oracle** (it re-derives every claim below on each CI run — this file is the
human-readable record, not the source of truth).

## Pinned endpoints (immutable)

| role | short | full SHA | date |
|---|---|---|---|
| base (pre-CS-1) | `f2c17b1` | `f2c17b1e9cd125d5018cd671dd596fc5c1e2e7bb` | 2026-07-14 21:16:38 +0300 |
| head (CS-1 done) | `f2628ba` | `f2628ba76f0a4de648638d3ab14ea8ba3cdd9436` | 2026-07-15 07:02:50 +0300 |

The **immutable provenance leg** (`cs1_immutable_provenance_base_vs_head`)
compares the two git blobs. The **live-drift / teeth leg**
(`cs1_live_drift_base_vs_worktree`) compares the base blob against the
**working-tree** file, so a mutation in *this* PR to any CS-1 test file is caught
RED (RP-R1-2). One file — `invariant_fuzzer/model.rs` — carries an approved
post-CS-1 delta (the `32166cc` fuzzer oracle fix) and is compared against its
`f2628ba` blob in the live leg (a documented carve-out;
`POST_CS1_CARVEOUT` in the test).

## Scope

**79 modified** + **5 added** `rust/prro/tests/*.rs` under `f2c17b1..f2628ba`.
The **5 added** files (`rp_cs1_3_command_facade.rs`, `rp_cs1_3_enum_facade.rs`,
`rp_cs1_3_id_facade.rs`, `rp_cs1_5_db_enum_roundtrip.rs`,
`rp_cs1_5_db_id_roundtrip.rs`) are **wholly new** characterization tests and are
NOT part of the provenance-equivalence set (there is no base endpoint to compare
against). The 79 modified files ARE the provenance set (pinned as
`CS1_MODIFIED_FILES`).

**Scope boundary.** This tool audits ONLY the `rust/prro/tests/*.rs` provenance set.
The CS-1 refactor ALSO changed the executed SQL statement text in **24 production
`src/db/repositories/*` read sites** (18 alias-type renames `X`→`DbX`, 6 runtime
alias removals) — these are **out of this tool's scope** and are separately verified
+ catalogued in **`docs/cs1r/PRODUCTION_SQL_DELTAS.md`** (which also carries the
`sqlx`-macro-verbatim proof that the `query!` macro does NOT strip `: Type`). Both
scopes are stated so the reader has the FULL delta surface, not just the test half.

## What the tool proves (per file, both legs)

1. **sqlx-signature equality.** For every sqlx query chain it extracts
   `{enclosing_fn, occurrence, runtime_sql_literal_bytes, ordered_normalized_bind_vector, fetch_mode}`
   and asserts the base list == head list. The **bind ORDER** is load-bearing:
   a bind swap/drop, a SQL-literal edit, or a fetch-mode change is RED here even
   if the AST compare missed it. **CS-1R2 A4:** the signature compares the **RAW**
   runtime SQL (`sql_raw` — the exact bytes SQLite executes). Removing a
   `col as "alias: Type"` alias from a RUNTIME `sqlx::query*` call DOES change the
   executed SQL bytes (this is NOT the compile-time `query!` macro), so those
   changes are NOT hidden — they are pinned in the `RUNTIME_SQL_DELTAS` catalog
   (3 sites in 2 files); any OTHER SQL edit is RED. Bind args are normalized
   (Db-wrap / `.map(Db)` / `.as_str()` stripped) so a WRAPPER change is not a
   false-positive but a VALUE change is caught. (~1047 query heads across the 79
   files.)
2. **AST token-equality outside the whitelist.** Both endpoints are parsed via
   `syn`, canonicalized by a `VisitMut` pass that undoes ONLY the whitelisted
   transforms, and compared as token streams. Any residual token divergence that
   is neither a whitelisted transform nor covered by a manual ruling → the file
   is FLAGGED and the check FAILS.

## Transform whitelist (the ONLY normalizations the tool applies)

The spec names four literal transforms; the real refactor also uses three
mechanically-equivalent ones. Per the spec, transforms **outside** the four
literal names are recorded here as **explicit manual rulings with an equivalence
argument** — they are NOT silent waivers. The tool's canonicalizer implements all
of them; the equivalence argument for each is below.

| id | transform (base → head) | in spec's literal 4? | ruling / equivalence argument |
|---|---|---|---|
| **W1** | `.bind(x)` → `.bind(DbX(x))` | ✅ `DbX(expr)→expr` | `DbX` is a `#[repr(transparent)]`-style newtype whose `Encode` forwards to the inner value's encoder; the bound bytes are identical. Guarded by the 15-name `DB_WRAPPERS` allowlist. |
| **W2** | `.bind(x)` → `.bind(x.map(DbX))` | ✅ `x.map(DbX)→x` | `Option::map(DbX)` wraps `Some` only; `None`→`NULL` unchanged; `Some(v)` encodes as `v` (W1). |
| **W3** | `query_*::<_, DbX>` decode type / `let (DbX, …)` | ✅ `DbX→X` in decode | the decode type is a compile-time hint; `DbX::decode` forwards to `X::decode` (RP-R3 pins the byte-for-byte decode). The tool DROPS the explicit turbofish for the AST compare (base used inference) and pins the runtime SQL + binds + fetch separately. |
| **W4** | `.map(\|w\| w.0)` / trailing `.0` / `.into_iter().map(\|w\| w.0).collect()` | ✅ `.map(\|w\| w.0)` | pure unwrap of the decode wrapper's inner value; no value change. Both scalar (`.0`) and Vec (`.into_iter()…collect()`) forms are the same idiom. |
| **T3** | `.bind(enum)` → `.bind(enum.as_str())` | ➕ MANUAL RULING | **Equivalence:** for a TEXT enum, `X::from_sql_str(x.as_str()) == x` for all variants (pinned by RP-CS1-5 representation conformance). The bound TEXT bytes are exactly what `DbX::encode` would have emitted. Stripped only when it is the direct argument of a `.bind(..)`. |
| **T8** | SQL ` as "col: Type"` alias removed on a RUNTIME `query_scalar` | ➕ CATALOGUED RUNTIME-SQL DELTA (A4) | **CS-1R3 A4 correction — sqlx sends the SQL string VERBATIM to SQLite for BOTH the runtime `sqlx::query*` API AND the compile-time `query!` MACRO; NEITHER strips `as "col: T"`.** (Proven via `sqlx-macros-core 0.8.6` `src/query/input.rs`+`output.rs` — `let sql=&input.sql; quote!{#sql}`, no strip — and the `.sqlx` describe cache, whose recorded column NAME is the whole `col: DbType` alias; see `docs/cs1r/PRODUCTION_SQL_DELTAS.md` §1.) So removing / renaming the alias **changes the executed SQL bytes**. This is a real, catalogued delta, NOT a hide: the fiscal RESULT + persisted representation are identical (the alias only names a read's output column), but the statement bytes changed. The test-side deltas are pinned in the code-owner-gated `docs/cs1r/pins/runtime_sql_deltas.tsv` (2 rows covering 3 test call sites); the signature compares the RAW SQL and accepts ONLY those; any other SQL edit (incl. a change to a REAL non-`:` alias) is RED. The analogous **production** `src` deltas (24 sites, 6 files) are catalogued in `docs/cs1r/PRODUCTION_SQL_DELTAS.md`. |
| **T6** | `use prro::db::types::{…}` import added | ➕ MANUAL RULING | **Equivalence:** add-only import lines (both standalone and nested inside a grouped `use prro::db::{…}`). Dropped from BOTH endpoints before the AST compare; they bring only the `Db*` wrappers into scope. |
| **T7** | use-site `.0` on a tuple-decoded id (+ removed `*` deref) | ➕ MANUAL RULING | **Equivalence:** in `live_dps_extended_smoke.rs` only, a `query_as` tuple decode flips to a `Db*` element, so the downstream consumer reads `id.0` instead of `id`. This is a value-preserving projection of the same fetched id. This file is the **single manual-ruling file** whose AST compare is allowed a residual (`manual_ruling_files()`); its sqlx signature is still fully pinned. **This is a `#![cfg(feature = "live-dps")]` harness that never runs in CI** — the delta is confined to diagnostic print/read helper call sites, not a fiscal assertion. |
| **T9** | rustfmt reflow (long line broken; trailing comma) | (formatting) | absorbed by the token compare after clearing trailing commas; not a semantic transform, so not a "ruling". |

## Classification result

| class | count | files |
|---|---|---|
| **pure whitelist** (AST reduces to token-equal via W1-W4 + T3/T6/T8/T9) | **78** | all modified files except the one below |
| **manual ruling** (documented residual: T7) | **1** | `rust/prro/tests/live_dps_extended_smoke.rs` |
| **genuine drift / non-mechanical change** | **0** | — |

**No hunk in any of the 79 files changes a fixture value, an assertion, or
control flow; and the fiscal result + persisted representation are identical.**
**The SQL statement text is NOT byte-identical** (sqlx sends `as "col: Type"`
VERBATIM to SQLite — the `query!` macro does NOT strip it; see the T8 row + §1 of
`docs/cs1r/PRODUCTION_SQL_DELTAS.md`): 3 test-side runtime `query_scalar`
column-aliases were cleaned (`col as "alias: Type"` → `col`), catalogued verbatim
in the code-owner-gated `docs/cs1r/pins/runtime_sql_deltas.tsv` (2 rows, 3 call
sites). Those aliases only named the output column of a read, so the fetched value
and the stored bytes are unchanged; only the executed statement's alias bytes
changed. This is proven by the tool: (a) the ordered sqlx bind-vector is identical
in all 79 files (no bind added/dropped/reordered); (b) after canonicalization all
78 pure-whitelist files reduce to byte-identical AST token streams; (c) the RAW
runtime SQL is byte-identical in every file EXCEPT the 3 catalogued deltas — any
un-catalogued SQL edit is RED; (d) the one manual-ruling file's only residual is
the documented T7 use-site `.0`, pinned by fingerprint, and its sqlx signature is
unchanged. **(The analogous PRODUCTION `src` SQL-text deltas — 24 sites, 6 files —
are out of this tool's scope and catalogued in
`docs/cs1r/PRODUCTION_SQL_DELTAS.md`.)**

## RED-pin RP-R1-2 (teeth — empirically verified)

Each mutation applied to a working-tree file makes `cs1_live_drift_base_vs_worktree`
RED (each reverted after). The spec names five teeth; **three were run empirically
this session** (2026-07-16, `shift_create_primitive.rs`) — the two marked *(argued)*
are covered by the same two mechanisms one of the run teeth already tripped (SQL
bytes / AST token), so re-running them adds no new signal:

| tooth | mutation run | RED how | evidence |
|---|---|---|---|
| swap two same-type `.bind` | `.bind(FN)` ↔ `.bind(state.as_str())` in the `INSERT INTO shifts` chain | sqlx bind-vector ORDER (`["id","FN","state"]` → `["id","state","FN"]`) + AST token | **run → RED** |
| change a SQL literal | `VALUES (…, 'ONLINE', …)` → `'OFLINE'` | sqlx SQL bytes + AST token | **run → RED** |
| change an assertion RHS | `assert_eq!(row.state, ShiftState::Created)` → `::Opened` | AST token | **run → RED** |
| change a fixture value | e.g. `0` → `1` in an `INSERT … VALUES` literal | sqlx SQL bytes + AST token | *(argued — same path as the SQL-literal tooth)* |
| change a control-flow condition | e.g. `if …is_some()` → `if …is_none()` | AST token | *(argued — same path as the assertion-RHS tooth)* |

The two run mechanisms (sqlx-signature SQL/bind-vector, and outside-whitelist AST
token-equality) are exactly the two surfaces the other two teeth would hit: a
fixture-value edit is a SQL-literal edit (SQL-bytes path, run) and a control-flow
edit is a non-whitelisted AST node (token path, run).

## How to run

```
cd rust
cargo test -p prro --features test-support --test cs1_test_provenance --locked
```

Green = the CS-1 refactor is provenance-equivalent at the AST + sqlx level and no
CS-1 test file has drifted in this PR beyond the whitelist.
