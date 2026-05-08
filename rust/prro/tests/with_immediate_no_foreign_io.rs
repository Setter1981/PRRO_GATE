//! W0-2 §9.1 cases 1 + 3 — static-scan gate for `with_immediate`
//! closure bodies.  Sibling lint to `tests/api_surface_no_db_handle.rs`
//! (W5 ADR-M2-6 scan); shares the same syn 2 + visitor pattern.
//!
//! Forbids inside any `with_immediate(...)` closure body in
//! `rust/prro/src/**/*.rs`:
//!
//! - Method calls to M2 substrate denylist (`sign_cms_detached`,
//!   `verify_dstu`, `unwrap_envelope`, `fetch_cert_by_ski`,
//!   `send_chk`, `last_chk`, `ping`, `status_rro`, `info_rro`,
//!   `by_server_fiscal_no`, `query_by_local_identity`).
//! - Function calls to `tokio::task::spawn_blocking` /
//!   `tokio::task::block_in_place` (matched on the last path
//!   segment so both qualified `tokio::task::spawn_blocking(...)`
//!   and bare `spawn_blocking(...)` after `use tokio::task::*` are
//!   caught).
//!
//! Detection runs on **call expressions**, not just `.await` paths —
//! `let fut = provider.sign_cms_detached(...); fut.await` and
//! `let h = tokio::task::spawn_blocking(...); h.await` are both
//! caught at the call expression even though the `.await` lives a
//! line later.
//!
//! Positive controls (cases 1 and 3) live as **synthetic** snippets
//! parsed inline by the harness, so the production scan over
//! `rust/prro/src/` stays GREEN — we never commit a real forbidden
//! call site to source just to drive a positive test.

use std::fs;
use std::path::{Path, PathBuf};

use syn::visit::Visit;

const SUBSTRATE_METHODS: &[&str] = &[
    "sign_cms_detached",
    "verify_dstu",
    "unwrap_envelope",
    "fetch_cert_by_ski",
    "send_chk",
    "last_chk",
    "ping",
    "status_rro",
    "info_rro",
    "by_server_fiscal_no",
    "query_by_local_identity",
];

const FORBIDDEN_FUNCTIONS: &[&str] = &["spawn_blocking", "block_in_place"];

#[derive(Debug, Clone)]
struct Violation {
    /// Read structurally only via `Debug` in the assert message; clippy
    /// would otherwise flag this as dead code since no code path
    /// accesses `.file` directly.
    #[allow(dead_code)]
    file: String,
    detail: String,
}

/// Walks one closure body and records every forbidden call expression.
struct ClosureBodyScan<'a> {
    file: &'a str,
    violations: Vec<Violation>,
}

impl<'ast, 'a> Visit<'ast> for ClosureBodyScan<'a> {
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        let name = node.method.to_string();
        if SUBSTRATE_METHODS.contains(&name.as_str()) {
            self.violations.push(Violation {
                file: self.file.to_string(),
                detail: format!(
                    "M2 substrate method `.{name}(...)` called inside with_immediate closure body"
                ),
            });
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path_expr) = &*node.func {
            if let Some(last) = path_expr.path.segments.last() {
                let name = last.ident.to_string();
                if FORBIDDEN_FUNCTIONS.contains(&name.as_str()) {
                    self.violations.push(Violation {
                        file: self.file.to_string(),
                        detail: format!(
                            "forbidden function `{name}(...)` called inside with_immediate closure body"
                        ),
                    });
                }
                // Substrate methods can also be invoked via UFCS /
                // associated-function syntax — `CryptoProvider::
                // fetch_cert_by_ski(&p, ...)` is `Expr::Call`, not
                // `Expr::MethodCall`.  Match on last path segment so
                // both `Trait::method(...)` and `Type::method(...)`
                // are caught.
                if SUBSTRATE_METHODS.contains(&name.as_str()) {
                    self.violations.push(Violation {
                        file: self.file.to_string(),
                        detail: format!(
                            "M2 substrate method `{name}(...)` called via UFCS / associated-function syntax inside with_immediate closure body"
                        ),
                    });
                }
            }
        }
        syn::visit::visit_expr_call(self, node);
    }
}

/// Walks one .rs file, locates every `with_immediate(...)` call, and
/// invokes [`ClosureBodyScan`] on the closure argument.
struct WithImmediateFinder<'a> {
    file: &'a str,
    violations: Vec<Violation>,
}

impl<'ast, 'a> Visit<'ast> for WithImmediateFinder<'a> {
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        let is_with_immediate = matches!(
            &*node.func,
            syn::Expr::Path(p)
                if p.path.segments.last()
                    .map(|s| s.ident == "with_immediate")
                    .unwrap_or(false)
        );
        if is_with_immediate && node.args.len() >= 2 {
            let closure_arg = &node.args[1];
            // Refuse non-inline closure arguments.  `let f = move
            // |c| ...; with_immediate(pool, f)` would otherwise
            // bypass the static scan because the body lives in `f`,
            // not in the call expression — the scanner cannot
            // resolve local bindings.  Inline `move |c| ...` is
            // the only shape we can structurally walk.
            if !matches!(closure_arg, syn::Expr::Closure(_)) {
                self.violations.push(Violation {
                    file: self.file.to_string(),
                    detail: "with_immediate closure argument must be inline (Expr::Closure); \
                             non-inline form (variable, function pointer, etc.) bypasses the \
                             static scan over the closure body"
                        .to_string(),
                });
            } else {
                let mut scan = ClosureBodyScan {
                    file: self.file,
                    violations: Vec::new(),
                };
                scan.visit_expr(closure_arg);
                self.violations.extend(scan.violations);
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

fn scan_source(text: &str, file_name: &str) -> Vec<Violation> {
    let parsed = match syn::parse_file(text) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let mut finder = WithImmediateFinder {
        file: file_name,
        violations: Vec::new(),
    };
    finder.visit_file(&parsed);
    finder.violations
}

#[test]
fn production_src_has_no_foreign_io_inside_with_immediate() {
    let src_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs_files(&src_root, &mut files);
    assert!(
        !files.is_empty(),
        "expected at least one .rs file under {}",
        src_root.display()
    );

    let mut all_violations = Vec::new();
    for path in &files {
        let text = fs::read_to_string(path).expect("read .rs file");
        let rel = path
            .strip_prefix(env!("CARGO_MANIFEST_DIR"))
            .unwrap_or(path)
            .display()
            .to_string();
        all_violations.extend(scan_source(&text, &rel));
    }

    assert!(
        all_violations.is_empty(),
        "with_immediate closure bodies must not call M2 substrate methods or \
         spawn_blocking/block_in_place — violations: {all_violations:#?}"
    );
}

// ─── Synthetic positive controls (W0-2 §9.1 cases 1 + 3) ──────────────

#[test]
fn case_1_substrate_method_inside_closure_is_caught() {
    // Case 1 from W0-2 §9.1 — direct M2-substrate `.await` inside a
    // `with_immediate` closure body.  Awaited inline.
    let snippet = r#"
        async fn _scenario(pool: &sqlx::SqlitePool, provider: &dyn Stub) {
            with_immediate(pool, move |_tx| {
                Box::pin(async move {
                    let _ = provider.sign_cms_detached(req).await;
                    Ok(())
                })
            })
            .await
            .unwrap();
        }
    "#;
    let violations = scan_source(snippet, "<case_1>");
    assert!(
        violations
            .iter()
            .any(|v| v.detail.contains("sign_cms_detached")),
        "case 1 must catch direct sign_cms_detached call: {violations:#?}"
    );
}

#[test]
fn case_1_substrate_method_saved_then_awaited_is_caught() {
    // Hardening of case 1: the call expression shape is what matters,
    // not whether `.await` follows immediately.  This pattern would
    // have slipped past a `.await`-path-only scanner.
    let snippet = r#"
        async fn _scenario(pool: &sqlx::SqlitePool, provider: &dyn Stub) {
            with_immediate(pool, move |_tx| {
                Box::pin(async move {
                    let fut = provider.fetch_cert_by_ski(urls, ski, t);
                    let _ = fut.await;
                    Ok(())
                })
            })
            .await
            .unwrap();
        }
    "#;
    let violations = scan_source(snippet, "<case_1_saved>");
    assert!(
        violations
            .iter()
            .any(|v| v.detail.contains("fetch_cert_by_ski")),
        "stored-future variant must still be caught: {violations:#?}"
    );
}

#[test]
fn case_3_spawn_blocking_inside_closure_is_caught() {
    // Case 3 from W0-2 §9.1 — direct `spawn_blocking` ad-hoc inside a
    // `with_immediate` closure body.  Bypasses the M2 substrate and
    // would not be caught by the runtime guard (task-local invisible
    // inside spawn_blocking closure body).
    let snippet = r#"
        async fn _scenario(pool: &sqlx::SqlitePool) {
            with_immediate(pool, move |_tx| {
                Box::pin(async move {
                    let _ = tokio::task::spawn_blocking(|| heavy_crypto()).await;
                    Ok(())
                })
            })
            .await
            .unwrap();
        }
    "#;
    let violations = scan_source(snippet, "<case_3>");
    assert!(
        violations
            .iter()
            .any(|v| v.detail.contains("spawn_blocking")),
        "case 3 must catch tokio::task::spawn_blocking: {violations:#?}"
    );
}

#[test]
fn case_3_block_in_place_inside_closure_is_caught() {
    // Hardening: the second forbidden function in the denylist.
    let snippet = r#"
        async fn _scenario(pool: &sqlx::SqlitePool) {
            with_immediate(pool, move |_tx| {
                Box::pin(async move {
                    tokio::task::block_in_place(|| heavy_crypto());
                    Ok(())
                })
            })
            .await
            .unwrap();
        }
    "#;
    let violations = scan_source(snippet, "<case_3_block_in_place>");
    assert!(
        violations
            .iter()
            .any(|v| v.detail.contains("block_in_place")),
        "case 3 must catch tokio::task::block_in_place: {violations:#?}"
    );
}

#[test]
fn case_3_non_inline_closure_argument_is_flagged() {
    // Hardening of case 3: a sufficiently determined caller could
    // sneak `spawn_blocking` past a body-only scanner by binding the
    // closure to a local first:
    //   `let f = move |c| ...; with_immediate(pool, f)`
    // The scanner cannot resolve `f` to its definition (no
    // type / binding resolution in the syn pass).  We close this
    // bypass structurally by requiring `with_immediate`'s second
    // argument to BE an inline closure — anything else is a
    // violation in its own right.
    let snippet = r#"
        async fn _scenario(pool: &sqlx::SqlitePool) {
            let f = move |_tx| {
                Box::pin(async move {
                    let _ = tokio::task::spawn_blocking(|| heavy_crypto()).await;
                    Ok(())
                })
            };
            with_immediate(pool, f).await.unwrap();
        }
    "#;
    let violations = scan_source(snippet, "<case_3_non_inline>");
    assert!(
        violations
            .iter()
            .any(|v| v.detail.contains("must be inline")),
        "non-inline closure must be flagged structurally: {violations:#?}"
    );
}

#[test]
fn case_1_ufcs_substrate_call_is_caught() {
    // Hardening of case 1: substrate methods invoked via UFCS /
    // associated-function syntax are `Expr::Call`, not
    // `Expr::MethodCall`, and would slip past a method-call-only
    // scanner.  The `visit_expr_call` arm checks the last path
    // segment against the same SUBSTRATE_METHODS denylist.
    let snippet = r#"
        async fn _scenario(pool: &sqlx::SqlitePool, provider: InProcessProvider) {
            with_immediate(pool, move |_tx| {
                Box::pin(async move {
                    let _ = CryptoProvider::fetch_cert_by_ski(&provider, urls, ski, t).await;
                    Ok(())
                })
            })
            .await
            .unwrap();
        }
    "#;
    let violations = scan_source(snippet, "<case_1_ufcs>");
    assert!(
        violations
            .iter()
            .any(|v| v.detail.contains("fetch_cert_by_ski") && v.detail.contains("UFCS")),
        "UFCS / associated-function-style substrate call must be caught: {violations:#?}"
    );
}

#[test]
fn substrate_call_outside_with_immediate_is_not_flagged() {
    // Negative control for the static scan — substrate calls in
    // ordinary async code, not inside any `with_immediate(...)`
    // call, MUST NOT be flagged.  The scanner is scoped to closure
    // bodies of `with_immediate(...)`.
    let snippet = r#"
        async fn _scenario(provider: &dyn Stub) {
            // Ordinary substrate call — not inside any with_immediate.
            let _ = provider.sign_cms_detached(req).await;
            tokio::task::spawn_blocking(|| heavy_crypto()).await.unwrap();
        }
    "#;
    let violations = scan_source(snippet, "<negative_control>");
    assert!(
        violations.is_empty(),
        "scanner must not flag calls outside with_immediate: {violations:#?}"
    );
}
