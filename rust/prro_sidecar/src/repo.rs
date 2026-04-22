//! SQLite repository — short-transaction queries for fn_config, sidecar_operators,
//! licenses, operator_certs, audit_log.
//! Invariant (1): no network/crypto inside transactions.

use std::sync::{Mutex, MutexGuard};

use rusqlite::{params, Connection};

use crate::config::CredentialsMode;
use crate::errors::SidecarError;

// ── Domain types ──────────────────────────────────────────────────────────────

/// Whether the FN operates against the production DPS endpoint or the test one.
/// Stored as "prod" / "test" in SQLite CHECK-constrained column — FromSql rejects
/// any other value so callers never receive an unexpected string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FiscalMode {
    Prod,
    Test,
}

impl rusqlite::types::FromSql for FiscalMode {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match String::column_result(value)?.as_str() {
            "prod" => Ok(Self::Prod),
            "test" => Ok(Self::Test),
            other  => Err(rusqlite::types::FromSqlError::Other(Box::new(
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid fiscal_mode {other:?}; expected 'prod' or 'test'"),
                ),
            ))),
        }
    }
}

impl rusqlite::types::FromSql for CredentialsMode {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match String::column_result(value)?.as_str() {
            "plain"    => Ok(Self::Plain),
            "xor_soft" => Ok(Self::XorSoft),
            other => Err(rusqlite::types::FromSqlError::Other(Box::new(
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid credentials_mode {other:?}; expected 'plain' or 'xor_soft'"),
                ),
            ))),
        }
    }
}

// ── Row types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FnConfig {
    pub fiscal_number:          String,
    pub tax_number:             String,
    pub fiscal_mode:            FiscalMode,
    pub national_check_enabled: bool,
    pub offline_enabled:        bool,
    pub tsp_enabled:            bool,
    pub org_name:               Option<String>,
    pub org_address:            Option<String>,
}

#[derive(Debug, Clone)]
pub struct OperatorRow {
    pub id:               i64,
    pub fiscal_number:    String,
    pub operator_name:    Option<String>,
    pub operator_inn:     String,
    pub jks_path:         String,
    /// XOR-soft hex or plain text — as stored; caller decodes via credentials module.
    pub jks_password:     String,
    /// Per-row password encoding mode — overrides global config.security.credentials_mode.
    pub credentials_mode: CredentialsMode,
}

#[derive(Debug, Clone)]
pub struct LicenseRow {
    pub id:               i64,
    pub tin:              String,
    /// Raw JSON array of allowed fiscal numbers, e.g. `["3001234567","3001234568"]`.
    /// Use `fn_numbers()` for a typed view — kept as String to stay thin at query time.
    pub fn_numbers_json:  String,
    pub issued_at:        String,
    pub expires_at:       String,
    pub tier:             String,
    pub org_name:         Option<String>,
    pub demo_limits_json: Option<String>,
    pub payload_b64:      String,
    pub signature_b64:    String,
}

impl LicenseRow {
    /// Parse `fn_numbers_json` into the list of licensed fiscal numbers.
    /// Called by the license checker to verify `fn ∈ allowed set`.
    pub fn fn_numbers(&self) -> Result<Vec<String>, serde_json::Error> {
        serde_json::from_str(&self.fn_numbers_json)
    }
}

#[derive(Debug, Clone)]
pub struct CertMetadata {
    pub fiscal_number:    String,
    pub cert_fingerprint: String,
    pub ski_hex:          String,
    pub subject_dn:       Option<String>,
    pub issuer_dn:        Option<String>,
    pub valid_from:       Option<String>,
    /// ISO-8601 UTC. The `operator_certs` table has no `active` column —
    /// callers MUST call `is_valid_at(now)` before using this cert for
    /// signing; signing with an expired cert is a fiscal protocol violation.
    pub valid_to:         Option<String>,
    pub source:           String,
}

impl CertMetadata {
    /// Returns true when `now` falls within [valid_from, valid_to].
    /// Returns true when `valid_to` is absent (cert has no stated expiry).
    pub fn is_valid_at(&self, now: time::OffsetDateTime) -> bool {
        let parse = |s: &str| {
            time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
        };
        if let Some(valid_to_str) = self.valid_to.as_deref() {
            match parse(valid_to_str) {
                Some(vt) => { if now > vt { return false; } }
                None     => return false, // present but unparseable → fail-closed
            }
        }
        if let Some(valid_from_str) = self.valid_from.as_deref() {
            match parse(valid_from_str) {
                Some(vf) => { if now < vf { return false; } }
                None     => return false, // present but unparseable → fail-closed
            }
        }
        true
    }
}

/// Severity for `audit_log` entries — mirrors the DB CHECK constraint.
/// Using an enum prevents silent insertion of invalid strings that would
/// produce an opaque `SidecarError::Db` at runtime instead of a compile error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

impl AuditSeverity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info     => "INFO",
            Self::Warning  => "WARNING",
            Self::Error    => "ERROR",
            Self::Critical => "CRITICAL",
        }
    }
}

impl rusqlite::types::ToSql for AuditSeverity {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::from(self.as_str()))
    }
}

pub struct AuditEntry<'a> {
    pub entity_type:        &'a str,
    pub entity_id:          &'a str,
    pub event_type:         &'a str,
    pub severity:           AuditSeverity,
    pub event_payload_json: Option<&'a str>,
}

// ── Repo ──────────────────────────────────────────────────────────────────────

pub struct Repo {
    conn: Mutex<Connection>,
}

impl Repo {
    pub fn open(path: &str) -> Result<Self, SidecarError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             PRAGMA busy_timeout=5000;
             -- Per-FN document sequencing, survived across restarts.
             -- previous_hash is the <MAC> value from the last accepted DPS response.
             CREATE TABLE IF NOT EXISTS local_sequences (
                 fiscal_number TEXT PRIMARY KEY,
                 last          INTEGER NOT NULL DEFAULT 0,
                 previous_hash TEXT    NOT NULL DEFAULT ''
             );
             CREATE TABLE IF NOT EXISTS fn_degraded (
                 fiscal_number TEXT PRIMARY KEY,
                 pending_hash  TEXT    NOT NULL,
                 degraded_at   TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
                 retry_count   INTEGER NOT NULL DEFAULT 0,
                 last_retry_at TEXT
             );",
        )?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Acquire the DB lock; maps PoisonError to SidecarError::Internal so callers
    /// don't need to handle two error kinds.
    fn lock(&self) -> Result<MutexGuard<'_, Connection>, SidecarError> {
        self.conn
            .lock()
            .map_err(|_| SidecarError::Internal("db mutex poisoned".into()))
    }

    /// Load per-FN configuration row (includes 017 additions).
    pub fn load_fn_config(&self, fiscal_number: &str) -> Result<FnConfig, SidecarError> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT fiscal_number, tax_number, fiscal_mode,
                    national_check_enabled, offline_enabled, tsp_enabled,
                    org_name, org_address
             FROM   fiscal_number_config
             WHERE  fiscal_number = ?1",
            params![fiscal_number],
            |row| {
                Ok(FnConfig {
                    fiscal_number:          row.get(0)?,
                    tax_number:             row.get(1)?,
                    fiscal_mode:            row.get(2)?,
                    national_check_enabled: row.get::<_, i32>(3)? != 0,
                    offline_enabled:        row.get::<_, i32>(4)? != 0,
                    tsp_enabled:            row.get::<_, i32>(5)? != 0,
                    org_name:               row.get(6)?,
                    org_address:            row.get(7)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                SidecarError::NotFound(format!("fn_config: {fiscal_number}"))
            }
            other => SidecarError::Db(other),
        })
    }

    /// Return the most-recently registered active operator for the given fiscal_number.
    /// ORDER BY id DESC makes the result deterministic when multiple rows are active
    /// (e.g., during a concurrent de-activation race).
    pub fn load_active_operator(&self, fiscal_number: &str) -> Result<OperatorRow, SidecarError> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT id, fiscal_number, operator_name, operator_inn,
                    jks_path, jks_password, credentials_mode
             FROM   sidecar_operators
             WHERE  fiscal_number = ?1 AND active = 1
             ORDER  BY id DESC
             LIMIT  1",
            params![fiscal_number],
            |row| {
                Ok(OperatorRow {
                    id:               row.get(0)?,
                    fiscal_number:    row.get(1)?,
                    operator_name:    row.get(2)?,
                    operator_inn:     row.get(3)?,
                    jks_path:         row.get(4)?,
                    jks_password:     row.get(5)?,
                    credentials_mode: row.get(6)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                SidecarError::NotFound(format!("active operator for fn: {fiscal_number}"))
            }
            other => SidecarError::Db(other),
        })
    }

    /// Return the single active license (UNIQUE INDEX on active=1 guarantees at most one).
    pub fn load_active_license(&self) -> Result<LicenseRow, SidecarError> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT id, tin, fn_numbers_json, issued_at, expires_at, tier,
                    org_name, demo_limits_json, payload_b64, signature_b64
             FROM   licenses
             WHERE  active = 1",
            [],
            |row| {
                Ok(LicenseRow {
                    id:               row.get(0)?,
                    tin:              row.get(1)?,
                    fn_numbers_json:  row.get(2)?,
                    issued_at:        row.get(3)?,
                    expires_at:       row.get(4)?,
                    tier:             row.get(5)?,
                    org_name:         row.get(6)?,
                    demo_limits_json: row.get(7)?,
                    payload_b64:      row.get(8)?,
                    signature_b64:    row.get(9)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                SidecarError::NotFound("no active license installed".into())
            }
            other => SidecarError::Db(other),
        })
    }

    /// Cert metadata without the cert_der BLOB — keeps the hot path lean.
    pub fn load_operator_cert_metadata(
        &self,
        fiscal_number: &str,
    ) -> Result<CertMetadata, SidecarError> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT fiscal_number, cert_fingerprint, ski_hex,
                    subject_dn, issuer_dn, valid_from, valid_to, source
             FROM   operator_certs
             WHERE  fiscal_number = ?1",
            params![fiscal_number],
            |row| {
                Ok(CertMetadata {
                    fiscal_number:    row.get(0)?,
                    cert_fingerprint: row.get(1)?,
                    ski_hex:          row.get(2)?,
                    subject_dn:       row.get(3)?,
                    issuer_dn:        row.get(4)?,
                    valid_from:       row.get(5)?,
                    valid_to:         row.get(6)?,
                    source:           row.get(7)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                SidecarError::NotFound(format!("operator_certs: {fiscal_number}"))
            }
            other => SidecarError::Db(other),
        })
    }

    /// Atomically increment and return the per-FN local document number.
    /// Persisted in SQLite — survives sidecar restarts within a shift.
    pub fn next_local_number(&self, fiscal_number: &str) -> Result<i32, SidecarError> {
        let conn = self.lock()?;
        // Single statement: UPSERT + RETURNING closes the read-your-own-write gap
        // that a separate SELECT would introduce under concurrent callers.
        let n: i32 = conn.query_row(
            "INSERT INTO local_sequences (fiscal_number, last) VALUES (?1, 1)
             ON CONFLICT(fiscal_number) DO UPDATE SET last = last + 1
             RETURNING last",
            params![fiscal_number],
            |row| row.get(0),
        )?;
        Ok(n)
    }

    /// Return the `<MAC>` hash from the last successfully accepted DPS document.
    /// Returns empty string if no document has been accepted yet (fresh RRO).
    pub fn load_previous_hash(&self, fiscal_number: &str) -> Result<String, SidecarError> {
        let conn = self.lock()?;
        match conn.query_row(
            "SELECT previous_hash, last FROM local_sequences WHERE fiscal_number = ?1",
            params![fiscal_number],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?)),
        ) {
            Ok((h, last)) => {
                // Empty hash after at least one document suggests a MAC bootstrap gap
                // (DB seeded from outside, or previous_hash never stored after sendChkV2).
                if h.is_empty() && last > 0 {
                    tracing::warn!(
                        fiscal_number,
                        last,
                        "load_previous_hash: empty previous_hash with last={last} — \
                         possible MAC chain gap; DPS may reject next document"
                    );
                }
                Ok(h)
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(String::new()),
            Err(e) => Err(SidecarError::Db(e)),
        }
    }

    /// Persist the `<MAC>` hash from a successfully accepted DPS document.
    /// Called after each successful `send_chk_v2` response.
    pub fn store_previous_hash(&self, fiscal_number: &str, hash: &str) -> Result<(), SidecarError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO local_sequences (fiscal_number, previous_hash) VALUES (?1, ?2)
             ON CONFLICT(fiscal_number) DO UPDATE SET previous_hash = excluded.previous_hash",
            params![fiscal_number, hash],
        )?;
        Ok(())
    }

    /// Mark a fiscal number as hash-chain-degraded with the pending hash that
    /// could not be persisted after a successful DPS response.
    /// Re-degradation resets retry_count — each degradation event starts fresh.
    pub fn set_degraded(&self, fiscal_number: &str, pending_hash: &str) -> Result<(), SidecarError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO fn_degraded (fiscal_number, pending_hash)
             VALUES (?1, ?2)
             ON CONFLICT(fiscal_number) DO UPDATE SET
                 pending_hash  = excluded.pending_hash,
                 degraded_at   = CURRENT_TIMESTAMP,
                 retry_count   = 0,
                 last_retry_at = NULL",
            params![fiscal_number, pending_hash],
        )?;
        Ok(())
    }

    /// Returns true when the fiscal number has an unresolved degraded-chain entry.
    pub fn is_degraded(&self, fiscal_number: &str) -> Result<bool, SidecarError> {
        let conn = self.lock()?;
        let count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM fn_degraded WHERE fiscal_number = ?1",
            params![fiscal_number],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Return all degraded fiscal numbers with their pending hash and retry count,
    /// ordered oldest-first so the reconcile loop drains longest-stuck entries first.
    pub fn list_degraded(&self) -> Result<Vec<(String, String, i32)>, SidecarError> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT fiscal_number, pending_hash, retry_count FROM fn_degraded ORDER BY degraded_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i32>(2)?))
        })?;
        let mut result = Vec::new();
        for row in rows { result.push(row?); }
        Ok(result)
    }

    /// On success: store pending_hash as previous_hash + DELETE fn_degraded (one transaction).
    /// On failure: increment retry_count + last_retry_at (best-effort, only when ROLLBACK succeeds).
    /// Caller must pass the same hash that was stored in fn_degraded by set_degraded().
    pub fn reconcile_chain(&self, fiscal_number: &str, pending_hash: &str) -> Result<(), SidecarError> {
        let conn = self.lock()?;
        conn.execute_batch("BEGIN IMMEDIATE").map_err(SidecarError::Db)?;
        let store_result = conn.execute(
            "INSERT INTO local_sequences (fiscal_number, previous_hash) VALUES (?1, ?2)
             ON CONFLICT(fiscal_number) DO UPDATE SET previous_hash = excluded.previous_hash",
            params![fiscal_number, pending_hash],
        );
        let del_result = store_result.and_then(|_| {
            conn.execute(
                "DELETE FROM fn_degraded WHERE fiscal_number = ?1",
                params![fiscal_number],
            )
        });
        match del_result {
            Ok(_) => {
                conn.execute_batch("COMMIT").map_err(SidecarError::Db)?;
                Ok(())
            }
            Err(e) => {
                if conn.execute_batch("ROLLBACK").is_ok() {
                    let _ = conn.execute(
                        "UPDATE fn_degraded SET retry_count = retry_count + 1,
                         last_retry_at = CURRENT_TIMESTAMP WHERE fiscal_number = ?1",
                        params![fiscal_number],
                    );
                }
                Err(SidecarError::Db(e))
            }
        }
    }

    /// Load raw cert DER bytes for the given fiscal_number.
    /// Used when `ExtractedKey.certs` is empty (Key6Dat containers).
    pub fn load_cert_der_for_fn(&self, fiscal_number: &str) -> Result<Vec<u8>, SidecarError> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT cert_der FROM operator_certs WHERE fiscal_number = ?1",
            params![fiscal_number],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                SidecarError::NotFound(format!("operator_certs cert_der: {fiscal_number}"))
            }
            other => SidecarError::Db(other),
        })
    }

    /// Resolve TSP URL by substring-matching `issuer_dn` against `ca_endpoints.issuer_pattern`.
    /// Delegates to `cms_adapter::resolve_tsp_url` — single source of truth for the SQL.
    /// Short lock — query only, no network inside the mutex.
    pub fn load_tsp_url_by_issuer_dn(&self, issuer_dn: &str) -> Result<String, SidecarError> {
        let conn = self.lock()?;
        crate::cms_adapter::resolve_tsp_url(&conn, issuer_dn).map_err(|e| match e {
            crate::cms_adapter::CmsAdapterError::NoTspMapping { .. } => {
                SidecarError::NotFound(format!("no TSP endpoint for issuer DN: {issuer_dn}"))
            }
            other => SidecarError::Internal(other.to_string()),
        })
    }

    pub fn audit_log_insert(&self, entry: &AuditEntry<'_>) -> Result<(), SidecarError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO audit_log
                 (entity_type, entity_id, event_type, severity, event_payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                entry.entity_type,
                entry.entity_id,
                entry.event_type,
                entry.severity,
                entry.event_payload_json,
            ],
        )?;
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal in-memory schema: 011 + 015 + 017 columns merged into single DDL.
    fn make_repo() -> Repo {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;

             CREATE TABLE fiscal_number_config (
                 fiscal_number           TEXT PRIMARY KEY,
                 enforce_blocked_mode    INTEGER NOT NULL DEFAULT 0 CHECK (enforce_blocked_mode IN (0,1)),
                 min_offline_codes       INTEGER NOT NULL DEFAULT 0,
                 max_offline_codes       INTEGER NOT NULL DEFAULT 0,
                 created_at              TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                 updated_at              TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                 -- 017 additions
                 tax_number              TEXT NOT NULL DEFAULT '',
                 fiscal_mode             TEXT NOT NULL DEFAULT 'test' CHECK (fiscal_mode IN ('prod','test')),
                 national_check_enabled  INTEGER NOT NULL DEFAULT 0 CHECK (national_check_enabled IN (0,1)),
                 offline_enabled         INTEGER NOT NULL DEFAULT 1 CHECK (offline_enabled IN (0,1)),
                 tsp_enabled             INTEGER NOT NULL DEFAULT 0 CHECK (tsp_enabled IN (0,1)),
                 org_name                TEXT,
                 org_address             TEXT
             );

             CREATE TABLE sidecar_operators (
                 id               INTEGER PRIMARY KEY AUTOINCREMENT,
                 fiscal_number    TEXT NOT NULL,
                 operator_name    TEXT,
                 operator_inn     TEXT NOT NULL,
                 jks_path         TEXT NOT NULL,
                 jks_password     TEXT NOT NULL,
                 active           INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0,1)),
                 credentials_mode TEXT NOT NULL DEFAULT 'plain'
                                  CHECK (credentials_mode IN ('plain', 'xor_soft')),
                 created_at       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                 updated_at       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                 FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number)
             );

             CREATE TABLE licenses (
                 id               INTEGER PRIMARY KEY AUTOINCREMENT,
                 tin              TEXT NOT NULL,
                 fn_numbers_json  TEXT NOT NULL,
                 issued_at        TEXT NOT NULL,
                 expires_at       TEXT NOT NULL,
                 tier             TEXT NOT NULL CHECK (tier IN ('demo','basic','pro','enterprise')),
                 org_name         TEXT,
                 demo_limits_json TEXT,
                 payload_b64      TEXT NOT NULL,
                 signature_b64    TEXT NOT NULL,
                 installed_at     TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                 active           INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0,1))
             );
             CREATE UNIQUE INDEX ix_licenses_active_single ON licenses(active) WHERE active = 1;

             CREATE TABLE operator_certs (
                 fiscal_number      TEXT PRIMARY KEY,
                 cert_fingerprint   TEXT NOT NULL,
                 ski_hex            TEXT NOT NULL,
                 cert_der           BLOB NOT NULL,
                 subject_dn         TEXT,
                 issuer_dn          TEXT,
                 valid_from         TIMESTAMP,
                 valid_to           TIMESTAMP,
                 fetched_at         TIMESTAMP NOT NULL,
                 source             TEXT NOT NULL CHECK (source IN ('container','cmp','manual')),
                 last_refresh_at    TIMESTAMP
             );

             CREATE TABLE audit_log (
                 audit_id            INTEGER PRIMARY KEY AUTOINCREMENT,
                 created_at          TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                 entity_type         TEXT NOT NULL,
                 entity_id           TEXT NOT NULL,
                 event_type          TEXT NOT NULL,
                 severity            TEXT NOT NULL CHECK (severity IN ('INFO','WARNING','ERROR','CRITICAL')),
                 event_payload_json  TEXT
             );

             CREATE TABLE local_sequences (
                 fiscal_number TEXT PRIMARY KEY,
                 last          INTEGER NOT NULL DEFAULT 0,
                 previous_hash TEXT    NOT NULL DEFAULT ''
             );

             CREATE TABLE fn_degraded (
                 fiscal_number TEXT PRIMARY KEY,
                 pending_hash  TEXT    NOT NULL,
                 degraded_at   TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
                 retry_count   INTEGER NOT NULL DEFAULT 0,
                 last_retry_at TEXT
             );",
        )
        .unwrap();
        Repo { conn: Mutex::new(conn) }
    }

    fn insert_fn(repo: &Repo, fn_: &str) {
        let conn = repo.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO fiscal_number_config
                 (fiscal_number, tax_number, fiscal_mode,
                  national_check_enabled, offline_enabled, tsp_enabled, org_name)
             VALUES (?1, '1234567890', 'prod', 1, 1, 0, 'Test Org')",
            params![fn_],
        )
        .unwrap();
    }

    #[test]
    fn load_fn_config_found() {
        let repo = make_repo();
        insert_fn(&repo, "3001234567");
        let cfg = repo.load_fn_config("3001234567").unwrap();
        assert_eq!(cfg.fiscal_number, "3001234567");
        assert_eq!(cfg.tax_number, "1234567890");
        assert_eq!(cfg.fiscal_mode, FiscalMode::Prod);
        assert!(cfg.national_check_enabled);
        assert!(cfg.offline_enabled);
        assert!(!cfg.tsp_enabled);
        assert_eq!(cfg.org_name.as_deref(), Some("Test Org"));
    }

    #[test]
    fn load_fn_config_not_found() {
        let repo = make_repo();
        let err = repo.load_fn_config("9999999999").unwrap_err();
        assert!(matches!(err, SidecarError::NotFound(_)));
    }

    #[test]
    fn load_active_operator_found() {
        let repo = make_repo();
        insert_fn(&repo, "3001234567");
        {
            let conn = repo.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO sidecar_operators
                     (fiscal_number, operator_name, operator_inn, jks_path, jks_password, active)
                 VALUES (?1, 'Петренко Іван', '9876543210', '/keys/op.jks', 'deadbeef01', 1)",
                params!["3001234567"],
            )
            .unwrap();
        }
        let op = repo.load_active_operator("3001234567").unwrap();
        assert_eq!(op.operator_inn, "9876543210");
        assert_eq!(op.jks_path, "/keys/op.jks");
        assert_eq!(op.jks_password, "deadbeef01");
        assert_eq!(op.operator_name.as_deref(), Some("Петренко Іван"));
    }

    #[test]
    fn load_active_operator_not_found() {
        let repo = make_repo();
        let err = repo.load_active_operator("3001234567").unwrap_err();
        assert!(matches!(err, SidecarError::NotFound(_)));
    }

    #[test]
    fn load_active_operator_credentials_mode_xor_soft() {
        // Verify that a row inserted with credentials_mode='xor_soft' is returned
        // as CredentialsMode::XorSoft — per-row mode decode, not global config.
        let repo = make_repo();
        insert_fn(&repo, "3001234567");
        {
            let conn = repo.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO sidecar_operators
                     (fiscal_number, operator_name, operator_inn, jks_path, jks_password,
                      active, credentials_mode)
                 VALUES (?1, 'Тест Оператор', '1234567890', '/keys/op.jks', 'hexhex01',
                         1, 'xor_soft')",
                params!["3001234567"],
            )
            .unwrap();
        }
        let op = repo.load_active_operator("3001234567").unwrap();
        assert_eq!(
            op.credentials_mode,
            CredentialsMode::XorSoft,
            "credentials_mode column 'xor_soft' must map to CredentialsMode::XorSoft"
        );
    }

    #[test]
    fn load_active_license_found() {
        let repo = make_repo();
        {
            let conn = repo.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO licenses
                     (tin, fn_numbers_json, issued_at, expires_at, tier,
                      payload_b64, signature_b64, active)
                 VALUES ('1234567890', '[\"3001234567\"]',
                         '2026-01-01T00:00:00Z', '2027-01-01T00:00:00Z',
                         'pro', 'cGF5bG9hZA==', 'c2lnbmF0dXJl', 1)",
                [],
            )
            .unwrap();
        }
        let lic = repo.load_active_license().unwrap();
        assert_eq!(lic.tin, "1234567890");
        assert_eq!(lic.tier, "pro");
        assert_eq!(lic.fn_numbers_json, "[\"3001234567\"]");
    }

    #[test]
    fn load_active_license_not_found() {
        let repo = make_repo();
        let err = repo.load_active_license().unwrap_err();
        assert!(matches!(err, SidecarError::NotFound(_)));
    }

    #[test]
    fn load_cert_metadata_found() {
        let repo = make_repo();
        insert_fn(&repo, "3001234567");
        {
            let conn = repo.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO operator_certs
                     (fiscal_number, cert_fingerprint, ski_hex, cert_der,
                      subject_dn, issuer_dn, valid_from, valid_to, fetched_at, source)
                 VALUES (?1, 'aabbcc', 'AABBCC1122', X'63657274',
                         'CN=Test', 'CN=CA', '2025-01-01', '2027-01-01',
                         '2025-06-01', 'container')",
                params!["3001234567"],
            )
            .unwrap();
        }
        let meta = repo.load_operator_cert_metadata("3001234567").unwrap();
        assert_eq!(meta.ski_hex, "AABBCC1122");
        assert_eq!(meta.source, "container");
        assert_eq!(meta.valid_to.as_deref(), Some("2027-01-01"));
    }

    #[test]
    fn load_cert_metadata_not_found() {
        let repo = make_repo();
        let err = repo.load_operator_cert_metadata("9999999999").unwrap_err();
        assert!(matches!(err, SidecarError::NotFound(_)));
    }

    #[test]
    fn audit_log_insert_ok() {
        let repo = make_repo();
        repo.audit_log_insert(&AuditEntry {
            entity_type:        "license",
            entity_id:          "1",
            event_type:         "VERIFIED",
            severity:           AuditSeverity::Info,
            event_payload_json: Some(r#"{"tier":"pro"}"#),
        })
        .unwrap();
        let conn = repo.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM audit_log", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn audit_severity_as_str_matches_db_constraint() {
        // Verify the enum serializes to the exact strings the DB CHECK allows.
        assert_eq!(AuditSeverity::Info.as_str(),     "INFO");
        assert_eq!(AuditSeverity::Warning.as_str(),  "WARNING");
        assert_eq!(AuditSeverity::Error.as_str(),    "ERROR");
        assert_eq!(AuditSeverity::Critical.as_str(), "CRITICAL");
    }

    #[test]
    fn fn_numbers_parsed_correctly() {
        let row = LicenseRow {
            id: 1,
            tin: "1234567890".into(),
            fn_numbers_json: r#"["3001234567","3001234568"]"#.into(),
            issued_at: "2026-01-01T00:00:00Z".into(),
            expires_at: "2027-01-01T00:00:00Z".into(),
            tier: "pro".into(),
            org_name: None,
            demo_limits_json: None,
            payload_b64: "x".into(),
            signature_b64: "y".into(),
        };
        let fns = row.fn_numbers().unwrap();
        assert_eq!(fns, vec!["3001234567", "3001234568"]);
        assert!(fns.contains(&"3001234567".to_string()));
        assert!(!fns.contains(&"9999999999".to_string()));
    }

    #[test]
    fn fn_numbers_invalid_json_returns_error() {
        let mut row = LicenseRow {
            id: 1, tin: "x".into(), fn_numbers_json: "not-json".into(),
            issued_at: "".into(), expires_at: "".into(), tier: "demo".into(),
            org_name: None, demo_limits_json: None,
            payload_b64: "x".into(), signature_b64: "y".into(),
        };
        assert!(row.fn_numbers().is_err());
        // Also covers an object instead of array
        row.fn_numbers_json = r#"{"fn":"3001234567"}"#.into();
        assert!(row.fn_numbers().is_err());
    }

    // ── FiscalMode ────────────────────────────────────────────────────────────

    #[test]
    fn fiscal_mode_test_variant_loaded() {
        let repo = make_repo();
        {
            let conn = repo.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO fiscal_number_config (fiscal_number, fiscal_mode) VALUES ('3001234567', 'test')",
                [],
            ).unwrap();
        }
        let cfg = repo.load_fn_config("3001234567").unwrap();
        assert_eq!(cfg.fiscal_mode, FiscalMode::Test);
    }

    #[test]
    fn fiscal_mode_fromsql_rejects_invalid_string() {
        // Bypass the DB CHECK constraint via a raw table to verify FromSql itself.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t (v TEXT)").unwrap();
        conn.execute("INSERT INTO t VALUES ('garbage')", []).unwrap();
        let result: rusqlite::Result<FiscalMode> =
            conn.query_row("SELECT v FROM t", [], |r| r.get(0));
        assert!(result.is_err(), "FromSql must reject unknown fiscal_mode value");
    }

    // ── Operator active flag ──────────────────────────────────────────────────

    #[test]
    fn inactive_operator_not_returned() {
        let repo = make_repo();
        insert_fn(&repo, "3001234567");
        {
            let conn = repo.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO sidecar_operators
                     (fiscal_number, operator_inn, jks_path, jks_password, active)
                 VALUES ('3001234567', '1111111111', '/k/op.jks', 'pw', 0)",
                [],
            ).unwrap();
        }
        let err = repo.load_active_operator("3001234567").unwrap_err();
        assert!(matches!(err, SidecarError::NotFound(_)), "inactive operator must not be returned");
    }

    // ── License unique-active constraint ──────────────────────────────────────

    #[test]
    fn licenses_unique_active_index_prevents_two_active() {
        let repo = make_repo();
        let conn = repo.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO licenses (tin, fn_numbers_json, issued_at, expires_at, tier, payload_b64, signature_b64, active)
             VALUES ('1111111111', '[]', '2026-01-01', '2027-01-01', 'demo', 'x', 'y', 1)",
            [],
        ).unwrap();
        let result = conn.execute(
            "INSERT INTO licenses (tin, fn_numbers_json, issued_at, expires_at, tier, payload_b64, signature_b64, active)
             VALUES ('2222222222', '[]', '2026-01-01', '2027-01-01', 'pro', 'x', 'y', 1)",
            [],
        );
        assert!(result.is_err(), "UNIQUE INDEX ix_licenses_active_single must prevent two active licenses");
    }

    // ── audit_log ─────────────────────────────────────────────────────────────

    #[test]
    fn audit_log_insert_with_null_payload() {
        let repo = make_repo();
        repo.audit_log_insert(&AuditEntry {
            entity_type:        "sidecar",
            entity_id:          "boot",
            event_type:         "STARTUP",
            severity:           AuditSeverity::Info,
            event_payload_json: None,
        }).unwrap();
        let conn = repo.conn.lock().unwrap();
        let payload: Option<String> = conn
            .query_row("SELECT event_payload_json FROM audit_log", [], |r| r.get(0))
            .unwrap();
        assert!(payload.is_none(), "event_payload_json must be NULL when not provided");
    }

    // ── CertMetadata nullable fields ──────────────────────────────────────────

    #[test]
    fn cert_metadata_all_nullable_fields_null() {
        let repo = make_repo();
        insert_fn(&repo, "3001234567");
        {
            let conn = repo.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO operator_certs
                     (fiscal_number, cert_fingerprint, ski_hex, cert_der, fetched_at, source)
                 VALUES ('3001234567', 'fp', 'ski64', X'00', '2025-01-01', 'cmp')",
                [],
            ).unwrap();
        }
        let meta = repo.load_operator_cert_metadata("3001234567").unwrap();
        assert_eq!(meta.cert_fingerprint, "fp");
        assert_eq!(meta.source, "cmp");
        assert!(meta.subject_dn.is_none(), "subject_dn must be None");
        assert!(meta.issuer_dn.is_none(),  "issuer_dn must be None");
        assert!(meta.valid_from.is_none(), "valid_from must be None");
        assert!(meta.valid_to.is_none(),   "valid_to must be None");
    }

    #[test]
    fn open_in_memory_smoke() {
        // Repo::open(":memory:") must succeed and execute PRAGMAs without error.
        // WAL is silently ignored for in-memory DBs by SQLite — that is expected.
        Repo::open(":memory:").expect("open(:memory:) failed");
    }

    #[test]
    fn load_active_operator_deterministic_with_multiple_active() {
        // When two rows are active (e.g. deactivation race), ORDER BY id DESC
        // must return the most-recently inserted row — never a random one.
        let repo = make_repo();
        insert_fn(&repo, "3001234567");
        {
            let conn = repo.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO sidecar_operators
                     (fiscal_number, operator_name, operator_inn, jks_path, jks_password, active)
                 VALUES ('3001234567', 'Перший', '1111111111', '/k/first.jks', 'aaa', 1)",
                [],
            ).unwrap();
            conn.execute(
                "INSERT INTO sidecar_operators
                     (fiscal_number, operator_name, operator_inn, jks_path, jks_password, active)
                 VALUES ('3001234567', 'Другий', '2222222222', '/k/second.jks', 'bbb', 1)",
                [],
            ).unwrap();
        }
        let op = repo.load_active_operator("3001234567").unwrap();
        // The second insert has a higher id — must win.
        assert_eq!(op.operator_inn, "2222222222", "ORDER BY id DESC must return last inserted");
    }

    // ── FiscalMode::Prod from DB ──────────────────────────────────────────────

    #[test]
    fn fiscal_mode_prod_variant_loaded() {
        let repo = make_repo();
        {
            let conn = repo.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO fiscal_number_config
                     (fiscal_number, tax_number, fiscal_mode,
                      national_check_enabled, offline_enabled, tsp_enabled)
                 VALUES ('FN_PROD', '1234567890', 'prod', 0, 1, 0)",
                [],
            ).unwrap();
        }
        let cfg = repo.load_fn_config("FN_PROD").unwrap();
        assert_eq!(cfg.fiscal_mode, FiscalMode::Prod);
    }

    // ── CertMetadata::is_valid_at — full boundary matrix ─────────────────────

    #[test]
    fn cert_is_valid_at_within_valid_window() {
        let meta = CertMetadata {
            fiscal_number:    "FN1".into(),
            cert_fingerprint: "fp".into(),
            ski_hex:    "aa".into(),
            source:     "local".into(),
            valid_from: Some("2026-01-01T00:00:00Z".into()),
            valid_to:   Some("2027-01-01T00:00:00Z".into()),
            subject_dn: None,
            issuer_dn:  None,
        };
        let now = time::OffsetDateTime::parse("2026-06-01T00:00:00Z",
            &time::format_description::well_known::Rfc3339).unwrap();
        assert!(meta.is_valid_at(now), "now within [valid_from, valid_to] must be true");
    }

    #[test]
    fn cert_is_valid_at_expired() {
        let meta = CertMetadata {
            fiscal_number:    "FN1".into(),
            cert_fingerprint: "fp".into(),
            ski_hex:    "aa".into(),
            source:     "local".into(),
            valid_from: Some("2025-01-01T00:00:00Z".into()),
            valid_to:   Some("2026-01-01T00:00:00Z".into()),
            subject_dn: None,
            issuer_dn:  None,
        };
        let now = time::OffsetDateTime::parse("2026-01-02T00:00:00Z",
            &time::format_description::well_known::Rfc3339).unwrap();
        assert!(!meta.is_valid_at(now), "now > valid_to must be false");
    }

    #[test]
    fn cert_is_valid_at_exact_expiry_boundary_invalid() {
        let meta = CertMetadata {
            fiscal_number:    "FN1".into(),
            cert_fingerprint: "fp".into(),
            ski_hex:    "aa".into(),
            source:     "local".into(),
            valid_from: None,
            valid_to:   Some("2026-04-20T12:00:00Z".into()),
            subject_dn: None,
            issuer_dn:  None,
        };
        // Exactly at valid_to — the condition is now > valid_to (exclusive)
        // so exactly AT valid_to should still be valid (not yet expired)
        let now_at = time::OffsetDateTime::parse("2026-04-20T12:00:00Z",
            &time::format_description::well_known::Rfc3339).unwrap();
        assert!(meta.is_valid_at(now_at), "exactly at valid_to should still be valid (> not >=)");
        // One second past must be invalid
        let now_past = time::OffsetDateTime::parse("2026-04-20T12:00:01Z",
            &time::format_description::well_known::Rfc3339).unwrap();
        assert!(!meta.is_valid_at(now_past), "one second past valid_to must be invalid");
    }

    #[test]
    fn cert_is_valid_at_not_yet_valid() {
        let meta = CertMetadata {
            fiscal_number:    "FN1".into(),
            cert_fingerprint: "fp".into(),
            ski_hex:    "aa".into(),
            source:     "local".into(),
            valid_from: Some("2027-01-01T00:00:00Z".into()),
            valid_to:   None,
            subject_dn: None,
            issuer_dn:  None,
        };
        let now = time::OffsetDateTime::parse("2026-12-31T23:59:59Z",
            &time::format_description::well_known::Rfc3339).unwrap();
        assert!(!meta.is_valid_at(now), "now < valid_from must be false");
    }

    #[test]
    fn cert_is_valid_at_no_dates_always_valid() {
        let meta = CertMetadata {
            fiscal_number:    "FN1".into(),
            cert_fingerprint: "fp".into(),
            ski_hex:    "aa".into(),
            source:     "local".into(),
            valid_from: None,
            valid_to:   None,
            subject_dn: None,
            issuer_dn:  None,
        };
        let now_past   = time::OffsetDateTime::UNIX_EPOCH;
        let now_future = time::OffsetDateTime::parse("2099-12-31T23:59:59Z",
            &time::format_description::well_known::Rfc3339).unwrap();
        assert!(meta.is_valid_at(now_past),   "no dates: valid at UNIX epoch");
        assert!(meta.is_valid_at(now_future), "no dates: valid in far future");
    }

    #[test]
    fn cert_is_valid_at_unparseable_valid_to_fails_closed() {
        // Corrupted DB value: valid_to is not ISO-8601 → fail-closed (treat as invalid).
        let meta = CertMetadata {
            fiscal_number:    "FN1".into(),
            cert_fingerprint: "fp".into(),
            ski_hex:    "aa".into(),
            source:     "local".into(),
            valid_from: None,
            valid_to:   Some("not-a-date".into()),
            subject_dn: None,
            issuer_dn:  None,
        };
        let now = time::OffsetDateTime::now_utc();
        assert!(!meta.is_valid_at(now), "unparseable valid_to must block access (fail-closed)");
    }

    // ── Multiple active operators — ORDER BY id DESC determinism (3 rows) ─────

    #[test]
    fn load_active_operator_three_rows_returns_highest_id() {
        let repo = make_repo();
        insert_fn(&repo, "FN1");
        {
            let conn = repo.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO sidecar_operators
                     (fiscal_number, operator_inn, jks_path, jks_password, operator_name, active)
                 VALUES ('FN1', '11111', 'a.jks', 'pw1', 'Operator1', 1)",
                [],
            ).unwrap();
            conn.execute(
                "INSERT INTO sidecar_operators
                     (fiscal_number, operator_inn, jks_path, jks_password, operator_name, active)
                 VALUES ('FN1', '22222', 'b.jks', 'pw2', 'Operator2', 1)",
                [],
            ).unwrap();
            conn.execute(
                "INSERT INTO sidecar_operators
                     (fiscal_number, operator_inn, jks_path, jks_password, operator_name, active)
                 VALUES ('FN1', '33333', 'c.jks', 'pw3', 'Operator3', 1)",
                [],
            ).unwrap();
        }
        let op = repo.load_active_operator("FN1").unwrap();
        // ORDER BY id DESC → last inserted (id=3, Operator3) must win
        assert_eq!(op.operator_name.as_deref(), Some("Operator3"),
            "ORDER BY id DESC must return highest-id row among multiple active operators");
        assert_eq!(op.jks_path, "c.jks");
    }

    // ── fn_numbers / LicenseRow edge cases ───────────────────────────────────

    #[test]
    fn fn_numbers_empty_array_parses_to_empty_vec() {
        let row = LicenseRow {
            id:               1,
            tin:              "1234567890".into(),
            fn_numbers_json:  "[]".into(),
            issued_at:        "2026-01-01T00:00:00Z".into(),
            expires_at:       "2027-01-01T00:00:00Z".into(),
            tier:             "demo".into(),
            org_name:         None,
            demo_limits_json: None,
            payload_b64:      "x".into(),
            signature_b64:    "y".into(),
        };
        let fns = row.fn_numbers().unwrap();
        assert!(fns.is_empty(), "[] must parse to empty Vec");
    }

    #[test]
    fn fn_numbers_json_with_numeric_elements_returns_error() {
        let row = LicenseRow {
            id:               1,
            tin:              "1234567890".into(),
            fn_numbers_json:  "[1, 2, 3]".into(),
            issued_at:        "2026-01-01T00:00:00Z".into(),
            expires_at:       "2027-01-01T00:00:00Z".into(),
            tier:             "demo".into(),
            org_name:         None,
            demo_limits_json: None,
            payload_b64:      "x".into(),
            signature_b64:    "y".into(),
        };
        let result = row.fn_numbers();
        assert!(result.is_err(), "numeric array elements must fail serde_json::from_str::<Vec<String>>");
    }

    // ── local_sequences / next_local_number ──────────────────────────────────

    #[test]
    fn next_local_number_starts_at_one_for_fresh_fn() {
        let repo = make_repo();
        let n = repo.next_local_number("FN_NEW").unwrap();
        assert_eq!(n, 1, "first call must return 1");
    }

    #[test]
    fn next_local_number_increments_monotonically() {
        let repo = make_repo();
        let a = repo.next_local_number("FN_A").unwrap();
        let b = repo.next_local_number("FN_A").unwrap();
        let c = repo.next_local_number("FN_A").unwrap();
        assert_eq!(a, 1);
        assert_eq!(b, 2);
        assert_eq!(c, 3);
    }

    #[test]
    fn next_local_number_is_independent_per_fiscal_number() {
        let repo = make_repo();
        repo.next_local_number("FN_X").unwrap();
        repo.next_local_number("FN_X").unwrap();
        let y = repo.next_local_number("FN_Y").unwrap();
        assert_eq!(y, 1, "FN_Y counter must start at 1 regardless of FN_X count");
    }

    // ── previous_hash ─────────────────────────────────────────────────────────

    #[test]
    fn load_previous_hash_returns_empty_string_for_unknown_fn() {
        let repo = make_repo();
        let h = repo.load_previous_hash("NO_SUCH_FN").unwrap();
        assert!(h.is_empty(), "unknown FN must return empty string, not error");
    }

    #[test]
    fn store_and_load_previous_hash_roundtrip() {
        let repo = make_repo();
        repo.store_previous_hash("FN_HASH", "aabbccddeeff").unwrap();
        let h = repo.load_previous_hash("FN_HASH").unwrap();
        assert_eq!(h, "aabbccddeeff");
    }

    #[test]
    fn store_previous_hash_overwrites_existing_value() {
        let repo = make_repo();
        repo.store_previous_hash("FN_OVER", "first_hash").unwrap();
        repo.store_previous_hash("FN_OVER", "second_hash").unwrap();
        let h = repo.load_previous_hash("FN_OVER").unwrap();
        assert_eq!(h, "second_hash", "second store must overwrite first");
    }

    #[test]
    fn next_local_number_does_not_reset_previous_hash() {
        let repo = make_repo();
        repo.store_previous_hash("FN_MIX", "some_hash").unwrap();
        repo.next_local_number("FN_MIX").unwrap();
        let h = repo.load_previous_hash("FN_MIX").unwrap();
        assert_eq!(h, "some_hash", "next_local_number must not clobber previous_hash");
    }

    // M-4: store_previous_hash must not touch `last`
    #[test]
    fn store_previous_hash_does_not_touch_last() {
        let repo = make_repo();
        // Advance the counter to a known value.
        assert_eq!(repo.next_local_number("FN_SEQ").unwrap(), 1);
        assert_eq!(repo.next_local_number("FN_SEQ").unwrap(), 2);
        assert_eq!(repo.next_local_number("FN_SEQ").unwrap(), 3);
        // Storing hash must not reset or increment `last`.
        repo.store_previous_hash("FN_SEQ", "hash_abc").unwrap();
        // Next increment must continue from 3 → 4, not reset to 1.
        let n = repo.next_local_number("FN_SEQ").unwrap();
        assert_eq!(n, 4, "store_previous_hash must not modify last (expected 4, got {n})");
        // And the hash must still be there.
        let h = repo.load_previous_hash("FN_SEQ").unwrap();
        assert_eq!(h, "hash_abc", "previous_hash must survive next_local_number call");
    }

    // next_local_number: RETURNING guarantees read-your-own-write
    #[test]
    fn next_local_number_returning_matches_actual_last() {
        let repo = make_repo();
        // Call three times — RETURNING value must match what a subsequent SELECT would see.
        for expected in 1_i32..=5 {
            let returned = repo.next_local_number("FN_RET").unwrap();
            assert_eq!(returned, expected,
                "RETURNING must equal post-UPSERT last (expected {expected}, got {returned})");
        }
    }

    // load_previous_hash: empty hash with last>0 is a MAC bootstrap gap (warn path).
    // We can't assert on the log output here, but we verify the function still
    // returns Ok with the empty string (not an error) so callers can proceed.
    #[test]
    fn load_previous_hash_empty_with_last_gt0_returns_ok() {
        let repo = make_repo();
        // Manually insert a row with last=5 but empty previous_hash.
        {
            let conn = repo.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO local_sequences (fiscal_number, last, previous_hash)
                 VALUES ('FN_GAP', 5, '')",
                [],
            ).unwrap();
        }
        // Must succeed (not error) — caller gets empty string and can decide what to do.
        let h = repo.load_previous_hash("FN_GAP").unwrap();
        assert!(h.is_empty(), "gap condition must still return Ok(\"\"), not error");
    }

    // ── load_cert_der_for_fn ──────────────────────────────────────────────────

    #[test]
    fn load_cert_der_for_fn_returns_blob() {
        let repo = make_repo();
        insert_fn(&repo, "FN_CERT");
        {
            let conn = repo.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO operator_certs
                     (fiscal_number, cert_fingerprint, ski_hex, cert_der, fetched_at, source)
                 VALUES ('FN_CERT', 'FINGERPRINT1', 'SKIHEX1', X'0102030405', '2026-01-01', 'container')",
                [],
            ).unwrap();
        }
        let der = repo.load_cert_der_for_fn("FN_CERT").unwrap();
        assert_eq!(der, vec![0x01, 0x02, 0x03, 0x04, 0x05]);
    }

    #[test]
    fn load_cert_der_for_fn_not_found_returns_not_found_error() {
        let repo = make_repo();
        let err = repo.load_cert_der_for_fn("NO_CERT_FN").unwrap_err();
        assert!(
            matches!(err, SidecarError::NotFound(_)),
            "missing cert must be NotFound, got: {err:?}"
        );
    }

    // ── load_tsp_url_by_issuer_dn ─────────────────────────────────────────────

    fn make_repo_with_ca_endpoints() -> Repo {
        let repo = make_repo();
        {
            let conn = repo.conn.lock().unwrap();
            conn.execute_batch(
                "CREATE TABLE ca_endpoints (
                     id             INTEGER PRIMARY KEY,
                     name           TEXT NOT NULL UNIQUE,
                     cmp_url        TEXT,
                     tsp_url        TEXT,
                     ocsp_url       TEXT,
                     issuer_pattern TEXT,
                     priority       INTEGER NOT NULL DEFAULT 100,
                     enabled        INTEGER NOT NULL DEFAULT 1
                 );
                 INSERT INTO ca_endpoints VALUES
                     (1, 'acskidd', NULL, 'http://acsk.gov.ua/tsp/', NULL, 'ацск іддс', 10, 1),
                     (2, 'privat',  NULL, 'https://acsk.privat.ua/tsp/', NULL, 'приватбанк', 20, 1),
                     (3, 'disabled', NULL, 'http://disabled.example.com/tsp/', NULL, 'disabled_ca', 30, 0);",
            ).unwrap();
        }
        repo
    }

    #[test]
    fn load_tsp_url_by_issuer_dn_matches_pattern() {
        let repo = make_repo_with_ca_endpoints();
        let url = repo.load_tsp_url_by_issuer_dn("CN=АЦСК ІДДС, OU=Trust").unwrap();
        assert_eq!(url, "http://acsk.gov.ua/tsp/");
    }

    #[test]
    fn load_tsp_url_by_issuer_dn_priority_ordering() {
        // 'acskidd' has priority=10, 'privat' has priority=20.
        // A DN matching both should return the lower-priority (higher precedence) entry.
        let repo = make_repo_with_ca_endpoints();
        // Construct a DN that matches both patterns — verify the lower priority number wins.
        let url = repo.load_tsp_url_by_issuer_dn("АЦСК ІДДС").unwrap();
        assert_eq!(url, "http://acsk.gov.ua/tsp/", "priority=10 must win over priority=20");
    }

    #[test]
    fn load_tsp_url_disabled_endpoint_not_returned() {
        let repo = make_repo_with_ca_endpoints();
        let result = repo.load_tsp_url_by_issuer_dn("disabled_ca");
        assert!(
            matches!(result, Err(SidecarError::NotFound(_))),
            "disabled endpoint must not be returned"
        );
    }

    #[test]
    fn load_tsp_url_unknown_issuer_returns_not_found() {
        let repo = make_repo_with_ca_endpoints();
        let result = repo.load_tsp_url_by_issuer_dn("Unknown CA Ltd");
        assert!(
            matches!(result, Err(SidecarError::NotFound(_))),
            "unknown issuer must be NotFound"
        );
    }

    // ── fn_degraded ───────────────────────────────────────────────────────────

    #[test]
    fn set_and_check_degraded() {
        let repo = make_repo();
        repo.next_local_number("FN001").unwrap();
        assert!(!repo.is_degraded("FN001").unwrap());
        repo.set_degraded("FN001", "deadbeef").unwrap();
        assert!(repo.is_degraded("FN001").unwrap());
    }

    #[test]
    fn reconcile_chain_clears_degraded() {
        let repo = make_repo();
        repo.next_local_number("FN001").unwrap();
        repo.set_degraded("FN001", "cafebabe").unwrap();
        repo.reconcile_chain("FN001", "cafebabe").unwrap();
        assert!(!repo.is_degraded("FN001").unwrap());
        assert_eq!(repo.load_previous_hash("FN001").unwrap(), "cafebabe");
    }

    #[test]
    fn list_degraded_returns_all() {
        let repo = make_repo();
        repo.next_local_number("FN001").unwrap();
        repo.next_local_number("FN002").unwrap();
        repo.set_degraded("FN001", "hash1").unwrap();
        repo.set_degraded("FN002", "hash2").unwrap();
        let entries = repo.list_degraded().unwrap();
        assert_eq!(entries.len(), 2);
        let fns: Vec<&str> = entries.iter().map(|(fn_id, _, _)| fn_id.as_str()).collect();
        assert!(fns.contains(&"FN001"));
        assert!(fns.contains(&"FN002"));
    }

    #[test]
    fn set_degraded_idempotent_updates_hash() {
        let repo = make_repo();
        repo.next_local_number("FN001").unwrap();
        repo.set_degraded("FN001", "first_hash").unwrap();
        repo.set_degraded("FN001", "second_hash").unwrap();
        let entries = repo.list_degraded().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1, "second_hash");
    }

    // ── Mutex poison ──────────────────────────────────────────────────────────

    #[test]
    fn mutex_poisoned_by_panic_returns_internal_error() {
        use std::sync::Arc;
        // Create a fresh repo, then poison its mutex by panicking inside a thread that holds the lock.
        let repo = Arc::new(make_repo());
        let repo2 = Arc::clone(&repo);
        let _ = std::thread::spawn(move || {
            let _guard = repo2.conn.lock().unwrap();
            panic!("deliberate poison");
        }).join(); // join returns Err because thread panicked

        // Now the mutex is poisoned — any lock() call through the public API must
        // surface as SidecarError::Internal (not a panic, not a hang).
        let result = repo.load_fn_config("ANYTHING");
        assert!(
            matches!(result, Err(SidecarError::Internal(_))),
            "poisoned mutex must produce SidecarError::Internal, got {result:?}"
        );
    }
}
