//! Production [`OperatorKeyLoader`] — reads a per-FN JKS container +
//! plaintext password and assembles a [`SigningContext`] backed by the
//! in-process DSTU signer.  RS-1 Pieces 3 + 3b (the "M5 crypto wiring"
//! the [`crate::runtime::bindings::OperatorKeyLoader`] stub anticipated).
//!
//! Ports the live-proven W4-Z3 path
//! (`extract_private_key` → [`SigningSession::from_extracted`] →
//! [`build_fn_sign`]), DPS-ЄВПЗ-accepted 2026-05-29.
//!
//! **Hardening boundary:** the JKS password is handled FLAT (unsealed)
//! here — sealing it at-rest (a `cred_salt` column + seal/unseal
//! pipeline) is a deliberate, separately-tracked follow-up, NOT done in
//! this piece.

use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use async_trait::async_trait;
use prro_crypto::cms::builder::{CmsBuildOptions, CmsError, CmsSigner};
use prro_crypto::cms::profile::CmsProfile;
use prro_crypto::cms::signer::DstuInProcessSigner;
use prro_crypto::core::curve::Curve;
use prro_crypto::core::field::FieldEl;
use prro_crypto::interop::prro::containers::{extract_private_key, ContainerError};
use prro_crypto::interop::prro::jks::JksError;
use zeroize::Zeroizing;

use crate::crypto::in_process::InProcessProvider;
use crate::crypto::provider::CryptoProvider;
use crate::crypto::session::SigningSession;
use crate::runtime::bindings::{KeyLoadFailure, OperatorKeyLoader};
use crate::services::write_path::stage_sign::SigningContext;
use crate::transports::dps::dto::CheckSignBlob;

/// The DPS-proven CMS profile for the FN-sign blob + the operator
/// signing context (DSTU-4145 sig + GOST-34.311 hash, curve PB-257).
const FN_SIGN_PROFILE: CmsProfile = CmsProfile::Dstu4145WithGost34311Pb;

/// Production cashier-key loader.  Stateless — a single instance serves
/// every FN (the per-FN identity arrives as `operator_id` + `key_path`
/// per call).
pub struct JksOperatorKeyLoader;

#[async_trait]
impl OperatorKeyLoader for JksOperatorKeyLoader {
    async fn load(
        &self,
        operator_id: &str,
        key_path: &Path,
        password: &[u8],
    ) -> Result<SigningContext, KeyLoadFailure> {
        // Read the container bytes.  ENOENT / unreadable → FileNotFound.
        let data = std::fs::read(key_path)
            .map_err(|_| KeyLoadFailure::FileNotFound(key_path.to_path_buf()))?;

        // `extract_private_key` wants `&str`; the password arrives as a
        // borrowed `&[u8]` into the caller's `Zeroizing` buffer.  Re-wrap
        // the UTF-8 view in `Zeroizing<String>` so no plain `String`
        // escapes (secret-material discipline at the trait boundary).
        let password_str: Zeroizing<String> = match std::str::from_utf8(password) {
            Ok(s) => Zeroizing::new(s.to_string()),
            Err(_) => {
                return Err(KeyLoadFailure::Other(
                    "key password is not valid UTF-8".to_string(),
                ));
            }
        };

        let extracted = extract_private_key(&data, password_str.as_str())
            .map_err(|e| map_container_err(e, key_path))?;

        // `from_extracted` selects the SIGNING cert (KeyUsage=
        // digitalSignature — NOT certs[0], the -14 `CryptBadSign` trap)
        // and stores the real `operator_id` (cashier INN) verbatim.
        let session = SigningSession::from_extracted(operator_id.to_string(), extracted)
            .map_err(|e| KeyLoadFailure::Other(format!("signing session assembly failed: {e:?}")))?;

        Ok(SigningContext {
            provider: Arc::new(InProcessProvider::new()) as Arc<dyn CryptoProvider>,
            session,
            profile: FN_SIGN_PROFILE,
        })
    }
}

/// Map a `prro_crypto` container error to the loader's audit-friendly
/// [`KeyLoadFailure`].  Only a bad JKS password is distinguishable as
/// `WrongPassword`; every other parse/extract failure is `Other` (with a
/// `Debug` rendering for the audit payload — no secret bytes leak, the
/// container errors carry only structural reasons).
fn map_container_err(e: ContainerError, key_path: &Path) -> KeyLoadFailure {
    match e {
        ContainerError::Jks(JksError::BadPassword) => {
            KeyLoadFailure::WrongPassword(key_path.to_path_buf())
        }
        other => KeyLoadFailure::Other(format!("{other:?}")),
    }
}

/// Build the per-FN `CheckSignBlob` (the `rro_fn_sign` blob attached to
/// `lastChk`/`statusRro`/`infoRro` read RPCs) — a native ATTACHED
/// CAdES-BES CMS over the **fiscal-number string**, signed with the
/// session's key.  Ported verbatim from the live-proven W4-Z3
/// `sign_fn_blob` (DPS ЄВПЗ-accepted 2026-05-29).
///
/// Reuses the already-loaded [`SigningSession`] (its `param_d` +
/// embedded signing `cert_der`) — no second JKS read.  The supervisor
/// (RS-1 Piece 5) calls this once per FN at boot to build the
/// `fn_sign` map.  `attached: true` is load-bearing (the blob embeds the
/// eContent, the ЦЗО CAdES-BES sample shape); `signing_cert()` selection
/// already happened inside `from_extracted`, so `session.cert_der()` is
/// the correct signing cert.
pub fn build_fn_sign(
    session: &SigningSession,
    fiscal_number: &str,
) -> Result<CheckSignBlob, CmsError> {
    let curve = Curve::dstu_pb_257();
    let d = FieldEl::from_le_bytes(&session.param_d()[..], curve.mod_words);
    let signer = DstuInProcessSigner::new(d);
    let der = CmsSigner {
        cert_der: session.cert_der(),
        signer: &signer,
        profile: FN_SIGN_PROFILE,
    }
    .sign_with(
        fiscal_number.as_bytes(),
        CmsBuildOptions {
            attached: true,
            signing_time: Some(SystemTime::now()),
        },
    )?
    .cms_der;
    Ok(CheckSignBlob(der))
}
