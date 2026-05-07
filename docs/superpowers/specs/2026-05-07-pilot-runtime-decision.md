# Pilot Runtime Decision — Rust-only pilot, Python eradication

**Status:** ACCEPTED. This ADR is the canonical source of truth for pilot
runtime architecture and supersedes any contradicting language in
`docs/M3-W0-handoff.md §3` until that handoff is rewritten to match.

**Scope:** runtime architecture of the live PRRO Gateway pilot — what binaries
run, what languages they are written in, what the path from M3a to live pilot
looks like.

**Out of scope:** internals of M3a (already locked by ADR-M3-A1..A9 and the
three M3-W0 specs); fiscal protocol semantics; per-stage write-path design.

**Format:** one primary decision (Decision / Alternatives rejected / Tests
required / Open risk), followed by a section of provisional defaults that may
be revisited without amending the primary decision, followed by an explicit
list of open items that must be closed before M4/M5 sizing.

---

## Context

Three facts forced this decision and are documented here so the reasoning does
not get lost over the multi-month execution window:

1. **No production deployment exists.** The Python service in `src/prro_gateway/`
   and the Rust `prro_sidecar` are both engineering artifacts, not battle-tested
   prod stacks. There is no live system to "protect" or to run a pilot
   alongside.

2. **Pilot acceptance criteria require offline lifecycle and forbid passthrough
   signing.** The pilot acceptance test plan (drafted in
   `docs/PILOT_ACCEPTANCE_TEST_PLAN.md`; expected to commit alongside or before
   this ADR) defines a dedicated mandatory **Phase 6 — Offline With One Fiscal
   Number**: enter offline mode, issue receipts under `OFFLINE_LOCAL_ACK` (not
   final `ACK`), block Z-report while offline backlog is pending, return online,
   run synchronization, finalize. The same plan's **Final Go Criteria** lists
   "Offline lifecycle passes" as binding, and its **No-Go Conditions** forbid
   "Production-like configuration using stub transport or passthrough signing".

   The ADR carries this evidence inline so its argument does not depend on the
   acceptance plan being indexable at the moment of reading; the cross-reference
   in §Cross-references is informational.

   Together these rule out a Python-only pilot, a Python-signs-itself pilot, and
   a Rust-online-only pilot.

3. **Strategic direction: Python eradication.** Project owner has stated the
   final stack contains zero Python. This is a destination commitment, not just
   a long-term aspiration. There is therefore no value in investing pilot
   validation effort in Python code paths that are scheduled for removal.

Together, these mean: the cheapest path to a validated pilot is **not** the
cheapest path to a long-lived runtime. The pilot must run on the architecture
that lives in production after the pilot.

---

## Decision

### Decision

**The pilot runs on a single Rust binary (`prro` crate) with all functional
subsystems implemented in Rust. Python is removed from the pilot path entirely
— no Python ingress, no Python services, no Python signing, no Python admin.
The `prro_sidecar` HTTP boundary is not part of the pilot runtime; the gateway
embeds `prro_crypto` directly via `rlib`.**

The path to the pilot is the four-milestone sequence:

```
M3a — ONLINE write-path Rust            (6-8 weeks)   ← anchored, no scope change
M3b — Phase-6-min offline subsystem     (4-6 weeks)
M4  — Rust ingress + maria304 re-bridge (4-6 weeks)
M5  — services tail (ingress writer,
       post-boot reconciler, ops
       integration hooks, plus open
       items §3 below; backup/restore
       is a parallel docs+ops track,
       see D3, not part of M5)         (4-10 weeks)
M6  — admin surface (CLI; web TBD)      (≤ 2 weeks on CLI; web post-pilot)
```

End-to-end estimate for solo execution at M3a discipline (W-tasks bundled with
§9 fixtures, ADR-anchored): **5-8 calendar months** to live pilot. This is the
expected timeline; aggressive shortening compromises the test gate that this
project has already paid to install.

### Alternatives rejected

- **Hybrid pilot (Python ingress + Rust sidecar).** Validated against Phase 6
  acceptance, but invests engineering attention in Python offline_sync.py
  (606 lines, integration in `services/write_path.py:387-412`), which is on
  the eradication path. Any bug found in Python is bug-fixed in code that will
  be deleted; lessons do not transfer cleanly to the Rust port. Acceptable as
  an internal smoke contour for early DPS contact (see Tests required), not as
  a pilot runtime.

- **M3a expanded to include full M3b in scope.** Restores the monolithic M3
  plan that `M3-W0-handoff §3` explicitly split apart for budget reasons.
  Does not change the destination, only the chunking; rejected because the
  W-task gating discipline of M3a benefits from a tight ONLINE-only scope.

- **Pilot deferred until full Python eradication (M3a → M5 → all post-pilot
  services rewritten).** Maximum cleanliness, zero pilot signal for an extra
  3-4 months. Rejected because a pilot is the cheapest way to surface DPS
  reality mismatches; deferring it past M5 means architectural decisions made
  in M3a-M5 stay unverified for an extra quarter.

- **Python kept as a "thin shim" for ingress only (option C from the
  decision-frame discussion).** Rejected explicitly per project-owner
  direction: no Python on the pilot path. Recorded here so future readers do
  not re-litigate the choice.

### Tests required

For the **decision itself** to be considered honoured, the following gates
exist independently of M3a's own §9 fixture set:

1. **Gateway does not invoke Python.** Audit on the pilot deployment manifest
   and supervisor unit files: no project virtualenv, no Python service unit
   under the gateway's supervisor scope, no Python runtime listed as a runtime
   dependency of the gateway installer. The presence of an OS-level Python
   interpreter that the gateway does not call is **not** a failure (it is
   normal on Linux hosts and irrelevant to this gate).
2. **`prro_sidecar` not running on pilot host.** Process list audit and
   supervisor configuration audit: no `prro_sidecar` binary launched by the
   gateway's service unit and no sidecar URL configured in the gateway runtime
   config.
3. **Phase 6 (offline lifecycle, as inlined in §Context point 2) passes
   against Rust-only stack.** This is the binding gate; it cannot be relaxed.
   The detailed test scenario is owned by the pilot acceptance plan once it
   commits; until then, the inlined definition in this ADR is the authoritative
   description.
4. **Internal smoke milestone: M3a-end ONLINE-against-test-DPS.** Before M3b
   work begins, the M3a binary should be exercised against any non-production
   DPS contour available (sandbox, test, partner-test) to surface
   wire-format / TLS / cert / proto-drift issues that local mock DPS does not
   reproduce. This is **internal**, not part of pilot acceptance.

   **Mandatory if a non-production DPS contour is available** (sandbox or
   partner-test access at the time M3a closes). **Otherwise** the gate is
   discharged by an explicit owner waiver recorded in this repo (a
   `docs/superpowers/specs/<date>-m3a-end-smoke-waiver.md` stub with the date,
   the reason no contour was available, and the compensating risk acceptance)
   committed before M3b work begins. This avoids the gate becoming a
   non-technical blocker when external access is unavailable.

### Open risk

- **Solo execution duration.** 5-8 months is long; team sickness, scope
  surprises (especially in M5), or DPS-side regulatory change can stretch this
  to 9-10 months. Mitigations: (a) the M3a-end smoke gate above as early signal,
  (b) M3b/M4/M5 must each have their own W0-style research phase before plan
  writing, (c) decision-defer items (§3 below) reduce M4/M5 surface where
  acceptable.

- **Maria304 re-bridge timing.** `maria304_driver` currently bridges to Python
  REST via `reqwest::blocking` in `spawn_blocking`. Re-bridging to in-process
  Rust gateway is M4 work, but the existing Python REST endpoint disappears
  before M4 is done if we remove Python services in any other order. Mitigation:
  M4 ingress must come before M5 services purge, not after.

- **Regulatory deadline pressure.** If a regulator-imposed live-pilot date
  arrives before M3a-M5 is done, the only honest options are (a) ship the
  Rust-only stack with documented gaps and seek explicit operator sign-off
  on missing offline-limit enforcement etc., or (b) miss the date. Hybrid is
  not on the table per this ADR.

---

## Default assumptions (provisional)

These are baseline choices made now to unblock M-series sizing; they may be
revisited without amending the primary decision above. Each is recorded with
its rationale so a future reader can re-evaluate against then-current facts.

### D1: `prro_sidecar` is archived-but-kept

The `prro_sidecar` crate stays in the workspace for the duration of M3a-M5 and
is not actively developed. A `README.md` is added to the crate marking it
"archived; superseded by in-process `prro::crypto` wrapper; retained for M2
test-fixture continuity until pilot." It is **not** deployed on the pilot host
and is **not** wired into any pilot installer.

Rationale: removing it now risks breaking M2 test infrastructure (which uses
sidecar output as oracle). Removal is mechanical and cheap once pilot is live;
defer to a post-pilot cleanup PR.

### D2: Pilot admin surface is CLI-only

A `prro_admin` CLI binary (subcommands at minimum: `status`, `set-config`,
`cert show / rotate`, `node-state show / set`, `manual-reconcile <doc-id>`)
is delivered as part of M6 with a budget of ≤ 2 weeks. Web admin UI
(`admin_ui/routes.py` + Jinja templates) is **not** ported for the pilot.
Operators on the pilot interact with the gateway via CLI, logs, and metrics.

Rationale: web UI is 2-4 weeks of UI rework with no fiscal-correctness
benefit. CLI gives operators sufficient control for pilot scale (one or two
fiscal numbers, one operator). Web admin can be added post-pilot once real
operator workflow needs are observed.

### D3: Backup/restore procedure is a hard pilot prerequisite

`OPERATIONS.md` runbooks for backup/restore (SQLite WAL `.backup` API, not
file copy), key/CA bundle rotation, and rollback rehearsal must be written and
**rehearsed** before live pilot. This is **not** in M3b (which is offline
subsystem); it is a parallel docs+ops track that runs alongside M3b/M4/M5 and
gates pilot Go criterion "Rollback to WebCheck is documented and rehearsed".

Rationale: the pilot acceptance criteria (see §Context point 2) explicitly require
this; treating it as M5 tail risk is wrong because it is procedural, not
implementation, and operator rehearsal cycles take real wall-clock time.

---

## Open items — must close before M4/M5 sizing

These cannot be defaulted today; each has a real impact on M4 or M5 scope and
must be resolved before those plans are written. They are listed here to
prevent accidental defaulting.

### O1: 1С OLE bridge scope on the pilot

`docs/OLE_METHODS_USED_BY_1C.md` exists but is not committed. The size of M4
ingress depends on whether the pilot operator profile uses 1С (and which
methods), uses Maria304 only, uses REST only, or some combination. If 1С is on
the pilot path, M4 grows by an OLE-bridge subsystem (likely 2-3 weeks).

Decision needed: pilot operator profile inventory + 1С method whitelist.

### O2: Onboarding key/identity automation

`services/onboarding_key_identity.py` exists in Python. Question: is automated
DSTU key/cert provisioning a pilot requirement, or is hand-provisioning
acceptable for one or two fiscal numbers? If automated: M5 grows by an
onboarding subsystem. If manual: M5 saves ~1-2 weeks but adds an operational
runbook line.

Decision needed: pilot-scale key/cert provisioning model.

### O3: `retention` and `shift_aggregation` depth

Python has `services/retention.py` and `services/shift_aggregation.py`. For a
pilot of 1-2 fiscal numbers and a few weeks duration, neither is operationally
necessary. For a pilot extending past one calendar month, retention becomes
relevant; for any pilot reading shift summaries, `shift_aggregation` is needed.

Decision needed: pilot duration estimate + reporting requirements.

---

## Cross-references

- `docs/PILOT_ACCEPTANCE_TEST_PLAN.md` — Phase 6 (offline), Final Go Criteria,
  No-Go Conditions. **Informational pointer only**; the load-bearing quotes
  from that plan are inlined in §Context point 2 of this ADR so the argument
  is self-contained even if that file is not yet committed at read time.
- `docs/M3-W0-handoff.md` — §3 deferrals (will be rewritten to remove
  Python-fallback assumption).
- `docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md` — ADR-M3-A1..A9 (M3a
  contract; this ADR does not amend M3a internals).
- `docs/superpowers/specs/2026-05-06-m3-w0-1-state-sequence.md`,
  `…-w0-2-lock-discipline.md`, `…-w0-3-retry-recovery.md` — M3a contracts.
- `docs/superpowers/plans/2026-05-07-m3a-implementation.md` — M3a plan
  (unchanged by this ADR).
- bd:
  - `PRRO_GATE-9qd` (M3 epic), `9qd.1` (M3a), `9qd.2` (M3b — to be rescoped to
    "Phase-6-min offline" per this ADR).
  - `PRRO_GATE-gx2` (pilot offline decision — closed with reference to this
    ADR after this ADR commits).
  - `PRRO_GATE-er6` (Sprint 2 step 1: OfflineSyncService selector — superseded
    by Rust-only pilot decision; spec source remains as historical reference,
    no implementation under this scope).

---

## Propagation checklist (post-approval)

Strict order; each step gates the next. Source of truth lives in this ADR
until each downstream artifact is updated to match.

1. ADR commit on branch (this file). **← gate**
2. `docs/M3-W0-handoff.md §3` rewrite: replace "Python fallback for offline"
   language with "M3b → M4 → M5 sequence under Rust-only pilot decision" and
   cite this ADR.
3. bd updates (in this order):
   - `bd close PRRO_GATE-gx2` with note "Rust-only pilot decision recorded;
     offline required and moved to Rust Phase-6-min M3b scope. See
     `docs/superpowers/specs/2026-05-07-pilot-runtime-decision.md`."
   - `bd close PRRO_GATE-er6` with note "Superseded by Rust-only pilot
     decision; Python OfflineSyncService is not on the pilot path. Spec
     content retained as historical reference; canonical scope now under
     M3b epic `PRRO_GATE-9qd.2`."
   - `bd update PRRO_GATE-9qd.2` description: rescope to "Phase-6-min offline
     subsystem in Rust" per this ADR.
   - `bd create` epics for M4 (ingress) and M5 (services tail) and M6 (admin)
     as children of `PRRO_GATE-9qd` (or of a new `PRRO_GATE-rust-pilot` parent
     if hierarchy demands).
4. After all of (3): bd export / sync to ensure persistence.

No bd action precedes ADR commit. No M3-W0-handoff edit precedes ADR commit.
