# External Code Review Request — W4-Z3 (Multi-Protocol PRRO Gateway, Rust)

## Your role

Senior Rust engineer with deep CMS / PKCS#7 / ASN.1-DER and Ukrainian
fiscal-crypto (DSTU 4145-2002, GOST 34.311-95) experience, plus SQLite and
edge-reliability instincts. You are reviewing an unmerged local branch in a
Ukrainian PRRO (Programmable Registrar of Settlement Operations) gateway — a
local edge service that builds, signs, and submits fiscal receipts to the DPS
(State Tax Service). Correctness, auditability and legal compliance matter far
more than performance or ergonomics.

**IMPORTANT framing correction (the codebase moved):** signing is now **native,
in-process Rust** via the `prro_crypto` crate (DSTU 4145-2002 over GOST 34.311-95,
CMS/CAdES-BES). The old Node.js `jkurwa` sidecar (port 8091) is **dead/removed** —
do NOT assume a sidecar exists. The project is being rewritten Python→Rust; the
Python tree (`src/prro_gateway`) is an INCOMPLETE reference, not authoritative.

## What you are reviewing

Three commits on branch `feat/m4-w4-z3-dps-extended-smoke` (base = `rust-gateway`,
NOT `main`). There is no GitHub PR yet — the full unified diff is **embedded at
the bottom of this document** (section "UNIFIED DIFF"), plus one block of
load-bearing PRE-EXISTING code the diff depends on ("CONTEXT CODE").

```
9e38c9b  w4(z3/review-r1)    cfg-gate + cabinet-allowlist + attached doc fixes + framing assert
07d8499  w4(z3/attached-fix) InProcessProvider signs ATTACHED CMS (DPS requires eContent)
25475cb  w4(z3/piece-1)      live-dps feature + smoke harness + DPS connect probe
```

To reproduce locally instead of reading the embedded diff:
```bash
git clone https://github.com/Setter1981/PRRO_GATE && cd PRRO_GATE
git fetch origin rust-gateway
# (the branch is local/unpushed at review time; diff range is 1ee3ca3..HEAD)
git diff 1ee3ca3..HEAD
```

## Worklet scope (W4-Z3 = live-DPS extended-XML fiscal smoke)

W4-Z3 is a **live DPS smoke test** worklet whose purpose is to prove the native
Rust fiscal cycle (SHIFT_OPEN → extended SELL → Z_REPORT) is ACCEPTED by the real
DPS test server `cabinet.tax.gov.ua:9443`, not just by mocks + byte-goldens. It is
being built in 7 pieces with mid-review gates. **Only the foundation is in these
three commits:**

1. **piece-1** — a new `live-dps` cargo feature and `tests/live_dps_extended_smoke.rs`
   (triple-gated: `#![cfg(feature="live-dps")]` + `#[ignore]` + `PRRO_LIVE_DPS=1`
   env kill-switch, plus a test-host allowlist refusing any non-`cabinet` host).
   So far it contains only a connectivity probe (connect + `last_chk` with a dummy
   32-byte blob; only `DpsError::Transport` = FAIL).

2. **attached-fix** (the hot-zone change, the main thing to scrutinize) — flips the
   native gateway signer `InProcessProvider` from producing a **DETACHED** CMS to an
   **ATTACHED** CMS (the receipt XML embedded as `eContent`). Rationale: the DPS
   `sendChkV2` gRPC `Check` message carries the document ONLY inside the `check_sign`
   bytes field — there is no separate plaintext-XML field — so a detached signature
   would reach DPS without the receipt and be rejected `-1 ERROR_VEREFY`. This was
   confirmed against the decompiled, **proven-DPS-accepted** competitor client
   "WebCheck" (`CtxSignFile(.., external:false, appendCert:true)` = attached +
   embedded cert). The native+live combination had NEVER been exercised before
   (prior integration tests used a stub DPS channel; the only prior live cycle used
   the now-dead attached sidecar). Mechanism: a new thin public fn
   `sign_attached_with_content_digest` mirrors the existing detached fn but passes
   `Some(content)` to the (pre-existing) `assemble_signed_data_with_opts`, and the
   gateway caller switches to it. Claim made by the author: signed-attributes and
   the signature value are byte-identical to the detached form (signature covers
   signed-attrs only; `message-digest` = H(content) in both); ONLY `eContent` is added.

3. **review-r1** — fixes from a first internal review round: adds the missing
   `#![cfg(feature="live-dps")]` gate (it was absent → the feature gated nothing);
   replaces a prod-host blocklist (`prro.tax.gov.ua`, which missed the legacy prod
   `prro2.tax.gov.ua:443`) with a default-deny test-host allowlist; corrects stale
   "detached" doc comments; strengthens the attached proof test with an OCTET-STRING
   framing assertion.

**NOT in these commits (deferred to later pieces):** the real `lastChk` MAC-chain
seed, the extended `<P TX TX1><CA>` SELL build, the full pipeline drive to a DPS
Ack, SHIFT_OPEN/Z_REPORT bookends, the runbook. So the load-bearing claim "DPS
accepts ATTACHED" is **not yet exercised end-to-end by any test here** — that is
the explicit job of later pieces. Treat live acceptance as an open hypothesis.

## Frozen invariants you must check are preserved (from CLAUDE.md)

1. **No network or crypto calls inside long SQLite write transactions.**
2. One `fiscal_number` = one logical single-writer write-path.
3. Channel switch forbidden with an open shift.
4. Idempotency mandatory.
5. Offline must respect time and code limits.
6. Adapters build full canonical payloads.
7. All canonical envelopes carry `schema_version`.
8. Recovery/reconciliation preserve state-machine correctness.
9. Graceful shutdown over "finishing fast".
10. **Local signing may be bypassed only by explicit profile/config behavior, not
    by accidental code drift.**

Hot zones: `services/write_path/*`, `transports/*`, crypto, migrations/DDL, runtime
startup/shutdown, shift/offline/node state.

## What has already been verified (so focus on what we missed)

- `prro_crypto` lib builds; the full `-p prro` test suite (~801 tests) is GREEN with
  the detached→attached flip applied (no regression).
- An independent reviewer parsed a real-key ATTACHED signature with **OpenSSL 3.0.13**:
  `encapContentInfo = {eContentType = pkcs7-data (1.2.840.113549.1.7.1), eContent [0]
  EXPLICIT OCTET STRING(content)}`, content recovered verbatim, and the `message-digest`
  signed-attr == `gost34311(content)` byte-for-byte (no double-hash). The byte-goldens
  pin the deterministic *XML to be signed*, not the non-deterministic CMS bytes, so they
  are unaffected.
- The new proof test asserts the content is embedded AND wrapped in a primitive DER
  OCTET STRING (`04 <len>`), and that detached does NOT contain the content.

## What I want from you — independent, adversarial, terse

Assume the self-review + one internal round may have missed things or fixed them
insufficiently. Prioritise the **attached-CMS crypto change** (it now alters EVERY
signed fiscal document in production). Specifically:

### A. ATTACHED-CMS crypto correctness (highest priority)
- Is the produced SignedData structurally valid + DPS-acceptable? (`eContent [0]
  EXPLICIT OCTET STRING`, CertificateSet, signedAttrs set, SignerInfo.) See the
  CONTEXT CODE block for the pre-existing `assemble_signed_data_inner` the new fn calls.
- Is the "signed-attrs + signature byte-identical to detached, only eContent added"
  claim actually true, or is there a path where attaching changes what is signed?
- For CAdES-BES with eContent present, must `message-digest` be H(eContent) (yes) —
  confirm no off-by-one between "content hashed" and "content embedded" (the gateway
  computes the digest once and passes both content + digest; is that wiring correct?).
- **signingTime**: the gateway emits NO `signingTime` signed-attr and no RFC-3161 TSP
  token, while DPS docs describe `check_sign` as signed "з позначкою часу" (with a time
  mark) and WebCheck/EUSignCP includes it. Is a timestamp-less BES likely to be rejected?
  Is the `certificates` field's `[0] IMPLICIT`-wrapping-a-bare-cert (not `SET OF`) a risk?
- cp1251: the bytes embedded as eContent are the already-cp1251-encoded canonical XML
  (same `Vec<u8>` that is hashed) — confirm there is no UTF-8/cp1251 mismatch between
  the hashed bytes and the embedded bytes.

### B. The detached→attached flip as a production change
- The trait method is still named `sign_cms_detached` but now produces ATTACHED. Is the
  retained-name-with-doc-note acceptable, or a dangerous footgun in a crypto hot zone?
- Any other caller / state-machine path (offline drain, MAC-recovery re-sign, KVT2
  envelope decrypt) that assumed detached semantics and could now break?
- Frozen invariant #1 (no crypto in write-tx): the sign runs in `spawn_blocking` with an
  `assert_not_in_with_immediate` guard — still honoured?

### C. The live-smoke harness (test-only, but it can hit a real fiscal server)
- Triple-gate soundness: feature + `#[ignore]` + `PRRO_LIVE_DPS=1`. Any way the test
  runs unintentionally? Is the `cabinet.tax.gov.ua` allowlist bypassable / too strict?
- Could piece-1 (dummy-sign `last_chk`) accidentally mutate fiscal state? (It should be
  a pure read.)
- The proof test silently PASSES (green) when the JKS key is absent (CI has no key) —
  acceptable, or should a key-less synthetic-key path also assert the attached shape?

### D. Test gaps / validation debt
- Beyond the substring+framing proof, is a real RFC-5652 *parse* (eContentType == id-data,
  decoded eContent == content) or a signature-VERIFY round-trip warranted before this
  ships, given it is a production crypto flip? (DSTU sig-verify is non-standard; note that.)
- A CI step compiles nothing under `--features live-dps` now → bitrot risk for the harness.
  Worth a compile-only CI gate?

### E. Anything a paranoid fiscal-systems reviewer demands before merge that is absent.

## Output format

Numbered findings. Each: **Severity** (Critical/High/Medium/Low/Info) · **Category**
(A–E) · **file:line** (cite the embedded diff / context code; do NOT invent line
numbers) · **concern** (one paragraph) · **suggested fix** (1–2 lines). If a category
has nothing actionable, say so explicitly. End with exactly one of: "Recommend MERGE",
"Recommend MERGE WITH FOLLOW-UPS", or "Recommend BLOCK", + one-sentence justification.
Be terse — signal over thoroughness theatre. Do not invent repo facts; if you need a
file not shown, say which and why.

---

## CONTEXT CODE (pre-existing, NOT in the diff — the attached path's load-bearing assembler)

The new `sign_attached_with_content_digest` ends by calling
`assemble_signed_data_with_opts(.., Some(content))`, which delegates to the
pre-existing `assemble_signed_data_inner`. That function (unchanged by this work)
is what actually builds `encapContentInfo` with/without `eContent`. Its relevant
body is included below so you can judge the attached encoding without the repo:

```rust
fn assemble_signed_data_inner(
    profile: CmsProfile,
    cert_der: &[u8],
    signed_attrs_set_der: &[u8],
    signature_value: &[u8],
    content_for_encap: Option<&[u8]>,
    timestamp_token_der: Option<&[u8]>,
    revocation_values_der: Option<&[u8]>,
) -> Result<Vec<u8>, CmsError> {
    use crate::cms::der_writer as dw;
    use crate::cms::oids;
    use der::{Decode, Encode};

    // 1. Parse the certificate to extract issuer + serial number.
    let cert = x509_cert::Certificate::from_der(cert_der)
        .map_err(|e| CmsError::Der(format!("cert parse: {}", e)))?;
    let issuer_der = cert
        .tbs_certificate
        .issuer
        .to_der()
        .map_err(|e| CmsError::Der(format!("issuer encode: {}", e)))?;
    let serial_der = cert
        .tbs_certificate
        .serial_number
        .to_der()
        .map_err(|e| CmsError::Der(format!("serial encode: {}", e)))?;

    // 2. IssuerAndSerialNumber ::= SEQUENCE { issuer Name, serialNumber CertificateSerialNumber }
    let mut isn_inner = Vec::with_capacity(issuer_der.len() + serial_der.len());
    isn_inner.extend_from_slice(&issuer_der);
    isn_inner.extend_from_slice(&serial_der);
    let isn_der = dw::sequence(&isn_inner);

    // 3. AlgorithmIdentifiers
    let digest_alg_der =
        dw::algorithm_identifier(profile.digest_oid()).map_err(|e| CmsError::Der(e.to_string()))?;
    let signature_alg_der = dw::algorithm_identifier(profile.signature_oid())
        .map_err(|e| CmsError::Der(e.to_string()))?;

    // 4. signedAttrs as [0] IMPLICIT SET OF Attribute
    // Our `signed_attrs_set_der` already starts with 0x31 (SET tag).
    // For SignerInfo embedding, replace tag with 0xA0 (context [0] IMPLICIT
    // constructed); content bytes (after the tag+length) stay the same.
    let signed_attrs_implicit = retag_set_to_implicit_zero(signed_attrs_set_der)?;

    // 5. signature is raw OCTET STRING
    let signature_octet = dw::octet_string(signature_value);

    // 6. Build SignerInfo — plus optional unsignedAttrs carrying a
    //    signature-time-stamp-token (CAdES-T uplift).
    let mut si_inner = Vec::new();
    si_inner.extend_from_slice(&dw::integer_u32(1)); // version
    si_inner.extend_from_slice(&isn_der); // sid
    si_inner.extend_from_slice(&digest_alg_der); // digestAlgorithm
    si_inner.extend_from_slice(&signed_attrs_implicit); // signedAttrs
    si_inner.extend_from_slice(&signature_alg_der); // signatureAlgorithm
    si_inner.extend_from_slice(&signature_octet); // signature

    // unsignedAttrs [1] IMPLICIT SET OF Attribute — may carry TST
    // (CAdES-T) and/or revocation-values (CAdES-LT). If both are None,
    // we omit the whole field.
    let mut unsigned_attrs_concat: Vec<u8> = Vec::new();
    if let Some(tst) = timestamp_token_der {
        let tsa_oid_der = oids::ID_AA_SIGNATURE_TIME_STAMP_TOKEN
            .to_der()
            .map_err(|e| CmsError::Der(e.to_string()))?;
        let values_set = dw::set(tst);
        let mut attr_inner = Vec::with_capacity(tsa_oid_der.len() + values_set.len());
        attr_inner.extend_from_slice(&tsa_oid_der);
        attr_inner.extend_from_slice(&values_set);
        unsigned_attrs_concat.extend_from_slice(&dw::sequence(&attr_inner));
    }
    if let Some(rv) = revocation_values_der {
        let rv_oid_der = oids::ID_AA_ETS_REVOCATION_VALUES
            .to_der()
            .map_err(|e| CmsError::Der(e.to_string()))?;
        let values_set = dw::set(rv);
        let mut attr_inner = Vec::with_capacity(rv_oid_der.len() + values_set.len());
        attr_inner.extend_from_slice(&rv_oid_der);
        attr_inner.extend_from_slice(&values_set);
        unsigned_attrs_concat.extend_from_slice(&dw::sequence(&attr_inner));
    }
    if !unsigned_attrs_concat.is_empty() {
        si_inner.extend_from_slice(&dw::implicit_constructed_tag(1, &unsigned_attrs_concat));
    }
    let signer_info_der = dw::sequence(&si_inner);

    // 7. signerInfos SET OF SignerInfo (one element)
    let signer_infos_der = dw::set(&signer_info_der);

    // 8. digestAlgorithms SET OF AlgorithmIdentifier (one element)
    let digest_algs_der = dw::set(&digest_alg_der);

    // 9. encapContentInfo. Detached form omits `eContent`; attached
    //    form embeds the original content inside an OCTET STRING wrapped
    //    in `[0] EXPLICIT`. The content_for_encap argument decides:
    //    `None` → detached, `Some(bytes)` → attached.
    //
    //    EncapsulatedContentInfo ::= SEQUENCE {
    //        eContentType  OID,
    //        eContent     [0] EXPLICIT OCTET STRING OPTIONAL
    //    }
    let id_data_der = oids::ID_DATA
        .to_der()
        .map_err(|e| CmsError::Der(e.to_string()))?;
    let encap_content_info_der = match content_for_encap {
        None => dw::sequence(&id_data_der),
        Some(bytes) => {
            let octet = dw::octet_string(bytes);
            let explicit = dw::explicit_context_tag(0, &octet);
            let mut inner = Vec::with_capacity(id_data_der.len() + explicit.len());
            inner.extend_from_slice(&id_data_der);
            inner.extend_from_slice(&explicit);
            dw::sequence(&inner)
        }
    };

    // 10. certificates [0] IMPLICIT CertificateSet
    //    For one cert, this is the cert DER wrapped in [0] IMPLICIT.
    //    Per RFC 5652, certificate is a CHOICE — the default branch is
    //    Certificate (the X.509 cert SEQUENCE). We embed the cert_der
    //    directly inside [0] IMPLICIT.
    let certificates_der = dw::implicit_constructed_tag(0, cert_der);

    // 11. SignedData
    let mut sd_inner = Vec::new();
    sd_inner.extend_from_slice(&dw::integer_u32(1)); // version
    sd_inner.extend_from_slice(&digest_algs_der);
    sd_inner.extend_from_slice(&encap_content_info_der);
    sd_inner.extend_from_slice(&certificates_der);
    sd_inner.extend_from_slice(&signer_infos_der);
    let signed_data_der = dw::sequence(&sd_inner);

    // 12. ContentInfo
    let content_explicit = dw::explicit_context_tag(0, &signed_data_der);
    let signed_data_oid_der = oids::ID_SIGNED_DATA
        .to_der()
        .map_err(|e| CmsError::Der(e.to_string()))?;

    let mut ci_inner = Vec::with_capacity(signed_data_oid_der.len() + content_explicit.len());
    ci_inner.extend_from_slice(&signed_data_oid_der);
    ci_inner.extend_from_slice(&content_explicit);
    let content_info_der = dw::sequence(&ci_inner);

    Ok(content_info_der)
}
```

---

## UNIFIED DIFF (`git diff 1ee3ca3..HEAD`)

```diff
diff --git a/rust/prro/Cargo.toml b/rust/prro/Cargo.toml
index 041461e..0511a58 100644
--- a/rust/prro/Cargo.toml
+++ b/rust/prro/Cargo.toml
@@ -35,6 +35,16 @@ default = []
 # compile — by design.
 test-support = []
 
+# W4-Z3 live-DPS smoke gate.  Gates the `tests/live_dps_extended_smoke.rs`
+# target (whole file is `#![cfg(feature = "live-dps")]`), so it does NOT
+# compile — and cannot be run — unless explicitly built with
+# `--features live-dps`.  These tests hit the REAL DPS test server
+# (cabinet.tax.gov.ua:9443) with a REAL key + native prro_crypto signing,
+# so they are additionally `#[ignore]`'d AND guarded by a `PRRO_LIVE_DPS=1`
+# in-body env kill-switch (triple gate: feature + ignore + env).  NEVER run
+# in CI.  See the test file header for the env contract + rate-limit caveat.
+live-dps = []
+
 [dependencies]
 # Async runtime
 tokio = { version = "1", features = ["full"] }
diff --git a/rust/prro/src/crypto/in_process.rs b/rust/prro/src/crypto/in_process.rs
index fa94e6e..4af0263 100644
--- a/rust/prro/src/crypto/in_process.rs
+++ b/rust/prro/src/crypto/in_process.rs
@@ -114,15 +114,16 @@ fn sign_cms_blocking(
     session: &SigningSession,
     profile: prro_crypto::cms::profile::CmsProfile,
 ) -> Result<Vec<u8>, CryptoError> {
-    use prro_crypto::cms::builder::sign_detached_with_content_digest;
+    use prro_crypto::cms::builder::sign_attached_with_content_digest;
     use prro_crypto::cms::profile::CmsProfile;
     use prro_crypto::cms::signer::DstuInProcessSigner;
     use prro_crypto::core::curve::Curve;
     use prro_crypto::core::field::FieldEl;
     use prro_crypto::core::hash::{gost_34_311_95, kupyna_256};
 
-    // Build the content digest per profile.  `sign_detached_with_content_digest`
-    // owns signedAttrs hashing — we hand in the message digest.
+    // Build the content digest per profile.  `sign_attached_with_content_digest`
+    // owns signedAttrs hashing — we hand in the message digest AND the content
+    // bytes (the latter are embedded as `eContent`; see the ATTACHED note below).
     // `CmsProfile` is marked `#[non_exhaustive]` upstream; the wildcard
     // arm preserves forward-compat — a future profile we don't recognise
     // returns CurveMismatch (the closest existing reason; the wrapper
@@ -145,10 +146,25 @@ fn sign_cms_blocking(
     let d = FieldEl::from_le_bytes(&session.param_d()[..], curve.mod_words);
     let signer = DstuInProcessSigner::new(d);
 
-    sign_detached_with_content_digest(profile, session.cert_der(), &content_digest, &signer)
-        .map_err(|_| CryptoError::CmsSign {
-            reason: SignKind::BackendError,
-        })
+    // ATTACHED encapsulation: the canonical XML is embedded as the CMS
+    // `eContent`, because the DPS `sendChkV2` gRPC `Check.check_sign` field is
+    // the ONLY document carrier on the wire — a detached signature would reach
+    // DPS without the receipt and be rejected `-1 ERROR_VEREFY`.  Confirmed
+    // against the proven-accepted WebCheck client (`CtxSignFile(.., external:
+    // false, appendCert: true)`).  Signed-attributes + signature value are
+    // byte-identical to the detached form; only `eContent` is added.  (The
+    // `sign_cms_detached` trait-method name is retained to avoid cross-crate
+    // namespace churn; it now produces an ATTACHED CMS.)
+    sign_attached_with_content_digest(
+        profile,
+        session.cert_der(),
+        canonical_xml,
+        &content_digest,
+        &signer,
+    )
+    .map_err(|_| CryptoError::CmsSign {
+        reason: SignKind::BackendError,
+    })
 }
 
 fn verify_blocking(
diff --git a/rust/prro/src/crypto/provider.rs b/rust/prro/src/crypto/provider.rs
index 54c6416..935309c 100644
--- a/rust/prro/src/crypto/provider.rs
+++ b/rust/prro/src/crypto/provider.rs
@@ -12,7 +12,8 @@ use async_trait::async_trait;
 use crate::crypto::errors::CryptoError;
 use crate::crypto::session::SigningSession;
 
-/// Returned by `sign_cms_detached`.  Wraps the SignedData DER bytes.
+/// Returned by `sign_cms_detached`.  Wraps the ATTACHED CMS SignedData
+/// DER bytes (the `canonical_xml` is embedded as `eContent`).
 #[derive(Debug, Clone)]
 pub struct SignedCmsBytes(pub Vec<u8>);
 
@@ -46,7 +47,11 @@ pub struct SignCmsRequest<'a> {
 /// after pubkey-validation cache warm-up).
 #[async_trait]
 pub trait CryptoProvider: Send + Sync {
-    /// Build a CMS-detached signed envelope around `request.canonical_xml`.
+    /// Build a CMS signed envelope around `request.canonical_xml` using
+    /// ATTACHED encapsulation — the XML is embedded as `eContent`, the form
+    /// DPS `sendChkV2` requires (`check_sign` is the only document carrier on
+    /// the wire).  NOTE: the `_detached` suffix is retained for back-compat
+    /// only; the produced CMS is ATTACHED.  See `in_process.rs` for rationale.
     async fn sign_cms_detached(
         &self,
         request: SignCmsRequest<'_>,
diff --git a/rust/prro/src/services/write_path/stage_sign.rs b/rust/prro/src/services/write_path/stage_sign.rs
index bbbc20b..6411589 100644
--- a/rust/prro/src/services/write_path/stage_sign.rs
+++ b/rust/prro/src/services/write_path/stage_sign.rs
@@ -514,9 +514,10 @@ pub struct ReSignedArtifacts {
     /// writes this into `fiscal_documents.unsigned_xml_sha256` inside
     /// the MR-PERSIST `with_immediate` envelope.
     pub unsigned_xml_sha256: [u8; 32],
-    /// CMS detached signature produced by the configured provider over
-    /// `unsigned_xml`.  Caller writes it into `document_files.SIGNED_XML`
-    /// via `document_files::replace_tx`.
+    /// CMS signature produced by the configured provider over
+    /// `unsigned_xml` (ATTACHED encapsulation — the XML is embedded as
+    /// `eContent`, per DPS sendChkV2).  Caller writes it into
+    /// `document_files.SIGNED_XML` via `document_files::replace_tx`.
     pub signed_xml_cms: SignedCmsBytes,
 }
 
diff --git a/rust/prro/tests/live_dps_extended_smoke.rs b/rust/prro/tests/live_dps_extended_smoke.rs
new file mode 100644
index 0000000..ed8c066
--- /dev/null
+++ b/rust/prro/tests/live_dps_extended_smoke.rs
@@ -0,0 +1,175 @@
+#![cfg(feature = "live-dps")]
+//! **W4-Z3 — Live DPS extended-XML fiscal-cycle smoke (2026-05-28)**.
+//!
+//! These tests drive the REAL Rust write-path against the REAL DPS test
+//! server (`cabinet.tax.gov.ua:9443`) with a REAL signing key and the
+//! NATIVE `prro_crypto` in-process signer (NO jkurwa sidecar — that
+//! architecture is dead).  They exist to prove the native fiscal cycle
+//! (SHIFT_OPEN → extended SELL → Z_REPORT) is ACCEPTED by live DPS, not
+//! just by our mock + byte-goldens.
+//!
+//! ## Triple gate (this file never runs by accident)
+//!
+//! 1. **Cargo feature** — the whole file is `#![cfg(feature = "live-dps")]`,
+//!    so it does not even COMPILE without `--features live-dps`.
+//! 2. **`#[ignore]`** — every test is ignored; opt-in needs `-- --ignored`.
+//! 3. **`PRRO_LIVE_DPS=1` env kill-switch** — every test self-skips (prints
+//!    a SKIP line and returns OK) unless this is set, so a stray
+//!    `--ignored` run still cannot touch live DPS.
+//!
+//! ```bash
+//! PRRO_LIVE_DPS=1 \
+//! PRRO_LIVE_DPS_JKS_PATH="/abs/path/key_13667753_13667753 (2).jks" \
+//! PRRO_LIVE_DPS_JKS_PASS=... \
+//!   cargo test -p prro --features live-dps \
+//!     --test live_dps_extended_smoke -- --ignored --nocapture
+//! ```
+//!
+//! ## Env contract
+//!
+//! | var | required | default | meaning |
+//! |-----|----------|---------|---------|
+//! | `PRRO_LIVE_DPS`          | yes (gate) | —                               | must equal `1` or every test self-skips |
+//! | `PRRO_LIVE_DPS_HOST`     | no         | `https://cabinet.tax.gov.ua:9443` | DPS test endpoint (gRPC over TLS) |
+//! | `PRRO_LIVE_DPS_FN`       | no         | `4000162280`                    | test fiscal number (`rro_fn`) |
+//! | `PRRO_LIVE_DPS_JKS_PATH` | signing    | —                               | path to the JKS key container |
+//! | `PRRO_LIVE_DPS_JKS_PASS` | signing    | —                               | JKS password (NEVER logged) |
+//!
+//! Signing key for FN `4000162280` is the JKS `key_13667753_…(2).jks`
+//! (TN `13667753`, signer «ГАЛЬЧУН МИКОЛА ДМИТРОВИЧ»).  The key files are
+//! gitignored — the operator mounts them locally and points
+//! `PRRO_LIVE_DPS_JKS_PATH` at one.
+//!
+//! ## Caveats (per operator memory)
+//!
+//! 1. **DPS rate limit** (`project_dps_rate_limit`): the test server returns
+//!    `status=-4` after too many errors, with a 5+ minute per-FN cooldown.
+//!    Run sparsely and manually — NEVER in a loop and NEVER in CI.
+//! 2. **Production refusal**: tests refuse to run against the production
+//!    endpoint (`prro.tax.gov.ua`) — test host only.
+//! 3. **Native signing** (`prro_crypto::cms`, DSTU 4145-2002 + GOST 34.311,
+//!    CAdES-BES, ATTACHED) — no external sidecar.
+
+use prro::transports::dps::channel::DpsChannel;
+use prro::transports::dps::dto::CheckSignBlob;
+use prro::transports::dps::error::DpsError;
+use prro::transports::dps::grpc::GrpcDpsChannel;
+use std::time::Duration;
+
+// ─── Env contract ──────────────────────────────────────────────────────
+
+/// Hard env kill-switch: every test self-skips unless this equals `"1"`.
+const ENV_GATE: &str = "PRRO_LIVE_DPS";
+/// DPS endpoint override; default = test cabinet (gRPC over TLS).
+const ENV_HOST: &str = "PRRO_LIVE_DPS_HOST";
+/// Test fiscal number override.
+const ENV_FN: &str = "PRRO_LIVE_DPS_FN";
+
+const DEFAULT_HOST: &str = "https://cabinet.tax.gov.ua:9443";
+const DEFAULT_FN: &str = "4000162280";
+
+/// Allowlist marker: the live smoke is permitted ONLY against a DPS *test*
+/// cabinet host (the default and any dev cabinet both contain this).  Any
+/// other host — including EVERY production endpoint (`prro.tax.gov.ua`,
+/// the legacy `prro2.tax.gov.ua`, `fs.tax.gov.ua`) — is refused, so the
+/// smoke can never accidentally fiscalize against production.  Default-deny
+/// is deliberate: a prod-host blocklist would miss variants like `prro2`.
+const TEST_HOST_MARKER: &str = "cabinet.tax.gov.ua";
+
+/// Per-call deadline.  15s is generous (typical DPS response ~1-3s);
+/// covers slow TLS handshake + intermittent network during manual runs.
+const SMOKE_TIMEOUT_SECS: u64 = 15;
+
+fn resolve_host() -> String {
+    std::env::var(ENV_HOST).unwrap_or_else(|_| DEFAULT_HOST.to_string())
+}
+
+fn resolve_fn() -> String {
+    std::env::var(ENV_FN).unwrap_or_else(|_| DEFAULT_FN.to_string())
+}
+
+/// Returns `true` when the live env gate is armed (`PRRO_LIVE_DPS=1`).
+fn live_enabled() -> bool {
+    std::env::var(ENV_GATE).as_deref() == Ok("1")
+}
+
+/// Self-skip guard for every live test body.  Prints a SKIP line and
+/// returns `false` unless the env gate is armed; also enforces the
+/// production-endpoint refusal.  Usage: `if !live_armed("name") { return; }`.
+fn live_armed(test_name: &str) -> bool {
+    if !live_enabled() {
+        println!(
+            "=== {test_name} SKIP: set {ENV_GATE}=1 to run live DPS smoke \
+             (feature `live-dps` + `--ignored` + {ENV_GATE}=1 all required) ==="
+        );
+        return false;
+    }
+    let host = resolve_host();
+    if !host.contains(TEST_HOST_MARKER) {
+        panic!(
+            "{test_name} REFUSED: {ENV_HOST}={host} is not a DPS TEST cabinet \
+             (must contain `{TEST_HOST_MARKER}`).  The live smoke is test-server \
+             only (default {DEFAULT_HOST}); refusing to risk fiscalizing against \
+             a production endpoint (prro/prro2/fs.tax.gov.ua)."
+        );
+    }
+    true
+}
+
+// ─── Piece 1 — connectivity probe ───────────────────────────────────────
+
+/// **W4-Z3 Smoke 1 — DPS connect + wire round-trip (connectivity)**.
+///
+/// Cheapest reachability check, mirroring `live_smoke_w12_hardening` Smoke A:
+/// `GrpcDpsChannel::connect` (eager TLS handshake + HTTP/2 SETTINGS) then a
+/// `last_chk` with a dummy `CheckSignBlob`.  DPS rejects the dummy sign with
+/// a typed application-level error — which itself proves TLS + HTTP/2 + gRPC
+/// + response-parse all work.  Only `DpsError::Transport` is a FAIL (wire
+/// brokenness).  No real signing, no fiscal mutation, zero rate-limit cost
+/// beyond one read RPC.
+///
+/// PASS: connect ok AND `last_chk` returns `Ok(_)` or any non-`Transport`
+/// `Err`.  FAIL: connect error, or `DpsError::Transport`.
+#[tokio::test]
+#[ignore = "live DPS endpoint required; opt-in via --features live-dps + --ignored + PRRO_LIVE_DPS=1"]
+async fn live_smoke_1_connect_probe() {
+    if !live_armed("W4-Z3 Smoke 1") {
+        return;
+    }
+    let host = resolve_host();
+    let fiscal_number = resolve_fn();
+    println!("\n=== W4-Z3 Smoke 1: DPS connectivity ===");
+    println!("Endpoint: {host}");
+    println!("FN:       {fiscal_number}");
+    println!("Timeout:  {SMOKE_TIMEOUT_SECS}s\n");
+
+    let channel = GrpcDpsChannel::connect(&host, Duration::from_secs(SMOKE_TIMEOUT_SECS))
+        .await
+        .unwrap_or_else(|e| {
+            panic!(
+                "Smoke 1 FAIL: GrpcDpsChannel::connect — wire-level connectivity \
+                 broken (TLS / DNS / network / handshake timeout).  Endpoint: \
+                 {host}.  Error: {e:?}"
+            )
+        });
+
+    let dummy_sign = CheckSignBlob(vec![0u8; 32]);
+    let result = channel.last_chk(&dummy_sign).await;
+    println!("last_chk(dummy) response: {result:?}\n");
+
+    match result {
+        Err(DpsError::Transport(msg)) => {
+            panic!(
+                "Smoke 1 FAIL: wire-level Transport error on last_chk — connection \
+                 established but mid-call failure (server reset / deadline / proxy).  \
+                 Error: {msg}"
+            );
+        }
+        Ok(_) | Err(_) => {
+            println!(
+                "Smoke 1 PASS: wire path operational (TLS + HTTP/2 + gRPC + \
+                 response parse).  Ready for native-signed RPCs (pieces 2+)."
+            );
+        }
+    }
+}
diff --git a/rust/prro_crypto/src/cms/builder.rs b/rust/prro_crypto/src/cms/builder.rs
index b52afe6..2c2535a 100644
--- a/rust/prro_crypto/src/cms/builder.rs
+++ b/rust/prro_crypto/src/cms/builder.rs
@@ -313,6 +313,43 @@ pub fn sign_detached_with_content_digest(
     assemble_signed_data(profile, cert_der, &attrs_der, &signature_value)
 }
 
+/// Attached counterpart of [`sign_detached_with_content_digest`].
+///
+/// Produces a CMS `SignedData` with **byte-identical signed-attributes and
+/// signature value** to the detached form — the signature is computed over
+/// the signed-attributes, whose `message-digest` is `H(content)` in both
+/// cases — but EMBEDS `content` as `encapContentInfo.eContent` (attached /
+/// enveloping encapsulation).
+///
+/// This is the form the Ukrainian DPS `sendChkV2` gRPC endpoint requires:
+/// the `Check.check_sign` field is the ONLY document carrier on the wire,
+/// so the receipt XML must travel INSIDE the CMS.  Confirmed against the
+/// proven-accepted WebCheck client (`CtxSignFile(.., external: false,
+/// appendCert: true)` = attached + embedded cert).
+///
+/// `content` MUST be the exact bytes whose digest is `content_digest`
+/// (the caller computes the digest once and passes both, so the embedded
+/// eContent and the `message-digest` attribute cannot drift apart).
+pub fn sign_attached_with_content_digest(
+    profile: CmsProfile,
+    cert_der: &[u8],
+    content: &[u8],
+    content_digest: &[u8],
+    signer: &dyn RawSigner,
+) -> Result<Vec<u8>, CmsError> {
+    let attrs = build_signed_attrs(profile, content_digest, cert_der)?;
+    let attrs_der = attrs.to_der_set_of()?;
+    let signed_attrs_digest = compute_digest(profile, &attrs_der)?;
+    let signature_value = signer.sign_digest(&signed_attrs_digest)?;
+    assemble_signed_data_with_opts(
+        profile,
+        cert_der,
+        &attrs_der,
+        &signature_value,
+        Some(content),
+    )
+}
+
 // ─── Hash dispatch ──────────────────────────────────────────────────────────
 
 /// Compute the digest matching the profile's hash algorithm.
diff --git a/rust/prro_crypto/src/cms/mod.rs b/rust/prro_crypto/src/cms/mod.rs
index 90f27bf..fdfd324 100644
--- a/rust/prro_crypto/src/cms/mod.rs
+++ b/rust/prro_crypto/src/cms/mod.rs
@@ -48,7 +48,8 @@ pub mod signer;
 pub mod tsp;
 
 pub use builder::{
-    sign_detached_with_content_digest, CmsError, CmsSigner, DetachedSignature,
+    sign_attached_with_content_digest, sign_detached_with_content_digest, CmsError, CmsSigner,
+    DetachedSignature,
 };
 pub use profile::CmsProfile;
 pub use signer::{DstuInProcessSigner, RawSigner, SignerError};
diff --git a/rust/prro_crypto/tests/test_cms_end_to_end.rs b/rust/prro_crypto/tests/test_cms_end_to_end.rs
index a5da7a5..06488c1 100644
--- a/rust/prro_crypto/tests/test_cms_end_to_end.rs
+++ b/rust/prro_crypto/tests/test_cms_end_to_end.rs
@@ -143,3 +143,87 @@ fn test_cms_round_trip_via_x509_parser() {
         result.cms_der.len()
     );
 }
+
+fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
+    needle.len() <= haystack.len() && haystack.windows(needle.len()).any(|w| w == needle)
+}
+
+/// W4-Z3 attached-CMS proof: `sign_attached_with_content_digest` embeds the
+/// content as `eContent` (the form DPS `sendChkV2` requires — the gateway's
+/// `InProcessProvider` now drives this), whereas the detached form does NOT
+/// carry the content.  Signed-attributes + signature value are identical
+/// between the two (same `content_digest`); only `eContent` is added.
+#[test]
+fn test_attached_embeds_econtent_detached_does_not() {
+    let (cert_der, key_d_bytes) = match load_signing_material() {
+        Some(p) => p,
+        None => {
+            eprintln!("SKIP: production JKS not available");
+            return;
+        }
+    };
+    let curve = prro_crypto::Curve::dstu_pb_257();
+    let d = bytes_le_to_field(&key_d_bytes, curve.mod_words);
+    let signer = DstuInProcessSigner::new(d);
+
+    // Distinctive content so we can byte-search for it in the CMS DER.
+    let content = b"<RQ V=\"1\">PRRO-W4Z3-ATTACHED-ECONTENT-PROOF</RQ>";
+    let profile = CmsProfile::default(); // Dstu4145WithGost34311Pb
+    let content_digest = prro_crypto::core::hash::gost_34_311_95(content).to_vec();
+
+    let detached = prro_crypto::cms::sign_detached_with_content_digest(
+        profile,
+        &cert_der,
+        &content_digest,
+        &signer,
+    )
+    .expect("detached sign failed");
+    let attached = prro_crypto::cms::sign_attached_with_content_digest(
+        profile,
+        &cert_der,
+        content,
+        &content_digest,
+        &signer,
+    )
+    .expect("attached sign failed");
+
+    // Both are well-formed ContentInfo DER (outer SEQUENCE).
+    assert_eq!(detached[0], 0x30, "detached not SEQUENCE");
+    assert_eq!(attached[0], 0x30, "attached not SEQUENCE");
+
+    // ATTACHED embeds the content (eContent); DETACHED does not.
+    assert!(
+        contains_subslice(&attached, content),
+        "attached CMS must embed the content bytes as eContent"
+    );
+    // Structural strengthening (not just a coincidental byte match): the
+    // content must be wrapped in a primitive DER OCTET STRING — i.e. preceded
+    // by `04 <len>` (single-byte length for our <128-byte content) — which is
+    // exactly the eContent encoding `[0] EXPLICIT OCTET STRING(content)`.
+    assert!(content.len() < 0x80, "test content must be < 128 bytes for the framing check");
+    let mut octet_framed = vec![0x04u8, content.len() as u8];
+    octet_framed.extend_from_slice(content);
+    assert!(
+        contains_subslice(&attached, &octet_framed),
+        "attached eContent must be a primitive OCTET STRING (04 {:02X}) wrapping the content",
+        content.len()
+    );
+    assert!(
+        !contains_subslice(&detached, content),
+        "detached CMS must NOT carry the content bytes"
+    );
+    // Attached exceeds detached by at least the embedded content length.
+    assert!(
+        attached.len() >= detached.len() + content.len(),
+        "attached ({}) must exceed detached ({}) by >= content ({})",
+        attached.len(),
+        detached.len(),
+        content.len()
+    );
+    eprintln!(
+        "✓ attached={} bytes (eContent embedded) vs detached={} bytes; content={} bytes",
+        attached.len(),
+        detached.len(),
+        content.len()
+    );
+}

```
