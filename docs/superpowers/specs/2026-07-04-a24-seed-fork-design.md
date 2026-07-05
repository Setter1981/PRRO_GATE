# A2.4 online-lane seed-fork — DESIGN (v3 — AUDITED, Go-with-amendments applied)

**Status: v3 — AUDITED (Go-with-amendments applied) — pending architect LOCK; next = LOCK → A.3
implementation (A.3 starts only after LOCK).** Not a code contract; no implementation is authorized by this
document. D1–D7 were adjudicated (v2), then run through an 8-lens external audit (verdict:
**Go-after-amendment** — D1/D2/D3/D5/D6/D7 sound, **D4 re-spec'd to fetch-then-filter** since a SQL literal
cannot host a Rust fn, §4/§5; no new STOP). The audit closures — the MAC-recovery×advance-at-SEND arc, the
D5-gate re-spec (predicate + co-requisite runtime resolver), the C10 Z-report consumer, and all MED/LOW
items — live in the companion dossier **§9**. Evidence + option matrix + anchor provenance + the D3(ii)
sweep: [`2026-07-04-a24-seed-fork-dossier.md`](./2026-07-04-a24-seed-fork-dossier.md) (v3).

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

- **Advance + gate:** `stage_finalize.rs:288/304/315` (move advance to SEND; generalize gate). **Steps
  "move advance" and "generalize gate" are ONE atomic commit** — see §6 interim-ordering pin (dossier §9.5).
- **Audit walk:** `invariant_scan.rs:268` (online arm → SENT+) — in-memory, calls the `is_issued` fn.
- **Boot projection SSOT (C3):** `last_issued_unsigned_xml_sha256` SQL literal `fiscal_documents.rs:933`
  (online arm; fn @ `:916`) — **re-spec'd to fetch-then-filter** (a SQL string cannot host a Rust fn; D4/§4).
  `boot_phase.rs:1810` **stays SSOT-only** — never encodes its own predicate (keeps NC-03 = pure projection).
- **Z-report sets (C10):** `list_shift_issued_receipts` (`fiscal_documents.rs:560/:570`) +
  `list_shift_pending_receipts_for_z_quiescence` (`:602/:598-601`) — the **third** hardcoded literal; count
  CONFIRMED, block quiescence on issued-unconfirmed (architect ruling pending, dossier §9.3). Dormant
  (`FULL_Z_SURFACE_READY=false`) ≠ exempt.
- **MAC-recovery (C14):** `run_mac_recovery` (`mac_recovery.rs:351`) — pre-advance gate gains a
  `mac_recovery_attempts >= 1` recovered-doc branch (skip-equality + reseed-to-own-sha) + M2-X1 rationale
  reword (`:386-398`); dossier §9.1 Variant P.
- **Fuzzer:** RefModel online advance `tests/invariant_fuzzer/model.rs:265-271` (→ SENT); the model gains a
  **per-doc sfn/issued-bit on the Sent-crossing** (today none — D7/§5, dossier §9.6); add the online
  advance-trigger tooth (D3 today guards only the offline set — a silent-drift blind spot).
- **Replay:** `tests/webcheck_replay.rs:757-761` `is_issued` (online arm → SENT+).
- **Comment-pins:** `stage_finalize.rs:280-288`, `backlog_drain.rs:930-937`, `replay.rs:59` (C9).

`last_ack_unsigned_xml_sha256` (`fiscal_documents.rs:873`) and `last_server_fiscal_no` (`:693`, ACK-only,
C13) are **ACK-only by design** — do **not** widen either.

**Shape — LOCKED (D4), re-spec'd by audit:** the single shared predicate fn
**`is_issued(state, offline_fiscal_no, server_fiscal_no)`** in `fiscal_documents.rs` remains the SSOT for the
**in-memory** consumers — offline arm = the existing `OFFLINE_ISSUED_STATES` const, online arm = the D3
`server_fiscal_no` predicate (§4). **No** `ONLINE_ISSUED_STATES` const (one SSOT; bias against namespace
churn). The in-memory literals (C2 `invariant_scan.rs:268`, C10 Z-sets) die into the fn; the walk (C2) +
replay (C8) + the D5 gate + Z-sets (C10) call it; the fuzzer tooth (C7) pins the model mirror. **C3 is the
one consumer the fn cannot reach in-SQL** (`fiscal_documents.rs:933` is a SQL string): it uses
**fetch-then-filter** — return candidates `ORDER BY lnd DESC`, take the first `is_issued()` row in Rust (NC-03
is a rare boot path, the scan is cheap). A SQL-mirror + a SQL≡fn equivalence-tooth (matrix
`state × offline_fiscal_no × server_fiscal_no`) is an acceptable fallback **only** with an explicit
justification of why fetch-then-filter did not fit (dossier §9.4).

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

**MAC-recovery interaction (audit HIGH 1.1 — dossier §9.1, STOP-gate CLEAR).** `run_mac_recovery`
(`mac_recovery.rs:351`) overwrites a `-12`'d doc's `previous_hash := H_dps` (the DPS-supplied hash) and
re-sends in-run. Since a `-12` means `seed_sign ≠ H_dps` **by definition**, Option 0's naive pre-advance
drift-assert `ns.seed == doc.previous_hash` would false-fail on **every** successful recovery, after the wire
call → wedge. **Fix (Variant P):** for `mac_recovery_attempts >= 1` the SEND advance skips the equality gate
(recovery deliberately voided that premise) and **re-anchors** — advances `ns.seed := doc.unsigned_xml_sha256`
(the re-signed sha, the same target as a normal doc) on the fresh `Sent` `Applied` CAS. Safe because the loop
is in-run under the single-writer FN lease (`ns.seed` cannot have moved). The recovered doc's attempt#2 is a
`WireDecision::Sent` → it **stamps `server_fiscal_no` atomic with the advance**, so the D3 discriminator holds
(`sfn set ⟺ seed advanced`). This also **closes** a latent advance-at-ACK hazard (a recovered doc reaching
ACK with `ns.seed ≠ H_dps` false-fails the finalize `:304` guard today). Alternatives R (resync-in-MR-PERSIST
— reseeds pre-issuance) and E (escalate — defeats auto-recovery) are rejected in §9.1.

---

## 5. Adjudicated decisions D1–D7 (LOCKED)

| # | Decision | Ruling |
|---|----------|--------|
| **D1** | Issuance moment | **= SENT** (KVT1 reopens the SENT-SENT fork window; SENT = local-commit crossing, symmetric to `OFFLINE_LOCAL_ACK`; drift-assert + fresh-`Applied`-only CAS make the moment well-defined). |
| **D2** | Rejected-pin | **LOCKED wording:** *"pre-SENT reject → `REJECTED`, lnd consumed, seed NOT advanced (pin survives verbatim); post-SENT reject → manual-recon escalation, seed NOT rolled back (pin expands)."* Citers to update **with the A.3 code**: root `CLAUDE.md` M3b persistence paragraph (**NB — `.claude/CLAUDE.md` does not carry it; do not touch**), barrier prose `kill_point_matrix.rs:2516-2539`, roadmap A.1 constraints. |
| **D3** | Discriminator | **= `server_fiscal_no` column** (§4). Not a state-set. Plus latent `(Sent,Rejected)` edge removal + D3(ii) verification obligation. |
| **D4** | Predicate shape | **shared `is_issued(state, offline_fiscal_no, server_fiscal_no)` fn** for in-memory consumers; **no** `ONLINE_ISSUED_STATES` const (§2). **Audit re-spec:** C3 (`fiscal_documents.rs:933`, a SQL string) cannot host the fn → **fetch-then-filter** (candidates `ORDER BY lnd DESC`, first `is_issued()` row in Rust); SQL-mirror + equivalence-tooth = justified fallback only (dossier §9.4). |
| **D5** | NC-03 ordering + gate | **NC-03 sufficient** — `ORDER BY lnd DESC LIMIT 1`; advances monotonic in `lnd` under single-writer + drift-assert. **Gate re-spec (audit HIGH 1.2, dossier §9.2):** predicate = **`is_issued`-complement** (`∃ doc: non-terminal AND NOT is_issued(...)`), **NOT** a "pre-SENT state-set" (`ERROR_RETRYABLE ∈ OFFLINE_ISSUED_STATES` → a state-set gate stalls the offline lane). It **lands only paired with a runtime resolver** (extend `online_convergence` onto the ER/pre-SENT cohort via `er_redrive_policy`, or in-band resolve) — **a LOCK-condition**, else the gate = FN-wide sign-refusal until reboot. Two-layer enforcement: fail-closed assert **inside the `stage_sign` pin-tx** (boot `dispatch_prepared_via_chain` bypasses acquire) **+** acquire early-refuse. Block-set incl. non-pinned `Prepared` (closes the new lnd-vs-chain-order residual) + an `invariant_scan` "chain order == lnd order over issued" check. |
| **D6** | Config surface | **= hardcoded DI-swap, NO config knob.** Runtime write-path switch on a fiscal edge-device = Frozen #10 drift hazard; rollback = gated code revert; off-switch adds nothing over stopping the service. Operator controls = separate Phase-D spec. **Downgrade correction (audit 3.4):** rollback after the first advance-at-SENT doc is safe **only after the FN quiesces to a terminal** (`ACK`/escalated); a bare binary revert clears in-flight-issued docs onto the old finalize-guard and wedges them. |
| **D7** | Fuzzer tooth | **approved** — pins the model `is_issued` mirror == prod fn (**both** arms, not just the offline const), paired (positive + negative), rides the lockstep consumer commit (landing step 4). |

---

## 6. Landing plan (post-LOCK — for reference, NOT authorized here)

Ordered so the RED-pin flips green only when the fork is actually closed. **⚠ Two atomicity constraints
(audit, dossier §9.5) bind the PR grouping:** steps **2+4 are ONE commit** (interim-ordering pin), and the
**D5 gate (step 7) lands ONLY paired with its runtime resolver** (LOCK-condition).

0. **Pre-LOCK re-confirm (read-only):** re-run the D3(ii) sweep (sole `server_fiscal_no` writer; no
   SENT+-without-sfn path — incl. the §9.6 W9 fact that boot never CASes `Sending→Sent`) and the **D5
   interleave reachability** (worker doc-selection under `ErrorRetryable` parking); a one-time **pre-gate
   fork-pair boot-scan** (duplicate `previous_hash` among non-terminal docs, LOW, §9.2f).
1. Extend `fetch_send_inputs_tx` to carry `unsigned_xml_sha256` + `previous_hash` (mirror
   `fetch_finalize_inputs_tx`).
2. **[atomic with step 4]** Insert the drift-assert + `update_last_known_xml_sha_tx` in the
   `stage_send.rs:1373` `WireDecision::Sent` block (same 4-b tx), fresh-`Applied`-only, **atomic with the
   existing `set_server_fiscal_no_tx`**. **Include the MAC-recovery gate branch (C14, §9.1 Variant P):** for
   `mac_recovery_attempts >= 1`, skip the equality gate and reseed to the doc's own re-signed sha; reword the
   M2-X1 rationale (`mac_recovery.rs:386-398`) to advance-at-SEND.
3. **sfn-lockstep (D3ii):** ensure **every** online issued-forward edge stamps `server_fiscal_no` atomic with
   the seed advance — the inline `Sending→Kvt1` fast-path and `ErrorRetryable→Sent/Kvt1` re-sends today stamp
   sfn only via `WireDecision::Sent`; the A2.4 inline lane must not introduce an sfn-less issued edge.
4. **[atomic with step 2 — INTERIM-ORDERING PIN]** Generalize the `stage_finalize.rs:288` gate → unified
   "already-advanced-at-issuance" predicate on the **shared `is_issued(...)` fn** (D4) AND disable the
   `:315` online advance. *If step 2 lands without this, `stage_finalize` double-advances → the `:304` guard
   false-fails → every online doc wedges at KVT2 (§9.5 3.3).*
5. Move all §2 consumers in lockstep — C2 walk (`invariant_scan.rs:268`) + C6/C8 fuzzer/replay call the
   `is_issued` fn (fuzzer model gains the per-doc sfn bit, D7); **C3 (`fiscal_documents.rs:933`) →
   fetch-then-filter** (SQL cannot host the fn, §4/D4); **C10 Z-sets** per the §9.3 ruling (count CONFIRMED,
   block quiescence on issued-unconfirmed); comment-pins incl. `replay.rs:59`. Kill the in-memory `ACK`
   literals.
6. **Remove the latent `(Sent, Rejected)` edge** (`fiscal_documents.rs:190`) **and, in the SAME commit, the
   five dormant sfn-less edges** (`:186`, `:203`, `:241`, `:242`, `:243` — all zero prod-invokers, §9.5 3.1;
   `(Sending,Kvt1)` `:254` returns with the inline fast-path) + update the table-pin test
   (`repo_fiscal_documents_state_cas.rs:322`); implement post-SENT-reject routing per D2/D3
   (`online_convergence` pattern — doc stays Sent/Kvt2, shift → RMR, no rollback).
7. **[LOCK-condition — gate + resolver together]** Resolve the **D5 gate** (`is_issued`-complement predicate,
   §9.2): fail-closed assert in the `stage_sign` pin-tx + acquire early-refuse; block-set incl. non-pinned
   `Prepared`; **paired with a runtime resolver** for the ER/pre-SENT cohort (extend `online_convergence` via
   `er_redrive_policy`, or in-band resolve) — never ship the gate alone. Add the `invariant_scan`
   "chain order == lnd order over issued" check (§9.2d) + extend check 3a to the SENT/KVT1/KVT2 sfn-backstop
   (§9.5 3.2).
8. SW-4: split `ChainSeedMismatch` out of the inline `StructuralDrift` arm (`inline.rs:754-780`) →
   `escalate_fn_to_manual_recon` (dossier §5b) — unconditional, independent of the gate choice (§9.2e).
9. **Migration boundary-assert** (~5 lines, fail-closed, no permanent trace): stop the A.3 migration if any
   `offline_fiscal_no IS NULL AND server_fiscal_no != '' AND state != 'ACK'` row pre-exists (restored/foreign
   DB, §9.5 3.5). Update the D2 citers via a **grep-sweep** (`seed NOT advanced|lnd consumed|issue.*at ACK`,
   §9.6 4.2), not a fixed list.
10. Un-ignore `m1_02_online_seed_fork_a24_prerequisite`; confirm green. (Binding flip + inbox-terminalise
    audit = separate A.3 piece, mandatory external review, lands last.)

This sequence is **not** authorized by this document — decisions are LOCKED/AUDITED but implementation awaits
the architect's **LOCK** on this amendment. Recorded so the architect can scope A.3.
