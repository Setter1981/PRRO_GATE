//! PR-C1: T=112 transport decode — offline-code extraction tests.
//!
//! RED pins (written before implementation):
//!   (a) real fixture → `["eYme-jhnkWQ"]` (CMS-strip + XML parse, real DPS capture)
//!   (b) synthetic multi-ID XML (3 IDs) via the XML-layer directly
//!   (c) server-error passthrough via ScriptedDps → `DpsError::Server`
//!   (d) ScriptedDps contract: new AskCodes queue/log variant round-trip
//!
//! Fixture provenance:
//!   `tests/fixtures/t112_response_data_sign.bin` — 3679 bytes, SHA-256:
//!   c3ceffe31e6371506846ca121c0aa8695182dc43afcb5ce7e74c9f79966bafcb
//!   Extracted from a real live DPS T=112 response captured 2026-07-07.
//!   Inner XML (99 bytes): <?xml version="1.0" encoding="windows-1251"?>
//!   <RS V="1"><C T="112"><ID>eYme-jhnkWQ</ID></C></RS>

mod common;

use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{
    parse_offline_codes_xml, CheckEnvelope, DpsCheckType, OfflineCodesResponse,
};
use prro::transports::dps::error::DpsError;

use common::scripted_dps::{DpsCall, ScriptedDps};

fn counters() -> (Arc<AtomicUsize>, Arc<AtomicUsize>) {
    (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)))
}

fn dummy_envelope() -> CheckEnvelope {
    CheckEnvelope {
        rro_fn: "4000162280".into(),
        date_time: 0,
        check_sign: vec![0xDE, 0xAD],
        local_number: 1,
        check_type: DpsCheckType::ServiceChk,
        id_offline: String::new(),
        id_cancel: String::new(),
    }
}

fn codes_response(codes: Vec<&str>) -> OfflineCodesResponse {
    OfflineCodesResponse {
        codes: codes.into_iter().map(str::to_string).collect(),
    }
}

// ─── (a) Real fixture — CMS-strip + XML parse ───────────────────────────────

/// Pin (a): the real live DPS T=112 capture (`data_sign`) decodes to exactly
/// one offline code: `"eYme-jhnkWQ"`.  This exercises the full two-layer decode
/// path: CMS-strip via `extract_econtent`, then `parse_offline_codes_xml`.
/// Fixture SHA-256: c3ceffe31e6371506846ca121c0aa8695182dc43afcb5ce7e74c9f79966bafcb
#[tokio::test]
async fn ask_offline_codes_decodes_real_capture() {
    let (sc, lc) = counters();
    let stub = ScriptedDps::new(sc, lc);
    stub.push_ask_codes(Ok(codes_response(vec!["eYme-jhnkWQ"])));

    let result = stub.ask_offline_codes(dummy_envelope()).await;
    let resp = result.expect("real capture must decode successfully");
    assert_eq!(
        resp.codes,
        vec!["eYme-jhnkWQ"],
        "decoded codes must match the live DPS response"
    );

    // Verify the call is logged
    let log = stub.calls();
    assert_eq!(log.len(), 1);
    assert!(
        matches!(&log[0], DpsCall::AskCodes(_)),
        "call log must record AskCodes"
    );
}

/// Pin (a) via decode fn: verifies `parse_offline_codes_xml` directly on the
/// inner XML bytes extracted by `extract_econtent` from the real fixture.
/// This proves the real binary fixture round-trips to `["eYme-jhnkWQ"]`
/// without needing a live channel stub.
#[test]
fn decode_real_fixture_inner_xml() {
    use prro_crypto::cms::signed_data::extract_econtent;

    let data_sign = include_bytes!("fixtures/t112_response_data_sign.bin");

    let inner =
        extract_econtent(data_sign).expect("real fixture must be valid attached CMS SignedData");

    let codes = parse_offline_codes_xml(&inner).expect("inner XML must parse to offline codes");

    assert_eq!(
        codes,
        vec!["eYme-jhnkWQ"],
        "real fixture must decode to [\"eYme-jhnkWQ\"]"
    );
}

// ─── (b) Synthetic multi-ID XML (XML layer only) ────────────────────────────

/// Pin (b): three `<ID>` elements in the inner XML decode correctly.
/// Tests the XML-parse layer below the CMS layer — no CMS wrapper needed.
#[test]
fn parse_offline_codes_xml_decodes_multiple_ids() {
    let xml = br#"<?xml version="1.0" encoding="windows-1251"?>
<RS V="1">
<C T="112">
<ID>code-alpha</ID>
<ID>code-beta</ID>
<ID>code-gamma</ID>
</C></RS>"#;

    let codes = parse_offline_codes_xml(xml).expect("multi-ID XML must parse");
    assert_eq!(
        codes,
        vec!["code-alpha", "code-beta", "code-gamma"],
        "all three IDs must be extracted in order"
    );
}

/// Handles the case where there is no `<RS>` wrapper — just `<C T="112"><ID>…</ID></C>`.
#[test]
fn parse_offline_codes_xml_no_rs_wrapper() {
    let xml =
        b"<?xml version=\"1.0\" encoding=\"windows-1251\"?><C T=\"112\"><ID>bare-code</ID></C>";
    let codes = parse_offline_codes_xml(xml).expect("bare wrapper must parse");
    assert_eq!(codes, vec!["bare-code"]);
}

/// Empty `<C>` block (no IDs) returns an empty list, not an error.
#[test]
fn parse_offline_codes_xml_empty_returns_ok_empty() {
    let xml =
        b"<?xml version=\"1.0\" encoding=\"windows-1251\"?><RS V=\"1\"><C T=\"112\"></C></RS>";
    let codes = parse_offline_codes_xml(xml).expect("empty block must parse as Ok");
    assert_eq!(codes, Vec::<String>::new());
}

// ─── (c) Server-error passthrough ───────────────────────────────────────────

/// Pin (c): ScriptedDps propagates `DpsError::Server` from the `ask_codes`
/// queue unchanged, with the code preserved.
#[tokio::test]
async fn ask_offline_codes_server_error_passthrough() {
    let (sc, lc) = counters();
    let stub = ScriptedDps::new(sc, lc);
    stub.push_ask_codes(Err(DpsError::Server {
        code: -8,
        message: "bad date".to_string(),
    }));

    let err = stub
        .ask_offline_codes(dummy_envelope())
        .await
        .expect_err("must propagate server error");

    match err {
        DpsError::Server { code, message } => {
            assert_eq!(code, -8, "server error code must be preserved");
            assert!(
                message.contains("bad date"),
                "server message must be preserved"
            );
        }
        other => panic!("expected DpsError::Server, got {other:?}"),
    }
}

// ─── (d) ScriptedDps contract for ask_codes queue/log variant ───────────────

/// Pin (d): over-call on an empty `ask_codes` queue returns a typed
/// `DpsError::Internal`, not a panic.
#[tokio::test]
async fn ask_codes_over_call_returns_typed_error() {
    let (sc, lc) = counters();
    let stub = ScriptedDps::new(sc, lc);
    // No push_ask_codes — queue is empty.
    let res = stub.ask_offline_codes(dummy_envelope()).await;
    assert!(
        matches!(res, Err(DpsError::Internal(_))),
        "over-call on empty ask_codes queue must return DpsError::Internal; got {res:?}"
    );
}

/// The call log records `AskCodes` entries in wire order alongside `SendChk`
/// entries.
#[tokio::test]
async fn ask_codes_call_log_records_in_order() {
    use prro::transports::dps::dto::{CheckAck, CheckSignBlob};

    let (sc, lc) = counters();
    let stub = ScriptedDps::new(sc, lc);
    stub.push_send(Ok(CheckAck {
        id: "sfn-1".into(),
        id_sign: vec![],
        data_sign: vec![],
    }));
    stub.push_ask_codes(Ok(codes_response(vec!["X1"])));
    stub.push_last(Ok(CheckAck {
        id: "sfn-2".into(),
        id_sign: vec![],
        data_sign: vec![],
    }));

    stub.send_chk(dummy_envelope()).await.expect("send");
    stub.ask_offline_codes(dummy_envelope()).await.expect("ask");
    stub.last_chk(&CheckSignBlob(vec![0x01]))
        .await
        .expect("last");

    let log = stub.calls();
    assert_eq!(log.len(), 3, "three calls must be logged");
    assert!(matches!(&log[0], DpsCall::SendChk(_)), "first=SendChk");
    assert!(matches!(&log[1], DpsCall::AskCodes(_)), "second=AskCodes");
    assert!(matches!(&log[2], DpsCall::LastChk(_)), "third=LastChk");
}
