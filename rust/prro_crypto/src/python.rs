//! PyO3 bindings — exposes prro_crypto to Python.
//!
//! Compiled into `prro_crypto._native` when the `python` feature is enabled.
//! Build via `maturin build --release` to produce a wheel.
//!
//! Public Python API:
//! ```python
//! import prro_crypto
//!
//! # Read JKS, extract DSTU 4145 private key
//! key_d_hex = prro_crypto.read_jks_param_d("/path/to/key.jks", "password")
//!
//! # Sign a hash with a known random scalar (caller provides RNG)
//! r_hex, s_hex = prro_crypto.sign_dstu_pb_257(
//!     d_hex=key_d_hex,
//!     hash_hex="01020304...",
//!     rand_e_hex="cafebabe...",
//! )
//!
//! # Verify a signature
//! ok = prro_crypto.verify_dstu_pb_257(
//!     pub_x_hex=..., pub_y_hex=...,
//!     hash_hex=..., r_hex=..., s_hex=...,
//! )
//! ```

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

use crate::{der, jks, sign as sign_mod, Curve, FieldEl, Point, Signature};

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

/// Read a JKS keystore and extract the DSTU 4145 private key as a hex string
/// (big-endian, padded to curve word size).
///
/// Returns the private key `d` as a hex string suitable for passing to
/// `sign_dstu_pb_257`.
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

/// Read the alias and certificate chain (DER-encoded) from a JKS keystore.
/// Returns (alias, list of cert bytes).
#[pyfunction]
fn read_jks_certs(jks_path: &str, password: &str) -> PyResult<(String, Vec<Vec<u8>>)> {
    let data = std::fs::read(jks_path)
        .map_err(|e| PyRuntimeError::new_err(format!("read JKS file: {}", e)))?;
    let entry = jks::read_jks(&data, password)
        .map_err(|e| PyRuntimeError::new_err(format!("decrypt JKS: {}", e)))?;
    Ok((entry.alias, entry.certs))
}

/// Sign a hash using DSTU_PB_257 with caller-provided random scalar.
/// Returns (r_hex, s_hex). Raises ValueError if the random scalar produces
/// a degenerate signature (caller should retry with a different rand_e).
///
/// All hex inputs are big-endian, optionally padded with leading zeros.
/// Hashes longer than 257 bits will be truncated by the signing algorithm.
#[pyfunction]
fn sign_dstu_pb_257(d_hex: &str, hash_hex: &str, rand_e_hex: &str) -> PyResult<(String, String)> {
    let curve = Curve::dstu_pb_257();
    let d = FieldEl::from_hex(d_hex, curve.mod_words);
    let hash = FieldEl::from_hex(hash_hex, curve.mod_words);
    let rand_e = FieldEl::from_hex(rand_e_hex, curve.mod_words);

    let sig = sign_mod::sign(&curve, &d, &hash, &rand_e).ok_or_else(|| {
        PyValueError::new_err(
            "degenerate signature (eG.x or r is zero); retry with different rand_e",
        )
    })?;
    Ok((field_to_hex(&sig.r), field_to_hex(&sig.s)))
}

/// Verify a DSTU_PB_257 signature.
///
/// `pub_x_hex` and `pub_y_hex` are the public point Q coordinates. In jkurwa
/// convention, Q = -d*G — `Priv.pub()` returns the negated d*G.
#[pyfunction]
fn verify_dstu_pb_257(
    pub_x_hex: &str,
    pub_y_hex: &str,
    hash_hex: &str,
    r_hex: &str,
    s_hex: &str,
) -> PyResult<bool> {
    let curve = Curve::dstu_pb_257();
    let pub_x = FieldEl::from_hex(pub_x_hex, curve.mod_words);
    let pub_y = FieldEl::from_hex(pub_y_hex, curve.mod_words);
    let pub_q = Point::new(pub_x, pub_y);
    let hash = FieldEl::from_hex(hash_hex, curve.mod_words);
    let r = FieldEl::from_hex(r_hex, curve.mod_words);
    let s = FieldEl::from_hex(s_hex, curve.mod_words);
    let sig = Signature { r, s };
    Ok(sign_mod::verify(&curve, &pub_q, &hash, &sig))
}

/// Compute the public point Q = -d*G for a given private key d.
/// Returns (Q_x_hex, Q_y_hex).
#[pyfunction]
fn pubkey_dstu_pb_257(d_hex: &str) -> PyResult<(String, String)> {
    let curve = Curve::dstu_pb_257();
    let d = FieldEl::from_hex(d_hex, curve.mod_words);
    let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
    let dg = g.mul(&d, &curve);
    let q = dg.negate();
    Ok((field_to_hex(&q.x), field_to_hex(&q.y)))
}

/// Module entry point. Registered as `prro_crypto`.
#[pymodule]
fn prro_crypto(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(read_jks_param_d, m)?)?;
    m.add_function(wrap_pyfunction!(read_jks_certs, m)?)?;
    m.add_function(wrap_pyfunction!(sign_dstu_pb_257, m)?)?;
    m.add_function(wrap_pyfunction!(verify_dstu_pb_257, m)?)?;
    m.add_function(wrap_pyfunction!(pubkey_dstu_pb_257, m)?)?;
    m.add("__version__", "0.1.0-alpha.1")?;
    Ok(())
}
