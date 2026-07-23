//! S7-P2-2 (design §8) — static SOLE-SEAM pin for the CS-3 S7-1 live cutover.
//!
//! After the cutover the double-issue guarantee **P2 (≤1 wire per document lifetime)** rests on there
//! being exactly ONE production wire seam. This static scan pins that seam STRUCTURALLY (syn AST, not
//! grep) so a future edit that re-introduces a second wire path fails CI:
//!
//!   - `send_chk_observed` (the sole DPS wire method) has EXACTLY ONE production call site, and it is
//!     inside `services/write_path/submit.rs` (the file that defines `submit_authorized`, and nothing
//!     else — so file-granularity == "inside `submit_authorized`").
//!   - `submit_authorized` (the sole wire function) has EXACTLY ONE production caller, and it is inside
//!     `services/write_path/stage_send.rs` (`stage_send::run`).
//!
//! **RED-FIRST (pre-cutover, expected to FAIL today).** Before the atomic cutover:
//!   - `send_chk_observed` is called from TWO sites — the legacy `stage_send.rs:1568` 4a wire AND
//!     `submit.rs:81` (inside the still-INACTIVE `submit_authorized`) → the "exactly one" assert FAILS.
//!   - `submit_authorized` has ZERO production callers (INACTIVE foundation) → the "exactly one" assert
//!     FAILS.
//!
//! Both assertions go GREEN only when the cutover relocates the wire behind `submit_authorized`
//! (deleting the `stage_send.rs:1568` site) and wires `submit_authorized` into `stage_send::run`.
//! This is the intended bite — the pin is a load-bearing part of the atomic cutover commit.
//!
//! Mirrors the syn-2 visitor of `with_immediate_no_foreign_io.rs` (method-call + UFCS call detection)
//! and the `#[cfg(test)]` structural exclusion of `support/inactive_lifecycle_scan.rs` (test
//! scaffolding is not a production caller). Definitions (`fn send_chk_observed` / `fn submit_authorized`,
//! trait decls, impls) are `ItemFn`/`ImplItemFn`/`TraitItemFn` — never `Expr::Call`/`Expr::MethodCall`
//! — so they are not miscounted as call sites; no defining-file exclusion is needed.

use std::fs;
use std::path::{Path, PathBuf};

use syn::visit::Visit;

const SUBMIT_RS: &str = "src/services/write_path/submit.rs";
const STAGE_SEND_RS: &str = "src/services/write_path/stage_send.rs";

/// One production call site (method-call or free-fn call) to a scanned symbol.
#[derive(Debug, Clone)]
struct CallSite {
    /// The `src`-relative file the call site lives in.
    file: String,
}

/// `true` for `#[cfg(test)]` EXACTLY (mirrors `support/inactive_lifecycle_scan.rs`).
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

struct SoleSeamScan<'a> {
    file: &'a str,
    /// method-call sites `expr.send_chk_observed(...)` + UFCS `Trait::send_chk_observed(...)`.
    send_chk_observed: Vec<CallSite>,
    /// call sites `submit_authorized(...)` / `submit::submit_authorized(...)`.
    submit_authorized: Vec<CallSite>,
    /// method-call sites `expr.send_chk(...)` + UFCS `Trait::send_chk(...)` — the OTHER submit-RPC
    /// (public `DpsChannel::send_chk`). Scanned so a future write-path bypass around
    /// `send_chk_observed` (whose provenance/observation seal the P2 sole-wire pin depends on) fails
    /// CI. The name match is EXACT (`== "send_chk"`), so `send_chk_observed` is NOT double-counted.
    send_chk: Vec<CallSite>,
}

impl SoleSeamScan<'_> {
    fn push_wire(&mut self) {
        self.send_chk_observed.push(CallSite {
            file: self.file.to_string(),
        });
    }
    fn push_submit(&mut self) {
        self.submit_authorized.push(CallSite {
            file: self.file.to_string(),
        });
    }
    fn push_send_chk(&mut self) {
        self.send_chk.push(CallSite {
            file: self.file.to_string(),
        });
    }
}

impl<'ast> Visit<'ast> for SoleSeamScan<'_> {
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

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if node.method == "send_chk_observed" {
            self.push_wire();
        }
        // EXACT match — `send_chk_observed` above does NOT fall through here.
        if node.method == "send_chk" {
            self.push_send_chk();
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*node.func {
            if let Some(last) = p.path.segments.last() {
                // UFCS / associated-function form of the wire method
                // (`DpsChannel::send_chk_observed(&chan, env)`) is an `Expr::Call`, not
                // `Expr::MethodCall` — catch it on the last path segment.
                if last.ident == "send_chk_observed" {
                    self.push_wire();
                }
                // The OTHER submit-RPC, UFCS form (`DpsChannel::send_chk(chan, env)`).
                // EXACT ident match — `send_chk_observed` does not collide.
                if last.ident == "send_chk" {
                    self.push_send_chk();
                }
                // The sole wire function, called free (`submit_authorized(...)`) or qualified
                // (`submit::submit_authorized(...)` / `write_path::submit::submit_authorized(...)`).
                if last.ident == "submit_authorized" {
                    self.push_submit();
                }
            }
        }
        syn::visit::visit_expr_call(self, node);
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

fn scan_source(text: &str, file: &str) -> (Vec<CallSite>, Vec<CallSite>, Vec<CallSite>) {
    let parsed = match syn::parse_file(text) {
        Ok(f) => f,
        Err(_) => return (Vec::new(), Vec::new(), Vec::new()),
    };
    let mut scan = SoleSeamScan {
        file,
        send_chk_observed: Vec::new(),
        submit_authorized: Vec::new(),
        send_chk: Vec::new(),
    };
    scan.visit_file(&parsed);
    (
        scan.send_chk_observed,
        scan.submit_authorized,
        scan.send_chk,
    )
}

/// Scan every production `src/**/*.rs` and return
/// (send_chk_observed sites, submit_authorized sites, send_chk sites).
fn scan_src_tree() -> (Vec<CallSite>, Vec<CallSite>, Vec<CallSite>) {
    let src_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs_files(&src_root, &mut files);
    assert!(
        !files.is_empty(),
        "expected at least one .rs file under {}",
        src_root.display()
    );
    let mut wire = Vec::new();
    let mut submit = Vec::new();
    let mut send_chk = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(env!("CARGO_MANIFEST_DIR"))
            .unwrap_or(path)
            .display()
            .to_string();
        let text = fs::read_to_string(path).expect("read .rs file");
        let (w, s, c) = scan_source(&text, &rel);
        wire.extend(w);
        submit.extend(s);
        send_chk.extend(c);
    }
    (wire, submit, send_chk)
}

// ─── The two sole-seam pins (RED-first until the atomic cutover) ──────────────

#[test]
fn production_send_chk_observed_has_exactly_one_call_site_inside_submit() {
    let (wire, _submit, _send_chk) = scan_src_tree();
    assert_eq!(
        wire.len(),
        1,
        "P2 sole-wire: `send_chk_observed` must have EXACTLY ONE production call site (the cutover \
         relocates the legacy `stage_send.rs` 4a wire behind `submit_authorized`). Found {}: {:#?}",
        wire.len(),
        wire
    );
    assert_eq!(
        wire[0].file, SUBMIT_RS,
        "the sole `send_chk_observed` call site must live inside `{SUBMIT_RS}` (== inside \
         `submit_authorized`); found it in `{}`",
        wire[0].file
    );
}

#[test]
fn production_submit_authorized_has_exactly_one_caller_in_stage_send() {
    let (_wire, submit, _send_chk) = scan_src_tree();
    assert_eq!(
        submit.len(),
        1,
        "P2 sole-seam: `submit_authorized` must have EXACTLY ONE production caller (`stage_send::run`, \
         wired at the cutover). Found {}: {:#?}",
        submit.len(),
        submit
    );
    assert_eq!(
        submit[0].file, STAGE_SEND_RS,
        "the sole `submit_authorized` caller must live inside `{STAGE_SEND_RS}` (`stage_send::run`); \
         found it in `{}`",
        submit[0].file
    );
}

/// COMPLETENESS tooth (re-audit) — the sole-wire pin above scans `send_chk_observed`, but the
/// PUBLIC `DpsChannel::send_chk` (the OTHER submit-RPC) was UN-scanned. `send_chk` returns only the
/// bare `Result<CheckAck, DpsError>` — it drops the `RawSendObservation` provenance/observation that
/// the P2 sole-wire pin (and the CS-3 evidence seal) rest on. A future write-path edit that submits
/// via `send_chk` instead of `send_chk_observed` would be a wire bypass INVISIBLE to the sole-wire
/// assert. It is NOT a current break — there are ZERO production `send_chk` call sites in the
/// write-path today (the only `self.send_chk(...)` uses live in `#[cfg(test)]` mock `DpsChannel`
/// impls, excluded by the scanner; the trait decl + grpc impl are definitions, never call sites).
/// This asserts that budget of 0 and FAILS if anyone adds a production `send_chk` submit.
#[test]
fn production_send_chk_has_zero_call_sites_no_wire_bypass() {
    let (_wire, _submit, send_chk) = scan_src_tree();
    assert_eq!(
        send_chk.len(),
        0,
        "P2 sole-wire completeness: the public `DpsChannel::send_chk` submit-RPC must have ZERO \
         production call sites (all submits go through `send_chk_observed`, which alone carries the \
         RawSendObservation the evidence seal depends on). A new `send_chk` call site is a wire \
         bypass INVISIBLE to the sole-`send_chk_observed` pin. Found {}: {:#?}",
        send_chk.len(),
        send_chk
    );
}

// ─── Synthetic controls — prove the scanner is load-bearing (independent oracle) ──────────────

#[test]
fn scanner_detects_a_synthetic_send_chk_observed_method_call() {
    let snippet = r#"
        async fn _wire(chan: &dyn DpsChannel, env: CheckEnvelope) {
            let _ = chan.send_chk_observed(env).await;
        }
    "#;
    let (wire, _s, send_chk) = scan_source(snippet, "<synthetic>");
    assert_eq!(
        wire.len(),
        1,
        "must detect a method-call wire site: {wire:#?}"
    );
    // `send_chk_observed` must NOT be miscounted as a `send_chk` (exact-match discipline).
    assert!(
        send_chk.is_empty(),
        "send_chk_observed must not count as send_chk: {send_chk:#?}"
    );
}

#[test]
fn scanner_detects_a_synthetic_send_chk_observed_ufcs_call() {
    let snippet = r#"
        async fn _wire(chan: &dyn DpsChannel, env: CheckEnvelope) {
            let _ = DpsChannel::send_chk_observed(chan, env).await;
        }
    "#;
    let (wire, _s, _c) = scan_source(snippet, "<synthetic_ufcs>");
    assert_eq!(wire.len(), 1, "must detect a UFCS wire site: {wire:#?}");
}

#[test]
fn scanner_detects_a_synthetic_send_chk_method_call() {
    // The send_chk completeness scan must detect a bare `send_chk` submit (method-call form).
    let snippet = r#"
        async fn _bypass(chan: &dyn DpsChannel, env: CheckEnvelope) {
            let _ = chan.send_chk(env).await;
        }
    "#;
    let (wire, _s, send_chk) = scan_source(snippet, "<synthetic_send_chk>");
    assert_eq!(
        send_chk.len(),
        1,
        "must detect a bare send_chk method-call site: {send_chk:#?}"
    );
    // A `send_chk` call is not a `send_chk_observed` wire site.
    assert!(
        wire.is_empty(),
        "send_chk must not count as send_chk_observed: {wire:#?}"
    );
}

#[test]
fn scanner_detects_a_synthetic_send_chk_ufcs_call() {
    // UFCS form of the bare submit-RPC (`DpsChannel::send_chk(chan, env)`).
    let snippet = r#"
        async fn _bypass(chan: &dyn DpsChannel, env: CheckEnvelope) {
            let _ = DpsChannel::send_chk(chan, env).await;
        }
    "#;
    let (_w, _s, send_chk) = scan_source(snippet, "<synthetic_send_chk_ufcs>");
    assert_eq!(
        send_chk.len(),
        1,
        "must detect a UFCS send_chk site: {send_chk:#?}"
    );
}

#[test]
fn scanner_detects_a_synthetic_submit_authorized_call() {
    let snippet = r#"
        async fn _run() {
            let _ = submit::submit_authorized(chan, &binding, auth, envelope, doc_type).await;
        }
    "#;
    let (_w, submit, _c) = scan_source(snippet, "<synthetic_submit>");
    assert_eq!(
        submit.len(),
        1,
        "must detect a submit_authorized call: {submit:#?}"
    );
}

#[test]
fn scanner_excludes_cfg_test_call_sites() {
    // A `#[cfg(test)]` module is test scaffolding, NOT a production caller — it must not count
    // toward the sole-seam budget (a unit test in `src` calling the wire is legitimate).
    let snippet = r#"
        #[cfg(test)]
        mod tests {
            async fn _t(chan: &dyn DpsChannel, env: CheckEnvelope) {
                let _ = chan.send_chk_observed(env).await;
                let _ = chan.send_chk(env).await;
                let _ = submit::submit_authorized(chan).await;
            }
        }
    "#;
    let (wire, submit, send_chk) = scan_source(snippet, "<synthetic_cfg_test>");
    assert!(
        wire.is_empty() && submit.is_empty() && send_chk.is_empty(),
        "cfg(test) call sites must be excluded: wire={wire:#?} submit={submit:#?} send_chk={send_chk:#?}"
    );
}

#[test]
fn scanner_ignores_definitions_and_comments() {
    // The trait/impl DEFINITION of the wire method and the fn DEFINITION of submit_authorized are
    // not call sites; a doc comment mentioning the names is not a call site either.
    let snippet = r#"
        /// mentions send_chk_observed, send_chk and submit_authorized in prose only.
        trait DpsChannel {
            async fn send_chk(&self, env: CheckEnvelope) -> Reply;
            async fn send_chk_observed(&self, env: CheckEnvelope) -> Reply;
        }
        pub async fn submit_authorized(chan: &dyn DpsChannel) -> Obs { todo!() }
    "#;
    let (wire, submit, send_chk) = scan_source(snippet, "<synthetic_defs>");
    assert!(
        wire.is_empty() && submit.is_empty() && send_chk.is_empty(),
        "definitions/comments must not count as call sites: wire={wire:#?} submit={submit:#?} send_chk={send_chk:#?}"
    );
}
