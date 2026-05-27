//! W4-Z0 piece 7 — `runtime::bootstrap::bootstrap_fn_defaults` per-FN seeding.
//!
//! Per `docs/superpowers/specs/2026-05-26-w4-z0-config-storage-spec.md`
//! §2.  On first `add-operator` for a brand-new FN, seed 11 tax_groups
//! (WebCheck defaults), 4 payment_methods (WebCheck defaults),
//! `fn_outgress_profile = FSCO_ZZD`, and `useecheckmegovua = '0'`.
//!
//! Idempotency: bootstrap MUST be safe to call repeatedly.  Existing
//! rows (operator-customized via admin CLI) are NOT overwritten —
//! INSERT OR IGNORE pattern at the SQL layer.

use prro::db::open_secure_pool;
use prro::db::repositories::fn_outgress_profile::OutgressProfile;
use prro::db::repositories::{
    fn_integration_flags, fn_outgress_profile, payment_methods, tax_groups,
};
use prro::runtime::bootstrap::bootstrap_fn_defaults;
use sqlx::SqlitePool;

async fn fresh_secure_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = open_secure_pool(&dir.path().join("secure.db"))
        .await
        .expect("open_secure_pool");
    (dir, pool)
}

#[tokio::test]
async fn bootstrap_empty_fn_seeds_11_tax_groups() {
    let (_dir, pool) = fresh_secure_pool().await;

    bootstrap_fn_defaults(&pool, "4000000001")
        .await
        .expect("bootstrap");

    let rows = tax_groups::list_active_for_fn(&pool, "4000000001")
        .await
        .expect("list");
    assert_eq!(rows.len(), 11, "expected 11 WebCheck-default tax groups");

    // Spot-check the canonical mapping (per spec §2.1):
    //   tx_num=1, letter=А, txpr=20.0  (ПДВ 20% standard)
    //   tx_num=4, letter=ГА, dtpr=5.0, txpr=20.0, txal=2 (excise)
    //   tx_num=11, letter=К, txpr=14.0
    let a = rows.iter().find(|r| r.tx_num == 1).expect("tx_num=1");
    assert_eq!(a.letter, "А");
    assert_eq!(a.txpr, 20.0);
    assert_eq!(a.dtpr, 0.0);

    let ga = rows.iter().find(|r| r.tx_num == 4).expect("tx_num=4");
    assert_eq!(ga.letter, "ГА");
    assert_eq!(ga.dtpr, 5.0);
    assert_eq!(ga.txpr, 20.0);
    assert_eq!(ga.txal, 2);

    let k = rows.iter().find(|r| r.tx_num == 11).expect("tx_num=11");
    assert_eq!(k.letter, "К");
    assert_eq!(k.txpr, 14.0);
}

#[tokio::test]
async fn bootstrap_empty_fn_seeds_4_payment_methods() {
    let (_dir, pool) = fresh_secure_pool().await;

    bootstrap_fn_defaults(&pool, "4000000001")
        .await
        .expect("bootstrap");

    let rows = payment_methods::list_active_for_fn(&pool, "4000000001")
        .await
        .expect("list");
    assert_eq!(rows.len(), 4, "expected 4 WebCheck-default payment methods");

    // Per spec §2.2: 1=Готівка (cash), 2=Картка, 3=Кредит, 4=Сертифікат
    let names: Vec<String> = rows.iter().map(|r| r.name.clone()).collect();
    assert_eq!(rows.iter().find(|r| r.pay_index == 1).unwrap().name, "Готівка");
    assert!(rows.iter().find(|r| r.pay_index == 1).unwrap().iscash);
    assert_eq!(rows.iter().find(|r| r.pay_index == 2).unwrap().name, "Картка");
    assert_eq!(rows.iter().find(|r| r.pay_index == 3).unwrap().name, "Кредит");
    assert_eq!(rows.iter().find(|r| r.pay_index == 4).unwrap().name, "Сертифікат");
    for r in &rows {
        if r.pay_index != 1 {
            assert!(!r.iscash, "{} must be cashless", r.name);
        }
    }
    let _ = names;
}

#[tokio::test]
async fn bootstrap_empty_fn_sets_profile_to_fsco_zzd() {
    let (_dir, pool) = fresh_secure_pool().await;

    bootstrap_fn_defaults(&pool, "4000000001")
        .await
        .expect("bootstrap");

    let profile = fn_outgress_profile::get_profile(&pool, "4000000001")
        .await
        .expect("query")
        .expect("profile present");
    assert_eq!(profile, OutgressProfile::FscoZzd);
}

#[tokio::test]
async fn bootstrap_empty_fn_sets_useecheckmegovua_flag_off() {
    let (_dir, pool) = fresh_secure_pool().await;

    bootstrap_fn_defaults(&pool, "4000000001")
        .await
        .expect("bootstrap");

    let v = fn_integration_flags::get_flag(&pool, "4000000001", "useecheckmegovua")
        .await
        .expect("query")
        .expect("flag present");
    assert_eq!(v, "0", "Національний чек integration off by default");
}

#[tokio::test]
async fn bootstrap_is_idempotent() {
    let (_dir, pool) = fresh_secure_pool().await;

    bootstrap_fn_defaults(&pool, "4000000001").await.unwrap();
    bootstrap_fn_defaults(&pool, "4000000001")
        .await
        .expect("second bootstrap call must be no-op, not error");

    // Counts must remain at defaults — no duplicate rows
    let tax_rows = tax_groups::list_active_for_fn(&pool, "4000000001").await.unwrap();
    assert_eq!(tax_rows.len(), 11);
    let pay_rows = payment_methods::list_active_for_fn(&pool, "4000000001").await.unwrap();
    assert_eq!(pay_rows.len(), 4);
}

#[tokio::test]
async fn bootstrap_does_not_overwrite_operator_customised_tax_rate() {
    let (_dir, pool) = fresh_secure_pool().await;

    // Operator pre-customises tax_group А (tx_num=1) to txpr=18.0
    // (hypothetical wartime VAT rate change) before bootstrap fires.
    tax_groups::insert(
        &pool,
        &tax_groups::NewTaxGroup {
            fn_id: "4000000001".to_string(),
            tx_num: 1,
            letter: "А".to_string(),
            dtpr: 0.0,
            txpr: 18.0,
            txal: 0,
            txty: 0,
        },
    )
    .await
    .unwrap();

    bootstrap_fn_defaults(&pool, "4000000001")
        .await
        .expect("bootstrap must not error");

    // Operator's 18.0 stays — bootstrap respects pre-existing row
    let row = tax_groups::find(&pool, "4000000001", 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.txpr, 18.0, "operator customisation preserved");

    // Other defaults still seeded
    let rows = tax_groups::list_active_for_fn(&pool, "4000000001").await.unwrap();
    assert_eq!(rows.len(), 11);
}

/// Audit Round-1 (2026-05-27): plain INSERT OR IGNORE previously left
/// soft-deleted defaults stranded.  New ON CONFLICT DO UPDATE
/// reactivates them WITHOUT overwriting operator customisations on
/// active rows.
#[tokio::test]
async fn bootstrap_reactivates_soft_deleted_default() {
    let (_dir, pool) = fresh_secure_pool().await;

    bootstrap_fn_defaults(&pool, "4000000001").await.unwrap();
    // Operator soft-deletes the default tax_group А (tx_num=1)
    tax_groups::soft_delete(&pool, "4000000001", 1).await.unwrap();
    let active_count = tax_groups::list_active_for_fn(&pool, "4000000001").await.unwrap().len();
    assert_eq!(active_count, 10, "А soft-deleted, 10 remain active");

    // Re-running bootstrap reactivates the soft-deleted row.
    bootstrap_fn_defaults(&pool, "4000000001")
        .await
        .expect("recovery via re-bootstrap");

    let active = tax_groups::list_active_for_fn(&pool, "4000000001").await.unwrap();
    assert_eq!(active.len(), 11, "soft-deleted default reactivated");
    assert!(active.iter().any(|r| r.letter == "А"), "А present again");
}

/// The reactivation must NOT overwrite a customised rate on an
/// already-active row.  Pin the no-overwrite guarantee.
#[tokio::test]
async fn bootstrap_reactivation_does_not_overwrite_active_customised_rate() {
    let (_dir, pool) = fresh_secure_pool().await;

    bootstrap_fn_defaults(&pool, "4000000001").await.unwrap();
    // Operator customises А txpr 20→18 (hypothetical VAT change)
    tax_groups::update_rates(&pool, "4000000001", 1, 0.0, 18.0, 0, 0)
        .await
        .unwrap();

    bootstrap_fn_defaults(&pool, "4000000001")
        .await
        .expect("bootstrap re-run");

    let row = tax_groups::find(&pool, "4000000001", 1).await.unwrap().unwrap();
    assert_eq!(row.txpr, 18.0, "active customisation NOT overwritten by re-bootstrap");
}

#[tokio::test]
async fn bootstrap_does_not_overwrite_existing_profile() {
    let (_dir, pool) = fresh_secure_pool().await;

    // Operator pre-sets EVPZ_DPS (forward-looking, even though
    // dispatch is unimplemented in pilot).
    fn_outgress_profile::set_profile(&pool, "4000000001", OutgressProfile::EvpzDps)
        .await
        .unwrap();

    bootstrap_fn_defaults(&pool, "4000000001").await.unwrap();

    let profile = fn_outgress_profile::get_profile(&pool, "4000000001")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        profile,
        OutgressProfile::EvpzDps,
        "operator's explicit profile choice preserved"
    );
}

#[tokio::test]
async fn bootstrap_different_fns_independent() {
    let (_dir, pool) = fresh_secure_pool().await;

    bootstrap_fn_defaults(&pool, "4000000001").await.unwrap();
    bootstrap_fn_defaults(&pool, "4000000002").await.unwrap();

    let a = tax_groups::list_active_for_fn(&pool, "4000000001").await.unwrap();
    let b = tax_groups::list_active_for_fn(&pool, "4000000002").await.unwrap();
    assert_eq!(a.len(), 11);
    assert_eq!(b.len(), 11);
}

/// Integration test for the W4-Z0 piece 7 wiring into
/// `admin::add_operator`: registering a brand-new cashier on a
/// brand-new FN must seed the config defaults via bootstrap_fn_defaults.
#[tokio::test]
async fn add_operator_invokes_bootstrap_on_first_fn_registration() {
    use prro::admin::{add_operator, AddOperatorInput};
    use prro::db::models::enums::FiscalMode;
    use prro::db::open_pool;
    use prro::db::repositories::fiscal_number_config as fn_cfg;

    let main_dir = tempfile::tempdir().unwrap();
    let pool_main = open_pool(&main_dir.path().join("prro.db"))
        .await
        .expect("main pool");

    let (_secure_dir, pool_secure) = fresh_secure_pool().await;

    // Cross-DB FK pre-check requires the FN row to exist in
    // fiscal_number_config (main DB).
    fn_cfg::insert(
        &pool_main,
        &fn_cfg::NewFnConfig {
            fiscal_number: "4000000001".to_string(),
            tax_number: "TN-12345".to_string(),
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
    .expect("seed fiscal_number_config");

    // Pre-condition: secure DB has no config rows for this FN.
    assert_eq!(
        tax_groups::list_active_for_fn(&pool_secure, "4000000001").await.unwrap().len(),
        0
    );

    add_operator(
        &pool_main,
        &pool_secure,
        AddOperatorInput {
            operator_id: "OP-001".to_string(),
            name: "Test Cashier".to_string(),
            key_path: "/var/keys/c.dat".to_string(),
            fiscal_number: "4000000001".to_string(),
            password: b"hunter2".to_vec().into(),
        },
    )
    .await
    .expect("add_operator must succeed and bootstrap");

    // Post-condition: bootstrap fired — defaults seeded.
    let tg = tax_groups::list_active_for_fn(&pool_secure, "4000000001").await.unwrap();
    assert_eq!(tg.len(), 11, "11 tax_groups seeded by bootstrap");
    let pm = payment_methods::list_active_for_fn(&pool_secure, "4000000001").await.unwrap();
    assert_eq!(pm.len(), 4, "4 payment_methods seeded by bootstrap");
    let profile = fn_outgress_profile::get_profile(&pool_secure, "4000000001")
        .await
        .unwrap()
        .expect("profile seeded");
    assert_eq!(profile, OutgressProfile::FscoZzd);
    let flag = fn_integration_flags::get_flag(&pool_secure, "4000000001", "useecheckmegovua")
        .await
        .unwrap()
        .expect("flag seeded");
    assert_eq!(flag, "0");
}
