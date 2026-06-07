//! RS-2 — the `runtime::ingress` public surface is reachable + compiles.
//!
//! Supersedes the M4 W1 `IngressServer` skeleton smoke test: RS-2 replaced the
//! no-op `IngressServer` stub with the real axum server (`server`) + the
//! axum-free handler core + the D1 startup preflight.  This fixture asserts the
//! public surface other crates / the supervisor wire against is reachable;
//! reaching the paths IS the assertion (a missing/`pub`-less item fails to
//! compile here).

use prro::runtime::ingress::preflight::{preflight_d1_slots, D1PreflightError};
use prro::runtime::ingress::server::{router, serve, IngressState, MAX_BODY_BYTES};

#[test]
fn ingress_public_surface_reachable() {
    // Function items referenced as values — no need to name their axum return
    // types in this (non-axum-dep) test crate.
    let _ = router;
    let _ = serve;
    let _ = preflight_d1_slots;
    let _ = MAX_BODY_BYTES;

    // Public types are nameable.
    fn _takes_state(_s: IngressState) {}
    fn _takes_err(_e: D1PreflightError) {}
}
