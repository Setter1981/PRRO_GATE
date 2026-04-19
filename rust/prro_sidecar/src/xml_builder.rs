//! Byte-identical port of `dps_xml.py` — canonical JSON → cp1251 XML.
//! Phase 2 implements full build_* functions; this is a Phase 0 stub.

#![allow(dead_code)]

use crate::input::CanonicalCommand;

pub fn build(_cmd: &CanonicalCommand) -> Result<Vec<u8>, crate::errors::SidecarError> {
    unimplemented!("xml_builder: Phase 2 stub")
}
