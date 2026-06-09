//! Recover the encapsulated content (`eContent`) from an ATTACHED CMS
//! `SignedData` (PKCS#7).
//!
//! W4-Z3 needs this to seed the fiscal MAC chain: DPS `lastChk` returns the
//! previous check in `data_sign` as an ATTACHED `.p7s`, and the next check's
//! `<MAC>` is the SHA-256 of that previous check's *inner content* (the CMS
//! signature stripped off). This module does the strip — parse the
//! `SignedData` envelope and return the `eContent` octets verbatim.
//!
//! Parsing only — no crypto, no signature verification (callers validate at
//! their layer; the gateway only needs the byte-exact content to hash).

use super::asn1_util::{self as a1, Asn1Error};

const TAG_SEQUENCE: u8 = 0x30;
const TAG_OID: u8 = 0x06;
const TAG_OCTET_STRING: u8 = 0x04;
/// `[0]` EXPLICIT — constructed, context-specific class.
const TAG_CONTEXT_0: u8 = 0xA0;

/// Extract the `eContent` octets from an ATTACHED CMS `SignedData` DER.
///
/// Walks the standard structure and returns the OCTET STRING value bytes:
///
/// ```text
/// ContentInfo ::= SEQUENCE {
///   contentType  OID (id-signedData 1.2.840.113549.1.7.2),
///   content      [0] EXPLICIT SignedData ::= SEQUENCE {
///     version            INTEGER,
///     digestAlgorithms   SET,
///     encapContentInfo   SEQUENCE {
///       eContentType  OID,
///       eContent      [0] EXPLICIT OCTET STRING   -- present iff ATTACHED
///     },
///     ... (certificates / crls / signerInfos — not read here)
///   }
/// }
/// ```
///
/// Returns a typed [`Asn1Error`] on any tag mismatch, truncation, or a
/// DETACHED CMS (no `eContent`). The `eContent` OCTET STRING must be
/// primitive (definite-length DER); a constructed/segmented body (BER) is
/// rejected — DPS emits DER.
pub fn extract_econtent(der: &[u8]) -> Result<Vec<u8>, Asn1Error> {
    let expect = |off: usize, tag: u8, what: &str| -> Result<(), Asn1Error> {
        let t = a1::peek_tag(der, off)?;
        if t != tag {
            return Err(Asn1Error::new(format!(
                "{what}: expected tag {tag:#04x}, found {t:#04x} at offset {off}"
            )));
        }
        Ok(())
    };

    // ContentInfo SEQUENCE.
    expect(0, TAG_SEQUENCE, "ContentInfo")?;
    let (_ci_end, ci_val) = a1::read_tlv(der, 0)?;

    // contentType OID (id-signedData) — skip its value.
    expect(ci_val, TAG_OID, "ContentInfo.contentType")?;
    let (ct_end, _) = a1::read_tlv(der, ci_val)?;

    // content [0] EXPLICIT.
    expect(ct_end, TAG_CONTEXT_0, "ContentInfo.content [0]")?;
    let (_c0_end, c0_val) = a1::read_tlv(der, ct_end)?;

    // SignedData SEQUENCE.
    expect(c0_val, TAG_SEQUENCE, "SignedData")?;
    let (sd_end, sd_val) = a1::read_tlv(der, c0_val)?;

    // version INTEGER → digestAlgorithms SET → encapContentInfo SEQUENCE.
    let (ver_end, _) = a1::read_tlv(der, sd_val)?;
    let (da_end, _) = a1::read_tlv(der, ver_end)?;
    expect(da_end, TAG_SEQUENCE, "encapContentInfo")?;
    let (eci_end, eci_val) = a1::read_tlv(der, da_end)?;
    if eci_end > sd_end {
        return Err(Asn1Error::new(
            "encapContentInfo overruns SignedData".to_string(),
        ));
    }

    // eContentType OID — skip.
    expect(eci_val, TAG_OID, "encapContentInfo.eContentType")?;
    let (ect_end, _) = a1::read_tlv(der, eci_val)?;

    // eContent [0] EXPLICIT — absent ⇒ DETACHED (nothing embedded to strip).
    if ect_end >= eci_end {
        return Err(Asn1Error::new(
            "CMS is DETACHED: encapContentInfo has no eContent to extract".to_string(),
        ));
    }
    expect(ect_end, TAG_CONTEXT_0, "encapContentInfo.eContent [0]")?;
    let (ec0_end, ec0_val) = a1::read_tlv(der, ect_end)?;
    if ec0_end > eci_end {
        return Err(Asn1Error::new(
            "eContent [0] overruns encapContentInfo".to_string(),
        ));
    }

    // The primitive OCTET STRING carrying the content bytes.
    expect(ec0_val, TAG_OCTET_STRING, "eContent OCTET STRING")?;
    let (oct_end, oct_val) = a1::read_tlv(der, ec0_val)?;

    Ok(der[oct_val..oct_end].to_vec())
}
