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
//!     `SigningContext` constructor are byte-for-byte identical
//!     across `write_path_stage4_send.rs` and the new W10.5
//!     `write_path_dps_error_routing.rs`.  Earlier copies drifted
//!     (each file accumulated minor variations); the shared
//!     definition is the single source of truth (R-W10.5-review
//!     MED 2 close).
//!   - **Seed helpers stay per-file** because each integration test
//!     file's setup shape is slightly different (lnd parameter
//!     conventions, additional payload/artifact fixtures); unifying
//!     would either narrow each test's intent or balloon the
//!     helper's parameter list.  Per-file seeds remain.
//!
//! Why partial adoption (`mac_recovery_orchestrator.rs` and
//! `re_sign_after_mac_recovery.rs` keep local stubs):
//!   - `re_sign_after_mac_recovery.rs` exercises crypto-error paths
//!     that need a provider variant returning
//!     `CryptoError::CmsSign { reason: SignKind::BackendError }`;
//!     a single constant-return `DetCrypto` cannot model both happy
//!     and error paths.
//!   - `mac_recovery_orchestrator.rs` uses a queue-based crypto stub
//!     so attempt #1 vs attempt #2 CMS bytes can be byte-distinguished
//!     in assertions (needed for re-sign-write atomicity proofs).
//!
//! If a future fixture needs either of those flavours from shared
//! infra, replace `DetCrypto` here with a `QueueCrypto` (analogous to
//! `StubDpsChannel::with_queue`) and a `FailingCrypto` variant.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sqlx::SqlitePool;

use prro::crypto::errors::CryptoError;
use prro::crypto::provider::{
    CertDer, CryptoProvider, DstuVerifyResult, SignCmsRequest, SignedCmsBytes,
};
use prro::crypto::session::SigningSession;
use prro::db::models::ids::DocumentId;
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
///
/// **Concurrency contract.**  Uses `std::sync::Mutex` (not
/// `tokio::sync::Mutex`) because tokio tests in this crate drive
/// `send_chk` from a single task per fixture, so locks are
/// short-held and never cross an `.await` while held.  If a future
/// fixture exercises parallel `send_chk` from multiple tasks, switch
/// to `tokio::sync::Mutex` to avoid blocking the runtime under
/// contention.
pub struct StubDpsChannel {
    responses: Mutex<VecDeque<Result<CheckAck, DpsError>>>,
    send_chk_calls: AtomicUsize,
    on_send_chk: Option<Box<dyn Fn() + Send + Sync>>,
    /// M3b W12 Commit 4 (2026-05-22) — `lastChk` response queue.
    /// `evaluate_lastchk` (via `by_server_fiscal_no` default impl)
    /// calls `self.last_chk(fn_sign)` to fetch DPS evidence.  Pre-W12
    /// drain code did not exercise `last_chk`; Commit 4 wires the
    /// Sent-source path (OFFLINE_LOCAL_ACK / ER → stage_send Sent →
    /// confirm_drain_doc → lastChk).  Tests that exercise the
    /// happy-path Acked chain must enqueue at least one response here.
    last_chk_responses: Mutex<VecDeque<Result<CheckAck, DpsError>>>,
    last_chk_calls: AtomicUsize,
}

impl StubDpsChannel {
    pub fn new(response: Result<CheckAck, DpsError>) -> Self {
        let mut q = VecDeque::with_capacity(1);
        q.push_back(response);
        Self {
            responses: Mutex::new(q),
            send_chk_calls: AtomicUsize::new(0),
            on_send_chk: None,
            last_chk_responses: Mutex::new(VecDeque::new()),
            last_chk_calls: AtomicUsize::new(0),
        }
    }

    pub fn with_queue(responses: Vec<Result<CheckAck, DpsError>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            send_chk_calls: AtomicUsize::new(0),
            on_send_chk: None,
            last_chk_responses: Mutex::new(VecDeque::new()),
            last_chk_calls: AtomicUsize::new(0),
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
            last_chk_responses: Mutex::new(VecDeque::new()),
            last_chk_calls: AtomicUsize::new(0),
        }
    }

    pub fn call_count(&self) -> usize {
        self.send_chk_calls.load(Ordering::SeqCst)
    }

    /// M3b W12 Commit 4 — enqueue `last_chk` responses.  Builder-style
    /// for fluent test setup.  Pre-W12 tests can ignore (lastChk path
    /// is unreachable when stage_send returns non-Sent outcomes).
    pub fn with_last_chk_queue(self, responses: Vec<Result<CheckAck, DpsError>>) -> Self {
        *self.last_chk_responses.lock().unwrap() = responses.into();
        self
    }

    pub fn last_chk_call_count(&self) -> usize {
        self.last_chk_calls.load(Ordering::SeqCst)
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
        self.last_chk_calls.fetch_add(1, Ordering::SeqCst);
        self.last_chk_responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| {
                panic!(
                    "stub: last_chk called but no response enqueued — \
                     M3b W12 Commit 4 wired the Sent-source path through \
                     lastChk; tests exercising stage_send Sent must \
                     supply at least one last_chk response via \
                     StubDpsChannel::with_last_chk_queue(...)"
                )
            })
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
///
/// **Limitation: constant return.**  Suitable only for fixtures that
/// don't need to byte-distinguish attempt #1 vs attempt #2 CMS bytes
/// (e.g., for re-sign-write atomicity proofs).  If such
/// distinguishability is required, clone the
/// `StubDpsChannel::with_queue` pattern into a `QueueCrypto` variant
/// rather than parametrising `DetCrypto`.
///
/// Non-signing methods (`verify_dstu`, `unwrap_envelope`,
/// `fetch_cert_by_ski`) use `unreachable!()` because the integration
/// fixtures here exercise only the signing path; if those panic at
/// runtime, the fixture has wired up the wrong code path.
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
        unreachable!("stub: verify_dstu not exercised");
    }
    async fn unwrap_envelope(
        &self,
        _: &[u8],
        _: &[u8],
        _: &SigningSession,
    ) -> Result<Vec<u8>, CryptoError> {
        unreachable!("stub: unwrap_envelope not exercised");
    }
    async fn fetch_cert_by_ski(
        &self,
        _: &[String],
        _: &[u8; 32],
        _: std::time::Duration,
    ) -> Result<CertDer, CryptoError> {
        unreachable!("stub: fetch_cert_by_ski not exercised");
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

/// W9b NIT-C7-R1 helper (2026-05-21): construct the W2 lock-token
/// for integration tests that call `backlog_drain::drain` directly
/// (bypassing the App entry).  Uses the test-only constructor
/// (gated behind `test-support` feature) so production builds
/// physically cannot mint a token without the App mutex.
pub fn drain_test_guard() -> prro::services::reconciliation::ReconcileGuard<'static> {
    prro::services::reconciliation::ReconcileGuard::for_integration_test_only()
}

// ─── M3b W12 Commit 4b.2 — test infrastructure prep (Variant B) ─────
//
// Helpers below stage the per-doc + per-FN prereqs required by
// `stage_finalize::run` to advance a doc Kvt2 → Ack atomically.
// Operator-pinned 4b infra-only scope (no production change, no
// behavioral test change): these helpers are defined-but-unused in
// 4b.2 (`#[allow(dead_code)]`) and consumed in 4b.3 when broken
// pre-W12 stub-locking fixtures migrate to W12 ACK-era assertions.
//
// Required prereqs for stage_finalize::run to succeed (per
// `write_path/stage_finalize.rs` + W8 contract):
//   1. fiscal_documents.unsigned_xml_sha256 — NOT NULL on Kvt2 docs
//      (used as the new chain seed for the NEXT doc).
//   2. fiscal_documents.previous_hash — MUST match
//      node_state.last_known_unsigned_xml_sha256 (W8 F2 chain-
//      continuity guard).
//   3. fiscal_documents.signing_inputs_pinned_at — populated to
//      satisfy W6 "signing inputs pinned" invariant.
//   4. ingress_inbox row in PROCESSING — stage_finalize Step 5
//      marks DONE atomically with the Kvt2→Ack CAS.
//
// The seed helpers assume the caller has already inserted the
// fiscal_documents row + document_files SIGNED_XML (via the
// per-file seed_doc / seed_complete_offline_local_ack patterns)
// and only stages the W12 extras.

/// **M3b W12 Commit 4b.2 (infra-only, 2026-05-22)** — stage the W12
/// finalize prereqs onto an existing fiscal_documents row.
///
/// `prev_hash` MUST match the current `node_state.last_known_unsigned_
/// xml_sha256` for this FN at the time `stage_finalize::run` is
/// invoked.  For a chain of docs, caller stages doc 1 with
/// `prev_hash = initial_chain_seed`, then after stage_finalize::run
/// commits doc 1 → Ack (advancing node_state to doc 1's
/// `unsigned_xml_sha256`), stages doc 2 with `prev_hash = doc 1's
/// unsigned_xml_sha256`.
///
/// `unsigned_xml_sha256` value is the chain anchor that the NEXT
/// doc's `previous_hash` must match.  Tests pick deterministic
/// values keyed off doc lnd / id to keep chain bookkeeping simple.
///
/// Internally:
///   1. UPDATE fiscal_documents SET unsigned_xml_sha256, previous_hash,
///      signing_inputs_pinned_at WHERE document_id = ?.
///   2. SELECT request_id FROM fiscal_documents WHERE document_id = ?
///      (read-back to share request_id with ingress_inbox row).
///   3. INSERT OR IGNORE INTO ingress_inbox (request_id, fiscal_number,
///      protocol, operation_type, idempotency_key, payload_json,
///      payload_sha256_canonical, status=PROCESSING).
///
/// `INSERT OR IGNORE` makes the helper safe to call multiple times
/// on the same doc (idempotent — re-running enrichment in a test
/// fixture refactor does not error).
#[allow(dead_code)]
pub async fn seed_w12_finalize_prereqs(
    pool: &SqlitePool,
    fn_id: &str,
    doc_id: DocumentId,
    prev_hash: [u8; 32],
    unsigned_xml_sha: [u8; 32],
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE fiscal_documents SET \
            unsigned_xml_sha256 = ?, \
            previous_hash = ?, \
            signing_inputs_pinned_at = '2026-05-20T00:00:00Z' \
         WHERE document_id = ?",
    )
    .bind(&unsigned_xml_sha[..])
    .bind(&prev_hash[..])
    .bind(doc_id)
    .execute(pool)
    .await?;
    let request_id: Vec<u8> =
        sqlx::query_scalar("SELECT request_id FROM fiscal_documents WHERE document_id = ?")
            .bind(doc_id)
            .fetch_one(pool)
            .await?;
    let payload_sha = vec![0u8; 32];
    let idempotency_key = format!("idem-w12-{}", hex_encode_lower(&request_id));
    sqlx::query(
        "INSERT OR IGNORE INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical, status) \
         VALUES (?, ?, 'REST', 'sell', ?, '{}', ?, 'PROCESSING')",
    )
    .bind(&request_id[..])
    .bind(fn_id)
    .bind(&idempotency_key)
    .bind(&payload_sha)
    .execute(pool)
    .await?;
    Ok(())
}

/// **M3b W12 Commit 4b.2 (infra-only, 2026-05-22)** — seed the
/// initial chain anchor in node_state.  Subsequent
/// `stage_finalize::run` invocations advance this column as each
/// doc reaches Ack (W8 F3 contract); caller stages the FIRST doc
/// of a chain with `prev_hash = initial_seed`.
///
/// UPDATE on existing node_state row — caller MUST have already
/// inserted node_state (via the per-file seed_node_state helper)
/// before invoking this.
#[allow(dead_code)]
pub async fn init_chain_seed(
    pool: &SqlitePool,
    fn_id: &str,
    initial_seed: [u8; 32],
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE node_state SET last_known_unsigned_xml_sha256 = ? \
         WHERE fiscal_number = ?",
    )
    .bind(&initial_seed[..])
    .bind(fn_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// **M3b W12 Commit 4b.2 (infra-only, 2026-05-22)** — deterministic
/// 32-byte chain anchor keyed off a single discriminator byte.
/// Convenience for test fixtures building synthetic chains —
/// `chain_anchor(0x01)` ≠ `chain_anchor(0x02)`, supports up to 256
/// unique anchors per fixture which exceeds any realistic test
/// chain depth.
#[allow(dead_code)]
pub fn chain_anchor(discriminator: u8) -> [u8; 32] {
    [discriminator; 32]
}

/// Local helper — lowercase hex of arbitrary byte slice.  Inlined
/// instead of importing `prro::services::write_path::types::
/// hex_encode_lower` to avoid pulling production hex impl into
/// tests-only module dependency surface.
fn hex_encode_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
