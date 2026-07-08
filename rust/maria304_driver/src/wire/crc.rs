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

    // Explicit byte-level reference vector for a 3-byte input.
    // Hand-traced through the C algorithm:
    //   byte 'A' (0x41):
    //     crc = 0 ^ 0x41 = 0x0041
    //     a = (0x0041 ^ 0x0410) & 0xFF = 0x0451 & 0xFF = 0x51
    //     crc = (0x0041 >> 8) ^ (0x51 << 8) ^ (0x51 << 3) ^ (0x51 >> 4)
    //         = 0x00 ^ 0x5100 ^ 0x0288 ^ 0x0005
    //         = 0x538D
    //   byte 'B' (0x42):
    //     crc = 0x538D ^ 0x0042 = 0x53CF
    //     a = (0x53CF ^ 0x3CF0) & 0xFF = 0x6F3F & 0xFF = 0x3F
    //     crc = (0x53CF >> 8) ^ (0x3F << 8) ^ (0x3F << 3) ^ (0x3F >> 4)
    //         = 0x53 ^ 0x3F00 ^ 0x01F8 ^ 0x0003
    //         = 0x3EA8
    //   byte 'C' (0x43):
    //     crc = 0x3EA8 ^ 0x0043 = 0x3EEB
    //     a = (0x3EEB ^ 0xEEB0) & 0xFF = 0xD05B & 0xFF = 0x5B
    //     crc = (0x3EEB >> 8) ^ (0x5B << 8) ^ (0x5B << 3) ^ (0x5B >> 4)
    //         = 0x3E ^ 0x5B00 ^ 0x02D8 ^ 0x0005
    //         = 0x59E3
    #[test]
    fn three_byte_abc_matches_hand_trace() {
        assert_eq!(crc16(b"ABC"), 0x59E3);
    }

    // Sensitivity: single-bit flip in input produces a different CRC.
    // Strong check against accidental early-return or constant-zero bugs.
    #[test]
    fn single_bit_flip_changes_crc() {
        for cmd in [
            b"PREP" as &[u8],
            b"CANC",
            b"COMP1234567890",
            b"UPAS1111111111",
        ] {
            let base = crc16(cmd);
            for i in 0..cmd.len() {
                let mut flipped = cmd.to_vec();
                flipped[i] ^= 0x01;
                assert_ne!(
                    crc16(&flipped),
                    base,
                    "bit flip at pos {i} in {cmd:?} did not change CRC"
                );
            }
        }
    }

    // Input sensitivity to byte ORDER: "AB" != "BA".  Guards against
    // accidentally using a polynomial / shift that commutes over input.
    #[test]
    fn byte_order_affects_crc() {
        assert_ne!(crc16(b"AB"), crc16(b"BA"));
        assert_ne!(crc16(b"PREP1"), crc16(b"PREP2"));
    }
}
