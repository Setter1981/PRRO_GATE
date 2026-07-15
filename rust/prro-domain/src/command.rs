//! The in-memory canonical fiscal command.
//!
//! **CS-1c (contract §3):** `CanonicalFiscalCommand` relocated here from
//! `prro::services::write_path::types` **under the same name**, behind the
//! explicit compatibility shim in `prro`. It is a pure in-memory command — it
//! derives only `#[derive(Debug, Clone)]` (NOT `Serialize`/`Deserialize`, NOT
//! `sqlx`); it is never persisted, so no store-side wrapper is needed. Its
//! non-primitive fields (`DocType`, `CashierId`, `DriverId`) already live in
//! this crate (CS-1b / CS-1b′) and are referenced as `crate::…`.

use crate::{CashierId, DocType, DriverId};

/// Minimal canonical envelope view used by W5 guards and stage 1
/// INSERT.  Full canonicalisation / XML build happens in stage 3
/// (W6); W5 only needs enough to drive doc_type-shaped guards and
/// build a `NewDocument`.
#[derive(Debug, Clone)]
pub struct CanonicalFiscalCommand {
    pub doc_type: DocType,
    pub business_ts: String,
    pub total_sum_kop: Option<i64>,
    pub payload_json: String,
    pub payload_sha256_canonical: [u8; 32],
    /// RS-3 D5 — the SOURCE hash: the sha256 of the inbox payload that
    /// produced this command, carried so `stage_acquire` can cross-check
    /// it against `ingress_inbox.payload_sha256_canonical` (the idempotency
    /// + crash-recovery anchor, invariant #4).
    ///
    /// For NON-Z documents this COINCIDES with `payload_sha256_canonical`
    /// (one payload, one hash). The A1Z Z path makes them DIVERGE:
    /// `source_sha256` stays the inbox wire-intent hash while
    /// `payload_sha256_canonical` becomes the hash of the *aggregated* Z
    /// report body. Keep the two distinct so a future reader never assumes
    /// the canonical (possibly-aggregated) hash equals the inbox hash.
    pub source_sha256: [u8; 32],
    /// W14a-2b §1.4 — operator/cashier id that will sign this document.
    /// Carries through stage 1 (PREPARED insert) → stage 3 (sign) →
    /// stage 4 (send envelope) and is consumed by `signer_guard` at
    /// stage_send 4-pre (see spec §1.4 + §2.3).  `None` whenever
    /// operator attribution is unavailable: system-context paths
    /// (e.g. boot-phase snapshot reconstruction), test fixtures that
    /// don't exercise signer enforcement, and current ingress
    /// adapters that have not yet been plumbed.
    ///
    /// **Operator-resolved (spec §8 OQ #1):** `CanonicalFiscalCommand`
    /// is not currently `Deserialize`, so adding a field is a Rust-
    /// struct-literal breakage only (all callers updated in this
    /// commit).  No serde concern.
    pub signed_by_cashier_id: Option<CashierId>,

    /// W4-Z0 piece 9 — listener-stamped driver vendor identifier.
    /// `Some` whenever the ingress listener supplies it from
    /// `ops/config.yaml` per-port `driver_id` config.  `None` for
    /// system-context paths (boot reconcile, test fixtures, etc.).
    ///
    /// Used by the W4-Z1 conversion layer to look up
    /// `driver_tax_mapping` for the vendor's letter→canonical
    /// translation table.  NEVER appears in W3 wire DTO — runtime
    /// context only.
    pub driver_id: Option<DriverId>,
}
