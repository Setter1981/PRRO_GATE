# Reconciled Pilot Roadmap — one line, three framings

**Date:** 2026-05-30 · **Branch:** `rust-gateway` (HEAD `a940520`) · **Status:** orientation artifact, no code change

> **Why this doc exists.** Operator: *"где-то сбились с роадмапа."* Correct — because three roadmap framings were
> never overlaid on one line: (1) the **canonical ADR milestones** M3a..M6, (2) the **W-series PRs actually shipped**,
> (3) the **pilot critical path** WL-0..6 + the Runtime Spine (RS). This doc reconciles all three, marks exactly where
> execution drifted from the ADR, and draws the single forward line to a live pilot. **Canonical source of the
> milestone sequence:** the ACCEPTED ADR `docs/superpowers/specs/2026-05-07-pilot-runtime-decision.md`.

---

## §1 — The canonical line (ADR) + you-are-here

```
M3a  ONLINE write-path (Rust)                                   ✅ CLOSED   (PR #37-46)
M3b  Phase-6-min offline subsystem                              ✅ CLOSED   (W0a-W9b, PR #69; +W12 hardening tail)
M4   Rust ingress + maria304 re-bridge                          🟡 PARTIAL  ◀── YOU ARE HERE
M5   services tail: ingress WRITER + post-boot reconciler + crypto wiring + ops hooks   ⬜ NOT STARTED
M6   admin surface (CLI)                                        ⬜ NOT STARTED
```

ADR estimate: **5-8 calendar months** M3a→pilot at the project's test-gate discipline. M3a+M3b are behind us.
The live pilot runs **Rust-only** (no Python ingress/services/signing/admin; `prro_crypto` embedded via `rlib`).
Binding pilot gate (ADR §Tests-required #3): **Phase 6 offline lifecycle passes against the Rust-only stack** —
this cannot be relaxed.

---

## §2 — Execution reality: what each shipped PR delivered vs its ADR milestone

| PR | Worklet | Delivered | ADR bucket | On-mandate? |
|----|---------|-----------|------------|-------------|
| #96 | M4 **W1** | axum skeleton (`IngressServer` shell — `serve(){}` left empty) | M4 ingress | ✅ ingress foundation |
| #97 | M4 **W2a** | secure-db foundation (operators pool) | M4 ingress | ✅ ingress foundation |
| #98 | M4 **W2b** | operator-bindings (`BindingsRegistry`) | M4 ingress / M5 crypto | ✅ DI bridge (the resolver) |
| #99 | M4 **W3** | ingress DTO + parity | M4 ingress | ✅ ingress foundation |
| #100 | M4 **W4-Z0** | outgress step-0 (DocType→profile routing) — *not* the ingress payload conversion RS-2 needs | *outbound* | ⚠️ M5-natured, M4-labeled |
| #101 | M4 **W4-Z1** | ФСКО Table 23 XML expansion | *outbound* | ⚠️ M5-natured |
| #103 | M4 **W4-Z2a** | tax-mapping wiring | *outbound* | ⚠️ M5-natured |
| (unmerged) | M4 **W4-Z3** | native-crypto live smoke (SHIFT_OPEN/SELL/Z proven 2026-05-29) | M3a-end smoke gate (ADR test #4) | ⚠️ M3a gate, run very late |
| #105/#106/#107 | hardening | clippy / CMS-time-parser / **signing-cert fix** (the `-14` root-cause) | M3a crypto correctness | ⚠️ late crypto validation |

**What the table shows:** W1-W3 (#96-99) genuinely built the M4 ingress *foundation*. Then the **W4 sub-series
(Z0/Z1/Z2a)** turned to outbound routing / ФСКО / tax — **M5-services-tail breadth carried under the M4 label** —
and the **M4 ingress was never finished**: `IngressServer::serve()` has been an empty `{}` since the W1 skeleton (#96).
The native-crypto track (W4-Z3 + #106/#107) was effectively the ADR's **M3a-end live-DPS smoke gate**, discharged
late — and it surfaced real wire/cert bugs exactly as the ADR warned local mock DPS would hide.

---

## §3 — The drift, named

**At the W3→W4 seam, M4 silently expanded from "Rust ingress + re-bridge" (ADR) to "ingress skeleton + outgress +
tax + crypto" (actual).** In that expansion the *original* M4 deliverable — a running ingress → write-path → re-bridge,
i.e. a binary that actually processes a receipt — was deprioritized behind outbound/crypto depth.

The result is the [Runtime Spine gap](./2026-05-30-runtime-spine-connection-blueprint.md): `prro serve` boots and
idles (`main.rs:365` *"M1 just idles"*); `stage_acquire` has 0 production callers; the whole fiscal library is real
and tested but **nothing in the binary drives it**. That is not a new problem to solve — it is **the unfinished tail
of M4 plus the front of M5**, and the code says so itself: the missing key loader is tagged *"M5 crypto wiring"*
(`bindings.rs:117`) and the missing supervisor is tagged *"W7"* (`app.rs:149`).

---

## §4 — Reconciliation: RS + WL-0..6 mapped back onto M4/M5

The pilot critical path (WL-0..6) and the Runtime Spine (RS) are **not a parallel scheme** — they are the ADR's
M4-finish + M5, re-derived. The mapping:

| Pilot-path token | = ADR work | RS unit | Hard-Blocker it bears |
|------------------|-----------|---------|------------------------|
| RS-2 ingress server + payload conversion | **finish M4** (ingress + re-bridge) | RS-2 | — |
| RS-1 supervisor / composition-root | **M5** "ops integration" + W7 | RS-1 | — |
| RS-3 live write-path worker | **M5** "ingress **writer**" | RS-3 | unblocks HB(1) |
| RS-4 spawn reconcile/drain/probe + health | **M5** "post-boot **reconciler**" | RS-4 | — |
| RS key loader / `SigningContext` | **M5** "crypto wiring" (`bindings.rs:117`) + ADR **O2** | RS-1 | — |
| WL-1 online shift lifecycle (+ offline reachability) | **M5** lifecycle | (post-RS) | **closes HB(1)** |
| WL-2 real-ingress cycle proof / WL-3 MAC byte-exactness | pilot acceptance proofs | (post-WL-1) | **HB(?) / WL-3 blocker** |
| WL-5 load/soak · WL-6 runbook+observability+matrix | pilot package | (tail) | — |
| M6 admin CLI (ADR **D2**) | M6 | — | — |

**Cross-cutting NO-GO gates** (ALGORITHMIC_MAP §1.11 — five hard blockers; RS+WL-1 close only HB(1)):
HB(2) native **attached** crypto unmerged (`feat/m4-w4-z3` branch — live-ACCEPTED, needs merge + review);
HB(3) `PRRO_FISCAL_MODE` not harness-enforced; HB(4) INV-05/06 channel-switch guards UNWIRED;
HB(5) INV-09/10 offline time/count limits + the 24h continuous-shift wall UNWIRED.

---

## §5 — The single forward line to pilot (dependency-correct)

```
   ┌─ NOW: mid-M4, ingress unfinished, binary idles ─┐
   ▼
1. FINISH M4 INGRESS          RS-2: real IngressServer::serve (axum POST /v1/ingress/maria304) + payload-conversion
                              layer (wire-shape → CheckJson/ZReportJson/ShiftOpenJson) + inbox insert. Re-bridge
                              maria304 driver Python→Rust (ADR M4 mandate).
   ▼
2. M5 RUNTIME SPINE           RS-1 supervisor (call build_from_db + resolver) · RS-3 live write-path worker ·
   (closes the idle gap)      RS-4 spawn reconcile/drain/probe + health. CRYPTO WIRING = the one real build:
                              production OperatorKeyLoader (port live-proven JKS→SigningContext + CheckSignBlob from
                              W4-Z3) — channel ctor already exists (grpc.rs:62, wire-only). [ADR O2 → manual JKS]
   ▼
3. M5 SHIFT LIFECYCLE         WL-1 pieces 0b..5 (online shift edges 1/3/8/10 + node_state Offline setter +
   (closes HB(1))             open_session caller). Needs the Q1 arch decision (A node_state-centric vs B shifts-table).
   ▼
4. PILOT ACCEPTANCE PROOFS    WL-2 real maria304 receipt → DPS Ack  ‖  WL-3 MAC internal-advance byte-exactness
   (run in parallel)          (online, vs DPS echo — currently unproven live).
   ▼
5. CLEAR THE OTHER BLOCKERS   HB(2) merge native attached crypto + review · HB(3) enforce PRRO_FISCAL_MODE ·
                              HB(4) wire INV-05/06 channel guards · HB(5) wire INV-09/10 + 24h shift wall.
   ▼
6. PILOT PACKAGE              D3 backup/restore runbook rehearsed (hard prereq) · WL-6 runbook+observability+matrix ·
                              WL-5 load/soak · M6 admin CLI (D2).
   ▼
7. PILOT GO                   Phase-6 offline lifecycle passes against the Rust-only stack (ADR binding gate).
```

The **only hard serialization** the evidence forces is 1→2→3 (you cannot drive the lifecycle without the spine, nor
the spine without the ingress). 4 runs after 3; WL-2∥WL-3; step 5's four blockers are independent of each other and
can be picked off in parallel with 3-4; step 6's runbook/load/CLI overlap.

---

## §6 — Open decisions that gate sizing (close before planning the step)

**ADR open items** (§3 of the ADR): **O1** 1С OLE-bridge scope — does the pilot operator use 1С? (if yes M4 grows
~2-3 wk); **O2** key/identity provisioning — **RS-Q2 answers: manual JKS-path loader for pilot scale, no automation**;
**O3** retention + shift_aggregation depth — pilot duration estimate decides (skip both for a <1-month pilot).

**WL decisions** (integration map §5): **Q1** Option A (node_state-centric) vs B (full shifts-table) — gates WL-1/step 3;
**Q3** per-shift-reset vs per-RRO-continuous `lnd`; **Q5** offline in pilot or descope; **Q-load** WL-5 live vs mock.

**RS decisions** (blueprint §6): **RS-Q1** ingress response inline-vs-worker; **RS-Q5** `request_id` mint strategy.
(RS-Q2/Q3 resolved; `Protocol::Maria304` present.)

---

## §7 — What is NOT on the critical path (candidates to pause)

Real work, but **behind the spine** — pausing these does not delay pilot, and continuing them does not advance it:
- further **outgress breadth** (EVPZ / Єдине вікно onboarding) — pilot ships FSCO/ZZD only;
- the **W4-Z4 hardening campaign** (queued) — valuable, but it hardens a library the binary doesn't yet run;
- **`prro_crypto` clippy debt** + general cleanup.

The discipline that ends the drift: **do not add more M5-library breadth under M4 labels — finish M4's ingress, then
build the M5 spine.** Everything in §5 is on the line; everything in §7 waits.

---

*Reconciles: ADR `2026-05-07-pilot-runtime-decision.md` (canonical M-series) · merge history #95-#107 (execution) ·
`2026-05-30-runtime-spine-connection-blueprint.md` (RS, code-verified) · `2026-05-29-pilot-integration-map.md`
(WL-0..6) · `ALGORITHMIC_MAP.md` §1.11 (5 hard blockers). W-PR→milestone bucketing in §2 is an honest interpretation
against the ADR's milestone definitions, not ADR-literal labels.*
