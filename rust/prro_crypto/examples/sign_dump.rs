//! Crypto-debug: sign `content` ATTACHED CAdES-BES (the production profile) with
//! the given key container and write the `.p7s` DER to `out`, for byte/asn1
//! comparison against EUSignCP reference vectors (`czo_test/*.p7s`).
//!
//! usage: sign_dump <key-container> <password> <content-string> <out.p7s>

use prro_crypto::cms::builder::{CmsBuildOptions, CmsSigner};
use prro_crypto::cms::profile::CmsProfile;
use prro_crypto::cms::signer::DstuInProcessSigner;
use prro_crypto::core::curve::Curve;
use prro_crypto::core::field::FieldEl;
use prro_crypto::interop::prro::containers::extract_private_key;
use std::time::SystemTime;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let [_, path, pass, content, out] = a.as_slice() else {
        eprintln!("usage: sign_dump <key> <password> <content> <out.p7s>");
        std::process::exit(1);
    };
    let data = std::fs::read(path).unwrap_or_else(|e| {
        eprintln!("read {path}: {e}");
        std::process::exit(2);
    });
    let ek = extract_private_key(&data, pass).unwrap_or_else(|e| {
        eprintln!("extract: {e:?}");
        std::process::exit(3);
    });
    let cert: &[u8] = ek.certs.first().map(Vec::as_slice).unwrap_or_else(|| {
        eprintln!("no cert");
        std::process::exit(4);
    });
    let curve = Curve::dstu_pb_257();
    let d = FieldEl::from_le_bytes(&ek.param_d[..], curve.mod_words);
    let signer = DstuInProcessSigner::new(d);
    let der = CmsSigner {
        cert_der: cert,
        signer: &signer,
        profile: CmsProfile::Dstu4145WithGost34311Pb,
    }
    .sign_with(
        content.as_bytes(),
        CmsBuildOptions {
            attached: true,
            signing_time: Some(SystemTime::now()),
        },
    )
    .unwrap_or_else(|e| {
        eprintln!("sign: {e:?}");
        std::process::exit(5);
    })
    .cms_der;
    std::fs::write(out, &der).unwrap();
    eprintln!("wrote {} bytes -> {out}", der.len());
}
