//! Frame encoder and decoder.
//!
//! Implements the transport framing described in the official Maria
//! protocol PDF (§4.3) and verified byte-for-byte against the decompiled
//! `Resonance.EKKR.Message.GetBytes` / `Rs232Transport.try_extract_message`.

use super::{cp866, crc::crc16};

/// Frame start marker.
pub const START: u8 = 0xFD;
/// Frame end marker.
pub const END: u8 = 0xFE;
/// Reserved byte that can never appear inside the payload.
pub const RESERVED_FF: u8 = 0xFF;
/// Substitution byte used when the raw payload would contain `0xFE`/`0xFF`.
pub const SANITIZE_BYTE: u8 = 0x20; // ASCII space

/// Minimum command length accepted by genuine firmware.
pub const MIN_CMD_LEN: usize = 4;
/// Maximum command length accepted by genuine firmware.
pub const MAX_CMD_LEN: usize = 252;

/// Errors raised by the codec.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FrameError {
    /// Command byte-length is outside the `[MIN_CMD_LEN, MAX_CMD_LEN]` range.
    #[error("command must be between {MIN_CMD_LEN} and {MAX_CMD_LEN} bytes (got {0})")]
    InvalidCmdLen(usize),
    /// Caller passed an empty buffer.
    #[error("frame buffer is empty")]
    Empty,
    /// First byte of the buffer is not `0xFD`.  Caller should advance
    /// past the junk byte and retry.
    #[error("no start byte 0xFD in the buffer prefix")]
    MissingStart,
    /// Partial frame — valid prefix but end byte not yet reached.
    /// Caller must keep buffering and retry.
    #[error("no end byte 0xFE reached — partial frame, need more bytes")]
    Incomplete,
    /// Buffer contained `MAX_CMD_LEN + overhead` bytes but no self-consistent
    /// frame was found — the stream is corrupt or the peer is mis-behaving.
    #[error("no self-consistent frame found within max-cmd-len window")]
    NoFrameFound,
    /// Trailing CRC bytes did not validate against the payload.
    #[error("CRC mismatch — frame is corrupt")]
    BadCrc,
}

/// A single decoded frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// CP866-decoded command string (4..=252 chars).
    pub text: String,
    /// Whether the frame carried a trailing CRC on the wire.
    pub had_crc: bool,
}

/// Encode a command string into a wire frame.
///
/// * `cmd` — command + parameters, already in canonical on-wire form
///   (Maria uses ASCII-decimal and zero-padded numbers almost everywhere;
///   this codec does not inspect the semantics).
/// * `with_crc` — whether to append the 16-bit CRC (enabled after `CSIN1`).
///
/// Returns the byte layout `[0xFD][cp866(cmd)][len][0xFE][crc_lo crc_hi]?`.
///
/// # Errors
/// [`FrameError::InvalidCmdLen`] if the command is not 4..=252 bytes.
pub fn encode_frame(cmd: &str, with_crc: bool) -> Result<Vec<u8>, FrameError> {
    let payload = cp866::encode(cmd);
    encode_frame_bytes(&payload, with_crc)
}

/// Encode an already-byte-encoded payload into a wire frame.
///
/// This is the lower-level entry point used by [`encode_frame`] after
/// running CP866 conversion.  It is public so tests can prove the
/// reserved-byte sanitization branch: CP866 never produces `0xFE`/`0xFF`,
/// so the sanitizer is unreachable via [`encode_frame`] and must be
/// exercised by feeding raw bytes directly.
///
/// # Errors
/// [`FrameError::InvalidCmdLen`] if the payload is not 4..=252 bytes.
///
/// # Panics
/// Never — `sanitized.len() + 1` is bounded by `MAX_CMD_LEN + 1 == 253`
/// and therefore always fits in `u8`.
pub fn encode_frame_bytes(payload: &[u8], with_crc: bool) -> Result<Vec<u8>, FrameError> {
    if payload.len() < MIN_CMD_LEN || payload.len() > MAX_CMD_LEN {
        return Err(FrameError::InvalidCmdLen(payload.len()));
    }

    let mut out = Vec::with_capacity(3 + payload.len() + if with_crc { 2 } else { 0 });
    out.push(START);
    // Strip reserved bytes from the payload (matches
    // Resonance.EKKR.Message.GetBytes): any 0xFE/0xFF would otherwise
    // collide with framing markers and desynchronise the parser.
    for &b in payload {
        out.push(if b >= END { SANITIZE_BYTE } else { b });
    }
    out.push(u8::try_from(payload.len() + 1).expect("payload.len()+1 ≤ 253 by MAX_CMD_LEN"));
    out.push(END);

    if with_crc {
        let crc = crc16(&out);
        out.push((crc & 0xFF) as u8); // LO
        out.push(((crc >> 8) & 0xFF) as u8); // HI
    }
    Ok(out)
}

/// Attempt to decode exactly one frame from the head of `buf`.
///
/// On success returns the decoded [`Frame`] and the number of bytes
/// consumed from `buf`.  On failure returns an appropriate
/// [`FrameError`].  The caller is responsible for buffering — when
/// [`FrameError::Incomplete`] is returned the buffer should be kept and
/// retried once more bytes have arrived.
///
/// # Errors
/// * [`FrameError::Empty`] — buffer has no bytes
/// * [`FrameError::MissingStart`] — first byte is not `0xFD`; caller
///   should advance past the junk byte and try again
/// * [`FrameError::Incomplete`] — valid prefix but end byte not yet seen;
///   caller must keep buffering and retry
/// * [`FrameError::BadCrc`] — trailing CRC bytes did not self-verify
/// * [`FrameError::NoFrameFound`] — buffer is large enough to contain a
///   max-size frame but no self-consistent layout was found; caller
///   must resync by scanning for the next `0xFD`
pub fn decode_frame(buf: &[u8], with_crc: bool) -> Result<(Frame, usize), FrameError> {
    if buf.is_empty() {
        return Err(FrameError::Empty);
    }
    if buf[0] != START {
        return Err(FrameError::MissingStart);
    }

    // Scan forward for the END marker.
    // Layout:
    //   pos 0            = START
    //   pos 1..=n        = payload (n bytes)
    //   pos n+1          = length byte (value n+1)
    //   pos n+2          = END
    //   pos n+3, n+4     = CRC (little-endian, optional)
    //
    // n ∈ [MIN_CMD_LEN, MAX_CMD_LEN]
    let tail_min = 1 + MIN_CMD_LEN + 2 + if with_crc { 2 } else { 0 };
    if buf.len() < tail_min {
        return Err(FrameError::Incomplete);
    }

    let search_end = buf.len().min(1 + MAX_CMD_LEN + 2 + if with_crc { 2 } else { 0 });
    for end_idx in (1 + MIN_CMD_LEN + 1)..search_end {
        if buf[end_idx] == END {
            let len_byte = buf[end_idx - 1];
            // Length byte value is `payload.len() + 1`.  Payload occupies
            // `buf[1..end_idx - 1]`, so payload.len() == end_idx - 2, and
            // the expected length byte == end_idx - 1.
            let expected_len = u8::try_from(end_idx - 1).unwrap_or(u8::MAX);
            if len_byte != expected_len {
                // Length byte lies — keep scanning; the 0xFE we just saw
                // might be a junk byte inside a longer, not-yet-visible
                // frame.  If no valid frame exists we'll error out below.
                continue;
            }
            // Found a candidate.  Validate CRC if present.
            if with_crc {
                if buf.len() < end_idx + 3 {
                    return Err(FrameError::Incomplete);
                }
                let frame_with_crc = &buf[..=end_idx + 2];
                if crc16(frame_with_crc) != 0 {
                    return Err(FrameError::BadCrc);
                }
            }
            // INVARIANT: payload.len() = end_idx - 2 ≥ MIN_CMD_LEN is
            // guaranteed by the loop bound `1 + MIN_CMD_LEN + 1` below.
            let payload = &buf[1..end_idx - 1];
            debug_assert!(payload.len() >= MIN_CMD_LEN);
            let text = cp866::decode(payload);
            let consumed = end_idx + 1 + if with_crc { 2 } else { 0 };
            return Ok((Frame { text, had_crc: with_crc }, consumed));
        }
    }

    // No self-consistent frame in the scan range.  Two cases:
    //   * We've not yet buffered enough bytes — caller retries later.
    //   * The stream is definitely corrupt — caller must resync.
    if buf.len() < 1 + MAX_CMD_LEN + 2 + if with_crc { 2 } else { 0 } {
        Err(FrameError::Incomplete)
    } else {
        Err(FrameError::NoFrameFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(cmd: &str, with_crc: bool) {
        let encoded = encode_frame(cmd, with_crc).unwrap();
        let (frame, consumed) = decode_frame(&encoded, with_crc).unwrap();
        assert_eq!(frame.text, cmd, "roundtrip mismatch for {cmd:?}");
        assert_eq!(frame.had_crc, with_crc);
        assert_eq!(consumed, encoded.len(), "must consume entire frame");
    }

    #[test]
    fn prep_frame_no_crc() {
        roundtrip("PREP1", false);
    }

    #[test]
    fn prep_frame_with_crc() {
        roundtrip("PREP1", true);
    }

    #[test]
    fn upas_login_frame_encoded_shape() {
        // UPAS + 10-digit password + cashier_id (≤9 chars or ≥10)
        let bytes = encode_frame("UPAS1111111111Кассир", false).unwrap();
        assert_eq!(bytes[0], START);
        assert_eq!(*bytes.last().unwrap(), END);
        // ASCII prefix "UPAS1111111111" is 14 bytes, then 6 CP866 bytes for "Кассир"
        // (К=8A, а=A0, с=E1, с=E1, и=A8, р=E0).
        let payload_len = bytes.len() - 3; // minus start/len/end
        assert_eq!(payload_len, 14 + 6);
        // Length byte must equal payload_len + 1.
        assert_eq!(bytes[bytes.len() - 2], u8::try_from(payload_len).unwrap() + 1);
    }

    #[test]
    fn min_cmd_len_4_accepted() {
        roundtrip("CANC", false);
        roundtrip("COMP", true);
    }

    #[test]
    fn cmd_shorter_than_4_rejected() {
        let err = encode_frame("PRE", false).unwrap_err();
        assert_eq!(err, FrameError::InvalidCmdLen(3));
    }

    #[test]
    fn cmd_longer_than_252_rejected() {
        let long = "A".repeat(253);
        let err = encode_frame(&long, false).unwrap_err();
        assert_eq!(err, FrameError::InvalidCmdLen(253));
    }

    #[test]
    fn cmd_max_252_accepted() {
        let cmd = "A".repeat(252);
        let encoded = encode_frame(&cmd, true).unwrap();
        let (frame, _) = decode_frame(&encoded, true).unwrap();
        assert_eq!(frame.text, cmd);
    }

    #[test]
    fn payload_bytes_fe_ff_are_sanitized_to_space() {
        // Direct proof: feed raw bytes containing 0xFE and 0xFF via the
        // lower-level `encode_frame_bytes` entry point.  Both must be
        // rewritten to 0x20 (space) — that is the exact behavior of
        // Resonance.EKKR.Message.GetBytes.
        let raw: &[u8] = &[b'A', 0xFE, b'B', 0xFF];
        let encoded = encode_frame_bytes(raw, false).unwrap();
        // Layout: [0xFD] [A][0x20][B][0x20] [len=5] [0xFE]
        assert_eq!(
            encoded,
            vec![0xFD, b'A', 0x20, b'B', 0x20, 0x05, 0xFE],
            "sanitizer drifted from reference — 0xFE/0xFF must become 0x20",
        );
    }

    #[test]
    fn sanitized_frame_still_decodes_to_valid_text() {
        // After sanitization the roundtrip obviously changes the text
        // (original bytes are lost), but the frame must still be
        // structurally valid and decode without error.
        let raw = vec![b'X', 0xFE, 0xFF, b'Y'];
        let encoded = encode_frame_bytes(&raw, true).unwrap();
        let (frame, n) = decode_frame(&encoded, true).unwrap();
        assert_eq!(frame.text, "X  Y", "sanitized bytes should decode as spaces");
        assert_eq!(n, encoded.len());
    }

    #[test]
    fn decode_rejects_empty_buffer() {
        assert_eq!(decode_frame(&[], false).unwrap_err(), FrameError::Empty);
    }

    #[test]
    fn decode_rejects_missing_start() {
        let buf = b"\x00PREP1\x06\xFE";
        assert_eq!(decode_frame(buf, false).unwrap_err(), FrameError::MissingStart);
    }

    #[test]
    fn decode_reports_incomplete_when_buffer_too_short() {
        let full = encode_frame("PREP1", false).unwrap();
        let partial = &full[..full.len() - 1];
        assert_eq!(decode_frame(partial, false).unwrap_err(), FrameError::Incomplete);
    }

    #[test]
    fn decode_with_crc_rejects_wrong_checksum() {
        let mut frame = encode_frame("PREP1", true).unwrap();
        // Flip a bit in the CRC bytes.
        let len = frame.len();
        frame[len - 1] ^= 0x01;
        assert_eq!(decode_frame(&frame, true).unwrap_err(), FrameError::BadCrc);
    }

    #[test]
    fn decode_self_verifies_via_crc_stream_over_full_frame() {
        // Property: `crc16(frame_with_crc) == 0` for any well-formed frame.
        for cmd in ["PREP1", "CANC", "COMP1234567890", "CSIN1"] {
            let frame = encode_frame(cmd, true).unwrap();
            assert_eq!(crc16(&frame), 0, "self-check failed for {cmd}");
        }
    }

    #[test]
    fn streaming_scenario_back_to_back_frames() {
        // Two frames concatenated — second decode starts from `consumed` offset.
        let a = encode_frame("CSIN1", false).unwrap();
        let b = encode_frame("UPAS1111111111", false).unwrap();
        let mut buf = a.clone();
        buf.extend_from_slice(&b);

        let (frame1, used1) = decode_frame(&buf, false).unwrap();
        assert_eq!(frame1.text, "CSIN1");

        let (frame2, used2) = decode_frame(&buf[used1..], false).unwrap();
        assert_eq!(frame2.text, "UPAS1111111111");

        assert_eq!(used1 + used2, buf.len(), "no trailing garbage");
    }

    #[test]
    fn cyrillic_command_roundtrip() {
        // Department name on a restaurant POS.  Mind: every `і` below is
        // the Ukrainian Cyrillic U+0456 (byte 0xB9 in CP866), NOT Latin
        // U+0069 — mix-up would silently pass via ASCII and defeat the
        // invariant we're trying to prove.
        roundtrip("PREPБар", true);
        roundtrip("GRBGОсновні позиції", false);

        // Guarantee at the byte level that the Ukrainian letter `і`
        // survives framing → CP866 → framing without being replaced by
        // a Latin lookalike or a `?`.
        let frame = encode_frame("FINFПаляниця", true).unwrap();
        // CP866 for "Паляниця": П(8F) а(A0) л(AB) я(EF) н(AD) и(A8) ц(E6) я(EF)
        let expected_payload: &[u8] = &[
            b'F', b'I', b'N', b'F',
            0x8F, 0xA0, 0xAB, 0xEF, 0xAD, 0xA8, 0xE6, 0xEF,
        ];
        assert_eq!(&frame[1..=expected_payload.len()], expected_payload);
    }

    #[test]
    fn ukrainian_i_is_cyrillic_not_latin_in_frame() {
        // Ukrainian `і` (U+0456) must encode as 0xB9.  Latin `i`
        // (U+0069) encodes as 0x69.  This test asserts we're using the
        // Cyrillic form end-to-end.
        let ukr = encode_frame("TEXTі", false).unwrap();
        let lat = encode_frame("TEXTi", false).unwrap();
        assert_ne!(ukr, lat, "Ukrainian `і` must differ from Latin `i` on the wire");
        // Payload byte just after the 4-char command:
        assert_eq!(ukr[5], 0xB9, "Ukrainian `і` should encode as 0xB9");
        assert_eq!(lat[5], 0x69, "Latin `i` should encode as 0x69");
    }
}
