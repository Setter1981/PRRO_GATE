# SPEC rev2 — the peer-tip axis: modelling the second party of the MAC chain

**Date:** 2026-07-31 (rev2 — same day; rev1 was adversarially reviewed by a 5-agent
verify/attack pass before any code, and did not survive it intact)
**Status:** DRAFT for operator review — design only, no code written.
**Unblocks:** `bd PRRO_GATE-2ds` (ambiguous-T112 generator leaf) and `bd PRRO_GATE-5hc`
(the MacRecovery **success** path — which, it turns out, closes one phase *earlier* than 2ds).
**Rev1 → rev2 in one line:** the movers table was wrong in three rows and missing at least six,
the "derived invariant" was falsified as stated, and the ordering assumption behind the override
had zero evidence — one live data point cuts *against* it. Everything below is grounded
file:line; the claims marked **[N=1]** rest on a single live observation and say so.

---

## 1. What is missing (unchanged from rev1, still true)

Not a symbol — **the second party**. A DPS reply is dictated by `DpsScript`, so the oracle checks
our *reaction* to `-12` but never that `-12` *arises* exactly when the chains disagree. Three
things are stuck on this at once: the ambiguous-T112 leaf (`2ds`), `-12` fidelity, and the
MacReseed success path (`5hc` — guard-B's corroboration disjunct
`delivery_reservation.rs:1437-1447` is exercised only by directed tests; the stub's `store` is the
constant `DPS_RECOVERY_TIP`, `op.rs:29-41`, with "Nothing consumes it yet" in its own comment).
The class is not hypothetical: `3uo` was a P1 trap living in exactly this seam.

## 2. What the axis is — and what it is NOT (tautology verdict, accepted)

The adversarial pass sustained a tautology attack against rev1's central claim, and rev2 accepts
it: for the **plain online path**, chain continuity is *already* proven per-op by
`check_doc_against_mutation` (`oracle.rs:207-212`: the real doc's `previous_hash` IS the prior
real tip) and referentially by the scanner chain walk (`invariant_scan.rs:393,413`). A derived
`-12` there detects nothing new, and rev1's "derived invariant" — *whenever our_seed ≠ peer_tip
the next online send MUST earn -12* — is **false as stated** anyway: after any offline
`OFFLINE_LOCAL_ACK` issuance the node seed is legitimately ahead of the peer for the whole
backlog, and the drain sends succeed (`model.rs:1490-1502`). The implementable rule is
**per-DOCUMENT**: *a send whose `previous_hash` ≠ peer_tip earns `-12`*.

So, honestly framed:

- **The axis is coverage machinery, not a new correctness oracle.** Its production-facing value
  is the *trajectories* it makes generatively reachable, each with a real oracle at the end:
  1. divergence → `-12` → **corroborated MacReseed → convergence → next send SUCCEEDS**
     (the `5hc` success path; today unreachable);
  2. **ambiguous T=112** → follow-on derived `-12` (`2ds`; today unrepresentable);
  3. **operator-claim vs peer-truth**: `OperatorComplete(Accepted)` when the peer did NOT take
     the doc (and the converse) — today the claim is applied unconditionally
     (`model.rs:567-587`) and NO check ties it to any later wire consequence;
  4. `-12`-during-**boot**: a reboot re-drives SIGNED sends with a stale `previous_hash` — a
     MacRecovery-in-boot path nothing exercises today.
- **Plus one negative fidelity property**: on every agreeing run, *no spurious `-12`* — which is
  precisely the empirical load test of the movers table (§4).
- The per-document rule above is the **stub's construction rule**, not a pinned production
  invariant. The pins that matter are the trajectory pins.

## 3. Mechanics (verified feasible, with three corrections from the pass)

`FuzzCtx` owns `PeerState { tip: Option<TipVal> }`; the stub gets a pool handle and the
`PeerState` Arc-style — the exact pattern the wire counters already use (`interp.rs:530-532`,
`scripted_dps.rs:70-71`). All of the following was **CONFIRMED** by the verify pass:

- **Timing is safe.** `previous_hash` (pin-tx) and `unsigned_xml_sha256` (3-PERSIST tx) are
  committed before stage 4's wire call, which runs outside any write tx (invariant #1;
  `stage_sign.rs:273-557` → `stage_send.rs:1916-1941` → `submit.rs:87`). A stub-side
  `SELECT previous_hash, unsigned_xml_sha256 WHERE fiscal_number=? AND lnd=?` sees committed
  rows; `(fiscal_number, lnd)` is UNIQUE (`ux_fd_fn_lnd`). The race attack was tried and
  refuted (skeptic 1, verdict NONE).
- **Correction 1 — ShiftOpen lookup.** `build_send_envelope` hard-overrides `local_number=0` for
  `WireArtifactKind::ShiftOpen` in BOTH lanes (`stage_send.rs:461-465`). The stub resolves a
  ShiftOpen by `id_offline` (offline lane) or by the FN's unique `SENDING`-state row (online —
  single-writer guarantees uniqueness).
- **Correction 2 — the override lane.** `send_chk_observed` already has an observation-override
  queue consulted BEFORE `scripted_observation` (`scripted_dps.rs:203-206`, built for
  `UnknownStatus` real-decode). A `-12` override must compose with that lane, not bypass it.
- **Correction 3 — T=112 surfaces.** `ask_offline_codes` is a separate stub queue that does NOT
  increment `send_calls` (`scripted_dps.rs:249-261`) — peer bookkeeping for T=112 must key on its
  own call log, not the send counter. On the GRANTED arm the harness receives `request_xml` +
  `new_seed_hex` in `ReplenishSummary` (`offline_code_replenish.rs:419-425`) — **no
  reconstruction problem**; rev1's worry was misplaced. On the AMBIGUOUS arm nothing is returned
  and nothing persisted (`Err` before Step 6, line 326 — the `3uo` finding), so the peer's new
  tip is a **synthetic value `S`** that later appears only in `-12 store` fields and MacReseed
  corroboration — consistent by construction.
- `DetCrypto` makes `check_sign` a constant, so a hash-validating peer stays unconstructible —
  the DB-read peer is the only faithful option (unchanged from rev1, re-confirmed).
- Genesis: fuzzer fixtures never call `init_chain_seed` → peer starts `None`; `None == None`
  is a match.

## 4. The movers table, rev2 — per-CALL, not per-script

Rev1's table was per-*script*; the pass proved the tip moves per-**wire-call**: in
`[Ack, NotFound]` the send-Ack IS the acceptance moment (the doc merely rests SENT awaiting
quittance — the "NotFound" tail is actually the K4 **empty-quittance** reply, `interp.rs:2798`,
temporally plausible for a peer that accepted). So: **the peer tip moves on the `send_chk` reply;
`last_chk` never moves it.**

| # | event (per wire call / completion) | our seed | peer tip | evidence |
|---|---|---|---|---|
| 1 | online doc send, accepting reply (`Ack…`, incl. `[Ack, NotFound]`) | advances at `Sending→Sent` CAS | advances at the stub reply | `model.rs:1370-1372`; order observable only via crash (row 12) |
| 2 | online doc send, pre-SENT reject (`[Reject]`) | holds (D2) | holds (parsed & refused) | `fiscal_documents.rs:256` |
| 3 | online doc send, HELD (ambiguous / transient after CALL_STARTED) | holds | **generator-chosen `Took`/`NotTook`** | §5 |
| 4 | **forced `[BadHashPrev]` leaf** (kept — see §6) | holds; doc `SENDING`, node STOP | **peer tip := the `store` value it declares** (`DPS_RECOVERY_TIP` or current peer tip) | `op.rs:29-41`, `interp.rs:2831-2838` |
| 5 | offline issuance (`OFFLINE_LOCAL_ACK`, incl. lazy B10 BEGIN) | advances | holds (no wire) | `model.rs:1474-1481,1502` |
| 6 | drain send of a backlog doc, accepted | holds (advanced at OLA) | advances per doc | `model.rs:1626` |
| 7 | **drain-finalize END (DocType=10) mint+send** | **advances** (it is an ONLINE issuance) | advances on accept | `model.rs:1687` — **rev1's drain row was wrong here** |
| 8 | drain send, `[Reject]` (edges 6/14) | holds (kept — no rollback at drain) | holds | `backlog_drain.rs:923-925`; shift→RMR, node STOP |
| 9 | drain send, `[Superseded]` / held | holds | generator-chosen (row 3 applies to the DRAIN lane too — rev1 scoped it to online only, wrongly) | `model.rs:1735-1748` |
| 10 | **boot/reboot-driven sends** (SIGNED re-drives, full drain, END mint) | per rows 1-9 | per rows 1-9 — **the override and movers MUST reach boot's response feed** (today a hardcoded all-Ack loop, `interp.rs:1695-1721`) | FATAL #2 of the pass |
| 11 | `OperatorComplete(Accepted)`, ONLINE-origin hold | **advances at completion** to the held doc's own `unsigned_xml_sha256`; sfn is operator-supplied | holds (no wire) — consistency vs the row-3 choice is §5's adjudication point | `delivery_reservation.rs:1463-1474` |
| 11b | `OperatorComplete(Accepted)`, OFFLINE-origin hold | holds (zero seed writes) | holds | `delivery_reservation.rs:1463` (`if online`) |
| 11c | `OperatorComplete(NotAccepted)` | holds; doc→RMR | holds | `delivery_reservation.rs:1484-1488` |
| 11d | `OperatorComplete(NotAcceptedOffline)` | **REWINDS** to the held doc's `previous_hash` (possibly a non-doc T=112 value) + OLA-cohort cancel | holds | `delivery_reservation.rs:1489-1534`; sole caller of the rewind primitive (`:1506`) |
| 11e | `OperatorComplete(MacReseed(seed))` | **set to `seed`** (guard-B: `seed == active_tip` OR corroborated by the recorded `store`) | holds | `delivery_reservation.rs:1437-1447` |
| 12 | `Crash(Send)` — killed inside the wire await | holds (nothing committed) | **out-of-script generator choice** — the script leaf is NEVER consumed (counter+spy precede the hang, pop follows it, `scripted_dps.rs:168-186`) | MAJOR #7 |
| 12b | `Crash(Kvt1)` — send-Ack consumed, killed at the `last_chk` hang | **advances** (Sent committed) | **advances** (Ack consumed) | `interp.rs:1488-1491` — advance/advance hidden inside a Fault-class op |
| 12c | `Crash(Sign)` / `Crash(OfflineAck)` — no wire reached | per their commit points | holds | `interp.rs:1533-1605` |
| 13 | T=112 GRANTED, **backlog EMPTY** | advances to `sha256(request_xml)` (real, from `ReplenishSummary`) | **advances to the SAME value — a CONVERGENCE event** | §7; live **[N=1]** |
| 13a | T=112 GRANTED, **backlog NON-EMPTY** | advances | **DIVERGES** — the undrained backlog is already frozen on the pre-T112 chain | `bd PRRO_GATE-knk` (P1) — found by phase A itself; see §7 |
| 13b | T=112 AMBIGUOUS (reply lost) | holds (nothing persisted) | advances to synthetic `S` | the `2ds` divergence |
| 13c | T=112 while a fence/reservation is active | refused in-envelope (seed holds) — note the wire call itself DOES happen even in STOP_MODE (no node-mode gate, `offline_code_replenish.rs:173-229`) | §7 | verify Q6 |
| 14 | **totality clause**: every op not named above — `XReport`, `L5Probe`, refused/inert mints (D5 gate, mode refusals, closed shift, `DuplicateIdemKey` replays), `GoOffline`, session open/close bookkeeping — moves NEITHER tip | holds | holds | model apply arms enumerated in the verify pass |

Ordering inside row 1 (peer moves at the reply, we move at the later CAS) is observable exactly
once: `Crash(Send)`/`Crash(Kvt1)` land between the two — which is why rows 12/12b are not
optional decoration but the ordering witnesses.

## 5. Ambiguity and the operator — where divergences are MANUFACTURED

Rev1's "peer is definite, we are ignorant" survives, with the missing half filled in:

- **The choice lives on the leaf, out-of-script for crashes.** A NEW appended `WireResponse`
  variant (append-last — the corpus-preservation rule, `op.rs:53-60`, `strategy.rs:116-118`
  forbids changing existing arities or inserting arms) carries `peer: Took|NotTook` for held
  leaves; `Crash(Send)` gets the choice as a separate generator dimension because its script
  leaf is provably never consumed.
- **`OperatorComplete` is the adjudication point.** Nothing constrains the operator's claim to
  the peer's earlier branch — and that is a feature *for the online origin*: `Accepted`-vs-
  `NotTook` and `NotAccepted`-vs-`Took` manufacture exactly the real-world "operator guessed
  wrong" divergences, which then MUST earn the derived `-12` and be recoverable via corroborated
  MacReseed. These are trajectory pins of phase C.
- **The OFFLINE origin is the exception, and phase C.1 CONSTRAINS it.** The pass proved
  (MAJOR #6): `NotAcceptedOffline` after peer-`Took` rewinds our seed while the peer keeps the
  doc — and the resulting divergence has **no exit**: every later drain send earns `-12`, the
  hold is offline-origin, and MacReseed is fail-closed refused for offline origin
  (`MacReseedNotOfflineDefined`, `delivery_reservation.rs:1378-1380`). Until production grows a
  recovery story for that state, the generator constrains offline-hold completions to the peer
  truth. The unconstrained mode is phase D — and it is likely to file a **production** finding,
  because the operator CAN make that mistake in reality.

  > **MEASURED IN PHASE C-2 (2026-08-04) — the mechanism above is WRONG, and the constraint was
  > therefore NOT shipped.** C-2's annotated leaves made the state constructible for the first
  > time, so it could finally be driven against the real seam rather than reasoned about
  > (`phase_c1_offline_hold_the_peer_took_is_a_fork_with_no_exit`). What actually happens:
  > - there are **no "later drain sends"** — the completion's OLA-cohort cancel EMPTIES the backlog
  >   (successors → `CANCELLED`, the held doc → `RMR`);
  > - **MacReseed is refused one step earlier and for a different reason** — *"no held reservation
  >   rests"*, because the completion already released the hold. The `MacReseedNotOfflineDefined`
  >   origin guard this bullet named is never reached;
  > - the FN **does** park unrecoverably, but as `GoingOnline` + an RMR shift — issuance refused
  >   `NODE_GOING_ONLINE`, the drain a guarded no-op (AUD-K8-1 re-entry). Nothing about that park
  >   involves the peer.
  >
  > And the decisive fact: **production cannot see the peer's truth**, so its behaviour is
  > byte-identical whether the peer took the document or not — the same park is reachable today via
  > an ordinary `Superseded` drain, and has been all along. A generator constraint keyed on the
  > peer's truth would have removed freshly-won coverage while preventing nothing. The trajectory is
  > therefore GENERATED, with two pins holding the ground: one documents the park and REDs first if
  > production ever grows a way out, one drives the whole sequence through `run_harness` so every
  > oracle is known to survive it. Revisit if phase D ever turns the derived `-12` on
  > **generatively** — that is the world in which this bullet's reasoning would start to bite.

## 6. The forced `[BadHashPrev]` leaf — kept, and made consistent

The blast-radius inventory found **every** existing `-12` test drives the forced leaf at
*matching* tips (fresh-FN genesis or post-Ack), so a derived-only `-12` breaks all of them —
the leaf stays, as reaction-coverage. Rev2 adds the one rule that makes it consistent instead of
Byzantine: **when the forced leaf fires, the peer tip := the `store` value the message declares.**
The peer has stated its tip; from then on everything is ordinary axis behaviour: a
`MacReseed(local tip)` via disjunct (i) does NOT converge (faithfully — the real DPS would `-12`
again), while a corroborated `MacReseed(store)` DOES, and the next send succeeds. That closes
`5hc`'s success path **in phase B, without any ambiguity machinery** — the single biggest
scope win of the review.

## 7. Two decisions forced by evidence

- **T=112 granted is a convergence, not a chain-check.** The live H2 capture **[N=1]**: after an
  ambiguous T=112 (peer advanced, we stale), the FRESH follow-up T=112 was **ACCEPTED** and
  returned the same codes. So the peer does not `-12` a T=112 on a stale embedded tip — it
  accepts and re-bases; both tips land on `sha256(new request_xml)` and the divergence HEALS.
  This deletes rev1's implied "T=112 is chain-checked" and dissolves the ask-codes-`-12` problem
  (MAJOR #9) — the surface needs convergence semantics, not an override. Corollary worth pinning:
  **a granted T=112 is a legitimate divergence-healing move available even in STOP_MODE** (the
  service has no node-mode gate; only persist is fence-gated).

  > **CORRECTED BY PHASE A (2026-07-31, same day).** "Convergence" holds only with an **EMPTY**
  > backlog. Phase A's assertion found the other half within one run: with an **undrained** offline
  > backlog the replenish advances the chain while the backlog stays frozen on the pre-T112 link,
  > so it DIVERGES instead of converging — `bd PRRO_GATE-knk` (P1), shrunk to
  > `[OfflineServiceIn, Replenish(Granted), GoOnline([Ack,Ack])]`. Three grounded facts settled it
  > against my own initial reading: offline docs ARE chained on the wire (`emit_mac` puts the hash
  > inside `<MAC ID='code'>previous_hash</MAC>`); our own live smoke already recorded a drained
  > offline doc REJECTED for a mismatched MAC and polls around it
  > (`live_dps_extended_smoke.rs:2603-2612`) — but only for the T=112-then-offline order, not this
  > reverse one; and the reference client cannot hit it at all because it **re-anchors** each
  > document from a live `lastChk` at send time (`SendingOfflineChecks.cs:40,47-48`) where we
  > **freeze** `previous_hash` at sign time. What stays open is severity, not existence: whether
  > DPS would accept that T=112 at all, since its `<MAC>` is a value DPS has never seen — the
  > earlier [N=1] involved a merely STALE (previously-seen) tip, which is not the same input class.
  > The bd carries the five-step live probe.
  >
  > **PROBE RAN 2026-08-01 — SETTLED, and the answer narrows the finding.** Handed a T=112 whose
  > `<MAC>` is a value DPS has NEVER seen, DPS **refuses**:
  > `-12 ERROR_BAD_HASH_PREV  store <its own tip> chk <ours>`, and its tip does **not** move
  > (`live_probe_knk_t112_foreign_mac.rs`, TEST cabinet, FN 4000162280). So DPS **does**
  > chain-check the replenish request, the "accepted → backlog unsendable" branch is refuted live,
  > and `knk` drops to P2: what remains is that an operator cannot replenish with a diverged chain
  > and nothing tells them to drain first.
  >
  > **The two live observations are about DIFFERENT INPUT CLASSES and both stand.** On a
  > STALE-but-previously-seen tip (the H2 capture) DPS ACCEPTS and re-bases — which is what makes a
  > granted replenish a divergence-HEALING move. On a NEVER-SEEN value it refuses. Neither may be
  > restated as the general rule; the `[N=1]` marking on the earlier one is exactly what kept this
  > from being built on.
  >
  > Consequence for the axis: `ReplenishLeaf::Granted` is a free generator choice today, so the
  > model can emit a GRANTED replenish where production would earn `-12` — vacuous coverage, and
  > the reason `knk` first looked like a P1. Constraining that leaf belongs to phase B, where the
  > peer tip becomes authoritative.
- **The chain-check-first ordering is UNVERIFIED — do not hard-code it.** No repo evidence
  orders DPS's chain check vs business validation, and `FRESH_WEBCHECK_ANALYSIS.md:52` records an
  unresolved `-15`/`-12` pairing anomaly. Resolution: while diverged, the **generator does not
  emit business-reject leaves** — the ambiguous ordering simply never arises; the override
  applies to the leaves that remain. A live probe ("business-invalid doc on a diverged chain —
  which error wins?") goes to the backlog; if it ever runs, the constraint can be lifted.

## 8. Model side — this is a representation change, and rev1 undercosted it

Three verified clashes (MAJOR #8), each with its resolution:

1. **The rewind marker.** `NotAcceptedOffline` sets model `seed = None` as a structural marker
   (`model.rs:557`) — unusable for a peer comparison exactly where it matters. The model must
   restore the *symbolic* pre-cohort value, which requires retaining a per-advance symbolic
   history (who advanced to what: `Doc(lnd)` / `T112(ordinal)` / `PeerDeclared(store)`).
2. **The MacReseed arm.** Model sets `seed := synth(held_lnd)` (`model.rs:586`); prod installs
   the OPERATOR'S seed — which in the corroborated path is the PEER'S tip, a non-doc value. The
   arm becomes `seed := model.peer_tip`. Without this, step-4's very first trajectory diverges
   spuriously.
3. **Fault re-sync.** After any Fault op the model re-adopts from the DB
   (`adopt_fault_deferred`, `model.rs:1791-1933`) — but the peer tip is ENVIRONMENT state with
   no DB row. A `FuzzCtx → model` sync channel (the `sync_fence_active` pattern,
   `model.rs:1965-1974`) is required.

Convergence rule for the synthetic algebra: **every converging event assigns ONE fresh symbol to
both sides** (granted T=112, corroborated MacReseed); symbols are namespaced (`synth(lnd)`
per-doc, negative ordinals for T=112, the `PeerDeclared` constants) so aliasing is impossible.

## 9. Phasing, rev2 — each phase independently green, teeth named

> **PHASE A HAS SHIPPED** (PR #363, branch `fuzzer/peer-tip-phase-a`). It behaved as designed: the
> assertion caught a missing mover on its FIRST run (T=112 rides `ask_offline_codes`, a queue the
> send-side observer cannot see, so the peer fell a replenish behind), and then found
> `bd PRRO_GATE-knk` (P1) generatively. A third finding, `bd PRRO_GATE-01g`, surfaced during the
> 2048-case run from the PRE-EXISTING `release` differential — verified to reproduce on the parent
> branch without the axis, so the axis is exonerated; the corpus seeds it added perturb the proptest
> stream, which is how the region got explored. Both findings are pinned `#[ignore]`d against the
> FUTURE contract rather than papered over.

**Phase A — the axis, observed but silent.** `PeerState` in `FuzzCtx`; stub pool handle;
per-CALL movers for ALL wire surfaces including boot/drain/END and the forced-leaf
`peer := store` rule; NO override. New harness assert after every op: *absent any
divergence-creating event, `peer_tip == real active tip`*. This is the empirical load test of
§4 — rev1's "inert" step asserted nothing.
*Tooth:* flip one mover (offline issuance advances the peer) → the assert REDs across the suite.

**Phase B — derived `-12` + the `5hc` close.** Override on doc-send mismatch (`store` = peer tip
bytes); generator withholds business-reject leaves while diverged (§7). Divergence source in this
phase: the forced leaf's declared tip. Trajectory pin: forced `-12` → `MacReseed(local)` refused
to converge → derived `-12` → corroborated `MacReseed(store)` → **next send SUCCEEDS**.
*Tooth:* peer ignores the mismatch → the trajectory pin REDs.
*Deliverable:* `PRRO_GATE-5hc` closes here.

> **PHASE C — PART ONE HAS SHIPPED (2026-08-04, branch `fuzzer/peer-tip-phase-c`).** The model
> representation change (§8.1, §8.3) and both §9 trajectory pins landed; §8.2 and the C-2 leaf work
> did NOT, and the boundary is exact rather than approximate.
>
> **Landed.** (1) §8.1 — the rewind marker is gone. The model records each document's chain link at
> mint (`mint_doc`, `or_insert`, mirroring an immutable prod column), the `NotAcceptedOffline`
> rewind restores that link, and `adopt_fault_deferred` re-derives every link from the ledger. A new
> `run_harness` assertion projects the model's tip AND reality's onto {Genesis, Doc(lnd), NonDoc}
> after every op and demands they agree — the seed differential previously only asked whether the
> tip MOVED. (2) The model's own peer mirror, moved by the §4 movers table, asserted the same way
> against the phase-A harness peer. (3) Both §9 pins: the operator's wrong claim earning a derived
> `-12` and recovering through the corroborated reseed, and `Crash(Kvt1)` as an advance/advance that
> re-syncs.
>
> **Two rows of §4 were wrong in rev1 and are corrected here, each with a canary that fires:** the
> drain-finalize END advances the PEER as well as our seed (it is an online issuance minted at drain
> time), and the held/ambiguous leaf is generator-chosen in the DRAIN lane too, not only online.
>
> **The honest gate.** The model's peer comparison is suppressed once `peer_unknown` is set — a held
> or ambiguous wire outcome, or a crash inside the wire call, leaves the peer's acceptance genuinely
> undetermined and the model states that rather than guessing. That gate is exactly what phase C-2's
> `Took`/`NotTook` leaf exists to narrow, so the size of it IS the measure of what C-2 buys — and it
> was MEASURED rather than waved at, by counting both branches through a capstone run at the PR
> default: **online capstone 990 open / 10 closed (99%), offline capstone 783 / 217 (78%)**. So the
> mirror is asserted on the large majority of ops today, and C-2's leaf is worth roughly the
> remaining 1-22% — concentrated in the offline lane, which is where the held drain outcomes live.
>
> **§8.2 is deliberately NOT landed.** The `MacReseed` arm still reads `synth(held_lnd)` where it
> should read `model.peer_tip`. `MacReseed` is generator-EXCLUDED and every directed pin drives it
> through `operator_complete_macreseed`, which bypasses the model — so the arm is unreachable and
> its canary cannot be made to fire. It lands with the machinery that reaches it (an interpreter
> that supplies the corroborated seed the peer itself declared), not before: a fix whose teeth have
> not been watched to bite is exactly what this project's teeth rule forbids.

> **PHASE C — PART TWO HAS SHIPPED (2026-08-04, same branch).** The annotated leaf, the
> `Crash(Send)` dimension, and the C.1 adjudication. §8.2 remains deliberately unlanded (its arm is
> still unreachable — see part one's note).
>
> **The leaf.** `WireResponse::HeldWithPeer(Took|NotTook)`, appended last. Client-side it is
> `Superseded` **through the same `wire_to_result` arm**, so it adds no production contract to
> re-verify — the annotation rides in the stub's send queue entry itself (not a parallel queue that
> a future `push_send` site could forget to keep in lockstep) and reaches only the harness peer.
> `NotTook` leaves the run UNdiverged, which is the half that buys coverage: both the phase-A
> mismatch assertion and the model's mirror keep their teeth for the rest of the sequence.
>
> **`Crash(Send)`.** `Op::CrashSend(PeerTruth)`, appended last, because the scripted leaf is
> provably never consumed (the hang hook precedes the pop). The truth is applied to the document
> the stub recorded on the way in. Pinned in both directions, including what production does about
> it in EITHER branch: nothing automatic — the abandoned doc rests `ERROR_RETRYABLE`, a non-issued
> sibling, so the D5 write gate refuses the next issuance (`WRITE_GATE_SIBLING_PENDING`). The
> trajectory I first wrote — "and the FN carries on" — does not exist, and the pin says so.
>
> **The gate, re-measured with a baseline this time.** The part-one figure (990/10 online,
> 783/217 offline) is NOT comparable — it does not reproduce under the same counter, so both
> numbers below were taken today, one method, with and without the new symbols, at the PR default:
> **online 1137 open / 243 closed → 1267 / 158** (closed ops down 35%), **offline 1389 / 20 →
> 1318 / 0** (the gate never closes in the drain lane). The direction the part-one note predicted —
> that the offline lane is where the leaf pays — holds; the magnitudes it quoted do not.
>
> **C.1 was adjudicated AGAINST the spec and not shipped** — see the measured note in §5. The
> constraint was built, driven against the real seam, and removed: its stated mechanism does not
> occur, and production cannot see the peer's truth at all, so keying a generator constraint on it
> would cost coverage and prevent nothing.

**Phase C — the model mirror + annotated ambiguity.** §8's representation change; appended
peer-truth leaf variants; `Crash(Send)` out-of-script choice; C.1 constrains offline-hold
completions to peer truth (§5). Trajectory pins: operator-wrong-claim (online) earns `-12` and
recovers; `Crash(Kvt1)` advance/advance re-syncs.
*Tooth:* model MacReseed arm left at `synth(held_lnd)` → spurious divergence, RED.

**Phase D — ambiguous T=112 (`2ds`).** `ReplenishLeaf::Ambiguous` appended (its exclusion
comment cites RULING 2 §4's "known-red until a live capture lands" — **stale**, the capture
landed in #351; the true blocker was this axis — fix the comment here). Trajectory pin:
`[T112-ambiguous, OnlineSell → derived -12, MacReseed(corroborated S), OnlineSell → success]`,
plus the healing variant `[T112-ambiguous, T112-granted → converged, OnlineSell → success]`
**[N=1 live-anchored]**. Unconstrained offline-operator mode, expected to file a prod finding
(§5).
*Tooth:* the ambiguous leaf advancing OUR seed too → divergence pin REDs.

**Out of scope, stated:** `last_chk` stays fully scripted — the `[Ack, NotFound]` tail is the K4
empty-quittance hold ("accepted, quittance lagging"), which is temporally plausible, and nine
directed pins plus both generator slices depend on the held-at-SENT shape. A quittance-readiness
axis is a possible phase E, not assumed. A Byzantine peer (self-contradicting) stays excluded —
with one honest caveat from the pass: a scripted `last_chk` CAN still contradict the chosen peer
truth (Took + probe-NotFound); phase C's generator avoids emitting that combination, same policy
as §7.

## 10. Cost, restated after review

Rev1 said "seven movers"; the true count is **~14 event families, several per-CALL**, plus a
model representation change (§8) that rev1 did not cost at all. Phases A and B are still the
risk-bearers; C is where the model work lives; D is small once C exists. The two FATALs of the
review (OperatorComplete rows, boot-driven sends) are both phase-A scope — which is exactly why
phase A's assert exists before any override or model work is attempted.
