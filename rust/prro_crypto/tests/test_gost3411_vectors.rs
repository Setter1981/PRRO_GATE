//! Byte-identical proof: GOST 34.311-95 vs jkurwa for 21 vectors.
//!
//! Critical correctness gate. Any divergence here = bug in GOST 28147
//! block cipher or hash chain.

use serde::Deserialize;

use prro_crypto::hash::gost_34_311_95;

#[derive(Debug, Deserialize)]
struct Vector {
    label: String,
    input_hex: String,
    expected_hex: String,
}

#[test]
fn test_gost_34_311_byte_identical_vs_jkurwa() {
    let json = include_str!("vectors/gost3411_jkurwa.json");
    let vectors: Vec<Vector> = serde_json::from_str(json).expect("malformed");
    assert!(!vectors.is_empty());

    let mut passed = 0;
    let mut failures: Vec<String> = Vec::new();

    for v in &vectors {
        let input = hex::decode(&v.input_hex).expect("bad hex");
        let got = gost_34_311_95(&input);
        let got_hex = hex::encode(got);
        if got_hex == v.expected_hex {
            passed += 1;
        } else {
            failures.push(format!(
                "{}: expected {}, got {}",
                v.label, v.expected_hex, got_hex
            ));
        }
    }

    eprintln!("\nGOST 34.311 vector results: {}/{}", passed, vectors.len());
    if !failures.is_empty() {
        eprintln!("\n=== Failures ===");
        for f in failures.iter().take(5) {
            eprintln!("  {}", f);
        }
        panic!("{} vectors failed", failures.len());
    }
}
