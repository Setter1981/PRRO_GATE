//! CMS/CAdES profile selection.
//!
//! A profile bundles together: digest algorithm, signature algorithm,
//! and any CAdES attributes that differ between variants.
//!
//! ## v1 scope (locked 2026-04-15 per expert review)
//!
//! Only `Dstu4145WithGost34311(pb)` ships in v1. Kupyna-based profiles
//! (`Dstu4145WithDstu7564`) are reserved for a future minor version —
//! the OIDs belong to a different signature algorithm branch in the
//! Ukrainian registry and must not be conflated with the GOST variant.
//!
//! The enum is marked `#[non_exhaustive]` so callers cannot assume a
//! closed set of variants; this leaves room for adding Kupyna profiles
//! later without a breaking API change.

use crate::cms::oids;
use const_oid::ObjectIdentifier;

/// v1 profile variants.
///
/// All variants are CAdES-BES (basic electronic signature, baseline B-B),
/// detached content, single signer, certificate embedded. See
/// `THREAT_MODEL.md` and the Phase 4 CMS plan for scope rationale.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CmsProfile {
    /// DSTU 4145-LE with GOST 34.311-95 digest, `pb` (polynomial basis) form.
    /// Corresponds to the Ukrainian OID `Dstu4145WithGost34311(pb)`
    /// (`1.2.804.2.1.1.1.1.3.1.1`). This is the only profile supported by
    /// v1 and maps directly to how existing Ukrainian PRRO/ДПС deployments
    /// sign fiscal documents today.
    Dstu4145WithGost34311Pb,
}

impl CmsProfile {
    /// OID of the digest algorithm used for content + signedAttrs hashing.
    pub fn digest_oid(&self) -> ObjectIdentifier {
        match self {
            Self::Dstu4145WithGost34311Pb => oids::GOST_34_311_95,
        }
    }

    /// Digest output length in bytes.
    pub fn digest_len(&self) -> usize {
        match self {
            Self::Dstu4145WithGost34311Pb => 32, // GOST 34.311 = 256 bits
        }
    }

    /// OID of the signature algorithm.
    ///
    /// **Profile-specific.** The Ukrainian registry has distinct OIDs for
    /// `Dstu4145WithGost34311(pb)` vs `Dstu4145WithDstu7564(...)`; returning
    /// a single shared OID for all profiles (the bug present pre-2026-04-15)
    /// would produce structurally valid CMS that announces the wrong
    /// signature algorithm to verifiers. Each profile must map to its
    /// canonical composite-algorithm OID here.
    pub fn signature_oid(&self) -> ObjectIdentifier {
        match self {
            Self::Dstu4145WithGost34311Pb => oids::DSTU_4145_WITH_GOST_34311_PB,
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
        // Regression: sig_oid used to be the SAME constant for every profile,
        // which silently mislabels the signature algorithm in CMS output.
        // Profiles now map to composite OIDs (Dstu4145With<hash>Pb).
        let p = CmsProfile::Dstu4145WithGost34311Pb;
        assert_eq!(p.signature_oid(), oids::DSTU_4145_WITH_GOST_34311_PB);
        assert_eq!(
            p.signature_oid().to_string(),
            "1.2.804.2.1.1.1.1.3.1.1",
            "must be Dstu4145WithGost34311(pb), not a generic DSTU 4145 arc"
        );
    }
}
