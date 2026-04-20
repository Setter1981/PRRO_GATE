//! JKS password obfuscation — XorSoft (default) and Plain backends.
//!
//! XorSoft: key = SHA-256(cert_valid_to + operator_name[1])
//!          stored = hex(XOR(password_bytes, key[i % 32]))
//! Plain:   raw password string — opt-out for WebCheck migration / debug.
//!
//! XOR-soft is obfuscation, not encryption. The salt entropy is one Unicode
//! character from position 1 of the operator name plus the cert expiry date.
//! Two operators sharing the same second character and cert expiry produce the
//! same key — acceptable for obfuscation, not for secrecy guarantees.

use sha2::{Digest, Sha256};

fn derive_key(valid_to: &str, operator_name: &str) -> [u8; 32] {
    let c = operator_name.chars().nth(1).unwrap_or('?');
    let salt = format!("{}{}", valid_to, c);
    Sha256::digest(salt.as_bytes()).into()
}

pub fn encode_password(password: &str, valid_to: &str, operator_name: &str) -> String {
    let key = derive_key(valid_to, operator_name);
    let encoded: Vec<u8> = password
        .as_bytes()
        .iter()
        .enumerate()
        .map(|(i, b)| b ^ key[i % 32])
        .collect();
    hex::encode(encoded)
}

pub fn decode_password(
    hex_str: &str,
    valid_to: &str,
    operator_name: &str,
) -> Result<String, String> {
    let bytes = hex::decode(hex_str).map_err(|e| format!("hex decode: {e}"))?;
    let key = derive_key(valid_to, operator_name);
    let decoded: Vec<u8> = bytes
        .iter()
        .enumerate()
        .map(|(i, b)| b ^ key[i % 32])
        .collect();
    String::from_utf8(decoded).map_err(|e| format!("utf8: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_xor_soft() {
        let pw = "MySecret123!";
        let vt = "2026-12-31";
        let name = "Сідоренко";
        let encoded = encode_password(pw, vt, name);
        assert_ne!(encoded, pw);
        let decoded = decode_password(&encoded, vt, name).unwrap();
        assert_eq!(decoded, pw);
    }

    #[test]
    fn different_names_different_ciphertext() {
        let pw = "pass";
        let vt = "2026-12-31";
        let a = encode_password(pw, vt, "Антоненко");
        let b = encode_password(pw, vt, "Бойченко");
        assert_ne!(a, b);
    }

    #[test]
    fn decode_invalid_hex_returns_err() {
        let err = decode_password("not!!hex", "2026-12-31", "Тест").unwrap_err();
        assert!(err.contains("hex decode"), "expected 'hex decode' in: {err}");
    }

    #[test]
    fn encode_empty_password_produces_empty_hex() {
        let encoded = encode_password("", "2026-12-31", "Тест");
        assert_eq!(encoded, "", "empty password → empty hex");
        let decoded = decode_password(&encoded, "2026-12-31", "Тест").unwrap();
        assert_eq!(decoded, "");
    }

    #[test]
    fn encode_password_longer_than_32_bytes_cycles_key() {
        // 64-char password — key is 32 bytes, so key[i % 32] cycles twice.
        let pw = "абвгдеєжзиіїйклмнопрстуфхцчшщьюя"; // 32 Ukrainian chars = 64 UTF-8 bytes
        let vt = "2026-12-31";
        let name = "Тестовий";
        let encoded = encode_password(pw, vt, name);
        // hex length must equal 2 × byte-length of password
        assert_eq!(encoded.len(), pw.len() * 2);
        let decoded = decode_password(&encoded, vt, name).unwrap();
        assert_eq!(decoded, pw);
    }

    #[test]
    fn short_operator_name_uses_fallback_char() {
        // Names with 0 or 1 chars → nth(1) returns None → '?' used as salt char.
        // Both produce the same key, hence the same ciphertext.
        let pw = "secret";
        let vt = "2026-12-31";
        let empty = encode_password(pw, vt, "");
        let one   = encode_password(pw, vt, "А");
        assert_eq!(empty, one, "both use '?' fallback → identical key → identical ciphertext");

        // Two-char name uses the actual second char → different key.
        let two = encode_password(pw, vt, "АБ");
        assert_ne!(empty, two, "second char 'Б' changes the key");
    }

    #[test]
    fn different_valid_to_different_ciphertext() {
        let pw = "pass";
        let name = "Сідоренко";
        let a = encode_password(pw, "2026-01-01", name);
        let b = encode_password(pw, "2027-06-30", name);
        assert_ne!(a, b, "different cert expiry → different key → different ciphertext");
    }

    #[test]
    fn encoding_is_deterministic() {
        // Same inputs must always produce the same hex — no random component.
        let pw = "MyPass";
        let vt = "2026-12-31";
        let name = "Іваненко";
        assert_eq!(
            encode_password(pw, vt, name),
            encode_password(pw, vt, name),
        );
    }

    #[test]
    fn decode_xor_result_non_utf8_returns_error() {
        // XOR key byte 0 = SHA256("2026-12-31?")[0]
        // We need a password whose first byte XOR key[0] produces a non-UTF8 byte.
        // Strategy: encode the byte 0xFF as the first password byte, then check if decode fails.
        // derive_key("2026-12-31", "") → sha256("2026-12-31?")
        let vt = "2026-12-31";
        let name = ""; // operator_name: uses '?' fallback char
        let key = {
            use sha2::{Digest, Sha256};
            let salt = format!("{}?", vt);
            let hash: [u8; 32] = Sha256::digest(salt.as_bytes()).into();
            hash
        };
        // Craft a hex string whose first decoded byte XOR key[0] == 0x80 (invalid UTF-8 start byte)
        // key[0] XOR target[0] = 0x80 → target[0] = key[0] ^ 0x80
        let bad_byte = key[0] ^ 0x80;
        // Use a 1-byte "password" that decodes to 0x80 (invalid UTF-8 continuation byte alone)
        let hex_str = hex::encode([bad_byte]);
        let result = decode_password(&hex_str, vt, name);
        assert!(
            result.is_err(),
            "XOR result producing 0x80 (lone continuation byte) must fail UTF-8 validation, got {result:?}"
        );
        assert!(
            result.unwrap_err().contains("utf8"),
            "error message must mention utf8"
        );
    }

    #[test]
    fn decode_hex_uppercase_accepted() {
        // hex crate decodes both upper and lowercase hex. Verify we accept uppercase output
        // from external tools (e.g. if stored in DB as uppercase).
        let pw   = "TestPass";
        let vt   = "2026-12-31";
        let name = "Тестовий";
        let encoded_lower = encode_password(pw, vt, name); // returns lowercase hex
        let encoded_upper = encoded_lower.to_uppercase();  // simulate DB storing uppercase
        let decoded = decode_password(&encoded_upper, vt, name)
            .expect("uppercase hex must decode correctly");
        assert_eq!(decoded, pw, "uppercase hex must roundtrip to original password");
    }

    #[test]
    fn encode_password_single_unicode_char() {
        // Single-char operator name: nth(1) returns None → '?' used as fallback.
        // Single UTF-8 multi-byte char: "А" is 2 bytes but 1 char — nth(1) returns None.
        let pw   = "secret";
        let vt   = "2026-12-31";
        let name = "А"; // 1 char, 2 UTF-8 bytes

        let encoded = encode_password(pw, vt, name);
        let decoded = decode_password(&encoded, vt, name).unwrap();
        assert_eq!(decoded, pw, "single-char name roundtrip must succeed");

        // Must match empty-name encoding (both use '?' fallback)
        let empty_encoded = encode_password(pw, vt, "");
        assert_eq!(
            encoded, empty_encoded,
            "single-char name uses same '?' fallback as empty name"
        );
    }

    #[test]
    fn encode_with_null_byte_in_password_roundtrips() {
        // Rust strings allow null bytes — verify XOR handles them correctly.
        let pw   = "pass\x00word"; // contains null byte
        let vt   = "2026-12-31";
        let name = "Тест";
        let encoded = encode_password(pw, vt, name);
        let decoded = decode_password(&encoded, vt, name)
            .expect("null byte in password must roundtrip");
        assert_eq!(decoded, pw, "null byte in password must survive encode/decode");
    }

    #[test]
    fn encode_all_ascii_printable_characters_roundtrip() {
        // Stress-test with full printable ASCII range
        let pw: String = (0x20u8..=0x7E).map(|b| b as char).collect();
        let vt   = "2026-12-31";
        let name = "Оператор";
        let encoded = encode_password(&pw, vt, name);
        let decoded = decode_password(&encoded, vt, name)
            .expect("full printable ASCII must roundtrip");
        assert_eq!(decoded, pw);
    }

    #[test]
    fn decode_odd_length_hex_returns_error() {
        // hex::decode requires even-length string (each pair = 1 byte)
        let result = decode_password("abc", "2026-12-31", "Тест");
        assert!(result.is_err(), "odd-length hex must fail");
        assert!(result.unwrap_err().contains("hex decode"));
    }
}
