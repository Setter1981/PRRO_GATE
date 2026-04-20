//! Admin HTTP API for the `maria304_driver` process.
//!
//! Exposed surface (per plan §M10):
//!
//! * `GET /admin/fns`       — list configured FN listeners + status.
//! * `GET /admin/sessions`  — active session summaries.
//! * `GET /admin/metrics`   — aggregate metrics snapshot.
//! * `GET /admin/health`    — 200/OK liveness probe.
//!
//! All requests require a bearer token (`Authorization: Bearer
//! <token>`).  The token is configured once at server start and
//! compared with constant-time equality.  No cookies, no CSRF —
//! this is a machine-to-machine ops API, not user-facing.

pub mod registry;
pub mod server;

pub use registry::{FnSnapshot, MetricsSummary, Registry, SessionSnapshot};
pub use server::{serve_admin, AdminConfig};
