# prro_crypto

**Native Rust DSTU 4145 (Ukrainian elliptic curve cryptography) for fiscal applications.**

Replaces the Node.js [jkurwa](https://github.com/dstucrypt/jkurwa) sidecar
with an in-process Rust library. Python bindings via PyO3.

## Installation

```bash
pip install prro-crypto    # Linux x86_64, ARM64, macOS, Windows wheels
```

Or build from source:
```bash
maturin build --release
pip install target/wheels/prro_crypto-*.whl
```

## Quick start

```python
import prro_crypto

# Read a JKS keystore (Ukrainian production-format)
d_hex = prro_crypto.read_jks_param_d("key.jks", "password")

# Compute public key Q = -d*G (jkurwa convention)
pub_x, pub_y = prro_crypto.pubkey_dstu_pb_257(d_hex)

# Sign a hash with caller-supplied random scalar
r, s = prro_crypto.sign_dstu_pb_257(d_hex, hash_hex, rand_e_hex)

# Verify a signature
ok = prro_crypto.verify_dstu_pb_257(pub_x, pub_y, hash_hex, r, s)

# Read certificate chain from JKS
alias, certs_der = prro_crypto.read_jks_certs("key.jks", "password")
```

## Status

| Step | Module | Status |
|------|--------|--------|
| 1 | GF(2^m) polynomial arithmetic (`gf2m`) | ✅ 170 jkurwa byte-identical vectors |
| 2 | Field wrapper, DSTU curve params, point arithmetic | ✅ 14 jkurwa vectors + algebraic n*G=∞ |
| 3 | DSTU 4145 sign/verify | ✅ 6 byte-identical vs jkurwa |
| 4 | JKS reader + DER parser | ✅ Reads real production JKS |
| 5 | **End-to-end JKS → sign with real key** | ✅ Byte-identical (r,s) vs jkurwa |
| 6 | **Optimization: specialized fsqr (bit-spreading)** | ✅ **12x speedup for mod_sqr** |
| 7 | **Constant-time field equality** | ✅ Side-channel safe comparison |
| 8 | **PyO3 Python bindings** | ✅ Drop-in Python module |
| 9 | GOST 28147-89 cipher (for Key-6.dat / PFX) | Pending |
| 10 | CMS/PKCS#7 SignedData builder | Pending |
| 11 | Production-grade constant-time scalar mul | Pending |
| 12 | Scalar blinding, fault attack defense | Pending |

## Performance benchmarks

Measured on a single core (i9-class CPU equivalent, WSL2):

| Operation | Baseline | Round 2 | Speedup |
|-----------|----------|---------|---------|
| `gf2m::fmul` (257-bit) | 1.97 µs | 1.97 µs | — |
| `gf2m::fmod` (257-bit) | 144 ns | 144 ns | — |
| `gf2m::finv` (257-bit) | 22.5 µs | 22.5 µs | — |
| `field::mod_sqr` | 2.65 µs | **226 ns** | **12x** (fsqr bit-spread) |
| `field::mod_mul` | 1.87 µs | 1.87 µs | — |
| `field::invert` | 22.2 µs | 22.2 µs | — |
| `point::twice` (affine) | 30.3 µs | 30.3 µs | — |
| `point::add` (affine) | 28.4 µs | 28.4 µs | — |
| **`point::mul`** (full 256-bit scalar) | **11.7 ms** | **4.25 ms** | **2.7x** (Lopez-Dahab proj + wNAF) |
| **`sign::sign_full`** | **11.0 ms** | **4.6 ms** | **2.4x** |

Single-thread throughput: **~220 signs/sec** through the full Python wheel.
For per-cashdesk gateway (1 sign per ~30s) this is ~6500x more than required.

Optimizations applied in Round 2:
- **Specialized `fsqr`** — bit-spreading O(n) instead of `mul(x, x)` O(n²)
- **wNAF scalar mul** (window 4) — ~50% fewer additions
- **Lopez-Dahab projective coordinates** — eliminates per-step inversions
- **Constant-time field equality** — side-channel safe comparison

Future targets (Round 3): SIMD (PCLMULQDQ on x86, PMULL on AArch64) for fmul,
constant-time scalar mul (Montgomery ladder), Itoh-Tsujii inverse, scalar
blinding. Estimated: sign < 1 ms with constant-time guarantees.

## Security status

**This crate is `0.1.0-alpha`. Use for development and testing only.**

Production deployment requires:
- Constant-time scalar multiplication (Montgomery ladder)
- Constant-time field inversion (Itoh-Tsujii)
- Scalar blinding (defense vs fault injection)
- Audit by qualified cryptographer

Currently in place:
- Constant-time field equality (`FieldEl::equals`, `is_zero_ct`)
- No `unsafe` code
- Pure Rust (no system OpenSSL)
- Test vector parity with reference implementation

Currently NOT in place:
- Variable-time scalar mul (timing leaks bits of private key)
- Variable-time field invert (timing leaks bits of intermediate values)
- No protection against fault attacks
- No protection against malformed input on JKS / DER parsers (fuzz needed)

## Build

```bash
# Rust crate (optional Python feature)
cargo build --release
cargo test --release
cargo bench --bench crypto

# Python wheel
maturin build --release    # output: target/wheels/*.whl

# With Python feature for Rust tests
cargo test --release --features python
```

## Test coverage

41 tests across 6 suites:

- **gf2m**: 11 unit + 170 jkurwa byte-identical vectors
- **field**: 4 unit
- **curve**: 2 unit
- **point**: 5 unit + 14 jkurwa vectors + algebraic n*G=∞ proof
- **sign**: 4 unit + 6 jkurwa byte-identical vectors
- **jks + der**: 5 unit + real production JKS load
- **e2e**: real JKS → Rust sign produces byte-identical (r,s) with jkurwa

## Architecture

```
prro_crypto/
├── gf2m       — low-level GF(2^m) primitives (mul, fmod, finv, fsqr, blength)
├── field      — FieldEl struct: hex/word ctors, mod ops, invert, trace
├── curve      — Curve struct with DSTU_PB_257 hardcoded parameters
├── point      — Point + add/twice/negate/mul (affine, double-and-add)
├── sign       — DSTU 4145 sign/verify + truncate
├── jks        — JKS keystore reader (binary + SHA1 keystream)
├── der        — Minimal ASN.1 DER reader (just enough for DSTU privkey)
└── python     — PyO3 bindings (when feature = python)
```

## License

BSD 3-Clause. Based on jkurwa by Ilya Petrov (BSD).

## Why this exists

The Ukrainian PRRO ecosystem (programmatic cash registers reporting to the
State Tax Service) depends on jkurwa, a 10-year-old JavaScript library
maintained by one volunteer. Every fiscal solution in Ukraine ships its own
Node.js sidecar to call it.

This crate is a contribution toward modernizing that infrastructure:
- Single binary deployment (no Node.js sidecar)
- Native ARM and x86 — runs on retail edge hardware
- Memory-safe (Rust)
- Cross-platform (Linux, Windows, macOS, Android)
- Open source, BSD, on crates.io

For PRRO Gateway specifically, this enables:
- Drop-in replacement of the Node.js sidecar
- Lower latency (no HTTP roundtrip for signing)
- Smaller deployment footprint
- Easier installation and updates
