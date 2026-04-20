//! Sprint M2 — golden vectors for the protocol layer.
//!
//! These tests pin the byte-exact output of the typed `Response`
//! builders to handcrafted reference values.  Any drift in the layout
//! or padding logic makes them fail immediately.

use maria304_driver::protocol::{
    CompBuilder, ConfBuilder, ConfMode, ErrorCode, ExpectedCmd, Response, SysKey,
};
use maria304_driver::wire::{decode_frame, encode_frame, Frame};

// ---------------------------------------------------------------------------
// CONF — realistic pilot-grade device dump
// ---------------------------------------------------------------------------

#[test]
fn conf_ascii_body_matches_hand_crafted_reference() {
    let builder = ConfBuilder {
        factory_serial: "MRY304-001",        // 10 chars exactly
        fiscal_number: "3001234567",         // 10 chars exactly
        merchant: "ТОВ Приклад",              // short — space-padded to 36
        now: ("20260420", "101530"),
        sys_key: SysKey::Work,
        expected_cmd: ExpectedCmd::Idle,
        cashier_registered: true,
        cashier_id: "csh1",
        z_report_done: false,
        z_report_number: 42,
        last_receipt_number: 1234,
        last_command_id: "COMP",
        fw_version_id: "4120",
        fw_build_date: "20250115",
        receipt_info_line: "MARIA 304 VIRTUAL",   // <18 chars → space-padded
        currency_date: "20250101",
        decimals: 2,
        currency: "Грн",
    };

    let body = builder.to_body(ConfMode::Ascii);

    // Field-by-field assertions using char-count slicing (multi-byte UTF-8).
    let chars: Vec<char> = body.chars().collect();
    assert_eq!(chars.len(), 148, "CONF body must be exactly 148 chars");

    let chunk = |from: usize, n: usize| -> String { chars[from..from + n].iter().collect() };

    assert_eq!(chunk(0, 10), "MRY304-001");
    assert_eq!(chunk(10, 10), "3001234567");
    assert!(chunk(20, 36).starts_with("ТОВ Приклад"));
    assert_eq!(chunk(20, 36).chars().count(), 36);
    assert_eq!(chunk(56, 8), "20260420");
    assert_eq!(chunk(64, 6), "101530");
    assert_eq!(chunk(70, 1), "1"); // sys_key = Work → '1'
    assert_eq!(chunk(71, 1), " "); // ExpectedCmd::Idle → space
    assert_eq!(chunk(72, 1), "1"); // cashier registered
    assert_eq!(chunk(73, 4), "csh1");
    assert_eq!(chunk(77, 1), "0"); // z_report_done = false
    assert_eq!(chunk(78, 12), "000000000042");
    assert_eq!(chunk(90, 12), "000000001234");
    assert_eq!(chunk(102, 4), "COMP");
    assert_eq!(chunk(106, 4), "4120");
    assert_eq!(chunk(110, 8), "20250115");
    assert_eq!(chunk(118, 18).trim_end(), "MARIA 304 VIRTUAL");
    assert_eq!(chunk(136, 8), "20250101");
    assert_eq!(chunk(144, 1), "2");
    assert_eq!(chunk(145, 3), "Грн");
}

#[test]
fn conf_wire_payload_framed_with_crc_is_self_verifying() {
    use maria304_driver::wire::crc16;
    let builder = ConfBuilder {
        factory_serial: "MRY304-001",
        fiscal_number: "3001234567",
        merchant: "ТОВ Приклад",
        now: ("20260420", "101530"),
        sys_key: SysKey::Work,
        expected_cmd: ExpectedCmd::Idle,
        cashier_registered: true,
        cashier_id: "csh1",
        z_report_done: false,
        z_report_number: 0,
        last_receipt_number: 0,
        last_command_id: "NONE",
        fw_version_id: "4120",
        fw_build_date: "20250115",
        receipt_info_line: "",
        currency_date: "20250101",
        decimals: 2,
        currency: "Грн",
    };
    let payload = builder.to_wire_payload(ConfMode::Ascii);
    let bytes = encode_frame(&payload, true).expect("CONf payload must fit in a frame");
    assert_eq!(crc16(&bytes), 0, "CONf self-check CRC must be zero");

    // Decode roundtrip preserves the payload character-for-character.
    let (Frame { text, .. }, consumed) = decode_frame(&bytes, true).unwrap();
    assert_eq!(consumed, bytes.len());
    assert_eq!(text, payload);
}

// ---------------------------------------------------------------------------
// COMP — hand-built reference response after a SELL
// ---------------------------------------------------------------------------

#[test]
fn comp_body_matches_hand_crafted_reference() {
    let builder = CompBuilder::new(1234, 500_000, 150_000);
    assert_eq!(
        builder.to_body(),
        concat!(
            "0000001234", // check number
            "0000500000", // sale total (5000.00 UAH = 500 000 kopecks)
            "0000000000", // seg 2 — unused
            "0000150000", // return total (1500.00 UAH = 150 000 kopecks)
            "0000000000",
            "0000000000",
            "0000000000",
            "0000000000",
            "0000000000",
        ),
    );
}

#[test]
fn comp_wire_payload_starts_with_opcode() {
    let builder = CompBuilder::new(1, 2, 3);
    let payload = builder.to_wire_payload();
    assert!(payload.starts_with("COMP"));
    assert_eq!(payload.len(), 4 + 90);
}

// ---------------------------------------------------------------------------
// Response — full wire roundtrip for every canonical shape
// ---------------------------------------------------------------------------

#[test]
fn response_ready_produces_exact_wire_shape() {
    let bytes = Response::Ready.to_wire(false).unwrap();
    assert_eq!(bytes, vec![0xFD, b'R', b'E', b'A', b'D', b'Y', 0x06, 0xFE]);
}

#[test]
fn response_done_produces_exact_wire_shape() {
    let bytes = Response::Done.to_wire(false).unwrap();
    assert_eq!(bytes, vec![0xFD, b'D', b'O', b'N', b'E', 0x05, 0xFE]);
}

#[test]
fn response_work_padded_to_min_cmd_len_on_wire() {
    let bytes = Response::Work.to_wire(false).unwrap();
    // WRK\0 → 4 payload bytes → len = 5 → total 7 bytes no crc.
    assert_eq!(bytes, vec![0xFD, b'W', b'R', b'K', 0x00, 0x05, 0xFE]);
}

#[test]
fn response_error_softblock_goes_on_wire_with_full_identifier() {
    let bytes = Response::Error(ErrorCode::SoftBlock).to_wire(false).unwrap();
    // "SOFTBLOCK" — 9 bytes payload, len = 10, + start/end = 12 total.
    assert_eq!(bytes.len(), 12);
    assert_eq!(bytes[0], 0xFD);
    assert_eq!(&bytes[1..10], b"SOFTBLOCK");
    assert_eq!(bytes[10], 0x0A);
    assert_eq!(bytes[11], 0xFE);
}

// ---------------------------------------------------------------------------
// Error-code catalogue — every known wire identifier round-trips
// ---------------------------------------------------------------------------

#[test]
fn every_known_error_code_roundtrips_on_wire() {
    let codes = [
        ErrorCode::SoftBlock,
        ErrorCode::SoftBadCs,
        ErrorCode::SoftUpas,
        ErrorCode::SoftBadArt,
        ErrorCode::SoftDifArt,
        ErrorCode::SoftRegist,
        ErrorCode::SoftCheck,
        ErrorCode::SoftPrnErr,
        ErrorCode::SoftNoDoc,
        ErrorCode::SoftKey,
        ErrorCode::SoftSvc,
        ErrorCode::SoftOfflBufFull,
        ErrorCode::SoftOfflDup,
        ErrorCode::SoftLocked,
    ];
    for code in codes {
        let resp = Response::Error(code.clone());
        let bytes = resp.to_wire(true).unwrap();
        let (frame, _) = decode_frame(&bytes, true).unwrap();
        assert_eq!(frame.text, code.as_wire());
        // And a fresh `ErrorCode::parse` round-trips back to the same
        // variant — proves catalog consistency end-to-end.
        assert_eq!(ErrorCode::parse(&frame.text).unwrap(), code);
    }
}
