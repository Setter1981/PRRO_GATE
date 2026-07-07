//! A′.3 PR-O1 — offline-surface release-gate tripwire (RP-O1-9).
//!
//! Mirrors the Z-surface gate (`z_builder::ensure_full_z_surface_ready`):
//! `FULL_OFFLINE_SURFACE_READY` gates the operator DOOR (admin
//! GO_OFFLINE / GO_ONLINE). In O1 the door stays shut; the flip + coupling-pin
//! land in O2 together with the drain path.

use prro::services::offline_sync::offline_surface::{
    ensure_full_offline_surface_ready, OfflineSurfaceNotReady,
};

/// RP-O1-9 tripwire: the offline door is deliberately gated in O1 — the gate
/// returns the typed fail-closed error while `FULL_OFFLINE_SURFACE_READY` is
/// false. Flipping the flag (without O2's drain path + coupling-pin) makes the
/// gate return `Ok(())` and REDs this pin — opening the door without a
/// reachable drain re-opens the stranded-backlog hazard. (Asserts on the gate
/// fn, not the raw const, mirroring `z_live_dispatch_is_gated_until_full_z_surface`.)
#[test]
fn offline_door_gated_until_full_offline_surface() {
    assert_eq!(
        ensure_full_offline_surface_ready(),
        Err(OfflineSurfaceNotReady),
        "the offline door must stay gated until O2 wires the drain path + coupling-pin"
    );
}
