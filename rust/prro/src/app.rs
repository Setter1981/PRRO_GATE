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

    /// M3b W7b update (2026-05-16): post-merge, only `GoingOnline`
    /// mode triggers this fail-fast at boot.  `Offline` /
    /// `GoingOffline` modes no longer abort boot — boot reconciliation
    /// processes their docs through the W7b post-sign dispatcher
    /// (`services::write_path::dispatch::dispatch_post_sign`).  This
    /// variant name is preserved for ABI continuity but the
    /// operator-facing message reflects the new semantic.
    #[error(
        "FN {fiscal_number} is in GOING_ONLINE mode — W9 backlog drain owns this FN's reconciliation; \
         re-run boot once the drain has completed and node_state.mode is Online or Offline again"
    )]
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
    /// M3a hardening pass 2 — structural enforcement of ADR-M3-A10
    /// (`docs/superpowers/specs/2026-05-12-adr-m3-a10-global-single-writer.md`).
    /// The ADR mandates "runtime enforces global-single-writer" + "one
    /// tokio worker"; prior to this field the invariant was convention-
    /// only.  `App: Clone` allows multiple handles, and
    /// `reconcile_pending_with` is `pub`, so two parallel calls could
    /// each CAS Signed → Sending + fire `send_chk` between the 4-pre
    /// and 4-b envelopes.  The mutex serialises every boot-recovery
    /// dispatcher call so the per-row CAS + `BEGIN IMMEDIATE` envelope
    /// guarantees compose with a top-level App-scoped serialisation.
    ///
    /// `tokio::sync::Mutex` (not `std::sync::Mutex`) because the
    /// critical section spans `.await` points across many short
    /// per-FN envelopes — holding a `std::sync::Mutex` across `.await`
    /// would block the runtime under contention.  Mutex held for the
    /// duration of one `reconcile_pending_inner` call; concurrent
    /// callers serialise without panicking.
    reconcile_mutex: tokio::sync::Mutex<()>,
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

        // (3a) M3a hardening pass 3 — pre-migration integrity probe
        //      for existing DBs.  Per Finding 2 of the second deep
        //      review: `sqlx::migrate!` re-applies on corrupted DBs
        //      and can silently overwrite the corrupted region (the
        //      4 `#[ignore]`d fixtures in `app_boot_quick_check_failure.rs`
        //      were blocked by exactly this).  Two-phase open:
        //        (a) Existing DB → probe-only open WITHOUT migrate
        //            → `PRAGMA quick_check(1)` → close.  If
        //            quick_check fails, return
        //            `BootError::IntegrityCheckFailed` BEFORE any
        //            migration runs against the corrupted file; no
        //            domain writes (audit_log / node_state / shifts)
        //            can land on a broken DB.
        //        (b) Fresh DB → skip Phase A (file is empty / absent;
        //            quick_check on header-only file would either
        //            fail spuriously or pass trivially); fall through
        //            to the standard create+migrate path at (3b).
        //      Singleton lock (step 2) already prevents another
        //      process from racing this two-phase open.
        let db_path = &config.database.db_path;
        let db_exists = db_path.try_exists().unwrap_or(false)
            && std::fs::metadata(db_path)
                .map(|m| m.len() > 0)
                .unwrap_or(false);
        if db_exists {
            // Phase A.  Attempt probe-open; if that itself fails on
            // an EXISTING file (which we just verified above), the
            // most likely cause is structural damage that prevents
            // SQLite from even initialising the pager.  Surface as
            // `IntegrityCheckFailed` — same fail-closed semantics as
            // a malformed-row quick_check result.  Transient I/O
            // errors are extremely rare on a local SQLite file we
            // already metadata'd one syscall ago; if they happen,
            // the operator will retry boot.
            let probe = match crate::db::open_pool_no_migrate(db_path).await {
                Ok(p) => p,
                Err(open_err) => {
                    let sqlx_err_opt = open_err.downcast::<sqlx::Error>();
                    let reason = match &sqlx_err_opt {
                        Ok(e) => format!("probe pool open failed: {e}"),
                        Err(other) => format!("probe pool open failed: {other}"),
                    };
                    tracing::error!(
                        target: "prro::boot",
                        phase = "pre_migrate_open",
                        quick_check = %reason,
                        "DB_INTEGRITY_CHECK_FAILED"
                    );
                    return Err(BootError::IntegrityCheckFailed { reason });
                }
            };
            // Run quick_check + map SQLite corruption-class errors
            // to `IntegrityCheckFailed`.  When the file's btree
            // pages are damaged but the header still parses,
            // `connect_with` succeeds but `PRAGMA quick_check(1)`
            // returns `SQLITE_CORRUPT` (code 11) at query execution
            // rather than completing with a malformed-row report.
            // Both shapes must produce the same operator-facing
            // signal: the DB is broken; do NOT migrate.
            let quick_check_result = sqlx::query_scalar::<_, String>("PRAGMA quick_check(1)")
                .fetch_all(&probe)
                .await;
            // Close the probe pool BEFORE returning OR re-opening
            // with migrate.  We don't want two pools racing the WAL
            // lock, and we don't want a leaked probe handle on the
            // fail path either.
            probe.close().await;
            let reason = match quick_check_result {
                Ok(rows) => match rows.as_slice() {
                    [s] if s == "ok" => None,
                    [first, ..] => Some(first.clone()),
                    [] => Some(String::from("quick_check returned zero rows")),
                },
                // Any sqlx error at quick_check time on an existing
                // DB indicates the file is damaged enough that
                // SQLite couldn't complete the structural check.
                // Same fail-closed treatment as a malformed-row
                // result: surface as IntegrityCheckFailed and abort
                // before migrations can write into the file.
                Err(err) => Some(format!("quick_check query failed: {err}")),
            };
            if let Some(reason) = reason {
                tracing::error!(
                    target: "prro::boot",
                    phase = "pre_migrate",
                    quick_check = %reason,
                    "DB_INTEGRITY_CHECK_FAILED"
                );
                // Critical: return BEFORE any migration runs.  No
                // domain writes touch the corrupted file.
                return Err(BootError::IntegrityCheckFailed { reason });
            }
        }

        // (3b) Pool open + migrations (sqlx::migrate! inside `open_pool`).
        //      `open_pool` returns `anyhow::Error`; if the underlying cause
        //      is `sqlx::Error` preserve that as `BootError::Database`
        //      (W9.1 review LOW 1 fix — type-information preservation).
        //      Otherwise fall back to `BootError::Internal`.
        let db = crate::db::open_pool(&config.database.db_path)
            .await
            .map_err(|e| match e.downcast::<sqlx::Error>() {
                Ok(sqlx_err) => BootError::Database(sqlx_err),
                Err(other) => BootError::Internal(format!("open_pool: {other}")),
            })?;

        // (4) Defence-in-depth post-migrate `PRAGMA quick_check(1)`.
        //     For existing DBs this is redundant with Phase A (3a);
        //     for fresh DBs this is the first opportunity to validate
        //     that sqlx::migrate! produced a structurally sound file.
        //     `query_scalar` + `fetch_all` defensive against
        //     zero-row corruption shapes (cycle-7 LOW 1 fix).
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
                phase = "post_migrate",
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
                reconcile_mutex: tokio::sync::Mutex::new(()),
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
    /// **Ctx-needy dispatch deferral on this entry.**  Per-DocState
    /// dispatches that require `DpsChannel` / `SigningContext`
    /// (PREPARED / SIGNED / SENT / ERROR_RETRYABLE) emit
    /// `BOOT_DISPATCH_DEFERRED` audit and leave the doc in its source
    /// state when invoked through `reconcile_pending` (ctx-free entry).
    /// Runtime composition lands via [`reconcile_pending_with`] —
    /// that entry point accepts a [`ReconciliationRuntime`] resolver
    /// that the dispatcher consults per FN to drive the ctx-needy
    /// arms forward.  Use this entry only for ctx-free recovery
    /// (e.g. test harnesses or operator probes that do not need to
    /// touch DPS / crypto); production boot should call
    /// [`reconcile_pending_with`] with the operator-bound runtime.
    ///
    /// [`reconcile_pending_with`]: Self::reconcile_pending_with
    /// [`ReconciliationRuntime`]: crate::services::reconciliation::ReconciliationRuntime
    pub async fn reconcile_pending(&self) -> Result<ReconciliationSummary, BootError> {
        self.reconcile_pending_inner(None).await
    }

    /// Runtime-composed boot reconciliation.  Accepts a
    /// [`ReconciliationRuntime`] resolver that the dispatcher calls
    /// per FN to obtain the `DpsChannel` / `SigningContext` /
    /// `CheckSignBlob` bundle ([`RuntimeView`]) for each FN.
    ///
    /// **Per-FN resolver contract (M3a hardening pass 1).**  Inside
    /// [`reconcile_pending_inner`], for each FN under recovery,
    /// `deps.resolve(&fn_id)` is invoked BEFORE
    /// `run_boot_reconciliation`.  The dispatcher threads
    /// `Option<&RuntimeView>` (not `Option<&ReconciliationRuntime>`)
    /// through the dispatch tree.  `resolve` returning `None` for an
    /// FN with ctx-needy pending docs falls through to the legacy
    /// ctx-free path (emits `BOOT_DISPATCH_DEFERRED`); recovery
    /// NEVER substitutes foreign identity.
    ///
    /// **Dispatch surface.**  Under this entry (M3b W7b updated
    /// 2026-05-16):
    ///   - PREPARED → `dispatch_prepared_via_chain` (snapshot
    ///     envelope + `stage_sign::run` → **W7b post-sign
    ///     dispatcher**: Online → `stage_send::run` (M3a online
    ///     ladder unchanged); Offline | GoingOffline →
    ///     `stage_offline_ack::run` (pipeline terminates at
    ///     `OFFLINE_LOCAL_ACK`); Blocked | StopMode |
    ///     CryptoDegraded | GoingOnline → typed dispatcher refusal
    ///     `WRITE_PATH_DISPATCH_REFUSED`).  Drift between
    ///     `fiscal_documents` and `ingress_inbox` emits
    ///     `BOOT_PREPARED_REPLAY_DRIFT` CRITICAL and holds.
    ///   - SIGNED → **W7b post-sign dispatcher** (same routing as
    ///     PREPARED post-stage_sign; covers crash-recovery of docs
    ///     that crashed between sign and send in a prior tick).
    ///   - SENT → `dispatch_sent_via_probe` (3-way `last_chk`
    ///     classification: Match → KVT1, Mismatch → RM, NotFound →
    ///     ER tick-1 of two-tick retry).
    ///   - ERROR_RETRYABLE → `dispatch_error_retryable_by_class`
    ///     reads durable `retry_class` from `transport_trace` and
    ///     routes: TransientRetry → `stage_send::run`;
    ///     FnConfigError / WrapperBug / OperatorEscalation /
    ///     MacRecovery / TerminalReject → CAS to
    ///     RequiresManualReconciliation; ProbeRequired and None →
    ///     hold without state change.  (TransientRetry retry is
    ///     NOT gated by the W7b dispatcher — it is a resume of an
    ///     in-progress online send, not a post-sign decision.)
    ///
    /// Per ADR-M3-A10: under the global-single-writer invariant, this
    /// call holds the dispatcher task for the duration of one boot
    /// pass; concurrent invocation across the same `App` instance is
    /// not supported.
    ///
    /// [`reconcile_pending`]: Self::reconcile_pending
    /// [`reconcile_pending_inner`]: Self::reconcile_pending_inner
    /// [`ReconciliationRuntime`]: crate::services::reconciliation::ReconciliationRuntime
    /// [`RuntimeView`]: crate::services::reconciliation::RuntimeView
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

        // M3a hardening pass 2 — structural enforcement of ADR-M3-A10
        // (global-single-writer).  Acquire the App-scoped recon mutex
        // BEFORE any state read or per-FN dispatch.  Two concurrent
        // `reconcile_pending_with` calls on the same `App` (or on
        // distinct `Arc<Inner>` clones) serialise here instead of
        // racing each other through the per-row CAS + per-FN
        // `BEGIN IMMEDIATE` envelopes.  Mutex is `tokio::sync::Mutex`
        // because the critical section spans `.await` points across
        // many short per-FN envelopes; held for the duration of one
        // recon call (caller is the dispatcher task, single-task by
        // construction in production).
        //
        // **M3b W2 — module-level enforcement.**  Wrap the
        // `MutexGuard` in a `ReconcileGuard` lock-token immediately
        // after acquisition.  `run_boot_reconciliation` (below) requires
        // a `&ReconcileGuard<'_>` as its first parameter, closing the
        // pre-W2 bypass where direct callers could skip the mutex.
        // The token's lifetime is bound to the `MutexGuard`, so the
        // mutex is released exactly when `_recon_guard` drops at the
        // end of this function.  See `services::reconciliation::guard`.
        let _recon_guard = crate::services::reconciliation::ReconcileGuard::from_app_mutex(
            self.inner.reconcile_mutex.lock().await,
        );

        let pool = self.db();

        // **M3b W12 Post-Closure Hardening Phase 3 / REC-3 (2026-05-24)** —
        // boot orphan-trace scanner: closes any `transport_trace` rows
        // allocated by Envelope 1c-pre but never completed (process
        // crashed mid-DPS-call: SIGKILL / OOM / power loss).  Per-row
        // atomic close + audit emit; 60s TTL guards against false-
        // positive close of legitimate in-flight calls на graceful-
        // shutdown boundary.  Runs BEFORE per-FN reconciliation loop
        // so trace state is clean before any drain pass.
        let orphans_closed = boot_phase::close_orphan_transport_traces(pool, 60)
            .await
            .map_err(|e| BootError::ReconciliationFailed {
                fiscal_number: "<boot-orphan-scanner>".to_string(),
                source: e,
            })?;
        if orphans_closed > 0 {
            tracing::info!(
                orphans_closed,
                "boot orphan-trace scanner closed transport_trace rows"
            );
        }

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
                &_recon_guard,
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

    /// W9b §2.1 (a) — App-owned entry for the offline backlog drain
    /// orchestrator.  Acquires the App reconcile mutex (W2 enforcement
    /// per ADR-M3-A10 + spec §9 OQ-5) and delegates to the pure-
    /// function entry [`backlog_drain::drain`].
    ///
    /// **Mutex scope**: the App reconcile mutex is held for the ENTIRE
    /// drain — per spec §9 OQ-5 operator pin (2026-05-20).  The mutex
    /// is the LOGICAL App-level guard; the inner drain does NOT wrap
    /// the per-doc loop in a single SQLite write transaction (per-doc
    /// DB tx scopes live INSIDE `stage_send::run` /
    /// `apply_w12_confirmation` / `commit_finalize_envelope`).
    /// Concurrent invocation across the same `App` instance serializes
    /// without panic; pilot UX cost (10+s block on large backlog) is
    /// accepted as the correctness tradeoff against double-advance /
    /// mis-finalize races with `boot_phase` reconciliation.
    ///
    /// `deps` is the per-FN [`RuntimeView`] bundle (`dps`,
    /// `signing_ctx`, `fn_sign`).  Boot-recovery callers should
    /// construct a [`ReconciliationRuntime`] resolver and call
    /// `.resolve(fn_id)` to obtain the view BEFORE invoking this
    /// entry; pilot single-operator-single-FN deployments can pass
    /// a static view directly.
    ///
    /// [`backlog_drain::drain`]: crate::services::offline_sync::backlog_drain::drain
    /// [`RuntimeView`]: crate::services::reconciliation::RuntimeView
    /// [`ReconciliationRuntime`]: crate::services::reconciliation::ReconciliationRuntime
    pub async fn drain_offline_backlog_with<'a>(
        &self,
        fiscal_number: &str,
        deps: &crate::services::reconciliation::RuntimeView<'a>,
    ) -> Result<crate::services::offline_sync::backlog_drain::DrainSummary, BootError> {
        // NIT-C7-R1 hardening (2026-05-21): mint the W2 lock-token from
        // the App reconcile mutex.  Token lifetime ties to the
        // MutexGuard — dropping the token releases the mutex.  Drain
        // requires the token in its signature so direct callers cannot
        // bypass the App-level serialization (symmetric to
        // `boot_phase::run_boot_reconciliation` gating).
        let mutex_guard = self.inner.reconcile_mutex.lock().await;
        let recon_guard =
            crate::services::reconciliation::ReconcileGuard::from_app_mutex(mutex_guard);
        crate::services::offline_sync::backlog_drain::drain(
            &recon_guard,
            self.db(),
            deps,
            fiscal_number,
        )
        .await
    }

    pub fn config(&self) -> &AppConfig {
        &self.inner.config
    }

    pub fn db(&self) -> &SqlitePool {
        &self.inner.db
    }

    /// M3b W8b — App-owned return-online probe wiring seam.
    ///
    /// Spawns the periodic return-online probe loop bound to this
    /// App's pool + config.  This is the **App runtime boundary**:
    /// composition root supplies the transport channel + per-FN
    /// signer blobs via [`ReturnOnlineProbeDeps`]; App owns
    /// enumeration, clamp/audit, and lifecycle.
    ///
    /// Composed responsibilities (W8b deliverables):
    /// 1. Read `self.config().offline.clamped_probe_interval_seconds()`.
    /// 2. Emit `RETURN_ONLINE_PROBE_INTERVAL_CLAMPED` WARN audit when
    ///    the operator-supplied value was outside `[5, 3600]`.
    /// 3. Enumerate **ALL** configured FNs via
    ///    `fiscal_number_config::list_all` (NOT only
    ///    `(Offline | GoingOffline)` — the tick-level skip in the W8a
    ///    primitive filters `Online` / `GoingOnline` cheaply BEFORE
    ///    the wire call, and a boot-time mode filter would orphan
    ///    FNs that start `Online` and later transition to `Offline`).
    /// 4. Join each enumerated FN with `deps.fn_signs`.  FNs present
    ///    in `fiscal_number_config` but absent from `fn_signs` emit
    ///    a `RETURN_ONLINE_PROBE_FN_SKIPPED_NO_SIGNER` WARN audit
    ///    and are excluded from `Vec<ProbeSpec>`; the loop spawns
    ///    with the surviving FNs only.  Graceful skip (NOT fail-fast)
    ///    keeps the probe multi-FN-resilient against single-FN
    ///    config drift.
    /// 5. Spawn the W8a primitive `spawn_probe_loop` with
    ///    `Arc::new(self.db().clone())` (sqlx::SqlitePool is itself
    ///    internally Arc-shared; double-wrap is harmless — primitive
    ///    signature cleanup is out of W8b scope).
    /// 6. Return the `JoinHandle`.  Caller owns the
    ///    `watch::Sender<bool>` (built outside) and the
    ///    `JoinHandle`; App does NOT track either.
    ///
    /// **Production wiring intentionally deferred.**  M1 `main.rs`
    /// remains idle.  The first production caller of this method is
    /// a future runtime-composition task where the concrete DPS
    /// channel (and any future channel router) is constructed.
    /// This separation lets W8b ship the App-side seam without
    /// committing to a specific DPS transport implementation; the
    /// project's planned two-channel DPS architecture (direct DPS +
    /// WebCheck-compatible channel) is unaffected by this method's
    /// shape.
    ///
    /// Errors: DB-level only (`fiscal_number_config::list_all` failure
    /// or audit-append failure).  Wrapped in `anyhow::Error` because
    /// the failure modes are heterogeneous and not part of
    /// [`BootError`].
    ///
    /// Empty FN list: spawns the loop with an empty
    /// `Vec<ProbeSpec>`.  The loop ticks but does no per-FN work;
    /// cost is one `tokio::time::interval`.  Avoids changing the
    /// return type to `Option<JoinHandle>` for the cold-boot case.
    pub async fn spawn_return_online_probe(
        &self,
        deps: crate::services::offline_sync::return_online_probe::ReturnOnlineProbeDeps,
        shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> anyhow::Result<tokio::task::JoinHandle<()>> {
        use crate::config::{PROBE_INTERVAL_MAX_SECONDS, PROBE_INTERVAL_MIN_SECONDS};
        use crate::db::models::enums::Severity;
        use crate::db::repositories::{audit_log, fiscal_number_config};
        use crate::services::offline_sync::return_online_probe::{self, ProbeSpec};

        let pool = self.db();

        // (1) Clamp interval + WARN audit if operator-supplied value
        //     was outside the safe bounds.  Audit payload carries both
        //     raw and clamped values so operators can diagnose the
        //     misconfiguration source.  entity_type = "app",
        //     entity_id = "" — app-wide config event, not FN-scoped.
        let (clamped_seconds, was_clamped) =
            self.inner.config.offline.clamped_probe_interval_seconds();
        if was_clamped {
            let raw = self
                .inner
                .config
                .offline
                .return_online_probe_interval_seconds;
            let payload = serde_json::json!({
                "raw_seconds": raw,
                "clamped_seconds": clamped_seconds,
                "min_seconds": PROBE_INTERVAL_MIN_SECONDS,
                "max_seconds": PROBE_INTERVAL_MAX_SECONDS,
            });
            audit_log::append(
                pool,
                "app",
                "",
                "RETURN_ONLINE_PROBE_INTERVAL_CLAMPED",
                Severity::Warning,
                None,
                Some(&payload.to_string()),
            )
            .await?;
        }

        // (2) Enumerate ALL configured FNs.  Boot-time mode filter
        //     would orphan late `Online -> Offline` transitions; the
        //     tick-level skip filters `Online` / `GoingOnline`
        //     cheaply.  See module-level doc on the W8a primitive.
        let fns = fiscal_number_config::list_all(pool).await?;

        // (3) Join with `deps.fn_signs`; emit WARN audit + skip per
        //     FN missing a signer blob.  Anchor the audit on
        //     `fiscal_number_config` (config-layer drift, not runtime
        //     mode drift — see W8b operator decision).
        let mut specs: Vec<ProbeSpec> = Vec::with_capacity(fns.len());
        for fn_cfg in &fns {
            match deps.fn_signs.get(&fn_cfg.fiscal_number) {
                Some(fn_sign) => {
                    specs.push(ProbeSpec {
                        fiscal_number: fn_cfg.fiscal_number.clone(),
                        fn_sign: fn_sign.clone(),
                    });
                }
                None => {
                    let payload = serde_json::json!({
                        "fiscal_number": &fn_cfg.fiscal_number,
                        "reason": "missing_fn_sign",
                        "configured_fn": true,
                    });
                    audit_log::append(
                        pool,
                        "fiscal_number_config",
                        &fn_cfg.fiscal_number,
                        "RETURN_ONLINE_PROBE_FN_SKIPPED_NO_SIGNER",
                        Severity::Warning,
                        None,
                        Some(&payload.to_string()),
                    )
                    .await?;
                }
            }
        }

        // (4) Spawn the W8a primitive loop.  `Arc::new(self.db().clone())`
        //     — sqlx::SqlitePool is itself internally Arc-shared; the
        //     double-wrap is harmless.  Primitive signature cleanup
        //     (SqlitePool by value instead of Arc<SqlitePool>) is out
        //     of W8b scope; tracked as future cleanup.
        let interval = std::time::Duration::from_secs(clamped_seconds);
        let handle = return_online_probe::spawn_probe_loop(
            std::sync::Arc::new(pool.clone()),
            deps.dps,
            specs,
            interval,
            shutdown_rx,
        );
        Ok(handle)
    }
}
