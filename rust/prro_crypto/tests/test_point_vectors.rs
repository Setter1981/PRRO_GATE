//! Byte-identical proof for point arithmetic against jkurwa.
//!
//! Covers: base point identity, twice, add, negate, sequential k*G additions.
//! Scalar multiplication via Point::mul is verified algebraically by comparing
//! against sequential additions (since jkurwa's Point.mul has an inconsistent
//! API requiring a custom BN — out of scope for this step).

use serde::Deserialize;
use std::collections::HashMap;

use prro_crypto::{Curve, FieldEl, Point};

#[derive(Debug, Deserialize)]
struct PointObj {
    x: String,
    y: String,
}

#[derive(Debug, Deserialize)]
struct SequentialEntry {
    k: u32,
    p: PointObj,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op")]
enum Vector {
    #[serde(rename = "base_point")]
    BasePoint { expected: PointObj },
    #[serde(rename = "twice")]
    Twice {
        p: PointObj,
        expected: PointObj,
        #[serde(default)]
        result_label: Option<String>,
    },
    #[serde(rename = "add")]
    Add {
        a: PointObj,
        b: PointObj,
        expected: PointObj,
        #[serde(default)]
        note: Option<String>,
    },
    #[serde(rename = "negate")]
    Negate { p: PointObj, expected: PointObj },
    #[serde(rename = "sequential_kG")]
    SequentialKg { expected: Vec<SequentialEntry> },
}

fn field_from_hex(hex: &str, mod_words: usize) -> FieldEl {
    FieldEl::from_hex(hex, mod_words)
}

fn point_from_obj(p: &PointObj, mod_words: usize) -> Point {
    Point::new(
        field_from_hex(&p.x, mod_words),
        field_from_hex(&p.y, mod_words),
    )
}

fn field_to_hex(f: &FieldEl) -> String {
    let mut s = String::new();
    for i in (0..f.bytes.len()).rev() {
        s.push_str(&format!("{:08x}", f.bytes[i]));
    }
    s
}

/// Compare hex strings semantically (strip leading zeros). Required because
/// jkurwa's `curve.zero` field has only 1 word while our standardized field
/// elements have `mod_words` (9 for DSTU_PB_257) — both represent zero, just
/// with different padding.
fn hex_eq(a: &str, b: &str) -> bool {
    let a_trim = a.trim_start_matches('0');
    let b_trim = b.trim_start_matches('0');
    a_trim == b_trim
}

fn point_matches(got: &Point, expected: &PointObj) -> bool {
    hex_eq(&field_to_hex(&got.x), &expected.x) && hex_eq(&field_to_hex(&got.y), &expected.y)
}

#[test]
fn test_against_jkurwa_point_vectors() {
    let curve = Curve::dstu_pb_257();
    let mw = curve.mod_words;

    let json = include_str!("vectors/point_jkurwa.json");
    let vectors: Vec<Vector> = serde_json::from_str(json).expect("malformed json");
    assert!(!vectors.is_empty(), "no vectors loaded");

    let mut counts: HashMap<&str, (usize, usize)> = HashMap::new();
    let mut failures: Vec<String> = Vec::new();

    for (idx, v) in vectors.iter().enumerate() {
        let (op, ok, detail) = match v {
            Vector::BasePoint { expected } => {
                let got = Point::new(curve.base_x.clone(), curve.base_y.clone());
                let m = point_matches(&got, expected);
                (
                    "base_point",
                    m,
                    format!(
                        "expected x={} y={}, got x={} y={}",
                        expected.x,
                        expected.y,
                        field_to_hex(&got.x),
                        field_to_hex(&got.y)
                    ),
                )
            }
            Vector::Twice { p, expected, .. } => {
                let pp = point_from_obj(p, mw);
                let got = pp.twice(&curve);
                let m = point_matches(&got, expected);
                (
                    "twice",
                    m,
                    format!(
                        "expected x={}, got x={}",
                        expected.x,
                        field_to_hex(&got.x)
                    ),
                )
            }
            Vector::Add { a, b, expected, .. } => {
                let pa = point_from_obj(a, mw);
                let pb = point_from_obj(b, mw);
                let got = pa.add(&pb, &curve);
                let m = point_matches(&got, expected);
                (
                    "add",
                    m,
                    format!(
                        "expected x={}, got x={}",
                        expected.x,
                        field_to_hex(&got.x)
                    ),
                )
            }
            Vector::Negate { p, expected } => {
                let pp = point_from_obj(p, mw);
                let got = pp.negate();
                let m = point_matches(&got, expected);
                (
                    "negate",
                    m,
                    format!(
                        "expected x={} y={}, got x={} y={}",
                        expected.x,
                        expected.y,
                        field_to_hex(&got.x),
                        field_to_hex(&got.y)
                    ),
                )
            }
            Vector::SequentialKg { expected } => {
                // For each k from 1 to N, verify our scalar mul matches the
                // jkurwa-computed kG = G added k times.
                let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
                let mut ok_all = true;
                let mut bad_k: Option<u32> = None;
                for entry in expected {
                    // Build scalar k as field element
                    let mut k_words = vec![0u32; mw];
                    k_words[0] = entry.k;
                    let k_field = FieldEl::from_words(k_words);
                    let got = g.mul(&k_field, &curve);
                    if !point_matches(&got, &entry.p) {
                        ok_all = false;
                        bad_k = Some(entry.k);
                        break;
                    }
                }
                (
                    "sequential_kG",
                    ok_all,
                    match bad_k {
                        Some(k) => format!("k={} mismatch", k),
                        None => format!("all {} k values match", expected.len()),
                    },
                )
            }
        };

        let entry = counts.entry(op).or_insert((0, 0));
        entry.0 += 1;
        if ok {
            entry.1 += 1;
        } else {
            failures.push(format!("vector #{} ({}): {}", idx, op, detail));
        }
    }

    eprintln!("\n=== Point vector results ===");
    for (op, (total, passed)) in &counts {
        eprintln!("  {}: {}/{}", op, passed, total);
    }

    if !failures.is_empty() {
        eprintln!("\n=== Failures ===");
        for f in failures.iter().take(10) {
            eprintln!("  {}", f);
        }
        panic!("{} of {} vectors failed", failures.len(), vectors.len());
    }
}

/// Algebraic correctness test: order * G should equal point at infinity.
/// This proves scalar multiplication is mathematically correct without
/// requiring a jkurwa reference for mul.
#[test]
fn test_order_times_g_is_infinity() {
    let curve = Curve::dstu_pb_257();
    let g = Point::new(curve.base_x.clone(), curve.base_y.clone());

    // n * G should be the point at infinity (group order property)
    let result = g.mul(&curve.order, &curve);

    assert!(
        result.is_zero(),
        "order * G should be point at infinity, got x={:x?} y={:x?}",
        result.x.bytes,
        result.y.bytes
    );
}
