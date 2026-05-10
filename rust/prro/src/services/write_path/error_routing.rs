//! W10.1 — pure-fn DpsError → routing decision module.
//!
//! Anchored on:
//!   - W8 successor freeze: `docs/superpowers/specs/2026-05-10-m3a-w10-dps-dispatch-design.md` (v3.1).
//!   - ADR-M3-A6 + ADR-M3-A9 step 5-6 (Pattern B retry path).
//!   - W0-3 §2 main table (8 DpsError variants) + §2.1 sub-table
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
//! The `route_dps_error` body matches all 8 variants explicitly with
//! NO `_` catch-all — adding a 9th variant breaks the build at
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
}

/// Closed enum of every audit `event_type` the W10 routing fn may
/// emit on the post-CAS commit.  As-str strings are the canonical
/// wire form; written into `audit_log.event_type`.  Adding a new
/// event requires extending this enum AND a fixture asserting it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditEvent {
    /// Happy commit (W7-inherited, used by `stage_send.rs` directly
    /// for OK arm) AND transient retry (Transport / Server-3 — used
    /// by routing fn for `RetryClass::TransientRetry`).  Differentiation
    /// by audit payload `outcome_kind` field.  AND first-attempt
    /// MAC-recovery (Server{-12}) — same string, payload carries
    /// `retry_class=MacRecovery` for forensics.
    StageSendResult,
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
            Self::StageSendRejected => "STAGE_SEND_REJECTED",
            Self::StageSendFnNotRegistered => "STAGE_SEND_FN_NOT_REGISTERED",
            Self::StageSendWrapperBug => "STAGE_SEND_WRAPPER_BUG",
            Self::StageSendFiscalIdMismatch => "STAGE_SEND_FISCAL_ID_MISMATCH",
            Self::StageSendDecodeUnknown => "STAGE_SEND_DECODE_UNKNOWN",
            Self::StageSendProbeRequired => "STAGE_SEND_PROBE_REQUIRED",
            Self::StageSendNodeBlocked => "STAGE_SEND_NODE_BLOCKED",
            Self::StageSendOperatorEscalation => "STAGE_SEND_OPERATOR_ESCALATION",
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
    /// `-2` ERROR_CHECK + doc_type ∈ {SHIFT_CLOSE, Z_REPORT}.
    Code2CloseShift,
    /// `-15` ERROR_NOT_OPEN_SHIFT + doc_type ∈ {SHIFT_CLOSE, Z_REPORT}.
    Code15CloseShift,
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
/// **Exhaustive match (B1 close):** all 8 `DpsError` variants have
/// explicit arms; NO `_` catch-all.  A 9th variant added in the
/// future BREAKS the build at compile time.  Only the raw
/// `Server { code: i32 }` integer dispatch carries a fail-closed `_`
/// arm (since `i32` isn't an enum).
///
/// **`is_live_send` semantics:** W10 implements ONLY `is_live_send=true`.
/// `is_live_send=false` is RESERVED for W9 (freeze §3.5); the parameter
/// is threaded through for forward-compat but does not currently change
/// the routing.  Calling with `false` in W10 yields a routing decision
/// whose contract has NOT been audited and may change without notice.
pub fn route_dps_error(err: &DpsError, doc_type: DocType, _is_live_send: bool) -> RoutingDecision {
    match err {
        DpsError::Transport(_) => RoutingDecision {
            target_state: DocState::ErrorRetryable,
            retry_class: RetryClass::TransientRetry,
            audit_event: AuditEvent::StageSendResult,
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
        DpsError::Server { code, message } => route_server_code(*code, message, doc_type),
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
        // NO `_` catch-all on the enum match — adding a 9th DpsError
        // variant in the future MUST break the build (B1 close).
    }
}

/// Per W0-3 §2.1 sub-table.  12 known codes (-2/-3/-5/-6/-7..-10/-11/
/// -12/-15/-16) have explicit arms; **unknown codes route to
/// `WrapperBug`** (`i32` is not an enum, so a fail-closed `_` arm is
/// the only way to be exhaustive).
fn route_server_code(code: i32, message: &str, doc_type: DocType) -> RoutingDecision {
    match code {
        -2 => {
            // ERROR_CHECK.  W0-3 §2.1 row -2: terminal-business by
            // default; close-shift exception (doc_type ∈ {SHIFT_CLOSE,
            // Z_REPORT} AND error_message indicates "open shift") →
            // ProbeRequired.
            if is_close_shift(doc_type) && message.to_lowercase().contains("open shift") {
                RoutingDecision {
                    target_state: DocState::ErrorRetryable,
                    retry_class: RetryClass::ProbeRequired,
                    audit_event: AuditEvent::StageSendProbeRequired,
                    audit_severity: Severity::Warning,
                    node_mode_flip: None,
                    probe_hint: Some(ProbeHint {
                        reason: ProbeReason::Code2CloseShift,
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
            audit_event: AuditEvent::StageSendResult,
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
            audit_event: AuditEvent::StageSendResult,
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
                        reason: ProbeReason::Code15CloseShift,
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

    // ─── 10 fixtures covering W0-3 §2 main 8 DpsError variants ──────

    #[test]
    fn fixture_01_transport_routes_to_transient_retry() {
        let d = route_dps_error(
            &DpsError::Transport("TLS reset".into()),
            DocType::Sell,
            true,
        );
        assert_eq!(d.target_state, DocState::ErrorRetryable);
        assert_eq!(d.retry_class, RetryClass::TransientRetry);
        assert_eq!(d.audit_event, AuditEvent::StageSendResult);
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
        let d = route_server_code(-2, "ERROR_CHECK", DocType::Sell);
        assert_eq!(d.target_state, DocState::Rejected);
        assert_eq!(d.retry_class, RetryClass::TerminalReject);
        assert_eq!(d.audit_event, AuditEvent::StageSendRejected);
        assert_eq!(d.audit_severity, Severity::Critical);
    }

    #[test]
    fn fixture_12_server_minus_2_close_shift_routes_to_probe_required() {
        // Per §2.1 row -2: doc_type ∈ {SHIFT_CLOSE, Z_REPORT} +
        // error_message includes "open shift" → ProbeRequired.
        for dt in [DocType::ShiftClose, DocType::ZReport] {
            let d = route_server_code(-2, "no open shift on RRO", dt);
            assert_eq!(d.target_state, DocState::ErrorRetryable);
            assert_eq!(d.retry_class, RetryClass::ProbeRequired);
            assert_eq!(d.audit_event, AuditEvent::StageSendProbeRequired);
            assert_eq!(
                d.probe_hint,
                Some(ProbeHint {
                    reason: ProbeReason::Code2CloseShift
                })
            );
        }
    }

    #[test]
    fn fixture_13_server_minus_3_routes_to_transient_retry() {
        let d = route_server_code(-3, "ERROR_SAVE", DocType::Sell);
        assert_eq!(d.target_state, DocState::ErrorRetryable);
        assert_eq!(d.retry_class, RetryClass::TransientRetry);
        assert_eq!(d.audit_event, AuditEvent::StageSendResult);
        assert_eq!(d.audit_severity, Severity::Warning);
    }

    #[test]
    fn fixture_14_server_minus_5_routes_to_terminal() {
        let d = route_server_code(-5, "ERROR_TYPE", DocType::Sell);
        assert_eq!(d.target_state, DocState::Rejected);
        assert_eq!(d.retry_class, RetryClass::TerminalReject);
        assert_eq!(d.audit_severity, Severity::Critical);
    }

    #[test]
    fn fixture_15_server_minus_6_routes_to_operator_escalation() {
        let d = route_server_code(-6, "ERROR_NOT_PREV_ZREPORT", DocType::Sell);
        assert_eq!(d.target_state, DocState::ErrorRetryable);
        assert_eq!(d.retry_class, RetryClass::OperatorEscalation);
        assert_eq!(d.audit_event, AuditEvent::StageSendOperatorEscalation);
        assert_eq!(d.audit_severity, Severity::Error);
    }

    #[test]
    fn fixture_16_server_minus_7_to_minus_10_xml_class_routes_to_terminal() {
        // Parametrised: XML-class errors all route the same way.
        for code in [-7, -8, -9, -10] {
            let d = route_server_code(code, "ERROR_XML_*", DocType::Sell);
            assert_eq!(d.target_state, DocState::Rejected, "code {code}");
            assert_eq!(d.retry_class, RetryClass::TerminalReject, "code {code}");
            assert_eq!(d.audit_event, AuditEvent::StageSendRejected, "code {code}");
            assert_eq!(d.audit_severity, Severity::Critical, "code {code}");
        }
    }

    #[test]
    fn fixture_17_server_minus_11_routes_to_terminal_with_node_blocked_flip() {
        let d = route_server_code(-11, "ERROR_OFFLINE_168", DocType::Sell);
        assert_eq!(d.target_state, DocState::Rejected);
        assert_eq!(d.retry_class, RetryClass::TerminalReject);
        assert_eq!(d.audit_event, AuditEvent::StageSendNodeBlocked);
        assert_eq!(d.audit_severity, Severity::Critical);
        assert_eq!(d.node_mode_flip, Some(NodeMode::Blocked));
    }

    #[test]
    fn fixture_18_server_minus_12_routes_to_mac_recovery() {
        let msg = "ERROR_BAD_HASH_PREV: store deadbeef0123456789abcdef0123456789abcdef0123456789abcdef0123456789ab";
        let d = route_server_code(-12, msg, DocType::Sell);
        assert_eq!(d.target_state, DocState::ErrorRetryable);
        assert_eq!(d.retry_class, RetryClass::MacRecovery);
        assert_eq!(d.audit_event, AuditEvent::StageSendResult);
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
            let d = route_server_code(-15, "ERROR_NOT_OPEN_SHIFT", dt);
            assert_eq!(d.target_state, DocState::ErrorRetryable);
            assert_eq!(d.retry_class, RetryClass::ProbeRequired);
            assert_eq!(d.audit_event, AuditEvent::StageSendProbeRequired);
            assert_eq!(
                d.probe_hint,
                Some(ProbeHint {
                    reason: ProbeReason::Code15CloseShift
                })
            );
        }
    }

    #[test]
    fn fixture_20_server_minus_15_non_shift_routes_to_terminal() {
        let d = route_server_code(-15, "ERROR_NOT_OPEN_SHIFT", DocType::Sell);
        assert_eq!(d.target_state, DocState::Rejected);
        assert_eq!(d.retry_class, RetryClass::TerminalReject);
        assert_eq!(d.audit_severity, Severity::Critical);
    }

    #[test]
    fn fixture_21_server_minus_16_m3a_routes_to_terminal_alert() {
        // M3a is ONLINE-only (W0-3 §5).  M3b will route to offline-id
        // reconciliation; M3a fails fast.
        let d = route_server_code(-16, "ERROR_OFFLINE_ID", DocType::Sell);
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
            let d = route_server_code(code, "unknown", DocType::Sell);
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
            route_server_code(-2, "x", DocType::Sell),
            route_server_code(-5, "x", DocType::Sell),
            route_server_code(-7, "x", DocType::Sell),
            route_server_code(-11, "x", DocType::Sell),
            route_server_code(-15, "x", DocType::Sell),
            route_server_code(-16, "x", DocType::Sell),
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
            route_server_code(-99, "unknown", DocType::Sell),
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
                route_server_code(-2, "open shift", DocType::ShiftClose),
                ProbeReason::Code2CloseShift,
            ),
            (
                route_server_code(-15, "x", DocType::ZReport),
                ProbeReason::Code15CloseShift,
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

    #[test]
    fn mac_recovery_class_carries_hint_with_raw_message() {
        let msg = "store ABCDEF0123456789...";
        let d = route_server_code(-12, msg, DocType::Sell);
        assert_eq!(d.retry_class, RetryClass::MacRecovery);
        assert_eq!(
            d.mac_recovery_hint
                .as_ref()
                .map(|h| h.raw_error_message.as_str()),
            Some(msg)
        );
    }
}
