//! CS-1R R1.1 — per-hunk provenance audit engine (syn-based).
//!
//! Spec `docs/superpowers/specs/2026-07-15-cs1r-remediation-spec.md` §4 R1.1.
//!
//! This module is the MACHINE mechanism behind the CS-1 test-provenance proof.
//! It parses BOTH endpoints of each CS-1 modified test file via `syn`, applies a
//! `VisitMut` canonicalization that undoes ONLY the whitelisted `Db*`-wrapper
//! transforms, and compares the resulting token streams. It ALSO extracts a
//! signature for every sqlx query chain (SQL literal bytes, ORDERED normalized
//! bind vector, fetch mode) so a bind swap / SQL edit / fetch-mode change is
//! caught even if it survived the AST compare.
//!
//! ## Base / head SHAs (pinned)
//! - base = `f2c17b1` (pre-CS-1)
//! - head = `f2628ba` (CS-1 completion — the immutable provenance endpoint)
//!
//! The teeth leg (`cs1_test_provenance.rs`) additionally runs base-vs-WORKING-TREE
//! so an edit to a live file in this PR is caught RED.
//!
//! ## Whitelisted transforms (spec §4, the ONLY normalizations allowed)
//! - **W1** `DbX(expr) -> expr`               (bind wrap)
//! - **W2** `x.map(DbX) -> x`                  (Option-id bind map)
//! - **W3** `DbX -> X` in a decode-type position (`query_scalar/query_as` generic)
//! - **W4** the `.map(|w| w.0)` / trailing `.0` service unwrap
//!
//! Two MECHANICALLY-EQUIVALENT transforms are used by the real refactor beyond
//! the four literal names; they are normalized here AND recorded as explicit
//! **manual rulings** (with an equivalence argument) in the committed artifact —
//! NOT silent waivers:
//! - **T3** `.bind(enum) -> .bind(enum.as_str())` (enum bind — the `as_str`
//!   projection is the value-identity of a TEXT enum: `X::from_sql_str(x.as_str()) == x`)
//! - **T8** SQL `as "col: Type"` decode-annotation removal / rename on a runtime
//!   `query_scalar`/`query_as`. CS-1R4: classified as a **fiscal-neutral transform
//!   class**, like W1-W4/T3 — NOT a byte-identity claim and NOT a catalogued
//!   diff-set. The refactor **changed the executed SQL statement text** (the
//!   `as "col: Type"` clause only names a READ's output column + its Rust decode
//!   type, so a **fiscal change is NOT intended**); **SQL completeness / identity is
//!   NOT asserted** by this tool — fiscal-neutrality is covered by the behavioral
//!   fiscal test suite (RP-CS1-5 + the full `prro` suite). The sqlx signature
//!   therefore compares the DECODE-ANNOTATION-STRIPPED SQL (`sql`): the fiscal-
//!   neutral annotation removal normalizes to equal, while ANY other SQL edit (a
//!   table, a WHERE clause, a literal VALUE, a REAL non-`:` alias) still diverges
//!   and is RED. Binds (order+value), fetch mode, and decode type are pinned
//!   separately. There is NO SQL-byte-identity assertion and NO diff↔set machinery
//!   (round-4 auditor: "don't add SQL machinery").
//! - **T6** `use prro::db::types::{...}` import additions (add-only import lines;
//!   dropped before compare)
//! - **T7** use-site `.0` on a tuple-decoded id (`live_dps_extended_smoke.rs`
//!   only — recorded as a manual ruling; the sqlx signature still pins the
//!   fetch)
//!
//! Any residual token divergence NOT in this set → the file is FLAGGED. A flagged
//! file must appear in the artifact with an explicit ruling or the check FAILS.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::visit_mut::VisitMut;
use syn::{Expr, File, Type};

/// base = pre-CS-1 (immutable).
pub const BASE_SHA: &str = "f2c17b1";
/// head = CS-1 completion (immutable provenance endpoint).
pub const HEAD_SHA: &str = "f2628ba";
/// CS-3 S7-1 cutover re-baseline: the LIVE-drift leg's base/head endpoint. The immutable
/// provenance leg (`BASE_SHA`->`HEAD_SHA`) is UNTOUCHED — the CS-1 refactor neutrality proof
/// stays frozen at `f2c17b1..f2628ba` (historical). The cutover is an explicit, adjudicated
/// contract change to the frozen test files (HELD/RMR/escalate), so ONLY the live-drift leg is
/// re-anchored to the cutover commit: `git_show(LIVE_DRIFT_BASE_SHA, file) == worktree`, and it
/// still fails RED on any FURTHER non-neutral drift vs this base. No carve-out, no weakening of
/// the immutable proof. Re-anchor again (bump this) at the next adjudicated frozen-file change.
pub const LIVE_DRIFT_BASE_SHA: &str = "9ec0b41";

/// The 15 `Db*` wrapper newtype identifiers introduced by CS-1 (`db/types.rs`).
/// Used to recognize W1/W2/W3 nodes; NO other single-segment `Db…` ctor is
/// unwrapped, so a hand-written `DbFoo(x)` an author might add is NOT silently
/// eaten.
pub const DB_WRAPPERS: &[&str] = &[
    "DbDocState",
    "DbOfflineSessionState",
    "DbShiftState",
    "DbNodeMode",
    "DbProtocol",
    "DbDocType",
    "DbFiscalMode",
    "DbSeverity",
    "DbDocumentId",
    "DbRequestId",
    "DbShiftId",
    "DbOperatorId",
    "DbPrinterId",
    "DbOfflineSessionId",
    "DbCashierId",
];

fn is_db_wrapper(seg: &str) -> bool {
    DB_WRAPPERS.contains(&seg)
}

/// Strip the leading `Db` from a wrapper ident, giving the domain type (`W3`).
fn strip_db_prefix(ident: &str) -> Option<String> {
    if is_db_wrapper(ident) {
        Some(ident.trim_start_matches("Db").to_string())
    } else {
        None
    }
}

/// Clear a trailing comma from a `Punctuated` argument list so `f(a,)` and
/// `f(a)` render identically (T9 rustfmt reflow).
fn strip_trailing_comma(args: &mut syn::punctuated::Punctuated<Expr, syn::token::Comma>) {
    if args.trailing_punct() {
        let collected: Vec<Expr> = args.iter().cloned().collect();
        *args = collected.into_iter().collect();
    }
}

/// Return the single trailing path segment of an expression path, if the whole
/// expression is a bare/qualified path (e.g. `DbShiftId`, `prro::…::DbShiftId`).
fn path_last_seg(expr: &Expr) -> Option<String> {
    if let Expr::Path(p) = expr {
        p.path.segments.last().map(|s| s.ident.to_string())
    } else {
        None
    }
}

/// `VisitMut` pass that canonicalizes the HEAD-shape (`Db*`-wrapped) AST back to
/// the BASE shape by undoing the whitelisted transforms. Applied to BOTH
/// endpoints (idempotent on the base, which has nothing to undo), so the two
/// canonical forms are compared on equal footing.
pub struct Canonicalizer;

impl VisitMut for Canonicalizer {
    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        // Recurse first so inner nodes are already canonical.
        syn::visit_mut::visit_expr_mut(self, expr);

        // T9: absorb rustfmt reflow — a long `query_*(…)` / `.bind(…)` that got
        // broken across lines gains a TRAILING COMMA in its arg list. `syn`
        // preserves trailing `Punctuated` punctuation in `to_token_stream()`, so
        // clear it on every call/method-call so `f(a,)` == `f(a)`.
        match expr {
            Expr::Call(call) => strip_trailing_comma(&mut call.args),
            Expr::MethodCall(mc) => strip_trailing_comma(&mut mc.args),
            _ => {}
        }

        // W1: `DbX(inner)` -> `inner`  (single-arg tuple-struct ctor call).
        if let Expr::Call(call) = expr {
            if call.args.len() == 1 {
                if let Some(seg) = path_last_seg(&call.func) {
                    if is_db_wrapper(&seg) {
                        let inner = call.args.first().unwrap().clone();
                        *expr = inner;
                        return;
                    }
                }
            }
        }

        // T4a/T4b/T4c decode-type turbofish: the CS-1 head made the decode type
        // EXPLICIT (`query_scalar::<_, DbDocState>(…)`) where the base relied on
        // inference (`query_scalar(…)`). The decode type is a compile-time hint
        // (it does NOT change the runtime SQL), and the sqlx-signature extractor
        // independently pins SQL + binds + fetch-mode. So for the AST compare we
        // DROP the explicit turbofish generics on a `query*` head entirely — this
        // reconciles "base has no turbofish" with "head has `::<_, DbX>`".
        if let Expr::Call(call) = expr {
            if let Expr::Path(p) = &mut *call.func {
                if let Some(last) = p.path.segments.last_mut() {
                    if last.ident.to_string().starts_with("query") {
                        if matches!(last.arguments, syn::PathArguments::AngleBracketed(_)) {
                            last.arguments = syn::PathArguments::None;
                        }
                        // T8 + delimiter/format canonicalization: strip the
                        // ` as "alias: Type"` sqlx decode annotation from the SQL
                        // string-literal args of a query head, AND re-mint every
                        // query-head string literal as a canonical PLAIN `LitStr`
                        // of its stripped value. Re-minting normalizes the string
                        // DELIMITER too (`r#"…"#` vs `"…"`) — CS-1 flipped a base
                        // raw-string to plain once the embedded `"alias: Type"`
                        // quotes were removed, so base and head must compare on the
                        // string VALUE, not its literal spelling.
                        for arg in call.args.iter_mut() {
                            if let Expr::Lit(l) = arg {
                                if let syn::Lit::Str(s) = &l.lit {
                                    let stripped = strip_sqlx_decode_annotations(&s.value());
                                    l.lit = syn::Lit::Str(syn::LitStr::new(&stripped, s.span()));
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Expr::MethodCall(mc) = expr {
            let method = mc.method.to_string();

            // W2: `recv.map(DbX)` -> `recv`  (arg is a bare path to a wrapper).
            if method == "map" && mc.args.len() == 1 {
                if let Some(seg) = path_last_seg(mc.args.first().unwrap()) {
                    if is_db_wrapper(&seg) {
                        *expr = (*mc.receiver).clone();
                        return;
                    }
                }
            }

            // W4 / T5b: `recv.map(|w| w.0)` and tuple-destructure unwraps
            // `recv.map(|(a, b)| (a.0, b.0))` -> `recv`. Recognized structurally:
            // a single closure arg whose body only re-packages `.0` projections
            // of the closure's own bindings.
            if method == "map" && mc.args.len() == 1 {
                if let Expr::Closure(cl) = mc.args.first().unwrap() {
                    if closure_is_dot0_unwrap(cl) {
                        *expr = (*mc.receiver).clone();
                        return;
                    }
                }
            }

            // W4 (Vec form): the collection-level `Db*`-tuple unwrap idiom
            // `fetch_all().await.unwrap().into_iter().map(|w| w.0).collect()`. By
            // the time we reach `.collect()`, the inner `.map(dot0)` has ALREADY
            // been stripped to its receiver (`<tail>.into_iter()`), so we strip
            // `X.collect()` when `X` is `<fetch-tail>.into_iter()`. Scoped to a
            // fetch tail so an unrelated `.into_iter().collect()` is untouched.
            if method == "collect" && mc.args.is_empty() {
                if let Expr::MethodCall(inner) = &*mc.receiver {
                    if inner.method == "into_iter"
                        && inner.args.is_empty()
                        && base_is_fetch_tail(&inner.receiver)
                    {
                        *expr = (*inner.receiver).clone();
                        return;
                    }
                }
            }

            // T3: `.bind(enum.as_str())` -> `.bind(enum)`. Strip a trailing
            // `.as_str()` ONLY when it is the direct argument of a `.bind(..)`
            // call — never an unrelated `.as_str()`.
            if method == "bind" && mc.args.len() == 1 {
                if let Expr::MethodCall(arg) = mc.args.first().unwrap() {
                    if arg.method == "as_str" && arg.args.is_empty() {
                        let inner = (*arg.receiver).clone();
                        *mc.args.first_mut().unwrap() = inner;
                    }
                }
            }
        }

        // W4 / T5a: a trailing `.0` on a fetch chain: `<chain>.unwrap().0`,
        // `<chain>.await.0`, `<chain>.expect(..).0`. The head added `.0` to
        // unwrap the `Db*` decode wrapper the (now-dropped) turbofish produced.
        // Strip a `.0` field access whose base is an `.unwrap()/.expect()/.await`
        // tail — scoped so a legitimate `tuple.0` elsewhere is untouched.
        if let Expr::Field(f) = expr {
            if matches!(&f.member, syn::Member::Unnamed(idx) if idx.index == 0)
                && base_is_fetch_tail(&f.base)
            {
                let inner = (*f.base).clone();
                *expr = inner;
            }
        }
    }

    fn visit_type_mut(&mut self, ty: &mut Type) {
        syn::visit_mut::visit_type_mut(self, ty);
        // W3: a decode-type position `DbX` -> `X`. Rewrites the `let x: (DbX, ..)`
        // annotations (T4c) and any residual `Db*` type path. The turbofish on
        // `query*` heads is dropped wholesale in `visit_expr_mut` above, so this
        // mainly serves the `let`-binding annotations. Guarded by the wrapper
        // allowlist so only the 15 known decode wrappers are touched.
        if let Type::Path(tp) = ty {
            if let Some(last) = tp.path.segments.last_mut() {
                let id = last.ident.to_string();
                if let Some(inner) = strip_db_prefix(&id) {
                    last.ident = syn::Ident::new(&inner, last.ident.span());
                }
            }
        }
    }
}

/// True if `e` is a fetch-chain tail we expect a `.0` unwrap to sit on:
/// `.unwrap()`, `.expect(..)`, or `.await` (or a `?`-operator wrap thereof).
fn base_is_fetch_tail(e: &Expr) -> bool {
    match e {
        Expr::MethodCall(mc) => {
            let m = mc.method.to_string();
            m == "unwrap" || m == "expect"
        }
        Expr::Await(_) => true,
        Expr::Try(t) => base_is_fetch_tail(&t.expr),
        _ => false,
    }
}

/// Recognize a closure that is purely a `.0` unwrap:
///   `|w| w.0`, `|(a, b)| (a.0, b.0)`, `|(a, b)| (a, b.0)` etc.
/// i.e. the body is the closure's binding(s) with `.0` optionally applied and no
/// other operation. Conservative: any other body shape returns false.
fn closure_is_dot0_unwrap(cl: &syn::ExprClosure) -> bool {
    fn expr_is_binding_or_dot0(e: &Expr) -> bool {
        match e {
            // `w` or `a`
            Expr::Path(p) => p.path.segments.len() == 1,
            // `w.0` — a field access with a numeric `.0` member on a bare ident.
            Expr::Field(f) => {
                matches!(&f.member, syn::Member::Unnamed(idx) if idx.index == 0)
                    && matches!(&*f.base, Expr::Path(p) if p.path.segments.len() == 1)
            }
            _ => false,
        }
    }
    match &*cl.body {
        Expr::Field(_) | Expr::Path(_) => expr_is_binding_or_dot0(&cl.body),
        Expr::Tuple(t) => !t.elems.is_empty() && t.elems.iter().all(expr_is_binding_or_dot0),
        _ => false,
    }
}

/// Load a file's text from a git blob (`git show <sha>:<path>`), from the
/// worktree at `root`.
pub fn git_show(root: &PathBuf, sha: &str, repo_rel_path: &str) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("show")
        .arg(format!("{sha}:{repo_rel_path}"))
        .output()
        .unwrap_or_else(|e| panic!("git show {sha}:{repo_rel_path} failed to spawn: {e}"));
    assert!(
        out.status.success(),
        "git show {sha}:{repo_rel_path} exited non-zero: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("git blob not utf-8")
}

/// CS-1R2 A2b — the git blob SHA of a worktree file (`git hash-object <path>`).
/// Compared against the pinned approved SHA so a mutation to a carved-out file
/// (which changes its blob) is RED.
pub fn git_hash_object(root: &PathBuf, abs_path: &std::path::Path) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["hash-object", "--"])
        .arg(abs_path)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "git hash-object {} failed to spawn: {e}",
                abs_path.display()
            )
        });
    assert!(
        out.status.success(),
        "git hash-object {} exited non-zero: {}",
        abs_path.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .expect("git hash-object not utf-8")
        .trim()
        .to_string()
}

/// Repo root = the worktree containing this crate. `CARGO_MANIFEST_DIR` is
/// `<root>/rust/prro`; the repo root is two levels up.
pub fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root two levels above CARGO_MANIFEST_DIR")
        .to_path_buf()
}

/// Parse source text, drop `use prro::db::types::{...}` import items (T6) and any
/// nested `types::Db*` leaf inside a grouped `use prro::db::{...}`, then run the
/// canonicalization passes, and return the normalized token stream.
pub fn normalize_source(src: &str) -> Result<TokenStream, String> {
    let mut file: File = syn::parse_file(src).map_err(|e| format!("parse error: {e}"))?;
    drop_db_types_imports(&mut file);
    Canonicalizer.visit_file_mut(&mut file);
    Ok(file.to_token_stream())
}

/// CS-1R2 A2a — the EXACT residual fingerprint of a manual-ruling file.
///
/// A manual-ruling file (only `live_dps_extended_smoke.rs`) is allowed to differ
/// from its base after canonicalization ONLY by the documented T7 use-site `.0`
/// residual. The old gate accepted ANY residual for such a file
/// (`else if manual.contains(path) { ok }`), so an assertion / logic / SQL change
/// smuggled into that file passed silently. Instead we pin the EXACT residual:
/// a stable fingerprint of the (base-normalized, head-normalized) token pair.
/// The documented T7 `.0` residual reproduces this fingerprint; ANY other change
/// (assertion RHS flip, control-flow edit, added statement) perturbs the head
/// token stream → the fingerprint changes → the file is FLAGGED. Deterministic
/// (syn `to_token_stream` is stable), so it is safe to pin as a constant.
pub fn manual_residual_fingerprint(base_norm: &str, head_norm: &str) -> String {
    // A cheap, dependency-free 128-bit FNV-1a fold over both normalized streams,
    // rendered hex. (No external hash crate in this dev-test module.)
    fn fnv1a_128(seed: u128, bytes: &[u8]) -> u128 {
        const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013B;
        let mut h = seed;
        for &b in bytes {
            h ^= b as u128;
            h = h.wrapping_mul(PRIME);
        }
        h
    }
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    let h = fnv1a_128(OFFSET, base_norm.as_bytes());
    // domain-separate the two streams with a NUL so a byte shift across the
    // boundary cannot collide.
    let h = fnv1a_128(h, b"\0");
    let h = fnv1a_128(h, head_norm.as_bytes());
    format!("{h:032x}")
}

/// Remove `use prro::db::types::…;` items and strip a nested `types::Db*` leaf
/// from a grouped `use prro::db::{…}` (T6). This is add-only in HEAD, so
/// removing it from BOTH endpoints keeps them comparable (base has none).
fn drop_db_types_imports(file: &mut File) {
    use syn::{Item, UseTree};

    fn tree_is_db_types(tree: &UseTree) -> bool {
        // matches `prro::db::types` prefix (then a group/name/glob of Db* wrappers)
        fn walk(tree: &UseTree, depth: usize, path: &[&str]) -> bool {
            let expect = ["prro", "db", "types"];
            match tree {
                UseTree::Path(p) => {
                    let seg = p.ident.to_string();
                    if depth < expect.len() && seg == expect[depth] {
                        // continue descending along the prro::db::types spine
                        let mut p2: Vec<&str> = path.to_vec();
                        p2.push(expect[depth]);
                        walk(&p.tree, depth + 1, &p2)
                    } else {
                        false
                    }
                }
                // reached `types::{...}` or `types::DbX` — this is a db::types use
                _ => depth == expect.len(),
            }
        }
        walk(tree, 0, &[])
    }

    // Strip a `types::…` leaf out of a grouped `use prro::db::{ …, types::DbX }`.
    fn prune_grouped_db(tree: &mut UseTree) {
        if let UseTree::Path(p) = tree {
            if p.ident == "prro" {
                if let UseTree::Path(db) = &mut *p.tree {
                    if db.ident == "db" {
                        if let UseTree::Group(g) = &mut *db.tree {
                            g.items = g
                                .items
                                .iter()
                                .filter(
                                    |it| !matches!(it, UseTree::Path(tp) if tp.ident == "types"),
                                )
                                .cloned()
                                .collect();
                        }
                    }
                }
            }
        }
    }

    file.items.retain(|item| {
        if let Item::Use(u) = item {
            !tree_is_db_types(&u.tree)
        } else {
            true
        }
    });
    for item in file.items.iter_mut() {
        if let Item::Use(u) = item {
            prune_grouped_db(&mut u.tree);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// sqlx query-chain signature extractor
// ─────────────────────────────────────────────────────────────────────────────

/// Fetch/execute terminator of a sqlx chain.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum FetchMode {
    Execute,
    FetchOne,
    FetchOptional,
    FetchAll,
}

/// A canonical signature for one sqlx query chain. The `enclosing_fn` +
/// `occurrence` disambiguate multiple chains in the same fn; the SQL bytes and
/// the ORDERED normalized bind vector are the load-bearing equality surface.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SqlxSig {
    pub enclosing_fn: String,
    pub occurrence: usize,
    /// Runtime SQL literal bytes (concatenated string-literal args), with the
    /// `as "col: Type"` sqlx DECODE ANNOTATION stripped (T8 — a fiscal-neutral
    /// transform class, NOT a byte-identity claim). This is the SQL-comparison
    /// surface: the fiscal-neutral decode-annotation removal normalizes to equal,
    /// while any OTHER SQL edit (a table, WHERE clause, literal VALUE, or a REAL
    /// non-`:` alias) still diverges → RED. CS-1R4 removed the earlier `sql_raw`
    /// field + the catalogued-delta escape hatch: this tool asserts NO SQL-byte
    /// identity and carries NO diff↔set machinery.
    pub sql: String,
    /// Kind: `query`, `query_scalar`, `query_as`, `query_scalar_with`, …
    pub query_kind: String,
    /// CS-1R2 A2c — the decode type pinned from the query-head turbofish, with
    /// each `Db*` wrapper mapped to its domain type (`DbShiftState → ShiftState`)
    /// so `query_scalar(SQL)` (base, inferred) and
    /// `query_scalar::<_, DbShiftState>(SQL)` (head, explicit) compare EQUAL —
    /// but a change to a DIFFERENT decode type (`DbShiftState → DbDocState`, or a
    /// tuple-arity change) is caught. Empty when the head carried no turbofish at
    /// EITHER endpoint. The old gate blanket-stripped the whole turbofish, so the
    /// decode type was pinned NOWHERE.
    pub decode_type: String,
    /// Ordered normalized bind expressions (Db-wrap / `.map(Db)` / `.as_str()`
    /// stripped) — a bind SWAP or drop changes this vector.
    pub binds: Vec<String>,
    pub fetch_mode: FetchMode,
}

/// CS-1R3 A2 — pin DATA files (loaded, not inlined next to the oracle).
///
/// The pin CONSTANTS (the manual-residual fingerprint, the carve-out blob SHAs)
/// live in committed DATA files under `docs/cs1r/pins/`, code-owner-gated by
/// `.github/CODEOWNERS`. A pin change is a visible data diff distinct from the gate
/// LOGIC — closing the A2 finding that a pin sitting next to its checker is
/// self-rewritable (one PR edits an assertion + its pin → GREEN). The loaders below
/// read those files at test time (the tests already read committed blobs / worktree
/// files, so file I/O is in-scope).
///
/// **CS-1R4 pin-loader hardening (auditor finding-7).** These are the ONLY two pin
/// files, and each name is a COMPILE-TIME constant joined to `repo_root()` by
/// [`pin_path`] — there is NO directory scan, NO glob, NO env-var override, and NO
/// per-directory discovery. A second pin file placed ANYWHERE else in the tree is
/// simply never read (it is not on this fixed list), so it cannot shadow or override
/// the canonical pin. `pin_path` additionally REJECTS any name that is not exactly
/// one of these two canonical basenames under `docs/cs1r/pins/`, so a caller cannot
/// be tricked into reading an off-canon path. (Canary: `pin_loader_reads_only_canonical_path`
/// in `cs1_test_provenance.rs` places a spoofed pin outside the canonical dir and
/// proves the loader still returns the canonical value.)
///
/// NOTE (CS-1R4): the runtime-SQL delta catalog (`runtime_sql_deltas` /
/// `RUNTIME_SQL_DELTAS_FILE` / `is_catalogued_sql_delta`) was DELETED. This tool no
/// longer asserts SQL-byte identity, so there is no diff↔set to load.
pub const MANUAL_RESIDUAL_FINGERPRINT_FILE: &str = "docs/cs1r/pins/manual_residual_fingerprint.txt";
pub const POST_CS1_CARVEOUT_FILE: &str = "docs/cs1r/pins/post_cs1_carveout.tsv";

/// The one canonical pin directory. Every pin path MUST be exactly
/// `<CANONICAL_PIN_DIR>/<basename>` for one of the two allowed basenames — enforced
/// by [`pin_path`] so a pin can only ever be read from this location.
pub const CANONICAL_PIN_DIR: &str = "docs/cs1r/pins";

/// CS-1R4 (auditor finding-7) — resolve a pin file to its ONE canonical absolute
/// path, REJECTING any argument that is not exactly one of the two allow-listed
/// canonical pin files under `docs/cs1r/pins/`. This is the single choke point: a
/// caller cannot ask for an off-canon path, an absolute path, a `..` escape, or a
/// different directory — a second pin placed elsewhere can never be honored.
fn pin_path(file_rel: &str) -> PathBuf {
    assert!(
        file_rel == MANUAL_RESIDUAL_FINGERPRINT_FILE || file_rel == POST_CS1_CARVEOUT_FILE,
        "pin loader refused a non-canonical pin path {file_rel:?}: the ONLY pin files are \
         {MANUAL_RESIDUAL_FINGERPRINT_FILE:?} and {POST_CS1_CARVEOUT_FILE:?} under \
         {CANONICAL_PIN_DIR:?}. A pin placed anywhere else is not read (finding-7)."
    );
    // Belt-and-braces: the canonical file MUST live directly under the canonical dir.
    assert_eq!(
        std::path::Path::new(file_rel)
            .parent()
            .and_then(|p| p.to_str()),
        Some(CANONICAL_PIN_DIR),
        "pin file {file_rel:?} is not directly under the canonical pin dir {CANONICAL_PIN_DIR:?}",
    );
    repo_root().join(file_rel)
}

/// Read a canonical pin data file's meaningful lines (drop `#` comments + blank
/// lines). The path is resolved ONLY via [`pin_path`], so nothing off-canon loads.
fn read_pin_lines(file_rel: &str) -> Vec<String> {
    let path = pin_path(file_rel);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read pin file {}: {e}", path.display()));
    text.lines()
        .map(str::trim_end)
        .filter(|l| {
            let t = l.trim_start();
            !t.is_empty() && !t.starts_with('#')
        })
        .map(str::to_string)
        .collect()
}

/// CS-1R3 A2 — the pinned manual-ruling residual fingerprint, LOADED from
/// `docs/cs1r/pins/manual_residual_fingerprint.txt` (single 32-hex line).
pub fn manual_residual_fingerprint_pin() -> String {
    let lines = read_pin_lines(MANUAL_RESIDUAL_FINGERPRINT_FILE);
    assert_eq!(
        lines.len(),
        1,
        "manual_residual_fingerprint.txt must have exactly one value line, got {}",
        lines.len()
    );
    lines[0].trim().to_string()
}

/// CS-1R3 A2 — the POST-CS1 carve-outs `(repo-relative path, approved blob SHA)`,
/// LOADED from `docs/cs1r/pins/post_cs1_carveout.tsv` (path<TAB>sha<TAB>note).
pub fn post_cs1_carveout() -> Vec<(String, String)> {
    read_pin_lines(POST_CS1_CARVEOUT_FILE)
        .into_iter()
        .map(|line| {
            let mut it = line.split('\t');
            let path = it.next().unwrap_or("").to_string();
            let sha = it
                .next()
                .unwrap_or_else(|| panic!("malformed post_cs1_carveout row (no TAB): {line:?}"))
                .to_string();
            (path, sha)
        })
        .collect()
}

impl SqlxSig {
    /// CS-1R2 A2c / CS-1R4 — compare two signatures across the CS-1 base→head
    /// transition. Identical to `PartialEq` EXCEPT:
    ///  * the decode type is compared EMPTY-IS-WILDCARD (A2c): an INFERRED
    ///    endpoint (`""` — the CS-1 base) matches an explicit head type
    ///    (the whitelisted W3 inferred→explicit `Db*` transform); two DIFFERENT
    ///    explicit types diverge;
    ///  * the SQL compared is the DECODE-ANNOTATION-STRIPPED `sql` (T8 fiscal-neutral
    ///    transform class). CS-1R4 removed the earlier `sql_raw` byte-compare + the
    ///    catalogued-delta escape hatch: this tool asserts NO SQL-byte identity. The
    ///    `as "col: Type"` decode-annotation removal normalizes to equal here; ANY
    ///    other SQL edit (a table, WHERE clause, literal VALUE, or a REAL non-`:`
    ///    alias) still diverges → RED.
    pub fn equiv_across_cs1(&self, other: &SqlxSig) -> bool {
        self.enclosing_fn == other.enclosing_fn
            && self.occurrence == other.occurrence
            && self.sql == other.sql
            && self.query_kind == other.query_kind
            && self.binds == other.binds
            && self.fetch_mode == other.fetch_mode
            && decode_types_compatible(&self.decode_type, &other.decode_type)
    }
}

/// EMPTY-IS-WILDCARD decode-type compatibility (see `equiv_across_cs1`).
pub fn decode_types_compatible(a: &str, b: &str) -> bool {
    a.is_empty() || b.is_empty() || a == b
}

/// Walk a file's AST and extract every sqlx query chain signature.
pub fn extract_sqlx_sigs(file: &File) -> Vec<SqlxSig> {
    let mut v = SqlxVisitor {
        sigs: Vec::new(),
        fn_stack: Vec::new(),
        per_fn_occ: std::collections::HashMap::new(),
    };
    use syn::visit::Visit;
    v.visit_file(file);
    v.sigs
}

struct SqlxVisitor {
    sigs: Vec<SqlxSig>,
    fn_stack: Vec<String>,
    per_fn_occ: std::collections::HashMap<String, usize>,
}

impl<'ast> syn::visit::Visit<'ast> for SqlxVisitor {
    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        self.fn_stack.push(f.sig.ident.to_string());
        syn::visit::visit_item_fn(self, f);
        self.fn_stack.pop();
    }
    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        self.fn_stack.push(f.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, f);
        self.fn_stack.pop();
    }

    fn visit_expr(&mut self, expr: &'ast Expr) {
        // A sqlx chain is a method-call tail (`.execute(..)`, `.fetch_one(..)`,
        // …) whose receiver spine begins at a `sqlx::query*` head. We detect the
        // TERMINATOR and unwind the spine.
        if let Expr::MethodCall(mc) = expr {
            let m = mc.method.to_string();
            let fetch = match m.as_str() {
                "execute" => Some(FetchMode::Execute),
                "fetch_one" => Some(FetchMode::FetchOne),
                "fetch_optional" => Some(FetchMode::FetchOptional),
                "fetch_all" => Some(FetchMode::FetchAll),
                _ => None,
            };
            if let Some(fetch_mode) = fetch {
                if let Some(sig) = self.build_sig(&mc.receiver, fetch_mode) {
                    self.sigs.push(sig);
                }
            }
        }
        syn::visit::visit_expr(self, expr);
    }
}

impl SqlxVisitor {
    /// Given the receiver of a terminator (the chain up to `.execute`/`.fetch_*`),
    /// walk down collecting binds and finding the `sqlx::query*` head.
    fn build_sig(&mut self, recv: &Expr, fetch_mode: FetchMode) -> Option<SqlxSig> {
        let mut binds_rev: Vec<String> = Vec::new();
        let mut cur = recv;
        loop {
            match cur {
                Expr::MethodCall(mc) => {
                    let m = mc.method.to_string();
                    if m == "bind" && mc.args.len() == 1 {
                        binds_rev.push(normalize_bind_arg(mc.args.first().unwrap()));
                    }
                    // other adapter methods (`.persistent(..)`, `.map(..)`) are
                    // transparently skipped — we only care about bind + head.
                    cur = &mc.receiver;
                }
                Expr::Call(call) => {
                    // The head: `sqlx::query_scalar::<_, T>(SQL)` or `query(SQL)`.
                    if let Some(kind) = query_head_kind(&call.func) {
                        let sql_concat = call
                            .args
                            .iter()
                            .filter_map(extract_str_lit)
                            .collect::<Vec<_>>()
                            .join("");
                        // CS-1R4: the SQL-comparison surface is the DECODE-ANNOTATION-
                        // STRIPPED SQL (T8 fiscal-neutral transform class). We no
                        // longer keep a raw-SQL byte-identity field (that field + the
                        // catalogued-delta escape hatch were deleted).
                        let sql = strip_sqlx_decode_annotations(&sql_concat);
                        binds_rev.reverse();
                        let enclosing_fn = self
                            .fn_stack
                            .last()
                            .cloned()
                            .unwrap_or_else(|| "<top>".into());
                        let occ = {
                            let e = self.per_fn_occ.entry(enclosing_fn.clone()).or_insert(0);
                            let v = *e;
                            *e += 1;
                            v
                        };
                        let decode_type = query_head_decode_type(&call.func);
                        return Some(SqlxSig {
                            enclosing_fn,
                            occurrence: occ,
                            sql,
                            query_kind: kind,
                            decode_type,
                            binds: binds_rev,
                            fetch_mode,
                        });
                    }
                    return None;
                }
                _ => return None,
            }
        }
    }
}

/// Is `func` a `sqlx::query*` head? Returns the kind (`query`, `query_scalar`,
/// `query_as`, with any `_with` suffix), ignoring turbofish generics.
fn query_head_kind(func: &Expr) -> Option<String> {
    if let Expr::Path(p) = func {
        let last = p.path.segments.last()?;
        let id = last.ident.to_string();
        if id.starts_with("query") {
            return Some(id);
        }
    }
    None
}

/// CS-1R2 A2c — the decode type carried by a `query*` head's turbofish, rendered
/// canonically so base (inferred, no turbofish) and head
/// (`::<_, DbShiftState>` — explicit) compare EQUAL, but a change to a *different*
/// decode type is caught. Rules:
///   * no turbofish → `""` (base's inferred form, and head's when still inferred);
///   * a `_` inference placeholder segment is dropped (base relied on inference
///     for the row type; head pins the output type — the `_` is not load-bearing);
///   * each `Db*` wrapper is mapped to its domain type (`DbShiftState → ShiftState`,
///     `(DbShiftState, String) → (ShiftState, String)`), matching W3, so the
///     *wrapper* newtype (a pure decode-forwarding shim) is transparent while the
///     UNDERLYING decode type is pinned.
///
/// So `query_scalar::<_, DbShiftState>` and (hypothetical) base
/// `query_scalar::<_, ShiftState>` render identically to `ShiftState`, but
/// `query_scalar::<_, DbDocState>` renders `DocState` ≠ `ShiftState` → RED.
fn query_head_decode_type(func: &Expr) -> String {
    let Expr::Path(p) = func else {
        return String::new();
    };
    let Some(last) = p.path.segments.last() else {
        return String::new();
    };
    let syn::PathArguments::AngleBracketed(ab) = &last.arguments else {
        return String::new();
    };
    // Canonicalize each generic arg: drop `_` placeholders, collapse each type to
    // its LAST path segment (so `prro::db::types::DbNodeMode` and a bare
    // `NodeMode` compare equal — CS-1 qualified the path AND `Db*`-wrapped it),
    // then map `Db*` → domain (W3). A genuine type SWAP (`NodeMode`→`DocState`)
    // still differs. Tuples recurse element-wise.
    let mut parts: Vec<String> = Vec::new();
    for arg in &ab.args {
        if let syn::GenericArgument::Type(ty) = arg {
            if matches!(ty, Type::Infer(_)) {
                continue; // `_` inference placeholder — not load-bearing.
            }
            parts.push(normalize_decode_type(ty));
        }
    }
    parts.join(" , ")
}

/// Collapse a decode type to a canonical, module-path-independent form: each
/// path type → its last segment with `Db*`→domain applied; tuples recurse.
fn normalize_decode_type(ty: &Type) -> String {
    match ty {
        Type::Path(tp) => {
            if let Some(last) = tp.path.segments.last() {
                let id = last.ident.to_string();
                let base = strip_db_prefix(&id).unwrap_or(id);
                if matches!(last.arguments, syn::PathArguments::None) {
                    base
                } else {
                    // a generic leaf (rare): keep only the last segment, Db*-strip
                    // its ident, render the args verbatim after normalization.
                    let mut seg = last.clone();
                    if let Some(inner) = strip_db_prefix(&seg.ident.to_string()) {
                        seg.ident = syn::Ident::new(&inner, seg.ident.span());
                    }
                    let mut only = syn::TypePath {
                        qself: None,
                        path: syn::Path {
                            leading_colon: None,
                            segments: syn::punctuated::Punctuated::new(),
                        },
                    };
                    only.path.segments.push(seg);
                    only.to_token_stream().to_string()
                }
            } else {
                ty.to_token_stream().to_string()
            }
        }
        Type::Tuple(tup) => {
            let elems: Vec<String> = tup.elems.iter().map(normalize_decode_type).collect();
            format!("({})", elems.join(" , "))
        }
        other => other.to_token_stream().to_string(),
    }
}

fn extract_str_lit(e: &Expr) -> Option<String> {
    if let Expr::Lit(l) = e {
        if let syn::Lit::Str(s) = &l.lit {
            return Some(s.value());
        }
    }
    None
}

/// Normalize a bind argument to its semantic value: strip `DbX(..)`, `.map(DbX)`,
/// and a trailing `.as_str()`. So `DbShiftId(x)`, `x.map(DbShiftId)`, and
/// `x.as_str()` all reduce to `x` — a bind SWAP (`x` vs `y`) still differs, a
/// wrapper change does not. Rendered as a token string for comparison.
fn normalize_bind_arg(arg: &Expr) -> String {
    let mut e = arg.clone();
    Canonicalizer.visit_expr_mut(&mut e);
    // strip a trailing `.as_str()`
    if let Expr::MethodCall(mc) = &e {
        if mc.method == "as_str" && mc.args.is_empty() {
            e = (*mc.receiver).clone();
        }
    }
    e.to_token_stream().to_string()
}

/// Strip sqlx inline decode annotations from a SQL string so BOTH endpoints
/// compare on the RUNTIME SQL only (T8). sqlx's `col as "alias: Type"` is a
/// COMPILE-TIME decode hint — with a `Db*` wrapper carrying the `Decode` impl the
/// hint is unnecessary and CS-1 removed the entire ` as "alias: Type"` clause
/// (base `SELECT state as "state: ShiftState"` → head `SELECT state`). To make
/// the runtime SQL comparable regardless of whether the annotation is present, we
/// remove any ` as "<alias>: <type>"` clause (the `as` keyword + the quoted
/// type-annotated alias) whole. A plain `as "alias"` (no `:` — a real runtime
/// column alias, not a decode hint) is KEPT. Applied identically to base + head,
/// so an author who changes a REAL alias still diverges (the alias text is
/// preserved when it has no `:`).
fn strip_sqlx_decode_annotations(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // Look for the ` as "…: …"` decode-annotation clause. Match ascii-case-
        // insensitive `as` bounded by whitespace, then optional ws, then a
        // double-quoted string containing a ':'.
        if is_word_at(&chars, i, "as") {
            // find the start of a following quoted string, skipping ws
            let mut j = i + 2;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && chars[j] == '"' {
                // find closing quote
                if let Some(rel) = chars[j + 1..].iter().position(|&c| c == '"') {
                    let inner: String = chars[j + 1..j + 1 + rel].iter().collect();
                    if inner.contains(':') {
                        // drop the whole ` as "…: …"` clause; also swallow a
                        // single leading space already emitted.
                        if out.ends_with(' ') {
                            out.pop();
                        }
                        i = j + 1 + rel + 1;
                        continue;
                    }
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Whitespace/boundary-bounded case-insensitive word match at position `i`.
fn is_word_at(chars: &[char], i: usize, word: &str) -> bool {
    let w: Vec<char> = word.chars().collect();
    if i + w.len() > chars.len() {
        return false;
    }
    for (k, wc) in w.iter().enumerate() {
        if chars[i + k].to_ascii_lowercase() != *wc {
            return false;
        }
    }
    let before_ok = i == 0 || chars[i - 1].is_whitespace();
    let after_ok = i + w.len() >= chars.len() || chars[i + w.len()].is_whitespace();
    before_ok && after_ok
}

/// The set of files (repo-relative) that require a MANUAL RULING because they
/// carry a transform outside the 4 literal whitelist names (T7 / a residual the
/// canonicalizer cannot fully reduce). Each MUST have an equivalence argument in
/// the committed artifact. The AST-equality check tolerates a residual ONLY for
/// these; the sqlx signature check still applies to them unconditionally.
pub fn manual_ruling_files() -> BTreeSet<String> {
    // live_dps_extended_smoke.rs carries T7 (use-site `.0` on a tuple-decoded id
    // + a removed deref) that the value-level canonicalizer does not rewrite.
    // Its equivalence argument is recorded in docs/cs1r/CS1_TEST_PROVENANCE.md.
    ["rust/prro/tests/live_dps_extended_smoke.rs".to_string()]
        .into_iter()
        .collect()
}
