//! `prro doctor` — preflight checks for config, DB, lock, listen address.

use crate::config::AppConfig;
use std::path::Path;

pub async fn run(config_path: &Path) -> anyhow::Result<()> {
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

    // 5. Admin UI listen address parses (whether or not enabled).
    let _: std::net::SocketAddr = cfg.admin_ui.listen.parse()?;
    println!("[OK]  admin_ui.listen: {}", cfg.admin_ui.listen);

    drop(pool);
    drop(lock);
    println!("== ALL CHECKS PASSED ==");
    Ok(())
}
