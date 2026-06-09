//! sqlx::Type wrappers for state-machine and protocol enums.
//!
//! Stored as TEXT in SQLite (matches the CHECK lists in migrations),
//! deserialized into typed Rust enums in repository layer.

use serde::{Deserialize, Serialize};
use sqlx::Type;

macro_rules! str_enum {
    ($name:ident { $( $variant:ident => $sql:literal ),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
        #[sqlx(type_name = "TEXT")]
        pub enum $name {
            $(
                #[sqlx(rename = $sql)]
                #[serde(rename = $sql)]
                $variant,
            )+
        }

        impl $name {
            pub fn as_str(&self) -> &'static str {
                match self { $( Self::$variant => $sql, )+ }
            }
        }
    };
}

str_enum!(DocState {
    Prepared                     => "PREPARED",
    Signed                       => "SIGNED",
    Encrypted                    => "ENCRYPTED",
    // Pattern B intent-marker (ADR-M3-A9 step 2).  Stored after a
    // successful CAS Signed->Sending or Encrypted->Sending and BEFORE
    // the wire send; the recovery rule (Sending->ErrorRetryable on
    // boot, ZERO send_chk invocations) prevents duplicate fiscalisation
    // because DPS does not deduplicate.
    Sending                      => "SENDING",
    Sent                         => "SENT",
    Kvt1                         => "KVT1",
    Kvt2                         => "KVT2",
    Ack                          => "ACK",
    OfflineLocalAck              => "OFFLINE_LOCAL_ACK",
    Rejected                     => "REJECTED",
    Cancelled                    => "CANCELLED",
    ErrorRetryable               => "ERROR_RETRYABLE",
    RequiresManualReconciliation => "REQUIRES_MANUAL_RECONCILIATION",
});

// M3b W5 — OfflineSession state machine vocabulary, aligned with
// migration 015's CHECK constraint on `offline_sessions.state`.
// Whitelist + transition semantics live in
// `db::repositories::offline_sessions`.  See M3b plan §Task 5.
str_enum!(OfflineSessionState {
    Opening  => "OPENING",
    Open     => "OPEN",
    Draining => "DRAINING",
    Closed   => "CLOSED",
    Aborted  => "ABORTED",
});

str_enum!(ShiftState {
    Created                      => "CREATED",
    Opening                      => "OPENING",
    OpenedLocalPendingDrain      => "OPENED_LOCAL_PENDING_DRAIN",    // M3b W14a-1: offline-open Pattern C destination
    Opened                       => "OPENED",
    ClosingLocalPendingDrain     => "CLOSING_LOCAL_PENDING_DRAIN",   // M3b W14a-1: offline-close Pattern C destination
    Closing                      => "CLOSING",
    Closed                       => "CLOSED",
    RequiresManualReconciliation => "REQUIRES_MANUAL_RECONCILIATION",// M3b W14a-1: drain-reject terminal (per spec §16.7)
    Error                        => "ERROR",
});

str_enum!(NodeMode {
    Online         => "ONLINE",
    GoingOffline   => "GOING_OFFLINE",
    Offline        => "OFFLINE",
    GoingOnline    => "GOING_ONLINE",
    Blocked        => "BLOCKED",
    StopMode       => "STOP_MODE",
    CryptoDegraded => "CRYPTO_DEGRADED",
});

str_enum!(Protocol {
    Rest           => "REST",
    XmlRpc         => "XMLRPC",
    Maria          => "MARIA",
    Maria304       => "MARIA304",
    CheckboxCompat => "CHECKBOX_COMPAT",
    Internal       => "INTERNAL",
});

str_enum!(DocType {
    ShiftOpen      => "SHIFT_OPEN",
    ShiftClose     => "SHIFT_CLOSE",
    Sell           => "SELL",
    Return         => "RETURN",
    ServiceIn      => "SERVICE_IN",
    ServiceOut     => "SERVICE_OUT",
    CashWithdrawal => "CASH_WITHDRAWAL",
    XReport        => "X_REPORT",
    ZReport        => "Z_REPORT",
});

str_enum!(FiscalMode {
    Test => "test",
    Prod => "prod",
});

str_enum!(Severity {
    Info     => "INFO",
    Warning  => "WARNING",
    Error    => "ERROR",
    Critical => "CRITICAL",
});

str_enum!(InboxStatus {
    New        => "NEW",
    Processing => "PROCESSING",
    Done       => "DONE",
    Rejected   => "REJECTED",
    Error      => "ERROR",
});
