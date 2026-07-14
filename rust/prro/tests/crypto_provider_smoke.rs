//! W1 smoke: trait wiring + redacted Debug + zero secret-substring leak.
//!
//! Five tests:
//!   1. `signing_session_debug_is_redacted` — manual redacted Debug on
//!      SigningSession; the 32-byte canary must NOT appear in `{:?}`.
//!   2. `sealed_material_debug_is_redacted` — same for SealedMaterial.
//!   3. `unseal_with_garbage_jks_returns_typed_seal_error` — unseal_jks
//!      returns CryptoError::JksUnseal { reason: MalformedJks } for
//!      bytes that aren't a real JKS / Key-6 / PFX container, with no
//!      password substring leak through the Debug output.
//!   4. `fetch_cert_with_no_urls_returns_typed_all_urls_failed` — empty
//!      urls slice → typed CertFetch { reason: AllUrlsFailed } before
//!      any network call is attempted.
//!   5. `verify_dstu_known_good_sig_returns_true` — wrap `verify_dstu`
//!      around a freshly-signed deterministic vector; the wrapper must
//!      accept the good sig.  Then flip one bit and prove the wrapper
//!      does NOT silently return true (returns false OR a typed error).
//!      This is the test that prevents a future stub regression in
//!      `verify_dstu`.
//!   6. `sign_cms_uses_the_sessions_real_private_scalar` — end-to-end
//!      teeth for `SigningSession::param_d`: build a session around a
//!      KNOWN scalar `d`, produce a REAL CMS via `sign_cms_detached`,
//!      extract the raw DSTU signature from the assembled CMS, and prove
//!      it verifies against the pubkey `Q = -d·G` derived from THAT SAME
//!      `d`.  If `param_d` ever returns a constant/leaked scalar instead
//!      of the operator's real key, the signature is made with the wrong
//!      `d` and this verification fails — the exact FW-1 mutant
//!      (`param_d -> Box::leak(Zeroizing::from([1; 32]))`) that survived
//!      because tests only exercised the verifier with a self-contained
//!      `(d, sig, pubkey)` triple that never flowed through `param_d`.

use prro::crypto::{
    CryptoError, CryptoProvider, DstuVerifyResult, InProcessProvider, SealKind, SealedMaterial,
    SignCmsRequest, SignKind, SigningSession, VerifyKind,
};

#[test]
fn signing_session_debug_is_redacted() {
    // 32-byte ASCII canary so a successful redaction assertion below
    // is unambiguous (no UTF-8 lossy mapping).
    let secret: [u8; 32] = *b"super-secret-canary-32bytes-aaaa";
    // "operator-1" stands in for the cashier INN (PII) that the RS-1 key
    // loader now threads into PRODUCTION sessions — Debug must NOT reveal it.
    let session = SigningSession::new_for_test("operator-1".into(), secret, b"<cert-der>".to_vec());
    let s = format!("{session:?}");
    assert!(
        !s.contains("operator-1"),
        "operator_id (cashier INN / PII) must be redacted from Debug: {s}"
    );
    assert!(
        s.contains("<redacted>"),
        "operator_id + param_d should be redacted: {s}"
    );
    // No contiguous prefix of the canary may leak through Debug.
    assert!(
        !s.contains("super-secret-canary"),
        "secret substring leaked: {s}"
    );
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
    let s = format!("{mat:?}");
    // operator_id is the cashier INN (PII) — must be redacted, uniform
    // with SigningSession.
    assert!(
        !s.contains("op-2"),
        "operator_id (INN/PII) must be redacted: {s}"
    );
    assert!(s.contains("<redacted>"));
    // No password / salt / jks-bytes substring may leak.
    assert!(!s.contains("deadbeef"), "password leaked: {s}");
    assert!(!s.contains("keystore-bytes"), "jks bytes leaked: {s}");
}

#[test]
fn unseal_with_garbage_jks_returns_typed_seal_error() {
    // Bytes that look nothing like JKS / Key-6 / PFX magic.  unseal_jks
    // must surface a typed JksUnseal error, NOT panic, NOT reveal the
    // password substring through Debug.
    let salt = [0x11u8; 16];
    let sealed = SealedMaterial {
        operator_id: "op-3",
        jks_bytes: b"this is not a jks file",
        // Sealed = (plaintext "wrong-password" XOR salt), hex-encoded.
        // We don't actually care about the exact value here — the test
        // is that *garbage container bytes* surface a typed error.
        jks_password_hex: "00112233445566778899aabbccddeeff",
        cred_salt: &salt,
    };
    let err = prro::crypto::unseal_jks(sealed).expect_err("garbage bytes must fail");
    let dbg = format!("{err:?}");
    match err {
        CryptoError::JksUnseal {
            reason: SealKind::MalformedJks,
            ..
        } => {} // expected
        CryptoError::JksUnseal {
            reason: SealKind::BadPassword,
            ..
        } => {
            // Acceptable on platforms where the underlying parser
            // detects the magic mismatch as "wrong password" before
            // "malformed JKS" — both reasons are honest classifications
            // of "this isn't a real keystore".  Reject any OTHER
            // CryptoError variant.
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
    // Password hex substring must NOT leak through error Debug.
    assert!(
        !dbg.contains("00112233445566778899aabbccddeeff"),
        "password hex leaked through error Debug: {dbg}"
    );
}

#[tokio::test]
async fn fetch_cert_with_no_urls_returns_typed_all_urls_failed() {
    let provider = InProcessProvider::new();
    let ski = [0u8; 32];
    let err = provider
        .fetch_cert_by_ski(&[], &ski, std::time::Duration::from_secs(5))
        .await
        .expect_err("empty urls must produce a typed error");
    assert!(
        matches!(
            err,
            CryptoError::CertFetch {
                reason: prro::crypto::FetchKind::AllUrlsFailed
            }
        ),
        "expected AllUrlsFailed, got {err:?}"
    );
}

/// Generate a known-good (content_digest, sig_64, pubkey_compressed)
/// triple at test time using a deterministic `(d, hash, rand_e)` and
/// `prro_crypto::core::sign::sign`.  Self-contained — no JSON IO, no
/// frozen hex constants.  Captures the real verifier path and proves
/// `verify_dstu` is not a stub.
fn generate_known_good_dstu_triple() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    use prro_crypto::core::curve::Curve;
    use prro_crypto::core::field::FieldEl;
    use prro_crypto::core::point::{compress_point, Point};
    use prro_crypto::core::sign::sign;

    let curve = Curve::dstu_pb_257();

    // Hard-coded private scalar `d` (32 LE bytes).
    let d_bytes: [u8; 32] = [
        0x42, 0x13, 0x37, 0xc0, 0xde, 0xca, 0xfe, 0x99, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
        0x08, 0xfe, 0xed, 0xfa, 0xce, 0xba, 0xad, 0xf0, 0x0d, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
        0x77, 0x00,
    ];
    let d = FieldEl::from_le_bytes(&d_bytes, curve.mod_words);

    // Build Q = -d·G (jkurwa convention; see `prro_crypto::python::pubkey_dstu_pb_257`).
    let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
    let pub_q = g.mul(&d, &curve).negate();
    let pubkey_compressed = compress_point(&pub_q, &curve);

    // Hard-coded message hash bytes (would normally be the output of
    // `gost_34_311_95(message)` — for the verifier we just need 32
    // bytes; the verifier hashes nothing on its own).
    let content_digest: Vec<u8> = (0..32).map(|i| 0x10u8 ^ (i as u8)).collect();
    let hash_fe = FieldEl::from_le_bytes(&content_digest, curve.mod_words);

    // Deterministic rand_e with word 8 == 0 (per sign()'s contract — see
    // `core/sign.rs:237-`).  We use 32 bytes (= 8 words at PB-257
    // mod_words=9 means word 8 stays zero by from_le_bytes).
    let rand_e_bytes: [u8; 32] = [
        0xa5, 0x5a, 0xc3, 0x3c, 0xf0, 0x0f, 0x12, 0x21, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
        0xee, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0xde, 0xad, 0xbe, 0xef, 0x01, 0x02,
        0x03, 0x04,
    ];
    let rand_e = FieldEl::from_le_bytes(&rand_e_bytes, curve.mod_words);

    let signature = sign(&curve, &d, &hash_fe, &rand_e)
        .expect("deterministic sign must succeed for these inputs");

    // Pack r||s back to 64 LE bytes — the wire shape `verify_dstu` expects.
    // Each FieldEl holds `mod_words` u32 words in LE order.  We need
    // exactly 32 bytes (8 words).
    let mut sig_64 = vec![0u8; 64];
    for (i, b) in sig_64.iter_mut().enumerate().take(32) {
        let word = signature.r.bytes.get(i / 4).copied().unwrap_or(0);
        *b = (word >> ((i % 4) * 8)) as u8;
    }
    for (i, b) in sig_64.iter_mut().enumerate().skip(32).take(32) {
        let local = i - 32;
        let word = signature.s.bytes.get(local / 4).copied().unwrap_or(0);
        *b = (word >> ((local % 4) * 8)) as u8;
    }

    (content_digest, sig_64, pubkey_compressed)
}

#[tokio::test]
async fn verify_dstu_known_good_sig_returns_true() {
    let provider = InProcessProvider::new();
    let (content_digest, sig_64, pubkey_compressed) = generate_known_good_dstu_triple();

    let result = provider
        .verify_dstu(&content_digest, &sig_64, &pubkey_compressed)
        .await
        .expect("verify_dstu must not error on known-good inputs");
    assert!(
        result.0,
        "real verify must accept a known-good sig — if this is false, \
         either the wrapper stub-returned false (regression) or the \
         deterministic vector generation drifted"
    );

    // Negative complement: flip one bit of the signature.  Verify must
    // either return DstuVerifyResult(false) OR a typed VerifyFailed
    // error (depending on whether the flipped sig is structurally
    // malformed or just wrong).  Silently returning true is forbidden.
    let mut bad = sig_64.clone();
    bad[0] ^= 0x01;
    match provider
        .verify_dstu(&content_digest, &bad, &pubkey_compressed)
        .await
    {
        Ok(DstuVerifyResult(false)) => {}
        Err(CryptoError::VerifyFailed { reason: _ }) => {}
        Ok(DstuVerifyResult(true)) => {
            panic!("flipped sig must NOT verify true — silent stub regression?")
        }
        Err(other) => panic!("unexpected error: {other:?}"),
    }

    // Sanity: a 0-byte sig must surface MalformedSignature, not panic.
    match provider
        .verify_dstu(&content_digest, &[], &pubkey_compressed)
        .await
    {
        Err(CryptoError::VerifyFailed {
            reason: VerifyKind::MalformedSignature,
        }) => {}
        other => panic!("expected MalformedSignature, got {other:?}"),
    }
}

// ─── Test 6: `param_d` feeds the operator's REAL private scalar ─────────
//
// FW-1 mutation survivor: `SigningSession::param_d` returns
// `Box::leak(Box::new(Zeroizing::from([1; 32])))` — a leaked constant —
// instead of the real private scalar.  `param_d` is `pub(crate)`, so a
// `tests/` integration crate cannot read it directly; the only surface that
// consumes it is `sign_cms_detached` (and `unwrap_envelope`).  This test
// therefore drives the WHOLE production signing line
// (`InProcessProvider::sign_cms_detached` → `sign_cms_blocking` →
// `FieldEl::from_le_bytes(&session.param_d()[..], ..)` →
// `DstuInProcessSigner`), then cryptographically verifies the produced CMS
// signature against the pubkey `Q = -d·G` derived from the SAME known `d`.
// Under the mutation the signature is made with `d = [1; 32]` → it does not
// verify against `Q(d_known)` → the test fails.

/// Committed self-signed X.509 test cert (DER) — PUBLIC, no key material.
/// `sign_cms_detached` parses it for the `IssuerAndSerialNumber` +
/// `signingCertificateV2` attr; its own key/curve is irrelevant — the CMS
/// signature value is produced from the session's `param_d`, which is what
/// we verify.  (Same fixture the ATTACHED-profile proof uses.)
const TEETH_CERT_DER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/SELF_SIGNED_ENC_6929.cer"
));

/// Known DSTU 4145 private scalar `d` (32 LE bytes) — the "operator key"
/// this session must actually sign with.  Distinct from the constant the
/// mutant leaks (`[1; 32]`).
const TEETH_KNOWN_D: [u8; 32] = [
    0x42, 0x13, 0x37, 0xc0, 0xde, 0xca, 0xfe, 0x99, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
    0xfe, 0xed, 0xfa, 0xce, 0xba, 0xad, 0xf0, 0x0d, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x00,
];

/// Compressed pubkey `Q = -d·G` (jkurwa/DSTU convention — see
/// `prro_crypto::core::sign` tests + `generate_known_good_dstu_triple`
/// above), the form `verify_dstu` expands via `expand_compressed_checked`.
fn teeth_pubkey_from_known_d() -> Vec<u8> {
    use prro_crypto::core::curve::Curve;
    use prro_crypto::core::field::FieldEl;
    use prro_crypto::core::point::{compress_point, Point};

    let curve = Curve::dstu_pb_257();
    let d = FieldEl::from_le_bytes(&TEETH_KNOWN_D, curve.mod_words);
    let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
    let pub_q = g.mul(&d, &curve).negate();
    compress_point(&pub_q, &curve)
}

/// The `SignerInfo.signature` in an ATTACHED DSTU CMS is the terminal
/// `SignatureValue ::= OCTET STRING` — a raw 64-byte `r || s`.  It is the
/// LAST `04 40 <64 bytes>` TLV in the assembled DER (the 64-byte OCTET
/// STRINGs that appear earlier are cert public-key / SKI material, never at
/// the tail).  Return the last such run.
fn teeth_extract_signature(cms: &[u8]) -> Vec<u8> {
    let mut found: Option<Vec<u8>> = None;
    let mut i = 0usize;
    while i + 2 + 64 <= cms.len() {
        if cms[i] == 0x04 && cms[i + 1] == 0x40 {
            found = Some(cms[i + 2..i + 2 + 64].to_vec());
        }
        i += 1;
    }
    found.expect("no 64-byte OCTET STRING (SignerInfo.signature) found in CMS")
}

#[tokio::test]
async fn sign_cms_uses_the_sessions_real_private_scalar() {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    let provider = InProcessProvider::new();
    let profile = prro_crypto::cms::profile::CmsProfile::Dstu4145WithGost34311Pb;
    let content = b"<RQ V=\"1\">FW1-PARAM-D-TEETH</RQ>";

    // Session built around the KNOWN operator scalar.
    let session =
        SigningSession::new_for_test("operator-1".into(), TEETH_KNOWN_D, TEETH_CERT_DER.to_vec());

    // Produce a REAL CMS via the production provider path.  It stamps
    // `signingTime = SystemTime::now()` internally (whole-second UTCTIME
    // resolution), so bracket the call to recover the exact second later.
    let sec_before = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let cms = provider
        .sign_cms_detached(SignCmsRequest {
            session: &session,
            canonical_xml: content,
            profile,
        })
        .await
        .expect("sign_cms_detached must succeed with the committed test cert")
        .0;
    let sec_after = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Raw DSTU r||s the provider actually placed in the SignerInfo.
    let sig = teeth_extract_signature(&cms);
    assert_eq!(sig.len(), 64, "DSTU signature value must be 64 bytes");

    // Pubkey derived from the KNOWN d.  If `param_d` returned the real
    // scalar, the sig verifies against this; if it leaked `[1; 32]`, it
    // does not.
    let pubkey = teeth_pubkey_from_known_d();

    // The signer signs H(SET OF signedAttrs).  Rebuild those signedAttrs
    // via the SAME public builder the provider uses, sweeping the 1-2
    // candidate whole-seconds the `now()` stamp could have landed on.
    let content_digest = prro_crypto::core::hash::gost_34_311_95(content);
    let mut verified = false;
    for sec in sec_before..=sec_after {
        let t = UNIX_EPOCH + Duration::from_secs(sec);
        let attrs = prro_crypto::cms::attrs::build_signed_attrs_with_time(
            profile,
            &content_digest,
            TEETH_CERT_DER,
            Some(t),
        )
        .expect("rebuild signedAttrs");
        let attrs_der = attrs.to_der_set_of().expect("SET OF signedAttrs DER");
        let signed_attrs_digest = prro_crypto::core::hash::gost_34_311_95(&attrs_der);

        if let Ok(DstuVerifyResult(true)) = provider
            .verify_dstu(&signed_attrs_digest, &sig, &pubkey)
            .await
        {
            verified = true;
            break;
        }
    }

    assert!(
        verified,
        "CMS signature does NOT verify against the pubkey derived from the \
         session's known private scalar d — `SigningSession::param_d` fed the \
         signer a DIFFERENT scalar than the one the session holds (e.g. a \
         leaked constant). Every operator signature would be made with the \
         wrong key → DPS `CryptBadSign` → zero receipts issuable."
    );
}

/// FW-1 teeth (MEDIUM survivor `in_process.rs:129`): the accepted CMS
/// profile guard in `sign_cms_blocking` must MATCH the production profile
/// (`Dstu4145WithGost34311Pb`), NOT let it fall through to the wildcard
/// reject arm.  Deleting the accept arm (the cargo-mutants MEDIUM survivor)
/// makes the ONLY production profile fall through → `SignKind::CurveMismatch`
/// → every fiscal document fails to sign → total loss of issuance.
///
/// Oracle design: drive `sign_cms_detached` for the accepted profile with a
/// deliberately-garbage cert-DER.  On correct code the profile guard passes,
/// so the signer proceeds and fails LATER inside `CmsSigner::sign_with`
/// (bad cert) → `SignKind::BackendError`.  Under the mutation the accepted
/// profile is no longer matched → the wildcard arm returns
/// `SignKind::CurveMismatch` BEFORE any signer work.  We assert the surfaced
/// error is NOT `CurveMismatch`; on correct code that holds (BackendError),
/// under the mutation it fails.  This is the assertion the existing
/// `secret_flow_tracing::drive_cms_sign_path` driver lacked.
#[tokio::test]
async fn accepted_cms_profile_is_not_rejected_as_curve_mismatch() {
    // Accepted production profile + garbage cert-DER: correct code passes the
    // profile guard, then fails at sign time (BackendError, not CurveMismatch).
    let secret: [u8; 32] = *b"fw1-teeth-canary-secret-32bytes!";
    let session =
        SigningSession::new_for_test("op-fw1".into(), secret, b"<not-a-real-cert-DER>".to_vec());
    let request = prro::crypto::SignCmsRequest {
        session: &session,
        canonical_xml: b"<test-payload/>",
        profile: prro_crypto::cms::profile::CmsProfile::Dstu4145WithGost34311Pb,
    };
    let provider = InProcessProvider::new();
    let err = provider
        .sign_cms_detached(request)
        .await
        .expect_err("garbage cert-DER must surface a CmsSign error");

    // The accepted profile MUST be matched by the guard, so the failure must
    // come from downstream (BackendError), NOT the wildcard CurveMismatch
    // reject.  If the accept arm is deleted (the survivor), the accepted
    // profile falls through and this assertion fails.
    match err {
        CryptoError::CmsSign {
            reason: SignKind::CurveMismatch,
        } => panic!(
            "accepted production profile Dstu4145WithGost34311Pb was rejected as \
             CurveMismatch — the CmsProfile accept arm in sign_cms_blocking was \
             not matched (in_process.rs:129 accept arm deleted?). This means every \
             fiscal document would fail to sign."
        ),
        CryptoError::CmsSign {
            reason: SignKind::BackendError,
        } => {} // expected on correct code: guard passed, signer rejected garbage cert
        other => panic!(
            "unexpected error variant for accepted profile + garbage cert: {other:?} — \
             expected CmsSign {{ BackendError }}"
        ),
    }
}
