//! `CryptoError` — every `CryptoProvider` method returns this.
//!
//! Per ADR-M2-5 §4: secret-bearing types MUST NOT `#[derive(Debug)]`.
//! `CryptoError` itself does not carry secret bytes today, but its
//! variants carry `String` operator ids and reason enums; we provide a
//! manual `Debug` impl so a future field that did carry secret bytes
//! could be redacted at the same site without surprising any caller.

use std::fmt;

#[derive(thiserror::Error)]
pub enum CryptoError {
    #[error("JKS unseal failed for operator {operator_id}: {reason:?}")]
    JksUnseal {
        operator_id: String,
        reason: SealKind,
    },
    #[error("CMS sign failed: {reason:?}")]
    CmsSign { reason: SignKind },
    #[error("envelope decrypt failed: {reason:?}")]
    EnvelopeDecrypt { reason: DecryptKind },
    #[error("cert fetch failed: {reason:?}")]
    CertFetch { reason: FetchKind },
    #[error("signature verification failed: {reason:?}")]
    VerifyFailed { reason: VerifyKind },
}

impl fmt::Debug for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::JksUnseal {
                operator_id,
                reason,
            } => f
                .debug_struct("JksUnseal")
                .field("operator_id", operator_id)
                .field("reason", reason)
                .finish(),
            Self::CmsSign { reason } => f.debug_struct("CmsSign").field("reason", reason).finish(),
            Self::EnvelopeDecrypt { reason } => f
                .debug_struct("EnvelopeDecrypt")
                .field("reason", reason)
                .finish(),
            Self::CertFetch { reason } => {
                f.debug_struct("CertFetch").field("reason", reason).finish()
            }
            Self::VerifyFailed { reason } => f
                .debug_struct("VerifyFailed")
                .field("reason", reason)
                .finish(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SealKind {
    BadPassword,
    BadSalt,
    MalformedJks,
    KeyExtractionFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignKind {
    CurveMismatch,
    InvalidDigest,
    BackendError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecryptKind {
    ParseFailed,
    KekDeriveFailed,
    MacFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchKind {
    TransportError,
    ParseFailed,
    SkiMismatch,
    AllUrlsFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyKind {
    /// raw `r||s` LE-bytes blob is not exactly 64 bytes for PB-257.
    MalformedSignature,
    /// `expand_compressed_checked` rejected the public key (off-curve,
    /// wrong length, or wrong cofactor).
    MalformedPubkey,
    /// Signature parsed and curve checks passed but `verify` returned
    /// false — used by callers that want to distinguish "wrong sig"
    /// from "malformed inputs".
    SignatureRejected,
}
