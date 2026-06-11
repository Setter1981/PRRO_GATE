// Operator benchmark — explicit-index loops mirror the CT house-pattern
// (see prro_crypto/src/core/mod.rs allows); not a lib code path.
#![allow(clippy::needless_range_loop)]

/// Standalone timing test for raw DSTU 4145 sign + verify (no CMS, no TSP, no I/O).
///
/// Run with:   cargo test --test bench_sign_raw --release -- --nocapture
/// (debug build is ~10× slower — always use --release for meaningful numbers)
use std::time::Instant;

use prro_crypto::core::{
    curve::Curve,
    field::FieldEl,
    hash::{gost_34_311_95, kupyna_256},
    point::Point,
    sign::{sign, verify, Signature},
};

fn xorshift(x: u32) -> u32 {
    let x = x ^ (x << 13);
    let x = x ^ (x >> 17);
    x ^ (x << 5)
}

fn make_rand_e(seed: u32, mw: usize) -> FieldEl {
    let mut x = seed;
    let mut v = vec![0u32; mw];
    for i in 0..mw {
        x = xorshift(x);
        v[i] = x;
    }
    v[mw - 1] = 0; // keep < 2^(32*(mw-1)) — sign() contract
    FieldEl::from_words(v)
}

#[test]
fn raw_dstu4145_throughput() {
    const WARMUP: usize = 10;
    const SIGN_ITERS: usize = 500;
    const VERIFY_ITERS: usize = 500;
    const HASH_ITERS: usize = 5_000;

    let curve = Curve::dstu_pb_257();
    let mw = curve.mod_words;

    // Use a known-good (d, pub_x, pub_y) triple from tests/vectors/sign_jkurwa.json.
    // pub_q = Q = point whose coordinates are given in the test vector.
    // This is jkurwa's convention: Q is stored as-is (not re-negated here).
    let d = FieldEl::from_hex(
        "DEADBEEFCAFE12345678ABCD90909090ABCDEF1234567890FEDCBA0987654321",
        mw,
    );
    let pub_q = Point::new(
        FieldEl::from_hex(
            "00000001090b051c42fb2ad5012a35a177600cb36d586894a52812d5d3f4b128ec81f49a",
            mw,
        ),
        FieldEl::from_hex(
            "0000000195f2da315d29da8370293915918848cd524f790b02e7f25f2f0e533ad7754c2e",
            mw,
        ),
    );

    // Use a known hash from the same vector to confirm sign/verify roundtrip.
    let hash = FieldEl::from_hex(
        "01020304050607080910AABBCCDDEEFF1234567890ABCDEF1122334455667788",
        mw,
    );

    // Verify the known vector signature as a sanity check.
    let known_sig = Signature {
        r: FieldEl::from_hex(
            "00000000135bbdf209c61401e1e4800604ea21ce8ce6a1c09cf19ae17cc0307376abafc3",
            mw,
        ),
        s: FieldEl::from_hex(
            "00000000321d16d15abe3a370fbe6941610fe2da916361767201722a4b464c90a163765a",
            mw,
        ),
    };
    assert!(
        verify(&curve, &pub_q, &hash, &known_sig),
        "known test-vector signature must verify"
    );

    // Confirm fresh sign + verify roundtrip before timing.
    let rand_e0 = make_rand_e(0xBEEF, mw);
    let fresh_sig = sign(&curve, &d, &hash, &rand_e0).expect("sign must not return None");
    assert!(
        verify(&curve, &pub_q, &hash, &fresh_sig),
        "fresh sign+verify roundtrip failed"
    );

    // Pre-generate rand_e pool.
    let rand_seeds: Vec<FieldEl> = (0..SIGN_ITERS + WARMUP)
        .map(|i| make_rand_e(0xE12E ^ (i as u32).wrapping_mul(7919), mw))
        .collect();

    // ── Warmup sign ──────────────────────────────────────────────────────
    for i in 0..WARMUP {
        let _ = sign(&curve, &d, &hash, &rand_seeds[i]).unwrap();
    }

    // ── Sign benchmark ───────────────────────────────────────────────────
    let t0 = Instant::now();
    for i in 0..SIGN_ITERS {
        let _ = std::hint::black_box(sign(&curve, &d, &hash, &rand_seeds[WARMUP + i]).unwrap());
    }
    let sign_elapsed = t0.elapsed();
    let sign_us = sign_elapsed.as_micros() as f64 / SIGN_ITERS as f64;
    let sign_per_sec = 1_000_000.0 / sign_us;

    // ── Verify benchmark ─────────────────────────────────────────────────
    // First call pays pubkey-validation cache miss.
    let t_vmiss = Instant::now();
    let _ = verify(&curve, &pub_q, &hash, &known_sig);
    let vmiss_us = t_vmiss.elapsed().as_micros();

    let t1 = Instant::now();
    for _ in 0..VERIFY_ITERS {
        let ok = std::hint::black_box(verify(&curve, &pub_q, &hash, &known_sig));
        assert!(ok);
    }
    let verify_elapsed = t1.elapsed();
    let verify_us = verify_elapsed.as_micros() as f64 / VERIFY_ITERS as f64;
    let verify_per_sec = 1_000_000.0 / verify_us;

    // ── Hash benchmarks ──────────────────────────────────────────────────
    let t2 = Instant::now();
    for i in 0..HASH_ITERS {
        let p: Vec<u8> = (0u8..64).map(|b| b ^ (i as u8)).collect();
        let _ = std::hint::black_box(gost_34_311_95(&p));
    }
    let gost_us = t2.elapsed().as_micros() as f64 / HASH_ITERS as f64;

    let t3 = Instant::now();
    for i in 0..HASH_ITERS {
        let p: Vec<u8> = (0u8..64).map(|b| b ^ (i as u8)).collect();
        let _ = std::hint::black_box(kupyna_256(&p));
    }
    let kupyna_us = t3.elapsed().as_micros() as f64 / HASH_ITERS as f64;

    // ── Print results ────────────────────────────────────────────────────
    println!();
    println!(
        "=== DSTU 4145-2002 / pb-257 — pure math throughput ({} iters) ===",
        SIGN_ITERS
    );
    println!(
        "sign              {:>8.1} µs/op   {:>8.0} ops/sec",
        sign_us, sign_per_sec
    );
    println!(
        "verify (hot)      {:>8.1} µs/op   {:>8.0} ops/sec",
        verify_us, verify_per_sec
    );
    println!(
        "verify (cold)     {:>8} µs/op   (first call — pubkey validation + cache fill)",
        vmiss_us
    );
    println!(
        "gost3411_64B      {:>8.1} µs/op   {:>8.0} ops/sec",
        gost_us,
        1_000_000.0 / gost_us
    );
    println!(
        "kupyna256_64B     {:>8.1} µs/op   {:>8.0} ops/sec",
        kupyna_us,
        1_000_000.0 / kupyna_us
    );
    println!();
    println!("IIT EUSignCP comparison (OCSP+TSP per sign): 200–800 ms/op = 1–5 ops/sec");

    assert!(
        sign_us < 10_000.0,
        "sign too slow: {:.1} µs (threshold 10 000 µs)",
        sign_us
    );
}
