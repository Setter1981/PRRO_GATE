//! Minimal ASN.1 DER reader/writer for DSTU 4145 primitives.
//!
//! Hand-rolled to avoid dragging in a full ASN.1 framework when we only need
//! a handful of types (DstuPrivkey, CMS SignedData). Supports just what we
//! need: SEQUENCE, INTEGER, OCTET STRING, OBJECT IDENTIFIER, BIT STRING,
//! IMPLICIT tagged fields with [n] markers.

#[derive(Debug, thiserror::Error)]
pub enum DerError {
    #[error("truncated input at offset {0}")]
    Truncated(usize),
    #[error("unexpected tag at offset {offset}: expected {expected:#x}, got {got:#x}")]
    UnexpectedTag {
        offset: usize,
        expected: u8,
        got: u8,
    },
    #[error("invalid length encoding")]
    BadLength,
}

/// DER tags we care about.
pub const TAG_INTEGER: u8 = 0x02;
pub const TAG_BIT_STRING: u8 = 0x03;
pub const TAG_OCTET_STRING: u8 = 0x04;
pub const TAG_NULL: u8 = 0x05;
pub const TAG_OID: u8 = 0x06;
pub const TAG_SEQUENCE: u8 = 0x30;
pub const TAG_SET: u8 = 0x31;

/// Parser state: a slice of bytes and a cursor.
pub struct Reader<'a> {
    pub buf: &'a [u8],
    pub pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    pub fn at_end(&self) -> bool {
        self.pos >= self.buf.len()
    }

    fn read_byte(&mut self) -> Result<u8, DerError> {
        if self.pos >= self.buf.len() {
            return Err(DerError::Truncated(self.pos));
        }
        let b = self.buf[self.pos];
        self.pos += 1;
        Ok(b)
    }

    fn read_length(&mut self) -> Result<usize, DerError> {
        let first = self.read_byte()?;
        if first & 0x80 == 0 {
            return Ok(first as usize);
        }
        let n = (first & 0x7F) as usize;
        if n == 0 || n > 4 {
            return Err(DerError::BadLength);
        }
        let mut len = 0usize;
        for _ in 0..n {
            let b = self.read_byte()?;
            len = (len << 8) | (b as usize);
        }
        Ok(len)
    }

    /// Read a TLV: returns (tag, content_slice) without copying.
    pub fn read_tlv(&mut self) -> Result<(u8, &'a [u8]), DerError> {
        let tag = self.read_byte()?;
        let len = self.read_length()?;
        if self.pos + len > self.buf.len() {
            return Err(DerError::Truncated(self.pos));
        }
        let content = &self.buf[self.pos..self.pos + len];
        self.pos += len;
        Ok((tag, content))
    }

    /// Expect a specific tag; return the content slice on success.
    pub fn expect(&mut self, expected: u8) -> Result<&'a [u8], DerError> {
        let pos = self.pos;
        let (tag, content) = self.read_tlv()?;
        if tag != expected {
            return Err(DerError::UnexpectedTag {
                offset: pos,
                expected,
                got: tag,
            });
        }
        Ok(content)
    }

    /// Skip a TLV (any tag).
    pub fn skip_tlv(&mut self) -> Result<(), DerError> {
        self.read_tlv().map(|_| ())
    }
}

/// Extract `param_d` (private key OCTET STRING) from a DER-encoded
/// DstuPrivkey structure. Returns the raw bytes (little-endian).
///
/// DstuPrivkey ::= SEQUENCE {
///     version    INTEGER,
///     priv0      SEQUENCE { ... },
///     param_d    OCTET STRING,         -- THE prize
///     attr       [0] IMPLICIT ... OPTIONAL
/// }
/// Extract param_d from a PKCS#8-wrapped DSTU 4145 private key.
///
/// ```text
/// PKCS8 ::= SEQUENCE {
///     version    INTEGER,
///     algId      SEQUENCE { ... },
///     privkey    OCTET STRING  ← contains DstuPrivkey DER
///     attributes [0] IMPLICIT ... OPTIONAL
/// }
/// ```
pub fn extract_param_d_from_pkcs8(pkcs8: &[u8]) -> Result<Vec<u8>, DerError> {
    let mut r = Reader::new(pkcs8);
    let outer = r.expect(TAG_SEQUENCE)?;
    let mut inner = Reader::new(outer);
    inner.expect(TAG_INTEGER)?; // skip version
    inner.expect(TAG_SEQUENCE)?; // skip algId
    let privkey_der = inner.expect(TAG_OCTET_STRING)?;
    extract_param_d(privkey_der)
}

pub fn extract_param_d(der: &[u8]) -> Result<Vec<u8>, DerError> {
    let mut r = Reader::new(der);
    let outer = r.expect(TAG_SEQUENCE)?;
    let mut inner = Reader::new(outer);
    // Skip version INTEGER
    inner.expect(TAG_INTEGER)?;
    // Skip priv0 SEQUENCE
    inner.expect(TAG_SEQUENCE)?;
    // Read param_d OCTET STRING
    let d_bytes = inner.expect(TAG_OCTET_STRING)?;
    Ok(d_bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_simple_sequence() {
        // SEQUENCE { INTEGER 5, OCTET STRING "abc" }
        // Content: 02 01 05 + 04 03 61 62 63 = 8 bytes
        let data = &[0x30, 0x08, 0x02, 0x01, 0x05, 0x04, 0x03, b'a', b'b', b'c'];
        let mut r = Reader::new(data);
        let outer = r.expect(TAG_SEQUENCE).unwrap();
        let mut inner = Reader::new(outer);
        let int_content = inner.expect(TAG_INTEGER).unwrap();
        assert_eq!(int_content, &[5]);
        let str_content = inner.expect(TAG_OCTET_STRING).unwrap();
        assert_eq!(str_content, b"abc");
        assert!(inner.at_end());
    }

    #[test]
    fn test_long_form_length() {
        // OCTET STRING with 200-byte content (long form length: 0x81, 0xC8)
        let mut data = vec![0x04, 0x81, 0xC8];
        data.extend_from_slice(&[0xAA; 200]);
        let mut r = Reader::new(&data);
        let content = r.expect(TAG_OCTET_STRING).unwrap();
        assert_eq!(content.len(), 200);
        assert!(content.iter().all(|&b| b == 0xAA));
    }

    #[test]
    fn test_extract_param_d_synthetic() {
        // Build a minimal DstuPrivkey:
        // SEQUENCE {
        //   INTEGER 0,             -- version
        //   SEQUENCE {              -- priv0 (just OID + params, simplified to a byte stub)
        //     INTEGER 1
        //   },
        //   OCTET STRING [0xDE, 0xAD, 0xBE, 0xEF]   -- param_d
        // }
        let priv0 = &[0x30, 0x03, 0x02, 0x01, 0x01]; // SEQUENCE { INTEGER 1 }
        let param_d = &[0xDE, 0xAD, 0xBE, 0xEF];
        let mut inner = Vec::new();
        inner.extend_from_slice(&[0x02, 0x01, 0x00]); // INTEGER 0
        inner.extend_from_slice(priv0);
        inner.push(0x04); // OCTET STRING tag
        inner.push(param_d.len() as u8);
        inner.extend_from_slice(param_d);
        let mut der = Vec::new();
        der.push(0x30); // SEQUENCE
        der.push(inner.len() as u8);
        der.extend_from_slice(&inner);

        let extracted = extract_param_d(&der).unwrap();
        assert_eq!(extracted, param_d);
    }

    #[test]
    fn test_extract_from_real_jks_key() {
        use crate::interop::prro::jks::read_jks;
        let path = "/mnt/d/PRRO_GATE/key_13667753_13667753 (2).jks";
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(_) => return, // skip if no test JKS
        };
        let entry = read_jks(&data, "Jrcfyf123").unwrap();
        let d_bytes =
            extract_param_d(&entry.key_der).expect("DSTU key wrapper must parse from real JKS");
        // DSTU_PB_257 private key is 32 bytes (256 bits) plus possibly leading zero
        assert!(
            d_bytes.len() >= 32 && d_bytes.len() <= 34,
            "param_d length {} unexpected for DSTU_PB_257",
            d_bytes.len()
        );
        // Size-only — the real scalar bytes must never leave the
        // process memory via a test log.
        eprintln!("extracted param_d: {} bytes", d_bytes.len());
    }
}
