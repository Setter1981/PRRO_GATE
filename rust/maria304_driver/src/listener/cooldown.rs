//! Post-disconnect cooldown timer.
//!
//! Real Maria hardware needs ~3 seconds to reset its serial state
//! after a client disconnects.  The Resonance OLE DLL's
//! `Done()` + `Init()` sequence assumes this delay — skipping it
//! causes the second `Init` to fail intermittently.  The virtual
//! driver mirrors the behaviour so OLE keeps its retry logic happy.
//!
//! Implementation: track "earliest time a new connection may take
//! the FN" via `std::time::Instant`.  The listener checks this
//! stamp before accepting a connection; if the cooldown has not
//! elapsed, the socket is closed without even reading any bytes.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Default cooldown per plan doc DRV-4.
pub const DEFAULT_COOLDOWN: Duration = Duration::from_secs(3);

#[derive(Debug)]
pub struct Cooldown {
    duration: Duration,
    next_allowed: Mutex<Option<Instant>>,
}

impl Cooldown {
    #[must_use]
    pub fn new(duration: Duration) -> Self {
        Self {
            duration,
            next_allowed: Mutex::new(None),
        }
    }

    #[must_use]
    pub fn default_3s() -> Self {
        Self::new(DEFAULT_COOLDOWN)
    }

    /// Record that a session just ended — from now, wait `duration`
    /// before another connect is allowed.
    ///
    /// # Panics
    /// If the internal mutex is poisoned by a prior panic.
    pub fn start_now(&self) {
        *self.next_allowed.lock().expect("Cooldown mutex poisoned") =
            Some(Instant::now() + self.duration);
    }

    /// Is it currently OK to accept a new connection?
    #[must_use]
    pub fn is_clear(&self) -> bool {
        self.is_clear_at(Instant::now())
    }

    /// Parameterised on `now` so tests can pin the clock.
    ///
    /// # Panics
    /// If the internal mutex is poisoned by a prior panic.
    #[must_use]
    pub fn is_clear_at(&self, now: Instant) -> bool {
        let guard = self.next_allowed.lock().expect("Cooldown mutex poisoned");
        match *guard {
            Some(t) => now >= t,
            None => true,
        }
    }

    /// Time until cooldown elapses — `Duration::ZERO` if clear.
    #[must_use]
    pub fn remaining(&self) -> Duration {
        self.remaining_at(Instant::now())
    }

    /// # Panics
    /// If the internal mutex is poisoned by a prior panic.
    #[must_use]
    pub fn remaining_at(&self, now: Instant) -> Duration {
        let guard = self.next_allowed.lock().expect("Cooldown mutex poisoned");
        match *guard {
            Some(t) if t > now => t - now,
            _ => Duration::ZERO,
        }
    }
}

impl Default for Cooldown {
    fn default() -> Self {
        Self::default_3s()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_cooldown_is_clear() {
        assert!(Cooldown::default_3s().is_clear());
    }

    #[test]
    fn cooldown_blocks_immediately_after_start() {
        let c = Cooldown::new(Duration::from_millis(100));
        c.start_now();
        assert!(!c.is_clear());
        assert!(c.remaining() > Duration::ZERO);
    }

    #[test]
    fn cooldown_clears_after_duration_elapses() {
        let c = Cooldown::new(Duration::from_millis(50));
        c.start_now();
        std::thread::sleep(Duration::from_millis(80));
        assert!(c.is_clear());
        assert_eq!(c.remaining(), Duration::ZERO);
    }

    #[test]
    fn is_clear_at_custom_instant_respects_pinning() {
        let c = Cooldown::new(Duration::from_secs(10));
        c.start_now();
        let now = Instant::now();
        assert!(!c.is_clear_at(now));
        assert!(c.is_clear_at(now + Duration::from_secs(11)));
    }

    #[test]
    fn restart_extends_cooldown() {
        let c = Cooldown::new(Duration::from_millis(30));
        c.start_now();
        std::thread::sleep(Duration::from_millis(20));
        c.start_now(); // resets
        std::thread::sleep(Duration::from_millis(20));
        // Total elapsed ≈ 40ms but the restart rewound the clock, so
        // another ~10ms remain.
        assert!(!c.is_clear(), "restart must extend the lockout");
    }

    #[test]
    fn default_matches_drv_4_three_second_rule() {
        let c = Cooldown::default();
        c.start_now();
        // Must be blocked at t+0 and t+2.9s, clear at t+3.1s.
        let now = Instant::now();
        assert!(!c.is_clear_at(now));
        assert!(!c.is_clear_at(now + Duration::from_millis(2900)));
        assert!(c.is_clear_at(now + Duration::from_millis(3100)));
    }
}
