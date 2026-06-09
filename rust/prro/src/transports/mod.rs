//! External transport clients.
//!
//! M2 starts with the DPS gRPC channel. Public transport APIs must stay free of
//! database handles per ADR-M2-6; persistence orchestration belongs in services.

pub mod dps;
