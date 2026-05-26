//! W2 PR-B piece 7 — `admin::add_operator` command-level tests.
//!
//! Plan §3 W2 "Admin CLI happy path" + "MED-PR90-04 / W2 add-operator".
//! Focus on the command logic; the password input plumbing (TTY
//! double-input vs non-TTY single-line) lives in
//! `add_operator_cli_password_input.rs` (LOW-PR90-01).
//!
//! Five contracts:
//!
//!   1. Happy path: FN exists in fiscal_number_config + non-empty
//!      args + password → row lands in operators with obfuscated
//!      `key_pass_enc` (NOT the plaintext); audit
//!      `ADMIN_OPERATOR_REGISTERED` emitted.
//!   2. FN missing from fiscal_number_config →
//!      `AdminError::FiscalNumberNotInConfig`; no operators row;
//!      no audit row emitted (rejection happens pre-INSERT).
//!   3. Duplicate active cashier for same FN →
//!      `AdminError::DuplicateActiveCashier`.
//!   4. Empty `--inn` / `--name` / `--key-path` / `--fn` →
//!      `AdminError::EmptyArgument`.
//!   5. Empty password → `AdminError::EmptyPassword`; no INSERT.

use prro::admin::{add_operator, AddOperatorInput, AdminError};
use prro::db::models::enums::{FiscalMode, Severity};
use prro::db::{open_pool, open_secure_pool, repositories::audit_log};
use prro::db::repositories::{fiscal_number_config as fn_cfg, operators as ops_repo};
use prro::runtime::coding::Coding;
use sqlx::SqlitePool;

async fn fresh_main_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = open_pool(&dir.path().join("main.db"))
        .await
        .expect("open_pool");
    (dir, pool)
}

async fn fresh_secure_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = open_secure_pool(&dir.path().join("secure.db"))
        .await
        .expect("open_secure_pool");
    (dir, pool)
}

async fn seed_fn(pool: &SqlitePool, fn_id: &str) {
    fn_cfg::insert(
        pool,
        &fn_cfg::NewFnConfig {
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
        },
    )
    .await
    .expect("seed FN config");
}

fn good_input(fn_id: &str, password: &[u8]) -> AddOperatorInput {
    AddOperatorInput {
        operator_id: "3456789012".into(),
        name: "Test Cashier".into(),
        key_path: "/var/keys/cashier.dat".into(),
        fiscal_number: fn_id.into(),
        password: zeroize::Zeroizing::new(password.to_vec()),
    }
}

#[tokio::test]
async fn happy_path_inserts_row_with_encoded_password_and_audit() {
    let (_dm, pool_main) = fresh_main_pool().await;
    let (_ds, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    let plaintext_pw = b"secret-cashier-pw";

    add_operator(
        &pool_main,
        &pool_secure,
        good_input("4000000001", plaintext_pw),
    )
    .await
    .expect("happy path must succeed");

    let row = ops_repo::find_by_fiscal_number(&pool_secure, "4000000001")
        .await
        .unwrap()
        .expect("operators row present");
    assert_eq!(row.operator_id, "3456789012");
    assert_eq!(row.name, "Test Cashier");
    // Stored BLOB is the OBFUSCATED password — NOT the plaintext.
    assert_ne!(
        row.key_pass_enc, plaintext_pw,
        "stored BLOB must be encoded, not plaintext"
    );
    // Decoded round-trips back to the plaintext.
    let decoded = Coding::decode(&row.key_pass_enc).unwrap();
    assert_eq!(decoded, plaintext_pw);

    // Audit: ADMIN_OPERATOR_REGISTERED Info, payload contains
    // operator_id + key_path but NOT the password.
    let audits = audit_log::list_for_entity(&pool_main, "operator", "4000000001", 10)
        .await
        .unwrap();
    let registered = audits
        .iter()
        .find(|a| a.event_type == "ADMIN_OPERATOR_REGISTERED")
        .expect("ADMIN_OPERATOR_REGISTERED emitted");
    assert_eq!(registered.severity, Severity::Info);
    let payload = registered.event_payload_json.as_deref().unwrap_or("");
    assert!(payload.contains("3456789012"));
    assert!(payload.contains("/var/keys/cashier.dat"));
    assert!(
        !payload.contains("secret-cashier-pw"),
        "audit MUST NOT carry plaintext password"
    );
}

#[tokio::test]
async fn fn_missing_in_config_rejected_pre_insert() {
    let (_dm, pool_main) = fresh_main_pool().await;
    let (_ds, pool_secure) = fresh_secure_pool().await;
    // Intentionally NOT seeding FN config.

    let err = add_operator(
        &pool_main,
        &pool_secure,
        good_input("9999999999", b"pw"),
    )
    .await
    .expect_err("missing FN must reject");
    match err {
        AdminError::FiscalNumberNotInConfig(fn_id) => {
            assert_eq!(fn_id, "9999999999");
        }
        other => panic!("expected FiscalNumberNotInConfig, got: {other:?}"),
    }
    // No row inserted.
    let row = ops_repo::find_by_fiscal_number(&pool_secure, "9999999999")
        .await
        .unwrap();
    assert!(row.is_none(), "rejection must be pre-INSERT");
}

#[tokio::test]
async fn duplicate_active_cashier_for_same_fn_rejected() {
    let (_dm, pool_main) = fresh_main_pool().await;
    let (_ds, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    add_operator(
        &pool_main,
        &pool_secure,
        good_input("4000000001", b"first"),
    )
    .await
    .expect("first add succeeds");

    let mut second = good_input("4000000001", b"second");
    second.operator_id = "9999999999".into();
    let err = add_operator(&pool_main, &pool_secure, second)
        .await
        .expect_err("second active add must reject");
    match err {
        AdminError::DuplicateActiveCashier(fn_id) => {
            assert_eq!(fn_id, "4000000001");
        }
        other => panic!("expected DuplicateActiveCashier, got: {other:?}"),
    }
}

#[tokio::test]
async fn empty_required_string_args_rejected() {
    let (_dm, pool_main) = fresh_main_pool().await;
    let (_ds, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    let cases: Vec<(&str, AddOperatorInput)> = vec![
        ("inn", AddOperatorInput {
            operator_id: " ".into(),
            ..good_input("4000000001", b"pw")
        }),
        ("name", AddOperatorInput {
            name: "".into(),
            ..good_input("4000000001", b"pw")
        }),
        ("key-path", AddOperatorInput {
            key_path: "  ".into(),
            ..good_input("4000000001", b"pw")
        }),
        ("fn", AddOperatorInput {
            fiscal_number: "".into(),
            ..good_input("4000000001", b"pw")
        }),
    ];
    for (expected_arg, input) in cases {
        let err = add_operator(&pool_main, &pool_secure, input)
            .await
            .expect_err("empty arg must reject");
        match err {
            AdminError::EmptyArgument(arg) => assert_eq!(arg, expected_arg),
            other => panic!("expected EmptyArgument({expected_arg}), got: {other:?}"),
        }
    }
}

#[tokio::test]
async fn empty_password_rejected() {
    let (_dm, pool_main) = fresh_main_pool().await;
    let (_ds, pool_secure) = fresh_secure_pool().await;
    seed_fn(&pool_main, "4000000001").await;

    let err = add_operator(
        &pool_main,
        &pool_secure,
        good_input("4000000001", b""),
    )
    .await
    .expect_err("empty password must reject");
    assert!(matches!(err, AdminError::EmptyPassword));

    // Whitespace-only password also rejected.
    let err2 = add_operator(
        &pool_main,
        &pool_secure,
        good_input("4000000001", b"   "),
    )
    .await
    .expect_err("whitespace-only password must reject");
    assert!(matches!(err2, AdminError::EmptyPassword));

    // No row landed.
    let row = ops_repo::find_by_fiscal_number(&pool_secure, "4000000001")
        .await
        .unwrap();
    assert!(row.is_none());
}
