//! SHIFT-LIFE MATRIX — full-stack scenario harness through the LIVE binding.
//!
//! Operator's directive (2026-07-12): stop verifying the online↔offline
//! transition matrix cell-by-cell with isolated unit tests. Instead **stand the
//! product up**, feed it GENERATED packets of receipts under different shift
//! variants, run each through the WHOLE real path (ingress → write-path →
//! transport) with legible logging, and **halt on the first divergence**. The
//! only thing faked is the network (`MatrixDps` — a stateful, fault-injectable
//! `DpsChannel` whose `online` flag models a real net drop).
//!
//! WHAT IS REAL: `App::boot` (real SQLite, real migrations), the real per-FN
//! signing context, the real registry, the real `production_write_path` /
//! `InlineWritePath` binding, the real ingress seam (`WritePathEntry::fiscalize`
//! — every command enters exactly as the ingress handler drives it), the real
//! staged pipeline, the real offline lane, the real live GO_OFFLINE / GO_ONLINE
//! doors, and the real drain. ONLY the DPS transport is a stub.
//!
//! WHY in-process (not a spawned OS process over HTTP): the transition-matrix
//! goal is state correctness across online↔offline↔drain, which is faster,
//! deterministic, and directly assertable in-process. The OS-process / HTTP-
//! adapter-wire layer is an orthogonal concern (does the REST adapter translate
//! bytes correctly) — a candidate follow-up harness, not this one.
//!
//! HOW IT DIFFERS from the pilot e2e tests (`pilot_online_half_e2e`,
//! `pilot_offline_full_drill_e2e`): those replay THREE hardcoded receipts. This
//! harness GENERATES varied packets (`gen_sell` — varied item counts, prices,
//! cash/card payforms) so genuinely diverse documents flow through the path
//! under scripted shift compositions. "Они без новых документов" — this one is
//! not.
//!
//! HALT-ON-ERROR: `invariant_scan::assert_clean` runs after EVERY step; any
//! illegal non-terminal doc at a quiescent boundary (the #192 / P1 ledger-pin)
//! panics the scenario with the full per-step log printed above it.

mod common;

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use prro::app::App;
use prro::config::AppConfig;
use prro::crypto::session::SigningSession;
use prro::db::models::enums::{DocState, FiscalMode, NodeMode, Protocol, ShiftState};
use prro::db::models::ids::RequestId;
use prro::db::repositories::ingress_inbox::{
    self as inbox, InboxInsertOutcome, InboxRow, NewInboxEntry,
};
use prro::db::repositories::{fiscal_number_config as fn_cfg, operators as ops_repo};
use prro::runtime::bindings::{BindingsRegistry, KeyLoadFailure, OperatorKeyLoader};
use prro::runtime::coding::Coding;
use prro::runtime::ingress::inline_binding::production_write_path;
use prro::runtime::ingress::seam::{FiscalError, FiscalOutcome, WritePathEntry};
use prro::services::reconciliation::runtime::RuntimeView;
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
const CASHIER: &str = "test-cashier";
const DRIVER: &str = "drv-test";
const SHIFT_OPEN_PAYLOAD: &str = r#"{"opening_sum_kop":0}"#;
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
// Packet generator — varied receipts (NOT one canned check)
// ════════════════════════════════════════════════════════════════════════

/// Deterministic-from-`seed` SELL payload with varied item count (1–3),
/// varied per-item prices, and alternating cash/card payform. Returns
/// `(payload_json, total_kop)`. Reproducible: same seed → same packet.
fn gen_sell(seed: u64) -> (String, i64) {
    let n_items = 1 + (seed % 3) as usize;
    let mut items = String::new();
    let mut total: i64 = 0;
    for k in 0..n_items {
        let price = 1000 + (((seed >> (k * 3)) % 90) as i64 + 1) * 100; // 1100..=10000
        total += price;
        if k > 0 {
            items.push(',');
        }
        items.push_str(&format!(
            r#"{{"code":"item-{k}","name":"Item {k}","price_kop":{price},"quantity_thousandths":1000,"sum_kop":{price}}}"#
        ));
    }
    let (name, type_code) = if seed % 2 == 0 {
        ("Cash", "0")
    } else {
        ("Card", "1")
    };
    let payload = format!(
        r#"{{"items":[{items}],"payments":[{{"name":"{name}","sum_kop":{total},"type_code":"{type_code}"}}]}}"#
    );
    (payload, total)
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
// Ingress driver + probes
// ════════════════════════════════════════════════════════════════════════

fn entry(op: &str, payload: &str, idem: &str, total: Option<i64>) -> NewInboxEntry {
    let request_id: [u8; 16] = *RequestId::new().as_bytes();
    let payload_sha256_canonical: [u8; 32] = Sha256::digest(payload.as_bytes()).into();
    NewInboxEntry {
        request_id,
        fiscal_number: FN.into(),
        protocol: Protocol::Rest,
        operation_type: op.into(),
        idempotency_key: idem.into(),
        payload_json: payload.into(),
        payload_sha256_canonical,
        correlation_id: None,
        signed_by_cashier_id: Some(CASHIER.into()),
        driver_id: Some(DRIVER.into()),
        business_ts: Some("2026-07-12T12:00:00Z".into()),
        total_sum_kop: total,
    }
}

async fn drive(
    wp: &dyn WritePathEntry,
    pool: &SqlitePool,
    entry: NewInboxEntry,
) -> Result<FiscalOutcome, FiscalError> {
    let row: InboxRow = match inbox::insert(pool, &entry).await.unwrap() {
        InboxInsertOutcome::Created(row) => row,
        other => panic!("expected a fresh Created inbox row, got {other:?}"),
    };
    wp.fiscalize(&row).await
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
}

/// Run a scenario end-to-end through the live binding, logging every step and
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
        let outcome: String = match act {
            Action::Open => {
                let out = drive(
                    wp,
                    app.db(),
                    entry(
                        "SHIFT_OPEN",
                        SHIFT_OPEN_PAYLOAD,
                        &format!("mx-{name}-OPEN-{i}"),
                        None,
                    ),
                )
                .await
                .expect("SHIFT_OPEN must reach terminal ACK (net up)");
                assert_eq!(out.document_state, DocState::Ack, "SHIFT_OPEN → ACK");
                assert_eq!(
                    shift_state(app.db()).await,
                    "OPENED",
                    "shift OPENED after open"
                );
                format!("SHIFT_OPEN → {:?}", out.document_state)
            }
            Action::Sell(seed) => {
                let (payload, total) = gen_sell(*seed);
                let out = drive(
                    wp,
                    app.db(),
                    entry(
                        "SELL",
                        &payload,
                        &format!("mx-{name}-SELL-{i}"),
                        Some(total),
                    ),
                )
                .await
                .unwrap_or_else(|e| panic!("SELL step {i} (seed {seed}) errored: {e:?}"));
                let mode = node_row(app.db()).await.0;
                let want = if mode == "OFFLINE" {
                    DocState::OfflineLocalAck
                } else {
                    DocState::Ack
                };
                assert_eq!(
                    out.document_state, want,
                    "SELL step {i}: mode={mode} → expected {want:?}"
                );
                format!("SELL(seed={seed},total={total}) → {:?}", out.document_state)
            }
            Action::Return(seed) => {
                let (payload, total) = gen_sell(*seed);
                let out = drive(
                    wp,
                    app.db(),
                    entry(
                        "RETURN",
                        &payload,
                        &format!("mx-{name}-RET-{i}"),
                        Some(total),
                    ),
                )
                .await
                .unwrap_or_else(|e| panic!("RETURN step {i} (seed {seed}) errored: {e:?}"));
                let mode = node_row(app.db()).await.0;
                let want = if mode == "OFFLINE" {
                    DocState::OfflineLocalAck
                } else {
                    DocState::Ack
                };
                assert_eq!(out.document_state, want, "RETURN step {i}: mode={mode}");
                format!("RETURN(seed={seed}) → {:?}", out.document_state)
            }
            Action::ZClose => {
                let out = drive(
                    wp,
                    app.db(),
                    entry("Z_REPORT", "{}", &format!("mx-{name}-Z-{i}"), None),
                )
                .await
                .expect("Z_REPORT must close the shift and reach terminal ACK");
                assert_eq!(out.document_state, DocState::Ack, "Z → ACK");
                assert_eq!(
                    shift_state(app.db()).await,
                    "CLOSED",
                    "shift CLOSED after Z"
                );
                format!("Z_REPORT → {:?} (shift CLOSED)", out.document_state)
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
                // Convergence: no OLA left, node back ONLINE.
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
        };

        let (mode, node_shift) = node_row(app.db()).await;
        let wire = dps.drain_calls();
        eprintln!(
            "[{name}][step {i:>2}] {outcome:<48} | mode={mode:<12} shift={:<10} node_shift={node_shift:<10} cash={} | dps{:?}",
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
// Increment 1 scenarios — harness self-proof on KNOWN-GREEN paths
// ════════════════════════════════════════════════════════════════════════

/// S1 — online happy life: open, three VARIED sells, one return, close.
/// Proves the harness drives varied generated packets through the real path to
/// terminal ACK on a clean scan at every step.
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
/// operator flagged as "only think it works" — now watched end-to-end.
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
