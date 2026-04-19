//! IIT "Key-6.dat" private-key container reader.
//!
//! The format Ukrainian ІІТ ("Інфотех") tooling produces and that ДПС
//! issues for tax authority signing. Compatible with `ZS2`-converted
//! files (АЦСК "Україна" tooling can also export to this layout).
//!
//! ## ASN.1 layout (port of jkurwa `lib/spec/keystore.js::StoreIIT`)
//!
//! ```text
//! StoreIIT ::= SEQUENCE {
//!     cryptParam SEQUENCE {
//!         cryptType OBJECT IDENTIFIER,           -- 1.3.6.1.4.1.19398.1.1.1.2
//!         cryptParam SEQUENCE {
//!             mac    OCTET STRING (4 bytes),     -- integrity tag (head of GOST34311 of decrypted)
//!             pad    OCTET STRING (0..7 bytes)   -- ECB padding to 8-byte block
//!         }
//!     },
//!     cryptData OCTET STRING                     -- GOST 28147 ECB encrypted DstuPrivkey
//! }
//! ```
//!
//! ## Decryption
//!
//! 1. Derive a 32-byte session key from the password via `dumb_kdf`:
//!    10 000 iterations of `GOST34311(prev_hash)` starting from
//!    `prev_hash = GOST34311(password)`. Mirrors `gost89/lib/util.js::dumb_kdf`
//!    + `gost89/lib/compat.js::convert_password` for `format == 'IIT'`.
//! 2. Decrypt `cryptData || pad` (each 8-byte block via [`Gost::decrypt64`]).
//! 3. Discard the trailing `pad` bytes — what remains is a DER-encoded
//!    `DstuPrivkey` SEQUENCE that [`crate::interop::prro::der::extract_param_d`]
//!    already understands.
//!
//! ## Test fixtures
//!
//! `/mnt/d/PRRO_GATE/39197544_*.dat` — converted from the matching `.ZS2`
//! files via the user's external utility, password `061082`. They share
//! the same private scalar with their `.ZS2` siblings, so a successful
//! parse is one half of the cross-validation against the PFX/ZS2 path.

use crate::core::hash::{gost28147::Gost, gost_34_311_95};
use crate::interop::prro::der::{
    self, DerError, Reader, TAG_OCTET_STRING, TAG_OID, TAG_SEQUENCE,
};

/// IIT cryptType OID (`1.3.6.1.4.1.19398.1.1.1.2`) in DER bytes.
/// We compare bytes-for-bytes rather than parsing the OID into arcs.
const IIT_STORE_OID_DER: &[u8] = &[
    0x2B, 0x06, 0x01, 0x04, 0x01, 0x81, 0x97, 0x46, 0x01, 0x01, 0x01, 0x02,
];

/// Iteration count for the IIT key-derivation chain. Hardcoded by the
/// format (no field carries it).
const IIT_KDF_ITERATIONS: usize = 10_000;

#[derive(Debug, thiserror::Error)]
pub enum Key6Error {
    #[error("ASN.1: {0}")]
    Der(#[from] DerError),
    #[error("not a Key-6 container (cryptType OID does not match IIT marker)")]
    NotKey6,
    #[error("invalid mac field length {0} (expected 4)")]
    BadMacLength(usize),
    #[error("invalid pad field length {0} (must be < 8)")]
    BadPadLength(usize),
    #[error("ciphertext length {0} not multiple of 8 (GOST 28147 block)")]
    BadCiphertextLength(usize),
    #[error("decrypted plaintext shorter than declared pad (would underflow)")]
    PadUnderflow,
    #[error("integrity check failed — wrong password or corrupted container")]
    BadPassword,
    #[error("inner DSTU privkey parse: {0}")]
    InnerPrivkey(String),
}

/// Output of a successful Key-6 parse.
///
/// **Secret material.** `param_d` wrapped in `Zeroizing` — wiped on
/// drop. Custom `Debug` redacts bytes.
#[derive(Clone)]
pub struct Key6Parsed {
    /// DSTU 4145 private scalar bytes (little-endian, typically 32 bytes
    /// for PB-257). Pass through `bytes_le_to_field` to construct the
    /// signing input.
    pub param_d: zeroize::Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for Key6Parsed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Key6Parsed")
            .field("param_d", &"<redacted>")
            .finish()
    }
}

/// Parse a Key-6.dat container with the given password.
///
/// Returns the extracted DSTU private scalar.
///
/// # MAC handling — read carefully
///
/// The IIT Key-6.dat envelope stores a 4-byte MAC field, and the
/// `mac/pad` TLV *is* validated for shape (length == 4, pad < 8). The
/// MAC value itself is **intentionally NOT verified** — this mirrors
/// jkurwa's behaviour (`gost89/lib/compat.js::decode_data`) and was
/// preserved deliberately because:
///   - The IIT reference implementation's MAC is derived from a
///     non-standard construction without public spec. Re-implementing
///     it by experiment would risk accepting invalid MACs and rejecting
///     valid ones.
///   - A wrong password produces garbage plaintext that fails inner
///     DstuPrivkey ASN.1 parsing — caught as `Key6Error::BadPassword`
///     (we map inner DER parse failures to BadPassword because the only
///     realistic cause is wrong-key decryption producing garbage bytes).
///     So "wrong password" is already rejected, just via a structurally
///     different signal than a MAC compare would give.
///
/// **Security implication:** a containter whose encrypted body has
/// been bit-flipped by an attacker with access to the file AND the
/// exact structural constraints (block alignment, inner ASN.1 shape)
/// could in theory pass our check while carrying a different plain-
/// text. In practice this requires a file-write MITM on the
/// operator's disk — outside our threat model. If you widen the
/// threat model, implement MAC verification here before shipping.
pub fn parse(data: &[u8], password: &[u8]) -> Result<Key6Parsed, Key6Error> {
    // ── 1. Walk the ASN.1 envelope ──────────────────────────────────────
    let mut outer = Reader::new(data);
    let store_body = outer.expect(TAG_SEQUENCE)?;
    let mut store = Reader::new(store_body);

    // crypt_param SEQUENCE { OID, SEQUENCE { mac, pad } }
    let crypt_param_body = store.expect(TAG_SEQUENCE)?;
    let mut crypt_param = Reader::new(crypt_param_body);

    let oid_bytes = crypt_param.expect(TAG_OID)?;
    if oid_bytes != IIT_STORE_OID_DER {
        return Err(Key6Error::NotKey6);
    }

    let mac_pad_body = crypt_param.expect(TAG_SEQUENCE)?;
    let mut mac_pad = Reader::new(mac_pad_body);
    let mac = mac_pad.expect(TAG_OCTET_STRING)?;
    if mac.len() != 4 {
        return Err(Key6Error::BadMacLength(mac.len()));
    }
    let pad = mac_pad.expect(TAG_OCTET_STRING)?;
    if pad.len() >= 8 {
        return Err(Key6Error::BadPadLength(pad.len()));
    }

    // crypt_data OCTET STRING. The body length on its own is NOT
    // block-aligned — the `pad` field carries the trailing bytes that
    // bring `body || pad` up to a multiple of 8 so GOST 28147 ECB can
    // decrypt it block by block. We strip those tail bytes after
    // decryption and what remains is the raw DSTU privkey DER.
    let cipher_body = store.expect(TAG_OCTET_STRING)?;
    let total_len = cipher_body.len() + pad.len();
    if total_len % 8 != 0 {
        return Err(Key6Error::BadCiphertextLength(cipher_body.len()));
    }

    // ── 2. Derive session key from password ─────────────────────────────
    let key = dumb_kdf(password, IIT_KDF_ITERATIONS);

    // ── 3. Decrypt ciphertext (block-wise ECB) ──────────────────────────
    let mut cipher = Gost::with_dku();
    cipher.set_key(&key);

    // Concatenate body || pad in a single buffer for the block walk.
    let mut combined = Vec::with_capacity(total_len);
    combined.extend_from_slice(cipher_body);
    combined.extend_from_slice(pad);

    let mut plain = vec![0u8; total_len];
    let mut block_in = [0u8; 8];
    let mut block_out = [0u8; 8];
    for i in (0..total_len).step_by(8) {
        block_in.copy_from_slice(&combined[i..i + 8]);
        cipher.decrypt64(&block_in, &mut block_out);
        plain[i..i + 8].copy_from_slice(&block_out);
    }

    // Strip trailing pad bytes. After this `plain` is the raw DstuPrivkey
    // DER (length matches the original `body` field).
    plain.truncate(cipher_body.len());
    // `mac` is intentionally not verified. The IIT format stores a 4-byte
    // tag but jkurwa never checks it (`gost89/lib/compat.js::decode_data`
    // returns the plaintext untouched, and `Priv.from_asn1` then tries to
    // parse it). Wrong password ⇒ garbage plaintext ⇒ ASN.1 parse failure
    // ⇒ `Key6Error::BadPassword`. We keep the same convention.
    let _ = mac;

    // ── 4. Pull `param_d` out of the inner DstuPrivkey SEQUENCE ─────────
    //      A failure here is treated as the equivalent of "bad password" —
    //      because the only realistic way the inner DER fails to parse is
    //      that the decryption produced garbage from a wrong key.
    let param_d = der::extract_param_d(&plain)
        .map_err(|_| Key6Error::BadPassword)?;
    Ok(Key6Parsed {
        param_d: zeroize::Zeroizing::new(param_d),
    })
}

/// IIT "dumb" key-derivation function: iterated GOST 34.311 hash chain
/// over the password. Mirrors `gost89/lib/util.js::dumb_kdf`.
///
/// `h_0 = GOST34311(password)`, `h_i = GOST34311(h_{i-1})`, return `h_{iters-1}`.
/// The format hard-codes `iters = 10_000`.
pub fn dumb_kdf(password: &[u8], iterations: usize) -> [u8; 32] {
    if iterations == 0 {
        return [0u8; 32];
    }
    let mut h = gost_34_311_95(password);
    for _ in 1..iterations {
        h = gost_34_311_95(&h);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test fixture: real Key-6.dat converted from `_U250703163535.ZS2` by
    /// user's external utility. Same private scalar as the .ZS2 sibling,
    /// password `061082`. Skipped if file unavailable.
    #[test]
    fn parse_real_legal_entity_key6() {
        let path = "/mnt/d/PRRO_GATE/39197544_U250703163535.dat";
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(_) => {
                eprintln!("SKIP: {} not available", path);
                return;
            }
        };
        let parsed = parse(&data, b"061082").expect("must parse");
        // DSTU PB-257 private key is 32 bytes (256-bit scalar).
        assert!(
            parsed.param_d.len() >= 32 && parsed.param_d.len() <= 34,
            "param_d length {} unexpected for DSTU_PB_257",
            parsed.param_d.len()
        );
        // Never print the scalar bytes — it's a real private key and
        // `cargo test` output lands in CI logs, terminal scrollback,
        // and wrapper tee'd files. Size-only is enough for a smoke log.
        eprintln!("extracted param_d: {} bytes", parsed.param_d.len());
    }

    /// Director's personal key (paired with `_DU{date}.ZS2`). Sanity that
    /// the parser is not accidentally hard-wired to one specific layout —
    /// both real .dat files in the test fixture must parse with the same
    /// password.
    #[test]
    fn parse_real_director_key6() {
        let path = "/mnt/d/PRRO_GATE/39197544_2790008754_DU250703163535.dat";
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(_) => return,
        };
        let parsed = parse(&data, b"061082").expect("director key6 must parse");
        assert!(parsed.param_d.len() >= 32 && parsed.param_d.len() <= 34);
        // Size-only. Printing the scalar would leak the director's
        // private key into CI logs and terminal scrollback.
        eprintln!("director param_d: {} bytes", parsed.param_d.len());
    }

    /// Same Phase-1 chain but for a JKS container (PrivatBank key) —
    /// proves `compute_ski` works across different Ukrainian CAs, not
    /// just АЦСК "Україна". JKS also happens to carry the cert inside
    /// the container, so we can cross-verify two independent paths:
    ///   A) param_d → Q → compress → SKI  (the cert provisioning path)
    ///   B) cert.subjectPublicKey → compute_ski (the SKI we'd read from
    ///      a cert already on disk)
    /// Both must agree byte-for-byte.
    #[test]
    fn e2e_phase1_jks_ski_round_trip() {
        let jks_path = "/mnt/d/PRRO_GATE/key_13667753_13667753 (2).jks";
        let data = match std::fs::read(jks_path) { Ok(d) => d, Err(_) => return };
        let entry = crate::interop::prro::jks::read_jks(&data, "Jrcfyf123")
            .expect("JKS must decrypt");
        let d_bytes = crate::interop::prro::der::extract_param_d(&entry.key_der)
            .expect("must extract param_d");

        use crate::core::{curve::Curve, field::FieldEl, point::{compress_point, Point}};
        let curve = Curve::dstu_pb_257();
        let d = FieldEl::from_le_bytes(&d_bytes, curve.mod_words);
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
        let q = g.mul(&d, &curve).negate();
        let compressed = compress_point(&q, &curve);
        let ski_from_key = crate::cms::envelope::compute_ski(&compressed);

        // Path B: Find which cert in the chain has matching pubkey.
        // JKS may carry the CA chain (intermediates + root) alongside
        // the end-entity cert; the ordering isn't standardised.
        let mut matching_idx = None;
        for (i, cert) in entry.certs.iter().enumerate() {
            if let Ok(pk) = crate::cms::envelope::extract_cert_pubkey_bytes(cert) {
                let s = crate::cms::envelope::compute_ski(&pk);
                let hex = s.iter().map(|b| format!("{:02x}", b)).collect::<String>();
                eprintln!("  cert[{}] SKI: {}", i, hex);
                if s == ski_from_key {
                    matching_idx = Some(i);
                }
            } else {
                eprintln!("  cert[{}] SKI: (non-DSTU pubkey, skipped)", i);
            }
        }
        let idx = matching_idx.expect(
            "one of the JKS certs must have pubkey matching the decrypted private key",
        );
        eprintln!("end-entity is cert[{}]", idx);

        let hex = ski_from_key.iter().map(|b| format!("{:02x}", b)).collect::<String>().to_uppercase();
        eprintln!("JKS (Privat) end-entity SKI: {}", hex);
        assert_eq!(ski_from_key.len(), 32);
    }

    /// End-to-end Phase 1 sanity: the full chain ZS2 + password →
    /// param_d → pubkey Q → compressed Q → SKI must produce the SKI
    /// found in the corresponding cert. This proves we can rebuild a
    /// cert lookup key purely from the private key file, without ever
    /// having seen the cert itself — exactly what the cert provisioning
    /// service needs at operator-onboarding time.
    #[test]
    fn e2e_phase1_zs2_to_ski_matches_cert_extension() {
        let zs2_path = "/mnt/d/PRRO_GATE/39197544_2790008754_DU250703163535.ZS2";
        let cert_path = "/mnt/c/ProgramData/WebCheck/Keys/CA-0882240800000000000000000000000000000001.cer";
        let zs2 = match std::fs::read(zs2_path) { Ok(d) => d, Err(_) => return };
        let cert = match std::fs::read(cert_path) { Ok(d) => d, Err(_) => return };

        // 1. Decrypt → param_d
        let parsed = crate::interop::prro::pfx::parse(&zs2, b"061082").expect("ZS2 must parse");

        // 2. Derive pubkey: Q = -d·G
        use crate::core::{curve::Curve, field::FieldEl, point::{compress_point, Point}};
        let curve = Curve::dstu_pb_257();
        let d = FieldEl::from_le_bytes(&parsed.param_d, curve.mod_words);
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
        let q = g.mul(&d, &curve).negate();

        // 3. Compress Q
        let compressed = compress_point(&q, &curve);
        assert_eq!(compressed.len(), 33, "DSTU PB-257 compressed point is 33 bytes");

        // 4. Compute SKI from compressed Q
        let derived_ski = crate::cms::envelope::compute_ski(&compressed);
        let derived_hex = derived_ski.iter().map(|b| format!("{:02x}", b)).collect::<String>().to_uppercase();

        // 5. Read SKI from cert extension (via re-running compute_ski on
        // the cert's own pubkey bytes — a chain we already verified separately).
        let cert_pubkey = crate::cms::envelope::extract_cert_pubkey_bytes(&cert)
            .expect("cert must have a DSTU pubkey");
        let cert_ski = crate::cms::envelope::compute_ski(&cert_pubkey);
        let cert_hex = cert_ski.iter().map(|b| format!("{:02x}", b)).collect::<String>().to_uppercase();

        assert_eq!(
            derived_hex, cert_hex,
            "SKI derived from ZS2 must equal SKI computed from the matching cert"
        );
        assert_eq!(
            derived_hex,
            "FDC59901EBA8CC051173D29896A0ECEE90454F12908F275580E4551C679AD13B",
            "SKI must match the known-good value for this director key"
        );
    }

    /// Regression: `compute_ski` over the pubkey of a real АЦСК "Україна"
    /// end-entity cert reproduces the value found in that cert's
    /// `2.5.29.14` extension byte-for-byte. Source: director cert from
    /// WebCheck local store. Skipped if the file is absent.
    #[test]
    fn compute_ski_matches_real_ukrainian_cert() {
        let cert_path = "/mnt/c/ProgramData/WebCheck/Keys/CA-0882240800000000000000000000000000000001.cer";
        let cert = match std::fs::read(cert_path) {
            Ok(d) => d,
            Err(_) => return,
        };
        let pubkey_bytes = crate::cms::envelope::extract_cert_pubkey_bytes(&cert)
            .expect("cert must have a DSTU pubkey");
        let ski = crate::cms::envelope::compute_ski(&pubkey_bytes);
        let ski_hex = ski.iter().map(|b| format!("{:02x}", b)).collect::<String>().to_uppercase();
        assert_eq!(
            ski_hex,
            "FDC59901EBA8CC051173D29896A0ECEE90454F12908F275580E4551C679AD13B",
            "SKI must match the value in cert's 2.5.29.14 extension"
        );
    }

    /// Wrong password must produce `BadPassword`, not panic, not garbage.
    #[test]
    fn parse_wrong_password_returns_typed_error() {
        let path = "/mnt/d/PRRO_GATE/39197544_U250703163535.dat";
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(_) => return,
        };
        let r = parse(&data, b"wrongpassword");
        assert!(
            matches!(r, Err(Key6Error::BadPassword)),
            "expected BadPassword, got {:?}",
            r
        );
    }

    /// A non-IIT blob (just SEQUENCE with random content) must surface as
    /// `NotKey6`, allowing the container detector dispatcher to try other
    /// formats instead of failing hard.
    #[test]
    fn parse_non_iit_returns_not_key6() {
        // SEQUENCE { SEQUENCE { OID 1.2.3, SEQUENCE { OCTET STRING, OCTET STRING } }, OCTET STRING }
        // Construct a minimal but well-formed envelope with a wrong OID.
        let blob = [
            0x30, 0x18, // SEQUENCE
            0x30, 0x12, //   SEQUENCE (cryptParam)
            0x06, 0x02, 0x2A, 0x03, //     OID 1.2.3 (not IIT)
            0x30, 0x0C, //     SEQUENCE
            0x04, 0x04, 0x00, 0x00, 0x00, 0x00, //       mac (4 bytes)
            0x04, 0x04, 0x00, 0x00, 0x00, 0x00, //       pad (4 bytes — also wrong, pad < 8)
            0x04, 0x02, 0xAA, 0xBB, //   cryptData (2 bytes — not divisible by 8 either)
        ];
        let r = parse(&blob, b"x");
        assert!(matches!(r, Err(Key6Error::NotKey6)));
    }

    /// `dumb_kdf` is deterministic — same input ⇒ same output.
    #[test]
    fn dumb_kdf_deterministic() {
        let h1 = dumb_kdf(b"hello", 100);
        let h2 = dumb_kdf(b"hello", 100);
        assert_eq!(h1, h2);
        // And different password ⇒ different output.
        let h3 = dumb_kdf(b"world", 100);
        assert_ne!(h1, h3);
    }
}
