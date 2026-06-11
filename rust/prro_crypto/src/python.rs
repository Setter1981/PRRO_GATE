//! PyO3 bindings — exposes prro_crypto to Python.
//!
//! Compiled into the `prro_crypto` Python module when the `python`
//! cargo feature is enabled. Build with `maturin build --release` to
//! produce a wheel that PRRO_GATE imports as a regular Python package.
//!
//! ## Public API surface
//!
//! ```python
//! import prro_crypto
//!
//! # ── Container reading (Sprint 2.4 unified path) ─────────────────────
//! data = open("/path/to/key.zs2", "rb").read()
//! fmt  = prro_crypto.detect_container_format(data)        # "jks" | "key6" | "pfx" | None
//! ek   = prro_crypto.extract_private_key(data, "password")
//! # ek = { "format": "pfx", "param_d_hex": "...", "certs": [b"...", ...] }
//! # NB: .zs2 files (АЦСК "Україна") are structurally PKCS#12 → "pfx"
//!
//! # ── Signing — production path with OsRng ────────────────────────────
//! r_hex, s_hex = prro_crypto.sign_with_osrng(ek["param_d_hex"], hash_hex)
//!
//! # ── Signing — deterministic rand_e (test / replay only) ─────────────
//! r_hex, s_hex = prro_crypto.sign_dstu_pb_257(d_hex, hash_hex, rand_e_hex)
//!
//! # ── Full CMS/CAdES-BES detached signature ───────────────────────────
//! p7s_bytes = prro_crypto.cms_sign_detached(
//!     param_d_hex = ek["param_d_hex"],
//!     cert_der    = ek["certs"][0],
//!     content     = b"the receipt bytes the gateway needs signed",
//! )
//!
//! # ── EnvelopedData decrypt (Sprint 3.1) ─────────────────────────────
//! plaintext = prro_crypto.cms_decrypt_envelope(
//!     param_d_hex         = ek["param_d_hex"],
//!     envelope_der        = open("incoming.p7", "rb").read(),
//!     originator_cert_der = sender_cert_bytes,
//! )
//!
//! # ── CAdES-T (signature-timestamp via live TSA round-trip) ───────────
//! p7s = prro_crypto.cms_sign_detached_with_tsp(
//!     param_d_hex = ek["param_d_hex"],
//!     cert_der    = ek["certs"][0],
//!     content     = b"receipt payload",
//!     tsa_url     = "http://acskidd.gov.ua/services/tsp/",
//! )
//!
//! # ── CAdES-LT (signature-timestamp + revocation-values) ─────────────
//! p7s = prro_crypto.cms_sign_detached_cades_lt(
//!     param_d_hex    = ek["param_d_hex"],
//!     cert_der       = ek["certs"][0],
//!     content        = b"receipt payload",
//!     tst_der        = tst_bytes,    # pre-fetched TST or None
//!     crls           = [crl_der],    # list of CRL DER blobs
//!     ocsp_responses = [ocsp_der],   # list of BasicOCSPResponse DER blobs
//! )
//!
//! # ── Verify ──────────────────────────────────────────────────────────
//! ok = prro_crypto.verify_dstu_pb_257(pub_x_hex, pub_y_hex, hash_hex, r_hex, s_hex)
//!
//! # ── Auto-cert provisioning (fetch signing cert by SKI from any UA CA)
//! qx, qy = prro_crypto.pubkey_dstu_pb_257(ek["param_d_hex"])
//! compressed = prro_crypto.compress_pubkey(qx, qy)
//! ski = prro_crypto.compute_ski(compressed)
//! cert = prro_crypto.fetch_cert_by_ski("http://acskidd.gov.ua/services/cmp/", ski)
//!
//! # ── OCSP / CRL / TSP / cert metadata ───────────────────────────────
//! tsa_url  = prro_crypto.tsa_url_from_cert(cert_der)    # or None
//! ocsp_url = prro_crypto.ocsp_url_from_cert(cert_der)   # or None
//! crl_url  = prro_crypto.crl_url_from_cert(cert_der)    # or None
//! tst_der  = prro_crypto.fetch_timestamp(tsa_url, digest_bytes)
//! ocsp_der = prro_crypto.fetch_ocsp_response(ocsp_url, ...)
//! crl_der  = prro_crypto.fetch_crl(crl_url)
//! status   = prro_crypto.parse_ocsp_status(ocsp_der)     # dict
//!
//! # ── Operational helpers ─────────────────────────────────────────────
//! prro_crypto.warm_up()                         # prime CPUID dispatch
//! qx, qy = prro_crypto.pubkey_dstu_pb_257(d_hex)
//!
//! # ── Legacy JKS-only API (kept for backwards compat) ─────────────────
//! d_hex                = prro_crypto.read_jks_param_d(path, password)
//! alias, cert_list     = prro_crypto.read_jks_certs(path, password)
//! ```

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use crate::{
    cms::{
        builder::CmsBuildOptions,
        cmp,
        envelope::{extract_cert_pubkey_bytes, unwrap_envelope},
        revocation, tsp, CmsProfile, CmsSigner, DstuInProcessSigner,
    },
    core::sign as sign_mod,
    interop::prro::{
        containers::{self, ContainerError, ContainerFormat},
        der, jks,
    },
    Curve, FieldEl, Point, Signature,
};

// ─── helpers ──────────────────────────────────────────────────────────────

fn field_to_hex(f: &FieldEl) -> String {
    let mut s = String::new();
    for i in (0..f.bytes.len()).rev() {
        s.push_str(&format!("{:08x}", f.bytes[i]));
    }
    s
}

fn bytes_le_to_field(bytes: &[u8], mod_words: usize) -> FieldEl {
    let mut words = vec![0u32; mod_words];
    for (i, b) in bytes.iter().enumerate() {
        let word_idx = i / 4;
        let byte_idx = i % 4;
        if word_idx >= mod_words {
            break;
        }
        words[word_idx] |= (*b as u32) << (byte_idx * 8);
    }
    FieldEl::from_words(words)
}

fn format_to_str(f: ContainerFormat) -> &'static str {
    // `.zs2` files are structurally PKCS#12 and return `"pfx"` here —
    // the byte detector cannot (and must not) promise a stronger
    // distinction than the bytes actually carry. Callers that want to
    // remember "this came from an АЦСК Україна .zs2 file" should track
    // that as filename/origin metadata, not as a format label.
    match f {
        ContainerFormat::Jks => "jks",
        ContainerFormat::Key6Dat => "key6",
        ContainerFormat::Pfx => "pfx",
    }
}

fn container_err_to_py(e: ContainerError) -> PyErr {
    match e {
        ContainerError::UnknownFormat
        | ContainerError::ParserNotImplemented(_)
        | ContainerError::Jks(_)
        | ContainerError::Key6(_)
        | ContainerError::Pfx(_)
        | ContainerError::Der(_)
        | ContainerError::BadKeyWidth(_) => PyValueError::new_err(e.to_string()),
    }
}

// ─── unified container API (Sprint 2.4) ───────────────────────────────────

/// Detect container format from raw bytes (signature-based, ignores
/// file extension). Returns the format name as a string or `None` if
/// nothing matches.
#[pyfunction]
fn detect_container_format(data: &[u8]) -> Option<&'static str> {
    containers::detect_format(data).map(format_to_str)
}

/// Extract a DSTU 4145 private key (and any embedded certs) from any
/// supported container: JKS, PFX (includes `.zs2`), Key-6.dat.
///
/// Returns a dict:
///     { "format":       <"jks" | "key6" | "pfx">,
///       "param_d_hex":  <64-char hex, big-endian>,
///       "certs":        [<bytes>, ...]   # DER-encoded, may be empty }
///
/// `.zs2` files (АЦСК "Україна") are classified as `"pfx"` — they use
/// the same PKCS#12 structural layout with DSTU-specific OIDs. Store
/// the fact that a file was `.zs2` as origin metadata on the caller
/// side if you need it; do not infer it from `"format"`.
#[pyfunction]
fn extract_private_key<'py>(
    py: Python<'py>,
    data: &[u8],
    password: &str,
) -> PyResult<Bound<'py, PyDict>> {
    let extracted = containers::extract_private_key(data, password).map_err(container_err_to_py)?;
    let curve = Curve::dstu_pb_257();
    let d_field = bytes_le_to_field(extracted.param_d.as_slice(), curve.mod_words);

    let dict = PyDict::new_bound(py);
    dict.set_item("format", format_to_str(extracted.format))?;
    dict.set_item("param_d_hex", field_to_hex(&d_field))?;
    let certs: Vec<Bound<'py, PyBytes>> = extracted
        .certs
        .iter()
        .map(|c| PyBytes::new_bound(py, c))
        .collect();
    dict.set_item("certs", certs)?;
    Ok(dict)
}

// ─── signing (CMS pipeline) ───────────────────────────────────────────────

/// Build a full CMS/CAdES-BES signature over `content`, returning the
/// `.p7s` DER bytes ready to ship to ДПС.
///
/// Defaults match production fiscal signing: detached encapsulation,
/// `signingTime` = wall clock at sign time. Override either via the
/// optional kwargs:
///
/// - `attached=True` embeds the content as `eContent` (matches the
///   shape ЦЗО ships in its CAdES-BES test samples).
/// - `signing_time_unix=<int|None>` overrides the UTCTIME used for the
///   signingTime SignedAttribute. Pass `None` to omit the attribute
///   entirely (rare — not all relying parties accept it missing).
///
/// Internally uses `OsRng` for the per-call ephemeral scalar. The
/// signing profile is fixed to DSTU 4145 + GOST 34.311-95.
#[pyfunction]
#[pyo3(signature = (param_d_hex, cert_der, content, attached=false, signing_time_unix=None))]
fn cms_sign_detached<'py>(
    py: Python<'py>,
    param_d_hex: &str,
    cert_der: &[u8],
    content: &[u8],
    attached: bool,
    signing_time_unix: Option<i64>,
) -> PyResult<Bound<'py, PyBytes>> {
    let curve = Curve::dstu_pb_257();
    let d = hex_to_field_el(param_d_hex, curve.mod_words, "param_d_hex")?;
    let signer = DstuInProcessSigner::new(d);
    let cms_signer = CmsSigner {
        cert_der,
        signer: &signer,
        profile: CmsProfile::from_cert_der(&cert_der),
    };

    let signing_time = match signing_time_unix {
        // Sentinel `Some(-1)` would also be meaningful as "omit"; we keep
        // the API explicit: pass Python `None` to omit, otherwise positive
        // UNIX seconds. Default = use the current wall clock.
        Some(t) if t < 0 => {
            return Err(PyValueError::new_err(
                "signing_time_unix must be non-negative; use None to omit the attribute",
            ))
        }
        Some(t) => Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(t as u64)),
        None => Some(std::time::SystemTime::now()),
    };
    let opts = CmsBuildOptions {
        attached,
        signing_time,
    };
    // Full CMS sign = digest + scalar mul + signedAttrs DER. ~4-6 ms
    // on PB-257. Release the GIL so other Python threads (health,
    // REST, reconciliation) stay responsive under signing burst load.
    let result = py
        .allow_threads(|| cms_signer.sign_with(content, opts))
        .map_err(|e| PyRuntimeError::new_err(format!("CMS sign: {}", e)))?;
    Ok(PyBytes::new_bound(py, &result.cms_der))
}

/// Production sign with `OsRng` for the ephemeral scalar — caller does
/// not pass `rand_e`. Returns `(r_hex, s_hex)`.
///
/// `OsRng` retries internally on the (vanishing) chance of a degenerate
/// signature; a `RuntimeError` here means the OS RNG itself failed.
#[pyfunction]
fn sign_with_osrng(param_d_hex: &str, hash_hex: &str) -> PyResult<(String, String)> {
    use crate::cms::RawSigner;
    let curve = Curve::dstu_pb_257();
    let d = hex_to_field_el(param_d_hex, curve.mod_words, "param_d_hex")?;
    let signer = DstuInProcessSigner::new(d);

    // Build the digest bytes the same way `sign_detached` does — we want
    // the "raw signature" surface for callers that build their own CMS.
    let hash_bytes = hex_to_bytes(hash_hex)?;
    let raw = signer
        .sign_digest(&hash_bytes)
        .map_err(|e| PyRuntimeError::new_err(format!("sign: {}", e)))?;
    if raw.len() != 64 {
        return Err(PyRuntimeError::new_err(format!(
            "unexpected raw signature length {}",
            raw.len()
        )));
    }
    let r_hex = bytes_le_to_field(&raw[..32], curve.mod_words);
    let s_hex = bytes_le_to_field(&raw[32..], curve.mod_words);
    Ok((field_to_hex(&r_hex), field_to_hex(&s_hex)))
}

/// Fallible `FieldEl` construction from caller-supplied hex. Wraps
/// `FieldEl::try_from_hex` so every pyfunction that used to call
/// `FieldEl::from_hex(user_hex, ...)` (which panics on bad input)
/// now surfaces a typed `PyValueError`.
fn hex_to_field_el(hex: &str, mod_words: usize, label: &str) -> PyResult<FieldEl> {
    FieldEl::try_from_hex(hex, mod_words)
        .map_err(|e| PyValueError::new_err(format!("{label}: {e}")))
}

fn hex_to_bytes(s: &str) -> PyResult<Vec<u8>> {
    let s = s.trim_start_matches("0x");
    if s.len() % 2 != 0 {
        return Err(PyValueError::new_err("hex string must have even length"));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|e| PyValueError::new_err(format!("invalid hex at {}: {}", i, e)))
        })
        .collect()
}

// ─── deterministic / verify / pubkey (kept as before) ─────────────────────

/// Sign a hash with caller-supplied `rand_e` (deterministic).
///
/// Production code should prefer `sign_with_osrng` or `cms_sign_detached`.
/// This entry point exists for replay tests, vector reproduction, and
/// migration parity checks against jkurwa.
#[pyfunction]
fn sign_dstu_pb_257(d_hex: &str, hash_hex: &str, rand_e_hex: &str) -> PyResult<(String, String)> {
    let curve = Curve::dstu_pb_257();
    let d = hex_to_field_el(d_hex, curve.mod_words, "d_hex")?;
    let hash = hex_to_field_el(hash_hex, curve.mod_words, "hash_hex")?;
    let rand_e = hex_to_field_el(rand_e_hex, curve.mod_words, "rand_e_hex")?;
    let sig = sign_mod::sign(&curve, &d, &hash, &rand_e).ok_or_else(|| {
        PyValueError::new_err(
            "degenerate signature (eG.x or r is zero); retry with different rand_e",
        )
    })?;
    Ok((field_to_hex(&sig.r), field_to_hex(&sig.s)))
}

/// Verify a DSTU_PB_257 signature against a public point Q = -d·G.
#[pyfunction]
fn verify_dstu_pb_257(
    pub_x_hex: &str,
    pub_y_hex: &str,
    hash_hex: &str,
    r_hex: &str,
    s_hex: &str,
) -> PyResult<bool> {
    let curve = Curve::dstu_pb_257();
    let pub_q = Point::new(
        hex_to_field_el(pub_x_hex, curve.mod_words, "pub_x_hex")?,
        hex_to_field_el(pub_y_hex, curve.mod_words, "pub_y_hex")?,
    );
    let hash = hex_to_field_el(hash_hex, curve.mod_words, "hash_hex")?;
    let sig = Signature {
        r: hex_to_field_el(r_hex, curve.mod_words, "r_hex")?,
        s: hex_to_field_el(s_hex, curve.mod_words, "s_hex")?,
    };
    Ok(sign_mod::verify(&curve, &pub_q, &hash, &sig))
}

/// Compute Q = -d·G for a given private key. Returns (Q_x_hex, Q_y_hex).
#[pyfunction]
fn pubkey_dstu_pb_257(d_hex: &str) -> PyResult<(String, String)> {
    let curve = Curve::dstu_pb_257();
    let d = hex_to_field_el(d_hex, curve.mod_words, "d_hex")?;
    let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
    let q = g.mul(&d, &curve).negate();
    Ok((field_to_hex(&q.x), field_to_hex(&q.y)))
}

/// Prime the backend dispatch (CPUID probe) so the first `sign_*` in a
/// process does not pay the one-time detection cost.
#[pyfunction]
fn warm_up() {
    crate::core::backend::warm_up();
}

// ─── legacy JKS-only API (backwards compat) ───────────────────────────────

/// **Legacy.** Prefer `extract_private_key(read(path), password)`.
#[pyfunction]
fn read_jks_param_d(jks_path: &str, password: &str) -> PyResult<String> {
    let data = std::fs::read(jks_path)
        .map_err(|e| PyRuntimeError::new_err(format!("read JKS file: {}", e)))?;
    let entry = jks::read_jks(&data, password)
        .map_err(|e| PyRuntimeError::new_err(format!("decrypt JKS: {}", e)))?;
    let d_bytes = der::extract_param_d(&entry.key_der)
        .map_err(|e| PyRuntimeError::new_err(format!("parse DSTU privkey: {}", e)))?;
    let curve = Curve::dstu_pb_257();
    let d_field = bytes_le_to_field(&d_bytes, curve.mod_words);
    Ok(field_to_hex(&d_field))
}

/// **Legacy.** Prefer `extract_private_key(read(path), password)`.
#[pyfunction]
fn read_jks_certs(jks_path: &str, password: &str) -> PyResult<(String, Vec<Vec<u8>>)> {
    let data = std::fs::read(jks_path)
        .map_err(|e| PyRuntimeError::new_err(format!("read JKS file: {}", e)))?;
    let entry = jks::read_jks(&data, password)
        .map_err(|e| PyRuntimeError::new_err(format!("decrypt JKS: {}", e)))?;
    Ok((entry.alias, entry.certs))
}

// ─── EnvelopedData decrypt (Sprint 3.1) ───────────────────────────────────

/// Decrypt a PKCS#7 EnvelopedData ciphertext with the recipient's
/// DSTU 4145 private key.
///
/// Arguments:
/// - `param_d_hex`: recipient private key (from `extract_private_key`)
/// - `envelope_der`: the full `.p7` bytes
/// - `originator_cert_der`: sender's X.509 cert, used to recover the
///   ECDH originator pubkey (caller looks up the cert by IssuerAndSerial
///   from the envelope; Rust does not do that lookup itself)
///
/// Returns the plaintext bytes.
#[pyfunction]
fn cms_decrypt_envelope<'py>(
    py: Python<'py>,
    param_d_hex: &str,
    envelope_der: &[u8],
    originator_cert_der: &[u8],
) -> PyResult<Bound<'py, PyBytes>> {
    let curve = Curve::dstu_pb_257();
    let d = hex_to_field_el(param_d_hex, curve.mod_words, "param_d_hex")?;

    let pub_compressed = extract_cert_pubkey_bytes(originator_cert_der)
        .map_err(|e| PyValueError::new_err(format!("originator cert parse: {}", e)))?;
    // Use the checked decompression path — this is untrusted-origin
    // cert data, so any malformed / invalid-curve / wrong-subgroup
    // pubkey must be rejected before it reaches the ECDH ladder.
    let pub_q = crate::core::point::expand_compressed_checked(&pub_compressed, &curve)
        .map_err(|e| PyValueError::new_err(format!("originator pubkey: {}", e)))?;

    // Release the GIL across the ECDH ladder + GOST keywrap + CFB
    // decrypt — these are the crate's heaviest single-call primitives
    // (single-digit ms each, but cumulative under load). Other Python
    // threads must not wait on them.
    let plaintext = py
        .allow_threads(|| unwrap_envelope(envelope_der, &d, &pub_q, &curve))
        .map_err(|e| PyRuntimeError::new_err(format!("envelope unwrap: {}", e)))?;

    Ok(PyBytes::new_bound(py, &plaintext))
}

// ─── TSP / CAdES-T (Sprint 3.2) ───────────────────────────────────────────

/// Extract the TSA URL from a signer's X.509 certificate
/// SubjectInfoAccess extension. Returns `None` if the cert has no SIA
/// or no `id-ad-timeStamping` accessMethod.
#[pyfunction]
fn tsa_url_from_cert(cert_der: &[u8]) -> PyResult<Option<String>> {
    match tsp::tsa_url_from_cert(cert_der) {
        Ok(url) => Ok(Some(url)),
        Err(tsp::TspError::Parse(_)) => Ok(None),
        Err(e) => Err(PyRuntimeError::new_err(format!("SIA parse: {}", e))),
    }
}

/// Blocking TSP round-trip. Sends `digest` to `tsa_url` as a minimal
/// RFC 3161 TimeStampReq (hashAlgorithm = GOST 34.311) and returns the
/// DER bytes of the TimeStampToken on success.
///
/// **Trust model:** returns bytes as-delivered. This function does NOT
/// verify the TSA signature on the returned token and does NOT check
/// the TSA cert chain. Tamper-resistance comes from TLS to the TSA
/// endpoint only; if the transport is plain HTTP (osplus.ini default
/// for Ukrainian CAs), a network attacker can substitute the token.
/// Callers MUST not rely on the token for legal/fiscal purposes
/// without out-of-band validation. See SECURITY.md.
#[cfg(feature = "tsp_http")]
#[pyfunction]
#[pyo3(signature = (tsa_url, digest, timeout_seconds=5.0))]
fn fetch_timestamp<'py>(
    py: Python<'py>,
    tsa_url: &str,
    digest: &[u8],
    timeout_seconds: f64,
) -> PyResult<Bound<'py, PyBytes>> {
    if !(0.1..=60.0).contains(&timeout_seconds) {
        return Err(PyValueError::new_err(
            "timeout_seconds must be in [0.1, 60]",
        ));
    }
    let timeout = std::time::Duration::from_millis((timeout_seconds * 1000.0) as u64);
    // Release the GIL across the HTTP round-trip — on Ukrainian CAs
    // under 3G/LTE fallback a single timestamp can take 1-15 s. Holding
    // the GIL for that window starves every other Python thread in the
    // gateway (health endpoints, REST ingress, reconciliation).
    let tst = py
        .allow_threads(|| tsp::fetch_timestamp(tsa_url, digest, timeout))
        .map_err(|e| PyRuntimeError::new_err(format!("TSP: {}", e)))?;
    Ok(PyBytes::new_bound(py, &tst))
}

/// CAdES-T: build a detached signature AND embed a signature-timestamp
/// token (live TSA round-trip). Returns the final `.p7s` DER bytes.
///
/// If `tsa_url` is empty, the URL is extracted from the signer's
/// certificate SubjectInfoAccess extension — matching jkurwa's default.
#[cfg(feature = "tsp_http")]
#[pyfunction]
#[pyo3(signature = (
    param_d_hex, cert_der, content,
    tsa_url=None, timeout_seconds=5.0,
    attached=false, signing_time_unix=None
))]
fn cms_sign_detached_with_tsp<'py>(
    py: Python<'py>,
    param_d_hex: &str,
    cert_der: &[u8],
    content: &[u8],
    tsa_url: Option<&str>,
    timeout_seconds: f64,
    attached: bool,
    signing_time_unix: Option<i64>,
) -> PyResult<Bound<'py, PyBytes>> {
    if !(0.1..=60.0).contains(&timeout_seconds) {
        return Err(PyValueError::new_err(
            "timeout_seconds must be in [0.1, 60]",
        ));
    }
    let curve = Curve::dstu_pb_257();
    let d = hex_to_field_el(param_d_hex, curve.mod_words, "param_d_hex")?;
    let signer = DstuInProcessSigner::new(d);
    let cms_signer = CmsSigner {
        cert_der,
        signer: &signer,
        profile: CmsProfile::from_cert_der(&cert_der),
    };

    let signing_time = match signing_time_unix {
        Some(t) if t < 0 => {
            return Err(PyValueError::new_err(
                "signing_time_unix must be non-negative; use None to omit",
            ))
        }
        Some(t) => Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(t as u64)),
        None => Some(std::time::SystemTime::now()),
    };
    let opts = CmsBuildOptions {
        attached,
        signing_time,
    };

    let resolved_url: String = match tsa_url {
        Some(u) if !u.is_empty() => u.to_string(),
        _ => tsp::tsa_url_from_cert(cert_der).map_err(|e| {
            PyValueError::new_err(format!(
                "tsa_url not given and cert has no usable SIA: {}",
                e
            ))
        })?,
    };

    let timeout = std::time::Duration::from_millis((timeout_seconds * 1000.0) as u64);
    // Sign → hash signature → HTTP round-trip to TSA → re-assemble
    // with TST. The TSA call dominates wall-clock; release the GIL so
    // nothing in the Python side starves while we wait on ca.tax.gov.ua.
    let result = py
        .allow_threads(|| cms_signer.sign_with_tsp(content, opts, &resolved_url, timeout))
        .map_err(|e| PyRuntimeError::new_err(format!("CMS sign+TSP: {}", e)))?;
    Ok(PyBytes::new_bound(py, &result.cms_der))
}

/// CAdES-T with a pre-fetched TST. Used when the caller already has
/// the TimeStampToken in hand (cache / batched fetch / different
/// transport). No network is touched here.
#[pyfunction]
#[pyo3(signature = (param_d_hex, cert_der, content, tst_der, attached=false, signing_time_unix=None))]
fn cms_sign_detached_with_tst<'py>(
    py: Python<'py>,
    param_d_hex: &str,
    cert_der: &[u8],
    content: &[u8],
    tst_der: &[u8],
    attached: bool,
    signing_time_unix: Option<i64>,
) -> PyResult<Bound<'py, PyBytes>> {
    let curve = Curve::dstu_pb_257();
    let d = hex_to_field_el(param_d_hex, curve.mod_words, "param_d_hex")?;
    let signer = DstuInProcessSigner::new(d);
    let cms_signer = CmsSigner {
        cert_der,
        signer: &signer,
        profile: CmsProfile::from_cert_der(&cert_der),
    };
    let signing_time = match signing_time_unix {
        Some(t) if t < 0 => {
            return Err(PyValueError::new_err(
                "signing_time_unix must be non-negative; use None to omit",
            ))
        }
        Some(t) => Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(t as u64)),
        None => Some(std::time::SystemTime::now()),
    };
    let opts = CmsBuildOptions {
        attached,
        signing_time,
    };

    let result = py
        .allow_threads(|| cms_signer.sign_with_tst(content, opts, tst_der))
        .map_err(|e| PyRuntimeError::new_err(format!("CMS sign+TST: {}", e)))?;
    Ok(PyBytes::new_bound(py, &result.cms_der))
}

// ─── Revocation / CAdES-LT (Sprint 3.3) ───────────────────────────────────

/// Extract the OCSP responder URL from a certificate's AuthorityInfoAccess.
/// Returns `None` if AIA is missing or has no `id-ad-ocsp` entry.
#[pyfunction]
fn ocsp_url_from_cert(cert_der: &[u8]) -> PyResult<Option<String>> {
    match revocation::ocsp_url_from_cert(cert_der) {
        Ok(u) => Ok(Some(u)),
        Err(revocation::RevocationError::Parse(_)) => Ok(None),
        Err(e) => Err(PyRuntimeError::new_err(format!("AIA parse: {e}"))),
    }
}

/// Extract the first HTTP CRL URL from a cert's CRLDistributionPoints.
#[pyfunction]
fn crl_url_from_cert(cert_der: &[u8]) -> PyResult<Option<String>> {
    match revocation::crl_url_from_cert(cert_der) {
        Ok(u) => Ok(Some(u)),
        Err(revocation::RevocationError::Parse(_)) => Ok(None),
        Err(e) => Err(PyRuntimeError::new_err(format!("CDP parse: {e}"))),
    }
}

/// Blocking HTTP GET of a CRL. Returns DER bytes as-is.
///
/// **Trust model:** the crate does NOT verify the CRL's issuer
/// signature. TLS to the CA is the only tamper barrier. Do not embed
/// an un-verified CRL in a downstream CAdES-LT without first
/// validating it against the expected CA cert at the caller's layer.
#[cfg(feature = "tsp_http")]
#[pyfunction]
#[pyo3(signature = (crl_url, timeout_seconds=10.0))]
fn fetch_crl<'py>(
    py: Python<'py>,
    crl_url: &str,
    timeout_seconds: f64,
) -> PyResult<Bound<'py, PyBytes>> {
    if !(0.1..=60.0).contains(&timeout_seconds) {
        return Err(PyValueError::new_err(
            "timeout_seconds must be in [0.1, 60]",
        ));
    }
    let timeout = std::time::Duration::from_millis((timeout_seconds * 1000.0) as u64);
    let crl = py
        .allow_threads(|| revocation::fetch_crl(crl_url, timeout))
        .map_err(|e| PyRuntimeError::new_err(format!("CRL fetch: {e}")))?;
    Ok(PyBytes::new_bound(py, &crl))
}

/// Parse a BasicOCSPResponse DER blob and return a dict:
///     {
///         "status": "good" | "revoked" | "unknown",
///         "revoked_at": int | None,    # Unix seconds
///         "this_update": int,          # Unix seconds
///         "next_update": int | None,   # Unix seconds
///     }
///
/// **Two things this function does NOT do (by design):**
///
/// 1. It does NOT verify the OCSP responder's signature.
/// 2. It does NOT bind the response to a specific cert serial — it
///    takes the **first** `SingleResponse` unconditionally. In a
///    multi-response scenario the returned status may belong to a
///    different certificate. The caller MUST cross-check the serial
///    at their layer.
///
/// See `SECURITY.md` — section "Відомі компроміси".
#[pyfunction]
fn parse_ocsp_status<'py>(py: Python<'py>, basic_ocsp_der: &[u8]) -> PyResult<Bound<'py, PyDict>> {
    let parsed = revocation::parse_ocsp_status(basic_ocsp_der)
        .map_err(|e| PyValueError::new_err(format!("parse_ocsp_status: {e}")))?;
    let dict = PyDict::new_bound(py);
    dict.set_item("status", parsed.status)?;
    match parsed.revoked_at {
        Some(v) => dict.set_item("revoked_at", v)?,
        None => dict.set_item("revoked_at", py.None())?,
    }
    dict.set_item("this_update", parsed.this_update)?;
    match parsed.next_update {
        Some(v) => dict.set_item("next_update", v)?,
        None => dict.set_item("next_update", py.None())?,
    }
    // `serial_matches` from the Rust struct is deliberately omitted —
    // it never carried a real serial comparison and returning a
    // permanently-true flag would invite callers to write
    // `if resp["serial_matches"]:` checks that are meaningless.
    Ok(dict)
}

/// Blocking OCSP round-trip. Returns the BasicOCSPResponse DER on success.
///
/// **Trust model:** the crate does NOT verify the OCSP responder's
/// signature on the returned BasicOCSPResponse. Caller MUST validate
/// the responder cert chain and signature before acting on the status.
/// TLS to the CA is the only baseline tamper-resistance.
#[cfg(feature = "tsp_http")]
#[pyfunction]
#[pyo3(signature = (
    ocsp_url, issuer_name_hash, issuer_key_hash, serial_der,
    nonce=None, timeout_seconds=5.0
))]
fn fetch_ocsp_response<'py>(
    py: Python<'py>,
    ocsp_url: &str,
    issuer_name_hash: &[u8],
    issuer_key_hash: &[u8],
    serial_der: &[u8],
    nonce: Option<&[u8]>,
    timeout_seconds: f64,
) -> PyResult<Bound<'py, PyBytes>> {
    if !(0.1..=60.0).contains(&timeout_seconds) {
        return Err(PyValueError::new_err(
            "timeout_seconds must be in [0.1, 60]",
        ));
    }
    let timeout = std::time::Duration::from_millis((timeout_seconds * 1000.0) as u64);
    let basic = py
        .allow_threads(|| {
            revocation::fetch_ocsp_response(
                ocsp_url,
                issuer_name_hash,
                issuer_key_hash,
                serial_der,
                nonce,
                timeout,
            )
        })
        .map_err(|e| PyRuntimeError::new_err(format!("OCSP: {e}")))?;
    Ok(PyBytes::new_bound(py, &basic))
}

/// CAdES-LT uplift with caller-supplied revocation info. Accepts an
/// optional TST (bytes) and lists of CRLs and OCSP responses to embed
/// under `id-aa-ets-revocationValues`.
#[pyfunction]
#[pyo3(signature = (
    param_d_hex, cert_der, content,
    tst_der=None, crls=None, ocsp_responses=None,
    attached=false, signing_time_unix=None
))]
#[allow(clippy::too_many_arguments)]
fn cms_sign_detached_cades_lt<'py>(
    py: Python<'py>,
    param_d_hex: &str,
    cert_der: &[u8],
    content: &[u8],
    tst_der: Option<&[u8]>,
    crls: Option<Vec<Vec<u8>>>,
    ocsp_responses: Option<Vec<Vec<u8>>>,
    attached: bool,
    signing_time_unix: Option<i64>,
) -> PyResult<Bound<'py, PyBytes>> {
    let curve = Curve::dstu_pb_257();
    let d = hex_to_field_el(param_d_hex, curve.mod_words, "param_d_hex")?;
    let signer = DstuInProcessSigner::new(d);
    let cms_signer = CmsSigner {
        cert_der,
        signer: &signer,
        profile: CmsProfile::from_cert_der(&cert_der),
    };
    let signing_time = match signing_time_unix {
        Some(t) if t < 0 => {
            return Err(PyValueError::new_err(
                "signing_time_unix must be non-negative; use None to omit",
            ))
        }
        Some(t) => Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(t as u64)),
        None => Some(std::time::SystemTime::now()),
    };
    let opts = CmsBuildOptions {
        attached,
        signing_time,
    };

    let crls = crls.unwrap_or_default();
    let ocsps = ocsp_responses.unwrap_or_default();

    let result = py
        .allow_threads(|| cms_signer.sign_with_lt(content, opts, tst_der, &crls, &ocsps))
        .map_err(|e| PyRuntimeError::new_err(format!("CMS sign+LT: {e}")))?;
    Ok(PyBytes::new_bound(py, &result.cms_der))
}

// ─── Cert provisioning (Sprint 14 — Phase 1) ─────────────────────────────

/// Compute the Ukrainian Subject Key Identifier (SKI) of a DSTU 4145
/// compressed public key. Returns 32 bytes (GOST 34.311-95 over the
/// BIT STRING content of subjectPublicKey, excluding the unused-bits
/// byte). Used to look up an end-entity cert by SKI in a CA repository
/// when only the private key + password are known up front.
#[pyfunction]
fn compute_ski<'py>(py: Python<'py>, pubkey_compressed: &[u8]) -> PyResult<Bound<'py, PyBytes>> {
    let ski = crate::cms::envelope::compute_ski(pubkey_compressed);
    Ok(PyBytes::new_bound(py, &ski))
}

/// Compress a DSTU 4145 public key (qx, qy) to its 33-byte LE form
/// with parity-bit-in-LSB-of-x. Inverse of point decompression — used
/// to compute SKI from a freshly-loaded private key without a cert.
#[pyfunction]
fn compress_pubkey<'py>(
    py: Python<'py>,
    qx_hex: &str,
    qy_hex: &str,
) -> PyResult<Bound<'py, PyBytes>> {
    let curve = Curve::dstu_pb_257();
    let qx = hex_to_field_el(qx_hex, curve.mod_words, "qx_hex")?;
    let qy = hex_to_field_el(qy_hex, curve.mod_words, "qy_hex")?;
    let q = Point::new(qx, qy);
    let bytes = crate::core::point::compress_point(&q, &curve);
    Ok(PyBytes::new_bound(py, &bytes))
}

/// Extract the raw compressed subjectPublicKey bytes from an X.509 cert.
/// Returns the 33-byte compressed Q that feeds into [`compute_ski`].
/// Used during provisioning to match a private-key-derived SKI against
/// every cert embedded in the container.
#[pyfunction]
fn extract_cert_pubkey<'py>(py: Python<'py>, cert_der: &[u8]) -> PyResult<Bound<'py, PyBytes>> {
    let pub_bytes = extract_cert_pubkey_bytes(cert_der)
        .map_err(|e| PyValueError::new_err(format!("extract_cert_pubkey: {e}")))?;
    Ok(PyBytes::new_bound(py, &pub_bytes))
}

/// Blocking CMP lookup: resolve an end-entity certificate by 32-byte
/// DSTU 4145 Subject Key Identifier.
///
/// Sends the IIT proprietary cert-lookup payload (reverse-engineered
/// from `dstucrypt/agent`) as an HTTP POST to `cmp_url`.
///
/// Matches the lookup flow ІІТ WebCheck performs under the hood
/// (default CA front: http://acskidd.gov.ua:80). Raises on both
/// NOT_FOUND and transport errors — the caller translates into
/// "needs_manual_upload".
///
/// **Trust model:** the crate validates ONLY that the returned cert
/// is a DER SEQUENCE whose computed SKI matches the queried SKI
/// (cheap sanity). It does NOT verify the cert's signature chain to
/// any root. Caller MUST validate the chain before trusting the cert
/// for signature verification or encryption-to-originator. TLS to
/// the CA is not a full substitute — the CA front is plain HTTP by
/// default. See SECURITY.md.
#[cfg(feature = "tsp_http")]
#[pyfunction]
#[pyo3(signature = (cmp_url, ski, timeout_seconds=10.0))]
fn fetch_cert_by_ski<'py>(
    py: Python<'py>,
    cmp_url: &str,
    ski: &[u8],
    timeout_seconds: f64,
) -> PyResult<Bound<'py, PyBytes>> {
    if !(0.1..=60.0).contains(&timeout_seconds) {
        return Err(PyValueError::new_err(
            "timeout_seconds must be in [0.1, 60]",
        ));
    }
    if ski.len() != 32 {
        return Err(PyValueError::new_err("SKI must be 32 bytes"));
    }
    let timeout = std::time::Duration::from_millis((timeout_seconds * 1000.0) as u64);
    let cert = py
        .allow_threads(|| cmp::fetch_cert_by_ski(cmp_url, ski, timeout))
        .map_err(|e| PyRuntimeError::new_err(format!("CMP: {e}")))?;
    Ok(PyBytes::new_bound(py, &cert))
}

// ─── module entry ─────────────────────────────────────────────────────────

#[pymodule]
fn prro_crypto(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Sprint 2.4 unified container path.
    m.add_function(wrap_pyfunction!(detect_container_format, m)?)?;
    m.add_function(wrap_pyfunction!(extract_private_key, m)?)?;
    // Signing.
    m.add_function(wrap_pyfunction!(cms_sign_detached, m)?)?;
    // Decryption (Sprint 3.1).
    m.add_function(wrap_pyfunction!(cms_decrypt_envelope, m)?)?;
    // TSP / CAdES-T (Sprint 3.2).
    m.add_function(wrap_pyfunction!(tsa_url_from_cert, m)?)?;
    m.add_function(wrap_pyfunction!(cms_sign_detached_with_tst, m)?)?;
    #[cfg(feature = "tsp_http")]
    {
        m.add_function(wrap_pyfunction!(fetch_timestamp, m)?)?;
        m.add_function(wrap_pyfunction!(cms_sign_detached_with_tsp, m)?)?;
    }
    // Revocation / CAdES-LT (Sprint 3.3).
    m.add_function(wrap_pyfunction!(ocsp_url_from_cert, m)?)?;
    m.add_function(wrap_pyfunction!(crl_url_from_cert, m)?)?;
    m.add_function(wrap_pyfunction!(cms_sign_detached_cades_lt, m)?)?;
    // Cert-status monitoring (Sprint 13).
    m.add_function(wrap_pyfunction!(parse_ocsp_status, m)?)?;
    // Cert provisioning (Sprint 14 — Phase 1).
    m.add_function(wrap_pyfunction!(compute_ski, m)?)?;
    m.add_function(wrap_pyfunction!(compress_pubkey, m)?)?;
    m.add_function(wrap_pyfunction!(extract_cert_pubkey, m)?)?;
    #[cfg(feature = "tsp_http")]
    {
        m.add_function(wrap_pyfunction!(fetch_crl, m)?)?;
        m.add_function(wrap_pyfunction!(fetch_ocsp_response, m)?)?;
        // Sprint 14 — Phase 2 cert-by-SKI CMP lookup.
        m.add_function(wrap_pyfunction!(fetch_cert_by_ski, m)?)?;
    }
    m.add_function(wrap_pyfunction!(sign_with_osrng, m)?)?;
    m.add_function(wrap_pyfunction!(sign_dstu_pb_257, m)?)?;
    m.add_function(wrap_pyfunction!(verify_dstu_pb_257, m)?)?;
    m.add_function(wrap_pyfunction!(pubkey_dstu_pb_257, m)?)?;
    m.add_function(wrap_pyfunction!(warm_up, m)?)?;
    // Legacy JKS-only.
    m.add_function(wrap_pyfunction!(read_jks_param_d, m)?)?;
    m.add_function(wrap_pyfunction!(read_jks_certs, m)?)?;
    m.add("__version__", "0.1.0-alpha.1")?;
    Ok(())
}
