//! `CryptoProvider` async trait — every cryptographic operation in M2
//! goes through here.
//!
//! Per ADR-M2-1 (in-process) the production impl is `InProcessProvider`
//! backed by `prro_crypto`.  Per ADR-M2-6 NO method takes a DB handle
//! (sqlx types).  Per ADR-M2-5 the request/response types are
//! plaintext-or-public-bytes only — secret material lives behind
//! `SigningSession`.

use async_trait::async_trait;

use crate::crypto::errors::CryptoError;
use crate::crypto::session::SigningSession;

/// Returned by `sign_cms_detached`.  Wraps the ATTACHED CMS SignedData
/// DER bytes (the `canonical_xml` is embedded as `eContent`).
#[derive(Debug, Clone)]
pub struct SignedCmsBytes(pub Vec<u8>);

/// Returned by `fetch_cert_by_ski`.  DER-encoded X.509 cert.
#[derive(Debug, Clone)]
pub struct CertDer(pub Vec<u8>);

/// Result of `verify_dstu`.  Wrap-around-bool to keep the call-site
/// type-clear (`if v.0 { ... }` vs `if v { ... }` — the first hints at
/// "this came from a verifier", the second can be confused with any bool).
#[derive(Debug, Clone, Copy)]
pub struct DstuVerifyResult(pub bool);

/// Inputs to `sign_cms_detached`.  All fields are public bytes or a
/// session reference — no plaintext private key.
pub struct SignCmsRequest<'a> {
    pub session: &'a SigningSession,
    pub canonical_xml: &'a [u8],
    pub profile: prro_crypto::cms::profile::CmsProfile,
}

/// The crypto-substrate trait every M2 caller binds to.
///
/// `Send + Sync` so providers can cross thread boundaries (the in-process
/// impl has zero state and is `Copy`; future remote-sidecar impls will
/// hold transport state behind their own `Mutex`).
///
/// All four methods are `async`.  Internally, sign/decrypt/cmp-fetch
/// delegate to blocking C-FFI in `prro_crypto` via
/// `tokio::task::spawn_blocking`; verify stays on the executor (~µs
/// after pubkey-validation cache warm-up).
#[async_trait]
pub trait CryptoProvider: Send + Sync {
    /// Build a CMS signed envelope around `request.canonical_xml` using
    /// ATTACHED encapsulation — the XML is embedded as `eContent`, the form
    /// DPS `sendChkV2` requires (`check_sign` is the only document carrier on
    /// the wire).  NOTE: the `_detached` suffix is retained for back-compat
    /// only; the produced CMS is ATTACHED.  See `in_process.rs` for rationale.
    async fn sign_cms_detached(
        &self,
        request: SignCmsRequest<'_>,
    ) -> Result<SignedCmsBytes, CryptoError>;

    /// Verify a DSTU 4145 raw signature.
    ///
    /// `content_digest` is the already-computed message hash bytes
    /// (caller chooses the profile and hands in the digest, mirroring
    /// `sign_cms_detached`'s contract); `sig_bytes` is the 64-byte raw
    /// concatenation `r || s` LE-packed (`prro_crypto::python::
    /// raw_split_to_rs` shape); `pubkey_compressed` is the 33-byte LE
    /// compressed point as returned by
    /// `prro_crypto::cms::envelope::extract_cert_pubkey_bytes`.
    async fn verify_dstu(
        &self,
        content_digest: &[u8],
        sig_bytes: &[u8],
        pubkey_compressed: &[u8],
    ) -> Result<DstuVerifyResult, CryptoError>;

    /// Decrypt a CMS envelope (KVT2 / DPS-encrypted response).
    ///
    /// `originator_cert_der` is the originator's signing cert DER —
    /// `prro_crypto::cms::envelope::unwrap_envelope` requires the
    /// originator's PUBLIC key for the ECDH-derived CEK; this impl
    /// extracts the compressed pubkey via `extract_cert_pubkey_bytes`
    /// + `expand_compressed_checked`.  `session` carries the recipient
    /// PRIVATE key.
    async fn unwrap_envelope(
        &self,
        envelope_der: &[u8],
        originator_cert_der: &[u8],
        session: &SigningSession,
    ) -> Result<Vec<u8>, CryptoError>;

    /// Fetch a cert by SKI from the IIT-proprietary CMP-look-alike
    /// channel (W0-2 finding).  URLs are caller-supplied (loaded from
    /// `cert_provisioning_config` / `ca_endpoints` by the service-layer
    /// caller — the provider itself stays stateless).
    ///
    /// `request_timeout` is forwarded to every per-URL probe; the real
    /// `prro_crypto::cms::cmp::fetch_cert_by_ski` is
    /// `(cmp_url: &str, ski: &[u8], timeout: Duration)`.
    async fn fetch_cert_by_ski(
        &self,
        urls: &[String],
        ski: &[u8; 32],
        request_timeout: std::time::Duration,
    ) -> Result<CertDer, CryptoError>;
}
