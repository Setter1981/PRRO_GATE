//! Shared ASN.1 DER helpers for the CMS layer.
//!
//! All four CMS modules (`cmp`, `envelope`, `revocation`, `tsp`) each
//! grew their own copy of `read_tlv` / `read_integer_small` / length
//! helpers. Each copy had slightly different error types and — more
//! importantly — slightly different guards. An audit pass found that
//! several call sites indexed `data[pos]` after a `read_tlv` without
//! rechecking `pos < data.len()`, which turns a truncated-but-declared-
//! well-formed DER into a panic.
//!
//! This module consolidates the primitives so every caller gets the
//! same bounded, fallible API. Crate-internal (`pub(crate)`) by design —
//! external consumers stick to the typed error returns of each module.

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("ASN.1 parse: {0}")]
pub struct Asn1Error(pub String);

impl Asn1Error {
    #[inline]
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

/// Read a DER TLV at `offset`.
///
/// Returns `(end_offset, content_start)` on success. Errors on:
/// - offset past end of buffer
/// - truncated length field
/// - non-supported length form (indefinite / > 4-byte)
/// - declared length extending past the buffer
///
/// **Does NOT** move `offset` past the TLV; the caller does that
/// by setting their cursor to `end_offset`.
#[inline]
pub fn read_tlv(data: &[u8], offset: usize) -> Result<(usize, usize), Asn1Error> {
    if offset >= data.len() {
        return Err(Asn1Error::new(format!(
            "TLV read past end at offset {offset} (buffer {} bytes)",
            data.len()
        )));
    }
    let mut pos = offset + 1;
    if pos >= data.len() {
        return Err(Asn1Error::new("truncated length byte".to_string()));
    }
    let len_byte = data[pos];
    pos += 1;
    let length: usize = if len_byte < 0x80 {
        len_byte as usize
    } else {
        let nbytes = (len_byte & 0x7f) as usize;
        if nbytes == 0 {
            return Err(Asn1Error::new(
                "indefinite-length form not supported in DER".to_string(),
            ));
        }
        if nbytes > 4 {
            return Err(Asn1Error::new(format!(
                "unsupported DER length form (>4 bytes): {nbytes}"
            )));
        }
        if pos + nbytes > data.len() {
            return Err(Asn1Error::new("truncated long-form length".to_string()));
        }
        let mut l: usize = 0;
        for _ in 0..nbytes {
            l = (l << 8) | (data[pos] as usize);
            pos += 1;
        }
        l
    };
    let content_start = pos;
    let end = pos
        .checked_add(length)
        .ok_or_else(|| Asn1Error::new("length overflow".to_string()))?;
    if end > data.len() {
        return Err(Asn1Error::new(format!(
            "TLV at {offset} declares content {length} bytes, only {} remain",
            data.len().saturating_sub(pos)
        )));
    }
    Ok((end, content_start))
}

/// Peek the tag byte at `offset` without consuming the TLV. Returns a
/// typed error instead of panicking on out-of-bounds.
#[inline]
pub fn peek_tag(data: &[u8], offset: usize) -> Result<u8, Asn1Error> {
    data.get(offset)
        .copied()
        .ok_or_else(|| Asn1Error::new(format!("peek_tag past end at {offset}")))
}

/// Read a big-endian INTEGER from a half-open byte range.
/// Clamps reads that would overflow u64 — caller is expected to know
/// their range; this helper is for small enumeration-style INTEGERs.
#[inline]
pub fn read_integer_be_small(data: &[u8], start: usize, end: usize) -> Result<u64, Asn1Error> {
    if end > data.len() || start > end {
        return Err(Asn1Error::new(format!(
            "integer range out of bounds: start={start}, end={end}, len={}",
            data.len()
        )));
    }
    if end - start > 8 {
        return Err(Asn1Error::new(format!(
            "integer too wide for u64: {} bytes",
            end - start
        )));
    }
    let mut v: u64 = 0;
    for &byte in &data[start..end] {
        v = (v << 8) | (byte as u64);
    }
    Ok(v)
}

/// Read a 4-byte little-endian u32 at `offset`. Used by the IIT CMP
/// status header and any other LE integer sitting inside a DER payload.
#[inline]
pub fn read_u32_le(data: &[u8], offset: usize) -> Result<u32, Asn1Error> {
    if offset + 4 > data.len() {
        return Err(Asn1Error::new(format!(
            "u32 LE read past end: offset={offset}, need 4, have {}",
            data.len().saturating_sub(offset)
        )));
    }
    Ok(u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_tlv_basic_short_form() {
        let data = &[0x30, 0x03, 0x02, 0x01, 0x05];
        let (end, content) = read_tlv(data, 0).unwrap();
        assert_eq!(end, 5);
        assert_eq!(content, 2);
    }

    #[test]
    fn read_tlv_long_form_2_bytes() {
        // SEQUENCE length = 0x0100 (256)
        let mut data = vec![0x30, 0x82, 0x01, 0x00];
        data.extend(std::iter::repeat_n(0xAA, 256));
        let (end, content) = read_tlv(&data, 0).unwrap();
        assert_eq!(end, 260);
        assert_eq!(content, 4);
    }

    #[test]
    fn read_tlv_rejects_offset_past_end() {
        let data = &[0x30, 0x00];
        let e = read_tlv(data, 5).unwrap_err();
        assert!(e.0.contains("past end"));
    }

    #[test]
    fn read_tlv_rejects_truncated_length_byte() {
        let data = &[0x30]; // tag only, no length
        let e = read_tlv(data, 0).unwrap_err();
        assert!(e.0.contains("truncated length"));
    }

    #[test]
    fn read_tlv_rejects_indefinite_length() {
        let data = &[0x30, 0x80, 0x00, 0x00];
        let e = read_tlv(data, 0).unwrap_err();
        assert!(e.0.contains("indefinite"));
    }

    #[test]
    fn read_tlv_rejects_overlong_length_form() {
        let data = &[0x30, 0x85, 0, 0, 0, 0, 0];
        let e = read_tlv(data, 0).unwrap_err();
        assert!(e.0.contains("unsupported DER length form"));
    }

    #[test]
    fn read_tlv_rejects_truncated_long_length() {
        // Claims 3 length bytes but only 2 follow.
        let data = &[0x30, 0x83, 0x00, 0x00];
        let e = read_tlv(data, 0).unwrap_err();
        assert!(e.0.contains("truncated long-form length"));
    }

    #[test]
    fn read_tlv_rejects_length_past_buffer() {
        // SEQUENCE claims 100 content bytes, only 2 follow.
        let data = &[0x30, 0x64, 0xAA, 0xBB];
        let e = read_tlv(data, 0).unwrap_err();
        assert!(e.0.contains("only"));
    }

    #[test]
    fn peek_tag_bounded() {
        assert_eq!(peek_tag(&[0x30, 0x00], 0).unwrap(), 0x30);
        assert!(peek_tag(&[0x30], 5).is_err());
        assert!(peek_tag(&[], 0).is_err());
    }

    #[test]
    fn read_u32_le_basic() {
        let data = &[0xDE, 0xAD, 0xBE, 0xEF, 0xFF];
        assert_eq!(read_u32_le(data, 0).unwrap(), 0xEFBEADDE);
        assert_eq!(read_u32_le(data, 1).unwrap(), 0xFFEFBEAD);
    }

    #[test]
    fn read_u32_le_rejects_out_of_bounds() {
        assert!(read_u32_le(&[0, 1, 2], 0).is_err());
        assert!(read_u32_le(&[0, 1, 2, 3], 1).is_err());
        assert!(read_u32_le(&[], 0).is_err());
    }

    #[test]
    fn read_integer_be_small_basic() {
        // INTEGER 0x1234 (big-endian 2 bytes)
        assert_eq!(read_integer_be_small(&[0x12, 0x34], 0, 2).unwrap(), 0x1234);
        // 8-byte integer still fits
        let data = &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        assert_eq!(
            read_integer_be_small(data, 0, 8).unwrap(),
            0x0102030405060708
        );
        // 9 bytes does not fit
        let wide = &[0u8; 9];
        assert!(read_integer_be_small(wide, 0, 9).is_err());
        // Out of bounds
        assert!(read_integer_be_small(&[0x01], 0, 5).is_err());
    }

    /// Integer overflow: a crafted long-form length claiming usize::MAX
    /// bytes must be caught by checked_add before we try to dereference
    /// trillions of bytes past the buffer.
    #[test]
    fn read_tlv_rejects_length_overflow() {
        // Construct a 4-byte long-form length that overflows when added
        // to pos. pos after header = 6; length = usize::MAX - 3 would
        // wrap. We can't quite express usize::MAX in 4 bytes on 64-bit,
        // but 0xFFFFFFFF + pos easily exceeds a small buffer's len.
        let data = &[0x30, 0x84, 0xFF, 0xFF, 0xFF, 0xFF];
        let e = read_tlv(data, 0).unwrap_err();
        // We'll land on either "only N remain" or "length overflow"
        // depending on pos arithmetic; accept either as a bounded error.
        assert!(e.0.contains("only") || e.0.contains("overflow"));
    }
}
