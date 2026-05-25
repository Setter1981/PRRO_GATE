# M4 — Rust Ingress + maria304 Re-Bridge — Implementation Plan

**Status:** REVISED-v2 (2026-05-25 — A3/A7 revision after O1-O5 resolved by operator; W2 rewritten — `operators` table + admin CLI added; `fn_sign` removed as misnomer)
**Branch start:** `docs/w12-audit-dashboard-spec` (M3b CLOSED at rust-gateway `49e11fc` per memory `project_w12_hardening_closure`)
**Supersedes language about Python ingress in:** `docs/M3-W0-handoff.md §3` (already flagged by ADR 2026-05-07 §Propagation)
**Anchors:**
- ADR 2026-05-07 — Rust-only pilot decision (M4 is "Rust ingress + maria304 re-bridge")
- Plan 2026-04-20 — maria304-driver §1 (process boundary), §5 (bridge HTTP contract)
- Frozen Invariants #1, #2, #4, #6, #7, #9 (project CLAUDE.md)
- Operator pin `feedback_manual_recon_catastrophe` (HoldRetry > EscalateManual)

> **Architectural posture.** M3a + M3b shipped the *engine*. M4 ships the
> *housing*: HTTP ingress shell, supervisor loop, response denormalization,
> and the maria304 re-bridge. **The write-path itself is not changed.**
> Everything M4 adds wraps existing stage entry points (`stage_acquire::run`,
> `stage_sign::run`, `dispatch_post_sign`, `stage_send::run`,
> `stage_finalize::run`) and existing repository APIs (`ingress_inbox::insert`,
> `acquire_lease`, `mark_done_tx`, `mark_rejected_tx`).

---

## 1. W0 — Research findings

### 1.1 What already exists (verified by reading the files in §0 of the brief)

| Concern | Status | Location |
|---|---|---|
| `ingress_inbox` repository (idempotent insert + lease + finalize) | DONE | `rust/prro/src/db/repositories/ingress_inbox.rs` — `insert`, `acquire_lease`, `mark_rejected_tx`, `mark_done_tx` (already used by `stage_acquire::run` and `stage_finalize::run`) |
| Stage 1+2 entry: lease + guard + lnd + PREPARED + audit | DONE | `write_path/stage_acquire.rs::run(pool, request_id, CanonicalFiscalCommand) -> WorkerProcessResult` |
| Stage 3 sign | DONE | `write_path/stage_sign.rs::run` (needs `SigningContext`) |
| Post-sign router (Online → send, Offline → offline_ack, Refused) | DONE | `write_path/dispatch.rs::dispatch_post_sign(pool, doc_id, fn)` |
| Stage 4 send | DONE | `stage_send.rs::run(pool, &dyn DpsChannel, doc, Option<&SigningContext>)` |
| Stage 5 finalize (KVT2 → ACK, inbox→DONE, outbox INSERT) | DONE | `stage_finalize.rs::run(pool, doc) -> StageFinalizeOutcome` |
| Backlog drain + return-online probe | DONE | `App::drain_offline_backlog_scheduled`, `App::spawn_return_online_probe` |
| Boot reconciliation | DONE | `App::reconcile_pending_with(deps)` — runs in `Cmd::Serve` but currently nothing calls it |
| Crate-local crypto + DPS channel | DONE | `prro_crypto` rlib; `transports::dps::GrpcDpsChannel` |
| RuntimeView (per-FN identity bundle: dps + signing_ctx + fn_sign) | DONE | `services::reconciliation::runtime::{ReconciliationRuntime, RuntimeView}` |
| `CanonicalCommand` / `CanonicalResponse` wire DTO (driver-side) | DONE | `rust/maria304_driver/src/bridge/dto.rs` |
| Bridge HTTP client (driver-side) | DONE | `rust/maria304_driver/src/bridge/http_client.rs::HttpBridge` — points at `gateway_url` from driver config; no code change required to retarget once Rust gateway listens on that URL |
| HTTP server inside `prro` crate | **MISSING** | Cargo.toml has no `axum` / `tower-http` / `hyper`. The "ingress shell" in `Cmd::Serve` is a `tokio::select` over signals — no listener. |
| Worker loop draining `ingress_inbox` after pure-HTTP insert | **MISSING** | `stage_acquire::run` is invoked directly today only from tests; production has no caller. Production hot path needs (a) HTTP handler that inserts inbox row, (b) worker that drives lease → sign → dispatch → send → finalize. |
| Response denormalization (CanonicalFiscalResponse → wire) | **PARTIAL** | `dto::CanonicalResponse` shape exists; `dto::classify_response` exists; per-protocol mapping (e.g. SOFTBLOCK/SOFTBADART) lives in `maria304_driver/src/protocol/comp_response.rs` already. **Rust gateway side** must build `CanonicalResponse` from `(DocumentRow, StageOutcome)` — that builder does not exist. |

### 1.2 Architectural decisions required BEFORE worklets

These need an operator answer before W2+ touches code. List is short on purpose.

- **A1 — Request/response model: sync-await or async-callback?**
  - **Recommendation (operator-locked):** synchronous. The Maria 304 wire
    protocol is request/response with a 15s `request_timeout_ms` (driver
    plan §6). 1С OLE Manager is also blocking. A synchronous HTTP request
    that returns the final `CanonicalResponse` *after* the write-path has
    reached a terminal state (ACK / OFFLINE_LOCAL_ACK / Rejected) maps
    one-to-one to what the driver expects. The Python interim already
    behaved this way. **Defer async-callback (202 Accepted + webhook) to
    post-pilot.**
  - **Implication:** the supervisor's per-request future must `.await` the
    pipeline through to terminal state before returning HTTP 200/4xx.
    Backpressure is enforced by the supervisor's bounded queue, not by
    HTTP 202.
  - **Why this is safe under invariant #1:** the write-path stages each
    open their own `with_immediate` envelopes; the HTTP handler holds no
    SQLite lock while awaiting stages. Network and crypto already live
    outside `with_immediate` in M3a/M3b — that property carries forward.

- **A2 — One worker per FN or one global worker?**
  - **Recommendation:** **one Tokio task per `fiscal_number`**, multiplexed
    behind `tokio::sync::mpsc::channel`. This is the natural reading of
    Frozen Invariant #2 (one fiscal_number = one logical single-writer)
    and matches ADR-M3-A10 (global-single-writer is currently enforced
    by an App-scoped mutex, but A10 explicitly allows refining to
    per-FN once the seam exists). HTTP handler routes by
    `command.fiscal_number` to the right mpsc; the per-FN task drains
    its mpsc serially.
  - **Trade-off rejected:** one global worker across all FNs. Simpler
    code, but each FN's slow DPS round-trip blocks every other FN; on a
    50-point / 70-cash-register pilot host (operator profile) that is
    pilot-killing latency.
  - **Trade-off rejected:** per-request tokio::spawn with App-mutex. The
    mutex would serialise everything globally, same problem.
  - **Implication for M3a/M3b invariants:** the App reconcile_mutex stays
    as-is for boot-recon + scheduled drain (those are App-scoped
    operations). The per-FN worker mutex is a *new* discipline at the
    supervisor seam, NOT a replacement for the App mutex.

- **A3 — How is per-FN `RuntimeView` (DPS channel + SigningContext) constructed at boot?** *(revised 2026-05-25 — `fn_sign` removed from bindings; see A7)*
  - The runtime composition for M3a/M3b was deliberately deferred
    (`App::spawn_return_online_probe` doc-comment §"Production wiring
    intentionally deferred"). M4 *is* that composition.
  - **Recommendation:** read operator → FN associations from a new
    `operators` SQLite table (see A7), construct one
    `Arc<GrpcDpsChannel>` shared across FNs (DPS endpoint is global
    per environment), construct one `SigningContext` per cashier
    operator (loaded from the operator's EDS key file). Bundle into a
    `HashMap<String, OperatorBindings>` keyed by `fiscal_number`,
    where `OperatorBindings { dps: Arc<dyn DpsChannel>, sign_ctx:
    SigningContext }`. Resolver is a closure that does HashMap lookup.
    Surface as `ReconciliationRuntime::with_resolver`.
  - **`CheckSignBlob` is NOT stored** — it is a CMS-signed blob over
    `(FN + caller metadata)` built **per-request at runtime** via
    `sign_ctx.sign_check_blob(fiscal_number, metadata)`. ДПС does not
    issue any persistent "fn_sign" artefact; the wire field
    `rro_fn_sign` is a derived signature, regenerated each call.
    Verified by reading `rust/prro/src/transports/dps/dto.rs:55-60`
    (doc-comment: "the wire field `rro_fn_sign` is a CMS-signed blob
    containing the FN + caller metadata") and by reading WebCheck
    decompiled source (`docs/webcheck_reverse/WebCheckMain/WebCheck/
    FormOperator.cs:479,546-559`) which stores ONLY the cashier's EDS
    key file path + encoded password, no DPS-issued FN identity blob.

- **A4 — Boot order: recon → ingress listen → return-online probe.**
  - Per `App::boot` doc-comments + `BootError::OfflineModeRefusal`,
    boot reconciliation must complete BEFORE HTTP listen opens. Health
    `/health/ready` flips after recon. This is in line with the
    pilot acceptance "Final Go Criteria" referenced in ADR 2026-05-07
    §Context. `/health/live` and `/health/startup` flip earlier.

- **A5 — Checkbox-compatible REST endpoint scope.**
  - The brief asks for `POST /v1/ingress/maria304` *and*
    `POST /v1/ingress/checkout` (Checkbox-compatible). [OPEN QUESTION]
    On Checkbox the live request payload is a different shape; if the
    pilot operator profile (`user_operator_profile`: 50 points / 70
    cash registers using WebCheck + 1C) does NOT include Checkbox-API
    consumers, drop `/v1/ingress/checkout` to M5. Recommendation:
    **defer Checkbox REST to M5** unless operator confirms a pilot 1С
    contour talks Checkbox JSON natively (memory says 1С uses OLE +
    WebCheck, not Checkbox REST).
  - WebCheck gRPC: also deferred to M5. Brief lists it but pilot
    operator profile doesn't include it as an *ingress* contour —
    WebCheck is the legacy outbound transport, NOT POS-facing ingress.

- **A6 — 1С OLE bridge.**
  - ADR 2026-05-07 §O1 explicitly listed this as decision-needed.
  - The pilot operator (50 points / 70 cashiers, WebCheck + 1С) uses
    1С as POS, which talks to a virtual COM bridge that already speaks
    Maria 304 wire. **Recommendation: 1С goes through the maria304
    driver in M4** (this is exactly the maria304_driver value
    proposition per its plan §1 "Why"). **A separate OLE bridge is M5
    work** if and only if a pilot site uses 1С methods NOT covered by
    the Maria 304 protocol surface (e.g. KSEF replay). For the pilot
    we are NOT shipping that — 1С → Maria304 driver → Rust ingress is
    the path.

- **A7 — Operator (cashier) EDS key storage.** *(added 2026-05-25)*
  - **WebCheck-style storage** in SQLite, NOT in TOML config. The
    decompiled WebCheck source persists per-cashier keys in an
    `OPERATORS` table (columns: `OPERATORNAME`, `INN`, `KEYPATH`,
    `KEYPASS`) with the password obfuscated via `Coding().Cod()`.
    Files are operator-supplied (`.dat` / `.pfx` / `.zs2` / `.pk8` /
    `.jks`) at paths the operator chooses.
  - **Recommendation: mirror this model in `prro` SQLite.** New
    migration adds:
    ```sql
    CREATE TABLE operators (
        operator_id   TEXT PRIMARY KEY,        -- ІПН касира
        name          TEXT NOT NULL,
        key_path      TEXT NOT NULL,
        key_pass_enc  BLOB NOT NULL,           -- obfuscated, NOT crypto-strong
        fiscal_number TEXT NOT NULL,           -- which FN this cashier serves
        created_at    TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP)
    );
    CREATE UNIQUE INDEX ix_operators_fn ON operators(fiscal_number);
    ```
    For pilot 1-cashier / 1-FN, this is unique-by-FN; multi-cashier per
    FN is M5-territory and would relax to a non-unique index + active
    cashier selection at HTTP handler layer.
  - **Why NOT in TOML config:** matches WebCheck operator UX (operator
    adds cashier via UI/CLI without editing config files); avoids
    pushing secrets into shared config; supports rotation without
    restart in M5.
  - **CLI registration** (M4 W2 scope): `prro admin add-operator
    --inn <INN> --name "..." --key-path /var/prro/keys/cashier-<INN>.dat
    --fn <FN>` prompts interactively for password, encodes via the same
    `Coding().Cod()` equivalent (small Rust helper), inserts row.
  - **Per-cashier `SigningContext`** is constructed at boot by reading
    `operators` rows, loading each EDS key file with its decoded
    password, and wrapping in a `SigningContext`. Failed loads emit a
    Critical audit + skip that FN (FN absent from registry → handler
    returns 503 for requests on that FN until operator fixes it).

### 1.3 Architectural pins (constraints these worklets MUST honor)

- The HTTP handler does **insert into `ingress_inbox` synchronously**
  inside its own request future, then **enqueues** a per-FN worker
  message containing `request_id`. The worker (single per FN) calls
  `stage_acquire::run` → ... → `stage_finalize::run` and sends the
  terminal outcome back to the handler via a oneshot channel. This
  preserves Frozen Invariants #2 (single-writer per FN, by construction
  of the mpsc), #4 (idempotency, by reusing `ingress_inbox::insert`'s
  Created/Replay/Conflict tri-state), #6 (handler builds full canonical
  payload — driver already does this), #7 (`schema_version` field
  already in `dto::CanonicalCommand`).
- No network and no crypto call lives inside `with_immediate` — already
  true in M3a/M3b; M4 adds nothing to violate that property.
- The HTTP listener accepts shutdown signals via the *same* signal path
  `main.rs::await_shutdown_signal` uses today. Graceful shutdown order:
  stop accept → drain in-flight per-FN queues with a bounded timeout →
  close App. Invariant #9.

---

## 2. M4 scope definition

### 2.1 M4 MVP — pilot-blocking (IN scope)

1. **HTTP ingress shell** in `prro` crate.
   - `POST /v1/ingress/maria304` — accepts `dto::CanonicalCommand` shape
     (binary-identical to driver-side DTO; see §3 W3).
   - `GET /health/live`, `/health/ready`, `/health/startup`.
   - `GET /metrics` — Prometheus exposition; stub for M4 (real metric
     wiring is M6 ops work; M4 surfaces only request counts + per-FN
     queue depth).
2. **Per-FN supervisor**: bounded mpsc, single tokio task per FN,
   driving `stage_acquire → stage_sign → dispatch_post_sign →
   (stage_send | offline_ack stops here) → stage_finalize`.
3. **Operator bindings registry**: per-FN `Arc<DpsChannel>` (shared) +
   `SigningContext` + `CheckSignBlob`, built once at boot, exposed via
   `ReconciliationRuntime::with_resolver`.
4. **Response builder**: pure function `(DocumentRow, terminal_state) →
   CanonicalResponse` reading from `fiscal_documents` post-terminal.
5. **maria304_driver re-bridge**: zero Rust changes inside
   `maria304_driver` — only `bridge.gateway_url` in deployment config
   flips from Python `:8000` to Rust ingress port. The DTO already
   matches. Confirmed by reading
   `rust/maria304_driver/src/bridge/dto.rs` against §3 below.
6. **Boot orchestration in `Cmd::Serve`**: replace the M1 idle with
   the seven-step sequence: load operator bindings → recon_pending_with
   → spawn per-FN workers → spawn return-online probe → spawn scheduled
   drain ticker → listen HTTP → on signal: stop accept → drain → close.
7. **Pilot smoke**: end-to-end `maria304_driver` → `prro` → live DPS,
   single FN, SELL + Z_REPORT, in both Online and Offline modes (with
   manual node-mode flip for offline validation — automated transition
   already in `App::spawn_return_online_probe`).

### 2.2 Deferred to M5 (OUT of M4 scope)

- **`POST /v1/ingress/checkout` (Checkbox-compatible REST)** — see A5.
- **`POST /v1/ingress/webcheck` (XML-RPC and/or gRPC)** — see A5.
- **1С OLE bridge** as a separate process. M4 routes 1С through Maria304
  driver per A6.
- **XML-RPC ingress** (was in Python). Not in pilot operator profile.
- **Onboarding key/identity automation** (ADR §O2). Pilot is
  hand-provisioned.
- **Python eradication PR** (delete `src/prro_gateway/` tree). M4 just
  makes the Rust stack functional; the actual delete happens in M5
  once we've confirmed pilot acceptance.
- **`prro_sidecar` archive marker** (ADR §D1). Defer.
- **Web admin UI** (ADR §D2). CLI only.
- **`retention.py`-equivalent** and `shift_aggregation.py`-equivalent
  (ADR §O3). Pilot duration likely < 1 month.

### 2.3 Discipline boundary

If during a worklet implementation we find the scope expanding (e.g. a
"small fix" needs to touch `stage_send.rs`), STOP and produce a new
plan entry. M3a/M3b are CLOSED; M4 must be additive. Memory pin
`feedback_autonomous_isolated_env` allows file/branch ops without
confirm; it does **not** authorize hot-zone re-entry.

---

## 3. Worklets

Numbering convention: `W` + monotonic int. Each worklet is intended to
be a single PR. Each lists files, the 10 frozen invariants it
touches, tests, and a binary acceptance gate.

Order is dependency-driven from foundation → integration. Tests are
incremental — each worklet adds its layer on the previous.

---

### W1 — Add axum + tower dependencies, lay out `runtime::ingress` module shell

**One-line:** Cargo.toml + empty `runtime::ingress` module + `Cmd::Serve`
keeps idle.

**Files changed (additive only):**
- `rust/prro/Cargo.toml` — add `axum = "0.7"`, `tower = "0.5"`,
  `tower-http = { version = "0.6", features = ["trace", "timeout"] }`,
  `hyper-util = "0.1"`.
- `rust/prro/src/runtime/ingress/mod.rs` — new file, empty stubs:
  `pub struct IngressServer; impl IngressServer { pub async fn serve(...) }`.
- `rust/prro/src/runtime/mod.rs` — add `pub mod ingress;` (already
  exports `singleton`).

**Invariant impact:** none. No behavior change. `Cmd::Serve` still idles.

**Tests:**
- `cargo build --all-features` green.
- `cargo test` baseline still 256/256.
- New test `tests/ingress_module_compiles.rs`: import the new types
  and assert nothing wires up yet.

**Acceptance:** new deps land; binary still functionally identical to
HEAD.

**Estimate:** 0.5 day.

---

### W2 — Operator-bindings registry + `operators` table + admin CLI

*(revised 2026-05-25 after A7 decision — `fn_sign` removed from struct;
WebCheck-style SQLite storage for cashier EDS keys; CLI registration
command added; migration added.)*

**One-line:** New `operators` SQLite table + builder that reads it,
loads each cashier's EDS key file, constructs per-FN `OperatorBindings
{ dps, sign_ctx }`. Adds `prro admin add-operator` CLI for
registration.

**Files changed:**
- New `rust/prro/migrations/020_operators.sql` — DDL for `operators`
  table per A7. Single migration; pure additive.
- New `rust/prro/src/runtime/bindings.rs` — `pub struct
  OperatorBindings { pub dps: Arc<dyn DpsChannel>, pub sign_ctx:
  SigningContext }` (NO `fn_sign` field); `pub struct
  BindingsRegistry { inner: HashMap<String, OperatorBindings> }`;
  `BindingsRegistry::build_from_db(&AppConfig, &SqlitePool) ->
  anyhow::Result<Self>` reads `operators` rows, loads each
  `key_path`, decodes `key_pass_enc`, constructs `SigningContext`,
  inserts into `HashMap` keyed by `fiscal_number`.
- New `rust/prro/src/db/repositories/operators.rs` — typed CRUD:
  `insert(operator_id, name, key_path, key_pass_enc, fn) ->
  Result<(), OperatorsRepoError>`, `list_all() ->
  Result<Vec<OperatorRow>, _>`, `find_by_fiscal_number(fn) ->
  Result<Option<OperatorRow>, _>`.
- New `rust/prro/src/admin/operators.rs` — extends existing admin
  module with `add_operator` subcommand (reads `--inn`, `--name`,
  `--key-path`, `--fn`; prompts password via `rpassword::prompt_password`;
  encodes via `Coding::encode`; INSERTs via repository). Mirrors
  pattern of existing `reset_stop_mode` admin command (`admin.rs:118`).
- New `rust/prro/src/runtime/coding.rs` — tiny obfuscation helper
  matching WebCheck's `Coding().Cod()` symmetry (NOT crypto-strong —
  matches the WebCheck threat model: protect against casual file
  inspection, NOT against an attacker with DB access). Could be a
  simple XOR-with-constant or rotate-shift; explicit doc-comment
  declaring this is obfuscation NOT encryption.
- `rust/prro/src/main.rs` — add `AdminCmd::AddOperator { ... }` arm.
- `rust/prro/src/runtime/mod.rs` — `pub mod bindings;`, `pub mod
  coding;`.

**Migration safety:** pure additive. No data backfill needed (empty
operators table is the natural pre-pilot state). Migration runs
through existing `sqlx::migrate!` machinery.

**Invariant impact:** None at runtime — registry construction is
boot-only. Invariant #1 not at risk: no SQLite writes inside boot
construction beyond migration + `App::boot` existing path. Invariant
#10 protected: this worklet does NOT touch `signer_guard` logic or
any sign/send path. **New surface**: cashier EDS keys are loaded by
`prro` itself for the first time; failed load emits Critical audit
and skips the FN — handler later returns 503 for that FN.

**Tests:**
- Migration test: `tests/migration_020_operators.rs` verifies
  schema + unique index applied.
- Repository test: `operators::insert` Created + duplicate FN Conflict.
- Coding helper: `tests/coding_roundtrip.rs` — `Coding::encode(s)`
  then `Coding::decode(...)` returns original; non-empty output for
  non-empty input.
- Unit: `BindingsRegistry::build_from_db` with two `operators` rows
  → registry has two FNs, single shared `Arc<DpsChannel>` instance,
  each entry has a `SigningContext` whose underlying key file matches
  the row's `key_path`.
- Unit: `key_path` points to missing file → Critical audit row +
  FN is absent from registry (NOT a panic).
- Unit: `key_pass_enc` decodes to wrong password → Critical audit +
  FN absent.
- Smoke (mock DPS): construct registry against `MockDps` + a temp
  test EDS key, call `App::reconcile_pending_with(resolver)` — should
  still pass the existing reconcile tests, now wired through new
  registry.
- Admin CLI: `prro admin add-operator --inn ... --name ... --key-path
  ... --fn ...` (password piped via stdin in test) inserts a row;
  subsequent `BindingsRegistry::build_from_db` picks it up.

**Acceptance:**
- Migration 020 lands; `operators` table exists.
- `prro admin add-operator` can register a cashier; row visible via
  `sqlite3 var/prro.db "SELECT * FROM operators"`.
- Existing 256 + new tests green when reconcile invoked through
  `with_resolver(|fn| registry.get(fn))`.
- Missing-key / bad-password cases produce Critical audits + 503-able
  registry-absent state (NOT a panic / NOT a startup abort).

**Estimate:** 2.0 days *(was 1.5; added 0.5 for migration + CLI +
coding helper + key-load failure-class tests)*.

---

### W3 — Ingress DTO crate-local copy + parity test against driver-side DTO

**One-line:** Copy `CanonicalCommand` / `CanonicalResponse` into
`prro::runtime::ingress::dto`. Add a build-time parity test that
deserialises a fixture JSON produced by `maria304_driver::bridge::dto`
and asserts roundtrip.

**Files changed:**
- New `rust/prro/src/runtime/ingress/dto.rs` — copy of the driver's
  DTOs with identical serde shapes. Plus mapping helper
  `to_canonical_fiscal_command(&CanonicalCommand) -> CanonicalFiscalCommand`
  which constructs `write_path::types::CanonicalFiscalCommand`
  (doc_type, business_ts, total_sum_kop, payload_json,
  payload_sha256_canonical, signed_by_cashier_id).
- New `rust/prro/tests/ingress_dto_parity.rs` — JSON fixtures (one per
  CommandType variant). Asserts that
  `serde_json::from_str::<prro::CanonicalCommand>(json)` parses, and
  that `serde_json::to_string` re-serialises to a byte-stable form
  matching the driver's emission.

**Trade-off:** *not* sharing the DTO via a new crate dependency from
`prro` → `maria304_driver` because (a) `prro` is the system-of-record
and should not depend on a driver-binary crate; (b) reverse dep (driver
depending on prro) would also pull the entire DB layer. The parity
test guards the wire contract at CI level; rename in either side
breaks the test.

**Invariant impact:** #7 (schema_version) — DTO declares it; mapping
helper REJECTS payloads with mismatched schema_version (typed error).

**Tests:**
- DTO roundtrip per CommandType variant.
- Schema_version mismatch → typed error.
- `to_canonical_fiscal_command` hashes payload via SHA-256 of
  canonical JSON (use existing canonicalization helper; if none exists,
  use serde_json::to_vec sorted-keys via a small helper).

**Acceptance:** DTO parity test green. Mapping helper produces a
`CanonicalFiscalCommand` whose `payload_sha256_canonical` matches the
one that `ingress_inbox::insert` would compute (asserted by inserting
and reading back).

**Estimate:** 1 day.

---

### W4 — Per-FN supervisor (mpsc + worker task)

**One-line:** Implement the per-FN pipeline driver. One mpsc per FN,
one tokio task draining it serially, calling acquire → sign →
dispatch → send → finalize.

**Files changed:**
- New `rust/prro/src/runtime/supervisor.rs` — `pub struct Supervisor {
  txs: HashMap<String, mpsc::Sender<WorkItem>> }`,
  `Supervisor::spawn_for(&App, &BindingsRegistry, fn_id) ->
  (mpsc::Sender, JoinHandle)`, `Supervisor::submit(&self, fn_id, item)
  -> Result<(), SupervisorError>`.
- `WorkItem { request_id: RequestId, command:
  CanonicalFiscalCommand, reply: oneshot::Sender<TerminalOutcome> }`.
- `TerminalOutcome` enum: `Acked { fiscal_id, lnd, ts }`,
  `OfflineLocalAcked { offline_fiscal_no, lnd, ts }`,
  `Rejected { reason: RejectionReason }`, `Refused {
  reason: DispatcherRefusalReason }`, `WireError { class:
  RetryClass, message: Option<String> }`, `StateConflict { ... }`,
  `DocumentMissing`.

**Algorithm (per worker, per work item):**
1. `stage_acquire::run(pool, request_id, command)` →
   - `Noop` → reply `DocumentMissing` (already-leased by another
     copy; should not happen under per-FN serialisation, but typed).
   - `Rejected(r)` → reply `Rejected(r)`; STOP.
   - `Resumed(ctx)` or `Proceed(ctx)` → continue.
2. Resolve `RuntimeView` for FN from `BindingsRegistry`. If `None` →
   reply `Refused(NodeBlocked)` with audit. (Shouldn't happen for a
   configured FN; defensive.)
3. `stage_sign::run(pool, ctx.document, sign_ctx)` → on error, reply
   `WireError(class=Internal)`.
4. `dispatch_post_sign(pool, doc_id, fn_id)` →
   - `Online` → continue to step 5.
   - `Offline{ outcome: Applied { offline_fiscal_no, .. } }` → reply
     `OfflineLocalAcked { offline_fiscal_no, lnd, ts }`. Pipeline
     terminates here; NO finalize call (offline docs terminate at
     OFFLINE_LOCAL_ACK and are drained later by
     `drain_offline_backlog_scheduled`).
   - `Offline{ outcome: Refused(_) }` → reply with the typed refusal.
   - `Refused(reason)` → reply `Refused(reason)`.
5. `stage_send::run(pool, &*deps.dps, doc_id, Some(sign_ctx))` →
   *Note: `stage_send::run` itself builds the per-request
   `CheckSignBlob` via `sign_ctx.sign_check_blob(fiscal_number,
   metadata)` (CMS over FN + caller metadata, regenerated each call).
   Worker passes `sign_ctx` only — does NOT pre-compute or cache a
   blob. Per A3 (revised) the wire `rro_fn_sign` field has no
   persistent storage form.*
   - `Sent { server_fiscal_no, .. }` → continue to step 6.
   - `Routed { decision, .. }` → reply
     `WireError(class=decision.retry_class)`. *NOT* finalize.
   - `StateConflict / DocumentMissing / SignerRefused` → reply
     accordingly.
6. `stage_finalize::run(pool, doc_id)` →
   - `Acked { fiscal_number, lnd }` → reply `Acked { fiscal_id:
     server_fiscal_no, lnd, ts: <document.fiscal_ts from row> }`.
   - `AlreadyAcked` → re-read doc row + reply Acked (idempotent
     replay through worker — should never happen since acquire-lease
     already prevents this; defensive).
   - Other → reply `WireError`.

**Important — what this code does NOT do:**
- Does NOT call DPS / crypto inside any with_immediate (each stage
  already opens/closes its own envelopes).
- Does NOT swallow Critical audits — those are written by the stages
  themselves to `audit_log`.
- Does NOT advance state itself — only the stages do. Worker just
  marshalls the calls.
- Does NOT do retry. WireError outcomes propagate up to the HTTP
  handler which surfaces them as 5xx with a retry hint. Operator-
  side retry semantics are handled by `drain_offline_backlog_scheduled`
  (for Sending/ErrorRetryable rows) and by client retry (which is
  idempotent thanks to `ingress_inbox::insert`'s Replay branch).

**Invariant impact:**
- **#1** Preserved: each stage call is bounded; nothing wraps a
  stage call in `with_immediate`.
- **#2** Preserved structurally: one tokio task per FN, single mpsc;
  cannot interleave. The App reconcile_mutex remains the *additional*
  serialisation against boot-recon / scheduled drain (which can run
  concurrently with HTTP-driven worker).
- **#4** Preserved: `ingress_inbox::acquire_lease` is CAS NEW→PROCESSING
  inside `with_immediate`. Replay via `insert` returning `Replay(row)`
  is handled at the HTTP handler layer (W6), not here.
- **#9** Preserved: worker accepts a `CancellationToken`; on cancel,
  drains pending mpsc with a per-item timeout, then closes.

**Concurrency note (App reconcile_mutex):**
- Steps 1, 4, 5, 6 each take a fresh BEGIN IMMEDIATE — they will
  contend with boot-recon / scheduled drain on the App mutex. **The
  worker MUST NOT hold any guard across stage calls.** Steps run
  sequentially; that's the per-FN single-writer.
- Crucially: the supervisor needs to coordinate with
  `App::drain_offline_backlog_scheduled` so the drain doesn't fire
  while a worker has a doc in PREPARED/SIGNED/SENDING for the same
  FN. The drain's CAS-based source-state filter (`OFFLINE_LOCAL_ACK`
  only) already prevents touching docs the worker is operating on
  — verified by reading `stage_send.rs::run` source-state CAS allowlist
  (Signed | ErrorRetryable | OfflineLocalAck) and the drain's
  use of the App mutex. So the existing locking discipline already
  composes; we just have to *not introduce* a new lock that would
  deadlock with the App mutex.

**Tests:**
- New `rust/prro/tests/supervisor_happy_path.rs` (integration, mock
  DPS): submit a SELL workitem → expect `Acked` outcome; doc row in
  Ack; inbox row in DONE; outbox row present.
- `tests/supervisor_offline_path.rs`: node mode = Offline → expect
  `OfflineLocalAcked`; no finalize; no DPS call.
- `tests/supervisor_rejected_guard.rs`: SELL with shift closed →
  expect `Rejected(ShiftNotOpen)`.
- `tests/supervisor_idempotent_replay.rs`: submit same request_id
  twice → first Acked, second Noop / DocumentMissing (or worker
  short-circuits via inbox already-DONE).
- `tests/supervisor_shutdown_drains.rs`: queue 3 items, send cancel
  before they all process, assert pending get a typed Cancelled
  outcome (not silent drop) — preserves audit trail.

**Acceptance:** all 5 new tests + existing 256 tests green. Supervisor
runs entirely without the HTTP layer.

**Estimate:** 2.5 days.

---

### W5 — Response builder

**One-line:** Convert `TerminalOutcome` + `DocumentRow` (re-read post-
terminal) into a `dto::CanonicalResponse`.

**Files changed:**
- New `rust/prro/src/runtime/ingress/response.rs` — pure functions:
  `pub fn build_canonical_response(pool: &SqlitePool, request_id:
  &RequestId, terminal: &TerminalOutcome) -> anyhow::Result<CanonicalResponseHttp>`
  where `CanonicalResponseHttp` is either `Ok(CanonicalResponse)` or
  `Err(ErrorBody)` matching the driver's expected 4xx shape per
  `bridge/dto.rs` and `http_client.rs::submit`'s error decoder.
- Map outcomes:
  - `Acked` → 200 + `{ok=true, document_state="ACK", fiscal_id, ...}`.
  - `OfflineLocalAcked` → 200 + `{ok=true,
    document_state="OFFLINE_LOCAL_ACK", fiscal_id=offline_fiscal_no, ...}`.
  - `Rejected(reason)` → 4xx + `{ok=false, error_code=...,
    error_message=...}`. Error code mapping from `RejectionReason`
    variant to a `SOFT*` code uses an explicit match arm (no string-
    formatting). NOT hardcoded — pull from a small mapper module so
    Maria-driver-side `comp_response.rs` and the gateway agree.
  - `Refused / WireError / StateConflict / DocumentMissing` →
    5xx + appropriate error_code. WireError(Transport) → SOFTBLOCK.
    WireError(MacRecovery / TerminalReject) → SOFTLOCKED.
- The "re-read DocumentRow" is so that even on Acked we can return
  the persisted `fiscal_ts` (Kyiv-local timestamp built in stage_finalize),
  which the worker does not pass through the oneshot channel.

**Trade-off:** could include `fiscal_ts` in `TerminalOutcome` instead
of re-reading. Re-reading is one extra SQLite read per request but
avoids leaking row-shape into supervisor types. The extra read is
< 1ms on local SQLite WAL — acceptable.

**Invariant impact:** Invariant #7 preserved via DTO `schema_version`
field hardcoded by the response builder.

**Tests:**
- Unit per outcome variant → expected HTTP status + body.
- Property test: `RejectionReason` ↔ error_code mapping is total
  (no panic on any variant, including future ones via `#[non_exhaustive]`
  default arm to SOFTBLOCK).

**Acceptance:** unit tests green; mapping table reviewed against
existing `maria304_driver/src/protocol/error_codes.rs`.

**Estimate:** 1 day.

---

### W6 — HTTP handler + axum router

**One-line:** Axum router with `POST /v1/ingress/maria304` and the
three `/health/*` endpoints. Handler does: parse → `to_canonical_fiscal_command`
→ `ingress_inbox::insert` → if `Conflict` → 409 → if `Replay` →
synthesise reply from existing terminal state → if `Created` →
`Supervisor::submit` + `.await` oneshot → `build_canonical_response`.

**Files changed:**
- `rust/prro/src/runtime/ingress/mod.rs` — flesh out `IngressServer::serve`.
- New `rust/prro/src/runtime/ingress/handler.rs` — handler functions.
- New `rust/prro/src/runtime/ingress/health.rs` — liveness / readiness
  / startup probes. `/health/ready` reads an `AtomicBool` flipped after
  boot recon completes.

**Replay handling (subtle, invariant #4):**
- `ingress_inbox::insert` returns `Replay(row)` when the same
  (fn, idem_key, payload_hash) is re-submitted.
- For Replay, the handler MUST NOT call `Supervisor::submit` (the
  request was already processed or is in flight). It SHOULD read the
  current `fiscal_documents` row by `request_id` and synthesise a
  CanonicalResponse from its state:
  - `state == Ack` → success.
  - `state == OfflineLocalAck` → offline success.
  - `state == Rejected / Cancelled` → 4xx.
  - `state ∈ {Prepared, Signed, Sending, Sent, Kvt1, Kvt2}` →
    response is "still in flight". For sync M4, return 503 with
    Retry-After. This is rare under the per-FN single-writer (only
    happens if the same request is retried *during* its first
    in-flight attempt — strongly suggests buggy client). Document
    in OPERATIONS.md.
- This Replay path is the load-bearing surface of Invariant #4 for
  the synchronous HTTP model. It MUST be tested explicitly.

**Conflict handling:**
- `ingress_inbox::insert` returns `Conflict` when same idem_key +
  different payload hash. Handler returns 409 + critical audit (audit
  emit is at gateway level since the inbox repo only returns the
  outcome without writing to audit_log itself). The audit is by
  Frozen Invariant #4 — different payload for same key is a client
  bug worth a Critical row.

**Invariant impact:**
- **#4** Preserved and surfaced. Replay path tested.
- **#6** Handler receives a fully-built `CanonicalCommand` from the
  driver; it does NOT summarise.

**Tests:**
- New `rust/prro/tests/ingress_http_handler.rs` — uses
  `tower::ServiceExt::oneshot` (axum integration test pattern):
  - Created → 200 Acked.
  - Replay (same payload) → 200 Acked (synthesised, NOT re-processed).
  - Conflict (different payload, same key) → 409 + Critical audit row
    visible in `audit_log`.
  - Malformed JSON → 400.
  - Missing fiscal_number / unknown FN → 404.
  - In-flight replay → 503 Retry-After. (Use a sleep injection point
    inside a mock stage to simulate.)
- Health endpoints: ready=false before recon, ready=true after.

**Acceptance:** all HTTP tests green; replay path proven.

**Estimate:** 2 days.

---

### W7 — `Cmd::Serve` orchestration wiring

**One-line:** Replace the M1 idle in `main.rs::Cmd::Serve` with the
real boot sequence. This is the only file in the production hot path
this worklet touches.

**Files changed:**
- `rust/prro/src/main.rs::Cmd::Serve` arm.
- `rust/prro/src/lib.rs` — re-exports as needed for tests.

**Boot sequence:**
1. `App::boot(cfg)` (existing).
2. `BindingsRegistry::build_from_config(&app.config(), app.db())`.
3. `app.reconcile_pending_with(reg.as_reconciliation_runtime())`.
   - On `OfflineModeRefusal` → exit 78 (existing behavior).
4. Spawn per-FN supervisor workers (one task per FN known to the
   registry).
5. Spawn return-online probe via `app.spawn_return_online_probe(deps,
   shutdown_rx)`.
6. Spawn scheduled drain ticker — new lightweight task that, every
   N seconds (default 30), iterates registry FNs and calls
   `app.drain_offline_backlog_scheduled(fn, &view)`. Backoff state
   already lives in App.
7. Set `/health/ready` AtomicBool to true.
8. Bind axum to `cfg.ingress.bind` (new config field, default
   `127.0.0.1:8000` for pilot single-host).
9. `await_shutdown_signal()` (existing).
10. Initiate shutdown: stop accept → broadcast shutdown_tx → join all
    workers with timeout (cfg.shutdown_timeout, default 30s) →
    `drop(app)` (releases singleton lock).

**Invariant impact:**
- **#9** Preserved and central — graceful shutdown order is explicit.
- **#10** Preserved — Checkbox-compatible signing is NOT enabled here
  (Checkbox REST endpoint is M5).

**Tests:**
- `tests/serve_orchestration_smoke.rs` — spawn the binary against a
  temp DB and mock DPS, send one HTTP request, assert response, send
  SIGTERM, assert exit code 0 and no orphaned files / unflushed audit.
  (May need `assert_cmd` dev-dep; small addition.)
- Restart smoke: kill mid-request, restart, assert boot-recon picks
  up the orphaned doc and either finalises (if DPS got it) or marks
  as ErrorRetryable. This exercises the M3a/M3b recovery paths through
  the M4 boot wiring.

**Acceptance:** smoke test green; production binary actually serves
HTTP for the first time in project history.

**Estimate:** 1.5 days.

---

### W8 — maria304_driver re-bridge config flip + driver-side smoke

**One-line:** Zero Rust code changes in `maria304_driver`. Update its
example config to point at the Rust ingress; produce a CI smoke that
runs both binaries together.

**Files changed:**
- `rust/maria304_driver/example_config.yaml` (or whichever name) —
  `bridge.gateway_url` → `http://127.0.0.1:8000/v1/ingress/maria304`.
- New CI workflow OR new test target `rust/maria304_driver/tests/
  end_to_end_against_rust_prro.rs` — spawns prro + driver, sends a
  TCP-wire Maria 304 SELL, asserts COMP response.

**Verification of contract parity (already done in W3):**
- Driver-side `bridge::dto::CanonicalCommand` → identical to ingress
  DTO. JSON wire is byte-stable.
- Driver-side `bridge::dto::CanonicalResponse` shape matches what the
  Rust ingress emits (W5).
- Driver-side `bridge::dto::classify_response` works against Rust
  responses (it only reads `ok`, `fiscal_id`, `document_state`).

**Invariant impact:** none — only config + a CI test.

**Tests:**
- E2E SELL: 1С-mimicking client → driver TCP → bridge HTTP → prro →
  mock DPS → COMP frame back.
- E2E SHIFT_OPEN + SELL + Z_REPORT sequence.
- E2E Offline: mark node offline, SELL → driver gets COMP with
  `OFFLINE_LOCAL_ACK` state.
- Idempotency: same SELL submitted twice from driver (driver-side
  retry) → second one is Replay-synthesised, no double-fiscalisation.

**Acceptance:** E2E tests green against mock DPS.

**Estimate:** 1.5 days.

---

### W9 — Integration: pilot smoke against live DPS test contour

**One-line:** Re-run the existing pilot smoke (memory `project_sprint7_complete`)
against the new ingress instead of the bare write-path. This is the
M3a-end smoke ADR §Test 4 equivalent for M4.

**Files changed:**
- `rust/prro/tests/live_dps_smoke_via_ingress.rs` (gated behind
  `--ignored` like existing live smokes).
- `docs/superpowers/specs/2026-05-25-m4-live-smoke-runbook.md` —
  runbook describing operator steps to execute the gated smoke.

**Invariant impact:** none directly; this is verification, not new
behavior.

**Tests:**
- Manual against `cabinet.tax.gov.ua:9443` (test contour) following
  the existing Sprint 7 procedure but driving via Maria 304 wire
  rather than calling `stage_send` directly.
- Capture: SHIFT_OPEN succeeds, SELL succeeds, Z_REPORT succeeds.
- Validate: `fiscal_documents` end-state = ACK; `outbox` rows
  present; `audit_log` clean.

**Acceptance:** live smoke pass; documented as "pilot-ready M4" gate.

**Estimate:** 0.5 day implementation + 0.5 day operator-run.

---

## 4. Integration checkpoints

| After | Run |
|---|---|
| W2 | Re-run all M3a/M3b reconcile tests through `with_resolver`. |
| W4 | `cargo test --test supervisor_*` + full test suite (≥ 256 + new). |
| W6 | `cargo test --test ingress_*` + curl smoke against a running binary. |
| W7 | `tests/serve_orchestration_smoke.rs` — first time the binary is end-to-end alive. |
| W8 | `tests/end_to_end_against_rust_prro.rs` — Maria 304 wire works against Rust. |
| W9 | Live DPS smoke (manual, gated). |

**Hard gate before declaring M4 closed:** W9 passes against the real
test DPS contour. If a live-DPS waiver applies (see ADR 2026-05-07 §Tests
required item 4), explicitly record it in
`docs/superpowers/specs/2026-05-25-m4-live-smoke-waiver.md` per ADR
precedent.

---

## 5. Critical-path risks

| ID | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | Per-FN supervisor deadlocks against `App.reconcile_mutex` (scheduled drain blocks worker, worker holds nothing but tries to advance a doc the drain CAS'd) | Medium | High — could halt pilot under offline | Verified above: drain's CAS source-state filter only touches OFFLINE_LOCAL_ACK rows; worker's stage_send CAS handles OfflineLocalAck explicitly. Add a dedicated test: race a drain against a worker SELL on the same FN. |
| R2 | Replay synthesis returns stale data when doc is mid-flight | Medium | Medium — confusing operator behaviour | 503 + Retry-After response for in-flight states; document client retry semantics. Add explicit test. |
| R3 | Boot-recon takes long; ingress not ready in time for first cashier login | Low | Medium | `/health/ready` flips only after recon. Driver-side bridge has `connect_timeout_ms=5000`. Recommend operator runbook: start driver AFTER prro logs `ingress listening`. |
| R4 | Operator wants Checkbox REST as well — pilot site uses Checkbox-compatible 1С module | Low | High if true — adds 1-2 weeks | [OPEN QUESTION → operator]: confirm pilot operator profile does NOT include Checkbox-REST clients. If false, escalate to M4.5. |
| R5 | DTO drift between maria304_driver and prro — silently break wire | Low | High | W3 parity test guards this at CI level. |
| R6 | Bridge token / TLS misconfig — driver can't talk to gateway | Medium | Low (operator-visible immediately) | runbook step in W9 doc. |
| R7 | Per-FN mpsc channel saturated under burst load — handler responses delayed | Low | Low | Bounded channel size = 64 per FN; on full, 503 with Retry-After. Pilot is 1-2 FN, so this is not exercised. |

---

## 6. Estimates *(revised 2026-05-25 — W2 +0.5d for migration/CLI/coding helper)*

| Worklet | Estimate (working days) | Cumulative |
|---|---:|---:|
| W1 axum deps + module skeleton | 0.5 | 0.5 |
| W2 OperatorBindings + migration 020 + admin CLI | 2.0 | 2.5 |
| W3 DTO parity + canonical mapping | 1.0 | 3.5 |
| W4 Per-FN supervisor + worker | 2.5 | 6.0 |
| W5 Response builder | 1.0 | 7.0 |
| W6 HTTP handler + axum router | 2.0 | 9.0 |
| W7 Cmd::Serve orchestration | 1.5 | 10.5 |
| W8 maria304 re-bridge + smoke | 1.5 | 12.0 |
| W9 Live DPS smoke via ingress | 1.0 | 13.0 |

**Total M4 estimate: 13.0 working days ≈ 3 calendar weeks** at the
M3a/M3b discipline cadence (review cycles, scope-creep guards,
operator pin checks). Matches ADR 2026-05-07's "M4 — 4-6 weeks" upper
bound if review rounds add 1-2 weeks of cycle.

---

## 7. Open questions — RESOLVED 2026-05-25

| # | Question | Resolution | Source |
|---|---|---|---|
| O1 | Pilot includes Checkbox-REST ingress? | **NO** — Checkbox REST deferred to M5. Pilot is Maria 304 + 1С → maria304_driver only. | operator 2026-05-25 |
| O2 | Where does cashier EDS key live? | **SQLite `operators` table** (WebCheck-style: per-cashier `key_path` + obfuscated `key_pass_enc`), NOT in TOML config. Migration 020 + `prro admin add-operator` CLI. `CheckSignBlob` (`rro_fn_sign`) is **derived runtime** from EDS key via CMS-sign over `(FN + metadata)` — NOT a persistent artefact, NOT issued by ДПС. | operator 2026-05-25 (corrected initial mis-interpretation); verified vs `transports/dps/dto.rs:55-60` + `webcheck_reverse/FormOperator.cs:479,546-559` |
| O3 | Ingress bind address | **`127.0.0.1:8000`** for pilot (single-host, 1 каса). `0.0.0.0` configurable for multi-host post-pilot. | operator 2026-05-25 |
| O4 | HTTPS on loopback? | **NO** — plain HTTP on loopback for pilot. TLS deferred until multi-host topology emerges. | operator 2026-05-25 |
| O5 | Live DPS smoke waiver applicable for W9? | **YES** — waiver applicable per ADR 2026-05-07 §Tests precedent. W9 closes via documented waiver if test contour unavailable; otherwise live smoke runs. | operator 2026-05-25 |

**All open questions closed.** W1-W9 unblocked; can proceed in
dependency order.

---

## 8. Rollback / containment

- Each worklet is a single PR; revert is `git revert <pr-merge-commit>`.
- W1, W3-W6 are purely additive — none of them changes existing
  `services/write_path/*`, `services/offline_sync/*`,
  `services/reconciliation/*`, repositories, or migrations.
- **W2 adds migration 020 (`operators` table) and one new repository
  module (`operators.rs`).** Pure additive DDL; no existing data is
  migrated. Rollback = `git revert` of W2 PR; migration is forward-only
  by sqlx convention, so a revert leaves the `operators` table empty
  but present — harmless for write-path (nothing reads from it without
  registry construction).
- W7 is the only worklet that changes a hot file (`main.rs::Cmd::Serve`).
  Rollback restores M1 idle behavior, which is safe (binary keeps
  running, no traffic served, audit_log unaffected).
- W8 is config-only on driver side. Rollback restores the Python URL.
- W9 is a test-only worklet.

If a worklet introduces a regression that escapes its tests:
1. Operator pin `feedback_manual_recon_catastrophe` — first instinct
   is **never** to push EscalateManual into the gateway. If the bug
   manifests as a doc stuck mid-pipeline, the recovery path is
   `App::reconcile_pending_with` on next boot, which reads
   `transport_trace.retry_class` and decides — that path is fully
   tested in M3a/M3b and not touched by M4.
2. The worker layer has explicit `HoldRetry`-style retry semantics
   (WireError → caller can retry via idempotency); revert preserves
   that.

---

## 9. What this plan does NOT cover (intentionally)

- WebCheck XML-RPC / gRPC ingress.
- Checkbox-compatible REST ingress.
- XML-RPC ingress (legacy).
- Python eradication PR (delete `src/prro_gateway/`).
- Web admin UI.
- Onboarding key automation.
- Retention / shift_aggregation services.
- 1С OLE bridge as a separate process (1С goes through Maria 304
  driver in M4).
- Multi-tenant operator isolation beyond per-FN bindings (single-operator-
  many-FN is the pilot operator profile).
- Backup/restore runbooks (ADR §D3 — parallel docs+ops track).

All deferred items are explicitly in M5 or later per ADR 2026-05-07.

---

## 10. Sign-off

This plan is **architecturally minimum-diff**:
- Zero changes to `services/write_path/*`, `services/offline_sync/*`,
  `services/reconciliation/*`.
- One new repository module (`operators.rs`) for cashier EDS key
  storage; no changes to existing repositories.
- One new migration (`020_operators.sql`) — pure additive DDL, no
  data backfill, no existing-row touch.
- Zero changes to existing tests (256 stay green; new tests are
  additive).
- Hot-zone files touched in production hot path: exactly one
  (`main.rs::Cmd::Serve`), in exactly one worklet (W7).

This satisfies M3a/M3b's "M3a/M3b CLOSED" status and Frozen Invariant
#10 (signing behavior is unchanged — Checkbox-compatibility deferral
to M5 means no profile/config code drift in M4).

When ready to execute: open W1 PR first; do NOT batch worklets into a
single PR. Each merges to `main` independently per project CLAUDE.md
"branch / git behavior". Operator pin `feedback_autonomous_isolated_env`
authorises file/branch operations without confirm, but per-worklet PR
discipline is preserved.
