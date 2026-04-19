//! Detached DSTU-signed license — payload, JCS canonicalization, verify.
//! Master pubkeys embedded via include_bytes! (current + next rotation slot).
//! Phase 3.1 implements full logic; this is a Phase 0 stub.

#![allow(dead_code)]

const PUBKEY_CURRENT: &[u8] = include_bytes!("license_pubkey_current.der");
const PUBKEY_NEXT:    &[u8] = include_bytes!("license_pubkey_next.der");

#[derive(Debug, Clone)]
pub enum LicenseState {
    Valid,
    Grace { days_left: i32 },
    Expired,
    Demo,
    TinMismatch,
    FnNotLicensed,
    SignatureInvalid,
}

pub fn verify_signature_only(
    _payload_b64: &str,
    _signature_b64: &str,
    _now: time::OffsetDateTime,
) -> Result<LicenseState, String> {
    unimplemented!("license::verify_signature_only: Phase 3 stub")
}

pub fn verify(
    _payload_b64: &str,
    _signature_b64: &str,
    _fn_: &str,
    _tin: &str,
    _now: time::OffsetDateTime,
) -> Result<LicenseState, String> {
    unimplemented!("license::verify: Phase 3 stub")
}
