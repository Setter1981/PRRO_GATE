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
            pub fn new() -> Self { Self(Uuid::now_v7()) }
            pub fn from_bytes(b: [u8; 16]) -> Self { Self(Uuid::from_bytes(b)) }
            pub fn as_bytes(&self) -> &[u8; 16] { self.0.as_bytes() }
        }

        impl Default for $name {
            fn default() -> Self { Self::new() }
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
                buf.push(SqliteArgumentValue::Blob(Cow::Owned(self.0.as_bytes().to_vec())));
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
