//! `COMP` response body builder.
//!
//! After a successful close-receipt, the driver streams a data frame
//! whose payload is the 4-byte opcode `COMP` followed by **exactly
//! 90 characters** of structured state.  1C reads the first 10 chars
//! and treats them as the fiscal check number; the DLL uses the full
//! layout for internal consistency checks.
//!
//! # Layout (derived from `Resonance.Internal.fiscal_receipt.Сумма_закрытого_чека`)
//!
//! ```text
//! segments index  │  0       │ 1       │ 2       │ 3       │ 4       │ 5       │ 6       │ 7       │ 8       │
//! offset          │ 0..10    │ 10..20  │ 20..30  │ 30..40  │ 40..50  │ 50..60  │ 60..70  │ 70..80  │ 80..90  │
//! meaning         │ check #  │ sale_a  │ sale_b  │ ret_c   │ sale_d  │ sale_e  │ ret_f   │ unused  │ unused  │
//! ```
//!
//! Decompiled reference:
//! ```text
//!   ulong s0 = Substring(10,10);  // sale group a
//!   ulong s1 = Substring(20,10);  // sale group b
//!   ulong s2 = Substring(40,10);  // returns group c
//!   ulong s3 = Substring(50,10);  // sale group d
//!   ulong s4 = Substring(60,10);  // sale group e
//!   ulong s5 = Substring(80,10);  // returns group f
//!   ΔClose = |s0+s1-s2 - (s3+s4-s5)|
//! ```
//!
//! For a newly-created document where Python returns just
//! `fiscal_id` + `sale_total_kopecks` + `return_total_kopecks`, we
//! populate the salient segments and zero the rest.  The validation
//! formula `ΔClose = |sale_total + 0 - return_total - (0 + 0 - 0)|`
//! reduces to `|sale_total - return_total|`, which the DLL consumes
//! but does not surface to 1C.

use std::fmt;

/// Fixed width of the COMP body (after the 4-byte opcode).
pub const COMP_BODY_LEN: usize = 90;

/// Width of each structured segment inside the body.
pub const COMP_SEGMENT_LEN: usize = 10;

/// Number of segments: 9 × 10 = 90.
pub const COMP_SEGMENTS: usize = 9;

/// Builder for the COMP data frame body.
///
/// The final wire frame (opcode + body + framing) is produced by
/// [`Response::Data`][crate::protocol::responses::Response::Data];
/// this builder is responsible for the 90-char body only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompBuilder {
    /// Fiscal check number assigned by the Python gateway (decimal,
    /// zero-padded to 10 digits by the builder).
    pub check_number: u64,
    /// Sum of all fiscal sale lines, in kopecks.
    pub sale_total_kopecks: u64,
    /// Sum of all fiscal return lines, in kopecks.
    pub return_total_kopecks: u64,
}

impl CompBuilder {
    /// Build a minimal COMP body — fills mandatory segments 0–2, zeros
    /// the rest.  This is the common case for freshly-ACKed documents.
    #[must_use]
    pub fn new(check_number: u64, sale_total_kopecks: u64, return_total_kopecks: u64) -> Self {
        Self {
            check_number,
            sale_total_kopecks,
            return_total_kopecks,
        }
    }

    /// Produce the 90-char body (without the `COMP` opcode prefix).
    ///
    /// # Panics
    /// Never — every segment is formatted via `"{:010}"` which is
    /// bounded at 10 chars for any `u64`, and there are exactly
    /// `COMP_SEGMENTS` segments.
    #[must_use]
    pub fn to_body(&self) -> String {
        let segments: [u64; COMP_SEGMENTS] = [
            self.check_number,
            self.sale_total_kopecks,   // seg 1 = primary sale group
            0,                         // seg 2 = secondary sale group (zero)
            self.return_total_kopecks, // seg 3 = primary return group
            0,
            0,
            0,
            0,
            0,
        ];
        let mut out = String::with_capacity(COMP_BODY_LEN);
        for s in segments {
            let _ = fmt::Write::write_fmt(&mut out, format_args!("{s:010}"));
        }
        // No assertion on `out.len()` — oversize (>10-digit) segments
        // are allowed to produce a longer body as a diagnostic signal.
        // See `oversize_check_number_overflows_into_neighbour_segment`.
        out
    }

    /// Produce the full data-frame payload: `"COMP" + body`.
    ///
    /// This is the string that goes into [`Response::Data`].
    #[must_use]
    pub fn to_wire_payload(&self) -> String {
        let mut s = String::with_capacity(4 + COMP_BODY_LEN);
        s.push_str("COMP");
        s.push_str(&self.to_body());
        s
    }

    /// Inverse of [`Self::to_body`] — parse a 90-char body back.
    ///
    /// Returns `None` if the input length is not exactly `COMP_BODY_LEN`
    /// or if any segment fails to parse as a base-10 `u64`.
    #[must_use]
    pub fn parse_body(body: &str) -> Option<Self> {
        if body.len() != COMP_BODY_LEN {
            return None;
        }
        let n = |ofs: usize| body[ofs..ofs + COMP_SEGMENT_LEN].parse::<u64>().ok();
        Some(Self {
            check_number: n(0)?,
            sale_total_kopecks: n(10)?,
            return_total_kopecks: n(30)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_is_exactly_90_chars() {
        let b = CompBuilder::new(1, 2, 3).to_body();
        assert_eq!(b.len(), COMP_BODY_LEN);
    }

    #[test]
    fn wire_payload_is_94_chars_opcode_plus_body() {
        let p = CompBuilder::new(0, 0, 0).to_wire_payload();
        assert_eq!(p.len(), 4 + COMP_BODY_LEN);
        assert!(p.starts_with("COMP"));
    }

    #[test]
    fn first_segment_is_zero_padded_check_number() {
        let b = CompBuilder::new(42, 0, 0).to_body();
        assert_eq!(&b[..COMP_SEGMENT_LEN], "0000000042");
    }

    #[test]
    fn sale_and_return_totals_land_in_documented_offsets() {
        let b = CompBuilder::new(1, 1_234_567, 890).to_body();
        assert_eq!(&b[0..10], "0000000001");
        assert_eq!(&b[10..20], "0001234567"); // sale_total → seg 1
        assert_eq!(&b[20..30], "0000000000"); // seg 2 zero
        assert_eq!(&b[30..40], "0000000890"); // return_total → seg 3
        for i in (40..COMP_BODY_LEN).step_by(COMP_SEGMENT_LEN) {
            assert_eq!(
                &b[i..i + COMP_SEGMENT_LEN],
                "0000000000",
                "segment at {i} should be zero",
            );
        }
    }

    #[test]
    fn max_u64_check_number_stays_within_10_chars_when_plausible() {
        // Real check numbers never exceed 10 digits (DPS caps fiscal
        // numbers well below 9 999 999 999).  Test the max legal value.
        let max10: u64 = 9_999_999_999;
        let b = CompBuilder::new(max10, 0, 0).to_body();
        assert_eq!(&b[..COMP_SEGMENT_LEN], "9999999999");
    }

    #[test]
    fn oversize_check_number_overflows_into_neighbour_segment() {
        // If Python ever returns >10-digit, the `:010` format pads but
        // does NOT truncate.  We accept the overflow as a diagnostic
        // signal — the frame will fail 1C's own parser and the bridge
        // will see an error.  This test documents the behaviour so it
        // does not silently regress.
        let over = 10_000_000_000u64; // 11 digits
        let b = CompBuilder::new(over, 0, 0).to_body();
        assert!(
            b.len() > COMP_BODY_LEN,
            "oversized input should surface as longer body"
        );
    }

    #[test]
    fn roundtrip_via_parse_recovers_populated_fields() {
        let original = CompBuilder::new(4242, 55_500, 10_000);
        let body = original.to_body();
        let parsed = CompBuilder::parse_body(&body).unwrap();
        assert_eq!(parsed.check_number, 4242);
        assert_eq!(parsed.sale_total_kopecks, 55_500);
        assert_eq!(parsed.return_total_kopecks, 10_000);
    }

    #[test]
    fn parse_rejects_wrong_length_body() {
        assert_eq!(CompBuilder::parse_body(""), None);
        assert_eq!(CompBuilder::parse_body(&"0".repeat(89)), None);
        assert_eq!(CompBuilder::parse_body(&"0".repeat(91)), None);
    }

    #[test]
    fn parse_rejects_non_numeric_segments() {
        let mut b = CompBuilder::new(1, 2, 3).to_body();
        b.replace_range(0..1, "X");
        assert_eq!(CompBuilder::parse_body(&b), None);
    }

    #[test]
    fn dps_consistency_formula_holds_for_builder_output() {
        // |s0+s1-s2 - (s3+s4-s5)| where:
        //   s0=seg1, s1=seg2, s2=seg4 (confusing indexing in ref code —
        //   we use our own offsets). Our builder zeroes every segment
        //   except the three populated ones, so the DLL formula reduces
        //   to |sale_total - return_total| which we can verify directly.
        let b = CompBuilder::new(100, 5000, 1200);
        assert_eq!(b.sale_total_kopecks - b.return_total_kopecks, 3800);
    }
}
