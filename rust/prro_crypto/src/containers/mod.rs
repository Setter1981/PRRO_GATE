//! `containers` — higher-level document wrappers (PDF, XML, ASiC).
//!
//! The [`cms`](crate::cms) module produces a CMS/CAdES SignedData
//! (`.p7s`). For some relying parties the signature must be embedded
//! inside a specific document container rather than shipped as a
//! stand-alone `.p7s`:
//!
//! - **PDF** (not yet implemented): ISO 32000 signature dictionary.
//! - **XML / XAdES** (not yet implemented): enveloped/enveloping XML-DSig.
//! - **ASiC-S / ASiC-E** (not yet implemented): ZIP-based containers.
//!
//! This module is **empty today** — it exists so the architectural
//! separation (`cms` → signature bytes; `containers` → document
//! embedding) is visible in the source tree. When a real container
//! format lands, it goes here.
