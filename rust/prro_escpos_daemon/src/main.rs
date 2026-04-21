//! `prro_escpos_daemon` — universal ESC/POS print service.
//!
//! Binary entrypoint: parse CLI / config, build the `AppState`, bind
//! the HTTP listener, serve until SIGTERM.

use clap::Parser;
use prro_escpos_daemon::{
    api::{build_router, AppState},
    config::Config,
    ProfileRegistry,
};
use std::sync::Arc;
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Parser, Debug)]
#[command(name = "prro_escpos_daemon", about, version)]
struct Cli {
    /// Path to a TOML config file.  When absent, built-in defaults
    /// are used (loopback bind on 127.0.0.1:9105).
    #[arg(long, env = "PRRO_ESCPOSD_CONFIG")]
    config: Option<std::path::PathBuf>,

    /// Override `listen` from config.
    #[arg(long, env = "PRRO_ESCPOSD_LISTEN")]
    listen: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Structured logs to stderr — daemons stream logs via journald /
    // file redirect; JSON flag can be added via RUST_LOG_FORMAT later.
    fmt()
        .with_env_filter(EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info,tower_http=warn")))
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let mut cfg = if let Some(path) = cli.config.as_ref() {
        let text = std::fs::read_to_string(path)?;
        Config::from_toml_str(&text)?
    } else {
        Config::default()
    };
    if let Some(listen) = cli.listen {
        cfg.listen = listen;
    }

    let mut registry = ProfileRegistry::new();
    if let Some(dir) = cfg.profiles_dir.as_ref() {
        registry = registry.with_extra_dir(dir);
    }

    let listener = tokio::net::TcpListener::bind(&cfg.listen).await?;
    let addr = listener.local_addr()?;
    tracing::info!(%addr, "prro_escpos_daemon listening");

    let state = AppState {
        registry: Arc::new(registry),
        default_timeout_ms: cfg.default_timeout_ms,
    };
    let app = build_router(state);

    axum::serve(listener, app).await?;
    Ok(())
}
