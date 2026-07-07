//! `prro doctor` — preflight checks for config, DB, lock, listen address.
//!
//! Extended with an optional `--live` section (see `live` submodule):
//! FormTest-parity READ-ONLY diagnostics against the live DPS.  The live
//! section runs ONLY when `live_args` is `Some`; without it the function
//! is behaviorally identical to the original single-arg version
//! (regression-pinned in `tests/doctor_smoke.rs` and
//! `tests/doctor_live_verdicts.rs`).

pub mod live;
pub use live::LiveArgs;

use crate::config::AppConfig;
use std::path::Path;

pub async fn run(config_path: &Path, live_args: Option<LiveArgs>) -> anyhow::Result<()> {
    println!("== prro doctor ==");

    // 1. Config file readable + parses.
    let text = std::fs::read_to_string(config_path)?;
    let cfg = AppConfig::from_toml(&text)?;
    println!("[OK]  config:  {}", config_path.display());

    // 2. DB parent directory exists or can be created.
    if let Some(parent) = cfg.database.db_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
            println!("[OK]  db dir:  {}", parent.display());
        }
    }

    // 3. Singleton lock — must come BEFORE opening the DB pool.  Per spec
    //    §3.4.1 maintenance CLI refuses *before* any direct DB-touch, so a
    //    running `prro serve` is not racing us through migrations or other
    //    write paths just because we ran `doctor`.  Lock is held through
    //    the rest of the checks and released on function return.
    let lock = crate::runtime::singleton::acquire(&cfg.database.db_path)?;
    println!("[OK]  pid lock acquirable");

    // 4. DB pool opens (this also runs migrations idempotently — safe now
    //    that we hold the lock).
    let pool = crate::db::open_pool(&cfg.database.db_path).await?;
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await?;
    println!("[OK]  migrations applied: {n}");

    // 4b. W2 / HIGH-AUDIT-01 — secure pool preflight.  Mirrors the
    //     boot-time secure-DB open in `App::boot` step (4b).  Without
    //     this, `prro doctor` succeeds on a corrupted or missing
    //     `secure.db` and the operator only learns about the failure
    //     at the first `prro serve` invocation.
    if let Some(parent) = cfg.database.secure_db_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
            println!("[OK]  secure db dir:  {}", parent.display());
        }
    }
    let secure_pool = crate::db::open_secure_pool(&cfg.database.secure_db_path).await?;
    let secure_mig: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&secure_pool)
        .await?;
    let operators_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM operators")
        .fetch_one(&secure_pool)
        .await?;
    println!(
        "[OK]  secure migrations applied: {secure_mig}, operators registered: {operators_count}"
    );

    // 5. Admin UI listen address parses (whether or not enabled).
    let _: std::net::SocketAddr = cfg.admin_ui.listen.parse()?;
    println!("[OK]  admin_ui.listen: {}", cfg.admin_ui.listen);

    drop(secure_pool);
    println!("== ALL LOCAL CHECKS PASSED ==");

    // 6. Live DPS section — only when --live is supplied.
    if let Some(args) = live_args {
        live::run_live_from_env(&pool, &args).await?;
    }

    drop(pool);
    drop(lock);
    Ok(())
}
