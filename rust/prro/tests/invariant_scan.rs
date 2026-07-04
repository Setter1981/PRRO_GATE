//! Invariant-scan tool acceptance (audit pass-2, item 1).
//!
//! Each fixture seeds ONE deliberate ledger breach and asserts the scan
//! reports exactly that violation class; the clean fixtures pin zero
//! false positives. The scan itself is the reusable post-condition gate
//! for the kill-point matrix / soak / chaos harnesses that follow.

mod common;

use prro::db::invariant_scan::{scan, Violation};
use prro::db::models::enums::{FiscalMode, NodeMode, ShiftState};
use prro::db::models::ids::{DocumentId, OfflineSessionId, RequestId, ShiftId};
use prro::db::open_pool;
use prro::db::repositories::{fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig};
use sqlx::SqlitePool;

const FN: &str = "4000000001";

async fn fresh_pool() -> SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("scan.db");
    std::mem::forget(dir);
    open_pool(&path).await.unwrap()
}

async fn seed_fn(pool: &SqlitePool) -> ShiftId {
    fn_repo::insert(
        pool,
        &NewFnConfig {
            fiscal_number: FN.into(),
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
    let shift_id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, serial, state, open_mode, \
            cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED', 'ONLINE', 0, 'cashier')",
    )
    .bind(shift_id)
    .bind(FN)
    .execute(pool)
    .await
    .unwrap();
    shift_id
}

async fn seed_node_state(pool: &SqlitePool, shift_id: ShiftId, seed: Option<[u8; 32]>) {
    sqlx::query(
        "INSERT INTO node_state \
         (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
          backend_profile_id, transport_profile_id, last_known_unsigned_xml_sha256) \
         VALUES (?, ?, ?, ?, 10, 'b', 't', ?)",
    )
    .bind(FN)
    .bind(NodeMode::Online)
    .bind(ShiftState::Opened)
    .bind(shift_id)
    .bind(seed.map(|s| s.to_vec()))
    .execute(pool)
    .await
    .unwrap();
}

/// One configurable fiscal-document row (+ optional KVT1_RAW evidence).
#[allow(clippy::too_many_arguments)]
async fn seed_doc(
    pool: &SqlitePool,
    shift_id: ShiftId,
    lnd: i64,
    state: &str,
    server_fiscal_no: Option<&str>,
    previous_hash: Option<[u8; 32]>,
    unsigned_sha: Option<[u8; 32]>,
    offline_fiscal_no: Option<i64>,
    with_kvt1_raw: bool,
) -> (DocumentId, [u8; 16]) {
    let doc_id = DocumentId::new();
    let request_id: [u8; 16] = *RequestId::new().as_bytes();
    sqlx::query(
        "INSERT INTO fiscal_documents( \
            document_id, request_id, fiscal_number, shift_id, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, server_fiscal_no, \
            previous_hash, unsigned_xml_sha256, offline_fiscal_no \
         ) VALUES (?, ?, ?, ?, ?, 'SELL', ?, 'b', 't', 'ONLINE', \
            '2026-06-11T00:00:00Z', '{}', ?, ?, ?, ?, ?)",
    )
    .bind(doc_id)
    .bind(&request_id[..])
    .bind(FN)
    .bind(shift_id)
    .bind(lnd)
    .bind(state)
    .bind(vec![0u8; 32])
    .bind(server_fiscal_no)
    .bind(previous_hash.map(|h| h.to_vec()))
    .bind(unsigned_sha.map(|h| h.to_vec()))
    .bind(offline_fiscal_no)
    .execute(pool)
    .await
    .unwrap();
    if with_kvt1_raw {
        sqlx::query(
            "INSERT INTO document_files(document_id, kind, content) VALUES (?, 'KVT1_RAW', ?)",
        )
        .bind(doc_id)
        .bind(vec![0xAAu8; 8])
        .execute(pool)
        .await
        .unwrap();
    }
    (doc_id, request_id)
}

fn h(b: u8) -> [u8; 32] {
    [b; 32]
}

// ─── Clean fixtures: zero false positives ───────────────────────────────

#[tokio::test]
async fn clean_empty_db_scans_clean() {
    let pool = fresh_pool().await;
    assert_eq!(scan(&pool).await.unwrap(), vec![]);
}

/// A consistent two-receipt ledger: genesis ACK (prev=NULL) → second ACK
/// chaining off it; node seed = last ACK's sha; both with DPS id + KVT1_RAW.
#[tokio::test]
async fn clean_consistent_ledger_scans_clean() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, Some(h(2))).await;
    seed_doc(
        &pool,
        shift,
        1,
        "ACK",
        Some("D-1"),
        None,
        Some(h(1)),
        None,
        true,
    )
    .await;
    seed_doc(
        &pool,
        shift,
        2,
        "ACK",
        Some("D-2"),
        Some(h(1)),
        Some(h(2)),
        None,
        true,
    )
    .await;
    assert_eq!(scan(&pool).await.unwrap(), vec![]);
}

// ─── Detection fixtures: one breach each ────────────────────────────────

/// Drift-guard: if a future migration LOSES the `ux_fd_fn_lnd` unique
/// index, the scan still catches double-issued lnd.
#[tokio::test]
async fn detects_duplicate_lnd() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, None).await;
    sqlx::query("DROP INDEX ux_fd_fn_lnd")
        .execute(&pool)
        .await
        .unwrap();
    seed_doc(
        &pool,
        shift,
        7,
        "SENT",
        Some("D-A"),
        None,
        Some(h(1)),
        None,
        false,
    )
    .await;
    seed_doc(
        &pool,
        shift,
        7,
        "SENT",
        Some("D-B"),
        None,
        Some(h(2)),
        None,
        false,
    )
    .await;
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter().any(|x| matches!(
            x,
            Violation::DuplicateLnd { fiscal_number, lnd: 7, count: 2 } if fiscal_number == FN
        )),
        "got: {v:#?}"
    );
}

#[tokio::test]
async fn detects_stuck_sending() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, None).await;
    seed_doc(
        &pool,
        shift,
        1,
        "SENDING",
        None,
        None,
        Some(h(1)),
        None,
        false,
    )
    .await;
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::StuckSending { .. })),
        "got: {v:#?}"
    );
}

/// Part 3 — durable ledger-only enforcement: a PRE-SEND non-terminal
/// {PREPARED, SIGNED, ENCRYPTED} doc resting at a quiescent boundary is a leaked
/// write-path artifact (the inline pass must flow it to SENDING / a terminal in
/// one pass; the Aborted seam closes the refusal path).  `scan()` is a
/// quiescent-only post-condition gate (no production / pre-recovery caller — see
/// the module docstring), so the check is unconditional, mirroring StuckSending.
#[tokio::test]
async fn detects_stuck_non_terminal_pre_send_doc() {
    for state in ["PREPARED", "SIGNED", "ENCRYPTED"] {
        let pool = fresh_pool().await;
        let shift = seed_fn(&pool).await;
        seed_node_state(&pool, shift, None).await; // no issued tip
                                                   // A resting pre-send doc: unsigned set, genesis prev, never issued.
        seed_doc(&pool, shift, 1, state, None, None, Some(h(1)), None, false).await;
        let v = scan(&pool).await.unwrap();
        assert!(
            v.iter().any(|x| matches!(
                x,
                Violation::StuckNonTerminalDoc { state: s, .. } if s == state
            )),
            "a resting {state} doc must be flagged StuckNonTerminalDoc; got: {v:#?}"
        );
    }
}

/// Part 3 NEGATIVE (no false positive): the legitimately-RESTING states
/// (`ERROR_RETRYABLE` retry-parked, `SENT` awaiting DPS) are NOT pre-send and
/// must NOT be flagged StuckNonTerminalDoc — only the {PREPARED,SIGNED,ENCRYPTED}
/// pipeline pre-send states are.
#[tokio::test]
async fn stuck_non_terminal_excludes_legit_resting_states() {
    for (state, sfn) in [("ERROR_RETRYABLE", None), ("SENT", Some("D-1"))] {
        let pool = fresh_pool().await;
        let shift = seed_fn(&pool).await;
        seed_node_state(&pool, shift, None).await;
        seed_doc(&pool, shift, 1, state, sfn, None, Some(h(1)), None, false).await;
        let v = scan(&pool).await.unwrap();
        assert!(
            !v.iter()
                .any(|x| matches!(x, Violation::StuckNonTerminalDoc { .. })),
            "a legitimately-resting {state} doc must NOT be flagged StuckNonTerminalDoc; got: {v:#?}"
        );
    }
}

#[tokio::test]
async fn detects_ack_without_server_fiscal_no() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, Some(h(1))).await;
    seed_doc(&pool, shift, 1, "ACK", None, None, Some(h(1)), None, true).await;
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::AckWithoutServerFiscalNo { .. })),
        "got: {v:#?}"
    );
}

#[tokio::test]
async fn detects_ack_without_kvt1_raw() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, Some(h(1))).await;
    seed_doc(
        &pool,
        shift,
        1,
        "ACK",
        Some("D-1"),
        None,
        Some(h(1)),
        None,
        false,
    )
    .await;
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::AckWithoutKvt1Raw { .. })),
        "got: {v:#?}"
    );
}

/// Second ACK's previous_hash points at garbage instead of the first
/// ACK's unsigned sha.
#[tokio::test]
async fn detects_chain_break() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, Some(h(2))).await;
    seed_doc(
        &pool,
        shift,
        1,
        "ACK",
        Some("D-1"),
        None,
        Some(h(1)),
        None,
        true,
    )
    .await;
    seed_doc(
        &pool,
        shift,
        2,
        "ACK",
        Some("D-2"),
        Some(h(9)),
        Some(h(2)),
        None,
        true,
    )
    .await;
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::ChainBreak { lnd: 2, .. })),
        "got: {v:#?}"
    );
}

/// Docs chain fine, but node_state's seed was left behind (or ahead).
#[tokio::test]
async fn detects_chain_seed_mismatch() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, Some(h(9))).await; // wrong seed
    seed_doc(
        &pool,
        shift,
        1,
        "ACK",
        Some("D-1"),
        None,
        Some(h(1)),
        None,
        true,
    )
    .await;
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::ChainSeedMismatch { .. })),
        "got: {v:#?}"
    );
}

/// MAC-walk fence (post-sign refusal orphan fix): a refused `SIGNED → Aborted`
/// doc carries `unsigned_xml_sha256` (set at sign) but is NON-issued
/// (`offline_fiscal_no IS NULL`, state ABORTED ∉ OFFLINE_ISSUED_STATES, never
/// ACK).  The chain walk MUST NOT (a) advance the seed over it nor (b) flag it
/// as a `ChainBreak`: under single-writer its `previous_hash` equals the tip it
/// was signed against (= the prior issued tip), and the issued-predicate leaves
/// `expected` unchanged so the node seed still equals the last ISSUED tip.
#[tokio::test]
async fn aborted_doc_does_not_trip_chain_walk() {
    // Case 1 — Aborted FOLLOWS an issued ACK: it chains to the ACK's tip but
    // must NOT advance the seed (the node seed stays at the ACK tip).
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, Some(h(1))).await; // last ISSUED tip = ACK's
    seed_doc(
        &pool,
        shift,
        1,
        "ACK",
        Some("D-1"),
        None,
        Some(h(1)),
        None,
        true,
    )
    .await;
    // Aborted: previous_hash = the ACK tip (signed against it), unsigned set,
    // offline_fiscal_no NULL (never issued).
    seed_doc(
        &pool,
        shift,
        2,
        "ABORTED",
        None,
        Some(h(1)),
        Some(h(2)),
        None,
        false,
    )
    .await;
    assert_eq!(
        scan(&pool).await.unwrap(),
        vec![],
        "an Aborted doc after an ACK must not trip ChainBreak / ChainSeedMismatch"
    );

    // Case 2 — Aborted is the GENESIS doc (the fuzzer's no-code-first-sell case):
    // no prior issued tip, node seed stays None.
    let pool2 = fresh_pool().await;
    let shift2 = seed_fn(&pool2).await;
    seed_node_state(&pool2, shift2, None).await; // no issued doc yet
    seed_doc(
        &pool2,
        shift2,
        1,
        "ABORTED",
        None,
        None,
        Some(h(1)),
        None,
        false,
    )
    .await;
    assert_eq!(
        scan(&pool2).await.unwrap(),
        vec![],
        "a genesis Aborted doc must not trip the chain walk (no false ChainBreak)"
    );
}

/// REJECTED inbox + accepted doc for the same request — the replay-lie
/// hazard (AUD-1) must be visible to the scan.
#[tokio::test]
async fn detects_rejected_inbox_with_accepted_doc() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, Some(h(1))).await;
    let (_, request_id) = seed_doc(
        &pool,
        shift,
        1,
        "ACK",
        Some("D-1"),
        None,
        Some(h(1)),
        None,
        true,
    )
    .await;
    sqlx::query(
        "INSERT INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical, status) \
         VALUES (?, ?, 'REST', 'SELL', 'k-1', '{}', ?, 'REJECTED')",
    )
    .bind(&request_id[..])
    .bind(FN)
    .bind(vec![0u8; 32])
    .execute(&pool)
    .await
    .unwrap();
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::RejectedInboxWithAcceptedDoc { .. })),
        "got: {v:#?}"
    );
}

/// ERROR inbox + accepted doc for the same request — the SAME replay-lie
/// hazard (AUD-1) as REJECTED: `runtime/ingress/replay.rs:153` short-circuits
/// `Failed` on `"REJECTED" | "ERROR"`, so an ERROR inbox row beside an
/// ACK ledger doc is the same lie about fiscal truth.  AUD-L8-2 widened
/// check-5 to that full terminal-fail set; the scan must not be blind to
/// the ERROR half.  Mirror of `detects_rejected_inbox_with_accepted_doc`.
#[tokio::test]
async fn detects_error_inbox_with_accepted_doc() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, Some(h(1))).await;
    let (_, request_id) = seed_doc(
        &pool,
        shift,
        1,
        "ACK",
        Some("D-1"),
        None,
        Some(h(1)),
        None,
        true,
    )
    .await;
    sqlx::query(
        "INSERT INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical, status) \
         VALUES (?, ?, 'REST', 'SELL', 'k-1', '{}', ?, 'ERROR')",
    )
    .bind(&request_id[..])
    .bind(FN)
    .bind(vec![0u8; 32])
    .execute(&pool)
    .await
    .unwrap();
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::RejectedInboxWithAcceptedDoc { .. })),
        "got: {v:#?}"
    );
}

/// AUD-L8-2 over-fire guard: an ERROR inbox row beside a NON-accepted
/// (`REJECTED`) ledger doc for the same request is the legitimate,
/// consistent terminal pair — both sides failed, no fiscalized receipt is
/// being short-circuited.  The widen to `('REJECTED','ERROR')` must NOT
/// turn this clean case into a false positive.
#[tokio::test]
async fn clean_error_inbox_with_non_accepted_doc() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, None).await;
    let (_, request_id) =
        seed_doc(&pool, shift, 1, "REJECTED", None, None, None, None, false).await;
    sqlx::query(
        "INSERT INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical, status) \
         VALUES (?, ?, 'REST', 'SELL', 'k-1', '{}', ?, 'ERROR')",
    )
    .bind(&request_id[..])
    .bind(FN)
    .bind(vec![0u8; 32])
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        scan(&pool).await.unwrap(),
        vec![],
        "ERROR inbox + non-accepted doc is the consistent terminal pair — no over-fire"
    );
}

#[tokio::test]
async fn detects_half_consumed_offline_code() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, None).await;
    sqlx::query(
        "INSERT INTO offline_codes(fiscal_number, code_lnd, consumed_at, consumed_by_document_id) \
         VALUES (?, 5, '2026-06-11T00:00:01Z', NULL)",
    )
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::OfflineCodeHalfConsumed { code_lnd: 5, .. })),
        "got: {v:#?}"
    );
}

/// A doc claims offline_fiscal_no=5 but no code row was consumed by it.
#[tokio::test]
async fn detects_unbacked_offline_fiscal_no() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, None).await;
    let session = OfflineSessionId::new();
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, 'OPEN', '2026-06-11T00:00:00Z')",
    )
    .bind(session)
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    seed_doc(
        &pool,
        shift,
        1,
        "OFFLINE_LOCAL_ACK",
        None,
        None,
        Some(h(1)),
        Some(5),
        false,
    )
    .await;
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::OfflineFiscalNoUnbacked { .. })),
        "got: {v:#?}"
    );
}

/// Two docs share one (fn, offline_fiscal_no) — an offline fiscal number
/// was issued twice (the post-power-cut double-consume scenario DUR-1
/// exists to prevent; the scan is its detector of last resort).
#[tokio::test]
async fn detects_duplicate_offline_fiscal_no() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, None).await;
    seed_doc(
        &pool,
        shift,
        1,
        "OFFLINE_LOCAL_ACK",
        None,
        None,
        Some(h(1)),
        Some(5),
        false,
    )
    .await;
    seed_doc(
        &pool,
        shift,
        2,
        "OFFLINE_LOCAL_ACK",
        None,
        None,
        Some(h(2)),
        Some(5),
        false,
    )
    .await;
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter().any(|x| matches!(
            x,
            Violation::DuplicateOfflineFiscalNo {
                offline_fiscal_no: 5,
                count: 2,
                ..
            }
        )),
        "got: {v:#?}"
    );
}

/// M2-01 review F1: an offline-origin doc parked in ERROR_RETRYABLE
/// mid-drain (RetryClass::TransientRetry re-drive) is still ISSUED — the
/// walk must advance `expected` through it, exactly as the drain cohort
/// keeps re-selecting it.  Pre-fix this fixture read as a false
/// ChainBreak on doc#2 plus a false ChainSeedMismatch at the tail.
#[tokio::test]
async fn clean_error_retryable_offline_doc_mid_drain_scans_clean() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, Some(h(2))).await;
    let session = OfflineSessionId::new();
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, 'OPEN', '2026-06-12T00:00:00Z')",
    )
    .bind(session)
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    let (doc_a, _) = seed_doc(
        &pool,
        shift,
        1,
        "ERROR_RETRYABLE",
        None,
        None,
        Some(h(1)),
        Some(1),
        false,
    )
    .await;
    let (doc_b, _) = seed_doc(
        &pool,
        shift,
        2,
        "OFFLINE_LOCAL_ACK",
        None,
        Some(h(1)),
        Some(h(2)),
        Some(2),
        false,
    )
    .await;
    for (code_lnd, doc) in [(1i64, doc_a), (2, doc_b)] {
        sqlx::query(
            "INSERT INTO offline_codes(fiscal_number, code_lnd, consumed_at, \
                consumed_by_document_id) \
             VALUES (?, ?, '2026-06-12T00:00:01Z', ?)",
        )
        .bind(FN)
        .bind(code_lnd)
        .bind(doc)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE fiscal_documents SET offline_session_id = ? WHERE document_id = ?")
            .bind(session)
            .bind(doc)
            .execute(&pool)
            .await
            .unwrap();
    }
    assert_eq!(scan(&pool).await.unwrap(), vec![]);
}

/// **M2-N2b (architect ruling, 2026-06-13)** — an offline-origin doc that
/// reached OFFLINE_LOCAL_ACK (advancing the local seed, M2-01) and was THEN
/// REJECTED at drain is STILL "issued": a successor chained off it
/// (`prev = hash(it)`) before the reject.  The MAC-walk issued-set must advance
/// `expected` over the REJECTED predecessor, else a FALSE `ChainBreak` fires at
/// the successor.  Pre-fix this fixture read as `ChainBreak{lnd:2}`.
///   - lnd1: offline-origin REJECTED, chain HEAD (prev NULL), unsigned=H1, ofn=1
///   - lnd2: offline-origin OFFLINE_LOCAL_ACK, prev=H1, unsigned=H2, ofn=2
///   - node seed = H2  →  scan CLEAN.
#[tokio::test]
async fn m2_n2b_rejected_offline_origin_predecessor_scans_clean() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, Some(h(2))).await;
    let session = OfflineSessionId::new();
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, 'OPEN', '2026-06-13T00:00:00Z')",
    )
    .bind(session)
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    // lnd1 — REJECTED at drain, but it reached OFFLINE_LOCAL_ACK earlier and
    // advanced the seed (chain HEAD: previous_hash NULL, unsigned = H1).
    let (doc_a, _) = seed_doc(
        &pool,
        shift,
        1,
        "REJECTED",
        None,
        None,
        Some(h(1)),
        Some(1),
        false,
    )
    .await;
    // lnd2 — the successor that chained off the now-rejected predecessor.
    let (doc_b, _) = seed_doc(
        &pool,
        shift,
        2,
        "OFFLINE_LOCAL_ACK",
        None,
        Some(h(1)),
        Some(h(2)),
        Some(2),
        false,
    )
    .await;
    for (code_lnd, doc) in [(1i64, doc_a), (2, doc_b)] {
        sqlx::query(
            "INSERT INTO offline_codes(fiscal_number, code_lnd, consumed_at, \
                consumed_by_document_id) \
             VALUES (?, ?, '2026-06-13T00:00:01Z', ?)",
        )
        .bind(FN)
        .bind(code_lnd)
        .bind(doc)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE fiscal_documents SET offline_session_id = ? WHERE document_id = ?")
            .bind(session)
            .bind(doc)
            .execute(&pool)
            .await
            .unwrap();
    }
    assert_eq!(
        scan(&pool).await.unwrap(),
        vec![],
        "REJECTED offline-origin predecessor must NOT trip a false ChainBreak at its successor"
    );
}

/// M2-05: an OFFLINE_LOCAL_ACK doc with NULL offline_session_id is
/// invisible to the session-scoped drain cohort (silent backlog leak) —
/// the scan must flag it.
#[tokio::test]
async fn detects_offline_origin_without_session() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, None).await;
    // seed_doc never sets offline_session_id (always NULL).  Offline-origin docs
    // (offline_fiscal_no Some) in a DRAIN-COHORT state → flagged (M2-05 + the
    // M2-X3 widening to SENT/KVT1/ERROR_RETRYABLE/KVT2).
    seed_doc(
        &pool,
        shift,
        1,
        "OFFLINE_LOCAL_ACK",
        None,
        None,
        Some(h(1)),
        Some(11),
        false,
    )
    .await;
    seed_doc(
        &pool,
        shift,
        2,
        "SENT",
        Some("SFN-2"),
        None,
        Some(h(2)),
        Some(12),
        false,
    )
    .await;
    // Clean — must NOT trip: (a) an online doc (offline_fiscal_no NULL),
    // (b) a fully-drained ACK (state outside the drain cohort).
    seed_doc(
        &pool,
        shift,
        3,
        "SENT",
        Some("SFN-3"),
        None,
        Some(h(3)),
        None,
        false,
    )
    .await;
    seed_doc(
        &pool,
        shift,
        4,
        "ACK",
        Some("SFN-4"),
        None,
        Some(h(4)),
        Some(14),
        true,
    )
    .await;
    let v = scan(&pool).await.unwrap();
    let flagged = v
        .iter()
        .filter(|x| matches!(x, Violation::OfflineOriginWithoutSession { .. }))
        .count();
    assert_eq!(
        flagged, 2,
        "offline-origin NULL-session cohort docs (OFFLINE_LOCAL_ACK + SENT) flagged; \
         online + drained-ACK excluded: {v:#?}"
    );
}

// ─── SW-5b: Mirror-1 (shifts.state ↔ node_state.shift_state) ──────────────────

/// RED→GREEN: `node_state.shift_state` != the active `shifts.state` for
/// `current_shift_id` is the m3b §5 load-bearing mirror desync (Mirror-1).
#[tokio::test]
async fn detects_shift_state_mirror_drift() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await; // shifts.state = OPENED
    seed_node_state(&pool, shift, None).await; // node_state.shift_state = OPENED, current_shift_id = shift
                                               // Desync Mirror-1: drive the shifts row to RMR while node_state stays OPENED.
    sqlx::query("UPDATE shifts SET state = 'REQUIRES_MANUAL_RECONCILIATION' WHERE shift_id = ?")
        .bind(shift)
        .execute(&pool)
        .await
        .unwrap();
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter().any(|x| matches!(
            x,
            Violation::ShiftStateMirrorDrift {
                fiscal_number,
                node_state_shift_state,
                shifts_state,
            } if fiscal_number == FN
                && node_state_shift_state == "OPENED"
                && shifts_state == "REQUIRES_MANUAL_RECONCILIATION"
        )),
        "expected ShiftStateMirrorDrift, got: {v:#?}"
    );
}

/// Benign (over-fire guard): a consistent mirror (node_state.shift_state ==
/// shifts.state) must NOT trip the check.
#[tokio::test]
async fn clean_consistent_shift_state_mirror() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await; // shifts.state = OPENED
    seed_node_state(&pool, shift, None).await; // node_state.shift_state = OPENED (consistent)
    let v = scan(&pool).await.unwrap();
    assert!(
        !v.iter()
            .any(|x| matches!(x, Violation::ShiftStateMirrorDrift { .. })),
        "consistent mirror must NOT flag ShiftStateMirrorDrift: {v:#?}"
    );
}

/// Benign (over-fire guard): a NULL `current_shift_id` (NC-03 / bootstrap class)
/// must NOT trip the check even when shift_state differs from any shifts row —
/// the `current_shift_id IS NOT NULL` predicate excludes it.
#[tokio::test]
async fn clean_null_current_shift_id_no_mirror_drift() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await; // a shifts row exists (OPENED), but unpointed
    seed_node_state(&pool, shift, None).await;
    // NULL the pointer + diverge shift_state — the check must still skip it.
    sqlx::query("UPDATE node_state SET current_shift_id = NULL, shift_state = 'CLOSED' WHERE fiscal_number = ?")
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();
    let v = scan(&pool).await.unwrap();
    assert!(
        !v.iter()
            .any(|x| matches!(x, Violation::ShiftStateMirrorDrift { .. })),
        "NULL current_shift_id must NOT flag ShiftStateMirrorDrift: {v:#?}"
    );
}

// ── U1 A1 — adoption-lint funnel: static source-scan of the fuzzer RefModel ──

/// U1 A1 — mechanical exhaustiveness of the derive-don't-adopt funnel: ALL DB
/// access in the fuzzer's `RefModel` (`invariant_fuzzer/model.rs`) MUST go through
/// one of the three tagged wrapper fns, so no future raw read can silently
/// re-open a differential vacuity at the D1–D5 adoption sites.  A raw
/// `sqlx`/`query_as`/`query_scalar`/`fetch_`/`.execute` outside a wrapper =
/// FORBIDDEN non-empty = scan failure.  Source-text scan (the model file is a
/// sibling of this test); the same discipline as the compile-fail/guard scans.
#[test]
fn model_db_access_is_funneled_through_tagged_wrappers() {
    const WRAPPERS: &[&str] = &[
        "adopt_fault_deferred", // post-fault recovery residue (was resync_from_db)
        "adopt_precondition",   // mode/shift/session precondition residue
        "read_seed_fixture",    // seed-fixture grounding
    ];
    // The DB-access tokens sqlx exposes; a raw one outside a wrapper is a leak.
    const MARKERS: &[&str] = &[
        "sqlx::",
        ".fetch_",
        ".execute(",
        "query_as",
        "query_scalar",
        "query!",
    ];
    let src = include_str!("invariant_fuzzer/model.rs");
    let mut current_fn = "<module>";
    let mut violations: Vec<String> = Vec::new();
    for (idx, line) in src.lines().enumerate() {
        let trimmed = line.trim_start();
        // Track the enclosing fn (methods live in `impl` blocks; the most recent
        // `fn NAME` is the enclosing scope for the DB-access check).
        for prefix in ["pub async fn ", "async fn ", "pub fn ", "fn "] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                current_fn = rest
                    .split(|c: char| c == '(' || c == '<' || c == ' ')
                    .next()
                    .unwrap_or("<anon>");
                break;
            }
        }
        // `use` imports (`use sqlx::SqlitePool;`) and comments are not DB access.
        if trimmed.starts_with("use ") || trimmed.starts_with("//") {
            continue;
        }
        if MARKERS.iter().any(|m| line.contains(m)) && !WRAPPERS.contains(&current_fn) {
            violations.push(format!(
                "model.rs:{}: DB access in `{current_fn}` — not a tagged wrapper: {}",
                idx + 1,
                trimmed
            ));
        }
    }
    assert!(
        violations.is_empty(),
        "U1 A1 adoption-lint FORBIDDEN non-empty — every DB read in model.rs must go through \
         {WRAPPERS:?}:\n{}",
        violations.join("\n")
    );
}
