# Quality Charter — Multi-Protocol PRRO Gateway

**Status:** draft charter (2026-07-08). To be distilled into enforceable project rules (§8 → `CLAUDE.md`, CI gates, workflow).

**The bar.** This is a fiscal edge system with legal and operational risk across a fleet of registers. We are not building "a gateway with tests" — we are building **provable correctness** as the product. Testing here is the moat, not a QA phase. Everything below serves one question an enterprise buyer actually asks: *"How do you know it won't lose or mis-issue a receipt on 200 registers?"* The answer must be the language of **proof**, not coverage percentages.

---

## 1. Principle: proof over coverage; invariants first

Coverage measures which lines executed, not which **properties hold**. For this domain the load-bearing artifacts are **invariants**, and tests target them directly:

- no document rests in a non-terminal state (`PREPARED`/`SIGNED`/`ENCRYPTED`) at a quiescent boundary (bug #192);
- one `fiscal_number` = one single-writer write-path (INV-2);
- idempotency is mandatory (INV-4);
- offline respects time and code limits (INV-5);
- the online chain seed advances atomically at the `Sending→Sent` CAS (advance-at-SEND / D2);
- recovery never silently violates a state transition (INV-8).

The classic test pyramid inverts: the foundation is **machine-checked invariants of the core**, not example unit tests.

## 2. The ladder of confidence

Each rung is strictly stronger than the one below. "Above market" means living on rungs 3–5 for hot zones.

1. **Example-based** (unit/integration) — "I imagined case X." Weakest: encodes the author's assumptions (and their bugs).
2. **Property-based** — "for ANY input, property P holds." The machine generates cases; you own the property.
3. **Model-based / stateful** — a reference model + a differential oracle exercised on the real seams (our invariant fuzzer). Finds the rare interleavings where real bugs live.
4. **Differential against a REFERENCE** — not assertions you wrote, but "does it match the 4-year-prod WebCheck / captured-DPS golden byte-for-byte?" An oracle that does not share your misconceptions.
5. **Formal / metamorphic properties** — relations that hold independent of path: `online ≡ offline outcome`, `N sells = Z total`, `RETURN reverses SELL`.

## 3. Adversarial generation — orthogonal axes ("the злюки")

Humans test what they imagined; bugs live where they didn't. So we field **independent adversaries on different axes** — each multiplies coverage rather than adding to it. Breadth of adversary = breadth of proof = strength of moat.

| Axis | Adversary | What it hunts | Status |
|---|---|---|---|
| **State** | model-based invariant fuzzer | rare op-interleavings, ledger-delta drift | HAVE (extending: shift/Z/RMR — see [[fuzzer alphabet gaps]]) |
| **Input** | byzantine-DPS / wire-decode fuzzer | malformed/hostile DPS replies crashing or mis-driving the machine | ROADMAP (top pick) |
| **Reference** | differential replay vs WebCheck / DPS golden | subtle wire-format divergence the self-model can't see | ROADMAP (max campaign confidence) |
| **Meta** | mutation testing (`cargo-mutants`) | weak / tautological / rotted tests | ROADMAP (cheap, brutal) |
| **Concurrency** | race fuzzer (deterministic interleaving / loom) | single-writer-per-FN under concurrent ingress (fleet) | ROADMAP (Tier-2) |
| **Time** | clock/limit fuzzer | 24h shift, 168h offline, cert-window with skewed/backwards clock | ROADMAP (fiscal gaps #1/#2) |
| **Ingress input** | receipt/XSD fuzz | receipts DPS would reject + inputs we should refuse (50k, tax) | ROADMAP |

## 4. Recovery and runtime are first-class

In ordinary projects the happy path is tested more than recovery. In a responsible system it is the **reverse**: recovery paths run rarely but under the worst conditions (crash, torn write, partial commit, boot-resume). "Graceful shutdown > finishing fast" must carry test coverage *higher* than the sell path.

Testing does not end at merge. In production the invariant is **monitored**: `invariant_scan`, continuous reconciliation, `doctor --repair`, stuck-document detectors, `audit_log`. For a fleet, fiscal correctness is a runtime property with always-on checks — tests that never sleep.

## 5. Disciplines against erosion (where long-lived projects quietly fail)

Quality decays invisibly. These disciplines keep the moat alive over years:

- **Bug ratchet:** every bug → a failing test FIRST (RED-first TDD), then the fix. The suite becomes the accumulated memory of every way the system can break. One-way ratchet — a fixed bug never returns.
- **Teeth verification (meta):** a test you never watched fail proves nothing. Mutation testing + deliberate revert-canaries prove tests actually catch. Guard against **tautological tests** — a model that mirrors the implementation instead of predicting independently loses its teeth (the exact hazard guarded in the B10 fuzzer fix: "the model predicts; the oracle compares; they must be able to disagree").
- **Alphabet tracks the product surface:** a new `Op` / state / edge enters the adversarial alphabet AT THE SAME TIME it enters the code — never "later." Otherwise coverage erodes as fleet features accrete. This is a workflow discipline, not a one-time project.
- **Gates are REQUIRED, not advisory:** quality that can be bypassed (non-required CI) gets bypassed. The fuzzer and the suite must be REQUIRED merge gates so the moat is load-bearing. (Current gap H5: fuzzer lives in non-required CI — a red regression can be merged.)
- **Determinism / reproducibility:** seed-persist, recorded interleavings, replay. A found failure must reproduce on demand — the difference between "found a bug" and "fixed it forever." A flaky test is worse than none.
- **Test behavior/invariants, not structure:** over years code churns; only the property contract is stable. Tests pinned to invariants survive refactors; tests pinned to implementation detail rot.

## 6. Risk-weighted rigor

Testing is not free; uniform rigor is waste. Tier by blast radius:

- **Hot zones** (`services/write_path`, `services/reconciliation`, `repositories`, `transports`, `adapters`, state machines, migrations, runtime startup/shutdown): model-based + differential + adversarial + recovery-torture. Mandatory RED-first for any change.
- **Warm zones** (ingress adapters, config, admin): property + integration.
- **Cold zones** (UI, docs tooling): example tests.

Framing: **cost-of-test ≪ cost-of-failure.** For us cost-of-failure is legal + operational across the fleet — which justifies heavy investment in the core and discipline against over-testing the periphery.

## 7. Where we stand (grounding, not aspiration)

- **Have:** model-based invariant fuzzer with a proven differential oracle (caught #192 over-ruling, the P1 boot-resume twin, and the B10 lazy-9 ledger-delta drift — three real catches before merge); RED-first TDD as house style; a written invariant set (`LEGAL_INVARIANTS.md`, `CLAUDE.md` frozen invariants); crash/reboot ops in the fuzzer; live smoke harness against the DPS test cabinet.
- **Roadmap (moat-completion):** fuzzer Tier-1 (shift/Z/RMR ops + RMR-as-oracle + advance-at-SEND fidelity) before campaign; the orthogonal adversaries of §3 (byzantine-decode first, then differential-replay, then mutation testing); fuzzer as a required gate; multi-FN concurrency for the fleet phase.

## 8. From charter to enforceable rules (the distillation targets)

When we "implement into the rules," these become concrete and checkable:

1. **RED-first is mandatory in hot zones** — no production diff in a hot zone without a preceding failing test in the same change. (→ `CLAUDE.md` workflow + review checklist.)
2. **New surface enters the fuzzer alphabet in the same PR** — a new `Op`/state/edge without a matching fuzzer-alphabet extension is an incomplete change. (→ review checklist item.)
3. **Fuzzer + full suite are REQUIRED merge gates** — wire the fuzzer lane into required CI (close H5). (→ CI config.)
4. **Teeth are proven, not assumed** — every new differential/model assertion ships with a revert-canary demonstrating it goes RED. Periodic mutation-testing run on hot zones. (→ review checklist + nightly CI.)
5. **Recovery coverage ≥ happy-path coverage in hot zones** — a change to a recovery path requires a crash-injection test. (→ review checklist.)
6. **Determinism** — fuzzer seeds persist; a found failure lands as a deterministic regression test before the fix. (→ CI + TDD rule.)
7. **Risk-tiered rigor is explicit** — the PR states its blast-radius tier and the matching rigor applied. (→ delivery format.)
8. **Runtime invariants are shipped, not just tested** — hot-zone changes that alter durable state consider the corresponding `invariant_scan` / reconciliation check. (→ review checklist.)

---

*This charter is the "why". The rules (§8) are the "must". Keep them in sync: when a rule changes, update the charter; when the charter's bar rises, tighten the rules.*
