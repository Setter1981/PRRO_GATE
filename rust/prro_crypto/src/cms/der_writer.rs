//! Minimal DER writer helpers for CMS assembly.
//!
//! Hand-rolled to keep the CMS builder transparent and audit-friendly.
//! Covers exactly what we need:
//! - SEQUENCE / SET wrappers
//! - INTEGER (small positive values)
//! - OCTET STRING
//! - context-specific tags (EXPLICIT and IMPLICIT)
//! - AlgorithmIdentifier helper
//!
//! Anything more complex (Name, OID parsing) is delegated to `der` and
//! `x509-cert` crates.

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DerWriterError {
    #[error("integer too large for u64 encoding")]
    IntegerOverflow,
    #[error("der: {0}")]
    Der(String),
}

/// Encode DER length (short form for n<128, long form otherwise).
pub fn encode_length(n: usize, out: &mut Vec<u8>) {
    if n < 0x80 {
        out.push(n as u8);
    } else if n < 0x100 {
        out.push(0x81);
        out.push(n as u8);
    } else if n < 0x10000 {
        out.push(0x82);
        out.push((n >> 8) as u8);
        out.push(n as u8);
    } else if n < 0x1000000 {
        out.push(0x83);
        out.push((n >> 16) as u8);
        out.push((n >> 8) as u8);
        out.push(n as u8);
    } else {
        out.push(0x84);
        out.push((n >> 24) as u8);
        out.push((n >> 16) as u8);
        out.push((n >> 8) as u8);
        out.push(n as u8);
    }
}

/// Wrap content with a tag byte and length prefix. Generic helper.
pub fn wrap_tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(content.len() + 8);
    out.push(tag);
    encode_length(content.len(), &mut out);
    out.extend_from_slice(content);
    out
}

/// Wrap content as `SEQUENCE { content }` (tag 0x30).
pub fn sequence(content: &[u8]) -> Vec<u8> {
    wrap_tlv(0x30, content)
}

/// Wrap content as `SET { content }` (tag 0x31).
pub fn set(content: &[u8]) -> Vec<u8> {
    wrap_tlv(0x31, content)
}

/// Wrap content as OCTET STRING (tag 0x04).
pub fn octet_string(content: &[u8]) -> Vec<u8> {
    wrap_tlv(0x04, content)
}

/// Encode a small positive integer as DER INTEGER.
/// For CMS versions (1, 3, 4, 5) this is sufficient.
pub fn integer_u32(value: u32) -> Vec<u8> {
    // Find minimum number of bytes needed (big-endian, two's complement).
    let mut bytes: Vec<u8> = value.to_be_bytes().to_vec();
    while bytes.len() > 1 && bytes[0] == 0 && (bytes[1] & 0x80) == 0 {
        bytes.remove(0);
    }
    // If high bit set, prepend 0x00 to make it positive in two's complement.
    if !bytes.is_empty() && (bytes[0] & 0x80) != 0 {
        bytes.insert(0, 0x00);
    }
    wrap_tlv(0x02, &bytes)
}

/// Wrap content as `[n] EXPLICIT { content }` — context-specific constructed tag.
/// Used for SignedData wrapping in ContentInfo: `content [0] EXPLICIT SignedData`.
pub fn explicit_context_tag(tag_number: u8, content: &[u8]) -> Vec<u8> {
    // Class = context-specific (0b10), Constructed (0b1), tag = tag_number
    debug_assert!(tag_number < 0x1F, "high-tag-number form not supported");
    let tag_byte = 0xA0 | tag_number;
    wrap_tlv(tag_byte, content)
}

/// Wrap content as `[n] IMPLICIT { content }` for constructed types
/// (e.g., SET OF wrapped in [0] IMPLICIT for signedAttrs in SignerInfo).
///
/// Implicit tagging REPLACES the underlying tag with the context tag,
/// while keeping the underlying value/encoding. For a SET OF that would
/// be tag 0x31, we replace with 0xA0 (context-specific [0] constructed).
pub fn implicit_constructed_tag(tag_number: u8, content: &[u8]) -> Vec<u8> {
    debug_assert!(tag_number < 0x1F);
    let tag_byte = 0xA0 | tag_number;
    wrap_tlv(tag_byte, content)
}

/// Build `AlgorithmIdentifier ::= SEQUENCE { algorithm OID, parameters NULL }`.
///
/// We always emit NULL parameters (RFC 4055 §2.1 recommends NULL for hash
/// algorithms; for DSTU 4145-LE the parameters are arguably curve identifier
/// but our profile is hardcoded to PB-257 so we emit NULL for now).
pub fn algorithm_identifier(
    alg_oid: const_oid::ObjectIdentifier,
) -> Result<Vec<u8>, DerWriterError> {
    use der::Encode;
    let oid_der = alg_oid
        .to_der()
        .map_err(|e| DerWriterError::Der(e.to_string()))?;
    let null_der = [0x05u8, 0x00];

    let mut inner = Vec::with_capacity(oid_der.len() + 2);
    inner.extend_from_slice(&oid_der);
    inner.extend_from_slice(&null_der);
    Ok(sequence(&inner))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integer_u32_small() {
        assert_eq!(integer_u32(1), vec![0x02, 0x01, 0x01]);
        assert_eq!(integer_u32(127), vec![0x02, 0x01, 0x7F]);
    }

    #[test]
    fn test_integer_u32_high_bit() {
        // 128 needs leading 0x00 (otherwise interpreted as -128)
        assert_eq!(integer_u32(128), vec![0x02, 0x02, 0x00, 0x80]);
    }

    #[test]
    fn test_sequence_empty() {
        assert_eq!(sequence(&[]), vec![0x30, 0x00]);
    }

    #[test]
    fn test_octet_string() {
        let r = octet_string(&[0xCA, 0xFE]);
        assert_eq!(r, vec![0x04, 0x02, 0xCA, 0xFE]);
    }

    #[test]
    fn test_explicit_context_tag() {
        let r = explicit_context_tag(0, &[0x05, 0x00]);
        assert_eq!(r, vec![0xA0, 0x02, 0x05, 0x00]);
    }
}
