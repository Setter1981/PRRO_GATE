//! CS-3 3.2 PR2 — the total, transport-minted send observation (spec §4.2).
//!
//! [`RawSendReply`] is the total evidence of ONE returned wire observation, minted only by the DPS
//! decoder. It is an **opaque struct over a private inner enum**: because the field is private and
//! `RawSendReply` lives in `crate::transports::dps`, a sibling module (the engine, `crate::services`)
//! **cannot construct it** — this is compile-time Rust module privacy, not a source-gate (contrast
//! the cross-crate digest/provenance mint, which Rust cannot sibling-seal). The engine READS it via
//! [`kind`](RawSendReply::kind); the PR4 engine mapper turns it into a `SendResponse`, carrying the
//! transport-minted digest/id it reads out — never fabricating one.
//!
//! [`WireDiagnostics`] is a NON-authority forensic sidecar (spec §4.2, blocker B4): it preserves the
//! status code / gRPC code / message that the authoritative evidence drops, for trace + the live
//! `-12` MAC hint, WITHOUT being part of the delivery contract.

// Wired into `send_chk_observed` (the single-RPC fan-out) by PR2 pin 3; until then the type is
// exercised only by the in-module tests, so the non-test build sees it as unused.
#![allow(dead_code)]

use prro_domain::delivery::{
    BoundedText, DecodedResponseDigest, GrpcStatusDigest, NoResponseCause, NonEmptyFiscalNumber,
    NonOkStatusCode,
};

/// Total evidence of one returned wire observation — opaque, transport-minted (module-sealed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawSendReply(RawSendReplyInner);

/// Private inner algebra — a sibling module cannot name or construct these variants.
#[derive(Clone, Debug, PartialEq, Eq)]
enum RawSendReplyInner {
    /// Status OK + a non-empty server fiscal id. No digest (the id IS the transport-proven evidence).
    Accepted { fiscal_id: NonEmptyFiscalNumber },
    /// Status OK but an empty id — cannot prove acceptance.
    OkNoFiscalId { digest: DecodedResponseDigest },
    /// A non-OK, non-zero DPS status code (the `-1..-16` verdicts + any unknown non-zero code).
    ServerCode {
        code: NonOkStatusCode,
        digest: DecodedResponseDigest,
    },
    /// `status == 0` (proto default / missing status) — a decode-indeterminate.
    MissingStatus { digest: DecodedResponseDigest },
    /// TLS-proven `Unauthenticated`/`PermissionDenied` — an authenticated peer, non-DPS body.
    RemoteAuthStatus { grpc: GrpcStatusDigest },
    /// No trusted DPS reply (genuine absence, or the branch-9 catch-all).
    NoResponse { cause: NoResponseCause },
}

impl RawSendReply {
    // ── Transport-only builders: `pub(in crate::transports::dps)` so the DPS decoder submodules
    //    (grpc.rs/dto.rs) mint them, but the engine (crate::services) cannot. ──

    pub(in crate::transports::dps) fn accepted(fiscal_id: NonEmptyFiscalNumber) -> Self {
        Self(RawSendReplyInner::Accepted { fiscal_id })
    }
    pub(in crate::transports::dps) fn ok_no_fiscal_id(digest: DecodedResponseDigest) -> Self {
        Self(RawSendReplyInner::OkNoFiscalId { digest })
    }
    pub(in crate::transports::dps) fn server_code(
        code: NonOkStatusCode,
        digest: DecodedResponseDigest,
    ) -> Self {
        Self(RawSendReplyInner::ServerCode { code, digest })
    }
    pub(in crate::transports::dps) fn missing_status(digest: DecodedResponseDigest) -> Self {
        Self(RawSendReplyInner::MissingStatus { digest })
    }
    pub(in crate::transports::dps) fn remote_auth_status(grpc: GrpcStatusDigest) -> Self {
        Self(RawSendReplyInner::RemoteAuthStatus { grpc })
    }
    pub(in crate::transports::dps) fn no_response(cause: NoResponseCause) -> Self {
        Self(RawSendReplyInner::NoResponse { cause })
    }

    /// Borrowed view for the engine mapper — read (match) but never construct.
    pub fn kind(&self) -> RawSendReplyKind<'_> {
        match &self.0 {
            RawSendReplyInner::Accepted { fiscal_id } => RawSendReplyKind::Accepted { fiscal_id },
            RawSendReplyInner::OkNoFiscalId { digest } => RawSendReplyKind::OkNoFiscalId { digest },
            RawSendReplyInner::ServerCode { code, digest } => RawSendReplyKind::ServerCode {
                code: *code,
                digest,
            },
            RawSendReplyInner::MissingStatus { digest } => {
                RawSendReplyKind::MissingStatus { digest }
            }
            RawSendReplyInner::RemoteAuthStatus { grpc } => {
                RawSendReplyKind::RemoteAuthStatus { grpc }
            }
            RawSendReplyInner::NoResponse { cause } => {
                RawSendReplyKind::NoResponse { cause: *cause }
            }
        }
    }
}

/// Borrowed, read-only view of a [`RawSendReply`] (the engine mapper matches on this).
#[derive(Debug)]
pub enum RawSendReplyKind<'a> {
    Accepted {
        fiscal_id: &'a NonEmptyFiscalNumber,
    },
    OkNoFiscalId {
        digest: &'a DecodedResponseDigest,
    },
    ServerCode {
        code: NonOkStatusCode,
        digest: &'a DecodedResponseDigest,
    },
    MissingStatus {
        digest: &'a DecodedResponseDigest,
    },
    RemoteAuthStatus {
        grpc: &'a GrpcStatusDigest,
    },
    NoResponse {
        cause: NoResponseCause,
    },
}

/// Non-authority forensic sidecar (spec §4.2 / blocker B4). Preserves what the authoritative
/// [`RawSendReply`] drops — for `transport_trace`, the live `-12` MAC hint, audit — WITHOUT being
/// part of the delivery contract. NOT provenance: any code may build one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WireDiagnostics {
    pub status_code: Option<i32>,
    pub grpc_code: Option<String>,
    pub message: Option<BoundedText>,
}

/// The transport's dual output for the shadow path: the authoritative evidence + the diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawSendObservation {
    evidence: RawSendReply,
    diagnostics: WireDiagnostics,
}

impl RawSendObservation {
    pub(in crate::transports::dps) fn new(
        evidence: RawSendReply,
        diagnostics: WireDiagnostics,
    ) -> Self {
        Self {
            evidence,
            diagnostics,
        }
    }
    /// The authoritative, transport-minted evidence.
    pub fn evidence(&self) -> &RawSendReply {
        &self.evidence
    }
    /// The non-authority forensic sidecar.
    pub fn diagnostics(&self) -> &WireDiagnostics {
        &self.diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dg() -> DecodedResponseDigest {
        DecodedResponseDigest::from_transport_digest([0xAB; 32])
    }
    fn grpc() -> GrpcStatusDigest {
        GrpcStatusDigest::from_transport_digest([0xCD; 32])
    }

    #[test]
    fn every_variant_round_trips_through_kind() {
        let id = NonEmptyFiscalNumber::from_transport("DPS-1".into()).unwrap();
        let accepted = RawSendReply::accepted(id.clone());
        assert!(
            matches!(accepted.kind(), RawSendReplyKind::Accepted { fiscal_id } if fiscal_id == &id)
        );

        let ok_empty = RawSendReply::ok_no_fiscal_id(dg());
        assert!(
            matches!(ok_empty.kind(), RawSendReplyKind::OkNoFiscalId { digest } if digest == &dg())
        );

        let code = NonOkStatusCode::from_transport(-11).unwrap();
        let server = RawSendReply::server_code(code, dg());
        assert!(matches!(server.kind(), RawSendReplyKind::ServerCode { code: c, .. } if c == code));

        let missing = RawSendReply::missing_status(dg());
        assert!(matches!(
            missing.kind(),
            RawSendReplyKind::MissingStatus { .. }
        ));

        let remote = RawSendReply::remote_auth_status(grpc());
        assert!(
            matches!(remote.kind(), RawSendReplyKind::RemoteAuthStatus { grpc: g } if g == &grpc())
        );

        let none = RawSendReply::no_response(NoResponseCause::Timeout);
        assert!(
            matches!(none.kind(), RawSendReplyKind::NoResponse { cause } if cause == NoResponseCause::Timeout)
        );
    }

    #[test]
    fn observation_carries_evidence_and_diagnostics() {
        let obs = RawSendObservation::new(
            RawSendReply::no_response(NoResponseCause::CallFailedWithoutTrustedDpsEnvelope),
            WireDiagnostics {
                status_code: None,
                grpc_code: Some("Unavailable".into()),
                message: None,
            },
        );
        assert!(matches!(
            obs.evidence().kind(),
            RawSendReplyKind::NoResponse { .. }
        ));
        assert_eq!(obs.diagnostics().grpc_code.as_deref(), Some("Unavailable"));
    }
}
