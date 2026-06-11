//! Benchmarks for the prro_crypto core operations.
//!
//! Run with: `cargo bench --bench crypto`

// Explicit index loops mirror the core's CT style (see core/mod.rs).
#![allow(clippy::needless_range_loop)]

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use prro_crypto::{
    core::{
        fe::Fe,
        field::FieldEl,
        gf2m::{blength, finv, fmod, fmul},
        hash::{gost_34_311_95, kupyna_256, kupyna_512},
        point::Point,
        proj::ProjPoint,
        sign::{sign, verify},
    },
    Curve,
};

fn make_words(seed: u32) -> Vec<u32> {
    // Pseudo-random 9-word value (~257 bits) for benchmarking
    let mut x = seed;
    let mut v = vec![0u32; 9];
    for i in 0..9 {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        v[i] = x;
    }
    v[8] &= 0x1; // mask top to <2^257
    v
}

fn bench_gf2m(c: &mut Criterion) {
    let mut g = c.benchmark_group("gf2m");

    let a = make_words(0xCAFE);
    let b = make_words(0xBABE);
    let p_exp = [257u32, 12, 0];

    g.bench_function("fmul_257bit", |bench| {
        let mut s = vec![0u32; 20];
        bench.iter(|| {
            fmul(black_box(&a), black_box(&b), &mut s);
        });
    });

    g.bench_function("fmod_257bit", |bench| {
        let mut input = vec![0u32; 20];
        for i in 0..20 {
            input[i] = (a[i % 9]).wrapping_mul(0xDEAD_BEEF).wrapping_add(i as u32);
        }
        let mut ret = vec![0u32; 20];
        bench.iter(|| {
            fmod(black_box(&input), black_box(&p_exp), &mut ret);
        });
    });

    g.bench_function("finv_257bit", |bench| {
        let mut p_words = vec![0u32; 9];
        p_words[0] = 0x1001;
        p_words[8] = 0x2;
        let mut ret = vec![0u32; 9];
        bench.iter(|| {
            finv(black_box(&a), black_box(&p_words), &mut ret);
        });
    });

    g.bench_function("blength", |bench| {
        bench.iter(|| {
            black_box(blength(black_box(&a)));
        });
    });

    g.finish();
}

fn bench_field(c: &mut Criterion) {
    let mut g = c.benchmark_group("field");

    let curve = Curve::dstu_pb_257();
    let a = FieldEl::from_words(make_words(1));
    let b = FieldEl::from_words(make_words(2));

    g.bench_function("mod_mul", |bench| {
        bench.iter(|| {
            black_box(a.mod_mul(black_box(&b), &curve.p_exp, curve.mod_words));
        });
    });

    g.bench_function("mod_sqr", |bench| {
        bench.iter(|| {
            black_box(a.mod_sqr(&curve.p_exp, curve.mod_words));
        });
    });

    g.bench_function("invert", |bench| {
        bench.iter(|| {
            black_box(a.invert(&curve.p_exp, &curve.p_words, curve.mod_words));
        });
    });

    // Fixed-size Fe equivalents — Phase 2 / Commit 1
    let a_fe = Fe(<[u32; 9]>::try_from(make_words(1).as_slice()).unwrap());
    let b_fe = Fe(<[u32; 9]>::try_from(make_words(2).as_slice()).unwrap());

    g.bench_function("fe_mod_mul", |bench| {
        bench.iter(|| {
            black_box(a_fe.mod_mul(black_box(&b_fe)));
        });
    });

    g.bench_function("fe_mod_sqr", |bench| {
        bench.iter(|| {
            black_box(a_fe.mod_sqr());
        });
    });

    g.bench_function("fe_invert", |bench| {
        bench.iter(|| {
            black_box(a_fe.invert());
        });
    });

    g.finish();
}

fn bench_point(c: &mut Criterion) {
    let mut g = c.benchmark_group("point");

    let curve = Curve::dstu_pb_257();
    let p = Point::new(curve.base_x.clone(), curve.base_y.clone());
    let p2 = p.twice(&curve);

    g.bench_function("twice", |bench| {
        bench.iter(|| {
            black_box(p.twice(&curve));
        });
    });

    g.bench_function("add_distinct", |bench| {
        bench.iter(|| {
            black_box(p.add(black_box(&p2), &curve));
        });
    });

    // Scalar mul with full 256-bit scalar (worst case)
    let k = FieldEl::from_words(make_words(42));
    g.bench_function("mul_full_scalar", |bench| {
        bench.iter(|| {
            black_box(p.mul(black_box(&k), &curve));
        });
    });

    // Projective microbenchmarks — break down where time goes
    let p_proj = ProjPoint::from_affine(&p, curve.mod_words);
    g.bench_function("proj_double", |bench| {
        bench.iter(|| {
            black_box(p_proj.double(&curve));
        });
    });

    g.bench_function("proj_madd_affine", |bench| {
        bench.iter(|| {
            black_box(p_proj.madd_affine(black_box(&p2), &curve));
        });
    });

    g.bench_function("proj_to_affine", |bench| {
        bench.iter(|| {
            black_box(p_proj.to_affine(&curve));
        });
    });

    g.finish();
}

fn bench_sign(c: &mut Criterion) {
    let mut g = c.benchmark_group("sign");

    let curve = Curve::dstu_pb_257();
    let d = FieldEl::from_words(make_words(0xD1D));
    let hash = FieldEl::from_words(make_words(0xA3FF));
    // Keep bit 256 zero to satisfy the Sprint 2.2 sign() contract.
    let mut rand_words = make_words(0xE12E);
    rand_words[8] = 0;
    let rand_e = FieldEl::from_words(rand_words);

    g.bench_function("sign_full", |bench| {
        bench.iter(|| {
            black_box(sign(&curve, &d, &hash, &rand_e));
        });
    });

    // Verify benchmark: produce a valid signature, then measure verify
    let sig = sign(&curve, &d, &hash, &rand_e).expect("sign must succeed for bench inputs");
    let base_g = Point::new(curve.base_x.clone(), curve.base_y.clone());
    let pub_q = base_g.mul(&d, &curve).negate();

    g.bench_function("verify_full", |bench| {
        bench.iter(|| {
            black_box(verify(&curve, &pub_q, &hash, &sig));
        });
    });

    g.finish();
}

/// Per-primitive micro-benches for the PCLMULQDQ backend (Sprint 2.3
/// x86 polish). Expose the cost of the building blocks (`clmul64`,
/// `clmul128`, `fmul_packed`) alongside the public `fmul_257` entry
/// point, so any regression after a compiler or SIMD-layout change
/// shows up at the right level of granularity.
#[cfg(target_arch = "x86_64")]
fn bench_pclmul_primitives(c: &mut Criterion) {
    use prro_crypto::core::backend;
    use prro_crypto::core::backend::pack::pack_fe;
    use prro_crypto::core::fe::FeWide;

    if !std::is_x86_feature_detected!("pclmulqdq") {
        return;
    }

    let mut g = c.benchmark_group("pclmul");

    let a_words = make_words(0xAAAA);
    let b_words = make_words(0x5555);
    let a_fe = Fe(<[u32; 9]>::try_from(a_words.as_slice()).unwrap());
    let b_fe = Fe(<[u32; 9]>::try_from(b_words.as_slice()).unwrap());
    let a_packed = pack_fe(&a_fe);
    let b_packed = pack_fe(&b_fe);
    let a64 = ((a_words[1] as u64) << 32) | a_words[0] as u64;
    let b64 = ((b_words[1] as u64) << 32) | b_words[0] as u64;
    let a128 = [a_packed.0[0], a_packed.0[1]];
    let b128 = [b_packed.0[0], b_packed.0[1]];

    g.bench_function("clmul64", |bench| {
        bench.iter(|| unsafe {
            black_box(backend::x86_pclmul::clmul64(black_box(a64), black_box(b64)));
        });
    });

    g.bench_function("clmul128_karatsuba", |bench| {
        bench.iter(|| unsafe {
            black_box(backend::x86_pclmul::clmul128(
                black_box(a128),
                black_box(b128),
            ));
        });
    });

    g.bench_function("fmul_packed", |bench| {
        bench.iter(|| unsafe {
            black_box(backend::x86_pclmul::fmul_packed(
                black_box(&a_packed),
                black_box(&b_packed),
            ));
        });
    });

    g.bench_function("fmul_257_full_dispatch", |bench| {
        let mut out = FeWide::ZERO;
        bench.iter(|| {
            backend::fmul_257(black_box(&a_fe), black_box(&b_fe), &mut out);
            black_box(&out);
        });
    });

    g.finish();
}

#[cfg(not(target_arch = "x86_64"))]
fn bench_pclmul_primitives(_c: &mut Criterion) {}

fn bench_hash(c: &mut Criterion) {
    let mut g = c.benchmark_group("hash");

    // 64-byte message (typical fiscal receipt hash input)
    let msg_64: Vec<u8> = (0x00u8..=0x3F).collect();
    // 256-byte message
    let msg_256: Vec<u8> = (0..256).map(|i| (i & 0xFF) as u8).collect();
    // 1 KB message
    let msg_1k = vec![0xABu8; 1024];

    g.bench_function("gost3411_64B", |bench| {
        bench.iter(|| black_box(gost_34_311_95(black_box(&msg_64))));
    });

    g.bench_function("kupyna256_64B", |bench| {
        bench.iter(|| black_box(kupyna_256(black_box(&msg_64))));
    });

    g.bench_function("kupyna512_64B", |bench| {
        bench.iter(|| black_box(kupyna_512(black_box(&msg_64))));
    });

    g.bench_function("gost3411_256B", |bench| {
        bench.iter(|| black_box(gost_34_311_95(black_box(&msg_256))));
    });

    g.bench_function("kupyna256_256B", |bench| {
        bench.iter(|| black_box(kupyna_256(black_box(&msg_256))));
    });

    g.bench_function("gost3411_1KB", |bench| {
        bench.iter(|| black_box(gost_34_311_95(black_box(&msg_1k))));
    });

    g.bench_function("kupyna256_1KB", |bench| {
        bench.iter(|| black_box(kupyna_256(black_box(&msg_1k))));
    });

    g.finish();
}

criterion_group!(
    benches,
    bench_gf2m,
    bench_field,
    bench_point,
    bench_sign,
    bench_hash,
    bench_pclmul_primitives
);
criterion_main!(benches);
