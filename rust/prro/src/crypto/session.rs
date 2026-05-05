//! Secret-material boundary.
//!
//! `unseal_jks` is the only function that turns sealed bytes into
//! plaintext.  The plaintext private key lives in `Zeroizing<[u8; 32]>`
//! and is exposed to the rest of the crate ONLY through `SigningSession`'s
//! crate-internal accessor; external callers never see plaintext key bytes.
//!
//! ADR-M2-5 §4d demands no secret substring leak through `Debug`.  Both
//! `SealedMaterial` and `SigningSession` provide manual redacted `Debug`
//! impls — `#[derive(Debug)]` is forbidden on these types and is enforced
//! by absence (the W6 tracing test additionally proves no substring escapes
//! through any `tracing` event).

use std::sync::Arc;

use zeroize::Zeroizing;

use crate::crypto::errors::{CryptoError, SealKind};

/// Sealed-on-disk material handed to `unseal_jks`.  Held by the caller
/// (typically a storage repository).  Borrows everything to avoid owning
/// any secret bytes longer than the unseal call itself.
pub struct SealedMaterial<'a> {
    pub operator_id: &'a str,
    pub jks_bytes: &'a [u8],
    /// Hex-encoded XOR-soft-sealed JKS password.
    pub jks_password_hex: &'a str,
    /// Per-operator credential salt; XORed against the decoded sealed
    /// password (cycled if the password is longer than the salt) to
    /// recover the plaintext.
    pub cred_salt: &'a [u8],
}

impl std::fmt::Debug for SealedMaterial<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealedMaterial")
            .field("operator_id", &self.operator_id)
            .field("jks_bytes", &"<redacted>")
            .field("jks_password_hex", &"<redacted>")
            .field("cred_salt", &"<redacted>")
            .finish()
    }
}

/// In-memory signing session.  The `Arc<Inner>` shape lets the in-process
/// provider clone the session into a `spawn_blocking` closure without
/// copying the 32-byte private scalar — only the strong-count is bumped.
/// `Zeroizing<[u8; 32]>` zeroes the array when the last `Arc` holder
/// drops.
#[derive(Clone)]
pub struct SigningSession {
    inner: Arc<SigningSessionInner>,
}

struct SigningSessionInner {
    operator_id: String,
    /// DSTU 4145 private scalar `d`, 32 LE bytes.  Zeroed on drop.
    param_d: Zeroizing<[u8; 32]>,
    /// Operator's signing certificate, DER-encoded.
    cert_der: Vec<u8>,
}

impl std::fmt::Debug for SigningSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SigningSession")
            .field("operator_id", &self.inner.operator_id)
            .field("param_d", &"<redacted>")
            .field(
                "cert_der",
                &format_args!("<{} bytes>", self.inner.cert_der.len()),
            )
            .finish()
    }
}

impl SigningSession {
    pub fn operator_id(&self) -> &str {
        &self.inner.operator_id
    }

    /// DER-encoded operator certificate.  Public bytes — no redaction.
    pub fn cert_der(&self) -> &[u8] {
        &self.inner.cert_der
    }

    /// Crate-internal accessor; the in-process provider reads this when
    /// constructing a `DstuInProcessSigner`.  External callers MUST NOT
    /// see plaintext key bytes — `pub(crate)` is the wall.
    pub(crate) fn param_d(&self) -> &Zeroizing<[u8; 32]> {
        &self.inner.param_d
    }

    /// Test-only constructor.
    ///
    /// **Production must not call this.**  The only production path to
    /// a `SigningSession` is `unseal_jks`.  This constructor is exposed
    /// (un-feature-gated) because integration tests in `tests/` are
    /// separate crates and cannot see `cfg(test)`-only items in the
    /// lib; the function name carries the warning instead.  The
    /// architectural risk is bounded: a caller who does invoke this
    /// from production code already had the plaintext private scalar
    /// in hand, so this constructor neither weakens the seal boundary
    /// nor leaks anything.
    pub fn new_for_test(operator_id: String, param_d: [u8; 32], cert_der: Vec<u8>) -> Self {
        Self {
            inner: Arc::new(SigningSessionInner {
                operator_id,
                param_d: Zeroizing::new(param_d),
                cert_der,
            }),
        }
    }
}

/// Unseal a JKS keystore + extract the DSTU private scalar.
///
/// Pipeline:
///   1. Hex-decode the sealed `jks_password_hex`.
///   2. XOR-undo the soft seal with `cred_salt` (cycled to match length).
///   3. Decode plaintext as UTF-8 (rejects non-UTF-8 with a typed error).
///   4. Call `prro_crypto::interop::prro::containers::extract_private_key`
///      which routes to the JKS parser based on the file's magic bytes.
///   5. Pick the first cert from the keystore as the operator's signing
///      cert (the JKS reader returns them in stored order).
///
/// All intermediate plaintext (the unsealed password) is held in
/// `Zeroizing` and dropped before this function returns.
pub fn unseal_jks(sealed: SealedMaterial<'_>) -> Result<SigningSession, CryptoError> {
    let operator_id = sealed.operator_id.to_string();
    let make_err = |reason: SealKind| CryptoError::JksUnseal {
        operator_id: operator_id.clone(),
        reason,
    };

    // 1. hex → bytes.  Sealed password is hex-encoded so it survives
    //    serialisation as plain text (TOML/SQL/YAML config files).
    if !sealed.jks_password_hex.len().is_multiple_of(2) {
        return Err(make_err(SealKind::BadPassword));
    }
    let mut sealed_bytes: Zeroizing<Vec<u8>> =
        Zeroizing::new(Vec::with_capacity(sealed.jks_password_hex.len() / 2));
    let bytes = sealed.jks_password_hex.as_bytes();
    for i in 0..bytes.len() / 2 {
        let hi = match hex_digit(bytes[i * 2]) {
            Some(v) => v,
            None => return Err(make_err(SealKind::BadPassword)),
        };
        let lo = match hex_digit(bytes[i * 2 + 1]) {
            Some(v) => v,
            None => return Err(make_err(SealKind::BadPassword)),
        };
        sealed_bytes.push((hi << 4) | lo);
    }

    // 2. XOR-unseal with cred_salt.  Empty salt would be a no-op (and
    //    means the seal scheme was misconfigured); reject explicitly.
    if sealed.cred_salt.is_empty() {
        return Err(make_err(SealKind::BadSalt));
    }
    let mut password_plain: Zeroizing<Vec<u8>> = Zeroizing::new(sealed_bytes.to_vec());
    for (i, b) in password_plain.iter_mut().enumerate() {
        *b ^= sealed.cred_salt[i % sealed.cred_salt.len()];
    }

    // 3. UTF-8 decode the unsealed password.  prro_crypto's JKS reader
    //    expects a `&str`.  We re-wrap the resulting String in a
    //    drop-on-end closure-scope so the plaintext doesn't outlive the
    //    extract_private_key call.
    let password_str: Zeroizing<String> = match std::str::from_utf8(&password_plain) {
        Ok(s) => Zeroizing::new(s.to_string()),
        Err(_) => return Err(make_err(SealKind::BadPassword)),
    };

    // 4. Extract.  prro_crypto routes JKS / Key-6 / PFX automatically
    //    based on the file's magic bytes.  M2 only commits to JKS in
    //    production (per ADR-M2-1 + the audit doc) but we don't reject
    //    other formats here — the call will surface a typed error if
    //    the bytes don't match the JKS magic and a different format
    //    parser isn't shipped yet.
    let extracted = prro_crypto::interop::prro::containers::extract_private_key(
        sealed.jks_bytes,
        password_str.as_str(),
    )
    .map_err(|e| {
        use prro_crypto::interop::prro::containers::ContainerError;
        let kind = match e {
            ContainerError::Jks(jks_err) => match jks_err {
                prro_crypto::interop::prro::jks::JksError::BadPassword => SealKind::BadPassword,
                prro_crypto::interop::prro::jks::JksError::BadMagic
                | prro_crypto::interop::prro::jks::JksError::NotPrivateKey(_)
                | prro_crypto::interop::prro::jks::JksError::Truncated(_) => SealKind::MalformedJks,
            },
            ContainerError::UnknownFormat | ContainerError::ParserNotImplemented(_) => {
                SealKind::MalformedJks
            }
            ContainerError::Der(_) | ContainerError::BadKeyWidth(_) => {
                SealKind::KeyExtractionFailed
            }
            ContainerError::Key6(_) | ContainerError::Pfx(_) => SealKind::KeyExtractionFailed,
        };
        make_err(kind)
    })?;

    // 5. First cert in the keystore is the operator's signing cert.
    let cert_der = extracted
        .certs
        .into_iter()
        .next()
        .ok_or_else(|| make_err(SealKind::KeyExtractionFailed))?;

    // Move the still-Zeroizing<[u8; 32]> into the session inner.  No
    // clone of the 32 bytes — the Zeroizing<...> is moved.
    let param_d_zeroizing: Zeroizing<[u8; 32]> = extracted.param_d;
    Ok(SigningSession {
        inner: Arc::new(SigningSessionInner {
            operator_id,
            param_d: param_d_zeroizing,
            cert_der,
        }),
    })
}

fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}
