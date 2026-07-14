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
    DocState, DocType, FiscalMode, NodeMode, OfflineSessionState, Protocol, Severity, ShiftState,
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
                    None => Err(format!(
                        "unknown {} literal in TEXT column: {:?}",
                        stringify!($inner),
                        s
                    )
                    .into()),
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
