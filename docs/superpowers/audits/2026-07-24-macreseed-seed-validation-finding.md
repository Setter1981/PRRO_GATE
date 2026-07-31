# Finding: `resolve_operator_pending` does not validate the operator's MacReseed seed

**Status:** OPEN — candidate `bd`-task (potential prod hardening)
**Severity:** MEDIUM (operator-error chain-corruption risk; no fail-closed; caught only by a later
`invariant_scan`, not at the point of the mutation)
**Discovered-from:** CS-3 fuzzer-oracle Increment 1b (generative `OperatorComplete`), branch
`fuzzer-cs3-oracle`, commit `c18a5907`. Surfaced by the generative completion×prehistory exploration
(`harness_online_seeded`).
**Component:** `rust/prro/src/admin.rs::resolve_operator_pending` →
`rust/prro/src/db/repositories/delivery_reservation.rs` `complete_operator_pending`
(`OperatorResolution::MacReseed { seed }` branch, ~1431-1437).

---

## Summary

`OperatorResolution::MacReseed { seed }` (`-12` MacReseedPending recovery) takes the corrected chain
seed **verbatim from the operator** and calls `node_advance_seed(tx, fn, seed)` with NO validation
that `seed` matches the expected chain tip (the held document's `previous_hash` / the last issued
doc's `unsigned_xml_sha256`). A wrong seed re-bases `node_state.last_known_unsigned_xml_sha256` to a
value unrelated to the document chain, corrupting it. There is **no fail-closed** at the completion —
the corruption is only detected later by `invariant_scan` as a `ChainSeedMismatch` violation.

## Reproduction (fuzzer)

Minimal generated op sequence (`harness_online_seeded`):

```
[ OnlineSell(DpsScript([Superseded])),   // → held: SENDING, PENDING_APPLY, STOP_MODE, fence
  OperatorComplete(MacReseed) ]          // resolve_operator_pending(MacReseed { seed: <arbitrary> })
```

The completion succeeds (prod does not reject it), advances the seed to the arbitrary value, moves
the doc to `RequiresManualReconciliation`, and un-halts the node. The next `assert_clean` /
`invariant_scan` reports:

```
ledger invariant scan found 1 violation(s):
    ChainSeedMismatch { ... }
```

## Why it matters

- In production the operator supplies the seed by hand (a corrective `-12` MAC reseed). A typo or a
  stale value is not rejected at the completion — the chain is silently corrupted and the FN can only
  be recovered by another manual intervention after `invariant_scan` surfaces it.
- The completion is otherwise a fail-closed, all-in-one-tx surface (origin cross-check, full-authority
  CAS, mode CAS) — this is the one input it trusts without a guard.
- MacReseed is meant for a `MacReseedPending` (BadHashPrev) hold; the fuzzer also applied it to a
  `Superseded` (NoResponse) hold, where no reseed is warranted at all — prod does not check the
  resolution kind against the hold type either. Both are the same missing-validation class.

## Proposed hardening (not yet implemented — prod is FROZEN for the fuzzer work)

Options (pick per operational policy):
1. **Validate the seed** against the expected chain tip in `complete_operator_pending` before
   `node_advance_seed`; fail closed (`CompletionError`) on mismatch, so a wrong operator seed rolls
   the whole tx back instead of corrupting the chain.
2. **Gate MacReseed by hold type** — only accept `MacReseed` for a `MacReseedPending` reservation
   (reject it for `SubmittedUnknown` / `NoResponse` / other holds).
3. At minimum, surface the `ChainSeedMismatch` risk to the operator at completion time (a confirm /
   dry-run diff) rather than only in a later scan.

## Fuzzer handling (this increment)

- `MacReseed` is EXCLUDED from the 1b generator (it needs a valid operator seed the fuzzer cannot
  construct generatively). The enum variant + interp mapping remain for a directed valid-seed test in
  a later increment. This is a documented scope boundary, NOT a model work-around of a prod defect.
- The finding is recorded here + in `docs/CS3_FUZZER_ORACLE_DOSSIER.md`.
