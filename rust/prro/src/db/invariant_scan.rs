//! Invariant scan — SQL post-conditions over the fiscal ledger.
//!
//! Test/ops tooling (audit pass-2, 2026-06-11): after ANY scenario — a
//! targeted test, a kill-point run, a soak, a chaos session — one call
//! checks the WHOLE set of ledger-level legal invariants at once, instead
//! of each test re-asserting the subset it remembered to ask about:
//!
//!   1. `lnd` unique per FN (no double issuance; drift-guard for the
//!      `ux_fd_fn_lnd` index surviving future migrations).
//!   2. No doc resting in `SENDING` — Pattern B's intent-marker must
//!      never be a quiescent state (boot recovery downgrades it).
//!   3. Every `ACK` carries a non-empty `server_fiscal_no` AND a
//!      persisted `KVT1_RAW` evidence blob (HIGH-C5-2).
//!   4. The MAC chain holds: walking signed docs by `lnd`, each doc's
//!      `previous_hash` equals the seed left by the previous ACK; the
//!      final seed equals `node_state.last_known_unsigned_xml_sha256`.
//!   5. A terminally-failed inbox row (`REJECTED` or `ERROR`) never
//!      coexists with an ACCEPTED ledger doc for the same `request_id`
//!      (replay would short-circuit `Failed` for a fiscalized receipt —
//!      the AUD-1 hazard).
//!   6. Offline codes are consistent: consumption is all-or-nothing
//!      (`consumed_at` ⇔ `consumed_by_document_id`), every doc-side
//!      `offline_fiscal_no` is backed by a code row consumed by THAT
//!      doc, and no two docs share one `(fn, offline_fiscal_no)`.
//!
//! **Quiescence contract**: the scan describes a system AT REST. Run it
//! after scenarios complete (drain finished, no in-flight worker). A
//! scenario that deliberately leaves in-flight state filters the report
//! via [`scan`]'s returned list rather than using [`assert_clean`].
//!
//! Read-only; never inside a `with_immediate` (invariant #1).

#![cfg(any(test, feature = "test-support"))]

use sqlx::SqlitePool;

use crate::services::write_path::types::hex_encode_lower;

/// One detected breach of a ledger-level invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// Two+ fiscal documents share one `(fiscal_number, lnd)`.
    DuplicateLnd {
        fiscal_number: String,
        lnd: i64,
        count: i64,
    },
    /// A doc rests in `SENDING` — the Pattern B intent marker leaked
    /// into quiescence (boot recovery must downgrade it).
    StuckSending { document_id_hex: String },
    /// A doc rests in a PRE-SEND pipeline state {`PREPARED`, `SIGNED`,
    /// `ENCRYPTED`} at a QUIESCENT boundary — a leaked write-path artifact.
    /// The inline pass must flow such a doc to SENDING / OFFLINE_LOCAL_ACK, or
    /// (on a pre-issuance refusal) to the `Aborted` terminal, within ONE
    /// transaction-bounded pass; it must NEVER rest here.  Durable enforcement
    /// of the ledger-only pin — the pre-send extension of [`StuckSending`].
    /// (`ERROR_RETRYABLE` / `SENT` / `KVT1` / `KVT2` / `OFFLINE_LOCAL_ACK` are
    /// EXCLUDED — they legitimately rest: retry-parked / awaiting DPS /
    /// offline-issued.)
    StuckNonTerminalDoc {
        document_id_hex: String,
        state: String,
    },
    /// Terminal `ACK` without a non-empty DPS `server_fiscal_no`.
    AckWithoutServerFiscalNo { document_id_hex: String },
    /// **A.3 PR-C (D3 backstop below ACK, AUD-L2-F4)** — an ONLINE-origin doc
    /// (`offline_fiscal_no IS NULL`) resting in an ISSUED-lane state
    /// `SENT`/`KVT1`/`KVT2` with a NULL/empty `server_fiscal_no`.  Under
    /// advance-at-SEND the sfn is stamped at the `Sending → Sent` CAS, so an
    /// issued-lane online row MUST carry it (D3, [`fiscal_documents::is_issued`]);
    /// a missing sfn means the row claims an issued state it never reached.  The
    /// existing `AckWithoutServerFiscalNo` (3a) owns the ACK tip; this extends
    /// the guard down the SENT/KVT1/KVT2 lane.
    OnlineIssuedStateWithoutServerFiscalNo {
        document_id_hex: String,
        state: String,
    },
    /// Terminal `ACK` without a persisted `KVT1_RAW` evidence blob.
    AckWithoutKvt1Raw { document_id_hex: String },
    /// A signed doc's `previous_hash` does not extend the chain seed
    /// left by the previous ACK at that point of the lnd walk.
    ChainBreak {
        fiscal_number: String,
        lnd: i64,
        expected_hex: String,
        found_hex: String,
    },
    /// The walk's final seed differs from
    /// `node_state.last_known_unsigned_xml_sha256`.
    ChainSeedMismatch {
        fiscal_number: String,
        walk_hex: String,
        node_state_hex: String,
    },
    /// Terminally-failed inbox row (`REJECTED` or `ERROR`) + ACCEPTED
    /// (`ACK`/`OFFLINE_LOCAL_ACK`) ledger doc for the same `request_id` —
    /// replay would lie `Failed` about a fiscalized receipt.  The status
    /// set mirrors `runtime/ingress/replay.rs:153`'s short-circuit set
    /// (`"REJECTED" | "ERROR"`) — AUD-L8-2 widened this check from
    /// `REJECTED`-only so the scan is not blind to the `ERROR` half of the
    /// replay-lie hazard.
    RejectedInboxWithAcceptedDoc {
        request_id_hex: String,
        doc_state: String,
    },
    /// Offline code half-consumed: `consumed_at` and
    /// `consumed_by_document_id` must be both set or both NULL.
    OfflineCodeHalfConsumed {
        fiscal_number: String,
        code_lnd: i64,
    },
    /// A doc carries `offline_fiscal_no` with no code row consumed by
    /// THAT doc — the offline number is unbacked.
    OfflineFiscalNoUnbacked { document_id_hex: String },
    /// Two+ docs share one `(fiscal_number, offline_fiscal_no)` — an
    /// offline fiscal number was issued twice.
    DuplicateOfflineFiscalNo {
        fiscal_number: String,
        offline_fiscal_no: i64,
        count: i64,
    },
    /// M2-05 / M2-X3: an offline-origin doc (`offline_fiscal_no` NOT NULL) in any
    /// DRAIN-COHORT state with NULL `offline_session_id` — machine-enforces
    /// `backlog_drain.rs`'s "W7 always stamps offline_session_id" assumption;
    /// such a doc is invisible to the session-scoped drain cohort (silent backlog
    /// leak).  Widened (M2-X3, 2026-06-12) from `OFFLINE_LOCAL_ACK`-only to the
    /// full cohort state set, hence the rename from `…OfflineLocalAckWithout…`.
    OfflineOriginWithoutSession { document_id_hex: String },
    /// **SW-5b (M1-M2 cross-class re-pass, 2026-06-15)** — Mirror-1 desync:
    /// `node_state.shift_state` != the active `shifts.state` for
    /// `current_shift_id`.  The m3b §5 load-bearing mirror invariant
    /// (`node_state.shift_state` MUST equal the active shift row's state) — this
    /// closes the only uncovered load-bearing mirror (Mirror-2 = check 6d,
    /// Mirror-3 = check 5).  Defense-in-depth + forward-compat: catches a desync
    /// when the §16.7 operator-force seam (SW-5a) is wired.
    ShiftStateMirrorDrift {
        fiscal_number: String,
        node_state_shift_state: String,
        shifts_state: String,
    },
    /// A′.3 PR-O3 STOP-O3-1 fix (c) — a pending-drain shift
    /// (`OpenedLocalPendingDrain` / `ClosingLocalPendingDrain`) whose ANCHOR
    /// lifecycle doc is dead: the offline `SHIFT_OPEN` (for OLPD) or the offline
    /// `SHIFT_CLOSE` / `Z_REPORT` (for CLPD) is present only in a terminal
    /// non-issued state (`ABORTED` / `REJECTED` / `CANCELLED`), with NO live
    /// anchor (a `PREPARED` / `SIGNED` mid-flight doc, or an issued
    /// `OFFLINE_LOCAL_ACK` / `ACK`).  Such a shift can never fire its only exit
    /// (edge 5 / 13 needs the anchor to drain) — the orphan wedge fix (b) /
    /// boot SW-2 escalate to `RequiresManualReconciliation`.  DETECTOR ONLY
    /// (read-only): the auto-heal lives in the runtime / boot escalation, not
    /// here; this makes the pre-escalation window (best-effort runtime failure
    /// before the boot re-run) OBSERVABLE at a quiescent boundary.
    OrphanedPendingDrainShift {
        fiscal_number: String,
        shift_id_hex: String,
        state: String,
    },
    /// **L0 INV-21 — cash-anchor drift**.  The opening `cash_balance_kop` stored
    /// in the current open shift's row diverges from the re-derived carry
    /// (re-derived by re-walking all prior closed shifts' receipts).  This can
    /// only happen if the carry was written incorrectly at open-time or if a
    /// closed shift's `cash_balance_kop` was tampered with.
    ///
    /// `stored` = the shift row's `cash_balance_kop`; `re_derived` = the value
    /// computed from the journal.  DETECTOR ONLY (report + alert; do NOT auto-heal
    /// — drift here means the journal and the stored anchor disagree, requiring
    /// human investigation).
    CashAnchorDrift {
        fiscal_number: String,
        stored: i64,
        re_derived: i64,
    },
}

fn hex32_opt(b: &Option<Vec<u8>>) -> String {
    match b {
        Some(v) => hex_encode_lower(v),
        None => "<none>".to_string(),
    }
}

/// Chain-walk row: `(lnd, state, previous_hash, unsigned_xml_sha256,
/// offline_fiscal_no, server_fiscal_no, chain_superseded_at)`.  `offline_fiscal_no`
/// (M2-01) marks an offline-origin doc, which issues at `OfflineLocalAck` (not ACK)
/// and so advances the seed from that state onward.  `chain_superseded_at` (migration
/// 039) marks a doc whose MAC contribution was REWOUND away by a `NotAcceptedOffline`
/// completion — historical-issued, but NOT the active chain tip (excluded from the walk).
type ChainRow = (
    i64,
    String,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    Option<i64>,
    Option<String>,
    Option<String>,
);

/// Run every check; return ALL violations found (empty = clean).
pub async fn scan(pool: &SqlitePool) -> sqlx::Result<Vec<Violation>> {
    let mut out = Vec::new();

    // 1. lnd unique per FN (drift-guard for ux_fd_fn_lnd).
    let dup_lnd: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT fiscal_number, lnd, COUNT(*) FROM fiscal_documents \
         GROUP BY fiscal_number, lnd HAVING COUNT(*) > 1",
    )
    .fetch_all(pool)
    .await?;
    for (fiscal_number, lnd, count) in dup_lnd {
        out.push(Violation::DuplicateLnd {
            fiscal_number,
            lnd,
            count,
        });
    }

    // 2. No quiescent ORPHAN SENDING.  A `SENDING` doc is the Pattern-B intent marker; at a
    // quiescent boundary it must not rest — UNLESS it is a PROVEN operator HOLD: a consistent
    // (document, reservation) pair under a halted node.  CS-3 S7-1: the composed HOLD contract
    // durably rests an offline-origin reject / submitted-unknown doc in `SENDING` + `PENDING_APPLY`
    // pending operator completion.  Exempt ONLY the FULL relational witness — same document_id AND
    // fiscal_number, reservation `OUTCOME_OBSERVED` + `PENDING_APPLY`, that reservation IS the
    // node's ACTIVE, CURRENT-generation one, and the node is halted (STOP_MODE|BLOCKED).  This does
    // NOT weaken the bug-#192 guard; it makes it PRECISE.  Everything else still reds, in particular:
    // SENDING with no reservation; a `CALL_STARTED` (crash-mid-wire) reservation; a stale
    // generation / non-active pointer; a clean `Accepted` stalled pre-apply without STOP; an
    // arbitrary planted `PENDING_APPLY` not matching the active-generation witness.
    let sending: Vec<(String,)> = sqlx::query_as(
        "SELECT lower(hex(fd.document_id)) FROM fiscal_documents fd \
         WHERE fd.state = 'SENDING' \
           AND NOT EXISTS ( \
             SELECT 1 FROM delivery_reservation r \
             JOIN node_state ns ON ns.fiscal_number = r.fiscal_number \
             WHERE r.document_id = fd.document_id \
               AND r.fiscal_number = fd.fiscal_number \
               AND r.state = 'OUTCOME_OBSERVED' \
               AND r.apply_state = 'PENDING_APPLY' \
               AND r.reservation_id = ns.active_delivery_reservation_id \
               AND r.authorized_generation = ns.delivery_generation \
               AND ns.mode IN ('STOP_MODE', 'BLOCKED') )",
    )
    .fetch_all(pool)
    .await?;
    for (document_id_hex,) in sending {
        out.push(Violation::StuckSending { document_id_hex });
    }

    // 2b. No quiescent PRE-SEND non-terminal {PREPARED, SIGNED, ENCRYPTED}.
    // Like StuckSending (2 above), these are legitimate ONLY while the inline
    // write-path is mid-pass; at this quiescent boundary a resting one is a
    // leaked write-path artifact (the Aborted terminal closes the refusal path).
    // ERROR_RETRYABLE / SENT / KVT1 / KVT2 / OFFLINE_LOCAL_ACK legitimately rest
    // (retry-parked / awaiting DPS / offline-issued) and are EXCLUDED.
    let pre_send: Vec<(String, String)> = sqlx::query_as(
        "SELECT lower(hex(document_id)), state FROM fiscal_documents \
         WHERE state IN ('PREPARED','SIGNED','ENCRYPTED')",
    )
    .fetch_all(pool)
    .await?;
    for (document_id_hex, state) in pre_send {
        out.push(Violation::StuckNonTerminalDoc {
            document_id_hex,
            state,
        });
    }

    // 3a. ACK ⇒ non-empty server_fiscal_no.
    let ack_no_sfn: Vec<(String,)> = sqlx::query_as(
        "SELECT lower(hex(document_id)) FROM fiscal_documents \
         WHERE state = 'ACK' AND (server_fiscal_no IS NULL OR server_fiscal_no = '')",
    )
    .fetch_all(pool)
    .await?;
    for (document_id_hex,) in ack_no_sfn {
        out.push(Violation::AckWithoutServerFiscalNo { document_id_hex });
    }

    // 3a′. A.3 PR-C (D3 backstop below ACK, AUD-L2-F4) — an ONLINE-origin doc
    //      in an ISSUED-lane state SENT/KVT1/KVT2 MUST carry a non-empty
    //      server_fiscal_no (advance-at-SEND stamps it at the Sending→Sent CAS).
    //      `offline_fiscal_no IS NULL` scopes to online-origin (offline rows
    //      issue via offline_fiscal_no and legitimately carry NULL sfn).  3a
    //      above owns the ACK tip; this is the sub-ACK extension of the same
    //      D3 invariant (issued-lane online ⟺ server_fiscal_no set).
    let online_issued_no_sfn: Vec<(String, String)> = sqlx::query_as(
        "SELECT lower(hex(document_id)), state FROM fiscal_documents \
         WHERE state IN ('SENT','KVT1','KVT2') AND offline_fiscal_no IS NULL \
           AND (server_fiscal_no IS NULL OR server_fiscal_no = '')",
    )
    .fetch_all(pool)
    .await?;
    for (document_id_hex, state) in online_issued_no_sfn {
        out.push(Violation::OnlineIssuedStateWithoutServerFiscalNo {
            document_id_hex,
            state,
        });
    }

    // 3b. ACK ⇒ persisted KVT1_RAW evidence (HIGH-C5-2).
    let ack_no_kvt1: Vec<(String,)> = sqlx::query_as(
        "SELECT lower(hex(fd.document_id)) FROM fiscal_documents fd \
         WHERE fd.state = 'ACK' AND NOT EXISTS ( \
            SELECT 1 FROM document_files df \
            WHERE df.document_id = fd.document_id AND df.kind = 'KVT1_RAW')",
    )
    .fetch_all(pool)
    .await?;
    for (document_id_hex,) in ack_no_kvt1 {
        out.push(Violation::AckWithoutKvt1Raw { document_id_hex });
    }

    // 4. MAC chain walk per FN: every SIGNED doc's previous_hash must
    //    equal the seed left by the previous ACK; ACKs advance the seed;
    //    the final seed must equal node_state's projection.
    let fns: Vec<(String, Option<Vec<u8>>)> =
        sqlx::query_as("SELECT fiscal_number, last_known_unsigned_xml_sha256 FROM node_state")
            .fetch_all(pool)
            .await?;
    for (fiscal_number, node_seed) in fns {
        let docs: Vec<ChainRow> = sqlx::query_as(
            "SELECT lnd, state, previous_hash, unsigned_xml_sha256, offline_fiscal_no, \
                    server_fiscal_no, chain_superseded_at \
             FROM fiscal_documents \
             WHERE fiscal_number = ? AND unsigned_xml_sha256 IS NOT NULL \
             ORDER BY lnd ASC",
        )
        .bind(&fiscal_number)
        .fetch_all(pool)
        .await?;
        let mut expected: Option<Vec<u8>> = None;
        // The unsigned hash of the immediately-preceding row (any state, lnd ASC).  Used to verify the
        // internal continuity of the rewound-orphan sub-chain (a `chain_superseded_at` held doc + its
        // `CANCELLED` cohort): each dead doc chained onto its lnd-predecessor at sign time, so a corrupt
        // back-pointer THERE is still a real MAC break even though the doc is voided (audit MAJOR / bd
        // PRRO_GATE-2nk).  Distinct from `expected`, which is the LIVE (active) chain tip.
        let mut prev_row_unsigned: Option<Vec<u8>> = None;
        // bd PRRO_GATE-hpc — durable T=112 seed witnesses (`chain_seed_transitions`).  A replenish
        // advances the seed to a NON-DOCUMENT `Hs` with no ledger row, so a document issued AFTER it
        // legitimately carries `previous_hash = Hs`.  Without re-anchoring, the walk's running
        // `expected` would still hold the PRE-replenish doc hash → a FALSE `ChainBreak` at that doc
        // (empirically reproduced).  Same shape as the 039 marker re-anchor below.  `lnd_at_write` is
        // the FN's `next_lnd` when the replenish ran (the ordinal the NEXT doc takes), so a witness
        // applies to a doc at `lnd >= lnd_at_write`; a doc that already advanced the tip past that
        // ordinal makes the witness stale (`last_advance_ordinal` guard).
        let witnesses: Vec<(i64, Vec<u8>)> = sqlx::query_as(
            "SELECT lnd_at_write, new_seed FROM chain_seed_transitions \
             WHERE fiscal_number = ? ORDER BY lnd_at_write ASC, created_at ASC, rowid ASC",
        )
        .bind(&fiscal_number)
        .fetch_all(pool)
        .await?;
        let mut last_advance_ordinal: i64 = i64::MIN;
        for (
            lnd,
            state,
            previous_hash,
            unsigned_sha,
            offline_fiscal_no,
            server_fiscal_no,
            chain_superseded_at,
        ) in docs
        {
            // A `chain_superseded_at` doc (migration 039) had its MAC contribution REWOUND away by a
            // `NotAcceptedOffline` completion — historical-issued, but NOT the active chain tip.  Its
            // `previous_hash` is the rewind target (possibly a NON-doc T=112 seed), so it is neither a
            // live link (no active-tip check) nor an advance of `expected` (the seed moved below it).
            // Keyed on the EXPLICIT marker, NOT the RMR state — a general RMR (Sent→NotFound / ER-drain
            // / MacReseed) is not superseded and stays in the active walk (bd PRRO_GATE-2nk).  It IS the
            // cohort's predecessor, so it sets `prev_row_unsigned` for the sub-chain check below.
            //
            // RE-ANCHOR the ACTIVE tip to the rewind target (external re-review MAJOR): the completion
            // rewound the seed to THIS doc's `previous_hash`, so the next ACTIVE doc must chain onto
            // THAT (a possibly-non-doc T=112 seed), NOT the pre-rewind tip.  Without re-anchoring,
            // `expected` would stay stuck at the last pre-marker issued hash → (a) a legit doc chaining
            // onto a non-doc rewind target would FALSE-`ChainBreak`, and worse (b) a doc FORKED off the
            // STALE pre-rewind predecessor would be silently accepted (a lost fork detection — the exact
            // fork this whole fix exists to make detectable).  The marker's OWN back-pointer is NOT
            // checked against `expected` here: a T=112 replenish advanced the seed to `previous_hash`
            // WITHOUT a doc, so that transition is invisible to the walk and cannot be validated from
            // the ledger — only re-anchored onto.
            if chain_superseded_at.is_some() {
                expected = previous_hash;
                last_advance_ordinal = lnd;
                prev_row_unsigned = unsigned_sha;
                continue;
            }
            // A `CANCELLED` doc is the dead `OFFLINE_LOCAL_ACK` cohort of the cohort-cancel: NOT a live
            // link (never checked against / advancing the active tip `expected`), but its `previous_hash`
            // must still chain onto its lnd-predecessor — a corrupt back-pointer in the voided sub-chain
            // is a real break (audit MAJOR).  `ABORTED` stays in the ACTIVE walk (never issued; its
            // `previous_hash` == the live tip, so a wrongly-chained aborted doc still breaks there).
            if state == "CANCELLED" {
                if previous_hash != prev_row_unsigned {
                    out.push(Violation::ChainBreak {
                        fiscal_number: fiscal_number.clone(),
                        lnd,
                        expected_hex: hex32_opt(&prev_row_unsigned),
                        found_hex: hex32_opt(&previous_hash),
                    });
                }
                prev_row_unsigned = unsigned_sha;
                continue;
            }
            // hpc: re-anchor onto the NEWEST T=112 witness that fired at/below this doc's ordinal
            // AND after the last tip advance (a witness the chain already passed must not re-apply).
            if let Some((w_ord, w_seed)) = witnesses
                .iter()
                .rfind(|(o, _)| *o <= lnd && *o > last_advance_ordinal)
            {
                expected = Some(w_seed.clone());
                last_advance_ordinal = *w_ord;
            }
            if previous_hash != expected {
                out.push(Violation::ChainBreak {
                    fiscal_number: fiscal_number.clone(),
                    lnd,
                    expected_hex: hex32_opt(&expected),
                    found_hex: hex32_opt(&previous_hash),
                });
            }
            // M2-01 (Fable-locked): the seed advances once per ISSUED doc.
            // Online-origin docs issue at SEND (A.3 advance-at-SEND — see the
            // A.3/C2 note below); offline-origin docs (offline_fiscal_no
            // NOT NULL) issue at OfflineLocalAck and stay "issued" through their
            // drain states — so they advance `expected` from OFFLINE_LOCAL_ACK
            // onward.  This validates the offline chain DURING the offline window
            // (closes M2-03) and stays correct mid-drain (no false ChainBreak
            // when doc#1 is at KVT2 and doc#2 chains off it).  The drain-state
            // set mirrors the drain cohort IN-set (fiscal_documents.rs) —
            // including ERROR_RETRYABLE: a transient send failure parks the doc
            // but it stays issued (M2-01 review F1; keep the two sets in
            // lockstep).
            //
            // **M2-N2b (architect-locked, 2026-06-13)**: REJECTED and
            // REQUIRES_MANUAL_RECONCILIATION are ALSO "issued" for offline-origin
            // docs.  Such a doc ALREADY advanced the local seed at OfflineLocalAck
            // (M2-01) and a successor chained off it (prev = hash(it)) BEFORE the
            // drain rejected / manual-escalated it.  Excluding these from the
            // walk's issued-set would NOT advance `expected` over the rejected
            // predecessor → a FALSE `ChainBreak` at the successor.  Ever-reached-
            // OfflineLocalAck ⇒ seed advanced, regardless of later drain outcome.
            // A.3 / C2 (design v3 §6 step 5): the issued-predicate is now the
            // shared [`fiscal_documents::is_issued`] fn — online-origin issues
            // when `server_fiscal_no` is set (advance-at-SEND, D3), NOT only at
            // `ACK`.  Without this the walk would advance `expected` only over
            // `ACK` docs while `node_state.seed` advanced at SEND → a FALSE
            // `ChainSeedMismatch` on any SENT-resting online doc.  Offline arm
            // unchanged (`OFFLINE_ISSUED_STATES`).  This walk and
            // `active_chain_tip_unsigned_xml_sha256` share `is_issued` → CANNOT
            // diverge on which docs advanced the seed.
            let issued = crate::db::repositories::fiscal_documents::is_issued(
                &state,
                offline_fiscal_no,
                server_fiscal_no.as_deref(),
            );
            if issued {
                expected = unsigned_sha.clone();
                last_advance_ordinal = lnd;
            }
            prev_row_unsigned = unsigned_sha;
        }
        // bd PRRO_GATE-2nk — the node seed must equal the ACTIVE chain tip, which after a
        // `NotAcceptedOffline` rewind is the held doc's own `previous_hash` (possibly a non-doc T=112
        // seed), NOT the last issued doc's hash.  Use the SAME projection recovery uses, so the scan
        // and NC-03 boot CANNOT diverge (the pre-fix `node_seed == walk's final expected` check
        // false-positived on the rewound tip).
        let active_tip =
            crate::db::repositories::fiscal_documents::active_chain_tip_unsigned_xml_sha256(
                pool,
                &fiscal_number,
            )
            .await?;
        if node_seed != active_tip {
            out.push(Violation::ChainSeedMismatch {
                fiscal_number,
                walk_hex: hex32_opt(&active_tip),
                node_state_hex: hex32_opt(&node_seed),
            });
        }
    }

    // 5. A terminally-failed inbox row (REJECTED or ERROR) must not coexist
    //    with an accepted ledger doc — replay would short-circuit `Failed`
    //    for a fiscalized receipt (the AUD-1 hazard).  The inbox-status set
    //    mirrors `runtime/ingress/replay.rs:153`'s short-circuit set
    //    (`"REJECTED" | "ERROR"`); keep the two in lockstep (AUD-L8-2 —
    //    same single-source discipline as OFFLINE_ISSUED_STATES / M2-N2b).
    let lie: Vec<(String, String)> = sqlx::query_as(
        "SELECT lower(hex(i.request_id)), fd.state \
         FROM ingress_inbox i \
         JOIN fiscal_documents fd ON fd.request_id = i.request_id \
         WHERE i.status IN ('REJECTED', 'ERROR') AND fd.state IN ('ACK', 'OFFLINE_LOCAL_ACK')",
    )
    .fetch_all(pool)
    .await?;
    for (request_id_hex, doc_state) in lie {
        out.push(Violation::RejectedInboxWithAcceptedDoc {
            request_id_hex,
            doc_state,
        });
    }

    // 6a. Offline-code consumption is all-or-nothing.
    let half: Vec<(String, i64)> = sqlx::query_as(
        "SELECT fiscal_number, code_lnd FROM offline_codes \
         WHERE (consumed_at IS NULL) != (consumed_by_document_id IS NULL)",
    )
    .fetch_all(pool)
    .await?;
    for (fiscal_number, code_lnd) in half {
        out.push(Violation::OfflineCodeHalfConsumed {
            fiscal_number,
            code_lnd,
        });
    }

    // 6b. Every doc-side offline_fiscal_no is backed by a code row
    //     consumed by THAT doc.
    let unbacked: Vec<(String,)> = sqlx::query_as(
        "SELECT lower(hex(fd.document_id)) FROM fiscal_documents fd \
         WHERE fd.offline_fiscal_no IS NOT NULL AND NOT EXISTS ( \
            SELECT 1 FROM offline_codes oc \
            WHERE oc.fiscal_number = fd.fiscal_number \
              AND oc.code_lnd = fd.offline_fiscal_no \
              AND oc.consumed_by_document_id = fd.document_id)",
    )
    .fetch_all(pool)
    .await?;
    for (document_id_hex,) in unbacked {
        out.push(Violation::OfflineFiscalNoUnbacked { document_id_hex });
    }

    // 6c. No two docs share one (fn, offline_fiscal_no) — an offline
    //     fiscal number issued twice (the DUR-1 double-consume scenario).
    let dup_off: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT fiscal_number, offline_fiscal_no, COUNT(*) FROM fiscal_documents \
         WHERE offline_fiscal_no IS NOT NULL \
         GROUP BY fiscal_number, offline_fiscal_no HAVING COUNT(*) > 1",
    )
    .fetch_all(pool)
    .await?;
    for (fiscal_number, offline_fiscal_no, count) in dup_off {
        out.push(Violation::DuplicateOfflineFiscalNo {
            fiscal_number,
            offline_fiscal_no,
            count,
        });
    }

    // 6d. M2-05 / M2-X3: no offline-origin doc in the DRAIN-COHORT state set with
    //     NULL offline_session_id — machine-enforce the drain's "W7 always stamps
    //     offline_session_id" assumption (such a doc is invisible to the
    //     session-scoped cohort).  Widened (M2-X3) from OFFLINE_LOCAL_ACK-only to
    //     the cohort set (mirrors `list_drain_candidates_for_fn_ordered_by_lnd`);
    //     `offline_fiscal_no IS NOT NULL` excludes online docs (which never carry
    //     a session) so no legitimate row trips it.
    let no_session: Vec<(String,)> = sqlx::query_as(
        "SELECT lower(hex(document_id)) FROM fiscal_documents \
         WHERE state IN ('OFFLINE_LOCAL_ACK','SENT','KVT1','ERROR_RETRYABLE','KVT2') \
           AND offline_session_id IS NULL AND offline_fiscal_no IS NOT NULL",
    )
    .fetch_all(pool)
    .await?;
    for (document_id_hex,) in no_session {
        out.push(Violation::OfflineOriginWithoutSession { document_id_hex });
    }

    // Mirror-1 (SW-5b — m3b §5 load-bearing): node_state.shift_state MUST equal
    // the active shifts.state for current_shift_id.  Closes the only uncovered
    // load-bearing mirror (Mirror-2 = check 6d, Mirror-3 = check 5).
    // `current_shift_id IS NOT NULL` excludes the NC-03 / bootstrap NULL-pointer
    // case (no legitimate at-rest row trips it); the scan runs on a quiescent
    // boundary where apply_shift_transition (RS-3 C1) has already synced both
    // sides atomically.
    let mirror_drift: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT ns.fiscal_number, ns.shift_state, s.state \
         FROM node_state ns \
         JOIN shifts s ON s.shift_id = ns.current_shift_id \
         WHERE ns.current_shift_id IS NOT NULL \
           AND ns.shift_state != s.state",
    )
    .fetch_all(pool)
    .await?;
    for (fiscal_number, node_state_shift_state, shifts_state) in mirror_drift {
        out.push(Violation::ShiftStateMirrorDrift {
            fiscal_number,
            node_state_shift_state,
            shifts_state,
        });
    }

    // 15. A′.3 PR-O3 fix (c) — orphaned pending-drain shift.  A shift resting in
    // OpenedLocalPendingDrain / ClosingLocalPendingDrain whose ANCHOR lifecycle
    // doc (SHIFT_OPEN for OLPD; SHIFT_CLOSE / Z_REPORT for CLPD) exists only in a
    // terminal non-issued state (ABORTED / REJECTED / CANCELLED) — i.e. NO live
    // anchor (PREPARED/SIGNED mid-flight, or issued OFFLINE_LOCAL_ACK/ACK).  Such
    // a shift can never fire edge 5 / 13; fix (b) / boot SW-2 escalate it to RMR,
    // so at a quiescent boundary this fires ONLY in the pre-escalation window
    // (a best-effort runtime failure before the boot re-run).  Detector only.
    let orphan_shifts: Vec<(String, Vec<u8>, String)> = sqlx::query_as(
        "SELECT s.fiscal_number, s.shift_id, s.state \
         FROM shifts s \
         WHERE s.state IN ('OPENED_LOCAL_PENDING_DRAIN', 'CLOSING_LOCAL_PENDING_DRAIN') \
           AND NOT EXISTS ( \
             SELECT 1 FROM fiscal_documents d \
             WHERE d.shift_id = s.shift_id \
               AND d.state NOT IN ('ABORTED', 'REJECTED', 'CANCELLED') \
               AND ( \
                     (s.state = 'OPENED_LOCAL_PENDING_DRAIN' AND d.doc_type = 'SHIFT_OPEN') \
                  OR (s.state = 'CLOSING_LOCAL_PENDING_DRAIN' \
                      AND d.doc_type IN ('SHIFT_CLOSE', 'Z_REPORT')) \
               ) \
           )",
    )
    .fetch_all(pool)
    .await?;
    for (fiscal_number, shift_id, state) in orphan_shifts {
        out.push(Violation::OrphanedPendingDrainShift {
            fiscal_number,
            shift_id_hex: hex_encode_lower(&shift_id),
            state,
        });
    }

    // 16. L0 cash-anchor drift — re-derive the open shift's opening anchor
    // from the journal (walking all closed shifts) and compare to the stored
    // `cash_balance_kop`.  Drift means either the carry was written incorrectly
    // at open-time or a closed-shift row was tampered with.
    //
    // Guard: skip FNs whose current shift state is NOT a "real open" (i.e. the
    // fuzzer's `SellWithClosedShift` helper uses `force_shift_closed` which
    // bypasses the closing-cash write → the closed-shift anchor is wrong for
    // that seam only).  We detect this by checking that the open shift's
    // `cash_balance_kop` matches the carry from the PRECEDING closed shifts
    // (not including the open shift itself).  `reconcile_opening_anchor`
    // already does this correctly: it returns `Some((re_derived, stored))`
    // where `stored` = open shift's opening anchor = carry from prior closed
    // shifts.  False-positives from the force-close seam only arise when the
    // CLOSED shift's anchor is wrong; the open-shift-opening check is
    // unaffected.
    //
    // To guard precisely against the known false-positive surface: the
    // force_shift_closed helper does NOT update `cash_balance_kop` on the
    // closed shift row.  After force-close that row carries the OPENING value
    // (not the closing value).  So the re-derived carry (from the closed
    // shift's docs + that opening) will be wrong.  We skip Check 16 when the
    // LATEST CLOSED shift has a zero `cash_balance_kop` AND it has non-zero
    // issued receipts — that is the exact force-close signature.
    //
    // In production the force-close seam does NOT exist; Check 16 runs on
    // all FNs unconditionally.
    let fns_for_check16: Vec<(String,)> =
        sqlx::query_as("SELECT DISTINCT fiscal_number FROM node_state")
            .fetch_all(pool)
            .await?;

    for (fiscal_number,) in fns_for_check16 {
        // Skip: last closed shift has cash_balance_kop=0 AND still holds documents the
        // re-derivation counts (the force-close test-seam signature — only present in tests).
        //
        // bd `PRRO_GATE-vuw` — this guard was doubly broken and BOTH halves are repaired here:
        //
        //  1. It keyed the "latest closed shift" on `s.serial = (SELECT MAX(serial) …)`. `serial`
        //     is the DEAD column of bd `PRRO_GATE-seb`: nothing writes it, so every row holds NULL,
        //     `MAX(serial)` is NULL, and `NULL = NULL` is NULL — never TRUE. The skip therefore
        //     COULD NOT FIRE. This was the last consumer of `serial` that bd `seb` missed; it now
        //     uses the same total order `cash_ledger::opening_carry_for_fn` settled on
        //     (`closed_at DESC, rowid DESC` — `closed_at` is written atomically with `state='CLOSED'`,
        //     `rowid` breaks the second-granular tie).
        //
        //  2. Its receipt probe asked `state IN ('ACK','OFFLINE_LOCAL_ACK')` while the thing it
        //     guards — `reconcile_opening_anchor` → `aggregate_shift_cash` — now selects
        //     [`counted_in_turnover`] (bd `PRRO_GATE-a6n`). A guard must ask the SAME question as
        //     the check it protects, or it stops recognising the very seam it exists for: a
        //     force-closed shift whose doc rests mid-drain was invisible to the probe yet counted by
        //     the re-derivation, producing a false `CashAnchorDrift`.
        //
        // In production the force-close seam does NOT exist (a shift reaches CLOSED only through the
        // Z path, which runs quiescence AND persists `derive_closing_cash`), so this skip stays inert
        // there and Check 16 runs unconditionally — as the comment above already promised.
        let latest_closed: Option<(Vec<u8>, i64)> = sqlx::query_as(
            "SELECT shift_id, cash_balance_kop FROM shifts \
             WHERE fiscal_number = ? AND state = 'CLOSED' \
             ORDER BY closed_at DESC, rowid DESC LIMIT 1",
        )
        .bind(&fiscal_number)
        .fetch_optional(pool)
        .await?;
        if let Some((shift_id_blob, cash_balance_kop)) = latest_closed {
            if cash_balance_kop == 0 {
                let rows: Vec<(String, Option<i64>, Option<String>)> = sqlx::query_as(
                    "SELECT state, offline_fiscal_no, server_fiscal_no FROM fiscal_documents \
                     WHERE fiscal_number = ? AND shift_id = ?",
                )
                .bind(&fiscal_number)
                .bind(&shift_id_blob)
                .fetch_all(pool)
                .await?;
                let counted_any = rows.iter().any(|(state, ofn, sfn)| {
                    crate::db::repositories::fiscal_documents::counted_in_turnover(
                        state,
                        *ofn,
                        sfn.as_deref(),
                    )
                });
                if counted_any {
                    // Force-close seam detected — skip to avoid false-positive.
                    continue;
                }
            }
        }

        if let Some((re_derived, stored)) =
            crate::services::cash_ledger::reconcile_opening_anchor(pool, &fiscal_number).await?
        {
            if re_derived != stored {
                out.push(Violation::CashAnchorDrift {
                    fiscal_number,
                    stored,
                    re_derived,
                });
            }
        }
    }

    Ok(out)
}

/// Convenience gate: panic with a readable report unless the scan is clean.
pub async fn assert_clean(pool: &SqlitePool) {
    let violations = scan(pool).await.expect("invariant scan query failed");
    assert!(
        violations.is_empty(),
        "ledger invariant scan found {} violation(s):\n{:#?}",
        violations.len(),
        violations
    );
}
