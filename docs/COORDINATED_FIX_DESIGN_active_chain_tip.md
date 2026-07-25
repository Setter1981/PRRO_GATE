# Coordinated fix DESIGN — active-chain-tip after `NotAcceptedOffline` rewind

**bd:** PRRO_GATE-2nk (P1). **Fixes:** the proven recovery bug (`NotAcceptedOffline` rewind undone by
NC-03 boot) + the `invariant_scan` cohort-cancel false-positives, coordinated so **boot recovery, the
MacReseed guard-B tip, and `invariant_scan` all agree** on one chain-tip notion.

Boot repro (was RED, now GREEN): `boot_phase.rs::tests::nc03_boot_preserves_not_accepted_offline_rewind`
+ `nc03_boot_recovers_non_doc_t112_rewind_target` (the non-doc T=112 rewind target).

> **Design history (READ THIS):** the first attempt EXCLUDED `chain_superseded_at` docs from
> `last_issued_unsigned_xml_sha256` (old §4). An external audit found the BLOCKER: the rewind target can
> be a **non-document seed** (a T=112 code replenish sets the seed to `sha256(request_xml)` with no
> producing doc), so "the surviving predecessor doc's hash" is the WRONG value — exclusion returns the
> predecessor Hp, not the rewind target Hs. The as-built fix (variant 1, below) reverts `last_issued`
> and reads the superseded doc's `previous_hash` DIRECTLY via a new `active_chain_tip` projection.

---

## 1. Root cause (proven)

`is_issued(RMR-offline) = true` (`OFFLINE_ISSUED_STATES` includes `REQUIRES_MANUAL_RECONCILIATION`).
After a `NotAcceptedOffline` rewind (seed→Hs, held→RMR, cohort→CANCELLED), the held RMR doc is the
highest-lnd issued doc, so a chain-tip projection built on `is_issued` picks the held doc's own hash
**H_held** instead of the durable rewind target **Hs**:
- NC-03 boot reconstruction resurrects H_held (BLOCKER 1);
- MacReseed guard-B expected-tip reads H_held (would mis-adjudicate a reseed);
- `invariant_scan` final-seed walk → ChainSeedMismatch (the original false-positive).

All three must agree on ONE chain-tip notion, so the fix is a single new projection applied to all three.

## 2. The distinction (active-chain vs historical-issued)

- **historical-issued** (`is_issued`, UNCHANGED): "ever crossed OFFLINE_LOCAL_ACK / carries sfn" (M2-01).
  Consumers that must NOT change: Z-quiescence, offline-code backing, legal offline-receipt history.
  The held doc DID locally fiscalize → it stays historical-issued.
- **active-chain tip** (the seed the live MAC chain currently rests at): after a `NotAcceptedOffline`
  rewind the held doc's contribution is VOID (the seed moved BELOW it, to `held.previous_hash`). Only
  the chain-TIP consumers (boot / guard-B / scan) must see the rewound tip, NOT the held doc's hash.

## 3. The marker (why, not RMR-state)

BLOCKER 2: RMR is a GENERAL terminal (`Sent→NotFound`, ER-drain, boot-ER, online `NotAccepted`,
`MacReseed`) — `state == 'RMR'` does NOT prove a rewind. Key on an EXPLICIT witness of a completed
`NotAcceptedOffline`, not the bare state.

**Design:** an additive, nullable marker on `fiscal_documents`, set ONLY by the `NotAcceptedOffline`
rewind on the held doc. All existing rows keep NULL; no backfill; no behavior change for existing data.

```sql
-- migration 039_fiscal_documents_chain_superseded.sql
ALTER TABLE fiscal_documents ADD COLUMN chain_superseded_at TEXT;  -- NULL = active; non-NULL = its
-- chain contribution was rewound away by a NotAcceptedOffline completion (still historical-issued, but
-- NOT the active chain tip). Set once, at the completion; never cleared.
```

## 4. The active-chain-tip projection (as-built — direct `previous_hash`)

`active_chain_tip_unsigned_xml_sha256(fiscal_number)` walks the ledger **newest-first** (`lnd DESC`)
over rows with `unsigned_xml_sha256 IS NOT NULL`:

1. `CANCELLED` / `ABORTED` → skip (dead cohort / never-issued; voided sub-chain, not a live link).
2. first `chain_superseded_at IS NOT NULL` → **return its `previous_hash`** — the exact rewind target
   VERBATIM, whether that is a surviving doc's hash, a **non-doc T=112 seed** (`sha256(request_xml)`,
   no producing doc), or genesis (`NULL` → `None`). This is the key correction over exclusion: the
   target is read directly off the immutable back-pointer, never inferred from another doc.
3. first `is_issued` → its `unsigned_xml_sha256` (a NORMAL issuance ABOVE a marker re-advanced the seed
   to its own hash → it overrides the older marker).
4. else (non-issued crash artifact: `PREPARED`/`SIGNED`/`ENCRYPTED`) → fall through, keep walking.
5. no seed-changing doc → `None` (genesis).

`is_issued` itself is UNCHANGED. `last_issued_unsigned_xml_sha256` is REVERTED to its honest "last
issued doc" semantics and now has **0 production callers** (kept as a historical primitive).

**Why direct-`previous_hash` is correct where exclusion was not:** the rewind writes `seed :=
held.previous_hash` and marks the held doc. `held.previous_hash` is the single durable record of that
target. When the target is a non-doc T=112 seed, NO surviving doc carries it, so any projection that
returns "some other doc's unsigned" (last_issued, with or without exclusion) is wrong; only reading
`held.previous_hash` recovers it.

## 5. Changes (coordinated)

1. **Migration 039** (§3) — additive nullable `chain_superseded_at`.
2. **`complete_operator_pending` `NotAcceptedOffline` arm** (`delivery_reservation.rs`): alongside the
   rewind + `doc_to_rmr` + cohort cancel, `UPDATE fiscal_documents SET chain_superseded_at = <ts> WHERE
   document_id = held AND chain_superseded_at IS NULL`. SAME tx (atomic; idempotent under retry).
3. **NEW `active_chain_tip_unsigned_xml_sha256`** (`fiscal_documents.rs`, §4). `last_issued` reverted.
4. **Three consumers rewired to `active_chain_tip`**: NC-03 boot `reconstruct_lost_node_state`
   (`boot_phase.rs`), MacReseed guard-B expected-tip (`delivery_reservation.rs`), and the
   `invariant_scan` final-seed check (`invariant_scan.rs`).
5. **`invariant_scan` MAC-walk**: two `continue` guards on the pristine walk — skip
   `chain_superseded_at IS NOT NULL` (rewound orphan) and skip `CANCELLED` (dead cohort) — plus the
   final seed check now compares `node_seed` against `active_chain_tip` (shared projection → scan and
   boot cannot diverge). Per-doc `previous_hash != expected` ChainBreak UNCHANGED, so real forks/breaks
   are still caught; `ABORTED` stays in the walk (F4).

## 6. Explicitly out of scope (parallel findings, do NOT conflate)

- **Standalone T=112 replenish** (`PRRO_GATE-hpc`): a replenish that sets a non-doc seed with NO
  subsequent `NotAcceptedOffline` rewind leaves NO ledger trace (nothing carries the marker), so NC-03
  boot cannot reconstruct it. `active_chain_tip` recovers a non-doc seed ONLY when a rewind captured it
  on a superseded doc's `previous_hash`. Needs its own durable seed-transition record.
- **MacReseed** (`PRRO_GATE-mcc`): same CLASS of boot-reconstruction bug (reseed value ≠ any doc hash,
  not ledger-derivable), DIFFERENT mechanism (`chain_superseded` does not help — MacReseed's seed is
  the operator's value, not a surviving predecessor's hash). Separate bd; needs its own durable record.

This fix is `NotAcceptedOffline`-rewind-durability ONLY. It does NOT claim "NC-03 fully solved".

## 7. Test plan (teeth, both directions) — all GREEN

- **Boot (e2e):** `nc03_boot_preserves_not_accepted_offline_rewind` (prefix doc H0, held superseded,
  cohort cancelled → recovers H0); `nc03_boot_recovers_non_doc_t112_rewind_target` (held superseded with
  `previous_hash = Hs`, NO producing doc → recovers Hs).
- **Projection (`active_chain_tip`):** `active_tip_recovers_t112_non_doc_rewind_target`,
  `active_tip_genesis_superseded_recovers_none`, `active_tip_normal_predecessor_doc`,
  `active_tip_repeat_rewind_two_markers`, `active_tip_newer_issued_overrides_marker`,
  `active_tip_skips_signed_crash_artifact_above_tip` (non-issued fall-through).
- **Scan:** `cohort_cancel_marked_state_scans_clean`; `unmarked_rmr_held_doc_is_not_excused` (BLOCKER-2
  canary — marker-keyed, not RMR-state); `live_issued_doc_above_rewound_seed_is_flagged` (fork still
  caught); `nested_rewind_two_superseded_docs_scans_clean`; `corrupt_cancelled_subchain_still_breaks`.
- **Completion:** oc10 (cohort-cancel + rewind), oc15 (genesis rewind), oc22 (tx-rollback) unchanged.
- **is_issued unchanged:** the 32 frozen `invariant_scan` tests + M2-N2b stay green (byte-identical).

## 8. Risk / invariants

- Additive nullable migration; no backfill; existing behavior byte-identical for NULL rows.
- Completion change is IN the existing rewind tx (atomic; no new network/crypto in the tx — inv #1).
- `is_issued` untouched → Z-quiescence / codes / legal history unaffected.
- Fork detection preserved (scan flags a live issued doc above the seed; boot still BLOCKS).
- Single-writer per `fiscal_number` (inv #2) preserved; the marker is set-once, never cleared.

## 9. Reachability of the cross-session/online fork edge (re-review, PASS)

Concern: `active_chain_tip` (lnd DESC) returns the FIRST non-dead issued/superseded row; if an issued
doc with `lnd > held.lnd`, non-superseded, non-cancelled, in a DIFFERENT session (or online) coexisted
with a `NotAcceptedOffline` completion, the DESC walk would return ITS hash, overriding the rewind — and
the session-scoped cohort-cleanup + fork-guard (`offline_session_id = ? AND lnd > ?`) would miss it.

**Unreachable — the STOP_MODE FN-wide issuance fence:**
- The W9b drain-reject that CREATES the held doc sets `node_state.mode = STOP_MODE` atomically
  (`backlog_drain.rs:2378`, `set_mode_stop_mode_tx`).
- `stage_acquire.rs:299-354` rejects ALL issuance (any doc_type / session / online, `NodeStopMode`)
  while the mode is `StopMode`/`Blocked`/`GoingOnline`, BEFORE `allocate_next_lnd`.
- `lnd` is strictly monotonic per FN under a single atomic allocator with a `UNIQUE(fiscal_number,lnd)`
  guard, so a higher-lnd doc is strictly later in time; opening a new offline session / returning online
  requires the prior session's drain to COMPLETE (channel switch forbidden with an open shift), but the
  held doc is un-acked precisely because its drain rejected.

So from the drain-reject until the completion, no later-lnd issued successor (any session/online) can
be minted above the held doc. Teeth-backed: `stage1_node_stop_mode_rejects_with_audit`,
`c612_tier_2_stop_mode_escalation_fires_at_50_consecutive_holds`. *Load-bearing: if a future change
allows a mint above a held offline lnd, revisit (a durable seed record would then be required).*

---

## 10. As-built + review verdict (2026-07-25)

**Decorrelated re-review (8-item static workflow + external prompt): 7/8 PASS; the one FAIL was the
CS-1 inventory-manifest re-mint (CI-greenness, not soundness), resolved by re-minting at the true tip.**

- **A active_chain_tip correctness** — PASS. All six orderings return the correct branch (T=112 non-doc,
  genesis, normal predecessor, nested rewind, newer-issued-override, crash-artifact skip).
- **B lnd ordering** — PASS. Single atomic monotonic allocator + `UNIQUE(fn,lnd)`; DESC faithfully
  tracks chain recency.
- **C marker atomicity/idempotency** — PASS. Same caller-owned tx as cohort-cancel + rewind +
  `doc_to_rmr`; `WHERE ... AND chain_superseded_at IS NULL` double-idempotent; whole-tx rollback on error.
- **D is_issued untouched + consumer completeness** — PASS. `is_issued`/`OFFLINE_ISSUED_STATES`
  byte-identical to origin/main; `last_issued` reverted to 0 prod callers; exactly 3 `active_chain_tip`
  call sites (boot, guard-B, scan).
- **E invariant_scan rewrite** — PASS. CANCELLED sub-chain break still caught; superseded skip hides no
  real ChainBreak; final `node_seed != active_chain_tip` still flags forks.
- **F CS-1 frozen-file / inventory gate** — resolved. The prod edit is `src/db/invariant_scan.rs` (not
  the frozen TEST file); manifests re-minted at the true tip (10 test identities; source shas refreshed).
- **G cross-session/online fork** — PASS (§9): unreachable behind the STOP_MODE issuance fence.
- **H migration + scope honesty** — PASS. Migration additive/nullable/STRICT-safe/forward-only; scope
  honestly bounded (hpc + mcc carved out as unsolved parallel bds).

**As-built scan walk is MINIMAL, not seed-anchored** — two `continue` guards on the pristine walk; the
`node_seed`/`previous_hash` checks are otherwise unchanged, avoiding the rejected scan-only design's
`seed_seen` restructure and its F1/F2 holes.

**Documented follow-ups (NOT in this PR):** `PRRO_GATE-hpc` (standalone T=112 NC-03), `PRRO_GATE-mcc`
(MacReseed NC-03), orphan-suffix internal continuity hardening (optional), fuzzer model mirror (INFO).
