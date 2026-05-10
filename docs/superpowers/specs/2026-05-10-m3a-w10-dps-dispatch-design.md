# M3a W10 — DpsError Routing Dispatch (full 8-variant + 12-status-code table) — Design Freeze

**Date:** 2026-05-10
**Status:** v3.1 — third-round 3 doc-drift findings closed (1 MED + 2 LOW/MED); ready for W10.1 apply
**Anchors:** ADR-M3-A6 (full retry policy table), ADR-M3-A9 step 5-6 (Pattern B retry path), W0-3 §2 main + §2.1 sub-table + §9.2 acceptance, M3a plan Task 9
**Predecessor:** W8 (PR #27 + #28, merged `1d29315`) — stage 5 finalize
**Successor:** W9 (App::boot reconciliation; consumes W10's routing in non-live context), W11 (deterministic-replay gate)

---

## 1. Purpose & scope

W10 lands the **complete `DpsError` → `DocState` routing contract** that W7 stage 4 send opted out of (W7-conservative shipped `classify_send_outcome` mapping all `Server { code }` to `Retryable::Server`).  Every wire reply now resolves to an explicit, testable routing decision per W0-3 §2 + §2.1.

**Three concrete deliverables:**
1. **Pure-fn routing module** `services/write_path/error_routing.rs` — `route_dps_error(err, doc_type, is_live_send) → RoutingDecision`.  No DB, no I/O.
2. **Stage 4 wire-in** — `stage_send.rs` replaces W7-minimal `classify_send_outcome` with `route_send_result(...)`.  W7's `SendOutcome` enum is **dropped wholesale**; W10 only exposes `WireDecision::{Sent, Routed(RoutingDecision)}` (see §3).  No facade or compat shim — W7.5 fixtures asserting the old enum shape are updated as part of W10.2.
3. **MAC recovery -12 in-stage path** — regex-extract `store {64hex}`, one bounded re-derive + re-sign + re-send via Pattern B (per ADR-M3-A6 §2.1 row -12).

**Two side effects W10 must wire:**
- `node_state.mode → BLOCKED` on `Server { code: -11 }` (cumulative-offline-168h breach; per W0-3 §2.1 + W0-1 §2.4).
- `last_chk` reconciliation **hint** (deferred execution) for Decode (status=0), `-2` close-shift, `-15` close-shift — W10 routes the doc to `ErrorRetryable` and emits a `probe_required` audit field; W9 reconciliation worker performs the actual `last_chk` probe.

**21 routing fixtures + 1 MAC recovery fixture** per W0-3 §9.2 — the structural proof that ADR-M3-A6 is enforced behaviour, not documentation.

---

## 2. Anchor matrix

| Anchor | Source : line | Constraint |
|---|---|---|
| **ADR-M3-A6** | `docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md:612-647` | W0-3 §2 main + §2.1 sub-table is the binding routing contract.  Three pillars: WebCheck-derived retry classes, per-call gRPC deadline, bounded recovery (default 5). |
| **ADR-M3-A9 step 5-6** | `2026-05-04-m2-pre-plan-adr.md:684-711` | Pattern B retry path: `ErrorRetryable → Sending → wire → Sending → {Sent|Kvt1|Rejected|ErrorRetryable}`.  M3a DPS code MUST NOT use legacy `(ErrorRetryable, Sent)` whitelist `:99` for wire send. |
| **W0-3 §2 main table** | `2026-05-06-m3-w0-3-retry-recovery.md:194-298` | 8 DpsError variants × retry policy + max attempts + backoff + dead-letter + source-state implications. |
| **W0-3 §2.1 sub-table** | `2026-05-06-m3-w0-3-retry-recovery.md:300-348` | 12 Server-routed status codes (the 4 dto-pre-routed codes -1/-4/-13/-14 are covered in §2 main row). |
| **W0-3 §9.2 acceptance** | `2026-05-06-m3-w0-3-retry-recovery.md:1206-1262` | 21 fixtures: 10 §2 main + 11 §2.1 (with `-2`/`-15` two-variant + `-7..-10` parametrised XML-class). |
| **W7 baseline** | `rust/prro/src/services/write_path/stage_send.rs:284-380` | Minimal `classify_send_outcome` shipped W7-conservative — replaced by W10. |
| **DpsError shape** | `rust/prro/src/transports/dps/error.rs:14-93` | 8 variants + `AuthorizationKind` (M2/W4-landed). |

---

## 3. WireDecision + RoutingDecision shape

**Q3 finalised:** `route_send_result(Result<CheckAck, DpsError>, doc_type, is_live_send) → WireDecision`.  The OK arm carries `CheckAck.id` (server_fiscal_no) which can't be expressed via `RoutingDecision` shape; an outer `WireDecision` wrapper preserves it without losing the typed routing for the Err arm.

```rust
/// Pure-fn output of the W10 routing surface.  Wraps the OK arm
/// (which carries `CheckAck.id` for the success path) and the Err
/// arm (which produces a typed RoutingDecision).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireDecision {
    /// `Ok(CheckAck)` from `send_chk` — happy path.  4-b CAS
    /// `Sending → Sent`; `set_server_fiscal_no_tx(server_fiscal_no)`;
    /// audit `STAGE_SEND_RESULT` with outcome=OK.
    Sent { server_fiscal_no: String },
    /// `Err(DpsError)` from `send_chk` — typed routing per
    /// W0-3 §2 + §2.1.  4-b CAS `Sending → decision.target_state`;
    /// dispatch side effects per `decision`.
    Routed(RoutingDecision),
}

/// Pure-fn routing of a `DpsError` into a `(target_state, retry_class,
/// audit, side-effect-hints)` decision.  No DB, no clock, no I/O.
/// Stage 4 (live, is_live_send=true) consumes this; W9 reconciliation
/// will consume the same shape with is_live_send=false.
///
/// **Invariant (B2 close):** `target_state` is ALWAYS `DocState` —
/// every live DpsError finalises 4-b in an explicit state.  No
/// "no DocState transition" branch exists at this layer; W0-3 §2
/// rows that say "no transition" (NotFound, QueryNotSupported)
/// surface here as `WrapperBug` retry class with `target_state =
/// ErrorRetryable` and CRITICAL audit — leaving SENDING durable
/// without a transition would create a recovery-sensitive stuck
/// state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingDecision {
    /// Target DocState after the post-CAS commit.  ALWAYS `DocState`
    /// (no AbortLivePostWire variant).  Per B2 fold:
    /// - Live (is_live_send=true): from `Sending` source.
    /// - Reconciliation (is_live_send=false): from `Sent` source (W9 territory).
    pub target_state: DocState,

    /// Class for retry/backoff bookkeeping + audit severity.
    pub retry_class: RetryClass,

    /// Audit event_type to append on the post-CAS commit (rich payload
    /// composed by the caller).  Closed enum (Q4 close) — every
    /// audit event_type ever emitted by W10 is enumerated here.
    pub audit_event: AuditEvent,

    /// Audit severity (INFO / WARN / ERROR / CRITICAL).
    pub audit_severity: Severity,

    /// Side-effect: flip `node_state.mode` to `BLOCKED` (only for
    /// `Server { code: -11 }`; None otherwise).
    pub node_mode_flip: Option<NodeMode>,

    /// Side-effect: schedule a `last_chk` reconciliation probe (W9
    /// territory).  `Some` for Decode (status=0), `-2` close-shift,
    /// `-15` close-shift.  W10 emits this in the audit payload as
    /// `probe_required = true`; W9 picks up.
    pub probe_hint: Option<ProbeHint>,

    /// Side-effect: invoke the bounded MAC recovery path (only for
    /// `Server { code: -12 }`).  W10 acts on this synchronously in
    /// stage_send::run.
    pub mac_recovery_hint: Option<MacRecoveryHint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryClass {
    /// Terminal — no retry, no recovery.  Routed to `Rejected`.
    /// Applies to: Authorization{DocumentReject},
    /// Server{-2 non-shift,-5,-7..-10,-11,-16}, Server{-15} non-shift,
    /// Server{-12} hash-not-extractable fallback.
    TerminalReject,
    /// Transient — re-driven via Pattern B `(ErrorRetryable → Sending → wire)`.
    /// Bounded by `max_recovery_attempts=5` (default).
    /// Applies to: Transport, Server{-3}, Server{-4} reroute via Transport.
    TransientRetry,
    /// Authorization sub-class for FN-config errors (-13 / -14).
    /// Routed to `ErrorRetryable` with audit `STAGE_SEND_FN_NOT_REGISTERED`;
    /// W9 chains `ErrorRetryable → RequiresManualReconciliation`.
    FnConfigError,
    /// Wrapper-side bug or invariant breach
    /// (Internal, ServerFiscalIdMismatch, NotFound on live send,
    /// QueryNotSupported on live send, unknown Server code).
    /// Routed to `ErrorRetryable` with CRITICAL audit; W9 chains to
    /// `RequiresManualReconciliation`.  NotFound and QueryNotSupported
    /// folded into this class per B2: live send_chk should never
    /// produce these, but if it does, doc must NOT be left durably
    /// in SENDING.
    WrapperBug,
    /// Decode / `-2` close-shift / `-15` close-shift — needs a
    /// `last_chk` probe to disambiguate.  W10 routes to ErrorRetryable
    /// + emits probe_hint; W9 performs the probe.
    ProbeRequired,
    /// Server `-12` ERROR_BAD_HASH_PREV — bounded ONE auto-recovery
    /// (regex-extract + re-derive + re-sign + re-send via Pattern B).
    /// W10 acts synchronously; on success → Sent; on failure → Rejected.
    MacRecovery,
    /// `Server{-6}` ERROR_NOT_PREV_ZREPORT — operator-recoverable,
    /// not auto-retried.  Routed via ErrorRetryable →
    /// RequiresManualReconciliation chain (W9 step).
    OperatorEscalation,
}

/// Closed enum of every audit event_type W10 may emit.  As-str
/// strings are the canonical wire form; written into
/// `audit_log.event_type` TEXT column.  Adding a new event requires
/// extending this enum AND a fixture asserting it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditEvent {
    StageSendResult,                        // happy commit ONLY (OK arm); routing fn never emits this
    StageSendTransientRetry,                // R-W10-F4: Transport / Server-3 (split from StageSendResult)
    StageSendRejected,                      // terminal reject (-1, -2, -5..-10, -15 non-shift, -16, -12 fallback)
    StageSendFnNotRegistered,               // -13 / -14
    StageSendWrapperBug,                    // Internal / NotFound (live) / QueryNotSupported (live)
    StageSendFiscalIdMismatch,              // ServerFiscalIdMismatch (CRITICAL)
    StageSendDecodeUnknown,                 // status=0; probe required
    StageSendProbeRequired,                 // -2/-15 close-shift; probe required
    StageSendNodeBlocked,                   // -11; node_state.mode → BLOCKED
    StageSendOperatorEscalation,            // -6
    StageSendMacHashMismatch,               // R-W10-F4: -12 first attempt (split from StageSendResult)
    MacRecoveryHashNotExtractable,          // -12 with malformed message
    MacRecoveryResigned,                    // -12 successful re-pin + re-sign
    MacRecoveryFailedRepeatHashMismatch,    // -12 → -12 second time
}

// **R-W10-F4 amendment 2026-05-10:** Earlier draft overloaded
// `StageSendResult` for happy + transient retry + MAC first attempt.
// W10.1 review found this collapses three semantically-distinct
// events into one wire string and degrades log discoverability;
// `StageSendTransientRetry` and `StageSendMacHashMismatch` were
// added so each retry-class has a distinct grep pattern.

impl AuditEvent {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StageSendResult => "STAGE_SEND_RESULT",
            Self::StageSendTransientRetry => "STAGE_SEND_TRANSIENT_RETRY",
            Self::StageSendRejected => "STAGE_SEND_REJECTED",
            Self::StageSendFnNotRegistered => "STAGE_SEND_FN_NOT_REGISTERED",
            Self::StageSendWrapperBug => "STAGE_SEND_WRAPPER_BUG",
            Self::StageSendFiscalIdMismatch => "STAGE_SEND_FISCAL_ID_MISMATCH",
            Self::StageSendDecodeUnknown => "STAGE_SEND_DECODE_UNKNOWN",
            Self::StageSendProbeRequired => "STAGE_SEND_PROBE_REQUIRED",
            Self::StageSendNodeBlocked => "STAGE_SEND_NODE_BLOCKED",
            Self::StageSendOperatorEscalation => "STAGE_SEND_OPERATOR_ESCALATION",
            Self::StageSendMacHashMismatch => "STAGE_SEND_MAC_HASH_MISMATCH",
            Self::MacRecoveryHashNotExtractable => "MAC_RECOVERY_HASH_NOT_EXTRACTABLE",
            Self::MacRecoveryResigned => "MAC_RECOVERY_RESIGNED",
            Self::MacRecoveryFailedRepeatHashMismatch => "MAC_RECOVERY_FAILED_REPEAT_HASH_MISMATCH",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeHint {
    /// Why W9 should probe (carried in audit payload for forensics).
    pub reason: ProbeReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeReason {
    /// status=0 UNKNOWN — proto-default, server contract drift suspected.
    DecodeUnknown,
    /// `-2` ERROR_CHECK + doc_type ∈ {SHIFT_CLOSE, Z_REPORT}.
    Code2CloseShift,
    /// `-15` ERROR_NOT_OPEN_SHIFT + doc_type ∈ {SHIFT_CLOSE, Z_REPORT}.
    Code15CloseShift,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacRecoveryHint {
    /// Original wire error message; W10 regex-extracts the expected
    /// `store {64hex}` from it.  If extraction fails → MacRecovery
    /// downgrades to TerminalReject + audit `MacRecoveryHashNotExtractable`.
    pub raw_error_message: String,
}
```

**Why a struct + outer enum:** `WireDecision` enum captures the OK/Err split (CheckAck.id is only on OK).  `RoutingDecision` struct carries the multi-faceted Err shape (target state, retry class, audit, side-effect hints) — an enum would lose information or duplicate variants combinatorially.

### 3.5 `is_live_send` parameter — W10 implements TRUE; FALSE is RESERVED for W9 (MED 2 close)

The `route_dps_error(err, doc_type, is_live_send)` and `route_send_result(...)` signatures both carry the `is_live_send: bool` parameter for forward-compat with W9 reconciliation.  **W10 implements ONLY the `is_live_send=true` branch.**  Calling `route_dps_error` with `is_live_send=false` in W10 returns a `RoutingDecision` whose contract has not yet been finalised (W9 will define the reconciliation-side source-state mapping `Sent → ...` per W0-3 §2 "Source-state implications" column).

**W10 enforcement of the RESERVED status:**
- The unit tests in `error_routing.rs::tests` cover `is_live_send=true` exclusively.  W10 ships ZERO fixtures asserting `is_live_send=false` behaviour.
- Production caller — `stage_send::run` — passes `true` literally at the only call site.  No other caller exists in W10.
- Module doc on `route_dps_error` includes a STABILITY NOTE: «`is_live_send=false` is RESERVED for W9; calling it in W10 yields a routing decision whose contract has not been audited and may change without notice.»
- W9 design freeze (when written) MUST extend the routing fn body to handle `is_live_send=false` AND extend the unit tests to cover both branches.

**Why a parameter at all in W10, not "delete and add later":** the routing fn signature is a stable API surface that W7 baseline `classify_send_outcome` did NOT have.  Threading `is_live_send` through W10 prevents a future signature break that would force W9 to refactor every call site.  The RESERVED treatment lets W10 ship the parameter without committing to its semantics.

---

## 4. Decomposition (5 sub-units)

### 4.1 W10.1 — `error_routing.rs` pure-fn module
- New `rust/prro/src/services/write_path/error_routing.rs`:
  - `pub enum WireDecision`, `pub struct RoutingDecision`, `pub enum RetryClass`, `pub enum AuditEvent` (closed) + `as_str()`, `pub struct ProbeHint`, `pub enum ProbeReason`, `pub struct MacRecoveryHint`.
  - `pub fn route_send_result(r: Result<CheckAck, DpsError>, doc_type: DocType, is_live_send: bool) -> WireDecision` — outer wrapper preserving `CheckAck.id`.
  - `pub fn route_dps_error(err: &DpsError, doc_type: DocType, is_live_send: bool) -> RoutingDecision` — pure-fn the Err arm dispatches to.
  - **Exhaustive `match err` (B1 close).**  `DpsError` is NOT `#[non_exhaustive]` (verified at `transports/dps/error.rs:15`).  Match has explicit arms for ALL 8 variants: Transport / Authorization{kind:DocumentReject} / Authorization{kind:FiscalNumberNotRegistered} / Decode / Server / NotFound / ServerFiscalIdMismatch / QueryNotSupported / Internal — **no `_` catch-all**.  If a future commit adds a 9th DpsError variant, the build BREAKS — exactly the safety net we want; the match-arm forces explicit routing decision rather than silently default-routing.
  - **Server{code:i32} fail-closed default arm (B1 + R-W10-1 close).**  For the `Server { code, message }` arm, dispatch to a private `route_server_code(code, message, doc_type, is_live_send)`.  Inside, match the **12 known codes** (-2/-3/-5/-6/-7..-10/-11/-12/-15/-16) explicitly; trailing `_` arm for **unknown** codes routes to `WrapperBug` retry class with `target_state = ErrorRetryable`, `audit_event = StageSendWrapperBug`, `audit_severity = Critical`.  `i32` is not an enum, so the catch-all is required here (B1 distinction: enum match exhaustiveness vs raw int dispatch).
  - **Live-only routing for query variants (B2 close).**  `NotFound` / `QueryNotSupported` on `is_live_send=true` route to `WrapperBug` → `target_state = ErrorRetryable` + CRITICAL audit `StageSendWrapperBug`.  These shapes should NEVER come from `send_chk` in live; the WrapperBug routing ensures the doc doesn't get stuck in SENDING.  **Reconciliation context (`is_live_send=false`) is RESERVED in W10 (MED 2 close)** — see §3.5; the routing fn signature carries the parameter for forward compat, but W10 unit tests assert that the only currently-supported value is `true`, and W9 will define + ship the FALSE branch.
  - Unit tests inside the file (`mod tests`): 21 cases (10 main + 11 sub-table) + 4 RetryClass sanity checks + 1 unknown-Server-code fail-closed test.  Pure Rust — no DB, no async, sub-millisecond.
- Register `pub mod error_routing;` in `services/write_path/mod.rs`.

### 4.2 W10.2 — Wire routing into `stage_send.rs`

**HIGH 3 close — extend 4-pre source-state CAS to accept `Signed` OR `ErrorRetryable`.**  W7's hardcoded CAS `Signed → Sending` works only for the live first-attempt path.  Pattern B retry path (per ADR-M3-A9 step 5-6) requires `ErrorRetryable → Sending` for any re-attempt — including MAC recovery (W10.4) and W9 boot recovery for stuck `ErrorRetryable` docs.  Without this extension, Pattern B retry-path proof (fixture 21) is impossible to land cleanly.

**New 4-pre CAS dispatch (replaces W7 hardcoded):**

```rust
let inputs = fd::fetch_send_inputs_tx(tx, doc).await?
    .ok_or_else(|| ... DocumentMissing ...)?;

let source_state = match inputs.state {
    DocState::Signed | DocState::ErrorRetryable => inputs.state,
    other => return Ok(PreOutcome::StateConflict { observed: other }),
};

match fd::transition_state(tx, doc, source_state, DocState::Sending).await? {
    TransitionOutcome::Applied => { /* proceed */ }
    TransitionOutcome::Conflict => return Ok(PreOutcome::StateConflict { observed: source_state }),
    TransitionOutcome::NotFound  => return Ok(PreOutcome::DocumentMissing),
    TransitionOutcome::Forbidden => unreachable!(
        "(Signed,Sending) and (ErrorRetryable,Sending) are both whitelisted"
    ),
}
```

Whitelist already carries both edges (`fiscal_documents.rs:158, 164`); W10.2 is purely a stage_send.rs change.

**Routing wire-in.**  Replace W7-minimal `classify_send_outcome` with:

```rust
let outcome = error_routing::route_send_result(
    wire_result,            // Result<CheckAck, DpsError>
    inputs.doc_type,
    /* is_live_send */ true,
);
```

Returns `WireDecision`.  4-b dispatch:
- `WireDecision::Sent { server_fiscal_no }` → CAS `Sending → Sent` + `set_server_fiscal_no_tx` + audit `StageSendResult` + `transport_trace::complete_tx { outcome_kind: OK }`.
- `WireDecision::Routed(decision)` → CAS `Sending → decision.target_state` + side-effects (`node_mode_flip`, `mac_recovery_hint` triggers W10.4 orchestrator before 4-b commit; see §4.4) + audit `decision.audit_event` + `transport_trace::complete_tx { outcome_kind = decision-derived }`.

**`SendOutcome` removal.**  W7's `SendOutcome` enum is dropped wholesale; W10 only exposes `WireDecision` (outer) + `RoutingDecision` (inner).  Existing W7.5 fixtures asserting the old `SendOutcome::{Sent, Rejected, Retryable}` shape are updated to assert `WireDecision::{Sent, Routed}`.

**W3 static scan invariant** stays green (no foreign IO inserted into `with_immediate` closures).

**Caller obligation — retry-loop policy (R-W10.2-review HIGH 1 close).**  4-pre CAS allowlist `(Signed | ErrorRetryable) → Sending` makes `stage_send::run` willing to re-attempt ANY doc currently in `ErrorRetryable`, regardless of which `RetryClass` put it there.  This is intentional — the routing fn is pure; the policy of "which retry classes warrant another wire send" lives one layer up.

The contract:

| `RetryClass` of last attempt | Caller MAY re-invoke `stage_send::run` |
|---|---|
| `TransientRetry` (Transport / Server-3) | YES — back off + re-attempt is the point of `ErrorRetryable` |
| `FnConfigError` (Authorization{FnNotRegistered}, -13/-14) | NO — needs operator (W9 chains via `RequiresManualReconciliation`) |
| `WrapperBug` (Internal / NotFound on live / QueryNotSupported on live / unknown server code / ServerFiscalIdMismatch) | NO — wrapper bug; needs code fix or W9 escalation |
| `ProbeRequired` (Decode / `-2` close-shift / `-15` close-shift) | NO — needs W9 `last_chk` probe to disambiguate |
| `MacRecovery` (`-12` first attempt) | NO — needs W10.4 orchestrator for re-pin + re-sign before retry |
| `OperatorEscalation` (`-6` ERROR_NOT_PREV_ZREPORT) | NO — operator must reconcile a prior Z-report |

Calling `run` repeatedly on a non-`TransientRetry` `ErrorRetryable` doc will produce an unbounded crash-loop: the same envelope ships, the server returns the same status, the doc lands in `ErrorRetryable` again with the same `retry_class`.

**Where the gate lives.**  W10.2 does NOT gate inside `stage_send::run`; the gate is the worker dispatcher's responsibility (W11+).  `transport_trace.last_attempt_for(doc).retry_class` is the natural source-of-truth for the dispatcher's filter.  Until the dispatcher lands, callers (integration tests, ops scripts, ad-hoc replay) must manually respect the table above.  The module-level docstring on `stage_send.rs` carries the same warning.

**Why stage_send doesn't enforce.**  Reading the last trace row inside 4-pre would entangle stage 4 logic with W9 reconciliation policy and force `stage_send::run` to know the retry-class semantics — a layering violation.  Single-policy enforcement at the dispatcher gives one chokepoint that ops + W9 share.

### 4.3 W10.3 — `node_state.mode → BLOCKED` side effect for `-11` ✅ (closed)

Implementation landed in commit `d2a3f91` + R-W10.3-review fix-up:

- `pub async fn set_mode_blocked_tx(tx: &mut WriteTxConn<'_>, fn_id: &str) -> sqlx::Result<bool>` added to `node_state.rs`.  Mirror of `update_last_known_xml_sha_tx`; returns `bool` for missing-row detection per W7.2 / W8.2 convention.
- `stage_send.rs` 4-b closure honours `decision.node_mode_flip == Some(NodeMode::Blocked)` inside the same `with_immediate` envelope as the post-CAS write.  Atomic with the doc-state transition; missing FN row surfaces as typed `StageSendError::NodeStateMissingForBlock` and rolls back the entire 4-b tx — no half-applied state.
- Audit payload extended on the routed arm: `node_mode_flipped: "Blocked"` (PascalCase per R-W10.3-review LOW 1; consistent with `retry_class` + `probe_hint`) recorded alongside `retry_class` and `probe_hint` reason where present (LOW/MED 3 close).
- Future-compat guard: `debug_assert_eq!(target_mode, NodeMode::Blocked)` fires in dev/CI if the routing fn ever emits a different `NodeMode` target without extending stage_send.

### 4.4 W10.4 — MAC recovery `-12` in-stage path

**HIGH 1 + HIGH 2 + LOW 1 close — Pattern B-consistent two-attempt sequence with atomic single-tx PERSIST.**  The earlier draft had two issues: (a) it implicitly created two wire calls without acknowledging that each must produce its own `transport_trace` row, and (b) `mac_recovery_repin_tx` updated `previous_hash` separately from the artifact PERSIST, opening a crash window where the new hash + old XML/sha could be observed.  The v2 design closes both.

**Q1 finalised: dedicated `mac_recovery_attempts` column + migration 013.**  Single-bit budget DDL-enforced via CHECK.

> **Migration numbering note (R-W10.2-review).**  Migration 012 was claimed in W10.2 review fix-up to durably encode `transport_trace.retry_class` (closes the retry-loop policy gap — see §4.2 caller obligation table).  W10.4's MAC-recovery column therefore moves to **migration 013**.  W10.5's `transport_trace.outcome_kind` CHECK extension for `RETRYABLE_MAC_HASH_MISMATCH` (per §3.4) lands in the same migration 013 to keep MAC-recovery DDL atomic.

```sql
-- migrations/013_mac_recovery_attempts.sql
ALTER TABLE fiscal_documents
  ADD COLUMN mac_recovery_attempts INTEGER NOT NULL DEFAULT 0
  CHECK (mac_recovery_attempts IN (0, 1));

-- transport_trace.outcome_kind CHECK extension is a SQLite table
-- rebuild (see migration 008 precedent) to add 'RETRYABLE_MAC_HASH_MISMATCH'.
```

#### 4.4.1 Pattern B-consistent state-machine flow (HIGH 1 close)

The MAC recovery does NOT bypass Pattern B — it lives ABOVE the standard 4-pre/4a/4b cycle.  Two wire attempts ⇒ two `transport_trace` rows.

**Sequence diagram:**

```
attempt #1 — original signed payload
  4-pre tx        : CAS Signed→Sending; trace[1] alloc; submission_attempted_at; audit STAGE_SEND_INTENT_MARKED
  4a (no-tx)      : send_chk(envelope_v1) → Err(Server{-12, "...store {hex}..."})
  classify        : RoutingDecision { target_state=ErrorRetryable, retry_class=MacRecovery, mac_recovery_hint=Some(_) }
  4b tx           : CAS Sending→ErrorRetryable; trace[1].complete(outcome=RETRYABLE_MAC_HASH_MISMATCH); audit StageSendMacHashMismatch

  ↓ stage_send::run sees `decision.mac_recovery_hint == Some(_)`; invokes orchestrator
    BEFORE returning to caller.  The routing fn is pure (no DB) and CANNOT see
    `mac_recovery_attempts` — counter knowledge lives entirely in the orchestrator
    and stage_send loop bookkeeping.

mac_recovery_orchestrator()
  MR-CLAIM tx     : claim counter atomically: state==ErrorRetryable AND attempts==0
                    → SET attempts=1.  No previous_hash write yet (HIGH 2 close).
                    On rows_affected==0: counter already burnt OR wrong state →
                    return Outcome::CounterExhausted.
  MR-NO-TX        : regex_extract_store_hash(error_message) → Option<[u8; 32]>.
                    On None: return Outcome::HashNotExtractable.
                    Build canonical XML using extracted_hash as previous_hash;
                    sha256(unsigned_xml); sign_cms_detached(...).
                    All pure CPU + crypto, OUTSIDE any tx.
  MR-PERSIST tx   : atomic single tx that rewrites the FOUR drift-sensitive artifacts together
                    (HIGH 2 close): previous_hash, unsigned_xml_sha256, document_files{PAYLOAD_XML},
                    document_files{SIGNED_XML}.  Audit MacRecoveryResigned.

  ↓ orchestrator returns Outcome::Resigned; stage_send::run sets local flag
    `mac_recovery_invoked = true` and re-enters the standard 4-pre/4a/4b cycle.

attempt #2 — re-signed payload
  4-pre tx        : source_state = ErrorRetryable; CAS ErrorRetryable→Sending (per HIGH 3 §4.2);
                    trace[2] alloc; submission_attempted_at; audit STAGE_SEND_INTENT_MARKED.
  4a (no-tx)      : send_chk(envelope_v2) → outcome.
  classify        : routing fn is pure — it produces the SAME `RoutingDecision`
                    for any -12 (target=ErrorRetryable, retry_class=MacRecovery,
                    mac_recovery_hint=Some(_)).  It has no notion of "first vs
                    second time".
  4b tx           : stage_send::run inspects `mac_recovery_invoked`:
                    - if true (we already used the budget this run) → OVERRIDE the
                      routing decision: CAS Sending→Rejected; trace[2].complete with
                      `RETRYABLE_MAC_HASH_MISMATCH`-class outcome; audit
                      `MacRecoveryFailedRepeatHashMismatch`.  Do NOT invoke the
                      orchestrator again.
                    - if false (this is a fresh run after a crash, doc could be in
                      ErrorRetryable with attempts already 1) → invoke orchestrator;
                      MR-CLAIM returns rows_affected==0 → Outcome::CounterExhausted →
                      stage_send::run downgrades to the same TerminalReject path
                      (Sending→Rejected + audit MacRecoveryFailedRepeatHashMismatch).

                    For non-(-12) outcomes on attempt #2: standard
                    CAS Sending→decision.target_state; trace[2].complete; audit
                    decision.audit_event.
```

Two `transport_trace` rows materialise: `attempt_no=1` (outcome=RETRYABLE_MAC_HASH_MISMATCH) and `attempt_no=2` (outcome per fresh classify).  Forensic visibility: operator can replay the chain "first envelope rejected with hash X, recovery rebuilt with hash Y, second envelope <result>".  The `transport_trace` PRIMARY KEY `(document_id, attempt_no)` already enforces uniqueness.

**New `OutcomeKind` variant** for `transport_trace.outcome_kind` CHECK list: `RETRYABLE_MAC_HASH_MISMATCH`.  Migration 013 extends the CHECK to include it (table-rebuild per migration 008 precedent — SQLite cannot ALTER an existing CHECK in place).

**Expected per-recovery audit / trace row counts (R-W10.4-senior-review LOW 6 — operator runbook).**  Each MAC-recovery outcome materialises a deterministic number of `audit_log` rows + `transport_trace` rows.  Operators alerting on "doc reached recovery" can use these counts to validate the forensic chain shape before drilling into payloads:

| Outcome | audit_log rows | transport_trace rows |
|---|---|---|
| **Resigned** (happy attempt #2 → Sent) | 5: `STAGE_SEND_INTENT_MARKED` ×2 + `STAGE_SEND_MAC_HASH_MISMATCH` + `MAC_RECOVERY_RESIGNED` + `STAGE_SEND_RESULT` | 2: attempt 1 = `RETRYABLE_MAC_HASH_MISMATCH`, attempt 2 = `OK` |
| **HashNotExtractable** (attempt #1 → orchestrator → Rejected) | 3: `STAGE_SEND_INTENT_MARKED` + `STAGE_SEND_MAC_HASH_MISMATCH` + `MAC_RECOVERY_HASH_NOT_EXTRACTABLE` (+ no caller audit; CAS to Rejected only) | 1: attempt 1 = `RETRYABLE_MAC_HASH_MISMATCH` |
| **CounterExhausted** (attempt #1 → orchestrator MR-CLAIM fails → Rejected) | 3: `STAGE_SEND_INTENT_MARKED` + `STAGE_SEND_MAC_HASH_MISMATCH` + `MAC_RECOVERY_FAILED_REPEAT_HASH_MISMATCH` | 1 |
| **Second `-12`** (Resigned → attempt #2 also `-12` → Rejected) | 5: `STAGE_SEND_INTENT_MARKED` ×2 + `STAGE_SEND_MAC_HASH_MISMATCH` ×2 + `MAC_RECOVERY_RESIGNED` (+ short-circuit emits `MAC_RECOVERY_FAILED_REPEAT_HASH_MISMATCH`) — 6 total | 2: both `RETRYABLE_MAC_HASH_MISMATCH` |
| Resigned → attempt #2 returns terminal-business reject | 5: `STAGE_SEND_INTENT_MARKED` ×2 + `STAGE_SEND_MAC_HASH_MISMATCH` + `MAC_RECOVERY_RESIGNED` + the routed audit per `RetryClass` (e.g. `STAGE_SEND_REJECTED` for `-5`) | 2 |

**Counter / state cross-check** (one row per outcome above):

| Outcome | `mac_recovery_attempts` (post) | doc state (post) |
|---|---|---|
| Resigned → Sent | 1 | `Sent` |
| HashNotExtractable | 0 | `Rejected` |
| CounterExhausted | 1 (was 1 pre-call; helper CAS no-op; rare crash-recovery path) | `Rejected` |
| Second `-12` | 1 | `Rejected` |
| Resigned → other terminal | 1 | per routed decision |

`MAC_RECOVERY_RESIGNED` audit row absence + `mac_recovery_attempts == 1` together signal a recovery that started but never completed (orchestrator crashed between MR-CLAIM and MR-PERSIST).  Operator runbook: pull `transport_trace.last_attempt.retry_class` to disambiguate from a clean attempt-#1 `-12` that hasn't been processed yet.

#### 4.4.2 New helpers (HIGH 2 + LOW 1 close)

**`mac_recovery_claim_counter_tx`** — only claims the budget; does NOT write `previous_hash`.  Lives in `fiscal_documents.rs` (touches only `fiscal_documents` columns):

```rust
pub async fn mac_recovery_claim_counter_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
) -> sqlx::Result<u64> {
    let res = sqlx::query(
        "UPDATE fiscal_documents SET \
            mac_recovery_attempts = mac_recovery_attempts + 1 \
         WHERE document_id = ? \
           AND state = 'ERROR_RETRYABLE' \
           AND mac_recovery_attempts = 0",
    )
    .bind(doc_id)
    .execute(&mut **tx)
    .await?;
    Ok(res.rows_affected())
}
```
- Source-state guard: `ERROR_RETRYABLE` (post-4b commit of attempt #1, NOT `SENDING`).
- Counter guard: `attempts = 0` (first and only attempt).
- DDL CHECK `IN (0, 1)` is the second-line safety: if a future bug somehow tried to increment past 1, INSERT/UPDATE fails loudly.

**`mac_recovery_persist_tx`** — atomic FOUR-write inside one tx.  Lives in **`mac_recovery.rs`** (LOW 1 close — orchestrator-local, not in `fiscal_documents.rs` because it crosses repo boundaries: writes `fiscal_documents` columns AND replaces `document_files` rows):

```rust
// mac_recovery.rs — atomic PERSIST step
async fn persist_resigned(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
    new_previous_hash: &[u8; 32],
    new_unsigned_xml_sha256: &[u8; 32],
    new_payload_xml: &[u8],
    new_signed_xml: &[u8],
) -> sqlx::Result<()> {
    // 1. fiscal_documents: previous_hash + unsigned_xml_sha256 in one UPDATE.
    sqlx::query(
        "UPDATE fiscal_documents SET \
            previous_hash       = ?, \
            unsigned_xml_sha256 = ? \
         WHERE document_id = ?"
    ).bind(&new_previous_hash[..]).bind(&new_unsigned_xml_sha256[..]).bind(doc_id)
        .execute(&mut **tx).await?;
    // 2. document_files: replace PAYLOAD_XML and SIGNED_XML rows.
    //    Uses new document_files::replace_tx helper (LOW 1 close — repo
    //    boundary respected).
    document_files::replace_tx(tx, doc_id, DocumentFileKind::PayloadXml, new_payload_xml).await?;
    document_files::replace_tx(tx, doc_id, DocumentFileKind::SignedXml, new_signed_xml).await?;
    Ok(())
}
```

**`document_files::replace_tx`** — new repo helper (LOW 1 close):
```rust
// document_files.rs — replace existing artifact (uses ON CONFLICT REPLACE
// or DELETE+INSERT to keep PK invariant).  W6 stage 3 only INSERTs;
// this helper is the canonical replace surface for MAC recovery.
pub async fn replace_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
    kind: DocumentFileKind,
    content: &[u8],
) -> sqlx::Result<()>;
```

**Q2 finalised — `re_sign_after_mac_recovery` helper extraction** in `stage_sign.rs`.  Same scope as before: rebuild canonical XML + sign already-pinned/recovered inputs; NO Z alloc, NO `Prepared→Signed` CAS.  Pure no-tx.  W3 invariant holds.

#### 4.4.3 Orchestrator (`mac_recovery.rs`)

```rust
pub(super) enum MacRecoveryOutcome {
    /// MR succeeded; caller MUST re-enter the standard 4-pre/4a/4b
    /// cycle; the doc is now in `ErrorRetryable` with attempts=1 and
    /// fresh artifacts persisted.
    Resigned,
    /// regex extraction failed → terminal Reject path; caller routes
    /// to RoutingDecision with retry_class=TerminalReject + audit
    /// MacRecoveryHashNotExtractable.
    HashNotExtractable,
    /// Counter already burnt OR wrong state → terminal Reject path;
    /// caller routes likewise (audit MacRecoveryFailedRepeatHashMismatch
    /// if the path was the second -12).
    CounterExhausted,
}

pub(super) async fn run_mac_recovery(
    pool: &SqlitePool,
    crypto: Arc<dyn CryptoProvider>,
    session: SigningSession,
    profile: CmsProfile,
    doc: DocumentId,
    hint: &MacRecoveryHint,
) -> Result<MacRecoveryOutcome, StageSendError>
```

Steps:
1. **Hash extraction (pure-fn):** `regex_extract_store_hash(&hint.raw_error_message) → Option<[u8; 32]>`.  Pattern `r"store ([0-9a-fA-F]{64})"`.  Failure → `Outcome::HashNotExtractable`.
2. **MR-CLAIM (with_immediate #1):** `mac_recovery_claim_counter_tx`.  rows_affected==0 → `Outcome::CounterExhausted`.
3. **MR-NO-TX:** read `fiscal_documents.payload_json`, `lnd`, `doc_type`, etc. (separate read, NOT in claim tx; can be done via existing `get_signing_inputs_tx` or a new lightweight helper).  Parse typed payload; build canonical XML using `extracted_hash` as previous_hash.  Sign.
4. **MR-PERSIST (with_immediate #2):** `persist_resigned(...)` — atomic four-write per HIGH 2.  Audit `MacRecoveryResigned` with payload `{old_previous_hash_hex, new_previous_hash_hex, new_unsigned_xml_sha256_hex}` for forensic correlation.
5. Return `Outcome::Resigned`.  Caller (`stage_send::run`) re-enters the standard 4-pre/4a/4b loop; the existing source-state CAS dispatch (per §4.2) accepts `ErrorRetryable` and proceeds with attempt #2.

**Note on ordering and crash recovery:** `attempts` counter is claimed in step 2 BEFORE the artifacts are rewritten in step 4.  Crash between 2 and 4: doc state stays `ErrorRetryable`, `attempts=1`, OLD artifacts.  Worker re-enters via the next tick:

- **stage_send::run** is invoked again on the same doc (still in `ErrorRetryable` from a prior tick that triggered an Err on attempt #1).
- 4-pre CAS `ErrorRetryable → Sending` succeeds (per §4.2 source-state dispatch).  attempt #N trace allocated.
- `send_chk` runs against the OLD signed payload (PERSIST never completed).  Whatever DPS returns, classify produces a `RoutingDecision`.  If it's again `Server{-12}`, classify yields `Routed(MacRecovery)` again.
- `mac_recovery_invoked` flag is FALSE (this is a fresh `run()` invocation; the flag is local).  So stage_send calls the orchestrator.
- Orchestrator MR-CLAIM: `rows_affected==0` (state is `SENDING`, not `ErrorRetryable` — the 4-pre CAS just moved it; AND attempts is already 1 from the earlier crash).  Return `Outcome::CounterExhausted`.
- stage_send::run downgrades to `TerminalReject + MacRecoveryFailedRepeatHashMismatch`.

The `attempts=1 + OLD artifacts` partial state is forensically visible (audit log carries `MacRecoveryResigned` only on PERSIST commit; if absent, recovery never completed) and never silently progresses.  Routing fn never inspects the counter — that's stage_send + orchestrator territory (see Finding-1 close in §3 / sequence diagram).

**LoC budget:** ~280 in `mac_recovery.rs` (orchestrator + persist + extract), ~15 in `fiscal_documents.rs` (claim counter + counter+previous_hash columns join), ~30 in `stage_sign.rs` (extracted re-sign), ~30 in `document_files.rs` (replace_tx), ~50 integration in `stage_send.rs`.  Migration 013 = column ALTER + table-rebuild for `outcome_kind` CHECK extension + 1 schema fixture.

#### 4.4.4 Implementation step plan

W10.4 is large enough (≈400 LoC across 5 files + migration + DDL fixtures + integration fixtures) that it ships as **three commits**, each independently verifiable.  Step 1 is already merged on the branch; step 2 is the current GO target; step 3 lands alongside W10.5 to keep MAC-recovery integration fixtures with the rest of the routing fixtures.

##### Step 1 — schema + claim helper (LANDED, commits `53f2c50` + `b18bfb1`)

**Scope:**
- Migration 013 (`migrations/013_mac_recovery.sql`):
    - `fiscal_documents.mac_recovery_attempts INTEGER NOT NULL DEFAULT 0 CHECK (... IN (0, 1))`.
    - `transport_trace.outcome_kind` CHECK list extended with `'RETRYABLE_MAC_HASH_MISMATCH'` via `defer_foreign_keys = ON` table rebuild (mirrors migration 008 precedent).  Indexes from 010 + 012 re-created.
- `OutcomeKind::RetryableMacHashMismatch` enum variant + `as_str()` mapping.
- `wire_decision_to_outcome_kind` updated: `RetryClass::MacRecovery → OutcomeKind::RetryableMacHashMismatch` (was best-effort `RetryableServer`).
- `fiscal_documents::mac_recovery_claim_counter_tx(tx, doc) → bool` — CAS-guarded helper (state=ERROR_RETRYABLE AND counter=0 → counter=1).
- Tests: 4 DDL fixtures in `tests/migration_013_mac_recovery.rs` + 4 helper fixtures in `tests/fiscal_documents_send_helpers.rs`.

**Verified:** 93 lib + 11 helpers + 4 migration_013 + 7 migration_010 + 16 stage4 + 8 W3 scanner + clippy clean.

##### Step 2 — orchestrator + helpers (in progress)

Step 2 ships as **four review-friendly sub-commits** (2a / 2b / 2c / 2d).  Each sub-commit is independently buildable + testable; cumulative behaviour change happens only at sub-commit 2d (where `stage_send::run` actually invokes the orchestrator).

###### Step 2a — `document_files::replace_tx` (LANDED, commits `9d99159` + `8c14b00`)

**Scope:**
- `pub async fn replace_tx(tx, doc_id, kind, content) -> sqlx::Result<()>` — `INSERT OR REPLACE INTO document_files`.  Single-statement atomic upsert by PK.
- Targeted fixtures in `tests/document_files_replace.rs`: replace overwrite, INSERT-when-missing, FK violation, content byte round-trip, `created_at` reset on REPLACE (5 fixtures total).

**Carry-forward to 2c (R-W10.4-step2a-review LOW 3):** `replace_tx` is intentionally permissive (silent INSERT on missing row).  The orchestrator's MR-PERSIST step MUST surface a typed error before invoking `replace_tx` if the existing PAYLOAD_XML / SIGNED_XML row is absent (W6 stage-3 invariant breach).  See step 2c "Pre-PERSIST assertion" item.

###### Step 2b — `stage_sign::re_sign_after_mac_recovery` extracted helper (NEXT)

**Scope:**
- Extract from existing `stage_sign::run` Pattern A logic.  Same canonical-XML build + sign as W6, BUT:
  - NO Z allocation (already done on attempt #1; the doc has its `lnd` / Z number stable).
  - NO `Prepared → Signed` CAS (the doc is already in `ErrorRetryable` post-attempt-#1 4-b).
  - Inputs: typed payload (re-read from `payload_json` column) + `lnd` + `doc_type` + `new_previous_hash: [u8; 32]` (caller passes the recovered hash from MR-NO-TX regex).
  - Output: `(canonical_unsigned_xml: Vec<u8>, unsigned_xml_sha256: [u8; 32], signed_xml_cms: Vec<u8>)`.
- Pure no-tx (per Q2 finalised in §3 finalised questions; W3 invariant holds).  Caller (orchestrator) feeds the output into MR-PERSIST.
- Targeted lib tests: deterministic build (same input → same output), `previous_hash` propagation (different `new_previous_hash` → different `unsigned_xml_sha256`).

###### Step 2c — `mac_recovery.rs` orchestrator (after 2b)

**Scope:**
- `regex_extract_store_hash(message: &str) → Option<[u8; 32]>` (pure-fn, regex `r"store ([0-9a-fA-F]{64})"`; mirrors Python `dps_fiscal_server.py:494`).
- `MacRecoveryOutcome` enum: `Resigned`, `HashNotExtractable`, `CounterExhausted`.
- `run_mac_recovery(pool, crypto, session, profile, doc, hint) → Result<MacRecoveryOutcome, StageSendError>` orchestrator: regex extract → MR-CLAIM `with_immediate` → MR-NO-TX (read inputs + re-sign via 2b helper) → MR-PERSIST `with_immediate` (atomic four-write per HIGH 2).
- `persist_resigned_tx(tx, doc, new_previous_hash, new_unsigned_xml_sha256, new_payload_xml, new_signed_xml)` — orchestrator-local helper (LOW 1 close — crosses repo boundary, lives in `mac_recovery.rs` not `fiscal_documents.rs`).
- **Pre-PERSIST assertion (R-W10.4-step2a-review LOW 3 close):** before MR-PERSIST opens its `with_immediate` envelope, the orchestrator reads existing PAYLOAD_XML + SIGNED_XML rows via `document_files::get_tx`; if either is `None`, surface a typed `StageSendError::SignedArtifactMissing`-equivalent error (W6 stage-3 invariant breach).  `replace_tx`'s permissive INSERT-on-missing semantic does NOT silently mask the breach.
- Audit: emits `MAC_RECOVERY_HASH_NOT_EXTRACTABLE` / `MAC_RECOVERY_RESIGNED` per freeze §3.4 closed enum.  (Caller — `stage_send::run` in step 2d — emits `MAC_RECOVERY_FAILED_REPEAT_HASH_MISMATCH` on second `-12`.)
- Targeted lib tests:
  - `regex_extract_store_hash` happy + malformed message + non-hex chars + wrong-length hex.
  - `MacRecoveryOutcome` dispatch table (3 variants).
  - Pre-PERSIST assertion: missing PAYLOAD_XML / SIGNED_XML surfaces typed error before `replace_tx` is ever invoked.

###### Step 2d — `stage_send::run` integration (after 2c)

**Scope:**
- After 4-b commit returns and `wire_decision == WireDecision::Routed(d) && d.retry_class == MacRecovery && !mac_recovery_invoked`, invoke `mac_recovery::run_mac_recovery` BEFORE returning to the caller.
- Local `mac_recovery_invoked: bool` flag in `run()` scope tracks budget use within a single invocation.
- Outcome routing:
  - `Resigned` → set flag = true, re-enter the standard 4-pre/4a/4b cycle in a loop (the 4-pre source-state allowlist already accepts `ErrorRetryable → Sending` per §4.2).
  - `HashNotExtractable` → override `wire_decision` to `RoutingDecision { target_state=Rejected, retry_class=TerminalReject, audit_event=MacRecoveryHashNotExtractable, ... }`.  CAS `ErrorRetryable → Rejected` in a follow-up `with_immediate` envelope.  Audit + trace 2 closure.
  - `CounterExhausted` (second `-12` on attempt #2) → similar override but audit `MacRecoveryFailedRepeatHashMismatch`.
- Loop bound: at most ONE re-entry per `run()` invocation (the flag prevents infinite looping).  Crash recovery semantics per §4.4.3 step list paragraph "Note on ordering and crash recovery".
- **W3 scanner**: `mac_recovery::run_mac_recovery` lives at module top level; `MR-NO-TX` (re-sign) executes BETWEEN two `with_immediate` envelopes (MR-CLAIM and MR-PERSIST), exactly the same shape stage 3 / stage 4-pre+4-b uses.  W3 static scan must stay green.

**Verify after step 2d (BEFORE merge to main):**
- `cargo test -p prro --test document_files_replace` → 5 fixtures (LANDED in 2a).
- `cargo test -p prro --test write_path_stage4_send` → still 16 (step 2 leaves existing fixtures unchanged; W10.5 adds the MAC-recovery fixtures).
- `cargo test -p prro --lib` (full) → +N tests for `regex_extract_store_hash` + `MacRecoveryOutcome` dispatch + 2b deterministic-build pin.
- `cargo test -p prro --test with_immediate_no_foreign_io` → still 8 (W3 scanner green; orchestrator IO is OUTSIDE any `with_immediate`).
- `cargo clippy -p prro --tests --no-deps -- -D warnings` → clean.

**Acceptance criteria:**
- All four MR-PERSIST writes commit atomically OR all roll back (HIGH 2 close).
- Counter claim happens in a separate `with_immediate` BEFORE re-sign starts (HIGH 2 close — counter is the budget gate, not the persist gate).
- Pre-PERSIST assertion catches missing PAYLOAD_XML / SIGNED_XML before `replace_tx` is invoked (R-W10.4-step2a LOW 3 close).
- `mac_recovery_invoked` flag prevents the orchestrator from being invoked twice in the same `run()` call (loop-bound proof).
- No foreign IO inside `with_immediate` envelopes.

##### Step 3 — integration fixtures (W10.5 territory)

**Scope** (lives in W10.5 alongside the 21 routing fixtures, per freeze §4.5):
- `tests/write_path_mac_recovery.rs`:
    - **Happy** — `Server{-12}` with extractable hash → re-sign succeeds → attempt #2 OK → doc Sent.
    - **Hash not extractable** — `Server{-12}` with malformed message → `MAC_RECOVERY_HASH_NOT_EXTRACTABLE` audit + doc Rejected.
    - **Repeat -12** — bounded one attempt → second `-12` → doc Rejected with `MAC_RECOVERY_FAILED_REPEAT_HASH_MISMATCH` audit + counter remains 1.
- Stub `DpsChannel` records `(envelope, response)` pairs so the test asserts attempt #1 envelope ≠ attempt #2 envelope (different `previous_hash`).

##### Step ordering rationale

- Step 1 ships **schema-only**; no orchestrator means step 1 is pure additive (column DEFAULT 0 + new closed-enum variant + new helper).  Production deploys can roll forward without behaviour change because no caller invokes `mac_recovery_claim_counter_tx` yet.
- Step 2 ships **orchestrator + integration**, behaviour-changing.  Doc previously routed to `ErrorRetryable` on `-12` will now (a) re-sign (b) re-attempt or (c) terminal Reject after counter exhaustion.  Without step 2 the doc is "stuck" in `ErrorRetryable` for W9 reconciliation — same as the pre-W10.4 baseline.
- Step 3 ships **fixtures**, no behaviour change; just contract pinning.

### 4.5 W10.5 — Test fixtures
- New `rust/prro/tests/write_path_dps_error_routing.rs`: 21 fixtures per W0-3 §9.2 (lines 1218-1256).  Pure-DB integration via stub `DpsChannel` (mirror W7.5 stub pattern); driver invokes `stage_send::run` end-to-end; asserts post-tx `state` + audit event_type + audit payload `retry_class` + (where applicable) `node_state.mode` + `probe_hint`.
  - 1 Transport
  - 2 Authorization{DocumentReject, -1}
  - 3-4 Authorization{FiscalNumberNotRegistered, -13/-14}
  - 5 Decode (status=0) → ErrorRetryable + probe_hint=DecodeUnknown
  - 6 NotFound on live send_chk → WrapperBug → CAS Sending → ErrorRetryable + CRITICAL audit `StageSendWrapperBug` (B2 close: live path never leaves doc durably in SENDING)
  - 7 ServerFiscalIdMismatch
  - 8 QueryNotSupported on live send_chk → WrapperBug → CAS Sending → ErrorRetryable + CRITICAL audit `StageSendWrapperBug` (same B2 rationale as #6 — live path never leaves doc durably in SENDING)
  - 9 Internal
  - 10 Server{-2} non-shift → Rejected
  - 11 Server{-2} close-shift → ErrorRetryable + probe_hint=Code2CloseShift
  - 12 Server{-3} → ErrorRetryable
  - 13 Server{-5} → Rejected
  - 14 Server{-6} → ErrorRetryable (operator escalation hint)
  - 15 Server{-7..-10} parametrised → Rejected
  - 16 Server{-11} → Rejected + node_mode=BLOCKED
  - 17 Server{-12} → covered by W10.4 separate fixture
  - 18 Server{-15} non-shift → Rejected
  - 19 Server{-15} close-shift → ErrorRetryable + probe_hint=Code15CloseShift
  - 20 Server{-16} M3a → Rejected (alert audit)
  - 21 Pattern B retry-path proof — verifies M3a never invokes `(ErrorRetryable, Sent)` whitelist `:99` for wire send (provider spy on a Transport-class fixture; assert spy observed exactly one CAS path through Sending).
- New `rust/prro/tests/write_path_mac_recovery.rs`:
  - Happy: `Server{-12}` with extractable hash → re-sign succeeds → Sent.
  - Hash not extractable: `Server{-12}` with malformed message → `MAC_RECOVERY_HASH_NOT_EXTRACTABLE` + Rejected.
  - Repeat -12 on resend: bounded one attempt → second `-12` → Rejected with `MAC_RECOVERY_FAILED_REPEAT_HASH_MISMATCH`.

---

## 5. Invariants asserted by W10

| # | Invariant | How asserted |
|---|---|---|
| I1 | No network/crypto inside `with_immediate` | `route_dps_error` is pure-fn; MAC recovery re-sign is W6 Pattern A (sign no-tx); W3 static scan stays green |
| I4 | Idempotency mandatory | Every routing decision is deterministic on `(err, doc_type, is_live_send)` — same input → same RoutingDecision → same DocState target |
| I8 | Recovery does not violate state transitions | Pattern B retry path enforced (`ErrorRetryable → Sending → wire`); legacy `(ErrorRetryable, Sent)` whitelist `:99` is NEVER invoked by M3a DPS — fixture 21 proves via provider spy |
| **NEW** | All 8 DpsError variants have explicit routing | `route_dps_error` exhaustive match (no `_` arm); `DpsError` is NOT `#[non_exhaustive]` (verified) — adding a 9th variant breaks the build at compile time, exactly the safety net we want (B1 close) |
| **NEW** | All 12 Server-routed codes have explicit routing | `route_server_code` covers all 12 known codes; unknown `i32` → `WrapperBug` retry class + `target_state = ErrorRetryable` + CRITICAL audit `StageSendWrapperBug` (fail-closed) |
| **NEW (MAC recovery)** | Two `transport_trace` rows per recovered doc | Attempt #1 trace closed with `RETRYABLE_MAC_HASH_MISMATCH` outcome; attempt #2 trace allocated by re-entry into 4-pre after MR-PERSIST (HIGH 1 close) |
| **NEW (MAC recovery)** | Atomic four-write at MR-PERSIST | `previous_hash` + `unsigned_xml_sha256` + `PAYLOAD_XML` + `SIGNED_XML` rewritten under one `BEGIN IMMEDIATE`; counter is claimed in a separate prior tx (HIGH 2 close) |
| **NEW (Pattern B)** | 4-pre source-state CAS accepts `Signed` OR `ErrorRetryable` | New dispatch in §4.2; both edges already in `allowed_transition` whitelist (HIGH 3 close) |
| **NEW** | MAC recovery is bounded to ONE attempt | Recovery counter check + repeat-on-12 fixture |
| **NEW** | NodeMode flip for -11 atomic with state transition | -11 fixture asserts `node_state.mode == 'BLOCKED'` AND doc state == 'REJECTED' post-tx |

---

## 6. Out of scope (intentionally deferred)

- **`last_chk` reconciliation probe execution.**  W10 emits `probe_hint` in audit payload; W9 reconciliation worker performs the actual `last_chk` probe and decides "drive forward" vs "RequiresManualReconciliation".  Rationale: `last_chk` probes are reconciliation-side work (multi-RPC, asynchronous); embedding them in stage 4 live path would inflate the wire-window and complicate W3 invariant.
- **`is_live_send=false` (reconciliation context).**  W10 wires the routing fn for both contexts but only consumes `is_live_send=true` (live stage 4).  W9 will be the first consumer of the FALSE branch.
- **`Sent → Kvt1 → Kvt2` quittance pipeline.**  Out of scope for both W10 and W9; needs a separate slice (post-M3a or W11-adjacent).
- **WebCheck status `0` precise routing.**  W0-3 §2.1 row 0 deviates from Python ("M3 fails loudly on protocol drift").  W10 enforces this: status=0 → Decode → ProbeRequired (probe_hint=DecodeUnknown).
- **Backoff jitter ±20%.**  W0-3 §2 mentions thundering-herd avoidance; W10 ships fixed exponential backoff for retry-class entries.  Jitter is a recovery-worker concern (W9 retry tick); W10 just records `retry_class`.
- **OFFLINE_LOCAL_ACK / offline-pool reconciliation.**  M3b deferred per CLAUDE.md.  `Server{-16}` in M3a is terminal Rejected; M3b will route to offline-id reconciliation.

---

## 7. Open questions — FINALISED

1. **Q1 — MAC recovery counter (closed: A).**  Dedicated `mac_recovery_attempts` column via **migration 013** (renumbered from 012 in W10.2 review fix-up; see §4.4) with `CHECK (mac_recovery_attempts IN (0, 1))` — DDL-enforced single-bit budget.  `recovery_attempts` does not exist in the current schema (verified), so reuse was never an option.  Per §4.4 v2 split: tx helper `mac_recovery_claim_counter_tx` carries `WHERE state='ERROR_RETRYABLE' AND mac_recovery_attempts = 0` (claim phase, post-attempt-#1 4-b commit); `rows_affected == 0` ⇒ counter exhausted OR wrong state ⇒ TerminalReject + audit.  Atomicity of `previous_hash` + artifacts handled in MR-PERSIST step, NOT in claim (HIGH 2 close).
2. **Q2 — Re-sign helper (closed: A, scoped).**  Extract `pub async fn re_sign_after_mac_recovery(...)` from `stage_sign.rs`.  **Scope:** rebuild canonical XML + sign already-pinned/recovered inputs.  **Excludes:** new Z-report number allocation; `Prepared → Signed` CAS.  Pure no-tx segment per W3 invariant.  See §4.4 helper signature.
3. **Q3 — `SendOutcome` shape (closed: modified A).**  Drop W7-minimal `SendOutcome`.  Do NOT replace with bare `RoutingDecision` — the OK arm carries `CheckAck.id` (server_fiscal_no) which RoutingDecision can't express without losing the typed routing structure.  Wrap in `WireDecision::{Sent { server_fiscal_no }, Routed(RoutingDecision)}`.  See §3.
4. **Q4 — Audit event_type (closed: A).**  Closed enum `AuditEvent` with `as_str() -> &'static str`.  All event_types written into `audit_log.event_type` go through this enum; new events require enum extension AND a fixture asserting it.  Mirrors W7.1 `OutcomeKind` precedent.  See §3.
5. **Q5 — Pattern B retry-path proof (closed: B + W7-style spy).**  Two layers:
   - **transport_trace row count.**  Assert exactly 1 trace row per wire attempt for the test doc; duplicate rows from `(ErrorRetryable, Sent)` re-entry would fail.
   - **W7-style fresh-read spy inside `send_chk`.**  Source state for the re-attempt MUST be `ErrorRetryable → Sending`; the spy callback opens a fresh tokio runtime + cloned pool + reads `fiscal_documents.state` and asserts `'SENDING'` (NOT `'SENT'`) at wire-call time.  Mirrors the W7.5 Pattern B ordering proof shape.

---

## 8. Apply order (post-GO)

1. **W10.1 first** — `error_routing.rs` pure-fn + 21 unit tests inside the file.  Pure Rust; no DB; trivial to verify.  Day budget: 1.5d.
2. **W10.2** — wire into `stage_send.rs`, replace W7 minimal classify.  Update existing W7.5 fixtures that asserted W7-conservative `Retryable::Server` shape.  Day budget: 0.5d.
3. **W10.3** — `set_mode_blocked_tx` helper + integration in 4-b closure.  Day budget: 0.25d.
4. **W10.4** — MAC recovery -12 path: hash extractor + re-sign helper extraction + recovery orchestrator + integration into `stage_send::run`.  Day budget: 1.0d.
5. **W10.5** — 21 routing fixtures + MAC recovery fixture + Pattern B retry-path fixture.  Day budget: 1.0d.

Estimated total: **4 days** (matches plan Task 9 budget).

---

## 9. Verify hooks

- `cargo test -p prro --test write_path_dps_error_routing` — 21 fixtures.
- `cargo test -p prro --test write_path_mac_recovery` — 3 fixtures (happy + hash-not-extractable + repeat-on-12).
- `cargo test -p prro --lib error_routing` — 21 unit tests + 4 sanity checks.
- `cargo test -p prro --test with_immediate_no_foreign_io` — W3 scanner stays 8/8 (must not regress when MAC recovery sign helper is invoked).
- `cargo test -p prro` — full suite stays 295+ passed (W10 adds ~25 fixtures).
- `cargo fmt -p prro --check` clean.
- `cargo clippy -p prro --tests --no-deps` — 0 prro warnings.

---

## 10. Carry-forward from earlier slices (active)

- W7 freeze §6: «Full DpsError → DocState dispatch table» — **closed by this slice**.
- W7 freeze §6: «KVT1 inline production path» — still deferred (W11 or post-M3a).
- W8 freeze §6: «`Sent → Kvt1 → Kvt2` quittance pipeline» — still deferred.
- W8 freeze §11 F1-bis (`mark_rejected_tx → Result<bool>`): bd `9qd.1.1`.
- W8 freeze §12 F1-bis (`read_state_tx` thinner helper): bd `9qd.1.1`.
- W8 freeze §13 (this PR's senior-review tracking pattern): apply to W10 if review surfaces findings.

---

## 11. Risks (quick scan)

- **R-W10-1 (low, mitigated by exhaustive match):** DpsError is **NOT** `#[non_exhaustive]` (verified at `transports/dps/error.rs:15`).  W10 leverages this: `route_dps_error` uses an exhaustive match with no `_` catch-all — adding a 9th DpsError variant in the future BREAKS the build, exactly the safety net we want.  The reviewer flagged this in B1: enum-level safety is structural; only the raw `i32` Server-code dispatch needs a fail-closed `_` arm (since `i32` isn't an enum).
- **R-W10-2 (low):** MAC recovery re-sign uses W6 helpers; if W6 internals change shape, MAC recovery breaks.  Mitigation: extracted helper `re_sign_after_mac_recovery` — explicit API boundary; W6 changes must update the helper.
- **R-W10-3 (low):** `Server{-12}` regex extraction depends on DPS server message format `store {64hex}`.  If DPS changes the format, MAC recovery falls back to `HashNotExtractable` → terminal Rejected.  Acceptable; not a regression vs current state where W7 routes -12 to generic Retryable.  Forensic audit captures the original message for operator review.
- **R-W10-4 (medium):** `is_live_send` parameter — if a future caller passes the wrong bool, routing decision targets the wrong source state.  Mitigation: caller convention enforced at the call sites — stage_send.rs always passes `true`; W9 reconciliation worker always passes `false`.  Documented on the fn doc.

---

## 12. Sign-off checklist (v3 — second-round findings closed)

**v2 first-round closures (preserved):**
- [x] All 5 open questions in §7 finalised (Q1=A, Q2=A scoped, Q3=modified A, Q4=A, Q5=B+spy).
- [x] §8 apply order accepted (W10.1 → W10.5).
- [x] §6 out-of-scope items frozen.
- [x] B1 — exhaustive `match err` on `DpsError` (no `_`); only `Server{code:i32}` raw-int dispatch has fail-closed `_`.
- [x] B2 — `target_state` always `DocState`; NotFound/QueryNotSupported on live → ErrorRetryable+WrapperBug+CRITICAL.
- [x] B3 — atomic multi-write at MR-PERSIST.
- [x] B4 — `mac_recovery_*` helpers distinct from W6 pin.

**v3.1 third-round closures:**
- [x] **MED (repeat-12 attribution)** — repeat-12 detection now correctly attributed to stage_send + orchestrator boundary, NOT routing fn.  Routing fn is pure (no DB, no counter).  stage_send carries `mac_recovery_invoked` local flag; orchestrator's `MR-CLAIM` returns `Outcome::CounterExhausted` when budget already burnt.  Both paths converge on `TerminalReject + MacRecoveryFailedRepeatHashMismatch`.  See §4.4.1 sequence diagram + §4.4.3 ordering note.
- [x] **LOW/MED (stale §1 SendOutcome phrase)** — §1 paragraph 2 rewritten: «W7's `SendOutcome` enum is dropped wholesale; W10 only exposes `WireDecision`».  No facade or compat shim.
- [x] **LOW/MED (stale fixtures 6 + 8 "no state mutation")** — §4.5 fixture list updated: NotFound/QueryNotSupported on live send_chk → WrapperBug → CAS `Sending → ErrorRetryable` + CRITICAL audit `StageSendWrapperBug`.  Aligned with B2 close in §3 + §4.1.

**v3 second-round closures:**
- [x] **HIGH 1 (MAC recovery trace flow)** — explicit two-attempt sequence per §4.4.1; trace[1] closed with `RETRYABLE_MAC_HASH_MISMATCH` on 4-b commit of attempt #1; trace[2] allocated by 4-pre re-entry after MR-PERSIST.  Each wire call has its own forensic row.
- [x] **HIGH 2 (MAC recovery atomicity)** — claim phase only updates the counter (no `previous_hash` write); PERSIST phase atomically rewrites `previous_hash` + `unsigned_xml_sha256` + `PAYLOAD_XML` + `SIGNED_XML` under one `with_immediate`.  Crash window between claim and PERSIST: counter burnt + old artifacts → re-entry routes to TerminalReject (forensically visible, never silent progress).
- [x] **HIGH 3 (4-pre source-state CAS)** — explicit dispatch in §4.2 accepts `Signed` OR `ErrorRetryable`; anything else returns `StateConflict`.  Whitelist already carries both edges.  Pattern B retry-path proof (fixture 21) lands cleanly via this dispatch.
- [x] **MED 1 (stale references)** — §5 invariants table rewritten; §7 Q1 description aligned with §4.4 v2 helpers; §3.5 added documenting `is_live_send` semantics.
- [x] **MED 2 (`is_live_send=false` underdefined)** — §3.5 marks the FALSE branch RESERVED for W9.  W10 ships ZERO fixtures asserting it; production caller passes `true` literally; module doc carries STABILITY NOTE.
- [x] **LOW 1 (repo boundary)** — `mac_recovery_persist_tx` lives in `mac_recovery.rs` (orchestrator-local) and uses new `document_files::replace_tx` helper.  `fiscal_documents.rs` only carries the counter-claim helper.

GO confirmed for W10.1 → W10.5 apply order.
