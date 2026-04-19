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

# ── Load a key from ANY supported container (JKS / PFX / ZS2 / Key-6.dat) ──
data = open("key.zs2", "rb").read()
fmt  = prro_crypto.detect_container_format(data)       # "jks" | "key6" | "pfx" | None
ek   = prro_crypto.extract_private_key(data, "password")
# ek = {"format": "pfx", "param_d_hex": "…", "certs": [b"…", …]}

# ── Production CMS/CAdES-BES detached signature ────────────────────────────
p7s = prro_crypto.cms_sign_detached(
    param_d_hex = ek["param_d_hex"],
    cert_der    = ek["certs"][0],
    content     = b"receipt payload",
)

# ── EnvelopedData decrypt (for reading ДПС responses) ──────────────────────
plain = prro_crypto.cms_decrypt_envelope(
    param_d_hex         = ek["param_d_hex"],
    envelope_der        = open("response.p7", "rb").read(),
    originator_cert_der = sender_cert_bytes,
)

# ── Auto-fetch signing cert from Ukrainian CA by SKI ───────────────────────
qx, qy = prro_crypto.pubkey_dstu_pb_257(ek["param_d_hex"])
compressed = prro_crypto.compress_pubkey(qx, qy)
ski = prro_crypto.compute_ski(compressed)
cert = prro_crypto.fetch_cert_by_ski("http://acskidd.gov.ua/services/cmp/", ski)

# ── Low-level: sign + verify ───────────────────────────────────────────────
r, s = prro_crypto.sign_with_osrng(ek["param_d_hex"], hash_hex)
ok   = prro_crypto.verify_dstu_pb_257(pub_x, pub_y, hash_hex, r, s)
```

## Status

Current version is `0.1.0-alpha.1` — **production-pilot-acceptable under the documented threat model and scope** (see [`SECURITY.md`](SECURITY.md) and "Known limitations" below). Not a general-purpose Ukrainian crypto toolkit.

**Implemented and covered by tests + live round-trips:**

- DSTU 4145 sign/verify with byte-identical output vs jkurwa on the shipped vector set
- GOST 34.311-95 hash
- GOST 28147-89 block cipher (ECB + CFB) with DSTU DKU S-box
- CMS/CAdES — **BES** (detached + attached), **T** (TSP timestamp), **LT** (revocation-values)
- EnvelopedData decrypt (ECDH cofactor-DH + GOST key-unwrap + CFB)
- Container readers: JKS (Privat), PFX including `.zs2` (АЦСК "Україна"), Key-6.dat (ІІТ/ДПС)
- IIT cert-lookup-by-SKI client (reverse-engineered from `dstucrypt/agent`), live-tested against **acskidd.gov.ua**, **uakey.com.ua**, **acsk.privatbank.ua**
- TSP / OCSP / CRL HTTP clients
- Constant-time scalar multiplication (López-Dahab x-only Montgomery ladder) for both base-point and arbitrary-point paths — covers signing `rand_e` and ECDH `d·h` secret scalars
- Public-point validation (on-curve + prime-order subgroup + infinity) in `verify()` and ECDH
- `zeroize`-on-drop for all secret-bearing types (`FieldEl`, `Scalar`, `DstuInProcessSigner`, `PfxParsed`, `Key6Parsed`, `ExtractedKey`, `JksEntry`)
- PyO3 bindings with GIL released across all HTTP round-trips and heavy crypto

**Deliberately NOT implemented** — see "Known limitations":

- Kupyna (DSTU 7564) hash — parked until a real Kupyna-issued key reaches a user
- Full PKI lifecycle (cert issuance, revocation, cross-certification)
- OCSP/CRL/TSP response signature verification — the crate embeds bytes, callers validate
- Key-6 MAC verification and PFX `macData` verification (intentional compromises documented in `SECURITY.md`)

## Performance

Run benchmarks on the pinned toolchain:

```bash
cargo bench --bench crypto
```

Key number for production sizing: a full CMS-BES sign (GOST 34.311
digest + constant-time scalar mul + DER assembly) takes ~4-6 ms on a
modern x86_64 core. For a per-cashdesk gateway signing one receipt
every ~30 s, this is several thousand times headroom.

The CT Montgomery ladder costs roughly 2x vs the variable-time wNAF
path — a deliberate trade for side-channel safety. Future targets:
PCLMULQDQ SIMD backend (already shipped in portable/x86 dispatch) and
an optional ARM64 PMULL backend for POS-class hardware.

## Security posture

Full threat model, acceptance scope, and known compromises are in [`SECURITY.md`](SECURITY.md). Summary:

- Constant-time Montgomery ladder for secret-scalar × point multiplication on the **signing** (`rand_e × G`) and **ECDH** (`d·h × Q`) hot paths only. Other helpers that touch the private scalar — notably `pubkey_dstu_pb_257(d_hex)` — use the **variable-time** wNAF path and are **explicitly outside the CT scope**.
- **Not CT-protected:** `pubkey_dstu_pb_257()` computes `Q = -d·G` via wNAF. The private scalar `d` traverses variable-time table lookups. This is a measurable timing side-channel on `d` during the call. The function is intended for onboarding / provisioning (called once per key lifetime), not for the per-receipt signing hot path — but it does expose `d` to timing observation for the duration of that single call. Callers in timing-sensitive environments should be aware of this limitation.
- Itoh-Tsujii constant-time field inversion on the hot paths.
- Masked conditional-swap via `subtle::ConditionallySelectable`.
- `rust-toolchain.toml` pins `rustc` so the CT guarantees don't drift across compiler versions.
- dudect harness (`cargo test --release --test test_dudect_sign_ct -- --ignored`) with `|t| < 3.5` on three independent sessions at three input classes.
- `#[non_exhaustive]` on every public error enum; `zeroize` + redacted custom `Debug` on every secret-bearing type. **Caveat:** `Scalar` is `Copy` (required by the ladder's pass-by-value semantics), which precludes `Drop`-based auto-wipe. It implements `Zeroize` for manual `.zeroize()` calls but secret `Scalar` temporaries on the stack are NOT auto-zeroed — they rely on the OS zeroing freed stack frames, which is best-effort. `FieldEl` and container output structs (`PfxParsed`, `Key6Parsed`, `ExtractedKey`, `JksEntry`) do auto-wipe via `Zeroizing<…>` / `ZeroizeOnDrop`.
- `unsafe` exists only in `core::backend::x86_pclmul.rs` for the PCLMULQDQ SIMD intrinsics behind a CPUID guard. The rest of the crate is safe Rust. No system OpenSSL.

Three consecutive internal audit passes + a final product-risk sweep (2026-04-16). All critical / high findings closed; audit log lives in `../../prro_crypto_chunks/PHASE_4_BACKLOG.md` for traceability.

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

226 unit tests + integration tests + 3 live CA round-trips, all green on the pinned toolchain. Live tests are `#[ignore]`-gated so CI doesn't depend on external CAs. Run them explicitly when validating a release:

```bash
cargo test --release
cargo test --release --features legacy_jkurwa_interop
cargo test --release --no-default-features --features python
cargo test --release --lib live_ -- --ignored  # network required
```

Test families:

- **core::gf2m / fe / scalar / field** — arithmetic primitives + jkurwa vector replay
- **core::point / mladder / wnaf / proj** — EC primitives + ladder differential vs wNAF on random valid Q
- **core::sign** — sign/verify roundtrip + audit-driven regressions (off-curve Q, oversized scalar, malformed width)
- **core::hash** — GOST 34.311 vector replay
- **cms::*** — TLV primitives, DER roundtrip, EnvelopedData decrypt against jkurwa fixture → `"123"`, CAdES-T/LT attribute embedding, IIT CMP wire format
- **interop::prro::*** — JKS / PFX / ZS2 / Key-6 parse + cross-validation of param_d between containers for the same key
- **live CA (ignored)** — round-trip cert lookup against acskidd / uakey / acsk.privatbank

## Architecture

```
prro_crypto/src/
├── core/           — universal DSTU 4145 primitives
│   ├── gf2m        — low-level GF(2^m) polynomial arithmetic
│   ├── field       — FieldEl (GF(2^m) element wrapper)
│   ├── fe          — fixed-size PB-257 specialisation
│   ├── scalar      — 256-bit scalar arithmetic mod curve order (Barrett)
│   ├── curve       — DSTU_PB_257 parameters
│   ├── point       — affine points + checked decompression
│   ├── mladder     — López-Dahab x-only Montgomery ladder (CT, secret scalars)
│   ├── sign        — DSTU 4145 sign + verify with full pubkey validation
│   ├── hash        — GOST 34.311-95
│   └── backend     — CPU-feature-dispatched multiplication (PCLMULQDQ + portable)
├── cms/            — CMS/CAdES + IIT-Ukrainian network protocols
│   ├── builder     — BES / T / LT assembly
│   ├── envelope    — EnvelopedData parse + decrypt (ECDH + keywrap + CFB)
│   ├── tsp         — RFC 3161 TimeStamp client (feature = tsp_http)
│   ├── revocation  — OCSP + CRL ASN.1 + HTTP clients
│   ├── cmp         — IIT proprietary cert-lookup-by-SKI (reverse-engineered)
│   ├── asn1_util   — shared bounded DER primitives used across cms/*
│   └── ...
├── interop/prro/   — Ukrainian-specific container / encoding support
│   ├── jks / pfx / key6   — encrypted container readers
│   ├── containers         — unified dispatcher
│   └── pbe                — GOST PBKDF2 / CFB / keywrap / MAC
└── python.rs       — PyO3 bindings (feature = python)
```

## Known limitations

Поточна версія має кілька свідомо прийнятих обмежень:

- **Key-6 MAC не верифікується.**
  Неправильний пароль або пошкоджений вміст зазвичай виявляються на етапі внутрішнього ASN.1/DER parse, але це не є повною перевіркою цілісності контейнера.

- **PFX `macData` наразі ігнорується.**
  Тому import PFX не слід трактувати як повну зовнішню integrity verification контейнера.

- **Бібліотека не покриває повний PKI lifecycle.**
  Scope обмежений crypto/CMS/container/interop задачами, потрібними для прикладних workflow.

- **Підтримка зосереджена на задокументованих українських workflow.**
  Нестандартні, історичні або vendor-specific edge cases поза цим scope можуть працювати в режимі best effort або не підтримуватись.

- **Transport layer не входить у security scope crate.**
  Бібліотека повертає криптографічні артефакти та helper-функції; мережевий протокол, retry/logging/idempotency/operational policy живуть у верхньому шарі.

- **Це не універсальна "українська криптоплатформа на всі випадки життя".**
  Це вузько сфокусований стек для реальних бізнес-флоу, насамперед PRRO та суміжних сценаріїв.

Розширений security policy і threat model — див. [`SECURITY.md`](SECURITY.md).

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
