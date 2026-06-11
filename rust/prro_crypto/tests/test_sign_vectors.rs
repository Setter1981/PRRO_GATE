//! Byte-identical proof for DSTU 4145 sign/verify against jkurwa.
//!
//! This is the most important integration test: it proves that our
//! signature output matches jkurwa byte-for-byte for the same inputs.
//! If this passes, the cryptographic chain (gf2m → field → curve → point
//! → sign) is correct end-to-end.
//!
//! Two test entry points:
//!   - `test_sign_byte_identical_with_jkurwa` — the committed 3-vector
//!     smoke test; runs on every `cargo test`.
//!   - `test_sign_byte_identical_with_jkurwa_10k` — opt-in `#[ignore]`
//!     test that reads `tests/vectors/sign_jkurwa_10k.json` if present
//!     and compares every vector. Use it as the pre-release acceptance
//!     gate when swapping jkurwa for `prro_crypto` in PRRO_GATE. Generate
//!     the file with
//!     `node scripts/extract_sign_vectors_bulk.js 10000 > tests/vectors/sign_jkurwa_10k.json`
//!     (the file is not committed — too large for repo history).

use serde::Deserialize;

use prro_crypto::{sign, verify, Curve, FieldEl, Point, Signature};

#[derive(Debug, Deserialize)]
#[serde(tag = "op")]
enum Vector {
    #[serde(rename = "sign")]
    Sign {
        d: String,
        hash: String,
        rand_e: String,
        expected_r: String,
        expected_s: String,
    },
    #[serde(rename = "verify")]
    Verify {
        // Mirrors the fixture JSON shape (the generating key); verify
        // itself consumes only the public point — never read here.
        #[allow(dead_code)]
        d: String,
        hash: String,
        r: String,
        s: String,
        pub_x: String,
        pub_y: String,
        expected_valid: bool,
    },
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

fn run_vectors(vectors: Vec<Vector>, label: &str) {
    let curve = Curve::dstu_pb_257();
    let mw = curve.mod_words;

    let mut sign_passed = 0;
    let mut sign_total = 0;
    let mut verify_passed = 0;
    let mut verify_total = 0;
    let mut failures: Vec<String> = Vec::new();

    for (idx, v) in vectors.iter().enumerate() {
        match v {
            Vector::Sign {
                d,
                hash,
                rand_e,
                expected_r,
                expected_s,
            } => {
                sign_total += 1;
                let d_field = FieldEl::from_hex(d, mw);
                let hash_field = FieldEl::from_hex(hash, mw);
                let e_field = FieldEl::from_hex(rand_e, mw);

                let result = sign(&curve, &d_field, &hash_field, &e_field);
                match result {
                    None => {
                        failures.push(format!(
                            "vector #{} sign: returned None (jkurwa expected r={}, s={})",
                            idx, expected_r, expected_s
                        ));
                    }
                    Some(sig) => {
                        let got_r = field_to_hex_padded(&sig.r);
                        let got_s = field_to_hex_padded(&sig.s);
                        if hex_eq(&got_r, expected_r) && hex_eq(&got_s, expected_s) {
                            sign_passed += 1;
                        } else {
                            failures.push(format!(
                                "vector #{} sign mismatch:\n  expected r={}\n  got      r={}\n  expected s={}\n  got      s={}",
                                idx, expected_r, got_r, expected_s, got_s
                            ));
                        }
                    }
                }
            }
            Vector::Verify {
                d: _,
                hash,
                r,
                s,
                pub_x,
                pub_y,
                expected_valid,
            } => {
                verify_total += 1;
                let hash_field = FieldEl::from_hex(hash, mw);
                let r_field = FieldEl::from_hex(r, mw);
                let s_field = FieldEl::from_hex(s, mw);
                let pub_q = Point::new(FieldEl::from_hex(pub_x, mw), FieldEl::from_hex(pub_y, mw));
                let sig = Signature {
                    r: r_field,
                    s: s_field,
                };
                let got_valid = verify(&curve, &pub_q, &hash_field, &sig);
                if got_valid == *expected_valid {
                    verify_passed += 1;
                } else {
                    failures.push(format!(
                        "vector #{} verify: expected {}, got {}",
                        idx, expected_valid, got_valid
                    ));
                }
            }
        }
    }

    eprintln!("\n=== Sign vector results [{}] ===", label);
    eprintln!("  sign:   {}/{}", sign_passed, sign_total);
    eprintln!("  verify: {}/{}", verify_passed, verify_total);

    if !failures.is_empty() {
        eprintln!("\n=== Failures [{}] ===", label);
        for f in failures.iter().take(5) {
            eprintln!("{}", f);
        }
        if failures.len() > 5 {
            eprintln!("... and {} more", failures.len() - 5);
        }
        panic!("{} failures in [{}]", failures.len(), label);
    }
}

#[test]
fn test_sign_byte_identical_with_jkurwa() {
    let json = include_str!("vectors/sign_jkurwa.json");
    let vectors: Vec<Vector> = serde_json::from_str(json).expect("malformed json");
    assert!(!vectors.is_empty(), "no vectors loaded");
    run_vectors(vectors, "sign_jkurwa.json (3 vectors)");
}

/// Pre-release acceptance gate: 10 000 random-but-deterministic
/// (d, hash, rand_e) triples generated by jkurwa via a seeded PRNG.
/// Byte-identity across this batch is the evidence that jkurwa and
/// prro_crypto produce the same `(r, s)` for any input in the covered
/// space — i.e. that swapping one for the other in PRRO_GATE cannot
/// change what ДПС sees on the wire.
///
/// Marked `#[ignore]` because the 10k vector file is not committed
/// (~4 MB, regenerable). Run it explicitly:
///
/// ```
/// node scripts/extract_sign_vectors_bulk.js 10000 > tests/vectors/sign_jkurwa_10k.json
/// cargo test --release --test test_sign_vectors -- --ignored --nocapture
/// ```
///
/// Skipped with a message if the vector file is missing.
#[test]
#[ignore]
fn test_sign_byte_identical_with_jkurwa_10k() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/vectors/sign_jkurwa_10k.json"
    );
    let data = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "SKIP {}: {}\n  generate with:\n    node scripts/extract_sign_vectors_bulk.js \
                 10000 > tests/vectors/sign_jkurwa_10k.json",
                path, e
            );
            return;
        }
    };
    let vectors: Vec<Vector> = serde_json::from_str(&data).expect("malformed 10k json");
    assert!(!vectors.is_empty(), "no vectors loaded");
    run_vectors(vectors, "sign_jkurwa_10k.json");
}
