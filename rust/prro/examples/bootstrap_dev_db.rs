//! One-off helper: create `var/prro.dev.db` and apply migrations so that
//! `sqlx::query!` macros can compile against the dev schema.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    std::fs::create_dir_all("var")?;
    let _pool = prro::db::open_pool(std::path::Path::new("var/prro.dev.db")).await?;
    println!("dev DB ready: var/prro.dev.db");
    Ok(())
}
