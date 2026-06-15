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
    /// Terminal `ACK` without a non-empty DPS `server_fiscal_no`.
    AckWithoutServerFiscalNo { document_id_hex: String },
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
}

fn hex32_opt(b: &Option<Vec<u8>>) -> String {
    match b {
        Some(v) => hex_encode_lower(v),
        None => "<none>".to_string(),
    }
}

/// Chain-walk row: `(lnd, state, previous_hash, unsigned_xml_sha256,
/// offline_fiscal_no)`.  `offline_fiscal_no` (M2-01) marks an offline-origin
/// doc, which issues at `OfflineLocalAck` (not ACK) and so advances the seed
/// from that state onward.
type ChainRow = (i64, String, Option<Vec<u8>>, Option<Vec<u8>>, Option<i64>);

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

    // 2. No quiescent SENDING (Pattern B intent marker must not rest).
    let sending: Vec<(String,)> = sqlx::query_as(
        "SELECT lower(hex(document_id)) FROM fiscal_documents WHERE state = 'SENDING'",
    )
    .fetch_all(pool)
    .await?;
    for (document_id_hex,) in sending {
        out.push(Violation::StuckSending { document_id_hex });
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
            "SELECT lnd, state, previous_hash, unsigned_xml_sha256, offline_fiscal_no \
             FROM fiscal_documents \
             WHERE fiscal_number = ? AND unsigned_xml_sha256 IS NOT NULL \
             ORDER BY lnd ASC",
        )
        .bind(&fiscal_number)
        .fetch_all(pool)
        .await?;
        let mut expected: Option<Vec<u8>> = None;
        for (lnd, state, previous_hash, unsigned_sha, offline_fiscal_no) in docs {
            if previous_hash != expected {
                out.push(Violation::ChainBreak {
                    fiscal_number: fiscal_number.clone(),
                    lnd,
                    expected_hex: hex32_opt(&expected),
                    found_hex: hex32_opt(&previous_hash),
                });
            }
            // M2-01 (Fable-locked): the seed advances once per ISSUED doc.
            // Online docs issue at ACK; offline-origin docs (offline_fiscal_no
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
            // Online-origin docs are unchanged (they only ever issue at ACK).
            // AUD-L6-1 (2026-06-14): the offline-issued state set is now the
            // single-source-of-truth const `OFFLINE_ISSUED_STATES` (shared with
            // `last_issued_unsigned_xml_sha256`, the boot seed projection) so the
            // walk's final `expected` and the boot projection CANNOT diverge.
            let issued = state == "ACK"
                || (offline_fiscal_no.is_some()
                    && crate::db::repositories::fiscal_documents::OFFLINE_ISSUED_STATES
                        .contains(&state.as_str()));
            if issued {
                expected = unsigned_sha;
            }
        }
        if node_seed != expected {
            out.push(Violation::ChainSeedMismatch {
                fiscal_number,
                walk_hex: hex32_opt(&expected),
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
