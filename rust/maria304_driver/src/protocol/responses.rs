//! Typed wire responses.
//!
//! Every response the driver emits back to 1C is one of five shapes:
//!
//! | Variant        | On-wire payload               | Role                                                |
//! | -------------- | ----------------------------- | --------------------------------------------------- |
//! | [`Ready`]      | `READY`                       | End-of-reply marker; frees the command channel.      |
//! | [`Wait`]       | `WAIT`                        | Acknowledge command received, processing started.    |
//! | [`Work`]       | `WRK`                         | Heartbeat for long-running commands (2 s interval).  |
//! | [`Printing`]   | `PRN`                         | Heartbeat specific to print activity.                |
//! | [`Done`]       | `DONE`                        | Last artefact before `READY` on success.             |
//! | [`Error`]      | `SOFT…` identifier            | Terminal failure; no further frames until next cmd.  |
//! | [`Data`]       | `<cmd><payload>`              | Command-specific body (CONF, CCAS, COMP, GLCN, …).   |
//!
//! The real firmware streams multiple frames per command: typically
//! `WAIT` → (`WRK` | `PRN`)\* → `DATA` → `DONE` → `READY`.  Our virtual
//! driver elides the `WAIT`/`WRK` stream for commands that complete
//! synchronously (i.e. no bridge call, or a fast one); the session
//! dispatcher decides when to emit them.

use super::error_codes::ErrorCode;
use crate::wire::{encode_frame, FrameError};

/// A single wire frame the driver intends to send to 1C.
///
/// The `to_wire` method produces the exact byte sequence — framing,
/// CP866, optional CRC — ready for `AsyncWrite::write_all`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    /// Channel free, ready to accept the next command.
    Ready,
    /// Acknowledgement of command receipt; processing in progress.
    Wait,
    /// Non-idle heartbeat (non-printing work).
    Work,
    /// Print-activity heartbeat.
    Printing,
    /// Command completed successfully.
    Done,
    /// Terminal failure; 1C will treat as error.
    Error(ErrorCode),
    /// Command-specific data payload (e.g. `CONF` state dump, `COMP`
    /// fiscal numbers).  The string is the full frame payload
    /// including the 4-char command echo (e.g. `"CONF<148-char body>"`).
    ///
    /// Construct with [`Response::data`] — the public constructor
    /// validates length at construction time.  The enum variant is
    /// intentionally public so tests and `admin replay` can feed raw
    /// captured bytes, but production code should go through the
    /// checked constructor.
    Data(String),
}

impl Response {
    /// Construct a validated [`Response::Data`].
    ///
    /// Surfaces wire-framing errors at construction time instead of
    /// deferring them to [`Self::to_wire`] — closes the "silent
    /// invalid response" landmine flagged in the M2 review.
    ///
    /// # Errors
    /// [`FrameError::InvalidCmdLen`] if the CP866-encoded length of
    /// `payload` falls outside the protocol-defined 4..=252-byte range.
    pub fn data(payload: impl Into<String>) -> Result<Self, FrameError> {
        let s = payload.into();
        // The wire codec validates byte length after CP866 conversion.
        // Running through encode_frame is the most direct proof that
        // the payload is admissible — we just discard the bytes.
        let _probe = encode_frame(&s, false)?;
        Ok(Self::Data(s))
    }

    /// Produce the exact byte sequence for the wire.
    ///
    /// # Errors
    /// [`FrameError::InvalidCmdLen`] if the payload would fall outside
    /// the protocol-defined 4..=252-byte range.  For variants other
    /// than [`Self::Data`] this is impossible — their payloads are
    /// internally bounded to 3..=10 chars plus NUL padding.
    pub fn to_wire(&self, with_crc: bool) -> Result<Vec<u8>, FrameError> {
        encode_frame(&self.as_payload_string(), with_crc)
    }

    /// The payload string before framing.  Exposed mainly for tests.
    #[must_use]
    pub fn as_payload_string(&self) -> String {
        match self {
            Self::Ready => "READY".to_string(),
            Self::Wait => "WAIT".to_string(),
            Self::Work => "WRK\0".to_string(), // pad to MIN_CMD_LEN
            Self::Printing => "PRN\0".to_string(),
            Self::Done => "DONE".to_string(),
            Self::Error(code) => pad_to_min_len(code.as_wire()),
            Self::Data(s) => s.clone(),
        }
    }
}

/// Minimum command length required by the wire codec.
///
/// Duplicated from `wire::codec::MIN_CMD_LEN` to avoid a cross-module
/// coupling in inlined formatters.  If this value ever changes upstream
/// the test in `tests/wire_vectors.rs` will fail.
const MIN_CMD_LEN: usize = 4;

/// Pad a short identifier up to [`MIN_CMD_LEN`] bytes with NUL.  The
/// real firmware uses trailing NULs for 3-char opcodes like `WRK`/`PRN`
/// — decompiled frames show `WRK\0` / `PRN\0`.
fn pad_to_min_len(s: &str) -> String {
    if s.len() >= MIN_CMD_LEN {
        s.to_string()
    } else {
        let mut out = s.to_string();
        for _ in s.len()..MIN_CMD_LEN {
            out.push('\0');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::decode_frame;

    fn roundtrip(resp: &Response, with_crc: bool) -> String {
        let bytes = resp.to_wire(with_crc).unwrap();
        let (frame, consumed) = decode_frame(&bytes, with_crc).unwrap();
        assert_eq!(consumed, bytes.len(), "frame must consume entire wire");
        assert_eq!(frame.had_crc, with_crc);
        frame.text
    }

    #[test]
    fn ready_wire_is_exactly_five_chars() {
        assert_eq!(Response::Ready.as_payload_string(), "READY");
    }

    #[test]
    fn done_wire_is_exactly_four_chars() {
        assert_eq!(Response::Done.as_payload_string(), "DONE");
    }

    #[test]
    fn wrk_and_prn_are_padded_to_min_cmd_len() {
        // Real firmware uses trailing NUL padding for 3-char opcodes
        // because the wire codec enforces a 4-byte minimum payload.
        assert_eq!(Response::Work.as_payload_string(), "WRK\0");
        assert_eq!(Response::Printing.as_payload_string(), "PRN\0");
    }

    #[test]
    fn error_wire_padded_for_short_custom_codes() {
        let short = Response::Error(ErrorCode::Custom("ERR".to_string()));
        assert_eq!(short.as_payload_string(), "ERR\0");
    }

    #[test]
    fn error_known_code_is_not_padded() {
        // "SOFTBLOCK" is already 9 chars — padding is a no-op.
        assert_eq!(
            Response::Error(ErrorCode::SoftBlock).as_payload_string(),
            "SOFTBLOCK",
        );
    }

    #[test]
    fn data_passes_caller_supplied_payload_verbatim() {
        let payload = "COMP0000012345".to_string(); // not full COMP, just shape proof
        let r = Response::Data(payload.clone());
        assert_eq!(r.as_payload_string(), payload);
    }

    #[test]
    fn every_short_variant_survives_framing_roundtrip() {
        for resp in [
            Response::Ready,
            Response::Wait,
            Response::Work,
            Response::Printing,
            Response::Done,
            Response::Error(ErrorCode::SoftBlock),
            Response::Error(ErrorCode::SoftBadArt),
            Response::Error(ErrorCode::Custom("SOFTX".to_string())),
        ] {
            for with_crc in [false, true] {
                let text = roundtrip(&resp, with_crc);
                assert_eq!(text, resp.as_payload_string(), "{resp:?} / crc={with_crc}");
            }
        }
    }

    #[test]
    fn data_variant_roundtrips_through_wire() {
        let data = Response::Data("CONFabcdef".to_string()); // minimal sample
        let text = roundtrip(&data, true);
        assert_eq!(text, "CONFabcdef");
    }

    #[test]
    fn data_variant_shorter_than_min_cmd_len_is_rejected_on_encode() {
        // Caller must supply at least 4 bytes of payload; anything
        // shorter should surface the underlying FrameError.
        let too_short = Response::Data("abc".to_string());
        let err = too_short.to_wire(false).unwrap_err();
        match err {
            FrameError::InvalidCmdLen(3) => {}
            other => panic!("expected InvalidCmdLen(3), got {other:?}"),
        }
    }

    #[test]
    fn data_variant_longer_than_max_cmd_len_is_rejected() {
        let too_long = Response::Data("x".repeat(253));
        let err = too_long.to_wire(false).unwrap_err();
        match err {
            FrameError::InvalidCmdLen(253) => {}
            other => panic!("expected InvalidCmdLen(253), got {other:?}"),
        }
    }

    // ── Data() validated-constructor tests (post-M2 review) ─────────

    #[test]
    fn data_constructor_accepts_valid_payload() {
        // 4 bytes is the minimum accepted by the wire codec.
        let r = Response::data("COMP").expect("minimum 4-char payload must validate");
        assert_eq!(r.as_payload_string(), "COMP");
    }

    #[test]
    fn data_constructor_rejects_short_payload_at_construction() {
        // Rejects at construction time instead of deferring to to_wire.
        let err = Response::data("abc").unwrap_err();
        match err {
            FrameError::InvalidCmdLen(3) => {}
            other => panic!("expected InvalidCmdLen(3), got {other:?}"),
        }
    }

    #[test]
    fn data_constructor_rejects_over_252_char_payload() {
        let err = Response::data("x".repeat(253)).unwrap_err();
        match err {
            FrameError::InvalidCmdLen(253) => {}
            other => panic!("expected InvalidCmdLen(253), got {other:?}"),
        }
    }

    #[test]
    fn data_constructor_exact_boundary_252_bytes_accepted() {
        let r = Response::data("y".repeat(252)).expect("252-byte payload must validate");
        assert_eq!(r.as_payload_string().len(), 252);
    }

    #[test]
    fn display_through_framing_produces_crc_self_verifying_bytes() {
        use crate::wire::crc16;
        for resp in [
            Response::Ready,
            Response::Done,
            Response::Error(ErrorCode::SoftCheck),
        ] {
            let bytes = resp.to_wire(true).unwrap();
            assert_eq!(crc16(&bytes), 0, "self-check failed for {resp:?}");
        }
    }
}
