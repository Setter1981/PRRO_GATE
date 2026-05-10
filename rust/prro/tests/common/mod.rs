//! Shared test infrastructure for W10 integration test files.
//!
//! Cargo treats `tests/` integration tests as one crate per top-level
//! `.rs` file; subdirectories like `tests/common/` are NOT compiled
//! as test crates themselves but ARE accessible to the top-level
//! integration tests via `mod common;`.  Each integration test file
//! that uses this module pulls it in once with:
//!
//! ```ignore
//! mod common;
//! use common::*;
//! ```
//!
//! Why a shared module:
//!   - **Stub providers** (`StubDpsChannel`, `DetCrypto`) and
//!     `SigningContext` constructor are byte-for-byte identical across
//!     `write_path_stage4_send.rs`, `mac_recovery_orchestrator.rs`,
//!     `re_sign_after_mac_recovery.rs`, and the new W10.5
//!     `write_path_dps_error_routing.rs`.  Earlier copies drifted (each
//!     file accumulated minor variations); the shared definition is
//!     the single source of truth (R-W10.5-review MED 2 close).
//!   - **Seed helpers stay per-file** because each integration test
//!     file's setup shape is slightly different (lnd parameter
//!     conventions, additional payload/artifact fixtures); unifying
//!     would either narrow each test's intent or balloon the helper's
//!     parameter list.  Per-file seeds remain.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use prro::crypto::errors::CryptoError;
use prro::crypto::provider::{
    CertDer, CryptoProvider, DstuVerifyResult, SignCmsRequest, SignedCmsBytes,
};
use prro::crypto::session::SigningSession;
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{CheckAck, CheckEnvelope, CheckSignBlob, RroInfo, StatusSnapshot};
use prro::transports::dps::error::DpsError;

// ─── In-memory stub DpsChannel ───────────────────────────────────────

/// Lightweight queue-based stub: scripted response queue + call
/// counter + optional spy callback fired BEFORE the response is
/// returned.
///
/// Single-response constructors (`new` / `with_spy`) push exactly one
/// element into the queue for backwards compat with the W7.5
/// fixtures; `with_queue` is for multi-attempt scenarios (MAC
/// recovery's two-attempt sequence).
pub struct StubDpsChannel {
    responses: Mutex<VecDeque<Result<CheckAck, DpsError>>>,
    send_chk_calls: AtomicUsize,
    on_send_chk: Option<Box<dyn Fn() + Send + Sync>>,
}

impl StubDpsChannel {
    pub fn new(response: Result<CheckAck, DpsError>) -> Self {
        let mut q = VecDeque::with_capacity(1);
        q.push_back(response);
        Self {
            responses: Mutex::new(q),
            send_chk_calls: AtomicUsize::new(0),
            on_send_chk: None,
        }
    }

    pub fn with_queue(responses: Vec<Result<CheckAck, DpsError>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            send_chk_calls: AtomicUsize::new(0),
            on_send_chk: None,
        }
    }

    pub fn with_spy(
        response: Result<CheckAck, DpsError>,
        spy: Box<dyn Fn() + Send + Sync>,
    ) -> Self {
        let mut q = VecDeque::with_capacity(1);
        q.push_back(response);
        Self {
            responses: Mutex::new(q),
            send_chk_calls: AtomicUsize::new(0),
            on_send_chk: Some(spy),
        }
    }

    pub fn call_count(&self) -> usize {
        self.send_chk_calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl DpsChannel for StubDpsChannel {
    async fn send_chk(&self, _envelope: CheckEnvelope) -> Result<CheckAck, DpsError> {
        self.send_chk_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(spy) = &self.on_send_chk {
            spy();
        }
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("StubDpsChannel response queue empty (caller forgot to enqueue)")
    }

    async fn last_chk(&self, _: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        unreachable!("stub: last_chk not exercised");
    }

    async fn ping(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        unreachable!("stub: ping not exercised");
    }

    async fn status_rro(&self, _: &CheckSignBlob) -> Result<StatusSnapshot, DpsError> {
        unreachable!("stub: status_rro not exercised");
    }

    async fn info_rro(&self, _: &CheckSignBlob) -> Result<RroInfo, DpsError> {
        unreachable!("stub: info_rro not exercised");
    }
}

// ─── Deterministic crypto stub for MAC recovery fixtures ─────────────

/// Returns a fixed CMS byte string `RECOVERED-CMS` on every
/// `sign_cms_detached` call.  Used by MAC recovery integration
/// fixtures that need a callable `SigningContext` but don't care
/// about the actual signature contents.
pub struct DetCrypto;

#[async_trait]
impl CryptoProvider for DetCrypto {
    async fn sign_cms_detached(
        &self,
        _: SignCmsRequest<'_>,
    ) -> Result<SignedCmsBytes, CryptoError> {
        Ok(SignedCmsBytes(b"RECOVERED-CMS".to_vec()))
    }
    async fn verify_dstu(
        &self,
        _: &[u8],
        _: &[u8],
        _: &[u8],
    ) -> Result<DstuVerifyResult, CryptoError> {
        unimplemented!("not exercised");
    }
    async fn unwrap_envelope(
        &self,
        _: &[u8],
        _: &[u8],
        _: &SigningSession,
    ) -> Result<Vec<u8>, CryptoError> {
        unimplemented!("not exercised");
    }
    async fn fetch_cert_by_ski(
        &self,
        _: &[String],
        _: &[u8; 32],
        _: std::time::Duration,
    ) -> Result<CertDer, CryptoError> {
        unimplemented!("not exercised");
    }
}

/// Build a `SigningContext` over `DetCrypto` for MAC recovery
/// fixtures.  Test session uses operator id "operator-1" + zero
/// public-key digest; production never instantiates this path.
pub fn det_signing_ctx() -> SigningContext {
    SigningContext {
        provider: Arc::new(DetCrypto) as Arc<dyn CryptoProvider>,
        session: SigningSession::new_for_test("operator-1".into(), [0u8; 32], vec![]),
        profile: prro_crypto::cms::profile::CmsProfile::Dstu4145WithGost34311Pb,
    }
}

/// Minimal `CheckAck` builder used by happy-path fixtures.
pub fn ack(id: &str) -> CheckAck {
    CheckAck {
        id: id.into(),
        id_sign: vec![],
        data_sign: vec![],
    }
}
