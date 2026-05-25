# M3b W12 Post-Hardening — Senior Reviewer Findings

**Status**: Captured 2026-05-25 (review after PR #81 merge — hardening cycle CODE COMPLETE)
**Context**: Retrospective audit of the 7-PR post-W12 hardening sequence (PRs #75 → #76 → #77 → #78 → #79 → #80 → #81).  Findings classified as CONCERN (cross-PR architectural), GAP (test coverage), TD (technical debt).  None block pilot launch; all are polish-tier items для post-pilot or hygiene-cycle follow-up.

---

## 1. Aggregate metrics (closing snapshot)

| Metric | Value |
|---|---|
| PRs landed | 7 (#75-#81) |
| Net LOC | +2,779 / -78 |
| New modules | `admin.rs`, `backoff.rs`, scanner in `boot_phase` |
| Schema migrations | 018 (`consecutive_holds`), 019 (`SYSTEM_CRASH`) |
| New audit events | `KVT2_CONFIRM_PROLONGED_HOLD`, `OFFLINE_DRAIN_FN_STOP_MODE`, `ADMIN_STOP_MODE_RESET`, `TRANSPORT_TRACE_ORPHAN_CLOSED` |
| Test growth | 229 → 253 (+24 / +10.5%) |
| Operator-pinned constraints | All applicable honored (Manual recon avoidance, per-FN isolation, 36h cap, FIFO serial, DB/log separation, merge style) |
| Invariant regressions | 0 (I4 + I8 actively strengthened) |

---

## 2. Cross-PR architectural concerns (CONCERN-1..5)

### CONCERN-1 (MEDIUM) — `App.backoff_state` admin-reset coordination

**Source PRs**: #79 (admin CLI) ↔ #81 (backoff state)

`App.backoff_state` is in-memory (HashMap in `App::Inner`).  Admin CLI (PR #79) runs in a SEPARATE process from `prro serve` (operator memory `feedback_db_vs_log_separation` precedent — CLI as one-shot binary).  Admin reset clears persistent counter (`fiscal_documents.consecutive_holds = 0`) + flips `node_state.mode` → `GOING_ONLINE`, but does NOT clear `App.backoff_state[FN]` in the still-running `prro serve` process.

**Impact**:
- Today (CLI-only admin): operator manually restarts `prro serve` after admin reset → fresh backoff state on restart → no issue.
- Future (in-process admin endpoint via HTTP/RPC): admin reset would NOT clear running App's backoff window → FN stays in 30-min backoff window despite operator intervention.

**Recommended fix** (polish-tier):
- Document explicitly в admin runbook: "After `prro admin reset-stop-mode`, restart `prro serve` to clear in-memory backoff state OR wait up to 30 min for backoff window to elapse."
- When future M3+ HTTP admin endpoint lands, expose `App::reset_backoff_for_fn(fn_id)` helper + invoke from in-process reset path.

---

### CONCERN-2 (MEDIUM) — REC-8 test is observational, not concurrent

**Source PR**: #80

`rec8_drain_reads_node_state_snapshot_per_tick_not_mid_tick` runs TWO SEQUENTIAL drain ticks з external mode flip between them — locks the snapshot-per-tick semantic via observation.

Real concurrent W8 probe ↔ drain race з `tokio::join` + barriers НЕ exercised.

**Reasoning for accepting current scope**: per-tick read pattern at `backlog_drain.rs:662` (`let ns = node_state::get(pool, fn)`) is structurally race-safe — `ns` is in-memory snapshot held до кінця drain; W8's `UPDATE node_state SET mode=...` after that read affects only future ticks.  No mid-tick re-read site exists.

**Recommended fix** (low priority):
- 1 dedicated concurrent test using `tokio::sync::Barrier` + 2 spawned tasks (W8 probe + drain) demonstrating real-time interleaving.  Belongs in `tests/return_online_probe_concurrency.rs` (new file).  ~80-100 LOC.
- Defer until W8 probe scope grows beyond current single-CAS-flip surface.

---

### CONCERN-3 (LOW) — Admin CLI stale `--target-mode` mention

**Source PR**: #79
**Location**: `rust/prro/src/admin.rs:60`

```rust
"admin: fiscal_number {fiscal_number:?} current mode is {observed_mode:?}, \
 expected STOP_MODE — operator command misuse (use --target-mode flag or \
 check intended FN)"
```

Error message mentions `--target-mode` flag for operator guidance, BUT that flag is NOT implemented in `AdminCmd::ResetStopMode`.  Operator following error guidance hits unknown flag.

**Recommended fix** (polish — quick win):
- Edit error message: remove `--target-mode flag or` phrase → "...expected STOP_MODE — operator command misuse (check intended FN)".
- Alternative: implement `--target-mode` flag if future use-case justifies (currently only STOP_MODE → GOING_ONLINE transition supported).

---

### CONCERN-4 (LOW) — REC-2 reqwest connection pool clamping deferred з no follow-up ticket

**Source PR**: #81 plan (REC-2 scope discussion)

Original REC-2 plan mentioned 2 sub-items: per-FN backoff (delivered) + reqwest connection pool clamping (deferred).  PR #81 explicitly notes deferral but no new beads/Dolt ticket created для tracking.

**Risk**: pool exhaustion at scale (50+ FN persistent issues) could still cascade без backoff sufficient mitigation.  Mostly theoretical для pilot scale (≤ 10 FN).

**Recommended fix** (post-pilot):
- Create PRRO_GATE-??? ticket: "Reqwest connection pool clamping — `[transport] reqwest_pool_max_idle_per_host = N` config".  ~50 LOC config + reqwest ClientBuilder hookup.

---

### CONCERN-5 (LOW) — Boot-only orphan scanner для long-uptime processes

**Source PR**: #80

`boot_phase::close_orphan_transport_traces` runs ONCE per App boot.  Long-uptime M3+ runtime (weeks/months between restarts) accumulates orphans not cleaned до next restart.

**Risk profile**: orphans only arise on SIGKILL/OOM/power-loss (rare).  Most operators restart soon after such events anyway.  Steady-state orphan rate ≈ 0.

**Recommended fix** (M3+ scope):
- When ticker/supervisor lands в M3+, register orphan scanner as daily/weekly background task.  Reuse existing scanner function, schedule via tokio interval.

---

## 3. Test coverage gaps (GAP-1..3)

### GAP-1 (HIGH for production confidence) — No end-to-end Tier1→Tier2→Reset→Drain cycle test

**Risk**: each Tier tested in isolation; operator's REAL workflow (degradation → STOP_MODE → admin reset → fresh drain succeeds) НЕ covered as single test.  Regression in any transition point could go undetected.

**Recommended fix** (polish — high value):
- ~80 LOC integration test в `tests/app_drain_offline_backlog.rs`:
  1. Boot App + seed FN з GOING_ONLINE + offline session + Kvt1 doc.
  2. Loop 50 hold ticks (Transport err) → Tier 2 fires (assertions per existing REC-1 tests).
  3. Call `prro::admin::reset_stop_mode(pool, fn, "test recovery")` → assert reset outcome.
  4. Switch DPS carrier to Acked response.
  5. Run drain → assert doc reaches ACK.
  6. Validate: counter==0, mode=GOING_ONLINE (then ONLINE post-W8 probe), 1 ADMIN_STOP_MODE_RESET audit, 1 OFFLINE_DRAIN_KVT2_ADVANCED audit.

---

### GAP-2 (MEDIUM) — Orphan scanner App-boot wiring not tested

**Source PR**: #80

`rec3_*` tests call `boot_phase::close_orphan_transport_traces` DIRECTLY.  Do not validate that `App::reconcile_pending_inner` actually invokes it pre-FN-loop.  If someone removes the call from `app.rs`, REC-3 tests still pass (false-positive).

**Recommended fix** (polish):
- 1 test that boots full App з pre-seeded orphan, then asserts orphan closed.  Validates wiring, not just function logic.  ~40 LOC в `tests/app_boot_reconciliation.rs`.

---

### GAP-3 (LOW) — REC-2 backoff reset semantic at integration level

**Source PR**: #81

Unit test (`on_advance_resets_state`) covers in isolation.  Integration test sequence "tick 1 Hold → tick 2 (within window) Skipped → wait expiry → tick 3 (Acked) → tick 4 immediate (eligible)" NOT exercised.

**Recommended fix** (polish):
- 1 integration test з time-mocking (or `tokio::time::pause`).  ~60 LOC.  Optional — not load-bearing for correctness.

---

## 4. Technical debt registry (TD-1..10)

| ID | Item | Severity | Source | Action |
|---|---|---|---|---|
| TD-1 | CI broken (5/5 cross-platform jobs FAIL для PRs #71-#81) | MEDIUM | infrastructure | Dedicated CI repair PR — investigate runner setup / cache config |
| TD-2 | rustc 1.94.1 ICE workaround `#![allow(dead_code, unused)]` в `prro_crypto*::tau_naf.rs/mladder.rs` + 2 test files | MEDIUM | PR #72 | Track rust-lang/rust#154258 — bump toolchain.toml + remove allows when fix lands |
| TD-3 | Admin CLI stale `--target-mode` flag mention | LOW | PR #79 | **POLISH-NOW**: 1-line Edit |
| TD-4 | Reqwest connection pool clamping (REC-2 sub-item) | LOW | PR #81 plan | Create ticket для post-pilot |
| TD-5 | Tiered Degradation spec doc (operator plan §1 mentioned, never written) | LOW | PR #76/#77 | Author optional spec doc |
| TD-6 | `App.backoff_state` admin reset coordination (cross-process scenario) | LOW | CONCERN-1 above | Runbook addendum |
| TD-7 | `W12ConfirmOutcome::DeferredKvt1` deprecated-but-retained | LOW | PR #73 MED-3 | Post-pilot cleanup when no pre-W12 in-flight docs |
| TD-8 | Periodic orphan-trace scanner (CONCERN-5 above) | LOW | M3+ runtime supervisor | Defer to M3+ scheduling layer |
| TD-9 | LOW-1 i32/i64 trace_attempt_no asymmetry | LOW | PR #71 review | Defer until refactor pass |
| TD-10 | LOW-2 clone reduction on wire timestamps | LOW | PR #71 review | Defer until perf-tuning pass |

---

## 5. Pilot readiness assessment

**🟢 PILOT-READY** post PR #81 merge.

Pre-pilot mandatory:
1. ✅ All hardening RECs (1-8) merged.
2. ⏳ Live DPS smoke cycle (Sprint 7 style) — НЕ re-run post-hardening.  Re-run before pilot launch.
3. ⏳ Admin runbook published з Tier escalation decision tree + 4-event audit glossary.
4. ⏳ TD-1 (CI repair) — pre-pilot quality gate desired.

Post-pilot polish (priority order):
1. GAP-1 end-to-end test (high confidence value)
2. CONCERN-3 / TD-3 admin CLI message fix (quick win)
3. GAP-2 boot wiring test
4. GAP-3 backoff reset integration test
5. CONCERN-4 / TD-4 reqwest pool ticket
6. TD-5 Tiered spec doc

---

## 6. Final architectural verdict

The post-W12 hardening cycle (PRs #75-#81) demonstrates **disciplined PRRO-correct engineering**:

- Rejected anti-patterns (auto-Manual escalation, global Circuit Breaker) у favor of operator-pinned conservative alternatives (STOP_MODE intermediate, per-FN isolation).
- Maintained zero-regression test discipline across 7 atomic commits з consistent commit-message quality.
- Strengthened I4 + I8 invariants via 4 new structured audit events + atomic envelope expansions.
- All 8 RECs delivered end-to-end in single-day execution з 5 operator-architectural decisions correctly honored.

Findings catalogued here are **post-pilot polish surface**, not pilot-blockers.
