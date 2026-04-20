//! Thin adapter over `prro_crypto::cms::CmsSigner` — signs cp1251 XML bytes
//! using DSTU 4145-2002, optionally adding RFC 3161 TSP timestamp.
//!
//! TSP URL is resolved from `ca_endpoints` table by substring-matching
//! the cert issuer DN — never passed directly by the caller.

use prro_crypto::cms::{builder::{CmsBuildOptions, CmsError, CmsSigner}, signer::{DstuInProcessSigner, RawSigner, SignerError}, CmsProfile};
use rusqlite::OptionalExtension;

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum CmsAdapterError {
    #[error("CMS sign error: {0}")]
    Cms(#[from] CmsError),
    #[error("TSP fetch error: {0}")]
    Tsp(String),
    #[error("no TSP endpoint found for issuer DN {issuer_dn:?}")]
    NoTspMapping { issuer_dn: String },
    #[error("cert issuer DN extraction failed: {0}")]
    CertParse(String),
    #[error("DB query: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("signing: {0}")]
    Sign(#[from] SignerError),
}

// ─── Context ─────────────────────────────────────────────────────────────────

pub struct CmsSignCtx<'a> {
    /// When true, a TSP timestamp is added; URL resolved via ca_endpoints.
    pub tsp_enabled:    bool,
    /// TSP request timeout in milliseconds.
    pub tsp_timeout_ms: u64,
    /// Signing time to embed in signedAttrs.
    pub now:            time::OffsetDateTime,
    /// Open DB connection for ca_endpoints lookup (required iff tsp_enabled).
    pub conn:           Option<&'a rusqlite::Connection>,
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Sign `content` (cp1251 XML bytes) and return CAdES-BES or CAdES-T DER.
///
/// When `ctx.tsp_enabled = true`, the TSP URL is resolved from the
/// `ca_endpoints` table by matching a substring of the cert issuer DN.
pub fn sign_content(
    content:  &[u8],
    signer:   &DstuInProcessSigner,
    cert_der: &[u8],
    ctx:      &CmsSignCtx<'_>,
) -> Result<Vec<u8>, CmsAdapterError> {
    let profile = CmsProfile::default(); // DSTU 4145 + GOST 34.311-95

    let signing_time = Some(std::time::UNIX_EPOCH
        + std::time::Duration::from_secs(ctx.now.unix_timestamp() as u64));

    let cms_signer = CmsSigner {
        cert_der,
        signer: signer as &dyn RawSigner,
        profile,
    };

    let opts = CmsBuildOptions { attached: false, signing_time };

    if ctx.tsp_enabled {
        let conn = ctx.conn.ok_or_else(|| CmsAdapterError::Db(
            rusqlite::Error::InvalidParameterName("conn required when tsp_enabled".into())
        ))?;

        let issuer_dn = extract_issuer_dn(cert_der)?;
        let tsp_url   = resolve_tsp_url(conn, &issuer_dn)?;
        let timeout   = std::time::Duration::from_millis(ctx.tsp_timeout_ms);
        // sign_with_tsp correctly hashes signature_value (not full CMS DER) for TSP messageImprint
        let sig = cms_signer.sign_with_tsp(content, opts, &tsp_url, timeout)?;
        Ok(sig.cms_der)
    } else {
        let sig = cms_signer.sign_with(content, opts)?;
        Ok(sig.cms_der)
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Query `ca_endpoints` for a TSP URL matching the cert's issuer DN.
///
/// Uses case-insensitive substring match: the stored `issuer_pattern` is
/// a distinctive fragment of the issuer DN (e.g. "ацск" or "privatbank").
pub fn resolve_tsp_url(
    conn:      &rusqlite::Connection,
    issuer_dn: &str,
) -> Result<String, CmsAdapterError> {
    let lower_dn = issuer_dn.to_lowercase();
    let url: Option<String> = conn.query_row(
        "SELECT tsp_url FROM ca_endpoints
          WHERE enabled = 1
            AND tsp_url IS NOT NULL
            AND INSTR(?, lower(issuer_pattern)) > 0
          ORDER BY priority ASC
          LIMIT 1",
        rusqlite::params![lower_dn],
        |row| row.get(0),
    ).optional()?;

    url.ok_or_else(|| CmsAdapterError::NoTspMapping { issuer_dn: issuer_dn.to_string() })
}

/// Extract the issuer DN string from a DER-encoded X.509 certificate.
/// Uses a minimal ASN.1 walk — no dependency on an X.509 library.
fn extract_issuer_dn(cert_der: &[u8]) -> Result<String, CmsAdapterError> {
    use prro_crypto::cms::asn1_util as a1;

    let err = |msg: &str| CmsAdapterError::CertParse(msg.to_string());

    // SEQUENCE (Certificate) → SEQUENCE (tbsCertificate)
    let (_, tbs_start) = a1::read_tlv(cert_der, 0)
        .map_err(|e| CmsAdapterError::CertParse(e.to_string()))?;
    let (_, tbs_inner) = a1::read_tlv(cert_der, tbs_start)
        .map_err(|e| CmsAdapterError::CertParse(e.to_string()))?;

    let mut pos = tbs_inner;
    // Skip optional version [0]
    if a1::peek_tag(cert_der, pos)
        .map_err(|e| CmsAdapterError::CertParse(e.to_string()))? == 0xa0
    {
        let (end, _) = a1::read_tlv(cert_der, pos)
            .map_err(|e| CmsAdapterError::CertParse(e.to_string()))?;
        pos = end;
    }
    // Skip serialNumber, signature, then read issuer (4th field)
    for _ in 0..2 {
        let (end, _) = a1::read_tlv(cert_der, pos)
            .map_err(|e| CmsAdapterError::CertParse(e.to_string()))?;
        pos = end;
    }
    // issuer is next — read its raw bytes and format as hex for matching
    let (issuer_end, issuer_inner) = a1::read_tlv(cert_der, pos)
        .map_err(|e| CmsAdapterError::CertParse(e.to_string()))?;

    if issuer_end > cert_der.len() {
        return Err(err("issuer sequence extends beyond cert DER"));
    }

    // Extract printable string content from issuer DN for pattern matching.
    // Walk the RDN SETs to collect all string values.
    let mut dn_parts = Vec::new();
    let mut rdn_pos = issuer_inner;
    while rdn_pos < issuer_end {
        let Ok((set_end, set_inner)) = a1::read_tlv(cert_der, rdn_pos) else { break };
        let Ok((atv_end, atv_inner)) = a1::read_tlv(cert_der, set_inner) else {
            rdn_pos = set_end; continue;
        };
        // Skip OID, read value
        let Ok((oid_end, _)) = a1::read_tlv(cert_der, atv_inner) else {
            rdn_pos = set_end; continue;
        };
        if let Ok((val_end, val_inner)) = a1::read_tlv(cert_der, oid_end) {
            if val_inner < val_end && val_end <= cert_der.len() {
                let bytes = &cert_der[val_inner..val_end];
                if let Ok(s) = std::str::from_utf8(bytes) {
                    dn_parts.push(s.to_string());
                } else {
                    // CP1251 fallback for legacy Ukrainian CAs that encode RDN values in Windows-1251
                    let (cow, _, _) = encoding_rs::WINDOWS_1251.decode(bytes);
                    dn_parts.push(cow.into_owned());
                }
            }
            let _ = atv_end;
        }
        rdn_pos = set_end;
    }

    Ok(dn_parts.join(", "))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_db_with_endpoints() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE ca_endpoints (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                cmp_url TEXT, tsp_url TEXT, ocsp_url TEXT,
                issuer_pattern TEXT,
                priority INTEGER NOT NULL DEFAULT 100,
                enabled INTEGER NOT NULL DEFAULT 1
            );
            INSERT INTO ca_endpoints VALUES
                (1,'acskidd',NULL,'http://acskidd.gov.ua/services/tsp/',NULL,'ацск іддс',10,1),
                (2,'privatbank',NULL,'https://acsk.privatbank.ua/services/tsp/',NULL,'приватбанк',20,1),
                (3,'disabled',NULL,'http://disabled.example.com/tsp/',NULL,'disabled_ca',30,0);",
        ).unwrap();
        conn
    }

    #[test]
    fn resolve_tsp_acskidd() {
        let conn = make_db_with_endpoints();
        let url = resolve_tsp_url(&conn, "CN=Тест, O=АЦСК ІДДС, C=UA").unwrap();
        assert_eq!(url, "http://acskidd.gov.ua/services/tsp/");
    }

    #[test]
    fn resolve_tsp_privatbank() {
        let conn = make_db_with_endpoints();
        let url = resolve_tsp_url(&conn, "CN=Test Signer, O=ПРИВАТБАНК ACSK, C=UA").unwrap();
        assert_eq!(url, "https://acsk.privatbank.ua/services/tsp/");
    }

    #[test]
    fn resolve_tsp_unknown_issuer_returns_error() {
        let conn = make_db_with_endpoints();
        let err = resolve_tsp_url(&conn, "CN=Unknown, O=UnknownCA, C=UA").unwrap_err();
        assert!(matches!(err, CmsAdapterError::NoTspMapping { .. }),
                "expected NoTspMapping, got {err}");
    }

    #[test]
    fn resolve_tsp_disabled_endpoint_not_matched() {
        let conn = make_db_with_endpoints();
        // "disabled_ca" has enabled=0 — must not match
        let err = resolve_tsp_url(&conn, "CN=Test, O=Disabled_CA, C=UA").unwrap_err();
        assert!(matches!(err, CmsAdapterError::NoTspMapping { .. }));
    }

    #[test]
    fn resolve_tsp_priority_order() {
        let conn = make_db_with_endpoints();
        // Pattern that matches both acskidd (priority=10) and privatbank (priority=20)
        // via both patterns... actually they won't both match a single DN easily.
        // Just verify that acskidd (lower priority number) wins if both matched:
        // priority ASC means smallest number = highest priority
        let url = resolve_tsp_url(&conn, "O=АЦСК ІДДС").unwrap();
        assert_eq!(url, "http://acskidd.gov.ua/services/tsp/");
    }

    // ── A. TSP URL NULL in database — row must not match ─────────────────────

    #[test]
    fn resolve_tsp_null_tsp_url_not_matched() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE ca_endpoints (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                cmp_url TEXT, tsp_url TEXT, ocsp_url TEXT,
                issuer_pattern TEXT,
                priority INTEGER NOT NULL DEFAULT 100,
                enabled INTEGER NOT NULL DEFAULT 1
            );
            -- Row with NULL tsp_url: must NOT be returned even if pattern matches
            INSERT INTO ca_endpoints VALUES
                (1,'null_tsp',NULL,NULL,NULL,'testissuer',10,1);",
        ).unwrap();

        let err = resolve_tsp_url(&conn, "CN=Test, O=TestIssuer, C=UA").unwrap_err();
        assert!(
            matches!(err, CmsAdapterError::NoTspMapping { .. }),
            "NULL tsp_url must not be returned: got {err}"
        );
    }

    // ── B. Empty issuer_pattern matches ANY issuer ────────────────────────────

    #[test]
    fn resolve_tsp_empty_pattern_matches_any_issuer() {
        // INSTR(haystack, '') is always 1 in SQLite — empty string always found.
        // An empty issuer_pattern is an overly-broad match — verify this behavior.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE ca_endpoints (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                cmp_url TEXT, tsp_url TEXT, ocsp_url TEXT,
                issuer_pattern TEXT,
                priority INTEGER NOT NULL DEFAULT 100,
                enabled INTEGER NOT NULL DEFAULT 1
            );
            INSERT INTO ca_endpoints VALUES
                (1,'catch_all',NULL,'https://catch-all.example.com/tsp/',NULL,'',10,1);",
        ).unwrap();

        // Any issuer DN must match the empty pattern
        let url = resolve_tsp_url(&conn, "CN=Anyone, O=AnyOrg, C=UA").unwrap();
        assert_eq!(
            url, "https://catch-all.example.com/tsp/",
            "empty issuer_pattern must match any issuer DN via INSTR"
        );
    }

    // ── C. extract_issuer_dn with empty bytes ─────────────────────────────────

    #[test]
    fn extract_issuer_dn_empty_input_returns_error() {
        let result = extract_issuer_dn(&[]);
        assert!(
            result.is_err(),
            "empty DER input must return CertParse error"
        );
        assert!(
            matches!(result.unwrap_err(), CmsAdapterError::CertParse(_)),
            "error type must be CertParse"
        );
    }

    // ── D. extract_issuer_dn with truncated DER ───────────────────────────────

    #[test]
    fn extract_issuer_dn_truncated_to_few_bytes_returns_error() {
        // A real X.509 cert DER starts with SEQUENCE tag (0x30) followed by length.
        // Truncated to just 4 bytes cannot contain a valid TBS structure.
        let truncated = [0x30u8, 0x82, 0x04, 0x00]; // SEQUENCE header only, no content
        let result = extract_issuer_dn(&truncated);
        assert!(
            result.is_err(),
            "truncated DER (4 bytes) must return error, got Ok"
        );
    }

    // ── E. issuer_pattern with LIKE metacharacters — INSTR treats them literally

    #[test]
    fn resolve_tsp_issuer_pattern_with_like_metacharacters_does_not_inject() {
        // With INSTR, "%" is treated literally — only matches if '%' appears in the issuer DN.
        // (With the old LIKE approach, issuer_pattern = "%" would match everything.)
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE ca_endpoints (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                cmp_url TEXT, tsp_url TEXT, ocsp_url TEXT,
                issuer_pattern TEXT,
                priority INTEGER NOT NULL DEFAULT 100,
                enabled INTEGER NOT NULL DEFAULT 1
            );
            -- Pattern with LIKE wildcard — with INSTR this must only match if '%' is in the DN
            INSERT INTO ca_endpoints VALUES
                (1,'percent_ca',NULL,'https://percent.example.com/tsp/',NULL,'%',10,1);",
        ).unwrap();

        // A normal issuer DN (no '%') must NOT match this pattern with INSTR
        let result = resolve_tsp_url(&conn, "CN=Normal CA, O=NormalOrg, C=UA");
        assert!(
            result.is_err(),
            "LIKE metachar '%' in issuer_pattern must not match via INSTR (treated literally)"
        );
    }

    #[test]
    fn resolve_tsp_issuer_dn_with_percent_char_matches_percent_pattern() {
        // Inverse: if the issuer DN literally contains '%', the pattern '%' matches.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE ca_endpoints (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                cmp_url TEXT, tsp_url TEXT, ocsp_url TEXT,
                issuer_pattern TEXT,
                priority INTEGER NOT NULL DEFAULT 100,
                enabled INTEGER NOT NULL DEFAULT 1
            );
            INSERT INTO ca_endpoints VALUES
                (1,'percent_ca',NULL,'https://percent.example.com/tsp/',NULL,'%',10,1);",
        ).unwrap();

        // Issuer DN that literally contains '%' must match the '%' pattern
        let url = resolve_tsp_url(&conn, "CN=CA 100% Trusted, O=Org, C=UA");
        assert!(url.is_ok(), "DN containing '%' must match pattern '%' via INSTR");
    }

    // ── F. Priority ordering: lower number wins ───────────────────────────────

    #[test]
    fn resolve_tsp_multiple_matching_patterns_lowest_priority_wins() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE ca_endpoints (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                cmp_url TEXT, tsp_url TEXT, ocsp_url TEXT,
                issuer_pattern TEXT,
                priority INTEGER NOT NULL DEFAULT 100,
                enabled INTEGER NOT NULL DEFAULT 1
            );
            -- Two rows where both patterns match the same issuer DN:
            -- 'test' appears in 'testcorp' and 'corp' also appears in 'testcorp'
            INSERT INTO ca_endpoints VALUES
                (1,'high_priority',NULL,'https://first.example.com/tsp/',NULL,'test',10,1),
                (2,'low_priority', NULL,'https://second.example.com/tsp/',NULL,'corp',20,1);",
        ).unwrap();

        // priority=10 wins over priority=20 (ORDER BY priority ASC → smallest first)
        let url = resolve_tsp_url(&conn, "testcorp").unwrap();
        assert_eq!(
            url, "https://first.example.com/tsp/",
            "priority=10 must beat priority=20 (ASC order, smaller = higher priority)"
        );
    }
}
