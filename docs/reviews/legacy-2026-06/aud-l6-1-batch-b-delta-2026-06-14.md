# AUD-L6-1 + Batch B — implementation delta (Opus → Fable review)

**Branch:** `fix/aud-l6-1-bootseed-and-tipguard-notfound` · **DO-NOT-MERGE** (architect reviews migration-grade)
**Spec:** `REMEDIATION-PLAN-2026-06-13.md` §A1(boot-half = A1.2) + §B · details in `m1-m2-adversarial-pass-2026-06-13.md` (AUD-L6-1 FT, AUD-L4-1 HIGH, AUD-L4-2 LOW)
**Date:** 2026-06-14 · base: origin/main `0e109c8` (post-#162; reused its M2-N2b issued-set)

> Scope note (per the prompt): this PR does **AUD-L6-1 (A1 boot-half)** + **Batch B** only.
> A2/A3 (already merged as M2-N1/N4 in #162) and Batch C are out of scope. **Merging this PR
> alone does NOT lift the NO-GO** — it closes the FT seed-corruption + the HIGH tip-guard wedge.

---

## Part 1 — AUD-L6-1 (FT): boot seed projection from the EVER-ISSUED tip

**Root:** NC-03 boot branch (a) (`boot_phase.rs`) projected `node_state.last_known_unsigned_xml_sha256`
from `last_ack_unsigned_xml_sha256` (state=`ACK` ONLY). M2-01 advances the seed at
`OFFLINE_LOCAL_ACK` for offline-origin docs (and `stage_finalize` SKIPS the advance for them), so
when an FN's tail is an offline-origin doc with `lnd` > the last online ACK, boot wrote the STALE
last-ACK seed (h1) instead of the true tip (h3) → forked legal MAC chain at the next write. Hits
TODAY on a restored/recovered DB.

**Fix:**
- New `fiscal_documents::last_issued_unsigned_xml_sha256(pool, fn)` — `unsigned_xml_sha256` of the
  highest-`lnd` EVER-ISSUED doc: online `ACK` OR offline-origin (`offline_fiscal_no IS NOT NULL`)
  in any `OFFLINE_ISSUED_STATES` state. `ORDER BY lnd DESC LIMIT 1`.
- **Single source of truth:** new `pub const fiscal_documents::OFFLINE_ISSUED_STATES: [&str;7]`
  (`OFFLINE_LOCAL_ACK,SENT,KVT1,ERROR_RETRYABLE,KVT2,REJECTED,REQUIRES_MANUAL_RECONCILIATION`).
  The `invariant_scan` MAC-walk issued-set (M2-N2b, #162) now uses `OFFLINE_ISSUED_STATES.contains(..)`
  (was an inline `matches!`), and `last_issued_…` builds its SQL `IN`-list from the SAME const — so
  the boot projection **equals** the walk's final `expected` by construction (same predicate + max-lnd).
- boot branch (a) swaps `last_ack_…` → `last_issued_…`. `last_ack_unsigned_xml_sha256` is KEPT
  (no ACK-only caller today; retained per the architect, docstring corrected — the old "projecting
  from last ACK is unambiguous" is false post-M2-01). Boot comment (`boot_phase.rs:1592`) corrected.

**Pins:**
- `app_boot_reconciliation::aud_l6_1_boot_projects_seed_from_offline_origin_tip_not_last_ack`
  (RED-first verified — current code projected h1, expected h3): ACK lnd1 (h1) + 2 offline
  OFFLINE_LOCAL_ACK (lnd2 prev-h1 h2, lnd3 prev-h2 h3), node_state deleted → boot projects **h3**,
  `next_lnd=4`, `invariant_scan::assert_clean`.
- `invariant_scan::m2_n2b_rejected_offline_origin_predecessor_scans_clean` (from #162) stays green —
  the const refactor preserves the walk's REJECTED/MANUAL issued semantics (pin b).
- All `backup_restore` tip-guard + restore tests + `app_boot_reconciliation` stay green (pin c —
  ACK-tip case unchanged).

## Part 2 — Batch B (AUD-L4-1 HIGH + AUD-L4-2 LOW): tip-guard NotFound must not false-BLOCK

**Root:** the tip-guard NotFound arm (`boot_phase.rs`) flips `mode→BLOCKED` on a `lastChk` NotFound
of `expected` = `last_submitted_server_fiscal_no` (max-`lnd` over `{SENT,KVT1,KVT2,ACK}` — includes a
non-ACK **in-flight offline-origin SENT** doc). But the drain treats that exact SENT+NotFound as the
HIGH-C5-3 SAFE re-send (Sent→ER→Pattern-B). The tip-guard runs FIRST → BLOCKED → the drain's mode-gate
then skips the FN → permanent wedge + ingress refused. Hits TODAY on a restored/crashed DB.

**Fix:**
- New `fiscal_documents::newest_submitted_state(pool, fn)` — the `state` of the SAME max-`lnd`
  submitted tip doc as `last_submitted_server_fiscal_no` (same cohort + order).
- NotFound arm: if the tip's state is `SENT` (non-ACK in-flight) → **DEFER** (no BLOCK; INFO
  `TIP_GUARD_DEFERRED_INFLIGHT` audit; new outcome `TipGuardOutcome::DeferredInFlightSent`) so the
  drain's safe-redrive owns it. Else (genuine ACK/KVT-tail divergence) → BLOCK as before.
- **Mismatch arm re-examined (per the spec):** STAYS a BLOCK. `expected` is the FN's newest
  submitted doc → there is no newer submitted doc of ours → a different DPS tip is genuine
  divergence, which the drain itself treats as fatal `StructuralDrift` (not the safe-redrive). So
  BLOCK is correct + consistent; only the NotFound arm changes.
- **AUD-L4-2:** corrected the now-false NotFound comment + the `block_on_stale_tip` audit rationale
  ("last ACK / confirmed ACK" → "newest SUBMITTED tip, which may be a non-ACK in-flight SENT").

**Pins (extend `backup_restore.rs`):**
- `tip_guard_inflight_sent_notfound_defers_to_drain`: GoingOnline FN + active offline session +
  in-flight offline-origin SENT tip + `lastChk` NotFound → `DeferredInFlightSent`, node stays ONLINE
  (NOT BLOCKED), `TIP_GUARD_DEFERRED_INFLIGHT` audit, NO `TIP_GUARD_STALE_LEDGER`.
- `tip_guard_ack_tail_notfound_still_blocks`: ACK tail (no in-flight SENT) + NotFound → still
  `Blocked` + CRITICAL (preserves the real stale-ledger guard — proves the fix did not over-defer).
- (Note: the defer pin references the NEW `DeferredInFlightSent` variant, so a literal pre-fix RED
  is a compile error; the behavioural RED is "current code BLOCKs the SENT tip". The ACK-tail pin is
  green both pre- and post-fix and locks the no-over-defer boundary.)

---

## Decisions / faithfulness

- **No design divergence to escalate.** Defer is SENT-only (the documented HIGH-C5-3 safe-redrive,
  per the spec's "in-flight SENT"); KVT1/KVT2/ACK tails on NotFound BLOCK (genuine divergence). The
  Mismatch arm stays BLOCK (consistent with the drain). The shared `OFFLINE_ISSUED_STATES` const
  satisfies the spec's "single source of truth so scan-walk and projection don't diverge".

## Files changed

| File | Change |
|------|--------|
| `src/db/repositories/fiscal_documents.rs` | new `OFFLINE_ISSUED_STATES` const + `last_issued_unsigned_xml_sha256` (AUD-L6-1) + `newest_submitted_state` (Batch B); corrected `last_ack_…` docstring |
| `src/db/invariant_scan.rs` | MAC-walk issued-set uses the shared `OFFLINE_ISSUED_STATES` const |
| `src/services/reconciliation/boot_phase.rs` | branch (a) projects from `last_issued_…` + comment; tip-guard NotFound defers on a SENT tip + new `DeferredInFlightSent` outcome + Mismatch re-exam comment + L4-2 rationale fix |
| `tests/app_boot_reconciliation.rs` | AUD-L6-1 boot-projection pin |
| `tests/backup_restore.rs` | Batch B defer + ACK-tail-still-blocks pins + SENT-tip seed helper |

## Gate

- `cargo fmt -p prro -- --check` → clean.
- `cargo clippy -p prro --all-targets --features test-support -- -D warnings` → zero warnings.
- Targeted `cargo nextest` (architect's instruction — not the full suite for a small add): the
  affected binaries (`app_boot_reconciliation`, `backup_restore`, `invariant_scan`,
  `write_path_deterministic_replay`, `kill_point_matrix`, `backlog_drain_*`) → all pass. (Full suite
  was green on #162's base: 1405 passed.)

## Invariant check

- **INV-1 (no net/crypto in long write tx):** `last_issued_…` / `newest_submitted_state` are short
  pool reads OUTSIDE `with_immediate`; the tip-guard probe is unchanged (outside tx); the defer adds
  one pool-bound audit append (no wire).
- **INV-2 (single-writer):** boot runs per-FN under the reconcile guard; the two new reads see a
  consistent snapshot (no concurrent writer during boot).
- **INV-8 (recovery preserves state-machine correctness):** AUD-L6-1 makes boot project the CORRECT
  seed (fewer corrupt-chain outcomes); Batch B removes a false BLOCK (the FN can recover via the
  drain instead of wedging) while preserving the real stale-ledger BLOCK.

## Not done (separate follow-ups — do NOT gate this PR)
- **AUD-L2-1 + AUD-L5-1** → Batch C (A2.4 online-lane seed-fork + KVT1 superseded tolerance).
- LOWs (AUD-L1-2, L3-1, L8-2) → Batch D doc-cleanup.
