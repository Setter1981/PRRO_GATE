# M2 — second external-critic (GPT) pass — architect adjudication

**Date:** 2026-06-13 · **Adjudicator:** architect (Fable), code-verified · **Subject:** post-fix
adversarial pass over the COMPLETED M2 surface (after #150/#151/#154/#158/#159 merged).
**Verdict: NO-GO confirmed for M2 → A2.2/A2.4 until M2-N1 + M2-N2a land.** The second contour
earned its keep again (parity with the M1 broken-CLEAN-verdicts pass): it found a **reachable FT
that M2-01 itself introduced** and every internal pass (hunt, seam-pass, rulings) missed.

## Root cause (synthesis)
**M2-01 turned offline-origin docs into a STRICT predecessor chain** (each offline doc signs
`previous_hash = unsigned(prior issued doc)`; the seed advances at OfflineLocalAck). But the
offline-drain SEQUENCING predates that and still assumes INDEPENDENT docs: it **sibling-continues**
past a failed doc and only halts on `if shift_in_pending_drain` + manual-recon. A strict chain
requires a **strictly-sequential drain that STOPS at the first non-ACK predecessor** — sending
`lnd+1` whose predecessor was rejected/held is unsafe (cascade-reject or wire-chain corruption).
This is the same *class* as the seam-pass synthesis (drain didn't inherit the graceful arms) but
deeper: it is a **sequencing invariant**, not just an arm.

## Rulings (all CONFIRMED — code-verified)

### M2-N1 | FT | CONFIRMED — drain sends a successor of a rejected predecessor on a plain Opened shift
- **Reachability (the linchpin) — CONFIRMED reachable, NOT test-only.** `stage_offline_ack`
  validates the shift as `{Opened | OpenedLocalPendingDrain}` (widened set) but does NOT transition
  it. So a shift **opened ONLINE (plain `Opened`)** whose node then drops to OFFLINE mid-shift and
  issues offline SELLs accumulates offline-origin docs while the shift STAYS `Opened`. (The common
  "shift open, connectivity dropped, kept selling offline, reconnect, drain" case.) `OpenedLocalPendingDrain`
  is only for shifts *opened* offline — it is NOT implied by offline docs on an online-opened shift.
- **Mechanism — CONFIRMED by code.** `backlog_drain.rs` per-doc loop: the halt/escalate is gated
  `if shift_in_pending_drain { if Failed{manual_recon:true} { escalate; return } }` (~:986-1001).
  On plain `Opened` the gate is false → **no escalate, no break, no predecessor-ACK check** → the
  loop falls through to the next doc and processes (sends) it. With M2-01, doc C's
  `previous_hash = hash(B)`; after B is `REJECTED` the drain still sends C.
- **Impact:** a single rejected offline doc now **cascade-poisons the entire remaining offline
  backlog** (each successor's predecessor is unconfirmed): either DPS `ERROR_BAD_HASH_PREV`-rejects
  the whole tail, or (if DPS doesn't enforce) accepts a chain with a rejected predecessor. Either
  way the ledger ends with a broken chain + customer-facing offline receipts DPS never accepted,
  with no auto-recovery.
- **Existing test pins the now-unsafe behavior:** `backlog_drain_per_doc_loop.rs:778/792` (B
  rejected, C reaches ACK) — encodes the pre-M2-01 "independent docs" assumption. The fix MUST
  update it (M2-02 class: a test pinning behavior that became unsafe after a chain-invariant change).

### M2-N2a | HIGH | CONFIRMED — FN wedges in GoingOnline/Draining with no operator surface
- After the reject (plain Opened), the loop finishes → `finalize_drain` → `finalize_eligibility`
  = NotEligible (not all-ACK: B is REJECTED) → `OFFLINE_DRAIN_PARTIAL`; node stays `GoingOnline`,
  session stays `Draining`. Next tick: cohort excludes REJECTED (`fiscal_documents.rs:897/901`) →
  empty → empty-backlog recovery finalizes ONLY if all session docs ACK (`:777/943`) → B is
  REJECTED → does NOT finalize → `OFFLINE_DRAIN_SKIPPED_EMPTY_BACKLOG`, returns. **Forever wedge**:
  no finalize, no manual escalation (plain Opened doesn't escalate). Operator has no actionable state.

### M2-N2b | MED→HIGH | CONFIRMED — invariant_scan false ChainBreak on a REJECTED offline-origin predecessor
- The MAC-walk `issued` set (`invariant_scan.rs:205-210`) advances `expected` for
  `ACK || (offline_fiscal_no NOT NULL && state ∈ {OFFLINE_LOCAL_ACK,SENT,KVT1,ERROR_RETRYABLE,KVT2})`
  — it **excludes REJECTED + REQUIRES_MANUAL_RECONCILIATION**. But an offline-origin doc that
  reached `OFFLINE_LOCAL_ACK` ALREADY advanced the local seed (M2-01), so even if it is later
  REJECTED/Manual at drain, a successor (`prev = hash(it)`) chained off it. The walk not advancing
  over it → **false `ChainBreak{lnd: successor}`**. Reachable independent of N1: emit lnd1,lnd2
  offline (lnd2.prev=hash(lnd1)); drain rejects lnd1; scan → false ChainBreak at lnd2.
- **Fix:** the offline-origin issued-set must include `REJECTED` and `REQUIRES_MANUAL_RECONCILIATION`
  (ever-issued = ever reached OFFLINE_LOCAL_ACK ⇒ seed advanced, regardless of later drain outcome).

### M2-N4 | MED | CONFIRMED (uniform gap with boot) — superseded trusts local max_lnd, not the DPS tip id
- Superseded is decided ONLY by `doc.lnd < max_submitted_lnd(fn)` (`kvt2_confirm.rs:666` →
  `classify_check_result` `ServerFiscalIdMismatch` arm :336). It does NOT check that the DPS-returned
  `actual_id` equals the `server_fiscal_no` of one of OUR newer submitted docs. A FOREIGN/garbage
  DPS tip (someone else fiscalised on the FN, or DPS-state divergence) → mislabeled benign
  `SupersededHold`, **masking real structural drift**. **Boot's M1-02 arm has the SAME gap**
  (decides on lnd, `actual_id` only forensic — `boot_phase.rs:2424` + `complete_probe_trace_tip_superseded`).
- **Fix (uniform, both boot + drain):** superseded = `(doc.lnd < max_submitted_lnd)` AND
  `(actual_id == sfn of some submitted doc with lnd > doc.lnd)`; else structural drift.

### H-M2-1 | confirmed-relevant | DPS ERROR_BAD_HASH_PREV cascade contract
- Whether DPS cascade-rejects successors of a rejected predecessor is DPS-behavior-dependent and
  unknown from our corpus (zero rejected-offline-backlog empirics). It does NOT change the fix
  (stop-on-non-ACK is safe under both behaviors) but MUST be pinned by a stubbed-DPS contract test
  + fed to the live test-campaign.

## GO/NO-GO
**CONCUR: NO-GO** for vivifying M2 into the live write-path (A2.2 → A2.4) until M2-N1 (FT) and
M2-N2a (wedge) land. N2b/N4 ride the same fix-batch.

## Locked fix contract (architect → implementer)

TDD order; HOT-zone (offline drain + scan + boot). Minimal diff per item; escalate on divergence.

1. **Strict-sequential offline-origin drain (M2-N1, FT).** In `backlog_drain.rs` per-doc loop:
   the offline-origin chain must STOP at the FIRST doc that does NOT reach `ACK` — REJECTED,
   RequiresManualReconciliation, transient hold, or superseded-hold (ANY non-ACK) — **regardless of
   `shift_state`** (replace the `if shift_in_pending_drain`-only halt). Do NOT process/send higher-`lnd`
   offline-origin successors once a predecessor is non-ACK (their predecessor is unconfirmed → unsafe
   in the strict chain). Online-origin / non-chained docs: behaviour UNCHANGED. Note: `SupersededHold`
   already `continue`s today — under strict-sequential it must ALSO stop the chain (a superseded
   predecessor is non-ACK from the successor's chain view); re-examine that interaction explicitly.
   - **Pin:** 3 offline SELLs via the REAL inline path, plain `Opened` shift, DPS rejects the middle
     doc → assert ONLY doc#1 sent+acked, doc#2 REJECTED, **doc#3 NOT sent** (`send_chk==2`, doc#3 stays
     `OFFLINE_LOCAL_ACK`), FN escalated to manual-recon (NOT wedged), `invariant_scan::assert_clean`.
   - **Update** `backlog_drain_per_doc_loop.rs:778` (it pins the now-unsafe sibling-continue) — rebuild
     it to the strict-sequential expectation (RED against current code first).
2. **No wedge / clear escalation (M2-N2a).** When the offline chain halts on a non-ACK predecessor,
   escalate the FN to `RequiresManualReconciliation` (§16.7) **regardless of shift_state** — plain
   `Opened` included — so there is an operator surface, never a silent GoingOnline/Draining wedge.
   - **Pin:** plain-Opened reject → FN reaches manual-recon (not GoingOnline-forever); second drain
     tick does not silent-skip into limbo.
3. **Scan issued-set (M2-N2b).** Add `REJECTED` + `REQUIRES_MANUAL_RECONCILIATION` to the
   offline-origin branch of the MAC-walk `issued` set in `invariant_scan.rs` (ever-OfflineLocalAck ⇒
   seed advanced). Keep online-origin unchanged.
   - **Pin:** SQL repro (lnd1 offline-origin REJECTED unsigned=H1 ofn=1; lnd2 offline-origin
     OFFLINE_LOCAL_ACK prev=H1 unsigned=H2 ofn=2; node seed=H2) → scan CLEAN (no false ChainBreak).
4. **Superseded requires matching tip id (M2-N4), UNIFORM in boot + drain.** Superseded ⟺
   `doc.lnd < max_submitted_lnd` AND `actual_id` equals the `server_fiscal_no` of a submitted doc with
   `lnd > doc.lnd`; else `StructuralDrift`. Apply to BOTH `kvt2_confirm::classify_check_result` (thread
   the check via the caller, classifier stays pure) AND `boot_phase` M1-02 arm.
   - **Pin:** foreign/garbage tip id with a newer submitted doc present → `StructuralDrift` (NOT
     superseded), in both boot and drain.
5. **H-M2-1 contract test.** Stubbed-DPS test pinning the ERROR_BAD_HASH_PREV behavior assumption +
   a note feeding the live test-campaign. Does not gate the fix.

**Gate:** `cargo nextest run -p prro --features test-support` + fmt + `clippy -D warnings`.
**DO-NOT-MERGE** — architect reviews the delta migration-grade (FT). Items 1+2 are the NO-GO
blockers; 3/4/5 ride the same batch.

## Note on adjudication method
M2-N1 ruled CONFIRMED on **conclusive code evidence** (explicit `if shift_in_pending_drain`-only
gate + no break/predecessor-check on the plain-Opened path + verified reachability), NOT a fresh
throwaway repro — the empirical pin IS the fix's mandatory RED test (item 1), so it lands as part
of the fix rather than being double-run now. N2b/N4 code-verified at the cited lines.
