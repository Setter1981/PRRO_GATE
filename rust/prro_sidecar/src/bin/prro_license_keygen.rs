//! Generate a DSTU 4145 PB-257 master keypair for license signing.
//!
//! Usage:
//!   prro_license_keygen --out-priv master.key --out-pub master.pub.der [--force]
//!
//! Private key: 32 raw LE bytes (DSTU scalar d).
//! Public key:  33 raw bytes (compressed DSTU PB-257 point, sign‖x LE).

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "prro_license_keygen",
    about = "Generate DSTU master keypair for license signing"
)]
struct Cli {
    /// Output path for the 32-byte private key (LE scalar)
    #[arg(long)]
    out_priv: PathBuf,
    /// Output path for the 33-byte compressed public key
    #[arg(long)]
    out_pub: PathBuf,
    /// Overwrite existing files
    #[arg(long)]
    force: bool,
}

fn main() {
    let cli = Cli::parse();

    // Guard: refuse to overwrite unless --force
    for path in [&cli.out_priv, &cli.out_pub] {
        if path.exists() && !cli.force {
            eprintln!("error: {:?} already exists; use --force to overwrite", path);
            std::process::exit(1);
        }
    }

    use prro_crypto::core::curve::Curve;
    use prro_crypto::core::field::FieldEl;
    use prro_crypto::core::point::{compress_point, Point};
    use rand_core::RngCore;

    let curve = Curve::dstu_pb_257();

    // Rejection-sample a uniformly random private scalar d in [1, n)
    let d = loop {
        let mut buf = [0u8; 32];
        rand_core::OsRng
            .try_fill_bytes(&mut buf)
            .expect("OsRng must be available");
        let candidate = FieldEl::from_le_bytes(&buf, curve.mod_words);
        if !candidate.is_zero() {
            break candidate;
        }
    };

    // Q = -d * G (DSTU 4145 convention)
    let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
    let pub_q = g.mul(&d, &curve).negate();
    let pub_compressed = compress_point(&pub_q, &curve); // 33 bytes

    // Serialize d as 32 LE bytes
    let mut priv_bytes = vec![0u8; 32];
    for (wi, &word) in d.bytes.iter().enumerate() {
        for bi in 0..4usize {
            let idx = wi * 4 + bi;
            if idx < 32 {
                priv_bytes[idx] = (word >> (bi * 8)) as u8;
            }
        }
    }

    // Write
    std::fs::write(&cli.out_priv, &priv_bytes).unwrap_or_else(|e| {
        eprintln!("error writing {:?}: {e}", cli.out_priv);
        std::process::exit(1);
    });
    std::fs::write(&cli.out_pub, &pub_compressed).unwrap_or_else(|e| {
        eprintln!("error writing {:?}: {e}", cli.out_pub);
        std::process::exit(1);
    });

    println!("Private key → {:?} (32 bytes)", cli.out_priv);
    println!(
        "Public key  → {:?} (33 bytes, compressed DSTU PB-257)",
        cli.out_pub
    );
}
