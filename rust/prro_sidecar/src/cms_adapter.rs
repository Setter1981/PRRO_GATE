//! Thin adapter over `prro_crypto::cms::CmsSigner` — signs cp1251 XML bytes
//! using DSTU 4145-2002, optionally adding RFC 3161 TSP timestamp.
//! Phase 4 implements full logic; this is a Phase 0 stub.

#![allow(dead_code)]

pub fn sign_content(
    _xml_cp1251: &[u8],
) -> Result<Vec<u8>, crate::errors::SidecarError> {
    unimplemented!("cms_adapter::sign_content: Phase 4 stub")
}
