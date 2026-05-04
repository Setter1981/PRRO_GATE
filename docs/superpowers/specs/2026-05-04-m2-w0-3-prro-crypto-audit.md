# M2-W0-3 Findings — prro_crypto API Audit & CryptoProvider Trait Shape

Status: final
Date: 2026-05-04
ADR effect: confirms ADR-M2-1, ADR-M2-5, ADR-M2-6 unchanged

Owner: M2 W0 mini-plan task #20 (controller-finished after two
arch-planner subagent dispatches paused before write).
Inputs reviewed:
- `rust/prro_crypto/Cargo.toml`
- `rust/prro_crypto/src/lib.rs:1-12` (root pub surface)
- `rust/prro_crypto/src/cms/{builder.rs, cmp.rs, envelope.rs, signer.rs, revocation.rs, tsp.rs, attrs.rs, asn1_util.rs, der_writer.rs, profile.rs}`
- `rust/prro_crypto/src/interop/prro/{containers.rs, der.rs, jks.rs, key6.rs, pbe.rs, pfx.rs, legacy_sign.rs}`
- `rust/prro_crypto/src/core/{curve.rs, field.rs, fe.rs, point.rs, sign.rs, comb.rs, fixed_base.rs, gf2m.rs, backend/}`
- consumer surfaces: `rust/prro_sidecar/src/bin/{prro_sidecar.rs, prro_admin.rs, prro_license_keygen.rs, prro_license_sign.rs, prro_sidecar_preflight.rs}`, `rust/prro_crypto_v2/`
- `docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md` (ADR-M2-1, -5, -6)
- `docs/superpowers/specs/2026-05-04-m2-w0-1-dps-wire.md`, `2026-05-04-m2-w0-2-cmp-probe.md`

## Headline finding (read this first)

`prro_crypto` already covers every cryptographic operation `prro::crypto`
needs in M2.  No breaking extensions, no signature-shaping changes.
The wrapper trait is a thin async facade with `tokio::task::spawn_blocking`
at the boundary; the existing blocking C-FFI signing path is left
untouched.

This makes ADR-M2-1 (in-process wrapper, no sidecar HTTP) trivial to
land in W1+.  The risk that motivated the ADR's "open risk" language —
"`prro_crypto` API surface may need extension" — is essentially closed:
extensions are limited to small additive items (an `async`-friendly
batched sign helper if convenient; a public `Zeroizing` constructor for
unsealed key material if not already exposed).  Both are non-breaking.

---

## 1. Existing prro_crypto public API used by prro::crypto

| Operation | prro_crypto symbol | File:line | What `prro::crypto` uses it for |
|---|---|---|---|
| **Sign (DSTU raw)** | `core::sign::sign` (re-exported at root) | `src/lib.rs:9` | Raw DSTU 4145 signing primitive; backs the CMS signing path |
| **Verify (DSTU raw)** | `core::sign::verify` (re-exported) | `src/lib.rs:9` | KVT response signature verification + golden-test verification |
| **CMS detached sign** | `cms::builder::sign_detached_with_content_digest` | `src/cms/builder.rs:303` | Build the signed XML envelope `prro` posts to DPS |
| **CMS signer adapter** | `cms::signer::RawSigner` (trait, `Send + Sync`) | `src/cms/signer.rs:50` | Trait `prro::crypto` plugs into; lets tests inject a deterministic signer |
| **DSTU in-process signer** | `cms::signer::DstuInProcessSigner` | `src/cms/signer.rs:87` | Concrete `RawSigner` impl backed by `core::sign` |
| **CMS envelope decrypt** | `cms::envelope::{decrypt_with_params, unwrap_envelope, parse_envelope_params}` | `src/cms/envelope.rs:233,307,318` | Decrypt KVT2 / DPS-encrypted responses |
| **Compute SKI** | `cms::envelope::compute_ski` | `src/cms/envelope.rs:517` | Caller-side SKI re-check after CMP fetch (see W0-2) |
| **Extract cert pubkey** | `cms::envelope::extract_cert_pubkey_bytes` | `src/cms/envelope.rs:538` | Cert-chain validation in cert_refresher |
| **CMP cert lookup** | `cms::cmp::{encode_iit_cert_lookup, parse_iit_cert_response, fetch_cert_by_ski}` | `src/cms/cmp.rs:104,133,312` | Cert refresh path; `fetch_cert_by_ski` is the wire client (W0-2) |
| **TSP timestamp** | `cms::tsp::{encode_tsp_request, parse_tsp_response, fetch_timestamp, tsa_url_from_cert}` | `src/cms/tsp.rs:55,76,254,134` | Optional TSP timestamping in CMS envelope |
| **OCSP / CRL** | `cms::revocation::{fetch_ocsp_response, fetch_crl, parse_ocsp_status, ocsp_url_from_cert, crl_url_from_cert}` | `src/cms/revocation.rs:377,425,508,191,197` | Revocation checks on certs surfaced from CMP / signed-by chain |
| **JKS unseal** | `interop::prro::jks::{read_jks, JksEntry}` | `src/interop/prro/jks.rs` | Read JKS file + password → `JksEntry` (private key + cert) |
| **Container detection** | `interop::prro::containers::{detect_format, extract_private_key, ContainerFormat, ExtractedKey, ContainerError}` | `src/interop/prro/containers.rs` | Auto-detect JKS / PFX / Key6 / .dat container types and extract material |
| **PFX / Key6 / PBE** | `interop::prro::{pfx, key6, pbe}` | `src/interop/prro/{pfx,key6,pbe}.rs` | Fallback container formats and the GOST 28147 / PBKDF2 primitives they use |
| **Curve / FieldEl / Point / Signature** | `core::{curve, field, point, sign}` (re-exported at root) | `src/lib.rs:7-9` | Type system glue used by every signing call |
| **Warm-up** | `core::backend::warm_up` (re-exported) | `src/lib.rs:10` | Optional pre-compute of base-point combs at process start |

Coverage assessment: every operation enumerated by ADR-M2-1's
"Tests required" + W0-1's contract-subset table + W0-2's CMP path is
backed by an existing `prro_crypto` public symbol.

---

## 2. Existing consumers (compatibility baseline)

Real Rust consumers of `prro_crypto`'s public API outside its own
tests/benches/examples (greps run with `--exclude-dir=target`):

| Consumer | File | Symbols used |
|---|---|---|
| Production sidecar | `rust/prro_sidecar/src/bin/prro_sidecar.rs:25` | multi-symbol `use prro_crypto::{...}` block |
| Sidecar preflight | `rust/prro_sidecar/src/bin/prro_sidecar_preflight.rs:11` | multi-symbol `use prro_crypto::{...}` |
| Sidecar admin CLI | `rust/prro_sidecar/src/bin/prro_admin.rs:18` | `interop::prro::extract_private_key` |
| License keygen CLI | `rust/prro_sidecar/src/bin/prro_license_keygen.rs:37-39` | `core::{curve, field, point}` |
| License sign CLI | `rust/prro_sidecar/src/bin/prro_license_sign.rs:84-87` | `core::{curve, field, hash}` + `cms::signer::{DstuInProcessSigner, RawSigner}` |
| Crypto v2 benches | `rust/prro_crypto_v2/benches/crypto_v2.rs` | `prro_crypto::*` (benchmark parity surface) |

Compatibility rule: any change `prro::crypto` requires from
`prro_crypto` MUST NOT break the symbols listed above without a
documented migration plan (see §4).  Verified for the proposed
trait shape in §5: zero changes touch the symbols above.

(Note: a prior subagent observed "sidecar declares `prro_crypto` as a
dep but doesn't actually call it from Rust" — verified to be incorrect.
The 4 sidecar bins above are real Rust callers.  The Python sidecar
process itself goes through `python.rs` PyO3 bindings, which is a
separate consumer surface — see §3 deferred note.)

---

## 3. Extensions required (additive / signature-shaping / breaking)

| Extension | Classification | Rationale |
|---|---|---|
| `async` wrappers in `prro::crypto` itself | n/a — wrapper-side | `prro::crypto` wraps blocking calls in `tokio::task::spawn_blocking`; `prro_crypto` stays sync.  No change to `prro_crypto`. |
| Public `Zeroizing<...>` constructor for the unsealed-key bytes returned by `extract_private_key` | additive (likely already exposed via `ExtractedKey`) | ADR-M2-5 demands `Zeroizing` on plaintext key bytes.  If `ExtractedKey` already wraps them, this is a no-op; if not, expose as a new `pub fn into_zeroizing(self) -> Zeroizing<Vec<u8>>` method.  Either way, additive. |
| Manual `impl Debug` for `JksEntry`, `ExtractedKey`, `Key6Parsed`, `PfxParsed` (redacted) | additive (likely already done; verify in §6) | ADR-M2-5 §4: secret-bearing types must NOT `#[derive(Debug)]`.  If any of the above carry private-key bytes and currently derive `Debug`, replace with manual redacted `impl Debug` — that's a breaking change for any caller that pretty-printed the struct, but no consumer in §2 does. |
| `cms::cmp::fetch_cert_by_ski_async` | additive (optional) | Convenience: the wrapper can do `spawn_blocking` itself, so this is purely an ergonomic add.  Not required for W1+. |
| Cert-chain bundle accessor (issuer chain walk via OCSP/CRL URLs) | additive | `prro_crypto::cms::revocation::{ocsp_url_from_cert, crl_url_from_cert}` already give the URL accessors; an issuer-chain walker would be a new function in `services::cert_refresher` (NOT in `prro_crypto`).  No `prro_crypto` change required. |

**Total non-additive extensions: 0.**  All needs are met by the existing
public surface or by additive helpers.  ADR-M2-1's "open risk" note can
remain as-is — the audit confirms it but resolves the magnitude as
trivial.

---

## 4. Migration plan per non-additive extension

There are no non-additive extensions in §3.  Section retained for
structural parity with W0-1 / W0-2 docs.

If §6's redacted-`Debug` audit later reveals a current `#[derive(Debug)]`
on a secret-bearing type, the migration plan is:
1. Single PR.  Replace `#[derive(Debug)]` with manual `impl Debug` printing `"<redacted>"`.
2. Verify each consumer in §2 still compiles (none use `{:?}` on those types in `prro_sidecar/src/bin/`).
3. Land in `prro_crypto` first; `prro::crypto` consumes after.
This is documented here as the standing migration template should the
audit in §6 require it.

---

## 5. Proposed `CryptoProvider` trait shape

```rust
//! prro::crypto — in-process wrapper over prro_crypto.
//! Per ADR-M2-1, ADR-M2-5, ADR-M2-6.

use async_trait::async_trait;
use std::sync::Arc;
use zeroize::Zeroizing;

/// Opaque handle returned by `unseal_jks`.  Holds the unsealed private
/// key in `Zeroizing<...>`; dropped at end of crypto operation.
/// Manual `Debug` impl prints `"<redacted>"` (see §6).
pub struct SigningSession {
    inner: Arc<SigningSessionInner>,
}

struct SigningSessionInner {
    /// `Zeroizing<Vec<u8>>` — DSTU 4145 private scalar bytes.
    /// Never logged; never `Debug`-printed; zeroed on drop.
    key: Zeroizing<Vec<u8>>,
    /// Public cert DER (non-secret).
    cert_der: Vec<u8>,
    /// Curve params reference (non-secret).
    curve: prro_crypto::Curve,
}

/// Sealed material as it lives in DB (`sidecar_operators` row + JKS bytes).
/// Inputs to `unseal_jks` only — never crosses an `await` boundary in
/// plaintext form.
pub struct SealedMaterial<'a> {
    pub jks_bytes: &'a [u8],
    pub jks_password_hex: &'a str,
    pub cred_salt: &'a [u8; 16],
}

#[async_trait]
pub trait CryptoProvider: Send + Sync {
    /// Build a CMS-detached signed envelope around `request.canonical_xml`.
    /// Sync-on-the-inside, async-on-the-outside via `spawn_blocking`.
    async fn sign_cms_detached(
        &self,
        request: SignCmsRequest<'_>,
    ) -> Result<SignedCmsBytes, CryptoError>;

    /// Verify a DSTU 4145 raw signature.  Used for KVT response checks
    /// and golden-test verification; cheap enough that
    /// `spawn_blocking` is optional.
    async fn verify_dstu(
        &self,
        msg: &[u8],
        sig_bytes: &[u8],
        pubkey_compressed: &[u8],
    ) -> Result<bool, CryptoError>;

    /// Decrypt a CMS envelope (KVT2 / DPS-encrypted response).
    /// Takes a `SigningSession` that already holds the private key.
    async fn unwrap_envelope(
        &self,
        envelope_der: &[u8],
        session: &SigningSession,
    ) -> Result<Vec<u8>, CryptoError>;

    /// Fetch a cert by SKI from the IIT-proprietary CMP-look-alike
    /// channel (W0-2).  URLs come from `services::cert_refresher`,
    /// which loaded them from `cert_provisioning_config` / `ca_endpoints`
    /// in services-layer code — NEVER from a DB handle inside this
    /// trait (ADR-M2-6).
    async fn fetch_cert_by_ski(
        &self,
        urls: &[String],
        ski: &[u8; 32],
    ) -> Result<CertDer, CryptoError>;
}

/// Boundary function — NOT a trait method.  Lives in `prro::crypto::session`
/// or similar.  Takes sealed material and returns an opaque session.
/// Plaintext key bytes never escape this function.
pub fn unseal_jks(sealed: SealedMaterial<'_>) -> Result<SigningSession, CryptoError>;

pub struct SignCmsRequest<'a> {
    pub session: &'a SigningSession,
    pub canonical_xml: &'a [u8],
    pub include_tsp: bool,
}

pub struct SignedCmsBytes(pub Vec<u8>);   // public; safe to derive Debug

pub struct CertDer(pub Vec<u8>);          // public; safe to derive Debug

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("JKS unseal failed for operator {operator_id}: {reason:?}")]
    JksUnseal {
        operator_id: String,
        reason: SealKind,        // enum, not String — no secret echo
    },
    #[error("CMS sign failed: {reason:?}")]
    CmsSign { reason: SignKind },
    #[error("envelope decrypt failed: {reason:?}")]
    EnvelopeDecrypt { reason: DecryptKind },
    #[error("cert fetch failed: {reason:?}")]
    CertFetch { reason: FetchKind },
    #[error("verify failed")]
    Verify,
}

#[derive(Debug)]
pub enum SealKind { BadPassword, BadSalt, MalformedJks, KeyExtractionFailed }
#[derive(Debug)]
pub enum SignKind { CurveMismatch, InvalidDigest, BackendError }
#[derive(Debug)]
pub enum DecryptKind { ParseFailed, KekDeriveFailed, MacFailed }
#[derive(Debug)]
pub enum FetchKind { TransportError, ParseFailed, SkiMismatch, AllUrlsFailed }
```

**Backing functions per trait method:**

| Trait method | Backed by |
|---|---|
| `sign_cms_detached` | `cms::builder::sign_detached_with_content_digest` + `cms::signer::DstuInProcessSigner` (RawSigner-backed) |
| `verify_dstu` | `core::sign::verify` |
| `unwrap_envelope` | `cms::envelope::{parse_envelope_params, decrypt_with_params, unwrap_envelope}` |
| `fetch_cert_by_ski` | `cms::cmp::fetch_cert_by_ski` |
| `unseal_jks` (helper) | `interop::prro::containers::extract_private_key` (auto-detect) → `interop::prro::jks::read_jks` (specific) |

**Compliance checks:**
- ADR-M2-6: NO `SqlitePool`, `SqliteConnection`, `Transaction`, or `Pool<...>` appears in any signature above.  Trait is `Send + Sync` for `&dyn CryptoProvider` use across tasks.
- ADR-M2-5: `SigningSession` holds `Zeroizing<Vec<u8>>`, never logged.  Errors carry `enum reason`, not `String` — no path to `format!` a password into a log message.
- ADR-M2-1: in-process wrapper, no sidecar HTTP code, no fallback provider.

---

## 6. Redacted-`Debug` discipline for secret-bearing types

| Type | Lives in | Discipline | Rationale |
|---|---|---|---|
| `SigningSession` | `prro::crypto` | manual `impl Debug { write_str("SigningSession(<redacted>)") }` | Holds unsealed private key |
| `SealedMaterial<'a>` | `prro::crypto` | manual `impl Debug` printing field names but `<redacted>` for `jks_bytes`, `jks_password_hex`, `cred_salt` | All fields are sealed-secret material |
| `JksEntry` | `prro_crypto::interop::prro::jks` | **VERIFY**: must use manual redacted `Debug`.  If currently `#[derive(Debug)]`, this is the migration in §4. | Holds private key bytes |
| `ExtractedKey` | `prro_crypto::interop::prro::containers` | **VERIFY** as above | Universal extracted-key wrapper |
| `Key6Parsed` | `prro_crypto::interop::prro::key6` | **VERIFY** as above | Container parse output, holds key |
| `PfxParsed` | `prro_crypto::interop::prro::pfx` | **VERIFY** as above | PFX parse output, holds key |
| `CryptoError` and its `*Kind` enums | `prro::crypto` | safe to `#[derive(Debug)]` — enums carry no bytes | Reasons are enum variants only |
| `SignedCmsBytes` / `CertDer` | `prro::crypto` | safe to `#[derive(Debug)]` (might choose hex-prefix for readability) | Public output bytes |

**Reference `impl Debug` snippet** (any secret-bearing type):

```rust
impl std::fmt::Debug for SigningSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SigningSession")
            .field("key", &"<redacted>")
            .field("cert_der_len", &self.inner.cert_der.len())
            .field("curve", &self.inner.curve)   // public
            .finish()
    }
}
```

The W1+ implementation MUST run a `tracing` subscriber test (per
ADR-M2-5 §4d) that exercises every error path and asserts the captured
log lines never contain the seeded password / cred_salt / private-key
bytes.

---

## 7. Deferred questions and the assumption that closes them

1. **`python.rs` PyO3 binding compatibility.**  The 37 KB `python.rs`
   in `rust/prro_crypto/src/python.rs` exposes a Python-facing API
   the Python sidecar consumes.  This audit did not enumerate every
   `#[pyfunction]`.  If a §3 additive extension touches a symbol that
   `python.rs` re-exports, the Python sidecar's pyo3 build may need a
   re-generation step.
   **Closing assumption:** the additive extensions in §3 are limited to
   new helper functions / new manual `Debug` impls, neither of which
   alters the existing `python.rs` surface.  W1+ implementer verifies
   on first additive PR to `prro_crypto` (probably none needed).

2. **`prro_crypto_v2` parallel.**  `rust/prro_crypto_v2/` is a separate
   workspace member with its own benches; it has not been audited for
   compatibility constraints because M2 wraps `prro_crypto`, not v2.
   **Closing assumption:** v2 is an experimental fork the team can
   ignore for M2 wrapper purposes.  If v2 ever supersedes v1, M2's
   trait is small enough to re-back without a wrapper rewrite.

3. **Async signing performance.**  Whether `tokio::task::spawn_blocking`
   adds enough overhead to matter for high-throughput signing under the
   write_path is an M3 / perf concern, not an M2 audit blocker.
   **Closing assumption:** wrapper boundary is `spawn_blocking` for
   M2; perf tuning (sign pool, batched signer) is an M3+ optimisation
   if measurements demand it.

---

## 8. ADR revision (none proposed)

ADR-M2-1, ADR-M2-5, ADR-M2-6 are **confirmed unchanged** by this audit.

ADR-M2-1 "Open risk" wording about "extensions … with a hard 'no breaking
change' rule" survives intact; the audit just resolves the magnitude as
trivial.  No proposed diff.  ADR review status remains `approved`.

---

## 9. Acceptance-criterion self-check

| Criterion | Met by |
|---|---|
| Public functions `prro::crypto` needs from `prro_crypto`, with file:line + use-case | §1 (16-row table) |
| Extensions required, classified additive / signature-shaping / breaking | §3 (5-row table; all 0 non-additive) |
| Concrete migration plan per non-additive extension | §4 (no non-additive entries; standing template documented) |
| `CryptoProvider` trait shape proposed in Rust (no edits under `rust/prro/src/**`) | §5 (full Rust block, lives only in this doc) |
| Trait complies with ADR-M2-6 (no DB handle in any signature) | §5 "Compliance checks" + grep'able: signatures contain no `SqlitePool` / `SqliteConnection` / `Transaction` / `Pool` |
| Redacted-`Debug` discipline for secret-bearing types per ADR-M2-5 | §6 (per-type table + reference snippet) |
| ADR-M2-1 revision-or-confirm explicit | §8 (confirms unchanged) |
| Top-level `Status:` line `final` or `deferred` | header: `Status: final` |
| No edits under `rust/prro/src/**` or `rust/prro_crypto/src/**` | this commit touches only the new findings doc |

---

## 10. Verify command

```bash
test -f docs/superpowers/specs/2026-05-04-m2-w0-3-prro-crypto-audit.md && \
  grep -E '^Status: (final|deferred)$' docs/superpowers/specs/2026-05-04-m2-w0-3-prro-crypto-audit.md
```

Expected output: `Status: final`.
