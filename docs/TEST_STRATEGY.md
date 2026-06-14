# PRRO Gateway — Test Strategy (five-oracle model)

**Architect-authored, 2026-06-14.** Umbrella strategy over the existing detailed plans
(`PILOT_ACCEPTANCE_TEST_PLAN.md`, `GOLDEN_DATASET_PLAN.md`,
`superpowers/plans/2026-06-10-architecture-audit-and-test-plan.md`,
`superpowers/plans/2026-06-11-kill-point-matrix-spec.md`, `webcheck_reverse/`,
`reviews/legacy-2026-06/REMEDIATION-PLAN-2026-06-13.md`, `LEGAL_INVARIANTS.md`).

---

## 0. Framing — why a fiscal system needs this

For a fiscal gateway the **invariants ARE the product**: not losing a receipt, not
double-fiscalizing, an unbroken MAC chain, offline-code conservation. These are **legal/regulatory
requirements**, not engineering nice-to-haves — a lost or double-fiscalized receipt is the
merchant's tax liability. So "correctness under failure" is not gold-plating; it is the thing we
sell.

**Pilot-readiness is multi-dimensional.** A green test suite proves the *state machine* axis; it
says nothing about *interop* with the real DPS, the *unbuilt features* (RETURN/UI/monitoring), or
the *operational envelope* (hardware variety, deployment, cashier error). This document covers the
*correctness + interop* axes — the highest legal-liability ones. The other axes have their own
tracks.

---

## 1. The dominant bug class (empirically derived)

Every fiscal correctness defect found in the M1–M2 hardening cycle was a **semantic / sequencing /
recovery / cross-fix** bug, **never a happy-path bug**:

| Finding | Class |
|---|---|
| M2-01 (offline seed not advanced) | seed |
| AUD-L6-1 (boot projects stale seed) | seed |
| M2-N1 (strict-drain sends orphaned successor) | drain-reentry / chain |
| AUD-K8-1 (drain re-entry after escalate re-sends) | drain-reentry |
| AUD-L5-1 (KVT1 superseded false-fatal) | chain / shared-fn |
| EDIT-E (widened fetch breaks a consumer's premise) | shared-fn-caller |

**Four recurring classes** (the taxonomy): **seed** (MAC-seed advance/read), **chain**
(`previous_hash` continuity + `ChainSeedMismatch` handling), **drain-reentry** (idempotency of
recovery loops under re-entry), **shared-fn-caller** (a widened shared predicate cuts a caller that
assumed the old narrow behavior — the spine: "a widened invariant invalidates old code", same root
as M2-01).

The happy path (`open shift → sell → ACK → close`) is a *single* deterministic sequence, already
covered by integration tests and proven against the real DPS. **All the risk lives in the unhappy
combinatorics:** {operations} × {crash points} × {DPS responses} × {offline transitions} ×
{timing} = millions of sequences, hand-unreachable.

---

## 2. Method — sampling → enumeration

- **Sampling** (adversarial multi-lens passes): each finder/critic samples *some* bugs by
  intuition. Cheap, discovers the bug **classes**, but never converges ("the next pass finds more").
  This is the discovery phase that *produced* the taxonomy above.
- **Enumeration** (the systematic sweep): once the classes are known, build the **complete list** of
  every site of each class and verify each against its invariant. Completeness is *checkable*
  (re-grep + completeness-critic), so there is a **floor** — when every site is verified clean/fixed
  across N independent passes, the class is closed.

You cannot enumerate a class you have not discovered; sampling maps the territory, enumeration
exhausts it. The trigger to switch is when findings stop revealing *new* classes and become
variations of known ones — reached at the end of the M2 cycle.

---

## 3. The five oracles

Each oracle answers a different question on a different axis. No single one is sufficient.

| # | Oracle | Axis / question | Properties |
|---|--------|-----------------|------------|
| **O1** | **Systematic sweep** (enumeration) | CODE — "is every site of every known class accounted for?" | exhaustive over known classes; produces the M1-M2 exit criterion |
| **O2** | **`invariant_scan`** (`db/invariant_scan.rs`, `assert_clean`) | FISCAL-TRUTH — "does the DB state violate any invariant *right now*?" | always-on oracle embedded in every test + (future) prod canary; the **spine** — every test's `assert_clean` strengthens for free as the scan hardens |
| **O3** | **Stub-DPS invariant-fuzzer** (model-based stateful, §4) | STATE-MACHINE under faults — "does any operation sequence break an invariant?" | exhaustive, deterministic, CI, millions of runs, **minimal-repro shrinking**; subsumes the kill-matrix |
| **O4** | **DPS test server** (`cabinet.tax.gov.ua`) | INTEROP — "does the real DPS accept our wire output?" | live, coarse (accept/reject), rate-limited, stateful-on-their-side, live RETURN; covers **both egress channels** when EVPZ lands |
| **O5** | **WebCheck (decompiled) + production DBs** | REFERENCE-DIFFERENTIAL + real corpus — "does the new gateway reproduce the proven predecessor byte-for-byte?" | fine (byte-exact XML/sig/chain); real-world sequences; de-risks O3's reference model; "rewrite with the old system as ground-truth" |

### O2 — `invariant_scan` (the spine)
Six check groups (unique monotonic `lnd`; no `SENDING` at rest; `ACK ⇒ KVT1_RAW`; MAC walk
`previous_hash`/seed; `REJECTED|ERROR` inbox ⇏ accepted doc = AUD-1; offline-code
atomicity/backing/non-reuse = DUR-1). Hardened continuously by the fix cycle (A1 scan-widen, the
AUD-L8-2 ERROR-parity, M2-N2b issued-set). **Every other oracle leans on it** — O3 calls
`assert_clean` after each step; the kill-matrix asserts it after every crash.

### O3 — stub-fuzzer (the workhorse, see §4)
Explores the unhappy combinatorics O2 alone can't reach by sampling. Crash/timeout/reorder
fault-injection only the stub can do (you cannot crash the real DPS on demand).

### O4 — DPS test server
The interop seam already exists: `live_dps_extended_smoke` under `#![cfg(feature="live-dps")]`
(compiled in CI, run by hand against the real server; W4-Z3). Interop happy-path already proven
("DPS accepted our checks"). **Caveats:** their server is stateful (the chain advances → no free
replay of arbitrary sequences → curated replay, not exhaustive fuzz); rate-limited; non-deterministic
(no clean shrink); needs provisioned test FNs/keys (CMS, ADR-004).

### O5 — WebCheck + production DBs (the strongest)
WebCheck is the operator's **proven predecessor** PRRO (working against the real DPS; its production
DBs = the "WebCheck corpus"). Decompiled (`docs/webcheck_reverse/`) → exact logic visible. This is a
**rewrite-with-old-system-as-oracle**: replay all production history through the new Rust gateway and
assert **byte-identical** fiscal output to WebCheck; any divergence = a new-gateway bug (WebCheck is
proven). It also (a) supplies the **real seed corpus** for O3 (the edge cases that *actually
happened* — offline/returns/corrections), and (b) can **replace/de-risk O3's hand-built reference
model** (the expensive Phase-1 wildcard). **Caveats:** live byte-differential only if the decompile
is *executable* (else spec-extraction of the XML/sig/`related_receipt_id` format — still closes the
RETURN format gap, RT-3); a **known-divergence allowlist** where the new gateway *deliberately*
improves on WebCheck; production DBs = real merchant data → anonymize / keep local; WebCheck-accepted
≠ spec-pure → still cross-check format against O4.

---

## 4. The invariant-fuzzer (O3) — design

A **model-based stateful property test**: a generator emits random *valid* fiscal-operation
sequences → drives them through the REAL system → asserts the invariants after EVERY step;
`proptest` shrinks any failure to the minimal repro.

**~60 % already scaffolded** — this is wiring existing assets into a generative loop, not a
from-scratch build:

| Component | Status |
|---|---|
| Oracle = `invariant_scan::assert_clean` | ✅ exists |
| Crash-injection = kill-matrix cancellation-injection | ✅ exists (crash = one op; K1–K9 generalize to "crash at any point in any sequence" → fuzzer **subsumes** the kill-matrix) |
| DPS adversary = `KpStub` | ✅ exists (generalize to a fault-schedule) |
| Determinism = `synchronous=FULL` + W2 `ReconcileGuard` test-seams (`run_boot_reconciliation`, `inline::run`, `drain`, `set_node_mode`) | ✅ exists (seed replay) |
| Generator (op-alphabet + `proptest` strategy + preconditions) | ❌ build |
| Reference model | ❌ build — **or use O5/WebCheck** (de-risks this) |

**Operation alphabet** (the domain model): shift(open/close/Z) · receipt(sell/**return**) ·
channel(go_offline/go_online) · offline(open_session/sell_offline/drain) · DPS-response
(ack/reject/timeout/superseded/`ERROR_BAD_HASH_PREV`/not_found) ·
fault(crash@{acquire,sign,send,kvt1,kvt2,finalize,offline-ack,drain}→reboot) ·
clock(offline-cap 36 h / cert-expiry). **Egress profile** (gRPC `sendChkV2` / EVPZ) is an additional
axis once EVPZ lands.

**Phases & estimate** (inferred, effort ≈ calendar under the single-implementer model):
- **Phase 0 — MVP (~1–2 wk):** ~8-op alphabet + generator + interpreter (reuses the kill-matrix
  seams) + `assert_clean` after each step. **No reference model** — "scan stays clean after every
  operation" already catches the whole class. **Most of the ROI is here.**
- **Phase 1 — reference model (~1–2 wk, WILDCARD):** a second simplified write-path that must agree
  with recovery semantics under faults; can 1.5–2×. **WebCheck (O5) collapses this wildcard** — use
  it as the reference instead of hand-building.
- **Phase 2 — expand alphabet** (RETURN/Z/superseded/BAD_HASH_PREV/clock). **Phase 3 — CI** (PR-time
  small-N + nightly large-N + shrink→auto-filed minimal repro). Full 0–3 ≈ 6–10 wk effort.

**Commit only to the MVP**, decide from there. `proptest` gives shrinking for free.

---

## 5. Coverage as a cartesian product (the compounding return)

The fuzzer tests each operation **in combination with everything else** — not in isolation. A new
feature is **+1 op in the alphabet**, and the moment it lands it is exercised against ALL invariants
under ALL fault combinations × both egress channels. Example — **RETURN** (today the *least*-tested
op: zero returns in goldens, RT-3): wiring it into the fuzzer (~1 day: generator + interpreter +
model + RETURN-invariants) then yields hours of `return × offline × crash × chain × supersede`
exploration — versus weeks of incomplete hand-tests. **Marginal cost of validating each new feature
→ near zero**, and it compounds: bugs found become persisted regression seeds; cross-fix-invalidation
becomes a CI gate instead of a manual hunt; the harness replaces the by-hand adversarial passes.

This is also an **enterprise/diligence asset** — "continuous generative invariant testing across the
fault space, byte-differential against a proven predecessor" is a reusable answer in vendor
security/certification reviews.

---

## 6. Egress channels (both must be covered)

Egress to DPS is profiled in `runtime/outgress.rs` (typed `Unimplemented` placeholders, never
`todo!()` panic — Round-2 audit):
1. **`GrpcSendChkV2Transport` — IMPLEMENTED**: gRPC `sendChkV2` → `cabinet.tax.gov.ua:9443` (test) /
   `prro.tax.gov.ua:443` (prod); the only production `DpsChannel` impl (`GrpcDpsChannel`).
2. **EVPZ profile — NOT IMPLEMENTED** (`docs/evpz_dps_protokol/`): builders/sign/transport return
   typed `Unimplemented`.

EVPZ mirrors the **dormant online-lane / A2.4** pattern (`UnimplementedWritePath`, `supervisor.rs`):
present as a profile, returns `Unimplemented`, activated later — and **must pass O3 + O4 when it goes
live**. Interop and fuzzer coverage must span **both** channels (each is its own DPS protocol).

---

## 7. What this does NOT cover (honest bounds)

- **Operational / human / hardware envelope** — deployment, hardware variety, cashier error, support.
- **Unbuilt features** — the fuzzer can't test what doesn't exist (RETURN, UI, monitoring, printing,
  Maria 301). Pilot-readiness needs those built first.
- **Performance / load** — only partially (soak); see `PERFORMANCE_PLAN.md`.
- **Legal / IP / privacy** — production DBs (O5) are real merchant data; handle per the
  security/secrets discipline.

A green five-oracle run = "the fiscal core does not lie under the chaos of real-world operation,
interop matches the real DPS, and output reproduces the proven predecessor." It is **not** a blanket
"pilot ready" — that is multi-axis.

---

## 8. Sequencing

1. **Batch C** (online-lane robustness; in flight) — AUD-L5-1 ✅, AUD-L2-1b + AUD-L2-1a RED-pin in
   progress.
2. **Systematic sweep (O1)** — enumerate seed/chain/drain-reentry/shared-fn-caller; **hardens the O2
   oracle in passing**; produces the M1-M2 code exit criterion. (Fuzzer wants a stable core; sweep
   cleans the known first.)
3. **Invariant-fuzzer MVP (O3, Phase 0)** — on the swept-stable core.
4. **WebCheck differential (O5) + DPS test-server campaign (O4)** — interop + reference-replay of the
   production corpus; live RETURN; both egress channels.
5. **Pilot** — when the correctness + interop axes are green AND the other axes (features, ops) are
   ready.

Each new feature thereafter rides all oracles at near-zero marginal cost — which is what makes entry
to pilot, and every release after, safe along the highest-liability axes.
