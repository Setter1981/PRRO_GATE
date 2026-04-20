//! Wire-layer primitives for Maria 304 protocol.
//!
//! # Framing (from decompiled `Resonance.EKKR.Message.GetBytes`)
//!
//! Direction — both VU→EKKR and EKKR→VU use the identical shape:
//!
//! ```text
//! [0xFD] [cmd_bytes (CP866)] [len = cmd_bytes.len()+1] [0xFE] [crc_lo crc_hi]?
//! ```
//!
//! Invariants:
//!   * Start byte = `0xFD` (253)
//!   * End byte   = `0xFE` (254)
//!   * Payload bytes `0xFE` / `0xFF` inside the command are **replaced
//!     with `0x20`** (space) on encode — they can never appear in the
//!     data region.
//!   * Length byte = `payload.len() + 1`, range `0x05..=0xFD`.
//!   * Command length = 4..=252 bytes (from genuine code path).
//!   * CRC is optional until the peer sends `CSIN1`; after that both
//!     sides prepend CRC to every frame.  CRC-16 uses a Maria-specific
//!     polynomial (see [`crc`]).
//!
//! # Encoding
//!
//! Cyrillic text (receipt item names, operator names, department
//! labels) is CP866 — specifically the "alternative Russian" variant
//! where the upper half matches IBM OEM 866.  Ukrainian glyphs (`і`,
//! `ї`, `є`, `ґ`) use explicit mappings that differ from both Windows-1251
//! and raw PC-866; see [`cp866`].

pub mod codec;
pub mod cp866;
pub mod crc;
pub mod llv;

pub use codec::{decode_frame, encode_frame, encode_frame_bytes, Frame, FrameError};
pub use crc::crc16;
pub use llv::Llv;
