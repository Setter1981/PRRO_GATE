//! RawSigner trait — abstraction over the underlying DSTU 4145 signer.
//!
//! The CMS builder does not know or care about `Fe`, `Scalar`, or
//! `fixed_base::mul_base`. It only knows: "given these signed-attrs
//! bytes, produce a signature value". The builder is thus reusable
//! with any signer that satisfies this trait — including a hardware
//! signer, a keyholder proxy, or an external process.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SignerError {
    #[error("DSTU sign failed: {0}")]
    SignFailed(String),
    #[error("invalid key material")]
    InvalidKey,
    #[error("rng failure: {0}")]
    Rng(String),
}

/// Trait implemented by anything that can produce a DSTU 4145-LE signature.
///
/// The input is the **digest of the signed-attributes DER**, not the
/// raw content. Per CMS spec: when signedAttrs is present (required for
/// CAdES-BES), signature covers the DER of SET OF Attributes.
///
/// ## Output contract (locked per expert review 2026-04-15)
///
/// Returns the **raw signature value octets** suitable for direct placement
/// into SignerInfo.signature (which is CMS `SignatureValue ::= OCTET STRING`).
/// The CMS encoder adds the outer OCTET STRING wrapping; implementations
/// must NOT add a `[0x04, len, ...]` DER-TLV wrapper themselves.
///
/// For DSTU 4145-LE the value bytes are `r || s` with each component
/// little-endian encoded (typically 32 bytes each for PB-257 curve,
/// producing a 64-byte signature value). Exact width is implementation-
/// defined but must match what the relying party expects for the
/// signatureAlgorithm OID.
///
/// Interop adapters (e.g. for consumers that expect the legacy jkurwa
/// `short_sign` form of `[0x04, len, r, s]`) should wrap the raw bytes
/// at the adapter layer, not inside the trait.
pub trait RawSigner {
    /// Sign a pre-hashed digest.
    ///
    /// `digest` is the hash of the DER-encoded signedAttrs SET.
    /// Length must match the profile's digest_len.
    ///
    /// Returns raw signature octets (r || s little-endian for DSTU 4145-LE).
    fn sign_digest(&self, digest: &[u8]) -> Result<Vec<u8>, SignerError>;
}

/// Source of per-signature ephemeral scalar `rand_e`.
enum RandESource {
    /// Production path: `rand_core::OsRng`. Each `sign_digest()` call draws
    /// fresh entropy from the OS.
    Os,
    /// Test-only path: deterministic derivation from a seed + digest. Enabled
    /// by the `dangerous_deterministic_k_for_tests` cargo feature.
    #[cfg(feature = "dangerous_deterministic_k_for_tests")]
    Deterministic(u64),
}

/// Adapter that plugs `prro_crypto::sign::sign` into `RawSigner`.
///
/// Holds a private key and a per-signature `rand_e` source. Default
/// constructor uses `OsRng`; a deterministic constructor exists only
/// when the `dangerous_deterministic_k_for_tests` feature is on.
///
/// Output is `r_le(32) || s_le(32)` — raw signature value octets for
/// direct placement into SignerInfo.signature (see RawSigner trait docs).
pub struct DstuInProcessSigner {
    d: crate::field::FieldEl,
    rand_e: RandESource,
}

impl DstuInProcessSigner {
    /// Production constructor: `rand_e` is drawn from `OsRng` per call.
    pub fn new(d: crate::field::FieldEl) -> Self {
        Self { d, rand_e: RandESource::Os }
    }

    /// Test-only constructor. `rand_e` is derived deterministically from
    /// `(seed, digest)` so sign outputs are reproducible across runs.
    ///
    /// **SECURITY**: A deterministic `rand_e` trivially breaks DSTU 4145 —
    /// two signatures sharing the same `rand_e` leak the private key. Only
    /// compile this constructor under the explicit test feature.
    #[cfg(feature = "dangerous_deterministic_k_for_tests")]
    pub fn with_deterministic_seed_for_tests(d: crate::field::FieldEl, seed: u64) -> Self {
        Self { d, rand_e: RandESource::Deterministic(seed) }
    }
}

impl RawSigner for DstuInProcessSigner {
    fn sign_digest(&self, digest: &[u8]) -> Result<Vec<u8>, SignerError> {
        use crate::curve::Curve;

        let curve = Curve::dstu_pb_257();
        let hash_field = bytes_to_field_el(digest, curve.mod_words);

        // Retry budget for the rare case `rand_e` produces a degenerate
        // signature (r == 0 or s == 0). For OsRng we retry up to 8 times;
        // for the deterministic path there is nothing to retry against —
        // a failure there indicates a test-vector bug.
        let max_attempts = match &self.rand_e {
            RandESource::Os => 8,
            #[cfg(feature = "dangerous_deterministic_k_for_tests")]
            RandESource::Deterministic(_) => 1,
        };

        let mut last_err: Option<SignerError> = None;
        for _ in 0..max_attempts {
            let rand_e = match &self.rand_e {
                RandESource::Os => draw_rand_e_os(curve.mod_words)?,
                #[cfg(feature = "dangerous_deterministic_k_for_tests")]
                RandESource::Deterministic(seed) => {
                    derive_deterministic_e(*seed, digest, curve.mod_words)
                }
            };

            match crate::sign::sign(&curve, &self.d, &hash_field, &rand_e) {
                Some(sig) => {
                    let r_bytes = fe_to_bytes_le(&sig.r, 32);
                    let s_bytes = fe_to_bytes_le(&sig.s, 32);
                    let mut out = Vec::with_capacity(64);
                    out.extend_from_slice(&r_bytes);
                    out.extend_from_slice(&s_bytes);
                    return Ok(out);
                }
                None => {
                    last_err = Some(SignerError::SignFailed(
                        "degenerate signature (r=0 or s=0)".into(),
                    ));
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            SignerError::SignFailed("exhausted rand_e attempts".into())
        }))
    }
}

/// Draw a fresh `rand_e` from `OsRng`.
///
/// Fills `mod_words * 4` bytes of entropy and packs them as a FieldEl.
/// The bias toward values >= n is ~2^-256 for PB-257 — negligible. A
/// rare degenerate output is handled by the retry loop in the caller.
fn draw_rand_e_os(mod_words: usize) -> Result<crate::field::FieldEl, SignerError> {
    use rand_core::RngCore;
    let mut rng = rand_core::OsRng;
    let mut buf = vec![0u8; mod_words * 4];
    rng.try_fill_bytes(&mut buf)
        .map_err(|e| SignerError::Rng(e.to_string()))?;
    Ok(bytes_to_field_el(&buf, mod_words))
}

/// Interop adapter: wraps a raw signature value (from RawSigner) in the
/// legacy jkurwa "short_sign" form `[0x04, len, r_le, s_le]`.
///
/// Only use when the relying party specifically expects this format.
/// By default, CMS SignerInfo.signature takes the raw bytes directly.
pub fn to_jkurwa_short_sign(raw_sig: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + raw_sig.len());
    out.push(0x04);
    out.push(raw_sig.len() as u8);
    out.extend_from_slice(raw_sig);
    out
}

fn bytes_to_field_el(bytes: &[u8], mod_words: usize) -> crate::field::FieldEl {
    let mut words = vec![0u32; mod_words];
    for (i, &b) in bytes.iter().enumerate() {
        let word_idx = i / 4;
        let byte_idx = i % 4;
        if word_idx >= mod_words {
            break;
        }
        words[word_idx] |= (b as u32) << (byte_idx * 8);
    }
    crate::field::FieldEl::from_words(words)
}

fn fe_to_bytes_le(fe: &crate::field::FieldEl, target_len: usize) -> Vec<u8> {
    let mut out = vec![0u8; target_len];
    for (word_idx, word) in fe.bytes.iter().enumerate() {
        for byte_offset in 0..4 {
            let idx = word_idx * 4 + byte_offset;
            if idx >= target_len {
                break;
            }
            out[idx] = ((word >> (byte_offset * 8)) & 0xFF) as u8;
        }
    }
    out
}

#[cfg(feature = "dangerous_deterministic_k_for_tests")]
fn derive_deterministic_e(
    seed: u64,
    digest: &[u8],
    mod_words: usize,
) -> crate::field::FieldEl {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(seed.to_le_bytes());
    hasher.update(digest);
    let first = hasher.finalize();

    let mut hasher = Sha1::new();
    hasher.update(&first);
    hasher.update(b"prro_crypto_rand_e");
    let second = hasher.finalize();

    let mut combined = Vec::with_capacity(40);
    combined.extend_from_slice(&first);
    combined.extend_from_slice(&second);

    bytes_to_field_el(&combined[..32], mod_words)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::FieldEl;

    /// Two signatures over the same digest must differ when OsRng is in use.
    /// If they matched, `rand_e` would be reused across signatures — the
    /// classic DSTU/ECDSA key-recovery disaster.
    #[test]
    fn os_rng_produces_fresh_rand_e_per_call() {
        let d = FieldEl::from_hex("DEADBEEFCAFE12345678", 9);
        let signer = DstuInProcessSigner::new(d);
        let digest = [0x42u8; 32];

        let sig_a = signer.sign_digest(&digest).expect("sign a");
        let sig_b = signer.sign_digest(&digest).expect("sign b");

        assert_eq!(sig_a.len(), 64);
        assert_eq!(sig_b.len(), 64);
        assert_ne!(
            sig_a, sig_b,
            "OsRng must draw fresh rand_e per call — got identical signatures"
        );
    }

    /// Counterpart check: under the deterministic test feature, same (seed,
    /// digest) must reproduce the same signature. This guards tests that
    /// depend on reproducible outputs.
    #[cfg(feature = "dangerous_deterministic_k_for_tests")]
    #[test]
    fn deterministic_seed_reproduces_signature() {
        let d = FieldEl::from_hex("DEADBEEFCAFE12345678", 9);
        let signer_a =
            DstuInProcessSigner::with_deterministic_seed_for_tests(d.clone(), 7);
        let signer_b = DstuInProcessSigner::with_deterministic_seed_for_tests(d, 7);
        let digest = [0x42u8; 32];

        let sig_a = signer_a.sign_digest(&digest).expect("sign a");
        let sig_b = signer_b.sign_digest(&digest).expect("sign b");

        assert_eq!(sig_a, sig_b, "deterministic seed must reproduce signature");
    }
}
