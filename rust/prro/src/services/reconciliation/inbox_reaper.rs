//! RS-3 inbox-reaper — stale-`NEW`/`PROCESSING` inbox sweep.
//!
//! Closes ruling-2 (the 202-hang from a structural-breach `PROCESSING` row with
//! no terminal ledger doc) + audit finding-1 (a pre-lease binding failure leaves
//! the row `NEW`-forever). It converges those stuck rows to a terminal inbox
//! status so a replay answers an honest terminal instead of `202 IN_PROGRESS`
//! forever.
//!
//! **D-R1 — TERMINALISE-ONLY, NEVER re-drive.** The reaper NEVER re-fiscalizes a
//! stuck row. A cashier who saw a visible failure re-rings the receipt with a
//! NEW idempotency key by hand; an auto re-drive behind their back would phantom
//! a SECOND fiscal check for ONE physical sale (the idempotency key stops a
//! same-key dupe, not a re-ring dupe). Fiscal reality > liveness: the client
//! gets an honest terminal `INBOX_REJECTED` and re-submits with a new key. The
//! A-H1 identity fields stay on the row — the re-drive path is not burned, just
//! not built here.
//!
//! **D-R2 — inbox + audit ONLY.** The reaper writes ONLY `ingress_inbox.status`
//! (+ `audit_log`). ZERO writes to `fiscal_documents` / `node_state` / `shifts`
//! → the per-FN write gate is NOT needed (invariant #2 untouched), the document
//! state machine is never entered (invariant #8), and there is no schema change
//! (the statuses already exist).
//!
//! **D-R3 — SSOT classification.** Each row's doc is classified with the SAME
//! [`is_accepted`] / [`is_terminally_failed`] fns `resolve_replay` uses — the
//! reaper and replay cannot diverge by construction.
//!
//! **Age-gate.** The boot sweep passes `min_age = None` — it runs AFTER boot
//! reconciliation and BEFORE `bind_rest_ingress`/`serve`, so no live fiscalize
//! can race it. The runtime tick passes `min_age = Some(REAPER_STALE_THRESHOLD)`
//! — only rows whose lease/arrival is older than the threshold are touched, so a
//! live in-flight fiscalize (leased seconds ago, doc not yet minted) is never
//! reaped mid-flight. The per-row CAS guard (`WHERE status = <prior>`) makes a
//! lost race a clean no-op.

use std::fmt::Write as _;
use std::time::Duration;

use sqlx::SqlitePool;

use crate::db::models::enums::{DocState, Severity};
use crate::db::repositories::{audit_log, fiscal_documents, ingress_inbox};
use crate::db::tx::with_immediate;
use crate::runtime::ingress::replay::{is_accepted, is_terminally_failed};

/// Stale threshold for the RUNTIME tick.  A `NEW`/`PROCESSING` row whose
/// lease/arrival (`COALESCE(processed_at, received_at)`) is older than this with
/// no terminal-or-in-flight doc is reaped.  Hardcoded — NO config knob (D6).
pub const REAPER_STALE_THRESHOLD: Duration = Duration::from_secs(15 * 60);

/// How often the supervisor runtime tick runs the age-gated sweep.  Hardcoded —
/// NO config knob (D6).  A stale row is reaped within one tick after it crosses
/// [`REAPER_STALE_THRESHOLD`].
pub const REAPER_TICK_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Per-sweep outcome counters (operator log surface).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ReaperSummary {
    /// Candidate rows the age-gated SELECT returned this sweep.
    pub scanned: usize,
    /// `NEW`/`PROCESSING` + NO doc → `REJECTED` (Critical audit) — the
    /// ruling-2 / finding-1 core (a structural breach or pre-lease failure).
    pub no_doc_terminalised: usize,
    /// `PROCESSING` + accepted doc (ACK / OFFLINE_LOCAL_ACK) → `DONE` (Warning)
    /// — accounting convergence for a finalize that crashed pre inbox-DONE.
    pub accepted_converged_done: usize,
    /// row + terminally-failed doc → `REJECTED` (Warning).
    pub failed_terminalised: usize,
    /// row + IN-FLIGHT doc → UNTOUCHED (recon owns the doc; zero writes).
    pub in_flight_skipped: usize,
    /// The CAS affected zero rows — the row's status changed under us (a live
    /// worker won the race).  No-op, not an error.
    pub raced: usize,
}

/// What the reaper will do to one stale row (computed OUTSIDE the tx from the
/// SSOT classification, then applied under a CAS guard inside a small tx).
struct ReapAction {
    /// `true` → CAS to `DONE` (accepted doc); `false` → CAS to `REJECTED`.
    to_done: bool,
    /// The event_type + severity for the audit row written iff the CAS marks.
    event: &'static str,
    severity: Severity,
    /// The doc's state for the audit payload (`None` = no doc).
    doc_state: Option<DocState>,
}

pub async fn sweep(pool: &SqlitePool, min_age: Option<Duration>) -> anyhow::Result<ReaperSummary> {
    // The age modifier: `None` (boot) → `+0 seconds` (all NEW/PROCESSING rows,
    // safe because boot runs before `serve` — no live fiscalize); `Some(d)`
    // (runtime tick) → only rows whose lease/arrival is older than `d`.
    let modifier = match min_age {
        Some(d) => format!("-{} seconds", d.as_secs()),
        None => "+0 seconds".to_string(),
    };

    // SELECT-first: candidate rows. Age anchor is `processed_at` (stamped by
    // `acquire_lease` at NEW→PROCESSING) with `received_at` fallback for a
    // still-`NEW` row (which has no `processed_at`).
    let rows: Vec<(Vec<u8>, String)> = sqlx::query_as(
        "SELECT request_id, status FROM ingress_inbox \
         WHERE status IN ('NEW','PROCESSING') \
           AND COALESCE(processed_at, received_at) <= datetime('now', ?)",
    )
    .bind(&modifier)
    .fetch_all(pool)
    .await?;

    let mut summary = ReaperSummary {
        scanned: rows.len(),
        ..Default::default()
    };

    for (rid_vec, prior_status) in rows {
        let Ok(request_id) = <[u8; 16]>::try_from(rid_vec.as_slice()) else {
            // Malformed request_id blob (schema CHECK makes this unreachable).
            continue;
        };

        // Doc-join + SSOT classification (D-R3 — the SAME truth `resolve_replay`
        // answers with).  The read is pool-bound (outside any write-tx, INV #1);
        // a between-read-and-CAS change is caught by the CAS guard below.
        let doc = fiscal_documents::terminal_outcome_by_request_id(pool, &request_id).await?;

        let action = match &doc {
            // IN-FLIGHT (neither accepted nor terminally-failed) → UNTOUCHED.
            // Recon owns the doc; a terminalise here would lie (a durably-`Sent`
            // doc must NOT replay as failed).  ZERO writes — load-bearing.
            Some(o) if !is_accepted(o.state) && !is_terminally_failed(o.state) => {
                summary.in_flight_skipped += 1;
                continue;
            }
            // Accepted (ACK / OFFLINE_LOCAL_ACK) → DONE (accounting convergence
            // for a finalize that crashed before inbox-DONE).
            Some(o) if is_accepted(o.state) => ReapAction {
                to_done: true,
                event: "INBOX_REAPER_ACCEPTED_CONVERGED_DONE",
                severity: Severity::Warning,
                doc_state: Some(o.state),
            },
            // Terminally-failed doc → REJECTED.
            Some(o) => ReapAction {
                to_done: false,
                event: "INBOX_REAPER_FAILED_DOC_TERMINALISED",
                severity: Severity::Warning,
                doc_state: Some(o.state),
            },
            // No doc at all → REJECTED (Critical) — the ruling-2 / finding-1
            // core: a structural-breach `PROCESSING` or a pre-lease-failed `NEW`.
            None => ReapAction {
                to_done: false,
                event: "INBOX_REAPER_NO_DOC_TERMINALISED",
                severity: Severity::Critical,
                doc_state: None,
            },
        };

        let marked = apply_reap(pool, request_id, &prior_status, action).await?;
        if !marked {
            // The CAS matched zero rows — the status changed under us (a live
            // worker won the race, or a concurrent sweep already reaped it).
            summary.raced += 1;
            continue;
        }
        match &doc {
            None => summary.no_doc_terminalised += 1,
            Some(o) if is_accepted(o.state) => summary.accepted_converged_done += 1,
            Some(_) => summary.failed_terminalised += 1,
        }
    }

    Ok(summary)
}

/// Apply one reap under a CAS guard, atomically with its audit row (D-R2: the
/// ONLY writes are `ingress_inbox.status` + `audit_log`; INV #1 short tx).
/// Returns whether the guarded CAS actually marked a row.
async fn apply_reap(
    pool: &SqlitePool,
    request_id: [u8; 16],
    prior_status: &str,
    action: ReapAction,
) -> anyhow::Result<bool> {
    let from_new = prior_status == "NEW";
    let entity = hex_lower(&request_id);
    let payload = format!(
        r#"{{"request_id":"{}","prior_status":"{}","doc_state":{}}}"#,
        entity,
        prior_status,
        match action.doc_state {
            Some(d) => format!(r#""{}""#, d.as_str()),
            None => "null".to_string(),
        }
    );
    let ReapAction {
        to_done,
        event,
        severity,
        ..
    } = action;

    with_immediate(pool, move |tx| {
        Box::pin(async move {
            let marked = if to_done {
                ingress_inbox::mark_done_tx(tx, &request_id).await?
            } else if from_new {
                ingress_inbox::mark_rejected_if_new_tx(tx, &request_id).await?
            } else {
                ingress_inbox::mark_rejected_if_processing_tx(tx, &request_id).await?
            };
            if marked {
                audit_log::append_tx(
                    tx,
                    "ingress_inbox",
                    &entity,
                    event,
                    severity,
                    None,
                    Some(&payload),
                )
                .await?;
            }
            Ok::<bool, anyhow::Error>(marked)
        })
    })
    .await
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}
