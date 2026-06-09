//! Service-layer modules.
//!
//! Per ADR-M2-6, `services::*` MAY take a `SqlitePool` because their job
//! is to orchestrate DB writes around crypto + transport calls.  The
//! `crypto` and `transports` modules themselves do NOT see DB handles —
//! W5's static check enforces that boundary at build time.

pub mod cert_refresher;
pub mod offline_session;
pub mod offline_sync;
pub mod reconciliation;
pub mod shift;
pub mod write_path;
