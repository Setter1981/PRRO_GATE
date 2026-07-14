//! Full Windows-1251 (cp1251) encoder for the canonical-XML builder
//! boundary.
//!
//! The canonical wire encoding for ФСКО receipts is cp1251.  This
//! module ships the full standard table (every defined codepoint in
//! the 0x80..0xFF range, plus ASCII identity in 0x00..0x7F) so a
//! Ukrainian-language payload (containing І / і / Ї / ї / Є / є / Ґ /
//! ґ) and any cp1251-mappable typographical character (№, ё, NBSP,
//! «», ‘ ’ “ ”, –, —, …, €, etc.) round-trips correctly.
//!
//! A character outside the cp1251 set (any emoji, CJK, hieroglyphs,
//! the few undefined slots like 0x98) returns `XmlBuildError::
//! Cp1251Unmappable(c, codepoint)` — fail-closed, no `?` replacement.

use super::XmlBuildError;

/// Encode a `&str` as a cp1251 byte stream.  ASCII is identity; the
/// Cyrillic + Ukrainian + cp1251-typography subset maps via the
/// standard Windows-1251 table; everything else is a typed error.
pub(super) fn encode(s: &str) -> Result<Vec<u8>, XmlBuildError> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        match encode_char(c) {
            Some(b) => out.push(b),
            None => return Err(XmlBuildError::Cp1251Unmappable(c, c as u32)),
        }
    }
    Ok(out)
}

/// One-codepoint cp1251 mapping.  `None` for any codepoint outside
/// the cp1251 set (callers fail closed; see `encode` above).
fn encode_char(c: char) -> Option<u8> {
    let cp = c as u32;
    match cp {
        // ASCII identity
        0x00..=0x7F => Some(cp as u8),
        // Cyrillic uppercase А..Я → 0xC0..0xDF
        0x0410..=0x042F => Some((0xC0 + (cp - 0x0410)) as u8),
        // Cyrillic lowercase а..я → 0xE0..0xFF
        0x0430..=0x044F => Some((0xE0 + (cp - 0x0430)) as u8),
        // 0x80..0xBF — punctuation / Slavic / Ukrainian extras.
        // Order: by cp1251 byte value (column on the right).
        0x0402 => Some(0x80), // Ђ
        0x0403 => Some(0x81), // Ѓ
        0x201A => Some(0x82), // ‚
        0x0453 => Some(0x83), // ѓ
        0x201E => Some(0x84), // „
        0x2026 => Some(0x85), // …
        0x2020 => Some(0x86), // †
        0x2021 => Some(0x87), // ‡
        0x20AC => Some(0x88), // €
        0x2030 => Some(0x89), // ‰
        0x0409 => Some(0x8A), // Љ
        0x2039 => Some(0x8B), // ‹
        0x040A => Some(0x8C), // Њ
        0x040C => Some(0x8D), // Ќ
        0x040B => Some(0x8E), // Ћ
        0x040F => Some(0x8F), // Џ
        0x0452 => Some(0x90), // ђ
        0x2018 => Some(0x91), // ‘
        0x2019 => Some(0x92), // ’
        0x201C => Some(0x93), // “
        0x201D => Some(0x94), // ”
        0x2022 => Some(0x95), // •
        0x2013 => Some(0x96), // –
        0x2014 => Some(0x97), // —
        // 0x98 is undefined in cp1251.
        0x2122 => Some(0x99), // ™
        0x0459 => Some(0x9A), // љ
        0x203A => Some(0x9B), // ›
        0x045A => Some(0x9C), // њ
        0x045C => Some(0x9D), // ќ
        0x045B => Some(0x9E), // ћ
        0x045F => Some(0x9F), // џ
        0x00A0 => Some(0xA0), // NBSP
        0x040E => Some(0xA1), // Ў
        0x045E => Some(0xA2), // ў
        0x0408 => Some(0xA3), // Ј
        0x00A4 => Some(0xA4), // ¤
        0x0490 => Some(0xA5), // Ґ  (Ukrainian)
        0x00A6 => Some(0xA6), // ¦
        0x00A7 => Some(0xA7), // §
        0x0401 => Some(0xA8), // Ё
        0x00A9 => Some(0xA9), // ©
        0x0404 => Some(0xAA), // Є  (Ukrainian)
        0x00AB => Some(0xAB), // «
        0x00AC => Some(0xAC), // ¬
        0x00AD => Some(0xAD), // SHY
        0x00AE => Some(0xAE), // ®
        0x0407 => Some(0xAF), // Ї  (Ukrainian)
        0x00B0 => Some(0xB0), // °
        0x00B1 => Some(0xB1), // ±
        0x0406 => Some(0xB2), // І  (Ukrainian)
        0x0456 => Some(0xB3), // і  (Ukrainian)
        0x0491 => Some(0xB4), // ґ  (Ukrainian)
        0x00B5 => Some(0xB5), // µ
        0x00B6 => Some(0xB6), // ¶
        0x00B7 => Some(0xB7), // ·
        0x0451 => Some(0xB8), // ё
        0x2116 => Some(0xB9), // №
        0x0454 => Some(0xBA), // є  (Ukrainian)
        0x00BB => Some(0xBB), // »
        0x0458 => Some(0xBC), // ј
        0x0405 => Some(0xBD), // Ѕ
        0x0455 => Some(0xBE), // ѕ
        0x0457 => Some(0xBF), // ї  (Ukrainian)
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FW-1 mutation A4 — the 17 `encode_char` match arms cargo-mutants can
    /// delete without any existing test noticing (the unit tests pin only the
    /// KEPT arms — 201C/201D/2014 etc. — and the goldens use ASCII + core
    /// Cyrillic + і/ї). Deleting an arm ⇒ that codepoint returns `None` ⇒
    /// `encode` fails with `Cp1251Unmappable` ⇒ the whole receipt build is
    /// REFUSED (false receipt-refusal). Several are common in Ukrainian fiscal
    /// text: `«` U+00AB (opening guillemet in legal company names — note the
    /// closing `»` U+00BB is a KEPT arm, so breakage is asymmetric), `°` (spirit
    /// names), `·`, plus typographic `… – ‘ ’ • „`. One round-trip pins all 17;
    /// deleting any single arm turns this test RED.
    #[test]
    fn deleted_table_arms_round_trip_to_exact_bytes() {
        let cases: &[(char, u8)] = &[
            ('\u{201E}', 0x84), // „
            ('\u{2026}', 0x85), // …
            ('\u{2030}', 0x89), // ‰
            ('\u{0409}', 0x8A), // Љ
            ('\u{2018}', 0x91), // ‘
            ('\u{2019}', 0x92), // ’
            ('\u{2022}', 0x95), // •
            ('\u{2013}', 0x96), // –
            ('\u{045B}', 0x9E), // ћ
            ('\u{045F}', 0x9F), // џ
            ('\u{045E}', 0xA2), // ў
            ('\u{0408}', 0xA3), // Ј
            ('\u{00AB}', 0xAB), // «
            ('\u{00AC}', 0xAC), // ¬
            ('\u{00AD}', 0xAD), // SHY
            ('\u{00B0}', 0xB0), // °
            ('\u{00B7}', 0xB7), // ·
        ];
        for &(c, expected) in cases {
            let out = encode(&c.to_string())
                .unwrap_or_else(|_| panic!("cp1251 must encode U+{:04X}", c as u32));
            assert_eq!(
                out,
                vec![expected],
                "U+{:04X} must map to 0x{:02X}",
                c as u32,
                expected
            );
        }
    }

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
    fn ukrainian_letters_map_to_cp1251_table() {
        // Ґ=0xA5, Є=0xAA, Ї=0xAF, І=0xB2, і=0xB3, ґ=0xB4, є=0xBA, ї=0xBF
        let out = encode("ҐЄЇІіґєї").unwrap();
        assert_eq!(out, [0xA5, 0xAA, 0xAF, 0xB2, 0xB3, 0xB4, 0xBA, 0xBF]);
    }

    #[test]
    fn numero_sign_maps_to_b9() {
        // № = U+2116 → 0xB9 in cp1251.  Common in fiscal payloads
        // (receipt header lines like "№ 12345").
        let out = encode("№42").unwrap();
        assert_eq!(out, [0xB9, b'4', b'2']);
    }

    #[test]
    fn smart_quotes_and_em_dash_map_to_cp1251() {
        // “ = 0x93, ” = 0x94, — = 0x97
        let out = encode("\u{201C}A\u{201D}\u{2014}B").unwrap();
        assert_eq!(out, [0x93, b'A', 0x94, 0x97, b'B']);
    }

    #[test]
    fn nbsp_maps_to_a0() {
        let out = encode("a\u{00A0}b").unwrap();
        assert_eq!(out, [b'a', 0xA0, b'b']);
    }

    #[test]
    fn yo_letters_map() {
        // Ё=0xA8, ё=0xB8
        let out = encode("Ёё").unwrap();
        assert_eq!(out, [0xA8, 0xB8]);
    }

    #[test]
    fn euro_sign_maps_to_88() {
        let out = encode("\u{20AC}").unwrap();
        assert_eq!(out, [0x88]);
    }

    #[test]
    fn emoji_unmappable() {
        let err = encode("😀").unwrap_err();
        assert!(matches!(err, XmlBuildError::Cp1251Unmappable(c, _) if c == '😀'));
    }

    #[test]
    fn cjk_unmappable() {
        let err = encode("中").unwrap_err();
        assert!(matches!(err, XmlBuildError::Cp1251Unmappable(_, 0x4E2D)));
    }

    #[test]
    fn empty_string_encodes_empty() {
        assert_eq!(encode("").unwrap(), Vec::<u8>::new());
    }
}
