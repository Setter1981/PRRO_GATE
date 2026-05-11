use clap::{Parser, Subcommand};
use prro::{config::AppConfig, App};
use std::path::PathBuf;
use tokio::signal;

#[derive(Parser, Debug)]
#[command(name = "prro", version, about = "PRRO Gateway")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Print build info and exit.
    Version,
    /// Apply DB migrations (via App::boot) and exit.
    Migrate {
        #[arg(long)]
        config: PathBuf,
    },
    /// Run preflight checks (config, DB, lock, listen) and exit.
    Doctor {
        #[arg(long)]
        config: PathBuf,
    },
    /// Boot the gateway and serve until SIGINT/SIGTERM.
    Serve {
        #[arg(long)]
        config: PathBuf,
    },
}

/// Drive `App::boot` to completion; on `BootError` exit with the
/// variant's BSD sysexits code (per W9 freeze §5.5).  Returns the
/// `App` on success.  The error message is emitted to stderr via the
/// `Display` impl of `BootError` before exit.
async fn boot_or_exit(cfg: AppConfig) -> anyhow::Result<App> {
    match App::boot(cfg).await {
        Ok(app) => Ok(app),
        Err(boot_err) => {
            eprintln!("prro: boot failed: {boot_err}");
            std::process::exit(boot_err.exit_code());
        }
    }
}

/// Wait for a graceful-shutdown signal.
///
/// On Unix, return on either SIGINT or SIGTERM (systemd / docker stop default).
/// On Windows, only Ctrl-C is supported via tokio::signal — SIGTERM has no
/// equivalent there, so the function reduces to ctrl_c().
async fn await_shutdown_signal() -> anyhow::Result<&'static str> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate())?;
        tokio::select! {
            _ = signal::ctrl_c() => Ok("SIGINT"),
            _ = term.recv()      => Ok("SIGTERM"),
        }
    }
    #[cfg(not(unix))]
    {
        signal::ctrl_c().await?;
        Ok("Ctrl-C")
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Version => {
            println!("prro {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Cmd::Migrate { config } => {
            let text = std::fs::read_to_string(&config)?;
            let cfg = AppConfig::from_toml(&text)?;
            // W9: App::boot now acquires the singleton lock internally
            // (consolidated pre-flight pipeline); no separate acquire here.
            let _app = boot_or_exit(cfg).await?;
            tracing::info!("migrations applied");
            Ok(())
        }
        Cmd::Doctor { config } => prro::doctor::run(&config).await,
        Cmd::Serve { config } => {
            let text = std::fs::read_to_string(&config)?;
            let cfg = AppConfig::from_toml(&text)?;
            let app = boot_or_exit(cfg).await?;
            tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                "prro listening (M1 — idle)"
            );
            // M3+ adds the supervisor + ingress shells.  M1 just idles.
            let signal_name = await_shutdown_signal().await?;
            tracing::info!(signal = signal_name, "shutting down");
            drop(app);
            Ok(())
        }
    }
}
