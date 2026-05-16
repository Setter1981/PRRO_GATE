# M3b Shift State Expansion — Design Freeze (2026-05-17)

> **Docs-only design freeze.**  Code implementation, ingress wiring, W10 policy guard, and DFS-channel adapter all explicitly DEFERRED.  This document defines the target state model + migration shape + audit vocabulary so the implementation PR can be reviewed against a frozen contract.

## 1. Context + correction lineage

- **PR #62** (`docs(prro/m3b): correct offline Z-report policy and 24h close trap`, merged `2dc21f4`) landed:
  - W10 policy correction: ONLINE `Z_REPORT` over offline backlog blocked; OFFLINE-mode local `Z_REPORT` close-of-day allowed as Pattern C `OFFLINE_LOCAL_ACK` document.
  - Hard close-code reserve = 1 (FN-scoped) — legal escape hatch from 24h trap.
  - DPS Channel Taxonomy: WebCheck/gRPC + DFS HTTP/XML pluggable behind `DpsChannel`; Maria 304 is ingress, not channel.
  - X-report read-only invariant.
  - Audit vocabulary including `ONLINE_Z_REPORT_BLOCKED_BACKLOG`, `OFFLINE_Z_REPORT_LOCAL_CLOSE_ACCEPTED`, `OFFLINE_Z_REPORT_LOCAL_CLOSE_REFUSED` (Critical when `reason = "code_pool_exhausted"`), `POST_LOCAL_CLOSE_SALE_REFUSED`, `OFFLINE_CODE_RESERVED_FOR_CLOSE`.
- **This freeze sits BETWEEN PR #62 and W10 implementation**: W10's policy guard must be coded against the expanded state machine, not the 6-state legacy.  Sequence pinned by operator: PR #62 merge → this design freeze → migration + repository implementation PR → W10 policy guard PR.  Reverse sequence would force a W10 rewrite.

## 2. Problem statement

The current 6-state `ShiftState` (`Created → Opening → Opened → Closing → Closed / Error`, `rust/prro/src/db/models/enums.rs:62`) has three architectural gaps that surface as soon as M3b W10 (and the related W7 offline path) needs to support offline shift open + correct recovery semantics.

### 2.1 Offline `SHIFT_OPEN` catch-22

The Pattern C offline path lands a `SHIFT_OPEN` document in `OFFLINE_LOCAL_ACK` while DPS is unreachable.  The shift state machine has no state that simultaneously:
- accepts subsequent offline `SELL` / `RETURN` / `SERVICE_*` operations (load-bearing — the business case for offline mode is to keep selling), AND
- signals "shift not yet fiscally confirmed by DPS" (load-bearing — recovery must distinguish locally-committed-pending-drain from DPS-confirmed).

`Opening` is anti-ops (intent-marker, mirrors the `Sending` document state).  `Opened` implies DPS confirmation.  Neither fits.

### 2.2 `Closing` overload — recovery safety

`Closing` currently encodes three operationally distinct cases (per `docs/OFFLINE_SHIFT_CLOSE_DECISION.md` §6.2 — accepted by intent):
1. Online `Z_REPORT` / `SHIFT_CLOSE` in flight (Pattern B style; recoverable via reconciliation; DPS may accept/reject).
2. Offline local `Z_REPORT` landed `OFFLINE_LOCAL_ACK`, drain pending.
3. Crash-recovery middle ground.

The operator-facing semantic ("no new fiscal operations") is identical across all three.  But the **recovery decision tree** differs:
- Case 1, DPS rejects with `Authorization::DocumentReject` → revert `Closing → Opened` and retry.
- Case 1, DPS rejects with `Authorization::FiscalNumberNotRegistered` / `Server` hard reject → operator action; **NEVER** revert.
- Case 2, drain reject → operator action; **NEVER** revert to `Opened` (would re-open a shift that was locally closed; all OFFLINE_LOCAL_ACK SELL's on that shift become invalid).

A single `Closing` state forces recovery code to JOIN on `close_document_id` / `z_report_document_id` and pattern-match on `(shift_state, linked_doc.state, retry_class)` triples to make the right call.  If a future dev forgets the JOIN, they can apply case-1 recovery to case-2 state → **catastrophic shift-state divergence**.

State-encoded discipline (an explicit `CLOSING_LOCAL_PENDING_DRAIN` state) closes the gap **at compile/match-arm time**, not via runtime JOIN that may be omitted in a refactor.

### 2.3 `Error` too coarse for "drain rejected, manual needed"

`Error` is currently a generic terminal.  `fiscal_documents` distinguishes `Rejected` (terminal not-recoverable) from `RequiresManualReconciliation` (terminal, operator action needed).  Shifts need the same distinction:
- Drain rejected `SHIFT_OPEN` on offline path → 30+ OFFLINE_LOCAL_ACK SELL's on a shift that fiscally never opened.  Pure `Error` says "broken"; `RequiresManualReconciliation` says "operator must compensate".
- Drain rejected `Z_REPORT` close-of-day → the shift closed locally but DPS will not honour the close.  Same operator-action shape.

## 3. State machine design

### 3.1 New state set (9 total, 3 new)

```rust
str_enum!(ShiftState {
    Created                      => "CREATED",
    Opening                      => "OPENING",                       // online intent
    OpenedLocalPendingDrain      => "OPENED_LOCAL_PENDING_DRAIN",    // NEW
    Opened                       => "OPENED",                         // DPS confirmed
    ClosingLocalPendingDrain     => "CLOSING_LOCAL_PENDING_DRAIN",   // NEW
    Closing                      => "CLOSING",                        // online intent
    Closed                       => "CLOSED",                         // DPS confirmed
    RequiresManualReconciliation => "REQUIRES_MANUAL_RECONCILIATION",// NEW
    Error                        => "ERROR",
});
```

### 3.2 Per-state semantics

| State | Wire | Operator UI collapses to | Ops permitted | Recovery branch |
|---|---|---|---|---|
| Created | `CREATED` | "starting" | none | new shift not yet committed |
| Opening | `OPENING` | "opening" | none (anti-ops) | online open in flight; Ack on any attempt → Opened (edge 3).  Recoverable rejection (operator re-issues SHIFT_OPEN with corrected payload) keeps shift in Opening — the retry-loop is doc-state-machine territory, not a shift edge.  Hard reject or operator-driven give-up → RequiresManualReconciliation (edge 4) |
| **OpenedLocalPendingDrain** | `OPENED_LOCAL_PENDING_DRAIN` | "opened" | **offline ops only** (see §3.3) | drain-driven; → Opened (edge 5) only after **full W9b backlog drain for the FN completes** (zero `OFFLINE_LOCAL_ACK` / `Sent` / `Kvt1` / `Kvt2` docs remain) AND `node_state.mode` flips `GoingOnline → Online` — SHIFT_OPEN Ack alone is not sufficient (see §3.3 online-ops-resume rule); → manual on any drain reject |
| Opened | `OPENED` | "opened" | all fiscal ops | M3a happy path |
| **ClosingLocalPendingDrain** | `CLOSING_LOCAL_PENDING_DRAIN` | "closing" | none (post-local-close lockout) | drain-driven; → Closed on `Z_REPORT` final ACK; → manual on drain reject |
| Closing | `CLOSING` | "closing" | none (anti-ops) | online close in flight; recovery taxonomy per §6 |
| Closed | `CLOSED` | "closed" | none (terminal) | terminal until next `SHIFT_OPEN` |
| **RequiresManualReconciliation** | `REQUIRES_MANUAL_RECONCILIATION` | "manual recon needed" | none | operator must compensate; durable until explicit operator action |
| Error | `ERROR` | "error" | none | terminal hard error; not reached via normal whitelist (see §4.2) |

### 3.3 `OpenedLocalPendingDrain` operations contract

While `shift.state == OpenedLocalPendingDrain`:
- **Offline-channel `SELL` / `RETURN` / `SERVICE_*` PERMITTED.**  The shift is locally committed via Pattern C `OFFLINE_LOCAL_ACK` SHIFT_OPEN doc; subsequent offline ops Pattern-C-land their own `OFFLINE_LOCAL_ACK` docs.
- **Offline-channel `Z_REPORT` PERMITTED** (operator can close-of-day even before the SHIFT_OPEN drains, e.g. shift hits 24h limit while still offline).
- **Online-channel ops REFUSED.**  If `node_state.mode` is `Online` / `GoingOnline` and the shift is still in `OpenedLocalPendingDrain`, a new online `SELL` / `RETURN` / `SERVICE_*` MUST be refused with audit `SHIFT_OPEN_PENDING_DRAIN_OP_REFUSED` (Warning).  Rationale: DPS hasn't confirmed the shift open; sending a SELL online before the SHIFT_OPEN drains would arrive at DPS before the open marker — breaks INV-03 ("shift opened before fiscal operations") on the DPS side.
- **Drain ordering rule.**  When `node_state.mode` returns to `GoingOnline` and W9b backlog drain runs, it MUST drain by strict `lnd` ASC; the offline `SHIFT_OPEN` document must be the first document of that local shift in `lnd` order — its `lnd` is whatever the FN-global allocator assigned at offline-open time, NOT necessarily `1` (lifecycle docs from prior shifts may have allocated earlier `lnd` values).  W9b enforces this by walking docs in `lnd` ASC and refusing to drain doc N+1 before doc N has reached `Ack` via W12 confirmation.
- **Online-ops-resume rule (CORRECTED).**  Drain ACK of `SHIFT_OPEN` alone is **not sufficient** to resume online ops — the FN backlog may still contain offline `SELL` / `RETURN` / `SERVICE_*` / `Z_REPORT` docs queued in `lnd` ASC after the open doc.  Allowing a new online op while drain is still mid-backlog would break strict `lnd` ASC ordering: the new online op would arrive at DPS interleaved with later backlog docs.  Therefore online ops on the FN resume only after **all** of the following hold: (a) W9b full backlog drain for this FN completes (zero `OFFLINE_LOCAL_ACK` / `Sent` / `Kvt1` / `Kvt2` docs remain on this FN), AND (b) `node_state.mode` flips `GoingOnline → Online`.  The W8 return-online probe + W9b drain finalization together drive (b); until then `OpenedLocalPendingDrain` continues to refuse online ops via `SHIFT_OPEN_PENDING_DRAIN_OP_REFUSED` audit.  Shift state edge `OpenedLocalPendingDrain → Opened` (edge 5 in §4.1) fires when the full drain criterion in (a) is met AND `SHIFT_OPEN` has acked; the two events typically coincide (last doc to ack on the shift can be either SHIFT_OPEN if no other backlog, or the last trailing SELL/Z_REPORT — the shift edge fires once on the combined criterion).

## 4. Allowed transitions (whitelist enumerated)

### 4.1 Allowed edges

The `fiscal_documents::allowed_transition`-style whitelist for shifts.  Every edge enumerated with rationale; **anything not listed is forbidden** and the W6/W7 `transition_state` helper must surface a typed `Forbidden` outcome.

| # | From | To | Trigger | Rationale |
|---|---|---|---|---|
| 1 | Created | Opening | online `SHIFT_OPEN` ingress | M3a Pattern B intent-marker |
| 2 | Created | OpenedLocalPendingDrain | offline `SHIFT_OPEN` ingress (Pattern C) | offline-open seam (new) |
| 3 | Opening | Opened | online send → DPS Ack on **any** attempt (first or after operator-driven re-issue) | M3a happy path; retry-loop is doc-state-machine concern, not shift edge |
| 4 | Opening | RequiresManualReconciliation | online send → hard reject (`FnConfigError` / `Server` hard / id-mismatch) OR operator gives up on the open attempts | recovery taxonomy §6 |
| 5 | OpenedLocalPendingDrain | Opened | W9b drain `SHIFT_OPEN` → final DPS Ack via W12; **no other backlog docs remain** | offline-open happy path with empty trailing backlog |
| 6 | OpenedLocalPendingDrain | RequiresManualReconciliation | drain `SHIFT_OPEN` rejected | catastrophic-rollback path (new) |
| 7 | OpenedLocalPendingDrain | ClosingLocalPendingDrain | offline `Z_REPORT` ingress while shift still locally-open and `SHIFT_OPEN` not yet drained | offline close-of-day before SHIFT_OPEN drains (new) |
| 8 | Opened | Closing | online `Z_REPORT` / `SHIFT_CLOSE` ingress | M3a Pattern B intent-marker |
| 9 | Opened | ClosingLocalPendingDrain | offline `Z_REPORT` ingress (Pattern C) | offline-close seam (new) |
| 10 | Closing | Closed | online send → DPS Ack | M3a happy path |
| 11 | Closing | Opened | online send → `Authorization::DocumentReject` only (specific recoverable class per §6) | narrow rollback |
| 12 | Closing | RequiresManualReconciliation | online send → hard reject | recovery taxonomy §6 |
| 13 | ClosingLocalPendingDrain | Closed | W9b drain reached final DPS Ack via W12 for **every backlog doc including** prior offline `SHIFT_OPEN` and the close `Z_REPORT` | offline-close happy path; see §4.3 for the "all-prior-acks" predicate |
| 14 | ClosingLocalPendingDrain | RequiresManualReconciliation | drain rejected **any** backlog doc (offline `SHIFT_OPEN`, intermediate `SELL`/`RETURN`/`SERVICE_*`, or close `Z_REPORT` itself) | catastrophic-rollback path; reject of any drained doc terminates the close path |

Total: **14 edges** (HIGH-fix Round 1 — the earlier "Opening → Opened on DocumentReject + retry" line was a duplicate; retry is doc-state-machine territory, not a separate shift edge).

### 4.3 ClosingLocalPendingDrain — drain progress vs state transition

`ClosingLocalPendingDrain` carries an **internal predicate** that determines when edge 13 fires.  Drain-ACK of an individual backlog doc (offline `SHIFT_OPEN`, prior `SELL`, intermediate offline ops, close `Z_REPORT`) does NOT itself change `ShiftState` — the doc transitions to `Ack` in `fiscal_documents.state` and the shift stays in `ClosingLocalPendingDrain`.  Edge 13 (`→ Closed`) fires when **ALL** of the following are true:

1. The shift's `z_report_document_id` doc has reached `fiscal_documents.state = Ack` via W9b drain + W12 confirmation.
2. Every backlog doc with `shift_id == this_shift.shift_id` AND `lnd <= z_report.lnd` has also reached `Ack` (no `OFFLINE_LOCAL_ACK` / `Sent` / `Kvt1` / `Kvt2` doc remains on the shift).
3. If the shift entered `ClosingLocalPendingDrain` via edge 7 (offline `Z_REPORT` issued while `SHIFT_OPEN` still locally-pending), the offline `SHIFT_OPEN` doc is one of the docs counted in (2) and must also be `Ack` — its earlier-`lnd` position in strict ASC ordering means W9b drains it first regardless.

**Drain-ACK-of-SHIFT_OPEN-from-ClosingLocalPendingDrain semantics**: when shift is in `ClosingLocalPendingDrain` and drain processes the offline `SHIFT_OPEN` doc to `Ack`, this records the open confirmation **on the doc**, but does NOT transition the shift back to `Opened` (the shift is past `Opened`; rollback would re-enable ordinary fiscal ops on a shift that operator already locally closed — invariant breach).  The shift state machine has no edge "ClosingLocalPendingDrain → Opened" by design (per §4.4 forbidden list below).  The W9b drain orchestrator must NOT mistake "first-doc-of-shift acked" for "shift opened"; the close-down sequence is doc-by-doc until predicate (1)+(2)+(3) holds, at which point edge 13 fires once.

**Drain-REJECT-of-SHIFT_OPEN-from-ClosingLocalPendingDrain semantics**: if drain rejects the offline `SHIFT_OPEN` doc while shift is in `ClosingLocalPendingDrain`, edge 14 fires immediately — shift → `RequiresManualReconciliation`, same operator-action shape as a reject from `OpenedLocalPendingDrain`.  The doc rejection cascades through the shift state regardless of subsequent doc states.  W9b drain MUST halt as soon as any backlog doc on the shift rejects (per W9b ordering rule — don't continue draining doc N+1 if doc N has just rejected; the rejection invalidates the shift's close path).

### 4.4 Forbidden patterns

- **No blanket `* → Error`**: the only way to reach `Error` is via an explicit, audited `ShiftRepository::force_to_error_with_audit(shift_id, reason, audit_evidence)` seam.  This seam is operator-action territory (e.g. unrecoverable schema corruption manually flagged), NOT a fallback for unexpected transition outcomes.  Unexpected transitions surface as `TransitionOutcome::Forbidden` (typed error to caller) so the bug is observable, not silently swept into `Error`.
- **No blanket `Closing → Opened`**: see §6 — only `Authorization::DocumentReject` warrants rollback.  Hard rejects + drain rejects route to `RequiresManualReconciliation`.
- **No `ClosingLocalPendingDrain → Opened`**: once an offline `Z_REPORT` lands `OFFLINE_LOCAL_ACK`, the shift cannot rollback to `Opened` automatically.  Local Pattern C commitment + post-local-close lockout are durable; only operator-driven `force_*` seam can undo, with full audit trail.
- **No `OpenedLocalPendingDrain → Closed` directly**: the only path to `Closed` from a locally-opened shift is via `ClosingLocalPendingDrain` (offline `Z_REPORT` first) or through full drain to `Opened` + then `Closing → Closed` (online close after drain).  Skipping `ClosingLocalPendingDrain` would mean the close happened without a `z_report_document_id` link.
- **While in `ClosingLocalPendingDrain`, drain ACK of a non-`Z_REPORT` doc does NOT fire a shift edge** (HIGH-fix Round 2): drain ACKs for `SELL` / `RETURN` / `SERVICE_*` / offline `SHIFT_OPEN` on the close-path are doc-state transitions only; the shift stays in `ClosingLocalPendingDrain` until the close `Z_REPORT` itself ACKs AND predicate §4.3 holds.  Only then edge 13 (`→ Closed`) fires.  This forbidden pattern is **scoped to `ClosingLocalPendingDrain`** — it does NOT apply to `OpenedLocalPendingDrain`, where edge 5 (`→ Opened`) is precisely triggered by `SHIFT_OPEN` drain ACK + empty trailing backlog.
- **No `OpenedLocalPendingDrain → Opening`**: state graph is forward-only on the open-side; can't "downgrade" a locally-committed open back to an online-intent state.

### 4.5 Force-error / manual seam

```rust
// rust/prro/src/db/repositories/shifts.rs (proposed)
impl ShiftRepository {
    /// Operator-driven force-transition into Error / RequiresManualReconciliation.
    /// Bypasses the whitelist intentionally; requires explicit `evidence_json`
    /// audit payload describing why the operator (or supervisory automation)
    /// is short-circuiting normal recovery.  Emits `SHIFT_FORCE_*` audit row
    /// with Critical severity.
    pub async fn force_to_error_with_audit(
        tx: &mut WriteTxConn<'_>,
        shift_id: ShiftId,
        target: ShiftState, // must be Error or RequiresManualReconciliation
        evidence_json: &str,
    ) -> sqlx::Result<()>;
}
```

Test contract (load-bearing): `transition_state` MUST NOT have a code path reaching `Error` or `RequiresManualReconciliation` outside the whitelist edges in §4.1; force-seam is the only entry.  A `tests/shifts_no_silent_error_paths.rs` scanner test pins this.

## 5. node_state.shift_state synchronisation

`node_state.shift_state` (CHECK constraint in migration `001_core_identities.sql`) currently mirrors the same 6-state set.  Same expansion applies: 9-state CHECK on both tables.

**Invariant (load-bearing)**: `node_state.shift_state` for an FN MUST equal the `state` column of the currently-active shift row for that FN.  Existing code maintains this invariant for the 6-state model; expansion preserves the same write discipline — every `shifts.transition_state` call that flips state also UPDATEs `node_state.shift_state` inside the same `with_immediate` envelope.

If `node_state.shift_state` is inconsistent with `shifts.state` post-recovery, that is itself a structural breach → `RequiresManualReconciliation` is the appropriate landing.

## 6. Closing recovery taxonomy (narrow whitelist)

> **Round 2 HIGH fix (2026-05-17):** This section originally proposed to reuse `RetryClass` from `fiscal_documents` W10.x dispatcher (`rust/prro/src/services/write_path/error_routing.rs`) as the discriminator for the `Closing → Opened` vs `Closing → RequiresManualReconciliation` decision.  That mapping was **dangerous**: `RetryClass::TerminalReject` is a *coarse* class assigned to multiple DPS rejection shapes — not only `DpsError::Authorization { kind: DocumentReject }` (genuinely operator-recoverable by re-issue) but also `Server { -1 else branch }` (verify failure with Critical severity, lines 411-417), `Server { code in -5/-7/-8/-9/-10 }` (XML/builder hard-rejects, Critical, line 437), and `Server { code: -11 }` (168h legal limit, node→`Blocked`, Critical, line 460).  Routing all of those back to `Opened` would re-open a shift on a hard-reject — exactly the catastrophic case this design exists to prevent.  Reverse-classification on the W10.x dispatcher's `RetryClass` cannot be safe because it strips the discriminating information (the underlying `DpsError` variant + the server code) that determines whether re-issue is meaningful.

### 6.1 `ShiftCloseRecoveryClass` — typed, shift-specific

The shift-close recovery decision uses a separate, narrower taxonomy.  Implementation PR adds an enum (proposed name; final pinned in impl):

```rust
// rust/prro/src/services/write_path/shift_close_recovery.rs (proposed location)
#[non_exhaustive]
pub enum ShiftCloseRecoveryClass {
    /// W9b-style retry continues; shift stays in Closing.  Caller MUST
    /// be prepared for the next-tick wrapper to re-attempt.
    HoldRetry,
    /// Operator-recoverable: the rejected close doc can be re-issued
    /// with corrected payload (DocumentReject specifically — signature /
    /// cert / canonical-payload re-build).  Shift rolls back Closing →
    /// Opened so a new close attempt can land.
    RollbackToOpened,
    /// Terminal but recoverable via operator action: hard rejects, FN
    /// config breach, 168h limit breach, builder bugs, wrapper bugs,
    /// id mismatch.  Shift advances Closing → RequiresManualReconciliation.
    EscalateManual,
    /// MAC-recovery orchestrator is running; hold until it completes,
    /// then re-classify.
    MacRecoveryInProgress,
}
```

### 6.2 Classification — by `DpsError` variant, NOT by `RetryClass`

Implementation MUST classify by matching the underlying `DpsError` variant + (where applicable) `AuthorizationKind` + the wire `code` field — NOT by reading `RoutingDecision.retry_class` from the doc-state-machine layer.

| Underlying error | `ShiftCloseRecoveryClass` | Shift edge |
|---|---|---|
| `DpsError::Transport(_)` — **transient, retry budget remaining** (W9b / `stage_send` wire-loop has remaining attempts) | `HoldRetry` | none — stays `Closing` |
| `DpsError::Transport(_)` — **retry budget exhausted** (W9b / `stage_send` wire-loop hit its retry ceiling; durable transport failure on this close attempt) | `EscalateManual` | `Closing → RequiresManualReconciliation` (edge 12) — staying in `Closing` indefinitely is the "closing forever" trap; escalation is the operator-action shape |
| `DpsError::Authorization { kind: DocumentReject, .. }` | `RollbackToOpened` | `Closing → Opened` (edge 11) — operator re-issues close |
| `DpsError::Authorization { kind: FiscalNumberNotRegistered, .. }` | `EscalateManual` | `Closing → RequiresManualReconciliation` (edge 12) |
| `DpsError::Server { code: -1 }` (verify failure branch, NOT mapped to `DocumentReject`) | `EscalateManual` | `Closing → RequiresManualReconciliation` |
| `DpsError::Server { code: -5 / -7 / -8 / -9 / -10 }` (XML / builder hard-rejects) | `EscalateManual` | same |
| `DpsError::Server { code: -11 }` (168h cumulative-offline limit; node→`Blocked`) | `EscalateManual` | same |
| `DpsError::Server { code: -6 }` (`ERROR_NOT_PREV_ZREPORT`, operator-recoverable) | `EscalateManual` | same — operator must close the prior Z first |
| `DpsError::Server { code: -3 }` (transient retry) | `HoldRetry` | none |
| `DpsError::Decode(_)` | `EscalateManual` | upstream-contract drift; investigate manually |
| `DpsError::ServerFiscalIdMismatch { .. }` | `EscalateManual` | reconciliation territory |
| `DpsError::Internal(_)` / `QueryNotSupported(_)` | `EscalateManual` | wrapper-side bug — fix code |
| Doc state observed in `MacRecovery` orchestrator run | `MacRecoveryInProgress` | hold; re-classify after orchestrator completes |

The decision MUST be made on the actual `DpsError` (from `transport_trace` or directly from the wire response that triggered the close-doc state transition).  Implementation MUST NOT short-circuit on `retry_class == TerminalReject` because that bucket conflates `DocumentReject` (rollback-safe) with `Server -5/-7/-8/-9/-10/-11` (catastrophic if rolled back).

### 6.3 Drain-side rejection — universal `EscalateManual`

For docs rejected during W9b drain on `ClosingLocalPendingDrain` or `OpenedLocalPendingDrain` paths: ANY rejection routes to `EscalateManual` regardless of the wire-side `DpsError`.  Rationale: drain has already crossed the local-commit threshold (the doc is `OFFLINE_LOCAL_ACK` durable); ordinary `RollbackToOpened` semantics don't apply — re-issue would mean re-sending an offline-acked doc through the wire, which has different idempotency / `lastChk`-evidence shape (W12 territory).  Manual reconciliation is the only safe landing.

## 7. Reserve rule extension (W10 docs follow-up)

The PR #62 reserve rule is **close-code reserve = 1** (FN-scoped, while shift open and offline Z_REPORT not emitted).  This freeze extends it for the offline-open seam:

### 7.1 Offline `SHIFT_OPEN` reserve gate

When the W10 policy guard evaluates an offline `SHIFT_OPEN` request, the predicate is:

```
free_offline_codes_for_fn >= 2
```

- **1 code** is consumed by the `SHIFT_OPEN` document itself at the moment of Pattern C landing.
- **1 code** must remain in the pool as the close-reserve floor for the future offline `Z_REPORT` close-of-day.

If `free_offline_codes_for_fn < 2` at the moment of attempt → typed refusal + audit `OFFLINE_SHIFT_OPEN_REFUSED_INSUFFICIENT_RESERVE` (**Critical** severity — operator cannot open offline; business-blocking event).

Edge case: `free == 1` is a particularly nasty trap — superficially "we have a code", but it's the close-reserve.  Refusal is correct; the operator must wait for code refill (operational watermark trigger) before opening offline.

### 7.2 Updated reserve enforcement table

| Shift state | FN pool free | Ordinary offline op | Offline Z_REPORT | Offline SHIFT_OPEN |
|---|---|---|---|---|
| (no shift) | ≥ 2 | N/A | N/A | **Allowed** (consumes 1) |
| (no shift) | 1 | N/A | N/A | **Refused** `OFFLINE_SHIFT_OPEN_REFUSED_INSUFFICIENT_RESERVE` |
| (no shift) | 0 | N/A | N/A | **Refused** same |
| OpenedLocalPendingDrain / Opened | ≥ 2 | Allowed | Allowed | **Refused** (already in shift) |
| OpenedLocalPendingDrain / Opened | 1 (close-reserve) | **Refused** `OFFLINE_CODE_RESERVED_FOR_CLOSE` | Allowed (consumes reserved code) | **Refused** |
| OpenedLocalPendingDrain / Opened | 0 | Refused (no code at all) | **Refused** `OFFLINE_Z_REPORT_LOCAL_CLOSE_REFUSED { reason: "code_pool_exhausted" }` (Critical) | **Refused** |
| ClosingLocalPendingDrain | any | Refused (post-local-close lockout, `POST_LOCAL_CLOSE_SALE_REFUSED`) | Refused (already closing) | Refused |

## 8. Audit event vocabulary (additions)

| Event type | Severity | Payload sketch | Trigger |
|---|---|---|---|
| `SHIFT_OPENED_LOCAL_PENDING_DRAIN` | Info | `{fiscal_number, shift_id, open_document_id, lnd}` | offline `SHIFT_OPEN` landed `OFFLINE_LOCAL_ACK` |
| `SHIFT_CLOSING_LOCAL_PENDING_DRAIN` | Info | `{fiscal_number, shift_id, z_report_document_id, lnd}` | offline `Z_REPORT` landed `OFFLINE_LOCAL_ACK` |
| `SHIFT_OPEN_PENDING_DRAIN_OP_REFUSED` | Warning | `{fiscal_number, shift_id, requested_doc_type, node_mode, shift_state, open_document_id, reason: "shift_open_not_confirmed_by_dps"}` | online op attempted while shift in `OpenedLocalPendingDrain` |
| `SHIFT_OPEN_DRAIN_REJECTED` | **Critical** | `{fiscal_number, shift_id, open_document_id, drain_error_class, drain_error_detail}` | drain rejected `SHIFT_OPEN` — shift → `RequiresManualReconciliation`; orphan SELL/RETURN docs on this shift now require operator compensation |
| `SHIFT_CLOSE_DRAIN_REJECTED` | **Critical** | `{fiscal_number, shift_id, z_report_document_id, drain_error_class, drain_error_detail}` | drain rejected close `Z_REPORT` — shift → `RequiresManualReconciliation` |
| `SHIFT_REQUIRES_MANUAL_RECONCILIATION` | **Critical** | `{fiscal_number, shift_id, transition_from, transition_reason}` | shift landed `RequiresManualReconciliation` from any allowed edge |
| `OFFLINE_SHIFT_OPEN_REFUSED_INSUFFICIENT_RESERVE` | **Critical** | `{fiscal_number, requested_doc_type: "SHIFT_OPEN", free_pool_count, required: 2}` | offline SHIFT_OPEN attempt with `free < 2` |
| `SHIFT_FORCE_TO_ERROR` | **Critical** | `{fiscal_number, shift_id, from_state, evidence_json}` | `force_to_error_with_audit` seam invoked |
| `SHIFT_FORCE_TO_MANUAL_RECONCILIATION` | **Critical** | same shape | same seam, target = manual |

Critical severity events MUST surface immediately on operator audit dashboards.

## 9. Migration design

### 9.1 Tables touched

1. `shifts.state` — CHECK constraint enumerates the 6 existing values.  Extends to 9.
2. `node_state.shift_state` — same CHECK in `migrations/001_core_identities.sql`.  Extends to 9.

Both via the W4-established **DROP+CREATE-same-name** pattern (not `ALTER TABLE` — `ALTER TABLE RENAME` breaks deferred FK validation, per W4 commit message lesson).

### 9.2 Migration sketch (`migrations/016_shift_state_expansion.sql`)

Two tables rebuild — `shifts` AND `node_state` — both via the W4-established DROP+CREATE-same-name pattern.  Two table rebuilds means two trigger restores + one inbound-FK handling.

**Per-table concerns** (table-specific, NOT generic W4 templating):

- **`shifts`**:
  - Trigger to restore byte-identically: `shifts_updated_at` (`migrations/001_core_identities.sql:52`).
  - Inbound FK: `fiscal_documents.shift_id → shifts.shift_id ON DELETE RESTRICT` (declared in `migrations/002_fiscal_documents.sql:38` AND re-declared during the W3-era rebuild in `migrations/008_doc_state_sending.sql:118` — the second is the live one).  **CRITICAL: this FK requires the W4 NULL-FK / holding-column dance, NOT bare `defer_foreign_keys`.**  `PRAGMA defer_foreign_keys = ON` defers FK *validation* to COMMIT, but FK *actions* (`RESTRICT`, `CASCADE`) fire at statement time — so `DROP TABLE shifts` with any child `fiscal_documents.shift_id IS NOT NULL` row would fail the `ON DELETE RESTRICT` immediately on populated DBs (the W4 lesson is explicit on this, see `migrations/015_offline_normalize.sql:52-80` for the worked example with `offline_sessions` + the same `ON DELETE RESTRICT` shape).  `fiscal_documents.shift_id` is `BLOB` (nullable per `migrations/008_doc_state_sending.sql:87`), which makes the W4 NULL-stash dance applicable directly.
  - **Required step sequence for `shifts` rebuild** (mirrors `migrations/015_offline_normalize.sql` exactly):
    1. `PRAGMA defer_foreign_keys = ON` — defers FK validation; FK actions still fire at statement time.
    2. Add temporary holding column on `fiscal_documents` (e.g. `shift_id_stash BLOB`) to preserve values.
    3. Copy `fiscal_documents.shift_id` into the holding column.
    4. `UPDATE fiscal_documents SET shift_id = NULL` — so `RESTRICT` action sees no children.
    5. Snapshot `shifts` into a no-FK temp table.
    6. `DROP TABLE shifts` — succeeds because no `fiscal_documents.shift_id` row points at it.
    7. `CREATE TABLE shifts (… state TEXT CHECK (state IN (9 values)) …)` under the SAME name (NOT `_new` + RENAME — that breaks deferred FK validation per W4 lesson).
    8. Restore `shifts` rows from snapshot (identity map for the 6 existing state values; pre-pilot has zero rows, but the dance must work for populated DBs too).
    9. Restore `fiscal_documents.shift_id` from the holding column.  By now FK references point at the rebuilt `shifts` (same name); `defer_foreign_keys` defers validation to COMMIT.
    10. Drop the holding column on `fiscal_documents`.
    11. Re-create `shifts_updated_at` trigger byte-identically (snapshot DDL before drop; diff after restore must be empty).
    12. Re-create any indexes on `shifts(state)` byte-identically.
  - Hard FK guard at end (per W4 lesson + step 14 of §9.2 code sketch): `INSERT INTO __m016_fk_guard__ … SELECT count(*) FROM pragma_foreign_key_check` — non-zero count fails the temp table's CHECK constraint and aborts the migration before sqlx commits.
  - Indexes: verify with `SELECT sql FROM sqlite_master WHERE type='index' AND tbl_name='shifts'` snapshot before drop.

- **`node_state`**:
  - Trigger to restore byte-identically: `node_state_updated_at` (`migrations/001_core_identities.sql:76`).
  - Inbound FK: none directly on `shift_state` (the column is denormalised mirror of `shifts.state` for the active shift; FK lives on the `fiscal_number` boundary, not on state).  No special FK handling needed beyond the standard W4 `defer_foreign_keys` discipline.
  - Indexes: verify any existing indexes on `node_state(shift_state)` before drop.

- **Triggers MUST be re-DROPped + re-CREATEd byte-identically** (W4 lesson: `fd_updated_at` snapshot/restore).  The migration's BEFORE state of trigger DDL must equal AFTER state — diff zero.  Audit hygiene check goes via `SELECT sql FROM sqlite_master WHERE type='trigger' AND tbl_name IN ('shifts','node_state')` snapshot pair.

```sql
-- M3b shift-state expansion (per design freeze 2026-05-17).
-- W4-style schema rebuild ×2 (shifts + node_state) — table-specific
-- trigger/index/FK concerns enumerated in §9.2 above.

PRAGMA defer_foreign_keys = ON;

-- ── Fail-closed FK guard (W4 lesson) ──────────────────────────────
CREATE TEMP TABLE __m016_fk_guard__ (
    fk_violation_count INTEGER CHECK (fk_violation_count = 0)
);
-- INSERT happens at end of migration; failure short-circuits commit.

-- ── shifts rebuild — W4 NULL-FK / holding-column dance ───────────
-- Required because fiscal_documents.shift_id has ON DELETE RESTRICT
-- and defer_foreign_keys does NOT defer FK ACTIONS (only validation).
-- See migrations/015_offline_normalize.sql:52-80 for the worked
-- example with offline_sessions.
-- (1)  Snapshot trigger DDL + index DDL for diff-zero restore.
-- (2)  ALTER TABLE fiscal_documents ADD COLUMN shift_id_stash BLOB;
-- (3)  UPDATE fiscal_documents SET shift_id_stash = shift_id;
-- (4)  UPDATE fiscal_documents SET shift_id = NULL;
-- (5)  CREATE TEMP TABLE shifts_snapshot AS SELECT … FROM shifts;
-- (6)  DROP TRIGGER shifts_updated_at;
-- (7)  DROP TABLE shifts;
-- (8)  CREATE TABLE shifts (… state TEXT CHECK (state IN (9 values)) …)
--      — SAME NAME (no _new + RENAME).
-- (9)  INSERT INTO shifts SELECT … FROM shifts_snapshot;
--      (identity map for the 6 existing states; pre-pilot zero rows
--      but dance MUST work for populated DBs.)
-- (10) UPDATE fiscal_documents SET shift_id = shift_id_stash;
-- (11) ALTER TABLE fiscal_documents DROP COLUMN shift_id_stash;
-- (12) Re-create shifts_updated_at trigger byte-identically.
-- (13) Re-create indexes byte-identically.

-- ── node_state rebuild — simpler (no inbound FK on shift_state) ──
-- (1)  Snapshot trigger DDL + index DDL.
-- (2)  CREATE TEMP TABLE ns_snapshot AS SELECT … FROM node_state;
-- (3)  DROP TRIGGER node_state_updated_at;
-- (4)  DROP TABLE node_state;
-- (5)  CREATE TABLE node_state (… shift_state TEXT CHECK (… 9 values …) …)
-- (6)  INSERT INTO node_state SELECT … FROM ns_snapshot;
-- (7)  Re-create node_state_updated_at trigger byte-identically.
-- (8)  Re-create indexes byte-identically.

-- ── node_state rebuild ────────────────────────────────────────────
-- Same pattern; trigger node_state_updated_at restored byte-identically.

-- ── FK guard exit ─────────────────────────────────────────────────
INSERT INTO __m016_fk_guard__ (fk_violation_count)
SELECT count(*) FROM pragma_foreign_key_check;
-- CHECK constraint on the temp table requires count == 0; any FK
-- violation aborts the migration BEFORE the surrounding sqlx tx commits.
```

The implementation PR materialises the full DDL; this freeze pins the **shape** + the table-specific concerns above.  Author MUST NOT mechanically copy W4 DDL — `fd_updated_at` is not the trigger here; `shifts_updated_at` + `node_state_updated_at` are.

### 9.3 Backward compatibility

- **Pre-pilot status**: no production rows yet for the Rust gateway.  Identity map for the 6 existing states is trivial.
- **Existing test fixtures**: M3a + M3b W2-W9a tests use the 6 states; migration preserves them.  No fixture rewrites needed beyond `tests/shifts_no_silent_error_paths.rs` (new) and `tests/shift_state_whitelist_matrix.rs` (extended whitelist count).
- **sqlx prepare**: regen `.sqlx/` after schema change (per W9a Round 1 lesson — `cargo sqlx prepare` from `rust/prro/` package root, commit the updated cache).

## 10. W11-Δ replay coverage (new fixtures)

Extend `tests/write_path_deterministic_replay.rs` (already at 28 fixtures post-W11-Δ) with 5 new fixtures covering the new state-transition crash points:

1. `replay_crash_after_offline_shift_open_local_ack` — crash immediately after offline `SHIFT_OPEN` landed `OPENED_LOCAL_PENDING_DRAIN`; reboot → shift stays in `OpenedLocalPendingDrain`; subsequent offline `SELL` accepted.
2. `replay_crash_during_drain_open_resync` — crash mid-drain after `SHIFT_OPEN` doc reached `Sending`; reboot → W9b resumes drain from `SHIFT_OPEN`; idempotent.
3. `replay_offline_shift_open_drain_rejected_lands_manual_recon` — DPS rejects `SHIFT_OPEN` on drain; shift → `RequiresManualReconciliation`; orphan OFFLINE_LOCAL_ACK SELL's on that shift remain queryable.
4. `replay_offline_z_report_drain_rejected_lands_manual_recon` — DPS rejects close `Z_REPORT` on drain; shift → `RequiresManualReconciliation`.
5. `replay_closing_local_pending_drain_through_w12_to_closed` — happy path: offline `Z_REPORT` → `ClosingLocalPendingDrain` → drain → W12 confirm → `Closed`.

After this freeze's implementation PR: W11 total = 28 + 5 = 33 fixtures.

## 11. Implementation PR shape (next-task spec)

The next code PR after this freeze merges.  Operator-driven; this section is informational, not the freeze itself.

**Branch**: `m3b/shift-state-expansion-impl` (off post-freeze-merge HEAD).

**Files touched**:

*Core enum + schema + repository*:
- `rust/prro/src/db/models/enums.rs` — `ShiftState` extended to 9 variants (replaces the 6-variant `str_enum!` at line 62).
- `rust/prro/migrations/016_shift_state_expansion.sql` — new migration per §9.  Rebuilds `shifts` + `node_state` CHECK constraints AND their `_updated_at` triggers (both currently in `migrations/001_core_identities.sql:52` for `shifts_updated_at`, `:76` for `node_state_updated_at`).  Handles inbound FK from `fiscal_documents.shift_id` (declared in `migrations/002_fiscal_documents.sql:38` + `migrations/008_doc_state_sending.sql:118` — second declaration is the W3 rebuild).
- `rust/prro/src/db/repositories/shifts.rs` — whitelist extended to 14 edges (per §4.1); new `force_to_error_with_audit` + `force_to_manual_reconciliation_with_audit` seams; `transition_state` typed errors.
- `rust/prro/src/db/repositories/node_state.rs` — `shift_state` column read/write paths accept new variants; mirror-write discipline preserved (per §5).
- `rust/prro/.sqlx/` — query cache regen (mandatory after CHECK constraint change — `cargo sqlx prepare` from `rust/prro/` per W9a Round 1 lesson; commit `.sqlx/` delta).

*Hot-path code consuming `ShiftState` (load-bearing — exhaustive match arms expand)*:
- `rust/prro/src/services/write_path/stage_acquire.rs` (around line 346 — `check_shift_guard`-style gating).  `Opening | Closing` branches today; expansion must:
  - Add `OpenedLocalPendingDrain` arm that permits offline-channel ops (per §3.3) and refuses online-channel ops with `SHIFT_OPEN_PENDING_DRAIN_OP_REFUSED` audit.
  - Add `ClosingLocalPendingDrain` arm that refuses all fiscal ops (post-local-close lockout).
  - Add `RequiresManualReconciliation` arm that refuses all fiscal ops with operator-action message.
- `rust/prro/src/services/write_path/stage_offline_ack.rs` (around line 206 — currently asserts `ShiftState::Opened` only).  Expansion must accept `ShiftState::OpenedLocalPendingDrain` as a valid source for offline `SELL` / `RETURN` / `SERVICE_*` / `Z_REPORT` doc landing in `OFFLINE_LOCAL_ACK`.  Edge 7 (`OpenedLocalPendingDrain → ClosingLocalPendingDrain`) is triggered by `Z_REPORT` source from this state and the W10 policy guard's `AllowOfflineLocalClose` routing.
- `rust/prro/src/services/reconciliation/boot_phase.rs` — recovery branch matching today on `Opening | Closing` for shift state; expansion adds branches for `OpenedLocalPendingDrain` (resume drain), `ClosingLocalPendingDrain` (resume drain), `RequiresManualReconciliation` (skip — operator action awaited).  Replay fixtures in §10 lock these branches.
- *Anywhere else* that pattern-matches `ShiftState` exhaustively — Rust's `match` exhaustiveness check surfaces these at compile time on impl PR.  The author MUST `grep -rn "ShiftState::" rust/prro/src/` and audit every match block (W9a Round 1 lesson: shotgun rewrite is cheaper than missing a code path).

*Tests*:
- `rust/prro/tests/shifts_no_silent_error_paths.rs` — new scanner test pinning that `transition_state` cannot reach `Error` / `RequiresManualReconciliation` outside force-seam.
- `rust/prro/tests/shift_state_whitelist_matrix.rs` — extended whitelist matrix (14 edges + forbidden pairs per §4.4).
- `rust/prro/tests/stage_acquire_shift_guard.rs` (if exists, else new) — coverage for the 3 new arms in stage_acquire.
- `rust/prro/tests/stage_offline_ack_shift_states.rs` (if exists, else new) — coverage for `OpenedLocalPendingDrain` as a valid source.
- W11-Δ replay fixtures per §10.

*Documentation*:
- Sync `docs/superpowers/plans/2026-05-14-m3b-implementation.md` §Task 10 audit vocabulary table with the new shift-level events.

**Verify command**:
```bash
cargo test -p prro --features test-support --test shifts_no_silent_error_paths --test shift_state_whitelist_matrix --test write_path_deterministic_replay
cargo clippy -p prro --all-targets --no-deps --features test-support -- -D warnings
cargo test -p prro --features test-support  # full suite — no regressions
```

**Acceptance criteria**:
1. 9-state `ShiftState` enum in code + 14-edge whitelist (drift-guard contract — locked-count test enforces the exact number).
2. Migration rebuilds both `shifts` + `node_state` CHECK constraints atomically.
3. No silent path to `Error` / `RequiresManualReconciliation` from `transition_state`.
4. `force_to_error_with_audit` seam exists + audited per §8.
5. 5 new W11-Δ replay fixtures green (total 33).
6. Full M3a + M3b regression suite green (no fixture rewrites required by this expansion).
7. `node_state.shift_state` mirror invariant preserved by tests (existing `tests/node_state_*` extended to cover new states).

## 12. W10 phasing — W10a (policy) + W10b (offline `SHIFT_OPEN` wiring)

> **Round 2 MED-fix (2026-05-17):** the freeze originally deferred offline `SHIFT_OPEN` ingress wiring to "post-W10", but W10's reserve=2 rule for offline `SHIFT_OPEN` would have no caller without that ingress — dead policy.  W10 splits into two slices in the W7a/W7b pattern.

After the implementation PR lands (`m3b/shift-state-expansion-impl` per §11), W10 implementation proceeds as **two PRs**:

### 12.1 W10a — policy guard primitive (no offline `SHIFT_OPEN` ingress yet)

- New `services::offline_guard::evaluate_z_report_policy(pool, fiscal_number, requested_doc_type) → PolicyDecision`.
- `PolicyDecision` variants: `AllowOnline`, `AllowOfflineLocalClose`, `RefuseOnlineBacklogPending`, `RefuseOfflineNoCode`, `RefuseAfterLocalClose`, `RefuseShiftOpenPendingDrain` (new — refuses online op when shift in `OpenedLocalPendingDrain` and node mode `Online`/`GoingOnline`).
- Reserve checks:
  - close-reserve = 1 (PR #62 rule) — `OFFLINE_CODE_RESERVED_FOR_CLOSE` audit.
  - **offline `SHIFT_OPEN` gate = pool ≥ 2** (specified per §7) — the policy *seam* is implemented in W10a, but it can only refuse with `OFFLINE_SHIFT_OPEN_REFUSED_INSUFFICIENT_RESERVE` if and when an offline `SHIFT_OPEN` request reaches it.  In W10a no caller yet routes offline `SHIFT_OPEN` through this evaluation; the gate exists as a *typed seam waiting for its caller*, not as an active path.
- W10a fixtures verify policy decisions on a stub-driven ingress (no `stage_offline_ack` integration); the offline `SHIFT_OPEN` reserve check is fixture-only proof, not production-active.
- `stage_acquire`, `stage_offline_ack`, `boot_phase` arms for `OpenedLocalPendingDrain` / `ClosingLocalPendingDrain` / `RequiresManualReconciliation` are wired (matches shift-state-expansion-impl from §11), but the **doc_type discriminator** for offline `SHIFT_OPEN` is left out of `stage_offline_ack` until W10b.
- W10a acceptance tests reference the new states; no rewrites because W10 hasn't been coded yet.

### 12.2 W10b — offline `SHIFT_OPEN` ingress + stage_offline_ack extension

- `stage_offline_ack::run` extended to accept `DocType::ShiftOpen` source (today only `SELL` / `RETURN` / `SERVICE_*` / `Z_REPORT`).  Pattern C landing: SHIFT_OPEN doc → `OFFLINE_LOCAL_ACK`; shift state edge `Created → OpenedLocalPendingDrain` (edge 2 per §4.1) fires inside the same `with_immediate` envelope.
- Ingress / write-path wiring routes offline `SHIFT_OPEN` through the W10a policy guard *before* reaching `stage_offline_ack` — reserve=2 check becomes active and `OFFLINE_SHIFT_OPEN_REFUSED_INSUFFICIENT_RESERVE` (Critical) starts firing on real attempts.
- `boot_phase` recovery branch for `OpenedLocalPendingDrain` populated to actually drive the offline `SHIFT_OPEN` doc through W9b drain on return-online (W10a stubbed the arm; W10b lights it up).
- W10b fixtures cover the full offline-open path end to end:
  - `w10b_offline_shift_open_landed_local_ack` — pool=2; offline SHIFT_OPEN consumes 1; shift → `OpenedLocalPendingDrain`; reserve check confirms pool=1 remaining = close-reserve floor.
  - `w10b_offline_shift_open_refused_pool_1` — pool=1 (less than reserve=2 gate); refusal + `OFFLINE_SHIFT_OPEN_REFUSED_INSUFFICIENT_RESERVE` Critical audit.
  - `w10b_offline_shift_open_drain_acks_lands_opened` — happy path: drain SHIFT_OPEN to Ack with empty trailing backlog → shift edge 5 fires → `Opened` after node mode `GoingOnline → Online`.
  - `w10b_offline_shift_open_drain_rejects_lands_manual` — edge 6 fires → `RequiresManualReconciliation`; orphan offline SELL/RETURN docs on the shift remain queryable for operator compensation.

### 12.3 W10a→W10b sequencing rationale

W10a alone is reviewable as "policy decision seam + integration with existing stage_acquire / stage_offline_ack / boot_phase arms".  W10b adds the offline `SHIFT_OPEN` doc-type ingress + the recovery drive — a distinct change surface (touches doc-type whitelist + new ingress validation + drain recovery path).  Splitting matches the W7a/W7b pattern and keeps review focused per slice.  W10b is **mandatory follow-up** before §Task 10 in the plan is considered closed; W10a alone does not satisfy the offline-shift-open use case.

## 13. Out of W14a (this freeze) scope (deferred)

- **State machine is channel-neutral.**  Transport-specific drain evidence (`lastChk` ticket on WebCheck/gRPC vs `/fs/pck` package response on DFS) remains backend-specific; the state machine model is shared across WebCheck/gRPC and future DFS HTTP/XML channels.  Per-channel transport adapter consumes the same state transitions; only the evidence shape that triggers `OpenedLocalPendingDrain → Opened` (or → `RequiresManualReconciliation` on reject) varies by channel.  DFS-side adapter when it lands does not require state machine changes.
- **Offline `SHIFT_OPEN` ingress wiring** is **W10b** per §12.2 — NOT deferred indefinitely.  `stage_offline_ack` extension to accept `DocType::ShiftOpen` lands in the W10b follow-up immediately after the W10a policy-guard PR.  W10a's reserve=2 gate exists as a typed seam that becomes production-active when W10b wires the caller.  §Task 10 in the plan is closed only after W10b merges; W10a alone is insufficient for the offline-shift-open use case.
- **W12 KVT2 confirmation extension** for `SHIFT_OPEN`.  W12 currently confirms `Sent → Kvt1 → Kvt2 → Ack` for fiscal docs; the same shape applies to drained `SHIFT_OPEN`.  Implementation detail for the W12 follow-up; freeze flags it but doesn't specify.
- **Operator UI / dashboard convention.**  Producing collapsed-3-state operator-facing UI vs 9-state forensic dashboard is an operations concern, not a state-machine design concern.  Documented as a recommendation; out of scope to enforce.
- **DFS-channel state mapping.**  Same — channel-neutral state model means DFS adapter inherits the state machine; freeze does not pre-specify the DFS-side drain evidence shape.

## 14. Risks + mitigations

| Risk | Mitigation |
|---|---|
| Migration cost (table rebuild ×2) | W4 lessons captured; pre-pilot status means no production data to migrate |
| Cognitive load on operators reading 9 states | Operator UI collapses to 3 (opened/closing/closed); forensic dashboards expand to 9 |
| Force-error seam misused as escape hatch | `tests/shifts_no_silent_error_paths.rs` scanner test enforces a *two-tier* contract: (a) **`Error` is reachable ONLY via the `force_to_error_with_audit` seam** — no whitelist edge ever lands on `Error`; (b) **`RequiresManualReconciliation` is reachable via specific whitelist edges (4, 6, 12, 14 per §4.1) AND via the `force_to_manual_reconciliation_with_audit` seam — and NOTHING ELSE** (no silent path through `TransitionOutcome::Forbidden` etc.).  The scanner enumerates `transition_state` call sites + audits exhaustively against the whitelist; any silent landing on `Error` OR on `RequiresManualReconciliation` outside (a)+(b) surfaces as a CI failure. |
| `node_state.shift_state` drift from `shifts.state` | Existing mirror-write discipline + tests pin the invariant; expanded states preserve same writer |
| Whitelist matrix size grows (5→14 edges) | Locked-edge count test prevents accidental additions; same shape as `fiscal_documents` W6 pattern |
| New audit Critical events flood dashboards under sustained outage | Future audit-dedup rule (PR #62 §7b L3 residual) addresses cardinality independently of state machine |
| Code paths that pattern-match on `ShiftState` break with new variants | Rust exhaustive `match` enforces compile-time coverage; non-exhaustive paths surface as compiler errors at impl PR time |

## 15. Open questions (explicit — for operator decision before impl PR)

1. **Naming bikeshed**: `OPENED_LOCAL_PENDING_DRAIN` vs `OPENED_LOCALLY_PENDING_DRAIN` vs `LOCAL_OPENED_PENDING_DRAIN` vs `OPENED_OFFLINE_PENDING_CONFIRM`.  Recommend the first (terse, parallels `ClosingLocalPendingDrain`).  Operator to confirm before impl.
2. **`RequiresManualReconciliation` recovery flow** — does the operator-action seam allow `RequiresManualReconciliation → Closed` (compensated close) and `RequiresManualReconciliation → Opened` (compensated re-open)?  Or is it strictly terminal until next `SHIFT_OPEN`?  This freeze proposes terminal-until-next-SHIFT_OPEN; revisit if pilot reveals need for in-state operator compensation.
3. **Reserve = 2 configurability**.  Should the offline-SHIFT_OPEN reserve floor be operator-configurable, or invariant?  Recommend invariant — making it configurable risks operators tightening it to 1 (no close reserve) and re-asserting the 24h trap.  Audit alert if a future config knob is added.
4. **Crash mid-W10-policy-decision behaviour** for ops attempted while shift in transient states.  Crash window analysis specific to W10 implementation; cross-reference at impl PR time.
5. **`Closing → Opened` rollback audit shape** — what audit event surfaces a recoverable rejection that drove rollback?  Propose `SHIFT_ONLINE_CLOSE_RECOVERABLE_REJECT` (Warning), distinct from `SHIFT_CLOSE_DRAIN_REJECTED` (Critical) — operator triage benefit.

---

**Trigger phrase for review**: `проверь shift-state-expansion`.
