//! `prro admin` — administrative operations (operator-only intervention).
//!
//! **M3b W12 Post-Closure Hardening Phase 2a.2 — REC-1 Tier 3 (2026-05-24)**:
//! exposes manual `STOP_MODE` reset path для коли operator inspected an FN
//! that auto-escalated до `STOP_MODE` (50+ consecutive Hold ticks per Tier 2
//! contract в `services/offline_sync/backlog_drain.rs::trigger_tier_2_
//! stop_mode`) і визначив root cause + resolved upstream (наприклад,
//! restored DPS connectivity, rotated expired credentials, deployed contract
//! fix для DPS protocol drift, etc.).
//!
//! ## Operator runbook
//!
//! ```bash
//! prro admin reset-stop-mode \
//!     --config /etc/prro/config.toml \
//!     --fiscal-number 1234567890 \
//!     --reason "DPS network outage resolved 2026-05-24T10:30; verified ping OK"
//! ```
//!
//! Atomic side-effects (one `with_immediate` envelope):
//!
//! 1. CAS `node_state.mode`: `STOP_MODE → GoingOnline` (fail-loud if current
//!    mode != STOP_MODE — operator wrong-command guard).
//! 2. UPDATE `fiscal_documents` SET `consecutive_holds = 0` WHERE
//!    `fiscal_number = ? AND consecutive_holds > 0` (reset all held docs
//!    on FN; next-tick drain re-evaluates fresh).
//! 3. Emit `ADMIN_STOP_MODE_RESET` Severity::Critical audit row з
//!    structured payload: `{fiscal_number, reason, mode_before,
//!    mode_after, docs_reset_count}`.
//!
//! Operator memory `feedback_manual_recon_catastrophe` rationale: Tier 3 is
//! the **operator-decided** escape hatch from Tier 2; auto-escalation to
//! `REQUIRES_MANUAL_RECONCILIATION` is intentionally avoided per pinned
//! "Manual recon = ЧП из ЧП" constraint.  Operator chooses when DPS is
//! healthy enough to resume; reset transitions back to `GoingOnline` so W8
//! return_online_probe re-validates connectivity BEFORE full `Online`
//! promotion.

use crate::db::tx::with_immediate;
use sqlx::SqlitePool;
use std::path::Path;
use thiserror::Error;

/// Typed errors for admin operations.  Each variant maps to a clear
/// operator-visible exit code (per `exit_code()`).
#[derive(Debug, Error)]
pub enum AdminError {
    /// Config / DB infrastructure failure (read path / migrations / lock).
    #[error("admin: infrastructure: {0}")]
    Infrastructure(String),

    /// FN row not found in `node_state` — operator typo or wrong DB.
    #[error("admin: fiscal_number {0:?} has no node_state row — verify config / FN spelling")]
    FiscalNumberNotFound(String),

    /// CAS guard failed: current `node_state.mode` is NOT `STOP_MODE`.
    /// Operator using wrong command (FN is in different state).  Refuses
    /// to mutate to avoid masking legitimate state.
    #[error(
        "admin: fiscal_number {fiscal_number:?} current mode is {observed_mode:?}, expected STOP_MODE — operator command misuse (use --target-mode flag or check intended FN)"
    )]
    NotInStopMode {
        fiscal_number: String,
        observed_mode: String,
    },

    /// Empty / whitespace-only `--reason` arg.  Forensic accountability
    /// requires explicit human-readable reason для audit_log trail.
    #[error(
        "admin: --reason MUST be a non-empty, non-whitespace description for forensic audit trail"
    )]
    EmptyReason,
}

impl AdminError {
    /// BSD sysexits.h-aligned process exit code.
    pub fn exit_code(&self) -> i32 {
        match self {
            // EX_NOINPUT (66): config / DB issue.
            AdminError::Infrastructure(_) => 66,
            // EX_USAGE (64): operator command misuse.
            AdminError::FiscalNumberNotFound(_)
            | AdminError::NotInStopMode { .. }
            | AdminError::EmptyReason => 64,
        }
    }
}

/// Outcome of a successful `reset_stop_mode` invocation — для CLI
/// stdout reporting + structured logging.
#[derive(Debug)]
pub struct ResetOutcome {
    pub fiscal_number: String,
    /// Number of `fiscal_documents` rows that had `consecutive_holds > 0`
    /// reset to 0.  Reflects scope of admin intervention.
    pub docs_reset_count: i64,
}

/// **Phase 2a.2 / REC-1 Tier 3 (2026-05-24)** — manual STOP_MODE reset
/// для конкретного fiscal_number.  Atomic per I4 invariant: node_state
/// CAS + counter resets + audit emission bundled in one `with_immediate`.
///
/// **Caller contract**:
/// - `pool`: opened via `crate::db::open_pool` (migrations already
///   applied per `App::boot` или CLI boot path).
/// - `fiscal_number`: must exist в `node_state`.
/// - `reason`: non-empty/non-whitespace operator-supplied description.
///
/// Returns `Ok(ResetOutcome)` on success.  All failures structurally
/// typed via [`AdminError`] для clean CLI exit code mapping.
///
/// Atomic side-effects per envelope:
/// 1. CAS `node_state.mode`: `STOP_MODE → GoingOnline` (W8 return_online_
///    probe will verify connectivity before full `Online` promotion).
/// 2. UPDATE `fiscal_documents.consecutive_holds = 0` for всі rows на
///    FN з counter > 0 (next-tick drain re-evaluates fresh).
/// 3. INSERT `audit_log` `ADMIN_STOP_MODE_RESET` Critical з payload.
pub async fn reset_stop_mode(
    pool: &SqlitePool,
    fiscal_number: &str,
    reason: &str,
) -> Result<ResetOutcome, AdminError> {
    // (1) Validate reason — empty/whitespace gets rejected up-front
    // (avoids polluting audit_log з placeholder reasons).
    if reason.trim().is_empty() {
        return Err(AdminError::EmptyReason);
    }
    // (2) Pre-read current mode to provide actionable error message
    // when CAS fails.  Atomicity not impacted: even if mode changes
    // between read and tx (concurrent W8 probe), the tx-side CAS
    // `WHERE mode = 'STOP_MODE'` still guards correctness — pre-read
    // is purely for error-message diagnostic.
    let observed: Option<String> =
        sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = ?")
            .bind(fiscal_number)
            .fetch_optional(pool)
            .await
            .map_err(|e| AdminError::Infrastructure(format!("read node_state.mode: {e}")))?;
    let observed_mode = match observed {
        None => return Err(AdminError::FiscalNumberNotFound(fiscal_number.to_string())),
        Some(m) => m,
    };
    if observed_mode != "STOP_MODE" {
        return Err(AdminError::NotInStopMode {
            fiscal_number: fiscal_number.to_string(),
            observed_mode,
        });
    }

    // (3) Atomic envelope: mode CAS + counter reset + audit row.
    let fn_owned = fiscal_number.to_string();
    let reason_owned = reason.to_string();
    let docs_reset_count: i64 = with_immediate(pool, move |tx| {
        Box::pin(async move {
            // (a) CAS node_state.mode: STOP_MODE → GOING_ONLINE.
            // Guard z WHERE mode = 'STOP_MODE' — defensive race-safety
            // even though pre-read confirmed (concurrent W8 could've
            // already moved it).
            let mode_rows = sqlx::query(
                "UPDATE node_state SET mode = 'GOING_ONLINE' \
                 WHERE fiscal_number = ? AND mode = 'STOP_MODE'",
            )
            .bind(&fn_owned)
            .execute(&mut **tx)
            .await?
            .rows_affected();
            if mode_rows != 1 {
                // Race detected: mode changed между pre-read and tx.
                // Fail-loud — operator should re-run command + verify.
                return Err(anyhow::anyhow!(
                    "admin: race detected during reset_stop_mode for fn={fn_owned} — \
                     mode CAS produced rows_affected={mode_rows} (concurrent state \
                     change between pre-read and tx; re-run command)"
                ));
            }
            // (b) Reset consecutive_holds для всіх held docs на FN.
            let reset_count: i64 = sqlx::query(
                "UPDATE fiscal_documents SET consecutive_holds = 0 \
                 WHERE fiscal_number = ? AND consecutive_holds > 0",
            )
            .bind(&fn_owned)
            .execute(&mut **tx)
            .await?
            .rows_affected() as i64;
            // (c) Critical audit row z forensic payload.
            let payload = serde_json::json!({
                "fiscal_number": fn_owned,
                "reason": reason_owned,
                "mode_before": "STOP_MODE",
                "mode_after": "GOING_ONLINE",
                "docs_reset_count": reset_count,
                "tier": 3,
            });
            crate::db::repositories::audit_log::append_tx(
                tx,
                "fn",
                &fn_owned,
                "ADMIN_STOP_MODE_RESET",
                crate::db::models::enums::Severity::Critical,
                None,
                Some(&payload.to_string()),
            )
            .await?;
            Ok::<i64, anyhow::Error>(reset_count)
        })
    })
    .await
    .map_err(|e| AdminError::Infrastructure(format!("reset_stop_mode envelope: {e}")))?;

    Ok(ResetOutcome {
        fiscal_number: fiscal_number.to_string(),
        docs_reset_count,
    })
}

/// CLI entry-point for `prro admin reset-stop-mode`.  Reads config,
/// opens pool (runs migrations), executes `reset_stop_mode`, prints
/// outcome to stdout.  Returns BSD sysexits-aligned exit code.
pub async fn run_reset_stop_mode(
    config_path: &Path,
    fiscal_number: &str,
    reason: &str,
) -> Result<ResetOutcome, AdminError> {
    let cfg_text = std::fs::read_to_string(config_path)
        .map_err(|e| AdminError::Infrastructure(format!("read config: {e}")))?;
    let cfg = crate::config::AppConfig::from_toml(&cfg_text)
        .map_err(|e| AdminError::Infrastructure(format!("parse config: {e}")))?;
    // Acquire singleton lock before touching DB (consistent з doctor
    // pattern: refuses to race з `prro serve`).
    let _lock = crate::runtime::singleton::acquire(&cfg.database.db_path)
        .map_err(|e| AdminError::Infrastructure(format!("singleton lock: {e}")))?;
    let pool = crate::db::open_pool(&cfg.database.db_path)
        .await
        .map_err(|e| AdminError::Infrastructure(format!("open db pool: {e}")))?;
    let outcome = reset_stop_mode(&pool, fiscal_number, reason).await?;
    drop(pool);
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper для test harness — minimal pool з migrations applied.
    async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().expect("tempdir");
        let pool = crate::db::open_pool(&dir.path().join("admin_test.db"))
            .await
            .expect("open_pool runs migrations");
        sqlx::query(
            "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
             VALUES ('1234567890', '12345678', 'test')",
        )
        .execute(&pool)
        .await
        .unwrap();
        (dir, pool)
    }

    async fn seed_node_state(pool: &SqlitePool, mode: &str) {
        sqlx::query(
            "INSERT INTO node_state(fiscal_number, mode, shift_state, next_lnd) \
             VALUES ('1234567890', ?, 'OPENED', 100)",
        )
        .bind(mode)
        .execute(pool)
        .await
        .unwrap();
    }

    /// **Phase 2a.2 unit test 1**: успішний reset переводить mode +
    /// emit audit + повертає docs_reset_count=0 коли немає held docs.
    #[tokio::test]
    async fn reset_stop_mode_happy_path_transitions_to_going_online() {
        let (_d, pool) = fresh_pool().await;
        seed_node_state(&pool, "STOP_MODE").await;

        let outcome = reset_stop_mode(&pool, "1234567890", "operator restored DPS")
            .await
            .unwrap();
        assert_eq!(outcome.fiscal_number, "1234567890");
        assert_eq!(outcome.docs_reset_count, 0);

        // Mode transitioned.
        let mode: String =
            sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = ?")
                .bind("1234567890")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(mode, "GOING_ONLINE");

        // Audit row emitted з Critical severity.
        let audit: (String, String) = sqlx::query_as(
            "SELECT severity, event_payload_json FROM audit_log \
             WHERE event_type = 'ADMIN_STOP_MODE_RESET' \
             ORDER BY audit_id DESC LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(audit.0, "CRITICAL");
        let payload: serde_json::Value = serde_json::from_str(&audit.1).unwrap();
        assert_eq!(payload["fiscal_number"], "1234567890");
        assert_eq!(payload["reason"], "operator restored DPS");
        assert_eq!(payload["mode_before"], "STOP_MODE");
        assert_eq!(payload["mode_after"], "GOING_ONLINE");
        assert_eq!(payload["tier"], 3);
    }

    /// **Phase 2a.2 unit test 2**: refuses to mutate коли current mode
    /// != STOP_MODE (NotInStopMode error з actionable diagnostic).
    #[tokio::test]
    async fn reset_stop_mode_refuses_when_not_in_stop_mode() {
        let (_d, pool) = fresh_pool().await;
        seed_node_state(&pool, "ONLINE").await;

        let err = reset_stop_mode(&pool, "1234567890", "valid reason")
            .await
            .expect_err("non-STOP_MODE MUST be rejected");
        match err {
            AdminError::NotInStopMode {
                fiscal_number,
                observed_mode,
            } => {
                assert_eq!(fiscal_number, "1234567890");
                assert_eq!(observed_mode, "ONLINE");
            }
            other => panic!("expected NotInStopMode, got: {other:?}"),
        }

        // Mode unchanged.
        let mode: String =
            sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = ?")
                .bind("1234567890")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(mode, "ONLINE");

        // No audit row emitted (refused before tx).
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log WHERE event_type = 'ADMIN_STOP_MODE_RESET'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 0);
    }

    /// **Phase 2a.2 unit test 3**: empty reason rejected up-front
    /// (EmptyReason error; no DB touch).
    #[tokio::test]
    async fn reset_stop_mode_refuses_empty_reason() {
        let (_d, pool) = fresh_pool().await;
        seed_node_state(&pool, "STOP_MODE").await;

        let err = reset_stop_mode(&pool, "1234567890", "   ")
            .await
            .expect_err("empty/whitespace reason MUST be rejected");
        assert!(matches!(err, AdminError::EmptyReason));

        // Mode unchanged (refused before any DB write).
        let mode: String =
            sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = ?")
                .bind("1234567890")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(mode, "STOP_MODE");
    }

    /// **Phase 2a.2 unit test 4**: missing FN row → FiscalNumberNotFound.
    #[tokio::test]
    async fn reset_stop_mode_refuses_missing_fiscal_number() {
        let (_d, pool) = fresh_pool().await;
        // No seed_node_state — FN absent.

        let err = reset_stop_mode(&pool, "9999999999", "valid reason")
            .await
            .expect_err("missing FN MUST be rejected");
        match err {
            AdminError::FiscalNumberNotFound(fn_id) => {
                assert_eq!(fn_id, "9999999999");
            }
            other => panic!("expected FiscalNumberNotFound, got: {other:?}"),
        }
    }
}
