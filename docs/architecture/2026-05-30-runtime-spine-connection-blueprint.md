# Runtime Spine — Connection Blueprint

**Date:** 2026-05-30 · **Branch:** `rust-gateway` (HEAD ~`a940520`) · **Status:** planning artifact, no code change
**Supersedes nothing.** Re-expresses the WL-0..6 critical path of
[`2026-05-29-pilot-integration-map.md`](./2026-05-29-pilot-integration-map.md) by **hoisting WL-1 §0.4 Piece 0a**
(the "live ingress→write-path worker" prerequisite) into a standalone foundation worklet: the **Runtime Spine (RS)**.

> **Why this doc exists.** Operator observation, twice: *"много кусков кода, не связаных между собой."* This is not
> an impression — it is the literal state of the binary. `prro serve` boots and idles (`main.rs:365`:
> *"M3+ adds the supervisor + ingress shells. M1 just idles"*). Every fiscal subsystem exists and is unit/integration
> tested, but **nothing in the deployable binary drives any of them**. The pilot's foundational gap is not WL-1
> (shift lifecycle) — it is the missing connective tissue underneath it: the **Runtime Spine**. Verified 2026-05-30
> by two code-grounded investigations (4-agent driver sweep + 6-agent subsystem inventory).

---

## §1 — The severed wire

```
            LIVE REQUEST PATH (what a real receipt must traverse)            status on HEAD
  ┌──────────────────────────────────────────────────────────────────┐
  │ maria304_driver  TCP listener → Maria-304 wire → build_canonical  │   ✅ wire path built,
  │   → HttpBridge POST /v1/ingress/maria304   (no `prro` dep)        │      error-path unit-tested (submit @ http_client.rs:61)
  └──────────────────────────────────────────────────────────────────┘
                              │  HTTP POST (CanonicalCommand JSON)
                              ▼
  ╳ SEAM 1  prro HTTP endpoint                IngressServer::serve(){}  ── EMPTY STUB (runtime/ingress/mod.rs:22)
                              │                                            axum deps present, 0 Router::new in src
                              ▼
  ╳ SEAM 2  DTO map + payload conversion      to_canonical_fiscal_command_with_context (dto.rs:360) ── 0 PROD CALLERS
            wire-shape payload ≠ stage_sign shape → SignError::PayloadSchema (dto.rs:37-43) ── CONVERSION LAYER ABSENT
                              ▼
  ╳ SEAM 3  inbox insert                       ingress_inbox::insert (ingress_inbox.rs:65) ── 0 PROD CALLERS
                              ▼
  ╳ SEAM 4  live write-path worker             stage_acquire::run (stage_acquire.rs:48) ── 0 PROD CALLERS (29 test sites)
            → stage_sign → dispatch_post_sign → stage_send                ── the worker DOES NOT EXIST
                              ▼
  fiscal_documents  PREPARED → SIGNED → SENT                              (stages WIRED — but only from boot/drain)

  ┌──────────────────────────────────────────────────────────────────┐
  │ Cmd::Serve (main.rs:359-369):  boot_from_path_or_exit → IDLE → drop │   ╳ ZERO spawned tasks
  │   never calls reconcile / drain / probe / ingress                  │      "M1 just idles" (main.rs:365)
  └──────────────────────────────────────────────────────────────────┘
```

The chain is severed at **four consecutive seams**, all inside `prro`, all because **`Serve` spawns nothing**.
The driver front-end and the write-path stages are both real; the spine that connects them is the empty middle.

> **Egress caveat.** `maria304_driver` was built to POST the **Python** gateway (`bridge/mod.rs:11` doc *"forwards to
> the Python gateway"*; `dto.rs:6` → `src/prro_gateway/adapters/maria304_native.py`). Its HTTP egress is wire-built
> and its connection-refused error path is unit-tested (`http_client.rs:127`), but **no live egress against a `prro`
> endpoint has ever been proven** — there is no `prro` endpoint to hit. Re-pointing the driver at `prro` (RS-2) is
> itself part of the wiring.

---

## §2 — Subsystem inventory (built + tested, but driver-orphaned)

`WIRED` = has a production caller · `boot/drain` = reachable only from boot reconcile or offline drain (neither of
which `Serve` invokes) · `STUB / 0-callers` = exists + tested, no production caller.

| # | Subsystem | Entry point (file:line) | Driver status on HEAD |
|---|-----------|-------------------------|------------------------|
| 1 | **DI root** `App` (Arc<Inner>: config, db, db_secure, singleton, reconcile_mutex, backoff_state) | `app.rs:135` / boot `app.rs:202` | **WIRED** — but `boot` only migrates+integrity-checks; spawns nothing |
| 2 | **maria304 ingress** (TCP→wire→canonical→HTTP POST) | `maria304_driver/src/bin/…:238` | **WIRED** end-to-end up to the HTTP POST (no `prro` dep) |
| 3 | **prro HTTP ingress** `IngressServer::serve()` | `runtime/ingress/mod.rs:22` | **STUB** — empty `{}`; W5/W7 TBD |
| 4 | **DTO mapper** `to_canonical_fiscal_command[_with_context]` | `runtime/ingress/dto.rs:241/360` | tested in isolation; **0 prod callers**; payload-conversion gap (dto.rs:37-43) |
| 5 | **inbox repo** insert / acquire_lease / mark_done_tx (status = raw TEXT, no enum) | `ingress_inbox.rs:65/213/347` | **0 prod callers** of `insert` (only tests) |
| 6 | **stage_acquire** (creates PREPARED row, INSERT @650) | `stage_acquire.rs:48` | **0 prod callers** (29 test sites) |
| 7 | **stage_sign / dispatch_post_sign / stage_send / stage_finalize** | `stage_sign.rs:193`, `dispatch.rs:126`, `stage_send.rs:911`, `stage_finalize.rs:234` | **boot/drain only** (`boot_phase:2486/2507/2511`, `backlog_drain:1192`) |
| 8 | **boot reconcile** `App::reconcile_pending_with` | `app.rs:477` | **0 prod callers** (Serve never calls) |
| 9 | **offline drain** `App::drain_offline_backlog_scheduled` → `_with` | `app.rs:654 → 606` | **0 prod callers** |
| 10 | **return-online probe** `App::spawn_return_online_probe` (only real interval loop, watch-shutdown-aware) | spawn `app.rs:768` / loop `return_online_probe.rs:444`, biased-select `:459-461` | **0 prod callers** |
| 11 | **DI bridge** `BindingsRegistry::build_from_db` (operators → per-FN `{dps, sign_ctx}`) | `runtime/bindings.rs:179` | **0 prod callers** (8 test sites) |
| 12 | **dep model** `RuntimeView{dps,signing_ctx,fn_sign}` / `ReconciliationRuntime` / `ReturnOnlineProbeDeps` | `runtime.rs:64/94`, `return_online_probe.rs:144` | types only; constructed by **tests only** |
| 13 | **shift FSM** 14-edge whitelist + `transition_state` | `shifts.rs:67/237` | only edges **5/6/13/14** reached (via drain, itself 0-caller); 1/2/3/4/7/8/9/10/11/12 = **0 prod caller** |
| 14 | **node_state mode** setters | `node_state.rs:177/208`, `return_online_probe.rs:362`, `admin.rs:214` | setters for `Blocked`/`StopMode`/`GoingOnline` exist — **NO Offline/GoingOffline setter anywhere** |
| 15 | **offline session** `open_session` / start_drain / close_session | `offline_session.rs:74/124/140` | **0 prod callers** → no OPEN session ever forms |
| 16 | **config** thin TOML `AppConfig` (`etc/prro.example.toml`) | `config/mod.rs:10` | parsed; **`listeners` field unused**; has only the probe-interval knob — no crypto/transport/shutdown-timeout/drain-cadence knobs |
| 17 | **health / metrics** `/health/*`, `/metrics` | — | **absent in Rust** (Python only); axum deps present, unused |
| 18 | **graceful shutdown** `await_shutdown_signal` + probe's `watch::Receiver<bool>` biased-select | `main.rs:318`, `return_online_probe.rs:459-461` | primitive **exists**; no `watch::channel` is ever created; signal only triggers `drop(app)` |

**Reading of the table:** the gateway is a complete, tested **fiscal library + a TCP front-end**, hosted by a
**boot-only shell**. The one thing absent at the binary level is a **composition root / supervisor** that constructs
the per-FN dependency bundle, spawns the loops, and wires ingress→inbox→worker. There is no `supervisor` /
`container` / `runtime_root` module — `runtime/mod.rs` has `{bindings, bootstrap, coding, ingress, outgress,
singleton, tax_snapshot}` and nothing that orchestrates them.

> **Scope of this table:** the *live request path* (maria304 → write-path). Deliberately out of frame because they
> are off the ingress→write-path spine: M4 outgress/FSCO-ZZD routing (`runtime/outgress`), `tax_snapshot`, the admin
> CLI, and XML-RPC/WebCheck ingress (config-scaffolded only — `ListenerKind::WebcheckXmlrpc` exists at
> `config/mod.rs:68`, but no Rust shell). The only real driver today is maria304.

---

## §3 — The missing connective layer = **Runtime Spine (RS)**

RS **is** WL-1 §0.4 Piece 0a, promoted to its own foundation worklet because every downstream WL depends on it.
It is **mostly wiring** of already-built, already-tested parts — **plus four genuinely-new components** flagged below
(the payload-conversion layer in RS-2, the production `DpsChannel` + key constructors in RS-1, the live-worker front
half in RS-3, and the `node_state` Offline setter for offline reachability) — but no new fiscal *logic*. Four build units:

### RS-1 · Composition root + supervisor (the single missing orchestration node)
- New `runtime/supervisor.rs`, called from `Cmd::Serve` (`main.rs:359-369`) **after** `boot_from_path_or_exit`,
  **replacing** the bare idle.
- Construct the per-FN dependency bundle once: `BindingsRegistry::build_from_db(app.db_secure(), app.db(), dps, loader)`
  (`bindings.rs:179`) yields `OperatorBindings{dps, sign_ctx}` (`bindings.rs:55-58`) per FN; bridge it to
  `ReconciliationRuntime::with_resolver` (`runtime.rs:121`). **Caveat (RS-Q3):** `RuntimeView` also requires
  `fn_sign: &CheckSignBlob` (`runtime.rs:67`), which `OperatorBindings` does **not** carry — a third per-FN loader for
  `CheckSignBlob` must be added, or the registry extended, before the resolver can return all three fields.
- Create one `tokio::sync::watch::channel(false)`; thread the `Receiver` into every spawned task; on
  `await_shutdown_signal()` (`main.rs:366`) flip the `Sender` and **join all handles** before `drop(app)`
  (preserves graceful-shutdown invariant #9). All tasks share the single `Arc<Inner>` App → serialize through the
  **global** `reconcile_mutex` (`app.rs:179`, `tokio::sync::Mutex<()>` = ADR-M3-A10 global-single-writer, which
  *implies* invariant #2's per-FN guarantee — it is not a per-FN mutex).
- **DPS channel is wire-only; the key loader is the one real build (RS-Q2, resolved).** `GrpcDpsChannel::connect(endpoint, timeout)`
  (`grpc.rs:62`) is a **production** constructor (server-trust TLS, **no mTLS client-cert** — DPS auth is app-layer CMS,
  proven live) → read the endpoint from config + `Arc::new(...) as Arc<dyn DpsChannel>`. The genuinely-new build is a
  **production `OperatorKeyLoader`** (all 4 impls are test fixtures) that calls the existing prod `unseal_jks`
  (`session.rs:130`) + `InProcessProvider::new` (`in_process.rs:24`) to assemble `SigningContext`, plus porting the
  **live-proven** `CheckSignBlob` builder (`sign_fn_blob`, W4-Z3 test) into `src/`. The code itself defers this as
  *"M5 crypto wiring"* (`bindings.rs:117`); then call the prod `build_from_db` (`bindings.rs:179`) at boot.

### RS-2 · Ingress server + payload conversion (seams 1–3)
- Give `IngressServer::serve()` (`runtime/ingress/mod.rs:22`) a real axum `Router` with `POST /v1/ingress/maria304`
  (the exact URL `maria304_driver` targets — `bridge/mod.rs:11`). Handler: bearer-auth → deserialize
  `dto::CanonicalCommand` → `to_canonical_fiscal_command_with_context` (`dto.rs:360`, listener-stamped driver_id +
  FN-mismatch reject) → reply with the `{ok,error_code,error_message}` body the driver's decoder expects
  (`http_client.rs:82-97`).
- **Build the payload-conversion layer** (`dto.rs:37-43` gap, full block `:11-56`): wire-shape `ReceiptPayload`
  (`goods[].price_kopecks/quantity_milli`) → stage_sign-ready `CheckJson` / `ZReportJson{sell_count,return_count}` /
  `ShiftOpenJson`. This is **repository-touching** (sell/return counts derived from the ledger), so it lives between
  the mapper and the worker. Without it, every real receipt dies at `SignError::PayloadSchema`.
- Build `NewInboxEntry` (`ingress_inbox.rs:44`) → `ingress_inbox::insert` (`:65`); map `Created`→enqueue,
  `Replay`→return prior result (idempotency #4), `Conflict`→reject.

### RS-3 · Live write-path worker (seam 4 — the deepest gap)
- New per-FN worker that, for one fresh inbox row keyed by `request_id:[u8;16]`, calls
  `stage_acquire::run(pool, pool_secure, driver_id, request_id, command)` (`stage_acquire.rs:48`) — **the online
  analogue of `boot_phase::dispatch_prepared_doc`, which today enters at stage_sign over *existing* PREPARED rows.**
- On `Proceed(ctx)`/`Resumed(ctx)` → `stage_sign::run(pool, &SigningContext, ctx)` → `dispatch_post_sign(pool, doc_id, fn)`;
  on `PostSignRoute::Online` → `stage_send::run(pool, &dyn DpsChannel, doc_id, Some(&signing_ctx))` → SENT.
  On `Offline`/`Refused` → terminate (do NOT call stage_send). The **post-sign arm** (dispatch→send→Offline/Refused
  terminate) mirrors `boot_phase.rs:2507-2549`; the **`stage_acquire`→`stage_sign` front half** (handling
  `WorkerProcessResult::{Proceed,Resumed,Noop,Rejected}`, which boot_phase never produces — it enters at stage_sign
  over existing rows) is **genuinely new** worker code. Both preserve invariant #1 (no network/crypto in a write tx).
- Bridge the synchronous driver POST: handler awaits the worker outcome → serialize `CanonicalResponse`
  (`dto.rs` ok/document_id/fiscal_id/document_state).

### RS-4 · Spawn the three orphaned loops
All three drivers exist + are tested; RS only constructs their deps + spawns them shutdown-safely.
- `app.reconcile_pending_with(deps)` (`app.rs:477`) **once at startup, before live traffic** (crash recovery under
  the mutex while the worker is quiesced).
- A drain ticker (`tokio::time::interval`) calling `app.drain_offline_backlog_scheduled(fn, &view)` (`app.rs:654`)
  per FN — the "M3+ runtime ticker" its own doc references; backoff gating already inside.
- `app.spawn_return_online_probe(deps, shutdown_rx)` (`app.rs:768`) — already returns a `JoinHandle`, already
  watch-aware.
- Add the **health/metrics host** (absent in Rust): axum router `/health/live|ready|startup` + `/metrics`, with a
  readiness flag set true after RS-4's reconcile pass completes (post-recovery readiness, matching the Python contract).

---

## §4 — Wiring order (dependency-correct)

```
WL-0  Foundation confirm + A/B decision  ───  investigation DONE (this blueprint + the 0a/inventory sweeps);
        gate NOT closed — operator decisions still owed: Q1 (Option A node_state-centric vs B full shifts-table) + Q3 + Q5
                                   │
                                   ▼
RS    RUNTIME SPINE  (= WL-1 Piece 0a, hoisted)  ◀── NEW FOUNDATION WORKLET, blocks everything below
        RS-1 supervisor/composition-root · RS-2 ingress+conversion · RS-3 live worker · RS-4 spawn loops + health
        UNBLOCKS the WL-1 work that closes Hard-Blocker(1) — does NOT close it alone   (ALGORITHMIC_MAP §1.11)
                                   │
                                   ▼
WL-1  Online shift lifecycle  (now reachable: pieces 0b..5)  ──► closes Hard-Blocker(1)
        0b insert_created_tx (NEW tx-variant of insert_created, shifts.rs:119) · 1 node_state mirror ·
        2 stage_send confirm edges 3/10 · 3 stage_acquire shift-create edges 1/8 ·
        4 offline edges 2/7/9 + W10a/W10b · 5 crash-recovery + Pattern-C e2e
        + Offline reachability (Q5): node_state Offline/GoingOffline setter (MISSING) + open_session prod caller (MISSING)
                                   │
                   ┌───────────────┴───────────────┐   (both depend on WL-1; can run partly in parallel)
                   ▼                                ▼
WL-2  Real-ingress cycle proof          WL-3  MAC internal-advance correctness ── PILOT-BLOCKER
      (maria304 receipt → DPS Ack)             (byte-exactness vs DPS echo; unproven live)
                   └───────────────┬───────────────┘
                                   ▼
WL-5  load/soak (Q-load: live vs mock)  ──►  WL-6  runbook + observability + matrix

WL-4  transient-reject taxonomy  ── ORTHOGONAL (Depends on: — ; can proceed in parallel now)
```

**Why RS comes first:** WL-1 pieces 0b..5 are *hooks at stage_acquire / stage_send / dispatch_post_sign*. None of
those stages has a live caller until RS-3 exists (WL-1 plan §0.1 line 20). WL-2 and WL-3 each need a real receipt
flowing, which needs RS-2. So RS is the literal prerequisite to every downstream worklet — it is the spine the
operator's "disconnected pieces" feeling is pointing at. (The RS→WL-1 ordering is the only *hard* serialization the
evidence forces; WL-2∥WL-3 are siblings off WL-1, and WL-4 has no prerequisite — per integration-map §4 deps.)

---

## §5 — What RS touches (hot-zones) + invariant guard

RS is **mostly wiring**, but it spawns concurrency over the write-path, so it lands in hot zones. Guards:

| Invariant | Where RS must honor it |
|-----------|------------------------|
| #1 no network/crypto in a write tx | RS-3's post-sign arm mirrors the boot-reconcile branch (`stage_sign` does crypto *outside* the tx; `stage_send` does the wire send between two short CAS txns). Do not collapse the ladder into one tx. |
| #2 single-writer (global, ADR-M3-A10) | every RS task shares `Arc<Inner>` → the **global** `reconcile_mutex` (`app.rs:179`, `tokio::sync::Mutex<()>` — serialises *every* dispatcher call; stronger than, and implies, per-FN #2). RS-4's reconcile pass completes (mutex held) before RS-3 admits live traffic. |
| #4 idempotency mandatory | RS-2 routes `insert` outcomes: `Replay`→return prior result, `Conflict`→reject. `acquire_lease` CAS (`ingress_inbox.rs:213`) protects against double-processing. |
| #9 graceful shutdown | single `watch::channel`; all `JoinHandle`s joined before `drop(app)`. The probe already models the biased-select contract — mirror it in the worker + drain ticker. |
| #2/#8 state-machine integrity | RS adds no new edges; it only *invokes* existing CAS helpers. The 14-edge whitelist + typed transitions stay the authority. |

---

## §6 — Open decisions (carry-forward + newly surfaced)

**From the integration map** (do not re-number): **Q1** Option A (node_state-centric) vs B (full shifts-table) —
gates WL-1; **Q3** per-shift-reset vs per-RRO-continuous `lnd`; **Q5** offline scaffolding fix inside WL-1 or
separate; **Q6** confirm scaffolding isn't a branch artifact; **Q-load** WL-5 live vs mock.

**Newly surfaced by this inventory (need a decision before RS code):**
- **RS-Q1 (response model):** the driver's `bridge.submit` is a *blocking* HTTP POST (`http_client.rs:61`). Does the
  handler run the write-path **inline** (POST→insert→acquire→sign→send→respond) or **enqueue + await** a worker? The
  blocking contract forces a synchronous reply either way; the channel/queue shape is unscaffolded.
- **RS-Q2 (DPS + key prod constructors) — RESOLVED (verified):** the DPS channel constructor **exists** and is reusable
  (`GrpcDpsChannel::connect`, `grpc.rs:62`; server-trust TLS, **no mTLS** — proven live), so the channel is *wire-only*.
  The real gap is a **production `OperatorKeyLoader`** (all 4 impls are tests; the prod loader is the code's own
  *"M5 crypto wiring"* deferral — `bindings.rs:117`), but its logic is **live-proven** in the W4-Z3 test
  (`load_signing_key`, `sign_fn_blob`) and only needs porting into `src/`. `build_from_db` (`bindings.rs:179`) is
  prod-ready, awaiting the boot caller (the W7 supervisor, `app.rs:149`). **No longer a hard blocker — RS-1 is wiring
  + one productionized loader, not a from-scratch transport/crypto build.** (For pilot scale a manual JKS-path loader
  suffices → answers ADR open-item O2 without provisioning automation.)
- **RS-Q3 (`fn_sign` source) — source identified:** `OperatorBindings` (`bindings.rs:55-58`) carries `{dps, sign_ctx}`
  only, no `CheckSignBlob` — so the production `OperatorKeyLoader` from RS-Q2 must **also** emit the per-FN
  `CheckSignBlob` (via the ported `sign_fn_blob`), or the resolver must source it alongside. Build it with the loader.
- **RS-Q4 (config home):** Rust `AppConfig` lacks crypto/transport/reconcile-toggle/shutdown-timeout/drain-cadence
  knobs the Python `ops/config*.yaml` has (Rust cannot parse that YAML). Decide config-vs-DB-resolved per knob.
- **RS-Q5 (request_id mint):** nothing in `src` mints `request_id:[u8;16]` for a live request (tests hard-code it) —
  the handler must mint it (UUID or idempotency-key hash); the canonical source is unspecified. *(The `Protocol`
  variant question is RESOLVED: `Protocol::Maria304` already exists — `enums.rs:88` — and `NewInboxEntry` already
  carries `protocol: Protocol`; no new variant needed.)*
- **RS-Q6 (readiness gating):** `/health/ready` should gate on the RS-4 reconcile pass, but no readiness flag exists
  on `App`/`Inner` — it must be added from scratch.

---

## §7 — What this blueprint does **not** change

- The write-path **stage library** (acquire/sign/dispatch/send/finalize) — correct + tested; RS only *calls* it.
- The **M3b 9-state shift machine** + 14-edge whitelist — authoritative; WL-1 supplies the missing *drivers*, not
  new edges.
- **Native crypto** — Hard-Blocker(2) is branch-resolved / HEAD-blocked (the live-ACCEPTED attached CAdES signer is
  on the unmerged `feat/m4-w4-z3` branch). RS assumes a working `SigningContext`; the crypto merge is a separate
  track (see ALGORITHMIC_MAP §1.11 + `project_native_crypto_dps_verify_blocker`).
- **WL-3 MAC internal-advance byte-exactness** — a distinct pilot-blocker, unproven live; orthogonal to RS.
- **The other NO-GO hard blockers** (ALGORITHMIC_MAP §1.11 lists **five**): RS+WL-1 close only HB(1). Out of RS scope
  and still gating the pilot NO-GO: **HB(3)** `PRRO_FISCAL_MODE` not harness-enforced (DF-5); **HB(4)** INV-05/INV-06
  channel-switch guards UNWIRED; **HB(5)** INV-09/INV-10 offline time/count limits + the 24h continuous-shift wall
  UNWIRED (CF-R4). Clearing the NO-GO needs all five tracks, not just the spine.

---

*Evidence: 4-agent driver sweep + 6-agent subsystem inventory + 3-agent adversarial verification + 4-agent RS-Q2
crypto/transport sweep, 2026-05-30, all file:line code-grounded on `rust-gateway` HEAD. Verify-before-build status:
**RS-Q2 RESOLVED** — DPS channel constructor reusable (`grpc.rs:62`), the prod `OperatorKeyLoader` is the one real
build (= the code's "M5 crypto wiring", live-proven in W4-Z3); **RS-Q3 (`fn_sign`)** source identified (the same
loader must emit `CheckSignBlob`); **`Protocol::Maria304` present** (`enums.rs:88`); **request_id mint (RS-Q5)
remains open**.*
