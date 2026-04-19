//! Standalone benchmark — no criterion dependency.
//! Compile: cargo build --release --example bench_standalone
//! Run on Windows: target\release\examples\bench_standalone.exe

use std::time::Instant;

use prro_crypto::core::field::FieldEl;
use prro_crypto::core::hash::{gost_34_311_95, kupyna_256, kupyna_512};
use prro_crypto::core::point::Point;
use prro_crypto::core::sign::{sign, verify};
use prro_crypto::Curve;

fn bench<F: FnMut()>(name: &str, iterations: u32, mut f: F) {
    // Warm up
    for _ in 0..5 {
        f();
    }
    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    let elapsed = start.elapsed();
    let per_iter = elapsed / iterations;
    let per_sec = if per_iter.as_nanos() > 0 {
        1_000_000_000 / per_iter.as_nanos()
    } else {
        0
    };
    println!(
        "{:<24} {:>10.3} us   ({} ops/sec, {} iters)",
        name,
        per_iter.as_nanos() as f64 / 1000.0,
        per_sec,
        iterations,
    );
}

fn main() {
    println!("=== prro_crypto standalone benchmark ===");
    println!();

    let curve = Curve::dstu_pb_257();

    // Prepare sign inputs
    let d = FieldEl::from_hex("DEADBEEFCAFE12345678", curve.mod_words);
    let hash = FieldEl::from_hex("01020304050607080910AABBCCDDEEFF", curve.mod_words);
    let mut rand_words = vec![0u32; curve.mod_words];
    rand_words[0] = 0xE12E_CAFE;
    rand_words[1] = 0xDEAD_BEEF;
    rand_words[2] = 0x1234_5678;
    rand_words[3] = 0x9ABC_DEF0;
    let rand_e = FieldEl::from_words(rand_words);

    // Prepare verify inputs
    let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
    let pub_q = g.mul(&d, &curve).negate();
    let sig = sign(&curve, &d, &hash, &rand_e).expect("sign must succeed");

    // Hash inputs
    let msg_64: Vec<u8> = (0x00u8..=0x3F).collect();
    let msg_256: Vec<u8> = (0..256).map(|i| (i & 0xFF) as u8).collect();
    let msg_1k = vec![0xABu8; 1024];

    println!("--- Sign / Verify ---");
    bench("sign_full", 500, || {
        std::hint::black_box(sign(&curve, &d, &hash, &rand_e));
    });
    // First verify — includes full pubkey validation (n·Q, cofactor)
    bench("verify_first", 1, || {
        std::hint::black_box(verify(&curve, &pub_q, &hash, &sig));
    });
    // Subsequent — pubkey cached, only Shamir dual-mul
    bench("verify_cached", 500, || {
        std::hint::black_box(verify(&curve, &pub_q, &hash, &sig));
    });

    println!();
    println!("--- Hash (64 bytes) ---");
    bench("gost3411_64B", 5000, || {
        std::hint::black_box(gost_34_311_95(&msg_64));
    });
    bench("kupyna256_64B", 5000, || {
        std::hint::black_box(kupyna_256(&msg_64));
    });
    bench("kupyna512_64B", 5000, || {
        std::hint::black_box(kupyna_512(&msg_64));
    });

    println!();
    println!("--- Hash (256 bytes) ---");
    bench("gost3411_256B", 2000, || {
        std::hint::black_box(gost_34_311_95(&msg_256));
    });
    bench("kupyna256_256B", 2000, || {
        std::hint::black_box(kupyna_256(&msg_256));
    });

    println!();
    println!("--- Hash (1 KB) ---");
    bench("gost3411_1KB", 1000, || {
        std::hint::black_box(gost_34_311_95(&msg_1k));
    });
    bench("kupyna256_1KB", 1000, || {
        std::hint::black_box(kupyna_256(&msg_1k));
    });

    println!();
    println!("Done.");
}
