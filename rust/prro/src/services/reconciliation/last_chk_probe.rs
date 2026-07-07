//! W9 SENT recovery — `DpsChannel::last_chk` probe + 3-way
//! classification (per design freeze §4.5).
//!
//! Doc in `Sent` state could be in one of three wire situations:
//!
//! 1. **Reply lost in transit** — DPS received the original `send_chk`,
//!    persisted the doc, and returned the ack, but our gateway's
//!    process crashed before recording the response.  `last_chk`
//!    returns the same ack again; matching `ack.id ==
//!    doc.transport_request_id` proves DPS has the doc → recovery
//!    advances `Sent → Kvt1` locally.
//!
//! 2. **Last-attempt ack doesn't match** — DPS's "last submitted
//!    check" is a different doc than ours.  Two sub-cases:
//!      - Another doc was submitted between our crash and reboot,
//!        OR our doc was never received.  Either way, recovery
//!        cannot prove receipt → escalate to operator.
//!
//! 3. **NotFound** — DPS replies with explicit "no last check"
//!    (server has zero history for our FN_sign).  This is the safe
//!    re-send case: DPS doesn't have our doc; Pattern B re-drive
//!    via `ErrorRetryable → Sending` is correct.
//!
//! Plus three error fall-throughs (M1 review item 3, 2026-06-11 — contract
//! corrected to match the caller's actual, ruled-intended behaviour):
//!   - Transport error → keep `Sent` + audit `BOOT_LAST_CHK_TRANSPORT_RETRY`.
//!   - Decode error → DEFER: keep `Sent` (no state change), re-probe next boot.
//!     (The earlier "escalate to RequiresManualReconciliation / no bounded
//!     retry" wording was aspirational; the SENT-recovery caller defers, which
//!     the architect ruled is the intended safe behaviour — unbounded-defer
//!     visibility belongs to monitoring v1's stuck-FN detector, not a terminal.)
//!   - Unexpected `DpsError` (Authorization / Server{code} / Internal — not
//!     normally surfaced by a query probe) → also DEFER, distinct audit signal.
//!
//! This module emits the **classified outcome**; it does NOT mutate
//! state.  Caller (W9.3 `boot_phase::run_boot_reconciliation`)
//! dispatches per the outcome to the appropriate `boot_phase::*`
//! helper.

use crate::transports::dps::channel::DpsChannel;
use crate::transports::dps::dto::{CheckAck, CheckSignBlob};
use crate::transports::dps::error::DpsError;

/// Closed-enum classification of a `last_chk` probe outcome.
///
/// `Match.ack` carries the full server ack so the caller can extract
/// `ack.id` + `ack.data_sign` (the KVT1 receipt bytes per W9 freeze
/// §4.5 helper `advance_sent_to_kvt1_from_probe`).
#[derive(Debug)]
pub enum ProbeOutcome {
    /// `ack.id == expected_transport_request_id`; doc IS server-
    /// acknowledged.  Caller advances `Sent → Kvt1` via
    /// `boot_phase::advance_sent_to_kvt1_from_probe`.
    Match { ack: CheckAck },

    /// `ack.id` differs from expected.  Caller escalates to
    /// `RequiresManualReconciliation` (operator triage).
    Mismatch { actual_id: String },

    /// DPS returned `NotFound` — has no record of any check for
    /// this FN_sign.  Safe to re-drive via Pattern B
    /// (`ErrorRetryable → Sending → wire`).
    NotFound,

    /// `DpsError::Transport` — transient.  Caller keeps doc in
    /// `Sent`, emits `BOOT_LAST_CHK_TRANSPORT_RETRY` audit.
    TransportRetry { reason: String },

    /// `DpsError::Decode` — protocol drift.  Caller DEFERS (keeps `Sent`, no
    /// state change; re-probed next boot) — M1 review item 3 ruling: defer is
    /// the intended safe behaviour, NOT a terminal `RequiresManualReconciliation`
    /// (despite the historical variant name).  Stuck-FN visibility is monitoring
    /// v1's job.
    DecodeEscalate { reason: String },

    /// W9.4 L4 fix: catch-all for unexpected `DpsError` variants that
    /// `last_chk` shouldn't normally surface (e.g. `Authorization`,
    /// `Server { code }` — these belong to submit paths, not query
    /// probes).  Distinct from `DecodeEscalate` so an operator
    /// dashboard alarming on certificate-auth failures can fire on
    /// the right signal instead of conflating with protocol drift.
    Unexpected { dps_error: String },
}

/// Issue a single `DpsChannel::last_chk` call and classify the
/// response per [`ProbeOutcome`].  Caller captures wire times
/// around this fn (for downstream `complete_via_recovery_tx`); the
/// probe itself is concerned only with outcome classification.
pub async fn probe(
    channel: &dyn DpsChannel,
    fn_sign: &CheckSignBlob,
    expected_transport_request_id: &str,
) -> ProbeOutcome {
    match channel.last_chk(fn_sign).await {
        Ok(ack) => {
            if ack.id == expected_transport_request_id {
                ProbeOutcome::Match { ack }
            } else {
                ProbeOutcome::Mismatch { actual_id: ack.id }
            }
        }
        Err(DpsError::NotFound) => ProbeOutcome::NotFound,
        Err(DpsError::Transport(msg)) => ProbeOutcome::TransportRetry { reason: msg },
        Err(DpsError::Decode(msg)) => ProbeOutcome::DecodeEscalate { reason: msg },
        // W9.4 L4 fix: surface unexpected variants distinctly so
        // operator alerting can fire on the right signal.
        // `last_chk` is a query path; Authorization / Server { code }
        // / etc. belong to submit paths.  Don't conflate with
        // DecodeEscalate (protocol drift).
        Err(other) => ProbeOutcome::Unexpected {
            dps_error: format!("{other}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transports::dps::channel::DpsChannel;
    use crate::transports::dps::dto::{
        CheckAck, CheckEnvelope, CheckSignBlob, RroInfo, StatusSnapshot,
    };
    use crate::transports::dps::error::DpsError;
    use async_trait::async_trait;

    /// Lightweight stub `DpsChannel` for outcome-classification tests.
    /// Each fixture constructs one with a scripted `last_chk` response;
    /// other methods are `unreachable!()` because only `last_chk` is
    /// exercised.
    struct StubChannel {
        last_chk_response: Result<CheckAck, DpsError>,
    }

    #[async_trait]
    impl DpsChannel for StubChannel {
        async fn send_chk(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
            unreachable!("stub: send_chk not exercised in probe tests");
        }
        async fn last_chk(&self, _: &CheckSignBlob) -> Result<CheckAck, DpsError> {
            // Clone the scripted response (CheckAck + DpsError are not
            // Clone in general — re-construct manually for these tests).
            match &self.last_chk_response {
                Ok(ack) => Ok(CheckAck {
                    id: ack.id.clone(),
                    id_sign: ack.id_sign.clone(),
                    data_sign: ack.data_sign.clone(),
                }),
                Err(DpsError::NotFound) => Err(DpsError::NotFound),
                Err(DpsError::Transport(s)) => Err(DpsError::Transport(s.clone())),
                Err(DpsError::Decode(s)) => Err(DpsError::Decode(s.clone())),
                Err(DpsError::Internal(s)) => Err(DpsError::Internal(s.clone())),
                Err(_) => Err(DpsError::Internal("test-only unhandled variant".into())),
            }
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
        async fn ask_offline_codes(
            &self,
            _: CheckEnvelope,
        ) -> Result<crate::transports::dps::dto::OfflineCodesResponse, DpsError> {
            unreachable!("stub: ask_offline_codes not exercised");
        }
    }

    fn fn_sign() -> CheckSignBlob {
        CheckSignBlob(vec![0xDEu8, 0xAD, 0xBE, 0xEF])
    }

    #[tokio::test]
    async fn probe_match_when_ack_id_equals_expected() {
        let stub = StubChannel {
            last_chk_response: Ok(CheckAck {
                id: "abc-123".into(),
                id_sign: vec![],
                data_sign: vec![1, 2, 3],
            }),
        };
        let out = probe(&stub, &fn_sign(), "abc-123").await;
        match out {
            ProbeOutcome::Match { ack } => {
                assert_eq!(ack.id, "abc-123");
                assert_eq!(ack.data_sign, vec![1, 2, 3]);
            }
            other => panic!("expected Match, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn probe_mismatch_when_ack_id_differs() {
        let stub = StubChannel {
            last_chk_response: Ok(CheckAck {
                id: "other-doc".into(),
                id_sign: vec![],
                data_sign: vec![],
            }),
        };
        let out = probe(&stub, &fn_sign(), "abc-123").await;
        match out {
            ProbeOutcome::Mismatch { actual_id } => {
                assert_eq!(actual_id, "other-doc");
            }
            other => panic!("expected Mismatch, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn probe_not_found_on_dps_not_found() {
        let stub = StubChannel {
            last_chk_response: Err(DpsError::NotFound),
        };
        let out = probe(&stub, &fn_sign(), "abc-123").await;
        assert!(matches!(out, ProbeOutcome::NotFound));
    }

    #[tokio::test]
    async fn probe_transport_retry_on_transport_error() {
        let stub = StubChannel {
            last_chk_response: Err(DpsError::Transport("connection reset".into())),
        };
        let out = probe(&stub, &fn_sign(), "abc-123").await;
        match out {
            ProbeOutcome::TransportRetry { reason } => {
                assert!(reason.contains("connection reset"));
            }
            other => panic!("expected TransportRetry, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn probe_decode_escalate_on_decode_error() {
        let stub = StubChannel {
            last_chk_response: Err(DpsError::Decode("bad proto bytes".into())),
        };
        let out = probe(&stub, &fn_sign(), "abc-123").await;
        match out {
            ProbeOutcome::DecodeEscalate { reason } => {
                assert!(reason.contains("bad proto bytes"));
            }
            other => panic!("expected DecodeEscalate, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn probe_unexpected_on_non_query_dps_error() {
        // L4 fix: surface unexpected variants distinctly so operator
        // alerting can fire on the right signal.  Internal here stands
        // in for any variant that shouldn't surface from a query probe.
        let stub = StubChannel {
            last_chk_response: Err(DpsError::Internal("wrapper-side fault".into())),
        };
        let out = probe(&stub, &fn_sign(), "abc-123").await;
        match out {
            ProbeOutcome::Unexpected { dps_error } => {
                assert!(
                    dps_error.contains("wrapper-side fault"),
                    "Unexpected.dps_error should carry the formatted error: {dps_error}"
                );
            }
            other => panic!("expected Unexpected, got {other:?}"),
        }
    }
}
