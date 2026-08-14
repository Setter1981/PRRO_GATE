//! Operation alphabet (spec §5) + the per-op DPS wire script.
//!
//! PURE, enumerable data — no execution.  The mapping
//! `WireResponse -> Result<CheckAck, DpsError>` and the feed into a
//! `ScriptedDps` queue belong to the Task 2 interpreter, NOT here.

/// Kill-point stages — the document / offline commit boundaries the kill-point
/// matrix (`kill_point_matrix.rs`, K1..K9) pins (spec §5: `crash@{...}`).  Pure
/// marker data; the Task 2 interpreter maps each to drop-injection (wire
/// stages) or stage-composition (non-wire boundaries).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    Acquire,
    Sign,
    Send,
    Kvt1,
    Kvt2,
    Finalize,
    OfflineAck,
    Drain,
}

/// The chain tip DPS claims to hold when it answers `-12 ERROR_BAD_HASH_PREV`.
///
/// The stub embeds this in the `"store "` field of the error message, so the
/// fuzzer's `-12` carries the SHAPE live DPS actually sends (captured
/// 2026-07-31, `bd PRRO_GATE-2ds`) rather than a bare code string.
///
/// Nothing consumes it yet — S7-1 retired the inline MAC-recovery orchestrator,
/// so `-12` simply HOLDS. It is here deliberately anyway: the fix for
/// `bd PRRO_GATE-3uo` makes guard-B accept an operator seed corroborated by this
/// very field, and that fix cannot be exercised generatively against a stub
/// whose `-12` carries no `store` at all.
///
/// Deliberately distinct from every `synth_unsigned_hash(lnd)` (those carry `lnd`
/// big-endian in the leading 8 bytes, so a leading `0xD9…` can never collide with
/// a document hash for any plausible `lnd`).
pub const DPS_RECOVERY_TIP: [u8; 32] = [
    0xd9, 0x51, 0x2c, 0x0d, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x22, 0x33, 0x44,
    0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00, 0x0d, 0x15, 0x2c, 0x0d,
];

/// Peer-tip axis PHASE C-2 (spec §5) — the PEER's side of a HELD wire outcome, NAMED by the
/// generator because the wire itself cannot say it.
///
/// A held outcome is precisely one where no trusted envelope came back, so "did DPS take the
/// document" has no answer on the client side — production holds exactly because it cannot know.
/// The peer, however, DID one of the two things. Phase C-1 (the axis's own phase A/B) modelled that
/// as ignorance on both sides; C-2 splits it into the two worlds, so the model's mirror keeps
/// asserting through a hold instead of falling silent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PeerTruth {
    /// DPS accepted the document; its tip advanced onto it.  We hold ⇒ the chains DIVERGE, which is
    /// the real-world shape an operator's `Accepted` claim would (correctly) resolve.
    Took,
    /// DPS did not take it; its tip holds.  The chains still AGREE — which is what makes this branch
    /// the valuable one: nothing diverged, so every downstream assertion stays live.
    NotTook,
}

/// One DPS wire-call response (spec §5).  Not mapped to `Result<CheckAck, _>`
/// here — that is the Task 2 interpreter's job.  Pure enumerable data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WireResponse {
    Ack,
    Reject,
    Timeout,
    Superseded,
    BadHashPrev,
    NotFound,
    /// CS-3 Slice E oracle: a parsed DPS envelope carrying an unnamed non-zero status code
    /// (`-4 ERROR_UNKNOWN`, `-17`, …) → the `UnknownStatus` evidence leaf → `ProbeRequired` HELD.
    /// **Appended LAST** so existing regression-corpus seed indices are preserved. Unlike the
    /// other arms, this MUST reach the wire through the REAL production decode
    /// (`observe_check_reply` / `scripted_raw_observation`), NOT a legacy `DpsError` — a legacy
    /// `Indeterminate` degrades to `NoResponse` in `observe_faithful_from_legacy`, losing the
    /// `ProbeRequired` classification (see `interp::load_script` + `ScriptedDps` obs-override).
    UnknownStatus(i32),
    /// Peer-tip axis PHASE C-2 (spec §5, §4 rows 3 + 9) — a HELD send whose PEER-side truth is
    /// ANNOTATED by the generator.  **Appended LAST** (corpus-seed indices preserved).
    ///
    /// OUR side is byte-identical to [`WireResponse::Superseded`]: the same
    /// `ServerFiscalIdMismatch` reaches the same production routing (`WrapperBug` → a recorded HOLD,
    /// doc `SENDING` under `PENDING_APPLY`, node `STOP_MODE`).  That identity is deliberate — the
    /// leaf must add a peer fact, never a new client-side contract to re-verify; the whole delta
    /// lives on the other side of the wire.
    ///
    /// Why `Superseded` and not `Timeout`: `Timeout` is generator-EXCLUDED (its scenario is realized
    /// via `Crash` drop-injection), so its client-side routing has no differential coverage to
    /// inherit.  And the choice is honest for this leaf — a mismatched server fiscal id means the
    /// reply came back UNUSABLE, which is exactly a state where DPS may or may not hold the
    /// document.
    HeldWithPeer(PeerTruth),
}

/// An ORDERED queue of per-call wire responses for a wire-hitting op.  A real
/// path makes MULTIPLE wire calls (send -> last_chk -> drain probes), so a
/// single response per op is too weak — that is exactly where the convergence /
/// drain defects live (plan Task 1 audit MED).  The interpreter plays these
/// into the `ScriptedDps` queue in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DpsScript(pub Vec<WireResponse>);

impl DpsScript {
    /// send -> Ack, last_chk -> Ack (the happy online / drain path).
    pub fn ack_path() -> Self {
        Self(vec![WireResponse::Ack, WireResponse::Ack])
    }

    /// send -> Ack, last_chk -> NotFound (SENT held pending probe; K4 shape).
    pub fn send_ack_then_last_not_found() -> Self {
        Self(vec![WireResponse::Ack, WireResponse::NotFound])
    }

    /// send -> Reject (a per-document DPS reject).
    pub fn send_then_reject() -> Self {
        Self(vec![WireResponse::Reject])
    }

    /// The n-th wire call (1-based) times out; the preceding calls Ack.
    pub fn timeout_at_call(n: usize) -> Self {
        let mut v = vec![WireResponse::Ack; n.saturating_sub(1)];
        v.push(WireResponse::Timeout);
        Self(v)
    }

    /// The server tip is superseded (a newer tip exists than the one sent).
    pub fn superseded_tip() -> Self {
        Self(vec![WireResponse::Superseded])
    }

    /// The send is rejected for a bad previous-hash chain link.
    ///
    /// ONE response, and that is the FULL contract: CS-3 S7-1 (R3) retired the
    /// inline MAC-recovery orchestrator, so a `-12` produces a
    /// `MacReseedPending` HELD (node `STOP_MODE`, doc rests `SENDING` under a
    /// `PENDING_APPLY` reservation) and there is **no second wire**. Pins:
    /// `write_path_stage4_send::minus_12_bad_hash_prev_records_held_stop_no_second_wire`,
    /// `record_outcome::rc05_bad_hash_prev_held_stop_mode`.
    ///
    /// A two-response "recovery cycle" script was written for this and then
    /// deleted: it modelled a retry that production no longer performs. The
    /// stale `error_routing.rs` comment describing "bounded ONE auto-recovery"
    /// is what made it look real — see `bd PRRO_GATE-3uo`.
    pub fn bad_hash_prev() -> Self {
        Self(vec![WireResponse::BadHashPrev])
    }

    /// Peer-tip axis PHASE C-2 — the HELD send of [`WireResponse::HeldWithPeer`]: one response, the
    /// leaf HOLDS (no `last_chk` follows a held send), and the peer's truth rides along.
    pub fn held_with_peer(truth: PeerTruth) -> Self {
        Self(vec![WireResponse::HeldWithPeer(truth)])
    }

    /// CS-3 Slice E: the DPS returns a parsed envelope with an unnamed non-zero status `code`
    /// (`-4`/`-17`) → `UnknownStatus → ProbeRequired` HELD (SENDING + STOP + PENDING_APPLY, one wire,
    /// no auto-resend). A single send response (the leaf HOLDS; no `last_chk` in the send path).
    pub fn unknown_status(code: i32) -> Self {
        Self(vec![WireResponse::UnknownStatus(code)])
    }
}

/// bd `PRRO_GATE-2ds` / `PRRO_GATE-hpc` — the DECIDED outcomes of a T=112
/// `ask_offline_codes` replenish.
///
/// **Deliberately only two leaves.** `RULING 2` §4
/// (`docs/RULINGS_2026-07-10_SHIFT_T112_AUTOZ.md`) pins the ambiguous /
/// transport-timeout branch as **known-red until a live capture lands**
/// (bd `PRRO_GATE-2ds`, blocked on the operator), so the generator MUST NOT emit
/// it: doing so would pin a contract that has no evidence behind it yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplenishLeaf {
    /// DPS grants a code window.  Prod persists the codes (`INSERT OR IGNORE`),
    /// advances the chain seed to `sha256(request_xml)` — a NON-DOCUMENT seed —
    /// and appends the durable witness row (migration 040), all in ONE
    /// `with_immediate` envelope.  Allocates NO `lnd`.
    Granted,
    /// DPS refuses server-side: NO codes persisted, NO seed advance, NO witness.
    ServerReject,
}

/// Operation alphabet (spec §5).  Wire-hitting ops carry a [`DpsScript`]; there
/// is intentionally NO `go_offline` op — the offline lane is fixture-seeded
/// (spec §5).  Invalid / re-entry / replay ops are first-class so the generator
/// (Task 3) can deliberately emit them to exercise the guard / idempotency /
/// shared-predicate paths the M2-N1 / AUD-K8-1 / SW-1..3 bugs lived in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    // ── valid ──
    OnlineSell(DpsScript),
    GoOnline(DpsScript),
    /// Offline issuance is LOCAL — no wire call at issuance (spec §5:
    /// offline_sell consumes a code locally).  The offline doc's later wire
    /// interaction is driven by the `Drain` op's own `DpsScript`, so this
    /// variant carries no script.
    OfflineSell,
    /// PR-R-fuzz — a RETURN is chain-wise IDENTICAL to a SELL (consumes an
    /// `lnd`, advances the FN seed at the same boundary, participates in the
    /// D5 gate + offline-code pool identically — all three verified
    /// doc-type-agnostic in prod).  The SELL vs RETURN delta is purely
    /// differential-level (sum_out / wire `T=1`), never model state.  Mirrors
    /// the Sell pair: `OnlineReturn` carries a wire script; offline issuance is
    /// LOCAL so `OfflineReturn` carries none.
    OnlineReturn(DpsScript),
    OfflineReturn,
    /// Online service cash-in (службове внесення).  Wire-hitting; carries a script.
    OnlineServiceIn(DpsScript),
    /// Online service cash-out (службова видача).  Wire-hitting; carries a script.
    OnlineServiceOut(DpsScript),
    /// Offline service cash-in.  Issuance is local (OFFLINE_LOCAL_ACK + code
    /// consumed); drain/GoOnline submits it.  Mirrors `OfflineSell`/`OfflineReturn`.
    OfflineServiceIn,
    /// Offline service cash-out.  Issuance is local; drain/GoOnline submits it.
    /// Guard-3b (cash-floor) is NOT checked in-lease for offline — only
    /// pre-inbox L1 guard fires.  The fuzzer fixture always has sufficient
    /// cash after a prior `OfflineServiceIn`.
    OfflineServiceOut,
    /// Online shift-open path.  The script drives send/last.
    OnlineShiftOpen(DpsScript),
    /// Offline shift-open path.  Issuance is local; Drain/GoOnline submits it.
    OfflineShiftOpen,
    /// Online close-shift / Z-report path.  The production write path treats
    /// `Z_REPORT` as the close-shift wire artifact; the script drives send/last.
    OnlineZReport(DpsScript),
    /// Offline close-shift / Z-report path.  Issuance is local; later wire
    /// interaction is driven by the existing Drain/GoOnline ops.
    OfflineZReport,
    /// Online EPZ — видача готівки за ЕПЗ (cash advance against a card).
    /// Wire-hitting (`<C T='8'>`); carries a script.  Cash-OUT (`− epz_out`);
    /// guard-3c refuses (NoMutation) when the card sum exceeds cash-on-hand.
    OnlineEpz(DpsScript),
    /// Offline EPZ.  Issuance is local (OFFLINE_LOCAL_ACK + code consumed);
    /// drain/GoOnline submits it.  Mirrors `OfflineServiceOut`.
    OfflineEpz,
    /// L6 — X-report (поточний звіт): a LOCAL-ONLY, SIDE-EFFECT-FREE read of the
    /// current open shift's turnover.  Carries NO `DpsScript` (never hits the
    /// wire) and can be inserted at ANY point in a sequence.  The whole point is
    /// that it mutates NOTHING: no lnd, no seed, no code, no doc row, no inbox
    /// row, no shift transition — the model predicts `NoMutation` and the oracle
    /// asserts the six-point no-mutation set PLUS that the returned turnover
    /// snapshot (cash-on-hand) matches the model's tracked total.
    XReport,
    /// L5 — a SELL whose AMOUNT-SHAPE deliberately spans the four fail-closed
    /// pre-inbox input guards (`convert.rs`).  Unlike every other SELL op — which
    /// seeds an already-CONVERTED signer payload directly and enters at
    /// `inline::run` — this op drives the SELL THROUGH `convert_to_signer_payload`
    /// (the layer the L5 guards live in), so an over-cap / zero-price /
    /// zero-payment / underpaid shape is actually REFUSED pre-inbox (no row) and a
    /// `Valid` shape converts + issues.  Carries the violation `kind`; the wire
    /// script (for `Valid`) is fixed to `ack_path` (issuance-shape is orthogonal
    /// to the amount guard).  This is the "equivalent bounded strategy spanning
    /// the guard boundaries" the L5 fuzzer mandate asks for.
    L5Probe(L5Kind),
    Drain(DpsScript),
    Crash(Stage),
    Reboot,
    // ── invalid / re-entry / replay ──
    RepeatDrain,
    RepeatReboot,
    DuplicateIdemKey,
    GoOnlineWithoutBacklog,
    OfflineSellDuringGoingOnline,
    SellWithClosedShift,
    /// T=112 offline-code replenish — the ONLY op that moves the MAC chain seed
    /// WITHOUT minting a document.  Drives the REAL
    /// `OfflineCodeReplenishService`, so the durable witness (migration 040) and
    /// the `active_chain_tip` fold that bd `PRRO_GATE-hpc` added are exercised
    /// generatively rather than by directed tests alone.  Allocates no `lnd`,
    /// which is exactly what makes the witness's `lnd_at_write` ordering frame
    /// meaningful.
    Replenish(ReplenishLeaf),
    /// CS-3 operator-completion — the SOLE legal exit from a `PENDING_APPLY` + `STOP_MODE` HELD
    /// reservation (the eternal-BRICK guard).  Drives the real `admin::resolve_operator_pending`
    /// seam against the doc held by the most-recent wire op; a no-op when no held reservation rests.
    /// **Appended LAST** (preserves the `Op` discriminant order the regression corpus depends on).
    OperatorComplete(OperatorResolutionKind),
    /// Peer-tip axis PHASE C-2 (spec §4 row 12, §5) — `Crash(Stage::Send)` with the PEER's truth
    /// named.  **Appended LAST** (same corpus rule).
    ///
    /// A separate op rather than an annotated leaf because the leaf is provably never consumed: the
    /// crash parks INSIDE the `send_chk` await, after the envelope was handed over and the spy
    /// recorded it, but BEFORE the response queue is popped (`scripted_dps.rs`, the hang hook
    /// precedes the pop).  So the script has no say here — the choice has to be its own generator
    /// dimension, which is exactly what the spec's §5 says and what row 12 records.
    ///
    /// `NotTook` is the branch that buys coverage back: a bare `Crash(Send)` marks the run diverged
    /// forever, so phase A's mismatch assertion is dead for the whole REST of the sequence.  With
    /// the peer's refusal named, the two sides still agree and the assertion keeps its teeth.
    CrashSend(PeerTruth),
}

/// The operator's verified resolution of a HELD reservation.  A test-local mirror of the prod
/// `OperatorResolution` (op.rs must not import prod types into the generator); the interpreter maps
/// each kind to the real `OperatorResolution` when it drives `resolve_operator_pending`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorResolutionKind {
    /// DPS accepted (operator supplies the observed FN) → doc `SENT`, seed advances, node released.
    Accepted,
    /// DPS did not accept (ONLINE origin) → doc `REQUIRES_MANUAL_RECONCILIATION`, node released.
    NotAccepted,
    /// OFFLINE-origin not accepted → doc RMR + OLA-cohort cancel + chain rewind.
    NotAcceptedOffline,
    /// `-12` MacReseed (operator supplies the corrected seed) → doc RMR, seed re-based, node released.
    MacReseed,
}

/// L5 amount-shape for an [`Op::L5Probe`].  Each violation kind targets exactly
/// one of the four fail-closed pre-inbox guards in `convert.rs`; `Valid` is the
/// admit-and-issue control.  The interpreter builds a `CanonicalCommand` with the
/// matching amount shape and drives it through `convert_to_signer_payload`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum L5Kind {
    /// G1 — a single cash payment of 5_000_000 kop (Σ cash > 4_999_999 cap).
    OverCap,
    /// G2 — a good priced 0 (item_sum_kop == 0).
    ZeroPrice,
    /// G3 — a card leg that covers the good + a zero-amount cash leg.
    ZeroPayment,
    /// G4 — a good of 1000 kop paid by a 900-kop cash leg (underpaid).
    Underpaid,
    /// Control — a good of 15_000 kop paid in full by cash (converts + issues).
    Valid,
}

impl Op {
    /// CS-3 Slice E — the wire `DpsScript` carried by an online wire op that mints a HELD-able Doc
    /// (rests SENDING under a reservation), for the held-witness delivery-axis oracle.  `None` for
    /// non-wire / offline / recovery ops (`GoOnline` / `Drain` produce a `Recovered` ledger-delta,
    /// not a single held Doc, so they are intentionally excluded).
    pub fn wire_script(&self) -> Option<&DpsScript> {
        match self {
            Op::OnlineSell(s)
            | Op::OnlineReturn(s)
            | Op::OnlineServiceIn(s)
            | Op::OnlineServiceOut(s)
            | Op::OnlineEpz(s)
            | Op::OnlineShiftOpen(s)
            | Op::OnlineZReport(s) => Some(s),
            _ => None,
        }
    }
}
