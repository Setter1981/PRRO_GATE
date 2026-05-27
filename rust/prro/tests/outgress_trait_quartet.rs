//! W4-Z0 piece 10 — outgress trait quartet architectural scaffolding tests.
//!
//! Verify: trait quartet compiles, FSCO pilot impls implement traits,
//! EVPZ placeholders implement traits with typed Unimplemented errors,
//! `dispatch_to_outgress` returns ProfileNotImplemented for EVPZ in pilot.

use prro::db::models::ids::DriverId;
use prro::db::models::enums::DocType;
use prro::runtime::outgress::{
    self, BuilderContext, CmsOverCheckSignedFileEnvelope, CmsOverDatEnvelope,
    DpsResponseParser, DpsTransport, DpsXmlBuilder, EvpzResponseParser, EvpzXmlBuilder,
    FscoResponseParser, FscoXmlBuilder, GrpcSendChkV2Transport, HttpsRestTransport,
    OutgressError, OutgressProfile, SignContext, SignEnvelope, TargetEndpoint,
};
use prro::services::write_path::types::CanonicalFiscalCommand;

fn dummy_cmd() -> CanonicalFiscalCommand {
    CanonicalFiscalCommand {
        doc_type: DocType::Sell,
        business_ts: "2026-05-27T10:00:00Z".to_string(),
        total_sum_kop: Some(2500),
        payload_json: "{}".to_string(),
        payload_sha256_canonical: [0u8; 32],
        signed_by_cashier_id: None,
        driver_id: Some(DriverId::new("maria304").unwrap()),
    }
}

fn dummy_builder_context() -> BuilderContext {
    BuilderContext {
        fiscal_number: "4538765845".to_string(),
        tax_number: "TN-12345".to_string(),
        previous_hash: None,
        z_number: 0,
        local_number: 1,
        ts_str: "20260527100000".to_string(),
    }
}

fn dummy_target() -> TargetEndpoint {
    TargetEndpoint {
        host: "cabinet.tax.gov.ua".to_string(),
        port: 9443,
        path: "/".to_string(),
    }
}

// ─── Trait-implementation compile assertions ─────────────────────

#[test]
fn fsco_pilot_impls_implement_traits() {
    // The function compiles ⇒ the impls satisfy the trait bounds.
    fn _check_builder(_x: &dyn DpsXmlBuilder) {}
    fn _check_envelope(_x: &dyn SignEnvelope) {}
    fn _check_transport(_x: &(dyn DpsTransport + Sync)) {}
    fn _check_parser(_x: &dyn DpsResponseParser) {}
    _check_builder(&FscoXmlBuilder);
    _check_envelope(&CmsOverDatEnvelope);
    _check_transport(&GrpcSendChkV2Transport);
    _check_parser(&FscoResponseParser);
}

#[test]
fn evpz_placeholder_impls_implement_traits() {
    fn _check_builder(_x: &dyn DpsXmlBuilder) {}
    fn _check_envelope(_x: &dyn SignEnvelope) {}
    fn _check_transport(_x: &(dyn DpsTransport + Sync)) {}
    fn _check_parser(_x: &dyn DpsResponseParser) {}
    _check_builder(&EvpzXmlBuilder);
    _check_envelope(&CmsOverCheckSignedFileEnvelope);
    _check_transport(&HttpsRestTransport);
    _check_parser(&EvpzResponseParser);
}

// ─── EVPZ placeholders return typed Unimplemented ────────────────

#[test]
fn evpz_builder_returns_typed_unimplemented() {
    let result = EvpzXmlBuilder.build_check(&dummy_cmd(), &dummy_builder_context());
    assert!(matches!(
        result.unwrap_err(),
        outgress::BuildError::Unimplemented(_)
    ));
}

#[test]
fn evpz_envelope_returns_typed_unimplemented() {
    let result = CmsOverCheckSignedFileEnvelope.wrap(&[1, 2, 3], &SignContext::default());
    assert!(matches!(
        result.unwrap_err(),
        outgress::SignError::Unimplemented(_)
    ));
}

#[tokio::test]
async fn evpz_transport_returns_typed_unimplemented() {
    let result = HttpsRestTransport.submit(&[1, 2, 3], &dummy_target()).await;
    assert!(matches!(
        result.unwrap_err(),
        outgress::TransportError::Unimplemented(_)
    ));
}

#[test]
fn evpz_parser_returns_typed_unimplemented() {
    let result = EvpzResponseParser.parse_response(&[1, 2, 3], &dummy_cmd());
    assert!(matches!(
        result.unwrap_err(),
        outgress::ParseError::Unimplemented(_)
    ));
}

// ─── Router: EVPZ in pilot returns ProfileNotImplemented ─────────

#[tokio::test]
async fn dispatch_returns_profile_not_implemented_for_evpz_dps() {
    let err = outgress::dispatch_to_outgress(
        OutgressProfile::EvpzDps,
        &dummy_cmd(),
        &dummy_builder_context(),
        &dummy_target(),
        &SignContext::default(),
    )
    .await
    .expect_err("EVPZ_DPS must surface ProfileNotImplemented in pilot");

    match err {
        OutgressError::ProfileNotImplemented { profile } => {
            assert_eq!(profile, OutgressProfile::EvpzDps);
        }
        other => panic!("expected ProfileNotImplemented, got: {other:?}"),
    }
}

// ─── Quartet accessor: returns Arc<dyn Trait> per profile ────────

#[test]
fn quartet_for_fsco_returns_pilot_impls() {
    let (builder, envelope, transport, parser) =
        outgress::quartet_for(OutgressProfile::FscoZzd);
    // Pointer equality not possible across Arc<dyn>; we just check
    // they are constructable (compile + Arc::strong_count == 1).
    assert_eq!(std::sync::Arc::strong_count(&builder), 1);
    assert_eq!(std::sync::Arc::strong_count(&envelope), 1);
    assert_eq!(std::sync::Arc::strong_count(&transport), 1);
    assert_eq!(std::sync::Arc::strong_count(&parser), 1);
}

#[test]
fn quartet_for_evpz_returns_placeholders() {
    let (builder, _envelope, _transport, _parser) =
        outgress::quartet_for(OutgressProfile::EvpzDps);
    // Builder is the EVPZ placeholder; calling it returns Unimplemented.
    let result = builder.build_check(&dummy_cmd(), &dummy_builder_context());
    assert!(matches!(
        result.unwrap_err(),
        outgress::BuildError::Unimplemented(_)
    ));
}
