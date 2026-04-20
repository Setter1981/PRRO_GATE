//! `SOFT*` error code catalogue.
//!
//! The EKKR firmware reports every logical failure via a plain-text
//! response frame whose payload starts with one of the `SOFT…`
//! identifiers documented in §18 of the official protocol PDF.  Each
//! identifier maps to a specific failure class; 1C's `ФискальныйРегистратор`
//! module uses them to decide whether the operation is retryable.
//!
//! This catalogue is deliberately *typed* — the dispatcher hands
//! `ErrorCode` values to the response builder, which formats them back
//! to their on-wire representation.  That round-trip keeps
//! stringly-typed error handling out of the session layer and makes
//! the mapping from bridge-returned HTTP errors to Maria wire codes an
//! explicit table rather than ad-hoc formatting.

use std::fmt;

/// A typed Maria wire error code.
///
/// Variants correspond 1:1 to `SOFT*` identifiers returned by real
/// firmware.  `Custom` covers vendor-specific extensions that real
/// firmware may emit but the protocol PDF does not enumerate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorCode {
    /// Block is logically inconsistent with current device state; the
    /// client should retry the higher-level operation from scratch.
    SoftBlock,
    /// Bad CRC in an incoming frame (response to CRC-protected command).
    SoftBadCs,
    /// Password rejected by `UPAS`.
    SoftUpas,
    /// Article code out of range (`FISC` with article >15516 etc.).
    SoftBadArt,
    /// Article parameters differ from a previous use within the same
    /// shift — the `FISC` parameters for an already-activated code are
    /// immutable until a resetting Z-report.
    SoftDifArt,
    /// Device is operating in training mode (no registration) — `NREP`
    /// and similar fiscal commands short-circuit.
    SoftRegist,
    /// Generic receipt-level command rejection (invalid totals,
    /// inconsistent line data, etc.).
    SoftCheck,
    /// Low-level hardware reports a paper/printer fault.  Rare in
    /// a virtual driver; surfaced only by the bridge on rare paths.
    SoftPrnErr,
    /// Command requires an opened fiscal receipt; none is present.
    SoftNoDoc,
    /// Command is not permitted in the current "system key" position
    /// (see `SVSL`).
    SoftKey,
    /// Operator tried a command that requires service-mode access.
    SoftSvc,
    /// Offline / KSEF buffer is full; further offline receipts are
    /// blocked until settlement.
    SoftOfflBufFull,
    /// Duplicate `offline_fiscal_no` — the bridge emits this when an
    /// `idempotency_key` collides with an existing document in a way
    /// that indicates client-side replay without state reset.
    SoftOfflDup,
    /// Device is locked (service-limit, day-limit, crypto issue).
    SoftLocked,
    /// Vendor-specific or not-yet-enumerated wire code.
    Custom(&'static str),
}

impl ErrorCode {
    /// Wire identifier — what goes inside the response frame payload.
    ///
    /// The payload for an error frame is just this identifier (4–10
    /// ASCII bytes), framing is added by the wire codec.
    #[must_use]
    pub fn as_wire(&self) -> &'static str {
        match self {
            Self::SoftBlock => "SOFTBLOCK",
            Self::SoftBadCs => "SOFTBADCS",
            Self::SoftUpas => "SOFTUPAS",
            Self::SoftBadArt => "SOFTBADART",
            Self::SoftDifArt => "SOFTDIFART",
            Self::SoftRegist => "SOFTREGIST",
            Self::SoftCheck => "SOFTCHECK",
            Self::SoftPrnErr => "SOFTPRNERR",
            Self::SoftNoDoc => "SOFTNODOC",
            Self::SoftKey => "SOFTKEY",
            Self::SoftSvc => "SOFTSVC",
            Self::SoftOfflBufFull => "SOFTOFFLBF",
            Self::SoftOfflDup => "SOFTOFFLDP",
            Self::SoftLocked => "SOFTLOCKED",
            Self::Custom(s) => s,
        }
    }

    /// Parse a wire identifier back into a typed `ErrorCode`.
    ///
    /// Returns `None` when the identifier is empty or shorter than the
    /// 4-char `SOFT` prefix.  Unknown but well-formed identifiers are
    /// accepted as [`ErrorCode::Custom`].
    #[must_use]
    pub fn parse(wire: &str) -> Option<Self> {
        if wire.len() < 4 {
            return None;
        }
        let known: &[(ErrorCode, &str)] = &[
            (Self::SoftBlock, "SOFTBLOCK"),
            (Self::SoftBadCs, "SOFTBADCS"),
            (Self::SoftUpas, "SOFTUPAS"),
            (Self::SoftBadArt, "SOFTBADART"),
            (Self::SoftDifArt, "SOFTDIFART"),
            (Self::SoftRegist, "SOFTREGIST"),
            (Self::SoftCheck, "SOFTCHECK"),
            (Self::SoftPrnErr, "SOFTPRNERR"),
            (Self::SoftNoDoc, "SOFTNODOC"),
            (Self::SoftKey, "SOFTKEY"),
            (Self::SoftSvc, "SOFTSVC"),
            (Self::SoftOfflBufFull, "SOFTOFFLBF"),
            (Self::SoftOfflDup, "SOFTOFFLDP"),
            (Self::SoftLocked, "SOFTLOCKED"),
        ];
        for (code, id) in known {
            if wire == *id {
                return Some(code.clone());
            }
        }
        // Unknown SOFT* variant — keep as Custom so the session layer
        // can still react meaningfully.
        // The leaked `&'static str` allocation is one-per-unknown-code
        // and survives the process lifetime — acceptable for an edge
        // path that should be rare.
        Some(Self::Custom(Box::leak(wire.to_string().into_boxed_str())))
    }

    /// Whether the higher-level caller should consider the underlying
    /// operation transient (retry is meaningful) or terminal (operator
    /// attention required).
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::SoftBlock | Self::SoftPrnErr)
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_enumerated_variant_roundtrips_via_wire_string() {
        let all = [
            ErrorCode::SoftBlock,
            ErrorCode::SoftBadCs,
            ErrorCode::SoftUpas,
            ErrorCode::SoftBadArt,
            ErrorCode::SoftDifArt,
            ErrorCode::SoftRegist,
            ErrorCode::SoftCheck,
            ErrorCode::SoftPrnErr,
            ErrorCode::SoftNoDoc,
            ErrorCode::SoftKey,
            ErrorCode::SoftSvc,
            ErrorCode::SoftOfflBufFull,
            ErrorCode::SoftOfflDup,
            ErrorCode::SoftLocked,
        ];
        for code in all {
            let wire = code.as_wire();
            let parsed = ErrorCode::parse(wire).unwrap();
            assert_eq!(parsed, code, "roundtrip drift for {wire}");
        }
    }

    #[test]
    fn wire_identifiers_are_all_distinct() {
        let all = [
            "SOFTBLOCK", "SOFTBADCS", "SOFTUPAS", "SOFTBADART",
            "SOFTDIFART", "SOFTREGIST", "SOFTCHECK", "SOFTPRNERR",
            "SOFTNODOC", "SOFTKEY", "SOFTSVC", "SOFTOFFLBF",
            "SOFTOFFLDP", "SOFTLOCKED",
        ];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a, b, "duplicate wire identifier: {a}");
            }
        }
    }

    #[test]
    fn wire_identifiers_start_with_soft_prefix() {
        // Firmware convention — all logical error codes use the `SOFT`
        // prefix.  `Custom` is allowed to deviate but enumerated
        // variants must not.
        let all = [
            ErrorCode::SoftBlock,
            ErrorCode::SoftBadCs,
            ErrorCode::SoftUpas,
            ErrorCode::SoftBadArt,
            ErrorCode::SoftDifArt,
            ErrorCode::SoftRegist,
            ErrorCode::SoftCheck,
            ErrorCode::SoftPrnErr,
            ErrorCode::SoftNoDoc,
            ErrorCode::SoftKey,
            ErrorCode::SoftSvc,
            ErrorCode::SoftOfflBufFull,
            ErrorCode::SoftOfflDup,
            ErrorCode::SoftLocked,
        ];
        for code in all {
            assert!(code.as_wire().starts_with("SOFT"), "{code:?}");
        }
    }

    #[test]
    fn parse_rejects_inputs_shorter_than_four_chars() {
        assert_eq!(ErrorCode::parse(""), None);
        assert_eq!(ErrorCode::parse("x"), None);
        assert_eq!(ErrorCode::parse("SOF"), None);
    }

    #[test]
    fn parse_accepts_unknown_variants_as_custom() {
        let got = ErrorCode::parse("SOFTFOOBAR").unwrap();
        match got {
            ErrorCode::Custom(s) => assert_eq!(s, "SOFTFOOBAR"),
            other => panic!("expected Custom, got {other:?}"),
        }
    }

    #[test]
    fn display_uses_wire_identifier() {
        assert_eq!(format!("{}", ErrorCode::SoftBlock), "SOFTBLOCK");
        assert_eq!(format!("{}", ErrorCode::Custom("SOFTX")), "SOFTX");
    }

    #[test]
    fn retryable_classification_matches_intent() {
        assert!(ErrorCode::SoftBlock.is_retryable());
        assert!(ErrorCode::SoftPrnErr.is_retryable());
        // Everything else is terminal — operator must fix data.
        for code in [
            ErrorCode::SoftBadCs,
            ErrorCode::SoftUpas,
            ErrorCode::SoftBadArt,
            ErrorCode::SoftDifArt,
            ErrorCode::SoftRegist,
            ErrorCode::SoftCheck,
            ErrorCode::SoftNoDoc,
            ErrorCode::SoftKey,
            ErrorCode::SoftSvc,
            ErrorCode::SoftOfflBufFull,
            ErrorCode::SoftOfflDup,
            ErrorCode::SoftLocked,
        ] {
            assert!(!code.is_retryable(), "{code:?} should be terminal");
        }
    }
}
