# M3a W9 — App::boot Reconciliation Phase — Design Freeze

**Date:** 2026-05-10
**Status:** Preview — pending GO before apply
**Anchors:** ADR-M3-A7 (App::boot reconciliation contract); W0-3 §3 (pending-state recovery rules); W0-3 §4 (6-branch decision tree); W0-3 §9.1 (acceptance matrix); PRRO_GATE-ah8 (shift_state preservation acceptance test); PRRO_GATE-6bj (retry/recovery policy); **WebCheck decompiled** (`docs/webcheck_reverse/WebCheckMain/WebCheck/SubmitPtr.cs`, `SendingOfflineChecks.cs`); **WebCheck analysis** (`docs/webcheck_reverse/WEBCHECK_ANALYSIS.md`); **Python parity** (`src/prro_gateway/runtime/{container,supervisor}.py`, `services/reconciliation.py`, `services/write_path.py`).
**Predecessor:** W10 (PR #29, merged `c31116c`) — DPS error routing dispatch + MAC recovery.
**Successor:** W11 (offline session boundary); M3a handoff.

This freeze is intentionally exhaustive and unambiguous.  Every branch, every DocState, every pre-condition, every audit event has a single canonical action and a single canonical assertion shape.  Where W0-3 §4 left a phrase like "final naming is M3a impl detail", this freeze fixes the name.  Where §3 / §2.1 cite a Python or WebCheck (`SubmitPtr.cs`) anchor, this freeze repeats it next to the Rust action so the implementation reviewer can cross-check without leaving the document.

---

## 1. Purpose & scope (one paragraph)

W9 lands the M3a **boot reconciliation phase**: the per-FN decision tree that runs after pool open + migrations + integrity probe and before the runtime accepts ingress traffic.  It addresses the PRRO_GATE-ah8 hazard (`node_state::upsert_initial` silently masking a crashed `shift_state = Opened`), implements the §3 pending-state recovery contract (all 8 pending DocStates including `SENDING`), enforces fail-closed semantics on `PRAGMA quick_check` failure (no writes to a DB the integrity probe just rejected), and proves idempotency for two consecutive boots.  Recovery for any in-flight `ERROR_RETRYABLE` doc whose re-send re-enters stage 4 routes through `error_routing::route_send_result` — W10's closed-enum dispatch is the **single source of truth** for DpsError → next-state mapping; W9 does not hand-roll a parallel routing table.

**Out of scope (deferred to M3b unless otherwise noted):** offline-pool reconciliation (`OFFLINE_LOCAL_ACK` doc rows); auto-flip OFFLINE → ONLINE via `ping()`; operator UI for the reconciliation histogram; reconciliation crontab outside boot.  These are explicitly named so they do NOT leak into W9.

---

## 2. Frozen decisions (zero ambiguity)

### 2.1 API surface — final names

| Symbol | Visibility | Final name | Notes |
|---|---|---|---|
| Module | `pub mod` under `services/` | `services::reconciliation` | New module tree.  Mirrors Python's `src/prro_gateway/services/reconciliation.py`. |
| Boot-phase submodule | `pub(super) mod` | `services::reconciliation::boot_phase` | Contains the 6-branch tree + per-DocState §3 recovery dispatch.  Re-exports `pub(crate) fn run_boot_reconciliation`. |
| Public entry — `App::boot` | `pub async fn` | `App::boot(config: AppConfig) -> Result<Self, BootError>` | Extends current shape (pool + migrations).  W9 adds: singleton acquire, quick_check, integrity probe.  Returns typed `BootError` (NOT `anyhow::Error`) so callers can distinguish corruption from other failures. |
| Public entry — reconcile | `pub async fn` | `App::reconcile_pending(&self) -> Result<ReconciliationSummary, BootError>` | NEW method.  Iterates `fiscal_number_config::list_all`, applies §4.3 decision tree per FN, returns a `ReconciliationSummary { branch_a: usize, branch_b: usize, ..., docs_advanced: BTreeMap<DocState, usize>, shift_orphans_to_error: usize }`. |
| Error type | `pub enum` | `BootError` | Variants: `IntegrityCheckFailed { reason }`, `OfflineModeRefusal { fiscal_number }`, `Database(sqlx::Error)`, `Internal(String)`.  Maps to non-zero exit code from the `prro` binary via `main.rs`. |

**Discarded alternatives (rejected by this freeze, not to be reintroduced):**

- ✗ `App::recover_and_reconcile()` — verbose, doesn't match Python `reconcile_pending` parity.
- ✗ Folding reconcile into `App::boot` as a single call — breaks the test split (`app_boot_quick_check_failure.rs` must exercise `App::boot` alone; `app_boot_reconciliation.rs` must exercise `reconcile_pending` against a known-good post-boot state).
- ✗ Returning `anyhow::Error` from `App::boot` — operator-visible failure modes (corruption, offline refusal) need to be matched by callers (e.g. `main.rs` decides exit code).

### 2.2 FN list source

The canonical source of the per-FN iteration is the `fiscal_number_config` table.  Specifically: `db::repositories::fiscal_number_config::list_all(pool)` (`rust/prro/src/db/repositories/fiscal_number_config.rs:109-128`), which returns `Vec<FiscalNumberConfigRow>` ordered by `fiscal_number`.

**Rejected alternatives:**

- ✗ `AppConfig.fiscal_numbers` — does not exist on `AppConfig`.  Even if added, it would diverge from runtime truth: an operator could provision a new FN via REST then restart; the config file is stale until rewritten.  `fiscal_number_config` is the operational source.
- ✗ `SELECT DISTINCT fiscal_number FROM node_state` — wrong, because branch (a) "FN row absent from node_state" needs the FN list to come from a source that survives the missing-row case.  `fiscal_number_config` is the right one.

### 2.3 Singleton lock — acquired in `App::boot`, BEFORE migrations

Per W0-3 §4.2 step 1.  Current code (`rust/prro/src/app.rs:28-42`) does NOT acquire it.  M3a adds:

```rust
let _lock = runtime::singleton::acquire(&config.database.db_path).await
    .map_err(|e| BootError::Internal(format!("singleton lock: {e}")))?;
```

before `db::open_pool`.  The lock's lifetime is the lifetime of the returned `App` (stored in `Inner`); dropped on `App` drop, which is process exit.

### 2.4 `quick_check` placement — `App::boot`, AFTER migrations, BEFORE `reconcile_pending`

Per §4.2 step 3 ("`PRAGMA quick_check` must return `ok` ... fail-closed before any FN-row write").  Implementation: after `db::open_pool` returns successfully (migrations have already run inside `open_pool` per current M1 contract), run:

```rust
let result: String = sqlx::query_scalar("PRAGMA quick_check")
    .fetch_one(&db).await
    .map_err(BootError::Database)?;
if result != "ok" {
    return Err(BootError::IntegrityCheckFailed { reason: result });
}
```

**Hard constraint:** the `PRAGMA` runs `fetch_one` (single row).  SQLite returns `"ok"` on a clean DB; on corruption it returns the first error line.  M3a impl MUST treat anything ≠ exact string `"ok"` as failure.  No "starts_with('ok')" — partial corruption can produce strings beginning with "ok" then an error.

**On failure:** return `BootError::IntegrityCheckFailed { reason }` immediately.  **NO writes to `node_state`, `shifts`, `audit_log`, or any other table.**  The earlier draft of W0-3 §4.2 suggested writing `node_state.mode = STOP_MODE`; that suggestion was withdrawn in the final §4.2 text and we honour the withdrawal verbatim.  CRITICAL log line `DB_INTEGRITY_CHECK_FAILED` is emitted via `tracing::error!` (NOT into `audit_log`).  `/health/startup` returns 503 — wired in §5 below.

### 2.5 Single-writer-per-FN enforcement (HIGH 4 fix — no separate per-FN lease helper)

**Previous draft claim withdrawn.**  Earlier wording referenced `services::lease::acquire_per_fn` as if it existed; verification shows there is **no per-FN lease module** in the current M3a codebase.  The actual single-writer mechanisms in M3a are:

1. **SQLite WAL `BEGIN IMMEDIATE` serialisation** (per W3 contract via the `with_immediate` helper) — ensures only one writer holds the write lock at any moment across the whole DB.  This already enforces "one writer per FN" because there is at most one writer per database.
2. **Request-scoped lease via `ingress_inbox::acquire_lease(tx, &request_id)`** (`stage_acquire.rs:46`) — per-`request_id` CAS NEW→PROCESSING that prevents two ingress workers re-driving the same submission concurrently.  This is **request-scoped**, not FN-scoped.

W9 boot reconciliation runs **sequentially** (one FN at a time, single-threaded loop in `App::reconcile_pending`).  Sequential iteration + `BEGIN IMMEDIATE` serialisation = single-writer invariant preserved without needing a new per-FN lock module.

ThreadPoolExecutor parity with Python `reconciliation.py:296-316` is **explicitly out of W9 scope** — that's a post-M3a optimisation.  If cross-FN concurrency is added later, it MUST land alongside a real per-FN lease (separate slice, separate freeze).

### 2.6 W10 routing — single source of truth for in-flight `ERROR_RETRYABLE`

When W9 §3 recovery for an `ERROR_RETRYABLE` doc decides "re-send via stage 4 entry (Pattern B)", the re-send goes through `services::write_path::stage_send::run`.  Stage_send internally consumes `services::write_path::error_routing::route_send_result` (W10) for outcome routing.  **W9 does not call `route_send_result` directly** — that would create two callsites and risk drift.  W9 simply re-drives via stage_send and trusts the W10 dispatch.

For `Sent` recovery via `last_chk` re-query (the other DPS-querying branch), W9 calls a new helper `services::reconciliation::last_chk_probe::probe(ctx, doc)` (introduced in this slice) that wraps the `DpsChannel::last_chk` call + 3-way routing (match → KVT1; no-match → RequiresManualReconciliation; NotFound → ErrorRetryable for subsequent stage_send re-drive).  The 3-way routing here is recovery-only; it does NOT duplicate W10's `RoutingDecision` enum (which is for stage-4 wire-send outcomes, not for `last_chk`).

---

## 3. The 6-branch decision tree (verbatim §4.3, expanded with acceptance + Python anchor + WebCheck anchor)

The 6 branches in §4.3 are mutually exclusive on `(node_state row presence, mode, shift_state, pending docs)`.  Each branch produces exactly one audit event at the FN level (plus per-doc audits inside branch (c)).

### 3.1 Branch (a) — FN row absent

| Field | Value |
|---|---|
| Pre-condition | `node_state::get(pool, fn_id)` returns `Ok(None)` |
| Action | `node_state::upsert_initial(fn_id, mode=Online, shift_state=Closed, next_lnd=1)` — the **only** permitted use of `upsert_initial` from boot |
| Post-state | `node_state` row exists with `(mode=Online, shift_state=Closed, next_lnd=1, last_known_unsigned_xml_sha256=NULL)` |
| Audit event | `NODE_STATE_INITIALISED` INFO; payload `{"fiscal_number": "...", "branch": "a"}` |
| Side effects on other tables | None |
| Python parity | `runtime/supervisor.py:34-58` — PHASE1_STARTING → fresh-node bootstrap |
| WebCheck parity | **Not applicable.**  WebCheck has no per-FN bootstrap because the FN is provisioned via GUI (`docs/webcheck_reverse/WebCheckExe/WebCheck/Form0.cs`) and the SQLite tables are created lazily on first use (`SQLlite.cs`).  WebCheck never had the multi-FN gateway problem |
| Acceptance fixture (§9.1 #1) | "DB with no `node_state` row for `fn=X`; run `reconcile_pending`; assert row inserted with the exact 4-tuple; assert one `NODE_STATE_INITIALISED` audit row" |

### 3.2 Branch (b) — FN row present + `mode == Online` + no pending docs

| Field | Value |
|---|---|
| Pre-condition | `node_state::get` returns `Ok(Some(row))` with `row.mode == Online`; `fiscal_documents::list_pending_for_fn(pool, fn_id)` returns empty |
| Action | **No-op.** Do NOT call `upsert_initial`. Do NOT touch `shift_state` / `next_lnd` / `last_known_unsigned_xml_sha256` |
| Post-state | Row UNCHANGED — byte-for-byte identical pre/post |
| Audit event | `NODE_STATE_BOOT_IDEMPOTENT` INFO; payload `{"fiscal_number": "...", "branch": "b", "observed_mode": "Online", "observed_shift_state": "..."}` |
| Side effects | None |
| Python parity | `container.py:282-294` `_ops_tick` ONLINE branch when pending set is empty (no-op pass-through) |
| WebCheck parity | Not applicable |
| Acceptance fixture (§9.1 #2) | "Pre-seed `node_state(fn=X, mode=Online, shift_state=Opened)` (note: Opened, not Closed — proves shift_state is NOT touched); zero pending docs; run `reconcile_pending`; assert row byte-identical post; assert `upsert_initial` NOT called (spy verifies); assert one `NODE_STATE_BOOT_IDEMPOTENT` audit row" |

### 3.3 Branch (c) — FN row present + pending docs

| Field | Value |
|---|---|
| Pre-condition | `node_state::get` returns `Ok(Some(row))`; `list_pending_for_fn` returns `Vec<DocumentRow>` with `.len() >= 1` |
| Action | For each pending doc, in `(lnd, created_at, document_id)` ascending order, apply the §4 (this freeze) per-DocState recovery rule.  Do NOT touch `node_state.mode` / `shift_state` / `next_lnd` directly — they update as side-effects of the §4 transitions |
| Post-state | Each doc has either transitioned (per its source-state rule) OR remains in its source state — in which case `transport_trace.attempt_no` may have advanced if W9 re-drove via stage_send (per §4.0 attempt-counter semantics).  `node_state` may have its `next_lnd` / `last_known_unsigned_xml_sha256` advanced as side-effects of doc transitions (e.g. KVT2 → Ack finalize advances seed) |
| Audit event | `NODE_STATE_BOOT_RECONCILED` INFO **at end of per-FN loop**; payload carries histogram `{"by_outcome": {"Sent->Kvt1": 2, "Sending->ErrorRetryable": 1, "Kvt2->Ack": 3, ...}, "fiscal_number": "..."}`.  Per-doc audits are emitted by the respective stage workers / recovery helpers — W9 does NOT duplicate them |
| Side effects | Per-doc stage transitions; see §4 for the per-state map |
| Python parity | `reconciliation.py:200-256` — `_apply_poll_result` per-doc dispatch |
| WebCheck parity | **Decompiled cross-refs (verified):** `docs/webcheck_reverse/WebCheckMain/WebCheck/SubmitPtr.cs:50-152` — the entire submit-then-retry loop body.  Specific WebCheck inline-retry patterns that M3a *replaces with the durable `transport_trace.attempt_no` counter* (see §4.0 + §4.8 + §15): (1) `SubmitPtr.cs:66-76` (-3 ERROR_SAVE → 7× retry with 333ms sleep), (2) `SubmitPtr.cs:77-90` (-15 close-shift edge → `LastCheckAllInfa()` probe), (3) `SubmitPtr.cs:103-141` (-2 close-shift edge → `LastCheckAllInfa()` probe), (4) `SubmitPtr.cs:105-117` (status 0 UNKNOWN → `LastCheckAllInfa()` probe).  M3a's boot-tick semantics make these inline loops unnecessary — each W9 re-drive via `stage_send::run` allocates a new `transport_trace` row with `attempt_no = prev + 1`, and `attempts_used(doc_id) >= MAX_BOOT_ATTEMPTS = 5` triggers deterministic escalation to `RequiresManualReconciliation` |
| Acceptance fixture (§9.1 #3) | "Pre-seed `node_state(fn=X, mode=Online)` + one pending doc in EACH state ∈ {PREPARED, SIGNED, SENDING, SENT, KVT1, KVT2, ERROR_RETRYABLE} (7 docs); run `reconcile_pending`; assert each doc transitions per the §4 rule for its source state; assert one `NODE_STATE_BOOT_RECONCILED` audit row with the full histogram" |

### 3.4 Branch (d) — FN row present + `mode ∈ {Offline, GoingOffline, GoingOnline}`

| Field | Value |
|---|---|
| Pre-condition | `node_state::get` returns `Ok(Some(row))` with `row.mode` ∈ {`Offline`, `GoingOffline`, `GoingOnline`} |
| Action | **Refuse boot.** Return `Err(BootError::OfflineModeRefusal { fiscal_number: fn_id })` from `reconcile_pending` immediately. Do NOT iterate further FNs (fail-fast on the first OFFLINE FN encountered, sorted by `fiscal_number` ascending). Do NOT touch the row |
| Post-state | Row UNCHANGED |
| Audit event | `NODE_STATE_BOOT_OFFLINE_REFUSAL` ERROR (NOTE: still written before returning Err — this audit is operator-forensic; it's the one exception to "no writes on failure" because the audit is informational, not corruption-compounding); payload `{"fiscal_number": "...", "observed_mode": "Offline", "message": "FN $fn is in OFFLINE mode — start with --recover-offline M3b CLI"}` |
| Process behaviour | `App::reconcile_pending` returns Err; caller (`main.rs`) propagates via `?`; process exits non-zero (exit code 78 = `EX_CONFIG` per BSD sysexits; final exit-code mapping in §5.4) |
| Python parity | Python `container.py:262` (`_maybe_ping_and_go_online`) DOES auto-flip on a successful ping.  M3a explicitly does NOT — auto-flip is M3b territory.  This is a documented drift |
| WebCheck parity | **Decompiled cross-ref:** `docs/webcheck_reverse/WebCheckMain/WebCheck/SubmitPtr.cs:91-100` — on `status == -16` (ERROR_OFFLINE_ID), WebCheck calls `All.OfflineOnTechno()` (technical-offline switch) and continues retrying inline.  M3a takes the **opposite** stance: refuse boot rather than auto-flip, because (a) M3a is ONLINE-only and the offline lifecycle is M3b, and (b) WebCheck's "auto-fallback" is opaque to the operator — M3a's explicit refusal forces operator decision |
| Acceptance fixture (§9.1 #4) | "Pre-seed `node_state(fn=X, mode=Offline)`; run `reconcile_pending`; assert returned `Err(BootError::OfflineModeRefusal { fiscal_number: 'X' })`; assert row byte-identical post; assert one `NODE_STATE_BOOT_OFFLINE_REFUSAL` audit row; assert process exits non-zero (driven by the binary test wrapper)" |

**Sub-fixture (§9.1 #4-bis, optional but recommended):** parametrise over `{Offline, GoingOffline, GoingOnline}` — all three trigger the same refusal.  Single test with three iterations.

**Rationale for lumping `Offline` / `GoingOffline` / `GoingOnline` (MED 4 fix):** these three modes share the property that they survived a crash *during* offline-lifecycle execution.  Without M3b infrastructure to (a) drain the offline pool, (b) probe with `ping` and decide auto-flip, (c) advance partial-transition rows safely, there is **no in-scope way to advance any of them** in M3a.  GoingOnline specifically might *look* benign ("almost online"), but the only safe completion path is the M3b ping-and-flip helper, which doesn't exist.  Refusing on all three forces operator awareness and prevents silent partial-advance bugs.  The single shared audit subkey (`observed_mode`) makes the operator-facing distinction visible without three separate audit kinds.

### 3.5 Branch (e) — Mid-transition shift (`shift_state ∈ {Opening, Closing}`)

Branch (e) splits into two sub-branches per §4.3 and §9.1 row 6:

#### 3.5.1 Sub-branch (e1) — Mid-transition with corresponding pending doc

| Field | Value |
|---|---|
| Pre-condition | `row.mode == Online`; `row.shift_state ∈ {Opening, Closing}` strictly; `list_pending_for_fn` returns a doc of type `SHIFT_OPEN` / `SHIFT_CLOSE` / `Z_REPORT` matching the in-flight shift |
| Action | Treat as branch (c).  Iterate pending docs in (lnd, created_at, document_id) order; apply §4 per-DocState rule.  After per-doc reconciliation completes, `shift_state` is implicitly correct via side-effects in the doc transitions (W6/W7/W8 already wire `_apply_shift_side_effects_locked`-equivalent) |
| Post-state | `node_state.shift_state` is whatever the doc transitions made it (e.g. SHIFT_CLOSE → Kvt2 → Ack advances shift_state to Closed; SHIFT_OPEN → Kvt2 → Ack advances to Opened).  **NOT** masked by `upsert_initial` |
| Audit event | `NODE_STATE_BOOT_RECONCILED` (same as branch (c)) |
| Python parity | `reconciliation.py:370-420` `_apply_shift_side_effects_locked` |
| WebCheck parity | Not applicable (WebCheck doesn't expose shift state directly) |
| Acceptance fixture (§9.1 #5-strict, NEW) | "Pre-seed `node_state(fn=X, mode=Online, shift_state=Opening)` + one pending `SHIFT_OPEN` doc in `Sent` state; run `reconcile_pending`; assert dispatch via branch (e1) → (c) cascade; assert doc transitions per §4.5 SENT rule; assert `upsert_initial` NOT called" |

**Clarifier on PRRO_GATE-ah8 scope (HIGH 1 fix):**
The PRRO_GATE-ah8 hazard scope is **broader than branch (e)** — the no-`upsert_initial` invariant protects `shift_state ∈ {Opening, Opened, Closing}` alike.  Specifically, **fixture #5 in §9.1** (which pre-seeds `shift_state=Opened`, not `Opening`/`Closing`) dispatches to **branch (c)** per the §3.7 partition matrix (mode=Online, shift_state ∉ {Opening, Closing}, pending doc exists).  The ah8 no-mask assertion is verified inside branch (c) because branch (c) by definition does NOT call `upsert_initial`.  W0-3 §4.3 (e) text says "this is the case PRRO_GATE-ah8 specifically calls out" — that quote refers to the *bug class* (silent shift_state masking), not the *specific branch* (e).  This freeze resolves the conflation by:
- Fixture #5 (Opened, verbatim from §9.1) → exercised under branch (c) — ah8 assertion verified there.
- Fixture #5-strict (Opening/Closing, NEW addition) → exercised under branch (e1) — strict-pre-condition coverage of §4.3 (e) text.
- Fixture #6 (orphan no-doc) → branch (e2) per §3.5.2.

#### 3.5.2 Sub-branch (e2) — Mid-transition orphan (no corresponding pending doc)

| Field | Value |
|---|---|
| Pre-condition | `row.shift_state ∈ {Opening, Closing}`; `list_pending_for_fn` returns NO doc of the matching shift type — only a `shifts` table row with `state ∈ {Opening, Closing}` and no anchoring pending fiscal doc |
| Action (HIGH 10 fix) | Single `with_immediate` envelope: (a) `shifts::transition_state(shift_id, current_state, Error)` (per W0-1 §2.2 `any → ERROR` whitelist); (b) targeted UPDATE `node_state SET shift_state = 'Closed' WHERE fiscal_number = ? AND shift_state IN ('Opening', 'Closing')` — NOT via `upsert_initial`, NOT as a bulk overwrite.  Rationale: with the orphan shift forced to Error, there IS no open shift for this FN — `shift_state = Closed` is the truthful reflection of "no transition in progress".  This is NOT the PRRO_GATE-ah8 hazard (ah8 is about unconditional `upsert_initial` masking; here the targeted UPDATE is operator-evidence-based: shift went Error → no open shift exists) |
| Post-state | `shifts.state = Error` for the orphan; `node_state.shift_state = Closed` (truthfully reflecting "no active shift"); `next_lnd` UNCHANGED; `last_known_unsigned_xml_sha256` UNCHANGED |
| Audit event | `SHIFT_BOOT_ORPHAN_ERROR` CRITICAL; payload `{"fiscal_number": "...", "shift_id": "...", "observed_shift_state_pre": "Opening|Closing", "observed_node_shift_state_pre": "Opening|Closing", "node_shift_state_post": "Closed", "branch": "e2"}` |
| Side effects | The orphan shift is no longer in a transitional state; `node_state.shift_state` is no longer mid-transition; operator sees a CRITICAL audit row carrying both pre-values; second boot will dispatch to (b) (idempotent no-op) |
| Python parity | NOT explicitly handled in Python.  W0-3 §4.6 drift table row 3 names this as a Rust-only addition |
| WebCheck parity | Not applicable |
| Acceptance fixture (§9.1 #6) | "Pre-seed `node_state(fn=X, mode=Online, shift_state=Opening)` + a `shifts` row `(shift_id=S, state=Opening)` + ZERO pending docs (operator deleted the SHIFT_OPEN doc); run `reconcile_pending`; assert `shifts(S).state == Error`; assert `node_state.shift_state == Closed` (HIGH 10 fix — added to acceptance); assert one `SHIFT_BOOT_ORPHAN_ERROR` CRITICAL audit row carrying both pre-values" |
| Acceptance fixture §9.1 #6-bis (HIGH 10 fix — idempotency, deferred-2 numbering) | "After fixture #6 above, run `reconcile_pending` again immediately; assert second boot dispatches to branch (b) (audit `NODE_STATE_BOOT_IDEMPOTENT`); assert no new `SHIFT_BOOT_ORPHAN_ERROR` audit; assert `shifts(S).state` STILL `Error`; assert `node_state.shift_state` STILL `Closed`" |

### 3.6 Branch (f) — FN row present + `mode ∈ {Blocked, StopMode, CryptoDegraded}`

Three sub-cases, all read-only / no-write:

| Sub-case | Pre-condition | Action | Audit | Reason |
|---|---|---|---|---|
| (f1) Blocked | `row.mode == Blocked` | No-op; preserve row | `NODE_STATE_BOOT_BLOCKED_PRESERVED` INFO; payload `{"fiscal_number": "...", "branch": "f1"}` | Ingress on Blocked FN is gated by month-rollover logic (out of W9 scope); preserving the mode is what tells the operator the FN is still blocked |
| (f2) StopMode | `row.mode == StopMode` | No-op; preserve row | `NODE_STATE_BOOT_STOP_MODE_PRESERVED` WARN; payload same shape with `"branch": "f2"` | StopMode is terminal-soft; no boot recovery; operator clears via separate CLI |
| (f3) CryptoDegraded | `row.mode == CryptoDegraded` | No-op; preserve row | `NODE_STATE_BOOT_CRYPTO_DEGRADED_PRESERVED` WARN; payload same shape with `"branch": "f3"` | Breaker stays open; first ingress attempt triggers half-open probe per `container.py:265-281` |

| Field | Value |
|---|---|
| Action (all f-sub-cases) | No write to `node_state`; emit the FN-level audit only |
| Post-state | Row UNCHANGED |
| Python parity | `container.py:265-281` (CRYPTO_DEGRADED breaker half-open); StopMode + Blocked do not have direct Python parity (Rust-specific operational state) |
| Acceptance fixture (§9.1 #7) | "Parametrised over `{Blocked, StopMode, CryptoDegraded}`: pre-seed `node_state(fn=X, mode=$M)`; run `reconcile_pending`; assert row byte-identical post; assert appropriate audit row per sub-case (`f1`/`f2`/`f3`)" |

### 3.7 Decision-tree mutual exclusion proof

The six branches are mutually exclusive because the pre-conditions partition the state space:

- Branch (a) ⟺ `node_state::get` returns `Ok(None)`.
- Branches (b)–(f) ⟺ `node_state::get` returns `Ok(Some(row))`.
- Inside the Some-arm:
  - Branch (d) ⟺ `row.mode ∈ {Offline, GoingOffline, GoingOnline}`.
  - Branch (f) ⟺ `row.mode ∈ {Blocked, StopMode, CryptoDegraded}`.
  - Branch (b)/(c)/(e) ⟺ `row.mode == Online`.  Inside:
    - Branch (e1) ⟺ `shift_state ∈ {Opening, Closing}` AND a matching pending doc exists.
    - Branch (e2) ⟺ `shift_state ∈ {Opening, Closing}` AND no matching pending doc.
    - Branch (b) ⟺ `shift_state ∉ {Opening, Closing}` AND `list_pending_for_fn` is empty.
    - Branch (c) ⟺ `shift_state ∉ {Opening, Closing}` AND `list_pending_for_fn` is non-empty.

Implementation MUST use a `match`/`if-else if` chain in exactly this order so the partition is preserved.  A unit test (`branch_partition_exhaustive_matrix`) enumerates all `(mode, shift_state, has_pending_doc)` triples and asserts exactly one branch fires per triple.

---

## 4. Per-DocState recovery contract (8 pending states + 5 terminal exclusions)

Branch (c) and sub-branch (e1) delegate per-doc recovery to this table.  Each pending DocState has exactly one canonical recovery action.

### 4.0 Attempt counter semantics (HIGH 2 fix — durable source)

**Important:** there is **no** `recovery_attempts` column on `fiscal_documents`.  Earlier drafts of this freeze (and W0-3 §4.4 invariant #2 wording) referenced such a column loosely; the **durable counter actually lives on `transport_trace.attempt_no`** (1-based, monotonic per `document_id`, allocated via `SELECT COALESCE(MAX(attempt_no), 0) + 1` per W7 migration 010).

**Canonical budget function (used throughout §4.1-4.8 and §7):**

```sql
-- attempt_budget_used(doc_id) := COALESCE(MAX(attempt_no), 0)
SELECT COALESCE(MAX(attempt_no), 0) AS attempts_used
  FROM transport_trace
 WHERE document_id = ?
```

Implementation: new helper `db::repositories::transport_trace::attempts_used(pool, doc_id) -> sqlx::Result<i64>` (W9.2 surface; LOW 5 clarification: `COALESCE(MAX(attempt_no), 0)` guarantees a non-NULL `INTEGER`, so the helper returns plain `i64` — no `Option<i64>` wrapper).  Caller obtains directly: `let used = transport_trace::attempts_used(pool, doc_id).await?;`.

**Budget cap:** `MAX_BOOT_ATTEMPTS = 5` (per W0-3 §2 policy "retry up to `max_recovery_attempts=5`").  Constant lives in `services::reconciliation::boot_phase::MAX_BOOT_ATTEMPTS`.

**Counter increment semantics (MED 3 fix):**

| Action | Counter behaviour |
|---|---|
| W9 re-drives via `stage_send::run` (Pattern B from Signed / ErrorRetryable / Encrypted-routed) | `stage_send` itself inserts a new `transport_trace` row with `attempt_no = prev + 1` per W7 §3 contract.  W9 does not increment manually |
| W9 re-drives via `stage_sign::run` (Prepared re-sign, ErrorRetryable without SIGNED_XML) | `stage_sign` does NOT touch `transport_trace`; counter unchanged |
| W9 re-drives via `stage_finalize::run` (Kvt2 → Ack advance) | `stage_finalize` does NOT touch `transport_trace`; counter unchanged |
| W9 SENDING crash-resume CAS (`Sending → ErrorRetryable` per §4.4) | Counter unchanged — no new wire attempt was made |
| W9 SENT recovery via `last_chk_probe::probe` (match → Kvt1) | Probe completes the existing `transport_trace` row (`completed_at = NOW`); counter unchanged |
| W9 SENT recovery via `last_chk_probe::probe` (NotFound → ErrorRetryable for subsequent re-drive) | Counter unchanged; the subsequent stage_send re-drive (on the next boot tick OR same tick) creates `attempt_no = prev + 1` |
| W9 KVT1 passive hold (per §4.6 — HIGH 6 fix option A) | Counter unchanged.  No DPS call.  No `transport_trace` row created.  Single forensic audit `BOOT_KVT1_HOLD_DEFERRED` emitted |

**Budget-exhausted escalation (§4.8 row):** when `attempts_used(doc_id) >= MAX_BOOT_ATTEMPTS` AND doc is in `ErrorRetryable`, W9 transitions `ErrorRetryable → RequiresManualReconciliation` (whitelist :101) + audit `BOOT_RETRY_BUDGET_EXHAUSTED` ERROR.  This is the **only place** W9 reads `attempts_used` for a decision.

**Idempotency under this model (§7 alignment):** because `transport_trace.attempt_no` is monotonic and never decremented, two consecutive boots see strictly increasing counter values for any doc that stage_send re-drove between them.  Second boot can therefore escalate a doc to `RequiresManualReconciliation` that the first boot left in `ErrorRetryable`.  This is intended.

### 4.1 PREPARED — re-drive via stage 3 (sign)

| Field | Value |
|---|---|
| Action | Call `services::write_path::stage_sign::run(pool, ctx, doc)` (existing W6 entry point).  No DPS query; document never left the gateway |
| Whitelist transitions invoked | `Prepared → Signed` (W1 whitelist :85) on sign success; `Prepared → Rejected` (W1 whitelist :86) only on pre-sign business validation failure |
| Whitelist transitions NOT invoked | `Prepared → ErrorRetryable` — **intentional gap** (W0-1 §2.1 design constraint: "fresh PREPARED has nothing to retry") |
| Audit | Inherits W6 stage 3 audit events (`STAGE_SIGN_*`) — W9 does NOT emit a separate boot-specific audit per PREPARED doc |
| Python parity | `write_path.py:351-363` — sign-retry resume |
| WebCheck parity | Not applicable (WebCheck has no PREPARED-equivalent state — its signing is inline) |

### 4.2 SIGNED — re-drive via stage 4 entry (Pattern B SENDING marker)

| Field | Value |
|---|---|
| Action | Call `services::write_path::stage_send::run(pool, dps_channel, doc, ctx)`.  Stage_send's 4-pre source-state CAS accepts `Signed → Sending` (whitelist :88 per W1 + W10 step 3).  CMS bytes are persisted in `document_files.SIGNED_XML`; recovery skips re-sign |
| Whitelist transitions invoked | `Signed → Sending` (Pattern B entry, W7 + W10); thereafter `Sending → {Sent, Kvt1, ErrorRetryable, Rejected}` per W10 dispatch |
| Whitelist transitions NOT invoked | `Signed → Rejected` (intentional gap per W0-1 §2.1: DPS reject only after wire); `Signed → Encrypted` (Checkbox-only, out of M3a scope); `Signed → OfflineLocalAck` (M3b only) |
| Audit | Inherits W7 stage 4 + W10 routing audit events.  W9 does NOT add a separate audit per SIGNED doc |
| Python parity | `write_path.py:144-165` — the safe re-drive side (SIGNED means wire has NOT fired) |
| WebCheck parity | Not applicable |

**Why safe to re-drive:** under Pattern B, `state = SIGNED` means the wire request has NOT been initiated yet — stage 4 always commits the `Signed → Sending` CAS BEFORE calling `DpsChannel::send_chk`.  The "wire-might-have-fired" state is `Sending`, not `Signed`.  This is the structural guarantee.

### 4.3 ENCRYPTED — re-drive via stage 4 (Checkbox flow); M3a routes through ErrorRetryable

| Field | Value |
|---|---|
| Action (MED 6 fix — 1-tick deferral) | M3a is ONLINE-only with Pattern B; the only legitimate way to land in ENCRYPTED is a misconfigured backend (Checkbox-mixed contour).  W9 routes to `RequiresManualReconciliation` via the ErrorRetryable chain: transition `Encrypted → ErrorRetryable` (whitelist :91), emit audit `BOOT_ENCRYPTED_REROUTED` WARN.  **Do NOT recurse into §4.8 on this tick** — the doc-iteration loop in branch (c) processes a *snapshot* of `list_pending_for_fn` taken at branch entry; re-classifying the just-transitioned doc would either require recursion (fragile) or a two-pass loop (over-engineered for this rare case).  Instead: leave the doc in ErrorRetryable; the **next** boot tick sees it in ErrorRetryable, dispatches via §4.8 (budget check → re-drive OR escalate).  Operator timeline: tick #1 audit `BOOT_ENCRYPTED_REROUTED` → tick #2 audit `BOOT_ERROR_RETRYABLE_REDRIVEN` (or `BOOT_RETRY_BUDGET_EXHAUSTED`) — the 1-tick gap is operator-visible and intentional |
| Whitelist transitions invoked | `Encrypted → ErrorRetryable` (whitelist :91); then `ErrorRetryable → {Sending, RequiresManualReconciliation}` via §4.8 |
| Audit | `BOOT_ENCRYPTED_REROUTED` WARN; payload `{"document_id": "...", "branch": "c-encrypted", "rationale": "M3a is Pattern B + ONLINE; ENCRYPTED is Checkbox-only contour"}` (consistent with branch (a)/(b)/(f) `branch` subkey) |
| Python parity | Not applicable (Python's Checkbox flow is in `services/dispatch_route.py`, out of M3a) |
| WebCheck parity | Not applicable |

**M3a note:** in practice no doc should reach ENCRYPTED in M3a's DPS path.  This rule exists so that if one does (test seed, manual operator action, schema migration leftover), boot doesn't crash on an unknown source state.

### 4.4 SENDING — DO NOT auto-re-send; CAS to ErrorRetryable

| Field | Value |
|---|---|
| Action | **DO NOT** call `stage_send`.  Execute the W9 helper `boot_phase::resume_sending_to_error_retryable(pool, doc)`: a single `with_immediate` envelope containing CAS `Sending → ErrorRetryable` (whitelist :92) + audit append.  Does NOT call DPS |
| Whitelist transitions invoked | `Sending → ErrorRetryable` (whitelist :92; the recovery-only edge per ADR-M3-A9) |
| Whitelist transitions NOT invoked | `Sending → Sent` (live-only); `Sending → Kvt1` (live-only); `Sending → Rejected` (live-only) — recovery NEVER drives Sending forward without an authoritative DPS query AND operator decision |
| Audit | `BOOT_RESUME_SENDING_TO_ERROR_RETRYABLE` ERROR; payload `{"document_id": "...", "branch": "c-sending", "rationale": "DPS does not deduplicate; re-sending would be duplicate-document hazard"}` |
| Python parity | `write_path.py:144-165` — direct parity of the contract |
| WebCheck parity | **Confirmed structural gap.**  WebCheck has NO Pattern B intermediate.  Decompiled evidence: `docs/webcheck_reverse/WebCheckMain/WebCheck/SendingOfflineChecks.cs:82-127` — `SubmitCheck` is called and then ONLY on success path do columns `signedanswerfromficscal` / `checksigned` get persisted via `UPDATEksef`.  A crash between the wire return and the `UPDATEksef` call would cause the next boot to re-send the same check, creating a duplicate-document hazard.  M3a's Pattern B SENDING marker (W7 + ADR-M3-A5) is structurally safer than WebCheck — this freeze cites the WebCheck implementation as the **anti-pattern** we intentionally avoid |

**Operator follow-up:** the resulting `ErrorRetryable` doc is what the operator triages.  If the operator confirms (via `last_chk` manually) that DPS has no record → they call the requeue API (M3a admin endpoint; in scope of M3a runtime UI work that's NOT W9).  If DPS already has it → operator marks `RequiresManualReconciliation`.

**Why W9 does not call `last_chk` automatically for SENDING:** §3 says it explicitly — recovery cannot distinguish "DPS has it but reply lost" from "wire never fired", and an automatic `last_chk` would either falsely advance OR loop into the same race.  The single `Sending → ErrorRetryable` flip is the binding contract.

### 4.5 SENT — `last_chk` re-query (3 outcomes)

| Field | Value |
|---|---|
| Action | Call `services::reconciliation::last_chk_probe::probe(ctx, doc)`.  The probe issues `DpsChannel::last_chk(fn_sign)` ONCE (single shot, not retried), parses the response, and routes |
| 3-way routing | (1) `Ok(ack)` with `ack.id == doc.transport_request_id` → call new W9-specific helper `boot_phase::advance_sent_to_kvt1_from_probe(pool, doc, ack)` (MED 1 fix — `stage_send` exposes ONLY `pub async fn run` for live wire submit; no public `finalize_sent_to_kvt1_tx` exists, so W9 ships its own).  The W9 helper is a single `with_immediate` envelope: CAS `Sent → Kvt1` (whitelist :95) + `document_files::replace_tx(doc_id, DocumentFileKind::Kvt1Raw, ack.data_sign.clone())` (HIGH 5 fix — `CheckAck` is a plain struct with `id: String`, `id_sign: Vec<u8>`, `data_sign: Vec<u8>` per `dto.rs:66-71`; no `encode()` method; the signed-receipt bytes are `data_sign` directly) + complete the existing in-flight `transport_trace` row via new helper `transport_trace::complete_via_recovery_tx(tx, doc_id, attempt_no, server_fiscal_no = ack.id)` — see HIGH 9 fix below — which writes `completed_at = NOW`, `outcome_kind = OutcomeKind::Ok` (HIGH 8 fix — `AckRecovered` is NOT in the closed enum at `transport_trace.rs:35-42`; the doc IS protocol-acknowledged, so `Ok` is the truthful classifier), `server_fiscal_no = ack.id`, while preserving the original row's `wire_call_started_at` / `wire_call_finished_at` (original `send_chk` times — recovery does NOT overwrite the forensic record of when DPS originally received) + audit `BOOT_LAST_CHK_MATCH_KVT1`.  Doc continues per §4.6 KVT1 rule on the same tick.  (2) `Ok(ack)` with `ack.id != doc.transport_request_id` → doc transitions Sent → ErrorRetryable (whitelist :93) → §4.8 immediately escalates to `RequiresManualReconciliation` (operator decision: lost en route).  (3) `Err(NotFound)` → doc transitions Sent → ErrorRetryable; §4.8 then re-drives via Pattern B (`ErrorRetryable → Sending → wire`); the `last_chk` failure was DPS confirming "no record", so re-send is safe |
| Asymmetry note (HIGH 7 surfaced during HIGH 5 fix) | W7 stage_send's live `Sent → Kvt1` path does NOT currently persist `Kvt1Raw` (the schema variant exists at `db/repositories/document_files.rs:34` but no `replace_tx(Kvt1Raw, ...)` call lives in stage_send).  W9 boot recovery **deliberately persists `Kvt1Raw`** via the helper above because boot recovery is the auditable forensic moment — operators need the bytes for the manual reconciliation trail when SENT recovery happens.  This is a *deliberate forensic-augmentation* of recovery over live happy-path, NOT a parity break.  A future cleanup PR may mirror the persistence into the live W7 path |
| `complete_via_recovery_tx` helper specification (HIGH 9 fix) | New helper in `db::repositories::transport_trace`: `pub async fn complete_via_recovery_tx(tx: &mut WriteTxConn<'_>, doc_id: DocumentId, attempt_no: i32, server_fiscal_no: String) -> sqlx::Result<()>`.  SQL: `UPDATE transport_trace SET completed_at = CURRENT_TIMESTAMP, outcome_kind = 'OK', server_fiscal_no = ? WHERE document_id = ? AND attempt_no = ? AND completed_at IS NULL`.  Returns Err if `rows_affected != 1` (defensive — caller (advance_sent_to_kvt1_from_probe) should have verified the in-flight row exists; this is a structural check).  **Differs from existing `complete_tx` (line 168)** because: (a) recovery preserves the ORIGINAL `wire_call_started_at` / `wire_call_finished_at` of the failed `send_chk` (those times are forensically valuable; they tell the operator when DPS originally received the doc).  Overwriting them with the `last_chk` probe times would lie about the original wire moment.  (b) Recovery doesn't have an `error_kind` / `error_message` / `server_status_code` to write (those are NULL by default).  The `WHERE completed_at IS NULL` clause makes the helper idempotent: a second recovery boot for an already-completed-via-recovery doc returns `rows_affected = 0`, which the caller treats as a re-entry signal |
| Whitelist transitions invoked | `Sent → Kvt1` (whitelist :95); `Sent → ErrorRetryable` (whitelist :93); `Sent → Rejected` (whitelist :94) — Rejected ONLY for pre-classified terminal business reject from the `last_chk` reply itself (e.g. ack.status indicates terminal) |
| Whitelist transitions NOT invoked | `Sent → Sending` direct — **intentional gap** (W0-1 §2.1: path goes via ErrorRetryable per ADR-M3-A9 step 3); preserves the "post-Sent reconciliation vs direct stage-4 entry" separation |
| Audit | `last_chk_probe` emits one of `BOOT_LAST_CHK_MATCH_KVT1` / `BOOT_LAST_CHK_MISMATCH_REJECTED` / `BOOT_LAST_CHK_NOT_FOUND_RETRY` per outcome |
| Python parity | `dps_fiscal_server.py:262-349` |
| WebCheck parity | **Decompiled cross-ref:** `docs/webcheck_reverse/WebCheckMain/WebCheck/SubmitPtr.cs:105-117` — on `status == 0` (proto-default UNKNOWN), WebCheck sleeps 333ms then calls `new CheckLastCheck().LastCheckAllInfa()`; if `MaxID("ksef") + 1 == returnDI` it treats the prior submit as successful and synthesises `returnStatus = 1`.  M3a's `last_chk_probe` is the rigorous version of the same idea: instead of comparing local sequence (`MaxID + 1`), M3a compares the `transport_request_id` directly (which is the wire's authoritative identity), and routes into three explicit outcomes instead of "synthesise success" |

**Failure mode of `last_chk` itself:** if `last_chk` returns `Err(Transport(_))` or `Err(Decode(_))`, fall through to the underlying error's policy — Transport → keep doc in `Sent` + audit `BOOT_LAST_CHK_TRANSPORT_RETRY` (per §4.0 the counter is NOT touched here because `last_chk` is a probe, not a wire submit; budget will advance only when a subsequent stage_send re-drive occurs); Decode → escalate to `RequiresManualReconciliation` per §2 main Decode rule (no bounded retry on Decode).

### 4.6 KVT1 — passive hold; KVT2 polling deferred to M3b (HIGH 6 fix — option A)

**Protocol verification finding:** the current Rust `DpsChannel` trait (`rust/prro/src/transports/dps/channel.rs:19-38`) exposes 5 methods — `send_chk`, `last_chk`, `ping`, `status_rro`, `info_rro`.  None of them carries the **second receipt (KVT2)** as a documented field:
- `last_chk` returns `CheckAck` (`id: String`, `id_sign: Vec<u8>`, `data_sign: Vec<u8>`) — the first-receipt-equivalent ack.
- `status_rro` returns `StatusSnapshot { open_shift, online, last_signer }` — RRO-wide state, no per-doc receipts.
- `info_rro` returns `RroInfo` — RRO + operator listing, no receipts.

There is **no Rust API today for polling KVT2** specifically.  Python's `reconciliation.py:60-74` `poll_status` is the conceptual equivalent, but the Rust port hasn't landed it yet.

**M3a binding decision: defer active KVT2 polling to M3b.**  W9 boot recovery for `Kvt1` docs is **passive**: keep doc in `Kvt1`, emit a forensic audit, do not call DPS, do not advance state.  Operator-driven manual reconciliation is the M3a escape hatch for stuck-Kvt1 docs.

| Field | Value |
|---|---|
| Helper | None (no new module).  Inline check inside `boot_phase::run_boot_reconciliation` |
| Action | No DPS call.  Single audit row INSERT (no state mutation, no transport_trace, no document_files write).  Doc stays in `Kvt1`.  Operator decides whether to escalate to `RequiresManualReconciliation` via the admin CLI (out of W9 scope) |
| Whitelist transitions invoked | **None** — doc is observed-only |
| Whitelist transitions NOT invoked | `Kvt1 → Kvt2` (deferred to M3b active polling); `Kvt1 → ErrorRetryable` (deferred — Kvt1 is not a wire-failure state in M3a); `Kvt1 → Rejected` (intentional gap per W0-1 §2.1) |
| Audit | `BOOT_KVT1_HOLD_DEFERRED` INFO; payload `{"document_id": "...", "branch": "c-kvt1", "deferred_to": "M3b active KVT2 polling"}` |
| Counter | NOT touched.  `attempts_used(doc_id)` aggregate unchanged (no transport_trace row created) |
| Python parity | `reconciliation.py:60-74` `poll_status` — **not yet ported to Rust**; deliberate M3a scope carve-out |
| WebCheck parity | Not applicable (WebCheck doesn't have two-receipt protocol) |
| Acceptance fixture (§10.1) | "Pre-seed doc in `Kvt1` state; run `reconcile_pending`; assert state STILL `Kvt1` post; assert no `transport_trace` row added; assert one `BOOT_KVT1_HOLD_DEFERRED` audit row; assert `document_files` for Kvt2Raw absent" |

**Why this is safe for fiscal correctness:** A doc in `Kvt1` is server-acknowledged at the protocol level (per W0-1 §2.1 "KVT1 means first receipt is persisted").  It is NOT a duplicate-send risk and NOT a fiscal-chain integrity risk.  The only operational concern is that the second receipt may arrive minutes/hours later, and the dashboard will show "N docs stuck in Kvt1".  This is acceptable for M3a because (a) the count is operator-visible via the audit, (b) M3b will land active polling, (c) manual reconciliation is the supported escape hatch.

**Why this is NOT a regression vs WebCheck:** WebCheck has no two-receipt protocol equivalent (per §15 row "Per-FN lease" — WebCheck's protocol is single-receipt).  M3a's KVT1 hold is forward-looking infrastructure that WebCheck never had to address.

### 4.7 KVT2 — re-drive forward to ACK (no DPS query)

| Field | Value |
|---|---|
| Action | Call `services::write_path::stage_finalize::run(pool, doc_id, ...)` — the existing W8 entry point.  Stage_finalize CAS Kvt2 → Ack + advance `node_state.last_known_unsigned_xml_sha256` + ingress_inbox DONE + outbox INSERT + audit — all in one `with_immediate` envelope, exactly as live KVT2 → ACK would have done it pre-crash |
| Whitelist transitions invoked | `Kvt2 → Ack` (whitelist :97) — the ONLY legal transition out of Kvt2 |
| Whitelist transitions NOT invoked | `Kvt2 → ErrorRetryable` — **intentional gap** (W0-1 §2.1: "Kvt2 recovery re-drives forward to Ack only") |
| Audit | Inherits W8 `STAGE_FINALIZE_ACK` audit |
| Python parity | Implicit in the Python finalize path (no separate KVT2 recovery; Ack is the same call) |
| WebCheck parity | Not applicable |

**Why no DPS query:** KVT2 is terminal at the server (per W0-1 §3.5); the only missing thing is the local Ack transition + finalize bookkeeping.  The wire round-trip already happened.

### 4.8 ERROR_RETRYABLE — re-drive via stage 3 or stage 4 entry per artifact presence

| Field | Value |
|---|---|
| Pre-check | Call `transport_trace::attempts_used(pool, doc_id)` (per §4.0).  If `attempts_used >= MAX_BOOT_ATTEMPTS` (= 5) → transition `ErrorRetryable → RequiresManualReconciliation` (whitelist :101); audit `BOOT_RETRY_BUDGET_EXHAUSTED` ERROR; payload `{"document_id": "...", "attempts_used": N, "branch": "c-error-retryable-terminal"}`.  Do NOT call stage_send / stage_sign |
| Action (budget remains) | Check `document_files::has(doc_id, SIGNED_XML)`.  If SIGNED_XML row exists → call `stage_send::run(pool, dps_channel, doc, ctx)` — Pattern B 4-pre CAS accepts `ErrorRetryable → Sending` (W10 whitelist :88).  If SIGNED_XML row missing → call `stage_sign::run(pool, ctx, doc)` to re-sign (Prepared/Signed return) then stage_send |
| Whitelist transitions invoked | `ErrorRetryable → Sending` (W10); subsequent W10 dispatch.  Or `ErrorRetryable → RequiresManualReconciliation` (budget exhausted) |
| Whitelist transitions NOT invoked | `ErrorRetryable → Kvt1` (reserved for direct last_chk paths — NOT used here); `ErrorRetryable → Sent` (still in whitelist for backward compat; M3a DPS code MUST NOT use it — re-introduces duplicate-send hazard) |
| MAC recovery interaction | If `doc.mac_recovery_attempts == 1` (W10 single-bit budget already burned), stage_send's W10 dispatch surfaces `CounterExhausted` on a second -12 ⇒ override to `Rejected`.  W9 does NOT need special handling here — W10 dispatch handles it natively |
| Audit | Inherits W7/W10 stage_send audits.  W9 adds `BOOT_ERROR_RETRYABLE_REDRIVEN` INFO at the entry (before stage_send call) carrying `{"document_id": "...", "attempts": N, "redrive_path": "Signed|Re-sign+Signed"}` |
| Python parity | `reconciliation.py:200-256` `poll.retryable` branch |
| WebCheck parity | **Decompiled cross-ref:** `docs/webcheck_reverse/WebCheckMain/WebCheck/SubmitPtr.cs:50-76` — WebCheck's `All.Retries` outer loop (operator-configurable, typically 7) + inner `-3 ERROR_SAVE` 7-iteration loop with 333ms sleeps.  Two compounding loops = up to 49 wire calls per failing submit, with no operator visibility into the retry state mid-loop.  M3a's `transport_trace.attempt_no` counter (max `MAX_BOOT_ATTEMPTS = 5` per §4.0) is the deliberate replacement: bounded, persistent in W7's existing migration 010, operator-readable.  Each W9 stage_send re-drive allocates `attempt_no = prev + 1`; on the 6th attempt (`attempts_used >= 5`) escalation to `RequiresManualReconciliation` is deterministic |

### 4.9 Terminal-state exclusions — no-op (per §3.1)

These 5 DocStates are reachable in the pending set OR via prior reconciliation; W9 explicitly does NOTHING:

| DocState | W9 action | Reason |
|---|---|---|
| `Ack` | No-op | Terminal success.  W9 audit: none (already-Ack is the normal terminal state for finalized docs and producing a noisy audit per such doc would flood the log on every boot) |
| `Rejected` | No-op | Terminal failure |
| `Cancelled` | No-op | Operator/system cancellation |
| `OfflineLocalAck` | No-op (M3b worker owns these) | Out of M3a scope per §3.1; the offline_sync_service has its own state machine |
| `RequiresManualReconciliation` | No-op | Operator-driven; auto-re-drive would lose the escalation signal.  W9 audit: **single** `BOOT_MANUAL_RECON_PENDING_HISTOGRAM` INFO at end of per-FN loop carrying the count, NOT per-doc — operator dashboard sees the number, log is not flooded |

**Implementation note:** `list_pending_for_fn` per its current contract (`fiscal_documents.rs:269`) returns ONLY pending states (PREPARED .. ERROR_RETRYABLE + the new SENDING per W7), so `Ack`/`Rejected`/`Cancelled` should not appear.  `OfflineLocalAck` and `RequiresManualReconciliation` *might* — verify against `fiscal_documents.rs:182-185` actual exclusion list in implementation.

---

## 5. Pre-flight contracts (App::boot expansion)

This section nails down `App::boot`'s post-W9 shape with line-precise responsibilities.

### 5.1 Singleton lock acquisition (NEW in W9)

```rust
pub async fn boot(config: AppConfig) -> Result<Self, BootError> {
    // (existing: parent dir create)
    if let Some(parent) = config.database.db_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| BootError::Internal(format!("create parent dir: {e}")))?;
        }
    }

    // (NEW) singleton — two `serve` processes against the same DB are forbidden.
    // HIGH 3 fix: `runtime::singleton::acquire` is sync `pub fn` (file-descriptor
    // advisory lock; no async work).  Returns `anyhow::Result<PidLock>`; we wrap
    // into BootError::Internal.  PidLock is stored on Inner and released on drop.
    let singleton = runtime::singleton::acquire(&config.database.db_path)
        .map_err(|e| BootError::Internal(format!("singleton lock: {e}")))?;

    // (existing) pool open + migrations to head (open_pool runs migrations internally).
    let db = crate::db::open_pool(&config.database.db_path).await
        .map_err(|e| BootError::Database(sqlx::Error::Configuration(Box::new(e))))?;

    // (NEW) integrity probe — fail-closed BEFORE any FN-row write
    // (LOW 3-safe shape; cycle-7 LOW 1: use `query_scalar` for single-column
    // result instead of tuple-struct `query_as` — simpler ergonomics, same
    // SQL semantics).
    let rows: Vec<String> = sqlx::query_scalar("PRAGMA quick_check(1)")
        .fetch_all(&db).await
        .map_err(BootError::Database)?;
    let reason = match rows.as_slice() {
        [s] if s == "ok" => None,
        [first, ..] => Some(first.clone()),
        [] => Some(String::from("quick_check returned zero rows")),
    };
    if let Some(reason) = reason {
        tracing::error!(target: "prro::boot", quick_check = %reason, "DB_INTEGRITY_CHECK_FAILED");
        return Err(BootError::IntegrityCheckFailed { reason });
    }

    // NIT 1 fix: field named `singleton` (NOT `_singleton`) — the underscore
    // prefix in Rust signals "intentionally unused", but this field IS
    // load-bearing via RAII: dropping it on App drop releases the file
    // advisory lock.  If clippy complains about dead_code, add
    // `#[allow(dead_code)]` on the field; the field IS used by Drop.
    Ok(Self {
        inner: Arc::new(Inner { config, db, singleton }),
    })
}
```

**Exit ordering on error:**
- Singleton acquire fails → return Err immediately; no pool ever opened.
- `open_pool` fails → singleton drops on return; no pool to clean up.
- `quick_check` fails → pool is open (`PRAGMA` already ran on it), singleton is held, BUT no writes have happened.  Pool drops on return.  Singleton drops on return.

This ordering is what fixture #8 (§9.1) exercises end-to-end.

### 5.2 Migrations runner (UNCHANGED in W9)

Migrations run inside `db::open_pool` per current M1 contract; W9 does not move them.  W9 only adds quick_check AFTER `open_pool` returns successfully.

### 5.3 `quick_check` fail-closed semantics

**Hard contract:**

| Aspect | Requirement |
|---|---|
| Probe shape | `PRAGMA quick_check(1)` via `query_scalar::<String>().fetch_all()` (LOW 3 fix + cycle-7 LOW 1 refinement — `query_scalar` is the canonical sqlx call for single-column result; previous draft used `query_as::<(String,)>` which is functionally equivalent but uses tuple-struct ergonomics unnecessarily).  Explicit `(1)` cap on error rows — SQLite's default cap of 100 rows can return up to 100 error strings on heavily corrupt DBs; for fail-closed semantics we only need the *first* error (any failure → refuse boot).  `fetch_all` instead of `fetch_one` is defensive against schema-tool quirks where the PRAGMA might emit zero rows on certain corruption shapes (interpret zero rows as `"unknown"` and fail-closed).  No `PRAGMA integrity_check` (slower, same surface as `quick_check` for our use). |
| Success criterion | Exactly one row returned with content `"ok"` (3 chars).  Zero rows → fail-closed (treat as unknown corruption).  Multi-row → fail-closed (any row ≠ "ok" → take the first non-ok as reason).  Not `starts_with("ok")`, not case-insensitive. |
| Failure shape | Return `BootError::IntegrityCheckFailed { reason }` where `reason` is the entire returned string. |
| Side effects on failure | **ZERO writes** to any table.  CRITICAL log line via `tracing::error!` (NOT into `audit_log` — writing to a corrupt DB is a footgun).  Process exits non-zero from `main.rs`. |
| Health endpoint state | `/health/startup` returns 503 with `{"reason": "DB_INTEGRITY_CHECK_FAILED", "details": "..."}` — wired via the in-memory `BootError` carried up to the health handler.  M3a impl detail: health handler reads a `OnceLock<Option<BootError>>` populated by `main.rs`. |

### 5.4 Sequential per-FN iteration (HIGH 4 fix — no phantom lease helper)

```rust
pub async fn reconcile_pending(&self) -> Result<ReconciliationSummary, BootError> {
    let pool = self.db();
    let fns = fiscal_number_config::list_all(pool).await
        .map_err(BootError::Database)?;
    let mut summary = ReconciliationSummary::default();
    // Sequential per-FN iteration.  Single-writer invariant is enforced by
    // SQLite BEGIN IMMEDIATE (per W3) + this loop being single-threaded.
    // No separate per-FN lease module — see §2.5 for rationale.
    for fn_cfg in &fns {
        services::reconciliation::boot_phase::run_boot_reconciliation(
            pool, &fn_cfg.fiscal_number, &mut summary,
        ).await?;
    }
    Ok(summary)
}
```

**Concurrency model:** W9 boot recovery is **single-threaded sequential** over `fiscal_number_config::list_all` (ordered by `fiscal_number` ascending — deterministic for testing).  Each per-FN `run_boot_reconciliation` call may issue multiple `with_immediate` writes; the global `BEGIN IMMEDIATE` serialisation ensures these writes don't interleave with each other or with any ingress writer running on a different thread.

### 5.5 Exit code mapping (`main.rs`)

| `BootError` variant | Exit code | Convention |
|---|---|---|
| `IntegrityCheckFailed { .. }` | 65 | `EX_DATAERR` — DB is the data; it's corrupt |
| `OfflineModeRefusal { .. }` | 78 | `EX_CONFIG` — configuration (operational state) requires M3b which isn't enabled |
| `Database(_)` | 71 | `EX_OSERR` — IO/SQLite operational failure |
| `Internal(_)` | 70 | `EX_SOFTWARE` — internal invariant violation |

Wired in `main.rs` via a `From<BootError> for ExitCode` impl.

---

## 6. Post-conditions (verbatim from §4.4 with this freeze's explicit assertion shapes)

After `App::reconcile_pending` returns `Ok(_)` for all configured FNs:

| # | §4.4 invariant | Concrete assertion shape (used in fixture acceptance) |
|---|---|---|
| 1 | No FN row had `shift_state` silently masked | For every `fn_id` where pre-condition had `shift_state ∈ {Opening, Opened, Closing, Closed}` AND `mode == Online`, post-state `shift_state` is identical OR has advanced via legal whitelist (Opening→Opened or Closing→Closed) AS SIDE EFFECT of a doc transition (e.g. SHIFT_OPEN→Kvt2→Ack runs `_apply_shift_side_effects_locked`). **`upsert_initial` never observed in fixtures (b)/(c)/(e)/(f) via provider spy.** |
| 2 | Every pending doc advanced or `transport_trace.attempt_no` advanced + audit | For every doc in pre-state `list_pending_for_fn`, either: (a) doc transitioned per its source-state rule and W9 emitted (or stage worker emitted) the appropriate audit row, OR (b) `attempts_used(doc_id)` increased by at least 1 since pre-state (W9 re-drove via stage_send) and a `BOOT_*_RETRY` audit row exists, OR (c) doc stayed in source state with no new attempt (probe-only paths per §4.5/§4.6 transient failures) and a probe-class audit row exists |
| 3 | No FN's `next_lnd` decremented or reset | For every FN, post `next_lnd >= pre next_lnd`.  (Equality OR forward advance; never backwards.) |
| 4 | `node_state.last_known_unsigned_xml_sha256` unchanged unless KVT2→Ack | For every FN where no doc reached `Kvt2 → Ack` transition during boot, `last_known_unsigned_xml_sha256` is byte-identical pre/post.  Where one or more docs reached `Kvt2 → Ack`, post-value matches the `unsigned_xml_sha256` of the highest-lnd doc that finalized. |
| 5 | Health gates: `live → startup_complete → ready` | Once `App::reconcile_pending` returns Ok, `startup_complete = true`; the ingress shells (REST/XMLRPC/Maria) are wired to set `ready = true` only when they actually start listening.  `live` is set during process bootstrap and unaffected by W9 |

---

## 7. Idempotency contract (§4.5)

Running `reconcile_pending` twice in immediate succession against the same DB MUST produce:

| Pre-state of FN | First run | Second run (must observe) |
|---|---|---|
| FN absent | (a) `upsert_initial` + audit | (b) idempotent no-op + audit |
| FN+ONLINE+no-pending | (b) no-op + audit | (b) no-op + audit (audit duplicated; that's expected — each boot emits its own forensic event) |
| FN+ONLINE+pending | (c) per-doc transitions (some advance, some increment counter) | (c) per-doc transitions (those that advanced are now in terminal/next state and don't appear in `list_pending_for_fn`; those that incremented advance further OR hit terminal escalation) |
| FN+OFFLINE | (d) refuse + Err | (d) refuse + Err (repeat refusal is idempotent) |
| FN+(e)(e1) mid-transition with doc | (e1) per-doc via (c) | (b) if doc finalized; (c) if still pending — both legitimate |
| FN+(e)(e2) orphan no-doc | (e2) shift→Error + node_state.shift_state→Closed + CRITICAL audit (HIGH 10 fix — targeted UPDATE preserves the §3.5.2 idempotency claim) | (b) — orphan resolved (shifts.state=Error); shift_state=Closed → dispatches to (b) idempotent no-op |
| FN+Blocked/StopMode/CryptoDegraded | (f) preserve + audit | (f) preserve + audit |

**Counter preservation:** `transport_trace.attempt_no` is monotonic and never decremented (per §4.0).  Second boot's `attempts_used(doc_id)` is `>=` first boot's; if first boot left a doc in `ErrorRetryable` with attempts_used = 4, and the second boot re-drove via stage_send (creating attempt_no = 5), the third boot will escalate to `RequiresManualReconciliation` via the §4.8 pre-check.  Audit log accumulates across boots (no deduplication).

**Acceptance fixture (§9.1 #9):** run `reconcile_pending` twice; for previously-completed FNs assert (b) observed; for refused FNs assert (d) observed; for the docs that incremented counters, assert the counter advanced.

---

## 8. State-machine table — all 13 DocStates × W9 action × audit

| DocState | In `list_pending_for_fn`? | W9 action | Audit kind | §4 subsection |
|---|---|---|---|---|
| `Prepared` | yes | `stage_sign::run` (re-sign) | inherits W6 | §4.1 |
| `Signed` | yes | `stage_send::run` (Pattern B from Signed) | inherits W7+W10 | §4.2 |
| `Encrypted` | yes | route to ErrorRetryable chain (1-tick deferral — §4.8 fires on next boot tick) | `BOOT_ENCRYPTED_REROUTED` WARN tick #1, then `BOOT_ERROR_RETRYABLE_REDRIVEN` / `BOOT_RETRY_BUDGET_EXHAUSTED` tick #2 | §4.3 |
| `Sending` | yes (since W7 added it to the pending set) | `boot_phase::resume_sending_to_error_retryable` (CAS to ErrorRetryable) | `BOOT_RESUME_SENDING_TO_ERROR_RETRYABLE` ERROR | §4.4 |
| `Sent` | yes | `last_chk_probe::probe` (3-way routing) | `BOOT_LAST_CHK_*` | §4.5 |
| `Kvt1` | yes | **Passive hold** — no DPS call; inline audit only.  Active KVT2 polling deferred to M3b (HIGH 6 fix option A; verification: no Rust DpsChannel API exposes KVT2 today) | `BOOT_KVT1_HOLD_DEFERRED` INFO | §4.6 |
| `Kvt2` | yes | `stage_finalize::run` (advance to Ack) | inherits W8 | §4.7 |
| `OfflineLocalAck` | NO (excluded per `fiscal_documents.rs:184`) | M3b worker owns; W9 no-op | none (M3b worker audits) | §4.9 |
| `Rejected` | NO | no-op | none | §4.9 |
| `Cancelled` | NO | no-op | none | §4.9 |
| `ErrorRetryable` | yes | per-budget: re-sign+stage_send OR escalate | `BOOT_ERROR_RETRYABLE_REDRIVEN` / `BOOT_RETRY_BUDGET_EXHAUSTED` | §4.8 |
| `RequiresManualReconciliation` | NO (excluded) | no-op | `BOOT_MANUAL_RECON_PENDING_HISTOGRAM` ONCE per FN loop | §4.9 |
| `Ack` | NO | no-op | none | §4.9 |

**Exhaustive coverage proof:** 13 DocStates × this table = each row has exactly one canonical action.  Integration test `per_docstate_dispatch_exhaustive_matrix` (in `tests/app_boot_reconciliation.rs` per LOW 2 layer correction) enumerates all 13 and asserts the dispatched call (via spies on stage_sign / stage_send / stage_finalize / `last_chk_probe::probe` / `advance_sent_to_kvt1_from_probe` / `passive_hold_kvt1` / `resume_sending_to_error_retryable`).  No `status_rro_probe` spy — KVT2 polling is deferred to M3b per §4.6.

---

## 9. Files map (verbatim from Plan Task 10 metadata + this freeze additions)

```
NEW:
  rust/prro/src/services/reconciliation/mod.rs              (~80 LoC)
  rust/prro/src/services/reconciliation/boot_phase.rs       (~600-800 LoC — 6-branch dispatch + advance_sent_to_kvt1_from_probe + resume_sending_to_error_retryable)
  rust/prro/src/services/reconciliation/last_chk_probe.rs   (~150 LoC — SENT recovery, per §4.5)
  rust/prro/tests/app_boot_reconciliation.rs                (~800-1000 LoC; §10.1 fixtures including branch-partition + per-branch dispatch matrices moved from §10.3 per LOW 2)
  rust/prro/tests/app_boot_quick_check_failure.rs           (~120 LoC; integrity probe scenarios)

MODIFIED:
  rust/prro/src/app.rs                                      (+60 LoC: BootError enum; Inner struct extended with `singleton: PidLock` field (MED 5 fix; NIT 1 — no underscore prefix, the field IS load-bearing via Drop); singleton + quick_check pre-flight in App::boot; new App::reconcile_pending method)
  rust/prro/src/services/mod.rs                             (+1 line: pub mod reconciliation;)
  rust/prro/src/db/repositories/transport_trace.rs          (+30 LoC: pub async fn attempts_used per §4.0 + pub async fn complete_via_recovery_tx per §4.5 HIGH 9 fix)
  rust/prro/src/main.rs OR rust/prro/src/bin/serve.rs       (+10 LoC: ExitCode mapping; runtime call site app.reconcile_pending() before serving)
```

**No migration needed.**  All required schema (`fiscal_documents`, `node_state`, `shifts`, `audit_log`, `transport_trace` incl. `attempt_no` per W7 migration 010, `document_files`, `fiscal_number_config`, `outbox`) exists from W1-W10.  W9 introduces NO schema changes — durable counter sits on existing `transport_trace.attempt_no`.

---

## 10. Test plan

W9 ships 4 test crates + lib-test additions.

### 10.1 `tests/app_boot_reconciliation.rs` — 9 §9.1 fixtures + per-DocState matrix

Fixtures #1-9 verbatim from §9.1 (this freeze §3 columns "Acceptance fixture" — copy each into the test as `#[tokio::test] async fn fixture_N_<branch>_<assertion>()`).

Additional per-DocState dispatch fixtures (§4.1-4.9): one per DocState, total 13.  Each:

```rust
#[tokio::test]
async fn per_docstate_<state>_dispatches_to_<helper>_with_correct_args() {
    let pool = setup_test_pool().await;
    seed_doc(&pool, &fn_id, DocState::<State>, ...).await;
    let spy = StageWorkerSpy::new();  // counts stage_sign / stage_send / stage_finalize / last_chk_probe calls
    boot_phase::run_boot_reconciliation(&pool, &fn_id, &mut summary).await.unwrap();
    assert_eq!(spy.<helper>_calls(), 1);
    // For state-specific assertions: doc transition, audit row, attempts_used(doc_id) post-state, ...
}
```

Plus the branch-partition exhaustive matrix (§3.7).

**Total fixtures in this crate (updated post-finding-fixups):**
- 9 base §9.1 fixtures (#1-9)
- 1 NEW: #5-strict per HIGH 1 fix (branch e1 with shift_state=Opening + matching pending doc)
- 1 NEW: #6-bis per HIGH 10 fix (cycle-5; (e2) idempotency — second boot dispatches to (b))
- 13 per-DocState dispatch fixtures
- 1 branch-partition exhaustive matrix (moved from §10.3 per LOW 2)
- 6 per-branch dispatch fixtures with spy injection (one per branch a/b/c/d/e1/e2/f; moved from §10.3 per LOW 2)
- 3 sub-fixtures for branch (d) over `{Offline, GoingOffline, GoingOnline}`
- 3 sub-fixtures for branch (f) over `{Blocked, StopMode, CryptoDegraded}`

**Total: 37 fixtures** (up from earlier 36; growth driven by HIGH 1 + LOW 2 + HIGH 10 fixes).

### 10.2 `tests/app_boot_quick_check_failure.rs`

**Note on `BootError` matching shape (LOW 1):** `BootError::Database(sqlx::Error)` wraps a non-`Clone`, non-`PartialEq` type.  Fixture assertions MUST use `assert_matches!` (or `if let` patterns), NOT `assert_eq!` — e.g. `assert_matches!(result, Err(BootError::IntegrityCheckFailed { reason }) if !reason.is_empty())`.  Tests use the `assert_matches` crate (already a dev-dep per existing W7-W10 tests) or hand-written `match` blocks.


| # | Fixture | Setup | Acceptance |
|---|---|---|---|
| 1 | `quick_check_fail_returns_typed_error` | DB file with truncated mid-page (use a fixture from `tests/fixtures/corrupt_db_truncated.bin`) | `App::boot` returns `Err(BootError::IntegrityCheckFailed { reason })`; `reason` is non-empty |
| 2 | `quick_check_fail_emits_no_writes_to_node_state` | Same as #1; pre-seed `node_state` with two rows (assert table state pre vs post identical) | Post-boot, `node_state` table contents byte-identical via raw SELECT |
| 3 | `quick_check_fail_emits_no_writes_to_audit_log` | Same setup; pre-seed `audit_log` with one row | Post-boot, `audit_log` count unchanged |
| 4 | `quick_check_fail_emits_no_writes_to_shifts` | Same setup; pre-seed `shifts` with one row | Post-boot, `shifts` table byte-identical |
| 5 | `quick_check_fail_critical_log_line_emitted` | Same setup; capture `tracing::error!` via `tracing_test` subscriber | One log line at ERROR level with target `prro::boot` and message containing `DB_INTEGRITY_CHECK_FAILED` |
| 6 | `quick_check_ok_proceeds_to_reconcile` | Clean DB | `App::boot` returns Ok; subsequent `app.reconcile_pending()` succeeds |

**Total fixtures in this crate:** 6.

### 10.3 Lib tests (per-helper unit) — LOW 2 scope correction

Lib-test surface is restricted to **isolated helper logic** that doesn't need a real `SqlitePool` + multi-table seed.  Dispatch tests and partition-exhaustive matrix require integration-crate scope (real pool, real audit_log capture, real spy injection) — they live in §10.1 `tests/app_boot_reconciliation.rs`, NOT in lib-test modules.

**Lib-tests order matches §12 W9.2 helper list (NIT 2 fix — alphabetical by module then symbol):**

| Helper | File | Tests | Layer |
|---|---|---|---|
| `transport_trace::attempts_used` aggregate (§4.0, HIGH 2-fix) | `db/repositories/transport_trace.rs` (tests module) | (a) Zero rows → returns 0; (b) Three rows attempt_no = {1,2,3} → returns 3; (c) Multi-doc rows → query returns only the target doc's max | lib-unit |
| `transport_trace::complete_via_recovery_tx` (HIGH 9-fix) | `db/repositories/transport_trace.rs` (tests module) | (a) In-flight row exists → row completed with `outcome_kind = 'OK'`, `server_fiscal_no` set, `wire_call_started_at`/`wire_call_finished_at` preserved (read pre/post, assert byte-identical); (b) Row already completed (re-entry) → rows_affected = 0 → typed Err for caller to detect; (c) Row missing → rows_affected = 0 → typed Err | lib-unit |
| `boot_phase::advance_sent_to_kvt1_from_probe` (MED 1 + HIGH 5 + HIGH 8 + HIGH 9-fixed) | `services/reconciliation/boot_phase.rs` (tests module) | (a) Applied path: Sent → Kvt1 + `document_files(Kvt1Raw)` persisted with `ack.data_sign` bytes + transport_trace completed via `complete_via_recovery_tx` (outcome_kind = Ok, original wire times preserved) + audit `BOOT_LAST_CHK_MATCH_KVT1`; (b) Doc not in Sent state → CAS Conflict typed error; (c) transport_trace row missing for the doc → typed error (covers crash window where stage_send committed Sent but trace row absent — defensive); (d) Second invocation on already-completed row → idempotent re-entry (no double-audit, no double-write) | lib-unit (real pool, single-helper) |
| `boot_phase::passive_hold_kvt1` (HIGH 6-fixed) | `services/reconciliation/boot_phase.rs` (tests module) | (a) Doc in Kvt1 → single `BOOT_KVT1_HOLD_DEFERRED` audit row INSERT; (b) doc state unchanged post; (c) no `transport_trace` row created; (d) no `document_files` write | lib-unit (real pool, single-helper) |
| `boot_phase::resume_sending_to_error_retryable` | `services/reconciliation/boot_phase.rs` (tests module) | (a) Applied path: doc Sending → ErrorRetryable + audit; (b) CAS Forbidden (doc not in Sending — race) returns typed error; (c) NotFound returns typed error | lib-unit (uses real pool but isolated single-helper scope) |
| `last_chk_probe::probe` outcome classification | `services/reconciliation/last_chk_probe.rs` (tests module) | (a) match → `ProbeOutcome::Match`; (b) mismatch → `ProbeOutcome::Mismatch`; (c) NotFound → `ProbeOutcome::NotFound`; (d) Transport error → `ProbeOutcome::TransportRetry`; (e) Decode error → `ProbeOutcome::DecodeEscalate` — **outcome classification only**, NOT the downstream state mutation | lib-unit (uses mocked DpsChannel; no DB) |

**Branch dispatch + partition-exhaustive matrix tests are integration-shaped** and live in §10.1 (`tests/app_boot_reconciliation.rs`).  Specifically: 1 `branch_partition_exhaustive_matrix` integration test enumerating all `(mode, shift_state, has_pending_doc)` triples + 1 per-branch dispatch fixture verifying the correct helper invoked via spy.

**Total lib tests added by W9:** ~22 (updated count after HIGH 9 added `complete_via_recovery_tx`: 3 resume_sending + 3 complete_via_recovery_tx + 4 advance_sent_to_kvt1_from_probe + 4 passive_hold_kvt1 + 3 attempts_used + 5 last_chk_probe outcomes = 22).  Dispatch matrix moved to integration crate per LOW 2 fix.

### 10.4 Integration golden suite

Existing test crates that depend on `App::boot` shape:

- `tests/app_boot.rs` (5 tests today): verify the existing 5 tests still pass with the W9-extended `App::boot`.  If they probed for the "pool + migrations only" shape, they need updating to also exercise the new singleton + quick_check path (without exercising reconcile_pending — that's the separate crate).

**Expected delta:** 5/5 still green after adjusting to new `BootError` return type if needed.

---

## 11. Frozen invariants — carry-forward from W1–W10

W9 MUST preserve every invariant landed in prior slices.  Explicit list with one-line attestation each:

| # | Invariant | W9 attestation |
|---|---|---|
| I1 | No foreign IO inside `with_immediate` | W9 envelopes: `resume_sending_to_error_retryable` (single CAS + audit, no IO); `advance_sent_to_kvt1_from_probe` (CAS + Kvt1Raw persist + trace complete + audit, all DB-only); `passive_hold_kvt1` (single audit INSERT, no IO).  The one probe helper (`last_chk_probe::probe` calling `DpsChannel::last_chk`) — wire call lives OUTSIDE any `with_immediate` block; only the routing-write (`transition_state` + audit) is inside.  W3 scanner extended to cover the new `services/reconciliation/` module |
| I2 | Single-writer per FN | W9 per-FN lease acquired around the entire decision-tree body |
| I3 | `WriteTxConn<'_>` sealed newtype | Untouched; W9 uses existing helper signatures |
| I4 | Idempotency mandatory | §7 idempotency contract; fixture #9 |
| I5 | Offline respects time + code limits | Branch (d) refuses boot when OFFLINE-on-restart; M3a is ONLINE-only |
| I6 | Adapters build full canonical payloads | Untouched (W9 doesn't touch adapters) |
| I7 | `schema_version` on canonical envelopes | Untouched |
| I8 | Recovery does not silently violate state transitions | Every per-DocState rule (§4.1-4.8) maps to a single whitelist edge; intentional gaps (Prepared→ErrorRetryable, Kvt1→Rejected, Kvt2→ErrorRetryable, Signed→Rejected, Sent→Sending direct) preserved by NOT being invoked anywhere in W9 code |
| I9 | Graceful shutdown matters | `App::boot` failure is non-zero exit; singleton lock drops on App drop; quick_check failure short-circuits cleanly |
| I10 | Checkbox-compatible flows bypass local signing only by explicit profile | Untouched (W9 doesn't touch profile selection) |
| W7-NEW | Forensic trace `transport_trace.completed_at IS NULL` is boot-recovery signal | W9 SENDING recovery (§4.4) makes the trace row queryable; doc becomes ErrorRetryable; operator-visible |
| W8-NEW | `stage_finalize::run` reads fiscal_number + request_id from doc row | W9's KVT2 recovery calls `stage_finalize::run(pool, doc_id, ...)` — same contract |
| W8-NEW | Chain-continuity guard inside finalize tx | Inherited by W9's KVT2 recovery path |
| W8-NEW | `mark_done_tx` requires `WHERE status = 'PROCESSING'` | Inherited via stage_finalize call |
| W10-NEW | `error_routing::route_send_result` is single source of truth | W9's ErrorRetryable re-drive goes through stage_send which consumes W10 dispatch internally |
| W10-NEW | MAC recovery in-stage AT MOST ONCE per stage_send call | W9 does NOT call mac_recovery directly; the bound is per stage_send invocation |
| W10-NEW | `Online → Blocked` flip mandatory on Server -11 | W9 re-drives ERROR_RETRYABLE via stage_send → if the next attempt receives -11 again, the flip happens then (NOT in W9 directly) |

---

## 12. Slice breakdown — W9.1 .. W9.4 implementation plan

W9 implementation lands in 4 sub-slices, each ending with `cargo fmt + clippy + targeted tests + self-review` before next slice begins.

### W9.1 — Pre-flight pipeline (singleton + quick_check + BootError)

**Surface:** `app.rs` only.

**Files modified:** `rust/prro/src/app.rs`, `rust/prro/src/main.rs` (or `bin/serve.rs`).

**Tests:** `tests/app_boot_quick_check_failure.rs` (6 fixtures).  Existing `tests/app_boot.rs` regression-verified.

**Acceptance:**
- `BootError` enum lands with the 4 variants.
- `App::boot` acquires singleton, runs migrations (via `open_pool`), runs `PRAGMA quick_check`, returns typed error on failure.
- 6 quick_check fixtures green.
- `app_boot.rs` 5/5 still green.

**Stop criteria:** before slice W9.2, run senior-grade review on `app.rs` + the new error type.

### W9.2 — `services/reconciliation` module + per-DocState helpers

**Surface:** new `services/reconciliation/{mod,boot_phase,last_chk_probe}.rs` + `db/repositories/transport_trace.rs` (helper extension).

**Files added/modified:** the three new files above + `services/mod.rs` re-export + `transport_trace.rs` helper extension.

**Helpers landed in this slice (alphabetical by module then symbol; NIT 2 fix for consistent ordering — §10.3 lib-tests table sorts identically):**

In `db::repositories::transport_trace`:
- `attempts_used(pool, doc_id) -> sqlx::Result<i64>` (HIGH 2 fix, §4.0) — single SQL aggregate returning `COALESCE(MAX(attempt_no), 0)`.  Always non-NULL `i64`; caller does NOT need `Option<i64>`.
- `complete_via_recovery_tx(tx, doc_id, attempt_no, server_fiscal_no) -> sqlx::Result<()>` (HIGH 9 fix, §4.5) — completes an in-flight trace row preserving original `wire_call_started_at`/`wire_call_finished_at`; idempotent via `WHERE completed_at IS NULL`.

In `services::reconciliation::boot_phase`:
- `advance_sent_to_kvt1_from_probe(pool, doc, ack) -> Result<(), StageError>` (MED 1 + HIGH 5 + HIGH 8 + HIGH 9 fix) — single `with_immediate` envelope: CAS `Sent → Kvt1` + persist `Kvt1Raw` via `ack.data_sign.clone()` + `complete_via_recovery_tx` with `OutcomeKind::Ok` + audit.
- `passive_hold_kvt1(pool, doc_id) -> Result<(), StageError>` (HIGH 6 fix option A) — single `with_immediate` envelope: audit-only INSERT, no state mutation, no transport_trace write.
- `resume_sending_to_error_retryable(pool, doc_id) -> Result<(), StageError>` — single `with_immediate` envelope: CAS `Sending → ErrorRetryable` + audit.
- Stub `run_boot_reconciliation(pool, fn_id, summary)` returning `Ok(())` (decision-tree wired in W9.3).

In `services::reconciliation::last_chk_probe`:
- `probe(ctx, doc) -> ProbeOutcome` — `enum ProbeOutcome { Match { ack: CheckAck }, Mismatch { actual_id: String }, NotFound, TransportRetry { reason: String }, DecodeEscalate { reason: String } }`.

**Tests:** lib-tests for the six helpers (§10.3 rows 1-6 — count updated after HIGH 9 fix added `complete_via_recovery_tx`).  Active KVT2 polling deferred to M3b — no `status_rro_probe` module shipped in W9.

**Acceptance:**
- W3 scanner extended; passes.
- All helper unit tests green.
- No behavioural change to App::boot (stub `run_boot_reconciliation` is wired but not yet called from `reconcile_pending` — added in W9.3).

### W9.3 — 6-branch decision tree + per-DocState dispatch + `reconcile_pending` wiring

**Surface:** `boot_phase.rs::run_boot_reconciliation` body + `app.rs::reconcile_pending` method.

**Files modified:** `services/reconciliation/boot_phase.rs`, `rust/prro/src/app.rs`.

**Tests:** lib-test branch dispatch + partition exhaustive matrix (§10.3 rows 3-4).

**Acceptance:**
- All 6 branches dispatch to the correct helper.
- Partition matrix proves mutual exclusion.
- `reconcile_pending` iterates `fiscal_number_config::list_all`, acquires per-FN lease, calls `run_boot_reconciliation`.
- W10 routing consumption attested by reading `stage_send::run` call site (no parallel route table introduced).

### W9.4 — Acceptance fixtures + idempotency + final verify

**Surface:** `tests/app_boot_reconciliation.rs` only (test code).

**Files added:** `tests/app_boot_reconciliation.rs` (29 fixtures).

**Tests:** 29 fixtures green; full `cargo test` workspace green; W10 invariants verified via touched-zone targeted runs.

**Acceptance:**
- §9.1 fixtures #1-9 verbatim green.
- Per-DocState dispatch matrix (13 fixtures) green.
- Branch sub-fixtures for (d) and (f) green.
- Idempotency run-twice fixture green.
- PRRO_GATE-ah8 verbatim acceptance closed.

**Stop criteria:** PR opens against `rust-gateway`, regular merge commit per memory.

---

## 13. Open questions / decisions deferred to user

This freeze surfaces every ambiguity it found and resolves it; the items below are flagged **NOT** because they're unresolved but because they should be visible to the user before W9.1 lands.  If the user disagrees with any of these, they're freezable changes BEFORE implementation starts; after W9.1 they become migration work.

### 13.1 Naming: `App::reconcile_pending` (vs `recover_and_reconcile`, `bootstrap_phase2`, ...)

**This freeze chose:** `reconcile_pending` for Python parity with `reconciliation_service.reconcile_pending` (`container.py:282-294`, `supervisor.py:56`).  Strict Python parity makes operator-debugging easier (same name appears in Rust logs as in Python's existing log lines; cross-correlation between the two codebases when investigating a multi-day fiscal incident is materially faster with name match than name drift).  **Alternatives considered + rejected:**
- `bootstrap_phase2` (matches `supervisor.py` phase enum) — rejected because Python uses `reconcile_pending` as the runtime *method* name, while `bootstrap_phase2` is the *phase enum* name only.  Mismatch creates two terms for the same concept.
- `recover_and_reconcile` — rejected because "and" suggests two separable steps; in fact this is a single per-FN dispatch.
- `prepare_for_serve` — rejected because the contract is broader than serve-readiness (it also handles permanent escalations like `RequiresManualReconciliation`).

### 13.2 Whether to add `--reconcile-only` CLI flag

**This freeze chose:** NO.  M3a CLI exposes only `serve`.  Operator can re-trigger reconciliation by restarting the process.  Adding `--reconcile-only` would imply a separate code path that runs reconcile + exits (without serving), and the test coverage / operational discipline for that mode hasn't been scoped.  **Defer to M3b** if operationally needed.

### 13.3 Whether OfflineModeRefusal blocks ALL FNs or just the offline one

**This freeze chose:** all FNs (fail-fast on first offline FN encountered).  Rationale: if even one configured FN is in OFFLINE mode, the gateway as a whole cannot serve the M3a contract (the OFFLINE FN's ingress would be dropped silently otherwise, since M3b isn't implemented).  Operator-visible refusal is safer than partial operation.  **Alternative considered + rejected:** mark the offline FN as "blocked from ingress" and continue with online FNs — rejected because it adds a per-FN ingress gate not yet specified.

### 13.4 Audit-event naming convention

**This freeze chose:** `BOOT_<ACTION>_<OUTCOME>` for W9-specific audits (e.g. `BOOT_RESUME_SENDING_TO_ERROR_RETRYABLE`, `BOOT_LAST_CHK_MATCH_KVT1`, `BOOT_RETRY_BUDGET_EXHAUSTED`); `NODE_STATE_BOOT_<branch>` for the FN-level branch summary audit (e.g. `NODE_STATE_BOOT_IDEMPOTENT`, `NODE_STATE_BOOT_OFFLINE_REFUSAL`).  Matches W6/W7/W8/W10 audit prefix discipline.

### 13.5 Whether per-DocState dispatch logs at INFO or DEBUG

**This freeze chose:** the FN-level branch audit at INFO (one per FN per boot — operator sees ~20 events per boot for a 20-FN deployment, acceptable).  Per-doc transitions inherit stage-worker audit levels (W6 INFO, W7 INFO, W8 INFO, W10 WARN on Server -X).  Inheritance avoids per-W9 chatter.

### 13.6 KVT2 polling explicitly deferred to M3b (cycle-7 LOW 2 — centralised here)

**This freeze chose:** option A in §4.6 (HIGH 6 fix) — passive hold for `Kvt1` docs.  No active KVT2 polling shipped in W9.  **Alternatives considered + rejected:**
- Add `DpsChannel::poll_status(fn_sign, request_id)` method — rejected because (a) it's a new Rust DpsChannel trait method that lands more naturally with M3b's offline-and-poll work, (b) the Python `reconciliation.py:60-74` reference is `poll_status` which doesn't have a direct Rust analogue today, (c) M3a's scope is Pattern B + DPS routing, not full reconciliation primitives.
- Assume `last_chk` returns KVT2 alongside KVT1 in `data_sign` — rejected because freeze v3 verification (`docs/webcheck_reverse/...` + `dto.rs:66-72`) confirms `CheckAck` is a single-receipt shape; conflating the two would be a guess.

**Operator-facing impact:** Kvt1 docs accumulate in the pending set across boots; each boot emits one `BOOT_KVT1_HOLD_DEFERRED` INFO audit per stuck doc.  Operator dashboard surfaces the count; manual reconciliation via admin CLI (separate slice; out of W9) escalates individual docs to `RequiresManualReconciliation` if KVT2 truly never arrives.  This matches the conservative-by-design posture: M3a never silently advances state without authoritative DPS evidence.

---

## 14. Glossary of terms used in this freeze

| Term | Definition |
|---|---|
| **Branch (a)..(f)** | The 6 mutually-exclusive cases of §4.3 decision tree |
| **`reconcile_pending`** | The new public `App` method that runs the per-FN decision tree |
| **Pre-flight** | Singleton lock + migrations + quick_check, all inside `App::boot` |
| **Per-FN lease** | The same single-writer-per-FN lock used by live ingress |
| **In-flight `ERROR_RETRYABLE`** | A doc that landed in ERROR_RETRYABLE via prior stage_send and is in the boot-time pending set; recovery may re-drive via Pattern B |
| **MAC recovery budget** | W10's single-bit per-doc counter `mac_recovery_attempts CHECK IN (0,1)` |
| **Routing decision** | W10's closed-enum `RoutingDecision` consumed by `stage_send::run` |
| **WebCheck** | The legacy Windows fiscal client; reference for retry policy at `SubmitPtr.cs` |
| **`attempts_used(doc_id)`** (LOW 6 fix) | §4.0 helper: `SELECT COALESCE(MAX(attempt_no), 0) FROM transport_trace WHERE document_id = ?`.  Returns the durable wire-attempt counter for budget decisions; always non-NULL `i64` |
| **`complete_via_recovery_tx`** (LOW 6 fix) | §4.5 helper: completes an in-flight `transport_trace` row preserving the original `wire_call_started_at`/`wire_call_finished_at` of the failed `send_chk`; writes only `completed_at`, `outcome_kind = OK`, `server_fiscal_no`.  Idempotent via `WHERE completed_at IS NULL` |
| **`passive_hold_kvt1`** (LOW 6 fix) | §4.6 helper: single audit-row INSERT for KVT1 docs; no state mutation, no DPS call, no `transport_trace` write.  Active KVT2 polling deferred to M3b |
| **`MAX_BOOT_ATTEMPTS`** (LOW 6 fix) | §4.0 constant: `5`.  Per W0-3 §2 policy "retry up to max_recovery_attempts=5".  Lives in `services::reconciliation::boot_phase::MAX_BOOT_ATTEMPTS`.  Compared against `attempts_used(doc_id)` for §4.8 escalation |
| **Boot tick** (deferred-3 fix) | One invocation of `App::reconcile_pending`.  No clock involved — each invocation is a discrete "tick" that advances pending docs by at most one stage per doc.  Multi-tick recovery semantics (e.g. §4.3 ENCRYPTED → ErrorRetryable tick #1 → §4.8 dispatch tick #2) refer to consecutive `reconcile_pending` calls, typically across process restarts.  An operator can also trigger ticks by `kill -HUP` if the runtime supports it (out of W9 scope) |

---

## 15. WebCheck decompiled cross-reference table (canonical)

This table consolidates every WebCheck cross-ref cited in §3–§4 so the implementation reviewer can audit them in one place.  All file paths are relative to the repo root; line numbers are pinned to the decompiled tree as of 2026-04-17 (per `WEBCHECK_ANALYSIS.md`).

| W9 anchor | WebCheck file | Lines | What WebCheck does | M3a divergence / alignment |
|---|---|---|---|---|
| §4.5 SENT recovery — last_chk match path | `docs/webcheck_reverse/WebCheckMain/WebCheck/SubmitPtr.cs` | 105-117 | On `status == 0` (UNKNOWN proto-default), sleep 333ms, call `new CheckLastCheck().LastCheckAllInfa()`, compare `MaxID("ksef") + 1 == returnDI`, on match synthesise `returnStatus = 1` | **Divergence:** M3a compares `transport_request_id` (wire-authoritative identity), NOT local sequence (`MaxID + 1`).  Three explicit outcomes instead of "synthesise success".  No 333ms sleep at boot — `transport_trace.attempt_no` (§4.0) is the time dimension |
| §4.4 SENDING recovery — Pattern B anti-pattern | `docs/webcheck_reverse/WebCheckMain/WebCheck/SendingOfflineChecks.cs` | 82-127 | Calls `SubmitCheck`, then on success persists `signedanswerfromficscal` / `checksigned` via separate `UPDATEksef` calls.  No SENDING-equivalent intermediate state | **Divergence (M3a safer):** crash between `SubmitCheck` return and `UPDATEksef` causes next-boot duplicate-send.  M3a's Pattern B SENDING marker eliminates this race by committing SENDING BEFORE wire call (W7 contract); boot recovery is `Sending → ErrorRetryable` (manual triage) — never `Sending → Sent` direct |
| §4.8 ERROR_RETRYABLE recovery — bounded counter | `docs/webcheck_reverse/WebCheckMain/WebCheck/SubmitPtr.cs` | 50-76 | Two compounding inline-retry loops: outer `All.Retries` (operator-configurable, typically 7) × inner `-3 ERROR_SAVE` (7 hardcoded) = up to 49 wire calls per submit, with 333ms sleeps.  Hidden from operator | **Divergence:** M3a uses the durable `transport_trace.attempt_no` counter (`MAX_BOOT_ATTEMPTS = 5` per §4.0), allocated naturally by W7 stage_send on each re-drive.  Persistent, operator-readable, deterministic escalation to `RequiresManualReconciliation` on attempt #6 via the §4.8 pre-check |
| §3.4 Branch (d) OFFLINE refusal | `docs/webcheck_reverse/WebCheckMain/WebCheck/SubmitPtr.cs` | 91-100 | On `status == -16` (ERROR_OFFLINE_ID), calls `All.OfflineOnTechno()` to switch to technical-offline mode, then continues retrying inline | **Opposite stance:** M3a refuses boot.  Rationale: (a) M3a is ONLINE-only by scope (offline lifecycle is M3b); (b) WebCheck's auto-fallback is opaque to operator — M3a's explicit refusal forces a deliberate operator decision (start with `--recover-offline` once M3b lands) |
| §3.3 Branch (c) -2 close-shift edge | `docs/webcheck_reverse/WebCheckMain/WebCheck/SubmitPtr.cs` | 103-141 | On `status == -2 && OpenCloseShift`, parses message text length (21/54/77 chars) via `ErrorMessageOpenShift`, conditionally calls `LastCheckAllInfa()` and treats DI-match as success | **Divergence:** M3a's W10 dispatch parses structured error codes from `DpsError::Server { code, message }`; no message-text-length heuristics.  W9 inherits W10 dispatch; no W9-specific -2 logic |
| §3.3 Branch (c) -15 close-shift edge | `docs/webcheck_reverse/WebCheckMain/WebCheck/SubmitPtr.cs` | 77-90 | On `status == -15 && OpenCloseShift`, sleeps 333ms, calls `LastCheckAllInfa()`, treats DI-match as success | **Divergence:** same as -2.  W10 dispatch handles structured `Server { code: -15, .. }` — boot recovery uses W10 routing, NOT a W9-specific override |
| §3.3 -3 inline retry | `docs/webcheck_reverse/WebCheckMain/WebCheck/SubmitPtr.cs` | 66-76 | On `status == -3` (ERROR_SAVE), retry up to 7× with 333ms sleeps | **Replaced** by durable `transport_trace.attempt_no` counter per §4.0 (see ERROR_RETRYABLE row above) |
| §5.4 Per-FN lease | `docs/webcheck_reverse/WebCheckExe/WebCheck/SQLlite.cs` | (no analogue exists) | WebCheck is single-FN by construction — no inter-FN lease, no multi-FN iteration.  Concurrency model is single-threaded `Application.DoEvents()` per `SubmitPtr.cs:56` (WinForms event pumping during wire calls) | **Alignment by construction:** M3a is multi-FN but uses per-FN lease, so the same operational invariant ("one writer per FN") is preserved even when iterating multiple FNs at boot.  WebCheck's invariant is enforced by structural single-FN-ness; M3a's invariant is enforced by the lease |
| WebCheck error-message-length switch (anti-pattern) | `docs/webcheck_reverse/WebCheckMain/WebCheck/SubmitPtr.cs` | 322-365 (`ErrorMessageUA` / `ErrorMessageUAfn`) | Uses `message.Trim().Length` to classify error semantics (21 chars = "зміна вже відкрита", 54 chars = "цим підписом", etc.) | **Anti-pattern explicitly avoided.**  M3a's W10 dispatch consumes typed `DpsError` variants (8 variants + 12 Server { code }); error classification is via the code field, not message text length |
| WebCheck offline auto-trigger on status 0 / -1 | `docs/webcheck_reverse/WebCheckMain/WebCheck/SubmitPtr.cs` | 155-194 | On `status == 0 || status == -1` AND not already offline → switch to offline mode (`All.OfflineOn()`) and emit error 32 "Включен офлайн режим" | **Opposite stance:** M3a never auto-flips to offline.  Operator decides via separate CLI / config.  This freeze §3.4 makes the refusal explicit |
| WebCheck client construction | `docs/webcheck_reverse/WebCheckMain/WebCheck/SubmitPtr.cs` | 53-55 | New `Client` created per retry iteration (no connection pooling) | **Alignment:** M3a's `DpsChannel` is the connection pool; one channel per process.  M3a never creates a new connection per retry — that's a WebCheck inefficiency |

**Key takeaway for reviewers:** the WebCheck decompiled tree exists in the repo specifically so M3a can avoid replicating its known footguns (no Pattern B, message-length classification, opaque inline retry loops, automatic offline fallback) while preserving the proven recovery shapes (last_chk re-query, close-shift edge handling).  W9's implementation follows the §3–§4 contracts in this freeze; the WebCheck refs are evidence that the contracts are grounded in production-validated behaviour, not invented.

**Decompiled tree completeness note:** the WebCheck decompiled artefacts at `docs/webcheck_reverse/` cover three components — `WebCheckExe/` (WinForms GUI), `WebCheckMain/` (business logic, ~150 classes), `WebCheckServer/` (COM Add-in for 1C), plus `TaxGrpc/` (gRPC client to DPS).  W9 implementation does NOT need to read these directly; this freeze cites the line numbers that matter, and the implementation MUST follow the freeze (not the WebCheck source).

---

## 16. Implementation review checklist (used by senior review at end of W9.1–W9.4)

Each slice closes with senior-grade review.  The checklist below replaces the ad-hoc review prompts used in W7–W10:

**Invariants preservation:**
- [ ] W3 scanner extended to cover `services/reconciliation/`; passes.
- [ ] No `send_chk` / `crypto::*` / file IO inside any `with_immediate` closure in new module.
- [ ] No `transition_state` call invokes an edge OUTSIDE the W1 whitelist.
- [ ] Per-FN lease acquired around the entire decision-tree body (not per-doc).
- [ ] `BootError` returned end-to-end (no `anyhow::Error` smuggling) — exit code mapping verified.

**Branch / DocState coverage:**
- [ ] Partition exhaustive matrix passes (every `(mode, shift_state, has_pending_doc)` triple → exactly one branch).
- [ ] All 13 DocStates have an explicit dispatch path in `boot_phase.rs` (exhaustive `match` on DocState).
- [ ] All 6 branches have a `#[tokio::test]` fixture in §9.1 form.

**WebCheck divergence verified:**
- [ ] No 333ms sleep on retry path (matches `SubmitPtr.cs:66-76` anti-pattern avoidance).
- [ ] No message-text-length error classification.
- [ ] No automatic offline-mode fallback.
- [ ] Pattern B SENDING recovery follows §4.4 (CAS to ErrorRetryable; no auto-resend).

**Idempotency:**
- [ ] Run-twice fixture proves second boot = branch (b) for previously-OK FNs.
- [ ] `transport_trace.attempt_no` monotonically advances (per §4.0); never reset by boot.
- [ ] `last_known_unsigned_xml_sha256` unchanged unless a doc completed Kvt2 → Ack.

**Operational discipline:**
- [ ] `/health/startup` returns 503 on `BootError::IntegrityCheckFailed`.
- [ ] Exit codes match §5.5 table.
- [ ] CRITICAL log on quick_check failure goes to `tracing::error!`, NOT to `audit_log`.

---

## 17. Sign-off readiness

This freeze is **complete and unambiguous** for the M3a W9 scope.  Every branch has:
- A named pre-condition.
- A named action with a single canonical helper.
- A named audit event with payload shape.
- A named acceptance fixture with explicit assertions.
- A Python parity anchor (where one exists).
- A WebCheck parity anchor (where one exists).

The 4-slice implementation plan (W9.1–W9.4) lands the contract incrementally with verification at each gate.  No "M3a impl detail" placeholders remain — every previously-open decision is fixed in §2 or §13.

**Ready for user GO on W9.1.**
