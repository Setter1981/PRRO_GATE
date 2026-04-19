//! JKS password obfuscation — XorSoft (default) and Plain backends.
//!
//! XorSoft: key = SHA-256(cert_valid_to + operator_name[1])
//!          stored = hex(XOR(password_bytes, key[i % 32]))
//! Plain:   raw password string — opt-out for WebCheck migration / debug.
//!
//! Phase 4.4 implements full logic; this is a Phase 0 stub.

#![allow(dead_code)]

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
}
