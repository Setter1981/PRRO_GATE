//! Integration tests for the Maria 304 wire layer (Sprint M1).
//!
//! These exercise `maria304_driver::wire` the way the session layer
//! will consume it in later sprints: encode a real command, split the
//! bytes across arbitrary chunk boundaries, and decode it back.

use maria304_driver::wire::{crc16, decode_frame, encode_frame, FrameError, Llv};

/// Reference frame layout for `PREP1` without CRC.
///
/// Hand-traced from `Resonance.EKKR.Message.GetBytes`:
///   [0xFD] [0x50 0x52 0x45 0x50 0x31] [0x06] [0xFE]
const PREP1_NO_CRC: &[u8] = &[0xFD, 0x50, 0x52, 0x45, 0x50, 0x31, 0x06, 0xFE];

#[test]
fn encode_prep1_produces_exact_reference_bytes() {
    let bytes = encode_frame("PREP1", false).unwrap();
    assert_eq!(
        bytes, PREP1_NO_CRC,
        "bytes drift from Maria protocol reference"
    );
}

#[test]
fn decode_reference_prep1_frame() {
    let (frame, n) = decode_frame(PREP1_NO_CRC, false).unwrap();
    assert_eq!(frame.text, "PREP1");
    assert!(!frame.had_crc);
    assert_eq!(n, PREP1_NO_CRC.len());
}

#[test]
fn encode_prep1_with_crc_is_self_verifying() {
    let bytes = encode_frame("PREP1", true).unwrap();
    // Total layout: 1 start + 5 payload + 1 len + 1 end + 2 crc = 10 bytes.
    assert_eq!(bytes.len(), 10);
    // Self-verification property: CRC over the full frame (including the
    // appended CRC bytes) must equal zero.
    assert_eq!(crc16(&bytes), 0);
    // Decoder accepts it.
    let (frame, n) = decode_frame(&bytes, true).unwrap();
    assert_eq!(frame.text, "PREP1");
    assert_eq!(n, bytes.len());
}

#[test]
fn split_reads_decode_incrementally() {
    // Simulate TCP delivering the frame one byte at a time — the caller
    // should see Incomplete until the full frame has arrived.
    let full = encode_frame("CSIN1", false).unwrap();
    for n in 0..full.len() {
        match decode_frame(&full[..n], false) {
            Ok(_) => panic!("decoded too early (n={n})"),
            Err(FrameError::Empty) => assert_eq!(n, 0),
            Err(FrameError::Incomplete) => {}
            Err(e) => panic!("unexpected error at n={n}: {e:?}"),
        }
    }
    let (frame, consumed) = decode_frame(&full, false).unwrap();
    assert_eq!(frame.text, "CSIN1");
    assert_eq!(consumed, full.len());
}

#[test]
fn multiple_frames_stream_without_loss() {
    // Concatenated traffic that a session would actually receive.
    let mut stream = Vec::new();
    for cmd in ["CSIN1", "UPAS1111111111", "CONF", "CANC"] {
        stream.extend_from_slice(&encode_frame(cmd, false).unwrap());
    }

    let mut off = 0;
    let mut decoded = Vec::new();
    while off < stream.len() {
        let (frame, n) = decode_frame(&stream[off..], false).unwrap();
        decoded.push(frame.text);
        off += n;
    }
    assert_eq!(decoded, vec!["CSIN1", "UPAS1111111111", "CONF", "CANC"]);
    assert_eq!(off, stream.len());
}

#[test]
fn llv_composes_psd_slip_identically_to_reference() {
    // PSDt command body fragment (acquirer slip) — LLV-encoded fields.
    // Ref source: Resonance.Internal.maria_internal.AddSlip.
    let merchant = Llv::new("BANK_ABC").unwrap();
    let terminal = Llv::new("T001").unwrap();
    let pan = Llv::new("4111********1111").unwrap();
    let rrn = Llv::new("123456789012").unwrap();
    let wire = format!("{merchant}{terminal}{pan}{rrn}");
    assert_eq!(
        wire,
        "08BANK_ABC04T0011641 11********111112123456789012".replace(' ', "")
    );
}

#[test]
fn cyrillic_item_name_roundtrip_via_frame() {
    // FISC command with Ukrainian item name — ensures CP866 survives
    // the framing path without bit-level corruption.
    let frame_bytes = encode_frame("FINFПаляниця 650г", true).unwrap();
    let (frame, _) = decode_frame(&frame_bytes, true).unwrap();
    assert_eq!(frame.text, "FINFПаляниця 650г");
}

#[test]
fn crc_mismatch_surfaces_as_bad_crc_error() {
    let mut buf = encode_frame("UPAS1111111111", true).unwrap();
    let last = buf.len() - 1;
    buf[last] ^= 0xFF;
    assert_eq!(decode_frame(&buf, true), Err(FrameError::BadCrc));
}

// ---------------------------------------------------------------------------
// Additional proof-level tests (post-M1 review)
// ---------------------------------------------------------------------------

use maria304_driver::wire::encode_frame_bytes;

#[test]
fn junk_prefix_before_start_byte_is_reported_as_missing_start() {
    // Real TCP streams can deliver trailing bytes from a previous
    // malformed frame or handshake garbage.  The decoder must not
    // silently accept them.
    let mut stream = vec![0x00, 0x01, 0x02];
    stream.extend_from_slice(&encode_frame("PREP1", false).unwrap());
    assert_eq!(decode_frame(&stream, false), Err(FrameError::MissingStart));
    // Caller should advance to the first 0xFD and retry — this is the
    // expected recovery behavior for the session layer.
    let idx = stream.iter().position(|&b| b == 0xFD).unwrap();
    let (frame, _) = decode_frame(&stream[idx..], false).unwrap();
    assert_eq!(frame.text, "PREP1");
}

#[test]
fn minimum_command_length_boundary_accepted() {
    // Exactly MIN_CMD_LEN (4) bytes — boundary case.
    let encoded = encode_frame("CANC", false).unwrap();
    assert_eq!(encoded.len(), 1 + 4 + 1 + 1); // start + cmd + len + end
    let (frame, _) = decode_frame(&encoded, false).unwrap();
    assert_eq!(frame.text, "CANC");
}

#[test]
fn three_byte_command_rejected() {
    // Below MIN_CMD_LEN — must be rejected deterministically.
    let err = encode_frame("ABC", false).unwrap_err();
    match err {
        FrameError::InvalidCmdLen(3) => {}
        other => panic!("expected InvalidCmdLen(3), got {other:?}"),
    }
}

#[test]
fn oversized_buffer_without_valid_frame_reports_no_frame_found() {
    // Fill a buffer large enough that the decoder can't plead
    // "incomplete" — every byte is junk, no valid frame exists.
    let junk = vec![0xFD; 300]; // starts with START but no END follows
    let err = decode_frame(&junk, false).unwrap_err();
    // Either MissingStart (first byte is 0xFD so this branch not taken)
    // or NoFrameFound (scanned full range, nothing self-consistent).
    assert_eq!(err, FrameError::NoFrameFound);
}

#[test]
fn encode_frame_bytes_exposes_sanitization_branch() {
    // Proves the sanitizer actually fires — which is impossible via the
    // public `encode_frame(&str)` because CP866 never emits 0xFE/0xFF.
    let raw: &[u8] = &[b'A', 0xFE, b'B', 0xFF, b'C'];
    let encoded = encode_frame_bytes(raw, false).unwrap();
    // Payload bytes at positions 1..len-2:
    assert_eq!(&encoded[1..6], &[b'A', 0x20, b'B', 0x20, b'C']);
}

#[test]
fn consecutive_frames_with_mixed_crc_setting() {
    // Real sessions flip CRC mode mid-stream (CSIN0 → CSIN1).  Each frame
    // is decoded with its own crc flag — stream-level consistency is the
    // caller's job.
    let noncrc = encode_frame("CSIN1", false).unwrap();
    let crced = encode_frame("UPAS1111111111", true).unwrap();

    let (f1, n1) = decode_frame(&noncrc, false).unwrap();
    assert_eq!(f1.text, "CSIN1");
    assert!(!f1.had_crc);
    assert_eq!(n1, noncrc.len());

    let (f2, n2) = decode_frame(&crced, true).unwrap();
    assert_eq!(f2.text, "UPAS1111111111");
    assert!(f2.had_crc);
    assert_eq!(n2, crced.len());
}

#[test]
fn max_length_command_roundtrip_with_crc() {
    // Upper boundary of MAX_CMD_LEN (252 bytes) with CRC enabled.
    let cmd = "M".repeat(252);
    let encoded = encode_frame(&cmd, true).unwrap();
    // Total: 1 start + 252 payload + 1 len + 1 end + 2 crc = 257 bytes.
    assert_eq!(encoded.len(), 257);
    assert_eq!(encoded[0], 0xFD);
    assert_eq!(encoded[253], 253); // len byte = payload.len() + 1
    assert_eq!(encoded[254], 0xFE);
    let (frame, n) = decode_frame(&encoded, true).unwrap();
    assert_eq!(frame.text, cmd);
    assert_eq!(n, encoded.len());
}

#[test]
fn llv_rejects_oversized_input() {
    // 100-char value must not be accepted — and specifically must not
    // silently produce invalid wire output.  This closes the previous
    // `From<Option<_>>` landmine.
    let err = Llv::new("x".repeat(100)).unwrap_err();
    assert_eq!(err.0, 100);
}

#[test]
fn crc_stream_property_across_representative_commands() {
    // Self-verification must hold for every realistic command we expect
    // to transmit in M4 / M5 / M6.
    for cmd in [
        "CSIN1",
        "SYNC",
        "UPAS1111111111",
        "PREP1",
        "CANC",
        "CTXT",
        "FISCЦигарки L&M1000005000100030000001",
        "COMP0000000000000000000000000000000000000000000000000000000000",
        "ZREP",
        "NREP",
        "nrep",
        "NULL",
        "KASS",
        "CONf",
    ] {
        let frame = encode_frame(cmd, true).unwrap();
        assert_eq!(
            maria304_driver::wire::crc16(&frame),
            0,
            "self-check failed for {cmd}",
        );
    }
}

#[test]
fn incomplete_buffer_never_false_positives_to_a_different_error() {
    // Any buffer shorter than the minimum full-frame size must surface
    // as Incomplete (or Empty for len 0), never as NoFrameFound or
    // MissingStart-plus-valid-prefix.
    let full = encode_frame("UPAS1111111111", true).unwrap();
    for n in 1..full.len() {
        let err = decode_frame(&full[..n], true).unwrap_err();
        assert!(
            matches!(err, FrameError::Incomplete),
            "len={n} surfaced as {err:?} — expected Incomplete",
        );
    }
}
