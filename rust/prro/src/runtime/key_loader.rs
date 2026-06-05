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

use crate::crypto::in_process::InProcessProvider;
use crate::crypto::provider::CryptoProvider;
use crate::crypto::session::SigningSession;
use crate::runtime::bindings::{KeyLoadFailure, OperatorKeyLoader};
use crate::services::write_path::stage_sign::SigningContext;
use crate::transports::dps::dto::CheckSignBlob;
// NOTE: no `Zeroizing` import — the loader borrows the caller's already-
// zeroized password slice without making an owned copy.

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

        // `extract_private_key` wants `&str`.  The password is already a
        // borrowed `&[u8]` into the caller's `Zeroizing` buffer (wiped on
        // drop), so borrow a `&str` VIEW directly — do NOT allocate an
        // owned copy (`to_string()`/`Zeroizing<String>` would be an
        // unnecessary SECOND copy of secret material; external review
        // 2026-05-30).
        let password_str = std::str::from_utf8(password)
            .map_err(|_| KeyLoadFailure::Other("key password is not valid UTF-8".to_string()))?;

        let extracted =
            extract_private_key(&data, password_str).map_err(|e| map_container_err(e, key_path))?;

        // `from_extracted` selects the SIGNING cert (KeyUsage=
        // digitalSignature — NOT certs[0], the -14 `CryptBadSign` trap)
        // and stores the real `operator_id` (cashier INN) verbatim.
        // `from_extracted`'s only failure is a missing signing certificate.
        // Use a FIXED, PII-free message: the `CryptoError` Debug embeds the
        // `operator_id` (cashier INN), which must not land in a
        // `KeyLoadFailure` string a future consumer might log/Display.
        let session =
            SigningSession::from_extracted(operator_id.to_string(), extracted).map_err(|_| {
                KeyLoadFailure::Other("no signing certificate in key container".to_string())
            })?;

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
/// `attached: true` is load-bearing (the blob embeds the eContent, the
/// ЦЗО CAdES-BES sample shape); `signing_cert()` selection already
/// happened inside `from_extracted`, so `session.cert_der()` is the
/// correct signing cert.
///
/// # FRESHNESS — do NOT cache the result across RPCs
///
/// `signing_time` is stamped `SystemTime::now()` at CALL time, so the
/// returned `CheckSignBlob` is TIME-SENSITIVE.  The ФСКО protocol requires
/// `rro_fn_sign` to carry a time mark INSIDE the signed blob ("підписаний
/// електронним підписом з позначкою часу"; the read RPCs have NO separate
/// timestamp field), and BOTH reference clients re-sign it FRESH on every
/// read RPC (WebCheck deletes + re-signs `FN.xml.p7s` per `lastChk`;
/// PRRODPS signs per call with a `now`-nonce) — confirmed 2026-05-30.  A
/// stale (boot-cached) `signingTime` is therefore non-conformant and risks
/// DPS rejection.  The supervisor (RS-1 Piece 5) MUST build `fn_sign`
/// fresh **per read-RPC / per probe-tick — NOT once at boot into a
/// long-lived map**.  Per-tick rebuild (the ~60s return-online probe /
/// keepalive cadence) is sufficient — uniqueness per individual RPC is not
/// required.  Reconcile this with the existing
/// `RuntimeView.fn_sign: &CheckSignBlob` (a cached blob) when wiring the
/// loops in Piece 5.
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
