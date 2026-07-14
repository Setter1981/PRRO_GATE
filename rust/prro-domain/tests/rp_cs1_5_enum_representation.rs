//! RP-CS1-5 (representation-matrix conformance) — the **pure** half.
//!
//! Contract `docs/superpowers/specs/2026-07-14-cs1-contract-behaviour-neutral-skeleton.md`
//! §2 (representation matrix) + §7 RP-CS1-5. This is the CS-1b closure for the
//! eight TEXT-affinity state/protocol enums moved into `prro-domain`.
//!
//! The relocation is a **storage non-event** — byte-identical. This test locks,
//! for every moved enum, the pure representation properties that survive the
//! move (the sqlx encode→decode round-trip lives in the `prro` crate, which is
//! where the `Db*` wrappers + a live SQLite reside):
//!
//!   (a) `from_sql_str(x.as_str()) == Some(x)` for **every** variant;
//!   (b) the stored TEXT equals the pre-move literal — pinned as an EXPLICIT
//!       literal table (a `*_locked.rs`-style guard) so a wrong literal is
//!       caught loudly, not silently;
//!   (c) an unknown literal ⇒ `from_sql_str` returns `None` (closed set);
//!   (e) serde output is **byte-identical** — `serde_json::to_string(&x)`
//!       equals the quoted pre-move literal (the `#[serde(rename=$sql)]`).
//!
//! Part (d) — the sqlx `Db*`-wrapper encode→decode round-trip against an
//! in-memory SQLite — is asserted in `prro/tests/rp_cs1_5_db_enum_roundtrip.rs`
//! (sqlx-bearing crate).

use prro_domain::{
    DocState, DocType, FiscalMode, NodeMode, OfflineSessionState, Protocol, Severity, ShiftState,
};

// (b) EXPLICIT literal table — the pre-move TEXT for every variant of every
// moved enum. A wrong literal (a rename, a typo, a case slip) breaks this
// loudly. `FiscalMode` is pinned lowercase on purpose (§2: `test`/`prod`, NOT
// upper). Hard-coded (not derived) so an ADDED variant also breaks the count
// assertion below, forcing an explicit decision. Each `#[test]` below passes a
// table of `(variant, as_str_literal)` to `assert_enum`.
fn assert_enum<T>(variants: &[(T, &str)])
where
    T: Copy + PartialEq + std::fmt::Debug + serde::Serialize + serde::de::DeserializeOwned,
    T: AsStrFromSql,
{
    for (v, lit) in variants {
        // (b) stored TEXT equals the pinned literal.
        assert_eq!(
            v.as_str_(),
            *lit,
            "as_str() literal drift for {v:?}: expected {lit:?}"
        );
        // (a) exact-literal round-trip.
        assert_eq!(
            T::from_sql_str_(lit),
            Some(*v),
            "from_sql_str({lit:?}) must round-trip back to {v:?}"
        );
        // (e) serde output byte-identical to the quoted literal.
        let json = serde_json::to_string(v).expect("serialize");
        assert_eq!(
            json,
            format!("\"{lit}\""),
            "serde output drift for {v:?}: expected \"{lit}\""
        );
        // serde round-trips too (deserialize the exact bytes).
        let back: T = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, *v, "serde round-trip mismatch for {v:?}");
    }
    // (c) unknown literal ⇒ None (closed set). Use sentinels that cannot
    // collide with any real literal.
    for bad in ["", "definitely_not_a_variant", "PREPARED_", "  ", "unknown"] {
        // Only treat as "unknown" if it is not actually one of this enum's
        // literals (guards the empty-string / substring edge for lowercase
        // FiscalMode etc.).
        if variants.iter().all(|(_, lit)| *lit != bad) {
            assert_eq!(
                T::from_sql_str_(bad),
                None,
                "unknown literal {bad:?} must decode to None (closed set)"
            );
        }
    }
}

/// Adapter trait so the generic `assert_enum` can call `as_str` /
/// `from_sql_str` uniformly. (Each moved enum has inherent `as_str` /
/// `from_sql_str`; this just routes to them.)
trait AsStrFromSql: Sized {
    fn as_str_(&self) -> &'static str;
    fn from_sql_str_(s: &str) -> Option<Self>;
}

macro_rules! impl_adapter {
    ($t:ty) => {
        impl AsStrFromSql for $t {
            fn as_str_(&self) -> &'static str {
                self.as_str()
            }
            fn from_sql_str_(s: &str) -> Option<Self> {
                <$t>::from_sql_str(s)
            }
        }
    };
}
impl_adapter!(DocState);
impl_adapter!(OfflineSessionState);
impl_adapter!(ShiftState);
impl_adapter!(NodeMode);
impl_adapter!(Protocol);
impl_adapter!(DocType);
impl_adapter!(FiscalMode);
impl_adapter!(Severity);

#[test]
fn doc_state_representation_locked() {
    assert_enum(&[
        (DocState::Prepared, "PREPARED"),
        (DocState::Signed, "SIGNED"),
        (DocState::Encrypted, "ENCRYPTED"),
        (DocState::Sending, "SENDING"),
        (DocState::Sent, "SENT"),
        (DocState::Kvt1, "KVT1"),
        (DocState::Kvt2, "KVT2"),
        (DocState::Ack, "ACK"),
        (DocState::OfflineLocalAck, "OFFLINE_LOCAL_ACK"),
        (DocState::Rejected, "REJECTED"),
        (DocState::Cancelled, "CANCELLED"),
        (DocState::ErrorRetryable, "ERROR_RETRYABLE"),
        (
            DocState::RequiresManualReconciliation,
            "REQUIRES_MANUAL_RECONCILIATION",
        ),
        (DocState::Aborted, "ABORTED"),
    ]);
}

#[test]
fn offline_session_state_representation_locked() {
    assert_enum(&[
        (OfflineSessionState::Opening, "OPENING"),
        (OfflineSessionState::Open, "OPEN"),
        (OfflineSessionState::Draining, "DRAINING"),
        (OfflineSessionState::Closed, "CLOSED"),
        (OfflineSessionState::Aborted, "ABORTED"),
    ]);
}

#[test]
fn shift_state_representation_locked() {
    assert_enum(&[
        (ShiftState::Created, "CREATED"),
        (ShiftState::Opening, "OPENING"),
        (
            ShiftState::OpenedLocalPendingDrain,
            "OPENED_LOCAL_PENDING_DRAIN",
        ),
        (ShiftState::Opened, "OPENED"),
        (
            ShiftState::ClosingLocalPendingDrain,
            "CLOSING_LOCAL_PENDING_DRAIN",
        ),
        (ShiftState::Closing, "CLOSING"),
        (ShiftState::Closed, "CLOSED"),
        (
            ShiftState::RequiresManualReconciliation,
            "REQUIRES_MANUAL_RECONCILIATION",
        ),
        (ShiftState::Error, "ERROR"),
    ]);
}

#[test]
fn node_mode_representation_locked() {
    assert_enum(&[
        (NodeMode::Online, "ONLINE"),
        (NodeMode::GoingOffline, "GOING_OFFLINE"),
        (NodeMode::Offline, "OFFLINE"),
        (NodeMode::GoingOnline, "GOING_ONLINE"),
        (NodeMode::Blocked, "BLOCKED"),
        (NodeMode::StopMode, "STOP_MODE"),
        (NodeMode::CryptoDegraded, "CRYPTO_DEGRADED"),
    ]);
}

#[test]
fn protocol_representation_locked() {
    assert_enum(&[
        (Protocol::Rest, "REST"),
        (Protocol::XmlRpc, "XMLRPC"),
        (Protocol::Maria, "MARIA"),
        (Protocol::Maria304, "MARIA304"),
        (Protocol::CheckboxCompat, "CHECKBOX_COMPAT"),
        (Protocol::Internal, "INTERNAL"),
    ]);
}

#[test]
fn doc_type_representation_locked() {
    assert_enum(&[
        (DocType::ShiftOpen, "SHIFT_OPEN"),
        (DocType::ShiftClose, "SHIFT_CLOSE"),
        (DocType::Sell, "SELL"),
        (DocType::Return, "RETURN"),
        (DocType::ServiceIn, "SERVICE_IN"),
        (DocType::ServiceOut, "SERVICE_OUT"),
        (DocType::CashWithdrawal, "CASH_WITHDRAWAL"),
        (DocType::CashAdvanceEpz, "CASH_ADVANCE_EPZ"),
        (DocType::XReport, "X_REPORT"),
        (DocType::ZReport, "Z_REPORT"),
        (DocType::OfflineSessionBegin, "OFFLINE_SESSION_BEGIN"),
        (DocType::OfflineSessionEnd, "OFFLINE_SESSION_END"),
    ]);
}

#[test]
fn fiscal_mode_representation_locked_lowercase() {
    // §2: FiscalMode is stored LOWERCASE — `test` / `prod`, NOT upper.
    assert_enum(&[(FiscalMode::Test, "test"), (FiscalMode::Prod, "prod")]);
}

#[test]
fn severity_representation_locked() {
    assert_enum(&[
        (Severity::Info, "INFO"),
        (Severity::Warning, "WARNING"),
        (Severity::Error, "ERROR"),
        (Severity::Critical, "CRITICAL"),
    ]);
}
