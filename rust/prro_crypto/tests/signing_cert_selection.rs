//! `ExtractedKey::signing_cert()` must pick the KeyUsage=digitalSignature cert,
//! NOT certs[0] — a UA EDS keystore holds both a signing and a key-agreement
//! (encryption) cert, and embedding the encryption cert makes DPS reject the
//! signature `CryptBadSign` (confirmed live 2026-05-29). Integration test
//! because prro_crypto's in-crate `#[cfg(test)]` lib-test is blocked by a
//! separate node_modules fixture.

use prro_crypto::interop::prro::containers::{ContainerFormat, ExtractedKey};
use zeroize::Zeroizing;

/// A minimal DER blob carrying ONLY a KeyUsage extension (OID 2.5.29.15)
/// with the given BIT STRING content — enough for the KeyUsage scanner.
/// `ku` is the BIT STRING value `03 02 <unused> <bits>`.
fn cert_with_keyusage(ku_bits: &[u8]) -> Vec<u8> {
    // 06 03 55 1D 0F  (OID KeyUsage)  04 <len> <BIT STRING>
    let mut v = vec![0x30, 0x00]; // dummy outer SEQUENCE header (scanner ignores)
    v.extend_from_slice(&[0x06, 0x03, 0x55, 0x1D, 0x0F]); // OID 2.5.29.15
    v.push(0x04); // extnValue OCTET STRING
    v.push(ku_bits.len() as u8);
    v.extend_from_slice(ku_bits); // BIT STRING 03 02 <unused> <bits>
    v
}

const KU_DIGITAL_SIGNATURE: &[u8] = &[0x03, 0x02, 0x06, 0xC0]; // bit0+bit1 (digitalSignature+nonRepudiation)
const KU_KEY_AGREEMENT: &[u8] = &[0x03, 0x02, 0x03, 0x08]; // bit4 (keyAgreement)

fn mk(certs: Vec<Vec<u8>>) -> ExtractedKey {
    ExtractedKey {
        format: ContainerFormat::Jks,
        param_d: Zeroizing::new([0u8; 32]),
        certs,
    }
}

#[test]
fn picks_signing_cert_not_first_encryption_cert() {
    // Mirror the real ГАЛЬЧУН JKS order: [0]=encryption, [1]=signing.
    let enc = cert_with_keyusage(KU_KEY_AGREEMENT);
    let sig = cert_with_keyusage(KU_DIGITAL_SIGNATURE);
    let ek = mk(vec![enc.clone(), sig.clone()]);
    assert_eq!(
        ek.signing_cert().unwrap(),
        sig.as_slice(),
        "must select the digitalSignature cert, not certs[0] (the encryption cert)"
    );
}

#[test]
fn picks_signing_cert_regardless_of_position() {
    let sig = cert_with_keyusage(KU_DIGITAL_SIGNATURE);
    let enc = cert_with_keyusage(KU_KEY_AGREEMENT);
    let ek = mk(vec![sig.clone(), enc]);
    assert_eq!(ek.signing_cert().unwrap(), sig.as_slice());
}

#[test]
fn returns_none_when_no_digitalsignature() {
    // No cert declares digitalSignature -> NO certs[0] fallback (RS-1 F1):
    // `None`, so the caller fails closed instead of embedding the wrong cert.
    let a = cert_with_keyusage(KU_KEY_AGREEMENT);
    let b = cert_with_keyusage(&[0x03, 0x02, 0x01, 0x06]); // keyCertSign-ish
    let ek = mk(vec![a, b]);
    assert!(
        ek.signing_cert().is_none(),
        "no digitalSignature cert -> None (no certs[0] fallback; CryptBadSign avoidance)"
    );
}

#[test]
fn none_for_empty() {
    assert!(mk(vec![]).signing_cert().is_none());
}
