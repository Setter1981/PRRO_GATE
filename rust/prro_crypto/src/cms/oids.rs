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

/// Kupyna-256 (DSTU 7564:2014, hash length 256 bits).
/// Ukrainian national hash standard, replacement for GOST 34.311-95.
/// OID: `1.2.804.2.1.1.1.1.2.2.1` per Ukrainian crypto OID registry.
pub const KUPYNA_256: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.804.2.1.1.1.1.2.2.1");

/// `Dstu4145WithDstu7564-256(pb)` — DSTU 4145 signature using
/// polynomial-basis curves with Kupyna-256 hash.
/// OID: `1.2.804.2.1.1.1.1.3.6.1.1` per UAPKI `oids.h` line 206.
/// Note: Kupyna branch is `3.6`, NOT `3.1.2` (GOST is `3.1`).
pub const DSTU_4145_WITH_DSTU_7564_PB: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.804.2.1.1.1.1.3.6.1.1");

// ─── DSTU curve OIDs ────────────────────────────────────────────────────────

/// DSTU_PB_257 — the only curve ДПС uses for fiscal signing.
pub const DSTU_PB_257: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.804.2.1.1.1.1.3.1.1.2.6");

/// SHA-256 (for SigningCertificateV2 cert hash when using modern profile).
pub const SHA_256: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");

// ─── TSP / CAdES-T (RFC 3161 + RFC 5126) ────────────────────────────────────

/// id-aa-signatureTimeStampToken — unsigned attribute carrying a TST that
/// covers the SignerInfo.signature bytes. Canonical embedding point for
/// CAdES-T uplift of an existing CAdES-BES signature.
pub const ID_AA_SIGNATURE_TIME_STAMP_TOKEN: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.2.14");

/// id-ad-timeStamping — SubjectInfoAccess accessMethod marking an
/// accessLocation as a TSA endpoint. Used by jkurwa (and by us) to pull
/// the TSA URL out of the signer's own certificate.
pub const ID_AD_TIME_STAMPING: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.48.3");

/// id-pe-subjectInfoAccess — X.509 extension (RFC 5280 §4.2.2.2) carrying
/// SubjectInfoAccess entries.
pub const ID_PE_SUBJECT_INFO_ACCESS: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.1.11");

// ─── Revocation info / CAdES-LT (RFC 6960 + RFC 5280 + ETSI) ────────────────

/// id-pe-authorityInfoAccess — AIA extension carrying accessMethod entries.
pub const ID_PE_AUTHORITY_INFO_ACCESS: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.1.1");

/// id-ad-ocsp — accessMethod used inside AIA to point at an OCSP responder.
pub const ID_AD_OCSP: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.48.1");

/// id-ce-cRLDistributionPoints — X.509 extension listing CRL URIs.
pub const ID_CE_CRL_DISTRIBUTION_POINTS: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("2.5.29.31");

/// id-aa-ets-revocationValues — unsigned attribute wrapping CRLs +
/// BasicOCSPResponses for CAdES-LT / CAdES-X-Long.
pub const ID_AA_ETS_REVOCATION_VALUES: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.2.24");

/// id-pkix-ocsp-basic — the OID identifying BasicOCSPResponse inside
/// OCSPResponse.responseBytes.
pub const ID_PKIX_OCSP_BASIC: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.48.1.1");

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
