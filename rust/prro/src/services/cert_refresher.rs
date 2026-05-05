//! Async cert refresh service (M2/W2).
//!
//! C1 landed the module seam + migration 006.  C2 (this commit) lands
//! the substrate: types, helpers, and pure read paths.  No write-path
//! side effects.  The transactional `refresh_for_fn` body lands in C3.
//!
//! Pipeline (per ADR-M2-4 + ADR-M2-6):
//!
//! 1. Load FN row + `cert_provisioning_config` + `ca_endpoints` (DB
//!    read, no tx).
//! 2. If currently-active cert's `valid_to - now > refresh_within_days`,
//!    return `NoChange`.
//! 3. Compute the SKI to fetch (= currently-active cert's SKI for
//!    refresh).
//! 4. Call `provider.fetch_cert_by_ski(urls, ski, timeout)` — outside
//!    any tx.
//! 5. Parse cert metadata (SKI, valid_from/to, subject/issuer DN) via
//!    `prro_crypto::cms::envelope::parse_cert_basic_fields`.
//! 6. If the new SKI matches the active SKI → in-place UPDATE the
//!    existing active=1 row (single short tx, rows_affected==1).
//! 7. Else (key-roll) → ONE `with_immediate` tx that runs:
//!    `INSERT … active=0 ON CONFLICT(ski_hex) DO UPDATE …
//!    WHERE fiscal_number = excluded.fiscal_number AND active = 0`
//!    (idempotent stage that REFUSES to clobber foreign-owned or
//!    active=1 rows), then `UPDATE … SET active=0 WHERE
//!    fiscal_number=? AND active=1`, then `UPDATE … SET active=1
//!    WHERE ski_hex=?`, then audit_log INSERT.  Atomic + idempotent
//!    on retry (no orphan staged-row window).
//! 8. Return `RefreshedInPlace { ski }` or `RefreshedKeyRoll { old, new }`.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use sqlx::SqlitePool;

use crate::crypto::{CryptoError, CryptoProvider};

/// Static configuration loaded from `cert_provisioning_config` at the
/// start of every refresh cycle.  Held by value because the table has
/// at most one row (id = 1) and the values rarely change — a fresh
/// `load_refresh_config` per call costs ~one SQLite page lookup.
#[derive(Debug, Clone)]
pub struct RefreshConfig {
    /// If `valid_to - now <= refresh_within_days`, the cert is eligible
    /// for refresh.  Sourced from M1's
    /// `cert_provisioning_config.refresh_within_days` (default 30).
    pub refresh_within_days: i64,
    /// Per-URL CMP probe timeout, forwarded to
    /// `CryptoProvider::fetch_cert_by_ski`.  Sourced from M1's existing
    /// `cert_provisioning_config.timeout_seconds` column (default 10s).
    pub cmp_request_timeout: Duration,
}

/// Successful outcome of `refresh_for_fn`.
///
/// The contract is `Result<RefreshOutcome, RefreshError>` (no `Failed`
/// variant); every failure mode is an `Err(...)` and never a `Ok(Failed)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshOutcome {
    /// Active cert is not yet eligible for refresh
    /// (`valid_to - now > refresh_within_days`).
    NoChange,
    /// Same-SKI refresh: the cert was reissued but the key is unchanged.
    /// In-place UPDATE only — no PK conflict on `operator_certs.ski_hex`,
    /// no atomic flip needed (still exactly one `active=1` for this FN
    /// throughout).
    RefreshedInPlace { ski: String },
    /// Key-roll: the new cert carries a different SKI.  The key was
    /// rotated; old cert deactivated, new cert activated atomically
    /// inside one `with_immediate` tx.
    RefreshedKeyRoll { ski_old: String, ski_new: String },
}

/// Typed error surface for `refresh_for_fn`.  Each variant maps cleanly
/// to a single failure mode; nothing is swallowed.
#[derive(Debug, thiserror::Error)]
pub enum RefreshError {
    #[error("no active cert in operator_certs for FN {fn_id}")]
    NoActiveCert { fn_id: String },
    #[error("no enabled CA endpoints in ca_endpoints")]
    NoEnabledEndpoints,
    #[error("CMP fetch failed across all configured URLs")]
    AllUrlsFailed,
    #[error("CMP fetch returned a cert whose SKI differs from the request")]
    SkiMismatch,
    #[error("malformed cert metadata in operator_certs row for FN {fn_id}: {field}")]
    MalformedCertMetadata { fn_id: String, field: &'static str },
    #[error("malformed cert DER bytes returned from CMP")]
    MalformedCert,
    #[error("DB error: {0}")]
    Db(String),
    #[error("crypto provider error: {0:?}")]
    Crypto(CryptoError),
}

/// Snapshot of a cert's persisted metadata after a fresh
/// `prro_crypto::cms::envelope::parse_cert_basic_fields` walk.  All
/// four cert-metadata fields are extracted in one pass — `parse_cert_metadata`
/// is the single place a cert DER is parsed, so callers persist the
/// snapshot rather than re-walking the DER.
///
/// `valid_from` / `valid_to` are RFC 3339 strings (the
/// `BasicCertFields` shape); `refresh_for_fn` parses them once via
/// `chrono::DateTime::parse_from_rfc3339` to compute eligibility.
#[derive(Debug, Clone)]
pub(crate) struct ParsedCertMetadata {
    pub(crate) ski_hex: String,
    pub(crate) valid_from: String,
    pub(crate) valid_to: String,
    pub(crate) subject_dn: String,
    pub(crate) issuer_dn: String,
}

/// Currently-active cert row loaded by `load_active_cert`.
///
/// Only the fields the refresh-eligibility check + key-roll branch
/// decision need.  `valid_to` is parsed at load time (fail-closed: a
/// malformed string returns `MalformedCertMetadata`, not a panic).
#[derive(Debug, Clone)]
pub(crate) struct ActiveCertRow {
    pub(crate) ski_hex: String,
    pub(crate) valid_to: DateTime<Utc>,
}

// ─── Read paths (no tx) ────────────────────────────────────────────────

pub(crate) async fn load_refresh_config(pool: &SqlitePool) -> sqlx::Result<RefreshConfig> {
    // Both columns ship in M1's `cert_provisioning_config`.  W2 adds no
    // new column for this — `timeout_seconds` is reused as the per-URL
    // CMP probe budget (revision 2026-05-05; see plan W2 step 1 schema
    // note).  If a future migration adds more columns, extend the
    // SELECT explicitly — do not `SELECT *` here.
    let row: (i64, i64) = sqlx::query_as(
        "SELECT refresh_within_days, timeout_seconds \
         FROM cert_provisioning_config WHERE id = 1",
    )
    .fetch_one(pool)
    .await?;
    Ok(RefreshConfig {
        refresh_within_days: row.0,
        cmp_request_timeout: Duration::from_secs(row.1.max(1) as u64),
    })
}

pub(crate) async fn load_active_cert(
    pool: &SqlitePool,
    fn_id: &str,
) -> Result<Option<ActiveCertRow>, RefreshError> {
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT ski_hex, valid_to FROM operator_certs \
         WHERE fiscal_number = ? AND active = 1",
    )
    .bind(fn_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| RefreshError::Db(e.to_string()))?;
    match row {
        None => Ok(None),
        Some((_, None)) => Err(RefreshError::MalformedCertMetadata {
            fn_id: fn_id.to_string(),
            field: "valid_to",
        }),
        Some((ski_hex, Some(valid_to_str))) => {
            let valid_to = parse_iso8601(fn_id, "valid_to", &valid_to_str)?;
            Ok(Some(ActiveCertRow { ski_hex, valid_to }))
        }
    }
}

pub(crate) async fn load_ca_urls(pool: &SqlitePool) -> sqlx::Result<Vec<String>> {
    // Read enabled endpoints in priority order from ca_endpoints (M2 W2
    // migration 006).  The `cmp_url` value already includes the
    // `/services/cmp/` path component (W2's whole point — M1's
    // `cert_provisioning_config.{primary,fallback}_cmp_url` columns
    // lack it and are deprecated for M2 routing, even though they
    // remain in the schema as no-op columns until M3+ schema cleanup).
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT cmp_url FROM ca_endpoints \
         WHERE enabled = 1 ORDER BY priority ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(u,)| u).collect())
}

// ─── Public entry point (C2 skeleton; transactional body in C3) ────────

/// Refresh the operator cert for `fn_id` if it is within the
/// configured `refresh_within_days` window.  C2 (this commit) only
/// lands the read-side of the pipeline — the transactional body
/// (in-place UPDATE / key-roll inside `with_immediate`) lands in C3.
///
/// Until C3 lands, any caller hitting the `Refreshed*` branches
/// receives `RefreshError::Db("C3-not-yet-landed: …")` so the gap is
/// loud, not silent.  The eligibility-check branch (`NoChange`) IS
/// fully functional in C2 — that path covers the most common runtime
/// outcome (a cert is checked, found still good, returns NoChange).
pub async fn refresh_for_fn(
    pool: &SqlitePool,
    fn_id: &str,
    provider: Arc<dyn CryptoProvider>,
) -> Result<RefreshOutcome, RefreshError> {
    let cfg = load_refresh_config(pool)
        .await
        .map_err(|e| RefreshError::Db(e.to_string()))?;

    let active =
        load_active_cert(pool, fn_id)
            .await?
            .ok_or_else(|| RefreshError::NoActiveCert {
                fn_id: fn_id.to_string(),
            })?;

    let now = Utc::now();
    if active.valid_to - now > ChronoDuration::days(cfg.refresh_within_days) {
        return Ok(RefreshOutcome::NoChange);
    }

    let urls = load_ca_urls(pool)
        .await
        .map_err(|e| RefreshError::Db(e.to_string()))?;
    if urls.is_empty() {
        return Err(RefreshError::NoEnabledEndpoints);
    }
    let ski_bytes =
        hex_to_ski(&active.ski_hex).map_err(|reason| RefreshError::MalformedCertMetadata {
            fn_id: fn_id.to_string(),
            field: reason,
        })?;

    let new_cert = provider
        .fetch_cert_by_ski(&urls, &ski_bytes, cfg.cmp_request_timeout)
        .await
        .map_err(map_crypto_to_refresh)?;

    let parsed = parse_cert_metadata(&new_cert.0)?;
    // C3 will persist all five fields below into operator_certs; for C2
    // we touch each one to keep the compiler honest about the type's
    // shape and keep the dead-code warning silent.
    let _ = (
        compute_fingerprint(&new_cert.0),
        &parsed.valid_from,
        &parsed.valid_to,
        &parsed.subject_dn,
        &parsed.issuer_dn,
    );

    // C3-fence: writing the new cert (in-place or via key-roll) lands in C3.
    Err(RefreshError::Db(format!(
        "C3-not-yet-landed: cert fetched (ski={ski}) but transactional refresh body \
         is gated on the next commit; rerun once C3 ships",
        ski = parsed.ski_hex,
    )))
}

// ─── Pure helpers (no DB, no network) ──────────────────────────────────

/// Fail-closed hex → 32-byte SKI converter.  Rejects wrong-length input
/// and any non-hex character with a typed reason instead of returning
/// zero bytes (the previous design silently mapped a corrupt `ski_hex`
/// value to all-zeros, which the CMP server would accept and resolve
/// to a wrong cert).
pub(crate) fn hex_to_ski(hex: &str) -> Result<[u8; 32], &'static str> {
    if hex.len() != 64 {
        return Err("ski_hex");
    }
    let bytes = hex.as_bytes();
    let mut out = [0u8; 32];
    for i in 0..32 {
        let hi = hex_digit(bytes[i * 2]).ok_or("ski_hex")?;
        let lo = hex_digit(bytes[i * 2 + 1]).ok_or("ski_hex")?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Parse the four cert-metadata fields the refresh service persists into
/// `operator_certs`.  Wraps `prro_crypto::cms::envelope::
/// parse_cert_basic_fields` (additive helper PRRO_GATE-aty, landed in
/// aebc9f0) so the M2 services layer never inlines its own ASN.1
/// walker.  SKI is computed via `compute_ski` over the cert's
/// compressed pubkey — the same primitive `extract_cert_pubkey_bytes`
/// already exposes.
pub(crate) fn parse_cert_metadata(cert_der: &[u8]) -> Result<ParsedCertMetadata, RefreshError> {
    use prro_crypto::cms::envelope::{
        compute_ski, extract_cert_pubkey_bytes, parse_cert_basic_fields,
    };
    let pubkey = extract_cert_pubkey_bytes(cert_der).map_err(|_| RefreshError::MalformedCert)?;
    let ski = compute_ski(&pubkey);
    let basic = parse_cert_basic_fields(cert_der).map_err(|_| RefreshError::MalformedCert)?;
    Ok(ParsedCertMetadata {
        ski_hex: ski.iter().map(|b| format!("{b:02x}")).collect(),
        valid_from: basic.valid_from,
        valid_to: basic.valid_to,
        subject_dn: basic.subject_dn,
        issuer_dn: basic.issuer_dn,
    })
}

/// SHA-256 hex of the raw cert DER — the value persisted into
/// `operator_certs.cert_fingerprint` so a row can be quickly compared
/// against a freshly-fetched cert without re-parsing the DER.
pub(crate) fn compute_fingerprint(cert_der: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(cert_der))
}

/// Parse an RFC 3339 timestamp held in an `operator_certs` text column.
/// Fails closed: a malformed string returns `MalformedCertMetadata`
/// with the `(fn_id, field)` context the operator needs to diagnose
/// the corrupt row.  Never falls through to `Utc::now()`.
pub(crate) fn parse_iso8601(
    fn_id: &str,
    field: &'static str,
    s: &str,
) -> Result<DateTime<Utc>, RefreshError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| RefreshError::MalformedCertMetadata {
            fn_id: fn_id.to_string(),
            field,
        })
}

/// Map a `CryptoError` from `CryptoProvider::fetch_cert_by_ski` onto a
/// `RefreshError`.  The two well-defined fetch outcomes (`AllUrlsFailed`,
/// `SkiMismatch`) get dedicated `RefreshError` variants; every other
/// crypto-side error is wrapped in `RefreshError::Crypto`.
pub(crate) fn map_crypto_to_refresh(e: CryptoError) -> RefreshError {
    use crate::crypto::FetchKind;
    match e {
        CryptoError::CertFetch {
            reason: FetchKind::AllUrlsFailed,
        } => RefreshError::AllUrlsFailed,
        CryptoError::CertFetch {
            reason: FetchKind::SkiMismatch,
        } => RefreshError::SkiMismatch,
        other => RefreshError::Crypto(other),
    }
}

// ─── Unit tests for the pure helpers ───────────────────────────────────

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn hex_to_ski_accepts_valid_64char_hex() {
        let ski = hex_to_ski("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
            .expect("valid 64-char lowercase hex must succeed");
        assert_eq!(ski[0], 0x01);
        assert_eq!(ski[31], 0xef);
    }

    #[test]
    fn hex_to_ski_accepts_uppercase() {
        let ski = hex_to_ski("0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF")
            .expect("valid 64-char uppercase hex must succeed");
        assert_eq!(ski[15], 0xef);
    }

    #[test]
    fn hex_to_ski_rejects_short_input() {
        assert_eq!(hex_to_ski("0123").unwrap_err(), "ski_hex");
        assert_eq!(hex_to_ski("").unwrap_err(), "ski_hex");
    }

    #[test]
    fn hex_to_ski_rejects_long_input() {
        let too_long: String = (0..65).map(|_| 'a').collect();
        assert_eq!(hex_to_ski(&too_long).unwrap_err(), "ski_hex");
    }

    #[test]
    fn hex_to_ski_rejects_non_hex_chars() {
        // Wrong-but-correct-length input with a 'z' in the middle.
        let bad = "0123456789abcdef0z23456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(bad.len(), 64);
        assert_eq!(hex_to_ski(bad).unwrap_err(), "ski_hex");
    }

    #[test]
    fn compute_fingerprint_is_deterministic_64_hex() {
        let der = b"\x30\x82\x00\x10just-a-fixture";
        let fp1 = compute_fingerprint(der);
        let fp2 = compute_fingerprint(der);
        assert_eq!(fp1, fp2);
        assert_eq!(fp1.len(), 64); // SHA-256 → 32 bytes → 64 hex chars
        assert!(fp1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn parse_iso8601_round_trip() {
        let dt = parse_iso8601("FN-1", "valid_to", "2026-05-04T10:00:00Z").unwrap();
        assert_eq!(dt.to_rfc3339(), "2026-05-04T10:00:00+00:00");
    }

    #[test]
    fn parse_iso8601_rejects_malformed() {
        let err = parse_iso8601("FN-1", "valid_to", "not-a-date").expect_err("must reject");
        match err {
            RefreshError::MalformedCertMetadata { fn_id, field } => {
                assert_eq!(fn_id, "FN-1");
                assert_eq!(field, "valid_to");
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn map_crypto_to_refresh_routes_known_outcomes() {
        use crate::crypto::FetchKind;
        let e = map_crypto_to_refresh(CryptoError::CertFetch {
            reason: FetchKind::AllUrlsFailed,
        });
        assert!(matches!(e, RefreshError::AllUrlsFailed));

        let e = map_crypto_to_refresh(CryptoError::CertFetch {
            reason: FetchKind::SkiMismatch,
        });
        assert!(matches!(e, RefreshError::SkiMismatch));

        let e = map_crypto_to_refresh(CryptoError::CertFetch {
            reason: FetchKind::TransportError,
        });
        assert!(matches!(e, RefreshError::Crypto(_)));
    }
}
