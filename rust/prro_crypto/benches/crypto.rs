//! Benchmarks for the prro_crypto core operations.
//!
//! Run with: `cargo bench --bench crypto`

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use prro_crypto::{
    blength, finv, fmod, fmul,
    fe::Fe,
    field::FieldEl,
    point::Point,
    proj::ProjPoint,
    sign::sign,
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
    let rand_e = FieldEl::from_words(make_words(0xE12E));

    g.bench_function("sign_full", |bench| {
        bench.iter(|| {
            black_box(sign(&curve, &d, &hash, &rand_e));
        });
    });

    g.finish();
}

criterion_group!(benches, bench_gf2m, bench_field, bench_point, bench_sign);
criterion_main!(benches);
