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
            | AdminError::PasswordMismatch => 64,
            // EX_IOERR (74): input device failure.
            AdminError::PasswordReadIo(_) => 74,
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
