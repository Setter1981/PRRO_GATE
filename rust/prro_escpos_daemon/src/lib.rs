//! Universal ESC/POS print daemon.
//!
//! Exposes [`prro_escpos`] as an HTTP JSON service any client can
//! consume — Python, 1С, Node, curl.  Stateless request-response, no
//! session affinity.
//!
//! ## Endpoints
//!
//! - `GET  /health`          — liveness + build info
//! - `GET  /profiles`        — list loaded vendor profiles
//! - `POST /compile`         — JSON instructions → ESC/POS bytes (hex)
//! - `POST /print`           — compile + send over TCP:9100
//!
//! ## Design
//!
//! - **Stateless**: every request carries full job spec; no shared
//!   state between calls beyond profile registry.
//! - **Declarative**: client sends semantic instructions (Text /
//!   Align / Feed / Cut / Raw / ...), daemon maps to vendor-specific
//!   bytes via the profile dictionary.
//! - **Transport-agnostic**: `POST /compile` returns bytes for any
//!   client-owned transport; `POST /print` encapsulates the common
//!   "TCP:9100" path.  Serial / USB — next step.
pub mod api;
pub mod config;
pub mod registry;
pub mod transport;

pub use registry::ProfileRegistry;
