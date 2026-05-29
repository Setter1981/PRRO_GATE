//! W4-Z0 piece 8 — admin CLI library function integration tests.
//!
//! Per spec §3 + W4-Z0 review criteria item 5.  16 commands in 5 families:
//! tax_groups / payment_methods / integration_flags / driver_tax_mapping /
//! fn_outgress_profile.  Each happy-path command verified end-to-end:
//! repo row landed + audit_log Info event emitted (or absent for
//! read-only commands).  Per-family error paths spot-checked.

use prro::admin_w4_z0::{self as cli, CfgAdminError};
use prro::db::models::enums::{FiscalMode, Severity};
use prro::db::repositories::audit_log;
use prro::db::repositories::fn_outgress_profile::OutgressProfile;
use prro::db::repositories::{
    fiscal_number_config as fn_cfg, fn_integration_flags, fn_outgress_profile,
};
use prro::db::{open_pool, open_secure_pool};
use sqlx::SqlitePool;

async fn fresh_main_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = open_pool(&dir.path().join("prro.db")).await.unwrap();
    (dir, pool)
}

async fn fresh_secure_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = open_secure_pool(&dir.path().join("secure.db"))
        .await
        .unwrap();
    (dir, pool)
}

async fn seed_fn(pool_main: &SqlitePool, fn_id: &str) {
    fn_cfg::insert(
        pool_main,
        &fn_cfg::NewFnConfig {
            fiscal_number: fn_id.to_string(),
            tax_number: "TN-test".to_string(),
            vat_payer_inn: None,
            fiscal_mode: FiscalMode::Test,
            org_name: None,
            point_name: None,
            org_address: None,
            tsp_enabled: false,
            offline_enabled: true,
            national_check_enabled: false,
            min_offline_codes: 50,
            max_offline_codes: 1000,
        },
    )
    .await
    .expect("seed fn_cfg");
}

async fn count_audit_events(pool_main: &SqlitePool, event_type: &str) -> i64 {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE event_type = ?")
        .bind(event_type)
        .fetch_one(pool_main)
        .await
        .unwrap();
    row.0
}

// ─── tax_groups family ────────────────────────────────────────────

#[tokio::test]
async fn add_tax_group_happy_path() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    cli::add_tax_group(&pool_main, &pool_secure, "4000000001", 1, "А", 0.0, 20.0, 0)
        .await
        .expect("add_tax_group");

    let rows = cli::list_tax_groups(&pool_secure, "4000000001")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].letter, "А");
    assert_eq!(
        count_audit_events(&pool_main, "ADMIN_TAX_GROUP_ADDED").await,
        1
    );
}

#[tokio::test]
async fn add_tax_group_rejects_unknown_fn() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;

    let err = cli::add_tax_group(&pool_main, &pool_secure, "9999999999", 1, "А", 0.0, 20.0, 0)
        .await
        .expect_err("must reject unknown FN");
    assert!(matches!(err, CfgAdminError::FiscalNumberNotInConfig(_)));
    assert_eq!(
        count_audit_events(&pool_main, "ADMIN_TAX_GROUP_ADDED").await,
        0
    );
}

#[tokio::test]
async fn update_tax_rate_partial_field() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    cli::add_tax_group(&pool_main, &pool_secure, "4000000001", 1, "А", 0.0, 20.0, 0)
        .await
        .unwrap();

    // Update only txpr; leave dtpr/txal untouched
    cli::update_tax_rate(
        &pool_main,
        &pool_secure,
        "4000000001",
        1,
        None,
        Some(18.0),
        None,
    )
    .await
    .expect("update");

    let row = cli::list_tax_groups(&pool_secure, "4000000001")
        .await
        .unwrap();
    assert_eq!(row[0].txpr, 18.0);
    assert_eq!(row[0].dtpr, 0.0);
    assert_eq!(
        count_audit_events(&pool_main, "ADMIN_TAX_GROUP_UPDATED").await,
        1
    );
}

#[tokio::test]
async fn remove_tax_group_soft_deletes() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    cli::add_tax_group(&pool_main, &pool_secure, "4000000001", 1, "А", 0.0, 20.0, 0)
        .await
        .unwrap();
    cli::remove_tax_group(&pool_main, &pool_secure, "4000000001", 1)
        .await
        .expect("remove");

    let rows = cli::list_tax_groups(&pool_secure, "4000000001")
        .await
        .unwrap();
    assert!(rows.is_empty(), "removed tax_group excluded from list");
    assert_eq!(
        count_audit_events(&pool_main, "ADMIN_TAX_GROUP_REMOVED").await,
        1
    );
}

// ─── payment_methods family ────────────────────────────────────────

// Audit Round-1 (2026-05-27) regression tests
// ───────────────────────────────────────────────

#[tokio::test]
async fn add_payment_method_rejects_pay_index_zero() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    let err = cli::add_payment_method(&pool_main, &pool_secure, "4000000001", 0, "Bogus", false)
        .await
        .expect_err("must reject pay_index=0");
    assert!(matches!(err, CfgAdminError::InvalidPayIndex(0)));
}

#[tokio::test]
async fn add_payment_method_rejects_pay_index_out_of_range() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    let err = cli::add_payment_method(&pool_main, &pool_secure, "4000000001", 100, "Bogus", false)
        .await
        .expect_err("must reject pay_index=100");
    assert!(matches!(err, CfgAdminError::InvalidPayIndex(100)));
}

#[tokio::test]
async fn bootstrap_defaults_recovery_command_seeds_missing() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    // Simulate "stranded FN" — FN exists in fn_cfg but secure.db has no
    // config rows for it (bootstrap failed during add-operator).
    let tax_rows_before = cli::list_tax_groups(&pool_secure, "4000000001")
        .await
        .unwrap();
    assert!(tax_rows_before.is_empty());

    cli::bootstrap_defaults(&pool_main, &pool_secure, "4000000001")
        .await
        .expect("recovery bootstrap");

    let tax_rows = cli::list_tax_groups(&pool_secure, "4000000001")
        .await
        .unwrap();
    assert_eq!(tax_rows.len(), 11);
    let pay_rows = cli::list_payment_methods(&pool_secure, "4000000001")
        .await
        .unwrap();
    assert_eq!(pay_rows.len(), 4);
    assert_eq!(
        count_audit_events(&pool_main, "ADMIN_FN_DEFAULTS_BOOTSTRAPPED").await,
        1
    );
}

#[tokio::test]
async fn bootstrap_defaults_is_idempotent_when_already_seeded() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    cli::bootstrap_defaults(&pool_main, &pool_secure, "4000000001")
        .await
        .unwrap();
    cli::bootstrap_defaults(&pool_main, &pool_secure, "4000000001")
        .await
        .expect("second invocation must be safe");

    let tax_rows = cli::list_tax_groups(&pool_secure, "4000000001")
        .await
        .unwrap();
    assert_eq!(tax_rows.len(), 11);
}

#[tokio::test]
async fn update_driver_mapping_changes_canonical_tx_num() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;

    cli::add_driver_mapping(&pool_main, &pool_secure, "eccelio", 5, 5, Some("ГА"))
        .await
        .unwrap();
    cli::update_driver_mapping(&pool_main, &pool_secure, "eccelio", 5, 4)
        .await
        .expect("update");

    let rows = cli::list_driver_mappings(&pool_secure, "eccelio")
        .await
        .unwrap();
    assert_eq!(rows[0].canonical_tx_num, 4);
    assert_eq!(
        count_audit_events(&pool_main, "ADMIN_DRIVER_MAPPING_UPDATED").await,
        1
    );
}

#[tokio::test]
async fn update_driver_mapping_on_missing_returns_not_found() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;

    let err = cli::update_driver_mapping(&pool_main, &pool_secure, "eccelio", 99, 4)
        .await
        .expect_err("must surface NotFound");
    assert!(matches!(err, CfgAdminError::DriverMappingNotFound(_, 99)));
}

#[tokio::test]
async fn audit_failure_does_not_block_mutation() {
    // Mutation succeeds on pool_secure; if pool_main's audit_log is
    // somehow broken (here: dropped before the call), the command
    // returns Ok and logs the audit failure via tracing.  The
    // config row IS visible.
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    // Drop the audit_log table to simulate audit DB transient breakage
    sqlx::query("DROP TABLE audit_log")
        .execute(&pool_main)
        .await
        .expect("drop audit_log");

    // Mutation must still succeed (best-effort audit)
    cli::add_tax_group(&pool_main, &pool_secure, "4000000001", 1, "А", 0.0, 20.0, 0)
        .await
        .expect("mutation succeeds even when audit cannot land");

    let rows = cli::list_tax_groups(&pool_secure, "4000000001")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "tax_group row landed despite audit failure");
}

// ───────────────────────────────────────────────
// Original W4-Z0 piece 8 tests follow
// ───────────────────────────────────────────────

#[tokio::test]
async fn add_payment_method_happy_path() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    cli::add_payment_method(&pool_main, &pool_secure, "4000000001", 5, "Visa", false)
        .await
        .expect("add");

    let rows = cli::list_payment_methods(&pool_secure, "4000000001")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Visa");
    assert_eq!(
        count_audit_events(&pool_main, "ADMIN_PAYMENT_METHOD_ADDED").await,
        1
    );
}

#[tokio::test]
async fn add_payment_method_rejects_duplicate_name() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    cli::add_payment_method(&pool_main, &pool_secure, "4000000001", 5, "Visa", false)
        .await
        .unwrap();

    let err = cli::add_payment_method(&pool_main, &pool_secure, "4000000001", 6, "Visa", false)
        .await
        .expect_err("must reject duplicate name");
    assert!(matches!(err, CfgAdminError::DuplicatePaymentName(..)));
}

#[tokio::test]
async fn update_payment_method_changes_iscash() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    cli::add_payment_method(&pool_main, &pool_secure, "4000000001", 5, "Visa", false)
        .await
        .unwrap();
    cli::update_payment_method(&pool_main, &pool_secure, "4000000001", 5, None, Some(true))
        .await
        .expect("update");

    let rows = cli::list_payment_methods(&pool_secure, "4000000001")
        .await
        .unwrap();
    assert!(rows[0].iscash);
    assert_eq!(
        count_audit_events(&pool_main, "ADMIN_PAYMENT_METHOD_UPDATED").await,
        1
    );
}

#[tokio::test]
async fn remove_payment_method_soft_deletes() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    cli::add_payment_method(&pool_main, &pool_secure, "4000000001", 5, "Visa", false)
        .await
        .unwrap();
    cli::remove_payment_method(&pool_main, &pool_secure, "4000000001", 5)
        .await
        .expect("remove");

    let rows = cli::list_payment_methods(&pool_secure, "4000000001")
        .await
        .unwrap();
    assert!(rows.is_empty());
    assert_eq!(
        count_audit_events(&pool_main, "ADMIN_PAYMENT_METHOD_REMOVED").await,
        1
    );
}

// ─── integration_flags family ──────────────────────────────────────

#[tokio::test]
async fn set_flag_happy_path() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    cli::set_flag(
        &pool_main,
        &pool_secure,
        "4000000001",
        "useecheckmegovua",
        "1",
    )
    .await
    .expect("set");

    let v = fn_integration_flags::get_flag(&pool_secure, "4000000001", "useecheckmegovua")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(v, "1");
    assert_eq!(count_audit_events(&pool_main, "ADMIN_FLAG_SET").await, 1);
}

#[tokio::test]
async fn set_national_receipt_alias_flips_useecheckmegovua() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    cli::set_national_receipt(&pool_main, &pool_secure, "4000000001", true)
        .await
        .expect("set on");
    let v = fn_integration_flags::get_flag(&pool_secure, "4000000001", "useecheckmegovua")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(v, "1");

    cli::set_national_receipt(&pool_main, &pool_secure, "4000000001", false)
        .await
        .expect("set off");
    let v = fn_integration_flags::get_flag(&pool_secure, "4000000001", "useecheckmegovua")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(v, "0");

    assert_eq!(count_audit_events(&pool_main, "ADMIN_FLAG_SET").await, 2);
}

#[tokio::test]
async fn list_flags_returns_all_set_flags() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    cli::set_flag(
        &pool_main,
        &pool_secure,
        "4000000001",
        "useecheckmegovua",
        "1",
    )
    .await
    .unwrap();
    cli::set_flag(
        &pool_main,
        &pool_secure,
        "4000000001",
        "some_other_flag",
        "value",
    )
    .await
    .unwrap();

    let flags = cli::list_flags(&pool_secure, "4000000001").await.unwrap();
    assert_eq!(flags.len(), 2);
}

// ─── driver_tax_mapping family ─────────────────────────────────────

/// Audit Round-4 (2026-05-27): admin driver-mapping CLI commands
/// must normalise `driver_id` via `DriverId::new` so a `--driver-id
/// ' maria304 '` (with whitespace) persists as `"maria304"` and
/// matches the supervisor-stamped value at W4-Z1 lookup time.
#[tokio::test]
async fn add_driver_mapping_normalizes_whitespace_in_driver_id() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;

    cli::add_driver_mapping(&pool_main, &pool_secure, "  maria304\n", 4, 4, Some("ГА"))
        .await
        .expect("trim succeeds");

    // Looked up via the normalised id — would miss if raw was stored.
    let rows = cli::list_driver_mappings(&pool_secure, "maria304")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].driver_id, "maria304");
    assert_eq!(rows[0].canonical_tx_num, 4);
}

#[tokio::test]
async fn driver_mapping_admin_rejects_whitespace_only_driver_id() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;

    let err = cli::add_driver_mapping(&pool_main, &pool_secure, "   ", 1, 1, None)
        .await
        .expect_err("whitespace-only driver-id must reject");
    assert!(matches!(err, CfgAdminError::EmptyArgument("driver-id")));
}

#[tokio::test]
async fn add_driver_mapping_happy_path() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;

    cli::add_driver_mapping(&pool_main, &pool_secure, "maria304", 4, 4, Some("ГА"))
        .await
        .expect("add");

    let rows = cli::list_driver_mappings(&pool_secure, "maria304")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].canonical_tx_num, 4);
    assert_eq!(rows[0].driver_letter.as_deref(), Some("ГА"));
    assert_eq!(
        count_audit_events(&pool_main, "ADMIN_DRIVER_MAPPING_ADDED").await,
        1
    );
}

#[tokio::test]
async fn remove_driver_mapping_soft_deletes() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;

    cli::add_driver_mapping(&pool_main, &pool_secure, "maria304", 4, 4, None)
        .await
        .unwrap();
    cli::remove_driver_mapping(&pool_main, &pool_secure, "maria304", 4)
        .await
        .expect("remove");

    let rows = cli::list_driver_mappings(&pool_secure, "maria304")
        .await
        .unwrap();
    assert!(rows.is_empty());
    assert_eq!(
        count_audit_events(&pool_main, "ADMIN_DRIVER_MAPPING_REMOVED").await,
        1
    );
}

// ─── fn_outgress_profile family ────────────────────────────────────

#[tokio::test]
async fn set_outgress_profile_happy_path_fsco() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    cli::set_outgress_profile(&pool_main, &pool_secure, "4000000001", "FSCO_ZZD")
        .await
        .expect("set FSCO_ZZD");

    let profile = cli::show_outgress_profile(&pool_secure, "4000000001")
        .await
        .unwrap();
    assert_eq!(profile, OutgressProfile::FscoZzd);
    assert_eq!(
        count_audit_events(&pool_main, "ADMIN_OUTGRESS_PROFILE_SET").await,
        1
    );
}

#[tokio::test]
async fn set_outgress_profile_accepts_evpz_dps_even_in_pilot() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    // Pilot accepts EVPZ_DPS at admin-CLI layer; runtime supervisor
    // refuses dispatch via OutgressError::ProfileNotImplemented.
    cli::set_outgress_profile(&pool_main, &pool_secure, "4000000001", "EVPZ_DPS")
        .await
        .expect("EVPZ_DPS accepted at admin layer");
    let profile = cli::show_outgress_profile(&pool_secure, "4000000001")
        .await
        .unwrap();
    assert_eq!(profile, OutgressProfile::EvpzDps);
}

#[tokio::test]
async fn set_outgress_profile_rejects_unknown_profile_string() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    let err = cli::set_outgress_profile(&pool_main, &pool_secure, "4000000001", "BOGUS")
        .await
        .expect_err("must reject");
    assert!(matches!(err, CfgAdminError::UnknownProfile(_)));
}

#[tokio::test]
async fn show_outgress_profile_missing_returns_not_found() {
    let (_sd, pool_secure) = fresh_secure_pool().await;
    let err = cli::show_outgress_profile(&pool_secure, "9999999999")
        .await
        .expect_err("must surface NotFound");
    assert!(matches!(err, CfgAdminError::OutgressProfileNotFound(_)));
}

// ─── audit-event severity discipline ──────────────────────────────

#[tokio::test]
async fn all_mutation_events_are_info_severity() {
    let (_md, pool_main) = fresh_main_pool().await;
    let (_sd, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    cli::add_tax_group(&pool_main, &pool_secure, "4000000001", 1, "А", 0.0, 20.0, 0)
        .await
        .unwrap();
    cli::add_payment_method(&pool_main, &pool_secure, "4000000001", 5, "Visa", false)
        .await
        .unwrap();
    cli::set_flag(
        &pool_main,
        &pool_secure,
        "4000000001",
        "useecheckmegovua",
        "1",
    )
    .await
    .unwrap();
    cli::set_outgress_profile(&pool_main, &pool_secure, "4000000001", "FSCO_ZZD")
        .await
        .unwrap();

    let events: Vec<(String, String)> =
        sqlx::query_as("SELECT event_type, severity FROM audit_log ORDER BY audit_id")
            .fetch_all(&pool_main)
            .await
            .unwrap();
    assert_eq!(events.len(), 4);
    for (event_type, sev) in &events {
        assert_eq!(
            sev,
            Severity::Info.as_str(),
            "{event_type} expected Info severity, got {sev}"
        );
    }
    let _ = audit_log::append;
}
