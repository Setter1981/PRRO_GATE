use prro::db::{
    models::enums::{FiscalMode, Protocol},
    open_pool,
    repositories::{
        fiscal_number_config as fn_repo,
        fiscal_number_config::NewFnConfig,
        ingress_inbox::{delete_new_by_request_id, insert, InboxInsertOutcome, NewInboxEntry},
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
        signed_by_cashier_id: None,
        driver_id: Some("drv-test".into()),
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
    let first = entry(&fn_id, "k1", [1u8; 32], r#"{"x":1}"#);
    let persisted_request_id = first.request_id;
    let _ = insert(&pool, &first).await.unwrap();

    // Same idem_key but different payload hash — must surface as Conflict
    // with both hashes AND the PERSISTED row's request identity so the
    // caller can build a config_drift response keyed to the durable row
    // (RS-2 piece-4b/5a) and audit both ids; never a silent replay.
    let second = entry(&fn_id, "k1", [2u8; 32], r#"{"x":2}"#);
    let submitted_request_id = second.request_id;
    let outcome = insert(&pool, &second).await.unwrap();
    match outcome {
        InboxInsertOutcome::Conflict {
            existing_request_id,
            existing_payload_hash,
            submitted_payload_hash,
        } => {
            assert_eq!(
                existing_request_id, persisted_request_id,
                "Conflict must report the PERSISTED (first) row's request_id"
            );
            assert_ne!(
                existing_request_id, submitted_request_id,
                "the persisted id must differ from the late submission's id"
            );
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

/// RS-2 A-H1 (migration 021) — the recovery identity (`driver_id`,
/// `signed_by_cashier_id`) round-trips through BOTH the Created insert and
/// the Replay read, so the persisted row is self-contained for the seam +
/// a crash-recovery reaper.  Replay returns the PERSISTED identity (not the
/// resubmission's).  A NULL cashier is preserved, and a legacy pre-021
/// row (columns absent at insert) reads back NULL — additive/backward-safe.
#[tokio::test]
async fn recovery_identity_round_trips_created_replay_and_legacy_null() {
    let (pool, fn_id) = fresh_with_fn().await;

    // Created carries both (driver + cashier).
    let mut e = entry(&fn_id, "k-id", [1u8; 32], r#"{"x":1}"#);
    e.driver_id = Some("DRV-9".into());
    e.signed_by_cashier_id = Some("K-7".into());
    let created = match insert(&pool, &e).await.unwrap() {
        InboxInsertOutcome::Created(r) => r,
        other => panic!("expected Created, got {other:?}"),
    };
    assert_eq!(created.driver_id.as_deref(), Some("DRV-9"));
    assert_eq!(created.signed_by_cashier_id.as_deref(), Some("K-7"));

    // Replay (same key+hash) returns the PERSISTED identity, ignoring the
    // resubmission's `entry()` defaults (driver "drv-test" / cashier None).
    let replay = match insert(&pool, &entry(&fn_id, "k-id", [1u8; 32], r#"{"x":1}"#))
        .await
        .unwrap()
    {
        InboxInsertOutcome::Replay(r) => r,
        other => panic!("expected Replay, got {other:?}"),
    };
    assert_eq!(
        replay.driver_id.as_deref(),
        Some("DRV-9"),
        "Replay must return the durable driver_id, not the resubmitter's"
    );
    assert_eq!(replay.signed_by_cashier_id.as_deref(), Some("K-7"));

    // NULL cashier preserved (a command with no cashier).
    let mut e2 = entry(&fn_id, "k-nocsh", [2u8; 32], r#"{"x":2}"#);
    e2.signed_by_cashier_id = None;
    e2.driver_id = Some("DRV-9".into());
    let created2 = match insert(&pool, &e2).await.unwrap() {
        InboxInsertOutcome::Created(r) => r,
        other => panic!("expected Created, got {other:?}"),
    };
    assert_eq!(created2.signed_by_cashier_id, None, "NULL cashier preserved");
    assert_eq!(created2.driver_id.as_deref(), Some("DRV-9"));

    // Legacy pre-021 row: raw INSERT omitting the new columns → both read
    // back NULL through the Replay probe (additive migration is read-safe).
    let legacy_rid = uuid::Uuid::now_v7().into_bytes();
    sqlx::query(
        "INSERT INTO ingress_inbox \
         (request_id, fiscal_number, protocol, operation_type, idempotency_key, \
          payload_json, payload_sha256_canonical) \
         VALUES (?, ?, 'REST', 'SELL', 'k-legacy', '{}', ?)",
    )
    .bind(&legacy_rid[..])
    .bind(&fn_id)
    .bind(&[3u8; 32][..])
    .execute(&pool)
    .await
    .unwrap();
    let legacy = match insert(&pool, &entry(&fn_id, "k-legacy", [3u8; 32], "{}"))
        .await
        .unwrap()
    {
        InboxInsertOutcome::Replay(r) => r,
        other => panic!("expected Replay of legacy row, got {other:?}"),
    };
    assert_eq!(legacy.driver_id, None, "legacy row reads NULL driver_id");
    assert_eq!(
        legacy.signed_by_cashier_id, None,
        "legacy row reads NULL cashier"
    );
}

/// RS-2 piece-5a — `delete_new_by_request_id` releases exactly the NEW
/// row (rows_affected pinned), and is idempotent once the row is gone.
#[tokio::test]
async fn delete_new_by_request_id_pins_rows_affected() {
    let (pool, fn_id) = fresh_with_fn().await;
    let e = entry(&fn_id, "k-del", [1u8; 32], r#"{"x":1}"#);
    let rid = e.request_id;
    assert!(matches!(
        insert(&pool, &e).await.unwrap(),
        InboxInsertOutcome::Created(_)
    ));

    let n = delete_new_by_request_id(&pool, &rid).await.unwrap();
    assert_eq!(n, 1, "a NEW row must be released exactly once");

    let n2 = delete_new_by_request_id(&pool, &rid).await.unwrap();
    assert_eq!(n2, 0, "second delete affects no rows (idempotent release)");
}

/// The `WHERE status = 'NEW'` guard means an ADVANCED (durable) row is
/// NEVER dropped — a future RS-3 error firing after the row leaves NEW
/// must not be able to delete it.
#[tokio::test]
async fn delete_new_by_request_id_never_drops_advanced_row() {
    let (pool, fn_id) = fresh_with_fn().await;
    let e = entry(&fn_id, "k-adv", [2u8; 32], r#"{"x":1}"#);
    let rid = e.request_id;
    insert(&pool, &e).await.unwrap();
    // RS-3 would advance the row out of NEW inside fiscalize.
    sqlx::query("UPDATE ingress_inbox SET status = 'PROCESSING' WHERE request_id = ?")
        .bind(&rid[..])
        .execute(&pool)
        .await
        .unwrap();

    let n = delete_new_by_request_id(&pool, &rid).await.unwrap();
    assert_eq!(n, 0, "guarded delete must never drop a PROCESSING row");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ingress_inbox WHERE request_id = ?")
        .bind(&rid[..])
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "the durable row must survive");
}
