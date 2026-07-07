//! A′.3 — offline-surface release-gate pin.
//!
//! O1 shipped this gated OFF (tripwire asserted the gate returns Err). O2
//! flips `FULL_OFFLINE_SURFACE_READY = true` TOGETHER with the drain path +
//! the coupling-pin (`tests/offline_door_coupling.rs`). This pin is the
//! deliberate, conscious inversion of the O1 tripwire per its own contract,
//! mirroring `z_builder`'s post-flip state.

use prro::services::offline_sync::offline_surface::{
    ensure_full_offline_surface_ready, OfflineSurfaceNotReady,
};

/// O2 flip: the offline operator surface is now ENABLED — the gate returns
/// `Ok(())`, so the GO_OFFLINE / GO_ONLINE door is live. Reverting the flip
/// (`FULL_OFFLINE_SURFACE_READY = false`) makes this RED and refuses the door
/// — the ship-together coupling (no live door without the drain path).
#[test]
fn offline_surface_is_live_after_o2_flip() {
    assert_eq!(
        ensure_full_offline_surface_ready(),
        Ok::<(), OfflineSurfaceNotReady>(())
    );
}
