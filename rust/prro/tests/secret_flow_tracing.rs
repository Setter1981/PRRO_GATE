//! W6 — secret-material flow tracing test.
//!
//! Implements ADR-M2-5 §4d: prove that no `prro::crypto` code path
//! leaks a substring of the seeded password / `cred_salt` / private-
//! key bytes through:
//!
//! - any `tracing` event (in any encoding a Rust `Debug` impl might
//!   emit — lowercase hex, uppercase hex, decimal `[u8]` Debug,
//!   `{:02x?}` Debug, spaced/colon hex);
//! - `format!("{err:?}")` / `format!("{err}")` debug printing of a
//!   `CryptoError`;
//! - `format!("{:?}")` of secret-bearing types (`SigningSession`,
//!   `SealedMaterial`).
//!
//! Architecture:
//!
//! - **Global subscriber via `set_global_default`** (NOT thread-local
//!   `set_default`).  Sensitive paths inside `InProcessProvider` run
//!   inside `tokio::task::spawn_blocking` closures — that lands on
//!   tokio's blocking thread pool, not the current_thread executor.
//!   A `set_default`-only subscriber would silently miss every
//!   `tracing::error!(?session)` a future contributor adds inside
//!   `sign_cms_blocking`, `unwrap_envelope_blocking`, or
//!   `fetch_cert_blocking`.  We install once per process and serialise
//!   tests via a `Mutex` so they don't race the shared capture buffer.
//! - **Self-managed `Arc<Mutex<Vec<u8>>>` MakeWriter** — no
//!   `tracing-test` dep (its helpers live under
//!   `tracing_test::internal::*` which is explicitly NOT public API).
//! - **Cross-thread positive control** — a `spawn_blocking` closure
//!   emits a seeded canary; the test proves the global subscriber
//!   captures it from the blocking thread pool, not just from the
//!   test's runtime thread.
//! - **Per-needle positive detector** — every canary representation
//!   (lowercase / uppercase / decimal / hex-debug / spaced / colon)
//!   is exercised by emitting a synthesised string in each form and
//!   verifying the substring scanner detects it.  Without this proof
//!   the main test could silently miss leak forms it never named.

use std::io::{self, Write};
use std::sync::{Arc, Mutex, OnceLock};

use prro::crypto::{
    CryptoError, CryptoProvider, FetchKind, InProcessProvider, SealedMaterial, SigningSession,
};
use tracing_subscriber::fmt;

// ─── Seeded secrets ───────────────────────────────────────────────────

/// 32-char obviously-synthetic password.  ASCII so we can substring-
/// scan UTF-8 captured bytes without encoding gymnastics.
const SEEDED_PASSWORD: &str = "p@ssw0rd-leak-canary-9f8a-not-real";

/// 32-byte salt rendered as hex.
const SEEDED_SALT_HEX: &str = "0123456789abcdeffedcba9876543210";

/// Raw seeded private-key bytes (32 = PB-257 param_d width).  Picked
/// so the first 8 bytes are visually unique across encodings.
const SEEDED_PRIVATE_KEY: [u8; 32] = [
    0xde, 0xad, 0xbe, 0xef, 0xde, 0xad, 0xbe, 0xef, 0xc0, 0xff, 0xee, 0x42, 0xc0, 0xff, 0xee, 0x42,
    0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xb1, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8,
];

/// Build the per-encoding needles for the seeded private key.  We
/// scan only the first 8 bytes — that's already 64 bits of entropy,
/// far more uniqueness than any tracing message would carry by
/// accident, and short enough to be a substring-friendly canary.
fn private_key_needles() -> Vec<(&'static str, String)> {
    let prefix = &SEEDED_PRIVATE_KEY[..8];
    let lowercase_hex: String = prefix.iter().map(|b| format!("{b:02x}")).collect();
    let uppercase_hex: String = prefix.iter().map(|b| format!("{b:02X}")).collect();
    // Decimal Debug for `&[u8]` / `[u8; N]` — Rust's default Debug
    // for byte slices renders each byte as its decimal value.
    // Substring "222, 173, 190, 239" is enough to identify the
    // canary; we don't need the full 8-byte tuple.
    let decimal_debug: String = prefix
        .iter()
        .map(|b| b.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    // {:02x?} Debug — `format!("{:02x?}", &slice)` emits
    // `[de, ad, be, ef, ...]`.
    let hex_debug: String = prefix
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(", ");
    let spaced_hex: String = prefix
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
    let colon_hex: String = prefix
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":");
    vec![
        ("private-key lowercase hex", lowercase_hex),
        ("private-key uppercase hex", uppercase_hex),
        ("private-key decimal debug", decimal_debug),
        ("private-key hex debug ({:02x?})", hex_debug),
        ("private-key spaced hex", spaced_hex),
        ("private-key colon hex", colon_hex),
    ]
}

// ─── CaptureBuf MakeWriter + global subscriber ────────────────────────

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
    fn snapshot(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).expect("captured logs must be UTF-8")
    }
    fn clear(&self) {
        self.0.lock().unwrap().clear();
    }
}

/// Global state lives ONCE per test-binary process.  `set_global_default`
/// errors after the first call, so we install on first access via
/// `OnceLock` and share the resulting `CaptureBuf` across all tests.
static GLOBAL_CAPTURE: OnceLock<CaptureBuf> = OnceLock::new();

/// Test serial mutex.  Every async test acquires this before
/// clearing the shared buffer + driving paths so two tests never
/// interleave their emissions.  `tokio::sync::Mutex` (NOT std
/// `Mutex`) because the guard is held across `.await` points
/// (`yield_now`, `spawn_blocking().await`); a std `MutexGuard` held
/// across an await is `!Send` and clippy `-D warnings` flags it.
static TEST_SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn install_or_get_capture() -> CaptureBuf {
    GLOBAL_CAPTURE
        .get_or_init(|| {
            let buf = CaptureBuf::default();
            let subscriber = fmt()
                .with_writer(buf.clone())
                .with_max_level(tracing::Level::TRACE)
                .with_ansi(false)
                .finish();
            tracing::subscriber::set_global_default(subscriber)
                .expect("set_global_default must succeed once per process");
            buf
        })
        .clone()
}

/// RAII handle: acquires the serial mutex, returns a freshly-cleared
/// shared `CaptureBuf`, and releases the mutex on drop.  Tokio's
/// async mutex doesn't poison on panic — the next acquirer gets a
/// healthy lock automatically, so a single failing assertion can't
/// lock out the rest of the file.
async fn capture_for_test() -> (CaptureBuf, tokio::sync::MutexGuard<'static, ()>) {
    let guard = TEST_SERIAL.lock().await;
    let buf = install_or_get_capture();
    buf.clear();
    (buf, guard)
}

// ─── Substring assertion helpers ──────────────────────────────────────

fn assert_no_canary_leak(haystack: &str, context_label: &str) {
    let priv_needles = private_key_needles();
    let mut all_needles: Vec<(&str, &str)> = vec![
        ("seeded password", SEEDED_PASSWORD),
        ("seeded salt hex", SEEDED_SALT_HEX),
    ];
    for (label, n) in &priv_needles {
        all_needles.push((label, n));
    }
    for (name, needle) in all_needles {
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

fn drive_jks_unseal_path() -> CryptoError {
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
    futures::executor::block_on(provider.verify_dstu(b"<digest>", b"", b"<pubkey-bytes>"))
        .expect_err("0-byte sig must surface VerifyFailed::MalformedSignature")
}

// ─── Tests ────────────────────────────────────────────────────────────

#[tokio::test(flavor = "current_thread")]
async fn no_secret_substring_leaks_through_tracing_or_error_debug() {
    let (buf, _serial) = capture_for_test().await;

    // Drive every CryptoError variant.  Each format!() call also
    // checks immediately so a static leak surfaces with its
    // originating path in the diagnostic.
    let errors: Vec<CryptoError> = vec![
        drive_jks_unseal_path(),
        drive_cms_sign_path(),
        drive_unwrap_envelope_path(),
        drive_fetch_cert_path(),
        drive_verify_dstu_path(),
    ];

    // Yield to exercise the across-await behaviour explicitly.
    tokio::task::yield_now().await;

    for err in &errors {
        let dbg = format!("{err:?}");
        let disp = format!("{err}");
        assert_no_canary_leak(&dbg, "format!(\"{err:?}\")");
        assert_no_canary_leak(&disp, "format!(\"{err}\")");
    }

    // Happy-state Debug coverage (per W6 acceptance "every variant +
    // happy paths"): the secret-bearing types must REDACT secrets in
    // their Debug impls regardless of how the formatter is invoked.
    let session = SigningSession::new_for_test(
        "op-happy".into(),
        SEEDED_PRIVATE_KEY,
        b"<cert-der-not-real>".to_vec(),
    );
    let session_dbg = format!("{session:?}");
    assert_no_canary_leak(&session_dbg, "format!(\"{session:?}\")");

    let salt_bytes = hex_decode(SEEDED_SALT_HEX);
    let sealed = SealedMaterial {
        operator_id: "op-happy",
        jks_bytes: b"<jks-bytes-not-real>",
        jks_password_hex: SEEDED_SALT_HEX,
        cred_salt: &salt_bytes,
    };
    let sealed_dbg = format!("{sealed:?}");
    assert_no_canary_leak(&sealed_dbg, "format!(\"{sealed:?}\")");

    // Whatever events the paths emitted (none today; future
    // contributors might add tracing::error!(?) etc.) must not
    // contain any seeded-secret substring.
    let captured = buf.snapshot();
    assert_no_canary_leak(&captured, "captured tracing logs");
}

#[tokio::test(flavor = "current_thread")]
async fn cross_thread_capture_works_from_spawn_blocking() {
    // The HIGH finding: InProcessProvider runs sensitive paths via
    // `tokio::task::spawn_blocking`, which executes on tokio's
    // blocking thread pool (NOT the current_thread runtime).  A
    // `set_default`-only subscriber would silently miss any future
    // `tracing::error!(?session)` inside those closures.  This test
    // proves the global subscriber installed by `install_or_get_capture`
    // captures emissions from the blocking thread.
    let (buf, _serial) = capture_for_test().await;

    tokio::task::spawn_blocking(|| {
        tracing::info!(canary = "leaked-from-blocking-thread-3f7a");
    })
    .await
    .expect("spawn_blocking must succeed");

    let captured = buf.snapshot();
    assert!(
        captured.contains("leaked-from-blocking-thread-3f7a"),
        "global subscriber must capture spawn_blocking emissions \
         (captured {} bytes):\n  {captured}",
        captured.len()
    );
}

#[test]
fn each_canary_representation_is_actually_detected_by_assert_no_canary_leak() {
    // Positive detector: prove every needle representation we list
    // in `private_key_needles` is genuinely caught by
    // `assert_no_canary_leak`.  Without this, the main test would
    // silently miss any leak form whose needle string is malformed
    // (typo, wrong delimiter, etc.).
    //
    // For each form, build the leaked string from the seeded private
    // key in that exact encoding, then assert that
    // `assert_no_canary_leak` panics — i.e. the needle catches it.

    let prefix = &SEEDED_PRIVATE_KEY[..8];
    let cases: Vec<(&'static str, String)> = vec![
        (
            "lowercase hex",
            prefix.iter().map(|b| format!("{b:02x}")).collect(),
        ),
        (
            "uppercase hex",
            prefix.iter().map(|b| format!("{b:02X}")).collect(),
        ),
        // Default Rust Debug for `&[u8]` produces decimal-comma form.
        ("default Debug for &[u8]", format!("{prefix:?}")),
        // {:02x?} format spec produces hex-comma form (lowercase).
        ("{:02x?} Debug for &[u8]", format!("{prefix:02x?}")),
        (
            "spaced hex",
            prefix
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" "),
        ),
        (
            "colon hex",
            prefix
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(":"),
        ),
    ];
    for (label, leaked) in cases {
        let result = std::panic::catch_unwind(|| {
            assert_no_canary_leak(&leaked, "positive detector");
        });
        assert!(
            result.is_err(),
            "needle for `{label}` did not detect the leak: {leaked}"
        );
    }

    // Sanity: a string with NO seeded substrings must NOT trigger
    // (this guards against false positives — e.g. if a needle was
    // accidentally truncated to a common substring).
    let benign = "this is a perfectly innocent string with no canaries";
    assert_no_canary_leak(benign, "negative-control benign string");

    // Whole-key needle checks too — also seeded password + salt.
    let pw_leak = format!("password is {SEEDED_PASSWORD} btw");
    let result = std::panic::catch_unwind(|| {
        assert_no_canary_leak(&pw_leak, "positive detector");
    });
    assert!(result.is_err(), "password needle did not catch leak");

    let salt_leak = format!("salt is {SEEDED_SALT_HEX}");
    let result = std::panic::catch_unwind(|| {
        assert_no_canary_leak(&salt_leak, "positive detector");
    });
    assert!(result.is_err(), "salt needle did not catch leak");
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
