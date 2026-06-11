//! RawSigner trait — abstraction over the underlying DSTU 4145 signer.
//!
//! The CMS builder does not know or care about `Fe`, `Scalar`, or
//! `fixed_base::mul_base`. It only knows: "given these signed-attrs
//! bytes, produce a signature value". The builder is thus reusable
//! with any signer that satisfies this trait — including a hardware
//! signer, a keyholder proxy, or an external process.

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
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
/// `Send + Sync` bound lets the builder run inside
/// `Python::allow_threads(|| ...)` — the pyo3 bindings release the GIL
/// across full CMS sign and the signer reference has to cross into the
/// thread-external closure. All in-process signers we ship today
/// (`DstuInProcessSigner`) are naturally thread-safe; remote/HSM
/// signers must uphold the same bound.
pub trait RawSigner: Send + Sync {
    /// Sign a pre-hashed digest.
    ///
    /// `digest` is the hash of the DER-encoded signedAttrs SET.
    /// Length must match the profile's digest_len.
    ///
    /// Returns raw signature octets (r || s little-endian for DSTU 4145-LE).
    fn sign_digest(&self, digest: &[u8]) -> Result<Vec<u8>, SignerError>;
}

/// Source of per-signature ephemeral scalar `rand_e`.
#[derive(Debug)]
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
///
/// # Secret handling
///
/// `d` is a [`FieldEl`] which auto-zeroes on drop (see that type's
/// `ZeroizeOnDrop` impl). `Debug` is explicitly NOT derived — the
/// manual impl below prints only a placeholder so operator logs can
/// safely format the signer without leaking the private key.
pub struct DstuInProcessSigner {
    d: crate::core::field::FieldEl,
    rand_e: RandESource,
}

impl std::fmt::Debug for DstuInProcessSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DstuInProcessSigner")
            .field("d", &"<redacted>")
            .field("rand_e", &self.rand_e)
            .finish()
    }
}

impl DstuInProcessSigner {
    /// Production constructor: `rand_e` is drawn from `OsRng` per call.
    pub fn new(d: crate::core::field::FieldEl) -> Self {
        Self {
            d,
            rand_e: RandESource::Os,
        }
    }

    /// Test-only constructor. `rand_e` is derived deterministically from
    /// `(seed, digest)` so sign outputs are reproducible across runs.
    ///
    /// **SECURITY**: A deterministic `rand_e` trivially breaks DSTU 4145 —
    /// two signatures sharing the same `rand_e` leak the private key. Only
    /// compile this constructor under the explicit test feature.
    #[cfg(feature = "dangerous_deterministic_k_for_tests")]
    pub fn with_deterministic_seed_for_tests(d: crate::core::field::FieldEl, seed: u64) -> Self {
        Self {
            d,
            rand_e: RandESource::Deterministic(seed),
        }
    }
}

impl RawSigner for DstuInProcessSigner {
    fn sign_digest(&self, digest: &[u8]) -> Result<Vec<u8>, SignerError> {
        use crate::core::curve::Curve;

        let curve = Curve::dstu_pb_257();

        // Enforce the documented digest-length contract: GOST 34.311
        // produces exactly 32 bytes; anything else is either a
        // truncated input, a wrong-hash domain, or an upstream bug.
        // Signing a non-32-byte blob "works" mathematically (it just
        // pads/truncates in `bytes_to_field_el`) but produces a
        // signature over an unintended value.
        const GOST_34311_DIGEST_LEN: usize = 32;
        if digest.len() != GOST_34311_DIGEST_LEN {
            return Err(SignerError::SignFailed(format!(
                "digest length {} != {} (GOST 34.311 output width); \
                 check that the correct hash was applied before signing",
                digest.len(),
                GOST_34311_DIGEST_LEN
            )));
        }

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

            match crate::core::sign::sign(&curve, &self.d, &hash_field, &rand_e) {
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

        Err(last_err.unwrap_or_else(|| SignerError::SignFailed("exhausted rand_e attempts".into())))
    }
}

/// Draw a fresh `rand_e` scalar from `OsRng`, guaranteed to lie in
/// `[1, curve.order)` and packed into a `mod_words`-wide `FieldEl`
/// with bit 256 and above explicitly zero.
///
/// Sprint 2.2: this helper used to draw the full `mod_words * 32` bits
/// of entropy and return an unreduced FieldEl. That violated the
/// `sign()` contract that `rand_e < curve.order`, because the
/// Montgomery ladder then processed all 288 bits while the
/// scalar-domain code only saw the low 256 — producing signatures that
/// failed verification. The fix is rejection sampling on 32 bytes:
/// draw uniformly in `[0, 2^256)`, reduce via `Scalar::from_le_bytes`,
/// reject zero, repack as a canonical 9-word `FieldEl` whose high word
/// is zero by construction. `n ≈ 2^255.7` so the rejection rate is
/// ~1 zero in 2^255 draws — effectively never in practice.
/// Draw a uniformly random non-zero scalar in `[1, n)` from `OsRng`
/// using **true rejection sampling**.
///
/// # Why rejection sampling, not modular reduction
///
/// The previous implementation fed a 256-bit random sample through
/// `Scalar::from_le_bytes`, which is a single-subtract Barrett
/// canonicalizer. Because `n ≈ 2^255.5`, roughly half the 256-bit
/// space is `≥ n` and wraps to `raw − n`, making residues in
/// `[0, 2^256 − n)` occur **twice as often** as the rest.
///
/// For DSTU 4145 (ECDSA-class), a biased ephemeral scalar is
/// key-recovery material given enough signatures (Bleichenbacher
/// lattice attack). The fix: draw raw 256-bit samples, **reject**
/// any `≥ n` or `== 0`, and use the survivors as-is. Expected draws
/// per accepted sample ≈ 2 (acceptable for OsRng throughput).
fn draw_rand_e_os(mod_words: usize) -> Result<crate::core::field::FieldEl, SignerError> {
    use crate::core::scalar::Scalar;
    use rand_core::RngCore;

    let mut rng = rand_core::OsRng;
    loop {
        let mut buf = [0u8; 32];
        rng.try_fill_bytes(&mut buf)
            .map_err(|e| SignerError::Rng(e.to_string()))?;

        // Build raw u256 from LE bytes WITHOUT canonicalization.
        // `Scalar::from_limbs` is a direct wrap — it does not reduce.
        let limbs = [
            u64::from_le_bytes([
                buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
            ]),
            u64::from_le_bytes([
                buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
            ]),
            u64::from_le_bytes([
                buf[16], buf[17], buf[18], buf[19], buf[20], buf[21], buf[22], buf[23],
            ]),
            u64::from_le_bytes([
                buf[24], buf[25], buf[26], buf[27], buf[28], buf[29], buf[30], buf[31],
            ]),
        ];
        let candidate = Scalar::from_limbs(limbs);

        // Reject if >= n (not canonical) or == 0.
        if !candidate.is_canonical() || candidate.is_zero() {
            continue;
        }
        // Wipe the raw buffer before returning.
        use zeroize::Zeroize;
        buf.zeroize();
        return Ok(scalar_to_fieldel(&candidate, mod_words));
    }
}

/// Convert a canonical `Scalar` (guaranteed `< order < 2^256`) into a
/// `mod_words`-wide `FieldEl`. The high words past the scalar's 256-bit
/// payload are explicitly zero, so every caller — including the
/// Montgomery ladder that iterates all `mod_words * 32` bits — sees
/// exactly the same scalar value.
fn scalar_to_fieldel(
    scalar: &crate::core::scalar::Scalar,
    mod_words: usize,
) -> crate::core::field::FieldEl {
    let bytes = scalar.to_le_bytes(); // 32 little-endian bytes
    let mut words = vec![0u32; mod_words];
    for i in 0..8.min(mod_words) {
        words[i] = u32::from_le_bytes([
            bytes[4 * i],
            bytes[4 * i + 1],
            bytes[4 * i + 2],
            bytes[4 * i + 3],
        ]);
    }
    crate::core::field::FieldEl::from_words(words)
}

pub(crate) fn bytes_to_field_el(bytes: &[u8], mod_words: usize) -> crate::core::field::FieldEl {
    let mut words = vec![0u32; mod_words];
    for (i, &b) in bytes.iter().enumerate() {
        let word_idx = i / 4;
        let byte_idx = i % 4;
        if word_idx >= mod_words {
            break;
        }
        words[word_idx] |= (b as u32) << (byte_idx * 8);
    }
    crate::core::field::FieldEl::from_words(words)
}

fn fe_to_bytes_le(fe: &crate::core::field::FieldEl, target_len: usize) -> Vec<u8> {
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
) -> crate::core::field::FieldEl {
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
    use crate::core::field::FieldEl;

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
        let signer_a = DstuInProcessSigner::with_deterministic_seed_for_tests(d.clone(), 7);
        let signer_b = DstuInProcessSigner::with_deterministic_seed_for_tests(d, 7);
        let digest = [0x42u8; 32];

        let sig_a = signer_a.sign_digest(&digest).expect("sign a");
        let sig_b = signer_b.sign_digest(&digest).expect("sign b");

        assert_eq!(sig_a, sig_b, "deterministic seed must reproduce signature");
    }
}
