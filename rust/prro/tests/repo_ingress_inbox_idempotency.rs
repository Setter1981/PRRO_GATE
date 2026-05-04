use prro::db::{
    models::enums::{FiscalMode, Protocol},
    open_pool,
    repositories::{
        fiscal_number_config as fn_repo,
        fiscal_number_config::NewFnConfig,
        ingress_inbox::{insert, InboxInsertOutcome, NewInboxEntry},
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

fn entry(fn_id: &str, idem: &str, hash: [u8; 32], payload: &str) -> NewInboxEntry {
    NewInboxEntry {
        request_id: uuid::Uuid::now_v7().into_bytes(),
        fiscal_number: fn_id.to_string(),
        protocol: Protocol::Rest,
        operation_type: "SELL".into(),
        idempotency_key: idem.into(),
        payload_json: payload.into(),
        payload_sha256_canonical: hash,
        correlation_id: None,
    }
}

#[tokio::test]
async fn first_insert_creates() {
    let (pool, fn_id) = fresh_with_fn().await;
    let outcome = insert(&pool, &entry(&fn_id, "k1", [1u8; 32], r#"{"x":1}"#))
        .await
        .unwrap();
    match outcome {
        InboxInsertOutcome::Created(row) => {
            assert_eq!(row.fiscal_number, fn_id);
            assert_eq!(row.idempotency_key, "k1");
            assert_eq!(row.status, "NEW");
            assert_eq!(row.payload_sha256_canonical, [1u8; 32]);
            assert!(
                !row.received_at.is_empty(),
                "server-side received_at must be populated"
            );
        }
        other => panic!("expected Created, got {other:?}"),
    }
}

#[tokio::test]
async fn second_with_same_hash_replays() {
    let (pool, fn_id) = fresh_with_fn().await;
    let first = match insert(&pool, &entry(&fn_id, "k1", [1u8; 32], r#"{"x":1}"#))
        .await
        .unwrap()
    {
        InboxInsertOutcome::Created(r) => r,
        other => panic!("expected Created on first insert, got {other:?}"),
    };

    // Second insert: same idem_key + same hash = Replay.  request_id is
    // different (entry() generates a fresh uuid), but the returned InboxRow
    // must reflect the EXISTING (first) row's request_id and received_at.
    let outcome = insert(&pool, &entry(&fn_id, "k1", [1u8; 32], r#"{"x":1}"#))
        .await
        .unwrap();
    match outcome {
        InboxInsertOutcome::Replay(replay_row) => {
            assert_eq!(
                replay_row.request_id, first.request_id,
                "Replay must return the original row's request_id, not the resubmitter's"
            );
            assert_eq!(replay_row.payload_sha256_canonical, [1u8; 32]);
            assert_eq!(replay_row.received_at, first.received_at);
        }
        other => panic!("expected Replay, got {other:?}"),
    }
}

#[tokio::test]
async fn second_with_different_hash_conflicts() {
    let (pool, fn_id) = fresh_with_fn().await;
    let _ = insert(&pool, &entry(&fn_id, "k1", [1u8; 32], r#"{"x":1}"#))
        .await
        .unwrap();

    // Same idem_key but different payload hash — must surface as Conflict
    // with both hashes so the caller can audit; never a silent replay.
    let outcome = insert(&pool, &entry(&fn_id, "k1", [2u8; 32], r#"{"x":2}"#))
        .await
        .unwrap();
    match outcome {
        InboxInsertOutcome::Conflict {
            existing_payload_hash,
            submitted_payload_hash,
        } => {
            assert_eq!(existing_payload_hash, [1u8; 32]);
            assert_eq!(submitted_payload_hash, [2u8; 32]);
        }
        other => panic!("expected Conflict, got {other:?}"),
    }

    // Cross-check: only the original row is in the table.
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ingress_inbox WHERE fiscal_number = ?")
            .bind(&fn_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        count, 1,
        "Conflict must NOT insert a second row — exactly one row should remain"
    );
    let stored_hash: Vec<u8> = sqlx::query_scalar(
        "SELECT payload_sha256_canonical FROM ingress_inbox WHERE fiscal_number = ?",
    )
    .bind(&fn_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        stored_hash,
        vec![1u8; 32],
        "stored hash must be the FIRST submission's"
    );
}

#[tokio::test]
async fn different_idempotency_keys_create_separate_rows() {
    let (pool, fn_id) = fresh_with_fn().await;
    assert!(matches!(
        insert(&pool, &entry(&fn_id, "k-a", [1u8; 32], r#"{}"#))
            .await
            .unwrap(),
        InboxInsertOutcome::Created(_)
    ));
    assert!(matches!(
        insert(&pool, &entry(&fn_id, "k-b", [2u8; 32], r#"{}"#))
            .await
            .unwrap(),
        InboxInsertOutcome::Created(_)
    ));
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ingress_inbox WHERE fiscal_number = ?")
            .bind(&fn_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 2);
}
