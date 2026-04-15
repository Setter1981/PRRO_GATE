---
name: prro-invariants
description: Architectural invariants and high-risk zones for the Multi-Protocol PRRO Gateway. Claude should apply this automatically whenever work touches write_path, transports, offline, shifts, crypto, runtime, or persistence.
user-invocable: false
---

Apply these rules whenever the task touches hot paths.

## Core invariants
1. No network or crypto inside long SQLite write transactions.
2. One `fiscal_number` = one logical single-writer write-path.
3. Channel switch forbidden with an open shift.
4. Idempotency is mandatory.
5. Offline is bounded by explicit limits and code availability.
6. Adapters produce full canonical payloads.
7. Canonical envelopes carry `schema_version`.
8. Recovery and reconciliation must preserve state-machine correctness.
9. Graceful shutdown matters.
10. Minimal diff is preferred over broad refactor.

## Hot zones
- `services/write_path.py`
- `services/reconciliation.py`
- `transports/*`
- `adapters/*`
- `repositories/*`
- `alembic/*`
- runtime startup / shutdown
- shift / offline / node state handling

## Required behavior in hot zones
- plan first
- explain invariant impact
- keep the change small
- run targeted tests
- summarize remaining risk

## Strong bias
- preserve existing seams
- avoid speculative abstractions
- avoid style-only churn
