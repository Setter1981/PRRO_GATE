//! Parser for SoftBalance printer-profile XML.
//!
//! The original format has BOM + CRLF + Cyrillic comments — handled
//! transparently by `quick-xml` in UTF-8 mode.  The parser is forgiving:
//! it skips unknown tags so future profile additions don't break
//! loading.

use crate::error::{Error, Result};
use quick_xml::events::Event;
use quick_xml::reader::Reader;

/// Full printer profile loaded from a vendor XML file.
#[derive(Debug, Clone, Default)]
pub struct PrinterProfile {
    pub name: String,
    pub full_name: String,
    pub version: String,
    /// Comma-separated interface list (COM/LPT/LAN).
    pub interfaces: String,
    pub commands: Vec<Command>,
    pub procedures: Vec<Procedure>,
}

impl PrinterProfile {
    /// Parse a profile from UTF-8 XML bytes.  BOM is tolerated.
    pub fn from_xml_str(xml: &str) -> Result<Self> {
        let xml = xml.trim_start_matches('\u{FEFF}'); // strip BOM
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut profile = PrinterProfile::default();
        let mut buf = Vec::new();
        let mut stack: Vec<String> = Vec::new();
        let mut current_command: Option<Command> = None;
        let mut current_procedure: Option<Procedure> = None;
        let mut text_target: Option<&mut String> = None;

        loop {
            match reader.read_event_into(&mut buf) {
                Err(e) => return Err(Error::Xml(e.to_string())),
                Ok(Event::Eof) => break,
                Ok(Event::Start(e)) => {
                    let tag = e.name().as_ref().to_ascii_lowercase();
                    let tag = String::from_utf8_lossy(&tag).into_owned();
                    stack.push(tag.clone());

                    match tag.as_str() {
                        "command" => {
                            let mut cmd = Command::default();
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref().to_ascii_lowercase();
                                let val = attr.unescape_value()
                                    .map_err(|e| Error::Xml(e.to_string()))?
                                    .into_owned();
                                match key.as_slice() {
                                    b"name" => cmd.name = val,
                                    b"code" => cmd.code_hex = val,
                                    b"length" => cmd.length = val.parse().unwrap_or(0),
                                    _ => {}
                                }
                            }
                            current_command = Some(cmd);
                        }
                        "value" if current_command.is_some() => {
                            let mut value = CommandValue::default();
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref().to_ascii_lowercase();
                                let val = attr.unescape_value()
                                    .map_err(|e| Error::Xml(e.to_string()))?
                                    .into_owned();
                                match key.as_slice() {
                                    b"byte" => value.byte_offset = val.parse().unwrap_or(0),
                                    b"name" => value.name = val,
                                    b"hvalue" => value.hex_value = Some(val),
                                    b"dvalue" => value.decimal_value =
                                        Some(val.parse().unwrap_or(0)),
                                    _ => {}
                                }
                            }
                            if let Some(cmd) = current_command.as_mut() {
                                cmd.values.push(value);
                            }
                        }
                        "procedure" => {
                            let mut proc = Procedure::default();
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref().to_ascii_lowercase();
                                let val = attr.unescape_value()
                                    .map_err(|e| Error::Xml(e.to_string()))?
                                    .into_owned();
                                match key.as_slice() {
                                    b"name" => proc.name = val,
                                    b"title" => proc.title = val,
                                    _ => {}
                                }
                            }
                            current_procedure = Some(proc);
                        }
                        "prm" if current_procedure.is_some() => {
                            let mut prm = ProcedureParam::default();
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref().to_ascii_lowercase();
                                let val = attr.unescape_value()
                                    .map_err(|e| Error::Xml(e.to_string()))?
                                    .into_owned();
                                match key.as_slice() {
                                    b"parameter" => prm.parameter_index =
                                        val.parse().unwrap_or(0),
                                    b"command" => prm.command_name = val,
                                    _ => {}
                                }
                            }
                            if let Some(proc) = current_procedure.as_mut() {
                                proc.params.push(prm);
                            }
                        }
                        "name" => text_target = Some(&mut profile.name),
                        "fullname" => text_target = Some(&mut profile.full_name),
                        "version" => text_target = Some(&mut profile.version),
                        "interface" => text_target = Some(&mut profile.interfaces),
                        _ => {}
                    }
                }
                Ok(Event::End(e)) => {
                    let tag = e.name().as_ref().to_ascii_lowercase();
                    let tag = String::from_utf8_lossy(&tag).into_owned();
                    let _ = stack.pop();
                    text_target = None;
                    match tag.as_str() {
                        "command" => {
                            if let Some(cmd) = current_command.take() {
                                if !cmd.name.is_empty() {
                                    profile.commands.push(cmd);
                                }
                            }
                        }
                        "procedure" => {
                            if let Some(proc) = current_procedure.take() {
                                if !proc.name.is_empty() {
                                    profile.procedures.push(proc);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Empty(e)) => {
                    // Self-closing equivalents of Start→End.
                    let tag = e.name().as_ref().to_ascii_lowercase();
                    let tag = String::from_utf8_lossy(&tag).into_owned();
                    match tag.as_str() {
                        "command" => {
                            let mut cmd = Command::default();
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref().to_ascii_lowercase();
                                let val = attr.unescape_value()
                                    .map_err(|e| Error::Xml(e.to_string()))?
                                    .into_owned();
                                match key.as_slice() {
                                    b"name" => cmd.name = val,
                                    b"code" => cmd.code_hex = val,
                                    b"length" => cmd.length = val.parse().unwrap_or(0),
                                    _ => {}
                                }
                            }
                            if !cmd.name.is_empty() {
                                profile.commands.push(cmd);
                            }
                        }
                        "value" if current_command.is_some() => {
                            let mut value = CommandValue::default();
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref().to_ascii_lowercase();
                                let val = attr.unescape_value()
                                    .map_err(|e| Error::Xml(e.to_string()))?
                                    .into_owned();
                                match key.as_slice() {
                                    b"byte" => value.byte_offset = val.parse().unwrap_or(0),
                                    b"name" => value.name = val,
                                    b"hvalue" => value.hex_value = Some(val),
                                    b"dvalue" => value.decimal_value =
                                        Some(val.parse().unwrap_or(0)),
                                    _ => {}
                                }
                            }
                            if let Some(cmd) = current_command.as_mut() {
                                cmd.values.push(value);
                            }
                        }
                        "prm" if current_procedure.is_some() => {
                            let mut prm = ProcedureParam::default();
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref().to_ascii_lowercase();
                                let val = attr.unescape_value()
                                    .map_err(|e| Error::Xml(e.to_string()))?
                                    .into_owned();
                                match key.as_slice() {
                                    b"parameter" => prm.parameter_index =
                                        val.parse().unwrap_or(0),
                                    b"command" => prm.command_name = val,
                                    _ => {}
                                }
                            }
                            if let Some(proc) = current_procedure.as_mut() {
                                proc.params.push(prm);
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Text(t)) => {
                    if let Some(target) = text_target.as_mut() {
                        let text = t.unescape()
                            .map_err(|e| Error::Xml(e.to_string()))?
                            .into_owned();
                        target.push_str(&text);
                    }
                }
                _ => {}
            }
            buf.clear();
        }

        Ok(profile)
    }

    pub fn command(&self, name: &str) -> Option<&Command> {
        self.commands.iter().find(|c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn procedure(&self, name: &str) -> Option<&Procedure> {
        self.procedures.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }
}

/// One named command: base hex bytes + optional enum of values.
#[derive(Debug, Clone, Default)]
pub struct Command {
    pub name: String,
    /// Hex-encoded base bytes, e.g. `"1D5600"` for `GS V 0`.
    pub code_hex: String,
    /// Declared total length of the emitted bytes (base + values).
    pub length: u32,
    pub values: Vec<CommandValue>,
}

impl Command {
    /// Raw base bytes decoded from `code_hex`.
    pub fn base_bytes(&self) -> Result<Vec<u8>> {
        if self.code_hex.is_empty() || self.code_hex == "0" || self.code_hex == "00" {
            // "length=0" sentinel commands (Text / Array / Mode) have
            // no fixed bytes — handled by the executor separately.
            return Ok(Vec::new());
        }
        hex::decode(&self.code_hex).map_err(|_| Error::InvalidHex {
            command: self.name.clone(),
            found: self.code_hex.clone(),
        })
    }

    pub fn value_by_name(&self, name: &str) -> Option<&CommandValue> {
        self.values.iter().find(|v| v.name.eq_ignore_ascii_case(name))
    }
}

/// One enum-like value option inside a command, e.g.
/// `<value byte="2" name="Center" dvalue="1" />` for alignment.
#[derive(Debug, Clone, Default)]
pub struct CommandValue {
    /// Which byte position inside the emitted command this value
    /// overrides.  In practice always 2 (last byte).
    pub byte_offset: u32,
    pub name: String,
    /// Hex form (`"11"` → 0x11). Preferred when present.
    pub hex_value: Option<String>,
    /// Decimal form (`"66"` → 0x42).
    pub decimal_value: Option<u32>,
}

impl CommandValue {
    /// Resolve the single byte that this value contributes.
    pub fn byte(&self) -> Result<u8> {
        if let Some(h) = &self.hex_value {
            u8::from_str_radix(h, 16).map_err(|_| Error::InvalidHex {
                command: self.name.clone(),
                found: h.clone(),
            })
        } else if let Some(d) = self.decimal_value {
            if d > u8::MAX as u32 {
                return Err(Error::InvalidHex {
                    command: self.name.clone(),
                    found: d.to_string(),
                });
            }
            Ok(d as u8)
        } else {
            Err(Error::MissingParameter {
                command: self.name.clone(),
            })
        }
    }
}

/// Composite command.  Binds a sequence of `(parameter_index,
/// command_name)` pairs so that callers supply parameters positionally.
#[derive(Debug, Clone, Default)]
pub struct Procedure {
    pub name: String,
    /// Comma-separated parameter title list, documentation only.
    pub title: String,
    pub params: Vec<ProcedureParam>,
}

#[derive(Debug, Clone, Default)]
pub struct ProcedureParam {
    pub parameter_index: u32,
    pub command_name: String,
}
