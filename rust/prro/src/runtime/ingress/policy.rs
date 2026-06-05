//! RS-2 piece-1b — ingress command-class policy (§0.4 H3 + M6).
//!
//! Classifies a wire [`CommandType`] at the ingress boundary, BEFORE any
//! fiscal-inbox write, into three buckets so unsupported / read-only ops
//! fail (or divert) EARLY here instead of late at `stage_sign`
//! ([`derive_wire_artifact_kind`] fails-closed on
//! `Service*`/`CashWithdrawal`/`XReport` only after a `DocType` already
//! exists and an inbox row may have been written).
//!
//! §0.4 M6: the policy keys off [`CommandType`], NOT `DocType` —
//! `PeriodicReport` is a `CommandType` (rejected at the mapper before a
//! `DocType` exists), so a `DocType`-keyed policy could never see it.
//!
//! [`derive_wire_artifact_kind`]: crate::services::write_path::stage_sign::derive_wire_artifact_kind

use super::dto::CommandType;

/// Boundary classification of a wire command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandClass {
    /// Signable fiscal document — proceed to convert → inbox → seam.
    /// Mirrors `stage_sign::derive_wire_artifact_kind`'s signable set
    /// (`ShiftOpen`/`Sell`/`Return`/`ShiftClose`/`ZReport`).
    Signable,

    /// Read-only / status op (`X_REPORT`).  Per `docs/LEGAL_INVARIANTS.md`
    /// (X_REPORT) it must NOT persist / sign / advance LND / consume a
    /// code — so it MUST NOT enter the fiscal inbox.  Served via the
    /// read-only `GET /v1/status` surface or rejected as read-only;
    /// never fiscalized.
    ReadOnly,

    /// Not supported by the RS-2 fiscal pipeline — typed rejection
    /// (HTTP 422) BEFORE any inbox write.  Covers the cash-movement ops
    /// the signer cannot build (`SERVICE_IN` / `SERVICE_OUT` /
    /// `CASH_WITHDRAWAL`) and `PERIODIC_REPORT` (a `CommandType` the
    /// mapper already rejects).
    Unsupported,
}

/// Classify a wire [`CommandType`] at the ingress boundary.
///
/// **Exhaustive match (no wildcard):** adding a `CommandType` variant is
/// a compile error here, forcing an explicit policy decision rather than
/// a silent default into one bucket.
pub fn classify_command(cmd: CommandType) -> CommandClass {
    match cmd {
        CommandType::Sell
        | CommandType::Return
        | CommandType::ShiftOpen
        | CommandType::ShiftClose
        | CommandType::ZReport => CommandClass::Signable,

        CommandType::XReport => CommandClass::ReadOnly,

        CommandType::ServiceIn
        | CommandType::ServiceOut
        | CommandType::CashWithdrawal
        | CommandType::PeriodicReport => CommandClass::Unsupported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The signable bucket is exactly `stage_sign::derive_wire_artifact_kind`'s
    /// signable set — these proceed to convert → inbox → seam.
    #[test]
    fn signable_set_matches_signer() {
        for c in [
            CommandType::Sell,
            CommandType::Return,
            CommandType::ShiftOpen,
            CommandType::ShiftClose,
            CommandType::ZReport,
        ] {
            assert_eq!(classify_command(c), CommandClass::Signable, "{c:?}");
        }
    }

    /// `X_REPORT` is read-only (LEGAL X_REPORT) — never fiscalized.
    #[test]
    fn xreport_is_read_only() {
        assert_eq!(
            classify_command(CommandType::XReport),
            CommandClass::ReadOnly
        );
    }

    /// Cash-movement ops + `PERIODIC_REPORT` are typed-unsupported and
    /// must be rejected BEFORE any inbox write.
    #[test]
    fn cash_movement_and_periodic_are_unsupported() {
        for c in [
            CommandType::ServiceIn,
            CommandType::ServiceOut,
            CommandType::CashWithdrawal,
            CommandType::PeriodicReport,
        ] {
            assert_eq!(classify_command(c), CommandClass::Unsupported, "{c:?}");
        }
    }
}
