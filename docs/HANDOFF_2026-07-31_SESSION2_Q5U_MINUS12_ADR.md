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
