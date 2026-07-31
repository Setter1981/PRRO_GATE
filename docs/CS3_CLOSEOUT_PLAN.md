# CS-3 closeout plan (S7-1 double-issue cutover → GO)

**State (2026-07-22):** atomic stack on `cs3-de-slice7-cutover` (worktree `cs3-de-slice2`), UNPUSHED, full gate GREEN (nextest `--all-features` 2250/2250, cs1_test_provenance both legs, inventory_gate control-1/3, clippy+fmt):
- `15196e8` — atomic cutover (composition core P2/P3/HELD-RMR + all re-audit fixes B3/F2/B2/B4/B1/B5/F3 + §4.5-p1).
- `dfbea70` — CS-1 re-baseline + inventory re-mint (`LIVE_DRIFT_BASE_SHA=15196e8`; immutable `f2c17b1..f2628ba` untouched).
- `236cf46` — docs (handoff + external brief).
- Recovery anchor (pre-rebuild): `4a9f672` (reflog).

**Operator decisions baked in:** merge-timing = **option 1** (merge after §5.3, Slice E as a separate slice on the merged green base); §5.3 depth = **full** decorrelated workflow; nice-to-haves = **scheduled** (not backlog-only). Models: **no Fable** (opus for the re-audit workflow).

---

## Architecture reconciliation (done vs not)

| Item | Status |
|---|---|
| **F1** — boot applies via common `apply_orchestration::apply_recorded_outcome` (shift edges 3/10 + closing-cash) | ✅ DONE (B3, in atomic). Tooth `ao04` (SHIFT_OPEN crash-resume). *Gap: SHIFT_CLOSE/Z crash teeth — Phase 5.* |
| **F2** — both Sent+NotFound producers escalate RMR+STOP atomically, no intermediate ER | ✅ DONE (boot `cas_sent_not_found_to_manual_from_probe` + kvt2 `commit_sent_replay_envelope_1c_manual`, both via `sent_not_found_to_manual`). Teeth `fixture_6`, `c5b2`. |
| **Slice E** — single evidence authority for ALL post-wire projections (trace/audit/StageSendOutcome/drain) | ❌ NOT DONE (deferred, `project_backlog_cs3_slice_e_full`). F3 was targeted (1 of 3 divergent leaves). |
| Deterministic teeth on F1/F2/F3-delta | ✅ for landed fixes; full projection table = part of Slice E. |
| FSM fuzzer extension | ❌ deferred to after Slice E (building earlier would encode known-wrong transitions). |

---

## Test-pattern coverage map (inventory 2026-07-22)

| Pattern | Status | Evidence |
|---|---|---|
| Crash-point matrix | PARTIAL (strong) | `kill_point_matrix.rs` K1–K6. Gaps: crash-inside-apply-tx, after-apply-online (only offline K6). |
| Concurrency race (task-level) | mostly ABSENT | only FN-level `m1_02` (two SELL same FN). → fuzzer. |
| Idempotency/replay | PARTIAL | rc07, ao04, oc09, k8. Gap: explicit boot×2/record×2/apply×2. |
| Metamorphic live-vs-boot | ABSENT | only implicit K5. → Phase 5. |
| HELD lifecycle | PARTIAL (strong state-mgmt) | `operator_completion.rs` oc01–15. Gap: "next doc exactly one wire", crash-mid-completion. |
| Slice E projection consistency | evidence→DB only | `cs3_evidence_matrix_conformance.rs` (11 leaves × DB columns). Full trace/audit/return/drain consistency = Slice E (not done). |
| NotFound fork-canary | PARTIAL | `sent_not_found.rs` sn01–04 (atomic tx). Explicit interleave = low value (structural). |
| Shift crash recovery | PARTIAL | `s7_apply_orchestration.rs` ao01/ao04 (SHIFT_OPEN only). Gap: SHIFT_CLOSE/Z, multi-boot no-op. |
| Multi-FN isolation | ABSENT | → fuzzer. |
| Cancellation | PARTIAL | drop-injection K3/K4 (not true task-cancel). |
| Real migration replay 034→037 | PARTIAL | individual migration tests; cumulative chain absent. |
| Mutation/revert teeth | PARTIAL | compile-fail guards + per-fix revert-canaries; not systematic. |

---

## Phases

### Phase 1 — §5.3 narrow re-audit (GATE before merge) — IN PROGRESS
Decorrelated internal workflow (opus) on the atomic diff `15196e8`. Surfaces: crash-shift (B3), F2-boot, F2-kvt2, F3-deltas, **admin-surface** (`admin.rs`, load-bearing B1/B5, not in original audited diff), core-regression (P2/P3/HELD). Each surface attacked to REFUTE soundness → findings adversarially verified by 2 skeptics → synthesis + completeness critic → GO / fix-list. Re-ground §4.5 defers (#2 rc09 / #4 whitelist / #5 SubmitRefused) on the way.
**External brief** (`docs/CS3_S7_1_CUTOVER_REAUDIT_EXTERNAL_BRIEF.md`) is the operator's separate cross-model pass.
**Exit:** GO or bounded fix-list.

### Phase 2 — Fix-pass (only if re-audit finds anything)
TDD RED-first WIP → re-squash into the atomic (§5.2 procedure, proven). Includes any obligatory teeth the re-audit demands.

### Phase 3 — Merge (§5.4, operator-driven)
Force-push over origin WIP `e344077` (needs operator confirm) → retarget top-PR→main → squash-merge. Lands the sound double-issue cutover.

### Phase 4 — Slice E (fresh slice on merged green base)
Single evidence-source-of-truth for all post-wire projections; legacy → diagnostics-only; exhaustive match. RED-first per-leaf. Unblocks projection-consistency teeth.

### Phase 5 — Obligatory deterministic teeth (cross-cutting, no combinatorial blowup)
1. SHIFT_CLOSE + Z_REPORT crash-recovery.
2. Metamorphic live-vs-boot (SHIFT_OPEN/CLOSE/Z + accepted SELL + reject + HELD).
3. Crash inside apply-tx + after-apply-online (fill K-matrix).
4. Multi-boot idempotency (2nd boot = no-op, explicit).
5. Projection-consistency table (all leaves × 6 projections) — needs Phase 4.

### Phase 6 — FSM fuzzer (on stabilized Slice-E model)
Alphabet: crash at each authorize→wire→record→apply boundary · SHIFT_OPEN/CLOSE/Z · Sent+NotFound ∥ acquire · all evidence leaves DB/trace/audit/return consistency · boot/apply replay + stale-gen · online↔offline flip · concurrency/interleave/multi-FN. Invariants each step: wire-count/doc ≤ 1 · one continuous seed/FN · no lost outcome · HELD full relational witness · post-HOLD recovery → cassa works.

### Phase 7 — Corpus + regression seeds → final GO
Big run → minimal seeds as ordinary regression tests → full workspace + clippy + migration/inventory/provenance gates + short external re-audit → **CS-3 closed.**

**Nice-to-have (scheduled, non-blocking):** cumulative migration replay 034→037 · true task-cancellation · systematic mutation-teeth.
