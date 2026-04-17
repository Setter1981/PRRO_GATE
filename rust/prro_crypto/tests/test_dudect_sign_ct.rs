//! Dudect-style timing sanity test for the DSTU 4145 signing path.
//!
//! ## What this is
//!
//! A *sanity* check, not a formal constant-time proof. It runs the full
//! `core::sign::sign` function over two input classes and applies Welch's
//! t-test to the timing samples. A large |t| over a large sample size is
//! evidence that timing correlates with the varying input — almost always
//! a secret-dependent side channel worth investigating.
//!
//! The methodology follows Reparaz/Balasch/Verbauwhede 2017 (the "dudect"
//! paper). This is a much weaker guarantee than formal verification; an
//! adversary with a better timing oracle can still find leaks dudect
//! doesn't. Conversely, a clean dudect run doesn't prove CT — it proves
//! no leak was detected at the resolution of this harness, on this host,
//! during this run. Expert guidance (Sprint 2 plan): rerun on multiple
//! sessions before drawing conclusions.
//!
//! ## Classes
//!
//! Three separate tests, each comparing a fixed-input class against a
//! varying-input class. The input that varies is the suspected secret:
//!
//! - `dudect_d_sensitivity`: private key `d` varies in class B, `e` and
//!   `hash` fixed. Detects timing correlated to `d`.
//! - `dudect_e_sensitivity`: ephemeral `e` varies in class B, `d` and
//!   `hash` fixed. Detects timing correlated to `e`.
//! - `dudect_paired_sensitivity`: both `d` and `e` vary — the realistic
//!   signing scenario. Detects any end-to-end leak.
//!
//! ## Running
//!
//! ```
//! cargo test --release --test test_dudect_sign_ct -- --ignored --nocapture
//! ```
//!
//! The tests are `#[ignore]`d by default — they run for ~20 s each and
//! need a quiet machine to give stable numbers. In CI we don't run them
//! automatically; treat this as a manual hardening check, not a gate.
//!
//! ## Interpretation
//!
//! - `|t| < 4.5`: within the dudect paper's "no detectable leak" band.
//! - `4.5 ≤ |t| < 10`: potential leak — rerun on a quiet machine across
//!   several sessions before taking it seriously. System noise (thermal
//!   throttling, interrupt bursts, neighbours on the same core) can push
//!   `|t|` up without any actual secret leak.
//! - `|t| ≥ 10` consistently: highly likely real leak. Investigate.

use std::time::Instant;

use prro_crypto::{
    core::sign::sign,
    Curve, FieldEl,
};

const WARMUP_ITERS: usize = 2_000;
const MEASUREMENT_ITERS: usize = 80_000;
const CROP_PERCENTILE: f64 = 0.01; // drop top 1 % to absorb interrupts

/// Conservative automated gate. Paper's "suspicious" threshold is 4.5,
/// but on a busy dev laptop noise alone drives |t| into the 5-8 range;
/// we fail only on the clearly-leak end of the scale. The surrounding
/// docs explain why a softer signal still deserves a manual rerun.
const AUTOMATED_FAIL_T: f64 = 10.0;

/// PRNG state for generating inputs. Not cryptographic — dudect inputs
/// need to span the space, not be secret.
struct Xorshift(u64);

impl Xorshift {
    fn new(seed: u64) -> Self {
        // Avoid the fixed-point at zero.
        Xorshift(if seed == 0 { 0xDEAD_BEEF_CAFE_BABE } else { seed })
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn random_fe(&mut self, mod_words: usize) -> FieldEl {
        // Fill only words 0..=7 (bits 0..255). Word 8 (bit 256+) stays
        // zero so the generated scalar is inside the Sprint 2.2 sign()
        // contract `rand_e < 2^256`. Filling all 9 words would push the
        // scalar above 2^256 and sign() would correctly reject it — the
        // dudect harness would then measure the rejection fast-path
        // instead of the real signing path.
        let mut w = vec![0u32; mod_words];
        for i in 0..8.min(mod_words) {
            w[i] = self.next_u64() as u32;
        }
        FieldEl::from_words(w)
    }
}

/// Two-class timing collector.
///
/// `builder` is called once per sample and returns `(class_tag, inputs)`.
/// Class tag is 0 or 1; the sign path is measured, timing sorted by class.
fn collect_timings<F>(
    curve: &Curve,
    mut builder: F,
    iters: usize,
) -> (Vec<u128>, Vec<u128>)
where
    F: FnMut(&mut Xorshift) -> (u8, FieldEl, FieldEl, FieldEl),
{
    let mut prng = Xorshift::new(0xAAAA_BBBB_CCCC_DDDD);
    let mut class_a: Vec<u128> = Vec::with_capacity(iters / 2);
    let mut class_b: Vec<u128> = Vec::with_capacity(iters / 2);

    // Warm-up. Runs the full sign path so CPUID dispatch, PCLMULQDQ
    // detection cache, fixed-base table construction, and branch
    // predictor all settle before the first measured sample.
    for _ in 0..WARMUP_ITERS {
        let (_, d, hash, e) = builder(&mut prng);
        let _ = std::hint::black_box(sign(curve, &d, &hash, &e));
    }

    // Measurement. Alternate class A and B by asking the builder; this
    // keeps slow long-scale system drift (thermal, other processes) from
    // biasing one class.
    for _ in 0..iters {
        let (tag, d, hash, e) = builder(&mut prng);
        let t0 = Instant::now();
        let out = sign(curve, &d, &hash, &e);
        let dt = t0.elapsed().as_nanos();
        std::hint::black_box(out); // stop LLVM from DCE-ing the call
        if tag == 0 {
            class_a.push(dt);
        } else {
            class_b.push(dt);
        }
    }

    // Crop top CROP_PERCENTILE of each class — interrupt/thermal outliers
    // would otherwise dominate the variance term.
    let crop = |mut v: Vec<u128>| {
        v.sort_unstable();
        let keep = ((v.len() as f64) * (1.0 - CROP_PERCENTILE)) as usize;
        v.truncate(keep);
        v
    };
    (crop(class_a), crop(class_b))
}

/// Welch's t-statistic for two unequal-variance samples.
fn welch_t(a: &[u128], b: &[u128]) -> f64 {
    let (mean_a, var_a) = mean_var(a);
    let (mean_b, var_b) = mean_var(b);
    let n_a = a.len() as f64;
    let n_b = b.len() as f64;
    let denom = (var_a / n_a + var_b / n_b).sqrt();
    if denom == 0.0 {
        return 0.0;
    }
    (mean_a - mean_b) / denom
}

fn mean_var(xs: &[u128]) -> (f64, f64) {
    let n = xs.len() as f64;
    let mean = xs.iter().map(|&x| x as f64).sum::<f64>() / n;
    let var = xs
        .iter()
        .map(|&x| {
            let d = x as f64 - mean;
            d * d
        })
        .sum::<f64>()
        / (n - 1.0);
    (mean, var)
}

fn log_result(tag: &str, a: &[u128], b: &[u128]) {
    let t = welch_t(a, b);
    let (mean_a, _) = mean_var(a);
    let (mean_b, _) = mean_var(b);
    eprintln!(
        "[dudect:{tag}] samples A={} B={} mean_A={:.0} ns mean_B={:.0} ns  t = {:+.2}",
        a.len(),
        b.len(),
        mean_a,
        mean_b,
        t
    );
    assert!(
        t.abs() < AUTOMATED_FAIL_T,
        "[dudect:{tag}] |t| = {:.2} exceeds automated fail threshold {:.1}. \
         This likely indicates a real secret-dependent timing leak. Rerun \
         on a quiet machine to rule out system noise; if persistent, audit \
         recent changes to the sign path.",
        t.abs(),
        AUTOMATED_FAIL_T
    );
}

/// Fixed `e` and `hash`, variable `d` in class B. Detects timing that
/// correlates with the private key.
#[test]
#[ignore]
fn dudect_d_sensitivity() {
    let curve = Curve::dstu_pb_257();
    let mw = curve.mod_words;
    let fixed_d = FieldEl::from_hex(
        "DEADBEEFCAFE12345678900000000000AAAA5555AAAA5555DEADBEEFCAFEBABE",
        mw,
    );
    let fixed_e = FieldEl::from_hex(
        "0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF",
        mw,
    );
    let fixed_hash = FieldEl::from_hex(
        "FEDCBA9876543210FEDCBA9876543210FEDCBA9876543210FEDCBA9876543210",
        mw,
    );

    let (a, b) = collect_timings(
        &curve,
        |prng| {
            let tag = (prng.next_u64() & 1) as u8;
            let d = if tag == 0 {
                fixed_d.clone()
            } else {
                prng.random_fe(mw)
            };
            (tag, d, fixed_hash.clone(), fixed_e.clone())
        },
        MEASUREMENT_ITERS,
    );
    log_result("d_sensitivity", &a, &b);
}

/// Fixed `d` and `hash`, variable `e` in class B. Detects timing that
/// correlates with the ephemeral scalar (the Montgomery-ladder input).
#[test]
#[ignore]
fn dudect_e_sensitivity() {
    let curve = Curve::dstu_pb_257();
    let mw = curve.mod_words;
    let fixed_d = FieldEl::from_hex(
        "DEADBEEFCAFE12345678900000000000AAAA5555AAAA5555DEADBEEFCAFEBABE",
        mw,
    );
    let fixed_e = FieldEl::from_hex(
        "0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF",
        mw,
    );
    let fixed_hash = FieldEl::from_hex(
        "FEDCBA9876543210FEDCBA9876543210FEDCBA9876543210FEDCBA9876543210",
        mw,
    );

    let (a, b) = collect_timings(
        &curve,
        |prng| {
            let tag = (prng.next_u64() & 1) as u8;
            let e = if tag == 0 {
                fixed_e.clone()
            } else {
                prng.random_fe(mw)
            };
            (tag, fixed_d.clone(), fixed_hash.clone(), e)
        },
        MEASUREMENT_ITERS,
    );
    log_result("e_sensitivity", &a, &b);
}

/// Both `d` and `e` vary — the realistic production signing scenario.
/// Class A always signs the same fixed triple; class B draws fresh
/// random `(d, e)` each iteration. End-to-end leak detector.
#[test]
#[ignore]
fn dudect_paired_sensitivity() {
    let curve = Curve::dstu_pb_257();
    let mw = curve.mod_words;
    let fixed_d = FieldEl::from_hex(
        "DEADBEEFCAFE12345678900000000000AAAA5555AAAA5555DEADBEEFCAFEBABE",
        mw,
    );
    let fixed_e = FieldEl::from_hex(
        "0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF",
        mw,
    );
    let fixed_hash = FieldEl::from_hex(
        "FEDCBA9876543210FEDCBA9876543210FEDCBA9876543210FEDCBA9876543210",
        mw,
    );

    let (a, b) = collect_timings(
        &curve,
        |prng| {
            let tag = (prng.next_u64() & 1) as u8;
            let (d, e) = if tag == 0 {
                (fixed_d.clone(), fixed_e.clone())
            } else {
                (prng.random_fe(mw), prng.random_fe(mw))
            };
            (tag, d, fixed_hash.clone(), e)
        },
        MEASUREMENT_ITERS,
    );
    log_result("paired_sensitivity", &a, &b);
}
