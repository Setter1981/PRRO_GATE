//! `interop` — adapters that bridge the universal signing core to
//! application-specific systems.
//!
//! Each subdirectory corresponds to one integration target. The rule:
//! nothing in `core/` or `cms/` may depend on anything under `interop/`;
//! the dependency arrow points strictly outward.
//!
//! ## Current adapters
//!
//! - [`prro`] — adapters for the Ukrainian PRRO fiscal ecosystem:
//!   JKS keystore parsing, PKCS#8 key extraction for DSTU material,
//!   legacy jkurwa `short_sign` wire format (feature-gated).

pub mod prro;
