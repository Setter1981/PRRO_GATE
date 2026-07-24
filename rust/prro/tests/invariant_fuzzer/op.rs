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
    pub fn bad_hash_prev() -> Self {
        Self(vec![WireResponse::BadHashPrev])
    }

    /// CS-3 Slice E: the DPS returns a parsed envelope with an unnamed non-zero status `code`
    /// (`-4`/`-17`) → `UnknownStatus → ProbeRequired` HELD (SENDING + STOP + PENDING_APPLY, one wire,
    /// no auto-resend). A single send response (the leaf HOLDS; no `last_chk` in the send path).
    pub fn unknown_status(code: i32) -> Self {
        Self(vec![WireResponse::UnknownStatus(code)])
    }
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
