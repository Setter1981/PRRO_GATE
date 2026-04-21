//! Takes a [`PrinterProfile`] and resolves semantic command names
//! into raw ESC/POS byte sequences.
//!
//! The executor is stateless — each call returns the bytes that would
//! be emitted for that specific command.  The [`ReceiptCompiler`]
//! threads calls together into a full receipt.

use crate::error::{Error, Result};
use crate::profile::PrinterProfile;

pub struct Executor<'a> {
    pub profile: &'a PrinterProfile,
}

impl<'a> Executor<'a> {
    pub fn new(profile: &'a PrinterProfile) -> Self {
        Self { profile }
    }

    /// Emit bytes for a command that takes no argument (e.g. `Left`,
    /// `Center`, `FONTDEFAULT`).
    pub fn simple(&self, name: &str) -> Result<Vec<u8>> {
        let cmd = self.profile.command(name)
            .ok_or_else(|| Error::UnknownCommand(name.to_string()))?;
        cmd.base_bytes()
    }

    /// Emit bytes for an enum-style command, selecting the `value`
    /// branch by name (e.g. `with_value("CodePage", "866")`).  Replaces
    /// the byte at the value's declared offset with its resolved byte.
    pub fn with_value(&self, command: &str, value_name: &str) -> Result<Vec<u8>> {
        let cmd = self.profile.command(command)
            .ok_or_else(|| Error::UnknownCommand(command.to_string()))?;
        let mut bytes = cmd.base_bytes()?;
        let val = cmd.value_by_name(value_name)
            .ok_or_else(|| Error::UnknownValue {
                command: command.to_string(),
                value: value_name.to_string(),
            })?;
        let byte = val.byte()?;
        let idx = val.byte_offset as usize;
        if idx < bytes.len() {
            bytes[idx] = byte;
        } else {
            // Base template shorter than the value offset — pad with
            // zeroes and set.  This matches what the SoftBalance
            // reference driver does for commands whose `length > 3`.
            bytes.resize(idx + 1, 0);
            bytes[idx] = byte;
        }
        Ok(bytes)
    }

    /// Emit bytes for an enum-style command where the caller supplies
    /// a raw byte directly (e.g. `FeedingThePaperByNLines` with N=4).
    ///
    /// SoftBalance XML convention: the last byte of `code_hex` is the
    /// placeholder slot (`00`) which the runtime replaces with the
    /// supplied argument.  Example: `code="1B6400"` emits `ESC d N`,
    /// where we replace byte[2] with `N`.
    pub fn with_byte(&self, command: &str, byte: u8) -> Result<Vec<u8>> {
        let cmd = self.profile.command(command)
            .ok_or_else(|| Error::UnknownCommand(command.to_string()))?;
        let mut bytes = cmd.base_bytes()?;
        // If the command declared a default value byte-offset, honor it.
        if let Some(idx) = cmd.values.first().map(|v| v.byte_offset as usize) {
            if idx >= bytes.len() {
                bytes.resize(idx + 1, 0);
            }
            bytes[idx] = byte;
        } else if !bytes.is_empty() {
            // No explicit <value>: override the last byte, which is the
            // conventional placeholder slot in this profile format.
            *bytes.last_mut().unwrap() = byte;
        } else {
            bytes.push(byte);
        }
        Ok(bytes)
    }
}
