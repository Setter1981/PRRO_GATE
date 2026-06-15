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
    /// Offline issuance is LOCAL (no wire call at issuance); the carried script
    /// is reserved for the offline doc's later drain interaction and is unused
    /// at `OFFLINE_LOCAL_ACK` (spec §5: offline_sell consumes a code locally).
    OfflineSell(DpsScript),
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
