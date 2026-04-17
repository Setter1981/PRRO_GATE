//! Legacy interop helpers, gated behind the `legacy_jkurwa_interop` feature.
//!
//! These adapters exist only to bridge systems that expect older jkurwa
//! wire formats. They are **not** part of the stable CMS API and should
//! not be relied on by new integrations — the blessed path is
//! `CmsSigner::sign_detached`, whose output is a proper CMS SignedData.
//!
//! Enable with `--features legacy_jkurwa_interop`.

/// Wrap a raw DSTU 4145-LE signature value (`r_le || s_le`) in the legacy
/// jkurwa `short_sign` form: `[0x04, len, r_le, s_le]`.
///
/// This is **not** a valid CMS SignatureValue encoding — CMS expects the
/// raw bytes inside an OCTET STRING, not this ad-hoc tag-length prefix.
/// The only reason this helper exists is for legacy relying parties that
/// were built against jkurwa's pre-CMS output.
pub fn to_jkurwa_short_sign(raw_sig: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + raw_sig.len());
    out.push(0x04);
    out.push(raw_sig.len() as u8);
    out.extend_from_slice(raw_sig);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_sign_shape() {
        let raw = [0xAAu8; 64];
        let out = to_jkurwa_short_sign(&raw);
        assert_eq!(out[0], 0x04);
        assert_eq!(out[1], 64);
        assert_eq!(&out[2..], &raw);
    }
}
