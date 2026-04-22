//! Detached DSTU-signed license — payload, JCS canonicalization, verify.
//!
//! Wire format:
//!   payload_b64  — base64-standard of UTF-8 JCS-canonical license JSON
//!   signature_b64 — base64-standard of 64-byte raw DSTU sig (r‖s, LE, 32+32)
//!
//! Public keys are embedded as raw 33-byte compressed DSTU PB-257 points.
//! On startup the sidecar calls verify_signature_only; per-request the
//! write-path calls verify (full: sig + expiry + TIN + FN).

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const PUBKEY_CURRENT: &[u8] = include_bytes!("license_pubkey_current.der");
const PUBKEY_NEXT:    &[u8] = include_bytes!("license_pubkey_next.der");

const GRACE_DAYS: i64 = 14;

// ─── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicensePayload {
    pub schema_version: String,
    pub tin:            String,
    pub fn_numbers:     Vec<String>,
    pub issued_at:      String,
    pub expires_at:     String,
    pub tier:           LicenseTier,
    pub org_name:       Option<String>,
    pub demo_limits:    Option<DemoLimits>,
    pub issuer:         String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoLimits {
    pub max_payment_sum_kopecks: i64,
    pub no_offline:              bool,
    pub require_dps_online:      bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LicenseTier { Demo, Basic, Pro, Enterprise }

#[derive(Debug, Clone)]
pub enum LicenseState {
    Valid,
    Grace   { days_left: i32 },
    Expired,
    Demo    { limits: DemoLimits },
    TinMismatch     { in_license: String, requested: String },
    FnNotLicensed   { fn_: String },
    SignatureInvalid,
}

#[derive(Debug, thiserror::Error)]
pub enum LicenseError {
    #[error("base64 decode: {0}")]
    Base64(String),
    #[error("JSON parse: {0}")]
    Json(#[from] serde_json::Error),
    #[error("date parse '{0}': {1}")]
    DateParse(String, String),
    #[error("demo tier is missing DemoLimits")]
    MissingDemoLimits,
}

impl From<base64::DecodeError> for LicenseError {
    fn from(e: base64::DecodeError) -> Self { Self::Base64(e.to_string()) }
}

// ─── JCS (RFC 8785) ───────────────────────────────────────────────────────────

impl LicensePayload {
    /// Return the JCS-canonical UTF-8 bytes used as the signature input.
    pub fn to_canonical_bytes(&self) -> Vec<u8> {
        let val = serde_json::to_value(self).expect("LicensePayload serializes");
        jcs_serialize(&val).into_bytes()
    }
}

/// Serialize a serde_json Value using RFC 8785 JCS rules:
///   - object keys sorted lexicographically by Unicode code point
///   - no insignificant whitespace
///   - numbers via serde_json's default (IEEE 754 shortest representation)
///   - strings via serde_json's default (Unicode escape for control chars)
fn jcs_serialize(val: &Value) -> String {
    match val {
        Value::Object(map) => {
            let mut pairs: Vec<(&str, &Value)> =
                map.iter().map(|(k, v)| (k.as_str(), v)).collect();
            pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
            let items: Vec<String> = pairs
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(k).expect("key serializes"),
                        jcs_serialize(v)
                    )
                })
                .collect();
            format!("{{{}}}", items.join(","))
        }
        Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(jcs_serialize).collect();
            format!("[{}]", items.join(","))
        }
        // Null, Bool, Number, String — serde_json already produces minimal JSON
        other => serde_json::to_string(other).expect("scalar serializes"),
    }
}

// ─── DSTU verification ────────────────────────────────────────────────────────

/// Verify a raw DSTU signature over `message` with a 33-byte compressed pubkey.
/// `sig_bytes` must be exactly 64 bytes: first 32 = r (LE), next 32 = s (LE).
fn verify_detached(pubkey_compressed: &[u8], message: &[u8], sig_bytes: &[u8]) -> bool {
    use prro_crypto::core::curve::Curve;
    use prro_crypto::core::field::FieldEl;
    use prro_crypto::core::hash::gost_34_311_95;
    use prro_crypto::core::point::expand_compressed_checked;
    use prro_crypto::core::sign::{verify, Signature};

    if sig_bytes.len() != 64 || pubkey_compressed.len() != 33 {
        return false;
    }

    let curve = Curve::dstu_pb_257();

    let pub_q = match expand_compressed_checked(pubkey_compressed, &curve) {
        Ok(q) => q,
        Err(_) => return false,
    };

    let hash_bytes = gost_34_311_95(message);
    let hash_fe = FieldEl::from_le_bytes(&hash_bytes, curve.mod_words);

    let r = FieldEl::from_le_bytes(&sig_bytes[..32], curve.mod_words);
    let s = FieldEl::from_le_bytes(&sig_bytes[32..], curve.mod_words);
    let sig = Signature { r, s };

    verify(&curve, &pub_q, &hash_fe, &sig)
}

// ─── Date helper ─────────────────────────────────────────────────────────────

fn parse_iso8601(s: &str) -> Result<time::OffsetDateTime, LicenseError> {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
        .map_err(|e| LicenseError::DateParse(s.to_string(), e.to_string()))
}

// ─── Core verify logic ────────────────────────────────────────────────────────

/// Internal: accepts injectable pubkey slice for testing.
fn verify_inner(
    payload_b64:   &str,
    signature_b64: &str,
    fn_:           Option<&str>,
    tin:           Option<&str>,
    now:           time::OffsetDateTime,
    pubkeys:       &[&[u8]],
) -> Result<LicenseState, LicenseError> {
    let payload_bytes = B64.decode(payload_b64)?;
    let sig_bytes     = B64.decode(signature_b64)?;

    let payload: LicensePayload = serde_json::from_slice(&payload_bytes)?;

    // 1. Signature: try each pubkey (current first, then next for rotation)
    let jcs = payload.to_canonical_bytes();
    let sig_ok = pubkeys.iter().any(|pk| verify_detached(pk, &jcs, &sig_bytes));
    if !sig_ok {
        return Ok(LicenseState::SignatureInvalid);
    }

    // 2. TIN / FN membership (full verify only)
    if let Some(tin) = tin {
        if payload.tin != tin {
            return Ok(LicenseState::TinMismatch {
                in_license: payload.tin,
                requested:  tin.into(),
            });
        }
    }
    if let Some(fn_) = fn_ {
        if !payload.fn_numbers.iter().any(|f| f == fn_) {
            return Ok(LicenseState::FnNotLicensed { fn_: fn_.into() });
        }
    }

    // 3. Demo tier: return caps immediately (no expiry logic for demo)
    if payload.tier == LicenseTier::Demo {
        let limits = payload.demo_limits.ok_or(LicenseError::MissingDemoLimits)?;
        return Ok(LicenseState::Demo { limits });
    }

    // 4. Expiry / grace
    let expires     = parse_iso8601(&payload.expires_at)?;
    let grace_start = expires - time::Duration::days(GRACE_DAYS);

    if now >= expires {
        return Ok(LicenseState::Expired);
    }
    if now >= grace_start {
        let days_left = (expires - now).whole_days() as i32;
        return Ok(LicenseState::Grace { days_left });
    }

    Ok(LicenseState::Valid)
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Called once at startup — panics immediately if the embedded pubkeys
/// are malformed (wrong length or not a valid DSTU PB-257 compressed point).
/// A 32-byte placeholder would silently make verify_detached always return false,
/// causing every license verification to report SignatureInvalid regardless of key.
pub fn check_embedded_pubkeys() {
    use prro_crypto::core::curve::Curve;
    use prro_crypto::core::point::expand_compressed_checked;

    let curve = Curve::dstu_pb_257();
    for (label, key) in [("current", PUBKEY_CURRENT), ("next", PUBKEY_NEXT)] {
        assert_eq!(
            key.len(), 33,
            "license_pubkey_{label}.der must be 33 bytes (DSTU PB-257 compressed point); got {}",
            key.len()
        );
        expand_compressed_checked(key, &curve)
            .unwrap_or_else(|e| panic!(
                "license_pubkey_{label}.der is not a valid DSTU PB-257 compressed point: {e}"
            ));
    }
}

/// Verify signature + expiry only. Used at startup and during install_license.
/// Does NOT check TIN or FN membership — those are checked per-request.
pub fn verify_signature_only(
    payload_b64:   &str,
    signature_b64: &str,
    now:           time::OffsetDateTime,
) -> Result<LicenseState, LicenseError> {
    verify_inner(payload_b64, signature_b64, None, None, now, &[PUBKEY_CURRENT, PUBKEY_NEXT])
}

/// Full per-request check: sig + expiry + TIN + FN membership + tier limits.
pub fn verify(
    payload_b64:   &str,
    signature_b64: &str,
    fn_:           &str,
    tin:           &str,
    now:           time::OffsetDateTime,
) -> Result<LicenseState, LicenseError> {
    verify_inner(payload_b64, signature_b64, Some(fn_), Some(tin), now, &[PUBKEY_CURRENT, PUBKEY_NEXT])
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use prro_crypto::core::curve::Curve;
    use prro_crypto::core::field::FieldEl;
    use prro_crypto::core::point::{compress_point, Point};
    use prro_crypto::core::sign::sign;
    use prro_crypto::core::hash::gost_34_311_95;
    use time::macros::datetime;

    // Fixed deterministic test keypair. Well-formed values chosen to be
    // < order and produce a non-degenerate signature with this e scalar.
    const TEST_D: &str = "1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF";
    const TEST_E: &str = "FEDCBA9876543210FEDCBA9876543210FEDCBA9876543210FEDCBA9876543210";

    /// Returns (private_key_le_bytes, compressed_pubkey_33bytes).
    fn test_keypair() -> (Vec<u8>, Vec<u8>) {
        let curve = Curve::dstu_pb_257();
        let d     = FieldEl::from_hex(TEST_D, curve.mod_words);
        let g     = Point::new(curve.base_x.clone(), curve.base_y.clone());
        let pub_q = g.mul(&d, &curve).negate(); // Q = -d·G (DSTU convention)
        (fe_to_le_bytes(&d, 32), compress_point(&pub_q, &curve))
    }

    fn fe_to_le_bytes(fe: &FieldEl, len: usize) -> Vec<u8> {
        let mut out = vec![0u8; len];
        for (wi, &word) in fe.bytes.iter().enumerate() {
            for bi in 0..4 {
                let idx = wi * 4 + bi;
                if idx < len { out[idx] = (word >> (bi * 8)) as u8; }
            }
        }
        out
    }

    /// Sign the JCS bytes of `payload` with the given private key bytes.
    fn sign_payload(payload: &LicensePayload, d_bytes: &[u8]) -> String {
        let curve    = Curve::dstu_pb_257();
        let d        = FieldEl::from_le_bytes(d_bytes, curve.mod_words);
        let jcs      = payload.to_canonical_bytes();
        let hash_raw = gost_34_311_95(&jcs);
        let hash_fe  = FieldEl::from_le_bytes(&hash_raw, curve.mod_words);
        let e        = FieldEl::from_hex(TEST_E, curve.mod_words);

        let sig = sign(&curve, &d, &hash_fe, &e)
            .expect("test scalar must produce a non-degenerate signature");

        let mut sig_bytes = fe_to_le_bytes(&sig.r, 32);
        sig_bytes.extend_from_slice(&fe_to_le_bytes(&sig.s, 32));
        B64.encode(&sig_bytes)
    }

    fn pro_payload(expires_at: &str) -> LicensePayload {
        LicensePayload {
            schema_version: "license.v1".into(),
            tin:            "1234567890".into(),
            fn_numbers:     vec!["3001234567".into(), "3001234568".into()],
            issued_at:      "2026-01-01T00:00:00Z".into(),
            expires_at:     expires_at.into(),
            tier:           LicenseTier::Pro,
            org_name:       Some("ТОВ Тест".into()),
            demo_limits:    None,
            issuer:         "PRRO_GATE_ADMIN".into(),
        }
    }

    fn demo_payload() -> LicensePayload {
        LicensePayload {
            schema_version: "license.v1".into(),
            tin:            "1234567890".into(),
            fn_numbers:     vec!["3001234567".into()],
            issued_at:      "2026-01-01T00:00:00Z".into(),
            expires_at:     "2099-01-01T00:00:00Z".into(),
            tier:           LicenseTier::Demo,
            org_name:       None,
            demo_limits:    Some(DemoLimits {
                max_payment_sum_kopecks: 50_000,
                no_offline:             true,
                require_dps_online:     true,
            }),
            issuer: "PRRO_GATE_ADMIN".into(),
        }
    }

    // ── helpers that drive verify_inner ─────────────────────────────────────

    fn do_verify(
        payload: &LicensePayload,
        d_bytes: &[u8],
        pub_compressed: &[u8],
        fn_: Option<&str>,
        tin: Option<&str>,
        now: time::OffsetDateTime,
    ) -> LicenseState {
        let p_b64  = B64.encode(payload.to_canonical_bytes());
        let s_b64  = sign_payload(payload, d_bytes);
        verify_inner(&p_b64, &s_b64, fn_, tin, now, &[pub_compressed])
            .expect("verify_inner must not error on well-formed input")
    }

    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn valid_pro_within_expiry() {
        let (d, pub_k) = test_keypair();
        let payload = pro_payload("2027-04-19T00:00:00Z");
        let now     = datetime!(2026-04-19 12:00:00 UTC);
        let state   = do_verify(&payload, &d, &pub_k, Some("3001234567"), Some("1234567890"), now);
        assert!(matches!(state, LicenseState::Valid), "expected Valid, got {state:?}");
    }

    #[test]
    fn grace_window_13d() {
        let (d, pub_k) = test_keypair();
        let payload = pro_payload("2026-05-02T00:00:00Z");   // expires in 13d
        let now     = datetime!(2026-04-19 12:00:00 UTC);    // 13d before expiry
        let state   = do_verify(&payload, &d, &pub_k, Some("3001234567"), Some("1234567890"), now);
        assert!(
            matches!(state, LicenseState::Grace { days_left } if days_left >= 12 && days_left <= 13),
            "expected Grace(12..13), got {state:?}"
        );
    }

    #[test]
    fn grace_window_1d() {
        let (d, pub_k) = test_keypair();
        let payload = pro_payload("2026-04-20T12:00:00Z");   // expires tomorrow
        let now     = datetime!(2026-04-19 12:00:00 UTC);
        let state   = do_verify(&payload, &d, &pub_k, Some("3001234567"), Some("1234567890"), now);
        assert!(
            matches!(state, LicenseState::Grace { days_left: 1 }),
            "expected Grace(1), got {state:?}"
        );
    }

    #[test]
    fn expired_1d_ago() {
        let (d, pub_k) = test_keypair();
        let payload = pro_payload("2026-04-18T00:00:00Z");
        let now     = datetime!(2026-04-19 12:00:00 UTC);
        let state   = do_verify(&payload, &d, &pub_k, Some("3001234567"), Some("1234567890"), now);
        assert!(matches!(state, LicenseState::Expired), "expected Expired, got {state:?}");
    }

    #[test]
    fn tin_mismatch() {
        let (d, pub_k) = test_keypair();
        let payload = pro_payload("2027-04-19T00:00:00Z");
        let now     = datetime!(2026-04-19 12:00:00 UTC);
        let state   = do_verify(&payload, &d, &pub_k, Some("3001234567"), Some("9999999999"), now);
        assert!(
            matches!(state, LicenseState::TinMismatch { ref requested, .. } if requested == "9999999999"),
            "expected TinMismatch, got {state:?}"
        );
    }

    #[test]
    fn fn_not_in_list() {
        let (d, pub_k) = test_keypair();
        let payload = pro_payload("2027-04-19T00:00:00Z");
        let now     = datetime!(2026-04-19 12:00:00 UTC);
        let state   = do_verify(&payload, &d, &pub_k, Some("9999999999"), Some("1234567890"), now);
        assert!(
            matches!(state, LicenseState::FnNotLicensed { ref fn_ } if fn_ == "9999999999"),
            "expected FnNotLicensed, got {state:?}"
        );
    }

    #[test]
    fn signature_tampered() {
        let (d, pub_k)  = test_keypair();
        let payload = pro_payload("2027-04-19T00:00:00Z");
        let p_b64   = B64.encode(payload.to_canonical_bytes());
        let mut sig_bytes = B64.decode(sign_payload(&payload, &d)).unwrap();
        sig_bytes[0] ^= 0xFF; // flip first byte of r
        let bad_sig = B64.encode(&sig_bytes);
        let state = verify_inner(&p_b64, &bad_sig, None, None,
                                  datetime!(2026-04-19 12:00:00 UTC), &[&pub_k])
            .unwrap();
        assert!(matches!(state, LicenseState::SignatureInvalid),
                "expected SignatureInvalid, got {state:?}");
    }

    #[test]
    fn demo_valid() {
        let (d, pub_k) = test_keypair();
        let payload = demo_payload();
        let now     = datetime!(2026-04-19 12:00:00 UTC);
        let state   = do_verify(&payload, &d, &pub_k, None, None, now);
        assert!(
            matches!(state, LicenseState::Demo { limits: DemoLimits { max_payment_sum_kopecks: 50_000, .. } }),
            "expected Demo, got {state:?}"
        );
    }

    #[test]
    fn demo_without_limits_errors() {
        let (d, pub_k) = test_keypair();
        let mut payload = demo_payload();
        payload.demo_limits = None;  // invalid: demo tier but no limits
        let p_b64 = B64.encode(payload.to_canonical_bytes());
        let s_b64 = sign_payload(&payload, &d);
        let result = verify_inner(&p_b64, &s_b64, None, None,
                                   datetime!(2026-04-19 12:00:00 UTC), &[&pub_k]);
        assert!(
            matches!(result, Err(LicenseError::MissingDemoLimits)),
            "expected MissingDemoLimits error, got {result:?}"
        );
    }

    #[test]
    fn jcs_key_order_stable_across_serializations() {
        let payload1 = pro_payload("2027-04-19T00:00:00Z");
        let payload2 = pro_payload("2027-04-19T00:00:00Z");
        assert_eq!(
            payload1.to_canonical_bytes(),
            payload2.to_canonical_bytes(),
            "JCS must be deterministic"
        );
        // Check actual key ordering — schema_version < expires_at < fn_numbers etc.
        let jcs = String::from_utf8(payload1.to_canonical_bytes()).unwrap();
        let demo_jcs = String::from_utf8(demo_payload().to_canonical_bytes()).unwrap();
        let first_key = |s: &str| s.chars().skip(1).take_while(|c| *c != '"').collect::<String>();
        // Keys sorted lex: "demo_limits" < "expires_at" < "fn_numbers" < "issued_at" ...
        // First key must be "demo_limits" in demo payload, "expires_at" alphabetically
        assert!(demo_jcs.starts_with(r#"{"demo_limits":"#), "demo_limits must sort first: {demo_jcs}");
        let _ = (jcs, first_key); // suppress unused warnings
    }

    #[test]
    fn jcs_unicode_chars_preserved() {
        let mut payload = pro_payload("2027-04-19T00:00:00Z");
        payload.org_name = Some("ТОВ «Магазин» 商店".into());
        let jcs = String::from_utf8(payload.to_canonical_bytes()).unwrap();
        assert!(jcs.contains("ТОВ «Магазин» 商店"), "Unicode must not be percent-escaped in JCS");
    }

    #[test]
    fn grace_boundary_exactly_14_days_before_expiry() {
        // now = expires_at - 14 days exactly → should be in Grace period (not Valid)
        // Grace window: [expires_at - 14d, expires_at)
        let (d, pub_k) = test_keypair();
        let expires_at = datetime!(2026-05-03 00:00:00 UTC); // 14 days after 2026-04-19
        let payload = pro_payload("2026-05-03T00:00:00Z");
        let now      = datetime!(2026-04-19 00:00:00 UTC); // exactly 14d before expiry
        let state    = do_verify(&payload, &d, &pub_k, Some("3001234567"), Some("1234567890"), now);
        assert!(
            matches!(state, LicenseState::Grace { days_left: 14 }),
            "exactly 14 days before expiry must be Grace(14), got {state:?}"
        );
        let _ = expires_at;
    }

    #[test]
    fn grace_boundary_15_days_before_expiry_is_still_valid() {
        // 15 days before expiry is before grace window → Valid
        let (d, pub_k) = test_keypair();
        let payload = pro_payload("2026-05-04T00:00:00Z"); // 15 days after 2026-04-19
        let now     = datetime!(2026-04-19 00:00:00 UTC);
        let state   = do_verify(&payload, &d, &pub_k, Some("3001234567"), Some("1234567890"), now);
        assert!(
            matches!(state, LicenseState::Valid),
            "15 days before expiry must be Valid, got {state:?}"
        );
    }

    #[test]
    fn expired_at_exact_expiry_timestamp() {
        // now == expires_at exactly → condition is now >= expires → Expired
        let (d, pub_k) = test_keypair();
        let payload = pro_payload("2026-04-19T12:00:00Z");
        let now     = datetime!(2026-04-19 12:00:00 UTC); // exactly at expiry
        let state   = do_verify(&payload, &d, &pub_k, Some("3001234567"), Some("1234567890"), now);
        assert!(
            matches!(state, LicenseState::Expired),
            "now == expires_at must be Expired (>= boundary), got {state:?}"
        );
    }

    #[test]
    fn expired_one_second_before_exact_timestamp_is_grace() {
        // One second before expiry → still Grace (just barely)
        let (d, pub_k) = test_keypair();
        let payload = pro_payload("2026-04-19T12:00:00Z");
        let now     = datetime!(2026-04-19 11:59:59 UTC);
        let state   = do_verify(&payload, &d, &pub_k, Some("3001234567"), Some("1234567890"), now);
        assert!(
            matches!(state, LicenseState::Grace { .. }),
            "1 second before expiry must be Grace, got {state:?}"
        );
    }

    #[test]
    fn invalid_expires_at_returns_date_parse_error() {
        // Malformed expires_at should surface as LicenseError::DateParse
        let (d, pub_k) = test_keypair();
        let mut payload = pro_payload("2026-05-01T00:00:00Z");
        payload.expires_at = "2026-13-45T00:00:00Z".into(); // month 13 is invalid

        let p_b64 = B64.encode(payload.to_canonical_bytes());
        let s_b64 = sign_payload(&payload, &d);
        let result = verify_inner(&p_b64, &s_b64, None, None,
                                   datetime!(2026-04-19 00:00:00 UTC), &[&pub_k]);
        assert!(
            matches!(result, Err(LicenseError::DateParse(ref s, _)) if s.contains("2026-13-45")),
            "invalid month in expires_at must produce DateParse, got {result:?}"
        );
    }

    #[test]
    fn empty_fn_numbers_makes_any_fn_not_licensed() {
        // payload.fn_numbers = [] — no FN is authorized
        let (d, pub_k) = test_keypair();
        let mut payload = pro_payload("2027-04-19T00:00:00Z");
        payload.fn_numbers = vec![];

        let now   = datetime!(2026-04-19 12:00:00 UTC);
        let state = do_verify(&payload, &d, &pub_k, Some("3001234567"), Some("1234567890"), now);
        assert!(
            matches!(state, LicenseState::FnNotLicensed { ref fn_ } if fn_ == "3001234567"),
            "empty fn_numbers must return FnNotLicensed, got {state:?}"
        );
    }

    #[test]
    fn multiple_fn_numbers_match_any_in_list() {
        // FN must be found by .any() — check the LAST element is also matched
        let (d, pub_k) = test_keypair();
        let mut payload = pro_payload("2027-04-19T00:00:00Z");
        payload.fn_numbers = vec!["3001234567".into(), "3001234568".into(), "9999999999".into()];

        let now   = datetime!(2026-04-19 12:00:00 UTC);
        let state = do_verify(&payload, &d, &pub_k, Some("9999999999"), Some("1234567890"), now);
        assert!(
            matches!(state, LicenseState::Valid),
            "last element in fn_numbers must still match, got {state:?}"
        );
    }

    #[test]
    fn signature_too_short_returns_invalid() {
        // sig_bytes.len() != 64 → verify_detached returns false → SignatureInvalid
        let (d, pub_k) = test_keypair();
        let payload = pro_payload("2027-04-19T00:00:00Z");
        let p_b64   = B64.encode(payload.to_canonical_bytes());
        // 32 bytes (half a signature) — wrong length
        let short_sig = B64.encode(&[0xAB_u8; 32]);
        let state = verify_inner(&p_b64, &short_sig, None, None,
                                  datetime!(2026-04-19 12:00:00 UTC), &[&pub_k])
            .unwrap();
        assert!(
            matches!(state, LicenseState::SignatureInvalid),
            "32-byte sig (should be 64) must return SignatureInvalid, got {state:?}"
        );
        let _ = d;
    }

    #[test]
    fn pubkey_wrong_length_returns_invalid() {
        // pubkey_compressed.len() != 33 → verify_detached returns false → SignatureInvalid
        let (d, _) = test_keypair();
        let wrong_pubkey = vec![0x04_u8; 65]; // uncompressed point, not 33-byte compressed

        let payload = pro_payload("2027-04-19T00:00:00Z");
        let p_b64   = B64.encode(payload.to_canonical_bytes());
        let s_b64   = sign_payload(&payload, &d);
        let state   = verify_inner(&p_b64, &s_b64, None, None,
                                    datetime!(2026-04-19 12:00:00 UTC), &[&wrong_pubkey])
            .unwrap();
        assert!(
            matches!(state, LicenseState::SignatureInvalid),
            "65-byte pubkey (should be 33) must return SignatureInvalid, got {state:?}"
        );
    }

    #[test]
    fn payload_invalid_base64_returns_error() {
        // Base64 decode error surfaces as LicenseError::Base64
        let result = verify_inner("not!!valid!!base64===", "AAAA", None, None,
                                   datetime!(2026-04-19 12:00:00 UTC), &[&[0u8; 33]]);
        assert!(
            matches!(result, Err(LicenseError::Base64(_))),
            "invalid payload base64 must return Base64 error, got {result:?}"
        );
    }

    #[test]
    fn signature_invalid_base64_returns_error() {
        let (_, pub_k) = test_keypair();
        // Valid payload b64 (just needs to decode to valid JSON)
        let payload = pro_payload("2027-04-19T00:00:00Z");
        let p_b64   = B64.encode(payload.to_canonical_bytes());
        let result  = verify_inner(&p_b64, "!!!not_base64===", None, None,
                                    datetime!(2026-04-19 12:00:00 UTC), &[&pub_k]);
        assert!(
            matches!(result, Err(LicenseError::Base64(_))),
            "invalid signature base64 must return Base64 error, got {result:?}"
        );
    }

    #[test]
    fn tin_mismatch_stores_correct_fields() {
        // in_license = what the license says; requested = what the caller passed
        let (d, pub_k) = test_keypair();
        let payload = pro_payload("2027-04-19T00:00:00Z"); // tin = "1234567890"
        let now     = datetime!(2026-04-19 12:00:00 UTC);
        let state   = do_verify(&payload, &d, &pub_k, Some("3001234567"), Some("9999999999"), now);
        match state {
            LicenseState::TinMismatch { ref in_license, ref requested } => {
                assert_eq!(in_license, "1234567890", "in_license must be the payload TIN");
                assert_eq!(requested,  "9999999999", "requested must be the caller-supplied TIN");
            }
            other => panic!("expected TinMismatch, got {other:?}"),
        }
    }

    #[test]
    fn basic_and_enterprise_tiers_parse_and_verify() {
        let (d, pub_k) = test_keypair();
        let now = datetime!(2026-04-19 12:00:00 UTC);
        for tier in [LicenseTier::Basic, LicenseTier::Enterprise] {
            let mut payload = pro_payload("2027-04-19T00:00:00Z");
            payload.tier = tier;
            let state = do_verify(&payload, &d, &pub_k, Some("3001234567"), Some("1234567890"), now);
            assert!(
                matches!(state, LicenseState::Valid),
                "tier {tier:?} must produce Valid within expiry, got {state:?}"
            );
        }
    }
}
