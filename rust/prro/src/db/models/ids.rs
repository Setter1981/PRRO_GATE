//! Strongly-typed UUIDv7 BLOB ids.

use serde::{Deserialize, Serialize};
use sqlx::sqlite::{Sqlite, SqliteArgumentValue};
use sqlx::{Decode, Encode, Type};
use std::borrow::Cow;
use uuid::Uuid;

macro_rules! id_newtype {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }
            pub fn from_bytes(b: [u8; 16]) -> Self {
                Self(Uuid::from_bytes(b))
            }
            pub fn as_bytes(&self) -> &[u8; 16] {
                self.0.as_bytes()
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl Type<Sqlite> for $name {
            fn type_info() -> <Sqlite as sqlx::Database>::TypeInfo {
                <Vec<u8> as Type<Sqlite>>::type_info()
            }
        }

        impl<'q> Encode<'q, Sqlite> for $name {
            fn encode_by_ref(
                &self,
                buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer<'q>,
            ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
                // Owned blob: self's lifetime is shorter than 'q, so we copy
                // the 16-byte UUID into the argument buffer.
                buf.push(SqliteArgumentValue::Blob(Cow::Owned(
                    self.0.as_bytes().to_vec(),
                )));
                Ok(sqlx::encode::IsNull::No)
            }
        }

        impl<'r> Decode<'r, Sqlite> for $name {
            fn decode(
                value: <Sqlite as sqlx::Database>::ValueRef<'r>,
            ) -> Result<Self, sqlx::error::BoxDynError> {
                let bytes = <Vec<u8> as Decode<Sqlite>>::decode(value)?;
                let array: [u8; 16] = bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| "invalid UUID byte length")?;
                Ok(Self::from_bytes(array))
            }
        }
    };
}

id_newtype!(DocumentId);
id_newtype!(RequestId);
id_newtype!(ShiftId);
id_newtype!(OperatorId);
id_newtype!(PrinterId);
id_newtype!(OfflineSessionId);

// ─── M3b W14a-2a: text-based cashier identity ────────────────────────
//
// CashierId is a TEXT-shaped opaque identifier for the cashier-of-record
// per spec §16.8 (1-cashier-per-shift invariant).  Stored as TEXT in
// shifts.opened_by_cashier_id / shifts.closed_by_cashier_id +
// cashier_certs.cashier_id (per migration 016 schema).
//
// Distinct from the UUID-BLOB id_newtype! macro above — cashier identity
// is operator-domain (e.g. POS-side login handle, 5-ПРРО registration
// string) and arrives at the gateway as a string from ingress; we do not
// generate it.  Validation (non-empty, length cap) at construction time
// keeps invalid values out of the type system.
//
// Minimal newtype per W14a-2a operator decision (2026-05-17):
// repository/seam APIs accept CashierId; broad String → CashierId
// refactor across the codebase is intentionally out of scope.
// DB binds use `.as_str()`.

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CashierId(String);

#[derive(Debug, thiserror::Error)]
pub enum CashierIdError {
    #[error("cashier_id must not be empty")]
    Empty,
    #[error("cashier_id too long: {0} bytes (max 128)")]
    TooLong(usize),
}

impl CashierId {
    /// Maximum byte length for a cashier identifier.  Sized generously
    /// for any reasonable POS-side handle (5-ПРРО registration strings,
    /// operator usernames, etc).
    pub const MAX_LEN: usize = 128;

    /// Constructs a `CashierId` from any string-like.  Returns `Err` if
    /// empty or longer than `MAX_LEN`.
    pub fn new(s: impl Into<String>) -> Result<Self, CashierIdError> {
        let s = s.into();
        if s.is_empty() {
            return Err(CashierIdError::Empty);
        }
        if s.len() > Self::MAX_LEN {
            return Err(CashierIdError::TooLong(s.len()));
        }
        Ok(Self(s))
    }

    /// Borrow as `&str` for SQL binds (`.bind(cid.as_str())`).
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume into the underlying `String`.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl std::fmt::Display for CashierId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for CashierId {
    type Err = CashierIdError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

// sqlx Type / Encode / Decode — TEXT column type, owned String semantics.

impl Type<Sqlite> for CashierId {
    fn type_info() -> <Sqlite as sqlx::Database>::TypeInfo {
        <String as Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for CashierId {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        buf.push(SqliteArgumentValue::Text(Cow::Owned(self.0.clone())));
        Ok(sqlx::encode::IsNull::No)
    }
}

impl<'r> Decode<'r, Sqlite> for CashierId {
    fn decode(
        value: <Sqlite as sqlx::Database>::ValueRef<'r>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <String as Decode<Sqlite>>::decode(value)?;
        // Decode path is from DB-side state — we assume invariants were
        // checked at insert time; bypass the constructor checks to avoid
        // rejecting historical rows that pre-date W14a-2a (including the
        // `__pre_w14a1__` sentinel from migration 016 back-fill).
        //
        // PR #66 R1 L1: defensive observability — emit `tracing::warn!`
        // if a stored cashier_id ever exceeds MAX_LEN.  No panic, no
        // Decode error (would reject the row); operators detect schema
        // drift via logs.
        if s.len() > Self::MAX_LEN {
            tracing::warn!(
                cashier_id_len = s.len(),
                max_len = Self::MAX_LEN,
                "CashierId decoded value exceeds MAX_LEN — possible upstream schema drift"
            );
        }
        Ok(Self(s))
    }
}

#[cfg(test)]
mod cashier_id_tests {
    use super::*;

    #[test]
    fn new_rejects_empty() {
        assert!(matches!(CashierId::new(""), Err(CashierIdError::Empty)));
    }

    #[test]
    fn new_rejects_too_long() {
        let s = "x".repeat(CashierId::MAX_LEN + 1);
        assert!(matches!(
            CashierId::new(s),
            Err(CashierIdError::TooLong(_))
        ));
    }

    #[test]
    fn new_accepts_typical_handle() {
        let c = CashierId::new("cashier-vasya").unwrap();
        assert_eq!(c.as_str(), "cashier-vasya");
        assert_eq!(format!("{c}"), "cashier-vasya");
    }

    #[test]
    fn from_str_roundtrip() {
        let c: CashierId = "operator-007".parse().unwrap();
        assert_eq!(c.as_str(), "operator-007");
    }
}
