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

## Batch C — A2.4 online-lane pre-flight gate (AUD-L2-1 HIGH; NOT a now-blocker, A2.4-gated)

Pair with M2-X2. Must land BEFORE the inline write-path is vivified (A2.4).
- **Online seed-fork:** the online lane must either advance the seed per-issued-doc OR gate a new
  acquire/sign on the prior doc being terminal — eliminate the genesis fork where two online SENT docs
  share one `previous_hash` (proven reachable by `kill_point_matrix.rs:1107`).
- **Convergence + boot manual-escalation:** the online convergence tick (`online_convergence.rs`, maps
  to Infrastructure log-and-skip; filters Sent|Kvt1) and the boot KVT2 arm (`boot_phase.rs:3508-3537`,
  Warning-only) must escalate a `ChainSeedMismatch` (a KVT2-wedged online doc) to
  `RequiresManualReconciliation` — mirror the drain's M2-04 seam — so a locally+DPS-fiscalized receipt
  cannot wedge at KVT2 with no operator surface.
- **Pins:** two online SELLs resting at SENT (online_confirm Hold) → assert NO shared `previous_hash`
  (fork eliminated) OR the second acquire is gated; a forced ChainSeedMismatch at KVT2 → manual-recon
  via BOTH convergence and boot (not log-and-skip/Warning-only).

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
4. **C** — deferred to the A2.4 vivification scope (gate, not now).
5. **D** — cosmetic, fold into whichever batch touches the file.

**NO-GO lifts when A1+A2+A3+B are merged + architect-reviewed.** C remains a hard A2.4 pre-flight gate.

## Per-batch Opus prompts
Each batch's contract above IS the spec. Hand Opus: "Implement Batch <X> per
`docs/reviews/legacy-2026-06/REMEDIATION-PLAN-2026-06-13.md` §Batch <X> — read it + the cited
adjudication dossiers (m2-gpt-critic-2-adjudication.md, m1-m2-adversarial-pass-2026-06-13.md). TDD,
HOT-zone, minimal diff, escalate on divergence. Branch fix/<batch-slug> from fresh origin/main. Gate
(nextest+fmt+clippy). DO-NOT-MERGE — architect reviews migration-grade. Co-Authored-By: Claude Opus 4.8
<noreply@anthropic.com>."
