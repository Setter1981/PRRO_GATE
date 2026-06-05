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

use prro::crypto::{
    CryptoError, CryptoProvider, DstuVerifyResult, InProcessProvider, SealKind, SealedMaterial,
    SigningSession, VerifyKind,
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
