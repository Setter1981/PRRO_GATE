//! bd `PRRO_GATE-a6n` — pins for the TURNOVER-counted set.
//!
//! Spec: `docs/superpowers/specs/2026-08-02-spec-turnover-counted-set.md`.
//!
//! The cash aggregate used to filter `state IN ('ACK','OFFLINE_LOCAL_ACK')` —
//! DELIVERY state read as FISCAL validity.  `backlog_drain` walks a drained
//! offline document `OFFLINE_LOCAL_ACK → SENDING → SENT → KVT1 → KVT2 → ACK`,
//! and a `TransientRetry` parks it durably in `ERROR_RETRYABLE` **while the
//! shift stays `OpenedLocalPendingDrain`** (backlog_drain module doc,
//! HIGH-C4-8) — i.e. open, so the X-report answers and the INV-21 guards read.
//! The same cash leg was counted at OLA, uncounted for the whole drain, then
//! counted again at ACK, with no fiscal event in between.
//!
//! RED-first: pins 1/2/4/6 were observed RED before the fix and GREEN after.
//! Pins 3/5 are the anti-erosion twins (they must stay GREEN across the change
//! — a fix that reds them has widened turnover too far).  Pin 7 is the §8.1
//! canary for the neutrality argument that licenses the whole change.
//!
//! New tests live here and NOT in `l0_l1_cash_ledger.rs`, which is one of the
//! 79 frozen CS-1 files (`docs/cs1r/pins/cs1_canonical_fingerprints.tsv:81`).

use prro::db::models::enums::{DocState, DocType, FiscalMode, NodeMode, ShiftState};
use prro::db::models::ids::{DocumentId, RequestId, ShiftId};
use prro::db::repositories::fiscal_documents as fd;
use prro::db::repositories::fiscal_number_config::{self as fn_repo, NewFnConfig};
use prro::db::repositories::node_state;
use prro::db::repositories::payment_methods::{insert as pm_insert, NewPaymentMethod};
use prro::db::repositories::shifts;
use prro::db::types::{DbDocumentId, DbShiftId};
use prro::db::{open_pool, open_secure_pool};
use prro::runtime::ingress::convert::{convert_to_signer_payload, ConvertError};
use prro::runtime::ingress::dto::CanonicalCommand;
use prro::runtime::ingress::z_builder::{quiesce_shift_before_z, QuiescenceOutcome};
use prro::services::cash_ledger::{aggregate_shift_cash, cash_on_hand_for_fn, derive_closing_cash};
use sqlx::SqlitePool;

const FN: &str = "4000100001";
const CASH_KOP: i64 = 15_000;

/// The delivery-transit states a drained OFFLINE-origin doc walks through.
/// Not one of them is a fiscal event — they are delivery bookkeeping.
const OFFLINE_TRANSIT: [DocState; 5] = [
    DocState::Sending,
    DocState::Sent,
    DocState::Kvt1,
    DocState::Kvt2,
    DocState::ErrorRetryable,
];

/// The same for an ONLINE-origin doc, which enters turnover at the
/// `Sending → Sent` CAS (A.3 stamps `server_fiscal_no` there).  `SENDING` is
/// absent by design: pre-CAS there is no sfn, so the doc is not yet issued.
const ONLINE_TRANSIT: [DocState; 4] = [
    DocState::Sent,
    DocState::Kvt1,
    DocState::Kvt2,
    DocState::ErrorRetryable,
];

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

async fn fresh_main() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let main = open_pool(&dir.path().join("main.db"))
        .await
        .expect("open main");
    (dir, main)
}

async fn seed_fn(pool: &SqlitePool) {
    fn_repo::insert(
        pool,
        &NewFnConfig {
            fiscal_number: FN.to_string(),
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
    .expect("seed fn_config");
}

async fn seed_payment_methods(secure: &SqlitePool) {
    pm_insert(
        secure,
        &NewPaymentMethod {
            fn_id: FN.to_string(),
            pay_index: 1,
            name: "Готівка".into(),
            iscash: true,
        },
    )
    .await
    .expect("seed cash pm");
    pm_insert(
        secure,
        &NewPaymentMethod {
            fn_id: FN.to_string(),
            pay_index: 2,
            name: "Картка".into(),
            iscash: false,
        },
    )
    .await
    .expect("seed card pm");
}

/// A shift parked exactly where a drain leaves it: `OpenedLocalPendingDrain`
/// (edge 2 — the offline SHIFT_OPEN Pattern C destination).  This is an OPEN
/// state: the X-report open-shift gate admits it, and `cash_on_hand_for_fn`
/// resolves it.
async fn seed_pending_drain_shift(pool: &SqlitePool, id: ShiftId) {
    use prro::db::tx::with_immediate;
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            shifts::insert_created_tx(tx, id, FN, "OFFLINE", "cashier-1", 0)
                .await
                .map_err(Into::into)
        })
    })
    .await
    .expect("insert shift");
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            shifts::transition(
                tx,
                id,
                ShiftState::Created,
                ShiftState::OpenedLocalPendingDrain,
            )
            .await
            .map(|_| ())
            .map_err(Into::into)
        })
    })
    .await
    .expect("shift → OpenedLocalPendingDrain");

    node_state::upsert_initial(
        pool,
        FN,
        NodeMode::Offline,
        ShiftState::OpenedLocalPendingDrain,
        1,
    )
    .await
    .unwrap();
    sqlx::query("UPDATE node_state SET current_shift_id = ? WHERE fiscal_number = ?")
        .bind(DbShiftId(id))
        .bind(FN)
        .execute(pool)
        .await
        .unwrap();
}

/// Insert one cash-bearing doc.  `offline` picks the origin lane:
///   - offline-origin stamps `offline_fiscal_no` (it crossed OLA, advanced the
///     MAC seed, and the customer holds the receipt);
///   - online-origin stamps `server_fiscal_no` (the A.3 advance-at-SEND
///     discriminator).
async fn seed_cash_doc(
    pool: &SqlitePool,
    shift: ShiftId,
    lnd: i64,
    doc_type: DocType,
    offline: bool,
    sum_kop: i64,
) -> DocumentId {
    let payments = format!(r#"{{"name":"Готівка","sum_kop":{sum_kop},"type_code":"0"}}"#);
    let new = fd::NewDocument {
        document_id: DocumentId::new(),
        request_id: RequestId::new(),
        fiscal_number: FN.to_string(),
        shift_id: Some(shift),
        offline_session_id: None,
        lnd,
        doc_type,
        backend_profile_id: "b".into(),
        transport_profile_id: "t".into(),
        fs_mode: if offline { "OFFLINE" } else { "ONLINE" },
        business_ts: "2026-08-02T10:00:00Z".into(),
        total_sum_kop: Some(sum_kop),
        payload_json: format!(r#"{{"items":[],"payments":[{payments}]}}"#),
        payload_sha256_canonical: [0u8; 32],
        source_sha256: [0u8; 32],
        unsigned_xml_sha256: None,
        previous_hash: None,
        signed_by_cashier_id: None,
        signing_config_snapshot_id: None,
    };
    let id = new.document_id;
    fd::insert_prepared(pool, &new)
        .await
        .expect("insert_prepared");
    if offline {
        sqlx::query("UPDATE fiscal_documents SET offline_fiscal_no = ? WHERE document_id = ?")
            .bind(lnd)
            .bind(DbDocumentId(id))
            .execute(pool)
            .await
            .expect("stamp offline_fiscal_no");
    } else {
        sqlx::query("UPDATE fiscal_documents SET server_fiscal_no = ? WHERE document_id = ?")
            .bind(format!("DPS-{lnd}"))
            .bind(DbDocumentId(id))
            .execute(pool)
            .await
            .expect("stamp server_fiscal_no");
    }
    id
}

/// Seed an EPZ (cash-advance) doc — offline-origin.
async fn seed_epz_doc(pool: &SqlitePool, shift: ShiftId, lnd: i64, sum_kop: i64) -> DocumentId {
    let new = fd::NewDocument {
        document_id: DocumentId::new(),
        request_id: RequestId::new(),
        fiscal_number: FN.to_string(),
        shift_id: Some(shift),
        offline_session_id: None,
        lnd,
        doc_type: DocType::CashAdvanceEpz,
        backend_profile_id: "b".into(),
        transport_profile_id: "t".into(),
        fs_mode: "OFFLINE",
        business_ts: "2026-08-02T10:00:00Z".into(),
        total_sum_kop: Some(sum_kop),
        payload_json: format!(r#"{{"sum_kop":{sum_kop},"name":"EPZ"}}"#),
        payload_sha256_canonical: [0u8; 32],
        source_sha256: [0u8; 32],
        unsigned_xml_sha256: None,
        previous_hash: None,
        signed_by_cashier_id: None,
        signing_config_snapshot_id: None,
    };
    let id = new.document_id;
    fd::insert_prepared(pool, &new)
        .await
        .expect("insert_prepared epz");
    sqlx::query("UPDATE fiscal_documents SET offline_fiscal_no = ? WHERE document_id = ?")
        .bind(lnd)
        .bind(DbDocumentId(id))
        .execute(pool)
        .await
        .expect("stamp offline_fiscal_no");
    id
}

async fn force_state(pool: &SqlitePool, id: DocumentId, state: DocState) {
    sqlx::query("UPDATE fiscal_documents SET state = ? WHERE document_id = ?")
        .bind(state.as_str())
        .bind(DbDocumentId(id))
        .execute(pool)
        .await
        .expect("set state");
}

fn return_cmd_cash(kop: i64) -> CanonicalCommand {
    let json = format!(
        r#"{{
            "schema_version": "1.0",
            "fiscal_number": "{FN}",
            "command_type": "RETURN",
            "idempotency_key": "ret-a6n",
            "cashier_id": null,
            "department": null,
            "return_check_number": null,
            "payload": {{
                "direction": "RETURN",
                "goods": [{{"name":"Item","quantity_milli":1000,"price_kopecks":{kop},"tax_group_1":0,"tax_group_2":0,"article_code":1}}],
                "payments": [{{"type":"CASH","amount_kopecks":{kop}}}],
                "totals": {{"sale_kopecks":0,"return_kopecks":{kop}}}
            }}
        }}"#
    );
    serde_json::from_str(&json).expect("parse RETURN cmd")
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 1 (§8.2/1) — the offline flicker is gone
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn offline_cash_leg_survives_the_drain_transit() {
    let (_dir, pool) = fresh_main().await;
    seed_fn(&pool).await;
    let shift = ShiftId::new();
    seed_pending_drain_shift(&pool, shift).await;
    let doc = seed_cash_doc(&pool, shift, 1, DocType::Sell, true, CASH_KOP).await;

    // The full drain walk, as `backlog_drain` enumerates its own cohort.
    let mut walk = vec![DocState::OfflineLocalAck];
    walk.extend_from_slice(&OFFLINE_TRANSIT);
    walk.push(DocState::Ack);

    let mut observed: Vec<(&str, i64, i64)> = Vec::new();
    for st in walk {
        force_state(&pool, doc, st).await;
        let aggregate = derive_closing_cash(&pool, FN, shift, 0)
            .await
            .expect("derive_closing_cash");
        // What the X-report prints and what the INV-21 guards read.
        let observable = cash_on_hand_for_fn(&pool, FN).await.expect("cash_on_hand");
        observed.push((st.as_str(), aggregate, observable));
    }

    let dropped: Vec<&(&str, i64, i64)> =
        observed.iter().filter(|(_, a, _)| *a != CASH_KOP).collect();
    assert!(
        dropped.is_empty(),
        "an issued offline cash receipt left the drawer during a DELIVERY-only \
         transition.\n  expected {CASH_KOP} kop counted in every drain state\n  \
         observed (state, aggregate_kop, cash_on_hand_for_fn_kop): {observed:#?}\n  \
         dropped in: {dropped:#?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 2 (§8.2/2) — the online lane counts from SENT, not ACK
//
// Authority: spec §4.0.  Our cash formula was ported from WebCheck's `Nal()`,
// whose turnover query (`Reports.cs:50-84`) has NO delivery predicate, and whose
// receipt row is written the instant the DPS submit returns a fiscal number
// (`StringXML.cs:1382`) — which IS our `Sending → Sent` CAS.
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn online_cash_leg_counts_from_sent_not_ack() {
    let (_dir, pool) = fresh_main().await;
    seed_fn(&pool).await;
    let shift = ShiftId::new();
    seed_pending_drain_shift(&pool, shift).await;
    let doc = seed_cash_doc(&pool, shift, 1, DocType::Sell, false, CASH_KOP).await;

    let mut observed: Vec<(&str, i64)> = Vec::new();
    for st in ONLINE_TRANSIT.iter().copied().chain([DocState::Ack]) {
        force_state(&pool, doc, st).await;
        let observable = cash_on_hand_for_fn(&pool, FN).await.expect("cash_on_hand");
        observed.push((st.as_str(), observable));
    }
    let dropped: Vec<&(&str, i64)> = observed.iter().filter(|(_, c)| *c != CASH_KOP).collect();
    assert!(
        dropped.is_empty(),
        "an sfn-stamped online receipt is not in turnover.\n  observed: {observed:#?}\n  \
         dropped in: {dropped:#?}"
    );

    // The pre-CAS half of the same rule: an online doc still in SENDING has NO
    // `server_fiscal_no` yet, so it must NOT be counted.  Clearing the stamp
    // reproduces the pre-CAS row exactly.
    sqlx::query(
        "UPDATE fiscal_documents SET server_fiscal_no = NULL, state = 'SENDING' \
         WHERE document_id = ?",
    )
    .bind(DbDocumentId(doc))
    .execute(&pool)
    .await
    .expect("clear sfn");
    assert_eq!(
        cash_on_hand_for_fn(&pool, FN).await.expect("cash_on_hand"),
        0,
        "an online doc in SENDING has no sfn — it is NOT yet issued and must not be counted"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 3 (§8.2/3) — ANTI-EROSION: the x5o adjudication survives verbatim
//
// bd PRRO_GATE-x5o ruled prod RIGHT: a cohort-cancelled offline receipt and an
// RMR-escalated held doc legitimately leave turnover (DPS never accepted them).
// This is the regression that matters most — the fix must not re-admit them.
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn x5o_cancelled_and_rmr_and_rejected_stay_out_of_turnover() {
    let (_dir, pool) = fresh_main().await;
    seed_fn(&pool).await;
    let shift = ShiftId::new();
    seed_pending_drain_shift(&pool, shift).await;
    let doc = seed_cash_doc(&pool, shift, 1, DocType::Sell, true, CASH_KOP).await;

    for st in [
        DocState::Cancelled,
        DocState::RequiresManualReconciliation,
        DocState::Rejected,
        DocState::Aborted,
    ] {
        force_state(&pool, doc, st).await;
        assert_eq!(
            cash_on_hand_for_fn(&pool, FN).await.expect("cash_on_hand"),
            0,
            "a receipt in {} is VOID — DPS never accepted it, so it must not be turnover \
             (bd PRRO_GATE-x5o adjudication)",
            st.as_str()
        );
    }

    // And the pre-issuance staging states are equally out.
    for st in [DocState::Prepared, DocState::Signed, DocState::Encrypted] {
        force_state(&pool, doc, st).await;
        assert_eq!(
            cash_on_hand_for_fn(&pool, FN).await.expect("cash_on_hand"),
            0,
            "a doc in {} was never issued",
            st.as_str()
        );
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Pins 4 + 5 (§8.2/4,5) — INV-21: stops false-refusing, still refuses for real
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn inv21_admits_a_return_while_the_backlog_doc_is_mid_drain() {
    let dir = tempfile::tempdir().expect("tempdir");
    let main = open_pool(&dir.path().join("main.db")).await.expect("main");
    let secure = open_secure_pool(&dir.path().join("secure.db"))
        .await
        .expect("secure");
    seed_fn(&main).await;
    seed_payment_methods(&secure).await;
    let shift = ShiftId::new();
    seed_pending_drain_shift(&main, shift).await;
    let doc = seed_cash_doc(&main, shift, 1, DocType::Sell, true, CASH_KOP).await;

    // The customer paid 150.00 in cash and holds an offline receipt.  The drain
    // hit a TransientRetry, so the doc rests in ERROR_RETRYABLE on a still-OPEN
    // pending-drain shift.  A refund of 100.00 is well within the drawer.
    force_state(&main, doc, DocState::ErrorRetryable).await;

    let converted = convert_to_signer_payload(&return_cmd_cash(10_000), FN, &main, &secure).await;
    assert!(
        converted.is_ok(),
        "INV-21 refused a legitimate refund because the receipt that put the cash in the \
         drawer was mid-drain: {:?}",
        converted.err()
    );
}

#[tokio::test]
async fn inv21_still_refuses_over_a_genuinely_empty_drawer() {
    let dir = tempfile::tempdir().expect("tempdir");
    let main = open_pool(&dir.path().join("main.db")).await.expect("main");
    let secure = open_secure_pool(&dir.path().join("secure.db"))
        .await
        .expect("secure");
    seed_fn(&main).await;
    seed_payment_methods(&secure).await;
    let shift = ShiftId::new();
    seed_pending_drain_shift(&main, shift).await;

    // Nothing issued at all → the floor must still bite.
    let err = convert_to_signer_payload(&return_cmd_cash(100), FN, &main, &secure)
        .await
        .expect_err("empty drawer must still refuse");
    assert!(
        matches!(
            err,
            ConvertError::CashInsufficient {
                cash_on_hand_kop: 0,
                return_cash_kop: 100
            }
        ),
        "expected CashInsufficient(0, 100), got {err:?}"
    );

    // A VOID receipt must not create spendable cash either — the widened set
    // must not have swallowed the x5o exclusions.
    //
    // Both void classes are exercised on purpose, because they bite DIFFERENT
    // over-widenings: `CANCELLED` is already outside `is_issued`, so only a
    // predicate that abandoned that SSOT would admit it; `REQUIRES_MANUAL_
    // RECONCILIATION` IS inside `is_issued` and is held out ONLY by this
    // predicate's delta — so dropping the delta (the most likely over-widening)
    // reds this pin.
    let doc = seed_cash_doc(&main, shift, 1, DocType::Sell, true, CASH_KOP).await;
    for void in [DocState::Cancelled, DocState::RequiresManualReconciliation] {
        force_state(&main, doc, void).await;
        let err = convert_to_signer_payload(&return_cmd_cash(100), FN, &main, &secure)
            .await
            .expect_err("a void receipt is not cash");
        assert!(
            matches!(err, ConvertError::CashInsufficient { .. }),
            "a {} receipt must not fund a refund, got {err:?}",
            void.as_str()
        );
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 6 (§8.2/6) — the Z `<EPZ>` count and sum select the SAME set
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn epz_count_and_sum_select_the_same_turnover_set() {
    let (_dir, pool) = fresh_main().await;
    seed_fn(&pool).await;
    let shift = ShiftId::new();
    seed_pending_drain_shift(&pool, shift).await;

    let counted = seed_epz_doc(&pool, shift, 1, 5_000).await;
    let void = seed_epz_doc(&pool, shift, 2, 7_000).await;
    force_state(&pool, counted, DocState::Kvt1).await; // mid-drain: counted
    force_state(&pool, void, DocState::Cancelled).await; // void: not counted

    let sum = prro::services::cash_ledger::aggregate_shift_epz(&pool, FN, shift)
        .await
        .expect("aggregate_shift_epz");
    assert_eq!(
        sum, 5_000,
        "EPSM must include the mid-drain EPZ and exclude the cancelled one"
    );

    // EPC is derived by the Z aggregation over the SAME predicate; re-derive it
    // here so a divergence between count and sum cannot pass unnoticed.
    let rows: Vec<(String, Option<i64>, Option<String>)> = sqlx::query_as(
        "SELECT state, offline_fiscal_no, server_fiscal_no FROM fiscal_documents \
         WHERE fiscal_number = ? AND shift_id = ? AND doc_type = 'CASH_ADVANCE_EPZ'",
    )
    .bind(FN)
    .bind(DbShiftId(shift))
    .fetch_all(&pool)
    .await
    .expect("epz rows");
    let epc = rows
        .iter()
        .filter(|(s, o, sf)| fd::counted_in_turnover(s, *o, sf.as_deref()))
        .count();
    assert_eq!(
        epc, 1,
        "EPC must count exactly the EPZ docs EPSM summed — count and sum may never disagree"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 7 (§8.1) — THE NEUTRALITY CANARY
//
// The whole licence for touching a hot zone is spec §5: widening the counted set
// is behaviour-NEUTRAL for every Z output BY CONSTRUCTION, because every state
// the change admits is a member of the Z-quiescence blocking set — so at the
// instant Z aggregation runs, they are provably empty.
//
// That argument must not rest on prose.  If this pin ever goes RED, §5 is no
// longer load-bearing and the change needs re-argument, not a patch.
//
// The pin is on the BLOCKING-SET PROJECTION, not on the full `quiesce` pass, and
// that is deliberate: `quiesce` Pass 1 inline-finalizes a LEADING `Kvt2` run, so
// a `Kvt2` doc may leave the set by being driven to `Ack` rather than by
// blocking.  Either mechanism satisfies §5 — what must hold is that no added
// state is ever INVISIBLE to the gate.  Asserting the projection covers both and
// does not drag `stage_finalize`'s preconditions into a set-membership pin.
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn z_quiescence_sees_every_newly_counted_state() {
    let (_dir, pool) = fresh_main().await;
    seed_fn(&pool).await;
    let shift = ShiftId::new();
    seed_pending_drain_shift(&pool, shift).await;
    let doc = seed_cash_doc(&pool, shift, 1, DocType::Sell, true, CASH_KOP).await;

    // Every state added to the counted set beyond the old ACK/OLA literal —
    // both lanes (ONLINE_TRANSIT is a subset of OFFLINE_TRANSIT).
    for st in OFFLINE_TRANSIT {
        force_state(&pool, doc, st).await;
        let pending = fd::list_shift_pending_receipts_for_z_quiescence(&pool, FN, shift)
            .await
            .expect("quiescence projection");
        assert!(
            pending.iter().any(|(id, s)| *id == doc && *s == st),
            "§5 neutrality is BROKEN: the Z-quiescence gate does NOT see a doc resting in {} — \
             which the widened turnover set now counts.  A Z could therefore aggregate over a \
             state the old literal excluded, so the change is no longer behaviour-neutral for Z \
             and must be re-argued, not patched.",
            st.as_str()
        );
    }

    // Sanity — the projection is not vacuously non-empty: a fully drained doc
    // leaves it, which is what lets a Z run at all.
    force_state(&pool, doc, DocState::Ack).await;
    let pending = fd::list_shift_pending_receipts_for_z_quiescence(&pool, FN, shift)
        .await
        .expect("quiescence projection");
    assert!(
        pending.is_empty(),
        "an ACK'd shift must quiesce empty, else this pin proves nothing: {pending:?}"
    );

    // And the full pass agrees on the clear case.
    assert!(
        matches!(
            quiesce_shift_before_z(&pool, FN, shift)
                .await
                .expect("quiescence pass"),
            QuiescenceOutcome::Clear
        ),
        "the quiescence pass must clear once every doc is ACK"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Aggregate-level twin of pin 1 — the SELL/RETURN legs specifically, so a
// regression in `list_shift_issued_receipts` (which also feeds the Z turnover
// section) is caught separately from the cash formula.
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn shift_aggregate_keeps_the_sell_leg_through_transit() {
    let (_dir, pool) = fresh_main().await;
    seed_fn(&pool).await;
    let shift = ShiftId::new();
    seed_pending_drain_shift(&pool, shift).await;
    let doc = seed_cash_doc(&pool, shift, 1, DocType::Sell, true, CASH_KOP).await;

    for st in OFFLINE_TRANSIT {
        force_state(&pool, doc, st).await;
        let (sell, ret, _in, _out, _epz) = aggregate_shift_cash(&pool, FN, shift)
            .await
            .expect("aggregate_shift_cash");
        assert_eq!(
            (sell, ret),
            (CASH_KOP, 0),
            "the SELL cash leg dropped out of the shift aggregate in state {}",
            st.as_str()
        );
    }
}
