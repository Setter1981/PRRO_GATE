//! W2 PR-B piece 4 — `runtime::coding::Coding` obfuscation roundtrip.
//!
//! Per `docs/superpowers/plans/2026-05-25-m4-ingress-plan.md` §3 W2
//! ("Coding helper" bullet) and MED-PR90-02 acceptance: the helper is
//! a **symmetric obfuscation** of cashier-key passwords matching
//! WebCheck's `Coding().Cod()` discipline.  It is explicitly NOT
//! cryptography — its threat model is "protect against casual file
//! inspection" (the secure DB is already chmod 0o600 and physically
//! isolated per HIGH-AUDIT-01).  An attacker with DB read access can
//! trivially reverse it; preventing that is the job of the secure DB
//! file mode, not this helper.
//!
//! Three contracts:
//!
//!   1. Bijective roundtrip on every byte value 0..=255 — passwords
//!      may contain non-UTF8 bytes (`.dat`/`.jks` carrier formats),
//!      so the helper must NOT assume ASCII or UTF-8.
//!   2. ASCII roundtrip on a representative cashier password
//!      (smoke / readability check).
//!   3. Empty input is a typed error, NOT a silent no-op — storing
//!      an empty `key_pass_enc` BLOB is meaningless and the repository
//!      column is `NOT NULL`.

use prro::runtime::coding::{Coding, CodingError};

#[test]
fn roundtrip_all_byte_values_is_bijective() {
    let plain: Vec<u8> = (0u8..=255).collect();
    let encoded = Coding::encode(&plain).expect("encode 0..=255 must succeed");
    assert_ne!(
        encoded, plain,
        "obfuscation must change at least one byte (output != input)"
    );
    let decoded = Coding::decode(&encoded).expect("decode round-trips");
    assert_eq!(
        &decoded[..],
        &plain[..],
        "decode(encode(x)) == x for full byte range"
    );
}

#[test]
fn roundtrip_typical_ascii_password() {
    let plain = b"cashier-secret-1234";
    let encoded = Coding::encode(plain).expect("encode ASCII");
    let decoded = Coding::decode(&encoded).expect("decode ASCII");
    assert_eq!(&decoded[..], plain);
}

#[test]
fn encode_empty_input_returns_typed_error() {
    let err = Coding::encode(&[]).expect_err("encode of empty must be typed error");
    assert!(matches!(err, CodingError::EmptyInput));
}

#[test]
fn decode_empty_input_returns_typed_error() {
    let err = Coding::decode(&[]).expect_err("decode of empty must be typed error");
    assert!(matches!(err, CodingError::EmptyInput));
}

#[test]
fn encode_produces_non_empty_output_for_non_empty_input() {
    let plain = b"x";
    let encoded = Coding::encode(plain).expect("encode single byte");
    assert!(
        !encoded.is_empty(),
        "non-empty input must produce non-empty output"
    );
    assert_eq!(encoded.len(), plain.len(), "length is preserved");
}
