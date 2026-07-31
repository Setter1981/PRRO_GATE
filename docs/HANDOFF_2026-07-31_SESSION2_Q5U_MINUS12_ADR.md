# Handoff — 2026-07-31 (session 2): q5u closed, `-12` promoted, `0ps` decided

Base for everything below: `origin/main` @ **`b1d44ed4`**. **Four local branches, nothing pushed,
no PR opened.** The operator stepped away mid-session; pushing is an outward-facing action and was
left for them.

> **Read this first.** Two "known facts" I inherited turned out to be **wrong**, and both were wrong
> the same way: a design note / docstring that sounded authoritative described machinery that had
> already been retired or fixed. Before restating anything from a ticket body, check the pins.
> - `q5u`'s design note said the inline path leaves the document non-terminal. It does not —
>   `inline::terminalise_inbox` has aborted dangling `{PREPARED,SIGNED}` docs since the ledger-only
>   pin. **BOOT** was the uncovered caller.
> - the fuzzer's `-12` fault-bucket bail said "one auto re-sign + retry". S7-1 R3 retired that
>   orchestrator; there is no second wire.

---

## 1. Branches, in the order they should land

| branch | commits | what |
|---|---|---|
| `fix/q5u-deterministic-sign-defect` | 2 | **P1** — a deterministic tax defect no longer parks a doc in `PREPARED` forever |
| `fuzzer/minus12-into-oracle` | 3 | `-12` promoted out of the assertion-free fault bucket + two corrected docstrings + CS-1 re-anchor |
| `docs/adr-dps-rpc-surface` | 1 | **P1** — ADR deciding the DPS RPC surface + a tooth on it |
| `docs/handoff-2026-07-31-session2` | 1 | this file |

They are independent (each branched from `origin/main`), deliberately NOT stacked — the inventory
gate computes additions-only against the PR base, and a stacked base has bitten us before.

**Ordering caveat:** `fuzzer/minus12-into-oracle` re-anchors `LIVE_DRIFT_BASE_SHA` to `8750a6f6`,
a commit on that branch. If it is squash-merged the SHA stops being reachable from `main`, exactly
as with the previous re-anchors (`fa296155` is likewise not an ancestor of `main`). That is the
established pattern here, not a new problem — but if the provenance leg ever goes RED on `main`,
this is the first thing to look at.

---

## 2. `PRRO_GATE-q5u` (P1) — the deterministic sign defect

`derive_check_tax_summaries` fails in stage 3-NO-TX with `SignError::TaxSummary` when a POS tax rate
is absent from the driver mapping. The boot dispatcher erased the type into `anyhow`, emitted
`BOOT_DISPATCH_ERROR` (Warning) and returned `Ok(())` — so the doc stayed `PREPARED` and every later
boot re-dispatched it identically. Same input, same failure, forever, with no operator-visible
terminal state. The #192 class.

**Fix:** one site in `stage_sign`. On `SignError::TaxSummary` → `PREPARED → Aborted` in its own short
envelope + `SIGN_DETERMINISTIC_DEFECT_ABORTED` audit naming the typed cause; the original error still
reaches the caller unchanged.

**Why in-stage rather than at the dispatcher** (the reasoning changed mid-work, and the honest version
is in the commit): the ticket's claim that inline shares the hole is stale. The stage is still right
because the contract is declared on `SignError` there, one site serves every caller, and the audit
names the cause instead of the generic `INLINE_REFUSED_DOC_ABORTED`.

**Totality checks that a reviewer will ask about:**
- `TaxSummary` is reachable ONLY from `check_payload_from` → SELL / RETURN. SHIFT_OPEN and Z have no
  `CalcTaxError` path (the Z's `<TXS>` arrives pre-aggregated), so the in-stage abort can never
  pre-empt `terminalise_inbox`'s `aborted_shift_class` RMR escalation (spec §16.7 family 1).
- `Aborted` is already `is_terminally_failed`, so the inbox reaper and replay converge on an honest
  terminal instead of 202-forever. That is a side benefit, not a regression.

Three pins (new file, so no frozen-set edit); all three RED under a revert-the-abort canary.

## 3. `-12` into the oracle — a deletion, and it caught nothing

Five early `return ExpectedOutcome::Fault` for `[BadHashPrev, ..]` (service-io, EPZ, SHIFT_OPEN, Z,
SELL/RETURN). `Fault` answers `check_differential` with a bare `Ok(())` — so any claim about `-12`
passed silently.

The model already knew everything: `online_outcome_state` drops BadHashPrev into `_ => Sending`,
`online_origin_advances_seed` excludes `Sending`, and `held_witness` for BadHashPrev is complete.
So the promotion removed five bails and added no prediction code. **Zero production behaviour change.**

**No findings.** 172/172 at `FUZZ_CASES=256` and again at 2048 (738 s). Production was right
everywhere the oracle now looks.

**Teeth, both directions** — this is the actual result:
- taught the model to lie about `-12` (claim the seed advanced under the hold) → 3 tests RED with
  `seed-advance mismatch: real_advanced=false model_advanced=true`;
- restored the fault bail with the SAME lie in place → all 3 GREEN.

The canary's proptest seed was reverted out of `invariant_fuzzer.regressions` — an artificial failure
is not a find, and leaving it there would poison the replay list.

## 4. `PRRO_GATE-0ps` (P1) — the DPS RPC surface, decided

`docs/superpowers/specs/2026-07-31-adr-dps-rpc-surface.md` + `rust/prro/tests/dps_rpc_surface_pin.rs`.

- **`sendChk` (v1): deferred, v2-only.** The ticket's rationale was incomplete. WebCheck does not only
  use v1 when configured to — it downgrades **on the document date** (`SubmitPtrRobot.cs:73-84`), and
  the condition is literally `year < 2022 AND month < 10`, *not* "before 2022-10" (a 2021-11 document
  is not downgraded; a 2019-03 one is). Reads like a WebCheck defect; recorded as observed behaviour.
  Unreachable for us. An `apiver=1` config is **REFUSED, never silently migrated**.
- **`delLastChk` / `delLastChkId`: deferred to `2pz` (P3), probe first.** Our ledger invariants assume
  append-only and we do not know what a DPS-side deletion does to the numbering or the MAC chain.
- **`verAPI` is not on our wire at all** — it appears nowhere in `src/` except two prose comments. In
  WebCheck it is a client parameter that PICKS the RPC. So "we are verAPI=2" IS "our only submit RPC
  is `sendChkV2`", which is what the tooth asserts. Hard-coding a `verAPI` field would be wrong.

Teeth note worth remembering: adding `rpc sendChk` to the proto alone **fails to compile** (the mock
service owes the trait method), which would have made the pin look redundant. The canary therefore
also stubbed the mock so the drift compiles — then 3/3 RED. The pin catches the *decision*, not just
the compiler.

## 5. `PRRO_GATE-6bj` (P1) — AC rewritten, NOT implemented

Recorded as a bd comment. Do not implement the AC as written: `ErRedriveDecision::Redrive` was
DELETED by S7-1 R6 precisely because a re-drive is a second wire for an already-`CALL_STARTED` doc.
`-3` today is a HELD outcome (doc `Sending`, node STOP_MODE — pinned in `shift_life_matrix.rs:1088`),
neither fail-and-stop nor auto-retry.

The proposed replacement is evidence-based reconciliation: ask `lastChk` / `by_server_fiscal_no` what
the peer actually holds, then converge forward, re-send, or stay held — with a test-enforced
invariant that no second `sendChkV2` is ever emitted without a typed witness. Final wording is the
operator's call. Coordinate with NC-02 (the ER budget is doc_type- and wall-clock-blind, while §16
says transport-class for shift open/close is unbounded).

Two `RetryClass` docstrings that described the retired machinery were corrected on the fuzzer branch
— they are what kept this AC looking implementable.

---

## 6. Verification actually run

- `fix/q5u…`: full suite **2346/2346** (`--features test-support`) and **2347/2347** (`--all-features`);
  fmt, `clippy --all-targets -D warnings`, inventory gate PASSED (3 added, 0 removed).
- `fuzzer/minus12…`: fuzzer binary 172/172 at 256 and at 2048; full suite **2343/2343**; provenance
  6/6 after the re-anchor; inventory gate PASSED (0 added, 0 superseded).
- `docs/adr-…`: the 3 new pins GREEN, fmt clean, inventory gate PASSED (3 added, 0 removed), full
  suite **2346/2346**.

Not run anywhere: `--all-features` on the two later branches (only on `fix/q5u…`), and CI itself.
A green local `nextest` is not a green CI here — that has bitten before (#313).

## 7. Next

1. Push the four branches and open PRs (not done: outward-facing, operator away).
2. Close `q5u` and `0ps` in bd after merge; `5hc` is half-done already (the stub carries the live
   `store <hash>` since #356) — re-read its AC and either close it or reduce it to the remaining
   half: generative coverage of the MacRecovery **success** path.
3. The ambiguous-T112 generator leaf is still blocked, and the blocker is confirmed by grep: there is
   **no remote-tip axis** in the model (`remote_tip|dps_tip|peer_tip|server_tip` → 0 hits in
   `model.rs` + `interp.rs`). Adding one is a slice, not a follow-up.
4. Then CS-4 — spec #6 + the thin per-FN coordinator, routing exactly one command through it.

---

# ADDENDUM — the same session continued: the peer-tip axis shipped, and found a P1

Everything in §1-§7 above was written BEFORE this part. What follows supersedes §7's "Next" list:
items 1-3 are now done or superseded.

## A. All six branches are pushed, with PRs open

| PR | branch | base | state |
|---|---|---|---|
| #358 | `fix/q5u-deterministic-sign-defect` | main | CI 4/4 |
| #359 | `fuzzer/minus12-into-oracle` | main | CI 4/4 |
| #360 | `docs/adr-dps-rpc-surface` | main | needed a clippy fix — see §E |
| #361 | `docs/handoff-2026-07-31-session2` | main | this document |
| #362 | `spec/fuzzer-peer-tip-axis` | main | rev2 + rev2.1 |
| #363 | `fuzzer/peer-tip-phase-a` | **#359** | phase A |

**#363 is stacked on #359 deliberately** — both carry a `LIVE_DRIFT_BASE_SHA` re-anchor, and
branching phase A from `main` would red the provenance leg the moment either merged. Merge order:
#358 → #359 → #363, with #360/#361/#362 independent.

## B. The peer-tip axis: spec rev1 → rev2 → phase A

§7 item 3 said the ambiguous-T112 leaf is blocked on a missing remote-tip axis. That axis now
exists in its first phase, and the road to it is worth reading as a method note:

1. **rev1** (my design) was put through a 5-agent verify/attack pass before any code. It did not
   survive: **two FATALs** (the movers table omitted `OperatorComplete` entirely — the only legal
   exit from every HELD state — and omitted that a restart is a wire-SENDING op), a factually wrong
   drain row, a "derived invariant" that was **false as stated**, and an ordering assumption with
   zero evidence.
2. **rev2** accepted a sustained tautology attack: for the plain online path, chain continuity is
   ALREADY proven by `check_doc_against_mutation` + the scanner. The axis is **coverage
   machinery**, not a new correctness oracle — its value is the trajectories it makes reachable.
   Biggest scope win found by the review: keeping the forced `[BadHashPrev]` leaf and adding one
   rule (*peer tip := the declared `store` value*) closes `5hc`'s success path **in phase B, with
   no ambiguity machinery at all**.
3. **Phase A** (PR #363) ships the axis as a read-only observer plus one assertion: *while the run
   has not diverged, every outgoing document's `previous_hash` already equals the peer's tip*.
   Deliberately NOT rev1's "inert" step — an inert step asserts nothing and would have shipped both
   blind spots below.

## C. Phase A paid off three times in one evening

1. **It caught my own omission on its first run.** T=112 rides `ask_offline_codes`, a stub queue
   that does not even increment `send_calls`, so the send-side observer never saw it and the peer
   fell a replenish behind.
2. **`bd PRRO_GATE-knk` (P1)** — a granted T=112 while an UNDRAINED offline backlog rests strands
   that backlog on the pre-T112 chain. Three ops:
   `[OfflineServiceIn, Replenish(Granted), GoOnline([Ack,Ack])]`. Adjudicated by four lenses that
   SPLIT (2 PROD_DEFECT / 1 NEEDS_LIVE_PROBE / 1 HARNESS_ARTIFACT), then settled by checking three
   facts directly: offline docs ARE chained on the wire (`emit_mac`); our own live smoke already
   recorded a drained offline doc REJECTED for a mismatched MAC and polls around it
   (`live_dps_extended_smoke.rs:2603-2612`) — but only for the opposite order; and the reference
   client cannot hit it at all because WebCheck **re-anchors** each document from a live `lastChk`
   at send time (`SendingOfflineChecks.cs:40,47-48`) where we **freeze** `previous_hash` at sign.
   **Open node is severity, not existence** — whether DPS accepts that T=112 at all. Five-step live
   probe is in the bd. **This needs the operator: it is a live run.**
3. **`bd PRRO_GATE-01g` (P2)** — surfaced during the 2048-case run from the PRE-EXISTING `release`
   differential. **Triaged: it reproduces on the parent branch WITHOUT the axis**, identical
   message, so the axis is exonerated. Adjudication prod-vs-model was still in flight at handoff.

Both findings are pinned `#[ignore]`d against the FUTURE contract, not papered over.

## D. Two reusable lessons about the fuzzer corpus

- **Committing a seed to `invariant_fuzzer.regressions` perturbs the whole subsequent search**,
  because corpus entries replay FIRST. That is how `01g` got surfaced. Useful, but it means a new
  failure after adding seeds must be triaged against the parent branch before blaming your own
  change.
- **Never commit a RED seed.** The corpus replays at EVERY scale including the PR gate
  (`FUZZ_CASES=256`), so a red seed reddens everything. Pin it `#[ignore]`d with the sequence
  inline (the NC-02 pattern) and keep the seed in the ticket.

## E. A CI lesson I re-learned the hard way

#360 went red on `fmt + clippy (gnu)` — a required check — for `doc list item overindented`
(`doc_overindented_list_items`, deny-level). Cause: the pin file lives only on that branch, and I
had run fmt + the full suite + the inventory gate there, but **not clippy**. My own pre-push
checklist says fmt+clippy+inventory+nextest **per branch**. A green local `nextest` is not a green
CI, and neither is a green clippy on a different branch.

## F. Revised Next

1. Merge in order: #358 → #359 → #363; #360/#361/#362 independent. Close `q5u`, `0ps` in bd after.
2. **`knk` live probe** (operator-gated: needs the key + `PRRO_LIVE_DPS=1`). Five steps in the bd.
   It decides which of the two defect shapes we have, and therefore which fix.
3. Finish `01g` adjudication, then fix whichever side is wrong. Do NOT weaken the oracle.
4. Phase B of the axis (derived `-12` + `5hc` closes) — only once the movers table has settled on
   green across a few full runs.
5. `5hc` can then be closed; `2ds` (ambiguous T=112) is phase D.
6. CS-4 remains the roadmap item after that.

