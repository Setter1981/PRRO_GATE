//! W2 — symmetric obfuscation for cashier-key passwords.
//!
//! **NOT cryptography.**  This helper is XOR-with-constant obfuscation
//! matching the WebCheck `Coding().Cod()` discipline.  Its threat
//! model is "protect against casual file inspection" — the secure DB
//! is already chmod 0o600 and isolated to its own physical file per
//! HIGH-AUDIT-01.  An attacker with DB read access can trivially
//! reverse the XOR; preventing THAT is the job of the file mode +
//! filesystem permissions, not this helper.
//!
//! Properties:
//!
//!   - Symmetric: `decode(encode(x)) == x` and `encode(decode(x)) == x`.
//!   - Length-preserving: `encode(x).len() == x.len()`.
//!   - Bijective on the full byte space 0..=255: passwords may carry
//!     non-UTF8 bytes via `.dat` / `.jks` carrier formats, so callers
//!     MUST NOT assume ASCII / UTF-8 input.
//!
//! Anti-properties (DO NOT rely on these for security):
//!
//!   - The MASK constant is NOT a secret.  Known-plaintext attack
//!     recovers it instantly.  Don't pretend the BLOB is encrypted.
//!   - No authentication; tampered BLOBs decode to garbage but the
//!     repository / CLI will not catch the corruption — that is the
//!     responsibility of operator workflow + filesystem permissions,
//!     not this helper.
//!
//! See MED-PR90-02 acceptance item in
//! `docs/superpowers/plans/2026-05-25-m4-ingress-plan.md` §3 W2.

use thiserror::Error;
use zeroize::Zeroizing;

/// XOR mask.  Single byte chosen arbitrarily; the constant is NOT a
/// secret.  Documented only so a future reader knows the on-disk
/// BLOB transformation is deterministic.
const MASK: u8 = 0x5A;

/// Typed errors from [`Coding`].
#[derive(Debug, Error)]
pub enum CodingError {
    /// Empty input — storing an empty `key_pass_enc` BLOB is meaningless
    /// (repository column is `NOT NULL`).  Caller must validate before
    /// invoking encode/decode.
    #[error("coding input is empty")]
    EmptyInput,
}

/// WebCheck-symmetry obfuscation namespace.
pub struct Coding;

impl Coding {
    /// Obfuscate a password (or other secret BLOB) for storage in
    /// `operators.key_pass_enc`.  Length-preserving; reversible via
    /// [`Self::decode`].
    pub fn encode(plain: &[u8]) -> Result<Vec<u8>, CodingError> {
        if plain.is_empty() {
            return Err(CodingError::EmptyInput);
        }
        Ok(plain.iter().map(|b| b ^ MASK).collect())
    }

    /// Deobfuscate.  Length-preserving; inverse of [`Self::encode`].
    ///
    /// Returns the plaintext wrapped in [`Zeroizing`] so the recovered
    /// secret is wiped from heap on drop.  Callers MUST pass the
    /// resulting slice to consumers (e.g., [`crate::runtime::bindings::
    /// OperatorKeyLoader::load`]) by reference, NOT by clone — cloning
    /// loses the wipe guarantee for the duplicate buffer.
    pub fn decode(obfuscated: &[u8]) -> Result<Zeroizing<Vec<u8>>, CodingError> {
        if obfuscated.is_empty() {
            return Err(CodingError::EmptyInput);
        }
        Ok(Zeroizing::new(
            obfuscated.iter().map(|b| b ^ MASK).collect(),
        ))
    }
}
