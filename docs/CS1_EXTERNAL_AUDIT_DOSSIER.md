# CS-1 external-audit dossier — behaviour-neutral crate-split skeleton

**Purpose.** A **self-contained** package for a decorrelated external auditor to REFUTE the load-bearing
claims of **CS-1** (the strangler/facade crate-split of the Multi-Protocol PRRO Gateway). The auditor has
no repo access — every artifact needed to attack a claim is quoted here **verbatim with `file:line`**.
Ground state: `origin/main` = `197337d`. The Rust workspace lives under `rust/`.

**What to do:** for each artifact, attack the **CLAIM** via the **ATTACK-SURFACE** hints (and any you find).
Priority target: **behaviour-neutrality** (a skeleton that changes behaviour is a failed skeleton) and
**whether the gates enforce what they claim** (a gate that is present but unsound gives false confidence —
worse than no gate).

---

## §0 · What CS-1 is + slice→commit map

CS-1 splits the monolithic `prro` crate into a **pure domain crate** + **empty contract-crate boundaries**,
keeping the legacy `prro` crate as a **facade** that re-exports moved symbols so **every legacy path still
resolves and behaviour is unchanged** (≈3023 tests stayed green). Crate-split is **by state-ownership**:

- `prro-domain` — pure fiscal value types (state enums, id newtypes, `CanonicalFiscalCommand`). MUST be free
  of `sqlx / tonic / tokio / axum / prost / hyper / reqwest` (I/O, async runtime, DB, RPC, HTTP).
- `prro-ingress-contract` / `prro-dps-contract` / `prro-fleet-contract` — **empty** boundary skeletons,
  mutually orthogonal (ingress ⊥ dps ⊥ fleet).
- `prro-testkit` — dev-only shared test scaffolding (`publish = false`), must be **absent** from every
  production dependency graph.
- `prro` (legacy) — keeps the SQLite mapping in `prro::db::types` store-side `Db*` newtype wrappers (orphan
  rule: `impl sqlx::* for prro_domain::DocState` is illegal from `prro`, so a local newtype owns it).

| Slice | Commit (on `origin/main`) | PR | What it did |
|---|---|---|---|
| docs | `f485dd3` | #286 | CS-1 contract (CONTRACT-READY) + line-item read-model note |
| CS-1a | `45d1bce` | #287 | workspace scaffolding — **empty** `prro-domain` + `prro-testkit` + CI wiring + the purity gate |
| CS-1b | `2d326a0` | #288 | move **8 TEXT-affinity enums** to `prro-domain` (behaviour-neutral) |
| CS-1b′ | `e37f333` | #290 | move **UUID-BLOB + legacy TEXT ids** to `prro-domain` (behaviour-neutral) |
| CS-1c | `a6f4a51` | #292 | move **`CanonicalFiscalCommand`** to `prro-domain` behind a facade shim |
| CS-1d | `f2628ba` | #293 | three **empty** `*-contract` crate boundaries + the **RP-CS1-4 DAG** gate |

Authoritative contract: `docs/superpowers/specs/2026-07-14-cs1-contract-behaviour-neutral-skeleton.md`.

---

## §1 · Purity gate (PRIMARY — cargo-metadata) — `rust/prro-domain/tests/purity_gate.rs`

The gate walks the **resolved** dependency graph (not `use`-sites) and asserts `prro-domain`'s
normal+build+target closure reaches **none** of a fixed forbidden set.

```rust
// rust/prro-domain/tests/purity_gate.rs
const FORBIDDEN: &[&str] = &[
    "sqlx", "tonic", "tokio", "axum", "prost", "hyper", "reqwest",
];

// ...shell out to `cargo metadata --format-version 1` (env SQLX_OFFLINE=true),
// build id→name from the `packages` table, then:
fn non_dev_closure(meta, root_name) -> HashSet<String> {
    // BFS from root over resolve.nodes[].deps, keeping an edge iff at least one
    // dep_kind is non-dev:  kind == null (normal)  OR  kind == "build".
    // (target-specific edges included; pure-dev edges excluded by design.)
}

#[test]
fn prro_domain_is_sqlx_and_io_free() {
    let closure = non_dev_closure(&meta, "prro-domain");
    let violations = FORBIDDEN.iter().filter(|f| closure.contains(*f)).collect();
    assert!(violations.is_empty(), ...);
}
```

A twinned **fast trybuild canary** (`purity_gate_compile_fail.rs` → `purity_compile_fail/forbidden_sqlx_extern.rs`)
does `extern crate sqlx;` and asserts rustc `E0463` (sqlx not reachable). The metadata test is the authority;
the trybuild is a smoke.

**Allowed deps** (`rust/prro-domain/Cargo.toml`): `uuid` (v7/v5/serde — ids keep byte-identical
`now_v7`/`new_v5`/serde), `serde` (derive), `thiserror` (pure proc-macro). Dev-deps: `serde_json`, `trybuild`.

- **CLAIM:** `prro-domain` is sqlx-free / I/O-free / runtime-free, structurally, over the resolved graph
  (catching alias / build-dep / target-specific-dep evasion a `use`-site check would miss).
- **HOW-ENFORCED:** a `#[test]` in `prro-domain`, run in CI by a **dedicated required leg** (see §6) — not
  merely a lint. Walks the real `cargo metadata` resolve graph.
- **ATTACK-SURFACE:**
  1. **`FORBIDDEN` is a 7-name DENYLIST, not an allowlist.** The gate proves "none of these 7", NOT "pure".
     A *different* I/O/DB/runtime crate — `rusqlite`, `async-std`, `smol`, `ureq`, `surf`, `native-tls`,
     `rustls`, `mio`, `hyper-util`, `h2` — would enter `prro-domain`'s closure **silently**. Purity is
     asserted only against a hand-maintained blocklist.
  2. **No `--all-features`.** `cargo metadata` is invoked with default features. An **optional** dependency
     behind a non-default feature does not appear in `resolve.nodes[].deps`, so `sqlx = { optional = true }`
     behind a feature would **evade** the walk. (Contrast the Spec #5A fleet-agent DAG gate, which explicitly
     resolves "under all features".)
  3. **Denylist name-matching only.** A forbidden crate re-published/vendored under a different package name,
     or reached via a renamed dependency (`package = "sqlx"`), is matched by resolved **package name** — the
     id→name map should catch a rename (name is authoritative), but a fork under a new name evades.

## §2 · Contract-crate DAG gate — `rust/prro-domain/tests/rp_cs1_4_contract_dag.rs`

Same walker as §1; three assertions over `["prro-ingress-contract","prro-dps-contract","prro-fleet-contract"]`:

```rust
#[test] fn contract_crates_are_workspace_members()  { /* all three ∈ workspace_members (RED when absent) */ }
#[test] fn contract_crates_are_io_free()            { /* none reaches the same FORBIDDEN set */ }
#[test] fn contract_crates_do_not_depend_on_each_other() { /* ingress ⊥ dps ⊥ fleet — no contract→contract edge */ }
```

Plus, in §1's file, **`prro_testkit_absent_from_production_graphs()`**: `prro-testkit` is a workspace member
but reachable from **no** production package's non-dev closure (dev-dep use is invisible to the walk, hence allowed).

- **CLAIM:** the three ports are empty, I/O-free, and mutually orthogonal; testkit never ships in prod.
- **HOW-ENFORCED:** same CI leg (§6).
- **ATTACK-SURFACE:** inherits §1's denylist + no-`--all-features` gaps. Orthogonality is checked as
  *reachability* (`closure.contains(other)`) — a **transitive** coupling through a future shared crate would
  be caught (good), but a coupling introduced via a **dev-dep** (e.g. an ingress integration test pulling
  `prro-dps-contract`) is invisible — acceptable now (empty crates), a latent gap once ports carry test code.

## §3 · Facade shims — `prro/src/db/models/` re-exports

The legacy paths must resolve unchanged. `prro-domain/src/lib.rs` publishes the canonical symbols
**per-symbol**:

```rust
// rust/prro-domain/src/lib.rs
pub use command::CanonicalFiscalCommand;
pub use enums::{ DocState, DocType, FiscalMode, NodeMode, OfflineSessionState, Protocol, Severity, ShiftState };
pub use ids::{ CashierId, CashierIdError, DocumentId, DriverId, DriverIdError, OfflineSessionId, OperatorId, PrinterId, RequestId, ShiftId };
```

The `prro`-side shim re-exports them **explicitly, per-symbol** (the stated discipline — NOT `pub use
prro_domain::*`):

```rust
// rust/prro/src/db/models/enums.rs  — "Explicit per-symbol facade re-exports (contract §5)."
pub use prro_domain::{
    DocState, DocType, FiscalMode, NodeMode, OfflineSessionState, Protocol, Severity, ShiftState,
};
// InboxStatus deliberately STAYS here with its own sqlx-bearing `str_enum!` derive
// (its domain-vs-store home is deferred to spec #3).
```

**BUT** the parent module still wildcards the shim:

```rust
// rust/prro/src/db/models/mod.rs   (all 4 lines)
pub mod enums;
pub mod ids;
pub use enums::*;   // <-- wildcard over the shim
pub use ids::*;     // <-- wildcard over the shim
```

- **CLAIM:** every legacy path (`prro::db::models::enums::DocState`, …) resolves unchanged; the facade is
  per-symbol so nothing is silently dropped or silently widened.
- **HOW-ENFORCED:** the Rust **compiler** — a move that dropped a still-referenced symbol fails to build.
- **ATTACK-SURFACE:**
  1. **"Not `pub use *`" is only true one level down.** `models/mod.rs` DOES `pub use enums::*` /
     `pub use ids::*`. So `prro::db::models::DocState` is a wildcard re-export of the shim. The per-symbol
     discipline holds at the `prro_domain → shim` boundary but NOT at `shim → models`. Consequence: the
     re-export **set** at `prro::db::models::*` is whatever the shim happens to expose, not a pinned list.
  2. **Facade completeness is NOT machine-checked.** No test asserts "shim re-export set == `prro-domain`
     public set". For the CS-1 *moves*, the compiler catches a dropped symbol (existing consumers reference
     it). But a **newly added** `prro-domain` symbol that is forgotten in the shim is invisible until a new
     consumer needs the legacy path — a latent regression vector for CS-2+.
  3. **Two `str_enum!` macros now exist** — a pure one in `prro-domain/src/enums.rs` and a sqlx-bearing one in
     `prro/src/db/models/enums.rs` (for `InboxStatus`). They can drift (derive set, attributes) with no test
     tying them together.

## §4 · `Db*` newtype byte-identity — `prro/src/db/types.rs` (250 lines)

The pure enums are sqlx-free; the SQLite mapping is a `prro`-local newtype (orphan rule):

```rust
// rust/prro/src/db/types.rs
macro_rules! db_text_enum {
    ($wrapper:ident, $inner:ty) => {
        pub struct $wrapper(pub $inner);
        impl From<$inner> for $wrapper { fn from(v) -> Self { Self(v) } }
        impl From<$wrapper> for $inner { fn from(w) -> Self { w.0 } }
        impl Type<Sqlite>   for $wrapper { /* delegates to <str as Type<Sqlite>> — TEXT */ }
        impl Encode<Sqlite> for $wrapper {
            fn encode_by_ref(&self, buf) -> ... {
                buf.push(SqliteArgumentValue::Text(Cow::Borrowed(self.0.as_str())));  // byte-identical literal
                Ok(IsNull::No)
            }
        }
        impl Decode<Sqlite> for $wrapper {
            fn decode(value) -> ... {
                let s = <String as Decode<Sqlite>>::decode(value)?;
                match <$inner>::from_sql_str(&s) { Some(v) => Ok(Self(v)),
                    None => /* closed set: unknown literal is a HARD decode error */ }
            }
        }
    };
}
db_text_enum!(DbDocState, DocState);          db_text_enum!(DbOfflineSessionState, OfflineSessionState);
db_text_enum!(DbShiftState, ShiftState);      db_text_enum!(DbNodeMode, NodeMode);
db_text_enum!(DbProtocol, Protocol);          db_text_enum!(DbDocType, DocType);
db_text_enum!(DbFiscalMode, FiscalMode);      db_text_enum!(DbSeverity, Severity);
// + db_blob_id! for the UUID ids: Type = Vec<u8> (BLOB), Encode = 16 raw bytes, Decode = Vec<u8> → [u8;16]
//   (length ≠ 16 ⇒ decode error) — "byte-identical to the pre-move id_newtype! sqlx impls".
```

The pure enum's macro (in `prro-domain/src/enums.rs`) is the pre-move `str_enum!` **minus** `sqlx::Type` /
`#[sqlx(rename)]`, **keeping** the derive set + per-variant `#[serde(rename = $sql)]` + `as_str()`, and
**adding** `from_sql_str(&str) -> Option<Self>` (exact-literal match; unknown ⇒ `None`).

- **CLAIM:** encode/decode bytes are **byte-identical** to the pre-split `#[sqlx(rename=…)]` mapping; struct
  field types stay the pure domain enum (conversions only at the repository boundary).
- **HOW-ENFORCED:** `Encode` pushes `as_str()` (same literal as `#[sqlx(rename)]`); `Decode` matches the same
  closed literal set; RP-CS1-5 asserts byte-identical `serde_json::to_string`.
- **ATTACK-SURFACE:**
  1. **Decode error behaviour changed shape.** Pre-move, an unknown TEXT decoded via sqlx's derive; now via
     `from_sql_str` → a `prro`-constructed decode error. Both error, but if any code path **matches on the
     decode-error message/type**, behaviour drifted. (Unknown literals shouldn't occur, but the skeptic
     should confirm no error-string coupling.)
  2. **Every bind/query site must now wrap in `DbX`.** The compiler enforces this (the old sqlx impl is gone),
     EXCEPT sites using **runtime `sqlx::query()` with manual `.bind(x.as_str())`** or raw SQL strings — those
     bypass the wrapper and were unaffected by the move, but are also unverified by RP-CS1-5. Confirm no
     `query_as!`-decoded struct field silently relies on the old derive.
  3. **`Type::compatible`** now delegates to `<str as Type<Sqlite>>::compatible` — confirm this equals the
     pre-move column-type acceptance (TEXT affinity) exactly, incl. any `NULL`/`Option<Enum>` column.
  4. **BLOB ids**: `Decode` does `Vec<u8> → [u8;16]`. Confirm the pre-move impl rejected length≠16 identically
     (no silent truncation/pad change).

## §5 · Moved TEXT enum fidelity — `prro-domain/src/enums.rs` (`DocState` shown)

```rust
// rust/prro-domain/src/enums.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]  // (pre-move set MINUS sqlx::Type)
pub enum DocState { #[serde(rename="PREPARED")] Prepared, #[serde(rename="SIGNED")] Signed,
    #[serde(rename="ENCRYPTED")] Encrypted, /* …Sending intent-marker, etc… */ }
impl DocState { pub fn as_str(&self) -> &'static str { /* same literal */ }
    pub fn from_sql_str(s:&str) -> Option<Self> { /* exact match; unknown ⇒ None */ } }
```

- **CLAIM:** variant string representation (serde + stored TEXT) is byte-identical to pre-move.
- **HOW-ENFORCED:** `#[serde(rename=$sql)]` preserved verbatim; `as_str()` returns the same literal; RP-CS1-5.
- **ATTACK-SURFACE:** the derive set dropped `sqlx::Type` and **the variant ORDER** is the source of `Hash`
  and `PartialEq` structural identity — confirm no variant was reordered/renamed during the move (a reorder
  is invisible to serde output but changes `Hash` values, which matters only if a `HashMap<DocState,_>` is
  persisted/compared across process boundaries — it isn't, but confirm). Also confirm **no `Ord`/`PartialOrd`
  was silently added or removed** (neither pre nor post has it; a sort relying on it would not compile).

## §6 · Behaviour-neutrality evidence (necessary ≠ sufficient)

- **CI enforcement is real and required.** `.github/workflows/rust-prro.yml` has a **dedicated leg**
  (line ~149): *"Test prro-domain (RP-CS1-1 purity gate + RP-CS1-4 contract-DAG + testkit graph-absence)"* →
  `cargo nextest run -p prro-domain -p prro-testkit -p prro-ingress-contract -p prro-dps-contract
  -p prro-fleet-contract --locked`. The main leg runs `cargo nextest run -p prro --features test-support`
  (≈the full 3023-test suite). `pull_request` is **unfiltered** so the required check reports on every PR.
- **What "green" does NOT prove** (the skeptic's list): (a) any behaviour exercised only by code paths without
  a test; (b) byte-identity of **runtime** `sqlx::query()` bind sites not covered by RP-CS1-5; (c) decode-error
  **shape** on malformed DB rows (no test feeds an unknown literal at every call site); (d) the denylist gaps
  in §1 (a new I/O crate is not a test failure — it's simply not on the list); (e) facade **completeness**
  for symbols with no current consumer (§3.2).
- **Storage non-event is the crux:** the entire behaviour-neutrality argument reduces to *"the bytes written
  to / read from SQLite, and the serde output, are identical"*. §4–§5 are where to attack it.

---

## §7 · Sharpest questions for the auditor (numbered — refute these)

1. **Denylist vs purity (§1.1):** the purity gate proves absence of **7 named crates**, not purity. Is a
   denylist an acceptable enforcement of "sqlx-free / I/O-free / runtime-free", or must it be an **allowlist**
   (root deps ∈ {uuid, serde, thiserror, + their vetted transitive closure})? Name the I/O/DB/runtime crate
   most likely to slip in un-listed.
2. **Optional-feature evasion (§1.2):** `cargo metadata` runs **without `--all-features`**. Construct the case
   where a forbidden crate behind an optional feature is invisible to the walk. Does this defeat the gate's
   "structural, over the resolved graph" claim? Should it mirror Spec #5A's "under all features"?
3. **Facade wildcard (§3.1):** `models/mod.rs` does `pub use enums::*` / `pub use ids::*`. Does the "explicit
   per-symbol, never `pub use *`" discipline actually hold end-to-end, or only at the `prro_domain → shim`
   hop? Does the wildcard at the outer layer create a silent re-export-set drift risk?
4. **Facade completeness (§3.2):** nothing machine-checks that the shim re-exports the **full** `prro-domain`
   public set. For CS-1's moves the compiler catches drops; for CS-2+ **additions** it does not. Is a
   "re-export set == public set" test warranted now, before more symbols move?
5. **Decode-error shape (§4.1):** the move changed an unknown-literal decode from sqlx's derive error to a
   `from_sql_str`-`None` error. Is byte-identity of the *happy path* sufficient for "behaviour-neutral", or
   does the changed **error** path count as a behaviour change that must be proven inert?
6. **Runtime bind sites (§4.2):** RP-CS1-5 asserts serde byte-identity, but does it (or any test) cover every
   **`sqlx::query()` runtime bind** and **`query_as!` decode** site for the 8 enums + ids? If not, what's the
   residual byte-identity risk?
7. **`Type::compatible` equivalence (§4.3):** delegating to `<str as Type<Sqlite>>::compatible` — is column
   acceptance (incl. `Option<Enum>`/NULL and any non-TEXT-affinity column that previously worked) provably
   identical to the pre-move derive?
8. **Two `str_enum!` macros (§3.3 / §4):** a pure one (domain) and a sqlx one (`prro`, for `InboxStatus`) now
   coexist. Is the duplication a latent drift hazard worth collapsing, or correctly deferred to spec #3 / CS-7?
9. **BLOB id length rule (§4.4):** confirm `Vec<u8> → [u8;16]` decode rejects length≠16 identically to the
   pre-move `id_newtype!` impl (no silent truncation).
10. **Scope honesty:** is anything claimed "behaviour-neutral" that actually *changed* — even beneficially
    (e.g. a stricter decode, a new `from_sql_str`)? A skeleton must be **inert**; flag any smuggled behaviour.

---
*Assembled from `origin/main` `197337d` via `git show`. Every quote is `file:line`-grounded; verbatim blocks
are lightly elided (marked `…`/`/* */`) for focus — full files are in the named paths.*
