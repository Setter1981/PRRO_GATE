# M1+M2 multi-lens adversarial pass — architect adjudication (2026-06-13)

**Method:** 8 parallel lens-finders (L1-L8) over current main `fb9f09a` + adversarial
per-candidate verifier (refute / known / legally-locked). 23 agents, ~2.5M tokens. Each REAL
candidate re-verified against `/mnt/d/Prro_gate`; the FT cross-checked personally by the architect.
**Result:** 15 raw → **9 real (1 FT · 3 HIGH · 1 MED · 4 LOW)**, 4 known-duplicate, 2 refuted.
**Verdict: NO-GO reinforced** (independently of the already-open M2-N1 NO-GO).

The pass again confirmed the recurring class: **a later invariant change silently invalidated an
earlier merged fix.** Here the standout (AUD-L6-1, FT) is a regression in the architect's OWN
NC-03 (#148) caused by M2-01 (#150) landing the next day.

## NEW CANDIDATES

`AUD-L6-1 | FT | L6 | boot_phase.rs:1599-1608 + fiscal_documents.rs:801-815`
NC-03 boot seed reconstruction projects `node_state.last_known_unsigned_xml_sha256` from
`last_ack_unsigned_xml_sha256` (state='ACK' ORDER BY lnd DESC). M2-01 (PR#150, landed the day
AFTER NC-03 fc3bcdd; commit-order proven 4c23a7c ancestor-of-check) made the seed advance at
**OFFLINE_LOCAL_ACK** for offline-origin docs and made `stage_finalize` SKIP the advance for them
(offline_fiscal_no.is_some()). So when an FN's true tip is an offline-origin doc (lnd > last online
ACK), boot reconstructs a STALE seed (last-ACK h1, not the offline tip h3), persists it, and the
CRITICAL audit falsely reports `mac_seed_projected:true`. Post-unblock a new online doc signs
`previous_hash`=stale seed → forked legal MAC chain on the wire; the online finalize guard passes
(both stale) and cements the fork. **Repro:** ACK lnd1(h1) + OFFLINE_LOCAL_ACK lnd2(prev h1,h2) +
lnd3(prev h2,h3), delete node_state, `run_boot_reconciliation` → projected_seed=h1 (wrong; true h3);
`invariant_scan::scan` → `ChainSeedMismatch{walk=h3,node=h1}`. **Reach:** wrong seed persisted
**TODAY** at boot (branch (a) is a live recovery surface); wire-fork at A2.4. **Breaks:** NC-03
(#148) seed-projection correctness × M2-01 — novel NC-03×M2-01 interaction; NOT M2-N2b (that is the
scan's REJECTED-exclusion). **Fix:** symmetric companion to M2-N2b — project from the highest-lnd
*ever-issued* doc (ACK OR offline-origin reaching OFFLINE_LOCAL_ACK), not last pure ACK. Verified
personally (both halves seen earlier this session: the ACK-only query + M2-01's offline-ack advance).

`AUD-L1-1 | HIGH | L1 | (same root as AUD-L6-1)` — the state-machine view of the same NC-03×M2-01
seed-projection staleness. Merge into AUD-L6-1's fix; not a separate item.

`AUD-L4-1 | HIGH | L4 | boot_phase.rs:2154-2163 vs kvt2_confirm.rs:321-335`
Boot tip-guard and W9 drain interpret a DPS `last_chk` **NotFound** on the FN's newest *submitted*
SENT tip in CONTRADICTORY ways, and the tip-guard runs first and wins. Post-ruling-3 the tip-guard's
`expected` = `last_submitted_server_fiscal_no` (max-lnd over {SENT,KVT1,KVT2,ACK} — INCLUDES an
in-flight offline-origin SENT doc), and NotFound → `block_on_stale_tip` → mode=BLOCKED. But the
drain treats the identical SENT+NotFound as the documented HIGH-C5-3 **safe re-send** (Sent→ER→
Pattern-B). Because BLOCKED is set at startup before the drain loop spawns, the drain's mode-gate
then permanently skips the FN → the entire offline backlog wedges + new ingress refused, until
manual unblock. The tip-guard NotFound-arm comment still claims "we hold a confirmed ACK" — false
since ruling-3 widened `expected` to non-ACK submitted docs. **Reach:** TODAY on a restored/crashed
DB (the drain loop runs per-FN over the registry independent of ingress). **Breaks:** ruling-3's
"in-flight expected-tip set removes the drain-lag false positive" — holds for Match, NOT NotFound.
**Fix:** the tip-guard must not BLOCK on NotFound of a non-ACK in-flight SENT tip — defer to the
drain's safe-redrive (or gate the tip-guard NotFound-BLOCK to genuine ACK-tail divergence only).

`AUD-L2-1 | HIGH (finder FT→HIGH) | L2 | inline.rs:748 + stage_finalize.rs:288-322 + invariant_scan.rs:205-213`
ONLINE-lane MAC-chain fork — the offline analogue of M2-01, on the online path which M2-01 did NOT
fix. An online receipt resting at SENT via `online_confirm` Hold (transient lastChk OR empty
data_sign) does NOT advance the seed; a second online receipt then signs the SAME unadvanced seed →
forked chain (two genesis-`previous_hash` SENT docs — pinned reachable by
`kill_point_matrix.rs:1107 m1_02_reachability_...`, scan reports CLEAN). On convergence doc#1 ACKs
(seed advances), doc#2 hits `stage_finalize` ChainSeedMismatch → wedges at KVT2. The online
convergence tick (filters Sent|Kvt1, log-and-skips Infrastructure errors) and the boot KVT2 arm
(Warning-only) do NOT escalate ChainSeedMismatch to manual-recon — only the DRAIN does (M2-04 #151).
So the wedged receipt has no operator surface. **Reach:** A2.4 only (inline dormant; production =
UnimplementedWritePath). **Breaks:** M2-01 completeness (offline-scoped) + M2-04 (manual-escalation
only in drain). **Fix:** A2.4 pre-flight — online lane must advance seed per-issued-doc OR gate a new
acquire/sign on prior-doc terminality; convergence + boot KVT2 must escalate ChainSeedMismatch to
manual-recon (mirror the drain). Pair with M2-X2.

`AUD-L5-1 | MED | L5 | kvt2_confirm.rs (superseded SentReplay-only) + online_convergence`
A HEALTHY resting **KVT1** doc that is no longer the DPS last_chk tip (a newer doc on the FN became
the tip) is falsely classified fatal `StructuralDrift::LastChkIdMismatch` — the superseded
exception (SEAM-B-3) is computed ONLY for SentReplay/SENT, not for the KVT1-reentry path. **Reach:**
TODAY (online_convergence is a live loop; reachable via restored DB with a KVT1 doc + newer tip).
**Fix:** extend the superseded tolerance to the KVT1-reentry classifier (companion to M2-N4's
tip-id-match hardening).

### LOW (real)
- `AUD-L1-2 | LOW` — two whitelisted DocState edges with no production invoker
  ((OfflineLocalAck,Cancelled) etc.); dead-but-declared, doc/cleanup only.
- `AUD-L3-1 | LOW` — Tier-2 STOP_MODE escalation doc-comment claims auto-recovery "через W8
  return_online_probe" that the probe does not actually perform; stale comment.
- `AUD-L4-2 | LOW` — tip-guard NotFound/Mismatch block-rationale comment still says "last ACK /
  confirmed ACK" after ruling-3 widened to submitted tip (the comment half of AUD-L4-1).
- `AUD-L8-2 | LOW` — invariant_scan check-5 (RejectedInboxWithAcceptedDoc) guards only
  `i.status='REJECTED'` while replay short-circuits both REJECTED+ERROR; minor scan-vs-replay skew.

## HYPOTHESES
- **H-AUD-1 (from H-M2-1, still open):** DPS `ERROR_BAD_HASH_PREV` cascade behavior on a successor
  of a rejected predecessor — needs a stubbed/live DPS contract test. Informs blast radius of
  AUD-L6-1 / AUD-L2-1 / M2-N1, does not change the fixes.

## DEFENCES VERIFIED (44 attacks held; notable)
- Boot dispatch is genuinely exhaustive (explicit terminal-states `bail!`, no `_` catch-all;
  `boot_phase.rs:3460-3662` + cohort WHERE excludes them).
- `stage_send` never mutates `fs_mode`/`offline_session_id` → drained offline doc stays in the
  session-scoped cohort across ticks.
- lnd allocation + PREPARED insert share ONE `with_immediate` → no skipped-lnd window on crash.
- `stage_offline_ack` runs {code-consume, CAS, seed-advance, audit} in ONE `with_immediate` → no
  code-without-doc / seed-without-doc crash window.
- SENT doc cannot re-enter `stage_send` (4-pre CAS allowlist excludes SENT) → no double-send; drain
  SENT branch is read-only lastChk.
- `finalize_eligibility` is strict (Eligible only when zero failures/holds AND advanced==backlog) →
  no partial-drain session close.
- Replay treats OFFLINE_LOCAL_ACK as accepted with fiscal_id=None (no fabricated id).
- SentReplay orphan trace (is_probe=1) does not burn the ER budget; boot orphan-scanner closes it.

## DISAGREEMENTS
- AUD-L2-1: finder said FT; I concur with the verifier's **HIGH** — the fork becomes
  forensically VISIBLE (ChainBreak) at convergence, FT only if an operator force-finalizes past the
  scan; and it is A2.4-gated. Logged HIGH.

## CLEARED / REFUTED
- `AUD-L5-2 — REFUTED`: "FN keeps trading against a foreign DPS chain until reboot" — the
  online-convergence asymmetry exists but does NOT let the FN keep trading (boot tip-guard + the
  resting-doc semantics hold); no new trading-while-diverged vector. `holds`.
- `AUD-L8-1 — REFUTED`: "scan has no drain-cohort↔STATE correlation check" — the static claim is
  true but it is not a defect (no legally-bad ledger it lets pass that another check doesn't). `holds`.
- `AUD-L6-2, AUD-L8-3 — known-duplicate of M2-N2b` (issued-set widen; CANCELLED/REJECTED label
  variants — fold into the M2-N2b fix). `holds as M2-N2b`.
- `AUD-L7-1, AUD-L7-2 — known-duplicate of RT-1 / B1 §4 ER-redrive` (already deferred). `holds`.

## TOP 3
1. **AUD-L6-1 (FT, NC-03×M2-01):** boot reconstructs a STALE MAC seed (last-ACK, ignoring an
   undrained offline tail) — wrong seed persisted TODAY, wire-visible forked chain at A2.4. A
   regression in the architect's own merged NC-03 introduced by M2-01.
2. **AUD-L4-1 (HIGH, reachable TODAY):** boot tip-guard NotFound→BLOCK directly contradicts the
   drain's NotFound→safe-redrive; tip-guard wins and wedges the entire FN backlog + refuses ingress
   on a restored/crashed DB.
3. **AUD-L2-1 (HIGH, A2.4):** the online lane has the same seed-fork as M2-01 (unfixed) PLUS no
   manual-recon escalation for the resulting KVT2 wedge in convergence/boot (only the drain
   escalates).

## FINAL VERDICT
`FT=1 HIGH=3 MED=1 LOW=4` (+ 4 known-dup, 2 refuted).
**NO-GO** for M1+M2 into the production write-path: AUD-L6-1 (FT) persists a corrupt MAC seed at
boot today and forks the legal chain at A2.4, and AUD-L4-1 (HIGH) wedges restored-DB FNs today —
both on top of the already-open M2-N1 NO-GO.

## Disposition (fix routing)
- **AUD-L6-1** → join the M2-N2b fix (shared root: ever-OFFLINE_LOCAL_ACK advances the seed → boot
  projection must use the ever-issued tip). FT — architect re-review migration-grade.
- **AUD-L4-1** → new contract: tip-guard NotFound must not BLOCK a non-ACK in-flight SENT tip
  (defer to drain safe-redrive). Reachable today → priority.
- **AUD-L2-1 + AUD-L5-1** → A2.4 pre-flight batch (online-lane seed-fork + convergence/boot
  manual-escalation + KVT1 superseded tolerance), pair with M2-X2.
- LOWs → doc/cleanup batch.
