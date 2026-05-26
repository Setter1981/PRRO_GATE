//! W2 PR-B piece 6 — `BindingsRegistry::build_from_db` core contracts.
//!
//! Per `docs/superpowers/plans/2026-05-25-m4-ingress-plan.md` §3 W2:
//!   "Unit: BindingsRegistry::build_from_db з two operators rows
//!    (is_active=1) → registry has two FNs, single shared
//!    Arc<DpsChannel> instance, each entry has SigningContext whose
//!    underlying key file matches the row's key_path."
//!
//! Plus cross-DB FK runtime check (HIGH-PR90-01 enforcement at the
//! registry layer) and the OPERATOR_NOT_REGISTERED Info audit for
//! configured FNs without a matching operators row.
//!
//! Key-load failure subcases live in
//! `operator_key_load_failure_audits.rs` (MED-PR90-03).

mod common;

use async_trait::async_trait;
use prro::db::models::enums::{FiscalMode, Severity};
use prro::db::open_pool;
use prro::db::repositories::{audit_log, fiscal_number_config as fn_cfg, operators as ops_repo};
use prro::db::open_secure_pool;
use prro::runtime::bindings::{
    BindingsRegistry, KeyLoadFailure, OperatorKeyLoader,
};
use prro::runtime::coding::Coding;
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::channel::DpsChannel;
use sqlx::SqlitePool;
use std::path::Path;
use std::sync::Arc;

/// Always-succeeds loader — returns a deterministic test SigningContext
/// regardless of the key_path/password it receives.  Suitable for the
/// happy-path + cross-DB FK + missing-row tests that do not exercise
/// the key-loading failure branches.
struct AlwaysOkLoader;

#[async_trait]
impl OperatorKeyLoader for AlwaysOkLoader {
    async fn load(
        &self,
        _key_path: &Path,
        _password: &[u8],
    ) -> Result<SigningContext, KeyLoadFailure> {
        Ok(common::det_signing_ctx())
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

fn new_op(operator_id: &str, fn_id: &str, key_path: &str, password: &[u8]) -> ops_repo::NewOperator {
    ops_repo::NewOperator {
        operator_id: operator_id.into(),
        fiscal_number: fn_id.into(),
        name: "Cashier".into(),
        key_path: key_path.into(),
        key_pass_enc: Coding::encode(password).expect("encode test password"),
    }
}

fn dps() -> Arc<dyn DpsChannel> {
    Arc::new(common::StubDpsChannel::new(Ok(common::ack("test"))))
}

#[tokio::test]
async fn happy_single_operator_lands_in_registry() {
    let (_d_main, pool_main) = fresh_main_pool().await;
    let (_d_secure, pool_secure) = fresh_secure_pool().await;

    fn_cfg::insert(&pool_main, &fn_config("4000000001"))
        .await
        .expect("seed FN config");
    ops_repo::insert(
        &pool_secure,
        &new_op("OP-1", "4000000001", "/tmp/k1.dat", b"secret1"),
    )
    .await
    .expect("seed operator");

    let registry = BindingsRegistry::build_from_db(
        &pool_secure,
        &pool_main,
        dps(),
        &AlwaysOkLoader,
        &["4000000001".to_string()],
    )
    .await
    .expect("build_from_db");

    assert_eq!(registry.len(), 1);
    assert!(registry.get("4000000001").is_some());
    assert!(registry.get("9999999999").is_none());
}

#[tokio::test]
async fn two_operators_share_single_arc_dps_channel() {
    let (_d_main, pool_main) = fresh_main_pool().await;
    let (_d_secure, pool_secure) = fresh_secure_pool().await;

    for fn_id in ["4000000001", "4000000002"] {
        fn_cfg::insert(&pool_main, &fn_config(fn_id)).await.unwrap();
    }
    ops_repo::insert(
        &pool_secure,
        &new_op("OP-1", "4000000001", "/tmp/k1.dat", b"s1"),
    )
    .await
    .unwrap();
    ops_repo::insert(
        &pool_secure,
        &new_op("OP-2", "4000000002", "/tmp/k2.dat", b"s2"),
    )
    .await
    .unwrap();

    let channel = dps();
    let baseline_strong = Arc::strong_count(&channel);

    let registry = BindingsRegistry::build_from_db(
        &pool_secure,
        &pool_main,
        Arc::clone(&channel),
        &AlwaysOkLoader,
        &["4000000001".to_string(), "4000000002".to_string()],
    )
    .await
    .expect("build");

    assert_eq!(registry.len(), 2);
    // Two registry entries + the local `channel` Arc + the temporary
    // passed to build_from_db (consumed into the registry) → strong
    // count grew by at least 2.  Pointer-equality check via Arc::ptr_eq
    // confirms the same channel is shared.
    let b1 = registry.get("4000000001").unwrap();
    let b2 = registry.get("4000000002").unwrap();
    assert!(
        Arc::ptr_eq(&b1.dps, &b2.dps),
        "both entries must share the same Arc<DpsChannel>"
    );
    let after_strong = Arc::strong_count(&channel);
    assert!(
        after_strong > baseline_strong,
        "registry must hold at least one strong reference to the shared channel"
    );
}

#[tokio::test]
async fn orphan_operator_fn_skipped_and_audited() {
    let (_d_main, pool_main) = fresh_main_pool().await;
    let (_d_secure, pool_secure) = fresh_secure_pool().await;

    // Main has FN-A only.  Operators table has FN-A (valid) and FN-B (orphan).
    fn_cfg::insert(&pool_main, &fn_config("4000000001"))
        .await
        .unwrap();
    ops_repo::insert(
        &pool_secure,
        &new_op("OP-A", "4000000001", "/tmp/a.dat", b"sa"),
    )
    .await
    .unwrap();
    ops_repo::insert(
        &pool_secure,
        &new_op("OP-B", "4000000002", "/tmp/b.dat", b"sb"),
    )
    .await
    .unwrap();

    let registry = BindingsRegistry::build_from_db(
        &pool_secure,
        &pool_main,
        dps(),
        &AlwaysOkLoader,
        &["4000000001".to_string()],
    )
    .await
    .expect("build");

    assert_eq!(registry.len(), 1, "only FN-A makes it; FN-B is orphan");
    assert!(registry.get("4000000001").is_some());
    assert!(registry.get("4000000002").is_none());

    let audits = audit_log::list_for_entity(&pool_main, "operator", "4000000002", 10)
        .await
        .expect("query orphan audits");
    let orphan = audits
        .iter()
        .find(|a| a.event_type == "OPERATOR_ORPHAN_FN")
        .expect("OPERATOR_ORPHAN_FN audit emitted for FN-B");
    assert_eq!(orphan.severity, Severity::Critical);
    assert!(
        orphan.event_payload_json.as_deref().unwrap_or("").contains("OP-B"),
        "audit payload should include the orphan operator_id"
    );
}

#[tokio::test]
async fn configured_fn_without_operator_row_emits_not_registered_audit() {
    let (_d_main, pool_main) = fresh_main_pool().await;
    let (_d_secure, pool_secure) = fresh_secure_pool().await;

    // FN-X configured in main, but no operators row anywhere.
    fn_cfg::insert(&pool_main, &fn_config("4000000099"))
        .await
        .unwrap();

    let registry = BindingsRegistry::build_from_db(
        &pool_secure,
        &pool_main,
        dps(),
        &AlwaysOkLoader,
        &["4000000099".to_string()],
    )
    .await
    .expect("build");

    assert!(registry.is_empty(), "no operators row → empty registry");

    let audits = audit_log::list_for_entity(&pool_main, "operator", "4000000099", 10)
        .await
        .expect("audits");
    let not_reg = audits
        .iter()
        .find(|a| a.event_type == "OPERATOR_NOT_REGISTERED")
        .expect("OPERATOR_NOT_REGISTERED audit emitted");
    assert_eq!(
        not_reg.severity,
        Severity::Info,
        "missing-operator-row is Info, not Critical (no key, no leak)"
    );
}
