//! prro_admin — operator / FN / license management CLI.
//!
//! Usage: prro_admin --db <path> <subcommand>
//!
//! Subcommands:
//!   register-fn    — add / update fiscal_number_config row
//!   add-operator   — add sidecar_operators row
//!   list-operators — list operators for a fiscal number
//!   deactivate     — set active=0 for an operator row
//!   load-license   — install license from payload+signature files

use std::io::{self, BufRead};

use clap::{Parser, Subcommand};
use rusqlite::{params, Connection};

#[derive(Parser)]
#[command(name = "prro_admin", about = "Fiscal sidecar management CLI")]
struct Cli {
    #[arg(long, env = "PRRO_DB_PATH", default_value = "prro.db")]
    db: String,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Register a fiscal number in fiscal_number_config.
    RegisterFn {
        fiscal_number: String,
        tax_number:    String,
        #[arg(long, default_value = "test", value_parser = ["prod", "test"])]
        mode:            String,
        #[arg(long)]
        org_name:        Option<String>,
        #[arg(long)]
        org_address:     Option<String>,
        #[arg(long, default_value_t = false)]
        tsp_enabled:     bool,
        #[arg(long, default_value_t = true)]
        offline_enabled: bool,
    },

    /// Add an operator (JKS + password) for a fiscal number.
    ///
    /// Set PRRO_JKS_PASSWORD env var to avoid passing the password on the
    /// command line (which would be visible in ps/history/audit logs).
    /// If neither --jks-password nor the env var is set, the password is read
    /// from stdin.
    AddOperator {
        fiscal_number: String,
        operator_inn:  String,
        jks_path:      String,
        /// JKS password. Prefer PRRO_JKS_PASSWORD env var over --jks-password
        /// to avoid process-args exposure.
        #[arg(long, env = "PRRO_JKS_PASSWORD", hide_env_values = true)]
        jks_password: Option<String>,
        #[arg(long)]
        name: Option<String>,
    },

    /// List operators for a fiscal number (all fiscal numbers if omitted).
    ListOperators {
        fiscal_number: Option<String>,
    },

    /// Deactivate an operator by row id.
    Deactivate {
        operator_id: i64,
    },

    /// Install license payload + signature from files.
    /// Validates the signature before persisting.
    LoadLicense {
        /// File containing the base64-encoded license payload
        #[arg(long, value_name = "FILE")]
        payload_file: std::path::PathBuf,
        /// File containing the base64-encoded signature
        #[arg(long, value_name = "FILE")]
        signature_file: std::path::PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    let conn = Connection::open(&cli.db).unwrap_or_else(|e| {
        eprintln!("db open {:?}: {e}", cli.db);
        std::process::exit(1);
    });
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;",
    )
    .unwrap_or_else(|e| {
        eprintln!("pragma: {e}");
        std::process::exit(1);
    });

    let result = match cli.cmd {
        Cmd::RegisterFn {
            fiscal_number, tax_number, mode, org_name, org_address,
            tsp_enabled, offline_enabled,
        } => cmd_register_fn(
            &conn, &fiscal_number, &tax_number, &mode,
            org_name.as_deref(), org_address.as_deref(),
            tsp_enabled, offline_enabled,
        ),

        Cmd::AddOperator { fiscal_number, operator_inn, jks_path, jks_password, name } => {
            let pw = resolve_jks_password(jks_password, &operator_inn);
            cmd_add_operator(&conn, &fiscal_number, &operator_inn, &jks_path, &pw, name.as_deref())
        }

        Cmd::ListOperators { fiscal_number } => cmd_list_operators(&conn, fiscal_number.as_deref()),

        Cmd::Deactivate { operator_id } => cmd_deactivate(&conn, operator_id),

        Cmd::LoadLicense { payload_file, signature_file } => {
            cmd_load_license(&conn, &payload_file, &signature_file)
        }
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

/// Resolve JKS password: env var → CLI arg → stdin prompt.
fn resolve_jks_password(cli_value: Option<String>, operator_inn: &str) -> String {
    if let Some(pw) = cli_value {
        return pw;
    }
    // Read from stdin — avoids storing in shell history.
    eprint!("JKS password for operator {operator_inn}: ");
    let stdin = io::stdin();
    let mut line = String::new();
    let n = stdin
        .lock()
        .read_line(&mut line)
        .unwrap_or_else(|e| { eprintln!("stdin: {e}"); std::process::exit(1); });
    if n == 0 {
        eprintln!("error: empty password (stdin closed — set PRRO_JKS_PASSWORD env var)");
        std::process::exit(1);
    }
    line.trim_end_matches('\n').trim_end_matches('\r').to_string()
}

// ── register-fn ───────────────────────────────────────────────────────────────

fn cmd_register_fn(
    conn:            &Connection,
    fiscal_number:   &str,
    tax_number:      &str,
    mode:            &str,
    org_name:        Option<&str>,
    org_address:     Option<&str>,
    tsp_enabled:     bool,
    offline_enabled: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute(
        "INSERT INTO fiscal_number_config
             (fiscal_number, tax_number, fiscal_mode,
              tsp_enabled, offline_enabled, org_name, org_address)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(fiscal_number) DO UPDATE SET
             tax_number      = excluded.tax_number,
             fiscal_mode     = excluded.fiscal_mode,
             tsp_enabled     = excluded.tsp_enabled,
             offline_enabled = excluded.offline_enabled,
             org_name        = excluded.org_name,
             org_address     = excluded.org_address,
             updated_at      = CURRENT_TIMESTAMP",
        params![
            fiscal_number, tax_number, mode,
            tsp_enabled as i32, offline_enabled as i32,
            org_name, org_address
        ],
    )?;
    println!("registered fn: {fiscal_number} (mode={mode})");
    Ok(())
}

// ── add-operator ──────────────────────────────────────────────────────────────

fn cmd_add_operator(
    conn:          &Connection,
    fiscal_number: &str,
    operator_inn:  &str,
    jks_path:      &str,
    jks_password:  &str,
    name:          Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute(
        "INSERT INTO sidecar_operators
             (fiscal_number, operator_inn, jks_path, jks_password, operator_name)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![fiscal_number, operator_inn, jks_path, jks_password, name],
    )
    .map_err(|e| map_operator_insert_error(e, fiscal_number))?;
    let row_id = conn.last_insert_rowid();
    println!("added operator id={row_id} inn={operator_inn} fn={fiscal_number}");
    Ok(())
}

/// Provide actionable guidance for FK violation (fiscal_number not registered).
fn map_operator_insert_error(e: rusqlite::Error, fiscal_number: &str) -> Box<dyn std::error::Error> {
    if let rusqlite::Error::SqliteFailure(ref f, _) = e {
        if f.extended_code == 787 {
            // SQLITE_CONSTRAINT_FOREIGNKEY
            return format!(
                "fiscal_number {fiscal_number:?} is not registered — \
                 run 'register-fn {fiscal_number} <tax_number>' first"
            )
            .into();
        }
    }
    Box::new(e)
}

// ── list-operators ────────────────────────────────────────────────────────────

fn cmd_list_operators(
    conn:          &Connection,
    fiscal_number: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(fn_id) = fiscal_number {
        let mut s = conn.prepare(
            "SELECT id, fiscal_number, operator_inn, operator_name, jks_path, active
             FROM   sidecar_operators
             WHERE  fiscal_number = ?1
             ORDER BY id",
        )?;
        let rows: Vec<_> = s
            .query_map(params![fn_id], map_operator_row)?
            .collect::<Result<_, _>>()?;
        print_operator_rows(&rows);
    } else {
        let mut s = conn.prepare(
            "SELECT id, fiscal_number, operator_inn, operator_name, jks_path, active
             FROM   sidecar_operators
             ORDER BY fiscal_number, id",
        )?;
        let rows: Vec<_> = s
            .query_map([], map_operator_row)?
            .collect::<Result<_, _>>()?;
        print_operator_rows(&rows);
    }
    Ok(())
}

struct OperatorDisplay {
    id:            i64,
    fiscal_number: String,
    operator_inn:  String,
    operator_name: Option<String>,
    jks_path:      String,
    active:        bool,
}

fn map_operator_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<OperatorDisplay> {
    Ok(OperatorDisplay {
        id:            row.get(0)?,
        fiscal_number: row.get(1)?,
        operator_inn:  row.get(2)?,
        operator_name: row.get(3)?,
        jks_path:      row.get(4)?,
        active:        row.get::<_, i32>(5)? != 0,
    })
}

fn print_operator_rows(rows: &[OperatorDisplay]) {
    if rows.is_empty() {
        println!("(no operators found)");
        return;
    }
    println!(
        "{:<5} {:<14} {:<12} {:<20} {:<6} {}",
        "ID", "FiscalNum", "INN", "Name", "Active", "JKS (filename)"
    );
    println!("{}", "-".repeat(80));
    for r in rows {
        // Show only the filename to avoid leaking key storage layout.
        let jks_display = std::path::Path::new(&r.jks_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("…");
        println!(
            "{:<5} {:<14} {:<12} {:<20} {:<6} {}",
            r.id,
            r.fiscal_number,
            r.operator_inn,
            r.operator_name.as_deref().unwrap_or("-"),
            if r.active { "yes" } else { "no" },
            jks_display,
        );
    }
}

// ── deactivate ────────────────────────────────────────────────────────────────

fn cmd_deactivate(
    conn:        &Connection,
    operator_id: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    let changed = conn.execute(
        "UPDATE sidecar_operators SET active = 0, updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
        params![operator_id],
    )?;
    if changed == 0 {
        return Err(format!("operator id={operator_id} not found").into());
    }
    println!("deactivated operator id={operator_id}");
    Ok(())
}

// ── load-license ──────────────────────────────────────────────────────────────

fn cmd_load_license(
    conn:           &Connection,
    payload_file:   &std::path::Path,
    signature_file: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let payload_b64   = std::fs::read_to_string(payload_file)
        .map_err(|e| format!("read {:?}: {e}", payload_file))?.trim().to_string();
    let signature_b64 = std::fs::read_to_string(signature_file)
        .map_err(|e| format!("read {:?}: {e}", signature_file))?.trim().to_string();

    // Verify signature before persisting.
    let now = time::OffsetDateTime::now_utc();
    let state = prro_sidecar::license::verify_signature_only(&payload_b64, &signature_b64, now)
        .map_err(|e| format!("license parse error: {e}"))?;

    use prro_sidecar::license::LicenseState;
    match state {
        LicenseState::SignatureInvalid => {
            return Err("license signature verification FAILED — payload or signature corrupted".into());
        }
        LicenseState::Expired => eprintln!("warning: license is expired (installing anyway)"),
        LicenseState::Grace { days_left } => {
            eprintln!("warning: license in grace period ({days_left} days left)");
        }
        _ => {}
    }

    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    let payload_bytes = B64.decode(&payload_b64)?;
    let meta: serde_json::Value = serde_json::from_slice(&payload_bytes)?;
    let tin        = meta["tin"].as_str().unwrap_or("?");
    let tier       = meta["tier"].as_str().unwrap_or("?");
    let expires_at = meta["expires_at"].as_str().unwrap_or("?");
    let fn_list    = meta["fn_numbers"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", "))
        .unwrap_or_default();

    // Atomic swap: deactivate old + insert new in a single transaction.
    let tx = conn.unchecked_transaction()?;
    tx.execute("UPDATE licenses SET active = 0 WHERE active = 1", [])?;
    tx.execute(
        "INSERT INTO licenses
             (tin, fn_numbers_json, issued_at, expires_at, tier,
              org_name, demo_limits_json, payload_b64, signature_b64, active)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1)",
        params![
            tin,
            meta["fn_numbers"].to_string(),
            meta["issued_at"].as_str().unwrap_or(""),
            expires_at,
            tier,
            meta["org_name"].as_str(),
            meta.get("demo_limits").map(|v| v.to_string()),
            payload_b64,
            signature_b64,
        ],
    )?;
    tx.commit()?;

    println!("license installed:");
    println!("  TIN:        {tin}");
    println!("  tier:       {tier}");
    println!("  expires_at: {expires_at}");
    println!("  fn_numbers: {fn_list}");
    Ok(())
}
