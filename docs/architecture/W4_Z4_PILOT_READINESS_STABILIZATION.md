# W4-Z4 Pilot Readiness Stabilization

## Problem Statement

After W4-Z3, the Rust PRRO gateway must not enter pilot only because it is
feature-complete. W4-Z4 is a stabilization gate that gives the fiscal path,
recovery paths, SQLite concurrency, crypto and wire profile, offline mode,
live-DPS execution, static hygiene, and known technical debt an explicit
go/no-go status.

The goal is to make the gateway pilot-ready, not merely implementation-complete.

## Start Condition

W4-Z4 starts only after W4-Z3 is complete.

Create the tracking epic:

```bash
bd create "W4-Z4 Pilot readiness stabilization" \
  --description="Ensure gateway readiness for pilot deployment through algorithmic mapping, review playbooks, test matrices, and smoke runbooks." \
  -t epic -p 1 --json
```

All findings, follow-ups, accepted deferrals, and debt must be tracked only in
`bd`, linked with `discovered-from:<epic-id>`.

## Artifacts

1. `docs/architecture/ALGORITHMIC_MAP.md`
2. `docs/architecture/PILOT_REVIEW_PLAYBOOK.md`
3. `docs/architecture/PILOT_TEST_MATRIX.md`
4. `docs/ops/LIVE_DPS_SMOKE_RUNBOOK.md`
5. `bd` epic and linked issues

## 1. ALGORITHMIC_MAP.md

This is the main regulatory document for W4-Z4.

### 1.1 End-to-End Fiscal Flow

For every step:

| Step | Module | State Before | State After | Idempotency Key | Resume Trigger | Resume Actor | DPS Idempotency Surface | Failure Recovery | Tests |
|---|---|---|---|---|---|---|---|---|---|

Required questions:

- Where is the resume point?
- Who performs resume: client retry, boot reconciler, background worker, offline drainer, MAC recovery, or operator CLI?
- What is the local idempotency key?
- What does DPS see as the duplicate-resistant identity?
- What happens on timeout after `send_chk_v2`?
- Why does retry not fiscalize a duplicate?
- Which audit events survive rollback?
- Where does the document become byte-immutable and no longer rebuildable?

### 1.2 Online / Offline Branches

Online:

- ingress
- acquire lease
- create or resume fiscal document
- pin signing inputs
- build canonical XML
- sign attached CMS
- send DPS
- KVT / ACK
- finalize or recover

Offline:

- detect offline eligibility
- allocate offline code
- calculate offline MAC / hash chain
- local ACK
- preserve ordering
- drain later
- DPS acceptance after reconnect

Rejoin:

- offline drain
- send preserved documents
- KVT handling
- failure and retry behavior

### 1.3 State Machines

Document:

- `ingress_inbox.status`
- `fiscal_documents.state`
- `shifts.status`
- `offline_sessions.state`
- `transport_trace`
- MAC recovery state
- operator / cert binding lifecycle

For every machine:

| State | Allowed Next States | Owner Module | CAS Guard | Illegal Transition Behavior | Recovery Path | Tests |
|---|---|---|---|---|---|---|

### 1.4 Cross-Machine Invariants

| Invariant | Machines/Tables | Enforced By | Failure Behavior | Tests |
|---|---|---|---|---|

Required invariants:

- closed shift forbids new fiscal signing
- active offline session requires valid shift context
- `SIGNED` / `SENDING` / `SENT` / `KVT*` document must have immutable signed bytes
- `ingress_inbox DONE` must correspond to terminal or accepted handling
- signing snapshot cannot drift after pin
- offline local ACK does not imply DPS acceptance
- no new online fiscal send while FN is in incompatible offline or stop state

### 1.5 SQLite Transaction Map

All `with_immediate` envelopes:

| Tx Envelope | Caller | Purpose | Max Hold-Time Budget | Tables | Statements | External I/O | Rollback Meaning | Audit Semantics |
|---|---|---|---|---|---|---|---|---|

Rules:

- network, crypto, or filesystem I/O inside `with_immediate` is a pilot blocker
- long CPU inside write transaction is High unless bounded
- no implicit nested transaction assumptions
- `SAVEPOINT` usage must be explicitly documented if present
- raw `SQLITE_BUSY` must not leak as undefined business behavior
- contention must become bounded wait, typed retry, Noop, or controlled failure

### 1.6 External I/O Map

Document:

- DPS gRPC
- CMS signing
- CMP / cert fetch
- JKS / filesystem access
- live-DPS smoke
- where each happens relative to SQLite transactions
- timeout and failure class

### 1.7 Time / Date Map

Document:

- `business_ts`
- XML `TS`
- DPS `date_time`
- CMS `signingTime`
- cert `valid_from` / `valid_to`
- OCSP / CRL time parsers
- KVT age
- transport trace TTL
- SQLite `CURRENT_TIMESTAMP`

Rules:

- internal chronology is UTC only
- DB timestamps are UTC only
- idempotency, ordering, LND, and state transitions never depend on Kyiv-local wall time
- Europe/Kiev projection is render-only
- DST fallback repeated hour must not create key or ordering collisions
- invalid dates must fail loud and must not normalize silently
- system clock rollback behavior must be documented
- parser policy must be explicit for leap seconds, 2049 / 2050, and invalid month / day

### 1.8 Crypto / Wire Profile

Document:

- CP1251 XML bytes
- canonical XML builder
- tax summary derivation
- attached CMS
- eContent
- signedAttrs
- DER SET OF lexicographic sorting
- signingTime
- SigningCertificateV2
- DPS DTO mapping

Rules:

- exact CP1251 XML bytes passed to crypto are immutable
- no XML rebuild, reformat, attribute reorder, whitespace rewrite, or encoding conversion after signing
- exact bytes hashed are exact bytes embedded as CMS `eContent`
- retry and resume use persisted signed bytes unless explicitly in a controlled re-sign path
- DER SET OF sorting guarantee must be documented and tested

### 1.9 Recovery Algorithms

Document:

- boot recovery
- resume
- retryable DPS errors
- MAC recovery
- KVT1 / KVT2 holds
- orphan transport trace closure
- offline drain recovery
- corrupted snapshot / cert metadata handling

For every algorithm:

| Algorithm | Trigger | Detection Query | State Transition | Audit Event | Idempotency Guarantee | Operator Outcome |
|---|---|---|---|---|---|---|

### 1.10 Audit / Forensics Map

| Event | Severity | Emitted In Tx | Survives Rollback | Payload Fields | Operator Meaning |
|---|---|---|---|---|---|

### 1.11 Known Deferrals

Only `bd` links:

| Issue | Severity | Why Not Pilot-Blocking | Owner | Target |
|---|---|---|---|---|

## 2. PILOT_REVIEW_PLAYBOOK.md

### 2.1 Severity Taxonomy

Critical:

- silent fiscal divergence
- duplicate fiscalization
- data corruption
- wrong production / live-DPS target

High:

- realistic race or state corruption
- lost critical forensic event
- wrong crypto / wire profile
- write-path panic on realistic malformed input or date
- uncontrolled SQLite contention

Medium:

- risky missing coverage
- parser / date edge
- recovery ambiguity
- operator runbook gap

Low:

- naming, docs, or local cleanup

Info:

- accepted debt or future hardening

### 2.2 Pilot Blockers

Blocker:

- duplicate fiscalization risk
- wrong CMS profile / `ERROR_VERIFY`
- crypto, network, or filesystem I/O inside SQLite write transaction
- unbounded write transaction
- raw write-path panic on malformed date / input
- state-machine corruption
- raw `SQLITE_BUSY` leaking as undefined behavior
- lost critical forensic audit
- unsafe live-DPS host handling
- unsafe secret handling
- test / production environment ambiguity

Non-blocker:

- naming debt
- stale comments without behavioral risk
- performance tuning without correctness impact
- parser hardening for non-pilot path, if tracked in `bd`
- extra observability nice-to-have

### 2.3 Secrets Logging Review

Pilot blocker if logs, tracing, or audit can expose:

- JKS password
- private key material / `param_d`
- decrypted private container bytes
- full secret-bearing config
- raw secret-bearing XML at INFO / DEBUG without approved redaction policy

Allowed:

- hashes
- SKI
- cert fingerprint
- public cert metadata
- truncated IDs

### 2.4 SQLite Busy Timeout Review

Verify:

- SQLite pool busy timeout is configured
- `with_immediate` contention has bounded behavior
- business logic never treats raw `SQLITE_BUSY` as undefined outcome
- concurrency tests cover contention

### 2.5 Review Rounds

Round A: fiscal / state-machine correctness

Round B: SQLite / concurrency / recovery

Round C: crypto / date / ASN.1 / XML

Round D: DPS / live ops / security

Round E: tests / coverage

### 2.6 Chaos / Fault Injection Round

Automated or semi-automated:

- process kill between state transitions
- network loss during `send_chk_v2`
- concurrent workers on same DB
- malformed timestamps
- corrupt snapshot / cert metadata
- replay same request
- fail during pin / sign / send boundaries

Manual lab:

- ENOSPC
- VM / process kill during SQLite write
- WAL recovery inspection

### 2.7 Required Evidence

Each finding must include:

- severity
- file / line
- invariant violated
- execution path or reproduction
- suggested fix
- expected test

### 2.8 Exit Criteria

- 0 open Critical / High
- Medium fixed or explicitly accepted in `bd`
- Low / Info tracked
- algorithmic map current
- test matrix green
- live-DPS runbook ready

## 3. PILOT_TEST_MATRIX.md

### 3.1 Static Gate

```bash
cargo fmt --check
cargo clippy -p prro --features test-support --tests -- -D warnings
cargo clippy -p prro_crypto --all-targets -- -D warnings
```

### 3.2 Build / Feature Matrix

```bash
cargo build -p prro --tests --features test-support
cargo test -p prro --features test-support
cargo test -p prro --features live-dps --test live_dps_extended_smoke --no-run
```

### 3.3 Targeted Suites

Required areas:

- acquire / sign / send / finalize
- DPS mock channel
- CMS / crypto
- tax mapping
- recovery
- MAC recovery
- offline drain
- boot reconciliation
- cert refresh / cert metadata
- time / date parsers

### 3.4 Concurrency Stress Gate

Must test:

- same DB
- same FN contention
- different FN parallelism
- same request replay
- reader / writer overlap
- acquire / sign / send / finalize contention where applicable

Acceptance:

- no state corruption
- no duplicate fiscal document
- no raw uncontrolled `SQLITE_BUSY`
- no stuck intermediate state without recovery owner

### 3.5 Migration Verification

Test:

- schema N to current
- fiscal documents preserved
- shifts preserved
- transport traces preserved
- pending docs recoverable
- secure DB config preserved
- no historical timestamp damage

### 3.6 Offline State Machine Gate

Required cases:

- enter offline
- allocate offline code
- calculate offline MAC / hash chain
- enforce offline duration limit
- enforce offline code / count limits
- local ACK does not imply DPS acceptance
- drain preserves order
- drain failure is retryable / idempotent
- reconnect does not skip pending offline docs

### 3.7 Rollback / Crash Injection Gate

Required cases:

- fail during acquire transaction
- fail during pin signing inputs
- fail after XML build before CMS persist
- fail after CMS persist before send
- fail after send timeout before KVT persist
- verify no orphan half-state unless recovery owns it
- verify audit behavior: rollback vs committed forensic event

### 3.8 Date / Crypto Tests

Must cover:

- Kyiv summer / winter DST
- repeated local hour at EEST -> EET fallback
- CMS signingTime Jan / Feb
- 2049 / 2050 UTCTIME cliff
- far-future signingTime fail-fast
- cert validity UTCTime / GeneralizedTime
- OCSP / CRL invalid calendar dates
- DER SET OF sorting
- attached CMS eContent
- no XML rebuild after signing

### 3.9 Live DPS Compile Gate

`live-dps` test must compile in CI with `--no-run`. It must not execute in CI.

## 4. LIVE_DPS_SMOKE_RUNBOOK.md

### 4.1 Purpose

Manual live DPS acceptance for the native Rust path, not mock validation.

### 4.2 Pre-flight Checks

Operator verifies:

- JKS file exists
- file permissions are sane
- cert validity window is current
- FN matches key / cert / operator binding
- target host is test cabinet
- `PRRO_FISCAL_MODE=TEST`
- no unexpected open shift
- local clock / NTP is sane
- no active DPS cooldown / rate-limit from previous run
- DB backup / snapshot taken if needed

### 4.3 Environment Contract

Document:

- `PRRO_LIVE_DPS=1`
- `PRRO_FISCAL_MODE=TEST`
- host
- FN
- JKS path
- password handling
- rate-limit caveats

### 4.4 Secret Handling

Rules:

- no password in command-line args
- no password in committed config
- avoid shell history
- prefer interactive prompt or short-lived env var
- unset env vars after run
- never print secret values
- never commit key material

### 4.5 Host Safety

Rules:

- default host must be test cabinet
- host allowlist must parse real URI host
- production endpoints refused
- mode, FN, and host printed before network call
- full fiscal-cycle smoke requires explicit operator confirmation if supported

### 4.6 Execution Steps

Exact commands for:

- compile-only
- connectivity probe
- full live smoke
- log collection
- cleanup

### 4.7 Expected PASS / FAIL

Classify:

- transport fail
- DPS application reject
- CMS verify reject
- rate-limit
- auth / key failure
- state / recovery failure
- clock / cert validity failure

### 4.8 Emergency Off-Switch

Concrete actions:

- stop worker / listener process
- disable ingress for test FN
- prevent further sends
- put FN into safe local stop / offline mode if available
- collect DB / log / audit snapshot
- verify DPS state before rerun
- do not loop retries against live DPS

## Work Order

1. Finish W4-Z3 without scope creep.
2. Create W4-Z4 `bd` epic.
3. Freeze baseline commit.
4. Draft `ALGORITHMIC_MAP.md`.
5. Draft `PILOT_REVIEW_PLAYBOOK.md`.
6. Draft `PILOT_TEST_MATRIX.md`.
7. Draft `LIVE_DPS_SMOKE_RUNBOOK.md`.
8. Run static gate.
9. Fix mechanical fmt / clippy / build debt.
10. Run review campaign.
11. File every finding in `bd`.
12. Fix Critical / High in small pieces.
13. Decide Medium: fix or explicitly accept.
14. Re-run full pilot gate.
15. Produce pilot go/no-go summary.

## Risks And Invariant Impact

Main risks W4-Z4 must control:

- scope creep into broad refactor
- stale docs diverging from code
- accepting Medium without explicit owner
- accidental live-DPS production target
- false confidence from mock-only tests
- clippy / static debt hiding real issues
- untested recovery around send timeout
- timezone repeated-hour bugs
- XML / CMS byte drift after signing

Containment:

- small pieces
- review delta only after each fix
- all debt in `bd`
- no feature expansion unless a blocker requires it

## Final Pilot Gate

Pilot is allowed only when:

- W4-Z3 live-DPS path is reproducible
- W4-Z4 artifacts exist and match code
- 0 Critical / High open
- every Medium has fix or accepted `bd` record
- clippy / fmt / build green
- targeted tests green
- live-DPS compile gate green
- operator runbook complete
- emergency stop path documented
- secrets policy verified
- test / production separation verified
- known deferrals explicit

This is the W4-Z4 contract: make the gateway operationally pilot-ready, not merely
implementation-complete.
