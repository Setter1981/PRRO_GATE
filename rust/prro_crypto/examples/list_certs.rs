//! Crypto-debug: list every cert in a key container with index, KeyUsage,
//! and SubjectKeyIdentifier — to find the SIGNING cert (the one that must be
//! embedded in a CMS signature; certs[0] may be the encryption cert).
//! usage: list_certs <container> <password>
use prro_crypto::interop::prro::containers::extract_private_key;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let [_, path, pass] = a.as_slice() else {
        eprintln!("usage: list_certs <container> <password>");
        std::process::exit(1);
    };
    let data = std::fs::read(path).unwrap();
    let ek = extract_private_key(&data, pass).unwrap();
    println!("certs in container: {}", ek.certs.len());
    for (i, c) in ek.certs.iter().enumerate() {
        // crude scan for KeyUsage (OID 2.5.29.15 = 55 1D 0F) extension value
        // and SubjectKeyIdentifier (2.5.29.14 = 55 1D 0E).
        let ku = find_ext(c, &[0x55, 0x1D, 0x0F]);
        let ski = find_ext(c, &[0x55, 0x1D, 0x0E]);
        println!(
            "  cert[{i}] len={}  KeyUsage(ext)={}  SKI(ext)={}",
            c.len(),
            ku.map(|v| hex(&v)).unwrap_or_else(|| "?".into()),
            ski.map(|v| hex(&v)).unwrap_or_else(|| "?".into()),
        );
    }
}

/// Find the extnValue OCTET STRING bytes following an extension OID.
fn find_ext(der: &[u8], oid: &[u8]) -> Option<Vec<u8>> {
    let mut i = 0;
    while i + oid.len() + 2 < der.len() {
        // OID TLV: 06 <len> <oid bytes>
        if der[i] == 0x06
            && der[i + 1] as usize == oid.len()
            && &der[i + 2..i + 2 + oid.len()] == oid
        {
            // next TLV after the OID is either BOOLEAN(critical) then OCTET STRING, or OCTET STRING.
            let mut j = i + 2 + oid.len();
            if der.get(j) == Some(&0x01) {
                // skip BOOLEAN critical (01 01 ff/00)
                j += 2 + der.get(j + 1).copied().unwrap_or(0) as usize;
            }
            if der.get(j) == Some(&0x04) {
                let len = der[j + 1] as usize;
                return Some(der[j + 2..j + 2 + len].to_vec());
            }
        }
        i += 1;
    }
    None
}
