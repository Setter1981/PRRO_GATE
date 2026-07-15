//! Store-side sqlx wrappers for the pure `prro-domain` TEXT-affinity enums
//! (CS-1b, contract §4).
//!
//! The domain enums (`prro_domain::{DocState, …}`) are **sqlx-free** — the
//! SQLite `Type`/`Encode`/`Decode` mapping cannot live on them (that would drag
//! sqlx into the pure crate, and the orphan rule forbids `impl sqlx::* for
//! prro_domain::DocState` from `prro` anyway). Instead each moved enum gets a
//! thin `prro`-local newtype wrapper (`DbDocState(pub DocState)`, …) that owns
//! the mapping:
//!
//!   * `Type<Sqlite>` — delegates to `<str as Type<Sqlite>>::type_info` (TEXT).
//!   * `Encode<Sqlite>` — encodes `self.0.as_str()` (byte-identical TEXT).
//!   * `Decode<Sqlite>` — decodes `String`, then `DomainEnum::from_sql_str`; an
//!     **unknown literal ⇒ decode error** (closed set).
//!   * `From<Enum> for DbX` / `From<DbX> for Enum` — cheap conversions at the
//!     repository boundary so struct field types (and all downstream logic)
//!     stay the pure domain enum, unchanged.
//!
//! Storage non-event (contract §2): the encoded bytes are exactly the pre-move
//! `#[sqlx(rename = …)]` literal, because `as_str()` returns the same literal.
//!
//! This whole module relocates wholesale into `prro-store-sqlite` at CS-7; keep
//! it self-contained (no dependency on repository internals). The orphan rule is
//! respected because every `DbX` is a `prro`-local type.

use prro_domain::{
    CashierId, DocState, DocType, DocumentId, FiscalMode, NodeMode, OfflineSessionId,
    OfflineSessionState, OperatorId, PrinterId, Protocol, RequestId, Severity, ShiftId, ShiftState,
};
use sqlx::sqlite::{Sqlite, SqliteArgumentValue};
use sqlx::{Decode, Encode, Type};
use std::borrow::Cow;

/// Define a store-side TEXT wrapper `Db$name(pub $inner)` for a pure domain
/// enum that exposes `as_str(&self) -> &'static str` and
/// `from_sql_str(&str) -> Option<Self>`.
macro_rules! db_text_enum {
    ($wrapper:ident, $inner:ty) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $wrapper(pub $inner);

        impl From<$inner> for $wrapper {
            fn from(v: $inner) -> Self {
                Self(v)
            }
        }

        impl From<$wrapper> for $inner {
            fn from(w: $wrapper) -> Self {
                w.0
            }
        }

        impl Type<Sqlite> for $wrapper {
            fn type_info() -> <Sqlite as sqlx::Database>::TypeInfo {
                <str as Type<Sqlite>>::type_info()
            }

            fn compatible(ty: &<Sqlite as sqlx::Database>::TypeInfo) -> bool {
                <str as Type<Sqlite>>::compatible(ty)
            }
        }

        impl<'q> Encode<'q, Sqlite> for $wrapper {
            fn encode_by_ref(
                &self,
                buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer<'q>,
            ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
                // Byte-identical TEXT: the pre-move `#[sqlx(rename=…)]` literal.
                buf.push(SqliteArgumentValue::Text(Cow::Borrowed(self.0.as_str())));
                Ok(sqlx::encode::IsNull::No)
            }
        }

        impl<'r> Decode<'r, Sqlite> for $wrapper {
            fn decode(
                value: <Sqlite as sqlx::Database>::ValueRef<'r>,
            ) -> Result<Self, sqlx::error::BoxDynError> {
                let s = <String as Decode<Sqlite>>::decode(value)?;
                match <$inner>::from_sql_str(&s) {
                    Some(v) => Ok(Self(v)),
                    // Closed set: an unknown literal is a hard decode error, not
                    // a silent fallback — matches the pre-move `#[sqlx]` derive.
                    //
                    // CS-1R R3.1 (spec §3): the error string is FROZEN to the
                    // pre-move `#[sqlx(type_name="TEXT")]` derive's inner
                    // `ColumnDecode.source` Display — `invalid value {value:?}
                    // for enum {EnumIdent}` — captured empirically at f2c17b1
                    // (golden `tests/golden/cs1r_decode_errors.json`) so the
                    // relocation stays observably equivalent at this layer.
                    None => {
                        Err(format!("invalid value {:?} for enum {}", s, stringify!($inner)).into())
                    }
                }
            }
        }
    };
}

db_text_enum!(DbDocState, DocState);
db_text_enum!(DbOfflineSessionState, OfflineSessionState);
db_text_enum!(DbShiftState, ShiftState);
db_text_enum!(DbNodeMode, NodeMode);
db_text_enum!(DbProtocol, Protocol);
db_text_enum!(DbDocType, DocType);
db_text_enum!(DbFiscalMode, FiscalMode);
db_text_enum!(DbSeverity, Severity);

// ─── CS-1b′: store-side wrappers for the pure `prro-domain` ids ───────
//
// The six UUID-BLOB ids (`DocumentId`, …) and `CashierId` are sqlx-free in
// `prro-domain` (the orphan rule forbids `impl sqlx::* for prro_domain::X` from
// `prro`, and dragging sqlx into the pure crate would break the purity gate).
// Each gets a thin `prro`-local `Db*` newtype that owns the SQLite mapping,
// **byte-identical** to the pre-move impls (contract §2). `DriverId` gets NO
// wrapper — it keeps its raw-`String` boundary (bind `.as_str()`, decode
// `String` then `DriverId::new()`), exactly as the baseline. Relocates into
// `prro-store-sqlite` at CS-7.

/// Define a store-side BLOB wrapper `Db$name(pub $inner)` for a pure UUID-BLOB
/// id that exposes `as_bytes(&self) -> &[u8; 16]` and `from_bytes([u8; 16])`.
///
/// Byte-identical to the pre-move `id_newtype!` sqlx impls: `Type` = `Vec<u8>`
/// (BLOB affinity); `Encode` pushes the 16 raw bytes as an owned blob; `Decode`
/// reads a `Vec<u8>`, `try_into`s a `[u8; 16]` (**length≠16 ⇒ decode error
/// `"invalid UUID byte length"`**, no truncation/pad), then rebuilds the id.
macro_rules! db_blob_id {
    ($wrapper:ident, $inner:ty) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $wrapper(pub $inner);

        impl From<$inner> for $wrapper {
            fn from(v: $inner) -> Self {
                Self(v)
            }
        }

        impl From<$wrapper> for $inner {
            fn from(w: $wrapper) -> Self {
                w.0
            }
        }

        impl Type<Sqlite> for $wrapper {
            fn type_info() -> <Sqlite as sqlx::Database>::TypeInfo {
                <Vec<u8> as Type<Sqlite>>::type_info()
            }
        }

        impl<'q> Encode<'q, Sqlite> for $wrapper {
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

        impl<'r> Decode<'r, Sqlite> for $wrapper {
            fn decode(
                value: <Sqlite as sqlx::Database>::ValueRef<'r>,
            ) -> Result<Self, sqlx::error::BoxDynError> {
                let bytes = <Vec<u8> as Decode<Sqlite>>::decode(value)?;
                let array: [u8; 16] = bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| "invalid UUID byte length")?;
                Ok(Self(<$inner>::from_bytes(array)))
            }
        }
    };
}

db_blob_id!(DbDocumentId, DocumentId);
db_blob_id!(DbRequestId, RequestId);
db_blob_id!(DbShiftId, ShiftId);
db_blob_id!(DbOperatorId, OperatorId);
db_blob_id!(DbPrinterId, PrinterId);
db_blob_id!(DbOfflineSessionId, OfflineSessionId);

/// Store-side TEXT wrapper for the legacy-tolerant `CashierId`.
///
/// Byte-identical to the pre-move `CashierId` sqlx impls (contract §2):
///   * `Type<Sqlite>` = `String` (TEXT affinity).
///   * `Encode` writes `self.0.as_str()` as owned TEXT.
///   * `Decode` reads a `String`, then hydrates via
///     [`CashierId::from_persisted_unchecked`] — **legacy-tolerant**: an empty
///     string is accepted **SILENTLY**; a `> CashierId::MAX_LEN` value is
///     accepted **WITH a `tracing::warn!`** (defensive observability for
///     upstream schema drift — no panic, no decode error, which would reject a
///     historical row). The strict `CashierId::new()` still rejects Empty /
///     TooLong at construction; only this store-side decode path is tolerant.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DbCashierId(pub CashierId);

impl From<CashierId> for DbCashierId {
    fn from(v: CashierId) -> Self {
        Self(v)
    }
}

impl From<DbCashierId> for CashierId {
    fn from(w: DbCashierId) -> Self {
        w.0
    }
}

impl Type<Sqlite> for DbCashierId {
    fn type_info() -> <Sqlite as sqlx::Database>::TypeInfo {
        <String as Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for DbCashierId {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        buf.push(SqliteArgumentValue::Text(Cow::Owned(
            self.0.as_str().to_owned(),
        )));
        Ok(sqlx::encode::IsNull::No)
    }
}

impl<'r> Decode<'r, Sqlite> for DbCashierId {
    fn decode(
        value: <Sqlite as sqlx::Database>::ValueRef<'r>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <String as Decode<Sqlite>>::decode(value)?;
        // Decode path is from DB-side state — we assume invariants were
        // checked at insert time; bypass the constructor checks to avoid
        // rejecting historical rows that pre-date W14a-2a (including the
        // `__pre_w14a1__` sentinel from migration 016 back-fill).
        //
        // PR #66 R1 L1 (moved store-side in CS-1b′): defensive observability —
        // emit `tracing::warn!` if a stored cashier_id ever exceeds MAX_LEN.
        // No panic, no Decode error (would reject the row); operators detect
        // schema drift via logs.
        if s.len() > CashierId::MAX_LEN {
            // CS-1R R3.2 (spec §3): the `target:` is restored EXPLICITLY to the
            // pre-move module path `prro::db::models::ids` (this wrapper moved
            // to `prro::db::types`, which would otherwise change the event's
            // default module-path target). Message + fields stay verbatim.
            tracing::warn!(
                target: "prro::db::models::ids",
                cashier_id_len = s.len(),
                max_len = CashierId::MAX_LEN,
                "CashierId decoded value exceeds MAX_LEN — possible upstream schema drift"
            );
        }
        Ok(Self(CashierId::from_persisted_unchecked(s)))
    }
}
