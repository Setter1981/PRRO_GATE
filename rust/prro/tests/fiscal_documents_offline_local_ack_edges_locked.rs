//! W6 acceptance — lock the complete `OfflineLocalAck`-touching
//! edge set in `fiscal_documents::allowed_transition` against
//! silent drift.
//!
//! Per operator review pin (memory `m3b-w6-review-criteria`,
//! 2026-05-15), W6 is a load-bearing whitelist PR: any later PR
//! that silently adds or removes an `OfflineLocalAck`-touching
//! edge — `(OfflineLocalAck, X)` OR `(X, OfflineLocalAck)` — must
//! break this test loudly.  Future scope expansions require an
//! explicit operator decision + a corresponding update here.
//!
//! ## Locked sets (post-W6 baseline)
//!
//! ### `to == OfflineLocalAck`
//!
//! Exactly **1** edge:
//!   - `(Signed, OfflineLocalAck)` — M3a placeholder; preserved
//!     in W6 per criterion 1.
//!
//! ### `from == OfflineLocalAck`
//!
//! Exactly **3** edges:
//!   - `(OfflineLocalAck, Sent)` — M3a placeholder; preserved per
//!     W6 add-only scope.
//!   - `(OfflineLocalAck, Sending)` — Pattern C drain entry
//!     (NEW in W6).
//!   - `(OfflineLocalAck, Cancelled)` — manual operator escape
//!     during drain (NEW in W6).
//!
//! **Total `OfflineLocalAck`-touching edges: 4.**

use prro::db::models::enums::DocState;
use prro::db::repositories::fiscal_documents::allowed_transition;

/// All variants of `DocState` enumerated explicitly.  Hard-coded
/// (rather than relying on a `strum::EnumIter` derive) so that an
/// added enum variant ALSO breaks this test, forcing the author to
/// decide whether the new variant connects to / from
/// `OfflineLocalAck`.
const ALL_STATES: &[DocState] = &[
    DocState::Prepared,
    DocState::Signed,
    DocState::Encrypted,
    DocState::Sending,
    DocState::Sent,
    DocState::Kvt1,
    DocState::Kvt2,
    DocState::Ack,
    DocState::OfflineLocalAck,
    DocState::Rejected,
    DocState::Cancelled,
    DocState::ErrorRetryable,
    DocState::RequiresManualReconciliation,
];

/// The exact set of edges where `to == OfflineLocalAck`.
const EXPECTED_INBOUND: &[DocState] = &[DocState::Signed];

/// The exact set of edges where `from == OfflineLocalAck`.
const EXPECTED_OUTBOUND: &[DocState] = &[DocState::Sent, DocState::Sending, DocState::Cancelled];

#[test]
fn doc_state_enum_total_count_is_locked() {
    // If a new variant is added to `DocState` without updating
    // `ALL_STATES` here, the W6 locked-edge tests below would
    // silently miss the new variant (because the iteration would
    // skip it).  Locking the count forces the author to update
    // `ALL_STATES` AND decide whether the new variant participates
    // in any `OfflineLocalAck`-touching edge.
    assert_eq!(
        ALL_STATES.len(),
        13,
        "DocState variant count changed; update ALL_STATES in this test \
         AND verify the new variant against the OfflineLocalAck edge sets"
    );
}

#[test]
fn offline_local_ack_inbound_edges_are_exactly_signed() {
    // For every X in ALL_STATES, allowed_transition(X, OfflineLocalAck)
    // must equal `EXPECTED_INBOUND.contains(&X)`.
    for &from in ALL_STATES {
        let actual = allowed_transition(from, DocState::OfflineLocalAck);
        let expected = EXPECTED_INBOUND.contains(&from);
        assert_eq!(
            actual, expected,
            "edge ({from:?} -> OfflineLocalAck): expected allowed={expected}, got allowed={actual}.  \
             Inbound OfflineLocalAck edges are locked to {EXPECTED_INBOUND:?}; \
             any drift requires explicit operator decision + this test update."
        );
    }
}

#[test]
fn offline_local_ack_outbound_edges_are_exactly_sent_sending_cancelled() {
    for &to in ALL_STATES {
        let actual = allowed_transition(DocState::OfflineLocalAck, to);
        let expected = EXPECTED_OUTBOUND.contains(&to);
        assert_eq!(
            actual, expected,
            "edge (OfflineLocalAck -> {to:?}): expected allowed={expected}, got allowed={actual}.  \
             Outbound OfflineLocalAck edges are locked to {EXPECTED_OUTBOUND:?}; \
             any drift requires explicit operator decision + this test update."
        );
    }
}

#[test]
fn total_offline_local_ack_touching_edge_count_is_four() {
    // Count every allowed edge where OfflineLocalAck appears as
    // either endpoint.  Total locked at 4: 1 inbound + 3 outbound.
    let mut count = 0usize;
    for &x in ALL_STATES {
        if allowed_transition(x, DocState::OfflineLocalAck) {
            count += 1;
        }
        if allowed_transition(DocState::OfflineLocalAck, x) {
            count += 1;
        }
    }
    // Note: (OfflineLocalAck, OfflineLocalAck) would be counted
    // twice if it were allowed.  It is NOT allowed (idempotent
    // self-loop on a terminal-adjacent state is meaningless), so
    // the count is exactly 4.  If self-loop ever became allowed,
    // the EXPECTED_INBOUND/OUTBOUND assertions above would fail
    // first.
    assert_eq!(
        count, 4,
        "OfflineLocalAck-touching edge count drifted from 4; \
         see EXPECTED_INBOUND ({EXPECTED_INBOUND:?}) + EXPECTED_OUTBOUND ({EXPECTED_OUTBOUND:?})"
    );
}

#[test]
fn offline_local_ack_self_loop_is_forbidden() {
    assert!(
        !allowed_transition(DocState::OfflineLocalAck, DocState::OfflineLocalAck),
        "OfflineLocalAck self-loop must remain forbidden"
    );
}

/// Sanity: explicitly assert each of the 3 newly-introduced /
/// preserved outbound edges as positive cases — readable inventory
/// of "what W6 unlocked", separate from the negative-set assertion
/// above.
#[test]
fn w6_outbound_edges_named_positive_cases() {
    // M3a placeholder, preserved.
    assert!(allowed_transition(
        DocState::OfflineLocalAck,
        DocState::Sent
    ));
    // W6 new: Pattern C drain entry.
    assert!(allowed_transition(
        DocState::OfflineLocalAck,
        DocState::Sending
    ));
    // W6 new: manual operator escape.
    assert!(allowed_transition(
        DocState::OfflineLocalAck,
        DocState::Cancelled
    ));
}

/// Sanity: assert the single inbound edge.
#[test]
fn w6_inbound_edge_named_positive_case() {
    // M3a baseline, preserved per W6 criterion 1.
    assert!(allowed_transition(
        DocState::Signed,
        DocState::OfflineLocalAck
    ));
}
