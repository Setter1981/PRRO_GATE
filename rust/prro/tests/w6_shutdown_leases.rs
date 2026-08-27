//! W6 (Tier-3) — shutdown with held / waiting per-FN leases (dossier §7's last clause).
//!
//! These pins fix the EXISTING contract between the supervisor's graceful shutdown and the
//! per-FN write gate (frozen invariant #9: graceful shutdown matters more than finishing
//! fast).  The load-bearing facts, each pinned below rather than implied:
//!
//! - The gate is NOT a supervised resource: shutdown neither waits for a held lease nor
//!   seizes it — the lease survives the supervisor's return untouched (RAII only).
//! - Waiters queued on a held gate are NOT poisoned or dropped by shutdown: they keep
//!   waiting, and complete in FIFO order once the holder releases.
//! - Grace-elapse DETACHES a still-running task, never aborts it: a detached task that
//!   holds a lease KEEPS holding it after `supervise_task_set` returns — the documented
//!   crash-safe posture (the per-doc write path is durable; the process exit is the final
//!   release).
//!
//! What is deliberately NOT here: the axum in-flight-request drain (axum 0.7
//! `with_graceful_shutdown` semantics — new accepts stop, in-flight requests are awaited)
//! is upstream behavior our ingress wiring inherits; K3/K4 own the crash-recovery of an op
//! cut at process exit.

use std::time::Duration;

use prro::config::AppConfig;
use prro::runtime::supervisor::{self, SupervisedTask};
use prro::App;
use tokio::sync::watch;
use tokio::time::timeout;

const FN: &str = "4000000001";

async fn boot_app(grace_seconds: Option<u64>) -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir
        .path()
        .join("w6sd.db")
        .display()
        .to_string()
        .replace('\\', "/");
    let supervisor_block = match grace_seconds {
        Some(s) => format!("\n[supervisor]\nshutdown_grace_seconds = {s}\n"),
        None => String::new(),
    };
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
{supervisor_block}"#
    );
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    let app = App::boot(cfg).await.unwrap();
    (dir, app)
}

/// A well-behaved supervised loop: parks on the watch and returns on the flip.
fn watch_abiding_stub(
    mut rx: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<anyhow::Result<()>> {
    tokio::spawn(async move {
        loop {
            if rx.changed().await.is_err() || *rx.borrow() {
                return Ok(());
            }
        }
    })
}

/// Shutdown completes promptly while a per-FN lease is HELD, and neither waits for it nor
/// seizes it: the lease is intact after the supervisor returns, and an ordinary release
/// still works.  A supervisor that force-released (or awaited) held gates would either
/// break single-writer under a racing op or hang shutdown behind a wire call — both are
/// the regression classes this pin names.
#[tokio::test]
async fn shutdown_completes_with_a_held_lease_and_does_not_seize_it() {
    let (_dir, app) = boot_app(None).await;
    let held = app.acquire_fn_gate(FN).await;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let stub = watch_abiding_stub(shutdown_rx);
    let res = timeout(
        Duration::from_secs(5),
        supervisor::supervise_task_set(
            app.clone(),
            async {},
            shutdown_tx,
            vec![SupervisedTask::runs_until_shutdown("stub", stub)],
        ),
    )
    .await
    .expect("shutdown with a held lease must be bounded — the gate is not a supervised task");
    assert!(res.is_ok(), "clean shutdown returns Ok: {res:?}");

    // The lease was neither seized nor poisoned by the shutdown…
    assert!(
        timeout(Duration::from_millis(300), app.acquire_fn_gate(FN))
            .await
            .is_err(),
        "the lease must STILL be held after the supervisor returns — a force-release here \
         would put a second writer behind a fiscal number mid-op (invariant #2)"
    );
    // …and releases normally afterwards.
    drop(held);
    let reacquired = timeout(Duration::from_secs(1), app.acquire_fn_gate(FN)).await;
    assert!(
        reacquired.is_ok(),
        "an ordinary RAII release must still hand the gate over after shutdown"
    );
}

/// Waiters queued on a held gate SURVIVE the supervisor's shutdown — no poisoning, no
/// silent drop — and complete in FIFO order once the holder releases.  Today a waiter's
/// only escape is cancellation-by-drop at process exit; this pin fixes that contract so a
/// future "reject new acquirers during drain" feature must change it CONSCIOUSLY.
#[tokio::test]
async fn gate_waiters_survive_shutdown_and_complete_in_order_on_release() {
    let (_dir, app) = boot_app(None).await;
    let held = app.acquire_fn_gate(FN).await;

    let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut waiters = Vec::new();
    for i in 0..2u32 {
        let app = app.clone();
        let order = order.clone();
        waiters.push(tokio::spawn(async move {
            let g = app.acquire_fn_gate(FN).await;
            order.lock().unwrap().push(i);
            drop(g);
        }));
        tokio::task::yield_now().await;
    }

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let stub = watch_abiding_stub(shutdown_rx);
    supervisor::supervise_task_set(
        app.clone(),
        async {},
        shutdown_tx,
        vec![SupervisedTask::runs_until_shutdown("stub", stub)],
    )
    .await
    .expect("clean shutdown");

    // Both waiters are still parked — shutdown did not drop or error them out.
    assert!(
        waiters.iter().all(|w| !w.is_finished()),
        "queued waiters must survive the supervisor's shutdown untouched"
    );

    // Release: the queue drains, in arrival order.
    drop(held);
    for (i, w) in waiters.into_iter().enumerate() {
        timeout(Duration::from_secs(5), w)
            .await
            .unwrap_or_else(|_| panic!("waiter {i} never completed after release"))
            .expect("waiter task panicked");
    }
    assert_eq!(
        *order.lock().unwrap(),
        vec![0, 1],
        "post-shutdown release must serve waiters in arrival order"
    );
}

/// Grace-elapse DETACHES a still-running task — it does NOT abort it.  A supervised task
/// that holds a per-FN lease and ignores the flip keeps holding that lease after
/// `supervise_task_set` returns Ok: the documented crash-safe posture (the write path is
/// per-doc durable; the process exit is the final release), and the documented hazard — a
/// detached task's `App` clone keeps pools + pid-lock alive until it ends.  An "abort on
/// deadline" change would cut a write path mid-op and MUST redden this pin first.
#[tokio::test]
async fn grace_detach_leaves_a_task_held_lease_held() {
    // Grace clamped floor is 1s — small enough to keep the pin fast.
    let (_dir, app) = boot_app(Some(1)).await;

    let (shutdown_tx, _shutdown_rx) = watch::channel(false);
    let holder = {
        let app = app.clone();
        tokio::spawn(async move {
            let _g = app.acquire_fn_gate(FN).await;
            // Ignore the flip entirely; outlast the grace by far.
            tokio::time::sleep(Duration::from_secs(60)).await;
            anyhow::Ok(())
        })
    };
    tokio::task::yield_now().await;

    let start = std::time::Instant::now();
    let res = timeout(
        Duration::from_secs(5),
        supervisor::supervise_task_set(
            app.clone(),
            async {},
            shutdown_tx,
            vec![SupervisedTask::runs_until_shutdown("holder", holder)],
        ),
    )
    .await
    .expect("grace-elapse must detach, bounded by the shared grace");
    assert!(
        res.is_ok(),
        "grace-elapse with a detached task is a NORMAL shutdown (crash-safe posture): {res:?}"
    );
    assert!(
        start.elapsed() < Duration::from_millis(3500),
        "bounded by ONE shared grace (~1s), took {:?}",
        start.elapsed()
    );

    // The detached task is alive and still holds the lease — detach, never abort.
    assert!(
        timeout(Duration::from_millis(300), app.acquire_fn_gate(FN))
            .await
            .is_err(),
        "the detached task must STILL hold its lease — an abort-on-deadline would have cut \
         a write path mid-op, the exact thing invariant #9 forbids"
    );
}
