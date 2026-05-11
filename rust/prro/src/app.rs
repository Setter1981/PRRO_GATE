//! Composition root.  M1 wires DB pool + config; M2+ adds crypto,
//! transports, services.
//!
//! IMPORTANT (per bd-issue PRRO_GATE-ah8): `App::boot` MUST NOT blindly
//! call `node_state::upsert_initial(fn, Online, Closed, 1)` for every
//! configured FN.  Doing so would overwrite a `shift_state = Opened`
//! left by a crashed in-flight shift and mask the recovery requirement.
//! M1's boot only opens the pool + applies migrations; bootstrap of
//! `node_state` rows is deferred to a later task with explicit
//! reconciliation against `shifts` / `fiscal_documents`.
//!
//! W9 (per `docs/superpowers/specs/2026-05-10-m3a-w9-boot-reconciliation-design.md`)
//! extends boot to a fail-closed pre-flight pipeline:
//!   1. parent dir create (existing M1).
//!   2. singleton lock acquire (W9 — moved here from main.rs's per-command
//!      caller to consolidate boot pipeline; lifetime = App lifetime).
//!   3. pool open + migrations (existing M1 via `db::open_pool`).
//!   4. `PRAGMA quick_check(1)` integrity probe — fail-closed on any
//!      non-"ok" result with ZERO writes to any table (per W0-3 §4.2
//!      step 3; writing into a corrupt DB compounds corruption).
//!
//! The `App::reconcile_pending` method (per-FN decision tree per W0-3
//! §4) is stubbed in W9.1; W9.2 + W9.3 land helpers + dispatch.

use crate::config::AppConfig;
use crate::runtime::singleton::PidLock;
use sqlx::SqlitePool;
use std::sync::Arc;

/// Typed boot failure surface.
///
/// Variants map to BSD sysexits via [`BootError::exit_code`] for the
/// `prro` binary to surface specific operator-actionable exit codes
/// (per W9 freeze §5.5).
///
/// `sqlx::Error` is intentionally not `Clone`/`PartialEq`; tests use
/// `match` patterns to assert variant shape (per freeze §10.2 LOW 1
/// fix).
#[derive(Debug, thiserror::Error)]
pub enum BootError {
    #[error("DB integrity check failed: {reason}")]
    IntegrityCheckFailed { reason: String },

    #[error("FN {fiscal_number} is in OFFLINE mode — start with --recover-offline M3b CLI")]
    OfflineModeRefusal { fiscal_number: String },

    #[error("database error during boot: {0}")]
    Database(#[from] sqlx::Error),

    /// Config file IO failure (W9.1 review LOW 3 fix — symmetric
    /// sysexits mapping so missing config and corrupt DB give
    /// distinct operator-actionable exit codes).
    #[error("config file read failed: {0}")]
    ConfigRead(#[from] std::io::Error),

    /// Config file parse failure (W9.1 review LOW 3 fix).  Stored
    /// as String because `AppConfig::from_toml` returns
    /// `anyhow::Result` and `anyhow::Error` is not `Error + Send +
    /// Sync` in the thiserror derive context without explicit
    /// shimming.
    #[error("config file parse failed: {0}")]
    ConfigParse(String),

    #[error("internal boot error: {0}")]
    Internal(String),
}

impl BootError {
    /// BSD sysexits-compatible exit code (per freeze §5.5 + W9.1
    /// review LOW 3 fix adding config-error variants).
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::IntegrityCheckFailed { .. } => 65, // EX_DATAERR
            Self::OfflineModeRefusal { .. } => 78,   // EX_CONFIG
            Self::Database(_) => 71,                 // EX_OSERR
            Self::ConfigRead(_) => 66,               // EX_NOINPUT
            Self::ConfigParse(_) => 65,              // EX_DATAERR (config IS data)
            Self::Internal(_) => 70,                 // EX_SOFTWARE
        }
    }
}

/// Application root.
///
/// **Singleton-lock lifetime semantics (W9.1 review NIT 2):**
/// `App` is `Clone`; all clones share the same `Arc<Inner>`, which
/// owns the `PidLock`.  The OS advisory lock is released only when
/// the LAST `App` clone drops (`Arc` refcount → 0).  Long-lived
/// clones held in disconnected threads will keep the lock past the
/// intended scope.  Conventionally, `App` is constructed once in
/// `main.rs::boot_or_exit` and passed by reference / cloned only
/// into short-lived task scopes that drop at shutdown.
#[derive(Clone)]
pub struct App {
    inner: Arc<Inner>,
}

struct Inner {
    config: AppConfig,
    db: SqlitePool,
    /// Singleton process lock — held for App lifetime.  Dropped on App
    /// drop, releasing the OS advisory lock.  Per freeze NIT 1 fix:
    /// field has no underscore prefix because it IS load-bearing via
    /// RAII (the field's Drop is the unlock mechanism).  Suppress
    /// dead_code lint because the field is never read directly.
    #[allow(dead_code)]
    singleton: PidLock,
}

impl App {
    pub async fn boot(config: AppConfig) -> Result<Self, BootError> {
        // (1) Parent dir create (existing M1 behaviour).
        if let Some(parent) = config.database.db_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    BootError::Internal(format!("creating db parent dir {}: {e}", parent.display()))
                })?;
            }
        }

        // (2) Singleton lock — sync `pub fn` (file-descriptor advisory
        // lock; per freeze HIGH 3 fix, no `.await`).  Returns
        // `anyhow::Result<PidLock>`; wrap into BootError::Internal.
        let singleton = crate::runtime::singleton::acquire(&config.database.db_path)
            .map_err(|e| BootError::Internal(format!("singleton lock: {e}")))?;

        // (3) Pool open + migrations (sqlx::migrate! inside `open_pool`).
        //     `open_pool` returns `anyhow::Error`; if the underlying cause
        //     is `sqlx::Error` preserve that as `BootError::Database`
        //     (W9.1 review LOW 1 fix — type-information preservation).
        //     Otherwise fall back to `BootError::Internal`.
        let db = crate::db::open_pool(&config.database.db_path)
            .await
            .map_err(|e| match e.downcast::<sqlx::Error>() {
                Ok(sqlx_err) => BootError::Database(sqlx_err),
                Err(other) => BootError::Internal(format!("open_pool: {other}")),
            })?;

        // (4) Integrity probe — `PRAGMA quick_check(1)` (cap at first
        // error; we only need fail-closed signal).  Use `query_scalar`
        // for single-column result (cycle-7 LOW 1 fix).  `fetch_all`
        // is defensive against zero-row corruption shapes.
        let rows: Vec<String> = sqlx::query_scalar("PRAGMA quick_check(1)")
            .fetch_all(&db)
            .await?;
        let reason = match rows.as_slice() {
            [s] if s == "ok" => None,
            [first, ..] => Some(first.clone()),
            [] => Some(String::from("quick_check returned zero rows")),
        };
        if let Some(reason) = reason {
            tracing::error!(
                target: "prro::boot",
                quick_check = %reason,
                "DB_INTEGRITY_CHECK_FAILED"
            );
            return Err(BootError::IntegrityCheckFailed { reason });
        }

        Ok(Self {
            inner: Arc::new(Inner {
                config,
                db,
                singleton,
            }),
        })
    }

    /// Per-FN decision tree (W0-3 §4.3, 6 branches).
    ///
    /// W9.1 stub: returns `Ok(())` without iterating FNs.  W9.3 wires
    /// the dispatch.  Lives here because it's the public callable from
    /// `main.rs` between `App::boot` and accepting ingress traffic.
    pub async fn reconcile_pending(&self) -> Result<(), BootError> {
        // TODO(W9.3): iterate fiscal_number_config::list_all, dispatch
        // via services::reconciliation::boot_phase::run_boot_reconciliation.
        Ok(())
    }

    pub fn config(&self) -> &AppConfig {
        &self.inner.config
    }

    pub fn db(&self) -> &SqlitePool {
        &self.inner.db
    }
}
