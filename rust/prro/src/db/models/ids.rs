//! Compatibility facade for the strongly-typed ids.
//!
//! **CS-1b′ (contract §5):** the six UUID-BLOB newtype ids (`DocumentId`,
//! `RequestId`, `ShiftId`, `OperatorId`, `PrinterId`, `OfflineSessionId`) plus
//! the legacy TEXT ids (`CashierId`, `DriverId`) and their error types
//! (`CashierIdError`, `DriverIdError`) moved into the pure `prro-domain` crate.
//! This module re-exports them **explicitly, per-symbol** (NOT `pub use
//! prro_domain::*`) so every legacy path — `prro::db::models::ids::DocumentId`,
//! … — resolves unchanged.
//!
//! Their SQLite mapping now lives in the store-side `prro::db::types` wrappers
//! (`DbDocumentId`, …, `DbCashierId`); the domain ids themselves are sqlx-free.
//! `DriverId` keeps its raw-`String` DB boundary — it has **no** `Db*` wrapper
//! (contract §2/§3).

// Explicit per-symbol facade re-exports (contract §5).
pub use prro_domain::{
    CashierId, CashierIdError, DocumentId, DriverId, DriverIdError, OfflineSessionId, OperatorId,
    PrinterId, RequestId, ShiftId,
};
