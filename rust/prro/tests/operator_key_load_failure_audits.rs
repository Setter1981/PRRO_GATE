//! W2 PR-B — MED-PR90-03 key-load failure audit semantics.
//!
//! Per `docs/superpowers/plans/2026-05-25-m4-ingress-plan.md` §3 W2
//! Acceptance MED-PR90-03:
//!
//!   "missing-resolver behavior at boot is fully audited + skipped,
//!    NEVER panics.  Tests operator_key_load_failure_audits.rs
//!    + handler_503_on_missing_operator.rs cover the chain."
//!
//! Three sub-cases:
//!
//!   1. `key_path` points to missing file → `OPERATOR_KEY_LOAD_FAILED`
//!      Critical audit з `reason="FileNotFound"`; FN absent from
//!      registry; boot continues.
//!   2. `key_pass_enc` decodes to wrong password → audit з
//!      `reason="WrongPassword"`; FN absent; boot continues.
//!   3. No `operators` row для configured FN-X → `OPERATOR_NOT_REGISTERED`
//!      Info audit (different event_type); FN absent; boot continues.
//!
//! Sub-case #3 is also covered in `bindings_registry_build.rs` —
//! duplicated here so the MED-PR90-03 spec link reads end-to-end.

mod common;

use async_trait::async_trait;
use prro::db::models::enums::{FiscalMode, Severity};
use prro::db::open_pool;
use prro::db::repositories::{audit_log, fiscal_number_config as fn_cfg, operators as ops_repo};
use prro::db::open_secure_pool;
use prro::runtime::bindings::{BindingsRegistry, KeyLoadFailure, OperatorKeyLoader};
use prro::runtime::coding::Coding;
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::channel::DpsChannel;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Loader that always returns `FileNotFound`.
struct FileNotFoundLoader;

#[async_trait]
impl OperatorKeyLoader for FileNotFoundLoader {
    async fn load(
        &self,
        key_path: &Path,
        _password: &[u8],
    ) -> Result<SigningContext, KeyLoadFailure> {
        Err(KeyLoadFailure::FileNotFound(key_path.to_path_buf()))
    }
}

/// Loader that always returns `WrongPassword`.
struct WrongPasswordLoader;

#[async_trait]
impl OperatorKeyLoader for WrongPasswordLoader {
    async fn load(
        &self,
        key_path: &Path,
        _password: &[u8],
    ) -> Result<SigningContext, KeyLoadFailure> {
        Err(KeyLoadFailure::WrongPassword(key_path.to_path_buf()))
    }
}

async fn fresh_main_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = open_pool(&dir.path().join("main.db")).await.expect("main");
    (dir, pool)
}

async fn fresh_secure_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = open_secure_pool(&dir.path().join("secure.db"))
        .await
        .expect("secure");
    (dir, pool)
}

fn fn_config(fn_id: &str) -> fn_cfg::NewFnConfig {
    fn_cfg::NewFnConfig {
        fiscal_number: fn_id.into(),
        tax_number: "12345678".into(),
        vat_payer_inn: None,
        fiscal_mode: FiscalMode::Test,
        org_name: None,
        point_name: None,
        org_address: None,
        tsp_enabled: false,
        offline_enabled: true,
        national_check_enabled: true,
        min_offline_codes: 0,
        max_offline_codes: 0,
    }
}

fn new_op(operator_id: &str, fn_id: &str, key_path: &str) -> ops_repo::NewOperator {
    ops_repo::NewOperator {
        operator_id: operator_id.into(),
        fiscal_number: fn_id.into(),
        name: "Cashier".into(),
        key_path: key_path.into(),
        key_pass_enc: Coding::encode(b"any-test-password").expect("encode"),
    }
}

fn dps() -> Arc<dyn DpsChannel> {
    Arc::new(common::StubDpsChannel::new(Ok(common::ack("any"))))
}

#[tokio::test]
async fn case_1_missing_key_file_emits_key_load_failed_audit() {
    let (_d_main, pool_main) = fresh_main_pool().await;
    let (_d_secure, pool_secure) = fresh_secure_pool().await;

    fn_cfg::insert(&pool_main, &fn_config("4000000001"))
        .await
        .unwrap();
    ops_repo::insert(
        &pool_secure,
        &new_op("OP-1", "4000000001", "/var/keys/does-not-exist.dat"),
    )
    .await
    .unwrap();

    let registry = BindingsRegistry::build_from_db(
        &pool_secure,
        &pool_main,
        dps(),
        &FileNotFoundLoader,
        &["4000000001".to_string()],
    )
    .await
    .expect("boot must NOT abort on key-load failure");

    assert!(registry.is_empty(), "FN absent from registry");

    let audits = audit_log::list_for_entity(&pool_main, "operator", "4000000001", 20)
        .await
        .unwrap();
    let load_failed = audits
        .iter()
        .find(|a| a.event_type == "OPERATOR_KEY_LOAD_FAILED")
        .expect("OPERATOR_KEY_LOAD_FAILED audit emitted");
    assert_eq!(load_failed.severity, Severity::Critical);
    let payload = load_failed.event_payload_json.as_deref().unwrap_or("");
    assert!(
        payload.contains(r#""reason":"FileNotFound""#),
        "payload must carry reason=FileNotFound, got: {payload}"
    );
}

#[tokio::test]
async fn case_2_wrong_password_emits_key_load_failed_audit() {
    let (_d_main, pool_main) = fresh_main_pool().await;
    let (_d_secure, pool_secure) = fresh_secure_pool().await;

    fn_cfg::insert(&pool_main, &fn_config("4000000001"))
        .await
        .unwrap();
    ops_repo::insert(
        &pool_secure,
        &new_op("OP-1", "4000000001", "/var/keys/k.dat"),
    )
    .await
    .unwrap();

    let registry = BindingsRegistry::build_from_db(
        &pool_secure,
        &pool_main,
        dps(),
        &WrongPasswordLoader,
        &["4000000001".to_string()],
    )
    .await
    .expect("boot must NOT abort");

    assert!(registry.is_empty());

    let audits = audit_log::list_for_entity(&pool_main, "operator", "4000000001", 20)
        .await
        .unwrap();
    let load_failed = audits
        .iter()
        .find(|a| a.event_type == "OPERATOR_KEY_LOAD_FAILED")
        .expect("OPERATOR_KEY_LOAD_FAILED audit emitted");
    assert_eq!(load_failed.severity, Severity::Critical);
    let payload = load_failed.event_payload_json.as_deref().unwrap_or("");
    assert!(
        payload.contains(r#""reason":"WrongPassword""#),
        "payload must carry reason=WrongPassword, got: {payload}"
    );
    let _ = PathBuf::from("dummy"); // suppress unused-import warning
}

#[tokio::test]
async fn case_3_no_operators_row_emits_not_registered_info_audit() {
    let (_d_main, pool_main) = fresh_main_pool().await;
    let (_d_secure, pool_secure) = fresh_secure_pool().await;

    fn_cfg::insert(&pool_main, &fn_config("4000000001"))
        .await
        .unwrap();
    // Intentionally NO operators rows inserted.

    let registry = BindingsRegistry::build_from_db(
        &pool_secure,
        &pool_main,
        dps(),
        &FileNotFoundLoader, // any loader; never called
        &["4000000001".to_string()],
    )
    .await
    .expect("boot continues");

    assert!(registry.is_empty());

    let audits = audit_log::list_for_entity(&pool_main, "operator", "4000000001", 20)
        .await
        .unwrap();
    let not_reg = audits
        .iter()
        .find(|a| a.event_type == "OPERATOR_NOT_REGISTERED")
        .expect("OPERATOR_NOT_REGISTERED audit emitted");
    assert_eq!(
        not_reg.severity,
        Severity::Info,
        "missing operators row is Info (no key material exposed)"
    );
    // NOT Critical — distinct from OPERATOR_KEY_LOAD_FAILED.
    assert!(
        audits
            .iter()
            .all(|a| a.event_type != "OPERATOR_KEY_LOAD_FAILED"),
        "loader never invoked when no operators row exists"
    );
}
