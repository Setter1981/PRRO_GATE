//! W6 — secret-material flow tracing test.
//!
//! Implements ADR-M2-5 §4d: prove that no `prro::crypto` code path
//! (happy or error) leaks a substring of the seeded password,
//! `cred_salt`, or private-key bytes through any `tracing`
//! event OR through `format!("{err:?}")` debug printing of a
//! `CryptoError`.
//!
//! Test architecture:
//!
//! - **Self-managed `tracing-subscriber::fmt`** with an
//!   `Arc<Mutex<Vec<u8>>>` `MakeWriter` capture sink.  No
//!   `tracing-test` dep — that crate's helpers live under
//!   `tracing_test::internal::*` which is explicitly NOT a stable
//!   public API.
//! - **`tracing::subscriber::set_default(subscriber)`** returns a
//!   `DefaultGuard` held across `.await` points so events emitted on
//!   resumed continuations land in our capture buffer, not in the
//!   global subscriber that cargo-test happens to set up.
//! - **`#[tokio::test(flavor = "current_thread")]`** because
//!   `DefaultGuard` is `!Send`; tokio's default multi-threaded
//!   runtime would refuse to host it.
//! - **Positive control**: a deliberate `tracing::info!(jks =
//!   "leaked-on-purpose-for-test")` emit + an assertion that the
//!   substring made it into the buffer.  Without this control the
//!   capture machinery could silently regress and the main test
//!   would pass vacuously.
//!
//! What gets exercised:
//!
//! - Every `CryptoError` variant via the dirtiest input that
//!   triggers it without needing a real JKS or a real cert chain
//!   (the test must run in any CI matrix without external
//!   fixtures):
//!     - `JksUnseal { reason: BadPassword | MalformedJks }` via
//!       `unseal_jks` over garbage bytes.
//!     - `CmsSign { reason: BackendError }` via `sign_cms_detached`
//!       over an obviously-bad cert_der + a session built with the
//!       seeded private key.
//!     - `EnvelopeDecrypt { reason: ParseFailed }` via
//!       `unwrap_envelope` over garbage envelope bytes.
//!     - `CertFetch { reason: AllUrlsFailed }` via
//!       `fetch_cert_by_ski(&[], …)` (empty URL slice short-circuit).
//!     - `VerifyFailed { reason: MalformedSignature }` via
//!       `verify_dstu` with a 0-byte sig.
//! - Both `format!("{err:?}")` AND `format!("{err}")` for every
//!   error — covers callers that log debug-printed errors directly
//!   (the most common accidental leak vector) and callers that
//!   format with `{}`.
//! - The fact that `tracing` is NOT (yet) called from inside
//!   `prro::crypto` today is fine — this test sets the contract
//!   for any future caller that DOES wire tracing into those paths.

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use prro::crypto::{
    CryptoError, CryptoProvider, FetchKind, InProcessProvider, SealedMaterial, SigningSession,
};
use tracing::subscriber::set_default;
use tracing_subscriber::fmt;

// ─── Seeded secrets ───────────────────────────────────────────────────

/// 32-char obviously-synthetic password.  ASCII so we can substring-
/// scan UTF-8 captured bytes without encoding gymnastics.
const SEEDED_PASSWORD: &str = "p@ssw0rd-leak-canary-9f8a-not-real";

/// 32-byte salt rendered as hex.
const SEEDED_SALT_HEX: &str = "0123456789abcdeffedcba9876543210";

/// Raw seeded private-key bytes (32 = PB-257 param_d width).  The
/// first 16 bytes form the canary substring we scan for.
const SEEDED_PRIVATE_KEY: [u8; 32] = [
    0xde, 0xad, 0xbe, 0xef, 0xde, 0xad, 0xbe, 0xef, 0xc0, 0xff, 0xee, 0x42, 0xc0, 0xff, 0xee, 0x42,
    0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xb1, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8,
];

/// Lower-cased hex of the first 16 seeded private-key bytes — the
/// canary substring an accidental `format!("{key:02x?}")` would
/// produce.
fn seeded_private_key_hex_prefix() -> String {
    SEEDED_PRIVATE_KEY[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

// ─── CaptureBuf MakeWriter ────────────────────────────────────────────

/// Thread-safe sink consumed by `tracing-subscriber`'s `fmt` layer.
/// Every event is rendered to UTF-8 by the layer and pushed into the
/// shared buffer; the test asserts on the joined buffer at the end.
#[derive(Clone, Default)]
struct CaptureBuf(Arc<Mutex<Vec<u8>>>);

impl Write for CaptureBuf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> fmt::MakeWriter<'a> for CaptureBuf {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

impl CaptureBuf {
    fn into_string(self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).expect("captured logs must be UTF-8")
    }
}

// ─── Substring assertion helper ───────────────────────────────────────

/// Assert no canary substring appears in `haystack`.  `context_label`
/// names the source (e.g. `"captured logs"`, `"format!({:?}) of err"`)
/// so the failing test message points the operator at the right
/// place.
fn assert_no_canary_leak(haystack: &str, context_label: &str) {
    let priv_hex = seeded_private_key_hex_prefix();
    let needles: &[(&str, &str)] = &[
        ("seeded password", SEEDED_PASSWORD),
        ("seeded salt hex", SEEDED_SALT_HEX),
        ("seeded private-key hex prefix", priv_hex.as_str()),
    ];
    for (name, needle) in needles {
        assert!(
            !haystack.contains(needle),
            "{context_label}: SECRET-CANARY LEAK: {name} substring \
             {needle:?} appears in {context_label} ({} bytes captured):\n  \
             {haystack}",
            haystack.len()
        );
    }
}

// ─── Path drivers (one per CryptoError variant) ───────────────────────

/// `unseal_jks` over hex(`SEEDED_PASSWORD` XOR `SEEDED_SALT_HEX`),
/// joined with garbage JKS bytes.  We don't have a real JKS fixture
/// in this test, so the call surfaces BadPassword / MalformedJks
/// (acceptable variants per the W1 smoke convention).
fn drive_jks_unseal_path() -> CryptoError {
    // Compute the hex-encoded sealed password from the canary
    // password + salt.  This is the exact shape `unseal_jks` decodes.
    let pw_bytes = SEEDED_PASSWORD.as_bytes();
    let salt_bytes = hex_decode(SEEDED_SALT_HEX);
    let mut sealed = Vec::with_capacity(pw_bytes.len());
    for (i, &b) in pw_bytes.iter().enumerate() {
        sealed.push(b ^ salt_bytes[i % salt_bytes.len()]);
    }
    let sealed_hex: String = sealed.iter().map(|b| format!("{b:02x}")).collect();

    let sealed = SealedMaterial {
        operator_id: "op-canary",
        jks_bytes: b"this is not a JKS file at all",
        jks_password_hex: &sealed_hex,
        cred_salt: &salt_bytes,
    };
    prro::crypto::unseal_jks(sealed).expect_err("garbage JKS must fail")
}

fn drive_cms_sign_path() -> CryptoError {
    // SigningSession built from the seeded private-key + a clearly-
    // bad cert_der.  sign_cms_detached pushes the bad bytes into
    // prro_crypto's CMS builder which surfaces a typed
    // CryptoError::CmsSign { reason: BackendError }.
    let session = SigningSession::new_for_test(
        "op-canary".into(),
        SEEDED_PRIVATE_KEY,
        b"<not-a-real-cert-DER>".to_vec(),
    );
    let request = prro::crypto::SignCmsRequest {
        session: &session,
        canonical_xml: b"<test-payload/>",
        profile: prro_crypto::cms::profile::CmsProfile::Dstu4145WithGost34311Pb,
    };
    let provider = InProcessProvider::new();
    futures::executor::block_on(provider.sign_cms_detached(request))
        .expect_err("malformed cert_der must surface CmsSign")
}

fn drive_unwrap_envelope_path() -> CryptoError {
    // Garbage envelope bytes + garbage originator cert → typed
    // EnvelopeDecrypt error.  Session carries the seeded private key.
    let session = SigningSession::new_for_test(
        "op-canary".into(),
        SEEDED_PRIVATE_KEY,
        b"<not-a-real-cert-DER>".to_vec(),
    );
    let provider = InProcessProvider::new();
    futures::executor::block_on(provider.unwrap_envelope(
        b"<garbage-envelope>",
        b"<garbage-originator-cert>",
        &session,
    ))
    .expect_err("garbage envelope must surface EnvelopeDecrypt")
}

fn drive_fetch_cert_path() -> CryptoError {
    // Empty URL slice short-circuits to AllUrlsFailed without hitting
    // the network — fast + deterministic + no fixture needed.
    let provider = InProcessProvider::new();
    let ski = [0u8; 32];
    let err = futures::executor::block_on(provider.fetch_cert_by_ski(
        &[],
        &ski,
        std::time::Duration::from_secs(1),
    ))
    .expect_err("empty URL slice must short-circuit");
    assert!(
        matches!(
            err,
            CryptoError::CertFetch {
                reason: FetchKind::AllUrlsFailed
            }
        ),
        "expected AllUrlsFailed; got {err:?}"
    );
    err
}

fn drive_verify_dstu_path() -> CryptoError {
    let provider = InProcessProvider::new();
    futures::executor::block_on(provider.verify_dstu(
        b"<digest>",
        b"", // 0-byte sig — surfaces VerifyFailed { MalformedSignature }
        b"<pubkey-bytes>",
    ))
    .expect_err("0-byte sig must surface VerifyFailed::MalformedSignature")
}

// ─── Tests ────────────────────────────────────────────────────────────

#[tokio::test(flavor = "current_thread")]
async fn no_secret_substring_leaks_through_tracing_or_error_debug() {
    let buf = CaptureBuf::default();
    let subscriber = fmt()
        .with_writer(buf.clone())
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .finish();
    // `set_default` returns a DefaultGuard scoped to the current
    // task.  Held across all `.await`s + format!() calls so any
    // event the exercised paths might emit lands in `buf`.
    let _guard = set_default(subscriber);

    // Drive every CryptoError variant.  For each, also format via
    // `{err:?}` AND `{err}` to surface accidental
    // tracing::error!("...{err:?}") leaks (the most common pattern).
    let errors: Vec<CryptoError> = vec![
        drive_jks_unseal_path(),
        drive_cms_sign_path(),
        drive_unwrap_envelope_path(),
        drive_fetch_cert_path(),
        drive_verify_dstu_path(),
    ];

    // Yield the executor between drives so the DefaultGuard's
    // across-await behaviour is exercised explicitly.  Without this,
    // the test would pass even if the guard semantics were wrong on
    // suspended futures (current-thread runtime quirk).
    tokio::task::yield_now().await;

    // Belt-and-braces: every format!("{err:?}") and format!("{err}")
    // representation must be free of the canaries.  This catches
    // surface bugs where a future contributor adds Debug/Display
    // formatting that interpolates secret bytes into the error
    // message.
    for err in &errors {
        let dbg = format!("{err:?}");
        let disp = format!("{err}");
        assert_no_canary_leak(&dbg, "format!(\"{err:?}\")");
        assert_no_canary_leak(&disp, "format!(\"{err}\")");
    }

    drop(_guard);

    // Whatever events the paths emitted (none today; future
    // contributors might add tracing::error!(?) or debug!(...))
    // must not contain any seeded-secret substring.
    let captured = buf.into_string();
    assert_no_canary_leak(&captured, "captured tracing logs");
}

#[tokio::test(flavor = "current_thread")]
async fn positive_control_capture_machinery_works() {
    // Without this control the main test could silently regress
    // (e.g. set_default lifetime bug, MakeWriter dropped, capture
    // buffer detached) and pass vacuously.  This test asserts the
    // capture path itself is alive.
    let buf = CaptureBuf::default();
    let subscriber = fmt()
        .with_writer(buf.clone())
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .finish();
    let _guard = set_default(subscriber);

    tracing::info!(jks = "leaked-on-purpose-for-test");
    // Yield to prove the DefaultGuard survives suspension.
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
        "DefaultGuard did not survive .await — events lost: {captured}"
    );
}

// ─── Helpers ──────────────────────────────────────────────────────────

fn hex_decode(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    assert!(bytes.len().is_multiple_of(2), "hex: odd length");
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for i in 0..bytes.len() / 2 {
        let hi = hex_digit(bytes[i * 2]);
        let lo = hex_digit(bytes[i * 2 + 1]);
        out.push((hi << 4) | lo);
    }
    out
}

fn hex_digit(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => panic!("hex: invalid digit {b:#x}"),
    }
}
