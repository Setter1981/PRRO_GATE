//! Shared structural scanner for the "no production caller of the INACTIVE S7-1 delivery
//! lifecycle" static pins (`migration_032::p03` + `migration_033::rg08`).
//!
//! Retargeted after S7-2 (`3f69af5`): the broad grep for ANY `delivery_reservation` reference no
//! longer matches the invariant. S7-2 intentionally ACTIVATED the FN-fence read helper
//! `fn_fence_active_tx`, wiring it into five production writers (pinned by
//! `fence_wired_into_writers_static_pin`). The invariant that still holds is narrower and is
//! enforced here STRUCTURALLY (syn AST, not grep): the INACTIVE S7-1 *lifecycle* functions
//! (authorize → submit → record → apply, boot resume/list, reservation insert) have NO production
//! caller until the whole-fence cutover. `#[cfg(test)]` code is excluded structurally (test
//! scaffolding, not a production caller); `#[cfg(not(test))]` is production and is NOT excluded.
//!
//! Mirrors the syn-2 visitor pattern of `with_immediate_no_foreign_io.rs`. Included by both pins
//! via `#[path = "support/inactive_lifecycle_scan.rs"] mod ...;`.

use std::fs;
use std::path::{Path, PathBuf};

use syn::visit::Visit;

/// The still-INACTIVE S7-1 delivery lifecycle symbols. A PRODUCTION (non-`#[cfg(test)]`) reference
/// — a call OR a `use` import — to any of these means the INACTIVE foundation gained a live caller
/// before the cutover (forbidden). Deliberately EXCLUDES:
/// - `fn_fence_active_tx` — the S7-2 fence read-helper, intentionally active (pinned by
///   `fence_wired_into_writers_static_pin`);
/// - `complete_operator_pending` — the sanctioned gap-4a operator-completion entrypoint (sole
///   caller = `services::reconciliation::operator_completion`).
///
/// `insert` is matched ONLY as the qualified `delivery_reservation::insert` (the bare name collides
/// with `HashMap::insert`, sqlx, etc.).
const UNIQUE_INACTIVE_LIFECYCLE: &[&str] = &[
    "authorize_submission",
    "record_outcome",
    "apply_outcome",
    "resume_crashed_reservation",
    "list_call_started_without_outcome",
    // not yet defined — guarded pre-emptively for the §7.1 boot-first reservation pass:
    "list_outcome_observed_pending_apply",
    "get_active_for_fn",
];

#[derive(Debug)]
pub struct LifecycleRef {
    // Read via `.detail` in `migration_032`'s bite controls and via `Debug` in the assert
    // messages; `#[allow(dead_code)]` covers the binaries that only Debug-print (e.g. `rg08`).
    #[allow(dead_code)]
    pub file: String,
    #[allow(dead_code)]
    pub detail: String,
}

/// `true` for `#[cfg(test)]` EXACTLY — NOT `#[cfg(not(test))]` (whose top-level nested meta is
/// `not`, so `test` is never seen as a direct predicate).
fn is_cfg_test_attr(attr: &syn::Attribute) -> bool {
    if !attr.path().is_ident("cfg") {
        return false;
    }
    let mut found = false;
    let _ = attr.parse_nested_meta(|m| {
        if m.path.is_ident("test") {
            found = true;
        }
        Ok(())
    });
    found
}

/// Uniform attribute access over the `syn::Item` variants that can be `#[cfg(test)]`-gated.
fn item_attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Const(i) => &i.attrs,
        syn::Item::Enum(i) => &i.attrs,
        syn::Item::Fn(i) => &i.attrs,
        syn::Item::Impl(i) => &i.attrs,
        syn::Item::Mod(i) => &i.attrs,
        syn::Item::Static(i) => &i.attrs,
        syn::Item::Struct(i) => &i.attrs,
        syn::Item::Trait(i) => &i.attrs,
        syn::Item::Use(i) => &i.attrs,
        _ => &[],
    }
}

/// If a dotted path (as segment idents) references an INACTIVE lifecycle symbol, return the symbol.
fn segments_hit(segs: &[String]) -> Option<String> {
    if let Some(s) = segs
        .iter()
        .find(|s| UNIQUE_INACTIVE_LIFECYCLE.contains(&s.as_str()))
    {
        return Some(s.clone());
    }
    if segs
        .windows(2)
        .any(|w| w[0] == "delivery_reservation" && w[1] == "insert")
    {
        return Some("delivery_reservation::insert".to_string());
    }
    None
}

/// Flatten a `use` tree into its leaf paths (segment-ident vectors). A `Rename` keeps the ORIGINAL
/// imported name (an alias cannot launder a lifecycle import); a `Glob` records its prefix so a
/// glob of `delivery_reservation` is still visible to `segments_hit`.
fn flatten_use(tree: &syn::UseTree, prefix: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
    match tree {
        syn::UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            flatten_use(&p.tree, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Name(n) => {
            let mut full = prefix.clone();
            full.push(n.ident.to_string());
            out.push(full);
        }
        syn::UseTree::Rename(r) => {
            let mut full = prefix.clone();
            full.push(r.ident.to_string());
            out.push(full);
        }
        syn::UseTree::Group(g) => {
            for t in &g.items {
                flatten_use(t, prefix, out);
            }
        }
        syn::UseTree::Glob(_) => out.push(prefix.clone()),
    }
}

struct LifecycleScan<'a> {
    file: &'a str,
    hits: Vec<LifecycleRef>,
}

impl LifecycleScan<'_> {
    fn record(&mut self, sym: String) {
        self.hits.push(LifecycleRef {
            file: self.file.to_string(),
            detail: format!(
                "{}: production reference to INACTIVE S7-1 lifecycle symbol `{sym}`",
                self.file
            ),
        });
    }
}

impl<'ast> Visit<'ast> for LifecycleScan<'_> {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        if item_attrs(item).iter().any(is_cfg_test_attr) {
            return; // #[cfg(test)] item — test scaffolding, not a production caller
        }
        syn::visit::visit_item(self, item);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        if node.attrs.iter().any(is_cfg_test_attr) {
            return;
        }
        syn::visit::visit_impl_item_fn(self, node);
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        // (visit_item already filtered #[cfg(test)] uses.)
        let mut out = Vec::new();
        let mut prefix = Vec::new();
        flatten_use(&node.tree, &mut prefix, &mut out);
        for segs in out {
            if let Some(sym) = segments_hit(&segs) {
                self.record(format!("use {sym}"));
            }
        }
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segs: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        if let Some(sym) = segments_hit(&segs) {
            self.record(sym);
        }
        syn::visit::visit_path(self, path);
    }
}

/// Scan one source file for production (non-`#[cfg(test)]`) references to any INACTIVE lifecycle
/// symbol. A parse failure yields no hits (mirrors `with_immediate_no_foreign_io.rs`).
pub fn scan_inactive_lifecycle_refs(text: &str, file: &str) -> Vec<LifecycleRef> {
    let parsed = match syn::parse_file(text) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let mut scan = LifecycleScan {
        file,
        hits: Vec::new(),
    };
    scan.visit_file(&parsed);
    scan.hits
}

pub fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                collect_rs_files(&p, out);
            } else if p.extension().map(|e| e == "rs").unwrap_or(false) {
                out.push(p);
            }
        }
    }
}

/// The shared production pin body: scan every `src/**/*.rs` (except the repo file, which DEFINES
/// these fns and calls `insert` internally, and `repositories/mod.rs`, which only declares the
/// module) and return every production reference to an INACTIVE lifecycle symbol.
pub fn scan_src_tree(manifest_dir: &str) -> Vec<LifecycleRef> {
    let src_root = Path::new(manifest_dir).join("src");
    let mut files = Vec::new();
    collect_rs_files(&src_root, &mut files);
    assert!(
        !files.is_empty(),
        "expected at least one .rs file under {}",
        src_root.display()
    );
    let mut hits = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(manifest_dir)
            .unwrap_or(path)
            .display()
            .to_string();
        if rel.ends_with("repositories/delivery_reservation.rs")
            || rel.ends_with("repositories/mod.rs")
        {
            continue;
        }
        let text = fs::read_to_string(path).expect("read .rs file");
        hits.extend(scan_inactive_lifecycle_refs(&text, &rel));
    }
    hits
}
