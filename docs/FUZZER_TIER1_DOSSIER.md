# Fuzzer Tier-1 Dossier — Shift / Z / RMR Alphabet Expansion

**Status:** implementer contract (2026-07-09). To be built by an implementer (possibly a different LLM) and **adversarially reviewed by the architect** against this document. Read the whole thing before writing code.

**Tracking:** `PRRO_GATE-hov` — "Extend invariant fuzzer Tier-1 with shift/Z/RMR state machine".

---

## 0. The one rule that matters most

This project's moat is **fuzzer-proven correctness**. The invariant fuzzer is a **model-based differential harness**: a hand-written `RefModel` predicts the outcome of each operation INDEPENDENTLY, the real system runs the operation, and an oracle compares the two. It has caught 3 real production bugs before merge (#192, the P1 boot-resume twin, the B10 offline-drift).

**THE RULE:** the `RefModel` must predict from **first principles**, NEVER by reading or mirroring what the implementation produced. A model that adopts the impl's output is **tautological** — it passes every test and proves NOTHING. That is worse than no test, because it looks like coverage while giving none. Every new prediction you add must be accompanied by a **teeth canary** (§7) proving the oracle goes RED when the impl is deliberately broken. This is non-negotiable and is the first thing the review checks.

**RED-first TDD:** write the failing model/oracle assertion first, run it, watch it fail for the RIGHT reason, then implement. No production/model code without a preceding failing test.

---

## 1. Intent & the gap

The fuzzer's `Op` alphabet currently covers: `OnlineSell/OfflineSell/OnlineReturn/OfflineReturn`, `Drain/RepeatDrain`, `GoOnline/GoOnlineWithoutBacklog/OfflineSellDuringGoingOnline`, `Crash/Reboot/RepeatReboot`, `DuplicateIdemKey`, `SellWithClosedShift`. Model: `apply_sell/apply_return/apply_drain/apply_go_online`.

**The gap:** the entire **shift / Z / session lifecycle is OUT of the alphabet.** Shift appears only as a static guard (`SellWithClosedShift`), NOT as a driven state machine. There is no `Op::ShiftOpen/ShiftClose/ZReport` and no `apply_shift_*`. So the 9-state shift machine, its 14 edges, RMR escalation, Z tax-surface, and the offline session lifecycle are **unfuzzed** — and this is the most complex and highest-risk machine in the system.

**Tier-1 closes this gap.** It is the single highest-ROU fuzzer investment before the pilot campaign.

---

## 2. Files (verify actual structure before editing)

`rust/prro/tests/invariant_fuzzer/`:
- `op.rs` — the `Op` alphabet enum + any op metadata.
- `model.rs` — the `RefModel` (`apply_*`, state tracking). **B10 already extended this** with two-doc offline BEGIN/END prediction (`approach-d`: interp emits `RealOutcome::Recovered` → routed to `check_ledger_delta`). Study that pattern — you extend it.
- `interp.rs` — `FuzzCtx`; drives each `Op` through the REAL system (production write-path / drain / go-online). Note the per-case temp-DB lifecycle (there was a leak — clean up any temp DBs you add).
- `oracle.rs` — `check_differential`, `check_ledger_delta`, `check_doc_against_mutation`. **Read these carefully** — `check_doc_against_mutation` hard-codes `doc.previous_hash == prior_tip` chain-continuity; a multi-doc op must route through `check_ledger_delta` instead (this exact trap bit B10).
- `strategy.rs` — proptest generators / `Op` sequence strategy.
- The harness entry (`run_harness`) + the directed/proptest tests live in `rust/prro/tests/invariant_fuzzer.rs`.

---

## 3. Scope — the ops to add

Add to the alphabet (confirm exact naming/shape against `op.rs` conventions):
- `Op::ShiftOpen { online: bool }` — drives a `SHIFT_OPEN`. Online → to DPS (SENT→…→ACK). Offline → `OFFLINE_LOCAL_ACK`, and per B10 the lazy DocType=9 BEGIN mints first.
- `Op::ShiftClose { online: bool }` (and/or `Op::ZReport { online: bool }`) — drives the shift close. **Confirm the canonical distinction** between `SHIFT_CLOSE` and `Z_REPORT` in the model (`db/models/enums.rs` DocType + the M3b spec) and model whichever the write-path actually uses to close a shift.

Drive all shift ops through the **production write-path** (like the B10 tests' `drive()` / `production_write_path`), NOT direct seeding — the point is to fuzz the real state machine.

---

## 4. The 9-state shift machine (model it in `RefModel`)

`Created → Opening → OpenedLocalPendingDrain → Opened → ClosingLocalPendingDrain → Closing → Closed / RequiresManualReconciliation / Error`.

Authoritative spec: `docs/superpowers/specs/2026-05-17-m3b-shift-state-expansion.md` (§16 = the operational-reality alignment that overrides earlier sections on conflict). Edges 1–14 there. The `RefModel` must track the shift state per FN and independently predict the transition on each shift op + each fault. The impl's shift transitions live in `services/write_path/stage_acquire.rs` (guard/edge table) + the shift repository; the model must NOT read those — it re-derives the edge from the spec.

---

## 5. RMR as a checked oracle — THE hard, highest-value part

`RequiresManualReconciliation` is "ЧП из ЧП" — extremely rare, and the one place a silent transition-violation would be catastrophic. The oracle must prove RMR fires **EXACTLY by right** and never silently violates a transition (INV-8).

**Confirmed RMR trigger families** (CLAUDE.md persistence model / spec §16.7):
1. **Any W9b drain reject of an `OFFLINE_LOCAL_ACK` backlog doc on `OpenedLocalPendingDrain` / `ClosingLocalPendingDrain`** (edges 6/14) — drain has crossed the local-commit threshold, rollback semantics don't apply. (FN-deregistered-while-offline is the real-world subtype.)
2. **Ambiguous wire timeout for online `SHIFT_OPEN` / `Z_REPORT`** (edges 4/12) — cannot determine if DPS accepted.
3. **Operator-driven force seam.**

The oracle must assert three things, all independently modeled:
- (a) RMR **fires** when (and only when) a trigger family occurs;
- (b) RMR does **NOT** fire on any non-trigger reject/fault (no over-escalation);
- (c) the transition **into** RMR is a **whitelisted edge** (no silent violation of the transition table).

Model the fault ops needed to hit these: an ambiguous-timeout fault on a shift op, and a drain-reject of an OLA doc.

---

## 6. advance-at-SEND / D2 / D5 fidelity — extend to shift docs

The model (post-B10) already reflects seed-advance for sell/return. Extend the **same** fidelity to `SHIFT_OPEN` / `Z_REPORT`:
- **advance-at-SEND:** the online chain seed advances atomically with the `server_fiscal_no` stamp at the `Sending→Sent` CAS — **that CAS is the issuance moment, not ACK.**
- **pre-SENT reject** → `Sending→Rejected` CAS: a non-issued `Rejected` row legitimately rests (lnd consumed, seed **NOT** advanced) — **D2**.
- **post-SENT reject** → `RequiresManualReconciliation`, **NEVER** `Rejected` (seed NOT rolled back; the `(Sent, Rejected)` edge was removed in A.3 PR-B) — **D2 expanded**.
The model must predict these for the shift docs too. Getting the pre-SENT/post-SENT split right for shift docs is a core correctness target.

---

## 7. Teeth (MANDATORY — reviewed first)

For EVERY new model/oracle assertion, ship a **revert-canary** test proving the differential goes RED when the impl is deliberately broken. Minimum set:
- **Shift-edge canary:** make the model predict a shift transition; a test that forces an ILLEGAL transition in the impl (or reverts a legal edge) → the shift-state oracle REDs.
- **RMR canary:** a model that SUPPRESSES an RMR escalation must DIVERGE from the impl that correctly escalates (and vice-versa: a model that spuriously escalates diverges from an impl that correctly does not).
- **advance-at-SEND canary:** break the seed-advance / pre-vs-post-SENT split for a shift doc → the ledger-delta REDs.

Each canary states the exact revert and the resulting RED. If you cannot write a canary that REDs, your assertion is vacuous — fix it.

---

## 8. RED-first pins (write these failing first)

1. `Op::ShiftOpen{online:true}` → model predicts `Opening→Opened` on SENT-ACK; oracle matches impl.
2. `Op::ShiftOpen{online:false}` → model predicts the B10 lazy BEGIN + `OpenedLocalPendingDrain`; drain → `Opened`.
3. `Op::ZReport{online:true}` → `Opened→Closing→Closed`.
4. **RMR:** ambiguous-timeout on online `SHIFT_OPEN` (edge 4) → model + impl both → RMR, whitelisted edge.
5. **RMR:** drain-reject of an OLA doc on `OpenedLocalPendingDrain` (edge 6) → RMR.
6. **D2:** pre-SENT reject of a `SHIFT_OPEN` → `Rejected`, seed NOT advanced. post-SENT reject → RMR, seed NOT rolled back.
7. Teeth canaries (§7).
8. **Proptest:** sequences composing shift ops with sell/offline/crash/drain run with zero divergence (composition, not silos).

---

## 9. Invariants to preserve

- The existing full gate (**nextest 1751-pass baseline** on `main`; confirm the current count) stays green + your new tests add coverage.
- **Model/test-only** — no production `src/` change UNLESS the fuzzer finds a real bug; then fix it RED-first as a separate, clearly-labeled commit (that's the fuzzer doing its job — flag it loudly, don't bury it).
- Single-writer / idempotency / D2 — the model reflects them; never encodes a violation as "expected".
- **Determinism:** seeds persist (fix the `FileFailurePersistence` path if it warns); a found failure lands as a checked-in regression seed + a directed test.

---

## 10. Coordination (avoid collision — important)

- Work on a **SEPARATE branch in a SEPARATE worktree, based off `main`.** Do NOT base off the B10 branch `worktree-agent-a8e9bc72a98df18bc` (it has in-flight offline two-doc-model changes — you'd entangle). Do NOT commit to `main`, the B10 branch, or any other feature branch. The branch/worktree name should make the scope obvious, e.g. `fuzzer-tier1-shift-z-rmr`.
- If B10 merges to `main` before you finish, **rebase onto the new `main`** (the offline BEGIN/END model lands there). Coordinate the base point with the architect.
- Never force-push, never push to `main`, never rewrite shared history.

---

## 11. Review criteria (what the architect verifies — build to pass these)

1. Full gate green: `cargo nextest run -p prro --features test-support` all-pass + fmt + clippy (verified from OUTPUT, not exit code).
2. **Every new assertion has a proven teeth canary** (revert → RED). No exceptions.
3. **RMR oracle fires exactly by right** — no over/under-escalation; every RMR transition is a whitelisted edge; non-triggers never RMR.
4. The new ops **genuinely exercise** the shift machine — a deliberate-bug canary catches (not no-ops).
5. **advance-at-SEND / D2** correct for shift docs (pre-SENT→Rejected / post-SENT→RMR split).
6. **Zero tautology** — the model predicts independently; the oracle can disagree with reality.
7. Determinism (seed-persist + regression seeds checked in).
8. Delivery in the 7-item format + a table: `{new Op/fault → model prediction → teeth canary → RED-proof}`.

---

## 12. What this unlocks

Closes the biggest fuzzer alphabet gap; the moat now covers the most complex/risky machine (9-state shift + RMR). Next: **Tier-2** (B8/B9 sign→persist crash-window, T=112 pool-lifecycle ops, node-mode `BLOCKED/STOP_MODE/CRYPTO_DEGRADED` completeness + INV-3 as oracle); **Tier-3** (multi-FN concurrent interleaving — per-FN single-writer under fuzz, the fleet dimension). And the discipline pin: from here, **any new Op/state/edge enters the fuzzer alphabet in the same PR that adds it**, and the fuzzer becomes a **required** CI gate. See `docs/QUALITY_CHARTER.md` §5/§8.
