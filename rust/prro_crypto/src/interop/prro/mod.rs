//! PRRO-specific adapters.
//!
//! Everything here is scoped to the Ukrainian PRRO fiscal use case:
//!
//! - [`jks`] — Java KeyStore parser (the format ДПС accredited CAs ship
//!   signer material in)
//! - [`der`] — DSTU-private-key extraction helpers for PKCS#8-wrapped
//!   keys found inside those JKS entries
//! - [`legacy_sign`] — jkurwa `short_sign` wire format, feature-gated
//!   behind `legacy_jkurwa_interop` for relying parties built against
//!   the pre-CMS Node.js sidecar
//!
//! The universal CMS/CAdES builder in `crate::cms` has no knowledge of
//! JKS, ДПС, or PRRO. If you find yourself needing those concepts in
//! `core/` or `cms/`, the abstraction is wrong — route it through
//! `interop/prro/` instead.

pub mod containers;
pub mod der;
pub mod jks;
pub mod key6;
pub mod pbe;
pub mod pfx;

#[cfg(feature = "legacy_jkurwa_interop")]
pub mod legacy_sign;

pub use containers::{
    detect_format, extract_private_key, ContainerError, ContainerFormat, ExtractedKey,
};
