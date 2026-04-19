//! Unified key-container reader for the Ukrainian PRRO ecosystem.
//!
//! Every accredited Ukrainian CA ships private keys in its own
//! file format. The three structural families we support:
//!
//! - **JKS** — used by ПриватБанк-issued keys. Magic `0xFEEDFEED`,
//!   SHA1-XOR keystream cipher (Sun's legacy keystore format).
//! - **PFX** — RFC 7292 PKCS#12 with a Ukraine-specific PBES2 variant
//!   (PBKDF2-HMAC-GOST34311 + GOST 28147 CFB). Files with `.pfx`,
//!   `.p12`, and `.zs2` extensions all fall under this structural
//!   family — `.zs2` (АЦСК "Україна") is PKCS#12 with DSTU OIDs.
//!   See [`ContainerFormat`] docs for the rationale.
//! - **Key-6.dat** — ДПС-issued keys via ІІТ ("Інфотех") tooling.
//!   Iterated GOST 34.311 KDF + GOST 28147 ECB.
//!
//! ## Pipeline
//!
//! ```text
//!   bytes  ──►  detect_format()  ──►  format-specific parser
//!                                       │
//!                                       ▼
//!                                  ExtractedKey {
//!                                    format,
//!                                    param_d: Zeroizing<[u8; 32]>,
//!                                    certs: Vec<Vec<u8>>,
//!                                  }
//!                                       │
//!                                       ▼
//!                              prro_crypto::sign(...)
//! ```
//!
//! All three formats are fully implemented and cross-validated
//! against real production keys from ДПС, АЦСК "Україна", and
//! ПриватБанк (same `param_d` extracted from PFX and Key-6.dat of
//! the same key pair).
//! the public API does not change when that happens.

use super::der;
use super::jks;
use super::key6;
use super::pfx;

/// Family of supported key container formats, classified by *byte
/// structure only*. Filename / extension is explicitly NOT part of the
/// identity — the same PKCS#12 bytes served as `.pfx`, `.p12`, or
/// `.zs2` all fall under [`ContainerFormat::Pfx`].
///
/// In particular: files with a `.zs2` extension (АЦСК "Україна" keys)
/// are structurally PKCS#12 and are classified as [`Pfx`]. The
/// previous `Zs2` enum variant was false polymorphism — it advertised
/// a distinction that signature-based detection cannot make, which
/// would tempt upper layers to attach different trust/policy to ZS2
/// versus PFX purely on filename. That trust path is now closed by
/// construction: pass a `.zs2` and you get `Pfx`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ContainerFormat {
    /// Sun legacy Java KeyStore (`0xFEEDFEED`). PrivatBank-issued keys.
    Jks,
    /// PKCS#12 / PFX (RFC 7292) — includes `.zs2` files from АЦСК
    /// "Україна" which use the same structural layout with
    /// DSTU/GOST-specific algorithm OIDs.
    Pfx,
    /// ІІТ "Key-6.dat" container. ДПС-issued keys via ІІТ tooling.
    Key6Dat,
}

/// Normalized key extract returned from the dispatcher.
///
/// **Secret material.** `param_d` is the raw DSTU private scalar —
/// wrapped in `Zeroizing<[u8; 32]>` so it's zeroed on drop without
/// blocking `certs` move. Custom `Debug` redacts bytes.
#[derive(Clone)]
pub struct ExtractedKey {
    pub format: ContainerFormat,
    /// DSTU 4145 private scalar (`param_d`) packed as 32 little-endian
    /// bytes — exactly the shape that `bytes_le_to_field` consumes
    /// when constructing the signing input.
    pub param_d: zeroize::Zeroizing<[u8; 32]>,
    /// Certificate chain stored alongside the key, in container order.
    /// Empty for formats that ship only the key.
    pub certs: Vec<Vec<u8>>,
}

impl std::fmt::Debug for ExtractedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtractedKey")
            .field("format", &self.format)
            .field("param_d", &"<redacted>")
            .field("certs", &format_args!("[{} cert(s)]", self.certs.len()))
            .finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ContainerError {
    #[error("unrecognised container format (no known magic / structure match)")]
    UnknownFormat,
    #[error("recognised {0:?} but parser not yet implemented in this build")]
    ParserNotImplemented(ContainerFormat),
    #[error("JKS read failure: {0}")]
    Jks(#[from] jks::JksError),
    #[error("DER / param_d extraction failure: {0}")]
    Der(String),
    #[error("Key-6: {0}")]
    Key6(#[from] key6::Key6Error),
    #[error("PFX/ZS2: {0}")]
    Pfx(#[from] pfx::PfxError),
    #[error("extracted private key has unexpected width: got {0} bytes, expected 32")]
    BadKeyWidth(usize),
}

/// Sniff the container format from the first bytes. Returns `None` when
/// nothing matches — the caller can then try `extract_private_key`
/// which will surface the same outcome as a typed error.
pub fn detect_format(data: &[u8]) -> Option<ContainerFormat> {
    // JKS magic: 4 bytes 0xFEEDFEED, big-endian.
    if data.len() >= 4 && data[..4] == [0xFE, 0xED, 0xFE, 0xED] {
        return Some(ContainerFormat::Jks);
    }
    // Key-6.dat: the IIT cryptType OID (1.3.6.1.4.1.19398.1.1.1.2) lives
    // at a known offset inside the outer/inner SEQUENCE pair. Searching
    // for the encoded OID bytes in the first ~30 bytes is a cheap and
    // reliable sniff that survives small layout variations between IIT
    // tooling versions.
    if looks_like_key6(data) {
        return Some(ContainerFormat::Key6Dat);
    }
    // PFX/PKCS#12 starts with an ASN.1 SEQUENCE then a `version`
    // INTEGER of value 3. The minimal sniff: first byte SEQUENCE tag
    // (0x30) followed by a long-form length and an INTEGER 0x02 0x01 0x03
    // somewhere in the first few bytes. We do the cheap structural
    // check; the parser stub will reject anything that pretends harder.
    if looks_like_pfx(data) {
        return Some(ContainerFormat::Pfx);
    }
    None
}

/// Single entry point for any supported container.
///
/// Detects the format, dispatches to the matching parser, and returns
/// the extracted DSTU private scalar plus any embedded certs in a
/// uniform shape. `ParserNotImplemented` is returned for formats whose
/// detector matches but whose parser hasn't shipped yet — the caller
/// can decide whether to fall back to an external loader (Python
/// `dstucrypt/agent`, jkurwa shell-out, etc.) until that lands.
pub fn extract_private_key(
    data: &[u8],
    password: &str,
) -> Result<ExtractedKey, ContainerError> {
    let format = detect_format(data).ok_or(ContainerError::UnknownFormat)?;
    match format {
        ContainerFormat::Jks => extract_from_jks(data, password),
        ContainerFormat::Key6Dat => extract_from_key6(data, password),
        ContainerFormat::Pfx => extract_from_pfx(data, password, format),
    }
}

fn extract_from_pfx(
    data: &[u8],
    password: &str,
    format: ContainerFormat,
) -> Result<ExtractedKey, ContainerError> {
    let parsed = pfx::parse(data, password.as_bytes())?;
    if parsed.param_d.len() != 32 {
        return Err(ContainerError::BadKeyWidth(parsed.param_d.len()));
    }
    let mut param_d = [0u8; 32];
    param_d.copy_from_slice(&parsed.param_d);
    Ok(ExtractedKey {
        format,
        param_d: zeroize::Zeroizing::new(param_d),
        certs: parsed.certs.clone(),
    })
}

fn extract_from_key6(data: &[u8], password: &str) -> Result<ExtractedKey, ContainerError> {
    let parsed = key6::parse(data, password.as_bytes())?;
    if parsed.param_d.len() != 32 {
        return Err(ContainerError::BadKeyWidth(parsed.param_d.len()));
    }
    let mut param_d = [0u8; 32];
    param_d.copy_from_slice(&parsed.param_d);
    Ok(ExtractedKey {
        format: ContainerFormat::Key6Dat,
        param_d: zeroize::Zeroizing::new(param_d),
        // Key-6.dat carries only the encrypted key; the cert chain lives
        // alongside it in the parent ZS2 (or in a separate .crt). The
        // unified `ExtractedKey` shape returns an empty cert list here
        // — callers that need a cert must source it elsewhere.
        certs: Vec::new(),
    })
}

/// JKS-specific path: read the keystore, extract `param_d` from the
/// PKCS#8-wrapped private key inside, copy the cert chain.
fn extract_from_jks(data: &[u8], password: &str) -> Result<ExtractedKey, ContainerError> {
    let entry = jks::read_jks(data, password)?;
    let d_bytes =
        der::extract_param_d(&entry.key_der).map_err(|e| ContainerError::Der(e.to_string()))?;
    if d_bytes.len() != 32 {
        return Err(ContainerError::BadKeyWidth(d_bytes.len()));
    }
    let mut param_d = [0u8; 32];
    param_d.copy_from_slice(&d_bytes);
    Ok(ExtractedKey {
        format: ContainerFormat::Jks,
        param_d: zeroize::Zeroizing::new(param_d),
        certs: entry.certs.clone(),
    })
}

/// IIT cryptType OID DER-encoded form. We byte-search for this marker
/// inside the first ~30 bytes of the file rather than parsing ASN.1
/// twice — the dispatch is cheap and tolerant of small layout drift.
const IIT_OID_DER: &[u8] = &[
    0x06, 0x0C, 0x2B, 0x06, 0x01, 0x04, 0x01, 0x81, 0x97, 0x46, 0x01, 0x01, 0x01, 0x02,
];

fn looks_like_key6(data: &[u8]) -> bool {
    if data.len() < 4 || data[0] != 0x30 {
        return false;
    }
    let scan_window = data.len().min(40);
    data[..scan_window]
        .windows(IIT_OID_DER.len())
        .any(|w| w == IIT_OID_DER)
}

fn looks_like_pfx(data: &[u8]) -> bool {
    // PKCS#12 PFX outer SEQUENCE. Long-form length always (real files are
    // > 256 bytes), so structure is `30 82 LL LL` followed by `02 01 03`
    // for `version INTEGER 3`. Scan up to the first 16 bytes to keep the
    // sniff cheap.
    if data.len() < 8 || data[0] != 0x30 {
        return false;
    }
    // After SEQUENCE tag + 1..5 length bytes, look for the version INTEGER.
    let length_form = data[1];
    let header_len = if length_form < 0x80 {
        2
    } else {
        2 + (length_form & 0x7F) as usize
    };
    if data.len() < header_len + 3 {
        return false;
    }
    // version INTEGER 3: tag 0x02, len 0x01, value 0x03.
    data[header_len] == 0x02 && data[header_len + 1] == 0x01 && data[header_len + 2] == 0x03
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Detector must recognise the production JKS we already test against.
    #[test]
    fn detector_classifies_real_jks() {
        let path = "/mnt/d/PRRO_GATE/key_13667753_13667753 (2).jks";
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(_) => {
                eprintln!("SKIP: production JKS not available");
                return;
            }
        };
        assert_eq!(detect_format(&data), Some(ContainerFormat::Jks));
    }

    /// End-to-end JKS adapter: file → ExtractedKey, with `param_d` matching
    /// the value `der::extract_param_d` returns for the same JKS.
    #[test]
    fn extract_from_real_jks_matches_legacy_path() {
        let path = "/mnt/d/PRRO_GATE/key_13667753_13667753 (2).jks";
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(_) => return,
        };
        let extracted = extract_private_key(&data, "Jrcfyf123")
            .expect("real JKS must extract");

        // Cross-check against the existing direct path used by
        // `tests/test_e2e_jks_sign.rs`.
        let entry = jks::read_jks(&data, "Jrcfyf123").unwrap();
        let legacy_d = der::extract_param_d(&entry.key_der).unwrap();
        assert_eq!(&extracted.param_d[..], legacy_d.as_slice());
        assert_eq!(extracted.format, ContainerFormat::Jks);
        assert!(!extracted.certs.is_empty(), "JKS must carry cert chain");
    }

    /// Wrong password must surface as `ContainerError::Jks` (BadPassword
    /// inside), not panic, not generic.
    #[test]
    fn jks_wrong_password_returns_typed_error() {
        let path = "/mnt/d/PRRO_GATE/key_13667753_13667753 (2).jks";
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(_) => return,
        };
        let r = extract_private_key(&data, "wrong");
        assert!(matches!(r, Err(ContainerError::Jks(jks::JksError::BadPassword))));
    }

    /// PFX detector spot check: a synthesised SEQUENCE { INTEGER 3, ... }
    /// passes the structural sniff (real files don't exist in tree yet).
    #[test]
    fn detector_recognises_pfx_outer_shape() {
        // 30 82 12 34   02 01 03   (rest doesn't matter for the sniff)
        let buf = [0x30, 0x82, 0x12, 0x34, 0x02, 0x01, 0x03, 0x00, 0x00];
        assert_eq!(detect_format(&buf), Some(ContainerFormat::Pfx));
    }

    /// Synthetic PFX header that passes the structural sniff but fails
    /// inner ASN.1 parse — must surface as a `Pfx(...)` error, not
    /// panic. Confirms the dispatcher correctly routes recognised PFX
    /// shapes to the new parser instead of returning `ParserNotImplemented`.
    #[test]
    fn pfx_parser_rejects_malformed_inner_structure() {
        let buf = [0x30, 0x82, 0x12, 0x34, 0x02, 0x01, 0x03, 0x00, 0x00];
        let r = extract_private_key(&buf, "");
        assert!(matches!(r, Err(ContainerError::Pfx(_))), "got {:?}", r);
    }

    #[test]
    fn unknown_format_returns_unknown_error() {
        let buf = [0x00u8; 32];
        assert!(matches!(
            extract_private_key(&buf, ""),
            Err(ContainerError::UnknownFormat)
        ));
    }

    /// PFX/ZS2 dispatch on real legal-entity ZS2.
    #[test]
    fn extract_from_real_pfx_dispatches_correctly() {
        let path = "/mnt/d/PRRO_GATE/39197544_U250703163535.ZS2";
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(_) => return,
        };
        assert_eq!(detect_format(&data), Some(ContainerFormat::Pfx));
        let extracted =
            extract_private_key(&data, "061082").expect("ZS2 must extract");
        assert_eq!(extracted.format, ContainerFormat::Pfx);
        assert_eq!(extracted.param_d.len(), 32);
        // ZS2 carries the cert chain alongside the encrypted key.
        // For this fixture there is at least the signing cert.
        // (Empty is also acceptable if the bag layout is key-only.)
    }

    /// **Critical correctness gate.** Two independent parsers
    /// (PFX/ZS2 path and Key-6 path) on two different file formats must
    /// produce byte-identical `param_d` for the same private key. Both
    /// fixture files (`_U250703163535.ZS2` and `_U250703163535.dat`) are
    /// the same legal-entity signing key — one is the original `.ZS2`,
    /// the other was converted to `.dat` by the user's external tooling.
    #[test]
    fn cross_validation_pfx_vs_key6_legal_entity() {
        let zs2 = match std::fs::read("/mnt/d/PRRO_GATE/39197544_U250703163535.ZS2") {
            Ok(d) => d,
            Err(_) => return,
        };
        let dat = match std::fs::read("/mnt/d/PRRO_GATE/39197544_U250703163535.dat") {
            Ok(d) => d,
            Err(_) => return,
        };
        let from_pfx = extract_private_key(&zs2, "061082").unwrap();
        let from_key6 = extract_private_key(&dat, "061082").unwrap();
        assert_eq!(from_pfx.param_d, from_key6.param_d,
            "PFX/ZS2 vs Key-6 byte-diff on same private key — one of the parsers is wrong");
        assert_eq!(from_pfx.format, ContainerFormat::Pfx);
        assert_eq!(from_key6.format, ContainerFormat::Key6Dat);
    }

    /// Same check on the director's key — covers the second fixture pair
    /// and rules out "the parsers happen to agree on one specific
    /// layout but diverge on another".
    #[test]
    fn cross_validation_pfx_vs_key6_director() {
        let zs2 =
            match std::fs::read("/mnt/d/PRRO_GATE/39197544_2790008754_DU250703163535.ZS2") {
                Ok(d) => d,
                Err(_) => return,
            };
        let dat =
            match std::fs::read("/mnt/d/PRRO_GATE/39197544_2790008754_DU250703163535.dat") {
                Ok(d) => d,
                Err(_) => return,
            };
        let from_pfx = extract_private_key(&zs2, "061082").unwrap();
        let from_key6 = extract_private_key(&dat, "061082").unwrap();
        assert_eq!(from_pfx.param_d, from_key6.param_d);
    }

    /// End-to-end Key-6.dat dispatcher: real .dat file → ExtractedKey
    /// with format=Key6Dat and 32-byte param_d.
    #[test]
    fn extract_from_real_key6_dispatches_correctly() {
        let path = "/mnt/d/PRRO_GATE/39197544_U250703163535.dat";
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(_) => return,
        };
        // Detector must classify as Key-6 (not PFX, not Unknown).
        assert_eq!(detect_format(&data), Some(ContainerFormat::Key6Dat));
        let extracted =
            extract_private_key(&data, "061082").expect("real Key-6 must extract");
        assert_eq!(extracted.format, ContainerFormat::Key6Dat);
        assert_eq!(extracted.param_d.len(), 32);
        // Cert chain is empty for bare Key-6 (cert lives alongside in ZS2).
        assert!(extracted.certs.is_empty());
    }
}
