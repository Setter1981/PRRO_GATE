//! `GrpcDpsChannel` — production `DpsChannel` impl backed by a tonic
//! `ChkIncomeServiceClient`.
//!
//! Holds ONE long-lived `tonic::transport::Channel` (Arc-cloning is
//! cheap; HTTP/2 connections are reused across calls).  Per-call we
//! clone the channel into a fresh `ChkIncomeServiceClient` rather than
//! locking a shared client behind a `Mutex` — tonic clients are
//! designed for this idiom.
//!
//! C3 (this commit) wires the real RPC method bodies + the
//! `tonic::Status` → `DpsError` mapping.  C4 lands the native tonic
//! mock server + integration tests covering the 5 error categories +
//! ByServerFiscalNo match/mismatch/absent triple.

use std::time::Duration;

use async_trait::async_trait;
use tonic::transport::{Channel, ClientTlsConfig, Endpoint};
use tonic::Status;

use super::channel::DpsChannel;
use super::digest_framing::{DigestFramer, MsgType};
use super::dto::{
    decode_offline_codes, try_decode_check_response, try_decode_rro_info_response,
    try_decode_status_response, CheckAck, CheckEnvelope, CheckSignBlob, OfflineCodesResponse,
    RroInfo, StatusSnapshot,
};
use super::error::DpsError;
use super::gen::chk_income_service_client::ChkIncomeServiceClient;
use prro_domain::delivery::GrpcStatusDigest;

/// Whether this channel proved TLS peer-authentication at connect time (R1).
///
/// Gates the CS-3 Slice A′ trust boundary: a gRPC `Unauthenticated` /
/// `PermissionDenied` status may only be surfaced as [`DpsError::RemoteStatus`]
/// (which the domain reads as `AuthenticatedPeer` provenance) when the channel is
/// `TlsProven`.  Over an `Unproven` (plaintext) channel a MITM could forge those
/// statuses, so they collapse to [`DpsError::Transport`] — no authenticated-peer
/// claim without a proven TLS peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PeerAuth {
    /// The endpoint scheme parsed as `https` and the TLS config + eager connect
    /// succeeded — the peer's identity was authenticated by the TLS handshake.
    TlsProven,
    /// Plaintext (`http`) or otherwise non-TLS transport — the peer identity is
    /// NOT authenticated; no `AuthenticatedPeer` provenance may be claimed.
    Unproven,
}

/// Long-lived gRPC client to a single DPS endpoint.
///
/// Construct with [`GrpcDpsChannel::connect`].  The held `Channel` is
/// cheaply `Clone` (it wraps an `Arc` over the underlying HTTP/2
/// connection pool); per-call we clone it into a fresh
/// `ChkIncomeServiceClient` rather than locking a shared client behind
/// a `Mutex` — tonic clients are designed for this idiom.
#[derive(Debug, Clone)]
pub struct GrpcDpsChannel {
    /// Endpoint URI as supplied to `connect`, kept for diagnostics.
    /// Not used on the hot path — the live HTTP/2 state lives in
    /// `channel`.
    #[allow(dead_code)] // diagnostic logging surface — wired in M3 ops layer
    endpoint: String,
    /// Default per-call deadline.  Applied at TWO layers:
    ///   1. `Endpoint::timeout` at construction (covers connect /
    ///      handshake / generic transport-level stalls).
    ///   2. `tonic::Request::set_timeout` on every per-call request
    ///      (writes the `grpc-timeout` HTTP/2 metadata header that
    ///      tonic 0.12 honours as the gRPC deadline; without this
    ///      the server has no deadline visibility and a hung server
    ///      could keep the call open past `Endpoint::timeout`).
    request_timeout: Duration,
    channel: Channel,
    /// TLS trust fact for this channel (R1) — gates `RemoteStatus` emission.
    peer_auth: PeerAuth,
}

impl GrpcDpsChannel {
    /// Open a long-lived channel to `endpoint` (e.g.
    /// `https://cabinet.tax.gov.ua:9443`).  Eagerly establishes the
    /// HTTP/2 connection so a misconfigured endpoint surfaces a
    /// typed `DpsError::Transport` at construction time instead of
    /// on the first RPC.  Validates the URI and configures the
    /// default per-call deadline.
    pub async fn connect(endpoint: &str, request_timeout: Duration) -> Result<Self, DpsError> {
        let mut ep = Endpoint::from_shared(endpoint.to_string())
            .map_err(|e| DpsError::Transport(format!("invalid endpoint URI: {e}")))?
            .timeout(request_timeout)
            .connect_timeout(request_timeout);
        // **TLS fix 2026-05-25 (smoke A/B root cause)**: `https://`
        // endpoints require explicit `tls_config(...)` on tonic 0.12
        // — without it the channel silently fails з "transport error"
        // on first RPC.  `with_native_roots()` mirrors Python
        // grpc.ssl_channel_credentials(root_certificates=None) behavior
        // (uses OS native trust store).  Plain `http://` endpoints
        // unaffected (used by in-process mock tests in
        // dps_channel_smoke.rs).
        // R1: derive the TLS trust fact from the PARSED URI scheme (tonic parsed the
        // endpoint in `from_shared` above), NEVER a string prefix — a malformed or
        // non-`https` scheme can never be mistaken for a TLS channel.  `https` →
        // configure TLS (native roots, per the 2026-05-25 smoke fix) and mark the peer
        // TLS-proven; anything else (plaintext `http`, used by the in-process mock in
        // dps_channel_smoke.rs) stays `Unproven`, so no `AuthenticatedPeer` claim is made.
        let peer_auth = if ep.uri().scheme_str() == Some("https") {
            ep = ep
                .tls_config(ClientTlsConfig::new().with_native_roots())
                .map_err(|e| DpsError::Transport(format!("tls config: {e}")))?;
            PeerAuth::TlsProven
        } else {
            PeerAuth::Unproven
        };
        let channel = ep
            .connect()
            .await
            .map_err(|e| DpsError::Transport(format!("connect: {e}")))?;
        Ok(Self {
            endpoint: endpoint.to_string(),
            request_timeout,
            channel,
            peer_auth,
        })
    }

    /// Build a fresh per-call client by cloning the long-lived
    /// channel.  Channel clone is an Arc bump; no new TCP / HTTP/2
    /// state is allocated.
    fn client(&self) -> ChkIncomeServiceClient<Channel> {
        ChkIncomeServiceClient::new(self.channel.clone())
    }

    /// Wrap a request payload in a `tonic::Request` with the
    /// `grpc-timeout` metadata header set to `self.request_timeout`.
    /// `Endpoint::timeout` alone is not enough — it covers transport
    /// stalls but does NOT write the gRPC deadline header, so a
    /// server that holds a unary call open indefinitely could blow
    /// past the configured deadline.  `Request::set_timeout` is the
    /// tonic 0.12-documented way to set the gRPC deadline (W0-1
    /// review finding).
    fn request<T>(&self, payload: T) -> tonic::Request<T> {
        let mut req = tonic::Request::new(payload);
        req.set_timeout(self.request_timeout);
        req
    }
}

/// Map a `tonic::Status` (gRPC-level error) to a `DpsError`, gated by the channel's
/// TLS trust fact (R1).
///
/// **The law (CS-3 Slice A′ corrected by R1):**
/// ```text
/// RemoteStatus  ⇔  code ∈ {Unauthenticated, PermissionDenied}  ∧  TLS peer-auth proven
/// ```
/// - `Unauthenticated | PermissionDenied` **over a `TlsProven` channel** →
///   [`DpsError::RemoteStatus`]: an authenticated TLS peer (WAF / gateway) returned a
///   status, not a transport silence.  They have no DPS application envelope and no DPS
///   status code, so they cannot map to [`DpsError::Authorization`] (which needs a typed
///   `AuthorizationKind` from a parsed envelope).  A separate variant lets the classifier
///   differentiate them from genuine transport absences without parsing error strings.
/// - **The same two codes over an `Unproven` (plaintext) channel** → [`DpsError::Transport`]:
///   without a proven TLS peer, a plaintext MITM could forge those statuses, so no
///   `AuthenticatedPeer` provenance may be claimed (provenance-forgery fix, R1).
/// - **Everything else** (DeadlineExceeded, Unavailable, Internal, Unknown, connection
///   errors, …) → [`DpsError::Transport`].
///
/// The application-level [`DpsError::Authorization`] variant remains reserved for
/// documented DPS status codes from parsed `CheckResponse` / `StatusResponse` /
/// `RroInfoResponse` envelopes.
fn map_tonic_status(s: Status, peer_auth: PeerAuth) -> DpsError {
    match s.code() {
        tonic::Code::Unauthenticated | tonic::Code::PermissionDenied
            if peer_auth == PeerAuth::TlsProven =>
        {
            // R3: fingerprint the actual status reply as lossless raw-reply evidence.
            let digest = status_digest(&s);
            DpsError::RemoteStatus {
                code: format!("{:?}", s.code()),
                message: s.message().to_string(),
                digest,
            }
        }
        _ => DpsError::Transport(format!("gRPC {:?}: {}", s.code(), s.message())),
    }
}

/// Byte-exact framed digest of a gRPC status reply (`{code, message, details}`) — the
/// decoded-content evidence for [`DpsError::RemoteStatus`] (CS-3 3.2, spec §4.1). For an
/// authenticated-peer error status the status IS the reply. `code` is the **canonical gRPC numeric
/// code** (`tonic::Code as i32` → `i64`-be), NOT a `Debug` string. This is the SOLE mint of a
/// `GrpcStatusDigest` (source-gated to this decoder module — PR1 pin 5).
fn status_digest(s: &Status) -> GrpcStatusDigest {
    let mut f = DigestFramer::new(MsgType::GrpcStatus);
    f.field_int(s.code() as i32 as i64)
        .field_str(s.message())
        .field_bytes(s.details());
    GrpcStatusDigest::from_transport_digest(f.finalize())
}

#[async_trait]
impl DpsChannel for GrpcDpsChannel {
    async fn send_chk(&self, envelope: CheckEnvelope) -> Result<CheckAck, DpsError> {
        crate::db::tx::assert_not_in_with_immediate("send_chk");
        let req = self.request(envelope.into());
        let resp = self
            .client()
            .send_chk_v2(req)
            .await
            .map_err(|s| map_tonic_status(s, self.peer_auth))?;
        try_decode_check_response(resp.into_inner())
    }

    async fn last_chk(&self, fn_sign: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        crate::db::tx::assert_not_in_with_immediate("last_chk");
        let req = self.request(fn_sign.into());
        let resp = self
            .client()
            .last_chk(req)
            .await
            .map_err(|s| map_tonic_status(s, self.peer_auth))?;
        try_decode_check_response(resp.into_inner())
    }

    async fn ping(&self, envelope: CheckEnvelope) -> Result<CheckAck, DpsError> {
        crate::db::tx::assert_not_in_with_immediate("ping");
        let req = self.request(envelope.into());
        let resp = self
            .client()
            .ping(req)
            .await
            .map_err(|s| map_tonic_status(s, self.peer_auth))?;
        try_decode_check_response(resp.into_inner())
    }

    async fn status_rro(&self, fn_sign: &CheckSignBlob) -> Result<StatusSnapshot, DpsError> {
        crate::db::tx::assert_not_in_with_immediate("status_rro");
        let req = self.request(fn_sign.into());
        let resp = self
            .client()
            .status_rro(req)
            .await
            .map_err(|s| map_tonic_status(s, self.peer_auth))?;
        try_decode_status_response(resp.into_inner())
    }

    async fn info_rro(&self, fn_sign: &CheckSignBlob) -> Result<RroInfo, DpsError> {
        crate::db::tx::assert_not_in_with_immediate("info_rro");
        let req = self.request(fn_sign.into());
        let resp = self
            .client()
            .info_rro(req)
            .await
            .map_err(|s| map_tonic_status(s, self.peer_auth))?;
        try_decode_rro_info_response(resp.into_inner())
    }

    async fn ask_offline_codes(
        &self,
        envelope: CheckEnvelope,
    ) -> Result<OfflineCodesResponse, DpsError> {
        crate::db::tx::assert_not_in_with_immediate("ask_offline_codes");
        // T=112 rides the same sendChkV2 RPC as send_chk (live-proven 2026-07-07:
        // typCheck=3 / ServiceChk on sendChkV2 returned CheckAck with data_sign
        // containing CMS-wrapped offline-code XML).
        let req = self.request(envelope.into());
        let resp = self
            .client()
            .send_chk_v2(req)
            .await
            .map_err(|s| map_tonic_status(s, self.peer_auth))?;
        let ack = try_decode_check_response(resp.into_inner())?;
        decode_offline_codes(&ack.data_sign)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transports::dps::error::DpsError;

    // RP4B-4: map_tonic_status split — Unauthenticated / PermissionDenied
    // MUST yield RemoteStatus; deadline / unavailable MUST yield Transport.

    #[test]
    fn rp4b4_unauthenticated_yields_remote_status() {
        let s = Status::unauthenticated("invalid token");
        let err = map_tonic_status(s, PeerAuth::TlsProven);
        assert!(
            matches!(err, DpsError::RemoteStatus { .. }),
            "expected RemoteStatus, got {err:?}"
        );
        if let DpsError::RemoteStatus { code, message, .. } = err {
            assert_eq!(code, "Unauthenticated");
            assert_eq!(message, "invalid token");
        }
    }

    #[test]
    fn rp4b4_permission_denied_yields_remote_status() {
        let s = Status::permission_denied("cert mismatch");
        let err = map_tonic_status(s, PeerAuth::TlsProven);
        assert!(
            matches!(err, DpsError::RemoteStatus { .. }),
            "expected RemoteStatus, got {err:?}"
        );
        if let DpsError::RemoteStatus { code, message, .. } = err {
            assert_eq!(code, "PermissionDenied");
            assert_eq!(message, "cert mismatch");
        }
    }

    /// CS-3 3.2: `RemoteStatus` carries a `GrpcStatusDigest` — the byte-exact framed fingerprint of
    /// the ACTUAL gRPC status reply (canonical numeric code, §4.1), so the Bridge carries the
    /// transport-minted digest, never a fabricated one. Distinct status content → distinct digest;
    /// identical content → identical digest. Teeth: a constant/fabricated digest fails the `assert_ne!`.
    #[test]
    fn r3_remote_status_carries_reply_digest_not_fabricated() {
        let mk =
            |msg: &str| match map_tonic_status(Status::unauthenticated(msg), PeerAuth::TlsProven) {
                DpsError::RemoteStatus { digest, .. } => digest,
                other => panic!("expected RemoteStatus, got {other:?}"),
            };
        let da = mk("reply A");
        let db = mk("reply B");
        let da2 = mk("reply A");
        assert_ne!(da, db, "different replies must yield different digests");
        assert_eq!(
            da, da2,
            "identical replies must yield identical digest (deterministic)"
        );
    }

    /// R1 (TLS trust boundary). The LAW:
    ///   RemoteStatus ⇔ code ∈ {Unauthenticated, PermissionDenied} ∧ TLS peer-auth proven.
    /// Over an UNPROVEN (plaintext) channel a gRPC Unauthenticated / PermissionDenied
    /// cannot be trusted as an authenticated-peer reply — a plaintext MITM could forge
    /// it — so it MUST map to `Transport`, never `RemoteStatus` (which the domain reads
    /// as `AuthenticatedPeer` provenance). Forgery canary, run for BOTH codes.
    #[test]
    fn r1_unproven_channel_cannot_forge_authenticated_peer() {
        for s in [
            Status::unauthenticated("forged over plaintext"),
            Status::permission_denied("forged over plaintext"),
        ] {
            let code = s.code();
            let err = map_tonic_status(s, PeerAuth::Unproven);
            assert!(
                matches!(err, DpsError::Transport(_)),
                "plaintext (Unproven) channel must NOT yield RemoteStatus for {code:?} \
                 (AuthenticatedPeer provenance forgery); got {err:?}"
            );
        }
    }

    /// R1: over a TLS-proven channel the same two codes DO yield `RemoteStatus` — the
    /// authenticated-peer reply is genuine (the other half of the iff).
    #[test]
    fn r1_tls_proven_channel_yields_remote_status_for_both_codes() {
        for (s, want_code) in [
            (Status::unauthenticated("x"), "Unauthenticated"),
            (Status::permission_denied("y"), "PermissionDenied"),
        ] {
            match map_tonic_status(s, PeerAuth::TlsProven) {
                DpsError::RemoteStatus { code, .. } => assert_eq!(code, want_code),
                other => {
                    panic!("TLS-proven must yield RemoteStatus for {want_code}, got {other:?}")
                }
            }
        }
    }

    #[test]
    fn rp4b4_deadline_exceeded_yields_transport() {
        let s = Status::deadline_exceeded("timeout");
        let err = map_tonic_status(s, PeerAuth::TlsProven);
        assert!(
            matches!(err, DpsError::Transport(_)),
            "expected Transport, got {err:?}"
        );
    }

    #[test]
    fn rp4b4_unavailable_yields_transport() {
        let s = Status::unavailable("connection refused");
        let err = map_tonic_status(s, PeerAuth::TlsProven);
        assert!(
            matches!(err, DpsError::Transport(_)),
            "expected Transport, got {err:?}"
        );
    }

    #[test]
    fn rp4b4_remote_status_retry_class_equals_transport_retry_class() {
        // Behaviour-preservation: route_dps_error(RemoteStatus) must
        // yield the same retry_class as route_dps_error(Transport).
        use crate::db::models::enums::DocType;
        use crate::services::write_path::error_routing::route_dps_error;
        let transport_decision = route_dps_error(
            &DpsError::Transport("TLS reset".into()),
            DocType::Sell,
            true,
        );
        let remote_status_decision = route_dps_error(
            &DpsError::RemoteStatus {
                code: "Unauthenticated".into(),
                message: "invalid token".into(),
                digest: prro_domain::delivery::GrpcStatusDigest::from_transport_digest([0xAB; 32]),
            },
            DocType::Sell,
            true,
        );
        assert_eq!(
            transport_decision.retry_class, remote_status_decision.retry_class,
            "RemoteStatus must route identically to Transport (compatibility projection; emission is TLS-gated per R1)"
        );
        assert_eq!(
            transport_decision.target_state,
            remote_status_decision.target_state,
        );
        assert_eq!(
            transport_decision.audit_event,
            remote_status_decision.audit_event,
        );
    }
}
