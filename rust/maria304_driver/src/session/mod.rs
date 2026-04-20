//! Per-connection session layer.
//!
//! M3 lands only the synchronous state machine — no TCP (that is M6),
//! no Python bridge (that is M7), no receipt buffer (that is M4).
//! The dispatcher is a pure function that takes an incoming
//! [`Command`] + current [`Session`] and returns the vector of
//! [`Response`] frames to hit the wire.

pub mod dispatcher;
pub mod state;

pub use dispatcher::{dispatch, Clock, Identity};
pub use state::{OpenReceipt, Session, SessionState};
