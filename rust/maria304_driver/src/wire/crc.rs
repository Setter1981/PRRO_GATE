//! CRC-16 for Maria 304 frames.
//!
//! The polynomial is documented in the official protocol PDF (section 4.3.4)
//! as *X.25/CCITT* `x^16 + x^12 + x^5 + 1`, but the **computation order**
//! used by genuine firmware diverges from every standard CRC-16 variant
//! I could find — so we port the reference implementation byte-for-byte
//! from the decompiled `Resonance.EKKR.Message.ComputeCRC`:
//!
//! ```c
//! unsigned int crc = 0;
//! while (len--) {
//!     crc ^= *pch;
//!     a = (crc ^ (crc << 4)) & 0x00FF;
//!     crc = (crc >> 8) ^ (a << 8) ^ (a << 3) ^ (a >> 4);
//!     pch++;
//! }
//! ```
//!
//! The checksum is appended to the frame as two bytes in **little-endian**
//! order (low byte first, high byte second).  Because the append step
//! makes the CRC part of the frame, `crc16(full_frame_including_crc_bytes)`
//! of a valid frame returns `0` — this is how the decoder self-verifies.

/// Compute the Maria CRC-16 over `data`.
///
/// Returns the raw 16-bit checksum.  Caller is responsible for appending
/// little-endian bytes onto the frame.
#[must_use]
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &b in data {
        crc ^= u16::from(b);
        let a = (crc ^ (crc << 4)) & 0x00FF;
        crc = (crc >> 8) ^ (a << 8) ^ (a << 3) ^ (a >> 4);
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reference vector: empty buffer
    #[test]
    fn empty_buffer_is_zero() {
        assert_eq!(crc16(&[]), 0);
    }

    // Reference vector: single byte 0x00
    #[test]
    fn single_zero_byte_is_zero() {
        assert_eq!(crc16(&[0x00]), 0);
    }

    // Reference vector: single byte 0xFF
    // Hand-traced through the C reference:
    //   crc = 0
    //   crc ^= 0xFF → crc = 0x00FF
    //   a = (0x00FF ^ 0x0FF0) & 0xFF = 0x0F0F & 0xFF = 0x0F
    //   crc = (0x00FF >> 8) ^ (0x0F << 8) ^ (0x0F << 3) ^ (0x0F >> 4)
    //       = 0x00     ^ 0x0F00       ^ 0x0078       ^ 0x00
    //       = 0x0F78
    #[test]
    fn single_ff_byte_matches_c_trace() {
        assert_eq!(crc16(&[0xFF]), 0x0F78);
    }

    // Streaming identity: crc of a frame followed by its own crc-le
    // bytes evaluates to 0 (self-verification property used by decoder).
    #[test]
    fn self_verification_property() {
        let payload = b"\xFDHELO\x05\xFE"; // made-up valid frame body
        let c = crc16(payload);
        let mut with_crc = payload.to_vec();
        with_crc.push((c & 0xFF) as u8);
        with_crc.push(((c >> 8) & 0xFF) as u8);
        assert_eq!(crc16(&with_crc), 0, "self-check must be zero");
    }

    // Deterministic across runs.
    #[test]
    fn stable_value_for_classic_abc() {
        let abc = crc16(b"ABC");
        assert_eq!(abc, crc16(b"ABC"));
        // And distinct inputs map to distinct values (weak but useful)
        assert_ne!(abc, crc16(b"ABD"));
    }
}
