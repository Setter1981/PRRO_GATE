//! Benchmarks for prro_crypto_v2 — sign, verify, batch_verify.
//!
//! Run (Windows bare-metal, release mode):
//!   cargo bench -p prro_crypto_v2 --bench crypto_v2
//!
//! HTML report: rust/target/criterion/report/index.html
//!
//! Groups:
//!   field/           — GF(2^257) arithmetic baselines
//!   sign/            — single signature (CT comb)
//!   verify/          — cold-cache vs warm-cache single verify
//!   batch_verify/    — N verifications, same key vs mixed keys
//!   batch_fast/      — batch_verify_fast (early-exit) same key

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use prro_crypto_v2::{
    batch_verify, batch_verify_fast,
    core::{
        curve::Curve,
        field::FieldEl,
        point::Point,
        sign::{sign, verify, Signature},
    },
    BatchItem,
};

// ── helpers ───────────────────────────────────────────────────────────────────

fn prng_step(x: &mut u32) -> u32 {
    *x ^= *x << 13;
    *x ^= *x >> 17;
    *x ^= *x << 5;
    *x
}

/// Deterministic 257-bit field element. High word kept ≤ 1 (≤ 2^257).
fn make_fe(seed: u32, mw: usize) -> FieldEl {
    let mut x = seed | 1; // ensure non-zero seed
    let mut v = vec![0u32; mw];
    for w in v.iter_mut() {
        *w = prng_step(&mut x);
    }
    v[mw - 1] = 0; // keep word 8 zero so scalar stays < 2^256
    FieldEl::from_words(v)
}

fn base_point(curve: &Curve) -> Point {
    Point::new(curve.base_x.clone(), curve.base_y.clone())
}

fn make_pubkey(curve: &Curve, d: &FieldEl) -> Point {
    base_point(curve).mul(d, curve)
}

struct Fixture {
    pubkey: Point,
    hash: FieldEl,
    sig: Signature,
}

/// Build a valid (pubkey, hash, signature) triple, retrying if sign() returns
/// None (degenerate nonce — very rare but possible with random seeds).
fn make_fixture(curve: &Curve, key_seed: u32, hash_seed: u32, nonce_seed: u32) -> Fixture {
    let mw = curve.mod_words;
    let d = make_fe(key_seed, mw);
    let hash = make_fe(hash_seed, mw);
    let pubkey = make_pubkey(curve, &d);

    let mut ns = nonce_seed | 1;
    loop {
        let e = make_fe(ns, mw);
        if let Some(sig) = sign(curve, &d, &hash, &e) {
            return Fixture { pubkey, hash, sig };
        }
        ns = ns.wrapping_add(0x1337); // advance nonce on degenerate case
    }
}

// ── field arithmetic baselines ────────────────────────────────────────────────

fn bench_field(c: &mut Criterion) {
    use prro_crypto_v2::core::fe::Fe;

    let a = Fe::from_hex("01CEF494720115657E18F938D7A7942394FF9425C1458C57861F9EEA6ADBE3BE10");
    let b = Fe::from_hex("2A29EF207D0E9B6C55CD260B306C7E007AC491CA1B10C62334A9E8DCD8D20FB7");

    let mut g = c.benchmark_group("field");
    g.bench_function("mul_257", |b_| {
        b_.iter(|| black_box(a).mod_mul(&black_box(b)))
    });
    g.bench_function("sqr_257", |b_| b_.iter(|| black_box(a).mod_sqr()));
    g.bench_function("inv_257", |b_| b_.iter(|| black_box(a).invert()));
    g.finish();
}

// ── sign ──────────────────────────────────────────────────────────────────────

fn bench_sign(c: &mut Criterion) {
    let curve = Curve::dstu_pb_257();
    let mw = curve.mod_words;
    let d = make_fe(0xDEAD_BEEF, mw);
    let hash = make_fe(0xCAFE_1234, mw);
    let e = make_fe(0xABCD_1234, mw);

    c.benchmark_group("sign")
        .throughput(Throughput::Elements(1))
        .bench_function("dstu_pb_257", |b| {
            b.iter(|| {
                sign(
                    black_box(&curve),
                    black_box(&d),
                    black_box(&hash),
                    black_box(&e),
                )
            })
        });
}

// ── verify ────────────────────────────────────────────────────────────────────

fn bench_verify(c: &mut Criterion) {
    let curve = Curve::dstu_pb_257();

    // Pre-generate 64 distinct fixtures with different keys so cold-cache bench
    // can cycle through them without recomputing inside the hot loop.
    let cold_fixtures: Vec<Fixture> = (0u32..64)
        .map(|i| {
            make_fixture(
                &curve,
                0xABCD_0000 + i * 31,
                0xCAFE_0000 + i,
                0xDEAD_0000 + i,
            )
        })
        .collect();

    // One fixture for warm-cache (same pubkey every call → Q_CACHE always warm).
    let warm = make_fixture(&curve, 0xDEAD_BEEF, 0xCAFE_1234, 0xABCD_0001);
    // Trigger Q_CACHE warm-up before the bench loop.
    let _ = verify(&curve, &warm.pubkey, &warm.hash, &warm.sig);

    let mut g = c.benchmark_group("verify");
    g.throughput(Throughput::Elements(1));

    g.bench_function("cold_key", |b| {
        let mut idx = 0usize;
        b.iter(|| {
            let f = &cold_fixtures[idx % cold_fixtures.len()];
            idx = idx.wrapping_add(1);
            verify(
                black_box(&curve),
                black_box(&f.pubkey),
                black_box(&f.hash),
                black_box(&f.sig),
            )
        })
    });

    g.bench_function("warm_key", |b| {
        b.iter(|| {
            verify(
                black_box(&curve),
                black_box(&warm.pubkey),
                black_box(&warm.hash),
                black_box(&warm.sig),
            )
        })
    });

    g.finish();
}

// ── batch_verify — same key ───────────────────────────────────────────────────

fn bench_batch_same_key(c: &mut Criterion) {
    let curve = Curve::dstu_pb_257();
    const SIZES: &[usize] = &[1, 4, 8, 16, 32, 64];

    // All from the same device (same private key d=0xDEAD_BEEF).
    let fixtures: Vec<Fixture> = (0u32..64)
        .map(|i| {
            make_fixture(
                &curve,
                0xDEAD_BEEF,
                0xCAFE_0000 + i,
                0xABCD_0000 + i * 7 + 1,
            )
        })
        .collect();

    let mut g = c.benchmark_group("batch_verify/same_key");
    for &n in SIZES {
        g.throughput(Throughput::Elements(n as u64));
        g.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            let items: Vec<BatchItem<'_>> = fixtures[..n]
                .iter()
                .map(|f| BatchItem {
                    pub_q: &f.pubkey,
                    hash: &f.hash,
                    sig: &f.sig,
                })
                .collect();
            b.iter(|| batch_verify(black_box(&items), black_box(&curve)))
        });
    }
    g.finish();
}

// ── batch_verify — distinct keys ─────────────────────────────────────────────

fn bench_batch_mixed_keys(c: &mut Criterion) {
    let curve = Curve::dstu_pb_257();
    const SIZES: &[usize] = &[1, 4, 8, 16, 32];

    // Each fixture uses a different private key.
    let fixtures: Vec<Fixture> = (0u32..32)
        .map(|i| {
            make_fixture(
                &curve,
                0xDEAD_0000 + i * 37 + 1,
                0xCAFE_0000 + i,
                0xABCD_0000 + i * 13 + 1,
            )
        })
        .collect();

    let mut g = c.benchmark_group("batch_verify/mixed_keys");
    for &n in SIZES {
        g.throughput(Throughput::Elements(n as u64));
        g.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            let items: Vec<BatchItem<'_>> = fixtures[..n]
                .iter()
                .map(|f| BatchItem {
                    pub_q: &f.pubkey,
                    hash: &f.hash,
                    sig: &f.sig,
                })
                .collect();
            b.iter(|| batch_verify(black_box(&items), black_box(&curve)))
        });
    }
    g.finish();
}

// ── batch_verify_fast ─────────────────────────────────────────────────────────

fn bench_batch_fast(c: &mut Criterion) {
    let curve = Curve::dstu_pb_257();
    const SIZES: &[usize] = &[4, 16, 64];

    let fixtures: Vec<Fixture> = (0u32..64)
        .map(|i| {
            make_fixture(
                &curve,
                0xDEAD_BEEF,
                0xCAFE_0000 + i,
                0xABCD_0000 + i * 7 + 1,
            )
        })
        .collect();

    let mut g = c.benchmark_group("batch_fast/same_key");
    for &n in SIZES {
        g.throughput(Throughput::Elements(n as u64));
        g.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            let items: Vec<BatchItem<'_>> = fixtures[..n]
                .iter()
                .map(|f| BatchItem {
                    pub_q: &f.pubkey,
                    hash: &f.hash,
                    sig: &f.sig,
                })
                .collect();
            b.iter(|| batch_verify_fast(black_box(&items), black_box(&curve)))
        });
    }
    g.finish();
}

// ── criterion wiring ──────────────────────────────────────────────────────────

criterion_group!(
    benches,
    bench_field,
    bench_sign,
    bench_verify,
    bench_batch_same_key,
    bench_batch_mixed_keys,
    bench_batch_fast,
);
criterion_main!(benches);
