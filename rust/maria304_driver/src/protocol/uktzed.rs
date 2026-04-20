//! UKTZED prefix helper.
//!
//! The OLE Manager convention for passing UKTZED (Ukrainian tariff
//! code) alongside an item name is to prefix the name with
//! `"NNNNNNNNNN#"` — a 10-digit tariff code followed by `#`.  The
//! decompiled `Resonance.Internal.maria_internal.fiscalLine` comment
//! says verbatim:
//!
//! > "Є можливість задати код УКТЗЕД в команді: для цього назва
//! > товару повинна починатись з префіксу вигляду 'NNNNNNNNNN#'"
//!
//! Our pilot 1C sample uses exactly this format.  Python adapter
//! (M7) splits on first `#` — this Rust helper exposes the same
//! semantics for in-Rust consumers (M10 admin / capture dumps) and
//! gives us a clean unit-testable surface.

/// Extracted UKTZED + clean item name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UktzedSplit<'a> {
    /// Tariff code — always exactly 10 ASCII digits if present.
    pub uktzed: Option<&'a str>,
    /// Item name with the `"NNNNNNNNNN#"` prefix stripped.
    pub name: &'a str,
}

/// Parse `"NNNNNNNNNN#name"` into `(uktzed, name)`, falling back to
/// `(None, whole)` for unprefixed inputs.  Cheap — O(20) bytes at
/// most, zero allocations.
#[must_use]
pub fn split_uktzed_prefix(raw: &str) -> UktzedSplit<'_> {
    // Fast path: first 11 bytes must be ASCII digits followed by '#'.
    let bytes = raw.as_bytes();
    if bytes.len() < 11 || bytes[10] != b'#' {
        return UktzedSplit { uktzed: None, name: raw };
    }
    if !bytes[..10].iter().all(u8::is_ascii_digit) {
        return UktzedSplit { uktzed: None, name: raw };
    }
    UktzedSplit {
        uktzed: Some(&raw[..10]),
        name: &raw[11..],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_name_has_no_uktzed() {
        let s = split_uktzed_prefix("Паляниця");
        assert_eq!(s.uktzed, None);
        assert_eq!(s.name, "Паляниця");
    }

    #[test]
    fn well_formed_prefix_is_extracted() {
        let s = split_uktzed_prefix("1234567890#Паляниця");
        assert_eq!(s.uktzed, Some("1234567890"));
        assert_eq!(s.name, "Паляниця");
    }

    #[test]
    fn nine_digits_plus_hash_is_not_a_match() {
        // Must be exactly 10 digits.
        let s = split_uktzed_prefix("123456789#Name");
        assert_eq!(s.uktzed, None);
        assert_eq!(s.name, "123456789#Name");
    }

    #[test]
    fn eleven_digits_without_hash_is_not_a_match() {
        let s = split_uktzed_prefix("12345678901Name");
        assert_eq!(s.uktzed, None);
    }

    #[test]
    fn ten_non_digit_chars_plus_hash_is_not_a_match() {
        // The `#` is at position 10 but the prefix isn't digits.
        let s = split_uktzed_prefix("АБВГДЕЖЗИJ#Name");
        assert_eq!(s.uktzed, None);
    }

    #[test]
    fn empty_name_after_prefix_is_still_valid() {
        let s = split_uktzed_prefix("1234567890#");
        assert_eq!(s.uktzed, Some("1234567890"));
        assert_eq!(s.name, "");
    }

    #[test]
    fn cyrillic_item_with_uktzed_preserves_name_intact() {
        // UKTZED+Cyrillic — the pilot 1C case.  The byte-offset
        // slicing must not split a UTF-8 boundary.
        let s = split_uktzed_prefix("4813201200#Цигарки L&M");
        assert_eq!(s.uktzed, Some("4813201200"));
        assert_eq!(s.name, "Цигарки L&M");
    }

    #[test]
    fn multiple_hashes_only_first_delimits() {
        let s = split_uktzed_prefix("1234567890#Goods#with#hash");
        assert_eq!(s.uktzed, Some("1234567890"));
        assert_eq!(s.name, "Goods#with#hash");
    }

    #[test]
    fn short_input_is_safe_without_panic() {
        for raw in ["", "1", "12#", "1234567890"] {
            let s = split_uktzed_prefix(raw);
            assert_eq!(s.uktzed, None, "{raw:?}");
            assert_eq!(s.name, raw);
        }
    }
}
