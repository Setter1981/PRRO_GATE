# M2 Pre-Plan ADR — Crypto Provider, Mock DPS, Byte-Equivalence, Cert/Secret Lifecycle, Invariant #1

**Status:** PRE-PLAN — fixates 6 architectural decisions before M2 task
breakdown.  Not yet a plan; M2 plan (`docs/superpowers/plans/2026-MM-DD-…`)
must not be written until this ADR is reviewed.

**Scope:** decisions that have to be locked before any code lands on
`rust/prro/src/crypto/`, `rust/prro/src/transports/`, write-path
staging, or test-vector capture.

**Out of scope:** M3 write-path internals (transition envelope), admin
UI, ingress shells.

**Format:** each ADR has Decision / Alternatives rejected / Tests
required / Open risk.  Open risks are explicit — they do NOT become
plan tasks until reviewed.

---

## ADR-M2-1: Crypto provider = in-process wrapper over `prro_crypto`

### Decision
M2 ships `prro::crypto` as a thin in-process wrapper over the existing
`prro_crypto` workspace crate.  No HTTP hop to a sidecar process.  The
wrapper exposes a `CryptoProvider` trait that crypto consumers
(transports, sign-stage of write-path) take by `&dyn CryptoProvider` so
the call boundary is mockable for tests.

The Python-style sidecar HTTP proxy is **explicitly NOT** the M2
production crypto path.  It is permitted only as:
- a test **oracle** (run live Python sidecar against the same canonical
  inputs and diff output bytes against the in-process implementation);
- a **fixture generator** (capture sidecar output once, freeze in repo
  as test-vectors).

It is NOT a fallback provider.  Production binaries built in M2 do not
ship sidecar HTTP support.

### Alternatives rejected
- **Sidecar HTTP as primary.** Spec §8.3 already names in-process as the
  target.  Sidecar would force every signing op to cross a process
  boundary forever and would freeze the architectural detour as
  load-bearing.
- **Pluggable at runtime (config-flagged in-process or sidecar).**
  Adds a forever-permanent secondary code path that has to be tested,
  documented, and supported.  Pluggability for testing oracle is
  test-time only, not config-flagged at runtime.

### Tests required
- Unit tests on `CryptoProvider` trait round-trip (sign/verify,
  encrypt/decrypt) with deterministic test keys.
- Byte-equivalence goldens (see ADR-M2-3) against Python sidecar
  output for: unsigned XML, CMS-signed bytes (where deterministic),
  KVT parser inputs.
- Negative tests: malformed key material, missing CMP cert, expired
  cert — typed errors, no panic, no secret echo (see ADR-M2-5).

### Open risk
- `prro_crypto` API surface may need extension (e.g. async signing
  wrapper, cert-chain bundle accessor).  M2 plan must allocate a
  task for crypto-crate API audit before writing the wrapper, with
  a hard "no breaking change to existing prro_crypto consumers"
  rule (Python sidecar still uses it).

---

## ADR-M2-2: Mock DPS = native Rust tonic server, full gRPC contract subset

### Decision
M2 ships `tests/mock_dps_server.rs` (or a dedicated test crate) as a
native Rust `tonic`-based server mirroring the gRPC contract subset
that `DpsChannel` calls in production.  The mock implements the
DpsChannel-trait operations (`submit` / `query_status` at trait level;
exact gRPC method names mapped to the actual production `.proto` after
W0 wire-format check, see ADR-M2-2 open risk).  The mock also covers
the metadata fields the transport layer relies on (auth headers,
deadlines, error mapping).

Mock DPS is the primary target for `DpsChannel` integration tests in
M2.  HTTP-replay fixtures of recorded responses are a **secondary**
mechanism — useful for deterministic KVT/KVT2 parsing tests where the
parser is decoupled from the transport — but they do not validate
tonic metadata, gRPC status codes, deadline propagation, retry
behaviour, or connection-failure modes.

### Alternatives rejected
- **HTTP-replay as primary.** Spec decision #20 + §9.3 already say
  native tonic.  Replay cannot exercise streaming, deadlines,
  metadata, or transport-level error categorisation.
- **Reuse the Python `mock_dps_server.py`** by spawning it from Rust
  tests.  Adds a Python runtime dependency to Rust CI; defeats the
  cross-platform claim of T16; replaces a clear contract pin with a
  process-spawn dance.

### Tests required
- `DpsChannel::submit_document` happy path against mock — assert
  emitted gRPC method, headers, deadline, request payload bytes
  (canonical XML), and that the parsed response maps to the typed
  `DpsAck` struct.
- Error categorisation: mock returns `INVALID_ARGUMENT`,
  `UNAUTHENTICATED`, `DEADLINE_EXCEEDED`, `UNAVAILABLE`, transport
  drop mid-call — each yields a distinct typed error variant; no
  variant collapses to "generic error".
- `DpsChannel::query_status` round-trip against mock with realistic
  KVT1/KVT2 payload bytes captured from prod (frozen as fixtures per
  ADR-M2-3).

### Open risk
- The exact gRPC `.proto` we mirror.  If the Python sidecar uses
  hand-rolled `xmlrpc`/SOAP rather than gRPC for the production DPS
  contour, "mock DPS as tonic" is mocking the wrong thing.  M2 plan
  task `W0` must verify the production DPS contour wire format before
  any tonic mock work — and revise this ADR if reality is SOAP/REST.
  This is the single biggest unknown blocking M2 planning.

---

## ADR-M2-3: Byte-equivalence goldens — Python primary oracle, spec vectors secondary

### Decision
Test-vectors that pin byte-for-byte equivalence with the existing
Python gateway/sidecar live in `rust/prro/tests/goldens/` (or a
dedicated `prro_goldens` test crate) and are **frozen** binary
artefacts, not generated at test time.

Capture pipeline:
1. **Primary oracle = current Python gateway/sidecar output** on a
   fixed canonical input set.  Run Python-side once, capture the byte
   output of each hot zone, write to `goldens/<zone>/<case>.bin`.
2. **Secondary = protocol/spec vectors where concrete bytes exist.**
   The current rust-rewrite spec §10 is a cutover plan, not a vector
   annex; concrete bytes today live mostly in the legacy spec PDFs and
   the Python test-data corpus.  Vendor those alongside the goldens or
   add a future annex when one is written.

CI does NOT re-run Python.  Goldens are byte-compared against the
Rust output deterministically.

Hot zones in scope for M2 goldens:
- canonical unsigned XML (cp1251 encoding, attribute ordering, namespace
  declarations) for at least: `SHIFT_OPEN`, `SELL`, `RETURN`,
  `Z_REPORT` (which doubles as the CloseShift wire artifact —
  WebCheck `CreateDB.cs:624` indexes shift-close on `doctype = '80'`,
  the Z-report doctype; DPS `Check.Type::ZREPORT = 2`; revision
  2026-05-06);
- `previous_hash` chain seed (first-after-bootstrap and steady-state);
- CMS-signed XML bytes where DSTU signing is deterministic with a
  fixed test key (otherwise pin only the XML pre-signature);
- KVT1 / KVT2 parser input → typed struct (input-frozen,
  output-asserted);
- Maria 304 frame bytes (in/out, byte-level);
- ESC/POS receipt bytes for at least one printer profile.

### Alternatives rejected
- **Generate goldens at test time from a live Python.** CI already
  cross-platform across 4 Rust targets; Python+Java sidecar would
  need to install on every runner.  Defeats reproducibility — a
  Python upgrade silently invalidates the oracle.
- **Spec vectors only.** Spec gives partial coverage and is sometimes
  out of date vs what the live Python actually emits to DPS.
  Cutover risk is "we broke what kassir/POS already see", not "we
  mismatched a spec".

### Tests required
- Per-zone golden test that diffs Rust output vs frozen file byte-by-byte.
- A "regenerate-goldens" helper script (manual, not CI-triggered)
  that captures from a live Python checkout into `goldens/`.  Script
  prints a diff against the existing frozen vectors so the operator
  can review intentional drift before committing.
- Documented procedure in `docs/M2-goldens-capture.md` for who
  re-captures, when, and how to review the diff.

### Open risk
- Some CMS-signed bytes are non-deterministic (random IV, timestamp).
  The plan must split signing tests into "deterministic prefix"
  (XML-to-be-signed) and "signature shape" (parse + verify, not
  byte-equivalent).  ADR pins the principle; M2 plan task pins the
  splitting strategy.

---

## ADR-M2-4: Cert cache lifecycle — durable metadata, no plaintext keys, atomic flip, refresh outside DB tx

### Decision
`operator_certs` (M1 schema, PK = `ski_hex`, partial unique idx
`(fiscal_number) WHERE active=1`) is the **durable metadata cache** for
operator certificates: `cert_der`, `subject_dn`, `issuer_dn`,
`valid_from`, `valid_to`, `fetched_at`, `source`, `last_refresh_at`,
`active`.  No private key material lives here.

Private key / JKS password material lives only in `sidecar_operators`
(already always-sealed via `xor_soft` per spec decision #16) and is
never copied into `operator_certs`.

Cert refresh policy:
- A `cert_refresher` service in M2 owns the schedule.
- Refresh window is defined by `cert_provisioning_config.refresh_within_days`
  (default 30) and runs as a background task, NOT inside any DB write
  transaction.
- New cert is fetched from CMP outside any tx, then staged into
  `operator_certs` at `active = 0`.
- Atomic flip uses the M1 `with_immediate` primitive: in one tx,
  `UPDATE … SET active = 0 WHERE fiscal_number = ? AND active = 1` and
  `UPDATE … SET active = 1 WHERE ski_hex = ?` — partial unique idx
  guarantees only one active per FN at any moment.
- CMP fetch / network calls are NEVER inside a DB tx (see ADR-M2-6).

### Alternatives rejected
- **Cache private key material in `operator_certs`.** Doubles the
  attack surface; couples cert refresh with secret unsealing; breaks
  invariant #10 in spirit.
- **Refresh inline on signing call.** Signing path is hot; inline
  CMP fetch would block under network failure.  Background refresh is
  the design implication of M1 schema (`cert_provisioning_config`
  carrying `refresh_within_days` / `cache_ttl_seconds`) and ADR-M2-4
  itself; spec §8.3 (cert provisioning) is the reference for protocol
  details, not for the schedule mechanic.
- **No staging — overwrite active cert atomically.** Loses the
  rolling-refresh guarantee; a CMP fetch that brings back stale or
  invalid bytes would replace the working cert.

### Tests required
- Atomic flip test: stage cert B at active=0 while A is active=1; flip
  in one `with_immediate` tx; partial idx still satisfied; subsequent
  signing op uses B.
- Refresh-fails-outside-tx test: simulate CMP timeout in
  `cert_refresher` background loop; assert the active cert row is
  unchanged and no DB tx is open during the failure.
- `cert_provisioning_config.refresh_within_days` honoured: cert with
  `valid_to - now > refresh_within_days` is not refreshed; one with
  `valid_to - now <= refresh_within_days` is.

### Open risk
- Resolved 2026-05-04 in
  `docs/superpowers/specs/2026-05-04-m2-w0-2-cmp-probe.md`.  Ukrainian
  CA "CMP" is an IIT-proprietary 120-byte cert lookup-by-SKI request,
  NOT RFC 4210.  The wire client already exists in
  `prro_crypto::cms::cmp::fetch_cert_by_ski` (encode + POST + parse +
  SKI re-check), so M2 W1+ must not spend scope porting an encoder or
  parser.
- The lookup channel is unauthenticated.  Current endpoint rows use
  plain `http://.../services/cmp/` and the existing Rust client disables
  redirects, so the implemented integrity guard is response-side SKI
  re-check, not TLS.  If W1+ requires server-authenticated transport,
  it must prefer verified HTTPS endpoints where available or document
  the operational exception.
- Remaining W1+ work on this path is orchestration: async wrapper
  (likely `spawn_blocking` around the existing blocking
  `fetch_cert_by_ski`), service-level retry/backoff/multi-URL fallback
  following `sql/016_ca_endpoints.sql`, and atomic active-flip via the
  M1 `with_immediate` primitive.

---

## ADR-M2-5: Secret material flow — unseal at crypto boundary, zeroize, no logging, typed errors without secret echo

### Decision
Secret material (`sidecar_operators.jks_password_hex`,
`sidecar_operators.cred_salt`, raw JKS file bytes, decrypted private
key bytes) flows through the system as follows:

1. Reads from DB as sealed (hex + salt) — no plaintext on the
   read path.
2. Unseal happens **only at the crypto operation boundary** (just
   before signing / decrypting), in a function that takes the sealed
   material and the operator's identity, and returns a short-lived
   handle (e.g. `SigningSession`).
3. Plaintext password / key bytes are stored in `Zeroizing<...>`
   wrappers (existing `zeroize` crate) for the duration of the call,
   and are dropped (zeroed) at function return.  No `String` /
   `Vec<u8>` of plaintext crosses an `await` boundary unless wrapped.
4. Logging boundary (NB: `tracing_subscriber::EnvFilter` is NOT a
   field-redaction mechanism — it filters by target/level only, so
   "filter by field name" is the wrong shape).  The actual rule is:
   a) Structured tracing events MUST NOT emit secret-bearing fields
   at the call site (no `tracing::info!(jks_password = …)` etc.).
   b) Secret-bearing types implement redacted `Debug` / `Display`
   (Rust pattern: `#[derive(Debug)]` is forbidden; manual `impl Debug`
   prints `"<redacted>"`).  This makes accidental `?password` /
   `{password:?}` safe by construction.
   c) Errors are typed with enum reasons, e.g.
   `CryptoError::JksUnseal { operator_id, reason: SealKind }` —
   reasons are enums, NOT free-form `String`s, so a developer
   cannot accidentally `format!` a password into the error.
   d) Tests install a `tracing` subscriber, run signing / unsealing
   end-to-end, and assert NO captured event contains a substring of
   the seeded password / cred_salt / private-key bytes.  This is the
   actual safety net, not a global filter.
5. The crypto provider trait does not return plaintext key material to
   callers; it returns signed/encrypted output bytes only.

### Alternatives rejected
- **Decrypt once at App boot, hold in process memory.** Long-lived
  plaintext in memory; harder to zero on shutdown; turns the whole
  process into a high-value secret target.
- **Free-form error strings.** Past Python sidecar incidents show
  this is exactly how passwords leak into logs.
- **Trust callers not to log.** Default safe configuration must make
  the unsafe path require explicit opt-in.

### Tests required
- A "no secret in error message" test: trigger every typed crypto
  error variant and assert the error's `Display` and `Debug`
  representations contain none of: a substring of the seeded JKS
  password, a substring of `cred_salt` hex, raw cert DER bytes.
- A `Zeroizing` discipline test: where reasonable, verify the
  underlying buffer is zeroed after `drop` (best-effort — Rust does
  not guarantee zero on all platforms but the wrapper is the
  industry-standard pattern).
- A logging filter test: install a test subscriber, run a signing
  op end-to-end, assert no log line contains the JKS password or
  cred_salt.

### Open risk
- Async + `Zeroizing` interactions are subtle — a future that holds
  a `Zeroizing<Vec<u8>>` across `await` keeps the buffer alive
  during scheduling.  The plan must include a guideline (and at least
  one test) on minimising that window: do all crypto work
  synchronously inside one async function body, never store plaintext
  in a self-referential future or in `tokio::spawn`'d tasks unless
  the ownership is provably ephemeral.

---

## ADR-M2-6: Invariant #1 enforcement — staged pipeline; crypto/transport modules MUST NOT accept `&mut SqliteConnection`

### Decision
`#1: No network or crypto calls inside long SQLite write transactions`
is enforced at the **module API surface**, not just by convention.

Architectural rule: **modules in `crypto::` and `transports::` MUST
NOT accept any sqlx connection / pool / transaction handle in their
public API.**  They take typed inputs (canonical bytes, request
structs) and return typed outputs.  This makes "crypto inside
`with_immediate`" a compile error, not a code-review catch.

**`cert_refresher` and repositories are explicitly OUT of scope of
this rule.**  Cert refresh is a service / orchestrator concern that
DOES read & write `operator_certs` rows via repositories — taking a
`SqlitePool` there is correct.  The rule it follows is the staged
pipeline below: any CMP fetch / crypto operation runs OUTSIDE any
`with_immediate`, the result is materialised as typed bytes, and the
DB write that flips `active` (a single CAS UPDATE compound) runs
inside `with_immediate` afterwards.  `cert_refresher` lives under
`services::` (or similar), NOT under `crypto::` / `transports::`,
specifically so that the no-DB-handle rule applies cleanly to the
provider/channel API surface without trapping the orchestrator.

Write-path stages in M3 (and any service-level orchestration in M2)
follow the staged pipeline:

```
[stage 1] DB acquire/mark in with_immediate -> drop tx
[stage 2] crypto sign / encrypt              <- no DB handle, no with_immediate
[stage 3] transport send / receive           <- no DB handle, no with_immediate
[stage 4] DB transition + audit_log.append in with_immediate
```

Between stages, state is materialised (canonical bytes, hash, request
id, response bytes).  Each stage's public API takes those values, not
DB handles.

The M1 `transition_state` Conflict/NotFound race
(`PRRO_GATE-k99`) becomes a non-issue under this model: in stage 4,
caller wraps a single CAS-then-audit_log compound op in
`with_immediate`.  No crypto / network is in scope.

### Alternatives rejected
- **Convention-only enforcement** ("everyone knows not to do this").
  Spec already lists this as invariant #1; it has been violated in
  the Python codebase (per project memory `safe-write-path-change`).
  Compile-level enforcement is the only way.
- **Allow `Pool` (not connection) into crypto/transports** "for
  configuration".  Crypto config is read once at App boot, not
  per-call; pool inside crypto is a code smell that will end up
  spawning a `with_immediate` somewhere.

### Tests required
- A static / structural check: a CI lint (or a dedicated test that
  scans the crate's public API) that asserts no `pub fn` / `pub
  async fn` in `crypto::` or `transports::` mentions
  `SqlitePool`, `SqliteConnection`, `Pool<Sqlite>`, or `Transaction`.
  Implementation likely via a small `cargo-deny`-style rule or a
  hand-rolled test that uses `syn` to parse `mod.rs`.
- An integration test on the staged pipeline: insert a doc into
  `fiscal_documents` (stage 1), run a no-op sign through the crypto
  trait (stage 2), assert that during stage 2 the DB pool reports
  zero in-flight transactions.

### Open risk
- The CI lint may have to be pragmatic about test code (`#[cfg(test)]`
  modules can use whatever they want).  Plan must specify exactly
  which targets the lint covers (lib only, not tests, not examples).
- Future write-path orchestrator may want to share a `Span`-like
  context across stages (correlation id, FN, request id).  That's
  a typed value, not a DB handle, so it does not violate this ADR;
  call it out in the plan to head off "but I need to pass the pool
  for tracing" arguments.

---

## What this ADR explicitly does NOT decide

- The exact Rust module/proto layout for the generated `DpsChannel`
  mock.  W0-1 resolved the production wire format as gRPC and named
  `src/prro_gateway/transports/proto/fiscal_server.proto` as the schema
  of record; W1+ still chooses the Rust-side crate/file layout.
- The exact filenames / layouts under `goldens/`.
- The endpoint policy for CMP over `http://` vs verified HTTPS.  W0-2
  resolved the wire format and auth model; W1+ decides whether to keep
  the Python-compatible HTTP endpoints as an operational exception or
  prefer HTTPS endpoints where verified.
- M2 task breakdown (W1..W4 or otherwise) — that is the M2 plan,
  written **after** this ADR is reviewed.

## What blocks M2 plan-writing

The split is intentionally narrow to keep the ADR a *gate*, not a
*ceiling*:

- **M2 implementation plan (W1+) is BLOCKED** until the open risks
  below are resolved (or explicitly deferred with documented
  assumptions).  Without resolution, the implementation tasks can
  only be guesses.
- **A short M2-W0 research / ADR-resolution mini-plan IS allowed**
  and is the recommended next artifact: a focused plan whose only
  scope is to close the three W0 risks below (verify wire format,
  CMP probe, prro_crypto API audit) and surface findings as ADR
  fix-commits or new ADR sections.

W0 risk status (each item maps to a task in the mini-plan):

- ADR-M2-2 open risk: **resolved in W0-1**.  Production DPS wire format
  is gRPC and the schema of record is
  `src/prro_gateway/transports/proto/fiscal_server.proto`; see
  `docs/superpowers/specs/2026-05-04-m2-w0-1-dps-wire.md`.
- ADR-M2-4 open risk: **resolved in W0-2**.  Ukrainian CA "CMP" is IIT
  cert lookup-by-SKI; `prro_crypto::cms::cmp::fetch_cert_by_ski`
  already implements the wire client; see
  `docs/superpowers/specs/2026-05-04-m2-w0-2-cmp-probe.md`.
- ADR-M2-1 open risk: `prro_crypto` API audit — what extensions does
  the wrapper need, and can they be added without breaking the
  Python sidecar consumer?

After W0-3 lands, the W1..Wn implementation plan can be written
against verified inputs.

---

**Review status:** approved 2026-05-04 (after two review rounds; six
findings closed in commits `7e154bd` and `25ad32a`).  All three W0
risks RESOLVED:
- W0-1 (DPS wire = gRPC) — `1ad7492`, doc `docs/superpowers/specs/2026-05-04-m2-w0-1-dps-wire.md`.
- W0-2 (CMP = IIT lookup-by-SKI, `prro_crypto::cms::cmp::fetch_cert_by_ski` ready) — `2c92907` + `32705e6`, doc `docs/superpowers/specs/2026-05-04-m2-w0-2-cmp-probe.md`.
- W0-3 (`prro_crypto` API surface sufficient; `CryptoProvider` trait shape drafted) — `d58c881`, doc `docs/superpowers/specs/2026-05-04-m2-w0-3-prro-crypto-audit.md`.

M2 implementation plan-writing is UNBLOCKED.  Active artefact: M2 W1+
implementation plan at `docs/superpowers/plans/2026-05-04-m2-w1-implementation.md`
(currently under docs-fix review; do NOT start coding from it until the
review is closed).

---

## ADR-M2-2 amendment — WebCheck parity note (2026-05-05)

A WebCheck `TaxGrpc` decompilation pass during M2/W3-C3 review
revealed wire-contract drift between the canonical
`fiscal_server.proto` (5 RPCs) and the WebCheck reference client
(8 generated RPCs).  This amendment **does not change** the
approved ADR-M2-2 decision (mock DPS = native Rust tonic for the
M2 RPC subset); it pins the scope explicitly and adds a pilot-
readiness open risk.

### Confirmed (no decision change)

- **Mock DPS = native Rust tonic** for the M2 RPC subset
  (`sendChkV2`, `lastChk`, `ping`, `statusRro`, `infoRro`).  This
  is the W3 acceptance scope.
- ByServerFiscalNo semantic = `lastChk(fn_sign) + response.id
  match` (PRRO_GATE-5js); decision unchanged.

### Explicit non-goals (deferred)

- **`sendChk` (API v1)** — deferred unless a pilot configuration
  hard-codes `apiver=1`.  Migration plan for legacy WebCheck-era
  configs lands in PRRO_GATE-0ps.
- **`delLastChk` / `delLastChkId`** — destructive recovery /
  admin operations.  These do NOT belong in `DpsChannel` casually;
  deferred to a separate "DpsAdminOps / manual recovery" ADR with
  audit + hard gate.  Tracked in PRRO_GATE-0ps.

### New open risk (pre-pilot)

- **Pilot parity with WebCheck TaxGrpc requires decisions on 3
  extra RPCs (`sendChk` / `delLastChk` / `delLastChkId`) before
  pilot sign-off.**  These decisions MUST be recorded in
  PRRO_GATE-0ps before M2 → pilot scope review.

### Adjacent operational risks (separate follow-ups)

The same reverse-engineering pass surfaced these gaps; they are
NOT amendments to ADR-M2-2 itself but ARE cross-linked from the
W3 sign-off gate:

- **TLS CA bundle** for production DPS — PRRO_GATE-k54 (P1);
  potentially a small ADR if config shape diverges from M1
  precedent.
- **M3 `services::write_path` retry/recovery policy** derived
  from WebCheck `SubmitPtr.cs:50` — PRRO_GATE-6bj (P1).  Belongs
  in an M3 ADR, not M2.
- **WebCheck COM/1C 19-method compatibility** — PRRO_GATE-iap
  (P2; bumps to P1 if pilot survey identifies a dependent
  operator).
- **Offline lifecycle parity** — PRRO_GATE-gx2 (P1).  Likely
  warrants its own ADR / milestone if pilot requires offline.
- **Print/export/check URL parity** — PRRO_GATE-3a8 (P2).  M5
  scope adjustment, not M2/M3.

See `docs/superpowers/specs/2026-05-04-m2-w0-1-dps-wire.md §10`
for the full RPC matrix and evidence references, and
`docs/superpowers/specs/2026-05-05-webcheck-pilot-parity-findings.md`
for the consolidated pilot-readiness view.

---

## ADR-M3 amendments — W0 findings (2026-05-07)

This block records 9 amendments born out of M3-W0 research
(see `docs/M3-W0-handoff.md` §1 for the gate summary).
Numbering family is `ADR-M3-Ax` (separate from `ADR-M2-x`)
because these decisions belong to the M3 milestone and
emerged from W0 research, not from M2 implementation.

Approval status: **all 9 approved 2026-05-07** (per
`docs/M3-W0-handoff.md` §5 entry-gate user decision).
Closure of bd issues PRRO_GATE-ddn / -zti / -k99 / -6bj /
-ah8 happens at M3a implementation time — not at this
amendment commit.

Sources cited (do not duplicate here):
`docs/superpowers/specs/2026-05-06-m3-w0-1-state-sequence.md`,
`docs/superpowers/specs/2026-05-06-m3-w0-2-lock-discipline.md`,
`docs/superpowers/specs/2026-05-06-m3-w0-3-retry-recovery.md`.

### ADR-M3-A1 — `lnd` source-of-truth: `node_state.next_lnd` transactional sequencer + UNIQUE(fiscal_number, lnd)

**Decision.** M3a uses the existing `node_state.next_lnd` column (per `migrations/001_core_identities.sql:64`) as the lnd source-of-truth, advanced inside `with_immediate` via `UPDATE … SET next_lnd = next_lnd + 1 … RETURNING next_lnd - 1`.  Paired with a new UNIQUE INDEX `ux_fd_fn_lnd ON fiscal_documents(fiscal_number, lnd)` (additive migration `007_lnd_unique.sql`) that fails-closed on any drift.

**Source.** W0-1 §6.1 (4-candidate evaluation; rejected MAX(lnd)+1 / ROWID-AUTOINCREMENT / in-memory counter).

**M3a implementation contract.**  Sequencer call site lives in stage 1 (acquire+validate) per W0-2 §2 row 1; runs inside `with_immediate`.  Existing `next_lnd` column reused as-is; only the UNIQUE index is new.

**Tests.**  Per W0-2 §9 stage-1 fixtures + concurrent-writer race test (UNIQUE constraint asserts on collision).

**Closes (at M3a impl time):** PRRO_GATE-ddn.

### ADR-M3-A2 — CloseShift → ZReport mapping at the Rust XML builder boundary

**Decision.** M3a keeps `OperationType.SHIFT_CLOSE` as the internal canonical label and maps SHIFT_CLOSE → ZReport at the Rust XML builder boundary (`prro::xml::build_canonical_xml`).  Z-number allocation in the Rust write-path MUST derive `wire_artifact_kind` first and allocate when `wire_artifact_kind == ZReport` — NOT key on the internal `OperationType` label like Python does at `write_path.py:535` (which has a latent fragility masked only by upstream COM clients).

**Source.** W0-1 §6.2 (candidate (b) selected; (a) end-to-end rename rejected for schema/COM-1C blast radius).  W0-1 §4.5 + §4.6 binding constraint.

**M3a implementation contract.**  XML builder branch already exists (M2 W4 commit `fd81b03` — `xml_z_report_byte_equivalent_doubles_as_close_shift` golden).  M3a write-path code must NOT replicate Python's `op == OperationType.Z_REPORT` predicate; allocation gate is `wire_artifact_kind == ZReport`.

**Tests.**  W0-2 §9.4 boundary-pattern smoke fixture #1 (Pattern A hoist proof at stage 3) + golden test parity confirms wire output.

**Closes (at M3a impl time):** PRRO_GATE-zti.

### ADR-M3-A3 — `db::tx::with_immediate` enforcement: hybrid (Send bound + static scan + `tokio::task_local!`)

**Decision.** Hybrid enforcement of "no foreign IO inside `with_immediate`" (M2 invariant #1):
1. Keep the `F: ... + Send` bound (catches `!Send` captures; necessary but not sufficient).
2. Add a W5-sibling syn-based static scan over every `with_immediate(...)` closure body.  Denylist: M2 substrate method names (`sign_cms_detached`, `verify_dstu`, `unwrap_envelope`, `fetch_cert_by_ski`, `send_chk`, `last_chk`, `ping`, `status_rro`, `info_rro`, `query_by_local_identity`, `by_server_fiscal_no`) AND literal `tokio::task::spawn_blocking` / `tokio::task::block_in_place` call expressions.
3. Add `tokio::task_local!` `IN_WITH_IMMEDIATE` (NOT `thread_local!` — tokio multi-threaded runtime migrates futures across worker threads after `.await`).  `with_immediate` enters via `IN_WITH_IMMEDIATE.scope((), async { f(&mut wt).await })`.  M2 substrate public-API entry points `debug_assert!(IN_WITH_IMMEDIATE.try_with(|_| ()).is_err(), ...)`.
4. POLICY ONLY for arbitrary helper-fn-of-helper-fn chains past two levels of indirection — reviewer-only.

**Important nuance.** `tokio::task_local!` is visible at provider public-API entry (which runs in the awaiting task's polling context) but NOT inside `tokio::task::spawn_blocking` closure bodies (those run on the blocking pool without async context).  The static scan is the structural gate for ad-hoc `spawn_blocking` inside `with_immediate`; the runtime guard is the gate for substrate-method calls (catches before internal `spawn_blocking` dispatches).

**Source.** W0-2 §3 (4-option evaluation; option (d) hybrid selected).

**Tests.**  W0-2 §9.1 — 5 fixtures: 2 static-scan gates (#1 substrate methods, #3 ad-hoc `spawn_blocking`); 2 runtime gates (#2 indirect helper, #4 provider entry positive control); #5 negative control outside tx.

**Closes (at M3a impl time):** reinforces PRRO_GATE-k99 (the helper-side hardening lives in ADR-M3-A4 below).

### ADR-M3-A4 — `WriteTxConn<'_>` sealed newtype + `transition_state` / `shifts::transition` signature change

**Decision.** Introduce sealed newtype `WriteTxConn<'a>` in `rust/prro/src/db/tx.rs` whose constructor is module-private (`fn new`, NOT `pub(crate)`) and whose `_seal: ()` private field prevents struct-literal construction from outside `db::tx`.  Test-only constructor `#[cfg(test)] pub(super) fn new_for_test` lives inside `db::tx`.  `with_immediate`'s closure signature changes from `for<'c> FnOnce(&'c mut SqliteConnection) -> BoxFuture<'c, _>` to `for<'c> FnOnce(&'c mut WriteTxConn<'c>) -> BoxFuture<'c, _>`.  All transactional repository helpers — `fiscal_documents::transition_state` (`:139`), `shifts::transition` (`:83`), and any future `transition_*` analogues — change signature to take `&mut WriteTxConn<'_>` instead of `&SqlitePool`.

**Source.** W0-2 §4 (3-option evaluation; option (b) sealed-newtype variant selected; option (a) POLICY-ONLY rejected as reviewer-only; option (c) helper-internal micro-tx rejected as breaking compound-op atomicity vs Python `write_path.py:737-773`).

**M3a implementation contract.**  Three lifetime-shape fallbacks documented in W0-2 §4.4 if the borrow checker rejects the primary `for<'c> FnOnce(&'c mut WriteTxConn<'c>)`: (i) separate inner/outer lifetimes with `'a: 'c` bound; (ii) by-value `WriteTxConn<'c>` move into closure.  M3a impl picks whichever compiles cleanest.  Existing inline `with_immediate` call sites at `ingress_inbox.rs:67` + `cert_refresher.rs:292,365` get a mechanical `&mut **conn` refactor (one extra deref through DerefMut).

**Tests.**  W0-2 §9.2 — 5 trybuild compile-fail fixtures (raw `&mut SqliteConnection` rejected; `WriteTxConn::new` private outside `db::tx`; struct-literal seal-field private; valid usage compiles; `new_for_test` cfg(test)-gated).  W0-2 §9.3 two-phase atomicity test (Phase A pre-fix local proof of CAS-vs-SELECT race; Phase B post-fix CI deterministic regression).

**Closes (at M3a impl time):** PRRO_GATE-k99 by construction.

### ADR-M3-A5 — Boundary-pattern selection per pipeline stage

**Decision.** M3a write-path uses:
- Pattern A ("compute outside, persist inside") at stage 3 sign and any other foreign-IO stage where the wire side-effect is naturally idempotent.
- **Pattern B ("persist intent, act, persist outcome") MANDATORY at stage 4 send.**  DPS does NOT deduplicate (Python `write_path.py:148` explicit); the SENDING marker (Python `write_path.py:786-803` + `:144-165`) is the only crash-resume safety mechanism.  Adopting Pattern A only at stage 4 would create a real duplicate-send hazard at DPS on any process crash between state=SIGNED commit and the wire reply landing.
- Pattern C ("stage and flip") reserved for M3b (offline lifecycle).

**Source.** W0-2 §5 catalogue (3 patterns) + §5.4 selection matrix.  Earlier W0-2 draft recommended Pattern A only at stage 4; that recommendation was withdrawn after senior review (single-process daemons can crash mid-wire too).

**M3a implementation contract.**  Pattern B implementation details (DocState::Sending value, migration 008, whitelist additions, recovery rule) live in **ADR-M3-A9 below**.  This ADR is the architectural decision; A9 is the implementation contract.  Both must land together.

**Tests.**  W0-2 §9.4 boundary-pattern smoke fixtures (Pattern A hoist proof at stage 3, Pattern B intent-marker order proof at stage 4, crash-resume zero-send proof for SENDING).

**Closes (at M3a impl time):** reinforces M2 invariant #1 + invariant #4 (idempotency at the wire) by structural design.

### ADR-M3-A6 — DpsError → retry policy table (8 variants × 12 Server-status sub-codes)

**Decision.** M3a adopts the W0-3 §2 main table + §2.1 sub-table as the binding routing contract for `DpsError` variants in `services::write_path`.  Three pillars:
1. WebCheck-derived retry classes for negative `CheckResponse.Status` codes -3, -15 (close-shift only), -16, and 0 (proto-default UNKNOWN) carry distinct semantics from "all other negatives are terminal" — see W0-3 §2.1.
2. Per-call gRPC deadline is constant + short (3–9 s band per WebCheck `All.cs:38-56`); set at `GrpcDpsChannel::connect` time, not per-call.
3. Recovery attempts bounded (default 5, mirror Python `reconciliation.py:44`); on exhaustion, escalate to `REQUIRES_MANUAL_RECONCILIATION` via the ErrorRetryable→RequiresManualReconciliation chain.

All retry / reconciliation work happens OUTSIDE `with_immediate` per M2 invariant #1.

**Pre-requisite — M2/W3 additive amendment to `DpsError::Authorization`** (approved alongside this ADR per `docs/M3-W0-handoff.md` §5 gate item 2):
```rust
// rust/prro/src/transports/dps/error.rs
#[derive(Debug, Error)]
pub enum DpsError {
    #[error("DPS authorization {kind:?} (code={code}): {message}")]
    Authorization { code: i32, kind: AuthorizationKind, message: String },
    // … other variants unchanged …
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationKind {
    /// `-1` ERROR_VEREFY: per-document authorization failure
    DocumentReject,
    /// `-13` / `-14` ERROR_NOT_REGISTERED_RRO / ERROR_NOT_REGISTERED_SIGNER:
    /// per-FN configuration failure
    FiscalNumberNotRegistered,
}
```
Decoder at `dto.rs:178-184` updates to populate the new fields based on the raw status code already in scope.  This is an **additive amendment** to the M2 W3 frozen contract (no existing public API removed; variant gets richer fields).  Without it, the -1 vs -13/-14 routing collapses to "single safe destination = RequiresManualReconciliation" with documented operational-load trade-off — fallback rejected in favour of the additive extension.

**Source.** W0-3 §1 (WebCheck retry-class audit) + §2 main table + §2.1 sub-table.

**Tests.**  W0-3 §9.2 — 21 fixtures (10 covering §2 main 8 variants + 11 covering §2.1 sub-table 12 codes).

**Closes (at M3a impl time):** PRRO_GATE-6bj (retry-policy half).

### ADR-M3-A7 — App::boot reconciliation contract (6-branch per-FN decision tree)

**Decision.** M3a App::boot follows the W0-3 §4.3 per-FN decision tree.  Specifically:
- `node_state::upsert_initial` is permitted ONLY for branch (a) (FN row absent).
- For all other branches, App::boot MUST `node_state::get(fn)` first and reconcile via `list_pending_for_fn` + the W0-3 §3 per-state recovery rules.
- OFFLINE-on-boot in M3a is a hard refusal (branch d, option (i)).  Audit `NODE_STATE_BOOT_OFFLINE_REFUSAL` ERROR.
- Mid-transition shift orphan (branch e, no corresponding pending doc) transitions the shift to `Error` + CRITICAL audit.
- PRAGMA quick_check failure: fail-closed BEFORE any FN-row write; refuse startup; surface via stderr / `/health/startup` 503; do NOT write `STOP_MODE` to a corrupt DB (mirror Python `container.py:144` raises RuntimeError).

**Acceptance test (mandatory in M3a):** create a `node_state` row with `shift_state = Opened`, run App::boot, assert the row still has `shift_state = Opened` (no overwrite).  PRRO_GATE-ah8 acceptance verbatim.

**Source.** W0-3 §4.1–§4.5 (status quo + pre-conditions + decision tree + post-conditions + idempotency invariant).

**M3a implementation contract.** App::boot at `rust/prro/src/app.rs:28` keeps its current shape (pool + migrations); a new method (working name `App::reconcile_pending` or similar) runs the per-FN decision tree before runtime accepts ingress.  Health gates flip in order `live → startup_complete → ready` per Python `runtime/supervisor.py:34-58` parity.

**Tests.**  W0-3 §9.1 — 9 fixtures covering branches (a)–(f) + idempotency (run-twice) + quick_check failure.

**Closes (at M3a impl time):** PRRO_GATE-ah8.

### ADR-M3-A8 — Pending-set documentation alignment (M2's 7 + M3a's SENDING = 8)

**Decision.** The W0-3 §3 pending-state recovery rules table becomes the binding M3a recovery contract.  The 7 M2-shipped pending states from `rust/prro/src/db/repositories/fiscal_documents.rs:176`, PLUS the M3a-introduced `SENDING` state per ADR-M3-A9 (8 pending states total in M3a), plus the explicit exclusions for OFFLINE_LOCAL_ACK / REQUIRES_MANUAL_RECONCILIATION and the 3 terminal states (ACK / REJECTED / CANCELLED), are the M3a recovery surface.

For each pending state the W0-3 §3 table specifies:
- the exact recovery action (re-drive forward / re-query DPS / mark recoverable / mark stuck for operator),
- the whitelist transitions invoked,
- the W0-1 §2.1 design constraint preserved (no Signed→Rejected; Kvt2 forward-only; etc.).

The W0-3 §6 deterministic-replay invariant — particularly §6.6 KVT2 — is the proof obligation that justifies including KVT2 in the pending set.  Removing KVT2 from the pending set would re-introduce the KVT2-strand bug cited at `fiscal_documents.rs:178-180`.

**Source.** W0-3 §3 + §3.1 + §6.

**Tests.**  W0-3 §9.3 — 9 deterministic-replay fixtures (one per pending state).

**Closes (at M3a impl time):** PRRO_GATE-6bj (pending-set / recovery-rules half).

### ADR-M3-A9 — `DocState::Sending` + Pattern B for stage 4 (full implementation contract)

**Decision.** M3a adopts Pattern B for the stage-4 send boundary (per ADR-M3-A5 architectural decision).  This requires a new DocState value `Sending` that joins the pending set and gates wire send through a CAS Signed→Sending → wire → CAS Sending→Sent/Kvt1 sequence, mirroring Python `write_path.py:786-803`.

**Rationale.** DPS does NOT deduplicate at the wire — Python `write_path.py:148` explicitly states so.  Without a SENDING intermediate, a process crash between state=SIGNED commit and the wire reply lets recovery re-drive forward to send, producing a duplicate document at DPS.  The SENDING marker makes the dangerous state structurally distinct from the safe SIGNED state: SIGNED means "stage 4 has not yet started"; SENDING means "wire send was initiated, outcome unknown".  Recovery rules (W0-3 §3 + §6.3) treat the two cases differently: SIGNED is safe to re-drive forward; SENDING is routed to ErrorRetryable for operator inspection and never auto-re-sent.

**Required code changes (M3a impl).**
1. `rust/prro/src/db/models/enums.rs:29-42` — add `Sending => "SENDING"` to the DocState enum (12 → 13 values).
2. `rust/prro/migrations/008_doc_state_sending.sql` (new) — extend the `fiscal_documents.state` CHECK constraint to include `'SENDING'`.  Additive (existing rows keep their states; no backfill).
3. `rust/prro/src/db/repositories/fiscal_documents.rs:81-103` — extend `allowed_transition` whitelist with:
   - `(Signed, Sending)` — Pattern B entry (DPS profile)
   - `(Encrypted, Sending)` — Pattern B entry (Checkbox/encrypted)
   - `(Sending, Sent)` — wire OK, no inline KVT1
   - `(Sending, Kvt1)` — wire OK with inline KVT1
   - `(Sending, ErrorRetryable)` — transient transport failure with known wire reply, OR crash-resume
   - `(Sending, Rejected)` — immediate stage-4-4b terminal reject (Authorization -1, Server -2, -5..-11, -16)
   - `(ErrorRetryable, Sending)` — **retry/requeue path under Pattern B** (the only DPS re-send path).  M3a DPS code MUST NOT use the legacy `(ErrorRetryable, Sent)` whitelist `:99` for wire send — that re-introduces the duplicate-send hazard.
4. `rust/prro/src/db/repositories/fiscal_documents.rs:172-205` — extend `list_pending_for_fn`: doc-comment 7 → 8 pending states; SQL `state IN (...)` clause includes `'SENDING'`.
5. M3a stage-4 implementation: `with_immediate` → CAS Signed→Sending → commit → release → call `DpsChannel::send_chk` outside lock → on reply: `with_immediate` → CAS Sending→{Sent|Kvt1|Rejected|ErrorRetryable} → commit + audit + transport_trace.
6. App::boot recovery worker: a doc found in SENDING after restart is unconditionally CAS'd Sending→ErrorRetryable with audit `crash_resume_sending_to_error_retryable`.  No wire calls made.  Operator (or M3b automated reconciler) resolves via `last_chk` + manual re-queue or escalation.
7. recovery_attempts column policy: SENDING does NOT count toward the per-doc recovery attempt budget on its own; the SENDING→ErrorRetryable transition is bookkeeping, not a retry.

**Acceptance test.** Pre-seed a doc with `state=SENDING`; run App::boot; assert (a) the doc transitions to `ErrorRetryable`, (b) audit `crash_resume_sending_to_error_retryable` is logged, (c) DpsChannel mock records ZERO `send_chk` invocations for the doc id.

**Source.** W0-2 §5.2 (Pattern B catalogue) + W0-3 §3 SENDING row + §6.3 SENDING crash-resume + §8.4 ADR-M3-A9.

**Tests.**  W0-3 §9.2 fixtures #1–#10 (Pattern B routing) + W0-3 §9.3 §6.3 crash-resume fixture.

**Closes (at M3a impl time):** PRRO_GATE-6bj (Pattern B / SENDING half).

### ADR-M3 amendments — alternatives rejected (block-level)

- **Numbering as continuation of M2 (M3-7..M3-15 instead of A1..A9)** — rejected.  M3-W0 is a separate decision family; the `A` prefix marks "amendment born of W0 research" and prevents collision with future M3-original ADRs.
- **Per-ADR commit (9 separate commits)** — rejected.  Decisions are interconnected (SENDING/Pattern B + WriteTxConn + retry policy + App::boot recovery cannot be safely split).  Single amendment commit preserves atomicity of the decision set per `docs/M3-W0-handoff.md` §5 gate item 3.
- **DpsError::Authorization fallback ("single safe destination")** — rejected for pilot.  Loses the per-doc-reject vs FN-config-error distinction; overloads operator on -1 vs -13/-14 where the corrective action is different (rotate doc vs rotate creds).  Additive variant extension chosen instead.

### ADR-M3 amendments — open risk

- **`DpsError::Authorization` amendment** is additive but still touches the M2 W3 frozen public API.  M3a impl prep MUST land it before any §3 SIGNED / SENDING recovery exercises end-to-end.  W3 caller-side update is mechanical (decoder dispatch logic at `dto.rs:178-184`); test impact bounded to the W3 status-routing tests.
- **`WriteTxConn<'_>` lifetime shape** is the riskiest M3a impl item.  Three fallback HRTB shapes documented in W0-2 §4.4; M3a impl validates with `cargo check` early.  If all three shapes fail, fall back to ADR-M3-A4 fallback (option (a) POLICY ONLY review — reduces enforcement to STRONG CONVENTION, with PRRO_GATE-k99 closure deferred until a future enforcement strategy lands).
- **`tokio::task_local!` runtime guard** does NOT cover `spawn_blocking` closure bodies; the static scan covers ad-hoc `spawn_blocking` calls in `with_immediate` closures via the AST denylist.  The combination is sound but relies on the static scan being kept in sync with new substrate methods or new sync-blocking primitives — see W0-2 §9.1 case 3 + ADR-M3-A3 step 4 POLICY-ONLY note for the residual.

### ADR-M3 amendments — what they explicitly do NOT decide

- **Offline lifecycle** (open/drain/close OFFLINE_LOCAL_ACK pool) — M3b scope per `docs/M3-W0-handoff.md` §3.
- **Operator recovery UI / manual reconciliation flows** — M3b scope.
- **Automated SENDING reconciler** (calls `last_chk` with cooldown / rate-limiting to resolve operator-stuck docs) — M3b scope.
- **OFFLINE→ONLINE auto-flip via `ping(fn_sign)`** — M3b scope.
- **`ix_offline_active` UNIQUE migration** — M3b blocker (M3a never opens offline sessions, so the constraint absence cannot be exercised in M3a).

See `docs/M3-W0-handoff.md` §3 + §4 for the full deferral list and bd-issue closure-gate per issue.
