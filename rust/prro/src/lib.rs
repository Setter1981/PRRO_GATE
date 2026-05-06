//! PRRO Gateway — single-binary Rust implementation.
//!
//! Top-level architecture lives in `app::App`. This crate is composed
//! of:
//! - `config`         — TOML + env + CLI overrides
//! - `db`             — sqlx pool, transaction primitive, repositories
//! - `runtime`        — singleton lock, supervisor, ops loop (M3+)
//! - `crypto`         — wraps `prro_crypto` (M2)
//! - `transports`     — DPS gRPC, Checkbox REST (M2)
//! - `services`       — write_path, reconciliation, ingress (M3+)
//! - `ingress`        — REST/XML-RPC/Maria/Maria304/Checkbox-compat (M4+)
//! - `admin_ui`       — Askama-rendered admin (M5)
//! - `rendering`      — receipt formatter + HTML/PDF/ESC-POS (M5)
//! - `doctor`         — `prro doctor` diagnostics

pub mod app;
pub mod config;
pub mod crypto;
pub mod db;
pub mod doctor;
pub mod runtime;
pub mod services;
pub mod transports;
pub mod xml;

pub use app::App;
