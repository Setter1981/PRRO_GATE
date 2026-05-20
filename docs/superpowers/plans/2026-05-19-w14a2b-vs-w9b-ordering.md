# W14a-2b vs W9b — Ordering decision (post PR #66 merge)

**Date:** 2026-05-19
**Status:** decision pending — operator selects W14a-2b first OR W9b first
**Context:** PR #66 (W14a-2a) merged at `rust-gateway` `67add6b` (2026-05-17).  Both slices unblocked, no hard dependency between them.

---

## 1. Side-by-side scope comparison

|  | **W14a-2b** | **W9b** |
|---|---|---|
| **One-line scope** | Channel-aware stage_acquire + sign-time cashier enforcement + Conflict test polish | Offline backlog drain orchestration (Pattern C stage-and-flip) |
| **Plan section** | `2026-05-17-m3b-w14a-2-repository.md` §1.4 + §1.5 | `2026-05-14-m3b-implementation.md` §Task 9 |
| **Primary new module** | None — extends existing `stage_acquire` + `SigningContext` | `services::offline_sync::backlog_drain` + `App::drain_offline_backlog_with(...)` |
| **Files touched (est.)** | 5-7 (stage_acquire / SigningContext / signer pipeline / 2-3 test files) | 8-10 (new backlog_drain module / app.rs entry / 4+ integration tests / W12 prep) |
| **LOC est.** | ~400-600 | ~800-1200 |
| **Day budget** | 1.5-2.5 days | 3-4 days (operator-acknowledged as "largest task" in plan) |
| **Review rounds est.** | 2-3 (smaller scope, focused on signer plumbing) | 4-5 (drain orchestration is invariant-heavy — I4/I8/MAC chain/W12 interleave guard) |
| **BlockedBy (now)** | W14a-2a ✓ merged | W7a/W7b/W8a/W8b/W9a all ✓ merged |
| **Unblocks** | Sign-time cashier audit forensics for shift integrity | **Pilot Phase 6 critical path** (steps 8-9: synchronization + final DPS ACK) |
| **Pilot Phase 6 critical path?** | ❌ No (shift-lifecycle integrity, not offline replay) | ✅ **YES** (drain → final ACK is the proof Phase 6 needs) |
| **Hot zones touched** | `services/write_path/*` (stage_send signer wiring), `services/signing/*` | `services/write_path/stage_send.rs` (consumer-side), `services/offline_sync/*` (new), `services/reconciliation/boot_phase.rs` (drain entry callsite) |
| **Frozen invariant risk** | I7 (canonical envelopes schema_version stays unchanged), I10 (minimal diff vs ergonomic SigningContext refactor) | **I4 (idempotency — drain re-entry safety)**, **I8 (state-machine correctness across crash points)**, MAC chain preservation (lnd ASC) |
| **W12 dependency** | None | W12 (in-drain `lastChk` confirmation) is the NEXT step after W9b — W9b shapes W12's call seam |
| **Carries open question** | OQ from spec §16.8 — should SHIFT_CLOSE/Z_REPORT also enforce senior-cashier role? (deferred to W14a-3 role registry) | OQ4 (per plan §Task 9 acceptance) — strict `lnd` ASC vs DPS-tolerated alternative; default is strict ASC |

---

## 2. Decision criteria

### 2.1 Pilot critical path argument (FOR W9b first)

The pilot acceptance is **Phase 6**: enter offline → issue receipts → block Z-report while backlog pending → return online → **synchronize** → final DPS ACK → Z-report after empty.

The ONLY missing pieces of Phase 6 are:
- **W9b** (sync step 8): orchestrates the drain.
- **W10** (Z-report guard, step 5-6): blocks Z-report while backlog non-empty.
- **W12** (Kvt2 confirm via lastChk): final DPS ACK evidence.

W14a-2b does NOT appear on Phase 6 critical path.  Shift-state expansion (the W14a track entirely) was an **operator-priority detour from W9** to land shift-state vocabulary BEFORE drain orchestration — the vocabulary is now landed (W14a-1 + W14a-2a merged).  Returning to W9 track resumes the original pilot trajectory.

Net: **W9b first** minimizes time-to-pilot.

### 2.2 W14a track coherence argument (FOR W14a-2b first)

W14a-2a left two carry-forwards that close the W14a chapter:
- §1.4 sign-time cashier enforcement — UNTIL this lands, signed docs are NOT verified against `shifts.opened_by_cashier_id`.  Forensic gap: an attacker (or buggy signer pipeline) could sign on behalf of a different cashier and the gateway wouldn't notice until human review.  But: cashier_certs+composite-FK already prevents *registration* of non-FN cashiers, so the gap is narrow.
- §1.5 channel-aware stage_acquire — UNTIL this lands, W14a-1's defensive arm refuses ALL ops in OpenedLocalPendingDrain, including OFFLINE channel ops.  This means: operator who opens a shift offline and tries to sell while still in `OpenedLocalPendingDrain` will hit the defensive arm and get a confusing refusal.  This is actually a regression vs current shift behavior because OFFLINE channel should work in `OpenedLocalPendingDrain` per spec §3.3.

Net: **W14a-2b first** closes a real UX regression in W14a-1's defensive arm + closes the cashier-attribution gap before drain (W9b) hands off to multi-doc replay.

### 2.3 Risk-surface argument

| | W14a-2b | W9b |
|---|---|---|
| Hot path touched | stage_send signer wiring (M3a hot path) | stage_send 4-pre CAS (already widened in W9a) + new drain module |
| Frozen invariant load | I7/I10 (low) | I4/I8 + MAC chain (high) |
| Crash-point coverage | None (no W11-Δ extension) | Multiple — drain interrupt + replay must be deterministic (future W11-Δ fixtures) |
| Independent reviewability | YES — sign-time cashier + channel-aware ops are 2 orthogonal items, can split into W14a-2b-1 / W14a-2b-2 if too invasive | Single integrated drain — splitting hurts coherence |

W9b has a larger invariant surface (drain is the most invariant-heavy task in M3b per plan §Task 9 budget); W14a-2b is more contained.  But W14a-2b is sequencing-light (no W12 dependency forward), while W9b shapes the W12 call seam — getting W9b right first defers re-work on W12.

---

## 3. Recommended order (operator decides)

Two coherent paths:

### Path A — W14a-2b first (W14a track closure)
1. W14a-2b (1.5-2.5 days) → closes shift-state chapter; merges before drain.
2. W9b (3-4 days) → drain orchestration on cleaner shift surface.
3. W10 (1 day) → Z-report guard.
4. W12 (1.5-2 days) → kvt2_confirm via lastChk.
5. W11-Δ (1 day) → 7 deterministic replay fixtures.
- **Total**: ~9-11 days to Phase 6 close.

### Path B — W9b first (pilot critical path)
1. W9b (3-4 days) → drain orchestration first.
2. W10 (1 day) → Z-report guard.
3. W12 (1.5-2 days) → kvt2_confirm.
4. W11-Δ (1 day) → replay fixtures.
5. W14a-2b (1.5-2.5 days) → returns to W14a track after pilot Phase 6 is provable.
- **Total**: ~8-10 days to Phase 6 close + W14a closure separately.

**Difference**: Path A is ~1 day longer but yields a cleaner shift-state surface throughout drain.  Path B is shortest path to pilot Phase 6 demonstrable.

---

## 4. Open question to operator

**Q1**: Is W14a-1's defensive arm refusing OFFLINE channel ops on OpenedLocalPendingDrain currently triggering for any test fixtures or integration paths in the offline acceptance flow?  If YES → §1.5 channel-aware stage_acquire is blocking and Path A is forced.  If NO (defensive arm only fires on online channel attempts, which is the intended W14a-1 contract) → operator chooses based on schedule preference.

**Q2**: Pilot date target — does the operator have a specific calendar deadline for Phase 6 demonstrable? If YES + tight → Path B.  If schedule is flexible → either path acceptable.

**Q3**: Senior-cashier role enforcement on Z_REPORT (per §16.8 OQ) — is this scope creep beyond W14a-2b §1.4, or should W14a-2b §1.4 explicitly close it?  Affects W14a-2b LOC estimate (~+200 if included).

---

## 5. Implementation plan deferred

Both W14a-2b and W9b have their canonical implementation specs already (§1.4 + §1.5 of `2026-05-17-m3b-w14a-2-repository.md` for W14a-2b; §Task 9 of `2026-05-14-m3b-implementation.md` for W9b).  No new specs needed; this doc resolves ordering only.

Once operator picks Path A or Path B, open the chosen slice as the next PR per the existing per-slice review cadence (~2-4 review rounds typical for hot-zone work).
