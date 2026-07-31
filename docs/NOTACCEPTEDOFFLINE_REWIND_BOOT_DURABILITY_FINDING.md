# FINDING (CONFIRMED, systemic) — `NotAcceptedOffline` seed rewind is NOT durable across NC-03 boot

**Date:** 2026-07-24 · **Severity:** recovery-correctness (MAC-seed integrity) · **bd:** PRRO_GATE-2nk
(upgraded from "invariant_scan not cohort-cancel-aware" — that scan false-positive was a SYMPTOM of
this root cause). **Status:** REPRODUCED empirically through the REAL runtime seams. Scan change
REVERTED (do not touch the scan until the coordinated projection fix is designed).

Discovered-from: the external adversarial audit (NO-GO) of the invariant_scan cohort-cancel PR.

---

## 1. The bug (one line)

After a `NotAcceptedOffline` completion durably rewinds the MAC seed to **H0** (the held doc's
`previous_hash`), an **NC-03 boot** (`node_state` row lost, ledger survives) reconstructs the seed as
**H1** (the held doc's OWN hash) — **undoing the durable rewind and resurrecting the rejected chain
tip**. The next document after the node is unblocked would chain from H1, not H0.

## 2. Root cause — the shared issued-tip projection is not cohort-cancel-aware

`reconstruct_lost_node_state` (`boot_phase.rs:1728`) projects the seed via
`last_issued_unsigned_xml_sha256` (`fiscal_documents.rs`), which fetches docs `ORDER BY lnd DESC` and
takes the FIRST `is_issued` one. `is_issued` counts `REQUIRES_MANUAL_RECONCILIATION` as issued
(`OFFLINE_ISSUED_STATES`, `fiscal_documents.rs:1215`). After the cohort-cancel the held predecessor is
RMR (issued) at the highest lnd among issued docs (its later cohort is CANCELLED = not issued) → the
projection picks the RMR doc's hash **H1**, not the rewind target **H0**.

The projection's own comment asserts *"this projection and the `invariant_scan` walk CANNOT diverge"*
(they share `is_issued`) — so **boot recovery AND the diagnostic scan are BOTH consumers of the same
projection**, and BOTH mishandle the rewound cohort-cancel state. This is why fixing the scan in
isolation is unsound (it would diverge from `last_issued`, breaking the documented invariant, and mask
this boot bug). The fix must be a SINGLE cohort-cancel-aware active-tip projection shared by all
consumers.

## 3. Empirical reproduction (REAL seams — `complete_operator_pending` + `reconstruct_lost_node_state`)

Added as a unit test in `boot_phase.rs`'s test module (in-crate: both seams reachable), RUN (RED
confirmed), and COMMITTED `#[ignore]`d on this branch — the ready-to-run RED pin the coordinated fix
un-ignores for its RED→GREEN.

**Result:** steps 1-3 PASS (the real completion produced the oc10/oc15-pinned durable state:
seed=H0, held→RMR, cohort→CANCELLED); step 6 FAILS —

```
assertion `left == right` failed:
NC-03 boot MUST preserve the NotAcceptedOffline rewind (seed=H0), not resurrect the RMR held doc's own hash (H1=…)
  left:  Some([0xB1; 32])   // H1 — the resurrected RMR held-doc hash (what boot recovered)
  right: Some([0xB0; 32])   // H0 — the durable rewind target (what it MUST be)
```

Repro test (verbatim — paste into `boot_phase.rs` `mod tests`, un-ignore, for the fix's RED→GREEN):

```rust
#[tokio::test]
async fn nc03_boot_undoes_not_accepted_offline_rewind() {
    use crate::db::models::ids::DocumentId;
    use crate::db::repositories::delivery_reservation::{
        authorize_submission, complete_operator_pending, resume_crashed_reservation,
        NewReservation, OperatorResolution, ReservationId,
    };
    use crate::db::tx::with_immediate;
    // seed_off(pool, fscl, byte, lnd, state, previous_hash, unsigned_sha, session) — offline SELL row.
    // (fn_config + node_state seed=0xEE + DRAINING session; pred lnd10 SENDING prev=H0 unsigned=H1;
    //  successors lnd11/12 OFFLINE_LOCAL_ACK.)
    // Held reservation: authorize_submission(new_res) + resume_crashed_reservation → PENDING_APPLY + STOP_MODE.
    // (2) complete_operator_pending(tx, res_id, OperatorResolution::NotAcceptedOffline).
    // (3) assert seed==H0, pred==RMR, successors==CANCELLED.   <-- these PASS (real completion is correct)
    // (4) DELETE FROM node_state WHERE fiscal_number=?.
    // (5) reconstruct_lost_node_state(&pool, fscl, None).
    // (6) assert recovered seed == H0.   <-- FAILS today: recovered = H1.
}
```
(The full ~120-line body — seeding helper `seed_off`, the two reservation seams, and the six numbered
steps — is COMMITTED `#[ignore]`d in `boot_phase.rs` `mod tests` on this branch
(`investigate/notacceptedoffline-rewind-boot-durability`). The shape above is the load-bearing structure.)

Reachability: requires `NotAcceptedOffline` (rare) THEN an NC-03 `node_state` loss (rare disaster).
Boot leaves the node **BLOCKED** (operator must clear), so it is NOT a silent auto-trade — but clearing
the block does NOT re-fix the seed; the wrong H1 is already persisted, and the next doc chains from H1.

## 4. Related consumers of the same projection (grep, for the coordinated fix's blast radius)

- **Boot NC-03 recovery** — `reconstruct_lost_node_state` / `boot_phase.rs:1728,1770` (THIS bug).
- **`invariant_scan` MAC-walk** — shares `is_issued`; the original scan false-positive (bd 2nk symptom).
- **`last_issued_unsigned_xml_sha256`** — the projection itself (`fiscal_documents.rs`).
- **Z-quiescence / fuzzer model / webcheck replay** — other `is_issued` consumers (verify each).
- **RMR is a general terminal** (audit BLOCKER 2): `Sent→RMR` on NotFound (`sent_not_found.rs:61`, keeps
  sfn → online is_issued=true), ErrorRetryable→RMR (`backlog_drain.rs:1694`), boot ER escalation
  (`boot_phase.rs:2921`), online `NotAccepted`/`MacReseed` completion (`delivery_reservation.rs:1446`).
  A `state == 'RMR'` string does NOT prove rewind provenance — any fix must key on a relational witness
  of a completed `NotAcceptedOffline` (FN/document/session/rewound-seed + a valid cancelled cohort), not
  the bare state.

## 5. Coordinated-fix design scope (SEPARATE effort — own spec / design / audit / review)

The fix must span, consistently, so scan == last_issued == boot:
1. **Decide the invariant**: define the ACTIVE-tip projection (the seed the chain currently rests at)
   distinctly from historical-issued (`is_issued` / M2-01). Options:
   (a) after a `NotAcceptedOffline` rewind the held doc becomes a terminal that `is_issued` returns
       FALSE for (so last_issued/boot/scan all naturally skip it and agree on H0) — a completion +
       is_issued change, possibly a migration / new state or marker; OR
   (b) a cohort-cancel-aware active-tip projection that all consumers call (boot, scan, last_issued),
       keyed on the rewind witness, NOT bare RMR.
2. **Do NOT break M2-N2b** (an RMR with a LIVE issued successor must still anchor the chain) and do NOT
   globally drop RMR/CANCELLED (audit F4: CANCELLED historical MAC-link continuity should still be
   verifiable separately — audit MAJOR).
3. **Regression teeth**: the repro above (RED→GREEN); a second-boot idempotency check; the next doc
   signs from H0; the scan cohort-cancel-clean + fork-guard; M2-N2b preserved.

## 6. Not done / decisions deferred to the coordinated fix

- Whether the held doc's terminal should change (5.1a) vs. a projection change (5.1b) — a design call
  touching fiscal state semantics + legal offline-receipt history. NOT decided here.
- The scan stays REVERTED (bc6f1937 pristine) until 5.1 is decided.

## 7. Resolution (2026-07-25)

Decided: **variant 5.1b, direct-`previous_hash`** — see `COORDINATED_FIX_DESIGN_active_chain_tip.md`.
`is_issued` is UNCHANGED (option 5.1a rejected — it would have altered legal offline-receipt history and,
critically, could NOT recover a non-doc T=112 rewind seed). A new `active_chain_tip_unsigned_xml_sha256`
projection reads the `chain_superseded_at`-marked held doc's `previous_hash` DIRECTLY; boot, MacReseed
guard-B, and `invariant_scan` all call it. `last_issued_unsigned_xml_sha256` was reverted to honest
"last issued doc" semantics. Scope is `NotAcceptedOffline`-rewind durability ONLY — standalone T=112
(`PRRO_GATE-hpc`) and MacReseed (`PRRO_GATE-mcc`) NC-03 recovery remain open parallel bds.
