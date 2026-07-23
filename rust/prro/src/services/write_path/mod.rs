//! Write-path pipeline (M3a stages 1–5).
//!
//! M3a W5 lands stages 1+2 (acquire+validate+guard) per ADR-M3-A1,
//! ADR-M3-A5, ADR-M3-A7 and W0-1 §3.1–§3.2.  Subsequent stages live
//! in their own M3a tasks:
//!   - W6: stage 3 sign (Pattern A)
//!   - W7: stage 4 send (Pattern B + SENDING)
//!   - W8: stage 5 finalize
//!   - W9: App::boot reconciliation
//!   - W10: DpsError routing dispatch
//!   - W11: cross-stage deterministic-replay gate

/// CS-3 S7-1 — shared apply orchestration: `apply_outcome` + online shift edges 3/10.
pub mod apply_orchestration;
pub mod auto_z;
pub mod dispatch;
pub mod error_routing;
pub mod inline;
pub(crate) mod inline_map;

pub use auto_z::{run_auto_z_if_over_limit, AutoZOutcome};
pub mod kvt2_advance;
pub mod mac_recovery;
/// CS-3 3.2 PR4 — the §4.4-gated engine mapper (`RawSendReply` + doc_type → `SendResponse`).
pub(crate) mod shadow_map;
pub mod signer_guard;
pub mod stage_acquire;
pub mod stage_finalize;
pub mod stage_offline_ack;
pub mod stage_send;
pub mod stage_sign;
/// CS-3 S7-1 — the sole wire function: `Authorization` → one `send_chk_observed` → `AttemptObservation`.
pub mod submit;
pub mod tax_summary;
pub mod types;
