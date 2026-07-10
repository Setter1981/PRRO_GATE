# Fuzzer Tier-2 «RAGE» — Implementer Dossier (handoff contract)

**Author:** architect · **Date:** 2026-07-10 · **Executor:** companion LLM (same handoff model as `docs/FUZZER_TIER1_DOSSIER.md`, which produced the merged `fuzzer-tier1-shift-z-rmr` → #251).
**Baseline:** `main` AFTER #252 (B10 offline handshake + the Tier-1×B10 reconciled fuzzer — `invariant_fuzzer` 106/106, full gate 1775/1775). All file refs relative to `rust/prro/tests/invariant_fuzzer{.rs,/}`.

## 0. Mission

Make the model-based differential fuzzer **maximally mean**: every production surface the gateway now has (or gains) must be predictable by the RefModel, checked by an INDEPENDENT oracle, and provably non-tautological (teeth). This dossier consolidates EVERY known uncovered surface into prioritized waves. Work wave-by-wave; each wave lands as its own reviewed increment (TEST-ONLY unless a wave explicitly says otherwise).

**Non-negotiable disciplines (apply to every wave):**
- **RED-first**: a new oracle/op lands with a failing pin first where feasible.
- **Teeth per oracle**: for EVERY new oracle, prove it bites — revert/break the guarded production invariant locally, watch the fuzzer go RED with clean shrinking, restore byte-exact. Record the teeth in the PR text.
- **Known-red fencing, never model-fudging**: an unresolved production question is EXCLUDED from the generator + fenced by an inverted-assertion known-red test (the `teeth_d5_shift_doc_superseded_known_red` pattern at `invariant_fuzzer.rs:333` — a test that FAILS if anyone silently normalizes the gap). No silent caps: anything excluded gets `log`ged in the PR + an issue.
- **TEST-ONLY**: production findings become ISSUES (like PRRO_GATE-eid), not drive-by prod edits. One exception is called out in W2-1.
- **Determinism**: no wall-clock/randomness outside proptest; regression seeds committed (`invariant_fuzzer.regressions`, union-merge discipline).
- **Gate**: per wave — `cargo nextest run -p prro --features test-support --test invariant_fuzzer` green + the full crate gate + fmt + clippy, read from output.

## 1. Current coverage (do not re-build)

Ops: SELL/RETURN (online+offline), ShiftOpen/ZReport (online+offline, Tier-1), Drain/RepeatDrain, GoOnline(+variants), Crash(Sign/Send/Kvt1)/Reboot/RepeatReboot, DuplicateIdemKey, SellWithClosedShift. Oracles: per-doc differential + two-doc ledger-delta (B10 BEGIN/END), Z-aggregation (independent re-derivation + drift teeth), shift-state differential, RMR-tombstone reentry, Z-quiescence, boundary-chain teeth (BEGIN/business/seed, now shift/Z-wide). Model: lazy-BEGIN prediction for ALL offline doc-types, END-as-online-issuance at drain finalize, D2/advance-at-SEND split, write-gate sibling.

## 2. WAVE 1 — finish the shift machine (Tier-1.5)

1. **Edges 4/12 trigger fidelity.** Today the model reaches RMR for online SHIFT_OPEN/Z via DPS *Reject*; spec §16.7(2) says the edge-4/12 family is the **ambiguous wire timeout** (cannot determine if DPS accepted). Add a `WireResponse::AmbiguousTimeout` (or equivalent) the stub can serve on the send leg of shift-class docs; model: online SHIFT_OPEN/Z with ambiguous-timeout → shift `RequiresManualReconciliation`, doc rests per the impl (read the impl truth from `stage_send`/`error_routing` timeout classification — do NOT guess; if the impl maps a wire timeout to a retryable transport class instead of RMR, pin THAT and flag the spec-vs-impl gap as an issue).
2. **Edge 14 distinct labeling.** The drain-reject-of-offline-Z path currently folds into the generic drain-reject arm. Give it a distinct model/audit label so a regression in the *specific* edge is attributable.
3. **BEGIN-then-refuse composite, first-class.** Replace the `Fault` defer in `apply_offline_shift_open`/`apply_offline_z_report` (duplicate/wrong-state business doc after a hoisted BEGIN — see the pin `offline_shift_open_refused_after_lazy_begin_mints_begin_row`) with a real predicted outcome (e.g. widen the two-doc `Recovered` route to a `begin_only` branch: interp already reports `b10_lazy_begin_interposed`; predict BEGIN@lnd, code#1, seed advance, business-doc absent). Keep the directed pin.
4. **Commit the closed-shift fail-closed matrix** (the +81 WIP: `online_closed_shift_blocks_receipt_z_and_wrong_lane_as_true_noops`) if not already landed.
5. **Directed SHIFT_OPEN write-gate differential** (model already has `has_write_gate_blocker` at `model.rs:374`; SELL/RETURN have directed pins, SHIFT_OPEN does not).
6. **Z-aggregation prediction in RefModel** (today only the DB-reading oracle re-derives; the model cannot predict Z turnover standalone). Optional if costly — the oracle is independent already; do it only if it does not force mirroring.
7. **STILL EXCLUDED (known-red fenced, do NOT enable):** `Superseded`/`NotFound` scripts on shift-class ops stay OUT of the generator until the PRRO_GATE-eid production ruling lands (online superseded-held shift is an unbounded benign-hold today; enabling generation now would force the model to either fudge or flake). The known-red tooth stays.

## 3. WAVE 2 — the wire/envelope axis (the `-8` lesson)

1. **Envelope-format oracle.** The month-long `-8` hunt happened because NOTHING validated the wire ENVELOPE against the doc's lane: our drain docs sent an epoch `date_time` where DPS requires the 14-digit `<TS>`-int (`kyiv_comp_date`, WebCheck parity, fixed in `9a7ad7c`). Teach the scripted-DPS stub to VALIDATE every submitted envelope: (a) `date_time` format matches the lane — offline-origin + BEGIN/END → 14-digit comp-date equal to the signed `<TS>`; online → the epoch convention; (b) `id_offline` presence matches shaped-ness (offline-shaped docs carry the code; online/bare-MAC docs carry EMPTY); (c) `local_number`/DI sanity. A violation → the stub rejects with a distinct code → differential RED. **Teeth:** flip the `kyiv_comp_date` branch in `stage_send.rs` back to epoch for one doc-type → fuzzer must redden. This makes the exact bug class that cost us a month a one-run catch, forever. (This wave may need a small `#[cfg(test)]`-visible accessor on the envelope — the ONE allowed prod touch, additive-only.)
2. **XML-shape pins per doc-type**: the stub asserts the canonical body shape it receives (BEGIN `<C T='109'>` offline-shaped `<MAC ID>`, END `<C T='110'>` bare `<MAC>`, Z `<Z NO>` online bare) — a drift in `emit_offline_session_boundary`/Z-builder shape reddens without a live cabinet.

## 4. WAVE 3 — pool lifecycle + crash windows

1. **Pool ops**: `Op::AskOfflineCodes(n)` (T=112 as a first-class op — advances the chain per B10 reality, feeds the pool), pool-exhaustion sequences (the model already predicts abort-vs-rowless per lane; generate them), replenish thresholds. When the **reserve-floor** increment lands in prod (backlog: gate SELL/RETURN at ≤2 codes so Z can always close), add the floor to the model + an oracle "a shift is NEVER wedged un-closable for lack of a code" — that is the invariant the floor exists for.
2. **Crash windows**: generative `Crash(Sign)` buried-SIGNED (P1 follow-up), `Crash × Return` (R1 residual, pre-authorized), crash between the hoisted BEGIN and its business doc, crash between Z-quiescence and Z-send, crash mid-drain between content-ACK and END-mint. Each: boot-resume must terminalize per the #192 pin ("no doc rests non-terminal at a quiescent boundary") — that IS the oracle.
3. **Idempotency width**: `DuplicateIdemKey` replays for shift/Z ops (today receipt-only).

## 5. WAVE 4 — mode completeness + input hostility

1. **Node-mode alphabet**: `BLOCKED` / `STOP_MODE` / `CRYPTO_DEGRADED` entry ops + the INV-3 oracle (channel switch forbidden with an open shift) + fail-closed behavior of every op in each mode (true-noop matrices, the closed-shift-matrix pattern).
2. **Byzantine DPS decode** (input axis; backlog `byzantine_dps_handling`): a stub mode serving hostile responses — truncated CMS, garbage XML, wrong-id acks, oversized fields, unknown status codes — oracle: the decode layer NEVER panics, classifies fail-closed (no doc advances on garbage), node does NOT flip offline on a single byzantine response. Teeth: weaken one decode guard → RED.
3. **Receipt-input fuzz** (backlog `receipt_fuzz`): property-generate hostile canonical payloads (sum bounds incl. the 50k cash-cap gap, tax-group edge rates, huge line counts) → fail-closed pre-inbox 422s never mint rows; valid extremes round-trip the Z-aggregation oracle (banker's rounding at scale).

## 6. WAVE 5 — time axis (lands WITH the 168h/36h increment)

When the shift/offline time-limits increment lands in prod (operator ruling 2026-07-10: 168h/month offline + 36h/shift tracked FROM DOCUMENTS, enforcement config-toggleable, **auto-Z unconditional**): add `Op::AdvanceClock(dur)` (model + a test-clock seam), model the two budgets derived from documents, and pin the two invariants: (a) with enforcement ON, ops beyond a budget are refused fail-closed; (b) **regardless of the toggle, a shift NEVER crosses the limit without a Z** — the unconditional auto-Z is the oracle. Fence with known-red until the prod increment exists.

## 7. WAVE 6 — fleet (Tier-3, pre-fleet phase)

Multi-FN concurrent interleaving: N independent FNs, interleaved op streams, per-FN single-writer isolation (INV-2) as the oracle (no cross-FN lnd/seed/pool bleed); then recovery-class routing coverage (MacReseed/KeyRotationPending/TechSupport...). Design the harness so FN-count is a parameter (fleet = 200).

## 8. Deliverables per wave (7-item format each)

Intent · files · RED-then-GREEN evidence + **teeth transcript** (what was reverted, the RED, the restore) · gate output (fuzzer + full crate + fmt + clippy) · known risks + everything EXCLUDED (with issue refs — no silent caps) · invariant check · next step. Branch per wave off current `main`; regression seeds committed; architect (me) reviews each wave with independence/teeth verification before merge.

## 9. Priority ruling

W1 → W2 are the pre-campaign bar (finish the machine we just shipped + never re-live the `-8`). W3 before the live soak. W4-W5 ride their prod increments. W6 gates the fleet phase. If capacity forces a cut inside a wave, cut BREADTH not DISCIPLINE — a smaller alphabet with proven teeth beats a wide tautology.
