//! W9 boot reconciliation — per-FN decision tree dispatch.
//!
//! Module surface (per design freeze
//! `docs/superpowers/specs/2026-05-10-m3a-w9-boot-reconciliation-design.md`):
//!
//! - [`boot_phase`] — 6-branch dispatch (W9.3) + per-DocState helpers
//!   (`resume_sending_to_error_retryable`, `advance_sent_to_kvt1_from_probe`,
//!   `passive_hold_kvt1`).  W9.2 lands the helpers; W9.3 lands
//!   `run_boot_reconciliation` decision tree.
//! - [`last_chk_probe`] — SENT recovery probe (`DpsChannel::last_chk` +
//!   3-way classification).  W9.2 lands outcome classification; W9.3
//!   wires into branch (c)'s Sent dispatch.
//!
//! Active KVT2 polling (Kvt1 → Kvt2 advance) is **deferred to M3b**
//! per freeze HIGH 6 (option A); W9 uses `boot_phase::passive_hold_kvt1`
//! to emit a forensic audit row without state mutation.
//!
//! [`runtime`] (W11) — `ReconciliationRuntime<'a>` bundle that
//! `App::reconcile_pending_with` accepts; threaded through
//! `run_boot_reconciliation` and `dispatch_pending_doc` so PR-2 can
//! wire ctx-needy dispatch arms (PREPARED / SIGNED / SENT /
//! ERROR_RETRYABLE) without re-touching the dispatch tree shape.

pub mod boot_phase;
pub mod er_redrive_policy;
pub mod guard;
pub mod inbox_reaper;
pub mod last_chk_probe;
pub mod online_convergence;
pub mod operator_completion;
pub mod reservation_boot_pass;
pub mod runtime;
pub mod sent_not_found;

pub use guard::ReconcileGuard;
pub use runtime::{ReconciliationRuntime, RuntimeView};
