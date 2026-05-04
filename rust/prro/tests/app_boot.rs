use prro::{config::AppConfig, App};

fn cfg_toml(db_path: &str) -> String {
    format!(
        r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "{db_path}"

[admin_ui]
enabled = false
listen  = "127.0.0.1:8443"
"#
    )
}

#[test]
fn config_parses_full_toml_with_optional_keys_dir() {
    let with_keys = r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "/tmp/x.db"

[admin_ui]
enabled = true
listen  = "127.0.0.1:8443"
keys_dir = "/etc/prro/keys"
"#;
    let cfg = AppConfig::from_toml(with_keys).unwrap();
    assert_eq!(cfg.app_name, "prro");
    assert!(cfg.admin_ui.enabled);
    assert_eq!(
        cfg.admin_ui.keys_dir.as_deref(),
        Some(std::path::Path::new("/etc/prro/keys"))
    );

    let without_keys = r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "/tmp/x.db"

[admin_ui]
enabled = false
listen  = "127.0.0.1:0"
"#;
    let cfg = AppConfig::from_toml(without_keys).unwrap();
    assert!(cfg.admin_ui.keys_dir.is_none());
}

#[test]
fn config_rejects_missing_required_section() {
    let no_admin = r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "/tmp/x.db"
"#;
    AppConfig::from_toml(no_admin).expect_err("missing [admin_ui] must error");

    let no_db = r#"
app_name = "prro"
version  = "0.1.0"

[admin_ui]
enabled = false
listen  = "127.0.0.1:0"
"#;
    AppConfig::from_toml(no_db).expect_err("missing [database] must error");
}

#[tokio::test]
async fn boot_applies_migrations_and_returns_pool() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("a.db");
    let toml_text = cfg_toml(&db_path.display().to_string().replace('\\', "/"));
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    let app = App::boot(cfg).await.unwrap();

    // Migrations applied: schema is present and queryable.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_number_config")
        .fetch_one(app.db())
        .await
        .unwrap();
    assert_eq!(count, 0, "fresh DB must have empty fiscal_number_config");

    // All expected M1 tables are reachable.
    for table in [
        "fiscal_number_config",
        "shifts",
        "fiscal_documents",
        "ingress_inbox",
        "sidecar_operators",
        "operator_certs",
        "offline_sessions",
        "offline_codes",
        "node_state",
        "audit_log",
        "licenses",
    ] {
        let exists: Option<String> =
            sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' AND name=?")
                .bind(table)
                .fetch_optional(app.db())
                .await
                .unwrap();
        assert_eq!(exists.as_deref(), Some(table), "missing table {table}");
    }
}

#[tokio::test]
async fn boot_creates_parent_dir_for_db_path() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("does/not/exist/yet/a.db");
    let toml_text = cfg_toml(&nested.display().to_string().replace('\\', "/"));
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    let _app = App::boot(cfg).await.expect("boot must mkdir -p the parent");
    assert!(nested.parent().unwrap().exists());
    assert!(nested.exists());
}

#[tokio::test]
async fn boot_is_idempotent_across_two_calls() {
    // Re-booting on the same DB file (e.g. process restart) must succeed —
    // sqlx::migrate! is idempotent, and open_pool re-uses an existing file.
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("a.db");
    let toml_text = cfg_toml(&db_path.display().to_string().replace('\\', "/"));
    let cfg1 = AppConfig::from_toml(&toml_text).unwrap();
    let cfg2 = AppConfig::from_toml(&toml_text).unwrap();
    let app1 = App::boot(cfg1).await.unwrap();
    drop(app1);
    let _app2 = App::boot(cfg2)
        .await
        .expect("second boot on same DB must succeed");
}
