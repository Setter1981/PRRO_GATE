# Invariant Fuzzer — Phase 1: WebCheck Ground-Truth Oracle (design spec)

**Date:** 2026-07-02 (v2: 2026-07-03)
**Status:** **REVISED v2 (post external-audit — v1 No-Go accepted in full, 12 findings; HIGH#1/#4 architect-verified against the C# source).** To be re-checked, then locked.
**Scope:** (a) re-ground the hand model's adopted dimensions in **our own normative pins** with mechanical validation; (b) a **sanitized, hash-recomputed** field-data corpus from operator WebCheck DBs; (c) a directed **differential replay harness**. Tests/tools only (CP4 gates exceptions).
**Predecessors:** Phases 0/2/3 complete. Picks up Phase-3 §9 deferrals (broad D1–D5; O3 canonical-truth — the latter now explicitly *narrowed*, see U3/N4).

> **v2 changelog — the central reframe (audit HIGH#1/#2):** WebCheck's epistemic contribution is **NOT a rule source for our model** — key semantics do not transfer: its `localchecknumber` is **per-shift** (`CREATE TRIGGER checkcount` increments `SHIFTS.LastLocalCheckNumber`; reset `'0'` at shift insert — verified in `CreateDB.cs`) while our `lnd` is **per-FN monotonic** (`ux_fd_fn_lnd`, ADR-M3-A1); its shift model is 2-state vs our 9-state; its offline MAC flow is its own. WebCheck's real contributions: **(1)** a field-data corpus of real operation *shapes* (U2); **(2)** invariant-level differential replay (U3); **(3)** selected lane semantics *only where U0 shows transfer*. D1–D5 closures are re-grounded in **our own pinned invariants**. Also: §2 corrected (offline encoding — insert `'2'`, send-select `'2'`, post-send `'1'`, HIGH#4; `fns.used` semantics + no ordering column, HIGH#5; per-lane MAC flows, HIGH#3; `DATEEND='NULL'` **string** sentinel, LOW#11; approximate line-counts dropped); U0 promoted to the load-bearing unit; U2 gets concrete **sanitization invariants** + mandatory **hash recomputation over synthetic bytes** (HIGH#3-C1) + a mechanical corpus gate (MED#7); U3's conversion path pinned (HIGH#6) + O3 narrowed (MED#10); A1 made mechanically falsifiable via an adoption-lint (MED#9); A2's Z-replay overclaim dropped (MED#8); "legally correct by definition" wording removed (LOW#12).

---

## §0 Intent & why now

The fuzzer's hand-written `RefModel` demonstrably rots (4 drift incidents) and **adopts** the real DB where it cannot predict (D1–D5) — making the differential vacuous exactly where recovery bugs live. Phase 3 deliberately deferred the broad fix here.

**WebCheck** — the certified, 4-years-deployed **compatibility reference** whose complete decompiled C# source is in-repo — tells us what real operation sequences look like and lets us diff behavior at the invariant level. It is **not** an oracle of our internal semantics (our state machine is deliberately richer), and it is **not infallible** — a divergence may mean *we* are right (CP3 keeps that door open, consistently).

## §1 Goals / Non-goals

**Goals**
- **G1 (derive, don't adopt — grounded in OUR pins)** — every adopted `RefModel` dimension becomes a derived prediction justified by a cited **normative pin of ours** (schema constraint / ADR / state-machine spec) — or a documented, bounded deferral. Field validation of the derived rules comes from G2/G3 — that is WebCheck's role in D-closure.
- **G2 (real corpus, sanitized + recomputed)** — a committed corpus of field-realistic sequence *shapes* exported from operator WebCheck DBs, with **all identifying material stripped** and **every hash/chain value recomputed over synthetic bytes**.
- **G3 (invariant-level differential replay)** — a directed harness replaying corpus fixtures through our real write-path, diffing *invariant-level observables* (per-FN lnd monotonicity, state classes, chain consistency over our own recomputed hashes, code-consumption counts) against fixture expectations.

**Non-goals**
- **N1** — the live-DPS soak campaign (separate interop axis; reuses G2/G3 artifacts; U3 must not quietly depend on live DPS).
- **N2** — porting WebCheck behavior into gateway `src/`; a behavioral gap = triaged finding (CP3), never a silent re-blessing of either side.
- **N3** — RETURN/Z alphabet expansion (later tranche). **Z is exported by the tooling but NOT replayed in Phase 1** — there is no Z op in the alphabet and inline fail-closes Z pre-fiscal (audit MED#8).
- **N4** — byte-level canonical-XML/hash equality vs WebCheck (`StringXML`): requires a translation/normalization layer (encoding, attribute order, MAC placeholder, offline-ID behavior) that is NOT scoped in Phase 1 (audit MED#10). O3 here = *normalized structural comparison* only.

## §2 Verified baseline (v2 — corrected per audit; U0 re-cites every row)

- **Decompiled source (verified present):** `docs/webcheck_reverse/WebCheckMain/WebCheck/` — `ClassFiscal.cs`, `SQLlite.cs`, `All.cs` (`SubstitutePreviousMAC`, `MacTempOld`), `CreateDB.cs`, `NumbersOfflineUse.cs`, `StringXML.cs`, `Offlin.cs`, `SendingOfflineChecks.cs`. No approximate line-count "authority" in this spec — U0 regenerates anchors mechanically (audit LOW#11).
- **lnd semantics (HIGH#1, architect-verified):** WebCheck `localchecknumber` is **per-shift** — `CREATE TRIGGER checkcount AFTER INSERT ON ksef … SET LastLocalCheckNumber = LastLocalCheckNumber+1` (`CreateDB.cs`), counter seeded `'0'` at shift insert. Our `lnd` is **per-FN monotonic** — `CREATE UNIQUE INDEX ux_fd_fn_lnd ON fiscal_documents(fiscal_number, lnd)` (`001_baseline.sql`), allocator `node_state.next_lnd` (ADR-M3-A1). **The semantics do NOT transfer**; any corpus use requires an explicit mechanical translation (exported row order → synthetic per-FN lnd) defined in U0.
- **Offline doc encoding (HIGH#4, architect-verified):** offline docs are **inserted with `offline='2'`** (`SQLlite.cs` ksef INSERT); send-selection `WHERE offline='2'` (:1714) and backlog count likewise (:1278); a successful send updates to `'1'` (`SendingOfflineChecks.cs`); `'3'→'1'` transitions exist (`SQLlite.cs:1383`); `-1` = cancelled. The full line-cited lifecycle table is **U0's deliverable** — nothing may depend on these codes before it exists.
- **`fns` offline numbers (HIGH#5):** `used IS NULL` = available (`NumbersOfflineUse.cs`); consumption sets `used = datetime(...)` via **triggers on ksef insert/update** (`CreateDB.cs`); `used='2'` is a **bulk-invalidation marker** before loading fresh numbers — NOT normal consumption; selection has **no ORDER BY** and the table has **no ordering column** → WebCheck codes are modeled as availability/consumption **counts only**; no `code_lnd` mapping exists.
- **MAC/hash flows (HIGH#3):** *online*: `ksef.MAC` = SHA256 of that row's own XML; previous-MAC via DPS `lastChk` (+ in-memory `MacTempOld`; restart caveat); no stored `previous_hash`. *offline*: `SaveXMLcheckOffline` reads the **latest local `ksef.mac`** (`LastMac`), injects it into a transformed XML, hashes THAT (`MakCheck`) — the offline MAC chains off **local** state, not DPS. *drain*: `SendingOfflineChecks` **rewrites `checksigned`** with the DPS `lastChk` data **without recomputing the stored `mac`**. Consequence: `ksef.MAC` is NOT uniformly "our `unsigned_xml_sha256`" across lanes — the per-lane mapping is U0's table.
- **Shift sentinel (LOW#11):** WebCheck's open shift is `DATEEND = 'NULL'` — the **string literal**, not SQL NULL. A data-quality trap for any exporter/mapping.
- **Export tooling (verified):** `scripts/export_webcheck_samples.py` (+tests) — snapshot-copy, query-only, joins, categories. Its own header warns the output can contain fiscal/org/customer-sensitive data — the **sanitizer is a separate mandatory stage**, not this exporter (audit MED#7).
- **RefModel adoption sites (verified):** `resync_from_db` (full recovery state incl. `next_lnd`), `resync_preconditions_from_db` (mode/shift/session per-op), `BadHashPrev`→Fault, exotic-drain→Fault, shared `OFFLINE_ISSUED_STATES` const.
- **ABSENT:** operator `.db` dumps (read-only git scan confirms no tracked/historical `*.db`/`*.sqlite*` — U2 makes keeping it that way mechanical).

## §3 Units

### U0 — Line-cited lifecycle tables (the load-bearing grounding unit)

Deliverable `docs/webcheck_reverse/WEBCHECK_GROUND_TRUTH.md`, every row carrying a C# citation:
- exact DDL for `ksef`/`SHIFTS`/`Sessions`/`fns` from `CreateDB.cs`, **including the `checkcount` trigger and the `fns` `used=datetime` triggers**;
- the **`offline` lifecycle table** — resolve every observed code (`'2'` insert → `'2'` send-select → `'1'` post-send; `'3'` transitional; `-1` cancelled) with its query site;
- the **per-lane MAC flow** (online own-hash + lastChk-prev; offline local-`LastMac`-chained transformed-XML hash; drain `checksigned` rewrite without `mac` recompute) as a normative statement, with the `MacTempOld` restart caveat and the `DATEEND='NULL'` sentinel;
- the **lnd translation rule** — WebCheck per-shift `localchecknumber` → synthetic per-FN `lnd` (a mechanical mapping over exported row order);
- `fns`: availability/consumption semantics; explicit "no ordering column exists".
**Rule: nothing in U1–U3 may cite a WebCheck behavior absent from this doc.**

### U1 — Derive-don't-adopt, grounded in OUR normative pins — tests-only

**Resolution of the Phase-3 tension (audit HIGH#2):** Phase 3 forbade *speculative* hand-modeling. U1 does not speculate — each closure derives from an **already-pinned invariant of ours**, so the model finally asserts what our normative docs already promise. WebCheck's role in U1 is **validation substrate** (U2/U3 replay field-shaped sequences against these rules) — NOT rule source; none of D1–D5 transfers fully (per-closure verdicts below, per the audit's explicit B2 answer). Where a rule is ours, it is **labeled ours**.
- **D1** — `next_lnd := max(adopted lnds)+1`, asserted equal to the DB value. **Grounding: ADR-M3-A1 + `ux_fd_fn_lnd`** (per-FN monotonic, fail-closed). *Not WebCheck* (HIGH#1).
- **D2** — predict-then-assert mode/shift for ops the model understands, before precondition-resync. **Grounding: our M3b 9-state shift spec + node-mode machine.** WebCheck grounds only trivial open/closed existence.
- **D3** — fork `OFFLINE_ISSUED_STATES` into a model literal + `debug_assert` equality. **Grounding: ours** (anti-shared-const).
- **D4** — `BadHashPrev`: apply the no-resend bound (minimum) or predict the single-shot-stub terminal. **Grounding: our write-path MAC-recovery semantics (W10.4 budget).**
- **D5** — promote the deterministic exotic-drain scripts (Superseded→all ERROR_RETRYABLE; NotFound-hold→SENT) to predicted `Mutated`; **MAC-recovery labeled genuinely deferred**. **Grounding: our recovery semantics.**
- RED-first + paired negative teeth per closure (false-positive = merge-blocker on the enforced gate).
- **A1 is mechanical (audit MED#9): an adoption-lint** — every DB read in `model.rs` is classified `{seed-fixture | fault-deferred | precondition-only | FORBIDDEN}` in a tagged registry, with a test asserting the classification is exhaustive; A1 passes iff FORBIDDEN is empty and every closure's teeth pair is green.

### U2 — Sanitized, hash-recomputed corpus — ⚠ DATA-SENSITIVE (CP1 hard gate + mechanical gate)

- **Raw dumps:** operator-provided; **encrypted at rest, outside the repo tree (never transiting it), never committed, destroyed after export** (audit C3/C6).
- **Sanitization INVARIANTS (HIGH#3-C1 + MED#7)** — the committed corpus must provably contain NONE of: real FN/TIN/company/operator/customer identifiers; item names; raw WebCheck XML; real timestamps (synthetic time buckets only); real totals (synthetic amounts); real offline UUIDs; **any real `ksef.MAC` or other real hash** (hashes fingerprint real receipts); DPS blobs (`signedanswerfromficscal`).
- **Hash recomputation (locked, audit's C1 answer):** the corpus keeps only the **shape** — op-type sequence, shift boundaries, offline-code consumption pattern, the U0 per-shift→per-FN lnd translation. Fixture payloads are synthetic; **every expected hash/chain value is recomputed over the synthetic bytes**. Corpus CI asserts `expected_hash == sha256(fixture_payload)` for every fixture — which also keeps the O3 recompute-integrity check meaningful.
- **Mechanical gate (audit C5):** a corpus-dir scanner (pre-commit + CI step) enforcing the invariants (pattern checks: TIN shapes, UUID shapes, timestamp ranges, non-synthetic markers). CP1 human review sits ON TOP of the scanner, not instead of it.
- **CI leakage (audit C4):** fixtures are synthetic by construction → failure logs/artifacts are safe *because* the scanner enforces it.

### U3 — Directed differential replay harness — tests-only

- **Conversion path (pinned, HIGH#6):** exporter JSON → **golden canonical fixtures** (`CanonicalFiscalCommand` JSON — the `golden/webcheck_*` precedent) produced by the sanitizer stage → a NEW directed replay harness driving **`inline::run` directly** (not the Op-fuzzer), with ScriptedDps loaded from **abstract response classes** (`Ack`/`Reject`/`NotFound`/…) derived from each sample's outcome category — **never raw WebCheck response blobs** (format mismatch with `CheckAck`).
- **Diffed observables (invariant-level):** per-FN lnd monotonicity/no-gap vs the fixture's translated sequence; state classes; chain consistency **over our own recomputed hashes**; offline-code consumption counts; issued-set membership.
- **O3 slice (narrowed, MED#10):** *normalized structural comparison* of our canonical output vs the fixture's expected structure (field-level after normalization). **No byte/hash-equality claim vs WebCheck XML in Phase 1** (needs the N4 translation layer — explicitly deferred).
- Divergence = **finding** (CP3): triage with a repro — our bug / a mis-derived U1 rule / an intentional difference. The **intentional-differences map** (our 9-state shift, Pattern C, `Aborted` terminal, per-FN lnd) is a U3 deliverable.

## §4 Sequencing & risks

**Order:** **U0 → U1** (fully unblocked in-repo) → **U2 → U3** (blocked on operator dumps; U3's harness skeleton can start against `golden/webcheck_*`).

**Risks:**
- **R1** fiscal-data leakage → CP1 + invariants + mechanical scanner (U2).
- **R2** a mis-derived U1 rule → over-strict oracle blocking merges → paired negative teeth + reviewable pin citations + full-capstone re-runs.
- **R3** intentional-difference noise in U3 → invariant-level diff + the differences map.
- **R4** WebCheck's own bugs → CP3 keeps "we may be right".
- **R5 (audit E):** pre-corpus U1 validation is partially circular (validated against our own fuzzer + pins) — U1's Delivery must state this honestly ("field validation lands with U2/U3"), and U2/U3 close it.

## §5 Acceptance

- **A0:** `WEBCHECK_GROUND_TRUTH.md` exists; DDL + offline-lifecycle + per-lane MAC + lnd-translation + `fns` tables each carry C# citations; anchors machine-regenerated (no approximate counts).
- **A1:** adoption-lint registry exhaustive and FORBIDDEN empty; D1/D3 derived; D2/D4/D5 derived-or-bounded, each with an our-pin citation; paired teeth per closure; full suite + elevated-N probe green.
- **A2:** committed corpus covers {online sell run, offline session + drain} — **Z: exported, NOT replayed** (MED#8); scanner green over the corpus; every fixture `expected_hash == sha256(fixture_payload)`; CP1 sign-off recorded; no raw-dump traces in git history (mechanical check).
- **A3:** replay harness green on main over the corpus; an injected divergence (mutated expected lnd/state/chain) is caught (teeth); the normalized-structural O3 comparison runs on the sell fixtures; the intentional-differences map documented.

## §6 Cross-cutting discipline

Tests/tools only (CP4). Isolated worktrees; each unit its own PR through the enforced gnu gate; local gate `fmt --check` + `clippy -D warnings` + full `nextest`. Delivery per unit in the 7-item format.

## §7 Checkpoints

- **CP1 (U2, hard):** sanitization design + scanner reviewed BEFORE any real-data-derived commit.
- **CP2 (U1):** a derived rule contradicts observed gateway behavior → triage (our bug vs mis-derivation vs intentional); don't force.
- **CP3 (U3):** WebCheck-vs-us divergence → finding with repro; never silently re-bless either side.
- **CP4:** any gateway `src/` need → STOP, separate contract.
- **CP5:** dumps unavailable → U0+U1 proceed; U2/U3 wait (never fake a corpus).

## §8 Locked / deferred

- Byte-level canonical-XML/hash equality vs WebCheck (`StringXML` translation layer) — deferred (N4).
- Live-DPS soak — separate axis (N1). RETURN/Z alphabet — later tranche (N3).
- `Crash(Finalize)` src kill-point seam — still deferred (Phase-3 CP5).

## References

- External audit of v1 (2026-07-03, 12 findings, No-Go — accepted in full; HIGH#1/#4 architect-verified against `CreateDB.cs`/`SQLlite.cs`/`SendingOfflineChecks.cs`).
- `docs/webcheck_reverse/**`; `scripts/export_webcheck_samples.py`; `golden/webcheck_*`.
- **Our normative pins cited by U1:** `rust/prro/migrations/001_baseline.sql` (`ux_fd_fn_lnd`), ADR-M3-A1 (`node_state.next_lnd` SSOT), M3b shift spec (`docs/superpowers/specs/2026-05-17-m3b-shift-state-expansion.md`), `docs/LEGAL_INVARIANTS.md`, W10.4 MAC-recovery budget.
- `docs/superpowers/audits/2026-06-16-invariant-fuzzer-dryrun-findings.md` (D1–D5); Phase-3 spec §9 (deferrals).
