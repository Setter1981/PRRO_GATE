# Spec — the TURNOVER-COUNTED set (bd `PRRO_GATE-a6n`)

**Status:** proposed, spec-first. No code written against it yet.
**Scope decision (operator, 2026-08-02):** BOTH lanes — offline drain-transit *and* online post-SENT.
**Discovered-from:** the second side finding on bd `PRRO_GATE-x5o` (closed, PR #344 / `d895130e`).

---

## 1. The defect, stated once

`services/cash_ledger.rs` decides what is in turnover with a state literal —
`state IN ('ACK','OFFLINE_LOCAL_ACK')`. `services/offline_sync/backlog_drain.rs` walks a drained
offline document through `OFFLINE_LOCAL_ACK → SENDING → SENT → KVT1 → KVT2 → ACK`. The two sets
intersect only at the endpoints. So the same cash leg is **counted at `OFFLINE_LOCAL_ACK`, uncounted
for the entire drain, and counted again at `ACK`** — with no fiscal event anywhere in between.

Delivery bookkeeping is being read as fiscal validity.

### 1.1 Reproduced

`rust/prro/tests/cash_drain_transit_repro.rs` — one offline cash SELL of 15 000 kop on an
`OpenedLocalPendingDrain` shift, forced through each drain state:

| doc state | `derive_closing_cash` | `cash_on_hand_for_fn` |
|---|---|---|
| `OFFLINE_LOCAL_ACK` | 15000 | 15000 |
| `SENDING` | **0** | **0** |
| `SENT` | **0** | **0** |
| `KVT1` | **0** | **0** |
| `KVT2` | **0** | **0** |
| `ERROR_RETRYABLE` | **0** | **0** |
| `ACK` | 15000 | 15000 |

### 1.2 Why it is durable, not a blink

`backlog_drain.rs:66-73`, verbatim (the module's own known-gap register):

> **TransientRetry-stranded pending-drain shifts (HIGH-C4-8, 2026-05-21)**: a doc that hits
> `RetryClass::TransientRetry` during C4 drain moves `OFFLINE_LOCAL_ACK → Sending → ErrorRetryable`
> (via stage_send Pattern B routing). […] the doc exits the C4 `OFFLINE_LOCAL_ACK`-only scan **while
> the shift remains in `OpenedLocalPendingDrain` / `ClosingLocalPendingDrain`**.

That shift state is OPEN — `handle_x_report`'s gate (`handler.rs:478-484`) excludes only
`Created / Closed / RequiresManualReconciliation / Error`. So the doc rests off-ACK precisely in the
window where the X-report answers and the INV-21 guards read, until a later drain tick re-drives it.
Across a flaky-DPS reconnection — which is the whole reason we were offline — that is minutes.

### 1.3 Blast radius

1. **X-report** (`handler.rs:444`) — not quiescence-gated. Under-reports both the turnover section and
   `cash_on_hand_kop`. Its own doc comment (`handler.rs:441-443`) claims:
   > **Bimodal for free:** the aggregation reads the durable ledger (`ACK` + `OFFLINE_LOCAL_ACK`), so
   > an `OpenedLocalPendingDrain` (offline) shift returns the same turnover with no special-casing.

   True only while every backlog doc sits exactly at `OFFLINE_LOCAL_ACK`. One drain tick falsifies it.

2. **INV-21 guards — service-blocking, not cosmetic.** `convert.rs:1006` refuses when
   `cash_on_hand_kop < return_cash_kop`. With cash-on-hand 0, **every** cash RETURN is refused (422);
   guard-3b (`convert.rs:1043`, SERVICE_OUT / інкасація) and guard-3c (`convert.rs:1099`, EPZ) refuse
   identically. A cashier cannot refund or collect cash while one backlog receipt is mid-drain.

3. **Z is NOT affected.** `quiesce_shift_before_z` (`z_builder.rs:109`) blocks on the whole in-flight
   set — `list_shift_pending_receipts_for_z_quiescence` (`fiscal_documents.rs:874`) selects
   `PREPARED, SIGNED, ENCRYPTED, SENDING, SENT, KVT1, KVT2, ERROR_RETRYABLE` plus post-SENT online
   RMR. Aggregation cannot run while anything rests in the transit set. **This is why the design has
   held so far, and it is the load-bearing fact for §5.**

---

## 2. Why the fuzzer never caught it

Structural, and worth recording so we do not "fix" it by trusting the oracle harder.

- `tests/invariant_fuzzer/model.rs:1444-1456` deliberately mirrors prod's ACK-only rule and carries a
  written escape hatch, verbatim:
  > NOTE (follow-up, not blocking): cash-on-hand counts at ACK, not at SENT. The deep question of
  > "should SENT docs pre-book cash capacity" is a policy question separate from INV-21 correctness.

- Even had the model disagreed, the alphabet cannot observe it. The only ops that park a doc off-ACK
  durably are the drain `Reject` / `Superseded` holds (`model.rs:1772-1810`), and both set
  `shift_state = RequiresManualReconciliation`. `handle_x_report` then refuses with `NO_OPEN_SHIFT`
  before producing a snapshot, so the L6 turnover oracle (`invariant_fuzzer.rs:4012`) has nothing to
  compare.

**The blind spot is the combination: whatever parks the document also closes the observation window.**
§8.3 says what to do about that.

---

## 3. The two lanes are not symmetric — and the asymmetry is the design

| | offline-origin | online-origin |
|---|---|---|
| issuance moment | `OFFLINE_LOCAL_ACK` (M2-01) | the `Sending → Sent` CAS (A.3 advance-at-SEND) |
| caller has been told | **200 success** — `terminal_to_outcome`, `inline.rs:165` | **202 IN_PROGRESS** — `inline.rs:192-205` |
| receipt in customer's hand | yes | not yet confirmed |
| counted today | at OLA and again at ACK | at ACK only |
| defect class | **flicker** — counted, uncounted, counted, no fiscal event | **monotone** — never counted before ACK |

The offline half is not arguable: a value that leaves and returns with no fiscal event in between is
incoherent by construction. The online half is a genuine policy call, which is what the model's NOTE
was pointing at. §4 rules on it.

---

## 4. The rule

Do **not** reach for `fiscal_documents::is_issued` directly. It is the **seed-advance** authority
(`fiscal_documents.rs:1226-1240`) and it deliberately admits `REJECTED` and
`REQUIRES_MANUAL_RECONCILIATION` for offline origin, because those *did* move the MAC seed. Those must
stay OUT of turnover — that is exactly what bd `PRRO_GATE-x5o` adjudicated (a receipt DPS never
accepted is not turnover, prod was right).

The coherent rule is a **delta on that SSOT predicate**, not a new state list:

```
counted_in_turnover(state, offline_fiscal_no, server_fiscal_no) :=
        is_issued(state, offline_fiscal_no, server_fiscal_no)
    &&  state ∉ { REJECTED, REQUIRES_MANUAL_RECONCILIATION }
```

Expanded, for review:

- **offline-origin** — counted in `OFFLINE_LOCAL_ACK, SENDING, SENT, KVT1, KVT2, ERROR_RETRYABLE, ACK`.
  (`OFFLINE_ISSUED_STATES`, `fiscal_documents.rs:1215-1224`, minus the two terminal-void members, plus
  `ACK` which `is_issued` admits explicitly.)
- **online-origin** — counted iff `server_fiscal_no` is stamped and the state is not RMR. In practice:
  `SENT, KVT1, KVT2, ERROR_RETRYABLE, ACK`.

Three properties make this the right shape:

1. **`CANCELLED` needs no special case.** It is absent from `OFFLINE_ISSUED_STATES`, so `is_issued` is
   already false — the x5o cohort-cancel semantics survive untouched, for free.
2. **Online `SENDING` needs no special case.** Under A.3 the sfn is stamped *at* the `Sending → Sent`
   CAS, so an online doc in `SENDING` has no sfn and `is_issued` is already false. The lane asymmetry
   (offline `SENDING` counts, online `SENDING` does not) falls out of the existing discriminator
   instead of being hand-written.
3. **Online `REJECTED` cannot carry an sfn.** `(Sent, Rejected)` was removed in A.3 PR-B step 6 —
   `fiscal_documents.rs:194`, verbatim: `// A.3 PR-B step 6: (Sent, Rejected) removed — policy D3: a
   post-SENT`. The only surviving edge is `(Sending, Rejected)` (`:256`), i.e. pre-stamp. So the
   `REJECTED` exclusion is belt-and-braces for the offline lane only.

### 4.0 The reference PRRO settles the online half (added 2026-08-02, after the first draft)

The first draft left the online lane as an open policy question — "does the till open before the 200?".
**That was the wrong question, and the repo already contains the answer.**

`cash_ledger.rs:36-38` and `:68` state that our cash formula was ported from WebCheck's `Nal()`
(`docs/webcheck_reverse_v2/WebCheck/All.cs:431-462`). So look at what the source actually aggregates.
`Nal()` delegates to `Reports.Reprt2` (`Reports.cs:50-84`), whose SQL is, verbatim:

```sql
SELECT PAYMENTFORM as NM, SUM(TOTALSUM) AS SMI, NULL AS SMO from CHECKPAY
  where CHECKID in (select ID FROM CHECKHEAD WHERE SHIFTID = ? AND DOCTYPE = '0') GROUP BY PAYMENTFORM
union all
SELECT PAYMENTFORM as NM, NULL AS SMI, SUM(TOTALSUM) AS SMO from CHECKPAY
  where CHECKID in (select ID FROM CHECKHEAD WHERE SHIFTID = ? AND DOCTYPE = '1') GROUP BY PAYMENTFORM
```

**There is no delivery-state predicate — `SHIFTID` and `DOCTYPE` and nothing else.** Nor could there
be: `CHECKHEAD` (`CreateDB.cs:235`) has no state column at all, and no code path UPDATEs a receipt row
after insert. In the reference, **turnover is the receipt journal**. Delivery to DPS never subtracts
from it.

The decisive part is *when* a row enters that journal. `SaveCheck` → `INSERT INTO CHECKHEAD`
(`SQLlite.cs:762, :791`) is called from two paired sites in `StringXML.cs`:

- `:1382` — **online**: `SaveCheck(uid, …, typErrSubmit.returnNumber, …)`, i.e. the row is written the
  instant the DPS **submit returns a fiscal number** (`:1371` `CheckTaxNum = typErrSubmit.returnNumber`).
  There is no quittance wait and no second step.
- `:1229` — **offline**: the same insert with the locally-derived offline number.

Map that onto our ladder. WebCheck's "submit returned a fiscal number" **is** our `Sending → Sent` CAS —
the A.3 moment that stamps `server_fiscal_no` and advances the seed. WebCheck's offline number is our
`OFFLINE_LOCAL_ACK`. In both lanes, the reference admits the receipt into turnover at exactly the moment
this spec proposes, and never removes it again.

**Ruling: the online counting moment is `SENT`, not `ACK`.** Our `KVT1 → KVT2 → ACK` ladder is a
confirmation refinement the reference does not have; it is a better design, but it must not gate
turnover, because turnover was never a function of confirmation.

This also explains how the defect was born: the *formula* was ported from `Nal()`, but the source journal
has no delivery states, so there was no filter to port — and `state IN ('ACK','OFFLINE_LOCAL_ACK')` was
invented locally to fill a gap that did not exist upstream.

**Honest caveat.** The reference is *cruder*, not merely different: having no in-flight representation,
it cannot under-report, but it also cannot express a genuinely ambiguous send. So it offers no opinion
on §4.1 — the RMR exclusion below is our own, stricter, judgement and does not inherit this authority.

### 4.1 Ruling on post-SENT online RMR

Excluded — it stays out of turnover, as today.

Rationale: RMR is the "a human must decide" bucket, and the fiscal status of the document is by
definition unknown (DPS may or may not hold it). Counting it would be a guess in one direction;
`Z-quiescence` already refuses to close a shift over it (`fiscal_documents.rs:887-896`), and the
physical-vs-fiscal divergence for that bucket is what bd `PRRO_GATE-pr6` documents. Guessing is not
better than blocking here — the operator is already being summoned.

---

## 5. Behaviour-neutrality for Z (the safety argument)

Widening the counted set is a **no-op for every Z-class output**, by construction and not by
inspection: `quiesce_shift_before_z` refuses to aggregate while any doc rests in
`PREPARED..KVT2 | ERROR_RETRYABLE`. Every state this spec ADDS is a member of that blocking set.
Therefore at the instant aggregation runs, the added states are provably empty, and the new predicate
selects exactly the rows the old literal selected.

Consequence: the change lands on **X-report and the INV-21 guards only** — the two consumers that read
the aggregate without a quiescence gate. That is a small, well-bounded blast radius for a hot-zone
edit, and §8.1 turns the argument into a test rather than leaving it as prose.

---

## 6. Consumer inventory

`state IN ('ACK','OFFLINE_LOCAL_ACK')` occurs **11 times across 4 files** (9 live SQL + 2 doc
comments). They are NOT all turnover — a blanket replace would be wrong.

**Turnover sites — must adopt the new predicate (7 live SQL):**

| file:line | what |
|---|---|
| `services/cash_ledger.rs:179` | `aggregate_shift_epz` |
| `services/cash_ledger.rs:216` | `aggregate_shift_service_io` |
| `services/cash_ledger.rs:267` | `aggregate_shift_cash_tx` — SELL/RETURN legs |
| `services/cash_ledger.rs:303` | `aggregate_shift_cash_tx` — service-io |
| `services/cash_ledger.rs:332` | `aggregate_shift_cash_tx` — EPZ |
| `db/repositories/fiscal_documents.rs:831` | `list_shift_issued_receipts` (feeds `aggregate_shift_cash` **and** `aggregate_z_payload`) |
| `runtime/ingress/convert.rs:1258` | Z `<EPZ>` **count** (`EPC`) — must move in lockstep with `aggregate_shift_epz`, or count and sum disagree |

Plus the two stale doc comments at `cash_ledger.rs:165` and `:204`.

**NOT turnover — leave alone, with a one-line note saying why:**

| file:line | what | why it stays |
|---|---|---|
| `db/invariant_scan.rs:492` | `RejectedInboxWithAcceptedDoc` (AUD-1) | "accepted" here means *the inbox lied about a fiscalized doc* — a different question from turnover. Widening it is arguably right on its own merits and is **explicitly out of scope**; decide it on its own evidence, not as fallout. |
| `db/invariant_scan.rs:661` | Check-16 force-close test-seam skip | heuristic over CLOSED shifts; the transit set is empty there by §5 |

**Single-owner requirement.** Seven copies of one rule is how this defect was born. The predicate must
land as ONE owner that SQL sites reach through, mirroring how `is_issued` is the single owner for the
seed lane. Note `is_issued` cannot be pushed into SQL (it is a Rust fn over three columns), so the two
viable shapes are (a) fetch-then-filter in Rust, as `last_issued_unsigned_xml_sha256` already does per
`fiscal_documents.rs:1400`, or (b) one shared SQL fragment constant. **(a) is preferred** — it reuses
the existing `is_issued` SSOT verbatim and cannot drift from it; (b) would be a second spelling of the
same rule, which is the failure mode we are fixing. Cost: the aggregates already `fetch_all` and filter
in Rust (`cash_ledger.rs:136-150`), so (a) is not a new pattern here.

---

## 7. Cases table (the ruling, exhaustive over `DocState`)

| state | offline-origin | online-origin | note |
|---|---|---|---|
| `PREPARED` / `SIGNED` / `ENCRYPTED` | out | out | not issued either lane |
| `SENDING` | **in (new)** | out | offline crossed OLA; online has no sfn yet |
| `SENT` / `KVT1` / `KVT2` | **in (new)** | **in (new)** | sfn stamped / OLA crossed |
| `ERROR_RETRYABLE` | **in (new)** | **in (new)** if sfn stamped | the durable rest of §1.2 |
| `ACK` | in (today) | in (today) | unchanged |
| `OFFLINE_LOCAL_ACK` | in (today) | n/a | unchanged |
| `REJECTED` | out | out | x5o adjudication; online cannot hold an sfn (§4 pt 3) |
| `CANCELLED` | out | out | free — not in `OFFLINE_ISSUED_STATES` |
| `REQUIRES_MANUAL_RECONCILIATION` | out | out | §4.1 |
| `ABORTED` | out | out | post-sign refusal, never issued |

---

## 8. Teeth

RED-first, per the project's discipline: every pin below must be **observed RED before the fix and
GREEN after**, and each teeth test must be canaried (mutate → RED → revert) before it counts.

New tests go in a **non-frozen** file. `rust/prro/tests/l0_l1_cash_ledger.rs` is one of the 79 frozen
CS-1 files (`docs/cs1r/pins/cs1_canonical_fingerprints.tsv:81`) — adding to it is an AST-drift RED.
`rust/prro/tests/cash_drain_transit_repro.rs` (already written, currently RED) is the natural home.

### 8.1 The neutrality claim, as a test — not prose

§5 is the whole safety argument for touching a hot zone; it must not rest on reasoning alone.
Pin: for a shift whose docs rest in the newly-added states, `quiesce_shift_before_z` returns
`Pending` — so no Z can observe the widened set. Canary: remove one added state from the quiescence
blocking set and watch this pin go RED. If that canary does not fire, §5 is not load-bearing and the
whole change needs re-argument.

### 8.2 Behaviour pins

1. **The flicker is gone (offline).** The §1.1 table, asserted: 15000 in every drain state. *(This is
   the existing repro — it becomes the primary pin.)*
2. **The flicker is gone (online).** Same walk for an sfn-stamped online doc over
   `SENT/KVT1/KVT2/ERROR_RETRYABLE`.
3. **x5o survives verbatim.** A cohort-cancelled `CANCELLED` successor and an RMR-escalated held doc
   stay OUT. This is the regression that matters most — the fix must not silently re-admit what x5o
   adjudicated. Reuse the x5o trajectory.
4. **INV-21 stops false-refusing.** A cash RETURN against a drawer whose only receipt is mid-drain is
   ACCEPTED after the fix, refused before it. Drive it through `convert_to_signer_payload`, not
   through the aggregate — the point is the guard, not the sum.
5. **INV-21 still refuses for real.** A RETURN exceeding a genuinely-empty drawer is still 422. The
   floor must not be widened into uselessness — this is the anti-teeth-erosion twin of pin 4.
6. **`EPC` and `EPSM` agree.** The `<EPZ>` count (`convert.rs:1258`) and sum
   (`aggregate_shift_epz`) selected over the same widened set.

### 8.3 The fuzzer blind spot (§2) — close it or record it

Per the standing rule that a new feature must answer "should the fuzzer track this": the model
(`model.rs:1444-1456`) must be flipped to the new predicate **in the same PR**, or the pins in §8.2
are the only thing standing behind the change.

Note honestly that flipping the model alone does **not** restore coverage, because of the observation
window in §2: the shift goes RMR and the X-report refuses before a snapshot exists. Making this
generatively reachable needs a trajectory that parks a doc off-ACK on a shift that stays OPEN — i.e.
the `TransientRetry` drain outcome of §1.2, which the alphabet does not currently produce. **Decide
explicitly:** either add that symbol (a real slice, sized separately), or record the gap in the bd and
in `project_fuzzer_alphabet_gaps` so it is a known hole rather than an assumed cover. Do not let the
model edit masquerade as coverage.

### 8.4 Mutation impact

`services/cash_ledger.rs` and `db/repositories/fiscal_documents.rs` are fiscal-logic paths, so the
`mutation-diff` gate reaches the changed lines. A predicate that flips from a literal to a shared fn is
exactly the shape cargo-mutants attacks (negate the condition, swap the boolean). Expect new mutants;
each must be killed by a §8.2 pin or triaged per the FW-1 ratchet. If the diff-gate reports zero new
mutants on a change of this size, distrust the gate before trusting the tests — that is the
`PRRO_GATE-9g5` lesson.

---

## 9. Invariant check

- **#1 (no network/crypto in write-tx)** — the tx-bound variant `aggregate_shift_cash_tx` stays a pure
  SELECT; a fetch-then-filter in Rust keeps it pure. Preserved.
- **#2 (single-writer per FN)** — untouched; the in-lease re-check keeps reading inside the lease.
- **#4 (idempotency)** — untouched.
- **#8 (recovery must not silently violate transitions)** — this spec adds no transition. It changes
  only which existing rows a read counts.
- **INV-21** — the floor is preserved; only the input becomes truthful. Pin 5 in §8.2 is the guard
  against widening it into uselessness.
- **Legal** — `docs/LEGAL_INVARIANTS.md` INV-08..INV-14 use `OFFLINE_LOCAL_ACK` as the Pattern-C
  durable offline state. This spec does not move the issuance moment; it stops turnover from
  contradicting it during delivery. Cross-check the wording there before merge.

---

## 10. Decorrelated skeptic — attack list

Written to be attacked, not defended. Anyone reviewing should try these first:

1. **"The online lane is a policy change dressed as a bug fix."** Fair — §3 concedes the online half is
   monotone, not a flicker. It is in scope by explicit operator decision. If the reviewer disagrees,
   the offline half stands alone and the online half can be dropped without touching it: the predicate
   in §4 degrades cleanly by restricting to `offline_fiscal_no IS NOT NULL`.
2. ~~**"Counting a `SENT` online doc pre-books money the cashier has not taken."**~~ **CLOSED by §4.0.**
   The first draft raised this as the one blocking open question and framed it as "does the till open
   before the 200?". That framing was wrong: the reference PRRO never modelled the till at all, and its
   turnover query (`Reports.cs:50-84`) carries no delivery predicate — a receipt is in turnover from the
   moment a fiscal number exists for it, which in the online lane is the DPS submit's return
   (`StringXML.cs:1382`) = our `Sending → Sent` CAS. A reviewer who still wants to attack the online half
   must attack §4.0 directly — i.e. argue that our confirmation ladder SHOULD gate turnover even though
   the implementation we ported the formula from has no such gate.
3. **"§5 neutrality is too convenient."** It rests entirely on the quiescence blocking set being a
   superset of the added states. Verify `fiscal_documents.rs:885-886` by eye, then rely on §8.1's
   canary rather than on this paragraph.
4. **"You are widening a fail-closed guard."** Yes — INV-21 becomes less strict. The direction of the
   current error is *over*-refusal, and pin 5 keeps the real floor. The sharp version of the attack is:
   can a doc counted under the NEW predicate later end `CANCELLED`, after a RETURN has already spent
   its cash?

   **Resolved against prod, and in the safe direction.** `offline_cohort_cleanup`
   (`delivery_reservation.rs:1688-1751`) classifies every later successor before mutating anything, and
   **only `OFFLINE_LOCAL_ACK` is cancellable** (`:1718`). Every state this spec newly admits fails the
   completion CLOSED instead: `SENDING → LaterSuccessorInFlight` (`:1722`), and
   `SENT / KVT1 / KVT2 / ERROR_RETRYABLE / REJECTED / RMR / ACK → LaterSuccessorIssued` (`:1730-1738`),
   each rolling the whole transaction back.

   So the "counted, then voided" hazard does **not** extend to any newly-admitted state. It exists only
   for `OFFLINE_LOCAL_ACK` docs — which are counted **today**, unchanged by this spec. The change adds
   no new instance of the hazard. A reviewer should still confirm the classification list above has no
   fall-through, since it is a string `match` over states (`_ =>` catch-all at `:1732` fails closed on
   an unknown state, which is the right default).
5. **"Seven sites is a refactor in disguise."** The minimal diff would patch `cash_ledger.rs` only and
   leave `list_shift_issued_receipts` alone — but that splits X-report's cash figure from its turnover
   section, which is a worse incoherence than the one being fixed. The seven move together or not at
   all.

---

## 11. Out of scope

- `invariant_scan.rs:492` (AUD-1 accepted-doc set) — §6, decide separately.
- bd `PRRO_GATE-pr6` — the operator runbook for the cohort-cancel divergence. Same symptom at the
  counter, opposite verdict; keep them apart.
- Any change to the issuance moment, the seed lane, or `is_issued` itself. This spec is a **consumer**
  of that authority, never an editor of it.
- The `shifts.serial` dead column visible at `invariant_scan.rs:661` — bd `PRRO_GATE-seb`.
