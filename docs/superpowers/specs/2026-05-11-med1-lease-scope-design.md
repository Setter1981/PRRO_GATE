# MED-1 Lease-Scope Resolution Design

**Date:** 2026-05-11
**Author:** arch-planner
**Status:** PROPOSED — awaits operator decision
**Scope:** Pre-W11 resolution of MED-1 (`docs/superpowers/specs/2026-05-11-w10-final-audit.md`)
**References:** mac_recovery docstring at `rust/prro/src/services/write_path/mac_recovery.rs:280-310` (w10 worktree)

---

## 1. Verified facts

All citations resolved against the **w10 worktree** (`/mnt/d/PRRO_GATE-m3a-w10`, HEAD `f994ed0`) since `rust-gateway` proper is still at W8 (`1d29315`); the audit's MED-1 references files that live only on the W10 branch.

- **Lease docstring drift sites** (claim "caller holds single-writer-per-FN lease"):
  - `rust/prro/src/services/write_path/mac_recovery.rs:280-310` (primary MED-1 site — full "Caller obligation: single-writer-per-FN lease" section).
  - `rust/prro/src/services/write_path/mac_recovery.rs:340-342` ("Reordering is safe under single-writer-per-FN: the inputs we [pin] cannot be reordered by another writer holding the same lease").
  - `rust/prro/src/services/write_path/stage_send.rs:139` ("single-writer-per-FN + the 4-pre marker we just committed").
  - `rust/prro/src/services/write_path/stage_send.rs:152` ("the single-writer invariant: CAS Applied means the row exists").
  - `rust/prro/src/services/write_path/stage_finalize.rs:98,264` ("Impossible under M3a single-writer + the same [BEGIN IMMEDIATE envelope]").
  - `rust/prro/src/services/write_path/stage_acquire.rs:25,29,32,46` ("lease + guard + lnd-allocate" — `acquire_lease` keyed on `request_id`, see fact 4).

- **No FN-keyed lock primitive exists.** `grep "Mutex|RwLock|DashMap"` across `rust/prro/src/services/`, `rust/prro/src/db/repositories/`, and `rust/prro/src/app.rs` returns **zero hits** in both worktrees. The only `Mutex` references in the wider tree are `transports/dps/grpc.rs:7,35` (tonic client) and `crypto/provider.rs:41` (transport state) — neither is an FN-scope serialiser.

- **The "lease" the docstrings name is `ingress_inbox::acquire_lease`**, defined at `rust/prro/src/db/repositories/ingress_inbox.rs:181-204`. It is a row-status CAS keyed on `request_id` (`UPDATE ingress_inbox SET status='PROCESSING' WHERE request_id=? AND status='NEW' RETURNING ...`). Fact 4 confirms the key.

- **`acquire_lease` is keyed on `request_id`, NOT on `fiscal_number`** (`ingress_inbox.rs:181-189`). It guarantees one-time inbox-row consumption, not FN-scope exclusion. Two parallel workers handling two distinct inbox rows on the same FN would each pass `acquire_lease`.

- **W11 is single-worker test-only.** `docs/superpowers/plans/2026-05-07-m3a-implementation.md:426-451` defines W11 as "Deterministic-replay invariant — 9 crash-point fixtures": pure test fixtures driving `App::reconcile_pending`. Files: `rust/prro/tests/write_path_deterministic_replay.rs` only. Plan grep returns zero hits for `multi-worker|worker pool|dispatcher`. Multi-worker arrives in M3b at earliest (plan §"What this plan does NOT do" at line 469ff lists M3b deferrals; concurrent workers not enumerated as M3a scope).

- **`with_immediate` is the sole cross-doc serialiser today** (`rust/prro/src/db/tx.rs:118-124`): `sqlx::query("BEGIN IMMEDIATE").execute(...)`. All write-path stages run inside a `with_immediate` envelope (confirmed via `assert_not_in_with_immediate` guards in `crypto/in_process.rs:35,59,71,91` and the repository-level docstrings at `fiscal_documents.rs:18-19,204-227,314,399-404,453,571,619,654,710,747`).

---

## 2. Today's safety model

**What actually serialises writes today:**
1. **SQLite `BEGIN IMMEDIATE`** — global write-side mutual exclusion. Two parallel write txs across any docs/FNs serialise on the WAL writer.
2. **Document-state CAS** — every state transition is a conditional `UPDATE ... WHERE state = expected_prior`. Lost CAS races return `RowsAffected = 0` and the loser bails.
3. **Inbox row-status CAS** (`acquire_lease`) — request-id-scoped, one-shot consumption of a NEW inbox row.
4. **Single tokio worker by deployment** — the production write-path runs as one orchestrator task; `app.rs` does not spawn N workers.

**What the MED-1 docstring claims** vs reality:
- The docstring says callers "hold the single-writer-per-FN lease for `doc.fiscal_number`" (`mac_recovery.rs:286`).
- No code construct in the binary takes any FN-keyed lock. The runtime invariant is "one worker total, serialised by BEGIN IMMEDIATE + per-row CAS" — which is a **global-single-writer** model, not a **per-FN lease** model.
- Today this is safe (single worker + BEGIN IMMEDIATE is strictly stronger than per-FN exclusion).
- The docstring is **aspirational documentation, not load-bearing code**: it describes a constraint future multi-worker code MUST satisfy, but the term "lease" overloads `acquire_lease`'s inbox-row CAS, which does NOT provide FN-scope exclusion.

---

## 3. W11 scope dependency

W11 (`docs/superpowers/plans/2026-05-07-m3a-implementation.md:426-451`) is **9 crash-point test fixtures** driving `App::reconcile_pending`. It is **test-only**, single-worker, single-orchestrator. It does NOT introduce concurrent write paths.

Therefore: **MED-1 has no functional blocker on W11.** The docstring drift can be resolved either before or after W11 without changing W11's slice scope. The audit's "pre-W11 commitment" is a documentation/posture concern, not a correctness one.

Multi-worker arrives no earlier than M3b. The plan's §"What this plan does NOT do" (line 469ff) does not enumerate multi-worker dispatch; that conversation has not opened.

---

## 4. Three sub-options

| Option | Approach | When MED-1 closes | LoC | Migration | Audit churn |
|---|---|---|---|---|---|
| (a-memory) | `Arc<DashMap<FN, Arc<Mutex<()>>>>` in `app.rs`, acquired at the top of the write-path orchestrator and held for the lifetime of one write-path run; released on drop. | Now (W10.x patch) | ~150 | no | low — `mac_recovery.rs`, `stage_send.rs`, `stage_finalize.rs`, `app.rs`, one new test file |
| (a-DB) | `fn_writer_lock(fiscal_number TEXT PK, owner_token BLOB, acquired_at INT)` table + `INSERT ... ON CONFLICT FAIL` to take, `DELETE WHERE owner_token=?` to release; called from write-path orchestrator. | Now (W10.x patch) | ~200 | yes (migration `014_fn_writer_lock.sql`) | medium — schema churn, lease-leak recovery on crash needs reasoning |
| (b-rename) | Rename invariant in docstrings from "single-writer-per-FN lease" to "global-single-writer worker (M3a) — future multi-worker callers MUST add FN-scope exclusion before invoking". Add explicit ADR-M3-A10 capturing the deferral. Audit pass over all six drift sites (fact 1). | Now (docs only); real lock deferred to the multi-worker M3b slice that needs it. | ~30 | no | high — full audit pass over 6 sites + new ADR |

Notes on each option:

- **(a-memory)** matches `CLAUDE.md` "prefer explicit code over speculative abstractions" only weakly: today's binary does not race; we'd be adding a serialiser purely so the docstring becomes true. The lock has no observable behavioural effect under single-worker deployment. Recovery on crash is trivial (process restart clears the map).

- **(a-DB)** survives process crash but introduces orphaned-row recovery (who reclaims locks whose owner died?). For M3a's deployment posture (one process, one worker) this is over-engineering. It also adds a schema migration to a release that already has 013.

- **(b-rename)** is the smallest honest fix. It says out loud what the code does — global-single-writer plus `BEGIN IMMEDIATE` plus per-row CAS — and names the future obligation in an ADR. The "lease" word in `acquire_lease` is left alone (its scope is genuinely correct for inbox-row consumption); we only retire the misnaming in write-path stage docstrings.

---

## 5. Recommendation

**Option (b-rename).**

Reasoning:
- **CLAUDE.md decision rule:** "If a task can be solved either by changing architecture or by wiring the existing seam: wire the seam." — and here even the seam is invisible. The architecture is sound today; only its documentation lies. Fix the documentation.
- **CLAUDE.md decision rule:** "If a task can be solved either by clever abstraction or explicit code: prefer explicit code in hot paths." — `write_path/mac_recovery.rs` is the hottest of the hot zones. Adding a lock primitive that has no observable runtime effect under M3a is precisely the speculative-abstraction pattern the project bans.
- **W11 is single-worker test-only** (fact 5). No incoming code change in M3a will exercise a hypothetical per-FN lock; the lock would have zero coverage.
- **ADR-M3-A5** (and the wider ADR series) treats the global write-side serialiser + per-row CAS as the canonical safety model. (Inferred — needs cross-check against the ADR text; see open question 1.) Naming this honestly under a new ADR-M3-A10 is consistent with that posture.
- **MED-1 is a documentation drift finding**, not a correctness finding. The audit itself notes "Today safe (single worker + BEGIN IMMEDIATE)". The cheapest closure is to make the documentation match.
- **Carry-forward is clean:** when multi-worker arrives in M3b, the new slice owns introducing FN-scope exclusion (option (a-memory) or (a-DB) at that time) and updating the ADR; ADR-M3-A10 becomes the explicit pre-condition that slice closes.

---

## 6. Slice scope (option b-rename)

**Files touched (6):**
- `rust/prro/src/services/write_path/mac_recovery.rs` — rewrite `# Caller obligation: single-writer-per-FN lease` section (lines 280-310) to `# Caller obligation: global-single-writer worker (M3a; see ADR-M3-A10)`; rewrite the inline comment at 340-342.
- `rust/prro/src/services/write_path/stage_send.rs` — adjust lines 139 and 152 to use the renamed invariant.
- `rust/prro/src/services/write_path/stage_finalize.rs` — adjust lines 98 and 264.
- `rust/prro/src/services/write_path/stage_acquire.rs` — clarify lines 25-46 that "lease" here means `ingress_inbox::acquire_lease` (request-id-scoped inbox-row CAS), not an FN-scope lock.
- `docs/adr/ADR-M3-A10-global-single-writer.md` — new ADR (~80 lines) capturing: current safety model (BEGIN IMMEDIATE + per-row CAS + one worker), what multi-worker slices MUST add (FN-scope exclusion, lock-leak recovery, contention metrics), and the explicit deferral.
- `docs/superpowers/specs/2026-05-11-w10-final-audit.md` — append "MED-1 RESOLVED via ADR-M3-A10 + docstring pass" note with commit reference.

**Test surface:**
- No test changes required (option (b) is doc-only). One optional addition: a `#[doc = include_str!(...)]` smoke test that confirms ADR-M3-A10 file exists and is non-empty, to prevent the ADR from drifting away from the docstring references.

**Risk surface vs CLAUDE.md hot zones:**
- Hot-zone files touched: `services/write_path/*` — touched by docstring only; no compiled-code change. `cargo check` passes by construction.
- No transport, no crypto, no migration, no state-machine change.

**Single PR:** yes. ~30 LoC of substantive doc change + new ADR file. Trivially reviewable.

---

## 7. Carry-forward

Option (b-rename) does **not** introduce per-FN exclusion. The future M3b slice that opens multi-worker dispatch carries the obligation:

- **Slice name (proposed):** "M3b-Wn: multi-worker write-path orchestrator + FN-scope exclusion".
- **Pre-conditions encoded in ADR-M3-A10:** the slice MUST add either option (a-memory) or option (a-DB) — choice deferred to that slice based on then-current deployment posture (single process vs multi-process).
- **Audit hook:** ADR-M3-A10 becomes part of the M3b entry checklist; the M3a → M3b handoff document references it.

---

## 8. Open questions

1. **Does an existing ADR (suspected M3-A5) already enshrine the global-single-writer model?** — needs alignment. If yes, ADR-M3-A10 may be redundant and we can extend the existing ADR instead. If no, A10 is justified.
2. **Should `stage_acquire.rs`'s use of "lease" for `ingress_inbox::acquire_lease` be renamed too?** — recommended: leave it. The inbox row-status CAS genuinely is a lease-on-request-id; only the write-path-stage docstrings overloaded the term to mean FN-scope exclusion. Confirmation requested.
3. **Audit document amendment vs new follow-up audit?** — recommended: amend in place with a dated "MED-1 resolved" footer. Confirmation requested.
4. **Should the ADR also retire any remaining "per-FN" language elsewhere in the codebase** (e.g. `CLAUDE.md` invariant 2: "One `fiscal_number` = one logical single-writer write-path")? — invariant 2 is fine: it describes the **logical** model, which the global-single-writer implementation correctly satisfies. No `CLAUDE.md` change recommended. Confirmation requested.

---

**End of design doc.**
