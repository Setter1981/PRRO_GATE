---
name: safe-write-path-change
description: Checklist for changes to write_path, reconciliation, shifts, offline, transports, runtime, and persistence. Claude should load this automatically in risky backend tasks.
user-invocable: false
---

Before editing a risky backend path, run this checklist mentally and reflect it in your plan.

## Pre-edit checklist
- What state machine is touched?
- Could this affect idempotency?
- Could this alter transaction boundaries?
- Could this change recovery behavior?
- Could this break shift/channel lock assumptions?
- Could this impact offline limits or code allocation?
- Could this change transport ACK / pending semantics?
- Could this alter repository contracts or migrations?

## Implementation rules
- prefer existing seams
- preserve signatures unless there is a concrete reason
- if a signature expands, audit all callers
- if transport behavior changes, review send + poll + finalize + reconcile
- if shift/offline state changes, review both write path and reconciliation
- if schema changes, review migrations and restore implications

## Mandatory verification ideas
Choose the smallest set that proves the change:
- targeted pytest for touched service or transport
- regression test for discovered bug
- one integration path proving end-to-end state movement
- smoke test for startup/runtime if wiring changed
