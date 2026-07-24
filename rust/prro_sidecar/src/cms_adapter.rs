//! Helpers for TSP URL resolution and cert issuer DN extraction.
//!
//! TSP URL is resolved from `ca_endpoints` by substring-matching
//! the cert issuer DN — never passed directly by the caller.

use rusqlite::OptionalExtension;

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum CmsAdapterError {
    #[error("no TSP endpoint found for issuer DN {issuer_dn:?}")]
    NoTspMapping { issuer_dn: String },
    #[error("cert issuer DN extraction failed: {0}")]
    CertParse(String),
    #[error("DB query: {0}")]
    Db(#[from] rusqlite::Error),
}

/// Query `ca_endpoints` for a TSP URL matching the cert's issuer DN.
///
/// Uses case-insensitive substring match: the stored `issuer_pattern` is
/// a distinctive fragment of the issuer DN (e.g. "ацск" or "privatbank").
pub fn resolve_tsp_url(
    conn: &rusqlite::Connection,
    issuer_dn: &str,
) -> Result<String, CmsAdapterError> {
    let lower_dn = issuer_dn.to_lowercase();
    let url: Option<String> = conn
        .query_row(
            "SELECT tsp_url FROM ca_endpoints
          WHERE enabled = 1
            AND tsp_url IS NOT NULL
            AND INSTR(?, lower(issuer_pattern)) > 0
          ORDER BY priority ASC
          LIMIT 1",
            rusqlite::params![lower_dn],
            |row| row.get(0),
        )
        .optional()?;

    url.ok_or_else(|| CmsAdapterError::NoTspMapping {
        issuer_dn: issuer_dn.to_string(),
    })
}

/// Extract the issuer DN string from a DER-encoded X.509 certificate.
/// Uses a minimal ASN.1 walk — no dependency on an X.509 library.
pub fn extract_issuer_dn(cert_der: &[u8]) -> Result<String, CmsAdapterError> {
    use prro_crypto::cms::asn1_util as a1;

    let err = |msg: &str| CmsAdapterError::CertParse(msg.to_string());

    // SEQUENCE (Certificate) → SEQUENCE (tbsCertificate)
    let (_, tbs_start) =
        a1::read_tlv(cert_der, 0).map_err(|e| CmsAdapterError::CertParse(e.to_string()))?;
    let (_, tbs_inner) =
        a1::read_tlv(cert_der, tbs_start).map_err(|e| CmsAdapterError::CertParse(e.to_string()))?;

    let mut pos = tbs_inner;
    // Skip optional version [0]
    if a1::peek_tag(cert_der, pos).map_err(|e| CmsAdapterError::CertParse(e.to_string()))? == 0xa0 {
        let (end, _) =
            a1::read_tlv(cert_der, pos).map_err(|e| CmsAdapterError::CertParse(e.to_string()))?;
        pos = end;
    }
    // Skip serialNumber, signature, then read issuer (4th field)
    for _ in 0..2 {
        let (end, _) =
            a1::read_tlv(cert_der, pos).map_err(|e| CmsAdapterError::CertParse(e.to_string()))?;
        pos = end;
    }
    // issuer is next — read its raw bytes and format as hex for matching
    let (issuer_end, issuer_inner) =
        a1::read_tlv(cert_der, pos).map_err(|e| CmsAdapterError::CertParse(e.to_string()))?;

    if issuer_end > cert_der.len() {
        return Err(err("issuer sequence extends beyond cert DER"));
    }

    // Extract printable string content from issuer DN for pattern matching.
    // Walk the RDN SETs to collect all string values.
    let mut dn_parts = Vec::new();
    let mut rdn_pos = issuer_inner;
    while rdn_pos < issuer_end {
        let Ok((set_end, set_inner)) = a1::read_tlv(cert_der, rdn_pos) else {
            break;
        };
        let Ok((atv_end, atv_inner)) = a1::read_tlv(cert_der, set_inner) else {
            rdn_pos = set_end;
            continue;
        };
        // Skip OID, read value
        let Ok((oid_end, _)) = a1::read_tlv(cert_der, atv_inner) else {
            rdn_pos = set_end;
            continue;
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

/// Extract the notAfter date from a DER-encoded X.509 certificate.
/// Returns an RFC3339 string like "2027-01-01T00:00:00Z".
/// Used to derive the XorSoft key for password encoding/decoding.
/// The returned format must match what is stored in `operator_certs.valid_to`.
pub fn extract_cert_valid_to(cert_der: &[u8]) -> Result<String, CmsAdapterError> {
    use prro_crypto::cms::asn1_util as a1;

    // Certificate (SEQUENCE) → tbsCertificate (SEQUENCE)
    let (_, tbs_start) =
        a1::read_tlv(cert_der, 0).map_err(|e| CmsAdapterError::CertParse(e.to_string()))?;
    let (_, tbs_inner) =
        a1::read_tlv(cert_der, tbs_start).map_err(|e| CmsAdapterError::CertParse(e.to_string()))?;

    let mut pos = tbs_inner;
    // Skip optional version [0]
    if a1::peek_tag(cert_der, pos).map_err(|e| CmsAdapterError::CertParse(e.to_string()))? == 0xa0 {
        let (end, _) =
            a1::read_tlv(cert_der, pos).map_err(|e| CmsAdapterError::CertParse(e.to_string()))?;
        pos = end;
    }
    // Skip serialNumber, signature, issuer (3 fields)
    for _ in 0..3 {
        let (end, _) =
            a1::read_tlv(cert_der, pos).map_err(|e| CmsAdapterError::CertParse(e.to_string()))?;
        pos = end;
    }
    // validity SEQUENCE
    let (_, val_inner) =
        a1::read_tlv(cert_der, pos).map_err(|e| CmsAdapterError::CertParse(e.to_string()))?;
    // notBefore — check tag then skip it
    let not_before_tag =
        a1::peek_tag(cert_der, val_inner).map_err(|e| CmsAdapterError::CertParse(e.to_string()))?;
    if not_before_tag != 0x17 && not_before_tag != 0x18 {
        return Err(CmsAdapterError::CertParse(format!(
            "unexpected notBefore tag 0x{not_before_tag:02x}"
        )));
    }
    let (not_after_pos, _) =
        a1::read_tlv(cert_der, val_inner).map_err(|e| CmsAdapterError::CertParse(e.to_string()))?;
    // notAfter
    let not_after_tag = a1::peek_tag(cert_der, not_after_pos)
        .map_err(|e| CmsAdapterError::CertParse(e.to_string()))?;
    let (not_after_end, not_after_inner) = a1::read_tlv(cert_der, not_after_pos)
        .map_err(|e| CmsAdapterError::CertParse(e.to_string()))?;
    if not_after_end > cert_der.len() {
        return Err(CmsAdapterError::CertParse(
            "notAfter extends beyond cert DER".into(),
        ));
    }
    let raw = std::str::from_utf8(&cert_der[not_after_inner..not_after_end])
        .map_err(|e| CmsAdapterError::CertParse(format!("notAfter UTF-8: {e}")))?;
    // Normalize to RFC3339: "YYYY-MM-DDTHH:MM:SSZ"
    match not_after_tag {
        0x17 => {
            // UTCTime: YYMMDDHHMMSSZ (13 bytes)
            // RFC 5280: year 00-49 = 2000+, year 50-99 = 1900+
            if raw.len() < 13 {
                return Err(CmsAdapterError::CertParse(format!(
                    "UTCTime too short: {raw:?}"
                )));
            }
            let yy: u32 = raw[..2]
                .parse()
                .map_err(|_| CmsAdapterError::CertParse(format!("UTCTime yy: {raw}")))?;
            let century = if yy <= 49 { 2000u32 } else { 1900u32 };
            let yyyy = century + yy;
            Ok(format!(
                "{:04}-{}-{}T{}:{}:{}Z",
                yyyy,
                &raw[2..4],
                &raw[4..6],
                &raw[6..8],
                &raw[8..10],
                &raw[10..12]
            ))
        }
        0x18 => {
            // GeneralizedTime: YYYYMMDDHHMMSSZ (15 bytes)
            if raw.len() < 15 {
                return Err(CmsAdapterError::CertParse(format!(
                    "GeneralizedTime too short: {raw:?}"
                )));
            }
            Ok(format!(
                "{}-{}-{}T{}:{}:{}Z",
                &raw[..4],
                &raw[4..6],
                &raw[6..8],
                &raw[8..10],
                &raw[10..12],
                &raw[12..14]
            ))
        }
        tag => Err(CmsAdapterError::CertParse(format!(
            "unexpected notAfter tag 0x{tag:02x}"
        ))),
    }
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
        assert!(
            matches!(err, CmsAdapterError::NoTspMapping { .. }),
            "expected NoTspMapping, got {err}"
        );
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
        )
        .unwrap();

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
        )
        .unwrap();

        // Any issuer DN must match the empty pattern
        let url = resolve_tsp_url(&conn, "CN=Anyone, O=AnyOrg, C=UA").unwrap();
        assert_eq!(
            url, "https://catch-all.example.com/tsp/",
            "empty issuer_pattern must match any issuer DN via INSTR"
        );
    }

    // ── C. extract_issuer_dn with empty bytes ─────────────────────────────────

    // ── extract_cert_valid_to ─────────────────────────────────────────────────

    /// Build a minimal DER Certificate skeleton with a UTCTime notAfter.
    /// Structure: SEQUENCE { SEQUENCE { [0]{02 01 02} INTEGER{01} SEQUENCE{} SEQUENCE{} SEQUENCE{ UTCTime notBefore, UTCTime notAfter } } }
    fn make_minimal_cert_der(not_before: &[u8], not_after: &[u8]) -> Vec<u8> {
        // Encode a single TLV: tag, len, value
        let tlv = |tag: u8, val: &[u8]| -> Vec<u8> {
            let mut out = vec![tag];
            let len = val.len();
            if len < 0x80 {
                out.push(len as u8);
            } else {
                out.push(0x82);
                out.push((len >> 8) as u8);
                out.push((len & 0xff) as u8);
            }
            out.extend_from_slice(val);
            out
        };

        // version [0] EXPLICIT v3 (02 01 02)
        let version_inner = tlv(0x02, &[0x02]);
        let version = tlv(0xa0, &version_inner);
        // serialNumber INTEGER 1
        let serial = tlv(0x02, &[0x01]);
        // signature AlgorithmIdentifier (empty SEQUENCE)
        let sig_alg = tlv(0x30, &[]);
        // issuer (empty SEQUENCE)
        let issuer = tlv(0x30, &[]);
        // validity SEQUENCE { notBefore, notAfter }
        let mut validity_inner = Vec::new();
        validity_inner.extend_from_slice(not_before);
        validity_inner.extend_from_slice(not_after);
        let validity = tlv(0x30, &validity_inner);

        // tbsCertificate SEQUENCE
        let mut tbs_inner = Vec::new();
        tbs_inner.extend_from_slice(&version);
        tbs_inner.extend_from_slice(&serial);
        tbs_inner.extend_from_slice(&sig_alg);
        tbs_inner.extend_from_slice(&issuer);
        tbs_inner.extend_from_slice(&validity);
        let tbs = tlv(0x30, &tbs_inner);

        // Certificate SEQUENCE
        tlv(0x30, &tbs)
    }

    #[test]
    fn extract_cert_valid_to_utctime() {
        // UTCTime "270101000000Z" → "2027-01-01T00:00:00Z"
        let not_before = [
            0x17u8, 0x0d, b'2', b'6', b'0', b'1', b'0', b'1', b'0', b'0', b'0', b'0', b'0', b'0',
            b'Z',
        ];
        let not_after = [
            0x17u8, 0x0d, b'2', b'7', b'0', b'1', b'0', b'1', b'0', b'0', b'0', b'0', b'0', b'0',
            b'Z',
        ];
        let der = make_minimal_cert_der(&not_before, &not_after);
        let result = extract_cert_valid_to(&der).unwrap();
        assert_eq!(result, "2027-01-01T00:00:00Z");
    }

    #[test]
    fn extract_cert_valid_to_utctime_century_50_to_99() {
        // UTCTime "991231235959Z" → year 99 ≥ 50 → 1999-12-31T23:59:59Z
        let not_before = [
            0x17u8, 0x0d, b'9', b'8', b'0', b'1', b'0', b'1', b'0', b'0', b'0', b'0', b'0', b'0',
            b'Z',
        ];
        let not_after = [
            0x17u8, 0x0d, b'9', b'9', b'1', b'2', b'3', b'1', b'2', b'3', b'5', b'9', b'5', b'9',
            b'Z',
        ];
        let der = make_minimal_cert_der(&not_before, &not_after);
        let result = extract_cert_valid_to(&der).unwrap();
        assert_eq!(result, "1999-12-31T23:59:59Z");
    }

    #[test]
    fn extract_cert_valid_to_generalizedtime() {
        // GeneralizedTime "20270101000000Z" → "2027-01-01T00:00:00Z"
        let not_before = [
            0x18u8, 0x0f, b'2', b'0', b'2', b'6', b'0', b'1', b'0', b'1', b'0', b'0', b'0', b'0',
            b'0', b'0', b'Z',
        ];
        let not_after = [
            0x18u8, 0x0f, b'2', b'0', b'2', b'7', b'0', b'1', b'0', b'1', b'0', b'0', b'0', b'0',
            b'0', b'0', b'Z',
        ];
        let der = make_minimal_cert_der(&not_before, &not_after);
        let result = extract_cert_valid_to(&der).unwrap();
        assert_eq!(result, "2027-01-01T00:00:00Z");
    }

    #[test]
    fn extract_cert_valid_to_empty_returns_error() {
        let result = extract_cert_valid_to(&[]);
        assert!(matches!(result, Err(CmsAdapterError::CertParse(_))));
    }

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
        )
        .unwrap();

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
        )
        .unwrap();

        // Issuer DN that literally contains '%' must match the '%' pattern
        let url = resolve_tsp_url(&conn, "CN=CA 100% Trusted, O=Org, C=UA");
        assert!(
            url.is_ok(),
            "DN containing '%' must match pattern '%' via INSTR"
        );
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
        )
        .unwrap();

        // priority=10 wins over priority=20 (ORDER BY priority ASC → smallest first)
        let url = resolve_tsp_url(&conn, "testcorp").unwrap();
        assert_eq!(
            url, "https://first.example.com/tsp/",
            "priority=10 must beat priority=20 (ASC order, smaller = higher priority)"
        );
    }
}
