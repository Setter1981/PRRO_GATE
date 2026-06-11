# Pilot integration map & critical path (stock-take)

- **Date:** 2026-05-29
- **Purpose:** stop accreting pieces ad-hoc; put every floating piece into ONE coherent picture with status + dependencies, and sequence the work toward a single target. Triggered by the operator observation: "много кусков которые лепим в кучу."
- **Status:** PLANNING ONLY — no implementation pending operator go.
- **Feeds:** the W4-Z4 "Pilot Readiness" campaign (this is its ALGORITHMIC_MAP core).

---

## 1. The single coherent target

> A pilot-ready gateway that runs the full fiscal cycle **SHIFT_OPEN → SELL/RETURN → Z_REPORT** through the **real ingress** (not test seed-PREPARED), **online against real DPS**, **reliably** (transient wire errors handled, not stranded), with the **MAC chain advancing via the gateway's own internal mechanism** (no per-doc re-`lastChk` crutch), **crash-recoverable**, and **observable**.

Everything below is measured against that target. A piece is "pilot-blocking" iff the target cannot be met without it.

---

## 2. Inventory (what we actually have)

### ✅ PROVEN (live, solid) — the foundation is real
| Piece | Evidence |
|-------|----------|
| Native `prro_crypto` signing (signing-cert fix #107) | live-confirmed; resolved the `CryptBadSign`/`-14` blocker |
| ATTACHED CAdES-BES + signingTime profile | accepted by real DPS (else `-1`) |
| Live WIRE — full cycle | SHIFT_OPEN `1g41M3jDt-Q` (ServiceChk) → extended SELL `AOBSkplfIUU` (Chk) → Z_REPORT `L2AMnY2MkmA` (ZReport), all ACCEPTED on cabinet.tax.gov.ua:9443, 2026-05-29 |
| W4-Z2a driver→canonical tax translation | live: items carried driver 5/7 → wire emitted canonical `TX="1"/"2"` + excise `<CA>` + UKTZED `CZD` + VAT-incl `<TX>` |
| MAC seed from `lastChk` (DPS-truth) | accepted (no `-12`) across the 3-doc chain |
| Reconcile drive `PREPARED→SIGNED→SENT` | the W4-Z3 harness path |

### ⚠️ SCAFFOLDING (exists in repo + spec + tests, NOT wired in production)
| Piece | Status | Worklet |
|-------|--------|---------|
| Online shift lifecycle | `node_state.shift_state` never opens online; `shifts` table + `current_shift_id` never populated in prod (`insert_created` 0 prod callers; the only inserts are `#[cfg(test)]`). De-facto guard = `node_state.shift_state`. | **WL-1** (plan written: `2026-05-29-online-shift-lifecycle-wiring.md`) |
| Offline shift lifecycle | likely ALSO scaffolding — drain transitions require `current_shift_id` (NULL in prod → "structural drift"/no-op); `stage_offline_ack` doesn't apply spec edge 2. | WL-1 §Q5 (Option B) or separate |

### 🔬 PROVEN-ONLY-WITH-A-CRUTCH (real gap behind a test shortcut)
| Piece | The crutch | The real gap |
|-------|-----------|--------------|
| MAC chain on the **online** path | the live smoke re-seeded `<MAC>` from `lastChk` per doc (DPS-truth) | production online does NOT re-`lastChk` per doc — it advances `node_state.last_known_unsigned_xml_sha256` to OUR locally-computed hash at finalize. **GAP #2:** our hash == DPS's only if our canonical serialization == DPS echo byte-for-byte. The cp1251 observation is favorable but NOT proof. **Unproven live.** → **WL-3, pilot-blocking** |
| Real ingress | the smoke seeds PREPARED docs, bypassing `stage_acquire` (guards, lnd alloc, snapshot pin, shift admission) | the real POS path goes through ingress → `stage_acquire`; only WL-1 makes SELL admissible post-open → **WL-2** |

### ❓ OPEN / RELIABILITY
| Piece | Concern | Worklet |
|-------|---------|---------|
| Transient wire reject → terminal `REJECTED` | piece-7 Z: 1st attempt REJECTED, identical retry ACCEPTED → a transient DPS reject classified non-retryable could STRAND a doc in prod. Review `error_routing` classification of Z-applicable codes (-2/-3/-10…). | **WL-4** |
| Recovery of the online cycle | the SENT→KVT1→KVT2→Ack reconcile path + shift transitions must be crash-idempotent | folded into WL-1/WL-3 tests |

### 💤 DEFERRED / POST-PILOT (do NOT pull into the pile now)
- EVPZ "Objects" onboarding loader (`evpz_objects.rs` exists) — read-only registry pull; could land as a pre-pilot admin convenience, but the wider EVPZ submission protocol is post-pilot ([[m4-outgress-architecture]]).
- Extended Z aggregation (`<TXS>/<IO>/<EPZ>`) — minimal Z works live; full breakdown is W4-Z2c.
- `prro_crypto` own clippy debt + broken `envelope.rs` lib-test; CMS/cert time-parser hardening (review-r5 siblings).
- W4-Z3 runbook/docs + the formal W4-Z3 review campaign.

---

## 3. Critical path to the target

```
WL-0  Confirm foundation (Q6: is shift machinery wired on main / a shell we missed?) + lock A/B
   │
WL-1  Online shift lifecycle (SHIFT_OPEN→Opened / Z→Closed)      ── PILOT-BLOCKING
   │      unblocks ↓
WL-2  Real-ingress cycle proof (SHIFT_OPEN→SELL→Z through stage_acquire, mock + live)
   │
WL-3  MAC internal-advance byte-exactness (online, no lastChk crutch)  ── PILOT-BLOCKING
   │      (can run partly parallel to WL-2; both need WL-1)
WL-4  Transport reliability / transient-reject taxonomy            ── PILOT-RELEVANT
   │
WL-5  Load / soak (online cycle through ingress, bounded)          ── needs WL-1/2/3
   │
WL-6  Runbook + observability + pilot test matrix                  ── pilot packaging
```

**Dependencies in plain words:** you cannot load-test (WL-5) or prove the real cycle (WL-2) until the shift opens locally (WL-1). You cannot trust the online MAC chain in production (WL-3) regardless of WL-1 — it is an independent pilot-blocker the smoke sidestepped. WL-4 (reliability) is orthogonal but pilot-relevant.

---

## 4. Coherent worklet sequence (the anti-pile-up plan)

| WL | Title | Pilot? | Depends on | Est. shape |
|----|-------|--------|-----------|-----------|
| WL-0 | Foundation confirm + A/B decision | gate | — | 1 investigation piece + operator decision |
| WL-1 | Online shift lifecycle wiring | **blocking** | WL-0 | plan ready; ~5 pieces (node_state setters / stage_send confirm / stage_acquire intent / e2e / optional live) |
| WL-2 | Real-ingress cycle proof | **blocking** | WL-1 | extend smoke to drive via ingress; assert shift opens, SELL admitted, cycle to SENT |
| WL-3 | MAC internal-advance correctness | **blocking** | WL-1 | prove gateway's internal next-MAC == DPS echo (the GAP #2); live multi-doc without per-doc lastChk |
| WL-4 | Transient-reject taxonomy | relevant | — | review + adjust `error_routing` for ambiguous/transient codes (bias retry/offline over terminal, per [[auto-offline-unknown-errors]]) |
| WL-5 | Load / soak | relevant | WL-1/2/3 | bounded live burst OR mock-throughput — **decide target (Q-load)** |
| WL-6 | Runbook + observability + test matrix | pilot pkg | WL-1..5 | W4-Z4 artifacts |
| — | EVPZ loader / extended Z / crypto-debt | post-pilot | — | explicitly out of pilot scope |

---

## 5. Decisions still owed (block WL-0/sequencing)

- **Q1 (arch):** WL-1 Option **A** (node_state-centric, recommended) vs **B** (full shifts-table retrofit incl. offline).
- **Q3 (fiscal, operator):** local document number per-shift-reset vs per-RRO-continuous (affects `next_lnd` on open).
- **Q5:** offline shift scaffolding — fix with WL-1 (Option B) or separate post-pilot worklet.
- **Q6 (foundation):** confirm the scaffolding finding isn't a branch artifact (check `main` / runtime shells).
- **Q-load:** WL-5 against LIVE DPS (rate-limit risk) vs mock-throughput.

---

## 6. What this map changes

- **No more piece-by-piece discovery mid-flight.** WL-3 (online MAC internal-advance) is now an explicit pilot-blocker we *name* up front instead of hitting it during load.
- **The shift-lifecycle plan is WL-1, not a standalone.** It slots into a sequence; its output (admissible SELL) is WL-2's input.
- **Scope fence:** EVPZ / extended-Z / crypto-debt are explicitly post-pilot — they do not enter the pile.
- **Two genuine pilot-blockers remain unproven:** WL-1 (shift open) and WL-3 (online MAC internal-advance). Everything else is either proven, reliability-polish, or post-pilot.
