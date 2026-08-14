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
use sqlx::SqlitePool;
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

// ─── Peer-tip axis, PHASE A: observe, never override ────────────────────
//
// Spec: `docs/superpowers/specs/2026-07-31-spec-fuzzer-peer-tip-axis.md`.
//
// The fuzzer models only OUR side of the MAC chain; the peer's tip is whatever
// the script says it is.  Phase A introduces the peer's tip as harness state and
// checks — WITHOUT changing a single reply — that our movers table is right:
// on every wire send, the outgoing document's `previous_hash` must already equal
// the peer's tip, unless a divergence-creating event happened first.
//
// Why this is the load test of the table and not decoration: a wrong mover (say,
// "an offline issuance advances the peer") desynchronises the two sides, and the
// very next send records a mismatch on a run where nothing diverged.  Phase A
// therefore fails loudly on a table error BEFORE any override or model work is
// built on top of it.
//
// The peer reads the REAL ledger (what production actually committed and put on
// the wire), which is sound because the pin-tx and persist-tx are committed
// before stage 4's wire call and the call itself runs outside any write tx
// (frozen invariant #1).

/// The outgoing document as the peer sees it: `(lnd, doc_type, previous_hash,
/// unsigned_xml_sha256)` — the chain link it presents, and what the peer's tip
/// becomes if it accepts.
type OutgoingDoc = (i64, String, Option<Vec<u8>>, Option<Vec<u8>>);

/// Peer-tip axis PHASE C-2 — what the peer did with a document whose reply told the client
/// NOTHING.  The fuzzer's `op::PeerTruth` maps onto this; the two are separate types on purpose:
/// `op.rs` is the fuzzer's own alphabet and this stub is shared with ten other test binaries that
/// must not gain a dependency on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerAcceptance {
    /// The peer TOOK the document — its tip advances onto it while ours holds.  A real divergence,
    /// declared rather than guessed.
    Took,
    /// The peer did NOT take it — both tips hold, so the run has NOT diverged and every downstream
    /// assertion stays live.  This is the branch that buys back assertion coverage.
    NotTook,
}

/// One queued `send_chk` answer: the reply the CLIENT sees, plus — for a held reply — what the peer
/// actually did with the document (phase C-2).  The pair travels together so a truth can never be
/// mis-attributed to a different send than the one it was enqueued with.
type ScriptedSend = (Result<CheckAck, DpsError>, Option<PeerAcceptance>);

/// One observed disagreement between the outgoing document's chain link and the
/// peer's tip, recorded on a run that had NOT yet diverged.
#[derive(Debug, Clone)]
pub struct PeerMismatch {
    pub lnd: i64,
    pub doc_type: String,
    pub doc_previous_hash: Option<String>,
    pub peer_tip: Option<String>,
}

#[derive(Debug, Default)]
struct PeerInner {
    /// The peer's chain tip: the `unsigned_xml_sha256` of the last document it
    /// accepted (`None` = genesis, which the fuzzer fixtures always start at —
    /// they never call `init_chain_seed`).
    tip: Option<Vec<u8>>,
    /// Mismatches observed while the run was still agreeing.  Non-empty ⇒ the
    /// movers table is wrong (or production regressed).
    mismatches: Vec<PeerMismatch>,
    /// Set once a legitimate divergence-creating event occurs; from then on the
    /// two tips MAY differ and mismatches stop being recorded.
    diverged: Option<String>,
    /// Sends the stub could not attribute to a row (diagnostic only — a growing
    /// count means the resolver needs work, not that production is wrong).
    unresolved_sends: usize,
    /// Peer-tip axis PHASE C-2 — the document handed over on the LAST `send_chk`, kept so a
    /// `Crash(Send)` (which drops the future before any reply is popped, so `apply_reply` never
    /// runs) can still be told what the peer did with it.  The envelope was delivered; only the
    /// answer was lost.
    in_flight: Option<OutgoingDoc>,
    /// PHASE B — may the peer REFUSE a mismatched send with a derived `-12`?
    ///
    /// OFF by default, and that default is the phase boundary, not timidity. The
    /// fuzzer's model predicts wire outcomes independently and knows nothing about
    /// the peer; phase A's `mark_diverged` fires on every `OperatorComplete`, on
    /// held replies and on crashes — all routinely generative — so an always-on
    /// override would answer `-12` where the model expects an `Ack` and redden the
    /// differential across the suite. Teaching the model the peer IS phase C
    /// (spec §8, "this is a representation change"). Until then the directed pins
    /// opt in and generative runs are untouched, which is what keeps each phase
    /// independently green.
    derive_rejects: bool,
}

/// Harness-side model of the DPS peer's chain tip (phase A: read-only observer).
pub struct PeerLedger {
    inner: Mutex<PeerInner>,
    pool: SqlitePool,
    fiscal_number: String,
}

impl PeerLedger {
    pub fn new(pool: SqlitePool, fiscal_number: String) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(PeerInner::default()),
            pool,
            fiscal_number,
        })
    }

    /// Mark the run as legitimately diverged.  Callers: the interpreter, for the
    /// no-wire events that move OUR seed without the peer seeing anything
    /// (operator completions) and for a crash whose delivery is unknowable.
    pub fn mark_diverged(&self, reason: &str) {
        let mut g = self.inner.lock().unwrap();
        if g.diverged.is_none() {
            g.diverged = Some(reason.to_string());
        }
    }

    pub fn diverged(&self) -> Option<String> {
        self.inner.lock().unwrap().diverged.clone()
    }

    /// PHASE B — let the peer answer a mismatched send with a DERIVED `-12`.
    ///
    /// Opt-in; see `derive_rejects`. A directed pin calls this to model the DPS
    /// that actually exists: one that remembers its own chain tip and refuses a
    /// document that does not chain onto it, whatever the script had planned.
    /// Without it the harness models a peer that forgets its chain the moment it
    /// stops being told about it — which is why the corroborated-MacReseed
    /// success path had no generative coverage (bd PRRO_GATE-5hc).
    pub fn enable_derived_rejects(&self) {
        self.inner.lock().unwrap().derive_rejects = true;
    }

    /// The reply the peer would give on its own, or `None` to let the script
    /// stand. Called by the stub AFTER the scripted leaf is popped, so the queue
    /// stays in lockstep with the wire calls: one call consumes one leaf whether
    /// or not the peer overrides it. That is faithful — a real DPS would have
    /// refused regardless of what the generator intended for that send.
    ///
    /// Fires ONLY once the run has diverged. Before that, a mismatch is by
    /// construction a movers-table bug (spec §4), and phase A's assertion is what
    /// must surface it; converting it into a `-12` here would change the
    /// trajectory and bury the very error the axis exists to catch.
    fn derived_reject(&self, resolved: &Option<OutgoingDoc>) -> Option<DpsError> {
        let g = self.inner.lock().unwrap();
        if !g.derive_rejects || g.diverged.is_none() {
            return None;
        }
        let (_, _, previous_hash, _) = resolved.as_ref()?;
        if previous_hash.as_deref() == g.tip.as_deref() {
            return None;
        }
        // The live shape, byte-for-byte: two spaces after the code, `store` = the
        // tip this peer holds, `chk` = the link the client actually presented.
        // Unlike the forced leaf — which zero-fills `chk` because the stub cannot
        // know it — here the peer HAS the document in hand, so `chk` is real.
        Some(DpsError::Server {
            code: -12,
            message: format!(
                "ERROR_BAD_HASH_PREV  store {} chk {}",
                hex_lower(g.tip.as_deref().unwrap_or(&[0u8; 32])),
                hex_lower(previous_hash.as_deref().unwrap_or(&[0u8; 32])),
            ),
        })
    }

    /// A CONVERGENCE event: both sides land on the same fresh tip.
    ///
    /// The only instance today is a GRANTED T=112 — live-proven **[N=1]**: after
    /// an ambiguous replenish (peer moved, we did not) a FRESH T=112 was
    /// ACCEPTED and returned the same unused codes, i.e. DPS does not `-12` a
    /// replenish on a stale embedded tip — it accepts and re-bases, so the
    /// divergence HEALS.  Note the surface: T=112 rides `ask_offline_codes`, a
    /// separate stub queue that does not even increment `send_calls`, so the
    /// send-side observer cannot see it; the interpreter reports it here.
    pub fn converge_to(&self, tip: Option<Vec<u8>>) {
        let mut g = self.inner.lock().unwrap();
        g.tip = tip;
    }

    /// Peer-tip axis PHASE D — the peer moves and we do NOT: it processed a request whose reply we
    /// never received (an ambiguous T=112).
    ///
    /// The mirror image of `converge_to`, and deliberately a separate method rather than a flag on
    /// it: the two differ by whether the run has DIVERGED, which is the only thing that decides
    /// whether the next send earns a derived `-12`. Collapsing them into one call with a boolean is
    /// exactly how a caller ends up recording a convergence for a divergence.
    pub fn advance_without_us(&self, tip: Vec<u8>, reason: &str) {
        let mut g = self.inner.lock().unwrap();
        g.tip = Some(tip);
        if g.diverged.is_none() {
            g.diverged = Some(reason.to_string());
        }
    }

    pub fn mismatches(&self) -> Vec<PeerMismatch> {
        self.inner.lock().unwrap().mismatches.clone()
    }

    pub fn unresolved_sends(&self) -> usize {
        self.inner.lock().unwrap().unresolved_sends
    }

    pub fn tip_hex(&self) -> Option<String> {
        self.inner.lock().unwrap().tip.as_deref().map(hex_lower)
    }

    /// The peer's tip as raw bytes — phase C projects it onto a document ordinal so the MODEL's
    /// independently-derived mirror can be compared against it.
    pub fn tip(&self) -> Option<Vec<u8>> {
        self.inner.lock().unwrap().tip.clone()
    }

    /// Resolve the document currently on the wire.
    ///
    /// By state, NOT by `local_number`: `build_send_envelope` hard-overrides
    /// `local_number = 0` for SHIFT_OPEN in both lanes, so an lnd lookup misses
    /// exactly the shift-lifecycle docs.  At wire time the doc is committed in
    /// `SENDING` (the 4-pre CAS), and the per-FN single-writer lease guarantees
    /// there is at most one — so "the FN's SENDING row" identifies every doc
    /// kind uniformly, online and drain alike.
    async fn resolve_outgoing(&self) -> Option<OutgoingDoc> {
        let rows: Vec<OutgoingDoc> = sqlx::query_as(
            "SELECT lnd, doc_type, previous_hash, unsigned_xml_sha256 \
             FROM fiscal_documents WHERE fiscal_number = ? AND state = 'SENDING'",
        )
        .bind(self.fiscal_number.as_str())
        .fetch_all(&self.pool)
        .await
        .ok()?;
        if rows.len() == 1 {
            rows.into_iter().next()
        } else {
            None
        }
    }

    /// Peer-tip axis PHASE C-2 — apply the generator's named peer truth to the document that is (or
    /// was) on the wire, and report whether the peer's tip moved.
    ///
    /// `Took` advances the peer onto the document and marks the run diverged: the peer holds a
    /// document we do not know it holds, which is a REAL divergence — the point is that it is now a
    /// KNOWN one, so the model can keep mirroring instead of falling silent.  `NotTook` moves
    /// nothing and, crucially, does NOT mark the run diverged: both sides still agree, so phase A's
    /// mismatch assertion and the model's mirror both keep their teeth for the rest of the run.
    fn apply_peer_truth(&self, doc: Option<&OutgoingDoc>, truth: PeerAcceptance) -> bool {
        let mut g = self.inner.lock().unwrap();
        match truth {
            PeerAcceptance::Took => {
                let Some((_, _, _, Some(unsigned))) = doc else {
                    return false;
                };
                g.tip = Some(unsigned.clone());
                if g.diverged.is_none() {
                    g.diverged =
                        Some("peer TOOK a held document (phase C-2 leaf) — our side holds".into());
                }
                true
            }
            PeerAcceptance::NotTook => false,
        }
    }

    /// Peer-tip axis PHASE C-2 — resolve the truth of a document whose reply NEVER CAME (a
    /// `Crash(Send)`): the envelope was delivered and the future dropped before the pop, so the
    /// scripted leaf is provably never consumed and the choice has to arrive out-of-band.
    pub fn resolve_in_flight(&self, truth: PeerAcceptance) {
        let doc = self.inner.lock().unwrap().in_flight.clone();
        self.apply_peer_truth(doc.as_ref(), truth);
    }

    /// Called by the stub BEFORE the reply is produced: compare the outgoing
    /// document's chain link against the peer tip.  Returns the resolved row so
    /// the caller can advance the tip once the reply is known.
    async fn observe_send(&self) -> Option<OutgoingDoc> {
        let resolved = self.resolve_outgoing().await;
        let mut g = self.inner.lock().unwrap();
        g.in_flight = resolved.clone();
        match &resolved {
            None => {
                g.unresolved_sends += 1;
            }
            Some((lnd, doc_type, previous_hash, _)) => {
                if g.diverged.is_none() && previous_hash.as_deref() != g.tip.as_deref() {
                    let record = PeerMismatch {
                        lnd: *lnd,
                        doc_type: doc_type.clone(),
                        doc_previous_hash: previous_hash.as_deref().map(hex_lower),
                        peer_tip: g.tip.as_deref().map(hex_lower),
                    };
                    g.mismatches.push(record);
                }
            }
        }
        resolved
    }

    /// Called by the stub AFTER the reply is known.  Accepting reply ⇒ the peer
    /// took the document and its tip becomes that document's
    /// `unsigned_xml_sha256`.  This is the send-`Ack` moment, NOT the whole
    /// script: an `[Ack, NotFound]` tail is the K4 empty-quittance ("accepted,
    /// quittance lagging"), so the peer HAS taken it.
    fn apply_reply(
        &self,
        resolved: Option<OutgoingDoc>,
        reply: &Result<CheckAck, DpsError>,
        truth: Option<PeerAcceptance>,
    ) {
        // Peer-tip axis PHASE C-2 — an ANNOTATED held leaf answers the question the reply cannot,
        // so it takes precedence over the "unknowable" fallback below.  It applies only to the held
        // class: an accepting or `-12` reply says what the peer did all by itself, and letting an
        // annotation contradict a reply the client actually received would model a Byzantine peer
        // (excluded, spec §10).
        if let Some(truth) = truth {
            if matches!(reply, Err(e) if !is_named_peer_verdict(e)) {
                self.apply_peer_truth(resolved.as_ref(), truth);
                return;
            }
        }
        let mut g = self.inner.lock().unwrap();
        match reply {
            Ok(_) => {
                if let Some((_, _, _, Some(unsigned))) = resolved {
                    g.tip = Some(unsigned);
                }
            }
            // A `-12` is the peer NAMING its own tip in the `store` field.  Adopt
            // it and mark the run diverged: from here the two sides legitimately
            // disagree until an operator resolves it.
            Err(DpsError::Server { code: -12, message }) => {
                if let Some(store) = extract_store_hash(message) {
                    g.tip = Some(store);
                }
                if g.diverged.is_none() {
                    g.diverged = Some("peer declared its tip via -12".to_string());
                }
            }
            // A parsed business reject: the peer looked at it and refused — its
            // tip does not move, and the chains still agree.
            Err(DpsError::Server { .. }) => {}
            // Anything else (transport, decode, indeterminate) is a HELD outcome:
            // whether the peer took it is exactly what we cannot know.  Phase A
            // does not guess — it stops asserting.
            Err(_) => {
                if g.diverged.is_none() {
                    g.diverged = Some("held / indeterminate reply — peer state unknowable".into());
                }
            }
        }
    }
}

/// Peer-tip axis PHASE C-2 — does this error carry the PEER's own verdict on the document?
///
/// A `Server` envelope (a business reject, a `-12` naming the peer's tip) and an `Authorization`
/// reject are the peer speaking: it parsed the document and refused it.  Everything else — transport
/// collapse, a decode failure, an unusable server fiscal id — is the HELD class, where the client
/// learns nothing and a generator annotation is the only thing that can say what happened.
fn is_named_peer_verdict(e: &DpsError) -> bool {
    matches!(e, DpsError::Server { .. } | DpsError::Authorization { .. })
}

fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Extract the 64-hex value after the literal `store ` — the shape the live
/// capture pinned (`ERROR_BAD_HASH_PREV  store <64hex> chk <64hex>`, note the
/// two spaces).  Mirrors production's own extractor rather than importing it, so
/// a production regression in the extractor does not silently propagate here.
fn extract_store_hash(message: &str) -> Option<Vec<u8>> {
    let after = message.split("store ").nth(1)?;
    let hex: String = after
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect();
    if hex.len() != 64 {
        return None;
    }
    (0..32)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok())
        .collect()
}

/// Scripted DPS stub: two response queues (`send_chk` / `last_chk`), call
/// counters held behind `Arc<AtomicUsize>` so they can be SHARED across a
/// pre-crash and a post-recovery instance (the kill-point "counted through the
/// restart" assertions), an ordered call log, and an optional per-method hang
/// hook for cancellation-injection.
pub struct ScriptedDps {
    /// Peer-tip axis PHASE C-2 — each send response carries an OPTIONAL peer truth, in the queue
    /// itself rather than in a parallel one.  A second queue would have to be kept in lockstep by
    /// hand at every `push_send` call site (ten test binaries' worth), and the first site that
    /// forgot would silently mis-attribute a truth to the wrong document.
    send_q: Mutex<VecDeque<ScriptedSend>>,
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
    /// Peer-tip axis phase A.  `None` for every existing consumer — the stub is
    /// byte-for-byte unchanged without it; the fuzzer attaches one.
    peer: Option<Arc<PeerLedger>>,
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
            peer: None,
        }
    }

    /// Attach the peer-tip observer (fuzzer only).  Builder-style so the ~25
    /// `new_dps()` call sites stay one expression and the 10 other test files
    /// that build a `ScriptedDps` are untouched.
    pub fn with_peer(mut self, peer: Arc<PeerLedger>) -> Self {
        self.peer = Some(peer);
        self
    }

    pub fn push_send(&self, r: Result<CheckAck, DpsError>) {
        self.send_q.lock().unwrap().push_back((r, None));
    }

    /// Peer-tip axis PHASE C-2 — enqueue a HELD send response together with what the peer did with
    /// the document.  The reply the client sees is unchanged; the annotation only reaches the
    /// harness peer, which is the whole point: production must not be able to tell the difference.
    pub fn push_send_with_peer(&self, r: Result<CheckAck, DpsError>, truth: PeerAcceptance) {
        self.send_q.lock().unwrap().push_back((r, Some(truth)));
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
        // Peer-tip axis phase A: observe BEFORE the hang hook, so a `Crash(Send)`
        // (which drops the future while parked, never reaching the pop) still
        // records what the peer was handed.  Observation only — the reply below
        // is exactly what the script says.
        let resolved = match &self.peer {
            Some(peer) => peer.observe_send().await,
            None => None,
        };
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
        let (reply, peer_truth) = popped.unwrap_or_else(|| {
            (
                Err(DpsError::Internal(
                    "ScriptedDps.send_chk: response queue empty (over-call / caller forgot to \
                     enqueue)"
                        .to_string(),
                )),
                None,
            )
        });
        // Peer-tip axis phase B: the peer may refuse a document that does not
        // chain onto its tip, whatever the script said. The leaf is popped ABOVE
        // either way, so one wire call still consumes one leaf — a real DPS would
        // have refused regardless of what the generator intended for this send.
        // `apply_reply` then sees the reply that was actually returned, so the
        // tip bookkeeping never diverges from what the client observed.
        let reply = match &self.peer {
            Some(peer) => peer.derived_reject(&resolved).map(Err).unwrap_or(reply),
            None => reply,
        };
        if let Some(peer) = &self.peer {
            peer.apply_reply(resolved, &reply, peer_truth);
        }
        reply
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
