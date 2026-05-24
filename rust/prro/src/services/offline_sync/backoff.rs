//! **M3b W12 Post-Closure Hardening Phase 4 / REC-2 (2026-05-24)** —
//! per-FN exponential backoff scheduling для offline backlog drain.
//!
//! Background: post-W12 + Phase 2 wiring, transient DPS failures result
//! in `DocVerdict::HoldFnDrain` per-doc verdicts that halt FN drain
//! per-tick (per W0b state-unchanged contract + REC-1 Tier semantics).
//! Без backoff: каждий tick (default ~60s per W8 ticker) знову намагається
//! drain — при затяжних DPS issues це produces:
//!   - 60+ DPS wire-calls/hour per stuck FN.
//!   - Audit-log volume spike (KVT2_CONFIRM_HOLD per tick × N FN's).
//!   - reqwest connection-pool socket churn.
//!
//! Per-FN backoff (REC-2) introduces a delay between drain ticks based
//! on accumulated consecutive Hold count:
//!   `next_eligible = now + min(2^consecutive_holds * 30s, 30min)`.
//!
//! - Schedule sequence: 30s → 1min → 2min → 4min → ... → 30min cap.
//! - Reset on ANY non-Hold outcome (Advance / StructuralDrift halt /
//!   drain bootstrap).
//! - Per-FN isolation: backoff on FN-A не torcha FN-B (operator memory
//!   `feedback_offline_transition_strategy` — anti-pattern: global
//!   Circuit Breaker would cascade healthy FNs into degraded state).
//! - In-memory state: HashMap<String, BackoffState> в App.  Reset на
//!   App restart per operator-pinned design choice (pragmatic — if
//!   restart, restart fresh з ticker dispatch).
//!
//! NOT a connection pool clamp — that's separate REC (config-driven
//! `reqwest::ClientBuilder::pool_max_idle_per_host`).  REC-2 scope is
//! drain-orchestrator-level backoff only.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// **REC-2 backoff state per FN**.  Stored in `App::Inner.backoff_state`
/// (HashMap<String, BackoffState>); read + written under
/// `tokio::sync::Mutex` per-FN entry.
#[derive(Debug, Clone)]
pub struct BackoffState {
    /// Count of consecutive Hold outcomes на цій FN.  Increments on
    /// each Hold tick; resets to 0 on any non-Hold outcome.  Drives
    /// the `next_eligible` computation via [`compute_next_eligible`].
    pub consecutive_holds: u32,
    /// Earliest [`Instant`] at which the next drain tick is eligible
    /// to fire on this FN.  Caller (App scheduled drain wrapper)
    /// checks `Instant::now() >= next_eligible` before invoking
    /// `backlog_drain::drain`.
    pub next_eligible: Instant,
}

impl BackoffState {
    /// Construct fresh state (zero holds, immediately eligible).
    pub fn fresh(now: Instant) -> Self {
        Self {
            consecutive_holds: 0,
            next_eligible: now,
        }
    }
}

/// **REC-2 backoff window calculator**: `min(2^consecutive_holds * 30s,
/// 30min)`.  Pure function — separate з state tracker для unit
/// testability + clean separation з time-source (caller passes `now`).
///
/// Schedule (consecutive_holds → delay):
///   - 0 → 30s (initial tick after first Hold)
///   - 1 → 1min
///   - 2 → 2min
///   - 3 → 4min
///   - 4 → 8min
///   - 5 → 16min
///   - >= 6 → 30min (cap)
///
/// Cap prevents 64min / 128min / ... runaway scheduling on extreme
/// outage scenarios; 30min cap aligns з operator-pinned 36h offline
/// window (cert.NotAfter-2160min) — ~72 retries fit within the window,
/// giving sufficient retry coverage без operator alert spam.
pub fn compute_backoff_window(consecutive_holds: u32) -> Duration {
    const BASE_SECS: u64 = 30;
    const MAX_MINS: u64 = 30;
    let cap = Duration::from_secs(MAX_MINS * 60);
    // shift-left risk: if consecutive_holds >= 64, overflow.  Guard
    // by saturating at 8 (which is well past the 30min cap anyway —
    // 2^8 * 30s = 128min > 30min cap).
    let shift = consecutive_holds.min(8);
    let secs = BASE_SECS.saturating_mul(1u64 << shift);
    let computed = Duration::from_secs(secs);
    computed.min(cap)
}

/// **REC-2 backoff transition on Hold outcome**: increment counter +
/// compute new `next_eligible`.  Caller invokes after observing
/// HoldFnDrain class outcome from drain summary.
pub fn on_hold(state: &mut BackoffState, now: Instant) {
    state.consecutive_holds = state.consecutive_holds.saturating_add(1);
    state.next_eligible = now + compute_backoff_window(state.consecutive_holds);
}

/// **REC-2 backoff transition on non-Hold outcome**: reset counter +
/// immediate eligibility.  Caller invokes after observing Acked /
/// StructuralDrift / Bootstrap outcome from drain summary.
pub fn on_advance(state: &mut BackoffState, now: Instant) {
    state.consecutive_holds = 0;
    state.next_eligible = now;
}

/// **REC-2 backoff lookup**: returns `Some(until)` if `fn_id` is
/// currently within its backoff window (caller skips drain тіку);
/// `None` if fresh entry OR window has elapsed (caller runs drain).
pub fn check_eligibility(
    backoff_map: &HashMap<String, BackoffState>,
    fn_id: &str,
    now: Instant,
) -> Option<Instant> {
    backoff_map.get(fn_id).and_then(|state| {
        if now < state.next_eligible {
            Some(state.next_eligible)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_backoff_window_schedule_matches_doc() {
        // 0 → 30s, 1 → 1min, 2 → 2min, ..., cap at 30min.
        assert_eq!(compute_backoff_window(0), Duration::from_secs(30));
        assert_eq!(compute_backoff_window(1), Duration::from_secs(60));
        assert_eq!(compute_backoff_window(2), Duration::from_secs(120));
        assert_eq!(compute_backoff_window(3), Duration::from_secs(240));
        assert_eq!(compute_backoff_window(4), Duration::from_secs(480));
        assert_eq!(compute_backoff_window(5), Duration::from_secs(960));
        assert_eq!(compute_backoff_window(6), Duration::from_secs(1800)); // 30min cap
        assert_eq!(compute_backoff_window(10), Duration::from_secs(1800)); // cap holds
        assert_eq!(compute_backoff_window(100), Duration::from_secs(1800)); // cap holds
    }

    #[test]
    fn compute_backoff_window_does_not_panic_on_extreme_input() {
        // Saturating shift guard — must not panic at u32::MAX.
        let _ = compute_backoff_window(u32::MAX);
    }

    #[test]
    fn on_hold_increments_counter_and_pushes_next_eligible() {
        let t0 = Instant::now();
        let mut state = BackoffState::fresh(t0);
        on_hold(&mut state, t0);
        assert_eq!(state.consecutive_holds, 1);
        assert_eq!(state.next_eligible, t0 + Duration::from_secs(60));
        on_hold(&mut state, t0);
        assert_eq!(state.consecutive_holds, 2);
        assert_eq!(state.next_eligible, t0 + Duration::from_secs(120));
    }

    #[test]
    fn on_advance_resets_state() {
        let t0 = Instant::now();
        let mut state = BackoffState {
            consecutive_holds: 5,
            next_eligible: t0 + Duration::from_secs(960),
        };
        on_advance(&mut state, t0);
        assert_eq!(state.consecutive_holds, 0);
        assert_eq!(state.next_eligible, t0);
    }

    #[test]
    fn check_eligibility_returns_none_for_unknown_fn() {
        let map = HashMap::new();
        let now = Instant::now();
        assert!(check_eligibility(&map, "fn-001", now).is_none());
    }

    #[test]
    fn check_eligibility_returns_until_for_within_window() {
        let mut map = HashMap::new();
        let t0 = Instant::now();
        let future = t0 + Duration::from_secs(60);
        map.insert(
            "fn-001".to_string(),
            BackoffState {
                consecutive_holds: 1,
                next_eligible: future,
            },
        );
        let result = check_eligibility(&map, "fn-001", t0);
        assert_eq!(result, Some(future));
    }

    #[test]
    fn check_eligibility_returns_none_for_elapsed_window() {
        let mut map = HashMap::new();
        let t0 = Instant::now();
        let past = t0.checked_sub(Duration::from_secs(60)).unwrap_or(t0);
        map.insert(
            "fn-001".to_string(),
            BackoffState {
                consecutive_holds: 1,
                next_eligible: past,
            },
        );
        let result = check_eligibility(&map, "fn-001", t0);
        assert!(result.is_none());
    }
}
