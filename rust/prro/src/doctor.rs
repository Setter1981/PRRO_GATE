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
        std::fs::create_dir_all(parent)?;
        println!("[OK]  db dir:  {}", parent.display());
    }

    // 3. DB pool opens (this also runs migrations idempotently).
    let pool = crate::db::open_pool(&cfg.database.db_path).await?;
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await?;
    println!("[OK]  migrations applied: {n}");

    // 4. Singleton lock available (acquire then release immediately).
    let lock = crate::runtime::singleton::acquire(&cfg.database.db_path)?;
    drop(lock);
    println!("[OK]  pid lock acquirable");

    // 5. Admin UI listen address parses (whether or not enabled).
    let _: std::net::SocketAddr = cfg.admin_ui.listen.parse()?;
    println!("[OK]  admin_ui.listen: {}", cfg.admin_ui.listen);

    println!("== ALL CHECKS PASSED ==");
    Ok(())
}
