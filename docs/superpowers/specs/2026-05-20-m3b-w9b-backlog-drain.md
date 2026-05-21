# W9b — offline backlog drain orchestration

**Date:** 2026-05-20
**Status:** Implementation freeze — operator approval pending
**Predecessors:** W7a/W7b (offline-ack + dispatcher), W8a/W8b (return-online probe), W9a (stage_send source-state widening), W14a-1/W14a-2a/W14a-2b (shift state machine + signer enforcement) — all merged
**Base commit:** `origin/rust-gateway` `1e2690a` (PR #67 W14a-2b merge).
**Next downstream:** W12 (in-drain `lastChk` KVT2 confirmation), then W11-Δ (deterministic replay fixtures), then W10 (offline policy guard).

---

## Amendment 2026-05-21 — sibling-continue scope + unfinished cohort + halt-on-reject

Operator decisions during C4 senior review (2026-05-21) clarify three points the original spec text under-specified:

1. **Sibling-continue scope + manual-recon class definition (§2.5 + §6.3 clarification)**.  W9b sibling-continue applies ONLY to non-manual-recon-class per-doc failures.  Manual-recon-class on pending-drain shift escalates: shift → `RequiresManualReconciliation` via edges 6 / 14 (per `LEGAL_INVARIANTS.md` §INV-19 + `m3b-shift-state-expansion.md` §6.3), `node_state.shift_state` mirror updated in the same `with_immediate` envelope (per `m3b-shift-state-expansion.md` §5 load-bearing invariant), `Critical` `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` audit emitted, and the FN drain STOPS.  Subsequent backlog docs are NOT processed in the same drain tick.

   **Manual-recon class** (operator-pinned per `m3b-shift-state-expansion.md` §3.5 "Manual is last resort" + §6.2 wire-error taxonomy):
   - `RetryClass::TerminalReject` (Authorization{DocumentReject}, Server -1 / -2 non-shift / -5 / -7..-10 / -16)
   - `RetryClass::FnConfigError` (Server -13 / -14)
   - `RetryClass::WrapperBug` (Internal / NotFound on live / QueryNotSupported on live / ServerFiscalIdMismatch / unknown Server code)
   - `RetryClass::MacRecovery` (orchestrator already burned its retry — second -12)
   - `RetryClass::OperatorEscalation` (Server -6 ERROR_NOT_PREV_ZREPORT)
   - `StateConflict`, `DocumentMissing`, `SignerRefused` (structural drift / signer mismatch)
   - All `StageSendError` variants (structural invariant breach)

   **Non-manual class** (transient / retry-budget-preserving — sibling-continue applies EVEN on pending-drain shifts):
   - `RetryClass::TransientRetry` (Transport / Server -3) — retry within budget; shift stays in pending-drain awaiting next-tick drain
   - `RetryClass::ProbeRequired` (Decode / -2 close-shift / -15 close-shift) — W9 probe territory; not immediate Manual

   The W9b drain audit payload carries `manual_recon_class: bool` for operator-dashboard filtering.

2. **Unfinished drain cohort (§2.2 step 2 + §3.1 clarification + HIGH-C4-8 widening + HIGH-C5-1 session scoping + MED-C5-4 KVT2 deferral)**.  The backlog read MUST cover the full unfinished cohort, not only `OFFLINE_LOCAL_ACK`.  The C1 helper `list_offline_local_ack_for_fn_ordered_by_lnd` lands the OFFLINE_LOCAL_ACK-only scan in C1 but is renamed and widened in **C5** to `list_drain_candidates_for_fn_ordered_by_lnd(pool, fn_id, session_id)`, returning rows in `state IN ('OFFLINE_LOCAL_ACK','SENT','KVT1','ERROR_RETRYABLE') AND fiscal_number = ? AND offline_session_id = ? AND fs_mode = 'OFFLINE'` ordered by `lnd ASC`.  Empty-backlog skip applies only when zero unfinished candidates exist.  Per-doc dispatch by persisted state:
   - `OFFLINE_LOCAL_ACK` → `stage_send::run` (current C4 path).
   - `ERROR_RETRYABLE` → `stage_send::run` (W9a 4-pre source whitelist already accepts ErrorRetryable; re-drives Pattern B).  **Without this state in the cohort, a C4 drain that produced TransientRetry on a doc strands the pending-drain shift forever** — the doc moves OFFLINE_LOCAL_ACK → Sending → ErrorRetryable on Transport / Server -3 wire failure, exits the C4 OFFLINE_LOCAL_ACK-only scan, but the shift's `pending-drain` state holds the FN until that doc completes.  HIGH-C4-8 operator finding (2026-05-21) locks this gap-fix.
   - `SENT` → C5 `lastChk` pre-flight via `process_via_lastchk_replay` (closes I4 restart safety per spec §6).  Match path persists `KVT1_RAW = ack.data_sign` byte-for-byte inside the same `with_immediate` envelope as the Sent→Kvt1 CAS + audit (HIGH-C5-2 forensic evidence contract).  NotFound path downgrades to ErrorRetryable for safe Pattern B re-drive next tick (HIGH-C5-3, non-manual class).  Mismatch / Decode / Unexpected → per-doc failure (manual recon class).  TransportRetry → per-doc failure (non-manual; retry budget retained).
   - `KVT1` → `process_via_w12_only`: pre-W12 stub records DeferredKvt1 without DB mutation.  W12 PR adds lastChk evidence + `Kvt2 → Ack` via `stage_finalize::run`.
   - `KVT2` — **deferred to W12 PR (MED-C5-4)**.  Pre-W12 drain has no clean path: counting KVT2 as DeferredKvt1 mis-audits; advancing Kvt2→Ack would violate the operator-pinned "drain cannot finalize without real Ack proof" invariant.  W12 PR re-adds KVT2 to the cohort with `stage_finalize::run`.
   - Terminal states (`ACK` / `REJECTED` / `CANCELLED` / `REQUIRES_MANUAL_RECONCILIATION`) excluded by the SELECT.

3. **C4/C5 split**.  **C4** lands the inline `Sent → Kvt1` advance via the typed W12 stub seam — so `advanced_to_kvt1` counter, audit `to_state="KVT1"`, and persisted DB state stay consistent within a single C4-only flow.  **C5** widens the walker to the unfinished cohort + adds `lastChk` pre-flight + extracts the inline transition into `apply_w12_confirmation` helper.  **C5 is a blocker** before any "C4 approved" verdict at the PR level — pre-C5 the drain is NOT restart-safe for crashed-mid-drain SENT docs (M3a `boot_phase` covers SENT recovery in the meantime, but spec §6 I4 requires drain to own the rediscovery path post-W12).

The audit vocabulary (§4) gains one new event:

| Event | Severity | Payload |
|---|---|---|
| `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` | Critical | `{fiscal_number, shift_id, document_id, failure_class, current_shift_state, halt_position}` |

`halt_position` = the 0-based index of the doc in the backlog that triggered the halt; subsequent docs were NOT visited.

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

Before per-doc loop runs (post HIGH-C5-1 step reordering):

1. Read `node_state.mode`.  MUST be `GoingOnline`.  If not → return `DrainSummary` with `backlog_size_before = 0` + audit `OFFLINE_DRAIN_SKIPPED_NOT_GOING_ONLINE` (no Err).
2. **Read active offline session FIRST** (HIGH-C5-1 reordering): `offline_sessions::current_open_or_draining_session(pool, fn_id)`.  Missing session → `OFFLINE_DRAIN_SKIPPED_EMPTY_BACKLOG` audit with `reason="no_active_offline_session"` + return `DrainSummary::new(fn, 0)`.  This replaces the prior "Internal error on backlog-without-active-session" contract: the cohort walker (step 3) scopes by `offline_session_id`, so absent session means absent cohort by construction.
3. Read backlog scoped to the session: `fiscal_documents::list_drain_candidates_for_fn_ordered_by_lnd(pool, fn_id, session_id) -> Vec<DocumentRow>`.  SELECT filter: `state IN ('OFFLINE_LOCAL_ACK','SENT','KVT1','ERROR_RETRYABLE') AND offline_session_id = ? AND fs_mode = 'OFFLINE'`.  Strict `ORDER BY lnd ASC`.  If empty → return `DrainSummary` with all-zero counts + audit `OFFLINE_DRAIN_SKIPPED_EMPTY_BACKLOG` (no Err).
4. If session is in `Open` → transition to `Draining` via inline CAS + `OFFLINE_SESSION_DRAIN_STARTED` audit in one `with_immediate` envelope.
5. Emit `OFFLINE_DRAIN_STARTED` audit with `{fiscal_number, backlog_size, session_id}` payload.

### 2.3 Per-doc loop (sequential, strict `lnd` ASC)

For each `doc` in `backlog` (in order):

```
┌──────────────────────────────────────────────────────────┐
│ Step A: per-state dispatch (C5 amendment 2026-05-21)     │
│   doc.state ∈ {OFFLINE_LOCAL_ACK, ERROR_RETRYABLE}       │
│     → process_via_stage_send (Step B wire send + inline  │
│       Sent→Kvt1 via apply_w12_confirmation stub).        │
│   doc.state == SENT (cohort rediscovery)                 │
│     → process_via_lastchk_replay:                        │
│         Issue lastChk(fn_sign) probe.                    │
│         Match (id == doc.server_fiscal_no AND            │
│           !ack.data_sign.is_empty())                     │
│         → advance Sent→Kvt1 via apply_w12_confirmation   │
│           + persist KVT1_RAW = ack.data_sign in same     │
│           with_immediate envelope (HIGH-C5-2).           │
│         NotFound → CAS Sent → ErrorRetryable + audit;    │
│           non-manual class; next tick re-drives via ER   │
│           cohort (HIGH-C5-3, matches M3a boot_phase).    │
│         Mismatch / Decode / Unexpected → per-doc failure │
│           (manual recon class on pending-drain shift).   │
│         TransportRetry → per-doc failure (non-manual;    │
│           retry budget retained per spec §3.5).          │
│         NO wire fall-through on SENT (would double-      │
│         fiscalize via W9a 4-pre source whitelist).       │
│   doc.state == KVT1                                      │
│     → process_via_w12_only: pre-W12 stub records         │
│       DeferredKvt1 without DB mutation; W12 PR adds      │
│       lastChk + Kvt2→Ack.                                │
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

**Superseded by the 2026-05-21 amendment** (see top of file): sibling-continue applies only to non-manual-recon-class failures.  Manual-recon-class failures on a pending-drain shift halt the drain and escalate the shift to `RequiresManualReconciliation`.  TransientRetry and ProbeRequired are explicitly **non-manual** per operator pin — they retain the retry budget; sibling-continue applies even on pending-drain shifts.

The orchestrator catches per-doc outcomes, records them in `DrainSummary`, audits them, and continues per the amended scope.  Only **infrastructure failures** (DB connection lost, pool exhausted, audit-append sqlx error, mirror UPDATE drift) propagate as `BootError::*` to the caller.

---

## 3. Repository surface additions

### 3.1 `list_drain_candidates_for_fn_ordered_by_lnd` (C5 + HIGH-C5-1)

```rust
// rust/prro/src/db/repositories/fiscal_documents.rs

/// W9b §3.1 + spec amendment 2026-05-21 — strict `lnd ASC` walker
/// for the unfinished drain cohort, scoped to a specific offline
/// session.  Returns docs in
/// `state IN ('OFFLINE_LOCAL_ACK','SENT','KVT1','ERROR_RETRYABLE')`
/// AND `offline_session_id = ?` AND `fs_mode = 'OFFLINE'` for the
/// FN, ordered by MAC chain position (`lnd` authoritative;
/// `created_at` + `document_id` are tiebreakers).
pub async fn list_drain_candidates_for_fn_ordered_by_lnd(
    pool: &SqlitePool,
    fn_id: &str,
    session_id: OfflineSessionId,
) -> sqlx::Result<Vec<DocumentRow>> { ... }
```

**KVT2 deferred to W12 PR (MED-C5-4)**: KVT2 docs require `Kvt2 → Ack` via `stage_finalize::run`, which pre-W12 would violate the operator-pinned "drain cannot finalize without real Ack proof" invariant.  W12 PR re-adds KVT2 to the cohort along with the finalize path.

**Session scoping rationale (HIGH-C5-1)**: without `offline_session_id = ?` + `fs_mode = 'OFFLINE'`, the widened cohort could capture online docs of the same FN (online SENT/KVT1/ERROR_RETRYABLE).  Those are M3a `boot_phase` reconciliation territory; drain MUST NOT cross-process them.

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
| `OFFLINE_DRAIN_DOC_ADVANCED` | Info | `{document_id, from_state, to_state, replay_short_circuit, attempt_no, server_fiscal_no, w12_status}` |
| `OFFLINE_DRAIN_DOC_FAILED` | Warning | `{document_id, failure_class, manual_recon_class, retry_class?, target_state?, observed_state?, attempt_no?, wire_status_code?, wire_error_message?, mismatch_detail?, send_error_detail?}` |
| `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` | Critical | `{fiscal_number, shift_id, document_id, failure_class, current_shift_state, halt_position}` |
| `OFFLINE_DRAIN_COMPLETED` | Info | full `DrainSummary` payload |
| `OFFLINE_DRAIN_PARTIAL` | Warning | full `DrainSummary` payload — non-finalized state |

`failure_class` is a stable string taxonomy aligned with the C2 `FailureClass` enum (matches W8a `dps_error_class` convention):

| String | Source outcome |
|---|---|
| `"signer_refused"` | `StageSendOutcome::SignerRefused(_)` |
| `"state_conflict"` | `StageSendOutcome::StateConflict { .. }` |
| `"not_found"` | `StageSendOutcome::DocumentMissing` OR `StageSendError::DocumentMissingForRecovery` |
| `"wire_routing_terminal_reject"` | `Routed { decision: TerminalReject }` (Authorization{DocumentReject}, Server -1/-2 non-shift/-5/-7..-10/-16) |
| `"wire_routing_transient_retry"` | `Routed { decision: TransientRetry }` (Transport, Server -3) |
| `"wire_routing_probe_required"` | `Routed { decision: ProbeRequired }` (Decode, -2/-15 close-shift) |
| `"authorization"` | `Routed { decision: FnConfigError }` (-13 / -14) |
| `"server"` | `Routed { decision: OperatorEscalation }` (-6) |
| `"internal"` | `Routed { decision: WrapperBug / MacRecovery }` OR most `StageSendError` variants |
| `"offline_fiscal_no_missing"` | `StageSendError::OfflineFiscalNoMissing` |

The audit payload also carries `manual_recon_class: bool` — operator dashboards filter on this flag.  Mapping per operator pin (`m3b-shift-state-expansion.md` §3.5): `wire_routing_transient_retry` and `wire_routing_probe_required` are **false** (retry budget retained); everything else is **true** (manual-recon class).

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

**Q3 — `lastChk` pre-flight FAILED behavior**: **RESOLVED + AMENDED (C5 2026-05-21 + HIGH-C5-3)** — the original "fall through to wire send" answer was correct for OFFLINE_LOCAL_ACK / ERROR_RETRYABLE source states (and remains so in C5: those branches go through `stage_send::run` directly without lastChk pre-flight; `server_fiscal_no` is NULL by construction on these states).  For the SENT cohort (rediscovered crashed-mid-drain docs, HIGH-C4-1), wire fall-through is FORBIDDEN — re-driving a SENT doc through `stage_send::run` would be rejected by the W9a 4-pre source whitelist (Sent not in `{Signed, ErrorRetryable, OfflineLocalAck}`) and would risk double-fiscalization if the whitelist ever changed.  C5 instead routes per `last_chk_probe::ProbeOutcome`: Match → advance via `apply_w12_confirmation`; NotFound → downgrade to ErrorRetryable for next-tick Pattern B re-drive (HIGH-C5-3); Mismatch / Decode / Unexpected → per-doc manual-recon failure; TransportRetry → per-doc non-manual failure (retain retry budget).

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
