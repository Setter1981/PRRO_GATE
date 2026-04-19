//! cp1251 encoding helpers — UTF-8 → Windows-1251 byte conversion and
//! XML character escaping for the DPS wire format.
//! Phase 2 implements full encode; this is a Phase 0 stub.

#![allow(dead_code)]

pub fn encode_cp1251(utf8: &str) -> Result<Vec<u8>, crate::errors::SidecarError> {
    let (bytes, _, had_errors) = encoding_rs::WINDOWS_1251.encode(utf8);
    if had_errors {
        return Err(crate::errors::SidecarError::BadRequest(
            "string contains characters not representable in cp1251".into(),
        ));
    }
    Ok(bytes.into_owned())
}
