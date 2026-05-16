# M3b Implementation Plan — Phase-6-min offline subsystem + M3a structural carry-forwards

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers-extended-cc:subagent-driven-development` (recommended) or `superpowers-extended-cc:executing-plans` to execute this plan task-by-task.

**Goal.** Land the Rust **offline subsystem** sufficient to discharge `docs/PILOT_ACCEPTANCE_TEST_PLAN.md` Phase 6 ("Offline With One Fiscal Number") in the pilot dossier, *plus* close four structural M3a carry-forwards that affect production resilience: raw-CAS helper promotion, dedicated `first_kvt1_at` column, module-level single-writer enforcement, and W0b-scoped in-drain KVT2 confirmation.  Exit with: offline session lifecycle + `OFFLINE_LOCAL_ACK` transition whitelist + Pattern C stage-and-flip + Z-report guard + return-online detection + idempotent backlog sync — all under W11-extended deterministic replay covering offline crash points.

**Non-regression invariant (load-bearing).**  M3a ONLINE happy path stays baseline.  The 5-stage write path MUST NOT break.  W11 deterministic replay (21/21 fixtures green on `e183b82`) MUST stay green.  Every M3b task adds tests; no M3b task removes or skips an M3a test.

**Anchored on (canonical, committed):**
- `docs/M3a-handoff.md` — M3a closure baseline + §6.1 carry-forward list + §6.3 pilot gates (committed on `rust-gateway` at `e183b82`).
- `docs/PILOT_ACCEPTANCE_TEST_PLAN.md` Phase 6 (`docs/PILOT_ACCEPTANCE_TEST_PLAN.md:334-364`) — **acceptance anchor** for offline lifecycle exit criteria.  **NOTE: this file is currently untracked in `rust-gateway` at `e183b82`; it lands as a separate commit in THIS PR** (alongside the plan) so the anchor becomes resolvable post-merge.  Reviewers can see the diff in this PR's commits.
- ADR-M3-A1..A10 in `docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md` + `2026-05-12-adr-m3-a10-global-single-writer.md` — M3a state-machine + lock invariants that M3b must preserve.
- W0-1 / W0-2 / W0-3 freeze docs — state sequences, lock discipline, retry-recovery.
- Operator scope thesis (memory `project_m3b_scope_thesis`, set 2026-05-14) — frame for what M3b is and is NOT.
- `docs/superpowers/specs/2026-05-14-m3b-w0b-w12-gate-decision.md` — W0b verdict: **YES, drain-time/latest-doc only**.

**Architecture.**  Rust crate `prro` extends with:
- `services::offline_session` — offline-session state machine + repository contracts.
- `services::offline_sync` — return-online detection + backlog drain (Pattern C "stage and flip").
- `services::offline_guard` — Z-report block while backlog non-empty.
- `services::write_path::stage_offline_ack` — **new stage** (W7).  Post-sign dispatcher routes Offline / GoingOffline docs here.  `stage_finalize` is **untouched** — it remains strictly `Kvt2 → Ack`.
- `db::repositories::offline_sessions` — session + code pool persistence (normalization migration of existing 004-era tables — see W4).
- `db::repositories::fiscal_documents` whitelist extended with the offline edges; existing `transition_state` extended (W3) to stamp `first_kvt1_at` on `Sent → Kvt1`.
- `services::reconciliation::boot_phase` — service-layer `transition_with_audit` helper composes existing `fiscal_documents::transition_state`; 7 raw-CAS sites refactored.  LOW 3 scanner pivots (NOT deleted) to enumerate helper call sites.
- Module-level write enforcement: `boot_phase::run_boot_reconciliation` made `pub(crate)` or otherwise re-routed through `App::reconcile_pending_with` to close the HP2 mutex bypass.

**Pattern C** is the central new structural pattern (M3a had Pattern A = compute outside / persist inside; Pattern B = `Sending` intent-marker before wire send).  **Pattern C** = durable `OFFLINE_LOCAL_ACK` first inside `with_immediate` via `stage_offline_ack` (pre-send, post-sign), then *later* (return-online tick) a separate `with_immediate` envelope drives the doc through the M3a `Sending → Sent → Kvt1` ladder via backlog sync, then W12 confirms the just-sent latest doc with `lastChk(fn_sign)` and advances through `Kvt2 → Ack`.  W12 is **not** a general boot-time KVT2 poller; stale/pre-existing `Kvt1` docs stay on `passive_hold_kvt1`.

**Tech stack.**  Unchanged from M3a: Rust 1.95 + sqlx 0.8 (SQLite STRICT, WAL) + tonic 0.12 + tokio 1.x + tracing 0.1 + `tokio::task_local!`.  No new crate dependencies required.

**Bundle code + tests in every production W-task.**  No separate production-code tail tasks — for M3b a task is not landed without its targeted fixtures green.  **W11-Δ is the explicit cross-stage deterministic-replay extension gate**: it is test-only by design because the replay invariant cannot be proven inside any single offline-stage task.

**Day budgets are confidence ranges, not commitments.**  Aim: **3 weeks optimistic / 5 weeks realistic** end-to-end, including per-task review cycles.  Approximately **30–40 % of M3a's effort** per pre-plan sizing.

---

## Inputs (frozen)

- `docs/M3a-handoff.md` (M3a closure contract; §6.1 carry-forward residuals; §6.3 pilot gates).
- `docs/PILOT_ACCEPTANCE_TEST_PLAN.md` Phase 6 (`:334-364`) and Phase 7 (`:366-395` — restart/recovery; informs W11-Δ).
- `docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md` (ADR-M3-A1..A9) + `2026-05-12-adr-m3-a10-global-single-writer.md` (ADR-M3-A10).
- W0-1 / W0-2 / W0-3 freeze specs (state-sequence, lock-discipline, retry-recovery).
- Python behavioural reference: `src/prro_gateway/services/offline_session.py`, `offline_sync.py`, `offline_codes.py`, `services/return_online.py` (NOT a contract — informs naming and lifecycle shape only; M3b implementation does NOT mirror Python field-for-field).
- Rust substrate (M3a frozen): all `src/services/write_path/*`, `src/services/reconciliation/boot_phase.rs`, `src/db/repositories/*` at HEAD `e183b82`.
- `CLAUDE.md` frozen invariants 1–10.
- Operator scope thesis: `project_m3b_scope_thesis` memory.

---

## Dependency graph

```
W0a (admin)
  │
  └──> W0b (W12 gate decision — sets M3b exit criteria branch)            [BLOCKS W1+]
            │
            ├──> W1  (raw-CAS → service-layer transition_with_audit)     [PARALLEL with W2]
            └──> W2  (HP2 single-writer module-level enforcement)         [PARALLEL with W1]
                      │
W1,W2 ────────────────┴──────────> W3  (schema: first_kvt1_at column migration)
                              │
                              W4  (schema: offline_sessions + offline_codes migrations)
                                  │
                                  W5  (OfflineSession state machine + repository)
                                      │
                                      W6  (DocState OFFLINE_LOCAL_ACK whitelist edges)
                                          │
                                          W7  (offline-mode entry/exit boundary helpers)
                                              │
W7 ───────────────────────────────────────────W8  (return-online detection probe)
                                                  │
W7,W8 ────────────────────────────────────────────W9  (backlog drain — Pattern C stage-and-flip)
                                                       │
                                                       W10 (offline shift close/open policy guard)
                                                           │
W5..W10 ──────────────────────────────────────────────────W11-Δ (deterministic-replay extension)
                                                               │
                                                               W12 (in-drain KVT2 confirmation via lastChk)
                                                                   │
                                                                   W13 (M3b handoff doc + memory)
```

W0b ran first because the W12 gate verdict sets the M3b exit criteria branch.  Verdict recorded 2026-05-14: **YES, drain-time/latest-doc only** via `lastChk(fn_sign)` + `response.id == doc.server_fiscal_no`, under W2/ADR-M3-A10 single-writer discipline.  W1 + W2 are parallel (different failure domains; both are M3a structural cleanups).  W3 + W4 are sequential schema migrations (numbered in order to avoid conflicts).

---

## Task structure

### Task 0 (W0a): M3b epic + bd cross-link (administrative)

**Goal.** Create `PRRO_GATE-M3b` epic; link offline-lifecycle bd issues (`PRRO_GATE-gx2` if open + any new M3b-scoped P1 issues) as `child-of`; link M3b epic as `child-of` `PRRO_GATE-9qd` (M3 epic).  Mirror M2 / M3a admin pattern.

**Day budget:** ~30 min.

**Files:** none (bd-only).

**Acceptance.**
- M3b epic created with title containing "M3b" and "implementation".
- Child-of edges from offline-lifecycle issues to M3b epic.
- Child-of edge M3b epic → `PRRO_GATE-9qd`.
- `PRRO_GATE-9qd.6` M3a tail-cleanup epic stays closed (M3a is sealed).

**Verify.** `bd list --status open | grep -A 5 'M3b.*implementation'`.

**BlockedBy.** none.

```json:metadata
{"files":[],"verifyCommand":"bd list --status open | grep -A 5 'M3b.*implementation'","acceptanceCriteria":["M3b epic created","child-of edges from offline-lifecycle issues","child-of edge to PRRO_GATE-9qd"]}
```

---

### Task 0b (W0b): W12 gate decision — authoritative per-doc KVT2 evidence

**Status:** RESOLVED 2026-05-14.

**Verdict.**  **YES — with explicit scope restriction.**  `lastChk(fn_sign)` + `response.id == doc.server_fiscal_no` is authoritative per-doc KVT2 evidence only for the **latest** DPS document on a fiscal number, under ADR-M3-A10 / W2 single-writer discipline.  This is sufficient for W9's drain-time per-doc loop, where W12 runs immediately after `stage_send(doc_i)` and before any `doc_i+1` send can occur.  It is not sufficient for arbitrary boot-time polling of stale/pre-existing `Kvt1` docs.

**Decision file.** `docs/superpowers/specs/2026-05-14-m3b-w0b-w12-gate-decision.md`.

**Gate evidence.**
- `lastChk` takes `CheckRequest { rro_fn_sign }` and returns `CheckResponse` (`id`, `status`, `id_sign`, `data_sign`, `error_message`) per `rust/prro/proto/fiscal_server.proto:8-61`.
- M2 W0-1 records `response.id == transport_request_id` as the recovery match rule and maps `data_sign` to DPS signature of the full payload.
- `PRRO_GATE-5js` records the same WebCheck pattern: ByServerFiscalNo is `lastChk(fn_sign)` + `response.id` match, not a direct server-id lookup.

**Acceptance.**
- W12 stays in M3b, but is renamed/scoped to **in-drain KVT2 confirmation via `lastChk`**.
- W12 MUST NOT become a boot-time arbitrary `Kvt1` poller.
- `passive_hold_kvt1` remains the primary boot-time handler for stale/pre-existing `Kvt1` documents.
- M3b exit criteria final `Ack` applies only to the M3b offline-drain backlog.

**Verify.**
```
grep -l "YES - with explicit scope restriction" docs/superpowers/specs/2026-05-14-m3b-w0b-w12-gate-decision.md
grep -n "in-drain KVT2 confirmation" docs/superpowers/plans/2026-05-14-m3b-implementation.md
```

**BlockedBy.** W0a.

**Invariant impact.** Strengthens I8 for M3b offline-drain replay without widening the boot-time recovery surface.  I2 remains load-bearing: W9/W12 correctness requires W2 module-level enforcement so no same-FN send interleaves between `stage_send(doc_i)` and `lastChk(fn_sign)`.

```json:metadata
{"files":["docs/superpowers/specs/2026-05-14-m3b-w0b-w12-gate-decision.md","docs/superpowers/plans/2026-05-14-m3b-implementation.md"],"verifyCommand":"grep -l 'YES - with explicit scope restriction' docs/superpowers/specs/2026-05-14-m3b-w0b-w12-gate-decision.md && grep -n 'in-drain KVT2 confirmation' docs/superpowers/plans/2026-05-14-m3b-implementation.md","acceptanceCriteria":["spec file lands","verdict unambiguous","W12 scoped to drain-time latest-doc confirmation","passive_hold_kvt1 retained for stale boot-time Kvt1"]}
```

---

### Task 1 (W1): raw-CAS sites → service-layer `transition_with_audit` (uses existing repository `transition_state`)

**Architectural correction (MED-3 fix, 2026-05-14 review).**  M3a's `fiscal_documents::transition_state` (`rust/prro/src/db/repositories/fiscal_documents.rs:224`) is already tx-bound, takes `&mut WriteTxConn<'_>`, gates on `allowed_transition`, and returns `TransitionOutcome`.  W1 does NOT add a new repository fn — it composes a **service-layer helper** in `boot_phase` (or a shared service module) that:
1. Calls existing `fiscal_documents::transition_state(tx, doc_id, from, to)` — gets `TransitionOutcome`.
2. If `Applied` → builds the audit payload via a closure parameter and calls `audit_log::append_tx`.
3. Returns the same `TransitionOutcome` upward.

This keeps the repository layer responsible for state-machine + whitelist (correct concern), and pushes audit-payload policy into the service layer (correct concern).  No new pub fn on the repository.

**Goal.** Replace 7 raw `UPDATE fiscal_documents SET state` sites in `services/reconciliation/boot_phase.rs` with calls to the new service-layer `transition_with_audit` helper that wraps existing `fiscal_documents::transition_state` plus the audit-payload closure.  Closes M3a HIGH-2 structurally + retires LOW 3 scanner (via pivot, not delete).

**Files (proposed).**
- `src/services/reconciliation/boot_phase.rs` — **service-layer helper added at module top**:
  ```rust
  async fn transition_with_audit<F>(
      tx: &mut WriteTxConn<'_>,
      doc_id: DocumentId,
      from: DocState,
      to: DocState,
      event_type: &str,
      severity: Severity,
      payload_fn: F,
  ) -> anyhow::Result<TransitionOutcome>
  where
      F: FnOnce() -> serde_json::Value,
  {
      let outcome = fiscal_documents::transition_state(tx, doc_id, from, to).await?;
      if matches!(outcome, TransitionOutcome::Applied) {
          let payload = payload_fn();
          audit_log::append_tx(tx, "fiscal_document", &hex_lower(doc_id.as_bytes()),
                                event_type, severity, None, Some(&payload.to_string())).await?;
      }
      Ok(outcome)
  }
  ```
  Closure is `FnOnce() -> serde_json::Value` — sync (no async lifetime complexity); takes no parameters since `doc_id` + branch context are captured by the caller.  If a payload-builder needs DB lookups, the caller pre-reads them outside the closure and captures the values.
- `src/db/repositories/fiscal_documents.rs` — **NO new pub fn**.  Existing `transition_state` is sufficient.  This is the single load-bearing repository contract for CAS.
- `src/services/reconciliation/boot_phase.rs` — 7 call sites refactored:
  - `resume_sending_to_error_retryable` (line 318)
  - `advance_sent_to_kvt1_from_probe` (line 406)
  - `cas_sent_to_manual_reconciliation_from_probe` (line 499)
  - `cas_sent_to_error_retryable_from_probe` (line 586)
  - `cas_error_retryable_to_manual_reconciliation` (line 1336)
  - `cas_error_retryable_budget_exhausted` (line 1391)
  - Encrypted reroute (line 2057)
- `tests/boot_phase_raw_cas_edges_are_whitelisted.rs` — **pivot, do NOT delete** (decision pinned 2026-05-14 by operator: cheap regression guard remains valuable even after helper promotion; deleting it removes visible proof that all boot-recovery edges stay whitelisted).  Scanner rewrites its matcher to enumerate `transition_with_audit(...)` call sites in `boot_phase.rs` and extract each literal `(from, to)` pair from the helper invocation; asserts each pair satisfies `fiscal_documents::allowed_transition`.  `EXPECTED_RAW_CAS_COUNT` constant renames to `EXPECTED_HELPER_CALL_SITES` and stays locked at 7 (same operator surface, helper-mediated).  File rename optional (`boot_phase_helper_call_sites_are_whitelisted.rs` would be more accurate post-pivot); decision recorded in W1 PR.

**Rollback / containment.**  Identity-equivalent refactor — every CAS retains the same `(from, to)` and same audit-row shape; the only change is *who* issues the SQL (service helper composing existing repo fn vs inline raw SQL).  Rollback: `git revert <merge-commit>` is safe; no schema migration to back out, no on-disk state to reconcile, no in-flight transactions affected.  Containment scope: `src/services/reconciliation/boot_phase.rs` + scanner pivot in tests.  **Repository layer (`fiscal_documents.rs`) is unmodified** per MED-3 architectural resolution.  Does NOT touch `write_path`, `transports`, `runtime`, or any other module.

**Day budget:** 2–3 days.

**Acceptance.**
- No raw `UPDATE fiscal_documents SET state = '<X>' WHERE … AND state = '<Y>'` anywhere in `boot_phase.rs` (grep returns 0 matches).
- All 7 prior sites pass `cargo test -p prro --test boot_phase_w9_helpers` + `app_boot_reconciliation` + `write_path_deterministic_replay` unchanged.
- Helper docstring explicitly cross-references ADR-M3-A10 (global single-writer) and `with_immediate` envelope requirement.
- Scanner pivoted (NOT deleted) — `EXPECTED_HELPER_CALL_SITES = 7`; enumerates helper-callsite `(from, to)` pairs; passes `cargo test -p prro --test boot_phase_raw_cas_edges_are_whitelisted` (file may be renamed if desired, but the test logic survives).

**Verify.**
```
grep -n "UPDATE fiscal_documents SET state" rust/prro/src/services/reconciliation/boot_phase.rs    # → 0 matches
cargo test -p prro --test boot_phase_w9_helpers
cargo test -p prro --test app_boot_reconciliation
cargo test -p prro --test write_path_deterministic_replay
cargo test -p prro --test boot_phase_raw_cas_edges_are_whitelisted   # passes after pivot — file is NOT deleted
```

**BlockedBy.** W0a (and W0b — see W12-gate decision below).

**Invariant impact.** Strengthens I8 (recovery preserves state-machine correctness) — whitelist gate now runs on *every* boot-phase CAS, not just helper-mediated ones.

```json:metadata
{"files":["rust/prro/src/services/reconciliation/boot_phase.rs","rust/prro/tests/boot_phase_raw_cas_edges_are_whitelisted.rs"],"verifyCommand":"cargo test -p prro --test boot_phase_w9_helpers --test app_boot_reconciliation --test write_path_deterministic_replay --test boot_phase_raw_cas_edges_are_whitelisted","acceptanceCriteria":["grep raw UPDATE in boot_phase.rs returns 0","7 sites refactored via service-layer transition_with_audit composing existing fiscal_documents::transition_state","fiscal_documents.rs unmodified","scanner pivoted (NOT deleted) to enumerate helper call sites","helper docstring cross-refs ADR-M3-A10"]}
```

---

### Task 2 (W2): HP2 single-writer module-level enforcement

**Goal.** Close the `boot_phase::run_boot_reconciliation` direct-call bypass of the App-level `tokio::sync::Mutex` added in M3a HP2.  M3a's mutex protects `App::reconcile_pending_inner` but a direct call into `boot_phase::run_boot_reconciliation` skips it — acceptable for single-worker pilot, not acceptable as M3b foundation for offline drain where backlog sync is its own scheduled invocation.

**Approach.**  Make `boot_phase::run_boot_reconciliation` `pub(crate)` and route all callers through `App::reconcile_pending_with` (which already holds the mutex).  OR: pass a **non-clone lock-token type** that only `App` can produce, enforcing call discipline at the type level.

**Decision criteria for OQ2** (`pub(crate)` vs `ReconcileGuard` token):
- Choose **`pub(crate)`** IF: no external (tests, ops scripts) caller currently invokes `run_boot_reconciliation` AND fixing call sites is < 0.5 day.  Simpler surface, cheaper diff.
- Choose **`ReconcileGuard` token** IF: external callers exist AND need fine-grained discipline (e.g. tests that want to call `run_boot_reconciliation` outside an `App` context with audit-trace).  Type-system enforcement strictly stronger.  Token is **non-`Clone`** by construction (lock guards are not shareable); `Send` only if the actual call graph proves cross-task transfer is required (default: `!Send` to keep the guard pinned to its acquiring task).
- **Tiebreaker**: prefer `pub(crate)` if criteria don't differentiate — simpler API surface, smaller blast radius.

**Files (proposed — `ReconcileGuard` path):**
- `src/services/reconciliation/boot_phase.rs` — visibility tightened; entry signature accepts a `ReconcileGuard<'_>` token created only by `App::reconcile_pending_with`.  Token marked `#[must_use]`; lacks `Clone`; `Send` only if necessary per call-graph analysis (default `!Send`).
- `src/app.rs` (file path confirmed: `rust/prro/src/app.rs`, not `src/runtime/app.rs`) — single producer of `ReconcileGuard`.
- No audit row on guard creation: lock acquisition is operational telemetry, not fiscal audit (LOW-3 fix, 2026-05-14 review).  If operators need observability, emit `tracing::debug!` instead — does not pollute the audit log which is reserved for fiscal-domain events.

**Files (proposed — `pub(crate)` path):**
- `src/services/reconciliation/boot_phase.rs` — `pub` → `pub(crate)` on `run_boot_reconciliation`; any external callers fixed.
- `src/app.rs` — wrapper `App::reconcile_pending_with` becomes the only external entry; `pub` audit visibility unchanged.

**Day budget:** 1–2 days.

**Acceptance.**
- `boot_phase::run_boot_reconciliation` is NOT callable from outside `prro` crate (or requires a guard only `App` can mint).
- Existing tests that call it directly are refactored to go through `App`.
- New test: `cannot_call_run_boot_reconciliation_without_guard` (compile-fail trybuild fixture OR runtime assertion).
- HP2 mutex docstring updated: "module-level enforcement via `ReconcileGuard` since W2".

**Verify.**
```
cargo test -p prro --test app_boot_reconciliation
cargo build -p prro   # public-surface check
```

**BlockedBy.** W0a.  (Can run in parallel with W1; they touch different parts of `boot_phase.rs`.)

**Invariant impact.** Strengthens I2 (one FN, one writer) and the ADR-M3-A10 global-single-writer invariant.  Removes the carry-forward residual called out in M3a-handoff §6.1.

```json:metadata
{"files":["rust/prro/src/services/reconciliation/boot_phase.rs","rust/prro/src/app.rs"],"verifyCommand":"cargo test -p prro --test app_boot_reconciliation && cargo build -p prro","acceptanceCriteria":["run_boot_reconciliation not externally callable without guard","tests refactored to use App","HP2 mutex docstring updated","no RECONCILE_GUARD_ACQUIRED audit row (tracing::debug! only if observability needed)"]}
```

---

### Task 3 (W3): schema migration — `first_kvt1_at` column on `fiscal_documents`

**Goal.** Replace PR #45's `updated_at`-as-proxy approach with a dedicated `first_kvt1_at` TEXT column.  Populated on the `Sent → Kvt1` transition by **the existing repository `transition_state` helper** (not by a service-layer wrapper).  Centralising the stamp inside `transition_state` ensures every future `Kvt1` transition gets the column populated correctly — no caller can forget.

**Files (proposed).**
- `rust/prro/migrations/014_first_kvt1_at.sql` — `ALTER TABLE fiscal_documents ADD COLUMN first_kvt1_at TEXT;` (nullable — existing rows in Kvt1 get backfilled from `updated_at` for forward-compat; new rows populated by `transition_state`).  Backfill in a single statement: `UPDATE fiscal_documents SET first_kvt1_at = updated_at WHERE state = 'KVT1' AND first_kvt1_at IS NULL;`.  Migration number 014 because `013_mac_recovery.sql` is the highest currently committed on `rust-gateway` at `e183b82`.
- `src/db/repositories/fiscal_documents.rs::transition_state` — **extend the existing fn**, NOT add a new one (MED-3 architecture: repository owns CAS).  When `to == DocState::Kvt1`, the CAS UPDATE additionally sets `first_kvt1_at = CURRENT_TIMESTAMP`.  Concretely the UPDATE shape becomes a conditional column list:
  ```sql
  -- Default arm (to != Kvt1):
  UPDATE fiscal_documents SET state = ? WHERE document_id = ? AND state = ?
  -- Kvt1 arm (to == Kvt1):
  UPDATE fiscal_documents SET state = ?, first_kvt1_at = COALESCE(first_kvt1_at, CURRENT_TIMESTAMP)
   WHERE document_id = ? AND state = ?
  ```
  `COALESCE` means the column is set on the first Kvt1 transition and preserved on idempotent re-entry — important for crash-replay where a doc enters Kvt1 via boot recovery + later transitions back-and-forth.  Branching done in Rust (two prepared queries) or in SQL via parameter-driven UPDATE; decision recorded in W3 PR.
- `src/services/reconciliation/boot_phase.rs::passive_hold_kvt1` — read `first_kvt1_at` instead of `updated_at`; preserve PR #45's age-bucket logic and degrade-and-emit fallback.
- `src/services/reconciliation/boot_phase.rs::age_and_severity_for_kvt1` — unchanged signature; just gets a different input.
- Tests: `tests/boot_phase_w9_helpers.rs` — back-date helper switches from `updated_at` manipulation to `first_kvt1_at` manipulation; remove the `DROP TRIGGER fd_updated_at` workaround (no trigger interference on the new column — it's not in any auto-update trigger).

**Day budget:** 1 day.

**Acceptance.**
- Migration 014 applies cleanly on a fresh DB and on a populated DB.
- Backfill correctness test: existing Kvt1 row gets `first_kvt1_at = updated_at` post-migration.
- `transition_state` stamps `first_kvt1_at` on `Sent → Kvt1` automatically (new dedicated test: `transition_state_to_kvt1_stamps_first_kvt1_at`).
- `transition_state` does NOT overwrite `first_kvt1_at` on idempotent re-entry (e.g. boot recovery touching an already-Kvt1 doc) — verified via `COALESCE` semantics.
- `passive_hold_kvt1` continues to emit correct severity for the four age buckets (fresh / 1h / 24h / unparseable).
- The 4 PR #45 tests pass without trigger-drop helpers.

**Verify.**
```
cargo test -p prro --test boot_phase_w9_helpers
cargo test -p prro migration::test_014_first_kvt1_at_backfill   # new
```

**BlockedBy.** W1.  (W1 must land first so raw Kvt1 CAS is gone and every Kvt1 transition routes through `fiscal_documents::transition_state` — making the W3 extension of that single fn the canonical write seam for `first_kvt1_at`.)

**Invariant impact.** None directly; removes a known-fragile proxy (M3a-handoff §6.1 carry-forward).

```json:metadata
{"files":["rust/prro/migrations/014_first_kvt1_at.sql","rust/prro/src/db/repositories/fiscal_documents.rs","rust/prro/src/services/reconciliation/boot_phase.rs","rust/prro/tests/boot_phase_w9_helpers.rs"],"verifyCommand":"cargo test -p prro --test boot_phase_w9_helpers","acceptanceCriteria":["migration applies clean","backfill correctness","4 age-bucket tests pass","trigger-drop helper removed"]}
```

---

### Task 4 (W4): schema **normalization** of existing `offline_sessions` + `offline_codes` (NOT create-from-scratch)

**Architectural correction (HIGH-2 fix, 2026-05-14 second review).**  Both `offline_sessions` and `offline_codes` **already exist** in `rust/prro/migrations/004_offline_and_routing.sql` — M3a inherited Python-era schema verbatim.  Current shape (verified at `e183b82`):

| Table | Existing columns | M3b target |
|---|---|---|
| `offline_sessions` | `offline_session_id`, `fiscal_number`, **`status`** (CHECK `OPENING/OPEN/CLOSING/CLOSED/ABORTED`), `opened_at`, `closed_at`, `last_known_unsigned_xml_sha256`, `docs_count`, `created_at`, `updated_at` | rename `status → state`; map `CLOSING → DRAINING`; tighten partial index to UNIQUE for "one active per FN"; retain other columns (forensic value preserved) |
| `offline_codes` | `fiscal_number`, **`code_value`** (PK part), **`used_at`**, **`used_by_doc`** | rename `code_value → code_lnd`, `used_at → consumed_at`, `used_by_doc → consumed_by_document_id`; **NO `consumed` flag column** — semantic = `consumed_at IS NULL = unused`; add `ux_offline_codes_consumed_by_doc` partial UNIQUE on `consumed_by_document_id` |

**Goal.** W4 is a **normalization migration**, not new-tables creation.  The plan must explicitly describe column-rename mapping + CHECK constraint change + new partial UNIQUE index, applied as one atomic migration `015_offline_normalize.sql`.  No M3a-era state is silently rewritten — `UPDATE offline_sessions SET state = 'DRAINING' WHERE state = 'CLOSING'` is the only semantic transform; all other operations are pure column renames / new indices.

**Schema-model decision (MED-1 resolution).**  Codes are **FN-scoped pool** keyed on `(fiscal_number, code_lnd)` (post-rename), NOT session-scoped.  `offline_sessions` tracks session *state* only.  Backlog enumeration uses `JOIN fiscal_documents WHERE doc.state = 'OFFLINE_LOCAL_ACK' AND offline_codes.consumed_by_document_id = doc.document_id` — no session-id linkage at the codes layer.  This keeps FK graph minimal and lets codes survive their owning session for forensic audit.

**Files (proposed).**
- `rust/prro/migrations/015_offline_normalize.sql` — single migration applying all changes atomically.  Because SQLite cannot rewrite CHECK constraints in place, the canonical 4-step SQLite idiom is used:
  ```sql
  -- ─── offline_sessions normalization ──────────────────────────────
  -- 1. Create the new-shape table.
  CREATE TABLE offline_sessions_new (
      offline_session_id   BLOB PRIMARY KEY CHECK (length(offline_session_id) = 16),
      fiscal_number        TEXT NOT NULL,
      state                TEXT NOT NULL CHECK (state IN ('OPENING','OPEN','DRAINING','CLOSED','ABORTED')),
      opened_at            TEXT NOT NULL,
      drained_at           TEXT,                       -- NEW: timestamp at DRAINING entry
      closed_at            TEXT,
      reason_abort         TEXT,                       -- NEW: rationale for ABORTED
      last_known_unsigned_xml_sha256 BLOB,             -- preserved from 004
      docs_count           INTEGER NOT NULL DEFAULT 0, -- preserved from 004
      created_at           TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
      updated_at           TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
      FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
  ) STRICT;
  -- 2. Copy + transform.  status → state; CLOSING → DRAINING.
  INSERT INTO offline_sessions_new
      (offline_session_id, fiscal_number, state, opened_at, drained_at, closed_at, reason_abort,
       last_known_unsigned_xml_sha256, docs_count, created_at, updated_at)
  SELECT offline_session_id, fiscal_number,
         CASE status WHEN 'CLOSING' THEN 'DRAINING' ELSE status END AS state,
         opened_at,
         CASE status WHEN 'CLOSING' THEN COALESCE(updated_at, CURRENT_TIMESTAMP) ELSE NULL END AS drained_at,
         closed_at,
         NULL AS reason_abort,
         last_known_unsigned_xml_sha256, docs_count, created_at, updated_at
  FROM offline_sessions;
  -- 3. Drop old + rename.
  DROP TABLE offline_sessions;
  ALTER TABLE offline_sessions_new RENAME TO offline_sessions;
  -- 4. Tighten the active-session index to partial UNIQUE (was: non-unique in 004).
  DROP INDEX IF EXISTS ix_offline_active;
  CREATE UNIQUE INDEX ux_offline_active ON offline_sessions(fiscal_number)
      WHERE state IN ('OPENING','OPEN','DRAINING');

  -- ─── offline_codes normalization ─────────────────────────────────
  -- 1. Create new-shape table.  Column renames; NO `consumed` flag — use consumed_at IS NULL.
  CREATE TABLE offline_codes_new (
      fiscal_number             TEXT    NOT NULL,
      code_lnd                  INTEGER NOT NULL  CHECK (code_lnd > 0),
      consumed_at               TEXT,
      consumed_by_document_id   BLOB,
      PRIMARY KEY (fiscal_number, code_lnd),
      FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT,
      FOREIGN KEY (consumed_by_document_id) REFERENCES fiscal_documents(document_id) ON DELETE RESTRICT
  ) STRICT;
  -- 2. Copy + transform.  code_value → code_lnd; used_at → consumed_at; used_by_doc → consumed_by_document_id.
  INSERT INTO offline_codes_new (fiscal_number, code_lnd, consumed_at, consumed_by_document_id)
  SELECT fiscal_number, code_value, used_at, used_by_doc
  FROM offline_codes;
  -- 3. Drop old + rename.
  DROP TABLE offline_codes;
  ALTER TABLE offline_codes_new RENAME TO offline_codes;
  -- 4. Indices.
  CREATE INDEX ix_offline_codes_available
      ON offline_codes(fiscal_number, code_lnd) WHERE consumed_at IS NULL;
  CREATE UNIQUE INDEX ux_offline_codes_consumed_by_doc
      ON offline_codes(consumed_by_document_id)
      WHERE consumed_by_document_id IS NOT NULL;
  -- 5. Immutability trigger (acceptance MANDATORY per W4 PR — operator review 2026-05-14).
  -- Guards BOTH consumed_by_document_id mutation AND consumed_at reset-to-NULL.
  -- (Allowing consumed_at → NULL would let a doc's code be "un-consumed" and re-issued
  -- to a different doc, breaking the MAC chain anchor invariant.  Admin repair scripts
  -- must DROP + recreate the trigger if they need to forcibly unset; this is the
  -- explicit operator-escalation path, not the default mutation surface.)
  CREATE TRIGGER offline_codes_consumed_immutable
      BEFORE UPDATE OF consumed_by_document_id, consumed_at ON offline_codes
      WHEN OLD.consumed_at IS NOT NULL
       AND (NEW.consumed_by_document_id IS NOT OLD.consumed_by_document_id
            OR NEW.consumed_at IS NULL)
      BEGIN
          SELECT RAISE(ABORT, 'offline_codes consumed row is immutable; admin repair must DROP + recreate trigger');
      END;
  ```

**Day budget:** 1.5 days (migration plan is larger than originally estimated because of the 4-step idiom on two tables).

**Rollback / containment.**  The migration is **non-destructive of data** — every row in the pre-migration `offline_sessions` + `offline_codes` is preserved (with renamed columns and `CLOSING → DRAINING` semantic mapping).  Rollback: a paired down-migration that performs the reverse rename + `DRAINING → CLOSING` mapping is **NOT included in M3b** (M3a + earlier migrations are forward-only per repo convention; rollback for production deployments goes through full restore from the pre-migration backup).  Containment: only `004`-era tables are touched; `fiscal_documents`, `transport_trace`, `shifts`, `node_state` unchanged.

**Acceptance.**
- Migration `015_offline_normalize.sql` applies cleanly on a fresh DB (no rows to migrate).
- Migration applies cleanly on a populated DB containing pre-migration `offline_sessions` + `offline_codes` with mixed-state rows; post-migration row count is identical; `CLOSING` rows are mapped to `DRAINING` + `drained_at` populated from `updated_at`.
- `ux_offline_active` (partial UNIQUE on `offline_sessions(fiscal_number) WHERE state IN ('OPENING','OPEN','DRAINING')`) rejects a second active session for the same FN (negative test against post-migration table).
- Primary CAS guard against double-consumption verified: two concurrent `UPDATE offline_codes SET consumed_at = CURRENT_TIMESTAMP, consumed_by_document_id = ? WHERE consumed_at IS NULL AND code_lnd = ?` against the same row — one succeeds (`rows_affected = 1`), the other no-ops (`rows_affected = 0`).
- `ux_offline_codes_consumed_by_doc` (partial UNIQUE on `consumed_by_document_id WHERE NOT NULL`) rejects an attempt to set `consumed_by_document_id` on a second code row to a document already linked to another code (negative test — link uniqueness, not CAS uniqueness).
- `offline_codes_consumed_immutable` trigger (mandatory; W4 PR landing it as part of `015`) rejects:
  - UPDATE that mutates `consumed_by_document_id` after `consumed_at` was set (negative test).
  - UPDATE that resets `consumed_at` from non-NULL to NULL (negative test — was missing before; closes the 1→0 gap).
- Admin escape hatch documented: ops scripts that need to forcibly un-consume a code must explicitly `DROP TRIGGER offline_codes_consumed_immutable;` then perform the repair UPDATE, then recreate the trigger.  This is the operator-visible escalation path; the trigger does NOT silently allow mutation.
- `offline_codes` accepts FK to `fiscal_documents` only via `consumed_by_document_id`.
- Primary key `(fiscal_number, code_lnd)` prevents duplicate code rows in the same FN.

**Verify.**
```
cargo test -p prro migration::test_015_normalize_apply_fresh_db
cargo test -p prro migration::test_015_normalize_apply_populated_db_preserves_rows
cargo test -p prro migration::test_015_normalize_closing_mapped_to_draining
cargo test -p prro migration::test_015_ux_offline_active_rejects_duplicate
cargo test -p prro migration::test_015_cas_double_consume_blocked
cargo test -p prro migration::test_015_consumed_immutable_trigger_blocks_link_mutation
cargo test -p prro migration::test_015_consumed_immutable_trigger_blocks_unset_to_null
```

**BlockedBy.** W3.  (Migrations are numerically ordered: W3 lands `014_first_kvt1_at.sql` first; W4 lands `015_offline_normalize.sql` after.  The single `015` migration covers both `offline_sessions` + `offline_codes` normalization in one atomic step per the W4 4-step SQLite idiom.)

**Invariant impact.** Sets up I5 (offline bounded by code availability + limits) — codes table is the source of truth for available codes.

```json:metadata
{"files":["rust/prro/migrations/015_offline_normalize.sql"],"verifyCommand":"cargo test -p prro migration","acceptanceCriteria":["normalization migration applies clean on fresh + populated DB","status→state column rename","CLOSING→DRAINING semantic map","code_value→code_lnd column rename","used_at→consumed_at column rename","used_by_doc→consumed_by_document_id column rename","ux_offline_active partial UNIQUE rejects duplicate","CAS double-consume blocked","ux_offline_codes_consumed_by_doc link-uniqueness","offline_codes_consumed_immutable trigger blocks link mutation AND unset-to-NULL (1→0 case closed)","FK shape correct"]}
```

---

### Task 5 (W5): OfflineSession state machine + repository

**Goal.** Implement `services::offline_session::OfflineSessionService` + `db::repositories::offline_sessions::*` with the explicit state machine `OPENING → OPEN → DRAINING → CLOSED` (+ `ABORTED`).  Each transition gated by an `allowed_transition` whitelist (mirroring `fiscal_documents`).  All writes inside `with_immediate`.

**Files (proposed).**
- `rust/prro/src/db/models/enums.rs` — add `OfflineSessionState` enum + wire mapping.
- `rust/prro/src/db/repositories/offline_sessions.rs` — new module:
  - `insert_opening` — creates session row; respects `ux_offline_active`.
  - `transition_state` + `allowed_transition` — whitelist: `(Opening, Open)`, `(Open, Draining)`, `(Draining, Closed)`, `(Opening, Aborted)`, `(Open, Aborted)`, `(Draining, Aborted)`.
  - `acquire_code_tx` — atomically pick + consume the lowest available row from `offline_codes` (FN-scoped pool) where `consumed_at IS NULL`; sets `consumed_at = CURRENT_TIMESTAMP` + `consumed_by_document_id`.  Session row is NOT updated — codes are FN-scoped, sessions only track lifecycle state (per MED-1 schema decision).
  - `list_pending_for_session` — for backlog drain.
- `rust/prro/src/services/offline_session.rs` — service layer wiring repository to runtime; emits audit events `OFFLINE_SESSION_OPENED`, `OFFLINE_SESSION_DRAIN_STARTED`, `OFFLINE_SESSION_CLOSED`, `OFFLINE_SESSION_ABORTED`.

**Day budget:** 2–3 days.

**Acceptance.**
- State whitelist enforced via `allowed_transition` (parallel to `fiscal_documents::allowed_transition`).
- All writes via `WriteTxConn<'_>` + `with_immediate` — `cargo build` enforces.
- Code-pool exhaustion returns typed `OfflineSessionError::CodePoolExhausted` (not a generic anyhow).
- Concurrent acquisition safety: two concurrent tasks/acquisitions against the **same FN / active session context** never return the same `code_lnd`.  Test shape: spawn two `tokio::task`s on separate pool connections (NOT the same SQLite tx — that would be artificial), each calling `acquire_code_tx(fiscal_number)`; assert (a) the two returned codes are distinct AND (b) the DB-layer `ux_offline_codes_consumed_by_doc` invariant holds (no row has duplicate `consumed_by_document_id`).  Mechanism: the CAS `UPDATE … SET consumed_at = CURRENT_TIMESTAMP, … WHERE consumed_at IS NULL AND code_lnd=?` atomically blocks the second consumer; whichever loses retries against the next available row.
- `ux_offline_active` violations surface as `OfflineSessionError::AnotherSessionActive`.
- Allocation vs consumption discipline (load-bearing): `offline_codes` rows are **populated up-front** by an admin / operator seam (via `seed_code_range`) from operator-provided / DPS-issued code range — NOT inside `OfflineSessionService::open_session`.  Codes are FN-scoped (per MED-1 schema decision); a session opening does NOT allocate codes, it only flips state to OPEN.  `acquire_code_tx` only **marks an existing unconsumed row consumed** — it never INSERTs new rows.  Two distinct repository methods:
  - `seed_code_range(pool, fiscal_number, first_lnd, last_lnd) -> usize` — admin seam, idempotent INSERT OR IGNORE for each `(fn, code_lnd)` in `[first..=last]`; returns count of rows inserted.
  - `acquire_code_tx(tx, fiscal_number) -> Result<(code_lnd, …), OfflineSessionError>` — atomically picks the lowest available `code_lnd` for the FN where `consumed_at IS NULL` (no `consumed` flag — semantic = "consumed_at IS NULL means unused" per W4 schema), CAS-sets `consumed_at = CURRENT_TIMESTAMP` + `consumed_by_document_id`, returns `(code_lnd, consumed_at)`.

**Verify.**
```
cargo test -p prro --test offline_session_state_machine     # new
cargo test -p prro --test offline_session_code_pool         # new
```

**BlockedBy.** W4.

**Invariant impact.** Establishes I2 (single-writer-per-FN) for offline sessions; sets up I5 (bounded offline codes).

```json:metadata
{"files":["rust/prro/src/db/models/enums.rs","rust/prro/src/db/repositories/offline_sessions.rs","rust/prro/src/services/offline_session.rs","rust/prro/tests/offline_session_state_machine.rs","rust/prro/tests/offline_session_code_pool.rs"],"verifyCommand":"cargo test -p prro --test offline_session_state_machine --test offline_session_code_pool","acceptanceCriteria":["whitelist enforced","writes through with_immediate","code exhaustion typed error","concurrent acquire returns distinct codes","ux_offline_active surfaces typed error"]}
```

---

### Task 6 (W6): `DocState::OfflineLocalAck` whitelist edges

**Goal.** Extend `fiscal_documents::allowed_transition` with the M3b offline edges.  M3a already had `(OfflineLocalAck, Sent)` but only as a placeholder — M3b needs the full offline-aware ladder.

**Files (proposed).**
- `src/db/repositories/fiscal_documents.rs::allowed_transition` — add edges:
  - `(Signed, OfflineLocalAck)` — already present in M3a; verify.
  - `(OfflineLocalAck, Sending)` — start of backlog drain (Pattern C step 2).
  - `(OfflineLocalAck, Cancelled)` — manual operator escape during drain.
- The W1 service-layer `transition_with_audit` helper (composing existing `fiscal_documents::transition_state`) is the standard entry point inside boot recovery; for production write path, callers go directly through `fiscal_documents::transition_state`.  Whitelist gate runs inside the repository fn in both cases.
- Cross-reference comment in the whitelist match: "M3b §5.3 Pattern C — OFFLINE_LOCAL_ACK is the durable local ack; (OfflineLocalAck, Sending) drives the doc into the M3a online ladder on return-online."

**Day budget:** ~0.5 day.

**Acceptance.**
- `allowed_transition(Signed, OfflineLocalAck) == true` (preserved).
- `allowed_transition(OfflineLocalAck, Sending) == true` (new).
- `allowed_transition(OfflineLocalAck, Cancelled) == true` (new).
- All other M3a edges unchanged.
- Whitelist scanner from W1-pivot (or new W6 sub-test) confirms count and edge set.

**Verify.**
```
cargo test -p prro --test allowed_transition_whitelist   # new — pinned edge list
```

**BlockedBy.** W1 (whitelist gate goes through W1's helper).

**Invariant impact.** Extends I8 (recovery preserves state-machine correctness) to cover offline transitions.

```json:metadata
{"files":["rust/prro/src/db/repositories/fiscal_documents.rs","rust/prro/tests/allowed_transition_whitelist.rs"],"verifyCommand":"cargo test -p prro --test allowed_transition_whitelist","acceptanceCriteria":["Signed→OfflineLocalAck preserved","OfflineLocalAck→Sending added","OfflineLocalAck→Cancelled added","unchanged M3a edges","scanner alignment"]}
```

---

### Task 7 (W7): `stage_offline_ack` — Pattern C step 1 (pre-send local ack)

**Architectural correction (HIGH-2 fix, 2026-05-14 review).**  The offline branch is **NOT a stage_finalize extension** — `stage_finalize::run` is strictly the Kvt2 → Ack path (M3a `stage_finalize.rs:234-258`).  Offline local ack happens **BEFORE any wire send**, immediately after stage 3 (sign), bypassing stage 4 (send) and stage 5 (finalize) entirely.  Pattern C step 1 is its own stage.  Pattern C step 2 (the eventual re-entry into `stage_send → stage_finalize` via backlog drain) happens at W9.

**Goal.**  Create `services::write_path::stage_offline_ack::run` — Pattern C step 1.  After stage 3 emits `Signed`, the orchestrator dispatches on node mode: `Online → stage_send::run` (M3a unchanged); `Offline | GoingOffline → stage_offline_ack::run` (this task).  No DPS call, no crypto beyond stage 3 sign (already done), no `transport_trace` row.  Inside one `with_immediate` envelope: validate node mode + shift state, acquire an existing unconsumed `offline_codes` row, transition `Signed → OfflineLocalAck`, write `offline_fiscal_no` + `offline_fiscal_date`.  Frozen invariant 3 (no channel switch with open shift) preserved.  `stage_finalize` is **untouched** by this task.

**Files (proposed).**
- `src/services/write_path/stage_offline_ack.rs` — **new file**:
  - `pub async fn run(...) -> Result<OfflineAckOutcome, WritePathError>` — signature mirrors `stage_send::run`'s shape (doc ref, pool, runtime ctx).
  - Inside `with_immediate`: read node mode, validate Offline/GoingOffline, validate shift state, call `offline_sessions::acquire_code_tx` (W5), call `fiscal_documents::transition_state(Signed, OfflineLocalAck)` (existing helper — see W1 MED-3 resolution below), write `offline_fiscal_no = code_lnd`, `offline_fiscal_date = consumed_at`, emit `OFFLINE_LOCAL_ACK_EMITTED` audit.
  - Returns `OfflineAckOutcome { document_id, code_lnd, consumed_at }`.
- `src/services/write_path/mod.rs` (or wherever the post-sign dispatcher lives) — after stage 3 (`stage_sign::run`), branch:
  - `Online` → `stage_send::run` → `stage_finalize::run` (M3a unchanged).
  - `Offline` / `GoingOffline` → `stage_offline_ack::run`; pipeline terminates here for this doc.  No `stage_send::run` invocation, no `stage_finalize::run` invocation.
  - `Blocked` / `StopMode` / `CryptoDegraded` / `GoingOnline` → typed refusal at the dispatcher (Pattern C step 2 backlog drain runs through W9, not through this dispatcher).
- `src/services/node_state.rs` (or existing) — confirm node-mode read inside `with_immediate` envelope.
- `src/services/write_path/stage_finalize.rs` — **NO CHANGES** (load-bearing).  `stage_finalize` stays strictly `Kvt2 → Ack`.

**Day budget:** 2 days.

**Acceptance.**
- Node mode `Online` → existing M3a ladder unchanged: `Signed → Sending → Sent → Kvt1 → Kvt2 → Ack`.  `stage_finalize::run` is invoked normally.
- Node mode `Offline` → `stage_offline_ack::run` is invoked; pipeline terminates after `OfflineLocalAck`.  `stage_send::run` and `stage_finalize::run` are NOT invoked for this doc.
- An **existing** unconsumed `offline_codes` row (pre-seeded by `seed_code_range`, with `consumed_at IS NULL`) is **marked consumed** (`consumed_at = CURRENT_TIMESTAMP`, `consumed_by_document_id = <this doc>`) via the W5 `acquire_code_tx` helper.  No `consumed` flag column exists — "consumed" is the semantic of `consumed_at IS NOT NULL`.  No new `offline_codes` row is inserted.  Allocation and consumption strictly separate (per W5 discipline).
- `fiscal_documents.offline_fiscal_no` = consumed `code_lnd`; `fiscal_documents.offline_fiscal_date` = `consumed_at`.
- Node mode `Blocked`/`StopMode`/`CryptoDegraded`/`GoingOnline` → typed dispatcher refusal; doc state unchanged.
- Shift state check: emitting `OfflineLocalAck` while shift is `OPENED` is OK; while `CLOSED` is not OK (typed refusal).
- No DPS / network call inside the `with_immediate` envelope (frozen invariant 1 preserved — runtime assertion via `task_local!` flag).
- No `transport_trace` row created.
- `cargo test -p prro --test stage_finalize` (existing M3a tests) all pass unchanged — proves `stage_finalize` was not touched.
- `cargo test -p prro --test write_path_stage4_send` (existing M3a) all pass unchanged — proves `stage_send` source-state set was not narrowed.

**Verify.**
```
cargo test -p prro --test stage_offline_ack                    # new
cargo test -p prro --test write_path_dispatcher_post_sign      # new — covers dispatcher branching
cargo test -p prro --test stage_finalize                       # existing — must still pass UNCHANGED
cargo test -p prro --test write_path_stage4_send               # existing — must still pass UNCHANGED
```

**BlockedBy.** W5, W6.

**Invariant impact.** I1 (no network inside write tx) — preserved; wire interaction routed exclusively to W9 backlog drain.  I5 (offline bounded by code availability) — code-pool consumption is the bound.  I8 (state-machine correctness) — `stage_finalize` stays semantically pure (`Kvt2 → Ack` only); offline path is structurally separate.

```json:metadata
{"files":["rust/prro/src/services/write_path/stage_offline_ack.rs","rust/prro/src/services/write_path/mod.rs","rust/prro/src/services/node_state.rs","rust/prro/tests/stage_offline_ack.rs","rust/prro/tests/write_path_dispatcher_post_sign.rs"],"verifyCommand":"cargo test -p prro --test stage_offline_ack --test write_path_dispatcher_post_sign --test stage_finalize --test write_path_stage4_send","acceptanceCriteria":["new stage_offline_ack module","dispatcher branches post-sign","stage_finalize untouched","stage_send untouched","Online happy path preserved","Offline → OfflineLocalAck + code consumed","Blocked/StopMode/GoingOnline typed refusal","shift CLOSED refused","I1 preserved","no transport_trace row"]}
```

---

### Task 8 (W8): return-online detection probe

> **W8a / W8b split (2026-05-16).**  After Round 1 review on PR #58, this task was split into two PRs:
> - **W8a** (PR #58, in-review) — tested primitive: `run_tick_for_fn` + `spawn_probe_loop` + `OfflineCfg::return_online_probe_interval_seconds` (raw operator value) + `clamped_probe_interval_seconds()` helper-side clamp + stable `dps_error_class` audit taxonomy.
> - **W8b** (PR #59) — App-owned runtime seam (`App::spawn_return_online_probe`): NOT a change to `App::boot` itself.  The seam is wired by the future composition root (the runtime layer that constructs the concrete DPS channel); `main.rs` Serve remains M1-idle for the duration of W8b.  Seam responsibilities: enumerate ALL configured FNs (boot-time mode filter would orphan late `Online → Offline` transitions; tick-level skip already handles `Online` / `GoingOnline` cheaply), return the `JoinHandle` to the caller (caller owns `watch::Sender` + `JoinHandle`; App does not track lifecycle), route the raw config field through `clamped_probe_interval_seconds()` and emit WARN audit when `was_clamped == true`, durable `RETURN_ONLINE_PROBE_LOOP_ERROR` CRITICAL audit on tick error.  §Task 8 closes on API + tested App seam, NOT on production `main.rs` caller — that is a separate future runtime-composition task gated on DPS channel selection (direct DPS vs WebCheck-compatible vs future router).
>
> **§Task 8 remains OPEN until W8b merges.**  See design freeze §5 (split note) + §7a (W8b scope) — `docs/superpowers/specs/2026-05-16-m3b-w8-return-online-probe.md`.

**Goal.** Periodic background tick that, when node is `Offline` / `GoingOnline`, calls a lightweight DPS probe (`ping` or `statusRro` — choice recorded in the W8 design freeze) to detect that DPS is reachable + this FN's shift state is consistent with local.  On success: drive node mode `Offline → GoingOnline`.  Strictly read-only over the network; no fiscal documents.

**Files (proposed).**
- `src/services/offline_sync/return_online_probe.rs` — new module.
- Tick interval configurable (config: `offline.return_online_probe_interval_seconds`, default 60).
- Audit events: `RETURN_ONLINE_PROBE_ATTEMPT`, `RETURN_ONLINE_PROBE_SUCCESS`, `RETURN_ONLINE_PROBE_FAILED` (with DpsError class).
- Probe failure does NOT change state — stays Offline.  Probe success advances node to `GoingOnline` and triggers W9 backlog drain in the next tick.
- DPS surface choice (`ping` vs `statusRro`) recorded in `docs/superpowers/specs/2026-XX-XX-m3b-w8-return-online-probe.md` design freeze (companion file).

**Day budget:** 1–2 days.

**Acceptance.**
- Probe runs on a tokio task spawned through the App-owned seam (`App::spawn_return_online_probe`); production `main.rs` caller deferred to a future runtime-composition task.  Loop respects graceful shutdown (frozen invariant 9) — caller owns the `watch::Sender<bool>` and `JoinHandle<()>`.
- Probe failure → node mode unchanged; audit `RETURN_ONLINE_PROBE_FAILED` emitted with stable-string `dps_error_class` taxonomy.
- Probe success → node mode `Offline → GoingOnline`; audit `RETURN_ONLINE_PROBE_SUCCESS` emitted.
- A second successful probe while in `GoingOnline` is a no-op (idempotency, hard line 4).
- No state write while node is `Online` — tick-level skip filters `Online` / `GoingOnline` BEFORE the wire call (hard line 5).

**Verify.**
```
cargo test -p prro --test return_online_probe_success         # new
cargo test -p prro --test return_online_probe_failure         # new
cargo test -p prro --test return_online_probe_idempotent      # new
```

**BlockedBy.** W7.

**Invariant impact.** I9 (graceful shutdown) — probe task must respect shutdown channel.

```json:metadata
{"files":["rust/prro/src/services/offline_sync/return_online_probe.rs"],"verifyCommand":"cargo test -p prro --test return_online_probe_success --test return_online_probe_failure --test return_online_probe_idempotent","acceptanceCriteria":["probe spawned at boot","probe failure → audit only","probe success → GoingOnline + audit","second success no-op","no probe while Online"]}
```

---

### Task 9 (W9): backlog drain — Pattern C stage-and-flip

**Goal.** When node mode is `GoingOnline` and there is at least one `OfflineLocalAck` doc, drain the backlog sequentially.  W0b is resolved as scoped YES, so the M3b drain target is `Ack` for the offline-drain backlog:

- Drive each doc through the full M3a wire-send + finalize ladder `OfflineLocalAck → Sending → Sent → Kvt1 → Kvt2 → Ack` via existing `stage_send::run` (widened) + W12 in-drain `lastChk` confirmation + `stage_finalize::run`.  Final state per drained backlog doc: `Ack`.

The drain is idempotent: a doc that DPS reports as already-sent (`lastChk` match for `server_fiscal_no IS NOT NULL` docs only — see MED-4 narrowing below) skips wire-send and advances directly.  After all backlog docs reach `Ack`, node mode advances `GoingOnline → Online` and offline session transitions `Draining → Closed`.

**Files (proposed).**
- `src/services/offline_sync/backlog_drain.rs` — new module.
- `src/services/write_path/stage_send.rs` — **widening required** (HIGH-3 fix, 2026-05-14 review): the 4-pre CAS source-state set extends from `{Signed, ErrorRetryable}` (M3a, `stage_send.rs:489, 835`) to `{Signed, ErrorRetryable, OfflineLocalAck}`.  The wire-send semantics downstream of the 4-pre CAS are identical regardless of source state (build CheckEnvelope, sign envelope, wire send, post-wire CAS).  Adding a sibling `stage_send_offline_replay::run` would duplicate ~500 lines of code — the W9 default is **widen `stage_send`**.  The `allowed_transition` whitelist already gains `(OfflineLocalAck, Sending)` in W6; the runtime precondition in `stage_send::run` must match.  Decision recorded in W9 PR; OQ4-adjacent.
- Entry: `App::drain_offline_backlog_with(&self, fiscal_number: &str) -> Result<DrainSummary, BootError>`.  Holds the App reconcile mutex (W2 enforcement applies).
- Per-doc loop:
  1. **Conditional `lastChk` pre-flight** (MED-4 narrowing): IF `doc.server_fiscal_no IS NOT NULL` (doc was previously sent at least once and DPS may have recorded it), issue `lastChk` probe.  If DPS reports `status == OK`, id match, and non-empty `data_sign`, reuse that same response as W12 evidence and advance the doc to `Ack` without re-wiring.  IF `doc.server_fiscal_no IS NULL` (pure `OfflineLocalAck` that has never been wired to DPS) → SKIP pre-flight; proceed to step 2.  Pure offline-acked docs have no server-side state to be idempotent-against; idempotency comes from local CAS + Pattern B `Sending` marker on first send attempt (M3a's existing mechanism).
  2. CAS `OfflineLocalAck → Sending` via the W1 helper composition (uses existing `fiscal_documents::transition_state` + service-layer audit — see W1 MED-3 resolution).
  3. Reuse `stage_send::run` (widened in this task per HIGH-3 fix above) for wire send.
  4. **Hard W12 precondition:** before the drain sends any later doc on the same FN, call W12 for the current doc.  No same-FN send may interleave between `stage_send(doc_i)` and `lastChk(fn_sign)`.  This relies on W2 module-level enforcement + ADR-M3-A10 single-writer discipline.
  5. W12 calls `lastChk(fn_sign)` and requires `status == OK`, `response.id == doc.server_fiscal_no`, and non-empty `data_sign`.  On success, W12 advances through `Kvt1 → Kvt2` with audited evidence, then invokes/reuses the existing `stage_finalize::run` *Kvt2 → Ack* arm (M3a unchanged).  Drain target = `Ack`.
  6. Audit each doc transition (`OFFLINE_DRAIN_TO_ACK`).
- Failure handling: a single doc failing surfaces as `BootError::OfflineDrainFailed { document_id, source }` (per-doc attribution).  Sibling docs in the backlog continue, mirroring M3a try-and-audit shim.

**Day budget:** 3–4 days.  This is the largest task.

**Acceptance.**
- `stage_send::run` source-state set widened to `{Signed, ErrorRetryable, OfflineLocalAck}`; M3a 4-pre CAS allowlist updated; existing M3a tests for `Signed → Sending` and `ErrorRetryable → Sending` pass unchanged; new test covers `OfflineLocalAck → Sending`.
- Backlog of N docs drains **strictly in `lnd` ASC order** (MAC chain order).  Plan default is strict ASC; if OQ4 later resolves to permit a DPS-tolerated alternative ordering, a recorded design decision (companion file `2026-XX-XX-m3b-w9-pattern-c-design.md`) must update this acceptance bullet *before* W9 implementation lands.  Until then, strict ASC is contract.
- **`lastChk` pre-flight is the idempotency seam — but ONLY for docs with prior server evidence.**  For each doc in the backlog:
  - IF `doc.server_fiscal_no IS NOT NULL` (replay state — doc was sent at least once before this drain attempt): drain issues a `lastChk` probe; on `status == OK` + id match + non-empty `data_sign`, W12 reuses that response as KVT2 evidence and advances to `Ack` without re-wiring.
  - IF `doc.server_fiscal_no IS NULL` (pure `OfflineLocalAck` never wired): SKIP pre-flight; CAS `OfflineLocalAck → Sending` and invoke `stage_send::run`.  Idempotency for pure-offline docs is provided by local CAS (the `OfflineLocalAck → Sending` whitelist gate fails on the second attempt) + Pattern B `Sending` marker post-first-send.
  - Verified via two dedicated fixtures: `backlog_drain_lastchk_preflight_skips_wire_for_already_sent_doc` (replay path) and `backlog_drain_pure_offline_skips_preflight_relies_on_pattern_b` (pure-offline path).
- Backlog of N docs drains sequentially; all N reach `Ack` via W12 in-drain `lastChk` confirmation.  Verified via fixture `backlog_drain_scoped_yes_all_reach_ack`.
- W12 interleave guard verified: the drain cannot send `doc_i+1` before `doc_i` completes `lastChk` confirmation and reaches `Ack`.
- Idempotent re-drain: if drain is interrupted mid-loop and restarted, no doc is re-wired if DPS already accepted it (proven by the `lastChk` pre-flight + the `Sending → Sent` whitelist gate inherited from M3a).
- Per-doc error does not abort drain; sibling docs continue.
- After successful drain: node mode `Online`; session `Closed`; `offline_codes` rows untouched (codes were consumed at W7 time, drain just advances doc state — never re-allocates codes).
- MAC chain is not broken after drain (every doc consumed an offline code in the order they were issued; drain walks them in strict `lnd` ASC).

**Verify.**
```
cargo test -p prro --test backlog_drain_happy_path             # new
cargo test -p prro --test backlog_drain_idempotent_replay      # new
cargo test -p prro --test backlog_drain_per_doc_failure_sibling_continues   # new
cargo test -p prro --test backlog_drain_mac_chain_preserved    # new
```

**BlockedBy.** W7, W8.

**Invariant impact.** I4 (idempotency) — central to drain correctness; I8 (state-machine correctness) — drain must hit only whitelisted transitions; W11-Δ proves the cross-stage replay invariant.

```json:metadata
{"files":["rust/prro/src/services/offline_sync/backlog_drain.rs","rust/prro/src/app.rs"],"verifyCommand":"cargo test -p prro --test backlog_drain_happy_path --test backlog_drain_idempotent_replay --test backlog_drain_per_doc_failure_sibling_continues --test backlog_drain_mac_chain_preserved --test backlog_drain_scoped_yes_all_reach_ack","acceptanceCriteria":["N-doc backlog drains to Ack under scoped W0b YES","interrupted+replay is idempotent","per-doc failure → sibling continues","node mode Online + session Closed after success","MAC chain preserved","W12 interleave guard covered"]}
```

---

### Task 10 (W10): offline shift close/open policy guard

> **W10 correction (2026-05-16).**  Earlier framing of W10 as a blanket *"block Z-report whenever offline backlog or active offline session exists"* was an architectural error.  It conflated two distinct operations: (a) attempting an **online** Z-report over a stale offline backlog (must be blocked — DPS would record a Z without knowing the offline receipts to come), versus (b) closing the day **locally in offline mode** via a Pattern C `OFFLINE_LOCAL_ACK` Z_REPORT (must be allowed — the whole point of offline mode is to keep the cash desk operating, including close-of-day reporting).  A blanket Z-block would trap an offline shift against the 24h legal limit with no compliant exit path.  W10 is redesigned accordingly: it is an **offline shift close/open policy guard**, not a simple Z-report blocker.  See [`docs/OFFLINE_SHIFT_CLOSE_DECISION.md`](../../OFFLINE_SHIFT_CLOSE_DECISION.md) §"M3b correction 2026-05-16" for the authoritative policy note.

**Goal.** Apply policy gates that decide whether a `Z_REPORT` / `SHIFT_OPEN` / `SHIFT_CLOSE` attempt is *allowed*, *refused*, or *routed to local Pattern C close* based on the combination of node mode, shift state, offline-session state, offline backlog, and legal timers (24h shift / 36h continuous offline / 168h monthly offline — see [`docs/LEGAL_INVARIANTS.md`](../../LEGAL_INVARIANTS.md)).  Refusal is typed; outcomes audit-distinguished between `ONLINE_Z_REPORT_BLOCKED_BACKLOG`, `OFFLINE_Z_REPORT_LOCAL_CLOSE_ACCEPTED`, `OFFLINE_Z_REPORT_LOCAL_CLOSE_REFUSED`, and analogous SHIFT_OPEN / SHIFT_CLOSE audits.

**Out of W10 scope (explicit deferrals).**
- W10 does NOT drain the offline backlog — that is W9b.
- W10 does NOT implement W12 in-drain `lastChk` KVT2 confirmation.
- W10 does NOT itself transition documents through state edges; it gates *which doc types may enter* the existing pipeline given the current shift/offline policy.

**Files (proposed).**
- `src/services/offline_guard.rs` (or `src/services/write_path/policy_guard.rs`) — new module exposing the decision surface, e.g. `evaluate_z_report_policy(pool, fiscal_number, requested_doc_type) -> Result<PolicyDecision, _>` with `PolicyDecision::{AllowOnline, AllowOfflineLocalClose, RefuseOnlineBacklogPending, RefuseOfflineNoCode, RefuseAfterLocalClose, ...}`.
- `src/services/write_path/stage_acquire.rs` (or wherever doc-type entry validation lives) — pre-flight check before the doc enters the 5-stage pipeline.
- Audit event vocabulary: `ONLINE_Z_REPORT_BLOCKED_BACKLOG`, `OFFLINE_Z_REPORT_LOCAL_CLOSE_ACCEPTED`, `OFFLINE_Z_REPORT_LOCAL_CLOSE_REFUSED`, `POST_LOCAL_CLOSE_SALE_REFUSED`.  All Warning severity (operator-visible) except `_ACCEPTED` (Info).

**Day budget:** 1.5–2 days (was 1 day; widened to cover the policy decision surface).

**Acceptance.**
1. **Online Z_REPORT + non-empty OfflineLocalAck backlog → typed refusal + `ONLINE_Z_REPORT_BLOCKED_BACKLOG` audit.**  This is the original M3a-leaning case: DPS must not record a Z that omits offline receipts not yet drained.
2. **Online Z_REPORT + active offline session OPEN/DRAINING → typed refusal + `ONLINE_Z_REPORT_BLOCKED_BACKLOG` audit.**  Same reasoning: the session may yet produce more offline docs before drain.
3. **Offline / GoingOffline Z_REPORT + OPEN shift + active OPEN offline_session + available offline code → routed to Pattern C local close path (`stage_offline_ack` ladder, NOT the online ladder).**  Audit `OFFLINE_Z_REPORT_LOCAL_CLOSE_ACCEPTED` Info.
4. **Offline Z_REPORT consumes an offline code and lands in `OfflineLocalAck`.**  Drain (W9b) later replays it through the wire-send ladder; the offline Z_REPORT itself becomes a backlog doc, drained in `lnd` order alongside sales/returns.
5. **Offline Z_REPORT is ordered after all prior local offline docs by `lnd`.**  Strict ASC enforced by the existing `lnd` allocator; W9b drain order preserves it.
6. **After local offline Z_REPORT close (the Z_REPORT doc has reached `OfflineLocalAck`), new sale / return docs are refused with `POST_LOCAL_CLOSE_SALE_REFUSED` until the next allowed shift-open policy is satisfied** (next online `SHIFT_OPEN`, or, where policy permits, a fresh offline `SHIFT_OPEN` document — exact rule TBD by operator + legal review; W10 leaves the seam typed so the rule can land in a follow-up).
7. **Non-Z doc types unaffected EXCEPT** where the post-local-close block from (6) applies.  Returns of prior sales completed before close are out of scope here (handled separately by return policy).
8. **Audit vocabulary distinguishes** `ONLINE_Z_REPORT_BLOCKED_BACKLOG` (online refusal due to backlog) from `OFFLINE_Z_REPORT_LOCAL_CLOSE_ACCEPTED` / `_REFUSED` (offline-mode local close outcomes) and `POST_LOCAL_CLOSE_SALE_REFUSED` (post-close sale lockout).  Operators consuming audit logs can correlate refusals to root cause without parsing payload.

**Guard placement (load-bearing): the guard MUST run BEFORE any cryptographic operation (sign / canonicalize / hash), BEFORE any DPS / network call, BEFORE `fiscal_documents` row insert, BEFORE `ingress_inbox` mutation, AND BEFORE any `lnd` advancement** *for refused outcomes*.  For the routed `AllowOfflineLocalClose` outcome, the doc DOES proceed into the pipeline (stage_acquire continues, stage_sign canonicalizes, `stage_offline_ack` lands `OFFLINE_LOCAL_ACK`) — the policy decision routes the doc, it does not block it.  Refused outcomes leave system state exactly as if the request never arrived (modulo the audit-row trail).

**Implementation seam:** guard runs at `stage_acquire::run` entry (stage 1 of 5), strictly before stage 3 (`stage_sign`).  For refused outcomes the call short-circuits the pipeline.  For `AllowOfflineLocalClose` the policy result is threaded through to the W7b post-sign dispatcher so the doc routes to `stage_offline_ack` (the existing W7 ladder), not `stage_send` (the online wire-send ladder).

**Verify.**
```
cargo test -p prro --test w10_online_z_report_blocked_with_backlog         # new
cargo test -p prro --test w10_online_z_report_blocked_with_open_session    # new
cargo test -p prro --test w10_offline_z_report_local_close_accepted        # new
cargo test -p prro --test w10_offline_z_report_local_close_ordered_after_sales  # new
cargo test -p prro --test w10_post_local_close_sale_refused                # new
cargo test -p prro --test w10_no_state_mutation_on_refused_outcome         # new
```

**BlockedBy.** W5 (offline_sessions repository exists), W7 (OfflineLocalAck transitions exist), W9a (stage_send accepts OfflineLocalAck source — needed so the drained offline Z_REPORT can later re-wire through the online ladder via W9b).

**Invariant impact.** Pilot acceptance Phase 6 split into two test cases (see `docs/PILOT_ACCEPTANCE_TEST_PLAN.md`): (a) online Z-report blocked, (b) offline local Z_REPORT close accepted.  W10 is the single seam that decides both.  Legal invariants 24h / 36h / 168h are flagged as active engineering risks in `docs/LEGAL_INVARIANTS.md` — W10 must not resurrect the blanket-blocker design that would trap an offline shift against the 24h limit.

```json:metadata
{"files":["rust/prro/src/services/offline_guard.rs","rust/prro/src/services/write_path/stage_acquire.rs"],"verifyCommand":"cargo test -p prro --test w10_online_z_report_blocked_with_backlog --test w10_online_z_report_blocked_with_open_session --test w10_offline_z_report_local_close_accepted --test w10_offline_z_report_local_close_ordered_after_sales --test w10_post_local_close_sale_refused --test w10_no_state_mutation_on_refused_outcome","acceptanceCriteria":["online Z + backlog → blocked + audit","online Z + OPEN/DRAINING session → blocked + audit","offline Z + OPEN shift + code → routed to Pattern C local close","offline Z consumes code and lands OfflineLocalAck","offline Z ordered after prior offline docs by lnd","post-local-close sale refused until next shift-open","non-Z doc types unaffected except by post-close lockout","audit vocab distinguishes online block vs offline accept/refuse"]}
```

---

### Task 11 (W11-Δ): deterministic-replay extension for offline crash points

**Goal.** Extend `tests/write_path_deterministic_replay.rs` with fixtures covering offline-specific crash points.  Mirrors M3a's W11 design: each fixture pre-seeds a doc at a specific crash point and asserts `App::reconcile_pending → App::drain_offline_backlog_with` converges to the same final state regardless of where the crash happened.  W0b is scoped YES, so the post-drain final state for the M3b offline backlog is `Ack`; stale/pre-existing boot-time `Kvt1` docs remain outside this fixture set and stay covered by `passive_hold_kvt1`.

**New fixtures (proposed) — offline entry / return-online shared surface:**
- `replay_crash_in_offline_acquire_code` — crash between `acquire_code_tx` and `transition_state(Signed, OfflineLocalAck)`; expect rollback (with_immediate envelope semantics) — doc stays in `Signed`, code row has `consumed_at IS NULL`.
- `replay_crash_at_offline_local_ack_emit` — crash immediately after `OfflineLocalAck` lands; reboot → doc stays in `OfflineLocalAck`, audit visible.
- `replay_crash_during_return_online_probe` — probe in-flight crash; reboot → node mode unchanged (probe failure is rollback-equivalent).

**New fixtures (proposed) — scoped W0b YES drain target:**
- `replay_crash_mid_backlog_drain_scoped_yes` — crash mid-loop (after doc #2 of 5 reaches Ack); reboot → docs #1+#2 stay `Ack`, docs #3+#4+#5 stay `OfflineLocalAck`; second drain resumes from #3 and lands them at `Ack` via W12 in-drain confirmation.
- `replay_crash_between_stage_send_and_lastchk` — crash after `stage_send(doc_i)` records `server_fiscal_no` but before W12 confirms `lastChk`; reboot → W9 resumes `doc_i`, uses `lastChk` id match, and lands `Ack` without re-wiring the doc.
- `replay_lastchk_mismatch_after_stage_send_no_ack` — DPS returns a different `id` for the current doc; reboot leaves the doc replayable and never synthesizes `Ack`.
- `replay_crash_after_drain_before_session_close_scoped_yes` — crash after all docs Ack but before session → Closed; reboot → session closes on next reconcile.

**Day budget:** 2 days.

**Acceptance.**
- 7 new fixtures total (3 shared + 4 scoped-YES drain fixtures).
- W11 total goes from 21 → 28.
- Each fixture proves the deterministic-replay invariant (same final state regardless of crash point).  For the M3b offline-drain backlog, final state is `Ack`.
- `cargo test -p prro --test write_path_deterministic_replay` is the single command that runs all fixtures.

**Verify.**
```
cargo test -p prro --test write_path_deterministic_replay   # 28 passed
```

**BlockedBy.** W9 (backlog drain implementation exists).

**Invariant impact.** I8 (recovery preserves state-machine correctness) — proven across offline crash surface.

```json:metadata
{"files":["rust/prro/tests/write_path_deterministic_replay.rs"],"verifyCommand":"cargo test -p prro --test write_path_deterministic_replay","acceptanceCriteria":["7 new fixtures total","total 28 fixtures pass","each fixture proves replay invariant: final state is Ack for M3b offline-drain backlog"]}
```

---

### Task 12 (W12): in-drain KVT2 confirmation via `lastChk`

**Gate.** W0b is resolved as **YES — with explicit scope restriction** (`docs/superpowers/specs/2026-05-14-m3b-w0b-w12-gate-decision.md`).  W12 is in scope only for W9 drain-time latest-doc confirmation.  W12 is NOT a boot-time arbitrary `Kvt1` poller.

**Goal.** Implement the W9 per-doc confirmation step that drives the currently drained doc through `Kvt1 → Kvt2 → Ack` using `lastChk(fn_sign)` evidence.  The call occurs immediately after `stage_send(doc_i)` records `doc_i.server_fiscal_no` and before the drain attempts any later document on the same FN.

**Files (proposed).**
- `src/services/offline_sync/kvt2_confirm.rs` (or similar) — W12 confirmation helper invoked only from W9 backlog drain.
- `src/services/offline_sync/backlog_drain.rs` — calls W12 after `stage_send::run` and before advancing to the next doc.
- `src/services/reconciliation/boot_phase.rs` — keep `passive_hold_kvt1` as the primary boot-time handler for stale/pre-existing `Kvt1` docs; no generic boot-time KVT2 polling dispatcher is added in M3b.
- Audit events `KVT2_CONFIRM_ATTEMPT`, `KVT2_CONFIRM_SUCCESS`, `KVT2_CONFIRM_FAILED`.

**Day budget:** 1.5–2 days (narrower than the original generic polling task).

**Acceptance.**
- W12 is invoked only from W9 drain-time flow, for the current document being drained.
- Hard precondition enforced/tested: no same-FN send may interleave between `stage_send(doc_i)` and `lastChk(fn_sign)`; W12 relies on W2 module-level enforcement + ADR-M3-A10 single-writer discipline.
- Success requires all evidence checks:
  - `lastChk.status == OK`;
  - `response.id == doc.server_fiscal_no`;
  - `data_sign` is present and non-empty.
- On success: persist/audit the KVT2 evidence and CAS `Kvt1 → Kvt2 → Ack` via the W1 helper composition + existing `stage_finalize` for `Kvt2 → Ack`.
- On `status != OK`, id mismatch, missing/empty `data_sign`, or lost CAS: emit `KVT2_CONFIRM_FAILED` with typed error; doc does NOT reach `Ack`.
- `passive_hold_kvt1` remains the primary boot-time handler for arbitrary/stale `Kvt1` docs.
- Fixture coverage:
  - `kvt2_confirm_lastchk_match_advances_to_ack`;
  - `kvt2_confirm_lastchk_id_mismatch_no_ack`;
  - `kvt2_confirm_missing_data_sign_no_ack`;
  - `boot_time_stale_kvt1_still_passive_hold`;
  - `backlog_drain_no_next_send_before_current_lastchk`.

**BlockedBy.** W0b spec accepted, W1 (helper composition), W2 (module-level enforcement), W9 (backlog drain implementation exists), W11-Δ (replay extension covers the drain-time confirmation window).

**Invariant impact.** Strengthens I8 for M3b offline-drain replay while preserving the existing boot-time recovery boundary.  I2 is load-bearing: correctness depends on W2 closing the direct boot-phase bypass so a same-FN send cannot interleave between W9 send and W12 confirm.

```json:metadata
{"files":["rust/prro/src/services/offline_sync/kvt2_confirm.rs","rust/prro/src/services/offline_sync/backlog_drain.rs","rust/prro/src/services/reconciliation/boot_phase.rs"],"verifyCommand":"cargo test -p prro --test kvt2_confirm --test backlog_drain_scoped_yes_all_reach_ack --test boot_phase_w9_helpers","acceptanceCriteria":["W12 invoked only from W9 drain-time flow","lastChk status/id/data_sign evidence checks","Kvt1→Kvt2→Ack on success","typed failure with no Ack on mismatch/missing evidence","passive_hold_kvt1 remains primary for stale boot-time Kvt1","no same-FN send interleave before current lastChk"]}
```

---

### Task 13 (W13): M3b handoff doc + memory updates

**Goal.** Mirror M3a's handoff pattern.  Write `docs/M3b-handoff.md` capturing: M3b exit posture, residual carry-forwards (anything not landed), updated pilot gates (if any new ones surfaced during W1..W12), bd hygiene closure, worktree cleanup record, memory updates.

**Files (proposed).**
- `docs/M3b-handoff.md` — new.
- `docs/M3a-handoff.md` — minor cross-link update if M3b changed any §6.1 carry-forward language.
- Memory: update `project_m3a_starting_point` to note M3b closure; create new `project_m3b_starting_point` (mirrors the M3a pattern at PR #46).

**Day budget:** 1 day.

**Acceptance.**
- Handoff doc covers: scope landed, scope deferred, pilot gate discharge state, bd epic closure pointer.
- Memory updated; `MEMORY.md` index lines refreshed.
- W13 PR is docs-only.

**Verify.**
```
ls docs/M3b-handoff.md
grep -l m3b_starting_point ~/.claude/projects/*/memory/MEMORY.md
```

**BlockedBy.** W11-Δ AND W12.  W13 can land after the W0b verdict is recorded and W12 scoped-YES in-drain confirmation has landed with tests green.  W12 is NOT optional, but its scope is limited to W9 drain-time latest-doc confirmation; stale/pre-existing boot-time `Kvt1` docs remain documented as `passive_hold_kvt1` carry-forward.

**Invariant impact.** None directly; codifies M3b closure state.

```json:metadata
{"files":["docs/M3b-handoff.md"],"verifyCommand":"ls docs/M3b-handoff.md","acceptanceCriteria":["handoff doc lands","memory updated","bd epic closure pointer"]}
```

---

## Exit criteria for M3b

M3b is CLOSED when ALL of the following hold.  W0b selected the scoped-YES branch: final `Ack` applies to the **M3b offline-drain backlog only**, not to historical/pre-existing stale `Kvt1` documents.

### Item 1 — offline-drain backlog reaches final ACK

1. **Phase 6 of `docs/PILOT_ACCEPTANCE_TEST_PLAN.md` discharged end-to-end** in a dev contour to **final DPS ACK for the M3b offline-drain backlog**: enter offline → issue receipts as `OFFLINE_LOCAL_ACK` → block **online** Z-report while backlog exists → emit **offline** Z_REPORT local close-of-day as Pattern C `OFFLINE_LOCAL_ACK` document → enforce post-local-close sale lockout → return online → sync backlog (in strict `lnd` ASC, including the offline Z_REPORT) → finalize **all M3b backlog docs reach `Ack`** via W12 in-drain `lastChk` confirmation.  Evidence attached to pilot dossier (transport_trace + audit_log rows including `ONLINE_Z_REPORT_BLOCKED_BACKLOG` + `OFFLINE_Z_REPORT_LOCAL_CLOSE_ACCEPTED` + `POST_LOCAL_CLOSE_SALE_REFUSED` + DpsError distribution + KVT2_CONFIRM_* events).

Historical/pre-existing stale `Kvt1` docs remain out of M3b W12 scope and continue to surface through `passive_hold_kvt1` with age-bucket severity.

### Items 2–8

2. **W1..W11-Δ all merged** to `rust-gateway` with regular merge commits (per `feedback_pr_merge_style` memory).
3. **W0b verdict recorded** in `docs/superpowers/specs/2026-05-14-m3b-w0b-w12-gate-decision.md` AND W12 scoped in-drain confirmation landed.
4. **W13 handoff doc landed**, including the appropriate branch of Item 1 in M3b closure summary.
5. **W11 + W11-Δ pass green**: full `cargo test -p prro --test write_path_deterministic_replay` returns ≥28 passed.
6. **No M3a test broken**: full `cargo test -p prro` returns ≥470 passed / 0 failed / 1 ignored (M3a baseline) + W11-Δ + W5/W6/W7/W8/W9/W10 new tests; total ~520+ tests.
7. **Frozen invariants 1–10 preserved** — verified per W-task acceptance.
8. **M3b epic in bd closed** with all child issues resolved.

---

## What this plan does NOT do

Explicit non-goals (per operator thesis 2026-05-14):

- ❌ Full WebCheck parity.  M3b lands the offline subsystem shape that satisfies Phase 6, not Python's complete offline UX.
- ❌ Generic SENDING reconciler.  M3a's `boot_phase::dispatch_error_retryable_by_class` is enough for pilot; full automated reconciler scheduled M5.
- ❌ Operator manual reconciliation UI.  No CLI / admin / web surface in M3b unless a pilot blocker forces a narrow shim — even then, in-scope decision deferred to operator.
- ❌ Web admin UI.  None of any form.
- ❌ Backup/restore, CA/key rotation, rollback rehearsal.  Parallel ops/docs prerequisites, not M3b code scope.
- ❌ 36h / 168h / 24h shift-limit enforcement unless explicitly promoted by operator.  Otherwise: record pilot risk acceptance and defer.
- ❌ Generic boot-time KVT2 polling for arbitrary/stale `Kvt1` docs.  W12 is scoped to W9 drain-time latest-doc confirmation only.
- ❌ Channel switch with open shift.  Frozen invariant 3 absolute; M3b reinforces.

---

## Pilot gates (parallel track — NOT M3b-opening blockers)

`PRRO_GATE-k54` (TLS CA bundle) and `PRRO_GATE-0ps` (DPS proto drift) are pilot-gating per `docs/M3a-handoff.md §6.3.2 / §6.3.3`.  They may be discharged in parallel while M3b proceeds.  M3b does NOT depend on them; pilot deployment does.

---

## Companion files (created during M3b)

- `docs/superpowers/specs/2026-XX-XX-m3b-pre-plan-adr.md` — optional design freeze if W5/W7/W9 surface ADRs requiring sign-off (parallel to ADR-M3-A1..A10).
- `docs/superpowers/specs/2026-XX-XX-m3b-w8-return-online-probe.md` — W8 design freeze (DPS RPC choice + tick interval + audit shape).
- `docs/superpowers/specs/2026-XX-XX-m3b-w9-pattern-c-design.md` — W9 design freeze (drain loop + idempotency + lastChk pre-flight).
- `docs/superpowers/specs/2026-05-14-m3b-w0b-w12-gate-decision.md` — W0b/W12 scoped-YES verdict + three-condition checklist + rationale.
- `docs/M3b-handoff.md` — closure doc (W13).

---

## Invariant risk register

| ID | Invariant | Risk window | Mitigation |
|---|---|---|---|
| I1 | No DPS / network inside SQLite write tx | W7 `stage_offline_ack` (new stage, post-sign / pre-send), W9 drain loop | Code-pool consumption is pure DB; wire calls are between `with_immediate` envelopes, never inside |
| I2 | One FN, one writer | W9 send-to-lastChk confirmation window | W2 module-level enforcement + W9 holds App mutex; no same-FN send may interleave between `stage_send(doc_i)` and W12 `lastChk(fn_sign)` |
| I4 | Idempotency | W9 drain replay after crash | `lastChk` pre-flight probe for `server_fiscal_no IS NOT NULL` docs + `transition_state` CAS short-circuit for pure-offline docs + W12 replay fixture between `stage_send` and `lastChk` |
| I5 | Offline bounded by code availability + limits | W5 `acquire_code_tx`, W7 `stage_offline_ack` | `offline_codes.consumed_at IS NOT NULL` is durable; partial UNIQUE index `ux_offline_codes_consumed_by_doc` + `offline_codes_consumed_immutable` trigger prevent over-issue and unset-to-NULL; typed `CodePoolExhausted` error |
| I8 | Recovery preserves state-machine correctness | W11-Δ proves across offline crash points | W1 service-layer helper composes existing `fiscal_documents::transition_state` which enforces whitelist on every CAS; W11-Δ replay fixtures prove M3b offline-drain target = Ack while stale boot-time Kvt1 stays on passive hold |
| I9 | Graceful shutdown leaves replayable state | W8 probe task, W9 drain loop | Shutdown channel respected by both; pending transitions roll back inside `with_immediate` |

---

## Open question track

To be resolved during W0a or earlier in M3b execution (NOT plan-blocking):

- ~~**OQ1**~~: **CLOSED in-plan (2026-05-14).**  `transition_with_audit` (W1) takes a generic `<F: FnOnce() -> serde_json::Value>` audit-payload closure — sync, monomorphised, no allocation, no async-lifetime complexity.  See W1 code body for the canonical signature.  No `Box<dyn Fn>` decision deferred to PR.
- **OQ2**: W2 implementation choice — `pub(crate)` visibility OR `ReconcileGuard` token type?  Decision criteria pinned in W2 section above (no external callers + < 0.5 day fix → `pub(crate)`; otherwise token).  Token is non-`Clone`; `Send` only if call graph requires it.  Decision recorded in W2 PR.
- **OQ3**: W8 probe DPS RPC choice — `ping` (lighter) or `statusRro` (richer signal)?  Decision in W8 design freeze.
- **OQ4**: W9 backlog drain ordering — strictly by `lnd` ASC (MAC chain order) or accept DPS-permitted alternative orderings?  Decision in W9 design freeze.
- ~~**OQ5**~~: **CLOSED 2026-05-14.** W0b verdict = YES with explicit scope restriction: W12 is in-drain latest-doc confirmation via `lastChk`, not generic boot-time KVT2 polling.

---

## Sizing summary

| Phase | Tasks | Days (optimistic / realistic) |
|---|---|---|
| Admin + Gate | W0a, W0b | 0.5 / 0.5 |
| Foundations (locks/transitions) | W1, W2 | 3 / 5 |
| Schema | W3, W4 | 2 / 3 |
| Offline lifecycle | W5, W6, W7 | 4.5 / 6 |
| Sync/finalize | W8, W9, W10 | 5 / 7 |
| Tests/docs | W11-Δ, W12, W13 | 4 / 6 |
| **Total — scoped-YES path** | **14 tasks** | **19 days / 27.5 days** ≈ **3 weeks / 5.5 weeks** |

Confidence: realistic 5-5.5-week estimate has more buffer than M3a (which took ~6–8 weeks for ~3× the scope).  W0b is resolved before W1+: W12 remains in M3b, narrowed to in-drain latest-doc confirmation.
