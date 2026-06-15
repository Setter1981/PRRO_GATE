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

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tokio::sync::oneshot;

use prro::db::models::enums::{
    DocState, FiscalMode, NodeMode, OfflineSessionState, Protocol, ShiftState,
};
use prro::db::models::ids::{OfflineSessionId, RequestId, ShiftId};
use prro::db::repositories::ingress_inbox::{self as inbox, InboxRow, NewInboxEntry};
use prro::db::repositories::{fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig};
use prro::db::{open_pool, open_secure_pool};
use prro::services::offline_sync::{backlog_drain, return_online_probe};
use prro::services::reconciliation::{boot_phase, RuntimeView};
use prro::services::write_path::inline;
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{CheckAck, CheckSignBlob, StatusSnapshot};
use prro::transports::dps::error::{AuthorizationKind, DpsError};

use crate::common::scripted_dps::ScriptedDps;
use crate::common::{det_signing_ctx, drain_test_guard};
use crate::op::{DpsScript, Op, Stage, WireResponse};

// ─── Fixture constants (mirror tests/kill_point_matrix.rs) ──────────────────

const FN: &str = "4000000001";
const CASHIER: &str = "test-cashier";
const DRIVER: &str = "drv-test";
const SERVER_FISCAL_NO: &str = "DPS-FN-ONLINE-1";
const SELL_PAYLOAD: &str = r#"{"items":[{"code":"item-1","name":"Test item","price_kop":15000,"quantity_thousandths":1000,"sum_kop":15000}],"payments":[{"name":"Cash","sum_kop":15000,"type_code":"0"}]}"#;
const TOTAL_KOP: i64 = 15000;

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
    sign_ctx: SigningContext,
    fn_sign: CheckSignBlob,
    gate: Arc<tokio::sync::Mutex<()>>,
    fn_id: String,
    send_calls: Arc<AtomicUsize>,
    last_calls: Arc<AtomicUsize>,
    seq: u64,
}

impl FuzzCtx {
    /// Fixture: a fresh DB with an ONLINE node + open shift.
    pub async fn new_online_open_shift() -> Self {
        let pool = fresh_pool().await;
        let pool_secure = fresh_secure_pool().await;
        seed_fn_config(&pool).await;
        let shift_id = seed_open_shift(&pool).await;
        seed_node_state(&pool, NodeMode::Online, shift_id).await;
        Self {
            pool,
            pool_secure,
            sign_ctx: det_signing_ctx(),
            fn_sign: fn_sign_blob(),
            gate: Arc::new(tokio::sync::Mutex::new(())),
            fn_id: FN.to_string(),
            send_calls: Arc::new(AtomicUsize::new(0)),
            last_calls: Arc::new(AtomicUsize::new(0)),
            seq: 0,
        }
    }

    /// Fixture: a fresh DB with an OFFLINE node + open shift + an OPEN offline
    /// session carrying `codes` offline codes (the offline lane is fixture-
    /// seeded — there is no go_offline op, spec §5).
    pub async fn new_offline_open_shift(codes: i64) -> Self {
        let pool = fresh_pool().await;
        let pool_secure = fresh_secure_pool().await;
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
            sign_ctx: det_signing_ctx(),
            fn_sign: fn_sign_blob(),
            gate: Arc::new(tokio::sync::Mutex::new(())),
            fn_id: FN.to_string(),
            send_calls: Arc::new(AtomicUsize::new(0)),
            last_calls: Arc::new(AtomicUsize::new(0)),
            seq: 0,
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
        seed_inbox_sell_keyed(&self.pool, &idem).await
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

    async fn read_seed(&self) -> Option<Vec<u8>> {
        let v: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number = ?",
        )
        .bind(self.fn_id.as_str())
        .fetch_one(&self.pool)
        .await
        .unwrap();
        v
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
}

// ─── The interpreter ────────────────────────────────────────────────────────

/// Execute one `Op` against the live DB through its real seam and return the
/// observed ledger effect.
pub async fn run_op(ctx: &mut FuzzCtx, op: &Op) -> RealOutcome {
    match op {
        // ── valid ──
        Op::OnlineSell(script) => online_sell(ctx, script).await,
        Op::OfflineSell => offline_sell(ctx).await,
        Op::GoOnline(script) => go_online(ctx, script).await,
        Op::Drain(script) => drain_op(ctx, script).await,
        Op::Reboot => reboot(ctx).await,
        // ── crash (wire stages only — drop-injection) ──
        Op::Crash(Stage::Send) => crash_via_drop(ctx, Stage::Send).await,
        Op::Crash(Stage::Kvt1) => crash_via_drop(ctx, Stage::Kvt1).await,
        // non-wire crash stages need stage-composition; deferred — see plan §4
        // follow-up.  The generator never emits these (Crash drawn from
        // {Send, Kvt1} only), so this arm is NOT reachable from op_sequence().
        Op::Crash(stage) => unimplemented!(
            "Crash({stage:?}) (non-wire stage-composition) is a documented follow-up; \
             the generator only emits Crash(Send) / Crash(Kvt1)"
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
async fn online_sell(ctx: &mut FuzzCtx, script: &DpsScript) -> RealOutcome {
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
    )
    .await;
    drop(guard);
    match result {
        Ok(_outcome) => RealOutcome::Doc(ctx.observe_doc_by_request_id(&row.request_id).await),
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
        Some(Ok(_)) => RealOutcome::Doc(ctx.observe_doc_by_request_id(&row.request_id).await),
        Some(Err(e)) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// `Reboot` → `run_boot_reconciliation`.  The Sending arm is ctx-free
/// (`deps = None`, no wire call), matching kill-matrix K3; deps-bearing reboot
/// (SENT-probe arms) is added when Task 3 sequences it.
async fn reboot(ctx: &mut FuzzCtx) -> RealOutcome {
    // Pass deps so probe-requiring recovery arms (e.g. a SENT doc from a prior
    // Crash(Kvt1)) can run.  The probe dps has empty queues: a SENDING-arm
    // recovery makes no wire call (ctx-free → ERROR_RETRYABLE), while a Sent
    // probe's last_chk on the empty queue returns a typed Err the boot path
    // handles — never a panic.
    let dps = ctx.new_dps();
    let guard = drain_test_guard();
    let view = ctx.view(&dps);
    match boot_phase::run_boot_reconciliation(&guard, &ctx.pool, &ctx.fn_id, Some(&view)).await {
        Ok(branch) => RealOutcome::Recovered {
            branch: format!("{branch:?}"),
        },
        Err(e) => RealOutcome::Refused(format!("reboot: {e:?}")),
    }
}

/// `OfflineSell` → `inline::run` on an Offline node — the offline-ack path lands
/// `OFFLINE_LOCAL_ACK` and makes NO wire call (spec §5).
async fn offline_sell(ctx: &mut FuzzCtx) -> RealOutcome {
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
    )
    .await;
    drop(guard);
    match result {
        Ok(_) => RealOutcome::Doc(ctx.observe_doc_by_request_id(&row.request_id).await),
        Err(e) => RealOutcome::Refused(format!("{e:?}")),
    }
}

/// `GoOnline` → the REAL transition seam: `return_online_probe::run_tick_for_fn`
/// (Offline → GoingOnline via `status_rro`) THEN `backlog_drain::drain`
/// (GoingOnline → Online, draining the backlog).  NOT a setter.
async fn go_online(ctx: &mut FuzzCtx, script: &DpsScript) -> RealOutcome {
    let dps = ctx.new_dps();
    dps.push_status(Ok(online_status())); // probe sees DPS online → flip
    load_script(&dps, script); // drain wire responses (send/last per backlog doc)

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
    load_script(&dps, script);
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
    let row = ctx.seed_inbox_sell().await;
    let dps = ctx.new_dps();
    load_script(&dps, &DpsScript::ack_path()); // for the first pass if it hits the wire
    let guard = ctx.gate.clone().lock_owned().await;
    let first = inline::run(
        &ctx.pool,
        &ctx.pool_secure,
        &dps,
        &ctx.sign_ctx,
        &ctx.fn_sign,
        &guard,
        &row,
    )
    .await;
    if let Err(e) = first {
        // The first pass was refused (out-of-precondition) — no doc to replay
        // against; report the refusal without the (panic-prone) ledger resolve.
        drop(guard);
        return RealOutcome::Refused(format!("{e:?}"));
    }
    let second = inline::run(
        &ctx.pool,
        &ctx.pool_secure,
        &dps,
        &ctx.sign_ctx,
        &ctx.fn_sign,
        &guard,
        &row,
    )
    .await;
    drop(guard);
    match second {
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
        other => panic!("unknown DocState string from ledger: {other:?}"),
    }
}

async fn fresh_pool() -> SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fuzz.db");
    std::mem::forget(dir);
    open_pool(&path).await.unwrap()
}

async fn fresh_secure_pool() -> SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fuzz-secure.db");
    std::mem::forget(dir);
    open_secure_pool(&path).await.unwrap()
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
    sqlx::query(
        "INSERT INTO node_state \
         (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
          backend_profile_id, transport_profile_id) \
         VALUES (?, ?, ?, ?, 1, 'b', 't')",
    )
    .bind(FN)
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
    sqlx::query("INSERT INTO offline_codes(fiscal_number, code_lnd) VALUES (?, ?)")
        .bind(FN)
        .bind(code_lnd)
        .execute(pool)
        .await
        .unwrap();
}

async fn seed_inbox_sell_keyed(pool: &SqlitePool, idem: &str) -> InboxRow {
    let req_id = RequestId::new();
    let request_id: [u8; 16] = *req_id.as_bytes();
    let payload_sha256_canonical: [u8; 32] = Sha256::digest(SELL_PAYLOAD.as_bytes()).into();
    inbox::insert(
        pool,
        &NewInboxEntry {
            request_id,
            fiscal_number: FN.into(),
            protocol: Protocol::Rest,
            operation_type: "SELL".into(),
            idempotency_key: idem.into(),
            payload_json: SELL_PAYLOAD.into(),
            payload_sha256_canonical,
            correlation_id: None,
            signed_by_cashier_id: Some(CASHIER.into()),
            driver_id: Some(DRIVER.into()),
            business_ts: Some("2026-06-09T12:00:00Z".into()),
            total_sum_kop: Some(TOTAL_KOP),
        },
    )
    .await
    .unwrap();
    InboxRow {
        request_id,
        fiscal_number: FN.into(),
        protocol: Protocol::Rest,
        operation_type: "SELL".into(),
        idempotency_key: idem.into(),
        status: "NEW".into(),
        payload_json: SELL_PAYLOAD.into(),
        payload_sha256_canonical,
        correlation_id: None,
        received_at: "2026-06-09T12:00:00Z".into(),
        signed_by_cashier_id: Some(CASHIER.into()),
        driver_id: Some(DRIVER.into()),
        business_ts: Some("2026-06-09T12:00:00Z".into()),
        total_sum_kop: Some(TOTAL_KOP),
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

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
    let mut ctx = FuzzCtx::new_offline_open_shift(1).await;
    let out = run_op(&mut ctx, &Op::OfflineSell).await;
    match out {
        RealOutcome::Doc(d) => {
            assert_eq!(
                d.doc_state,
                DocState::OfflineLocalAck,
                "offline issuance is local"
            );
            assert_eq!(d.code_consumed, Some(1), "one offline code consumed");
        }
        other => panic!("expected Doc(OfflineLocalAck), got {other:?}"),
    }
    assert_eq!(
        ctx.send_calls(),
        0,
        "offline issuance must NOT touch the wire"
    );
}

#[tokio::test]
async fn go_online_after_backlog_drains_to_ack() {
    let mut ctx = FuzzCtx::new_offline_open_shift(1).await;
    let _ = run_op(&mut ctx, &Op::OfflineSell).await; // backlog: one OFFLINE_LOCAL_ACK doc
    let _ = run_op(&mut ctx, &Op::GoOnline(DpsScript::ack_path())).await;
    assert_eq!(
        ctx.only_doc_state().await,
        DocState::Ack,
        "GoOnline probes (status_rro) Offline→GoingOnline, then drains the backlog to ACK"
    );
}

#[tokio::test]
async fn drain_after_going_online_advances_backlog_to_ack() {
    let mut ctx = FuzzCtx::new_offline_open_shift(1).await;
    let _ = run_op(&mut ctx, &Op::OfflineSell).await;
    ctx.force_node_mode(NodeMode::GoingOnline).await; // fixture setter (test setup)
    let _ = run_op(&mut ctx, &Op::Drain(DpsScript::ack_path())).await;
    assert_eq!(
        ctx.only_doc_state().await,
        DocState::Ack,
        "drain advances the backlog doc to ACK"
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
