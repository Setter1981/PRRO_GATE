//! Async cert refresh service (M2/W2).
//!
//! Skeleton lands in C1 (this commit).  Types, helpers, and the
//! `refresh_for_fn` body land in C2 + C3.
//!
//! Pipeline (per ADR-M2-4 + ADR-M2-6):
//!
//! 1. Load FN row + `cert_provisioning_config` + `ca_endpoints` (DB
//!    read, no tx).
//! 2. If currently-active cert's `valid_to - now > refresh_within_days`,
//!    return `NoChange`.
//! 3. Compute the SKI to fetch (= currently-active cert's SKI for
//!    refresh).
//! 4. Call `provider.fetch_cert_by_ski(urls, ski, timeout)` — outside
//!    any tx.
//! 5. Parse cert metadata (SKI, valid_from/to, subject/issuer DN) via
//!    `prro_crypto::cms::envelope::parse_cert_basic_fields`.
//! 6. If the new SKI matches the active SKI → in-place UPDATE the
//!    existing active=1 row (single short tx, rows_affected==1).
//! 7. Else (key-roll) → ONE `with_immediate` tx that runs:
//!    `INSERT … active=0 ON CONFLICT(ski_hex) DO UPDATE …
//!    WHERE fiscal_number = excluded.fiscal_number AND active = 0`
//!    (idempotent stage that REFUSES to clobber foreign-owned or
//!    active=1 rows), then `UPDATE … SET active=0 WHERE
//!    fiscal_number=? AND active=1`, then `UPDATE … SET active=1
//!    WHERE ski_hex=?`, then audit_log INSERT.  Atomic + idempotent
//!    on retry (no orphan staged-row window).
//! 8. Return `RefreshedInPlace { ski }` or `RefreshedKeyRoll { old, new }`.
