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
    /// Boot the gateway and serve until SIGINT/SIGTERM.
    Serve {
        #[arg(long)]
        config: PathBuf,
    },
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
            let _app = App::boot(cfg).await?; // boot triggers migrate
            tracing::info!("migrations applied");
            Ok(())
        }
        Cmd::Serve { config } => {
            let text = std::fs::read_to_string(&config)?;
            let cfg = AppConfig::from_toml(&text)?;
            let app = App::boot(cfg).await?;
            tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                "prro listening (M1 — idle)"
            );
            // M3+ adds the supervisor + ingress shells.  M1 just idles.
            signal::ctrl_c().await?;
            tracing::info!("shutting down");
            drop(app);
            Ok(())
        }
    }
}
