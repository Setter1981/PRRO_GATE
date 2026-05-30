//! RS-1 Piece 5 — supervisor boot-to-shutdown integration test.
//!
//! 5a: the `run_with_registry` seam builds over a (here empty) registry,
//! awaits the shutdown future, and returns cleanly (dropping the App).
//! This file GROWS with reconcile-once (5c) + the drain/probe loops (5d) +
//! the watch-flip-and-join graceful shutdown (5e).

mod common;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use prro::config::AppConfig;
use prro::crypto::session::SigningSession;
use prro::db::models::enums::FiscalMode;
use prro::db::repositories::{fiscal_number_config as fn_cfg, operators as ops_repo};
use prro::runtime::bindings::{BindingsRegistry, KeyLoadFailure, OperatorKeyLoader};
use prro::runtime::coding::Coding;
use prro::runtime::supervisor;
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::channel::DpsChannel;
use prro::App;

/// Real DER DSTU-4145 cert so per-tick `build_fn_sign` produces a well-formed
/// CMS (the scalar is arbitrary — well-formedness, not on-wire acceptance).
const FIXTURE_CERT_DER: &[u8] = include_bytes!("fixtures/SELF_SIGNED_ENC_6929.cer");

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

/// A loader that returns a SigningContext whose session carries the REAL cert
/// fixture, so the supervisor's per-tick `build_fn_sign` succeeds and the
/// drain/probe tick bodies are actually exercised (the empty-cert
/// `det_signing_ctx_for` would make `build_fn_sign` fail → FN skipped).
struct FixtureLoader;

#[async_trait]
impl OperatorKeyLoader for FixtureLoader {
    async fn load(
        &self,
        operator_id: &str,
        _key_path: &Path,
        _password: &[u8],
    ) -> Result<SigningContext, KeyLoadFailure> {
        let mut ctx = common::det_signing_ctx_for(operator_id);
        ctx.session =
            SigningSession::new_for_test(operator_id.to_string(), [7u8; 32], FIXTURE_CERT_DER.to_vec());
        Ok(ctx)
    }
}

fn fn_config(fn_id: &str) -> fn_cfg::NewFnConfig {
    fn_cfg::NewFnConfig {
        fiscal_number: fn_id.into(),
        tax_number: "12345678".into(),
        vat_payer_inn: None,
        fiscal_mode: FiscalMode::Test,
        org_name: None,
        point_name: None,
        org_address: None,
        tsp_enabled: false,
        offline_enabled: true,
        national_check_enabled: true,
        min_offline_codes: 0,
        max_offline_codes: 0,
    }
}

fn new_op(operator_id: &str, fn_id: &str, key_path: &str, password: &[u8]) -> ops_repo::NewOperator {
    ops_repo::NewOperator {
        operator_id: operator_id.into(),
        fiscal_number: fn_id.into(),
        name: "Cashier".into(),
        key_path: key_path.into(),
        key_pass_enc: Coding::encode(password).expect("encode test password"),
    }
}

/// 1-FN happy path: boot reconcile-once + spawn the drain + probe loops over a
/// single registered FN (real cert → `build_fn_sign` succeeds), let the
/// immediate first tick of each loop run, then signal shutdown.  Asserts the
/// supervisor joins both loops and returns cleanly — no hang, no panic.
///
/// The node defaults to `Online` (reconcile seeds it), so the probe tick skips
/// before any wire call and the empty backlog makes the drain tick a no-op;
/// this test exercises the supervisor WIRING (resolver/fn_sign freshness +
/// watch-flip-join lifecycle), not the primitives (covered by W8/W9b tests).
#[tokio::test]
async fn supervisor_runs_loops_over_one_fn_and_shuts_down_cleanly() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = AppConfig::from_toml(&cfg_toml(dir.path())).expect("parse cfg");
    let app = App::boot(cfg).await.expect("boot");

    fn_cfg::insert(app.db(), &fn_config("4000000001"))
        .await
        .expect("seed FN config");
    ops_repo::insert(
        app.db_secure(),
        &new_op("OP-1", "4000000001", "/tmp/k1.dat", b"secret1"),
    )
    .await
    .expect("seed operator");

    let dps: Arc<dyn DpsChannel> = Arc::new(common::StubDpsChannel::new(Ok(common::ack("t"))));
    let registry = Arc::new(
        BindingsRegistry::build_from_db(app.db_secure(), app.db(), dps, &FixtureLoader)
            .await
            .expect("build_from_db"),
    );
    assert_eq!(registry.len(), 1, "one operator seeded → one FN");

    // Delayed shutdown lets the immediate first drain+probe tick run (each
    // rebuilds fn_sign FRESH over the real cert) before the loops stop.
    let shutdown = async {
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    let res = tokio::time::timeout(
        Duration::from_secs(5),
        supervisor::run_with_registry(app, registry, shutdown),
    )
    .await
    .expect("supervisor must not hang on the shutdown join");
    assert!(res.is_ok(), "supervisor must shut down cleanly: {res:?}");
}
