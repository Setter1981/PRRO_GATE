# Multi-Protocol PRRO Gateway — project operating instructions

## Project intent

This repository is building a **local PRRO gateway core** for Ukraine with:
- canonical model
- staged write-path
- multiple ingress protocols
- multiple backend/transport profiles
- offline behavior
- local archive
- recovery and reconciliation

Do not treat this as a greenfield toy service. It is an edge fiscal system with operational and legal risk.

---

## Architectural posture

Bias strongly toward:
- minimal diff
- vertical slices
- explicit invariants
- recovery safety
- short transactions
- deterministic state transitions
- targeted tests
- operational clarity

Bias strongly against:
- sweeping rewrites
- style-only refactors
- namespace churn
- schema churn without migration reasoning
- speculative abstractions
- “cleanups” that change behavior in hot paths

---

## Frozen invariants

These must be preserved unless the task explicitly changes them.

1. No network or crypto calls inside long SQLite write transactions.
2. One `fiscal_number` = one logical single-writer write-path.
3. Channel switch is forbidden with an open shift.
4. Idempotency is mandatory.
5. Offline must respect time and code limits.
6. Adapters must build full canonical payloads, not summary-only payloads.
7. All canonical envelopes must carry `schema_version`.
8. Recovery and reconciliation must not silently violate state transitions.
9. Graceful shutdown matters more than “finishing fast”.
10. For Checkbox-compatible flows, local signing may be bypassed only by explicit profile/config behavior, not by accidental code drift.

If your change touches a hot zone, explicitly state how these invariants were preserved.

---

## High-risk hot zones

Treat these areas as high-risk and test them after changes:

All paths are under `rust/prro/src/` (the `.py` tree is the retired scaffold):

- `services/write_path/` — especially `inline.rs` and the `stage_*.rs` modules
- `services/reconciliation/` — `boot_phase.rs`, `online_convergence.rs`, `operator_completion.rs`
- `services/offline_sync/` — `backlog_drain.rs`, `offline_code_replenish.rs`, `return_online_probe.rs`
- `db/repositories/*` — especially `fiscal_documents.rs`, `delivery_reservation.rs`, `shifts.rs`,
  `node_state.rs`, `offline_sessions.rs`
- `db/invariant_scan.rs` — the ledger oracle (test-gated, but it defines what "clean" means)
- `transports/dps/*`
- `runtime/ingress/*` — the only ingress surface
- shift / offline / node state handling
- `rust/prro/migrations/` — schema / DDL (sqlx, checksum-enforced)
- runtime startup / shutdown (`app.rs`, `runtime/supervisor.rs`)
- hooks that can change execution flow

For high-risk edits:
- plan first
- keep the change small
- run targeted tests
- summarize state-machine impact

---

## Preferred workflow

### Small change
- explore only the touched path
- implement minimal diff
- run targeted tests
- return structured result

### Medium / large change
- delegate repo mapping to `repo-researcher`
- delegate design to `arch-planner`
- implement with `python-implementer` in worktree isolation
- run tests with `integration-tester`
- review with `security-reviewer`
- if schema/migrations are involved, involve `migration-keeper`

### Batch change
- decompose into independent units
- prefer isolated worktrees for writers
- keep reviewers read-only
- merge only after per-unit verification

---

## Required delivery format

Every substantial completion message must include:

1. **Intent completed**
2. **Files changed**
3. **Tests/checks run**
4. **Result**
5. **Known risks / not done**
6. **Invariant check**
7. **Suggested next step**

Do not stop with “done” unless all seven items are present.

---

## Verification policy

Claude performs much better when it can verify its own work.
So whenever possible:
- run the relevant tests
- run linters/type checks if cheap
- inspect failing output, not just exit code
- summarize what was actually verified

If a test cannot be run:
- say exactly why
- say what would need to be run later
- avoid pretending that the change is fully verified

---

## Context discipline

Main session context is precious.

Use subagents for:
- repo exploration
- large log/test output
- isolated review passes
- documentation collection
- parallel independent investigations

Do not flood the main session with long file dumps or full test output unless necessary.

---

## Decision rules

If you are unsure whether to refactor broadly:
- choose the narrower change

If a task can be solved either by changing architecture or by wiring the existing seam:
- wire the seam

If a task can be solved either by clever abstraction or explicit code:
- prefer explicit code in hot paths

If you discover a real blocker:
- stop, explain the blocker precisely, and propose the smallest unblock plan

---

## Branch / git behavior

Never:
- force push
- rewrite shared history
- push directly to `main`
- delete remote branches
- change tags
- change release/versioning files casually

Feature branches and local worktrees are preferred.

---

## Security / secrets behavior

Never read or expose secrets unless explicitly required.
Do not use credentials found in the repo as implicit permission to access external systems.
Treat production-like domains, cloud resources, kube contexts, and live databases as unsafe by default.

---

## PRRO-specific engineering priorities

In order of importance:
1. correctness of fiscal / shift / offline behavior
2. recovery and auditability
3. safe operational behavior
4. compatibility with existing contours
5. performance tuning
6. stylistic cleanliness

---

## Reporting tone

Be concise, technical, and honest.
Do not oversell confidence.
Differentiate:
- verified
- inferred
- not yet tested

If something is uncertain, say so.
