//! Pure, sqlx-free strongly-typed ids (CS-1b′).
//!
//! These moved out of `prro::db::models::ids` into the sqlx-free domain crate.
//! They are the **canonical** definitions; `prro::db::models::ids` re-exports
//! them verbatim behind a compatibility shim (contract §5), so every legacy path
//! (`prro::db::models::ids::DocumentId`, …) resolves unchanged.
//!
//! **Storage non-event (contract §2).** Each id keeps its exact pre-move
//! behaviour: the BLOB ids serialise as their bare `Uuid` (`#[serde(transparent)]`)
//! and expose `new()`(`now_v7`) / `from_bytes` / `as_bytes` / `Default`;
//! `CashierId` / `DriverId` are TEXT `String` newtypes with the same strict
//! constructors and `#[serde(transparent)]` output.
//!
//! The SQLite `Type`/`Encode`/`Decode` mapping does **NOT** live here (this crate
//! is sqlx-free) — it lives in the store-side `prro::db::types` wrappers
//! (`DbDocumentId`, `DbCashierId`, …), which encode/decode against the pure
//! types below. `DriverId` has **no** wrapper: it keeps its raw-`String` DB
//! boundary (bind `.as_str()`, decode `String` then `DriverId::new()`).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Define a UUID-BLOB newtype id (the baseline `prro::db::models::ids::id_newtype!`
/// **minus** the `sqlx::Type`/`Encode`/`Decode` impls — those live store-side in
/// `prro::db::types` on the `Db*` wrappers). Keeps `new()`(`now_v7`) /
/// `from_bytes` / `as_bytes` / `Default` and `#[serde(transparent)]` verbatim.
macro_rules! id_newtype {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }
            pub fn from_bytes(b: [u8; 16]) -> Self {
                Self(Uuid::from_bytes(b))
            }
            pub fn as_bytes(&self) -> &[u8; 16] {
                self.0.as_bytes()
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
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

impl ShiftId {
    /// Deterministic shift-id for a SHIFT_OPEN document — A′.1 piece 0b.
    ///
    /// A namespaced UUIDv5 over the opening `document_id`, so an accidental
    /// re-create of the SAME SHIFT_OPEN yields the SAME `shift_id` and
    /// collides on the `shifts` PK (idempotent fail-closed backstop; there
    /// is no prod random-`ShiftId` generator on the create path). Namespaced
    /// (not `document_id`'s own bytes) so a shift-id never equals a
    /// document-id.
    pub fn deterministic_for_shift_open(document_id: DocumentId) -> Self {
        // Fixed namespace for PRRO shift-open shift-ids.  NEVER change — it
        // would re-key every deterministic shift_id and defeat the collision
        // backstop.  (16 raw bytes; a dedicated namespace, distinct from the
        // uuid crate's generic NAMESPACE_* so no other v5 use can collide.)
        const PRRO_SHIFT_OPEN_NS: Uuid = Uuid::from_bytes([
            0x50, 0x52, 0x52, 0x4f, 0x2d, 0x53, 0x48, 0x46, 0x54, 0x2d, 0x4f, 0x50, 0x45, 0x4e,
            0x00, 0x01,
        ]);
        Self(Uuid::new_v5(&PRRO_SHIFT_OPEN_NS, document_id.as_bytes()))
    }
}

// ─── M3b W14a-2a: text-based cashier identity ────────────────────────
//
// CashierId is a TEXT-shaped opaque identifier for the cashier-of-record
// per spec §16.8 (1-cashier-per-shift invariant).  Stored as TEXT in
// shifts.opened_by_cashier_id / shifts.closed_by_cashier_id +
// cashier_certs.cashier_id (per migration 016 schema).
//
// Distinct from the UUID-BLOB id_newtype! macro above — cashier identity
// is operator-domain (e.g. POS-side login handle, 5-ПРРО registration
// string) and arrives at the gateway as a string from ingress; we do not
// generate it.  Validation (non-empty, length cap) at construction time
// keeps invalid values out of the type system.
//
// Minimal newtype per W14a-2a operator decision (2026-05-17):
// repository/seam APIs accept CashierId; broad String → CashierId
// refactor across the codebase is intentionally out of scope.
// DB binds use `.as_str()` (via the store-side `DbCashierId` wrapper).

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CashierId(String);

#[derive(Debug, thiserror::Error)]
pub enum CashierIdError {
    #[error("cashier_id must not be empty")]
    Empty,
    #[error("cashier_id too long: {0} bytes (max 128)")]
    TooLong(usize),
}

/// Driver vendor identifier — stamped by the ingress listener from
/// `ops/config.yaml` per-port `driver_id` config.  NEVER in W3 wire
/// DTO; listener context only.  Used by the conversion layer (W4-Z1)
/// to look up `driver_tax_mapping` for letter→canonical translation
/// and route to the correct outgress quartet.
///
/// Per `project_m4_outgress_architecture` + `feedback_operator_ua_fiscal_authority`
/// memory pins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverId(String);

#[derive(Debug, thiserror::Error)]
pub enum DriverIdError {
    #[error("driver_id MUST be non-empty")]
    Empty,
    #[error("driver_id too long: {0} bytes (max 64)")]
    TooLong(usize),
}

impl DriverId {
    pub const MAX_LEN: usize = 64;

    /// Audit Round-2 (2026-05-27): `.trim()` before length/empty check.
    /// `driver_id` flows in from YAML listener config — whitespace
    /// (leading newline, trailing space from a copy-paste) would
    /// silently fail `driver_tax_mapping` lookups at runtime with no
    /// hint to the operator.  Trim at construction, reject all-
    /// whitespace as `Empty`.
    pub fn new(s: impl Into<String>) -> Result<Self, DriverIdError> {
        let raw = s.into();
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(DriverIdError::Empty);
        }
        if trimmed.len() > Self::MAX_LEN {
            return Err(DriverIdError::TooLong(trimmed.len()));
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl std::fmt::Display for DriverId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl CashierId {
    /// Maximum byte length for a cashier identifier.  Sized generously
    /// for any reasonable POS-side handle (5-ПРРО registration strings,
    /// operator usernames, etc).
    pub const MAX_LEN: usize = 128;

    /// Constructs a `CashierId` from any string-like.  Returns `Err` if
    /// empty or longer than `MAX_LEN`.
    pub fn new(s: impl Into<String>) -> Result<Self, CashierIdError> {
        let s = s.into();
        if s.is_empty() {
            return Err(CashierIdError::Empty);
        }
        if s.len() > Self::MAX_LEN {
            return Err(CashierIdError::TooLong(s.len()));
        }
        Ok(Self(s))
    }

    /// Hydrate a `CashierId` from a persisted (store-side) value **without**
    /// running the strict `new()` validation (CS-1b′).
    ///
    /// The private field + strict `new()` cannot construct legacy values that
    /// pre-date W14a-2a (the empty `__pre_w14a1__`-era back-fill, or an
    /// oversize row from upstream schema drift). The `Decode` path needs to
    /// reconstruct exactly what is stored, so the store-side `DbCashierId`
    /// wrapper (`prro::db::types`) calls this to bypass the constructor.
    ///
    /// **Silent by design** — this pure-crate constructor does NOT log. The
    /// oversize-drift `tracing::warn!` lives store-side in `DbCashierId::decode`
    /// (contract §2: empty ⇒ accepted SILENTLY; `>MAX_LEN` ⇒ accepted WITH a
    /// warning, emitted there), keeping `prro-domain` free of a logging
    /// dependency.
    pub fn from_persisted_unchecked(s: String) -> Self {
        Self(s)
    }

    /// Borrow as `&str` for SQL binds (`.bind(cid.as_str())`).
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume into the underlying `String`.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl std::fmt::Display for CashierId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for CashierId {
    type Err = CashierIdError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

#[cfg(test)]
mod cashier_id_tests {
    use super::*;

    #[test]
    fn new_rejects_empty() {
        assert!(matches!(CashierId::new(""), Err(CashierIdError::Empty)));
    }

    #[test]
    fn new_rejects_too_long() {
        let s = "x".repeat(CashierId::MAX_LEN + 1);
        assert!(matches!(CashierId::new(s), Err(CashierIdError::TooLong(_))));
    }

    #[test]
    fn new_accepts_typical_handle() {
        let c = CashierId::new("cashier-vasya").unwrap();
        assert_eq!(c.as_str(), "cashier-vasya");
        assert_eq!(format!("{c}"), "cashier-vasya");
    }

    #[test]
    fn from_str_roundtrip() {
        let c: CashierId = "operator-007".parse().unwrap();
        assert_eq!(c.as_str(), "operator-007");
    }
}
