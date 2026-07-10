# Testing Framework Roadmap — post-RAGE (synthesis 2026-07-10)

**Author:** architect. Synthesizes the "what else can improve the test framework" + "is it enough for confident development" discussion into a phased plan. **Sits AFTER the RAGE fuzzer waves** (`docs/FUZZER_TIER2_RAGE_DOSSIER.md`, W1-W6) in priority; the two are complementary.

## 0. Framing — three distinct things

- **RAGE waves (W1-W6)** = COVERAGE expansion — *what* is fuzzed (alphabet + oracles). Contracted, runs first.
- **This doc, §1** = framework METHODOLOGY — *how mean and how honest* the framework is. Runs after RAGE (cheap items may interleave).
- **This doc, §2** = confidence pieces that NO test framework provides — standing tracks, gate the pilot/fleet not the core.
- `docs/QUALITY_CHARTER.md` is the philosophy; this is the concrete build-out. The standing rule «every feature → check the fuzzer» ([[feedback_fuzzer_tracks_features]]) governs all of it.

## 1. Framework methodology improvements (prioritized)

### Tier 1 — biggest moat per effort
- **FW-1 · Mutation testing (`cargo-mutants`).** Automates + QUANTIFIES teeth across ALL production code: inject thousands of mutations (`<`→`<=`, branch deletion, `+`→`-`), measure the kill-rate; the SURVIVORS are an exact map of oracle-coverage holes. Turns "fuzzer-proven correctness" from a slogan into a tracked metric (charter ratchet). **⚠️ Consider pulling this FORWARD (before/alongside RAGE):** it measures the fuzzer ITSELF, so it directs the RAGE waves at real blind spots instead of guesses. Cheap to stand up (non-required CI job + a baseline run + a survivor triage list feeding RAGE priorities).
- **FW-2 · Golden wire-corpus oracle.** The differential proves `code ≡ our-model-of-DPS`, NOT `code ≡ DPS` — exactly where `-8` lived for a month. A growing corpus of CAPTURED real DPS/WebCheck wire bytes, replayed by every envelope/decode test, closes the model↔reality gap structurally. Elevates the W2/W4 golden mentions into a standing capture harness (`golden/` + a differential against it). This is the WebCheck-differential-harness backlog, promoted to framework.
- **FW-3 · Deterministic concurrency exploration (`loom` / `madsim`) for INV-2.** Single-writer-per-`fiscal_number` is the one large invariant tested by HOPE, not exhaustive interleaving. RAGE W6 runs an N=200 soak (statistical), which cannot PROVE no race. `loom` on the lease/single-writer path explores ALL interleavings — catches a race a soak misses.

### Tier 2
- **FW-4 · Physical fault injection.** Crash ops model LOGICAL windows only; SQLite errors, disk-full, partial writes, kill-at-syscall (fsync) — the durability PHYSICAL axis — are untested. (`fail-rs` / a syscall-level injector.)
- **FW-5 · Real-crypto test lane** wired into the fuzzer/harness — signing is mocked (opaque CMS), so envelope/XML oracles cannot validate against genuine CMS. (`prro_crypto` vectors exist but aren't wired into the differential harness; W2-2 touches this — generalize it.)
- **FW-6 · Flakiness hunter** — a CI job running the suite ×N to surface nondeterminism systematically (the TMPDIR flake was found by accident).
- **FW-7 · Hot-zone coverage as a tracked (NOT gate) metric** (`llvm-cov`) — visibility into what's unexercised in `write_path`/`reconciliation`/`transports`; risk-weighted, ratchet-tracked.

### Tier 3 — heavier / longer
- **FW-8 · State machine as single source of truth.** Encode the 9-state shift / offline-session / doc / node machines as a DATA transition table; generate BOTH the fuzzer model AND a runtime "no illegal transition ever occurs" assertion from the SAME table — kills prose/dup drift.
- **FW-9 · Formal design check (TLA+ / Alloy)** of the shift/offline machine — checks the DESIGN (not the code) for deadlock / unreachable-terminal. Would have caught the superseded-unbounded-hold (PRRO_GATE-eid) at DESIGN time, before code. "Prove the core," not "test it."
- **FW-10 · Operational soak/chaos of the REAL binary** — start the gateway, drive traffic, `kill -9`, restart, assert recovery. Today recovery is unit/integration-tested, not "the actual binary falls and gets up."
- **FW-11 · Perf-regression (`criterion`)** on the hot write-path — a latency regression is an operational risk (RTT budgets) that nothing currently guards.

## 2. Beyond ANY test framework — standing confidence tracks (NOT one-time; gate pilot/fleet, not the core)

- **CT-1 · Live production feedback loop.** The ultimate oracle is reality. The live-DPS campaign (replay + diff + soak) must be ONGOING, not one-shot; real incidents feed the golden corpus (FW-2) and become permanent regression seeds. Tests cannot predict what the first real fleet deployment teaches.
- **CT-2 · Non-core surface coverage as the product grows.** The framework above is CORE-shaped. Ingress adapters (REST/XML-RPC/Maria — malformed-input fuzz), the sidecar-crypto prod path (vs passthrough), provisioning, env-isolation (demo/prod DB), monitoring, licensing, printing — each new surface (esp. the control-plane/fleet features from [[project_product_vision]]) needs its OWN harness under the «feature → fuzzer-check» discipline.
- **CT-3 · Operational safety net.** Staged rollout, monitoring, FAST rollback, human-readable RMR/audit dashboards, incident→test loop. Testing lowers risk BEFORE merge; deployment discipline manages the residual AFTER. A mean fuzzer whose failures no one can triage is half-useful.

## 3. Calibration — the honest bottom line

- **Fiscal-core correctness:** §1 makes risk **known, measured, and managed** — a rare, best-in-market level. **Enough to develop the core with high confidence.**
- **Pilot launch:** §1 **+ CT-1 + CT-3** (staged rollout / monitoring / rollback). Tests do not replace a cautious first deployment.
- **Fleet scale:** core framework ready; every control-plane feature brings its own surface under the same standing discipline (CT-2).
- **There is no zero-risk** for an edge fiscal system with legal exposure. «Known + managed + measured + improving» IS the definition of confidence here — anyone promising zero is lying.

## 4. Sequencing recommendation

1. **FW-1 (mutation) — pull forward, alongside RAGE** (it measures + directs the waves; cheap).
2. RAGE W1-W6 (per the RAGE dossier) — the coverage backbone.
3. **FW-2 (golden corpus)** pairs with RAGE W2; **FW-3 (loom)** pairs with RAGE W6.
4. FW-4..FW-7 (tier 2) as the campaign approaches.
5. FW-8..FW-11 (tier 3) as ongoing hardening; FW-9 (formal) is a high-value one-off for the shift machine specifically.
6. **CT-1 / CT-3 gate the PILOT**, not the core; **CT-2 is continuous** with product growth.

Confidence is a standing lock-step (alphabet-tracks-surface), not a finish line — this roadmap is infrastructure; the confidence comes from keeping it forever in step with the product.
