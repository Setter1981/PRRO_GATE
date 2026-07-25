//! `invariant_scan` cohort-cancel awareness via the `chain_superseded_at` marker (migration 039,
//! bd PRRO_GATE-2nk). NON-FROZEN companion to the CS-1-frozen `invariant_scan.rs`.
//!
//! Pins that the MAC-walk excludes a `chain_superseded_at` doc (a `NotAcceptedOffline`-rewound held
//! doc) + its `CANCELLED` cohort from the ACTIVE chain — so the legitimate cohort-cancel state scans
//! CLEAN — while still catching a real fork, AND that the exclusion is keyed on the MARKER, not the
//! RMR state (an UNMARKED RMR held doc still trips the walk — audit BLOCKER 2).

use prro::db::invariant_scan::{scan, Violation};
use prro::db::models::enums::{FiscalMode, NodeMode, ShiftState};
use prro::db::models::ids::{DocumentId, OfflineSessionId, RequestId, ShiftId};
use prro::db::open_pool;
use prro::db::repositories::fiscal_documents::active_chain_tip_unsigned_xml_sha256 as active_tip;
use prro::db::repositories::{fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig};
use prro::db::types::{DbDocumentId, DbOfflineSessionId, DbShiftId};
use sqlx::SqlitePool;

const FN: &str = "4000000001";
const SESSION: [u8; 16] = [0x5E; 16];

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
    .bind(DbShiftId(shift_id))
    .bind(FN)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, 'DRAINING', '2026-07-24T00:00:00Z')",
    )
    .bind(DbOfflineSessionId(OfflineSessionId::from_bytes(SESSION)))
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
    .bind(NodeMode::Online.as_str())
    .bind(ShiftState::Opened.as_str())
    .bind(DbShiftId(shift_id))
    .bind(seed.map(|s| s.to_vec()))
    .execute(pool)
    .await
    .unwrap();
}

/// One offline-origin fiscal-document row, with an optional `chain_superseded_at` marker and an
/// optional backing offline code (so the OfflineFiscalNoUnbacked check stays silent).
#[allow(clippy::too_many_arguments)]
async fn seed_off(
    pool: &SqlitePool,
    shift: ShiftId,
    lnd: i64,
    state: &str,
    previous_hash: Option<[u8; 32]>,
    unsigned_sha: [u8; 32],
    superseded: bool,
) -> DocumentId {
    let doc_id = DocumentId::new();
    let request_id: [u8; 16] = *RequestId::new().as_bytes();
    sqlx::query(
        "INSERT INTO fiscal_documents( \
            document_id, request_id, fiscal_number, shift_id, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, previous_hash, unsigned_xml_sha256, offline_fiscal_no, \
            offline_session_id, chain_superseded_at) \
         VALUES (?, ?, ?, ?, ?, 'SELL', ?, 'b', 't', 'OFFLINE', '2026-07-24T00:00:00Z', '{}', \
            ?, ?, ?, ?, ?, ?)",
    )
    .bind(DbDocumentId(doc_id))
    .bind(&request_id[..])
    .bind(FN)
    .bind(DbShiftId(shift))
    .bind(lnd)
    .bind(state)
    .bind(&[0u8; 32][..])
    .bind(previous_hash.map(|h| h.to_vec()))
    .bind(&unsigned_sha[..])
    .bind(lnd) // offline_fiscal_no
    .bind(&SESSION[..])
    .bind(if superseded {
        Some("2026-07-24T00:00:01Z")
    } else {
        None
    })
    .execute(pool)
    .await
    .unwrap();
    // Back the offline code so check 6b (OfflineFiscalNoUnbacked) stays silent.
    sqlx::query(
        "INSERT INTO offline_codes(fiscal_number, code_lnd, consumed_at, consumed_by_document_id) \
         VALUES (?, ?, '2026-07-24T00:00:02Z', ?)",
    )
    .bind(FN)
    .bind(lnd)
    .bind(DbDocumentId(doc_id))
    .execute(pool)
    .await
    .unwrap();
    doc_id
}

fn h(b: u8) -> [u8; 32] {
    [b; 32]
}

/// The legitimate `NotAcceptedOffline` cohort-cancel state — the held doc MARKED `chain_superseded_at`,
/// its later OLA cohort `CANCELLED`, the node seed rewound to the held doc's `previous_hash` (H0 =
/// lnd 9's hash). The walk excludes the superseded held doc + the cancelled cohort → the active chain
/// (lnd 9) ends exactly at the node seed → CLEAN.
#[tokio::test]
async fn cohort_cancel_marked_state_scans_clean() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, Some(h(9))).await; // seed rewound to H0 = lnd9's hash
                                                     // lnd 9: the surviving active-chain tip the held doc chained onto (issued OLA).
    seed_off(&pool, shift, 9, "OFFLINE_LOCAL_ACK", None, h(9), false).await;
    // lnd 10: the held doc, RMR + chain_superseded (prev = H0 = h(9)).
    seed_off(
        &pool,
        shift,
        10,
        "REQUIRES_MANUAL_RECONCILIATION",
        Some(h(9)),
        h(10),
        true,
    )
    .await;
    // lnd 11, 12: the cancelled cohort (chained H10→H11).
    seed_off(&pool, shift, 11, "CANCELLED", Some(h(10)), h(11), false).await;
    seed_off(&pool, shift, 12, "CANCELLED", Some(h(11)), h(12), false).await;
    assert_eq!(
        scan(&pool).await.unwrap(),
        vec![],
        "a marked NotAcceptedOffline cohort-cancel (held→RMR+superseded, cohort→CANCELLED, seed \
         rewound to the surviving predecessor H0) is a legitimate ledger"
    );
}

/// AUDIT BLOCKER 2 canary — the exclusion is keyed on the MARKER, not the RMR state. The SAME ledger
/// as above but WITHOUT the `chain_superseded_at` marker on the held RMR doc still trips the walk
/// (the RMR doc advances `expected` to H10 while the node seed rests at H9 → ChainSeedMismatch). Proves
/// a bare `state='RMR'` (Sent→NotFound / ER-drain / MacReseed) does NOT get the free pass.
#[tokio::test]
async fn unmarked_rmr_held_doc_is_not_excused() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, Some(h(9))).await;
    seed_off(&pool, shift, 9, "OFFLINE_LOCAL_ACK", None, h(9), false).await;
    // held RMR doc WITHOUT the marker.
    seed_off(
        &pool,
        shift,
        10,
        "REQUIRES_MANUAL_RECONCILIATION",
        Some(h(9)),
        h(10),
        false, // NOT superseded
    )
    .await;
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::ChainSeedMismatch { .. })),
        "an UNMARKED RMR held doc must NOT be excused — the walk still counts it (marker, not \
         RMR-state, is load-bearing), got: {v:#?}"
    );
}

/// Fork guard — a LIVE issued doc (not superseded, not cancelled) left above the rewound seed is a
/// real breach the walk must still flag. lnd 12 is a live OFFLINE_LOCAL_ACK a buggy cohort-cancel
/// failed to cancel; it advances `expected` past the node seed → ChainSeedMismatch (and its
/// previous_hash chains onto a skipped cancelled doc → ChainBreak).
#[tokio::test]
async fn live_issued_doc_above_rewound_seed_is_flagged() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, Some(h(9))).await;
    seed_off(&pool, shift, 9, "OFFLINE_LOCAL_ACK", None, h(9), false).await;
    seed_off(
        &pool,
        shift,
        10,
        "REQUIRES_MANUAL_RECONCILIATION",
        Some(h(9)),
        h(10),
        true,
    )
    .await;
    seed_off(&pool, shift, 11, "CANCELLED", Some(h(10)), h(11), false).await;
    // lnd 12: a LIVE OLA doc that should have been cancelled — the fork.
    seed_off(
        &pool,
        shift,
        12,
        "OFFLINE_LOCAL_ACK",
        Some(h(11)),
        h(12),
        false,
    )
    .await;
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::ChainSeedMismatch { .. })),
        "a live issued doc orphaned above the rewound seed must still be flagged, got: {v:#?}"
    );
}

/// AUDIT residual #6 — nested / repeated rewind. Two independent `NotAcceptedOffline` rewinds each
/// superseded their held doc (lnd 10 and lnd 11) and each rewound the seed to the SAME surviving tip
/// (lnd 9, H0). The exclusion is transitive — the walk skips BOTH superseded docs → the active chain
/// (lnd 9) ends at the node seed → CLEAN. Pins that a second rewind does not resurrect the first.
#[tokio::test]
async fn nested_rewind_two_superseded_docs_scans_clean() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, Some(h(9))).await; // both rewinds targeted H0 = lnd9
    seed_off(&pool, shift, 9, "OFFLINE_LOCAL_ACK", None, h(9), false).await;
    // Two superseded held docs, each chained onto the surviving tip lnd 9 (prev = H0).
    seed_off(
        &pool,
        shift,
        10,
        "REQUIRES_MANUAL_RECONCILIATION",
        Some(h(9)),
        h(10),
        true,
    )
    .await;
    seed_off(
        &pool,
        shift,
        11,
        "REQUIRES_MANUAL_RECONCILIATION",
        Some(h(9)),
        h(11),
        true,
    )
    .await;
    assert_eq!(
        scan(&pool).await.unwrap(),
        vec![],
        "two superseded held docs (repeated rewind to the same surviving tip) scan clean — exclusion \
         is transitive"
    );
}

// ── active_chain_tip projection teeth (bd PRRO_GATE-2nk mandatory) ──────────

/// Tooth 1 (the BLOCKER) — the rewind target is a NON-document T=112 code-replenish seed (NO fiscal
/// doc has unsigned == Hs). `active_chain_tip` returns the superseded held doc's `previous_hash` = Hs
/// DIRECTLY, not the previous fiscal doc's hash / None. This is exactly the case the exclusion
/// approach got WRONG.
#[tokio::test]
async fn active_tip_recovers_t112_non_doc_rewind_target() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    let hs = h(0xB0); // sha256(T=112 request xml) — NO fiscal doc produces it
    seed_off(
        &pool,
        shift,
        10,
        "REQUIRES_MANUAL_RECONCILIATION",
        Some(hs),
        h(10),
        true,
    )
    .await;
    seed_off(&pool, shift, 11, "CANCELLED", Some(h(10)), h(11), false).await;
    assert_eq!(
        active_tip(&pool, FN).await.unwrap().as_deref(),
        Some(&hs[..]),
        "active tip must be the rewind target Hs (a non-doc T=112 seed), read from the superseded \
         held doc's previous_hash — NOT the previous fiscal doc / None"
    );
}

/// Tooth 2 — genesis: a superseded held BEGIN with NULL previous_hash → active tip None.
#[tokio::test]
async fn active_tip_genesis_superseded_recovers_none() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_off(
        &pool,
        shift,
        1,
        "REQUIRES_MANUAL_RECONCILIATION",
        None,
        h(1),
        true,
    )
    .await;
    seed_off(&pool, shift, 2, "CANCELLED", Some(h(1)), h(2), false).await;
    assert_eq!(
        active_tip(&pool, FN).await.unwrap(),
        None,
        "genesis rewind target → None"
    );
}

/// Tooth 3 — normal predecessor: the rewind target IS a surviving issued doc's hash.
#[tokio::test]
async fn active_tip_normal_predecessor_doc() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_off(&pool, shift, 9, "OFFLINE_LOCAL_ACK", None, h(9), false).await;
    seed_off(
        &pool,
        shift,
        10,
        "REQUIRES_MANUAL_RECONCILIATION",
        Some(h(9)),
        h(10),
        true,
    )
    .await;
    seed_off(&pool, shift, 11, "CANCELLED", Some(h(10)), h(11), false).await;
    assert_eq!(
        active_tip(&pool, FN).await.unwrap().as_deref(),
        Some(&h(9)[..])
    );
}

/// Tooth 4 — repeat rewind: two superseded held docs (each rewound to h9); active tip = the highest
/// superseded doc's previous_hash (the latest rewind), not resurrecting the first.
#[tokio::test]
async fn active_tip_repeat_rewind_two_markers() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_off(&pool, shift, 9, "OFFLINE_LOCAL_ACK", None, h(9), false).await;
    seed_off(
        &pool,
        shift,
        10,
        "REQUIRES_MANUAL_RECONCILIATION",
        Some(h(9)),
        h(10),
        true,
    )
    .await;
    seed_off(
        &pool,
        shift,
        11,
        "REQUIRES_MANUAL_RECONCILIATION",
        Some(h(9)),
        h(11),
        true,
    )
    .await;
    assert_eq!(
        active_tip(&pool, FN).await.unwrap().as_deref(),
        Some(&h(9)[..])
    );
}

/// Tooth 5 — a NEWER active issued doc (issued after the rewind, chained onto the rewound tip)
/// overrides the older marker: the seed re-advanced to that doc's own hash.
#[tokio::test]
async fn active_tip_newer_issued_overrides_marker() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_off(
        &pool,
        shift,
        10,
        "REQUIRES_MANUAL_RECONCILIATION",
        Some(h(9)),
        h(10),
        true,
    )
    .await;
    seed_off(
        &pool,
        shift,
        11,
        "OFFLINE_LOCAL_ACK",
        Some(h(9)),
        h(11),
        false,
    )
    .await;
    assert_eq!(
        active_tip(&pool, FN).await.unwrap().as_deref(),
        Some(&h(11)[..]),
        "a newer active issued doc re-advanced the seed past the marker"
    );
}

/// Tooth 6 (audit MAJOR) — a corrupt back-pointer WITHIN the dead CANCELLED cohort still breaks: the
/// cohort's internal MAC continuity is verified separately from the (excluded) active chain.
#[tokio::test]
async fn corrupt_cancelled_subchain_still_breaks() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, Some(h(9))).await;
    seed_off(&pool, shift, 9, "OFFLINE_LOCAL_ACK", None, h(9), false).await;
    seed_off(
        &pool,
        shift,
        10,
        "REQUIRES_MANUAL_RECONCILIATION",
        Some(h(9)),
        h(10),
        true,
    )
    .await;
    // CANCELLED with a WRONG previous_hash (0xEE != h10 = its lnd-predecessor's unsigned).
    seed_off(&pool, shift, 11, "CANCELLED", Some(h(0xEE)), h(11), false).await;
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::ChainBreak { lnd: 11, .. })),
        "a corrupt back-pointer in the cancelled cohort must still ChainBreak, got: {v:#?}"
    );
}

/// Tooth 7 (re-review MINOR — case 5) — a post-sign, NON-issued crash artifact (`SIGNED`, absent from
/// `OFFLINE_ISSUED_STATES`) resting ABOVE the real tip must NOT be treated as the active tip: the walk
/// falls through it (not dead, not superseded, not issued) and returns the issued doc below. Guards the
/// fall-through arm (fiscal_documents.rs) against a mutation that returned a non-issued row's unsigned.
#[tokio::test]
async fn active_tip_skips_signed_crash_artifact_above_tip() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    // lnd 9: the real active tip (issued OLA).
    seed_off(&pool, shift, 9, "OFFLINE_LOCAL_ACK", None, h(9), false).await;
    // lnd 10: a SIGNED crash artifact chained onto h(9); post-sign so unsigned is set (enters the
    // walk) but NON-issued → must be skipped, NOT returned as the tip.
    seed_off(&pool, shift, 10, "SIGNED", Some(h(9)), h(10), false).await;
    assert_eq!(
        active_tip(&pool, FN).await.unwrap().as_deref(),
        Some(&h(9)[..]),
        "a non-issued SIGNED crash artifact above the tip must be walked past, not treated as the tip"
    );
}
