//! W4-Z0 piece 9 / audit Round-2 — `AppConfig.listeners` schema.
//!
//! Per spec §4 — listener config sources `(driver_id, fn)` for the
//! ingress mapping helper.  Audit Round-2 (2026-05-27) flagged that
//! the W3 mapper accepted these params but AppConfig had no schema
//! to provide them.  This test pins the new ListenerCfg surface.

use prro::config::AppConfig;

const SAMPLE_CONFIG_WITH_LISTENERS: &str = r#"
app_name = "prro"
version = "0.1.0"

[database]
db_path = "var/prro.db"
secure_db_path = "var/secure.db"

[admin_ui]
enabled = false
listen = "127.0.0.1:8080"

[[listeners]]
type = "maria304_tcp"
port = 9099
driver_id = "maria304"
fn = "4538765845"

[[listeners]]
type = "maria304_tcp"
port = 9100
driver_id = "maria304"
fn = "4538765846"

[[listeners]]
type = "webcheck_xmlrpc"
port = 8081
driver_id = "webcheck"
fn = "1234567890"
"#;

const SAMPLE_CONFIG_NO_LISTENERS: &str = r#"
app_name = "prro"
version = "0.1.0"

[database]
db_path = "var/prro.db"
secure_db_path = "var/secure.db"

[admin_ui]
enabled = false
listen = "127.0.0.1:8080"
"#;

#[test]
fn config_parses_three_listeners_with_distinct_fn_per_port() {
    let cfg = AppConfig::from_toml(SAMPLE_CONFIG_WITH_LISTENERS)
        .expect("parse");
    assert_eq!(cfg.listeners.len(), 3);

    let l0 = &cfg.listeners[0];
    assert_eq!(l0.kind, "maria304_tcp");
    assert_eq!(l0.port, 9099);
    assert_eq!(l0.driver_id, "maria304");
    assert_eq!(l0.fiscal_number, "4538765845");

    let l1 = &cfg.listeners[1];
    assert_eq!(l1.driver_id, "maria304"); // same vendor
    assert_eq!(l1.fiscal_number, "4538765846"); // different FN
    assert_ne!(l0.port, l1.port);

    let l2 = &cfg.listeners[2];
    assert_eq!(l2.kind, "webcheck_xmlrpc");
    assert_eq!(l2.driver_id, "webcheck");
}

#[test]
fn config_without_listeners_section_defaults_to_empty_vec() {
    let cfg = AppConfig::from_toml(SAMPLE_CONFIG_NO_LISTENERS)
        .expect("back-compat parse: pre-W4-Z0 configs have no listeners section");
    assert!(cfg.listeners.is_empty());
}
