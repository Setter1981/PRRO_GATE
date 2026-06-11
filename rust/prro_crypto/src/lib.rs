//! `prro_crypto` — DSTU 4145 cryptography and CMS/CAdES pipeline for
//! Ukrainian fiscal-signing workflows.
//!
//! The crate is layered so application concerns do not leak into the
//! cryptographic core:
//!
//! - [`core`] — field / curve / scalar arithmetic, constant-time
//!   DSTU 4145-LE sign + verify (with full public-point validation),
//!   GOST 34.311-95 hash, CPU-feature-dispatched backends. Knows
//!   nothing about fiscal documents, ДПС, PRRO, containers, or any
//!   wire format.
//! - [`cms`] — CMS/CAdES pipeline on top of `core`: **BES** (RFC 5652),
//!   **T** (RFC 3161 signature-timestamp), **LT** (RFC 5126
//!   revocation-values). Also: EnvelopedData decrypt (ECDH cofactor-DH
//!   through the constant-time ladder), TSP / OCSP / CRL / IIT
//!   cert-lookup-by-SKI HTTP clients under the `tsp_http` feature,
//!   shared bounded DER primitives (`asn1_util`). CMS is still
//!   PRRO-agnostic — any caller with a DSTU cert can use it.
//! - [`profiles`] — catalogue of (signature-alg, hash-alg) profiles.
//! - [`interop`] — adapters for specific deployment targets. Today:
//!   [`interop::prro`] — JKS / PFX / ZS2 / Key-6.dat container readers
//!   and GOST PBE primitives. Anything Ukraine-fiscal-specific lives
//!   here.
//! - [`containers`] — higher-level document wrappers (PDF, XML).
//!   Placeholder today; future work.
//!
//! ## What the crate DOES speak over HTTP
//!
//! Under the `tsp_http` feature (default on) the CMS layer ships
//! blocking HTTP clients for Ukrainian CA-facing services: TSP
//! (RFC 3161), OCSP (RFC 6960), CRL download, and the IIT proprietary
//! cert-lookup-by-SKI protocol used by every Ukrainian CSP at
//! `/services/cmp/`. These are narrowly scoped to CA interaction and
//! do NOT verify the returned signatures — trust anchoring is TLS to
//! the CA plus caller-side validation. See `SECURITY.md` at the crate
//! root for the full trust model and known compromises.
//!
//! ## What the crate does NOT ship
//!
//! HTTP transport to ДПС itself (sendChkV2 / fiscal-document submit),
//! fiscal state machines, retry / idempotency policy, PDF or XML
//! packaging, PKI lifecycle (cert issuance, revocation). Those live
//! in the application (gateway) layer.

pub mod cms;
pub mod containers;
pub mod core;
pub mod interop;
pub mod profiles;

// ─── Top-level re-exports for convenient downstream `use`. ──────────────────
//
// These are the headline types an integrator imports. They do not
// replace the layered `core::...` path — anything more specialized
// must still be reached through the layered namespace.

pub use core::curve::Curve;
pub use core::field::FieldEl;
pub use core::point::Point;
pub use core::sign::{set_verify_cache_capacity, sign, truncate, verify, Signature};

/// Prime the backend dispatch so the first `sign()` in a process does
/// not pay the one-time `CPUID` probing cost. Operational helper, not
/// a cryptographic requirement — see `core::backend::warm_up` for the
/// full rationale.
pub use core::backend::warm_up;

// Low-level GF(2^m) primitives (`blength`, `fmod`, `fmul`, `fsqr`,
// `finv`, `mul_1x1`, `mul_2x2`, `shift_right`) are intentionally NOT
// re-exported at the crate root. Sprint 2.2 API tightening (Expert-2
// review): public surface is kept to the headline `FieldEl/Curve/Point/
// sign/verify/warm_up` types so downstream code is not encouraged to
// build on the variable-time / legacy primitives that still live in
// `core::gf2m::`. Reach them through `prro_crypto::core::gf2m::…` if
// genuinely needed for interop or testing.
