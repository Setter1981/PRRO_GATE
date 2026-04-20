//! Typed layer on top of the wire codec.
//!
//! Consumes [`crate::wire::Frame`] values, produces typed
//! [`Command`](commands::Command) inputs for the session layer, and
//! turns high-level outcomes back into wire-ready
//! [`Response`](responses::Response) values.  Two specialised builders
//! cover the two most-important multi-segment responses:
//!
//!   * [`comp_response::CompBuilder`] — 90-char `COMP` body emitted after
//!     every receipt close.
//!   * [`identity::ConfBuilder`] — 148-char `CONF` / `CONf` body emitted
//!     whenever 1C polls device identity + runtime state.
//!
//! Error codes are catalogued in [`error_codes::ErrorCode`].

pub mod commands;
pub mod comp_response;
pub mod error_codes;
pub mod identity;
pub mod responses;

pub use commands::Command;
pub use comp_response::CompBuilder;
pub use error_codes::ErrorCode;
pub use identity::{ConfBuilder, ConfMode, ExpectedCmd, SysKey};
pub use responses::Response;
