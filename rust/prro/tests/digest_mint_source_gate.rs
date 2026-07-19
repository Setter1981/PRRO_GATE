//! CS-3 3.2 PR4 fixup B1 — the "authority-flow" source-gate for the cross-crate transport
//! mint + delivery authority ctors (spec §4.4, auditor conditions a/b + the four proven bypasses).
//!
//! ## Why this gate exists
//!
//! Rust privacy cannot sibling-seal a **cross-crate** constructor. Three families of authority
//! symbols live in `prro-domain` yet must be produced ONLY by the `prro` transport decoder (mints)
//! or the one engine mapper (delivery ctors):
//!   * DIGEST mints — `{DecodedResponseDigest,GrpcStatusDigest}::from_transport_digest` (PR1 pin5);
//!   * PROVENANCE mints — `{NonEmptyFiscalNumber,NonOkStatusCode}::from_transport` (PR2 pin4). These
//!     are `pub`, so "a non-empty fiscal id / non-OK code PROVES transport provenance" (the D-4
//!     argument that lets `Accepted` carry no digest) is carried by THIS static gate, not by
//!     privacy — otherwise the engine could `NonEmptyFiscalNumber::from_transport("forged")` and
//!     inject provenance it never observed;
//!   * DELIVERY authority ctors — `SendResponse::{parsed,no_response,remote_status}` /
//!     `SendOutcome::{accepted,ok_but_no_fiscal_number,missing_status,from_server_code}` — `pub` in
//!     prro-domain, so "the engine can only FORWARD transport-minted evidence, never fabricate a
//!     `SendResponse`/`SendOutcome`" is enforced (best-effort) by THIS gate.
//!
//! ## WHAT THIS GATE IS — and is NOT (consolidated-audit round-2, honest framing)
//!
//! This is a **best-effort CI source-policy lint**, **NOT a type-level seal**. A syn scan cannot
//! perform macro expansion, type/alias resolution, cfg evaluation over all feature combinations, or
//! resolution of `include!`d / generated / proc-macro-expanded source. It catches the REALISTIC
//! threat (accidental / careless misplacement — a stray direct call, capture, wrapper, alias,
//! `cfg(not(test))`, or `include!`), which is what the canaries below pin. A DETERMINED adversary
//! retains residual vectors that are heuristic-only or beyond syn:
//!   - split-token / arbitrarily-computed macros (only a conservative heuristic here);
//!   - type-alias / associated-type projection indirection (`type A = SendResponse; <A>::parsed`);
//!   - proc-macro expansion (invisible to syn entirely);
//!   - feature-combination subtleties beyond "cfg logically requires test".
//!
//! **In 3.2 this is safe because the mapped `SendResponse` is a READ-ONLY shadow that drives nothing**
//! (bound as `_shadow_response`, never consumed) — a fabricated value is inert. The STRONG guarantee
//! (a real type-seal) requires co-locating the private mint with its sole consumer in one
//! crate/module so Rust privacy enforces it; that is a **deferred D/E entry-condition** (before the
//! shadow is wired live, either land that co-location OR record an accepted "lint + review" decision).
//! Do NOT describe this gate as "type-sealed authority" — it is a source policy backed by CI + review.
//!
//! ## The model (auditor's required fix — the old two-visitor gate was porous)
//!
//! The previous gate exempted whole directories/files and only caught DIRECT `Type::ctor(...)`
//! calls, so an auditor bypassed it four ways (all had passed GREEN):
//!   1. function-pointer capture — `let f = NonEmptyFiscalNumber::from_transport; f("FORGED")` — a
//!      captured VALUE was never a call, so neither visitor fired;
//!   2. a direct ctor under `#[cfg(not(test))]` — the naive `contains("test")` text check matched
//!      the substring `test` inside `not(test)` and wrongly exempted it;
//!   3. a `#[macro_export] macro_rules!` in the decoder emitting a mint, invoked bare from services;
//!   4. a forwarding wrapper INSIDE the allowlisted `shadow_map.rs` (a fn other than
//!      `map_send_reply` that constructs a `SendResponse`), called from outside.
//!
//! The new gate scans **EVERY** `.rs` under `prro/src/**` with NO directory/file exemption. A gated
//! symbol may appear in NON-TEST code ONLY as a DIRECT function-call (`Type::ctor(args)`) located
//! inside an ALLOWLISTED `(file-suffix, enclosing-fn)` site. Everything else is a VIOLATION:
//!   * capturing a gated symbol as a VALUE (an `ExprPath` that is NOT the callee of a call) — even
//!     inside an allowed file;
//!   * a direct call inside a NON-allowlisted function (even in the decoder / `shadow_map.rs` — e.g.
//!     a forwarding wrapper);
//!   * a `use ...` re-export of a gated symbol (mint idents OR a renamed delivery type);
//!   * a `macro_rules!` definition OR a macro invocation whose token-tree mentions a gated symbol;
//!   * a `type X = SendResponse|SendOutcome` alias;
//!   * code under `#[cfg(not(test))]`.
//!
//! Genuine test code IS exempt: an item (fn/mod/impl) carrying `#[test]`, `#[cfg(test)]`, or
//! `#[cfg(all(...))]` / `#[cfg(any(...))]` where `test` appears in a POSITIVE (non-negated)
//! position. `#[cfg(not(test))]` is NOT exempt. The cfg Meta is parsed STRUCTURALLY (the tree is
//! recursed; a `test` under `not(...)` does NOT exempt).
//!
//! A file that FAILS to parse (`syn::parse_file` Err) produces a VIOLATION (fail-closed), never a
//! silent skip.
//!
//! ## Allowlist (the ONLY current production call sites — verified against origin)
//!   * `from_transport_digest`:
//!       - (transports/dps/dto.rs, decoded_digest_check)
//!       - (transports/dps/dto.rs, decoded_digest_status)
//!       - (transports/dps/dto.rs, decoded_digest_rro_info)
//!       - (transports/dps/grpc.rs, status_digest)
//!   * `from_transport`:
//!       - (transports/dps/dto.rs, raw_reply_from_check_response)
//!       - (transports/dps/raw_reply.rs, observe_from_legacy)
//!   * delivery ctors:
//!       - (services/write_path/shadow_map.rs, map_send_reply)
//!
//! Test-mint sites in `services/*` are `#[cfg(test)]`-exempt.
//!
//! Sibling lint to `with_immediate_no_foreign_io.rs` / `api_surface_no_db_handle.rs` (same syn 2 +
//! visitor pattern; the structural cfg-test recursion mirrors `api_surface`'s `has_cfg_test`).

use std::fs;
use std::path::{Path, PathBuf};

use syn::visit::Visit;

// ─── Gated symbol tables ───────────────────────────────────────────────────────

/// DIGEST + PROVENANCE mints, matched on the LAST path segment ident (exact `==`). Note the
/// substring overlap `from_transport_digest` ⊃ `from_transport` is harmless: all matching is on
/// exact idents (`from_transport_digest != from_transport`), and the macro token-tree check is a
/// deliberately conservative superset where double-flagging one line is still just a violation.
const MINT_SYMBOLS: &[&str] = &["from_transport_digest", "from_transport"];

/// The gated `(Type, ctor)` delivery authority pairs (two-segment `Type::ctor` tail match).
const DELIVERY_CTORS: &[(&str, &str)] = &[
    ("SendResponse", "parsed"),
    ("SendResponse", "no_response"),
    ("SendResponse", "remote_status"),
    ("SendOutcome", "accepted"),
    ("SendOutcome", "ok_but_no_fiscal_number"),
    ("SendOutcome", "missing_status"),
    ("SendOutcome", "from_server_code"),
];

/// The two gated delivery types (whose ctors are the mapper-only symbols).
const DELIVERY_TYPES: &[&str] = &["SendResponse", "SendOutcome"];

/// A canonical "gated symbol name" used for allowlist keying and violation messages.
/// For a mint it is the mint ident; for a delivery ctor it is `Type::ctor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GatedSymbol {
    Mint(&'static str),
    Delivery(&'static str, &'static str),
}

impl GatedSymbol {
    fn label(&self) -> String {
        match self {
            GatedSymbol::Mint(s) => (*s).to_string(),
            GatedSymbol::Delivery(t, c) => format!("{t}::{c}"),
        }
    }
}

// ─── The production allowlist: (file-suffix, enclosing-fn, symbol) ─────────────
//
// A site is allowed iff the scanned file path ENDS WITH `file_suffix` (forward-slash normalized),
// the nearest enclosing fn name matches `fn_name`, AND the gated symbol matches. Anything else in
// non-test code is a violation. `is_allowed` is exposed as a small pure helper so it can be
// unit-tested directly (see `allowlist_helper_*`).

struct AllowEntry {
    file_suffix: &'static str,
    fn_name: &'static str,
    symbol: GatedSymbol,
}

const ALLOWLIST: &[AllowEntry] = &[
    // digest mints
    AllowEntry {
        file_suffix: "transports/dps/dto.rs",
        fn_name: "decoded_digest_check",
        symbol: GatedSymbol::Mint("from_transport_digest"),
    },
    AllowEntry {
        file_suffix: "transports/dps/dto.rs",
        fn_name: "decoded_digest_status",
        symbol: GatedSymbol::Mint("from_transport_digest"),
    },
    AllowEntry {
        file_suffix: "transports/dps/dto.rs",
        fn_name: "decoded_digest_rro_info",
        symbol: GatedSymbol::Mint("from_transport_digest"),
    },
    AllowEntry {
        file_suffix: "transports/dps/grpc.rs",
        fn_name: "status_digest",
        symbol: GatedSymbol::Mint("from_transport_digest"),
    },
    // provenance mints
    AllowEntry {
        file_suffix: "transports/dps/dto.rs",
        fn_name: "raw_reply_from_check_response",
        symbol: GatedSymbol::Mint("from_transport"),
    },
    AllowEntry {
        file_suffix: "transports/dps/raw_reply.rs",
        fn_name: "observe_from_legacy",
        symbol: GatedSymbol::Mint("from_transport"),
    },
    // delivery ctors — the one engine mapper
    AllowEntry {
        file_suffix: "services/write_path/shadow_map.rs",
        fn_name: "map_send_reply",
        symbol: GatedSymbol::Delivery("", ""), // matched specially below (any delivery ctor)
    },
];

/// `true` if a DIRECT call to `symbol` inside `(file, enclosing_fn)` is an allowlisted production
/// site. `file` is the scanned relative path (forward-slash normalized). A delivery ctor is allowed
/// only in `map_send_reply` inside `shadow_map.rs`; a mint only in its enumerated sites.
fn is_allowed(file: &str, enclosing_fn: Option<&str>, symbol: &GatedSymbol) -> bool {
    let file = file.replace('\\', "/");
    let Some(fnname) = enclosing_fn else {
        return false; // a call outside any fn (e.g. a const/static initializer) is never allowlisted
    };
    for e in ALLOWLIST {
        if !file.ends_with(e.file_suffix) || e.fn_name != fnname {
            continue;
        }
        match (&e.symbol, symbol) {
            (GatedSymbol::Mint(a), GatedSymbol::Mint(b)) if a == b => return true,
            // The shadow_map entry authorizes ANY delivery ctor inside `map_send_reply`.
            (GatedSymbol::Delivery(_, _), GatedSymbol::Delivery(_, _)) => return true,
            _ => {}
        }
    }
    false
}

// ─── cfg(test) / #[test] structural exemption ──────────────────────────────────

/// `true` if `attrs` carry `#[test]` or a `#[cfg(...)]` that is test-only (structural). Mirrors the
/// proven recursion in `api_surface_no_db_handle.rs`, but per this gate's spec ALSO treats
/// `any(...test...)`/`all(...test...)` with `test` in a POSITIVE (non-negated) position as exempt.
fn is_test_exempt(attrs: &[syn::Attribute]) -> bool {
    for attr in attrs {
        // `#[test]`
        if attr.path().is_ident("test") {
            return true;
        }
        // `#[cfg(...)]` — parse the inner Meta tree structurally.
        if let syn::Meta::List(ml) = &attr.meta {
            if !ml.path.is_ident("cfg") {
                continue;
            }
            let Ok(inner) = syn::parse2::<syn::Meta>(ml.tokens.clone()) else {
                continue;
            };
            if cfg_requires_test(&inner) {
                return true;
            }
        }
    }
    false
}

/// `true` iff the cfg predicate `m` LOGICALLY REQUIRES `test` — i.e. every build that satisfies `m`
/// has `test` enabled (so the item is genuinely test-only and safe to exempt). This is proper
/// boolean implication, NOT "mentions test anywhere" (consolidated-audit round-2 B1: the old
/// `.any()` on `any(...)` wrongly exempted `any(test, feature="x")`, which compiles in production
/// when `x` is on, and `any(test, not(test))`, which is always true):
/// - `test`         → requires test;
/// - `all(A,B,…)`   → requires test if ANY conjunct requires it (all needs every conjunct);
/// - `any(A,B,…)`   → requires test ONLY if EVERY disjunct requires it (any needs just one branch);
/// - `not(…)` / unknown wrappers → conservatively does NOT require test (no full boolean analysis;
///   worst case a pathological `not(not(test))` is scanned, which is safe — the author uses plain
///   `cfg(test)`).
fn cfg_requires_test(m: &syn::Meta) -> bool {
    match m {
        syn::Meta::Path(p) => p.is_ident("test"),
        syn::Meta::List(ml) => {
            let parsed = ml.parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            );
            let Ok(items) = parsed else { return false };
            if ml.path.is_ident("all") {
                items.iter().any(cfg_requires_test)
            } else if ml.path.is_ident("any") {
                !items.is_empty() && items.iter().all(cfg_requires_test)
            } else {
                // `not(...)` and any unknown predicate: conservatively NOT test-required.
                false
            }
        }
        syn::Meta::NameValue(_) => false,
    }
}

// ─── Violation record ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Violation {
    #[allow(dead_code)]
    file: String,
    detail: String,
}

// ─── The single authority-flow visitor ─────────────────────────────────────────

struct AuthorityScan<'a> {
    file: &'a str,
    /// Nearest-enclosing fn-name stack (pushed in `visit_item_fn`, popped after recursing) — used
    /// for the `(file, enclosing-fn)` allowlist key.
    enclosing_fn: Vec<String>,
    violations: Vec<Violation>,
}

// NB: test-exemption is enforced by EARLY-RETURN in the `visit_item_*` overrides (we never recurse
// into a test-exempt fn/mod/impl/type/macro subtree). Because entry into an exempt subtree is
// refused wholesale, no `flag()` call is ever reached inside it — a separate "test_depth" counter is
// therefore unnecessary. Any gated use in genuine test code is thus invisible to the gate.

impl<'a> AuthorityScan<'a> {
    fn new(file: &'a str) -> Self {
        AuthorityScan {
            file,
            enclosing_fn: Vec::new(),
            violations: Vec::new(),
        }
    }

    fn flag(&mut self, detail: String) {
        self.violations.push(Violation {
            file: self.file.to_string(),
            detail,
        });
    }

    fn current_fn(&self) -> Option<&str> {
        self.enclosing_fn.last().map(|s| s.as_str())
    }

    /// Classify a path/qself expression as a gated symbol, if it names one.
    fn classify_expr_path(p: &syn::ExprPath) -> Option<GatedSymbol> {
        // Two-segment `Type::ctor` tail (delivery), or a single-segment ctor under a qself.
        if let Some(d) = delivery_from_path(&p.path) {
            return Some(d);
        }
        if p.qself.is_some() {
            if let Some(d) = delivery_from_qself(p) {
                return Some(d);
            }
        }
        // Last-segment ident match (mints).
        if let Some(last) = p.path.segments.last() {
            if let Some(m) = mint_from_ident(&last.ident) {
                return Some(m);
            }
        }
        None
    }
}

/// The mint a bare ident names, if any (exact match on the LAST segment ident).
fn mint_from_ident(ident: &syn::Ident) -> Option<GatedSymbol> {
    MINT_SYMBOLS
        .iter()
        .copied()
        .find(|m| ident == m)
        .map(GatedSymbol::Mint)
}

/// The delivery `(Type, ctor)` a path's last two segments name, if any.
fn delivery_from_path(path: &syn::Path) -> Option<GatedSymbol> {
    let mut rev = path.segments.iter().rev();
    let last = rev.next()?.ident.to_string();
    let prev = rev.next()?.ident.to_string();
    DELIVERY_CTORS
        .iter()
        .copied()
        .find(|(t, c)| *t == prev && *c == last)
        .map(|(t, c)| GatedSymbol::Delivery(t, c))
}

/// The delivery `(Type, ctor)` a qualified `<Type>::ctor` names — syn puts `Type` in `qself.ty`,
/// leaving a single-segment `path == [ctor]`.
fn delivery_from_qself(p: &syn::ExprPath) -> Option<GatedSymbol> {
    let qself = p.qself.as_ref()?;
    // Round-2 B3: recurse into generic args so `<Identity<SendResponse>>::parsed` (identity alias
    // hiding the real type in a type-parameter) is caught, not just `<SendResponse>::parsed`.
    let ty = first_delivery_type_in(&qself.ty)?;
    let ctor = p.path.segments.last()?.ident.to_string();
    DELIVERY_CTORS
        .iter()
        .copied()
        .find(|(t, c)| **t == ty && **c == ctor)
        .map(|(t, c)| GatedSymbol::Delivery(t, c))
}

/// Recursively find a delivery authority TYPE named in `ty` — the type itself OR any of its generic
/// arguments. HEURISTIC (round-2 B3): syn cannot resolve type aliases / associated-type projections,
/// so deeper indirection (`type A = SendResponse; <A>::parsed`, `<T as Tr>::Assoc`) still evades.
/// See the module-level limits.
fn first_delivery_type_in(ty: &syn::Type) -> Option<String> {
    let syn::Type::Path(tp) = ty else { return None };
    let seg = tp.path.segments.last()?;
    let id = seg.ident.to_string();
    if DELIVERY_TYPES.contains(&id.as_str()) {
        return Some(id);
    }
    if let syn::PathArguments::AngleBracketed(ab) = &seg.arguments {
        for arg in &ab.args {
            if let syn::GenericArgument::Type(inner) = arg {
                if let Some(found) = first_delivery_type_in(inner) {
                    return Some(found);
                }
            }
        }
    }
    None
}

/// The last path-segment ident of a `Type` path, if it is a plain path type.
fn type_last_seg(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(tp) => tp.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

impl<'ast, 'a> Visit<'ast> for AuthorityScan<'a> {
    // ── test-exempt item skips (return early: do NOT recurse into exempt bodies) ──

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if is_test_exempt(&node.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if is_test_exempt(&node.attrs) {
            return;
        }
        // Track the nearest enclosing fn name for the (file, fn) allowlist check.
        self.enclosing_fn.push(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
        self.enclosing_fn.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        if is_test_exempt(&node.attrs) {
            return;
        }
        self.enclosing_fn.push(node.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, node);
        self.enclosing_fn.pop();
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if is_test_exempt(&node.attrs) {
            return;
        }
        syn::visit::visit_item_impl(self, node);
    }

    // Nested fns declared inside a fn body / closures also get an enclosing-fn frame via
    // visit_item_fn above; expression-position closures share the current frame (fine — an
    // allowlisted fn's closures are part of that fn).

    // ── the direct-call gate ──────────────────────────────────────────────────
    //
    // Inspect the callee. If it is a gated ExprPath, treat it as a DIRECT CALL — allowed only in an
    // allowlisted (file, fn, symbol) site, else flag. Then recurse ONLY into `node.args` (NOT the
    // func) so the callee path is not re-visited by `visit_expr_path` (which would misread a direct
    // call as a capture). Any gated path that DOES reach `visit_expr_path` is therefore a capture.
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        let mut handled_callee = false;
        if let syn::Expr::Path(p) = &*node.func {
            if let Some(sym) = Self::classify_expr_path(p) {
                handled_callee = true;
                if !is_allowed(self.file, self.current_fn(), &sym) {
                    let where_fn = self
                        .current_fn()
                        .map(|f| format!("fn `{f}`"))
                        .unwrap_or_else(|| "no enclosing fn".to_string());
                    self.flag(format!(
                        "`{}(...)` authority ctor called at a NON-allowlisted site ({where_fn}) — \
                         only the transport decoder (mints) / engine mapper (delivery) may construct it",
                        sym.label()
                    ));
                }
            }
        }
        if handled_callee {
            // Recurse into the arguments only — deliberately skip re-visiting `node.func`.
            for arg in &node.args {
                self.visit_expr(arg);
            }
        } else {
            syn::visit::visit_expr_call(self, node);
        }
    }

    // ── the capture gate ──────────────────────────────────────────────────────
    //
    // A gated ExprPath reaching here was NOT the callee of a direct call (we did not recurse into
    // the func above), so it is a CAPTURE / non-call use — e.g. `let f = SendResponse::parsed;` or
    // a fn-pointer coercion. ALWAYS a violation (even inside an allowlisted file), unless test-exempt.
    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        if let Some(sym) = Self::classify_expr_path(node) {
            self.flag(format!(
                "`{}` captured as a VALUE (fn-pointer / non-call use) — an authority ctor may only \
                 appear as a DIRECT call at an allowlisted site, never captured",
                sym.label()
            ));
        }
        syn::visit::visit_expr_path(self, node);
    }

    // NB: method syntax (`x.from_transport(..)`) is deliberately NOT flagged — the mints are
    // associated fns with no `self`, so a `.from_transport()` method call is always an UNRELATED
    // method on some other type (round-2 false-positive fix). The default traversal still recurses
    // into the receiver + args so a gated call nested inside is caught by `visit_expr_call`.

    // ── re-export gate: `use ... <mint>` or `use ... SendResponse as X` ──
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        if let Some(sym) = used_gated_symbol(&node.tree) {
            self.flag(format!(
                "`use` re-export / alias of gated authority symbol `{sym}` (laundering vector)"
            ));
        }
        syn::visit::visit_item_use(self, node);
    }

    // ── type-alias gate: `type X = SendResponse|SendOutcome;` ──
    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        if is_test_exempt(&node.attrs) {
            return;
        }
        if let Some(ty) = type_last_seg(&node.ty) {
            if DELIVERY_TYPES.contains(&ty.as_str()) {
                self.flag(format!(
                    "`type {} = {ty}` aliases a delivery authority type (ctor-call bypass)",
                    node.ident
                ));
            }
        }
        syn::visit::visit_item_type(self, node);
    }

    // ── macro laundering: token-tree mentions a mint ident OR a `Type::ctor` delivery pattern ──
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        // Round-2 B5: `include!("x.inc")` pulls source the .rs walker never sees → a mint/ctor in the
        // included file is invisible. Forbid production `include!` in the guarded area (there is none
        // today). Same class: `#[path]` mod (see visit_item_mod). syn cannot resolve either target.
        if node.path.is_ident("include") {
            self.flag(
                "`include!(...)` in guarded production code — the included source is not scanned; \
                 inline it or move the file under the normal module tree"
                    .to_string(),
            );
        }
        if let Some(what) = macro_tokens_mention_gated(&node.tokens.to_string()) {
            self.flag(format!(
                "macro invocation token-tree mentions gated symbol `{what}` (macro laundering)"
            ));
        }
        syn::visit::visit_macro(self, node);
    }

    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        if is_test_exempt(&node.attrs) {
            return;
        }
        if let Some(what) = macro_tokens_mention_gated(&node.mac.tokens.to_string()) {
            self.flag(format!(
                "macro_rules! definition token-tree emits gated symbol `{what}` (macro-export laundering)"
            ));
        }
        syn::visit::visit_item_macro(self, node);
    }
}

/// The gated symbol a `use` tree names, if any: a mint ident, OR a RENAMED delivery type
/// (`SendResponse as X`). A plain un-renamed delivery-type import (`use ...::SendResponse;`) is
/// legitimate — services name the type in signatures — and is NOT flagged.
fn used_gated_symbol(tree: &syn::UseTree) -> Option<String> {
    match tree {
        syn::UseTree::Path(p) => used_gated_symbol(&p.tree),
        syn::UseTree::Name(n) => mint_from_ident(&n.ident).map(|m| m.label()),
        syn::UseTree::Rename(r) => {
            // A renamed mint ident, OR a renamed delivery TYPE.
            if let Some(m) = mint_from_ident(&r.ident) {
                return Some(m.label());
            }
            let id = r.ident.to_string();
            if DELIVERY_TYPES.contains(&id.as_str()) {
                return Some(format!("{id} as {}", r.rename));
            }
            None
        }
        syn::UseTree::Group(g) => g.items.iter().find_map(used_gated_symbol),
        syn::UseTree::Glob(_) => None,
    }
}

/// If a (space-normalized) macro token string mentions a gated mint ident or a `Type::ctor`
/// delivery pattern, return the mentioned symbol label. Conservative syntactic ban (syn does not
/// expand macros).
fn macro_tokens_mention_gated(raw: &str) -> Option<String> {
    // Mint idents: match the token as a word-ish occurrence. Because `from_transport_digest`
    // contains `from_transport`, check the longer one first so the label is precise.
    for m in MINT_SYMBOLS {
        if raw.contains(m) {
            return Some((*m).to_string());
        }
    }
    let compact = raw.replace(' ', "");
    for (t, c) in DELIVERY_CTORS {
        if compact.contains(&format!("{t}::{c}")) {
            return Some(format!("{t}::{c}"));
        }
    }
    // Round-2 B2: split-token laundering — a macro that reassembles `<$ty>::$ctor(...)` receives the
    // type and ctor as SEPARATE tokens (e.g. `invoke!(SendResponse, no_response, arg)`), so the
    // `Type::ctor` adjacency check above misses it. Conservatively flag any macro token-tree that
    // mentions BOTH a delivery authority TYPE and a delivery CTOR ident. This is a HEURISTIC (may
    // over-flag; syn cannot expand the macro to know the real reassembly) — see the module limits.
    let mentions_type = DELIVERY_TYPES.iter().find(|t| compact.contains(**t));
    let mentions_ctor = DELIVERY_CTORS.iter().find(|(_, c)| compact.contains(c));
    if let (Some(t), Some((_, c))) = (mentions_type, mentions_ctor) {
        return Some(format!("split-token {t} + {c}"));
    }
    None
}

// ─── file walking + scan entry ─────────────────────────────────────────────────

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

/// Scan one source text. A PARSE FAILURE is a VIOLATION (fail-closed), never a silent skip.
fn scan_source(text: &str, file_name: &str) -> Vec<Violation> {
    let parsed = match syn::parse_file(text) {
        Ok(f) => f,
        Err(e) => {
            return vec![Violation {
                file: file_name.to_string(),
                detail: format!(
                    "file failed to parse ({e}) — the authority-flow gate fails CLOSED on unparseable source"
                ),
            }];
        }
    };
    let mut scan = AuthorityScan::new(file_name);
    scan.visit_file(&parsed);
    scan.violations
}

// ─── (1) the production scan ────────────────────────────────────────────────────

#[test]
fn production_src_authority_flow_gate_holds() {
    // Round-2 B4: scan the OWNING crate too — `prro-domain/src` DEFINES the authority ctors + mints
    // (pub cross-crate), so a forwarding wrapper THERE (invisible to a prro-only scan) could
    // fabricate. The workspace root is the manifest dir's parent (`rust/`).
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest
        .parent()
        .expect("crate dir has a parent (workspace root)");
    let roots = [
        manifest.join("src"),
        workspace.join("prro-domain").join("src"),
    ];

    let mut all = Vec::new();
    for root in &roots {
        let mut files = Vec::new();
        collect_rs_files(root, &mut files);
        assert!(
            !files.is_empty(),
            "expected .rs files under {}",
            root.display()
        );
        for path in &files {
            let text = fs::read_to_string(path).expect("read .rs");
            // Relative path (forward-slash normalized) so the file-suffix allowlist match works.
            let rel = path
                .strip_prefix(workspace)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            all.extend(scan_source(&text, &rel));
        }
    }

    assert!(
        all.is_empty(),
        "authority-flow gate found {} violation(s). A gated mint / delivery ctor may appear in \
         non-test code ONLY as a DIRECT call inside its allowlisted (file, fn) site. If this is a \
         legitimately new production call site, ADD it to ALLOWLIST; otherwise it is a real leak \
         (capture / wrapper / macro / re-export / cfg(not(test))). Violations: {:#?}",
        all.len(),
        all
    );
}

// ─── (2) canaries — synthetic snippets that MUST be caught ──────────────────────
//
// For "must be caught regardless of file" canaries we use a file_name that is NOT an allowlisted
// suffix (`<attack>`), so the (file, fn) allowlist can never match → violation.

const ATTACK_FILE: &str = "<attack>";

#[test]
fn fn_pointer_capture_of_mint_is_caught() {
    // Auditor bypass #1 (mint half): capturing the mint as a VALUE, never calling it directly.
    let snippet = r#"
        fn forge() {
            let f = NonEmptyFiscalNumber::from_transport;
            let _ = f;
        }
    "#;
    let v = scan_source(snippet, ATTACK_FILE);
    assert!(
        v.iter().any(
            |x| x.detail.contains("from_transport") && x.detail.contains("captured as a VALUE")
        ),
        "capturing the provenance mint as a fn-pointer must be caught: {v:#?}"
    );
}

#[test]
fn fn_pointer_capture_of_delivery_ctor_is_caught() {
    // Auditor bypass #1 (delivery half): `let h = SendResponse::parsed;` as a value.
    let snippet = r#"
        fn forge() {
            let h = SendResponse::parsed;
            let _ = h;
        }
    "#;
    let v = scan_source(snippet, ATTACK_FILE);
    assert!(
        v.iter().any(|x| x.detail.contains("SendResponse::parsed")
            && x.detail.contains("captured as a VALUE")),
        "capturing a delivery ctor as a fn-pointer must be caught: {v:#?}"
    );
}

#[test]
fn full_forge_chain_via_captures_is_caught() {
    // The auditor's full forge: three captures compose an `Accepted` out of a forged string. Every
    // captured ExprPath is a violation (the callee-of-a-call recursion never routes captures to the
    // call gate). We require ALL THREE gated captures to be caught.
    let snippet = r#"
        fn forge() {
            let f = NonEmptyFiscalNumber::from_transport;
            let g = SendOutcome::accepted;
            let h = SendResponse::parsed;
            let _ = h(g(f("FORGED".into()).unwrap()));
        }
    "#;
    let v = scan_source(snippet, ATTACK_FILE);
    let caught = |needle: &str| v.iter().any(|x| x.detail.contains(needle));
    assert!(
        caught("from_transport")
            && caught("SendOutcome::accepted")
            && caught("SendResponse::parsed"),
        "the 3-capture Accepted forge must flag all three gated captures: {v:#?}"
    );
}

#[test]
fn cfg_not_test_direct_ctor_is_caught() {
    // Auditor bypass #2: a direct ctor under `#[cfg(not(test))]` — production-only, NOT exempt. The
    // structural cfg parse must NOT be fooled by the substring `test` inside `not(test)`.
    let snippet = r#"
        #[cfg(not(test))]
        fn prod_only(o: SendOutcome) -> SendResponse {
            SendResponse::parsed(o)
        }
    "#;
    let v = scan_source(snippet, ATTACK_FILE);
    assert!(
        v.iter().any(|x| x.detail.contains("SendResponse::parsed")),
        "a direct ctor under #[cfg(not(test))] must be caught (not exempt): {v:#?}"
    );
}

#[test]
fn decoder_macro_export_mint_is_caught() {
    // Auditor bypass #3: a `#[macro_export] macro_rules!` emitting the mint, invoked bare from
    // services. syn does not expand it — the token-tree ban catches the DEFINITION.
    let snippet = r#"
        #[macro_export]
        macro_rules! mint_for_engine {
            ($s:expr) => { $crate::delivery::NonEmptyFiscalNumber::from_transport($s) };
        }
    "#;
    let v = scan_source(snippet, ATTACK_FILE);
    assert!(
        v.iter()
            .any(|x| x.detail.contains("laundering") && x.detail.contains("from_transport")),
        "a #[macro_export] macro_rules! emitting the mint must be caught: {v:#?}"
    );
}

#[test]
fn forwarding_wrapper_in_allowed_file_is_caught() {
    // Auditor bypass #4: a fn NAMED other than `map_send_reply` that constructs a `SendResponse`.
    // Even with the ALLOWLISTED file suffix, the enclosing-fn name is wrong → the (file, fn) site is
    // NOT allowlisted → violation. (We use the real allowed suffix to prove the fn-name axis bites.)
    let snippet = r#"
        fn sneaky_wrapper(o: SendOutcome) -> SendResponse {
            SendResponse::parsed(o)
        }
    "#;
    let v = scan_source(snippet, "services/write_path/shadow_map.rs");
    assert!(
        v.iter()
            .any(|x| x.detail.contains("SendResponse::parsed")
                && x.detail.contains("NON-allowlisted")),
        "a forwarding wrapper (wrong fn) inside the allowlisted file must be caught: {v:#?}"
    );
}

#[test]
fn qself_delivery_ctor_is_caught() {
    // `<SendResponse>::parsed(o)` — syn puts the type in qself, path is single-segment `[parsed]`.
    let snippet = r#"
        fn forge(o: SendOutcome) -> SendResponse {
            <SendResponse>::parsed(o)
        }
    "#;
    let v = scan_source(snippet, ATTACK_FILE);
    assert!(
        v.iter().any(|x| x.detail.contains("SendResponse::parsed")),
        "qself-form <SendResponse>::parsed must be caught: {v:#?}"
    );
}

#[test]
fn aliased_use_of_delivery_type_is_caught() {
    // `use ...SendResponse as SR;` — aliasing the type so `SR::parsed` bypasses the two-seg match.
    let snippet = r#"
        use prro_domain::delivery::SendResponse as SR;
    "#;
    let v = scan_source(snippet, ATTACK_FILE);
    assert!(
        v.iter()
            .any(|x| x.detail.contains("SendResponse") && x.detail.contains("re-export / alias")),
        "aliased `use SendResponse as SR` must be caught: {v:#?}"
    );
}

#[test]
fn aliased_use_of_mint_is_caught() {
    // A `use ... from_transport;` re-export of a mint ident.
    let snippet = r#"
        use prro_domain::delivery::NonEmptyFiscalNumber::from_transport;
    "#;
    let v = scan_source(snippet, ATTACK_FILE);
    assert!(
        v.iter()
            .any(|x| x.detail.contains("from_transport") && x.detail.contains("re-export / alias")),
        "a `use` re-export of the mint must be caught: {v:#?}"
    );
}

#[test]
fn macro_invocation_delivery_ctor_is_caught() {
    let snippet = r#"
        fn f(o: SendOutcome) {
            let _ = some_macro!(SendResponse::parsed(o));
        }
    "#;
    let v = scan_source(snippet, ATTACK_FILE);
    assert!(
        v.iter()
            .any(|x| x.detail.contains("SendResponse::parsed") && x.detail.contains("laundering")),
        "a macro invocation emitting a delivery ctor must be caught: {v:#?}"
    );
}

#[test]
fn parse_error_fails_gate() {
    // Fail-closed: an unparseable file yields a VIOLATION, never a silent skip.
    let snippet = r#"
        fn broken( {
            this is not valid rust @@@ ###
    "#;
    let v = scan_source(snippet, ATTACK_FILE);
    assert!(
        v.iter().any(|x| x.detail.contains("failed to parse")),
        "an unparseable source must fail the gate closed: {v:#?}"
    );
}

// ── Round-2 auditor canaries (5 NEW bypasses on real production code) ──

#[test]
fn split_token_macro_ctor_is_caught() {
    // `invoke!(SendResponse, no_response, arg)` reassembles `<$ty>::$ctor(arg)` — the type and ctor
    // reach the invocation as SEPARATE tokens, so the `Type::ctor` adjacency check misses. The
    // split-token heuristic (mentions a delivery TYPE and a delivery CTOR) must catch it.
    let snippet = r#"
        fn forge() -> SendResponse {
            invoke!(SendResponse, no_response, NoResponseCause::Timeout)
        }
    "#;
    let v = scan_source(snippet, "services/write_path/other.rs");
    assert!(
        v.iter().any(|x| x.detail.contains("split-token")),
        "split-token macro must be caught: {v:#?}"
    );
}

#[test]
fn generic_identity_alias_delivery_ctor_is_caught() {
    // `<Identity<SendResponse>>::parsed(o)` hides the real type in a generic arg — the recursive
    // generic-arg walk must find SendResponse.
    let snippet = r#"
        fn forge(o: SendOutcome) -> SendResponse {
            <Identity<SendResponse>>::parsed(o)
        }
    "#;
    let v = scan_source(snippet, "services/write_path/other.rs");
    assert!(
        v.iter().any(|x| x.detail.contains("SendResponse::parsed")),
        "generic-identity-alias qself ctor must be caught: {v:#?}"
    );
}

#[test]
fn production_include_macro_is_caught() {
    // `include!("x.inc")` pulls source the .rs walker never scans → forbidden in the guarded area.
    let snippet = r#"
        include!("audit_authority.inc");
    "#;
    let v = scan_source(snippet, "services/write_path/other.rs");
    assert!(
        v.iter().any(|x| x.detail.contains("include!")),
        "production include! must be caught: {v:#?}"
    );
}

// ─── (3) negative controls — must NOT be flagged ────────────────────────────────

#[test]
fn genuine_cfg_test_mint_is_exempt() {
    // A real test double in `#[cfg(test)] mod` legitimately mints — exempt.
    let snippet = r#"
        #[cfg(test)]
        mod t {
            fn f() {
                let _ = DecodedResponseDigest::from_transport_digest([0u8; 32]);
                let _ = NonEmptyFiscalNumber::from_transport("DPS-1".to_string());
                let _ = SendResponse::parsed(SendOutcome::accepted(id));
            }
        }
    "#;
    let v = scan_source(snippet, ATTACK_FILE);
    assert!(
        v.is_empty(),
        "a genuine #[cfg(test)] mint / ctor must be exempt: {v:#?}"
    );
}

#[test]
fn cfg_any_test_or_feature_is_not_exempt() {
    // Round-2 B1: `#[cfg(any(test, feature = "x"))]` is production-reachable (compiles when `x` is
    // on WITHOUT test), so it does NOT logically require test → the mint inside is a VIOLATION.
    let snippet = r#"
        #[cfg(any(test, feature = "x"))]
        fn helper() {
            let _ = DecodedResponseDigest::from_transport_digest([0u8; 32]);
        }
    "#;
    let v = scan_source(snippet, ATTACK_FILE);
    assert!(
        !v.is_empty(),
        "cfg(any(test, feature)) is production-reachable and must NOT be exempt"
    );
}

#[test]
fn cfg_all_test_and_feature_is_exempt() {
    // `all(test, feature)` compiles ONLY when test is on → genuinely test-only → exempt.
    let snippet = r#"
        #[cfg(all(test, feature = "x"))]
        fn helper() {
            let _ = DecodedResponseDigest::from_transport_digest([0u8; 32]);
        }
    "#;
    let v = scan_source(snippet, ATTACK_FILE);
    assert!(
        v.is_empty(),
        "all(test, ...) requires test → must be exempt: {v:#?}"
    );
}

#[test]
fn allowlist_helper_allows_real_sites() {
    // The allowlist check itself, exercised directly (the production scan is the full proof; this
    // pins the helper so a bad edit to ALLOWLIST is caught in isolation).
    assert!(is_allowed(
        "prro/src/services/write_path/shadow_map.rs",
        Some("map_send_reply"),
        &GatedSymbol::Delivery("SendResponse", "parsed"),
    ));
    assert!(is_allowed(
        "prro/src/transports/dps/dto.rs",
        Some("decoded_digest_check"),
        &GatedSymbol::Mint("from_transport_digest"),
    ));
    assert!(is_allowed(
        "prro/src/transports/dps/dto.rs",
        Some("raw_reply_from_check_response"),
        &GatedSymbol::Mint("from_transport"),
    ));
    assert!(is_allowed(
        "prro/src/transports/dps/raw_reply.rs",
        Some("observe_from_legacy"),
        &GatedSymbol::Mint("from_transport"),
    ));
    assert!(is_allowed(
        "prro/src/transports/dps/grpc.rs",
        Some("status_digest"),
        &GatedSymbol::Mint("from_transport_digest"),
    ));
}

#[test]
fn allowlist_helper_rejects_wrong_axis() {
    // Right file, WRONG fn → not allowed (the forwarding-wrapper axis).
    assert!(!is_allowed(
        "prro/src/services/write_path/shadow_map.rs",
        Some("sneaky_wrapper"),
        &GatedSymbol::Delivery("SendResponse", "parsed"),
    ));
    // Right fn name, WRONG file → not allowed.
    assert!(!is_allowed(
        "prro/src/services/other.rs",
        Some("map_send_reply"),
        &GatedSymbol::Delivery("SendResponse", "parsed"),
    ));
    // Right mint site file+fn, WRONG symbol (delivery ctor at a mint-only site) → not allowed.
    assert!(!is_allowed(
        "prro/src/transports/dps/dto.rs",
        Some("decoded_digest_check"),
        &GatedSymbol::Delivery("SendResponse", "parsed"),
    ));
    // No enclosing fn → never allowed.
    assert!(!is_allowed(
        "prro/src/services/write_path/shadow_map.rs",
        None,
        &GatedSymbol::Delivery("SendResponse", "parsed"),
    ));
}

#[test]
fn plain_type_import_not_flagged() {
    // A plain (un-renamed) delivery type import + a signature naming it is legitimate.
    let snippet = r#"
        use prro_domain::delivery::SendResponse;
        fn sig(x: &SendResponse) -> Option<SendResponse> { None }
    "#;
    let v = scan_source(snippet, ATTACK_FILE);
    assert!(
        v.is_empty(),
        "a plain type import + signature must not be flagged: {v:#?}"
    );
}

#[test]
fn carrying_a_digest_not_flagged() {
    // Reading a digest / provenance out of an opaque value (as_bytes / as_str / passing on) is fine
    // — only the MINT ctor is gated, not carrying the value.
    let snippet = r#"
        fn carry(d: DecodedResponseDigest, id: &NonEmptyFiscalNumber) -> ([u8; 32], String) {
            (*d.as_bytes(), id.as_str().to_string())
        }
    "#;
    let v = scan_source(snippet, ATTACK_FILE);
    assert!(
        v.is_empty(),
        "carrying a digest/id value must not be flagged: {v:#?}"
    );
}

#[test]
fn unrelated_ctor_name_not_flagged() {
    // A `parsed`/`accepted` on ANOTHER type, or a bare fn call, must not false-positive — the
    // delivery match keys on the two-segment `SendResponse::`/`SendOutcome::` pair (or qself/alias).
    let snippet = r#"
        fn other(json: Value) {
            let _ = SomeOther::parsed(x);
            let _ = json.accepted();
            let _ = missing_status(y);
            let _ = StageSendOutcome::accepted(z);
        }
    "#;
    let v = scan_source(snippet, ATTACK_FILE);
    assert!(
        v.is_empty(),
        "unrelated parsed/accepted idents (incl. StageSendOutcome) must not be flagged: {v:#?}"
    );
}

// ─── (M3) live-seam tripwire — the mapper stays wired into the production send path ──────────────
//
// Consolidated-audit M3: the §4.6 drift-pin proves `map_send_reply` AGREES with the live
// `route_send_result` FOR ONE decode, but it constructs both sides itself — so DELETING the live
// `map_send_reply` call from `stage_send.rs` (the shadow drives nothing in 3.2) left every test
// GREEN. That call is the D/E TRIPWIRE: in Bridge + D/E the shadow becomes load-bearing (the
// authoritative record), so the seam MUST survive intact until it is deliberately promoted. This AST
// scan fails CLOSED if the live call disappears or its evidence source changes shape.
//
// The pin is intentionally tight (it asserts the exact live shape `map_send_reply(
// wire_observation.evidence(), ..)`), so removing OR silently rewiring the seam is a RED that forces
// a conscious update here — which is the whole point of a tripwire. Test code is exempt (the
// drift-pin's own `map_send_reply` calls live in `grpc.rs`, a different file, and are `#[test]`).

struct SeamScan {
    /// non-test call-sites to `map_send_reply` in this file, any argument shape.
    any_calls: usize,
    /// of those, the ones whose first argument is exactly `wire_observation.evidence()` (the live
    /// transport-minted observation) — the production seam we pin.
    live_calls: usize,
}

/// `true` iff `arg` is the method call `wire_observation.evidence()` (structural, quote-free).
fn arg_is_live_observation_evidence(arg: &syn::Expr) -> bool {
    if let syn::Expr::MethodCall(mc) = arg {
        if mc.method == "evidence" && mc.args.is_empty() {
            if let syn::Expr::Path(p) = &*mc.receiver {
                return p.path.is_ident("wire_observation");
            }
        }
    }
    false
}

impl<'ast> Visit<'ast> for SeamScan {
    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        if is_test_exempt(&f.attrs) {
            return;
        }
        syn::visit::visit_item_fn(self, f);
    }
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if is_test_exempt(&m.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, m);
    }
    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        if is_test_exempt(&f.attrs) {
            return;
        }
        syn::visit::visit_impl_item_fn(self, f);
    }
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*call.func {
            let is_mapper = p
                .path
                .segments
                .last()
                .map(|s| s.ident == "map_send_reply")
                .unwrap_or(false);
            if is_mapper {
                self.any_calls += 1;
                if call.args.first().is_some_and(arg_is_live_observation_evidence) {
                    self.live_calls += 1;
                }
            }
        }
        syn::visit::visit_expr_call(self, call);
    }
}

#[test]
fn m3_map_send_reply_stays_wired_into_stage_send() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let stage_send = manifest.join("src/services/write_path/stage_send.rs");
    let text = fs::read_to_string(&stage_send).expect("read stage_send.rs");
    let parsed = syn::parse_file(&text).expect("stage_send.rs parses");

    let mut scan = SeamScan {
        any_calls: 0,
        live_calls: 0,
    };
    scan.visit_file(&parsed);

    assert!(
        scan.any_calls >= 1,
        "the live `map_send_reply` call was REMOVED from stage_send.rs — the CS-3 3.2 read-only \
         shadow seam is the D/E tripwire and must stay wired until deliberately promoted. If this \
         removal is intentional (the shadow is being promoted / retired), update this tripwire."
    );
    assert_eq!(
        scan.live_calls, 1,
        "expected exactly ONE live `map_send_reply(wire_observation.evidence(), ..)` seam in \
         stage_send.rs, found {} (any-shape count {}). The seam was rewired to a different \
         evidence source — confirm it still forwards the single-RPC transport observation and \
         update this pin.",
        scan.live_calls, scan.any_calls
    );
}
