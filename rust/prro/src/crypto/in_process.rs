//! `InProcessProvider` — backs `CryptoProvider` with direct `prro_crypto`
//! calls, wrapping blocking C-FFI in `tokio::task::spawn_blocking`.
//!
//! Stateless: timeouts are method-arguments, curve is locked to PB-257
//! (the only DSTU curve in DPS-deployed CAs per W0-3 §1).  No
//! `Config` struct — single source of timeout truth is the caller (a
//! service-layer module) which loads it from M1's existing
//! `cert_provisioning_config.timeout_seconds` column (default 10s).

use std::time::Duration;

use async_trait::async_trait;

use crate::crypto::errors::{CryptoError, DecryptKind, FetchKind, SignKind, VerifyKind};
use crate::crypto::provider::{
    CertDer, CryptoProvider, DstuVerifyResult, SignCmsRequest, SignedCmsBytes,
};
use crate::crypto::session::SigningSession;

#[derive(Debug, Default, Clone, Copy)]
pub struct InProcessProvider;

impl InProcessProvider {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CryptoProvider for InProcessProvider {
    async fn sign_cms_detached(
        &self,
        request: SignCmsRequest<'_>,
    ) -> Result<SignedCmsBytes, CryptoError> {
        // The session is `Clone` over `Arc<Inner>`; we move a clone into
        // the blocking closure.  No copy of the `Zeroizing<[u8; 32]>`
        // private key — only Arc-strong-count bumps.
        let session = request.session.clone();
        let canonical = request.canonical_xml.to_vec();
        let profile = request.profile;

        let bytes =
            tokio::task::spawn_blocking(move || sign_cms_blocking(&canonical, &session, profile))
                .await
                .map_err(|_| CryptoError::CmsSign {
                    reason: SignKind::BackendError,
                })??;

        Ok(SignedCmsBytes(bytes))
    }

    async fn verify_dstu(
        &self,
        content_digest: &[u8],
        sig_bytes: &[u8],
        pubkey_compressed: &[u8],
    ) -> Result<DstuVerifyResult, CryptoError> {
        // Verify is fast (~150µs first call, ~10µs after the pubkey
        // validation cache warms up); stays on the executor.
        verify_blocking(content_digest, sig_bytes, pubkey_compressed).map(DstuVerifyResult)
    }

    async fn unwrap_envelope(
        &self,
        envelope_der: &[u8],
        originator_cert_der: &[u8],
        session: &SigningSession,
    ) -> Result<Vec<u8>, CryptoError> {
        let env = envelope_der.to_vec();
        let originator = originator_cert_der.to_vec();
        let session_clone = session.clone(); // Arc bump, no plaintext copy
        let plaintext = tokio::task::spawn_blocking(move || {
            unwrap_envelope_blocking(&env, &originator, &session_clone)
        })
        .await
        .map_err(|_| CryptoError::EnvelopeDecrypt {
            reason: DecryptKind::ParseFailed,
        })??;
        Ok(plaintext)
    }

    async fn fetch_cert_by_ski(
        &self,
        urls: &[String],
        ski: &[u8; 32],
        request_timeout: Duration,
    ) -> Result<CertDer, CryptoError> {
        if urls.is_empty() {
            return Err(CryptoError::CertFetch {
                reason: FetchKind::AllUrlsFailed,
            });
        }
        let urls_owned: Vec<String> = urls.to_vec();
        let ski_owned: [u8; 32] = *ski;
        let bytes = tokio::task::spawn_blocking(move || {
            fetch_cert_blocking(&urls_owned, &ski_owned, request_timeout)
        })
        .await
        .map_err(|_| CryptoError::CertFetch {
            reason: FetchKind::TransportError,
        })??;
        Ok(CertDer(bytes))
    }
}

// ─── Blocking helpers — invoked from spawn_blocking closures ────────────

fn sign_cms_blocking(
    canonical_xml: &[u8],
    session: &SigningSession,
    profile: prro_crypto::cms::profile::CmsProfile,
) -> Result<Vec<u8>, CryptoError> {
    use prro_crypto::cms::builder::sign_detached_with_content_digest;
    use prro_crypto::cms::profile::CmsProfile;
    use prro_crypto::cms::signer::DstuInProcessSigner;
    use prro_crypto::core::curve::Curve;
    use prro_crypto::core::field::FieldEl;
    use prro_crypto::core::hash::{gost_34_311_95, kupyna_256};

    // Build the content digest per profile.  `sign_detached_with_content_digest`
    // owns signedAttrs hashing — we hand in the message digest.
    // `CmsProfile` is marked `#[non_exhaustive]` upstream; the wildcard
    // arm preserves forward-compat — a future profile we don't recognise
    // returns CurveMismatch (the closest existing reason; the wrapper
    // is locked to PB-257 + GOST-34.311 / Kupyna-256 in M2).
    let content_digest: Vec<u8> = match profile {
        CmsProfile::Dstu4145WithGost34311Pb => gost_34_311_95(canonical_xml).to_vec(),
        CmsProfile::Dstu4145WithDstu7564Pb => kupyna_256(canonical_xml).to_vec(),
        _ => {
            return Err(CryptoError::CmsSign {
                reason: SignKind::CurveMismatch,
            });
        }
    };

    // Build the signer.  `FieldEl::from_le_bytes(bytes, mod_words)`
    // returns `Self` directly (PANICS on `bytes.len() > mod_words * 4`,
    // but PB-257 has mod_words=9 = 36-byte capacity and we hold [u8; 32]
    // — the assertion is structurally unreachable here).
    let curve = Curve::dstu_pb_257();
    let d = FieldEl::from_le_bytes(&session.param_d()[..], curve.mod_words);
    let signer = DstuInProcessSigner::new(d);

    sign_detached_with_content_digest(profile, session.cert_der(), &content_digest, &signer)
        .map_err(|_| CryptoError::CmsSign {
            reason: SignKind::BackendError,
        })
}

fn verify_blocking(
    content_digest: &[u8],
    sig_bytes: &[u8],
    pubkey_compressed: &[u8],
) -> Result<bool, CryptoError> {
    use prro_crypto::core::curve::Curve;
    use prro_crypto::core::field::FieldEl;
    use prro_crypto::core::point::expand_compressed_checked;
    use prro_crypto::core::sign::{verify, Signature};

    if sig_bytes.len() != 64 {
        return Err(CryptoError::VerifyFailed {
            reason: VerifyKind::MalformedSignature,
        });
    }
    let curve = Curve::dstu_pb_257();
    let pub_q = expand_compressed_checked(pubkey_compressed, &curve).map_err(|_| {
        CryptoError::VerifyFailed {
            reason: VerifyKind::MalformedPubkey,
        }
    })?;
    let hash = FieldEl::from_le_bytes(content_digest, curve.mod_words);
    let r = FieldEl::from_le_bytes(&sig_bytes[..32], curve.mod_words);
    let s = FieldEl::from_le_bytes(&sig_bytes[32..], curve.mod_words);
    let signature = Signature { r, s };
    Ok(verify(&curve, &pub_q, &hash, &signature))
}

fn unwrap_envelope_blocking(
    envelope_der: &[u8],
    originator_cert_der: &[u8],
    session: &SigningSession,
) -> Result<Vec<u8>, CryptoError> {
    use prro_crypto::cms::envelope::{extract_cert_pubkey_bytes, unwrap_envelope};
    use prro_crypto::core::curve::Curve;
    use prro_crypto::core::field::FieldEl;
    use prro_crypto::core::point::expand_compressed_checked;

    let curve = Curve::dstu_pb_257();
    let originator_pub_compressed =
        extract_cert_pubkey_bytes(originator_cert_der).map_err(|_| {
            CryptoError::EnvelopeDecrypt {
                reason: DecryptKind::ParseFailed,
            }
        })?;
    let originator_pub =
        expand_compressed_checked(&originator_pub_compressed, &curve).map_err(|_| {
            CryptoError::EnvelopeDecrypt {
                reason: DecryptKind::ParseFailed,
            }
        })?;
    let d = FieldEl::from_le_bytes(&session.param_d()[..], curve.mod_words);

    unwrap_envelope(envelope_der, &d, &originator_pub, &curve).map_err(|_| {
        CryptoError::EnvelopeDecrypt {
            reason: DecryptKind::MacFailed,
        }
    })
}

fn fetch_cert_blocking(
    urls: &[String],
    ski: &[u8; 32],
    request_timeout: Duration,
) -> Result<Vec<u8>, CryptoError> {
    use prro_crypto::cms::cmp::fetch_cert_by_ski;

    for url in urls {
        match fetch_cert_by_ski(url, &ski[..], request_timeout) {
            Ok(cert_der) => return Ok(cert_der),
            Err(_) => continue,
        }
    }
    Err(CryptoError::CertFetch {
        reason: FetchKind::AllUrlsFailed,
    })
}
