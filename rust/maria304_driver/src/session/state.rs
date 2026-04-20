//! Per-connection session state.
//!
//! The dispatcher threads one `Session` through the lifetime of a
//! single TCP connection.  It holds everything the firmware would
//! normally keep in RAM between commands: CRC toggle, logged-in
//! cashier, system-key position, last opened receipt, and the flag
//! that tells `CONF` responses whether a receipt is in-progress.
//!
//! **No I/O lives in this module.**  The session is a pure data
//! container; the dispatcher chooses how to transform it.  TCP
//! framing lives in M6, the bridge client in M7.

use crate::protocol::{ExpectedCmd, SysKey};

/// Top-level state of the TCP session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    /// TCP accepted, no `UPAS` yet.  Only handshake / identity
    /// commands are accepted here; fiscal operations reply with
    /// `SOFTUPAS`.
    Connected,
    /// Cashier has been authenticated.  Receipts, reports, and
    /// queries are all available.
    Authenticated,
    /// A fiscal receipt is currently open (after `PREP`, before
    /// `COMP` or `CANC`).  The `OpenReceipt` placeholder here will
    /// be replaced by the full `Receipt` struct in M4; for M3 we
    /// only need to know *that* a receipt is open.
    ReceiptOpen(OpenReceipt),
}

/// Placeholder for the in-flight receipt buffer.  M3 only tracks
/// presence/absence; M4 extends this with goods, payments, slips,
/// excise stamps, dual-tax mode, etc.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OpenReceipt {
    /// Department name from `PREP`.
    pub department: String,
    /// Optional return-check number staged by `SetReturnCheckNumber`
    /// (`BCHN` on the wire) before `PREP`.
    pub return_check_number: Option<String>,
}

/// Everything the session needs to answer a command.
#[derive(Debug, Clone)]
pub struct Session {
    /// Session lifecycle.
    pub state: SessionState,
    /// CRC toggle — starts `false`, set `true` after `CSIN1`.  All
    /// outgoing frames and incoming decodes observe this flag.
    pub crc_enabled: bool,
    /// Logged-in cashier id (first 4 chars kept for `CONF`).
    pub cashier_id: Option<String>,
    /// Virtual system-key position — updated by `SVSL`.
    pub sys_key: SysKey,
    /// Opcode of the last successfully-executed command.  Reflected
    /// in `CONF` so 1C can do its own consistency checks.
    pub last_command_id: String,
    /// Return-check number staged by `BCHN` before the next `PREP`.
    pub pending_return_check_number: Option<String>,
    /// Number of `PSDt` (acquirer slip) frames received in the
    /// current receipt.  Used to compute the `n` field of the next
    /// slip.  Resets on COMP / CANC.
    pub psdt_sequence: u8,
}

impl Session {
    /// Create a fresh session — no login, no CRC, no cashier.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: SessionState::Connected,
            crc_enabled: false,
            cashier_id: None,
            sys_key: SysKey::Work,
            last_command_id: "    ".to_string(), // 4 spaces — displayed in CONF before any cmd
            pending_return_check_number: None,
            psdt_sequence: 0,
        }
    }

    /// Whether the cashier has successfully completed `UPAS`.
    #[must_use]
    pub fn cashier_registered(&self) -> bool {
        self.cashier_id.is_some()
    }

    /// True when `PREP` has run and `COMP`/`CANC` has not.
    #[must_use]
    pub fn receipt_open(&self) -> bool {
        matches!(self.state, SessionState::ReceiptOpen(_))
    }

    /// Value for the "expected next command" field of `CONF`.  The
    /// firmware uses this so external apps can recover after a crash.
    #[must_use]
    pub fn expected_cmd(&self) -> ExpectedCmd {
        match self.state {
            SessionState::ReceiptOpen(_) => ExpectedCmd::CloseReceipt,
            _ => ExpectedCmd::Idle,
        }
    }

    /// Record that a command was executed successfully.  `opcode`
    /// must be ASCII and exactly 4 chars long — the CONF serializer
    /// pads shorter values with spaces.
    pub fn mark_command_ok(&mut self, opcode: &str) {
        let mut padded = String::with_capacity(4);
        for ch in opcode.chars().take(4) {
            padded.push(ch);
        }
        while padded.chars().count() < 4 {
            padded.push(' ');
        }
        self.last_command_id = padded;
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_session_starts_disconnected_and_cashierless() {
        let s = Session::new();
        assert_eq!(s.state, SessionState::Connected);
        assert!(!s.crc_enabled);
        assert!(!s.cashier_registered());
        assert!(!s.receipt_open());
        assert_eq!(s.expected_cmd(), ExpectedCmd::Idle);
        assert_eq!(s.last_command_id, "    "); // exactly 4 spaces
    }

    #[test]
    fn cashier_registered_tracks_cashier_id_presence() {
        let mut s = Session::new();
        assert!(!s.cashier_registered());
        s.cashier_id = Some("csh1".to_string());
        assert!(s.cashier_registered());
    }

    #[test]
    fn receipt_open_true_only_while_in_receipt_state() {
        let mut s = Session::new();
        assert!(!s.receipt_open());
        s.state = SessionState::Authenticated;
        assert!(!s.receipt_open());
        s.state = SessionState::ReceiptOpen(OpenReceipt::default());
        assert!(s.receipt_open());
    }

    #[test]
    fn expected_cmd_changes_with_receipt_open_flag() {
        let mut s = Session::new();
        assert_eq!(s.expected_cmd(), ExpectedCmd::Idle);
        s.state = SessionState::Authenticated;
        assert_eq!(s.expected_cmd(), ExpectedCmd::Idle);
        s.state = SessionState::ReceiptOpen(OpenReceipt::default());
        assert_eq!(s.expected_cmd(), ExpectedCmd::CloseReceipt);
    }

    #[test]
    fn mark_command_ok_pads_short_opcodes_to_four_chars() {
        let mut s = Session::new();
        s.mark_command_ok("UP");
        assert_eq!(s.last_command_id, "UP  ");
        s.mark_command_ok("PREP");
        assert_eq!(s.last_command_id, "PREP");
    }

    #[test]
    fn mark_command_ok_truncates_long_opcodes_to_four_chars() {
        let mut s = Session::new();
        s.mark_command_ok("PREPOSTEROUS");
        assert_eq!(s.last_command_id, "PREP");
    }

    #[test]
    fn open_receipt_default_has_empty_department_and_no_return() {
        let r = OpenReceipt::default();
        assert_eq!(r.department, "");
        assert_eq!(r.return_check_number, None);
    }

    #[test]
    fn session_default_matches_new() {
        let a = Session::default();
        let b = Session::new();
        assert_eq!(a.state, b.state);
        assert_eq!(a.crc_enabled, b.crc_enabled);
        assert_eq!(a.cashier_id, b.cashier_id);
        assert_eq!(a.last_command_id, b.last_command_id);
    }
}
