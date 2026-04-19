//! CMS/CAdES profile selection.
//!
//! A profile bundles together: digest algorithm, signature algorithm,
//! and any CAdES attributes that differ between variants.
//!
//! ## v1 scope (locked 2026-04-15 per expert review)
//!
//! `Dstu4145WithGost34311(pb)` is the default. `Dstu4145WithDstu7564(pb)`
//! (Kupyna-256) is auto-detected from the signer certificate's
//! `signatureAlgorithm` OID — callers don't need to select it manually.
//!
//! The enum is marked `#[non_exhaustive]` so callers cannot assume a
//! closed set of variants; this leaves room for adding Kupyna profiles
//! later without a breaking API change.

use crate::cms::oids;
use const_oid::ObjectIdentifier;

/// v1 profile variants.
///
/// All variants start from CAdES-BES (basic electronic signature,
/// baseline B-B) — the builder can then uplift to T (timestamp) or
/// LT (revocation-values) depending on the caller's options.
/// See `SECURITY.md` for the full threat model and scope rationale.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CmsProfile {
    /// DSTU 4145-LE with GOST 34.311-95 digest, `pb` (polynomial basis) form.
    /// Corresponds to the Ukrainian OID `Dstu4145WithGost34311(pb)`
    /// (`1.2.804.2.1.1.1.1.3.1.1`). Default profile — maps to how existing
    /// Ukrainian PRRO/ДПС deployments sign fiscal documents today.
    Dstu4145WithGost34311Pb,

    /// DSTU 4145-LE with Kupyna-256 (DSTU 7564) digest, `pb` form.
    /// Corresponds to `Dstu4145WithDstu7564-256(pb)` (`1.2.804.2.1.1.1.1.3.6.1.1`).
    /// Next-generation profile — 4× faster hash, same signature algorithm.
    Dstu4145WithDstu7564Pb,
}

impl CmsProfile {
    /// OID of the digest algorithm used for content + signedAttrs hashing.
    pub fn digest_oid(&self) -> ObjectIdentifier {
        match self {
            Self::Dstu4145WithGost34311Pb => oids::GOST_34_311_95,
            Self::Dstu4145WithDstu7564Pb => oids::KUPYNA_256,
        }
    }

    /// Digest output length in bytes.
    pub fn digest_len(&self) -> usize {
        match self {
            Self::Dstu4145WithGost34311Pb => 32,
            Self::Dstu4145WithDstu7564Pb => 32, // Kupyna-256 = 256 bits
        }
    }

    /// OID of the signature algorithm.
    ///
    /// **Profile-specific.** The Ukrainian registry has distinct OIDs for
    /// `Dstu4145WithGost34311(pb)` vs `Dstu4145WithDstu7564(...)`; each
    /// profile maps to its canonical composite-algorithm OID.
    pub fn signature_oid(&self) -> ObjectIdentifier {
        match self {
            Self::Dstu4145WithGost34311Pb => oids::DSTU_4145_WITH_GOST_34311_PB,
            Self::Dstu4145WithDstu7564Pb => oids::DSTU_4145_WITH_DSTU_7564_PB,
        }
    }

    /// Content type for encapContentInfo. Always `id-data` for v1.
    pub fn content_type_oid(&self) -> ObjectIdentifier {
        oids::ID_DATA
    }

    /// Hash used for SigningCertificateV2 cert hash.
    /// Matches the profile's content digest algorithm.
    pub fn cert_hash_oid(&self) -> ObjectIdentifier {
        self.digest_oid()
    }
}

impl CmsProfile {
    /// Auto-detect profile from a DER-encoded X.509 certificate.
    ///
    /// Reads the `signatureAlgorithm` OID from the certificate's
    /// TBSCertificate and maps it to the corresponding CMS profile.
    /// Falls back to `Dstu4145WithGost34311Pb` if the OID is
    /// unrecognized — this ensures backward compatibility with
    /// certificates issued under the legacy GOST regime.
    pub fn from_cert_der(cert_der: &[u8]) -> Self {
        match extract_sig_alg_oid(cert_der) {
            Some(oid_bytes) if oid_bytes == oids::DSTU_4145_WITH_DSTU_7564_PB.as_bytes() => {
                Self::Dstu4145WithDstu7564Pb
            }
            _ => Self::Dstu4145WithGost34311Pb,
        }
    }
}

/// Extract raw signatureAlgorithm OID bytes from a DER X.509 certificate.
///
/// Certificate layout:
/// ```text
/// SEQUENCE {              -- Certificate
///   SEQUENCE {            -- TBSCertificate
///     ...
///   }
///   SEQUENCE {            -- signatureAlgorithm (AlgorithmIdentifier)
///     OID ...             -- ← this is what we extract
///   }
///   BIT STRING ...        -- signatureValue
/// }
/// ```
fn extract_sig_alg_oid(cert_der: &[u8]) -> Option<&[u8]> {
    use crate::cms::asn1_util as a1;

    // Certificate SEQUENCE
    let (_, cert_inner) = a1::read_tlv(cert_der, 0).ok()?;
    // TBSCertificate SEQUENCE — skip it entirely
    let (tbs_end, _) = a1::read_tlv(cert_der, cert_inner).ok()?;
    // signatureAlgorithm SEQUENCE — right after TBS
    let (_, sig_alg_inner) = a1::read_tlv(cert_der, tbs_end).ok()?;
    // OID inside signatureAlgorithm
    let tag = a1::peek_tag(cert_der, sig_alg_inner).ok()?;
    if tag != 0x06 {
        return None; // not an OID
    }
    let (oid_end, oid_inner) = a1::read_tlv(cert_der, sig_alg_inner).ok()?;
    Some(&cert_der[oid_inner..oid_end])
}

impl Default for CmsProfile {
    fn default() -> Self {
        Self::Dstu4145WithGost34311Pb
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_profile() {
        assert_eq!(CmsProfile::default(), CmsProfile::Dstu4145WithGost34311Pb);
    }

    #[test]
    fn test_digest_lengths() {
        assert_eq!(CmsProfile::Dstu4145WithGost34311Pb.digest_len(), 32);
    }

    #[test]
    fn test_signature_oid_is_composite_not_generic() {
        let p = CmsProfile::Dstu4145WithGost34311Pb;
        assert_eq!(p.signature_oid(), oids::DSTU_4145_WITH_GOST_34311_PB);
        assert_eq!(
            p.signature_oid().to_string(),
            "1.2.804.2.1.1.1.1.3.1.1",
        );
    }

    #[test]
    fn test_kupyna_profile_oids() {
        let p = CmsProfile::Dstu4145WithDstu7564Pb;
        assert_eq!(p.digest_oid(), oids::KUPYNA_256);
        assert_eq!(p.digest_len(), 32);
        assert_eq!(p.signature_oid(), oids::DSTU_4145_WITH_DSTU_7564_PB);
        assert_eq!(
            p.signature_oid().to_string(),
            "1.2.804.2.1.1.1.1.3.6.1.1",
        );
    }

    /// Build a minimal X.509 DER cert with a given signatureAlgorithm OID.
    /// Just enough structure for `from_cert_der` to parse — not a real cert.
    fn fake_cert_with_sig_oid(oid_str: &str) -> Vec<u8> {
        let oid = ObjectIdentifier::new_unwrap(oid_str);
        let oid_bytes = oid.as_bytes();

        // AlgorithmIdentifier SEQUENCE { OID }
        let mut alg_id = vec![0x06, oid_bytes.len() as u8];
        alg_id.extend_from_slice(oid_bytes);
        let mut alg_seq = vec![0x30, alg_id.len() as u8];
        alg_seq.extend_from_slice(&alg_id);

        // Minimal TBSCertificate SEQUENCE (just a serial number)
        let tbs_content = vec![0x02, 0x01, 0x01]; // INTEGER 1
        let mut tbs = vec![0x30, tbs_content.len() as u8];
        tbs.extend_from_slice(&tbs_content);

        // signatureValue BIT STRING (empty)
        let sig_val = vec![0x03, 0x02, 0x00, 0x00];

        // Certificate SEQUENCE { TBS, AlgId, SigValue }
        let inner_len = tbs.len() + alg_seq.len() + sig_val.len();
        let mut cert = vec![0x30, 0x82, (inner_len >> 8) as u8, (inner_len & 0xFF) as u8];
        cert.extend_from_slice(&tbs);
        cert.extend_from_slice(&alg_seq);
        cert.extend_from_slice(&sig_val);
        cert
    }

    #[test]
    fn auto_detect_gost_from_cert() {
        let cert = fake_cert_with_sig_oid("1.2.804.2.1.1.1.1.3.1.1");
        assert_eq!(
            CmsProfile::from_cert_der(&cert),
            CmsProfile::Dstu4145WithGost34311Pb,
        );
    }

    #[test]
    fn auto_detect_kupyna_from_cert() {
        let cert = fake_cert_with_sig_oid("1.2.804.2.1.1.1.1.3.6.1.1");
        assert_eq!(
            CmsProfile::from_cert_der(&cert),
            CmsProfile::Dstu4145WithDstu7564Pb,
        );
    }

    #[test]
    fn auto_detect_unknown_oid_falls_back_to_gost() {
        let cert = fake_cert_with_sig_oid("1.2.3.4.5");
        assert_eq!(
            CmsProfile::from_cert_der(&cert),
            CmsProfile::Dstu4145WithGost34311Pb,
        );
    }
}
