//! Maria 304 fiscal register wire-protocol emulator.
//!
//! This crate implements the TCP wire protocol spoken by genuine
//! Ekka "Maria-304" cash registers so that accounting systems
//! (1C, BAS, etc.) which address the Resonance OLE Manager DLL
//! cannot tell our virtual device apart from real hardware.
//!
//! Sprint layout:
//! * [`wire`]     — M1: frame codec, custom CRC-16, CP866, LLV.
//! * [`protocol`] — M2: typed commands, responses, `COMP`/`CONF`
//!   builders, error-code catalogue.
//! * [`session`]  — M3: per-connection state machine + dispatcher
//!   + stub responses for every non-fiscal opcode.
//! * [`bridge`]   — M4: canonical envelope DTOs + `Bridge` trait
//!   + in-memory `MockBridge` for integration tests.
//! * [`listener`] — M6: tokio TCP listener per `fiscal_number` +
//!   single-connection exclusion gate + 3s cooldown + idle timeout.
//! * [`observability`] — M8: JSON tracing subscriber helper,
//!   per-session frame-trace ring buffer, atomic metrics counters.

pub mod bridge;
pub mod listener;
pub mod observability;
pub mod protocol;
pub mod session;
pub mod wire;
