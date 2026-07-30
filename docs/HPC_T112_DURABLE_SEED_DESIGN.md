# HPC T=112 durable seed witness — design (bd PRRO_GATE-hpc)

**Status:** design only, no implementation. Read-only investigation.
**Worktree:** `/home/setter/prro-gate-wt/hpc-t112` · branch `hpc-t112-durable-seed` (off `origin/main` 949d7cc1: bd 2nk fix + #338 + CS-3 fuzzer track).
**Author:** arch-planner, 2026-07-25.

---

## 1. Problem statement

A standalone T=112 offline-code replenish advances the online MAC-chain seed
(`node_state.last_known_unsigned_xml_sha256`) to `Hs = sha256(request_xml)` —
a **non-document** seed: no `fiscal_documents` row carries `Hs`. The advance is
atomic with the offline-code pool inserts in one `with_immediate` envelope
(`offline_code_replenish.rs:259-295`). `Hs` is cabinet-confirmed
(module doc line ~20), so it is grounded, not modeled-by-convenience.

NC-03 boot recovery (`boot_phase.rs::reconstruct_lost_node_state`, ~1734)
reconstructs the seed from the **ledger only** via
`fiscal_documents::active_chain_tip_unsigned_xml_sha256`. In the window

> replenish (seed→`Hs`) → NC-03 boot (node_state row lost, ledger survives) → first offline SELL

no doc carries `Hs`, so boot recovers the **pre-replenish** issued doc hash
`Hp` (≠ `Hs`), or genesis `None`. The next fiscal send then chains onto `Hp`;
DPS expects `Hs` → `BadHashPrev` → MacReseed hold. The operator supplies the
correct `Hs`, but MacReseed guard-B (`delivery_reservation.rs:1396-1411`)
requires `seed == active_chain_tip == Hp`, so `Hs` is rejected
(`MacReseedSeedMismatch`) → **recovery deadlock**.

The gap is **already named in the code** as an open finding, in three places
that share the ledger-walk projection:
- `fiscal_documents.rs:1395-1399` (the `active_chain_tip_unsigned_xml_sha256`
  scope note): *"A standalone T=112 replenish or MacReseed … leaves NO ledger
  trace and is NOT recoverable here — a durable seed-transition record is
  required (separate finding)."*
- `boot_phase.rs:1730-1733` (NC-03 uses that projection).
- `invariant_scan.rs:435-452` (the oracle uses that projection).

### 1.1 Reachability — CONFIRMED (proof-first)

The bug is **reachable**, and the surface is **wider than the bd stated**.

**Live-window (pre-crash) latent oracle false-positive — NEW finding.**
`invariant_scan.rs:446` compares `node_seed != active_tip` for every FN. After a
standalone replenish (no crash), `node_seed = Hs` but the ledger-walk
`active_chain_tip_unsigned_xml_sha256` returns `Hp` (or `None`) because no doc
carries `Hs`. So the scan emits a **false `ChainSeedMismatch`** the moment a
replenish lands, before any crash. `invariant_scan` is test-gated (zero prod
callers, `boot_phase.rs:1625`), so this is **not** a live operator escalation —
but it IS the fuzzer/oracle contract. A W-wave that emits `Replenish` then runs
the scan (or NC-03 recovery) would flag a legitimately-recovered `Hs` as a
mismatch. No existing test exercises `invariant_scan` OR NC-03 after a standalone
replenish (verified: `grep` of `tests/` for replenish+scan/boot = none), so the
false-positive is currently **untested and latent**, not caught.

**NC-03 window (post-crash).** As the bd describes: crash after replenish,
before the first SELL consumes a code and chains onto `Hs`. Narrow but reachable;
P2. Once a SELL chains onto `Hs`, `Hs` becomes doc-derivable
(`is_issued` arm of `active_chain_tip`) and the gap self-closes.

**Not covered by RULING 2.** RULING 2 §3 (lost-response ONLINE tip-lag, handled
by settle discipline) is the case where node_state SURVIVES and the local tip
lags DPS. hpc is the DISTINCT node_state-LOSS case; §3 explicitly does not cover
it (both the bd and RULING 2 wording agree). This design does not touch the
ambiguous-T112 no-retry doctrine (RULING 2 §1-2) — the witness is written only
on **DPS success**, exactly where the seed already advances today.

---

## 2. Current relevant architecture

### 2.1 The seed and its three consumers

`node_state.last_known_unsigned_xml_sha256` (BLOB, nullable, 32 bytes or NULL
for genesis) is the live MAC-chain tip. It is advanced by:
1. `stage_finalize` (online ACK / advance-at-SEND) — leaves a doc carrying the hash.
2. `stage_offline_ack` (offline OLA) — leaves a doc carrying the hash.
3. `NotAcceptedOffline` completion — **rewinds** to a held doc's `previous_hash`,
   marks that doc `chain_superseded_at` (migration 039) so the ledger walk can
   find the rewind target (bd 2nk fix).
4. **T=112 replenish** — advances to `Hs` with **no doc** (the gap).
5. MacReseed completion — installs an operator seed (guard-B gated).

Three consumers reconstruct/validate the tip from the **ledger** via
`active_chain_tip_unsigned_xml_sha256`:
- **NC-03 boot** (`boot_phase.rs:1734`) — recover after node_state loss.
- **MacReseed guard-B** (`delivery_reservation.rs:1402-1411`) — the operator
  seed must equal the active tip.
- **invariant_scan** (`invariant_scan.rs:440-452`) — oracle: `node_seed` must
  equal the active tip.

Cases 1-3 leave a ledger trace; the walk finds them. Cases 4-5 do not — that is
the whole gap. bd 2nk closed case 3 (rewind targets) via the `chain_superseded_at`
marker; this design closes case 4 (T=112) with the same shape: a durable ledger
trace that the SAME projection consults.

### 2.2 Why the walk cannot find `Hs` today

`active_chain_tip_unsigned_xml_sha256` (`fiscal_documents.rs:1400-1442`) walks
`fiscal_documents` newest-first and returns the first `chain_superseded_at`
doc's `previous_hash`, else the first `is_issued` doc's `unsigned_xml_sha256`,
else `None`. A standalone `Hs` matches none of these — there is no doc.

---

## 3. Proposed minimal change

### 3.1 WHERE to store the witness — decision

**Chosen: (a) a dedicated append-only seed-transition journal table**, but read
by **folding it into the SAME projection function**
(`active_chain_tip_unsigned_xml_sha256`) so all three consumers inherit the fix
with zero additional edits — mirroring exactly how bd 2nk added
`chain_superseded_at` handling inside that one function.

Evaluation of the three options:

| Option | Atomicity w/ seed advance | Survives NC-03 (node_state lost, ledger survives) | Ordering "newer than last doc" | Minimal diff | Verdict |
|---|---|---|---|---|---|
| **(a) journal table** `chain_seed_transitions(fiscal_number, seq, new_seed, source, created_at)` | YES — insert in the same `with_immediate` as `update_last_known_xml_sha_tx` | YES — separate table, not node_state | **YES — monotonic `seq` shared with the ledger's `lnd` frame** (see §4) | one migration + one INSERT + one branch in the projection fn | **CHOSEN** |
| (b) record `Hs` on the `offline_codes` rows the replenish inserts | YES — same insert | **PARTIAL** — but rows are consumable/mutable, and drill-seeded rows carry no code; recovering the tip means "find the newest offline_codes row with a non-null seed for this FN", which is fragile once codes are consumed/deleted and gives NO clean ordering vs docs | poor | medium | rejected — couples seed durability to a consumable pool; no doc/seed ordering frame |
| (c) node_state seed-history column | NO — **node_state is the thing that gets LOST in NC-03**; a history column there does not survive the exact failure we are recovering from | NO | n/a | small | **rejected — fails the survivability requirement by construction** |

Option (c) is disqualified outright: NC-03 is defined as "node_state row lost,
ledger survives", so anything stored in node_state is gone. Option (b) reuses the
existing atomic insert (the roadmap's "atomicity code-pool + seed" note) but
gives no ordering frame against documents and rots as codes are consumed. Option
(a) is the only one that is durable in the ledger tier AND gives a clean
monotonic ordering against documents.

### 3.2 The journal table (migration 040)

```sql
-- 040 — chain_seed_transitions: durable witness for NON-DOCUMENT seed advances
CREATE TABLE chain_seed_transitions (
    fiscal_number TEXT    NOT NULL,
    -- monotonic per-FN ordinal in the SAME frame as fiscal_documents.lnd:
    -- captured as the FN's current next_lnd at write time (see §4).
    lnd_at_write  INTEGER NOT NULL,
    -- the non-doc seed this transition installed (32-byte sha256).
    new_seed      BLOB    NOT NULL,
    -- provenance discriminator; only 'T112' in this slice. Future: 'MACRESEED'.
    source        TEXT    NOT NULL,
    created_at     TEXT    NOT NULL DEFAULT (datetime('now')),
    -- append-only: one row per non-doc advance; a later doc/advance simply
    -- appends a higher lnd_at_write. No UPDATE, no DELETE.
    PRIMARY KEY (fiscal_number, created_at, new_seed)
) STRICT;

CREATE INDEX ix_chain_seed_transitions_fn_lnd
    ON chain_seed_transitions(fiscal_number, lnd_at_write DESC);
```

Additive, STRICT, forward-only, no backfill (no historical standalone-T112 rows
exist pre-pilot). Mirrors the 028/039 STRICT/rollback reasoning: reverse = dead
table on pre-pilot DB reset.

### 3.3 The write (in the existing replenish envelope)

In `offline_code_replenish.rs` inside the existing `with_immediate` (line
259-295), AFTER `update_last_known_xml_sha_tx`, add ONE insert:

```
INSERT INTO chain_seed_transitions (fiscal_number, lnd_at_write, new_seed, source)
VALUES (?, ?, ?, 'T112')
```

where `lnd_at_write` = the FN's current `node_state.next_lnd` read inside the
same tx (the ordinal the NEXT document will take). No new state transition, no
new primitive beyond a repository insert fn. Invariant #8 style preserved
(seed advance stays via `update_last_known_xml_sha_tx`; the journal is a
sibling record, not a competing seed writer).

---

## 4. NC-03 selection rule (the ordering question — the crux)

The projection must choose between the doc-derived tip (`Hp`, via the existing
walk) and the T=112 witness (`Hs`). **Correctness hinges on ordering**: the
witness must win iff it is NEWER than the last seed-changing document; a stale
witness (a later doc chained past it) must lose.

### 4.1 The ordering frame: `lnd_at_write` vs `lnd`

The witness records `lnd_at_write = next_lnd at write time` = "the lnd the next
document would take". This places the witness in the SAME monotonic per-FN frame
as `fiscal_documents.lnd`, guaranteed by invariant #2 (single-writer per FN: the
replenish holds `acquire_fn_gate`, the same gate the write-path holds, so no doc
can interleave the read-of-next_lnd and the insert).

Property: a document with `lnd = k` was issued when `next_lnd` was `k`, and it
advances `next_lnd` to `k+1`. A witness written when `next_lnd = k` records
`lnd_at_write = k`. Therefore:
- **A witness with `lnd_at_write = k` is NEWER than every doc with `lnd < k`.**
- **A doc with `lnd = k` (issued at or after the witness) is NEWER than a witness
  with `lnd_at_write = k`** — the witness fired first (it recorded next_lnd = k),
  then the doc consumed lnd k. This is the "after-SELL" case: the SELL took the
  code and lnd k, chained onto `Hs`, and its own hash is now the tip.

Tie at equal ordinal (`witness.lnd_at_write == doc.lnd`) resolves to **the doc
wins** — the doc is strictly later in the single-writer sequence (it consumed
that ordinal after the witness reserved it). This is the load-bearing tie-break.

### 4.2 The selection rule (folded into `active_chain_tip_unsigned_xml_sha256`)

Extend the projection to consider the newest witness alongside the ledger walk:

```
let doc_tip: Option<(seed, ordinal)> =
    // existing walk, but also surface the ordinal of the doc that produced the tip:
    //   - chain_superseded_at doc → (its previous_hash, its lnd)
    //   - first is_issued doc     → (its unsigned_xml_sha256, its lnd)
    //   - else genesis            → None
let witness: Option<(seed, lnd_at_write)> =
    SELECT new_seed, lnd_at_write FROM chain_seed_transitions
    WHERE fiscal_number = ? ORDER BY lnd_at_write DESC, created_at DESC, rowid DESC LIMIT 1;

match (doc_tip, witness) {
    (None, None)        => None,                       // genesis
    (Some(d), None)     => Some(d.seed),               // pure-doc chain
    (None, Some(w))     => Some(w.seed),               // replenish before any doc
    (Some(d), Some(w))  =>
        if w.lnd_at_write >= d.ordinal  { Some(w.seed) }   // witness strictly newer
        else                            { Some(d.seed) },  // doc chained past → doc wins
}
```

Note the **`>=` in the witness-wins arm** is deliberate and is the tie-break from
§4.1 read the other way: a witness with `lnd_at_write = k` beats a doc-tip whose
producing doc had `lnd < k` (i.e. `d.ordinal <= k-1 < k`). When a SELL later
consumes lnd `k`, the doc-tip ordinal becomes `k`, so `w.lnd_at_write (k) >=
d.ordinal (k)` would be TRUE — WRONG. **Therefore the doc-tip ordinal for the
"after-SELL" doc is `k`, and the witness ordinal is also `k`; the witness must
LOSE.** Resolve by making the comparison **strict on the witness side**:
`w.lnd_at_write > d.ordinal → witness wins`, else doc wins. Re-stated cleanly:

```
(Some(d), Some(w)) =>
    if w.lnd_at_write > d.ordinal { Some(w.seed) } else { Some(d.seed) }
```

Verification of both sub-cases:

- **(i) NC-03 in the replenish→pre-SELL window.** The pre-replenish issued doc
  `Hp` has `lnd = k-1`; the witness has `lnd_at_write = k` (next_lnd was k). No
  SELL yet. `w.lnd_at_write (k) > d.ordinal (k-1)` → **witness wins → recover
  `Hs`.** Correct.
- **(ii) NC-03 AFTER a SELL chained onto `Hs`.** The SELL consumed lnd `k`,
  chained `previous_hash = Hs`, and (once issued) its own hash `Hsell` is the
  doc-tip with ordinal `k`. Witness still `lnd_at_write = k`. `w.lnd_at_write
  (k) > d.ordinal (k)` is FALSE → **doc wins → recover `Hsell`, not the stale
  `Hs`.** Correct.

Genesis-`Hp` edge: if the replenish is the FN's very first seed advance
(`Hp = None`, no prior doc), the doc walk returns `None`, witness wins by the
`(None, Some(w))` arm → recover `Hs`. Correct.

### 4.2.1 The `rowid DESC` tiebreaker (REVIEW FINDING — added after an empirical proof)

`(lnd_at_write, created_at)` alone is NOT a total order. A replenish allocates **no lnd**, so two
replenishes with no document between them share the same `lnd_at_write`; `created_at` defaults to
`datetime('now')` (**second** granularity), so two replenishes inside one second also tie on
`created_at`. The pick between the two tied rows was then left to the query plan.

**Empirically proven defect** (probe `tied_witnesses_same_second_recover_the_latest_seed`): with two
tied rows (`0xAA` then `0xBB`, `node_state` holding the later `0xBB`), the projection returned
**`0xAA`** — the EARLIER witness. After an NC-03 boot that reconstructs the wrong seed, and in the
live window it makes `invariant_scan` emit a false `ChainSeedMismatch`.

**Fix:** append `rowid DESC` as the final ordering key in BOTH witness reads (the repo's
`latest_seed_transition` and the projection's witness sub-select). The table is append-only (no
DELETE), so SQLite's implicit rowid is monotonically increasing per append and "highest rowid" ==
"most recently appended" == the seed `node_state` actually holds. The probe is RED before this fix
and GREEN after — it is the regression tooth for the tie.

### 4.2.2 The `invariant_scan` MAC-walk must ALSO re-anchor (REVIEW FINDING — empirically proven)

Folding the witness into `active_chain_tip_unsigned_xml_sha256` fixes the three consumers that ask
"what is the tip?" (NC-03 boot, guard-B, the scan's FINAL seed check). It does NOT fix the scan's
per-doc MAC-walk, which carries its own running `expected`. After a replenish the NEXT document
legitimately carries `previous_hash = Hs`, while the walk's `expected` still holds the pre-replenish
doc hash `Hp`.

**Empirically proven defect** (probe `scan_clean_when_a_doc_chains_onto_the_t112_witness`): the scan
emitted `ChainBreak { lnd: 5, expected: Hp, found: Hs }` for a perfectly legitimate link.

**Fix:** in the walk, re-anchor `expected` onto the newest witness whose `lnd_at_write <= doc.lnd`
AND `> last_advance_ordinal` (the ordinal of the last tip advance — a witness the chain already
passed must not re-apply). Structurally identical to the bd-2nk `chain_superseded_at` re-anchor that
sits a few lines above it. `last_advance_ordinal` is set by the superseded branch and by every issued
doc. The probe is RED before this fix and GREEN after.

### 4.3 Interaction with the bd-2nk `chain_superseded_at` rewind

If a `NotAcceptedOffline` rewinds the tip to a held doc's `previous_hash` that is
itself a non-doc `Hs` (the exact case the 2nk note calls out), the doc walk
already returns that `Hs` via the superseded doc's `previous_hash` with ordinal
= the superseded doc's `lnd`. If a witness for the SAME `Hs` also exists, both
agree on the value; the ordinal comparison picks one but the seed is identical →
no divergence. If a NEWER witness (a replenish AFTER the rewind) exists, its
higher `lnd_at_write` wins — correct, it is the newest seed event. No conflict.

---

## 5. Atomicity + crash semantics

The witness INSERT is in the SAME `with_immediate` envelope as
`update_last_known_xml_sha_tx` + `insert_dps_codes_tx` (replenish, lines
259-295). SQLite `IMMEDIATE` = one atomic commit.

- **Crash before commit:** the entire envelope rolls back — no codes, no seed
  advance, no witness. node_state seed stays at `Hp`; the ledger has no `Hs`
  doc; no witness row. The DPS call already returned success (codes issued
  server-side, chain advanced server-side), so this is the RULING-2 lost-response
  situation: local tip lags DPS. Recovery = a FRESH replenish (RULING 2 §2), which
  re-obtains the (dedup-safe) codes and re-advances + re-witnesses. **Consistent.**
- **Crash after commit:** codes + seed (`Hs`) + witness (`lnd_at_write = k`, seed
  `Hs`) all present. NC-03 (§4.2 case i) recovers `Hs`. **Consistent.**

There is NO window where the seed advanced but the witness did not (or vice
versa) — that is the whole point of putting the INSERT in the existing envelope.
This closes the "seed=Hs but no ledger trace" gap by construction, exactly as
bd 2nk closed "rewind but no marker" by putting the `chain_superseded_at` write
in the rewind's own tx.

---

## 6. guard-B / MacReseed interaction — witness-only path suffices

Once NC-03 recovers `Hs` correctly (via §4), the deadlock **dissolves without
touching guard-B**:

1. Boot recovers `node_state.last_known_unsigned_xml_sha256 = Hs`.
2. The next fiscal send reads `Hs`, chains onto `Hs` — DPS accepts (DPS expected
   `Hs`). **No BadHashPrev, no MacReseed hold at all.** The deadlock never arms.

guard-B only fires if a MacReseed hold is somehow reached. Even then: guard-B
(`delivery_reservation.rs:1402`) already calls
`active_chain_tip_unsigned_xml_sha256`. Because the witness is folded INTO that
function (§4.2), guard-B automatically sees `Hs` as the expected tip — so an
operator reseed to `Hs` would be ACCEPTED, not rejected. **No guard-B code
change; the witness feeds it transparently.** This is the preferred witness-only
path: guard-B is NOT weakened (any value ≠ the true active tip still fails
closed), and bd mcc is NOT reopened (a valid MacReseed's seed still == active
chain tip; the witness just makes the active tip correct for the T=112 case).

---

## 7. Files to touch (minimal-diff)

| File | Change | Hot zone? |
|---|---|---|
| `rust/prro/migrations/040_chain_seed_transitions.sql` | NEW additive STRICT table + index | migrations — plan-first (done) |
| `rust/prro/src/db/repositories/` (new `chain_seed_transitions.rs` or fold into `node_state.rs`) | one tx-bound `insert_seed_transition_tx` + one `latest_seed_transition` read | repositories |
| `rust/prro/src/db/repositories/fiscal_documents.rs` | extend `active_chain_tip_unsigned_xml_sha256` to surface the doc-tip **ordinal** and fold in the witness per §4.2 | repositories (SHARED projection — all 3 consumers) |
| `rust/prro/src/services/offline_sync/offline_code_replenish.rs` | add the witness INSERT inside the existing `with_immediate` (read next_lnd in-tx) | offline handling |

**No change** to: `boot_phase.rs`, `invariant_scan.rs`, `delivery_reservation.rs`
guard-B — they inherit the fix through the shared projection function. This is
the key minimal-diff lever: one function edit fixes all three consumers, exactly
as bd 2nk did.

---

## 8. Risks and invariant impact

**Frozen invariants:**
- **#1 (no wire/crypto in tx):** preserved — the witness INSERT is pure SQLite,
  inside the existing envelope; the DPS call and signing stay before it.
- **#2 (single-writer per FN):** LOAD-BEARING and preserved — the
  `acquire_fn_gate` around replenish is what makes `lnd_at_write = next_lnd`
  atomic and monotonic vs documents. If a doc could interleave, the ordinal frame
  would break. State this explicitly in the implementation.
- **#4 (idempotency):** a re-run replenish (fresh DI/TS per RULING 2) computes a
  DIFFERENT `Hs` (different request_xml) → a new witness row; append-only, no
  conflict. A byte-identical re-run is forbidden by RULING 2 anyway.
- **#8 (seed advance via existing primitive):** preserved — the seed still
  advances via `update_last_known_xml_sha_tx`; the witness is a sibling audit
  record, not a second seed authority.
- **Recovery must not silently violate transitions (#8-recovery):** NC-03 still
  ends BLOCKED; the only change is WHICH seed it projects. No new state.

**Hot-zone risks:**
- The shared projection edit is the highest-leverage AND highest-risk change:
  it is consumed by NC-03, guard-B, and the oracle. The ordinal tie-break (§4.2,
  strict `>`) is subtle — an off-by-one there silently corrupts recovery. This
  MUST be covered by the after-SELL directed test (§9) with a revert-canary
  (flip `>` to `>=` → the after-SELL test must go RED).
- `active_chain_tip_unsigned_xml_sha256` currently returns `Option<Vec<u8>>`; it
  needs the producing-doc ordinal internally. Prefer a private helper returning
  `(seed, ordinal)` and keep the public signature stable so guard-B / scan /
  boot call sites are untouched (minimal diff).

**bd mcc guard:** NOT reopened — a valid MacReseed's seed still equals the active
chain tip; the witness only corrects what the active tip IS for standalone T=112.

---

## 9. Tests / checks required

Directed tests (strict TDD, RED-first per project charter):

1. **NC-03 replenish→pre-SELL recovers `Hs`** (`app_boot_reconciliation.rs`):
   seed an FN with an issued doc (`Hp`, lnd k-1); run replenish (seed→`Hs`,
   witness lnd_at_write=k); DROP the node_state row; call
   `reconstruct_lost_node_state`; assert recovered seed == `Hs`, node BLOCKED.
   Revert-canary: without the witness fold, recovers `Hp` → test RED.
2. **NC-03 after-SELL recovers the SELL hash, not stale `Hs`**: as above, then
   issue an offline SELL consuming lnd k, chaining `previous_hash = Hs`,
   `unsigned_xml_sha256 = Hsell`; DROP node_state; reconstruct → assert `Hsell`,
   NOT `Hs`. Revert-canary: flip the §4.2 comparison `>`→`>=` → test RED.
   This is the ordering-correctness tooth.
3. **Crash-atomicity** (`offline_code_replenish.rs` test or an integration
   test): inject a failure inside the `with_immediate` after the seed advance but
   before commit; assert NONE of {codes, seed=Hs, witness row} persist (all
   roll back together). And a success path: assert ALL THREE present with matching
   `Hs`.
4. **invariant_scan clean after standalone replenish** (`invariant_scan.rs`):
   replenish an FN (no crash, no SELL); run the scan; assert **no
   `ChainSeedMismatch`** (today this false-positives — this test is RED before
   the fix, GREEN after; it locks the §1.1 live-window finding).
5. **guard-B accepts a reseed to `Hs`** (`delivery_reservation` completion test,
   defensive — the happy path should never reach a hold): construct a
   MacReseedPending hold on a replenished FN; complete with `seed = Hs`; assert
   ACCEPTED (guard-B reads the witness-fed active tip). And `seed = Hp` →
   `MacReseedSeedMismatch` (fail-closed preserved).

**Fuzzer impact (per project rule: new fiscal behavior → fuzzer question).**
The invariant fuzzer models seed transitions. The witness must be modeled in the
wave that emits `Replenish` (the T=112 track — this is RULING 2's **W3** family,
"local vs remote tips"). Specifically:
- The model's expected active tip after a `Replenish` op must become `Hs`, and
  the oracle (`invariant_scan` / `active_chain_tip`) must agree — currently they
  diverge (§1.1), which would surface as a spurious differential. Naming: this
  belongs in **W3-1/W3-2** (RULING 2 §3, "ambiguous T=112 followed by a fiscal
  doc must recover, never fork") extended with a **crash-between-replenish-and-SELL
  → NC-03 recover** generator step. The `Replenish` alphabet symbol is a known
  gap ([[project_fuzzer_alphabet_gaps]]). Add a directed teeth item to the
  mutation backlog for the §4.2 ordinal tie-break.

**Cheap checks:** `cargo fmt`, `clippy -D warnings`, `--all-features nextest` on
`app_boot_reconciliation`, `invariant_scan`, `offline_code_replenish`,
`delivery_reservation` test targets, plus a full nextest gate pre-push
(migration touches everything).

---

## 10. Rollback / containment plan

- Migration 040 is additive/nullable-free-but-new-table/forward-only; rollback
  = pre-pilot DB reset (per 028/039 doctrine). The new table is inert to all
  existing reads.
- If the projection fold proves risky, containment = a feature-flag-free but
  narrowly-scoped change: the witness is consulted ONLY when the doc walk would
  otherwise return a value the operator can't reconcile. But do NOT over-engineer
  a flag; the tie-break is deterministic and testable — prefer the direct fold.
- The change is behavior-preserving for every FN that never ran a standalone
  T=112 replenish (empty `chain_seed_transitions` → `(Some(d), None)` /
  `(None, None)` arms → identical to today). Blast radius is confined to
  replenished FNs.

---

## 11. Minimal-diff assessment

Smallest change that closes the gap: **one migration + one INSERT in the
existing replenish envelope + one fold into the shared projection function.**
No new state machine, no guard-B change, no boot/scan/guard call-site edits.
This mirrors the proven bd-2nk shape (marker + same-tx write + one projection
edit consumed by all three sites), which is the strongest evidence the approach
is sound and correctly scoped.
