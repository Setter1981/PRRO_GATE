use prro_crypto::interop::prro::containers::extract_private_key;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (path, pass, out) = match args.as_slice() {
        [_, p, pw, o] => (p.as_str(), pw.as_str(), o.as_str()),
        _ => {
            eprintln!("usage: dump_cert <zs2-or-pfx> <password> <out.der>");
            std::process::exit(1);
        }
    };
    let data = std::fs::read(path).unwrap_or_else(|e| {
        eprintln!("read {path}: {e}");
        std::process::exit(2);
    });
    let extracted = extract_private_key(&data, pass).unwrap_or_else(|e| {
        eprintln!("extract: {e:?}");
        std::process::exit(3);
    });
    println!(
        "format={:?}  certs={}",
        extracted.format,
        extracted.certs.len()
    );
    if extracted.certs.is_empty() {
        eprintln!("no certs in container");
        std::process::exit(4);
    }
    std::fs::write(out, &extracted.certs[0]).unwrap();
    println!("saved {} bytes -> {out}", extracted.certs[0].len());
}
