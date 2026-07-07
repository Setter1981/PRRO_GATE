//! A′.3 PR-O1 — offline-surface release gate (the operator DOOR).
//!
//! Mirrors [`crate::runtime::ingress::z_builder::FULL_Z_SURFACE_READY`]:
//! a compile-time flag gates the operator-facing `GO_OFFLINE` / `GO_ONLINE`
//! admin commands. While `false`, those commands fail closed with
//! [`OfflineSurfaceNotReady`] — the offline machinery is proven in O1 by
//! DIRECT seam calls (`open_session` + mode-set + offline SELL/RETURN via
//! `inline::run`), NOT through the door.
//!
//! ⚠️ SHIP-TOGETHER (A′.3 slicing): the `true` flip lands in O2 TOGETHER
//! with the drain path (return-online probe + backlog drain) and a
//! coupling-pin. Opening the door WITHOUT a reachable drain re-opens the
//! stranded-backlog hazard: an operator could `GO_OFFLINE`, accrue an
//! `OFFLINE_LOCAL_ACK` backlog, and have no convergence path back to
//! `ONLINE`. Do NOT flip this to `true` until O2 wires drain + the
//! coupling-pin. The tripwire `offline_door_gated_until_full_offline_surface`
//! (`tests/offline_surface.rs`) pins this flip as the deliberate O2 release
//! decision it is.
pub const FULL_OFFLINE_SURFACE_READY: bool = false;

/// Typed fail-closed error for an operator `GO_OFFLINE` / `GO_ONLINE` command
/// attempted before the offline surface (drain path) is enabled.
/// See [`FULL_OFFLINE_SURFACE_READY`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OfflineSurfaceNotReady;

impl std::fmt::Display for OfflineSurfaceNotReady {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "offline operator surface refused: GO_OFFLINE/GO_ONLINE is gated \
             until the drain path is enabled (A′.3 O2); the door stays shut in O1"
        )
    }
}

/// O2 release gate: `Ok(())` once the offline surface (drain path) is enabled,
/// else `Err(OfflineSurfaceNotReady)`. The admin `GO_OFFLINE` / `GO_ONLINE`
/// commands MUST call this before flipping node mode.
pub fn ensure_full_offline_surface_ready() -> Result<(), OfflineSurfaceNotReady> {
    if FULL_OFFLINE_SURFACE_READY {
        Ok(())
    } else {
        Err(OfflineSurfaceNotReady)
    }
}
