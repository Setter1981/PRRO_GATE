//! Pure state-machine and protocol enums (CS-1b).
//!
//! These moved out of `prro::db::models::enums` into the sqlx-free domain
//! crate. They are the **canonical** definitions; `prro::db::models::enums`
//! re-exports them verbatim behind a compatibility shim (contract §5), so every
//! legacy path (`prro::db::models::enums::DocState`, …) resolves unchanged.
//!
//! **Storage non-event (contract §2).** Each variant keeps its exact pre-move
//! TEXT literal (`as_str()`) and byte-identical serde output (`#[serde(rename =
//! $sql)]`). The SQLite `Type`/`Encode`/`Decode` mapping does NOT live here
//! (this crate is sqlx-free) — it lives in the store-side `prro::db::types`
//! wrappers (`DbDocState`, …), which delegate encode to `as_str()` and decode
//! through [`from_sql_str`]-style parsing.
//!
//! `InboxStatus` is deliberately **NOT** here — it stays in `prro` with its own
//! sqlx-bearing `str_enum!` derive (contract §2/§10; its domain-vs-store home is
//! decided in spec #3).

use serde::{Deserialize, Serialize};

/// Define a pure (sqlx-free) TEXT-literal enum.
///
/// This is the baseline `prro::db::models::enums::str_enum!` **minus** the
/// `sqlx::Type` derive and `#[sqlx(...)]` attributes. It keeps the derive set,
/// the per-variant `#[serde(rename = $sql)]` (byte-identical serde), `as_str()`,
/// and ADDS `from_sql_str(&str) -> Option<Self>` (exact-literal match; an
/// unknown literal ⇒ `None`, closed set) — the parse half that the store-side
/// `Decode` wrapper delegates to.
macro_rules! str_enum {
    ($name:ident { $( $variant:ident => $sql:literal ),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum $name {
            $(
                #[serde(rename = $sql)]
                $variant,
            )+
        }

        impl $name {
            /// The stored TEXT literal for this variant (byte-identical to the
            /// pre-move representation).
            pub fn as_str(&self) -> &'static str {
                match self { $( Self::$variant => $sql, )+ }
            }

            /// Parse a stored TEXT literal back into the variant.
            ///
            /// Exact-literal match against the closed set; an unknown literal
            /// yields `None`. This is the pure parse half the store-side
            /// `Decode` wrapper (`prro::db::types`) uses to reconstruct the
            /// domain enum, turning an unknown literal into a decode error.
            pub fn from_sql_str(s: &str) -> Option<Self> {
                match s {
                    $( $sql => Some(Self::$variant), )+
                    _ => None,
                }
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
    // Non-issued TERMINAL for an operation refused AFTER stage_sign (the doc
    // reached PREPARED/SIGNED but DPS/precondition refused issuance before any
    // fiscal number was assigned).  Restores the ledger-only pin: a post-sign
    // refusal lands here instead of orphaning a non-terminal SIGNED row.
    // offline_fiscal_no stays NULL (never issued).  Migration 025 adds it to the
    // fiscal_documents.state CHECK.
    Aborted                      => "ABORTED",
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
    // EPZ — видача готівки за ЕПЗ (cash advance / cashback against a card).
    // Distinct from CashWithdrawal (the fail-closed placeholder) so the
    // ledger / z-quiescence / aggregation filters stay unambiguous.  DPS wire
    // = compact `<C T='8'>` (StringXML.cs num17=abs(-8)=8), NOT verbose
    // operationtype='-8' (that is WebCheck's COM-input form).
    CashAdvanceEpz => "CASH_ADVANCE_EPZ",
    XReport        => "X_REPORT",
    ZReport        => "Z_REPORT",
    // B10 — offline-session drain-handshake boundary docs.  Gateway-INTERNAL
    // only (never built from an external ingress protocol; the ingress
    // string/CommandType gates fail-closed on them).  Wire `<C T="109">`
    // (BEGIN) / `<C T="110">` (END), typCheck ServiceChk(3), offline docs.
    OfflineSessionBegin => "OFFLINE_SESSION_BEGIN",
    OfflineSessionEnd   => "OFFLINE_SESSION_END",
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
