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
//!
//! FORWARD CONTRACTS the A2/B1 wiring review MUST enforce (this piece is the
//! primitive only — none of these are A4 defects):
//!   1. LOCK ORDERING — acquire the gate OUTSIDE every `with_immediate`; NEVER
//!      call `acquire` while holding a `BEGIN IMMEDIATE` write-tx (gate-outer /
//!      tx-inner), else the tokio gate ⇄ SQLite write-lock could interleave.
//!   2. KEY = the VALIDATED config `fiscal_number` (the ingress handler has
//!      already matched it against the listener FN-config), NOT a raw wire
//!      value — the map is never evicted, so an attacker-chosen FN key would be
//!      a request-controlled memory-growth vector.  The FN is carried verbatim
//!      end-to-end (the listener-FN match is exact-string, no normalization),
//!      so two spellings cannot split one FN across two gates.
//!   3. CRASH CLEANUP is NOT this gate's job — a holder that crashes leaves no
//!      durable state here (only the DB `PROCESSING` lease); B1's reaper owns
//!      the re-drive.
//!   4. DRAIN ⇄ FISCALIZE — run the offline drain UNDER the same per-FN gate so
//!      a drain pass and an inline `fiscalize` for one FN are mutually
//!      exclusive.  This is a LIVENESS/ordering refinement, NOT a correctness
//!      fix: the only lnd/MAC allocator (`stage_acquire::allocate_next_lnd`) is
//!      already serialized by `BEGIN IMMEDIATE` regardless of app-level lock,
//!      and the drain CONFIRMS-only (never allocates lnd) — coordinating them
//!      merely closes `SQLITE_BUSY` contention + lnd-ordering jitter.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

/// Per-`fiscal_number` async serialization gate (see module docs).  Internal
/// runtime infra — callers go through [`App::acquire_fn_gate`](crate::App).
#[derive(Default)]
pub(crate) struct FnWriteGate {
    /// The OUTER map lock is a [`std::sync::Mutex`] held ONLY for the
    /// non-async get-or-insert + `Arc` clone (never across an `.await`); the
    /// INNER per-FN [`tokio::sync::Mutex`] is what the caller awaits and holds
    /// across the whole fiscalize critical section.  Entries are never evicted
    /// — bounded by the deployment's FN count (tens), so no cleanup is needed.
    gates: Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

impl FnWriteGate {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Acquire the gate for `fiscal_number`, awaiting until any in-flight
    /// holder for the SAME FN releases.  Different FNs never contend.  The
    /// returned [`OwnedMutexGuard`] releases the gate on drop (RAII) —
    /// including when the holding `fiscalize` future is cancelled at shutdown
    /// (invariant #9: no explicit release is needed).
    pub(crate) async fn acquire(&self, fiscal_number: &str) -> OwnedMutexGuard<()> {
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
        // (A generous negative window — the future is genuinely blocked, so the
        // timeout always elapses; the window only needs to outlast scheduler
        // jitter on a loaded runner, never a real acquire.)
        let mut second = Box::pin(gate.acquire(FN_A));
        assert!(
            timeout(Duration::from_millis(200), &mut second)
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

    /// FIFO mutual exclusion under contention: with N waiters on ONE FN, at
    /// most one holder is ever inside the critical section, and all eventually
    /// proceed.  This is the deterministic #2 pin (a counter asserted `<= 1`,
    /// not a wall-clock window) — it would catch a regression that let two
    /// holders in (e.g. swapping the inner `Mutex` for a non-exclusive
    /// primitive).
    #[tokio::test]
    async fn many_waiters_one_at_a_time() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let gate = Arc::new(FnWriteGate::new());
        let inside = Arc::new(AtomicUsize::new(0));
        let completed = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..5 {
            let gate = Arc::clone(&gate);
            let inside = Arc::clone(&inside);
            let completed = Arc::clone(&completed);
            handles.push(tokio::spawn(async move {
                let _g = gate.acquire(FN_A).await;
                // Exactly one holder may be inside FN_A's critical section.
                let concurrent = inside.fetch_add(1, Ordering::SeqCst);
                assert_eq!(
                    concurrent, 0,
                    "two holders inside the FN_A critical section"
                );
                // Yield so a wrongly-admitted second holder would be polled here.
                tokio::task::yield_now().await;
                inside.fetch_sub(1, Ordering::SeqCst);
                completed.fetch_add(1, Ordering::SeqCst);
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(
            completed.load(Ordering::SeqCst),
            5,
            "every waiter must eventually proceed (no starvation/deadlock)"
        );
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
