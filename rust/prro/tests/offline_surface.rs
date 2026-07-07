//! A′.3 PR-O1 — offline-surface release-gate pins (RP-O1-9 tripwire).
//!
//! Mirrors the Z-surface gate (`z_builder::ensure_full_z_surface_ready`):
//! `FULL_OFFLINE_SURFACE_READY` gates the operator DOOR (admin
//! GO_OFFLINE / GO_ONLINE). In O1 the door stays shut (flag = false); the
//! flip + coupling-pin land in O2 together with the drain path.

use prro::services::offline_sync::offline_surface::{
    ensure_full_offline_surface_ready, OfflineSurfaceNotReady, FULL_OFFLINE_SURFACE_READY,
};

/// RP-O1-9 tripwire: the offline door is deliberately gated in O1.
/// If someone flips the flag without O2's drain path + coupling-pin, this
/// pin breaks loudly — opening the door without a reachable drain re-opens
/// the stranded-backlog hazard.
#[test]
fn offline_door_gated_until_full_offline_surface() {
    assert!(
        !FULL_OFFLINE_SURFACE_READY,
        "FULL_OFFLINE_SURFACE_READY must stay false until O2 wires the drain path + coupling-pin"
    );
}

/// The gate returns the typed fail-closed error while the surface is gated.
#[test]
fn ensure_full_offline_surface_ready_is_err_while_gated() {
    assert_eq!(
        ensure_full_offline_surface_ready(),
        Err(OfflineSurfaceNotReady)
    );
}
