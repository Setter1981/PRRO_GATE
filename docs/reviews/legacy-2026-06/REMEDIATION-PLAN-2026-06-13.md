# M1+M2 remediation plan — "лечение всего" (architect-locked, 2026-06-13)

Consolidates EVERY open confirmed finding from the M2 second external-critic (M2-N1..N4) and the
M1+M2 multi-lens adversarial pass (AUD-L6-1/L4-1/L2-1/L5-1 + LOWs) into sequenced, dependency-ordered
fix batches. Each batch is a separate PR, DO-NOT-MERGE, architect reviews the delta migration-grade.
Implementer = Opus; reviewer = Fable. Base each batch on the then-current `origin/main` (re-fetch
first — parallel track active). Gate every batch: `cargo nextest run -p prro --features test-support`
+ fmt + `clippy -D warnings` (`~/.cargo/bin/cargo`).

**Root theme (why all of these exist):** a later invariant change (M2-01: offline-origin docs become a
STRICT predecessor chain whose seed advances at OFFLINE_LOCAL_ACK) silently invalidated earlier merged
code (drain sequencing, NC-03 seed projection, the online lane) that assumed independent docs / ACK-only
seed advance. The fixes propagate the new invariant everywhere it must hold.

**NO-GO status:** lifts for production write-path entry when **Batch A + Batch B** land. **Batch C** is
the A2.4-vivification gate (NO-GO for A2.4 specifically). Batch D is cosmetic.

---

## Batch A — offline-drain + chain-seed correctness (NO-GO blocker; mostly reachable-today data-corruption)

Sub-ordered A1→A2→A3 (A1 first so `invariant_scan` is a trustworthy oracle for A2/A3 pins).

### A1 — issued-set + boot seed projection share ONE root: "ever-OFFLINE_LOCAL_ACK advances the seed"
Covers **M2-N2b** (scan false ChainBreak) + **AUD-L6-1 (FT)** (NC-03 boot seed projection stale) +
the CANCELLED-label dup (AUD-L8-3/L6-2).
- **A1.1 (M2-N2b):** in `invariant_scan.rs` MAC-walk `issued` set, the offline-origin branch must include
  `REJECTED` and `REQUIRES_MANUAL_RECONCILIATION` (and `CANCELLED` if reachable) — any offline-origin doc
  that EVER reached OFFLINE_LOCAL_ACK advanced the local seed, regardless of later drain outcome.
  Online-origin unchanged. Define the canonical predicate `offline_origin_ever_issued(state) =
  offline_fiscal_no IS NOT NULL AND state ∈ {OFFLINE_LOCAL_ACK,SENT,KVT1,KVT2,ERROR_RETRYABLE,REJECTED,
  REQUIRES_MANUAL_RECONCILIATION}`.
- **A1.2 (AUD-L6-1, FT):** add an offline-aware boot seed projection. NC-03 branch (a)
  (`boot_phase.rs:1599-1608`) must project `node_state.last_known_unsigned_xml_sha256` from the
  **highest-lnd EVER-ISSUED doc** (online ACK OR offline-origin per A1.1's predicate), NOT
  `last_ack_unsigned_xml_sha256` (ACK-only). Add a repo helper (e.g. `last_issued_unsigned_xml_sha256`)
  mirroring the walk's issued semantics; keep `last_ack_unsigned_xml_sha256` for any ACK-only caller.
  Fix the now-false docstring (fiscal_documents.rs:792-800) and the boot comment (boot_phase.rs:1593).
  Same predicate as A1.1 — single source of truth for "issued".
- **Pins:** (a) SQL repro (ACK lnd1 h1 + OFFLINE_LOCAL_ACK lnd2 prev-h1 h2 + lnd3 prev-h2 h3, node_state
  deleted) → `run_boot_reconciliation` projects **h3** (not h1); `invariant_scan::assert_clean`. (b) the
  M2-N2b SQL (lnd1 offline-origin REJECTED + lnd2 chained off it) → scan CLEAN, no false ChainBreak.
  (c) existing backup_restore Tests 15/15b stay green (ACK-tip case unchanged).

### A2 — strict-sequential offline drain (M2-N1 FT + M2-N2a wedge)
- The offline-origin drain chain must STOP at the FIRST doc that does NOT reach `ACK` (REJECTED /
  RequiresManualReconciliation / transient hold / superseded-hold — ANY non-ACK), **regardless of
  `shift_state`** (replace the `if shift_in_pending_drain`-only halt at `backlog_drain.rs:986`). Do NOT
  process/send higher-lnd offline-origin successors once a predecessor is non-ACK. Online-origin /
  non-chained docs UNCHANGED. `SupersededHold` must ALSO stop the chain (a superseded predecessor is
  non-ACK from the successor's view) — re-examine that interaction explicitly.
- On halt, escalate the FN to `RequiresManualReconciliation` (§16.7) **regardless of shift_state** (plain
  Opened included) — never a silent GoingOnline/Draining wedge.
- **Pins:** 3 offline SELLs via the REAL inline path, plain `Opened` shift, DPS rejects the middle →
  ONLY doc#1 sent+acked, doc#2 REJECTED, **doc#3 NOT sent** (`send_chk==2`, doc#3 stays
  OFFLINE_LOCAL_ACK), FN → manual-recon (not wedged), `invariant_scan::assert_clean`; second drain tick
  does not silent-skip into limbo. Rebuild `backlog_drain_per_doc_loop.rs:778` (pins the now-unsafe
  sibling-continue) RED-first.

### A3 — superseded requires a matching tip id, uniform across boot + drain + KVT1 (M2-N4 + AUD-L5-1)
- Superseded ⟺ `doc.lnd < max_submitted_lnd` **AND** the DPS-returned `actual_id` equals the
  `server_fiscal_no` of a submitted doc with `lnd > doc.lnd`; else `StructuralDrift` (real divergence /
  foreign tip). Apply to BOTH `kvt2_confirm::classify_check_result` (thread the tip-id check via the
  caller; classifier stays pure) AND `boot_phase` M1-02 arm (~:2424).
- **AUD-L5-1:** extend the superseded tolerance to the **KVT1-reentry** classifier path too — a healthy
  resting KVT1 doc that is no longer the tip (newer doc submitted) must be superseded-held, not
  fatal `StructuralDrift::LastChkIdMismatch`.
- **Pins:** foreign/garbage tip id (no matching newer submitted sfn) → `StructuralDrift` in boot AND
  drain; a genuine newer-tip match → superseded-hold; a KVT1 resting doc displaced by a newer tip →
  held, not fatal.

### A4 — durability companions K8/K9 (kill-matrix, added 2026-06-14)
**Rationale (calibrated — do NOT broadly expand the kill-matrix):** the recent FT/HIGH findings were
SEMANTIC / sequencing / cross-fix regressions, NOT crash-window bugs — the L2 (crash/durability) lens
returned mostly DEFENCES VERIFIED (single-envelope offline-ack, atomic lnd-alloc, idempotent finalize,
benign SentReplay orphan). So crash-durability is sound; the kill-matrix needs ONLY the two NEW durable
paths this fix-cycle introduces. (The bigger oracle win is the scan hardening in A1 — every existing
kill-point's `assert_clean` strengthens for free. The kill-matrix does NOT and cannot catch the
semantic/sequencing class — that is the adversarial-pass's job; do not over-invest here.)
- **K8 — crash mid strict-sequential drain (companions A2/M2-N1).** Crash AFTER doc#1→ACK + doc#2 →
  escalate-Manual committed, BEFORE `drain` returns. Next tick/boot MUST be idempotent: doc#2 stays
  RequiresManualReconciliation, doc#3 stays OFFLINE_LOCAL_ACK (NOT re-sent), zero double-escalation,
  `invariant_scan::assert_clean`.
- **K9 — crash mid offline-session chain (companion to A1/AUD-L6-1).** Crash AFTER offline-ack of doc#1
  advanced the seed, BEFORE doc#2 signs. Reboot MUST sign doc#2 with `previous_hash =
  unsigned(doc#1)` (seed survives under `synchronous=FULL`); chain unforked, scan clean. Durability
  companion to the AUD-L6-1 boot seed reconstruction.
- **Timing:** NON-blocking for NO-GO (blockers are the semantic FTs). Fold into the A1/AUD-L6-1 + Batch B
  review cycle (they already touch boot/drain durability), NOT a separate pass. Online-lane convergence
  crash-points deferred to A2.4 (online path dormant).

---

## Batch B — boot tip-guard NotFound must not false-BLOCK (AUD-L4-1 HIGH + AUD-L4-2 LOW; reachable TODAY)

- The tip-guard's NotFound arm (`boot_phase.rs:2154-2163`) currently flips mode→BLOCKED on a
  `last_chk` NotFound of the newest *submitted* tip — but post-ruling-3 `expected` is
  `last_submitted_server_fiscal_no` which includes a non-ACK **in-flight offline-origin SENT** doc, and
  the drain treats that exact SENT+NotFound as a SAFE re-send (Sent→ER→Pattern-B). The tip-guard runs
  first and wedges the whole FN + refuses ingress. **Fix:** the tip-guard must BLOCK on NotFound ONLY
  when the diverged tip is a genuine ACK-tail divergence; for a non-ACK in-flight SENT tip it must
  **defer to the drain's safe-redrive** (skip the BLOCK, let the drain run). Re-examine the Mismatch
  arm for the same non-ACK-tip subtlety.
- **AUD-L4-2 (LOW):** fix the now-false NotFound/Mismatch block-rationale comment ("last ACK / confirmed
  ACK" → "newest submitted tip, which may be a non-ACK in-flight SENT").
- **Pins:** GoingOnline FN + active offline session + in-flight SENT doc + lastChk NotFound → drain
  safe-redrives the SENT doc (NOT a permanent BLOCK); a genuine ACK-tail NotFound (no in-flight SENT)
  still BLOCKs (preserve the real stale-ledger guard). Extend `backup_restore.rs` tip-guard tests.

---

## Batch C — AUD-L2-1 (HIGH) — SCOPE-SPLIT by reachability (architect ruling 2026-06-14, vs `93904a7`)

**Verified by the 4-agent adversarial+design pass 2026-06-14.** AUD-L2-1 has THREE parts with
different reachability; the architect ruling (operator-approved, 2026-06-14) is to **fix the
reachable-today parts NOW** and **defer the dormant-lane seed-fork REDESIGN to A2.4** (the online lane
is `UnimplementedWritePath` in production — `supervisor.rs:180`; redesigning hot fiscal MAC-chain
semantics for dormant code without runtime feedback is premature; A2.4 is where the lane activates and
re-integrates). Done now as **PR C-1 (AUD-L5-1)** + **PR C-2 (AUD-L2-1b escalation + AUD-L2-1a RED-pin)**.

### Done now — AUD-L2-1b: ChainSeedMismatch → manual-recon surface (convergence + boot)
Reachable TODAY (boot-KVT2 arm via NC-03-class restore; convergence today-but-rare, broadens at A2.4).
The drain escalates `ChainSeedMismatch`→manual (M2-04, `backlog_drain.rs:2031-2056` → `escalate_drain_to_manual`),
but the online convergence tick log-and-skips it (`online_convergence.rs:131-141`, via
`advance_to_ack`→`ConfirmError::Infrastructure` `kvt2_advance.rs:207-211`) and the boot KVT2 arm is
Warning-only (`boot_phase.rs:3574-3603`). **RULING: SHIFT-level escalation, mirror drain M2-04**
(doc stays KVT2; same error-class → same surface across all three owners). Extract a shared
`escalate_fn_to_manual_recon` (edge 15 for plain Opened — confirmed working: K8 #168; edges 6/14 for
pending-drain), idempotent (skip if already RMR). Add `ConfirmError::ChainSeedMismatch` (typed downcast
before the generic Infrastructure map) + non-fatal `ConfirmDrainOutcome::ChainSeedMismatch`. Boot
NULL-shift edge → Critical `BOOT_KVT2_CHAIN_SEED_MISMATCH_NO_SHIFT` (no crash).

### Done now — AUD-L5-1 (folded from A3): KVT1-reentry superseded → HOLD
See A3. **RULING: HOLD-parity with boot** (`complete_probe_trace_tip_superseded`, Warning, no CAS) —
NOT Manual (online tick has no chain-head like the offline drain). Three coordinated changes in
`kvt2_confirm` (fetch widen `:685-692` to `Kvt1Reentry`, SupersededHold handler Kvt1 branch `:1004-1057`,
light `TIP_SUPERSEDED` Warning) + convergence arm (`online_convergence.rs:220-232` bail → counted hold).
SentFresh stays excluded; M2-N4 tip-id-match preserved.

### DEFERRED to A2.4 — AUD-L2-1a: online-lane MAC-seed-fork REDESIGN (A2.4 ACTIVATION PREREQUISITE)
**Do NOT activate `InlineWritePath` (flip `supervisor.rs:180` `UnimplementedWritePath`) until this is
resolved.** Tracked by the `#[ignore]` RED-pin `m1_02_online_seed_fork_a24_prerequisite`
(`tests/kill_point_matrix.rs`) + a barrier-comment at the binding site, both landed in PR C-2.

**The defect (verified `93904a7`):** the online lane reads the seed at sign (`stage_sign.rs:279-302`,
per-doc `previous_hash`) but advances it ONLY at terminal ACK/finalize (`stage_finalize.rs:285-321`,
gated `offline_fiscal_no.is_none()`), NEVER at SENT/issuance (`stage_send` has no seed write). When an
online receipt rests at SENT via `online_confirm` Hold (`inline.rs:748-758`, transient lastChk OR empty
data_sign), the seed is untouched, so a SECOND receipt signs the SAME un-advanced seed → two SENT docs
with identical `previous_hash` (genesis fork). On convergence doc#1 ACKs (seed advances), doc#2 hits
`stage_finalize` ChainSeedMismatch → durably wedges at KVT2.

**The A2.4 fix design (architect-preferred = A; mirror M2-01 STRUCTURE, advance per-issued-doc at local
commit — NOT the rejected 'advance-at-finalize-of-last' candidate):**
- **(A)** Advance `node_state.last_known_unsigned_xml_sha256` to the doc's `unsigned_xml_sha256` INSIDE
  the `Sending→Sent` `with_immediate` envelope in `stage_send` (the online 'issuance' moment, symmetric
  to offline-ack), guarded by the same pre-advance drift assert as `stage_offline_ack.rs:361-368`
  (read ns seed in-tx, assert `== doc.previous_hash`, else fail-closed). Advance ONLY on the fresh
  `Sending→Sent` `Applied` CAS (never on SentReplay re-entry — mirror `stage_offline_ack` `Applied` gate).
- Then **generalize** the `stage_finalize.rs:285` `offline_fiscal_no.is_none()` gate into a unified
  'already-advanced-at-issuance' predicate so finalize skips BOTH offline-ack AND online-Sent docs
  (advance once-per-issued-doc, lane-agnostic).
- **(B) fallback** only if (A) proves unsafe: gate acquire/sign of doc#2 on prior-doc terminality
  (hurts liveness under the Hold path — head-of-line stall — so (A) is preferred).

**LOAD-BEARING open questions for the A2.4 architect (ruled at A2.4, not now):**
1. **Issuance moment = SENT vs KVT1?** Recommend SENT (matches offline-ack 'local commit = chain
   commit'); KVT1 reopens the SENT-SENT fork window (two docs rest at SENT before either reaches KVT1),
   forcing design (B) acquire-gating.
2. **REJECTED-after-SENT policy:** a doc whose seed advanced at SENT but is later DPS-rejected at
   lastChk/KVT must escalate **manual-recon (NO seed rollback)** — mirror offline (offline-ack advances,
   a later drain reject escalates, never un-advances; M3b crossed-local-commit-threshold pin). Confirm
   no compensating rollback (which reintroduces the rollback-semantics §6.3 deliberately avoids).
3. **Landing:** as an A2.4 prerequisite (RED-pin un-ignored + fix) — the lane must not go live forked.

**Anchors (`93904a7`):** seed read `stage_sign.rs:279-302`; online advance `stage_finalize.rs:285-321`;
M2-01 offline ref `stage_offline_ack.rs:339-385`; Hold-at-SENT `inline.rs:748-758`; no-advance
`stage_send.rs:1352-1360`; `advance_to_ack` collapse `kvt2_advance.rs:206-211`; drain wedge endpoint
`backlog_drain.rs:2037-2055`; prod binding `supervisor.rs:180` + `inline.rs:3-4`.

**Pins (now, Batch C):** AUD-L2-1b — a forced ChainSeedMismatch at KVT2 → manual-recon via BOTH
convergence and boot (not log-and-skip / Warning-only). AUD-L2-1a — RED-pin asserts the FIXED chaining
(`doc2.previous_hash == doc1.unsigned_xml_sha256`) so it FAILS today, `#[ignore]`'d as the A2.4 gate.

---

## Batch D — LOW / doc-cleanup (anytime, non-blocking)
- **AUD-L1-2:** remove or document the unreachable whitelist edges ((OfflineLocalAck,Cancelled) etc.) —
  state explicitly they have no production invoker.
- **AUD-L3-1:** fix the Tier-2 STOP_MODE escalation doc-comment (auto-recovery "через W8
  return_online_probe" that the probe does not perform).
- **AUD-L8-2:** invariant_scan check-5 (RejectedInboxWithAcceptedDoc) — align with the replay
  short-circuit which covers REJECTED+ERROR (currently guards only REJECTED).

---

## Open hypothesis (test-campaign, not a code blocker)
- **H-AUD-1 / H-M2-1:** stubbed/live DPS contract test for `ERROR_BAD_HASH_PREV` cascade — does sending
  a successor of a rejected predecessor cascade-reject? Confirms the blast radius assumption behind
  M2-N1 / AUD-L6-1 / AUD-L2-1. Feed the live test-campaign (WebCheck corpus). Does not gate the fixes.

---

## Sequencing & dependencies
1. **A1** first (scan issued-set + boot seed projection) — fixes the FT seed-corruption AND makes
   `invariant_scan` a correct oracle for every downstream pin. Shared root, single PR.
2. **A2** (strict-sequential drain) and **B** (tip-guard NotFound) are independent of each other and of
   A3 — can proceed in parallel after A1; both are reachable-today blockers.
3. **A3** (superseded tip-id match + KVT1) — independent; pairs naturally with A2's review.
4. **A4** (K8/K9 durability companions) — NON-blocking; ride the A1/AUD-L6-1 + Batch B review cycle.
5. **C** — SCOPE-SPLIT (2026-06-14, architect ruling): L2-1b escalation + L5-1 (KVT1 superseded, from A3)
   done NOW (PR C-1/C-2, reachable-today); **L2-1a online seed-fork REDESIGN deferred to A2.4** (dormant
   lane — see §Batch C "DEFERRED to A2.4"). RED-pin + binding-site barrier land in C-2.
6. **D** — cosmetic, fold into whichever batch touches the file.

**STATUS (2026-06-14):** NO-GO **LIFTED**. Merged: #162 (A2 M2-N1/N2a + A3 M2-N4 + A1.1 M2-N2b scan-half);
#164 (A1.2 AUD-L6-1 boot seed projection + Batch B AUD-L4-1); #167 (Batch D D1/D2/D3 + A4 K8/K9 — K8 was a
`#[ignore]` RED-pin that surfaced **AUD-K8-1**); #168 (AUD-K8-1 fix — drain re-entry RMR guard). In
progress: **Batch C** — PR C-1 (AUD-L5-1) + PR C-2 (AUD-L2-1b escalation + AUD-L2-1a RED-pin), architect
contract locked 2026-06-14.

**NO-GO is LIFTED (A1+A2+A3+B in via #162/#164).** C is now a hard **A2.4 pre-flight gate** only — its
reachable-today parts (L2-1b, L5-1) land now; the L2-1a seed-fork redesign is an A2.4 activation
prerequisite (do not flip `InlineWritePath` until the RED-pin is resolved).

## Per-batch Opus prompts
Each batch's contract above IS the spec. Hand Opus: "Implement Batch <X> per
`docs/reviews/legacy-2026-06/REMEDIATION-PLAN-2026-06-13.md` §Batch <X> — read it + the cited
adjudication dossiers (m2-gpt-critic-2-adjudication.md, m1-m2-adversarial-pass-2026-06-13.md). TDD,
HOT-zone, minimal diff, escalate on divergence. Branch fix/<batch-slug> from fresh origin/main. Gate
(nextest+fmt+clippy). DO-NOT-MERGE — architect reviews migration-grade. Co-Authored-By: Claude Opus 4.8
<noreply@anthropic.com>."
