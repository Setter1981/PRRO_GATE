//! Compatibility facade for the state-machine and protocol enums.
//!
//! **CS-1b (contract §5):** the eight TEXT-affinity enums (`DocState`,
//! `OfflineSessionState`, `ShiftState`, `NodeMode`, `Protocol`, `DocType`,
//! `FiscalMode`, `Severity`) moved into the pure `prro-domain` crate. This
//! module re-exports them **explicitly, per-symbol** (NOT `pub use
//! prro_domain::*`) so every legacy path — `prro::db::models::enums::DocState`,
//! … — resolves unchanged. Their SQLite mapping now lives in the store-side
//! `prro::db::types` wrappers (`DbDocState`, …); the domain enums themselves are
//! sqlx-free.
//!
//! `InboxStatus` deliberately **stays here** (contract §2/§10): it keeps its
//! sqlx-bearing `str_enum!` derive; its domain-vs-store home is decided in
//! spec #3, not CS-1.

// Explicit per-symbol facade re-exports (contract §5).
pub use prro_domain::{
    DocState, DocType, FiscalMode, NodeMode, OfflineSessionState, Protocol, Severity, ShiftState,
};

use serde::{Deserialize, Serialize};
use sqlx::Type;

/// Local sqlx-bearing TEXT enum (the baseline macro), retained for the enums
/// that stay in `prro`. Currently only `InboxStatus`.
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

str_enum!(InboxStatus {
    New        => "NEW",
    Processing => "PROCESSING",
    Done       => "DONE",
    Rejected   => "REJECTED",
    Error      => "ERROR",
});
