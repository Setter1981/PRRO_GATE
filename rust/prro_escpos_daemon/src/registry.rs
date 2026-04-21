//! In-memory registry of loaded printer profiles.
//!
//! Bundled profiles from [`prro_escpos::bundled`] register on startup.
//! Operators can add their own via a config directory; new profiles
//! in `profiles_dir` are glob-loaded at startup.

use prro_escpos::PrinterProfile;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct ProfileRegistry {
    inner: Arc<BTreeMap<String, PrinterProfile>>,
}

impl ProfileRegistry {
    pub fn new() -> Self {
        let mut map: BTreeMap<String, PrinterProfile> = BTreeMap::new();
        for (key, xml) in [
            ("tm-t88ii", prro_escpos::bundled::EPSON_TM_T88II),
            ("pp8000l", prro_escpos::bundled::POSIFLEX_PP_8000_LAN),
            ("cts310ii", prro_escpos::bundled::CITIZEN_CT_S310II),
        ] {
            if let Ok(profile) = PrinterProfile::from_xml_str(xml) {
                map.insert(key.to_string(), profile);
            }
        }
        Self { inner: Arc::new(map) }
    }

    /// Load all `*.xml` files from `dir` alongside bundled profiles.
    /// File stem becomes the profile key (collision overrides bundled).
    pub fn with_extra_dir(mut self, dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        let mut map = (*self.inner).clone();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("xml") {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                let Ok(xml) = std::fs::read_to_string(&path) else {
                    continue;
                };
                if let Ok(profile) = PrinterProfile::from_xml_str(&xml) {
                    map.insert(stem.to_string(), profile);
                }
            }
        }
        self.inner = Arc::new(map);
        self
    }

    pub fn get(&self, key: &str) -> Option<&PrinterProfile> {
        self.inner.get(key)
    }

    pub fn list(&self) -> Vec<ProfileSummary> {
        self.inner
            .iter()
            .map(|(key, p)| ProfileSummary {
                key: key.clone(),
                name: p.name.clone(),
                full_name: p.full_name.clone(),
                version: p.version.clone(),
                interfaces: p.interfaces.clone(),
                command_count: p.commands.len(),
                procedure_count: p.procedures.len(),
            })
            .collect()
    }
}

impl Default for ProfileRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProfileSummary {
    pub key: String,
    pub name: String,
    pub full_name: String,
    pub version: String,
    pub interfaces: String,
    pub command_count: usize,
    pub procedure_count: usize,
}
