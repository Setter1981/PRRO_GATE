# A.1-prep DOSSIER — online-lane MAC-seed-fork (AUD-L2-1a / m1_02 / A2.4 prerequisite)

**Status: v3 — LOCKED (architect, 2026-07-05).** DOSSIER (evidence + analysis); decisions D1–D7 ADJUDICATED (§5.4 / §8), run through an 8-lens external audit whose findings are closed in **§9** (Go-after-amendment: D1/D2/D3/D5/D6/D7 sound, D4 re-spec'd — §9.4; no new STOP), and the two open ruling-requests resolved (§9.2b resolver = convergence-tick; §9.3 C10 = CONFIRMED + block-on-issued-unconfirmed). Companion spec: [`2026-07-04-a24-seed-fork-design.md`](./2026-07-04-a24-seed-fork-design.md) (v3, LOCKED). **A.3 is unblocked by this LOCK** (landing plan design §6; step 0 pre-LOCK re-confirms remain the first A.3 act).
**Role:** this dossier assembles machine-verified evidence + an option matrix for the architect to adjudicate → external audit → lock. It is **not** a code contract. No production code / tests / migrations were touched; the RED-pin was not un-ignored; the binding was not flipped.
**Adjudication summary:** design (A) / **Option 0 advance-at-SEND accepted**; discriminator **amended to the `server_fiscal_no` column** (not a state-set — RMR is ambiguous both sides of SENT, §5); D3(ii) verification sweep executed read-only (**no STOP** — single sfn writer confirmed, one sharpened A2.4 lockstep requirement, §5.3).
**Roadmap:** `docs/superpowers/plans/2026-07-04-pilot-path-roadmap.md` (LOCKED v2), step **A.1**.
**Provenance of anchors:** every `file:line` below was machine-verified by `grep`/`Read` against the working tree at `main` (`c9f96f4`). Where the source contract or Batch C cited a different line, the **machine-verified** anchor is used and the drift is noted.

---

## 0. Problem statement (fixed — not re-opened here)

The online inline-lane has a chain **SEED-FORK**. The chain seed
`node_state.last_known_unsigned_xml_sha256` is **read** as a doc's `previous_hash` at sign time
(`stage_sign.rs:286`) but **advanced** for online-origin docs only at the ACK transition inside
`stage_finalize` (`stage_finalize.rs:315`, gated `offline_fiscal_no.is_none()` @ `:288`). Two online
SELLs can both rest at `SENT` (empty-`data_sign` `lastChk` Hold), so `stage_finalize` never runs for
doc1 → the seed is never advanced → doc2 is signed against the same stale (pre-doc1) seed →
`doc2.previous_hash == doc1.previous_hash` = a **FORK**, not a chain. On convergence doc1 ACKs (seed
advances), doc2 hits the `stage_finalize` chain-continuity guard (`:304`) → durable `ChainSeedMismatch`
wedge at KVT2.

- **RED-pin (the gate):** `m1_02_online_seed_fork_a24_prerequisite`
  (`rust/prro/tests/kill_point_matrix.rs:2543`, `#[ignore]` @ `:2542`, barrier comment `:2516-2539`).
  It asserts the DESIRED chained property `doc2.previous_hash == doc1.unsigned_xml_sha256`
  (`:2614-2618`) and FAILS today. Un-ignoring it is the gate for flipping the prod binding.
- **Reachability proof (not ignored):** `m1_02_reachability_second_sell_while_first_rests_sent`
  (`kill_point_matrix.rs:999-1105`) proves two online SELLs on one FN both rest at `SENT` with distinct
  `server_fiscal_no` and `invariant_scan` returns zero violations — the fork precondition is live-reachable, not theoretical.
- **Dormancy:** the fork is **not reachable from a live request** — the inline lane is bound to
  `UnimplementedWritePath` (`runtime/supervisor.rs:188`), not an inline impl. See §7.

---

## 1. Seed lifecycle — full machine-verified trace

The seed is a single 32-byte column `node_state.last_known_unsigned_xml_sha256`
(`db/repositories/node_state.rs:40`, `Option<[u8; 32]>`).

### 1.1 Forward-writers (the ONLY places the seed moves forward)

| # | Writer method | Site | Class | Gate / condition |
|---|---------------|------|-------|------------------|
| W-a | `seed_prevhash` (pool) | `node_state.rs:134` | **bootstrap only** | **Zero** production `src/` call-sites — **tests-only** helper (there is **no** `prro fn seed-prevhash` CLI subcommand: `main.rs` exposes Version/Migrate/Doctor/Serve/Admin only — anchor corrected §9.6). Not a live-flow writer. |
| W-b | `update_last_known_xml_sha_tx` (tx) | `node_state.rs:163` | live forward-advance | 3 call-sites ↓ |
| — via W-b #1 | online ACK advance | `stage_finalize.rs:315` | **steady-state advance** | `offline_fiscal_no.is_none()` @ `:288`; seed = this doc's `unsigned_xml_sha256` |
| — via W-b #2 | offline per-doc advance | `stage_offline_ack.rs:409` | **steady-state advance** | on `Signed→OfflineLocalAck`; seed = this doc's `unsigned_xml_sha256` |
| — via W-b #3 | NC-03 recovery projection | `boot_phase.rs:1810` | **projection-site (recovery)** | node_state row LOST + ledger SURVIVED; projects seed from ledger tail, then BLOCKs node |

**Classification (per architect ruling):** advance-sites = **2 steady-state** (`stage_finalize:315`,
`stage_offline_ack:409`) **+ 1 projection-site** (`boot_phase:1810`). Site #3 is **not** a third
issuance advance — it is a pure reconstruction of derived state (a function of the issued-predicate via
the SSOT `last_issued_unsigned_xml_sha256`), gated behind a node BLOCK, one-shot at boot. It **cannot
diverge from the walk's final `expected` except under incomplete lockstep** (see §1b). **Spec
requirement:** NC-03 must never encode its own issued-predicate — only the shared SSOT.

> **STOP-point (b) — resolved.** Site #3 outside the two stages was the finding that triggered the
> STOP. Adjudicated by the architect: **accepted, design (A) augmented not invalidated**; reclassified
> as a projection-site. **STOP-check re-run clean:** no 4th forward-writer of the column exists (two
> parallel sweeps confirmed; the only `UPDATE node_state SET last_known_unsigned_xml_sha256` statements
> are `node_state.rs:136` [`seed_prevhash`] and `:169` [`update_last_known_xml_sha_tx`]).

### 1.2 Read-site (seed → previous_hash)

| Site | Anchor | Note |
|------|--------|------|
| `stage_sign` pin | `stage_sign.rs:286` (`let seed = ns.last_known_unsigned_xml_sha256;`) | Pinned into `pin_signing_inputs_tx` (`:302`) — THE canonical read where the seed becomes a doc's `previous_hash`. Read inside the pin tx (commit-stable snapshot). |

### 1.3 Assert-sites (chain-continuity / drift guards)

| Site | Anchor | Kind | Note |
|------|--------|------|------|
| online finalize guard | `stage_finalize.rs:304` | in-tx equality, fail-closed | `ns_row.last_known_unsigned_xml_sha256 != inputs.previous_hash` → `ChainSeedMismatch`. Online-origin only (inside `:288` gate). Advance @ `:315` follows. |
| offline drift `ensure!` | `stage_offline_ack.rs:389-395` | in-tx `ensure!`, fail-closed | `ns.…sha256 == prev` else "refusing to advance". Advance @ `:409` follows. **(Batch C cites this template as `stage_offline_ack.rs:361-368`; the file has since drifted — machine-verified location is `389-395`.)** |
| invariant_scan MAC-walk | `invariant_scan.rs:276-282` | read-only audit | Walks issued docs advancing `expected` (`:268-273`), then `node_seed != expected` → `Violation::ChainSeedMismatch`. `node_seed` read @ `:220`. |

### 1.4 Repo accessors that surface the seed

`node_state.rs:40` (field), `:71` (32-byte decode guard), `:249`/`:284` (SELECT `... as`), `:267`/`:302`
(`decode_chain_hash` into `NodeStateRow`) — reached via `node_state::get` / `get_tx`.

---

## 1b. Issued-predicate consumer table (NEW — mandatory per ruling)

The choice "issuance moment = SENT" (Batch C OQ#1) is **not** a point edit to `stage_send`: it
redefines online-issuance semantics, and **every consumer of the issued-predicate moves in lockstep**.
The codebase is already primed for this discipline — a single-source-of-truth const, "CANNOT diverge"
doc-comments, and a differential-fuzzer tooth. The table below is the complete consumer set an option-0
(or option-1) implementation must touch **in lockstep**; options 2/3 leave the predicate unchanged
(that is their whole point — and their cost, see §2).

**Current online-issued predicate everywhere = `state == "ACK"` (hardcoded literal, outside the SSOT const).**

| # | Consumer | Anchor | Current online rule | Edit under opt 0/1 (advance-before-ACK) | Under opt 2/3 |
|---|----------|--------|---------------------|------------------------------------------|---------------|
| C1 | `stage_finalize` advance+guard | `stage_finalize.rs:288,304,315` | advance @ ACK, `offline_fiscal_no.is_none()` gate | move advance to SENT; generalize gate → unified "already-advanced-at-issuance" predicate | unchanged (still advances @ ACK) |
| C2 | `invariant_scan` MAC-walk | `invariant_scan.rs:269` | `issued = state=="ACK"` (`// they only ever issue at ACK` @ `:263`) | online arm must treat SENT+ as issued, else FALSE `ChainSeedMismatch` when a doc rests at SENT | unchanged |
| C3 | `last_issued_unsigned_xml_sha256` SQL | `fiscal_documents.rs:933` (`state = 'ACK'` literal; fn @ `:916`, `ORDER BY lnd DESC LIMIT 1`) | online tail = last ACK | online arm literal must widen to SENT+ | unchanged |
| C4 | `OFFLINE_ISSUED_STATES` SSOT const | `fiscal_documents.rs:897` (7 elts, excludes ACK) | offline-only; doc-comment: *"Online-origin docs issue ONLY at ACK (handled separately, NOT in this set)"* | needs an online analog (see §7 config / ONLINE\_ISSUED\_STATES open decision) | unchanged |
| C5 | boot NC-03 projection | `boot_phase.rs:1751` (reads C3) | consumes C3 (SSOT) | **no edit if it stays SSOT-only** (ruling: never encode own predicate) | unchanged |
| C6 | fuzzer RefModel online advance | `tests/invariant_fuzzer/model.rs:270` (`if doc_state == DocState::Ack`) | model advances online seed @ ACK | must advance @ SENT to match prod, else differential oracle emits FALSE drift | unchanged |
| C7 | fuzzer D3 tooth | `tests/invariant_fuzzer.rs:120` (`teeth_d3_forked_set_matches_prod_const`) | guards **offline** set only — online advance-trigger **unguarded (blind spot)** | add an analogous tooth pinning model-online-advance-state == prod-online-advance-state | unchanged |
| C8 | webcheck replay `is_issued` | `tests/webcheck_replay.rs:757-761` (`rec.state == DocState::Ack \|\| (fs_mode!="ONLINE" && OFFLINE_ISSUED_STATES…)`) | online issued = ACK | online arm must accept SENT+ | unchanged |
| C9 | comment-pins | `stage_finalize.rs:280-288`; `backlog_drain.rs:930-937` | prose: "online-origin … fiscalise/issue at ACK" | reword to the new issuance moment | unchanged |

**`last_ack_unsigned_xml_sha256`** (`fiscal_documents.rs:871`) is **ACK-only by design** and its own
doc-comment says *"do NOT use this for the boot MAC-seed projection"*; sole reader is the doc-comment
cross-reference. It is **not** an issued-predicate consumer and must **not** be widened — it is retained
"for any ACK-only consumer". (Distinct from C3 `last_issued`.)

**Shape — ADJUDICATED (D4):** today C2 and C3 hardcode the online arm as a literal `ACK` *outside* the
SSOT const C4. The architect **LOCKED** a single shared predicate function
**`is_issued(state, offline_fiscal_no, server_fiscal_no)`** in `fiscal_documents.rs` (offline arm = the
existing `OFFLINE_ISSUED_STATES` const; online arm = the D3 `server_fiscal_no` predicate). **No**
`ONLINE_ISSUED_STATES` const (avoids a second namespace const — bias against namespace churn). Both
hardcoded literals (C2 `invariant_scan.rs:269`, C3 `fiscal_documents.rs:933`) die; C6/C8 call the fn;
C7 pins the model mirror against it. See §5.4 (D1–D7) + §5.2 (D3 discriminator).

---

## 2. Option matrix

Four options for closing the fork, in the fixed audit order. Legend: **crash-window** = what a crash
between stages leaves; **refusal-window** = behavior on a post-issuance DPS reject; **seed@Rej-after-SENT**
= what happens to the seed if a doc that already advanced is later rejected; **consumers** = §1b lockstep
set; **migration** = schema churn; **complexity**.

### Option 0 — advance-at-SEND *(Batch C design (A); architect-preferred; the RECOMMENDATION)*

Advance `node_state.…sha256` to the doc's `unsigned_xml_sha256` **inside the `Sending→Sent`
`with_immediate` envelope** in `stage_send`, guarded by a pre-advance drift-assert mirroring
`stage_offline_ack.rs:389-395`, applied **only on the fresh `Sending→Sent` `Applied` CAS** (never on
replay), then generalize the `stage_finalize` gate into a unified "already-advanced-at-issuance"
predicate so finalize skips both offline-ack and online-Sent docs.

- **Landing point (machine-verified):** the `if let WireDecision::Sent { server_fiscal_no } = &decision`
  block inside the 4-b `with_immediate` closure (`stage_send.rs:1373`), after the CAS `Applied`
  (`:1360-1369`) and alongside `set_server_fiscal_no_tx` (`:1373-1379`), before `transport_trace::complete_tx`.
  This closure already imports `node_state` and calls `node_state::set_mode_blocked_tx` — the seam is wired.
- **Prerequisite gaps:** (a) `SendInputs` does **not** currently carry `unsigned_xml_sha256` /
  `previous_hash` (`fetch_send_inputs_tx` must be extended, mirroring `fetch_finalize_inputs_tx`); (b)
  there is **no** fresh-Applied-vs-SentReplay distinction in `stage_send` today — the 4-b CAS accepts
  only `Applied` and a re-entering Sent doc is rejected far earlier at the 4-pre allowlist
  (`stage_send.rs:1062-1069`) as `StateConflict`, so replay never re-runs 4-b. The advance must reason
  about the crash-after-marker-before-4-b case.
- **crash-windows:** crash between SEND(advance) and ACK leaves a doc at SENT with the seed **already
  advanced** — this is the offline-symmetric "local commit crossed" resting state (mirror
  `OFFLINE_LOCAL_ACK`). Must be an accepted quiescent boundary under the M3b non-terminal pin (SENT is a
  transport state, not `PREPARED`/`SIGNED`/`ENCRYPTED`). Crash between the 4-pre marker and the 4-b CAS:
  the CAS re-runs on the still-`Sending` doc → single advance on the fresh `Applied` (idempotent by
  construction).
- **refusal-window / seed@Rej-after-SENT:** a doc that advanced at SENT then DPS-rejects → **manual-recon,
  NO seed-rollback** (mirror offline: advance-then-escalate, never un-advance; M3b crossed-local-commit
  pin). See §5 discriminator problem for the decidability wrinkle this creates.
- **consumers:** C1–C9 (full §1b lockstep set). C5 needs no edit (SSOT-only).
- **migration:** **none** required for the advance itself. A discriminator terminal state (§5) *may*
  need one — open decision.
- **complexity:** medium. One-time semantic simplification: the online-ACK-only special case
  **disappears** — the predicate becomes lane-uniform ("issued = crossed local-commit threshold";
  offline = `OFFLINE_LOCAL_ACK`+, online = `SENT`+). Symmetric to the landed M2-01 offline mechanic.

### Option 1 — advance-at-SIGN

Advance the seed at `stage_sign` (even earlier than SEND).

- ⚠ **Distinct crash/refusal windows from option 0.** A signed-but-not-sent doc has already moved the
  seed. This re-opens exactly the class the fuzzer fixes #192 / #196 closed: a post-sign refusal or crash
  leaves a **buried SIGNED** doc that advanced the seed. #192 (PR b1e8223) and #196 (PR b858d75) abort
  orphaned `SIGNED`→`Aborted`; under advance-at-SIGN the seed would have moved before the abort → a
  rollback-or-escalate decision on **every** post-sign crash, not just post-send. Contradicts the P1
  boot-resume semantics (buried-SIGNED must not resurrect).
- **crash-windows:** widest — every Sign→Send gap is a seed-advanced window.
- **seed@Rej-after-SENT:** worse — reject can occur at Send *and* the doc already advanced at Sign.
- **consumers:** C1–C9, but the issued-predicate would have to admit `SIGNED` (a non-terminal state the
  M3b quiescence pin forbids at rest) → **conflicts with a frozen pin.**
- **migration:** none intrinsic.
- **complexity:** high; **flagged INCOMPATIBLE** with the M3b non-terminal-doc quiescence pin + P1
  boot-resume semantics. Not recommended.

### Option 2 — serialize-SENT-per-FN *(Batch C fallback (B): acquire/sign-gate on prior-doc terminality)*

Gate acquire/sign of doc2 on the prior doc reaching a terminal/issued state (head-of-line stall).

- **crash-windows:** narrow (only one doc in flight per FN).
- **refusal-window / seed@Rej-after-SENT:** unchanged from today (advance stays at ACK) — **the current
  Rejected-pin survives verbatim, no discriminator problem.**
- **consumers:** **none** of C2–C9 change (predicate untouched). This is its appeal.
- **migration:** none.
- **complexity:** low to build, but pays a **permanent liveness tax**: `SENT` is a **legitimate resting
  state** (empty-`data_sign` Hold; pinned by `tests/invariant_scan.rs` `stuck_non_terminal_excludes_legit_resting_states`).
  Serializing forbids two docs resting at SENT → **head-of-line stall** exactly under the Hold path that
  produces the fork. A slow/held doc1 blocks doc2 indefinitely.

### Option 3 — seed-fence-at-sign (fail-closed if seed != prev of last issued)

Keep advance-at-ACK; add a fail-closed fence at sign refusing to sign doc2 if the seed doesn't match the
last issued doc.

- **crash-windows:** unchanged from today.
- **refusal-window / seed@Rej-after-SENT:** unchanged (advance @ ACK) — **current pin survives verbatim.**
- **consumers:** predicate untouched (C2–C9 unchanged); adds a new fence read at sign.
- **migration:** none.
- **complexity:** low-medium, but same **permanent liveness tax** as option 2 — the fence *is* a
  serialization: doc2 cannot sign until doc1 advanced the seed at ACK, so two docs cannot rest at SENT →
  head-of-line stall. Effectively option 2 enforced at sign instead of acquire.

### Matrix summary

| | opt 0 advance-SEND | opt 1 advance-SIGN | opt 2 serialize | opt 3 fence-sign |
|---|---|---|---|---|
| crash-window | SENT (=offline-symmetric rest) | **SIGNED (forbidden at rest)** | narrow | narrow |
| Rejected-pin | **reformulated + expanded** (§5) | reformulated, worse | **survives verbatim** | **survives verbatim** |
| consumers touched | C1–C9 (lockstep) | C1–C9 + `SIGNED` in predicate | none | none + fence read |
| migration | **none** (D3 = `server_fiscal_no` column reuse, no schema) | none | none | none |
| liveness | no tax (Hold ok) | no tax | **permanent HoL stall** | **permanent HoL stall** |
| verdict | **RECOMMEND** | INCOMPATIBLE (M3b + P1) | dispreferred (liveness) | dispreferred (liveness) |

**Architect a-priori (from ruling, recorded not decided):** (A)/option-0 holds. The trade is a
**one-time semantic simplification** (online-ACK special case vanishes; predicate becomes lane-uniform;
mirrors landed M2-01) **vs a permanent liveness tax** (options 2/3 forbid the legitimate SENT rest). The
matrix must show the lockstep-cost honestly (§1b), but the recommendation is not shifted to 2/3.

---

## 3. Batch C verbatim excerpt + assessment

Source: `docs/reviews/legacy-2026-06/REMEDIATION-PLAN-2026-06-13.md`, section
`### DEFERRED to A2.4 — AUD-L2-1a: online-lane MAC-seed-fork REDESIGN` (`:140`).

**Design (A) (`:153-162`, verbatim):**
> **(A)** Advance `node_state.last_known_unsigned_xml_sha256` to the doc's `unsigned_xml_sha256` INSIDE
> the `Sending→Sent` `with_immediate` envelope in `stage_send` (the online 'issuance' moment, symmetric
> to offline-ack), guarded by the same pre-advance drift assert as `stage_offline_ack.rs:361-368` (read
> ns seed in-tx, assert `== doc.previous_hash`, else fail-closed). Advance ONLY on the fresh
> `Sending→Sent` `Applied` CAS (never on SentReplay re-entry …). Then **generalize** the
> `stage_finalize.rs:285` `offline_fiscal_no.is_none()` gate into a unified 'already-advanced-at-issuance'
> predicate …

**Fallback (B) (`:163-164`, verbatim):**
> **(B) fallback** only if (A) proves unsafe: gate acquire/sign of doc#2 on prior-doc terminality (hurts
> liveness under the Hold path — head-of-line stall — so (A) is preferred).

**Open questions (`:166-174`, verbatim):**
> 1. **Issuance moment = SENT vs KVT1?** Recommend SENT … KVT1 reopens the SENT-SENT fork window …
> 2. **REJECTED-after-SENT policy:** a doc whose seed advanced at SENT but is later DPS-rejected … must
>    escalate **manual-recon (NO seed rollback)** — mirror offline … Confirm no compensating rollback …
> 3. **Landing:** as an A2.4 prerequisite (RED-pin un-ignored + fix) — the lane must not go live forked.

**SW-4 (`:176-186`, verbatim summary):** split `ChainSeedMismatch` out of the `StructuralDrift` arm in
inline and route to `escalate_fn_to_manual_recon` (mirror `online_convergence`). See §5b.

### Assessment — does design (A) hold after M3b / #192 / #196 / P1?

**Yes — it holds, and is strengthened, with two additions.**

1. **M3b crossed-local-commit-threshold pin:** (A) is *symmetric* to the landed offline mechanic, not
   parallel to it. Offline advances at `OFFLINE_LOCAL_ACK` and a later drain reject escalates without
   un-advancing (`OpenedLocalPendingDrain` universe). (A) makes SENT the online analog. **Consistent.**
2. **#192 / #196 / P1 (post-sign orphaned SIGNED / boot-resume):** these acted on the **offline** lane's
   buried-SIGNED surface. (A) advances at **SEND**, *after* sign — so it does **not** widen the SIGNED
   crash window (that is option 1, rejected). A SENT-resting doc is a transport state, not a buried
   SIGNED. **No conflict** with the P1 buried-SIGNED-must-not-resurrect pin.
3. **Addition 1 — the projection consumer (STOP-point b):** the boot NC-03 projection
   (`boot_phase.rs:1810`) + the `invariant_scan` walk are issued-predicate consumers absent from Batch
   C's design text. Design (A) survives but its lockstep-consumer set grows by these (§1b C2/C5) plus the
   fuzzer/replay consumers (C6–C8). **This is consumer-completeness, not scope-creep** (lesson AUD-L5-1
   EDIT-E).
4. **Addition 2 — the discriminator problem (§5):** extending M2-N2b's "REJECTED is issued" rule to the
   online lane is not free — online REJECTED is ambiguous as a state in a way offline REJECTED is not.
   Batch C OQ#2's answer (post-SENT reject → manual-recon, no rollback) simultaneously *resolves* the
   decidability if post-SENT reject lands in a state distinguishable from pre-SENT REJECTED. Design (A)
   holds; the Rejected-pin **expands** (new post-SENT branch), it does not break.

**Verdict:** design (A) is **not invalidated by any constraint** (STOP-point (a) does not fire). It is
**augmented** by the §1b consumer set and the §5 discriminator (adjudicated D3 = `server_fiscal_no` column).

---

## 4. RS3 A2.x status (A2.2 / A2.3 / A2.4 / A2.5)

**Path correction:** the RS3-A2 decomposition lives in `docs/superpowers/plans/2026-06-09-rs3-a2-implementation.md`
**§4** (`:207-218`) — **not** `docs/superpowers/specs/…` (that path does not exist). The source contract's
`specs/…§4` citation is wrong.

| Unit | Definition (verbatim §4) | Anchor | Status |
|------|--------------------------|--------|--------|
| **A2.2** | `stage_acquire` shift-link **+** C1-finalize hook (`stage_finalize::run_with_shift_transition`) | `:215` | **NOT DONE** — `run_with_shift_transition` absent from `src/`; listed as future "Successor" (`a2-1b-core-impl.md:6`) |
| **A2.3** | Offline-ack + Refused arms (`PostSignRoute::Offline`→`OfflineLocalAck`; `Refused`→typed error + terminalise) | `:216` | **NOT DONE** — future "Successor" |
| **A2.4** | **Production binding + inbox-terminalise audit** — replace `UnimplementedWritePath` with `InlineWritePath`; four-variant gate test is a HARD merge gate; **"Flip-the-switch piece … must land last"; mandatory external review** | `:217` | **NOT DONE** — binding still `UnimplementedWritePath` (`supervisor.rs:188`) |
| **A2.5** | Resume-path coverage (`WorkerProcessResult::Resumed` through full chain; no double-lnd, no re-INSERT) | `:218` | **NOT DONE** — future work; roadmap still requests status |

Predecessors MERGED (`a2-1b-core-impl.md:5`): A1 · A1Z · A2.0 · A2.1a · A3 · A4 · C1 · C2. A2.2–A2.5 are
all in the "Successors" (future) list. The 2026-07-04 roadmap (`:35`) still frames A2.4 as upcoming.

**NC-02 deferral (confirmed points at A2.2):** `rust/prro/tests/backlog_drain_state_dispatch.rs:3538-3540`
— `#[ignore = "NC-02: doc_type/wall-clock-aware ER budget deferred to A2.2/M5 (see dossier); pins the
future unbounded-for-shift contract — RED today"]`. This is the ≥1 deferral the roadmap requires the
dossier to surface.

**RS3 definition of A2.4 = binding flip + inbox-terminalise audit (§6 of the source contract):** confirmed
verbatim above (`:217`). A2.4 is the second half of this work (the flip); **A.1-prep produces the design
for the seed-fork that gates it**. A.3 of the roadmap = the flip itself + the inbox-terminalise audit
(every real-failure arm drives inbox non-`NEW` + audited; the four-variant gate test).

---

## 5. Discriminator problem (NEW — mandatory per ruling §5; **ADJUDICATED D3**)

Extending M2-N2b ("REJECTED / RMR are issued for offline-origin") to the online lane hits a fork the
offline lane does not have: the offline state-set trick works because **every** member of
`OFFLINE_ISSUED_STATES` (`fiscal_documents.rs:897`) entails "this doc crossed `OFFLINE_LOCAL_ACK`" — so
even REJECTED there *unconditionally* means "seed already advanced". For online under option 0 that is
**not** true, and — per the architect's D3 verification — it cannot be repaired by state routing.

> **ADJUDICATION D3 (amended from the DRAFT):** the discriminator is **NOT** a state-set — it is the
> existing column **`server_fiscal_no`**:
> `online-issued ⟺ offline_fiscal_no IS NULL AND server_fiscal_no IS NOT NULL AND server_fiscal_no != ''`.
> Rationale + the verification sweep are in §5.2–§5.3. The DRAFT's "route post-SENT reject to a
> distinct-from-REJECTED state → decidable" was **wrong on the mechanism**: RMR is ambiguous on *both*
> sides of SENT (§5.1), so no state-set predicate is decidable for online. The RMR **routing** of a
> post-SENT reject is still adopted (§5.2) — but decidability comes from `server_fiscal_no`, not from the
> resting state.

### 5.1 Machine-verified reachability of online REJECTED / RMR

| Transition | Anchor | Regime | Seed under opt-0 |
|------------|--------|--------|------------------|
| `(Sending, Rejected)` | `fiscal_documents.rs:256`; the **4-b post-wire CAS is `stage_send.rs:1360`** (`Sending → target`), target `Rejected` via the routed decision (`:1522` `synthetic_rejected_decision` builds the `ErrorRetryable→Rejected` override — **not** the 4-b CAS; anchor corrected §9.6) | **pre-SENT** | **NOT advanced** (advance only on `WireDecision::Sent`) |
| `(Sent, Rejected)` | `fiscal_documents.rs:190` — **legal edge, ZERO prod invokers** (sole citer = table-pin test `repo_fiscal_documents_state_cas.rs:322`) | **post-SENT** | **advanced** — an **open door** a future owner could route into, silently breaking state-decidability |
| `(ErrorRetryable, Rejected)` | `stage_send.rs:1558,1605` (retry-exhaustion) | **pre-SENT** (ErrorRetryable is a send-failure state, never reached Sent) | **NOT advanced** |
| `(Sent, RequiresManualReconciliation)` | `fiscal_documents.rs:199` — **live invoker:** boot-probe **W11 PR-2b** (`lastChk` id-mismatch → operator handoff; mismatch surface `kvt2_confirm.rs:231,339,361`) | **post-SENT** | **advanced** |
| `(ErrorRetryable, RequiresManualReconciliation)` | `fiscal_documents.rs:244` | **pre-SENT-reachable** (ErrorRetryable is a pre-Sent transport-park state) | NOT advanced |
| shift-level RMR escalation | `escalate_fn_to_manual_recon` `backlog_drain.rs:2399`, called by `online_convergence.rs:268` on post-Sent `Kvt2→Ack` ChainSeedMismatch (doc **stays at KVT2**, shift → RMR) | **post-SENT** | **advanced** |

Two facts (architect hand-verified, machine-confirmed here) that the DRAFT §5.1 missed and that **kill
the state-set discriminator**:

1. **`(Sent, Rejected)` @ `:190` is a *legal* edge** with zero production invokers today — harmless now,
   but an open door: a future owner routing a post-SENT reject there would put an *issued* doc into
   `REJECTED`, silently breaking any "REJECTED ⇒ non-issued" predicate.
2. **Doc-level RMR is already ambiguous:** `(Sent, RMR)` @ `:199` **is** invoked post-SENT by the live
   W11 PR-2b boot-probe, while `(ErrorRetryable, RMR)` @ `:244` is reachable **pre-SENT**. So RMR straddles
   SENT — "keep REJECTED purely pre-SENT" does **not** rescue the predicate, because RMR itself is
   reachable from both sides.

The lnd stamp being *"Atomic with the post-wire CAS `Sending → Rejected`"* is pinned by
`tests/invariant_fuzzer.rs:344-354` (the `node_state.rs:186`/`:204` doc-comment cited earlier is
`set_mode_blocked_tx`, not the lnd stamp — anchor corrected §9.6) — a pre-SENT reject consumes the lnd but
does **not** advance the seed. The current pin *"lnd consumed, seed NOT advanced"* therefore **survives
verbatim for the pre-SENT case**.

### 5.2 Why the state-set predicate is undecidable — and the `server_fiscal_no` discriminator

Under option 0 the issued-predicate must mean "online doc is issued iff it reached SENT+". No **state
string** decides that: `REJECTED` (via the latent `:190` edge) and `RMR` (via `:199`) are both reachable
post-SENT (issued) *and* pre-SENT/side (non-issued). The offline lane escapes this only because its
issued-set members all entail crossing `OFFLINE_LOCAL_ACK`; online has no such state-closed set.

**Resolution (D3):** use `server_fiscal_no`. `set_server_fiscal_no_tx` (`fiscal_documents.rs:1773`, SQL
`:1778`) has **exactly one caller** — `stage_send.rs:1374` — and it runs in the **same 4-b
`with_immediate` tx** as the CAS `Sending→Sent`. Under option 0 the seed advance lands in that same tx, so
**`server_fiscal_no` set ⟺ seed advanced, by construction, atomically** — a predicate that is
**state-independent** and immune to any future terminal routing (the `:190`/`:199` doors cannot break it,
because a rejected/RMR doc that never sent has no sfn, and one that did send has both sfn *and* an advanced
seed). Post-SENT reject **routing** still follows the `online_convergence` pattern (doc stays Sent/Kvt2,
shift → RMR, no seed rollback) — the Rejected-pin **expands** with that branch, does not break.

The codebase **already treats this as an invariant**, which is why the discriminator is a promotion, not a
new rule: `invariant_scan.rs:195` flags `state='ACK' AND (server_fiscal_no IS NULL OR ='')` as a
violation; `boot_phase.rs:2558` calls a SENT doc with `server_fiscal_no = None` a *"structural breach"*;
NC-04 (`boot_phase.rs:2270-2290`) states *"SENT-with-NULL-sfn is unreachable in healthy code
(SENT ⇐ WireDecision::Sent stamps sfn)"* and BLOCKs a malformed tail.

### 5.3 D3(ii) verification sweep + D5 residual (executed read-only, per ruling action 3)

**D3(ii) — sfn-invariant sweep result: CONFIRMED (no STOP).**

- **Writer side — single writer confirmed.** The only production writer of
  `fiscal_documents.server_fiscal_no` is `set_server_fiscal_no_tx` (one caller, `stage_send.rs:1374`). The
  two other `server_fiscal_no` writes in the tree are **not** it: `server.rs:613` is a `#[cfg(test)]` helper
  (`seed_ack`, `server.rs:587`), and `transport_trace.rs:206,490` UPDATE the **`transport_trace`** table
  (a different table's column).
- **SENT-without-sfn side — no live hole; one sharpened A2.4 requirement.** In the *live* staged lane every
  online doc reaches `Sent` via `WireDecision::Sent`, which stamps sfn atomically (and the NC-04 /
  `invariant_scan` guards fail-closed on any malformed SENT). **However**, the legal edges
  `(Sending, Kvt1)` (inline fast-path, `fiscal_documents.rs:1763`) and `(ErrorRetryable, Sent/Kvt1)`
  (re-send) reach an issued state, and stage_send stamps sfn **only** on the `WireDecision::Sent` arm — the
  `Routed` arm (`stage_send.rs:1388`) does not. Today that is harmless (the inline lane is dormant; retries
  re-enter via `ErrorRetryable→Sending→Sent` which stamps sfn). **For A2.4 this becomes a hard requirement:
  every online issued-forward edge — including the inline `Sending→Kvt1` fast-path — must stamp
  `server_fiscal_no` atomic with the seed advance (the same lockstep).** This does **not** refute the
  invariant (no second writer, no live sfn-less issued doc), so **D3 holds**; it is documented as an
  implementation obligation, not a STOP.
- **`''` (empty) semantics — consistent.** The discriminator uses `IS NOT NULL AND != ''`, matching
  NC-04's malformed-tail handling and `invariant_scan.rs:195`.

**D5 — NC-03 ordering + interleaving residual (named, for the LOCKed spec).** `ORDER BY lnd DESC LIMIT 1`
(`last_issued_unsigned_xml_sha256`, `fiscal_documents.rs:933`, fn @ `:916`) is **sufficient**: advances are
monotonic in `lnd` under single-writer + the fail-closed drift-assert. **Residual to carry into the LOCKed
spec:** the "ER-parked predecessor" interleave — if a worker can sign doc2 while doc1 rests **pre-SENT**
(`ErrorRetryable`), doc2's `previous_hash` is stale and its SENT drift-assert fails **after** the wire call
(DPS accepted, local refuse = ambiguous). This hazard **pre-exists** under advance-at-ACK (it wedges later,
at KVT2); option 0 only surfaces it earlier. The spec must (i) machine-verify interleave reachability (the
worker's doc-selection order under ER-parking) and (ii) choose: a **narrow gate** "do not sign while a
pre-SENT doc rests on the FN" (which — unlike option 2 — does *not* tax the SENT+ Hold path; architect's
prior) **or** a fail-closed route → manual-recon. Not resolved here; it is a spec obligation.

### 5.4 Adjudicated decisions (D1–D7)

All seven DRAFT open decisions are now **LOCKED** by the architect (design accepted with the §5 D3
amendment). Full text lives in the companion design doc (v2 ADJUDICATED); summary:

| # | Decision | Ruling |
|---|----------|--------|
| D1 | Issuance moment | **LOCKED = SENT** (KVT1 reopens the fork window; SENT = local-commit crossing, symmetric to `OFFLINE_LOCAL_ACK`) |
| D2 | Rejected-pin | **LOCKED** wording (§8.2); citers: root `CLAUDE.md` M3b persistence-para (**NB: `.claude/CLAUDE.md` lacks it — do not touch**), barrier prose `kill_point_matrix.rs:2516-2539`, roadmap A.1 constraints; updates ride A.3 code |
| D3 | Discriminator | **`server_fiscal_no`** column (not state-set); remove latent `(Sent,Rejected)` edge + table-pin test; D3(ii) verification obligation (§5.3) |
| D4 | Predicate shape | **shared `is_issued(state, offline_fiscal_no, server_fiscal_no)` fn** in `fiscal_documents.rs`; **no** `ONLINE_ISSUED_STATES` const; both hardcoded literals (C2 `invariant_scan.rs:269`, C3 `fiscal_documents.rs:933`) die |
| D5 | NC-03 ordering | **sufficient**, with the interleave residual above carried into the spec |
| D6 | Config surface | **LOCKED = hardcoded DI-swap, NO config knob** (Frozen #10 drift hazard; rollback = gated code revert); operator controls = Phase D |
| D7 | Fuzzer tooth | **approved** — pins model `is_issued` mirror == prod fn (both arms), paired (pos+neg), rides the lockstep consumer commit |

---

## 5b. SW-4 — inline ChainSeedMismatch mis-classification

**Current state (machine-verified):** the inline lane folds `ConfirmError::ChainSeedMismatch` into the
**same arm** as `ConfirmError::StructuralDrift` (`inline.rs:754-780`) → `terminalise_inbox(… "INLINE_ADVANCE_DRIFT" …)`
+ `FiscalError::Internal { code: REPLAY_LEDGER_DRIFT }` (HTTP 500), doc left durably at `Sent`/`Kvt2`,
**NO** escalation to `RequiresManualReconciliation`. The arm's own comment concedes the gap and notes the
lane is DORMANT.

**(Anchor correction:** the source contract cites `inline.rs:703-728`; the machine-verified arm is
`inline.rs:754-780`. `:703-728` is the earlier online `Sent→ACK` Proceed block.)**

**Every LIVE owner routes ChainSeedMismatch to escalation:** `online_convergence.rs:251` (arm) →
`escalate_fn_to_manual_recon` (**called at `online_convergence.rs:268`**, **defined at
`backlog_drain.rs:2399`**, `pub(crate)`, idempotent — early-returns `Escalated` if already RMR @ `:2411`),
plus boot-KVT2 and drain owners.

**Plan (rides the A2.4 gate):** split `ChainSeedMismatch` out of the `StructuralDrift` arm in
`inline.rs` and route it to `escalate_fn_to_manual_recon` (mirror `online_convergence.rs:251`).
Reachability = A2.4-only (inline lane is `UnimplementedWritePath` until the flip); the RED-pin +
`supervisor.rs:188` barrier gate it. **SW-4 travels the same gate as the seed-fork fix + binding flip.**

---

## 6. A2.4 = binding flip + inbox-terminalise audit (scope of roadmap A.3)

Per §4, A2.4 (`plans/…rs3-a2-implementation.md:217`) is the flip-the-switch piece. Its second half — the
**inbox-terminalise audit** — requires every real-failure arm to drive the inbox to a non-`NEW` audited
state; the four-variant gate test is a HARD merge gate; the piece carries **mandatory external review**
(binding + replay-forever risk) and **must land last**. A.1-prep (this dossier + the DRAFT design)
produces only the **seed-fork design that gates the flip**; the flip + audit is roadmap A.3.

---

## 7. Config-knob / binding surface (open decision — do NOT invent design)

**Machine-verified current state:**

- The production WritePath binding is **hardcoded**, unconditional:
  `let write_path: Arc<dyn WritePathEntry> = Arc::new(UnimplementedWritePath);` (`supervisor.rs:188`),
  preceded by an A2.4-prerequisite comment barrier (`:180-187`), threaded into every ingress
  `IngressState` (`:429`). `UnimplementedWritePath::fiscalize` always returns
  `FiscalError::NotImplemented` (`seam.rs:240`).
- **`InlineWritePath` does not exist as a type** — no `struct` / `impl WritePathEntry for InlineWritePath`
  anywhere in `src/`; the name appears only in doc-comment prose (`inline.rs:3-4`).
- **No config knob** selects the binding: `SupervisorCfg` (`config/mod.rs:265`) has 5 fields (none
  write-path); `AppConfig` (`config/mod.rs:10`) has none. `supervisor.enabled` gates whether the spine
  runs at all, not which impl is bound (both paths yield `UnimplementedWritePath`).

**ADJUDICATED (D6): hardcoded DI-swap, NO config knob.** A runtime write-path switch on a fiscal
edge-device is a Frozen #10 (accidental-drift) hazard; rollback = revert the flip through the same gated
external-review process; an off-switch adds nothing over stopping the service (`UnimplementedWritePath`
already refuses everything). Operator controls, if the pilot needs them, are a separate Phase-D spec.

---

## 8. Adjudicated decisions (D1–D7) — LOCKED

All seven decisions the DRAFT raised were **adjudicated by the architect** (design accepted with the §5
D3 amendment). The decision table is in §5.4; the load-bearing wording is captured here for citers.

- **D1 issuance moment = SENT** (LOCKED). **D5 NC-03 ordering = sufficient** (with the §5.3 interleave
  residual carried into the LOCKed spec). **D6 config = hardcoded DI-swap, no knob** (§7). **D7 fuzzer
  tooth approved** (§1b C7 / §5.4).
- **D2 Rejected-pin (LOCKED wording):** *"pre-SENT reject → `REJECTED`, lnd consumed, seed NOT advanced
  (pin survives verbatim); post-SENT reject → manual-recon escalation, seed NOT rolled back (pin
  expands)."* Citers to update **with the A.3 code, not earlier**: root `CLAUDE.md` M3b persistence
  paragraph (**NB — `.claude/CLAUDE.md` does *not* carry this paragraph; do not touch it**), barrier prose
  `kill_point_matrix.rs:2516-2539`, roadmap A.1 constraints.
- **D3 discriminator = `server_fiscal_no`** (not a state-set), plus removal of the latent
  `(Sent, Rejected)` edge (`fiscal_documents.rs:190`) + its table-pin test, plus the §5.3 D3(ii)
  verification obligation. Full ruling in §5.2.
- **D4 predicate shape = shared `is_issued(state, offline_fiscal_no, server_fiscal_no)` fn**, no
  `ONLINE_ISSUED_STATES` const (§1b).

---

## Appendix A — anchor drift (source contract / Batch C → machine-verified)

| Cited | Machine-verified | Note |
|-------|------------------|------|
| `stage_offline_ack.rs:361-368` (drift assert) | `stage_offline_ack.rs:389-395` | file drifted since 2026-06-13 |
| `stage_finalize.rs:285` (finalize gate) | `stage_finalize.rs:288` (gate); advance `:315`; guard `:304` | |
| `inline.rs:703-728` (SW-4 arm) | `inline.rs:754-780` | `:703-728` is the Sent→ACK Proceed block |
| `escalate_fn_to_manual_recon` @ `online_convergence.rs:251` | **defined** `backlog_drain.rs:2399`; **called** `online_convergence.rs:268` (arm @ `:251`) | |
| `supervisor.rs:180` (binding) | `supervisor.rs:188` (binding); `:180-187` comment | |
| `docs/superpowers/specs/2026-06-09-rs3-a2-*.md §4` | `docs/superpowers/plans/2026-06-09-rs3-a2-implementation.md:207-218` | `specs/` path does not exist |
| RED-pin fn `2541-2543` | `kill_point_matrix.rs:2543` (fn), `:2541` `#[tokio::test]`, `:2542` `#[ignore]` | consistent |

---

## 9. External-audit amendment (Go-with-amendments) — LOCKED (architect, 2026-07-05)

**Audit verdict:** **Go-after-amendment.** The 8-lens external audit found D1/D2/D3/D5/D6/D7 **sound** and D4 **needs-work** (SQL literal cannot host a Rust fn — §9.4). This section closes every finding: three HIGH design arcs (§9.1 MAC-recovery, §9.2 D5-gate re-spec, §9.3 C10), the D4 re-spec (§9.4), the MED set + A.3 implementation checklist (§9.5), and the LOW/hygiene + anchor-drift (§9.6). All anchors machine-verified against `main` (`909fc05`, the amendment base — the dossier body above was verified against `c9f96f4`; the two differ only cosmetically, noted per-anchor). **No new STOP fires:** the §9.1 STOP-gate (1.1) is CLEAR — Variant P passes every constraint; nothing in §9 invalidates D1–D3 / D5–D7.

### 9.1 HIGH — MAC-recovery (W10.4) × advance-at-SEND drift-assert  **[STOP-gate 1.1: CLEAR]**

**The missed consumer / constraint.** `run_mac_recovery` (`mac_recovery.rs:351`) re-signs an ONLINE doc against the DPS-supplied hash on a `-12` (`ERROR_BAD_HASH_PREV`) and re-sends it. It is an issued-predicate-adjacent **constraint** on Option 0 that fell out of the Batch-C design text and the §1b table — not merely a consumer to reword.

**Mechanics (machine-verified).** attempt#1 → DPS `-12` → `Routed{MacRecovery}` (doc parks `ErrorRetryable`; seed **NOT** advanced — advance is `WireDecision::Sent`-only). The in-run loop (`stage_send.rs:953-987`) invokes the orchestrator: regex-extract `H_dps = store {64hex}` (`:358`); re-sign against `H_dps` (`:438-452`); **MR-PERSIST** (`:511-521`) overwrites `fiscal_documents.previous_hash := H_dps` + `unsigned_xml_sha256 := sha₁` — the **node seed is untouched**. `Resigned → continue` (`:987`) → attempt#2 re-sends **in the same `run()`**, DPS-accepted → the standard 4-b `Sending→Sent` CAS (`:1360`) + `set_server_fiscal_no_tx` (`:1373`).

**The defect (deterministic).** A `-12` means `doc.previous_hash₀ (= seed_sign) ≠ H_dps` **by definition**. Under Option 0 the new pre-advance drift-assert `ns.seed == doc.previous_hash` runs on attempt#2's Sent CAS, where `doc.previous_hash = H_dps` (MR-PERSIST) and `ns.seed = seed_sign` (unmoved — same run, single-writer FN lease). So it reduces to `seed_sign == H_dps` → **guaranteed false on every successful recovery**, firing **after** the wire call (DPS already accepted) → ambiguous local-refuse → the recovered doc wedges. The M2-X1 guard rationale (`:386-398`, *"finalize advances the seed AFTER"*) was written against advance-at-ACK and does not cover this.

**Variant matrix.**

| Variant | Mechanic | Verdict |
|---|---|---|
| **P — recovery-aware advance** *(architect prior; ADOPT)* | For `mac_recovery_attempts >= 1`, the SEND advance does **not** gate on `ns.seed == doc.previous_hash` (recovery *deliberately* voided that premise by adopting `H_dps`); it treats the DPS re-sign as a **re-anchor** and advances `ns.seed := doc.unsigned_xml_sha256` (`sha₁`) on the fresh `Sent` `Applied` CAS. Advance **target is identical** to a normal doc (own re-signed sha) → Option 0's advance code is unchanged; only the pre-advance gate takes a recovered-doc branch. | **PASSES all constraints.** |
| R — resync in MR-PERSIST | MR-PERSIST also reseeds `ns.seed := H_dps` so attempt#2's assert passes verbatim. | **Rejected** — reseeds **pre-issuance** (before attempt#2 is DPS-accepted); a failed/crashed attempt#2 leaves `ns.seed` advanced for a **non-issued** doc → breaks advance-at-issuance, the invariant Option 0 rests on. |
| E — escalate recovered doc | A recovered doc that would trip the assert escalates to manual-recon. | **Rejected** — defeats W10.4's purpose (auto-recover `-12` without an operator); every `-12` → manual = a liveness regression. Retained only as the fail-closed fallback if P's single-writer premise is ever violated. |

**Why Variant P passes — constraint by constraint.**
1. **No silent corruption.** The only hazard is `ns.seed` being advanced by a *different* doc between this doc's sign and attempt#2. It cannot: the loop is **in-run** under the single-writer-per-FN lease → no other doc issues in that window → `ns.seed == seed_sign` at attempt#2, and reseeding to `sha₁` clobbers nothing. (Defense-in-depth, if a multi-worker dispatcher ever lands per ADR-M3-A10 §4: assert `ns.seed == old_previous_hash`, the pre-recovery hash already recorded in the `MAC_RECOVERY_RESIGNED` audit payload `old_previous_hash_hex`, `:531`. The spec adopts the **bare recovered-doc skip**; the assert-against-original is the named fallback.)
2. **Recovery completes.** The assert no longer false-fires; attempt#2 advances + stamps sfn → `Sent` → later `ACK`. Under Option 0's generalized finalize gate ("already-advanced-at-issuance") `stage_finalize` **skips** re-advance/guard for the now-SENT+ doc → the finalize guard (`:304`) does **not** re-fire against `H_dps`. **Bonus:** Option 0 thereby *closes* a latent advance-at-ACK hazard — today a recovered doc reaching ACK with `ns.seed ≠ H_dps` would false-fail the `:304` guard and wedge at KVT2 (the §5.3 "wedges later, at KVT2" family).
3. **D3(ii) discriminator survives.** attempt#2 is a `WireDecision::Sent` → `set_server_fiscal_no_tx` stamps sfn (`:1373-1379`) atomic with the Sent CAS **and** the seed advance → `sfn set ⟺ seed advanced` holds for the recovered doc, so `is_issued(state, ofn, sfn)` classifies it correctly. attempt#1 (`Routed{MacRecovery}` → `ErrorRetryable`) stamps no sfn and does not advance — clean. Under the inline lane, A2.4's D3(ii) obligation already forces the inline `Sending→Kvt1` fast-path to stamp sfn+advance in lockstep — mac-recovery inherits it, no new hole.
4. **Frozen #1 / #2.** The reseed is a `node_state` UPDATE inside the existing 4-b `with_immediate` tx (no new I/O); FN single-writer preserved.
5. **M2-X1 guard.** Its *conclusion* (refuse offline-origin docs) is unchanged and correct; only its rationale **comment** (`:386-398`) drifts under Option 0 → joins the lockstep reword set (online now advances at **SEND**, offline at offline-ack; re-signing an offline doc under a DPS hash still desyncs the local offline chain).

**Consumer/landing addition.** `mac_recovery.rs` is added to §1b as **C14** (see §9.6 table): (i) the pre-advance gate gains a `mac_recovery_attempts >= 1` recovered-doc branch (skip-equality + reseed-to-own-sha); (ii) the M2-X1 rationale comment rewords to advance-at-SEND. Both ride the interim-ordering atomic commit (landing steps 2+4, §9.5). **STOP-gate 1.1 CLEAR.**

### 9.2 HIGH — D5-gate re-specification (two formulation defects + residual)

The §5.3 D5 residual named a "narrow gate: do not sign while a pre-SENT doc rests on the FN." The audit shows that phrasing is doubly wrong and under-specified. **Re-spec (LOCKED unless the architect overrides):**

**(a) Predicate — NOT a "pre-SENT" state-set.** `ERROR_RETRYABLE` is a member of `OFFLINE_ISSUED_STATES` (`fiscal_documents.rs:897-905`, `ERROR_RETRYABLE` @ `:901`): an offline-origin ER doc is **issued** and legitimately rests in the drain backlog. A literal "pre-SENT state-set" gate (`Prepared|Signed|Encrypted|Sending|Sent|ErrorRetryable`) would catch those offline-ER docs and **stall the entire offline lane**. **LOCKED gate-predicate:** `∃ doc on FN: non-terminal AND NOT is_issued(state, offline_fiscal_no, server_fiscal_no)` — the **complement of the D4 SSOT**. It is transparent to every legitimate rest: SENT-Hold (issued via sfn), KVT1/KVT2, OFFLINE_LOCAL_ACK, and offline-ER (issued via `OFFLINE_ISSUED_STATES`). It gates only genuinely non-issued in-flight docs (online Prepared/Signed/Encrypted/Sending, online-ER).

> **AMENDMENT (architect ruling at PR-C, 2026-07-06) — ONLINE-LANE SCOPING.** The fuzzer's
> differential teeth caught that the unqualified "∃ doc on FN" wording over-reaches: **an
> OFFLINE-origin mint/sign must NEVER be gated** — offline availability is the legally-mandated
> mechanism (INV-08) and exists precisely for the connectivity failures that park online-ER docs;
> gating offline on a parked online sibling would refuse sales during an outage. **Both gate
> layers are therefore scoped to online-origin** (acquire: `channel == Online`; sign:
> `fs_mode == 'ONLINE'`). Accepted bounded cost: a parked online non-issued doc can become
> chain-**superseded** by offline advances during the outage → its later re-drive fail-closes at
> the SEND drift-assert → manual-recon family (the existing SupersededHeld/W9b shape). Two
> refinements also landed with the gate: **self-exclusion** (the predicate skips the doc being
> processed) and **lnd-ordering** (only `lnd < self` blocks — the chain-head passes, so boot's
> lnd-ASC re-drive cannot deadlock on legacy multi-doc rests).

**(b) Co-requisite resolver — a LOCK-condition, not a follow-up.** Online-ER is **not** re-driven in runtime today: `er_redrive_policy` parks its `Hold*` verdicts through boot (`HoldProbeRequired` `:66-70`, `HoldIndeterminate` `:72-77` — "Caller holds; no DB mutation"), and `online_convergence` owns only `SENT`/`KVT1` (`:3-10`), never pre-SENT. So the gate **alone** = an FN-wide sign-refusal that persists **until reboot** on any transient failure that parks a doc non-issued. **LOCKED:** the gate lands **only paired** with a resolver that drives the blocking doc forward at runtime. **Resolver RULED (architect, 2026-07-05) = (i) extend the `online_convergence` tick onto the ER/pre-SENT cohort via the existing `er_redrive_policy`.** Option (ii) in-band resolve at gate-refusal is **rejected**: it couples a new receipt's latency to another doc's wire retry (the per-receipt latency budget is an operational requirement), creates re-entrancy under the FN single-writer lease (a sign path triggering another doc's send), and splits reconciliation ownership across two homes. Gate refusal returns a typed retryable refusal (audit-only); the tick resolves in the background. This pairing is a **LOCK-condition**.

**(c) Coverage + enforcement.** The blocking (non-issued, non-terminal) set includes: **Signed** (dispatcher-refusal rest — `dispatch.rs:50-52` I8: "refused modes leave doc state untouched, still `Signed`"), **Encrypted**, **Sending**, and **both non-pinned and pinned `Prepared`** (resume reuses a stale pin: `stage_sign.rs:301-320`, `PinResult::Reused` returns the previously-pinned `previous_hash`). **Enforcement is two-layer:** (1) a **fail-closed assert inside the `stage_sign` pin-tx** — mandatory because boot's `dispatch_prepared_via_chain` enters `stage_sign` **bypassing `stage_acquire`**, so an acquire-only gate misses the boot path; plus (2) an **acquire-level early-refusal** (pre-mint, audit-only) for the live-request path.

**(d) New residual (audit) — lnd-vs-chain-order inversion.** A non-pinned `Prepared` (`lnd = N`) resumed **after** a later doc (`lnd = N+1`) already issued will pin the *current* seed → the chain stays hash-valid, but **chain-order ≠ lnd-order**. NC-03's `ORDER BY lnd DESC LIMIT 1` (`fiscal_documents.rs:933`) would then project the wrong tip — a counterexample to "monotonic in lnd." **Closed by:** including non-pinned `Prepared` in the block-set (c) **and** adding an `invariant_scan` check "chain order == lnd order over the issued set." Documented as a landing-plan obligation (§9.5).

**(e) SW-4 — unconditional.** The SW-4 split (`ChainSeedMismatch` out of the inline `StructuralDrift` arm → `escalate_fn_to_manual_recon`, §5b) is **independent of the gate choice** and already in the landing plan (design §6 step 8). Formulation **confirmed** — it rides the A2.4 gate regardless of how D5 resolves.

**(f) One-time pre-gate boot-scan (LOW).** A boot-time scan for pre-existing fork-pairs (duplicate `previous_hash` among non-terminal docs of a FN) is recorded in the landing plan as a **LOW** one-shot — it surfaces a fork minted *before* the gate existed (only reachable on a restored/foreign DB, since prod mints zero fiscal_documents today, §9.5/§3.8).

### 9.3 HIGH — C10: Z-report issued/pending sets (third hardcoded literal) — §1b addendum + ruling-request

A **third** hardcoded online-issued literal lives in prod SQL, outside the D4 SSOT (beyond C2 `invariant_scan.rs:268` and C3 `fiscal_documents.rs:933`):

- **`list_shift_issued_receipts`** (`fiscal_documents.rs:560`; `state IN ('ACK','OFFLINE_LOCAL_ACK')` @ `:570`; self-described "issued-ledger set" @ `:551`) — the Z aggregation source (consumer `convert.rs:616`).
- **`list_shift_pending_receipts_for_z_quiescence`** (`fiscal_documents.rs:602`; blocking set `PREPARED..ERROR_RETRYABLE` @ `:612-613`; `REJECTED`/`CANCELLED`/`RMR` explicitly **non-pending** @ `:598-601`) — the quiescence gate (consumers `z_builder.rs:115`,`:134` → `Clear` / `Pending{blocking}` @ `:80-89`).

**Defect.** Under the new predicate a **post-SENT RMR** doc is issued (lnd consumed, sfn set, seed advanced), but Z **neither counts it** (not `ACK`/`OFFLINE_LOCAL_ACK`) **nor waits on it** (RMR is bucketed non-pending) → the shift's Z totals **diverge from the DPS view**, and the quiescence pass can close a shift **over a doc DPS may still hold**. Dormant today (`FULL_Z_SURFACE_READY = false`, `z_builder.rs:48`), but **dormancy does not exempt a consumer from the §1b lockstep table**.

**Added to §1b as C10** (see §9.6 table). **RULED (architect, 2026-07-05) — the CONFIRMED/blocking split is ADOPTED:** these are **CONFIRMED**-sets, not raw issued-sets — Z **counts only DPS-confirmed** receipts (`ACK`/`OFFLINE_LOCAL_ACK`); Z's aggregation does **not** adopt `is_issued` directly (Z totals must stay DPS-faithful). The **quiescence gate MUST block on any issued-yet-unconfirmed doc** (post-SENT SENT/KVT1/KVT2/RMR with sfn set) rather than treat RMR as non-pending — a shift-close over a doc still in DPS's hands is forbidden; an RMR-parked issued doc therefore holds the Z until the operator resolves it (consistent with manual-recon as the operator-attention surface). The landing plan carries the reword + the block-on-issued-unconfirmed change (design §6 step 5).

### 9.4 D4 re-spec — fetch-then-filter (needs-work → sound)

C3 (`fiscal_documents.rs:927-940`) is a **SQL string** with the literal `state = 'ACK'` @ `:933` — a Rust `is_issued(state, offline_fiscal_no, server_fiscal_no)` fn **cannot run inside SQLite**, so the DRAFT landing-step-5 "collapse both literals into one shared fn" is **unimplementable for the SQL consumer** as written. **Architect ruling — fetch-then-filter:** `last_issued_unsigned_xml_sha256` (and the analogous NC-03 projection) returns **candidate rows** `ORDER BY lnd DESC` and a Rust loop takes the **first `is_issued()` row**. NC-03 is a rare boot path; the scan is cheap; *"by construction"* beats *"by convention."* A **SQL mirror + a SQL≡fn equivalence-tooth** (matrix `state × offline_fiscal_no × server_fiscal_no`) is an **acceptable fallback only** with an explicit justification of why fetch-then-filter did not fit. The shared `is_issued` fn (D4) **still stands** as the single SSOT for the *in-memory* consumers (C2 walk, C6/C8 fuzzer/replay, the D5 gate predicate, C10); C3 is the one consumer that reaches it via fetch-then-filter rather than in-SQL. §5 D4 row and design landing-step 5 are rewritten accordingly (§9.6 / design §2 + §6).

### 9.5 MED closures + A.3 implementation checklist

- **3.1 — Remove five dormant sfn-less edges** in the **same A.3 commit** as `(Sent,Rejected)` `:190`: `(Encrypted,Sent)` `:186`, `(OfflineLocalAck,Sent)` `:203`, `(OfflineLocalAck,Kvt2)` `:241`, `(ErrorRetryable,Sent)` `:242`, `(ErrorRetryable,Kvt1)` `:243` — all machine-verified **zero prod-invokers** (sole citer = the table-pin test `repo_fiscal_documents_state_cas.rs`). The `(Sent,Rejected)`-removal argument applies verbatim. **`(Sending,Kvt1)` `:254` is also removed now but RETURNS with the inline fast-path** when it lands (with its own sfn+advance stamp per D3ii). Update the table-pin test.
- **3.2 — Scan-backstop.** Extend `invariant_scan` check 3a (`src/db/invariant_scan.rs:192-201`, today `state='ACK'` only) to `state IN ('SENT','KVT1','KVT2','ACK') AND offline_fiscal_no IS NULL AND (server_fiscal_no IS NULL OR = '')`. In the landing plan; the DDL `CHECK` form is **optional**, deferred to the next table-rebuild.
- **3.3 — INTERIM-ORDERING PIN.** Landing steps **2** (advance-at-SEND) and **4** (generalize the finalize gate + disable the `:315` online advance) **MUST be one atomic commit/PR.** If step 2 lands without step 4, `stage_finalize` still advances at `:315` for online docs, but `ns.seed` already moved at SEND → the `:304` guard (`ns.seed == inputs.previous_hash`) false-fails → **every online doc deterministically wedges at KVT2.** Explicit paragraph added to the landing plan.
- **3.4 — D6 downgrade correction.** Append to D6: *"rollback after the FIRST advance-at-SENT doc = safe only after the FN quiesces to a terminal (`ACK`/escalated); a bare binary revert clears the in-flight-issued docs onto the old finalize-guard and wedges them."* One sentence into D6 (§5.4 / design §5).
- **3.5 — Migration-time boundary-assert** in the A.3 migration (~5 lines SQL, fail-closed, **no permanent trace**): hazard = `offline_fiscal_no IS NULL AND server_fiscal_no IS NOT NULL AND server_fiscal_no != '' AND state != 'ACK'` (a pre-deploy SENT/KVT/RMR doc caught with sfn-set-but-not-ACK ⇒ **stop the migration**). **NOT** a permanent check (post-A2.4 it would false-positive on a healthy DB). In the landing plan; closes the restored/foreign-DB vector (§3.8).
- **3.6 — Option-matrix additions** (§2): (a) **Option 4 "ledger-tail-derive"** — record-and-reject (passes the RED-pin, but: same C1–C9 lockstep with no saving; kills the seed-vs-ledger tripwires — finalize guard, offline `ensure!`, walk-check; the D5 interleave persists; a hybrid against minimal-diff); (b) the **"advance-at-SIGN + compensation"** sub-variant — reject on the **correct** grounds (M3b crossed-local-commit does **not** apply at SIGN): every abort-arc (#192/#196) becomes seed-corrupting; un-advance breaks monotonicity = the D5 premise; scanner self-contradiction (an issued `SIGNED` = a `StuckNonTerminalDoc`); (c) liveness footnote on opt-0's cell — "no tax on SENT-Hold; the D5 gate adds a narrow stall only on non-issued rest (the failure path)."
- **3.7 — C11/C12/C13 no-edit records** (§1b): **C11** = the prod-ingress accepted/failed family (`replay.rs:49-62` `is_accepted`/`is_terminally_failed`, `handler.rs:300` `classify_outcome`, `inline.rs:156`) — **no-edit** (client-accepted = confirmed; a post-SENT RMR already answers `InProgress` correctly), plus a comment-pin at `replay.rs:59` folded into C9; **C12** = the five submitted-tail predicates (`fiscal_documents.rs:728/752/776/801/830`, the NC-04 feeder) — **no-edit** with rationale, **but the D3 claim "immune to any future terminal routing" is NARROWED to the `is_issued` fn** (submitted-tail predicates key on a `SENT/KVT1/KVT2[/ACK]` state-set and are **not** immune); **C13** = `last_server_fiscal_no` (`:693`, ACK-only status endpoint) — a **do-not-widen** pin, twinned with `last_ack_unsigned_xml_sha256`.
- **3.8 — Upgrade-boundary / mint reformulation** (§0): in the prod DB of **this binary lineage there are ZERO `fiscal_documents` rows** — the mint (`insert_prepared_tx ← inline.rs:534 ← inline::run`) is unreachable behind `UnimplementedWritePath` (`supervisor.rs:188`). Reframe the §0 dormancy claim from "the lane" to **"the mint"** (a stronger statement). Boot can **park** an online doc at `SENT` (`boot_phase.rs:3607/3865`) but never **mint** one. The 3.5 boundary-assert closes the restored/foreign-DB case.

### 9.6 LOW / hygiene closures + anchor-drift (machine-verified, `909fc05`)

**3.7-related / D2 / D7 / doc_type / W9:**
- **4.2 (D2):** replace the fixed citers list with a **grep-sweep obligation** in the A.3 checklist: `seed NOT advanced|lnd consumed|issue.*at ACK` across `src/` + docs, updated with the A.3 code.
- **4.3 (D7):** the RefModel (`tests/invariant_fuzzer/model.rs:265-271`) carries `docs: map<lnd→DocState>`, `seed`, `next_lnd` — **no per-doc `server_fiscal_no`**; the online advance is keyed on `DocState::Ack`. D7 must add: the model gets a **per-doc sfn / issued-bit on the Sent-crossing**; the tooth domain = `DocState × offline_fiscal_no{None,Some} × server_fiscal_no{None,'',non-empty}`.
- **4.4 (doc_type-independence):** the issued-predicate is doc-type-independent — `stage_sign.rs:137-150` puts `ShiftOpen/Sell/Return/ShiftClose/ZReport` in one chain (`Service*`/`XReport` are refused pre-pin). Add one sentence; and one sentence that **`Z_REPORT`-on-SENT is an issued-but-unconfirmed shift-close artifact** (interplay with the M3b shift ladder — a shift-close whose Z advanced the seed at SEND but is not yet DPS-confirmed).
- **4.5 (W9):** boot **never** CASes `Sending→Sent` — only `Sending→ErrorRetryable` (`boot_phase.rs:3726` → `resume_sending_to_error_retryable` @ `:666-679`). Recorded as a **positive D3(→) fact** in §5.3: the single live online issued-forward edge is `WireDecision::Sent` in `stage_send` (no boot path forges an issued doc).

**4.1 — broken/imprecise anchors (corrected):**

| Dossier cite | Machine-verified | Fix |
|---|---|---|
| §1.1 W-a "CLI `prro fn seed-prevhash <hex>`" | **no `fn` subcommand exists** (`main.rs`: Version/Migrate/Doctor/Serve/Admin only); `seed_prevhash` is a pool helper (`node_state.rs:134`) with **zero prod call-sites — tests-only** | drop the CLI claim; W-a is a tests-only bootstrap helper |
| §5.1 "4-b post-wire CAS `stage_send.rs:1524` (`target_state: Rejected`)" | `:1522` = `synthetic_rejected_decision` (a W10.4 helper, `ErrorRetryable→Rejected`); `:1524` = its `target_state` field. The **real** 4-b post-wire CAS is `:1360` | correct §5.1 to `:1360` (CAS) + note `:1522` is the synthetic-reject helper |
| §5.1 "`node_state.rs:186` doc-comment confirms the lnd stamp" | `:186`/`:204` = the `set_mode_blocked_tx` docstring, **not** an lnd-stamp; the lnd-stamp claim is pinned by `tests/invariant_fuzzer.rs:344-354` | re-cite the lnd-stamp to the fuzzer pin |
| C2 online arm `invariant_scan.rs:269` | `:268` (`state == "ACK"`) | cosmetic |
| `node_seed` read `invariant_scan.rs:220` | query `:220`; loop-bind `:223` | cosmetic |
| `last_ack_unsigned_xml_sha256` `fiscal_documents.rs:871` | `:873` | cosmetic |
| NC-04 `boot_phase.rs:2270` | `:2273` | cosmetic |
| 32-byte decode guard `node_state.rs:71` | `:68` | cosmetic |

**§1b consumer-table additions (C10–C14).**

| # | Consumer | Anchor | Rule under Option 0 |
|---|----------|--------|---------------------|
| C10 | Z issued + quiescence sets | `fiscal_documents.rs:560` (`:570` literal), `:602` (`:598-601`) → `z_builder.rs:115/134`, `convert.rs:616` | **§9.3 ruling** — count CONFIRMED, block quiescence on issued-unconfirmed; dormant (`FULL_Z_SURFACE_READY=false`) ≠ exempt |
| C11 | ingress accepted/failed family | `replay.rs:49-62`,`:59`; `handler.rs:300`; `inline.rs:156` | **no-edit** (client-accepted = confirmed) + comment-pin `replay.rs:59` in C9 |
| C12 | submitted-tail predicates | `fiscal_documents.rs:728/752/776/801/830` | **no-edit**; **narrow** the D3 "immune to future terminal routing" claim to the `is_issued` fn only |
| C13 | `last_server_fiscal_no` | `fiscal_documents.rs:693` (ACK-only) | **do-not-widen** pin (twin of `last_ack_unsigned_xml_sha256`) |
| C14 | `run_mac_recovery` advance/gate | `mac_recovery.rs:351`,`:386-398`,`:511-521` | §9.1 Variant P — recovered-doc gate branch + M2-X1 rationale reword |

**D-table refinements (fold into §5.4 / §8):** **D4** — shared `is_issued` fn for in-memory consumers; **C3 via fetch-then-filter** (SQL cannot host the fn); SQL-mirror+equivalence-tooth = justified fallback only. **D5** — gate predicate = `is_issued`-complement (not a pre-SENT state-set); **lands only paired with a runtime resolver (LOCK-condition)**; two-layer enforcement (pin-tx assert + acquire early-refuse); +lnd-vs-chain-order scan. **D6** — +downgrade correction (§9.5 3.4).
