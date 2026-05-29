//! W2 PR-B piece 9 — MED-PR90-03 missing-operator end-to-end (scoped).
//!
//! Per `docs/superpowers/plans/2026-05-25-m4-ingress-plan.md` §3 W2
//! Acceptance MED-PR90-03 integration sub-test:
//!
//!   "start prro з one configured FN but operators table empty; submit
//!    HTTP request → 503 + error_code = OPERATOR_NOT_REGISTERED.  This
//!    proves the full chain from boot-time absence → handler refusal."
//!
//! ## Scoped to PR-B
//!
//! The HTTP handler that converts `registry.get(fn) == None` into a
//! `503 OPERATOR_NOT_REGISTERED` response does not exist yet — it
//! lands in W5 (handler bind + DTO routing).  Per arch-planner default
//! resolution for piece 9, this fixture verifies the PRE-handler half
//! of the chain end-to-end through the W2 surface that DOES exist:
//!
//!   1. `App::boot` opens both pools.
//!   2. A configured FN is present in `fiscal_number_config`.
//!   3. The `operators` table is empty (operator never registered, or
//!      registration failed offline).
//!   4. `BindingsRegistry::build_from_db` against the App's pools
//!      returns an empty registry.
//!   5. `registry.get(fn_id)` returns `None`.
//!   6. The boot-time `OPERATOR_NOT_REGISTERED` Info audit is emitted
//!      (this is the signal an operator runbook watches for —
//!      handler 503 in W5 is a downstream observable, but the upstream
//!      observable here is the audit row).
//!
//! When W5 lands, this file gets extended (or a sibling fixture lands)
//! that POSTs to the real ingress handler and asserts the HTTP 503 +
//! `error_code` body.  Both halves carry the MED-PR90-03 acceptance
//! label so reviewers can trace the contract end-to-end.

mod common;

use prro::admin::AdminError;
use prro::config::AppConfig;
use prro::db::models::enums::{FiscalMode, Severity};
use prro::db::repositories::{audit_log, fiscal_number_config as fn_cfg};
use prro::runtime::bindings::{BindingsRegistry, KeyLoadFailure, OperatorKeyLoader};
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::channel::DpsChannel;
use std::path::Path;
use std::sync::Arc;

/// Loader that panics if ever called — proves the missing-operator
/// path never reaches key loading.
struct UnreachableLoader;

#[async_trait::async_trait]
impl OperatorKeyLoader for UnreachableLoader {
    async fn load(
        &self,
        _key_path: &Path,
        _password: &[u8],
    ) -> Result<SigningContext, KeyLoadFailure> {
        panic!("UnreachableLoader: build_from_db reached key load with no operators row");
    }
}

#[tokio::test]
async fn missing_operator_yields_registry_none_and_audit_for_configured_fn() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("main.db");
    let secure_db_path = dir.path().join("secure.db");
    let toml_text = format!(
        r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "{0}"
secure_db_path = "{1}"

[admin_ui]
enabled = false
listen  = "127.0.0.1:8443"
"#,
        db_path.display().to_string().replace('\\', "/"),
        secure_db_path.display().to_string().replace('\\', "/"),
    );
    let cfg = AppConfig::from_toml(&toml_text).expect("config parse");
    let app = prro::App::boot(cfg).await.expect("App::boot");

    // Register the FN in main config — but DO NOT add any operators
    // row (mirrors "operator never registered" production scenario).
    fn_cfg::insert(
        app.db(),
        &fn_cfg::NewFnConfig {
            fiscal_number: "4000000001".into(),
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
        },
    )
    .await
    .expect("seed FN config");

    let dps: Arc<dyn DpsChannel> = Arc::new(common::StubDpsChannel::new(Ok(common::ack("ack"))));

    let registry =
        BindingsRegistry::build_from_db(app.db_secure(), app.db(), dps, &UnreachableLoader)
            .await
            .expect("build_from_db must not abort on empty operators");

    // Contract #5 — registry.get(fn) returns None.
    assert!(
        registry.get("4000000001").is_none(),
        "registry must NOT bind a FN that has no operators row"
    );

    // Contract #6 — boot-time OPERATOR_NOT_REGISTERED audit emitted
    // with Severity::Info (NOT Critical — no key material leaked,
    // just configuration drift).
    let audits = audit_log::list_for_entity(app.db(), "operator", "4000000001", 20)
        .await
        .expect("audit query");
    let not_reg = audits
        .iter()
        .find(|a| a.event_type == "OPERATOR_NOT_REGISTERED")
        .expect(
            "OPERATOR_NOT_REGISTERED audit emitted — this is the upstream signal \
             the runbook watches; W5 handler 503 is the downstream observable",
        );
    assert_eq!(not_reg.severity, Severity::Info);

    // Sanity — UnreachableLoader was never invoked (key load is
    // unreachable when there is no operators row to act on).
    drop(app);

    // Compile-time anchor for AdminError import (silences unused
    // import lint without burying the trace for grep-based review).
    let _ = std::mem::size_of::<AdminError>();
}
