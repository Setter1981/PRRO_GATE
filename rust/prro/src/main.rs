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
    /// Administrative operations (operator-only intervention paths).
    Admin {
        #[command(subcommand)]
        cmd: AdminCmd,
    },
}

#[derive(Subcommand, Debug)]
enum AdminCmd {
    /// **M3b W12 Hardening Phase 2a.2 / REC-1 Tier 3 (2026-05-24)** —
    /// reset STOP_MODE for a specific fiscal_number.  Use after
    /// operator-verified resolution of the root cause that triggered
    /// auto-escalation (50+ consecutive Hold ticks per Tier 2).
    ///
    /// Atomic: CAS node_state.mode STOP_MODE → GOING_ONLINE + reset
    /// consecutive_holds for all held docs on FN + emit Critical audit
    /// ADMIN_STOP_MODE_RESET.  Refuses if current mode != STOP_MODE
    /// (operator wrong-command guard).
    ResetStopMode {
        #[arg(long)]
        config: PathBuf,
        /// Fiscal number that escalated to STOP_MODE (must match a
        /// row in `node_state`).
        #[arg(long)]
        fiscal_number: String,
        /// Operator-supplied non-empty description for forensic audit
        /// trail.  Required (rejected if empty/whitespace).  Example:
        /// "DPS connectivity restored 2026-05-24T10:30 UTC; verified ping OK".
        #[arg(long)]
        reason: String,
    },

    /// W2 — register a cashier (operator) and bind their EDS key to a
    /// fiscal number.  Inserts a row into the secure DB's `operators`
    /// table.  Password is acquired interactively: TTY mode requires
    /// double-entry confirmation; non-TTY mode reads a single line
    /// from stdin (CI / scripted use).
    AddOperator {
        #[arg(long)]
        config: PathBuf,
        /// Cashier identifier — typically the cashier's INN.
        #[arg(long)]
        inn: String,
        /// Human-readable cashier name (forensic trail).
        #[arg(long)]
        name: String,
        /// Filesystem path to the cashier's `.dat` / `.jks` EDS carrier.
        #[arg(long)]
        key_path: String,
        /// Fiscal number to bind the cashier to.  Must exist in
        /// `fiscal_number_config` (main DB) — pre-INSERT check
        /// surfaces a clean error rather than letting the boot-time
        /// `OPERATOR_ORPHAN_FN` audit catch the typo hours later.
        #[arg(long = "fn")]
        fiscal_number: String,
    },

    // ─── W4-Z0 piece 8c — per-FN config management ─────────────────
    /// W4-Z0 — add a tax group for a fiscal number.
    AddTaxGroup {
        #[arg(long)]
        config: PathBuf,
        #[arg(long = "fn")]
        fiscal_number: String,
        #[arg(long)]
        tx_num: i64,
        #[arg(long)]
        letter: String,
        #[arg(long, default_value_t = 0.0)]
        dtpr: f64,
        #[arg(long, default_value_t = 0.0)]
        txpr: f64,
        #[arg(long, default_value_t = 0)]
        txal: i64,
    },
    /// W4-Z0 — update rate fields of an existing tax group.
    UpdateTaxRate {
        #[arg(long)]
        config: PathBuf,
        #[arg(long = "fn")]
        fiscal_number: String,
        #[arg(long)]
        tx_num: i64,
        #[arg(long)]
        dtpr: Option<f64>,
        #[arg(long)]
        txpr: Option<f64>,
        #[arg(long)]
        txal: Option<i64>,
    },
    /// W4-Z0 — soft-delete a tax group.
    RemoveTaxGroup {
        #[arg(long)]
        config: PathBuf,
        #[arg(long = "fn")]
        fiscal_number: String,
        #[arg(long)]
        tx_num: i64,
    },
    /// W4-Z0 — list active tax groups for a fiscal number.
    ListTaxGroups {
        #[arg(long)]
        config: PathBuf,
        #[arg(long = "fn")]
        fiscal_number: String,
    },

    /// W4-Z0 — add a payment method.
    AddPayment {
        #[arg(long)]
        config: PathBuf,
        #[arg(long = "fn")]
        fiscal_number: String,
        #[arg(long)]
        pay_index: i64,
        #[arg(long)]
        name: String,
        /// 1 = cash, 0 = cashless.  Default cashless.
        #[arg(long, default_value_t = false)]
        cash: bool,
    },
    /// W4-Z0 — update a payment method.
    UpdatePayment {
        #[arg(long)]
        config: PathBuf,
        #[arg(long = "fn")]
        fiscal_number: String,
        #[arg(long)]
        pay_index: i64,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        cash: Option<bool>,
    },
    /// W4-Z0 — soft-delete a payment method.
    RemovePayment {
        #[arg(long)]
        config: PathBuf,
        #[arg(long = "fn")]
        fiscal_number: String,
        #[arg(long)]
        pay_index: i64,
    },
    /// W4-Z0 — list active payment methods.
    ListPayments {
        #[arg(long)]
        config: PathBuf,
        #[arg(long = "fn")]
        fiscal_number: String,
    },

    /// W4-Z0 — set a per-FN integration flag.
    SetFlag {
        #[arg(long)]
        config: PathBuf,
        #[arg(long = "fn")]
        fiscal_number: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        value: String,
    },
    /// W4-Z0 — convenience alias for the Національний чек toggle
    /// (`useecheckmegovua` flag).
    SetNationalReceipt {
        #[arg(long)]
        config: PathBuf,
        #[arg(long = "fn")]
        fiscal_number: String,
        #[arg(long)]
        enabled: bool,
    },
    /// W4-Z0 — list all flags set for a fiscal number.
    ListFlags {
        #[arg(long)]
        config: PathBuf,
        #[arg(long = "fn")]
        fiscal_number: String,
    },

    /// W4-Z0 — add a driver→canonical TX number mapping.
    AddDriverMapping {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        driver_id: String,
        #[arg(long)]
        driver_number: i64,
        #[arg(long)]
        canonical: i64,
        #[arg(long)]
        letter: Option<String>,
    },
    /// W4-Z0 — update an existing driver mapping's canonical TX number.
    UpdateDriverMapping {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        driver_id: String,
        #[arg(long)]
        driver_number: i64,
        #[arg(long)]
        canonical: i64,
    },
    /// W4-Z0 — soft-delete a driver mapping.
    RemoveDriverMapping {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        driver_id: String,
        #[arg(long)]
        driver_number: i64,
    },
    /// W4-Z0 — list active driver mappings for a vendor.
    ListDriverMappings {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        driver_id: String,
    },

    /// W4-Z0 — set per-FN outgress profile (FSCO_ZZD | EVPZ_DPS).
    SetOutgressProfile {
        #[arg(long)]
        config: PathBuf,
        #[arg(long = "fn")]
        fiscal_number: String,
        #[arg(long)]
        profile: String,
    },
    /// W4-Z0 — show per-FN outgress profile.
    ShowOutgressProfile {
        #[arg(long)]
        config: PathBuf,
        #[arg(long = "fn")]
        fiscal_number: String,
    },

    /// W4-Z0 — explicit per-FN defaults bootstrap (recovery from
    /// failed `add-operator` bootstrap, or operator-driven re-seed).
    BootstrapDefaults {
        #[arg(long)]
        config: PathBuf,
        #[arg(long = "fn")]
        fiscal_number: String,
    },
}

/// Read config file → parse → boot.  On any `BootError`, prints the
/// `Display`-formatted error to stderr and `std::process::exit`s with
/// the variant's BSD sysexits code (per W9 freeze §5.5; W9.1 review
/// LOW 2: return signature is plain `App` because the Err arm never
/// returns — caller can use the return value directly without `?`).
///
/// **Hard-exit note (W9.1 review NIT 1):** `std::process::exit` is
/// called from inside the tokio runtime.  This is intentional for
/// fail-closed boot — no traffic has been accepted yet, no async
/// drains are needed; an immediate exit is correct.  Future
/// graceful-shutdown paths (post-serve) live elsewhere and use
/// signal-driven drains, NOT this helper.
async fn boot_from_path_or_exit(config: &std::path::Path) -> App {
    let result = (|| -> Result<AppConfig, prro::BootError> {
        let text = std::fs::read_to_string(config)?;
        AppConfig::from_toml(&text).map_err(|e| prro::BootError::ConfigParse(e.to_string()))
    })();
    let cfg = match result {
        Ok(cfg) => cfg,
        Err(boot_err) => {
            eprintln!("prro: {boot_err}");
            std::process::exit(boot_err.exit_code());
        }
    };
    match App::boot(cfg).await {
        Ok(app) => app,
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
            // W9.1 review LOW 2 + LOW 3 fix: single consolidated helper
            // reads config + parses + boots, with BootError → sysexits
            // mapping covering config-read / parse / DB-integrity paths
            // symmetrically.
            let _app = boot_from_path_or_exit(&config).await;
            tracing::info!("migrations applied");
            Ok(())
        }
        Cmd::Doctor { config } => prro::doctor::run(&config).await,
        Cmd::Serve { config } => {
            let app = boot_from_path_or_exit(&config).await;
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
        Cmd::Admin { cmd } => match cmd {
            AdminCmd::ResetStopMode {
                config,
                fiscal_number,
                reason,
            } => match prro::admin::run_reset_stop_mode(&config, &fiscal_number, &reason).await {
                Ok(outcome) => {
                    println!(
                        "ADMIN_STOP_MODE_RESET OK fiscal_number={} docs_reset_count={}",
                        outcome.fiscal_number, outcome.docs_reset_count
                    );
                    Ok(())
                }
                Err(err) => {
                    eprintln!("prro admin reset-stop-mode: {err}");
                    std::process::exit(err.exit_code());
                }
            },
            AdminCmd::AddOperator {
                config,
                inn,
                name,
                key_path,
                fiscal_number,
            } => match prro::admin::run_add_operator(
                &config,
                inn,
                name,
                key_path,
                fiscal_number.clone(),
            )
            .await
            {
                Ok(()) => {
                    println!("ADMIN_OPERATOR_REGISTERED OK fiscal_number={fiscal_number}");
                    Ok(())
                }
                Err(err) => {
                    eprintln!("prro admin add-operator: {err}");
                    std::process::exit(err.exit_code());
                }
            },

            // ─── W4-Z0 piece 8c — config management ───────────────
            AdminCmd::AddTaxGroup {
                config,
                fiscal_number,
                tx_num,
                letter,
                dtpr,
                txpr,
                txal,
            } => w4z0_dispatch(
                prro::admin_w4_z0::run_add_tax_group(
                    &config,
                    fiscal_number,
                    tx_num,
                    letter,
                    dtpr,
                    txpr,
                    txal,
                )
                .await,
                "add-tax-group",
            ),
            AdminCmd::UpdateTaxRate {
                config,
                fiscal_number,
                tx_num,
                dtpr,
                txpr,
                txal,
            } => w4z0_dispatch(
                prro::admin_w4_z0::run_update_tax_rate(
                    &config,
                    fiscal_number,
                    tx_num,
                    dtpr,
                    txpr,
                    txal,
                )
                .await,
                "update-tax-rate",
            ),
            AdminCmd::RemoveTaxGroup {
                config,
                fiscal_number,
                tx_num,
            } => w4z0_dispatch(
                prro::admin_w4_z0::run_remove_tax_group(&config, fiscal_number, tx_num).await,
                "remove-tax-group",
            ),
            AdminCmd::ListTaxGroups {
                config,
                fiscal_number,
            } => match prro::admin_w4_z0::run_list_tax_groups(&config, fiscal_number).await {
                Ok(rows) => {
                    for r in rows {
                        println!(
                            "tx_num={} letter={} dtpr={:.2} txpr={:.2} txal={} active={}",
                            r.tx_num, r.letter, r.dtpr, r.txpr, r.txal, r.is_active
                        );
                    }
                    Ok(())
                }
                Err(err) => {
                    eprintln!("prro admin list-tax-groups: {err}");
                    std::process::exit(err.exit_code());
                }
            },

            AdminCmd::AddPayment {
                config,
                fiscal_number,
                pay_index,
                name,
                cash,
            } => w4z0_dispatch(
                prro::admin_w4_z0::run_add_payment_method(
                    &config,
                    fiscal_number,
                    pay_index,
                    name,
                    cash,
                )
                .await,
                "add-payment",
            ),
            AdminCmd::UpdatePayment {
                config,
                fiscal_number,
                pay_index,
                name,
                cash,
            } => w4z0_dispatch(
                prro::admin_w4_z0::run_update_payment_method(
                    &config,
                    fiscal_number,
                    pay_index,
                    name,
                    cash,
                )
                .await,
                "update-payment",
            ),
            AdminCmd::RemovePayment {
                config,
                fiscal_number,
                pay_index,
            } => w4z0_dispatch(
                prro::admin_w4_z0::run_remove_payment_method(&config, fiscal_number, pay_index)
                    .await,
                "remove-payment",
            ),
            AdminCmd::ListPayments {
                config,
                fiscal_number,
            } => match prro::admin_w4_z0::run_list_payment_methods(&config, fiscal_number).await {
                Ok(rows) => {
                    for r in rows {
                        println!(
                            "pay_index={} name={} iscash={} active={}",
                            r.pay_index, r.name, r.iscash, r.is_active
                        );
                    }
                    Ok(())
                }
                Err(err) => {
                    eprintln!("prro admin list-payments: {err}");
                    std::process::exit(err.exit_code());
                }
            },

            AdminCmd::SetFlag {
                config,
                fiscal_number,
                name,
                value,
            } => w4z0_dispatch(
                prro::admin_w4_z0::run_set_flag(&config, fiscal_number, name, value).await,
                "set-flag",
            ),
            AdminCmd::SetNationalReceipt {
                config,
                fiscal_number,
                enabled,
            } => w4z0_dispatch(
                prro::admin_w4_z0::run_set_national_receipt(&config, fiscal_number, enabled).await,
                "set-national-receipt",
            ),
            AdminCmd::ListFlags {
                config,
                fiscal_number,
            } => match prro::admin_w4_z0::run_list_flags(&config, fiscal_number).await {
                Ok(rows) => {
                    for r in rows {
                        println!("flag={} value={}", r.flag_name, r.flag_value);
                    }
                    Ok(())
                }
                Err(err) => {
                    eprintln!("prro admin list-flags: {err}");
                    std::process::exit(err.exit_code());
                }
            },

            AdminCmd::AddDriverMapping {
                config,
                driver_id,
                driver_number,
                canonical,
                letter,
            } => w4z0_dispatch(
                prro::admin_w4_z0::run_add_driver_mapping(
                    &config,
                    driver_id,
                    driver_number,
                    canonical,
                    letter,
                )
                .await,
                "add-driver-mapping",
            ),
            AdminCmd::UpdateDriverMapping {
                config,
                driver_id,
                driver_number,
                canonical,
            } => w4z0_dispatch(
                prro::admin_w4_z0::run_update_driver_mapping(
                    &config,
                    driver_id,
                    driver_number,
                    canonical,
                )
                .await,
                "update-driver-mapping",
            ),
            AdminCmd::RemoveDriverMapping {
                config,
                driver_id,
                driver_number,
            } => w4z0_dispatch(
                prro::admin_w4_z0::run_remove_driver_mapping(&config, driver_id, driver_number)
                    .await,
                "remove-driver-mapping",
            ),
            AdminCmd::ListDriverMappings { config, driver_id } => {
                match prro::admin_w4_z0::run_list_driver_mappings(&config, driver_id).await {
                    Ok(rows) => {
                        for r in rows {
                            println!(
                                "driver_number={} canonical_tx_num={} letter={} active={}",
                                r.driver_number,
                                r.canonical_tx_num,
                                r.driver_letter.as_deref().unwrap_or("-"),
                                r.is_active
                            );
                        }
                        Ok(())
                    }
                    Err(err) => {
                        eprintln!("prro admin list-driver-mappings: {err}");
                        std::process::exit(err.exit_code());
                    }
                }
            }

            AdminCmd::SetOutgressProfile {
                config,
                fiscal_number,
                profile,
            } => w4z0_dispatch(
                prro::admin_w4_z0::run_set_outgress_profile(&config, fiscal_number, profile).await,
                "set-outgress-profile",
            ),
            AdminCmd::ShowOutgressProfile {
                config,
                fiscal_number,
            } => match prro::admin_w4_z0::run_show_outgress_profile(&config, fiscal_number).await {
                Ok(profile) => {
                    println!("profile={}", profile.as_db_str());
                    Ok(())
                }
                Err(err) => {
                    eprintln!("prro admin show-outgress-profile: {err}");
                    std::process::exit(err.exit_code());
                }
            },

            AdminCmd::BootstrapDefaults {
                config,
                fiscal_number,
            } => w4z0_dispatch(
                prro::admin_w4_z0::run_bootstrap_defaults(&config, fiscal_number).await,
                "bootstrap-defaults",
            ),
        },
    }
}

/// Common mutate-command CLI dispatch: prints "OK" on success or
/// formats the typed CfgAdminError + exits with `exit_code()`.
fn w4z0_dispatch(
    result: Result<(), prro::admin_w4_z0::CfgAdminError>,
    cmd_name: &str,
) -> anyhow::Result<()> {
    match result {
        Ok(()) => {
            println!("{cmd_name}: OK");
            Ok(())
        }
        Err(err) => {
            eprintln!("prro admin {cmd_name}: {err}");
            std::process::exit(err.exit_code());
        }
    }
}
