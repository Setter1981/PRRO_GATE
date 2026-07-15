//! Compatibility facade for the state-machine / protocol enums and the
//! strongly-typed ids.
//!
//! **CS-1R R4 (spec `2026-07-15-cs1r-remediation-spec.md` §1 RP-R4-1c).** The
//! two submodule globs (`pub use enums::*; pub use ids::*;`) were replaced with
//! an **explicit per-symbol legacy export-list**. A glob silently *widens* the
//! facade — re-exporting anything later added to `enums` / `ids` — and evades
//! the RP-R4-1c AST set-equality pin. The explicit list below is the single
//! reviewed place the legacy `prro::db::models::{…}` short-path surface is
//! defined; adding or removing a legacy symbol is a deliberate edit here.
//!
//! Both legacy paths keep resolving unchanged:
//!   * nested — `prro::db::models::enums::DocState`, `…::ids::DocumentId` (via
//!     the `pub mod enums; pub mod ids;` declarations + each submodule's own
//!     explicit `pub use prro_domain::{…}` shim);
//!   * short — `prro::db::models::DocState`, `prro::db::models::DocumentId`
//!     (via the per-symbol re-exports below).

pub mod enums;
pub mod ids;

// ── Explicit per-symbol legacy re-exports (RP-R4-1c; NO glob) ──────────────
//
// This set MUST equal the pinned legacy list asserted by
// `tests/rp_r4_1c_models_facade_no_glob.rs`. It is exactly what the prior
// `pub use enums::*; pub use ids::*;` globs exported.

// From `enums`: the 8 TEXT enums (re-exported from `prro_domain`) plus the
// locally-defined sqlx-bearing `InboxStatus` (stays in `prro`, contract §2/§10).
pub use enums::{
    DocState, DocType, FiscalMode, InboxStatus, NodeMode, OfflineSessionState, Protocol, Severity,
    ShiftState,
};

// From `ids`: the 6 UUID-BLOB newtype ids + the TEXT-shaped `CashierId` /
// `DriverId` + their error types (all re-exported from `prro_domain`).
pub use ids::{
    CashierId, CashierIdError, DocumentId, DriverId, DriverIdError, OfflineSessionId, OperatorId,
    PrinterId, RequestId, ShiftId,
};
