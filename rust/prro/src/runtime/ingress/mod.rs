//! M4 HTTP ingress shell.
//!
//! Built across the M4 worklets:
//!
//!   - **W1** (PR #96) established the module namespace + axum/tower
//!     deps; [`IngressServer`] is still a no-op stub at this layer.
//!   - **W3** (PR #99) lands [`dto`] — the crate-local copy of the
//!     `maria304_driver::bridge::dto` wire DTOs, plus
//!     `to_canonical_fiscal_command` mapping helper to the write-path's
//!     `CanonicalFiscalCommand`.
//!   - **W5** (TBD) wires the actual axum router + handler that
//!     deserialises [`dto::CanonicalCommand`] and calls the mapping
//!     helper.  `IngressServer::serve` gets a real body then.
//!   - **W7** (TBD) plumbs the per-FN supervisor channel that
//!     receives mapped commands from the handler.

pub mod dto;

pub struct IngressServer;

impl IngressServer {
    pub async fn serve() {}
}
