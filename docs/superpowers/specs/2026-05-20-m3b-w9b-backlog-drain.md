# W9b — offline backlog drain orchestration

**Date:** 2026-05-20
**Status:** Implementation freeze — operator approval pending
**Predecessors:** W7a/W7b (offline-ack + dispatcher), W8a/W8b (return-online probe), W9a (stage_send source-state widening), W14a-1/W14a-2a/W14a-2b (shift state machine + signer enforcement) — all merged
**Base commit:** `origin/rust-gateway` `1e2690a` (PR #67 W14a-2b merge).
**Next downstream:** W12 (in-drain `lastChk` KVT2 confirmation), then W11-Δ (deterministic replay fixtures), then W10 (offline policy guard).

---

## 1. Scope (per M3b plan §Task 9)

**Operator-chosen Path A**: W9b lands after W14a-2b closure.  This is the **largest task in M3b** (3-4 day budget) per plan.

**In scope:**
- New module `rust/prro/src/services/offline_sync/backlog_drain.rs` — per-FN drain orchestrator.
- New entry on `App`: `App::drain_offline_backlog_with(&self, fiscal_number: &str, deps: &RuntimeView<'_>) -> Result<DrainSummary, BootError>` — holds App reconcile mutex (W2 enforcement applies; logical mutex only — NO SQLite write tx wraps the whole drain; DB tx scopes stay inside existing stage calls per OQ-5 operator pin).
- New repository helper `fiscal_documents::list_offline_local_ack_for_fn_ordered_by_lnd` — sequential `lnd ASC` walker.
- Per-doc loop with conditional `lastChk` pre-flight (replay-only path).
- Audit chain: `OFFLINE_DRAIN_STARTED` / `OFFLINE_DRAIN_DOC_ADVANCED` / `OFFLINE_DRAIN_DOC_FAILED` / `OFFLINE_DRAIN_COMPLETED` / `OFFLINE_DRAIN_PARTIAL`.
- Typed `BootError::OfflineDrainFailed { document_id, source }` for per-doc fault attribution.

**Channel scope (per plan §"DPS Channel Taxonomy" 2026-05-16 correction):**
- W9b is scoped to the **WebCheck / gRPC channel only**.
- DFS HTTP / XML drain shape (chunked `/fs/pck` package upload) is **out of M3b scope** — future task.

**Out of W9b scope (explicit deferrals):**
- **W12** in-drain `lastChk` KVT2 confirmation is a separate task.  W9b orchestrates per-doc through `stage_send::run` (W9a widened) → `Kvt1`; final transition `Kvt1 → Kvt2 → Ack` requires W12.  W9b ships an EXPLICIT TYPED seam `W12ConfirmOutcome::DeferredKvt1 | Acked { server_fiscal_no }` (NOT `Result<(), _>`); stub `apply_w12_confirmation` ALWAYS returns `DeferredKvt1` so W9b can never silently mis-count Kvt1 as Ack (OQ-2 operator pin).  W12 PR replaces the stub body with real lastChk + Kvt1 → Kvt2 → Ack transition.
- **W10** offline shift close/open policy guard — separate task.
- **W11-Δ** — 7 deterministic-replay fixtures for offline crash points — separate task.
- **Signer enforcement at drain time**: W14a-2b shipped the `signer_guard` helper but did NOT wire it into stage_send for OfflineLocalAck source state.  W9b inherits the wiring as-is (signer enforcement runs ON the W9b drain path automatically because stage_send 4-pre invokes signer_guard regardless of source state — see W14a-2b spec §2.6 + Commit 5).  No additional W9b work needed for signer enforcement.

---

## 2. Drain orchestrator design

### 2.1 Entry point

```rust
// rust/prro/src/services/offline_sync/backlog_drain.rs

pub struct DrainSummary {
    pub fiscal_number: String,
    pub backlog_size_before: usize,
    pub advanced_to_ack: usize,           // docs that reached Ack (requires W12)
    pub advanced_to_kvt1: usize,          // docs that stopped at Kvt1 (W9b alone)
    pub advanced_via_lastchk_replay: usize, // docs short-circuited via lastChk pre-flight
    pub per_doc_failures: Vec<(DocumentId, String)>, // typed-class string per failure
    pub finalized: bool,                  // true → node mode advanced GoingOnline → Online + session Closed
}
```

Two distinct entry surfaces:

**(a) App-owned runtime seam (production):**

```rust
impl App {
    pub async fn drain_offline_backlog_with(
        &self,
        fiscal_number: &str,
        deps: &super::reconciliation::RuntimeView<'_>,
    ) -> Result<DrainSummary, BootError> { ... }
}
```

Holds the `App` reconcile mutex (W2 enforcement — only one drain per process at a time).

**(b) Pure-function entry (for boot_phase reconciliation + tests):**

```rust
pub async fn drain(
    pool: &SqlitePool,
    deps: &RuntimeView<'_>,
    fiscal_number: &str,
) -> Result<DrainSummary, BootError>;
```

Boot path can invoke this when post-W8 probe transitions `Offline → GoingOnline` and backlog is non-empty.  Caller is responsible for App mutex acquisition.

### 2.2 Drain prerequisites

Before per-doc loop runs:

1. Read `node_state.mode`.  MUST be `GoingOnline`.  If not → return `DrainSummary` with `backlog_size_before = 0` + audit `OFFLINE_DRAIN_SKIPPED_NOT_GOING_ONLINE` (no Err).
2. Read backlog: `fiscal_documents::list_offline_local_ack_for_fn_ordered_by_lnd(pool, fn_id) -> Vec<DocumentRow>`.  Strict `ORDER BY lnd ASC`.  If empty → return `DrainSummary` with all-zero counts + audit `OFFLINE_DRAIN_SKIPPED_EMPTY_BACKLOG` (no Err).
3. Read active offline session: `offline_sessions::current_active_session_id(pool, fn_id)`.  MUST be in `Draining` or `Open` state.  If `Open` → transition to `Draining` via service-layer `offline_session::transition_to_draining` (W5 surface — already shipped) + audit `OFFLINE_SESSION_DRAIN_STARTED`.
4. Emit `OFFLINE_DRAIN_STARTED` audit with `{fiscal_number, backlog_size, session_id}` payload.

### 2.3 Per-doc loop (sequential, strict `lnd` ASC)

For each `doc` in `backlog` (in order):

```
┌──────────────────────────────────────────────────────────┐
│ Step A: Conditional lastChk pre-flight                   │
│   IF doc.server_fiscal_no IS NOT NULL:                   │
│     Issue lastChk(fn_sign) probe.                        │
│     IF response.status == OK                             │
│        AND response.id == doc.server_fiscal_no            │
│        AND !response.data_sign.is_empty():               │
│       → REPLAY HIT: W12 reuses this response as KVT2     │
│         evidence; advance doc Kvt1 → Kvt2 → Ack          │
│         via stage_finalize::run.  Audit OFFLINE_DRAIN_   │
│         DOC_ADVANCED with replay_short_circuit=true.     │
│       → continue to next doc.                            │
│     IF lastChk reports anything else → fall through to   │
│       Step B (wire send).                                │
│   IF doc.server_fiscal_no IS NULL:                       │
│     → SKIP pre-flight; go directly to Step B.            │
│                                                          │
│ Step B: Wire send via widened stage_send::run            │
│   Invoke stage_send::run(pool, dps, doc.id, sign_ctx).   │
│   Outcomes:                                              │
│     - Sent { server_fiscal_no, attempt_no }              │
│       → Step C (W12 confirmation).                       │
│     - Routed { decision, .. } / StateConflict / etc.     │
│       → emit OFFLINE_DRAIN_DOC_FAILED with class str;    │
│         push (doc.id, class) to per_doc_failures;        │
│         continue to next doc.                            │
│     - SignerRefused(_) → emit OFFLINE_DRAIN_DOC_FAILED   │
│       with class="signer_refused"; sibling continues.    │
│                                                          │
│ Step C: W12 confirmation via TYPED seam                  │
│   Invoke apply_w12_confirmation(doc) -> W12ConfirmOutcome│
│   W9b stub body ALWAYS returns DeferredKvt1.  Match:     │
│     - W12ConfirmOutcome::DeferredKvt1                    │
│       → advanced_to_kvt1 += 1.  Audit OFFLINE_DRAIN_     │
│         DOC_ADVANCED with final_state="KVT1" +           │
│         w12_status="DeferredKvt1".                       │
│     - W12ConfirmOutcome::Acked { server_fiscal_no }      │
│       → ONLY reachable post-W12-PR.  advanced_to_ack +=1.│
│         Audit final_state="ACK" + w12_status="Acked".    │
│   Typed seam prevents Kvt1→Ack miscounting (OQ-2         │
│   operator pin).                                         │
│                                                          │
│ Step D: Hard interleave guard (single-writer per FN)     │
│   Per ADR-M3-A10 + W2 module-level enforcement, the      │
│   per-doc loop runs synchronously; no concurrent send    │
│   on the same FN can interleave between stage_send and   │
│   W12 lastChk.  Compositional contract — no explicit     │
│   lock in W9b code; just relies on App mutex.            │
└──────────────────────────────────────────────────────────┘
```

### 2.4 Finalization branch

After the loop completes, evaluate using the typed W12 seam result (OQ-2 contract):

**Finalize ONLY IF** `per_doc_failures.is_empty()` AND `advanced_to_ack == backlog_size_before`.

`advanced_to_ack` is incremented ONLY when `apply_w12_confirmation(doc)` returned `W12ConfirmOutcome::Acked { .. }`.  W9b pre-W12 stub always returns `DeferredKvt1` → `advanced_to_ack` stays 0 → finalize NEVER fires on W9b alone (operator-pinned OQ-2 invariant: drain cannot finalize without real Ack proof).

- **Finalize branch** (post-W12-PR + clean drain):
  - CAS `node_state.mode: GoingOnline → Online` (whitelisted edge).
  - Transition offline session `Draining → Closed` via `offline_session::transition_to_closed` (W5 surface).
  - Emit `OFFLINE_DRAIN_COMPLETED` audit with full `DrainSummary` payload.
  - `summary.finalized = true`.

- **Partial branch** (any failure OR any doc returned `DeferredKvt1`):
  - Do NOT finalize.  Node stays in `GoingOnline`, session stays in `Draining`.
  - Emit `OFFLINE_DRAIN_PARTIAL` audit with per-doc failure attribution + advanced/stopped counts.
  - `summary.finalized = false`.
  - Caller (e.g. boot_phase) sees `Ok(summary)` and decides whether to retry on next tick.

W9b pre-W12 behaviour: every successful wire-send doc lands in `advanced_to_kvt1` bucket; finalize never fires; partial summary is the steady-state.  This is intentional — finalize requires W12 evidence per spec §1.

### 2.5 Per-doc failure handling

A single doc failing surfaces as `BootError::OfflineDrainFailed { document_id, source: anyhow::Error }`.  Sibling docs continue — mirrors M3a try-and-audit shim convention.

The orchestrator catches per-doc Err's, audits them, and continues.  Only **infrastructure failures** (DB connection lost, pool exhausted) propagate as Err to the caller.

---

## 3. Repository surface additions

### 3.1 New: `list_offline_local_ack_for_fn_ordered_by_lnd`

```rust
// rust/prro/src/db/repositories/fiscal_documents.rs

/// W9b §3.1 — strict `lnd ASC` walker for backlog drain orchestration.
/// Returns all `OFFLINE_LOCAL_ACK` docs for the FN, ordered by MAC chain
/// position (`lnd` is the authoritative chain-recovery key — `created_at`
/// is second-granular and unstable for tiebreakers).
pub async fn list_offline_local_ack_for_fn_ordered_by_lnd(
    pool: &SqlitePool,
    fn_id: &str,
) -> sqlx::Result<Vec<DocumentRow>> { ... }
```

Filter: `state = 'OFFLINE_LOCAL_ACK' AND fiscal_number = ?` ORDER BY `lnd ASC, created_at ASC, document_id ASC`.

### 3.2 No other repository changes

Existing surfaces sufficient:
- `stage_send::run` widened in W9a — accepts `OfflineLocalAck` source state.
- `stage_finalize::run` (M3a) unchanged — handles Kvt2 → Ack arm.
- `transition_state` (W1 typed helper) — used at the CAS sites.
- `offline_session::transition_to_draining` / `transition_to_closed` (W5) — unchanged.

---

## 4. Audit vocabulary additions

Added to `audit_log` via standard `audit_log::append_tx` / `append` surface.  Severity per row:

| Event | Severity | Payload |
|---|---|---|
| `OFFLINE_DRAIN_STARTED` | Info | `{fiscal_number, backlog_size, session_id, started_at_iso}` |
| `OFFLINE_DRAIN_SKIPPED_NOT_GOING_ONLINE` | Info | `{fiscal_number, current_mode}` |
| `OFFLINE_DRAIN_SKIPPED_EMPTY_BACKLOG` | Info | `{fiscal_number, current_mode}` |
| `OFFLINE_DRAIN_DOC_ADVANCED` | Info | `{document_id, from_state, to_state, replay_short_circuit, attempt_no, server_fiscal_no?}` |
| `OFFLINE_DRAIN_DOC_FAILED` | Warning | `{document_id, failure_class, attempt_no?, observed_state?, wire_error_message?}` |
| `OFFLINE_DRAIN_COMPLETED` | Info | full `DrainSummary` payload |
| `OFFLINE_DRAIN_PARTIAL` | Warning | full `DrainSummary` payload — non-finalized state |

`failure_class` is a stable string taxonomy (matches W8a `dps_error_class` convention):
- `"signer_refused"` — `SignerRefused(_)` outcome.
- `"state_conflict"` — observed state diverged.
- `"wire_routing"` — `Routed { decision: TerminalReject / ProbeRequired / TransientRetry }`.
- `"transport"` — Transport error.
- `"authorization"` — Authorization error.
- `"server_<code>"` — Server status code error.
- `"decode"` / `"internal"` / `"not_found"` — etc.

---

## 5. State machine touchpoints

### 5.1 Document state transitions (per-doc)

Drain reuses existing whitelisted edges from W6 + M3a:
- `OfflineLocalAck → Sending` (W6 PR #55 edge, allowed via `stage_send::run` widened in W9a).
- `Sending → Sent` (M3a Pattern B).
- `Sent → Kvt1` (M3a).
- `Kvt1 → Kvt2 → Ack` (W12, deferred — W9b stops at Kvt1).

Plus replay-short-circuit path (added in W9b):
- `OfflineLocalAck → Kvt2` (NEW WHITELIST EDGE — W9b adds via lastChk pre-flight).
- `Kvt2 → Ack` (M3a).

**Whitelist edge addition (`fiscal_documents::allowed_transition`):**

```rust
// W9b — lastChk replay short-circuit edge.
| (OfflineLocalAck, Kvt2)   // 27 (NEW)
```

Locked-edge count: `28 → 29` (M3a-baseline 26 + W6 added 2 = 28; W9b adds 1 = 29).  Drift-guard test in `tests/repo_fiscal_documents_state_cas.rs` MUST be updated.

### 5.2 Node mode transitions

`GoingOnline → Online` — already whitelisted (M3a + W8 probe path).  Drain finalization CAS uses existing helper.

### 5.3 Offline session transitions

`Open → Draining` — W5 transition.  `Draining → Closed` — W5 transition.  Both reused as-is.

---

## 6. PRRO invariant verification

| Invariant | Verdict | Evidence |
|---|---|---|
| I1 (no network/crypto in long tx) | preserved | `stage_send::run` already enforces; drain orchestrator wraps each `stage_send::run` invocation in its own scope; no `with_immediate` wraps a `dps_channel.send_chk` call |
| I2 (single-writer per FN) | **strengthened** | App reconcile mutex enforces process-wide single-writer; per-doc loop is sequential per FN within the mutex |
| I3 (channel switch with open shift) | preserved | Drain only runs while node mode is GoingOnline; shift state unchanged by drain |
| I4 (idempotency) | **central** | `lastChk` pre-flight + W6 whitelist gate + Pattern B `Sending` marker = 3-layer idempotency.  Interrupt + restart re-discovers `Sent` docs via lastChk (skips wire); `OfflineLocalAck → Sending` whitelist fails on 2nd attempt (no double-wire) |
| I5 (offline bounded by limits) | preserved | Codes consumed at W7 time; drain does NOT re-allocate codes — just advances doc state |
| I6 (canonical payload) | preserved | stage_send already preserves canonical payload semantics; drain reuses |
| I7 (schema_version) | preserved | Envelope shape unchanged |
| I8 (state-machine correctness) | **load-bearing** | Drain must hit ONLY whitelisted transitions; new replay-short-circuit edge `(OfflineLocalAck, Kvt2)` added to whitelist; drift-guard test count `28 → 29` |
| I9 (graceful shutdown) | preserved | Per-doc loop checks shutdown_rx between docs (cooperative cancellation); already-in-flight `stage_send::run` completes before shutdown |
| I10 (minimal diff) | respected | New module + 1 new repository helper + 1 new whitelist edge + audit vocab.  ~600-800 LoC drain + ~200 LoC test |

---

## 7. Test plan

### 7.1 Unit / pure-function

- Drain skip cases:
  - `backlog_drain_skips_when_mode_not_going_online`
  - `backlog_drain_skips_when_backlog_empty`
- Drain happy path:
  - `backlog_drain_scoped_yes_all_reach_ack` (per plan acceptance — verifies finalize branch after W12 lands; pre-W12 verifies all docs reach Kvt1 + partial summary)
  - `backlog_drain_lastchk_preflight_skips_wire_for_already_sent_doc` (replay short-circuit)
  - `backlog_drain_pure_offline_skips_preflight_relies_on_pattern_b` (pure-offline path)
- Per-doc failure:
  - `backlog_drain_per_doc_failure_sibling_continues`
  - `backlog_drain_signer_refused_per_doc_does_not_abort_drain`
- Idempotent re-drain:
  - `backlog_drain_idempotent_replay_after_crash_at_kvt1`
  - `backlog_drain_idempotent_replay_after_crash_at_sending`
- MAC chain preservation:
  - `backlog_drain_mac_chain_preserved` (verify `lnd ASC` order on the drained transport_trace + ack chain)

### 7.2 Integration

- `app_drain_offline_backlog_with_deps_finalizes_node_state` (App-owned entry, deps view, full drain → finalize).
- `app_drain_offline_backlog_with_deps_partial_does_not_finalize` (one doc fails → partial summary, node stays GoingOnline).

### 7.3 Whitelist drift catch

Update existing test in `tests/repo_fiscal_documents_state_cas.rs` — locked edge count 28 → 29.  Plus new focused test:
- `fiscal_documents_offline_local_ack_to_kvt2_whitelist_edge_locked` — verifies the new lastChk replay short-circuit edge.

---

## 8. Acceptance criteria

W9b closes when:

1. ✅ New module `services/offline_sync/backlog_drain.rs` shipped with `drain(pool, deps, fn_id) -> Result<DrainSummary, BootError>` + `DrainSummary` struct.
2. ✅ New App entry `App::drain_offline_backlog_with(&self, fn_id, &deps) -> Result<DrainSummary, BootError>` — acquires App reconcile mutex.
3. ✅ New repository helper `list_offline_local_ack_for_fn_ordered_by_lnd` — strict `lnd ASC` walker.
4. ✅ Drain prerequisites: mode-check + backlog-check + session-state-transition (Open→Draining if needed).
5. ✅ Per-doc loop sequential in `lnd ASC` — verified via fixture asserting transport_trace `lnd` order matches input order.
6. ✅ Conditional `lastChk` pre-flight: ONLY for docs with `server_fiscal_no IS NOT NULL`.
7. ✅ `(OfflineLocalAck, Kvt2)` whitelist edge added + drift-guard test count bumped 28→29.
8. ✅ Per-doc failure isolated; sibling docs continue.
9. ✅ Finalization branch: ALL docs Ack → `GoingOnline → Online` + session `Draining → Closed` + `OFFLINE_DRAIN_COMPLETED` audit.
10. ✅ Partial drain: ANY failure OR W12-deferred Kvt1 → no finalize, `OFFLINE_DRAIN_PARTIAL` audit, summary returned to caller.
11. ✅ Audit chain covers `OFFLINE_DRAIN_STARTED` / `_DOC_ADVANCED` / `_DOC_FAILED` / `_COMPLETED` / `_PARTIAL` / `_SKIPPED_*`.
12. ✅ Full test suite `cargo test -p prro --features test-support` green; ~10 new tests added.
13. ✅ Clippy: `cargo clippy -p prro --all-targets --no-deps --features test-support -- -D warnings` → 0 errors.
14. ✅ Senior review pass — operator-trigger `проверь W9b`.

---

## 9. Open questions — RESOLVED 2026-05-20 (operator)

**Q1 — App entry signature**: **RESOLVED** — `(fn_id: &str, deps: &RuntimeView<'_>)`.  `fn_id` is the audit / repository boundary; explicit at the call site, not derived from `deps`.  Spec §1 + §2.1 aligned (was: §1 had a drift wording omitting `deps`).

**Q2 — W12 integration boundary**: **RESOLVED** — explicit TYPED seam, not `Result<(), _>`.  Stub returns `W12ConfirmOutcome::DeferredKvt1` (always, until W12 PR replaces body).  Acked path is reachable ONLY post-W12.  This way `advanced_to_kvt1` and `advanced_to_ack` counters never get confused; finalize branch only fires when **every** doc returned `Acked { server_fiscal_no }`.

```rust
pub enum W12ConfirmOutcome {
    /// W9b pre-W12 default — doc reached Kvt1 via stage_send but
    /// KVT2 confirmation is not yet implemented.  W9b counts in
    /// `advanced_to_kvt1` bucket; finalize branch refuses to fire.
    DeferredKvt1,
    /// W12 post-PR path — lastChk evidence accepted; doc advanced
    /// Kvt1 → Kvt2 → Ack via stage_finalize::run.  W9b counts in
    /// `advanced_to_ack` bucket.
    Acked { server_fiscal_no: String },
}
```

**Q3 — `lastChk` pre-flight FAILED behavior**: **RESOLVED** — fall through to wire send.  No sleep, no re-poll inside drain.  Pre-flight is best-effort replay optimization; authoritative routing stays in `stage_send::run` + `error_routing`.  Any rate-limit / transport / server failure on pre-flight is silently dropped; the wire send hits the same response with proper routing surface.

**Q4 — replay-short-circuit audit shape**: **RESOLVED** — boolean flag, no new event.  `OFFLINE_DRAIN_DOC_ADVANCED { replay_short_circuit: true|false, w12_status: "DeferredKvt1"|"Acked", final_state, ... }`.

**Q5 — W2 mutex scope**: **RESOLVED** — App's reconcile mutex held for ENTIRE drain.  **Important constraint**: this is the LOGICAL App-level mutex; NO SQLite write transaction wraps the whole drain.  Per-doc DB tx scopes stay INSIDE existing `stage_send::run` / `apply_w12_confirmation` / finalize stage calls.  Operator pin (2026-05-20): invariant-critical against concurrent boot_phase reconciliation / drain race that could double-advance documents or mis-finalize mode/session.  Pilot UX cost (10+s block during large backlog drain) is accepted as the correctness tradeoff.

---

## 10. Implementation slicing (within single PR)

Suggested commit chain inside the single W9b PR:

1. **C1 — Repository helper + whitelist edge**: `list_offline_local_ack_for_fn_ordered_by_lnd` + `(OfflineLocalAck, Kvt2)` whitelist edge + drift-guard test count bump.  ~80 LoC.
2. **C2 — `DrainSummary` + `BootError::OfflineDrainFailed` variant + `failure_class` taxonomy stub**.  ~50 LoC.
3. **C3 — `backlog_drain::drain` skeleton**: prerequisites (mode-check + backlog read + session transition) + audit emit.  No per-doc loop yet.  ~150 LoC + ~80 LoC tests for skip cases.
4. **C4 — Per-doc loop without lastChk**: invoke `stage_send::run`, audit `_DOC_ADVANCED` / `_DOC_FAILED`, sibling-continue.  ~200 LoC + ~150 LoC tests.
5. **C5 — Conditional `lastChk` pre-flight**: replay-short-circuit edge + W12 stub `apply_w12_confirmation` (always Ok-Kvt1 for now).  ~150 LoC + ~150 LoC tests.
6. **C6 — Finalization branch**: node mode + session transition + `OFFLINE_DRAIN_COMPLETED` / `_PARTIAL`.  ~80 LoC + ~80 LoC tests.
7. **C7 — App-owned entry + integration test**: `App::drain_offline_backlog_with` + 2 integration tests.  ~60 LoC + ~120 LoC tests.

Total ~770 LoC product + ~580 LoC tests ≈ 1350 LoC diff.

---

## 11. Worktree + branch convention

- Branch: `m3b/w9b-backlog-drain` (off `rust-gateway` `1e2690a`).
- Worktree: `/mnt/d/PRRO_GATE-m3b-w9b/`.
- PR target: `rust-gateway`.
- Merge style: `gh pr merge --merge` (per operator's PR merge style memory — NOT `--squash`).

---

## 12. Out of W9b scope (deferred to later tasks)

- **W12** — in-drain `lastChk` KVT2 confirmation per plan §Task 12.
- **W10** — offline shift close/open policy guard per plan §Task 10.
- **W11-Δ** — 7 deterministic-replay fixtures per plan §Task 11.
- **DFS HTTP / XML channel drain** — out of M3b scope per plan §"DPS Channel Taxonomy" 2026-05-16 correction.
- **Multi-FN parallel drain** — currently single-FN sequential; multi-FN parallelism would require per-FN sub-mutex.  Not in M3b.
- **W14a-3** — multi-cashier role registry; SHIFT_CLOSE/Z_REPORT senior-cashier role policy beyond §16.9.
