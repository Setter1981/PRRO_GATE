//! Single-connection-per-FN exclusion gate.
//!
//! Real Maria hardware exposes a single serial / TCP port and rejects
//! a second simultaneous connection.  The virtual driver mirrors
//! this — otherwise two parallel 1C instances could issue interleaved
//! commands against a single FN and produce duplicate check numbers.

use std::sync::atomic::{AtomicBool, Ordering};

/// Lock-free "someone holds the FN" flag.
///
/// `try_acquire` atomically transitions `false → true` and returns
/// whether the caller got the slot.  `release` flips back to `false`.
/// No unsafe, no mutex — `AtomicBool` CAS is all we need.
#[derive(Debug, Default)]
pub struct ConnectionGate {
    active: AtomicBool,
}

impl ConnectionGate {
    #[must_use]
    pub fn new() -> Self {
        Self {
            active: AtomicBool::new(false),
        }
    }

    /// Try to take the slot.  Returns `true` iff we successfully
    /// transitioned from vacant to active.
    #[must_use]
    pub fn try_acquire(&self) -> bool {
        // compare_exchange with Acquire on success so subsequent
        // reads see the gate taken; Relaxed on failure since we do
        // not need an ordering.
        self.active
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    /// Release the slot.  Must only be called by the task that
    /// successfully acquired it.
    pub fn release(&self) {
        self.active.store(false, Ordering::Release);
    }

    /// Observer — whether a session currently holds the FN.  Used by
    /// the admin HTTP API (M10).
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_gate_is_vacant() {
        let g = ConnectionGate::new();
        assert!(!g.is_active());
    }

    #[test]
    fn first_acquire_succeeds_second_fails() {
        let g = ConnectionGate::new();
        assert!(g.try_acquire());
        assert!(!g.try_acquire(), "second acquire must fail");
        assert!(g.is_active());
    }

    #[test]
    fn release_lets_next_acquire_succeed() {
        let g = ConnectionGate::new();
        assert!(g.try_acquire());
        g.release();
        assert!(!g.is_active());
        assert!(g.try_acquire(), "acquire after release must succeed");
    }

    #[test]
    fn concurrent_acquires_pick_exactly_one_winner() {
        use std::sync::Arc;
        use std::thread;

        let gate = Arc::new(ConnectionGate::new());
        let winners: Vec<_> = (0..32)
            .map(|_| {
                let gate = Arc::clone(&gate);
                thread::spawn(move || gate.try_acquire())
            })
            .collect();
        let results: Vec<bool> = winners.into_iter().map(|h| h.join().unwrap()).collect();
        let winners_count = results.iter().filter(|&&x| x).count();
        assert_eq!(
            winners_count, 1,
            "exactly one thread must win; got {winners_count} winners",
        );
    }
}
