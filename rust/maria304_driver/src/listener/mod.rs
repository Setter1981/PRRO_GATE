//! TCP listener layer.
//!
//! One [`FnListener`] per `fiscal_number`, each bound to its own TCP
//! port.  Real Maria hardware accepts exactly one TCP session at a
//! time; we mirror that via an exclusion gate — a second connect on
//! the same FN gets a `SOFTBLOCK` wire frame and the socket is
//! closed.  After every disconnect, a 3-second cooldown must elapse
//! before a new session is accepted.
//!
//! The listener is pure tokio — no busy waits, no threads outside
//! the runtime.  It owns the [`Bridge`](crate::bridge::Bridge) impl
//! which is shared across all connections via
//! `Arc<dyn Bridge + Send + Sync>`.

pub mod cooldown;
pub mod exclusion;
pub mod server;
pub mod session_loop;

pub use cooldown::Cooldown;
pub use exclusion::ConnectionGate;
pub use server::{FnListener, ListenerConfig, ListenerError};
