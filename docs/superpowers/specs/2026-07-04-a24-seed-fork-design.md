# A2.4 online-lane seed-fork — DESIGN (v2 — ADJUDICATED)

**Status: ADJUDICATED — decisions D1–D7 LOCKED by the architect; next = external audit → LOCK → A.3
implementation.** Not a code contract; no implementation is authorized by this document. Evidence + full
option matrix + anchor provenance + the D3(ii) verification sweep live in the companion dossier:
[`2026-07-04-a24-seed-fork-dossier.md`](./2026-07-04-a24-seed-fork-dossier.md).

**Adjudication (this revision):** Option 0 advance-at-SEND **accepted**; the discriminator is **amended
from the DRAFT** — it is the `server_fiscal_no` column, **not** a state-set (RMR is ambiguous on both
sides of SENT; §4). The D3(ii) sfn-invariant sweep was executed read-only: **no STOP** (single sfn writer
confirmed; one sharpened A2.4 lockstep requirement). All seven decisions locked in §5.

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
(§2) and the discriminator (now LOCKED to the `server_fiscal_no` column, §4) — are enrichments, not
blockers.

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

**Shape — LOCKED (D4):** collapse the two hardcoded online-`ACK` literals (`invariant_scan.rs:269`,
`fiscal_documents.rs:933`) into a single shared predicate function
**`is_issued(state, offline_fiscal_no, server_fiscal_no)`** in `fiscal_documents.rs` — offline arm = the
existing `OFFLINE_ISSUED_STATES` const, online arm = the D3 `server_fiscal_no` predicate (§4). **No**
`ONLINE_ISSUED_STATES` const (one SSOT, no second namespace const — bias against namespace churn). Both
hardcoded literals die; the walk (C2) + replay (C8) call the fn; the fuzzer tooth (C7) pins the model
mirror against it.

---

## 3. Alternatives (ranked)

1. **Option 0 advance-at-SEND** — RECOMMENDED (above).
2. **Option 2 serialize-SENT-per-FN** / **Option 3 seed-fence-at-sign** — viable, predicate untouched,
   Rejected-pin survives verbatim, no discriminator problem, no consumer churn. **Dispreferred:**
   permanent head-of-line-stall liveness tax on the legitimate SENT-Hold path. Retained as the **fallback
   only if** the D3(ii) sweep had refuted the `server_fiscal_no` invariant — it did not (§4 / dossier
   §5.3), so Option 0 stands.
3. **Option 1 advance-at-SIGN** — **REJECTED.** Re-opens the buried-SIGNED crash window, conflicts with
   the M3b non-terminal-doc quiescence pin (`SIGNED` at rest is forbidden) and the P1 boot-resume
   semantics (#196). Do not pursue.

---

## 4. Rejected-after-SENT + the discriminator — LOCKED (D2 + D3)

Under Option 0 a doc that advanced the seed at SENT and is later DPS-rejected must **escalate
manual-recon with NO seed rollback** (mirror offline; M3b crossed-local-commit pin). **D2 (Rejected-pin)
is locked** with that wording (§5). The DRAFT tried to make the *issued-predicate* state-decidable by
routing post-SENT rejects away from `REJECTED`; the architect's verification showed that is **not
sufficient**: `(Sent, Rejected)` is a *legal* edge (`fiscal_documents.rs:190`, latent) and `RMR` is
reachable **both** post-SENT (`(Sent,RMR)` `:199`, live W11 PR-2b boot-probe) **and** pre-SENT
(`(ErrorRetryable,RMR)` `:244`). No **state string** decides "issued" for online.

**D3 — LOCKED discriminator = the `server_fiscal_no` column** (not a state-set):

> `online-issued ⟺ offline_fiscal_no IS NULL AND server_fiscal_no IS NOT NULL AND server_fiscal_no != ''`

**Why it is sound (verified, dossier §5.2–5.3):** `set_server_fiscal_no_tx` (`fiscal_documents.rs:1773`)
has **exactly one caller** (`stage_send.rs:1374`), in the same 4-b `with_immediate` tx as the CAS
`Sending→Sent`. Under Option 0 the seed advance lands in that same tx, so **`server_fiscal_no` set ⟺ seed
advanced, atomically** — state-independent and immune to any future terminal routing (the `:190`/`:199`
doors cannot break it). The codebase already treats SENT/ACK ⟹ sfn as an invariant
(`invariant_scan.rs:195`, `boot_phase.rs:2558`, NC-04 `:2270-2290`), so this promotes an existing
guarantee rather than inventing one. Post-SENT reject **routing** still follows `online_convergence` (doc
stays Sent/Kvt2, shift → RMR, no rollback).

**Accompanying, LOCKED:**
- **Remove** the latent `(Sent, Rejected)` edge (`fiscal_documents.rs:190`) from the legal transition
  table + update its table-pin test (`repo_fiscal_documents_state_cas.rs:322`) — policy: a post-SENT
  reject is **never** routed to `Rejected`.
- **D3(ii) verification obligation (done read-only; must re-confirm before LOCK):** no writer of
  `fiscal_documents.server_fiscal_no` other than `set_server_fiscal_no_tx`; **no path minting an online
  doc into SENT+ without sfn** — in particular every online issued-forward edge (incl. the inline
  `Sending→Kvt1` fast-path and `ErrorRetryable→Sent/Kvt1` re-sends, and any boot recovery that concludes
  "DPS accepted" and moves a crashed `Sending` forward) must stamp `server_fiscal_no` atomic with the seed
  advance (same lockstep). The sweep found **no live hole** (dossier §5.3); this is the enforced A2.4
  requirement.

---

## 5. Adjudicated decisions D1–D7 (LOCKED)

| # | Decision | Ruling |
|---|----------|--------|
| **D1** | Issuance moment | **= SENT** (KVT1 reopens the SENT-SENT fork window; SENT = local-commit crossing, symmetric to `OFFLINE_LOCAL_ACK`; drift-assert + fresh-`Applied`-only CAS make the moment well-defined). |
| **D2** | Rejected-pin | **LOCKED wording:** *"pre-SENT reject → `REJECTED`, lnd consumed, seed NOT advanced (pin survives verbatim); post-SENT reject → manual-recon escalation, seed NOT rolled back (pin expands)."* Citers to update **with the A.3 code**: root `CLAUDE.md` M3b persistence paragraph (**NB — `.claude/CLAUDE.md` does not carry it; do not touch**), barrier prose `kill_point_matrix.rs:2516-2539`, roadmap A.1 constraints. |
| **D3** | Discriminator | **= `server_fiscal_no` column** (§4). Not a state-set. Plus latent `(Sent,Rejected)` edge removal + D3(ii) verification obligation. |
| **D4** | Predicate shape | **shared `is_issued(state, offline_fiscal_no, server_fiscal_no)` fn**; **no** `ONLINE_ISSUED_STATES` const (§2). |
| **D5** | NC-03 ordering | **sufficient** — `ORDER BY lnd DESC LIMIT 1`; advances monotonic in `lnd` under single-writer + fail-closed drift-assert. **Named residual (spec obligation):** the ER-parked-predecessor interleave — a worker signing doc2 while doc1 rests pre-SENT (`ErrorRetryable`) gives doc2 a stale `previous_hash`; its SENT drift-assert fails **after** the wire call (ambiguous). Pre-exists under advance-at-ACK (wedges at KVT2); Option 0 surfaces it earlier. Spec must (i) machine-verify interleave reachability (worker doc-selection under ER-park) and (ii) choose a **narrow gate** "do not sign while a pre-SENT doc rests on the FN" (does NOT tax the SENT+ Hold path, unlike Option 2 — architect prior) **or** a fail-closed → manual-recon route. |
| **D6** | Config surface | **= hardcoded DI-swap, NO config knob.** Runtime write-path switch on a fiscal edge-device = Frozen #10 drift hazard; rollback = gated code revert; off-switch adds nothing over stopping the service. Operator controls = separate Phase-D spec. |
| **D7** | Fuzzer tooth | **approved** — pins the model `is_issued` mirror == prod fn (**both** arms, not just the offline const), paired (positive + negative), rides the lockstep consumer commit (landing step 4). |

---

## 6. Landing plan (post-LOCK — for reference, NOT authorized here)

Ordered so the RED-pin flips green only when the fork is actually closed:

0. **Pre-LOCK re-confirm (read-only):** re-run the D3(ii) sweep (sole `server_fiscal_no` writer; no
   SENT+-without-sfn path) and the **D5 interleave reachability** (worker doc-selection order under
   `ErrorRetryable` parking) — both were done once read-only for this dossier; re-confirm at LOCK.
1. Extend `fetch_send_inputs_tx` to carry `unsigned_xml_sha256` + `previous_hash` (mirror
   `fetch_finalize_inputs_tx`).
2. Insert the drift-assert + `update_last_known_xml_sha_tx` in the `stage_send.rs:1373` `WireDecision::Sent`
   block (same 4-b tx), fresh-`Applied`-only, **atomic with the existing `set_server_fiscal_no_tx`**.
3. **sfn-lockstep:** ensure **every** online issued-forward edge stamps `server_fiscal_no` atomic with the
   seed advance — the inline `Sending→Kvt1` fast-path and `ErrorRetryable→Sent/Kvt1` re-sends today stamp
   sfn only via `WireDecision::Sent`; the A2.4 inline lane must not introduce an sfn-less issued edge (D3ii).
4. Generalize the `stage_finalize.rs:288` gate → unified "already-advanced-at-issuance" predicate driven by
   the **shared `is_issued(state, offline_fiscal_no, server_fiscal_no)` fn** (D4).
5. Move all §2 consumers (walk, SSOT SQL literal → `is_issued` fn, fuzzer model + tooth, replay, comments)
   in lockstep. Kill both hardcoded `ACK` literals.
6. **Remove the latent `(Sent, Rejected)` edge** (`fiscal_documents.rs:190`) + update its table-pin test
   (`repo_fiscal_documents_state_cas.rs:322`); implement post-SENT-reject routing per D2/D3
   (`online_convergence` pattern — doc stays Sent/Kvt2, shift → RMR, no rollback).
7. Resolve the **D5 interleave** per the LOCKed choice (narrow pre-SENT-rest gate or fail-closed route).
8. SW-4: split `ChainSeedMismatch` out of the inline `StructuralDrift` arm (`inline.rs:754-780`) →
   `escalate_fn_to_manual_recon` (dossier §5b).
9. Un-ignore `m1_02_online_seed_fork_a24_prerequisite`; confirm green. (Binding flip + inbox-terminalise
   audit = separate A.3 piece, mandatory external review, lands last.)

This sequence is **not** authorized by this document — decisions are LOCKED but implementation awaits
external audit → LOCK. Recorded so the architect can scope A.3.
