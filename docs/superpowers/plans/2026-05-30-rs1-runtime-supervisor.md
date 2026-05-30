# RS-1 — Runtime Supervisor / Composition Root (hot-zone implementation plan)

**Date:** 2026-05-30
**Crate / branch:** `rust/prro` on `rust-gateway`
**Worklet:** RS-1 — first build unit of the runtime spine
**Blueprint:** `docs/architecture/2026-05-30-runtime-spine-connection-blueprint.md` §3 (RS-1)
**Author role:** arch-planner (no code written — plan only)

---

## 1. Problem statement

The deployable binary is **M1-idle**. `Cmd::Serve` (`rust/prro/src/main.rs:359-369`) boots the
`App` then idles (`await_shutdown_signal` → `drop(app)`), comment `:365` "M1 just idles." **Zero
spawned tasks.** The whole write-path / reconcile / drain / probe machinery is built and
unit/integration tested, but **nothing in the binary constructs the per-FN dependencies or drives
the loops.**

RS-1 flips the binary from "M1 idle" to "M3 supervisor running": construct per-FN deps (DPS channel +
signing context + DPS identity blob), run the crash-recovery reconcile pass **once** under the global
single-writer mutex, then spawn the already-tested drain ticker + return-online probe loops, and join
them cleanly on shutdown.

RS-1 does **not** add ingress, a live write-path worker, or health endpoints (RS-2/RS-3/RS-4). After
RS-1 the binary boots → builds deps → reconciles once → runs drain/probe loops; there is still **no
live ingress**.

---

## 2. Current relevant architecture (verified anchors, 2026-05-30)

| Concern | Anchor (verified) | State |
|---|---|---|
| Serve idle | `main.rs:359-369` | bare boot → idle → drop |
| Config root | `config/mod.rs:10` `AppConfig`; `:104` `OfflineCfg`; clamp pattern `:139-149` | NO DPS endpoint field |
| DPS channel ctor (PROD) | `transports/dps/grpc.rs:62` `connect()`; TLS `:75-79`; `impl DpsChannel` `:131` | prod-ready, 0 prod callers |
| Bindings registry (PROD) | `runtime/bindings.rs:179` `build_from_db`; trait `:142` `OperatorKeyLoader`; `OperatorBindings{dps,sign_ctx}` `:55-58` | prod, 0 callers; **no prod `OperatorKeyLoader` impl** |
| RuntimeView / resolver | `services/reconciliation/runtime.rs:64-68` `{dps, signing_ctx, fn_sign}`; `:121` `with_resolver`; `:131` `resolve` | needs all three fields |
| Reconcile entry (PROD) | `app.rs:477` `reconcile_pending_with`; global `reconcile_mutex` `:179` (ADR-M3-A10) | prod |
| Drain entry (PROD) | `app.rs:654` `drain_offline_backlog_scheduled(fn, &view)`; per-FN backoff | prod, "caller = M3+ supervisor" |
| Probe spawn (PROD) | `app.rs:768` `spawn_return_online_probe(deps, shutdown_rx)`; watch-aware; returns `JoinHandle` | prod |
| Signing ctor (PROD) | `crypto/session.rs:130` `unseal_jks(SealedMaterial)`; `SealedMaterial` `:23-32` | prod path |
| Signing ctor (TEST) | `crypto/session.rs:104` `new_for_test` — doc says **"Production must not call this"** | test-only |
| `SigningContext` | `services/write_path/stage_sign.rs:66-70` `{provider, session, profile}` | — |
| Provider (PROD) | `crypto/in_process.rs:21` `InProcessProvider::new()` | prod |
| prro_crypto prims (PROD) | `extract_private_key` (containers.rs:196), `signing_cert()` (containers.rs:105) | prod |
| Live-proven port source (TEST-ONLY, other worktree) | `/mnt/d/prro_gate_m4_w4_z3/rust/prro/tests/live_dps_extended_smoke.rs`: `load_signing_key:277`, `live_signing_ctx:609`, `sign_fn_blob:304` | live-proven 2026-05-29; uses `new_for_test`, not `unseal_jks` |
| Operators row | `db/repositories/operators.rs:27-36` `{operator_id, fiscal_number, key_path, key_pass_enc: Vec<u8>, ...}` | **no `cred_salt` column** |

### Two design tensions discovered while verifying anchors (load-bearing — read before coding)

**Tension A — the seal-format gap (drives the riskiest piece).**
`unseal_jks` wants `SealedMaterial { operator_id, jks_bytes, jks_password_hex (hex), cred_salt (XOR
seal) }`. But the trait `OperatorKeyLoader::load(key_path, &password)` (bindings.rs:142) hands the
loader a **flat decoded password** (`build_from_db` already ran `Coding::decode(key_pass_enc)` per the
trait doc `:166`), and the `operators` row has **no `cred_salt` column**. So the prod loader **cannot
call `unseal_jks` as-is** through the existing trait — the inputs don't line up. Two routes:

  - **Route 1 (loader does its own extraction, mirrors the live-proven port):** loader reads
    `jks_bytes` from `key_path`, treats the `&password` arg as the JKS passphrase, calls
    `prro_crypto::extract_private_key(jks_bytes, password)` → `ExtractedKey`, selects `signing_cert()`,
    then builds `SigningSession`. This is exactly what `load_signing_key` + `live_signing_ctx` do
    today — **but they use `new_for_test`**, which is explicitly forbidden in prod. So Route 1 needs a
    **prod `SigningSession` ctor from `(operator_id, param_d, cert_der)`** added to `session.rs`
    (un-deprecating the shape `new_for_test` already has, but as a sanctioned prod constructor — e.g.
    `SigningSession::from_extracted(...)`). This is a small, contained crypto-surface addition.

  - **Route 2 (loader routes through `unseal_jks`):** requires the `operators` schema + the
    `Coding`/seal semantics to actually carry `jks_password_hex` + `cred_salt`. That is a **schema +
    decode-contract decision** (migration + repo change) that exceeds RS-1's "wire-only" mandate.

  **Recommendation: Route 1** — it matches the live-proven path with the smallest diff and adds one
  sanctioned crypto ctor. Route 2 is deferred (it is the "proper seal pipeline" task, not RS-1).
  **This choice is the central open sub-decision — flag for operator sign-off (see §8).**

**Tension B — `fn_sign` lifetime / ownership (RS-Q3).**
`OperatorBindings` (bindings.rs:55-58) carries only `{dps, sign_ctx}`. `RuntimeView`
(runtime.rs:64-68) needs `{dps, signing_ctx, fn_sign: &'a CheckSignBlob}` and is `Copy` over `&'a`
references. The `fn_sign` blob (native attached CAdES-BES over the FN string, port of
`sign_fn_blob:304`) is **not stored anywhere today**, and `RuntimeView` borrows it by reference — so
it needs an **owned home that outlives the resolver closure**. Cleanest: a supervisor-local
`HashMap<String, CheckSignBlob>` (one per FN), built once alongside the registry, with the resolver
closure borrowing both the registry's `sign_ctx` and this map's `fn_sign` by `&'a`. The supervisor
function owns both, so all `&'a` references are valid for the resolver's lifetime.

---

## 3. Proposed minimal change

Six small vertical-slice pieces. Each is a minimal diff with its own targeted test. Order is
dependency-driven: config → crypto ctor → loader → channel → resolver-bridge → supervisor → integration.

---

### Piece 1 — Config: add DPS endpoint + request timeout

- **Goal:** `OfflineCfg` (or a new `DpsCfg`) gains `dps_endpoint: String` (e.g.
  `https://cabinet.tax.gov.ua:9443`) + `dps_request_timeout_seconds: u64`, with the existing
  clamp+audit pattern for the timeout.
- **Seam:** `config/mod.rs:104` (`OfflineCfg`), mirroring `clamped_probe_interval_seconds`
  (`:139-149`) + the `PROBE_INTERVAL_MIN/MAX` consts (`:136-137`).
- **Files:** `config/mod.rs` (+ example config under `ops/`).
- **Decision:** endpoint has **no default** (fail-closed, same posture as `secure_db_path` `:88`) —
  an operator must choose it explicitly. Timeout gets a sane default + clamp.
- **Test:** unit — TOML parse of a sample with/without the field; clamp returns `(value, was_clamped)`.
- **Invariant note:** none of #1/#2/#9 touched (pure config struct).

### Piece 2 — Crypto: sanctioned prod `SigningSession` constructor (Route 1 enabler)

- **Goal:** add `SigningSession::from_extracted(operator_id, param_d: [u8;32], cert_der: Vec<u8>)` (or
  similar name) as a **production-sanctioned** ctor, so the loader can build a session from an
  `ExtractedKey` without calling the forbidden `new_for_test`.
- **Seam:** `crypto/session.rs` next to `new_for_test:104` / `unseal_jks:130`.
- **Files:** `crypto/session.rs`.
- **Note:** body is identical to `new_for_test`'s; the difference is the doc contract — this ctor is
  the sanctioned Route-1 prod path. Keep `unseal_jks` as the eventual "seal pipeline" path (Route 2);
  document why RS-1 uses `from_extracted` directly. **Gated on the Route-1 decision (§8).** If operator
  picks Route 2 instead, this piece is dropped and Piece 3 changes shape.
- **Test:** unit — round-trip a known `(param_d, cert_der)` → `operator_id()` / cert accessor return
  the inputs.
- **Invariant note:** zeroize discipline preserved (`param_d` wrapped in `Zeroizing` per existing
  `SigningSessionInner`); no plaintext password retained (the ctor never sees one).

### Piece 3 — Production `OperatorKeyLoader` impl (the one real build)

- **Goal:** a prod `impl OperatorKeyLoader` that, per FN: reads `jks_bytes` from `key_path` → calls
  `prro_crypto::extract_private_key(jks_bytes, password)` → selects `signing_cert()` → assembles
  `SigningContext { provider: Arc::new(InProcessProvider::new()), session:
  SigningSession::from_extracted(...), profile: CmsProfile::Dstu4145WithGost34311Pb }`.
- **Seam:** new file `runtime/key_loader.rs`; trait at `bindings.rs:142`; `SigningContext` at
  `stage_sign.rs:66`; `InProcessProvider::new` at `in_process.rs:24`.
- **Port source:** `live_dps_extended_smoke.rs` `load_signing_key:277` + `live_signing_ctx:609` —
  port the logic into `src/`, swapping `new_for_test` → `from_extracted` (Piece 2).
- **Files:** `runtime/key_loader.rs` (new), `runtime/mod.rs` (export).
- **Secret-material discipline (MANDATORY — bindings.rs:119-140):** the `password: &[u8]` borrows a
  `Zeroizing` buffer owned by `build_from_db`. The loader MUST NOT clone it into an un-zeroized
  buffer; if `extract_private_key` needs `&str`/owned, wrap in `Zeroizing` and drop before return.
  The returned `SigningContext` must not retain the password.
- **Error mapping:** map `extract_private_key` failures to `KeyLoadFailure::{FileNotFound (IO),
  WrongPassword (decrypt/MAC), Other}` so `build_from_db`'s audit `reason` tags stay meaningful
  (`bindings.rs:101-110`).
- **Test:** unit with a real test JKS fixture (the W4-Z3 fixtures exist) — success path returns a
  `SigningContext`; wrong-password path returns `WrongPassword`; missing-file returns `FileNotFound`.
- **Invariant note:** #10 (local signing via explicit profile, not drift) — profile is pinned to
  `Dstu4145WithGost34311Pb` explicitly. No network/crypto-in-tx (#1) — loader runs at boot, outside
  any tx.

### Piece 3b (sub-piece) — per-FN `CheckSignBlob` (`fn_sign`) builder

- **Goal:** a prod function `build_fn_sign(sign_ctx: &SigningContext, fiscal_number: &str) ->
  CheckSignBlob` that produces the native attached CAdES-BES signature over the FN string (the DPS
  `rro_fn_sign` identity blob `RuntimeView` needs).
- **Seam:** port `sign_fn_blob:304` from the live smoke; emit alongside the loader so the supervisor
  can build the per-FN `fn_sign` map (Tension B). RS-Q3 explicitly calls this out as missing from
  `OperatorBindings`.
- **Files:** `runtime/key_loader.rs` (same module) — it needs the same `signing_cert()` + native
  signer path the loader already pulls in.
- **Design choice:** build `fn_sign` **eagerly at boot** (once per FN) into the supervisor-owned map,
  NOT lazily per resolve — keeps the resolver closure pure-lookup and gives `fn_sign` a stable owned
  home (resolves the lifetime tension). Open question: signingTime in `fn_sign` is `SystemTime::now()`
  at boot — confirm DPS accepts a boot-time `signingTime` for the read RPCs across a long-running
  process (live smoke signed fresh per call). **Flag as a sub-decision (§8).**
- **Test:** unit — `build_fn_sign` over a fixture key + FN yields a non-empty `CheckSignBlob` whose
  DER parses as a CMS SignedData (structural assert; full DPS-accept is the live smoke's job).
- **Invariant note:** read-path identity only; no tx, no mutation.

### Piece 4 — DPS channel construction (wire-only)

- **Goal:** in the supervisor, read `dps_endpoint` + timeout from config (Piece 1), call
  `GrpcDpsChannel::connect(endpoint, timeout)` (`grpc.rs:62`), wrap as
  `Arc::new(...) as Arc<dyn DpsChannel>`. Single channel shared across all FNs (per
  `OperatorBindings` doc `:52`).
- **Seam:** `grpc.rs:62` (prod ctor, TLS already correct `:75-79` — server-trust, no mTLS).
- **Files:** consumed inside `supervisor.rs` (Piece 5); no change to `grpc.rs`.
- **Failure handling:** `connect` is eager — a bad endpoint fails at boot with
  `DpsError::Transport`. Decide: hard-fail boot (recommended — fail-closed) vs. degrade to a
  no-channel mode. **Flag as a sub-decision (§8).**
- **Test:** covered by Piece 6 integration (a stub/in-process channel for the boot-to-shutdown test;
  real endpoint is the live smoke's domain).
- **Invariant note:** none added; channel construction is pre-loop, no tx.

### Piece 5 — Supervisor module + Serve wiring + graceful shutdown (the integration seam)

- **Goal:** new `runtime/supervisor.rs` with one entry e.g. `run(app: App, config) -> Result<()>`,
  called from `Cmd::Serve` (`main.rs:359`) **after** `boot_from_path_or_exit`, **replacing the bare
  idle** (`main.rs:365-368`).
- **Sequence (exact):**
  1. Build `dps` channel (Piece 4).
  2. Build the prod `loader` (Piece 3) + the per-FN `fn_sign` map (Piece 3b), owned by `run`.
  3. `BindingsRegistry::build_from_db(app.db_secure(), app.db(), dps, &loader)` (`bindings.rs:179`)
     → per-FN registry, owned by `run`.
  4. Build `ReconciliationRuntime::with_resolver(|fn_id| ...)` (`runtime.rs:121`): closure looks up
     `registry.get(fn_id)` + `fn_sign_map.get(fn_id)`; returns
     `RuntimeView { dps: &*bindings.dps, signing_ctx: &bindings.sign_ctx, fn_sign }` or `None`.
     Both borrows are `&'a` into `run`-owned storage (Tension B resolved).
  5. `app.reconcile_pending_with(runtime).await` (`app.rs:477`) **ONCE** at startup, BEFORE any live
     loop — crash recovery under the global `reconcile_mutex` (`app.rs:179`, ADR-M3-A10).
  6. Create `tokio::sync::watch::channel(false)`.
  7. Spawn a **drain ticker**: a `tokio::time::interval` task that, per FN
     (`registry.fns()`), calls `app.drain_offline_backlog_scheduled(fn, &view)` (`app.rs:654`);
     per-FN backoff is internal. Model the biased-`select!` shutdown contract on
     `return_online_probe.rs:444-471` (mirror, don't reinvent).
  8. Spawn `app.spawn_return_online_probe(deps, shutdown_rx)` (`app.rs:768`) — already watch-aware,
     returns `JoinHandle`.
  9. `await_shutdown_signal()` (`main.rs:366`) → flip the watch sender to `true` → **join ALL**
     `JoinHandle`s → then return / `drop(app)`.
- **Files:** `runtime/supervisor.rs` (new), `runtime/mod.rs` (export), `main.rs` (`Cmd::Serve` arm
  only — replace `:365-368`).
- **Lifetime note:** `reconcile_pending_with` takes `ReconciliationRuntime<'a>` borrowing `run`-local
  data — fine, it's `.await`ed inline (step 5). The spawned loops (steps 7-8) take `RuntimeView`/deps
  built from the **same** `run`-owned `Arc<dyn DpsChannel>` + owned signing/`fn_sign` storage; if the
  spawned closures need `'static`, the deps must be `Arc`-cloned into the tasks (the channel already
  is `Arc`; `SigningContext`/`fn_sign` may need `Arc`-wrapping for the spawned drain ticker). **This
  `'static`-vs-borrow boundary for the spawned drain ticker is the subtlest part of the piece —
  resolve it explicitly (likely `Arc<BindingsRegistry>` + `Arc<HashMap<String,CheckSignBlob>>` cloned
  into the ticker task), and confirm `drain_offline_backlog_scheduled` accepts a `&RuntimeView` built
  from `Arc`-held data.**
- **Tests:** unit — supervisor builds registry + resolver from a seeded secure+main DB and an
  in-process stub channel; reconcile runs once; watch flip joins all handles within a bounded time.
- **Invariant notes:**
  - **#9 graceful shutdown:** single `watch::channel`; ALL `JoinHandle`s joined before `drop(app)`;
    drain ticker mirrors the probe's biased-select shutdown.
  - **#2 global single-writer:** reconcile (step 5) completes under `reconcile_mutex` BEFORE any loop
    spawns; loops use the same per-FN-serialized primitives.
  - **#1 no network/crypto in tx:** supervisor only constructs deps + spawns existing loops; adds no
    new tx; the existing loops already enforce `assert_not_in_with_immediate` (`grpc.rs:133`).

### Piece 6 — Boot-to-shutdown integration test

- **Goal:** a `tests/` integration test: boot a real `App` over a temp migrated DB (seeded with ≥1
  operator + matching `fiscal_number_config`), run `supervisor::run` with an **in-process stub DPS
  channel** + a test key fixture, assert: (a) reconcile ran once, (b) drain ticker + probe spawned,
  (c) a `watch` shutdown flip joins all handles cleanly and returns within a deadline.
- **Seam:** mirrors `boot_offline_app` (`live_dps_extended_smoke.rs:658`) + `fresh_app` patterns.
- **Files:** `tests/rs1_supervisor_boot.rs` (new).
- **Design choice:** the supervisor's DPS-channel construction must be **injectable for tests**
  (pass an `Arc<dyn DpsChannel>` in, rather than always calling `GrpcDpsChannel::connect`) — so the
  test can supply a stub. Add a `run_with_channel(app, config, dps)` seam that `run` delegates to.
- **Invariant note:** verifies #9 end-to-end (the one invariant a unit test can't fully cover).

---

## 4. Files to touch (summary)

| File | Piece | Change |
|---|---|---|
| `rust/prro/src/config/mod.rs` | 1 | + DPS endpoint + timeout field + clamp |
| `ops/config.example.yaml` (+ local) | 1 | + endpoint example |
| `rust/prro/src/crypto/session.rs` | 2 | + sanctioned `from_extracted` ctor |
| `rust/prro/src/runtime/key_loader.rs` (new) | 3, 3b | prod loader + `build_fn_sign` |
| `rust/prro/src/runtime/mod.rs` | 3, 5 | exports |
| `rust/prro/src/runtime/supervisor.rs` (new) | 5 | supervisor + `run`/`run_with_channel` |
| `rust/prro/src/main.rs` | 5 | `Cmd::Serve` arm — replace idle `:365-368` |
| `rust/prro/tests/rs1_supervisor_boot.rs` (new) | 6 | boot→shutdown integration |

No schema/migration changes (Route 1). `grpc.rs`, `app.rs`, `bindings.rs`, `runtime.rs` are
**consumed, not modified**.

---

## 5. Risks and invariant impact

- **Riskiest piece: Piece 5 (supervisor + wiring + shutdown).** It is the only piece that mutates the
  binary's run loop, owns the lifetime/`'static` reconciliation for spawned tasks (Tension B + the
  `Arc`-into-ticker boundary), and is the sole guardian of invariant #9. A shutdown bug here = tasks
  orphaned past `drop(app)` or a hang on join.
- **Second-riskiest: Piece 3 + 3b (crypto loader).** Secret-material discipline (bindings.rs:119-140)
  + correct `signing_cert()` selection (the 2026-05-29 `CryptBadSign` root cause was embedding the
  wrong cert — memory `project_native_crypto_dps_verify_blocker`). Get the cert wrong and DPS rejects
  silently at drain time, not at boot.
- **Invariants:** #1 (no net/crypto in tx) — preserved, RS-1 adds no tx; #2 (single-writer) —
  reconcile-once-under-mutex-before-loops; #9 (graceful shutdown) — single watch + join-all. None are
  weakened; #9 is newly *exercised* by real spawned tasks for the first time.
- **Containment:** RS-1 is additive at the binary edge. If the supervisor is wrong, the fallback is
  the current idle behavior — a feature-flag/config gate (`supervisor.enabled`, default off until
  proven) would let the binary ship M1-idle and flip to M3-supervisor by config. **Recommend gating
  Piece 5 behind such a flag for the pilot.**

---

## 6. Tests / checks required

- Per-piece targeted unit tests (above). Run `cargo test -p prro` (per memory `feedback_cargo_test_scope`),
  NOT full workspace, until pre-merge final.
- Piece 6 integration test is the gate: boot → reconcile-once → spawn → clean shutdown.
- `cargo clippy -p prro` — note pre-existing red under the 1.94.1 pin (memory
  `project_rust_gateway_clippy_debt`); diff against baseline before attributing a finding to RS-1.
- Live DPS smoke (W4-Z3 worktree) stays the authority for real-endpoint `connect` + `fn_sign` accept;
  RS-1 unit/integration tests use stubs.

---

## 7. Rollback / containment plan

- Each piece is an independent commit; Piece 5 is the only behavior-changing one at the binary edge.
- **Recommended:** gate the supervisor behind `supervisor.enabled` (default `false`) so a merged-but-
  unproven supervisor still ships as M1-idle; flip by config once the pilot DB + live channel are
  validated. Rollback = set the flag false (no code revert).
- Pieces 1-4 + 6 are inert without Piece 5 wiring → safe to land incrementally.

---

## 8. Open sub-decisions (need operator/owner sign-off)

1. **Route 1 vs Route 2 for the loader (CENTRAL).** Route 1 (loader does `extract_private_key` + new
   sanctioned `from_extracted` ctor, mirrors the live-proven port; no schema change) vs Route 2
   (route through `unseal_jks` → requires `operators` to carry `jks_password_hex` + `cred_salt`, a
   schema + seal-pipeline task). **Plan assumes Route 1.** Route 2 expands scope beyond "wire-only".
2. **Per-FN JKS path/password source.** Plan uses the existing `operators` secure table
   (`key_path` + `key_pass_enc`, via `build_from_db`) — confirm this is the intended source vs a
   config-file path. (Plan assumes secure operators table — it is the live `build_from_db` contract.)
3. **`fn_sign` signingTime at boot.** Built once at boot with `SystemTime::now()`. Confirm DPS accepts
   a boot-time `signingTime` for read RPCs over a long-lived process (live smoke re-signed per call).
   If not, `fn_sign` must be rebuilt periodically — a loop concern, not boot.
4. **DPS `connect` failure policy.** Hard-fail boot (recommended, fail-closed) vs degrade to
   no-channel idle.
5. **Drain-cadence config knob.** Does the drain ticker interval get its own
   `offline.drain_interval_seconds` (clamped, like the probe) or reuse the probe interval? Plan
   assumes a **separate clamped knob** for operational independence — add to Piece 1 if confirmed.
6. **Supervisor enable-flag for pilot** (see §7). Recommend yes.

---

## 9. What RS-1 explicitly does NOT do (boundary)

- **No ingress.** No HTTP/Maria/XML-RPC listener is bound. The `listeners` config (`config/mod.rs:29`)
  is **not** consumed by RS-1. That is **RS-2**.
- **No live write-path worker.** Nothing drives `stage_acquire` on fresh inbox rows. RS-1 spawns
  only the **recovery/maintenance** loops (reconcile-once, drain ticker, return-online probe). New
  receipts cannot be issued through the running binary after RS-1. That is **RS-3**.
- **No health endpoints.** `/health/*` + `/metrics` are not wired. That is **RS-4**.
- **No WebCheck shim.**
- **No schema/migration change** (under Route 1).
- **No new tx, no new state transitions.** RS-1 constructs deps + spawns existing, tested loops.

After RS-1 the binary: boots → builds per-FN deps → runs reconcile once under the global mutex → runs
drain + probe loops → joins cleanly on shutdown. **There is still no live ingress.**
