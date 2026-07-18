//! CS-3 3.2 — cross-crate transport-only MINT source-gate (spec §4.4, auditor condition a).
//!
//! Rust privacy cannot sibling-seal a **cross-crate** constructor. Two families of transport-only
//! mints live in `prro-domain` yet must be minted ONLY by the `prro` transport decoder:
//!   * the DIGEST mints — `DecodedResponseDigest::from_transport_digest` /
//!     `GrpcStatusDigest::from_transport_digest` (PR1 pin5);
//!   * the PROVENANCE mints — `NonEmptyFiscalNumber::from_transport` /
//!     `NonOkStatusCode::from_transport` (PR2 pin4). These are `pub` (the decoder in `prro` must
//!     call the type defined in `prro-domain`), so "a non-empty fiscal id / non-OK code PROVES
//!     transport provenance, not just form" (the D-4 argument that lets `Accepted` carry no digest)
//!     is carried by THIS static gate, not by privacy — otherwise the engine could
//!     `NonEmptyFiscalNumber::from_transport("forged")` and inject provenance it never observed.
//!
//! Sibling lint to `with_immediate_no_foreign_io.rs` (same syn 2 + visitor pattern).
//!
//! Forbidden ANYWHERE under `prro/src/**` EXCEPT `prro/src/transports/dps/**` (the sole decoder mint
//! site) and EXCEPT `#[cfg(test)]` / `#[test]` items (test doubles use a real minter):
//! - a call to any gated symbol (assoc-fn or method syntax);
//! - a `macro_rules!` / macro invocation / `#[macro_export]` whose token-tree MENTIONS a symbol
//!   (syn does NOT expand macros — this is a conservative *syntactic* ban that also catches macro
//!   laundering: a decoder-local `#[macro_export] macro_rules!` emitting the mint, invoked elsewhere);
//! - a `use ... <symbol>` re-export.
//!
//! Positive controls are synthetic snippets, so production `prro/src` stays GREEN.

use std::fs;
use std::path::{Path, PathBuf};

use syn::visit::Visit;

/// The gated symbols — the cross-crate transport mint constructors. `from_transport_digest`
/// (digest) and `from_transport` (provenance). NOTE the substring overlap is harmless: the
/// exact-ident checks (`==`) never confuse them; only the conservative macro token-tree check uses
/// `contains`, where double-flagging one macro line is benign (still a violation).
const GATED_SYMBOLS: &[&str] = &["from_transport_digest", "from_transport"];

/// The gated symbol matching `ident` exactly, if any (for method/path call sites).
fn matched_exact(ident: &syn::Ident) -> Option<&'static str> {
    GATED_SYMBOLS.iter().copied().find(|g| ident == g)
}

#[derive(Debug, Clone)]
struct Violation {
    #[allow(dead_code)]
    file: String,
    detail: String,
}

/// `true` if an item carries `#[cfg(test)]` or `#[test]` (a test double, exempt from the gate).
fn is_test_item(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        if a.path().is_ident("test") {
            return true;
        }
        if a.path().is_ident("cfg") {
            // conservative: any cfg(...) mentioning `test` in its tokens
            return a.meta.to_token_stream_string().contains("test");
        }
        false
    })
}

/// Small helper so we can stringify a `Meta`'s tokens without pulling `quote`.
trait ToTokenStreamString {
    fn to_token_stream_string(&self) -> String;
}
impl ToTokenStreamString for syn::Meta {
    fn to_token_stream_string(&self) -> String {
        match self {
            syn::Meta::List(l) => l.tokens.to_string(),
            syn::Meta::Path(p) => p
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_default(),
            syn::Meta::NameValue(_) => String::new(),
        }
    }
}

struct MintScan<'a> {
    file: &'a str,
    violations: Vec<Violation>,
}

impl<'a> MintScan<'a> {
    fn flag(&mut self, detail: String) {
        self.violations.push(Violation {
            file: self.file.to_string(),
            detail,
        });
    }
}

impl<'ast, 'a> Visit<'ast> for MintScan<'a> {
    // Skip `#[cfg(test)]` modules entirely (test doubles are exempt).
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if is_test_item(&node.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, node);
    }

    // Skip `#[test]` / `#[cfg(test)]` functions.
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if is_test_item(&node.attrs) {
            return;
        }
        syn::visit::visit_item_fn(self, node);
    }

    // `Type::<gated>(...)` (associated-fn syntax → Expr::Call).
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*node.func {
            if let Some(sym) = p.path.segments.last().and_then(|s| matched_exact(&s.ident)) {
                self.flag(format!(
                    "`{sym}(...)` transport mint called outside the transport decoder"
                ));
            }
        }
        syn::visit::visit_expr_call(self, node);
    }

    // `x.<gated>(...)` (method syntax).
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if let Some(sym) = matched_exact(&node.method) {
            self.flag(format!(
                "`.{sym}(...)` transport mint called outside the transport decoder"
            ));
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    // A `use ... <gated> ...` re-export.
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        if let Some(sym) = used_symbol(&node.tree) {
            self.flag(format!(
                "re-export of `{sym}` (`use`) outside the transport decoder"
            ));
        }
        syn::visit::visit_item_use(self, node);
    }

    // Any macro invocation whose token-tree MENTIONS a symbol (laundering vector).
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        let toks = node.tokens.to_string();
        if let Some(sym) = GATED_SYMBOLS.iter().find(|s| toks.contains(**s)) {
            self.flag(format!(
                "macro token-tree mentions `{sym}` outside the transport decoder (macro laundering)"
            ));
        }
        syn::visit::visit_macro(self, node);
    }

    // A `macro_rules!` / `#[macro_export] macro_rules!` DEFINITION mentioning a symbol.
    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        let toks = node.mac.tokens.to_string();
        if let Some(sym) = GATED_SYMBOLS.iter().find(|s| toks.contains(**s)) {
            self.flag(format!(
                "macro_rules! definition emits `{sym}` outside the transport decoder \
                 (macro-export laundering)"
            ));
        }
        syn::visit::visit_item_macro(self, node);
    }
}

/// The gated symbol a `use` tree names, if any.
fn used_symbol(tree: &syn::UseTree) -> Option<&'static str> {
    match tree {
        syn::UseTree::Path(p) => used_symbol(&p.tree),
        syn::UseTree::Name(n) => matched_exact(&n.ident),
        syn::UseTree::Rename(r) => matched_exact(&r.ident),
        syn::UseTree::Group(g) => g.items.iter().find_map(used_symbol),
        syn::UseTree::Glob(_) => None,
    }
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
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

fn scan_source(text: &str, file_name: &str) -> Vec<Violation> {
    let parsed = match syn::parse_file(text) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let mut scan = MintScan {
        file: file_name,
        violations: Vec::new(),
    };
    scan.visit_file(&parsed);
    scan.violations
}

#[test]
fn production_src_mints_digest_only_in_transport_decoder() {
    // Despite the historical name this gate now covers BOTH the digest AND the provenance mints
    // (all of `GATED_SYMBOLS`); the assertion is identical in shape — no mint outside the decoder.
    let src_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let dps_root = src_root.join("transports").join("dps");
    let mut files = Vec::new();
    collect_rs_files(&src_root, &mut files);
    assert!(
        !files.is_empty(),
        "expected .rs files under {}",
        src_root.display()
    );

    let mut all = Vec::new();
    for path in &files {
        // The ONLY allowed mint site: the transport DPS decoder module.
        if path.starts_with(&dps_root) {
            continue;
        }
        let text = fs::read_to_string(path).expect("read .rs");
        let rel = path
            .strip_prefix(env!("CARGO_MANIFEST_DIR"))
            .unwrap_or(path)
            .display()
            .to_string();
        all.extend(scan_source(&text, &rel));
    }
    assert!(
        all.is_empty(),
        "cross-crate transport mints {GATED_SYMBOLS:?} may be called ONLY in \
         prro/src/transports/dps/** — the engine must CARRY a transport-minted digest / provenance, \
         never fabricate one. Violations: {all:#?}"
    );
}

// ─── Synthetic positive controls (kept out of production source) ──────────────

#[test]
fn services_call_of_mint_is_caught() {
    let snippet = r#"
        fn map(reply: &Raw) -> SendResponse {
            let d = DecodedResponseDigest::from_transport_digest([0u8; 32]);
            SendResponse::parsed(d)
        }
    "#;
    let v = scan_source(snippet, "<services_call>");
    assert!(
        v.iter().any(|x| x.detail.contains("from_transport_digest")),
        "a services-side from_transport_digest call must be caught: {v:#?}"
    );
}

#[test]
fn services_call_of_provenance_mint_is_caught() {
    // PR2 pin4: the engine forging a fiscal-id provenance via the cross-crate `from_transport`
    // ctor. Without this the D-4 argument (`Accepted` carries no digest because the non-empty id
    // PROVES provenance) is defeatable. Both provenance types share the `from_transport` ident.
    let snippet = r#"
        fn map(reply: &Raw) -> SendResponse {
            let id = NonEmptyFiscalNumber::from_transport("forged".to_string());
            let code = NonOkStatusCode::from_transport(-1);
            SendResponse::accepted(id, code)
        }
    "#;
    let v = scan_source(snippet, "<services_provenance>");
    assert!(
        v.iter().filter(|x| x.detail.contains("from_transport")).count() >= 2,
        "engine-side NonEmptyFiscalNumber/NonOkStatusCode::from_transport must both be caught: {v:#?}"
    );
}

#[test]
fn macro_export_laundering_of_mint_is_caught() {
    // The exact laundering vector the auditor named: a `#[macro_export] macro_rules!` that emits the
    // gated mint. syn does not expand it — the syntactic token-tree ban catches the DEFINITION.
    let snippet = r#"
        #[macro_export]
        macro_rules! mint_for_engine {
            ($b:expr) => { $crate::delivery::DecodedResponseDigest::from_transport_digest($b) };
        }
    "#;
    let v = scan_source(snippet, "<macro_export>");
    assert!(
        v.iter().any(|x| x.detail.contains("macro")),
        "a macro_rules! emitting the mint must be caught: {v:#?}"
    );
}

#[test]
fn macro_invocation_laundering_is_caught() {
    let snippet = r#"
        fn map() {
            let _ = some_macro!(DecodedResponseDigest::from_transport_digest([0u8; 32]));
        }
    "#;
    let v = scan_source(snippet, "<macro_invoke>");
    assert!(
        v.iter().any(|x| x.detail.contains("from_transport_digest")),
        "a macro invocation mentioning the mint must be caught: {v:#?}"
    );
}

#[test]
fn reexport_of_mint_is_caught() {
    let snippet = r#"
        pub use prro_domain::delivery::DecodedResponseDigest::from_transport_digest;
    "#;
    let v = scan_source(snippet, "<reexport>");
    assert!(
        v.iter().any(|x| x.detail.contains("re-export")),
        "a re-export of the mint must be caught: {v:#?}"
    );
}

#[test]
fn reexport_of_provenance_mint_is_caught() {
    // PR2 pin4: a `use` re-export laundering the provenance ctor out of the decoder module.
    let snippet = r#"
        pub use prro_domain::delivery::NonEmptyFiscalNumber::from_transport;
    "#;
    let v = scan_source(snippet, "<reexport_provenance>");
    assert!(
        v.iter()
            .any(|x| x.detail.contains("re-export") && x.detail.contains("from_transport")),
        "a re-export of the provenance mint must be caught: {v:#?}"
    );
}

#[test]
fn cfg_test_double_is_exempt() {
    // Test doubles (in #[cfg(test)] modules) legitimately call the mints — exempt.
    let snippet = r#"
        #[cfg(test)]
        mod tests {
            fn make() {
                let _ = DecodedResponseDigest::from_transport_digest([0xAB; 32]);
                let _ = NonEmptyFiscalNumber::from_transport("DPS-1".to_string());
            }
        }
    "#;
    let v = scan_source(snippet, "<cfg_test>");
    assert!(
        v.is_empty(),
        "a #[cfg(test)] double must be exempt from the mint gate: {v:#?}"
    );
}

#[test]
fn ordinary_carry_of_a_digest_is_not_flagged() {
    // The engine READING a digest / provenance out of an opaque reply (as_bytes / as_str / passing
    // it on) is fine — only the MINT (from_transport* ctor) is gated.
    let snippet = r#"
        fn carry(d: DecodedResponseDigest, id: &NonEmptyFiscalNumber) -> ([u8; 32], String) {
            (*d.as_bytes(), id.as_str().to_string())
        }
    "#;
    let v = scan_source(snippet, "<carry>");
    assert!(
        v.is_empty(),
        "carrying a digest/id must not be flagged: {v:#?}"
    );
}
