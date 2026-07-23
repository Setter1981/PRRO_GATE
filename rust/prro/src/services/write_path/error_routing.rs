//! W10.1 — pure-fn DpsError → routing decision module.
//!
//! Anchored on:
//!   - W8 successor freeze: `docs/superpowers/specs/2026-05-10-m3a-w10-dps-dispatch-design.md` (v3.1).
//!   - ADR-M3-A6 + ADR-M3-A9 step 5-6 (Pattern B retry path).
//!   - W0-3 §2 main table (10 DpsError variants) + §2.1 sub-table
//!     (12 Server-routed status codes).
//!
//! **Pure-fn boundary.**  This module contains NO DB, NO async,
//! NO clock, NO I/O.  Every output is a deterministic function of
//! `(DpsError, DocType, is_live_send)`.  Side effects (counter
//! claims, `node_state.mode` flip, MAC recovery orchestration)
//! belong to `stage_send.rs` (W10.2 / W10.3 / W10.4) — this module
//! merely emits *hints* the caller acts on.
//!
//! **Exhaustive match on `DpsError` (B1 close).**  `DpsError` is
//! NOT `#[non_exhaustive]` (verified at `transports/dps/error.rs:15`).
//! The `route_dps_error` body matches all 10 variants explicitly with
//! NO `_` catch-all — adding an 11th variant breaks the build at
//! compile time, exactly the safety net we want.  Only the raw
//! `Server { code: i32 }` integer dispatch carries a fail-closed
//! `_` arm (since `i32` isn't an enum).
//!
//! **`is_live_send` parameter (MED 2 close, freeze §3.5).**  W10
//! implements **only** the `is_live_send=true` branch (live stage 4
//! send).  `is_live_send=false` is RESERVED for W9 reconciliation —
//! the parameter is threaded through W10 for forward-compat, but
//! the routing fn body currently does NOT differentiate; W9 will
//! introduce the FALSE branch and extend unit-test coverage.
//! Production caller `stage_send::run` passes `true` literally; W10
//! ships ZERO fixtures asserting FALSE behaviour.

use crate::db::models::enums::{DocState, DocType, NodeMode, Severity};
use crate::transports::dps::dto::CheckAck;
use crate::transports::dps::error::{AuthorizationKind, DpsError};

// ─── Public types ───────────────────────────────────────────────────

/// Outer wrapper of the W10 routing surface.  Captures the OK / Err
/// split because `CheckAck.id` is only available on the OK arm and
/// can't be expressed via `RoutingDecision`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireDecision {
    /// `Ok(CheckAck)` — happy path.  Caller (`stage_send::run`)
    /// dispatches: CAS `Sending → Sent` + `set_server_fiscal_no_tx` +
    /// audit `STAGE_SEND_RESULT` + `transport_trace::complete_tx
    /// { outcome_kind: OK }`.
    Sent { server_fiscal_no: String },
    /// `Err(DpsError)` — typed routing per W0-3 §2 + §2.1.
    Routed(RoutingDecision),
}

/// Pure-fn routing of a `DpsError` into a `(target_state, retry_class,
/// audit, side-effect-hints)` decision.  See module docs + freeze §3
/// for the contract; **`target_state` is ALWAYS `DocState`** (B2
/// close) — every live DpsError finalises 4-b in an explicit state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingDecision {
    pub target_state: DocState,
    pub retry_class: RetryClass,
    pub audit_event: AuditEvent,
    pub audit_severity: Severity,
    pub node_mode_flip: Option<NodeMode>,
    pub probe_hint: Option<ProbeHint>,
    pub mac_recovery_hint: Option<MacRecoveryHint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryClass {
    /// Terminal — Routed to `Rejected`; no retry, no recovery.
    /// Applies to: Authorization{DocumentReject},
    /// Server{-2 non-shift, -5, -7..-10, -11, -16}, Server{-15} non-shift,
    /// Server{-12} hash-not-extractable fallback (resolved by stage_send,
    /// not the routing fn).
    TerminalReject,
    /// Transient — re-driven via Pattern B `(ErrorRetryable → Sending → wire)`.
    /// Applies to: Transport, Server{-3}.
    TransientRetry,
    /// Authorization sub-class for FN-config errors (-13 / -14).
    /// Routed to `ErrorRetryable` with audit `STAGE_SEND_FN_NOT_REGISTERED`;
    /// W9 chains `ErrorRetryable → RequiresManualReconciliation`.
    FnConfigError,
    /// Wrapper-side bug or invariant breach: `Internal`,
    /// `ServerFiscalIdMismatch`, `NotFound` on live, `QueryNotSupported`
    /// on live, unknown Server code.  Routed to `ErrorRetryable` with
    /// CRITICAL audit; W9 chains to `RequiresManualReconciliation`.
    /// (B2 close: NotFound + QueryNotSupported on live MUST NOT leave
    /// the doc durably in SENDING.)
    WrapperBug,
    /// Decode (status=0) / `-2` close-shift / `-15` close-shift —
    /// needs a `last_chk` reconciliation probe to disambiguate.  W10
    /// routes to ErrorRetryable + emits `probe_hint`; **W9 performs
    /// the actual probe** (out of W10 scope, freeze §6).
    ProbeRequired,
    /// Server `-12` ERROR_BAD_HASH_PREV — bounded ONE auto-recovery.
    /// Routing fn emits `mac_recovery_hint`; stage_send invokes the
    /// orchestrator (W10.4); on success the worker re-enters 4-pre/
    /// 4a/4b for attempt #2.
    MacRecovery,
    /// `Server{-6}` ERROR_NOT_PREV_ZREPORT — operator-recoverable,
    /// not auto-retried.  Routed via ErrorRetryable →
    /// RequiresManualReconciliation chain (W9 step).
    OperatorEscalation,
    /// Historical B10 tag retained solely to decode rows written by the
    /// withdrawn `-8` chain-settle experiment.  No current routing emits it.
    /// Reconciliation treats it as manual-only, never as permission to
    /// re-send the persisted document.
    DrainChainSettleRetry,
}

impl RetryClass {
    /// Stable wire encoding for `transport_trace.retry_class` and
    /// audit-payload JSON.  These strings are part of a public DB
    /// contract (W10.2 review fix-up + migration 012); changing them
    /// requires a backfill migration.  Variant tags match the Rust
    /// `Debug` form so `format!("{:?}", retry_class)` and
    /// `retry_class.as_str()` stay in sync.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TerminalReject => "TerminalReject",
            Self::TransientRetry => "TransientRetry",
            Self::FnConfigError => "FnConfigError",
            Self::WrapperBug => "WrapperBug",
            Self::ProbeRequired => "ProbeRequired",
            Self::MacRecovery => "MacRecovery",
            Self::OperatorEscalation => "OperatorEscalation",
            Self::DrainChainSettleRetry => "DrainChainSettleRetry",
        }
    }

    /// Inverse of [`as_str`].  Returns `None` for unknown / NULL /
    /// pre-migration-012 rows.  Callers MUST treat `None` as
    /// "indeterminate from durable evidence": the doc is HELD in
    /// `ERROR_RETRYABLE` (not auto-retried, not terminalised) and its
    /// staleness is surfaced via monitoring v1's stuck-FN detector (M1
    /// review item 7, 2026-06-11 — the prior "forwarded to manual triage"
    /// wording was never wired to a state change; held-with-monitoring is
    /// the ruled, intended behaviour).
    pub fn from_wire_str(s: &str) -> Option<Self> {
        Some(match s {
            "TerminalReject" => Self::TerminalReject,
            "TransientRetry" => Self::TransientRetry,
            "FnConfigError" => Self::FnConfigError,
            "WrapperBug" => Self::WrapperBug,
            "ProbeRequired" => Self::ProbeRequired,
            "MacRecovery" => Self::MacRecovery,
            "OperatorEscalation" => Self::OperatorEscalation,
            "DrainChainSettleRetry" => Self::DrainChainSettleRetry,
            _ => return None,
        })
    }
}

/// Closed enum of every audit `event_type` the W10 routing fn may
/// emit on the post-CAS commit.  As-str strings are the canonical
/// wire form; written into `audit_log.event_type`.  Adding a new
/// event requires extending this enum AND a fixture asserting it.
///
/// **F4 close (W10.1 review polish):** distinct variants for
/// `StageSendTransientRetry` (Transport / Server-3) and
/// `StageSendMacHashMismatch` (Server-12 first attempt).  Earlier
/// draft overloaded `StageSendResult` for happy + retry + MAC
/// first-attempt, which collapsed three semantically-distinct events
/// into one wire string and degraded log discoverability.  Distinct
/// strings = distinct grep patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditEvent {
    /// Happy commit on the OK arm (W7-inherited; used by
    /// `stage_send.rs` directly when `WireDecision::Sent` lands).
    /// **Routing fn never emits this directly** — it's only for the
    /// success path which doesn't go through `route_dps_error`.
    StageSendResult,
    /// Transient retry: Transport / Server-3.  `RetryClass::TransientRetry`;
    /// doc routes to ErrorRetryable for re-drive under Pattern B.
    StageSendTransientRetry,
    /// Terminal reject: Authorization{DocumentReject -1}, Server{-2
    /// non-shift, -5, -7..-10, -15 non-shift, -16}.
    StageSendRejected,
    /// `-13` ERROR_NOT_REGISTERED_RRO / `-14` ERROR_NOT_REGISTERED_SIGNER.
    StageSendFnNotRegistered,
    /// Wrapper bugs: `Internal` / `NotFound` on live / `QueryNotSupported`
    /// on live / unknown Server code.  CRITICAL audit; doc routes
    /// to ErrorRetryable (B2 close).
    StageSendWrapperBug,
    /// `ServerFiscalIdMismatch` — distinct CRITICAL forensic event
    /// (PRRO_GATE-5js).
    StageSendFiscalIdMismatch,
    /// `Decode(_)` (typically status=0 UNKNOWN proto-default) — needs
    /// `last_chk` probe (W9 territory).
    StageSendDecodeUnknown,
    /// `-2` close-shift / `-15` close-shift — needs `last_chk` probe
    /// (W9 territory).
    StageSendProbeRequired,
    /// `-11` ERROR_OFFLINE_168 — node_state.mode flips to BLOCKED.
    StageSendNodeBlocked,
    /// `-6` ERROR_NOT_PREV_ZREPORT — operator-recoverable.
    StageSendOperatorEscalation,
    /// `Server{-12}` first-attempt MAC hash mismatch.  Forensic event
    /// for the wire reply that triggers MAC recovery.  Distinct from
    /// `MacRecoveryResigned` (PERSIST commit) and
    /// `MacRecoveryFailedRepeatHashMismatch` (second -12).
    StageSendMacHashMismatch,
    /// MAC recovery: regex extraction failed.  Routed by stage_send
    /// after orchestrator returns `Outcome::HashNotExtractable`.
    MacRecoveryHashNotExtractable,
    /// MAC recovery: PERSIST step succeeded; re-signed payload ready
    /// for attempt #2.  Emitted inside the orchestrator's MR-PERSIST
    /// tx, NOT by the routing fn.
    MacRecoveryResigned,
    /// MAC recovery: second `-12` from attempt #2 (or counter
    /// already burnt on a fresh-after-crash invocation).  Routed by
    /// stage_send via override of `WireDecision::Routed(MacRecovery)`.
    MacRecoveryFailedRepeatHashMismatch,
}

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
    pub reason: ProbeReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeReason {
    /// status=0 UNKNOWN — proto-default, server contract drift suspected.
    DecodeUnknown,
    /// CS-3 Slice E: `-2` (ERROR_CHECK) or `-15` (ERROR_NOT_OPEN_SHIFT) on a
    /// {SHIFT_CLOSE, Z_REPORT} doc — both are close-shift ambiguity that HOLDS for a probe with
    /// zero second wire. The raw code is already discarded in the sealed `CloseAmbiguous` evidence
    /// leaf (`prro-domain` `evidence.rs`), and both arms produced an identical `ProbeRequired`/HELD
    /// `RoutingDecision`, so they unify to ONE reason — an accepted observable audit-label merge
    /// (2→1). No downstream distinguishes `-2` from `-15`; the probe reads `routing_class`, not this.
    CloseShiftProbe,
    /// CS-3 S7-1 (dossier rev9): DPS returned OK (`status==1`) with an EMPTY fiscal id — no
    /// issuance occurred, so the receipt is HELD for a `last_chk` probe rather than treated as a
    /// `Sent{""}`. This is the target for the `OkButNoFiscalNumber` evidence leaf; the pre-cutover
    /// early empty-id guard turned the same condition into a refusal.
    OkButNoFiscalNumber,
    /// CS-3 S7-1 (F3): a TLS-proven `Unauthenticated`/`PermissionDenied` (an authenticated non-DPS
    /// peer, e.g. WAF/gateway) — the evidence classifier maps `RemoteAuthStatus → ProbeRequired`, so
    /// the receipt is HELD for a `last_chk` probe rather than the legacy `TransientRetry` re-drive.
    /// DISTINCT from [`OkButNoFiscalNumber`]: the cause is a remote status, not an empty fiscal id.
    RemoteStatus,
    /// CS-3 Slice E (Pin 2 / Track A): the `UnknownStatus` evidence leaf — a parsed DPS envelope
    /// carrying a status code OUTSIDE the recognized reject/accept enum (e.g. `-4 ERROR_UNKNOWN`,
    /// `-17`, `-99`). The submission crossed the wire and a real envelope came back, but its verdict
    /// is unresolvable, so the receipt is HELD for a `last_chk` probe. DISTINCT from
    /// [`DecodeUnknown`]: that is a proto-default `status==0` (no envelope verdict at all), whereas
    /// this is a NON-zero code the server DID return but the contract does not name.
    ///
    /// **Activated by Pin 3.** Pin 2 pre-wired this leaf's `ProbeRequired` arm in `wire_decision_from`
    /// dormant; Pin 3 flipped `routing_for_indeterminate(UnknownStatus) → ProbeRequired` (`prro-domain`
    /// `mod.rs`, atomic with migration 038), so the classifier now HOLDS this leaf and the projection
    /// emits this probe reason — no `wire_decision_from` change was needed for the flip.
    SubmittedUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacRecoveryHint {
    /// Original wire error_message; orchestrator (W10.4) regex-extracts
    /// the `store {64hex}` fragment.  Pattern: `r"store ([0-9a-fA-F]{64})"`.
    pub raw_error_message: String,
}

// ─── Public routing fns ─────────────────────────────────────────────

/// Outer wrapper: dispatches OK arm to `WireDecision::Sent`, Err arm
/// to `route_dps_error` → `WireDecision::Routed`.
pub fn route_send_result(
    r: Result<CheckAck, DpsError>,
    doc_type: DocType,
    is_live_send: bool,
) -> WireDecision {
    match r {
        Ok(ack) => WireDecision::Sent {
            server_fiscal_no: ack.id,
        },
        Err(err) => WireDecision::Routed(route_dps_error(&err, doc_type, is_live_send)),
    }
}

/// Pure-fn routing of a `DpsError` per W0-3 §2 + §2.1.
///
/// **Exhaustive match (B1 close):** all 10 `DpsError` variants have
/// explicit arms; NO `_` catch-all.  An 11th variant added in the
/// future BREAKS the build at compile time.  Only the raw
/// `Server { code: i32 }` integer dispatch carries a fail-closed `_`
/// arm (since `i32` isn't an enum).
///
/// **`is_live_send` semantics:** W10 implements ONLY `is_live_send=true`.
/// `is_live_send=false` is RESERVED for W9 (freeze §3.5); the parameter
/// is threaded through for forward-compat but does not currently change
/// the routing.  Calling with `false` in W10 yields a routing decision
/// whose contract has NOT been audited and may change without notice.
pub fn route_dps_error(err: &DpsError, doc_type: DocType, is_live_send: bool) -> RoutingDecision {
    match err {
        DpsError::Transport(_) => RoutingDecision {
            target_state: DocState::ErrorRetryable,
            retry_class: RetryClass::TransientRetry,
            // F4 close: distinct audit event for transient retry —
            // forensic clarity vs happy-path StageSendResult.
            audit_event: AuditEvent::StageSendTransientRetry,
            audit_severity: Severity::Warning,
            node_mode_flip: None,
            probe_hint: None,
            mac_recovery_hint: None,
        },
        // CS-3 Slice A′ + RA: gRPC `Unauthenticated` / `PermissionDenied` now arrive as
        // `RemoteStatus` (they were collapsed into `Transport` in `map_tonic_status`).
        // Routing here is IDENTICAL to `Transport` — same `ErrorRetryable` /
        // `TransientRetry` / audit — a compatibility projection.  The slice is NOT
        // behaviour-neutral overall: R1 TLS-gated the emission (a plaintext
        // `Unauthenticated` now routes as `Transport`) and consumers observe the distinct
        // type (`last_chk_probe` / `kvt2_confirm` / `dps_error_class`).  The distinct
        // in-memory type lets slice E (classifier) map RemoteStatus → ProbeRequired
        // without touching the Transport arm.  Separate arm kept intentionally.
        //
        // Slice E maps RemoteStatus → ProbeRequired; the incumbent routing stays
        // TransientRetry here until that classifier lands.
        DpsError::RemoteStatus { .. } => RoutingDecision {
            target_state: DocState::ErrorRetryable,
            retry_class: RetryClass::TransientRetry,
            audit_event: AuditEvent::StageSendTransientRetry,
            audit_severity: Severity::Warning,
            node_mode_flip: None,
            probe_hint: None,
            mac_recovery_hint: None,
        },
        // CS-3 Slice A + RA: a parsed DPS `-4` (ERROR_UNKNOWN) now arrives as
        // `Indeterminate` (it was collapsed into `Transport` at dto.rs). Routing here is
        // IDENTICAL to `Transport` — same `ErrorRetryable` / `TransientRetry` / audit —
        // a compatibility projection.  The `-4` retyping is NOT behaviour-neutral overall:
        // it is observable via `dps_error_class` / probe / confirm consumers.  The
        // distinct in-memory type lets the CS-3 classifier map `-4` to
        // `Parsed(Indeterminate)` (the fence / differentiation is slice E). Kept a
        // SEPARATE arm so E can change it without touching Transport.
        DpsError::Indeterminate { .. } => RoutingDecision {
            target_state: DocState::ErrorRetryable,
            retry_class: RetryClass::TransientRetry,
            audit_event: AuditEvent::StageSendTransientRetry,
            audit_severity: Severity::Warning,
            node_mode_flip: None,
            probe_hint: None,
            mac_recovery_hint: None,
        },
        DpsError::Authorization { kind, .. } => match kind {
            AuthorizationKind::DocumentReject => RoutingDecision {
                target_state: DocState::Rejected,
                retry_class: RetryClass::TerminalReject,
                audit_event: AuditEvent::StageSendRejected,
                audit_severity: Severity::Error,
                node_mode_flip: None,
                probe_hint: None,
                mac_recovery_hint: None,
            },
            AuthorizationKind::FiscalNumberNotRegistered => RoutingDecision {
                target_state: DocState::ErrorRetryable,
                retry_class: RetryClass::FnConfigError,
                audit_event: AuditEvent::StageSendFnNotRegistered,
                audit_severity: Severity::Error,
                node_mode_flip: None,
                probe_hint: None,
                mac_recovery_hint: None,
            },
        },
        DpsError::Decode(_) => RoutingDecision {
            target_state: DocState::ErrorRetryable,
            retry_class: RetryClass::ProbeRequired,
            audit_event: AuditEvent::StageSendDecodeUnknown,
            audit_severity: Severity::Warning,
            node_mode_flip: None,
            probe_hint: Some(ProbeHint {
                reason: ProbeReason::DecodeUnknown,
            }),
            mac_recovery_hint: None,
        },
        DpsError::Server { code, message } => {
            // F1 close: thread is_live_send through forward-compat
            // for W9, even though W10 body doesn't differentiate.
            route_server_code(*code, message, doc_type, is_live_send)
        }
        DpsError::NotFound | DpsError::QueryNotSupported(_) => RoutingDecision {
            // B2 close: live send_chk should never produce these
            // query-only shapes.  Doc must NOT be left durably in
            // SENDING; route to WrapperBug → ErrorRetryable.
            target_state: DocState::ErrorRetryable,
            retry_class: RetryClass::WrapperBug,
            audit_event: AuditEvent::StageSendWrapperBug,
            audit_severity: Severity::Critical,
            node_mode_flip: None,
            probe_hint: None,
            mac_recovery_hint: None,
        },
        DpsError::ServerFiscalIdMismatch { .. } => RoutingDecision {
            target_state: DocState::ErrorRetryable,
            retry_class: RetryClass::WrapperBug,
            audit_event: AuditEvent::StageSendFiscalIdMismatch,
            audit_severity: Severity::Critical,
            node_mode_flip: None,
            probe_hint: None,
            mac_recovery_hint: None,
        },
        DpsError::Internal(_) => RoutingDecision {
            target_state: DocState::ErrorRetryable,
            retry_class: RetryClass::WrapperBug,
            audit_event: AuditEvent::StageSendWrapperBug,
            audit_severity: Severity::Critical,
            node_mode_flip: None,
            probe_hint: None,
            mac_recovery_hint: None,
        },
        // NO `_` catch-all on the enum match — adding an 11th DpsError
        // variant in the future MUST break the build (B1 close).
    }
}

/// Per W0-3 §2.1 sub-table.  12 known codes (-2/-3/-5/-6/-7..-10/-11/
/// -12/-15/-16) have explicit arms; **unknown codes route to
/// `WrapperBug`** (`i32` is not an enum, so a fail-closed `_` arm is
/// the only way to be exhaustive).
///
/// F1 close: `is_live_send` is threaded through the signature for
/// forward-compat with W9.  W10 body does NOT branch on it (FALSE
/// branch RESERVED per freeze §3.5); W9 will introduce reconciliation-
/// side overrides without forcing a signature change.
///
/// **W9 follow-up:** drop the underscore prefix on `_is_live_send`
/// when the FALSE branch lands; F2 pin test
/// (`is_live_send_false_currently_mirrors_true_w10_reserves_for_w9`)
/// will fail when divergence is introduced and must be UPDATED, not
/// deleted, to encode the new W9 contract.
fn route_server_code(
    code: i32,
    message: &str,
    doc_type: DocType,
    _is_live_send: bool,
) -> RoutingDecision {
    match code {
        -2 => {
            // ERROR_CHECK.  W0-3 §2.1 row -2 (post-R-W10-F3 amendment):
            // terminal-business by default; close-shift exception →
            // ProbeRequired.  The original draft had a substring gate
            // on `error_message` indicating "open shift" — DROPPED
            // because DPS message text is not a stable contract;
            // routing on substring left the door open to silently
            // mis-classifying close-shift races as terminal Rejects
            // if DPS rewords the message.  The W9 `last_chk` probe
            // is the durable source-of-truth.  `message` is consumed
            // by the `-12` arm below for MAC recovery hint extraction;
            // here we intentionally do not branch on it.
            if is_close_shift(doc_type) {
                RoutingDecision {
                    target_state: DocState::ErrorRetryable,
                    retry_class: RetryClass::ProbeRequired,
                    audit_event: AuditEvent::StageSendProbeRequired,
                    audit_severity: Severity::Warning,
                    node_mode_flip: None,
                    probe_hint: Some(ProbeHint {
                        reason: ProbeReason::CloseShiftProbe,
                    }),
                    mac_recovery_hint: None,
                }
            } else {
                RoutingDecision {
                    target_state: DocState::Rejected,
                    retry_class: RetryClass::TerminalReject,
                    audit_event: AuditEvent::StageSendRejected,
                    audit_severity: Severity::Critical,
                    node_mode_flip: None,
                    probe_hint: None,
                    mac_recovery_hint: None,
                }
            }
        }
        -3 => RoutingDecision {
            // ERROR_SAVE — transient retry per W0-3 §2.1 row -3
            // (M3 deviates from Python, mirrors WebCheck).
            target_state: DocState::ErrorRetryable,
            retry_class: RetryClass::TransientRetry,
            // F4 close: distinct audit for transient retry.
            audit_event: AuditEvent::StageSendTransientRetry,
            audit_severity: Severity::Warning,
            node_mode_flip: None,
            probe_hint: None,
            mac_recovery_hint: None,
        },
        -5 | -7 | -8 | -9 | -10 => RoutingDecision {
            // ERROR_TYPE / ERROR_XML / ERROR_XML_DATE / ERROR_XML_CHK /
            // ERROR_XML_ZREPORT.  M3 builder/adapter bugs — terminal
            // Rejected, CRITICAL audit so they get fixed in code.
            target_state: DocState::Rejected,
            retry_class: RetryClass::TerminalReject,
            audit_event: AuditEvent::StageSendRejected,
            audit_severity: Severity::Critical,
            node_mode_flip: None,
            probe_hint: None,
            mac_recovery_hint: None,
        },
        -6 => RoutingDecision {
            // ERROR_NOT_PREV_ZREPORT — operator-recoverable; W9 chains
            // ErrorRetryable → RequiresManualReconciliation.
            target_state: DocState::ErrorRetryable,
            retry_class: RetryClass::OperatorEscalation,
            audit_event: AuditEvent::StageSendOperatorEscalation,
            audit_severity: Severity::Error,
            node_mode_flip: None,
            probe_hint: None,
            mac_recovery_hint: None,
        },
        -11 => RoutingDecision {
            // ERROR_OFFLINE_168 — 168-hour cumulative-offline limit
            // exceeded.  Doc terminally Rejected AND node_state.mode
            // flips to BLOCKED (W0-1 §2.4).  CRITICAL audit.
            target_state: DocState::Rejected,
            retry_class: RetryClass::TerminalReject,
            audit_event: AuditEvent::StageSendNodeBlocked,
            audit_severity: Severity::Critical,
            node_mode_flip: Some(NodeMode::Blocked),
            probe_hint: None,
            mac_recovery_hint: None,
        },
        -12 => RoutingDecision {
            // ERROR_BAD_HASH_PREV — MAC recovery class.  Routing fn
            // emits target=ErrorRetryable + mac_recovery_hint; stage_send
            // (W10.4) invokes the orchestrator before completing 4-b.
            // Counter knowledge belongs to stage_send + orchestrator,
            // NOT this fn.
            target_state: DocState::ErrorRetryable,
            retry_class: RetryClass::MacRecovery,
            // F4 close: distinct audit event for first-attempt MAC
            // hash mismatch — separates from happy-path StageSendResult
            // and from the recovery-followup MAC events.
            audit_event: AuditEvent::StageSendMacHashMismatch,
            audit_severity: Severity::Warning,
            node_mode_flip: None,
            probe_hint: None,
            mac_recovery_hint: Some(MacRecoveryHint {
                raw_error_message: message.to_string(),
            }),
        },
        -15 => {
            // ERROR_NOT_OPEN_SHIFT.  doc_type ∈ {SHIFT_CLOSE,
            // Z_REPORT}: ProbeRequired (close-shift race).  Otherwise
            // terminal Rejected (W0-1 §1.4 row 10: ingress guard bug).
            if is_close_shift(doc_type) {
                RoutingDecision {
                    target_state: DocState::ErrorRetryable,
                    retry_class: RetryClass::ProbeRequired,
                    audit_event: AuditEvent::StageSendProbeRequired,
                    audit_severity: Severity::Warning,
                    node_mode_flip: None,
                    probe_hint: Some(ProbeHint {
                        reason: ProbeReason::CloseShiftProbe,
                    }),
                    mac_recovery_hint: None,
                }
            } else {
                RoutingDecision {
                    target_state: DocState::Rejected,
                    retry_class: RetryClass::TerminalReject,
                    audit_event: AuditEvent::StageSendRejected,
                    audit_severity: Severity::Critical,
                    node_mode_flip: None,
                    probe_hint: None,
                    mac_recovery_hint: None,
                }
            }
        }
        -16 => RoutingDecision {
            // ERROR_OFFLINE_ID — M3a is ONLINE-only carve-out per
            // W0-3 §5.  Terminal Rejected with ALERT (encoded as
            // CRITICAL severity); offline failover lives in M3b.
            target_state: DocState::Rejected,
            retry_class: RetryClass::TerminalReject,
            audit_event: AuditEvent::StageSendRejected,
            audit_severity: Severity::Critical,
            node_mode_flip: None,
            probe_hint: None,
            mac_recovery_hint: None,
        },
        // Unknown Server code — fail-closed (B1: i32 isn't an enum).
        // Routes to WrapperBug → ErrorRetryable + CRITICAL audit.
        _ => RoutingDecision {
            target_state: DocState::ErrorRetryable,
            retry_class: RetryClass::WrapperBug,
            audit_event: AuditEvent::StageSendWrapperBug,
            audit_severity: Severity::Critical,
            node_mode_flip: None,
            probe_hint: None,
            mac_recovery_hint: None,
        },
    }
}

/// `doc_type ∈ {SHIFT_CLOSE, Z_REPORT}` — the close-shift family
/// per ADR-M3-A2 boundary mapping.  Used by Server-2 / Server-15
/// dispatch to disambiguate close-shift races from terminal rejects.
fn is_close_shift(doc_type: DocType) -> bool {
    matches!(doc_type, DocType::ShiftClose | DocType::ZReport)
}

// ─── Unit tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ack(id: &str) -> CheckAck {
        CheckAck {
            id: id.into(),
            id_sign: vec![],
            data_sign: vec![],
        }
    }

    // ─── 10 fixtures covering W0-3 §2 main 10 DpsError variants ──────

    #[test]
    fn fixture_01_transport_routes_to_transient_retry() {
        let d = route_dps_error(
            &DpsError::Transport("TLS reset".into()),
            DocType::Sell,
            true,
        );
        assert_eq!(d.target_state, DocState::ErrorRetryable);
        assert_eq!(d.retry_class, RetryClass::TransientRetry);
        // F4 close: distinct audit event for transient retry path.
        assert_eq!(d.audit_event, AuditEvent::StageSendTransientRetry);
        assert_eq!(d.audit_severity, Severity::Warning);
        assert!(d.node_mode_flip.is_none());
        assert!(d.probe_hint.is_none());
        assert!(d.mac_recovery_hint.is_none());
    }

    #[test]
    fn fixture_02_authorization_document_reject_routes_to_terminal() {
        let d = route_dps_error(
            &DpsError::Authorization {
                code: -1,
                kind: AuthorizationKind::DocumentReject,
                message: "ERROR_VEREFY".into(),
            },
            DocType::Sell,
            true,
        );
        assert_eq!(d.target_state, DocState::Rejected);
        assert_eq!(d.retry_class, RetryClass::TerminalReject);
        assert_eq!(d.audit_event, AuditEvent::StageSendRejected);
        assert_eq!(d.audit_severity, Severity::Error);
    }

    #[test]
    fn fixture_03_authorization_fn_not_registered_minus_13() {
        let d = route_dps_error(
            &DpsError::Authorization {
                code: -13,
                kind: AuthorizationKind::FiscalNumberNotRegistered,
                message: "ERROR_NOT_REGISTERED_RRO".into(),
            },
            DocType::Sell,
            true,
        );
        assert_eq!(d.target_state, DocState::ErrorRetryable);
        assert_eq!(d.retry_class, RetryClass::FnConfigError);
        assert_eq!(d.audit_event, AuditEvent::StageSendFnNotRegistered);
        assert_eq!(d.audit_severity, Severity::Error);
    }

    #[test]
    fn fixture_04_authorization_fn_not_registered_minus_14() {
        let d = route_dps_error(
            &DpsError::Authorization {
                code: -14,
                kind: AuthorizationKind::FiscalNumberNotRegistered,
                message: "ERROR_NOT_REGISTERED_SIGNER".into(),
            },
            DocType::Sell,
            true,
        );
        // Same routing as fixture 3 — kind is the discriminator.
        assert_eq!(d.retry_class, RetryClass::FnConfigError);
        assert_eq!(d.audit_event, AuditEvent::StageSendFnNotRegistered);
    }

    #[test]
    fn fixture_05_decode_routes_to_probe_required() {
        let d = route_dps_error(
            &DpsError::Decode("status=0 UNKNOWN".into()),
            DocType::Sell,
            true,
        );
        assert_eq!(d.target_state, DocState::ErrorRetryable);
        assert_eq!(d.retry_class, RetryClass::ProbeRequired);
        assert_eq!(d.audit_event, AuditEvent::StageSendDecodeUnknown);
        assert_eq!(d.audit_severity, Severity::Warning);
        assert_eq!(
            d.probe_hint,
            Some(ProbeHint {
                reason: ProbeReason::DecodeUnknown
            })
        );
    }

    #[test]
    fn fixture_06_not_found_on_live_routes_to_wrapper_bug() {
        // B2 close: NotFound on live send_chk → WrapperBug → ErrorRetryable
        // + CRITICAL audit.  Doc must NOT be left durably in SENDING.
        let d = route_dps_error(&DpsError::NotFound, DocType::Sell, true);
        assert_eq!(d.target_state, DocState::ErrorRetryable);
        assert_eq!(d.retry_class, RetryClass::WrapperBug);
        assert_eq!(d.audit_event, AuditEvent::StageSendWrapperBug);
        assert_eq!(d.audit_severity, Severity::Critical);
    }

    #[test]
    fn fixture_07_server_fiscal_id_mismatch_routes_to_wrapper_bug_distinct_audit() {
        let d = route_dps_error(
            &DpsError::ServerFiscalIdMismatch {
                expected_id: "A".into(),
                actual_id: "B".into(),
            },
            DocType::Sell,
            true,
        );
        assert_eq!(d.target_state, DocState::ErrorRetryable);
        assert_eq!(d.retry_class, RetryClass::WrapperBug);
        // Distinct audit event from the generic WrapperBug — for
        // forensic clarity (PRRO_GATE-5js).
        assert_eq!(d.audit_event, AuditEvent::StageSendFiscalIdMismatch);
        assert_eq!(d.audit_severity, Severity::Critical);
    }

    #[test]
    fn fixture_08_query_not_supported_on_live_routes_to_wrapper_bug() {
        // B2 close — same rationale as fixture 6.
        let d = route_dps_error(
            &DpsError::QueryNotSupported("ByLocalIdentity"),
            DocType::Sell,
            true,
        );
        assert_eq!(d.target_state, DocState::ErrorRetryable);
        assert_eq!(d.retry_class, RetryClass::WrapperBug);
        assert_eq!(d.audit_event, AuditEvent::StageSendWrapperBug);
        assert_eq!(d.audit_severity, Severity::Critical);
    }

    #[test]
    fn fixture_09_internal_routes_to_wrapper_bug() {
        let d = route_dps_error(
            &DpsError::Internal("wrapper bug".into()),
            DocType::Sell,
            true,
        );
        assert_eq!(d.target_state, DocState::ErrorRetryable);
        assert_eq!(d.retry_class, RetryClass::WrapperBug);
        assert_eq!(d.audit_event, AuditEvent::StageSendWrapperBug);
        assert_eq!(d.audit_severity, Severity::Critical);
    }

    #[test]
    fn fixture_10_server_dispatch_delegates_to_route_server_code() {
        // Server { code, .. } is the router that delegates per §2.1
        // sub-table.  Fixtures 11-21 exercise the sub-table directly.
        // Sanity check: a known code routes to the expected shape.
        let d = route_dps_error(
            &DpsError::Server {
                code: -3,
                message: "ERROR_SAVE".into(),
            },
            DocType::Sell,
            true,
        );
        assert_eq!(d.retry_class, RetryClass::TransientRetry);
    }

    // ─── 11 fixtures covering W0-3 §2.1 12 Server-routed codes ──────

    #[test]
    fn fixture_11_server_minus_2_non_shift_routes_to_terminal() {
        let d = route_server_code(-2, "ERROR_CHECK", DocType::Sell, true);
        assert_eq!(d.target_state, DocState::Rejected);
        assert_eq!(d.retry_class, RetryClass::TerminalReject);
        assert_eq!(d.audit_event, AuditEvent::StageSendRejected);
        assert_eq!(d.audit_severity, Severity::Critical);
    }

    #[test]
    fn fixture_12_server_minus_2_close_shift_routes_to_probe_required_message_independent() {
        // F3 close (W10.1 review): per §2.1 row -2 close-shift exception,
        // routing fires for doc_type ∈ {SHIFT_CLOSE, Z_REPORT} REGARDLESS
        // of error_message wording — the substring check on "open shift"
        // was dropped because DPS message text is not a stable contract.
        // The W9 last_chk probe is the durable source-of-truth.
        for dt in [DocType::ShiftClose, DocType::ZReport] {
            // Message intentionally does NOT mention "open shift" —
            // routing must STILL be ProbeRequired.
            let d = route_server_code(-2, "ERROR_CHECK arbitrary text", dt, true);
            assert_eq!(d.target_state, DocState::ErrorRetryable);
            assert_eq!(d.retry_class, RetryClass::ProbeRequired);
            assert_eq!(d.audit_event, AuditEvent::StageSendProbeRequired);
            assert_eq!(
                d.probe_hint,
                Some(ProbeHint {
                    reason: ProbeReason::CloseShiftProbe
                })
            );
        }
    }

    #[test]
    fn fixture_13_server_minus_3_routes_to_transient_retry() {
        let d = route_server_code(-3, "ERROR_SAVE", DocType::Sell, true);
        assert_eq!(d.target_state, DocState::ErrorRetryable);
        assert_eq!(d.retry_class, RetryClass::TransientRetry);
        // F4 close: distinct audit event for transient retry path.
        assert_eq!(d.audit_event, AuditEvent::StageSendTransientRetry);
        assert_eq!(d.audit_severity, Severity::Warning);
    }

    #[test]
    fn fixture_14_server_minus_5_routes_to_terminal() {
        let d = route_server_code(-5, "ERROR_TYPE", DocType::Sell, true);
        assert_eq!(d.target_state, DocState::Rejected);
        assert_eq!(d.retry_class, RetryClass::TerminalReject);
        assert_eq!(d.audit_severity, Severity::Critical);
    }

    #[test]
    fn fixture_15_server_minus_6_routes_to_operator_escalation() {
        let d = route_server_code(-6, "ERROR_NOT_PREV_ZREPORT", DocType::Sell, true);
        assert_eq!(d.target_state, DocState::ErrorRetryable);
        assert_eq!(d.retry_class, RetryClass::OperatorEscalation);
        assert_eq!(d.audit_event, AuditEvent::StageSendOperatorEscalation);
        assert_eq!(d.audit_severity, Severity::Error);
    }

    #[test]
    fn fixture_16_server_minus_7_to_minus_10_xml_class_routes_to_terminal() {
        // Parametrised: XML-class errors all route the same way.
        for code in [-7, -8, -9, -10] {
            let d = route_server_code(code, "ERROR_XML_*", DocType::Sell, true);
            assert_eq!(d.target_state, DocState::Rejected, "code {code}");
            assert_eq!(d.retry_class, RetryClass::TerminalReject, "code {code}");
            assert_eq!(d.audit_event, AuditEvent::StageSendRejected, "code {code}");
            assert_eq!(d.audit_severity, Severity::Critical, "code {code}");
        }
    }

    #[test]
    fn fixture_17_server_minus_11_routes_to_terminal_with_node_blocked_flip() {
        let d = route_server_code(-11, "ERROR_OFFLINE_168", DocType::Sell, true);
        assert_eq!(d.target_state, DocState::Rejected);
        assert_eq!(d.retry_class, RetryClass::TerminalReject);
        assert_eq!(d.audit_event, AuditEvent::StageSendNodeBlocked);
        assert_eq!(d.audit_severity, Severity::Critical);
        assert_eq!(d.node_mode_flip, Some(NodeMode::Blocked));
    }

    #[test]
    fn fixture_18_server_minus_12_routes_to_mac_recovery() {
        let msg = "ERROR_BAD_HASH_PREV: store deadbeef0123456789abcdef0123456789abcdef0123456789abcdef0123456789ab";
        let d = route_server_code(-12, msg, DocType::Sell, true);
        assert_eq!(d.target_state, DocState::ErrorRetryable);
        assert_eq!(d.retry_class, RetryClass::MacRecovery);
        // F4 close: distinct audit for first-attempt MAC hash mismatch.
        assert_eq!(d.audit_event, AuditEvent::StageSendMacHashMismatch);
        assert_eq!(
            d.mac_recovery_hint,
            Some(MacRecoveryHint {
                raw_error_message: msg.to_string()
            })
        );
    }

    #[test]
    fn fixture_19_server_minus_15_close_shift_routes_to_probe_required() {
        for dt in [DocType::ShiftClose, DocType::ZReport] {
            let d = route_server_code(-15, "ERROR_NOT_OPEN_SHIFT", dt, true);
            assert_eq!(d.target_state, DocState::ErrorRetryable);
            assert_eq!(d.retry_class, RetryClass::ProbeRequired);
            assert_eq!(d.audit_event, AuditEvent::StageSendProbeRequired);
            assert_eq!(
                d.probe_hint,
                Some(ProbeHint {
                    reason: ProbeReason::CloseShiftProbe
                })
            );
        }
    }

    #[test]
    fn fixture_20_server_minus_15_non_shift_routes_to_terminal() {
        let d = route_server_code(-15, "ERROR_NOT_OPEN_SHIFT", DocType::Sell, true);
        assert_eq!(d.target_state, DocState::Rejected);
        assert_eq!(d.retry_class, RetryClass::TerminalReject);
        assert_eq!(d.audit_severity, Severity::Critical);
    }

    #[test]
    fn fixture_21_server_minus_16_m3a_routes_to_terminal_alert() {
        // M3a is ONLINE-only (W0-3 §5).  M3b will route to offline-id
        // reconciliation; M3a fails fast.
        let d = route_server_code(-16, "ERROR_OFFLINE_ID", DocType::Sell, true);
        assert_eq!(d.target_state, DocState::Rejected);
        assert_eq!(d.retry_class, RetryClass::TerminalReject);
        assert_eq!(d.audit_severity, Severity::Critical);
    }

    // ─── Server-code fail-closed (B1, i32 dispatch) ─────────────────

    #[test]
    fn unknown_server_code_routes_to_wrapper_bug_fail_closed() {
        // i32 isn't an enum; a `_` arm is required.  W10 fail-closed
        // routing: unknown code → WrapperBug → ErrorRetryable +
        // CRITICAL audit.
        for code in [-99, -42, 100, 999] {
            let d = route_server_code(code, "unknown", DocType::Sell, true);
            assert_eq!(d.target_state, DocState::ErrorRetryable, "code {code}");
            assert_eq!(d.retry_class, RetryClass::WrapperBug, "code {code}");
            assert_eq!(
                d.audit_event,
                AuditEvent::StageSendWrapperBug,
                "code {code}"
            );
            assert_eq!(d.audit_severity, Severity::Critical, "code {code}");
        }
    }

    // ─── route_send_result outer wrapper ────────────────────────────

    #[test]
    fn route_send_result_ok_arm_yields_sent_with_server_fiscal_no() {
        let d = route_send_result(Ok(ack("DPS-FN-7")), DocType::Sell, true);
        assert_eq!(
            d,
            WireDecision::Sent {
                server_fiscal_no: "DPS-FN-7".into()
            }
        );
    }

    #[test]
    fn route_send_result_err_arm_delegates_to_route_dps_error() {
        let d = route_send_result(
            Err(DpsError::Transport("TLS reset".into())),
            DocType::Sell,
            true,
        );
        let WireDecision::Routed(decision) = d else {
            panic!("Err arm must yield Routed, got Sent");
        };
        assert_eq!(decision.retry_class, RetryClass::TransientRetry);
    }

    // ─── RetryClass × AuditEvent sanity ─────────────────────────────

    #[test]
    fn audit_event_as_str_returns_canonical_wire_strings() {
        // Pin the wire-string contract; if a future commit changes
        // any of these strings without intent, this test fails loudly.
        assert_eq!(AuditEvent::StageSendResult.as_str(), "STAGE_SEND_RESULT");
        assert_eq!(
            AuditEvent::StageSendTransientRetry.as_str(),
            "STAGE_SEND_TRANSIENT_RETRY"
        );
        assert_eq!(
            AuditEvent::StageSendRejected.as_str(),
            "STAGE_SEND_REJECTED"
        );
        assert_eq!(
            AuditEvent::StageSendFnNotRegistered.as_str(),
            "STAGE_SEND_FN_NOT_REGISTERED"
        );
        assert_eq!(
            AuditEvent::StageSendWrapperBug.as_str(),
            "STAGE_SEND_WRAPPER_BUG"
        );
        assert_eq!(
            AuditEvent::StageSendFiscalIdMismatch.as_str(),
            "STAGE_SEND_FISCAL_ID_MISMATCH"
        );
        assert_eq!(
            AuditEvent::StageSendDecodeUnknown.as_str(),
            "STAGE_SEND_DECODE_UNKNOWN"
        );
        assert_eq!(
            AuditEvent::StageSendProbeRequired.as_str(),
            "STAGE_SEND_PROBE_REQUIRED"
        );
        assert_eq!(
            AuditEvent::StageSendNodeBlocked.as_str(),
            "STAGE_SEND_NODE_BLOCKED"
        );
        assert_eq!(
            AuditEvent::StageSendOperatorEscalation.as_str(),
            "STAGE_SEND_OPERATOR_ESCALATION"
        );
        assert_eq!(
            AuditEvent::StageSendMacHashMismatch.as_str(),
            "STAGE_SEND_MAC_HASH_MISMATCH"
        );
        assert_eq!(
            AuditEvent::MacRecoveryHashNotExtractable.as_str(),
            "MAC_RECOVERY_HASH_NOT_EXTRACTABLE"
        );
        assert_eq!(
            AuditEvent::MacRecoveryResigned.as_str(),
            "MAC_RECOVERY_RESIGNED"
        );
        assert_eq!(
            AuditEvent::MacRecoveryFailedRepeatHashMismatch.as_str(),
            "MAC_RECOVERY_FAILED_REPEAT_HASH_MISMATCH"
        );
    }

    // ─── F2 close: is_live_send forward-compat pin ──────────────────

    #[test]
    fn is_live_send_false_currently_mirrors_true_w10_reserves_for_w9() {
        // F2 close (W10.1 review): the `is_live_send` parameter is
        // threaded through `route_dps_error` and `route_server_code`
        // for forward-compat with W9 reconciliation.  W10 body does
        // NOT branch on it (per freeze §3.5: `false` branch RESERVED).
        // This test PINS that contract: as long as W10 is on its own,
        // both `true` and `false` must yield identical decisions for
        // the full 10 DpsError variants × representative server codes.
        //
        // When W9 lands and starts diverging routing on `false`, the
        // freeze §3.5 STABILITY NOTE will be lifted and this pin test
        // will be UPDATED (not deleted) to encode the new contract.
        let cases: Vec<DpsError> = vec![
            DpsError::Transport("TLS".into()),
            // CS-3 Slice A′ + RA: RemoteStatus added to pin its ROUTING identity to Transport (a compatibility projection; the slice is NOT behaviour-neutral overall — R1).
            DpsError::RemoteStatus {
                code: "Unauthenticated".into(),
                message: "invalid token".into(),
                digest: prro_domain::delivery::GrpcStatusDigest::from_transport_digest([0xAB; 32]),
            },
            DpsError::Authorization {
                code: -1,
                kind: AuthorizationKind::DocumentReject,
                message: "ERROR_VEREFY".into(),
            },
            DpsError::Authorization {
                code: -13,
                kind: AuthorizationKind::FiscalNumberNotRegistered,
                message: "ERROR_NOT_REGISTERED_RRO".into(),
            },
            DpsError::Decode("status=0".into()),
            DpsError::NotFound,
            DpsError::ServerFiscalIdMismatch {
                expected_id: "A".into(),
                actual_id: "B".into(),
            },
            DpsError::QueryNotSupported("ByLocalIdentity"),
            DpsError::Internal("wrapper".into()),
            DpsError::Server {
                code: -2,
                message: "ERROR_CHECK".into(),
            },
            DpsError::Server {
                code: -3,
                message: "ERROR_SAVE".into(),
            },
            DpsError::Server {
                code: -11,
                message: "ERROR_OFFLINE_168".into(),
            },
            DpsError::Server {
                code: -12,
                message: "ERROR_BAD_HASH_PREV: store deadbeef".into(),
            },
            DpsError::Server {
                code: -15,
                message: "ERROR_NOT_OPEN_SHIFT".into(),
            },
            DpsError::Server {
                code: -99,
                message: "unknown".into(),
            },
        ];
        for err in &cases {
            for dt in [DocType::Sell, DocType::ShiftClose, DocType::ZReport] {
                let live = route_dps_error(err, dt, true);
                let reserved = route_dps_error(err, dt, false);
                assert_eq!(
                    live, reserved,
                    "is_live_send=false must currently mirror is_live_send=true \
                     in W10 (freeze §3.5 RESERVED); err={err:?}, doc_type={dt:?}"
                );
            }
        }
    }

    #[test]
    fn terminal_reject_class_routes_to_rejected_state_only() {
        // Sanity: every TerminalReject decision targets Rejected.
        let cases = [
            route_dps_error(
                &DpsError::Authorization {
                    code: -1,
                    kind: AuthorizationKind::DocumentReject,
                    message: "x".into(),
                },
                DocType::Sell,
                true,
            ),
            route_server_code(-2, "x", DocType::Sell, true),
            route_server_code(-5, "x", DocType::Sell, true),
            route_server_code(-7, "x", DocType::Sell, true),
            route_server_code(-11, "x", DocType::Sell, true),
            route_server_code(-15, "x", DocType::Sell, true),
            route_server_code(-16, "x", DocType::Sell, true),
        ];
        for d in cases {
            assert_eq!(d.retry_class, RetryClass::TerminalReject);
            assert_eq!(d.target_state, DocState::Rejected);
        }
    }

    #[test]
    fn wrapper_bug_class_always_routes_to_error_retryable() {
        // B2 sanity: every WrapperBug decision routes to
        // ErrorRetryable (NEVER Rejected, NEVER stays in SENDING).
        let cases = [
            route_dps_error(&DpsError::NotFound, DocType::Sell, true),
            route_dps_error(&DpsError::QueryNotSupported("x"), DocType::Sell, true),
            route_dps_error(&DpsError::Internal("x".into()), DocType::Sell, true),
            route_dps_error(
                &DpsError::ServerFiscalIdMismatch {
                    expected_id: "A".into(),
                    actual_id: "B".into(),
                },
                DocType::Sell,
                true,
            ),
            route_server_code(-99, "unknown", DocType::Sell, true),
        ];
        for d in cases {
            assert_eq!(d.retry_class, RetryClass::WrapperBug);
            assert_eq!(d.target_state, DocState::ErrorRetryable);
            assert_eq!(d.audit_severity, Severity::Critical);
        }
    }

    #[test]
    fn probe_required_class_carries_probe_hint() {
        let cases = [
            (
                route_dps_error(&DpsError::Decode("x".into()), DocType::Sell, true),
                ProbeReason::DecodeUnknown,
            ),
            (
                route_server_code(-2, "open shift", DocType::ShiftClose, true),
                ProbeReason::CloseShiftProbe,
            ),
            (
                route_server_code(-15, "x", DocType::ZReport, true),
                ProbeReason::CloseShiftProbe,
            ),
        ];
        for (d, expected_reason) in cases {
            assert_eq!(d.retry_class, RetryClass::ProbeRequired);
            assert_eq!(d.target_state, DocState::ErrorRetryable);
            assert_eq!(
                d.probe_hint.as_ref().map(|h| h.reason),
                Some(expected_reason)
            );
        }
    }

    // (CS-3 Slice E Pin 2: `target_wire_decision` + `ok_but_no_fiscal_number_routing` were removed —
    // the empty-SFN `Sent{""}`→held reconciliation is now the STRUCTURAL `OkButNoFiscalNumber` evidence
    // leaf in `wire_decision_from` (`stage_send.rs`), covered end-to-end by
    // `write_path_stage4_send.rs` (empty-SFN → OkButNoFiscalNumber ProbeRequired HELD). This unit test,
    // which exercised the now-deleted legacy-WireDecision seam, is retired with it.)

    #[test]
    fn mac_recovery_class_carries_hint_with_raw_message() {
        let msg = "store ABCDEF0123456789...";
        let d = route_server_code(-12, msg, DocType::Sell, true);
        assert_eq!(d.retry_class, RetryClass::MacRecovery);
        assert_eq!(
            d.mac_recovery_hint
                .as_ref()
                .map(|h| h.raw_error_message.as_str()),
            Some(msg)
        );
    }
}
