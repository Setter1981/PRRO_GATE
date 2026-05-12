//! W11 runtime composition surface for boot reconciliation.
//!
//! `ReconciliationRuntime` carries the dependencies that ctx-needy
//! dispatch branches will need to drive PREPARED / SIGNED / SENT /
//! ERROR_RETRYABLE docs forward on boot.  Plumbed in W11 PR-1a;
//! consumed by PR-1b (KVT2 — though KVT2 itself does not touch deps,
//! the parameter exists for symmetry) and PR-2 (the four ctx-needy
//! arms).
//!
//! Per ADR-M3-A10 (`docs/superpowers/specs/2026-05-12-adr-m3-a10-global-single-writer.md`):
//! under M3a's global-single-writer invariant, this runtime is held
//! for the duration of one `App::reconcile_pending_with` call; the
//! invariant remains "at most one writer mutates state for a given
//! `fiscal_number` at any moment", enforced by single tokio worker +
//! `with_immediate` BEGIN IMMEDIATE + per-row CAS.  Concurrent
//! dispatchers are not supported in M3a.

use crate::services::write_path::stage_sign::SigningContext;
use crate::transports::dps::dto::CheckSignBlob;
use crate::transports::dps::DpsChannel;

/// Bundle of runtime dependencies for ctx-needy boot dispatch.
///
/// Lifetime `'a` is the caller-bound lifetime of the borrowed channel,
/// signing context, and DPS identity blob.  The runtime is **not**
/// `Send`/`Sync` — `App::reconcile_pending_with` consumes it on the
/// dispatcher task (single-worker model, per ADR-M3-A10).
pub struct ReconciliationRuntime<'a> {
    /// DPS wire channel used by recovery branches that re-drive docs
    /// to the wire.  PR-1a plumbs this dep through `run_boot_reconciliation`
    /// and `dispatch_pending_doc`; the SENDING branch does NOT consult
    /// `dps` (Pattern B no-resend — structurally enforced by the
    /// ctx-free `resume_sending_to_error_retryable` helper).  KVT1
    /// passive hold and KVT2 finalize also do not consult `dps`.  PR-2
    /// wires `dps` into the four ctx-needy branches.
    pub dps: &'a dyn DpsChannel,
    /// Signing context (DSTU 4145-2002 + GOST 34.311 — per
    /// `prro_crypto`).  Required by re-sign code paths that PR-2 wires
    /// into PREPARED / SIGNED branches.  Not consulted by PR-1a's
    /// SENDING branch.
    pub signing_ctx: &'a SigningContext,
    /// Operator DPS identity blob (opaque bytes — the same blob the
    /// live worker holds for `DpsChannel::last_chk` / `status_rro` /
    /// `info_rro` calls).
    ///
    /// W11 PR-2 (SENT branch) consumes `fn_sign` to drive
    /// `last_chk_probe::probe` during SENT crash-recovery — the
    /// probe's `match | mismatch | not_found` classification is the
    /// canonical 3-way dispatch per W0-3 §6.4 + ADR-M3-A9 retry-path.
    /// Live runtime owns this blob via the operator's keystore; tests
    /// inject dummy bytes since the stub channel does not verify the
    /// blob's contents.
    pub fn_sign: &'a CheckSignBlob,
}
