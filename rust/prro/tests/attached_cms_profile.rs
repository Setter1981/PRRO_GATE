//! **W4-Z3 round-2 — non-skipping, key-free proof of the gateway ATTACHED
//! CAdES-BES signing profile** (the form DPS `sendChkV2` accepts).
//!
//! The gateway's `InProcessProvider` signs via `prro_crypto`'s
//! `CmsSigner::sign_with(.., CmsBuildOptions { attached: true, signing_time:
//! Some(now) })`.  This test drives that EXACT API with a STUB signer (so no
//! private key / JKS is required — it runs in CI under `cargo test -p prro`)
//! and a committed self-signed X.509 test cert, then byte-asserts the produced
//! CMS carries the ЦЗО CAdES-BES profile:
//!   1. `eContent` — the content embedded as a primitive OCTET STRING (ATTACHED);
//!   2. a `signingTime` signed-attribute (OID 1.2.840.113549.1.9.5);
//!   3. a `messageDigest` signed-attribute whose value == GOST-34.311(content);
//!   4. a `contentType` signed-attribute (OID 1.2.840.113549.1.9.3).
//!
//! Every official ЦЗО reference vector (`czo_test/*.p7s`) carries (1)–(4); this
//! test locks the gateway to the same shape so a future regression (e.g. a
//! detached or signingTime-less envelope) fails in CI rather than at live DPS.

use prro_crypto::cms::{CmsBuildOptions, CmsProfile, CmsSigner, RawSigner, SignerError};

/// Committed self-signed X.509 test cert (DER) — PUBLIC, no key material.
/// Only parsed for IssuerAndSerialNumber + embedded in `certificates`; the
/// signature value is stubbed, so the cert's own key/curve is irrelevant.
const TEST_CERT_DER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/SELF_SIGNED_ENC_6929.cer"
));

/// Stub signer: returns a fixed 64-byte raw DSTU signature value.  No key.
struct StubSigner;
impl RawSigner for StubSigner {
    fn sign_digest(&self, _digest: &[u8]) -> Result<Vec<u8>, SignerError> {
        Ok(vec![0u8; 64])
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && needle.len() <= haystack.len()
        && haystack.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn gateway_attached_cms_carries_econtent_signingtime_and_messagedigest() {
    let signer = StubSigner;
    let cms_signer = CmsSigner {
        cert_der: TEST_CERT_DER,
        signer: &signer,
        profile: CmsProfile::Dstu4145WithGost34311Pb,
    };

    // Distinctive content (< 128 bytes) so the byte-search + length framing hold.
    let content = b"<RQ V=\"1\">PRRO-W4Z3-PROFILE-PROOF</RQ>";
    assert!(
        content.len() < 0x80,
        "content must be < 128 bytes for the framing checks"
    );

    // EXACTLY the gateway's production options (see crypto/in_process.rs).
    let cms = cms_signer
        .sign_with(
            content,
            CmsBuildOptions {
                attached: true,
                signing_time: Some(std::time::SystemTime::now()),
            },
        )
        .expect("attached sign with the committed test cert")
        .cms_der;

    // Sanity: well-formed ContentInfo (outer SEQUENCE).
    assert_eq!(cms[0], 0x30, "not a SEQUENCE / ContentInfo");

    // (1) ATTACHED — content embedded as a primitive OCTET STRING (eContent).
    let mut octet_framed = vec![0x04u8, content.len() as u8];
    octet_framed.extend_from_slice(content);
    assert!(
        contains(&cms, &octet_framed),
        "eContent OCTET STRING(content) missing → CMS is NOT attached"
    );

    // (2) signingTime signed-attr — OID 1.2.840.113549.1.9.5 (ЦЗО profile requires it).
    const OID_SIGNING_TIME: [u8; 11] = [
        0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x05,
    ];
    assert!(
        contains(&cms, &OID_SIGNING_TIME),
        "signingTime signed-attr missing — diverges from the accepted ЦЗО CAdES-BES profile"
    );

    // (3) messageDigest signed-attr — OID 1.2.840.113549.1.9.4 — value == GOST-34.311(content).
    const OID_MESSAGE_DIGEST: [u8; 11] = [
        0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x04,
    ];
    assert!(
        contains(&cms, &OID_MESSAGE_DIGEST),
        "messageDigest signed-attr missing"
    );
    let digest = prro_crypto::core::hash::gost_34_311_95(content);
    let mut digest_framed = vec![0x04u8, digest.len() as u8];
    digest_framed.extend_from_slice(&digest);
    assert!(
        contains(&cms, &digest_framed),
        "messageDigest value != OCTET STRING(GOST-34.311(content))"
    );

    // (4) contentType signed-attr — OID 1.2.840.113549.1.9.3 (CAdES-BES required).
    const OID_CONTENT_TYPE: [u8; 11] = [
        0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x03,
    ];
    assert!(
        contains(&cms, &OID_CONTENT_TYPE),
        "contentType signed-attr missing"
    );
}

/// Regression for the `signingTime` UTCTIME conversion (`unix_secs_to_utc`):
/// the Hinnant month adjustment MUST use signed arithmetic.  For Jan/Feb
/// (mp≥10) the old `mp + (-9i64 as u64)` form OVERFLOWED under overflow-checks
/// (the `cargo test` debug default) → panic on every Jan/Feb signingTime — and
/// signingTime is now load-bearing in the gateway CMS path.  The other tests
/// stamp `now()` (currently outside Jan/Feb) so they would miss it.  These
/// fixed timestamps exercise the Jan/Feb branch and assert the encoded UTCTIME
/// carries the CORRECT month (01 / 02), not merely that it did not panic.
#[test]
fn signing_time_jan_feb_encode_correct_month_without_overflow() {
    let signer = StubSigner;
    let cms_signer = CmsSigner {
        cert_der: TEST_CERT_DER,
        signer: &signer,
        profile: CmsProfile::Dstu4145WithGost34311Pb,
    };
    let content = b"x";
    // (unix secs, expected UTCTIME yyMMddHHmmssZ)
    let cases: [(u64, &[u8]); 2] = [
        (1_609_459_200, b"210101000000Z"), // 2021-01-01T00:00:00Z (Jan, Hinnant mp=10)
        (1_612_137_600, b"210201000000Z"), // 2021-02-01T00:00:00Z (Feb, Hinnant mp=11)
    ];
    for (unix, utctime) in cases {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(unix);
        let cms = cms_signer
            .sign_with(
                content,
                CmsBuildOptions {
                    attached: true,
                    signing_time: Some(t),
                },
            )
            .expect("Jan/Feb signingTime must encode without panic/error")
            .cms_der;
        assert!(
            contains(&cms, utctime),
            "signingTime UTCTIME {:?} missing — Jan/Feb month conversion wrong/overflowed",
            std::str::from_utf8(utctime).unwrap()
        );
    }
}

/// Sibling coverage for the date/time conversion (date bugs travel in packs):
/// leap-day Feb 29, the Dec branch (Hinnant mp=9, opposite arm to Jan/Feb), a
/// full time-of-day rollover, the UTCTIME upper cliff (2049 still valid), and
/// the 2050 **fail-fast** — which MUST be an `Err`, NOT a panic and NOT a
/// silently-wrong UTCTIME (UTCTIME per RFC 5652 only covers 1950–2049; 2050+
/// requires GeneralizedTime — a tracked future follow-up).  All via the
/// production `CmsSigner::sign_with` signingTime path.
#[test]
fn signing_time_date_boundaries_and_2050_cliff() {
    let signer = StubSigner;
    let cms_signer = CmsSigner {
        cert_der: TEST_CERT_DER,
        signer: &signer,
        profile: CmsProfile::Dstu4145WithGost34311Pb,
    };
    let content = b"x";
    let sign_at = |secs: u64| {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
        cms_signer.sign_with(
            content,
            CmsBuildOptions {
                attached: true,
                signing_time: Some(t),
            },
        )
    };

    // Valid dates → the correct UTCTIME must be present.
    let ok: [(u64, &[u8]); 4] = [
        (1_709_164_800, b"240229000000Z"), // 2024-02-29 00:00:00Z — leap day
        (1_704_067_199, b"231231235959Z"), // 2023-12-31 23:59:59Z — Dec branch (mp=9) + time rollover
        (2_524_521_600, b"491231000000Z"), // 2049-12-31 00:00:00Z — UTCTIME upper cliff, still valid
        (1_609_459_200, b"210101000000Z"), // 2021-01-01 00:00:00Z — Jan sanity
    ];
    for (secs, utctime) in ok {
        let cms = sign_at(secs)
            .expect("valid signingTime must encode without panic/error")
            .cms_der;
        assert!(
            contains(&cms, utctime),
            "signingTime UTCTIME {:?} missing/wrong",
            std::str::from_utf8(utctime).unwrap()
        );
    }

    // 2050-01-01 → outside UTCTIME range → MUST fail-fast (Err), not panic, not
    // a silently-wrong encoding.  Locks the deliberate 1950..=2049 gate.
    assert!(
        sign_at(2_524_608_000).is_err(),
        "2050 signingTime must error (UTCTIME range exceeded), not silently encode"
    );

    // Far-future (year 10000): MUST also Err, not silently encode.  Locks the r4
    // fix that keeps `year` as i64 through the range check — a u32 cast could
    // wrap a huge year back into 1950..=2049 and emit a bogus UTCTIME.
    assert!(
        sign_at(253_402_300_800).is_err(),
        "year-10000 signingTime must error, not silently encode (year stays i64 pre-check)"
    );
}
