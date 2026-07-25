//! Reusable scripted `DpsChannel` stub for integration tests
//! (invariant-fuzzer Phase 0, Task 0).
//!
//! De-dups the two former `KpStub` copies that lived in
//! `tests/kill_point_matrix.rs` and `tests/online_convergence_tick.rs`.
//! This is the *superset* of both: the kill-point stub (dual response
//! queues + shared `Arc<AtomicUsize>` counters that survive the simulated
//! restart + per-method oneshot "hang" hooks for drop-injection) is the
//! source of truth; the convergence stub was the same shape minus the hang
//! hooks.
//!
//! Two additions over the former `KpStub`:
//!
//!   1. **Call log (the "envelope spy").**  Every `send_chk` / `last_chk`
//!      call is recorded, in order, with its envelope metadata (a clone of
//!      the `CheckEnvelope` / `CheckSignBlob`).  The fuzzer interpreter
//!      (Task 2+) asserts on the exact wire sequence; the K/convergence
//!      tests only need the counters and ignore the log.
//!
//!   2. **Typed unexpected-call error (no panic).**  An over-call against an
//!      empty queue returns `DpsError::Internal` — the variant the transport
//!      module documents for exactly this ("stub paths so they fail loudly
//!      without panicking").  The former `KpStub` did `pop_front().expect(..)`,
//!      which surfaces an over-send as a *panic* — flaky inside a dropped /
//!      spawned future under the fuzzer.  Returning a typed `Err` lets an
//!      over-send surface as a clean assertion instead.  This does not change
//!      any existing test: the K/convergence tests never over-call in their
//!      green happy path (and where over-call is the bug under test, e.g. the
//!      AUD-K8-1 re-tick pin, the counter increments *before* the queue check,
//!      so the wrongful call is still caught by the `send_calls == N`
//!      assertion / the surrounding `drain(..).expect(..)`).
//!
//! **Concurrency contract.**  `std::sync::Mutex` (not `tokio::sync::Mutex`):
//! every lock is taken, mutated, and dropped within a single statement — no
//! guard is ever held across an `.await`.  The hang hook awaits a `oneshot`
//! *after* the guard is released.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::oneshot;

use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{
    CheckAck, CheckEnvelope, CheckSignBlob, OfflineCodesResponse, RroInfo, StatusSnapshot,
};
use prro::transports::dps::error::DpsError;

/// One recorded wire call — the "envelope spy" entry.  Carries the call kind
/// plus a clone of the argument so a test can assert the exact wire sequence
/// and per-call metadata (`lnd` on a send, the `fn_sign` blob on a lastChk).
#[derive(Debug, Clone)]
pub enum DpsCall {
    SendChk(CheckEnvelope),
    LastChk(CheckSignBlob),
    /// T=112 ASK_OFFLINE_CODES call — carries the envelope sent to DPS.
    AskCodes(CheckEnvelope),
}

/// Scripted DPS stub: two response queues (`send_chk` / `last_chk`), call
/// counters held behind `Arc<AtomicUsize>` so they can be SHARED across a
/// pre-crash and a post-recovery instance (the kill-point "counted through the
/// restart" assertions), an ordered call log, and an optional per-method hang
/// hook for cancellation-injection.
pub struct ScriptedDps {
    send_q: Mutex<VecDeque<Result<CheckAck, DpsError>>>,
    last_q: Mutex<VecDeque<Result<CheckAck, DpsError>>>,
    send_calls: Arc<AtomicUsize>,
    last_calls: Arc<AtomicUsize>,
    send_reached: Mutex<Option<oneshot::Sender<()>>>,
    send_block: Mutex<Option<oneshot::Receiver<()>>>,
    last_reached: Mutex<Option<oneshot::Sender<()>>>,
    last_block: Mutex<Option<oneshot::Receiver<()>>>,
    /// `status_rro` response queue (the return-online probe's connectivity
    /// check).  Empty → a typed error, never a panic (same discipline as the
    /// send/last queues).  Added for the Task 2/3 `GoOnline` interpreter arm
    /// (`return_online_probe::run_tick_for_fn` calls `status_rro`).  Existing
    /// consumers never call `status_rro`, so this is behaviorally inert for them.
    status_q: Mutex<VecDeque<Result<StatusSnapshot, DpsError>>>,
    /// T=112 ASK_OFFLINE_CODES response queue.  Empty → `DpsError::Internal`
    /// (same over-call discipline as the other queues).
    ask_codes_q: Mutex<VecDeque<Result<OfflineCodesResponse, DpsError>>>,
    calls: Mutex<Vec<DpsCall>>,
    /// CS-3 Slice E oracle: OPTIONAL per-send observation OVERRIDE. When non-empty, the next
    /// `send_chk_observed` returns this REAL-decode `RawSendObservation` (from
    /// `scripted_raw_observation(gen::CheckResponse{status})`) instead of the faithful-from-legacy
    /// reconstruction — the ONLY way the fuzzer can drive an `UnknownStatus(-4/-17)` leaf (a legacy
    /// `Indeterminate` degrades to `NoResponse` in `observe_faithful_from_legacy`, losing the
    /// `ProbeRequired` classification). The wire STILL happens (send_chk counts/spies/hangs); only
    /// the OBSERVATION is overridden. Enqueued IN LOCKSTEP with a matching `push_send` legacy.
    send_obs_override_q: Mutex<VecDeque<prro::transports::dps::raw_reply::RawSendObservation>>,
}

impl ScriptedDps {
    /// Shared counters so a phase-1 (pre-crash) and a phase-2 (recovery)
    /// instance count `send_chk` / `last_chk` THROUGH the simulated restart.
    pub fn new(send_calls: Arc<AtomicUsize>, last_calls: Arc<AtomicUsize>) -> Self {
        Self {
            send_q: Mutex::new(VecDeque::new()),
            last_q: Mutex::new(VecDeque::new()),
            send_calls,
            last_calls,
            send_reached: Mutex::new(None),
            send_block: Mutex::new(None),
            last_reached: Mutex::new(None),
            last_block: Mutex::new(None),
            status_q: Mutex::new(VecDeque::new()),
            ask_codes_q: Mutex::new(VecDeque::new()),
            calls: Mutex::new(Vec::new()),
            send_obs_override_q: Mutex::new(VecDeque::new()),
        }
    }

    pub fn push_send(&self, r: Result<CheckAck, DpsError>) {
        self.send_q.lock().unwrap().push_back(r);
    }

    /// CS-3 Slice E: enqueue a REAL-decode observation OVERRIDE for the next `send_chk_observed`
    /// (used with an in-lockstep `push_send` legacy). Lets the fuzzer drive the `UnknownStatus` leaf
    /// through the production decode (`scripted_raw_observation`) instead of the degrading
    /// faithful-from-legacy path. The `send_chk` half still counts the wire + honours the hang hook.
    pub fn push_send_obs_override(
        &self,
        obs: prro::transports::dps::raw_reply::RawSendObservation,
    ) {
        self.send_obs_override_q.lock().unwrap().push_back(obs);
    }

    pub fn push_last(&self, r: Result<CheckAck, DpsError>) {
        self.last_q.lock().unwrap().push_back(r);
    }

    /// Enqueue a `status_rro` response (the return-online probe).
    pub fn push_status(&self, r: Result<StatusSnapshot, DpsError>) {
        self.status_q.lock().unwrap().push_back(r);
    }

    /// Enqueue an `ask_offline_codes` response (T=112 ASK_OFFLINE_CODES).
    pub fn push_ask_codes(&self, r: Result<OfflineCodesResponse, DpsError>) {
        self.ask_codes_q.lock().unwrap().push_back(r);
    }

    /// Arm the `send_chk` await to hang.  `reached` fires when the await is
    /// entered (any prior committed envelope is durable by then); `block`
    /// is awaited and parks the call until it resolves (controlled release)
    /// or the surrounding future is dropped (the simulated "crash").
    pub fn hang_send(&self, reached: oneshot::Sender<()>, block: oneshot::Receiver<()>) {
        *self.send_reached.lock().unwrap() = Some(reached);
        *self.send_block.lock().unwrap() = Some(block);
    }

    /// Arm the `last_chk` await to hang (Sent already committed when reached).
    pub fn hang_last(&self, reached: oneshot::Sender<()>, block: oneshot::Receiver<()>) {
        *self.last_reached.lock().unwrap() = Some(reached);
        *self.last_block.lock().unwrap() = Some(block);
    }

    /// A snapshot of the ordered call log (the envelope spy).
    pub fn calls(&self) -> Vec<DpsCall> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl DpsChannel for ScriptedDps {
    async fn send_chk(&self, envelope: CheckEnvelope) -> Result<CheckAck, DpsError> {
        self.send_calls.fetch_add(1, Ordering::SeqCst);
        self.calls.lock().unwrap().push(DpsCall::SendChk(envelope));
        let block = self.send_block.lock().unwrap().take();
        if let Some(block) = block {
            if let Some(reached) = self.send_reached.lock().unwrap().take() {
                let _ = reached.send(());
            }
            // Park until the block resolves (controlled release) or the
            // surrounding future is dropped (the "crash").
            let _ = block.await;
        }
        let popped = self.send_q.lock().unwrap().pop_front();
        popped.unwrap_or_else(|| {
            Err(DpsError::Internal(
                "ScriptedDps.send_chk: response queue empty (over-call / caller forgot to enqueue)"
                    .to_string(),
            ))
        })
    }

    async fn send_chk_observed(
        &self,
        envelope: CheckEnvelope,
    ) -> (
        Result<CheckAck, DpsError>,
        prro::transports::dps::raw_reply::RawSendObservation,
    ) {
        // The wire happens HERE: `send_chk` counts the call, records the spy entry, honours the
        // hang hook (crash drop-injection), and pops the legacy result.
        let legacy = self.send_chk(envelope).await;
        // CS-3 Slice E: if a REAL-decode observation override is queued (an `UnknownStatus` leaf),
        // return it instead of the degrading `observe_faithful_from_legacy` reconstruction. The
        // `legacy` is the production decode's OWN legacy (pushed in lockstep), so the pair is
        // consistent — the wire is still counted, only the OBSERVATION is the real-decode one.
        if let Some(obs) = self.send_obs_override_q.lock().unwrap().pop_front() {
            return (legacy, obs);
        }
        prro::transports::dps::dto::scripted_observation(legacy)
    }

    async fn last_chk(&self, fn_sign: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        self.last_calls.fetch_add(1, Ordering::SeqCst);
        self.calls
            .lock()
            .unwrap()
            .push(DpsCall::LastChk(fn_sign.clone()));
        let block = self.last_block.lock().unwrap().take();
        if let Some(block) = block {
            if let Some(reached) = self.last_reached.lock().unwrap().take() {
                let _ = reached.send(());
            }
            let _ = block.await;
        }
        let popped = self.last_q.lock().unwrap().pop_front();
        popped.unwrap_or_else(|| {
            Err(DpsError::Internal(
                "ScriptedDps.last_chk: response queue empty (over-call / caller forgot to enqueue)"
                    .to_string(),
            ))
        })
    }

    async fn ping(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        unreachable!("stub: ping not exercised");
    }

    async fn status_rro(&self, _: &CheckSignBlob) -> Result<StatusSnapshot, DpsError> {
        let popped = self.status_q.lock().unwrap().pop_front();
        popped.unwrap_or_else(|| {
            Err(DpsError::Internal(
                "ScriptedDps.status_rro: response queue empty (over-call / caller forgot to enqueue)"
                    .to_string(),
            ))
        })
    }

    async fn info_rro(&self, _: &CheckSignBlob) -> Result<RroInfo, DpsError> {
        unreachable!("stub: info_rro not exercised");
    }

    async fn ask_offline_codes(
        &self,
        envelope: CheckEnvelope,
    ) -> Result<OfflineCodesResponse, DpsError> {
        self.calls.lock().unwrap().push(DpsCall::AskCodes(envelope));
        let popped = self.ask_codes_q.lock().unwrap().pop_front();
        popped.unwrap_or_else(|| {
            Err(DpsError::Internal(
                "ScriptedDps.ask_offline_codes: response queue empty (over-call / caller forgot to enqueue)"
                    .to_string(),
            ))
        })
    }
}
