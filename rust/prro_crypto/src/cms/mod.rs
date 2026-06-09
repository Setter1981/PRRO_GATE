//! CMS/CAdES pipeline for DSTU 4145 + GOST 34.311-95.
//!
//! Produces `.p7s`-compatible output at three CAdES levels:
//!
//! - **BES** (baseline) — detached or attached encapsulation; signed
//!   attributes include `content-type`, `message-digest`,
//!   `signing-certificate-v2` (with full `IssuerSerial`), optional
//!   `signingTime` (UTCTIME).
//! - **T** (timestamped) — BES + `id-aa-signatureTimeStampToken` as an
//!   unsigned attribute. TSP client (RFC 3161, feature `tsp_http`)
//!   handles the HTTP round-trip; callers may also embed a pre-fetched
//!   TST via [`builder::CmsSigner::sign_with_tst`].
//! - **LT** (long-term) — T + `id-aa-ets-revocationValues` carrying
//!   CRLs and/or OCSP responses.
//!
//! Additionally:
//!
//! - **EnvelopedData decrypt** — ECDH cofactor-DH through the CT
//!   Montgomery ladder + GOST key-unwrap + CFB content decrypt.
//! - **IIT cert-lookup-by-SKI** — reverse-engineered proprietary
//!   binary protocol shared by all Ukrainian CSPs; live-tested against
//!   acskidd / uakey / acsk.privatbank.
//! - **OCSP / CRL fetchers** — bounded, redirects-off, per-call agents.
//!
//! ## Not implemented
//!
//! - Multiple signers / countersignatures.
//! - CAdES-LTA (archive-timestamp-v3) — for >7-year archival.
//! - Kupyna (DSTU 7564) digest — parked until first Kupyna-keyed cert
//!   appears in the field.
//! - Response signature verification (TSP/OCSP/CRL/CMP) — the crate
//!   embeds bytes as-delivered; callers validate at their layer.

pub mod asn1_util;
pub mod attrs;
pub mod builder;
pub mod calendar;
pub mod cmp;
pub mod der_writer;
pub mod envelope;
// Legacy jkurwa interop lives in `interop::prro::legacy_sign`, NOT in
// cms/. The `#[cfg(feature = "legacy_jkurwa_interop")] pub mod interop;`
// line that was here previously referenced a non-existent file
// (`src/cms/interop.rs`) and broke `--all-features` builds.
pub mod oids;
pub mod profile;
pub mod revocation;
pub mod signed_data;
pub mod signer;
pub mod tsp;

pub use builder::{
    sign_detached_with_content_digest, CmsBuildOptions, CmsError, CmsSigner, DetachedSignature,
};
pub use profile::CmsProfile;
pub use signed_data::extract_econtent;
pub use signer::{DstuInProcessSigner, RawSigner, SignerError};
