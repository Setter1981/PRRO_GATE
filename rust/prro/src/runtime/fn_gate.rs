//! RS-3 A4 — the per-`fiscal_number` runtime serialization gate.
//!
//! [`FnWriteGate`] is an in-process async primitive that serializes the live
//! write-path ([`WritePathEntry::fiscalize`](crate::runtime::ingress::seam),
//! A2) and the stale-`PROCESSING` reaper (B1) PER `fiscal_number`.  A caller
//! [`acquire`](FnWriteGate::acquire)s the gate for one FN and holds the
//! returned guard across its ENTIRE `fiscalize` future — including the DPS
//! send + KVT2-confirm waits — so no second receipt of the SAME FN drives the
//! write-path concurrently (invariant #2, enforced at the runtime level
//! rather than relying solely on the DB `acquire_lease` CAS).
//!
//! TWO-LEVEL DISTINCTION (the whole point — D1):
//!   - the GATE is a [`tokio::sync::Mutex`] that spans the full `fiscalize`
//!     future; it MAY be held across `.await` on network/crypto.
//!   - each `with_immediate` `BEGIN IMMEDIATE` write-tx NESTED inside that
//!     future stays short-lived and holds NO network/crypto (invariant #1).
//!
//! The gate is acquired OUTSIDE every `with_immediate` envelope, so the static
//! `with_immediate_no_foreign_io` scanner never sees it — the gate is NOT a DB
//! lock.
//!
//! This is DISTINCT from `App`'s `reconcile_mutex` (a single App-wide gate
//! that serializes boot-reconcile + the offline drain at a coarser
//! granularity); the two domains are intentionally separate and MUST NOT be
//! unified.  Whether the global drain and an inline `fiscalize` for the SAME
//! FN need additional coordination is an A2/Integration concern — the DB-level
//! `acquire_lease` CAS (`NEW→PROCESSING`) + active-shift uniqueness (C2) are
//! the row/shift-level backstops.
//!
//! DEPLOYMENT BOUNDARY (D1a): the in-process gate is sufficient because `App`
//! holds a singleton pid-lock per DB (one `prro serve` per DB).  Running two
//! instances over the SAME FN-set is UNSUPPORTED until a DB-level per-FN lease
//! exists.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

/// Per-`fiscal_number` async serialization gate (see module docs).
#[derive(Default)]
pub struct FnWriteGate {
    /// The OUTER map lock is a [`std::sync::Mutex`] held ONLY for the
    /// non-async get-or-insert + `Arc` clone (never across an `.await`); the
    /// INNER per-FN [`tokio::sync::Mutex`] is what the caller awaits and holds
    /// across the whole fiscalize critical section.  Entries are never evicted
    /// — bounded by the deployment's FN count (tens), so no cleanup is needed.
    gates: Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

impl FnWriteGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// Acquire the gate for `fiscal_number`, awaiting until any in-flight
    /// holder for the SAME FN releases.  Different FNs never contend.  The
    /// returned [`OwnedMutexGuard`] releases the gate on drop (RAII) —
    /// including when the holding `fiscalize` future is cancelled at shutdown
    /// (invariant #9: no explicit release is needed).
    pub async fn acquire(&self, fiscal_number: &str) -> OwnedMutexGuard<()> {
        // SHORT, non-async critical section: get-or-insert this FN's gate and
        // clone the `Arc`.  The `std::sync::Mutex` guard is dropped at the end
        // of this block, BEFORE the `.await` below — we never hold it across a
        // suspension point (so it cannot block the runtime under contention).
        let gate = {
            let mut gates = self.gates.lock().expect("FnWriteGate map poisoned");
            Arc::clone(
                gates
                    .entry(fiscal_number.to_string())
                    .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
            )
        };
        gate.lock_owned().await
    }

    /// Distinct FNs that have an entry (one per FN regardless of acquire
    /// count).  Observability / test only.
    #[cfg(test)]
    fn tracked_fns(&self) -> usize {
        self.gates.lock().expect("FnWriteGate map poisoned").len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    const FN_A: &str = "4000000001";
    const FN_B: &str = "4000000002";

    /// The core serialization property: a second `acquire` for the SAME FN
    /// blocks while the first guard is held, then proceeds once it releases.
    #[tokio::test]
    async fn same_fn_second_acquire_blocks_until_release() {
        let gate = FnWriteGate::new();
        let g1 = gate.acquire(FN_A).await;

        // While g1 is held, the second same-FN acquire must NOT complete.
        let mut second = Box::pin(gate.acquire(FN_A));
        assert!(
            timeout(Duration::from_millis(50), &mut second)
                .await
                .is_err(),
            "the second same-FN acquire must block while the first guard is held"
        );

        // After the first releases, the waiter proceeds promptly.
        drop(g1);
        let _g2 = timeout(Duration::from_millis(500), &mut second)
            .await
            .expect("the second acquire must proceed once the first releases");
    }

    /// Different FNs never contend — FN_B acquires immediately while FN_A is
    /// held (no cross-FN block).
    #[tokio::test]
    async fn different_fns_do_not_contend() {
        let gate = FnWriteGate::new();
        let _ga = gate.acquire(FN_A).await;
        let _gb = timeout(Duration::from_millis(200), gate.acquire(FN_B))
            .await
            .expect("a different FN must not block on FN_A's gate");
    }

    /// RAII release: dropping the guard frees the gate (the same path that
    /// fires when a `fiscalize` future is cancelled at shutdown).
    #[tokio::test]
    async fn dropping_the_guard_releases_the_gate() {
        let gate = FnWriteGate::new();
        {
            let _g = gate.acquire(FN_A).await;
        } // guard dropped here
        let _g2 = timeout(Duration::from_millis(200), gate.acquire(FN_A))
            .await
            .expect("the gate must release when its guard drops");
    }

    /// One map entry per distinct FN, regardless of how many times it is
    /// acquired — the map is bounded by the FN count, not the request count.
    #[tokio::test]
    async fn reacquiring_same_fn_reuses_one_entry() {
        let gate = FnWriteGate::new();
        for _ in 0..5 {
            drop(gate.acquire(FN_A).await);
        }
        drop(gate.acquire(FN_B).await);
        assert_eq!(
            gate.tracked_fns(),
            2,
            "one map entry per distinct FN, regardless of acquire count"
        );
    }
}
