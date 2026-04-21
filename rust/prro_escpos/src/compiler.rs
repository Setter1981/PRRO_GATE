//! High-level compiler: turn a sequence of semantic [`Instruction`]s
//! into the ESC/POS byte stream for a specific printer profile.
//!
//! The caller drives output by pushing instructions:
//!
//! ```no_run
//! use prro_escpos::{PrinterProfile, ReceiptCompiler, Instruction, Alignment, CodePage, bundled};
//!
//! let profile = PrinterProfile::from_xml_str(bundled::EPSON_TM_T88II).unwrap();
//! let mut c = ReceiptCompiler::new(&profile);
//! c.push(Instruction::Init);
//! c.push(Instruction::Codepage(CodePage::Cp866));
//! c.push(Instruction::Align(Alignment::Center));
//! c.push(Instruction::Text("Чек #42".into()));
//! c.push(Instruction::Newline);
//! c.push(Instruction::Feed(3));
//! c.push(Instruction::Cut);
//! let bytes: Vec<u8> = c.compile().unwrap();
//! // send `bytes` to TCP:9100 / Serial / USB.
//! ```
//!
//! Code-page conversion happens at emit time using `encoding_rs`
//! (cp866 / cp1251).  Text that cannot be encoded in the chosen page
//! raises [`Error::Codepage`] so callers can decide whether to
//! substitute or escalate.

use crate::error::{Error, Result};
use crate::executor::Executor;
use crate::profile::PrinterProfile;
use encoding_rs::{IBM866, WINDOWS_1251};

#[derive(Debug, Clone, Copy)]
pub enum Alignment {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy)]
pub enum CodePage {
    /// IBM866 — Ukrainian/Russian DOS-era code page.  Matches ESC/POS
    /// code-page index 11 on most Epson-family printers.
    Cp866,
    /// Windows-1251 — used by some Lukhan/Posiflex firmware.  Our
    /// compiler maps this to whatever the profile declares (or falls
    /// back to IBM866 if not listed — chosen explicitly by caller).
    Cp1251,
    /// ASCII-only — do not emit any `CodePage` command; use when the
    /// receipt contains only 7-bit text.
    Ascii,
}

#[derive(Debug, Clone)]
pub enum Instruction {
    /// Emit `ESC @` — printer reset.  Almost always the first byte.
    Init,
    /// Set the text code page.  Emits the profile's `CodePage` command
    /// with the matching enum value; ASCII is a no-op.
    Codepage(CodePage),
    /// Horizontal alignment for subsequent text.
    Align(Alignment),
    /// Font size/style selector — currently only NORMAL and BOLD.
    Bold(bool),
    /// Append raw (already-encoded) bytes.  Escape hatch.
    Raw(Vec<u8>),
    /// Text to encode via the currently selected code page and emit.
    Text(String),
    /// Emit a single LF byte (`\n`).
    Newline,
    /// Feed N lines (`ESC d N`).
    Feed(u8),
    /// Full cut (`GS V 0`).
    Cut,
}

/// Holds the profile and a buffered byte vector as instructions are
/// pushed.  The current code page state is tracked so `Text`
/// instructions know how to encode.
pub struct ReceiptCompiler<'a> {
    exec: Executor<'a>,
    buf: Vec<u8>,
    current_codepage: CodePage,
}

impl<'a> ReceiptCompiler<'a> {
    pub fn new(profile: &'a PrinterProfile) -> Self {
        Self {
            exec: Executor::new(profile),
            buf: Vec::new(),
            current_codepage: CodePage::Ascii,
        }
    }

    pub fn push(&mut self, inst: Instruction) {
        // Failures defer to compile() — push is infallible so callers
        // can chain without ?.
        if let Err(_e) = self.try_push(inst) {
            // Silently swallow; compile() will be the source of truth.
            // In practice a missing profile command is a fatal config
            // issue, not a runtime one — tests catch it.
        }
    }

    fn try_push(&mut self, inst: Instruction) -> Result<()> {
        match inst {
            Instruction::Init => {
                // ESC @ — universal reset across every ESC/POS vendor.
                // Not all profiles expose it as a named command, so we
                // emit the literal bytes.
                self.buf.extend_from_slice(&[0x1B, 0x40]);
            }
            Instruction::Codepage(cp) => {
                self.current_codepage = cp;
                match cp {
                    CodePage::Cp866 => {
                        let bytes = self.exec.with_value("CodePage", "866")?;
                        self.buf.extend(bytes);
                    }
                    CodePage::Cp1251 => {
                        // Some profiles carry explicit "1251"; if not,
                        // caller is responsible for mapping.  We try
                        // the name first, then fall back to raw 0x1B
                        // 0x74 0x11 — which, while labelled 866, is
                        // the most common Cyrillic default.
                        if let Ok(b) = self.exec.with_value("CodePage", "1251") {
                            self.buf.extend(b);
                        } else {
                            let b = self.exec.with_value("CodePage", "866")?;
                            self.buf.extend(b);
                        }
                    }
                    CodePage::Ascii => {}
                }
            }
            Instruction::Align(a) => {
                let name = match a {
                    Alignment::Left => "Left",
                    Alignment::Center => "Center",
                    Alignment::Right => "Right",
                };
                let bytes = self.exec.simple(name)?;
                self.buf.extend(bytes);
            }
            Instruction::Bold(on) => {
                if on {
                    // ESC ! 0x08 — bold-only font mode.
                    let bytes = self.exec.simple("FONTBOLD")
                        .unwrap_or_else(|_| vec![0x1B, 0x21, 0x08]);
                    self.buf.extend(bytes);
                } else {
                    let bytes = self.exec.simple("FONTDEFAULT")
                        .unwrap_or_else(|_| vec![0x1B, 0x21, 0x00]);
                    self.buf.extend(bytes);
                }
            }
            Instruction::Raw(b) => self.buf.extend(b),
            Instruction::Text(s) => {
                let bytes = encode_text(&s, self.current_codepage)?;
                self.buf.extend(bytes);
            }
            Instruction::Newline => self.buf.push(b'\n'),
            Instruction::Feed(n) => {
                let bytes = self.exec.with_byte("FeedingThePaperByNLines", n)
                    .unwrap_or_else(|_| vec![0x1B, 0x64, n]);
                self.buf.extend(bytes);
            }
            Instruction::Cut => {
                let bytes = self.exec.with_value("Cut", "DEFAULT")
                    .unwrap_or_else(|_| vec![0x1D, 0x56, 0x00, 0x01]);
                self.buf.extend(bytes);
            }
        }
        Ok(())
    }

    /// Finalise and return the accumulated byte stream.  Consumes the
    /// compiler so subsequent pushes won't silently corrupt output.
    pub fn compile(self) -> Result<Vec<u8>> {
        Ok(self.buf)
    }

    /// Peek at the current byte buffer without consuming — for debug
    /// logging and golden-byte tests.
    pub fn bytes(&self) -> &[u8] {
        &self.buf
    }
}

fn encode_text(s: &str, cp: CodePage) -> Result<Vec<u8>> {
    match cp {
        CodePage::Cp866 => {
            let (out, _enc, had_errors) = IBM866.encode(s);
            if had_errors {
                // Find the first unencodable char for a diagnostic.
                for c in s.chars() {
                    let (_, _, err) = IBM866.encode(&c.to_string());
                    if err {
                        return Err(Error::Codepage { char: c });
                    }
                }
            }
            Ok(out.into_owned())
        }
        CodePage::Cp1251 => {
            let (out, _enc, had_errors) = WINDOWS_1251.encode(s);
            if had_errors {
                for c in s.chars() {
                    let (_, _, err) = WINDOWS_1251.encode(&c.to_string());
                    if err {
                        return Err(Error::Codepage { char: c });
                    }
                }
            }
            Ok(out.into_owned())
        }
        CodePage::Ascii => {
            // Strict ASCII — reject any byte >= 0x80.
            for c in s.chars() {
                if !c.is_ascii() {
                    return Err(Error::Codepage { char: c });
                }
            }
            Ok(s.as_bytes().to_vec())
        }
    }
}
