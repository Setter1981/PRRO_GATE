use prro::db::models::enums::FiscalMode;
use prro::db::{
    models::{
        enums::{DocState, DocType},
        ids::{DocumentId, RequestId},
    },
    open_pool,
    repositories::{
        fiscal_documents as fd, fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig,
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

fn sample_doc(fn_id: &str) -> fd::NewDocument {
    fd::NewDocument {
        document_id: DocumentId::new(),
        request_id: RequestId::new(),
        fiscal_number: fn_id.to_string(),
        shift_id: None,
        offline_session_id: None,
        lnd: 1,
        doc_type: DocType::Sell,
        backend_profile_id: "b".into(),
        transport_profile_id: "t".into(),
        fs_mode: "ONLINE",
        business_ts: "2026-04-22T12:00:00Z".into(),
        total_sum_kop: Some(15000),
        payload_json: r#"{"goods":[]}"#.into(),
        payload_sha256_canonical: [0u8; 32],
        unsigned_xml_sha256: None,
        previous_hash: None,
    }
}

#[tokio::test]
async fn insert_then_transition_signed_returns_applied() {
    let (pool, fn_id) = fresh_with_fn().await;
    let new = sample_doc(&fn_id);
    let id = new.document_id;
    fd::insert_prepared(&pool, &new).await.unwrap();

    let outcome = fd::transition_state(&pool, id, DocState::Prepared, DocState::Signed)
        .await
        .unwrap();
    assert_eq!(outcome, fd::TransitionOutcome::Applied);
}

#[tokio::test]
async fn forbidden_transition_returns_forbidden() {
    let (pool, fn_id) = fresh_with_fn().await;
    let new = sample_doc(&fn_id);
    let id = new.document_id;
    fd::insert_prepared(&pool, &new).await.unwrap();
    // PREPARED -> ACK is not a whitelisted transition.
    let outcome = fd::transition_state(&pool, id, DocState::Prepared, DocState::Ack)
        .await
        .unwrap();
    assert_eq!(outcome, fd::TransitionOutcome::Forbidden);
}

#[tokio::test]
async fn cas_returns_conflict_when_actual_state_diverged() {
    let (pool, fn_id) = fresh_with_fn().await;
    let new = sample_doc(&fn_id);
    let id = new.document_id;
    fd::insert_prepared(&pool, &new).await.unwrap();
    // (Sent, Kvt1) is allowed by the whitelist, but the row is still PREPARED.
    let outcome = fd::transition_state(&pool, id, DocState::Sent, DocState::Kvt1)
        .await
        .unwrap();
    assert_eq!(outcome, fd::TransitionOutcome::Conflict);
}

#[tokio::test]
async fn transition_returns_not_found_for_missing_doc() {
    let (pool, _fn_id) = fresh_with_fn().await;
    let phantom = DocumentId::new();
    let outcome = fd::transition_state(&pool, phantom, DocState::Prepared, DocState::Signed)
        .await
        .unwrap();
    assert_eq!(outcome, fd::TransitionOutcome::NotFound);
}

#[tokio::test]
async fn list_pending_excludes_final_states() {
    let (pool, fn_id) = fresh_with_fn().await;

    let a = sample_doc(&fn_id);
    let id_a = a.document_id;
    fd::insert_prepared(&pool, &a).await.unwrap();
    assert_eq!(
        fd::transition_state(&pool, id_a, DocState::Prepared, DocState::Signed)
            .await
            .unwrap(),
        fd::TransitionOutcome::Applied
    );

    let mut b = sample_doc(&fn_id);
    b.lnd = 2;
    let id_b = b.document_id;
    fd::insert_prepared(&pool, &b).await.unwrap();
    assert_eq!(
        fd::transition_state(&pool, id_b, DocState::Prepared, DocState::Rejected)
            .await
            .unwrap(),
        fd::TransitionOutcome::Applied
    );

    let pending = fd::list_pending_for_fn(&pool, &fn_id).await.unwrap();
    let ids: Vec<_> = pending.iter().map(|r| r.document_id).collect();
    assert_eq!(
        ids,
        vec![id_a],
        "Rejected (final) must be excluded; Signed (pending) included"
    );
}

#[test]
fn allowed_transition_exhaustive_matrix() {
    use std::collections::HashSet;
    use DocState::*;

    let all = [
        Prepared,
        Signed,
        Encrypted,
        Sent,
        Kvt1,
        Kvt2,
        Ack,
        OfflineLocalAck,
        Rejected,
        Cancelled,
        ErrorRetryable,
        RequiresManualReconciliation,
    ];

    let allowed: HashSet<(DocState, DocState)> = [
        (Prepared, Signed),
        (Prepared, Rejected),
        (Signed, Encrypted),
        (Signed, ErrorRetryable),
        (Signed, OfflineLocalAck),
        (Encrypted, Sent),
        (Encrypted, ErrorRetryable),
        (Sent, Kvt1),
        (Sent, ErrorRetryable),
        (Sent, Rejected),
        (Kvt1, Kvt2),
        (Kvt1, ErrorRetryable),
        (Kvt2, Ack),
        (OfflineLocalAck, Sent),
        (ErrorRetryable, Sent),
        (ErrorRetryable, Kvt1),
        (ErrorRetryable, RequiresManualReconciliation),
    ]
    .into_iter()
    .collect();

    // 12 * 12 = 144 pairs covered explicitly.
    let mut allowed_count = 0usize;
    let mut forbidden_count = 0usize;
    for &from in &all {
        for &to in &all {
            let expected = allowed.contains(&(from, to));
            let actual = fd::allowed_transition(from, to);
            assert_eq!(
                expected, actual,
                "({from:?} -> {to:?}) expected {expected}, got {actual}"
            );
            if expected {
                allowed_count += 1;
            } else {
                forbidden_count += 1;
            }
        }
    }
    assert_eq!(allowed_count, 17, "expected 17 allowed pairs in spec §5");
    assert_eq!(forbidden_count, 144 - 17);
}
