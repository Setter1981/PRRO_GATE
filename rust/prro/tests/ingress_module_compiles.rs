//! M4 W1 acceptance — the `runtime::ingress` module skeleton compiles
//! and the `IngressServer` type is reachable from the public API.
//!
//! This test deliberately does **not** wire anything up.  W1 is a pure
//! skeleton; HTTP routing arrives in W3, boot wiring in W7.  Per
//! `docs/superpowers/plans/2026-05-25-m4-ingress-plan.md` §3 W1
//! Acceptance: "binary functionally identical to HEAD".  We assert
//! that property structurally by referencing the type without
//! constructing the server task.

use prro::runtime::ingress::IngressServer;

#[test]
fn ingress_server_type_reachable() {
    // Reaching the path is the assertion.  If `runtime::ingress` were
    // not declared, or `IngressServer` not pub, this fixture would
    // fail to compile.
    let _marker: Option<IngressServer> = None;
}
