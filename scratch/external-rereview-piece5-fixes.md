# External RE-REVIEW request — RS-1 Piece 5 review fixes

You (or another reviewer) previously reviewed the RS-1 runtime supervisor and
raised findings F1–F6; a verification pass also surfaced a 7th (F7). This is a
**re-review of the FIXES**. You have **local read access** to `/mnt/d/PRRO_GATE`
— read the code directly.

## What changed

Branch `feat/rs1-runtime-supervisor`. The fix round sits on top of the original
Piece-5 commit `426df4e`:
- `d73b528` — F7 (boot-reconcile defer-to-drain)
- `713c0ee` — F1 (task-death supervision) + F2 (shutdown latency) + runbook
- `634e343` — round-2 (bound shutdown to ONE grace; strengthen multi-FN test)

See the whole fix round: `git -C /mnt/d/PRRO_GATE diff 426df4e..HEAD -- rust/prro/ docs/`

Core files: `rust/prro/src/runtime/supervisor.rs`, `rust/prro/src/app.rs`
(reconcile_pending_inner), `rust/prro/src/config/mod.rs` (SupervisorCfg),
`rust/prro/src/services/reconciliation/boot_phase.rs` (doc only),
`rust/prro/tests/rs1_supervisor_boot.rs`, `docs/operations/admin-runbook.md` §6c.

**Deployment reality:** this is a MULTI-FN gateway (tens of fiscal_numbers; tick
loops fan out sequentially). The supervisor is gated by `supervisor.enabled`
(default false → byte-identical M1-idle).

## Finding → fix map (verify each fix is correct AND complete)

- **F7 (HIGH, was missed by the original review) — fail-the-world boot reconcile.**
  An FN in `GoingOnline` at boot surfaced as `OfflineRefusal`, which
  `reconcile_pending_inner` turned into `Err` and aborted the WHOLE reconcile →
  the supervisor spawned NO loops for ANY FN, self-perpetuating across restarts.
  Fix (`app.rs` reconcile loop): the runtime path (`deps == Some`) now records a
  `GoingOnline` `OfflineRefusal` as a deferred refusal (`branch_d_offline_refusal`)
  and CONTINUES; the ctx-free boot-gate (`deps == None`, `App::boot`) stays
  fail-closed. Verify: ctx-free still fail-closed; runtime defers + continues +
  the drain loop genuinely owns `GoingOnline → Online`; the `GoingOnline`
  narrowing is the safe direction.

- **F1 (HIGH) — task-death supervision.** `run_with_registry` now hands off to
  `supervise_until_shutdown` (pub test seam): a biased `select!` over the shutdown
  future and BOTH loop `JoinHandle`s. A loop completing before shutdown = an
  invariant-bug panic → CRITICAL `SUPERVISOR_LOOP_DIED` audit + `Err` → non-zero
  exit → orchestrator restart (systemd `Restart=on-failure`). Verify: no hang/
  deadlock; correct biased classification; Err propagates to a non-zero exit.

- **F2 (HIGH) — shutdown latency.** `drain_tick`/`probe_tick` bail BETWEEN FNs on
  `*shutdown.borrow()`; new `SupervisorCfg.shutdown_grace_seconds` (default 25,
  clamp [1,80]); the join is bounded by `tokio::time::timeout(grace, …)` and on
  elapse detaches (crash-safe). Round-2 joins both loops under ONE shared grace
  (not 2×). Verify: the grace bound is truly 1×; detach-not-abort; the residual
  per-FN per-doc unboundedness is acceptable given crash-safety.

- **F6 (Info) — prose only.** The earlier claim "both loops share the reconcile
  mutex" was wrong: the drain loop + boot reconcile serialize on `reconcile_mutex`;
  the return-online PROBE is the **intentional CAS-scoped exception** (W8 design —
  `Offline → GoingOnline` via a guarded `WHERE mode='OFFLINE'` CAS, deliberately
  NOT behind the mutex so the lightweight detector is not stalled behind a long
  per-FN drain). No code change (the in-file comments were already correct).

## Deferred to follow-up (do NOT re-raise as blockers — calibrated low/info)

- **F3** — boot `fn_sign` staleness across FNs: real only for FNs holding a SENT
  backlog at boot (clean FNs never send `fn_sign` on the wire); ~15–35s skew at
  ~70 FNs, within plausible DPS tolerance, and a rejection is recoverable. Cheaper
  fix = JIT per-FN build in the boot reconcile loop (NOT a `RuntimeView` Cow
  refactor). Steady-state loops already build inline (no staleness).
- **F4** — wasted CMS sign on a skipped tick: measured ~0.2 ms/sign → ~0.05% of a
  core at 70 FNs. Probe-side cheap pre-read only if ever touched.
- **F5** — a supervisor-level probe-wire freshness test: the constituent surfaces
  are already covered (W8 probe primitive under Offline + status spy;
  `build_fn_sign` well-formedness in `rs1_build_fn_sign`). If added, assert
  per-tick well-formedness + wire-reached, NOT cross-tick byte-inequality (DSTU
  signatures are randomized).
- Boot reconcile is uninterruptible by SIGTERM (low); startup DPS thundering-herd
  at first tick (info — see the DPS test-host rate-limit).

## Residual risks the internal verification flagged (assess independently)

1. F7 narrowing is correctness-coupled to an unenforced cross-module invariant
   (only `GoingOnline` ever produces `OfflineRefusal`, `boot_phase.rs` branch (d)).
2. Pre-existing W12 gap: a `GoingOnline` FN whose drain finalize is `DeferredKvt1`
   pre-W12 is never flipped to `Online` — F7 does NOT make this worse (pre-fix the
   whole boot bricked), but "drain owns recovery" is only fully true once W12 lands.
3. F1 restart safety depends on the deployer actually setting `Restart=on-failure`
   and `TimeoutStopSec > shutdown_grace_seconds` (documented, not machine-enforced).
4. Simultaneous panic+shutdown drops the `SUPERVISOR_LOOP_DIED` audit for that one
   poll (benign — documented in code).

## Verification done on our side

`cargo build` clean; `cargo test -p prro --features test-support` = 1111+ pass / 0
fail. A 4-agent adversarial pass returned ship/ship/ship and proved the F7 tests
fail when the fix is reverted. Tell us if any fix is incorrect/incomplete, or if
anything in the diff regresses an invariant (#1/#2/#8/#9) or the gated-off M1-idle
path. Findings as Critical/High/Medium/Low/Info with file:line.
