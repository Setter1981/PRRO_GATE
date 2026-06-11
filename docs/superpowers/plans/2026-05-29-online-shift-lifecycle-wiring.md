# Online shift-lifecycle wiring — implementation plan (external review)

- **Date:** 2026-05-29
- **Proposed branch:** `feat/m4-online-shift-lifecycle` off `rust-gateway`
- **Scope class:** hot-zone (fiscal write-path: `stage_acquire` + `stage_send` + shift/node-state). Frozen-invariant-adjacent (**INV-03** "shift opened before fiscal operations").
- **Origin:** operator directive 2026-05-29 — "подключи SHIFT_OPEN→OPENED / Z→CLOSED" — after W4-Z3 live cycle proved the WIRE (SHIFT_OPEN→SELL→Z all ACCEPTED by real DPS) but exposed that the gateway's LOCAL shift state never opens on the online path.

> **Reviewer: read §2 (Investigation findings) and §3 (Central decision) FIRST.** The task is NOT a small additive edge — investigation showed the entire shift-table state machine is test-only scaffolding today, so wiring "open the shift" forces an architecture decision about the production source of truth.

---

## 0. REVISION 2 — 2026-05-30 (round-3 code-grounded review; READ FIRST — SUPERSEDES §1, §3, §4, §5, §9 where they conflict)

The first cut recommended **Option A (online-only)**; the operator chose **Option A′ (wire online + offline)**. A round-3 code-grounded review then found the §0 premise wrong and the A′ design under-specified. **This is the corrected, implementation-ready basis.**

### 0.1 Verified reality (round-3-corrected — the gateway does NOT transact in production today)
- Prod bootstrap seeds **`shift_state = Closed`** (`boot_phase.rs:1304` `upsert_initial(pool, fn, Online, Closed, 1)`), **NOT `Opened`** (the earlier draft's premise was inverted; the only `OPENED` write in src is a `#[cfg(test)]` fixture, `admin.rs:903`). Under `Closed`, `(Sell, Closed) → ShiftNotOpen` **refuses** the SELL (`stage_acquire.rs:897`). **So on HEAD the gateway cannot transact at all** — the gate is shut, not silently open.
- `current_shift_id` never set; `shifts` never populated (`insert_created` `shifts.rs:119` ZERO prod callers).
- **Offline Pattern C is unreachable END-TO-END:** `node_state` has **no Offline/GoingOffline mode setter** (only `set_mode_blocked_tx`/`set_mode_stop_mode_tx`); `OfflineSessionService::open_session` has **zero prod callers**; `stage_offline_ack` requires `Opened` + an active session (`stage_offline_ack.rs:268-318`). So mode never flips Offline → no session → no `OFFLINE_LOCAL_ACK` doc → no backlog → `drain()` early-returns. The offline drain **never runs**; its safety machinery (pending-drain lockout §3.3, drain-reject→manual INV-19) is unreachable — not via a crash but because nothing upstream is wired. **This strengthens the NO-GO.** (The fix3-caps "silently non-functional, not a crash" framing remains correct; the *mechanism* is the stronger "no backlog ever forms", not "Opened→None finalizes a backlog".)
- 🔴 **PREREQUISITE (NEW, blocking, Piece 0a):** **`stage_acquire::run` has ZERO production callers** (`stage_acquire.rs:48`). There is no live ingress→write-path worker in `rust/prro/src`: `stage_send::run` / `stage_sign::run` / `dispatch_post_sign` run ONLY from `boot_phase.rs` (crash recovery) + `backlog_drain.rs` (drain). **So A′'s stage_acquire/stage_send hooks will NOT fire on live ingress until a live ingress write-path worker exists.** Piece 0a must wire (or locate, if it lives out-of-`rust/prro/src`) that driver first — without it the whole shift lifecycle (and online operation generally) does not run, regardless of the edge hooks.

### 0.2 Decision: **Option A′** (operator-chosen, 2026-05-30) — wire online + offline.
(Option C = descope offline / online-only supervised pilot, kept as the smaller fallback. A′ is chosen because offline-on-DPS-unreachability is the legally-mandated PRRO mechanism, INV-08.)

### 0.3 Corrected A′ design (SUPERSEDES §4/§5 — those are online-only + insufficient)

**New repo primitive (REQUIRED):** `shifts::insert_created` is **pool-bound (autocommit)** — it opens its own connection and CANNOT run inside a `with_immediate`/`BEGIN IMMEDIATE` envelope. Add **`insert_created_tx(tx: &mut WriteTxConn<'_>, …)`** so row-create + state-write + `current_shift_id` set commit in ONE envelope. (§5's "no new fns" is wrong.) `opened_by_cashier_id` is **NOT NULL** (`016:131`, §16.8 1-cashier invariant) — source it from the canonical command's signer identity; if the cashier-registry FK is deferred pre-pilot, **flag it; do NOT pass the `__pre_w14a1__` sentinel** (that silently defeats §16.8).

**node_state mirror (REQUIRED):** reuse/promote the drain's `mirror_node_state_shift_state_tx` (`backlog_drain.rs:2226-2253`) — CAS on `(fiscal_number, shift_state, current_shift_id)`, `rows_affected != 1` → structural-drift error. Do NOT add parallel `set_shift_*` setters that omit `current_shift_id` (the §5 draft did — it diverges from the drain's discipline).

**Hook table (corrected, BOTH channels):**

| Edge | Transition | Hook | Channel | Notes |
|---|---|---|---|---|
| create | shift row → `Created` + `current_shift_id` | online: `stage_acquire`; **offline: INSIDE the `stage_offline_ack` envelope** | both | offline create co-located with edge 2 per spec §3.3:91 — NOT split to stage_acquire (else orphan-shift window if offline-ack later refuses on NoActiveSession/CodePoolExhausted) |
| 1 | `Created→Opening` | `stage_acquire` SHIFT_OPEN accept | online | intent-marker |
| 3 | `Opening→Opened` | `stage_send` SENT-commit (4-b) | online | DPS-Ack = Sent + server_fiscal_no; thread `inputs.doc_type` + current_shift_id into the SENT closure (PRE-closure `inputs` is not in scope at 4-b as-is) |
| 2 | `Created→OpenedLocalPendingDrain` | `stage_offline_ack` | offline | **= spec W10a (reserve≥2 code-pool policy gate) + W10b (stage_offline_ack must ACCEPT `DocType::ShiftOpen`)**, BOTH currently UNIMPLEMENTED — this is a BODY (shift-row INSERT + reserve gate + DocType acceptance + recovery), NOT a one-liner |
| 8 | `Opened→Closing` | `stage_acquire` Z/SHIFT_CLOSE accept | online | |
| 10 | `Closing→Closed` | `stage_send` SENT-commit | online | |
| 9 | `Opened→ClosingLocalPendingDrain` | `stage_offline_ack` | offline | offline Z while shift `Opened` |
| **7** | `OpenedLocalPendingDrain→ClosingLocalPendingDrain` | `stage_offline_ack` | offline | **offline Z while shift opened-offline-not-yet-drained — was MISSING from A′; edge 9 (source `Opened`) does not cover it** |
| 5/13/6/14 | drain finalize / escalate | `backlog_drain` (ALREADY coded) | offline | become reachable once `current_shift_id` + pending-drain set — **no drain change for the transition plumbing**; BUT **verify** `process_one_doc` classifies drain rejects as `DocVerdict::Failed{manual_recon:true}` (§6.3 universal-EscalateManual) else edge 6/14 never fires |

**Idempotence + crash-safety:**
- Place the shift-create strictly in the **fresh-Proceed path** (AFTER resume-detect `stage_acquire.rs:554-563` + terminal-dup `:570`) so re-drives short-circuit before insert.
- Derive `shift_id` **deterministically** from the SHIFT_OPEN document_id/request_id (NOT random `ShiftId::new()` — there is no prod ShiftId generator today) so an accidental re-create collides on PK; add a **partial UNIQUE index** `ON shifts(fiscal_number) WHERE state IN (open-states)` as a fail-closed backstop (none exists — only non-unique `ix_shifts_fn_state`).
- **Co-write `shift_state` + `current_shift_id` in ONE envelope** (else `stage_acquire` Step 5 rejects `ShiftInvariantViolation` on open-state-with-NULL-current_shift_id, `:440-453`).
- **Crash between create and confirm:** orphan boot resolution ERRORs the `OPENING` shift + sets `node_state` `CLOSED` but does NOT clear `current_shift_id` → dangling pointer (self-heals next SHIFT_OPEN). **Extend orphan resolution (`boot_phase.rs:1491`) to clear `current_shift_id`** + add a regression test.

**Cutover/migration:** prod FNs are at `Closed`/NULL → first SHIFT_OPEN is clean (`(ShiftOpen, Closed)→allow`, no node_state backfill). Enumerate in-flight `fiscal_documents` (`Sending`/`ErrorRetryable`, `shift_id=NULL`) at cutover (shift-neutral for signer_guard, or drain them first); historical Ack'd docs are an immutable ledger — no backfill.

### 0.4 Corrected piece decomposition (A′)
- **0a. PREREQUISITE** — wire / confirm the live ingress write-path worker (inbox → stage_acquire → stage_sign → dispatch_post_sign → stage_send). **Gate: nothing else fires without it.**
- **0b.** `insert_created_tx` + deterministic `shift_id` + partial-unique index + `opened_by_cashier_id` sourcing.
- **1.** node_state mirror reuse (`mirror_node_state_shift_state_tx`) + unit tests.
- **2.** `stage_send` confirm edges 3/10 (thread doc_type into the SENT closure) + tests.
- **3.** `stage_acquire` online shift-create + edges 1/8 (fresh-Proceed path) + tests.
- **4.** `stage_offline_ack` offline: W10a reserve gate + W10b `DocType::ShiftOpen` accept + edges 2/7/9 + co-located create + tests.
- **5.** crash-recovery (orphan clears `current_shift_id`) + drain-classifier verify (`manual_recon:true`) + **Pattern-C end-to-end test** (offline open → SELL → return-online → drain → `Opened`; + drain-reject → `RequiresManualReconciliation`).
- **6.** *(optional)* live-smoke extension.

**Supersedes:** §1 Non-goals, §3 recommendation, §4 hook table (online-only), §5 component changes (online-only), §9 decomposition. §2 (investigation) + §6 (invariants) + §7 (edge cases) + §10 (open questions) remain valid and are extended by §0.3. **Pilot status:** WL-1 is a hard NO-GO blocker; A′ completion (0a..5) is the path to a functional shift lifecycle.

---

## 1. Goal & non-goals

**Goal.** On the ONLINE happy path: when a SHIFT_OPEN is confirmed by DPS, the gateway's local shift becomes usable (OPENED) so subsequent SELL/RETURN are admitted; when a Z_REPORT / SHIFT_CLOSE is confirmed, the shift closes.

**Non-goals (this worklet).** Offline shift-lifecycle retrofit (see §3 Option B / §10 Q5); the W4-Z3 live-smoke test branch; load-test harness.

**What is already correct and MUST NOT change.** The `stage_acquire` shift guards: `(Sell, Closed) → reject` is a fundamental fiscal invariant (no sale without an open shift); `(ShiftOpen, Closed) → allow`; `(Sell, Opened) → allow`. The gap is NOT the refusal — it is the missing **transition driver** that should flip the shift to OPENED after a confirmed SHIFT_OPEN.

---

## 2. Investigation findings (the production shift mechanism today)

Verified by crate-wide grep on worktree `/mnt/d/prro_gate_m4_w4_z3/rust` (branch off `rust-gateway`):

1. **`node_state.shift_state` is the de-facto production guard.** It is READ by `stage_acquire::check_shift_guard` and `stage_offline_ack`. It is WRITTEN in production only by `node_state::upsert_initial` (FN bootstrap; on-conflict updates `mode`+`shift_state`) and the boot-phase orphan-shift resolution (`SET shift_state='CLOSED'`).
2. **The `shifts` table + `current_shift_id` + the 9-state edge machine are test-only scaffolding.**
   - `shifts::insert_created` (`shifts.rs:119`) — the only fn that inserts a `shifts` row — has **zero production callers** (only the repo definition + tests).
   - The only `INSERT INTO shifts` / `INSERT INTO node_state(... current_shift_id ...)` statements are at `backlog_drain.rs:2953` / `:2937`, both **under `#[cfg(test)]`** (the file's test module starts at `:2753`).
   - `node_state.current_shift_id` is **never SET in production** (always NULL).
   - `shifts::transition_state` production callers are only the offline drain (`backlog_drain.rs:2169`, `:2498`) + the manual senior-cashier seam (`shifts.rs:840`). Both REQUIRE `ns.current_shift_id = Some(_)` (else `BootError::Internal "structural drift"` / the `mirror_node_state_shift_state_tx` CAS matches 0 rows). With `current_shift_id` NULL in production, these paths cannot actually fire — so even the OFFLINE shift-table lifecycle is effectively unexercised in production.
   - `stage_offline_ack::run` does NOT apply the spec's edge 2 (`Created → OpenedLocalPendingDrain`); it only validates `node_state.shift_state` and transitions the DOCUMENT (`Signed → OfflineLocalAck`). So spec §90.1 ("edge 2 fires at stage_offline_ack") diverges from the code.
3. **Net:** nothing in the production runtime (online OR offline) creates `shifts` rows, sets `current_shift_id`, or flips `node_state.shift_state` to `Opened` on a shift open. The shift lifecycle is unfinished M3b/M4 scaffolding; `node_state.shift_state` is the only live signal, and nothing advances it on the online happy path.

**Authoritative spec.** `docs/superpowers/specs/2026-05-17-m3b-shift-state-expansion.md` §4.1 defines the online edges (intent-marker at ingress, confirm at DPS-Ack):

| Edge | Transition | Trigger (spec §4.1) |
|------|------------|----------------------|
| 1 | `Created → Opening` | online `SHIFT_OPEN` **ingress** (M3a Pattern B intent-marker) |
| 3 | `Opening → Opened` | online send → **DPS Ack** on any attempt |
| 8 | `Opened → Closing` | online `Z_REPORT` / `SHIFT_CLOSE` **ingress** (intent-marker) |
| 10 | `Closing → Closed` | online close → **DPS Ack** |

**Critical clarification on "DPS Ack" (edge 3/10 trigger).** "DPS Ack" = `sendChkV2` returned status OK + a fiscal id = the gateway's **`Sent` DocState** (server_fiscal_no set). It is NOT the gateway's internal terminal `Ack` DocState, which is reached later via reconcile passes (`Sent → Kvt1` last_chk probe → … → `Kvt2 → Ack`). Therefore the confirm-edges hook at **`stage_send` (the SENT commit)**, NOT `stage_finalize`. (W4-Z3 live cycle confirmed: docs reach `Sent` with server_fiscal_no the moment DPS accepts — that is the correct open/close confirm point. Hooking at `stage_finalize` would block SELLs for the whole reconcile window — a usability + correctness flaw.) This mirrors the offline design where the shift edge co-locates with `stage_offline_ack` (local commit), not with drain-Ack.

---

## 3. Central design decision (for external review + operator)

Because the shift-table machinery is scaffolding (§2), wiring "open the shift" requires choosing the production source of truth:

### Option A — `node_state.shift_state`-centric (RECOMMENDED)
- Treat `node_state.shift_state` (+ `current_shift_id`) as the production source of truth — it is what the guards already read.
- Wire the online confirm transitions to set `node_state.shift_state` (`Opening`→`Opened`→`Closing`→`Closed`) + `current_shift_id`.
- ALSO populate a `shifts` table row per shift (via the existing `insert_created` + `transition_state`) co-located in the same envelopes, so the table stops being dead code and stays consistent with `node_state` for the online path — but `node_state` remains authoritative for guards.
- Does NOT retrofit the offline path (its shift-table writes stay scaffolding until a separate worklet).
- **Pros:** smallest diff; unblocks the online cycle + load tests; wires the existing seam; keeps the spec's table model alive for online. **Cons:** online vs offline shift-table maintenance stays asymmetric until the offline retrofit lands (tracked debt).

### Option B — full shifts-table lifecycle (spec-faithful, larger)
- Make the `shifts` table authoritative and `node_state.shift_state` a strict mirror, for BOTH online + offline; retrofit the offline path to actually create rows + set `current_shift_id` (currently `#[cfg(test)]`-only).
- **Pros:** matches the M3b spec model end-to-end; fixes the offline scaffolding. **Cons:** much larger; touches the offline drain hot-path; higher regression risk; out of proportion to the operator's immediate ask.

**Recommendation: Option A**, populating the `shifts` table for the online path (so table + node_state are kept consistent online), and tracking the offline-path retrofit as a separate worklet (§10 Q5). Rationale: minimal diff, unblocks the goal, preserves the spec's table model where it is now exercised, avoids a risky offline-hot-path retrofit the operator did not ask for.

---

## 4. Design (recommended Option A)

> ⚠️ **SUPERSEDED by §0.3 (A′).** This §4 hook table is the ONLINE-ONLY Option A and is INSUFFICIENT for the pilot (offline Pattern C stays non-functional). Implement from the §0.3 corrected A′ hook table (both channels, `insert_created_tx`, offline edges 2/7/9 co-located in `stage_offline_ack`, the live-ingress-driver prerequisite). Kept below for the online-subset reference only.

Hooks, keyed on `doc_type`, channel-aware (online only):

| Edge | Transition | Hook | Writes (all inside the stage's existing `with_immediate`) |
|------|------------|------|------------------------------------------------------------|
| 1 | `Created → Opening` | `stage_acquire` (online SHIFT_OPEN accept) | `insert_created` + `transition_state(Created→Opening)`; bind `fiscal_documents.shift_id`; `node_state` set `shift_state=Opening` + `current_shift_id` |
| 3 | `Opening → Opened` | `stage_send` SENT-commit (4-b), `doc_type=ShiftOpen` | `transition_state(Opening→Opened)`; `node_state.shift_state=Opened` |
| 8 | `Opened → Closing` | `stage_acquire` (online Z/SHIFT_CLOSE accept) | `transition_state(Opened→Closing)`; bind Z doc's `shift_id=current_shift_id`; `node_state.shift_state=Closing` |
| 10 | `Closing → Closed` | `stage_send` SENT-commit (4-b), `doc_type=ZReport\|ShiftClose` | `transition_state(Closing→Closed)`; `node_state.shift_state=Closed` + clear `current_shift_id` |

- SELL / RETURN / SERVICE_*: **shift-neutral** — no edge.
- `stage_send` already carries `doc_type` + `shift_id` in its send-inputs (`fiscal_documents.rs` ~1013-1047), so edge 3/10 need no new reads.

---

## 5. Per-component changes

> ⚠️ **SUPERSEDED by §0.3 (A′).** Online-only + incomplete: `insert_created` is pool-bound (need `insert_created_tx`); the proposed `set_shift_*` setters omit `current_shift_id` (reuse `mirror_node_state_shift_state_tx`); the offline `stage_offline_ack` edges 2/7/9 + W10a/W10b are not listed here. Use §0.3.

- **`node_state` repo (new tx setters, CAS-guarded on `(fiscal_number, expected shift_state)`):**
  - `set_shift_opening_tx(tx, fn, shift_id)` → `shift_state=Opening, current_shift_id=shift_id` WHERE `shift_state='CLOSED'`.
  - `set_shift_opened_tx(tx, fn)` → `shift_state=Opened` WHERE `shift_state='OPENING'`.
  - `set_shift_closing_tx(tx, fn)` → `shift_state=Closing` WHERE `shift_state='OPENED'`.
  - `set_shift_closed_tx(tx, fn)` → `shift_state=Closed, current_shift_id=NULL` WHERE `shift_state='CLOSING'`. Return `bool`/typed outcome for idempotence handling.
- **`shifts` repo:** reuse existing `insert_created` + `transition_state` (no new fns).
- **`stage_acquire`:** in the accept envelope for SHIFT_OPEN / Z|SHIFT_CLOSE, add the shift create/transition + `shift_id` bind. (Exact envelope structure to be confirmed at implementation; the doc INSERT already runs in a `with_immediate` there.)
- **`stage_send`:** in the SENT-commit envelope (4-b, after the wire call returns OK), add the doc_type-keyed confirm transition.

---

## 6. Invariant preservation

- **#1 (no network/crypto in write tx):** all shift writes are DB-only inside the stage's existing `with_immediate`; the wire call (`stage_send` 4-a) already returned before 4-b. ✓
- **#2 (one fiscal_number = single-writer):** `stage_acquire` + `stage_send` run under the FN's single-writer lease. ✓
- **#8 (recovery must not violate transitions):** every transition is CAS-guarded (`transition_state` / the new node_state setters' WHERE) and idempotent — a re-driven SENT doc finds the shift already `Opened` → CAS `Conflict` → treated as no-op, not an error. ✓
- **INV-03 (shift opened before fiscal operations):** preserved and strengthened — between SHIFT_OPEN ingress and its SENT-confirm the shift is `Opening`, and SELLs are correctly refused until the open is DPS-confirmed. ✓

---

## 7. Edge cases

- **Idempotence on reconcile re-drive.** `stage_send` re-processing a SENT doc (e.g. SENT→KVT1 probe path also touches the doc) must not fail: CAS `Opening→Opened` returning `Conflict`/`Forbidden` when already `Opened` → no-op + Info audit.
- **SENT-then-mismatch.** A SHIFT_OPEN reaches SENT (shift opened) but a later reconcile finds an id-mismatch → doc → `RequiresManualReconciliation`. The shift is then open under a manual-recon doc. Same profile as offline (open at local-ack, drain may reject). **Open question (§10 Q4):** does the shift follow doc → manual (edge 4 analog) or stay Opened pending operator recon?
- **`next_lnd` on open.** SHIFT_OPEN sends `local_number=0` (forced); the first SELL uses lnd. **Open question (§10 Q3):** is the local number per-shift-reset or per-RRO-continuous? If per-RRO-continuous (current code: `allocate_next_lnd` never resets), the open transition does NOT touch `next_lnd`.
- **Channel-awareness.** Edges 1/8 fire only when `node_state.mode == Online`. Offline SHIFT_OPEN flows the `stage_offline_ack` path (Option A leaves that as-is).

---

## 8. Test plan

**New (mock DPS):**
- `stage_acquire` online SHIFT_OPEN → creates `shifts` row `Created→Opening` + `node_state.shift_state=Opening` + `current_shift_id` set + doc `shift_id` bound.
- `stage_send` SENT (doc_type=ShiftOpen) → `Opening→Opened` + `node_state.shift_state=Opened`.
- Z path: ingress `Opened→Closing`; SENT `Closing→Closed` + `current_shift_id` cleared.
- **End-to-end admission:** after a confirmed SHIFT_OPEN, a SELL is now ADMITTED by `stage_acquire` (was refused before) → drives to SENT.
- Idempotent re-drive (second SENT pass → shift unchanged, no error).
- SELL / RETURN reaching SENT/Ack are shift-neutral.

**Regression:**
- `tests/write_path_deterministic_replay.rs::fixture_8_kvt2_crash_no_dps_query` (a SELL → Ack) stays green (SELL is shift-neutral; the confirm moved to `stage_send`, finalize untouched).
- Offline drain tests unaffected (Option A does not touch the offline path).
- Existing `stage_acquire` / `stage_send` suites — assess for shift-row assumptions.

**Optional live (W4-Z3 extension):** drive SHIFT_OPEN through REAL ingress (not seed-PREPARED), assert `node_state.shift_state=Opened` post-confirm, then a SELL admitted.

---

## 9. Piece decomposition (small, reviewable)

0. **Investigation confirm** (resolve §10 Q1/Q5/Q6) — verify the scaffolding finding against `main`/M3b closure; lock Option A vs B with the operator. *(gate before coding)*
1. `node_state` tx setters (open/close) + unit tests.
2. `stage_send` confirm transitions (edges 3/10) + tests.
3. `stage_acquire` intent transitions + shift creation + bind (edges 1/8) + tests.
4. End-to-end admission + idempotence + SELL-neutral regression (mock DPS).
5. *(optional)* live-smoke extension.

Review rounds per the hot-zone multi-round pattern (3–5 rounds; INV-03 frozen-adjacent).

---

## 10. Open questions (external review + operator)

- **Q1 (architecture):** Option A (node_state-centric, recommended) vs Option B (full shifts-table retrofit incl. offline)?
- **Q2:** Under Option A, populate the `shifts` table (forensics + keeps table alive) or go `node_state`-only (even smaller)?
- **Q3 (fiscal, operator):** local document number — per-shift-reset or per-RRO-continuous? (determines whether open touches `next_lnd`.)
- **Q4:** SENT-then-mismatch — shift disposition (follow doc→manual, or stay Opened pending recon)?
- **Q5:** the offline shift-table scaffolding (insert_created / current_shift_id never wired in prod) — fix in this worklet (Option B) or a separate offline worklet?
- **Q6 (foundation):** confirm the "scaffolding" finding is not a branch artifact — is the shift-table lifecycle wired on `main` or by a runtime/ingress shell not present on this `rust-gateway`-derived worktree? (If it IS wired elsewhere, §2/§3 shift.)

---

## 11. Risks

- Hot-zone: both hooks (`stage_acquire`, `stage_send`) are on the fiscal write-path.
- Option A leaves online/offline shift-table maintenance asymmetric (tracked debt).
- `stage_acquire` now creates a `shifts` row on SHIFT_OPEN — existing tests assuming no shift row may need updates.
- If Q6 reveals the machinery is wired elsewhere, the plan's foundation changes materially.
- The shift state machine is fiscal/legal-adjacent (INV-03); a wrong edge (e.g. opening before DPS-confirm) would let SELLs reach DPS before the shift is open → DPS `-15 NOT_OPEN_SHIFT`. The SENT-confirm hook (§2 clarification) is the guard against this.
