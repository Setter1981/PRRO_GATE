//! Maria 304 fiscal register wire-protocol emulator.
//!
//! This crate implements the TCP wire protocol spoken by genuine
//! Ekka "Maria-304" cash registers so that accounting systems
//! (1C, BAS, etc.) which address the Resonance OLE Manager DLL
//! cannot tell our virtual device apart from real hardware.
//!
//! Sprint M1 — wire layer only:
//!   * [`wire::codec`] frame encode/decode
//!   * [`wire::crc`]   custom CRC-16 (Maria polynomial)
//!   * [`wire::cp866`] CP866 encoding bridge
//!   * [`wire::llv`]   length-prefixed string encoder

pub mod wire;
