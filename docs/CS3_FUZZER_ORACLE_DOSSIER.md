# CS-3 Fuzzer Oracle — Deliverable Dossier

**Intent:** turn `invariant_fuzzer` into a permanent, INDEPENDENT oracle for the whole of
CS-3 (authorization, exactly-once wire, record→apply, crash recovery, FN-fence, operator
completion, Slice E classifier). The oracle must actually FIND: double-issue return, FN-chain
fork, doc loss between commit boundaries, double ledger/seed apply, illegal HELD release, and
eternal BRICK after a legal operator recovery.

**Branch:** `fuzzer-cs3-oracle`  **Base `origin/main` SHA:** `5360ecf` (Slice E merge).

**Discipline (non-negotiable):**
- Test/model-side diff ONLY — production (`rust/prro/src`, `rust/prro-domain/src`) is FROZEN.
- The reference model encodes the CS-3 contract INDEPENDENTLY from the SPEC — it does **not**
  import/call production `classify()` / `routing_for_indeterminate()` nor read production
  routing tables as the expected result.
- A real production defect the oracle finds → a SEPARATE `bd`-finding with `discovered-from`,
  never worked-around by weakening the model / blanket Fault / DB-resync / state-exclusion.
- New `prop_oneof!` arms are appended at END only (regression-seed indices preserved); no
  `prop_filter` on the main generator.
- Each RED-tooth is personally introduced → RED → reverted, naming the specific test/seed.

---

## Increment ledger

### ✅ Track 1 — wire alphabet: `UnknownStatus` leaf via REAL production decode (`12e8206`)

- `op.rs`: `WireResponse::UnknownStatus(i32)` appended LAST + `DpsScript::unknown_status(code)`.
- `scripted_dps.rs`: per-send observation-override queue (`push_send_obs_override`); the wire
  is still counted/spied/hung, only the OBSERVATION is the real-decode one.
- `interp.rs`: `load_script` routes `UnknownStatus` through `scripted_raw_observation`
  (production `observe_check_reply`), NOT a legacy `DpsError` (which
  `observe_faithful_from_legacy` degrades `Indeterminate → NoResponse`, losing `ProbeRequired`).
- `strategy.rs`: generator arm appended LAST on BOTH `dps_script` (sell lane) and
  `shift_dps_script` (SHIFT_OPEN / Z_REPORT lane).

**RED-tooth:** `generator_emits_unknown_status_on_sell_and_shift_lanes` (anti-silent-zero,
≥100/2000 both lanes). **Canary (verified):** stripping the appended arm → RED
(`UnknownStatus under-emitted on the shift/Z lane (0 over 2000 seqs)`).

### ✅ Track 2+6 (increment 1) — independent delivery-axis model + relational HELD-witness oracle (`cd62d62`)

- `model.rs`: `HeldWitness` (DB-text axis tuple) + `online_held_witness(script)` — the
  `UnknownStatus → ProbeRequired` contract encoded INDEPENDENTLY (Slice E directed pin +
  migration 038), NOT from prod `classify()`. Other held leaves return `None` (documented
  coverage boundary — see the leaf-expansion track).
- `interp.rs`: `ObservedHeld` + `FuzzCtx::read_held_witness` (joins `delivery_reservation`→doc
  by `request_id`, latest `attempt_no`; + `node_state.mode` and `active_delivery_reservation_id`
  fence) + `last_request_id`.
- `oracle.rs`: `check_held_witness` — pure comparator over 7 text axes + `evidence_code` +
  fence; a missing reservation while the model predicted a held witness is itself a divergence.
- `op.rs`: `Op::wire_script` (the held-able online wire ops).
- `invariant_fuzzer.rs`: `run_harness` wires the held-witness check into the
  `PredictableMutating` arm, gated on real `doc_state == Sending` (load-bearing: `inline::run`
  dispatches by NODE MODE, so the same op offline-seeded takes the offline lane; only a
  genuinely held SENDING doc carries the witness — the main differential still catches any
  `doc_state` divergence first).

**RED-teeth:**
- `held_witness_catches_routing_class_divergence` [pure]: `TransientRetry` vs the
  `ProbeRequired` contract → `Err`; faithful witness passes (non-vacuous); missing row → `Err`.
- `held_witness_unknown_status_matches_real_reservation` [end-to-end]: model's independent
  `ProbeRequired` contract MATCHES real prod. **Canary (verified):** flip
  `online_held_witness` routing_class → `"TransientRetry"` → RED with
  `held-witness routing_class mismatch: real "ProbeRequired" != model "TransientRetry"` —
  proving the oracle reads the REAL reservation, not the model.
- Regression seed `[OfflineShiftOpen, OnlineReturn(UnknownStatus(-4))]` guards the offline-lane
  gate (revert the `doc_state == Sending` gate → `harness_offline_seeded` REDs).

**Verification:** fuzzer binary **128/128 GREEN** (incl. the generative capstone running the
held-witness oracle on every generated `UnknownStatus`); fmt + clippy `-D` clean.

### ✅ Increment 1a — operator-completion release op + anti-BRICK oracle (`7853115`)

Closes the ONLY CS-3 mandate item with ZERO prior coverage: **eternal BRICK after a legal
operator recovery**. The fuzzer established HELD reservations but never released them.
- `op.rs`: `Op::OperatorComplete(OperatorResolutionKind)` (appended LAST).
- `model.rs`: `ReleasedWitness` + `released_witness` — completion contract INDEPENDENT from the
  SPEC (`delivery_reservation.rs` completion matrix, cited): APPLIED + fence-clear always;
  node_mode BLOCKED (origin-blocked, INV-05) / GOING_ONLINE (active session) / ONLINE — NEVER
  STOP_MODE; doc SENT (Accepted) / RMR (others).
- `interp.rs`: `operator_complete` drives the REAL `admin::resolve_operator_pending` + `RealOutcome::Released`.
- `oracle.rs`: `check_release_witness` — axes + the UNCONDITIONAL anti-BRICK invariant (a released
  reservation can NEVER rest STOP_MODE / fenced).
- **DIRECTED-ONLY** (deliberately not generated → never routes through the FaultOrRecovery
  adopt-hole; generative call site + model-tracked prediction = follow-up 1b).
- Teeth: `release_witness_accepted_clears_fence_and_stop` [pure] +
  `directed_operator_complete_releases_unknown_status_hold` [e2e, **canary**: skip the real release
  → `apply_state mismatch: real "PENDING_APPLY" != model "APPLIED"`] +
  `operator_complete_fn_mismatch_refused_hold_intact` [negative].

### ✅ Increment 3 — held-witness leaf expansion: Superseded + BadHashPrev (`a036db0`)

Model-only diff (read/comparator/call-site already existed). Tuples grounded EMPIRICALLY (a probe
read the real reservation row) then encoded independently:
- **Superseded** → `SUBMITTED_UNKNOWN / NO_RESPONSE / TransientRetry / NoNodeEffect / NoResponse`.
  The DURABLE class is `TransientRetry` (faithful-adapter NoResponse degrade), **NOT** the
  `WrapperBug` diagnostic OVERLAY. `node_effect NoNodeEffect` while `node_mode STOP_MODE` is real
  prod (halt = the HELD record, not the node-effect axis).
- **BadHashPrev** → `SUBMITTED / PARSED_DPS_ENVELOPE / MacRecovery / MacReseedPending / Rejected`.
- Teeth: `superseded_durable_class_is_transient_retry_not_wrapperbug` [pure] + 2 e2e
  (**canary**: Superseded routing → `"WrapperBug"` REDs both). Both generative capstones run the
  oracle on every generated Superseded/BadHashPrev (sell + shift + offline) → tuples are
  doc-type-agnostic across the whole space.

### ✅ Increment 2 — at-most-one-active-reservation double-issue guard (`d4ddd67`)

- `interp.rs`: `active_reservation_count()` (the `ux_reservation_active` predicate, migration
  035:53-55, spec-copied).
- `invariant_fuzzer.rs`: an UNCONDITIONAL `<= 1` after every op (NOT behind `is_settled` — a HELD
  reservation is exactly when a 2nd active row is the fork).
- Tooth: `directed_at_most_one_active_reservation_per_fn` [e2e, **canary**: broaden predicate to
  `COUNT(*)` → `double-issue: 2 ACTIVE ...`]. Runs after EVERY op in both capstones — prod NEVER
  produces > 1 active across the whole generated space (single-writer holds; no double-issue found).

---

## Op → expected-FSM matrix (wire leaves)

| Wire script leaf | model `online_outcome_state` | held witness (`online_held_witness`) |
|---|---|---|
| `[Ack, Ack]` | `Ack` (issued, seed advances at SEND) | `None` (released — see leaf-expansion) |
| `[Ack, NotFound]` | `Sent` (D5 held-at-SENT) | `None` (pending leaf-expansion) |
| `[Reject]` | `Rejected` (non-issued row, seed NOT advanced) | `None` (RELEASES to REJECTED — documented gap) |
| `[Superseded]` | `Sending` (`_`) | **Some**: SUBMITTED_UNKNOWN / NO_RESPONSE / TransientRetry / NoNodeEffect / NoResponse / — / PENDING_APPLY / STOP_MODE / fence |
| `[BadHashPrev]` | `Sending` (`_`) | **Some**: SUBMITTED / PARSED_DPS_ENVELOPE / MacRecovery / MacReseedPending / Rejected / — / PENDING_APPLY / STOP_MODE / fence |
| `[Ack, NotFound]` | `Sent` (D5 held-at-SENT) | `None` (online release-at-SEND — documented gap) |
| `[UnknownStatus(c)]` | `Sending` (`_`) | **Some**: SUBMITTED_UNKNOWN / PARSED_DPS_ENVELOPE / ProbeRequired / ProbeRequired / UnknownStatus / code=c / PENDING_APPLY / STOP_MODE / fence=held |

Release (operator-completion) matrix: `Accepted` → doc SENT + APPLIED + fence clear + node ONLINE
(no session) / GOING_ONLINE (active session) / BLOCKED (origin-blocked); `NotAccepted` / `MacReseed`
→ doc RMR + APPLIED + fence clear + un-halted.

## Generator arms (appended LAST, corpus-index-preserving)

- `dps_script` (sell lane): `+ Just(DpsScript::unknown_status(-4))`
- `shift_dps_script` (SHIFT_OPEN / Z_REPORT lane): `+ Just(DpsScript::unknown_status(-4))`
- `Op::OperatorComplete` is **NOT** in the generator (directed-only, 1a).

---

### ✅ Increment 1b — generative operator-completion (`c18a5907`)

Completes the BRICK property's SECOND half GENERATIVELY. `Op::OperatorComplete` is now in the
generator, combined with arbitrary HELD/crash/reboot/drain prehistory; `run_harness` asserts the
durable `ReleasedWitness` (incl the anti-BRICK invariant) + the doc/seed/mode transition on every
completion.
- `model.rs`: `ExpectedOutcome::Release(Option<ReleasedWitness>)` + `apply_operator_complete` —
  predicts the release (mutating docs/mode/seed so the NEXT op issues) or Refused; seed advance
  mirrors prod (Accepted advances ONLY online-origin). The hold PRECONDITION `held_reservation:
  Option<(lnd, online_origin)>` is RE-SYNCED from the real `delivery_reservation` table after every
  op (`sync_held_reservation`) — the adopt pattern; the release OUTCOME stays independent. Classified
  `OpClass::Release`, NEVER FaultOrRecovery (fixes review MAJOR-2's adopt-hole).
- `interp.rs`: `operator_complete` targets the FENCE's active reservation (`active_held_reservation`),
  not "the last wire op's doc" (a drain can hold a non-last doc).
- `strategy.rs`: `OperatorComplete` arm APPENDED LAST — **Accepted + NotAccepted** only.
- Teeth: `operator_complete_releases_then_next_sell_issues` [e2e, **canary**: Accepted witness
  doc_state → RMR → `real "SENT" != model "..RMR"`] + `operator_complete_without_hold_is_inert`.
- **Verified generatively at FUZZ_CASES=1500**: all 3 capstones GREEN (~1500 completion×prehistory
  cases each) — no divergence, no anti-BRICK breach, no double-issue.
- **Deferred (documented)**: `NotAcceptedOffline` (OLA-cohort cancel + chain rewind) and `MacReseed`
  (needs the operator's corrected seed) are EXCLUDED from the generator → the "drain-holds + offline
  origin-keying" increment; both kept as enum variants + interp mappings.

---

## Landed increments summary

8 committed on `fuzzer-cs3-oracle` (base `5360ecf`, tip `3c44be11`): Track 1 (`12e8206`) · Track 2+6
(`cd62d62`) · Increment 1a (`7853115`) · Increment 3 (`a036db0`) · Increment 2 (`d4ddd67`) · addendum
(`ef04c1d`) · Increment 1b (`c18a5907`) · step 2 crash/replay (`3c44be11`). Fuzzer **141/141 GREEN**;
each tooth canary-proven; externally adversarial-reviewed **GO (no BLOCKER)** through the addendum.

### ✅ step 2 — crash/replay held-reservation-survives-recovery (P4) (`3c44be11`)

A committed CS-3 `PENDING_APPLY` held reservation must SURVIVE a crash / reboot — boot recovery may
NOT release it (only an operator completes it) nor lose the doc.  `run_harness` captures the FN's
active held reservation `(reservation_id, request_id)` before every op and asserts it is UNCHANGED
after a Crash / Reboot / RepeatReboot (drain / go-online excluded).  Tooth:
`held_reservation_survives_reboot_then_operator_releases` [`held → Reboot → complete`] — **canary**
(inject an illegal `resolve_operator_pending` into the interp reboot path) RED'd with `crash/replay:
op Reboot changed the held reservation (before=Some(...) after=None)`.

## Remaining follow-ups (documented, not silent) — the user's ordering (do NOT reorder)

1. ✅ **Crash/replay axis-checks** (P4) — DONE (`3c44be11`).
2. **Drain-holds + held-leaf origin-keying** (offline half — the MOST IMPORTANT remaining gap; do NOT
   defer it behind the easier steps 3/4). Sub-parts, in the intended order: (i) origin-keying —
   `[Reject]` (online releases to REJECTED; offline-drain holds) and `[Ack,NotFound]` (release-at-SEND)
   origin-keyed witnesses; (ii) drain-produced HELD witness (the held-witness oracle firing after a
   drain, not just a direct send — map MINOR-2); (iii) `NotAcceptedOffline` cohort-cancel + chain
   rewind — LAST (the hard one).
3. **Increment 2 part (b)** — fence-IDENTITY strengthening (P3): the fence pointer NAMES this doc's
   reservation at the CURRENT `delivery_generation`, not merely non-NULL (columns confirmed, mig 034).
4. **RETURN idempotent replay** (P2) — a held RETURN re-driven / crash+reboot re-issue.
5. **THEN** CS-1 re-baseline + re-mint + PR-ready gate (batched at the TRUE tip — do NOT re-baseline
   early; every increment above touches the same files, so an early re-baseline = double churn).

## CI-gate state (pre-PR)

- Fuzzer nextest: 140/140 GREEN; fmt + crate-scoped clippy `-D` clean.
- `inventory_gate.sh`: all diffs are clean ADDITIONS (~13 new tests, zero removal/rename) —
  additions-only satisfied; needs `mint_manifests.sh` re-mint.
- `cs1_test_provenance`: the live-drift leg flags the actively-developed fuzzer files vs the stale
  `LIVE_DRIFT_BASE_SHA = 9ec0b41` — **re-baseline to the branch tip + re-mint manifests is the final
  pre-PR step** (batched at the tip, per the Slice E re-baseline pattern).

## Production findings surfaced by the fuzzer

- **PROD-HARDENING (discovered-from 1b) — `resolve_operator_pending` does NOT validate the operator's
  MacReseed seed.** The generative harness drove `[OnlineSell(Superseded) → OperatorComplete(MacReseed)]`
  and prod produced a durable `ChainSeedMismatch` (invariant_scan): a `-12` MacReseed with a seed that
  does not match the expected chain tip re-bases `node_state` to a value unrelated to the doc chain,
  corrupting it with no fail-closed. In prod an operator supplies the seed by hand, so a wrong seed is
  an operator-error chain-corruption risk. Not worked around in the model — MacReseed is excluded from
  the generator (needs a valid seed) and this is logged as a potential prod hardening (validate the
  operator seed against the expected tip, or fail closed). Candidate `bd`-task.

## Model-vs-prod divergences the fuzzer caught during 1b (all adjudicated → MODEL corrected)

The generative completion×prehistory exploration caught a sequence of MODEL imprecisions (prod was
correct in each; the model was tightened, never prod):
1. **Targeting** — `operator_complete` targeted "the last wire op's doc", but a drain can hold a doc
   that is not the most-recent sell → fixed to target the FENCE's active reservation.
2. **Non-reservation SENDING** — a drain/crash transport failure commits SENDING + STOP WITHOUT a CS-3
   reservation; the model's coarse "Sending doc under STOP_MODE" over-detected a completable hold →
   fixed with the reality-synced `held_reservation` precondition.
3. **Origin-blind seed advance** — Accepted advances the seed ONLY for an online-origin doc
   (`delivery_reservation.rs:1374 if online`) → the model's unconditional advance was gated on origin.
4. **Origin cross-check** (also review MAJOR-1) — `NotAcceptedOffline`/`MacReseed`/`NotAccepted` are
   origin-restricted → encoded the full cross-check (Refused prediction).

These are the fuzzer doing its job: an INDEPENDENT model that, when it disagrees with prod, forces an
adjudication (prod-correct-model-tightened, or a prod finding). Every 1b divergence was the former
except the MacReseed ChainSeedMismatch above.

## Rebase-onto-#338 latent findings (task #18 — offline-half kickoff)

The branch was rebased onto `main` `bc6f1937` (MacReseed hardening #338 + CI #339). **#338 is INERT
for any non-MacReseed resolution** (its guards are gated on `if let MacReseed`; the completion SELECT
only ADDS a `node_effect` column — same row). The two capstone REDs the rebase surfaced were therefore
NOT #338 regressions but **latent cases the `RngSeed::Random` capstone had not previously sampled** —
both `OperatorComplete(Accepted)` on a held doc. Each now carries a directed regression tooth AND a
pinned generative seed (`tests/invariant_fuzzer.regressions`). Fuzzer **143/143** after both fixes.

1. **Harness fixture-FN fidelity (offline liveness)** — shrank to
   `[Crash(Send), GoOnline([UnknownStatus(-4)]), OperatorComplete(Accepted)]`. An operator-`Accepted`
   go-online-drain held doc → `SENT` RE-ENTERS the drain cohort (`SENT` is a drain-candidate state);
   the settle-drain re-probes it via `last_chk`. The interp operator-completion stub FN
   (`"5000000001"`) ≠ the DPS stub's assigned FN (`SERVER_FISCAL_NO` = `"DPS-FN-ONLINE-1"`), forking a
   `LastChkIdMismatch` → structural-drift halt (§410) → node STUCK `GoingOnline` → the terminal
   liveness gate fires. **NOT a prod fault**: in reality the operator supplies the exact FN DPS
   assigned (they match on re-probe), and a genuine operator FN typo is CORRECTLY fail-closed by this
   very guard. Fix = fixture fidelity: `interp::operator_complete` Accepted FN → `SERVER_FISCAL_NO`.
   Tooth `harness_offline_operator_accepted_held_drain_doc_resettles_online` (**canary proven**:
   revert the FN → liveness RED). With a faithful FN the drain converges the whole cohort to ACK and
   the node settles `Online`.

2. **Model resync seed-placeholder (online seed-advance)** — shrank to
   `[OnlineServiceIn([Ack,Ack]), OnlineSell([BadHashPrev]), OperatorComplete(Accepted)]`. The
   post-fault resync `model::adopt_fault_deferred` placed the STRUCTURAL seed placeholder on
   `max(lnd)` — the held (`SENDING`, non-issued) BadHashPrev sell — instead of the real chain tip (the
   issued ServiceIn predecessor). When the operator later `Accepted`-completes the held doc, prod
   advances the seed onto that doc's hash; the model already sat on that lnd → the completion's real
   seed-advance read as "no model advance" → the Release differential (`invariant_fuzzer.rs:2992`)
   diverged. **Prod correct → MODEL tightened** (a 5th, 1b-class adjudication): `tip_lnd` now tracks
   the doc whose `unsigned_xml_sha256` equals the real seed (falls back to `max(lnd)` only when the
   seed matches no doc, e.g. a generator-excluded MacReseed rebase). Tooth
   `harness_online_operator_accepted_after_badhashprev_hold_seed_advance` (**canary proven**: revert
   `tip_lnd` to `max(lnd)` → seed-advance RED).
