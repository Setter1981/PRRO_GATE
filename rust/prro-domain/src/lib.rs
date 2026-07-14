//! `prro-domain` — the **pure** PRRO domain model.
//!
//! Home for the protocol-independent fiscal value types (state enums, id
//! newtypes, and `CanonicalFiscalCommand`) once they are relocated out of the
//! `prro` crate. This crate is deliberately **sqlx-free / I/O-free /
//! runtime-free**: no `sqlx`, `tonic`, `tokio`, `axum`, `prost`, `hyper`, or
//! `reqwest` may enter its normal/build/target dependency graph (contract §9,
//! enforced by the RP-CS1-1 purity gate in `tests/purity_gate.rs`). `uuid` is
//! the one heavyweight dependency allowed, so the UUID-BLOB ids keep their
//! `now_v7` / `new_v5` / serde behaviour byte-identical when they move.
//!
//! The oracle must never call `now()` — this crate holds **no clock**.
//!
//! CS-1a is the **scaffolding** slice: this is an empty skeleton. No domain
//! type has been moved yet — the enums / ids / `CanonicalFiscalCommand`
//! relocate in CS-1b / CS-1b′ / CS-1c behind explicit compatibility shims in
//! `prro`, per
//! `docs/superpowers/specs/2026-07-14-cs1-contract-behaviour-neutral-skeleton.md`.

#![forbid(unsafe_code)]
