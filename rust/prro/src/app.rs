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
use crate::services::reconciliation::boot_phase::ReconciliationSummary;
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

    /// W9.4 cycle-2 MED-B fix: preserves per-FN attribution + source
    /// error context when a `run_boot_reconciliation` call surfaces
    /// a non-sqlx anyhow chain (e.g. terminal-state contract bail,
    /// audit insert failure inside `with_immediate`).  Earlier code
    /// stringified through `BootError::Internal`, losing the
    /// `fiscal_number` tag operators need for triage.
    #[error("reconcile_pending({fiscal_number}): {source}")]
    ReconciliationFailed {
        fiscal_number: String,
        #[source]
        source: anyhow::Error,
    },

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
            Self::ReconciliationFailed { .. } => 70, // EX_SOFTWARE — per-FN internal
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
    /// Iterates `fiscal_number_config::list_all` sequentially (ordered
    /// by `fiscal_number` ascending — deterministic for tests).  For
    /// each FN, delegates to
    /// `services::reconciliation::boot_phase::run_boot_reconciliation`.
    ///
    /// **Returns** [`ReconciliationSummary`] aggregating per-FN branch
    /// counts + per-DocState dispatch histogram (W9.4 M1 fix per
    /// freeze §2.1).  Operator / test consumers read the summary
    /// directly instead of re-querying `audit_log`.
    ///
    /// **OFFLINE refusal (freeze §3.4 + §13.3).**  If any FN returns
    /// `BranchOutcome::OfflineRefusal`, this method fails-fast on the
    /// FIRST such FN encountered and returns
    /// `BootError::OfflineModeRefusal { fiscal_number }`.  The audit
    /// row `NODE_STATE_BOOT_OFFLINE_REFUSAL` was already emitted by
    /// `run_boot_reconciliation` before returning the outcome.  Partial
    /// progress for FNs already processed BEFORE the refusal is
    /// committed (each per-FN `with_immediate` envelope is atomic);
    /// the summary that would have been returned is discarded.
    ///
    /// **Ctx-needy dispatch deferral (W9.3).**  Per-DocState
    /// dispatches that require `DpsChannel` / `SigningContext`
    /// (PREPARED / SIGNED / SENT / ERROR_RETRYABLE) emit
    /// `BOOT_DISPATCH_DEFERRED` audit and leave the doc in its source
    /// state.  Runtime composition (W11) lands via
    /// [`reconcile_pending_with`] — that entry point accepts a
    /// [`ReconciliationRuntime`] which carries `DpsChannel` +
    /// `SigningContext`.  PR-1a plumbs the new entry; PR-2 wires the
    /// four ctx-needy arms.  Until PR-2 lands, even
    /// `reconcile_pending_with` emits `BOOT_DISPATCH_DEFERRED` for
    /// those four states, but with `deps_available: true` in the
    /// audit payload (vs `false` from this ctx-free path).
    ///
    /// [`reconcile_pending_with`]: Self::reconcile_pending_with
    /// [`ReconciliationRuntime`]: crate::services::reconciliation::ReconciliationRuntime
    pub async fn reconcile_pending(&self) -> Result<ReconciliationSummary, BootError> {
        self.reconcile_pending_inner(None).await
    }

    /// W11 runtime-composed boot reconciliation.  Accepts a
    /// [`ReconciliationRuntime`] carrying the `DpsChannel` and
    /// `SigningContext` that ctx-needy dispatch arms (PREPARED /
    /// SIGNED / SENT / ERROR_RETRYABLE) require.
    ///
    /// PR-1a (W11) ships the plumbing.  PR-1b adds the KVT2 / KVT1
    /// fixture proofs (those branches are already ctx-free in W9 —
    /// they consume only `pool + doc_id`).  PR-2 wires the four
    /// ctx-needy arms to consume `deps`.
    ///
    /// Under PR-1a, calling `reconcile_pending_with` is observationally
    /// identical to [`reconcile_pending`] for the four ctx-needy
    /// states — both emit `BOOT_DISPATCH_DEFERRED` — but the audit
    /// payload carries `deps_available: true`, providing operator
    /// trace of which boot tick had the runtime composition in place.
    ///
    /// Per ADR-M3-A10: under the global-single-writer invariant, this
    /// call holds the dispatcher task for the duration of one boot
    /// pass; concurrent invocation across the same `App` instance is
    /// not supported.
    ///
    /// [`reconcile_pending`]: Self::reconcile_pending
    /// [`ReconciliationRuntime`]: crate::services::reconciliation::ReconciliationRuntime
    pub async fn reconcile_pending_with<'a>(
        &self,
        deps: crate::services::reconciliation::ReconciliationRuntime<'a>,
    ) -> Result<ReconciliationSummary, BootError> {
        self.reconcile_pending_inner(Some(&deps)).await
    }

    async fn reconcile_pending_inner(
        &self,
        deps: Option<&crate::services::reconciliation::ReconciliationRuntime<'_>>,
    ) -> Result<ReconciliationSummary, BootError> {
        use crate::db::repositories::fiscal_number_config;
        use crate::services::reconciliation::boot_phase::{self, BranchOutcome};

        let pool = self.db();
        let fns = fiscal_number_config::list_all(pool)
            .await
            .map_err(BootError::Database)?;
        let mut summary = ReconciliationSummary::default();
        for fn_cfg in &fns {
            // M3a hardening pass 1: resolve per-FN RuntimeView BEFORE
            // dispatching.  `ReconciliationRuntime::resolve` returns
            // `Some(view)` only when the caller's resolver acknowledges
            // a binding for this specific FN; `None` falls through to
            // the ctx-free path (emits `BOOT_DISPATCH_DEFERRED` for any
            // ctx-needy pending docs).  Recovery NEVER substitutes
            // foreign identity — see `runtime::ReconciliationRuntime`
            // doc-comment.
            let per_fn_view = deps.and_then(|r| r.resolve(&fn_cfg.fiscal_number));
            // MED-B fix (cycle-2): route per-FN failures through
            // ReconciliationFailed which preserves `fiscal_number`
            // attribution + `source` anyhow::Error chain.  Only raw
            // sqlx::Error downcasts to Database (existing pattern).
            let outcome = boot_phase::run_boot_reconciliation(
                pool,
                &fn_cfg.fiscal_number,
                per_fn_view.as_ref(),
            )
            .await
            .map_err(|e| match e.downcast::<sqlx::Error>() {
                Ok(sqlx_err) => BootError::Database(sqlx_err),
                Err(other) => BootError::ReconciliationFailed {
                    fiscal_number: fn_cfg.fiscal_number.clone(),
                    source: other,
                },
            })?;
            if let BranchOutcome::OfflineRefusal { .. } = outcome {
                return Err(BootError::OfflineModeRefusal {
                    fiscal_number: fn_cfg.fiscal_number.clone(),
                });
            }
            summary.record(&outcome);
        }
        Ok(summary)
    }

    pub fn config(&self) -> &AppConfig {
        &self.inner.config
    }

    pub fn db(&self) -> &SqlitePool {
        &self.inner.db
    }
}
