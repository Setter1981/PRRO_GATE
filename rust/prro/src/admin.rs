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
use zeroize::Zeroizing;

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
        "admin: fiscal_number {fiscal_number:?} current mode is {observed_mode:?}, expected STOP_MODE — operator command misuse (FN is not in STOP_MODE; check intended FN or wait until Tier-2 escalation triggers)"
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

    /// W2 add-operator: `--fn` arg references a `fiscal_number` that
    /// does not exist in the main DB's `fiscal_number_config` table.
    /// This is the CLI side of the cross-DB FK compensating check —
    /// pre-INSERT rejection prevents orphan rows from ever landing in
    /// `operators`, sparing the boot-time `OPERATOR_ORPHAN_FN` audit
    /// path the work.
    #[error(
        "admin: fiscal_number {0:?} not registered in fiscal_number_config — \
         add the FN to the main DB first, or correct the --fn argument"
    )]
    FiscalNumberNotInConfig(String),

    /// W2 add-operator: an active cashier already exists for this FN
    /// (caught by the partial unique index `operators_active_fn_uidx
    /// WHERE is_active = 1`).  Operator must rotate the previous
    /// cashier out before registering a new one — silent overwrite
    /// would lose forensic continuity.
    #[error(
        "admin: fiscal_number {0:?} already has an active cashier registered — \
         rotate the previous cashier (mark is_active=0) before adding a new one"
    )]
    DuplicateActiveCashier(String),

    /// W2 add-operator: password input was empty (or all-whitespace).
    /// Storing an empty `key_pass_enc` BLOB is meaningless and would
    /// fail key load at boot.
    #[error("admin: password MUST be a non-empty string")]
    EmptyPassword,

    /// W2 add-operator: empty/whitespace `--inn` or `--name` or
    /// `--key-path` argument.  Forensic continuity requires a
    /// non-empty cashier identifier and key location.
    #[error("admin: --{0} MUST be a non-empty value")]
    EmptyArgument(&'static str),

    /// W2 add-operator (LOW-PR90-01): TTY mode requires the operator
    /// to type the same password twice; the two inputs differed.
    /// Refuse rather than silently accept the first guess.
    #[error("admin: password confirmation did not match the first entry — try again")]
    PasswordMismatch,

    /// W2 add-operator (LOW-PR90-01): the password prompt's underlying
    /// reader (TTY or stdin) returned an IO error before EOF.
    #[error("admin: password input IO error: {0}")]
    PasswordReadIo(String),

    /// A′.3 PR-O1 — the offline operator surface (GO_OFFLINE / GO_ONLINE)
    /// is gated behind `FULL_OFFLINE_SURFACE_READY` until the drain path
    /// lands (O2).  The door is deliberately shut in O1.
    #[error(
        "admin: offline operator surface is not enabled yet — GO_OFFLINE/GO_ONLINE is gated until the drain path lands (A′.3 O2)"
    )]
    OfflineSurfaceNotReady,

    /// A′.3 PR-O1 — GO_OFFLINE / GO_ONLINE mode-guard failed: the FN is not
    /// in the mode the command requires.  Refuses to mutate to avoid masking
    /// legitimate state (mirrors `NotInStopMode`).
    #[error(
        "admin: fiscal_number {fiscal_number:?} current mode is {observed_mode:?}, expected {expected} — operator command misuse"
    )]
    NotInExpectedMode {
        fiscal_number: String,
        observed_mode: String,
        expected: &'static str,
    },

    /// A′.3 PR-O1 — `seed-codes` range is empty or non-positive.
    #[error("admin: invalid offline-code range [{first}..={last}] — require 1 <= first <= last")]
    InvalidCodeRange { first: i64, last: i64 },

    /// A′.3 PR-O1 — `seed-codes` range overlaps codes already in the pool.
    /// Loud reject (the underlying primitive is INSERT OR IGNORE and would
    /// silently dedupe) so an operator re-seed with a stale range is caught.
    #[error(
        "admin: offline-code range [{first}..={last}] overlaps {overlap_count} code(s) already in the pool for fiscal_number {fiscal_number:?} — codes must be seeded exactly once (only real DPS-issued ranges)"
    )]
    CodeRangeOverlapsExistingPool {
        fiscal_number: String,
        first: i64,
        last: i64,
        overlap_count: i64,
    },

    /// T=112 C5 — `request-offline-codes` replenish failed.  Wraps the
    /// typed `ReplenishError` display so the operator sees a clear message.
    /// Exit code 75 (EX_TEMPFAIL) — transient network/DPS failure; operator
    /// can retry.  Server rejects also land here (DPS returned a non-0 code).
    #[error("admin: request-offline-codes replenish failed: {0}")]
    ReplenishFailed(String),
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
            | AdminError::EmptyReason
            | AdminError::FiscalNumberNotInConfig(_)
            | AdminError::DuplicateActiveCashier(_)
            | AdminError::EmptyPassword
            | AdminError::EmptyArgument(_)
            | AdminError::PasswordMismatch
            | AdminError::NotInExpectedMode { .. }
            | AdminError::InvalidCodeRange { .. }
            | AdminError::CodeRangeOverlapsExistingPool { .. } => 64,
            // EX_UNAVAILABLE (69): the gated offline surface is not enabled yet.
            AdminError::OfflineSurfaceNotReady => 69,
            // EX_IOERR (74): input device failure.
            AdminError::PasswordReadIo(_) => 74,
            // EX_TEMPFAIL (75): transient failure (network/DPS); operator can retry.
            AdminError::ReplenishFailed(_) => 75,
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

/// A′.3 PR-O1 — outcome of a successful `go_offline` (the mode flip +
/// atomic offline-session open).
#[derive(Debug)]
pub struct GoOfflineOutcome {
    pub fiscal_number: String,
    /// Hex-lower id of the OPEN offline session created in the same envelope.
    pub offline_session_id: String,
}

/// A′.3 PR-O1 — outcome of a successful `go_online` (mode → GOING_ONLINE).
#[derive(Debug)]
pub struct GoOnlineOutcome {
    pub fiscal_number: String,
}

/// A′.3 PR-O1 — outcome of a successful `seed-codes` provisioning.
#[derive(Debug)]
pub struct SeedCodesOutcome {
    pub fiscal_number: String,
    pub first_lnd: i64,
    pub last_lnd: i64,
    /// Codes actually inserted (== range size, since the overlap pre-check
    /// guarantees no pre-existing rows in range).
    pub inserted_count: u64,
}

/// T=112 C5 — outcome of a successful `request-offline-codes` replenish.
#[derive(Debug)]
pub struct ReplenishOutcome {
    pub fiscal_number: String,
    pub tax_number: String,
    pub codes_received: usize,
    pub inserted: u64,
    pub deduped: u64,
    pub new_seed_hex: String,
    pub request_xml: String,
}

/// T=112 C5 — resolve the effective replenish size.
///
/// Priority: `explicit_size` if `Some`; else `max_offline_codes` from config
/// if > 0; else sane default (1).  The C3 builder clamps 0 to error and
/// >2000 to 2000 — we do NOT duplicate that logic here.
pub fn resolve_replenish_size(max_offline_codes: i64, explicit_size: Option<u32>) -> u32 {
    match explicit_size {
        Some(s) => s,
        None => {
            if max_offline_codes > 0 {
                max_offline_codes.min(u32::MAX as i64) as u32
            } else {
                1
            }
        }
    }
}

/// T=112 C5 — look up the `tax_number` and `max_offline_codes` for a given
/// `fiscal_number` from `fiscal_number_config`.
///
/// Returns `AdminError::FiscalNumberNotInConfig` when no row exists — gives
/// the CLI a clear typed error before attempting any network call.
pub async fn lookup_fn_config_for_replenish(
    pool: &SqlitePool,
    fiscal_number: &str,
) -> Result<(String, i64), AdminError> {
    use crate::db::repositories::fiscal_number_config as fn_repo;
    let cfg = fn_repo::get(pool, fiscal_number)
        .await
        .map_err(|e| AdminError::Infrastructure(format!("fiscal_number_config lookup: {e}")))?;
    match cfg {
        Some(c) => Ok((c.tax_number, c.max_offline_codes)),
        None => Err(AdminError::FiscalNumberNotInConfig(
            fiscal_number.to_string(),
        )),
    }
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

// ─── A′.3 PR-O1 — offline operator surface (mode-seam + open_session + seed-codes) ───

/// Pre-read `node_state.mode` for an actionable wrong-mode diagnostic
/// (mirrors `reset_stop_mode`'s pre-read; the in-tx CAS guard still enforces
/// correctness under a concurrent probe).  Missing row → `FiscalNumberNotFound`.
async fn read_mode(pool: &SqlitePool, fiscal_number: &str) -> Result<String, AdminError> {
    let observed: Option<String> =
        sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = ?")
            .bind(fiscal_number)
            .fetch_optional(pool)
            .await
            .map_err(|e| AdminError::Infrastructure(format!("read node_state.mode: {e}")))?;
    observed.ok_or_else(|| AdminError::FiscalNumberNotFound(fiscal_number.to_string()))
}

/// A′.3 PR-O1 — operator **GO_OFFLINE** (the public DOOR).  Gated behind
/// [`crate::services::offline_sync::offline_surface::FULL_OFFLINE_SURFACE_READY`]:
/// fail-closed with [`AdminError::OfflineSurfaceNotReady`] while the drain
/// path is not yet enabled (O1).  The live door lands in O2 with the flag
/// flip + coupling-pin.  The machinery ([`go_offline_inner`]) is proven in O1
/// by direct unit tests + the offline reachability e2e (direct seams).
pub async fn go_offline(
    pool: &SqlitePool,
    fiscal_number: &str,
    reason: &str,
) -> Result<GoOfflineOutcome, AdminError> {
    crate::services::offline_sync::offline_surface::ensure_full_offline_surface_ready()
        .map_err(|_| AdminError::OfflineSurfaceNotReady)?;
    go_offline_inner(pool, fiscal_number, reason).await
}

/// The GO_OFFLINE machinery, gate-free (so O1 can prove the happy path while
/// the door stays shut).  Atomic per one `with_immediate` envelope:
///   1. CAS `node_state.mode` ONLINE → OFFLINE (MODE-ONLY; `shift_state`
///      untouched — Frozen #3).
///   2. Open an OFFLINE session (insert OPENING + Opening→Open CAS) so an
///      OPEN session exists BEFORE any offline doc (closes the
///      Offline-but-no-session window that `stage_offline_ack` Step-4 would
///      otherwise reject with `NoActiveSession`).
///   3. Emit `OFFLINE_SESSION_OPENED` (W5 parity) + `ADMIN_GO_OFFLINE`
///      Critical audit rows.
async fn go_offline_inner(
    pool: &SqlitePool,
    fiscal_number: &str,
    reason: &str,
) -> Result<GoOfflineOutcome, AdminError> {
    if reason.trim().is_empty() {
        return Err(AdminError::EmptyReason);
    }
    let observed = read_mode(pool, fiscal_number).await?;
    if observed != "ONLINE" {
        return Err(AdminError::NotInExpectedMode {
            fiscal_number: fiscal_number.to_string(),
            observed_mode: observed,
            expected: "ONLINE",
        });
    }

    let fn_owned = fiscal_number.to_string();
    let reason_owned = reason.to_string();
    let session_id = crate::db::models::ids::OfflineSessionId::new();
    let opened_at = chrono::Utc::now().to_rfc3339();
    let id_hex = crate::services::write_path::types::hex_encode_lower(session_id.as_bytes());
    let id_hex_ret = id_hex.clone();

    with_immediate(pool, move |tx| {
        Box::pin(async move {
            // (1) MODE-ONLY CAS ONLINE → OFFLINE.
            let flipped =
                crate::db::repositories::node_state::set_mode_offline_tx(tx, &fn_owned).await?;
            if !flipped {
                return Err(anyhow::anyhow!(
                    "admin: race detected during go_offline for fn={fn_owned} — mode CAS \
                     rows_affected=0 (concurrent state change; re-run command)"
                ));
            }
            // (2) Open the offline session atomically (same envelope).
            crate::db::repositories::offline_sessions::insert_opening(
                tx,
                &crate::db::repositories::offline_sessions::NewOpeningSession {
                    offline_session_id: session_id,
                    fiscal_number: &fn_owned,
                    opened_at: &opened_at,
                },
            )
            .await?;
            let outcome = crate::db::repositories::offline_sessions::transition_state(
                tx,
                session_id,
                crate::db::models::enums::OfflineSessionState::Opening,
                crate::db::models::enums::OfflineSessionState::Open,
                None,
            )
            .await?;
            if outcome != crate::db::repositories::fiscal_documents::TransitionOutcome::Applied {
                return Err(anyhow::anyhow!(
                    "admin: go_offline Opening→Open produced unexpected outcome: {outcome:?}"
                ));
            }
            // (3a) OFFLINE_SESSION_OPENED audit (W5 service parity).
            crate::db::repositories::audit_log::append_tx(
                tx,
                "offline_session",
                &id_hex,
                "OFFLINE_SESSION_OPENED",
                crate::db::models::enums::Severity::Info,
                None,
                None,
            )
            .await?;
            // (3b) ADMIN_GO_OFFLINE Critical audit with the mode transition.
            let payload = serde_json::json!({
                "fiscal_number": fn_owned,
                "reason": reason_owned,
                "mode_before": "ONLINE",
                "mode_after": "OFFLINE",
                "offline_session_id": id_hex,
            });
            crate::db::repositories::audit_log::append_tx(
                tx,
                "fn",
                &fn_owned,
                "ADMIN_GO_OFFLINE",
                crate::db::models::enums::Severity::Critical,
                None,
                Some(&payload.to_string()),
            )
            .await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await
    .map_err(|e| AdminError::Infrastructure(format!("go_offline envelope: {e}")))?;

    Ok(GoOfflineOutcome {
        fiscal_number: fiscal_number.to_string(),
        offline_session_id: id_hex_ret,
    })
}

/// A′.3 PR-O1 — operator **GO_ONLINE** (the public recovery DOOR).  Gated
/// like [`go_offline`].
pub async fn go_online(
    pool: &SqlitePool,
    fiscal_number: &str,
    reason: &str,
) -> Result<GoOnlineOutcome, AdminError> {
    crate::services::offline_sync::offline_surface::ensure_full_offline_surface_ready()
        .map_err(|_| AdminError::OfflineSurfaceNotReady)?;
    go_online_inner(pool, fiscal_number, reason).await
}

/// The GO_ONLINE machinery, gate-free.  CAS `node_state.mode`
/// `OFFLINE | GOING_OFFLINE → GOING_ONLINE` (MODE-ONLY) + `ADMIN_GO_ONLINE`
/// Critical audit.  The subsequent `GOING_ONLINE → ONLINE` convergence is
/// driven by the drain path — NOT here (inert until O2).
async fn go_online_inner(
    pool: &SqlitePool,
    fiscal_number: &str,
    reason: &str,
) -> Result<GoOnlineOutcome, AdminError> {
    if reason.trim().is_empty() {
        return Err(AdminError::EmptyReason);
    }
    let observed = read_mode(pool, fiscal_number).await?;
    if observed != "OFFLINE" && observed != "GOING_OFFLINE" {
        return Err(AdminError::NotInExpectedMode {
            fiscal_number: fiscal_number.to_string(),
            observed_mode: observed,
            expected: "OFFLINE or GOING_OFFLINE",
        });
    }

    let fn_owned = fiscal_number.to_string();
    let reason_owned = reason.to_string();
    let mode_before = observed.clone();

    with_immediate(pool, move |tx| {
        Box::pin(async move {
            let flipped =
                crate::db::repositories::node_state::set_mode_going_online_tx(tx, &fn_owned)
                    .await?;
            if !flipped {
                return Err(anyhow::anyhow!(
                    "admin: race detected during go_online for fn={fn_owned} — mode CAS \
                     rows_affected=0 (concurrent state change; re-run command)"
                ));
            }
            let payload = serde_json::json!({
                "fiscal_number": fn_owned,
                "reason": reason_owned,
                "mode_before": mode_before,
                "mode_after": "GOING_ONLINE",
            });
            crate::db::repositories::audit_log::append_tx(
                tx,
                "fn",
                &fn_owned,
                "ADMIN_GO_ONLINE",
                crate::db::models::enums::Severity::Critical,
                None,
                Some(&payload.to_string()),
            )
            .await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await
    .map_err(|e| AdminError::Infrastructure(format!("go_online envelope: {e}")))?;

    Ok(GoOnlineOutcome {
        fiscal_number: fiscal_number.to_string(),
    })
}

/// A′.3 PR-O1 (STOP-O1 ruling (b)) — manual offline-code provisioning for the
/// pilot drill.  Seeds `[first_lnd ..= last_lnd]` into the FN's `offline_codes`
/// pool via the tx-bound [`crate::db::repositories::offline_sessions::seed_code_range_tx`].
///
/// ⚠️ PILOT-DRILL AFFORDANCE, NOT a permanent mechanism.  The operator MUST
/// seed ONLY real DPS-issued ranges for this FN (from the DPS cabinet / prior
/// provisioning).  Invented codes would be sent to DPS on drain, rejected, and
/// cascade into RMR escalations.  The production code-fetch (a DPS ask-codes
/// request) is the named follow-up, co-scoped with the live campaign.
///
/// Validates the range (positive/ordered) and LOUD-rejects any overlap with the
/// existing pool (the primitive is INSERT OR IGNORE and would otherwise silently
/// dedupe a stale re-seed).  Emits `ADMIN_SEED_OFFLINE_CODES` Critical audit.
pub async fn seed_offline_codes(
    pool: &SqlitePool,
    fiscal_number: &str,
    first_lnd: i64,
    last_lnd: i64,
    reason: &str,
) -> Result<SeedCodesOutcome, AdminError> {
    if reason.trim().is_empty() {
        return Err(AdminError::EmptyReason);
    }
    if first_lnd < 1 || first_lnd > last_lnd {
        return Err(AdminError::InvalidCodeRange {
            first: first_lnd,
            last: last_lnd,
        });
    }
    let expected: u64 = (last_lnd - first_lnd + 1) as u64;
    let fn_owned = fiscal_number.to_string();
    let reason_owned = reason.to_string();
    // The overlap check + seed run in ONE `with_immediate` envelope so the
    // RESERVED lock serialises concurrent seeders: the LOUD overlap reject
    // cannot be raced past.  A pool-bound pre-check would TOCTOU — a concurrent
    // seed could slip codes into the range between the check and the tx, and
    // `seed_code_range_tx` (INSERT OR IGNORE) would then silently dedupe,
    // breaking the "seed exactly once" contract.  The typed AdminError is
    // carried out through `anyhow` and recovered by downcast (the
    // offline_session typed-error-preservation idiom).
    let result: Result<u64, anyhow::Error> = with_immediate(pool, move |tx| {
        Box::pin(async move {
            let overlap: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM offline_codes \
                 WHERE fiscal_number = ? AND code_lnd BETWEEN ? AND ?",
            )
            .bind(&fn_owned)
            .bind(first_lnd)
            .bind(last_lnd)
            .fetch_one(&mut **tx)
            .await?;
            if overlap > 0 {
                return Err(anyhow::Error::new(
                    AdminError::CodeRangeOverlapsExistingPool {
                        fiscal_number: fn_owned.clone(),
                        first: first_lnd,
                        last: last_lnd,
                        overlap_count: overlap,
                    },
                ));
            }
            let n = crate::db::repositories::offline_sessions::seed_code_range_tx(
                tx, &fn_owned, first_lnd, last_lnd,
            )
            .await?;
            // Defensive: after a clean (0-overlap) atomic check, INSERT OR IGNORE
            // must have inserted the WHOLE range.  A mismatch means the pool
            // moved under us — fail loud rather than silently under-seed.
            if n != expected {
                return Err(anyhow::anyhow!(
                    "seed_offline_codes: inserted {n} codes, expected {expected} for range \
                     [{first_lnd}..={last_lnd}] on fn={fn_owned} — unexpected pool state"
                ));
            }
            let payload = serde_json::json!({
                "fiscal_number": fn_owned,
                "reason": reason_owned,
                "first_lnd": first_lnd,
                "last_lnd": last_lnd,
                "inserted_count": n,
            });
            crate::db::repositories::audit_log::append_tx(
                tx,
                "fn",
                &fn_owned,
                "ADMIN_SEED_OFFLINE_CODES",
                crate::db::models::enums::Severity::Critical,
                None,
                Some(&payload.to_string()),
            )
            .await?;
            Ok::<u64, anyhow::Error>(n)
        })
    })
    .await;

    let inserted = match result {
        Ok(n) => n,
        Err(e) => {
            return Err(match e.downcast::<AdminError>() {
                Ok(admin_err) => admin_err,
                Err(other) => {
                    AdminError::Infrastructure(format!("seed_offline_codes envelope: {other}"))
                }
            });
        }
    };

    Ok(SeedCodesOutcome {
        fiscal_number: fiscal_number.to_string(),
        first_lnd,
        last_lnd,
        inserted_count: inserted,
    })
}

/// B8 test-support: seed a batch of DPS-issued opaque codes for `fiscal_number`
/// via [`crate::db::repositories::offline_sessions::insert_dps_codes_tx`].
///
/// Unlike [`seed_offline_codes`] (which seeds integer-range drill codes with
/// `dps_code NULL`), this function accepts explicit string codes and stores them
/// with `dps_code` set — the shape that `acquire_code_tx` requires after B8-1.
///
/// Intended for test helpers and pilot drills that need the realistic code shape.
/// No CLI exposure — pilot code injection uses the T=112 ask-codes flow in prod.
#[cfg(any(test, feature = "test-support"))]
pub async fn seed_dps_offline_codes(
    pool: &SqlitePool,
    fiscal_number: &str,
    codes: &[String],
) -> Result<crate::db::repositories::offline_sessions::InsertedSummary, AdminError> {
    if codes.is_empty() {
        return Ok(crate::db::repositories::offline_sessions::InsertedSummary {
            inserted: 0,
            deduped: 0,
        });
    }
    let fn_owned = fiscal_number.to_string();
    let codes_owned: Vec<String> = codes.to_vec();
    crate::db::tx::with_immediate(pool, move |tx| {
        Box::pin(async move {
            crate::db::repositories::offline_sessions::insert_dps_codes_tx(
                tx,
                &fn_owned,
                &codes_owned,
            )
            .await
            .map_err(anyhow::Error::from)
        })
    })
    .await
    .map_err(|e| AdminError::Infrastructure(format!("seed_dps_offline_codes: {e}")))
}

/// W2 / LOW-PR90-01 — abstract password-acquisition seam.  Production
/// CLI plugs in a [`RpasswordTtyPrompter`] (TTY) или [`StdinLinePrompter`]
/// (non-TTY pipe, for CI test harnesses); tests inject any
/// [`PasswordPrompter`] impl to drive [`acquire_password`] through
/// match / mismatch / empty / IO-error branches without touching the
/// real TTY.
///
/// # Executor safety
///
/// `prompt` is **synchronous and may block the calling thread for
/// unbounded duration** (until the operator finishes typing, or until
/// stdin returns EOF / IO error).  Callers MUST NOT invoke this trait
/// from an async context that runs on a tokio worker thread without
/// wrapping the call in `tokio::task::spawn_blocking`.  Acceptable
/// call sites: synchronous CLI entry points before `Runtime::block_on`,
/// or admin-only paths where the process is otherwise idle.
pub trait PasswordPrompter {
    /// Read one line of password input.  `prompt` is the human-readable
    /// "Password:" / "Repeat:" hint (CLI prints to stderr; tests can
    /// ignore).  Returns the typed string WITHOUT the trailing newline.
    fn prompt(&mut self, prompt: &str) -> std::io::Result<String>;
}

/// LOW-PR90-01 — acquire a cashier password via the supplied
/// [`PasswordPrompter`].  Behavior matrix:
///
///   - `is_tty = true`  → prompt twice, require exact match,
///     reject empty + reject all-whitespace.  Returns the verified
///     password.
///   - `is_tty = false` → single-line read from stdin pipe (no
///     confirmation; CI / scripted use case), reject empty +
///     all-whitespace.
///
/// Returns the plaintext as [`Zeroizing<Vec<u8>>`] so the byte buffer
/// is wiped from memory on drop (defence-in-depth — the obfuscated
/// BLOB lives in `operators.key_pass_enc` on disk, but the in-process
/// plaintext should not survive past the encode call site).
///
/// # Executor safety
///
/// Synchronous; see [`PasswordPrompter`] doc-block for the
/// blocking-thread caveat.
pub fn acquire_password<P: PasswordPrompter>(
    prompter: &mut P,
    is_tty: bool,
) -> Result<Zeroizing<Vec<u8>>, AdminError> {
    fn is_blank(s: &str) -> bool {
        s.is_empty() || s.chars().all(|c| c.is_whitespace())
    }
    // Intermediate `String` values from the prompter are wrapped in
    // `Zeroizing` immediately so the heap allocation of the typed
    // password is wiped on drop — without this, `String::drop` leaves
    // the bytes recoverable in freed heap pages until reuse.
    // `Zeroizing<String>` is available because `zeroize` 1.8 with the
    // default `alloc` feature implements `Zeroize for String` by
    // overwriting the string's bytes before deallocation.
    if is_tty {
        let first: Zeroizing<String> = Zeroizing::new(
            prompter
                .prompt("Cashier key password: ")
                .map_err(|e| AdminError::PasswordReadIo(e.to_string()))?,
        );
        if is_blank(&first) {
            return Err(AdminError::EmptyPassword);
        }
        let second: Zeroizing<String> = Zeroizing::new(
            prompter
                .prompt("Repeat password: ")
                .map_err(|e| AdminError::PasswordReadIo(e.to_string()))?,
        );
        if *first != *second {
            return Err(AdminError::PasswordMismatch);
        }
        // Hand the bytes to the caller wrapped in Zeroizing so the
        // owned Vec is also wiped on drop.  The intermediate
        // `Zeroizing<String>` values (`first`, `second`) are wiped
        // by their own Drop at end of scope.
        Ok(Zeroizing::new(first.as_bytes().to_vec()))
    } else {
        let one: Zeroizing<String> = Zeroizing::new(
            prompter
                .prompt("")
                .map_err(|e| AdminError::PasswordReadIo(e.to_string()))?,
        );
        if is_blank(&one) {
            return Err(AdminError::EmptyPassword);
        }
        Ok(Zeroizing::new(one.as_bytes().to_vec()))
    }
}

/// W2 — input for [`add_operator`].  Password arrives already
/// acquired (by [`acquire_password`] or test injection); admin layer
/// does not orchestrate stdin / TTY directly so the command logic
/// stays testable without TTY simulation infrastructure.
///
/// **Not `Clone`**: the `password` field is intentionally not
/// duplicatable to prevent accidental fan-out of the plaintext bytes.
/// Callers needing a second copy must construct a new
/// `AddOperatorInput` explicitly so the duplication is visible at
/// the call site.
///
/// **Custom `Debug`**: the derived `Debug` would delegate password
/// formatting to `Zeroizing<Vec<u8>>` which in turn delegates to
/// `Vec<u8>` — leaking the plaintext bytes via `{:?}` / `dbg!` /
/// any `tracing::` macro that uses Debug formatter on the struct.
/// We hand-implement Debug below to redact the password field.
pub struct AddOperatorInput {
    /// Cashier identifier — typically the cashier's INN per CLI
    /// `--inn` flag.  Required.
    pub operator_id: String,
    /// Human-readable cashier name — `--name` flag.  Required.
    pub name: String,
    /// Filesystem path to the cashier's `.dat` / `.jks` EDS carrier —
    /// `--key-path` flag.  Existence is NOT verified here; boot-time
    /// `BindingsRegistry::build_from_db` performs the load and emits
    /// `OPERATOR_KEY_LOAD_FAILED` if the path is unreadable.
    pub key_path: String,
    /// Fiscal number to bind the cashier to — `--fn` flag.  Cross-DB
    /// FK pre-check verifies this exists in `fiscal_number_config`
    /// BEFORE INSERT (prevents orphan rows).
    pub fiscal_number: String,
    /// Plaintext password bytes wrapped in [`Zeroizing`] — wiped on
    /// drop.  Admin layer encodes via [`crate::runtime::coding::Coding`]
    /// before storage and the wrapper ensures no stray copy survives
    /// past the encode call site.
    pub password: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for AddOperatorInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // operator_id is the cashier INN (PII) and name may identify the
        // cashier — both redacted (RS-1 F2, 2026-05-30): only audit_log may
        // carry the unredacted values, never a Debug/trace sink.
        f.debug_struct("AddOperatorInput")
            .field("operator_id", &"<redacted>")
            .field("name", &"<redacted>")
            .field("key_path", &self.key_path)
            .field("fiscal_number", &self.fiscal_number)
            .field("password", &"<redacted; len omitted>")
            .finish()
    }
}

/// W2 — register a new cashier (operator) bound to a fiscal_number.
///
/// Steps:
///
///   1. Validate all string args are non-empty.
///   2. Reject empty password.
///   3. Cross-DB FK pre-check: confirm `fiscal_number` exists in
///      `pool_main.fiscal_number_config`.  Returns
///      [`AdminError::FiscalNumberNotInConfig`] if not — prevents the
///      operator from registering an orphan that would only surface
///      at boot via `OPERATOR_ORPHAN_FN`.
///   4. Obfuscate password via [`crate::runtime::coding::Coding::encode`].
///   5. INSERT into `pool_secure.operators` via repository.
///       - On [`crate::db::repositories::operators::OperatorsRepoError::DuplicateActive`]
///         → returns [`AdminError::DuplicateActiveCashier`].
///   6. Append `ADMIN_OPERATOR_REGISTERED` Info audit to `pool_main`
///      (forensic trail — password / key_pass_enc NEVER appears in
///      payload, only operator_id + fiscal_number + key_path).
///
/// Password plaintext lifetime: the function consumes `input.password`
/// only to feed `Coding::encode`; the resulting encoded BLOB lives in
/// the INSERT bind.  No copy of the plaintext is retained or logged.
pub async fn add_operator(
    pool_main: &SqlitePool,
    pool_secure: &SqlitePool,
    input: AddOperatorInput,
) -> Result<(), AdminError> {
    if input.operator_id.trim().is_empty() {
        return Err(AdminError::EmptyArgument("inn"));
    }
    if input.name.trim().is_empty() {
        return Err(AdminError::EmptyArgument("name"));
    }
    if input.key_path.trim().is_empty() {
        return Err(AdminError::EmptyArgument("key-path"));
    }
    if input.fiscal_number.trim().is_empty() {
        return Err(AdminError::EmptyArgument("fn"));
    }
    if input.password.is_empty() || input.password.iter().all(|b| b.is_ascii_whitespace()) {
        return Err(AdminError::EmptyPassword);
    }

    // Cross-DB FK pre-check: refuse to register an operator whose FN
    // is not in fiscal_number_config.  This is the CLI-side mirror of
    // the boot-time check in `BindingsRegistry::build_from_db` —
    // catching the typo here gives a clean operator-facing error
    // instead of a surprise audit hours later at the next boot.
    let fn_present: Option<(String,)> =
        sqlx::query_as("SELECT fiscal_number FROM fiscal_number_config WHERE fiscal_number = ?")
            .bind(&input.fiscal_number)
            .fetch_optional(pool_main)
            .await
            .map_err(|e| AdminError::Infrastructure(format!("FN existence check: {e}")))?;
    if fn_present.is_none() {
        return Err(AdminError::FiscalNumberNotInConfig(input.fiscal_number));
    }

    // Pre-flight: catch the common DuplicateActiveCashier case BEFORE
    // any audit row lands.  This makes the most frequent operator-error
    // path observable as a CLI rejection without polluting the audit
    // trail with an ATTEMPTED→FAILED pair for an input the operator
    // can fix and retry immediately.  Race window between this read
    // and the eventual INSERT is acceptable: a concurrent add-operator
    // on the same FN is operationally bizarre (admin CLI is operator-
    // serialised), and the partial-unique-index in DDL is the
    // structural guard anyway — see R3-1 doc-block below.
    if let Some(existing) =
        crate::db::repositories::operators::find_by_fiscal_number(pool_secure, &input.fiscal_number)
            .await
            .map_err(|e| AdminError::Infrastructure(format!("active-cashier pre-flight: {e}")))?
    {
        let _ = existing; // discard row contents; FN is sufficient
        return Err(AdminError::DuplicateActiveCashier(input.fiscal_number));
    }

    let key_pass_enc = crate::runtime::coding::Coding::encode(&input.password)
        .map_err(|_| AdminError::EmptyPassword)?;

    let new_op = crate::db::repositories::operators::NewOperator {
        operator_id: input.operator_id.clone(),
        fiscal_number: input.fiscal_number.clone(),
        name: input.name.clone(),
        key_path: input.key_path.clone(),
        key_pass_enc,
    };

    // ---- R3-1: Truthful audit-event semantics under cross-DB constraint ----
    //
    // SQLite cannot wrap an INSERT into `pool_secure.operators` and an
    // INSERT into `pool_main.audit_log` in one transaction; the two
    // pools are physically separate files (HIGH-AUDIT-01).
    //
    // Round 2 reversed the order (audit-FIRST, INSERT-SECOND) to close
    // the silent-active-no-audit gap.  Round 3 audit caught that the
    // single `ADMIN_OPERATOR_REGISTERED` event_type lied on the
    // INSERT-failure branch (audit said "registered" but no row landed,
    // over-counting registrations in dashboards).
    //
    // Truthful three-event protocol:
    //
    //   1. `ADMIN_OPERATOR_REGISTRATION_ATTEMPTED` (Info) — emitted
    //      BEFORE the INSERT.  Carries identifiers; proves an attempt
    //      was made.  Crash here -> no operator row; operator retries
    //      and an attempted-no-completion pair can be reconciled
    //      forensically.
    //   2. On INSERT success -> `ADMIN_OPERATOR_REGISTERED` (Info).
    //      This is the ONLY event dashboards count as a successful
    //      registration (panel §4.12 query filters on this event_type
    //      exclusively).
    //   3. On INSERT failure -> `ADMIN_OPERATOR_REGISTRATION_FAILED`
    //      (Critical) with `reason` (DuplicateActive | DbError text).
    //      Paired with the prior ATTEMPTED row so a reviewer can grep
    //      the FN and see the full lifecycle.
    //
    // The pre-flight DuplicateActive check above means most operator-
    // typo cases never reach the audit pair at all — they fail at the
    // CLI with no audit pollution.  The ATTEMPTED/FAILED pair only
    // fires for races, crashes, and unexpected DB-class failures.
    //
    // Password / encoded BLOB NEVER appears in payload (security pin).
    // serde_json::json! escapes embedded chars (PR-B F1 fix preserved).
    let attempted_payload = serde_json::json!({
        "operator_id": input.operator_id,
        "name": input.name,
        "key_path": input.key_path,
    })
    .to_string();
    crate::db::repositories::audit_log::append(
        pool_main,
        "operator",
        &input.fiscal_number,
        "ADMIN_OPERATOR_REGISTRATION_ATTEMPTED",
        crate::db::models::enums::Severity::Info,
        None,
        Some(&attempted_payload),
    )
    .await
    .map_err(|e| AdminError::Infrastructure(format!("audit append ATTEMPTED: {e}")))?;

    match crate::db::repositories::operators::insert(pool_secure, &new_op).await {
        Ok(()) => {
            // Successful INSERT -> emit the truthful REGISTERED event.
            // Failure to append THIS event leaves the FN registered
            // operationally but with only the ATTEMPTED audit row;
            // the runbook §6a recovery scenarios cover the reconciliation.
            let registered_payload = serde_json::json!({
                "operator_id": input.operator_id,
                "name": input.name,
                "key_path": input.key_path,
            })
            .to_string();
            crate::db::repositories::audit_log::append(
                pool_main,
                "operator",
                &input.fiscal_number,
                "ADMIN_OPERATOR_REGISTERED",
                crate::db::models::enums::Severity::Info,
                None,
                Some(&registered_payload),
            )
            .await
            .map_err(|e| AdminError::Infrastructure(format!("audit append REGISTERED: {e}")))?;

            // W4-Z0 piece 7 + audit Round-1 fix (2026-05-27): per-FN
            // config bootstrap as **best-effort**.  Idempotent
            // (INSERT OR IGNORE for every row) so rotation /
            // additional-cashier add-operator calls remain no-ops on
            // the config tables.  Operator pre-customisations
            // (set-tax-rate, set-outgress-profile=EVPZ_DPS) survive.
            //
            // Pre-fix behaviour: bootstrap failure returned
            // Infrastructure error.  Operator saw "add-operator
            // failed" but operator row + REGISTERED audit had ALREADY
            // landed → re-running `add-operator` failed with
            // DuplicateActiveCashier → stranded operator with no
            // shipped recovery CLI.
            //
            // Post-fix behaviour: bootstrap failure logs via
            // tracing::error! + emits Critical audit
            // `ADMIN_FN_DEFAULTS_BOOTSTRAP_FAILED` carrying the
            // sqlite error text.  Operator row stays; add-operator
            // returns Ok.  Operator can then invoke individual
            // admin commands (`add-tax-group`, `add-payment`, ...)
            // to seed the missing config rows manually.  When the
            // standalone `bootstrap-defaults` admin command lands
            // (follow-up), that path becomes the canonical recovery.
            if let Err(e) =
                crate::runtime::bootstrap::bootstrap_fn_defaults(pool_secure, &input.fiscal_number)
                    .await
            {
                tracing::error!(
                    target: "prro::admin::add_operator",
                    fiscal_number = %input.fiscal_number,
                    cause = %e,
                    "bootstrap_fn_defaults failed AFTER operator INSERT — \
                     forensic trail recorded; operator must seed config \
                     defaults manually via per-table admin commands"
                );
                // Audit Round-3 (2026-05-27): if the bootstrap surface
                // produced a typed `PartialFailure`, unroll the per-row
                // failure details into the audit payload so operators
                // can grep the exact (table, identifier, cause) triples
                // for forensic recovery — not just "BootstrapError: ...".
                let bootstrap_failed_payload = match &e {
                    crate::runtime::bootstrap::BootstrapError::PartialFailure {
                        count,
                        failures,
                        ..
                    } => serde_json::json!({
                        "fiscal_number": input.fiscal_number,
                        "operator_id": input.operator_id,
                        "reason": "BootstrapError::PartialFailure",
                        "failed_row_count": count,
                        "failed_rows": failures
                            .iter()
                            .map(|f| serde_json::json!({
                                "table": f.table,
                                "identifier": f.identifier,
                                "cause": f.cause,
                            }))
                            .collect::<Vec<_>>(),
                    })
                    .to_string(),
                    other => serde_json::json!({
                        "fiscal_number": input.fiscal_number,
                        "operator_id": input.operator_id,
                        "reason": format!("BootstrapError: {other}"),
                    })
                    .to_string(),
                };
                // Audit-append failure here is best-effort too —
                // if even THIS audit can't land, log to tracing and
                // continue (the operator row is the primary
                // artifact; operator can run `list-tax-groups` to
                // confirm missing config and remediate).
                if let Err(audit_err) = crate::db::repositories::audit_log::append(
                    pool_main,
                    "operator",
                    &input.fiscal_number,
                    "ADMIN_FN_DEFAULTS_BOOTSTRAP_FAILED",
                    crate::db::models::enums::Severity::Critical,
                    None,
                    Some(&bootstrap_failed_payload),
                )
                .await
                {
                    tracing::error!(
                        target: "prro::admin::add_operator",
                        fiscal_number = %input.fiscal_number,
                        cause = %audit_err,
                        "audit append for ADMIN_FN_DEFAULTS_BOOTSTRAP_FAILED \
                         FAILED — forensic trail missing"
                    );
                }
            }

            Ok(())
        }
        Err(crate::db::repositories::operators::OperatorsRepoError::DuplicateActive(fn_id)) => {
            let failed_payload = serde_json::json!({
                "operator_id": input.operator_id,
                "reason": "DuplicateActiveCashier",
            })
            .to_string();
            // Audit-of-audit-failure observability (R4-3): the original
            // intent of this branch is to surface the DuplicateActive
            // error to the operator; we MUST NOT mask that error if the
            // FAILED audit append itself fails.  But silent discard
            // (the prior `let _ = ...`) leaves no forensic trail when
            // the audit DB is the broken thing.  Compromise: log via
            // `tracing::error!` so process logs at least record the
            // audit failure cause; the primary error still propagates
            // back to the operator as `DuplicateActiveCashier`.
            if let Err(e) = crate::db::repositories::audit_log::append(
                pool_main,
                "operator",
                &input.fiscal_number,
                "ADMIN_OPERATOR_REGISTRATION_FAILED",
                crate::db::models::enums::Severity::Critical,
                None,
                Some(&failed_payload),
            )
            .await
            {
                tracing::error!(
                    target: "prro::admin::add_operator",
                    fiscal_number = %fn_id,
                    cause = %e,
                    "audit append for ADMIN_OPERATOR_REGISTRATION_FAILED \
                     (DuplicateActive branch) FAILED — forensic trail missing"
                );
            }
            Err(AdminError::DuplicateActiveCashier(fn_id))
        }
        Err(crate::db::repositories::operators::OperatorsRepoError::Db(e)) => {
            let failed_payload = serde_json::json!({
                "operator_id": input.operator_id,
                "reason": format!("DbError: {e}"),
            })
            .to_string();
            // R4-3 observability — see DuplicateActive branch above.
            if let Err(audit_err) = crate::db::repositories::audit_log::append(
                pool_main,
                "operator",
                &input.fiscal_number,
                "ADMIN_OPERATOR_REGISTRATION_FAILED",
                crate::db::models::enums::Severity::Critical,
                None,
                Some(&failed_payload),
            )
            .await
            {
                tracing::error!(
                    target: "prro::admin::add_operator",
                    fiscal_number = %input.fiscal_number,
                    cause = %audit_err,
                    insert_cause = %e,
                    "audit append for ADMIN_OPERATOR_REGISTRATION_FAILED \
                     (Db branch) FAILED — forensic trail missing"
                );
            }
            Err(AdminError::Infrastructure(format!("INSERT operators: {e}")))
        }
    }
}

/// CLI entry-point for `prro admin add-operator`.  Reads config,
/// opens singleton lock + both pools (main + secure), detects TTY
/// mode from stdin, prompts for password via [`acquire_password`],
/// then dispatches to [`add_operator`].  Returns BSD sysexits-aligned
/// exit code.
///
/// Synchronous prompt path is executed BEFORE re-entering the tokio
/// runtime — `acquire_password` blocks the calling thread but the
/// runtime is not actively serving anything (this is an admin CLI).
pub async fn run_add_operator(
    config_path: &Path,
    operator_id: String,
    name: String,
    key_path: String,
    fiscal_number: String,
) -> Result<(), AdminError> {
    use std::io::IsTerminal;

    let cfg_text = std::fs::read_to_string(config_path)
        .map_err(|e| AdminError::Infrastructure(format!("read config: {e}")))?;
    let cfg = crate::config::AppConfig::from_toml(&cfg_text)
        .map_err(|e| AdminError::Infrastructure(format!("parse config: {e}")))?;

    let _lock = crate::runtime::singleton::acquire(&cfg.database.db_path)
        .map_err(|e| AdminError::Infrastructure(format!("singleton lock: {e}")))?;
    let pool_main = crate::db::open_pool(&cfg.database.db_path)
        .await
        .map_err(|e| AdminError::Infrastructure(format!("open main pool: {e}")))?;
    let pool_secure = crate::db::open_secure_pool(&cfg.database.secure_db_path)
        .await
        .map_err(|e| AdminError::Infrastructure(format!("open secure pool: {e}")))?;

    let is_tty = std::io::stdin().is_terminal();
    let mut prompter = if is_tty {
        StdinPasswordPrompter::tty()
    } else {
        StdinPasswordPrompter::stdin_pipe()
    };
    let password = acquire_password(&mut prompter, is_tty)?;

    let input = AddOperatorInput {
        operator_id,
        name,
        key_path,
        fiscal_number,
        password,
    };
    add_operator(&pool_main, &pool_secure, input).await?;

    pool_secure.close().await;
    pool_main.close().await;
    Ok(())
}

/// Production [`PasswordPrompter`].
///
/// Two paths based on construction:
///
///   - [`Self::tty`] uses `rpassword::prompt_password` — no-echo input
///     (the terminal is put into a no-echo mode for the read; the
///     password never appears on the operator's screen or in scrollback).
///   - [`Self::stdin_pipe`] reads a single stdin line via [`std::io::stdin`] —
///     for non-TTY contexts (CI, scripted use); echo is irrelevant
///     because there is no terminal.
///
/// Callers select the correct flavor based on
/// [`std::io::IsTerminal::is_terminal()`] for stdin; see
/// [`run_add_operator`] for the production wiring.
pub enum StdinPasswordPrompter {
    Tty,
    StdinPipe,
}

impl StdinPasswordPrompter {
    pub fn tty() -> Self {
        Self::Tty
    }
    pub fn stdin_pipe() -> Self {
        Self::StdinPipe
    }
}

impl PasswordPrompter for StdinPasswordPrompter {
    fn prompt(&mut self, msg: &str) -> std::io::Result<String> {
        match self {
            Self::Tty => {
                // rpassword writes `msg` to /dev/tty (NOT stderr) and
                // reads the password with terminal echo disabled.  No
                // plaintext lands in the terminal scrollback or in any
                // captured stderr stream.
                rpassword::prompt_password(msg)
            }
            Self::StdinPipe => {
                use std::io::{stderr, stdin, BufRead, Write};
                if !msg.is_empty() {
                    let mut err = stderr().lock();
                    err.write_all(msg.as_bytes())?;
                    err.flush()?;
                }
                let mut line = String::new();
                let stdin = stdin();
                let mut handle = stdin.lock();
                handle.read_line(&mut line)?;
                // Strip the trailing newline (LF or CRLF) so the typed
                // password matches what the operator entered, not the
                // terminal's line-discipline artifact.
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                Ok(line)
            }
        }
    }
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

/// Shared CLI boot for the A′.3 offline admin commands: read config, acquire
/// the singleton lock (refuses to race `prro serve`), open the pool (runs
/// migrations).  The returned guard MUST be held for the pool's lifetime.
async fn open_admin_pool(
    config_path: &Path,
) -> Result<(crate::runtime::singleton::PidLock, SqlitePool), AdminError> {
    let cfg_text = std::fs::read_to_string(config_path)
        .map_err(|e| AdminError::Infrastructure(format!("read config: {e}")))?;
    let cfg = crate::config::AppConfig::from_toml(&cfg_text)
        .map_err(|e| AdminError::Infrastructure(format!("parse config: {e}")))?;
    let lock = crate::runtime::singleton::acquire(&cfg.database.db_path)
        .map_err(|e| AdminError::Infrastructure(format!("singleton lock: {e}")))?;
    let pool = crate::db::open_pool(&cfg.database.db_path)
        .await
        .map_err(|e| AdminError::Infrastructure(format!("open db pool: {e}")))?;
    Ok((lock, pool))
}

/// CLI entry-point for `prro admin go-offline`.
pub async fn run_go_offline(
    config_path: &Path,
    fiscal_number: &str,
    reason: &str,
) -> Result<GoOfflineOutcome, AdminError> {
    let (_lock, pool) = open_admin_pool(config_path).await?;
    let outcome = go_offline(&pool, fiscal_number, reason).await?;
    drop(pool);
    Ok(outcome)
}

/// CLI entry-point for `prro admin go-online`.
pub async fn run_go_online(
    config_path: &Path,
    fiscal_number: &str,
    reason: &str,
) -> Result<GoOnlineOutcome, AdminError> {
    let (_lock, pool) = open_admin_pool(config_path).await?;
    let outcome = go_online(&pool, fiscal_number, reason).await?;
    drop(pool);
    Ok(outcome)
}

/// CLI entry-point for `prro admin seed-codes`.
pub async fn run_seed_offline_codes(
    config_path: &Path,
    fiscal_number: &str,
    first_lnd: i64,
    last_lnd: i64,
    reason: &str,
) -> Result<SeedCodesOutcome, AdminError> {
    let (_lock, pool) = open_admin_pool(config_path).await?;
    let outcome = seed_offline_codes(&pool, fiscal_number, first_lnd, last_lnd, reason).await?;
    drop(pool);
    Ok(outcome)
}

/// CLI entry-point for `prro admin request-offline-codes`.
///
/// ## Boot model
///
/// Calls `App::boot` (same as `prro serve` / `prro migrate`) which acquires
/// the singleton advisory lock.  **`prro serve` must be stopped before
/// running this command** — the singleton lock prevents two processes from
/// holding it simultaneously.  In-process / auto-replenish is a deferred
/// follow-up; this command is a maintenance-time trigger.
///
/// ## Key loading
///
/// Mirrors `doctor::live::run_live_from_env` (file:
/// `src/doctor/live.rs:344`): reads `PRRO_LIVE_DPS_JKS_PATH` +
/// `PRRO_LIVE_DPS_JKS_PASS`, calls `extract_private_key` +
/// `SigningSession::from_extracted` + assembles `SigningContext`.
///
/// ## Replenish call
///
/// Passes the assembled `App` / `Arc<dyn DpsChannel>` / `Arc<SigningContext>`
/// to `OfflineCodeReplenishService::new` and calls `replenish`.  The service
/// acquires the per-FN `fn_write_gate` internally (invariant #2).
pub async fn run_request_offline_codes(
    config_path: &Path,
    fiscal_number: &str,
    explicit_size: Option<u32>,
    host: &str,
    di: u32,
) -> Result<ReplenishOutcome, AdminError> {
    use crate::config::AppConfig;
    use crate::crypto::in_process::InProcessProvider;
    use crate::crypto::provider::CryptoProvider;
    use crate::crypto::session::SigningSession;
    use crate::runtime::key_loader::build_fn_sign;
    use crate::services::offline_sync::offline_code_replenish::OfflineCodeReplenishService;
    use crate::services::write_path::stage_sign::SigningContext;
    use crate::transports::dps::channel::DpsChannel;
    use crate::transports::dps::grpc::GrpcDpsChannel;
    use prro_crypto::cms::profile::CmsProfile;
    use prro_crypto::interop::prro::containers::extract_private_key;
    use std::sync::Arc;
    use std::time::Duration;

    // ── 1. Boot App (reads config, acquires singleton lock, migrates DB) ─────
    // Pattern identical to `boot_from_path_or_exit` in main.rs.
    let cfg_text = std::fs::read_to_string(config_path)
        .map_err(|e| AdminError::Infrastructure(format!("read config: {e}")))?;
    let cfg = AppConfig::from_toml(&cfg_text)
        .map_err(|e| AdminError::Infrastructure(format!("parse config: {e}")))?;
    let app = crate::App::boot(cfg)
        .await
        .map_err(|e| AdminError::Infrastructure(format!("App::boot failed: {e}")))?;

    // ── 2. Resolve tax_number + effective size from fiscal_number_config ─────
    let (tax_number, max_offline_codes) =
        lookup_fn_config_for_replenish(app.db(), fiscal_number).await?;
    let size = resolve_replenish_size(max_offline_codes, explicit_size);

    // ── 3. Load JKS key (mirrors doctor::live::run_live_from_env) ────────────
    let jks_path = std::env::var("PRRO_LIVE_DPS_JKS_PATH").map_err(|_| {
        AdminError::Infrastructure(
            "PRRO_LIVE_DPS_JKS_PATH not set (required for request-offline-codes)".to_string(),
        )
    })?;
    let jks_pass = std::env::var("PRRO_LIVE_DPS_JKS_PASS").map_err(|_| {
        AdminError::Infrastructure(
            "PRRO_LIVE_DPS_JKS_PASS not set (required for request-offline-codes)".to_string(),
        )
    })?;
    let jks_data = std::fs::read(&jks_path)
        .map_err(|e| AdminError::Infrastructure(format!("cannot read JKS at {jks_path}: {e}")))?;
    let extracted = extract_private_key(&jks_data, &jks_pass)
        .map_err(|e| AdminError::Infrastructure(format!("JKS load/decrypt failed: {e:?}")))?;

    // Build SigningContext — same profile as write-path stage_sign and
    // the production JksOperatorKeyLoader (src/runtime/key_loader.rs:82).
    let session =
        SigningSession::from_extracted(fiscal_number.to_string(), extracted).map_err(|_| {
            AdminError::Infrastructure("no signing certificate found in JKS container".to_string())
        })?;
    let sign_ctx = Arc::new(SigningContext {
        provider: Arc::new(InProcessProvider::new()) as Arc<dyn CryptoProvider>,
        session,
        profile: CmsProfile::Dstu4145WithGost34311Pb,
    });

    // Validate key health early (same check as doctor --live).
    let _fn_sign = build_fn_sign(&sign_ctx.session, fiscal_number)
        .map_err(|e| AdminError::Infrastructure(format!("fn_sign build failed: {e:?}")))?;

    // ── 4. Connect DPS gRPC channel ───────────────────────────────────────────
    // Pattern from doctor::live::run_live_from_env (src/doctor/live.rs:392).
    let dps = GrpcDpsChannel::connect(host, Duration::from_secs(30))
        .await
        .map_err(|e| AdminError::Infrastructure(format!("DPS connect to {host} failed: {e:?}")))?;
    let dps: Arc<dyn DpsChannel> = Arc::new(dps);

    // ── 5. Run replenish via OfflineCodeReplenishService ──────────────────────
    // `new(app, dps, sign_ctx)` — same constructor as C4 tests
    // (tests/offline_code_replenish.rs:132).  The service acquires the
    // per-FN fn_write_gate internally (invariant #2).
    let svc = OfflineCodeReplenishService::new(app.clone(), dps, sign_ctx);
    let summary = svc
        .replenish(fiscal_number, &tax_number, di, size)
        .await
        .map_err(|e| AdminError::ReplenishFailed(e.to_string()))?;

    Ok(ReplenishOutcome {
        fiscal_number: fiscal_number.to_string(),
        tax_number,
        codes_received: summary.codes_received,
        inserted: summary.inserted,
        deduped: summary.deduped,
        new_seed_hex: summary.new_seed_hex,
        request_xml: summary.request_xml,
    })
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

    // ─── A′.3 PR-O1 — offline operator surface ─────────────────────────

    async fn read_offline_session_state(pool: &SqlitePool, fn_id: &str) -> Option<String> {
        sqlx::query_scalar("SELECT state FROM offline_sessions WHERE fiscal_number = ?")
            .bind(fn_id)
            .fetch_optional(pool)
            .await
            .unwrap()
    }

    async fn latest_audit(pool: &SqlitePool, event_type: &str) -> Option<(String, String)> {
        sqlx::query_as(
            "SELECT severity, event_payload_json FROM audit_log \
             WHERE event_type = ? ORDER BY audit_id DESC LIMIT 1",
        )
        .bind(event_type)
        .fetch_optional(pool)
        .await
        .unwrap()
    }

    /// RP-O1-8 (gated-pin): the DOOR stays shut while the surface flag is
    /// false — GO_OFFLINE refuses without mutating.
    ///
    /// O2 flip: inverted from the O1 gated-pin per its own contract — the
    /// DOOR is now LIVE.  GO_OFFLINE (public, gated) flips ONLINE→OFFLINE and
    /// opens an OPEN session.  RED against `FULL_OFFLINE_SURFACE_READY=false`
    /// (the door refuses) — this is the flip's teeth.
    #[tokio::test]
    async fn go_offline_via_open_door_flips_and_opens_session() {
        let (_d, pool) = fresh_pool().await;
        seed_node_state(&pool, "ONLINE").await;

        let outcome = go_offline(&pool, "1234567890", "operator net drop")
            .await
            .expect("door is live after the O2 flip");
        assert_eq!(outcome.fiscal_number, "1234567890");

        let mode: String =
            sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = ?")
                .bind("1234567890")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(mode, "OFFLINE");
        assert_eq!(
            read_offline_session_state(&pool, "1234567890")
                .await
                .as_deref(),
            Some("OPEN")
        );
    }

    /// RP-O1-1 / RP-O1-2: the gate-free GO_OFFLINE machinery flips
    /// ONLINE→OFFLINE (mode-only), opens an OPEN session in the SAME envelope
    /// (no Offline-but-no-session window), and emits ADMIN_GO_OFFLINE.
    #[tokio::test]
    async fn go_offline_inner_flips_offline_opens_session_and_audits() {
        let (_d, pool) = fresh_pool().await;
        seed_node_state(&pool, "ONLINE").await;

        let outcome = go_offline_inner(&pool, "1234567890", "operator net drop 2026-07-07")
            .await
            .unwrap();
        assert_eq!(outcome.fiscal_number, "1234567890");
        assert!(!outcome.offline_session_id.is_empty());

        let (mode, shift): (String, String) =
            sqlx::query_as("SELECT mode, shift_state FROM node_state WHERE fiscal_number = ?")
                .bind("1234567890")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(mode, "OFFLINE");
        assert_eq!(
            shift, "OPENED",
            "GO_OFFLINE must not touch shift_state (Frozen #3)"
        );

        assert_eq!(
            read_offline_session_state(&pool, "1234567890")
                .await
                .as_deref(),
            Some("OPEN"),
            "an OPEN session must exist before any offline doc"
        );

        let (sev, payload) = latest_audit(&pool, "ADMIN_GO_OFFLINE")
            .await
            .expect("ADMIN_GO_OFFLINE audit row");
        assert_eq!(sev, "CRITICAL");
        let p: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(p["mode_before"], "ONLINE");
        assert_eq!(p["mode_after"], "OFFLINE");
        assert_eq!(p["reason"], "operator net drop 2026-07-07");
    }

    /// GO_OFFLINE mode-guard: a non-ONLINE node is refused, unmutated.
    #[tokio::test]
    async fn go_offline_inner_refuses_non_online() {
        let (_d, pool) = fresh_pool().await;
        seed_node_state(&pool, "BLOCKED").await;

        let err = go_offline_inner(&pool, "1234567890", "reason")
            .await
            .expect_err("must refuse non-ONLINE");
        match err {
            AdminError::NotInExpectedMode {
                observed_mode,
                expected,
                ..
            } => {
                assert_eq!(observed_mode, "BLOCKED");
                assert_eq!(expected, "ONLINE");
            }
            other => panic!("expected NotInExpectedMode, got {other:?}"),
        }
        let mode: String =
            sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = ?")
                .bind("1234567890")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(mode, "BLOCKED");
    }

    /// GO_OFFLINE rejects an empty/whitespace reason (forensic trail).
    #[tokio::test]
    async fn go_offline_inner_refuses_empty_reason() {
        let (_d, pool) = fresh_pool().await;
        seed_node_state(&pool, "ONLINE").await;
        assert!(matches!(
            go_offline_inner(&pool, "1234567890", "  ")
                .await
                .expect_err("empty reason"),
            AdminError::EmptyReason
        ));
    }

    /// O2 flip: the recovery DOOR is now LIVE — GO_ONLINE (public, gated)
    /// flips OFFLINE→GOING_ONLINE.  RED against `FULL_OFFLINE_SURFACE_READY=
    /// false` (the flip's teeth).
    #[tokio::test]
    async fn go_online_via_open_door_flips_to_going_online() {
        let (_d, pool) = fresh_pool().await;
        seed_node_state(&pool, "OFFLINE").await;
        go_online(&pool, "1234567890", "dps back")
            .await
            .expect("recovery door is live after the O2 flip");
        let mode: String =
            sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = ?")
                .bind("1234567890")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(mode, "GOING_ONLINE");
    }

    /// RP-O1-10: GO_ONLINE machinery flips OFFLINE→GOING_ONLINE + audits.
    /// (Convergence drain→ONLINE is NOT asserted here — inert until O2.)
    #[tokio::test]
    async fn go_online_inner_flips_offline_to_going_online() {
        let (_d, pool) = fresh_pool().await;
        seed_node_state(&pool, "OFFLINE").await;

        let outcome = go_online_inner(&pool, "1234567890", "dps connectivity restored")
            .await
            .unwrap();
        assert_eq!(outcome.fiscal_number, "1234567890");
        let mode: String =
            sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = ?")
                .bind("1234567890")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(mode, "GOING_ONLINE");
        let (sev, payload) = latest_audit(&pool, "ADMIN_GO_ONLINE")
            .await
            .expect("ADMIN_GO_ONLINE audit");
        assert_eq!(sev, "CRITICAL");
        let p: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(p["mode_before"], "OFFLINE");
        assert_eq!(p["mode_after"], "GOING_ONLINE");
    }

    /// RP-O1-6 / RP-O1-7: seed-codes populates the pool + Critical audit
    /// with the range in the payload.
    #[tokio::test]
    async fn seed_offline_codes_populates_pool_and_audits() {
        let (_d, pool) = fresh_pool().await;

        let outcome = seed_offline_codes(&pool, "1234567890", 100, 104, "cabinet range 2026-07")
            .await
            .unwrap();
        assert_eq!(outcome.inserted_count, 5);
        assert_eq!(outcome.first_lnd, 100);
        assert_eq!(outcome.last_lnd, 104);

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ?")
                .bind("1234567890")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 5);

        let (sev, payload) = latest_audit(&pool, "ADMIN_SEED_OFFLINE_CODES")
            .await
            .expect("ADMIN_SEED_OFFLINE_CODES audit");
        assert_eq!(sev, "CRITICAL");
        let p: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(p["first_lnd"], 100);
        assert_eq!(p["last_lnd"], 104);
        assert_eq!(p["inserted_count"], 5);
    }

    /// FW-1 mutation teeth — the `seed_offline_codes` range guard
    /// `if first_lnd < 1 || first_lnd > last_lnd` (admin.rs). The `< 1`→`<= 1`
    /// mutant wrongly rejects `first_lnd == 1` (the FIRST legal offline code); the
    /// `> last`→`>= last` mutant wrongly rejects a single-code range
    /// (`first == last`). Both are FALSE-POSITIVE rejections of valid provisioning
    /// the operator legitimately needs. The populates test (100,104) and the
    /// rejects test (5,3 / 0,4) never touch either boundary, so both survive.
    #[tokio::test]
    async fn seed_offline_codes_accepts_min_lnd_and_single_code_boundaries() {
        let (_d, pool) = fresh_pool().await;
        // first_lnd == 1 (the first legal offline code) must be ACCEPTED —
        // mutant `first_lnd <= 1` wrongly rejects it.
        let out = seed_offline_codes(&pool, "1234567890", 1, 3, "min-lnd boundary")
            .await
            .expect("first_lnd==1 is a valid range");
        assert_eq!(out.first_lnd, 1);
        assert_eq!(out.inserted_count, 3);
        // Single-code range first_lnd == last_lnd must be ACCEPTED —
        // mutant `first_lnd >= last_lnd` wrongly rejects it. Same (registered) FN,
        // a non-overlapping range so the overlap pre-check stays clear.
        let out2 = seed_offline_codes(&pool, "1234567890", 100, 100, "single-code boundary")
            .await
            .expect("single-code range (first==last) is valid");
        assert_eq!(out2.first_lnd, 100);
        assert_eq!(out2.last_lnd, 100);
        assert_eq!(out2.inserted_count, 1);
    }

    /// seed-codes rejects a non-positive / inverted range.
    #[tokio::test]
    async fn seed_offline_codes_rejects_invalid_range() {
        let (_d, pool) = fresh_pool().await;
        assert!(matches!(
            seed_offline_codes(&pool, "1234567890", 5, 3, "r")
                .await
                .expect_err("inverted"),
            AdminError::InvalidCodeRange { .. }
        ));
        assert!(matches!(
            seed_offline_codes(&pool, "1234567890", 0, 4, "r")
                .await
                .expect_err("non-positive"),
            AdminError::InvalidCodeRange { .. }
        ));
    }

    /// RP-O1-6: seed-codes LOUD-rejects an overlap with the existing pool
    /// (the primitive is INSERT OR IGNORE; the command surfaces the overlap).
    #[tokio::test]
    async fn seed_offline_codes_rejects_overlap() {
        let (_d, pool) = fresh_pool().await;
        seed_offline_codes(&pool, "1234567890", 100, 104, "first")
            .await
            .unwrap();

        let err = seed_offline_codes(&pool, "1234567890", 103, 106, "overlapping")
            .await
            .expect_err("overlap must be rejected loudly");
        match err {
            AdminError::CodeRangeOverlapsExistingPool { overlap_count, .. } => {
                assert_eq!(overlap_count, 2, "codes 103, 104 overlap");
            }
            other => panic!("expected CodeRangeOverlapsExistingPool, got {other:?}"),
        }
    }

    // ─── C5: request-offline-codes tests ─────────────────────────────────────

    /// C5 pin (a): `resolve_replenish_size` — when `--size` omitted it falls
    /// back to `max_offline_codes`; explicit `--size` passes through unchanged.
    #[test]
    fn resolve_replenish_size_from_config_when_not_specified() {
        // None → falls back to max_offline_codes (100).
        assert_eq!(resolve_replenish_size(100, None), 100);
    }

    #[test]
    fn resolve_replenish_size_explicit_overrides_config() {
        // Some(42) → 42, regardless of config value.
        assert_eq!(resolve_replenish_size(100, Some(42)), 42);
    }

    #[test]
    fn resolve_replenish_size_zero_config_falls_back_to_default() {
        // Config 0 → sane default (1).
        assert_eq!(resolve_replenish_size(0, None), 1);
    }

    /// C5 pin (b): missing FN in `fiscal_number_config` → typed error, not panic.
    #[tokio::test]
    async fn request_offline_codes_missing_fn_config_returns_typed_error() {
        let (_d, pool) = fresh_pool().await;
        // No fiscal_number_config row for "9999999999".
        let err = lookup_fn_config_for_replenish(&pool, "9999999999")
            .await
            .expect_err("must fail");
        assert!(
            matches!(err, AdminError::FiscalNumberNotInConfig(_)),
            "expected FiscalNumberNotInConfig, got {err:?}"
        );
    }

    /// C5 pin (c): `ReplenishError` → `AdminError::ReplenishFailed` maps to
    /// non-zero exit code, and the message contains the error description.
    #[test]
    fn replenish_error_maps_to_nonzero_exit_code() {
        let err = AdminError::ReplenishFailed("node_state row missing for FN".to_string());
        assert_ne!(err.exit_code(), 0, "ReplenishFailed must be non-zero exit");
        let msg = format!("{err}");
        assert!(
            msg.contains("node_state row missing"),
            "message must carry detail"
        );
    }
}
