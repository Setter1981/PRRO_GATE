use prro::db::models::enums::FiscalMode;
use prro::db::tx::with_immediate;
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
use std::collections::HashSet;

/// Test-only helper that drives `fd::transition_state` through the
/// production `with_immediate` envelope (per ADR-M3-A4 / W0-2 §4.4
/// — every transactional helper must run inside a BEGIN IMMEDIATE
/// transaction).  Tests use this rather than `WriteTxConn::new_for_test`
/// (which is module-private to `db::tx`) so they exercise the same
/// structural seal that production callers do.
async fn tx_transition_state(
    pool: &sqlx::SqlitePool,
    id: DocumentId,
    from: DocState,
    to: DocState,
) -> anyhow::Result<fd::TransitionOutcome> {
    with_immediate(pool, move |tx| {
        Box::pin(async move { Ok(fd::transition_state(tx, id, from, to).await?) })
    })
    .await
}

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
        signed_by_cashier_id: None,
        signing_config_snapshot_id: None,
    }
}

#[tokio::test]
async fn insert_then_transition_signed_returns_applied() {
    let (pool, fn_id) = fresh_with_fn().await;
    let new = sample_doc(&fn_id);
    let id = new.document_id;
    fd::insert_prepared(&pool, &new).await.unwrap();

    let outcome = tx_transition_state(&pool, id, DocState::Prepared, DocState::Signed)
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
    let outcome = tx_transition_state(&pool, id, DocState::Prepared, DocState::Ack)
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
    let outcome = tx_transition_state(&pool, id, DocState::Sent, DocState::Kvt1)
        .await
        .unwrap();
    assert_eq!(outcome, fd::TransitionOutcome::Conflict);
}

#[tokio::test]
async fn transition_returns_not_found_for_missing_doc() {
    let (pool, _fn_id) = fresh_with_fn().await;
    let phantom = DocumentId::new();
    let outcome = tx_transition_state(&pool, phantom, DocState::Prepared, DocState::Signed)
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
        tx_transition_state(&pool, id_a, DocState::Prepared, DocState::Signed)
            .await
            .unwrap(),
        fd::TransitionOutcome::Applied
    );

    let mut b = sample_doc(&fn_id);
    b.lnd = 2;
    let id_b = b.document_id;
    fd::insert_prepared(&pool, &b).await.unwrap();
    assert_eq!(
        tx_transition_state(&pool, id_b, DocState::Prepared, DocState::Rejected)
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

#[tokio::test]
async fn list_pending_full_state_inclusion_contract() {
    let (pool, fn_id) = fresh_with_fn().await;
    // 13 states per ADR-M3-A9 step 2 (Sending added).
    let all_states = [
        DocState::Prepared,
        DocState::Signed,
        DocState::Encrypted,
        DocState::Sending,
        DocState::Sent,
        DocState::Kvt1,
        DocState::Kvt2,
        DocState::Ack,
        DocState::OfflineLocalAck,
        DocState::Rejected,
        DocState::Cancelled,
        DocState::ErrorRetryable,
        DocState::RequiresManualReconciliation,
    ];

    for (i, &state) in all_states.iter().enumerate() {
        let mut doc = sample_doc(&fn_id);
        doc.lnd = (i as i64) + 1;
        let id = doc.document_id;
        fd::insert_prepared(&pool, &doc).await.unwrap();
        if state != DocState::Prepared {
            sqlx::query("UPDATE fiscal_documents SET state = ? WHERE document_id = ?")
                .bind(state)
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }
    }

    let pending = fd::list_pending_for_fn(&pool, &fn_id).await.unwrap();
    let pending_states: HashSet<DocState> = pending.iter().map(|r| r.state).collect();

    // 8 pending states per ADR-M3-A8.
    let expected_pending: HashSet<DocState> = [
        DocState::Prepared,
        DocState::Signed,
        DocState::Encrypted,
        DocState::Sending,
        DocState::Sent,
        DocState::Kvt1,
        DocState::Kvt2,
        DocState::ErrorRetryable,
    ]
    .into_iter()
    .collect();

    assert_eq!(
        pending_states, expected_pending,
        "pending set drift: returned {pending_states:?}, expected {expected_pending:?}"
    );

    for s in [
        DocState::Ack,
        DocState::Rejected,
        DocState::Cancelled,
        DocState::OfflineLocalAck,
        DocState::RequiresManualReconciliation,
    ] {
        assert!(
            !pending_states.contains(&s),
            "{s:?} must NOT be returned by list_pending_for_fn"
        );
    }
}

#[tokio::test]
async fn list_pending_orders_by_lnd_for_chain_safety() {
    let (pool, fn_id) = fresh_with_fn().await;
    // Insert in REVERSE lnd order; CURRENT_TIMESTAMP is second-granular and
    // these inserts may share the same created_at second, so created_at
    // alone is not sufficient — `lnd` must drive the order.
    for lnd in [3i64, 1, 2] {
        let mut doc = sample_doc(&fn_id);
        doc.lnd = lnd;
        fd::insert_prepared(&pool, &doc).await.unwrap();
    }
    let pending = fd::list_pending_for_fn(&pool, &fn_id).await.unwrap();
    let lnds: Vec<i64> = pending.iter().map(|r| r.lnd).collect();
    assert_eq!(
        lnds,
        vec![1, 2, 3],
        "list_pending_for_fn must order by lnd ascending, not just created_at"
    );
}

#[tokio::test]
async fn hashes_persist_at_32_bytes_through_insert_prepared() {
    let (pool, fn_id) = fresh_with_fn().await;
    let mut doc = sample_doc(&fn_id);
    doc.payload_sha256_canonical = [0xAAu8; 32];
    doc.unsigned_xml_sha256 = Some([0xBBu8; 32]);
    doc.previous_hash = Some([0xCCu8; 32]);
    let id = doc.document_id;
    fd::insert_prepared(&pool, &doc).await.unwrap();

    let row: (Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>) = sqlx::query_as(
        "SELECT payload_sha256_canonical, unsigned_xml_sha256, previous_hash \
         FROM fiscal_documents WHERE document_id = ?",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        row.0.len(),
        32,
        "payload_sha256_canonical must persist as 32 bytes"
    );
    assert_eq!(row.0, vec![0xAAu8; 32]);
    let unsigned = row.1.expect("unsigned_xml_sha256 must persist when set");
    assert_eq!(unsigned.len(), 32);
    assert_eq!(unsigned, vec![0xBBu8; 32]);
    let prev = row.2.expect("previous_hash must persist when set");
    assert_eq!(prev.len(), 32);
    assert_eq!(prev, vec![0xCCu8; 32]);
}

#[test]
fn allowed_transition_exhaustive_matrix() {
    use DocState::*;

    // 13 variants per ADR-M3-A9 step 2 (Sending added).
    let all = [
        Prepared,
        Signed,
        Encrypted,
        Sending,
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
        // W11 PR-2b addition: SENT last_chk-mismatch escalation
        // (§6.4-b operator-handoff edge).
        (Sent, RequiresManualReconciliation),
        (Kvt1, Kvt2),
        (Kvt1, ErrorRetryable),
        (Kvt2, Ack),
        (OfflineLocalAck, Sent),
        (ErrorRetryable, Sent),
        (ErrorRetryable, Kvt1),
        (ErrorRetryable, RequiresManualReconciliation),
        // 7 Pattern B additions per ADR-M3-A9 step 3.
        (Signed, Sending),
        (Encrypted, Sending),
        (Sending, Sent),
        (Sending, Kvt1),
        (Sending, ErrorRetryable),
        (Sending, Rejected),
        (ErrorRetryable, Sending),
        // 1 W10.5 addition: MAC recovery dispatch terminal — orchestrator
        // CounterExhausted / HashNotExtractable / second-`-12`-after-Resigned
        // overrides ERROR_RETRYABLE → REJECTED via
        // `override_to_rejected_with_failed_repeat_audit`
        // (services/write_path/stage_send.rs).
        (ErrorRetryable, Rejected),
        // 2 M3b W6 additions per spec §5.3 Pattern C / M3b plan §Task 6:
        //   - (OfflineLocalAck, Sending) — Pattern C drain entry on
        //     return-online (W7 will wire the runtime callsite).
        //   - (OfflineLocalAck, Cancelled) — manual operator escape
        //     during drain (operator decides to abandon the offline
        //     doc; alternative to flipping through the online ladder).
        // Locked-edge set + count for OfflineLocalAck-touching edges
        // separately pinned in
        // `tests/fiscal_documents_offline_local_ack_edges_locked.rs`.
        (OfflineLocalAck, Sending),
        (OfflineLocalAck, Cancelled),
        // M3b W9b §5.1 — lastChk replay short-circuit edge.  When
        // backlog_drain pre-flight on a doc with `server_fiscal_no IS
        // NOT NULL` confirms via lastChk (status OK + id match +
        // non-empty data_sign), drain skips wire send and advances
        // Kvt2 directly.  Final Kvt2 → Ack via existing M3a edge.
        // Drift-guard count above bumped 28 → 29.
        (OfflineLocalAck, Kvt2),
    ]
    .into_iter()
    .collect();

    // 13 * 13 = 169 pairs covered explicitly.
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
    assert_eq!(
        allowed_count, 29,
        "expected 17 base + 7 Pattern B + 1 MAC recovery dispatch terminal + 1 W11 PR-2b SENT→RM + 2 W6 Pattern C OfflineLocalAck edges + 1 W9b lastChk replay short-circuit edge = 29 allowed pairs"
    );
    assert_eq!(forbidden_count, 169 - 29);
}

/// Negative coverage of intentional whitelist gaps from ADR-M3-A8 / A9.
/// Without this, a future "looks-symmetric" expansion could re-introduce
/// silent fiscal hazards.
#[test]
fn intentional_whitelist_gaps_remain_forbidden() {
    use DocState::*;
    // Signed -> Rejected: DPS reject is only legal AFTER wire send;
    // direct Signed -> Rejected would skip the wire.
    assert!(!fd::allowed_transition(Signed, Rejected));
    // Prepared -> ErrorRetryable: nothing has been retried yet at
    // PREPARED; a failure here is a build/validation error, not a
    // retryable wire condition.
    assert!(!fd::allowed_transition(Prepared, ErrorRetryable));
    // Kvt2 -> ErrorRetryable: KVT2 is forward-only toward ACK.
    assert!(!fd::allowed_transition(Kvt2, ErrorRetryable));
    // Sending direct shortcuts that would defeat Pattern B safety.
    assert!(!fd::allowed_transition(Sending, Ack));
    assert!(
        !fd::allowed_transition(Sending, RequiresManualReconciliation),
        "operator escalation must go via ErrorRetryable"
    );
}
