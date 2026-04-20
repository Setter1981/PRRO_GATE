//! prro_sidecar_preflight — onboarding health check.
//!
//! Usage: prro_sidecar_preflight <config.toml> <fiscal_number>
//!
//! 1. Opens config + DB
//! 2. Loads active operator; decodes and extracts the DSTU key
//! 3. Signs the fiscal_number bytes with CMS (matching the live infoRro protocol)
//! 4. Calls DPS infoRro and prints RRO metadata + operator list
//! 5. Exits 0 on success, 1 on error

use prro_crypto::{
    cms::{
        builder::{CmsBuildOptions, CmsSigner},
        signer::{DstuInProcessSigner, RawSigner},
        CmsProfile,
    },
    core::field::FieldEl,
    interop::prro::extract_private_key,
};
use prro_sidecar::{
    config::{CredentialsMode, SidecarConfig},
    credentials,
    generated::CheckRequest,
    grpc_client::DpsGrpcPool,
    repo::Repo,
};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let config_path   = args.next().unwrap_or_else(|| usage());
    let fiscal_number = args.next().unwrap_or_else(|| usage());

    let config = SidecarConfig::from_toml_file(&config_path).unwrap_or_else(|e| {
        eprintln!("config error: {e}");
        std::process::exit(1);
    });

    let repo = Repo::open(&config.db.path).unwrap_or_else(|e| {
        eprintln!("db error: {e}");
        std::process::exit(1);
    });

    let fn_config = repo.load_fn_config(&fiscal_number).unwrap_or_else(|e| {
        eprintln!("fn_config: {e}");
        std::process::exit(1);
    });

    let operator = repo.load_active_operator(&fiscal_number).unwrap_or_else(|e| {
        eprintln!("active operator: {e}");
        std::process::exit(1);
    });

    // Decode password using the same logic as the signing server.
    let raw_pw = match config.security.credentials_mode {
        CredentialsMode::Plain => operator.jks_password.clone(),
        CredentialsMode::XorSoft => {
            let cert_meta = repo.load_operator_cert_metadata(&fiscal_number).unwrap_or_else(|e| {
                eprintln!("cert metadata: {e}");
                std::process::exit(1);
            });
            let valid_to = cert_meta.valid_to.as_deref().unwrap_or("");
            let op_name  = operator.operator_name.as_deref().unwrap_or("");
            credentials::decode_password(&operator.jks_password, valid_to, op_name)
                .unwrap_or_else(|e| {
                    eprintln!("decode password: {e}");
                    std::process::exit(1);
                })
        }
    };

    // Load JKS and extract DSTU private key.
    let jks_bytes = std::fs::read(&operator.jks_path).unwrap_or_else(|e| {
        eprintln!("read jks {:?}: {e}", operator.jks_path);
        std::process::exit(1);
    });
    let extracted = extract_private_key(&jks_bytes, &raw_pw).unwrap_or_else(|e| {
        eprintln!("extract key: {e}");
        std::process::exit(1);
    });

    // Resolve cert DER (Key6Dat containers embed no cert).
    let cert_der: Vec<u8> = if !extracted.certs.is_empty() {
        extracted.certs[0].clone()
    } else {
        repo.load_cert_der_for_fn(&fiscal_number).unwrap_or_else(|e| {
            eprintln!("cert DER from DB: {e}");
            std::process::exit(1);
        })
    };

    println!("── Key container ─────────────────────────────────");
    println!("  format:    {:?}", extracted.format);
    println!("  certs:     {} embedded", extracted.certs.len());
    println!("  operator:  {}", operator.operator_name.as_deref().unwrap_or("-"));
    println!("  INN:       {}", operator.operator_inn);
    println!("  fiscal_fn: {fiscal_number}");
    println!("  mode:      {:?}", fn_config.fiscal_mode);

    // Build the CMS-signed rro_fn_sign per DPS infoRro protocol.
    // The signature input is the fiscal_number string as UTF-8 bytes.
    let d           = FieldEl::from_le_bytes(&extracted.param_d[..], 9);
    let dstu_signer = DstuInProcessSigner::new(d);
    let cms_signer  = CmsSigner {
        cert_der: &cert_der,
        signer:   &dstu_signer as &dyn RawSigner,
        profile:  CmsProfile::default(),
    };
    let opts = CmsBuildOptions { attached: false, signing_time: None };
    let sig = cms_signer
        .sign_with(fiscal_number.as_bytes(), opts)
        .unwrap_or_else(|e| {
            eprintln!("CMS sign failed: {e}");
            std::process::exit(1);
        });

    let req = CheckRequest { rro_fn_sign: sig.cms_der };

    // gRPC pool with lazy connect — construction never fails due to network.
    let pool = DpsGrpcPool::new(&config.dps.prod, &config.dps.test).unwrap_or_else(|e| {
        eprintln!("gRPC pool: {e}");
        std::process::exit(1);
    });

    print!("\ncalling DPS infoRro({fiscal_number}, mode={:?}) ... ", fn_config.fiscal_mode);
    match pool.info_rro(&fn_config.fiscal_mode, req).await {
        Ok(info) => {
            println!("OK");
            println!("── RRO info ──────────────────────────────────────");
            println!("  name:            {}", info.name);
            println!("  address:         {}", info.addr);
            println!("  status_rro:      {}", info.status_rro);
            println!("  open_shift:      {}", info.open_shift);
            println!("  online:          {}", info.online);
            println!("  offline_allowed: {}", info.offline_allowed);
            println!("  single_tax:      {}", info.single_tax);
            println!("  last_local_num:  {}", info.lnum);
            if !info.operators.is_empty() {
                println!("  operators ({}):", info.operators.len());
                for op in &info.operators {
                    println!(
                        "    serial={} senior={} status={} name={}",
                        op.serial, op.senior, op.status, op.isname
                    );
                }
            }
        }
        Err(e) => {
            eprintln!("infoRro failed: {e}");
            std::process::exit(1);
        }
    }
}

fn usage() -> ! {
    eprintln!("usage: prro_sidecar_preflight <config.toml> <fiscal_number>");
    std::process::exit(2);
}
