//! Async cert refresh service (M2/W2).
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
//!    fiscal_number=? AND ski_hex=? AND active=1` (the `ski_hex`
//!    bind is the stale-read guard — if a concurrent writer flipped
//!    the active cert between our read and this tx, the WHERE
//!    filters us out → rows_affected==0 → typed error → ROLLBACK,
//!    no audit emitted), then `UPDATE … SET active=1 WHERE
//!    ski_hex=?`, then audit_log INSERT.  Atomic + idempotent on
//!    retry (no orphan staged-row window).
//! 8. Return `RefreshedInPlace { ski }` or `RefreshedKeyRoll { old, new }`.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use sqlx::SqlitePool;

use crate::crypto::{CryptoError, CryptoProvider};
use crate::db::tx::with_immediate;

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

// ─── Public entry point ────────────────────────────────────────────────

/// Refresh the operator cert for `fn_id` if it is within the configured
/// `refresh_within_days` window.
///
/// Pipeline (per ADR-M2-4 + ADR-M2-6):
///
/// 1. Read-only: load config, active cert, CA URL list — all OUTSIDE
///    any tx (invariant #1: no network/crypto inside a tx).
/// 2. Eligibility check: `valid_to - now > refresh_within_days` →
///    `NoChange`.
/// 3. Fetch the new cert via `provider.fetch_cert_by_ski` — also
///    outside any tx.
/// 4. Parse the new cert's metadata (SKI, validity, DNs).
/// 5. Branch on SKI:
///    - new SKI == active SKI → `in_place_refresh` (single short tx;
///      `rows_affected == 1` strict).
///    - new SKI != active SKI → `key_roll_atomic` inside ONE
///      `with_immediate` block (stage + flip + audit; all three
///      `rows_affected == 1` strict).
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

    // Network call OUTSIDE any tx — invariant #1.  W5 will static-assert
    // that no fetch happens inside `with_immediate`; this is the
    // "before" half of that contract.
    let new_cert = provider
        .fetch_cert_by_ski(&urls, &ski_bytes, cfg.cmp_request_timeout)
        .await
        .map_err(map_crypto_to_refresh)?;

    // Parse OUTSIDE any tx — `parse_cert_basic_fields` reads cert DER
    // bytes; no DB / network traffic.
    let parsed = parse_cert_metadata(&new_cert.0)?;

    // Branch decision is made on SKI equality.  Both branches now
    // perform writes; either path drops into a tx after this point.
    if parsed.ski_hex == active.ski_hex {
        in_place_refresh(pool, &new_cert.0, &parsed).await?;
        Ok(RefreshOutcome::RefreshedInPlace {
            ski: parsed.ski_hex,
        })
    } else {
        let ski_old = active.ski_hex.clone();
        let ski_new = parsed.ski_hex.clone();
        key_roll_atomic(pool, fn_id, &active.ski_hex, &new_cert.0, &parsed).await?;
        Ok(RefreshOutcome::RefreshedKeyRoll { ski_old, ski_new })
    }
}

/// Same-SKI refresh: cert reissued, key unchanged.
///
/// One short tx UPDATEs the existing `active=1` row.  Asserts
/// `rows_affected == 1` — a 0-row outcome means the active row was
/// concurrently deactivated by another writer (or the partial unique
/// index `ux_op_certs_active_per_fn` was violated upstream); both are
/// real bugs we surface as a typed error rather than absorb silently.
///
/// Wrapped in `with_immediate` because (a) it involves a write
/// (BEGIN IMMEDIATE acquires the RESERVED lock from the first
/// statement; spec decision #39) and (b) consistency with the
/// key-roll path simplifies the W5 static check.
async fn in_place_refresh(
    pool: &SqlitePool,
    new_cert_der: &[u8],
    parsed: &ParsedCertMetadata,
) -> Result<(), RefreshError> {
    let new_cert = new_cert_der.to_vec();
    let parsed = parsed.clone();
    with_immediate(pool, move |conn| {
        Box::pin(async move {
            let now_iso = Utc::now().to_rfc3339();
            let r = sqlx::query(
                "UPDATE operator_certs \
                 SET cert_der = ?, cert_fingerprint = ?, \
                     valid_from = ?, valid_to = ?, \
                     subject_dn = ?, issuer_dn = ?, \
                     fetched_at = ?, last_refresh_at = ? \
                 WHERE ski_hex = ? AND active = 1",
            )
            .bind(&new_cert)
            .bind(compute_fingerprint(&new_cert))
            .bind(&parsed.valid_from)
            .bind(&parsed.valid_to)
            .bind(&parsed.subject_dn)
            .bind(&parsed.issuer_dn)
            .bind(&now_iso)
            .bind(&now_iso)
            .bind(&parsed.ski_hex)
            .execute(&mut *conn)
            .await?;
            if r.rows_affected() != 1 {
                return Err(anyhow::anyhow!(
                    "in_place_refresh: expected 1 row, got {} for ski={}",
                    r.rows_affected(),
                    parsed.ski_hex
                ));
            }
            Ok(())
        })
    })
    .await
    .map_err(|e| RefreshError::Db(e.to_string()))
}

/// Key-roll: new cert carries a different SKI than the current active
/// row.  Stage + flip + audit run inside ONE `with_immediate` tx so the
/// operation is atomic AND idempotent on retry — a crash before COMMIT
/// leaves no orphan staged row (tx never committed); a crash AFTER
/// COMMIT means the row is already there with active=1, so a
/// subsequent refresh_for_fn call sees `valid_to - now >
/// refresh_within_days` and returns NoChange.
///
/// The stage uses `INSERT … ON CONFLICT(ski_hex) DO UPDATE … WHERE
/// operator_certs.fiscal_number = excluded.fiscal_number AND
/// operator_certs.active = 0` — so a stale staged row from a prior
/// interrupted refresh (same fn, active=0) is harmlessly overwritten,
/// while a foreign-owned `ski_hex` (different fn) or an `active=1`
/// row is REFUSED (sqlite_constraint or `rows_affected==0` → typed
/// error → with_immediate ROLLBACKs).  Why NOT `INSERT OR REPLACE`:
/// REPLACE is DELETE+INSERT and would silently wipe a foreign-owned
/// cert row, destroying ownership / metadata for an unrelated
/// operator.
// `pub(crate)` so the regression test that simulates a stale-read race
// can drive this function directly with a deliberately stale
// `active_ski` argument — the in-memory ski we *thought* was active at
// read time but which a concurrent writer has since swapped.
pub(crate) async fn key_roll_atomic(
    pool: &SqlitePool,
    fn_id: &str,
    active_ski: &str,
    new_cert_der: &[u8],
    parsed: &ParsedCertMetadata,
) -> Result<(), RefreshError> {
    let fn_id = fn_id.to_string();
    let active_ski = active_ski.to_string();
    let new_cert = new_cert_der.to_vec();
    let parsed = parsed.clone();
    with_immediate(pool, move |conn| {
        Box::pin(async move {
            let now_iso = Utc::now().to_rfc3339();

            // Stage at active=0.  ON CONFLICT(ski_hex) guard restricts
            // the UPDATE to a same-fiscal_number, active=0 row only —
            // foreign rows or active=1 rows produce rows_affected==0
            // (the WHERE filter excluded them) and we abort the tx.
            let stage_result = sqlx::query(
                "INSERT INTO operator_certs( \
                     ski_hex, fiscal_number, cert_fingerprint, cert_der, \
                     valid_from, valid_to, subject_dn, issuer_dn, \
                     fetched_at, source, active) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'cmp', 0) \
                 ON CONFLICT(ski_hex) DO UPDATE SET \
                     cert_fingerprint = excluded.cert_fingerprint, \
                     cert_der         = excluded.cert_der, \
                     valid_from       = excluded.valid_from, \
                     valid_to         = excluded.valid_to, \
                     subject_dn       = excluded.subject_dn, \
                     issuer_dn        = excluded.issuer_dn, \
                     fetched_at       = excluded.fetched_at, \
                     source           = excluded.source \
                 WHERE operator_certs.fiscal_number = excluded.fiscal_number \
                   AND operator_certs.active = 0",
            )
            .bind(&parsed.ski_hex)
            .bind(&fn_id)
            .bind(compute_fingerprint(&new_cert))
            .bind(&new_cert)
            .bind(&parsed.valid_from)
            .bind(&parsed.valid_to)
            .bind(&parsed.subject_dn)
            .bind(&parsed.issuer_dn)
            .bind(&now_iso)
            .execute(&mut *conn)
            .await?;
            if stage_result.rows_affected() != 1 {
                return Err(anyhow::anyhow!(
                    "key-roll stage: ski_hex {} already exists for a different \
                     fiscal_number or is already active=1; refusing to overwrite \
                     (rows_affected={})",
                    parsed.ski_hex,
                    stage_result.rows_affected()
                ));
            }

            // Deactivate the old active row.  WHERE clause is bound on
            // BOTH `fiscal_number` AND the EXACT `ski_hex` we read at
            // the start of refresh_for_fn (before fetch + parse).  This
            // is the stale-read guard: if another refresh / operator
            // action concurrently swapped the active cert between our
            // read and this with_immediate block, the WHERE filters us
            // out → rows_affected == 0 → typed error → ROLLBACK → the
            // already-newly-active row stays untouched and no audit
            // entry is emitted (the INSERT below never executes).
            // Without binding ski_hex, this UPDATE would silently
            // deactivate whatever is currently active and the next
            // UPDATE would promote a cert derived from a stale read
            // — a real wrong-cert-on-the-wire bug.
            let r1 = sqlx::query(
                "UPDATE operator_certs SET active = 0 \
                 WHERE fiscal_number = ? AND ski_hex = ? AND active = 1",
            )
            .bind(&fn_id)
            .bind(&active_ski)
            .execute(&mut *conn)
            .await?;
            if r1.rows_affected() != 1 {
                return Err(anyhow::anyhow!(
                    "key-roll deactivate: stale-read or concurrent swap detected \
                     (fn={fn_id} expected_active_ski={active_ski} rows_affected={})",
                    r1.rows_affected()
                ));
            }

            // Activate the new row.
            let r2 = sqlx::query("UPDATE operator_certs SET active = 1 WHERE ski_hex = ?")
                .bind(&parsed.ski_hex)
                .execute(&mut *conn)
                .await?;
            if r2.rows_affected() != 1 {
                return Err(anyhow::anyhow!(
                    "key-roll activate: expected 1 row, got {}",
                    r2.rows_affected()
                ));
            }

            // Audit trail in the same tx.
            sqlx::query(
                "INSERT INTO audit_log( \
                     entity_type, entity_id, event_type, severity, actor, event_payload_json) \
                 VALUES ('fn', ?, 'cert_refresh_key_roll', 'INFO', 'cert_refresher', ?)",
            )
            .bind(&fn_id)
            .bind(format!(
                r#"{{"ski_old":"{active_ski}","ski_new":"{}"}}"#,
                parsed.ski_hex
            ))
            .execute(&mut *conn)
            .await?;
            Ok(())
        })
    })
    .await
    .map_err(|e| RefreshError::Db(e.to_string()))
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

// ─── Stale-read race regression (in-tree so it can drive the pub(crate)
//      key_roll_atomic with a deliberately stale active_ski) ───────────

#[cfg(test)]
mod stale_read_tests {
    use super::*;

    /// Vendored DSTU 4145 cert; same fixture
    /// `cert_refresher_branches.rs` uses for branch coverage.  Lets us
    /// produce a `ParsedCertMetadata` from a real cert without hand-
    /// crafting one (`ParsedCertMetadata` is `pub(crate)`).
    const FIXTURE_CERT_DER: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../prro_crypto/node_modules/jkurwa/test/data/SELF_SIGNED_ENC_6929.cer"
    ));

    async fn fresh_pool_with_fn(fn_id: &str) -> (tempfile::TempDir, sqlx::SqlitePool) {
        let dir = tempfile::tempdir().expect("tempdir");
        let pool = crate::db::open_pool(&dir.path().join("m.db"))
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

    /// Stage an `operator_certs` row at the given `ski_hex` + `active`
    /// flag.  Used to simulate the post-race state where another
    /// writer has already swapped the active cert.
    async fn stage_cert(pool: &sqlx::SqlitePool, fn_id: &str, ski_hex: &str, active: i64) {
        let valid_to_soon = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
        sqlx::query(
            "INSERT INTO operator_certs( \
                 ski_hex, fiscal_number, cert_fingerprint, cert_der, \
                 valid_from, valid_to, subject_dn, issuer_dn, \
                 fetched_at, source, active) \
             VALUES (?, ?, 'fp', x'00', \
                     '2020-01-01T00:00:00Z', ?, \
                     'CN=stage', 'CN=stage-issuer', \
                     '2020-01-01T00:00:00Z', 'manual', ?)",
        )
        .bind(ski_hex)
        .bind(fn_id)
        .bind(&valid_to_soon)
        .bind(active)
        .execute(pool)
        .await
        .expect("stage cert row");
    }

    /// **Regression — stale-read race in `key_roll_atomic`.**
    ///
    /// Models the race where `refresh_for_fn` reads `active_ski = X`,
    /// then performs a network fetch + parse (during which time another
    /// writer flips the active cert from X → Y), then enters
    /// `with_immediate` armed with the stale `active_ski = X`.
    ///
    /// Pre-fix, the deactivate UPDATE used `WHERE fn = ? AND active = 1`
    /// — it would silently deactivate Y (the freshly-active row),
    /// activate the cert derived from the stale X read, and emit an
    /// audit entry citing X→new — a wrong-cert-on-the-wire bug.
    ///
    /// Post-fix, the deactivate UPDATE uses
    /// `WHERE fn = ? AND ski_hex = ? AND active = 1` bound on `X`.
    /// The WHERE filters us out (current active is Y, not X) →
    /// rows_affected == 0 → typed `RefreshError::Db` → `with_immediate`
    /// ROLLBACKs.  The newly-active row Y stays untouched; no audit
    /// entry is emitted (the audit INSERT runs AFTER the deactivate
    /// guard inside the same tx).
    #[tokio::test]
    async fn key_roll_atomic_detects_stale_active_ski_and_rolls_back() {
        let fn_id = "1234567890";
        let (_d, pool) = fresh_pool_with_fn(fn_id).await;

        // The "winner" of the race: a different ski_hex took active=1
        // between our read and our with_immediate block.
        let stale_ski = "a".repeat(64); // what refresh_for_fn THOUGHT was active
        let winner_ski = "b".repeat(64); // what actually IS active now
        stage_cert(&pool, fn_id, &winner_ski, 1).await;

        // ParsedCertMetadata: build via the public path so the SKI is
        // a real DSTU SKI distinct from both stale and winner.
        let parsed = parse_cert_metadata(FIXTURE_CERT_DER)
            .expect("fixture must parse via prro_crypto::parse_cert_basic_fields");
        assert_ne!(parsed.ski_hex, stale_ski);
        assert_ne!(parsed.ski_hex, winner_ski);

        // Drive key_roll_atomic with the stale ski.  Must surface a
        // typed RefreshError::Db (the with_immediate maps the
        // anyhow::anyhow! bail into RefreshError::Db) and ROLLBACK.
        let err = key_roll_atomic(&pool, fn_id, &stale_ski, FIXTURE_CERT_DER, &parsed)
            .await
            .expect_err("stale active_ski must surface a typed error");
        let msg = format!("{err:?}");
        assert!(
            matches!(err, RefreshError::Db(_)),
            "expected Db-wrapped failure; got {msg}"
        );
        // Diagnostic substring: the error message names BOTH the
        // expected-stale ski AND "stale-read or concurrent swap".
        // Lets an operator immediately classify the failure mode.
        assert!(
            msg.contains("stale-read") || msg.contains("concurrent swap"),
            "diagnostic must name the stale-read class; got {msg}"
        );
        assert!(
            msg.contains(&stale_ski),
            "diagnostic must name the expected_active_ski; got {msg}"
        );

        // Post-conditions: no DB corruption.
        //   - winner_ski is STILL the only active=1 row
        //   - the stage row from the stale-driven attempt is NOT present
        //     (with_immediate ROLLBACKed before COMMIT)
        //   - no audit_log entry for cert_refresh_key_roll
        let active_rows: Vec<(String, i64)> =
            sqlx::query_as("SELECT ski_hex, active FROM operator_certs WHERE fiscal_number = ?")
                .bind(fn_id)
                .fetch_all(&pool)
                .await
                .unwrap();
        let actives: Vec<&(String, i64)> = active_rows.iter().filter(|(_, a)| *a == 1).collect();
        assert_eq!(
            actives.len(),
            1,
            "exactly one active row must remain after rollback; got {active_rows:?}"
        );
        assert_eq!(
            actives[0].0, winner_ski,
            "active row must STILL be the winner; got {active_rows:?}"
        );
        // Stage row would have ski_hex == parsed.ski_hex.  Must NOT
        // be present after rollback.
        let stage_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM operator_certs WHERE fiscal_number = ? AND ski_hex = ?",
        )
        .bind(fn_id)
        .bind(&parsed.ski_hex)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            stage_count, 0,
            "rolled-back stage row must not be visible post-tx"
        );
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_log WHERE event_type = 'cert_refresh_key_roll'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            audit_count, 0,
            "no audit entry must be emitted when the tx ROLLBACKs"
        );
    }
}
