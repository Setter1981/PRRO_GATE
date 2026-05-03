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

str_enum!(ShiftState {
    Created => "CREATED",
    Opening => "OPENING",
    Opened  => "OPENED",
    Closing => "CLOSING",
    Closed  => "CLOSED",
    Error   => "ERROR",
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
