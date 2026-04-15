//! CMS/CAdES-BES detached signature builder for DSTU 4145.
//!
//! Entry point for Phase 4 Sprint 1 work. Produces .p7s-compatible
//! output that closes the drop-in replacement gap for the Node.js
//! sidecar.
//!
//! ## Scope (v1)
//!
//! - detached CAdES-BES (baseline B-B)
//! - single signer
//! - signed attributes: content-type, message-digest, signing-certificate-v2
//! - certificate embedded in `certificates`
//! - DSTU 4145-LE signature algorithm
//! - GOST 34.311-95 digest (Kupyna deferred to later minor version)
//!
//! ## NOT in v1
//!
//! - timestamps (CAdES-T/LT/LTA)
//! - multiple signers / countersignatures
//! - revocation info
//! - attached content (encapsulated)
//!
//! ## Current status (post-Sprint-1, post-expert-review 2026-04-15)
//!
//! Functional CMS builder with real GOST 34.311 hashing, real cert parsing,
//! OsRng-driven rand_e. Remaining release blockers tracked in
//! `PHASE_4_BACKLOG.md`: ESSCertIDv2.issuerSerial (B3), DPS differential (B4).

pub mod attrs;
pub mod builder;
pub mod der_writer;
pub mod oids;
pub mod profile;
pub mod signer;

pub use builder::{
    sign_detached_with_content_digest, CmsError, CmsSigner, DetachedSignature,
};
#[allow(deprecated)]
pub use builder::sign_detached_prehashed;
pub use profile::CmsProfile;
pub use signer::{to_jkurwa_short_sign, DstuInProcessSigner, RawSigner, SignerError};
