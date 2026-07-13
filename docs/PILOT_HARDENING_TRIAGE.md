# Pilot Hardening Triage — strengthening the correctness moat

**Framing:** pilot = ONE cash register live against real DPS with real money. Pre-pilot gate
is NARROW (verify the SHIPPED surface + live-DPS ground-truth + recovery net + legal caps +
no-false-green CI). The DEEP rigor (model-checking, coverage-guided, full oracle suite,
multi-FN) is the ENTERPRISE/FLEET MOAT — sequenced AFTER the pilot, and it is the main
commercial product, not optional polish (see `project_product_vision`).

**Why this triage exists (session 2026-07-11/12):** the EPZ offline oracle divergence lived
while agent-green (1860/0), a local architect gate (seed-variance), AND external CI (CLEAN)
all passed. Only the fuzzer + a teeth-proof caught it. Green ≠ correct. This roadmap closes
the gaps that failure exposed.

---

## PRE-PILOT (blocks live launch)

| # | Item | Why blocking | Effort |
|---|---|---|---|
| ~~P1~~ | ~~CI-wire fuzzer required + nightly large-N~~ **DONE/VERIFIED (2026-07-12)** | Investigated: fuzzer runs in the REQUIRED `x86_64` gate at **256 cases/harness** (`fuzz_cases()` default); `fuzzer-nightly.yml` cron 2am at **FUZZ_CASES=4096**; regression seeds persisted + a guard step fails the build on a new seed. The offline bug was NOT a CI-N failure — it was UNREACHABLE until the alphabet grew (`L5Probe` shifted the distribution); the same 256-gate caught it once L5 landed. **Real levers = alphabet-completeness (M3) + verify-self, NOT CI-N.** | — |
| P2 | **Live replay+diff vs real DPS** (the pilot on-ramp itself) | Model = our understanding; real DPS = truth. If model is wrong the same way as prod, differential is blind — only ground-truth catches it. Sequence: PR-DIAG `prro doctor --live` → Tier-1 smoke (live_smoke_1..5b) → T=112 ask-codes ground-truth → replay+diff → soak. Key+test-FN on disk. | L |
| P3 | **Mutation-testing on the SHIPPED surface** (EPZ / L5 / cash_ledger / shift / offline) — one-time density snapshot | Prove we're not shipping toothless fiscal code. L0 is measured (20/20); the new surface is not. NOT the eternal-nightly apparatus — just a density snapshot of what ships. mold+sccache make it cheap. | M |
| P4 | **Offline legal caps durable** — 168h/mo + 36h/shift + 24h auto-Z (T2 code-reserve + T3 unconditional auto-Z) | LEGAL requirement if the pilot goes offline. Plus B10 offline drain awaits live-smoke-9. | M |
| P5 | **Minimal runtime invariant net** — `prro doctor` detect + safe-recovery (invariant_scan exists) | When something breaks live, need detect + safe recovery (not blind DB mutation). "Пилот-adjacent Tier-1" per `project_backlog_operator_recovery`. | M |
| P6 | **Byzantine-DPS decode hardening** — fail-closed on garbage DPS responses | Real DPS "не лежит а отдаёт черти что" (`project_backlog_byzantine_dps_handling`). Pilot must not misbehave on malformed/garbage wire. Minimum: harden the DPS-response decode. | M |
| P0 | **QUALITY_CHARTER §8 → CLAUDE.md + CI rules** (institutionalize verify-self / teeth / ratchet / alphabet-tracks-surface) | Cheap process guard so discipline doesn't erode pre-pilot. | S |

**Pre-pilot is weeks, not months** — half is the already-built live on-ramp + the cheap CI-wire.

---

## POST-PILOT = BUILD THE MOAT (the enterprise product, not "someday")

| # | Item | Payoff |
|---|---|---|
| M1 | **Exhaustive/BFS model-checking** of the bounded machines (shift 9-state, doc-state, offline-session) — prove transition coverage, don't sample | Step-change rigor; this is what we SELL to enterprise |
| M2 | **Full oracle suite** — chain-integrity (AUD-K8-2), MAC-correctness, idempotency-replay-equivalence, offline-budget, audit-log-completeness + **metamorphic** (replay×2 identical, crash-anywhere→recoverable) + **completeness-critic** (model vs spec, not just prod) | Each = a new catchable bug class; the model can err in BOTH directions |
| M3 | **Multi-FN (Tier-3) = FLEET** + X-report / key-rotation / MAC-reseed / crypto-degraded / clock-skew ops | Pilot = 1 FN; fleet = the control-plane phase (core is already per-FN) |
| M4 | **Coverage-guided generation** (libFuzzer-style feedback vs blind proptest) | Steers into unexplored branches |
| M5 | **receipt-fuzz vs XSD** (`project_backlog_receipt_fuzz`) + full input/wire fuzzing | Structural input validation |
| M6 | **Mutation-testing as eternal nightly + ratchet** (kill-rate only up) | Standing erosion measure |

---

## Sequencing / next
Immediate forward step (pre-pilot, cheap, session-proven): **P1 — CI-wire fuzzer required at
meaningful N + nightly large-N**. Then P3 (mutation on shipped surface) in parallel with the
P2 live campaign (gated on key+test-FN). P4/P5/P6 fold into the pre-pilot batch. Post-pilot:
M1→M2→M3 build the moat.

Cross-refs: `TESTING_FRAMEWORK_ROADMAP.md` (FW-1..11 methodology), `ROADMAP_v3_PILOT.md`,
`FUZZER_TIER1_DOSSIER.md`, `FUZZER_TIER2_RAGE_DOSSIER.md`, `QUALITY_CHARTER.md`.
