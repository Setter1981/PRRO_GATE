//! RP-R4-1b fixture — a legacy TEXT enum no longer satisfies `sqlx::Type<Sqlite>`.
//!
//! Pre-CS-1 the enums carried `#[derive(sqlx::Type)]`; the impl moved store-side
//! to `prro::db::types::DbDocState`. Monomorphising a `T: Type<Sqlite>` bound on
//! the legacy `prro::db::models::DocState` path must fail (E0277). The store-side
//! `DbDocState` wrapper is what actually satisfies the bound now.

fn needs_sqlx_type<T: sqlx::Type<sqlx::Sqlite>>() {}

fn main() {
    // The legacy domain enum: sqlx-free since CS-1 → bound unsatisfied.
    needs_sqlx_type::<prro::db::models::DocState>();
}
