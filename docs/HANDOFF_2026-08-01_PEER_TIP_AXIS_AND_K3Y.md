# Handoff — 2026-08-01: the peer-tip axis landed, and it is finding things

`main` tip at handoff: **`b0662103`**. Ten PRs merged (#358-#366); **#367 open** (docs, awaiting CI).

> **Read this first.** Three findings came out of one evening, and the two that matter are *not*
> fuzzer-hygiene items — they are **state divergences with DPS**, reached from two different
> directions. `bd PRRO_GATE-k3y` is **P1 and un-tested**: a shift DPS accepted can end up marked
> `ERROR` locally. The reading behind it is strong and fully cited, but it is still *reading* — the
> owed directed test is spelled out in §4.

---

## 1. What landed

| PR | what |
|---|---|
| #358 | **`q5u` (P1, closed)** — a deterministic tax-config defect no longer parks a doc in `PREPARED` forever |
| #359 | `-12` promoted out of the assertion-free fault bucket into the oracle |
| #360 | **`0ps` (P1, closed)** — ADR deciding the DPS RPC surface + a machine-checked tooth |
| #361/#362 | handoff + the peer-tip axis spec (rev2, then rev2.1) |
| #363 | **peer-tip axis PHASE A** — the harness now models the DPS peer's chain tip |
| #364 | **`01g` (closed)** — a NON-DOCUMENT chain tip must not alias onto `max(lnd)` |
| #365 | prune of the merged supersession row |
| #366 | **canonical fingerprints** — the provenance live-drift leg no longer depends on git reachability |

## 2. The axis paid for itself immediately

Phase A ships the peer's tip as harness state plus ONE assertion — *while the run has not diverged,
every outgoing document's `previous_hash` already equals the peer's tip* — and overrides nothing.
It caught three things in one evening:

1. **My own missing mover**, on its first run (T=112 rides `ask_offline_codes`, a queue the
   send-side observer never sees).
2. **`bd PRRO_GATE-knk`** — see §3.
3. **`bd PRRO_GATE-01g`** — pre-existing, verified to reproduce on the parent branch *without* the
   axis. Root cause worth remembering: a comment that was **correct when written** and was
   invalidated by an **alphabet extension** ("reachable only via a MacReseed rebase —
   generator-excluded" stopped being true when the generative `Replenish` symbol landed).

Note the rev1 → rev2 → phase A road: a 5-agent adversarial pass killed rev1's movers table (two
FATALs), and then **phase A's assertion killed one of rev2's own claims**. rev1 had proposed an
"inert" phase A that asserted nothing — it would have shipped both blind spots.

## 3. `knk` — SETTLED LIVE, downgraded P1 → P2

The open node was: does DPS accept a T=112 whose `<MAC>` it has never seen? **It does not.**

```
[2] ask_offline_codes (T=112) with a FOREIGN <MAC>
    DPS REFUSED — verbatim:
      Server { code: -12, message: "ERROR_BAD_HASH_PREV  store abc15386…5531 chk 6aa74325…dbac" }
[3] tip AFTER = abc15386…5531   ← UNCHANGED
```

Probe: `rust/prro/tests/live_probe_knk_t112_foreign_mac.rs` (TEST cabinet, FN 4000162280). It hands
T=112 a foreign MAC directly instead of building a real backlog — **zero fiscal documents minted**,
nothing persisted locally. Re-runnable.

**What remains** (P2): with a diverged chain an operator **cannot replenish** — they get `-12` →
`MacReseedPending` → `STOP_MODE`, and nothing tells them the fix is to drain first. The pressure to
replenish peaks exactly when the backlog is largest. So the fix is an explicit refusal naming
"drain first", **not** a chain fence.

**Keep both live observations distinct — they are different input classes and both stand:**
- *stale-but-previously-seen* tip (the 2026-07-31 H2 capture) → DPS **accepts** and re-bases. This
  is what makes a granted replenish a divergence-**healing** move.
- *never-seen* value (this probe) → DPS **refuses**.

Neither may be restated as the general rule. The `[N=1]` marking on the earlier one is exactly what
stopped the spec from building on it.

**Spun out:** `ReplenishLeaf::Granted` is a free generator choice, so the model can emit a granted
replenish where production earns `-12` — vacuous coverage, and the reason `knk` first looked like a
P1. Constraining it belongs to phase B. Its own ticket exists.

## 4. `bd PRRO_GATE-k3y` — P1, UN-TESTED, do this first

**Claim:** a shift DPS accepted ends up marked `ERROR` locally, projection reset to `Closed`.

The chain, all cited in the ticket:

1. `apply_orchestration.rs:226-240` fires `confirm_shift_edge(Opening → Opened)` after
   `apply_outcome` — the **auto-apply** path, and the only route to `apply_shift_transition` (the
   sole `node_state.shift_state` writer).
2. The **operator** path does not go through it: the CLI runs `admin.rs:498 → admin.rs:537` straight
   into `complete_operator_pending`, which does its own CAS + SFN stamp + seed advance but cannot
   call the service layer. The service that would, `operator_completion::complete_operator_resolution`,
   calls itself "The SOLE production caller" and has **zero** production callers.
3. `boot_phase.rs` branch (e2), when `pending.is_empty()`, selects every shift in
   `OPENING`/`CLOSING` and `force_orphan_shift_to_error`s it — a RAW update to `ERROR` that
   deliberately bypasses the transition whitelist — then resets the projection to `Closed`.
4. Timing verified: `SENT` counts as pending (`fiscal_documents.rs:754`), so (e2) stays quiet right
   after the completion; once the doc reaches `ACK`, pending empties and the **next** boot fires.

**The fuzzer cannot catch this today** — its interpreter drives the same admin seam, so the model
reproduces the bypass along with the defect.

**OWED, and the reason this is not already fixed:** a directed RED-first test
`[hold a SHIFT_OPEN → OperatorComplete(Accepted) → advance the doc to ACK → boot]` asserting the
shift is `OPENED` and the projection is `Opened`. It settles the last doubt (does the orphan SELECT
really match an issued SHIFT_OPEN's row?) and pins the contract either way. **Do not fix before
that test is RED.**

Fix direction, once adjudicated: route the CLI through `complete_operator_resolution` — one place,
and it revives the lost Critical `OPERATOR_COMPLETION` audit. Teaching (e2) to skip issued shifts
treats the symptom. Either way, `operator_completion.rs:3` must be corrected: it asserts a
production role it does not have.

## 5. Reusable lessons (all paid for tonight)

- **Adding a generator symbol invalidates assumptions of the form "X is generator-excluded"** —
  they were written about the old alphabet. This is how `01g` was born.
- **Committing a seed to `invariant_fuzzer.regressions` perturbs the whole subsequent search**
  (corpus replays first). New redness after adding seeds must be triaged against the parent branch
  before blaming your own change.
- **Never commit a RED seed** — the corpus replays at every scale including the PR gate.
- **Run the full pre-push set AFTER the final edit**, not after the edit that felt final. This cost
  #360 two red CI rounds (clippy on a branch-only file; then a stale `source_files.sha256`, which
  hashes the test files themselves).
- **Squash-merging a stack** needs a manual retarget per level (`gh pr edit --base` fails on
  deprecated GraphQL — use the REST API), and each level then conflicts on the generated artefacts.
- A manifest minted against an old base **must be re-minted after merging main**, or control 1
  (`live == committed`) goes red *on main*.

## 6. Environment notes

- The provenance leg no longer depends on branch reachability, so **merged branches can finally be
  deleted** — that was the point of #366.
- `LIVE_DRIFT_BASE_SHA` is gone from the code; **stale mentions remain in docs and in the two CI
  workflow comments** — worth a cleanup pass.
- Live probes: key at `/home/setter/prro_gate/key_13667753_13667753 (2).jks`, FN `4000162280`,
  TN `13667753`, TEST cabinet only (default-deny allowlist; production hosts hard-refuse).
  **The JKS password passed through a chat transcript twice now** — the key is a test key and was
  already flagged compromised, but the rotation should actually be completed.
- `rtk` summarises `cargo test` output and will swallow `--nocapture`; use `rtk proxy cargo test …`
  for live probes.

## 7. Next steps, in order

1. **`k3y`** — write the RED-first directed test of §4, adjudicate, then fix. Highest value: it is
   the only open item touching fiscal correctness rather than test quality.
2. **`knk` fix (P2)** — an explicit refusal on the replenish path naming "drain first".
3. **Phase B of the peer-tip axis** — derived `-12`, which also closes `5hc` (the MacReseed SUCCESS
   path becomes generatively reachable) and is where the `Granted`-leaf constraint belongs. Only
   start once the movers table has settled green across a few full runs.
4. Merge **#367**; clean up the stale `LIVE_DRIFT_BASE_SHA` mentions; delete merged branches.
5. Then **CS-4** — spec #6 + the thin per-FN coordinator, routing exactly one command through it.

Phases C and D of the axis (model mirror, ambiguous-T112 leaf = `2ds`) come after B; the spec's §9
carries their shape and their teeth.
