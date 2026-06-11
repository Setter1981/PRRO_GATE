# External Code Review Request — W4-Z3 **ROUND 4 (convergence)** — Multi-Protocol PRRO Gateway (Rust)

## Your role

Senior Rust engineer with deep CMS / PKCS#7 / ASN.1-DER, Ukrainian fiscal-crypto
(DSTU 4145-2002 / GOST 34.311-95 / CAdES), and **date/time-arithmetic** rigor. You
are re-reviewing an unmerged local branch of a Ukrainian PRRO gateway (native
in-process `prro_crypto` signing; no jkurwa sidecar; Python tree is an incomplete
reference). Correctness + auditability over performance.

## This is ROUND 4 — verify the round-3 fix + reach convergence

Round-3 had **two** external reviewers and produced a **SPLIT verdict**:
- Reviewer X: **BLOCK** — found a real **High** bug: `unix_secs_to_utc` computed the
  Jan/Feb month as `mp + (-9i64 as u64)` (mp:u64) → integer **overflow panic** under
  overflow-checks (the `cargo test` debug default; no profile override exists), on
  the now-load-bearing `signingTime` path. Invisible to tests that stamp `now()`.
- Reviewer Y: **MERGE**, calling the same `encode_signing_time_utc` "flawless / fully
  correct" — which was **wrong** (it never exercised the Jan/Feb arm).

We fixed the bug (commit `e811ea8`, signed arithmetic), then **self-audited the whole
date/time conversion for siblings** and added boundary regression tests (`cc196a2`).
The full diff (now SIX commits, `1ee3ca3..HEAD`) is embedded at the bottom.

```
cc196a2  review-r3b   date self-audit — boundary + 2050-cliff regression tests
e811ea8  review-r3    FIX High signingTime Jan/Feb overflow panic + regression test
23cccc3  review-r2    integrate external review (signingTime via CmsSigner::sign_with, profile test, host allowlist, CI gate)
9e38c9b  review-r1    cfg-gate + cabinet-allowlist + doc + framing
07d8499  attached-fix InProcessProvider signs ATTACHED CMS
25475cb  piece-1      live-dps feature + smoke harness + connect probe
```

## Round-3 fix + self-audit (verify these)

**Fix:** `month = (mp as i64 + if mp < 10 { 3 } else { -9 }) as u32` (mp < 12) — signed,
no overflow for any month under overflow-checks.

**Self-audit conclusion (challenge it):** `unix_secs_to_utc` is the canonical Howard
Hinnant `civil_from_days` algorithm — leap-correct by construction over the 400-year
Gregorian cycle; the `z < 0` arm is dead (`secs: u64` → `days >= 0`). The `> 2049`
hard-fail in `encode_signing_time_utc` is a **deliberate, loud fail-fast** (UTCTIME per
RFC 5652 covers only 1950–2049; 2050+ needs GeneralizedTime — tracked, ~24y out).

**New regression test** `tests/attached_cms_profile.rs` (runs in CI, debug/overflow-
checked) locks: 2021-01-01 (Jan), 2024-02-29 (leap day), 2023-12-31 23:59:59 (Dec
branch mp=9 + full time rollover), 2049-12-31 (UTCTIME upper cliff, valid), and
2050-01-01 → **must be `Err`** (fail-fast, not panic, not silent-wrong).

## What I want from you (terse, adversarial) — focus on the date math

- **A. Date/time arithmetic.** Is `unix_secs_to_utc` now correct for ALL inputs? Trace
  the Hinnant transcription for: integer-overflow/underflow under overflow-checks
  anywhere else (the `doe - doe/1460 + …` and `doy - (…)` subtractions), leap-year /
  Feb-29 / Dec-31 / year-rollover correctness, and the day-of-month formula. Any
  remaining `as`-cast that can truncate or wrap? Is the `1950..=2049` lower bound
  (dead for `u64` secs) harmless?
- **B. signingTime semantics.** UTCTIME `yyMMddHHmmssZ` 13-byte encoding (tag 0x17) —
  RFC-5652-correct? Is the 2050 hard-fail the right call vs silently switching to
  GeneralizedTime now? (We chose fail-fast + tracked follow-up.)
- **C. Regression-test sufficiency.** Does the boundary test actually catch a
  reintroduced overflow (it runs under overflow-checks) and a wrong month/day (it
  asserts the exact UTCTIME substring)? Any boundary it omits that you'd demand
  (e.g. 2000-02-29 century-leap, 1972 first-leap, second=60 leap-second inputs)?
- **D. Anything from rounds 1–3 that regressed**, or any NEW issue in the 6-commit diff.
- **E. Convergence call.** Given the fix + audit + tests, and that **live DPS
  acceptance of the profile is the explicit job of later W4-Z3 pieces** (not these
  foundation commits): MERGE, MERGE-WITH-FOLLOWUPS, or BLOCK?

Accepted/tracked deferrals (don't re-litigate unless you find them unsafe): `#4`
rename `sign_cms_detached`→`sign_cms_attached` (future-only footgun, doc-noted);
2050 GeneralizedTime; live-acceptance = pieces 4+.

## Output format

Numbered findings: **Severity** · **Category (A–E)** · **file:line** (cite the embedded
diff / context; do NOT invent line numbers) · **concern** · **fix (1–2 lines)**. Clean
category → say so. End with exactly one of "Recommend MERGE", "Recommend MERGE WITH
FOLLOW-UPS", "Recommend BLOCK" + one-sentence justification. Terse.

---

## CONTEXT CODE (pre-existing + r3-fixed — the date/time conversion, the round-4 focus)

`prro_crypto/src/cms/attrs.rs` — the `signingTime` path: `CmsSigner::sign_with` →
`build_signed_attrs_with_time(.., Some(now))` → `encode_signing_time_utc` →
`unix_secs_to_utc`. (The CMS assembly + signed-attrs SET ordering were reviewed in
rounds 2–3 and are unchanged; only the date code changed in r3.)

```rust
fn encode_signing_time_utc(t: std::time::SystemTime) -> Result<Vec<u8>, AttrsError> {
    let secs = t
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| AttrsError::Der("signing_time before UNIX epoch".into()))?
        .as_secs();
    let (year, month, day, hour, minute, second) = unix_secs_to_utc(secs);
    if !(1950..=2049).contains(&year) {
        return Err(AttrsError::Der(format!(
            "signingTime year {} outside UTCTIME range 1950..=2049 — \
             switch to GeneralizedTime is required",
            year
        )));
    }
    let yy = year % 100;
    let s = format!("{:02}{:02}{:02}{:02}{:02}{:02}Z", yy, month, day, hour, minute, second);
    let bytes = s.as_bytes();
    debug_assert_eq!(bytes.len(), 13);
    // UTCTIME tag = 0x17, length = 13, then ASCII content.
    let mut out = Vec::with_capacity(15);
    out.push(0x17);
    out.push(bytes.len() as u8);
    out.extend_from_slice(bytes);
    Ok(out)
}

/// Convert UNIX seconds-since-epoch into broken-down UTC components (Hinnant).
fn unix_secs_to_utc(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let days = secs / 86_400;
    let sod = (secs % 86_400) as u32;
    let hour = sod / 3600;
    let minute = (sod % 3600) / 60;
    let second = sod % 60;

    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;                       // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);          // [0, 365]
    let mp = (5 * doy + 2) / 153;                               // [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;            // [1, 31]
    // r3 FIX — signed month arithmetic (was `mp + (-9i64 as u64)`, overflow-panicked
    // for Jan/Feb mp>=10 under overflow-checks).  mp < 12.
    let month = (mp as i64 + if mp < 10 { 3 } else { -9 }) as u32; // [1, 12]
    let year = (y + if month <= 2 { 1 } else { 0 }) as u32;
    (year, month, day, hour, minute, second)
}
```

---

## UNIFIED DIFF (`git diff 1ee3ca3..HEAD` — 6 commits)

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
index 0000000..20bc652
--- /dev/null
+++ b/rust/prro/tests/attached_cms_profile.rs
@@ -0,0 +1,184 @@
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
+
+/// Regression for the `signingTime` UTCTIME conversion (`unix_secs_to_utc`):
+/// the Hinnant month adjustment MUST use signed arithmetic.  For Jan/Feb
+/// (mp≥10) the old `mp + (-9i64 as u64)` form OVERFLOWED under overflow-checks
+/// (the `cargo test` debug default) → panic on every Jan/Feb signingTime — and
+/// signingTime is now load-bearing in the gateway CMS path.  The other tests
+/// stamp `now()` (currently outside Jan/Feb) so they would miss it.  These
+/// fixed timestamps exercise the Jan/Feb branch and assert the encoded UTCTIME
+/// carries the CORRECT month (01 / 02), not merely that it did not panic.
+#[test]
+fn signing_time_jan_feb_encode_correct_month_without_overflow() {
+    let signer = StubSigner;
+    let cms_signer = CmsSigner {
+        cert_der: TEST_CERT_DER,
+        signer: &signer,
+        profile: CmsProfile::Dstu4145WithGost34311Pb,
+    };
+    let content = b"x";
+    // (unix secs, expected UTCTIME yyMMddHHmmssZ)
+    let cases: [(u64, &[u8]); 2] = [
+        (1_609_459_200, b"210101000000Z"), // 2021-01-01T00:00:00Z (Jan, Hinnant mp=10)
+        (1_612_137_600, b"210201000000Z"), // 2021-02-01T00:00:00Z (Feb, Hinnant mp=11)
+    ];
+    for (unix, utctime) in cases {
+        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(unix);
+        let cms = cms_signer
+            .sign_with(content, CmsBuildOptions { attached: true, signing_time: Some(t) })
+            .expect("Jan/Feb signingTime must encode without panic/error")
+            .cms_der;
+        assert!(
+            contains(&cms, utctime),
+            "signingTime UTCTIME {:?} missing — Jan/Feb month conversion wrong/overflowed",
+            std::str::from_utf8(utctime).unwrap()
+        );
+    }
+}
+
+/// Sibling coverage for the date/time conversion (date bugs travel in packs):
+/// leap-day Feb 29, the Dec branch (Hinnant mp=9, opposite arm to Jan/Feb), a
+/// full time-of-day rollover, the UTCTIME upper cliff (2049 still valid), and
+/// the 2050 **fail-fast** — which MUST be an `Err`, NOT a panic and NOT a
+/// silently-wrong UTCTIME (UTCTIME per RFC 5652 only covers 1950–2049; 2050+
+/// requires GeneralizedTime — a tracked future follow-up).  All via the
+/// production `CmsSigner::sign_with` signingTime path.
+#[test]
+fn signing_time_date_boundaries_and_2050_cliff() {
+    let signer = StubSigner;
+    let cms_signer = CmsSigner {
+        cert_der: TEST_CERT_DER,
+        signer: &signer,
+        profile: CmsProfile::Dstu4145WithGost34311Pb,
+    };
+    let content = b"x";
+    let sign_at = |secs: u64| {
+        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
+        cms_signer.sign_with(content, CmsBuildOptions { attached: true, signing_time: Some(t) })
+    };
+
+    // Valid dates → the correct UTCTIME must be present.
+    let ok: [(u64, &[u8]); 4] = [
+        (1_709_164_800, b"240229000000Z"), // 2024-02-29 00:00:00Z — leap day
+        (1_704_067_199, b"231231235959Z"), // 2023-12-31 23:59:59Z — Dec branch (mp=9) + time rollover
+        (2_524_521_600, b"491231000000Z"), // 2049-12-31 00:00:00Z — UTCTIME upper cliff, still valid
+        (1_609_459_200, b"210101000000Z"), // 2021-01-01 00:00:00Z — Jan sanity
+    ];
+    for (secs, utctime) in ok {
+        let cms = sign_at(secs)
+            .expect("valid signingTime must encode without panic/error")
+            .cms_der;
+        assert!(
+            contains(&cms, utctime),
+            "signingTime UTCTIME {:?} missing/wrong",
+            std::str::from_utf8(utctime).unwrap()
+        );
+    }
+
+    // 2050-01-01 → outside UTCTIME range → MUST fail-fast (Err), not panic, not
+    // a silently-wrong encoding.  Locks the deliberate 1950..=2049 gate.
+    assert!(
+        sign_at(2_524_608_000).is_err(),
+        "2050 signingTime must error (UTCTIME range exceeded), not silently encode"
+    );
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
diff --git a/rust/prro_crypto/src/cms/attrs.rs b/rust/prro_crypto/src/cms/attrs.rs
index 09ff9a9..52e36e0 100644
--- a/rust/prro_crypto/src/cms/attrs.rs
+++ b/rust/prro_crypto/src/cms/attrs.rs
@@ -201,7 +201,11 @@ fn unix_secs_to_utc(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
     let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
     let mp = (5 * doy + 2) / 153;
     let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
-    let month = (mp + if mp < 10 { 3 } else { -9i64 as u64 }) as u32;
+    // Hinnant month: mp∈[0,11] → month = mp<10 ? mp+3 : mp-9 (Jan=mp10, Feb=mp11).
+    // MUST use signed arithmetic: the old `mp + (-9i64 as u64)` form overflowed
+    // for mp≥10 under overflow-checks (cargo test debug default) → panic on every
+    // Jan/Feb signingTime, and relied on wraparound otherwise.  mp is tiny (<12).
+    let month = (mp as i64 + if mp < 10 { 3 } else { -9 }) as u32;
     let year = (y + if month <= 2 { 1 } else { 0 }) as u32;
     (year, month, day, hour, minute, second)
 }
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
