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

// PR2 pin3 wires the mint (dto.rs/grpc.rs) + the degraded default (`observe_from_legacy`) + the
// stage_send fan-out. The READ accessors (`kind` / `RawSendReplyKind` / `evidence` / `diagnostics`)
// are consumed by the PR4 engine mapper; until then only the in-module + smoke tests read them, so
// the non-test build still sees them as unused.
#![allow(dead_code)]

use prro_domain::delivery::{
    BoundedText, DecodedResponseDigest, GrpcStatusDigest, NoResponseCause, NonEmptyFiscalNumber,
    NonOkStatusCode,
};

use super::dto::CheckAck;
use super::error::DpsError;

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

/// The closed subset of [`NoResponseCause`] a **live wire observation** may legitimately mint
/// (consolidated-audit M2). `CrashedBeforeObservation` is deliberately EXCLUDED: it is a
/// boot-resume/reaper cause (a durable `CALL_STARTED` marker found with no observed response after a
/// reboot), minted by the recovery path — NEVER by a returned transport observation, which by
/// definition *did* observe a returned result. Because the transport `no_response` builder takes
/// `TransportAbsence` (not the full domain `NoResponseCause`), a transport decoder **cannot name**
/// the crash cause: this is a genuine WITHIN-CRATE structural type-seal (the domain `SendResponse`
/// still accepts the full enum for the D/E boot-resume path). Contrast the cross-crate
/// digest/provenance mint, which Rust cannot sibling-seal — there a source-policy lint is the only
/// fence; here co-location makes the seal real.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::transports::dps) enum TransportAbsence {
    /// TCP/TLS/DNS never established a session with the far side.
    LocalHandshakeFailure,
    /// Per-call deadline expired / future dropped / clean shutdown.
    Timeout,
    /// Future cancelled (e.g. shutdown signal).
    Cancelled,
    /// A returned reply/status that is not a trusted DPS envelope (§4.3 branch 9).
    CallFailedWithoutTrustedDpsEnvelope,
}

impl TransportAbsence {
    /// Widen into the domain cause. The match is exhaustive, so a future `TransportAbsence` variant
    /// forces an arm here — and there is intentionally NO arm producing
    /// `NoResponseCause::CrashedBeforeObservation` (M2 seal).
    fn into_cause(self) -> NoResponseCause {
        match self {
            TransportAbsence::LocalHandshakeFailure => NoResponseCause::LocalHandshakeFailure,
            TransportAbsence::Timeout => NoResponseCause::Timeout,
            TransportAbsence::Cancelled => NoResponseCause::Cancelled,
            TransportAbsence::CallFailedWithoutTrustedDpsEnvelope => {
                NoResponseCause::CallFailedWithoutTrustedDpsEnvelope
            }
        }
    }
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
    pub(in crate::transports::dps) fn no_response(cause: TransportAbsence) -> Self {
        Self(RawSendReplyInner::NoResponse {
            cause: cause.into_cause(),
        })
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

/// DEGRADED shadow projection for the `DpsChannel::send_chk_observed` DEFAULT body (mock/test
/// channels only). It sees the reply ALREADY collapsed to `Result<CheckAck, DpsError>`, so it
/// CANNOT frame a real digest (the raw fields are gone). It therefore emits ONLY `Accepted` (when
/// the legacy `Ok(ack)` carries a non-empty id) or `NoResponse{CallFailedWithoutTrustedDpsEnvelope}`
/// — it NEVER fabricates a `ServerCode`/`OkNoFiscalId`/digest, so a default (mock) observation can
/// never masquerade as full-fidelity evidence to a live consumer. The same-reply/digest teeth
/// (which need real digests) run against the `GrpcDpsChannel` override, never this default.
pub(in crate::transports::dps) fn observe_from_legacy(
    legacy: &Result<CheckAck, DpsError>,
) -> RawSendObservation {
    let evidence = match legacy {
        Ok(ack) => match NonEmptyFiscalNumber::from_transport(ack.id.clone()) {
            Some(id) => RawSendReply::accepted(id),
            None => RawSendReply::no_response(TransportAbsence::CallFailedWithoutTrustedDpsEnvelope),
        },
        Err(_) => RawSendReply::no_response(TransportAbsence::CallFailedWithoutTrustedDpsEnvelope),
    };
    RawSendObservation::new(evidence, WireDiagnostics::default())
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

        let none = RawSendReply::no_response(TransportAbsence::Timeout);
        assert!(
            matches!(none.kind(), RawSendReplyKind::NoResponse { cause } if cause == NoResponseCause::Timeout)
        );
    }

    /// CS-3 3.2 PR4 (consolidated-audit M2) — the transport `no_response` builder takes the closed
    /// [`TransportAbsence`] subset, so a live wire observation can NEVER mint the boot-resume cause
    /// `CrashedBeforeObservation`. This is a genuine WITHIN-CRATE structural type-seal:
    /// `RawSendReply::no_response(NoResponseCause::CrashedBeforeObservation)` does not compile (wrong
    /// argument type) and `TransportAbsence` has no crash variant. The compile-time seal is the
    /// primary guarantee; this enumerates the whole subset as a belt-and-suspenders runtime witness —
    /// RED if a future edit adds a crash arm to `TransportAbsence`/`into_cause` and lists it here.
    #[test]
    fn m2_transport_absence_never_widens_to_crash_before_observation() {
        for a in [
            TransportAbsence::LocalHandshakeFailure,
            TransportAbsence::Timeout,
            TransportAbsence::Cancelled,
            TransportAbsence::CallFailedWithoutTrustedDpsEnvelope,
        ] {
            assert_ne!(
                a.into_cause(),
                NoResponseCause::CrashedBeforeObservation,
                "{a:?} must not widen to the boot-resume crash cause"
            );
            let reply = RawSendReply::no_response(a);
            assert!(matches!(
                reply.kind(),
                RawSendReplyKind::NoResponse { cause }
                    if cause != NoResponseCause::CrashedBeforeObservation
            ));
        }
    }

    #[test]
    fn observation_carries_evidence_and_diagnostics() {
        let obs = RawSendObservation::new(
            RawSendReply::no_response(TransportAbsence::CallFailedWithoutTrustedDpsEnvelope),
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

    /// Tooth #9 (degraded-default honesty): the DEFAULT-body projector emits ONLY `Accepted`
    /// (non-empty id) or `NoResponse` — NEVER a fabricated `ServerCode`/`OkNoFiscalId`/digest, so a
    /// mock observation can never masquerade as full-fidelity evidence. The same-reply/digest teeth
    /// (which need REAL digests) therefore run against the `GrpcDpsChannel` override, never this.
    #[test]
    fn observe_from_legacy_is_honestly_degraded() {
        let ok_id = Ok(CheckAck {
            id: "DPS-7".into(),
            id_sign: vec![],
            data_sign: vec![],
        });
        assert!(matches!(
            observe_from_legacy(&ok_id).evidence().kind(),
            RawSendReplyKind::Accepted { .. }
        ));

        // Ok but empty id → NoResponse (NOT OkNoFiscalId — no digest survives the collapse).
        let ok_empty = Ok(CheckAck {
            id: String::new(),
            id_sign: vec![],
            data_sign: vec![],
        });
        assert!(matches!(
            observe_from_legacy(&ok_empty).evidence().kind(),
            RawSendReplyKind::NoResponse { .. }
        ));

        // Any Err → NoResponse, never a fabricated ServerCode/digest.
        let err: Result<CheckAck, DpsError> = Err(DpsError::Server {
            code: -2,
            message: "x".into(),
        });
        assert!(matches!(
            observe_from_legacy(&err).evidence().kind(),
            RawSendReplyKind::NoResponse { .. }
        ));
    }

    /// PR4 §4.3 — the engine mapper `map_send_reply` maps every `RawSendReply` arm to the correct
    /// `SendResponse`. Total over the 6 arms; the code×doc_type `-2/-15` split checked both ways.
    /// (Lives here because only `transports::dps` can mint a `RawSendReply`; the mapper output is
    /// read via the domain's opaque `kind()` views.) Break any mapper arm → RED.
    #[test]
    fn pr4_map_send_reply_covers_section_4_3() {
        use crate::services::write_path::shadow_map::map_send_reply;
        use prro_domain::delivery::{
            DpsReject, RemoteStatusEvidence, SendIndeterminateKind, SendOutcomeKind,
            SendResponseKind,
        };
        use prro_domain::enums::DocType;

        let sell = DocType::Sell;
        let id = |s: &str| NonEmptyFiscalNumber::from_transport(s.to_string()).unwrap();
        let code = |c: i32| NonOkStatusCode::from_transport(c).unwrap();

        // Row 1 — Accepted → Parsed(Accepted{that id}).
        match map_send_reply(&RawSendReply::accepted(id("DPS-9")), sell).kind() {
            SendResponseKind::Parsed(o) => assert!(
                matches!(o.kind(), SendOutcomeKind::Accepted(a) if a.fiscal_number() == "DPS-9")
            ),
            other => panic!("row1: {other:?}"),
        }
        // Row 2 — OkNoFiscalId → Parsed(OkButNoFiscalNumber).
        assert!(matches!(
            map_send_reply(&RawSendReply::ok_no_fiscal_id(dg()), sell).kind(),
            SendResponseKind::Parsed(o)
                if matches!(o.kind(), SendOutcomeKind::Indeterminate(i)
                    if matches!(i.kind(), SendIndeterminateKind::OkButNoFiscalNumber { .. }))
        ));
        // Row 3 — named -1 → Parsed(Rejected{Verify}).
        assert!(matches!(
            map_send_reply(&RawSendReply::server_code(code(-1), dg()), sell).kind(),
            SendResponseKind::Parsed(o)
                if matches!(o.kind(), SendOutcomeKind::Rejected { verdict: DpsReject::Verify, .. })
        ));
        // Row 4 — -2 non-close → Rejected(Close); -2 close/Z → CloseAmbiguous (doc_type split).
        assert!(matches!(
            map_send_reply(&RawSendReply::server_code(code(-2), dg()), sell).kind(),
            SendResponseKind::Parsed(o)
                if matches!(o.kind(), SendOutcomeKind::Rejected { verdict: DpsReject::Close, .. })
        ));
        assert!(matches!(
            map_send_reply(&RawSendReply::server_code(code(-2), dg()), DocType::ZReport).kind(),
            SendResponseKind::Parsed(o)
                if matches!(o.kind(), SendOutcomeKind::Indeterminate(i)
                    if matches!(i.kind(), SendIndeterminateKind::CloseAmbiguous { .. }))
        ));
        // Row 5 — -3 → SaveError.
        assert!(matches!(
            map_send_reply(&RawSendReply::server_code(code(-3), dg()), sell).kind(),
            SendResponseKind::Parsed(o)
                if matches!(o.kind(), SendOutcomeKind::Indeterminate(i)
                    if matches!(i.kind(), SendIndeterminateKind::SaveError { .. }))
        ));
        // Row 6 — -4 and every unknown non-zero → UnknownStatus.
        for c in [-4i32, -17, 2, 12_345] {
            assert!(
                matches!(
                    map_send_reply(&RawSendReply::server_code(code(c), dg()), sell).kind(),
                    SendResponseKind::Parsed(o)
                        if matches!(o.kind(), SendOutcomeKind::Indeterminate(i)
                            if matches!(i.kind(), SendIndeterminateKind::UnknownStatus { .. }))
                ),
                "row6 code {c}"
            );
        }
        // Row 7 — MissingStatus → Parsed(MissingStatus).
        assert!(matches!(
            map_send_reply(&RawSendReply::missing_status(dg()), sell).kind(),
            SendResponseKind::Parsed(o)
                if matches!(o.kind(), SendOutcomeKind::Indeterminate(i)
                    if matches!(i.kind(), SendIndeterminateKind::MissingStatus { .. }))
        ));
        // Row 8 — RemoteAuthStatus → RemoteStatus (DIRECT), forwarding the SAME grpc digest.
        match map_send_reply(&RawSendReply::remote_auth_status(grpc()), sell).kind() {
            SendResponseKind::RemoteStatus(RemoteStatusEvidence::RemoteAuthStatus(g)) => {
                assert_eq!(
                    g,
                    &grpc(),
                    "row8 must forward the transport-minted grpc digest"
                )
            }
            other => panic!("row8: {other:?}"),
        }
        // Rows 9/10 — NoResponse → NoResponse (DIRECT), forwarding the SAME cause.
        match map_send_reply(&RawSendReply::no_response(TransportAbsence::Timeout), sell).kind() {
            SendResponseKind::NoResponse(c) => assert_eq!(*c, NoResponseCause::Timeout),
            other => panic!("row10: {other:?}"),
        }
    }

    /// CS-3 3.2 PR4 (consolidated-audit B2) — DIGEST FIDELITY per parsed arm. The §4.3 coverage test
    /// above only checks WHICH arm is produced (it feeds every row the same `dg()` and never reads the
    /// `DecodedResponseDigest` back), so a constant/fabricated digest — the auditor's `[0xEE;32]` at
    /// `from_server_code` — stayed all-green. This feeds EACH digest-carrying arm an INDEPENDENT input
    /// digest, reads the carried digest back out of the sealed outcome, and asserts it equals the
    /// input BYTE-FOR-BYTE. Because every row's input byte is unique, a per-arm constant OR a
    /// cross-row swap flips at least one assertion → RED. (The grpc-digest arm 8 is already covered
    /// with an independent digest above.)
    #[test]
    fn pr4_b2_map_send_reply_forwards_exact_digest_per_arm() {
        use crate::services::write_path::shadow_map::map_send_reply;
        use prro_domain::delivery::{
            DecodedResponseDigest, SendIndeterminateKind, SendOutcomeKind, SendResponse,
            SendResponseKind,
        };
        use prro_domain::enums::DocType;

        let sell = DocType::Sell;
        let zrep = DocType::ZReport;
        let code = |c: i32| NonOkStatusCode::from_transport(c).unwrap();
        let dgst = |b: u8| DecodedResponseDigest::from_transport_digest([b; 32]);

        // Pull the carried DecodedResponseDigest bytes out of a parsed outcome. Panics for an
        // arm that carries none (Accepted) or a non-parsed response.
        let parsed_digest = |resp: &SendResponse| -> [u8; 32] {
            match resp.kind() {
                SendResponseKind::Parsed(o) => match o.kind() {
                    SendOutcomeKind::Rejected { digest, .. } => *digest.as_bytes(),
                    SendOutcomeKind::Indeterminate(i) => match i.kind() {
                        SendIndeterminateKind::UnknownStatus { digest, .. }
                        | SendIndeterminateKind::SaveError { digest }
                        | SendIndeterminateKind::CloseAmbiguous { digest }
                        | SendIndeterminateKind::MissingStatus { digest }
                        | SendIndeterminateKind::OkButNoFiscalNumber { digest } => {
                            *digest.as_bytes()
                        }
                    },
                    SendOutcomeKind::Accepted(_) => panic!("Accepted carries no digest"),
                },
                other => panic!("not a parsed outcome: {other:?}"),
            }
        };

        // One case per digest-carrying arm — each with a UNIQUE input byte (single source of truth).
        let cases = [
            {
                let b = 0x11u8;
                (RawSendReply::ok_no_fiscal_id(dgst(b)), sell, b) // OkButNoFiscalNumber
            },
            {
                let b = 0x22u8;
                (RawSendReply::server_code(code(-1), dgst(b)), sell, b) // Rejected(Verify)
            },
            {
                let b = 0x33u8;
                (RawSendReply::server_code(code(-3), dgst(b)), sell, b) // SaveError
            },
            {
                let b = 0x44u8;
                (RawSendReply::server_code(code(-2), dgst(b)), zrep, b) // CloseAmbiguous (close doc)
            },
            {
                let b = 0x55u8;
                (RawSendReply::server_code(code(-4), dgst(b)), sell, b) // UnknownStatus
            },
            {
                let b = 0x66u8;
                (RawSendReply::missing_status(dgst(b)), sell, b) // MissingStatus
            },
        ];
        for (reply, dt, b) in cases {
            let out = parsed_digest(&map_send_reply(&reply, dt));
            assert_eq!(
                out, [b; 32],
                "arm fed digest [{b:#x};32] must forward it EXACTLY (no constant/swap)"
            );
        }
    }
}
