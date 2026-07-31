# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

---

# Multi-Protocol PRRO Gateway — project operating instructions

## Project intent

This repository is building a **local PRRO gateway core** for Ukraine with:
- canonical model
- staged write-path
- multiple ingress protocols
- multiple backend/transport profiles
- offline behavior
- local archive
- recovery and reconciliation

Do not treat this as a greenfield toy service. It is an edge fiscal system with operational and legal risk.

---

## Development commands

All commands run from `rust/`. **The Python tree under `src/prro_gateway/` is the retired
pre-Rust scaffold** — it is not built, not tested by CI, and not deployed. Do not change it and do
not treat it as documentation of current behaviour.

```bash
# Run the gateway (boots, migrates, serves until SIGINT/SIGTERM)
cargo run -p prro -- serve --config <path/to/config.yaml>

# Apply migrations and exit
cargo run -p prro -- migrate --config <path>

# Preflight diagnostics (config, DB, lock, listen); --live adds READ-ONLY DPS probes
cargo run -p prro -- doctor --config <path> [--live --fn <fiscal_number>]

# Operator-only intervention paths
cargo run -p prro -- admin <subcommand>

# Tests — the merge gate (same command CI runs)
cargo nextest run -p prro --features test-support --locked

# Single binary / by pattern
cargo nextest run -p prro --features test-support --locked -E 'binary(invariant_scan)'
cargo nextest run -p prro --features test-support --locked -E 'test(concurrency)'

# Static gates (required contexts)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features test-support -- -D warnings
```

The invariant fuzzer runs inside the required `cargo nextest run -p prro` step; `FUZZ_CASES`
scales the capstones (PR default 256, nightly 4096). Tests use per-case temp-file SQLite with the
real migration set applied.

---

## Architecture overview

**Workspace root:** `rust/` — the gateway crate is `rust/prro/`; the pure domain vocabulary
(`DocState`, `ShiftState`, `DocType`, ids, the sealed delivery algebra) lives in `rust/prro-domain/`.
Sibling crates: `prro_crypto` / `prro_crypto_v2` (DSTU signing), `prro_sidecar`, `maria304_driver`,
`prro_escpos*`, and the contract crates (`prro-ingress-contract`, `prro-dps-contract`,
`prro-fleet-contract`, `prro-testkit`).

**Request flow (all paths under `rust/prro/src/`):**
```
runtime/ingress/server.rs        ← axum router: POST /v1/ingress/:source, GET /v1/status/:fn
  → runtime/ingress/{preflight,convert,handler}.rs  ← validate + normalize to the canonical command
  → db/repositories/ingress_inbox.rs                ← idempotent inbox row (NEW/PROCESSING/DONE/…)
  → services/write_path/inline.rs                   ← drives the stages, single-writer per fiscal_number
      stage_acquire.rs   ← guards (node mode, shift, cash, offline limits) + lnd allocation
      stage_sign.rs      ← canonical XML + DSTU signature (offline code stamped in-band, B9)
      stage_offline_ack.rs ← offline durable commit → OFFLINE_LOCAL_ACK
      stage_send.rs      ← wire send + advance-at-SEND (Sending→Sent CAS)
      stage_finalize.rs  ← KVT1/KVT2 → ACK
  → transports/dps/                                 ← gRPC channel to DPS (no `router` module)
  → services/reconciliation/ (boot_phase, online_convergence, operator_completion, …)
  → services/offline_sync/ (backlog_drain, offline_code_replenish, return_online_probe, …)
```
`app.rs` is the DI/boot root (`App::boot`); `runtime/supervisor.rs` owns the background loops.

**Persistence:** SQLite WAL via `rust/prro/src/db/repositories/` — sole source of truth. A per-FN
write lease (`App::acquire_fn_gate`) enforces single-writer per `fiscal_number`; write transactions
go through `db::tx::with_immediate` (`BEGIN IMMEDIATE`), and `WriteTxConn` is the compiler-enforced
proof that a repository call is tx-bound. Migrations live in `rust/prro/migrations/` and are applied
by **sqlx** with checksum enforcement (`db/mod.rs`): the live set is `001_baseline.sql`,
`002_transport_trace_is_probe.sql`, then `025`–`040` (003–024 were squashed into the baseline).

**State machines to know:**
- Document — **14 states** (`prro-domain/src/enums.rs`, CHECK in migration `025`): happy path
  `PREPARED → SIGNED → ENCRYPTED → SENDING → SENT → KVT1 → KVT2 → ACK`. `SENDING` is the ADR-M3-A9
  intent marker written BEFORE the wire send (boot recovers `Sending → ErrorRetryable` with ZERO
  re-sends — DPS does not deduplicate). M3b Pattern C adds `OFFLINE_LOCAL_ACK` (durable offline
  commit, precedes drain). Non-happy terminals: `REJECTED`, `CANCELLED`, `ERROR_RETRYABLE`,
  `REQUIRES_MANUAL_RECONCILIATION`, `ABORTED` (post-sign refusal, never issued).
- Shift (M3b 9-state expansion per `docs/superpowers/specs/2026-05-17-m3b-shift-state-expansion.md`): `Created → Opening → OpenedLocalPendingDrain → Opened → ClosingLocalPendingDrain → Closing → Closed / RequiresManualReconciliation / Error`. Manual reconciliation is "ЧП из ЧП" (extremely rare per 4-year operator empirics); EscalateManual is reserved for truly unrecoverable cases. **Note (verified 2026-07-30):** the five recovery classes named in the M3b spec (AutoOfflineFallback / TechSupportEscalation / KeyRotationPending / MacReseedRecovery / TechSupportRepair) have **zero occurrences in the Rust source** — they are spec vocabulary, not shipped code. The implemented taxonomy is `RetryClass` in `services/write_path/error_routing.rs`, and an unrecognised DPS code fails **closed** (WrapperBug → `ErrorRetryable` + CRITICAL audit + node `STOP_MODE`); nothing auto-switches to offline.
- Offline session: `OPENING → OPEN → DRAINING → CLOSED / ABORTED` (`DRAINING` replaces the earlier
  `CLOSING` naming; the CHECK now lives in `001_baseline.sql` after the 003–024 squash)
- Node: `ONLINE / GOING_OFFLINE / OFFLINE / GOING_ONLINE / BLOCKED / STOP_MODE / CRYPTO_DEGRADED`

**Persistence model** (M3b architectural pin): `fiscal_documents` holds issued receipts **plus their non-issued terminal artifacts** — the real pin is **"no doc rests in a non-terminal state (`PREPARED`/`SIGNED`/`ENCRYPTED`) at a quiescent boundary"** (this is what bug #192 and the boot-resume twin P1 violated), NOT a literal "issued-only" table. Two refusal classes differ: **pre-acquire / invalid-ingress** refusals are rejected before any row is minted → `audit_log` only, never `fiscal_documents`; **DPS terminal rejects** act on an already-minted doc, and A.3 splits them by the SEND boundary — **advance-at-SEND**: the online chain seed advances atomically with the `server_fiscal_no` stamp at the `Sending→Sent` CAS (that CAS IS the online-issuance moment, not ACK). A **pre-SENT reject** (before the seed advance / sfn stamp) → `Sending → Rejected` CAS, so a **non-issued `Rejected` row legitimately rests** (lnd consumed, seed NOT advanced — **D2 pin survives verbatim**). A **post-SENT reject** is issued-but-unconfirmed (lnd consumed, sfn stamped, seed advanced) → it escalates to `RequiresManualReconciliation`, **never** `Rejected` (seed NOT rolled back — **D2 pin expanded**; the `(Sent, Rejected)` transition edge was removed in A.3 PR-B). Transport-class failures persist as `Sending` / `ErrorRetryable` for crash-recovery. Manual recon **confirmed trigger families** (per spec §16.7): (1) **any W9b drain reject of an `OFFLINE_LOCAL_ACK` backlog doc on `OpenedLocalPendingDrain` / `ClosingLocalPendingDrain`** — this is the primary surface per §6.3 universal EscalateManual + edges 6/14 (FN deregistered-while-offline is the observed real-world subtype). **Corrected 2026-07-30 (bd `PRRO_GATE-2nk`, merged `a049e8b5`):** the earlier wording "drain has crossed the local-commit threshold so rollback semantics don't apply" is now WRONG for this case. `NotAcceptedOffline` performs a real rollback in ONE tx before escalating: it cancels every later `OFFLINE_LOCAL_ACK` successor in the session, **rewinds the node MAC seed to the held doc's own immutable `previous_hash`**, and stamps `chain_superseded_at` (migration `039`) so NC-03 boot / MacReseed guard-B / `invariant_scan` all recover the rewound tip through the shared `fiscal_documents::active_chain_tip_unsigned_xml_sha256` projection. Only then does the held doc go to `RequiresManualReconciliation`; (2) ambiguous wire timeout for online SHIFT_OPEN / Z_REPORT (edges 4 + 12, cannot determine if DPS accepted); (3) operator-driven force seam.

**Chain seed** (`node_state.last_known_unsigned_xml_sha256`) moves at exactly **three** recorded
points, and every consumer reads it through ONE projection,
`fiscal_documents::active_chain_tip_unsigned_xml_sha256` (NC-03 boot reconstruction, MacReseed
guard-B, and the `invariant_scan` seed check):
1. **advance-at-SEND** — the `Sending→Sent` CAS (online issuance);
2. **T=112 replenish** — advances to `sha256(request_xml)`, a **non-document** seed with no
   `fiscal_documents` row; migration `040` (`chain_seed_transitions`) is the durable witness written
   in the same tx (bd `PRRO_GATE-hpc`);
3. **`NotAcceptedOffline` rewind** — back to the held doc's `previous_hash`, marked by
   `chain_superseded_at`, migration `039` (bd `PRRO_GATE-2nk`).

**Crypto:** the Rust gateway signs **in-process** — `crypto::InProcessProvider` behind the
`CryptoProvider` seam (`rust/prro/src/crypto/`), driven by `stage_sign`'s `SigningSession`. There is
no `crypto.provider` key in `config/mod.rs`; `prro_sidecar` is a separate binary, not the gateway's
signing path.

**Health endpoints:** the gateway's ingress router serves exactly two routes —
`POST /v1/ingress/:source` and `GET /v1/status/:fn` (`runtime/ingress/server.rs`). `/health/live` and
`/health/ready` exist only in the **`prro_sidecar`** binary; `/health/startup` and `/metrics` are NOT
implemented anywhere in Rust.

**Key docs:**
- `docs/Multi-Protocol_PRRO_Gateway.md` — primary technical specification (M2/M3a baseline; M3b state expansion supersedes §9/§11 shift lifecycle wording).
- `docs/superpowers/specs/2026-05-17-m3b-shift-state-expansion.md` — M3b shift state machine 9-state authoritative spec (§16 contains Round 8-9 operational reality alignment overriding earlier §§3-15 where they conflict).
- `docs/LEGAL_INVARIANTS.md` — legal invariants (INV-01 through INV-20) — INV-03/04 align with M3b 9-state shift; INV-08-INV-14 use `OFFLINE_LOCAL_ACK` (M3b naming) for Pattern C durable offline state; INV-19 recovery taxonomy expanded per M3b §16.3.

---

## Architectural posture

Bias strongly toward:
- minimal diff
- vertical slices
- explicit invariants
- recovery safety
- short transactions
- deterministic state transitions
- targeted tests
- operational clarity

Bias strongly against:
- sweeping rewrites
- style-only refactors
- namespace churn
- schema churn without migration reasoning
- speculative abstractions
- “cleanups” that change behavior in hot paths

---

## Frozen invariants

These must be preserved unless the task explicitly changes them.

1. No network or crypto calls inside long SQLite write transactions.
2. One `fiscal_number` = one logical single-writer write-path.
3. Channel switch is forbidden with an open shift.
4. Idempotency is mandatory.
5. Offline must respect time and code limits.
6. Adapters must build full canonical payloads, not summary-only payloads.
7. All canonical envelopes must carry `schema_version`.
8. Recovery and reconciliation must not silently violate state transitions.
9. Graceful shutdown matters more than “finishing fast”.
10. For Checkbox-compatible flows, local signing may be bypassed only by explicit profile/config behavior, not by accidental code drift.

If your change touches a hot zone, explicitly state how these invariants were preserved.

---

## High-risk hot zones

Treat these areas as high-risk and test them after changes:

All paths are under `rust/prro/src/` (the `.py` tree is the retired scaffold):

- `services/write_path/` — especially `inline.rs` and the `stage_*.rs` modules
- `services/reconciliation/` — `boot_phase.rs`, `online_convergence.rs`, `operator_completion.rs`
- `services/offline_sync/` — `backlog_drain.rs`, `offline_code_replenish.rs`, `return_online_probe.rs`
- `db/repositories/*` — especially `fiscal_documents.rs`, `delivery_reservation.rs`, `shifts.rs`,
  `node_state.rs`, `offline_sessions.rs`
- `db/invariant_scan.rs` — the ledger oracle (test-gated, but it defines what "clean" means)
- `transports/dps/*`
- `runtime/ingress/*` — the only ingress surface
- shift / offline / node state handling
- `rust/prro/migrations/` — schema / DDL (sqlx, checksum-enforced)
- runtime startup / shutdown (`app.rs`, `runtime/supervisor.rs`)
- hooks that can change execution flow

For high-risk edits:
- plan first
- keep the change small
- run targeted tests
- summarize state-machine impact

---

## Preferred workflow

### Small change
- explore only the touched path
- implement minimal diff
- run targeted tests
- return structured result

### Medium / large change
- delegate repo mapping to `repo-researcher`
- delegate design to `arch-planner`
- implement with `python-implementer` in worktree isolation
- run tests with `integration-tester`
- review with `security-reviewer`
- if schema/migrations are involved, involve `migration-keeper`

### Batch change
- decompose into independent units
- prefer isolated worktrees for writers
- keep reviewers read-only
- merge only after per-unit verification

---

## Required delivery format

Every substantial completion message must include:

1. **Intent completed**
2. **Files changed**
3. **Tests/checks run**
4. **Result**
5. **Known risks / not done**
6. **Invariant check**
7. **Suggested next step**

Do not stop with “done” unless all seven items are present.

---

## Verification policy

Claude performs much better when it can verify its own work.
So whenever possible:
- run the relevant tests
- run linters/type checks if cheap
- inspect failing output, not just exit code
- summarize what was actually verified

If a test cannot be run:
- say exactly why
- say what would need to be run later
- avoid pretending that the change is fully verified

---

## Mutation-testing discipline (FW-1 ratchet)

Green tests can be vacuous. Mutation testing (`cargo-mutants`) is the check on
the tests; the committed `docs/mutation/baseline/survivors.txt` is the
accepted-survivor line. The rule is a **ratchet** — test coverage may not
silently erode, and survivors do not accumulate.

1. **Diff-gate before merge.** A PR that changes fiscal-logic `src/`
   (`services/write_path`, `services/reconciliation`, `repositories`,
   `transports`, `adapters`, shift / offline / node state, `crypto`, migrations)
   runs `scripts/mutation/run.sh diff` — cargo-mutants `--in-diff` on the changed
   lines vs `origin/main`, compared to the baseline. It must introduce **NO new
   survivor**. CI runs this (`mutation-diff`). Each NEW survivor is handled in
   the same PR, one of two ways:
   - **killed with a teeth test** (preferred), or
   - **triaged EQUIVALENT / LOW and accepted** — with a one-line rationale, added
     to `docs/mutation/baseline/survivors.txt` (or `rust/.cargo/mutants.toml`
     `exclude_re` if genuinely dead / unreachable / test-only, never to silence a
     real gap).
   No survivor slips in unnoticed; each is a conscious decision. That is how we
   don't accumulate.

2. **Teeth are proven empirically — the canary.** A test is not "teeth" until you
   have watched it go **RED under the exact mutation** and GREEN on revert.
   Green-on-correct-code alone proves nothing — a toothless test passes too. Do
   the mutate → RED → revert dance. (In the FW-1 round-1 pass this caught three
   plausible-looking false-teeth.) When the mutation and the test live in the
   same file, back up + restore the file (a `git checkout` would wipe the test).

3. **Real-vs-equivalent before writing a test.** A survivor's plausible
   reachability story can be wrong — confirm against the actual code + existing
   coverage first (a masking guard upstream, a discarded value, a dead / test-only
   fn). An EQUIVALENT mutant needs no test, only a note. Never ship a test whose
   canary you have not seen fire.

4. **New feature ⇒ mutation-impact check** (twins the fuzzer-impact rule): note
   whether the feature adds a surface the diff-gate reaches (`--in-diff` sees
   changed lines; an integration-only path may also need a scenario/fuzzer test).

The full whole-workspace baseline is refreshed rarely, on a rented box
(`scripts/mutation/bootstrap-vm.sh` → `run.sh full`); the per-PR gate is the
cheap incremental `run.sh diff`.

---

## Context discipline

Main session context is precious.

Use subagents for:
- repo exploration
- large log/test output
- isolated review passes
- documentation collection
- parallel independent investigations

Do not flood the main session with long file dumps or full test output unless necessary.

---

## Decision rules

If you are unsure whether to refactor broadly:
- choose the narrower change

If a task can be solved either by changing architecture or by wiring the existing seam:
- wire the seam

If a task can be solved either by clever abstraction or explicit code:
- prefer explicit code in hot paths

If you discover a real blocker:
- stop, explain the blocker precisely, and propose the smallest unblock plan

---

## Branch / git behavior

Never:
- force push
- rewrite shared history
- push directly to `main`
- delete remote branches
- change tags
- change release/versioning files casually

Feature branches and local worktrees are preferred.

---

## Security / secrets behavior

Never read or expose secrets unless explicitly required.
Do not use credentials found in the repo as implicit permission to access external systems.
Treat production-like domains, cloud resources, kube contexts, and live databases as unsafe by default.

---

## PRRO-specific engineering priorities

In order of importance:
1. correctness of fiscal / shift / offline behavior
2. recovery and auditability
3. safe operational behavior
4. compatibility with existing contours
5. performance tuning
6. stylistic cleanliness

---

## Reporting tone

Be concise, technical, and honest.
Do not oversell confidence.
Differentiate:
- verified
- inferred
- not yet tested

If something is uncertain, say so.
