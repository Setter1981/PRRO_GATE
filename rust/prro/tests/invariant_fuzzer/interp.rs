//! Interpreter (Task 2): execute an `Op` sequence against a LIVE SQLite DB
//! through the REAL write-path seams.
//!
//! This is the first real consumer of `ScriptedDps` + `DpsScript`.  No
//! `proptest` generator (Task 3) and no model differential (Task 4) here — just
//! drive each `Op` through its real seam and read the observed ledger back.
//!
//! Task 2 wired `OnlineSell` / `Crash(Send)` / `Reboot`.  Task 3 completes the
//! rest of the generator-reachable alphabet: `OfflineSell`, `GoOnline` (probe +
//! drain), `Drain`, `Crash(Kvt1)` (drop-injection via hang_last), and the
//! invalid / re-entry intents (run the same seam, expect refusal / no-op).  Only
//! the NON-wire `Crash` stages (stage-composition) remain deferred — and the
//! generator never emits them (Crash drawn from {Send, Kvt1}), so no
//! generator-reachable op hits `unimplemented!`.
//!
//! Fixtures (`fresh_pool`, `seed_*`) are re-created here rather than imported:
//! the kill-point matrix keeps them file-local (not in `tests/common/`), and
//! Task 2's scope is `interp.rs` only.  `ScriptedDps` + `det_signing_ctx` +
//! `drain_test_guard` ARE shared from `tests/common/`.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tokio::sync::oneshot;

use prro::db::models::enums::{
    DocState, DocType, FiscalMode, NodeMode, OfflineSessionState, Protocol, ShiftState,
};
use prro::db::models::ids::{CashierId, OfflineSessionId, RequestId, ShiftId};
use prro::db::repositories::ingress_inbox::{self as inbox, InboxRow, NewInboxEntry};
use prro::db::repositories::payment_methods::{insert as pm_insert, NewPaymentMethod};
use prro::db::repositories::tax_groups::NewTaxGroup;
use prro::db::repositories::{fiscal_documents, offline_sessions, tax_groups};
use prro::db::repositories::{fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig};
use prro::db::types::{DbOfflineSessionId, DbShiftId};
use prro::db::{open_pool, open_secure_pool};
use prro::runtime::ingress::convert::convert_to_signer_payload;
use prro::runtime::ingress::dto::CanonicalCommand;
use prro::runtime::ingress::handler::{handle_command, IngressBody};
use prro::runtime::ingress::seam::UnimplementedWritePath;
use prro::services::offline_sync::{backlog_drain, return_online_probe};
use prro::services::reconciliation::{boot_phase, online_convergence, RuntimeView};
use prro::services::write_path::inline;
use prro::services::write_path::stage_sign::SigningContext;
use prro::services::write_path::types::{CanonicalFiscalCommand, WorkerProcessResult};
use prro::services::write_path::{stage_acquire, stage_sign};
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{CheckAck, CheckSignBlob, StatusSnapshot};
use prro::transports::dps::error::{AuthorizationKind, DpsError};

use crate::common::scripted_dps::{PeerLedger, PeerMismatch, ScriptedDps};
use crate::common::{det_signing_ctx, drain_test_guard};
use crate::op::{
    DpsScript, L5Kind, Op, OperatorResolutionKind, ReplenishLeaf, Stage, WireResponse,
};

/// A `(previous_hash, unsigned_xml_sha256)` chain-hash pair as read from a
/// `fiscal_documents` row — both columns nullable.  Named to satisfy
/// `clippy::type_complexity` at the B10 boundary-chain teeth query sites.
type ChainHashPair = (Option<Vec<u8>>, Option<Vec<u8>>);

// ─── Fixture constants (mirror tests/kill_point_matrix.rs) ──────────────────

const FN: &str = "4000000001";
const CASHIER: &str = "test-cashier";
const DRIVER: &str = "drv-test";
const SERVER_FISCAL_NO: &str = "DPS-FN-ONLINE-1";
const SELL_PAYLOAD: &str = r#"{"items":[{"code":"item-1","name":"Test item","price_kop":15000,"quantity_thousandths":1000,"sum_kop":15000}],"payments":[{"name":"Cash","sum_kop":15000,"type_code":"0"}]}"#;
const TAXABLE_PAYLOAD: &str = r#"{"items":[{"code":"tax-1","name":"Taxed item","price_kop":15000,"quantity_thousandths":1000,"sum_kop":15000,"tax_group_1":1}],"payments":[{"name":"Cash","sum_kop":15000,"type_code":"0"}]}"#;
const TOTAL_KOP: i64 = 15000;
/// Live SHIFT_OPEN payload consumed by stage_sign's `ShiftOpenJson`.
const SHIFT_OPEN_PAYLOAD: &str = r#"{"opening_sum_kop":0}"#;
/// A live Z_REPORT's inbox payload is the wire intent; inline Z dispatch
/// replaces it with the aggregated body before stage_acquire/stage_sign.
const Z_WIRE_INTENT: &str = r#"{}"#;
/// L3 — service cash-in signer payload (stage_sign parses as `ServiceIoJson`).
/// Amount = CASH_AMOUNT_KOP so the cash oracle stays in sync with the model.
const SERVICE_IN_PAYLOAD: &str =
    r#"{"schema_version":"1.0","amount_kop":15000,"name":"SERVICE_IN"}"#;
/// L3 — service cash-out signer payload.  Same amount so guard-3b symmetry holds.
const SERVICE_OUT_PAYLOAD: &str =
    r#"{"schema_version":"1.0","amount_kop":15000,"name":"SERVICE_OUT"}"#;
/// EPZ — видача готівки за ЕПЗ signer payload (stage_sign parses as `EpzJson`).
/// `sum_kop = CASH_AMOUNT_KOP` so the cash oracle stays in sync with the model
/// (EPZ drives `− epz_out`).  Card leg carries a paymentid ≥ 2 + slip requisites.
const EPZ_PAYLOAD: &str = r#"{"schema_version":"1.0","sum_kop":15000,"code":"0","name":"EPZ","paymentid":2,"pay_name":"Card","pa":"M","pb":"T","pc":"P","pd":"****","pe":"A","psnm":"Visa","rrn":"R"}"#;

// ─── Observed result (read back from the ledger after each op) ──────────────

/// Peer-tip axis PHASE C — what the REAL MAC tip is, structurally.  See
/// [`FuzzCtx::real_tip_class`]; the model's own symbolic tip projects onto the
/// same three cases, and the harness asserts the two agree after every op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealTipClass {
    /// No tip at all — the FN has never issued (`last_known_unsigned_xml_sha256`
    /// is NULL).
    Genesis,
    /// The tip is the `unsigned_xml_sha256` of the document at this `lnd`.
    Doc(i64),
    /// The tip is a value no `fiscal_documents` row carries: a T=112
    /// `sha256(request_xml)`, a MacReseed rebase, a peer-declared `store`.
    NonDoc,
}

/// The observed ledger effect of one op — exactly the fields the Task 4
/// differential will compare with `RefModel::Mutation` (lnd / doc_state /
/// previous_hash / seed) plus the offline code count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedDoc {
    pub lnd: i64,
    pub doc_state: DocState,
    pub previous_hash: Option<Vec<u8>>,
    /// `node_state.last_known_unsigned_xml_sha256` after the op (the MAC tip).
    pub seed_after: Option<Vec<u8>>,
    /// Count of consumed offline codes (None when zero — online ops).
    pub code_consumed: Option<i64>,
    /// Real `node_state.shift_state` after the op.  Most receipt ops leave this
    /// unchecked (`Mutation::shift_state_after = None`), but shift/Z ops pin it.
    pub shift_state_after: ShiftState,
}

/// CS-3 Slice E — the REAL durable delivery-axis witness read back from `delivery_reservation` +
/// `node_state` for a HELD online doc.  Compared against the model's independent
/// [`crate::model::HeldWitness`] by [`crate::oracle::check_held_witness`].  DB-TEXT values verbatim
/// (no decode), so a persisted-classifier regression surfaces as a plain string mismatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedHeld {
    pub submission_certainty: String,
    pub response_provenance: String,
    pub routing_class: Option<String>,
    pub node_effect: String,
    pub evidence_kind: String,
    pub evidence_code: Option<i64>,
    pub apply_state: String,
    pub node_mode: String,
    pub fence_held: bool,
}

/// CS-3 operator-completion — the REAL durable witness read back AFTER a legal operator completion
/// released a HELD reservation.  Compared against the model's independent
/// [`crate::model::ReleasedWitness`] by [`crate::oracle::check_release_witness`].  DB-TEXT verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedRelease {
    pub apply_state: String,
    pub node_mode: String,
    pub fence_held: bool,
    pub doc_state: String,
}

/// What `run_op` observed for one op.
#[derive(Debug, Clone)]
pub enum RealOutcome {
    /// A sell produced a ledger doc, read back from the DB.
    Doc(ObservedDoc),
    /// A crash op: the future was dropped mid-stage; the durably-committed
    /// transient is read back (e.g. `SENDING`).  No `FiscalOutcome` returned.
    Crashed {
        stage: Stage,
        committed_state: Option<DocState>,
    },
    /// A reboot/recovery op completed; carries the recovery branch debug string.
    /// The recovered ledger is read separately via `FuzzCtx` accessors.
    Recovered { branch: String },
    /// The seam returned a typed refusal / error (no issued doc).
    Refused(String),
    /// L6 — an X-report (поточний звіт) read completed: a SIDE-EFFECT-FREE
    /// snapshot.  Carries the observed turnover snapshot so the harness can
    /// assert it matches the model (`cash_on_hand`).  A `NoMutation` outcome for
    /// the differential (no doc, no lnd, no seed, no code) — the turnover
    /// equality is the harness's extra assertion.
    XReport {
        cash_on_hand_kop: i64,
        turnover_json: String,
    },
    /// CS-3 operator-completion — a legal `resolve_operator_pending` released a HELD reservation;
    /// carries the durable witness read back (`APPLIED` / fence-clear / node un-halted / doc
    /// terminal).  The anti-BRICK oracle asserts this against the model's `ReleasedWitness`.
    Released(ObservedRelease),
    /// bd `PRRO_GATE-hpc` / `PRRO_GATE-2ds` — a T=112 replenish COMPLETED: DPS granted a code window,
    /// prod persisted the codes (`INSERT OR IGNORE`), advanced the chain seed to
    /// `sha256(request_xml)` — a **non-document** seed — and appended the durable witness row
    /// (migration 040), all in ONE `with_immediate` envelope.  This is the only outcome that mutates
    /// the ledger WITHOUT minting a document and WITHOUT allocating an `lnd`, which is precisely what
    /// makes the witness's `lnd_at_write` ordering frame meaningful.  A server-side refusal returns
    /// [`RealOutcome::Refused`] instead (nothing persisted).
    Replenished {
        /// Codes actually inserted (duplicates are deduped by the partial unique index).
        inserted: u64,
        /// Codes the insert deduped away.
        deduped: u64,
        /// The non-document seed the replenish installed (`sha256(request_xml)`).
        new_seed: Vec<u8>,
    },
}

// ─── Race state threaded through `run_op` ───────────────────────────────────

/// Per-`fiscal_number` interpreter state.  Holds the live pools, the signing
/// context, the per-FN single-writer gate, and the `Arc<AtomicUsize>` wire
/// counters shared across every `ScriptedDps` this run mints — so "exactly one
/// send_chk across a crash + reboot" is counted THROUGH the simulated restart
/// (the kill-point discipline).
pub struct FuzzCtx {
    pub pool: SqlitePool,
    pub pool_secure: SqlitePool,
    /// RAII guards for the two per-case temp-DB directories. Declared **after**
    /// the pools so Rust's declaration-order drop closes the pools first, then
    /// these remove the directories — cleanup never races a live connection.
    /// Held only for their `Drop`; never read.
    _tempdir: tempfile::TempDir,
    /// bd PRRO_GATE-2ds/hpc — the fixture now owns a REAL `App` so the interpreter can drive
    /// production services that need one (the T=112 `OfflineCodeReplenishService` takes an `App`
    /// for the per-FN write gate + pool).  `pool` / `pool_secure` are clones of `app.db()` /
    /// `app.db_secure()`, so every existing op still runs against the SAME database.
    /// `App::boot` spawns NO background tasks (the loops live in `runtime::supervisor::run`, started
    /// only by `serve`), so fuzzer determinism is unaffected.
    pub app: prro::App,
    sign_ctx: SigningContext,
    fn_sign: CheckSignBlob,
    /// FIDELITY WRINKLE, documented rather than hidden (design §2): this is the gate `inline::run`
    /// takes, and it is a DIFFERENT mutex from the one `app.acquire_fn_gate` takes.  Unifying them is
    /// not possible through the public API — `App` exposes only `acquire_fn_gate() -> OwnedMutexGuard`,
    /// never the `Arc<Mutex<_>>` itself (`app.rs:402`), and `Inner` is private.  This is SOUND here
    /// because the harness drives ops strictly sequentially (one op fully completes before the next
    /// starts), so no two writers are ever in flight and the two locks are never contended.  It does
    /// mean the fuzzer does NOT exercise invariant #2 (one FN = one writer) as a concurrency property —
    /// that stays the job of the dedicated concurrency tests, not this harness.
    gate: Arc<tokio::sync::Mutex<()>>,
    fn_id: String,
    send_calls: Arc<AtomicUsize>,
    last_calls: Arc<AtomicUsize>,
    seq: u64,
    /// The last successfully-issued inbox row — replayed (idempotent no-op) by
    /// `DuplicateIdemKey` so a replay mints no NEW doc.
    last_row: Option<InboxRow>,
    /// Peer-tip axis PHASE A (spec `2026-07-31-spec-fuzzer-peer-tip-axis.md`):
    /// the harness's model of the DPS peer's chain tip.  Observes every wire
    /// send and records a mismatch whenever an outgoing document's
    /// `previous_hash` disagrees with the peer BEFORE any divergence-creating
    /// event.  Phase A changes NO reply — it exists to load-test the movers
    /// table, which is the part of the design that has to be right before an
    /// override or a model mirror is built on it.
    peer: Arc<PeerLedger>,
}

impl FuzzCtx {
    /// Fixture: a fresh DB with an ONLINE node + open shift.
    /// Return the fiscal number for this ctx (used by drive_sequence to call
    /// `check_cash_on_hand` after each op).
    pub fn fn_id(&self) -> &str {
        &self.fn_id
    }

    pub async fn new_online_open_shift() -> Self {
        let (app, _tempdir) = boot_fuzz_app(None).await;
        let pool = app.db().clone();
        let pool_secure = app.db_secure().clone();
        let peer_pool = pool.clone();
        seed_fn_config(&pool).await;
        let shift_id = seed_open_shift(&pool).await;
        seed_node_state(&pool, NodeMode::Online, shift_id).await;
        Self {
            pool,
            pool_secure,
            _tempdir,
            app,
            sign_ctx: det_signing_ctx(),
            fn_sign: fn_sign_blob(),
            gate: Arc::new(tokio::sync::Mutex::new(())),
            fn_id: FN.to_string(),
            send_calls: Arc::new(AtomicUsize::new(0)),
            last_calls: Arc::new(AtomicUsize::new(0)),
            seq: 0,
            last_row: None,
            peer: PeerLedger::new(peer_pool, FN.to_string()),
        }
    }

    /// Fixture variant used by the cleanup test: keep all DB tempdirs under a
    /// caller-owned base dir without mutating the process-global `TMPDIR`.
    async fn new_online_open_shift_in(base: &Path) -> Self {
        let (app, _tempdir) = boot_fuzz_app(Some(base)).await;
        let pool = app.db().clone();
        let pool_secure = app.db_secure().clone();
        let peer_pool = pool.clone();
        seed_fn_config(&pool).await;
        let shift_id = seed_open_shift(&pool).await;
        seed_node_state(&pool, NodeMode::Online, shift_id).await;
        Self {
            pool,
            pool_secure,
            _tempdir,
            app,
            sign_ctx: det_signing_ctx(),
            fn_sign: fn_sign_blob(),
            gate: Arc::new(tokio::sync::Mutex::new(())),
            fn_id: FN.to_string(),
            send_calls: Arc::new(AtomicUsize::new(0)),
            last_calls: Arc::new(AtomicUsize::new(0)),
            seq: 0,
            last_row: None,
            peer: PeerLedger::new(peer_pool, FN.to_string()),
        }
    }

    /// Fixture: a fresh DB with an ONLINE node and no open/current shift.
    /// `SHIFT_OPEN` should create and open the shift through stage_acquire.
    pub async fn new_online_closed_shift() -> Self {
        let (app, _tempdir) = boot_fuzz_app(None).await;
        let pool = app.db().clone();
        let pool_secure = app.db_secure().clone();
        let peer_pool = pool.clone();
        seed_fn_config(&pool).await;
        seed_node_state_with_shift(&pool, NodeMode::Online, ShiftState::Closed, None).await;
        Self {
            pool,
            pool_secure,
            _tempdir,
            app,
            sign_ctx: det_signing_ctx(),
            fn_sign: fn_sign_blob(),
            gate: Arc::new(tokio::sync::Mutex::new(())),
            fn_id: FN.to_string(),
            send_calls: Arc::new(AtomicUsize::new(0)),
            last_calls: Arc::new(AtomicUsize::new(0)),
            seq: 0,
            last_row: None,
            peer: PeerLedger::new(peer_pool, FN.to_string()),
        }
    }

    /// Fixture: a fresh DB with an OFFLINE node + open shift + an OPEN offline
    /// session carrying `codes` offline codes (the offline lane is fixture-
    /// seeded — there is no go_offline op, spec §5).
    pub async fn new_offline_open_shift(codes: i64) -> Self {
        let (app, _tempdir) = boot_fuzz_app(None).await;
        let pool = app.db().clone();
        let pool_secure = app.db_secure().clone();
        let peer_pool = pool.clone();
        seed_fn_config(&pool).await;
        let shift_id = seed_open_shift(&pool).await;
        seed_node_state(&pool, NodeMode::Offline, shift_id).await;
        seed_open_offline_session(&pool).await;
        for code_lnd in 1..=codes {
            seed_offline_code(&pool, code_lnd).await;
        }
        Self {
            pool,
            pool_secure,
            _tempdir,
            app,
            sign_ctx: det_signing_ctx(),
            fn_sign: fn_sign_blob(),
            gate: Arc::new(tokio::sync::Mutex::new(())),
            fn_id: FN.to_string(),
            send_calls: Arc::new(AtomicUsize::new(0)),
            last_calls: Arc::new(AtomicUsize::new(0)),
            seq: 0,
            last_row: None,
            peer: PeerLedger::new(peer_pool, FN.to_string()),
        }
    }

    /// Fixture: a fresh DB with an OFFLINE node, no open/current shift, and an
    /// OPEN offline session carrying `codes`.  `SHIFT_OPEN` local-acks.
    pub async fn new_offline_closed_shift(codes: i64) -> Self {
        let (app, _tempdir) = boot_fuzz_app(None).await;
        let pool = app.db().clone();
        let pool_secure = app.db_secure().clone();
        let peer_pool = pool.clone();
        seed_fn_config(&pool).await;
        seed_node_state_with_shift(&pool, NodeMode::Offline, ShiftState::Closed, None).await;
        seed_open_offline_session(&pool).await;
        for code_lnd in 1..=codes {
            seed_offline_code(&pool, code_lnd).await;
        }
        Self {
            pool,
            pool_secure,
            _tempdir,
            app,
            sign_ctx: det_signing_ctx(),
            fn_sign: fn_sign_blob(),
            gate: Arc::new(tokio::sync::Mutex::new(())),
            fn_id: FN.to_string(),
            send_calls: Arc::new(AtomicUsize::new(0)),
            last_calls: Arc::new(AtomicUsize::new(0)),
            seq: 0,
            last_row: None,
            peer: PeerLedger::new(peer_pool, FN.to_string()),
        }
    }

    /// Fixture-level setter: force the node mode (used by test setup and by the
    /// deliberately-adverse `OfflineSellDuringGoingOnline` intent).
    pub async fn force_node_mode(&self, mode: NodeMode) {
        sqlx::query("UPDATE node_state SET mode = ? WHERE fiscal_number = ?")
            .bind(mode.as_str())
            .bind(self.fn_id.as_str())
            .execute(&self.pool)
            .await
            .unwrap();
    }

    /// Fixture-level setter: close the shift (both `shifts.state` and the
    /// `node_state.shift_state` mirror) — realizes the `SellWithClosedShift`
    /// adverse precondition.
    async fn force_shift_closed(&self) {
        sqlx::query("UPDATE shifts SET state = 'CLOSED' WHERE fiscal_number = ?")
            .bind(self.fn_id.as_str())
            .execute(&self.pool)
            .await
            .unwrap();
        sqlx::query("UPDATE node_state SET shift_state = 'CLOSED' WHERE fiscal_number = ?")
            .bind(self.fn_id.as_str())
            .execute(&self.pool)
            .await
            .unwrap();
    }

    /// Test corruption (Mirror-2): repoint the FN's offline drain-cohort doc(s)
    /// at a fresh FOREIGN (CLOSED) session — a non-null but stale session id
    /// that invariant_scan's check-6d (NULL-only) does NOT catch, so it isolates
    /// the Mirror-2 mismatch predicate.
    pub async fn corrupt_cohort_session_to_foreign(&self) {
        let foreign = OfflineSessionId::new();
        sqlx::query(
            "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
             VALUES (?, ?, 'CLOSED', '2026-06-08T00:00:00Z')",
        )
        .bind(DbOfflineSessionId(foreign))
        .bind(self.fn_id.as_str())
        .execute(&self.pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE fiscal_documents SET offline_session_id = ? \
             WHERE fiscal_number = ? AND offline_fiscal_no IS NOT NULL",
        )
        .bind(DbOfflineSessionId(foreign))
        .bind(self.fn_id.as_str())
        .execute(&self.pool)
        .await
        .unwrap();
    }

    /// Adversarial-audit canary support: repoint the FN fence to a FOREIGN reservation id — a
    /// non-NULL pointer that does NOT name the held reservation (an open P3 / forked fence). A
    /// presence-only `fence_held = active_delivery_reservation_id IS NOT NULL` check would
    /// false-green; the authority predicate must compute `fence_held = false` and RED.
    pub async fn corrupt_active_fence_to_foreign(&self) {
        sqlx::query(
            "UPDATE node_state SET active_delivery_reservation_id = ? WHERE fiscal_number = ?",
        )
        .bind(&[0xA5u8; 16][..])
        .bind(self.fn_id.as_str())
        .execute(&self.pool)
        .await
        .unwrap();
    }

    /// Adversarial-audit canary support: advance `node_state.delivery_generation` past the held
    /// reservation's `authorized_generation` (a monotonic +1 — the schema permits an increase). The
    /// fence pointer still names the reservation, but at a STALE generation → the authority predicate
    /// must compute `fence_held = false` and RED (an ABA-style generation drift).
    pub async fn bump_delivery_generation(&self) {
        sqlx::query(
            "UPDATE node_state SET delivery_generation = delivery_generation + 1 \
             WHERE fiscal_number = ?",
        )
        .bind(self.fn_id.as_str())
        .execute(&self.pool)
        .await
        .unwrap();
    }

    /// Test corruption (O3): overwrite an ACK doc's stored `unsigned_xml_sha256`
    /// with a value that no longer matches its persisted `PAYLOAD_XML` — a
    /// stored-hash / stored-payload divergence the REFERENTIAL chain oracle
    /// (which trusts the stored hash) is blind to.
    pub async fn corrupt_stored_unsigned_hash(&self) {
        sqlx::query(
            "UPDATE fiscal_documents SET unsigned_xml_sha256 = ? \
             WHERE fiscal_number = ? AND state = 'ACK'",
        )
        .bind(vec![0u8; 32])
        .bind(self.fn_id.as_str())
        .execute(&self.pool)
        .await
        .unwrap();
    }

    /// Test corruption (O5): drop an ACK doc's `server_fiscal_no` → an
    /// `AckWithoutServerFiscalNo` scan violation (a NON-`StuckSending` breach the
    /// `ArtifactNoResend` filter must keep FATAL).  `ACK` is terminal, so boot
    /// reconciliation never touches it — the planted violation survives the
    /// settle loop's reboots, unlike a non-terminal doc which a reboot may
    /// resolve.
    pub async fn corrupt_ack_drop_server_fiscal_no(&self) {
        sqlx::query(
            "UPDATE fiscal_documents SET server_fiscal_no = NULL \
             WHERE fiscal_number = ? AND state = 'ACK'",
        )
        .bind(self.fn_id.as_str())
        .execute(&self.pool)
        .await
        .unwrap();
    }

    /// Test corruption (X2): simulate a LOST `ux_offline_active` partial-unique
    /// index (a schema regression) and plant a SECOND active `OPEN` session — the
    /// multi-active-session state the DB normally PREVENTS.  Today the index
    /// `ux_offline_active ON offline_sessions(fiscal_number) WHERE state IN
    /// ('OPENING','OPEN','DRAINING')` makes two active sessions unreachable (the
    /// `check_mirrors` / `adopt_precondition` `OPEN/DRAINING` filter is
    /// a subset), so the X2 `ORDER BY` + count guard is a DEFENSE-IN-DEPTH
    /// regression sentinel: this drops the index to construct the breach the
    /// guard is meant to catch if the schema protection is ever weakened.
    pub async fn plant_second_active_session_dropping_guard_index(&self) {
        sqlx::query("DROP INDEX IF EXISTS ux_offline_active")
            .execute(&self.pool)
            .await
            .unwrap();
        let extra = OfflineSessionId::new();
        sqlx::query(
            "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
             VALUES (?, ?, 'OPEN', '2026-06-09T00:00:00Z')",
        )
        .bind(DbOfflineSessionId(extra))
        .bind(self.fn_id.as_str())
        .execute(&self.pool)
        .await
        .unwrap();
    }

    fn view<'a>(&'a self, dps: &'a dyn DpsChannel) -> RuntimeView<'a> {
        RuntimeView {
            dps,
            signing_ctx: &self.sign_ctx,
            fn_sign: &self.fn_sign,
        }
    }

    fn next_idem(&mut self) -> String {
        self.seq += 1;
        format!("idem-fuzz-{}", self.seq)
    }

    /// send_chk count across the whole run (shared through restarts).
    pub fn send_calls(&self) -> usize {
        self.send_calls.load(Ordering::SeqCst)
    }

    /// last_chk count across the whole run.
    pub fn last_calls(&self) -> usize {
        self.last_calls.load(Ordering::SeqCst)
    }

    fn new_dps(&self) -> ScriptedDps {
        ScriptedDps::new(Arc::clone(&self.send_calls), Arc::clone(&self.last_calls))
            .with_peer(Arc::clone(&self.peer))
    }

    // ─── Peer-tip axis phase A: the harness surface ─────────────────────
    //
    // Read by `drive_sequence` after every op.  A mismatch on a run that never
    // diverged means the movers table (spec §4) is wrong — that is the whole
    // point of phase A, and it fires before any override exists to mask it.

    /// Wire sends where the outgoing document's chain link disagreed with the
    /// peer while the run was still agreeing.  MUST stay empty.
    pub fn peer_mismatches(&self) -> Vec<PeerMismatch> {
        self.peer.mismatches()
    }

    /// `Some(reason)` once a legitimate divergence-creating event has happened.
    pub fn peer_diverged(&self) -> Option<String> {
        self.peer.diverged()
    }

    /// Sends the peer could not attribute to a `SENDING` row.  Diagnostic: a
    /// growing count means the resolver is blind somewhere (a doc kind that does
    /// not rest in `SENDING` at wire time), which would make the assertion
    /// vacuous rather than wrong — so the harness asserts on it too.
    pub fn peer_unresolved_sends(&self) -> usize {
        self.peer.unresolved_sends()
    }

    /// The peer's current tip, for diagnostics in a failure message.
    pub fn peer_tip_hex(&self) -> Option<String> {
        self.peer.tip_hex()
    }

    /// Mark a NO-WIRE event that legitimately moves our seed without the peer
    /// seeing anything (operator completions), or whose delivery is unknowable
    /// (a crash parked inside the wire call).  After this the peer stops
    /// asserting — phases C/D replace it with a modelled peer truth.
    /// PHASE B opt-in — let the peer answer a mismatched send with a derived
    /// `-12` instead of whatever the script queued.
    ///
    /// DIRECTED PINS ONLY. Generative runs must not enable it until the model
    /// mirrors the peer (phase C): the model predicts wire outcomes on its own,
    /// and phase A marks a run diverged on every `OperatorComplete`, on held
    /// replies and on crashes, so an always-on override would answer `-12` where
    /// the model expects an `Ack` and redden the differential everywhere.
    pub fn peer_enable_derived_rejects(&self) {
        self.peer.enable_derived_rejects();
    }

    pub fn peer_mark_diverged(&self, reason: &str) {
        self.peer.mark_diverged(reason);
    }

    /// Peer-tip axis: a granted T=112 CONVERGES both sides onto the same fresh
    /// non-document seed.  Reported by the interpreter because the replenish
    /// rides `ask_offline_codes`, which the send-side observer never sees.
    pub fn peer_converge_to(&self, tip: Option<Vec<u8>>) {
        self.peer.converge_to(tip);
    }

    /// Offline documents that are locally issued but NOT yet delivered — the
    /// backlog a drain still owes DPS.  `bd PRRO_GATE-knk` turns on whether this
    /// is zero when a T=112 moves the chain.
    pub async fn undrained_offline_backlog(&self) -> i64 {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM fiscal_documents \
             WHERE fiscal_number = ? AND fs_mode = 'OFFLINE' AND state = 'OFFLINE_LOCAL_ACK'",
        )
        .bind(self.fn_id.as_str())
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0)
    }

    async fn seed_inbox_sell(&mut self) -> InboxRow {
        let idem = self.next_idem();
        seed_inbox_keyed(&self.pool, &idem, "SELL").await
    }

    /// PR-R-fuzz — seed a `RETURN` inbox row (the shared converted CheckJson
    /// body; the direction is carried by `operation_type` → `DocType::Return`,
    /// not the payload — same shape as a SELL row).
    async fn seed_inbox_return(&mut self) -> InboxRow {
        let idem = self.next_idem();
        seed_inbox_keyed(&self.pool, &idem, "RETURN").await
    }

    async fn seed_inbox_taxable(&mut self, operation_type: &str) -> InboxRow {
        let idem = self.next_idem();
        seed_inbox_keyed_payload(
            &self.pool,
            &idem,
            operation_type,
            TAXABLE_PAYLOAD,
            Some(TOTAL_KOP),
        )
        .await
    }

    /// Test-only tax fixture for Z aggregation: tax group 1 is 20% VAT-included
    /// and maps identity from the driver payload into the canonical snapshot.
    pub async fn seed_tax_group_20_percent(&self) {
        tax_groups::insert(
            &self.pool_secure,
            &NewTaxGroup {
                fn_id: FN.to_string(),
                tx_num: 1,
                letter: "A".to_string(),
                dtpr: 0.0,
                txpr: 20.0,
                txal: 0,
                txty: 0,
            },
        )
        .await
        .expect("seed tax group 1");
    }

    pub async fn run_taxable_online_sell(&mut self, script: &DpsScript) -> RealOutcome {
        let row = self.seed_inbox_taxable("SELL").await;
        run_inline_row(self, row, Some(script)).await
    }

    pub async fn run_taxable_online_return(&mut self, script: &DpsScript) -> RealOutcome {
        let row = self.seed_inbox_taxable("RETURN").await;
        run_inline_row(self, row, Some(script)).await
    }

    pub async fn run_taxable_offline_sell(&mut self) -> RealOutcome {
        let row = self.seed_inbox_taxable("SELL").await;
        run_inline_row(self, row, None).await
    }

    pub async fn run_taxable_offline_return(&mut self) -> RealOutcome {
        let row = self.seed_inbox_taxable("RETURN").await;
        run_inline_row(self, row, None).await
    }

    /// Seed a live `SHIFT_OPEN` inbox row (opening payload, no total).
    async fn seed_inbox_shift_open(&mut self) -> InboxRow {
        let idem = self.next_idem();
        seed_inbox_keyed_payload(&self.pool, &idem, "SHIFT_OPEN", SHIFT_OPEN_PAYLOAD, None).await
    }

    /// Seed a live `Z_REPORT` inbox row (wire intent, no total).  The write path
    /// aggregates the shift ledger into the canonical Z payload internally.
    async fn seed_inbox_z_report(&mut self) -> InboxRow {
        let idem = self.next_idem();
        seed_inbox_keyed_payload(&self.pool, &idem, "Z_REPORT", Z_WIRE_INTENT, None).await
    }

    /// L3 — seed a `SERVICE_IN` inbox row.  The payload is the already-converted
    /// signer format (`stage_sign::parse_payload` expects `ServiceIoJson`).
    /// Uses `CASH_AMOUNT_KOP` (= `TOTAL_KOP`) so the cash oracle stays in sync.
    async fn seed_inbox_service_in(&mut self) -> InboxRow {
        let idem = self.next_idem();
        seed_inbox_keyed_payload(
            &self.pool,
            &idem,
            "SERVICE_IN",
            SERVICE_IN_PAYLOAD,
            None, // no total_sum_kop for service-io (not a SELL/RETURN)
        )
        .await
    }

    /// L3 — seed a `SERVICE_OUT` inbox row.  Same shape as `SERVICE_IN` with
    /// `name = "SERVICE_OUT"`.
    async fn seed_inbox_service_out(&mut self) -> InboxRow {
        let idem = self.next_idem();
        seed_inbox_keyed_payload(&self.pool, &idem, "SERVICE_OUT", SERVICE_OUT_PAYLOAD, None).await
    }

    /// EPZ — seed a `CASH_ADVANCE_EPZ` inbox row (already-converted signer
    /// format; stage_sign parses `EpzJson`).  `sum_kop = CASH_AMOUNT_KOP`.
    async fn seed_inbox_epz(&mut self) -> InboxRow {
        let idem = self.next_idem();
        seed_inbox_keyed_payload(&self.pool, &idem, "CASH_ADVANCE_EPZ", EPZ_PAYLOAD, None).await
    }

    pub async fn observed_doc_count(&self) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ?")
            .bind(self.fn_id.as_str())
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    /// State of the single doc on the FN (panics if not exactly one).
    pub async fn only_doc_state(&self) -> DocState {
        let s: String =
            sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE fiscal_number = ?")
                .bind(self.fn_id.as_str())
                .fetch_one(&self.pool)
                .await
                .unwrap();
        doc_state_from_str(&s)
    }

    /// The raw `doc_type` column of the sole doc on the FN — `fetch_one` errors
    /// if there is no row (and takes the first if there were several); the
    /// single-op pins that call this leave exactly one row.  The chain
    /// differential cannot distinguish a SELL from a RETURN (chain-identical),
    /// so PR-R-fuzz pins the wire doc-type directly here (raw string — no typed
    /// decode needed for a `"SELL"`/`"RETURN"` pin).
    pub async fn only_doc_type(&self) -> String {
        sqlx::query_scalar("SELECT doc_type FROM fiscal_documents WHERE fiscal_number = ?")
            .bind(self.fn_id.as_str())
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    /// B10 — count of rows of a given `doc_type` on the FN.  Replaces
    /// `only_doc_type` for offline assertions where the lazy BEGIN adds a second
    /// row (so a single-row `fetch_one` would panic).
    pub async fn count_doc_type(&self, doc_type: &str) -> i64 {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ? AND doc_type = ?",
        )
        .bind(self.fn_id.as_str())
        .bind(doc_type)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    async fn observe_doc_by_request_id(&self, request_id: &[u8; 16]) -> ObservedDoc {
        let (lnd, state, previous_hash): (i64, String, Option<Vec<u8>>) = sqlx::query_as(
            "SELECT lnd, state, previous_hash FROM fiscal_documents \
             WHERE fiscal_number = ? AND request_id = ?",
        )
        .bind(self.fn_id.as_str())
        .bind(&request_id[..])
        .fetch_one(&self.pool)
        .await
        .unwrap();
        ObservedDoc {
            lnd,
            doc_state: doc_state_from_str(&state),
            previous_hash,
            seed_after: self.read_seed().await,
            code_consumed: self.read_codes_consumed().await,
            shift_state_after: self.read_shift_state().await,
        }
    }

    async fn observe_doc_state_by_request_id(&self, request_id: &[u8; 16]) -> Option<DocState> {
        let state: Option<String> = sqlx::query_scalar(
            "SELECT state FROM fiscal_documents WHERE fiscal_number = ? AND request_id = ?",
        )
        .bind(self.fn_id.as_str())
        .bind(&request_id[..])
        .fetch_optional(&self.pool)
        .await
        .unwrap();
        state.map(|s| doc_state_from_str(&s))
    }

    /// The MAC tip (`node_state.last_known_unsigned_xml_sha256`) — the real
    /// seed.  Public for the Task 4 differential's structural seed comparison.
    pub async fn read_seed(&self) -> Option<Vec<u8>> {
        let v: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number = ?",
        )
        .bind(self.fn_id.as_str())
        .fetch_one(&self.pool)
        .await
        .unwrap();
        v
    }

    /// Peer-tip axis PHASE C (spec §8) — the real MAC tip PROJECTED onto the
    /// model's symbolic algebra: which document, if any, the tip currently rests
    /// on.
    ///
    /// The model cannot hold real hashes (it builds no XML), so it carries
    /// `synth_unsigned_hash(lnd)` placeholders and the differential compares
    /// STRUCTURALLY.  Phase C needs that comparison to become a real assertion
    /// rather than a convention: `Doc(lnd)` when the tip is some document's
    /// `unsigned_xml_sha256`, `NonDoc` when it is a value no row carries (a
    /// T=112 `sha256(request_xml)`, a MacReseed rebase, a peer-declared `store`),
    /// `Genesis` when there is no tip at all.
    ///
    /// `ORDER BY lnd DESC` is deliberate and shared with
    /// `RefModel::adopt_fault_deferred`'s own lookup — the two must agree on
    /// which document owns a tip, or a fault re-sync would land the model on an
    /// ordinal this projection then calls wrong.
    pub async fn real_tip_class(&self) -> RealTipClass {
        let Some(seed) = self.read_seed().await else {
            return RealTipClass::Genesis;
        };
        let lnd: Option<i64> = sqlx::query_scalar(
            "SELECT lnd FROM fiscal_documents \
             WHERE fiscal_number = ? AND unsigned_xml_sha256 = ? \
             ORDER BY lnd DESC LIMIT 1",
        )
        .bind(self.fn_id.as_str())
        .bind(&seed[..])
        .fetch_optional(&self.pool)
        .await
        .unwrap();
        match lnd {
            Some(lnd) => RealTipClass::Doc(lnd),
            None => RealTipClass::NonDoc,
        }
    }

    /// The bounded MAC-recovery counter for the doc at `lnd`.
    ///
    /// `run_mac_recovery` claims it 0→1 before re-signing, and the claim is what
    /// bounds recovery to ONE attempt. Reading it is how a test distinguishes
    /// "recovery ran and succeeded" from "the doc happened to end up ACK" — the
    /// two are indistinguishable from the doc state alone, which is exactly why
    /// the `-12` path could stay uncovered for so long.
    pub async fn read_mac_recovery_attempts(&self, lnd: i64) -> Option<i64> {
        sqlx::query_scalar(
            "SELECT mac_recovery_attempts FROM fiscal_documents \
             WHERE fiscal_number = ? AND lnd = ?",
        )
        .bind(self.fn_id.as_str())
        .bind(lnd)
        .fetch_optional(&self.pool)
        .await
        .unwrap()
    }

    /// CS-3 (C-iii) — the FN's docs as `(lnd, state)` ordered by lnd, for asserting the durable
    /// OLA-cohort effects of a `NotAcceptedOffline` completion (later `OFFLINE_LOCAL_ACK` successors
    /// → `CANCELLED`; the held doc → `RMR`).
    pub async fn read_doc_states_by_lnd(&self) -> Vec<(i64, String)> {
        sqlx::query_as(
            "SELECT lnd, state FROM fiscal_documents WHERE fiscal_number = ? ORDER BY lnd ASC",
        )
        .bind(self.fn_id.as_str())
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }

    /// CS-3 (C-iii) — a doc's immutable `previous_hash` (the chain tip it chained onto at issuance),
    /// keyed by `request_id`. A `NotAcceptedOffline` completion rewinds `node_state`'s seed to the
    /// held doc's own `previous_hash` (`Some(prev)` → predecessor tip, `None` → genesis).
    pub async fn read_previous_hash(&self, request_id: &[u8; 16]) -> Option<Vec<u8>> {
        sqlx::query_scalar(
            "SELECT previous_hash FROM fiscal_documents WHERE fiscal_number = ? AND request_id = ?",
        )
        .bind(self.fn_id.as_str())
        .bind(&request_id[..])
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    /// CS-3 (C-iii) test setup — force a doc (by lnd) into an ISSUED state, realizing the fork-guard
    /// precondition: a `NotAcceptedOffline` completion on an earlier held doc must REFUSE (fork guard —
    /// a later ISSUED successor cannot be rewound away) rather than cancel it.
    pub async fn force_doc_state_by_lnd(&self, lnd: i64, state: &str) {
        sqlx::query("UPDATE fiscal_documents SET state = ? WHERE fiscal_number = ? AND lnd = ?")
            .bind(state)
            .bind(self.fn_id.as_str())
            .bind(lnd)
            .execute(&self.pool)
            .await
            .unwrap();
    }

    /// CS-3 MacReseed (task #18 (B)) — the FN's last-issued chain tip, the value guard B validates
    /// the operator's `-12` MacReseed seed against (`fiscal_documents::last_issued_unsigned_xml_sha256`,
    /// the SHARED `is_issued` projection `invariant_scan` walks to). A directed MacReseed VALID-path
    /// test supplies THIS as the operator seed (in reality the operator supplies the DPS-assigned tip).
    /// `None` if no doc has issued yet.
    pub async fn last_issued_tip(&self) -> Option<[u8; 32]> {
        prro::db::repositories::fiscal_documents::last_issued_unsigned_xml_sha256(
            &self.pool,
            self.fn_id.as_str(),
        )
        .await
        .unwrap()
        .map(|v| <[u8; 32]>::try_from(v.as_slice()).expect("chain tip is 32 bytes"))
    }

    /// CS-3 Slice E — the REAL durable delivery-axis witness for a doc: the latest
    /// `delivery_reservation` attempt (joined by `request_id`) + the node's `mode` and FN fence
    /// pointer.  `None` ⇒ the doc has NO reservation row (a pre-send / invalid-ingress refusal never
    /// reserves), so the held-witness oracle has nothing to assert.  Reads DB-text verbatim.
    pub async fn read_held_witness(&self, request_id: &[u8; 16]) -> Option<ObservedHeld> {
        #[allow(clippy::type_complexity)]
        let row: Option<(
            String,
            String,
            Option<String>,
            String,
            String,
            Option<i64>,
            String,
            String,
            Vec<u8>,
            Option<i64>,
        )> = sqlx::query_as(
            "SELECT dr.submission_certainty, dr.response_provenance, dr.routing_class, \
                        dr.node_effect, dr.evidence_kind, dr.evidence_code, dr.apply_state, \
                        dr.state, dr.reservation_id, dr.authorized_generation \
                 FROM delivery_reservation dr \
                 JOIN fiscal_documents fd \
                   ON fd.document_id = dr.document_id AND fd.fiscal_number = dr.fiscal_number \
                 WHERE fd.fiscal_number = ? AND fd.request_id = ? \
                 ORDER BY dr.attempt_no DESC LIMIT 1",
        )
        .bind(self.fn_id.as_str())
        .bind(&request_id[..])
        .fetch_optional(&self.pool)
        .await
        .unwrap();
        let (
            certainty,
            provenance,
            routing,
            node_effect,
            evidence_kind,
            evidence_code,
            apply_state,
            res_state,
            reservation_id,
            authorized_generation,
        ) = row?;
        let (mode, active_ptr, delivery_generation): (String, Option<Vec<u8>>, i64) =
            sqlx::query_as(
                "SELECT mode, active_delivery_reservation_id, delivery_generation \
                 FROM node_state WHERE fiscal_number = ?",
            )
            .bind(self.fn_id.as_str())
            .fetch_one(&self.pool)
            .await
            .unwrap();
        // `fence_held` is the FENCE AUTHORITY, not mere pointer presence: this doc's reservation IS
        // the node's ACTIVE, CURRENT-generation held one — the exact prod exemption predicate the
        // referential scan walks (`src/db/invariant_scan.rs:228-237`, the StuckSending HOLD carve-out).
        // A foreign pointer, a stale `delivery_generation`, or a non-`OUTCOME_OBSERVED` reservation
        // makes this FALSE → the held-witness oracle REDs on a broken/forked fence (a `fence.is_some()`
        // presence check would false-green there — adversarial-audit MAJOR fix).
        let fence_held = res_state == "OUTCOME_OBSERVED"
            && apply_state == "PENDING_APPLY"
            && active_ptr.as_deref() == Some(reservation_id.as_slice())
            && authorized_generation == Some(delivery_generation);
        Some(ObservedHeld {
            submission_certainty: certainty,
            response_provenance: provenance,
            routing_class: routing,
            node_effect,
            evidence_kind,
            evidence_code,
            apply_state,
            node_mode: mode,
            fence_held,
        })
    }

    /// The `request_id` of the doc minted by the MOST RECENT wire op (`self.last_row`), used to key
    /// the held-witness read.  `None` before any wire op has run.
    pub fn last_request_id(&self) -> Option<[u8; 16]> {
        self.last_row.as_ref().map(|r| r.request_id)
    }

    /// CS-3 Increment 2 — the count of ACTIVE delivery reservations for this FN, using the SAME
    /// predicate as the prod `ux_reservation_active` UNIQUE index (migration 035:53-55), spec-COPIED
    /// not imported.  Prod already enforces `<= 1` via that partial unique index; the fuzzer asserts
    /// the same structurally after every op, so a `> 1` (a double-issue — two in-flight reservations
    /// for one FN) or a dropped index REDs.
    pub async fn active_reservation_count(&self) -> i64 {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM delivery_reservation WHERE fiscal_number = ? \
             AND (state IN ('RESERVED_NOT_STARTED','CALL_STARTED') \
                  OR (state = 'OUTCOME_OBSERVED' AND apply_state = 'PENDING_APPLY'))",
        )
        .bind(self.fn_id.as_str())
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    /// CS-3 Increment 2 part (b) — P3 fence-IDENTITY (standalone, per-op residual). The held-witness
    /// read asserts the fence AUTHORITY only when a witness is EXPECTED; this asserts it UNCONDITIONALLY
    /// after every op, so a fence corruption on a SETTLED state (a foreign / stale-generation pointer
    /// over a resting hold, which no held-witness read would revisit) is still caught. Sound predicate
    /// (prod `invariant_scan.rs:228-237` fence authority + Increment 2 ≤1-active): a `PENDING_APPLY`
    /// hold MUST be NAMED by `node_state.active_delivery_reservation_id` at the CURRENT
    /// `delivery_generation`. Returns `Ok(())` or `Err(reason)`. Deliberately does NOT assert the
    /// converse ("a set pointer names a live reservation") — that would depend on pointer-clearing on
    /// every terminal path and risk a false-RED; the hold-is-fenced direction is the sound residual and
    /// is exactly what `corrupt_active_fence_to_foreign` / `bump_delivery_generation` break.
    pub async fn fence_integrity(&self) -> Result<(), String> {
        let (ptr, gen): (Option<Vec<u8>>, i64) = sqlx::query_as(
            "SELECT active_delivery_reservation_id, delivery_generation \
             FROM node_state WHERE fiscal_number = ?",
        )
        .bind(self.fn_id.as_str())
        .fetch_one(&self.pool)
        .await
        .unwrap();
        let pending: Vec<(Vec<u8>, i64)> = sqlx::query_as(
            "SELECT reservation_id, authorized_generation FROM delivery_reservation \
             WHERE fiscal_number = ? AND state = 'OUTCOME_OBSERVED' AND apply_state = 'PENDING_APPLY'",
        )
        .bind(self.fn_id.as_str())
        .fetch_all(&self.pool)
        .await
        .unwrap();
        for (rid, agen) in &pending {
            if ptr.as_deref() != Some(rid.as_slice()) {
                return Err(format!(
                    "PENDING_APPLY hold {rid:02x?} is not named by the fence pointer {ptr:02x?} \
                     (foreign / dangling fence over a resting hold)"
                ));
            }
            if *agen != gen {
                return Err(format!(
                    "PENDING_APPLY hold fenced at STALE generation {agen} != node_state {gen} \
                     (ABA-style generation drift)"
                ));
            }
        }
        Ok(())
    }

    /// CS-3 operator-completion (1b) — the FN's ACTIVE (`PENDING_APPLY`) held reservation as
    /// `(reservation_id, request_id)`: the reservation the operator resolves + its doc's request_id
    /// for the release-witness read.  `None` if no held reservation rests.  The fence enforces ≤1, so
    /// this targets THE held reservation blocking the FN — independent of which doc was the last wire
    /// op (a drain can hold a doc that is not the most-recent sell).
    pub async fn active_held_reservation(&self) -> Option<([u8; 16], [u8; 16])> {
        // The FULL fence-authority predicate (prod `invariant_scan.rs:228-237`), not merely "any
        // PENDING_APPLY": the reservation must BE the node's active, current-generation held one.
        // Sound by Increment 2 (≤1 active reservation — a PENDING_APPLY reservation is exactly the
        // single active one), so this is behavior-preserving in every sound state; under a corrupted
        // / forked fence it fail-closes to None instead of blessing a stray PENDING_APPLY row.
        let row: Option<(Vec<u8>, Vec<u8>)> = sqlx::query_as(
            "SELECT dr.reservation_id, fd.request_id \
             FROM delivery_reservation dr \
             JOIN fiscal_documents fd \
               ON fd.document_id = dr.document_id AND fd.fiscal_number = dr.fiscal_number \
             JOIN node_state ns ON ns.fiscal_number = dr.fiscal_number \
             WHERE dr.fiscal_number = ? \
               AND dr.state = 'OUTCOME_OBSERVED' \
               AND dr.apply_state = 'PENDING_APPLY' \
               AND dr.reservation_id = ns.active_delivery_reservation_id \
               AND dr.authorized_generation = ns.delivery_generation LIMIT 1",
        )
        .bind(self.fn_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .unwrap();
        row.map(|(r, q)| {
            (
                <[u8; 16]>::try_from(r.as_slice()).expect("reservation_id is 16 bytes"),
                <[u8; 16]>::try_from(q.as_slice()).expect("request_id is 16 bytes"),
            )
        })
    }

    /// CS-3 operator-completion — the `reservation_id` of the latest reservation attempt for a doc
    /// (keyed by `request_id`), the handle `resolve_operator_pending` completes.  `None` if the doc
    /// has no reservation row.
    pub async fn reservation_id_for_request(&self, request_id: &[u8; 16]) -> Option<[u8; 16]> {
        let id: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT dr.reservation_id \
             FROM delivery_reservation dr \
             JOIN fiscal_documents fd \
               ON fd.document_id = dr.document_id AND fd.fiscal_number = dr.fiscal_number \
             WHERE fd.fiscal_number = ? AND fd.request_id = ? \
             ORDER BY dr.attempt_no DESC LIMIT 1",
        )
        .bind(self.fn_id.as_str())
        .bind(&request_id[..])
        .fetch_optional(&self.pool)
        .await
        .unwrap();
        id.map(|v| <[u8; 16]>::try_from(v.as_slice()).expect("reservation_id is 16 bytes"))
    }

    /// CS-3 operator-completion — the REAL durable witness AFTER a legal completion: the reservation
    /// `apply_state`, the node `mode` + FN-fence pointer, and the doc's terminal `state`.  Mirrors
    /// `read_held_witness` (SAME JOIN + node_state read) but keyed for the release axes.
    pub async fn read_release_witness(&self, request_id: &[u8; 16]) -> ObservedRelease {
        let (apply_state, doc_state): (String, String) = sqlx::query_as(
            "SELECT dr.apply_state, fd.state \
             FROM delivery_reservation dr \
             JOIN fiscal_documents fd \
               ON fd.document_id = dr.document_id AND fd.fiscal_number = dr.fiscal_number \
             WHERE fd.fiscal_number = ? AND fd.request_id = ? \
             ORDER BY dr.attempt_no DESC LIMIT 1",
        )
        .bind(self.fn_id.as_str())
        .bind(&request_id[..])
        .fetch_one(&self.pool)
        .await
        .unwrap();
        let (mode, fence): (String, Option<Vec<u8>>) = sqlx::query_as(
            "SELECT mode, active_delivery_reservation_id FROM node_state WHERE fiscal_number = ?",
        )
        .bind(self.fn_id.as_str())
        .fetch_one(&self.pool)
        .await
        .unwrap();
        ObservedRelease {
            apply_state,
            node_mode: mode,
            fence_held: fence.is_some(),
            doc_state,
        }
    }

    /// B10 TEETH — verify the lazy-BEGIN two-doc chain is genuinely LINKED after a
    /// `b10_lazy_begin_interposed` op:
    ///   (a) BEGIN.previous_hash == the pre-op MAC tip (`prior_tip`);
    ///   (b) business.previous_hash == BEGIN.unsigned_xml_sha256 (the SELL/RETURN
    ///       chains OFF the BEGIN, not the pre-op tip);
    ///   (c) the FN seed == business.unsigned_xml_sha256.
    /// The BEGIN is the lowest-lnd `OFFLINE_SESSION_BEGIN`; the business doc is the
    /// highest-lnd offline SELL/RETURN.  A reverted BEGIN-chain (business chaining
    /// off the pre-op tip, or the BEGIN not advancing the seed) breaks (b)/(c) →
    /// `Err` — the revert-BEGIN-chain canary depends on this.
    pub async fn assert_b10_boundary_chain_linked(
        &self,
        prior_tip: Option<&[u8]>,
    ) -> Result<(), String> {
        let begin: Option<ChainHashPair> = sqlx::query_as(
            "SELECT previous_hash, unsigned_xml_sha256 FROM fiscal_documents \
             WHERE fiscal_number = ? AND doc_type = 'OFFLINE_SESSION_BEGIN' \
             ORDER BY lnd ASC LIMIT 1",
        )
        .bind(self.fn_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .unwrap();
        let (begin_prev, begin_unsigned) =
            begin.ok_or_else(|| "B10 teeth: no OFFLINE_SESSION_BEGIN row".to_string())?;
        let biz: Option<ChainHashPair> = sqlx::query_as(
            // Tier-1 widened the offline business-doc set: the lazy BEGIN can
            // interpose before a SHIFT_OPEN / Z_REPORT too, with identical
            // chain semantics (business chains OFF the BEGIN).
            // L3: SERVICE_IN / SERVICE_OUT also share the same chain semantics.
            // EPZ: CASH_ADVANCE_EPZ (видача готівки за ЕПЗ) likewise.
            "SELECT previous_hash, unsigned_xml_sha256 FROM fiscal_documents \
             WHERE fiscal_number = ? \
             AND doc_type IN ('SELL','RETURN','SHIFT_OPEN','Z_REPORT','SERVICE_IN','SERVICE_OUT','CASH_ADVANCE_EPZ') \
             AND fs_mode = 'OFFLINE' \
             ORDER BY lnd DESC LIMIT 1",
        )
        .bind(self.fn_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .unwrap();
        let (biz_prev, biz_unsigned) =
            biz.ok_or_else(|| "B10 teeth: no offline business doc".to_string())?;

        if begin_prev.as_deref() != prior_tip {
            return Err(format!(
                "B10 teeth (a): BEGIN.previous_hash {begin_prev:?} != pre-op tip {prior_tip:?}"
            ));
        }
        if biz_prev != begin_unsigned {
            return Err(format!(
                "B10 teeth (b): business.previous_hash {biz_prev:?} != BEGIN.unsigned \
                 {begin_unsigned:?} (business must chain OFF the BEGIN)"
            ));
        }
        if self.read_seed().await != biz_unsigned {
            return Err("B10 teeth (c): FN seed != business.unsigned_xml_sha256".to_string());
        }
        Ok(())
    }

    /// The full ledger (lnd → state) for the FN — for the Task 4 differential's
    /// drain / go-online ledger-delta (`RealOutcome::Recovered` carries no
    /// per-doc detail).
    pub async fn read_ledger(&self) -> BTreeMap<i64, DocState> {
        let rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT lnd, state FROM fiscal_documents WHERE fiscal_number = ? ORDER BY lnd",
        )
        .bind(self.fn_id.as_str())
        .fetch_all(&self.pool)
        .await
        .unwrap();
        rows.into_iter()
            .map(|(lnd, s)| (lnd, doc_state_from_str(&s)))
            .collect()
    }

    async fn read_codes_consumed(&self) -> Option<i64> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM offline_codes \
             WHERE fiscal_number = ? AND consumed_at IS NOT NULL",
        )
        .bind(self.fn_id.as_str())
        .fetch_one(&self.pool)
        .await
        .unwrap();
        (n > 0).then_some(n)
    }

    /// Consumed offline-code count (the harness no-issuance check).
    /// bd `PRRO_GATE-hpc` — TOTAL offline-code rows for the FN (granted, regardless of consumption).
    /// A T=112 replenish grows this without consuming anything, which is how the harness tells a
    /// granted replenish apart from a refused one.
    pub async fn offline_codes_total(&self) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ?")
            .bind(&self.fn_id)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    pub async fn consumed_codes_count(&self) -> i64 {
        self.read_codes_consumed().await.unwrap_or(0)
    }

    /// The real `node_state.mode` — the harness scan-timing gate reads this to
    /// scan ONLY in a SETTLED mode `{Online, Offline}` (never mid-transition).
    pub async fn read_node_mode(&self) -> NodeMode {
        sqlx::query_scalar::<_, prro::db::types::DbNodeMode>(
            "SELECT mode FROM node_state WHERE fiscal_number = ?",
        )
        .bind(self.fn_id.as_str())
        .fetch_one(&self.pool)
        .await
        .unwrap()
        .0
    }

    /// The real `node_state.shift_state` — the harness reads this BEFORE an op for
    /// the mode-independent AUD-K8-1 teeth (a drain re-tick on an RMR FN must make
    /// no new wire call).
    pub async fn read_shift_state(&self) -> ShiftState {
        sqlx::query_scalar::<_, prro::db::types::DbShiftState>(
            "SELECT shift_state FROM node_state WHERE fiscal_number = ?",
        )
        .bind(self.fn_id.as_str())
        .fetch_one(&self.pool)
        .await
        .unwrap()
        .0
    }

    /// B1/M1 — the FULL offline drain-cohort size: offline-origin docs in ANY
    /// drain-candidate state (the same set the real drain re-drives,
    /// `list_drain_candidates_for_fn_ordered_by_lnd`), NOT just OFFLINE_LOCAL_ACK.
    /// A prior partial / exotic drain can leave SENT / KVT1 / ERROR_RETRYABLE /
    /// KVT2 cohort docs; the AckPath drain must provision the wire for ALL of
    /// them (ample send/last per doc — a probe consumes fewer; unused entries are
    /// ignored), else it under-provisions and strands the non-OLA docs.
    pub async fn full_drain_cohort_count(&self) -> usize {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM fiscal_documents \
             WHERE fiscal_number = ? AND fs_mode = 'OFFLINE' \
               AND state IN ('OFFLINE_LOCAL_ACK','SENT','KVT1','ERROR_RETRYABLE','KVT2')",
        )
        .bind(self.fn_id.as_str())
        .fetch_one(&self.pool)
        .await
        .unwrap();
        n as usize
    }

    /// O1 — count of docs resting in the two online-convergence states
    /// (`SENT`/`KVT1`) for this FN: the set the online-convergence tick targets,
    /// and the set the referential scan never flags as stuck.
    pub async fn resting_online_doc_count(&self) -> usize {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM fiscal_documents \
             WHERE fiscal_number = ? AND state IN ('SENT','KVT1')",
        )
        .bind(self.fn_id.as_str())
        .fetch_one(&self.pool)
        .await
        .unwrap();
        n as usize
    }

    /// `node_state.next_lnd` — the local fiscal numerator.  A drain allocates NO
    /// new lnd (it re-drives existing cohort docs), so the MH bounded postcond
    /// asserts this is unchanged across a Fault-deferred exotic drain.
    pub async fn read_next_lnd(&self) -> i64 {
        sqlx::query_scalar::<_, i64>("SELECT next_lnd FROM node_state WHERE fiscal_number = ?")
            .bind(self.fn_id.as_str())
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    /// The active OPEN/DRAINING offline session id via the REAL predicate the
    /// drain uses (`current_open_or_draining_session`) — the structural
    /// settle-capability test for the terminal liveness gate (A4).  A GoingOnline
    /// node is only legitimately settle-able by a drain when an active session
    /// exists (the real drain skips with `no_active_offline_session` otherwise,
    /// `backlog_drain.rs:741`).
    pub async fn active_offline_session(&self) -> Option<OfflineSessionId> {
        offline_sessions::current_open_or_draining_session(&self.pool, &self.fn_id)
            .await
            .expect("active-session query")
            .map(|(id, _state)| id)
    }

    /// The real drain cohort size for `session_id` (the same predicate the drain
    /// scans, `list_drain_candidates_for_fn_ordered_by_lnd`) — non-empty ⟺ there
    /// is offline backlog a real drain would still own.  The terminal liveness
    /// gate panics only on a NON-empty cohort: an empty-cohort GoingOnline is a
    /// forced-mode artifact with nothing to drain, not a liveness failure.
    pub async fn drain_cohort_len(&self, session_id: OfflineSessionId) -> usize {
        fiscal_documents::list_drain_candidates_for_fn_ordered_by_lnd(
            &self.pool,
            &self.fn_id,
            session_id,
        )
        .await
        .expect("drain candidates query")
        .len()
    }
}

// ─── The interpreter ────────────────────────────────────────────────────────

/// Execute one `Op` against the live DB through its real seam and return the
/// observed ledger effect.
pub async fn run_op(ctx: &mut FuzzCtx, op: &Op) -> RealOutcome {
    match op {
        // ── valid ──
        Op::OnlineSell(script) => online_sell(ctx, script).await,
        Op::OfflineSell => offline_sell(ctx).await,
        // PR-R-fuzz — a RETURN drives the SAME write-path seam as a SELL with a
        // `RETURN` inbox row (operation_type → DocType::Return); the fuzzer
        // enters at `inline::run`, downstream of the ingress STOP-R1
        // `return_check_number` guard, so `return_check_number` is never set.
        Op::OnlineReturn(script) => online_return(ctx, script).await,
        Op::OfflineReturn => offline_return(ctx).await,
        // L3 — service cash-in/out: bimodal (online wire-hitting + offline local-ack).
        // Drain is generic (no doc_type filter) so offline service-io drains for free.
        Op::OnlineServiceIn(script) => online_service_in(ctx, script).await,
        Op::OnlineServiceOut(script) => online_service_out(ctx, script).await,
        Op::OfflineServiceIn => offline_service_in(ctx).await,
        Op::OfflineServiceOut => offline_service_out(ctx).await,
        Op::OnlineShiftOpen(script) => online_shift_open(ctx, script).await,
        Op::OfflineShiftOpen => offline_shift_open(ctx).await,
        Op::OnlineZReport(script) => online_z_report(ctx, script).await,
        Op::OfflineZReport => offline_z_report(ctx).await,
        // EPZ — bimodal (online wire-hitting `<C T='8'>` + offline local-ack).
        // Drain is generic (no doc_type filter) so offline EPZ drains for free.
        Op::OnlineEpz(script) => online_epz(ctx, script).await,
        Op::OfflineEpz => offline_epz(ctx).await,
        // L6 — X-report (поточний звіт): a side-effect-free read through the REAL
        // ingress dispatch (`handle_command` → the ReadOnly arm → `handle_x_report`).
        Op::XReport => x_report(ctx).await,
        // T=112 — drives the REAL `OfflineCodeReplenishService` (that is why `FuzzCtx` owns an `App`).
        Op::Replenish(leaf) => replenish(ctx, *leaf).await,
        // L5 — drive a SELL THROUGH convert_to_signer_payload (the pre-inbox guard
        // layer).  A violation kind is refused pre-inbox (Refused, no row); Valid
        // converts + issues via inline::run.
        Op::L5Probe(kind) => l5_probe(ctx, *kind).await,
        Op::GoOnline(script) => go_online(ctx, script).await,
        Op::Drain(script) => drain_op(ctx, script).await,
        Op::Reboot => reboot(ctx).await,
        // ── crash (wire stages only — drop-injection) ──
        Op::Crash(Stage::Send) => crash_via_drop(ctx, Stage::Send).await,
        Op::Crash(Stage::Kvt1) => crash_via_drop(ctx, Stage::Kvt1).await,
        // U3: the stage-composition crashes (no DPS hang — the pipeline is run
        // up to a committed-envelope boundary, then STOPPED).  They model
        // PROCESS death, so the harness holds "no new op until the resolving
        // Reboot" (dead-until-reboot in `run_harness`) — that realism is what
        // makes generative emission safe (pre-U3 a `[Crash(Sign), OnlineSell]`
        // buried the SIGNED doc under later issuance, an unreachable prod
        // state, so Crash(Sign) was directed-only).
        Op::Crash(Stage::Sign) => crash_after_sign(ctx).await,
        // The #192 birth-site window: the offline-ack envelope committed (or
        // typed-refused) and the process died BEFORE the post-ack inbox
        // finalize / refusal terminalisation.
        Op::Crash(Stage::OfflineAck) => crash_after_offline_ack(ctx).await,
        // Crash(Finalize) is DEFERRED (CP5): its true window (KVT2↔Ack commit ↔
        // inbox/audit write) sits INSIDE `inline::run`'s private ladder — an
        // honest tests-only composition cannot reach it without reimplementing
        // inline logic, and a kill-point hook there is a `src/` change.
        // Acquire/Kvt2/Drain likewise stay ungenerated.
        Op::Crash(stage) => unimplemented!(
            "Crash({stage:?}) (non-wire stage-composition) is not implemented; \
             the generator emits Crash(Send/Kvt1/Sign/OfflineAck) only"
        ),
        // ── invalid / re-entry / replay (run the same seam; expect refusal/no-op) ──
        Op::RepeatDrain => drain_op(ctx, &DpsScript(Vec::new())).await,
        Op::RepeatReboot => reboot(ctx).await,
        Op::DuplicateIdemKey => duplicate_idem_key(ctx).await,
        Op::GoOnlineWithoutBacklog => go_online(ctx, &DpsScript(Vec::new())).await,
        Op::OfflineSellDuringGoingOnline => offline_sell_during_going_online(ctx).await,
        Op::SellWithClosedShift => sell_with_closed_shift(ctx).await,
        Op::OperatorComplete(kind) => operator_complete(ctx, *kind).await,
    }
}

/// CS-3 operator-completion — drive the REAL `admin::resolve_operator_pending` seam (the SOLE legal
/// exit from a `PENDING_APPLY` HELD reservation — the eternal-BRICK guard) against the FN's ACTIVE
/// held reservation (the fence enforces ≤1).  The operator resolves THE held reservation blocking
/// the FN, NOT "the most-recent wire op's doc" — these can differ (a DRAIN can create a held
/// reservation on a doc that is not the last direct sell).  A no-op refusal when no held reservation
/// rests (so the generator can emit it freely without a `prop_filter`).  On success reads back the
/// durable `ReleasedWitness`; on a typed refusal (e.g. FN-mismatch) the hold is left intact.
async fn operator_complete(ctx: &mut FuzzCtx, kind: OperatorResolutionKind) -> RealOutcome {
    use prro::db::repositories::delivery_reservation::OperatorResolution;
    let Some((reservation_id, rid)) = ctx.active_held_reservation().await else {
        return RealOutcome::Refused("operator_complete: no held reservation rests".into());
    };
    let resolution = match kind {
        // Accepted needs the operator-observed NON-EMPTY server fiscal number (prod validates only
        // non-empty, delivery_reservation.rs:1368). It MUST equal the DPS stub's assigned FN
        // (`SERVER_FISCAL_NO`): in reality the operator supplies the exact number DPS assigned, so a
        // SUBSEQUENT drain that re-probes this now-`SENT` offline-origin doc (a completed drain-held
        // doc re-enters the cohort — `SENT` is a drain-candidate state) confirms it via `last_chk`
        // WITHOUT a spurious `LastChkIdMismatch`. An UNRELATED literal here forks the FN and
        // structurally-halts the go-online drain forever (node stuck `GoingOnline`) — a fixture
        // artifact, NOT a prod fault (fuzzer liveness finding, task #18 offline-half).
        OperatorResolutionKind::Accepted => OperatorResolution::Accepted {
            fiscal_number: SERVER_FISCAL_NO.to_string(),
        },
        OperatorResolutionKind::NotAccepted => OperatorResolution::NotAccepted,
        OperatorResolutionKind::NotAcceptedOffline => OperatorResolution::NotAcceptedOffline,
        OperatorResolutionKind::MacReseed => OperatorResolution::MacReseed { seed: [0x5a; 32] },
    };
    match prro::admin::resolve_operator_pending(
        &ctx.pool,
        ctx.fn_id.as_str(),
        reservation_id,
        resolution,
    )
    .await
    {
        Ok(_) => {
            // Peer-tip axis phase A.  A completion moves OUR side with no wire
            // call, so the peer cannot follow: `Accepted` (online origin)
            // advances the seed to the held doc's own hash, `NotAcceptedOffline`
            // REWINDS it and cancels the cohort, `MacReseed` re-bases it to the
            // operator's value.  Whether that AGREES with the peer is exactly
            // the operator-claim-vs-peer-truth question phase C models; phase A
            // records the event and stops asserting rather than guessing.
            ctx.peer_mark_diverged(&format!(
                "operator completion ({kind:?}) moved our seed with no wire call"
            ));
            RealOutcome::Released(ctx.read_release_witness(&rid).await)
        }
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// CS-3 MacReseed directed driver (task #18 (B)) — drive the REAL `resolve_operator_pending` with an
/// operator-supplied `MacReseed { seed }` against the FN's active held reservation, using an EXPLICIT
/// seed (NOT the `[0x5a; 32]` placeholder in `operator_complete`). MacReseed is generator-EXCLUDED (it
/// needs the operator's CORRECTED chain seed), so it is DIRECTED-only: the valid-path test supplies
/// `last_issued_tip` (guards A+B pass), the guard-B test a wrong seed, the guard-A test drives it on a
/// non-`MacReseedPending` hold.  Mirrors the prod teeth `operator_completion::oc23`/`oc24` through the
/// fuzzer's REAL seam.  A no-op refusal when no held reservation rests.
pub async fn operator_complete_macreseed(ctx: &FuzzCtx, seed: [u8; 32]) -> RealOutcome {
    use prro::db::repositories::delivery_reservation::OperatorResolution;
    let Some((reservation_id, rid)) = ctx.active_held_reservation().await else {
        return RealOutcome::Refused(
            "operator_complete_macreseed: no held reservation rests".into(),
        );
    };
    match prro::admin::resolve_operator_pending(
        &ctx.pool,
        ctx.fn_id.as_str(),
        reservation_id,
        OperatorResolution::MacReseed { seed },
    )
    .await
    {
        Ok(_) => RealOutcome::Released(ctx.read_release_witness(&rid).await),
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// `OnlineSell` → `inline::run` on an Online node, ScriptedDps loaded from the
/// op's `DpsScript`.
///
/// B10: `inline::run` dispatches by NODE MODE, not op name — so an `OnlineSell`
/// op on an OFFLINE-seeded ctx (the `harness_offline_seeded` proptest lane) takes
/// the offline lane and lazily interposes a BEGIN.  Detect that (BEGIN 0→1) and
/// report `Recovered` so the differential routes to the two-doc ledger-delta,
/// exactly like `offline_sell` — otherwise the per-doc chain-continuity check
/// spuriously REDs (the business doc chains off the BEGIN, not the pre-op tip).
async fn online_sell(ctx: &mut FuzzCtx, script: &DpsScript) -> RealOutcome {
    let begin_before = begin_doc_count(ctx).await;
    let row = ctx.seed_inbox_sell().await;
    let dps = ctx.new_dps();
    load_script(&dps, script);
    let guard = ctx.gate.clone().lock_owned().await;
    let result = inline::run(
        &ctx.pool,
        &ctx.pool_secure,
        &dps,
        &ctx.sign_ctx,
        &ctx.fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await;
    drop(guard);
    match result {
        Ok(_outcome) => {
            ctx.last_row = Some(row.clone()); // remember for DuplicateIdemKey replay
            if begin_doc_count(ctx).await > begin_before {
                return RealOutcome::Recovered {
                    branch: "b10_lazy_begin_interposed".into(),
                };
            }
            RealOutcome::Doc(ctx.observe_doc_by_request_id(&row.request_id).await)
        }
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

async fn run_inline_row(
    ctx: &mut FuzzCtx,
    row: InboxRow,
    script: Option<&DpsScript>,
) -> RealOutcome {
    let dps = ctx.new_dps();
    if let Some(script) = script {
        load_script(&dps, script);
    }
    let guard = ctx.gate.clone().lock_owned().await;
    let result = inline::run(
        &ctx.pool,
        &ctx.pool_secure,
        &dps,
        &ctx.sign_ctx,
        &ctx.fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await;
    drop(guard);
    match result {
        Ok(_outcome) => {
            let observed = ctx.observe_doc_by_request_id(&row.request_id).await;
            ctx.last_row = Some(row);
            RealOutcome::Doc(observed)
        }
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// Drop-injection crash on a wire stage (kill-matrix K3/K4, spec §4): hang
/// `ScriptedDps` on the wire await, then drop the `inline::run` future — the
/// "crash" mid-wire.  No timing hooks inside a `with_immediate`.
///
/// `Stage::Send` hangs `send_chk` (SENDING committed when reached).  `Stage::Kvt1`
/// pushes a successful send first (so Sending→Sent commits), then hangs the
/// `last_chk` confirm (SENT committed when reached).
///
/// Robust to out-of-precondition: if the wire is never reached (e.g. the shift
/// was closed earlier in the sequence so `inline::run` refuses before any wire
/// call), the future COMPLETES instead of hanging — that is a refusal / no-op,
/// not a crash, and is reported as such (no panic).
async fn crash_via_drop(ctx: &mut FuzzCtx, stage: Stage) -> RealOutcome {
    // B10: on an OFFLINE node the offline-ack path makes no wire call, so this
    // "crash" COMPLETES as a real offline sell — which lazily interposes a BEGIN
    // when it is the session's first offline doc.  Detect that (BEGIN 0→1 across
    // the completed run) → report `Recovered` so the O2 differential uses the
    // two-doc ledger-delta (the model's `predict_crash_completed_sell` →
    // `apply_sell` predicts both docs; the per-doc chain check would spuriously
    // RED on the SELL chaining off the BEGIN).
    let begin_before = begin_doc_count(ctx).await;
    let row = ctx.seed_inbox_sell().await;
    let dps = ctx.new_dps();
    let (reached_tx, reached_rx) = oneshot::channel::<()>();
    let (block_tx, block_rx) = oneshot::channel::<()>();
    match stage {
        Stage::Send => dps.hang_send(reached_tx, block_rx),
        Stage::Kvt1 => {
            dps.push_send(Ok(ack(SERVER_FISCAL_NO, Vec::new()))); // send Ok → Sending→Sent
            dps.hang_last(reached_tx, block_rx);
        }
        other => unreachable!("crash_via_drop handles only wire stages; got {other:?}"),
    }

    let guard = ctx.gate.clone().lock_owned().await;
    let completed = {
        let mut fut = Box::pin(inline::run(
            &ctx.pool,
            &ctx.pool_secure,
            &dps,
            &ctx.sign_ctx,
            &ctx.fn_sign,
            &guard,
            &row,
            prro::services::time_budget::system_gate(),
        ));
        tokio::select! {
            res = &mut fut => Some(res),          // wire never reached → not a crash
            _ = reached_rx => { drop(fut); None } // wire await reached → crash (drop the future)
        }
    };
    let _keep_block_tx = block_tx; // keep the block sender alive past the drop
    drop(guard);

    match completed {
        None => {
            // Peer-tip axis phase A.  `Crash(Send)` parks INSIDE `send_chk`
            // AFTER the envelope was handed over but BEFORE the reply is
            // popped, so the peer definitively RECEIVED the document and
            // nothing in the script ever says whether it took it — the choice
            // is out-of-script by construction, which is why phase C gives it
            // its own generator dimension.  (`Crash(Kvt1)` is different: the
            // send-Ack IS consumed first, so the peer advanced and so did we —
            // no divergence, and the stub's own reply handling already booked
            // it.)
            if stage == Stage::Send {
                ctx.peer_mark_diverged(
                    "Crash(Send): envelope delivered, reply never consumed — \
                     peer acceptance unknowable",
                );
            }
            RealOutcome::Crashed {
                stage,
                committed_state: ctx.observe_doc_state_by_request_id(&row.request_id).await,
            }
        }
        Some(Ok(_)) => {
            if begin_doc_count(ctx).await > begin_before {
                // Offline crash-completed sell interposed a BEGIN → two-doc delta.
                return RealOutcome::Recovered {
                    branch: "b10_lazy_begin_interposed".into(),
                };
            }
            RealOutcome::Doc(ctx.observe_doc_by_request_id(&row.request_id).await)
        }
        Some(Err(e)) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// `Crash(Sign)` — the NON-wire crash that opens the P1 boot-resume window.
/// Unlike the wire crashes (`crash_via_drop`), there is no DPS await to hang;
/// instead we drive the REAL pre-dispatch stages (`stage_acquire` →
/// `stage_sign`) to COMMIT a `SIGNED` doc (real `stage_sign` advances the MAC
/// tip correctly — no hand-seeded chain), then STOP — simulating a crash AFTER
/// the sign commit but BEFORE post-sign dispatch.  The committed `SIGNED` doc
/// survives to the next `Reboot`; on an Offline node with an EXHAUSTED code
/// pool, boot reconciliation's offline-ack refuses
/// `CodePoolExhausted` → the P1 abort (boot twin of #192).
///
/// Returns `Crashed{Sign}` so the harness treats it as a committed in-flight
/// transient (suppresses the settled scan until the resolving Reboot), exactly
/// like the wire crashes.  No code is consumed (the offline-ack never runs), so
/// this works whether or not the pool is empty.
async fn crash_after_sign(ctx: &mut FuzzCtx) -> RealOutcome {
    let row = ctx.seed_inbox_sell().await;
    // Build the canonical command from the seeded inbox row (mirrors
    // inline's build_canonical for a SELL; source_sha == canonical for non-Z).
    let command = CanonicalFiscalCommand {
        doc_type: DocType::Sell,
        business_ts: row
            .business_ts
            .clone()
            .unwrap_or_else(|| "2026-06-09T12:00:00Z".into()),
        total_sum_kop: row.total_sum_kop,
        payload_json: row.payload_json.clone(),
        payload_sha256_canonical: row.payload_sha256_canonical,
        source_sha256: row.payload_sha256_canonical,
        // U3: the signer MUST be attributed like the real inline path
        // (`Some(CASHIER)` matching the fixture shift's opened_by_cashier_id) —
        // the BOOT resume of this crashed doc runs the stage_send signer guard
        // on the Online lane, and a NULL signer is a structural refusal
        // (`SignerIdMissing`) that would false-strand the doc at SIGNED.
        signed_by_cashier_id: Some(CashierId::new(CASHIER).expect("fixture cashier id")),
        driver_id: None,
    };
    let driver_id = row
        .driver_id
        .as_deref()
        .expect("seed_inbox_sell sets driver_id");
    let _guard = ctx.gate.clone().lock_owned().await;
    // Stage 1: acquire (lease the inbox to PROCESSING + insert PREPARED).
    let acq = match stage_acquire::run(
        &ctx.pool,
        &ctx.pool_secure,
        driver_id,
        row.request_id,
        command,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => return RealOutcome::Refused(format!("crash_after_sign acquire: {e:?}")),
    };
    let wctx = match acq {
        WorkerProcessResult::Proceed(c) | WorkerProcessResult::Resumed(c) => c,
        WorkerProcessResult::Noop => {
            return RealOutcome::Refused("crash_after_sign acquire: unexpected Noop".into())
        }
        WorkerProcessResult::Rejected { reason } => {
            return RealOutcome::Refused(format!("crash_after_sign acquire rejected: {reason:?}"))
        }
    };
    // Stage 3: sign (commits SIGNED, advances the MAC tip).  Then STOP — the
    // simulated crash lands HERE, before dispatch_post_sign.
    match stage_sign::run(&ctx.pool, &ctx.sign_ctx, wctx).await {
        Ok(_) => RealOutcome::Crashed {
            stage: Stage::Sign,
            committed_state: ctx.observe_doc_state_by_request_id(&row.request_id).await,
        },
        Err(e) => RealOutcome::Refused(format!("crash_after_sign sign: {e:?}")),
    }
}

/// U3 / O4 — `Crash(OfflineAck)`: run the pipeline THROUGH the offline-ack
/// envelope, then STOP — the crash lands AFTER `stage_offline_ack`'s atomic
/// commit (or typed refusal) and BEFORE the post-ack inbox finalize / refusal
/// terminalisation.  This is the **#192 birth-site window**:
///   - ack COMMITTED → the doc is durably `OFFLINE_LOCAL_ACK` (issued, code
///     consumed) but the inbox row is still PROCESSING — boot must converge it
///     without double-issuance;
///   - ack REFUSED (e.g. `CodePoolExhausted` on a drained pool, or a mode
///     guard) → the SIGNED doc rests with the refusal never terminalised —
///     exactly the orphan #192/P1 closes on resume.
///
/// Both windows are handled by EXISTING recovery; this makes them reachable
/// generatively.  Returns `Crashed{OfflineAck}` with the observed committed
/// state, so the harness treats it as a process death (dead-until-reboot).
async fn crash_after_offline_ack(ctx: &mut FuzzCtx) -> RealOutcome {
    use prro::services::write_path::stage_offline_ack;
    let row = ctx.seed_inbox_sell().await;
    let command = CanonicalFiscalCommand {
        doc_type: DocType::Sell,
        business_ts: row
            .business_ts
            .clone()
            .unwrap_or_else(|| "2026-06-09T12:00:00Z".into()),
        total_sum_kop: row.total_sum_kop,
        payload_json: row.payload_json.clone(),
        payload_sha256_canonical: row.payload_sha256_canonical,
        source_sha256: row.payload_sha256_canonical,
        // Signer attributed like the real inline path (see crash_after_sign).
        signed_by_cashier_id: Some(CashierId::new(CASHIER).expect("fixture cashier id")),
        driver_id: None,
    };
    let driver_id = row
        .driver_id
        .as_deref()
        .expect("seed_inbox_sell sets driver_id");
    let _guard = ctx.gate.clone().lock_owned().await;
    let acq = match stage_acquire::run(
        &ctx.pool,
        &ctx.pool_secure,
        driver_id,
        row.request_id,
        command,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => return RealOutcome::Refused(format!("crash_after_offline_ack acquire: {e:?}")),
    };
    let wctx = match acq {
        WorkerProcessResult::Proceed(c) | WorkerProcessResult::Resumed(c) => c,
        WorkerProcessResult::Noop => {
            return RealOutcome::Refused("crash_after_offline_ack acquire: unexpected Noop".into())
        }
        WorkerProcessResult::Rejected { reason } => {
            return RealOutcome::Refused(format!(
                "crash_after_offline_ack acquire rejected: {reason:?}"
            ))
        }
    };
    let signed = match stage_sign::run(&ctx.pool, &ctx.sign_ctx, wctx).await {
        Ok(s) => s,
        Err(e) => return RealOutcome::Refused(format!("crash_after_offline_ack sign: {e:?}")),
    };
    // Stage 4-offline: the offline-ack envelope itself (atomic single-tx —
    // commits OFFLINE_LOCAL_ACK + consumes a code, or returns a typed
    // refusal).  Then STOP: the crash lands before the post-ack handling
    // (inbox finalize on ack / `terminalise_inbox` on refusal).
    match stage_offline_ack::run(&ctx.pool, signed.document.document_id, &ctx.fn_id).await {
        Ok(_outcome) => RealOutcome::Crashed {
            stage: Stage::OfflineAck,
            committed_state: ctx.observe_doc_state_by_request_id(&row.request_id).await,
        },
        Err(e) => RealOutcome::Refused(format!("crash_after_offline_ack ack: {e:?}")),
    }
}

/// `Reboot` → `run_boot_reconciliation`.  The Sending arm is ctx-free
/// (no wire call regardless of queue depth), matching kill-matrix K3.
///
/// U3: the boot dps is PROVISIONED (ample Ack send/last per pending doc, the
/// same philosophy as `load_drain_script` / `settle_drain_tick`) — it models
/// "DPS reachable at recovery".  Without it a composition-crash SIGNED doc on
/// the Online lane could never make its FIRST send at resume and rested
/// SIGNED at the settled boundary — a false StuckNonTerminalDoc (production
/// boot has a live channel; transport-down recovery is separately covered by
/// the K3 ctx-free SENDING arm + the ER retry class).  Unused entries are
/// ignored; arms that make no wire call (K3) still make none.
async fn reboot(ctx: &mut FuzzCtx) -> RealOutcome {
    let dps = ctx.new_dps();
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fiscal_documents \
         WHERE fiscal_number = ? \
           AND state IN ('PREPARED','SIGNED','ENCRYPTED','SENDING','SENT','KVT1','KVT2','ERROR_RETRYABLE')",
    )
    .bind(ctx.fn_id.as_str())
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    // B10: `+ 1` for the drain-time DocType=10 END (minted during the reboot's
    // drain, not counted in `pending`) — ample-provision so its wire submit lands
    // ACK.  Surplus responses are ignored.
    for _ in 0..pending + 1 {
        dps.push_send(wire_to_result(WireResponse::Ack));
        dps.push_last(wire_to_result(WireResponse::Ack));
        dps.push_last(wire_to_result(WireResponse::Ack));
    }
    let guard = drain_test_guard();
    let view = ctx.view(&dps);
    match boot_phase::run_boot_reconciliation(&guard, &ctx.pool, &ctx.fn_id, Some(&view)).await {
        Ok(branch) => RealOutcome::Recovered {
            branch: format!("{branch:?}"),
        },
        Err(e) => RealOutcome::Refused(format!("reboot: {e:?}")),
    }
}

/// B10 — count of committed `OFFLINE_SESSION_BEGIN` rows on the FN.  A 0→1 jump
/// across an offline op means THAT op lazily interposed the BEGIN → the op's
/// observable is a TWO-doc ledger delta (BEGIN + business) which the oracle diffs
/// via `check_ledger_delta` (the per-doc `check_doc_against_mutation` chains the
/// business doc against the PRE-op tip, but the BEGIN advanced the tip mid-op, so
/// per-doc chain-continuity would spuriously RED).
async fn begin_doc_count(ctx: &FuzzCtx) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM fiscal_documents \
         WHERE fiscal_number = ? AND doc_type = 'OFFLINE_SESSION_BEGIN'",
    )
    .bind(ctx.fn_id.as_str())
    .fetch_one(&ctx.pool)
    .await
    .unwrap()
}

/// `OfflineSell` → `inline::run` on an Offline node — the offline-ack path lands
/// `OFFLINE_LOCAL_ACK` and makes NO wire call (spec §5).
///
/// B10: on the FIRST offline doc of a session the impl lazily interposes an
/// `OFFLINE_SESSION_BEGIN` doc BEFORE the business doc.  When that happens (BEGIN
/// count 0→1) return `Recovered { branch: "b10_lazy_begin_interposed" }` → the
/// differential routes to the two-doc `check_ledger_delta` + boundary-chain teeth
/// (not the single-doc `check_doc_against_mutation`, whose chain-continuity check
/// cannot see the mid-op tip advance).
async fn offline_sell(ctx: &mut FuzzCtx) -> RealOutcome {
    let begin_before = begin_doc_count(ctx).await;
    let row = ctx.seed_inbox_sell().await;
    let dps = ctx.new_dps(); // offline branch never touches the wire
    let guard = ctx.gate.clone().lock_owned().await;
    let result = inline::run(
        &ctx.pool,
        &ctx.pool_secure,
        &dps,
        &ctx.sign_ctx,
        &ctx.fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await;
    drop(guard);
    match result {
        Ok(_) => {
            ctx.last_row = Some(row.clone()); // remember for DuplicateIdemKey replay
            if begin_doc_count(ctx).await > begin_before {
                return RealOutcome::Recovered {
                    branch: "b10_lazy_begin_interposed".into(),
                };
            }
            RealOutcome::Doc(ctx.observe_doc_by_request_id(&row.request_id).await)
        }
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// `OnlineReturn` → `inline::run` on an Online node with a `RETURN` inbox row.
/// Byte-for-byte the same seam as [`online_sell`] (the write-path is
/// doc-type-agnostic post-canonical: stage_sign parses the identical CheckJson,
/// stage_send maps both to `DpsCheckType::Chk`); only the seeded
/// `operation_type` differs, which `build_canonical` maps to `DocType::Return`.
async fn online_return(ctx: &mut FuzzCtx, script: &DpsScript) -> RealOutcome {
    // B10: mode-based dispatch — an `OnlineReturn` on an OFFLINE ctx takes the
    // offline lane + interposes a BEGIN; report `Recovered` so the differential
    // uses the two-doc ledger-delta (see `online_sell`).
    let begin_before = begin_doc_count(ctx).await;
    let row = ctx.seed_inbox_return().await;
    let dps = ctx.new_dps();
    load_script(&dps, script);
    let guard = ctx.gate.clone().lock_owned().await;
    let result = inline::run(
        &ctx.pool,
        &ctx.pool_secure,
        &dps,
        &ctx.sign_ctx,
        &ctx.fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await;
    drop(guard);
    match result {
        Ok(_outcome) => {
            ctx.last_row = Some(row.clone()); // remember for DuplicateIdemKey replay
            if begin_doc_count(ctx).await > begin_before {
                return RealOutcome::Recovered {
                    branch: "b10_lazy_begin_interposed".into(),
                };
            }
            RealOutcome::Doc(ctx.observe_doc_by_request_id(&row.request_id).await)
        }
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// `OfflineReturn` → `inline::run` on an Offline node — lands `OFFLINE_LOCAL_ACK`
/// and consumes an offline code, exactly like [`offline_sell`] (the offline-code
/// CAS `acquire_code_tx` is doc-type-agnostic); only the seeded `operation_type`
/// differs.
async fn offline_return(ctx: &mut FuzzCtx) -> RealOutcome {
    // B10: same lazy-BEGIN interposition as `offline_sell` — an offline RETURN
    // that is the session's first offline doc mints the BEGIN first.
    let begin_before = begin_doc_count(ctx).await;
    let row = ctx.seed_inbox_return().await;
    let dps = ctx.new_dps(); // offline branch never touches the wire
    let guard = ctx.gate.clone().lock_owned().await;
    let result = inline::run(
        &ctx.pool,
        &ctx.pool_secure,
        &dps,
        &ctx.sign_ctx,
        &ctx.fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await;
    drop(guard);
    match result {
        Ok(_) => {
            ctx.last_row = Some(row.clone()); // remember for DuplicateIdemKey replay
            if begin_doc_count(ctx).await > begin_before {
                return RealOutcome::Recovered {
                    branch: "b10_lazy_begin_interposed".into(),
                };
            }
            RealOutcome::Doc(ctx.observe_doc_by_request_id(&row.request_id).await)
        }
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// L3 `OfflineServiceIn` → `inline::run` on an Offline node with a `SERVICE_IN`
/// inbox row.  Local issuance — OFFLINE_LOCAL_ACK + code consumed + seed advance.
/// B10: first offline doc of a session interposes a lazy BEGIN (same as
/// [`offline_sell`]); report `Recovered` so the differential uses the two-doc
/// ledger-delta path.
///
/// Mode-guard: offline-only op.  If node is not Offline, return Refused
/// (model returns NoMutation — both agree: no row minted).
async fn offline_service_in(ctx: &mut FuzzCtx) -> RealOutcome {
    if ctx.read_node_mode().await != NodeMode::Offline {
        return RealOutcome::Refused("OfflineServiceIn: node not Offline".into());
    }
    let begin_before = begin_doc_count(ctx).await;
    let row = ctx.seed_inbox_service_in().await;
    let dps = ctx.new_dps(); // offline branch never touches the wire
    let guard = ctx.gate.clone().lock_owned().await;
    let result = inline::run(
        &ctx.pool,
        &ctx.pool_secure,
        &dps,
        &ctx.sign_ctx,
        &ctx.fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await;
    drop(guard);
    match result {
        Ok(_) => {
            ctx.last_row = Some(row.clone());
            if begin_doc_count(ctx).await > begin_before {
                return RealOutcome::Recovered {
                    branch: "b10_lazy_begin_interposed".into(),
                };
            }
            RealOutcome::Doc(ctx.observe_doc_by_request_id(&row.request_id).await)
        }
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// L3 `OfflineServiceOut` → `inline::run` on an Offline node with a `SERVICE_OUT`
/// inbox row.  Local issuance (OFFLINE_LOCAL_ACK); same as [`offline_service_in`].
/// Guard-3b (in-lease cash-floor) does NOT apply in the offline lane; only the
/// pre-inbox L1 guard (convert.rs) fires, which is upstream of `inline::run`.
///
/// Mode-guard: offline-only op.  Same rationale as [`offline_service_in`].
async fn offline_service_out(ctx: &mut FuzzCtx) -> RealOutcome {
    if ctx.read_node_mode().await != NodeMode::Offline {
        return RealOutcome::Refused("OfflineServiceOut: node not Offline".into());
    }
    let begin_before = begin_doc_count(ctx).await;
    let row = ctx.seed_inbox_service_out().await;
    let dps = ctx.new_dps(); // offline branch never touches the wire
    let guard = ctx.gate.clone().lock_owned().await;
    let result = inline::run(
        &ctx.pool,
        &ctx.pool_secure,
        &dps,
        &ctx.sign_ctx,
        &ctx.fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await;
    drop(guard);
    match result {
        Ok(_) => {
            ctx.last_row = Some(row.clone());
            if begin_doc_count(ctx).await > begin_before {
                return RealOutcome::Recovered {
                    branch: "b10_lazy_begin_interposed".into(),
                };
            }
            RealOutcome::Doc(ctx.observe_doc_by_request_id(&row.request_id).await)
        }
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// EPZ `OnlineEpz` → `inline::run` on an Online node with a `CASH_ADVANCE_EPZ`
/// inbox row.  Wire-hitting (`<C T='8'>`); same seam as [`online_service_out`].
/// Guard-3c (in-lease cash-floor) applies: an EPZ on an insufficient drawer is
/// refused in-lease (pre-mint, `Refused`, no fiscal_documents row).
///
/// Mode-guard: EPZ online lane is online-only.
async fn online_epz(ctx: &mut FuzzCtx, script: &DpsScript) -> RealOutcome {
    if ctx.read_node_mode().await != NodeMode::Online {
        return RealOutcome::Refused("OnlineEpz: node not Online".into());
    }
    let row = ctx.seed_inbox_epz().await;
    let dps = ctx.new_dps();
    load_script(&dps, script);
    let guard = ctx.gate.clone().lock_owned().await;
    let result = inline::run(
        &ctx.pool,
        &ctx.pool_secure,
        &dps,
        &ctx.sign_ctx,
        &ctx.fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await;
    drop(guard);
    match result {
        Ok(_) => {
            ctx.last_row = Some(row.clone());
            RealOutcome::Doc(ctx.observe_doc_by_request_id(&row.request_id).await)
        }
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// EPZ `OfflineEpz` → `inline::run` on an Offline node with a `CASH_ADVANCE_EPZ`
/// inbox row.  Local issuance (OFFLINE_LOCAL_ACK); same seam as
/// [`offline_service_out`].  Guard-3c is ONLINE-only in-lease; the offline lane
/// relies on the pre-inbox guard + durable local ledger (fixture ensures cash).
///
/// Mode-guard: offline-only op.
async fn offline_epz(ctx: &mut FuzzCtx) -> RealOutcome {
    if ctx.read_node_mode().await != NodeMode::Offline {
        return RealOutcome::Refused("OfflineEpz: node not Offline".into());
    }
    let begin_before = begin_doc_count(ctx).await;
    let row = ctx.seed_inbox_epz().await;
    let dps = ctx.new_dps(); // offline branch never touches the wire
    let guard = ctx.gate.clone().lock_owned().await;
    let result = inline::run(
        &ctx.pool,
        &ctx.pool_secure,
        &dps,
        &ctx.sign_ctx,
        &ctx.fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await;
    drop(guard);
    match result {
        Ok(_) => {
            ctx.last_row = Some(row.clone());
            if begin_doc_count(ctx).await > begin_before {
                return RealOutcome::Recovered {
                    branch: "b10_lazy_begin_interposed".into(),
                };
            }
            RealOutcome::Doc(ctx.observe_doc_by_request_id(&row.request_id).await)
        }
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// Ensure the D1 frozen payment slots (cash #1, card #2) exist on the secure
/// pool.  Idempotent: the fuzzer fixture does NOT seed payment_methods, and an
/// L5 probe converts a SELL that references the cash / card slots by name.
async fn ensure_payment_methods(ctx: &FuzzCtx) {
    for (idx, name, iscash) in [(1i64, "Готівка", true), (2i64, "Картка", false)] {
        let exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM payment_methods WHERE fn = ? AND pay_index = ?",
        )
        .bind(ctx.fn_id())
        .bind(idx)
        .fetch_one(&ctx.pool_secure)
        .await
        .unwrap();
        if exists == 0 {
            pm_insert(
                &ctx.pool_secure,
                &NewPaymentMethod {
                    fn_id: ctx.fn_id().to_string(),
                    pay_index: idx,
                    name: name.to_string(),
                    iscash,
                },
            )
            .await
            .unwrap();
        }
    }
}

/// Ensure tax group 1 (20% VAT-included) exists on the secure pool.  Idempotent:
/// the L5 probe's good carries `tax_group_1:1` (convert always emits a tax group),
/// so stage_acquire needs the group seeded to build the signing snapshot.
async fn ensure_tax_group_1(ctx: &FuzzCtx) {
    let exists: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM tax_groups WHERE fn = ? AND tx_num = ?")
            .bind(ctx.fn_id())
            .bind(1i64)
            .fetch_one(&ctx.pool_secure)
            .await
            .unwrap();
    if exists == 0 {
        ctx.seed_tax_group_20_percent().await;
    }
}

/// L5 — drive a SELL of the given amount-shape THROUGH `convert_to_signer_payload`
/// (the pre-inbox guard layer).  This is the ONLY fuzzer op that enters ABOVE
/// `inline::run` — every other SELL op seeds an already-converted payload — so it
/// is the only lane where the four L5 input guards (G1..G4) can actually fire.
///
/// A violation kind (`OverCap`/`ZeroPrice`/`ZeroPayment`/`Underpaid`) is REFUSED
/// by convert BEFORE any inbox / fiscal_documents row is minted → `Refused` (the
/// model predicts `NoMutation`; the harness's ExpectedNoMutation "minted no row"
/// assertion is the durable teeth — revert a prod guard ⇒ convert admits ⇒ prod
/// mints a row ⇒ RED).  `Valid` converts and then issues via `inline::run` like
/// an ordinary online SELL (differential-checked as `Mutated`).
///
/// Mode-guard: online-only.  On an offline node the op is a no-op (model
/// NoMutation) — the amount guards are ingress-layer input validation.
async fn l5_probe(ctx: &mut FuzzCtx, kind: L5Kind) -> RealOutcome {
    if ctx.read_node_mode().await != NodeMode::Online {
        return RealOutcome::Refused("L5Probe: node not Online".into());
    }
    ensure_payment_methods(ctx).await;
    // The probe's good carries `tax_group_1:1` (convert always emits a tax group),
    // so signing needs the group seeded (stage_acquire builds the snapshot from
    // `tax_groups`).  Idempotent: seed only if absent.  Harmless for the refusal
    // kinds (they never reach signing).
    ensure_tax_group_1(ctx).await;

    // Amount-shape per kind: (good_price_kop, payments_json_array, total_sale_kop).
    let (good_price_kop, payments_json, total_sale_kop): (i64, &str, i64) = match kind {
        L5Kind::OverCap => (
            5_000_000,
            r#"[{"type":"CASH","amount_kopecks":5000000}]"#,
            5_000_000,
        ),
        // Zero-price good but a NON-zero, non-underpaying cash leg → ONLY G2 can
        // refuse (isolates the ZeroPriceLine teeth from G3/G4).
        L5Kind::ZeroPrice => (0, r#"[{"type":"CASH","amount_kopecks":100}]"#, 100),
        L5Kind::ZeroPayment => (
            10000,
            r#"[{"type":"CASHLESS_1","amount_kopecks":10000},{"type":"CASH","amount_kopecks":0}]"#,
            10000,
        ),
        L5Kind::Underpaid => (1000, r#"[{"type":"CASH","amount_kopecks":900}]"#, 1000),
        L5Kind::Valid => (15000, r#"[{"type":"CASH","amount_kopecks":15000}]"#, 15000),
    };

    let idem = format!("l5-{}", ctx.next_idem());
    let cmd_json = format!(
        r#"{{
            "schema_version": "1.0",
            "fiscal_number": "{fn}",
            "command_type": "SELL",
            "idempotency_key": "{idem}",
            "cashier_id": null,
            "department": null,
            "return_check_number": null,
            "payload": {{
                "direction": "SALE",
                "goods": [{{"name":"Item","quantity_milli":1000,"price_kopecks":{good_price_kop},"tax_group_1":1,"tax_group_2":0,"article_code":1}}],
                "payments": {payments_json},
                "totals": {{"sale_kopecks":{total_sale_kop},"return_kopecks":0}}
            }}
        }}"#,
        fn = ctx.fn_id(),
    );
    let cmd: CanonicalCommand = serde_json::from_str(&cmd_json).expect("parse L5 SELL cmd");

    // THE guard layer: convert refuses a violation pre-inbox (no row).
    let converted =
        match convert_to_signer_payload(&cmd, ctx.fn_id(), &ctx.pool, &ctx.pool_secure).await {
            Ok(cp) => cp,
            Err(e) => return RealOutcome::Refused(format!("convert refused: {e:?}")),
        };

    // Valid path: seed the CONVERTED payload into the inbox and issue via inline.
    let row = seed_inbox_keyed_payload(
        &ctx.pool,
        &idem,
        "SELL",
        &converted.payload_json,
        Some(total_sale_kop),
    )
    .await;
    let dps = ctx.new_dps();
    load_script(&dps, &DpsScript::ack_path());
    let guard = ctx.gate.clone().lock_owned().await;
    let result = inline::run(
        &ctx.pool,
        &ctx.pool_secure,
        &dps,
        &ctx.sign_ctx,
        &ctx.fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await;
    drop(guard);
    match result {
        Ok(_) => {
            ctx.last_row = Some(row.clone());
            RealOutcome::Doc(ctx.observe_doc_by_request_id(&row.request_id).await)
        }
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// T=112 offline-code replenish — drives the REAL `OfflineCodeReplenishService`.
///
/// This op exists because bd `PRRO_GATE-hpc` gave the standalone replenish real production seed
/// semantics (a NON-document `Hs = sha256(request_xml)` plus the durable `chain_seed_transitions`
/// witness folded into `active_chain_tip_unsigned_xml_sha256`) with DIRECTED coverage only.
/// Re-implementing those effects here instead of driving the service would make the oracle agree with
/// itself by construction — so the fixture owns a real `App` and this arm calls production.
///
/// Scope (RULING 2 §4): only the DECIDED leaves.  `Granted` and `ServerReject` are contractually
/// settled; the ambiguous / transport-timeout branch stays out until the live capture lands
/// (bd `PRRO_GATE-2ds`).
///
/// Codes are made unique per call from the ctx sequence so a second replenish in one case exercises a
/// genuine INSERT rather than colliding with the previous window's dedup.
async fn replenish(ctx: &mut FuzzCtx, leaf: ReplenishLeaf) -> RealOutcome {
    let dps = Arc::new(ctx.new_dps());
    ctx.seq += 1;
    let seq = ctx.seq;
    match leaf {
        ReplenishLeaf::Granted => {
            dps.push_ask_codes(Ok(prro::transports::dps::dto::OfflineCodesResponse {
                codes: vec![format!("fuzz-code-{seq}")],
            }))
        }
        ReplenishLeaf::ServerReject => {
            dps.push_ask_codes(Err(prro::transports::dps::error::DpsError::Server {
                code: -8,
                message: "fuzzer: T=112 server reject".into(),
            }))
        }
    }
    let svc =
        prro::services::offline_sync::offline_code_replenish::OfflineCodeReplenishService::new(
            ctx.app.clone(),
            dps,
            Arc::new(det_signing_ctx()),
        );
    match svc.replenish(ctx.fn_id(), "12345678", seq as u32, 1).await {
        Ok(summary) => {
            let new_seed = (0..summary.new_seed_hex.len() / 2)
                .map(|i| {
                    u8::from_str_radix(&summary.new_seed_hex[i * 2..i * 2 + 2], 16).unwrap_or(0)
                })
                .collect::<Vec<u8>>();
            // Peer-tip axis phase A — CONVERGENCE, *conditionally*.
            //
            // With an EMPTY backlog a granted T=112 advances the chain to
            // `sha256(request_xml)` — a NON-document seed — on BOTH sides: the
            // peer processed the very request whose hash we adopted.  Without
            // this mover the peer falls a replenish behind and the next ordinary
            // send looks like a divergence, which is exactly how phase A caught
            // the omission on its first run.
            //
            // With a NON-EMPTY undrained backlog it is NOT a convergence — it is
            // `bd PRRO_GATE-knk` (P1): the backlog is already minted and FROZEN
            // on the pre-T112 chain, so moving the peer forward strands it.  Our
            // own live smoke observed a drained offline doc being rejected for a
            // mismatched MAC (`live_dps_extended_smoke.rs:2603-2612`) and works
            // around it by polling — but that workaround only covers the
            // T=112-then-offline order; this is the reverse, where no poll can
            // help.  The reference client never hits it because it RE-ANCHORS at
            // drain (`SendingOfflineChecks.cs:40,47-48` substitutes DPS's current
            // tip into the `mmmaaaccc` placeholder); we freeze at sign time.
            //
            // Phase A records it and stops asserting rather than pretending to
            // know which side is wrong — the open node is whether DPS would even
            // ACCEPT this T=112 (its `<MAC>` is a value DPS has never seen).  The
            // `#[ignore]`d pin `knk_t112_during_backlog_...` reproduces it.
            if ctx.undrained_offline_backlog().await > 0 {
                ctx.peer_mark_diverged(
                    "bd PRRO_GATE-knk: T=112 advanced the chain while an undrained \
                     offline backlog rests — the backlog is frozen on the pre-T112 chain",
                );
            } else {
                ctx.peer_converge_to(Some(new_seed.clone()));
            }
            RealOutcome::Replenished {
                inserted: summary.inserted,
                deduped: summary.deduped,
                new_seed,
            }
        }
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// L6 — X-report (поточний звіт) through the REAL ingress dispatch.
///
/// Drives `handle_command` with a `CommandType::XReport` command, exactly like
/// production — so the ReadOnly arm routes to `handle_x_report`, which is a pure
/// SELECT (no inbox row, no fiscal_documents row, no lnd/seed/code, no shift
/// transition).  Returns `RealOutcome::XReport` carrying the observed turnover
/// snapshot (cash-on-hand + aggregated payload JSON) so the harness can assert
/// it matches the model's tracked `cash_on_hand`.  A no-open-shift 422 (the
/// forced-Closed-shift / no-current-shift window) is a row-less `Refused` — also
/// a NoMutation, so the differential is satisfied either way.
async fn x_report(ctx: &mut FuzzCtx) -> RealOutcome {
    let idem = format!("x-report-{}", ctx.next_idem());
    let cmd_json = format!(
        r#"{{
            "schema_version": "1.0",
            "fiscal_number": "{fn}",
            "command_type": "X_REPORT",
            "idempotency_key": "{idem}",
            "cashier_id": null,
            "department": null,
            "return_check_number": null,
            "payload": {{"direction": "SALE", "totals": {{"sale_kopecks": 0, "return_kopecks": 0}}}}
        }}"#,
        fn = ctx.fn_id(),
    );
    let cmd: CanonicalCommand = serde_json::from_str(&cmd_json).expect("parse X_REPORT cmd");
    let drv = prro::db::models::ids::DriverId::new(DRIVER).expect("driver id");
    let wp = UnimplementedWritePath;
    let resp = handle_command(
        &cmd,
        ctx.fn_id(),
        drv,
        Protocol::Rest,
        &ctx.pool,
        &ctx.pool_secure,
        &wp,
    )
    .await;
    match resp.body {
        IngressBody::XReport(x) => RealOutcome::XReport {
            cash_on_hand_kop: x.cash_on_hand_kop,
            turnover_json: x.turnover.to_string(),
        },
        // A no-open-shift / closed-shift window → row-less 422 NO_OPEN_SHIFT.
        IngressBody::Error(e) => {
            RealOutcome::Refused(format!("x-report refused: {}", e.error_code))
        }
        IngressBody::Success(_) => {
            unreachable!("X-report must never return a fiscal Success envelope")
        }
    }
}

/// L3 `OnlineServiceIn` → `inline::run` on an Online node with a `SERVICE_IN`
/// inbox row.  Wire-hitting; same seam as [`online_sell`] — only the
/// `operation_type` differs (→ `DocType::ServiceIn`).
///
/// Mode-guard: service-io is online-only.  If the node is offline the op is a
/// no-op (real Refused, model NoMutation); offline service-io uses
/// [`offline_service_in`] via `Op::OfflineServiceIn`.
async fn online_service_in(ctx: &mut FuzzCtx, script: &DpsScript) -> RealOutcome {
    if ctx.read_node_mode().await != NodeMode::Online {
        return RealOutcome::Refused("OnlineServiceIn: node not Online".into());
    }
    let row = ctx.seed_inbox_service_in().await;
    let dps = ctx.new_dps();
    load_script(&dps, script);
    let guard = ctx.gate.clone().lock_owned().await;
    let result = inline::run(
        &ctx.pool,
        &ctx.pool_secure,
        &dps,
        &ctx.sign_ctx,
        &ctx.fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await;
    drop(guard);
    match result {
        Ok(_) => {
            ctx.last_row = Some(row.clone());
            RealOutcome::Doc(ctx.observe_doc_by_request_id(&row.request_id).await)
        }
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// L3 `OnlineServiceOut` → `inline::run` on an Online node with a `SERVICE_OUT`
/// inbox row.  Same seam as [`online_service_in`].  Guard-3b (in-lease
/// cash-floor) applies: a `SERVICE_OUT` on an empty drawer is refused in-lease
/// (pre-mint, `Refused` outcome, no fiscal_documents row).
///
/// Mode-guard: same as [`online_service_in`] — online-only op.
async fn online_service_out(ctx: &mut FuzzCtx, script: &DpsScript) -> RealOutcome {
    if ctx.read_node_mode().await != NodeMode::Online {
        return RealOutcome::Refused("OnlineServiceOut: node not Online".into());
    }
    let row = ctx.seed_inbox_service_out().await;
    let dps = ctx.new_dps();
    load_script(&dps, script);
    let guard = ctx.gate.clone().lock_owned().await;
    let result = inline::run(
        &ctx.pool,
        &ctx.pool_secure,
        &dps,
        &ctx.sign_ctx,
        &ctx.fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await;
    drop(guard);
    match result {
        Ok(_) => {
            ctx.last_row = Some(row.clone());
            RealOutcome::Doc(ctx.observe_doc_by_request_id(&row.request_id).await)
        }
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// `OnlineShiftOpen` → live SHIFT_OPEN through production inline path.
async fn online_shift_open(ctx: &mut FuzzCtx, script: &DpsScript) -> RealOutcome {
    if ctx.read_node_mode().await != NodeMode::Online {
        return RealOutcome::Refused("online SHIFT_OPEN requires an Online node".into());
    }
    let row = ctx.seed_inbox_shift_open().await;
    let dps = ctx.new_dps();
    load_script(&dps, script);
    let guard = ctx.gate.clone().lock_owned().await;
    let result = inline::run(
        &ctx.pool,
        &ctx.pool_secure,
        &dps,
        &ctx.sign_ctx,
        &ctx.fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await;
    drop(guard);
    match result {
        Ok(_outcome) => {
            let observed = ctx.observe_doc_by_request_id(&row.request_id).await;
            ctx.last_row = Some(row);
            RealOutcome::Doc(observed)
        }
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// `OfflineShiftOpen` → live SHIFT_OPEN local-ack through production inline path.
/// B10: the first offline doc interposes a lazy BEGIN — report `Recovered` so
/// the differential routes to the two-doc ledger-delta (see `offline_sell`).
async fn offline_shift_open(ctx: &mut FuzzCtx) -> RealOutcome {
    if ctx.read_node_mode().await != NodeMode::Offline {
        return RealOutcome::Refused("offline SHIFT_OPEN requires an Offline node".into());
    }
    let begin_before = begin_doc_count(ctx).await;
    let row = ctx.seed_inbox_shift_open().await;
    let dps = ctx.new_dps(); // offline branch never touches the wire
    let guard = ctx.gate.clone().lock_owned().await;
    let result = inline::run(
        &ctx.pool,
        &ctx.pool_secure,
        &dps,
        &ctx.sign_ctx,
        &ctx.fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await;
    drop(guard);
    match result {
        Ok(_outcome) => {
            ctx.last_row = Some(row.clone());
            if begin_doc_count(ctx).await > begin_before {
                return RealOutcome::Recovered {
                    branch: "b10_lazy_begin_interposed".into(),
                };
            }
            RealOutcome::Doc(ctx.observe_doc_by_request_id(&row.request_id).await)
        }
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// `OnlineZReport` → live inline Z dispatcher on an Online node.  This drives
/// the production quiesce → aggregate → build_z_canonical → staged write path.
async fn online_z_report(ctx: &mut FuzzCtx, script: &DpsScript) -> RealOutcome {
    if ctx.read_node_mode().await != NodeMode::Online {
        return RealOutcome::Refused("online Z_REPORT requires an Online node".into());
    }
    let row = ctx.seed_inbox_z_report().await;
    let dps = ctx.new_dps();
    load_script(&dps, script);
    let guard = ctx.gate.clone().lock_owned().await;
    let result = inline::run(
        &ctx.pool,
        &ctx.pool_secure,
        &dps,
        &ctx.sign_ctx,
        &ctx.fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await;
    drop(guard);
    match result {
        Ok(_outcome) => {
            let observed = ctx.observe_doc_by_request_id(&row.request_id).await;
            ctx.last_row = Some(row);
            RealOutcome::Doc(observed)
        }
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// `OfflineZReport` → live inline Z dispatcher on an Offline node.  The Z doc
/// local-acks and moves the shift to ClosingLocalPendingDrain; Drain/GoOnline
/// owns the later wire submission.
async fn offline_z_report(ctx: &mut FuzzCtx) -> RealOutcome {
    if ctx.read_node_mode().await != NodeMode::Offline {
        return RealOutcome::Refused("offline Z_REPORT requires an Offline node".into());
    }
    // B10: the first offline doc interposes a lazy BEGIN — report `Recovered`
    // so the differential routes to the two-doc ledger-delta (`offline_sell`).
    let begin_before = begin_doc_count(ctx).await;
    let row = ctx.seed_inbox_z_report().await;
    let dps = ctx.new_dps();
    let guard = ctx.gate.clone().lock_owned().await;
    let result = inline::run(
        &ctx.pool,
        &ctx.pool_secure,
        &dps,
        &ctx.sign_ctx,
        &ctx.fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await;
    drop(guard);
    match result {
        Ok(_outcome) => {
            ctx.last_row = Some(row.clone());
            if begin_doc_count(ctx).await > begin_before {
                return RealOutcome::Recovered {
                    branch: "b10_lazy_begin_interposed".into(),
                };
            }
            RealOutcome::Doc(ctx.observe_doc_by_request_id(&row.request_id).await)
        }
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// `GoOnline` → the REAL transition seam: `return_online_probe::run_tick_for_fn`
/// (Offline → GoingOnline via `status_rro`) THEN `backlog_drain::drain`
/// (GoingOnline → Online, draining the backlog).  NOT a setter.
async fn go_online(ctx: &mut FuzzCtx, script: &DpsScript) -> RealOutcome {
    let dps = ctx.new_dps();
    dps.push_status(Ok(online_status())); // probe sees DPS online → flip
    let backlog = ctx.full_drain_cohort_count().await; // M1: full cohort, not OLA-only
    load_drain_script(&dps, script, backlog); // one send/last per cohort doc

    let tick =
        return_online_probe::run_tick_for_fn(&ctx.pool, &dps, &ctx.fn_id, &ctx.fn_sign).await;

    let guard = drain_test_guard();
    let view = ctx.view(&dps);
    let drain = backlog_drain::drain(&guard, &ctx.pool, &view, &ctx.fn_id).await;
    RealOutcome::Recovered {
        branch: format!(
            "tick={tick:?} drain={}",
            match &drain {
                Ok(s) => format!(
                    "ok(backlog={},acked={})",
                    s.backlog_size_before(),
                    s.advanced_to_ack()
                ),
                Err(e) => format!("err({e:?})"),
            }
        ),
    }
}

/// `Drain` → `backlog_drain::drain` (requires GoingOnline; otherwise a logged
/// no-op with `backlog_size_before = 0`).
async fn drain_op(ctx: &mut FuzzCtx, script: &DpsScript) -> RealOutcome {
    let dps = ctx.new_dps();
    let backlog = ctx.full_drain_cohort_count().await; // M1: full cohort, not OLA-only
    load_drain_script(&dps, script, backlog); // one send/last per cohort doc
    let guard = drain_test_guard();
    let view = ctx.view(&dps);
    match backlog_drain::drain(&guard, &ctx.pool, &view, &ctx.fn_id).await {
        Ok(s) => RealOutcome::Recovered {
            branch: format!(
                "drain ok(backlog={},acked={})",
                s.backlog_size_before(),
                s.advanced_to_ack()
            ),
        },
        Err(e) => RealOutcome::Refused(format!("drain: {e:?}")),
    }
}

/// Terminal-recovery drain-tick (A4 settle): drive a REAL backlog drain with Ack
/// responses sized to the REAL cohort (`list_drain_candidates_for_fn_ordered_by_lnd`
/// via `drain_cohort_len`, NOT an OFFLINE_LOCAL_ACK-only undercount),
/// simulating DPS coming back so the WHOLE offline cohort — including re-driven
/// `ERROR_RETRYABLE` / `SENT` docs left by a prior exotic drain — drains to ACK
/// and finalize CAS's `GoingOnline → Online`.  One Ack send/last per cohort doc
/// is ample (a re-driven doc needs at most a send + a last; a probe needs fewer,
/// and unused queue entries are ignored).
pub async fn settle_drain_tick(ctx: &mut FuzzCtx) -> RealOutcome {
    let cohort = match ctx.active_offline_session().await {
        Some(sid) => ctx.drain_cohort_len(sid).await,
        None => 0,
    };
    let dps = ctx.new_dps();
    // B10: `+ 1` for the drain-time DocType=10 END (minted DURING the drain, not
    // in `cohort`) so its wire submit lands ACK and the drain can FINALIZE.
    for _ in 0..cohort + 1 {
        dps.push_send(wire_to_result(WireResponse::Ack));
        dps.push_last(wire_to_result(WireResponse::Ack));
    }
    let guard = drain_test_guard();
    let view = ctx.view(&dps);
    match backlog_drain::drain(&guard, &ctx.pool, &view, &ctx.fn_id).await {
        Ok(s) => RealOutcome::Recovered {
            branch: format!(
                "settle_drain ok(backlog={},acked={})",
                s.backlog_size_before(),
                s.advanced_to_ack()
            ),
        },
        Err(e) => RealOutcome::Refused(format!("settle_drain: {e:?}")),
    }
}

/// O1 — drive an online-convergence tick (`online_convergence::run_tick_for_fn`)
/// with the given ordered `last_chk` responses.  The seam is `Online`-only
/// (mode-guarded internally) and issues only `last_chk` (no fresh send): a
/// resting `SENT` doc cascades `SENT → (probe Match) → KVT1 → (confirm Match) →
/// ACK` within one tick (2 `last_chk` per doc).  Mirrors the offline
/// `settle_drain_tick`, for the online lane.
pub async fn run_convergence_tick_with(
    ctx: &FuzzCtx,
    last_responses: &[WireResponse],
) -> anyhow::Result<online_convergence::TickSummary> {
    let dps = ctx.new_dps();
    for wr in last_responses {
        dps.push_last(wire_to_result(*wr));
    }
    let view = ctx.view(&dps);
    online_convergence::run_tick_for_fn(&ctx.pool, &view, &ctx.fn_id).await
}

/// O1 — Ack/Match-loaded convergence tick sized to the resting `SENT`/`KVT1`
/// cohort (one probe + one confirm `last_chk` per resting doc, + slack).  The
/// settle-time analogue of `settle_drain_tick`: simulates DPS confirming, so
/// every Match-able resting online doc converges to ACK.
pub async fn settle_convergence_tick(
    ctx: &FuzzCtx,
) -> anyhow::Result<online_convergence::TickSummary> {
    let resting = ctx.resting_online_doc_count().await;
    let acks = vec![WireResponse::Ack; 2 * resting + 2];
    run_convergence_tick_with(ctx, &acks).await
}

/// O1 negative-tooth helper — a convergence tick whose `last_chk` returns the K4
/// Hold form (empty `data_sign`): a resting `SENT` doc legitimately HOLDS (no
/// Match evidence yet).  The convergence assert must NOT flag this.
pub async fn convergence_tick_holds(
    ctx: &FuzzCtx,
) -> anyhow::Result<online_convergence::TickSummary> {
    run_convergence_tick_with(ctx, &[WireResponse::NotFound]).await
}

/// `SellWithClosedShift` (invalid intent): close the shift, then attempt a SELL
/// — the dispatcher refuses (ShiftNotOpen / ShiftGuardRefused).  No assertion of
/// no-mutation here (that is Task 4); the bar is a typed refusal, no panic.
async fn sell_with_closed_shift(ctx: &mut FuzzCtx) -> RealOutcome {
    ctx.force_shift_closed().await;
    let row = ctx.seed_inbox_sell().await;
    let dps = ctx.new_dps();
    let guard = ctx.gate.clone().lock_owned().await;
    let result = inline::run(
        &ctx.pool,
        &ctx.pool_secure,
        &dps,
        &ctx.sign_ctx,
        &ctx.fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await;
    drop(guard);
    match result {
        Ok(_) => RealOutcome::Doc(ctx.observe_doc_by_request_id(&row.request_id).await),
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// `OfflineSellDuringGoingOnline` (invalid intent): force GoingOnline, then
/// attempt a SELL — the dispatcher refuses (mode is mid-transition).
async fn offline_sell_during_going_online(ctx: &mut FuzzCtx) -> RealOutcome {
    // CS-3 S7-1: a HELD doc's invariant-scan exemption depends on the node staying halted
    // (STOP_MODE|BLOCKED) — a halt is cleared ONLY by operator completion, never spontaneously.
    // This adversarial op forces GoingOnline to test the mid-transition sell refusal, but it must
    // NOT un-halt an already-halted node (prod never does), else it would strand a legitimately-
    // held SENDING doc under a non-halted mode. When already halted, leave the halt and run the
    // sell on it — a STOP/BLOCKED node refuses the sell just the same.
    let mode: String = sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = ?")
        .bind(ctx.fn_id.as_str())
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
    if !matches!(mode.as_str(), "STOP_MODE" | "BLOCKED") {
        ctx.force_node_mode(NodeMode::GoingOnline).await;
    }
    let row = ctx.seed_inbox_sell().await;
    let dps = ctx.new_dps();
    let guard = ctx.gate.clone().lock_owned().await;
    let result = inline::run(
        &ctx.pool,
        &ctx.pool_secure,
        &dps,
        &ctx.sign_ctx,
        &ctx.fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await;
    drop(guard);
    match result {
        Ok(_) => RealOutcome::Doc(ctx.observe_doc_by_request_id(&row.request_id).await),
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// `DuplicateIdemKey` (replay): process a SELL, then re-run `inline::run` on the
/// SAME inbox row — the second pass finds the row no longer NEW, takes the
/// idempotent Noop → resolve-against-ledger path, and mints no new doc.
async fn duplicate_idem_key(ctx: &mut FuzzCtx) -> RealOutcome {
    // Replay the LAST successfully-issued row.  It is already DONE, so
    // `inline::run` takes the idempotent Noop → resolve-against-ledger path and
    // mints NO new doc (no issuance, no seed/code advance) — a true replay.
    let Some(row) = ctx.last_row.clone() else {
        // Nothing issued yet to replay — a no-op refusal.
        return RealOutcome::Refused(
            "duplicate_idem_key: no prior issued request to replay".to_string(),
        );
    };
    let dps = ctx.new_dps(); // a replay resolves from the ledger — no fresh wire
    let guard = ctx.gate.clone().lock_owned().await;
    let result = inline::run(
        &ctx.pool,
        &ctx.pool_secure,
        &dps,
        &ctx.sign_ctx,
        &ctx.fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await;
    drop(guard);
    match result {
        Ok(_) => RealOutcome::Doc(ctx.observe_doc_by_request_id(&row.request_id).await),
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

// ─── DpsScript → ScriptedDps queue routing ──────────────────────────────────

/// Lay a `DpsScript` into the stub's queues: position 0 is the `send_chk`
/// response (`push_send`); positions 1+ are subsequent `last_chk` probes
/// (`push_last`).  Matches `AckPath = [Ack, Ack]` (send→Ack, last→Ack).
fn load_script(dps: &ScriptedDps, script: &DpsScript) {
    // Element 0 is the SEND response; every later element is a `lastChk`.
    //
    // A `RetrySend` boundary marker was added here to express a SECOND send —
    // the bounded MAC-recovery attempt #2 — and then removed: CS-3 S7-1 (R3)
    // retired that retry, so a `-12` HOLDS and no production path makes a second
    // send within one op. The grammar is one-send-then-lastChk because
    // production is. If a future slice reintroduces a retry, that marker is the
    // shape to bring back — see `bd PRRO_GATE-3uo` for why it was premature.
    for (i, wr) in script.0.iter().copied().enumerate() {
        match (i, wr) {
            // CS-3 Slice E: an UnknownStatus send response MUST reach the wire through the REAL
            // production decode (`observe_check_reply` via `scripted_raw_observation`), NOT a legacy
            // `DpsError` (which `observe_faithful_from_legacy` degrades to NoResponse, losing the
            // ProbeRequired classification). Push the legacy for the send counter/pop + the
            // real-decode observation as the override the stub returns.
            (0, WireResponse::UnknownStatus(code)) => {
                let (legacy, obs) = unknown_status_pair(code);
                dps.push_send(legacy);
                dps.push_send_obs_override(obs);
            }
            (0, _) => dps.push_send(wire_to_result(wr)),
            (_, _) => dps.push_last(wire_to_result(wr)),
        }
    }
}

/// CS-3 Slice E: the `(legacy, real-observation)` pair for an `UnknownStatus(code)` leaf — a raw
/// `gen::CheckResponse{ status: code }` fed through the PRODUCTION `observe_check_reply` decode. This
/// is the ONLY faithful path to the `UnknownStatus → ProbeRequired` leaf; a hand-built legacy
/// `DpsError::Indeterminate` degrades to `NoResponse` in `observe_faithful_from_legacy`.
fn unknown_status_pair(
    code: i32,
) -> (
    Result<CheckAck, DpsError>,
    prro::transports::dps::raw_reply::RawSendObservation,
) {
    prro::transports::dps::dto::scripted_raw_observation(
        prro::transports::dps::gen::CheckResponse {
            id: String::new(),
            status: code,
            id_sign: Vec::new(),
            data_sign: Vec::new(),
            error_message: String::new(),
        },
    )
}

/// Lay a drain's wire responses PER cohort doc (a drain submits + confirms each
/// backlog doc in turn, so one `send` + one `last` per doc).  Mirrors the model:
///   - AckPath  → each doc: send→Ack, last→Ack (the whole backlog → ACK);
///   - Reject   → the first doc's send rejects → strict-sequential halt (no
///     further sends), so a single send response suffices;
///   - otherwise → exotic; the model defers to Fault (the harness re-syncs and
///     does NOT differential-check it), so a best-effort lay is fine.
fn load_drain_script(dps: &ScriptedDps, script: &DpsScript, backlog: usize) {
    match script.0.as_slice() {
        [WireResponse::Ack, WireResponse::Ack, ..] => {
            // B10: `backlog` counts the pre-drain cohort; a full AckPath drain
            // ALSO mints + sends the DocType=10 END LAST → push ONE extra
            // send/last pair so the END's wire submit lands ACK (not
            // ErrorRetryable from an empty queue).  A surplus pair is harmless
            // when no END mints (leftover queued responses are ignored).
            for _ in 0..backlog + 1 {
                dps.push_send(wire_to_result(WireResponse::Ack));
                dps.push_last(wire_to_result(WireResponse::Ack));
            }
        }
        [WireResponse::Reject, ..] => {
            dps.push_send(wire_to_result(WireResponse::Reject));
        }
        _ => load_script(dps, script),
    }
}

/// Map one `WireResponse` to the transport `Result`.  Task 2 exercises the
/// `AckPath` only; the reject / timeout / superseded / bad-hash-prev / not-found
/// constructions are defined AND verified in Task 4 (the differential), where
/// they can be checked against the real seam's routing rather than guessed.
/// (`Timeout` is realized via `Crash` drop-injection, not a queued result.)
/// Lowercase hex, for the live-captured `-12` message shape below.
fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn wire_to_result(wr: WireResponse) -> Result<CheckAck, DpsError> {
    match wr {
        // Full ack: send → Sent; lastChk Match → ACK. The KVT1 evidence must be
        // ≥ MIN_KVT1_DATA_SIGN_LEN (64) to pass the plausibility floor (RISK 1
        // harden); a real DSTU CMS quittance is far larger.
        WireResponse::Ack => Ok(ack(SERVER_FISCAL_NO, vec![0xDE; 64])),
        // Empty data_sign on a lastChk → the K4 Hold form (doc rests at SENT).
        WireResponse::NotFound => Ok(ack(SERVER_FISCAL_NO, Vec::new())),
        // Per-document reject → Sending → Rejected (DPS code -1, ERROR_VEREFY).
        WireResponse::Reject => Err(DpsError::Authorization {
            code: -1,
            kind: AuthorizationKind::DocumentReject,
            message: "fuzz: document reject".to_string(),
        }),
        // Server tip superseded → ServerFiscalIdMismatch → ErrorRetryable.
        WireResponse::Superseded => Err(DpsError::ServerFiscalIdMismatch {
            expected_id: SERVER_FISCAL_NO.to_string(),
            actual_id: "DPS-FN-SUPERSEDED".to_string(),
        }),
        // Bad previous-hash chain link → Server(-12) ERROR_BAD_HASH_PREV → the
        // bounded automatic MAC recovery.
        //
        // The message shape is LIVE-CAPTURED, not invented (2026-07-31 against
        // the DPS test cabinet, bd PRRO_GATE-2ds — observed twice on different
        // hashes). Note the TWO spaces after the code name:
        //
        //     ERROR_BAD_HASH_PREV  store <64 hex> chk <64 hex>
        //
        // This matters: `mac_recovery::regex_extract_store_hash` reads the
        // literal `"store "` tag. Until this carried one, the stub emitted a
        // bare `"ERROR_BAD_HASH_PREV"`, extraction ALWAYS failed, and every
        // generated `-12` explored only the `HashNotExtractable` branch — the
        // recovery SUCCESS path had zero generative coverage. A RED-first test
        // (`mac_recovery_drives_attempt_two_to_ack`) pinned that: the doc rested
        // at SENDING instead of reaching ACK.
        //
        // `store` carries [`DPS_RECOVERY_TIP`] — the tip this simulated peer
        // claims. `chk` is what the client sent; the extractor does not read it,
        // and the stub does not know it, so it is zero-filled and documented
        // rather than faked into looking meaningful.
        WireResponse::BadHashPrev => Err(DpsError::Server {
            code: -12,
            message: format!(
                "ERROR_BAD_HASH_PREV  store {} chk {}",
                hex_lower(&crate::op::DPS_RECOVERY_TIP),
                hex_lower(&[0u8; 32]),
            ),
        }),
        // The timeout SCENARIO is realized via Crash(Send|Kvt1) drop-injection,
        // not a queued result — the generator never puts Timeout in a loaded
        // script.  This defensive mapping keeps wire_to_result total + panic-free
        // (a Transport error is the real seam's back-off-and-retry signal).
        WireResponse::Timeout => Err(DpsError::Transport(
            "fuzz: simulated timeout (normally realized via Crash drop-injection)".to_string(),
        )),
        // CS-3 Slice E: the legacy half of an UnknownStatus leaf. The send path routes it via
        // `load_script`'s obs-override (real decode); this defensive arm keeps `wire_to_result` total
        // for any last_chk position (unused by the shipped scripts — UnknownStatus is send-only).
        WireResponse::UnknownStatus(code) => unknown_status_pair(code).0,
    }
}

// ─── Helpers (re-created from kill_point_matrix.rs fixtures) ─────────────────

fn ack(id: &str, data_sign: Vec<u8>) -> CheckAck {
    CheckAck {
        id: id.to_string(),
        id_sign: vec![],
        data_sign,
    }
}

/// The `status_rro` snapshot the return-online probe needs to flip
/// Offline → GoingOnline (DPS reports the FN online with an open shift).
fn online_status() -> StatusSnapshot {
    StatusSnapshot {
        open_shift: true,
        online: true,
        last_signer: String::new(),
    }
}

fn fn_sign_blob() -> CheckSignBlob {
    CheckSignBlob(vec![0xAB, 0xCD])
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

/// Returns the pool **and** its backing `TempDir` guard. The caller (`FuzzCtx`)
/// must hold the guard for the pool's lifetime: dropping it removes the per-case
/// DB directory (RAII), replacing the old `std::mem::forget` leak.
/// Boot a REAL `App` for one fuzz case.  Both databases live under a single per-case temp dir whose
/// guard the caller (`FuzzCtx`) holds, so cleanup stays RAII.  The fixture takes `pool` /
/// `pool_secure` from `app.db()` / `app.db_secure()` so every existing op runs against the SAME
/// database the App owns — that is what lets the interpreter drive production services (e.g. the
/// T=112 `OfflineCodeReplenishService`, which needs an `App` for the per-FN write gate) instead of
/// re-implementing their effects, which would make the oracle vacuous.
///
/// `App::boot` spawns NO background tasks (the tickers live in `runtime::supervisor::run`, started
/// only by the `serve` subcommand), so this does not perturb fuzzer determinism.
async fn boot_fuzz_app(base: Option<&Path>) -> (prro::App, tempfile::TempDir) {
    let dir = match base {
        Some(b) => tempfile::Builder::new()
            .prefix("fuzz-app-")
            .tempdir_in(b)
            .unwrap(),
        None => tempfile::tempdir().unwrap(),
    };
    let db_path = dir
        .path()
        .join("fuzz.db")
        .display()
        .to_string()
        .replace('\\', "/");
    let toml_text = format!(
        r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "{db_path}"
secure_db_path = "{db_path}_secure"

[admin_ui]
enabled = false
listen  = "127.0.0.1:8443"
"#
    );
    let cfg = prro::config::AppConfig::from_toml(&toml_text).unwrap();
    let app = prro::App::boot(cfg).await.unwrap();
    (app, dir)
}

async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fuzz.db");
    let pool = open_pool(&path).await.unwrap();
    (pool, dir)
}

async fn fresh_secure_pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fuzz-secure.db");
    let pool = open_secure_pool(&path).await.unwrap();
    (pool, dir)
}

async fn fresh_pool_in(base: &Path) -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::Builder::new()
        .prefix("fuzz-db-")
        .tempdir_in(base)
        .unwrap();
    let path = dir.path().join("fuzz.db");
    let pool = open_pool(&path).await.unwrap();
    (pool, dir)
}

async fn fresh_secure_pool_in(base: &Path) -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::Builder::new()
        .prefix("fuzz-secure-db-")
        .tempdir_in(base)
        .unwrap();
    let path = dir.path().join("fuzz-secure.db");
    let pool = open_secure_pool(&path).await.unwrap();
    (pool, dir)
}

async fn seed_fn_config(pool: &SqlitePool) {
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
}

async fn seed_open_shift(pool: &SqlitePool) -> ShiftId {
    let shift_id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, serial, state, open_mode, \
            cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED', 'ONLINE', 0, ?)",
    )
    .bind(DbShiftId(shift_id))
    .bind(FN)
    .bind(CASHIER)
    .execute(pool)
    .await
    .unwrap();
    shift_id
}

async fn seed_node_state(pool: &SqlitePool, mode: NodeMode, shift_id: ShiftId) {
    seed_node_state_with_shift(pool, mode, ShiftState::Opened, Some(shift_id)).await;
}

async fn seed_node_state_with_shift(
    pool: &SqlitePool,
    mode: NodeMode,
    shift_state: ShiftState,
    current_shift_id: Option<ShiftId>,
) {
    sqlx::query(
        "INSERT INTO node_state \
         (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
          backend_profile_id, transport_profile_id) \
         VALUES (?, ?, ?, ?, 1, 'b', 't')",
    )
    .bind(FN)
    .bind(mode.as_str())
    .bind(shift_state.as_str())
    .bind(current_shift_id.map(DbShiftId))
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_open_offline_session(pool: &SqlitePool) {
    let session_id = OfflineSessionId::new();
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, ?, '2026-06-09T00:00:00Z')",
    )
    .bind(DbOfflineSessionId(session_id))
    .bind(FN)
    .bind(OfflineSessionState::Open.as_str())
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_offline_code(pool: &SqlitePool, code_lnd: i64) {
    // B8-1: acquire_code_tx requires dps_code IS NOT NULL; use synthetic codes.
    let dps_code = format!("DRILL-{code_lnd}");
    sqlx::query("INSERT INTO offline_codes(fiscal_number, code_lnd, dps_code) VALUES (?, ?, ?)")
        .bind(FN)
        .bind(code_lnd)
        .bind(&dps_code)
        .execute(pool)
        .await
        .unwrap();
}

/// Seed an inbox row for a check-class op (`operation_type` = `"SELL"` or
/// `"RETURN"`).  The payload body (`SELL_PAYLOAD`) is the shared converted
/// CheckJson — SELL and RETURN carry the identical `{items,payments}` shape at
/// the write-path layer; the direction is carried by `operation_type` (→
/// `DocType::Sell` / `DocType::Return` in `build_canonical`), not the body.
async fn seed_inbox_keyed(pool: &SqlitePool, idem: &str, operation_type: &str) -> InboxRow {
    seed_inbox_keyed_payload(pool, idem, operation_type, SELL_PAYLOAD, Some(TOTAL_KOP)).await
}

async fn seed_inbox_keyed_payload(
    pool: &SqlitePool,
    idem: &str,
    operation_type: &str,
    payload_json: &str,
    total_sum_kop: Option<i64>,
) -> InboxRow {
    let req_id = RequestId::new();
    let request_id: [u8; 16] = *req_id.as_bytes();
    let payload_sha256_canonical: [u8; 32] = Sha256::digest(payload_json.as_bytes()).into();
    inbox::insert(
        pool,
        &NewInboxEntry {
            request_id,
            fiscal_number: FN.into(),
            protocol: Protocol::Rest,
            operation_type: operation_type.into(),
            idempotency_key: idem.into(),
            payload_json: payload_json.into(),
            payload_sha256_canonical,
            correlation_id: None,
            signed_by_cashier_id: Some(CASHIER.into()),
            driver_id: Some(DRIVER.into()),
            business_ts: Some("2026-06-09T12:00:00Z".into()),
            total_sum_kop,
        },
    )
    .await
    .unwrap();
    InboxRow {
        request_id,
        fiscal_number: FN.into(),
        protocol: Protocol::Rest,
        operation_type: operation_type.into(),
        idempotency_key: idem.into(),
        status: "NEW".into(),
        payload_json: payload_json.into(),
        payload_sha256_canonical,
        correlation_id: None,
        received_at: "2026-06-09T12:00:00Z".into(),
        signed_by_cashier_id: Some(CASHIER.into()),
        driver_id: Some(DRIVER.into()),
        business_ts: Some("2026-06-09T12:00:00Z".into()),
        total_sum_kop,
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

/// Phase-2 U1 (spec §3 / acceptance A1): per-case temp DBs are cleaned when
/// their owning `FuzzCtx` drops — no `std::mem::forget` leak. Measured in an
/// isolated base dir so the count reflects only this harness, not global /tmp
/// noise.  The test does not mutate process-global `TMPDIR`, so it is safe under
/// ordinary parallel `cargo test` as well as nextest.
#[tokio::test]
async fn fuzz_ctx_drop_cleans_per_case_temp_dbs() {
    let base = tempfile::tempdir().unwrap();

    let count = || std::fs::read_dir(base.path()).unwrap().count();
    assert_eq!(count(), 0, "isolated temp base must start empty");

    // Create + drop many ctxs. With the `mem::forget` leak each iteration
    // forgets two `TempDir`s (pool + pool_secure) → the dir count grows
    // monotonically (32 leaked dirs after 16 iterations). Under RAII the count
    // returns to zero after every drop.
    for _ in 0..16 {
        let ctx = FuzzCtx::new_online_open_shift_in(base.path()).await;
        drop(ctx);
    }

    let leaked = count();

    assert_eq!(
        leaked, 0,
        "FuzzCtx drop must remove every per-case temp DB dir (no mem::forget)"
    );
}

#[tokio::test]
async fn valid_three_op_online_sell_sequence_all_reach_ack() {
    let mut ctx = FuzzCtx::new_online_open_shift().await;

    for i in 1..=3 {
        let out = run_op(&mut ctx, &Op::OnlineSell(DpsScript::ack_path())).await;
        match out {
            RealOutcome::Doc(doc) => {
                assert_eq!(doc.lnd, i, "lnd advances 1,2,3 across the sequence");
                assert_eq!(
                    doc.doc_state,
                    DocState::Ack,
                    "an online SELL on the AckPath lands ACK end-to-end"
                );
            }
            other => panic!("op {i}: expected Doc(ACK), got {other:?}"),
        }
    }
    assert_eq!(ctx.observed_doc_count().await, 3, "three issued docs");
}

#[tokio::test]
async fn crash_send_then_reboot_recovers_without_panic_or_resend() {
    let mut ctx = FuzzCtx::new_online_open_shift().await;

    let crashed = run_op(&mut ctx, &Op::Crash(Stage::Send)).await;
    match &crashed {
        RealOutcome::Crashed {
            stage,
            committed_state,
        } => {
            assert_eq!(*stage, Stage::Send);
            assert_eq!(
                *committed_state,
                Some(DocState::Sending),
                "crash@send leaves SENDING durably committed (Pattern B intent marker)"
            );
        }
        other => panic!("expected Crashed{{Send}}, got {other:?}"),
    }
    assert_eq!(ctx.send_calls(), 1, "exactly one send_chk before the crash");

    // Reboot recovery must not panic the interpreter (drop-injection + boot-recon).
    let _ = run_op(&mut ctx, &Op::Reboot).await;

    assert_eq!(
        ctx.only_doc_state().await,
        DocState::ErrorRetryable,
        "the Sending arm downgrades to ERROR_RETRYABLE (HoldIndeterminate, no resend)"
    );
    assert_eq!(
        ctx.send_calls(),
        1,
        "send_chk total stays 1 across crash + reboot — auto-resend is forbidden"
    );
}

// ── Task 3 Part A — directed per-arm tests for the completed run_op arms ─────

#[tokio::test]
async fn offline_sell_lands_offline_local_ack() {
    // B10: the first offline sell lazily mints a DocType=9 BEGIN (code#1) before
    // the SELL (code#2).  T2 close-reserve: the first offline sell is admitted only
    // while `free >= 1 + reserve(BEGIN+Z=2)` = 3, so seed 3 codes (BEGIN + SELL
    // consume 2, one stays reserved for the eventual offline Z); the op reports
    // `Recovered` (two-doc ledger delta) and BOTH docs rest OFFLINE_LOCAL_ACK.
    let mut ctx = FuzzCtx::new_offline_open_shift(3).await;
    let out = run_op(&mut ctx, &Op::OfflineSell).await;
    assert!(
        matches!(out, RealOutcome::Recovered { .. } | RealOutcome::Doc(_)),
        "expected a Doc/Recovered (interposed BEGIN) offline-local-ack, got {out:?}"
    );
    // Both the lazy BEGIN and the SELL rest OFFLINE_LOCAL_ACK; two codes consumed.
    let ola: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ? AND state = 'OFFLINE_LOCAL_ACK'",
    )
    .bind(ctx.fn_id.as_str())
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(ola, 2, "BEGIN + SELL both at OFFLINE_LOCAL_ACK");
    assert_eq!(
        ctx.count_doc_type("OFFLINE_SESSION_BEGIN").await,
        1,
        "one lazy BEGIN"
    );
    assert_eq!(
        ctx.consumed_codes_count().await,
        2,
        "two offline codes consumed"
    );
    assert_eq!(
        ctx.send_calls(),
        0,
        "offline issuance must NOT touch the wire (neither the BEGIN nor the SELL)"
    );
}

#[tokio::test]
async fn go_online_after_backlog_drains_to_ack() {
    // T2 close-reserve: the first offline sell needs pool >= 3 (BEGIN + SELL + a
    // Z-reserve code) to be admitted; a smaller pool would trip the reserve gate.
    let mut ctx = FuzzCtx::new_offline_open_shift(3).await;
    let _ = run_op(&mut ctx, &Op::OfflineSell).await; // backlog: BEGIN + SELL, both OFFLINE_LOCAL_ACK
    let _ = run_op(&mut ctx, &Op::GoOnline(DpsScript::ack_path())).await;
    assert_eq!(
        ctx.only_doc_state().await,
        DocState::Ack,
        "GoOnline probes (status_rro) Offline→GoingOnline, then drains the backlog to ACK"
    );
}

#[tokio::test]
async fn drain_after_going_online_advances_backlog_to_ack() {
    // T2 close-reserve: the first offline sell needs pool >= 3 to be admitted.
    let mut ctx = FuzzCtx::new_offline_open_shift(3).await;
    let _ = run_op(&mut ctx, &Op::OfflineSell).await;
    ctx.force_node_mode(NodeMode::GoingOnline).await; // fixture setter (test setup)
    let _ = run_op(&mut ctx, &Op::Drain(DpsScript::ack_path())).await;
    assert_eq!(
        ctx.only_doc_state().await,
        DocState::Ack,
        "drain advances the backlog doc to ACK"
    );
}

/// B1/M1 — the drain must provision the wire per the REAL drain cohort
/// (OFFLINE_LOCAL_ACK / SENT / KVT1 / ERROR_RETRYABLE / KVT2), not just
/// OFFLINE_LOCAL_ACK.  A prior partial drain leaves a SENT cohort doc; a
/// follow-up AckPath drain must re-drive it to ACK.  With the OLA-only undercount
/// (an OFFLINE_LOCAL_ACK-only count = 0 for a SENT doc) the AckPath drain
/// under-provisions and the doc is stranded; provisioning per the full cohort
/// re-drives it.
#[tokio::test]
async fn drain_provisions_full_cohort_not_just_offline_local_ack() {
    // T2 close-reserve: the first offline sell needs pool >= 3 to be admitted.
    let mut ctx = FuzzCtx::new_offline_open_shift(3).await;
    let _ = run_op(&mut ctx, &Op::OfflineSell).await; // doc1 OFFLINE_LOCAL_ACK
    ctx.force_node_mode(NodeMode::GoingOnline).await;
    // Partial drain: send→Ack (OLA→Sent), last→NotFound (K4 hold) → doc1 SENT.
    let _ = run_op(
        &mut ctx,
        &Op::Drain(DpsScript::send_ack_then_last_not_found()),
    )
    .await;
    assert_eq!(
        ctx.only_doc_state().await,
        DocState::Sent,
        "the partial drain holds doc1 at SENT (K4 hold)"
    );
    // Follow-up AckPath drain — must re-drive the SENT cohort doc to ACK (M1:
    // provisioned per the full cohort, not the OFFLINE_LOCAL_ACK-only undercount).
    let _ = run_op(&mut ctx, &Op::Drain(DpsScript::ack_path())).await;
    assert_eq!(
        ctx.only_doc_state().await,
        DocState::Ack,
        "M1: the SENT cohort doc is re-driven to ACK (full-cohort provisioning)"
    );
}

#[tokio::test]
async fn sell_with_closed_shift_is_refused() {
    let mut ctx = FuzzCtx::new_online_open_shift().await;
    let out = run_op(&mut ctx, &Op::SellWithClosedShift).await;
    assert!(
        matches!(out, RealOutcome::Refused(_)),
        "a sell against a closed shift must be a typed refusal; got {out:?}"
    );
}

#[tokio::test]
async fn crash_kvt1_leaves_sent_committed() {
    let mut ctx = FuzzCtx::new_online_open_shift().await;
    let out = run_op(&mut ctx, &Op::Crash(Stage::Kvt1)).await;
    match out {
        RealOutcome::Crashed {
            stage,
            committed_state,
        } => {
            assert_eq!(stage, Stage::Kvt1);
            // hang_last parks on the lastChk await AFTER Sending→Sent committed.
            assert_eq!(
                committed_state,
                Some(DocState::Sent),
                "crash@kvt1 (hang_last) leaves SENT durably committed"
            );
        }
        other => panic!("expected Crashed{{Kvt1}}, got {other:?}"),
    }
    assert_eq!(ctx.send_calls(), 1, "one send_chk before the lastChk crash");
}

// ─── L0 cash-ledger differential tests ──────────────────────────────────────
//
// (A) `cash_differential_sell_and_return` — after each issued SELL prod cash
//     increments; after a RETURN with sufficient cash it decrements. The
//     `check_cash_on_hand` oracle asserts prod == model at every step.
//
// (B) `cash_oracle_detects_divergence_and_matches_on_valid_path` — proves the
//     oracle FIRES on divergence (prod negative, model 0) and stays green on
//     the matching valid path.  The L1 guard lives in `convert.rs` (ingress);
//     the dedicated pin `pin_l1_teeth_revert_guard` in `l0_l1_cash_ledger.rs`
//     tests the guard directly.  These fuzzer tests verify the ORACLE layer.

/// L0 cash-differential — SELL builds cash; RETURN decrements; oracle
/// confirms prod == model after every issued op.
///
/// Sequence:
///   SELL₁  → prod 15000, model 15000
///   SELL₂  → prod 30000, model 30000
///   RETURN  → prod 15000, model 15000 (cash sufficient → admitted)
#[tokio::test]
async fn cash_differential_sell_and_return() {
    use crate::model::CASH_AMOUNT_KOP;
    use crate::oracle::check_cash_on_hand;

    let mut ctx = FuzzCtx::new_online_open_shift().await;

    // ── SELL₁ ──────────────────────────────────────────────────────────────
    let out1 = run_op(&mut ctx, &Op::OnlineSell(DpsScript::ack_path())).await;
    assert!(
        matches!(out1, RealOutcome::Doc(_)),
        "SELL₁ must issue; got {out1:?}"
    );
    let model_cash_after_sell1 = CASH_AMOUNT_KOP;
    check_cash_on_hand(&ctx.pool, &ctx.fn_id, model_cash_after_sell1)
        .await
        .expect("cash oracle mismatch after SELL₁");

    // ── SELL₂ ──────────────────────────────────────────────────────────────
    let out2 = run_op(&mut ctx, &Op::OnlineSell(DpsScript::ack_path())).await;
    assert!(
        matches!(out2, RealOutcome::Doc(_)),
        "SELL₂ must issue; got {out2:?}"
    );
    let model_cash_after_sell2 = 2 * CASH_AMOUNT_KOP;
    check_cash_on_hand(&ctx.pool, &ctx.fn_id, model_cash_after_sell2)
        .await
        .expect("cash oracle mismatch after SELL₂");

    // ── RETURN (cash sufficient → admitted) ────────────────────────────────
    let out3 = run_op(&mut ctx, &Op::OnlineReturn(DpsScript::ack_path())).await;
    assert!(
        matches!(out3, RealOutcome::Doc(_)),
        "RETURN with sufficient cash must issue; got {out3:?}"
    );
    let model_cash_after_return = CASH_AMOUNT_KOP; // 30000 − 15000
    check_cash_on_hand(&ctx.pool, &ctx.fn_id, model_cash_after_return)
        .await
        .expect("cash oracle mismatch after RETURN");
}

/// L0 cash-oracle teeth — HOLE 2 update: in-lease guard is now in the lane.
///
/// **Post-HOLE-2 reality**: the in-lease guard (`stage_acquire` Step 6b‴‴) is
/// in the fuzzer's lane for Online mode.  An `OnlineReturn` on an empty drawer
/// is NOW refused in-lease (pre-mint, no row minted, `Refused` outcome).
///
/// This test verifies three things:
///   (a) A RETURN on empty drawer is REFUSED by the in-lease guard (not issued).
///   (b) The valid path (SELL → RETURN) leaves the oracle green at every step.
///   (c) The oracle FIRES when model ≠ prod (teeth check via mismatched
///       model value — tells oracle model=CASH_AMOUNT_KOP when prod=0).
///
/// ★TEETH (generative): disabling the in-lease guard → the
/// `op_sequences_run_without_panic` proptest goes RED (`drive_sequence` calls
/// `check_cash_on_hand` after every op; minimal shrunk input:
/// `[OnlineReturn(DpsScript([Ack, Ack]))]`).  This unit test documents the
/// oracle layer; the proptest is the GENERATIVE teeth.
#[tokio::test]
async fn cash_oracle_detects_divergence_and_matches_on_valid_path() {
    use crate::model::CASH_AMOUNT_KOP;
    use crate::oracle::check_cash_on_hand;

    // ── (a): RETURN on empty drawer is REFUSED by in-lease guard ──────────
    // Post-HOLE-2: stage_acquire Step 6b‴‴ (Online-scoped) is in the fuzzer
    // lane.  A RETURN with cash_on_hand=0 is refused pre-mint.
    let mut ctx = FuzzCtx::new_online_open_shift().await;
    let out_return = run_op(&mut ctx, &Op::OnlineReturn(DpsScript::ack_path())).await;
    assert!(
        matches!(out_return, RealOutcome::Refused(_)),
        "RETURN on empty drawer must be refused by in-lease guard (HOLE 2); got {out_return:?}"
    );
    // Cash unchanged (0) — refusal is pre-mint (no cash delta).
    check_cash_on_hand(&ctx.pool, ctx.fn_id(), 0)
        .await
        .expect("oracle: refused RETURN must leave cash at 0");

    // ── (b): valid path (SELL → RETURN) — oracle stays green ──────────────
    let mut ctx2 = FuzzCtx::new_online_open_shift().await;
    let out_sell = run_op(&mut ctx2, &Op::OnlineSell(DpsScript::ack_path())).await;
    assert!(
        matches!(out_sell, RealOutcome::Doc(_)),
        "SELL must issue; got {out_sell:?}"
    );
    check_cash_on_hand(&ctx2.pool, ctx2.fn_id(), CASH_AMOUNT_KOP)
        .await
        .expect("oracle after SELL: prod==model==CASH_AMOUNT_KOP");
    let out_return2 = run_op(&mut ctx2, &Op::OnlineReturn(DpsScript::ack_path())).await;
    assert!(
        matches!(out_return2, RealOutcome::Doc(_)),
        "RETURN with sufficient cash must issue; got {out_return2:?}"
    );
    check_cash_on_hand(&ctx2.pool, ctx2.fn_id(), 0)
        .await
        .expect("oracle after valid RETURN: prod==model==0");

    // ── (c): oracle catches divergence (deliberate model mismatch) ─────────
    // ★TEETH: if check_cash_on_hand is weakened to always return Ok(()), this
    // expect_err fires and the teeth go RED.
    // After SELL+RETURN, prod cash=0.  Tell oracle model=CASH_AMOUNT_KOP →
    // divergence: prod(0) != model(15000).
    let divergence_result = check_cash_on_hand(&ctx2.pool, ctx2.fn_id(), CASH_AMOUNT_KOP).await;
    assert!(
        divergence_result.is_err(),
        "oracle must detect divergence when model != prod (teeth check)"
    );
}

// ─── HOLE 2 in-lease cash-floor re-check tests ──────────────────────────────
//
// These tests drive the write-path (inline::run) directly, bypassing the
// pre-inbox L1 guard in convert.rs.  They test the in-lease guard added in
// stage_acquire Step 6b‴‴ — the serialized check that fires under the FN
// write-lease and closes the TOCTOU between concurrent cash RETURNs.

/// HOLE 2 Pin 1 — serial RETURN pair: second is refused in-lease.
///
/// Sequence:
///   SELL   → cash_on_hand = CASH_AMOUNT_KOP
///   RETURN₁ → issued (cash_on_hand → 0)
///   RETURN₂ → REFUSED by in-lease guard (drawer empty after RETURN₁)
///              No fiscal_documents row minted; inbox row REJECTED.
///
/// ★TEETH: disable the in-lease guard → RETURN₂ ISSUES (cash < 0) → this
/// assertion goes RED.
#[tokio::test]
async fn pin_hole2_serial_return_second_refused_in_lease() {
    let mut ctx = FuzzCtx::new_online_open_shift().await;

    // SELL to build cash.
    let sell_out = online_sell(&mut ctx, &DpsScript::ack_path()).await;
    assert!(
        matches!(sell_out, RealOutcome::Doc(_)),
        "SELL must issue; got {sell_out:?}"
    );

    // RETURN₁ — cash sufficient; must issue.
    let return1_out = online_return(&mut ctx, &DpsScript::ack_path()).await;
    assert!(
        matches!(return1_out, RealOutcome::Doc(_)),
        "RETURN₁ must issue (cash sufficient at that point); got {return1_out:?}"
    );

    // RETURN₂ — drawer is now empty; must be REFUSED by the in-lease guard.
    // Note: the pre-inbox L1 guard (convert.rs) is bypassed by online_return
    // (which seeds an inbox row directly); only the in-lease guard catches this.
    let return2_out = online_return(&mut ctx, &DpsScript::ack_path()).await;
    assert!(
        matches!(return2_out, RealOutcome::Refused(_)),
        "RETURN₂ must be refused by in-lease guard (empty drawer after RETURN₁);          got {return2_out:?}"
    );

    // Confirm: no fiscal_documents row was minted for RETURN₂ (row-non-issued).
    // The last ctx.last_row is RETURN₁ (last successful issue); RETURN₂ is the
    // refused one. We verify the total issued doc count = 2 (SELL + RETURN₁).
    let issued_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ?          AND state NOT IN ('REJECTED','ABORTED','CANCELLED')",
    )
    .bind(ctx.fn_id())
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(
        issued_count, 2,
        "exactly 2 docs (SELL + RETURN₁); RETURN₂ minted no row"
    );

    // Confirm the in-lease refusal audit row was written.
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE event_type = 'inv21_cash_insufficient_in_lease'",
    )
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert!(
        audit_count >= 1,
        "audit row for in-lease refusal must be written; got {audit_count}"
    );
}

/// HOLE 2 Pin 2 — in-lease refusal is row-non-issued: no server_fiscal_no,
/// seed unchanged.
///
/// A RETURN on an empty drawer (bypassing L1 pre-inbox) MUST:
///   - produce no fiscal_documents row (inbox REJECTED only)
///   - leave node_state.last_known_unsigned_xml_sha256 unchanged
#[tokio::test]
async fn pin_hole2_in_lease_refusal_is_row_non_issued() {
    let mut ctx = FuzzCtx::new_online_open_shift().await;

    // Read seed before the refused RETURN.
    let seed_before: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number = ?",
    )
    .bind(ctx.fn_id())
    .fetch_optional(&ctx.pool)
    .await
    .unwrap()
    .flatten();

    // RETURN on an empty drawer (no prior SELL) — passes L1 (bypassed) but hits in-lease.
    let out = online_return(&mut ctx, &DpsScript::ack_path()).await;
    assert!(
        matches!(out, RealOutcome::Refused(_)),
        "RETURN on empty drawer must be refused in-lease; got {out:?}"
    );

    // No row in fiscal_documents.
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ?")
            .bind(ctx.fn_id())
            .fetch_one(&ctx.pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "in-lease refusal: no fiscal_documents row minted");

    // Seed unchanged.
    let seed_after: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number = ?",
    )
    .bind(ctx.fn_id())
    .fetch_optional(&ctx.pool)
    .await
    .unwrap()
    .flatten();
    assert_eq!(
        seed_before, seed_after,
        "in-lease refusal: chain seed must not advance"
    );
}

// ─── L3 guard-3b seeded-harness teeth ───────────────────────────────────────
//
// These tests drive `inline::run` directly (bypassing `convert.rs`'s pre-inbox
// guard) and verify the IN-LEASE guard-3b in `stage_acquire`.  ServiceOut on an
// empty drawer must be refused; ServiceIn must issue and build cash.
//
// ★TEETH: disable the in-lease guard for ServiceOut in `stage_acquire` →
// the `pin_guard3b_service_out_refused_in_lease` assertion goes RED.

/// Guard-3b teeth pin 1 — ServiceOut on empty drawer is refused in-lease.
///
/// Sequence:
///   ServiceOut(15000) on empty drawer → Refused (in-lease guard-3b fires)
///   No fiscal_documents row minted; seed unchanged.
///
/// ★TEETH: remove the ServiceOut branch from `stage_acquire`'s in-lease check →
/// this assert turns RED (ServiceOut issues, cash goes negative).
#[tokio::test]
async fn pin_guard3b_service_out_refused_in_lease() {
    use crate::oracle::check_cash_on_hand;

    let mut ctx = FuzzCtx::new_online_open_shift().await;

    // Read seed before the refused ServiceOut.
    let seed_before: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number = ?",
    )
    .bind(ctx.fn_id())
    .fetch_optional(&ctx.pool)
    .await
    .unwrap()
    .flatten();

    // ServiceOut on empty drawer (no prior ServiceIn or Sell): must be refused.
    let out = online_service_out(&mut ctx, &DpsScript::ack_path()).await;
    assert!(
        matches!(out, RealOutcome::Refused(_)),
        "ServiceOut on empty drawer must be refused by in-lease guard-3b; got {out:?}"
    );

    // No row in fiscal_documents.
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ?")
            .bind(ctx.fn_id())
            .fetch_one(&ctx.pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "guard-3b refusal: no fiscal_documents row minted");

    // Seed unchanged.
    let seed_after: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number = ?",
    )
    .bind(ctx.fn_id())
    .fetch_optional(&ctx.pool)
    .await
    .unwrap()
    .flatten();
    assert_eq!(
        seed_before, seed_after,
        "guard-3b refusal: chain seed must not advance"
    );

    // Cash stays at 0.
    check_cash_on_hand(&ctx.pool, ctx.fn_id(), 0)
        .await
        .expect("oracle: refused ServiceOut must leave cash at 0");
}

/// Guard-3b teeth pin 2 — ServiceIn builds cash; subsequent ServiceOut is admitted.
///
/// Sequence:
///   ServiceIn(15000) → issued (cash_on_hand → 15000)
///   ServiceOut(15000) → issued (cash_on_hand → 0) [guard-3b admits it]
///
/// This confirms the ADMIT path is live (guard-3b is not over-broad).
#[tokio::test]
async fn pin_guard3b_service_in_then_service_out_admitted() {
    use crate::model::CASH_AMOUNT_KOP;
    use crate::oracle::check_cash_on_hand;

    let mut ctx = FuzzCtx::new_online_open_shift().await;

    // ServiceIn → builds cash.
    let out_in = online_service_in(&mut ctx, &DpsScript::ack_path()).await;
    assert!(
        matches!(out_in, RealOutcome::Doc(_)),
        "ServiceIn must issue; got {out_in:?}"
    );
    check_cash_on_hand(&ctx.pool, ctx.fn_id(), CASH_AMOUNT_KOP)
        .await
        .expect("oracle: cash must be CASH_AMOUNT_KOP after ServiceIn");

    // ServiceOut → cash sufficient; must be admitted.
    let out_out = online_service_out(&mut ctx, &DpsScript::ack_path()).await;
    assert!(
        matches!(out_out, RealOutcome::Doc(_)),
        "ServiceOut must issue when cash sufficient; got {out_out:?}"
    );
    check_cash_on_hand(&ctx.pool, ctx.fn_id(), 0)
        .await
        .expect("oracle: cash must be 0 after ServiceIn+ServiceOut");
}
