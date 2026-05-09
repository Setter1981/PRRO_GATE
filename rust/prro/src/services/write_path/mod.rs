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

pub mod stage_acquire;
pub mod types;
