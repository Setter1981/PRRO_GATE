use prro::db::{models::enums::FiscalMode, open_pool, repositories::fiscal_number_config as repo};

async fn fresh() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    std::mem::forget(dir);
    open_pool(&path).await.unwrap()
}

fn sample(fn_id: &str) -> repo::NewFnConfig {
    repo::NewFnConfig {
        fiscal_number: fn_id.to_string(),
        tax_number: "12345678".to_string(),
        vat_payer_inn: None,
        fiscal_mode: FiscalMode::Test,
        org_name: Some("ТОВ Демо".to_string()),
        point_name: Some("Точка #1".to_string()),
        org_address: Some("м. Київ".to_string()),
        tsp_enabled: false,
        offline_enabled: true,
        national_check_enabled: true,
        min_offline_codes: 0,
        max_offline_codes: 0,
    }
}

#[tokio::test]
async fn insert_and_get_roundtrip() {
    let pool = fresh().await;
    repo::insert(&pool, &sample("4000000001")).await.unwrap();
    let got = repo::get(&pool, "4000000001").await.unwrap().unwrap();
    assert_eq!(got.tax_number, "12345678");
    assert_eq!(got.fiscal_mode, FiscalMode::Test);
    assert_eq!(got.org_name.as_deref(), Some("ТОВ Демо"));
    assert!(got.offline_enabled);
    assert!(!got.tsp_enabled);
    assert!(
        got.national_check_enabled,
        "National Check flag must round-trip (drives <L> tag)"
    );
}

#[tokio::test]
async fn get_missing_returns_none() {
    let pool = fresh().await;
    let got = repo::get(&pool, "9999999999").await.unwrap();
    assert!(got.is_none());
}

#[tokio::test]
async fn list_returns_sorted() {
    let pool = fresh().await;
    repo::insert(&pool, &sample("4000000003")).await.unwrap();
    repo::insert(&pool, &sample("4000000001")).await.unwrap();
    repo::insert(&pool, &sample("4000000002")).await.unwrap();
    let all = repo::list_all(&pool).await.unwrap();
    let nums: Vec<&str> = all.iter().map(|r| r.fiscal_number.as_str()).collect();
    assert_eq!(nums, vec!["4000000001", "4000000002", "4000000003"]);
}

#[tokio::test]
async fn update_org_metadata_works() {
    let pool = fresh().await;
    repo::insert(&pool, &sample("4000000010")).await.unwrap();
    let n = repo::update_org_metadata(
        &pool,
        "4000000010",
        Some("New name"),
        Some("New point"),
        Some("New addr"),
    )
    .await
    .unwrap();
    assert_eq!(n, 1);
    let got = repo::get(&pool, "4000000010").await.unwrap().unwrap();
    assert_eq!(got.org_name.as_deref(), Some("New name"));
    assert_eq!(got.point_name.as_deref(), Some("New point"));
    assert_eq!(got.org_address.as_deref(), Some("New addr"));
}
