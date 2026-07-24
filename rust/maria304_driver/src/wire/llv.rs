//! LLV — Length-prefixed ASCII-decimal string encoding.
//!
//! Format (verbatim from decompiled `Resonance.Internal.LLV.ToString`):
//!
//! ```text
//!   <2-digit decimal length><value bytes>
//! ```
//!
//! * `null` or empty value encodes as the literal `"00"`.
//! * Maximum length is 99 characters.  Longer inputs are rejected —
//!   matching `ArgumentOutOfRangeException` in the .NET impl.
//!
//! Used as a building block for composite commands such as `PSDt`
//! (payment terminal slip) and `ACLD` (excise stamps).

use std::fmt;

/// Length-prefixed value used inside Maria wire commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Llv(Option<String>);

/// Error returned when an LLV value exceeds the protocol limit of 99 chars.
#[derive(Debug, thiserror::Error)]
#[error("LLV value exceeds 99 chars (got {0})")]
pub struct LlvTooLong(pub usize);

impl Llv {
    /// Construct an LLV from an owned string.
    ///
    /// # Errors
    /// Returns [`LlvTooLong`] if the value is longer than 99 characters.
    pub fn new(value: impl Into<String>) -> Result<Self, LlvTooLong> {
        let s = value.into();
        if s.len() > 99 {
            return Err(LlvTooLong(s.len()));
        }
        Ok(Self(Some(s)))
    }

    /// Empty/null LLV — encodes as `"00"`.
    #[must_use]
    pub fn null() -> Self {
        Self(None)
    }

    /// Convenience constructor from `Option<impl Into<String>>`.
    ///
    /// # Errors
    /// Forwards [`LlvTooLong`] from [`Llv::new`].
    pub fn from_opt(value: Option<impl Into<String>>) -> Result<Self, LlvTooLong> {
        match value {
            Some(v) => Self::new(v),
            None => Ok(Self::null()),
        }
    }

    /// Serialize to the wire encoding.
    #[must_use]
    pub fn to_wire(&self) -> String {
        match &self.0 {
            Some(s) => format!("{:02}{s}", s.len()),
            None => "00".to_string(),
        }
    }
}

impl fmt::Display for Llv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_wire())
    }
}

// NOTE: intentionally NO `From<Option<S>> for Llv` impl.
// A lossy `From` would bypass the 99-char length check, and `to_wire()`
// would silently serialize an invalid frame (length byte becomes 3 digits,
// misaligning the downstream decoder).  Callers must go through
// [`Llv::new`] or [`Llv::from_opt`] which return `Result`.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_encodes_as_double_zero() {
        assert_eq!(Llv::null().to_wire(), "00");
    }

    #[test]
    fn from_opt_none_is_null() {
        let v: Llv = Llv::from_opt(None::<&str>).unwrap();
        assert_eq!(v.to_wire(), "00");
    }

    #[test]
    fn empty_string_encodes_as_zero_length() {
        let v = Llv::new("").unwrap();
        assert_eq!(v.to_wire(), "00");
    }

    #[test]
    fn short_string_is_prefixed_with_two_digit_length() {
        let v = Llv::new("RRN12345").unwrap();
        assert_eq!(v.to_wire(), "08RRN12345");
    }

    #[test]
    fn length_is_zero_padded() {
        let v = Llv::new("X").unwrap();
        assert_eq!(v.to_wire(), "01X");
    }

    #[test]
    fn max_length_99_is_accepted() {
        let s = "x".repeat(99);
        let v = Llv::new(&s).unwrap();
        let wire = v.to_wire();
        assert_eq!(&wire[..2], "99");
        assert_eq!(wire.len(), 2 + 99);
    }

    #[test]
    fn length_100_is_rejected() {
        let s = "x".repeat(100);
        let err = Llv::new(s).unwrap_err();
        assert_eq!(err.0, 100);
    }

    #[test]
    fn psd_slip_composite_matches_reference() {
        // A realistic acquirer slip — merchant, terminal, pan, rrn
        // — stitched in PSDt body order (only the LLV parts here).
        let merchant = Llv::new("MERCHANT42").unwrap();
        let terminal = Llv::new("TERM001").unwrap();
        let pan = Llv::new("411111******1111").unwrap();
        let rrn = Llv::new("234567890123").unwrap();
        let composed = format!("{merchant}{terminal}{pan}{rrn}");
        assert_eq!(
            composed,
            "10MERCHANT4207TERM00116411111******111112234567890123"
        );
    }

    #[test]
    fn display_trait_matches_to_wire() {
        // `to_wire` is the single source of truth; Display must go
        // through it so `format!("{llv}")` and `llv.to_wire()` never
        // diverge.
        for raw in ["", "x", "RRN123", "хост"] {
            let v = Llv::new(raw).unwrap();
            assert_eq!(format!("{v}"), v.to_wire());
        }
    }

    #[test]
    fn from_opt_some_validates_length() {
        let err = Llv::from_opt(Some("z".repeat(100))).unwrap_err();
        assert_eq!(err.0, 100);
    }

    #[test]
    fn equality_follows_content() {
        assert_eq!(Llv::new("abc").unwrap(), Llv::new("abc").unwrap());
        assert_ne!(Llv::new("abc").unwrap(), Llv::new("abd").unwrap());
        assert_ne!(Llv::new("").unwrap(), Llv::null());
    }

    #[test]
    fn len_2_digit_is_padded_for_all_lengths_1_to_9() {
        for (raw, expected_prefix) in [
            ("a", "01"),
            ("ab", "02"),
            ("abc", "03"),
            ("abcd", "04"),
            ("abcde", "05"),
            ("abcdef", "06"),
            ("abcdefg", "07"),
            ("abcdefgh", "08"),
            ("abcdefghi", "09"),
            ("abcdefghij", "10"),
        ] {
            let v = Llv::new(raw).unwrap().to_wire();
            assert!(
                v.starts_with(expected_prefix),
                "expected prefix {expected_prefix:?} for {raw:?}, got {v:?}",
            );
        }
    }

    // No `From<Option<S>>` impl exists (by design — see note in llv.rs).
    // This test freezes that as an invariant: attempting a lossy `.into()`
    // must fail to compile.  We can't easily express "negative compile"
    // in plain `#[test]`, so instead assert that the only legal path
    // produces a `Result` typed value.
    #[test]
    fn only_fallible_constructors_exist() {
        let _: Result<Llv, LlvTooLong> = Llv::new("x");
        let _: Result<Llv, LlvTooLong> = Llv::from_opt(Some("x"));
        let _: Result<Llv, LlvTooLong> = Llv::from_opt(None::<&str>);
        // Llv::null() is the only infallible entry, and it cannot
        // produce an out-of-range value by construction.
        let _: Llv = Llv::null();
    }
}
