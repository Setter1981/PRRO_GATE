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
  declarations) for at least: `SHIFT_OPEN`, `SHIFT_CLOSE`, `SELL`,
  `RETURN`, `Z_REPORT`;
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
- CMP protocol details (which endpoint, primary vs fallback URLs,
  authentication mode) are still spec-only.  M2 plan must include a
  task to implement a thin `CmpClient` against the test CA before
  the cert_refresher integration test can run.

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

- The exact `proto` definition for `DpsChannel` mock (depends on
  ADR-M2-2 open risk: production wire format check).
- The exact filenames / layouts under `goldens/`.
- The wire format and authentication scheme of the CMP client
  (ADR-M2-4 open risk).
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
  scope is to close the three open risks below (verify wire format,
  CMP probe, prro_crypto API audit) and surface findings as ADR
  fix-commits or new ADR sections.

Open risks (each becomes a W0 task in the mini-plan):

- ADR-M2-2 open risk: **verify production DPS wire format** (gRPC vs
  SOAP/REST).  Without this, "mock DPS as tonic" may be mocking the
  wrong protocol and the W1+ mock-DPS tasks would be invalid.
- ADR-M2-4 open risk: CMP protocol details for the test CA, so the
  cert_refresher integration test in W1+ can be sized.
- ADR-M2-1 open risk: `prro_crypto` API audit — what extensions does
  the wrapper need, and can they be added without breaking the
  Python sidecar consumer?

After W0 lands, the W1..Wn implementation plan can be written
against verified inputs.

---

**Review status:** unreviewed.  M2 plan-writing is blocked until this
ADR receives explicit approval and any required revisions are landed
as docs fix-commits on `rust-gateway`.
