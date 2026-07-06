//! HTTP ingress shell (M4 W1/W3 + RS-2).
//!
//! Layers, bottom-up:
//!   - [`dto`] — crate-local copy of the `maria304_driver::bridge::dto` wire
//!     DTOs + the `to_canonical_fiscal_command*` mapping helpers.
//!   - [`convert`] — wire→signer payload conversion (RS-2 piece-2).
//!   - [`policy`] — command-class classification (RS-2 piece-1b).
//!   - [`seam`] — the write-path `WritePathEntry` seam (RS-2 piece-3).
//!   - [`replay`] — the idempotency replay/conflict resolver (RS-2 piece-4b).
//!   - [`handler`] — the axum-free `handle_command` core (RS-2 piece-5a).
//!   - [`server`] — the per-listener axum HTTP server wrapping `handle_command`
//!     (RS-2 piece-5b-ii).
//!   - [`preflight`] — D1 frozen-slot startup validation (RS-2 piece-5b-ii).

pub mod canonical_builder;
pub mod convert;
pub mod dto;
pub mod handler;
pub mod inline_binding;
pub mod policy;
pub mod preflight;
pub mod replay;
pub mod seam;
pub mod server;
pub mod z_builder;
