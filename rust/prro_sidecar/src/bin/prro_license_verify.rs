//! Dry-run verifier — verify a license file against the embedded master pubkey.
//!
//! Usage:
//!   prro_license_verify --license license.json --tin 1234567890 --fn 3001234567
//!   prro_license_verify --license license.json --tin 1234567890 --fn 3001234567 \
//!       --now 2027-04-19T12:00:00Z
//!
//! Output JSON to stdout (for jq):
//!   { "state": "Grace", "days_left": 7, "tier": "pro", "tin": "1234567890" }
//!
//! Exit code: 0 on Valid/Grace/Demo, 1 on Expired/TinMismatch/FnNotLicensed/SignatureInvalid.

use std::path::PathBuf;
use clap::Parser;
use serde_json::json;
use prro_sidecar::license::{self, LicensePayload, LicenseState};
use base64::{engine::general_purpose::STANDARD as B64, Engine};

#[derive(Parser)]
#[command(name = "prro_license_verify", about = "Verify a license file against the embedded master pubkey")]
struct Cli {
    /// Path to license JSON file (from prro_license_sign)
    #[arg(long)]
    license: PathBuf,

    /// TIN to check membership for
    #[arg(long)]
    tin: String,

    /// Fiscal number to check membership for
    #[arg(long = "fn")]
    fn_: String,

    /// Override current time (ISO-8601, default: now)
    #[arg(long)]
    now: Option<String>,
}

fn main() {
    let cli = Cli::parse();

    let file_content = std::fs::read_to_string(&cli.license)
        .unwrap_or_else(|e| { eprintln!("error reading {:?}: {e}", cli.license); std::process::exit(2); });

    let doc: serde_json::Value = serde_json::from_str(&file_content)
        .unwrap_or_else(|e| { eprintln!("JSON parse error: {e}"); std::process::exit(2); });

    let payload_b64 = doc["payload_b64"].as_str()
        .unwrap_or_else(|| { eprintln!("missing payload_b64"); std::process::exit(2); });
    let signature_b64 = doc["signature_b64"].as_str()
        .unwrap_or_else(|| { eprintln!("missing signature_b64"); std::process::exit(2); });

    // Parse the payload JSON for metadata to include in output
    let payload_bytes = B64.decode(payload_b64)
        .unwrap_or_else(|e| { eprintln!("base64 decode error: {e}"); std::process::exit(2); });
    let payload: LicensePayload = serde_json::from_slice(&payload_bytes)
        .unwrap_or_else(|e| { eprintln!("payload JSON error: {e}"); std::process::exit(2); });

    let now = if let Some(ts) = &cli.now {
        time::OffsetDateTime::parse(ts, &time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|e| { eprintln!("invalid --now '{ts}': {e}"); std::process::exit(2); })
    } else {
        time::OffsetDateTime::now_utc()
    };

    let state = license::verify(payload_b64, signature_b64, &cli.fn_, &cli.tin, now)
        .unwrap_or_else(|e| { eprintln!("verify error: {e}"); std::process::exit(2); });

    let tier_str = serde_json::to_value(&payload.tier).unwrap()
        .as_str().unwrap_or("unknown").to_string();

    let (output, exit_code) = match &state {
        LicenseState::Valid => (
            json!({"state":"Valid","tier":tier_str,"tin":payload.tin}),
            0,
        ),
        LicenseState::Grace { days_left } => (
            json!({"state":"Grace","days_left":days_left,"tier":tier_str,"tin":payload.tin}),
            0,
        ),
        LicenseState::Demo { limits } => (
            json!({
                "state": "Demo",
                "tier": tier_str,
                "tin": payload.tin,
                "max_payment_sum_kopecks": limits.max_payment_sum_kopecks,
                "no_offline": limits.no_offline,
            }),
            0,
        ),
        LicenseState::Expired => (
            json!({"state":"Expired","tier":tier_str,"tin":payload.tin,"expires_at":payload.expires_at}),
            1,
        ),
        LicenseState::TinMismatch { in_license, requested } => (
            json!({"state":"TinMismatch","in_license":in_license,"requested":requested}),
            1,
        ),
        LicenseState::FnNotLicensed { fn_ } => (
            json!({"state":"FnNotLicensed","fn":fn_}),
            1,
        ),
        LicenseState::SignatureInvalid => (
            json!({"state":"SignatureInvalid"}),
            1,
        ),
    };

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
    std::process::exit(exit_code);
}
