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

## Ordering / totality checklist (MANDATORY when a change picks "the newest" or "the winner")

Added 2026-07-30 because this class of defect recurred twice in one slice, both times passing design
review and both times caught only by a test that constructed the tie deliberately (bd PRRO_GATE-hpc).
Designs here are written at the right altitude but under-verify ordering. Run these EVERY time a change
introduces or consumes an ordering, a "latest", a tie-break, or a new source of truth.

**Is it a total order?**
- List the ordering keys. Construct, on paper, two rows that are EQUAL on all of them. If you can, the
  order is partial and the pick is whatever the query plan returns — i.e. undefined.
- `ORDER BY ... LIMIT 1` over a partial order is a silent non-deterministic choice, not a selection.
- Granularity traps: `datetime('now')` is **second**-granular. A monotonic counter is only monotonic for
  the events that advance it — an operation that does NOT allocate an ordinal (e.g. a T=112 replenish
  allocates no `lnd`) leaves consecutive events tied.
- If ties are possible, add a final key that is provably monotonic for the write pattern (e.g. `rowid`
  on an append-only table) and say WHY it is monotonic.

**Who wins at equality — and can you prove it from the domain?**
- Strict `>` vs `>=` must be a domain argument, not a preference. Example that must be stated in the
  code: a doc that CONSUMED ordinal `k` is later than a witness that merely RESERVED `k`, so at equal
  ordinal the doc wins.
- Write the tie case as a test with the two rows forced to tie on every leading key. A test that "did
  not happen to tie" (rows created seconds apart) proves nothing — verify the tie actually occurred.

**Completeness: every consumer of the truth you changed**
- Enumerate EVERY reader of the projection/marker/state you touched, and prove each one either inherits
  the new rule or is deliberately excluded (with the reason).
- A shared projection fixes only the consumers that ask it. A consumer carrying its OWN running state
  (e.g. a walk with a local `expected`) does NOT inherit — it needs the same rule applied separately.
  Two consumers with different rules is a silent divergence, not a style issue.
- Enumerate the branches of the match: both-present, only-A, only-B, neither. Each needs a test.

## Implementation rules
- prefer existing seams
- preserve signatures unless there is a concrete reason
- if a signature expands, audit all callers
- if transport behavior changes, review send + poll + finalize + reconcile
- if shift/offline state changes, review both write path and reconciliation
- if schema changes, review migrations and restore implications

## Mandatory verification ideas
Choose the smallest set that proves the change:
- targeted `cargo nextest` run for the touched service or transport (the Python gateway is dead —
  everything runs through `rust/`)
- regression test for the discovered bug
- one integration path proving end-to-end state movement
- smoke test for startup/runtime if wiring changed
- for any ordering/tie-break change: a test that FORCES the tie (see the ordering checklist above)
- prove each new test bites: revert the fix, watch it go RED, restore
