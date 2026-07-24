//! CP866 encoding for Maria wire commands.
//!
//! The Resonance firmware speaks a Cyrillic-capable 8-bit encoding where
//! the lower 128 code-points are ASCII and the upper half matches
//! IBM-866 ("alternative Russian"), with a few Ukrainian-specific slots.
//!
//! The .NET implementation uses a dedicated `Resonance_CP866Encoding`
//! that extends IBM-866 with Ukrainian glyphs (`і`, `ї`, `є`, `ґ` and
//! their upper-case variants).  We do the same manually — bringing in
//! a full `encoding_rs` dependency just for this would be heavier than
//! the actual table.
//!
//! Only the characters that can legitimately appear in receipt item
//! names, department labels, cashier names and error messages are
//! mapped.  Anything else is replaced with `?` (0x3F).
//!
//! # Unicode normalization
//!
//! The encoder expects its input in **NFC** (canonical composed form).
//! All Ukrainian and Russian glyphs in this table are single code-points;
//! a decomposed `и + U+0306` (combining breve) sequence would be treated
//! as two separate characters — `и` encodes correctly, the combining
//! mark falls through to `?`.  The Resonance OLE Manager DLL emits NFC
//! exclusively, so this matches production behaviour.  Callers that
//! bridge from non-NFC sources must normalise upstream.

/// Encode a Unicode string into a Maria-compatible CP866 byte sequence.
///
/// Unknown glyphs are substituted with `?` (0x3F) to keep the output
/// length predictable — the firmware accepts any ASCII-printable
/// fallback.
///
/// Input must be in Unicode NFC (canonical composed form); see module
/// docs.
#[must_use]
pub fn encode(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for ch in s.chars() {
        out.push(encode_char(ch));
    }
    out
}

/// Decode a Maria CP866 byte slice back to Unicode.
#[must_use]
pub fn decode(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| decode_byte(b)).collect()
}

fn encode_char(ch: char) -> u8 {
    // ASCII — straight mapping.
    if (ch as u32) < 0x80 {
        return ch as u8;
    }
    match ch {
        // IBM-866 upper half: Russian capitals А..П at 0x80..0x8F
        'А' => 0x80,
        'Б' => 0x81,
        'В' => 0x82,
        'Г' => 0x83,
        'Д' => 0x84,
        'Е' => 0x85,
        'Ж' => 0x86,
        'З' => 0x87,
        'И' => 0x88,
        'Й' => 0x89,
        'К' => 0x8A,
        'Л' => 0x8B,
        'М' => 0x8C,
        'Н' => 0x8D,
        'О' => 0x8E,
        'П' => 0x8F,
        // 0x90..0x9F = Р..Я
        'Р' => 0x90,
        'С' => 0x91,
        'Т' => 0x92,
        'У' => 0x93,
        'Ф' => 0x94,
        'Х' => 0x95,
        'Ц' => 0x96,
        'Ч' => 0x97,
        'Ш' => 0x98,
        'Щ' => 0x99,
        'Ъ' => 0x9A,
        'Ы' => 0x9B,
        'Ь' => 0x9C,
        'Э' => 0x9D,
        'Ю' => 0x9E,
        'Я' => 0x9F,
        // 0xA0..0xAF = а..п
        'а' => 0xA0,
        'б' => 0xA1,
        'в' => 0xA2,
        'г' => 0xA3,
        'д' => 0xA4,
        'е' => 0xA5,
        'ж' => 0xA6,
        'з' => 0xA7,
        'и' => 0xA8,
        'й' => 0xA9,
        'к' => 0xAA,
        'л' => 0xAB,
        'м' => 0xAC,
        'н' => 0xAD,
        'о' => 0xAE,
        'п' => 0xAF,
        // 0xE0..0xEF = р..я
        'р' => 0xE0,
        'с' => 0xE1,
        'т' => 0xE2,
        'у' => 0xE3,
        'ф' => 0xE4,
        'х' => 0xE5,
        'ц' => 0xE6,
        'ч' => 0xE7,
        'ш' => 0xE8,
        'щ' => 0xE9,
        'ъ' => 0xEA,
        'ы' => 0xEB,
        'ь' => 0xEC,
        'э' => 0xED,
        'ю' => 0xEE,
        'я' => 0xEF,
        // Russian Ё / ё
        'Ё' => 0xF0,
        'ё' => 0xF1,
        // Ukrainian-specific (positions match IBM-866 "Cyrillic+" layout
        // used by the Resonance firmware).
        'Є' => 0xF2,
        'є' => 0xF3,
        'Ї' => 0xF4,
        'ї' => 0xF5,
        'Ў' => 0xF6,
        'ў' => 0xF7,
        '°' => 0xF8,
        '·' => 0xFA,
        'І' => 0xB8,
        'і' => 0xB9,
        'Ґ' => 0xBA,
        'ґ' => 0xBB,
        // Numeric / punctuation pass-through is ASCII already; anything
        // unhandled becomes '?' so downstream lengths stay predictable.
        _ => b'?',
    }
}

fn decode_byte(b: u8) -> char {
    if b < 0x80 {
        return b as char;
    }
    match b {
        0x80..=0x8F => [
            'А', 'Б', 'В', 'Г', 'Д', 'Е', 'Ж', 'З', 'И', 'Й', 'К', 'Л', 'М', 'Н', 'О', 'П',
        ][(b - 0x80) as usize],
        0x90..=0x9F => [
            'Р', 'С', 'Т', 'У', 'Ф', 'Х', 'Ц', 'Ч', 'Ш', 'Щ', 'Ъ', 'Ы', 'Ь', 'Э', 'Ю', 'Я',
        ][(b - 0x90) as usize],
        0xA0..=0xAF => [
            'а', 'б', 'в', 'г', 'д', 'е', 'ж', 'з', 'и', 'й', 'к', 'л', 'м', 'н', 'о', 'п',
        ][(b - 0xA0) as usize],
        0xE0..=0xEF => [
            'р', 'с', 'т', 'у', 'ф', 'х', 'ц', 'ч', 'ш', 'щ', 'ъ', 'ы', 'ь', 'э', 'ю', 'я',
        ][(b - 0xE0) as usize],
        0xF0 => 'Ё',
        0xF1 => 'ё',
        0xF2 => 'Є',
        0xF3 => 'є',
        0xF4 => 'Ї',
        0xF5 => 'ї',
        0xF6 => 'Ў',
        0xF7 => 'ў',
        0xB8 => 'І',
        0xB9 => 'і',
        0xBA => 'Ґ',
        0xBB => 'ґ',
        _ => '?',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_roundtrip() {
        let s = "PREP1";
        assert_eq!(encode(s), b"PREP1");
        assert_eq!(decode(b"PREP1"), "PREP1");
    }

    #[test]
    fn russian_roundtrip() {
        let s = "Касир";
        let bytes = encode(s);
        // Касир = К(0x8A) а(0xA0) с(0xE1) и(0xA8) р(0xE0)
        assert_eq!(bytes, vec![0x8A, 0xA0, 0xE1, 0xA8, 0xE0]);
        assert_eq!(decode(&bytes), s);
    }

    #[test]
    fn ukrainian_glyphs_have_stable_byte_values() {
        // Byte-level table — if any of these change, downstream firmware
        // will mis-render the chars on the receipt.  Pin to concrete values.
        let cases: &[(char, u8)] = &[
            ('І', 0xB8),
            ('і', 0xB9),
            ('Ґ', 0xBA),
            ('ґ', 0xBB),
            ('Є', 0xF2),
            ('є', 0xF3),
            ('Ї', 0xF4),
            ('ї', 0xF5),
        ];
        for &(ch, expected) in cases {
            let s = ch.to_string();
            let bytes = encode(&s);
            assert_eq!(
                bytes,
                vec![expected],
                "{ch} → expected 0x{expected:02X}, got {bytes:02X?}"
            );
            assert_eq!(decode(&bytes), s, "decode({expected:02X}) != {ch}");
        }
    }

    #[test]
    fn unknown_glyph_falls_back_to_question_mark() {
        // Emoji, Chinese, anything outside the CP866 table.
        for s in ["𝕏", "中", "😀", "é"] {
            let encoded = encode(s);
            assert!(
                encoded.iter().all(|&b| b == b'?'),
                "unknown glyph {s:?} encoded to unexpected bytes {encoded:02X?}"
            );
        }
    }

    // Byte-exact reference vector for a mixed Ukrainian sentence.
    // Guards against any accidental reshuffling of the encoding table.
    #[test]
    fn mixed_ukrainian_sentence_matches_byte_reference() {
        let s = "Ґроно ягід";
        let encoded = encode(s);
        assert_eq!(
            encoded,
            vec![
                0xBA, // Ґ
                0xE0, // р
                0xAE, // о
                0xAD, // н
                0xAE, // о
                0x20, // space
                0xEF, // я
                0xA3, // г
                0xB9, // і
                0xA4, // д
            ],
            "byte-level reference drift",
        );
        assert_eq!(decode(&encoded), s, "roundtrip via hand-verified bytes");
    }

    // Every lowercase Russian letter encodes to the expected IBM-866 slot.
    #[test]
    fn russian_lowercase_slot_positions() {
        let russian_lower = "абвгдежзийклмнопрстуфхцчшщъыьэюя";
        let expected: Vec<u8> = (0xA0..=0xAF).chain(0xE0..=0xEF).collect();
        assert_eq!(encode(russian_lower), expected);
    }

    // NFC contract: precomposed `й` (U+0439) encodes as one byte;
    // decomposed `и` + U+0306 is treated as two characters, with the
    // combining mark falling through to `?`.  This documents the
    // expectation explicitly so any regression flips the test.
    #[test]
    fn nfc_precomposed_yi_encodes_as_single_byte() {
        let nfc = "й"; // precomposed U+0439
        assert_eq!(encode(nfc), vec![0xA9]);
    }

    #[test]
    fn nfd_decomposed_form_loses_combining_mark() {
        // NFD form: base `и` + combining breve U+0306.
        let nfd = "\u{0438}\u{0306}";
        assert_eq!(encode(nfd), vec![0xA8, b'?']);
    }

    #[test]
    fn bytes_never_collide_with_frame_markers_in_cyrillic() {
        // 0xFD and 0xFE must NEVER be produced for valid cyrillic — the
        // framer replaces them if they appear, but the encoder itself
        // should never need to emit those bytes for any Ukrainian letter.
        let chars = "АБВГДЕЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯЁЄЇІабвгдежзийклмнопрстуфхцчшщъыьэюяёєїі";
        for b in encode(chars) {
            assert!(
                b != 0xFD && b != 0xFE,
                "byte {b:02X} collides with frame markers"
            );
        }
    }
}
