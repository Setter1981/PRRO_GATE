//! A′.3 PR-O1 — offline-surface release gate (the operator DOOR).
//!
//! Mirrors [`crate::runtime::ingress::z_builder::FULL_Z_SURFACE_READY`]:
//! a compile-time flag gates the operator-facing `GO_OFFLINE` / `GO_ONLINE`
//! admin commands. While `false`, those commands fail closed with
//! [`OfflineSurfaceNotReady`] — the offline machinery is proven in O1 by
//! DIRECT seam calls (`open_session` + mode-set + offline SELL/RETURN via
//! `inline::run`), NOT through the door.
//!
//! ✅ FLIPPED in A′.3 O2 (2026-07-07): the offline operator door is LIVE. The
//! flip lands TOGETHER with the drain path (return-online probe + backlog
//! drain, driven by the supervisor loops when `supervisor.enabled = true`) and
//! the coupling-pin.
//!
//! ⚠️ COUPLING (ship-together): this `true` is CONDITIONAL on a reachable
//! drain. Opening the door WITHOUT a drain path re-opens the stranded-backlog
//! hazard — an operator could `GO_OFFLINE`, accrue an `OFFLINE_LOCAL_ACK`
//! backlog, and have no convergence path back to `ONLINE`. If the drain path
//! is ever removed/broken, flip this back to `false` in the SAME change. The
//! coupling-pin `offline_door_flip_coupled_to_reachable_drain`
//! (`tests/offline_door_coupling.rs`) asserts the drain path is reachable,
//! flag-independent; the flip pin `offline_surface_is_live_after_o2_flip`
//! (`tests/offline_surface.rs`) REDs on a flag revert.
//!
//! SCOPE (honest-dormant, per O2 runbook): the door enables offline SELL/RETURN
//! and return-online drain on an ONLINE-opened shift (canonical Pattern C).
//! Shift OPEN/CLOSE performed *under* offline (edges 2/7/9) remain fail-closed
//! until PR-O3 — an availability limit, not a fiscal hazard.
pub const FULL_OFFLINE_SURFACE_READY: bool = true;

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
