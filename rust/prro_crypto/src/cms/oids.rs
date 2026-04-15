//! OID constants for DSTU 4145 fiscal signing.
//!
//! Centralizes all Object Identifiers used by the CMS/CAdES-BES builder.
//! Values copied from the Ukrainian cryptographic standard registry
//! (DSTU 4145-2002, DSTU 7564-2014, GOST 34.311-95) and RFC 5652 / 5035.

use const_oid::ObjectIdentifier;

// ─── CMS standard OIDs (RFC 5652) ───────────────────────────────────────────

/// id-data — the default content type for detached signatures.
pub const ID_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.1");

/// id-signedData
pub const ID_SIGNED_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2");

/// id-contentType signed attribute
pub const ID_CONTENT_TYPE: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.3");

/// id-messageDigest signed attribute
pub const ID_MESSAGE_DIGEST: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");

/// id-signingTime signed attribute (optional for BES)
pub const ID_SIGNING_TIME: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.5");

/// id-aa-signingCertificateV2 — ESS (RFC 5035)
pub const ID_AA_SIGNING_CERTIFICATE_V2: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.2.47");

// ─── DSTU composite signature-with-hash OIDs ────────────────────────────────
//
// Important (locked 2026-04-15 per expert review B4): DSTU 4145 does NOT
// use a single "signature algorithm" OID for all hashes. The Ukrainian
// registry defines one composite OID per (curve-form × hash) combination.
// Using the GOST-variant OID to announce a Kupyna-signed CMS would be
// structurally valid DER but semantically wrong — verifiers would hash
// signedAttrs with GOST 34.311 instead of the actual algorithm and reject
// the signature. Each `CmsProfile` MUST map to its own composite OID.

/// `Dstu4145WithGost34311(pb)` — DSTU 4145 signature using polynomial-basis
/// curves with GOST 34.311-95 hash. Canonical OID for all v1 fiscal signing.
pub const DSTU_4145_WITH_GOST_34311_PB: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.804.2.1.1.1.1.3.1.1");

// ─── DSTU hash algorithm OIDs ───────────────────────────────────────────────

/// GOST 34.311-95 hash (legacy, default for v1).
/// 256-bit output, used by majority of current Ukrainian PRRO deployments.
pub const GOST_34_311_95: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.804.2.1.1.1.1.2.1");

// ─── DSTU curve OIDs ────────────────────────────────────────────────────────

/// DSTU_PB_257 — the only curve ДПС uses for fiscal signing.
pub const DSTU_PB_257: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.804.2.1.1.1.1.3.1.1.2.6");

/// SHA-256 (for SigningCertificateV2 cert hash when using modern profile).
pub const SHA_256: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oid_arcs_match_strings() {
        // Spot-check that OIDs parse without panic (const_oid validates at macro time).
        assert_eq!(ID_DATA.to_string(), "1.2.840.113549.1.7.1");
        assert_eq!(
            DSTU_4145_WITH_GOST_34311_PB.to_string(),
            "1.2.804.2.1.1.1.1.3.1.1"
        );
        assert_eq!(GOST_34_311_95.to_string(), "1.2.804.2.1.1.1.1.2.1");
    }
}
