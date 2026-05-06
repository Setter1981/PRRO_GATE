//! Minimal cp1251 (Windows-1251) encoder for the canonical-XML
//! builder boundary.
//!
//! Why a hand-rolled encoder instead of a crate:  the W4 builder
//! emits a fixed alphabet — ASCII for tags / attribute names /
//! numeric values, plus the small Cyrillic block (U+0410..U+044F)
//! for the device-name field and product-name strings.  Every other
//! Unicode codepoint is unexpected in our payload and MUST surface
//! as a typed error so a future "smart-quote" or emoji from a buggy
//! upstream serialiser does not silently emit `?`-replacement bytes.
//!
//! Coverage:
//!
//! - U+0000..U+007F (ASCII): identity.
//! - U+0410..U+042F (Cyrillic uppercase А..Я): 0xC0..0xDF.
//! - U+0430..U+044F (Cyrillic lowercase а..я): 0xE0..0xFF.
//! - Selected punctuation (NBSP, ё/Ё, № etc.) NOT covered.  Adding
//!   them is a one-liner if a future payload needs them; doing it
//!   only-on-demand keeps the surface small.
//!
//! Anything else returns `XmlBuildError::Cp1251Unmappable`.

use super::XmlBuildError;

/// Encode a `&str` as a cp1251 byte stream.  Returns
/// `Cp1251Unmappable` on the first character outside the supported
/// subset (see module docs for the exact range).
pub(super) fn encode(s: &str) -> Result<Vec<u8>, XmlBuildError> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        let cp = c as u32;
        let byte = match cp {
            0x00..=0x7F => cp as u8,
            // Cyrillic uppercase А..Я → 0xC0..0xDF
            0x0410..=0x042F => (0xC0 + (cp - 0x0410)) as u8,
            // Cyrillic lowercase а..я → 0xE0..0xFF
            0x0430..=0x044F => (0xE0 + (cp - 0x0430)) as u8,
            _ => return Err(XmlBuildError::Cp1251Unmappable(c, cp)),
        };
        out.push(byte);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_passthrough() {
        let out = encode("Hello, World!").unwrap();
        assert_eq!(out, b"Hello, World!");
    }

    #[test]
    fn cyrillic_uppercase_maps_to_known_bytes() {
        // А=0xC0, Б=0xC1, В=0xC2, Я=0xDF
        let out = encode("АБВЯ").unwrap();
        assert_eq!(out, [0xC0, 0xC1, 0xC2, 0xDF]);
    }

    #[test]
    fn cyrillic_lowercase_maps_to_known_bytes() {
        // а=0xE0, б=0xE1, я=0xFF
        let out = encode("абя").unwrap();
        assert_eq!(out, [0xE0, 0xE1, 0xFF]);
    }

    #[test]
    fn pro_kasa_round_trip() {
        // "ПРО_каса" — the default device_name.
        let out = encode("ПРО_каса").unwrap();
        assert_eq!(out, [0xCF, 0xD0, 0xCE, 0x5F, 0xEA, 0xE0, 0xF1, 0xE0]);
    }

    #[test]
    fn emoji_unmappable() {
        let err = encode("😀").unwrap_err();
        assert!(matches!(err, XmlBuildError::Cp1251Unmappable(c, _) if c == '😀'));
    }

    #[test]
    fn smart_quote_unmappable() {
        // U+201C LEFT DOUBLE QUOTATION MARK — common smart-quote
        // pasted from word processors; cp1251 has it at 0x93 but
        // we deliberately don't ship it in the supported subset.
        let err = encode("\u{201C}").unwrap_err();
        assert!(matches!(err, XmlBuildError::Cp1251Unmappable(_, 0x201C)));
    }

    #[test]
    fn empty_string_encodes_empty() {
        assert_eq!(encode("").unwrap(), Vec::<u8>::new());
    }
}
