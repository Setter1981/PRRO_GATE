//! prro_sidecar — axum HTTP fiscal signing server.
//!
//! Usage: prro_sidecar [config.toml]  (default: sidecar.toml in CWD)
//!
//! Routes:
//!   POST /fiscal/send  — canonical JSON → XML → CMS sign → DPS gRPC
//!   GET  /health/live  — always 200
//!   GET  /health/ready — 200 if active license present, 503 otherwise

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use tokio::net::TcpListener;
use tracing::{info, warn};

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
    cms_adapter,
    config::{CredentialsMode, SidecarConfig},
    credentials,
    errors::SidecarError,
    generated::{check::Type as CheckType, Check},
    grpc_client::DpsGrpcPool,
    input::{CanonicalCommand, OperationType},
    license::LicenseState,
    repo::Repo,
    xml_builder::{self, BuildContext},
};

/// Wall-clock cap for a single /fiscal/send request (TSP + gRPC + signing).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

// ── Shared state ──────────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    config:    Arc<SidecarConfig>,
    repo:      Arc<Repo>,
    grpc_pool: Arc<DpsGrpcPool>,
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "sidecar.toml".to_string());

    let config = SidecarConfig::from_toml_file(&config_path).unwrap_or_else(|e| {
        eprintln!("config error: {e}");
        std::process::exit(1);
    });

    let subscriber = tracing_subscriber::fmt().with_env_filter(
        tracing_subscriber::EnvFilter::new(&config.sidecar.log_level),
    );
    if config.dev.log_pretty {
        subscriber.pretty().init();
    } else {
        subscriber.json().init();
    }

    let repo = Repo::open(&config.db.path).unwrap_or_else(|e| {
        eprintln!("db error: {e}");
        std::process::exit(1);
    });

    let grpc_pool = DpsGrpcPool::new(&config.dps.prod, &config.dps.test).unwrap_or_else(|e| {
        eprintln!("gRPC pool: {e}");
        std::process::exit(1);
    });

    let bind = config.sidecar.bind.clone();
    let state = AppState {
        config:    Arc::new(config),
        repo:      Arc::new(repo),
        grpc_pool: Arc::new(grpc_pool),
    };

    let app = Router::new()
        .route("/fiscal/send",  post(handle_fiscal_send))
        .route("/health/live",  get(health_live))
        .route("/health/ready", get(health_ready))
        .with_state(state);

    info!(bind, "prro_sidecar starting");
    let listener = TcpListener::bind(&bind).await.unwrap_or_else(|e| {
        eprintln!("bind {bind}: {e}");
        std::process::exit(1);
    });
    axum::serve(listener, app).await.unwrap_or_else(|e| {
        eprintln!("server: {e}");
        std::process::exit(1);
    });
}

// ── Health ────────────────────────────────────────────────────────────────────

async fn health_live() -> StatusCode {
    StatusCode::OK
}

async fn health_ready(State(st): State<AppState>) -> StatusCode {
    match st.repo.load_active_license() {
        Ok(_)  => StatusCode::OK,
        Err(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

// ── Fiscal send ───────────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
struct FiscalSendResponse {
    status:        i32,
    fiscal_id:     String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_message: Option<String>,
}

async fn handle_fiscal_send(
    State(st): State<AppState>,
    Json(cmd): Json<CanonicalCommand>,
) -> impl IntoResponse {
    // Per-request wall-clock timeout: TSP HTTP (≤5 s) + gRPC (≤? s) + signing.
    match tokio::time::timeout(REQUEST_TIMEOUT, fiscal_send_inner(&st, cmd)).await {
        Ok(Ok(r))  => (StatusCode::OK, Json(r)).into_response(),
        Ok(Err(e)) => e.into_response(),
        Err(_)     => SidecarError::Internal("request timeout".into()).into_response(),
    }
}

async fn fiscal_send_inner(
    st:  &AppState,
    cmd: CanonicalCommand,
) -> Result<FiscalSendResponse, SidecarError> {
    // ── 1. Validate operation ─────────────────────────────────────────────────
    if !cmd.operation_type.is_sidecar_supported() {
        return Err(SidecarError::BadRequest(format!(
            "operation {:?} not supported by this sidecar version",
            cmd.operation_type
        )));
    }

    let fn_id = &cmd.fiscal_number;

    // ── 2. dev.skip_sign bypass (Invariant 10) ────────────────────────────────
    // Validated at config load: requires DEV_MODE env var. Only available in
    // non-production builds — returns raw XML size, never calls DPS.
    if st.config.dev.skip_sign {
        warn!(fn_id, "dev.skip_sign=true: bypassing CMS sign and DPS send");
        // build_dev_xml loads fn_config internally because the normal path
        // (step 3 below) would bail before reaching here in non-dev mode.
        let xml_bytes = build_dev_xml(st, fn_id, &cmd)?;
        return Ok(FiscalSendResponse {
            status:        1, // synthetic OK
            fiscal_id:     String::new(),
            error_message: Some(format!("dev.skip_sign: {} bytes XML, DPS skipped", xml_bytes.len())),
        });
    }

    // ── 3. Load config, license, operator (short independent DB locks) ─────────
    let fn_config = st.repo.load_fn_config(fn_id)?;
    let lic_row   = st.repo.load_active_license()?;
    let operator  = st.repo.load_active_operator(fn_id)?;

    // ── 4. Verify license (sig + expiry + TIN + FN membership) ───────────────
    let now = time::OffsetDateTime::now_utc();
    let lic_state = prro_sidecar::license::verify(
        &lic_row.payload_b64,
        &lic_row.signature_b64,
        fn_id,
        &fn_config.tax_number,
        now,
    )
    .map_err(|e| SidecarError::License(e.to_string()))?;

    match lic_state {
        LicenseState::Valid | LicenseState::Grace { .. } | LicenseState::Demo { .. } => {}
        LicenseState::Expired => {
            return Err(SidecarError::License("license expired".into()));
        }
        LicenseState::TinMismatch { in_license, requested } => {
            return Err(SidecarError::License(format!(
                "TIN mismatch: license={in_license:?} config={requested:?}"
            )));
        }
        LicenseState::FnNotLicensed { fn_ } => {
            return Err(SidecarError::License(format!(
                "fiscal number {fn_:?} not covered by active license"
            )));
        }
        LicenseState::SignatureInvalid => {
            return Err(SidecarError::License("license signature invalid".into()));
        }
    }

    // ── 5. Cert metadata — always needed for validity check (C-1) ────────────
    // Invariant: signing with an expired cert produces DPS ERROR_VEREFY (-1)
    // which wastes a local_number slot and may corrupt the MAC chain.
    let cert_meta = st.repo.load_operator_cert_metadata(fn_id)?;
    if !cert_meta.is_valid_at(now) {
        return Err(SidecarError::License(format!(
            "operator cert for {fn_id} is expired or not yet valid \
             (valid_to={:?}); renew the certificate before sending documents",
            cert_meta.valid_to
        )));
    }

    // ── 6. Decode JKS password ────────────────────────────────────────────────
    let raw_pw = match st.config.security.credentials_mode {
        CredentialsMode::Plain => operator.jks_password.clone(),
        CredentialsMode::XorSoft => {
            let valid_to = cert_meta.valid_to.as_deref().unwrap_or("");
            let op_name  = operator.operator_name.as_deref().unwrap_or("");
            credentials::decode_password(&operator.jks_password, valid_to, op_name)
                .map_err(SidecarError::Credentials)?
        }
    };

    // ── 7. Load JKS and extract private key (async IO, no DB lock held) ───────
    let jks_bytes = tokio::fs::read(&operator.jks_path).await.map_err(|e| {
        SidecarError::Internal(format!("read jks: {e}"))
    })?;
    let extracted = extract_private_key(&jks_bytes, &raw_pw)
        .map_err(|e| SidecarError::Credentials(e.to_string()))?;

    // ── 8. Resolve cert DER ───────────────────────────────────────────────────
    // Key6Dat containers embed no cert — fall back to operator_certs table.
    let cert_der: Vec<u8> = if !extracted.certs.is_empty() {
        extracted.certs[0].clone()
    } else {
        st.repo.load_cert_der_for_fn(fn_id)?
    };

    // ── 9. Allocate local_number and load previous_hash (two short locks) ─────
    // local_num is incremented here — before CMS signing (step 11) and gRPC (step 12).
    // If either step fails, the counter has already advanced: the gap in the sequence
    // is visible in the audit log. This is an inherent trade-off of SQLite + gRPC
    // without 2PC. DPS does not require a gapless sequence; it only validates monotonicity.
    let local_num     = st.repo.next_local_number(fn_id)?;
    let previous_hash = st.repo.load_previous_hash(fn_id)?;

    // ── 10. Build cp1251 XML ──────────────────────────────────────────────────
    let device_name    = st.config.sidecar.device_name.as_deref().unwrap_or("ПРО_каса");
    let device_version = st.config.sidecar.device_version.as_deref().unwrap_or("1.1");
    let build_ctx = BuildContext {
        local_number:   local_num as i64,
        z_number:       0,
        previous_hash:  &previous_hash,
        tax_number:     &fn_config.tax_number,
        tax_groups:     None,
        device_name,
        device_version,
    };
    let xml_bytes = xml_builder::build(&cmd, &build_ctx)?;

    // ── 11. CMS sign ──────────────────────────────────────────────────────────
    // Invariant (1): TSP URL resolved in a short separate DB lock BEFORE the
    // HTTP call to the TSP server — never holds the lock during network I/O.
    let d           = FieldEl::from_le_bytes(&extracted.param_d[..], 9);
    let dstu_signer = DstuInProcessSigner::new(d);
    let profile     = CmsProfile::default();
    let signing_ts  = Some(
        std::time::UNIX_EPOCH
            + std::time::Duration::from_secs(now.unix_timestamp() as u64),
    );
    let opts = CmsBuildOptions { attached: false, signing_time: signing_ts };
    let cms_signer = CmsSigner {
        cert_der: &cert_der,
        signer:   &dstu_signer as &dyn RawSigner,
        profile,
    };

    let cms_der = if fn_config.tsp_enabled {
        let issuer_dn = cms_adapter::extract_issuer_dn(&cert_der)
            .map_err(|e| SidecarError::CmsSign(e.to_string()))?;
        // Short lock: resolve TSP URL, release, then make HTTP call outside any lock.
        let tsp_url = st.repo.load_tsp_url_by_issuer_dn(&issuer_dn)?;
        let timeout = Duration::from_millis(
            st.config.tsp.as_ref().map(|t| t.timeout_ms).unwrap_or(5_000),
        );
        cms_signer
            .sign_with_tsp(&xml_bytes, opts, &tsp_url, timeout)
            .map_err(|e| SidecarError::CmsSign(e.to_string()))?
            .cms_der
    } else {
        cms_signer
            .sign_with(&xml_bytes, opts)
            .map_err(|e| SidecarError::CmsSign(e.to_string()))?
            .cms_der
    };

    // ── 12. Send to DPS via gRPC ──────────────────────────────────────────────
    let check_type = match cmd.operation_type {
        OperationType::Sell | OperationType::Return => CheckType::Chk as i32,
        OperationType::ZReport                      => CheckType::Zreport as i32,
        _                                           => CheckType::Servicechk as i32,
    };
    let check = Check {
        rro_fn:       fn_id.clone(),
        date_time:    now.unix_timestamp(),
        check_sign:   cms_der,
        local_number: local_num,
        check_type,
        id_offline:   String::new(),
        id_cancel:    String::new(),
    };
    let resp = st
        .grpc_pool
        .send_chk_v2(&fn_config.fiscal_mode, check)
        .await
        .map_err(|e| SidecarError::Grpc(e.to_string()))?;

    // ── 13. Persist MAC hash only for DPS-accepted documents ─────────────────
    // data_sign from an error response MUST NOT be stored — using a rejected
    // document's MAC as previous_hash would break the chain for the retry.
    // DPS status=1 (OK) is the only accepted status; all negative codes are errors.
    if resp.status > 0 && !resp.data_sign.is_empty() {
        let mac_hex = hex::encode(&resp.data_sign);
        if let Err(e) = st.repo.store_previous_hash(fn_id, &mac_hex) {
            // Best-effort: log but don't fail — the document was accepted by DPS.
            tracing::warn!(fn_id, error = %e, "failed to persist previous_hash");
        }
    }

    let error_msg = if resp.error_message.is_empty() {
        None
    } else {
        Some(resp.error_message.clone())
    };
    Ok(FiscalSendResponse {
        status:        resp.status,
        fiscal_id:     resp.id.clone(),
        error_message: error_msg,
    })
}

/// Build XML for dev.skip_sign mode (no signing, no DPS).
/// local_number=0 is a sentinel — never sent to DPS, does not consume the sequence.
fn build_dev_xml(st: &AppState, fn_id: &str, cmd: &CanonicalCommand) -> Result<Vec<u8>, SidecarError> {
    let fn_config     = st.repo.load_fn_config(fn_id)?;
    let previous_hash = st.repo.load_previous_hash(fn_id)?;
    let device_name    = st.config.sidecar.device_name.as_deref().unwrap_or("ПРО_каса");
    let device_version = st.config.sidecar.device_version.as_deref().unwrap_or("1.1");
    let ctx = BuildContext {
        local_number:   0,
        z_number:       0,
        previous_hash:  &previous_hash,
        tax_number:     &fn_config.tax_number,
        tax_groups:     None,
        device_name,
        device_version,
    };
    xml_builder::build(cmd, &ctx)
}
