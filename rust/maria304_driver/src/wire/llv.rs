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

impl<S: Into<String>> From<Option<S>> for Llv {
    /// Lossy constructor — long strings will be rejected at `to_wire` time
    /// via `LlvTooLong`.  Prefer [`Llv::new`] or [`Llv::from_opt`] for
    /// explicit error handling.
    fn from(value: Option<S>) -> Self {
        match value {
            Some(v) => {
                let s: String = v.into();
                Self(Some(s))
            }
            None => Self::null(),
        }
    }
}

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
        assert_eq!(composed, "10MERCHANT4207TERM00116411111******111112234567890123");
    }
}
