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
    assert_eq!(bytes, PREP1_NO_CRC, "bytes drift from Maria protocol reference");
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
    assert_eq!(wire, "08BANK_ABC04T0011641 11********111112123456789012".replace(' ', ""));
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
