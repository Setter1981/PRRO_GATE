//! CS-3 S7-APPLY-GRAPH — the routing-projection normative lock (design §4.1 / §8, rev10 oracle
//! `docs/CS3_S7_APPLYPLAN_RECONCILIATION.md`).
//!
//! Locks the FULL `RoutingDecision` (all 7 fields) the live send path produces for every wire
//! outcome, so the S7-1 cutover cannot silently drift it. This is stronger than the existing
//! `grpc.rs` §4.6 `drift_check` (which normalises to retry-class only): it pins `audit_event` /
//! `audit_severity` / `probe_hint` / `node_mode_flip` too — the dims a normalised pin would miss.
//! The route-vs-classify DELTA set (3 declared deltas) is validated by `drift_check`; this pin owns
//! the incumbent full-tuple lock + the rev10 `MissingStatus` audit scoping.
//!
//! **Faithful decode (crate-internal reason):** inputs are built via the REAL decode
//! `observe_check_reply(chk(status, id))` — NOT hand-built `DpsError` — because the leaf → `DpsError`
//! mapping is non-obvious (e.g. `-1` → `Authorization{DocumentReject}`, `-4` → `Indeterminate`,
//! `0`/outside-enum → `Decode`; `route_server_code` has no `-1`/`-4` arm). `observe_check_reply` is
//! `pub(in crate::transports::dps)`, so this pin lives crate-internal here. Still REQUIRED-gated.

use crate::db::models::enums::{DocState, DocType, NodeMode, Severity};
use crate::services::write_path::error_routing::{
    route_dps_error, route_send_result, AuditEvent, MacRecoveryHint, ProbeHint, ProbeReason,
    RetryClass, RoutingDecision, WireDecision,
};
use crate::transports::dps::dto::observe_check_reply;
use crate::transports::dps::error::DpsError;
use crate::transports::dps::gen::CheckResponse;
use prro_domain::delivery::GrpcStatusDigest;

/// A `CheckResponse` with the given status/id (mirrors the `grpc.rs` §4.6 `chk` helper).
fn chk(status: i32, id: &str) -> CheckResponse {
    CheckResponse {
        id: id.to_string(),
        status,
        id_sign: Vec::new(),
        data_sign: Vec::new(),
        error_message: String::new(),
    }
}

/// Drive the REAL decode + routing for a `(status, id, doc_type)` wire reply.
fn wire_decision(status: i32, id: &str, doc_type: DocType) -> WireDecision {
    let (legacy, _raw, _diag) = observe_check_reply(chk(status, id));
    route_send_result(legacy, doc_type, true)
}

/// The `RoutingDecision` for a routed (Err) wire reply; panics if the reply routed to `Sent`.
fn routed(status: i32, doc_type: DocType) -> RoutingDecision {
    match wire_decision(status, "", doc_type) {
        WireDecision::Routed(rd) => rd,
        other => panic!("status {status} ({doc_type:?}) expected Routed, got {other:?}"),
    }
}

// ── normative RoutingDecision constructors (the locked incumbent shapes) ──────────────────

fn terminal_reject(severity: Severity) -> RoutingDecision {
    RoutingDecision {
        target_state: DocState::Rejected,
        retry_class: RetryClass::TerminalReject,
        audit_event: AuditEvent::StageSendRejected,
        audit_severity: severity,
        node_mode_flip: None,
        probe_hint: None,
        mac_recovery_hint: None,
    }
}

fn transient_retry() -> RoutingDecision {
    RoutingDecision {
        target_state: DocState::ErrorRetryable,
        retry_class: RetryClass::TransientRetry,
        audit_event: AuditEvent::StageSendTransientRetry,
        audit_severity: Severity::Warning,
        node_mode_flip: None,
        probe_hint: None,
        mac_recovery_hint: None,
    }
}

/// A `Decode`-sourced ProbeRequired (`status==0` MissingStatus AND every outside-proto-enum code):
/// the rev10-scoped audit is `StageSendDecodeUnknown`, NOT `StageSendProbeRequired`.
fn decode_probe_required() -> RoutingDecision {
    RoutingDecision {
        target_state: DocState::ErrorRetryable,
        retry_class: RetryClass::ProbeRequired,
        audit_event: AuditEvent::StageSendDecodeUnknown,
        audit_severity: Severity::Warning,
        node_mode_flip: None,
        probe_hint: Some(ProbeHint {
            reason: ProbeReason::DecodeUnknown,
        }),
        mac_recovery_hint: None,
    }
}

fn close_probe_required(reason: ProbeReason) -> RoutingDecision {
    RoutingDecision {
        target_state: DocState::ErrorRetryable,
        retry_class: RetryClass::ProbeRequired,
        audit_event: AuditEvent::StageSendProbeRequired,
        audit_severity: Severity::Warning,
        node_mode_flip: None,
        probe_hint: Some(ProbeHint { reason }),
        mac_recovery_hint: None,
    }
}

const SELL: DocType = DocType::Sell;
const Z: DocType = DocType::ZReport;

/// The full-tuple normative lock over every routed wire outcome. Any drift in ANY field of ANY row
/// REDs — that is the "4th delta / unchanged-row" gate at the routing layer (§4.1).
#[test]
fn s7_applyplan_routing_projection_is_locked() {
    // ── verdict rejects (Authorization / Server code arms) ──
    assert_eq!(
        routed(-1, SELL),
        terminal_reject(Severity::Error),
        "-1 Verify (Authorization)"
    );
    assert_eq!(
        routed(-5, SELL),
        terminal_reject(Severity::Critical),
        "-5 Type"
    );
    assert_eq!(
        routed(-7, SELL),
        terminal_reject(Severity::Critical),
        "-7 Xml"
    );
    assert_eq!(
        routed(-8, SELL),
        terminal_reject(Severity::Critical),
        "-8 XmlDate"
    );
    assert_eq!(
        routed(-9, SELL),
        terminal_reject(Severity::Critical),
        "-9 XmlChk"
    );
    assert_eq!(
        routed(-10, SELL),
        terminal_reject(Severity::Critical),
        "-10 XmlZReport"
    );
    assert_eq!(
        routed(-16, SELL),
        terminal_reject(Severity::Critical),
        "-16 OfflineId"
    );

    // ── FN-config (Authorization::FiscalNumberNotRegistered) ──
    let fn_config = RoutingDecision {
        target_state: DocState::ErrorRetryable,
        retry_class: RetryClass::FnConfigError,
        audit_event: AuditEvent::StageSendFnNotRegistered,
        audit_severity: Severity::Error,
        node_mode_flip: None,
        probe_hint: None,
        mac_recovery_hint: None,
    };
    assert_eq!(routed(-13, SELL), fn_config, "-13 NotRegisteredRro");
    assert_eq!(routed(-14, SELL), fn_config, "-14 NotRegisteredSigner");

    // ── operator / node-blocked / mac-recovery ──
    assert_eq!(
        routed(-6, SELL),
        RoutingDecision {
            target_state: DocState::ErrorRetryable,
            retry_class: RetryClass::OperatorEscalation,
            audit_event: AuditEvent::StageSendOperatorEscalation,
            audit_severity: Severity::Error,
            node_mode_flip: None,
            probe_hint: None,
            mac_recovery_hint: None,
        },
        "-6 NotPrevZReport (operator)"
    );
    assert_eq!(
        routed(-11, SELL),
        RoutingDecision {
            target_state: DocState::Rejected,
            retry_class: RetryClass::TerminalReject,
            audit_event: AuditEvent::StageSendNodeBlocked,
            audit_severity: Severity::Critical,
            node_mode_flip: Some(NodeMode::Blocked),
            probe_hint: None,
            mac_recovery_hint: None,
        },
        "-11 Offline168 (node BLOCKED)"
    );
    assert_eq!(
        routed(-12, SELL),
        RoutingDecision {
            target_state: DocState::ErrorRetryable,
            retry_class: RetryClass::MacRecovery,
            audit_event: AuditEvent::StageSendMacHashMismatch,
            audit_severity: Severity::Warning,
            node_mode_flip: None,
            probe_hint: None,
            mac_recovery_hint: Some(MacRecoveryHint {
                raw_error_message: String::new(),
            }),
        },
        "-12 BadHashPrev (mac recovery)"
    );

    // ── transient (SaveError) + indeterminate (-4) ──
    assert_eq!(routed(-3, SELL), transient_retry(), "-3 SaveError");
    assert_eq!(
        routed(-4, SELL),
        transient_retry(),
        "-4 ErrorUnknown (Indeterminate) — NOT a delta"
    );

    // ── close/non-close split (-2 / -15) ──
    assert_eq!(
        routed(-2, SELL),
        terminal_reject(Severity::Critical),
        "-2 non-close"
    );
    assert_eq!(
        routed(-15, SELL),
        terminal_reject(Severity::Critical),
        "-15 non-close"
    );
    assert_eq!(
        routed(-2, Z),
        close_probe_required(ProbeReason::Code2CloseShift),
        "-2 on close/Z"
    );
    assert_eq!(
        routed(-15, Z),
        close_probe_required(ProbeReason::Code15CloseShift),
        "-15 on close/Z"
    );

    // ── Decode-sourced ProbeRequired: status==0 (MissingStatus) AND outside-enum (-17) route
    //    IDENTICALLY (both DpsError::Decode). rev10: audit is StageSendDecodeUnknown, NOT ProbeRequired.
    assert_eq!(
        routed(0, SELL),
        decode_probe_required(),
        "0 MissingStatus (Decode)"
    );
    assert_eq!(
        routed(-17, SELL),
        decode_probe_required(),
        "-17 outside-proto-enum (Decode)"
    );

    // ── transport + TLS RemoteStatus (constructed DpsError; deterministic route) ──
    assert_eq!(
        route_dps_error(&DpsError::Transport("timeout".into()), SELL, true),
        transient_retry(),
        "Transport (NoResponse)"
    );
    assert_eq!(
        route_dps_error(
            &DpsError::RemoteStatus {
                code: "Unauthenticated".into(),
                message: "x".into(),
                digest: GrpcStatusDigest::from_transport_digest([0xAB; 32]),
            },
            SELL,
            true,
        ),
        transient_retry(),
        "TLS RemoteStatus incumbent (compat projection = TransientRetry)"
    );
}

/// rev10 tooth (architect-required): the `MissingStatus` (`status==0`, `Decode`) audit is
/// `StageSendDecodeUnknown` — NEVER `StageSendProbeRequired`. Swapping the locked value in
/// `s7_applyplan_routing_projection_is_locked` to `StageSendProbeRequired` MUST RED, because the
/// live incumbent is `StageSendDecodeUnknown`. This canary proves that lock is load-bearing.
#[test]
fn s7_applyplan_missing_status_audit_is_decode_unknown_not_probe_required() {
    let rd = routed(0, SELL);
    assert_eq!(
        rd.audit_event,
        AuditEvent::StageSendDecodeUnknown,
        "MissingStatus audit is the Decode-sourced event (rev10 scope)"
    );
    assert_ne!(
        rd.audit_event,
        AuditEvent::StageSendProbeRequired,
        "MissingStatus audit must NOT be the CloseAmbiguous/target-delta ProbeRequired event \
         (rev10 over-lock correction) — locking it to StageSendProbeRequired would RED the graph pin"
    );
}

/// The two Ok arms: `Accepted` (non-empty id → `Sent{id}`) and the empty-id case
/// (`Sent{""}` — the `OkButNoFiscalNumber` guard/held is a stage_send-level concern, not route).
#[test]
fn s7_applyplan_ok_arms_route_to_sent() {
    match wire_decision(1, "4000123456", SELL) {
        WireDecision::Sent { server_fiscal_no } => {
            assert_eq!(
                server_fiscal_no, "4000123456",
                "Accepted forwards the fiscal id"
            )
        }
        other => panic!("Accepted expected Sent, got {other:?}"),
    }
    match wire_decision(1, "", SELL) {
        WireDecision::Sent { server_fiscal_no } => assert!(
            server_fiscal_no.is_empty(),
            "empty-id routes to Sent{{empty}} at the route layer (guard is stage_send-level)"
        ),
        other => panic!("empty-id expected Sent, got {other:?}"),
    }
}
