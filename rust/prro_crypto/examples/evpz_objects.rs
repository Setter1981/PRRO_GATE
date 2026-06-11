//! Replicates WebCheck's «завантажити дані з кабінету податкової»
//! (`AccountantОffice.FileForSend` + sign + `SendFile`): POST a CMS-signed
//! `{"Command":"Objects"}` to the ЄВПЗ `/fs/cmd` endpoint and print the JSON
//! response — an array of `{Tin, NumFiscal}` = the PRRO fiscal numbers
//! registered to the signing key's owner.
//!
//! This is the ЄВПЗ (Єдине-вікно) command API — SEPARATE from the ФСКО
//! fiscal-PRRO gRPC (`cabinet.tax.gov.ua:9443`). The endpoint is plain HTTP
//! (no TLS), exactly as WebCheck uses it.
//!
//! ```text
//! cargo run -p prro_crypto --example evpz_objects -- \
//!   "/path/key.jks" "<password>" [http://host:port/fs/cmd]
//! ```
//! Default endpoint = `http://fs.tax.gov.ua:8609/fs/cmd` (the PRODUCTION
//! endpoint WebCheck targets; pass a test endpoint as the 3rd arg if needed).
//! The signed content is the raw command bytes, ATTACHED CAdES-BES +
//! signingTime — the same profile used for `rro_fn_sign`.

use prro_crypto::cms::builder::{CmsBuildOptions, CmsSigner};
use prro_crypto::cms::profile::CmsProfile;
use prro_crypto::cms::signer::DstuInProcessSigner;
use prro_crypto::core::curve::Curve;
use prro_crypto::core::field::FieldEl;
use prro_crypto::interop::prro::containers::extract_private_key;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, SystemTime};

const DEFAULT_ENDPOINT: &str = "http://fs.tax.gov.ua:8609/fs/cmd";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (path, pass, endpoint) = match args.as_slice() {
        [_, p, pw] => (p.clone(), pw.clone(), DEFAULT_ENDPOINT.to_string()),
        [_, p, pw, ep] => (p.clone(), pw.clone(), ep.clone()),
        _ => {
            eprintln!("usage: evpz_objects <key-container> <password> [http-endpoint]");
            std::process::exit(1);
        }
    };

    let data = std::fs::read(&path).unwrap_or_else(|e| {
        eprintln!("read {path}: {e}");
        std::process::exit(2);
    });
    let ek = extract_private_key(&data, &pass).unwrap_or_else(|e| {
        eprintln!("extract_private_key (wrong pass / unknown format?): {e:?}");
        std::process::exit(3);
    });
    // Pick the SIGNING cert (KeyUsage digitalSignature/nonRepudiation), NOT
    // certs[0] — a UA EDS container holds a signing cert AND a key-agreement
    // (encryption) cert; embedding the wrong one makes DPS verify the
    // signature against the wrong public key (-> CryptBadSign).
    let cert: &[u8] = ek
        .certs
        .iter()
        .find(|c| has_digital_signature_usage(c))
        .or_else(|| ek.certs.first())
        .map(Vec::as_slice)
        .unwrap_or_else(|| {
            eprintln!("container has no certificate");
            std::process::exit(4);
        });

    // Sign the command. Content = the raw JSON bytes; ATTACHED CAdES-BES +
    // signingTime (identical profile to the fiscal `rro_fn_sign`).
    let command = br#"{"Command":"Objects"}"#;
    let curve = Curve::dstu_pb_257();
    let d = FieldEl::from_le_bytes(&ek.param_d[..], curve.mod_words);
    let signer = DstuInProcessSigner::new(d);
    let signed = CmsSigner {
        cert_der: cert,
        signer: &signer,
        profile: CmsProfile::Dstu4145WithGost34311Pb,
    }
    .sign_with(
        command,
        CmsBuildOptions {
            attached: true,
            signing_time: Some(SystemTime::now()),
        },
    )
    .unwrap_or_else(|e| {
        eprintln!("CMS sign of Objects command failed: {e:?}");
        std::process::exit(5);
    })
    .cms_der;
    eprintln!(
        "signed {{\"Command\":\"Objects\"}}: {} bytes (attached CAdES-BES) -> POST {endpoint}",
        signed.len()
    );

    match http_post(&endpoint, &signed) {
        Ok(body) => println!("{body}"),
        Err(e) => {
            eprintln!("POST {endpoint} failed: {e}");
            std::process::exit(6);
        }
    }
}

/// Minimal plain-HTTP/1.1 POST (the ЄВПЗ cmd endpoint is `http://`, no TLS).
/// Sends `application/octet-stream`, reads to EOF (`Connection: close`), and
/// returns the response body (everything after the header terminator).
fn http_post(url: &str, body: &[u8]) -> std::io::Result<String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| io_err("endpoint must be plain http:// (no TLS)"))?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (
            h.to_string(),
            p.parse::<u16>()
                .map_err(|_| io_err("bad port in endpoint"))?,
        ),
        None => (authority.to_string(), 80),
    };

    let mut stream = TcpStream::connect((host.as_str(), port))?;
    stream.set_read_timeout(Some(Duration::from_secs(20)))?;
    stream.set_write_timeout(Some(Duration::from_secs(20)))?;

    let header = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\n\
         Content-Type: application/octet-stream\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;
    let text = String::from_utf8_lossy(&raw).into_owned();
    Ok(text
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .unwrap_or(text))
}

fn io_err(msg: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, msg)
}

/// True if the cert's KeyUsage extension (OID 2.5.29.15) asserts
/// `digitalSignature` (bit 0) — i.e. it is the SIGNING cert, not the
/// key-agreement/encryption cert.
fn has_digital_signature_usage(der: &[u8]) -> bool {
    const KU_OID: &[u8] = &[0x55, 0x1D, 0x0F];
    let mut i = 0usize;
    while i + 2 + KU_OID.len() < der.len() {
        if der[i] == 0x06
            && der[i + 1] as usize == KU_OID.len()
            && &der[i + 2..i + 2 + KU_OID.len()] == KU_OID
        {
            let mut j = i + 2 + KU_OID.len();
            if der.get(j) == Some(&0x01) {
                j += 2 + der.get(j + 1).copied().unwrap_or(0) as usize; // skip critical BOOLEAN
            }
            if der.get(j) == Some(&0x04) {
                // extnValue OCTET STRING -> BIT STRING { unused, bits }
                let inner = j + 2;
                if der.get(inner) == Some(&0x03) {
                    // der[inner+1]=len, der[inner+2]=unused-bits, der[inner+3]=first content byte
                    return der.get(inner + 3).map(|b| b & 0x80 != 0).unwrap_or(false);
                }
            }
        }
        i += 1;
    }
    false
}
