//! RP-R4-1b fixture — a legacy TEXT enum can no longer be `.bind`/`Encode`d.
//!
//! Pre-CS-1 `sqlx::query("…").bind(DocState::Prepared)` compiled because the
//! enum carried `impl sqlx::Encode<'_, Sqlite>`. That impl moved store-side to
//! `prro::db::types::DbDocState`. A `T: Encode<'q, Sqlite>` bound monomorphised
//! on the legacy `prro::db::models::DocState` path must fail (E0277) — the
//! registered CS-1 break. Callers now bind `DbDocState(DocState::Prepared)`.

fn needs_encode<'q, T: sqlx::Encode<'q, sqlx::Sqlite>>() {}

fn main() {
    needs_encode::<prro::db::models::DocState>();
}
