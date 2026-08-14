//! bd `PRRO_GATE-6bj` step 1 — ask DPS what it holds while a transient send outcome is HELD, and
//! RECORD the answer. Nothing else.
//!
//! ## The problem this addresses
//!
//! A routine transient DPS outcome (`-3 ERROR_SAVE`, and every other `TransientRetry`-classified
//! reply) leaves the document resting `SENDING` under a `PENDING_APPLY` reservation with the node in
//! `STOP_MODE`. Nothing releases that automatically — `apply_outcome`'s fall-through is
//! `HeldNotAutoRelease`, on the live path and on the boot pass alike — so an operator has to decide.
//! And today they decide BLIND: the gateway never asks DPS what actually happened.
//!
//! ## What this does, and what it deliberately does not
//!
//! It runs the existing `last_chk` probe and writes the verdict to the audit log. That is all.
//!
//! * **No document transition.** Not one CAS. The hold survives this tick exactly as it was.
//! * **No `send_chk`, ever.** The whole point of S7-1 R6 was that a document past `CALL_STARTED`
//!   must not be blindly re-sent; a probe is a READ, and this module must never grow a write.
//! * **No node-mode change.** `STOP_MODE` stays. The FN is still stopped; what changes is that the
//!   operator now has a fact instead of a guess.
//!
//! That fence is copied deliberately from `boot_phase`'s stale-tip guard, which established the
//! shape: probe outside any transaction, then a single short envelope that touches only
//! `node_state`/audit.
//!
//! ## Why it cannot go further today — the honest boundary
//!
//! The obvious next step is "if DPS has the document, release the hold automatically". It cannot be
//! built on this probe, and the reason is structural rather than a matter of effort:
//! [`last_chk_probe::probe`] proves ownership by comparing `ack.id` against the **expected**
//! server-assigned fiscal id — and a held transient outcome is precisely the case where no reply
//! arrived, so there is no expected id to compare against. Establishing "that last check IS mine"
//! would require reading the quittance's `econtent` and matching it to our document. That is a new
//! contract, and it belongs to its own change.
//!
//! So the verdicts here are stated in terms of what CAN be known without it:
//!   * `NotFound` — DPS has no last check for this FN at all. Strong evidence our document never
//!     landed;
//!   * `Mismatch { actual_id }` — DPS holds SOME check; the id is recorded so an operator can look
//!     it up. It is NOT evidence either way about ours;
//!   * transport / decode / unexpected — nothing learned; recorded as such and retried next tick.
//!
//! Reading in `STOP_MODE` is deliberate and has precedent: the T=112 replenish reaches the wire with
//! no node-mode gate at all, and the boot stale-tip guard probes before deciding to block. `STOP`
//! forbids ISSUING, not asking.

use std::time::Duration;

use sqlx::SqlitePool;

use super::last_chk_probe::{self, ProbeOutcome};
use super::runtime::RuntimeView;
use crate::db::models::enums::Severity;
use crate::db::repositories::audit_log;

/// How often the supervisor runs this tick.  Hardcoded — NO config knob, the same D6 precedent as
/// `inbox_reaper::REAPER_TICK_INTERVAL` and the same value: with nothing held a tick is a single
/// SELECT, and with a hold resting the probe is one `last_chk` per five minutes — enough for the
/// verdict to track a DPS outage ending, nowhere near enough to bother anyone.
pub const HELD_PROBE_TICK_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// The audit `event_type` this module writes. One code with the verdict in the payload, rather than
/// a code per verdict: an operator surface greps for the event, then reads what was learnt.
pub const HELD_PROBE_EVENT: &str = "HELD_TRANSIENT_PROBE";

/// `actor` recorded on the audit row. Forensic attribution matters here for the same reason it does
/// on the force seams: a human reading this row must be able to tell that the gateway asked, not a
/// person.
const HELD_PROBE_ACTOR: &str = "system:held-evidence-probe";

/// What one tick learnt. `None` from [`run_tick_for_fn`] means there was nothing to ask about — and
/// in that case NO wire call was made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldEvidence {
    /// Hex of the reservation the evidence is about.
    pub reservation_id_hex: String,
    /// The document's local number, so the audit row is readable without a join.
    pub lnd: i64,
    /// The verdict, in the vocabulary of [`ProbeOutcome`].
    pub verdict: HeldVerdict,
}

/// The verdicts this probe can honestly reach. Deliberately NOT a re-export of [`ProbeOutcome`]:
/// `Match` is unreachable here (there is no expected id to match against — see the module docs), and
/// a type that can express it would invite a caller to handle a case that cannot occur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeldVerdict {
    /// DPS reports no last check for this FN. Strong evidence our document never landed.
    PeerHasNoChecks,
    /// DPS holds some check; the id is recorded for the operator. Says nothing about ours.
    PeerHoldsOther { actual_id: String },
    /// The probe itself failed — nothing was learnt. Retried on the next tick.
    Indeterminate { reason: String },
}

impl HeldVerdict {
    fn as_str(&self) -> &'static str {
        match self {
            Self::PeerHasNoChecks => "PeerHasNoChecks",
            Self::PeerHoldsOther { .. } => "PeerHoldsOther",
            Self::Indeterminate { .. } => "Indeterminate",
        }
    }
}

/// Run one evidence-probe tick for a single FN.
///
/// SELECT-first: with no held transient reservation this returns `Ok(None)` having made ZERO wire
/// calls, which is the common case and is asserted by a test.
pub async fn run_tick_for_fn(
    pool: &SqlitePool,
    view: &RuntimeView<'_>,
    fiscal_number: &str,
) -> anyhow::Result<Option<HeldEvidence>> {
    // The fence-authoritative held reservation, narrowed to the TRANSIENT class. The narrowing is
    // the scope of bd 6bj: a `-12` (`MacReseedPending`) or a `SubmittedUnknown` hold has its own
    // resolution story and is not what this probe can speak to.
    let row: Option<(Vec<u8>, i64)> = sqlx::query_as(
        "SELECT dr.reservation_id, fd.lnd \
         FROM delivery_reservation dr \
         JOIN fiscal_documents fd \
           ON fd.document_id = dr.document_id AND fd.fiscal_number = dr.fiscal_number \
         JOIN node_state ns ON ns.fiscal_number = dr.fiscal_number \
         WHERE dr.fiscal_number = ? \
           AND dr.state = 'OUTCOME_OBSERVED' \
           AND dr.apply_state = 'PENDING_APPLY' \
           AND dr.routing_class = 'TransientRetry' \
           AND dr.reservation_id = ns.active_delivery_reservation_id \
           AND dr.authorized_generation = ns.delivery_generation \
           AND fd.state = 'SENDING' LIMIT 1",
    )
    .bind(fiscal_number)
    .fetch_optional(pool)
    .await?;
    let Some((reservation_id, lnd)) = row else {
        return Ok(None);
    };

    // The wire call, OUTSIDE any transaction (invariant #1). The expected id is empty on purpose:
    // there is none — see the module docs. `Match` is therefore unreachable, and the mapping below
    // says so rather than silently folding it into another verdict.
    let verdict = match last_chk_probe::probe(view.dps, view.fn_sign, "").await {
        ProbeOutcome::NotFound => HeldVerdict::PeerHasNoChecks,
        ProbeOutcome::Mismatch { actual_id } => HeldVerdict::PeerHoldsOther { actual_id },
        ProbeOutcome::Match { ack } => HeldVerdict::PeerHoldsOther { actual_id: ack.id },
        ProbeOutcome::TransportRetry { reason } | ProbeOutcome::DecodeEscalate { reason } => {
            HeldVerdict::Indeterminate { reason }
        }
        ProbeOutcome::Unexpected { dps_error } => HeldVerdict::Indeterminate { reason: dps_error },
    };

    let reservation_id_hex: String = reservation_id.iter().map(|b| format!("{b:02x}")).collect();

    // Record CHANGES of knowledge, not heartbeats. A hold can rest for hours waiting on an
    // operator, and this tick runs every five minutes; without this guard the audit log would grow
    // a row per tick saying the same thing, burying the rows that matter. The comparison is
    // against the LAST row only — verdict transitions (NoChecks → Indeterminate → NoChecks) are
    // all recorded — and on (reservation, verdict), not on `detail`: two Indeterminate probes with
    // different transport texts are the same knowledge.
    // ordering-justified: `audit_id` is `audit_log`'s INTEGER PRIMARY KEY (the rowid alias,
    // allocated by SQLite's monotonic rowid allocator) — unique across the whole table, so no two
    // rows inside this WHERE scope can tie and "the last row" has exactly one winner.
    let last: Option<Option<String>> = sqlx::query_scalar(
        "SELECT event_payload_json FROM audit_log \
         WHERE event_type = ? AND entity_id = ? ORDER BY audit_id DESC LIMIT 1",
    )
    .bind(HELD_PROBE_EVENT)
    .bind(fiscal_number)
    .fetch_optional(pool)
    .await?;
    let already_recorded = last
        .flatten()
        .and_then(|p| serde_json::from_str::<serde_json::Value>(&p).ok())
        .is_some_and(|v| {
            v.get("reservation_id").and_then(|x| x.as_str()) == Some(reservation_id_hex.as_str())
                && v.get("verdict").and_then(|x| x.as_str()) == Some(verdict.as_str())
        });
    if already_recorded {
        return Ok(Some(HeldEvidence {
            reservation_id_hex,
            lnd,
            verdict,
        }));
    }

    let payload = serde_json::json!({
        "reservation_id": reservation_id_hex,
        "lnd": lnd,
        "verdict": verdict.as_str(),
        "detail": match &verdict {
            HeldVerdict::PeerHoldsOther { actual_id } => actual_id.clone(),
            HeldVerdict::Indeterminate { reason } => reason.clone(),
            HeldVerdict::PeerHasNoChecks => String::new(),
        },
    })
    .to_string();
    audit_log::append(
        pool,
        "fiscal_number",
        fiscal_number,
        HELD_PROBE_EVENT,
        Severity::Warning,
        Some(HELD_PROBE_ACTOR),
        Some(&payload),
    )
    .await?;

    Ok(Some(HeldEvidence {
        reservation_id_hex,
        lnd,
        verdict,
    }))
}
