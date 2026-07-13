//! SHIFT-LIFE MATRIX — full-stack scenario harness through the REAL INGRESS.
//!
//! Operator's directive (2026-07-12): stop verifying the online↔offline
//! transition matrix cell-by-cell with isolated unit tests. Instead **stand the
//! product up**, feed it GENERATED packets of receipts under different shift
//! variants, run each through the WHOLE real path with legible logging, and
//! **halt on the first divergence**. The only thing faked is the network
//! (`MatrixDps` — a stateful, fault-injectable `DpsChannel` whose `online` flag
//! models a real net drop).
//!
//! INGRESS IS UNDER TEST (Increment 2): scenarios drive WIRE-shape
//! `CanonicalCommand`s (the `maria304`/WebCheck wire dialect — `price_kopecks`,
//! `quantity_milli`, `type:"CASH"`) through the REAL ingress core
//! `handler::handle_command` → command-class classify → wire→signer `convert`
//! → idempotent inbox insert → the live `production_write_path` binding →
//! staged pipeline. So convert + classify + inbox are exercised, not bypassed.
//! (`handle_command` is the axum-free core the REST `/v1/ingress/webcheck` route
//! wraps — the same seam a future WebCheck-XML adapter will feed.)
//!
//! WHAT IS REAL: `App::boot` (real SQLite + migrations), the real per-FN signing
//! context, the real registry, the real ingress core, the real staged pipeline,
//! the real offline lane, the real live GO_OFFLINE / GO_ONLINE doors, and the
//! real drain. ONLY the DPS transport is a stub.
//!
//! WHY in-process (not a spawned OS process over HTTP): the transition-matrix
//! goal is state correctness across online↔offline↔drain — faster, determin-
//! istic, and directly assertable in-process. The axum HTTP wire (routing +
//! source-allowlist + JSON deser) is thin and already unit-tested in `server.rs`;
//! a full-socket harness is a candidate follow-up, not this one.
//!
//! HOW IT DIFFERS from the pilot e2e tests: those replay THREE hardcoded
//! receipts. This harness GENERATES varied packets (`gen_line_and_pay` — varied
//! item counts, prices, cash/card payforms). "Они без новых документов" — this
//! one is not.
//!
//! HALT-ON-ERROR: `invariant_scan::assert_clean` runs after EVERY step; any
//! illegal non-terminal doc at a quiescent boundary (the #192 / P1 ledger-pin)
//! panics the scenario with the full per-step log printed above it.

mod common;

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;

use prro::app::App;
use prro::config::AppConfig;
use prro::crypto::session::SigningSession;
use prro::db::models::enums::{DocState, FiscalMode, NodeMode, Protocol, ShiftState};
use prro::db::models::ids::DriverId;
use prro::db::repositories::{fiscal_number_config as fn_cfg, operators as ops_repo};
use prro::runtime::bindings::{BindingsRegistry, KeyLoadFailure, OperatorKeyLoader};
use prro::runtime::coding::Coding;
use prro::runtime::ingress::dto::CanonicalCommand;
use prro::runtime::ingress::handler::{handle_command, IngressBody};
use prro::runtime::ingress::inline_binding::production_write_path;
use prro::runtime::ingress::seam::WritePathEntry;
use prro::services::reconciliation::runtime::RuntimeView;
use prro::services::reconciliation::ReconciliationRuntime;
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{
    CheckAck, CheckEnvelope, CheckSignBlob, OfflineCodesResponse, RroInfo, StatusSnapshot,
};
use prro::transports::dps::error::DpsError;
use prro::ScheduledDrainOutcome;
use sqlx::SqlitePool;

use common::{det_signing_ctx, det_signing_ctx_for};

const FN: &str = "4000000042";
const FIXTURE_CERT_DER: &[u8] = include_bytes!("fixtures/SELF_SIGNED_ENC_6929.cer");

// ════════════════════════════════════════════════════════════════════════
// MatrixDps — the fault-injectable "network"
// ════════════════════════════════════════════════════════════════════════

/// A stateful, generative `DpsChannel` stub whose `online` flag models the
/// physical network. Unlike the queue-based `ScriptedDps` / `StubDpsChannel`
/// (which require the caller to know the exact number of wire calls up front),
/// this one answers ANY number of `send_chk` / `last_chk` calls generatively —
/// so a scenario can drop and restore the network without recomputing queues.
///
///   * `online == true`  → `send_chk` returns a fresh Sent ack, `last_chk`
///     returns KVT1 evidence (non-empty `data_sign`).
///   * `online == false` → every wire call returns `DpsError::Transport`
///     ("connection refused") — a real net drop.
///   * `reject_next(code)` → the NEXT `send_chk` returns `DpsError::Server`
///     (a DPS terminal reject), then reverts to normal.
///
/// The per-call log (`drain_calls`) is the "envelope spy" the step logger
/// prints, so a human reads exactly what the product asked the network for.
struct MatrixDps {
    online: AtomicBool,
    reject_next: Mutex<Option<i32>>,
    send_ctr: AtomicUsize,
    open_shift: AtomicBool,
    calls: Mutex<Vec<String>>,
}

impl MatrixDps {
    fn new() -> Self {
        Self {
            online: AtomicBool::new(true),
            reject_next: Mutex::new(None),
            send_ctr: AtomicUsize::new(0),
            open_shift: AtomicBool::new(false),
            calls: Mutex::new(Vec::new()),
        }
    }
    fn set_online(&self, up: bool) {
        self.online.store(up, Ordering::SeqCst);
    }
    #[allow(dead_code)]
    fn reject_next(&self, code: i32) {
        *self.reject_next.lock().unwrap() = Some(code);
    }
    #[allow(dead_code)]
    fn set_open_shift(&self, o: bool) {
        self.open_shift.store(o, Ordering::SeqCst);
    }
    fn log(&self, s: String) {
        self.calls.lock().unwrap().push(s);
    }
    /// Take-and-clear the wire-call log accumulated since the last drain — the
    /// per-step delta the logger prints.
    fn drain_calls(&self) -> Vec<String> {
        std::mem::take(&mut *self.calls.lock().unwrap())
    }
}

#[async_trait]
impl DpsChannel for MatrixDps {
    async fn send_chk(&self, _e: CheckEnvelope) -> Result<CheckAck, DpsError> {
        if !self.online.load(Ordering::SeqCst) {
            self.log("send_chk→NET_DOWN".into());
            return Err(DpsError::Transport("matrix: network down".into()));
        }
        if let Some(code) = self.reject_next.lock().unwrap().take() {
            self.log(format!("send_chk→REJECT({code})"));
            return Err(DpsError::Server {
                code,
                message: "matrix: scripted DPS reject".into(),
            });
        }
        let n = self.send_ctr.fetch_add(1, Ordering::SeqCst);
        let sfn = format!("DPS-SFN-{n}");
        self.log(format!("send_chk→SENT({sfn})"));
        // Sent ack = empty data_sign (KVT1 evidence comes from last_chk).
        Ok(CheckAck {
            id: sfn,
            id_sign: vec![],
            data_sign: vec![],
        })
    }

    async fn last_chk(&self, _f: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        if !self.online.load(Ordering::SeqCst) {
            self.log("last_chk→NET_DOWN".into());
            return Err(DpsError::Transport("matrix: network down".into()));
        }
        let n = self.send_ctr.load(Ordering::SeqCst);
        self.log("last_chk→KVT1".into());
        Ok(CheckAck {
            id: format!("DPS-SFN-{}", n.saturating_sub(1)),
            id_sign: vec![],
            data_sign: vec![0xA0u8.wrapping_add(n as u8); 32],
        })
    }

    async fn ping(&self, _e: CheckEnvelope) -> Result<CheckAck, DpsError> {
        if !self.online.load(Ordering::SeqCst) {
            return Err(DpsError::Transport("matrix: network down".into()));
        }
        Ok(CheckAck {
            id: "PING".into(),
            id_sign: vec![],
            data_sign: vec![],
        })
    }

    async fn status_rro(&self, _f: &CheckSignBlob) -> Result<StatusSnapshot, DpsError> {
        if !self.online.load(Ordering::SeqCst) {
            return Err(DpsError::Transport("matrix: network down".into()));
        }
        Ok(StatusSnapshot {
            open_shift: self.open_shift.load(Ordering::SeqCst),
            online: true,
            last_signer: "matrix".into(),
        })
    }

    async fn info_rro(&self, _f: &CheckSignBlob) -> Result<RroInfo, DpsError> {
        Err(DpsError::Internal("matrix: info_rro not scripted".into()))
    }

    async fn ask_offline_codes(&self, _e: CheckEnvelope) -> Result<OfflineCodesResponse, DpsError> {
        Err(DpsError::Internal(
            "matrix: ask_offline_codes not scripted (codes are pre-seeded)".into(),
        ))
    }
}

// ════════════════════════════════════════════════════════════════════════
// Packet generator — varied WIRE-shape receipts (NOT one canned check)
// ════════════════════════════════════════════════════════════════════════

/// Deterministic-from-`seed` line + payment set: varied item count (1–3),
/// varied per-item prices, alternating cash/card payform. Returns the WIRE
/// `goods` + `payments` JSON fragments and the total. Reproducible.
fn gen_line_and_pay(seed: u64) -> (String, String, i64) {
    let n_items = 1 + (seed % 3) as usize;
    let mut goods = String::new();
    let mut total: i64 = 0;
    for k in 0..n_items {
        let price = 1000 + (((seed >> (k * 3)) % 90) as i64 + 1) * 100; // 1100..=10000
        total += price;
        if k > 0 {
            goods.push(',');
        }
        goods.push_str(&format!(
            r#"{{"name":"Item {k}","quantity_milli":1000,"price_kopecks":{price},"tax_group_1":1,"tax_group_2":0,"article_code":{}}}"#,
            k + 1
        ));
    }
    let kind = if seed.is_multiple_of(2) {
        "CASH"
    } else {
        "CASHLESS_1"
    };
    let payments = format!(r#"[{{"type":"{kind}","amount_kopecks":{total}}}]"#);
    (goods, payments, total)
}

/// A wire `CanonicalCommand` for a SELL keyed by `seed`.
fn sell_body(seed: u64, idem: &str) -> (String, i64) {
    let (goods, payments, total) = gen_line_and_pay(seed);
    let body = format!(
        r#"{{"schema_version":"1.0","fiscal_number":"{FN}","command_type":"SELL","idempotency_key":"{idem}","cashier_id":"test-cashier","department":null,"return_check_number":null,"payload":{{"direction":"SALE","goods":[{goods}],"payments":{payments},"totals":{{"sale_kopecks":{total},"return_kopecks":0}}}}}}"#
    );
    (body, total)
}

/// A wire `CanonicalCommand` for a RETURN keyed by `seed`.
fn return_body(seed: u64, idem: &str) -> (String, i64) {
    let (goods, payments, total) = gen_line_and_pay(seed);
    let body = format!(
        r#"{{"schema_version":"1.0","fiscal_number":"{FN}","command_type":"RETURN","idempotency_key":"{idem}","cashier_id":"test-cashier","department":null,"return_check_number":null,"payload":{{"direction":"RETURN","goods":[{goods}],"payments":{payments},"totals":{{"sale_kopecks":0,"return_kopecks":{total}}}}}}}"#
    );
    (body, total)
}

/// A wire `CanonicalCommand` for a SERVICE_IN (службове внесення — cash INTO the
/// drawer). Amount rides on `totals.sale_kopecks`; goods/payments empty.
fn service_in_body(kop: i64, idem: &str) -> String {
    format!(
        r#"{{"schema_version":"1.0","fiscal_number":"{FN}","command_type":"SERVICE_IN","idempotency_key":"{idem}","cashier_id":"test-cashier","department":null,"return_check_number":null,"payload":{{"direction":"SALE","goods":[],"payments":[],"totals":{{"sale_kopecks":{kop},"return_kopecks":0}}}}}}"#
    )
}

/// A wire `CanonicalCommand` for a SERVICE_OUT (службова видача — cash OUT of the
/// drawer). Amount rides on `totals.return_kopecks` (ASYMMETRIC vs SERVICE_IN).
fn service_out_body(kop: i64, idem: &str) -> String {
    format!(
        r#"{{"schema_version":"1.0","fiscal_number":"{FN}","command_type":"SERVICE_OUT","idempotency_key":"{idem}","cashier_id":"test-cashier","department":null,"return_check_number":null,"payload":{{"direction":"SALE","goods":[],"payments":[],"totals":{{"sale_kopecks":0,"return_kopecks":{kop}}}}}}}"#
    )
}

/// A wire `CanonicalCommand` for an EPZ cash advance (видача готівки за ЕПЗ —
/// card-funded cash handed to the cardholder). One CASHLESS card leg with an
/// acquirer slip at `pay_form_index` (must be ≥ 2 = a card slot); amount rides on
/// `payments[0].amount_kopecks`; totals zero, goods empty.
fn epz_body(kop: i64, pay_form_index: i64, idem: &str) -> String {
    format!(
        r#"{{"schema_version":"1.0","fiscal_number":"{FN}","command_type":"CASH_ADVANCE_EPZ","idempotency_key":"{idem}","cashier_id":"test-cashier","department":null,"return_check_number":null,"payload":{{"direction":"SALE","goods":[],"payments":[{{"type":"CASHLESS_1","amount_kopecks":{kop},"acquirer_slip":{{"payment_form_index":{pay_form_index},"merchant_id":"M-1","terminal_id":"T-1","operation_type":"PURCHASE","pan":"**** 4242","approval_code":"APPR1","payment_system":"Visa","transaction_code":"RRN-1","fee_kopecks":0,"cashier_signature_placeholder":false,"cardholder_signature_placeholder":false}}}}],"totals":{{"sale_kopecks":0,"return_kopecks":0}}}}}}"#
    )
}

/// A wire `CanonicalCommand` for a payload-less op (SHIFT_OPEN / Z_REPORT).
fn simple_body(command_type: &str, idem: &str) -> String {
    format!(
        r#"{{"schema_version":"1.0","fiscal_number":"{FN}","command_type":"{command_type}","idempotency_key":"{idem}","cashier_id":"test-cashier","department":null,"return_check_number":null,"payload":{{"direction":"SALE","totals":{{"sale_kopecks":0,"return_kopecks":0}}}}}}"#
    )
}

// ════════════════════════════════════════════════════════════════════════
// App boot + registry (mirrors the pilot scaffolding; offline_enabled=true)
// ════════════════════════════════════════════════════════════════════════

fn cfg_toml(db_path: &str) -> String {
    format!(
        r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "{db_path}"
secure_db_path = "{db_path}_secure"

[admin_ui]
enabled = false
listen  = "127.0.0.1:8444"
"#
    )
}

async fn boot_app() -> App {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("matrix.db");
    std::mem::forget(dir);
    let toml_text = cfg_toml(&db_path.display().to_string().replace('\\', "/"));
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    App::boot(cfg).await.unwrap()
}

struct FixtureLoader;
#[async_trait]
impl OperatorKeyLoader for FixtureLoader {
    async fn load(
        &self,
        operator_id: &str,
        _key_path: &Path,
        _password: &[u8],
    ) -> Result<SigningContext, KeyLoadFailure> {
        let mut ctx = det_signing_ctx_for(operator_id);
        ctx.session = SigningSession::new_for_test(
            operator_id.to_string(),
            [7u8; 32],
            FIXTURE_CERT_DER.to_vec(),
        );
        Ok(ctx)
    }
}

fn fn_config() -> fn_cfg::NewFnConfig {
    fn_cfg::NewFnConfig {
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
    }
}

async fn build_registry(app: &App, dps: Arc<dyn DpsChannel>) -> BindingsRegistry {
    fn_cfg::insert(app.db(), &fn_config())
        .await
        .expect("seed FN config");
    ops_repo::insert(
        app.db_secure(),
        &ops_repo::NewOperator {
            operator_id: "OP-1".into(),
            fiscal_number: FN.into(),
            name: "Cashier".into(),
            key_path: "/tmp/k1.dat".into(),
            key_pass_enc: Coding::encode(b"secret1").expect("encode test password"),
        },
    )
    .await
    .expect("seed operator");
    // Real per-FN default provisioning (tax_groups 1..11 + payment_methods
    // cash=1/card=2 + outgress profile + integration flags) — the SAME
    // production path `admin::register_fn` runs. The ingress `convert` needs
    // these (tax-group + payment-method lookups) or it breaches / rejects.
    prro::runtime::bootstrap::bootstrap_fn_defaults(app.db_secure(), FN)
        .await
        .expect("bootstrap FN defaults");
    BindingsRegistry::build_from_db(app.db_secure(), app.db(), dps, &FixtureLoader)
        .await
        .expect("build_from_db")
}

async fn seed_boot_baseline(pool: &SqlitePool) {
    sqlx::query(
        "INSERT INTO node_state \
         (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
          backend_profile_id, transport_profile_id) \
         VALUES (?, ?, ?, NULL, 1, 'b', 't')",
    )
    .bind(FN)
    .bind(NodeMode::Online)
    .bind(ShiftState::Closed)
    .execute(pool)
    .await
    .unwrap();
}

// ════════════════════════════════════════════════════════════════════════
// Ingress driver (through the REAL ingress core) + probes
// ════════════════════════════════════════════════════════════════════════

/// Drive one wire `CanonicalCommand` (JSON body) through the REAL ingress core:
/// `handle_command` → classify → convert → inbox → live binding → pipeline.
/// Returns `(http_status, document_state | "ERR:<code>")`.
async fn drive_ingress(app: &App, wp: &dyn WritePathEntry, body: &str) -> (u16, String) {
    let cmd: CanonicalCommand = serde_json::from_str(body).expect("parse wire CanonicalCommand");
    let resp = handle_command(
        &cmd,
        FN,
        DriverId::new("drv-test").unwrap(),
        Protocol::Rest,
        app.db(),
        app.db_secure(),
        wp,
    )
    .await;
    match resp.body {
        IngressBody::Success(cr) => (resp.http_status, cr.document_state),
        IngressBody::Error(er) => (
            resp.http_status,
            format!("ERR:{} ({})", er.error_code, er.error_message),
        ),
        // The matrix harness has no `Action::XReport`, so the ingress core never
        // returns an X-report body here. If an X-report action is ever added,
        // this panic forces the driver to grow a real arm rather than silently
        // mis-mapping the read snapshot to a "document_state".
        IngressBody::XReport(_) => {
            unreachable!("matrix harness issues no X-report; add an Action::XReport arm first")
        }
    }
}

async fn node_row(pool: &SqlitePool) -> (String, String) {
    sqlx::query_as("SELECT mode, shift_state FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn shift_state(pool: &SqlitePool) -> String {
    sqlx::query_scalar(
        "SELECT state FROM shifts WHERE fiscal_number = ? ORDER BY rowid DESC LIMIT 1",
    )
    .bind(FN)
    .fetch_optional(pool)
    .await
    .unwrap()
    .unwrap_or_else(|| "<none>".into())
}

async fn doc_count_in_state(pool: &SqlitePool, state: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ? AND state = ?",
    )
    .bind(FN)
    .bind(state)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn cash_on_hand(pool: &SqlitePool) -> i64 {
    prro::services::cash_ledger::cash_on_hand_for_fn(pool, FN)
        .await
        .unwrap_or(-1)
}

/// Count docs NOT in a terminal-rest state — the z-quiescence surface. A Z can
/// only close when this is 0 (else `Z_QUIESCENCE_PENDING` / 503).
#[allow(dead_code)]
async fn non_terminal_doc_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ? \
         AND state NOT IN ('ACK','REJECTED','OFFLINE_LOCAL_ACK')",
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The durable DB state of the most-recently-minted fiscal document — the doc
/// under test after a scripted DPS reject.
async fn last_doc_state(pool: &SqlitePool) -> String {
    sqlx::query_scalar(
        "SELECT state FROM fiscal_documents WHERE fiscal_number = ? ORDER BY rowid DESC LIMIT 1",
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ════════════════════════════════════════════════════════════════════════
// Drain wiring (mirrors pilot_offline_full_drill_e2e)
// ════════════════════════════════════════════════════════════════════════

struct DrainCarriers {
    dps: Arc<dyn DpsChannel>,
    signing_ctx: SigningContext,
    fn_sign: CheckSignBlob,
}

fn drain_carriers(dps: Arc<dyn DpsChannel>) -> DrainCarriers {
    DrainCarriers {
        dps,
        signing_ctx: det_signing_ctx(),
        fn_sign: CheckSignBlob(vec![0xAB, 0xCD]),
    }
}

fn drain_view(c: &DrainCarriers) -> RuntimeView<'_> {
    RuntimeView {
        dps: c.dps.as_ref(),
        signing_ctx: &c.signing_ctx,
        fn_sign: &c.fn_sign,
    }
}

// ════════════════════════════════════════════════════════════════════════
// The scenario interpreter
// ════════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
enum Action {
    /// Online SHIFT_OPEN (net must be up).
    Open,
    /// A generated SELL packet keyed by `seed` (varied items/price/payform).
    Sell(u64),
    /// A generated RETURN packet keyed by `seed`.
    Return(u64),
    /// Z_REPORT — close the shift.
    ZClose,
    /// Physical network drop (DPS unreachable) — mode is NOT touched.
    NetDown,
    /// Physical network restore.
    NetUp,
    /// Node reacts to the drop: GO_OFFLINE via the live door + seed codes.
    GoOffline,
    /// Net restored: GO_ONLINE via the live door + one drain tick.
    GoOnlineDrain,
    /// SELL while the net is DOWN but mode is STILL ONLINE → the doc must land
    /// `ERROR_RETRYABLE` (stays online, retried by reconciliation), NOT OLA/lost.
    SellRetryable(u64),
    /// Reconciliation tick — re-drive pending `ERROR_RETRYABLE` docs to terminal.
    Reconcile,
    /// Online-convergence tick — actively finalise KVT1 → ACK (the mechanism
    /// SEPARATE from `Reconcile`'s passive KVT1-hold; the supervisor drives both).
    OnlineConverge,
    /// Службове внесення — cash INTO the drawer, `kop` kopecks.
    ServiceIn(i64),
    /// Службова видача — cash OUT of the drawer, `kop` kopecks. The drawer must
    /// hold ≥ `kop` (guard-3b), else the op refuses (use a bespoke test for that).
    ServiceOut(i64),
    /// EPZ cash advance (видача готівки за ЕПЗ) — `kop` kopecks, card-funded (pay
    /// form index 2). Drawer must hold ≥ `kop` (guard-3c).
    Epz(i64),
}

/// Run a scenario end-to-end through the real ingress, logging every step and
/// asserting the ledger invariant scan after each. HALTS (panics with the log
/// printed above) on the first divergence.
async fn run(
    name: &str,
    steps: &[Action],
    app: &App,
    wp: &dyn WritePathEntry,
    dps: &Arc<MatrixDps>,
) {
    eprintln!("\n════════ SCENARIO: {name} ════════");
    for (i, act) in steps.iter().enumerate() {
        let t0 = Instant::now();
        let outcome: String = match act {
            Action::Open => {
                let (status, state) = drive_ingress(
                    app,
                    wp,
                    &simple_body("SHIFT_OPEN", &format!("mx-{name}-OPEN-{i}")),
                )
                .await;
                assert_eq!(state, DocState::Ack.as_str(), "SHIFT_OPEN → ACK [{status}]");
                assert_eq!(
                    shift_state(app.db()).await,
                    "OPENED",
                    "shift OPENED after open"
                );
                format!("SHIFT_OPEN → {state} [{status}]")
            }
            Action::Sell(seed) => {
                let (body, total) = sell_body(*seed, &format!("mx-{name}-SELL-{i}"));
                let (status, state) = drive_ingress(app, wp, &body).await;
                let mode = node_row(app.db()).await.0;
                let want = if mode == "OFFLINE" {
                    DocState::OfflineLocalAck.as_str()
                } else {
                    DocState::Ack.as_str()
                };
                assert_eq!(state, want, "SELL step {i}: mode={mode} status={status}");
                format!("SELL(seed={seed},total={total}) → {state} [{status}]")
            }
            Action::Return(seed) => {
                let (body, total) = return_body(*seed, &format!("mx-{name}-RET-{i}"));
                let (status, state) = drive_ingress(app, wp, &body).await;
                let mode = node_row(app.db()).await.0;
                let want = if mode == "OFFLINE" {
                    DocState::OfflineLocalAck.as_str()
                } else {
                    DocState::Ack.as_str()
                };
                assert_eq!(state, want, "RETURN step {i}: mode={mode} status={status}");
                format!("RETURN(seed={seed},total={total}) → {state} [{status}]")
            }
            Action::ZClose => {
                let (status, state) = drive_ingress(
                    app,
                    wp,
                    &simple_body("Z_REPORT", &format!("mx-{name}-Z-{i}")),
                )
                .await;
                assert_eq!(state, DocState::Ack.as_str(), "Z → ACK [{status}]");
                assert_eq!(
                    shift_state(app.db()).await,
                    "CLOSED",
                    "shift CLOSED after Z"
                );
                format!("Z_REPORT → {state} [{status}] (shift CLOSED)")
            }
            Action::NetDown => {
                dps.set_online(false);
                "NET_DOWN (DPS unreachable)".into()
            }
            Action::NetUp => {
                dps.set_online(true);
                "NET_UP (DPS restored)".into()
            }
            Action::GoOffline => {
                prro::admin::go_offline(app.db(), FN, "matrix: net down")
                    .await
                    .expect("live door: GO_OFFLINE");
                let codes: Vec<String> = (0..20).map(|j| format!("MX-{name}-{i}-{j}")).collect();
                prro::admin::seed_dps_offline_codes(app.db(), FN, &codes)
                    .await
                    .expect("seed offline codes");
                assert_eq!(
                    node_row(app.db()).await.0,
                    "OFFLINE",
                    "mode OFFLINE after GO_OFFLINE"
                );
                "GO_OFFLINE (live door) + 20 codes seeded".into()
            }
            Action::GoOnlineDrain => {
                prro::admin::go_online(app.db(), FN, "matrix: net restored")
                    .await
                    .expect("live door: GO_ONLINE");
                assert_eq!(
                    node_row(app.db()).await.0,
                    "GOING_ONLINE",
                    "mode GOING_ONLINE"
                );
                let carriers = drain_carriers(dps.clone());
                let view = drain_view(&carriers);
                let outcome = app
                    .drain_offline_backlog_scheduled(FN, &view)
                    .await
                    .expect("drain must run for GOING_ONLINE FN");
                match outcome {
                    ScheduledDrainOutcome::Ran(_) => {}
                    ScheduledDrainOutcome::SkippedBackoff { .. } => {
                        panic!("drain must run, not skip-backoff")
                    }
                }
                assert_eq!(
                    doc_count_in_state(app.db(), "OFFLINE_LOCAL_ACK").await,
                    0,
                    "all OLA docs drained to terminal after GO_ONLINE+drain"
                );
                assert_eq!(
                    node_row(app.db()).await.0,
                    "ONLINE",
                    "mode ONLINE after drain converge"
                );
                "GO_ONLINE (live door) + drain → converged".into()
            }
            Action::SellRetryable(seed) => {
                let (body, total) = sell_body(*seed, &format!("mx-{name}-SELLR-{i}"));
                let (status, _resp) = drive_ingress(app, wp, &body).await;
                let doc = last_doc_state(app.db()).await;
                let mode = node_row(app.db()).await.0;
                assert_eq!(
                    doc,
                    DocState::ErrorRetryable.as_str(),
                    "net-down online SELL step {i}: doc must rest ERROR_RETRYABLE \
                     (stays online, retried — NOT OLA, NOT lost)"
                );
                assert_eq!(
                    mode, "ONLINE",
                    "net-down SELL must NOT flip mode to OFFLINE"
                );
                format!("SELL(seed={seed},total={total}) NET-DOWN → doc={doc} [{status}]")
            }
            Action::Reconcile => {
                let carriers = drain_carriers(dps.clone());
                // Re-drive the ErrorRetryable doc (net restored). Several ticks,
                // exactly as the supervisor loop does.
                for _ in 0..8 {
                    let deps = ReconciliationRuntime::single_fn(drain_view(&carriers));
                    app.reconcile_pending_with(deps)
                        .await
                        .expect("reconcile_pending_with must run");
                }
                // PROVEN: reconcile forward-drives the doc OUT of ErrorRetryable
                // (not lost, not stuck-retryable). NOTE (harness finding): the
                // re-driven doc rests at KVT1 — the reconcile path does not
                // finalise KVT1→KVT2→ACK from the same last_chk evidence the
                // INLINE path finalises with. Asymmetry captured; full KVT2
                // fidelity (or the real online-convergence tick) is a follow-up.
                assert_eq!(
                    doc_count_in_state(app.db(), "ERROR_RETRYABLE").await,
                    0,
                    "reconcile must re-drive the doc FORWARD out of ERROR_RETRYABLE"
                );
                let st = last_doc_state(app.db()).await;
                format!("RECONCILE (re-drive) → ErrorRetryable cleared, doc now {st}")
            }
            Action::OnlineConverge => {
                let carriers = drain_carriers(dps.clone());
                // Active KVT1 → ACK finalisation (the M3b online-convergence tick,
                // distinct from Reconcile's passive KVT1-hold). Tick to terminal.
                let mut ticks = 0;
                loop {
                    let view = drain_view(&carriers);
                    app.converge_online_for_fn(FN, &view)
                        .await
                        .expect("converge_online_for_fn must run");
                    ticks += 1;
                    if non_terminal_doc_count(app.db()).await == 0 || ticks >= 8 {
                        break;
                    }
                }
                assert_eq!(
                    non_terminal_doc_count(app.db()).await,
                    0,
                    "online-convergence must finalise KVT1 → ACK ({ticks} ticks)"
                );
                format!("ONLINE-CONVERGE ({ticks} ticks) → KVT1 finalised to ACK")
            }
            Action::ServiceIn(kop) => {
                let (status, state) = drive_ingress(
                    app,
                    wp,
                    &service_in_body(*kop, &format!("mx-{name}-SVCIN-{i}")),
                )
                .await;
                let mode = node_row(app.db()).await.0;
                let want = if mode == "OFFLINE" {
                    DocState::OfflineLocalAck.as_str()
                } else {
                    DocState::Ack.as_str()
                };
                assert_eq!(
                    state, want,
                    "SERVICE_IN step {i}: mode={mode} status={status}"
                );
                format!("SERVICE_IN(+{kop}) → {state} [{status}]")
            }
            Action::ServiceOut(kop) => {
                let (status, state) = drive_ingress(
                    app,
                    wp,
                    &service_out_body(*kop, &format!("mx-{name}-SVCOUT-{i}")),
                )
                .await;
                let mode = node_row(app.db()).await.0;
                let want = if mode == "OFFLINE" {
                    DocState::OfflineLocalAck.as_str()
                } else {
                    DocState::Ack.as_str()
                };
                assert_eq!(
                    state, want,
                    "SERVICE_OUT step {i}: mode={mode} status={status} (drawer must cover {kop})"
                );
                format!("SERVICE_OUT(-{kop}) → {state} [{status}]")
            }
            Action::Epz(kop) => {
                let (status, state) =
                    drive_ingress(app, wp, &epz_body(*kop, 2, &format!("mx-{name}-EPZ-{i}"))).await;
                let mode = node_row(app.db()).await.0;
                let want = if mode == "OFFLINE" {
                    DocState::OfflineLocalAck.as_str()
                } else {
                    DocState::Ack.as_str()
                };
                assert_eq!(
                    state, want,
                    "EPZ step {i}: mode={mode} status={status} (drawer must cover {kop})"
                );
                format!("EPZ(-{kop} card-funded) → {state} [{status}]")
            }
        };

        let dt_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let (mode, node_shift) = node_row(app.db()).await;
        let wire = dps.drain_calls();
        eprintln!(
            "[{name}][step {i:>2}] {outcome:<48} {dt_ms:>6.1}ms | mode={mode:<12} shift={:<10} node_shift={node_shift:<10} cash={} | dps{:?}",
            shift_state(app.db()).await,
            cash_on_hand(app.db()).await,
            wire,
        );

        // HALT-ON-ERROR: the universal oracle. Any illegal non-terminal doc at
        // this quiescent boundary (#192 / P1 ledger-pin) panics here.
        prro::db::invariant_scan::assert_clean(app.db()).await;
    }
    eprintln!("════════ {name}: PASS ════════\n");
}

async fn fresh_harness() -> (App, Arc<MatrixDps>, Arc<dyn WritePathEntry>) {
    let app = boot_app().await;
    let dps = Arc::new(MatrixDps::new());
    let registry = build_registry(&app, dps.clone()).await;
    seed_boot_baseline(app.db()).await;
    let wp = production_write_path(app.clone(), Arc::new(registry));
    (app, dps, wp)
}

// ════════════════════════════════════════════════════════════════════════
// Increment 1/2 scenarios — harness self-proof on KNOWN-GREEN paths
// ════════════════════════════════════════════════════════════════════════

/// S1 — online happy life: open, three VARIED sells, one return, close.
/// Proves the harness drives varied generated packets through the REAL ingress
/// to terminal ACK on a clean scan at every step.
#[tokio::test]
async fn matrix_s1_online_happy_varied_packets() {
    let (app, dps, wp) = fresh_harness().await;
    let steps = vec![
        Action::Open,
        Action::Sell(1),
        Action::Sell(2),
        Action::Sell(7),
        Action::Return(2),
        Action::ZClose,
    ];
    run("S1-online-happy", &steps, &app, &*wp, &dps).await;
}

/// S2 — offline lifecycle: open online, sell online, DROP the net, go offline,
/// sell/return offline (OLA), restore the net, go online + drain (converge),
/// then close with a Z. This is the core online↔offline↔drain transition the
/// operator flagged as "only think it works" — now watched end-to-end through
/// the real ingress.
#[tokio::test]
async fn matrix_s2_offline_lifecycle_drain_close() {
    let (app, dps, wp) = fresh_harness().await;
    let steps = vec![
        Action::Open,
        Action::Sell(3),
        Action::NetDown,
        Action::GoOffline,
        Action::Sell(4),
        Action::Sell(5),
        Action::Return(4),
        Action::NetUp,
        Action::GoOnlineDrain,
        Action::ZClose,
    ];
    run("S2-offline-lifecycle", &steps, &app, &*wp, &dps).await;
}

/// S3 — DPS REJECT CORPUS ("соберём ошибки ДПС"). For a representative DPS
/// reject code per routing class (`error_routing.rs`), inject it at the SELL's
/// send, then record the product's END-TO-END reaction (ingress response + the
/// durable doc state + node mode) and ASSERT it matches the routing spec. This
/// is the byzantine-DPS empirical groundwork + the transition matrix UNDER
/// DPS-reject, proven through the real ingress. `assert_clean` is the universal
/// oracle after every reject. Each code runs on a FRESH harness (a `-11` BLOCK
/// would poison a shared node).
#[tokio::test]
async fn matrix_s3_dps_reject_corpus() {
    // (DPS code, meaning, expected durable doc state, expected node mode after)
    let corpus: &[(i32, &str, DocState, &str)] = &[
        (
            -2,
            "ERROR_CHECK (terminal reject)",
            DocState::Rejected,
            "ONLINE",
        ),
        (
            -3,
            "ERROR_SAVE (transient retry)",
            DocState::ErrorRetryable,
            "ONLINE",
        ),
        (
            -5,
            "ERROR_TYPE (builder-bug reject)",
            DocState::Rejected,
            "ONLINE",
        ),
        (
            -6,
            "ERROR_NOT_PREV_ZREPORT (escalate)",
            DocState::ErrorRetryable,
            "ONLINE",
        ),
        (
            -11,
            "ERROR_OFFLINE_168 (reject + BLOCK)",
            DocState::Rejected,
            "BLOCKED",
        ),
        // NOTE: -12 (ERROR_BAD_HASH_PREV) is intentionally NOT in this simple-
        // reject corpus. It triggers the multi-step W10.4 MAC-recovery subsystem
        // (status_rro + re-seed + re-sign + re-send), NOT a terminal routing — a
        // one-shot reject leaves the recovery unable to complete, so the doc
        // terminalises to Rejected here. MAC recovery deserves a dedicated
        // scenario (faithful re-seed modeling); tracked as a matrix follow-up.
        (
            -15,
            "ERROR_NOT_OPEN_SHIFT (reject)",
            DocState::Rejected,
            "ONLINE",
        ),
        (
            -16,
            "ERROR_OFFLINE_ID (reject)",
            DocState::Rejected,
            "ONLINE",
        ),
        (
            -99,
            "unknown code (byzantine fallback)",
            DocState::ErrorRetryable,
            "ONLINE",
        ),
    ];
    eprintln!("\n════════ DPS REJECT CORPUS ════════");
    for (code, meaning, exp_state, exp_mode) in corpus {
        let (app, dps, wp) = fresh_harness().await;
        // Open a shift online (this send succeeds).
        let (st, _) = drive_ingress(&app, &*wp, &simple_body("SHIFT_OPEN", "corpus-OPEN")).await;
        assert_eq!(
            st, 200,
            "corpus setup: SHIFT_OPEN must ACK before the reject (code {code})"
        );
        // Inject the DPS reject onto the SELL's send.
        dps.reject_next(*code);
        let (body, _total) = sell_body(1, "corpus-SELL");
        let (status, resp) = drive_ingress(&app, &*wp, &body).await;
        let doc = last_doc_state(app.db()).await;
        let mode = node_row(app.db()).await.0;
        eprintln!(
            "[corpus] DPS {code:>4} {meaning:<38} → ingress[{status} {resp:<34.34}] doc={doc:<15} node={mode}"
        );
        assert_eq!(
            doc,
            exp_state.as_str(),
            "DPS {code}: durable doc state must match routing spec"
        );
        assert_eq!(mode, *exp_mode, "DPS {code}: node mode after reject");
        prro::db::invariant_scan::assert_clean(app.db()).await;
    }
    eprintln!("════════ DPS REJECT CORPUS: PASS ════════\n");
}

/// (fiscal_documents count, sqlite page_count) — coarse DB-growth probe.
async fn db_stats(pool: &SqlitePool) -> (i64, i64) {
    let docs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(pool)
            .await
            .unwrap();
    let pages: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(pool)
        .await
        .unwrap_or(-1);
    (docs, pages)
}

/// S4 — THROUGHPUT + DB behavior ("посмотрим как база работает, померяем
/// скорості full-end"). Drive a large batch of VARIED SELLs through the real
/// ingress on one open shift, then a Z, measuring per-op latency + throughput +
/// DB growth. CAVEAT: signing here is the DetCrypto STUB (constant CMS), so this
/// measures the DB + ingress + staged-pipeline + orchestration cost per receipt,
/// NOT real DSTU-4145 crypto cost (that is a separate live/bench concern).
#[tokio::test]
async fn matrix_s4_throughput_and_db() {
    let (app, _dps, wp) = fresh_harness().await;
    let (st, _) = drive_ingress(&app, &*wp, &simple_body("SHIFT_OPEN", "perf-OPEN")).await;
    assert_eq!(st, 200, "perf setup: SHIFT_OPEN must ACK");

    let n: u64 = 200;
    let t0 = Instant::now();
    for i in 0..n {
        let (body, _) = sell_body(i, &format!("perf-SELL-{i}"));
        let (status, _state) = drive_ingress(&app, &*wp, &body).await;
        assert_eq!(status, 200, "SELL {i} must ACK");
    }
    let elapsed = t0.elapsed();
    let (dz, _) = drive_ingress(&app, &*wp, &simple_body("Z_REPORT", "perf-Z")).await;
    assert_eq!(dz, 200, "perf: Z must close the shift");

    let total_ms = elapsed.as_secs_f64() * 1000.0;
    let per_ms = total_ms / n as f64;
    let ops = n as f64 / elapsed.as_secs_f64();
    let (docs, pages) = db_stats(app.db()).await;
    eprintln!("\n════════ THROUGHPUT (sign=STUB) ════════");
    eprintln!(
        "  {n} SELLs in {total_ms:.0}ms  →  {per_ms:.2} ms/sell, {ops:.0} sells/sec  (pipeline+DB, crypto stubbed)"
    );
    eprintln!("  DB after Z: fiscal_documents={docs}, sqlite page_count={pages}");
    eprintln!("════════════════════════════════════════\n");
    prro::db::invariant_scan::assert_clean(app.db()).await;
}

/// S5 — net-drop MID-ONLINE (the least-verified transition cell). The network
/// drops while the node is STILL ONLINE (no operator GO_OFFLINE). A SELL must
/// land `ERROR_RETRYABLE` — it stays online, is neither lost nor turned into an
/// offline-local-ack, and rests on a CLEAN scan (crash-recovery state). When the
/// net returns, reconciliation (the SAME entry the supervisor loop calls)
/// re-drives it to terminal ACK. Whole path: real ingress → write-path →
/// stage_send (send fails) → ErrorRetryable → real reconcile → send → ACK; only
/// the network (MatrixDps) is faked.
#[tokio::test]
async fn matrix_s5_net_drop_mid_online_retry() {
    let (app, dps, wp) = fresh_harness().await;
    let steps = vec![
        Action::Open,             // online SHIFT_OPEN → ACK
        Action::Sell(6),          // online SELL → ACK
        Action::NetDown,          // physical net drop; mode STAYS online
        Action::SellRetryable(8), // SELL → ERROR_RETRYABLE (retryable, not lost)
        Action::NetUp,            // net restored
        Action::Reconcile,        // reconcile re-drives ErrorRetryable → KVT1 (holds)
        Action::OnlineConverge,   // online-convergence finalises KVT1 → ACK
        Action::ZClose,           // now quiescent → shift closes cleanly
    ];
    // Full net-drop-mid-online lifecycle: NET_DOWN online SELL → ERROR_RETRYABLE
    // (stays online, not lost) → reconcile re-drives → KVT1 (passive hold) →
    // online-convergence finalises → ACK → Z closes. The TWO-mechanism online
    // convergence (reconcile re-drive + convergence KVT2-finalise) is exactly
    // what the supervisor loop runs; a Z before quiescence is correctly blocked
    // (Z_QUIESCENCE_PENDING/503, observed live).
    run("S5-net-drop-online", &steps, &app, &*wp, &dps).await;
}

/// S6 — MIXED ONLINE↔OFFLINE DAY (the realistic degraded day). ONE shift lives
/// through the whole arc: online sells → net drop → operator GO_OFFLINE →
/// offline sells + a return (OLA) → net restore → GO_ONLINE + drain (backlog
/// converges) → MORE online sells AFTER the drain, same shift → Z. This is the
/// online→offline→online-WITHIN-ONE-SHIFT path the operator flagged as
/// "only think it works": selling on BOTH sides of an offline window and closing
/// clean. Every step scanned (`assert_clean`); a stuck non-terminal doc halts.
#[tokio::test]
async fn matrix_s6_mixed_online_offline_day() {
    let (app, dps, wp) = fresh_harness().await;
    let steps = vec![
        Action::Open,
        Action::Sell(11),
        Action::Sell(12),
        Action::NetDown,
        Action::GoOffline,
        Action::Sell(13),
        Action::Return(12),
        Action::Sell(14),
        Action::NetUp,
        Action::GoOnlineDrain,
        Action::Sell(15), // online SELL AFTER the drain, SAME shift
        Action::Return(11),
        Action::ZClose,
    ];
    run("S6-mixed-day", &steps, &app, &*wp, &dps).await;
}

/// S7 — MULTI-SHIFT SEQUENCE (reopen after close). Three shifts back-to-back on
/// the SAME FN: Open→sells→Z, Open→sell→Z, Open→sell→return→Z. Proves the shift
/// machine REOPENS cleanly after a Z (a Z closes the SHIFT, not the node) and
/// that chain / seed / lnd continuity holds ACROSS shift boundaries — the fiscal
/// chain must not fork or reset at a Z. Each Z leaves a clean scan; each reopen
/// mints a fresh OPENED shift. Return-after-sell keeps drawer cash ≥ 0.
#[tokio::test]
async fn matrix_s7_multi_shift_reopen_sequence() {
    let (app, dps, wp) = fresh_harness().await;
    let steps = vec![
        // ── shift 1 ──
        Action::Open,
        Action::Sell(21),
        Action::Sell(22),
        Action::ZClose,
        // ── shift 2 ──
        Action::Open,
        Action::Sell(23),
        Action::ZClose,
        // ── shift 3 ──
        Action::Open,
        Action::Sell(24),
        Action::Return(21),
        Action::ZClose,
    ];
    run("S7-multi-shift", &steps, &app, &*wp, &dps).await;
}

/// S8 — CROSS-SHIFT CASH CARRY-OVER (pins the behavior S7's log surfaced). A Z
/// does NOT empty the physical drawer: the залишок готівки carries to the next
/// shift as its opening anchor (Ukrainian fiscal reality — cash stays in the till
/// across shifts until a службова видача). S7's log showed cash oscillating
/// `4600 → 0 (at Z) → 4600 (at reopen)`, which reads alarming but is correct.
/// This turns that observation into teeth by pinning three facts:
///   (1) during the CLOSED window `cash_on_hand_for_fn` returns the DOCUMENTED
///       no-open-shift sentinel (0) — an out-of-contract read; the drawer is not
///       emptied, just unattributed (X-report is gated to open shifts, so prod
///       never reads cash here);
///   (2) the next shift OPENS with an opening drawer == the prior shift's CLOSE
///       drawer — carry-over is wired, not reset to 0;
///   (3) the carried cash is strictly > 0 (non-vacuous pin).
/// Even seeds are CASH payform (`gen_line_and_pay`: `seed.is_multiple_of(2)`), so
/// the drawer provably moves. A regression that resets the drawer at Z, or that
/// leaks drawer cash during the closed window, goes RED here.
#[tokio::test]
async fn matrix_s8_cross_shift_cash_carryover() {
    let (app, _dps, wp) = fresh_harness().await;

    // ── shift 1: open + two CASH sells (even seeds) → drawer accumulates ──
    let (st, _) = drive_ingress(&app, &*wp, &simple_body("SHIFT_OPEN", "s8-OPEN-1")).await;
    assert_eq!(st, 200, "S8 setup: shift 1 SHIFT_OPEN must ACK");
    for (idx, seed) in [2u64, 6u64].into_iter().enumerate() {
        let (body, _t) = sell_body(seed, &format!("s8-SELL1-{idx}"));
        let (s, state) = drive_ingress(&app, &*wp, &body).await;
        assert_eq!(
            state,
            DocState::Ack.as_str(),
            "S8 shift-1 CASH sell {idx} (seed={seed}) → ACK [{s}]"
        );
    }
    // Drawer at the END of shift 1 — still OPEN, so this is an IN-contract read.
    let close_drawer_1 = cash_on_hand(app.db()).await;
    assert!(
        close_drawer_1 > 0,
        "S8 precondition: shift 1 must accumulate CASH to carry (got {close_drawer_1})"
    );

    // ── Z closes shift 1 ──
    let (zs, zstate) = drive_ingress(&app, &*wp, &simple_body("Z_REPORT", "s8-Z-1")).await;
    assert_eq!(zstate, DocState::Ack.as_str(), "S8 Z-1 → ACK [{zs}]");
    assert_eq!(
        shift_state(app.db()).await,
        "CLOSED",
        "S8 shift 1 CLOSED after Z"
    );

    // (1) Closed-window read → documented no-open-shift sentinel (0), NOT the drawer.
    let sentinel = cash_on_hand(app.db()).await;
    assert_eq!(
        sentinel, 0,
        "S8: cash_on_hand during the CLOSED window is the no-open-shift sentinel (0), \
         not the drawer ({close_drawer_1}) — the drawer is not emptied, just unattributed"
    );

    // ── shift 2: reopen ──
    let (os, _) = drive_ingress(&app, &*wp, &simple_body("SHIFT_OPEN", "s8-OPEN-2")).await;
    assert_eq!(os, 200, "S8: shift 2 SHIFT_OPEN must ACK");
    assert_eq!(shift_state(app.db()).await, "OPENED", "S8 shift 2 OPENED");

    // (2)+(3) Opening drawer of shift 2 == closing drawer of shift 1 (carry-over).
    let open_drawer_2 = cash_on_hand(app.db()).await;
    assert_eq!(
        open_drawer_2, close_drawer_1,
        "S8: drawer cash must CARRY across the Z — shift-2 opening anchor ({open_drawer_2}) \
         must equal shift-1 closing drawer ({close_drawer_1}), not reset to 0"
    );

    prro::db::invariant_scan::assert_clean(app.db()).await;
    eprintln!(
        "════════ S8-carryover: PASS — drawer {close_drawer_1} carried across Z \
         (sentinel 0 during closed window) ════════"
    );
}

/// S9 — SERVICE-IO LIFECYCLE (службові внесення/видача through the real path).
/// Open → внесення (+50.00) → a CASH sell → видача (−20.00, covered) → Z. Proves
/// the drawer tracks service cash-in/out and that a shift holding service-io docs
/// still reaches a CLEAN Z (the `<IO>` aggregation + z-quiescence include
/// service-io — the #192 z-quiescence class L3 fixed). The per-step log shows the
/// drawer move.
#[tokio::test]
async fn matrix_s9_service_io_lifecycle() {
    let (app, dps, wp) = fresh_harness().await;
    let steps = vec![
        Action::Open,
        Action::ServiceIn(5000),  // +50.00 into the drawer
        Action::Sell(2),          // even seed = CASH sell → drawer grows
        Action::ServiceOut(2000), // -20.00 out of the drawer (covered)
        Action::ZClose,
    ];
    run("S9-service-io", &steps, &app, &*wp, &dps).await;
}

/// S10 — SERVICE-OUT guard-3b teeth (fail-closed видача over an empty drawer). A
/// службова видача larger than the drawer must be REFUSED at the canonical
/// boundary (CASH_INSUFFICIENT / HTTP 422) and mint ZERO fiscal rows — the drawer
/// can never go negative (INV-21). Then a видача of EXACTLY the funded drawer
/// proceeds to ACK and drains it to 0, proving the guard is a floor, not a wall.
#[tokio::test]
async fn matrix_s10_service_out_guard_3b() {
    let (app, _dps, wp) = fresh_harness().await;
    let (st, _) = drive_ingress(&app, &*wp, &simple_body("SHIFT_OPEN", "s10-OPEN")).await;
    assert_eq!(st, 200, "S10 setup: SHIFT_OPEN must ACK");
    let rows_before = db_stats(app.db()).await.0;

    // over-drawer видача on an EMPTY drawer → fail-closed, row-less
    let (status, state) =
        drive_ingress(&app, &*wp, &service_out_body(5000, "s10-SVCOUT-over")).await;
    assert_eq!(
        status, 422,
        "S10: over-drawer SERVICE_OUT must be HTTP 422 (got {status}: {state})"
    );
    assert!(
        state.contains("CASH_INSUFFICIENT"),
        "S10: refusal must be CASH_INSUFFICIENT, got {state}"
    );
    assert_eq!(
        db_stats(app.db()).await.0,
        rows_before,
        "S10: refused SERVICE_OUT must mint ZERO fiscal rows (row-less guard)"
    );

    // fund with a CASH sell, then видача the FULL drawer → ACK, cash 0
    let (body, _t) = sell_body(2, "s10-SELL"); // even = cash
    let (ss, sstate) = drive_ingress(&app, &*wp, &body).await;
    assert_eq!(sstate, DocState::Ack.as_str(), "S10 fund sell → ACK [{ss}]");
    let drawer = cash_on_hand(app.db()).await;
    assert!(drawer > 0, "S10: drawer funded (got {drawer})");
    let (cs, cstate) =
        drive_ingress(&app, &*wp, &service_out_body(drawer, "s10-SVCOUT-covered")).await;
    assert_eq!(
        cstate,
        DocState::Ack.as_str(),
        "S10: covered SERVICE_OUT (exactly the drawer) → ACK [{cs}]"
    );
    assert_eq!(
        cash_on_hand(app.db()).await,
        0,
        "S10: видача of the full drawer → cash 0"
    );

    prro::db::invariant_scan::assert_clean(app.db()).await;
    eprintln!(
        "════════ S10-guard-3b: PASS — over-drawer видача refused row-less; covered видача drains drawer to 0 ════════"
    );
}

/// S11 — EPZ LEDGER (видача готівки за ЕПЗ subtracts the drawer). Fund the drawer
/// with a службове внесення, read it, then an EPZ cash advance ≤ drawer, and
/// assert the drawer dropped by EXACTLY the EPZ sum (card-funded cash handed out).
/// Close with a Z (the `<EPZ>` aggregation + z-quiescence include EPZ).
#[tokio::test]
async fn matrix_s11_epz_cash_advance_ledger() {
    let (app, _dps, wp) = fresh_harness().await;
    let (st, _) = drive_ingress(&app, &*wp, &simple_body("SHIFT_OPEN", "s11-OPEN")).await;
    assert_eq!(st, 200, "S11 setup: SHIFT_OPEN must ACK");
    // fund the drawer (+80.00)
    let (fi, fstate) = drive_ingress(&app, &*wp, &service_in_body(8000, "s11-SVCIN")).await;
    assert_eq!(
        fstate,
        DocState::Ack.as_str(),
        "S11 fund внесення → ACK [{fi}]"
    );
    let before = cash_on_hand(app.db()).await;
    assert!(before >= 3000, "S11: drawer funded ≥ 30.00 (got {before})");

    // EPZ cash advance of 30.00 (card-funded) → drawer -= 30.00
    let (es, estate) = drive_ingress(&app, &*wp, &epz_body(3000, 2, "s11-EPZ")).await;
    assert_eq!(estate, DocState::Ack.as_str(), "S11: EPZ → ACK [{es}]");
    let after = cash_on_hand(app.db()).await;
    assert_eq!(
        after,
        before - 3000,
        "S11: EPZ must subtract 30.00 from the drawer ({before} → {after})"
    );

    let (zs, zstate) = drive_ingress(&app, &*wp, &simple_body("Z_REPORT", "s11-Z")).await;
    assert_eq!(zstate, DocState::Ack.as_str(), "S11: Z → ACK [{zs}]");
    prro::db::invariant_scan::assert_clean(app.db()).await;
    eprintln!(
        "════════ S11-epz: PASS — EPZ 30.00 subtracted drawer {before}→{after}, Z clean ════════"
    );
}

/// S12 — EPZ guard teeth (two fail-closed refusals, both row-less / HTTP 422).
/// (a) EPZ over an EMPTY drawer → CASH_INSUFFICIENT (guard-3c, WebCheck errCode-47
/// analog). (b) EPZ with payment_form_index < 2 (a cash slot) → EPZ_PAYMENT_ID_
/// TOO_LOW (WebCheck «paymentid не может быть меньше 2»). Neither mints a row.
#[tokio::test]
async fn matrix_s12_epz_guards() {
    let (app, _dps, wp) = fresh_harness().await;
    let (st, _) = drive_ingress(&app, &*wp, &simple_body("SHIFT_OPEN", "s12-OPEN")).await;
    assert_eq!(st, 200, "S12 setup: SHIFT_OPEN must ACK");
    let rows0 = db_stats(app.db()).await.0;

    // (a) over-drawer EPZ on an empty drawer → guard-3c
    let (s1, st1) = drive_ingress(&app, &*wp, &epz_body(5000, 2, "s12-EPZ-over")).await;
    assert_eq!(
        s1, 422,
        "S12(a): over-drawer EPZ must be 422 (got {s1}: {st1})"
    );
    assert!(
        st1.contains("CASH_INSUFFICIENT"),
        "S12(a): must be CASH_INSUFFICIENT, got {st1}"
    );

    // (b) paymentid < 2 (cash slot) → EPZ_PAYMENT_ID_TOO_LOW (checked before the
    // cash guard, so it fires even on an empty drawer)
    let (s2, st2) = drive_ingress(&app, &*wp, &epz_body(1000, 1, "s12-EPZ-payid")).await;
    assert_eq!(
        s2, 422,
        "S12(b): paymentid<2 EPZ must be 422 (got {s2}: {st2})"
    );
    assert!(
        st2.contains("EPZ_PAYMENT_ID_TOO_LOW"),
        "S12(b): must be EPZ_PAYMENT_ID_TOO_LOW, got {st2}"
    );

    assert_eq!(
        db_stats(app.db()).await.0,
        rows0,
        "S12: both refused EPZ mint ZERO fiscal rows"
    );
    prro::db::invariant_scan::assert_clean(app.db()).await;
    eprintln!(
        "════════ S12-epz-guards: PASS — over-drawer→CASH_INSUFFICIENT, paymentid<2→EPZ_PAYMENT_ID_TOO_LOW, row-less ════════"
    );
}

/// S13 — OFFLINE BIMODAL service-io + EPZ (the path the L3/EPZ unit suites do NOT
/// drive — researcher-flagged gap: внесення/видача/EPZ through the OFFLINE lane →
/// OFFLINE_LOCAL_ACK → drain). Fund the drawer online, drop the net, GO_OFFLINE,
/// then run внесення / видача / EPZ OFFLINE (each must land OFFLINE_LOCAL_ACK, the
/// guards reading the OLA-inclusive drawer), restore + drain (all converge to
/// terminal), then Z. End-to-end coverage of the offline branch for all three ops.
#[tokio::test]
async fn matrix_s13_offline_service_io_epz_bimodal() {
    let (app, dps, wp) = fresh_harness().await;
    let steps = vec![
        Action::Open,
        Action::Sell(4),          // even = CASH sell → fund the drawer online
        Action::ServiceIn(10000), // +100.00 online (headroom for offline out/epz)
        Action::NetDown,
        Action::GoOffline,
        Action::ServiceIn(3000),  // offline внесення → OLA
        Action::ServiceOut(2000), // offline видача → OLA (drawer covers)
        Action::Epz(1000),        // offline EPZ → OLA (drawer covers)
        Action::NetUp,
        Action::GoOnlineDrain, // all OLA docs drain to terminal
        Action::ZClose,
    ];
    run("S13-offline-svc-epz", &steps, &app, &*wp, &dps).await;
}
