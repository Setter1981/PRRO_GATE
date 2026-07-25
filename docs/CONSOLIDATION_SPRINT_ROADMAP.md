# Architecture Consolidation — Sprint Roadmap (where are we)

**Companion to `docs/ARCHITECTURE_CONSOLIDATION_PLAN.md` (rev 5, LOCKED).** This is the execution
tracker: each sprint has an **exit criterion** so "where are we" = which exits are met.

**Status ground truth:** `origin/main@806b661` (2026-07-19). The original estimates remain planning
history; the status markers below reflect merged code and committed oracle state.

> **Naming:** these are **Consolidation Sprints CS-0…CS-9** (distinct from the old feature Sprint
> 11/12 numbering). CS-0…CS-8 = the architecture program (~10–17 pw + 1–2 pw tests); CS-9+ = the
> operational layer (§7) + concrete adapters, **estimated separately**.

> **Legend:** ✅ done · 🟡 in progress · 🔴 not started · ⏸ deferred

---

## 📍 WHERE WE ARE NOW: CS-3 (final D/E implementation)

CS-0, CS-1/CS-1R, and CS-2 are complete. CS-3 foundation through the read-only 3.2 engine mapping is
merged; the corrected remediation oracle rev 3.1 is committed on main. The remaining CS-3 work is the
load-bearing D/E cutover: lifetime authorization, record-then-apply, whole-FN fence, operator recovery,
and retirement of every blind-resend edge. CS-3 is not complete until its RED-first exit pins prove the
real production path.

---

## CS-0 · Prep — plan on main + foundational specs ✅
- PR `ARCHITECTURE_CONSOLIDATION_PLAN.md` (+ B11 dossier + pilot-gate checklist) onto a clean branch
  off `origin/main`.
- Author **spec #1** (executable transition contract / state model) + **spec #2** (delivery-outcome
  + reservation FSM) with RED-pins, in `docs/superpowers/specs/`.
- Decide the Cargo **workspace layout**.
- **EXIT:** plan on main; specs #1+#2 locked; workspace layout agreed. *(no code behaviour yet)*
- **STATUS:** complete in PR #285 (`f2c17b1`) and the subsequent locked-spec passes.

## CS-1 · Behaviour-neutral skeleton (steps 1 + 2a) ✅
- Create `prro-domain` (move types + pure rules; **not** `CanonicalIngressEnvelope`); facade
  re-export from `prro`.
- Contract-crate skeletons: `prro-ingress-contract`, `prro-dps-contract`, `prro-fleet-contract`.
- `prro-testkit` (`publish=false`); CI → workspace + feature-matrix.
- **EXIT:** workspace compiles **green**, all existing tests pass **through the facade**, **zero
  behaviour change**.
- **STATUS:** complete through PRs #287–#293; the external-audit remediation CS-1R/R4 closed in
  PRs #300–#310.

## CS-2 · Inactive durable state + contract specs (step 2b) ✅
- Land the **INACTIVE** durable-inbox + reservation schema + persistence tests (wired, not on the
  hot path).
- Author **spec #3** (canonical ingress contract + `IdempotencyStrategy`), **#4** (DPS contract +
  binding + cross-protocol invariant), **#5** (fleet lifecycle) — in parallel.
- **EXIT:** schema landed + persistence-tested; contract specs #3–5 locked.
- **STATUS:** complete: reservation migration 032 + repository/tests in PR #295; specs #3–#5 locked
  in PRs #296–#299.

## CS-3 · Double-issue keystone (step 2c — THE semantic PR) 🟡
- Typed DPS outcome `NotSubmitted | SubmittedUnknown | ResponseObserved(...)` + reservation FSM
  `ReservedNotStarted → CallStarted → OutcomeObserved`; **eliminate the blind resend**
  (started-call → `SubmittedUnknown`; reconciliation on the original protocol, never resend).
- Separate semantic PR with **new regression evidence** + fuzzer op for the ambiguous-timeout.
- **EXIT:** **double-issue closed** (regression + fuzzer proven); delivery certainty typed
  end-to-end. *(highest-value correctness milestone; valuable independently of the rest)*
- **MERGED FOUNDATION:** migration 033/034, typed classifier/storage, `-4`/authenticated-peer seams,
  sealed delivery algebra, single-RPC raw observation, honest digest/provenance ownership, and
  read-only engine mapping (PRs #313–#329).
- **ORACLE:** remediation rev 3.1 is committed at PR #331 (`806b661`) with DESIGN GO.
- **REMAINING EXIT WORK:** implement D/E on the production path and prove lifetime call-once,
  crash-safe record/apply, offline-reject chain hold, verified operator release, and zero blind resend.

## CS-4 · The coordinator (spec #6) 🔴
- Author **spec #6** (coordinator command-lifecycle + `admission = f(axes)` + anti-god-object
  contract + `TransitionPlan` fencing).
- Implement the **thin** per-FN coordinator: pure-domain oracle for decisions, ports for I/O, no
  own durable state, one uniform command lifecycle. Route the first command (fiscalize) through it.
- **EXIT:** coordinator **unit-testable with FAKE ports**, within its size/complexity budget; one
  command flows through it. *(anti-god-object gate green)*

## CS-5 · Consolidate all transitions (step 4) 🔴
- Move `inline`, `drain`, `probe`, `fleet/admin`, `boot` through the coordinator **one at a time**.
- Split the overloaded `NodeMode` into the orthogonal axes (connectivity / session / holds /
  recovery / shift) → `admission = f(axes)`.
- **EXIT:** the ~8 FSMs are now **one machine**; single-writer is **structurally** enforced (not by
  discipline); the seam-races that broke B11 are gone.

## CS-6 · M+N adapters (step 2d) 🔴
- Migrate `native / maria304 / checkbox / xmlrpc` ingress adapters onto `CanonicalIngressEnvelope`,
  **one at a time**, each behind the **shared contract-suite**.
- DPS `grpc` adapter behind `prro-dps-contract`; `fleet-agent` **advisory-only** skeleton (telemetry
  + alerts, no commands).
- **EXIT:** **M×N→M+N** realized; contract-suite green for every adapter; fleet advisory telemetry live.

## CS-7 · Extract store + decompose giants (steps 5 + 7) 🔴
- Extract `prro-store-sqlite` (after commands stabilize); it applies a whole `TransitionPlan`
  atomically; no raw `SqliteTransaction` leaks out.
- Decompose `boot_phase.rs` (4469) / `backlog_drain.rs` (4096) / `stage_send.rs` (2643); `prro`
  becomes a thin composition root.
- **EXIT:** full crate structure; giant modules split; workspace clean.

## CS-8 · B11 on the new foundation 🔴
- Auto GO-OFFLINE (forward-progress detector, anti-mask counter) + forward-progress-verified return
  + fleet-hold — all **thin**, riding the coordinator + typed delivery.
- **EXIT:** B11 auto-offline + return **fuzzer- and live-proven**. *(offline pilot-gate residual closed)*

## CS-9+ · Operational layer (§7) + hardening — estimated SEPARATELY ⏸
- Time-model; backup/restore-via-reconciliation; physical-failure matrix; printing≠fiscalization;
  operator-recovery (`prro doctor`); keys/updates; `schema_version`/upgrade; monitoring/alerts;
  worst-case backlog/drain capacity; update+rollback rehearsal; Windows installer; real-rig +
  final fuzzer/mutation/live-DPS pass.
- **Reporting read-models (plan §7.13):** a derived, rebuildable, off-write-path `document_lines`
  projection (line-item analytics) + the shift/receipt browse read-model → the operator/fleet
  console. Fed from the outbox; reconciles to the signed payload (fuzzer tooth). Not pilot-critical
  (pilot renders lines from the payload); powers the fleet-console moat.
- Concrete adapters (**ЕВПЗ** second DPS protocol; each new ingress protocol) = **local slices**,
  slotted here as demand appears.
- **EXIT:** the **safe-first-physical-register** gate (per `PILOT_GATE_CHECKLIST.md`).

---

## Sizing recap (from plan §8)
Architecture program **CS-0…CS-8 ≈ 10–17 pw** + **1–2 pw** test relocation/CI. Two strong engineers
with a clean zone-split ≈ **8–12 calendar weeks**. CS-9+ (operational layer + concrete adapters) is
estimated **separately** — no single conflated "to physical register" number until §7 is decomposed.

## Critical-path notes
- CS-1 and CS-2 are complete; their crate/schema foundations are already consumed by CS-3.
- **CS-3 (double-issue) is the current gate.** Finish and prove D/E before starting CS-4; it does not
  wait for the coordinator.
- CS-4→CS-5 is the actual "consolidation" (the god-object risk lives here — hold the CS-4 gate).
- CS-8 (B11) only becomes *thin* after CS-3+CS-4+CS-5; do not attempt B11 before them.
