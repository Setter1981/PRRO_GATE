//! CS-3 Slice 7 gap 1 — the production `record_outcome` boundary.
//!
//! RED-first teeth for `delivery_reservation::record_outcome` (design §8 step 3 / plan §2.3).
//! `record_outcome` is the missing record-then-apply commit: it transitions a `CALL_STARTED`
//! reservation to `OUTCOME_OBSERVED + PENDING_APPLY` under the FULL authority CAS, serializing
//! the sealed `ObservedOutcomeV1` axes + `EvidenceDiscriminant` columns (so the migration-036
//! matrix is satisfied by construction) and applying the EARLY node-safety halt. INACTIVE — the
//! `authorize → submit → record → apply` composition is wired at the S7-1 cutover.
//!
//! - `rc01` Accepted — row → OO+PENDING, evidence Accepted/F, correlation=F, node stays ONLINE;
//! - `rc02` terminal Reject (`-1` Verify) — OO+PENDING, TerminalReject/NoNodeEffect, node ONLINE;
//! - `rc03` SubmittedUnknown (NoResponse/Timeout) — OO+PENDING, node → STOP_MODE;
//! - `rc04` `-11` (Offline168 / NodeBlocked) — node → BLOCKED (not STOP);
//! - `rc05` `-12` (BadHashPrev / MacReseedPending) — held, node → STOP_MODE;
//! - `rc06` matrix teeth: an Accepted with NO correlation trips the 036 matrix → `Db`, row rolls
//!   back to CALL_STARTED (proves the fail-closed matrix bites THROUGH record_outcome);
//! - `rc07` authority CAS: a boot-resumed (now OO) reservation refuses a late record of the same
//!   in-flight call (`AuthorityMismatch`) — the boot conversion wins, no double-record.
//! - `rc08` AttemptObservation lifecycle: the post-wire observation minted from a token with a
//!   full non-None 5-binding carries every component + the evidence through to a matching CAS.

use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{
    authorize_submission, record_outcome, resume_crashed_reservation, AttemptObservation,
    Authorization, NewReservation, RecordError,
};
use prro::db::tx::with_immediate;
use prro_domain::delivery::evidence::EvidenceDiscriminant;
use prro_domain::delivery::{
    classify, AuthorizedGeneration, BoundedText, DecodedResponseDigest, DpsProtocolBinding,
    DpsProtocolId, EnvelopeHash, NoResponseCause, NonEmptyFiscalNumber, NonOkStatusCode,
    ObservedOutcomeV1, PositiveGeneration, ProtocolContractVersion, SendOutcome, SendResponse,
    SubmissionEvidence,
};
use prro_domain::enums::DocType;
use sqlx::SqlitePool;

const TS: &str = "2026-07-20T00:00:00Z";

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations");
    (dir, pool)
}

async fn seed_doc(pool: &SqlitePool, fscl: &str, doc_byte: u8) -> DocumentId {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fscl)
    .execute(pool)
    .await
    .expect("seed fiscal_number_config");
    sqlx::query(
        "INSERT OR IGNORE INTO node_state (fiscal_number, mode, shift_state, next_lnd) \
         VALUES (?, 'ONLINE', 'CREATED', 1)",
    )
    .bind(fscl)
    .execute(pool)
    .await
    .expect("seed node_state");
    let doc_bytes = vec![doc_byte; 16];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, ?, 1, 'SELL', 'SENDING', 'b1', 't1', 'ONLINE', \
            '2026-07-17T12:34:56Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(vec![doc_byte ^ 0xFF; 16])
    .bind(fscl)
    .bind(vec![0u8; 32])
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

fn new_res(res_byte: u8, doc: DocumentId, fscl: &str) -> NewReservation {
    NewReservation {
        reservation_id: [res_byte; 16],
        document_id: doc,
        fiscal_number: fscl.to_string(),
        dps_protocol_id: "FSCO_ZZD".to_string(),
        protocol_contract_version: 1,
        capability_profile_version: None,
        endpoint_config_revision: None,
        envelope_hash: [0xAB; 32],
    }
}

/// Authorize a fresh document → `CALL_STARTED` and return the sealed token.
async fn authorize(pool: &SqlitePool, res_byte: u8, doc: DocumentId, fscl: &str) -> Authorization {
    let row = new_res(res_byte, doc, fscl);
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            authorize_submission(tx, row, TS)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("authorize")
}

/// Record an observed outcome. Consumes the (non-Clone) post-wire `AttemptObservation` — a
/// record spends the observation (which itself consumed the sealed authority token).
async fn record(
    pool: &SqlitePool,
    obs: AttemptObservation,
    outcome: ObservedOutcomeV1,
    evidence: EvidenceDiscriminant,
) -> Result<(), anyhow::Error> {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            record_outcome(tx, &obs, &outcome, &evidence)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
}

// ── domain builders (mirror evidence.rs / cs3_c) ─────────────────────────────────
fn binding() -> DpsProtocolBinding {
    DpsProtocolBinding {
        protocol_id: DpsProtocolId::FscoZzd,
        contract_version: ProtocolContractVersion(1),
        capability_profile_version: None,
        endpoint_config_revision: None,
    }
}
fn started(response: SendResponse) -> SubmissionEvidence {
    SubmissionEvidence::Started {
        response,
        binding: binding(),
        envelope_hash: EnvelopeHash([0u8; 32]),
    }
}
fn digest() -> DecodedResponseDigest {
    DecodedResponseDigest::from_transport_digest([0xAB; 32])
}
fn from_code(code: i32) -> SubmissionEvidence {
    started(SendResponse::parsed(SendOutcome::from_server_code(
        NonOkStatusCode::from_transport(code).unwrap(),
        DocType::Sell,
        digest(),
    )))
}
/// Mint the (evidence, outcome) pair a real caller would derive from one `SubmissionEvidence`.
fn build(
    ev: &SubmissionEvidence,
    corr: Option<&str>,
    gen: i64,
) -> (EvidenceDiscriminant, ObservedOutcomeV1) {
    let classified = classify(ev);
    let disc = EvidenceDiscriminant::from_evidence(ev);
    let outcome = ObservedOutcomeV1::record(
        &classified,
        corr.map(BoundedText::from_truncating),
        AuthorizedGeneration::Started(PositiveGeneration::new(gen).unwrap()),
    )
    .expect("observed-outcome mint");
    (disc, outcome)
}

// ── row / node readers ───────────────────────────────────────────────────────────
#[allow(clippy::type_complexity)]
async fn read_row(
    pool: &SqlitePool,
    res_byte: u8,
) -> (
    String,         // state
    Option<String>, // apply_state
    Option<String>, // evidence_kind
    Option<String>, // evidence_text
    Option<String>, // node_effect
    Option<String>, // routing_class
    Option<String>, // remote_correlation_id
) {
    sqlx::query_as(
        "SELECT state, apply_state, evidence_kind, evidence_text, node_effect, routing_class, \
                remote_correlation_id FROM delivery_reservation WHERE reservation_id = ?",
    )
    .bind(&[res_byte; 16][..])
    .fetch_one(pool)
    .await
    .unwrap()
}
async fn read_mode(pool: &SqlitePool, fscl: &str) -> String {
    sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = ?")
        .bind(fscl)
        .fetch_one(pool)
        .await
        .unwrap()
}
fn is_err(err: &anyhow::Error, pred: impl Fn(&RecordError) -> bool) -> bool {
    err.downcast_ref::<RecordError>().is_some_and(pred)
}

// ───────────────────────────────── rc01 — Accepted ───────────────────────────────

#[tokio::test]
async fn rc01_accepted_records_oo_pending_correlation_and_no_halt() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "7000000001";
    let doc = seed_doc(&pool, fscl, 0x11).await;
    let auth = authorize(&pool, 0x01, doc, fscl).await;

    let ev = started(SendResponse::parsed(SendOutcome::accepted(
        NonEmptyFiscalNumber::from_transport("4000123456".into()).unwrap(),
    )));
    let (disc, outcome) = build(&ev, Some("4000123456"), 1);
    let obs = AttemptObservation::from_authorization(auth, ev);
    record(&pool, obs, outcome, disc)
        .await
        .expect("Accepted records");

    let (state, apply, kind, text, node, routing, corr) = read_row(&pool, 0x01).await;
    assert_eq!(state, "OUTCOME_OBSERVED");
    assert_eq!(apply.as_deref(), Some("PENDING_APPLY"));
    assert_eq!(kind.as_deref(), Some("Accepted"));
    assert_eq!(text.as_deref(), Some("4000123456"));
    assert_eq!(node.as_deref(), Some("NoNodeEffect"));
    assert_eq!(routing, None, "clean Accepted has NULL routing_class");
    assert_eq!(corr.as_deref(), Some("4000123456"), "correlation = F");
    // A clean accept is RESOLVED — record does NOT halt the node.
    assert_eq!(read_mode(&pool, fscl).await, "ONLINE");
}

// ───────────────────────── rc02 — terminal Reject (-1) ────────────────────────────

#[tokio::test]
async fn rc02_terminal_reject_records_and_does_not_halt() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "7000000002";
    let doc = seed_doc(&pool, fscl, 0x11).await;
    let auth = authorize(&pool, 0x02, doc, fscl).await;

    let ev = from_code(-1); // Verify → TerminalReject / NoNodeEffect
    let (disc, outcome) = build(&ev, None, 1);
    let obs = AttemptObservation::from_authorization(auth, ev);
    record(&pool, obs, outcome, disc)
        .await
        .expect("terminal reject records");

    let (state, apply, kind, text, node, routing, corr) = read_row(&pool, 0x02).await;
    assert_eq!(state, "OUTCOME_OBSERVED");
    assert_eq!(apply.as_deref(), Some("PENDING_APPLY"));
    assert_eq!(kind.as_deref(), Some("Rejected"));
    assert_eq!(text.as_deref(), Some("Verify"));
    assert_eq!(node.as_deref(), Some("NoNodeEffect"));
    assert_eq!(routing.as_deref(), Some("TerminalReject"));
    assert_eq!(corr, None);
    // A definitive terminal reject is RESOLVED (apply releases it) — no halt.
    assert_eq!(read_mode(&pool, fscl).await, "ONLINE");
}

// ───────────────────── rc03 — SubmittedUnknown (NoResponse) ───────────────────────

#[tokio::test]
async fn rc03_submitted_unknown_halts_stop_mode() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "7000000003";
    let doc = seed_doc(&pool, fscl, 0x11).await;
    let auth = authorize(&pool, 0x03, doc, fscl).await;

    let ev = started(SendResponse::no_response(NoResponseCause::Timeout));
    let (disc, outcome) = build(&ev, None, 1);
    let obs = AttemptObservation::from_authorization(auth, ev);
    record(&pool, obs, outcome, disc)
        .await
        .expect("NoResponse records");

    let (state, apply, kind, text, node, _routing, _corr) = read_row(&pool, 0x03).await;
    assert_eq!(state, "OUTCOME_OBSERVED");
    assert_eq!(apply.as_deref(), Some("PENDING_APPLY"));
    assert_eq!(kind.as_deref(), Some("NoResponse"));
    assert_eq!(text.as_deref(), Some("Timeout"));
    assert_eq!(node.as_deref(), Some("NoNodeEffect"));
    // UNRESOLVED (SUBMITTED_UNKNOWN) — record halts the node pending operator completion.
    assert_eq!(read_mode(&pool, fscl).await, "STOP_MODE");
}

// ───────────────────────────── rc04 — -11 NodeBlocked ─────────────────────────────

#[tokio::test]
async fn rc04_offline168_blocks_node() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "7000000004";
    let doc = seed_doc(&pool, fscl, 0x11).await;
    let auth = authorize(&pool, 0x04, doc, fscl).await;

    let ev = from_code(-11); // Offline168 → NodeBlocked
    let (disc, outcome) = build(&ev, None, 1);
    let obs = AttemptObservation::from_authorization(auth, ev);
    record(&pool, obs, outcome, disc)
        .await
        .expect("-11 records");

    let (_state, _apply, _kind, text, node, _routing, _corr) = read_row(&pool, 0x04).await;
    assert_eq!(text.as_deref(), Some("Offline168"));
    assert_eq!(node.as_deref(), Some("NodeBlocked"));
    // `-11` → the node is BLOCKED (over the offline ceiling), NOT the ambiguous STOP_MODE.
    assert_eq!(read_mode(&pool, fscl).await, "BLOCKED");
}

// ──────────────────────────── rc05 — -12 MacReseedPending ─────────────────────────

#[tokio::test]
async fn rc05_bad_hash_prev_held_stop_mode() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "7000000005";
    let doc = seed_doc(&pool, fscl, 0x11).await;
    let auth = authorize(&pool, 0x05, doc, fscl).await;

    let ev = from_code(-12); // BadHashPrev → MacReseedPending (held)
    let (disc, outcome) = build(&ev, None, 1);
    let obs = AttemptObservation::from_authorization(auth, ev);
    record(&pool, obs, outcome, disc)
        .await
        .expect("-12 records");

    let (_state, apply, _kind, text, node, _routing, _corr) = read_row(&pool, 0x05).await;
    assert_eq!(apply.as_deref(), Some("PENDING_APPLY"));
    assert_eq!(text.as_deref(), Some("BadHashPrev"));
    assert_eq!(node.as_deref(), Some("MacReseedPending"));
    // A held `-12` is UNRESOLVED — halt pending MAC-reseed / operator completion.
    assert_eq!(read_mode(&pool, fscl).await, "STOP_MODE");
}

// ─────────────── rc06 — matrix teeth: Accepted without correlation rolls back ──────

#[tokio::test]
async fn rc06_accepted_without_correlation_trips_matrix_and_rolls_back() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "7000000006";
    let doc = seed_doc(&pool, fscl, 0x11).await;
    let auth = authorize(&pool, 0x06, doc, fscl).await;

    // Accepted requires `remote_correlation_id = evidence_text` (migration 036 arm 5). A caller
    // that forgets the correlation writes NULL ≠ F → the matrix trigger ABORTs the record.
    let ev = started(SendResponse::parsed(SendOutcome::accepted(
        NonEmptyFiscalNumber::from_transport("4000123456".into()).unwrap(),
    )));
    let (disc, outcome) = build(&ev, None, 1); // <-- NO correlation
    let obs = AttemptObservation::from_authorization(auth, ev);
    let err = record(&pool, obs, outcome, disc)
        .await
        .expect_err("the 036 matrix refuses Accepted without correlation");
    assert!(
        is_err(&err, |e| matches!(e, RecordError::Db(_))),
        "expected a matrix-ABORT Db error; got {err:?}"
    );
    // The whole record rolled back — the reservation is still the authorized in-flight call.
    let (state, apply, kind, ..) = read_row(&pool, 0x06).await;
    assert_eq!(state, "CALL_STARTED");
    assert_eq!(apply, None);
    assert_eq!(kind, None, "no evidence half-written");
    assert_eq!(read_mode(&pool, fscl).await, "ONLINE", "no partial halt");
}

// ─────────────── rc07 — authority CAS: boot-resumed row refuses late record ────────

#[tokio::test]
async fn rc07_boot_resumed_reservation_refuses_late_record() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "7000000007";
    let doc = seed_doc(&pool, fscl, 0x11).await;
    let auth = authorize(&pool, 0x07, doc, fscl).await;

    // A boot (that raced the in-flight thread) resumes the crashed CALL_STARTED reservation to
    // OUTCOME_OBSERVED first — no token consumed.
    let fscl_owned = fscl.to_string();
    with_immediate(&pool, move |tx| {
        Box::pin(async move {
            resume_crashed_reservation(tx, [0x07; 16], &fscl_owned)
                .await
                .map(|_| ())
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("boot resume converts CALL_STARTED → OO");

    // The original thread's late record of the SAME in-flight call is refused — the state is no
    // longer CALL_STARTED, so the full-authority CAS matches zero rows.
    let ev = from_code(-1);
    let (disc, outcome) = build(&ev, None, 1);
    let obs = AttemptObservation::from_authorization(auth, ev);
    let err = record(&pool, obs, outcome, disc)
        .await
        .expect_err("a boot-resumed reservation refuses a late record");
    assert!(
        is_err(&err, |e| matches!(e, RecordError::AuthorityMismatch)),
        "expected AuthorityMismatch; got {err:?}"
    );
    // The boot conversion WINS: the row is still the resumed NoResponse hold (immutable), and the
    // node stays STOP_MODE (boot's halt) — no double-record, no clobber.
    let (state, apply, kind, text, ..) = read_row(&pool, 0x07).await;
    assert_eq!(state, "OUTCOME_OBSERVED");
    assert_eq!(apply.as_deref(), Some("PENDING_APPLY"));
    assert_eq!(kind.as_deref(), Some("NoResponse"));
    assert_eq!(text.as_deref(), Some("CrashedBeforeObservation"));
    assert_eq!(read_mode(&pool, fscl).await, "STOP_MODE");
}

// ── rc08 — AttemptObservation carries the FULL 5-binding token→obs→record CAS ──────

#[tokio::test]
async fn rc08_attempt_observation_carries_full_binding_and_evidence() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "7000000008";
    let doc = seed_doc(&pool, fscl, 0x11).await;

    // Authorize with the two EXTRA binding pins set (cap / endpoint = non-None). The
    // AttemptObservation minted from that token must carry ALL FIVE binding components, so the
    // record full-authority CAS (which includes `capability_profile_version IS ? AND
    // endpoint_config_revision IS ?`) still matches. If `from_authorization` dropped a component,
    // the obs would carry NULL, the CAS would miss the row (which holds 7 / 3), and record → miss.
    let row = NewReservation {
        capability_profile_version: Some(7),
        endpoint_config_revision: Some(3),
        ..new_res(0x08, doc, fscl)
    };
    let auth = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            authorize_submission(tx, row, TS)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("authorize with full binding");
    assert_eq!(auth.capability_profile_version(), Some(7));
    assert_eq!(auth.endpoint_config_revision(), Some(3));

    let ev = from_code(-1); // Verify → TerminalReject
    let (disc, outcome) = build(&ev, None, 1);
    let obs = AttemptObservation::from_authorization(auth, ev.clone());
    // The observation carries the token's full binding + the observed evidence verbatim.
    assert_eq!(
        obs.capability_profile_version(),
        Some(7),
        "obs carries cap binding"
    );
    assert_eq!(
        obs.endpoint_config_revision(),
        Some(3),
        "obs carries endpoint binding"
    );
    assert_eq!(obs.evidence(), &ev, "obs carries the observed evidence");

    record(&pool, obs, outcome, disc)
        .await
        .expect("a full-binding observation matches the record CAS");
    let (state, apply, ..) = read_row(&pool, 0x08).await;
    assert_eq!(state, "OUTCOME_OBSERVED");
    assert_eq!(apply.as_deref(), Some("PENDING_APPLY"));
}
