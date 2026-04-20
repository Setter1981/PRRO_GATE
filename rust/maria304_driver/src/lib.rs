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

pub mod protocol;
pub mod wire;
