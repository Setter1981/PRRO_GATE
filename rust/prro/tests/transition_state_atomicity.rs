//! W0-2 §9.3 Phase B — post-fix CAS-vs-SELECT atomicity regression.
//!
//! ADR-M3-A4 / W0-2 §4.4 closes PRRO_GATE-k99 by requiring
//! `transition_state` to take `&mut WriteTxConn<'_>` (only obtainable
//! from inside a `with_immediate` BEGIN IMMEDIATE envelope).  The
//! disambiguation `SELECT` after a CAS-miss therefore runs on the
//! same connection inside the same transaction as the CAS `UPDATE`,
//! and SQLite's RESERVED lock guarantees no other writer can
//! interleave an INSERT/DELETE between them.
//!
//! Phase B (this test) is **deterministic** because the contention
//! resolves through SQLite's lock ordering: the second writer
//! observes RESERVED held, blocks until COMMIT, then acquires the
//! lock and sees the first writer's state.  No injected pause is
//! needed.  Phase A (the pre-fix proof of the race) is documented in
//! the W2 PR description rather than landed as a `#[ignore]` test
//! per W0-2 §9.3 guidance — there is nothing in the post-W2 codebase
//! that can reproduce the race deterministically without injected
//! sleeps, and committing test-only sleep instrumentation is out of
//! scope for the post-fix regression.

use prro::db::models::enums::FiscalMode;
use prro::db::models::{
    enums::{DocState, DocType},
    ids::{DocumentId, RequestId},
};
use prro::db::open_pool;
use prro::db::repositories::{
    fiscal_documents::{self as fd, NewDocument, TransitionOutcome},
    fiscal_number_config::{self as fn_repo, NewFnConfig},
};
use prro::db::tx::with_immediate;

async fn fresh_pool_with_fn() -> (sqlx::SqlitePool, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("atom.db");
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
    (pool, "4000000001".into())
}

fn sample_doc(fn_id: &str) -> NewDocument {
    NewDocument {
        document_id: DocumentId::new(),
        request_id: RequestId::new(),
        fiscal_number: fn_id.into(),
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
        // W4-Z2a piece 6b.1 — test fixture: None (test doesn't
        // exercise FK semantics).
        signing_config_snapshot_id: None,
    }
}

async fn drive_transition(
    pool: sqlx::SqlitePool,
    id: DocumentId,
    from: DocState,
    to: DocState,
) -> anyhow::Result<TransitionOutcome> {
    with_immediate(&pool, move |tx| {
        Box::pin(async move { Ok(fd::transition_state(tx, id, from, to).await?) })
    })
    .await
}

/// Two concurrent writers race for the same Prepared→Signed CAS.
/// Exactly one wins (Applied), the other observes Conflict — never
/// NotFound, because the disambiguation SELECT runs inside the same
/// BEGIN IMMEDIATE tx as the failed CAS, and the row is still there
/// when the SELECT runs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn transition_state_two_concurrent_cas_one_applied_one_conflict() {
    let (pool, fn_id) = fresh_pool_with_fn().await;
    let new = sample_doc(&fn_id);
    let id = new.document_id;
    fd::insert_prepared(&pool, &new).await.unwrap();

    let p1 = pool.clone();
    let p2 = pool.clone();

    let t1 = tokio::spawn(async move {
        drive_transition(p1, id, DocState::Prepared, DocState::Signed).await
    });
    let t2 = tokio::spawn(async move {
        drive_transition(p2, id, DocState::Prepared, DocState::Signed).await
    });

    let r1 = t1.await.unwrap().unwrap();
    let r2 = t2.await.unwrap().unwrap();
    let outcomes = [r1, r2];

    let applied = outcomes
        .iter()
        .filter(|o| matches!(o, TransitionOutcome::Applied))
        .count();
    let conflict = outcomes
        .iter()
        .filter(|o| matches!(o, TransitionOutcome::Conflict))
        .count();

    assert_eq!(
        applied, 1,
        "exactly one writer must win the CAS; got {outcomes:?}"
    );
    assert_eq!(
        conflict, 1,
        "the loser must observe Conflict (row exists but state diverged from Prepared); \
         got {outcomes:?}"
    );
    for o in &outcomes {
        // NotFound here would prove the CAS-vs-SELECT race that
        // ADR-M3-A4 closes by construction — the row is still in
        // fiscal_documents, so the disambiguation SELECT must see it.
        assert!(
            !matches!(o, TransitionOutcome::NotFound),
            "unexpected NotFound in atomic-CAS contention: {outcomes:?}"
        );
        // Prepared→Signed is whitelisted; Forbidden would indicate a
        // regression in the allowed_transition table.
        assert!(
            !matches!(o, TransitionOutcome::Forbidden),
            "unexpected Forbidden in atomic-CAS contention: {outcomes:?}"
        );
    }
}

/// Verifies the row is in Signed after both writers complete — the
/// winner's COMMIT persists, the loser's tx rolls back without
/// touching state.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn transition_state_post_contention_state_is_signed() {
    let (pool, fn_id) = fresh_pool_with_fn().await;
    let new = sample_doc(&fn_id);
    let id = new.document_id;
    fd::insert_prepared(&pool, &new).await.unwrap();

    let p1 = pool.clone();
    let p2 = pool.clone();

    let t1 = tokio::spawn(async move {
        drive_transition(p1, id, DocState::Prepared, DocState::Signed).await
    });
    let t2 = tokio::spawn(async move {
        drive_transition(p2, id, DocState::Prepared, DocState::Signed).await
    });
    t1.await.unwrap().unwrap();
    t2.await.unwrap().unwrap();

    let state: String =
        sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, "SIGNED");
}
