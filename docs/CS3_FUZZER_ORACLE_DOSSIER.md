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

| Wire script leaf | model `online_outcome_state` | held witness (`online_held_witness`) | offline-drain (`offline_held_witness`) |
|---|---|---|---|
| `[Ack, Ack]` | `Ack` (issued, seed advances at SEND) | `None` (released — see leaf-expansion) | `None` (issued — same) |
| `[Reject]` | `Rejected` (non-issued row, seed NOT advanced) | `None` (RELEASES to REJECTED) | **Some** (C-i): SUBMITTED / PARSED_DPS_ENVELOPE / **TerminalReject** / NoNodeEffect / Rejected / — / **PENDING_APPLY** / **STOP_MODE** / **fence** — HOLDS → RMR |
| `[Superseded]` | `Sending` (`_`) | **Some**: SUBMITTED_UNKNOWN / NO_RESPONSE / TransientRetry / NoNodeEffect / NoResponse / — / PENDING_APPLY / STOP_MODE / fence | same as online (classifier is origin-agnostic) |
| `[BadHashPrev]` | `Sending` (`_`) | **Some**: SUBMITTED / PARSED_DPS_ENVELOPE / MacRecovery / MacReseedPending / Rejected / — / PENDING_APPLY / STOP_MODE / fence | same as online |
| `[Ack, NotFound]` | `Sent` (D5 held-at-SENT) | `None` (release-at-SEND) | `None` (release-at-SEND — VERIFIED non-divergence, C-i) |
| `[UnknownStatus(c)]` | `Sending` (`_`) | **Some**: SUBMITTED_UNKNOWN / PARSED_DPS_ENVELOPE / ProbeRequired / ProbeRequired / UnknownStatus / code=c / PENDING_APPLY / STOP_MODE / fence=held | same as online |

**(C-i) origin key:** the offline `[Reject]` differs from online ONLY in the APPLY decision. The
classifier axes (`submission_certainty` / `response_provenance` / `routing_class` / `node_effect` /
`evidence_*`) are byte-identical (same prod delivery classifier runs on the drain send); the
divergence is `apply_state` (`PENDING_APPLY` held vs `APPLIED` released), `node_mode` (`STOP_MODE` vs
`ONLINE`), `fence_held` (`true` vs `false`), doc (`SENDING` held vs `REJECTED` non-issued). The
backlog doc crossed the local-commit threshold (`OFFLINE_LOCAL_ACK`) so a reject can NOT roll it back.

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
   defer it behind the easier steps 3/4). Sub-parts, in the intended order: (i) ✅ **DONE** —
   origin-keying: `[Reject]` (online releases to REJECTED; offline-drain holds → RMR) encoded +
   proven, `[Ack,NotFound]` empirically shown to be a **non-divergence** (release-at-SEND both
   origins) — see "(C-i) offline origin-keyed held witnesses" below; (ii) ✅ **DONE** — drain-produced
   HELD witness fires GENERATIVELY in `run_harness` (routes OFFLINE lane through `offline_held_witness`,
   pinning the delegation — see "(C-ii)" below); (iii) `NotAcceptedOffline` cohort-cancel + chain rewind
   — LAST (the hard one).
3. **Increment 2 part (b)** — fence-IDENTITY strengthening (P3): ✅ **DONE for the held-witness
   surface** (external-audit MAJOR fix — the `fence_held` axis + `active_held_reservation` now assert
   the fence NAMES this doc's reservation at the CURRENT `delivery_generation`, prod predicate
   `invariant_scan.rs:228-237`; 2 canaries). Any REMAINING fence-identity gaps outside the held-witness
   read (e.g. a standalone fence-pointer invariant on every op) are the residual of this item.
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

### (B) MacReseed directed teeth — #338 guard A/B conformance

The MacReseed seed-validation finding (above, "Production findings") landed as prod hardening #338
(two fail-closed guards in `complete_operator_pending`). The fuzzer now MIRRORS the prod teeth
`oc23`/`oc24` through its REAL seam. MacReseed stays generator-EXCLUDED (needs the operator's corrected
chain seed) → directed-only via `interp::operator_complete_macreseed(ctx, seed)` (explicit seed, not
the `[0x5a;32]` placeholder) + `FuzzCtx::last_issued_tip`. Model contract:
`model::macreseed_completion_releases(online_origin, hold_is_macreseed_pending, seed_matches_tip)` —
releases iff all three hold, INDEPENDENT of prod. Four teeth: the model contract [pure]; the VALID
reseed (seed==tip on a MacReseedPending hold → doc RMR, fence clear, node un-halted, scan clean); guard
A (non-MacReseedPending hold → `MacReseedHoldMismatch`, hold intact, seed unchanged); guard B (seed≠tip
→ `MacReseedSeedMismatch`, hold intact, seed unchanged = no ChainSeedMismatch). **Both guard teeth
canary-proven**: neutralize guards A+B in prod → both RED (Released, not refused) → restore (prod
pristine). Subtlety: guard B only accepts `seed == local last-issued tip`, so a valid reseed is a
value no-op — the model's generative `apply_operator_complete` still marks MacReseed `advances_seed=true`
(unexercised: generator-excluded), a documented latent model imprecision, not wired.

### ✅ (C-i) offline origin-keyed held witnesses — `[Reject]` holds / `[Ack,NotFound]` releases

Task #18 (C) sub-part (i). Test/model-side only; prod FROZEN (0 `src` diff). Method mirrored Increment
3 exactly: a throwaway instrumented probe drove the four leaves through the REAL seam on the offline
(`new_offline_open_shift(3)` → `OfflineSell` → `GoOnline([leaf])`) and online fixtures, DUMPED the real
`delivery_reservation` row + `node_state` mode/fence + shift, THEN the tuple was encoded INDEPENDENTLY
in the model (no prod `classify()` import). The probe was removed after; the values are pinned by teeth.

**Empirical ground truth (probe 2026-07-24, all four leaves):**

| leaf · origin | routing_class | evidence | apply_state | doc | node_mode | fence | held? |
|---|---|---|---|---|---|---|---|
| `[Reject]` · **offline-drain** | **TerminalReject** | Rejected | **PENDING_APPLY** | SENDING | **STOP_MODE** | **✓** | **HOLDS → shift RMR** |
| `[Reject]` · online | TerminalReject | Rejected | APPLIED | REJECTED | ONLINE | ✗ | releases (non-issued) |
| `[Ack,NotFound]` · offline-drain | — (NULL) | Accepted | APPLIED | SENT | GOING_ONLINE | ✗ | releases-at-SEND |
| `[Ack,NotFound]` · online | — (NULL) | Accepted | APPLIED | SENT | ONLINE | ✗ | releases-at-SEND |

- **`[Reject]` is the genuine origin divergence.** Same classifier axes both origins; the offline
  drain HOLDS (the backlog doc crossed the local-commit threshold — `OFFLINE_LOCAL_ACK` — so it can't
  roll back to a non-issued `REJECTED`), online APPLIES the reject. `model::offline_held_witness` (new,
  parallel to `online_held_witness`) encodes the `[Reject]` held tuple and delegates every other leaf
  to `online_held_witness` (the classifier is origin-agnostic for Superseded/BadHashPrev/UnknownStatus,
  and `[Ack,Ack]`/`[Ack,NotFound]` release on both).
- **`[Ack,NotFound]` is a VERIFIED non-divergence** — both origins release-at-SEND (APPLIED reservation,
  doc SENT, fence clear; the `NotFound` lastChk only defers the KVT quittance). The model's `None` for
  the leaf is an honest, probe-confirmed coverage boundary, not a gap.
- **Teeth (5, all directed — C-ii wires the generative firing):**
  `offline_reject_holds_terminal_reject_origin_keyed` [pure: offline Some vs online None],
  `offline_reject_held_witness_reds_on_prod_apply_regression` [pure canary: an APPLIED/unfenced
  regression — the held doc silently released like the online lane — REDs against the held contract],
  `directed_offline_reject_held_witness_matches_real_reservation` [e2e: real drain → probe → match],
  `directed_online_reject_releases_to_rejected_no_hold` [e2e: the origin counterpart],
  `directed_offline_ack_notfound_releases_at_send_not_held` [e2e: the non-divergence].
- **Canaries personally proven** (revert-canary): flip `offline_held_witness` `routing_class`
  → e2e RED `real "TerminalReject" != model "TransientRetry"`; flip `apply_state` (the origin-key axis)
  → e2e RED `real "PENDING_APPLY" != model "APPLIED"`. Both reverted; model pristine.
- **Verification:** fuzzer **152/152** (was 147); fmt + crate-scoped clippy `-D` clean; prod pristine.
- **Adversarial review (internal 3-lens panel, unanimous GO, zero soundness blockers):** Lens A
  (fidelity) confirmed every encoded axis against FROZEN prod source — the origin key is exactly
  `delivery_reservation.rs` `Some("Rejected") => if !online { HeldNotAutoRelease }` (offline holds
  PENDING_APPLY) vs the online release-to-REJECTED default branch; STOP_MODE/fence from
  `record_outcome`'s `offline_reject_hold` early-halt; `[Ack,NotFound]` release-both-origins from the
  `Accepted` apply branch. Lens B adversarially attacked the `_ => online_held_witness` delegation
  (hypothesis: offline Superseded/BadHashPrev diverge on node_mode/fence) and REFUTED it — both lanes
  route through `stage_send::run → record_outcome`, whose early node-safety halt sets
  STOP_MODE+PENDING_APPLY **origin-agnostically**, so the delegation is sound. Lens C confirmed
  independence (hardcoded literals, no prod `classify()` import), non-vacuous canary, and ZERO
  generative-capstone regression risk (`offline_held_witness` is referenced ONLY by the 5 new teeth;
  `run_harness` still uses `online_held_witness` on the online lane). The doc-type-agnostic claim
  (the probe/e2e drove the interposed `OFFLINE_SESSION_BEGIN`, not a `SELL`) was resolved sound: the
  `offline_reject_hold` predicate is keyed on `kind=="Rejected" && offline-origin`, NOT on doc_type.
- **→ C-ii handoff note (Lens B MINOR) — ✅ RESOLVED in (C-ii) below:** the delegation arm
  (`_ => online_held_witness` for Superseded/BadHashPrev/UnknownStatus/[Ack,Ack]/[Ack,NotFound]) was
  sound-but-unpinned; (C-ii) now exercises it both directedly (3 e2e drain teeth) and generatively
  (`run_harness` routes OFFLINE drains through `offline_held_witness`).

### ✅ (C-i) external cross-model audit — NO_GO → RESOLVED (fence authority + [Ack,NotFound] honesty)

An external decorrelated auditor returned **NO_GO** (production confirmed SOUND — A–G all
confirmed-sound, no model-vs-prod defect; the encoded `[Reject]` values are right). Two oracle-side
findings, both **model/oracle-should-tighten, NOT a prod defect**, both fixed:

- **MAJOR — the `fence_held` witness axis checked pointer PRESENCE, not AUTHORITY.** `read_held_witness`
  computed `fence_held = active_delivery_reservation_id IS NOT NULL`, and `active_held_reservation`
  selected any `PENDING_APPLY` row — neither verified the reservation IS the node's active,
  current-generation held one. **Empirical bypass (reproduced):** after a genuine offline reject holds,
  overwrite `node_state.active_delivery_reservation_id` with a foreign `[0xA5;16]` →
  `directed_offline_reject_held_witness_matches_real_reservation` PASSED (blessing a forked/P3 fence).
  **Fix:** both `read_held_witness.fence_held` and `active_held_reservation` now use the FULL prod
  exemption predicate (`src/db/invariant_scan.rs:228-237`): `state='OUTCOME_OBSERVED' AND
  apply_state='PENDING_APPLY' AND reservation_id = ns.active_delivery_reservation_id AND
  authorized_generation = ns.delivery_generation`. Tightening `active_held_reservation` is
  behavior-preserving by Increment 2 (≤1 active reservation ⇒ the `PENDING_APPLY` row IS the active
  one). Two negative canaries added — `offline_reject_held_witness_reds_on_foreign_fence_pointer` and
  `..._reds_on_stale_generation` — **both personally canary-proven**: reverting `fence_held` to
  presence-only makes both RED (the exact bypass), the full predicate makes both GREEN. All prior
  held-witness teeth (UnknownStatus/Superseded/BadHashPrev) + both generative capstones stay GREEN (a
  faithful hold's fence IS authoritative), so the generative oracle now asserts fence AUTHORITY on
  every generated hold. **This lands dossier follow-up #3 (fence-IDENTITY strengthening, P3) for the
  held-witness surface.**
- **MINOR — the `[Ack,NotFound]` leaf's `NotFound` is the EMPTY-QUITTANCE (K4) form**, not a real
  `DpsError::NotFound` (`interp::wire_to_result` maps it to send→Ack + empty-`data_sign` lastChk). The
  encoded `None` is right (either way the doc issued at SEND), but the "real NotFound" framing
  over-promised. Fixed: honest comment + `directed_offline_ack_notfound_releases_at_send_not_held` now
  POSITIVELY asserts `apply_state=APPLIED` / `doc=SENT` / fence pointer NULL (not just "no held row").
- **Verification:** fuzzer **154/154** (was 152; +2 fence canaries); fmt + clippy `-D` clean; prod
  FROZEN (0 src diff). Large-N (`FUZZ_CASES=4096`) generative capstones re-run to re-validate the
  touched generative fence path.

### ✅ (C-ii) drain-produced HELD witness — generative wiring + delegation pinned

Task #18 (C) sub-part (ii) — the held-witness oracle now fires after a DRAIN, not only a direct send,
and the OFFLINE lane routes through `offline_held_witness` (closing the Lens-B MINOR). Test-side only;
prod FROZEN (0 src diff); no model change (reuses C-i's `offline_held_witness`).

- **Delegation empirically pinned (probe, then removed).** A throwaway probe drove
  `OfflineSell → GoOnline([leaf])` through the REAL `backlog_drain::drain` for the three DELEGATED
  leaves and confirmed each drain-produced held witness MATCHES `offline_held_witness` (= the online
  tuple), with `fence_held = true` (the C-i fence-authority predicate holds on a genuine drain hold):
  `[Superseded]` → SUBMITTED_UNKNOWN/NO_RESPONSE/TransientRetry/NoNodeEffect/NoResponse/PENDING_APPLY/
  STOP_MODE/fence; `[BadHashPrev]` → SUBMITTED/PARSED/MacRecovery/MacReseedPending/Rejected/…;
  `[UnknownStatus(-4)]` → SUBMITTED_UNKNOWN/PARSED/ProbeRequired/ProbeRequired/UnknownStatus/code=-4/….
  So the `_ => online_held_witness` delegation is now EXERCISED (Lens-B MINOR closed), not assumed.
- **Generative wiring** (`run_harness`, post-match, arm-independent): after an `Op::GoOnline`/`Op::Drain`
  that NEWLY produced a fence-authoritative held reservation (`held_res_before.is_none()` → a hold now
  rests) AND whose leaf `offline_held_witness` encodes, assert the real persisted axes match. The
  `held_res_before.is_none()` guard is **load-bearing**: a drain on an already-halted node NO-OPs over a
  prior op's hold (before==after), and that stale hold's leaf need not equal the drain's script —
  attributing it would false-RED. Fires generatively on `[Reject]` (origin-key) + the three delegated
  leaves across both capstones.
- **Teeth (4):** 3 directed e2e (`directed_offline_drain_{superseded,bad_hash_prev,unknown_status}_
  held_witness_matches_real_reservation`, shared `assert_offline_drain_held_matches`) + 1
  generative-path (`harness_offline_drain_reject_fires_held_witness_check`, runs the FULL `run_harness`
  on `[OfflineSell, GoOnline([Reject])]`). **Canary personally proven:** flip `offline_held_witness`
  `[Reject]` routing_class → the generative-path tooth REDs with `drain-produced held-witness
  divergence on GoOnline([Reject]): real "TerminalReject" != model "TransientRetry"` → reverted.
- **Verification:** fuzzer **158/158** (was 154; +4 teeth); fmt + clippy `-D` clean; prod FROZEN.
  Large-N (`FUZZ_CASES=4096`) generative capstones re-run — the drain held-witness check now fires at
  scale on both lanes without false-RED.
