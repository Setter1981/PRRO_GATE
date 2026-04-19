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
}
