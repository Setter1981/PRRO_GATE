# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

---

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

## Development commands

```bash
# Setup
python -m venv .venv && source .venv/bin/activate
pip install -r requirements-dev.txt && pip install -e .

# Run (REST ingress — primary)
PRRO_GATEWAY_CONFIG=./ops/config.example.yaml python scripts/run_rest.py
# XML-RPC ingress
python scripts/run_xmlrpc.py
# Maria TCP ingress
python scripts/run_maria.py

# Tests — all
pytest tests/

# Single test file
pytest tests/test_write_path.py

# By pattern
pytest -k "concurrency"

# Migrations (manual)
python -m prro_gateway.migrations --db var/prro.db --sql-dir sql/

# Docker
docker compose up --build
```

Pytest is configured in `pyproject.toml` (`pythonpath=src`, `testpaths=tests`). No Makefile.
Tests use in-memory SQLite with auto-migrated schema via `conftest.py`.

---

## Architecture overview

**Package root:** `src/prro_gateway/`

**Request flow:**
```
Ingress script (scripts/run_*.py)
  → RuntimeContainer (runtime/container.py)  ← DI root, wires all layers
  → REST/XML-RPC/Maria shell (runtime/*_shell.py, rest_app.py)
  → Adapter (adapters/*.py)                  ← normalizes to CanonicalFiscalCommand
  → IngressService (services/ingress.py)     ← stores to inbox, triggers worker
  → WritePath worker (services/write_path.py) ← 6-stage pipeline, single-writer per fiscal_number
      stages: acquire+validate → guard → sign → send_or_offline → finalize
  → Transport (transports/router.py)         ← routes by backend/transport profile
  → Reconciliation (services/reconciliation.py) ← recovery from transport failures
```

**Persistence:** SQLite WAL (`repositories/`) — sole source of truth. Lease model ensures single-writer per `fiscal_number`. Migration runner uses checksum verification (`migrations/runner.py`).

**State machines to know:**
- Document: `PREPARED → SIGNED → ENCRYPTED → SENT → KVT1 → KVT2 → ACK / REJECTED / ERROR_*` (M3a Pattern B happy path). M3b Pattern C adds `OFFLINE_LOCAL_ACK` as durable offline state preceding drain.
- Shift (M3b 9-state expansion per `docs/superpowers/specs/2026-05-17-m3b-shift-state-expansion.md`): `Created → Opening → OpenedLocalPendingDrain → Opened → ClosingLocalPendingDrain → Closing → Closed / RequiresManualReconciliation / Error`. Manual reconciliation is "ЧП из ЧП" (extremely rare per 4-year operator empirics); EscalateManual reserved for truly unrecoverable cases — most failures route to AutoOfflineFallback / TechSupportEscalation / KeyRotationPending / MacReseedRecovery / TechSupportRepair recovery classes.
- Offline session: `OPENING → OPEN → DRAINING → CLOSED / ABORTED` (W4 normalization landed in migration `015_offline_normalize.sql`; `DRAINING` replaces earlier `CLOSING` naming)
- Node: `ONLINE / GOING_OFFLINE / OFFLINE / GOING_ONLINE / BLOCKED / STOP_MODE / CRYPTO_DEGRADED`

**Persistence model** (M3b architectural pin): `fiscal_documents` holds issued receipts **plus their non-issued terminal artifacts** — the real pin is **"no doc rests in a non-terminal state (`PREPARED`/`SIGNED`/`ENCRYPTED`) at a quiescent boundary"** (this is what bug #192 and the boot-resume twin P1 violated), NOT a literal "issued-only" table. Two refusal classes differ: **pre-acquire / invalid-ingress** refusals are rejected before any row is minted → `audit_log` only, never `fiscal_documents`; **DPS terminal rejects** act on an already-minted doc, and A.3 splits them by the SEND boundary — **advance-at-SEND**: the online chain seed advances atomically with the `server_fiscal_no` stamp at the `Sending→Sent` CAS (that CAS IS the online-issuance moment, not ACK). A **pre-SENT reject** (before the seed advance / sfn stamp) → `Sending → Rejected` CAS, so a **non-issued `Rejected` row legitimately rests** (lnd consumed, seed NOT advanced — **D2 pin survives verbatim**). A **post-SENT reject** is issued-but-unconfirmed (lnd consumed, sfn stamped, seed advanced) → it escalates to `RequiresManualReconciliation`, **never** `Rejected` (seed NOT rolled back — **D2 pin expanded**; the `(Sent, Rejected)` transition edge was removed in A.3 PR-B). Transport-class failures persist as `Sending` / `ErrorRetryable` for crash-recovery. Manual recon **confirmed trigger families** (per spec §16.7): (1) **any W9b drain reject of an `OFFLINE_LOCAL_ACK` backlog doc on `OpenedLocalPendingDrain` / `ClosingLocalPendingDrain`** — this is the primary surface per §6.3 universal EscalateManual + edges 6/14 (drain has crossed the local-commit threshold so rollback semantics don't apply; FN deregistered-while-offline is the observed real-world subtype); (2) ambiguous wire timeout for online SHIFT_OPEN / Z_REPORT (edges 4 + 12, cannot determine if DPS accepted); (3) operator-driven force seam.

**Crypto:** pluggable — `passthrough` (dev) or `sidecar` HTTP proxy (prod), configured in `config.yaml → crypto.provider`.

**Health endpoints:** `/health/live`, `/health/ready` (post-recovery), `/health/startup`; metrics at `/metrics`.

**Key docs:**
- `docs/Multi-Protocol_PRRO_Gateway.md` — primary technical specification (M2/M3a baseline; M3b state expansion supersedes §9/§11 shift lifecycle wording).
- `docs/superpowers/specs/2026-05-17-m3b-shift-state-expansion.md` — M3b shift state machine 9-state authoritative spec (§16 contains Round 8-9 operational reality alignment overriding earlier §§3-15 where they conflict).
- `docs/LEGAL_INVARIANTS.md` — legal invariants (INV-01 through INV-20) — INV-03/04 align with M3b 9-state shift; INV-08-INV-14 use `OFFLINE_LOCAL_ACK` (M3b naming) for Pattern C durable offline state; INV-19 recovery taxonomy expanded per M3b §16.3.

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

- `services/write_path.py`
- `services/reconciliation.py`
- `repositories/*`
- `transports/*`
- `adapters/*`
- shift / offline / node state handling
- migrations / schema / DDL
- runtime startup / shutdown / health
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
