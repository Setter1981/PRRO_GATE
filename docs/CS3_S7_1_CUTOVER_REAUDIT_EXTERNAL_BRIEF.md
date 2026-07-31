# CS-3 S7-1 double-issue cutover — external re-audit brief

**Hand this whole document to a fresh, model-decorrelated reviewer (ideally a DIFFERENT model
than the one that authored the cutover — the builder was Claude Opus 4.8; the prior external
round proved a different model catches what the same model's own adversarial passes miss).**

---

## 0. Your role and the stakes

You are an **adversarial auditor** of the single most dangerous change in this project: a **live
rewiring of the online fiscal send-path** of a Ukraine PRRO (РРО) gateway. If a hole survives,
the system can **double-issue a fiscal receipt** (issue the same sale to the tax authority twice,
advancing the legal chain seed twice) or **brick a live cash register** (STOP_MODE it with no
recovery). Treat every safety claim below as **guilty until proven innocent**: your job is to
**find the hole**, not to bless the design. Ground every finding in real `file:line` from the
**post-cutover tree** — the design narrative may be wrong or only partially implemented; trust the
code, not the prose.

## 1. What you are auditing

- **Repository:** the PRRO gateway (Rust workspace under `rust/`, crate `prro`).
- **Branch / worktree:** `cs3-de-slice7-cutover` (worktree `/home/setter/prro-gate-wt/cs3-de-slice2`).
- **The atomic change under audit:** commit **`761a178`** ("S7-1 double-issue cutover"). Its parent
  **`38bb8d6`** is the pre-cutover baseline.
- **The diff:** `git diff 38bb8d6..761a178` — **50 files, ~3127/1892 ins/del** (src: 17 files,
  ~1000/832). Also present: a follow-up `fd612f2` (CS-1 provenance re-baseline + inventory re-mint)
  — provenance bookkeeping, **not** part of the behavioural audit, but you may sanity-check it did
  not weaken the neutrality proof.
- **Read the actual post-cutover code**, e.g.:
  - `git -C <worktree> diff 38bb8d6..761a178 -- <path>`
  - `git -C <worktree> show 761a178:<path>` (post) / `... show 38bb8d6:<path>` (pre)
  - or read the files directly under `<worktree>/rust/prro/src`.

**Key src files (the whole behavioural surface):**

| File | What changed |
|---|---|
| `services/write_path/stage_send.rs` | **Core.** `run` rewired onto a 4-phase `run_one_attempt`: authorize-tx → submit (wire) → record-tx → apply. Legacy 4-b block + in-run MAC-recovery loop **deleted** (~1068 lines churned). |
| `services/write_path/apply_orchestration.rs` | **New (261 lines).** `apply_recorded_outcome(pool, reservation_id) -> anyhow::Result<ApplyResult>`; closing-cash derived OUTSIDE the tx; `confirm_shift_edge` gated. |
| `services/write_path/submit.rs` | `submit_authorized` — the **sole** `send_chk_observed` wire site; returns `(AttemptObservation, Result<CheckAck, DpsError>)`; rebind hashes via `wire_hash`. |
| `services/write_path/error_routing.rs` | `target_wire_decision(legacy) -> WireDecision`; `ProbeReason::OkButNoFiscalNumber`; the outcome→routing table. |
| `transports/dps/channel.rs` | `send_chk_observed` is now a **REQUIRED** trait method (default REMOVED) — compiler forces every channel/mock to give a faithful observation. |
| `transports/dps/dto.rs` | `CheckEnvelope::wire_hash()` = `SHA256(prost gen::Check)` over **all 7 wire fields**. |
| `db/repositories/delivery_reservation.rs` | `record_outcome` writes durable STOP for offline-origin reject (`offline_reject_hold`); trace/audit gap-fill. |
| `db/repositories/fiscal_documents.rs` | `SENDING` added to the **issued** set (`OFFLINE_ISSUED_STATES`/`is_issued`); the `(Sent, Rejected)` edge removed. |
| `db/invariant_scan.rs` | `StuckSending` **relational exemption** (a held SENDING doc is exempt ONLY under a full witness: same doc+FN reservation `OUTCOME_OBSERVED`+`PENDING_APPLY`, active-gen match, node `STOP_MODE`/`BLOCKED`). |
| `services/write_path/inline.rs` | `document_state` truthfulness: re-reads DB state instead of reporting the routing-target. |
| `services/reconciliation/er_redrive_policy.rs` + `online_convergence.rs` + `boot_phase.rs` + `backlog_drain.rs` + `last_chk_probe.rs` | **R6:** `ErRedriveDecision::Redrive` collapsed to `EscalateManual{TransientRetry}`; all 3 callers route stuck ErrorRetryable → RMR+STOP. R4/R5 producers → `sent_not_found_to_manual`. |
| `services/reconciliation/reservation_boot_pass.rs` | boot `apply_one` rewired onto `apply_recorded_outcome`. |

## 2. The new contract (what the cutover asserts)

The old model auto-redrove transient failures and ran an in-run MAC re-sign loop. The new model is
**"record a HOLD, halt, never blindly resend"** (a conscious liveness trade for P2 safety — a
transient/ambiguous failure does **not** prove the DPS never saw the receipt, so a blind resend is
exactly the double-issue class this cutover closes):

- **ambiguous / transport / decode / -3 / -6 / -99 / Superseded (post-CALL_STARTED)** → **HELD**:
  the doc rests `SENDING` under a `PENDING_APPLY` reservation, the node halts to `STOP_MODE`,
  **no auto-retry** (treated as `SubmittedUnknown`).
- **-12 (BadHashPrev)** → **HELD** (`MacReseedPending` + STOP), no re-sign, no 2nd wire.
- **a stuck `ErrorRetryable`** → escalates to **RequiresManualReconciliation** + STOP (R6), no re-wire.
- **offline-origin DRAIN reject** (DocumentReject / -12 / Superseded on `OpenedLocalPendingDrain` /
  `ClosingLocalPendingDrain`) → doc **HELD** + shift → **RequiresManualReconciliation** (strict-
  sequential halt): the offline receipt is already in the customer's hands, so it cannot be rolled
  back.
- **pre-wire P3**: an online-origin doc whose node seed `!= previous_hash`, or a missing
  `node_state`, **refuses authorize with ZERO wire** (atomic rollback).
- **terminal ONLINE reject (-1/-5/-15/-16)** → `Rejected`; **-11** → `Rejected` + `BLOCKED` (unchanged).
- **Seed / D2 pin:** the online chain seed advances **atomically with the `server_fiscal_no` stamp**
  at the `Sending→Sent` CAS (that CAS *is* the online-issuance moment, not ACK). A **pre-SENT reject**
  consumes the local number but does **NOT** advance the seed (`Rejected` row legitimately rests). A
  **post-SENT reject** is issued-but-unconfirmed → `RequiresManualReconciliation`, seed **NOT** rolled
  back.

## 3. Safety claims to REFUTE (attack each independently)

1. **Double-issue window.** EXACTLY ONE `send_chk_observed` wire call happens per document across
   ALL paths: happy, in-run retry, crash-resume, boot reservation-pass, -12/MAC, transport failure,
   ambiguous timeout, and **two `run()`s racing the same `fiscal_number`** (per-FN fence +
   generation-CAS on the reservation). Attack: any second wire after `CALL_STARTED`; boot re-wiring a
   `CALL_STARTED` reservation; a stale generation re-authorizing+re-wiring; the static sole-seam pin
   being bypassable.
2. **Seed / chain integrity.** Seed advances atomically with the sfn stamp; pre-SENT reject → no
   advance; post-SENT → RMR, no rollback; no fork, no seed gap. Attack: seed advancing without sfn (or
   vice-versa); a reject path that rolls the seed back; concurrent seed writers.
3. **Crash-safety across record→apply.** A crash at ANY seam (post-wire/pre-record, post-record/
   pre-apply, mid-apply) is recovered by boot with no lost and no double issuance; no doc rests in a
   non-terminal `PREPARED`/`SIGNED`/`ENCRYPTED` state at a quiescent boundary. Attack: a reservation
   boot cannot classify; `apply` running twice; boot auto-releasing a HELD doc.
4. **HELD contract completeness.** Every non-terminal outcome (§2) routes to the FULL HELD signature
   (`SENDING` + `PENDING_APPLY` + `STOP_MODE`, no auto-retry); stuck ER → RMR; offline-drain-reject →
   shift RMR; pre-wire P3 → zero-wire refuse. Attack: any outcome that leaks to auto-retry; an
   `invariant_scan` exemption that is too broad (masks a real stuck doc — regression of bug #192) or
   too narrow (false STuckSending alarm); `inline` reporting a state that disagrees with the DB.
5. **B1 / B2 / invariant #1.** **B1:** ONE unified `wire_hash` over all 7 wire fields is used for
   token + rebind + trace — no field substitutable. **B2:** the TARGET (`ClassifiedOutcome` +
   `EvidenceDiscriminant`) is the SOLE post-wire authority for record/trace/audit/return — legacy
   `WireDecision` used ONLY for the drift-pin. **Inv #1:** the wire call is strictly OUTSIDE every
   SQLite write-tx (no network/crypto inside `with_immediate`). Attack: any hash path still keyed on
   `check_sign` only; legacy `WireDecision` leaking in as authority (empty-SFN split-brain); an
   `.await` on the wire inside a write-tx.
6. **R6 / reconciliation.** `Redrive` fully collapsed to `EscalateManual`; all 3 callers route stuck
   ER → RMR+STOP, never re-wire/spin; no auto-redrive path survives; R4/R5 `sent_not_found_to_manual`
   has no gap. Attack: a surviving Redrive arm; a reconciliation path that re-drives ER through the wire.
7. **Test-convergence soundness.** No migrated test weakened its assertion or masks a real regression;
   the **14 removed test identities** genuinely tested RETIRED machinery (MAC-recovery loop / auto-
   redrive), not live behaviour, with no surviving unique-coverage loss; the `invariant_fuzzer` oracle
   change reflects real production behaviour (corrects the model), not hides a prod/model divergence.
   Attack: a migrated assertion that now asserts LESS; a removed test with no surviving coverage; a
   fuzzer-model edit that weakens the differential. (Removed identities: `write_path_dps_error_routing`
   ::{fx17, mac_fx01, mac_fx02, mac_fx03}; `write_path_stage4_send`::{mac_recovery_resigned,
   non_recovery_send_drift, retry_path_error_retryable, server_minus_11_missing_node_state,
   transport_retryable_routes, variant_p_mac_recovery}; `shift_life_matrix`::{s14, s15, s5};
   `write_path_deterministic_replay`::fixture_6.)

## 4. Prior-round findings — re-verify they actually held

The previous external round (before implementation) found these; they are **claimed fixed** in
`761a178`. Independently confirm each fix is real in the post-cutover code (do NOT take it on faith):

- **B1 (two-hash bug):** token/rebind previously hashed `check_sign` only, leaving
  `rro_fn/date_time/local_number/check_type/id_offline/id_cancel` substitutable → now one full-
  envelope `wire_hash`. *Verify `dto.rs::wire_hash` + every caller (`submit.rs`, `stage_send.rs`
  `compute_envelope_hash`, trace) uses it.*
- **B2 (legacy WireDecision split-brain):** legacy decision as post-wire authority caused an
  empty-SFN `Sent{""}` to be treated as accepted → now TARGET evidence is the sole authority.
  *Verify no record/trace/audit/return path reads the legacy decision as authority.*
- **C (`apply_recorded_outcome` contract):** returns `anyhow::Result<ApplyResult>`, and
  `HeldNotAutoRelease` is an EXPECTED-swallow (not fatal); boot downcast works. *Verify.*
- **D (SubmitRefused after CALL_STARTED):** fail-closed to STOP (unreachable by construction; boot
  normalizes). *Verify the fail-closed arm exists and cannot re-wire.*

## 5. Frozen invariants to check are preserved

1. No network or crypto calls inside long SQLite write transactions.
2. One `fiscal_number` = one logical single-writer write-path.
4. Idempotency is mandatory.
8. Recovery and reconciliation must not silently violate state transitions.
9. Graceful shutdown matters more than finishing fast.

## 6. Method and discipline

- **Read-only.** Use only `git -C <worktree> diff/show/log`, `grep`/`rg`, `cat`/`sed -n`. NEVER
  `git checkout/reset/commit/branch/stash/add/restore`, and never edit any file — the worktree is
  shared and live.
- Prefer a **concrete break-scenario** (a specific interleaving, crash-point, or DPS-reply sequence)
  over a general worry. A claim with no reachable code path is not a finding.
- Classify: **CRITICAL** (double-issue / chain-fork / lost issuance / brick) · **MAJOR** (recovery-gap /
  audit-gap / invariant erosion) · **MINOR** (style / naming / doc).
- Report even a single real hole. If a claim holds after genuine effort, say so **with the evidence
  that convinced you** (the guarding `file:line`).

## 7. Output format

Deliver:

1. **Verdict:** `GO` (no blocker) · `GO-WITH-FIXES` (minor/major to land before/with merge) · `NO-GO`
   (a CRITICAL hole survives).
2. **Findings**, each: `severity` · `title` · `file:line` · `safety claim broken` · `concrete
   scenario` · `evidence (quoted code)` · `minimal fix`.
3. **Coverage note:** any lens you could not fully discharge, or a surface you think was under-attacked.

Be concise and honest. Do not manufacture blockers to look thorough; do not soften a real one to look
agreeable.
