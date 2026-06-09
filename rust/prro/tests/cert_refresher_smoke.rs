//! W2/C4 transport-level smoke for `services::cert_refresher`.
//!
//! C3's `cert_refresher_branches.rs` covers the DB write-path
//! correctness (in-place vs key-roll, rows_affected guards, stale-read
//! rollback) using a `StaticCertProvider` mock that returns DER bytes
//! synchronously.  This file uses a real `InProcessProvider` against a
//! real `wiremock` HTTP server to cover the transport layer:
//!
//! 1. `refresh_succeeds_via_wiremock_iit_envelope` — single URL
//!    responds 200 with a valid IIT CMP envelope wrapping the
//!    fixture cert; the active row's `ski_hex` matches the
//!    fixture's SKI (same-SKI = cert reissued with same key).
//!    Asserts `Ok(RefreshedInPlace)`.
//! 2. `refresh_falls_back_when_first_url_returns_5xx` — two URLs in
//!    `ca_endpoints`; the first (higher priority) returns 503, the
//!    second returns 200 with a valid envelope.  Asserts
//!    `Ok(RefreshedInPlace)` AND that BOTH URLs were hit (multi-URL
//!    fallback proven at the transport layer, not just the mock layer).
//! 3. `refresh_returns_all_urls_failed_when_all_5xx` — both URLs
//!    return 503.  Asserts `Err(AllUrlsFailed)` AND the DB is
//!    unchanged (no orphan staged row, active cert still active=1).
//! 4. `refresh_returns_crypto_error_on_garbage_response_body` — URL
//!    returns 200 with random bytes (not a CMP envelope).  Asserts
//!    `Err(AllUrlsFailed)` because `parse_iit_cert_response` fails
//!    on the body, fetch_cert_blocking treats it as a per-URL
//!    failure, and the single URL exhausts the fallback list.
//!
//! Why not a wiremock key-roll test: the IIT CMP protocol is
//! request-by-SKI (client asks for cert with SKI=X, server returns
//! the cert with SKI=X plus its chain).  A real-world key-roll
//! happens when the operator generates a new key locally and
//! provisions a new cert via a separate channel; the refresher's
//! by-SKI lookup never sees a different-SKI response.
//! `cert_refresher_branches.rs` covers the key-roll DB path with a
//! `StaticCertProvider` that bypasses the protocol's SKI-match
//! invariant — a deliberate test artefact, not a real-world flow.

use std::sync::Arc;

use prro::crypto::{CryptoProvider, InProcessProvider};
use prro::services::cert_refresher::{refresh_for_fn, RefreshError, RefreshOutcome};
use wiremock::matchers::{method, path as path_matcher};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Compute the same SKI hex `parse_cert_metadata` does — exposes the
/// fixture's real SKI so the test scaffolding can stage an
/// `operator_certs` row whose `ski_hex` matches what
/// `fetch_cert_by_ski` will look for in the wiremock response.
fn fixture_cert_ski_hex() -> String {
    use prro_crypto::cms::envelope::{compute_ski, extract_cert_pubkey_bytes};
    let pubkey = extract_cert_pubkey_bytes(FIXTURE_CERT_DER).expect("fixture pubkey");
    let ski = compute_ski(&pubkey);
    ski.iter().map(|b| format!("{b:02x}")).collect()
}

const FIXTURE_CERT_DER: &[u8] = include_bytes!("fixtures/SELF_SIGNED_ENC_6929.cer");

// ─── IIT CMP response envelope builder ────────────────────────────────

/// Wrap a cert DER blob in the IIT CMP response wire format that
/// `prro_crypto::cms::cmp::parse_iit_cert_response` expects:
///
/// ```text
/// SEQUENCE {                          -- ContentInfo
///     OBJECT IDENTIFIER 1.2.840.113549.1.7.1   -- id-data
///     [0] EXPLICIT {
///         OCTET STRING {
///             4 bytes (header — content ignored by parser)
///             4 bytes LE u32 = 1                -- IIT_STATUS_SUCCESS
///             <cert SEQUENCE>                   -- found by SKI walker
///         }
///     }
/// }
/// ```
///
/// `find_cert_with_ski_iterative` walks the inner blob recursively
/// looking for SEQUENCE-shaped Certificate TLVs and matches by SKI,
/// so any wrapping that places the cert SEQUENCE somewhere inside the
/// post-status payload works.
fn build_iit_cmp_response_envelope(cert_der: &[u8]) -> Vec<u8> {
    // OID 1.2.840.113549.1.7.1 = id-data, DER-encoded (sans tag/length).
    const ID_DATA_OID_DER: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x07, 0x01];

    // IIT inner payload: 4 bytes header (we use zeros) + 4 bytes LE
    // status (0x01 = success) + cert SEQUENCE bytes.
    let mut iit_payload = Vec::with_capacity(8 + cert_der.len());
    iit_payload.extend_from_slice(&[0u8; 4]); // header — ignored
    iit_payload.extend_from_slice(&1u32.to_le_bytes()); // status = success
    iit_payload.extend_from_slice(cert_der);

    // OCTET STRING wrapping the IIT payload.
    let mut octet_string = Vec::with_capacity(2 + iit_payload.len() + 4);
    octet_string.push(0x04);
    push_der_len(&mut octet_string, iit_payload.len());
    octet_string.extend_from_slice(&iit_payload);

    // [0] EXPLICIT context-specific tag wrapping the OCTET STRING.
    let mut context_explicit = Vec::with_capacity(2 + octet_string.len() + 4);
    context_explicit.push(0xa0);
    push_der_len(&mut context_explicit, octet_string.len());
    context_explicit.extend_from_slice(&octet_string);

    // OID TLV.
    let mut oid_tlv = Vec::with_capacity(2 + ID_DATA_OID_DER.len());
    oid_tlv.push(0x06);
    push_der_len(&mut oid_tlv, ID_DATA_OID_DER.len());
    oid_tlv.extend_from_slice(ID_DATA_OID_DER);

    // Outer SEQUENCE.
    let inner_len = oid_tlv.len() + context_explicit.len();
    let mut out = Vec::with_capacity(2 + inner_len + 4);
    out.push(0x30);
    push_der_len(&mut out, inner_len);
    out.extend_from_slice(&oid_tlv);
    out.extend_from_slice(&context_explicit);
    out
}

/// Standard DER definite-length encoder; same shape `prro_crypto::cms::
/// envelope` uses.  Up to 4-byte long-form lengths is plenty for any
/// cert + IIT wrapper we'd ever build in tests.
fn push_der_len(out: &mut Vec<u8>, len: usize) {
    if len < 0x80 {
        out.push(len as u8);
    } else if len < 0x100 {
        out.push(0x81);
        out.push(len as u8);
    } else if len < 0x10000 {
        out.push(0x82);
        out.push((len >> 8) as u8);
        out.push(len as u8);
    } else {
        out.push(0x83);
        out.push((len >> 16) as u8);
        out.push((len >> 8) as u8);
        out.push(len as u8);
    }
}

// ─── Test scaffolding ─────────────────────────────────────────────────

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

/// Replace the seeded `ca_endpoints` rows with the caller-supplied
/// list (in priority order — first arg = priority 10, second = 20,
/// etc.).  Migration 006 seeds the production URLs by default; tests
/// need to point them at the wiremock instance instead.
async fn replace_ca_endpoints(pool: &sqlx::SqlitePool, urls: &[&str]) {
    sqlx::query("DELETE FROM ca_endpoints")
        .execute(pool)
        .await
        .expect("clear seed");
    for (i, url) in urls.iter().enumerate() {
        let priority = (i + 1) as i64 * 10;
        sqlx::query(
            "INSERT INTO ca_endpoints(name, cmp_url, priority, enabled) \
             VALUES (?, ?, ?, 1)",
        )
        .bind(format!("test-ca-{i}"))
        .bind(*url)
        .bind(priority)
        .execute(pool)
        .await
        .expect("seed test ca_endpoint");
    }
}

async fn stage_active_cert(
    pool: &sqlx::SqlitePool,
    fn_id: &str,
    ski_hex: &str,
    valid_to_rfc3339: &str,
) {
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
    .bind(valid_to_rfc3339)
    .execute(pool)
    .await
    .expect("stage active cert row");
}

fn provider() -> Arc<dyn CryptoProvider> {
    Arc::new(InProcessProvider::new())
}

fn near_expiry() -> String {
    (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339()
}

// ─── Tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn refresh_succeeds_via_wiremock_iit_envelope() {
    let fn_id = "1234567890";
    let (_d, pool) = fresh_pool_with_fn(fn_id).await;

    let mock = MockServer::start().await;
    let envelope = build_iit_cmp_response_envelope(FIXTURE_CERT_DER);
    Mock::given(method("POST"))
        .and(path_matcher("/services/cmp/a/"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(envelope))
        .expect(1)
        .mount(&mock)
        .await;

    replace_ca_endpoints(&pool, &[&format!("{}/services/cmp/a/", mock.uri())]).await;

    // Stage active row with the SAME SKI the fixture cert produces —
    // the IIT CMP protocol is request-by-SKI, so the refresher asks
    // for SKI=X and gets back the cert with SKI=X.  Same-SKI =
    // in-place refresh (cert reissued, key kept).
    let active_ski = fixture_cert_ski_hex();
    stage_active_cert(&pool, fn_id, &active_ski, &near_expiry()).await;

    let outcome = refresh_for_fn(&pool, fn_id, provider())
        .await
        .expect("wiremock-backed refresh must succeed");
    assert!(
        matches!(outcome, RefreshOutcome::RefreshedInPlace { .. }),
        "expected RefreshedInPlace; got {outcome:?}"
    );
    // wiremock's `.expect(1)` already enforces the call-count contract;
    // its drop on the MockServer panics if the count wasn't met.
}

#[tokio::test]
async fn refresh_falls_back_when_first_url_returns_5xx() {
    let fn_id = "1234567890";
    let (_d, pool) = fresh_pool_with_fn(fn_id).await;

    let mock = MockServer::start().await;
    let envelope = build_iit_cmp_response_envelope(FIXTURE_CERT_DER);

    // Path A (priority 10) → 503 once.  Path B (priority 20) → 200
    // valid envelope.  Both must be hit; .expect(1) on each enforces.
    Mock::given(method("POST"))
        .and(path_matcher("/services/cmp/a/"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path_matcher("/services/cmp/b/"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(envelope))
        .expect(1)
        .mount(&mock)
        .await;

    replace_ca_endpoints(
        &pool,
        &[
            &format!("{}/services/cmp/a/", mock.uri()),
            &format!("{}/services/cmp/b/", mock.uri()),
        ],
    )
    .await;
    let active_ski = fixture_cert_ski_hex();
    stage_active_cert(&pool, fn_id, &active_ski, &near_expiry()).await;

    let outcome = refresh_for_fn(&pool, fn_id, provider())
        .await
        .expect("multi-URL fallback must succeed when second URL responds 200");
    assert!(
        matches!(outcome, RefreshOutcome::RefreshedInPlace { .. }),
        "expected RefreshedInPlace; got {outcome:?}"
    );
}

#[tokio::test]
async fn refresh_returns_all_urls_failed_when_all_5xx() {
    let fn_id = "1234567890";
    let (_d, pool) = fresh_pool_with_fn(fn_id).await;

    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/services/cmp/a/"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path_matcher("/services/cmp/b/"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&mock)
        .await;

    replace_ca_endpoints(
        &pool,
        &[
            &format!("{}/services/cmp/a/", mock.uri()),
            &format!("{}/services/cmp/b/", mock.uri()),
        ],
    )
    .await;
    let stale_ski = "d".repeat(64);
    stage_active_cert(&pool, fn_id, &stale_ski, &near_expiry()).await;

    let err = refresh_for_fn(&pool, fn_id, provider())
        .await
        .expect_err("all-5xx fallback must produce a typed error");
    assert!(
        matches!(err, RefreshError::AllUrlsFailed),
        "expected AllUrlsFailed; got {err:?}"
    );

    // Post-condition: DB is untouched.  Active row still has the stale
    // ski_hex + active=1.  No staged row exists.  No audit_log entry.
    let rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT ski_hex, active FROM operator_certs WHERE fiscal_number = ?")
            .bind(fn_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 1, "no staged row should exist; got {rows:?}");
    assert_eq!(rows[0].0, stale_ski);
    assert_eq!(rows[0].1, 1);

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE event_type = 'cert_refresh_key_roll'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 0);
}

#[tokio::test]
async fn refresh_returns_all_urls_failed_on_garbage_response_body() {
    let fn_id = "1234567890";
    let (_d, pool) = fresh_pool_with_fn(fn_id).await;

    let mock = MockServer::start().await;
    // 200 OK with body that is NOT a valid IIT CMP envelope.
    // `parse_iit_cert_response` returns Err(CmpError::Parse(_));
    // fetch_cert_blocking maps that to a per-URL failure and tries
    // the next URL — there is no next URL, so AllUrlsFailed.
    Mock::given(method("POST"))
        .and(path_matcher("/services/cmp/a/"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"not-a-cmp-envelope".to_vec()))
        .expect(1)
        .mount(&mock)
        .await;

    replace_ca_endpoints(&pool, &[&format!("{}/services/cmp/a/", mock.uri())]).await;
    stage_active_cert(&pool, fn_id, &"d".repeat(64), &near_expiry()).await;

    let err = refresh_for_fn(&pool, fn_id, provider())
        .await
        .expect_err("garbage response body must surface a typed error");
    assert!(
        matches!(err, RefreshError::AllUrlsFailed),
        "expected AllUrlsFailed (parse failure → next URL → exhausted); got {err:?}"
    );
}
