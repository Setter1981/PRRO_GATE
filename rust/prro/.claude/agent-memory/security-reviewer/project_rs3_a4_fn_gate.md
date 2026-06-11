---
name: rs3-a4-fn-gate
description: RS-3 A4 per-FN runtime serialization gate — concurrency review verdict + forward contracts the A2/B1 wiring review must enforce
metadata:
  type: project
---

RS-3 A4 = per-`fiscal_number` runtime serialization gate `FnWriteGate` (`rust/prro/src/runtime/fn_gate.rs`), reviewed clean (MERGE) on branch `feat/rs3-a4-per-fn-gate` @ `094aa4c`.

Shape: `Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>`; `acquire` does a short non-async get-or-insert + `Arc::clone` inside a `let gate = {…};` block (std guard dropped before the await), then `gate.lock_owned().await`. Returns `OwnedMutexGuard<()>`. Lives in shared `Arc<Inner>` (`app.rs:208`), exposed via `App::acquire_fn_gate`. Distinct from App-wide `reconcile_mutex` (`app.rs:179`).

Concurrency verdict: std-guard-drop-before-await airtight; `lock_owned` FIFO-fair, wakes waiters on drop; RAII release covers both cancellation cases (awaiting vs holding); map bounded by FN count (~tens), never evicted = OK; `.expect("...poisoned")` unreachable (no panic site under the std lock).

**Why:** primitive-only piece — gate has ZERO live callers yet (grep: only tests reference it). Production binding still `UnimplementedWritePath`.

**How to apply (the load-bearing part for the A2/B1 wiring review):**
- Forward contract 1 — lock-ordering: A2/B1 MUST nest gate-OUTER / DB-tx-INNER. Call `acquire_fn_gate` first, THEN `with_immediate`/`acquire_lease` (SQLite BEGIN IMMEDIATE) inside the held guard. NEVER acquire the gate while holding a DB write-tx (inversion = deadlock hazard). The `with_immediate_no_foreign_io` scanner won't catch the gate (it's not a DB lock).
- Forward contract 2 — map-growth: the FN key fed to `acquire` MUST be the validated/canonical FN from `fiscal_number_config`, NOT a raw attacker-supplied ingress field. The map is never evicted, so a request-controlled key = unbounded-memory vector. Assert this at the A2 callsite.
- Forward contract 3 — the gate provides NO crash-state cleanup. A `fiscalize` cancelled mid-flight at shutdown releases the gate via RAII but leaves the DB row in PROCESSING; the B1 stale-PROCESSING reaper is what reclaims it. Correct division — don't expect the gate to roll back DB state.
- Deployment boundary (D1a): in-process gate assumes singleton pid-lock per DB (one `prro serve` per DB). Two instances over the same FN-set is UNSUPPORTED until a DB-level per-FN lease exists.

**Drain-vs-fiscalize lnd/MAC hazard — RESOLVED (2026-06-09 final verdict, MERGE):** the feared corruption does NOT exist. `allocate_next_lnd` (`node_state.rs:297`, atomic `UPDATE next_lnd=next_lnd+1 RETURNING next_lnd-1`) has ONE prod caller = `stage_acquire.rs:600` (inline fiscalize). Drain CONFIRMS only — routes backlog docs via `stage_send::run`/`stage_finalize::run` (`backlog_drain.rs:1155-1162,1877`), READS `existing.lnd`, never allocates. No persistent per-FN MAC counter on node_state (MAC derived per-doc in `stage_send mac_recovery`). And `allocate_next_lnd` runs inside `with_immediate` = `BEGIN IMMEDIATE` (`tx.rs:124`) → DB serializes the lone allocator regardless of which app lock (reconcile_mutex vs per-FN gate) is held. So disjoint app locks are corruption-safe.

**Forward contract 4 (A2/Integration, the residual):** drain (under `reconcile_mutex`) and inline-fiscalize (under per-FN gate) are DIFFERENT locks → can run concurrently per FN. Corruption-safe but causes lnd-ordering interleave + BEGIN-IMMEDIATE SQLITE_BUSY contention. A2 MUST make drain and inline-fiscalize per-FN mutually exclusive — run drain under the SAME `acquire_fn_gate(fn)` (or assert non-overlap) — to close ordering/liveness (NOT corruption, already DB-prevented).

Related: [[manual-recon-catastrophe]] not applicable; row-level backstop is `acquire_lease` NEW→PROCESSING CAS + C2 active-shift uniqueness.
