//! Targeted verification for `stage_sign::re_sign_after_mac_recovery`
//! (W10.4 step 2b).
//!
//! Pure no-tx canonical-XML rebuild + CMS sign for the MAC recovery
//! path.  Tests pin:
//!   1. Determinism — same inputs ⇒ byte-identical `unsigned_xml`,
//!      `unsigned_xml_sha256`, and `signed_xml_cms` (when the provider
//!      itself is deterministic).
//!   2. `new_previous_hash` propagation — a different `new_previous_hash`
//!      MUST produce a different `unsigned_xml` (the recovered hash
//!      lands in the `<DOC><HEAD><...PREV_DOC_HASH></HEAD>...>` region,
//!      observable in canonical XML bytes).  Hash propagation IS the
//!      whole point of the recovery flow — without it, attempt #2
//!      sends the same envelope as attempt #1.
//!   3. Crypto provider receives the rebuilt `unsigned_xml` (not the
//!      attempt-#1 bytes).  Spy captures the `canonical_xml` argument.
//!
//! Anchored on freeze §4.4.4 step 2b.

use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

use prro::crypto::errors::CryptoError;
use prro::crypto::provider::{
    CertDer, CryptoProvider, DstuVerifyResult, SignCmsRequest, SignedCmsBytes,
};
use prro::crypto::session::SigningSession;
use prro::services::write_path::stage_sign::{self, SignError, SigningContext, WireArtifactKind};

// ─── Minimal spy crypto provider (test-local) ────────────────────────

struct StubCrypto {
    captured_xml: Mutex<Vec<Vec<u8>>>,
    response_queue: Mutex<VecDeque<Vec<u8>>>,
    call_count: AtomicUsize,
}

impl StubCrypto {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            captured_xml: Mutex::new(Vec::new()),
            response_queue: Mutex::new(VecDeque::new()),
            call_count: AtomicUsize::new(0),
        })
    }

    fn enqueue(&self, bytes: Vec<u8>) {
        self.response_queue.lock().unwrap().push_back(bytes);
    }

    fn captured(&self) -> Vec<Vec<u8>> {
        self.captured_xml.lock().unwrap().clone()
    }

    fn call_count(&self) -> usize {
        self.call_count.load(std::sync::atomic::Ordering::Acquire)
    }
}

#[async_trait]
impl CryptoProvider for StubCrypto {
    async fn sign_cms_detached(
        &self,
        request: SignCmsRequest<'_>,
    ) -> Result<SignedCmsBytes, CryptoError> {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        self.captured_xml
            .lock()
            .unwrap()
            .push(request.canonical_xml.to_vec());
        let resp = self
            .response_queue
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| b"DEFAULT-CMS".to_vec());
        Ok(SignedCmsBytes(resp))
    }

    async fn verify_dstu(
        &self,
        _content_digest: &[u8],
        _sig_bytes: &[u8],
        _pubkey_compressed: &[u8],
    ) -> Result<DstuVerifyResult, CryptoError> {
        unimplemented!("not exercised");
    }

    async fn unwrap_envelope(
        &self,
        _envelope_der: &[u8],
        _originator_cert_der: &[u8],
        _session: &SigningSession,
    ) -> Result<Vec<u8>, CryptoError> {
        unimplemented!("not exercised");
    }

    async fn fetch_cert_by_ski(
        &self,
        _urls: &[String],
        _ski: &[u8; 32],
        _request_timeout: std::time::Duration,
    ) -> Result<CertDer, CryptoError> {
        unimplemented!("not exercised");
    }
}

fn ctx(stub: Arc<StubCrypto>) -> SigningContext {
    SigningContext {
        provider: stub as Arc<dyn CryptoProvider>,
        session: SigningSession::new_for_test("operator-1".into(), [0u8; 32], vec![]),
        profile: prro_crypto::cms::profile::CmsProfile::Dstu4145WithGost34311Pb,
    }
}

// ─── Sample inputs ───────────────────────────────────────────────────

const SELL_PAYLOAD_JSON: &str = r#"{
    "items": [
        {"code": "p-1", "name": "Item A", "price_kop": 1500, "quantity_thousandths": 1000, "sum_kop": 1500}
    ],
    "payments": [
        {"name": "Cash", "sum_kop": 1500, "type_code": "0"}
    ]
}"#;

const Z_REPORT_PAYLOAD_JSON: &str = r#"{"payments":[{"name":"CASH","sum_in_kop":15000,"sum_out_kop":0,"type_code":"0"}],"sell_count":1,"return_count":0}"#;

// ─── Fixtures ────────────────────────────────────────────────────────

#[tokio::test]
async fn re_sign_deterministic_for_identical_inputs() {
    let stub = StubCrypto::new();
    stub.enqueue(b"DETERMINISTIC-CMS".to_vec());
    stub.enqueue(b"DETERMINISTIC-CMS".to_vec());
    let ctx = ctx(Arc::clone(&stub));

    let prev_hash = [0xAAu8; 32];
    let a = stage_sign::re_sign_after_mac_recovery(
        &ctx,
        WireArtifactKind::Sell,
        "1234567890",
        "12345678",
        "2026-05-09T12:34:56Z",
        SELL_PAYLOAD_JSON,
        Some(1500),
        42,
        None,
        prev_hash,
        None,
    )
    .await
    .expect("first re-sign");

    let b = stage_sign::re_sign_after_mac_recovery(
        &ctx,
        WireArtifactKind::Sell,
        "1234567890",
        "12345678",
        "2026-05-09T12:34:56Z",
        SELL_PAYLOAD_JSON,
        Some(1500),
        42,
        None,
        prev_hash,
        None,
    )
    .await
    .expect("second re-sign");

    assert_eq!(
        a.unsigned_xml, b.unsigned_xml,
        "identical inputs MUST yield byte-identical unsigned_xml"
    );
    assert_eq!(
        a.unsigned_xml_sha256, b.unsigned_xml_sha256,
        "identical unsigned_xml MUST yield identical sha256"
    );
    assert_eq!(
        a.signed_xml_cms.0, b.signed_xml_cms.0,
        "identical inputs MUST yield identical CMS (provider stub returns same bytes)"
    );
    assert_eq!(stub.call_count(), 2, "provider invoked exactly twice");

    // R-W10.4-step2b-review NIT 2 close: pin lnd propagation into
    // the canonical XML.  Determinism alone doesn't catch a regression
    // where lnd is dropped before reaching the wire.  SELL/RETURN
    // canonical XML emits local_number as the `DI="..."` attribute
    // (per src/xml/mod.rs::tests).
    let xml_str = String::from_utf8_lossy(&a.unsigned_xml);
    assert!(
        xml_str.contains(r#"DI="42""#),
        "lnd 42 must propagate into DI=\"...\" attribute: {xml_str}"
    );
}

#[tokio::test]
async fn re_sign_propagates_new_previous_hash_into_canonical_xml() {
    // The whole point of MAC recovery: the new previous_hash MUST
    // appear in the canonical-XML bytes the wire receives.  Different
    // hash → different unsigned_xml → different sha → different CMS.
    let stub = StubCrypto::new();
    stub.enqueue(b"CMS-A".to_vec());
    stub.enqueue(b"CMS-B".to_vec());
    let ctx = ctx(Arc::clone(&stub));

    let hash_a = [0x11u8; 32];
    let hash_b = [0x22u8; 32];

    let a = stage_sign::re_sign_after_mac_recovery(
        &ctx,
        WireArtifactKind::Sell,
        "1234567890",
        "12345678",
        "2026-05-09T12:34:56Z",
        SELL_PAYLOAD_JSON,
        Some(1500),
        42,
        None,
        hash_a,
        None,
    )
    .await
    .expect("re-sign with hash_a");

    let b = stage_sign::re_sign_after_mac_recovery(
        &ctx,
        WireArtifactKind::Sell,
        "1234567890",
        "12345678",
        "2026-05-09T12:34:56Z",
        SELL_PAYLOAD_JSON,
        Some(1500),
        42,
        None,
        hash_b,
        None,
    )
    .await
    .expect("re-sign with hash_b");

    assert_ne!(
        a.unsigned_xml, b.unsigned_xml,
        "different new_previous_hash MUST produce different unsigned_xml"
    );
    assert_ne!(
        a.unsigned_xml_sha256, b.unsigned_xml_sha256,
        "different unsigned_xml MUST yield different sha256"
    );

    // Provider received the new bytes — pin via captured arg.
    let captured = stub.captured();
    assert_eq!(captured.len(), 2);
    assert_eq!(
        captured[0], a.unsigned_xml,
        "provider call #1 must receive a.unsigned_xml"
    );
    assert_eq!(
        captured[1], b.unsigned_xml,
        "provider call #2 must receive b.unsigned_xml"
    );

    // R-W10.4-step2b-review NIT 1 close: pin EXACT hex case
    // (lowercase per `stage_sign::hex_encode` `format!("{b:02x}", ...)`).
    // `to_uppercase()` symmetric on both sides loses the case-pin —
    // a regression to UPPERCASE hex would have gone unnoticed.
    let hash_a_hex_lower = "1111111111111111111111111111111111111111111111111111111111111111";
    let hash_b_hex_lower = "2222222222222222222222222222222222222222222222222222222222222222";
    let a_str = String::from_utf8_lossy(&a.unsigned_xml);
    let b_str = String::from_utf8_lossy(&b.unsigned_xml);
    assert!(
        a_str.contains(hash_a_hex_lower),
        "a.unsigned_xml must contain hash_a in LOWERCASE hex (per hex_encode contract): {a_str}"
    );
    assert!(
        b_str.contains(hash_b_hex_lower),
        "b.unsigned_xml must contain hash_b in LOWERCASE hex: {b_str}"
    );
    assert!(
        !a_str.contains(hash_b_hex_lower),
        "a.unsigned_xml must NOT contain hash_b hex (cross-contamination check): {a_str}"
    );
}

#[tokio::test]
async fn re_sign_provider_receives_rebuilt_xml_not_attempt_one_bytes() {
    // Forensic pin: the spy's captured_xml is exactly the bytes
    // the provider signs.  If a future refactor accidentally signs
    // the wrong buffer (e.g. an attempt-#1 payload from a closure
    // capture), this fixture catches it.
    let stub = StubCrypto::new();
    stub.enqueue(b"CMS-OUTPUT".to_vec());
    let ctx = ctx(Arc::clone(&stub));

    let out = stage_sign::re_sign_after_mac_recovery(
        &ctx,
        WireArtifactKind::Sell,
        "1234567890",
        "12345678",
        "2026-05-09T12:34:56Z",
        SELL_PAYLOAD_JSON,
        Some(1500),
        42,
        None,
        [0x33u8; 32],
        None,
    )
    .await
    .expect("re-sign");

    let captured = stub.captured();
    assert_eq!(captured.len(), 1);
    assert_eq!(
        captured[0], out.unsigned_xml,
        "provider must sign the exact bytes returned as unsigned_xml"
    );
    assert_eq!(out.signed_xml_cms.0, b"CMS-OUTPUT");
}

// ─── R-W10.4-step2b-review LOW 1 close: ZReport recovery path ─────────

#[tokio::test]
async fn re_sign_z_report_propagates_z_number_into_canonical_xml() {
    // ZReport recovery uses an already-allocated Z number from
    // attempt #1 — caller passes it through.  Pin that the Z number
    // lands in the canonical XML so the wire envelope stays valid.
    let stub = StubCrypto::new();
    stub.enqueue(b"CMS-Z".to_vec());
    let ctx = ctx(Arc::clone(&stub));

    let out = stage_sign::re_sign_after_mac_recovery(
        &ctx,
        WireArtifactKind::ZReport,
        "1234567890",
        "12345678",
        "2026-05-09T12:34:56Z",
        Z_REPORT_PAYLOAD_JSON,
        None, // ZReport doesn't carry total_sum_kop
        100,
        Some(42),
        [0u8; 32],
        None,
    )
    .await
    .expect("ZReport re-sign");

    let xml_str = String::from_utf8_lossy(&out.unsigned_xml);
    // Z_REPORT canonical XML emits Z number via the `<Z_NO>...</Z_NO>`
    // body slot (or `ZN="..."` attribute, depending on doc shape).
    // Assert the value appears as a number in the rendered bytes.
    assert!(
        xml_str.contains(">42<") || xml_str.contains(r#"ZN="42""#),
        "Z number 42 must propagate into the canonical XML (Z_NO body or ZN attribute): {xml_str}"
    );
}

// ─── R-W10.4-step2b-review MED 2 close: error path fixtures ───────────

#[tokio::test]
async fn re_sign_payload_schema_mismatch_returns_typed_error() {
    let stub = StubCrypto::new();
    let ctx = ctx(Arc::clone(&stub));
    let res = stage_sign::re_sign_after_mac_recovery(
        &ctx,
        WireArtifactKind::Sell,
        "1234567890",
        "12345678",
        "2026-05-09T12:34:56Z",
        "not-valid-json",
        Some(1500),
        42,
        None,
        [0u8; 32],
        None,
    )
    .await;
    match res {
        Err(SignError::PayloadSchema { detail }) => {
            assert!(
                detail.contains("Check"),
                "PayloadSchema detail must mention the kind: {detail}"
            );
        }
        other => panic!("expected SignError::PayloadSchema, got {other:?}"),
    }
    // Provider was never invoked — early-return on parse failure.
    assert_eq!(stub.call_count(), 0);
}

#[tokio::test]
async fn re_sign_invalid_business_ts_returns_typed_error() {
    let stub = StubCrypto::new();
    let ctx = ctx(Arc::clone(&stub));
    let res = stage_sign::re_sign_after_mac_recovery(
        &ctx,
        WireArtifactKind::Sell,
        "1234567890",
        "12345678",
        "not-a-timestamp",
        SELL_PAYLOAD_JSON,
        Some(1500),
        42,
        None,
        [0u8; 32],
        None,
    )
    .await;
    match res {
        Err(SignError::TimestampConversion { detail }) => {
            assert!(
                detail.contains("not-a-timestamp"),
                "TimestampConversion detail must echo the offending input: {detail}"
            );
        }
        other => panic!("expected SignError::TimestampConversion, got {other:?}"),
    }
    assert_eq!(stub.call_count(), 0);
}

#[tokio::test]
async fn re_sign_lnd_overflow_returns_range_error() {
    let stub = StubCrypto::new();
    let ctx = ctx(Arc::clone(&stub));
    let res = stage_sign::re_sign_after_mac_recovery(
        &ctx,
        WireArtifactKind::Sell,
        "1234567890",
        "12345678",
        "2026-05-09T12:34:56Z",
        SELL_PAYLOAD_JSON,
        Some(1500),
        (u32::MAX as i64) + 1, // overflow
        None,
        [0u8; 32],
        None,
    )
    .await;
    match res {
        Err(SignError::Range { field, value }) => {
            assert_eq!(field, "lnd");
            assert_eq!(value, (u32::MAX as i64) + 1);
        }
        other => panic!("expected SignError::Range {{ field: lnd, .. }}, got {other:?}"),
    }
    assert_eq!(stub.call_count(), 0);
}

#[tokio::test]
async fn re_sign_z_number_overflow_returns_range_error() {
    let stub = StubCrypto::new();
    let ctx = ctx(Arc::clone(&stub));
    let res = stage_sign::re_sign_after_mac_recovery(
        &ctx,
        WireArtifactKind::ZReport,
        "1234567890",
        "12345678",
        "2026-05-09T12:34:56Z",
        Z_REPORT_PAYLOAD_JSON,
        None,
        100,
        Some((u32::MAX as i64) + 1), // overflow
        [0u8; 32],
        None,
    )
    .await;
    match res {
        Err(SignError::Range { field, value }) => {
            assert_eq!(field, "z_report_number");
            assert_eq!(value, (u32::MAX as i64) + 1);
        }
        other => {
            panic!("expected SignError::Range {{ field: z_report_number, .. }}, got {other:?}")
        }
    }
    assert_eq!(stub.call_count(), 0);
}

#[tokio::test]
async fn re_sign_crypto_error_propagates_typed() {
    use prro::crypto::errors::{CryptoError, SignKind};
    // Spy that returns a hard-coded crypto error on the first call.
    struct ErrCrypto;
    #[async_trait]
    impl CryptoProvider for ErrCrypto {
        async fn sign_cms_detached(
            &self,
            _: SignCmsRequest<'_>,
        ) -> Result<SignedCmsBytes, CryptoError> {
            Err(CryptoError::CmsSign {
                reason: SignKind::BackendError,
            })
        }
        async fn verify_dstu(
            &self,
            _: &[u8],
            _: &[u8],
            _: &[u8],
        ) -> Result<DstuVerifyResult, CryptoError> {
            unimplemented!()
        }
        async fn unwrap_envelope(
            &self,
            _: &[u8],
            _: &[u8],
            _: &SigningSession,
        ) -> Result<Vec<u8>, CryptoError> {
            unimplemented!()
        }
        async fn fetch_cert_by_ski(
            &self,
            _: &[String],
            _: &[u8; 32],
            _: std::time::Duration,
        ) -> Result<CertDer, CryptoError> {
            unimplemented!()
        }
    }
    let ctx = SigningContext {
        provider: Arc::new(ErrCrypto) as Arc<dyn CryptoProvider>,
        session: SigningSession::new_for_test("operator-1".into(), [0u8; 32], vec![]),
        profile: prro_crypto::cms::profile::CmsProfile::Dstu4145WithGost34311Pb,
    };

    let res = stage_sign::re_sign_after_mac_recovery(
        &ctx,
        WireArtifactKind::Sell,
        "1234567890",
        "12345678",
        "2026-05-09T12:34:56Z",
        SELL_PAYLOAD_JSON,
        Some(1500),
        42,
        None,
        [0u8; 32],
        None,
    )
    .await;
    match res {
        Err(SignError::Crypto(_)) => { /* expected */ }
        other => panic!("expected SignError::Crypto, got {other:?}"),
    }
}
