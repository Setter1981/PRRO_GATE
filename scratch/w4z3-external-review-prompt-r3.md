# External Code Review Request — W4-Z3 **ROUND 3 (convergence)** — Multi-Protocol PRRO Gateway (Rust)

## Your role

Senior Rust engineer with deep CMS / PKCS#7 / ASN.1-DER and Ukrainian fiscal-crypto
(DSTU 4145-2002, GOST 34.311-95, CAdES) experience. You are re-reviewing an unmerged
local branch of a Ukrainian PRRO gateway that builds, signs, and submits fiscal
receipts to the DPS (State Tax Service). Correctness, auditability, legal compliance
matter more than performance.

**Framing (unchanged):** signing is NATIVE in-process Rust via `prro_crypto` (DSTU
4145-2002 / GOST 34.311-95, CMS/CAdES-BES). The old Node `jkurwa` sidecar is dead.
Python (`src/prro_gateway`) is an INCOMPLETE reference, not authoritative.

## This is ROUND 3 — verify convergence

You already reviewed the first three commits (round-2) and returned 7 findings + a
**"Recommend BLOCK"** (chiefly: the gateway omitted a `signingTime` signed-attr, and
the only attached-CMS proof skipped without a local key so CI exercised none of the
new path — too weak for a production-wide crypto flip before live DPS acceptance).

We integrated your review as a 4th commit, `23cccc3` (`review-r2`). The full diff
(now FOUR commits, range `1ee3ca3..HEAD`) is embedded at the bottom. Your job:
**confirm each round-2 finding was correctly + completely addressed, and flag
anything remaining or newly introduced.**

```
23cccc3  review-r2     ← integrates YOUR round-2 review (this is the new commit)
9e38c9b  review-r1     cfg-gate + cabinet-allowlist + attached doc fixes + framing assert
07d8499  attached-fix  InProcessProvider signs ATTACHED CMS
25475cb  piece-1       live-dps feature + smoke harness + connect probe
```

## How each round-2 finding was addressed (verify these)

| # (your sev) | finding | resolution in `23cccc3` |
|---|---|---|
| **#1 (High)** | gateway omits `signingTime`; DPS docs say check_sign is time-marked; WebCheck/EUSignCP includes it | **Resolved by FACT.** We parsed the official ЦЗО CAdES reference vectors `czo_test/*.p7s` with `openssl asn1parse`: EVERY CAdES-BES sample (DSTU detached, DSTU, Kupyna) carries `contentType + signingTime(UTCTIME) + messageDigest + signingCertificateV2`. WebCheck = EUSignCP `signLevel=1` with **TSP OFF** (`UseACSKTSPserver` default false) → same BES profile, no CAdES-T token. FIX: the gateway now signs via `CmsSigner::sign_with(canonical_xml, CmsBuildOptions{ attached:true, signing_time:Some(now) })` → ATTACHED CAdES-BES **with** signingTime. The custom no-time `sign_attached_with_content_digest` helper (which you saw in round-2) was **removed** as it produced the wrong profile. |
| **#2 (Med)** | "signature value byte-identical to detached" claim is false (DSTU random nonce) | Claim **dropped**. DSTU draws a fresh ephemeral per call, so signature values differ; comments now describe the ЦЗО profile and assert *structure*, not signature bytes. |
| **#3 (Med)** | only attached proof skips without JKS → CI exercises none of the attached path | **New NON-SKIPPING, key-free test** `rust/prro/tests/attached_cms_profile.rs` — uses a committed self-signed X.509 test cert + a stub `RawSigner` (no key) and asserts the produced CMS carries eContent (OCTET-STRING-framed content), `signingTime` OID, `messageDigest` == `GOST-34.311(content)`, and `contentType`. Runs in CI (`cargo test -p prro`). |
| **#4 (Med)** | trait method `sign_cms_detached` now produces ATTACHED — name footgun; rename | **Deferred** (tracked). Rename = 30 sites / 13 files incl. hot-zone `stage_finalize`/`stage_send` + 6 mock impls + the with_immediate scanner string; the footgun is *future-only* (exactly ONE impl exists, it is attached, and there is a doc note). Operator biases against namespace churn. We want your read: still a blocker, or safe to defer to a cleanup PR? |
| **#5 (Low)** | `host.contains("cabinet.tax.gov.ua")` is not host validation | Now parses the real hostname (`host_of`) and applies a **default-deny allowlist**: host must equal `cabinet.tax.gov.ua` or end with `.cabinet…`/`-cabinet…`. Rejects prod `prro`/`prro2`/`fs.tax.gov.ua` and lookalikes like `cabinet.tax.gov.ua.evil.com`. |
| **#6 (Low)** | nothing compiles `--features live-dps` → bitrot | Added a CI **compile-only** step (`cargo test -p prro --features live-dps --test live_dps_extended_smoke --no-run`). |
| **#7 (Info)** | core encoding fine | No change needed; you (round-2) + an OpenSSL parse confirmed `eContent [0] EXPLICIT OCTET STRING` is correct. |

## Current signing path (the thing to verify hardest)

Gateway `InProcessProvider::sign_cms_blocking` → `CmsSigner::sign_with(canonical_xml,
CmsBuildOptions{ attached:true, signing_time:Some(SystemTime::now()) })`. The
CONTEXT CODE below shows the (pre-existing) `CmsSigner::sign_with` pipeline and
`build_signed_attrs_with_time` (the signingTime construction + DER SET ordering).
`assemble_signed_data_inner` (the eContent / certificates assembler) is UNCHANGED
from round-2 and was OpenSSL-verified then, so it is not re-embedded here.

## Frozen invariants (must remain preserved)

1. No network/crypto inside SQLite write-tx (sign runs in `spawn_blocking` with an
   `assert_not_in_with_immediate` guard). 2. single-writer per fiscal_number.
3. channel-switch forbidden with open shift. 4. idempotency. 5. offline time/code
   limits. 8. recovery preserves state machine. 10. local signing bypass only by
   explicit config.

## What I want from you (terse, adversarial)

- **A.** Did #1 land correctly + completely? Is the emitted profile (ATTACHED
  eContent + contentType + signingTime + messageDigest + signingCertificateV2, **no**
  TSP token) the right ЦЗО CAdES-BES shape for DPS `sendChkV2`? Is `signingTime`'s
  UTCTIME encoding (`encode_signing_time_utc`, hard-fail outside 1950–2049) correct?
  Is the DER `SET OF` ordering (`to_der_set_of` sorts elements) right for the digest
  that gets signed?
- **B.** Is `attached_cms_profile.rs` a *sufficient* CI guard? Are its byte-search
  assertions (OID DER prefixes + length-framed OCTET STRINGs) robust, or is there a
  false-pass / false-fail risk (e.g. DER length-form edge, attribute ordering)?
- **C.** `host_of()` + allowlist — any parse bypass (userinfo, IPv6, trailing dot,
  uppercase) that defeats the prod-refusal?
- **D.** #4 rename deferral — do you accept it as a tracked follow-up, or still BLOCK?
- **E.** Anything in the round-2 diff that regressed, or that you would still block on
  *before merge* — keeping in mind live DPS acceptance of this profile is the explicit
  job of later W4-Z3 pieces (not these foundation commits)?

## Output format

Numbered findings: **Severity** (Critical/High/Medium/Low/Info) · **Category** (A–E) ·
**file:line** (cite the embedded diff / context; do NOT invent line numbers) ·
**concern** (one paragraph) · **fix** (1–2 lines). If a category is clean, say so.
End with exactly one of: "Recommend MERGE", "Recommend MERGE WITH FOLLOW-UPS",
"Recommend BLOCK" + one-sentence justification. Terse — signal over theatre.

---

## CONTEXT CODE (pre-existing, NOT in the diff — the gateway's signing pipeline)

### `CmsSigner::sign_with` (prro_crypto/src/cms/builder.rs) — the entry the gateway now calls

```rust
pub fn sign_with(
    &self,
    content: &[u8],
    opts: CmsBuildOptions,
) -> Result<DetachedSignature, CmsError> {
    let content_digest = compute_digest(self.profile, content)?;

    let attrs = crate::cms::attrs::build_signed_attrs_with_time(
        self.profile,
        &content_digest,
        self.cert_der,
        opts.signing_time,
    )?;
    let attrs_der = attrs.to_der_set_of()?;
    let signed_attrs_digest = compute_digest(self.profile, &attrs_der)?;
    let signature_value = self.signer.sign_digest(&signed_attrs_digest)?;

    let cms_der = assemble_signed_data_with_opts(
        self.profile,
        self.cert_der,
        &attrs_der,
        &signature_value,
        if opts.attached { Some(content) } else { None }, // <-- attached:true embeds eContent
    )?;

    Ok(DetachedSignature { cms_der })
}
```

### `build_signed_attrs_with_time` + DER SET ordering + signingTime UTCTIME (prro_crypto/src/cms/attrs.rs)

```rust
pub fn build_signed_attrs_with_time(
    profile: CmsProfile,
    content_digest: &[u8],
    cert_der: &[u8],
    signing_time: Option<std::time::SystemTime>,
) -> Result<SignedAttrsBlob, AttrsError> {
    if content_digest.len() != profile.digest_len() {
        return Err(AttrsError::DigestLen { got: content_digest.len(), want: profile.digest_len() });
    }
    // 1. content-type (id-data)
    let content_type = Attribute { oid: oids::ID_CONTENT_TYPE,
        value_der: profile.content_type_oid().to_der().map_err(|e| AttrsError::Der(e.to_string()))? };
    // 2. message-digest (the content hash)
    let md_value = OctetString::new(content_digest.to_vec()).map_err(|e| AttrsError::Der(e.to_string()))?;
    let message_digest = Attribute { oid: oids::ID_MESSAGE_DIGEST,
        value_der: md_value.to_der().map_err(|e| AttrsError::Der(e.to_string()))? };
    // 3. signing-certificate-v2 (GOST-34.311 cert hash + IssuerSerial)
    let cert_hash = compute_cert_hash(profile, cert_der);
    let scv2_der = build_signing_cert_v2(&cert_hash, profile.cert_hash_oid(), cert_der)?;
    let signing_cert_v2 = Attribute { oid: oids::ID_AA_SIGNING_CERTIFICATE_V2, value_der: scv2_der };
    // 4. signingTime (UTCTIME yyMMddHHmmssZ), optional
    let signing_time_attr = match signing_time {
        None => None,
        Some(t) => Some(Attribute { oid: oids::ID_SIGNING_TIME, value_der: encode_signing_time_utc(t)? }),
    };
    Ok(SignedAttrsBlob { content_type, message_digest, signing_cert_v2, signing_time: signing_time_attr })
}

impl SignedAttrsBlob {
    /// DER-encode as SET OF Attribute — this is what the signer signs.
    pub fn to_der_set_of(&self) -> Result<Vec<u8>, AttrsError> {
        let mut elements: Vec<Vec<u8>> = Vec::with_capacity(4);
        elements.push(self.content_type.to_der()?);
        elements.push(self.message_digest.to_der()?);
        elements.push(self.signing_cert_v2.to_der()?);
        if let Some(ref st) = self.signing_time { elements.push(st.to_der()?); }
        elements.sort();                 // DER SET OF: elements sorted by byte encoding
        let total_len: usize = elements.iter().map(|e| e.len()).sum();
        let mut out = Vec::with_capacity(total_len + 8);
        out.push(0x31);                  // SET tag
        encode_length(total_len, &mut out);
        for e in &elements { out.extend_from_slice(e); }
        Ok(out)
    }
}

fn encode_signing_time_utc(t: std::time::SystemTime) -> Result<Vec<u8>, AttrsError> {
    let secs = t.duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| AttrsError::Der("signing_time before UNIX epoch".into()))?.as_secs();
    let (year, month, day, hour, minute, second) = unix_secs_to_utc(secs);
    if !(1950..=2049).contains(&year) {
        return Err(AttrsError::Der(format!(
            "signingTime year {} outside UTCTIME range 1950..=2049 — switch to GeneralizedTime is required", year)));
    }
    let yy = year % 100;
    let s = format!("{:02}{:02}{:02}{:02}{:02}{:02}Z", yy, month, day, hour, minute, second);
    // ... wrapped as UTCTIME (tag 0x17) DER ...
}
```

---

## UNIFIED DIFF (`git diff 1ee3ca3..HEAD` — 4 commits)

```diff
diff --git a/.github/workflows/rust-prro.yml b/.github/workflows/rust-prro.yml
index a856b8b..fd55fdc 100644
--- a/.github/workflows/rust-prro.yml
+++ b/.github/workflows/rust-prro.yml
@@ -101,6 +101,15 @@ jobs:
         working-directory: rust
         run: cargo build -p prro --target ${{ matrix.target }} --locked
 
+      - name: Compile-only check — W4-Z3 live-dps smoke harness (never executed)
+        if: matrix.target != 'x86_64-unknown-linux-musl'
+        working-directory: rust
+        # The live-dps test target is gated `#![cfg(feature = "live-dps")]` and
+        # hits the REAL DPS server, so it is NEVER run in CI.  This step only
+        # COMPILES it (--no-run) so the harness cannot bit-rot (API drift in
+        # transports/dps, crypto, or env wiring) while normal suites stay green.
+        run: cargo test -p prro --target ${{ matrix.target }} --features live-dps --test live_dps_extended_smoke --no-run --locked
+
       - name: Test (cross-target binary)
         if: matrix.target != 'x86_64-unknown-linux-musl'
         working-directory: rust
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
index fa94e6e..506e7c4 100644
--- a/rust/prro/src/crypto/in_process.rs
+++ b/rust/prro/src/crypto/in_process.rs
@@ -114,28 +114,25 @@ fn sign_cms_blocking(
     session: &SigningSession,
     profile: prro_crypto::cms::profile::CmsProfile,
 ) -> Result<Vec<u8>, CryptoError> {
-    use prro_crypto::cms::builder::sign_detached_with_content_digest;
+    use prro_crypto::cms::builder::{CmsBuildOptions, CmsSigner};
     use prro_crypto::cms::profile::CmsProfile;
     use prro_crypto::cms::signer::DstuInProcessSigner;
     use prro_crypto::core::curve::Curve;
     use prro_crypto::core::field::FieldEl;
-    use prro_crypto::core::hash::{gost_34_311_95, kupyna_256};
-
-    // Build the content digest per profile.  `sign_detached_with_content_digest`
-    // owns signedAttrs hashing — we hand in the message digest.
-    // `CmsProfile` is marked `#[non_exhaustive]` upstream; the wildcard
-    // arm preserves forward-compat — a future profile we don't recognise
-    // returns CurveMismatch (the closest existing reason; the wrapper
-    // is locked to PB-257 + GOST-34.311 / Kupyna-256 in M2).
-    let content_digest: Vec<u8> = match profile {
-        CmsProfile::Dstu4145WithGost34311Pb => gost_34_311_95(canonical_xml).to_vec(),
-        CmsProfile::Dstu4145WithDstu7564Pb => kupyna_256(canonical_xml).to_vec(),
+
+    // `CmsProfile` is `#[non_exhaustive]` upstream, so the wildcard arm is
+    // mandatory here; we reject any unknown profile with a typed
+    // CurveMismatch (the wrapper is locked to PB-257 + GOST-34.311 /
+    // Kupyna-256 in M2).  `CmsSigner::sign_with` hashes the content with the
+    // matched profile's digest internally, so we do not pre-compute it.
+    match profile {
+        CmsProfile::Dstu4145WithGost34311Pb | CmsProfile::Dstu4145WithDstu7564Pb => {}
         _ => {
             return Err(CryptoError::CmsSign {
                 reason: SignKind::CurveMismatch,
             });
         }
-    };
+    }
 
     // Build the signer.  `FieldEl::from_le_bytes(bytes, mod_words)`
     // returns `Self` directly (PANICS on `bytes.len() > mod_words * 4`,
@@ -145,7 +142,34 @@ fn sign_cms_blocking(
     let d = FieldEl::from_le_bytes(&session.param_d()[..], curve.mod_words);
     let signer = DstuInProcessSigner::new(d);
 
-    sign_detached_with_content_digest(profile, session.cert_der(), &content_digest, &signer)
+    // Sign as ATTACHED CAdES-BES WITH a `signingTime` signed-attribute — the
+    // exact profile DPS `sendChkV2` accepts:
+    //   * ATTACHED: the cp1251 receipt XML is embedded as `eContent`, because
+    //     the gRPC `Check.check_sign` bytes field is the ONLY document carrier
+    //     on the wire — a detached signature would arrive without the receipt
+    //     and be rejected `-1 ERROR_VEREFY`.
+    //   * signingTime: every official ЦЗО CAdES-BES reference vector
+    //     (`czo_test/*.p7s`) carries contentType + signingTime + messageDigest
+    //     + signingCertificateV2, and WebCheck (EUSignCP signLevel=1, TSP off)
+    //     produces the same.  Omitting it diverges from the accepted profile.
+    // `CmsSigner::sign_with` computes the per-profile content digest and stamps
+    // `signingTime = now()`; no TSP token (that is CAdES-T, off in the
+    // reference).  The `sign_cms_detached` trait-method name is retained for
+    // back-compat; the produced CMS is ATTACHED.
+    let cms_signer = CmsSigner {
+        cert_der: session.cert_der(),
+        signer: &signer,
+        profile,
+    };
+    cms_signer
+        .sign_with(
+            canonical_xml,
+            CmsBuildOptions {
+                attached: true,
+                signing_time: Some(std::time::SystemTime::now()),
+            },
+        )
+        .map(|sig| sig.cms_der)
         .map_err(|_| CryptoError::CmsSign {
             reason: SignKind::BackendError,
         })
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
 
diff --git a/rust/prro/tests/attached_cms_profile.rs b/rust/prro/tests/attached_cms_profile.rs
new file mode 100644
index 0000000..d930ff7
--- /dev/null
+++ b/rust/prro/tests/attached_cms_profile.rs
@@ -0,0 +1,101 @@
+//! **W4-Z3 round-2 — non-skipping, key-free proof of the gateway ATTACHED
+//! CAdES-BES signing profile** (the form DPS `sendChkV2` accepts).
+//!
+//! The gateway's `InProcessProvider` signs via `prro_crypto`'s
+//! `CmsSigner::sign_with(.., CmsBuildOptions { attached: true, signing_time:
+//! Some(now) })`.  This test drives that EXACT API with a STUB signer (so no
+//! private key / JKS is required — it runs in CI under `cargo test -p prro`)
+//! and a committed self-signed X.509 test cert, then byte-asserts the produced
+//! CMS carries the ЦЗО CAdES-BES profile:
+//!   1. `eContent` — the content embedded as a primitive OCTET STRING (ATTACHED);
+//!   2. a `signingTime` signed-attribute (OID 1.2.840.113549.1.9.5);
+//!   3. a `messageDigest` signed-attribute whose value == GOST-34.311(content);
+//!   4. a `contentType` signed-attribute (OID 1.2.840.113549.1.9.3).
+//!
+//! Every official ЦЗО reference vector (`czo_test/*.p7s`) carries (1)–(4); this
+//! test locks the gateway to the same shape so a future regression (e.g. a
+//! detached or signingTime-less envelope) fails in CI rather than at live DPS.
+
+use prro_crypto::cms::{CmsBuildOptions, CmsProfile, CmsSigner, RawSigner, SignerError};
+
+/// Committed self-signed X.509 test cert (DER) — PUBLIC, no key material.
+/// Only parsed for IssuerAndSerialNumber + embedded in `certificates`; the
+/// signature value is stubbed, so the cert's own key/curve is irrelevant.
+const TEST_CERT_DER: &[u8] = include_bytes!(concat!(
+    env!("CARGO_MANIFEST_DIR"),
+    "/tests/fixtures/SELF_SIGNED_ENC_6929.cer"
+));
+
+/// Stub signer: returns a fixed 64-byte raw DSTU signature value.  No key.
+struct StubSigner;
+impl RawSigner for StubSigner {
+    fn sign_digest(&self, _digest: &[u8]) -> Result<Vec<u8>, SignerError> {
+        Ok(vec![0u8; 64])
+    }
+}
+
+fn contains(haystack: &[u8], needle: &[u8]) -> bool {
+    !needle.is_empty() && needle.len() <= haystack.len() && haystack.windows(needle.len()).any(|w| w == needle)
+}
+
+#[test]
+fn gateway_attached_cms_carries_econtent_signingtime_and_messagedigest() {
+    let signer = StubSigner;
+    let cms_signer = CmsSigner {
+        cert_der: TEST_CERT_DER,
+        signer: &signer,
+        profile: CmsProfile::Dstu4145WithGost34311Pb,
+    };
+
+    // Distinctive content (< 128 bytes) so the byte-search + length framing hold.
+    let content = b"<RQ V=\"1\">PRRO-W4Z3-PROFILE-PROOF</RQ>";
+    assert!(content.len() < 0x80, "content must be < 128 bytes for the framing checks");
+
+    // EXACTLY the gateway's production options (see crypto/in_process.rs).
+    let cms = cms_signer
+        .sign_with(
+            content,
+            CmsBuildOptions {
+                attached: true,
+                signing_time: Some(std::time::SystemTime::now()),
+            },
+        )
+        .expect("attached sign with the committed test cert")
+        .cms_der;
+
+    // Sanity: well-formed ContentInfo (outer SEQUENCE).
+    assert_eq!(cms[0], 0x30, "not a SEQUENCE / ContentInfo");
+
+    // (1) ATTACHED — content embedded as a primitive OCTET STRING (eContent).
+    let mut octet_framed = vec![0x04u8, content.len() as u8];
+    octet_framed.extend_from_slice(content);
+    assert!(
+        contains(&cms, &octet_framed),
+        "eContent OCTET STRING(content) missing → CMS is NOT attached"
+    );
+
+    // (2) signingTime signed-attr — OID 1.2.840.113549.1.9.5 (ЦЗО profile requires it).
+    const OID_SIGNING_TIME: [u8; 11] =
+        [0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x05];
+    assert!(
+        contains(&cms, &OID_SIGNING_TIME),
+        "signingTime signed-attr missing — diverges from the accepted ЦЗО CAdES-BES profile"
+    );
+
+    // (3) messageDigest signed-attr — OID 1.2.840.113549.1.9.4 — value == GOST-34.311(content).
+    const OID_MESSAGE_DIGEST: [u8; 11] =
+        [0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x04];
+    assert!(contains(&cms, &OID_MESSAGE_DIGEST), "messageDigest signed-attr missing");
+    let digest = prro_crypto::core::hash::gost_34_311_95(content);
+    let mut digest_framed = vec![0x04u8, digest.len() as u8];
+    digest_framed.extend_from_slice(&digest);
+    assert!(
+        contains(&cms, &digest_framed),
+        "messageDigest value != OCTET STRING(GOST-34.311(content))"
+    );
+
+    // (4) contentType signed-attr — OID 1.2.840.113549.1.9.3 (CAdES-BES required).
+    const OID_CONTENT_TYPE: [u8; 11] =
+        [0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x03];
+    assert!(contains(&cms, &OID_CONTENT_TYPE), "contentType signed-attr missing");
+}
diff --git a/rust/prro/tests/live_dps_extended_smoke.rs b/rust/prro/tests/live_dps_extended_smoke.rs
new file mode 100644
index 0000000..19d35eb
--- /dev/null
+++ b/rust/prro/tests/live_dps_extended_smoke.rs
@@ -0,0 +1,197 @@
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
+//! 2. **Test-host allowlist (default-deny)**: the resolved endpoint's HOST is
+//!    parsed and must be `cabinet.tax.gov.ua` (or a `*-cabinet`/`*.cabinet`
+//!    test/dev cabinet); ANY other host — every production endpoint
+//!    (`prro.tax.gov.ua`, legacy `prro2.tax.gov.ua`, `fs.tax.gov.ua`) and any
+//!    lookalike — is refused, so the smoke can never fiscalize against prod.
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
+    let endpoint = resolve_host();
+    let host = host_of(&endpoint);
+    // Default-deny allowlist on the PARSED host (not a substring of the raw
+    // URL): exact `cabinet.tax.gov.ua`, a `.cabinet…` subdomain, or a
+    // `*-cabinet…` dev cabinet.  Rejects every prod endpoint (prro/prro2/fs)
+    // AND lookalikes like `cabinet.tax.gov.ua.evil.com`.
+    let allowed = host == TEST_HOST_MARKER
+        || host.ends_with(&format!(".{TEST_HOST_MARKER}"))
+        || host.ends_with(&format!("-{TEST_HOST_MARKER}"));
+    if !allowed {
+        panic!(
+            "{test_name} REFUSED: {ENV_HOST}={endpoint} resolves to host `{host}`, \
+             which is not a DPS TEST cabinet (allowlist: `{TEST_HOST_MARKER}` / \
+             `*-{TEST_HOST_MARKER}` / `*.{TEST_HOST_MARKER}`).  The live smoke is \
+             test-server only (default {DEFAULT_HOST}); refusing to risk \
+             fiscalizing against a production endpoint (prro/prro2/fs.tax.gov.ua)."
+        );
+    }
+    true
+}
+
+/// Extract the bare hostname from an endpoint URL — strips scheme, optional
+/// userinfo, port, and any path — so the allowlist matches the real host and
+/// not a substring of the URL (e.g. a path or a lookalike domain).
+fn host_of(endpoint: &str) -> &str {
+    let after_scheme = endpoint.split("://").nth(1).unwrap_or(endpoint);
+    let authority = after_scheme.split('/').next().unwrap_or(after_scheme);
+    let hostport = authority.rsplit('@').next().unwrap_or(authority);
+    hostport.split(':').next().unwrap_or(hostport)
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
index b52afe6..1a9a9ca 100644
--- a/rust/prro_crypto/src/cms/builder.rs
+++ b/rust/prro_crypto/src/cms/builder.rs
@@ -313,6 +313,13 @@ pub fn sign_detached_with_content_digest(
     assemble_signed_data(profile, cert_der, &attrs_der, &signature_value)
 }
 
+// NOTE: the gateway's ATTACHED-with-signingTime signing path goes through the
+// high-level `CmsSigner::sign_with(content, CmsBuildOptions { attached: true,
+// signing_time: Some(now) })` (see rust/prro crypto/in_process.rs), which
+// matches the official ЦЗО CAdES-BES reference profile.  An earlier
+// `sign_attached_with_content_digest` low-level helper was removed: it omitted
+// `signingTime`, which diverges from that reference.
+
 // ─── Hash dispatch ──────────────────────────────────────────────────────────
 
 /// Compute the digest matching the profile's hash algorithm.
diff --git a/rust/prro_crypto/src/cms/mod.rs b/rust/prro_crypto/src/cms/mod.rs
index 90f27bf..934e8aa 100644
--- a/rust/prro_crypto/src/cms/mod.rs
+++ b/rust/prro_crypto/src/cms/mod.rs
@@ -48,7 +48,7 @@ pub mod signer;
 pub mod tsp;
 
 pub use builder::{
-    sign_detached_with_content_digest, CmsError, CmsSigner, DetachedSignature,
+    sign_detached_with_content_digest, CmsBuildOptions, CmsError, CmsSigner, DetachedSignature,
 };
 pub use profile::CmsProfile;
 pub use signer::{DstuInProcessSigner, RawSigner, SignerError};
diff --git a/rust/prro_crypto/tests/test_cms_end_to_end.rs b/rust/prro_crypto/tests/test_cms_end_to_end.rs
index a5da7a5..202874f 100644
--- a/rust/prro_crypto/tests/test_cms_end_to_end.rs
+++ b/rust/prro_crypto/tests/test_cms_end_to_end.rs
@@ -9,7 +9,7 @@
 //! pass since real production uses random rand_e per call.
 
 use prro_crypto::{
-    cms::{CmsProfile, CmsSigner, DstuInProcessSigner},
+    cms::{CmsBuildOptions, CmsProfile, CmsSigner, DstuInProcessSigner},
     interop::prro::{der, jks},
 };
 
@@ -143,3 +143,85 @@ fn test_cms_round_trip_via_x509_parser() {
         result.cms_der.len()
     );
 }
+
+fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
+    needle.len() <= haystack.len() && haystack.windows(needle.len()).any(|w| w == needle)
+}
+
+/// W4-Z3 attached-CMS proof: the high-level `CmsSigner::sign_with(.., attached:
+/// true ..)` embeds the content as `eContent` (the form DPS `sendChkV2`
+/// requires — the gateway's `InProcessProvider` drives the same API with
+/// `signing_time: Some(now)`), whereas the detached form does NOT carry the
+/// content.  `signing_time` is `None` here ONLY to isolate the eContent
+/// variable for the comparison; the signature value differs per call anyway
+/// (DSTU draws a fresh random nonce), so we assert structure, not bytes.
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
+    let cms_signer = CmsSigner {
+        cert_der: &cert_der,
+        signer: &signer,
+        profile: CmsProfile::default(), // Dstu4145WithGost34311Pb
+    };
+
+    let detached = cms_signer
+        .sign_with(content, CmsBuildOptions { attached: false, signing_time: None })
+        .expect("detached sign failed")
+        .cms_der;
+    let attached = cms_signer
+        .sign_with(content, CmsBuildOptions { attached: true, signing_time: None })
+        .expect("attached sign failed")
+        .cms_der;
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
