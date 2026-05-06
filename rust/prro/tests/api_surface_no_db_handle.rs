//! W5 — ADR-M2-6 static check.
//!
//! Walks every `.rs` file under `rust/prro/src/crypto/` and
//! `rust/prro/src/transports/`, parses it via `syn::parse_file`, and
//! fails the build if any reachable signature in those trees mentions
//! a sqlx DB-handle type:
//!
//! - `SqlitePool` (concrete type alias)
//! - `SqliteConnection` (concrete type alias)
//! - `Pool<…Sqlite…>` (parameterised pool)
//! - `Transaction<…Sqlite…>` (parameterised transaction)
//!
//! The check is recursive — `Arc<SqlitePool>`, `Option<&mut Transaction<
//! '_, Sqlite>>`, `Result<X, sqlx::Pool<sqlx::Sqlite>>` are ALL caught.
//!
//! Reachable signatures (per W5 design lock):
//!
//! - Free `pub fn` / `pub async fn` items.
//! - `pub trait T { fn …; }` method DEFINITIONS — the `CryptoProvider` +
//!   `DpsChannel` trait surfaces are exactly this shape, and a
//!   free-fn-only scanner would silently pass a violation here.
//! - `pub fn` methods inside inherent `impl` blocks.
//! - EVERY method inside a trait `impl` block (e.g.
//!   `impl DpsChannel for GrpcDpsChannel`), regardless of `pub` keyword
//!   — trait-impl methods are public via the trait surface.
//!
//! Skipped:
//! - `services::cert_refresher` (carved-out per ADR-M2-6 rationale —
//!   service-layer code orchestrates DB writes around crypto/transport
//!   calls).  Implemented as "scan only `src/crypto` + `src/transports`",
//!   not as a per-file allowlist.
//! - `#[cfg(test)]` items + items inside a `#[cfg(test)]` mod.
//!
//! On violation, the diagnostic carries:
//! - `path:line` (line resolved via source-text search for `fn <name>`)
//! - the offending item name
//! - the rendered type expression that matched
//! - the canonical-name kind that triggered the match.

use std::path::{Path, PathBuf};

use quote::ToTokens;
use syn::visit::Visit;

const SCAN_DIRS: &[&str] = &["src/crypto", "src/transports"];

/// What kind of forbidden sqlx type was matched.  Used purely in the
/// diagnostic message so the operator can classify drift quickly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForbiddenKind {
    SqlitePool,
    SqliteConnection,
    PoolOverSqlite,
    TransactionOverSqlite,
}

impl ForbiddenKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::SqlitePool => "SqlitePool",
            Self::SqliteConnection => "SqliteConnection",
            Self::PoolOverSqlite => "Pool<…Sqlite…>",
            Self::TransactionOverSqlite => "Transaction<…Sqlite…>",
        }
    }
}

#[derive(Debug, Clone)]
struct Violation {
    path: PathBuf,
    line: usize,
    item_name: String,
    rendered: String,
    kind: ForbiddenKind,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: ADR-M2-6 violation in `{}`: signature mentions {} (rendered: `{}`)",
            self.path.display(),
            self.line,
            self.item_name,
            self.kind.as_str(),
            self.rendered
        )
    }
}

// ─── AST type-walk: detect forbidden Type tree ────────────────────────

/// Walk a `syn::Type` and return `Some(kind)` if any nested type
/// matches a forbidden pattern.  Recurses through references / boxes
/// / arcs / options / results / tuples / arrays / slices / parens —
/// any wrapper that holds an inner type.
fn type_violates(ty: &syn::Type) -> Option<ForbiddenKind> {
    use syn::Type;
    match ty {
        Type::Path(tp) => {
            // Direct match on the final path segment.
            if let Some(last) = tp.path.segments.last() {
                let ident = last.ident.to_string();
                match ident.as_str() {
                    "SqlitePool" => return Some(ForbiddenKind::SqlitePool),
                    "SqliteConnection" => return Some(ForbiddenKind::SqliteConnection),
                    "Pool" if generic_args_contain_sqlite(&last.arguments) => {
                        return Some(ForbiddenKind::PoolOverSqlite);
                    }
                    "Transaction" if generic_args_contain_sqlite(&last.arguments) => {
                        return Some(ForbiddenKind::TransactionOverSqlite);
                    }
                    _ => {}
                }
            }
            // Recurse into generic args of any path (catches
            // `Arc<SqlitePool>`, `Option<&Transaction<'_, Sqlite>>`,
            // `Result<X, sqlx::Pool<sqlx::Sqlite>>`, etc.).
            for seg in &tp.path.segments {
                if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                    for arg in &args.args {
                        if let syn::GenericArgument::Type(inner) = arg {
                            if let Some(k) = type_violates(inner) {
                                return Some(k);
                            }
                        }
                    }
                }
            }
            None
        }
        Type::Reference(r) => type_violates(&r.elem),
        Type::Group(g) => type_violates(&g.elem),
        Type::Paren(p) => type_violates(&p.elem),
        Type::Ptr(p) => type_violates(&p.elem),
        Type::Slice(s) => type_violates(&s.elem),
        Type::Array(a) => type_violates(&a.elem),
        Type::Tuple(t) => t.elems.iter().find_map(type_violates),
        // Type::ImplTrait / Type::TraitObject / Type::BareFn etc. —
        // not in production crypto/transport surfaces today; if a
        // future signature uses them, extend here.  For now they
        // pass silently — documented limitation.
        _ => None,
    }
}

/// Does the path-arguments list contain a type whose final segment
/// is `Sqlite`?  Used by `Pool<…>` / `Transaction<…>` checks.
fn generic_args_contain_sqlite(args: &syn::PathArguments) -> bool {
    let syn::PathArguments::AngleBracketed(args) = args else {
        return false;
    };
    for arg in &args.args {
        if let syn::GenericArgument::Type(t) = arg {
            if type_contains_sqlite_ident(t) {
                return true;
            }
        }
    }
    false
}

fn type_contains_sqlite_ident(ty: &syn::Type) -> bool {
    if let syn::Type::Path(tp) = ty {
        if tp.path.segments.last().is_some_and(|s| s.ident == "Sqlite") {
            return true;
        }
        for seg in &tp.path.segments {
            if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                for arg in &args.args {
                    if let syn::GenericArgument::Type(inner) = arg {
                        if type_contains_sqlite_ident(inner) {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

// ─── Visitor over a single parsed file ────────────────────────────────

struct Visitor<'a> {
    source: &'a str,
    path: &'a Path,
    cfg_test_depth: u32,
    violations: Vec<Violation>,
    /// Names of every signature actually scanned — exposed to the
    /// "anchor" sanity test below so a regression that silently
    /// stops walking critical surfaces lights up.
    scanned: Vec<String>,
}

impl<'a> Visitor<'a> {
    fn new(source: &'a str, path: &'a Path) -> Self {
        Self {
            source,
            path,
            cfg_test_depth: 0,
            violations: Vec::new(),
            scanned: Vec::new(),
        }
    }

    fn line_for_item(&self, name: &str) -> usize {
        // Search for `fn <name>` (with the literal keyword prefix) so
        // a doc-comment that happens to mention the name doesn't
        // anchor the diagnostic on the wrong line.
        let needle = format!("fn {name}");
        for (idx, line) in self.source.lines().enumerate() {
            if line.contains(&needle) {
                return idx + 1;
            }
        }
        0
    }

    fn check_signature(&mut self, sig: &syn::Signature) {
        let name = sig.ident.to_string();
        self.scanned.push(name.clone());

        for arg in &sig.inputs {
            if let syn::FnArg::Typed(pat_ty) = arg {
                if let Some(kind) = type_violates(&pat_ty.ty) {
                    self.violations.push(Violation {
                        path: self.path.to_path_buf(),
                        line: self.line_for_item(&name),
                        item_name: name.clone(),
                        rendered: pat_ty.ty.to_token_stream().to_string(),
                        kind,
                    });
                }
            }
        }
        if let syn::ReturnType::Type(_, ty) = &sig.output {
            if let Some(kind) = type_violates(ty) {
                self.violations.push(Violation {
                    path: self.path.to_path_buf(),
                    line: self.line_for_item(&name),
                    item_name: name.clone(),
                    rendered: ty.to_token_stream().to_string(),
                    kind,
                });
            }
        }
    }
}

fn has_cfg_test(attrs: &[syn::Attribute]) -> bool {
    for attr in attrs {
        if let syn::Meta::List(ml) = &attr.meta {
            if ml.path.is_ident("cfg")
                && ml
                    .tokens
                    .to_string()
                    .split_whitespace()
                    .any(|t| t == "test")
            {
                return true;
            }
        }
    }
    false
}

impl<'ast> Visit<'ast> for Visitor<'_> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if has_cfg_test(&node.attrs) || self.cfg_test_depth > 0 {
            return;
        }
        if matches!(node.vis, syn::Visibility::Public(_)) {
            self.check_signature(&node.sig);
        }
        // Don't recurse into nested fns inside this fn's body — items
        // there can't form part of the public crypto/transport API.
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let entered = has_cfg_test(&node.attrs);
        if entered {
            self.cfg_test_depth += 1;
        }
        syn::visit::visit_item_mod(self, node);
        if entered {
            self.cfg_test_depth -= 1;
        }
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        if has_cfg_test(&node.attrs) || self.cfg_test_depth > 0 {
            return;
        }
        if !matches!(node.vis, syn::Visibility::Public(_)) {
            return;
        }
        for item in &node.items {
            if let syn::TraitItem::Fn(f) = item {
                if has_cfg_test(&f.attrs) {
                    continue;
                }
                self.check_signature(&f.sig);
            }
        }
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if has_cfg_test(&node.attrs) || self.cfg_test_depth > 0 {
            return;
        }
        let is_trait_impl = node.trait_.is_some();
        for item in &node.items {
            if let syn::ImplItem::Fn(f) = item {
                if has_cfg_test(&f.attrs) {
                    continue;
                }
                let is_pub = matches!(f.vis, syn::Visibility::Public(_));
                // Trait-impl methods are public through the trait
                // surface regardless of `pub`; inherent-impl methods
                // are public only when explicitly `pub`.
                if is_trait_impl || is_pub {
                    self.check_signature(&f.sig);
                }
            }
        }
    }
}

// ─── File walker + driver ────────────────────────────────────────────

fn walk_rs_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| {
        panic!("cannot read scan dir {}: {e}", dir.display());
    });
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_rs_files(&path));
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(path);
        }
    }
    out
}

fn scan_dir(dir: &Path) -> (Vec<Violation>, Vec<String>) {
    let mut all_violations = Vec::new();
    let mut all_scanned = Vec::new();
    for path in walk_rs_files(dir) {
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let parsed = syn::parse_file(&src)
            .unwrap_or_else(|e| panic!("syn::parse_file({}): {e}", path.display()));
        let mut v = Visitor::new(&src, &path);
        v.visit_file(&parsed);
        all_violations.extend(v.violations);
        all_scanned.extend(v.scanned);
    }
    (all_violations, all_scanned)
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn scan_production_surface() -> (Vec<Violation>, Vec<String>) {
    let mut violations = Vec::new();
    let mut scanned = Vec::new();
    let root = manifest_dir();
    for sub in SCAN_DIRS {
        let dir = root.join(sub);
        let (v, s) = scan_dir(&dir);
        violations.extend(v);
        scanned.extend(s);
    }
    (violations, scanned)
}

// ─── Tests ────────────────────────────────────────────────────────────

#[test]
fn current_production_surface_has_no_db_handle_violations() {
    let (violations, _scanned) = scan_production_surface();
    if !violations.is_empty() {
        let lines: Vec<String> = violations.iter().map(|v| v.to_string()).collect();
        panic!(
            "ADR-M2-6: prro::crypto / prro::transports public API must NOT \
             expose sqlx DB handles ({} violation(s)):\n  {}",
            violations.len(),
            lines.join("\n  ")
        );
    }
}

#[test]
fn scan_anchors_known_public_surface() {
    // Sanity check: confirm the walker actually visited the
    // load-bearing public symbols.  This guards against a future
    // regression where the visitor silently stops walking a critical
    // surface (e.g. a syn 3.0 update changes ItemTrait visit method).
    // Anchor names are chosen to span ALL four reachability paths
    // the scanner cares about:
    //   - free `pub fn`         → `unseal_jks`
    //   - `pub trait` method    → `sign_cms_detached`, `send_chk`
    //   - `pub fn` in `impl`    → `connect`, `new`
    //   - method in `impl Trait for Type` → `verify_dstu`
    // No counts asserted: adding legitimate public methods must not
    // break this test.
    let (_violations, scanned) = scan_production_surface();
    let scanned_set: std::collections::HashSet<&str> = scanned.iter().map(String::as_str).collect();
    for anchor in [
        "unseal_jks",        // free pub fn in crypto/session.rs
        "sign_cms_detached", // pub trait method in crypto/provider.rs
        "send_chk",          // pub trait method in transports/dps/channel.rs
        "connect",           // pub fn in transports/dps/grpc.rs (impl GrpcDpsChannel)
        "verify_dstu",       // method in `impl CryptoProvider for InProcessProvider`
    ] {
        assert!(
            scanned_set.contains(anchor),
            "scanner did not visit `{anchor}` — walker regressed; visited names: {:?}",
            {
                let mut v: Vec<&str> = scanned_set.iter().copied().collect();
                v.sort();
                v
            }
        );
    }
}

// ─── Negative fixtures (synthesised AST) ─────────────────────────────

fn scan_synthetic_source(src: &str, path_label: &str) -> Vec<Violation> {
    let parsed = syn::parse_file(src).expect("synthetic source must parse");
    let path = PathBuf::from(path_label);
    let mut v = Visitor::new(src, &path);
    v.visit_file(&parsed);
    v.violations
}

#[test]
fn negative_fixture_free_fn_violation_is_caught() {
    let src = r#"
use sqlx::SqlitePool;

pub async fn debug_violation(pool: &SqlitePool) -> sqlx::Result<()> {
    let _ = pool;
    Ok(())
}
"#;
    let violations = scan_synthetic_source(src, "synthetic/free_fn.rs");
    assert_eq!(
        violations.len(),
        1,
        "expected exactly one violation, got {violations:?}"
    );
    let v = &violations[0];
    assert_eq!(v.kind, ForbiddenKind::SqlitePool);
    assert_eq!(v.item_name, "debug_violation");
    assert!(v.rendered.contains("SqlitePool"));
    // Diagnostic carries path + line + name.
    let msg = v.to_string();
    assert!(msg.contains("synthetic/free_fn.rs"));
    assert!(msg.contains("debug_violation"));
    assert!(msg.contains("SqlitePool"));
}

#[test]
fn negative_fixture_arc_wrapped_violation_is_caught() {
    // Wrapper types don't hide the violation.
    let src = r#"
use std::sync::Arc;
pub fn wrapped(p: Arc<sqlx::SqlitePool>) {}
"#;
    let violations = scan_synthetic_source(src, "synthetic/arc.rs");
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].kind, ForbiddenKind::SqlitePool);
}

#[test]
fn negative_fixture_pool_over_sqlite_with_qualified_path() {
    let src = r#"
pub fn poolish(p: &sqlx::Pool<sqlx::Sqlite>) {}
"#;
    let violations = scan_synthetic_source(src, "synthetic/pool.rs");
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].kind, ForbiddenKind::PoolOverSqlite);
}

#[test]
fn negative_fixture_transaction_with_lifetime_caught() {
    let src = r#"
pub fn txn<'a>(t: &mut sqlx::Transaction<'a, sqlx::Sqlite>) {}
"#;
    let violations = scan_synthetic_source(src, "synthetic/txn.rs");
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].kind, ForbiddenKind::TransactionOverSqlite);
}

#[test]
fn cfg_test_module_is_skipped() {
    // A pub fn nested under a `#[cfg(test)] mod` MUST be skipped —
    // production code there is unreachable from the lib's API
    // surface.
    let src = r#"
#[cfg(test)]
mod tests {
    pub fn sneaky(_: sqlx::SqlitePool) {}
}
"#;
    let violations = scan_synthetic_source(src, "synthetic/cfg_test.rs");
    assert!(
        violations.is_empty(),
        "violations under #[cfg(test)] must be skipped; got {violations:?}"
    );
}

#[test]
fn cfg_test_attr_directly_on_fn_is_skipped() {
    let src = r#"
#[cfg(test)]
pub fn test_only(_: sqlx::SqlitePool) {}
"#;
    let violations = scan_synthetic_source(src, "synthetic/cfg_test_fn.rs");
    assert!(violations.is_empty());
}

#[test]
fn private_free_fn_is_not_scanned() {
    // A non-`pub` free fn cannot escape the crate; scanner must
    // ignore it so refactor-temporary helpers don't trip the gate.
    let src = r#"
fn helper(_: sqlx::SqlitePool) {}
"#;
    let violations = scan_synthetic_source(src, "synthetic/private.rs");
    assert!(violations.is_empty());
}
