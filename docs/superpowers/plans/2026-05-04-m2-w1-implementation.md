# M2 W1+ Implementation Plan — Crypto Wrapper, Cert Refresher, DPS Channel, Goldens, Architectural Gates

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers-extended-cc:subagent-driven-development (recommended) or superpowers-extended-cc:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the M2 implementation layer that the M1 Foundation crate has been stubbed for: an in-process `prro::crypto` wrapper over `prro_crypto`, an async `services::cert_refresher` that talks to the IIT-proprietary cert-lookup channel, a tonic-built `prro::transports::dps` gRPC client + native Rust mock, a byte-equivalence goldens harness with frozen fixtures, and two architectural gates (no DB handle in provider/channel APIs; no secret material in tracing).

**Architecture:** Six tasks (W1-W6), three implementation modules + two gates + one goldens harness, all per the M2 pre-plan ADR (`docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md`, approved 2026-05-04) and the three W0 findings docs (W0-1 DPS wire = gRPC; W0-2 CMP = IIT lookup-by-SKI, `prro_crypto::cms::cmp::fetch_cert_by_ski` ready; W0-3 prro_crypto API audit + `CryptoProvider` trait shape).  No M3 write-path work; M2 builds the substrate that M3 stages compose over.

**Tech Stack:** Rust 1.83+, sqlx 0.8 SQLite (M1), `prro_crypto` (workspace), new deps: `async-trait` 0.1, `zeroize` 1.7+, `tonic` 0.12, `tonic-build` 0.12, `prost` 0.13, `wiremock` 0.6 (HTTP byte-replay for the test CA), `httpmock` or hand-rolled axum test server alternative. SQLX_OFFLINE workflow inherited from M1; `cargo sqlx prepare` from `rust/prro/` with absolute `DATABASE_URL` whenever new `sqlx::query!` macros land.

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
      kvt1/                                    # NEW — KVT1 parser inputs/outputs
      kvt2/                                    # NEW — KVT2 parser inputs/outputs
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
    #[error("signature verification failed")]
    VerifyFailed,
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
            Self::VerifyFailed => f.write_str("VerifyFailed"),
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

/// Opaque handle returned by `unseal_jks`.  Holds the unsealed private key
/// in `Zeroizing<...>`; dropped at end of crypto operation.  Manual `Debug`
/// prints `<redacted>` for the key bytes.
pub struct SigningSession {
    operator_id: String,
    /// `Zeroizing<Vec<u8>>` — DSTU 4145 private scalar bytes.
    /// Never logged; never `Debug`-printed; zeroed on drop.
    private_key: Zeroizing<Vec<u8>>,
    /// Public cert DER (non-secret).
    cert_der: Vec<u8>,
}

impl fmt::Debug for SigningSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SigningSession")
            .field("operator_id", &self.operator_id)
            .field("private_key", &"<redacted>")
            .field("cert_der_len", &self.cert_der.len())
            .finish()
    }
}

impl SigningSession {
    pub fn operator_id(&self) -> &str {
        &self.operator_id
    }

    pub fn cert_der(&self) -> &[u8] {
        &self.cert_der
    }

    /// Crate-internal accessor; the trait `Encode` for signing reaches in
    /// here.  External callers MUST NOT see plaintext key bytes.
    pub(crate) fn private_key_bytes(&self) -> &[u8] {
        &self.private_key
    }

    /// Test-only constructor.
    #[cfg(any(test, feature = "test_helpers"))]
    pub fn new_for_test(operator_id: String, private_key: Vec<u8>, cert_der: Vec<u8>) -> Self {
        Self {
            operator_id,
            private_key: Zeroizing::new(private_key),
            cert_der,
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

    // `extracted.private_key_bytes` and `extracted.cert_der` shapes come from
    // `prro_crypto`; field names will likely match.  If not, this is the
    // additive helper W0-3 §3 flagged.
    Ok(SigningSession {
        operator_id: sealed.operator_id.to_string(),
        private_key: Zeroizing::new(extracted.private_key_bytes().to_vec()),
        cert_der: extracted.cert_der().to_vec(),
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
    pub include_tsp: bool,
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
    async fn verify_dstu(
        &self,
        msg: &[u8],
        sig_bytes: &[u8],
        pubkey_compressed: &[u8],
    ) -> Result<DstuVerifyResult, CryptoError>;

    /// Decrypt a CMS envelope (KVT2 / DPS-encrypted response).  Takes a
    /// `SigningSession` that already holds the private key.
    async fn unwrap_envelope(
        &self,
        envelope_der: &[u8],
        session: &SigningSession,
    ) -> Result<Vec<u8>, CryptoError>;

    /// Fetch a cert by SKI from the IIT-proprietary CMP-look-alike channel
    /// (W0-2 finding).  URLs come from the caller (a service-layer module
    /// that loaded them from `cert_provisioning_config` / `ca_endpoints`).
    async fn fetch_cert_by_ski(
        &self,
        urls: &[String],
        ski: &[u8; 32],
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
        // Copy out borrows so the async closure can move them — the trait
        // boundary requires lifetimes match `'_`, but `spawn_blocking` needs
        // 'static.
        let canonical = request.canonical_xml.to_vec();
        let include_tsp = request.include_tsp;
        let private_key = request.session.private_key_bytes().to_vec();
        let cert_der = request.session.cert_der().to_vec();

        let bytes = tokio::task::spawn_blocking(move || {
            sign_cms_blocking(&canonical, &private_key, &cert_der, include_tsp)
        })
        .await
        .map_err(|_| CryptoError::CmsSign { reason: SignKind::BackendError })??;

        Ok(SignedCmsBytes(bytes))
    }

    async fn verify_dstu(
        &self,
        msg: &[u8],
        sig_bytes: &[u8],
        pubkey_compressed: &[u8],
    ) -> Result<DstuVerifyResult, CryptoError> {
        // Verify is fast enough that staying on the executor is OK.
        let ok = prro_crypto_verify_blocking(msg, sig_bytes, pubkey_compressed);
        Ok(DstuVerifyResult(ok))
    }

    async fn unwrap_envelope(
        &self,
        envelope_der: &[u8],
        session: &SigningSession,
    ) -> Result<Vec<u8>, CryptoError> {
        let env = envelope_der.to_vec();
        let key = session.private_key_bytes().to_vec();
        let plaintext = tokio::task::spawn_blocking(move || unwrap_envelope_blocking(&env, &key))
            .await
            .map_err(|_| CryptoError::EnvelopeDecrypt { reason: DecryptKind::ParseFailed })??;
        Ok(plaintext)
    }

    async fn fetch_cert_by_ski(
        &self,
        urls: &[String],
        ski: &[u8; 32],
    ) -> Result<CertDer, CryptoError> {
        if urls.is_empty() {
            return Err(CryptoError::CertFetch { reason: FetchKind::AllUrlsFailed });
        }
        let urls_owned: Vec<String> = urls.to_vec();
        let ski_owned = *ski;
        let bytes = tokio::task::spawn_blocking(move || {
            fetch_cert_blocking(&urls_owned, &ski_owned)
        })
        .await
        .map_err(|_| CryptoError::CertFetch { reason: FetchKind::TransportError })??;
        Ok(CertDer(bytes))
    }
}

fn sign_cms_blocking(
    canonical_xml: &[u8],
    private_key: &[u8],
    cert_der: &[u8],
    include_tsp: bool,
) -> Result<Vec<u8>, CryptoError> {
    use prro_crypto::cms::builder::sign_detached_with_content_digest;
    sign_detached_with_content_digest(canonical_xml, private_key, cert_der, include_tsp)
        .map_err(|_| CryptoError::CmsSign { reason: SignKind::BackendError })
}

fn prro_crypto_verify_blocking(_msg: &[u8], _sig: &[u8], _pubkey: &[u8]) -> bool {
    // Wire to `prro_crypto::core::sign::verify` — exact signature TBD by
    // the implementer at integration time; signature derived from W0-3 §1.
    false
}

fn unwrap_envelope_blocking(envelope_der: &[u8], private_key: &[u8]) -> Result<Vec<u8>, CryptoError> {
    use prro_crypto::cms::envelope::{parse_envelope_params, unwrap_envelope};
    let params = parse_envelope_params(envelope_der)
        .map_err(|_| CryptoError::EnvelopeDecrypt { reason: DecryptKind::ParseFailed })?;
    unwrap_envelope(envelope_der, &params, private_key)
        .map_err(|_| CryptoError::EnvelopeDecrypt { reason: DecryptKind::MacFailed })
}

fn fetch_cert_blocking(urls: &[String], ski: &[u8; 32]) -> Result<Vec<u8>, CryptoError> {
    use prro_crypto::cms::cmp::fetch_cert_by_ski;
    for url in urls {
        match fetch_cert_by_ski(url, ski) {
            Ok(cert_der) => return Ok(cert_der),
            Err(_) => continue,
        }
    }
    Err(CryptoError::CertFetch { reason: FetchKind::AllUrlsFailed })
}
```

> Note: `sign_cms_blocking`, `prro_crypto_verify_blocking`, `unwrap_envelope_blocking`, `fetch_cert_blocking` reference `prro_crypto` symbols whose exact arguments W0-3 §1 names but whose precise types the implementer must verify when actually wiring.  If a `prro_crypto` extension is required (per W0-3 §3), file it as a separate additive PR against the `prro_crypto` crate and continue with the wrapper after — do NOT edit `rust/prro_crypto/src/**` from inside this task.

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

pub use errors::{CryptoError, DecryptKind, FetchKind, SealKind, SignKind};
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
    let session = SigningSession::new_for_test(
        "operator-1".into(),
        b"super-secret-private-key-bytes".to_vec(),
        b"<cert-der>".to_vec(),
    );
    let s = format!("{:?}", session);
    assert!(s.contains("operator-1"));
    assert!(s.contains("<redacted>"));
    assert!(!s.contains("super-secret-private-key-bytes"));
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
    let err = provider.fetch_cert_by_ski(&[], &ski).await.expect_err("empty urls");
    assert!(matches!(err, CryptoError::CertFetch { .. }));
}
```

- [ ] **Step 9: Build + test.**

```bash
cargo build -p prro
cargo test -p prro --test crypto_provider_smoke
```

Expected: build clean, 4 tests pass.

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
  cargo test -p prro --test crypto_provider_smoke → 4 passed
```

---

## Task 2 (W2) — `services::cert_refresher`

**Goal:** Land the async cert refresh service.  Reads multiple URLs from `cert_provisioning_config` / `ca_endpoints`; calls `prro::crypto::CryptoProvider::fetch_cert_by_ski`; on success, stages the cert at `active=0` and atomically flips `active=1` inside a `with_immediate` tx.  Honours `refresh_within_days`.

**Day budget:** 4-6 days.  Test-CA fixture work dominates.

**Implements:** ADR-M2-4, ADR-M2-6 (services-layer carve-out).  Uses W0-2 test-CA strategy.  blockedBy: W1.

**Files:**

- Modify: `rust/prro/Cargo.toml` (add `wiremock` to `[dev-dependencies]`)
- Modify: `rust/prro/src/lib.rs` (`pub mod services;`)
- Create: `rust/prro/src/services/mod.rs`
- Create: `rust/prro/src/services/cert_refresher.rs`
- Create: `rust/prro/tests/cert_refresher_smoke.rs`
- Create: `rust/prro/tests/fixtures/test_ca/` (vendored byte-replay corpus)

**Acceptance Criteria:**

- [ ] `cert_refresher::refresh_for_fn(pool, fn_id, provider)` returns `Ok(RefreshOutcome)` with one of `{NoChange, Refreshed { ski_old, ski_new }, Failed(reason)}`.
- [ ] Multi-URL fallback: if URL #1 returns transport error or a SKI mismatch, URL #2 is tried; if all fail, `Failed(AllUrlsFailed)` is returned without touching the DB.
- [ ] Atomic flip: stage at `active=0`, then `with_immediate(pool, |conn| ...)` runs `UPDATE … SET active=0 WHERE active=1` + `UPDATE … SET active=1 WHERE ski=?` in one tx.  Concurrent reader after the tx sees exactly one `active=1` row for the FN.
- [ ] `refresh_within_days` honoured: a cert whose `valid_to - now > refresh_within_days` is NOT refreshed (returns `NoChange`).  One whose `valid_to - now <= refresh_within_days` IS refreshed.
- [ ] No CMP fetch / network call happens inside any `with_immediate` block (W5 will static-assert this; W2 must not introduce a violation).
- [ ] Smoke tests pass against a `wiremock` HTTP byte-replay server using the vendored fixture corpus.

**Verify:**

```bash
cargo test -p prro --test cert_refresher_smoke
```

**Steps:**

- [ ] **Step 1: Add `wiremock` dev-dep.**

In `rust/prro/Cargo.toml` `[dev-dependencies]`:

```toml
wiremock = "0.6"
```

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
//!   4. Call `provider.fetch_cert_by_ski(urls, ski)` — outside any tx.
//!   5. Stage the new cert at active=0 in `operator_certs` (single INSERT).
//!   6. Atomically flip via `db::tx::with_immediate`:
//!        UPDATE operator_certs SET active=0 WHERE fiscal_number=? AND active=1;
//!        UPDATE operator_certs SET active=1 WHERE ski_hex=?;
//!   7. Append an `audit_log` row in the same tx.
//!   8. Return Refreshed{old, new}.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use sqlx::SqlitePool;

use crate::crypto::{CertDer, CryptoError, CryptoProvider};
use crate::db::tx::with_immediate;

#[derive(Debug, Clone)]
pub struct RefreshConfig {
    pub refresh_within_days: i64,
}

#[derive(Debug, Clone)]
pub enum RefreshOutcome {
    NoChange,
    Refreshed { ski_old: String, ski_new: String },
    Failed(RefreshError),
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum RefreshError {
    #[error("no active cert for FN {fn_id}")]
    NoActiveCert { fn_id: String },
    #[error("CMP fetch failed across all URLs")]
    AllUrlsFailed,
    #[error("CMP fetch returned a cert whose SKI differs from request")]
    SkiMismatch,
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
    let active = load_active_cert(pool, fn_id).await.map_err(|e| RefreshError::Db(e.to_string()))?
        .ok_or_else(|| RefreshError::NoActiveCert { fn_id: fn_id.to_string() })?;

    let now = Utc::now();
    if active.valid_to - now > Duration::days(cfg.refresh_within_days) {
        return Ok(RefreshOutcome::NoChange);
    }

    let urls = load_ca_urls(pool, fn_id).await.map_err(|e| RefreshError::Db(e.to_string()))?;
    let ski_bytes = hex_to_ski(&active.ski_hex);

    let new_cert: CertDer = provider
        .fetch_cert_by_ski(&urls, &ski_bytes)
        .await
        .map_err(map_crypto_to_refresh)?;

    let new_ski_hex = compute_ski_hex(&new_cert.0);

    // Stage outside any tx.
    stage_inactive_cert(pool, fn_id, &new_ski_hex, &new_cert.0)
        .await
        .map_err(|e| RefreshError::Db(e.to_string()))?;

    // Atomic flip + audit_log inside a single with_immediate.
    let active_ski = active.ski_hex.clone();
    let new_ski_for_tx = new_ski_hex.clone();
    let fn_id_owned = fn_id.to_string();
    with_immediate(pool, move |conn| {
        Box::pin(async move {
            sqlx::query("UPDATE operator_certs SET active = 0 WHERE fiscal_number = ? AND active = 1")
                .bind(&fn_id_owned)
                .execute(&mut *conn)
                .await?;
            sqlx::query("UPDATE operator_certs SET active = 1 WHERE ski_hex = ?")
                .bind(&new_ski_for_tx)
                .execute(&mut *conn)
                .await?;
            sqlx::query(
                "INSERT INTO audit_log(entity_type, entity_id, event_type, severity, actor, event_payload_json) \
                 VALUES ('fn', ?, 'cert_refresh', 'INFO', 'cert_refresher', ?)",
            )
            .bind(&fn_id_owned)
            .bind(format!(r#"{{"ski_old":"{}","ski_new":"{}"}}"#, active_ski, new_ski_for_tx))
            .execute(&mut *conn)
            .await?;
            Ok(())
        })
    })
    .await
    .map_err(|e| RefreshError::Db(e.to_string()))?;

    Ok(RefreshOutcome::Refreshed {
        ski_old: active.ski_hex,
        ski_new: new_ski_hex,
    })
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
    let days: i64 = sqlx::query_scalar(
        "SELECT refresh_within_days FROM cert_provisioning_config WHERE id = 1",
    )
    .fetch_one(pool)
    .await?;
    Ok(RefreshConfig { refresh_within_days: days })
}

async fn load_active_cert(pool: &SqlitePool, fn_id: &str) -> sqlx::Result<Option<ActiveCertRow>> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT ski_hex, valid_to FROM operator_certs WHERE fiscal_number = ? AND active = 1",
    )
    .bind(fn_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(ski_hex, valid_to_str)| ActiveCertRow {
        ski_hex,
        valid_to: parse_iso8601(&valid_to_str),
    }))
}

async fn load_ca_urls(pool: &SqlitePool, _fn_id: &str) -> sqlx::Result<Vec<String>> {
    // For M2 W2 minimal: read primary + fallback from cert_provisioning_config.
    // FN-specific override via ca_endpoints is a follow-up.
    let row: (String, Option<String>) = sqlx::query_as(
        "SELECT primary_cmp_url, fallback_cmp_url FROM cert_provisioning_config WHERE id = 1",
    )
    .fetch_one(pool)
    .await?;
    let mut urls = vec![row.0];
    if let Some(fallback) = row.1 {
        urls.push(fallback);
    }
    Ok(urls)
}

async fn stage_inactive_cert(
    pool: &SqlitePool,
    fn_id: &str,
    ski_hex: &str,
    cert_der: &[u8],
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO operator_certs(ski_hex, fiscal_number, cert_fingerprint, cert_der, \
            fetched_at, source, active) \
         VALUES (?, ?, ?, ?, ?, 'cmp', 0)",
    )
    .bind(ski_hex)
    .bind(fn_id)
    .bind(compute_fingerprint(cert_der))
    .bind(cert_der)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

fn hex_to_ski(hex: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        let hi = hex_digit(hex.as_bytes()[i * 2]);
        let lo = hex_digit(hex.as_bytes()[i * 2 + 1]);
        *b = (hi << 4) | lo;
    }
    out
}

fn hex_digit(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0,
    }
}

fn compute_ski_hex(cert_der: &[u8]) -> String {
    use prro_crypto::cms::envelope::compute_ski;
    let pubkey = prro_crypto::cms::envelope::extract_cert_pubkey_bytes(cert_der)
        .expect("cert without parseable pubkey");
    let ski = compute_ski(&pubkey);
    ski.iter().map(|b| format!("{:02x}", b)).collect()
}

fn compute_fingerprint(cert_der: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(cert_der))
}

fn parse_iso8601(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
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
    // ... wire pool with primary_cmp_url=mock.uri() and fallback=mock.uri() ...
    // ... call refresh_for_fn, assert Refreshed { ski_old, ski_new } ...
}

#[tokio::test]
async fn refresh_returns_all_urls_failed_without_touching_db() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock)
        .await;
    // ... wire two URLs that both 500 ...
    // ... call refresh_for_fn, assert RefreshOutcome::Failed(AllUrlsFailed) ...
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
- [ ] Methods covered: `submit`, `last_chk` (named per the actual proto message name; alias for documentation if the wire name is shorter), `query_by_local_identity` (returns `QueryNotSupported` typed variant per W0-1 finding), `ping`, `status_rro`, `info_rro`.
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
//! Re-export of the tonic-generated module under a stable path so callers
//! never `use` the raw generated module name (which depends on the .proto
//! package directive).

#![allow(clippy::all)]
tonic::include_proto!("fiscal_server");
```

(If the proto's `package` directive is not `fiscal_server`, the implementer adjusts the literal in `include_proto!`.)

- [ ] **Step 7: Write `rust/prro/src/transports/dps/types.rs`.**

Typed structs that the trait emits/consumes — never raw protobuf types.  Implementer maps from `gen::*` into these and back.  See W0-1 §3 for the field list.  Skeleton:

```rust
//! Typed DPS request/response structs.  The trait surface uses these,
//! NEVER the raw tonic-generated types.

#[derive(Debug, Clone)]
pub struct SubmitDocumentRequest {
    pub envelope_der: Vec<u8>,
    pub deadline_ms: u64,
}

#[derive(Debug, Clone)]
pub struct SubmitDocumentResponse {
    pub fiscal_no: String,
    pub fiscal_date: String,
    pub kvt1_der: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct LastChkRequest {
    pub signed_fn: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct LastChkResponse {
    pub id: String,
    pub fns_data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct PingResponse {
    pub server_time: String,
}

#[derive(Debug, Clone)]
pub struct StatusRroResponse {
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct InfoRroResponse {
    pub data: Vec<u8>,
}
```

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
//! signature (ADR-M2-6).

use async_trait::async_trait;
use tonic::transport::Channel;

use crate::transports::dps::errors::DpsError;
use crate::transports::dps::gen;     // tonic-generated types
use crate::transports::dps::types::*;

#[async_trait]
pub trait DpsChannel: Send + Sync {
    async fn submit(&self, req: SubmitDocumentRequest) -> Result<SubmitDocumentResponse, DpsError>;
    async fn last_chk(&self, req: LastChkRequest) -> Result<LastChkResponse, DpsError>;
    async fn ping(&self) -> Result<PingResponse, DpsError>;
    async fn status_rro(&self) -> Result<StatusRroResponse, DpsError>;
    async fn info_rro(&self) -> Result<InfoRroResponse, DpsError>;

    /// Encoded as `last_chk(fn_sign) + response.id == expected_fiscal_id`.
    /// Per `PRRO_GATE-5js`.
    async fn query_by_server_fiscal_no(
        &self,
        signed_fn: Vec<u8>,
        expected_fiscal_id: &str,
    ) -> Result<LastChkResponse, DpsError> {
        let resp = self.last_chk(LastChkRequest { signed_fn }).await?;
        if resp.id == expected_fiscal_id {
            Ok(resp)
        } else {
            Err(DpsError::ServerFiscalIdMismatch {
                expected: expected_fiscal_id.to_string(),
                actual: resp.id,
            })
        }
    }

    /// Per W0-1: always returns `QueryNotSupported`.  Callers handle this
    /// at the service layer.
    async fn query_by_local_identity(&self) -> Result<(), DpsError> {
        Err(DpsError::QueryNotSupported)
    }
}

pub struct GrpcDpsChannel {
    inner: gen::fiscal_server_client::FiscalServerClient<Channel>,
}

impl GrpcDpsChannel {
    pub async fn connect(endpoint: &str) -> Result<Self, DpsError> {
        let channel = Channel::from_shared(endpoint.to_string())
            .map_err(|_| DpsError::Internal("bad endpoint".into()))?
            .connect()
            .await
            .map_err(|_| DpsError::Unavailable)?;
        Ok(Self { inner: gen::fiscal_server_client::FiscalServerClient::new(channel) })
    }
}

#[async_trait]
impl DpsChannel for GrpcDpsChannel {
    async fn submit(&self, req: SubmitDocumentRequest) -> Result<SubmitDocumentResponse, DpsError> {
        // map req → gen::SubmitRequest → call → gen::SubmitResponse → SubmitDocumentResponse
        // map tonic::Status into typed DpsError per the spec mapping.
        let _ = req;
        Err(DpsError::Internal("not yet implemented".into())) // implementer wires
    }
    // ... last_chk, ping, status_rro, info_rro likewise ...

    async fn last_chk(&self, _req: LastChkRequest) -> Result<LastChkResponse, DpsError> { Err(DpsError::Internal("nyi".into())) }
    async fn ping(&self) -> Result<PingResponse, DpsError> { Err(DpsError::Internal("nyi".into())) }
    async fn status_rro(&self) -> Result<StatusRroResponse, DpsError> { Err(DpsError::Internal("nyi".into())) }
    async fn info_rro(&self) -> Result<InfoRroResponse, DpsError> { Err(DpsError::Internal("nyi".into())) }
}
```

The implementer fills in each method's tonic call + typed conversion.  Generated client name is `fiscal_server_client::FiscalServerClient` if the proto service is named `FiscalServer`.

- [ ] **Step 10: Write `rust/prro/src/transports/dps/mod.rs`.**

```rust
pub mod channel;
pub mod errors;
pub mod gen;
pub mod types;

pub use channel::{DpsChannel, GrpcDpsChannel};
pub use errors::{DpsError, InvalidKind};
pub use types::{
    InfoRroResponse, LastChkRequest, LastChkResponse, PingResponse, StatusRroResponse,
    SubmitDocumentRequest, SubmitDocumentResponse,
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

**Goal:** Land the goldens harness + first round of frozen test vectors covering: KVT1/KVT2 parser inputs/outputs, deterministic-prefix CMS-signed XML, and canonical unsigned XML for SHIFT_OPEN/SHIFT_CLOSE/SELL/RETURN/Z_REPORT.  Plus a manual-only re-capture script.

**Day budget:** 4-6 days.  XML builder for the 5 doc types + capture-script wiring + fixture review eat the budget.

**Implements:** ADR-M2-3.  blockedBy: W1, W3.  (W3 is needed because some KVT1/KVT2 inputs are best captured from the same path the M2 transport will exercise.)

**Files:**

- Create: `rust/prro/src/xml/` — minimal canonical XML builder for the 5 doc types (no general schema; literally just enough to produce byte-identical output to Python on these 5 cases).
- Create: `rust/prro/tests/goldens_byte_equiv.rs` — the harness.
- Create: `rust/prro/tests/goldens/{xml,kvt1,kvt2,cms,prevhash}/*.bin` — frozen fixtures.
- Create: `rust/prro/tests/goldens/regenerate.py` — manual capture script.
- Create: `rust/prro/tests/goldens/README.md` — operator procedure.
- Create: `docs/M2-goldens-capture.md` — operator-facing procedure.

**Acceptance Criteria:**

- [ ] `tests/goldens_byte_equiv.rs` runs against the frozen `tests/goldens/**/*.bin` files: each test reads a `.bin`, runs the Rust producer (XML builder / KVT parser / CMS signer with deterministic test key), and asserts byte equality.
- [ ] First-round goldens committed: `xml/{shift_open,shift_close,sell,return,z_report}.bin`; `kvt1/case1.bin`; `kvt2/case1.bin`; `cms/deterministic_prefix.bin` (or similar); `prevhash/seed.bin`.
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

#[test]
fn kvt1_parser_output_struct_eq() {
    let raw = std::fs::read("tests/goldens/kvt1/case1.input.bin").unwrap();
    let parsed = prro::crypto::parse_kvt1(&raw).unwrap();
    let expected = test_fixtures::kvt1_case1_expected();
    assert_eq!(parsed, expected);
}

// kvt2 likewise.

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
- [ ] For every `pub fn` / `pub async fn`, asserts no parameter type or return type stringifies to a name containing `SqlitePool`, `SqliteConnection`, `Pool` (with sqlx generic), or `Transaction` (with sqlx generic).
- [ ] `services::cert_refresher` is NOT scanned (carved out per ADR-M2-6).
- [ ] `#[cfg(test)]` modules / blocks are skipped.
- [ ] A negative-fixture test confirms the scanner WOULD catch a violation if injected (synthesised AST input with `pool: SqlitePool` parameter — must produce an error).

**Verify:**

```bash
cargo test -p prro --test api_surface_no_db_handle
```

**Steps:**

- [ ] **Step 1: Add `syn` dev-dep.**

```toml
[dev-dependencies]
syn = { version = "2", features = ["full", "extra-traits"] }
```

- [ ] **Step 2: Write the test.**

`tests/api_surface_no_db_handle.rs` walks the directory tree, parses each file, visits items, applies the rule.  ~150-200 lines; implementer follows `syn::visit::Visit` pattern.

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

- Modify: `rust/prro/Cargo.toml` (`tracing-test` to `[dev-dependencies]`)
- Create: `rust/prro/tests/secret_flow_tracing.rs`

**Acceptance Criteria:**

- [ ] Test installs a `tracing-test::traced_test`-style subscriber capturing every emitted log.
- [ ] Test seeds three known-secret values (a fixture password, a cred_salt, a private-key blob) into the test fixtures consumed by `unseal_jks`, `sign_cms_detached`, `cert_refresher::refresh_for_fn`.
- [ ] Exercises each `CryptoError` variant (`JksUnseal`, `CmsSign`, `EnvelopeDecrypt`, `CertFetch`, `VerifyFailed`) at least once.
- [ ] After all paths run, asserts no captured event's `format!("{event:?}")` representation contains any substring of any seeded secret.
- [ ] Includes a positive control: a deliberate `tracing::info!(jks = "leaked")` is emitted in a separate scoped block; the test confirms its capture machinery would have caught a leak.

**Verify:**

```bash
cargo test -p prro --test secret_flow_tracing
```

**Steps:**

- [ ] **Step 1: Add `tracing-test` dev-dep.**

```toml
[dev-dependencies]
tracing-test = "0.2"
```

- [ ] **Step 2: Write the test.**

```rust
// tests/secret_flow_tracing.rs (skeleton)

use tracing_test::traced_test;

const SEEDED_PASSWORD: &str = "p@ssw0rd-leak-canary-9f8a";
const SEEDED_SALT_HEX: &str = "0123456789abcdeffedcba9876543210";
const SEEDED_PRIVATE_KEY: &[u8] = b"\xde\xad\xbe\xef--seeded-private-key-canary--";

#[traced_test]
#[tokio::test]
async fn no_secret_substring_leaks_through_any_log() {
    // Step 1: exercise unseal_jks happy path with seeded password+key.
    // Step 2: exercise sign_cms_detached.
    // Step 3: exercise refresh_for_fn against a wiremock stub.
    // Step 4: exercise each CryptoError variant by triggering it.
    // ... (implementer wires; uses test_helpers from W1) ...

    // Capture is automatic via #[traced_test]; assert via tracing_test::logs_contain.
    assert!(!tracing_test::internal::logs_with_scope_contain("", SEEDED_PASSWORD));
    assert!(!tracing_test::internal::logs_with_scope_contain("", SEEDED_SALT_HEX));
    let priv_substr = std::str::from_utf8(&SEEDED_PRIVATE_KEY[..16]).unwrap_or("\xde\xad\xbe\xef");
    assert!(!tracing_test::internal::logs_with_scope_contain("", priv_substr));
}

#[traced_test]
#[tokio::test]
async fn positive_control_capture_works() {
    tracing::info!(jks = "leaked-on-purpose-for-test");
    assert!(tracing_test::internal::logs_with_scope_contain("", "leaked-on-purpose-for-test"));
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
