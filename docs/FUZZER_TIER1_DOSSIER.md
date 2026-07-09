# Fuzzer Tier-1 Implementation Contract

**Issue:** `PRRO_GATE-hov` — Extend invariant fuzzer Tier-1 with shift/Z/RMR state machine.

**Audience:** an implementer working in a separate branch/worktree, with an architect reviewing the result adversarially.

**Status:** binding handoff contract. If implementation reality conflicts with this document, stop and bring back the smallest concrete decision point. Do not silently widen scope.

---

## 1. Objective

Extend the Rust invariant fuzzer from "receipt/offline/drain focused" to a real shift/Z/RMR state-machine fuzzer.

The current fuzzer already covers:
- `OnlineSell`, `OfflineSell`, `OnlineReturn`, `OfflineReturn`
- `Drain`, `RepeatDrain`
- `GoOnline`, `GoOnlineWithoutBacklog`, `OfflineSellDuringGoingOnline`
- `Crash`, `Reboot`, `RepeatReboot`
- `DuplicateIdemKey`
- `SellWithClosedShift`

The gap is the product's highest-risk machine: shift lifecycle, Z-report close, offline local pending drain, and manual reconciliation. Today shift is mostly a static guard in the fuzzer, not a driven state machine.

Tier-1 is complete only when the fuzzer can generate and check shift lifecycle operations composed with existing sell/offline/crash/drain operations.

---

## 2. Branch And Worktree Contract

The implementer must work in a new branch and worktree based on `main`.

Recommended setup:

```bash
git fetch origin
git worktree add ../prro_gate-fuzzer-tier1 -b fuzzer-tier1-shift-z-rmr origin/main
cd ../prro_gate-fuzzer-tier1
bd update PRRO_GATE-hov --claim --json
```

Rules:
- Do not work on `main`.
- Do not base this work on the B10 branch or any B10 worktree.
- Do not commit to this dossier branch unless the task is explicitly to edit the dossier.
- Do not force-push.
- If B10 lands before this work finishes, rebase onto the new `origin/main` and re-run the fuzzer gate.

Reason: B10 changes the offline two-doc model surface. Mixing bases will make model/oracle failures ambiguous.

---

## 3. Product Surface To Add

Add fuzzer operations for:
- online shift open
- offline shift open
- online shift close / Z-report
- offline shift close / Z-report

The implementer must confirm the exact production operation split before coding:
- whether `ShiftClose` and `ZReport` are distinct `DocType` inputs in the current Rust path;
- which operation actually closes a shift through the production write path;
- where wire `Z_REPORT` numbering/allocation happens.

The fuzzer API may choose either explicit variants or a typed enum, but the generated alphabet must be readable in shrunk failures. Acceptable shapes include:

```rust
Op::ShiftOpen(DpsScript)
Op::OfflineShiftOpen
Op::ZReport(DpsScript)
Op::OfflineZReport
```

or:

```rust
Op::ShiftDoc {
    kind: ShiftDocKind,
    lane: FiscalLane,
    script: DpsScript,
}
```

Do not add a generic stringly operation. Fuzzer failures must be self-explaining.

---

## 4. Required Model Semantics

The `RefModel` must independently model the 9-state shift machine:

```text
Created
Opening
OpenedLocalPendingDrain
Opened
ClosingLocalPendingDrain
Closing
Closed
RequiresManualReconciliation
Error
```

The authoritative source is:

```text
docs/superpowers/specs/2026-05-17-m3b-shift-state-expansion.md
```

Section 16 is authoritative where it conflicts with earlier wording.

The model must add explicit shift methods or an equivalent clear structure:
- `apply_shift_open`
- `apply_shift_close` / `apply_z_report`
- shift-aware drain handling for `OpenedLocalPendingDrain` and `ClosingLocalPendingDrain`
- shift-aware crash/fault classification where already represented in the fuzzer

The model must not query the database or reuse production transition helpers to decide expected shift state. It may compare against production after the real op runs, but prediction must come from model state and the spec.

---

## 5. RMR Oracle Contract

`RequiresManualReconciliation` is not just another terminal state. It is an oracle target.

The implementation must prove all three properties:
- RMR fires when legally required.
- RMR does not fire on non-trigger outcomes.
- Every transition into RMR is a whitelisted shift edge.

Required trigger families:
- edge 4: ambiguous timeout for online shift open, `Opening -> RequiresManualReconciliation`
- edge 12: ambiguous timeout for online Z-report / shift close, `Closing -> RequiresManualReconciliation`
- edge 6: drain reject of an offline-local shift-open artifact, `OpenedLocalPendingDrain -> RequiresManualReconciliation`
- edge 14: drain reject of an offline-local Z-report artifact, `ClosingLocalPendingDrain -> RequiresManualReconciliation`

Non-trigger outcomes must be pinned too. A reject before the online SEND boundary is not the same as an ambiguous post-boundary state. Do not collapse everything into RMR.

The oracle must report a useful failure:
- operation sequence;
- model shift state before/after;
- real shift state before/after;
- expected RMR trigger class, if any;
- whether the transition was in the allowed edge set.

---

## 6. D2 / D5 / Advance-At-SEND Contract

The shift docs must follow the same issuance rules as receipt docs.

Online issuance moment:
- the chain seed advances at the `Sending -> Sent` CAS;
- the `server_fiscal_no` stamp and seed advance are the issuance boundary;
- ACK is confirmation, not issuance.

Required split:
- pre-SENT reject: row may become `Rejected`; lnd consumed; seed not advanced;
- post-SENT reject or ambiguity: row must not roll back to `Rejected`; route to `RequiresManualReconciliation` where required; seed is not rolled back.

D5 gate:
- non-issued blocking siblings must still block new online issuance;
- issued offline-origin docs must not be treated as non-issued blockers after local issuance;
- extend any model mirror needed for shift docs consciously, with teeth.

---

## 7. Interpreter Contract

New shift ops must run through production seams.

Required:
- use the real write path entry already used by the fuzzer (`inline::run` or the current production helper);
- create real inbox rows with the proper `DocType` / operation type;
- use `ScriptedDps` for online wire responses;
- use the real offline local ack/drain path for offline shift docs;
- observe real DB state after each op for differential comparison.

Forbidden:
- direct seeding of final `fiscal_documents` rows as the implementation of an op;
- direct mutation of `shifts` to make the model pass;
- special production-only bypasses under test feature flags;
- accepting `NoMutation` for a new op unless the op is genuinely out of precondition and the model predicted that refusal.

If a production seam cannot drive a needed state, stop and document the exact missing seam. Do not reimplement the write path inside the fuzzer.

---

## 8. Generator Contract

The generator must compose shift ops with existing ops. It must not build a siloed "shift-only" test lane.

Required:
- directed tests first;
- only then add the new ops to `strategy.rs`;
- generated sequences must include sells/returns, shift ops, drain, go-online, crash/reboot, and invalid/re-entry operations in the same stream;
- keep shrink quality: prefer flat `Vec<Op>` generation over stateful `prop_filter`-heavy strategies.

Do not overfit the generator to legal-only sequences. Illegal and out-of-precondition intents are useful when the interpreter and model classify them explicitly.

---

## 9. Teeth Protocol

Every new oracle must have teeth. A test that never goes red on a plausible break is not evidence.

Minimum required teeth:

| Area | Required proof |
| --- | --- |
| shift edge oracle | A deliberate illegal or missing shift transition makes the oracle fail. |
| RMR under-escalation | Suppressing a required RMR escalation makes the oracle fail. |
| RMR over-escalation | Escalating on a non-trigger makes the oracle fail. |
| advance-at-SEND | Moving seed advance to ACK or treating post-SENT reject as `Rejected` makes the oracle fail. |
| generated op reachability | A deliberate no-op implementation of a new op is detected by a directed canary. |

Each tooth must name:
- the bug class;
- the expected red assertion;
- the minimal sequence that exposes it;
- the exact revert or mutation target, if known.

It is acceptable for a tooth to be a deterministic directed test rather than a proptest case. It is not acceptable for a tooth to require manual inspection only.

---

## 10. Phased Landing Plan

Land in small, reviewable slices. Do not deliver a giant rewrite.

### Slice 1: Online Shift Close / Z Skeleton

Goal:
- prove the fuzzer can drive a shift-closing doc through production online path;
- model `Opened -> Closing -> Closed`;
- pin advance-at-SEND for this doc class.

Expected files:
- `rust/prro/tests/invariant_fuzzer/op.rs`
- `rust/prro/tests/invariant_fuzzer/interp.rs`
- `rust/prro/tests/invariant_fuzzer/model.rs`
- `rust/prro/tests/invariant_fuzzer/oracle.rs` if shift-state checking needs an explicit helper
- `rust/prro/tests/invariant_fuzzer.rs`

### Slice 2: Offline Shift Close / Edge 14

Goal:
- drive offline Z-report local ack;
- model `Opened -> ClosingLocalPendingDrain`;
- drain ACK reaches `Closed`;
- drain reject reaches RMR through edge 14.

### Slice 3: Shift Open / Edges 4 And 6

Goal:
- support online and offline shift open from a closed/created fixture;
- model `Created/Closed -> Opening -> Opened` and offline `OpenedLocalPendingDrain`;
- ambiguous timeout reaches edge 4;
- drain reject reaches edge 6.

### Slice 4: Generator Integration

Goal:
- add the new ops into `strategy.rs`;
- prove mixed sequences shrink cleanly;
- commit any found regression seed;
- run default fuzzer gate clean.

If any slice finds a production bug, stop and split:
- first commit/test proves the bug red;
- second commit fixes production;
- final commit re-enables/extends generator if needed.

---

## 11. Acceptance Gates

Before review, the implementer must run and report exact output for:

```bash
cd rust
cargo fmt -p prro -p prro_crypto -p prro_escpos -- --check
cargo clippy -p prro --all-targets --no-deps --features test-support -- -D warnings
cargo nextest run -p prro --features test-support --locked -E 'test(/invariant_fuzzer/)'
cargo nextest run -p prro --features test-support --locked
bash prro/tests/check_seed_committed.sh
```

If `nextest` is unavailable locally, use the repo's accepted fallback and state it. Do not mark the work complete without a full fuzzer run unless a real environment blocker exists.

Large-N is recommended before merge:

```bash
cd rust
FUZZ_CASES=4096 cargo nextest run -p prro --features test-support --locked -E 'test(/^harness_(online|offline)_seeded$/)'
```

---

## 12. Review Checklist

The architect will check:
- new ops are in the main alphabet and generator, not isolated side tests;
- each new op reaches production write/drain paths;
- the model predicts independently from the spec;
- RMR fires exactly by edge 4/6/12/14 rules;
- non-trigger rejects/faults do not become RMR by convenience;
- advance-at-SEND is correct for shift docs;
- pre-SENT vs post-SENT behavior is split;
- teeth tests would catch model/impl drift;
- existing fuzzer invariants still run;
- no broad production refactor was smuggled in;
- regression seeds are committed or the seed guard is clean.

---

## 13. Delivery Format

The final handoff must include:

```text
STEP:
Fuzzer Tier-1 shift/Z/RMR slice <n>

GOAL:
<what was closed>

CHANGED FILES:
<files>

NEW OPS:
<op list and intended production path>

MODEL PREDICTIONS:
<state/doc/seed predictions added>

RMR ORACLE:
<which edges covered and which non-triggers pinned>

TEETH:
<table: invariant -> canary -> red proof>

TESTS RUN:
<commands and result summary>

KNOWN GAPS:
<remaining Tier-1/Tier-2 items>

NEXT STEP:
<one bounded next slice>
```

No "done" without the teeth table.

---

## 14. Non-Goals For Tier-1

Do not include unless explicitly approved:
- multi-FN concurrency fuzzing;
- byzantine DPS decode fuzzing;
- differential replay against WebCheck;
- mutation-testing infrastructure;
- full node-mode alphabet for `BLOCKED`, `STOP_MODE`, `CRYPTO_DEGRADED`;
- T=112 offline-code ask/replenish/exhaust lifecycle;
- broad production rewrite of write path, drain, repositories, or migrations.

These are valuable, but they are Tier-2/Tier-3. Tier-1 is shift/Z/RMR correctness.

