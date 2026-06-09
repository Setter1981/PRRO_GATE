//! RS-3 A4 — `App`-level wiring of the per-`fiscal_number` write-path gate.
//!
//! The gate primitive itself is unit-tested in `src/runtime/fn_gate.rs`; here
//! we prove `App` EXPOSES it AND that all `App` clones gate against ONE
//! instance (it lives in the shared `Arc<Inner>`).  That shared-via-`Arc`
//! property is exactly what A2 relies on: the axum per-request `IngressState`
//! and the supervisor loops are `App` clones, and they must serialize the same
//! FN against each other.

use std::time::Duration;

use prro::{config::AppConfig, App};
use tokio::time::timeout;

const FN_A: &str = "4000000001";
const FN_B: &str = "4000000002";

async fn boot_app() -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir
        .path()
        .join("a.db")
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
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    let app = App::boot(cfg).await.unwrap();
    (dir, app)
}

/// Two `App` clones gate against ONE instance: while a clone holds FN_A, the
/// original blocks on FN_A (the gate is shared via `Arc<Inner>`, not
/// per-clone), then proceeds once the clone releases.
#[tokio::test]
async fn app_clones_share_one_per_fn_gate() {
    let (_dir, app) = boot_app().await;
    let clone = app.clone();

    let held = clone.acquire_fn_gate(FN_A).await;

    let mut second = Box::pin(app.acquire_fn_gate(FN_A));
    assert!(
        timeout(Duration::from_millis(200), &mut second)
            .await
            .is_err(),
        "an App clone holding FN_A must block the original on the same FN (one shared gate)"
    );

    drop(held);
    let _g = timeout(Duration::from_millis(500), &mut second)
        .await
        .expect("the gate must free once the holding clone releases");
}

/// Different FNs do not contend, even across clones.
#[tokio::test]
async fn app_gate_does_not_block_across_distinct_fns() {
    let (_dir, app) = boot_app().await;
    let _ga = app.acquire_fn_gate(FN_A).await;

    let clone = app.clone();
    let _gb = timeout(Duration::from_millis(200), clone.acquire_fn_gate(FN_B))
        .await
        .expect("a different FN must not block on FN_A's gate, even across clones");
}
