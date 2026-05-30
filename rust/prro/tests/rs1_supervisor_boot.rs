//! RS-1 Piece 5 — supervisor boot-to-shutdown integration test.
//!
//! 5a: the `run_with_registry` seam builds over a (here empty) registry,
//! awaits the shutdown future, and returns cleanly (dropping the App).
//! This file GROWS with reconcile-once (5c) + the drain/probe loops (5d) +
//! the watch-flip-and-join graceful shutdown (5e).

mod common;

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use prro::config::AppConfig;
use prro::runtime::bindings::{BindingsRegistry, KeyLoadFailure, OperatorKeyLoader};
use prro::runtime::supervisor;
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::channel::DpsChannel;
use prro::App;

fn cfg_toml(dir: &Path) -> String {
    let db = dir.join("main.db");
    let secure = dir.join("secure.db");
    format!(
        r#"
app_name = "prro"
version = "0.1.0"

[database]
db_path = "{}"
secure_db_path = "{}"

[admin_ui]
enabled = false
listen = "127.0.0.1:8081"
"#,
        db.display(),
        secure.display()
    )
}

/// A loader that panics if ever called — the empty-registry test asserts
/// it is NOT reached (no operators rows → no key load).
struct NoLoader;

#[async_trait]
impl OperatorKeyLoader for NoLoader {
    async fn load(
        &self,
        _operator_id: &str,
        _key_path: &Path,
        _password: &[u8],
    ) -> Result<SigningContext, KeyLoadFailure> {
        panic!("NoLoader: build_from_db reached key load with no operators row");
    }
}

#[tokio::test]
async fn supervisor_run_with_empty_registry_awaits_shutdown_and_returns() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = AppConfig::from_toml(&cfg_toml(dir.path())).expect("parse cfg");
    let app = App::boot(cfg).await.expect("boot");

    let dps: Arc<dyn DpsChannel> = Arc::new(common::StubDpsChannel::new(Ok(common::ack("t"))));
    let registry = Arc::new(
        BindingsRegistry::build_from_db(app.db_secure(), app.db(), dps, &NoLoader)
            .await
            .expect("build_from_db"),
    );
    assert_eq!(registry.len(), 0, "no operators seeded → empty registry");

    // Immediate shutdown → run_with_registry must return cleanly (drops App).
    let res = supervisor::run_with_registry(app, registry, async {}).await;
    assert!(res.is_ok(), "supervisor must shut down cleanly: {res:?}");
}
