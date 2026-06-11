# External review request — RS-1 Piece 5 (runtime supervisor)

You have **local read access** to this repo (`/mnt/d/PRRO_GATE`). Review the code
directly; do not rely on pasted snippets.

## What this is

RS-1 is the **runtime supervisor / composition root** — the missing "runtime
spine" that wires the (already-built, already-tested) maintenance primitives into
a live `prro serve`. Before RS-1, `prro serve` booted and idled (0 spawned tasks).

Piece 5 (5a→5e) is the supervisor itself. It is **gated by
`config.supervisor.enabled` (default false)** — when off, the binary is
byte-identical M1-idle (rollback = config flip, not code revert).

## Branch / diff

- Branch: `feat/rs1-runtime-supervisor`
- Fork base: `a940520` (on `rust-gateway`)
- See the whole seam:  `git -C /mnt/d/PRRO_GATE diff a940520..HEAD -- rust/prro/`
- Piece-5 core file: `rust/prro/src/runtime/supervisor.rs`
- Config: `rust/prro/src/config/mod.rs` (`SupervisorCfg`, `DpsCfg`, `OfflineCfg`)
- Serve gate: `rust/prro/src/main.rs` (`Cmd::Serve`)
- Test: `rust/prro/tests/rs1_supervisor_boot.rs`
- Key-loader / fn_sign: `rust/prro/src/runtime/key_loader.rs`
- The injected reconcile/drain/probe primitives it drives:
  `rust/prro/src/app.rs` (`reconcile_pending_with`, `drain_offline_backlog_scheduled`),
  `rust/prro/src/services/offline_sync/return_online_probe.rs` (`run_tick_for_fn`)

## Frozen invariants (must not regress)

1. No network/crypto inside a long SQLite write tx.
2. One `fiscal_number` = one logical single-writer.
3. Channel switch forbidden with an open shift.
4. Idempotency mandatory.
5. Offline respects time + code limits.
8. Recovery/reconciliation must not silently violate state transitions.
9. Graceful shutdown matters more than finishing fast.

## Please focus on these risk axes

1. **fn_sign freshness.** The DPS auth blob (`rro_fn_sign`, an attached CMS over
   the FN string) carries `signingTime` INSIDE the signed bytes and must be
   current at each wire call (empirically: WebCheck + PRRODPS both re-sign per
   call). Verify: is `fn_sign` rebuilt FRESH per tick / per reconcile pass, and is
   there NO path that caches it for the process lifetime? (Note the deliberate
   choice NOT to reuse `return_online_probe::spawn_probe_loop`, which freezes
   `fn_sign` at spawn.)

2. **Reconcile resolver lifetime.** `run_with_registry` builds a `fn_signs`
   `HashMap` local, then a resolver closure that returns
   `RuntimeView<'_>` borrowing it. Is the borrow sound (does the store outlive the
   reconcile call)? Any way the `RuntimeView` could dangle?

3. **Graceful shutdown (5e).** One `watch` channel + `biased` select; on shutdown
   we flip the watch then JOIN both loop handles before `drop(App)` — deliberately
   NO abort (a drain finishes its in-flight tick). Verify: (a) no deadlock/hang;
   (b) the join is actually bounded (we rely on the DPS `request_timeout` the
   channel enforces — is that sufficient, or is an explicit deadline warranted?);
   (c) `biased` ordering really makes shutdown win over a ready tick.

4. **Single-writer / mutex discipline.** Both tick loops run on cloned `App`s that
   share the same `Arc<Inner>` (so the same `reconcile_mutex`). `drain_offline_
   backlog_scheduled` acquires that mutex internally; `reconcile_pending_with`
   acquires it too. Confirm: concurrent drain-tick vs reconcile-once vs a second
   drain-tick all serialize (invariant #2), and no tick holds the mutex across a
   long-running section that would block boot reconcile.

5. **Gate correctness.** `enabled=false` ⇒ byte-identical M1-idle. `enabled=true`
   with a blank `dps.endpoint` ⇒ fail-closed boot error (no panic). Connect failure
   on the enabled path ⇒ hard boot failure (operator decision: an enabled
   supervisor must reach DPS; this is the online wire path, not offline fallback).
   Verify the branch in `main.rs` + `require_dps_endpoint()`.

6. **Probe CRITICAL-audit replication.** `probe_tick` replicates the W8 probe
   loop's "CRITICAL audit on infra-error" contract rather than reusing it. Is the
   replication faithful (event_type / severity / entity attribution), and is the
   duplication acceptable vs a refactor?

7. **Known limitation — task-death supervision.** A tick task that PANICS (a true
   bug; operational errors are caught + logged) is only observed at shutdown via
   the `JoinError` log — no respawn/escalate. Is deferring this acceptable for a
   pilot, or is it a blocker?

## What I believe is correct (challenge me)

- Per-tick fresh `fn_sign` in both loops (inline build in the tick scope).
- Reconcile-once runs BEFORE the loops, seeding `node_state` (`upsert_initial`
  Online/Closed) for every configured FN so the loops never see a missing row.
- Per-FN failures (fn_sign build / drain / tick) are logged + skipped, never fatal.
- `MissedTickBehavior::Skip` so a slow drain never queues missed ticks.

Return findings as Critical / High / Medium / Low / Info with file:line anchors.
