//! `profiles` — well-known signature profile catalog.
//!
//! A profile is a (signature-algorithm, hash-algorithm) pair plus any
//! CAdES policy knobs that differ per target. Currently the canonical
//! profile lives in [`crate::cms::profile`]; this module re-exports it
//! so downstream code can write `prro_crypto::profiles::CmsProfile`
//! without reaching into the CMS implementation namespace.
//!
//! Future profiles (Kupyna-256, Kupyna-512, distinct hashes per adapter)
//! will be added here behind the `#[non_exhaustive]` enum.

pub use crate::cms::CmsProfile;
