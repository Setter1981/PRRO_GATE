# Invariant Fuzzer — Phase 1: WebCheck Ground-Truth Oracle (design spec)

**Date:** 2026-07-02
**Status:** DRAFT v1 (architect). To be externally audited before lock (the Phase-2/Phase-3 spec cycle: draft → wide adversarial audit → v2 → lock).
**Scope:** replace the hand-written `RefModel`'s adopted (vacuous) dimensions with derivation rules **read from the WebCheck reference implementation**, plus a WebCheck-DB seed corpus and a differential replay harness. Tests/tools only — no gateway `src/` change (one possible exception gated by CP4).
**Predecessors:** Phase 0 (fuzzer, T0–T7 + §15), Phase 2 (durability, enforced gate), Phase 3 (oracle honesty: U2 #205 · U4 no-op #206 · U1 #208 · U3 #209). This spec picks up the decisions Phase 3 §9 explicitly deferred here: **broad model-fidelity D1–D5** and **O3 canonical-truth**.

---

## §0 Intent & why now

The fuzzer's reference model is hand-written and **demonstrably rots**: four independent model-drift incidents (6 reconciliations during T7; the O1 stacked-SENT artifact; the nightly-0627 `apply_go_online` gap; the U3 signer-attribution fixture gap). Each was found late, by luck or depth. The root cause is structural: the model is a *second hand-written opinion* about the same semantics, so (a) it drifts, and (b) where it can't predict it **adopts** the real DB (D1–D5) — making the differential vacuous exactly where recovery bugs live.

**WebCheck is the way out.** It is the government-certified, 4-years-in-production reference PRRO — *legally correct by definition* for the flows it implements. Its complete decompiled C# source is **already in-repo** (`docs/webcheck_reverse/`, ILSpy 8.1, verified readable), and a read-only DB exporter (`scripts/export_webcheck_samples.py` + tests) is ready. Deriving the model's rules **from the reference source** (instead of inventing them) and replaying **real field data** (instead of only random sequences) converts the differential from "us vs our own opinion" into "us vs the deployed, certified reality".

## §1 Goals / Non-goals

**Goals**
- **G1 (derive, don't adopt)** — close D1–D5: every adopted dimension of `RefModel` either becomes a *derived prediction justified by a cited WebCheck source rule* or a *documented, bounded deferral*.
- **G2 (real seed corpus)** — a committed, **sanitized** corpus of field-realistic op sequences exported from operator WebCheck DBs, replayed as directed fixtures.
- **G3 (interop differential)** — a replay harness: drive our write-path with WebCheck-sample inputs and diff the ledgers (`lnd` sequence, doc states, `unsigned_xml_sha256` chain, offline-code consumption) — including the **O3 canonical-truth** comparison (our canonical XML/hash vs WebCheck's `StringXML` construction for the same input).

**Non-goals**
- **N1 — the live-DPS soak campaign.** "WebCheck replay+diff+soak" against the *live DPS* is a separate validation axis (interop with the tax authority, not model honesty). This spec's replay is **offline** (DB/file-based). The soak campaign reuses G2/G3 artifacts but is its own effort.
- **N2 — porting WebCheck behavior into gateway `src/`.** WebCheck is the *oracle*, not the implementation. Any behavioral gap found is triaged as a finding (CP3-style), never silently "fixed to match".
- **N3 — RETURN/Z-report alphabet expansion.** Uses this infrastructure later; separate tranche.

## §2 Verified baseline (recon 2026-07-02, in-repo)

- **Decompiled source (full, verified):** `docs/webcheck_reverse/WebCheckMain/WebCheck/` — `ClassFiscal.cs` (ProcessCheck/ReportZ, 3832 lines), `SQLlite.cs` (all DB ops, 2678 lines), `All.cs` (`SubstitutePreviousMAC` :1481, `MacTempOld` :1505), `Offlin.cs`, `NumbersOfflineUse.cs`, `SendingOfflineChecks.cs`, `StringXML.cs`, `CreateDB.cs` (DDL); plus `TaxGrpc/`, analysis in `WEBCHECK_ANALYSIS.md`.
- **Export tooling (verified):** `scripts/export_webcheck_samples.py` (+ `tests/test_export_webcheck_samples.py`) — read-only, snapshot-copies a WebCheck `.db`, joins `ksef`/CHECKHEAD/CHECKBODY/CHECKPAY/CHECKTAX/SHIFTS, emits per-row JSON by category (sell_plain / offline / return / z_report / service_in / service_out).
- **Schema mapping (verified from source):** `ksef.localchecknumber`→`lnd`; **`ksef.MAC` = SHA256 of THAT row's XML = our `unsigned_xml_sha256`** (WebCheck stores NO `previous_hash` — the chain is recomputed via DPS `lastChk` + in-memory `MacTempOld`); `ksef.offline` int-encodes state (0 online / 1 pending-sync / 2 synced / 3 session-open marker / −1 cancelled); `SHIFTS.LastLocalCheckNumber`↔`next_lnd−1`; `Sessions`↔`offline_sessions`; `fns`↔`offline_codes` (`used IS NULL` = available; `CountNubmers()` ≡ `codes_issued − codes_consumed`).
- **MAC chain rule (verified):** `SubstitutePreviousMAC` replaces the `mmmaaaccc` placeholder with the SHA256 of the *previous* doc's decoded XML (obtained via `lastChk`); **offline mode keeps the placeholder literal** — the offline path authenticates via the offline UUID instead. Matches our lane-split seed semantics (online@ACK / offline@issuance).
- **RefModel surface to replace (verified):** state `{seed, next_lnd, shift_state, mode, session, codes_issued, codes_consumed, docs}`; harness calls `apply`, `predict_crash_completed_sell`, `resync_from_db` (adopts docs/mode/shift/next_lnd/seed-presence/session/codes → D1/D2), `resync_preconditions_from_db` (adopts mode/shift/session per-op → D2); `BadHashPrev`→Fault (D4); exotic-drain→Fault (D5); shared `OFFLINE_ISSUED_STATES` const (D3).
- **ABSENT (external dependency):** live/backup WebCheck `.db` files — **the operator must provide dumps**; nothing in-repo.

## §3 Units

### U0 — Ground-truth reference doc (read-only grounding)

Read `CreateDB.cs` in full + the query sites; produce `docs/webcheck_reverse/WEBCHECK_GROUND_TRUTH.md`:
- exact DDL for `ksef` / `SHIFTS` / `Sessions` / `fns` (column types, constraints);
- the **`offline`-encoding translation table** (int codes → our state model), each row justified by a cited query/branch in the C# source;
- the MAC-chain derivation rule (SubstitutePreviousMAC / MacTempOld / lastChk) as a normative statement, incl. the restart-mid-shift caveat (in-memory cache vs `ksef` column — **the `ksef.MAC` column is canonical for replay**);
- the `fns` ordering question resolved (is there an ordinal matching our `code_lnd`?).
Every later unit cites THIS doc, not raw impressions.

### U1 — Derive-don't-adopt (D1–D5 closure in `RefModel`) — tests-only

The core. Each vacuity point becomes a derivation **with a WebCheck source citation** (the "don't build the hand model twice" resolution: we don't invent rules — we read them from the certified reference):
- **D1** — `resync_from_db` stops adopting `next_lnd`: derive `next_lnd := max(adopted lnds) + 1` and **assert** the DB value equals it (WebCheck rule: `ReturnLocalCheckNumberShift` = max(localchecknumber)+1 per shift).
- **D2** — predict-then-assert mode/shift for the ops the model understands (go_online, drain-reject→RMR, the U3 mode-forcing intents) BEFORE `resync_preconditions_from_db`; resync only what remains genuinely unmodeled. (WebCheck's shift lifecycle: `SHIFTS.DATEEND IS NULL` = open; DocType 109/110 = go-offline/go-online markers.)
- **D3** — fork `OFFLINE_ISSUED_STATES` into a model-local literal + `debug_assert` equality with the prod const.
- **D4** — predict the `BadHashPrev` single-shot-stub terminal (or minimally: apply the no-resend bound to it).
- **D5** — promote the deterministic exotic-drain scripts (Superseded→all ERROR_RETRYABLE; NotFound-hold→SENT) to predicted `Mutated`; the Fault bucket shrinks to genuinely-nondeterministic MAC-recovery.
- **RED-first + paired negative teeth per D-closure** (Phase-3 U2 discipline: a false-positive is a merge-blocker on the enforced gate).

### U2 — WebCheck seed corpus (export → sanitize → commit) — ⚠ DATA-SENSITIVE (CP1)

- Operator provides WebCheck `.db` dump(s) (backup copies; **never committed raw** — production fiscal data: real receipts, tax numbers, totals).
- Run `export_webcheck_samples.py` locally; **sanitization step** (new small tool or script extension): strip/replace payloads (items, sums→synthetic, tax numbers→fixture TINs), KEEP the structural skeleton (`localchecknumber` sequence, `offline` codes, doc types, MAC-chain shape, shift boundaries, fns consumption pattern).
- Convert sanitized samples into committed **directed replay fixtures**: op sequences with expected `(lnd, state-class, chain-shape, code-consumption)` — a golden corpus in `rust/prro/tests/webcheck_corpus/` replayed by a directed harness test.
- **CP1 (hard gate):** the sanitization design is reviewed by the architect BEFORE any real-data-derived file is committed; a raw or under-sanitized dump in git history is unrecoverable.

### U3 — Differential replay harness (G3 + O3 canonical-truth) — tests-only

- A harness that takes a corpus fixture, drives OUR write-path (real seams, ScriptedDps with the fixture's DPS-response shapes), and diffs against the fixture's WebCheck-expected structure: `lnd` sequence, state classes, `unsigned_xml_sha256` **chain recomputed by ordering** (replay-canonical, per U0 — WebCheck stores no previous_hash), offline-code consumption count.
- **O3 canonical-truth slice:** for a fixture input, compare our canonical XML construction (hash) with WebCheck's `StringXML`-derived expectation for the same logical document. Divergence = either our canonicaliser bug or a documented format difference (triage, don't auto-bless). This closes the Phase-3 §9 O3 deferral **without a `src` seam** if the comparison is expressible over persisted artifacts; if a `src` seam turns out to be required → **CP4**.
- Any WebCheck-vs-us behavioral divergence = a **finding** (CP3): triage with a proven repro; prod fix (if we're wrong) or documented intentional difference (if the M3b design deliberately diverges — e.g. our richer state machine). NEVER silently adjust the oracle to pass.

## §4 Sequencing & risks

**Order:** **U0 → U1** (both fully unblocked — everything needed is in-repo) → **U2 → U3** (blocked on operator dumps; U3 needs U2's corpus format, its harness skeleton can start against the golden adapter fixtures).

**Risks:**
- **R1 (U2, highest):** fiscal-data leakage into git. Mitigated by CP1 hard gate + sanitize-before-commit + raw dumps live outside the repo tree.
- **R2 (U1):** a derived rule read WRONG from the C# source → an over-strict oracle blocking merges. Mitigation: paired negative teeth + every rule carries its source citation (reviewable) + full capstone re-runs.
- **R3 (U3):** intentional design differences (our 9-state shift machine, Pattern C, Aborted terminal) vs WebCheck's simpler model producing noise diffs. Mitigation: the diff compares *invariant-level* observables (lnd monotonicity, chain shape, code counts), not row-by-row equality; a documented mapping of intentional differences is part of U3's deliverable.
- **R4:** WebCheck itself has bugs (certified ≠ perfect). A divergence where WE are right and WebCheck is wrong is possible — triage keeps that door open (CP3 wording: "finding", not "our bug").

## §5 Acceptance

- **A0 (U0):** `WEBCHECK_GROUND_TRUTH.md` exists; DDL + offline-encoding table + MAC rule each carry C# citations; the `fns`-ordering question answered.
- **A1 (U1):** zero remaining silent adoptions for D1/D3; D2/D4/D5 either derived or *bounded* with an explicit comment citing why; each closure has paired positive+negative teeth; full suite green (no capstone false-positive at N=256 and at an elevated-N probe).
- **A2 (U2):** a committed sanitized corpus ≥ covering {online sell run, offline session with drain, Z-boundary} sequences; CP1 sign-off recorded; raw dumps demonstrably absent from git history.
- **A3 (U3):** the replay harness reproduces the corpus green on main; a deliberately injected divergence (mutated expected lnd/chain) is caught (teeth); the O3 canonical comparison runs on at least the sell fixtures; intentional-differences map documented.

## §6 Cross-cutting discipline

Tests/tools only (gateway `src/` untouched; CP4 escalates any exception). Isolated worktrees. Each unit its own PR through the enforced gnu gate. Local gate: `fmt --check` + `clippy -D warnings` + full `nextest` (+ elevated-N probe for U1). Delivery per unit in the 7-item format. External wide audit of THIS spec before lock (the proven cycle).

## §7 Checkpoints (stop-and-ask)

- **CP1 (U2):** sanitization design review BEFORE any real-data-derived commit. Hard gate.
- **CP2 (U1):** a derived rule contradicts observed gateway behavior on the existing suite → do not force; triage (our bug vs mis-read rule vs intentional difference).
- **CP3 (U3):** WebCheck-vs-us divergence → finding with repro; separate triage/fix; never silently re-bless.
- **CP4:** any unit appearing to need a gateway `src/` change (e.g. an O3 canonicaliser seam) → STOP, separate contract.
- **CP5 (U2/U3):** operator dumps unavailable → U0+U1 proceed alone; U2/U3 wait (do not fake a corpus).

## §8 Locked / deferred

- **Live-DPS soak campaign** — separate axis (N1), reuses G2/G3 artifacts.
- **RETURN/Z alphabet** — after this infrastructure lands.
- **`Crash(Finalize)` src kill-point seam** — still deferred (Phase-3 CP5); U3's replay may partially cover the finalize window from the outside.

## References

- Recon inventory (2026-07-02, in-session agent report) — the §2 baseline.
- `docs/webcheck_reverse/` — decompiled source; `WEBCHECK_ANALYSIS.md`.
- `scripts/export_webcheck_samples.py` + tests.
- `docs/superpowers/plans/2026-06-17-optimal-roadmap.md` §4.1 (the flagship intent).
- `docs/superpowers/audits/2026-06-16-invariant-fuzzer-dryrun-findings.md` — D1–D5 verbatim.
- `docs/superpowers/specs/2026-06-17-fuzzer-phase3-oracle-honesty-design.md` §9 — the deferrals this spec picks up.
