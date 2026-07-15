//! RP-R4-1b fixture — a legacy UUID-BLOB id no longer satisfies `sqlx::Type<Sqlite>`.
//!
//! Pre-CS-1 the ids carried `impl sqlx::Type/Encode/Decode`; that moved
//! store-side to `prro::db::types::DbDocumentId`. Monomorphising a
//! `T: Type<Sqlite>` bound on the legacy `prro::db::models::DocumentId` path
//! must fail (E0277).

fn needs_sqlx_type<T: sqlx::Type<sqlx::Sqlite>>() {}

fn main() {
    needs_sqlx_type::<prro::db::models::DocumentId>();
}
