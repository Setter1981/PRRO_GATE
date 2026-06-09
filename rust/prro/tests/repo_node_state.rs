use prro::db::{
    models::enums::{FiscalMode, NodeMode, ShiftState},
    open_pool,
    repositories::{
        fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig, node_state as ns,
    },
};

async fn fresh_with_fn() -> (sqlx::SqlitePool, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    std::mem::forget(dir);
    let pool = open_pool(&path).await.unwrap();
    fn_repo::insert(
        &pool,
        &NewFnConfig {
            fiscal_number: "4000000001".into(),
            tax_number: "12345678".into(),
            vat_payer_inn: None,
            fiscal_mode: FiscalMode::Test,
            org_name: None,
            point_name: None,
            org_address: None,
            tsp_enabled: false,
            offline_enabled: true,
            national_check_enabled: false,
            min_offline_codes: 0,
            max_offline_codes: 0,
        },
    )
    .await
    .unwrap();
    (pool, "4000000001".to_string())
}

#[tokio::test]
async fn upsert_then_get() {
    let (pool, fn_id) = fresh_with_fn().await;
    ns::upsert_initial(&pool, &fn_id, NodeMode::Online, ShiftState::Closed, 1)
        .await
        .unwrap();
    let row = ns::get(&pool, &fn_id).await.unwrap().unwrap();
    assert_eq!(row.fiscal_number, fn_id);
    assert_eq!(row.mode, NodeMode::Online);
    assert_eq!(row.shift_state, ShiftState::Closed);
    assert_eq!(row.next_lnd, 1);
    assert!(row.last_known_unsigned_xml_sha256.is_none());
}

#[tokio::test]
async fn upsert_initial_preserves_chain_state_on_conflict() {
    // Boot the FN, seed a prevhash, advance next_lnd via direct UPDATE,
    // then re-call upsert_initial with a different mode/shift_state.
    // The cheap fields (mode, shift_state) MUST refresh; the chain-critical
    // fields (next_lnd, last_known_unsigned_xml_sha256) MUST NOT be clobbered.
    let (pool, fn_id) = fresh_with_fn().await;
    ns::upsert_initial(&pool, &fn_id, NodeMode::Online, ShiftState::Closed, 1)
        .await
        .unwrap();
    let h = [0xABu8; 32];
    ns::seed_prevhash(&pool, &fn_id, &h).await.unwrap();
    sqlx::query("UPDATE node_state SET next_lnd = 42 WHERE fiscal_number = ?")
        .bind(&fn_id)
        .execute(&pool)
        .await
        .unwrap();

    // Re-bootstrap with different mode + shift_state + nominally next_lnd=1.
    ns::upsert_initial(&pool, &fn_id, NodeMode::Offline, ShiftState::Opened, 1)
        .await
        .unwrap();

    let row = ns::get(&pool, &fn_id).await.unwrap().unwrap();
    assert_eq!(row.mode, NodeMode::Offline, "mode must refresh");
    assert_eq!(
        row.shift_state,
        ShiftState::Opened,
        "shift_state must refresh"
    );
    assert_eq!(
        row.next_lnd, 42,
        "next_lnd must NOT be clobbered by upsert_initial — chain monotonicity"
    );
    assert_eq!(
        row.last_known_unsigned_xml_sha256,
        Some(h),
        "last_known_unsigned_xml_sha256 must NOT be cleared — chain prevhash"
    );
}

#[tokio::test]
async fn seed_prevhash_persists_at_32_bytes() {
    let (pool, fn_id) = fresh_with_fn().await;
    ns::upsert_initial(&pool, &fn_id, NodeMode::Online, ShiftState::Closed, 1)
        .await
        .unwrap();
    let h = [0xABu8; 32];
    assert!(ns::seed_prevhash(&pool, &fn_id, &h).await.unwrap());
    let row = ns::get(&pool, &fn_id).await.unwrap().unwrap();
    assert_eq!(row.last_known_unsigned_xml_sha256, Some(h));
    // Cross-check the raw BLOB length — schema CHECK enforces 32 bytes.
    let raw: Vec<u8> = sqlx::query_scalar(
        "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number = ?",
    )
    .bind(&fn_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(raw.len(), 32);
    assert_eq!(raw, vec![0xABu8; 32]);
}

#[tokio::test]
async fn seed_prevhash_unknown_fn_returns_false() {
    let (pool, _) = fresh_with_fn().await;
    let h = [0xCDu8; 32];
    assert!(
        !ns::seed_prevhash(&pool, "9999999999", &h).await.unwrap(),
        "seed_prevhash on missing FN must return false, not error"
    );
}
