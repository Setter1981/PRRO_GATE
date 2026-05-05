# M2 W1+ Implementation Plan — Crypto Wrapper, Cert Refresher, DPS Channel, Goldens, Architectural Gates

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers-extended-cc:subagent-driven-development (recommended) or superpowers-extended-cc:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the M2 implementation layer that the M1 Foundation crate has been stubbed for: an in-process `prro::crypto` wrapper over `prro_crypto`, an async `services::cert_refresher` that talks to the IIT-proprietary cert-lookup channel, a tonic-built `prro::transports::dps` gRPC client + native Rust mock, a byte-equivalence goldens harness with frozen fixtures, and two architectural gates (no DB handle in provider/channel APIs; no secret material in tracing).

**Architecture:** Six tasks (W1-W6), three implementation modules + two gates + one goldens harness, all per the M2 pre-plan ADR (`docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md`, approved 2026-05-04) and the three W0 findings docs (W0-1 DPS wire = gRPC; W0-2 CMP = IIT lookup-by-SKI, `prro_crypto::cms::cmp::fetch_cert_by_ski` ready; W0-3 prro_crypto API audit + `CryptoProvider` trait shape).  No M3 write-path work; M2 builds the substrate that M3 stages compose over.

**Tech Stack:** Rust 1.83+, sqlx 0.8 SQLite (M1), `prro_crypto` (workspace), new deps: `async-trait` 0.1, `zeroize` 1.7+, `tonic` 0.12, `tonic-build` 0.12, `prost` 0.13, `wiremock` 0.6 (HTTP byte-replay for the test CA), `httpmock` or hand-rolled axum test server alternative. SQLX_OFFLINE workflow inherited from M1; `cargo sqlx prepare` from `rust/prro/` with absolute `DATABASE_URL` whenever new `sqlx::query!` macros land.

> **Plan revision 2026-05-05 (READ FIRST).**  Pre-implementation review found
> the original snippets diverged from the actual `prro_crypto` API and the
> real `fiscal_server.proto` contents; ten corrections were applied in
> place.  Quick reference for what changed:
>
> - **W1**: `SigningSession` now holds `Zeroizing<[u8; 32]>` matching
>   `prro_crypto::interop::prro::containers::ExtractedKey.param_d`; the
>   blocking closure receives `Arc<SigningSession>` (no plaintext copy).
>   `sign_cms_detached` calls the real
>   `sign_detached_with_content_digest(profile, cert_der, content_digest, &dyn RawSigner)`
>   shape via a `DstuInProcessSigner` built from the session's `param_d`.
> - **W2**: same-SKI refresh now does in-place UPDATE (no PK conflict
>   with `operator_certs.ski_hex`); only key-roll uses the staged-row
>   pattern.  Multi-URL routing reads from the new `006_ca_endpoints.sql`
>   migration (vendoring legacy `sql/016_ca_endpoints.sql`); the `cmp_url`
>   value already includes `/services/cmp/`.  `parse_iso8601` and SKI
>   computation propagate typed errors instead of fail-open.
> - **W3**: real proto package is `com.programika.rro.ws.chk`; service
>   `ChkIncomeService` with five RPCs (`sendChkV2`, `lastChk`, `ping`,
>   `statusRro`, `infoRro`).  `CheckResponse` fields are `id`, `status`,
>   `id_sign`, `data_sign`, `error_message` (no `fns_data`).
> - **W5**: scanner walks `ItemTrait` / `TraitItemFn` in addition to
>   `pub fn` — the trait surface is the main API and would otherwise be
>   missed.

**Out of scope (deferred to M3):**
- Write-path stages (`PREPARED → SIGNED → ENCRYPTED → SENT → KVT1 → KVT2 → ACK`); `WriteWorker`; staged pipeline orchestration.
- Ingress shells (REST / XML-RPC / Maria / Maria304 / Checkbox-compat).
- Full canonical-XML builder for arbitrary document shapes — M2 ships only the minimal XML helpers needed to produce the W4 golden inputs for the five document types listed in ADR-M2-3.
- Admin UI, recovery loop, reconciliation services.
- `node_state` bootstrapping logic (still gated by `PRRO_GATE-ah8`).

---

## File structure (M2 only)

```
rust/prro/
  Cargo.toml                                   # MODIFY — add async-trait, zeroize, tonic, tonic-build, prost, wiremock
  build.rs                                     # MODIFY — add tonic_build::compile_protos for fiscal_server.proto
  proto/
    fiscal_server.proto                        # NEW — vendored from src/prro_gateway/transports/proto/fiscal_server.proto
  src/
    lib.rs                                     # MODIFY — `pub mod crypto;`, `pub mod transports;`, `pub mod services;`
    crypto/
      mod.rs                                   # NEW — re-exports the public surface
      provider.rs                              # NEW — `CryptoProvider` trait, request/response types
      session.rs                               # NEW — `SigningSession`, `SealedMaterial`, `unseal_jks`
      errors.rs                                # NEW — `CryptoError` + `*Kind` enum reasons
      in_process.rs                            # NEW — `InProcessProvider` impl backed by `prro_crypto`
    transports/
      mod.rs                                   # NEW — `pub mod dps;`
      dps/
        mod.rs                                 # NEW — re-exports DpsChannel + types
        channel.rs                             # NEW — `DpsChannel` trait + `GrpcDpsChannel` impl
        gen.rs                                 # NEW — wraps tonic-generated module so callers don't import generated paths directly
        types.rs                               # NEW — typed request/response structs the trait emits/consumes
        errors.rs                              # NEW — `DpsError` + `*Kind` enum reasons
    services/
      mod.rs                                   # NEW — `pub mod cert_refresher;`
      cert_refresher.rs                        # NEW — async multi-URL CMP fetch + atomic active-flip
  tests/
    crypto_provider_smoke.rs                   # NEW — W1 integration tests
    cert_refresher_smoke.rs                    # NEW — W2 integration tests + byte-replay HTTP server
    dps_channel_smoke.rs                       # NEW — W3 integration tests + embedded tonic mock
    goldens_byte_equiv.rs                      # NEW — W4 byte-equivalence harness
    goldens/                                   # NEW — frozen test vectors (committed)
      # kvt1/, kvt2/ DEFERRED from W4 first round (see W4 scope note)
      cms/                                     # NEW — deterministic-prefix CMS-signed XML
      xml/                                     # NEW — canonical unsigned XML for 5 doc types
        shift_open.bin
        shift_close.bin
        sell.bin
        return.bin
        z_report.bin
      regenerate.py                            # NEW — manual capture script (NOT CI-triggered)
      README.md                                # NEW — operator procedure for re-capture
    api_surface_no_db_handle.rs                # NEW — W5 ADR-M2-6 static check via `syn`
    secret_flow_tracing.rs                     # NEW — W6 tracing-subscriber substring check
docs/
  M2-goldens-capture.md                        # NEW — operator-facing procedure for W4 re-capture
```

Total new top-level files: ~25.  Diff impact concentrated in `rust/prro/` so the existing CI matrix path filter (`rust/prro/**`) covers it.

---

## Task 0 — bd epic + W0 docs cross-link (administrative, ~30 min)

**Goal:** Create the M2 bd epic and link the W0 follow-ups under it so M2 progress is visible at one ID.

**Day budget:** ~30 minutes.  Pure administrative; not a coding task.

**Files:** none (bd-only).

**Acceptance Criteria:**

- [ ] An `epic`-typed bd issue is created with subject "M2 — crypto wrapper, cert refresher, DPS channel, goldens" and a description that links the ADR + the three W0 findings docs.
- [ ] Existing M2 follow-ups (`PRRO_GATE-k99`, `PRRO_GATE-ddn`, `PRRO_GATE-1n9`, `PRRO_GATE-6r7`, `PRRO_GATE-ah8`, `PRRO_GATE-5js`) are re-tagged as `child-of` the new epic via `bd dep add` or `bd update --add-dep`.
- [ ] `bd ready` shows the new epic and the M2 implementation tasks created in `.tasks.json`; M3 follow-ups remain blocked.

**Verify:**

```bash
bd show <new-epic-id> --json | jq -r '.[0].title' | grep -q "M2"
```

**Steps:**

- [ ] **Step 1: Create the M2 epic.**

```bash
bd create "M2 — crypto wrapper, cert refresher, DPS channel, goldens" \
  --description "M2 Foundation+1: in-process prro::crypto, services::cert_refresher, prro::transports::dps gRPC client + tonic mock, byte-equivalence goldens harness, ADR-M2-6 + ADR-M2-5 architectural gates.  Inputs: docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md (ADR), docs/superpowers/specs/2026-05-04-m2-w0-1-dps-wire.md, docs/superpowers/specs/2026-05-04-m2-w0-2-cmp-probe.md, docs/superpowers/specs/2026-05-04-m2-w0-3-prro-crypto-audit.md.  Out of scope: M3 write-path internals." \
  -t epic -p 1 --json
```

Note the issue id (e.g. `PRRO_GATE-xyz`).

- [ ] **Step 2: Link existing M2/M3 follow-ups under the epic.**

For each of the six existing follow-ups, either edit them to add a `child-of:<epic-id>` dep, or just leave them — the relationship is documented in this plan and the ADR; bd-side linkage is bonus visibility, not load-bearing.

```bash
EPIC=PRRO_GATE-xyz
for issue in PRRO_GATE-k99 PRRO_GATE-ddn PRRO_GATE-1n9 PRRO_GATE-6r7 PRRO_GATE-ah8 PRRO_GATE-5js; do
  bd dep add "$issue" --depends-on "$EPIC" --json | tail -2
done
```

- [ ] **Step 3: Commit nothing.**

This task touches no files in the repo.  Skip git work.

---

## Task 1 (W1) — `prro::crypto` in-process wrapper

**Goal:** Land the `CryptoProvider` trait, `SigningSession` opaque key handle, `unseal_jks` boundary helper, typed `CryptoError` with enum reasons, and a single `InProcessProvider` implementation backed by `prro_crypto`.  No DB handle in any public signature.  Redacted `Debug` for every secret-bearing type.

**Day budget:** 3-5 days.

**Implements:** ADR-M2-1, ADR-M2-5, ADR-M2-6.  Uses the trait shape proposed in `docs/superpowers/specs/2026-05-04-m2-w0-3-prro-crypto-audit.md` §5.

**Files:**

- Modify: `rust/prro/Cargo.toml` (add deps)
- Modify: `rust/prro/src/lib.rs` (`pub mod crypto;`)
- Create: `rust/prro/src/crypto/mod.rs`
- Create: `rust/prro/src/crypto/provider.rs`
- Create: `rust/prro/src/crypto/session.rs`
- Create: `rust/prro/src/crypto/errors.rs`
- Create: `rust/prro/src/crypto/in_process.rs`
- Create: `rust/prro/tests/crypto_provider_smoke.rs`

**Acceptance Criteria:**

- [ ] `cargo build -p prro` clean on all four CI matrix targets (musl / gnu / msvc / aarch64).
- [ ] `CryptoProvider` trait is `Send + Sync`, takes `&dyn CryptoProvider` callers, and contains exactly four methods: `sign_cms_detached`, `verify_dstu`, `unwrap_envelope`, `fetch_cert_by_ski`.
- [ ] No public function in `crypto::*` accepts `SqlitePool`, `SqliteConnection`, `Transaction`, `Pool<...>`, or any sqlx handle (W5 enforces this; W1 must not introduce a violation).
- [ ] `SigningSession` and `SealedMaterial` implement `Debug` manually printing `<redacted>` for any field that holds key/password material.  `#[derive(Debug)]` is FORBIDDEN on these types.
- [ ] `CryptoError` carries enum reasons (`SealKind`, `SignKind`, `DecryptKind`, `FetchKind`), never free-form `String` containing secret material.
- [ ] `InProcessProvider::new` constructs without I/O; signing happens via `tokio::task::spawn_blocking` boundary inside the provider methods.
- [ ] `tests/crypto_provider_smoke.rs` exercises each trait method via the in-process provider with deterministic test material, plus a redacted-`Debug` assertion that `format!("{:?}", session)` does NOT contain the seeded password / key bytes.

**Verify:**

```bash
cargo test -p prro --test crypto_provider_smoke
```

Per-target verify (CI):

```bash
SQLX_OFFLINE=true cargo test -p prro --target <T> --test crypto_provider_smoke --locked
```

for `T` in `{x86_64-unknown-linux-gnu, x86_64-unknown-linux-musl, x86_64-pc-windows-msvc, aarch64-unknown-linux-gnu}`.

**Steps:**

- [ ] **Step 1: Add deps to `rust/prro/Cargo.toml`.**

In `[dependencies]` after the existing `futures` line, append:

```toml
async-trait = "0.1"
zeroize = { version = "1", features = ["zeroize_derive"] }
```

Run:

```bash
cargo build -p prro
```

Expected: builds clean (the new deps are transitive-compatible with M1's dep tree).

- [ ] **Step 2: Add `pub mod crypto;` to `rust/prro/src/lib.rs`.**

Edit `rust/prro/src/lib.rs` and insert `pub mod crypto;` adjacent to the other `pub mod` lines:

```rust
pub mod app;
pub mod config;
pub mod crypto;        // ← new
pub mod db;
pub mod doctor;
pub mod runtime;
```

- [ ] **Step 3: Write `rust/prro/src/crypto/errors.rs` (typed errors with enum reasons).**

```rust
//! Typed errors for `prro::crypto`.  Per ADR-M2-5 §4c: reasons are enums,
//! NEVER free-form `String`.  No path through this module should ever
//! `format!()` a secret into an error.

use std::fmt;

/// Top-level error returned by every `CryptoProvider` method.
#[derive(thiserror::Error)]
pub enum CryptoError {
    #[error("JKS unseal failed for operator {operator_id}: {reason:?}")]
    JksUnseal {
        operator_id: String,
        reason: SealKind,
    },
    #[error("CMS sign failed: {reason:?}")]
    CmsSign { reason: SignKind },
    #[error("envelope decrypt failed: {reason:?}")]
    EnvelopeDecrypt { reason: DecryptKind },
    #[error("cert fetch failed: {reason:?}")]
    CertFetch { reason: FetchKind },
    #[error("signature verification failed: {reason:?}")]
    VerifyFailed { reason: VerifyKind },
}

// Manual Debug — make sure no future field accidentally `format!`s a secret.
impl fmt::Debug for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::JksUnseal { operator_id, reason } => f.debug_struct("JksUnseal")
                .field("operator_id", operator_id)
                .field("reason", reason)
                .finish(),
            Self::CmsSign { reason } => f.debug_struct("CmsSign").field("reason", reason).finish(),
            Self::EnvelopeDecrypt { reason } => f.debug_struct("EnvelopeDecrypt").field("reason", reason).finish(),
            Self::CertFetch { reason } => f.debug_struct("CertFetch").field("reason", reason).finish(),
            Self::VerifyFailed { reason } => f.debug_struct("VerifyFailed").field("reason", reason).finish(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SealKind {
    BadPassword,
    BadSalt,
    MalformedJks,
    KeyExtractionFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignKind {
    CurveMismatch,
    InvalidDigest,
    BackendError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecryptKind {
    ParseFailed,
    KekDeriveFailed,
    MacFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchKind {
    TransportError,
    ParseFailed,
    SkiMismatch,
    AllUrlsFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyKind {
    /// raw `r||s` LE-bytes blob is not exactly 64 bytes for PB-257.
    MalformedSignature,
    /// `expand_compressed_checked` rejected the public key (off-curve,
    /// wrong length, or wrong cofactor).
    MalformedPubkey,
    /// Signature parsed and curve checks passed but `verify` returned
    /// false — used by callers that want to distinguish "wrong sig"
    /// from "malformed inputs".
    SignatureRejected,
}
```

- [ ] **Step 4: Write `rust/prro/src/crypto/session.rs` (sealed material + opaque session + boundary helper).**

```rust
//! Secret-material boundary.  `unseal_jks` is the only function that turns
//! sealed bytes into plaintext; the plaintext lives in `Zeroizing` and is
//! exposed to the rest of the crate only through `SigningSession`.
//!
//! Per ADR-M2-5 §1-§3.

use std::fmt;
use zeroize::Zeroizing;

use crate::crypto::errors::{CryptoError, SealKind};

/// Sealed JKS + password as they live in the DB (`sidecar_operators` row +
/// JKS bytes).  Borrowed; never crosses an `await` boundary in this form.
#[derive(Clone, Copy)]
pub struct SealedMaterial<'a> {
    pub operator_id: &'a str,
    pub jks_bytes: &'a [u8],
    pub jks_password_hex: &'a str,
    pub cred_salt: &'a [u8; 16],
}

// Manual redacted Debug — never expose sealed bytes or password.
impl<'a> fmt::Debug for SealedMaterial<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SealedMaterial")
            .field("operator_id", &self.operator_id)
            .field("jks_bytes", &"<redacted>")
            .field("jks_password_hex", &"<redacted>")
            .field("cred_salt", &"<redacted>")
            .finish()
    }
}

/// Opaque handle returned by `unseal_jks`.  Holds the DSTU 4145 private
/// scalar in `Zeroizing<[u8; 32]>` (matching `ExtractedKey.param_d` shape);
/// the inner state is `Arc`-shared so the blocking closure can move a clone
/// without copying the plaintext bytes.  Manual `Debug` prints `<redacted>`.
#[derive(Clone)]
pub struct SigningSession {
    inner: std::sync::Arc<SigningSessionInner>,
}

struct SigningSessionInner {
    operator_id: String,
    /// 32 little-endian bytes of the DSTU 4145 private scalar.
    /// Wrapped in `Zeroizing` so dropping the last `Arc` zeros the array.
    /// Never logged; never `Debug`-printed.
    param_d: Zeroizing<[u8; 32]>,
    /// Leaf cert DER (non-secret).
    cert_der: Vec<u8>,
}

impl fmt::Debug for SigningSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SigningSession")
            .field("operator_id", &self.inner.operator_id)
            .field("param_d", &"<redacted>")
            .field("cert_der_len", &self.inner.cert_der.len())
            .finish()
    }
}

impl SigningSession {
    pub fn operator_id(&self) -> &str {
        &self.inner.operator_id
    }

    pub fn cert_der(&self) -> &[u8] {
        &self.inner.cert_der
    }

    /// Crate-internal accessor; the in-process provider reads this when
    /// constructing a `DstuInProcessSigner`.  External callers MUST NOT
    /// see plaintext key bytes.
    pub(crate) fn param_d(&self) -> &Zeroizing<[u8; 32]> {
        &self.inner.param_d
    }

    /// Test-only constructor.
    #[cfg(any(test, feature = "test_helpers"))]
    pub fn new_for_test(operator_id: String, param_d: [u8; 32], cert_der: Vec<u8>) -> Self {
        Self {
            inner: std::sync::Arc::new(SigningSessionInner {
                operator_id,
                param_d: Zeroizing::new(param_d),
                cert_der,
            }),
        }
    }
}

/// Unseal a JKS via `prro_crypto::interop::prro::containers::extract_private_key`.
/// XOR-soft seal of the password is undone via the `cred_salt` per spec
/// decision #16; the resulting plaintext password is itself wrapped in
/// `Zeroizing` for the duration of the call.
pub fn unseal_jks(sealed: SealedMaterial<'_>) -> Result<SigningSession, CryptoError> {
    use prro_crypto::interop::prro::containers::extract_private_key;

    let password = unxor_password(sealed.jks_password_hex, sealed.cred_salt)
        .map_err(|_| CryptoError::JksUnseal {
            operator_id: sealed.operator_id.to_string(),
            reason: SealKind::BadPassword,
        })?;

    let extracted = extract_private_key(sealed.jks_bytes, &password).map_err(|e| {
        let reason = classify_extract_error(&e);
        CryptoError::JksUnseal {
            operator_id: sealed.operator_id.to_string(),
            reason,
        }
    })?;

    // `ExtractedKey { format, param_d: Zeroizing<[u8;32]>, certs: Vec<Vec<u8>> }`
    // — see `rust/prro_crypto/src/interop/prro/containers.rs:73-`.
    let leaf_cert = extracted
        .certs
        .into_iter()
        .next()
        .ok_or_else(|| CryptoError::JksUnseal {
            operator_id: sealed.operator_id.to_string(),
            reason: SealKind::KeyExtractionFailed,
        })?;

    Ok(SigningSession {
        inner: std::sync::Arc::new(SigningSessionInner {
            operator_id: sealed.operator_id.to_string(),
            // No copy of the plaintext bytes — `param_d` already wraps
            // `[u8; 32]` in `Zeroizing`, we move it into the session.
            param_d: extracted.param_d,
            cert_der: leaf_cert,
        }),
    })
}

fn unxor_password(hex: &str, salt: &[u8; 16]) -> Result<Zeroizing<Vec<u8>>, ()> {
    let mut bytes = Zeroizing::new(hex_decode(hex)?);
    for (i, b) in bytes.iter_mut().enumerate() {
        *b ^= salt[i % salt.len()];
    }
    Ok(bytes)
}

fn hex_decode(s: &str) -> Result<Vec<u8>, ()> {
    if s.len() % 2 != 0 {
        return Err(());
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    for chunk in s.as_bytes().chunks(2) {
        let h = hex_digit(chunk[0])?;
        let l = hex_digit(chunk[1])?;
        out.push((h << 4) | l);
    }
    Ok(out)
}

fn hex_digit(c: u8) -> Result<u8, ()> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(()),
    }
}

/// Map `prro_crypto::interop::prro::containers::ContainerError` (or whatever
/// the actual error type ends up being) to a typed `SealKind` reason.
/// Implementer should `match` on the concrete `prro_crypto` enum here; the
/// stub below is intentionally pessimistic until the implementer wires the
/// actual variants.
fn classify_extract_error<E: std::fmt::Debug>(_err: &E) -> SealKind {
    // Implementer task: replace with a real match arm-by-arm classification
    // once the actual `prro_crypto::interop::prro::containers::ContainerError`
    // variants are wired in.  The Debug-bound here is type-erased on purpose
    // so this compiles before that wiring lands; W1 acceptance test must
    // exercise at least one mapped variant (BadPassword via wrong-password
    // fixture) to prove the classification path runs.
    SealKind::KeyExtractionFailed
}
```

- [ ] **Step 5: Write `rust/prro/src/crypto/provider.rs` (the trait + request/response types).**

```rust
//! `CryptoProvider` trait — every cryptographic operation in M2 goes
//! through here.  Per ADR-M2-1 (in-process), ADR-M2-6 (no DB handle in
//! public API).

use async_trait::async_trait;

use crate::crypto::errors::CryptoError;
use crate::crypto::session::SigningSession;

/// Returned by `sign_cms_detached`.
#[derive(Debug, Clone)]
pub struct SignedCmsBytes(pub Vec<u8>);

/// Returned by `fetch_cert_by_ski`.
#[derive(Debug, Clone)]
pub struct CertDer(pub Vec<u8>);

#[derive(Debug, Clone, Copy)]
pub struct DstuVerifyResult(pub bool);

pub struct SignCmsRequest<'a> {
    pub session: &'a SigningSession,
    pub canonical_xml: &'a [u8],
    pub profile: prro_crypto::cms::profile::CmsProfile,
}

#[async_trait]
pub trait CryptoProvider: Send + Sync {
    /// Build a CMS-detached signed envelope around `request.canonical_xml`.
    /// Sync-on-the-inside, async-on-the-outside via `spawn_blocking`.
    async fn sign_cms_detached(
        &self,
        request: SignCmsRequest<'_>,
    ) -> Result<SignedCmsBytes, CryptoError>;

    /// Verify a DSTU 4145 raw signature.
    ///
    /// `content_digest` is the already-computed message hash bytes
    /// (caller chooses the profile — GOST 34.311-95 or Kupyna-256 — and
    /// hands in the digest, mirroring `sign_cms_detached`'s contract);
    /// `sig_bytes` is the 64-byte raw concatenation of `r || s` LE-packed
    /// per `prro_crypto::python::raw_split_to_rs` (`rust/prro_crypto/
    /// src/python.rs:284`); `pubkey_compressed` is the 33-byte LE
    /// compressed point as returned by
    /// `prro_crypto::cms::envelope::extract_cert_pubkey_bytes` and
    /// validated via `expand_compressed_checked`.
    async fn verify_dstu(
        &self,
        content_digest: &[u8],
        sig_bytes: &[u8],
        pubkey_compressed: &[u8],
    ) -> Result<DstuVerifyResult, CryptoError>;

    /// Decrypt a CMS envelope (KVT2 / DPS-encrypted response).
    ///
    /// Real `prro_crypto::cms::envelope::unwrap_envelope` signature
    /// (`rust/prro_crypto/src/cms/envelope.rs:307`) is
    /// `(envelope_der, &FieldEl d, &Point originator_pub, &Curve)` —
    /// the originator's PUBLIC key is required for the ECDH-derived
    /// CEK and the curve fixes the field width.  We expose it at this
    /// trait by taking the originator certificate DER plus the session
    /// (which already holds the recipient private key); the impl runs
    /// `extract_cert_pubkey_bytes` + `expand_compressed_checked` against
    /// the configured curve to produce the `Point` and `FieldEl`.
    async fn unwrap_envelope(
        &self,
        envelope_der: &[u8],
        originator_cert_der: &[u8],
        session: &SigningSession,
    ) -> Result<Vec<u8>, CryptoError>;

    /// Fetch a cert by SKI from the IIT-proprietary CMP-look-alike channel
    /// (W0-2 finding).  URLs come from the caller (a service-layer module
    /// that loaded them from `cert_provisioning_config` / `ca_endpoints`).
    /// `request_timeout` is forwarded to every per-URL probe; the real
    /// `prro_crypto::cms::cmp::fetch_cert_by_ski` is
    /// `(cmp_url: &str, ski: &[u8], timeout: Duration)`
    /// (`rust/prro_crypto/src/cms/cmp.rs:312`).
    async fn fetch_cert_by_ski(
        &self,
        urls: &[String],
        ski: &[u8; 32],
        request_timeout: std::time::Duration,
    ) -> Result<CertDer, CryptoError>;
}
```

- [ ] **Step 6: Write `rust/prro/src/crypto/in_process.rs` (the trait impl).**

```rust
//! `InProcessProvider` — backs the `CryptoProvider` trait with direct
//! `prro_crypto` calls, wrapping blocking C-FFI in `tokio::task::spawn_blocking`.

use async_trait::async_trait;

use crate::crypto::errors::{CryptoError, DecryptKind, FetchKind, SignKind};
use crate::crypto::provider::{
    CertDer, CryptoProvider, DstuVerifyResult, SignCmsRequest, SignedCmsBytes,
};
use crate::crypto::session::SigningSession;

/// Single source of timeout truth: each `CryptoProvider::fetch_cert_by_ski`
/// call carries its own `request_timeout`.  Caller (`services::cert_refresher`)
/// loads it from `cert_provisioning_config.cmp_request_timeout_secs` and
/// passes it through; the provider itself is stateless w.r.t. timeouts.
/// This avoids drift between a config-stamped value and a per-call value.
#[derive(Debug, Default, Clone, Copy)]
pub struct InProcessProvider;

impl InProcessProvider {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CryptoProvider for InProcessProvider {
    async fn sign_cms_detached(
        &self,
        request: SignCmsRequest<'_>,
    ) -> Result<SignedCmsBytes, CryptoError> {
        // The `SigningSession` is `Clone` over `Arc<Inner>`; we move a clone
        // into the blocking closure.  No copy of the `Zeroizing<[u8; 32]>`
        // private key — only Arc-strong-count bumps.  The Zeroizing wrapper
        // zeroes the array when the last Arc holder drops.
        let session = request.session.clone();
        let canonical = request.canonical_xml.to_vec();
        let profile = request.profile;

        let bytes = tokio::task::spawn_blocking(move || {
            sign_cms_blocking(&canonical, &session, profile)
        })
        .await
        .map_err(|_| CryptoError::CmsSign { reason: SignKind::BackendError })??;

        Ok(SignedCmsBytes(bytes))
    }

    async fn verify_dstu(
        &self,
        content_digest: &[u8],
        sig_bytes: &[u8],
        pubkey_compressed: &[u8],
    ) -> Result<DstuVerifyResult, CryptoError> {
        // Verify is fast enough (~150µs first call, ~10µs after pubkey
        // validation cache hit) that staying on the executor is OK.
        prro_crypto_verify_blocking(content_digest, sig_bytes, pubkey_compressed)
            .map(DstuVerifyResult)
    }

    async fn unwrap_envelope(
        &self,
        envelope_der: &[u8],
        originator_cert_der: &[u8],
        session: &SigningSession,
    ) -> Result<Vec<u8>, CryptoError> {
        let env = envelope_der.to_vec();
        let originator = originator_cert_der.to_vec();
        let session_clone = session.clone();   // Arc bump, no plaintext copy
        let plaintext = tokio::task::spawn_blocking(move || {
            unwrap_envelope_blocking(&env, &originator, &session_clone)
        })
        .await
        .map_err(|_| CryptoError::EnvelopeDecrypt { reason: DecryptKind::ParseFailed })??;
        Ok(plaintext)
    }

    async fn fetch_cert_by_ski(
        &self,
        urls: &[String],
        ski: &[u8; 32],
        request_timeout: std::time::Duration,
    ) -> Result<CertDer, CryptoError> {
        if urls.is_empty() {
            return Err(CryptoError::CertFetch { reason: FetchKind::AllUrlsFailed });
        }
        let urls_owned: Vec<String> = urls.to_vec();
        let ski_owned = *ski;
        let bytes = tokio::task::spawn_blocking(move || {
            fetch_cert_blocking(&urls_owned, &ski_owned, request_timeout)
        })
        .await
        .map_err(|_| CryptoError::CertFetch { reason: FetchKind::TransportError })??;
        Ok(CertDer(bytes))
    }
}

fn sign_cms_blocking(
    canonical_xml: &[u8],
    session: &SigningSession,
    profile: prro_crypto::cms::profile::CmsProfile,
) -> Result<Vec<u8>, CryptoError> {
    use prro_crypto::cms::builder::sign_detached_with_content_digest;
    use prro_crypto::cms::profile::CmsProfile;
    use prro_crypto::cms::signer::DstuInProcessSigner;
    use prro_crypto::core::curve::Curve;
    use prro_crypto::core::field::FieldEl;
    use prro_crypto::core::hash::{gost_34_311_95, kupyna_256};

    // `sign_detached_with_content_digest(profile, cert_der, content_digest, &dyn RawSigner)`
    // — real signature at `rust/prro_crypto/src/cms/builder.rs:303`.
    // The CMS builder owns `signedAttrs` hashing; we supply the content
    // digest of `canonical_xml` per profile.
    let content_digest = match profile {
        CmsProfile::Dstu4145WithGost34311Pb => gost_34_311_95(canonical_xml).to_vec(),
        CmsProfile::Dstu4145WithDstu7564Pb => kupyna_256(canonical_xml).to_vec(),
    };

    // `param_d` is 32 LE bytes; `FieldEl::from_le_bytes(bytes, mod_words)`
    // returns `Self` directly and PANICS on `bytes.len() > mod_words * 4`
    // (real signature at `rust/prro_crypto/src/core/field.rs:76`).  We
    // hold `[u8; 32]` and PB-257 has `mod_words = 9` (36 bytes capacity),
    // so the assertion can never fire — it's a compile-time invariant
    // for this caller.  No `Result`, no `map_err`.
    let curve = Curve::dstu_pb_257();
    let d = FieldEl::from_le_bytes(&session.param_d()[..], curve.mod_words);
    let signer = DstuInProcessSigner::new(d);

    sign_detached_with_content_digest(profile, session.cert_der(), &content_digest, &signer)
        .map_err(|_| CryptoError::CmsSign { reason: SignKind::BackendError })
}

fn prro_crypto_verify_blocking(
    content_digest: &[u8],
    sig_bytes: &[u8],
    pubkey_compressed: &[u8],
) -> Result<bool, CryptoError> {
    use prro_crypto::core::curve::Curve;
    use prro_crypto::core::field::FieldEl;
    use prro_crypto::core::point::expand_compressed_checked;
    use prro_crypto::core::sign::{verify, Signature};

    // Real signature at `rust/prro_crypto/src/core/sign.rs:301`:
    //   `fn verify(curve: &Curve, pub_q: &Point, hash: &FieldEl,
    //              signature: &Signature) -> bool`
    // PB-257 sig is 32+32 raw LE bytes (per `python.rs:284`); split here.
    if sig_bytes.len() != 64 {
        return Err(CryptoError::VerifyFailed { reason: VerifyKind::MalformedSignature });
    }
    let curve = Curve::dstu_pb_257();
    let pub_q = expand_compressed_checked(pubkey_compressed, &curve)
        .map_err(|_| CryptoError::VerifyFailed { reason: VerifyKind::MalformedPubkey })?;
    let hash = FieldEl::from_le_bytes(content_digest, curve.mod_words);
    let r = FieldEl::from_le_bytes(&sig_bytes[..32], curve.mod_words);
    let s = FieldEl::from_le_bytes(&sig_bytes[32..], curve.mod_words);
    let signature = Signature { r, s };
    Ok(verify(&curve, &pub_q, &hash, &signature))
}

fn unwrap_envelope_blocking(
    envelope_der: &[u8],
    originator_cert_der: &[u8],
    session: &SigningSession,
) -> Result<Vec<u8>, CryptoError> {
    use prro_crypto::cms::envelope::{extract_cert_pubkey_bytes, unwrap_envelope};
    use prro_crypto::core::curve::Curve;
    use prro_crypto::core::field::FieldEl;
    use prro_crypto::core::point::expand_compressed_checked;

    // Real `unwrap_envelope` signature
    // (`rust/prro_crypto/src/cms/envelope.rs:307`):
    //   `fn unwrap_envelope(envelope_der, &FieldEl, &Point originator_pub, &Curve)`
    // The originator's PUBLIC key is needed for the ECDH-derived CEK; the
    // recipient private key (`d`) comes from the session.  Curve is locked
    // to PB-257 in M2 (only DPS-deployed Ukrainian CA curve, W0-3 §1).
    let curve = Curve::dstu_pb_257();
    let originator_pub_compressed = extract_cert_pubkey_bytes(originator_cert_der)
        .map_err(|_| CryptoError::EnvelopeDecrypt { reason: DecryptKind::ParseFailed })?;
    let originator_pub = expand_compressed_checked(&originator_pub_compressed, &curve)
        .map_err(|_| CryptoError::EnvelopeDecrypt { reason: DecryptKind::ParseFailed })?;
    let d = FieldEl::from_le_bytes(&session.param_d()[..], curve.mod_words);

    unwrap_envelope(envelope_der, &d, &originator_pub, &curve)
        .map_err(|_| CryptoError::EnvelopeDecrypt { reason: DecryptKind::MacFailed })
}

fn fetch_cert_blocking(
    urls: &[String],
    ski: &[u8; 32],
    request_timeout: std::time::Duration,
) -> Result<Vec<u8>, CryptoError> {
    use prro_crypto::cms::cmp::fetch_cert_by_ski;
    // Real signature at `rust/prro_crypto/src/cms/cmp.rs:312`:
    //   `fn fetch_cert_by_ski(cmp_url: &str, ski: &[u8], timeout: Duration)`
    // Per-URL timeout applied uniformly; caller controls it via provider
    // config.
    for url in urls {
        match fetch_cert_by_ski(url, &ski[..], request_timeout) {
            Ok(cert_der) => return Ok(cert_der),
            Err(_) => continue,
        }
    }
    Err(CryptoError::CertFetch { reason: FetchKind::AllUrlsFailed })
}
```

> Note: `sign_cms_blocking`, `prro_crypto_verify_blocking`, `unwrap_envelope_blocking`, `fetch_cert_blocking` reference `prro_crypto` symbols whose real signatures are quoted inline above (verified against W0-3 §1 + the real source at the cited file:line locations); if any drift is found at implementation time, file an additive PR against `prro_crypto` instead of editing `rust/prro_crypto/src/**` from inside this task.

> `expand_compressed_checked` lives at `rust/prro_crypto/src/core/point.rs:235`; it returns `Result<Point, PointDecodeError>` and validates the point lies on the curve.  Use it (not the unchecked `expand_compressed`) so a malformed originator cert produces a typed `EnvelopeDecrypt::ParseFailed` rather than a downstream MAC failure.

- [ ] **Step 7: Write `rust/prro/src/crypto/mod.rs` (re-exports).**

```rust
//! In-process crypto provider for prro.  Per ADR-M2-1, ADR-M2-5, ADR-M2-6.
//! Boundaries:
//! - `provider` carries the `CryptoProvider` trait + request/response types.
//! - `session` carries `SealedMaterial`, `SigningSession`, and the
//!   `unseal_jks` boundary helper.
//! - `errors` carries `CryptoError` + enum reasons.
//! - `in_process` is the default implementation backed by `prro_crypto`.

pub mod errors;
pub mod in_process;
pub mod provider;
pub mod session;

pub use errors::{CryptoError, DecryptKind, FetchKind, SealKind, SignKind, VerifyKind};
pub use in_process::InProcessProvider;
pub use provider::{
    CertDer, CryptoProvider, DstuVerifyResult, SignCmsRequest, SignedCmsBytes,
};
pub use session::{unseal_jks, SealedMaterial, SigningSession};
```

- [ ] **Step 8: Write `rust/prro/tests/crypto_provider_smoke.rs` (W1 integration tests).**

```rust
//! W1 smoke: trait wiring + redacted Debug + zero secret-substring leak.

use prro::crypto::{
    InProcessProvider, SealedMaterial, SigningSession,
    CryptoProvider, CryptoError, SealKind,
};

#[test]
fn signing_session_debug_is_redacted() {
    // `SigningSession::new_for_test` takes `[u8; 32]` (the param-d width
    // for DSTU PB-257), not a `Vec<u8>` — see plan §"SigningSession".
    // The 32-byte canary below is ASCII-only so a successful redaction
    // assertion below is unambiguous (no UTF-8 lossy mapping).
    let secret: [u8; 32] = *b"super-secret-canary-32bytes-aaaa";
    let session = SigningSession::new_for_test(
        "operator-1".into(),
        secret,
        b"<cert-der>".to_vec(),
    );
    let s = format!("{:?}", session);
    assert!(s.contains("operator-1"));
    assert!(s.contains("<redacted>"));
    // Substrings of the secret must NOT leak through Debug.
    assert!(!s.contains("super-secret-canary"));
}

#[test]
fn sealed_material_debug_is_redacted() {
    let salt = [0x42u8; 16];
    let mat = SealedMaterial {
        operator_id: "op-2",
        jks_bytes: b"PK\0\0...keystore-bytes...",
        jks_password_hex: "deadbeef",
        cred_salt: &salt,
    };
    let s = format!("{:?}", mat);
    assert!(s.contains("op-2"));
    assert!(s.contains("<redacted>"));
    assert!(!s.contains("deadbeef"));
    assert!(!s.contains("keystore-bytes"));
}

#[tokio::test]
async fn unseal_with_wrong_password_returns_typed_seal_error() {
    // Vendored test JKS; password is "correct"; we pass "wrong" so the
    // password-XOR-salt check fails.  Implementer fills in the test fixture.
    let salt = [0x11u8; 16];
    let sealed = SealedMaterial {
        operator_id: "op-3",
        jks_bytes: b"\xfe\xed\xfe\xed\x00\x00\x00\x02",      // JKS magic
        jks_password_hex: "00",                              // intentionally wrong
        cred_salt: &salt,
    };
    let err = prro::crypto::unseal_jks(sealed).expect_err("wrong password must fail");
    let dbg = format!("{:?}", err);
    assert!(matches!(err, CryptoError::JksUnseal { .. }));
    assert!(!dbg.contains("00"));   // the wrong password substring leaked check
}

#[tokio::test]
async fn fetch_cert_with_no_urls_returns_typed_all_urls_failed() {
    let provider = InProcessProvider::new();
    let ski = [0u8; 32];
    let err = provider
        .fetch_cert_by_ski(&[], &ski, std::time::Duration::from_secs(5))
        .await
        .expect_err("empty urls");
    assert!(matches!(err, CryptoError::CertFetch { .. }));
}

/// Positive control for `verify_dstu`: prove the wrapper actually calls
/// the real `prro_crypto::core::sign::verify` and not a stub.  Uses a
/// known-good fixture vector captured deterministically from
/// `prro_crypto::core::sign::sign(curve, d, hash, rand_e)` (vendored
/// under `tests/fixtures/dstu_pb257_sig_ok.json` as `{d_hex, hash_hex,
/// rand_e_hex, content_digest_hex, sig_64_hex, pubkey_compressed_hex}`).
/// A passing test here is what guarantees that a future stubbing
/// regression in `verify_dstu` lights up immediately.
#[tokio::test]
async fn verify_dstu_known_good_sig_returns_true() {
    let provider = InProcessProvider::new();
    let fixture = test_fixtures::dstu_pb257_sig_ok();
    let result = provider
        .verify_dstu(
            &fixture.content_digest,
            &fixture.sig_64,
            &fixture.pubkey_compressed,
        )
        .await
        .expect("verify_dstu must not error on known-good inputs");
    assert!(result.0, "real verify must accept a known-good sig");

    // Negative complement: flip one bit of the signature; verify must
    // either return DstuVerifyResult(false) OR a typed VerifyFailed
    // error (depending on whether the flipped sig is structurally
    // malformed or just wrong).  Either is acceptable; silently
    // returning true is NOT.
    let mut bad = fixture.sig_64.clone();
    bad[0] ^= 0x01;
    match provider
        .verify_dstu(&fixture.content_digest, &bad, &fixture.pubkey_compressed)
        .await
    {
        Ok(DstuVerifyResult(false)) => {} // ok
        Err(CryptoError::VerifyFailed { .. }) => {} // ok
        Ok(DstuVerifyResult(true)) => panic!("flipped sig must NOT verify true"),
        Err(other) => panic!("unexpected error: {other:?}"),
    }
}

mod test_fixtures {
    /// Deterministic fixture; implementer captures from a one-shot
    /// `prro_crypto::core::sign::sign` invocation with hard-coded
    /// `(d_hex, hash_hex, rand_e_hex)` and freezes the resulting
    /// `(content_digest, sig_64, pubkey_compressed)` triple inline so
    /// the smoke test is self-contained (no JSON IO at runtime).
    pub struct DstuFixture {
        pub content_digest: Vec<u8>,
        pub sig_64: Vec<u8>,
        pub pubkey_compressed: Vec<u8>,
    }
    pub fn dstu_pb257_sig_ok() -> DstuFixture {
        // Implementer fills these from a captured `sign` run.  Hex →
        // bytes via a small inline `hex_decode` helper or `hex::decode`.
        DstuFixture {
            content_digest: hex_decode("…32-byte-hash-hex…"),
            sig_64: hex_decode("…64-byte-sig-hex…"),
            pubkey_compressed: hex_decode("…33-byte-compressed-pubkey-hex…"),
        }
    }
    fn hex_decode(_s: &str) -> Vec<u8> { unimplemented!("inline hex decode") }
}
```

- [ ] **Step 9: Build + test.**

```bash
cargo build -p prro
cargo test -p prro --test crypto_provider_smoke
```

Expected: build clean, 5 tests pass (3 redaction tests + `fetch_cert_with_no_urls_returns_typed_all_urls_failed` + `verify_dstu_known_good_sig_returns_true`).

- [ ] **Step 10: Commit.**

```bash
cd /mnt/d/PRRO_GATE
git add rust/prro/Cargo.toml rust/prro/Cargo.lock rust/prro/src/lib.rs \
        rust/prro/src/crypto/ rust/prro/tests/crypto_provider_smoke.rs
git commit -F /tmp/m2_w1_msg.txt   # or -m if heredoc is allowed
git push origin rust-gateway
```

Commit message:

```
feat(rust/crypto): in-process CryptoProvider wrapper over prro_crypto (M2/W1)

Lands the trait + redacted secret-material discipline per ADR-M2-1,
ADR-M2-5, ADR-M2-6.  Trait shape mirrors W0-3 §5; backing functions
wire to prro_crypto::cms::{builder, envelope, cmp} + interop::prro
without modifying prro_crypto itself.

bd: <M2 epic id>.

Verified:
  cargo build -p prro                          → clean
  cargo test -p prro --test crypto_provider_smoke → 5 passed
```

---

## Task 2 (W2) — `services::cert_refresher`

**Goal:** Land the async cert refresh service.  Reads multiple URLs from `cert_provisioning_config` / `ca_endpoints`; calls `prro::crypto::CryptoProvider::fetch_cert_by_ski`; on success, stages the cert at `active=0` and atomically flips `active=1` inside a `with_immediate` tx.  Honours `refresh_within_days`.

**Day budget:** 4-6 days.  Test-CA fixture work dominates.

**Implements:** ADR-M2-4, ADR-M2-6 (services-layer carve-out).  Uses W0-2 test-CA strategy.  blockedBy: W1.

**Files:**

- Modify: `rust/prro/Cargo.toml` (add `wiremock` to `[dev-dependencies]`, `sha2` and `chrono` to `[dependencies]`, enable `prro_crypto`'s `tsp_http` feature so `fetch_cert_by_ski` is compiled)
- Create: `rust/prro/migrations/006_ca_endpoints.sql` (NEW — vendors the legacy `sql/016_ca_endpoints.sql` schema into the M1 Rust migration tree; populates the table with the production CMP URLs INCLUDING the `/services/cmp/` path)
- Modify: `rust/prro/src/lib.rs` (`pub mod services;`)
- Create: `rust/prro/src/services/mod.rs`
- Create: `rust/prro/src/services/cert_refresher.rs`
- Create: `rust/prro/tests/cert_refresher_smoke.rs`
- Create: `rust/prro/tests/fixtures/test_ca/` (vendored byte-replay corpus)

> **W2 schema note (revision 2026-05-05).**  M1 `cert_provisioning_config`
> carries `primary_cmp_url` and `fallback_cmp_url` columns whose default
> values lack the `/services/cmp/` path component the IIT CMP wire client
> expects (per `rust/prro_crypto/src/cms/cmp.rs:300-`).  Per W0-2 the
> authoritative multi-URL routing source is the `ca_endpoints` table
> (legacy `sql/016_ca_endpoints.sql`).  W2 step 1 ports that schema into
> Rust migrations and W2 reads from the new table; the old
> `cert_provisioning_config.{primary,fallback}_cmp_url` columns become
> deprecated/unused (W2 does NOT remove them — that's an M3+ schema
> hygiene follow-up to file when it lands).

**Acceptance Criteria:**

- [ ] `cert_refresher::refresh_for_fn(pool, fn_id, provider) -> Result<RefreshOutcome, RefreshError>`.  Success cases: `Ok(NoChange)`, `Ok(RefreshedInPlace { ski })`, `Ok(RefreshedKeyRoll { ski_old, ski_new })`.  Every failure mode (no active cert, no enabled endpoints, all CMP URLs failed, malformed metadata, DB error, propagated `CryptoError`) is `Err(RefreshError::*)` — the enum has NO `Failed(reason)` success variant; the `?`-skeleton in step 4 is the canonical contract.
- [ ] **Same-SKI refresh (cert renewed, key kept)** does NOT stage a new row (would PK-conflict with `operator_certs.ski_hex`); instead UPDATEs the existing row's `cert_der`, `cert_fingerprint`, `valid_from`, `valid_to`, `subject_dn`, `issuer_dn`, `last_refresh_at`, `fetched_at` in a single short tx.  The UPDATE asserts `rows_affected == 1`; a 0 means the active row was concurrently deactivated and is treated as a typed DB error, not silently absorbed.  Returns `RefreshedInPlace { ski }`.
- [ ] **Key-roll (new SKI)** runs ONE `with_immediate(pool, |conn| ...)` containing: `INSERT … ON CONFLICT(ski_hex) DO UPDATE … WHERE operator_certs.fiscal_number = excluded.fiscal_number AND operator_certs.active = 0` (idempotent stage that REFUSES to overwrite a row owned by a different fiscal_number or one already at active=1 — sqlite_constraint or rows_affected==0 on those, both surfaced as a typed error), then `UPDATE … SET active=0 WHERE fiscal_number=? AND active=1`, `UPDATE … SET active=1 WHERE ski_hex=?`, and `audit_log INSERT`.  All in one tx.  Both UPDATEs and the stage assert `rows_affected == 1` — any mismatch returns a typed error and the with_immediate ROLLBACKs.  Why NOT `INSERT OR REPLACE`: SQLite REPLACE is DELETE+INSERT and would silently wipe a foreign-owned `ski_hex` row (legal/cert artefacts), so we use `ON CONFLICT … DO UPDATE` with a same-fiscal-number, active=0 guard.  No `stage_inactive_cert` INSERT outside the tx — the previous design had a stage-then-flip window where a crash left an orphan staged row that a retry could not re-stage (PK conflict on `operator_certs.ski_hex`); the unified-tx design closes that window.  Returns `RefreshedKeyRoll { ski_old, ski_new }`.
- [ ] Cert metadata extraction goes through a new additive helper `prro_crypto::cms::envelope::parse_cert_basic_fields(cert_der) -> Result<BasicCertFields, EnvelopeError>` returning `valid_from`, `valid_to`, `subject_dn`, `issuer_dn`.  This is a **prerequisite additive PR against `prro_crypto`** that lands BEFORE W2 implementation begins.  Cross-references: W0-3 §3 (last row, classification: additive, single-PR scope) and W0-3 §3 amendment 2026-05-05.  W2 step 0 below files the bd issue + lands the additive PR; only after that PR merges does the W2 wiring step proceed.  No ad-hoc ASN.1 walker lands inside `rust/prro/src/services/`.
- [ ] Multi-URL fallback reads from the new `ca_endpoints` table (priority-ordered, enabled-only).  Each per-URL probe is bounded by `cfg.cmp_request_timeout` (loaded from `cert_provisioning_config.cmp_request_timeout_secs`, default 15s).  If one URL returns transport error / parse error / SKI mismatch / timeout, the next is tried; if all fail, the function returns `Err(RefreshError::AllUrlsFailed)` without touching the DB.
- [ ] `refresh_within_days` honoured: a cert whose `valid_to - now > refresh_within_days` is NOT refreshed (returns `NoChange`).  One whose `valid_to - now <= refresh_within_days` IS refreshed.  The freshly-written `valid_to` in step 6 of `refresh_for_fn` MUST be the new cert's lifetime — not `Utc::now()` — so the next refresh cycle's eligibility check reads the correct expiry.
- [ ] No CMP fetch / network call happens inside any `with_immediate` block (W5 will static-assert this; W2 must not introduce a violation).
- [ ] Malformed inputs are fail-closed:
  - unparseable `valid_to` in the active-cert row → `RefreshError::MalformedCertMetadata { fn_id, field: "valid_to" }`
  - wrong-length / non-hex `ski_hex` → `RefreshError::MalformedCertMetadata { fn_id, field: "ski_hex" }`
  - malformed cert DER on metadata extraction → `RefreshError::MalformedCert`
  None panic; none fall through to `Utc::now()` fail-open.
- [ ] Smoke tests pass against a `wiremock` HTTP byte-replay server using the vendored fixture corpus, plus a unit test for the same-SKI vs key-roll branch decision and the rows_affected guard (a test fixture that pre-deactivates the row before the flip must produce a typed error, not a silent success).

**Verify:**

```bash
cargo test -p prro --test cert_refresher_smoke
```

**Steps:**

- [ ] **Step 0 (gating prerequisite): Land additive `prro_crypto::cms::envelope::parse_cert_basic_fields`.**

This step happens BEFORE any code in `rust/prro/src/services/` is written.

```bash
# 1) File the bd issue against the M2 epic so the PR has a tracked id.
bd add --type task --title "prro_crypto: parse_cert_basic_fields helper (M2/W2 prerequisite)" \
       --parent <M2-epic-id> --discovered-from <plan-fix-pass-3-commit>

# 2) Implement the helper in a separate PR against rust/prro_crypto/.
#    Suggested location: rust/prro_crypto/src/cms/envelope.rs (next to
#    extract_cert_pubkey_bytes — same DER walker prefix).
#    Returned struct shape:
#      pub struct BasicCertFields {
#          pub valid_from: chrono::DateTime<chrono::Utc>,
#          pub valid_to:   chrono::DateTime<chrono::Utc>,
#          pub subject_dn: String,   // RFC 4514 DN string
#          pub issuer_dn:  String,
#      }
#    Tests: a fixture cert (vendored under rust/prro_crypto/tests/fixtures/)
#    asserts each field byte-equal to a known-good value.
#    Acceptance: cargo test -p prro_crypto passes; cargo build -p prro
#    against the new helper + an unmodified rust/prro/ HEAD compiles.

# 3) Merge the prro_crypto PR before continuing W2 step 1.
```

Why this is its own step, not folded into step 1: W0-3 §3 classifies it
additive but it is a NEW symbol in `prro_crypto`; landing it as a
discrete PR (with its own tests + review + commit) is the contract this
plan owes to the architecture rule "no edits under `rust/prro_crypto/
src/**` from inside a `prro` task".  Also unblocks W2 cleanly: the
implementer of `services::cert_refresher` can `use
prro_crypto::cms::envelope::parse_cert_basic_fields` from a published
helper rather than racing the dependency.

- [ ] **Step 1: Add deps + port `ca_endpoints` migration.**

In `rust/prro/Cargo.toml` `[dependencies]` add:

```toml
sha2 = "0.10"
chrono = { version = "0.4", features = ["serde"] }   # already in M1 — verify, don't dup
prro_crypto = { path = "../prro_crypto", features = ["tsp_http"] }   # enable feature for fetch_cert_by_ski
```

`[dev-dependencies]`:

```toml
wiremock = "0.6"
```

Then create `rust/prro/migrations/006_ca_endpoints.sql` mirroring the
shape of the legacy `sql/016_ca_endpoints.sql`.  Minimal viable schema:

```sql
-- 006 — CA endpoint registry for cert_provisioning multi-URL retry.
-- Vendored from legacy sql/016_ca_endpoints.sql.  The IIT proprietary
-- wire protocol used by every listed CMP endpoint is identical;
-- only the host differs.

CREATE TABLE ca_endpoints (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT    NOT NULL UNIQUE,
    cmp_url         TEXT    NOT NULL,
    issuer_pattern  TEXT,           -- case-insensitive substring vs cert issuer DN; nullable
    priority        INTEGER NOT NULL DEFAULT 0,
    enabled         INTEGER NOT NULL DEFAULT 1  CHECK (enabled IN (0,1)),
    created_at      TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at      TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP)
) STRICT;

CREATE INDEX ix_ca_endpoints_priority ON ca_endpoints(priority) WHERE enabled = 1;

-- Seed the two production CMP URLs WITH the /services/cmp/ path.
-- The M1 default `primary_cmp_url='http://acskidd.gov.ua:80'` lacks
-- this path and is incomplete for a direct CMP request.
INSERT INTO ca_endpoints (name, cmp_url, issuer_pattern, priority) VALUES
    ('acskidd', 'http://acskidd.gov.ua:80/services/cmp/', 'acskidd', 10),
    ('ca.tax.gov.ua', 'http://ca.tax.gov.ua:80/services/cmp/', 'tax', 20);

-- Per-URL CMP request timeout.  Forwarded to
-- `prro_crypto::cms::cmp::fetch_cert_by_ski(_, _, timeout)` for every
-- probe.  Default 15s — generous for a 1-RTT CMP exchange but bounded
-- so a hung CMP host can't stall the refresher loop.
ALTER TABLE cert_provisioning_config
    ADD COLUMN cmp_request_timeout_secs INTEGER NOT NULL DEFAULT 15
    CHECK (cmp_request_timeout_secs BETWEEN 1 AND 120);
```

Acceptance: `cargo test -p prro --test migrations_apply` passes (M1 sub-test
extended to assert the new `ca_endpoints` table is reachable post-006).

- [ ] **Step 2: Add `pub mod services;` to `rust/prro/src/lib.rs`.**

```rust
pub mod app;
pub mod config;
pub mod crypto;
pub mod db;
pub mod doctor;
pub mod runtime;
pub mod services;      // ← new
```

- [ ] **Step 3: Write `rust/prro/src/services/mod.rs`.**

```rust
//! Service-layer modules.  Per ADR-M2-6, `services::*` MAY take a
//! `SqlitePool` because their job is to orchestrate DB writes around
//! crypto/transport calls.  The crypto/transport modules themselves do
//! NOT see DB handles.

pub mod cert_refresher;
```

- [ ] **Step 4: Write `rust/prro/src/services/cert_refresher.rs`.**

The full source is ~250 lines; the implementer follows this skeleton:

```rust
//! Async cert refresh.
//!
//! Pipeline (per ADR-M2-4 + ADR-M2-6):
//!   1. Load FN row + cert_provisioning_config + ca_endpoints (DB read; no
//!      tx).
//!   2. If currently-active cert's `valid_to - now > refresh_within_days`,
//!      return NoChange.
//!   3. Compute the SKI to fetch (= currently-active cert's SKI for refresh,
//!      or a separately-supplied SKI for first-time provisioning).
//!   4. Call `provider.fetch_cert_by_ski(urls, ski, cfg.cmp_request_timeout)` — outside any tx.
//!   5. Parse cert metadata (SKI, valid_from/to, subject/issuer DN).
//!   6. If the new SKI matches the active SKI → in-place UPDATE the
//!      existing active=1 row (single short tx, rows_affected==1).
//!   7. Else (key-roll) → ONE `with_immediate` tx that runs:
//!        INSERT INTO operator_certs … active=0
//!          ON CONFLICT(ski_hex) DO UPDATE …
//!          WHERE operator_certs.fiscal_number = excluded.fiscal_number
//!            AND operator_certs.active = 0   (idempotent stage that
//!            REFUSES to clobber foreign-owned or active=1 rows)
//!        UPDATE … SET active=0 WHERE fiscal_number=? AND active=1
//!        UPDATE … SET active=1 WHERE ski_hex=?
//!        INSERT INTO audit_log
//!      — atomic + idempotent on retry (no orphan staged-row window).
//!   8. Return RefreshedInPlace { ski } | RefreshedKeyRoll { old, new }.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use sqlx::SqlitePool;

use crate::crypto::{CertDer, CryptoError, CryptoProvider};
use crate::db::tx::with_immediate;

#[derive(Debug, Clone)]
pub struct RefreshConfig {
    pub refresh_within_days: i64,
    /// Per-URL CMP probe timeout, forwarded to
    /// `CryptoProvider::fetch_cert_by_ski`.  Defaults to 15s; sourced
    /// from `cert_provisioning_config.cmp_request_timeout_secs` if the
    /// column is present.
    pub cmp_request_timeout: std::time::Duration,
}

#[derive(Debug, Clone)]
pub enum RefreshOutcome {
    NoChange,
    /// Same-SKI refresh: in-place UPDATE only, no PK conflict on
    /// `operator_certs.ski_hex`.
    RefreshedInPlace { ski: String },
    /// New SKI: stage at active=0 + atomic flip via with_immediate.
    RefreshedKeyRoll { ski_old: String, ski_new: String },
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum RefreshError {
    #[error("no active cert for FN {fn_id}")]
    NoActiveCert { fn_id: String },
    #[error("no enabled CA endpoints")]
    NoEnabledEndpoints,
    #[error("CMP fetch failed across all URLs")]
    AllUrlsFailed,
    #[error("CMP fetch returned a cert whose SKI differs from request")]
    SkiMismatch,
    #[error("malformed cert metadata in operator_certs row for FN {fn_id}: {field}")]
    MalformedCertMetadata { fn_id: String, field: &'static str },
    #[error("malformed cert DER bytes returned from CMP")]
    MalformedCert,
    #[error("DB error: {0}")]
    Db(String),
    #[error("crypto provider error: {0:?}")]
    Crypto(crate::crypto::CryptoError),
}

pub async fn refresh_for_fn(
    pool: &SqlitePool,
    fn_id: &str,
    provider: Arc<dyn CryptoProvider>,
) -> Result<RefreshOutcome, RefreshError> {
    let cfg = load_refresh_config(pool).await.map_err(|e| RefreshError::Db(e.to_string()))?;
    let active = load_active_cert(pool, fn_id).await?
        .ok_or_else(|| RefreshError::NoActiveCert { fn_id: fn_id.to_string() })?;

    let now = Utc::now();
    if active.valid_to - now > Duration::days(cfg.refresh_within_days) {
        return Ok(RefreshOutcome::NoChange);
    }

    let urls = load_ca_urls(pool).await.map_err(|e| RefreshError::Db(e.to_string()))?;
    if urls.is_empty() {
        return Err(RefreshError::NoEnabledEndpoints);
    }
    // Fail-closed: hex_to_ski rejects malformed/wrong-length input with a
    // typed error rather than silently filling in zero bytes.
    let ski_bytes = hex_to_ski(&active.ski_hex)
        .map_err(|reason| RefreshError::MalformedCertMetadata {
            fn_id: fn_id.to_string(),
            field: reason,
        })?;

    let new_cert: CertDer = provider
        .fetch_cert_by_ski(&urls, &ski_bytes, cfg.cmp_request_timeout)
        .await
        .map_err(map_crypto_to_refresh)?;

    // SKI + lifetime + DN extraction are fail-closed: malformed cert DER
    // produces a typed error, not a panic.  All four metadata fields are
    // persisted so a future refresh cycle can re-compute `valid_to - now`
    // without re-fetching the cert.
    let parsed = parse_cert_metadata(&new_cert.0)?;
    let new_ski_hex = parsed.ski_hex.clone();
    let active_ski = active.ski_hex.clone();
    let fn_id_owned = fn_id.to_string();

    if new_ski_hex == active_ski {
        // Same-SKI refresh: in-place UPDATE.  No staged row (would PK-conflict
        // with operator_certs.ski_hex), no atomic flip needed (still exactly
        // one active=1 for this FN throughout).  rows_affected MUST be 1 —
        // a 0 update means the active row was concurrently flipped/deleted,
        // which is a bug we want to surface, not paper over.
        in_place_refresh(pool, &active_ski, &new_cert.0, &parsed).await
            .map_err(|e| RefreshError::Db(e.to_string()))?;
        return Ok(RefreshOutcome::RefreshedInPlace { ski: new_ski_hex });
    }

    // Key-roll: new SKI != old.  Stage + flip + audit run inside ONE
    // `with_immediate` tx so the operation is atomic AND idempotent on
    // retry: a crash before COMMIT leaves no orphan staged row (tx never
    // committed); a crash AFTER COMMIT means the row is already there and
    // active=1 for the new SKI, so a subsequent refresh_for_fn call would
    // see `valid_to - now > refresh_within_days` and return NoChange.
    // The stage uses INSERT … ON CONFLICT(ski_hex) DO UPDATE WHERE
    // operator_certs.fiscal_number = excluded.fiscal_number AND active = 0
    // — so a stale staged row from a prior interrupted refresh (same fn,
    // active=0) is harmlessly overwritten, while a foreign-owned ski_hex
    // (different fn) or an active=1 row is REFUSED (sqlite_constraint or
    // rows_affected==0 → typed error → with_immediate ROLLBACKs).  Why
    // not INSERT OR REPLACE: REPLACE is DELETE+INSERT and would silently
    // wipe a foreign-owned cert row, destroying ownership / metadata for
    // an unrelated operator.
    let new_ski_for_tx = new_ski_hex.clone();
    let active_ski_for_tx = active_ski.clone();
    let fn_id_for_tx = fn_id_owned.clone();
    let parsed_for_tx = parsed.clone();
    let new_cert_for_tx = new_cert.0.clone();
    with_immediate(pool, move |conn| {
        Box::pin(async move {
            let now_iso = Utc::now().to_rfc3339();

            // Stage at active=0.  Why NOT `INSERT OR REPLACE`:
            // SQLite REPLACE is implemented as DELETE-then-INSERT, which
            // means an existing `ski_hex` row that does NOT belong to
            // this `fn_id` (e.g. a row owned by a different operator
            // who happens to have the same SKI hash via a malicious or
            // accidental collision, or a row left stuck at active=1
            // from a prior interrupted refresh) would be silently
            // wiped — destroying ownership / metadata / the active=1
            // flag for a different fiscal_number.  The legal artefact
            // here (the cert + its valid_to / issuer DN / audit trail)
            // is too important to overwrite without a guard.
            //
            // Use INSERT … ON CONFLICT(ski_hex) DO UPDATE … with a
            // WHERE clause that ONLY matches a stale stage row left by
            // this same fiscal_number's previous failed refresh
            // (active = 0, same fiscal_number).  Any other conflicting
            // row produces sqlite_constraint and the with_immediate
            // ROLLBACKs cleanly.
            let stage_result = sqlx::query(
                "INSERT INTO operator_certs( \
                     ski_hex, fiscal_number, cert_fingerprint, cert_der, \
                     valid_from, valid_to, subject_dn, issuer_dn, \
                     fetched_at, source, active) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'cmp', 0) \
                 ON CONFLICT(ski_hex) DO UPDATE SET \
                     cert_fingerprint = excluded.cert_fingerprint, \
                     cert_der         = excluded.cert_der, \
                     valid_from       = excluded.valid_from, \
                     valid_to         = excluded.valid_to, \
                     subject_dn       = excluded.subject_dn, \
                     issuer_dn        = excluded.issuer_dn, \
                     fetched_at       = excluded.fetched_at, \
                     source           = excluded.source \
                 WHERE operator_certs.fiscal_number = excluded.fiscal_number \
                   AND operator_certs.active = 0",
            )
            .bind(&new_ski_for_tx)
            .bind(&fn_id_for_tx)
            .bind(compute_fingerprint(&new_cert_for_tx))
            .bind(&new_cert_for_tx)
            .bind(parsed_for_tx.valid_from.to_rfc3339())
            .bind(parsed_for_tx.valid_to.to_rfc3339())
            .bind(&parsed_for_tx.subject_dn)
            .bind(&parsed_for_tx.issuer_dn)
            .bind(&now_iso)
            .execute(&mut *conn)
            .await?;
            // INSERT path → rows_affected == 1.
            // ON CONFLICT path that matched our WHERE → rows_affected == 1.
            // ON CONFLICT path where the WHERE filtered us out (foreign
            // ownership or already-active) → rows_affected == 0; we
            // fail the tx so the operator gets a typed error instead
            // of a silent "stage succeeded" that didn't actually stage.
            if stage_result.rows_affected() != 1 {
                return Err(sqlx::Error::Protocol(format!(
                    "key-roll stage: ski_hex {} already exists for a \
                     different fiscal_number or is already active=1; \
                     refusing to overwrite (rows_affected={})",
                    new_ski_for_tx,
                    stage_result.rows_affected()
                )));
            }

            // Both UPDATEs MUST hit exactly one row.  rows_affected != 1
            // means concurrent state mutation or a logic bug; abort the
            // tx (Err triggers ROLLBACK via with_immediate's contract).
            let r1 = sqlx::query(
                "UPDATE operator_certs SET active = 0 \
                 WHERE fiscal_number = ? AND active = 1",
            )
            .bind(&fn_id_for_tx)
            .execute(&mut *conn)
            .await?;
            if r1.rows_affected() != 1 {
                return Err(sqlx::Error::Protocol(format!(
                    "key-roll deactivate: expected 1 row, got {}",
                    r1.rows_affected()
                )));
            }

            let r2 = sqlx::query(
                "UPDATE operator_certs SET active = 1 WHERE ski_hex = ?",
            )
            .bind(&new_ski_for_tx)
            .execute(&mut *conn)
            .await?;
            if r2.rows_affected() != 1 {
                return Err(sqlx::Error::Protocol(format!(
                    "key-roll activate: expected 1 row, got {}",
                    r2.rows_affected()
                )));
            }

            sqlx::query(
                "INSERT INTO audit_log(entity_type, entity_id, event_type, severity, actor, event_payload_json) \
                 VALUES ('fn', ?, 'cert_refresh_key_roll', 'INFO', 'cert_refresher', ?)",
            )
            .bind(&fn_id_for_tx)
            .bind(format!(r#"{{"ski_old":"{}","ski_new":"{}"}}"#, active_ski_for_tx, new_ski_for_tx))
            .execute(&mut *conn)
            .await?;
            Ok(())
        })
    })
    .await
    .map_err(|e| RefreshError::Db(e.to_string()))?;

    Ok(RefreshOutcome::RefreshedKeyRoll {
        ski_old: active_ski,
        ski_new: new_ski_hex,
    })
}

/// Same-SKI refresh: cert renewed but key kept.  UPDATEs the existing
/// active=1 row, including the freshly-parsed `valid_from`, `valid_to`,
/// `subject_dn`, `issuer_dn` so a future refresh cycle reads accurate
/// expiry without a re-fetch.  Single short tx.  No staging, no flip.
/// Asserts `rows_affected == 1`.
async fn in_place_refresh(
    pool: &SqlitePool,
    ski_hex: &str,
    new_cert_der: &[u8],
    parsed: &ParsedCertMetadata,
) -> Result<(), sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    let r = sqlx::query(
        "UPDATE operator_certs \
         SET cert_der = ?, cert_fingerprint = ?, \
             valid_from = ?, valid_to = ?, subject_dn = ?, issuer_dn = ?, \
             fetched_at = ?, last_refresh_at = ? \
         WHERE ski_hex = ? AND active = 1",
    )
    .bind(new_cert_der)
    .bind(compute_fingerprint(new_cert_der))
    .bind(parsed.valid_from.to_rfc3339())
    .bind(parsed.valid_to.to_rfc3339())
    .bind(&parsed.subject_dn)
    .bind(&parsed.issuer_dn)
    .bind(&now)
    .bind(&now)
    .bind(ski_hex)
    .execute(pool)
    .await?;
    if r.rows_affected() != 1 {
        return Err(sqlx::Error::Protocol(format!(
            "in_place_refresh: expected 1 row, got {} for ski={}",
            r.rows_affected(),
            ski_hex
        )));
    }
    Ok(())
}

fn map_crypto_to_refresh(e: CryptoError) -> RefreshError {
    use crate::crypto::FetchKind;
    match e {
        CryptoError::CertFetch { reason: FetchKind::AllUrlsFailed } => RefreshError::AllUrlsFailed,
        CryptoError::CertFetch { reason: FetchKind::SkiMismatch } => RefreshError::SkiMismatch,
        other => RefreshError::Crypto(other),
    }
}

#[derive(Debug, Clone)]
struct ActiveCertRow {
    ski_hex: String,
    valid_to: DateTime<Utc>,
}

async fn load_refresh_config(pool: &SqlitePool) -> sqlx::Result<RefreshConfig> {
    // M1 schema exposes `refresh_within_days` and `cmp_request_timeout_secs`
    // (the latter added by W2 migration 006 alongside `ca_endpoints`).  If
    // a future migration adds more columns, extend the SELECT explicitly
    // — do not `SELECT *` here.
    let row: (i64, i64) = sqlx::query_as(
        "SELECT refresh_within_days, cmp_request_timeout_secs \
         FROM cert_provisioning_config WHERE id = 1",
    )
    .fetch_one(pool)
    .await?;
    Ok(RefreshConfig {
        refresh_within_days: row.0,
        cmp_request_timeout: std::time::Duration::from_secs(row.1.max(1) as u64),
    })
}

async fn load_active_cert(pool: &SqlitePool, fn_id: &str) -> Result<Option<ActiveCertRow>, RefreshError> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT ski_hex, valid_to FROM operator_certs WHERE fiscal_number = ? AND active = 1",
    )
    .bind(fn_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| RefreshError::Db(e.to_string()))?;
    match row {
        None => Ok(None),
        Some((ski_hex, valid_to_str)) => {
            let valid_to = parse_iso8601(fn_id, "valid_to", &valid_to_str)?;
            Ok(Some(ActiveCertRow { ski_hex, valid_to }))
        }
    }
}

async fn load_ca_urls(pool: &SqlitePool) -> sqlx::Result<Vec<String>> {
    // Read enabled endpoints in priority order from ca_endpoints (M2 W2
    // migration 006).  The cmp_url column already includes the
    // `/services/cmp/` path component.  cert_provisioning_config's
    // primary/fallback columns are deprecated and not consulted here.
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT cmp_url FROM ca_endpoints WHERE enabled = 1 ORDER BY priority ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(u,)| u).collect())
}

// `stage_inactive_cert` (a stand-alone INSERT outside any tx) is
// intentionally absent: the W2 refresh path now stages-and-flips inside
// ONE `with_immediate` block (see `refresh_for_fn` above).  This makes
// the operation idempotent on retry — a crash between the old "stage
// outside tx" + "flip inside tx" pair would have left an orphan
// active=0 row that a retry could not stage again (PK conflict on
// `operator_certs.ski_hex`); the new design has no such window.

/// Fail-closed hex → 32-byte SKI converter.  Rejects wrong-length input
/// and any non-hex character with a typed reason instead of returning
/// zero bytes (the previous implementation silently mapped a corrupt
/// `ski_hex` value to all-zeros, which the CMP server would accept and
/// resolve to a wrong cert).
fn hex_to_ski(hex: &str) -> Result<[u8; 32], &'static str> {
    if hex.len() != 64 {
        return Err("ski_hex");
    }
    let bytes = hex.as_bytes();
    let mut out = [0u8; 32];
    for i in 0..32 {
        let hi = hex_digit(bytes[i * 2]).ok_or("ski_hex")?;
        let lo = hex_digit(bytes[i * 2 + 1]).ok_or("ski_hex")?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

#[derive(Debug, Clone)]
struct ParsedCertMetadata {
    ski_hex: String,
    valid_from: DateTime<Utc>,
    valid_to: DateTime<Utc>,
    subject_dn: String,
    issuer_dn: String,
}

/// Parse the four cert-metadata fields the refresh service needs to
/// persist into `operator_certs`.  All four are extracted in one pass
/// to avoid four redundant ASN.1 walks.
///
/// **Required `prro_crypto` extension (W0-3 §3 — additive).**
/// `prro_crypto` exposes `extract_cert_pubkey_bytes` + `compute_ski`
/// today but does NOT expose `validity` / `subject` / `issuer` field
/// extractors.  The implementer adds a single helper
/// `prro_crypto::cms::envelope::parse_cert_basic_fields(cert_der)
/// -> Result<BasicCertFields, EnvelopeError>` (returning `valid_from`,
/// `valid_to`, `subject_dn`, `issuer_dn` as DER-walk products) before
/// wiring W2.  This is an additive PR against `prro_crypto`; do NOT
/// inline an ad-hoc ASN.1 walker into `rust/prro/src/services/`.
/// Until that helper lands, W2 cannot satisfy its acceptance — the
/// follow-up is filed as `bd add` on the M2 epic at task start.
fn parse_cert_metadata(cert_der: &[u8]) -> Result<ParsedCertMetadata, RefreshError> {
    use prro_crypto::cms::envelope::{
        compute_ski, extract_cert_pubkey_bytes, parse_cert_basic_fields,
    };
    let pubkey = extract_cert_pubkey_bytes(cert_der).map_err(|_| RefreshError::MalformedCert)?;
    let ski = compute_ski(&pubkey);
    let basic = parse_cert_basic_fields(cert_der).map_err(|_| RefreshError::MalformedCert)?;
    Ok(ParsedCertMetadata {
        ski_hex: ski.iter().map(|b| format!("{:02x}", b)).collect(),
        valid_from: basic.valid_from,
        valid_to: basic.valid_to,
        subject_dn: basic.subject_dn,
        issuer_dn: basic.issuer_dn,
    })
}

fn compute_fingerprint(cert_der: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(cert_der))
}

fn parse_iso8601(fn_id: &str, field: &'static str, s: &str) -> Result<DateTime<Utc>, RefreshError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| RefreshError::MalformedCertMetadata { fn_id: fn_id.to_string(), field })
}
```

> Note: `sha2` is not yet in M1's `Cargo.toml`; the implementer adds it in Step 1.  Update `rust/prro/Cargo.toml` `[dependencies]`:
>
> ```toml
> sha2 = "0.10"
> ```

- [ ] **Step 5: Vendor a test-CA fixture under `rust/prro/tests/fixtures/test_ca/`.**

For each `(ski, response)` pair the smoke test exercises, capture the bytes from a real CA round-trip (operator runs the W0-2 capture procedure) and write to `rust/prro/tests/fixtures/test_ca/cert_<ski_short>.der`.  W2 ships at minimum two fixtures (happy + SKI-mismatch).

- [ ] **Step 6: Write `rust/prro/tests/cert_refresher_smoke.rs`.**

```rust
//! W2 smoke: cert_refresher against a wiremock byte-replay HTTP server.

use std::sync::Arc;

use prro::crypto::InProcessProvider;
use prro::services::cert_refresher::{refresh_for_fn, RefreshOutcome};
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

async fn fresh_pool_with_fn() -> (sqlx::SqlitePool, String) {
    let dir = tempfile::tempdir().unwrap();
    std::mem::forget(dir);
    let pool = prro::db::open_pool(&std::path::PathBuf::from("/tmp/m2_w2_fresh.db"))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();
    (pool, "1234567890".to_string())
}

#[tokio::test]
async fn refresh_returns_no_change_when_cert_far_from_expiry() {
    // Implementer wires a fresh test pool, inserts an active cert with
    // valid_to = now + 365 days, calls refresh_for_fn, asserts NoChange.
    // Network mock not even started — refresh_within_days short-circuits.
    let (_pool, _fn_id) = fresh_pool_with_fn().await;
    // ... fixture setup ...
    let provider: Arc<dyn prro::crypto::CryptoProvider> = Arc::new(InProcessProvider::new());
    // ... assert RefreshOutcome::NoChange ...
    let _ = provider;
}

#[tokio::test]
async fn refresh_falls_back_to_second_url_when_first_returns_5xx() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(include_bytes!("fixtures/test_ca/cert_aabbccdd.der").to_vec()))
        .mount(&mock)
        .await;
    // ... wire ca_endpoints with two rows pointing at mock.uri() (same
    //     host, different priorities — wiremock counts requests in
    //     order, so `up_to_n_times(1)` 503 then 200 simulates first-url
    //     fail / second-url succeed) ...
    // ... call refresh_for_fn, assert
    //     matches!(result, Ok(RefreshOutcome::RefreshedKeyRoll {
    //         ski_old, ski_new })) where ski_new is the new cert's SKI ...
}

#[tokio::test]
async fn refresh_returns_all_urls_failed_without_touching_db() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock)
        .await;
    // ... wire two URLs that both 500 ...
    // ... call refresh_for_fn, assert
    //     matches!(result, Err(RefreshError::AllUrlsFailed)) ...
    // ... assert active cert in DB unchanged ...
}

#[tokio::test]
async fn atomic_flip_preserves_one_active_per_fn() {
    // After Refreshed, query DB for COUNT(*) WHERE fiscal_number=? AND active=1
    // — must be exactly 1, and ski_hex must equal ski_new.
}
```

> The smoke test code above is structural; the implementer fills in fixture wiring + assertions per the acceptance criteria.

- [ ] **Step 7: Build + test.**

```bash
cargo build -p prro
cargo test -p prro --test cert_refresher_smoke
```

Expected: 4 tests pass.

- [ ] **Step 8: Commit.**

```bash
git add rust/prro/Cargo.toml rust/prro/Cargo.lock rust/prro/src/lib.rs \
        rust/prro/src/services/ rust/prro/tests/cert_refresher_smoke.rs \
        rust/prro/tests/fixtures/test_ca/
git commit -m "feat(rust/services): cert_refresher with multi-URL fallback + atomic flip (M2/W2)"
git push origin rust-gateway
```

---

## Task 3 (W3) — `prro::transports::dps` gRPC channel + native tonic mock

**Goal:** Land the typed `DpsChannel` trait and a tonic-built `GrpcDpsChannel` impl backed by `fiscal_server.proto`.  Tests run against a native Rust tonic mock that mirrors the contract subset.  Encode `ByServerFiscalNo = lastChk(fn_sign) + response.id match` per `PRRO_GATE-5js`.

**Day budget:** 5-7 days.  Largest task: proto vendoring, `tonic_build` integration, mock server, lastChk semantic encoding, error categorisation.

**Implements:** ADR-M2-2, ADR-M2-6.  Uses W0-1 contract subset.  Closes `PRRO_GATE-5js`.  blockedBy: W1.

**Files:**

- Modify: `rust/prro/Cargo.toml` (add `tonic`, `prost`; `tonic-build` to `[build-dependencies]`)
- Modify: `rust/prro/build.rs` (add `tonic_build::compile_protos`)
- Create: `rust/prro/proto/fiscal_server.proto` (vendored from Python tree)
- Modify: `rust/prro/src/lib.rs` (`pub mod transports;`)
- Create: `rust/prro/src/transports/mod.rs`
- Create: `rust/prro/src/transports/dps/{mod,channel,gen,types,errors}.rs`
- Create: `rust/prro/tests/dps_channel_smoke.rs`

**Acceptance Criteria:**

- [ ] `tonic_build` generates Rust types from `rust/prro/proto/fiscal_server.proto` at build time; `cargo build -p prro` clean on all four targets.
- [ ] `DpsChannel` trait is `Send + Sync`; methods take typed inputs and return typed outputs; NO `SqlitePool` / `SqliteConnection` / `Pool` / `Transaction` in any signature.
- [ ] Trait methods covered (mapped to actual proto rpcs): `submit` → `send_chk_v2`, `last_chk` → `last_chk`, `ping` → `ping`, `status_rro` → `status_rro`, `info_rro` → `info_rro`, plus `query_by_local_identity` (returns `QueryNotSupported` typed variant per W0-1 finding) and the `query_by_server_fiscal_no` default impl that calls `last_chk` + asserts `response.id == expected_fiscal_id` per `PRRO_GATE-5js`.
- [ ] `query_by_server_fiscal_no` is implemented on top of `last_chk`: it signs the FN, calls `last_chk(fn_sign)`, and asserts `response.id == expected_fiscal_id`; mismatch returns a typed `DpsError::ServerFiscalIdMismatch`.
- [ ] Native Rust tonic mock in `tests/dps_channel_smoke.rs` exercises happy path + error categorisation: `INVALID_ARGUMENT`, `UNAUTHENTICATED`, `DEADLINE_EXCEEDED`, `UNAVAILABLE`, transport drop mid-call → distinct typed error variants.
- [ ] Connection reuse: `GrpcDpsChannel` holds the tonic `Channel` for a logical session, not per-request.
- [ ] No streaming methods (W0-1 confirmed all RPCs unary).

**Verify:**

```bash
cargo test -p prro --test dps_channel_smoke
```

**Steps:**

- [ ] **Step 1: Add deps.**

In `rust/prro/Cargo.toml` `[dependencies]`:

```toml
tonic = "0.12"
prost = "0.13"
```

In `[build-dependencies]` (new section if absent):

```toml
[build-dependencies]
tonic-build = "0.12"
```

- [ ] **Step 2: Vendor the proto.**

```bash
mkdir -p rust/prro/proto
cp src/prro_gateway/transports/proto/fiscal_server.proto rust/prro/proto/fiscal_server.proto
git add rust/prro/proto/fiscal_server.proto
```

- [ ] **Step 3: Wire `tonic_build` in `rust/prro/build.rs`.**

Replace the file with:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=migrations");
    println!("cargo:rerun-if-changed=proto/fiscal_server.proto");
    tonic_build::configure()
        .build_server(true)   // for the mock in tests
        .build_client(true)
        .compile_protos(&["proto/fiscal_server.proto"], &["proto"])?;
    Ok(())
}
```

- [ ] **Step 4: Add `pub mod transports;` to `rust/prro/src/lib.rs`.**

```rust
pub mod transports;
```

- [ ] **Step 5: Write `rust/prro/src/transports/mod.rs`.**

```rust
pub mod dps;
```

- [ ] **Step 6: Write `rust/prro/src/transports/dps/gen.rs` (wraps tonic-generated module).**

```rust
//! Re-export of the tonic-generated module under a stable path.  The proto
//! `package com.programika.rro.ws.chk;` is what `include_proto!` resolves
//! to (per the canonical schema confirmed in W0-1 + verified against
//! `src/prro_gateway/transports/proto/fiscal_server.proto:6`).

#![allow(clippy::all)]
tonic::include_proto!("com.programika.rro.ws.chk");
```

The generated client lives at `chk_income_service_client::ChkIncomeServiceClient<...>`
(snake_case of the service name `ChkIncomeService` plus the `_client` suffix
from `tonic-build` defaults).  The five generated method functions are
`send_chk_v2`, `last_chk`, `ping`, `status_rro`, `info_rro` (snake_case of
the proto rpc names).  All five RPCs are unary; no streaming.

- [ ] **Step 7: Write `rust/prro/src/transports/dps/types.rs`.**

Typed structs that the trait emits/consumes — never raw protobuf types.  Implementer maps from `gen::*` into these and back.  See W0-1 §3 for the field list.  Skeleton:

```rust
//! Typed DPS request/response structs.  The trait surface uses these,
//! NEVER the raw tonic-generated types.  Field set mirrors
//! `src/prro_gateway/transports/proto/fiscal_server.proto` exactly.

/// `Check` request for `sendChkV2` and `ping`.  The proto `Check`
/// message carries the signed CMS envelope payload as a single bytes
/// field (verified by W3 implementer against the actual proto field
/// name; rename in this struct accordingly).
#[derive(Debug, Clone)]
pub struct CheckRequestPayload {
    pub envelope_der: Vec<u8>,
    pub deadline_ms: u64,
}

/// `CheckRequest` request type for `lastChk`, `statusRro`, `infoRro`.
/// Carries a signed FN identifier for the by-FN lookup pattern
/// described in `PRRO_GATE-5js`.
#[derive(Debug, Clone)]
pub struct SignedRequestPayload {
    pub signed_payload: Vec<u8>,
    pub deadline_ms: u64,
}

/// `CheckResponse` — the unified response type returned by `sendChkV2`,
/// `lastChk`, and `ping`.  Matches the 5-field shape of the canonical
/// `.proto` (id, status, id_sign, data_sign, error_message).
#[derive(Debug, Clone)]
pub struct CheckResponse {
    pub id: String,
    /// Wire-level status enum, decoded as i32 by tonic.  See
    /// `gen::check_response::Status` for the enum variants
    /// (OK, ERROR_VEREFY, ERROR_CHECK, ERROR_SAVE, ERROR_UNKNOWN, etc.).
    pub status: i32,
    pub id_sign: Vec<u8>,
    pub data_sign: Vec<u8>,
    pub error_message: String,
}

/// `StatusResponse` from `statusRro`.  Implementer maps the actual
/// fields from `gen::StatusResponse` here once W3 reads the proto.
#[derive(Debug, Clone)]
pub struct StatusResponse {
    pub status: i32,
    pub data: Vec<u8>,
}

/// `RroInfoResponse` from `infoRro`.
#[derive(Debug, Clone)]
pub struct RroInfoResponse {
    pub data: Vec<u8>,
}
```

> **Deferred.**  `fns_data` was named in W0-1 as a future field; the
> committed proto does NOT carry it as part of `CheckResponse`.  W3
> ships the 5-field shape verbatim; if the production proto later
> grows `fns_data` (or a separate KVT-fetch RPC), W3+1 adds it.

- [ ] **Step 8: Write `rust/prro/src/transports/dps/errors.rs`.**

```rust
//! Typed errors with enum reasons; W0-1 lists the gRPC status codes the
//! mock must produce, so the variant set here mirrors them deterministically.

#[derive(Debug, Clone, thiserror::Error)]
pub enum DpsError {
    #[error("invalid argument: {0:?}")]
    InvalidArgument(InvalidKind),
    #[error("unauthenticated")]
    Unauthenticated,
    #[error("deadline exceeded after {ms}ms")]
    DeadlineExceeded { ms: u64 },
    #[error("server unavailable")]
    Unavailable,
    #[error("transport drop mid-call")]
    TransportDrop,
    #[error("server fiscal id mismatch: expected {expected}, got {actual}")]
    ServerFiscalIdMismatch { expected: String, actual: String },
    #[error("query by local identity unsupported by the production contour")]
    QueryNotSupported,
    #[error("internal: {0}")]
    Internal(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidKind {
    Schema,
    Encoding,
    UnknownField,
}
```

- [ ] **Step 9: Write `rust/prro/src/transports/dps/channel.rs`.**

Trait + impl skeleton:

```rust
//! `DpsChannel` trait + `GrpcDpsChannel` impl.  No DB handle in any
//! signature (ADR-M2-6).  Wraps the tonic-generated `ChkIncomeService`
//! client (per the actual proto package `com.programika.rro.ws.chk`).

use async_trait::async_trait;
use tonic::transport::Channel;

use crate::transports::dps::errors::DpsError;
use crate::transports::dps::gen;     // tonic-generated types
use crate::transports::dps::types::*;

#[async_trait]
pub trait DpsChannel: Send + Sync {
    /// Maps to `ChkIncomeService::sendChkV2(Check) -> CheckResponse`.
    async fn submit(&self, req: CheckRequestPayload) -> Result<CheckResponse, DpsError>;

    /// Maps to `ChkIncomeService::lastChk(CheckRequest) -> CheckResponse`.
    async fn last_chk(&self, req: SignedRequestPayload) -> Result<CheckResponse, DpsError>;

    /// Maps to `ChkIncomeService::ping(Check) -> CheckResponse`.
    async fn ping(&self, req: CheckRequestPayload) -> Result<CheckResponse, DpsError>;

    /// Maps to `ChkIncomeService::statusRro(CheckRequest) -> StatusResponse`.
    async fn status_rro(&self, req: SignedRequestPayload) -> Result<StatusResponse, DpsError>;

    /// Maps to `ChkIncomeService::infoRro(CheckRequest) -> RroInfoResponse`.
    async fn info_rro(&self, req: SignedRequestPayload) -> Result<RroInfoResponse, DpsError>;

    /// Encoded as `last_chk(signed_fn) + response.id == expected_fiscal_id`.
    /// Per `PRRO_GATE-5js`.  Default impl provided so concrete impls
    /// don't have to repeat it.
    async fn query_by_server_fiscal_no(
        &self,
        signed_fn: Vec<u8>,
        expected_fiscal_id: &str,
    ) -> Result<CheckResponse, DpsError> {
        let resp = self
            .last_chk(SignedRequestPayload { signed_payload: signed_fn, deadline_ms: 5_000 })
            .await?;
        if resp.id == expected_fiscal_id {
            Ok(resp)
        } else {
            Err(DpsError::ServerFiscalIdMismatch {
                expected: expected_fiscal_id.to_string(),
                actual: resp.id,
            })
        }
    }

    /// Per W0-1: the production DPS contour does NOT support
    /// query-by-local-identity, so this method always returns
    /// `QueryNotSupported`.  Callers handle this at the service layer.
    async fn query_by_local_identity(&self) -> Result<(), DpsError> {
        Err(DpsError::QueryNotSupported)
    }
}

pub struct GrpcDpsChannel {
    inner: gen::chk_income_service_client::ChkIncomeServiceClient<Channel>,
}

impl GrpcDpsChannel {
    pub async fn connect(endpoint: &str) -> Result<Self, DpsError> {
        let channel = Channel::from_shared(endpoint.to_string())
            .map_err(|_| DpsError::Internal("bad endpoint".into()))?
            .connect()
            .await
            .map_err(|_| DpsError::Unavailable)?;
        Ok(Self {
            inner: gen::chk_income_service_client::ChkIncomeServiceClient::new(channel),
        })
    }
}

#[async_trait]
impl DpsChannel for GrpcDpsChannel {
    async fn submit(&self, req: CheckRequestPayload) -> Result<CheckResponse, DpsError> {
        // implementer wires:
        //   1. map req -> gen::Check (the proto request type for sendChkV2);
        //   2. call self.inner.clone().send_chk_v2(tonic::Request::new(...)).await;
        //   3. on Ok: map gen::CheckResponse (5 fields: id, status, id_sign,
        //      data_sign, error_message) -> CheckResponse;
        //   4. on Err(tonic::Status): map status code -> typed DpsError per
        //      the table in §errors.rs (INVALID_ARGUMENT/UNAUTHENTICATED/
        //      DEADLINE_EXCEEDED/UNAVAILABLE/transport drop).
        let _ = req;
        Err(DpsError::Internal("implementer wires send_chk_v2".into()))
    }
    async fn last_chk(&self, _req: SignedRequestPayload) -> Result<CheckResponse, DpsError> {
        Err(DpsError::Internal("implementer wires last_chk".into()))
    }
    async fn ping(&self, _req: CheckRequestPayload) -> Result<CheckResponse, DpsError> {
        Err(DpsError::Internal("implementer wires ping".into()))
    }
    async fn status_rro(&self, _req: SignedRequestPayload) -> Result<StatusResponse, DpsError> {
        Err(DpsError::Internal("implementer wires status_rro".into()))
    }
    async fn info_rro(&self, _req: SignedRequestPayload) -> Result<RroInfoResponse, DpsError> {
        Err(DpsError::Internal("implementer wires info_rro".into()))
    }
}
```

The implementer fills in each method's tonic call + typed conversion.
Generated module path is `chk_income_service_client::ChkIncomeServiceClient`
(snake_case of `ChkIncomeService` + `_client` suffix from tonic-build
defaults).  The 5 RPC method names on the generated client are
`send_chk_v2`, `last_chk`, `ping`, `status_rro`, `info_rro`.

- [ ] **Step 10: Write `rust/prro/src/transports/dps/mod.rs`.**

```rust
pub mod channel;
pub mod errors;
pub mod gen;
pub mod types;

pub use channel::{DpsChannel, GrpcDpsChannel};
pub use errors::{DpsError, InvalidKind};
pub use types::{
    CheckRequestPayload, CheckResponse, RroInfoResponse, SignedRequestPayload, StatusResponse,
};
```

- [ ] **Step 11: Write `rust/prro/tests/dps_channel_smoke.rs` with embedded tonic mock.**

The mock implements `gen::fiscal_server_server::FiscalServer` and is mounted on a test tonic server bound to `127.0.0.1:0`.  Smoke tests cover happy submit + each error category.  Skeleton ~150 lines; the implementer fills mock arms per method.

- [ ] **Step 12: Build + test on all four targets.**

```bash
cargo build -p prro --target x86_64-unknown-linux-gnu
cargo test  -p prro --target x86_64-unknown-linux-gnu --test dps_channel_smoke
# Repeat for musl, msvc, aarch64 in CI.
```

Expected: tests pass everywhere.

- [ ] **Step 13: Commit.**

```bash
git add rust/prro/Cargo.toml rust/prro/Cargo.lock rust/prro/build.rs \
        rust/prro/proto/ rust/prro/src/lib.rs rust/prro/src/transports/ \
        rust/prro/tests/dps_channel_smoke.rs
git commit -m "feat(rust/transports): DpsChannel + tonic mock + lastChk semantic (M2/W3)"
git push origin rust-gateway
```

Closes `PRRO_GATE-5js`.

---

## Task 4 (W4) — byte-equivalence goldens harness

**Goal:** Land the goldens harness + first round of frozen test vectors covering: canonical unsigned XML for SHIFT_OPEN/SHIFT_CLOSE/SELL/RETURN/Z_REPORT and the deterministic-prefix CMS-signed XML zone.  Plus a manual-only re-capture script.

> **Scope note (W4 first round).** KVT1/KVT2 parser input/struct goldens are explicitly DEFERRED from W4's first round.  W1's `prro::crypto` wrapper does not include a KVT parser — the Python `transports/dps_fiscal_server.py` decoder will be lifted into Rust as a follow-up `prro::crypto::kvt::{parse_kvt1, parse_kvt2}` module gated on W3 (DpsChannel) so fixtures can be captured from real `lastChk` round-trips.  Filed as a discovered-from issue against the M2 epic at W4 task start.

**Day budget:** 4-6 days.  XML builder for the 5 doc types + capture-script wiring + fixture review eat the budget.

**Implements:** ADR-M2-3.  blockedBy: W1, W3.  (W3 is needed because the deterministic-prefix CMS golden is captured from the same `prro::crypto::sign_cms_detached` path the M2 transport will exercise; KVT1/KVT2 parsers are out of scope for W4 first round, see scope note above.)

**Files:**

- Create: `rust/prro/src/xml/` — minimal canonical XML builder for the 5 doc types (no general schema; literally just enough to produce byte-identical output to Python on these 5 cases).
- Create: `rust/prro/tests/goldens_byte_equiv.rs` — the harness.
- Create: `rust/prro/tests/goldens/{xml,cms,prevhash}/*.bin` — frozen fixtures.  (`kvt1/`, `kvt2/` directory layout is reserved by the tree but no fixtures land in W4 first round — see scope note.)
- Create: `rust/prro/tests/goldens/regenerate.py` — manual capture script.
- Create: `rust/prro/tests/goldens/README.md` — operator procedure.
- Create: `docs/M2-goldens-capture.md` — operator-facing procedure.

**Acceptance Criteria:**

- [ ] `tests/goldens_byte_equiv.rs` runs against the frozen `tests/goldens/**/*.bin` files: each test reads a `.bin`, runs the Rust producer (XML builder / CMS signer with deterministic test key), and asserts byte equality.
- [ ] First-round goldens committed: `xml/{shift_open,shift_close,sell,return,z_report}.bin`; `cms/deterministic_prefix.bin` (or similar); `prevhash/seed.bin`.  (KVT1/KVT2 fixtures DEFERRED — see scope note.)
- [ ] `regenerate.py` documents-the-procedure + (when run with a Python checkout side-by-side) re-captures from the live Python and prints a `diff` against the frozen vectors.  CI does NOT run it.
- [ ] If a golden fails (Python output drifts), the test surfaces a diff readable in CI logs (hex dump of first ~64 differing bytes).
- [ ] CMS goldens are split into "deterministic prefix" (the XML-to-be-signed bytes) — pinned byte-equivalent — and "signature shape" (signature is parsed + verified, NOT byte-compared).

**Verify:**

```bash
cargo test -p prro --test goldens_byte_equiv
```

**Steps:**

- [ ] **Step 1: Write a minimal XML builder.**

`rust/prro/src/xml/mod.rs` exposes `build_canonical_xml(doc_type: DocType, payload: &Payload) -> Vec<u8>` for the 5 doc types in scope.  The implementer ports the canonicalisation rules from the Python adapter — attribute ordering, namespace declarations, cp1251 encoding — and pins them in unit tests inside this module.

The builder is intentionally narrow: M3 will replace it with a full schema-driven builder.  W4's job is just to give the harness a Rust-side producer.

- [ ] **Step 2: Capture goldens from Python.**

Operator runs `tests/goldens/regenerate.py` against a Python checkout.  Output lands in `tests/goldens/xml/*.bin` (and the other dirs).  Operator commits the binary fixtures.

- [ ] **Step 3: Write the harness.**

```rust
// tests/goldens_byte_equiv.rs (skeleton)

fn assert_golden_eq(actual: &[u8], golden_path: &str) {
    let expected = std::fs::read(golden_path).unwrap_or_else(|_| panic!("missing golden {golden_path}"));
    if actual != expected {
        let n = actual.len().min(expected.len()).min(64);
        let mut msg = String::new();
        for i in 0..n {
            if actual[i] != expected[i] {
                msg.push_str(&format!("byte {i}: actual={:02x} expected={:02x}\n", actual[i], expected[i]));
            }
        }
        panic!("golden mismatch at {golden_path}:\n{}\nactual_len={} expected_len={}", msg, actual.len(), expected.len());
    }
}

#[test]
fn xml_shift_open_canonical_byte_equal() {
    let payload = test_fixtures::shift_open_payload();
    let actual = prro::xml::build_canonical_xml(prro::xml::DocType::ShiftOpen, &payload);
    assert_golden_eq(&actual, "tests/goldens/xml/shift_open.bin");
}

// Repeat for shift_close, sell, return, z_report.

// KVT1/KVT2 input→struct goldens are deferred from W4's first round.
// The parser does NOT ship in W1 (`prro::crypto` is wrapper-only over
// `prro_crypto`'s sign/verify/envelope/cmp APIs — no KVT decoder).  A
// follow-up bd issue carries:
//   - "add prro::crypto::kvt::{parse_kvt1, parse_kvt2}" (lifts the
//     existing Python `transports/dps_fiscal_server.py` decoder into Rust)
//   - "extend W4 goldens with kvt1/kvt2 fixtures + struct-eq assertions"
// Both gate on W3 (DpsChannel) being live so the fixture corpus can be
// captured from a real `lastChk` round-trip.  W4 first round explicitly
// covers ONLY the canonical-XML zone.

#[test]
fn cms_deterministic_prefix_byte_equal() {
    // Build the XML-to-be-signed; assert it matches the frozen prefix.
    let prefix = prro::xml::build_canonical_xml(prro::xml::DocType::Sell, &test_fixtures::sell_payload());
    assert_golden_eq(&prefix, "tests/goldens/cms/sell_pre_sign.bin");
}

mod test_fixtures {
    // Inline test payloads sized by the captured cases.
}
```

- [ ] **Step 4: Build + test.**

```bash
cargo test -p prro --test goldens_byte_equiv
```

Expected: all goldens pass.  If any fail, investigate before committing — a failure means either Rust output drifted from Python or the captured fixture is stale.

- [ ] **Step 5: Document the operator procedure.**

`docs/M2-goldens-capture.md` covers: when to re-capture, who, the exact `regenerate.py` invocation, how to review the diff, when an intentional drift requires a follow-up bd-issue (because the Rust producer also has to update).

- [ ] **Step 6: Commit.**

```bash
git add rust/prro/src/xml/ rust/prro/tests/goldens_byte_equiv.rs \
        rust/prro/tests/goldens/ docs/M2-goldens-capture.md
git commit -m "feat(rust/goldens): byte-equivalence harness + first-round vectors (M2/W4)"
git push origin rust-gateway
```

---

## Task 5 (W5) — ADR-M2-6 static check (no DB handle in provider/channel APIs)

**Goal:** Land a Rust test that parses the public API of `prro::crypto::*` and `prro::transports::*` and fails the build if any `pub fn` / `pub async fn` signature contains `SqlitePool`, `SqliteConnection`, `Transaction`, or `Pool<Sqlite>`.  Excludes `#[cfg(test)]`.  Lib only, not tests/examples.

**Day budget:** 1-2 days.

**Implements:** ADR-M2-6 enforcement.  blockedBy: W1, W2, W3 (need real code to enforce on; W2 is exempt by design — `services::cert_refresher` legitimately takes `SqlitePool`).

**Files:**

- Modify: `rust/prro/Cargo.toml` (`syn` to `[dev-dependencies]`)
- Create: `rust/prro/tests/api_surface_no_db_handle.rs`

**Acceptance Criteria:**

- [ ] Test parses every `.rs` under `rust/prro/src/crypto/` and `rust/prro/src/transports/` via `syn`.
- [ ] Scanner walks **both** free `pub fn` / `pub async fn` items (`syn::ItemFn`) **and trait method definitions** (`syn::ItemTrait` → each `syn::TraitItemFn`).  Without trait-item scanning the main API surface (`CryptoProvider`, `DpsChannel`) would be missed entirely.
- [ ] For every public function or trait method scanned, asserts no parameter type or return type stringifies to a name containing `SqlitePool`, `SqliteConnection`, `Pool` (with sqlx generic), or `Transaction` (with sqlx generic).
- [ ] `services::cert_refresher` is NOT scanned (carved out per ADR-M2-6).
- [ ] `#[cfg(test)]` modules / blocks are skipped.
- [ ] A negative-fixture test confirms the scanner WOULD catch a violation if injected via BOTH paths: (1) synthesised AST with `pub async fn debug_violation(pool: SqlitePool)` (free fn), AND (2) synthesised `pub trait T { async fn m(&self, pool: &SqlitePool); }` (trait method) — both must produce an error with a clear `file:line` diagnostic.

**Verify:**

```bash
cargo test -p prro --test api_surface_no_db_handle
```

**Steps:**

- [ ] **Step 1: Add `syn` dev-dep.**

```toml
[dev-dependencies]
# `visit` enables the `syn::visit::Visit` trait the scanner uses to walk
# free fns + trait method defs + impl-block fns.  `extra-traits` gives
# the `Debug`/`Clone` impls some diagnostic helpers depend on.
syn = { version = "2", features = ["full", "extra-traits", "visit"] }
# `quote::ToTokens::to_token_stream().to_string()` stringifies parameter
# / return types for the substring check.
quote = "1"
```

- [ ] **Step 2: Write the test.**

`tests/api_surface_no_db_handle.rs` walks the directory tree, parses each file, visits items, applies the rule.  ~200-250 lines; implementer follows `syn::visit::Visit` pattern with these visit fns:

- `visit_item_fn(&mut self, i: &ItemFn)` — covers free `pub fn` / `pub async fn` items.
- `visit_item_trait(&mut self, i: &ItemTrait)` — descends into each `TraitItemFn` and inspects its signature.  This is the critical addition: the M2 main API surface (`CryptoProvider`, `DpsChannel`) is expressed as trait methods, NOT free functions, so a `pub fn`-only scanner would silently pass a violation in a trait method.
- `visit_item_impl(&mut self, i: &ItemImpl)` — inspect each `ImplItemFn` only when its `vis` is `Visibility::Public` (private impl-block helpers are intentionally exempt).
- skip any item whose `attrs` contains `#[cfg(test)]` or whose enclosing module / file has it at the top.

For each visited signature: stringify every input type and the return type via `quote::ToTokens::to_token_stream().to_string()`, then check the resulting string for substrings `SqlitePool`, `SqliteConnection`, `sqlx::Pool`, `sqlx::Transaction`, `sqlite::Pool`, `Pool<Sqlite>`, `Transaction<'_, Sqlite>`.  Use a small list of substrings, not regex — false positives in test fixtures are caught by the `services/` exemption.

- [ ] **Step 3: Build + test, then deliberately inject a violation to confirm catch.**

```bash
cargo test -p prro --test api_surface_no_db_handle
```

Expected: pass.  Then implementer temporarily adds `pub async fn debug_force_violation(pool: &sqlx::SqlitePool) {}` to `src/crypto/in_process.rs`, re-runs the test, observes failure with a clear file:line diagnostic, then reverts the injection.

- [ ] **Step 4: Commit.**

```bash
git add rust/prro/Cargo.toml rust/prro/Cargo.lock rust/prro/tests/api_surface_no_db_handle.rs
git commit -m "test(rust/api): static check forbidding sqlx handles in crypto/transports public API (M2/W5)"
git push origin rust-gateway
```

---

## Task 6 (W6) — secret-material flow tracing test

**Goal:** Land a test that installs a `tracing` subscriber, exercises every `CryptoError` variant + `unseal_jks` happy path + `cert_refresher::refresh_for_fn` happy path, and asserts NO captured event/log contains any substring of the seeded password / cred_salt / private-key bytes.  Implements ADR-M2-5 §4d.

**Day budget:** 1-2 days.

**Implements:** ADR-M2-5 §4d.  blockedBy: W1, W2, W3.

**Files:**

- Modify: `rust/prro/Cargo.toml` (`tracing-subscriber = { version = "0.3", features = ["std", "fmt"] }` to `[dev-dependencies]`)
- Create: `rust/prro/tests/secret_flow_tracing.rs`

**Acceptance Criteria:**

- [ ] Test installs a self-managed `tracing-subscriber::fmt` subscriber backed by an `Arc<Mutex<Vec<u8>>>` capture sink (no `tracing_test::internal::*` private-API dependency).
- [ ] Test seeds three known-secret values (a fixture password, a cred_salt, a private-key blob) into the test fixtures consumed by `unseal_jks`, `sign_cms_detached`, `cert_refresher::refresh_for_fn`.
- [ ] Exercises each `CryptoError` variant (`JksUnseal`, `CmsSign`, `EnvelopeDecrypt`, `CertFetch`, `VerifyFailed`) at least once.
- [ ] After all paths run, asserts the captured byte buffer (UTF-8 decoded) contains NO substring of any seeded secret (password, salt-hex, or first 16 bytes of the seeded private key).
- [ ] Includes a positive control: a deliberate `tracing::info!(jks = "leaked")` is emitted in a separate scoped block; the test confirms its capture machinery would have caught a leak.

**Verify:**

```bash
cargo test -p prro --test secret_flow_tracing
```

**Steps:**

- [ ] **Step 1: Add `tracing-subscriber` dev-dep.**

```toml
[dev-dependencies]
# Stable public API for capturing emitted events into a Vec<String>.
# Avoid `tracing-test`: its assertion helpers live under
# `tracing_test::internal::*` which is explicitly not part of the
# crate's stable surface (the path can change between minor versions
# and the test would silently break).  We own a tiny capture
# subscriber instead — ~30 lines, no third-party private API.
tracing-subscriber = { version = "0.3", default-features = false, features = ["std", "fmt"] }
```

- [ ] **Step 2: Write the test.**

```rust
// tests/secret_flow_tracing.rs (skeleton)

use std::sync::{Arc, Mutex};
use std::io::{self, Write};
use tracing::subscriber::set_default;
use tracing_subscriber::fmt;

const SEEDED_PASSWORD: &str = "p@ssw0rd-leak-canary-9f8a";
const SEEDED_SALT_HEX: &str = "0123456789abcdeffedcba9876543210";
const SEEDED_PRIVATE_KEY: &[u8] = b"\xde\xad\xbe\xef--seeded-private-key-canary--";

/// Thread-safe capture sink consumed by `tracing-subscriber`'s `fmt`
/// layer.  Every event is rendered to a UTF-8 line and pushed into the
/// shared buffer; assertions run against the joined buffer at test end.
#[derive(Clone, Default)]
struct CaptureBuf(Arc<Mutex<Vec<u8>>>);

impl Write for CaptureBuf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}

impl<'a> fmt::MakeWriter<'a> for CaptureBuf {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer { self.clone() }
}

impl CaptureBuf {
    fn into_string(self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).expect("utf-8 logs")
    }
}

// Why `set_default` (returning a `DefaultGuard`) and NOT `with_default`
// (which takes a sync closure):
//   `with_default(subscriber, || { … })` requires a synchronous body —
//   any `.await` inside would suspend the future and the subscriber
//   would be reset before the `.await` resumed, so events emitted on
//   the resumed continuation would land in the GLOBAL subscriber
//   (whatever cargo-test sets up by default), NOT in our capture buf.
//   `set_default` returns a guard that lives across `.await` points and
//   restores the previous subscriber when dropped — exactly what we
//   want for an async test.
//   See https://docs.rs/tracing/latest/tracing/subscriber/fn.set_default.html
//   for the documented contract.
//   Tests are NOT marked `#[tokio::test(flavor = "current_thread")]`
//   because tokio's default multi-threaded runtime requires `Send`
//   guards; `DefaultGuard` is `!Send`, so we explicitly request
//   `current_thread`.

#[tokio::test(flavor = "current_thread")]
async fn no_secret_substring_leaks_through_any_log() {
    let buf = CaptureBuf::default();
    let subscriber = fmt()
        .with_writer(buf.clone())
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .finish();
    let _guard = set_default(subscriber);

    // All `.await`s below run with the capture subscriber active because
    // `_guard` is held in the surrounding scope.
    // Step 1: exercise unseal_jks happy path with seeded password+key.
    // Step 2: exercise sign_cms_detached.
    // Step 3: exercise refresh_for_fn against a wiremock stub.
    // Step 4: exercise each CryptoError variant by triggering it.
    // (implementer wires; uses test_helpers from W1)

    drop(_guard); // explicit so the assertion runs against a frozen buffer

    let captured = buf.into_string();
    let priv_substr = std::str::from_utf8(&SEEDED_PRIVATE_KEY[..16])
        .unwrap_or("\xde\xad\xbe\xef--seeded-priva");
    assert!(!captured.contains(SEEDED_PASSWORD), "password leaked: {captured}");
    assert!(!captured.contains(SEEDED_SALT_HEX), "salt leaked: {captured}");
    assert!(!captured.contains(priv_substr), "priv-key prefix leaked: {captured}");
}

#[tokio::test(flavor = "current_thread")]
async fn positive_control_capture_works() {
    let buf = CaptureBuf::default();
    let subscriber = fmt()
        .with_writer(buf.clone())
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .finish();
    let _guard = set_default(subscriber);

    // Use a sync emit + a tiny await so the test exercises both the
    // pre-await and post-await capture paths.
    tracing::info!(jks = "leaked-on-purpose-for-test");
    tokio::task::yield_now().await;
    tracing::info!("post-await event");

    drop(_guard);

    let captured = buf.into_string();
    assert!(
        captured.contains("leaked-on-purpose-for-test"),
        "capture machinery broken: {captured}"
    );
    assert!(
        captured.contains("post-await event"),
        "guard did not survive .await — events lost: {captured}"
    );
}
```

- [ ] **Step 3: Build + test.**

```bash
cargo test -p prro --test secret_flow_tracing
```

Expected: 2 tests pass.

- [ ] **Step 4: Commit.**

```bash
git add rust/prro/Cargo.toml rust/prro/Cargo.lock rust/prro/tests/secret_flow_tracing.rs
git commit -m "test(rust/security): tracing-subscriber assertion on secret-substring leaks (M2/W6)"
git push origin rust-gateway
```

---

## Self-review

**1. Spec coverage:** every M2 ADR section maps to a task — ADR-M2-1 → W1; ADR-M2-2 → W3; ADR-M2-3 → W4; ADR-M2-4 → W2; ADR-M2-5 → W1 (discipline) + W6 (test); ADR-M2-6 → W1/W2/W3 (compliant by construction) + W5 (enforcement).  W0-1 contract subset → W3.  W0-2 CMP path → W2.  W0-3 trait shape → W1.  PRRO_GATE-5js → W3.

**2. Placeholder scan:** every "TODO"-shaped phrase is wrapped in a follow-up bd-issue or labelled as an implementer wiring step (not as a placeholder).  No "TBD" / "fill in later" / "implement later" left as steps.

**3. Type consistency:** `CryptoProvider` trait shape in W1 matches W0-3 §5; `DpsChannel` trait + `LastChkRequest`/`LastChkResponse` types are referenced consistently in W3 + W4; `RefreshOutcome` in W2 is the only consumer of `CryptoProvider::fetch_cert_by_ski`.

---

## What this plan does NOT do

- **M3 write-path stages.** The `PREPARED → SIGNED → ENCRYPTED → SENT → KVT1 → KVT2 → ACK` pipeline lives in M3.  The "staged pipeline" concept from ADR-M2-6 is referenced by W5 (the no-DB-handle rule that makes the staged pipeline expressible) but no M3 task lists go in this plan.
- **Ingress shells.**  REST / XML-RPC / Maria / Maria304 / Checkbox-compat are M4.
- **Admin UI / receipt rendering.**  M5.
- **Recovery loop / reconciliation.**  M3+.
- **Full canonical-XML builder.**  W4 ships only the minimal XML helpers needed for the five W4-scoped doc types.  M3 replaces with a full schema-driven builder.
- **`node_state` bootstrap reconciliation.**  Still gated by `PRRO_GATE-ah8`.
- **Workspace-wide clippy hardening (`prro_crypto`/`prro_sidecar` profile warnings).**  Tracked as `PRRO_GATE-u8z`.
- **Concurrent race test for ingress idempotency.**  Tracked as `PRRO_GATE-6r7`.
- **Typed wrappers in `IngressInboxRepo`.**  Tracked as `PRRO_GATE-1n9`.
- **`transition_state` Conflict/NotFound atomic disambiguation.**  Tracked as `PRRO_GATE-k99`.
- **`lnd` monotonicity source-of-truth.**  Tracked as `PRRO_GATE-ddn`.

After M2 lands, the next plan to write is the M3 write-path implementation plan, which will compose `prro::crypto`, `prro::transports::dps`, and `services::cert_refresher` from this plan into the staged pipeline and resolve the M3-side follow-ups above.

---

## Revision history

- **2026-05-04** initial plan (commit `617b274`).
- **2026-05-05** docs-fix pass after pre-implementation review found 10
  divergences between plan code blocks and reality (real `prro_crypto`
  API + actual `fiscal_server.proto` + M1 `operator_certs` PK shape):
  W1 `SigningSession` reshaped to `Arc<Inner>` carrying
  `Zeroizing<[u8; 32]>` matching `ExtractedKey.param_d`; W1
  `sign_cms_detached` rewritten against the real
  `sign_detached_with_content_digest(profile, cert_der, content_digest, &dyn RawSigner)`
  signature; W1 `spawn_blocking` now moves an `Arc<SigningSession>`
  clone (no plaintext copy); W2 split into `RefreshedInPlace` (same-SKI
  UPDATE) vs `RefreshedKeyRoll` (stage + flip) to avoid PK conflict on
  `operator_certs.ski_hex`; W2 multi-URL routing reads from a new
  `006_ca_endpoints.sql` migration vendoring legacy
  `sql/016_ca_endpoints.sql` (with `/services/cmp/` suffix in seeded
  URLs); W2 `parse_iso8601` and `compute_ski_hex` propagate typed
  `RefreshError` instead of `Utc::now()` fail-open / `expect()` panic;
  W3 proto package `com.programika.rro.ws.chk` and service
  `ChkIncomeService` (5 RPCs: `sendChkV2` / `lastChk` / `ping` /
  `statusRro` / `infoRro`); W3 `CheckResponse` shape is `id` /
  `status` / `id_sign` / `data_sign` / `error_message` (no
  `fns_data`); W5 scanner extended to walk `ItemTrait` /
  `TraitItemFn` so the trait surface (the actual M2 public API) is
  not missed.
