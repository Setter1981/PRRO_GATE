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

use crate::common::scripted_dps::ScriptedDps;
use crate::common::{det_signing_ctx, drain_test_guard};
use crate::op::{DpsScript, L5Kind, Op, Stage, WireResponse};

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
    _tempdir_secure: tempfile::TempDir,
    sign_ctx: SigningContext,
    fn_sign: CheckSignBlob,
    gate: Arc<tokio::sync::Mutex<()>>,
    fn_id: String,
    send_calls: Arc<AtomicUsize>,
    last_calls: Arc<AtomicUsize>,
    seq: u64,
    /// The last successfully-issued inbox row — replayed (idempotent no-op) by
    /// `DuplicateIdemKey` so a replay mints no NEW doc.
    last_row: Option<InboxRow>,
}

impl FuzzCtx {
    /// Fixture: a fresh DB with an ONLINE node + open shift.
    /// Return the fiscal number for this ctx (used by drive_sequence to call
    /// `check_cash_on_hand` after each op).
    pub fn fn_id(&self) -> &str {
        &self.fn_id
    }

    pub async fn new_online_open_shift() -> Self {
        let (pool, _tempdir) = fresh_pool().await;
        let (pool_secure, _tempdir_secure) = fresh_secure_pool().await;
        seed_fn_config(&pool).await;
        let shift_id = seed_open_shift(&pool).await;
        seed_node_state(&pool, NodeMode::Online, shift_id).await;
        Self {
            pool,
            pool_secure,
            _tempdir,
            _tempdir_secure,
            sign_ctx: det_signing_ctx(),
            fn_sign: fn_sign_blob(),
            gate: Arc::new(tokio::sync::Mutex::new(())),
            fn_id: FN.to_string(),
            send_calls: Arc::new(AtomicUsize::new(0)),
            last_calls: Arc::new(AtomicUsize::new(0)),
            seq: 0,
            last_row: None,
        }
    }

    /// Fixture variant used by the cleanup test: keep all DB tempdirs under a
    /// caller-owned base dir without mutating the process-global `TMPDIR`.
    async fn new_online_open_shift_in(base: &Path) -> Self {
        let (pool, _tempdir) = fresh_pool_in(base).await;
        let (pool_secure, _tempdir_secure) = fresh_secure_pool_in(base).await;
        seed_fn_config(&pool).await;
        let shift_id = seed_open_shift(&pool).await;
        seed_node_state(&pool, NodeMode::Online, shift_id).await;
        Self {
            pool,
            pool_secure,
            _tempdir,
            _tempdir_secure,
            sign_ctx: det_signing_ctx(),
            fn_sign: fn_sign_blob(),
            gate: Arc::new(tokio::sync::Mutex::new(())),
            fn_id: FN.to_string(),
            send_calls: Arc::new(AtomicUsize::new(0)),
            last_calls: Arc::new(AtomicUsize::new(0)),
            seq: 0,
            last_row: None,
        }
    }

    /// Fixture: a fresh DB with an ONLINE node and no open/current shift.
    /// `SHIFT_OPEN` should create and open the shift through stage_acquire.
    pub async fn new_online_closed_shift() -> Self {
        let (pool, _tempdir) = fresh_pool().await;
        let (pool_secure, _tempdir_secure) = fresh_secure_pool().await;
        seed_fn_config(&pool).await;
        seed_node_state_with_shift(&pool, NodeMode::Online, ShiftState::Closed, None).await;
        Self {
            pool,
            pool_secure,
            _tempdir,
            _tempdir_secure,
            sign_ctx: det_signing_ctx(),
            fn_sign: fn_sign_blob(),
            gate: Arc::new(tokio::sync::Mutex::new(())),
            fn_id: FN.to_string(),
            send_calls: Arc::new(AtomicUsize::new(0)),
            last_calls: Arc::new(AtomicUsize::new(0)),
            seq: 0,
            last_row: None,
        }
    }

    /// Fixture: a fresh DB with an OFFLINE node + open shift + an OPEN offline
    /// session carrying `codes` offline codes (the offline lane is fixture-
    /// seeded — there is no go_offline op, spec §5).
    pub async fn new_offline_open_shift(codes: i64) -> Self {
        let (pool, _tempdir) = fresh_pool().await;
        let (pool_secure, _tempdir_secure) = fresh_secure_pool().await;
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
            _tempdir_secure,
            sign_ctx: det_signing_ctx(),
            fn_sign: fn_sign_blob(),
            gate: Arc::new(tokio::sync::Mutex::new(())),
            fn_id: FN.to_string(),
            send_calls: Arc::new(AtomicUsize::new(0)),
            last_calls: Arc::new(AtomicUsize::new(0)),
            seq: 0,
            last_row: None,
        }
    }

    /// Fixture: a fresh DB with an OFFLINE node, no open/current shift, and an
    /// OPEN offline session carrying `codes`.  `SHIFT_OPEN` local-acks.
    pub async fn new_offline_closed_shift(codes: i64) -> Self {
        let (pool, _tempdir) = fresh_pool().await;
        let (pool_secure, _tempdir_secure) = fresh_secure_pool().await;
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
            _tempdir_secure,
            sign_ctx: det_signing_ctx(),
            fn_sign: fn_sign_blob(),
            gate: Arc::new(tokio::sync::Mutex::new(())),
            fn_id: FN.to_string(),
            send_calls: Arc::new(AtomicUsize::new(0)),
            last_calls: Arc::new(AtomicUsize::new(0)),
            seq: 0,
            last_row: None,
        }
    }

    /// Fixture-level setter: force the node mode (used by test setup and by the
    /// deliberately-adverse `OfflineSellDuringGoingOnline` intent).
    pub async fn force_node_mode(&self, mode: NodeMode) {
        sqlx::query("UPDATE node_state SET mode = ? WHERE fiscal_number = ?")
            .bind(mode)
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
        .bind(foreign)
        .bind(self.fn_id.as_str())
        .execute(&self.pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE fiscal_documents SET offline_session_id = ? \
             WHERE fiscal_number = ? AND offline_fiscal_no IS NOT NULL",
        )
        .bind(foreign)
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
        .bind(extra)
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
    pub async fn consumed_codes_count(&self) -> i64 {
        self.read_codes_consumed().await.unwrap_or(0)
    }

    /// The real `node_state.mode` — the harness scan-timing gate reads this to
    /// scan ONLY in a SETTLED mode `{Online, Offline}` (never mid-transition).
    pub async fn read_node_mode(&self) -> NodeMode {
        sqlx::query_scalar::<_, NodeMode>("SELECT mode FROM node_state WHERE fiscal_number = ?")
            .bind(self.fn_id.as_str())
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    /// The real `node_state.shift_state` — the harness reads this BEFORE an op for
    /// the mode-independent AUD-K8-1 teeth (a drain re-tick on an RMR FN must make
    /// no new wire call).
    pub async fn read_shift_state(&self) -> ShiftState {
        sqlx::query_scalar::<_, ShiftState>(
            "SELECT shift_state FROM node_state WHERE fiscal_number = ?",
        )
        .bind(self.fn_id.as_str())
        .fetch_one(&self.pool)
        .await
        .unwrap()
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
        None => RealOutcome::Crashed {
            stage,
            committed_state: ctx.observe_doc_state_by_request_id(&row.request_id).await,
        },
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
    ctx.force_node_mode(NodeMode::GoingOnline).await;
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
    for (i, wr) in script.0.iter().copied().enumerate() {
        let result = wire_to_result(wr);
        if i == 0 {
            dps.push_send(result);
        } else {
            dps.push_last(result);
        }
    }
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
fn wire_to_result(wr: WireResponse) -> Result<CheckAck, DpsError> {
    match wr {
        // Full ack: send → Sent; lastChk Match → ACK.
        WireResponse::Ack => Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF])),
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
        // Bad previous-hash chain link → Server(-12) ERROR_BAD_HASH_PREV → MAC
        // recovery / ErrorRetryable.
        WireResponse::BadHashPrev => Err(DpsError::Server {
            code: -12,
            message: "ERROR_BAD_HASH_PREV".to_string(),
        }),
        // The timeout SCENARIO is realized via Crash(Send|Kvt1) drop-injection,
        // not a queued result — the generator never puts Timeout in a loaded
        // script.  This defensive mapping keeps wire_to_result total + panic-free
        // (a Transport error is the real seam's back-off-and-retry signal).
        WireResponse::Timeout => Err(DpsError::Transport(
            "fuzz: simulated timeout (normally realized via Crash drop-injection)".to_string(),
        )),
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
    .bind(shift_id)
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
    .bind(mode)
    .bind(shift_state)
    .bind(current_shift_id)
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
    .bind(session_id)
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
