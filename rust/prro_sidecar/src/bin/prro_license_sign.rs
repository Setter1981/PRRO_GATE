//! Sign license payloads with the DSTU master private key.
//!
//! Single mode:
//!   prro_license_sign --master master.key \
//!       --tin 1234567890 --fn 3001234567 --fn 3001234568 \
//!       --expires-at 2027-04-19 --tier pro --org-name "ТОВ Магазин" \
//!       --out license.json
//!
//! Batch mode:
//!   prro_license_sign --master master.key --from-csv customers.csv --out-dir licenses/
//!
//! Output JSON format:
//!   { "payload_b64": "...", "signature_b64": "...", "payload": {...} }

use std::path::PathBuf;
use clap::Parser;
use serde_json::json;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use prro_sidecar::license::{DemoLimits, LicensePayload, LicenseTier};

#[derive(Parser)]
#[command(name = "prro_license_sign", about = "Sign license payloads with DSTU master key")]
struct Cli {
    /// Path to 32-byte LE private key from prro_license_keygen
    #[arg(long)]
    master: PathBuf,

    // ── Single-mode fields ──────────────────────────────────────────────────
    #[arg(long, conflicts_with = "from_csv")]
    tin: Option<String>,

    #[arg(long = "fn", conflicts_with = "from_csv")]
    fn_numbers: Vec<String>,

    #[arg(long, conflicts_with = "from_csv")]
    expires_at: Option<String>,

    #[arg(long, conflicts_with = "from_csv", value_parser = parse_tier)]
    tier: Option<LicenseTier>,

    #[arg(long, conflicts_with = "from_csv")]
    org_name: Option<String>,

    #[arg(long, conflicts_with = "from_csv")]
    out: Option<PathBuf>,

    // ── Batch mode ──────────────────────────────────────────────────────────
    #[arg(long, conflicts_with_all = ["tin", "fn_numbers", "expires_at", "tier", "out"])]
    from_csv: Option<PathBuf>,

    #[arg(long)]
    out_dir: Option<PathBuf>,
}

fn parse_tier(s: &str) -> Result<LicenseTier, String> {
    match s {
        "demo"       => Ok(LicenseTier::Demo),
        "basic"      => Ok(LicenseTier::Basic),
        "pro"        => Ok(LicenseTier::Pro),
        "enterprise" => Ok(LicenseTier::Enterprise),
        other => Err(format!("unknown tier '{other}'; choose demo|basic|pro|enterprise")),
    }
}

fn load_private_key(path: &PathBuf) -> Vec<u8> {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|e| { eprintln!("error reading master key {:?}: {e}", path); std::process::exit(1); });
    if bytes.len() != 32 {
        eprintln!("error: master key must be exactly 32 bytes, got {}", bytes.len());
        std::process::exit(1);
    }
    bytes
}

fn sign_license(payload: &LicensePayload, d_bytes: &[u8]) -> String {
    use prro_crypto::core::curve::Curve;
    use prro_crypto::core::field::FieldEl;
    use prro_crypto::core::hash::gost_34_311_95;
    use prro_crypto::cms::signer::{DstuInProcessSigner, RawSigner};

    let curve = Curve::dstu_pb_257();
    let d     = FieldEl::from_le_bytes(d_bytes, curve.mod_words);
    let jcs   = payload.to_canonical_bytes();
    let hash  = gost_34_311_95(&jcs);

    let signer   = DstuInProcessSigner::new(d);
    let sig_bytes = signer.sign_digest(&hash).expect("sign_digest must succeed");
    B64.encode(&sig_bytes)
}

fn write_license_file(path: &PathBuf, payload: &LicensePayload, d_bytes: &[u8]) {
    let payload_b64   = B64.encode(payload.to_canonical_bytes());
    let signature_b64 = sign_license(payload, d_bytes);
    let doc = json!({
        "payload_b64":   payload_b64,
        "signature_b64": signature_b64,
        "payload":       serde_json::to_value(payload).unwrap(),
    });
    let json_str = serde_json::to_string_pretty(&doc).unwrap();
    std::fs::write(path, json_str)
        .unwrap_or_else(|e| { eprintln!("error writing {:?}: {e}", path); std::process::exit(1); });
    println!("Written: {:?}", path);
}

fn build_single_payload(cli: &Cli) -> LicensePayload {
    let tin        = cli.tin.clone().unwrap_or_else(|| { eprintln!("--tin required in single mode"); std::process::exit(1); });
    let fn_numbers = if cli.fn_numbers.is_empty() { eprintln!("at least one --fn required"); std::process::exit(1); } else { cli.fn_numbers.clone() };
    let expires_at = cli.expires_at.clone().unwrap_or_else(|| { eprintln!("--expires-at required"); std::process::exit(1); });
    let tier       = cli.tier.unwrap_or_else(|| { eprintln!("--tier required"); std::process::exit(1); });

    // Refuse past-dated expiry
    let exp = time::OffsetDateTime::parse(&format!("{expires_at}T00:00:00Z"),
        &time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|e| { eprintln!("invalid --expires-at '{expires_at}': {e}"); std::process::exit(1); });
    if exp < time::OffsetDateTime::now_utc() {
        eprintln!("error: --expires-at is in the past ({expires_at})");
        std::process::exit(1);
    }

    let demo_limits = if tier == LicenseTier::Demo {
        Some(DemoLimits { max_payment_sum_kopecks: 50_000, no_offline: true, require_dps_online: true })
    } else { None };

    let mut fn_sorted = fn_numbers;
    fn_sorted.sort();

    LicensePayload {
        schema_version: "license.v1".into(),
        tin,
        fn_numbers:     fn_sorted,
        issued_at:      time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339).unwrap(),
        expires_at:     format!("{expires_at}T00:00:00Z"),
        tier,
        org_name:       cli.org_name.clone(),
        demo_limits,
        issuer:         "PRRO_GATE_ADMIN".into(),
    }
}

fn process_csv(csv_path: &PathBuf, out_dir: &PathBuf, d_bytes: &[u8]) {
    let content = std::fs::read_to_string(csv_path)
        .unwrap_or_else(|e| { eprintln!("error reading CSV {:?}: {e}", csv_path); std::process::exit(1); });
    std::fs::create_dir_all(out_dir)
        .unwrap_or_else(|e| { eprintln!("error creating out-dir {:?}: {e}", out_dir); std::process::exit(1); });

    let mut lines = content.lines();
    let header = lines.next().unwrap_or("");
    let expected = "tin,fn_numbers,expires_at,tier,org_name";
    if header.trim() != expected {
        eprintln!("CSV header must be: {expected}\ngot: {header}");
        std::process::exit(1);
    }

    for (i, line) in lines.enumerate() {
        let fields: Vec<&str> = line.splitn(5, ',').collect();
        if fields.len() < 4 {
            eprintln!("row {}: expected at least 4 fields", i + 2);
            std::process::exit(1);
        }
        let tin        = fields[0].trim().to_string();
        let fn_numbers: Vec<String> = fields[1].trim().trim_matches('"')
            .split(';').map(str::trim).map(String::from).collect();
        let expires_at = fields[2].trim().to_string();
        let tier       = parse_tier(fields[3].trim())
            .unwrap_or_else(|e| { eprintln!("row {}: {e}", i + 2); std::process::exit(1); });
        let org_name   = fields.get(4).map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty());

        let demo_limits = if tier == LicenseTier::Demo {
            Some(DemoLimits { max_payment_sum_kopecks: 50_000, no_offline: true, require_dps_online: true })
        } else { None };

        let mut fn_sorted = fn_numbers;
        fn_sorted.sort();

        let payload = LicensePayload {
            schema_version: "license.v1".into(),
            tin: tin.clone(),
            fn_numbers:     fn_sorted,
            issued_at:      time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339).unwrap(),
            expires_at:     format!("{expires_at}T00:00:00Z"),
            tier,
            org_name,
            demo_limits,
            issuer: "PRRO_GATE_ADMIN".into(),
        };
        let out_path = out_dir.join(format!("{tin}.json"));
        write_license_file(&out_path, &payload, d_bytes);
    }
}

fn main() {
    let cli = Cli::parse();
    let d_bytes = load_private_key(&cli.master);

    if let Some(csv_path) = &cli.from_csv {
        let out_dir = cli.out_dir.clone().unwrap_or_else(|| {
            eprintln!("--out-dir required with --from-csv"); std::process::exit(1);
        });
        process_csv(csv_path, &out_dir, &d_bytes);
    } else {
        let payload = build_single_payload(&cli);
        let out = cli.out.clone().unwrap_or_else(|| {
            eprintln!("--out required in single mode"); std::process::exit(1);
        });
        write_license_file(&out, &payload, &d_bytes);
    }
}
