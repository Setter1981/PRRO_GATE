use prro::db::{models::enums::Severity, open_pool, repositories::audit_log as al};

async fn fresh() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    std::mem::forget(dir);
    open_pool(&path).await.unwrap()
}

#[tokio::test]
async fn append_returns_monotonic_audit_id_and_list_returns_desc() {
    let pool = fresh().await;
    let id1 = al::append(
        &pool,
        "fn",
        "4000000001",
        "fn_registered",
        Severity::Info,
        Some("admin_ui"),
        Some(r#"{"mode":"test"}"#),
    )
    .await
    .unwrap();
    let id2 = al::append(
        &pool,
        "fn",
        "4000000001",
        "fn_updated",
        Severity::Info,
        Some("admin_ui"),
        None,
    )
    .await
    .unwrap();
    let id3 = al::append(
        &pool,
        "fn",
        "4000000001",
        "shift_open_recovery",
        Severity::Warning,
        None,
        None,
    )
    .await
    .unwrap();
    assert!(
        id2 > id1,
        "audit_id must be monotonic on consecutive appends"
    );
    assert!(id3 > id2);

    let entries = al::list_for_entity(&pool, "fn", "4000000001", 10)
        .await
        .unwrap();
    assert_eq!(entries.len(), 3);
    // DESC by audit_id.
    assert_eq!(entries[0].audit_id, id3);
    assert_eq!(entries[1].audit_id, id2);
    assert_eq!(entries[2].audit_id, id1);
    assert_eq!(entries[0].event_type, "shift_open_recovery");
    assert_eq!(entries[0].severity, Severity::Warning);
    assert_eq!(
        entries[2].event_payload_json.as_deref(),
        Some(r#"{"mode":"test"}"#)
    );
    // LIMIT respected.
    let limited = al::list_for_entity(&pool, "fn", "4000000001", 2)
        .await
        .unwrap();
    assert_eq!(limited.len(), 2);
    assert_eq!(limited[0].audit_id, id3);
    assert_eq!(limited[1].audit_id, id2);
}

#[tokio::test]
async fn list_for_entity_isolates_by_entity_type_and_id() {
    let pool = fresh().await;
    al::append(&pool, "fn", "4000000001", "x", Severity::Info, None, None)
        .await
        .unwrap();
    al::append(
        &pool,
        "shift",
        "4000000001",
        "x",
        Severity::Info,
        None,
        None,
    )
    .await
    .unwrap();
    al::append(&pool, "fn", "4000000002", "x", Severity::Info, None, None)
        .await
        .unwrap();

    // Different entity_type -> no overlap.
    let shifts_for_fn = al::list_for_entity(&pool, "shift", "4000000001", 10)
        .await
        .unwrap();
    assert_eq!(shifts_for_fn.len(), 1);
    assert_eq!(shifts_for_fn[0].entity_type, "shift");

    // Different entity_id -> no overlap.
    let fn_a = al::list_for_entity(&pool, "fn", "4000000001", 10)
        .await
        .unwrap();
    let fn_b = al::list_for_entity(&pool, "fn", "4000000002", 10)
        .await
        .unwrap();
    assert_eq!(fn_a.len(), 1);
    assert_eq!(fn_b.len(), 1);
    assert_ne!(fn_a[0].entity_id, fn_b[0].entity_id);

    // Empty result for unmatched entity.
    let empty = al::list_for_entity(&pool, "operator", "4000000001", 10)
        .await
        .unwrap();
    assert!(empty.is_empty());
}

#[tokio::test]
async fn append_outlives_entity_no_fk_check() {
    // audit_log has no FK to fiscal_number_config — log entries can outlive
    // the entities they reference.  Append for an FN that does not exist in
    // fiscal_number_config and verify the row is created.
    let pool = fresh().await;
    let id = al::append(
        &pool,
        "fn",
        "9999999999", // never registered
        "tombstone",
        Severity::Critical,
        Some("system"),
        None,
    )
    .await
    .unwrap();
    assert!(id > 0);

    let entries = al::list_for_entity(&pool, "fn", "9999999999", 10)
        .await
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].severity, Severity::Critical);
    assert_eq!(entries[0].actor.as_deref(), Some("system"));
}
