//! W2 — operator-bindings registry.
//!
//! Per `docs/superpowers/plans/2026-05-25-m4-ingress-plan.md` §3 W2:
//! the registry binds one cashier (EDS key + signing context) to each
//! configured `fiscal_number`.  Built once at boot from the secure
//! `operators` table; consulted at runtime by the per-FN supervisor
//! (W7) and reconciliation (`ReconciliationRuntime::with_resolver`).
//!
//! ## Hot-zone discipline
//!
//! Construction is **boot-only** — outside any `with_immediate`
//! transaction.  Invariant #1 is preserved by construction: the only
//! DB writes performed inside `build_from_db` are audit-log appends
//! (one row per skip / failure), each a single short statement against
//! the main pool's autocommit mode, never inside a long-running tx.
//!
//! ## Cross-DB FK runtime check (HIGH-PR90-01)
//!
//! SQLite cannot enforce foreign keys across attached database files,
//! so `operators.fiscal_number` does not have a SQL-level `REFERENCES
//! fiscal_number_config(fiscal_number)` clause.  The compensating
//! check lives here: every loaded operator row's `fiscal_number` is
//! looked up against the main pool's `fiscal_number_config` table;
//! mismatches emit `OPERATOR_ORPHAN_FN` Critical audit and are skipped
//! from the registry.
//!
//! ## Audit event taxonomy
//!
//! | Event | Severity | Fired when |
//! |---|---|---|
//! | `OPERATOR_KEY_LOAD_FAILED` | Critical | EDS key file missing or password wrong |
//! | `OPERATOR_ORPHAN_FN` | Critical | operators row references FN not in fiscal_number_config |
//! | `OPERATOR_NOT_REGISTERED` | Info | configured FN has no active operators row |
//!
//! All three are non-aborting — boot continues; the registry simply
//! lacks the FN.  Handler then returns 503 + `OPERATOR_NOT_REGISTERED`
//! for any request targeting an unregistered FN (W5 surface).

use crate::db::repositories::audit_log;
use crate::db::repositories::operators;
use crate::db::models::enums::Severity;
use crate::runtime::coding::Coding;
use crate::services::write_path::stage_sign::SigningContext;
use crate::transports::dps::channel::DpsChannel;
use async_trait::async_trait;
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

/// Per-FN bindings — `Arc<DpsChannel>` is shared across all entries
/// (one channel for the whole prro instance); each `sign_ctx` is
/// cashier-specific.
pub struct OperatorBindings {
    pub dps: Arc<dyn DpsChannel>,
    pub sign_ctx: SigningContext,
}

/// Boot-built read-only registry.  Keyed by `fiscal_number`.
pub struct BindingsRegistry {
    inner: HashMap<String, OperatorBindings>,
}

impl BindingsRegistry {
    /// Lookup binding for a fiscal_number.  Returns `None` for any FN
    /// that was not registered, key-failed, orphaned, or otherwise
    /// skipped during boot.  Callers MUST treat `None` as "respond
    /// 503 OPERATOR_NOT_REGISTERED" rather than panicking.
    pub fn get(&self, fn_id: &str) -> Option<&OperatorBindings> {
        self.inner.get(fn_id)
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn fns(&self) -> impl Iterator<Item = &str> {
        self.inner.keys().map(|s| s.as_str())
    }
}

/// Failure modes from a key-load attempt.  The variants are exposed
/// (not opaque) so [`BindingsRegistry::build_from_db`] can branch on
/// them when emitting the `reason` field of the
/// `OPERATOR_KEY_LOAD_FAILED` audit event.
#[derive(Debug, Error)]
pub enum KeyLoadFailure {
    #[error("key file not found at {0}")]
    FileNotFound(PathBuf),
    #[error("wrong password for key file {0}")]
    WrongPassword(PathBuf),
    #[error("other key load error: {0}")]
    Other(String),
}

impl KeyLoadFailure {
    /// Stable audit-friendly reason tag — used as the `reason` field
    /// of the `OPERATOR_KEY_LOAD_FAILED` event payload.
    pub fn reason(&self) -> &'static str {
        match self {
            Self::FileNotFound(_) => "FileNotFound",
            Self::WrongPassword(_) => "WrongPassword",
            Self::Other(_) => "Other",
        }
    }
}

/// Loader for cashier EDS keys.  Production impl reads the
/// `.dat`/`.jks` carrier at `key_path` and decrypts with `password`;
/// tests inject a stub returning a deterministic [`SigningContext`].
/// The seam exists so the W2 boot orchestration can land before the
/// production loader (M5 crypto wiring task) is finished.
///
/// # Secret-material discipline (M5 implementor MUST read)
///
/// The `password` parameter arrives as a borrowed `&[u8]` slice that
/// points into a [`Zeroizing<Vec<u8>>`] owned by [`BindingsRegistry::
/// build_from_db`] — the caller wipes the underlying buffer on drop
/// (PR-B audit Round 2 R2-4 closure).  Implementations MUST honor the
/// zeroize discipline at the trait boundary:
///
///   1. **Do NOT clone the slice into a plain `Vec<u8>` / `String`
///      / `Cow<[u8]>` / any other un-zeroized heap buffer.**  Such a
///      copy survives past `load` return without the wipe guarantee,
///      leaving the cashier password recoverable in freed heap pages.
///   2. If a temporary owned copy is unavoidable (e.g., the underlying
///      crypto library requires `Vec<u8>` by value), wrap it
///      immediately in [`zeroize::Zeroizing::new`] so Drop wipes it.
///   3. The returned [`SigningContext`] MUST NOT retain the plaintext
///      password — it should derive the signing key (or a key handle)
///      and discard the password before returning.
///
/// External audit Round 2 R2-4 explicitly flagged the trait boundary
/// as the weakest link in the password chain; this doc-block is the
/// load-bearing reminder for the M5 implementor.
#[async_trait]
pub trait OperatorKeyLoader: Send + Sync {
    async fn load(
        &self,
        key_path: &Path,
        password: &[u8],
    ) -> Result<SigningContext, KeyLoadFailure>;
}

impl BindingsRegistry {
    /// Build the registry from the secure DB + main DB cross-check.
    ///
    /// Steps (in order):
    ///
    ///   1. Load all `is_active = 1` operator rows from `pool_secure`.
    ///   2. Load all `fiscal_number_config.fiscal_number` rows from
    ///      `pool_main` into a `HashSet` — this set is both the
    ///      cross-DB FK check (step 3a) AND the authoritative list of
    ///      FNs that MUST have an operators row (step 4).  Callers
    ///      do not pass the configured list separately; the main DB
    ///      is the sole source of truth so caller-DB drift is impossible.
    ///   3. For each operator row:
    ///       a. If `fiscal_number` not in the FK set → emit
    ///          `OPERATOR_ORPHAN_FN` Critical audit + skip.
    ///       b. Decode `key_pass_enc` via [`Coding::decode`]; on empty
    ///          → emit `OPERATOR_KEY_LOAD_FAILED` Critical with
    ///          `reason="EmptyEncoded"` + skip.
    ///       c. Call `loader.load(&key_path, &password)`.  On
    ///          [`KeyLoadFailure`] → emit `OPERATOR_KEY_LOAD_FAILED`
    ///          Critical with `reason=<variant>` + skip.
    ///       d. On success → insert into registry.
    ///   4. For each FN in `main_fns` that ended up without a registry
    ///      entry: emit `OPERATOR_NOT_REGISTERED` Info audit.
    ///
    /// Returns `Ok(Self)` even if every operator failed — the empty
    /// registry is operationally valid (every handler will 503).
    /// Returns `Err` only on hard infrastructure errors (DB
    /// unreachable, etc.) where boot cannot continue.
    pub async fn build_from_db(
        pool_secure: &SqlitePool,
        pool_main: &SqlitePool,
        dps: Arc<dyn DpsChannel>,
        loader: &dyn OperatorKeyLoader,
    ) -> anyhow::Result<Self> {
        let rows = operators::list_all(pool_secure).await?;
        let active_rows: Vec<_> = rows.into_iter().filter(|r| r.is_active).collect();

        // Cross-DB FK set: every FN that exists in main's
        // fiscal_number_config.  An operators row whose fiscal_number
        // is NOT here is an orphan.
        let main_fns: std::collections::HashSet<String> = sqlx::query(
            "SELECT fiscal_number FROM fiscal_number_config",
        )
        .fetch_all(pool_main)
        .await?
        .into_iter()
        .map(|r| r.get::<String, _>("fiscal_number"))
        .collect();

        let mut inner: HashMap<String, OperatorBindings> = HashMap::new();

        let total_active = active_rows.len();
        let mut skipped_orphan: usize = 0;
        let mut skipped_keyload: usize = 0;

        for row in active_rows {
            // Step 3a — cross-DB FK check.
            //
            // PII discipline: `tracing::warn!` MUST NOT include the raw
            // `operator_id` field.  In the UA PRRO domain `operator_id`
            // is the cashier's INN (Identification Tax Number) — Personal
            // Identifiable Information under Ukrainian privacy law.
            // Process logs (stderr → journald / Loki) are not designed
            // for PII retention, so we log only the surrogate row id +
            // the `fiscal_number` (which is the entity scope key, not
            // personally identifying on its own).  The full payload
            // (including operator_id) still lands in `audit_log`, which
            // IS the appropriate retention surface for that data.
            if !main_fns.contains(&row.fiscal_number) {
                tracing::warn!(
                    target: "prro::runtime::bindings",
                    fiscal_number = %row.fiscal_number,
                    operators_row_id = row.id,
                    "OPERATOR_ORPHAN_FN: operators row references FN not in fiscal_number_config; skipping"
                );
                let payload = serde_json::json!({
                    "operator_id": row.operator_id,
                    "key_path": row.key_path,
                })
                .to_string();
                let _ = audit_log::append(
                    pool_main,
                    "operator",
                    &row.fiscal_number,
                    "OPERATOR_ORPHAN_FN",
                    Severity::Critical,
                    None,
                    Some(&payload),
                )
                .await;
                skipped_orphan += 1;
                continue;
            }

            // Step 3b — decode password.
            let password = match Coding::decode(&row.key_pass_enc) {
                Ok(p) => p,
                Err(_) => {
                    tracing::warn!(
                        target: "prro::runtime::bindings",
                        fiscal_number = %row.fiscal_number,
                        operators_row_id = row.id,
                        reason = "EmptyEncoded",
                        "OPERATOR_KEY_LOAD_FAILED: empty key_pass_enc BLOB; skipping"
                    );
                    let payload = serde_json::json!({
                        "operator_id": row.operator_id,
                        "key_path": row.key_path,
                        "reason": "EmptyEncoded",
                    })
                    .to_string();
                    let _ = audit_log::append(
                        pool_main,
                        "operator",
                        &row.fiscal_number,
                        "OPERATOR_KEY_LOAD_FAILED",
                        Severity::Critical,
                        None,
                        Some(&payload),
                    )
                    .await;
                    skipped_keyload += 1;
                    continue;
                }
            };

            // Step 3c — call loader.
            match loader.load(Path::new(&row.key_path), &password).await {
                Ok(sign_ctx) => {
                    inner.insert(
                        row.fiscal_number.clone(),
                        OperatorBindings {
                            dps: Arc::clone(&dps),
                            sign_ctx,
                        },
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        target: "prro::runtime::bindings",
                        fiscal_number = %row.fiscal_number,
                        operators_row_id = row.id,
                        reason = e.reason(),
                        "OPERATOR_KEY_LOAD_FAILED: loader rejected key; skipping"
                    );
                    let payload = serde_json::json!({
                        "operator_id": row.operator_id,
                        "key_path": row.key_path,
                        "reason": e.reason(),
                    })
                    .to_string();
                    let _ = audit_log::append(
                        pool_main,
                        "operator",
                        &row.fiscal_number,
                        "OPERATOR_KEY_LOAD_FAILED",
                        Severity::Critical,
                        None,
                        Some(&payload),
                    )
                    .await;
                    skipped_keyload += 1;
                }
            }
        }

        // Step 4 — emit OPERATOR_NOT_REGISTERED for every FN in
        // fiscal_number_config that ended up without a registry entry
        // (whether by orphan, key-fail, or missing operators row).
        // Iterating over `main_fns` (the authoritative source) instead
        // of a caller-passed list eliminates caller-DB drift.
        let mut not_registered: usize = 0;
        for fn_id in &main_fns {
            if !inner.contains_key(fn_id) {
                let _ = audit_log::append(
                    pool_main,
                    "operator",
                    fn_id,
                    "OPERATOR_NOT_REGISTERED",
                    Severity::Info,
                    None,
                    None,
                )
                .await;
                not_registered += 1;
            }
        }

        tracing::info!(
            target: "prro::runtime::bindings",
            registered = inner.len(),
            total_operators_rows = total_active,
            skipped_orphan = skipped_orphan,
            skipped_keyload = skipped_keyload,
            configured_fns_without_operator = not_registered,
            "BindingsRegistry built"
        );

        Ok(Self { inner })
    }
}
