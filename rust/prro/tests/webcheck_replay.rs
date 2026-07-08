//! WebCheck U3 — directed differential replay harness (tests-only).
//!
//! Loads the committed synthetic corpus (`tests/webcheck_corpus/`, U2) and
//! replays each fixture's receipt (`SELL`) ops through OUR real write-path by
//! driving `inline::run` directly (NOT the Op-fuzzer), with a `ScriptedDps`
//! producing abstract outcome classes (never raw WebCheck blobs). It then reads
//! back invariant-level observables and diffs them against the fixture's
//! expectations. A divergence on the CLEAN corpus is a CP3 finding (triage,
//! never re-bless); an INJECTED divergence must be caught (teeth).
//!
//! Authoritative spec: `docs/superpowers/specs/2026-07-02-webcheck-ground-truth-phase1-design.md`
//!   §3/U3 (conversion path + diffed observables), §5/A3 (acceptance), §7/CP3.
//! Ground truth (sole WebCheck citation source): `docs/webcheck_reverse/WEBCHECK_GROUND_TRUTH.md`.
//! Intentional differences (this harness's deliverable): `tests/webcheck_corpus/INTENTIONAL_DIFFERENCES.md`.
//!
//! CONVERSION PATH (pinned): lifecycle ops (`SHIFT_OPEN` / `DRAIN` /
//! `SHIFT_CLOSE`) are NOT drivable commands in our alphabet — they become
//! FIXTURE SETUP (seed an open shift / offline session + codes), exactly like
//! the invariant-fuzzer's `FuzzCtx`. Only `SELL` ops are driven through
//! `inline::run`. `z_report` is `replay:false` → skipped with an explicit log.
//! `cancel_edge` has a `cancelled` op our alphabet cannot express (Cancel lives
//! with Phase-2 RETURN) → replay-partial up to the cancel-point.

#![allow(dead_code)]

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use prro::db::models::enums::{
    DocState, FiscalMode, NodeMode, OfflineSessionState, Protocol, ShiftState,
};
use prro::db::models::ids::{OfflineSessionId, RequestId, ShiftId};
use prro::db::repositories::ingress_inbox::{
    self as inbox, InboxInsertOutcome, InboxRow, NewInboxEntry,
};
use prro::db::repositories::{fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig};
use prro::db::{open_pool, open_secure_pool};
use prro::services::offline_sync::{backlog_drain, return_online_probe};
use prro::services::reconciliation::RuntimeView;
use prro::services::write_path::inline;
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::dto::{CheckAck, CheckSignBlob, StatusSnapshot};
use prro::transports::dps::error::{AuthorizationKind, DpsError};

use crate::common::scripted_dps::ScriptedDps;
use crate::common::{det_signing_ctx, drain_test_guard};

mod common;

// ── Harness fixture constants (all synthetic; never diffed) ──────────────────

const HARNESS_FN: &str = "9000000001";
const CASHIER: &str = "cashier-01";
const DRIVER: &str = "drv-webcheck-u3";
const SERVER_FISCAL_NO: &str = "DPS-FN-U3";

// ─────────────────────────────────────────────────────────────────────────────
// Corpus loader (serde_json::Value — no serde-derive dep, mirrors the U2 scan test)
// ─────────────────────────────────────────────────────────────────────────────

/// One replayable op distilled from a corpus `sequence.json` entry.
struct CorpusOp {
    op_index: usize,
    operation_type: String,
    lnd: i64,
    offline_class: String,
    dps_outcome_class: String,
    idempotency_key: String,
    business_ts: String,
    /// The corpus (WebCheck-shaped) receipt payload — kept verbatim for the O3
    /// structural slice; translated to OUR canonical shape at drive time.
    payload_json: String,
}

/// The `expected_observables.json` for a fixture (full — includes boundary ops).
struct ExpectedObservables {
    lnd_sequence: Vec<i64>,
    state_class_sequence: Vec<String>,
    offline_codes_consumed: i64,
    previous_hash_chain: Vec<Option<String>>,
    issued_lnds: Vec<i64>,
    dropped_doctype_count: i64,
}

struct Fixture {
    name: String,
    replay: bool,
    ops: Vec<CorpusOp>,
    expected: ExpectedObservables,
}

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webcheck_corpus")
}

fn read_json(path: &PathBuf) -> serde_json::Value {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn load_fixture(name: &str) -> Fixture {
    let dir = corpus_dir().join(name);
    let seq = read_json(&dir.join("sequence.json"));
    let obs = read_json(&dir.join("expected_observables.json"));

    let replay = seq["replay"].as_bool().unwrap_or(true);
    let ops = seq["sequence"]
        .as_array()
        .expect("sequence array")
        .iter()
        .map(|op| {
            let cmd = &op["canonical_command"];
            CorpusOp {
                op_index: op["op_index"].as_u64().unwrap() as usize,
                operation_type: op["operation_type"].as_str().unwrap().to_string(),
                lnd: op["lnd"].as_i64().unwrap(),
                offline_class: op["offline_lifecycle_class"].as_str().unwrap().to_string(),
                dps_outcome_class: op["dps_outcome_class"].as_str().unwrap().to_string(),
                idempotency_key: cmd["idempotency_key"].as_str().unwrap().to_string(),
                business_ts: cmd["business_ts"].as_str().unwrap().to_string(),
                payload_json: cmd["payload_json"].as_str().unwrap().to_string(),
            }
        })
        .collect();

    let expected = ExpectedObservables {
        lnd_sequence: json_i64_vec(&obs["lnd_sequence"]),
        state_class_sequence: obs["state_class_sequence"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect(),
        offline_codes_consumed: obs["offline_codes_consumed"].as_i64().unwrap(),
        previous_hash_chain: obs["previous_hash_chain"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        issued_lnds: json_i64_vec(&obs["issued_lnds"]),
        dropped_doctype_count: obs["dropped_doctype_count"].as_i64().unwrap(),
    };

    Fixture {
        name: name.to_string(),
        replay,
        ops,
        expected,
    }
}

fn json_i64_vec(v: &serde_json::Value) -> Vec<i64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_i64().unwrap())
        .collect()
}

/// Corpus→canonical receipt adapter: map the WebCheck-shaped synthetic receipt
/// (`receipt.goods[]` with `price`/`quantity`/`sum`; `receipt.payments[]` with
/// `amount`/`payment_type`/`label`) into OUR canonical SELL payload (`items[]`
/// with `*_kop`/`quantity_thousandths`; `payments[]` with `sum_kop`/`type_code`)
/// so `inline::run` can build + sign the XML. Deterministic field-mapping of
/// committed synthetic content only — no external data. Returns
/// `(canonical_payload_json, total_sum_kop)`.
fn receipt_to_canonical(corpus_payload_json: &str) -> (String, i64) {
    let p: serde_json::Value = serde_json::from_str(corpus_payload_json).unwrap();
    let r = &p["receipt"];
    let items: Vec<serde_json::Value> = r["goods"]
        .as_array()
        .expect("receipt.goods")
        .iter()
        .map(|g| {
            serde_json::json!({
                "code": g["code"],
                "name": g["name"],
                "price_kop": g["price"].as_i64().unwrap(),
                "quantity_thousandths": g["quantity"].as_i64().unwrap(),
                "sum_kop": g["sum"].as_i64().unwrap(),
            })
        })
        .collect();
    let payments: Vec<serde_json::Value> = r["payments"]
        .as_array()
        .expect("receipt.payments")
        .iter()
        .map(|pay| {
            // WebCheck payment_type → our numeric type_code (CASH = "0", else "1").
            let type_code = if pay["payment_type"].as_str() == Some("CASH") {
                "0"
            } else {
                "1"
            };
            serde_json::json!({
                "name": pay["label"],
                "sum_kop": pay["amount"].as_i64().unwrap(),
                "type_code": type_code,
            })
        })
        .collect();
    let total = r["totals"]["total_sum"]
        .as_i64()
        .expect("receipt.totals.total_sum");
    let canonical = serde_json::json!({ "items": items, "payments": payments });
    (serde_json::to_string(&canonical).unwrap(), total)
}

// ─────────────────────────────────────────────────────────────────────────────
// Observed / Expected observable model + the differential comparator
// ─────────────────────────────────────────────────────────────────────────────

/// One driven doc read back from `fiscal_documents`, ordered by `lnd`.
#[derive(Clone)]
struct DocRec {
    lnd: i64,
    state: DocState,
    fs_mode: String,
    previous_hash: Option<Vec<u8>>,
    /// D3 discriminator inputs for the shared [`is_issued`] SSOT (C8): an
    /// online-origin doc keys on `server_fiscal_no` (set ⟺ seed advanced at
    /// SEND), an offline-origin doc keys on `offline_fiscal_no` + state.
    offline_fiscal_no: Option<i64>,
    server_fiscal_no: Option<String>,
}

/// What OUR write-path produced for the driven (SELL) subset.
struct Observed {
    docs: Vec<DocRec>,
    codes_consumed: i64,
}

/// What the fixture expects for the driven subset (boundary ops excluded).
#[derive(Clone)]
struct Expected {
    /// lnds of the driven sells, in order (== fixture `issued_lnds` for the
    /// driven prefix).
    lnds: Vec<i64>,
    /// per-driven-op state class (`online` / `offline_drained`).
    classes: Vec<String>,
    /// fixture `previous_hash` links for the driven sells (presence only — we
    /// diff structure, not hex, per §3/U3 "over our own recomputed hashes").
    chain: Vec<Option<String>>,
    codes_consumed: i64,
    issued_count: usize,
}

/// Map an observed doc to the corpus state-class vocabulary. The offline lane
/// rests at `OFFLINE_LOCAL_ACK` (Pattern C) then drains to `ACK` (fs_mode
/// OFFLINE) — WebCheck's "drained"; the online lane issues at `ACK` (fs_mode
/// ONLINE) — WebCheck's "online". See INTENTIONAL_DIFFERENCES.md.
fn observed_class(rec: &DocRec) -> String {
    match (&rec.state, rec.fs_mode.as_str()) {
        (DocState::Ack, "ONLINE") => "online".to_string(),
        (DocState::Ack, "OFFLINE") => "offline_drained".to_string(),
        (DocState::OfflineLocalAck, _) => "offline_local_ack".to_string(),
        (s, m) => format!("unexpected:{s:?}/{m}"),
    }
}

/// Invariant-level differential (spec §3/U3). Returns a list of divergences;
/// EMPTY == the fixture replayed faithfully. NEVER re-bless a non-empty result
/// on the clean corpus — triage it (CP3).
fn compare(exp: &Expected, obs: &Observed) -> Vec<String> {
    let mut d = Vec::new();
    let n = exp.lnds.len();

    // ── lnd: count, strict monotonicity, gap-free, offset-aligned to fixture ──
    // Our lnds legitimately start below the fixture's (we SEED the shift-open /
    // drain boundary docs the fixture minted), so the diff is invariant to a
    // single documented constant offset — but every relative step must match.
    if obs.docs.len() != n {
        d.push(format!(
            "doc count: observed {} vs expected {n}",
            obs.docs.len()
        ));
    }
    let our: Vec<i64> = obs.docs.iter().map(|r| r.lnd).collect();
    for w in our.windows(2) {
        if w[1] <= w[0] {
            d.push(format!(
                "lnd not strictly monotonic: {} then {}",
                w[0], w[1]
            ));
        }
    }
    if !our.is_empty() {
        let span = our[our.len() - 1] - our[0] + 1;
        if span != our.len() as i64 {
            d.push(format!(
                "lnd not gap-free: span {span} over {} docs",
                our.len()
            ));
        }
    }
    if !our.is_empty() && !exp.lnds.is_empty() {
        let offset = exp.lnds[0] - our[0];
        for (i, (&o, &e)) in our.iter().zip(exp.lnds.iter()).enumerate() {
            if o + offset != e {
                d.push(format!(
                    "lnd[{i}]: observed {o} (+offset {offset}) != expected {e}"
                ));
            }
        }
    }

    // ── state classes (our DocState/fs_mode → corpus vocabulary) ──
    for (i, rec) in obs.docs.iter().enumerate() {
        if let Some(exp_class) = exp.classes.get(i) {
            let got = observed_class(rec);
            if &got != exp_class {
                d.push(format!(
                    "state_class[{i}]: observed {got:?} != expected {exp_class:?}"
                ));
            }
        }
    }

    // ── chain: STRUCTURAL over our own hashes — length, interior-link presence,
    // no-fork. NOT hex-equality vs the fixture's synthetic payload-hash chain
    // (that is an intentional difference; our chain is over our recomputed
    // unsigned-XML hashes). The head link is not diffed (we seed the shift-open
    // head the fixture chains from). ──
    if exp.chain.len() != obs.docs.len() {
        d.push(format!(
            "chain length: observed {} vs expected {}",
            obs.docs.len(),
            exp.chain.len()
        ));
    }
    let mut seen: BTreeSet<Vec<u8>> = BTreeSet::new();
    for i in 1..obs.docs.len() {
        if exp.chain.get(i).map(|c| c.is_none()).unwrap_or(false) {
            d.push(format!(
                "chain[{i}]: expected interior link is null (corpus chain corrupted)"
            ));
        }
        match &obs.docs[i].previous_hash {
            Some(h) if !h.is_empty() => {
                if !seen.insert(h.clone()) {
                    d.push(format!("chain[{i}]: duplicate previous_hash (fork)"));
                }
            }
            _ => d.push(format!(
                "chain[{i}]: our interior previous_hash is null (chain broken)"
            )),
        }
    }

    // ── offline-code consumption ──
    if obs.codes_consumed != exp.codes_consumed {
        d.push(format!(
            "offline_codes_consumed: observed {} vs expected {}",
            obs.codes_consumed, exp.codes_consumed
        ));
    }

    // ── issued-set membership ──
    let issued = obs.docs.iter().filter(|r| is_issued(r)).count();
    if issued != exp.issued_count {
        d.push(format!(
            "issued_count: observed {issued} vs expected {}",
            exp.issued_count
        ));
    }

    d
}

// ─────────────────────────────────────────────────────────────────────────────
// The harness: a self-contained FuzzCtx-like fixture over the real seams
// ─────────────────────────────────────────────────────────────────────────────

struct Harness {
    pool: SqlitePool,
    pool_secure: SqlitePool,
    _tempdir: tempfile::TempDir,
    _tempdir_secure: tempfile::TempDir,
    sign_ctx: SigningContext,
    fn_sign: CheckSignBlob,
    gate: Arc<tokio::sync::Mutex<()>>,
    send_calls: Arc<AtomicUsize>,
    last_calls: Arc<AtomicUsize>,
}

impl Harness {
    async fn new(mode: NodeMode, offline_codes: i64) -> Self {
        let (pool, _tempdir) = fresh_pool().await;
        let (pool_secure, _tempdir_secure) = fresh_secure_pool().await;
        seed_fn_config(&pool).await;
        let shift_id = seed_open_shift(&pool).await;
        seed_node_state(&pool, mode, shift_id).await;
        if mode == NodeMode::Offline {
            seed_open_offline_session(&pool).await;
            for code_lnd in 1..=offline_codes {
                seed_offline_code(&pool, code_lnd).await;
            }
        }
        Self {
            pool,
            pool_secure,
            _tempdir,
            _tempdir_secure,
            sign_ctx: det_signing_ctx(),
            fn_sign: CheckSignBlob(vec![0xAB, 0xCD]),
            gate: Arc::new(tokio::sync::Mutex::new(())),
            send_calls: Arc::new(AtomicUsize::new(0)),
            last_calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn new_dps(&self) -> ScriptedDps {
        ScriptedDps::new(Arc::clone(&self.send_calls), Arc::clone(&self.last_calls))
    }

    /// Drive one corpus SELL op through the real write-path. `online` provisions
    /// the wire for the `[Ack, Ack]` happy path; the offline lane makes no wire
    /// call (§5) and lands `OFFLINE_LOCAL_ACK`.
    async fn drive_sell(&self, op: &CorpusOp, online: bool) -> Result<(), String> {
        let row = self.seed_inbox_sell(op).await;
        let dps = self.new_dps();
        if online {
            dps.push_send(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF])));
            dps.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF])));
        }
        let guard = self.gate.clone().lock_owned().await;
        let result = inline::run(
            &self.pool,
            &self.pool_secure,
            &dps,
            &self.sign_ctx,
            &self.fn_sign,
            &guard,
            &row,
        )
        .await;
        drop(guard);
        result.map(|_| ()).map_err(|e| format!("{e:?}"))
    }

    /// Realize the fixture's drain: probe Offline→GoingOnline, then
    /// `backlog_drain::drain` (AckPath, one send/last per cohort doc) →
    /// GoingOnline→Online, the offline backlog advancing to `ACK`.
    async fn drain_backlog(&self, backlog: usize) -> Result<(), String> {
        let dps = self.new_dps();
        dps.push_status(Ok(online_status()));
        for _ in 0..backlog {
            dps.push_send(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF])));
            dps.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF])));
        }
        return_online_probe::run_tick_for_fn(&self.pool, &dps, HARNESS_FN, &self.fn_sign)
            .await
            .map_err(|e| format!("probe: {e:?}"))?;
        let guard = drain_test_guard();
        let view = RuntimeView {
            dps: &dps,
            signing_ctx: &self.sign_ctx,
            fn_sign: &self.fn_sign,
        };
        backlog_drain::drain(&guard, &self.pool, &view, HARNESS_FN)
            .await
            .map_err(|e| format!("drain: {e:?}"))?;
        Ok(())
    }

    async fn seed_inbox_sell(&self, op: &CorpusOp) -> InboxRow {
        let request_id: [u8; 16] = *RequestId::new().as_bytes();
        // Translate the corpus (WebCheck-shaped) receipt into OUR canonical
        // payload so the write-path can build + sign the XML. The invariant-
        // relevant content (goods count, sums, payment) is preserved; only the
        // field shape changes. See INTENTIONAL_DIFFERENCES.md (corpus→canonical
        // adapter).
        let (payload_json, total_kop) = receipt_to_canonical(&op.payload_json);
        let payload_sha256_canonical: [u8; 32] = Sha256::digest(payload_json.as_bytes()).into();
        let outcome = inbox::insert(
            &self.pool,
            &NewInboxEntry {
                request_id,
                fiscal_number: HARNESS_FN.into(),
                protocol: Protocol::Rest,
                operation_type: "SELL".into(),
                idempotency_key: op.idempotency_key.clone(),
                payload_json,
                payload_sha256_canonical,
                correlation_id: None,
                signed_by_cashier_id: Some(CASHIER.into()),
                driver_id: Some(DRIVER.into()),
                business_ts: Some(op.business_ts.clone()),
                total_sum_kop: Some(total_kop),
            },
        )
        .await
        .expect("inbox insert");
        match outcome {
            InboxInsertOutcome::Created(r) | InboxInsertOutcome::Replay(r) => r,
            InboxInsertOutcome::Conflict { .. } => panic!("unexpected idempotency conflict"),
        }
    }

    /// Read the driven docs back, ordered by `lnd`, plus consumed offline codes.
    async fn observe(&self) -> Observed {
        // Positional sqlx read, destructured into DocRec immediately below; the
        // 6-tuple is contained to this one statement (C8 widened it by 2 cols).
        #[allow(clippy::type_complexity)]
        let rows: Vec<(
            i64,
            String,
            String,
            Option<Vec<u8>>,
            Option<i64>,
            Option<String>,
        )> = sqlx::query_as(
            "SELECT lnd, state, fs_mode, previous_hash, offline_fiscal_no, server_fiscal_no \
                 FROM fiscal_documents WHERE fiscal_number = ? ORDER BY lnd",
        )
        .bind(HARNESS_FN)
        .fetch_all(&self.pool)
        .await
        .unwrap();
        let docs = rows
            .into_iter()
            .map(
                |(lnd, state, fs_mode, previous_hash, offline_fiscal_no, server_fiscal_no)| {
                    DocRec {
                        lnd,
                        state: doc_state_from_str(&state),
                        fs_mode,
                        previous_hash,
                        offline_fiscal_no,
                        server_fiscal_no,
                    }
                },
            )
            .collect();
        let codes_consumed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM offline_codes \
             WHERE fiscal_number = ? AND consumed_at IS NOT NULL",
        )
        .bind(HARNESS_FN)
        .fetch_one(&self.pool)
        .await
        .unwrap();
        Observed {
            docs,
            codes_consumed,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Replay: fixture → (Expected, Observed)
// ─────────────────────────────────────────────────────────────────────────────

/// The driven (SELL) prefix: sells before the first `cancelled` op (replay
/// stops at the cancel-point — cancel is not in our alphabet). Returns the
/// driven ops together with whether the lane is offline.
fn driven_sells(fx: &Fixture) -> (Vec<&CorpusOp>, bool) {
    let cutoff = fx
        .ops
        .iter()
        .position(|op| op.offline_class == "cancelled")
        .unwrap_or(fx.ops.len());
    let driven: Vec<&CorpusOp> = fx.ops[..cutoff]
        .iter()
        .filter(|op| op.operation_type == "SELL")
        .collect();
    let offline = driven
        .iter()
        .any(|op| op.offline_class == "offline_drained");
    (driven, offline)
}

/// Build the `Expected` from the fixture, restricted to the driven sells.
fn expected_for(fx: &Fixture, driven: &[&CorpusOp]) -> Expected {
    let lnds: Vec<i64> = driven.iter().map(|op| op.lnd).collect();
    let classes: Vec<String> = driven.iter().map(|op| op.offline_class.clone()).collect();
    // the fixture chain link for each driven op (index by op_index into the full
    // per-op chain).
    let chain: Vec<Option<String>> = driven
        .iter()
        .map(|op| fx.expected.previous_hash_chain[op.op_index].clone())
        .collect();
    let codes_consumed = driven
        .iter()
        .filter(|op| op.offline_class == "offline_drained")
        .count() as i64;
    Expected {
        lnds,
        classes,
        chain,
        codes_consumed,
        issued_count: driven.len(),
    }
}

/// Drive a whole fixture and return (Expected, Observed).
async fn replay(fx: &Fixture) -> (Expected, Observed) {
    let (driven, offline) = driven_sells(fx);
    // B10: an offline session lazily mints a DocType=9 BEGIN at the first offline
    // doc (consumes +1 code) and a DocType=10 END at drain finalize (consumes +1
    // code, +1 wire send).  So an offline replay needs `driven.len() + 2` codes
    // (BEGIN + END on top of one per driven sell) and the drain sends
    // `driven.len() + 2` docs (BEGIN + content + END).
    let boundary_docs = if offline { 2 } else { 0 };
    let codes = if offline {
        driven.len() as i64 + boundary_docs
    } else {
        0
    };
    let mode = if offline {
        NodeMode::Offline
    } else {
        NodeMode::Online
    };
    let h = Harness::new(mode, codes).await;

    for op in &driven {
        h.drive_sell(op, !offline)
            .await
            .unwrap_or_else(|e| panic!("{}: drive op {} failed: {e}", fx.name, op.op_index));
    }
    if offline {
        // OLA → ACK via the real drain (WebCheck "drained").  BEGIN + content + END.
        h.drain_backlog(driven.len() + boundary_docs as usize)
            .await
            .unwrap_or_else(|e| panic!("{}: drain failed: {e}", fx.name));
    }
    let observed = h.observe().await;
    (expected_for(fx, &driven), observed)
}

// ─────────────────────────────────────────────────────────────────────────────
// File-local seeds (mirror the invariant-fuzzer fixtures; parametrized by FN)
// ─────────────────────────────────────────────────────────────────────────────

async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let pool = open_pool(&dir.path().join("u3.db")).await.unwrap();
    (pool, dir)
}

async fn fresh_secure_pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let pool = open_secure_pool(&dir.path().join("u3-secure.db"))
        .await
        .unwrap();
    (pool, dir)
}

async fn seed_fn_config(pool: &SqlitePool) {
    fn_repo::insert(
        pool,
        &NewFnConfig {
            fiscal_number: HARNESS_FN.into(),
            tax_number: "10000001".into(),
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
}

async fn seed_open_shift(pool: &SqlitePool) -> ShiftId {
    let shift_id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, serial, state, open_mode, \
            cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED', 'ONLINE', 0, ?)",
    )
    .bind(shift_id)
    .bind(HARNESS_FN)
    .bind(CASHIER)
    .execute(pool)
    .await
    .unwrap();
    shift_id
}

async fn seed_node_state(pool: &SqlitePool, mode: NodeMode, shift_id: ShiftId) {
    sqlx::query(
        "INSERT INTO node_state \
         (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
          backend_profile_id, transport_profile_id) \
         VALUES (?, ?, ?, ?, 1, 'b', 't')",
    )
    .bind(HARNESS_FN)
    .bind(mode)
    .bind(ShiftState::Opened)
    .bind(shift_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_open_offline_session(pool: &SqlitePool) {
    let session_id = OfflineSessionId::new();
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, ?, '2026-01-01T00:00:00Z')",
    )
    .bind(session_id)
    .bind(HARNESS_FN)
    .bind(OfflineSessionState::Open.as_str())
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_offline_code(pool: &SqlitePool, code_lnd: i64) {
    sqlx::query("INSERT INTO offline_codes(fiscal_number, code_lnd, dps_code) VALUES (?, ?, ?)")
        .bind(HARNESS_FN)
        .bind(code_lnd)
        .bind(format!("DRILL-{code_lnd}"))
        .execute(pool)
        .await
        .unwrap();
}

// ── DPS helpers (mirror the invariant-fuzzer) ────────────────────────────────

fn ack(id: &str, data_sign: Vec<u8>) -> CheckAck {
    CheckAck {
        id: id.to_string(),
        id_sign: vec![],
        data_sign,
    }
}

fn online_status() -> StatusSnapshot {
    StatusSnapshot {
        open_shift: true,
        online: true,
        last_signer: String::new(),
    }
}

/// Reject → `Sending → Rejected` (DPS -1). Not used on the happy path; kept for
/// future outcome-class coverage (RETURN tranche).
fn reject_err() -> DpsError {
    DpsError::Authorization {
        code: -1,
        kind: AuthorizationKind::DocumentReject,
        message: "u3: document reject".to_string(),
    }
}

fn doc_state_from_str(s: &str) -> DocState {
    match s {
        "PREPARED" => DocState::Prepared,
        "SIGNED" => DocState::Signed,
        "ENCRYPTED" => DocState::Encrypted,
        "SENDING" => DocState::Sending,
        "SENT" => DocState::Sent,
        "KVT1" => DocState::Kvt1,
        "KVT2" => DocState::Kvt2,
        "ACK" => DocState::Ack,
        "OFFLINE_LOCAL_ACK" => DocState::OfflineLocalAck,
        "REJECTED" => DocState::Rejected,
        "CANCELLED" => DocState::Cancelled,
        "ERROR_RETRYABLE" => DocState::ErrorRetryable,
        "REQUIRES_MANUAL_RECONCILIATION" => DocState::RequiresManualReconciliation,
        "ABORTED" => DocState::Aborted,
        other => panic!("unknown DocState string from ledger: {other:?}"),
    }
}

/// Issued ⟺ the shared prod SSOT [`fiscal_documents::is_issued`] says so (C8 —
/// no bespoke fork of the predicate). D3 discriminator: an **online-origin** doc
/// (`offline_fiscal_no` NULL) is issued ⟺ its `server_fiscal_no` is set (stamped
/// atomically with the seed advance at SEND); an **offline-origin** doc
/// (`offline_fiscal_no` set) is issued at `ACK` or any `OFFLINE_ISSUED_STATES`
/// durable/transient state. `fs_mode` is NOT consulted — the discriminator is
/// the fiscal-number pair, exactly as the boot projection and the fuzzer model.
fn is_issued(rec: &DocRec) -> bool {
    prro::db::repositories::fiscal_documents::is_issued(
        doc_state_str(rec.state),
        rec.offline_fiscal_no,
        rec.server_fiscal_no.as_deref(),
    )
}

fn doc_state_str(s: DocState) -> &'static str {
    match s {
        DocState::Prepared => "PREPARED",
        DocState::Signed => "SIGNED",
        DocState::Encrypted => "ENCRYPTED",
        DocState::Sending => "SENDING",
        DocState::Sent => "SENT",
        DocState::Kvt1 => "KVT1",
        DocState::Kvt2 => "KVT2",
        DocState::Ack => "ACK",
        DocState::OfflineLocalAck => "OFFLINE_LOCAL_ACK",
        DocState::Rejected => "REJECTED",
        DocState::Cancelled => "CANCELLED",
        DocState::ErrorRetryable => "ERROR_RETRYABLE",
        DocState::RequiresManualReconciliation => "REQUIRES_MANUAL_RECONCILIATION",
        DocState::Aborted => "ABORTED",
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

/// C8 (shared-`is_issued` SSOT — online arm kept LIVE): the differential
/// comparator's issued-set membership delegates to the prod
/// `fiscal_documents::is_issued`. The happy replay corpus rests every driven doc
/// at `ACK`/`OFFLINE_LOCAL_ACK`, so without a synthetic case the online arm
/// (`offline_fiscal_no` NULL ⟹ issued ⟺ `server_fiscal_no` set) is NEVER
/// exercised distinctly from the universal ACK clause — a dead arm (operator's
/// C8 note). Post-A.3 an online doc that crossed SEND rests at `SENT` with its
/// `server_fiscal_no` stamped and its seed advanced, so it MUST count issued;
/// the same doc pre-SEND (no `server_fiscal_no`, empty treated as unset per D3)
/// MUST NOT. This pin dies RED against a predicate that keys on `fs_mode`/state.
#[test]
fn c8_is_issued_online_sent_arm_keys_on_server_fiscal_no() {
    let rec = |state, offline_fiscal_no, sfn: Option<&str>| DocRec {
        lnd: 1,
        state,
        fs_mode: "ONLINE".to_string(),
        previous_hash: None,
        offline_fiscal_no,
        server_fiscal_no: sfn.map(str::to_string),
    };
    // online-issued rests at SENT with server_fiscal_no stamped → issued
    assert!(
        is_issued(&rec(DocState::Sent, None, Some("70000001"))),
        "online SENT with server_fiscal_no must be issued (seed advanced at SEND)"
    );
    // same online doc pre-SEND (no server_fiscal_no) → NOT issued
    assert!(
        !is_issued(&rec(DocState::Sent, None, None)),
        "online SENT without server_fiscal_no must NOT be issued (seed not advanced)"
    );
    // empty server_fiscal_no is treated as unset (D3)
    assert!(
        !is_issued(&rec(DocState::Sent, None, Some(""))),
        "empty server_fiscal_no must be treated as unset (D3)"
    );
}

/// TEETH (RED-first): an INJECTED divergence (a gap punched into the expected
/// lnd sequence) MUST be caught by `compare`. With the stage-1 stub `compare`
/// this FAILS (no diff) — proving the test bites; stage 2 makes it pass.
#[tokio::test]
async fn teeth_injected_lnd_gap_is_caught() {
    let fx = load_fixture("online_sell_run");
    let (mut expected, observed) = replay(&fx).await;
    // Punch a gap: shift the last expected lnd up by one (breaks contiguity).
    let last = expected.lnds.len() - 1;
    expected.lnds[last] += 5;
    let divergences = compare(&expected, &observed);
    assert!(
        !divergences.is_empty(),
        "injected lnd gap not caught — teeth failed to bite"
    );
}

/// TEETH: an injected STATE-class flip must be caught.
#[tokio::test]
async fn teeth_injected_state_class_is_caught() {
    let fx = load_fixture("online_sell_run");
    let (mut expected, observed) = replay(&fx).await;
    expected.classes[0] = "offline_drained".to_string(); // observed is "online"
    assert!(
        !compare(&expected, &observed).is_empty(),
        "injected state-class flip not caught"
    );
}

/// TEETH: an injected CHAIN corruption (interior link nulled) must be caught.
#[tokio::test]
async fn teeth_injected_chain_break_is_caught() {
    let fx = load_fixture("online_sell_run");
    let (mut expected, observed) = replay(&fx).await;
    let mid = expected.chain.len() / 2;
    expected.chain[mid] = None; // corpus chain corrupted at an interior link
    assert!(
        !compare(&expected, &observed).is_empty(),
        "injected chain break not caught"
    );
}

/// NEGATIVE (paired with the teeth): the CLEAN online-sell fixture replays with
/// ZERO divergence — our write-path matches the corpus invariants.
#[tokio::test]
async fn online_sell_run_replays_clean() {
    let fx = load_fixture("online_sell_run");
    let (expected, observed) = replay(&fx).await;
    let d = compare(&expected, &observed);
    assert!(
        d.is_empty(),
        "online_sell_run divergence (CP3 — triage, DO NOT re-bless):\n{}",
        d.join("\n")
    );
}

/// The offline session + drain fixture: sells land `OFFLINE_LOCAL_ACK`, the
/// real drain advances them to `ACK` ("drained"), 10 codes consumed.
#[tokio::test]
async fn offline_session_drain_replays_clean() {
    let fx = load_fixture("offline_session_drain");
    let (expected, observed) = replay(&fx).await;
    let d = compare(&expected, &observed);
    assert!(
        d.is_empty(),
        "offline_session_drain divergence (CP3 — triage):\n{}",
        d.join("\n")
    );
}

/// cancel_edge: our alphabet has no Cancel op → replay-partial up to the
/// cancel-point (documented; full cancel replay awaits Phase-2 RETURN). The
/// driven online-sell prefix must replay clean.
#[tokio::test]
async fn cancel_edge_replays_partial_clean() {
    let fx = load_fixture("cancel_edge");
    let (driven, _) = driven_sells(&fx);
    assert_eq!(driven.len(), 4, "cancel_edge drives the 4 pre-cancel sells");
    let (expected, observed) = replay(&fx).await;
    let d = compare(&expected, &observed);
    assert!(
        d.is_empty(),
        "cancel_edge (partial) divergence (CP3 — triage):\n{}",
        d.join("\n")
    );
}

/// z_report is `replay:false` (Z exported-not-replayed, A2/MED#8) — the harness
/// MUST skip it EXPLICITLY, never silently.
#[tokio::test]
async fn z_report_is_exported_not_replayed() {
    let fx = load_fixture("z_report");
    assert!(!fx.replay, "z_report must carry replay:false");
    eprintln!(
        "U3 skip: z_report is exported-not-replayed \
         (Z is not in the Phase-1 replay alphabet — N3)"
    );
}

/// O3 (narrowed, N4): normalized-STRUCTURAL comparison of our canonical output
/// vs the fixture receipt structure on the sell fixtures — field-level after
/// normalization. NO byte/hash-equality vs WebCheck XML (deferred, N4).
#[tokio::test]
async fn o3_normalized_structural_on_sells() {
    for name in ["online_sell_run", "cancel_edge"] {
        let fx = load_fixture(name);
        let (driven, _) = driven_sells(&fx);
        assert!(!driven.is_empty(), "{name}: has sell ops");
        for op in driven {
            let fixture: serde_json::Value = serde_json::from_str(&op.payload_json).unwrap();
            let goods = fixture["receipt"]["goods"].as_array().unwrap();
            let pays = fixture["receipt"]["payments"].as_array().unwrap();
            let fixture_total = fixture["receipt"]["totals"]["total_sum"].as_i64().unwrap();

            let (canonical_json, total) = receipt_to_canonical(&op.payload_json);
            let cv: serde_json::Value = serde_json::from_str(&canonical_json).unwrap();
            let items = cv["items"].as_array().unwrap();
            let cpays = cv["payments"].as_array().unwrap();

            // field-level structural correspondence after normalization
            assert_eq!(
                items.len(),
                goods.len(),
                "{name} op{}: goods↔items arity",
                op.op_index
            );
            assert_eq!(
                cpays.len(),
                pays.len(),
                "{name} op{}: payments arity",
                op.op_index
            );
            let items_sum: i64 = items.iter().map(|i| i["sum_kop"].as_i64().unwrap()).sum();
            assert_eq!(
                items_sum, total,
                "{name} op{}: Σitems == total",
                op.op_index
            );
            assert_eq!(
                total, fixture_total,
                "{name} op{}: total preserved",
                op.op_index
            );
        }
    }
}
