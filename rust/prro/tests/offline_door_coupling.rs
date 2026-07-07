//! A′.3 O2 — the offline door-flip coupling-pin.
//!
//! Ship-together invariant (mirrors `z_builder`'s coupling-pin): flipping
//! `FULL_OFFLINE_SURFACE_READY = true` opens the operator GO_OFFLINE door, and
//! that is only safe because a DRAIN PATH exists to converge the
//! `OFFLINE_LOCAL_ACK` backlog the door can accrue. Opening the door WITHOUT a
//! reachable drain re-opens the stranded-backlog hazard.
//!
//! This pin is deliberately FLAG-INDEPENDENT — it proves the drain tick is
//! wired and runs for a `GOING_ONLINE` FN (the door's precondition). Reverting
//! the flag flip does NOT touch this pin (drain is independent of the door
//! flag); removing the drain wiring breaks it. Teeth: `tests/offline_surface.rs`
//! (the flip) REDs on a flag revert while THIS pin stays GREEN.

mod common;

use std::sync::Arc;

use prro::config::AppConfig;
use prro::db::models::enums::{NodeMode, ShiftState};
use prro::services::reconciliation::runtime::RuntimeView;
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::dto::{CheckAck, CheckSignBlob};
use prro::transports::dps::error::DpsError;
use prro::ScheduledDrainOutcome;
use sqlx::SqlitePool;

use common::{det_signing_ctx, StubDpsChannel};

const FN: &str = "1234567890";

async fn boot_app(db_filename: &str) -> (tempfile::TempDir, prro::App, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join(db_filename);
    let toml_text = format!(
        r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "{0}"
secure_db_path = "{0}_secure"

[admin_ui]
enabled = false
listen  = "127.0.0.1:8443"
"#,
        db_path.display().to_string().replace('\\', "/")
    );
    let cfg = AppConfig::from_toml(&toml_text).expect("config parse");
    let app = prro::App::boot(cfg).await.expect("App::boot");
    let pool = app.db().clone();
    (dir, app, pool)
}

async fn seed_fn_config(pool: &SqlitePool) {
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(FN)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_node_state(pool: &SqlitePool, mode: NodeMode, shift: ShiftState) {
    sqlx::query(
        "INSERT INTO node_state(fiscal_number, mode, shift_state, next_lnd) \
         VALUES (?, ?, ?, 100)",
    )
    .bind(FN)
    .bind(mode)
    .bind(shift)
    .execute(pool)
    .await
    .unwrap();
}

struct DepsCarriers {
    dps: Arc<StubDpsChannel>,
    signing_ctx: SigningContext,
    fn_sign: CheckSignBlob,
}

fn carriers(responses: Vec<Result<CheckAck, DpsError>>) -> DepsCarriers {
    DepsCarriers {
        dps: Arc::new(StubDpsChannel::with_queue(responses)),
        signing_ctx: det_signing_ctx(),
        fn_sign: CheckSignBlob(vec![0xAB, 0xCD]),
    }
}

fn view_for(carriers: &DepsCarriers) -> RuntimeView<'_> {
    RuntimeView {
        dps: carriers.dps.as_ref(),
        signing_ctx: &carriers.signing_ctx,
        fn_sign: &carriers.fn_sign,
    }
}

/// The door-flip is coupled to a reachable drain: a `GOING_ONLINE` FN drives a
/// real (empty-cohort) drain pass through the scheduled entry — proving the
/// convergence path the open door depends on is wired. Flag-independent.
#[tokio::test]
async fn offline_door_flip_coupled_to_reachable_drain() {
    let (_d, app, pool) = boot_app("coupling_drain.db").await;
    seed_fn_config(&pool).await;
    seed_node_state(&pool, NodeMode::GoingOnline, ShiftState::Opened).await;

    let c = carriers(vec![]);
    let view = view_for(&c);
    let outcome = app
        .drain_offline_backlog_scheduled(FN, &view)
        .await
        .expect("the drain path the open door depends on must be reachable");
    match outcome {
        ScheduledDrainOutcome::Ran(summary) => {
            assert_eq!(
                summary.backlog_size_before(),
                0,
                "empty cohort drains cleanly"
            );
        }
        ScheduledDrainOutcome::SkippedBackoff { .. } => {
            panic!("a fresh GOING_ONLINE FN MUST run the drain — the door's precondition")
        }
    }
    drop(c);
}
