//! RS-3 A2.4 — the production write-path binding (`InlineWritePath`).
//!
//! Replaces [`UnimplementedWritePath`](super::seam::UnimplementedWritePath) at
//! the supervisor DI root once the A2.4 prerequisite gates pass (advance-at-SEND
//! / D5 gate + resolver / migration 027 / `m1_02` RED-pin green).  A thin,
//! FN-agnostic adapter: the handler hands it an inbox row; it resolves the
//! per-FN deps from the boot-built [`BindingsRegistry`] (DPS channel + signing
//! context), builds a fresh `fn_sign`, establishes the per-FN single-writer
//! lease ([`App::acquire_fn_gate`] — invariant #2, held OUTSIDE every write-tx
//! per invariant #1), and delegates to [`inline::run`].
//!
//! It owns NOTHING beyond the wiring: the inbox-terminalise obligation, the
//! lease CAS, and the fail-safe replay semantics all live in `inline::run`
//! (audited in the A2.4 Phase-1 arm-table).
use std::sync::Arc;

use async_trait::async_trait;

use crate::app::App;
use crate::db::repositories::ingress_inbox::InboxRow;
use crate::runtime::bindings::BindingsRegistry;
use crate::runtime::ingress::seam::{FiscalError, FiscalOutcome, WritePathEntry};
use crate::runtime::key_loader::build_fn_sign;
use crate::services::write_path::inline;
use crate::services::write_path::inline_map::codes;

/// The inline write-path binding.  Holds an [`App`] (pools + per-FN gate) and
/// the boot-built [`BindingsRegistry`] (per-FN DPS + signing context); resolves
/// both by `row.fiscal_number` at `fiscalize` time.
pub struct InlineWritePath {
    app: App,
    registry: Arc<BindingsRegistry>,
}

impl InlineWritePath {
    pub fn new(app: App, registry: Arc<BindingsRegistry>) -> Self {
        Self { app, registry }
    }
}

/// RS-3 A2.4 finding-2 pin — the SINGLE production write-path construction site.
/// The supervisor DI root calls THIS (not an inline `Arc::new(...)`) so the
/// binding choice is behaviourally testable: `production_write_path(..).fiscalize`
/// must NEVER return `NotImplemented`.  A revert of the flip (back to
/// `UnimplementedWritePath`) here fails the DI pin
/// (`tests/a3_final_binding_flip.rs`).
pub fn production_write_path(app: App, registry: Arc<BindingsRegistry>) -> Arc<dyn WritePathEntry> {
    Arc::new(InlineWritePath::new(app, registry))
}

#[async_trait]
impl WritePathEntry for InlineWritePath {
    async fn fiscalize(&self, row: &InboxRow) -> Result<FiscalOutcome, FiscalError> {
        let fn_id = row.fiscal_number.as_str();
        // Resolve the per-FN binding (DPS channel + signing context) from the
        // boot-built registry.  A registered RestHttp listener exists ONLY for
        // an FN present in the registry (`bind_rest_ingress` cross-checks
        // `fiscal_number_config`), so a miss here is a structural boot/registry
        // breach → 500 (defensive; unreachable on a healthy boot).
        let binding = self.registry.get(fn_id).ok_or(FiscalError::Internal {
            request_id: row.request_id,
            code: codes::MISSING_PROFILE_BINDING,
        })?;
        // A FRESH per-request `fn_sign` (stamps `signingTime = now()`), same as
        // the supervisor's reconcile / convergence passes build per tick.
        let fn_sign =
            build_fn_sign(&binding.sign_ctx.session, fn_id).map_err(|_| FiscalError::Internal {
                request_id: row.request_id,
                // A broken operator key/cert (the CMS build over the FN failed)
                // → a signing-setup breach, distinct from a missing binding.
                code: codes::SIGN_INTERNAL,
            })?;
        // Establish the per-FN single-writer lease and hold it across the ENTIRE
        // fiscalize (invariant #2; the A4 forward contract on `acquire_fn_gate`).
        // The gate is a `tokio::sync::Mutex`, held OUTSIDE every `with_immediate`
        // write-tx (invariant #1).  `inline::run` establishes the inbox lease
        // (NEW→PROCESSING) and owns the inbox-terminalise obligation + the
        // fail-safe replay semantics (audited in the A2.4 Phase-1 arm-table).
        let gate = self.app.acquire_fn_gate(fn_id).await;
        inline::run(
            self.app.db(),
            self.app.db_secure(),
            binding.dps.as_ref(),
            &binding.sign_ctx,
            &fn_sign,
            &gate,
            row,
        )
        .await
    }
}
