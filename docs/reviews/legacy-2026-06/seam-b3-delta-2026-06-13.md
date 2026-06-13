# SEAM-B-3 — implementation delta (Opus → Fable review)

**Branch:** `fix/seam-b3-drain-superseded` · **DO-NOT-MERGE** (architect reviews migration-grade)
**Contract:** `docs/reviews/legacy-2026-06/seam-pass-2026-06-12.md` §"Locked contract — SEAM-B-3 fix"
**Date:** 2026-06-13

---

## 1. What the fix does

Gives the offline-drain **SentReplay** `lastChk` classifier the M1-02 superseded
exception that boot's `dispatch_sent_via_probe` already has. A `ServerFiscalIdMismatch`
on a SENT doc that is **not** the FN's newest submitted doc (`doc.lnd < max_submitted_lnd`)
now routes to a non-fatal hold instead of hard-aborting the whole FN drain.

**Before:** the `ServerFiscalIdMismatch` arm routed UNCONDITIONALLY to
`StructuralDrift{LastChkIdMismatch}` → `BootError::Internal` → the entire FN drain halted.
**After:** when superseded, the doc is held in SENT (no state change), a `TIP_SUPERSEDED`
audit is emitted, the recovery trace is completed, and the drain CONTINUES to the next doc.
Mismatch on the actual newest doc is UNCHANGED (still `StructuralDrift`).

---

## 2. Contract conformance (item by item)

1. **superseded definition** — `doc.lnd < fiscal_documents::max_submitted_lnd(fn)` over
   `{SENT,KVT1,KVT2,ACK}`. Reused the existing M1-02 repo fn; **no new query**. ✅
2. **classifier stays PURE** — added `superseded: bool` param to `classify_check_result`.
   The caller (`confirm_drain_doc`) reads `max_submitted_lnd` + `doc.lnd` BEFORE the call
   and passes the flag. Only the `ServerFiscalIdMismatch` arm reads it. ✅
3. **superseded == true** → new `Kvt2ConfirmOutcome::SupersededHold { dps_tip_id, .. }`
   → `confirm_drain_doc` commits a 1c-superseded envelope (trace `complete_tx`
   `RetryableServer` / `LAST_CHK_TIP_SUPERSEDED` carrying the DPS tip id + `TIP_SUPERSEDED`
   audit `Severity::Warning`), **no doc-state CAS, no `consecutive_holds` increment**, and
   returns `ConfirmDrainOutcome::SupersededHeld`. Mirrors boot's
   `complete_probe_trace_tip_superseded` (same event/label, RetryableServer, no CAS). ✅
4. **superseded == false** → `StructuralDrift{LastChkIdMismatch}` (UNCHANGED). ✅
5. **invariants** — no new doc-state transition; no wire/crypto added; pure classifier
   stays pure; minimal diff. ✅

### Pins
- **Superseded drain does not halt** — new test
  `seam_b3_sent_replay_superseded_holds_doc_and_continues_drain`: 2-doc SENT cohort
  (lnd=1 superseded, lnd=2 tip); asserts drain returns `Ok`, doc_a held in SENT, doc_b
  → ACK, `TIP_SUPERSEDED` audit ×1, no `KVT2_CONFIRM_STRUCTURAL_DRIFT`, summary
  `held_at_sent == 1`, `last_chk_count == 2`, and `invariant_scan::assert_clean`. ✅
- **Mismatch on actual tip still drifts** — existing
  `c5b2_sent_replay_lastchk_mismatch_halts_drain_with_structural_drift_audit` stays green
  (single SENT doc = tip = not superseded → StructuralDrift → BootError). ✅
- **classifier unit tests stay green** — 16 call sites updated with `superseded=false`
  (behaviour unchanged for the non-superseded cases). ✅

---

## 3. Decisions the contract delegated (please review)

The contract specified the classifier mechanics precisely but said only "the drain maps it
to … CONTINUES" for the consumption side. Three calls were mine:

### A) New continue-primitive — **wedge avoidance** (the load-bearing decision)
`ConfirmDrainOutcome` had only `Advanced` (continue) and `HoldFnDrain` (which `break`s the
per-doc loop — STOPS the FN drain). Mapping superseded → `HoldFnDrain` would **wedge** the
drain: the superseded doc has the **lower lnd** and is processed **first** in the `lnd ASC`
cohort, so a `break` there would strand the higher-lnd tip, which never drains. I therefore
added a dedicated sibling-continue path:
- `ConfirmDrainOutcome::SupersededHeld` (new variant)
- `DocVerdict::SupersededHeld` (new variant)
- drain loop: records `held_at_sent` + `continue` (NOT `break`), **no tier (REC-1) check**.

The new test pins no-wedge by asserting the tip reaches ACK and `last_chk_count == 2`.

### B) Finalize semantics — **conservative `held_at_sent`** (flagging for your call)
The superseded doc is recorded via `record_doc_held_at_sent`, so `finalize_eligibility`
returns `NotEligible{DocsHeldAtSent}` → `OFFLINE_DRAIN_PARTIAL`; the node stays
`GoingOnline` and the session stays `Draining` until B1-v2 doc-scoped confirmation resolves
the doc. Rationale: conservative (does not finalize a session while a doc is locally
unconfirmable via `lastChk`), and minimal-diff (reuses the existing held-at-sent gate). The
forensic distinction lives in the `TIP_SUPERSEDED` audit + the `transport_trace` row, not a
new summary bucket.
**Architect ruling (Fable, 2026-06-13 — PASS):** the conservative `held_at_sent` soft-block
is ACCEPTED. The alternative ("a superseded SENT doc IS DPS-acked / fiscalised, so don't
block finalize") is **REJECTED**: `lastChk`-superseded does NOT prove the older doc was
ACKed — it only reports that a newer submitted doc is now the FN tip; the older doc may be
acked-then-superseded OR never acked. Concluding "acked" and finalizing an unconfirmed
receipt would be unsafe. Therefore the doc is HELD (not concluded), exactly as boot M1-02
holds. See the [Known residual](#known-residual-architect-accepted-2026-06-13) below.

### C) Defensive fail-loud on non-SentReplay
`superseded` is computed `true` only on the SentReplay path, so `SupersededHold` /
`SupersededHeld` are structurally unreachable for SentFresh / Kvt1Reentry. I added fail-loud
arms at every consumer so a routing regression cannot silently mis-route:
`confirm_drain_doc` (BootError::Internal), `process_via_stage_send` (SentFresh,
BootError::Internal), `process_via_w12_only` (Kvt1Reentry, BootError::Internal),
`online_convergence::converge_one_doc` (anyhow::bail!), `write_path::inline` (unreachable!).

---

## 4. Files changed

| File | Change |
|------|--------|
| `src/services/offline_sync/kvt2_confirm.rs` | `superseded` param on `classify_check_result` + `evaluate_lastchk`; `Kvt2ConfirmOutcome::SupersededHold`; `confirm_drain_doc` superseded compute (reuses `max_submitted_lnd`) + arm; `ConfirmDrainOutcome::SupersededHeld`; new `commit_sent_replay_envelope_1c_superseded` helper; 16 unit-test calls +`false` |
| `src/services/offline_sync/backlog_drain.rs` | `DocVerdict::SupersededHeld`; `process_via_lastchk_replay` maps it; drain-loop sibling-continue arm; 2 defensive arms (SentFresh / Kvt1Reentry); 1 exhaustiveness arm in a unit test |
| `src/services/reconciliation/online_convergence.rs` | defensive `SupersededHeld` arm (bail!) |
| `src/services/write_path/inline.rs` | `superseded=false` + defensive `SupersededHold` arm |
| `tests/backlog_drain_state_dispatch.rs` | new test `seam_b3_sent_replay_superseded_holds_doc_and_continues_drain` |

---

## 5. Gate

Final gate (post review-fold, architect's exact invocation):
- `cargo fmt -p prro -- --check` → clean. *(Correction: the initial-commit gate reported
  "fmt clean" via a buggy `fmt_exit=$?`-after-a-pipe that captured `tail`'s exit, not
  `cargo fmt`'s. The architect's exact re-gate caught that the SEAM-B-3 test +
  exhaustiveness arm were not rustfmt-clean; applied `cargo fmt` — token-preserving line
  wrapping only, no logic change — and re-gated clean.)*
- `cargo clippy -p prro --all-targets --features test-support -- -D warnings` → zero warnings.
- `cargo nextest run -p prro --features test-support` → 1400 passed, 5 skipped (this run the
  `reconcile_guard_enforcement_compile_fail` trybuild-under-nextest-parallelism flake came up
  green; it is known-flaky and passes isolated — unrelated to this change).

---

## 6. Invariant check (frozen invariants)

- **INV-1 (no network/crypto in long write tx):** `max_submitted_lnd` is a short read
  OUTSIDE `with_immediate`; the DPS `lastChk` call is unchanged (outside `with_immediate`);
  the 1c-superseded envelope is pool-only writes (trace complete + audit). ✅
- **INV-2 (single-writer per fiscal_number):** unchanged — drain owns the FN lease. ✅
- **INV-8 (recovery preserves state-machine correctness):** the superseded path makes
  **no** state transition (doc stays SENT) — strictly fewer transitions than the old halt;
  the tip's `Sent→Kvt1→Kvt2→Ack` path is untouched. ✅
- **Idempotency:** re-running the drain re-probes the superseded doc → superseded again →
  held again, no state change. ✅

---

## 7. Known residual (architect-accepted, 2026-06-13)

Until **B1-v2** doc-scoped confirmation lands, a superseded SENT doc remains `held_at_sent`,
so its **shift stays in pending-drain** (offline session `Draining` / shift
`OpenedLocalPendingDrain`). This is a **soft-block**, identical to boot M1-02's position
(hold, don't terminalise), and **strictly better than the prior hard-abort** (which raised
`BootError` and halted the *entire* FN drain). It does NOT lose, mis-state, or conclude the
doc: its ACK status stays UNKNOWN and recorded as such (`TIP_SUPERSEDED` audit + recovery
trace). Resolution of the superseded doc is owned by **B1-v2** doc-scoped confirmation /
monitoring — out of scope here. Accepted by the architect as part of the PASS verdict.

### Review fold (Fable PASS + mandatory fix, 2026-06-13)

- **Mandatory comment fix applied.** The over-claim "the doc was DPS-acked but superseded,
  NOT lost" is removed from all explanatory comments (the three named locations in
  `kvt2_confirm.rs` plus the analogous comments in `confirm_drain_doc`, the
  `commit_sent_replay_envelope_1c_superseded` doc-comment, and `backlog_drain.rs` /
  the test). All now state: **ACK status is UNKNOWN from `lastChk`; HOLD, do not conclude;
  resolution deferred to B1-v2** (boot M1-02 parity). No behaviour changed.
- **Runtime audit strings kept boot-verbatim (flagged).** The emitted `TIP_SUPERSEDED`
  `rationale` / `error_message` payload strings in
  `commit_sent_replay_envelope_1c_superseded` still carry boot M1-02's exact wording
  (incl. "DPS-acked") — kept verbatim for **forensic parity** across the boot + drain
  `TIP_SUPERSEDED` surfaces, and because the mandatory fix was scoped to comments /
  no-behaviour-change. The helper doc-comment now flags this and directs maintainers to the
  doc-comments (not the legacy payload wording) for authoritative semantics. If you want the
  payload strings reworded too (and boot's alongside, to keep parity), that's a trivial
  follow-up.

## 8. Not done

- The drain `StructuralDrift` re-audit (seam-pass item 3) remains a deferred follow-up,
  untouched.
