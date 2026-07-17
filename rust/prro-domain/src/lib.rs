//! `prro-domain` — the **pure** PRRO domain model.
//!
//! Home for the protocol-independent fiscal value types (state enums, id
//! newtypes, and `CanonicalFiscalCommand`) once they are relocated out of the
//! `prro` crate. This crate has **no DB / network / async-runtime / RPC / HTTP
//! adapter dependencies**: no `sqlx`, `tonic`, `tokio`, `axum`, `prost`,
//! `hyper`, or `reqwest` may enter its normal/build/target dependency graph
//! (contract §9, enforced by the CS-1R R2 purity gate in
//! `tests/purity_gate.rs`, which pins the whole `--all-features` non-dev
//! closure against `purity-closure.lock`).
//!
//! The **only capability dependency** is `getrandom` (OS entropy for UUID v7
//! random bits, reached via `uuid`); `serde`/`thiserror`/`uuid` are the direct
//! deps. The UUID-BLOB ids keep their `now_v7` / `new_v5` / serde behaviour
//! byte-identical when they move.
//!
//! The **v7 timestamp uses `std::time`, not a dependency** — there is no clock
//! *crate*. (CS-1R R2.5/R2.6: this is dependency-graph purity, honestly scoped;
//! it does NOT and cannot prove the absence of `std::{fs,net,process}` syscalls,
//! which Cargo cannot see — that is out of scope for CS-1R.)
//!
//! CS-1b landed the **TEXT-affinity state/protocol enums** (`DocState`,
//! `OfflineSessionState`, `ShiftState`, `NodeMode`, `Protocol`, `DocType`,
//! `FiscalMode`, `Severity`) here, pure. CS-1b′ landed the **UUID-BLOB ids +
//! legacy TEXT ids** (`DocumentId`, `RequestId`, `ShiftId`, `OperatorId`,
//! `PrinterId`, `OfflineSessionId`, `CashierId`, `DriverId` + their errors)
//! here, pure — their SQLite mapping lives store-side in `prro::db::types`
//! (`DbDocumentId`, `DbCashierId`, …). CS-1c landed `CanonicalFiscalCommand`
//! here (pure, `#[derive(Debug, Clone)]` only — never persisted), behind an
//! explicit compatibility shim in `prro`, per
//! `docs/superpowers/specs/2026-07-14-cs1-contract-behaviour-neutral-skeleton.md`.

#![forbid(unsafe_code)]

pub mod command;
pub mod delivery;
pub mod enums;
pub mod ids;

pub use command::CanonicalFiscalCommand;
pub use enums::{
    DocState, DocType, FiscalMode, NodeMode, OfflineSessionState, Protocol, Severity, ShiftState,
};
pub use ids::{
    CashierId, CashierIdError, DocumentId, DriverId, DriverIdError, OfflineSessionId, OperatorId,
    PrinterId, RequestId, ShiftId,
};
