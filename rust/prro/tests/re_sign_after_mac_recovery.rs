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
use prro::services::write_path::stage_sign::{self, SigningContext, WireArtifactKind};

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

    // Hash hex MUST be present in the canonical XML — visible via a
    // simple substring search.  The XML representation uses uppercase
    // hex (per `hex_encode` helper), so we pin that.
    let hash_a_hex = "1111111111111111111111111111111111111111111111111111111111111111";
    let hash_b_hex = "2222222222222222222222222222222222222222222222222222222222222222";
    let a_str = String::from_utf8_lossy(&a.unsigned_xml).to_uppercase();
    let b_str = String::from_utf8_lossy(&b.unsigned_xml).to_uppercase();
    assert!(
        a_str.contains(&hash_a_hex.to_uppercase()),
        "a.unsigned_xml must contain hash_a hex"
    );
    assert!(
        b_str.contains(&hash_b_hex.to_uppercase()),
        "b.unsigned_xml must contain hash_b hex"
    );
    assert!(
        !a_str.contains(&hash_b_hex.to_uppercase()),
        "a.unsigned_xml must NOT contain hash_b hex (cross-contamination check)"
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
