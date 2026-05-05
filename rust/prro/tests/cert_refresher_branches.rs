//! W2/C3 write-branch coverage.
//!
//! Proves the two `Refreshed*` branches in `services::cert_refresher::
//! refresh_for_fn` are real — eligible certs land in
//! `RefreshedInPlace` / `RefreshedKeyRoll`, not the
//! `RefreshError::Db("C3-not-yet-landed: ...")` stub the C2 commit
//! used as a fence.  The C4 commit will add wiremock-backed
//! transport-level smoke; this file covers the DB-level branch logic.
//!
//! Two tests:
//! 1. `refresh_same_ski_writes_in_place_only` — cert reissued with the
//!    same key.  Asserts `Ok(RefreshedInPlace { ski })`, the active
//!    row's metadata fields are refreshed, and there is still exactly
//!    one `active=1` row for the FN.
//! 2. `refresh_new_ski_runs_key_roll_atomic` — cert with a different
//!    SKI.  Asserts `Ok(RefreshedKeyRoll { ski_old, ski_new })`, the
//!    new row is `active=1`, the old row is `active=0`, and an
//!    `audit_log` entry for `cert_refresh_key_roll` was emitted in
//!    the same tx.
//!
//! The mock provider returns a vendored DSTU 4145 cert
//! (`SELF_SIGNED_ENC_6929.cer` from the jkurwa upstream test corpus);
//! its real SKI is computed at test time so we never hard-code a
//! magic hex value.

use std::sync::Arc;

use async_trait::async_trait;
use prro::crypto::{
    CertDer, CryptoError, CryptoProvider, DstuVerifyResult, SignCmsRequest, SignedCmsBytes,
    SigningSession,
};
use prro::services::cert_refresher::{refresh_for_fn, RefreshOutcome};

const FIXTURE_CERT_DER: &[u8] =
    include_bytes!("../../prro_crypto/node_modules/jkurwa/test/data/SELF_SIGNED_ENC_6929.cer");

/// Mock `CryptoProvider` that returns a configured cert DER for every
/// `fetch_cert_by_ski` call.  All other methods panic with
/// `unimplemented!()` because `refresh_for_fn` does not call them.
struct StaticCertProvider {
    cert_der: Vec<u8>,
}

#[async_trait]
impl CryptoProvider for StaticCertProvider {
    async fn sign_cms_detached(
        &self,
        _request: SignCmsRequest<'_>,
    ) -> Result<SignedCmsBytes, CryptoError> {
        unimplemented!("refresh_for_fn does not call sign_cms_detached")
    }

    async fn verify_dstu(
        &self,
        _content_digest: &[u8],
        _sig_bytes: &[u8],
        _pubkey_compressed: &[u8],
    ) -> Result<DstuVerifyResult, CryptoError> {
        unimplemented!("refresh_for_fn does not call verify_dstu")
    }

    async fn unwrap_envelope(
        &self,
        _envelope_der: &[u8],
        _originator_cert_der: &[u8],
        _session: &SigningSession,
    ) -> Result<Vec<u8>, CryptoError> {
        unimplemented!("refresh_for_fn does not call unwrap_envelope")
    }

    async fn fetch_cert_by_ski(
        &self,
        _urls: &[String],
        _ski: &[u8; 32],
        _request_timeout: std::time::Duration,
    ) -> Result<CertDer, CryptoError> {
        Ok(CertDer(self.cert_der.clone()))
    }
}

async fn fresh_pool_with_fn(fn_id: &str) -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs migrations");
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fn_id)
    .execute(&pool)
    .await
    .expect("seed FN row");
    (dir, pool)
}

/// Compute the same SKI hex `parse_cert_metadata` does.  Uses
/// `prro_crypto`'s public `extract_cert_pubkey_bytes` + `compute_ski`
/// — the exact pair `parse_cert_metadata` calls internally.
fn fixture_cert_ski_hex() -> String {
    use prro_crypto::cms::envelope::{compute_ski, extract_cert_pubkey_bytes};
    let pubkey = extract_cert_pubkey_bytes(FIXTURE_CERT_DER).expect("fixture pubkey");
    let ski = compute_ski(&pubkey);
    ski.iter().map(|b| format!("{b:02x}")).collect()
}

async fn stage_active_cert(pool: &sqlx::SqlitePool, fn_id: &str, ski_hex: &str, valid_to: &str) {
    sqlx::query(
        "INSERT INTO operator_certs( \
             ski_hex, fiscal_number, cert_fingerprint, cert_der, \
             valid_from, valid_to, subject_dn, issuer_dn, \
             fetched_at, source, active) \
         VALUES (?, ?, 'fp-stale', x'00', \
                 '2020-01-01T00:00:00Z', ?, \
                 'CN=stale', 'CN=stale-issuer', \
                 '2020-01-01T00:00:00Z', 'manual', 1)",
    )
    .bind(ski_hex)
    .bind(fn_id)
    .bind(valid_to)
    .execute(pool)
    .await
    .expect("stage active cert row");
}

#[tokio::test]
async fn refresh_same_ski_writes_in_place_only() {
    let fn_id = "1234567890";
    let (_d, pool) = fresh_pool_with_fn(fn_id).await;

    // Stage an active cert with the SAME ski_hex the mock will return,
    // and a `valid_to` close enough to now to trigger refresh
    // (refresh_within_days defaults to 30; we use now+1day).
    let ski_hex = fixture_cert_ski_hex();
    let valid_to_soon = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
    stage_active_cert(&pool, fn_id, &ski_hex, &valid_to_soon).await;

    let provider: Arc<dyn CryptoProvider> = Arc::new(StaticCertProvider {
        cert_der: FIXTURE_CERT_DER.to_vec(),
    });
    let outcome = refresh_for_fn(&pool, fn_id, provider)
        .await
        .expect("same-SKI refresh must succeed");
    assert_eq!(
        outcome,
        RefreshOutcome::RefreshedInPlace {
            ski: ski_hex.clone(),
        },
        "expected RefreshedInPlace; got {outcome:?}"
    );

    // Post-conditions:
    //   - exactly one row total for this FN (no staged-extra row)
    //   - that row is active=1, with the SAME ski_hex
    //   - subject_dn now reflects the fixture (was 'CN=stale'); proves
    //     the in-place UPDATE actually wrote the new metadata
    let rows: Vec<(String, i64, String)> = sqlx::query_as(
        "SELECT ski_hex, active, subject_dn FROM operator_certs WHERE fiscal_number = ?",
    )
    .bind(fn_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1, "must remain exactly one row; got {rows:?}");
    assert_eq!(rows[0].0, ski_hex);
    assert_eq!(rows[0].1, 1);
    assert_ne!(
        rows[0].2, "CN=stale",
        "subject_dn must be refreshed from the fixture cert; still stale: {rows:?}"
    );

    // No audit_log entry expected for in-place (only key-roll emits one).
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE event_type = 'cert_refresh_key_roll'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 0);
}

#[tokio::test]
async fn refresh_new_ski_runs_key_roll_atomic() {
    let fn_id = "1234567890";
    let (_d, pool) = fresh_pool_with_fn(fn_id).await;

    // Stage an active cert with a DIFFERENT ski_hex than the fixture's,
    // so the branch decision picks key-roll.  Must satisfy the
    // ski_hex CHECK (length(ski_hex) = 64) so use a 64-char dummy.
    let stale_ski = "f".repeat(64);
    let valid_to_soon = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
    stage_active_cert(&pool, fn_id, &stale_ski, &valid_to_soon).await;

    let new_ski = fixture_cert_ski_hex();
    assert_ne!(stale_ski, new_ski, "test setup invariant: SKIs must differ");

    let provider: Arc<dyn CryptoProvider> = Arc::new(StaticCertProvider {
        cert_der: FIXTURE_CERT_DER.to_vec(),
    });
    let outcome = refresh_for_fn(&pool, fn_id, provider)
        .await
        .expect("key-roll refresh must succeed");
    assert_eq!(
        outcome,
        RefreshOutcome::RefreshedKeyRoll {
            ski_old: stale_ski.clone(),
            ski_new: new_ski.clone(),
        },
        "expected RefreshedKeyRoll; got {outcome:?}"
    );

    // Post-conditions:
    //   - exactly two rows for this FN (old + new)
    //   - exactly one row active=1, with the new ski_hex
    //   - the old row is now active=0
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT ski_hex, active FROM operator_certs WHERE fiscal_number = ? ORDER BY ski_hex",
    )
    .bind(fn_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2, "must have old + new row; got {rows:?}");
    let active_count: usize = rows.iter().filter(|(_, a)| *a == 1).count();
    assert_eq!(active_count, 1, "exactly one active row; got {rows:?}");
    let active_ski = &rows.iter().find(|(_, a)| *a == 1).unwrap().0;
    assert_eq!(*active_ski, new_ski, "active row must be the new SKI");

    // Audit entry must exist with the right ski_old/ski_new payload.
    let audit_payload: String = sqlx::query_scalar(
        "SELECT event_payload_json FROM audit_log \
         WHERE event_type = 'cert_refresh_key_roll' AND entity_id = ?",
    )
    .bind(fn_id)
    .fetch_one(&pool)
    .await
    .expect("audit_log row must exist");
    assert!(
        audit_payload.contains(&stale_ski),
        "audit payload missing ski_old: {audit_payload}"
    );
    assert!(
        audit_payload.contains(&new_ski),
        "audit payload missing ski_new: {audit_payload}"
    );
}
