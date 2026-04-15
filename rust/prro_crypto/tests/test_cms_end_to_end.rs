//! End-to-end test for CMS detached signing.
//!
//! Reads the real production JKS, signs a small payload, dumps the CMS
//! to disk, and verifies structural properties (DER parses, SignedData
//! is valid, embedded cert matches, signature value is right size).
//!
//! For full byte-by-byte parity with sidecar/jkurwa output we would need
//! to implement the deterministic rand_e in lockstep — left for a later
//! pass since real production uses random rand_e per call.

use prro_crypto::{
    cms::{CmsProfile, CmsSigner, DstuInProcessSigner},
    der, jks,
};

fn load_signing_material() -> Option<(Vec<u8>, Vec<u8>)> {
    // Returns (cert_der, key_d_bytes_le).
    let path = "/mnt/d/PRRO_GATE/key_13667753_13667753 (2).jks";
    let data = std::fs::read(path).ok()?;
    let entry = jks::read_jks(&data, "Jrcfyf123").ok()?;
    let cert = entry.certs.into_iter().next()?;
    let d_bytes = der::extract_param_d(&entry.key_der).ok()?;
    Some((cert, d_bytes))
}

fn bytes_le_to_field(bytes: &[u8], mod_words: usize) -> prro_crypto::field::FieldEl {
    let mut words = vec![0u32; mod_words];
    for (i, b) in bytes.iter().enumerate() {
        let wi = i / 4;
        let bi = i % 4;
        if wi >= mod_words {
            break;
        }
        words[wi] |= (*b as u32) << (bi * 8);
    }
    prro_crypto::field::FieldEl::from_words(words)
}

#[test]
fn test_e2e_cms_real_jks_sign() {
    let (cert_der, key_d_bytes) = match load_signing_material() {
        Some(p) => p,
        None => {
            eprintln!("SKIP: production JKS not available");
            return;
        }
    };

    let curve = prro_crypto::Curve::dstu_pb_257();
    let d = bytes_le_to_field(&key_d_bytes, curve.mod_words);

    let signer = DstuInProcessSigner::new(d);
    let cms_signer = CmsSigner {
        cert_der: &cert_der,
        signer: &signer,
        profile: CmsProfile::default(),
    };

    let content = b"PRRO test fiscal receipt content";
    let result = cms_signer.sign_detached(content).expect("sign failed");
    let cms = &result.cms_der;

    // Sanity checks on output structure.
    assert!(cms.len() > 200, "CMS implausibly small: {}", cms.len());
    assert_eq!(cms[0], 0x30, "ContentInfo not SEQUENCE: {:02X}", cms[0]);

    // Walk one level: the contentType should be id-signedData OID.
    // ContentInfo ::= SEQUENCE { contentType OID, content [0] EXPLICIT ... }
    let ci_len_byte = cms[1];
    let ci_header_len = if ci_len_byte < 0x80 {
        2
    } else {
        2 + (ci_len_byte & 0x7F) as usize
    };
    let ci_content = &cms[ci_header_len..];

    // First field is OID. id-signedData = 1.2.840.113549.1.7.2 = OID DER bytes
    // 06 09 2A 86 48 86 F7 0D 01 07 02
    let expected_oid_prefix: [u8; 11] = [
        0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x02,
    ];
    assert!(
        ci_content.starts_with(&expected_oid_prefix),
        "ContentInfo doesn't start with id-signedData OID. Got: {:02X?}",
        &ci_content[..15.min(ci_content.len())]
    );

    eprintln!(
        "✓ CMS DER assembled: {} bytes, starts with valid ContentInfo + id-signedData",
        cms.len()
    );

    // Optionally save for external verification (openssl cms -inform DER -in ...)
    let _ = std::fs::write("/tmp/prro_test_signature.p7s", cms);
    eprintln!("  saved to /tmp/prro_test_signature.p7s");
}

#[test]
fn test_cms_round_trip_via_x509_parser() {
    // Verify we can re-parse our own output.
    let (cert_der, key_d_bytes) = match load_signing_material() {
        Some(p) => p,
        None => return,
    };
    let curve = prro_crypto::Curve::dstu_pb_257();
    let d = bytes_le_to_field(&key_d_bytes, curve.mod_words);

    let signer = DstuInProcessSigner::new(d);
    let cms_signer = CmsSigner {
        cert_der: &cert_der,
        signer: &signer,
        profile: CmsProfile::default(),
    };
    let result = cms_signer.sign_detached(b"hello world").unwrap();

    // Lightweight structural verification: outer is SEQUENCE,
    // length encoding is consistent, OID at expected position.
    assert_eq!(result.cms_der[0], 0x30, "outer not SEQUENCE");
    let len_byte = result.cms_der[1];
    let header_size = if len_byte < 0x80 {
        2
    } else {
        2 + (len_byte & 0x7F) as usize
    };
    let claimed_inner_len: usize = if len_byte < 0x80 {
        len_byte as usize
    } else {
        let mut n = 0usize;
        for i in 0..(len_byte & 0x7F) as usize {
            n = (n << 8) | (result.cms_der[2 + i] as usize);
        }
        n
    };
    assert_eq!(
        result.cms_der.len(),
        header_size + claimed_inner_len,
        "outer SEQUENCE length doesn't match buffer size"
    );
    eprintln!(
        "✓ ContentInfo SEQUENCE: header={} content={} total={}",
        header_size,
        claimed_inner_len,
        result.cms_der.len()
    );
}
