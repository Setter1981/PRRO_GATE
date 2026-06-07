//! W4-Z3 piece-3 primitive: `cms::signed_data::extract_econtent`.
//!
//! Recovers the inner content from an ATTACHED CMS `SignedData` — the strip
//! the fiscal MAC chain needs (DPS `lastChk.data_sign` → inner check → SHA-256
//! → next `<MAC>`). Driven as an INTEGRATION test because prro_crypto's
//! in-crate `#[cfg(test)]` lib-test is blocked by a separate pre-existing
//! `node_modules`-fixture `include_bytes!`.
//!
//! Two angles:
//!   1. Hand-built DER (TLV helper) — exercises the parser walk + negatives
//!      precisely, no key needed.
//!   2. Round-trip against REAL `CmsSigner` output — cross-checks the
//!      extractor against the actual production attached-CMS assembler.

use prro_crypto::cms::signed_data::extract_econtent;

// ─── 1. Hand-built DER (deterministic; no key) ──────────────────────────

/// DER length octets (short or long form).
fn der_len(len: usize) -> Vec<u8> {
    if len < 0x80 {
        vec![len as u8]
    } else {
        let mut body = Vec::new();
        let mut n = len;
        while n > 0 {
            body.insert(0, (n & 0xff) as u8);
            n >>= 8;
        }
        let mut out = vec![0x80 | body.len() as u8];
        out.extend_from_slice(&body);
        out
    }
}

fn tlv(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut v = vec![tag];
    v.extend_from_slice(&der_len(body.len()));
    v.extend_from_slice(body);
    v
}

// 1.2.840.113549.1.7.2 (id-signedData) / .1 (id-data) value octets.
const SIGNED_DATA_OID: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x02];
const DATA_OID: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x01];

/// Minimal well-formed ATTACHED CMS carrying `content` as eContent.
/// (signerInfos/certs omitted — the extractor only reads up to encap.)
fn attached_cms(content: &[u8]) -> Vec<u8> {
    let econtent = tlv(0xA0, &tlv(0x04, content)); // eContent [0] EXPLICIT OCTET STRING
    let encap = tlv(0x30, &[tlv(0x06, DATA_OID), econtent].concat());
    let sd = tlv(
        0x30,
        &[tlv(0x02, &[0x01]), tlv(0x31, &[]), encap].concat(), // version, SET{}, encap
    );
    let content0 = tlv(0xA0, &sd); // content [0] EXPLICIT
    tlv(0x30, &[tlv(0x06, SIGNED_DATA_OID), content0].concat())
}

/// DETACHED CMS: encapContentInfo with eContentType only, no eContent.
fn detached_cms() -> Vec<u8> {
    let encap = tlv(0x30, &tlv(0x06, DATA_OID));
    let sd = tlv(0x30, &[tlv(0x02, &[0x01]), tlv(0x31, &[]), encap].concat());
    let content0 = tlv(0xA0, &sd);
    tlv(0x30, &[tlv(0x06, SIGNED_DATA_OID), content0].concat())
}

#[test]
fn roundtrips_short_content() {
    let content = b"<RQ V=\"1\">PRRO-W4Z3-CHAIN-LINK</RQ>";
    assert_eq!(extract_econtent(&attached_cms(content)).unwrap(), content);
}

#[test]
fn roundtrips_long_content_longform_lengths() {
    // > 127 bytes forces long-form DER lengths at every layer.
    let content = vec![0x41u8; 300];
    assert_eq!(extract_econtent(&attached_cms(&content)).unwrap(), content);
}

#[test]
fn empty_econtent_is_ok() {
    // Genesis-adjacent: attached but zero-length content.
    assert_eq!(extract_econtent(&attached_cms(b"")).unwrap(), b"");
}

#[test]
fn detached_is_rejected() {
    let err = extract_econtent(&detached_cms()).unwrap_err();
    assert!(
        format!("{err}").to_lowercase().contains("detached"),
        "detached CMS should report no eContent, got: {err}"
    );
}

#[test]
fn malformed_inputs_error_not_panic() {
    assert!(extract_econtent(&[]).is_err());
    assert!(extract_econtent(&[0x30]).is_err()); // tag only, no length
    let good = attached_cms(b"hello");
    assert!(extract_econtent(&good[..good.len() / 2]).is_err()); // truncated
    let mut wrong = attached_cms(b"x");
    wrong[0] = 0x31; // SET instead of SEQUENCE at the top
    assert!(extract_econtent(&wrong).is_err());
}

// ─── 2. Cross-check against the REAL CmsSigner assembler ────────────────

#[test]
fn roundtrips_real_cmssigner_attached_output() {
    use prro_crypto::cms::builder::{CmsBuildOptions, CmsSigner};
    use prro_crypto::cms::profile::CmsProfile;
    use prro_crypto::cms::signer::DstuInProcessSigner;
    use prro_crypto::core::curve::Curve;
    use prro_crypto::core::field::FieldEl;

    // Committed public test cert (key-free): structure only — the signature
    // need not verify, we are checking the eContent framing the production
    // assembler emits matches what the extractor reads back.
    let cert: &[u8] = include_bytes!("../../prro/tests/fixtures/SELF_SIGNED_ENC_6929.cer");
    let curve = Curve::dstu_pb_257();
    let d = FieldEl::from_le_bytes(&[7u8; 32], curve.mod_words);
    let signer = DstuInProcessSigner::new(d);

    let content = b"<CHECK><CHECKHEAD><MAC></MAC></CHECKHEAD></CHECK>";
    let cms = CmsSigner {
        cert_der: cert,
        signer: &signer,
        profile: CmsProfile::Dstu4145WithGost34311Pb,
    }
    .sign_with(
        content,
        CmsBuildOptions {
            attached: true,
            signing_time: None,
        },
    )
    .expect("attached sign must assemble")
    .cms_der;

    assert_eq!(
        extract_econtent(&cms).expect("extract from real CmsSigner output"),
        content,
        "extractor must recover the exact eContent the production assembler embedded"
    );
}
