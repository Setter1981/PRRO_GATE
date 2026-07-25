# CS-3 D/E — implementer package (vertical slices, RED-first)

**Status:** IMPLEMENTATION TASK PACKAGE. The oracle + design-of-record are on `origin/main`
(`#330` write-back, `#331` correction `806b661`). **Design-of-record: `docs/CS3_REMEDIATION_DESIGN.md`
(rev3.1, DESIGN GO).** This package decomposes the design's §8 order into PR-sized slices, each mapping
the design's §7 teeth as **RED-first** pins. Language: **Rust** (`rust/prro`, `rust/prro-domain`).

**Hard constraints (design §8 + keystone):**
- **D and E ship in ONE production release.** Slices may land as separate PRs but each is INACTIVE / behind
  the sole-caller gate until the whole-fence cutover (Slice 7) activates them. A partial-E build without the
  cutover has a real double-issue window — do NOT deploy a subset.
- **INV-1:** no network/crypto inside any `BEGIN IMMEDIATE`. The wire (4-a) sits strictly between committed
  boundaries; the record tx and both apply commits are DB-only.
- **RED-first (design §7):** for every guard, author the test that breaks it, watch it **RED on guard
  removal**, then implement to green. A guard with no revert-canary does not count as done.
- Discipline: minimal-diff, short transactions, `git -C <worktree>` hygiene, migration-keeper for schema,
  security-reviewer before merge. Ground every anchor live (the design cites file:line).

---

## Slice map (§8 order → PR-sized slices)

### Slice 1 — migration 035 (schema only, INACTIVE) · LOW risk · deps: —
**Deliverable (design §5, §4.2):** `035_delivery_reservation_call_once_and_evidence.sql`, in order:
(1) fail-fast empty-table guard (`delivery_reservation` row count 0, mirrors 033); (2) four additive
evidence columns `evidence_kind/evidence_text/evidence_code/evidence_digest`; (3) `ux_delivery_document_ever_started`
partial unique index; (4) rebuild `ux_reservation_active` with the §3.1 predicate
(`state IN (RN,CS) OR (OO AND apply_state='PENDING_APPLY')`); (5) rebuild `delivery_reservation_no_replace`
with the **byte-identical FN predicate + the historical-document-started rejection**; (6) fail-closed evidence
matrix triggers (INSERT+UPDATE, NULL-bypass closed via `COALESCE(CASE … ELSE 0 END) <> 1`); (7) evidence
immutability trigger (freeze after `OUTCOME_OBSERVED`). **No `seed_advanced`** (proven dead).
**RED-first teeth (§7):** *SQL matrix tightness* (per leaf: delete one required payload / add one forbidden /
swap routing-effect → each rejected by SQLite; incl. NULL-bypass); *fence consistency* (index / no-replace /
`get_active_for_fn` / auth query share the §3.1 predicate byte-identical — structural extract-and-compare, not
comment-parsed); *no-orphan-RN* (DDL half); *INACTIVE-safety* (empty 034 DB migrates; non-empty aborts with
**no partial objects** — run with `foreign_keys=ON`).

### Slice 2 — durable evidence field + record/hydration · MED · deps: 1
**Deliverable (design §4):** payload-carrying `EvidenceDiscriminant` on `ObservedOutcomeV1`
(`Accepted{fiscal_number}`, `Rejected{verdict,digest}`, …); `record` receives the sealed classified outcome +
discriminant and rejects a disagreeing leaf; boot **hydration** re-reads the four slots, validates 32-byte
digests / non-empty `F`, rechecks the leaf→axes/generation matrix in Rust, fails loud on unknown tag / illegal
payload. Uses the existing algebra (narrow constructors on opaque types; no parallel public evidence algebra).
**RED teeth:** *durable Accepted* (record `Accepted(F)`, cold-reopen, hydrate → `F` survives; remove
`evidence_text` → RED); *full evidence round-trip* (all 11 leaves record→cold-reopen→hydrate→expected
axes/effect, via **production** `classify`/`record`, not a copied classifier).

### Slice 3 — lifetime authorization + sole-caller wire gate · HIGH (hot-path, INV-1) · deps: 1
**Deliverable (design §2, §3.3):** `authorize_submission` adds the `NOT EXISTS(… call_started_at IS NOT NULL)`
clause inside the RN→CALL_STARTED `BEGIN IMMEDIATE`; token minted only after commit; a source-level
**sole-caller gate** — the only production path to `send_chk`/`submit_raw` is `submit_authorized`.
**RED teeth:** *lifetime call-once* (two attempts race → exactly one CALL_STARTED, mock DPS ≤1 RPC; drop the
index OR the query clause → each fails a separate test); *crash-after-marker* (commit CALL_STARTED, drop the
future, reopen → no `submit_raw` for that document); *no-orphan-RN after started history* (attempt-2 INSERT
fails, leaves no row, an unrelated next doc still reserves).

### Slice 4 — repeatable apply + PENDING boot resume · HIGH · deps: 1,2,3
**Deliverable (design §4.3):** two-commit record-then-apply; the apply CAS is full-tuple/generation-guarded,
idempotent, **clears the active pointer atomically with `PENDING → APPLIED`** (rev3.1); boot re-applies PENDING
with **no wire**; a durable CALL_STARTED with no outcome → once → `NoResponse{CrashedBeforeObservation}` +
PENDING + STOP_MODE (local recovery write).
**RED teeth:** *apply replay* (crash after record / after each apply write → one ledger result, 0 RPC, APPLIED;
stale generation drops, ledger/seed/fence unchanged); *clean-accept atomic release* (fail before
seed/SFN/APPLIED commit → row stays PENDING, next doc refused; success exposes all effects + releases).

### Slice 5 — STOP / operator completion (strengthened `reset_stop_mode`) · HIGH · deps: 1–4
**Deliverable (design §3.4, rev3.1 §C5/§C… + §3):** unresolved outcomes (SubmittedUnknown / -12 / -6) →
PENDING + STOP_MODE in the record tx; `reset_stop_mode` strengthened — a plain reset **fails closed** while a
CS-3 PENDING row exists; release is gated on a **verified read-only `status_rro`** (outside tx; `online=true`,
`snapshot.open_shift` agrees) then completes PENDING→APPLIED, clears the pointer, selects **ONLINE** (no
session) or **GOING_ONLINE** (active drain), with the full origin×document-family shift/seed **rollback matrix**
(`Opening→Closed` / `OLPD→Closed` / `CLPD→OLPD`, offline-successor cancellation); guarded `BLOCKED→GOING_ONLINE`
after a recorded `-11` cause-clear; **offline-origin reject/`Offline168` HOLDS the fence** until chain repair.
Add the `(Sending → RMR)` whitelist edge (operator-completion-only). No new entity/token/state.
**RED teeth:** *SubmittedUnknown liveness* (plain reset fails; failed `status_rro` changes no fiscal row;
verified probe + completion → APPLIED + pointer clear + next doc authorizes, original never rewires);
*-12/-6 liveness*; *-11 liveness* (online releases-but-BLOCKED / offline stays PENDING+BLOCKED); *operator
matrix* (all origins × accepted/not-accepted × regular/shift-open/close-Z; offline Accepted 0 seed writes;
`Created` rollback stays refused; offline reject with a live successor refuses unless successors cancelled).

### Slice 6 — atomic `Sent + NotFound` · MED · deps: 1–5
**Deliverable (design §3.5):** both live producers (boot/convergence `boot_phase.rs`; drain
`kvt2_confirm.rs`) call one tx-bound helper inside one `with_immediate`: doc `Sent→RMR` + `node→STOP_MODE`
+ trace-complete + audit — **all four or none**; NOT via `shifts::force_to_manual_reconciliation_with_audit`.
**RED teeth:** *Sent+NotFound atomicity* (boot + offline variants → doc-RMR+STOP+trace/audit; inject failure
after each write → all-old-or-all-new; success then drain/ingress → 0 RPC until reset).

### Slice 7 — whole-fence cutover + retire blind-resend (E, the activation) · HIGH · deps: 1–6
**Deliverable (design §3.3, §7):** gate every `stage_send::run` caller + the `(ErrorRetryable,Sending)` edge +
the 4 seed-writers + offline issuance/session/code on the §3.1 fence via `authorize_submission`; retire the
issued-doc redrive; **legacy cutover** — a reservation-less in-flight `Sending`/`ErrorRetryable` doc fails
closed → RMR/HOLD (pre-deploy empty-in-flight gate preferred). **This is the release moment (D+E together).**
**RED teeth:** *NS-1 wire-count ≤ 1 per `document_id` over full history* (arbitrary boot/drain/convergence/crash
schedule → ≤1 real `submit_raw` per doc); *legacy cutover* (reservation-less in-flight fails closed before any
wire).

---

## Start here
**Slice 1 (migration 035)** — schema-only, INACTIVE, lowest risk, unblocks everything. Author the SQL-matrix
+ fence-consistency + INACTIVE-safety teeth **RED first** (write a mutation test, watch SQLite reject it only
after the trigger exists), then the DDL. Branch/worktree off `origin/main` (now `806b661`), migration-keeper +
security-reviewer before merge, docs-then-code discipline already satisfied (specs corrected in #331).

**Do NOT deploy any slice's activation until Slice 7 lands — D+E are one release.**
