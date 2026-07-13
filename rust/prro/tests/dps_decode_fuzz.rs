//! DECODE-FUZZ — byzantine DPS RESPONSE robustness.
//!
//! Operator directive (byzantine-DPS ROI, 2026-07-13): a live-but-broken DPS may
//! return a MALFORMED / truncated / garbage response. The gateway's response
//! DECODER must never PANIC and must turn any hostile bytes into a clean typed
//! error — never a crash, never a bogus accept. This is the last byzantine class
//! (the "жив, но отдаёт черти что" input-fuzz blind spot).
//!
//! The decode chain (repo-mapped): tonic/prost framing → `try_decode_*` status
//! dispatch → (for `ask_offline_codes` only) `extract_econtent` (a HAND-ROLLED
//! CMS / ASN.1 TLV walker in `prro_crypto`) → `parse_offline_codes_xml` (a
//! HAND-ROLLED `<ID>` XML scan). The two hand-rolled inner layers are the real
//! hostile surface — a 9-step ASN.1 walk and string-slice arithmetic — and
//! neither was previously PROPERTY-fuzzed (only hand-picked negatives + one live
//! fixture). Every raw index is bounds-checked on audit, but "looks safe" ≠
//! "proven safe against arbitrary bytes".
//!
//! These proptests hammer both layers with arbitrary `Vec<u8>` AND mutated-valid
//! inputs (byte flips of a REAL live CMS + truncation). Any panic = a
//! byzantine-response crash bug (RED, would demand a fail-closed decode fix).
//! All-green = durable robustness teeth on the decode surface.

use proptest::prelude::*;
use prro::transports::dps::dto::parse_offline_codes_xml;
use prro_crypto::cms::extract_econtent;

/// A real live DPS T=112 CMS response (3679 bytes) — the mutated-valid seed.
const LIVE_CMS: &[u8] = include_bytes!("fixtures/t112_response_data_sign.bin");

proptest! {
    // 4096 cases per property; `failure_persistence: None` because an integration
    // test has no source-adjacent lib.rs/main.rs for proptest's regression file
    // (a found failure prints its seed in the output, which is enough here).
    #![proptest_config(ProptestConfig { cases: 4096, failure_persistence: None, ..ProptestConfig::default() })]

    /// A — the CMS/ASN.1 eContent extractor must never panic on ARBITRARY bytes.
    #[test]
    fn fuzz_extract_econtent_arbitrary(bytes in prop::collection::vec(any::<u8>(), 0..4096)) {
        // Ok(econtent) or Err(Asn1Error) — the only forbidden outcome is a panic.
        let _ = extract_econtent(&bytes);
    }

    /// A′ — mutated-valid: flip up to 32 bytes of the REAL live CMS. This reaches
    /// the cross-TLV boundary checks that arbitrary bytes almost never hit — a
    /// mostly-valid DER structure with a corrupted length/tag/ordering, the
    /// classic off-by-one trigger for a hand-rolled ASN.1 walker.
    #[test]
    fn fuzz_extract_econtent_mutated_live(
        muts in prop::collection::vec((0usize..LIVE_CMS.len(), any::<u8>()), 0..32)
    ) {
        let mut b = LIVE_CMS.to_vec();
        for (i, v) in muts {
            b[i] = v;
        }
        let _ = extract_econtent(&b);
    }

    /// A″ — truncation: ANY prefix of the real CMS must not panic (a response cut
    /// off mid-TLV by a dropped connection).
    #[test]
    fn fuzz_extract_econtent_truncated(n in 0usize..=LIVE_CMS.len()) {
        let _ = extract_econtent(&LIVE_CMS[..n]);
    }

    /// B — the `<ID>` XML scanner must never panic on ARBITRARY bytes: exercises
    /// the UTF-8 rejection path AND the `str::find`-derived slice arithmetic.
    #[test]
    fn fuzz_parse_offline_codes_xml_arbitrary(bytes in prop::collection::vec(any::<u8>(), 0..4096)) {
        let _ = parse_offline_codes_xml(&bytes);
    }

    /// B′ — mutated-valid: a well-formed `<ID>`-list with random byte flips
    /// (unbalanced tags, embedded NULs, multibyte cuts, `</ID>` splices).
    #[test]
    fn fuzz_parse_offline_codes_xml_mutated_valid(
        muts in prop::collection::vec((0usize..64, any::<u8>()), 0..24)
    ) {
        let mut b = b"<C><ID>ABCDE1234</ID><ID>ZZZ99</ID><ID></ID></C>".to_vec();
        for (i, v) in muts {
            if i < b.len() {
                b[i] = v;
            }
        }
        let _ = parse_offline_codes_xml(&b);
    }
}
