# FINDING + hardening brief — `invariant_scan` is not cohort-cancel-aware (NotAcceptedOffline)

**Date:** 2026-07-24 · **Discovered-from:** wiring the GENERATIVE `NotAcceptedOffline` release path
through `run_harness` (task #18 C-iii generative — the handoff
`HANDOFF_2026-07-24_TASK18_CIII_GENERATIVE_NOTACCEPTEDOFFLINE.md`).
**Status:** decision = **defer generative release**; release stays **directed-only** (the 2 existing
`interp::run_op` directed teeth). The fuzzer surfaced a genuine **oracle-precision gap** in the prod
diagnostic `invariant_scan`. Fix = a SEPARATE production-hardening PR with its own design + audit
(operator directive, 2026-07-24). Generative `NotAcceptedOffline` is re-enabled AFTER that PR merges.
**Branch:** `fuzzer-cs3-oracle`, tip `a5948632` — **prod 0-diff, baseline 160/160** (all generative
apparatus REVERTED this session; nothing landed in code).

---

## 0. TL;DR

Running a real `NotAcceptedOffline` cohort-cancel through the fuzzer's ledger-clean oracle
(`prro/src/db/invariant_scan.rs`, reached via `oracle::assert_clean` in `run_harness`) reports **2
false-positive violations on a LEGITIMATE state**:

```
ChainBreak      @ lnd3: expected fe24…(=hash lnd1), found c31f…(=hash lnd2)
ChainSeedMismatch:      walk=fe24…(=hash lnd1),      node_state=<none>
```

- This is **NOT a model gap** — the fuzzer model predicts the SAME durable doc-states prod produces.
- This is **NOT a prod-runtime defect** — `invariant_scan::scan` is called ONLY from the diagnostic
  helper `assert_clean` (`invariant_scan.rs:591`); it is not on any boot/health/ingress path.
- It **IS** a precision gap in the diagnostic `invariant_scan` chain-walk: it does not model the
  cohort-cancel + chain-rewind recovery state that `NotAcceptedOffline` legitimately produces.

Because `invariant_scan.rs` lives under the FROZEN `rust/prro/src`, and the correct fix requires a
non-trivial semantic decision (see §4), the generative release path cannot be wired green in a
test-only slice. Deferred; hardening PR to follow.

---

## 1. The legitimate state (prod-correct, verified)

Authoritative prod tests `oc10` / `oc15` (`prro/tests/operator_completion.rs`) pin the gap-4b outcome
of `NotAcceptedOffline` on an OFFLINE-origin held doc (`delivery_reservation.rs:1451-1481` release
branch + `offline_cohort_cleanup:1579-1643`):

- the held predecessor doc → `REQUIRES_MANUAL_RECONCILIATION` (`doc_to_rmr`),
- every LATER same-session `OFFLINE_LOCAL_ACK` successor → `CANCELLED`,
- `node_state` chain seed **rewound to the held doc's own immutable `previous_hash`**
  (`Some(prev)` → predecessor, `None` → genesis).

Reproduction cohort (fuzzer + directed): `2×OfflineSell → GoOnline([Reject])` holds the offline
`OFFLINE_SESSION_BEGIN` (lnd 1); lnd 2/3 are `OFFLINE_LOCAL_ACK` SELL successors. After
`OperatorComplete(NotAcceptedOffline)`: lnd1→RMR, lnd2/lnd3→CANCELLED, seed rewound `Some → None`.

This state is CORRECT. The two directed teeth
(`directed_not_accepted_offline_cancels_cohort_and_rewinds` /
`…_refuses_on_later_issued_successor`) pin it against the real ledger and are GREEN.

---

## 2. Why `invariant_scan` false-positives on it

The MAC chain-walk (`invariant_scan.rs:309-382`) selects every doc with
`unsigned_xml_sha256 IS NOT NULL` (i.e. INCLUDING `CANCELLED`), checks each doc's `previous_hash`
against a running `expected`, and advances `expected` only for docs where
`fiscal_documents::is_issued(state, offline_fiscal_no, server_fiscal_no)` is true.

`is_issued` for offline-origin docs = `state == "ACK" || OFFLINE_ISSUED_STATES.contains(state)`, where
`OFFLINE_ISSUED_STATES` (`fiscal_documents.rs:1207-1216`) = `OFFLINE_LOCAL_ACK, SENDING, SENT, KVT1,
ERROR_RETRYABLE, KVT2, REJECTED, REQUIRES_MANUAL_RECONCILIATION`. **`CANCELLED`/`ABORTED` are NOT
issued; `REQUIRES_MANUAL_RECONCILIATION` IS.**

**Violation A — `ChainBreak` @ the 2nd+ cancelled successor.**
The walk includes cancelled docs (they retain `previous_hash` + `unsigned_xml_sha256`) but does NOT
advance `expected` over them (`is_issued(CANCELLED)=false`). Two consecutively-chained cancelled docs:
`lnd2` (prev = hash(lnd1), OK against `expected`=hash(lnd1)); `lnd3` (prev = hash(lnd2), but `expected`
is still stuck at hash(lnd1)) → **break**. A dead doc's `previous_hash` is a historical artifact and
should not be checked — but the walk checks it.

**Violation B — `ChainSeedMismatch`.**
The held predecessor → `RMR`, which `is_issued`=true, so the walk's terminal `expected` = hash(lnd1).
But prod legitimately **rewinds `node_state` past it** (to lnd1's own `previous_hash` = genesis =
`None`). `expected(hash lnd1) != node_seed(None)` → mismatch. **Even a single-successor cohort trips
this** (Violation A needs ≥2 chained cancelled docs; Violation B needs only the rewind), so it is not
an edge case — any `NotAcceptedOffline` release trips it under a real chain.

**The M2-N2b inversion.** The scan's own comment (`invariant_scan.rs:350-357`) justifies treating
`RMR`/`REJECTED` offline docs as issued because *"a successor chained off it BEFORE the drain
rejected/manual-escalated it"* — i.e. it assumes a **live** successor needs the RMR predecessor to
anchor the chain. In the cohort-cancel case the successors are all **CANCELLED (dead)**, so the M2-N2b
premise is inverted: the RMR predecessor anchors nothing, and the rewind correctly moves the seed
below it. `is_issued` cannot distinguish these two RMR shapes.

**Why never caught before.** The prod tests `oc10`/`oc15` (a) do NOT run `invariant_scan`, and (b)
seed successors with `previous_hash = NULL` and an identical `unsigned_xml_sha256 = [0x77;32]` for all
docs (`operator_completion.rs:877, 924`) — a synthetic setup with no real chain, so no `ChainBreak` is
possible even if scanned. The fuzzer issues a REAL chain (distinct hashes, real prev pointers) and runs
the scan → first exposure. `CS3_FUZZER_ORACLE_DOSSIER.md:420` already flags C-iii as "DIRECTED slice,
generative deferred" precisely because of this scan interaction.

---

## 3. Evidence chain (verified this session)

1. **RED (test bites):** the generative-path directed tooth via `run_harness` failed on the
   pre-existing structural seed-advance check — model didn't rewind (`real seed-advance (true) must
   match the model's (false)`). ✅
2. **Model fix → structural check passes**, then the PROD `invariant_scan` (via `assert_clean`) fired:
   `invariant_scan.rs:593` — `ChainBreak@lnd3` + `ChainSeedMismatch` (values above). This is prod's own
   oracle failing on prod's own legitimate ledger.
3. **Not runtime-bearing:** `grep` confirms `scan()` is invoked only at `invariant_scan.rs:592`
   (`assert_clean`, "Convenience gate: panic with a readable report") — no boot/health/ingress caller.
4. **Scan-gate reached because** `is_settled(mode, shift)` (`invariant_fuzzer.rs:2905`) admits
   `shift == RequiresManualReconciliation` even at mode `GOING_ONLINE`; after the release the shift
   rests RMR → the scan runs in place (by design: "a violation there is a REAL finding, not
   suppressed"). So we cannot merely lean on scan-timing suppression — the state is deliberately
   scanned.

---

## 4. Production-hardening PR — design constraints (operator directive)

Make `invariant_scan`'s chain-walk **cohort-cancel-aware**, as a standalone prod PR with its own
design + audit. **Do NOT weaken the scan via a test-side scoped allowance** — that would be a genuine
green-but-unsound compromise (operator, 2026-07-24).

Guardrails:

- **Define two distinct notions and separate them explicitly** (do not conflate):
  - *active-chain membership* — which docs are live links whose `previous_hash` participates in the
    walk and whose hash anchors the seed;
  - *historical-issued* — a doc that ever crossed `OFFLINE_LOCAL_ACK` (the M2-01 property `is_issued`
    encodes), independent of whether it is still an active chain link.
- **Do NOT globally drop `RMR` or `CANCELLED` from the walk hastily.** `RMR` with a **live** issued
  successor MUST stay chain-anchoring — removing it re-introduces the FALSE `ChainBreak` at the live
  successor that M2-N2b was added to prevent. The fix must keep M2-N2b's live-successor case intact.
- Candidate shape (design to be reviewed, not prescribed): treat `CANCELLED`/`ABORTED` as **not active
  chain links** (skip their `previous_hash` check entirely — dead pointers), and treat an `RMR`
  predecessor as **seed-anchoring only if it has a live issued successor**; a rewound `RMR` whose whole
  cohort is `CANCELLED` is a legitimate `NotAcceptedOffline` terminal whose seed sits at its own
  `previous_hash`. Both `ChainBreak` (Violation A) and `ChainSeedMismatch` (Violation B) must be
  reconciled without loosening detection of a REAL fork.
- **Regression teeth (both directions):** a REAL chain fork / a REAL rewound-past-issued must still
  RED; the legitimate cohort-cancel state must go GREEN. Add a directed scan test on the exact
  cohort-cancel ledger (currently RED) and a canary on a genuinely-broken chain.

---

## 5. Re-enable generative `NotAcceptedOffline` (AFTER the scan PR merges)

The generative apparatus was designed + verified this session, then REVERTED (deferred). Re-apply
verbatim once `invariant_scan` is cohort-cancel-aware:

### 5a. Model (`tests/invariant_fuzzer/model.rs`, `apply_operator_complete`, after the
`released_witness` Some/None gate, before the generic doc/mode/seed handling):

```rust
if matches!(kind, OperatorResolutionKind::NotAcceptedOffline) {
    // FORK GUARD (offline_cohort_cleanup): a later successor that is neither a cancellable
    // OFFLINE_LOCAL_ACK nor already dead (CANCELLED/ABORTED) → prod REFUSES, mutating nothing.
    // One-session approximation ("all later lnds" ≈ same session) is safe for this alphabet
    // (a halted offline hold admits no online-origin successor); a multi-session breach would RED.
    let later_fork = self.docs.range((held_lnd + 1)..).any(|(_, st)| {
        !matches!(st, DocState::OfflineLocalAck | DocState::Cancelled | DocState::Aborted)
    });
    if later_fork { return ExpectedOutcome::Release(None); }
    let cancel: Vec<i64> = self.docs.range((held_lnd + 1)..)
        .filter(|(_, st)| **st == DocState::OfflineLocalAck).map(|(l, _)| *l).collect();
    for l in cancel { self.docs.insert(l, DocState::Cancelled); }
    self.seed = None; // structural rewind marker; exact value asserted relationally by run_harness
    self.docs.insert(held_lnd, DocState::RequiresManualReconciliation);
    self.mode = node_mode_from_str(witness.node_mode); // GOING_ONLINE (active session drains)
    return ExpectedOutcome::Release(Some(witness));
}
```
(The existing `advances_seed` match arm `NotAccepted | NotAcceptedOffline => false` stays — unreachable
for `NotAcceptedOffline` given the early return, still exhaustive.)

### 5b. Relational oracle (`run_harness`, `invariant_fuzzer.rs`):
- Pre-op snapshot (right after `held_res_before`), gated on
  `Op::OperatorComplete(NotAcceptedOffline)`: capture `held_lnd` (from `model.held_reservation`),
  the held doc's `previous_hash` (`ctx.read_previous_hash(&rid)`), and `ctx.read_doc_states_by_lnd()`.
- In the Release arm `(Some(w), Released)`: assert `read_seed() == held previous_hash`, held doc → RMR,
  every later `OFFLINE_LOCAL_ACK` successor (from the pre snapshot) → `CANCELLED`.
- In the Release arm `(None, Refused)`: fork-guard intact — `read_doc_states_by_lnd()` unchanged.

### 5c. Directed generative tooth (`run_harness` on
`[OfflineSell, OfflineSell, GoOnline([Reject]), OperatorComplete(NotAcceptedOffline)]`) — currently
RED ONLY because of the scan gap; goes GREEN once the scan is cohort-cancel-aware. Canary: neutralize
the model rewind (`self.seed` unchanged) → structural seed check REDs; flip the oracle's cohort
expectation (OLA stays) → relational check REDs.

### 5d. Generator (`tests/invariant_fuzzer/strategy.rs:168-172`): add
`OperatorResolutionKind::NotAcceptedOffline` to the `OperatorComplete` `prop_oneof!` arm (appended
LAST — preserve corpus-index order). Keep `MacReseed` excluded (needs the operator's corrected seed).

### 5e. Then run capstones (`FUZZ_CASES=1500` → `4096`) and triage the remaining §4-handoff candidate
divergences (one-session approx; fork-guard classification of exotic later states; held targeting on a
non-first held doc; shift_state prediction on the completion — D2 in `run_harness`).

---

## 6. `bd` finding (BLOCKED — env)

`bd` is installed (`/home/setter/.local/bin/bd` v1.0.0) but its embedded-dolt DB manifest
(`/home/setter/prro_gate/.beads/embeddeddolt/PRRO_GATE/.dolt/noms/manifest`) is `-rw------- root root`
— unreadable as `setter` (uid 1000). Fix ownership (`sudo chown -R setter:setter .beads/embeddeddolt`)
or file via the `!`-prefix, then:

```bash
bd create "invariant_scan not cohort-cancel-aware: false ChainBreak+ChainSeedMismatch after NotAcceptedOffline cohort-cancel" \
  --type bug --priority 1 \
  --deps discovered-from:<task-18-cs3-fuzzer-oracle-bead> \
  --body-file docs/CS3_INVARIANT_SCAN_COHORT_CANCEL_FINDING.md
```
