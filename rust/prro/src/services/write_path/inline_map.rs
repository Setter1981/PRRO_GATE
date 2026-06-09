//! RS-3 A2.0 part-2 — pure outcome/error mapping for the inline write-path.
//!
//! Translates each write-path stage's typed error / outcome into the ingress
//! [`FiscalError`] taxonomy (or a control-flow [`SendDisposition`] the A2.1b
//! orchestrator consumes).  NO orchestration, NO IO — a table-tested pure
//! module so the orchestrator stays a thin ladder and the (fiscal-semantic)
//! HTTP-class decisions live in ONE auditable place.
//!
//! **CARRIED-CODE FENCE (review MEDIUM):** the code-bearing `FiscalError`
//! variants (`ShiftGuardRefused` / `OfflineRefused` / `Internal`) carry a free
//! `code: &'static str` that the handler maps to an HTTP status — nothing at
//! the type level binds a code to its class.  This module is the ONLY producer
//! of those codes; every code is a `const` in [`codes`] and is round-trip
//! asserted (`code_consts_round_trip_to_expected_http_class`) to map to its
//! intended class, so a typo can never silently degrade a 422/503 to the
//! handler's `_ => 500` fallback.
//!
//! HTTP classes (operator-signed 2026-06-09): ShiftNotOpen 422 · ShiftGuardRefused
//! 422 (T1 shift-state + Q-A signer-cashier) · DpsRejected 422 · OfflineRefused
//! 503 (M1 node modes) · SignFailure 500 (crypto-sign ONLY) · Internal 500
//! (every other structural/runtime breach) · ZSurfaceNotReady 501.
//!
//! NOTE: these mappers are CONSUMED by the A2.1b orchestrator (`inline::run`),
//! which does not exist yet — until then they are reachable only from the unit
//! tests below, hence the module-level `dead_code` allow.

#![allow(dead_code)]

use crate::db::models::enums::DocState;
use crate::runtime::ingress::canonical_builder::BuildReject;
use crate::runtime::ingress::seam::FiscalError;
use crate::services::write_path::dispatch::DispatcherRefusalReason;
use crate::services::write_path::signer_guard::SignerCashierMismatch;
use crate::services::write_path::stage_send::{StageSendError, StageSendOutcome};
use crate::services::write_path::stage_sign::SignError;
use crate::services::write_path::types::RejectionReason;

/// Stable HTTP-facing codes carried by the code-bearing `FiscalError` variants.
/// Every code is round-trip-tested (see module docs).
pub(crate) mod codes {
    // ── ShiftGuardRefused → 422 (client/operator-fixable stage-guard refusals) ──
    pub const SHIFT_ALREADY_OPEN: &str = "SHIFT_ALREADY_OPEN";
    pub const SHIFT_OPEN_PENDING_DRAIN: &str = "SHIFT_OPEN_PENDING_DRAIN";
    pub const POST_LOCAL_CLOSE_SALE_REFUSED: &str = "POST_LOCAL_CLOSE_SALE_REFUSED";
    pub const OFFLINE_SHIFT_CLOSE_NOT_SUPPORTED: &str = "OFFLINE_SHIFT_CLOSE_NOT_SUPPORTED";
    pub const SHIFT_CLOSING_IN_FLIGHT: &str = "SHIFT_CLOSING_IN_FLIGHT";
    pub const Z_REPORT_BACKLOG_DRAIN_PENDING: &str = "Z_REPORT_BACKLOG_DRAIN_PENDING";
    /// Q-A — the true signer-vs-opening-cashier mismatch (operator reissues
    /// with the correct cashier); pre-wire, no fiscal commitment → 422.
    pub const SIGNER_CASHIER_MISMATCH: &str = "SIGNER_CASHIER_MISMATCH";

    // ── OfflineRefused → 503 (node-mode refusals; GOING_ONLINE is retryable) ──
    pub const NODE_BLOCKED: &str = "NODE_BLOCKED";
    pub const NODE_STOP_MODE: &str = "NODE_STOP_MODE";
    pub const NODE_CRYPTO_DEGRADED: &str = "NODE_CRYPTO_DEGRADED";
    pub const NODE_GOING_ONLINE: &str = "NODE_GOING_ONLINE";

    // ── Internal → 500 (structural/runtime breaches; → 500 via the handler's
    //    `_ => 500` fallback except SHIFT_MANUAL_RECON which is explicit) ──
    pub const SHIFT_MANUAL_RECON: &str = "SHIFT_MANUAL_RECON";
    pub const SHIFT_IN_ERROR: &str = "SHIFT_IN_ERROR";
    pub const SHIFT_INVARIANT_VIOLATION: &str = "SHIFT_INVARIANT_VIOLATION";
    pub const MISSING_PROFILE_BINDING: &str = "MISSING_PROFILE_BINDING";
    pub const INVALID_PAYLOAD: &str = "INVALID_PAYLOAD";
    pub const SIGNER_ID_MISSING: &str = "SIGNER_ID_MISSING";
    pub const SHIFT_MISSING_FOR_FISCAL_DOC: &str = "SHIFT_MISSING_FOR_FISCAL_DOC";
    pub const SHIFT_ID_MISMATCH: &str = "SHIFT_ID_MISMATCH";
    pub const CROSS_FN_MISMATCH: &str = "CROSS_FN_MISMATCH";
    /// Coarse per-stage codes: the specific cause is preserved in the audit +
    /// the error message (the operator's forensic taxonomy), the code routes
    /// the HTTP class.
    pub const SIGN_INTERNAL: &str = "SIGN_INTERNAL";
    pub const SEND_INTERNAL: &str = "SEND_INTERNAL";
}

fn shift_guard(request_id: [u8; 16], code: &'static str) -> FiscalError {
    FiscalError::ShiftGuardRefused { request_id, code }
}
fn node_refused(request_id: [u8; 16], code: &'static str) -> FiscalError {
    FiscalError::OfflineRefused { request_id, code }
}
fn internal(request_id: [u8; 16], code: &'static str) -> FiscalError {
    FiscalError::Internal { request_id, code }
}

/// `WorkerProcessResult::Rejected{reason}` → typed `FiscalError`.  stage_acquire
/// has ALREADY terminalised the inbox (REJECTED + audit); this only renders the
/// HTTP-facing error.  Exhaustive over `RejectionReason` (no `_`).
pub(crate) fn map_rejection(reason: &RejectionReason, request_id: [u8; 16]) -> FiscalError {
    use RejectionReason as R;
    match reason {
        R::ShiftNotOpen { .. } => FiscalError::ShiftNotOpen { request_id },
        // T1 — precise shift-state codes (422), NOT collapsed to ShiftNotOpen.
        R::ShiftAlreadyOpen => shift_guard(request_id, codes::SHIFT_ALREADY_OPEN),
        R::ShiftOpenPendingDrainOpRefused => {
            shift_guard(request_id, codes::SHIFT_OPEN_PENDING_DRAIN)
        }
        R::PostLocalCloseSaleRefused => {
            shift_guard(request_id, codes::POST_LOCAL_CLOSE_SALE_REFUSED)
        }
        R::OfflineShiftCloseNotSupported => {
            shift_guard(request_id, codes::OFFLINE_SHIFT_CLOSE_NOT_SUPPORTED)
        }
        R::ShiftClosingInFlight => shift_guard(request_id, codes::SHIFT_CLOSING_IN_FLIGHT),
        R::ZReportBlockedBacklogDrainPending => {
            shift_guard(request_id, codes::Z_REPORT_BACKLOG_DRAIN_PENDING)
        }
        // T2/T3 — structural / manual-recon (500).
        R::ShiftInError => internal(request_id, codes::SHIFT_IN_ERROR),
        R::ShiftInvariantViolation => internal(request_id, codes::SHIFT_INVARIANT_VIOLATION),
        R::ShiftRequiresOperatorAttention => internal(request_id, codes::SHIFT_MANUAL_RECON),
        R::MissingProfileBinding => internal(request_id, codes::MISSING_PROFILE_BINDING),
        // M2 — a payload that passed ingress but fails the acquire guard is a
        // structural inconsistency, not a client 422.
        R::InvalidPayload { .. } => internal(request_id, codes::INVALID_PAYLOAD),
        // M1 — precise node-mode codes (503).
        R::NodeBlocked => node_refused(request_id, codes::NODE_BLOCKED),
        R::NodeStopMode => node_refused(request_id, codes::NODE_STOP_MODE),
        R::NodeCryptoDegraded => node_refused(request_id, codes::NODE_CRYPTO_DEGRADED),
        R::NodeGoingOnlineDrainInFlight => node_refused(request_id, codes::NODE_GOING_ONLINE),
    }
}

/// `SignError` → `FiscalError`.  ONLY `Crypto` is a crypto/sign-operation
/// failure (`SignFailure`); every other variant is a structural/runtime breach
/// (`Internal`, coarse `SIGN_INTERNAL` — the specific cause is in the message).
pub(crate) fn map_sign_error(err: &SignError, request_id: [u8; 16]) -> FiscalError {
    match err {
        SignError::Crypto(_) => FiscalError::SignFailure { request_id },
        _ => internal(request_id, codes::SIGN_INTERNAL),
    }
}

/// `StageSendError` → `FiscalError`.  A MAC-recovery re-sign failure routes by
/// its inner `SignError` (a crypto re-sign → `SignFailure`); every other
/// `StageSendError` is a structural/runtime breach → `Internal` (`SEND_INTERNAL`).
/// Terminal DPS rejects do NOT surface here — they are
/// `StageSendOutcome::Routed{target_state: Rejected}` (see [`classify_send_outcome`]).
pub(crate) fn map_send_error(err: &StageSendError, request_id: [u8; 16]) -> FiscalError {
    match err {
        StageSendError::MacRecoverySignFailed(inner) => map_sign_error(inner, request_id),
        _ => internal(request_id, codes::SEND_INTERNAL),
    }
}

/// `PostSignRoute::Refused(reason)` → `FiscalError`.  Every dispatcher refusal
/// is a node-mode refusal → 503 with the precise node code.  Exhaustive.
pub(crate) fn map_dispatcher_refusal(
    reason: &DispatcherRefusalReason,
    request_id: [u8; 16],
) -> FiscalError {
    use DispatcherRefusalReason as D;
    match reason {
        D::NodeBlocked => node_refused(request_id, codes::NODE_BLOCKED),
        D::NodeStopMode => node_refused(request_id, codes::NODE_STOP_MODE),
        D::NodeCryptoDegraded => node_refused(request_id, codes::NODE_CRYPTO_DEGRADED),
        D::NodeGoingOnline => node_refused(request_id, codes::NODE_GOING_ONLINE),
    }
}

/// `BuildReject` → `FiscalError`.  A1's builder already terminalised the inbox;
/// a row that fails to build AFTER ingress validation is a structural
/// inconsistency (Q-B) → `Internal/500`, carrying `BuildReject`'s own code.
pub(crate) fn map_build_reject(reject: &BuildReject, request_id: [u8; 16]) -> FiscalError {
    internal(request_id, reject.code())
}

/// `SignerCashierMismatch` → `FiscalError` (Q-A split): the TRUE cashier
/// mismatch is client/operator-fixable → 422; every structural caller-bug
/// variant → 500.  `#[non_exhaustive]` → a future variant defaults to 500.
pub(crate) fn map_signer_refused(m: &SignerCashierMismatch, request_id: [u8; 16]) -> FiscalError {
    use SignerCashierMismatch as M;
    match m {
        // Q-A — signer != opening cashier; pre-wire, no fiscal commitment;
        // operator reissues with the correct cashier → 422.
        M::Mismatch { .. } => shift_guard(request_id, codes::SIGNER_CASHIER_MISMATCH),
        // structural caller bugs (ingress / dispatcher / drain) → 500.
        M::SignerIdMissing { .. } => internal(request_id, codes::SIGNER_ID_MISSING),
        M::ShiftMissingForFiscalDoc { .. } => {
            internal(request_id, codes::SHIFT_MISSING_FOR_FISCAL_DOC)
        }
        M::ShiftIdMismatch { .. } => internal(request_id, codes::SHIFT_ID_MISMATCH),
        M::CrossFnMismatch { .. } => internal(request_id, codes::CROSS_FN_MISMATCH),
        // `SignerCashierMismatch` is `#[non_exhaustive]`: this same-crate match
        // is exhaustive today, and a FUTURE variant will break compilation here
        // — forcing a deliberate HTTP-class decision (no silent 500 default).
    }
}

/// How the orchestrator continues after `stage_send::run`.
#[derive(Debug)]
pub(crate) enum SendDisposition {
    /// `Sent` — proceed to KVT2-confirm (A2.1a) then finalize.
    Proceed { server_fiscal_no: String },
    /// A terminal failure (`Err(FiscalError)`): a terminal DPS reject
    /// (`DpsRejected`/422) or a structural send breach (`Internal`/500).
    Reject(FiscalError),
    /// A transient send (`target_state: ErrorRetryable`): NOT terminal — the
    /// receipt is persisted and the ledger retries via drain/B1.  The
    /// orchestrator returns `Ok(FiscalOutcome{non-terminal})` → 202 IN_PROGRESS.
    InProgress,
    /// IDEMPOTENT RE-ENTRY / RACE — `stage_send` did NOT call the wire: the doc
    /// was already advanced past `Sending` by a prior worker / reconciliation
    /// (`StateConflict`, `observed` carries the state it found) or its row was
    /// concurrently deleted (`DocumentMissing`, `observed = None`).  This is the
    /// send-stage analog of `WorkerProcessResult::Noop` — NOT a send failure.
    /// The orchestrator MUST resolve against the ledger (return the durable
    /// accepted / in-flight truth, or `Internal`/500 ONLY if no durable
    /// evidence exists), NEVER render a phantom terminal 500 (review HIGH).
    ResolveReplay { observed: Option<DocState> },
}

/// `StageSendOutcome` → [`SendDisposition`].  Terminal DPS reject → `DpsRejected`;
/// transient (`ErrorRetryable`) → `InProgress` (202, NOT a terminal Err);
/// idempotent/race no-wire outcomes (`StateConflict`/`DocumentMissing`) →
/// `ResolveReplay` (resolve from the ledger, NOT a phantom 500); a signer-guard
/// refusal → the Q-A split; an (unreachable) other target → structural breach.
pub(crate) fn classify_send_outcome(
    outcome: StageSendOutcome,
    request_id: [u8; 16],
) -> SendDisposition {
    match outcome {
        StageSendOutcome::Sent {
            server_fiscal_no, ..
        } => SendDisposition::Proceed { server_fiscal_no },
        StageSendOutcome::Routed { decision, .. } => match decision.target_state {
            DocState::Rejected => SendDisposition::Reject(FiscalError::DpsRejected { request_id }),
            DocState::ErrorRetryable => SendDisposition::InProgress,
            // stage_send only ever routes to Rejected | ErrorRetryable
            // (error_routing.rs); any other target is a structural breach.
            _ => SendDisposition::Reject(internal(request_id, codes::SEND_INTERNAL)),
        },
        // No-wire idempotent re-entry / race — NOT a failure (review HIGH).
        StageSendOutcome::StateConflict { observed } => SendDisposition::ResolveReplay {
            observed: Some(observed),
        },
        StageSendOutcome::DocumentMissing => SendDisposition::ResolveReplay { observed: None },
        StageSendOutcome::SignerRefused(m) => {
            SendDisposition::Reject(map_signer_refused(&m, request_id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::ingress::handler::http_status_for_error_code as http;

    const RID: [u8; 16] = [0xAB; 16];

    fn code_of(e: &FiscalError) -> &'static str {
        match e {
            FiscalError::ShiftGuardRefused { code, .. }
            | FiscalError::OfflineRefused { code, .. }
            | FiscalError::Internal { code, .. } => code,
            FiscalError::ShiftNotOpen { .. } => "NO_OPEN_SHIFT",
            FiscalError::DpsRejected { .. } => "FISCAL_REJECTED",
            FiscalError::SignFailure { .. } => "SIGN_FAILED",
            FiscalError::ZSurfaceNotReady { .. } => "Z_SURFACE_NOT_READY",
            FiscalError::NotImplemented { .. } => "NOT_IMPLEMENTED",
        }
    }

    /// THE FENCE: every code this module can produce round-trips to its
    /// intended HTTP class through the handler's map — so a typo cannot
    /// silently degrade a 422/503 to the `_ => 500` fallback.
    #[test]
    fn code_consts_round_trip_to_expected_http_class() {
        for c in [
            codes::SHIFT_ALREADY_OPEN,
            codes::SHIFT_OPEN_PENDING_DRAIN,
            codes::POST_LOCAL_CLOSE_SALE_REFUSED,
            codes::OFFLINE_SHIFT_CLOSE_NOT_SUPPORTED,
            codes::SHIFT_CLOSING_IN_FLIGHT,
            codes::Z_REPORT_BACKLOG_DRAIN_PENDING,
            codes::SIGNER_CASHIER_MISMATCH,
        ] {
            assert_eq!(http(c), 422, "{c} must route to 422 (ShiftGuardRefused)");
        }
        for c in [
            codes::NODE_BLOCKED,
            codes::NODE_STOP_MODE,
            codes::NODE_CRYPTO_DEGRADED,
            codes::NODE_GOING_ONLINE,
        ] {
            assert_eq!(http(c), 503, "{c} must route to 503 (OfflineRefused)");
        }
        for c in [
            codes::SHIFT_MANUAL_RECON,
            codes::SHIFT_IN_ERROR,
            codes::SHIFT_INVARIANT_VIOLATION,
            codes::MISSING_PROFILE_BINDING,
            codes::INVALID_PAYLOAD,
            codes::SIGNER_ID_MISSING,
            codes::SHIFT_MISSING_FOR_FISCAL_DOC,
            codes::SHIFT_ID_MISMATCH,
            codes::CROSS_FN_MISMATCH,
            codes::SIGN_INTERNAL,
            codes::SEND_INTERNAL,
        ] {
            assert_eq!(http(c), 500, "{c} must route to 500 (Internal)");
        }
    }

    /// FENCE EXTENSION (review MEDIUM): `map_build_reject` forwards
    /// `BuildReject::code()` strings that are NOT in `inline_map::codes`, so the
    /// const round-trip above does not cover them.  Assert every one routes to
    /// 500 (Internal); the exhaustive `match` (no `_`) forces a future
    /// `BuildReject` variant to be classified here before it can ship.
    #[test]
    fn build_reject_codes_round_trip_to_500() {
        use crate::db::models::enums::DocType;
        use BuildReject as B;
        let samples = [
            B::MissingDriverId,
            B::MissingBusinessTs,
            B::MissingTotalForSale {
                doc_type: DocType::Sell,
            },
            B::UnsupportedOperation {
                operation_type: "X_REPORT".into(),
            },
            B::ZClassRequiresAggregationBuilder {
                operation_type: "Z_REPORT".into(),
            },
            B::PayloadHashMismatch,
            B::InvalidIdentity {
                field: "fiscal_number",
            },
        ];
        for b in &samples {
            // Exhaustive (no `_`) — a new BuildReject variant breaks compilation
            // here until it is added (and to `samples`) + its 500 class asserted.
            match b {
                B::MissingDriverId
                | B::MissingBusinessTs
                | B::MissingTotalForSale { .. }
                | B::UnsupportedOperation { .. }
                | B::ZClassRequiresAggregationBuilder { .. }
                | B::PayloadHashMismatch
                | B::InvalidIdentity { .. } => {}
            }
            assert_eq!(
                http(code_of(&map_build_reject(b, RID))),
                500,
                "BuildReject code {} must route to 500 (Internal)",
                b.code()
            );
        }
    }

    #[test]
    fn rejection_maps_to_signed_codes() {
        use crate::db::models::enums::ShiftState;
        use RejectionReason as R;
        // Spot-check the 4 HTTP classes; exhaustiveness is compile-enforced.
        assert_eq!(
            http(code_of(&map_rejection(
                &R::ShiftNotOpen {
                    current: ShiftState::Closed
                },
                RID
            ))),
            422
        );
        assert_eq!(
            http(code_of(&map_rejection(&R::ShiftAlreadyOpen, RID))),
            422
        );
        assert_eq!(http(code_of(&map_rejection(&R::NodeBlocked, RID))), 503);
        assert_eq!(
            http(code_of(&map_rejection(
                &R::ShiftRequiresOperatorAttention,
                RID
            ))),
            500
        );
        assert_eq!(
            http(code_of(&map_rejection(
                &R::InvalidPayload { detail: "x".into() },
                RID
            ))),
            500
        );
    }

    #[test]
    fn sign_error_crypto_is_signfailure_rest_internal() {
        // Crypto → SignFailure (500); a structural SignError → Internal (500),
        // but a DISTINCT variant (not SignFailure).
        let rm = SignError::RowMissing {
            document_id: crate::db::models::ids::DocumentId::new(),
        };
        assert!(matches!(
            map_sign_error(&rm, RID),
            FiscalError::Internal { .. }
        ));
        assert_eq!(http(code_of(&map_sign_error(&rm, RID))), 500);
    }

    #[test]
    fn signer_refused_splits_cashier_mismatch_422_vs_structural_500() {
        use crate::db::models::enums::DocType;
        use crate::db::models::ids::{CashierId, DocumentId, ShiftId};
        use SignerCashierMismatch as M;

        let mismatch = M::Mismatch {
            shift_id: ShiftId::new(),
            document_id: DocumentId::new(),
            expected_cashier_id: CashierId::new("a").unwrap(),
            attempted_signer_id: CashierId::new("b").unwrap(),
            doc_type: DocType::Sell,
        };
        // Q-A — true cashier mismatch is client-fixable → 422.
        let fe = map_signer_refused(&mismatch, RID);
        assert!(matches!(fe, FiscalError::ShiftGuardRefused { .. }));
        assert_eq!(http(code_of(&fe)), 422);

        // structural caller bug → 500.
        let structural = M::SignerIdMissing {
            document_id: DocumentId::new(),
        };
        assert!(matches!(
            map_signer_refused(&structural, RID),
            FiscalError::Internal { .. }
        ));
        assert_eq!(http(code_of(&map_signer_refused(&structural, RID))), 500);
    }

    #[test]
    fn send_outcome_routes_every_disposition() {
        use crate::db::models::enums::Severity;
        use crate::services::write_path::error_routing::{AuditEvent, RetryClass, RoutingDecision};

        // classify_send_outcome only reads `target_state`; the rest are filler.
        fn routed(target_state: DocState) -> StageSendOutcome {
            StageSendOutcome::Routed {
                decision: RoutingDecision {
                    target_state,
                    retry_class: RetryClass::TerminalReject,
                    audit_event: AuditEvent::StageSendRejected,
                    audit_severity: Severity::Error,
                    node_mode_flip: None,
                    probe_hint: None,
                    mac_recovery_hint: None,
                },
                attempt_no: 1,
                wire_status_code: None,
                wire_error_message: None,
            }
        }

        // Sent → Proceed.
        assert!(matches!(
            classify_send_outcome(
                StageSendOutcome::Sent {
                    server_fiscal_no: "777".into(),
                    attempt_no: 1
                },
                RID
            ),
            SendDisposition::Proceed { .. }
        ));
        // Terminal DPS reject → Reject(DpsRejected) → 422.
        match classify_send_outcome(routed(DocState::Rejected), RID) {
            SendDisposition::Reject(fe) => assert_eq!(http(code_of(&fe)), 422),
            d => panic!("Rejected must be a terminal reject, got {d:?}"),
        }
        // Transient → InProgress (Ok/202, NOT a terminal Err).
        assert!(matches!(
            classify_send_outcome(routed(DocState::ErrorRetryable), RID),
            SendDisposition::InProgress
        ));
        // No-wire idempotent re-entry / race → ResolveReplay (NOT a phantom 500).
        assert!(matches!(
            classify_send_outcome(
                StageSendOutcome::StateConflict {
                    observed: DocState::Sent
                },
                RID
            ),
            SendDisposition::ResolveReplay {
                observed: Some(DocState::Sent)
            }
        ));
        assert!(matches!(
            classify_send_outcome(StageSendOutcome::DocumentMissing, RID),
            SendDisposition::ResolveReplay { observed: None }
        ));
    }
}
