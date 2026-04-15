//! End-to-end test: read real production JKS, extract private key, sign a
//! known hash, verify byte-identical output with jkurwa.
//!
//! This is the strongest test in the suite — it proves that the whole chain
//! (JKS reader → DER parser → field arithmetic → curve → point → sign)
//! works against a REAL Ukrainian production-format DSTU 4145 keystore.
//!
//! Skipped when the test JKS file is not present.

use serde::Deserialize;

use prro_crypto::{der, jks, sign, Curve, FieldEl};

#[derive(Debug, Deserialize)]
struct E2EVector {
    op: String,
    jks_path: String,
    password: String,
    d: String,
    hash: String,
    rand_e: String,
    expected_r: String,
    expected_s: String,
}

fn field_to_hex_padded(f: &FieldEl) -> String {
    let mut s = String::new();
    for i in (0..f.bytes.len()).rev() {
        s.push_str(&format!("{:08x}", f.bytes[i]));
    }
    s
}

fn hex_eq(a: &str, b: &str) -> bool {
    a.trim_start_matches('0') == b.trim_start_matches('0')
}

#[test]
fn test_e2e_jks_to_signature() {
    let json = include_str!("vectors/e2e_jks.json");
    let vectors: Vec<E2EVector> = serde_json::from_str(json).expect("malformed json");
    assert_eq!(vectors.len(), 1);
    let v = &vectors[0];
    assert_eq!(v.op, "e2e_jks_sign");

    // Step 1: read JKS file
    let jks_data = match std::fs::read(&v.jks_path) {
        Ok(d) => d,
        Err(_) => {
            eprintln!("SKIP: JKS file not at {}", v.jks_path);
            return;
        }
    };
    let entry = jks::read_jks(&jks_data, &v.password).expect("JKS read");
    eprintln!("JKS read: alias={}, key_der={} bytes, {} certs",
              entry.alias, entry.key_der.len(), entry.certs.len());

    // Step 2: extract param_d from DER-wrapped private key
    let d_bytes = der::extract_param_d(&entry.key_der).expect("extract param_d");
    eprintln!("Extracted param_d: {} bytes", d_bytes.len());

    // Step 3: convert d_bytes to FieldEl. param_d is stored little-endian per
    // jkurwa's `from_asn1` (see Priv.js `key0 = curve.pkey(util.BIG_LE(priv.param_d), 'buf32')`).
    // d_bytes from DER OCTET STRING is little-endian byte order, low-byte first.
    let curve = Curve::dstu_pb_257();
    let d_field = bytes_le_to_field(&d_bytes, curve.mod_words);
    let d_hex = field_to_hex_padded(&d_field);

    // Verify d matches what jkurwa produced
    assert!(
        hex_eq(&d_hex, &v.d),
        "extracted d mismatch:\n  expected: {}\n  got:      {}",
        v.d,
        d_hex,
    );
    eprintln!("d matches jkurwa: {}", v.d);

    // Step 4: sign with deterministic inputs
    let hash_field = FieldEl::from_hex(&v.hash, curve.mod_words);
    let e_field = FieldEl::from_hex(&v.rand_e, curve.mod_words);

    let sig = sign::sign(&curve, &d_field, &hash_field, &e_field)
        .expect("sign must succeed");

    let got_r = field_to_hex_padded(&sig.r);
    let got_s = field_to_hex_padded(&sig.s);

    eprintln!("expected r: {}", v.expected_r);
    eprintln!("got      r: {}", got_r);
    eprintln!("expected s: {}", v.expected_s);
    eprintln!("got      s: {}", got_s);

    assert!(hex_eq(&got_r, &v.expected_r), "r mismatch");
    assert!(hex_eq(&got_s, &v.expected_s), "s mismatch");

    eprintln!("\n✓ END-TO-END PROOF: real JKS → Rust sign produces byte-identical (r, s) with jkurwa");
}

/// Convert little-endian byte array to FieldEl with little-endian u32 words.
/// JKS stores param_d as bytes in little-endian order (low byte first).
fn bytes_le_to_field(bytes: &[u8], mod_words: usize) -> FieldEl {
    let mut words = vec![0u32; mod_words];
    for (i, b) in bytes.iter().enumerate() {
        let word_idx = i / 4;
        let byte_idx = i % 4;
        if word_idx >= mod_words {
            break;
        }
        words[word_idx] |= (*b as u32) << (byte_idx * 8);
    }
    FieldEl::from_words(words)
}
