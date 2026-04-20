//! `CONF` / `CONf` response body builder — device identity & state.
//!
//! Requested constantly by 1C to verify the device is alive and the
//! expected state (cashier logged in, receipt open, shift open, etc.).
//! Our virtual driver synthesises this body from the per-listener
//! configuration plus runtime state pulled from the Python gateway.
//!
//! # Layout (protocol PDF §12.5, cross-checked byte-by-byte)
//!
//! The body after the 4-char `CONF`/`CONf` opcode is **exactly 148
//! characters** arranged as fixed-width segments:
//!
//! ```text
//! offset │ width │ meaning                                              │
//! ───────┼───────┼──────────────────────────────────────────────────────┤
//!    0   │  10   │ factory serial — last 10 chars                       │
//!   10   │  10   │ registration / fiscal number                         │
//!   20   │  36   │ merchant name & address (zero-padded right)          │
//!   56   │   8   │ current date (ггггммдд)                              │
//!   64   │   6   │ current time (ччммсс)                                │
//!   70   │   1   │ system key position (see `SysKey`)                   │
//!   71   │   1   │ expected-command flag (see `ExpectedCmd`)            │
//!   72   │   1   │ cashier-registered flag ('0'/'1')                    │
//!   73   │   4   │ cashier identifier (first 4 chars of UPAS <п2>)      │
//!   77   │   1   │ Z-report-performed flag                              │
//!   78   │  12   │ Z-report fiscal number                               │
//!   90   │  12   │ last receipt number                                  │
//!  102   │   4   │ last-successful-command id                           │
//!  106   │   4   │ firmware version id                                  │
//!  110   │   8   │ firmware build date (ггггммдд)                       │
//!  118   │  18   │ current receipt info line (first 18 chars of HEAD)   │
//!  136   │   8   │ currency programming date (ггггммдд)                 │
//!  144   │   1   │ decimal places in money display                      │
//!  145   │   3   │ currency abbreviation (e.g. "Грн")                   │
//!  148   │       │ END                                                  │
//! ```
//!
//! # `CONF` vs `CONf`
//!
//! Byte-identical except three fields use different encoding schemes:
//!
//! | Field              | `CONF` (uppercase F) | `CONf` (lowercase f) |
//! | ------------------ | -------------------- | -------------------- |
//! | system key pos     | `chr(0..9)` raw byte | ASCII `'0'..'9'`     |
//! | Z-report flag      | `chr(0..1)` raw byte | ASCII `'0'..'1'`     |
//! | decimal places     | `chr(0..9)` raw byte | ASCII `'0'..'9'`     |
//!
//! The OLE Manager always calls `CONf` (lowercase) because the raw-byte
//! form would include byte `0x00` which corrupts downstream string
//! handling — our driver only implements the ASCII variant, but the
//! enum distinguishes the two so a future live-capture replay can
//! exercise the binary form if needed.

use std::fmt;

/// Total width of the CONF body after the 4-char opcode.
pub const CONF_BODY_LEN: usize = 148;

/// Mode of the CONF response — chooses the ASCII-vs-raw-byte encoding
/// for the three "coded" fields (key position, Z-flag, decimals).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfMode {
    /// Uppercase `CONF` — uses `chr(0..9)` / `chr(0..1)` raw bytes.
    Binary,
    /// Lowercase `CONf` — uses ASCII `'0'..'9'` / `'0'..'1'`.
    Ascii,
}

/// Virtual-device system key position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysKey {
    Off,             // '0' / chr(0)
    Work,            // '1' / chr(1)
    XReport,         // '2' / chr(2)
    ZReport,         // '3' / chr(3)
    Programming,     // '4' / chr(4)
}

impl SysKey {
    fn code(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::Work => 1,
            Self::XReport => 2,
            Self::ZReport => 3,
            Self::Programming => 4,
        }
    }
}

/// The "expected next command" hint — tells the client what command
/// flavour the firmware is waiting for.  Values are one-char tags from
/// the protocol's Section §18.3; the common ones:
///   * `' '` (space) — idle / no receipt open
///   * `'1'` — awaiting closing command (`COMP`) for a receipt in progress
///   * `'2'` — awaiting close of service document (`PRTX`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedCmd {
    Idle,
    CloseReceipt,
    CloseServiceDoc,
}

impl ExpectedCmd {
    fn as_char(self) -> char {
        match self {
            Self::Idle => ' ',
            Self::CloseReceipt => '1',
            Self::CloseServiceDoc => '2',
        }
    }
}

/// Compose a CONF body from device metadata + live state.
#[derive(Debug, Clone)]
pub struct ConfBuilder<'a> {
    /// Factory serial — driver uses the last 10 chars.  Must be ASCII.
    pub factory_serial: &'a str,
    /// 10-char fiscal registration number.
    pub fiscal_number: &'a str,
    /// Merchant name+address, up to 36 chars (truncated / space-padded).
    pub merchant: &'a str,
    /// Current timestamp: `(ggggmmdd, hhmmss)`.
    pub now: (&'a str, &'a str),
    /// System key position.
    pub sys_key: SysKey,
    /// Expected-command hint.
    pub expected_cmd: ExpectedCmd,
    /// Whether a cashier is logged in.
    pub cashier_registered: bool,
    /// Cashier identifier (first 4 chars kept).
    pub cashier_id: &'a str,
    /// Whether Z-report has been performed for the current day.
    pub z_report_done: bool,
    /// Z-report fiscal number (12 chars, zero-padded decimal).
    pub z_report_number: u64,
    /// Last receipt number (12 chars, zero-padded decimal).
    pub last_receipt_number: u64,
    /// Last successful command id (4 chars).
    pub last_command_id: &'a str,
    /// Firmware version id (4 chars).
    pub fw_version_id: &'a str,
    /// Firmware build date (`ggggmmdd`).
    pub fw_build_date: &'a str,
    /// First 18 chars of the current HEAD line.
    pub receipt_info_line: &'a str,
    /// Currency programming date (`ggggmmdd`).
    pub currency_date: &'a str,
    /// Decimal places in money display (0..=9).
    pub decimals: u8,
    /// Currency abbreviation (3 chars, e.g. `"Грн"`).
    pub currency: &'a str,
}

impl ConfBuilder<'_> {
    /// Produce the 148-char CONF body.
    #[must_use]
    pub fn to_body(&self, mode: ConfMode) -> String {
        let mut out = String::with_capacity(CONF_BODY_LEN);

        // 0..10   serial (last 10 chars)
        let serial = take_last(self.factory_serial, 10);
        push_padded(&mut out, &serial, 10);

        // 10..20  fiscal_number
        push_padded(&mut out, self.fiscal_number, 10);

        // 20..56  merchant
        push_padded(&mut out, self.merchant, 36);

        // 56..64  date
        push_padded(&mut out, self.now.0, 8);

        // 64..70  time
        push_padded(&mut out, self.now.1, 6);

        // 70..71  sys key
        out.push(encode_coded_field(self.sys_key.code(), mode));

        // 71..72  expected-command
        out.push(self.expected_cmd.as_char());

        // 72..73  cashier registered
        out.push(if self.cashier_registered { '1' } else { '0' });

        // 73..77  cashier id
        push_padded(&mut out, take_first(self.cashier_id, 4).as_str(), 4);

        // 77..78  z-report flag
        out.push(encode_coded_field(u8::from(self.z_report_done), mode));

        // 78..90  z-report number
        push_u64(&mut out, self.z_report_number, 12);

        // 90..102 last receipt number
        push_u64(&mut out, self.last_receipt_number, 12);

        // 102..106 last command id
        push_padded(&mut out, self.last_command_id, 4);

        // 106..110 fw version id
        push_padded(&mut out, self.fw_version_id, 4);

        // 110..118 fw build date
        push_padded(&mut out, self.fw_build_date, 8);

        // 118..136 receipt info line
        push_padded(&mut out, self.receipt_info_line, 18);

        // 136..144 currency date
        push_padded(&mut out, self.currency_date, 8);

        // 144..145 decimals
        out.push(encode_coded_field(self.decimals, mode));

        // 145..148 currency
        push_padded(&mut out, self.currency, 3);

        debug_assert_eq!(out.chars().count(), CONF_BODY_LEN);
        out
    }

    /// Produce the full data-frame payload with opcode: `"CONF"` or
    /// `"CONf"` depending on `mode`.
    #[must_use]
    pub fn to_wire_payload(&self, mode: ConfMode) -> String {
        let mut s = String::with_capacity(4 + CONF_BODY_LEN);
        s.push_str(match mode {
            ConfMode::Binary => "CONF",
            ConfMode::Ascii => "CONf",
        });
        s.push_str(&self.to_body(mode));
        s
    }
}

impl fmt::Display for SysKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Off => "Off",
            Self::Work => "Work",
            Self::XReport => "XReport",
            Self::ZReport => "ZReport",
            Self::Programming => "Programming",
        })
    }
}

// ── helpers ──────────────────────────────────────────────────────────

fn encode_coded_field(value: u8, mode: ConfMode) -> char {
    match mode {
        ConfMode::Binary => char::from_u32(u32::from(value)).unwrap_or('\u{0}'),
        ConfMode::Ascii => char::from_digit(u32::from(value), 10).unwrap_or('0'),
    }
}

fn take_first(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn take_last(s: &str, n: usize) -> String {
    let count = s.chars().count();
    s.chars().skip(count.saturating_sub(n)).collect()
}

fn push_padded(out: &mut String, src: &str, width: usize) {
    let mut count = 0;
    for ch in src.chars() {
        if count == width {
            break;
        }
        out.push(ch);
        count += 1;
    }
    for _ in count..width {
        out.push(' ');
    }
}

fn push_u64(out: &mut String, value: u64, width: usize) {
    let s = value.to_string();
    if s.len() >= width {
        // Deliberately keep oversized numbers intact — the caller will
        // notice via 1C's parser.  Matches COMP overflow semantics.
        out.push_str(&s);
    } else {
        for _ in 0..width - s.len() {
            out.push('0');
        }
        out.push_str(&s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample<'a>() -> ConfBuilder<'a> {
        ConfBuilder {
            factory_serial: "MRY304-001",
            fiscal_number: "3001234567",
            merchant: "ТОВ Приклад, Київ",
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
            receipt_info_line: "ПРО MARIA 304 VIRTUAL",
            currency_date: "20250101",
            decimals: 2,
            currency: "Грн",
        }
    }

    #[test]
    fn body_width_is_exactly_148_chars() {
        let b = sample().to_body(ConfMode::Ascii);
        assert_eq!(b.chars().count(), CONF_BODY_LEN);
    }

    #[test]
    fn wire_payload_opcode_matches_mode() {
        let b = sample();
        assert!(b.to_wire_payload(ConfMode::Ascii).starts_with("CONf"));
        assert!(b.to_wire_payload(ConfMode::Binary).starts_with("CONF"));
    }

    #[test]
    fn serial_is_last_10_chars() {
        let mut b = sample();
        b.factory_serial = "SUPERLONGERIAL-12345";
        let body = b.to_body(ConfMode::Ascii);
        // last 10 chars of 20-char string = "IAL-12345" (no wait, let's
        // compute: 20 chars total, take last 10 → chars 10..20)
        let expected: String = "SUPERLONGERIAL-12345".chars().skip(10).collect();
        assert_eq!(&body[..10], &expected);
    }

    #[test]
    fn fiscal_number_lands_at_offset_10() {
        let b = sample().to_body(ConfMode::Ascii);
        assert_eq!(&b[10..20], "3001234567");
    }

    #[test]
    fn merchant_right_padded_with_spaces_to_36_chars() {
        let mut b = sample();
        b.merchant = "Short";
        let body = b.to_body(ConfMode::Ascii);
        let slice: String = body.chars().skip(20).take(36).collect();
        assert!(slice.starts_with("Short"));
        assert_eq!(slice.chars().count(), 36);
        assert_eq!(slice.trim_end(), "Short");
    }

    #[test]
    fn date_and_time_are_exact_widths() {
        let b = sample().to_body(ConfMode::Ascii);
        // merchant ends at char 56 → date 56..64, time 64..70.  Use
        // char-count arithmetic because the merchant may contain
        // Cyrillic multi-byte UTF-8 glyphs but we target char-width.
        let chars: Vec<char> = b.chars().collect();
        let date: String = chars[56..64].iter().collect();
        let time: String = chars[64..70].iter().collect();
        assert_eq!(date, "20260420");
        assert_eq!(time, "101530");
    }

    #[test]
    fn ascii_mode_uses_digit_for_sys_key() {
        let mut b = sample();
        b.sys_key = SysKey::XReport;
        let body = b.to_body(ConfMode::Ascii);
        let ch = body.chars().nth(70).unwrap();
        assert_eq!(ch, '2');
    }

    #[test]
    fn binary_mode_uses_raw_byte_for_sys_key() {
        let mut b = sample();
        b.sys_key = SysKey::ZReport;
        let body = b.to_body(ConfMode::Binary);
        let ch = body.chars().nth(70).unwrap();
        assert_eq!(ch as u32, 3);
    }

    #[test]
    fn cashier_registered_flag_is_single_ascii_char() {
        let mut b = sample();
        b.cashier_registered = false;
        assert_eq!(b.to_body(ConfMode::Ascii).chars().nth(72).unwrap(), '0');
        b.cashier_registered = true;
        assert_eq!(b.to_body(ConfMode::Ascii).chars().nth(72).unwrap(), '1');
    }

    #[test]
    fn z_report_number_is_zero_padded_12_chars() {
        let mut b = sample();
        b.z_report_number = 7;
        let body = b.to_body(ConfMode::Ascii);
        let chars: Vec<char> = body.chars().collect();
        let z: String = chars[78..90].iter().collect();
        assert_eq!(z, "000000000007");
    }

    #[test]
    fn last_receipt_number_is_zero_padded_12_chars() {
        let mut b = sample();
        b.last_receipt_number = 1;
        let body = b.to_body(ConfMode::Ascii);
        let chars: Vec<char> = body.chars().collect();
        let r: String = chars[90..102].iter().collect();
        assert_eq!(r, "000000000001");
    }

    #[test]
    fn currency_abbreviation_lands_at_final_three_chars() {
        let b = sample().to_body(ConfMode::Ascii);
        let chars: Vec<char> = b.chars().collect();
        let cur: String = chars[145..148].iter().collect();
        assert_eq!(cur, "Грн");
    }

    #[test]
    fn decimals_coded_field_follows_mode() {
        let mut b = sample();
        b.decimals = 2;
        let ascii = b.to_body(ConfMode::Ascii);
        let bin = b.to_body(ConfMode::Binary);
        assert_eq!(ascii.chars().nth(144).unwrap(), '2');
        assert_eq!(bin.chars().nth(144).unwrap() as u32, 2);
    }

    #[test]
    fn short_factory_serial_is_space_padded_to_10() {
        let mut b = sample();
        b.factory_serial = "AB";
        let body = b.to_body(ConfMode::Ascii);
        assert_eq!(&body[..10], "AB        ");
    }

    #[test]
    fn cashier_id_is_truncated_to_four_chars() {
        let mut b = sample();
        b.cashier_id = "TooLongCashier";
        let body = b.to_body(ConfMode::Ascii);
        let chars: Vec<char> = body.chars().collect();
        let cid: String = chars[73..77].iter().collect();
        assert_eq!(cid, "TooL");
    }

    #[test]
    fn sys_key_display_is_human_readable() {
        assert_eq!(format!("{}", SysKey::Work), "Work");
        assert_eq!(format!("{}", SysKey::Off), "Off");
    }
}
