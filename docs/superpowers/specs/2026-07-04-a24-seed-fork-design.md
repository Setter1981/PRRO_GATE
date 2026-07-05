# A2.4 online-lane seed-fork — DESIGN (DRAFT)

**Status: DRAFT — for architect adjudication → external audit → LOCK.** Not a code contract; no
implementation is authorized by this document. Evidence + full option matrix + anchor provenance live in
the companion dossier: [`2026-07-04-a24-seed-fork-dossier.md`](./2026-07-04-a24-seed-fork-dossier.md).

**Gates it belongs to:** un-ignoring the RED-pin `m1_02_online_seed_fork_a24_prerequisite`
(`rust/prro/tests/kill_point_matrix.rs:2543`) + flipping the prod binding (`runtime/supervisor.rs:188`)
= roadmap A.3 (RS3 A2.4). This design is the **prerequisite** for that flip.

**Frozen-invariant posture:** #1 (no network/crypto in long SQLite tx) and #2 (single-writer per FN) are
preserved — the advance lands inside the existing `stage_send` 4-b `with_immediate` tx, adding only a
same-tx `node_state` read + UPDATE (no I/O). #3 (channel switch forbidden with open shift), #4
(idempotency), #8 (recovery preserves state-machine) frame the open decisions below.

---

## 1. Recommendation

**Adopt Option 0 — advance-at-SEND (Batch C design (A)).** Advance
`node_state.last_known_unsigned_xml_sha256` to the doc's `unsigned_xml_sha256` inside the `Sending→Sent`
`with_immediate` envelope in `stage_send` (landing point: the `WireDecision::Sent` block,
`stage_send.rs:1373`), guarded by a pre-advance drift-assert mirroring `stage_offline_ack.rs:389-395`,
applied only on the fresh `Sending→Sent` `Applied` CAS, then generalize the `stage_finalize` gate
(`:288`) into a unified "already-advanced-at-issuance" predicate.

**Why (net):** it makes online issuance **symmetric** to the landed offline M2-01 mechanic — the
online-ACK-only special case disappears and the seed advance becomes lane-uniform ("issued = crossed the
local-commit threshold"; offline = `OFFLINE_LOCAL_ACK`+, online = `SENT`+). That is a **one-time
semantic simplification**. The alternatives (options 2/3) keep the predicate but pay a **permanent
liveness tax** — they forbid two docs resting at `SENT`, and `SENT` is a *legitimate* resting state
(empty-`data_sign` Hold, pinned by `tests/invariant_scan.rs::stuck_non_terminal_excludes_legit_resting_states`),
so they reintroduce a head-of-line stall on exactly the Hold path that produces the fork.

**Validated against constraints (dossier §3):** design (A) is **not** invalidated. It does not widen the
buried-SIGNED crash window (#192/#196/P1 stay intact — that risk is option 1, rejected). It is symmetric
to M2-01, not parallel. Two things it *adds* over the Batch C text — a larger lockstep-consumer set
(§2 below) and a discriminator open decision (§4) — are enrichments, not blockers.

---

## 2. Mandatory lockstep-consumer set

"Issuance moment = SENT" redefines the online issued-predicate, so **every** consumer moves together.
Full table with anchors + per-option edits in dossier §1b. Summary of what an implementation MUST touch
under Option 0:

- **Advance + gate:** `stage_finalize.rs:288/304/315` (move advance to SEND; generalize gate).
- **Audit walk:** `invariant_scan.rs:269` (online arm → SENT+).
- **Boot projection SSOT:** `last_issued_unsigned_xml_sha256` SQL literal `fiscal_documents.rs:933`
  (online arm → SENT+; fn @ `:916`). `boot_phase.rs:1810` **stays SSOT-only** — it must **never** encode its own
  predicate (ruling; keeps NC-03 = pure projection of the issued-tail).
- **Fuzzer:** RefModel online advance `tests/invariant_fuzzer/model.rs:270` (→ SENT); add an online
  advance-trigger tooth (D3 today guards only the offline set — a silent-drift blind spot).
- **Replay:** `tests/webcheck_replay.rs:757-761` `is_issued` (online arm → SENT+).
- **Comment-pins:** `stage_finalize.rs:280-288`, `backlog_drain.rs:930-937`.

`last_ack_unsigned_xml_sha256` (`fiscal_documents.rs:871`) is **ACK-only by design** — do **not** widen it.

**Shape sub-recommendation:** collapse the two hardcoded online-`ACK` literals (`invariant_scan.rs:269`,
`fiscal_documents.rs:933`) into **one shared predicate** — either an `ONLINE_ISSUED_STATES` const
symmetric to `OFFLINE_ISSUED_STATES`, or a single `is_issued(state, offline_fiscal_no)` function. Prefer
the function (one SSOT, no second namespace const — bias against namespace churn). Architect to decide
(§5.4).

---

## 3. Alternatives (ranked)

1. **Option 0 advance-at-SEND** — RECOMMENDED (above).
2. **Option 2 serialize-SENT-per-FN** / **Option 3 seed-fence-at-sign** — viable, predicate untouched,
   Rejected-pin survives verbatim, no discriminator problem, no consumer churn. **Dispreferred:**
   permanent head-of-line-stall liveness tax on the legitimate SENT-Hold path. Keep as the fallback if
   the discriminator decision (§4) proves intractable without schema churn.
3. **Option 1 advance-at-SIGN** — **REJECTED.** Re-opens the buried-SIGNED crash window, conflicts with
   the M3b non-terminal-doc quiescence pin (`SIGNED` at rest is forbidden) and the P1 boot-resume
   semantics (#196). Do not pursue.

---

## 4. Open decision — Rejected-after-SENT + the discriminator problem

Under Option 0 a doc that advanced the seed at SENT and is later DPS-rejected must **escalate
manual-recon with NO seed rollback** (mirror offline; M3b crossed-local-commit pin). This creates a
decidability wrinkle absent offline: **online `REJECTED` is reachable both pre-SENT (seed NOT advanced,
non-issued) and — if allowed — post-SENT (seed advanced, issued).** Offline avoids this because every
`OFFLINE_ISSUED_STATES` member entails "crossed `OFFLINE_LOCAL_ACK`". Full reachability map + anchors in
dossier §5.

**Recommended resolution (architect to confirm):** route a **post-SENT** reject to a state **distinct
from `REJECTED`** — reuse the existing `online_convergence` pattern (doc stays at `Sent`/`Kvt2`, shift
escalates to RMR via `escalate_fn_to_manual_recon`). Then online `REJECTED` stays exclusively
pre-SENT/non-issued, the current pin *"lnd consumed, seed NOT advanced"* survives verbatim for it, the
pin **expands** with a post-SENT branch (does not break), and the issued-predicate stays state-decidable
**without schema churn**.

---

## 5. Open decisions for the architect (adjudication checklist)

1. **Lock issuance moment = SENT** (vs KVT1). Recommend SENT.
2. **Rejected-pin reformulation** — new wording (dossier §8.2): *"pre-SENT reject → REJECTED, seed NOT
   advanced (pin survives verbatim); post-SENT reject → manual-recon, seed NOT rolled back (pin
   expands)."* Confirm citers to update (`CLAUDE.md` M3b persistence-model paragraph; RED-pin barrier
   prose).
3. **Discriminator terminal (§4 / dossier §5.3):** RMR reuse (recommended, no schema) vs new terminal
   state / marker column (schema churn). Includes an RMR-collision audit — RMR is overloaded across
   doc-level and shift-level (dossier §5.1).
4. **Predicate shape (§2):** `ONLINE_ISSUED_STATES` const vs shared `is_issued()` fn.
5. **NC-03 projection ordering:** `last_issued_unsigned_xml_sha256` = `ORDER BY lnd DESC LIMIT 1`. When
   online-SENT enters the issued-set, "highest-lnd issued doc" identifies the tail **iff** live advances
   are monotonic in `lnd`, which the pre-advance drift-assert enforces. Confirm the drift-assert is
   preserved at the SENT advance so the projection stays sufficient.
6. **Config surface (dossier §7):** the flip has **no** config knob today (`InlineWritePath` is not even
   a type; `supervisor.rs:188` hardcodes `UnimplementedWritePath`). Decide: pure hardcoded DI-swap
   (flip = code + external review) vs config/feature-flag + rollback policy (weigh against "replay-forever
   risk" per RS3 A2.4). **Do not pre-design; this is a genuine new decision.**
7. **Fuzzer online-advance tooth:** add one symmetric to D3 (dossier §1b C7).

---

## 6. Landing plan (post-lock — for reference, NOT authorized here)

Ordered so the RED-pin flips green only when the fork is actually closed:

1. Extend `fetch_send_inputs_tx` to carry `unsigned_xml_sha256` + `previous_hash` (mirror
   `fetch_finalize_inputs_tx`).
2. Insert the drift-assert + `update_last_known_xml_sha_tx` in the `stage_send.rs:1373` `WireDecision::Sent`
   block (same 4-b tx), fresh-`Applied`-only.
3. Generalize the `stage_finalize.rs:288` gate → unified "already-advanced-at-issuance" predicate.
4. Move all §2 consumers (walk, SSOT SQL literal, fuzzer model + tooth, replay, comments) in lockstep.
5. Implement the §4 post-SENT-reject discriminator per the locked decision.
6. SW-4: split `ChainSeedMismatch` out of the inline `StructuralDrift` arm (`inline.rs:754-780`) →
   `escalate_fn_to_manual_recon` (dossier §5b).
7. Un-ignore `m1_02_online_seed_fork_a24_prerequisite`; confirm green. (Binding flip + inbox-terminalise
   audit = separate A.3 piece, mandatory external review, lands last.)

This sequence is **not** authorized by this DRAFT — it is recorded so the architect can scope A.3.
