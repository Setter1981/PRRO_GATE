//! W4-Z0 piece 7 — per-FN config bootstrap on `add-operator`.
//!
//! Per `docs/superpowers/specs/2026-05-26-w4-z0-config-storage-spec.md`
//! §2.  When `prro admin add-operator` registers a cashier on a
//! brand-new FN, seed the per-FN config tables with WebCheck-style
//! defaults so the FN is immediately usable for fiscal operations
//! without additional admin commands.
//!
//! ## Idempotency
//!
//! `bootstrap_fn_defaults` is safe to call repeatedly.  All inserts
//! use SQLite's `INSERT OR IGNORE` form, which silently skips any row
//! whose primary key already exists.  This means:
//!
//!   * Re-running `add-operator` for an FN that already has a cashier
//!     (rotation, additional cashiers) is a no-op for the config
//!     tables — operator's prior customisations are preserved.
//!   * If operator pre-customised a tax-group rate via
//!     `prro admin update-tax-rate` BEFORE registering the first
//!     cashier, that customisation survives the bootstrap call.
//!   * Concurrent races between two `add-operator` invocations for
//!     the same FN (operationally bizarre — admin CLI is operator-
//!     serialised) cannot duplicate rows.
//!
//! ## Why bootstrap on add-operator and not on first-receipt
//!
//! Admin CLI is the operator-controlled boundary where it is safe
//! to do meaningful work (no live fiscal pipeline running).  Doing
//! the bootstrap on first-receipt would mean the receipt path
//! depends on a write transaction to secure.db — violating
//! Invariant #1 (no long write tx in the hot path) and complicating
//! the per-FN supervisor.  Admin-CLI-time bootstrap is the right
//! seam.
//!
//! ## What this does NOT touch
//!
//! `driver_tax_mapping` is per-driver (not per-FN) — bootstrapped
//! at driver registration time, not here.  That seam lives in the
//! listener-startup path (W4 supervisor PR, future worklet).

use sqlx::SqlitePool;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BootstrapError {
    #[error("database error during bootstrap: {0}")]
    Db(#[from] sqlx::Error),

    /// Audit Round-3 (2026-05-27): per-row partial failure within
    /// the bootstrap loop.  Round-2 fix made each row's failure
    /// non-fatal (tracing::warn! + continue), but the original
    /// implementation also returned Ok unconditionally — which
    /// SWALLOWED partial failures from `add_operator`'s audit
    /// emission.  Now we aggregate row failures here so the caller
    /// (`add_operator`) sees an `Err` and emits the Critical
    /// `ADMIN_FN_DEFAULTS_BOOTSTRAP_FAILED` audit row while
    /// continuing with operator registration.
    #[error("bootstrap partial failure: {count} row(s) failed; first = {first_summary}")]
    PartialFailure {
        count: usize,
        first_summary: String,
        failures: Vec<RowFailure>,
    },
}

/// Per-row failure context for `BootstrapError::PartialFailure`.
/// Carried up to `add_operator` so the audit_log payload can name
/// the exact (table, identifier, cause) triple for forensic recovery.
#[derive(Debug, Clone)]
pub struct RowFailure {
    pub table: &'static str, // "tax_groups" / "payment_methods" / "fn_outgress_profile" / "fn_integration_flags"
    pub identifier: String, // human-readable row identifier (tx_num=1 letter=А, pay_index=2 name=Visa, etc.)
    pub cause: String,      // sqlx error message
}

impl RowFailure {
    fn summary(&self) -> String {
        format!("{}({}): {}", self.table, self.identifier, self.cause)
    }
}

/// Seed per-FN config defaults if not already present.  Idempotent
/// (`INSERT OR IGNORE` for all rows).  Called from `add_operator`
/// admin CLI after the operator row INSERT succeeds.
///
/// Inserts (each is OR IGNORE → no-op if PK exists):
///   * 11 `tax_groups` rows (А=20%, Б=0%, В=7%, ГА=20+5,
///     ГБ=0+5, ДА=20+7.5, ДБ=0+7.5, Е=0, Ж=0, З=0, К=14%)
///   * 4 `payment_methods` rows (1=Готівка cash, 2=Картка,
///     3=Кредит, 4=Сертифікат)
///   * 1 `fn_outgress_profile` row (FSCO_ZZD)
///   * 1 `fn_integration_flags` row (useecheckmegovua = '0')
pub async fn bootstrap_fn_defaults(
    pool_secure: &SqlitePool,
    fn_id: &str,
) -> Result<(), BootstrapError> {
    // Audit Round-3 (2026-05-27): each seed_* function returns its
    // per-row failures (typed `Vec<RowFailure>`).  We aggregate them
    // across the 4 seed functions and surface as a single
    // `BootstrapError::PartialFailure` so the caller's audit
    // emission fires for ANY partial failure (was previously
    // swallowed by per-row try/log/continue inside each seed_*).
    let mut failures: Vec<RowFailure> = Vec::new();
    failures.extend(seed_tax_groups(pool_secure, fn_id).await?);
    failures.extend(seed_payment_methods(pool_secure, fn_id).await?);
    failures.extend(seed_outgress_profile(pool_secure, fn_id).await?);
    failures.extend(seed_integration_flags(pool_secure, fn_id).await?);

    if !failures.is_empty() {
        let first_summary = failures[0].summary();
        return Err(BootstrapError::PartialFailure {
            count: failures.len(),
            first_summary,
            failures,
        });
    }
    Ok(())
}

/// WebCheck-derived defaults per `Directorys.cs:346` in the
/// reverse-engineered .NET source.  Each tuple is
/// `(tx_num, letter, dtpr, txpr, txal)`.  `txty` always 0.
const DEFAULT_TAX_GROUPS: &[(i64, &str, f64, f64, i64)] = &[
    (1, "А", 0.0, 20.0, 0),  // ПДВ 20% standard
    (2, "Б", 0.0, 0.0, 0),   // Звільнено від ПДВ
    (3, "В", 0.0, 7.0, 0),   // ПДВ 7% (медичні)
    (4, "ГА", 5.0, 20.0, 2), // ПДВ 20% + акциз 5%
    (5, "ГБ", 5.0, 0.0, 2),  // Звільнено + акциз 5%
    (6, "ДА", 7.5, 20.0, 2), // ПДВ 20% + акциз 7.5%
    (7, "ДБ", 7.5, 0.0, 2),  // Звільнено + акциз 7.5%
    (8, "Е", 0.0, 0.0, 0),   // (reserved)
    (9, "Ж", 0.0, 0.0, 0),   // (reserved)
    (10, "З", 0.0, 0.0, 0),  // (reserved)
    (11, "К", 0.0, 14.0, 0), // ПДВ 14% (special category)
];

async fn seed_tax_groups(
    pool: &SqlitePool,
    fn_id: &str,
) -> Result<Vec<RowFailure>, BootstrapError> {
    // Audit Round-1..3 (2026-05-27): ON CONFLICT DO UPDATE
    // reactivates soft-deleted defaults; per-row failures are
    // collected (not aborted, not swallowed); aggregated at
    // `bootstrap_fn_defaults` for the caller's audit emission.
    let mut failures = Vec::new();
    for (tx_num, letter, dtpr, txpr, txal) in DEFAULT_TAX_GROUPS {
        let result = sqlx::query(
            "INSERT INTO tax_groups \
                (fn, tx_num, letter, dtpr, txpr, txal, txty) \
             VALUES (?, ?, ?, ?, ?, ?, 0) \
             ON CONFLICT(fn, tx_num) DO UPDATE \
               SET is_active = 1 \
               WHERE tax_groups.is_active = 0",
        )
        .bind(fn_id)
        .bind(*tx_num)
        .bind(*letter)
        .bind(*dtpr)
        .bind(*txpr)
        .bind(*txal)
        .execute(pool)
        .await;

        if let Err(e) = result {
            tracing::warn!(
                target: "prro::runtime::bootstrap",
                fn_id = %fn_id,
                tx_num = *tx_num,
                letter = *letter,
                cause = %e,
                "tax_group seed row failed (likely letter-collision via prior operator \
                 customisation) — continuing with remaining defaults"
            );
            failures.push(RowFailure {
                table: "tax_groups",
                identifier: format!("tx_num={} letter={}", tx_num, letter),
                cause: e.to_string(),
            });
        }
    }
    Ok(failures)
}

/// WebCheck-derived payment defaults per `CreateDB.cs:459`.
/// Tuple: `(pay_index, name, iscash)`.  XML `<M T=...>` value is
/// `pay_index - 1` per WebCheck convention (cash → T=0, etc.).
const DEFAULT_PAYMENT_METHODS: &[(i64, &str, bool)] = &[
    (1, "Готівка", true),
    (2, "Картка", false),
    (3, "Кредит", false),
    (4, "Сертифікат", false),
];

async fn seed_payment_methods(
    pool: &SqlitePool,
    fn_id: &str,
) -> Result<Vec<RowFailure>, BootstrapError> {
    let mut failures = Vec::new();
    for (pay_index, name, iscash) in DEFAULT_PAYMENT_METHODS {
        let result = sqlx::query(
            "INSERT INTO payment_methods \
                (fn, pay_index, name, iscash) \
             VALUES (?, ?, ?, ?) \
             ON CONFLICT(fn, pay_index) DO UPDATE \
               SET is_active = 1 \
               WHERE payment_methods.is_active = 0",
        )
        .bind(fn_id)
        .bind(*pay_index)
        .bind(*name)
        .bind(if *iscash { 1_i64 } else { 0 })
        .execute(pool)
        .await;

        if let Err(e) = result {
            tracing::warn!(
                target: "prro::runtime::bootstrap",
                fn_id = %fn_id,
                pay_index = *pay_index,
                name = *name,
                cause = %e,
                "payment_method seed row failed (likely name-collision via prior \
                 operator customisation) — continuing with remaining defaults"
            );
            failures.push(RowFailure {
                table: "payment_methods",
                identifier: format!("pay_index={} name={}", pay_index, name),
                cause: e.to_string(),
            });
        }
    }
    Ok(failures)
}

async fn seed_outgress_profile(
    pool: &SqlitePool,
    fn_id: &str,
) -> Result<Vec<RowFailure>, BootstrapError> {
    // INSERT OR IGNORE — preserves any operator-set EVPZ_DPS choice
    // made before the first cashier was registered.
    let result = sqlx::query(
        "INSERT OR IGNORE INTO fn_outgress_profile (fn, profile) VALUES (?, 'FSCO_ZZD')",
    )
    .bind(fn_id)
    .execute(pool)
    .await;
    let mut failures = Vec::new();
    if let Err(e) = result {
        tracing::warn!(
            target: "prro::runtime::bootstrap",
            fn_id = %fn_id,
            cause = %e,
            "fn_outgress_profile seed failed — continuing with remaining defaults"
        );
        failures.push(RowFailure {
            table: "fn_outgress_profile",
            identifier: "profile=FSCO_ZZD".to_string(),
            cause: e.to_string(),
        });
    }
    Ok(failures)
}

async fn seed_integration_flags(
    pool: &SqlitePool,
    fn_id: &str,
) -> Result<Vec<RowFailure>, BootstrapError> {
    // Per WebCheck UI: Національний чек integration off by default.
    let result = sqlx::query(
        "INSERT OR IGNORE INTO fn_integration_flags (fn, flag_name, flag_value) \
         VALUES (?, 'useecheckmegovua', '0')",
    )
    .bind(fn_id)
    .execute(pool)
    .await;
    let mut failures = Vec::new();
    if let Err(e) = result {
        tracing::warn!(
            target: "prro::runtime::bootstrap",
            fn_id = %fn_id,
            cause = %e,
            "fn_integration_flags useecheckmegovua seed failed — continuing"
        );
        failures.push(RowFailure {
            table: "fn_integration_flags",
            identifier: "flag_name=useecheckmegovua".to_string(),
            cause: e.to_string(),
        });
    }
    Ok(failures)
}
