# RS-2 — ingress server + payload-conversion — implementation plan (external review)

- **Date:** 2026-06-05
- **Proposed branch:** `feat/rs2-ingress-server` off `rust-gateway` (RS-1 merged 2026-06-05, PR #108 → `4f5a935`).
- **Scope class:** hot-zone (runtime startup/shutdown + ingress + write-path seam). Frozen-invariant-adjacent: **#1** (no net/crypto in long SQLite write-tx — operator-emphasized for this worklet), **#2** (one FN = single-writer), **#4** (idempotency mandatory), **#9** (graceful shutdown), **#7** (schema_version on every envelope).
- **Origin:** operator directive 2026-06-05 "открывай план RS-2", after RS-1 (maintenance arm) merged. RS-2 wires the **ingress arm** of the runtime spine — the FRESH-receipt path that RS-1 deliberately left unwired.

> **Reviewer: read §3 (Central decision) + §2 (Investigation findings) FIRST.** Two facts reset expectations: (a) there is **no HTTP server in `rust/prro/src` today** — RS-2 stands up the first one; (b) the wire→canonical mapping already exists (W3, PR #99) but emits the **wrong payload shape** for the signer — closing that conversion gap is the substantive half of RS-2, and the ZReport arm is repository-touching, not a pure transform.

---

## 0. REVISION 2 — 2026-06-05 (external review; READ FIRST — SUPERSEDES §1/§3.3/§4/§5/§9/§10 where they conflict)

An external read-only architecture review found the v1 draft directionally right (inline response, inbox-before-seam, no long write-tx across sign/DPS, conversion = the real work) but **structurally under-grounded** in five places. **All findings verified against code; all confirmed.** This was the corrected basis as of Revision 2. **→ See §0.4 (REVISION 3) for the round-2 convergence corrections (1 external + 3 internal reviews); treat §0 (0.1 + 0.4) as the SOLE implementation basis.** §0.4 **D1/D2 RESOLVED (operator, both Option A — freeze slots + loopback-only)** → **PLAN-READY**.

### 0.1 Verified corrections (each finding code-anchored)

- **[High-1] No new `IngressCfg.listen` — bind per existing `ListenerCfg`.** `AppConfig.listeners: Vec<ListenerCfg>` already exists (`config/mod.rs:29`); each `ListenerCfg { kind: ListenerKind, port: u16, driver_id: String, fiscal_number: String }` (`:59-65`) serves **ONE `(driver_id, fn)` pair per port** (`ListenerKind::RestHttp` at `:79`); `tests/listener_config_parse.rs` pins "distinct FN per port". This is the W4-Z0 listener-stamped model that `to_canonical_fiscal_command_with_context(listener_driver_id, listener_fn)` was built to consume. **RS-2 filters `listeners` by `ListenerKind::RestHttp`, binds ONE axum server per matching listener, and stamps that listener's `(driver_id, fiscal_number)` into the mapping.** No parallel `ingress.listen`. (A global `ingress.enabled` master toggle is fine; the bind address + FN come from `listeners`.) The per-port-one-FN contract is **preserved**, not replaced — so no FN routing table is needed. (Aligns with [[feedback_operator_ua_fiscal_authority]]: listener-level/runtime-stamped before new config.)

- **[High-2] Replay is a state matrix, not "return the InboxRow".** `ingress_inbox::insert` returns the prior `InboxRow` carrying only `status` (`ingress_inbox.rs:101-112`); the fiscal outcome (`fiscal_id`/`document_state`) lives in `fiscal_documents`, written later. Inbox FSM: `NEW` → `PROCESSING` (CAS `acquire_lease` `:219`) → `DONE`/`REJECTED`/`ERROR`. **On `Replay`, branch on `status`:** `NEW`/`PROCESSING` (a request is still in-flight) → typed "in-progress, retry" (do NOT fake success; one register=one FN=one listener so same-key concurrency is rare but must be deterministic); `DONE` → **read `fiscal_documents`** for the terminal outcome (`ACK` vs `OFFLINE_LOCAL_ACK`) and build a truthful `CanonicalResponse` with the real `fiscal_id`; `REJECTED`/`ERROR` → typed error response; **drift** (terminal fiscal doc exists but inbox not `DONE`) → typed + audited. This matrix is a first-class RS-2 deliverable.

- **[High-3] Ingress command policy BEFORE inbox insert.** `to_canonical_fiscal_command` maps `X_REPORT`/`SERVICE_IN`/`SERVICE_OUT`/`CASH_WITHDRAWAL`/`SHIFT_CLOSE` to `DocType`, but `stage_sign::derive_wire_artifact_kind` (`stage_sign.rs:137`) signs only `ShiftOpen`/`Sell`/`Return`/`ShiftClose`/`ZReport` and **fails-closed** on `ServiceIn`/`ServiceOut`/`CashWithdrawal`/`XReport`. `X_REPORT` is **read-only — must not persist/sign/advance LND** (`docs/LEGAL_INVARIANTS.md:195`). **Add a command-class policy at the ingress boundary, before the fiscal inbox:** (a) **signable fiscal** {ShiftOpen, Sell, Return, ShiftClose, ZReport} → convert + inbox + seam; (b) **read-only/status** {XReport} → MUST NOT enter the fiscal inbox; serve via a read-only query path or return typed "read-only, not RS-2 fiscal scope"; (c) **typed-unsupported** {ServiceIn, ServiceOut, CashWithdrawal, PeriodicReport} → typed 422 before any inbox write. Mirror `derive_wire_artifact_kind` at the boundary so it fails **early**, not at signing time.

- **[High-4] `CanonicalResponse` DTO must change in §5.** Current `CanonicalResponse` (`dto.rs:80`) has **no `request_id`** and its `document_id`/`fiscal_id`/`fiscal_ts`/`document_state` are plain `String`. §5 must explicitly edit `dto.rs`: add `request_id`; define a **success-vs-typed-error envelope** (and whether pre-RS-3 `NotImplemented` returns an error envelope, NOT a success `CanonicalResponse`); define **optional `fiscal_id` semantics** (empty/None until fiscalized). For WebCheck `ReportZ`, the shim spec requires **raw Z-report XML in the response path** (`webcheck-shim-ingress-spec.md:147`) — couples to RS-3 (Z-XML built at sign time); see §0.2 Q4.

- **[High-5] Hash scope RESOLVED (no decide-later).** W3 hashes `cmd.payload` (wire shape) only (`dto.rs:310`). **Decision: persist the CONVERTED signer-ready `payload_json` and compute `payload_sha256_canonical` over the converted shape** — so the fiscal-document integrity hash matches the JSON the signer actually consumes (`parse_payload`). Idempotency/conflict detection then operates on the converted payload. Consequence (accepted): two wire payloads that converge to the same converted payload under the same `idem_key` are `Replay` — fine, they sign identically. **Raw-wire forensics** preserved by capturing the original `CanonicalCommand` in `audit_log` (correlation-linked), not in the inbox hash column. `convert.rs` therefore produces a fresh `(payload_json, payload_sha256_canonical)` pair, overwriting W3's wire-shape values before inbox insert.

- **[Medium-6] Test through a real validator, not the private `parse_payload`.** `stage_sign::parse_payload` is private (`stage_sign.rs:941`); the only test-support seam covers Check payloads, not Z/ShiftOpen (`:1098`). Add a narrow `#[cfg(any(test, feature = "test-support"))]` validator covering **all three** payload shapes, or assert via `stage_sign::run` fixtures.

- **[Medium-7] Protocol provenance via `driver_id`, NOT a new enum variant.** The `Protocol` enum actually has `Rest`/`XmlRpc`/`Maria`/`Maria304`/`CheckboxCompat`/`Internal` (`enums.rs:84` — v1 §2.7's "only Maria304+CheckboxCompat" was a grep artifact). A `RestHttp` listener → `Protocol::Rest`; **distinguish the WebCheck shim from `maria304_driver` by the listener-stamped `driver_id`**, not a new `Protocol::WebCheck` (zero migration; matches the runtime-stamped-over-schema-change preference). Drops v1 §10.1's "add WebCheck variant" recommendation.

- **[Medium-8] Payment `type_code` is repository-backed per-FN.** `payment_methods` (`payment_methods.rs`, W4-Z0 piece 3) holds per-FN `{pay_index, name, iscash}`; the wire `<M T>` = `pay_index - 1`. **`convert.rs` reads `payment_methods` for the listener's FN (READ, outside the write-tx) to map the wire payment → the signer's `type_code`/name** — do NOT hardcode `CASH`/`CASHLESS_1..3 → 0..3`. (Operator may risk-accept a hardcode for pilot if the pilot FN uses the standard pay-form set — §0.2 Q3.)

- **[Medium-9] Supervisor: named handle set, not "2 → 3 positional".** `supervise_until_shutdown` hard-codes exactly `(drain_handle, probe_handle)` with `Wake::LoopDied{which:"drain"|"probe"}` + a `which=="drain"` sibling-join (`supervisor.rs:216-281`, `join_both_with_grace:325`). With per-listener servers it's **N + 2** handles. **Refactor to a named supervised-task set** (`Vec<{name, JoinHandle}>`), generalizing the `select!`/loop-death/sibling-join logic. Specify: an axum task returning `Err(io::Error)` (bind/serve failure) → loop-death (CRITICAL `SUPERVISOR_LOOP_DIED` + `Err` → orchestrator restart); a graceful `with_graceful_shutdown` completion (`Ok(())`) after the watch flip → normal. **This touches the RS-1 F1 seam → re-review required.**

### 0.2 Operator decisions — RESOLVED (operator, 2026-06-05)

- **Q1 — Status endpoint: IN SCOPE, read-only auxiliary.** `GET /v1/status/{fn}` is in RS-2 as a **read-only auxiliary API** (NOT a POST-polling model — the inline POST contract §3.1 is unchanged). Without it WebCheck day-1 Initialization / GetCurrentStatus are artificial. **Minimal scope:** node mode + shift state, last local number / fiscal number where available, offline counters / code-pool where already readable. **Zero write-path side effects** (pure reads over `node_state` / shift / offline tables). The High-3 read-only `XReport` path does **not** route here unless trivially expressible as state — keep XReport typed-out of the fiscal inbox; the status endpoint is the operator/shim's read surface.
- **Q2 — Auth: conditional on bind address.** Default = **loopback bind (`127.0.0.1`) → no token**. **Any non-loopback / LAN bind → bearer MANDATORY** (not either/or — gated by the listener's bind address). The bearer is **auth only — it does NOT participate in routing or identity**; `(driver_id, fiscal_number)` are still stamped from `ListenerCfg`. Token supplied via **env / secret reference, never a literal in TOML**. → an auth middleware that (a) inspects the listener bind address, (b) requires `Authorization: Bearer` when non-loopback, (c) resolves the expected token from env/secret-ref.
- **Q3 — Payment mapping: repository-backed, no fallback.** `convert.rs` maps the wire `PaymentKind` to a **candidate `pay_index`** (`CASH → 1`, `CASHLESS_1 → 2`, `CASHLESS_2 → 3`, `CASHLESS_3 → 4`), then looks up `payment_methods` for the listener's FN: **`name` + validity come from the per-FN table**, `type_code = pay_index - 1`. **Missing / inactive payment method → typed conversion/config error, NO hardcode fallback.** (No pilot hardcode.)
- **Q4 — ReportZ response: passthrough field, born in RS-3.** The inline Z response must carry **raw Z XML** for WebCheck (shim §147). RS-2 **extends the response / seam outcome with an optional field (`report_xml` / `document_xml`) and passes it through**; the value is produced in **RS-3** at the sign/build-artifact boundary. Pre-RS-3 `NotImplemented` stays **non-2xx — no empty "success"**.

### 0.3 Corrected piece decomposition (supersedes §9)

1. **Command policy + listener-server binding model** — consume `AppConfig.listeners` (filter `RestHttp`), per-listener bind, `(driver_id, fn)` stamp; command-class policy (signable / read-only / typed-unsupported) at the boundary. **No `IngressCfg.listen`.**
2. **`convert.rs`** — wire → `CheckJson`/`ZReportJson`/`ShiftOpenJson`; `payment_methods`-backed payment mapping; ZReport ledger reads; **re-hash over converted** (High-5). Non-identity fixtures. *(Largest; mid-review.)*
3. **`seam.rs`** — `WritePathEntry` + `UnimplementedWritePath` (typed `NotImplemented`). Seam `FiscalOutcome` carries an **optional `report_xml`/`document_xml`** passthrough field (Q4; RS-2 leaves it `None`, RS-3 fills it at the build-artifact boundary).
4. **Response DTO + replay matrix** — edit `dto.rs::CanonicalResponse` (add `request_id`, typed-error envelope, optional `fiscal_id`, optional `report_xml` Q4); implement the §0.1 High-2 replay matrix (reads `fiscal_documents`).
5. **Per-listener axum servers + auth middleware + named-handle supervisor refactor** — `serve()` real router per `RestHttp` listener + `with_graceful_shutdown`; **auth middleware (Q2: loopback→none, non-loopback→bearer from env/secret-ref)**; generalize `supervise_until_shutdown` to a named task set (re-review F1 seam).
6. **`GET /v1/status/{fn}` read-only endpoint (Q1)** — node mode + shift state + last local/fiscal number + offline counters/pool, **pure reads, zero write-path side effects**; served on the same per-listener router.
7. **Tests** — convert units (non-identity, payment_methods-backed incl. missing/inactive→typed error, ZReport ledger), handler integration (Created/Replay-matrix/Conflict/command-policy/NotImplemented-not-silent), status-endpoint read-only assertions (incl. `{fn} == listener_fn` enforcement, CF1), auth/bind (loopback `RestHttp` accepted / **non-loopback `RestHttp` fails startup** — D2 loopback-only, no bearer day-1), lifecycle (graceful + axum-Err→loop-death), invariant (tx-released-before-seam), test-support payload validator (all 3 shapes), parity **invert** (M5).

**Supersedes:** §1 non-goals (status endpoint may be in-scope per Q1), §3.3 router (per-listener bind, not single-bind-all-FNs), §4 (IngressCfg→listeners; 2→3→named handle set), §5 (drop IngressCfg.listen row; add `dto.rs` + `convert.rs` payment_methods + command-policy rows), §9 decomposition, §10 (§10.1/§10.2 resolved; §10.4/§10.7 → §0.2 Q1; new §0.2 Q2-Q4). §2 (investigation), §6 (invariants), §7 (edges), §8 (test plan) remain valid and are extended by §0.1. **Correction to §2.7:** the `Protocol` enum is the full six-variant set above.

---

## 0.4 — REVISION 3 (2026-06-05, round-2 convergence: 1 external + 3 internal fresh-eyes reviews)

Round 2 ran four independent reviewers (operator's external + three internal: fiscal/repo, runtime/F1-seam, round-1-resolution/completeness). **Strong convergence:** all four confirm §0.1's round-1 resolutions are architecturally sound — the external + the internal Task-A re-verification both close High-1/High-3/High-4/High-5/Med-6/Med-7/Med-9. **No architecture rework.** Round 2 surfaced **6 High + 6 Medium** spec/mechanism gaps — two of them (Q3 payment, replay/OFFLINE_LOCAL_ACK) are **partial-resolutions** of round-1 items the external flagged as not-yet-closed. **Treat §0 (0.1 + 0.4) as the SOLE implementation basis — §1–§11 are historical and contain superseded statements (do not implement from them).**

### HIGH (block code)

- **H1 — Q3 payment mapping is fiscal-unsafe even with iscash-validation** *(external High + int-fiscal Med-B; external upgrades)*. `payment_methods` is per-FN **mutable** (`payment_methods.rs:116` updates pay_index/name/iscash; `repo_payment_methods.rs:116` reinserts active cash at another index); `iscash` is not a unique key; the wire `PaymentKind` carries no method name/id. The fixed `CASH→1` candidate is unsafe. **Resolution (operator → D1):** EITHER (A) **freeze `pay_index 1..4` slot semantics** — admin-restrict + config-validate the pilot FN's pay-form layout (cash=1, cashless_1=2, …) + a startup check that fails closed if the FN's `payment_methods` violates it + tests; OR (B) **carry a payment-method identifier in the wire/DTO** and look up by active `name`/`pay_index`. Regardless of A/B: add the `row.iscash == (kind == Cash)` mismatch → typed config-error arm.

- **H2 — Q2 auth not implementable from the current model** *(external High + int-runtime High-B, exact convergence)*. `ListenerCfg{kind,port,driver_id,fiscal_number}` (`config/mod.rs:59`) has **no bind/host field**; there is **no env/secret-ref resolver** in the crate (`rg env::var|std::env` = 0; secrets are TTY-prompt→secure-DB or TOML literal). **Resolution:** add an explicit **`listen_addr` field to `ListenerCfg` (default `127.0.0.1`)** + migrate `tests/listener_config_parse.rs`; add a **non-literal token reference** (resolver = new component or — preferred — **secure-DB-backed**, reusing the encrypted-at-rest precedent). **Startup fails closed when bind is non-loopback and the token ref is absent/unresolved.** **Operator → D2:** is the pilot ingress **loopback-only** (shim + gateway co-located → bearer deferred; only `listen_addr` default + fail-closed guard needed) or **LAN on day-1** (→ build the token resolver now)?

- **H3 — Replay matrix mishandles OFFLINE_LOCAL_ACK** *(external High + int-fiscal Med-C; external upgrades)*. Offline ack transitions the doc to `OFFLINE_LOCAL_ACK` and **terminates the pipeline** (`dispatch.rs:9`, `stage_offline_ack.rs:320`) **without** marking inbox `DONE` — `mark_done_tx` runs only in finalize after KVT2→ACK (`stage_finalize.rs:309`). The repo already treats `OFFLINE_LOCAL_ACK` as terminal-by-request-id (`fiscal_documents.rs:780`). So an offline-acked (complete) receipt sits at inbox `PROCESSING`. **Resolution:** replay is a **JOINT `(inbox_status, fiscal_doc_state)`** decision — on inbox `PROCESSING`, **probe `fiscal_documents` by `request_id` for terminal/client-terminal states (`OFFLINE_LOCAL_ACK`/`ACK`) before treating it as in-progress**. Do NOT mark inbox `DONE` at offline ack (don't perturb finalize/drain).

- **H4 — Supervisor cannot distinguish graceful axum `Ok(())` from premature `Err`/panic** *(int-runtime High-A)*. Today any handle completing before `shutdown` = loop death; `audit_loop_died` maps `Ok(())`→CRITICAL (`supervisor.rs:347`). An axum `with_graceful_shutdown` task returns `Ok(())` **normally** on watch-flip → biased-select race → mis-audit normal shutdown as `SUPERVISOR_LOOP_DIED`→`Err`→restart-storm across listeners. **Resolution:** tag each supervised task with an `expected_terminal` policy (`RunsUntilShutdown` for drain/probe vs `GracefulOkAfterShutdown` for axum) + capture **whether the watch was already flipped** when a handle completes; branch `(policy, res, watch_flipped)`: axum+`Ok`+flipped → normal (no audit); axum+`Err`/panic/`Ok`-before-flip → loop-death; drain/probe early completion → loop-death (unchanged). Bind failures surface at `serve()` startup → **fail boot**, not runtime loop-death. Pin with a piece-7 twin test (axum-Err→loop-death AND axum-Ok-after-flip→NOT audited).

- **H5 — `schema_version` (#7) missing from `CanonicalResponse` + status DTO** *(int-complete C-1)*. Both are outbound canonical envelopes; invariant #7 requires `schema_version`. §0.1 High-4 added only `request_id`. **Resolution:** add `schema_version` to `CanonicalResponse` and the `GET /v1/status/{fn}` DTO; broaden the §6 #7 check to the response path.

- **H6 — `uuid v4` feature not enabled → won't compile** *(int-complete C-2)*. `Cargo.toml:53` = `uuid` features `["v7","serde"]`. **Resolution:** mint **uuid v7** (already enabled; time-orderable → better for a DB-stored `request_id`; consistent with the no-determinism note), `Uuid::into_bytes() → [u8;16]`. (Supersedes §4/§10.3's "v4".)

### MEDIUM

- **M1 — the RS-3 inline seam must drive fresh `stage_acquire→stage_sign→dispatch/send` DIRECTLY, NOT via the App boot/drain entries** *(external M5 + int-runtime Med-C)*. The global `reconcile_mutex` is held across boot/drain recovery (`app.rs:490/613`); routing the fresh seam through those entries serializes unrelated FNs behind one slow FN. **Resolution:** §0/§4 state the seam calls the write-path stages directly (short DB envelopes + sign/DPS outside tx, `stage_sign.rs:7`/`stage_send.rs:878`), not `App::reconcile_pending_with`/`drain_*`. Names the multi-FN coupling honestly under #1/#9. (RS-3 owns it; RS-2 fixes the contract.)
- **M2 — shutdown of an in-flight inline request mid-DPS vs one-shared-grace** *(int-runtime Med-D)*. The inline DPS call MUST be bounded by a timeout **≤ grace** so the axum task resolves within the shared deadline; a request cut at grace-elapse is tied to the H3 replay matrix (inbox left `PROCESSING` → retry-after-restart replays deterministically), NOT a phantom-success or double-fiscalize.
- **M3 — Q1 status fields are NOT all "already readable"** *(external M4 + int-fiscal Low-A/D)*. `node_state::get` gives mode/shift_state/next_lnd (`node_state.rs:216`); offline session readable (`offline_sessions.rs:476/513`). But **code-pool counters + "last fiscal number" (`server_fiscal_no`/`offline_fiscal_no`, `fiscal_documents.rs:67`) lack public pool-bound helpers.** §0.3 piece-6 must **add** those read-only repo helpers explicitly (zero write-path impact), not imply a surface exists.
- **M4 — `operation_type` + `correlation_id` sourcing unspecified** *(int-complete C-3)*. `NewInboxEntry` is test-only-constructed today; the handler is the first prod constructor. §5 must specify: `operation_type` = the DocType wire string (e.g. "SELL"); `correlation_id` source (the wire `CanonicalCommand` has none → mint server-side or derive from `idempotency_key`).
- **M5 — invert the parity tests, don't "un-ignore"** *(int-complete C-4)*. Both `#[ignore]`'d tests in `tests/ingress_dto_parity.rs` end in unconditional `panic!`: `mapped_payload_json_is_wire_shape_not_stage_sign_ready` (:476) must be **inverted** to a positive parse-through assertion; `xreport_servicein_serviceout_cashwithdrawal_map_but_signer_will_reject` (:551) is the High-3 command-policy doc-test → invert in piece-7 to assert the **boundary** rejection.
- **M6 — command policy keys off `CommandType`, not `DocType`** *(int-complete C-5)*. `PeriodicReport` is a `CommandType`, not a `DocType` (rejected at the mapper before a DocType exists). State the policy classifies at the boundary on `CommandType`; the residual `DocType` set is exactly the 3 service-ops + XReport.

### LOW / doc-hygiene
Concrete body-limit (`axum::extract::DefaultBodyLimit` / `tower_http`); reconcile §4's `Replay→200` line with the H3 sub-matrix; `type_code` is a `String` (format `pay_index-1`); confirm the branch is cut off `rust-gateway` (RS-1's named-handle seam at `supervisor.rs:216` is the live Med-9/H4 dependency); the stale "Do NOT start coding until §0.2" line is corrected above. §1–§11 carry superseded statements (§2.7 protocol claim, §5 `IngressCfg` rows, §4 "2→3 handles" / "TcpListener on ingress.listen", §3.3 single-bind, §1 status non-goal) — **§0 is the sole implementation basis.**

### §0.4 decisions — RESOLVED (operator, 2026-06-05) → PLAN-READY
- **D1 — Q3 payment = (A) FREEZE SLOTS for pilot.** Fixed slot semantics `CASH=1, CASHLESS_1=2, CASHLESS_2=3, CASHLESS_3=4`; **`name` still comes from `payment_methods`**, but the slot→kind mapping is frozen. **Startup/config validation for every RS-2 `RestHttp` FN: fail-closed on a missing / inactive / `iscash`-mismatched protected slot.** **Admin must NOT be able to mutate protected slot semantics for RS-2-enabled FNs** (new admin guard — closes the "payment_methods is mutable" hole at the admin layer for RS-2 FNs). Wire-contract stays stable for pilot; carry-a-method-id (Option B) is post-pilot.
- **D2 — Q2 auth = (A) LOOPBACK-ONLY pilot.** Default `listen_addr = 127.0.0.1`, **no bearer day-1**. **Explicit fail-closed guard: if any RS-2 listener is configured non-loopback before the token-resolver lands, startup REFUSES.** LAN bearer / secure-DB token-resolver is post-pilot unless topology changes.

### Corrected piece-decomposition delta (supersedes §0.3 where they conflict)
- **Piece 1** gains: `ListenerCfg.listen_addr` (default `127.0.0.1`, H2) + command policy keyed on `CommandType` (M6).
- **Piece 2** gains: payment H1 resolution (per D1) + iscash-mismatch arm; **uuid-v7** request_id (H6).
- **Piece 4** gains: `CanonicalResponse.schema_version` (H5) + JOINT replay matrix (H3) + terminal-by-request_id pool-bound read (int-fiscal Low-D) + `operation_type`/`correlation_id` (M4).
- **Piece 5** gains: supervisor `expected_terminal`-policy discriminator (H4) + bind-failure→fail-boot + auth middleware (H2/D2) + seam-drives-stages-directly contract (M1) + DPS-timeout ≤ grace (M2) + body-limit.
- **Piece 6** gains: explicit code-pool / last-fiscal-number read helpers (M3) + `schema_version` on the status DTO (H5).
- **Piece 7** gains: invert BOTH parity tests (M5) + the H4 twin lifecycle test.

### §0.4 round-3 carry-forwards (2026-06-05 — convergence gate: external + internal both PLAN-READY, no new High)

Round 3 (1 external + 1 internal) both returned **PLAN-READY** — round-2 is faithfully folded, no architecture rework. Five implementation-placement details to pin so they are not lost during coding (none re-opens a blocker):

- **CF1 — status endpoint must enforce `listener_fn`, not just the path `{fn}`** *(external M7)*. `GET /v1/status/{fn}` runs on a per-listener router where the listener is bound to ONE `(driver_id, fn)`. Mirror POST's `to_canonical_fiscal_command_with_context` FN-validation (`dto.rs:355`): **reject `{fn} != listener_fn` → 403/404**, else the read-only API is cross-FN data access (acute under D2 loopback/no-token). → piece-6.
- **CF2 — D1 admin-guard is LIVE at the mutation surface, config-aware** *(external Med-2 + internal B1, converged)*. Enforce the protected-slot guard INSIDE `add_payment_method` / `update_payment_method` / `remove_payment_method` (`admin_w4_z0.rs:346/405/449`), NOT startup-only (the admin CLI opens pools directly, takes no singleton lock — `main.rs:500`, `singleton.rs:3`). Mechanism (internal B1): `with_pools!`→`open_pools_from_config` (`admin_w4_z0.rs:769/796`) **already parses the full `AppConfig` incl. `listeners` but drops it** — thread the derived **RS-2-enabled-FN set** (FNs with a `RestHttp` listener) through into the three fns. **Block** remove / inactivate / `iscash`-mutation of a protected slot when `fn_id` is RS-2-enabled; **allow** adding a missing protected slot only if it matches the required semantics. → piece-2 (+ admin guard).
- **CF3 — D1 startup-validation site = `supervisor::run`, alongside `build_from_db`** *(internal B2)*. `supervisor::run` (`supervisor.rs:45`) has `app.config()` (incl. `listeners`) + `app.db()` + `app.db_secure()` (→ `payment_methods`) in scope, and runs BEFORE `run_with_registry` binds anything — the exact precedent is `BindingsRegistry::build_from_db` (`:77-85`). Pin the per-RS-2-FN payment-slot validation there. → piece-1/5.
- **CF4 — D2 loopback guard semantics PINNED** *(internal B3 — the one real residual)*. `listen_addr` is **host-only** (port stays in `ListenerCfg.port`): (a) parse to `std::net::IpAddr` (NOT `SocketAddr`, NOT raw-string compare); (b) a hostname (`localhost`) does NOT parse as `IpAddr` → **fail-closed reject**; (c) classify via `IpAddr::is_loopback()` (true for `127.0.0.0/8` + `::1`); (d) `0.0.0.0` / `::` are `UNSPECIFIED` (all-interfaces) → **is_loopback()==false → REFUSED** (the footgun, correctly closed). Guard runs at the CF3 site, fail-closed before any bind. → piece-1.
- **CF5 — pick `correlation_id` source before piece-4** *(internal Task-A M4 note)*. The wire `CanonicalCommand` has no correlation field; choose: mint server-side uuid-v7, or leave `None` for pilot (idempotency is carried by `idempotency_key`, not `correlation_id`). → decide at piece-4. **Default: `None` for pilot** unless an audit-trace need surfaces.

Low doc-hygiene: §0.3's "non-loopback-bearer-required" test wording → under D2 the day-1 test is "a non-loopback `RestHttp` listener fails startup before the token-resolver lands"; the historical §4 "uuid v4" is superseded by H6 (uuid-v7) — §0 remains the sole basis.

**Decomposition delta (round-3):** piece-1 += `listen_addr` IpAddr-parse + `is_loopback` guard (CF4) at the CF3 site; piece-1/5 += per-RS-2-FN payment-slot startup validation (CF3); piece-2 += live admin-guard on the three payment mutation fns (CF2); piece-6 += status `{fn}==listener_fn` enforcement (CF1).

### §0.4 fresh-eyes carry-forwards (2026-06-05 — code review of committed piece-2a)

A fresh-eyes review of the committed RS-2 code returned REVISE; all closed in `convert.rs` (commit `f07e6a6`) EXCEPT one decision:

- **FE1 — `raw_frames` silent drop** → `ConvertError::RawFramesNotSupported` (fail-closed, M5-scope fiscal data; same posture as acquirer_slip). Closed.
- **FE2 — zero-quantity line / empty-goods SELL** → `ZeroQuantityLine` / `EmptyGoods`. Closed.
- **FE3 — maximal-item parse-through test** + `direction`-drop doc. Closed.
- **MED-2 — RESOLVED (operator, option (a)):** the H5 hash stays over the converted `payload_json`; `payment_methods.name` stays a runtime table lookup; a retry after an operator **renames** a payment slot may yield `Conflict` (accepted pilot risk — replay must NOT pretend a rename-changed doc is the same one; this is honester than freezing `name` against D1). **Piece-4 obligation:** the replay/response path MUST label such a `Conflict` as a possible **config-drift conflict**, NOT tampering, in the audit/log/error envelope, so an operator rename does not read as an attack.
- **EPZ follow-up (open):** `AcquirerSlip → EPZ` (PA/PB/…) attribute mapping + PA source (W4-Z1 §Q1) — operator decision, post-pilot-safe (RS-2 fails closed on slips today).

**Decomposition delta (fresh-eyes):** piece-4 += config-drift-vs-tampering `Conflict` labeling (MED-2a) in the replay matrix + response/audit envelope.

### §0.4 piece-4a review carry-forward (2026-06-06 — maria304_driver re-point obligation)

piece-4a made the gateway response `fiscal_id`/`fiscal_ts` `Option` (honest: offline-local-ack serialises `fiscal_id: null`, NOT a fake string).  Decision **B** (operator): do NOT widen the `maria304_driver` state machine inside RS-2, because it is **not a live `prro` consumer yet**.  **piece-4a is ACCEPTed for the WebCheck-pilot response route only**, NOT the maria304 live route.  **Obligation — before `maria304_driver` is re-pointed at `prro` (separate wiring, [[project-runtime-spine-gap]]):**
1. update its mirror `fiscal_id`/`fiscal_ts` → `Option<String>` (its current required `String` fails JSON decode on `fiscal_id: null` → `BridgeError::Transport`, before `classify_response`);
2. define a driver outcome for `document_state = "OFFLINE_LOCAL_ACK"` + `fiscal_id: null` (an accepted-offline state, NOT `SoftBlock`);
3. add an HTTP-decode test on `fiscal_id: null`;
4. add a dispatcher/classify test so the offline response resolves to accepted-offline, not `Transport(json decode)` or `SoftBlock`.
The pilot WebCheck shim (.NET, reads JSON) handles `null` natively, so nothing breaks today; the gateway keeps the honest `Option` contract.

### §0.4 piece-4b review carry-forward (2026-06-06 — piece-5/RS-3 obligations)

piece-4b (replay resolver, commit `849b16c`) ACCEPTed after fail-closed hardening (Ack-without-`server_fiscal_no` → `INBOX_LEDGER_DRIFT`; SELL/RETURN-missing-total → drift; `fiscal_ts` from `first_kvt1_at`, since `server_fiscal_date` is never written). Internal senior review surfaced obligations for **piece-5 / RS-3** (not piece-4b defects):
1. **InProgress → retryable HTTP.** piece-5 must map `ReplayResolution::InProgress` to a retryable status (202/425), and the WebCheck shim must LOOP on `error_code == "IN_PROGRESS"` (distinct from a hard failure), not treat it as terminal.
2. **Audit the Conflict hashes.** `ingress_inbox::insert` returns both `existing_payload_hash` + `submitted_payload_hash` on `Conflict`; piece-5 must AUDIT them (not only echo `config_drift=true`) so a genuine tampering divergence is forensically recoverable.
3. **`fiscal_ts` parity.** RS-3's `seam::FiscalOutcome.fiscal_ts` MUST also source `first_kvt1_at` (or the DPS stamp), so the FIRST-pass response and a later REPLAY response for the same receipt carry the SAME `fiscal_ts`.
4. **Q4 `report_xml` on a completed Z REPLAY** (external review Medium). `build_accepted` currently sets `report_xml: None` for every replay, so a retried `ReportZ`/`ShiftClose` of a *completed* Z would lose the raw Z XML the first-pass returned — breaking WebCheck `ReportZ` response idempotency. The XML is already persisted (`document_files`, `PAYLOAD_XML`/`SignedXml` at the sign boundary, `stage_sign.rs:431`). **Obligation:** when RS-3 lands, a completed `ZReport`/`ShiftClose` replay MUST read the stored Z XML from `document_files` and populate `report_xml` with the **same `DocumentFileKind` RS-3 emits in the first-pass `seam::FiscalOutcome.report_xml`** (parity), plus a "completed Z replay includes report_xml" test. Non-manifesting pre-RS-3 (the `NotImplemented` seam gates both first-pass and replay), so it is a tracked RS-3 coupling, not a live piece-4b gap.

---

## 0.6 — piece-2b design (ZReport / ShiftClose ledger aggregation) — PRE-IMPLEMENTATION

The conversion's second half: `Z_REPORT` and `SHIFT_CLOSE` both produce the signer's `ZReportJson { payments[]: {name, sum_in_kop, sum_out_kop, type_code}, sell_count, return_count }`. Unlike SELL/RETURN, this is **derived from the ledger**, not the wire — the wire `ZReport`/`ShiftClose` command carries no counters/sums. Operator-locked decisions (2026-06-05):

- **Shift boundary = `current_shift_id`**, NOT `business_ts >= opened_at`. The shift being closed is the FN's open shift: `node_state::get(main_pool, fn).current_shift_id`. Match ledger rows on `fiscal_documents.shift_id = current_shift_id`.
- **States = `ACK` + `OFFLINE_LOCAL_ACK` only** (the "issued receipts" ledger set; rejected/in-flight excluded).
- **Counts:** `sell_count` / `return_count` = number of `SELL` / `RETURN` docs in that shift (in those states).
- **Sums:** parse the **already-stored signer-ready `payload_json`** (the converted `CheckJson`) of each prior receipt, aggregate its `payments[]`; a `SELL` payment's `sum_kop` → that group's `sum_in_kop`, a `RETURN`'s → `sum_out_kop`; **group by `(type_code, name)`**. **Do NOT synthesize zero-valued payment rows.**
- **RS-3 caveat:** if RS-3 later returns the online receipt response at `SENT` (not after finalize/`ACK`), then a just-issued online doc may not yet be `ACK` at Z time → either ZReport must wait for pending online docs to finalize, OR this state-matrix is explicitly revisited. (Today nothing is wired; flagged for RS-3.)

**Planned implementation (de-risked):**
- **repo** (`fiscal_documents.rs`): `list_shift_issued_receipts(pool, fn, shift_id) -> Vec<(DocType, payload_json)>` — a **runtime** `sqlx::query_as` (DocType derives `sqlx::Type`→Decode, so no `.sqlx`/`sqlx prepare` cache needed): `WHERE fiscal_number=? AND shift_id=? AND doc_type IN ('SELL','RETURN') AND state IN ('ACK','OFFLINE_LOCAL_ACK') ORDER BY lnd`.
- **convert** (`convert.rs`): `aggregate_zreport(receipts: &[(DocType, String)]) -> Result<ZReportOut, ConvertError>` — pure: `BTreeMap<(type_code,name),(sum_in,sum_out)>` (deterministic payment order), checked sum adds, counts; parse each stored payload via a minimal non-`deny_unknown_fields` `StoredCheckPayments { payments: [{name, sum_kop, type_code}] }` (ignores `items`/extras).
- **orchestrator** (`convert_to_signer_payload`): the `ShiftClose | ZReport` arm reads `current_shift_id` (→ typed error if `None`), queries the ledger, calls `aggregate_zreport`, `finalize`s. Signature gains `main_pool` (ripple: piece-2a payment tests pass a main pool too).
- **tests:** pure `aggregate_zreport` units (group-by-(type_code,name); SELL→in / RETURN→out; no-zero-synthesis; multi-receipt; checked-overflow; malformed stored payload → typed error) + an integration test (seed `ACK` + `OFFLINE_LOCAL_ACK` SELL/RETURN docs in a shift + `node_state.current_shift_id` → ZReport convert asserts counts + grouped sums) + a parse-through of the produced `ZReportJson` via the test-support validator.

**Design-review outcome (fresh-eyes, 2026-06-05): DESIGN-OK.** piece-2b is a faithful Rust port of the operator-blessed Python `services/shift_aggregation.py` + W4-Z1 wire-shape — same `state IN (ACK, OFFLINE_LOCAL_ACK)` filter, same `shift_id` boundary, SELL→sum_in / RETURN→sum_out (positive). All eight scrutiny points verified correct against the code. Four recorded clarifications (none block implementation):

1. **In-flight docs are correctly EXCLUDED, not undercounted.** `{ACK, OFFLINE_LOCAL_ACK}` is exactly the "issued receipt" set (the `fiscal_documents`=issued-ledger pin + Python + W4-Z1 §NC). A doc still `Sending`/`Kvt1`/`ErrorRetryable` is not yet an issued receipt (may still be rejected) — including it would *over*-count. Filter is right.
2. **RS-3 OBLIGATION (tracked):** once RS-3 wires the online path and may return at `SENT` (pre-`ACK`), a fast Z after a slow in-flight online SELL would undercount. RS-3 MUST finalize/drain all pending shift docs **before** the Z aggregates (or the Z waits). Sequence point owned by RS-3, not piece-2b.
3. **DEPENDENCY (state it):** piece-2b's repo query + `aggregate_zreport` are unit/integration-correct + mergeable NOW, but **not end-to-end exercisable until RS-3 (drives `stage_acquire` → populates the ledger `shift_id`, `stage_acquire.rs:626`) + WL-1 (maintains `node_state.current_shift_id`)**. A live `0/0` Z before then is EXPECTED (same spine gap as the `NotImplemented` seam), NOT a piece-2b bug.
4. **Grouping `(type_code, name)`** is the operator decision (2026-06-05). Note: this **diverges from the Python golden, which groups by `type_code` only** — under D1 frozen slots `(type_code↔name)` is 1:1 so they coincide for the pilot FN; they differ only if two active methods share a `type_code` with distinct names (then `(type_code,name)` emits two `<M>` rows, more faithful; both DPS-legal). Do NOT "correct" it back to type_code-only; a Z-golden diff vs Python here is expected. **Shape note:** the stored payload parsed is the **converted** CheckJson (`{name, sum_kop, type_code}`, per §0.4 H5), NOT the Python wire shape; a malformed stored payload → typed error (deliberate, safer divergence from Python's silent skip).

**piece-2b code-review outcome (fresh-eyes, 2026-06-05): ACCEPT after 3 fail-closed hardenings (commit `0a9951d`).** SQL literal-vs-enum-rename verified correct (`'SELL'/'RETURN'/'ACK'/'OFFLINE_LOCAL_ACK'` match the `str_enum!` renames), aggregation/hash/determinism/tests all PASS, #1/#8 preserved. Closed: (High-1) `StoredCheckPayments.payments` made REQUIRED (was `#[serde(default)]` → a missing-`payments` stored payload silently underreported the Z; now a typed parse error); (High-2) negative stored `sum_kop` → typed `NegativeStoredPaymentSum` (ledger-corruption guard, no negative-turnover Z); (Low-1) `NoOpenShiftForZReport` message broadened.

**MEDIUM — RS-3 / handler sequencing obligation (piece-4/5 + RS-3, NOT a piece-2b code change):** Z aggregation happens INSIDE `convert_to_signer_payload` at convert-time. With this API shape, pieces 3-7 could call convert-for-Z BEFORE an RS-3 finalize/drain barrier, so RS-3 cannot enforce "finalize/drain pending shift docs before the Z aggregates" purely from the write-path. **Obligation:** before the handler wires Z through convert, EITHER (a) the handler imposes an explicit pre-convert quiescence/barrier for `ShiftClose`/`ZReport` (drain/finalize the shift's pending docs first), OR (b) the Z aggregation is invoked behind the write-path single-writer entrypoint (the seam) so the barrier is enforceable structurally. Decide at piece-4/5 (handler) + RS-3 wiring; pin in those pieces' design.

---

## 1. Goal & non-goals

**Goal.** Stand up the ingress HTTP server inside `supervisor::run` so that a fresh receipt from either front-end (`maria304_driver` today; the WebCheck COM-shim at pilot) flows: **HTTP POST `CanonicalCommand` JSON → FN-validate + driver_id stamp → wire→stage_sign-ready conversion → idempotent inbox insert (short RESERVED-tx) → write-path seam (inline) → typed `CanonicalResponse`**. The response is **inline-synchronous** (operator decision §3.1): the POST blocks until the receipt is fiscalized (or a typed pre-RS-3 not-yet-fiscalized) and carries `fiscal_id`/`document_state` when known.

**Non-goals (explicit boundaries):**
- **RS-3** — the real write-path front-half (`stage_acquire` → `stage_sign` → `dispatch_post_sign` → `stage_send` / offline-local-ack). RS-2 calls a **seam** that returns a typed `NotImplemented`/`NotYetFiscalized` until RS-3 lands. **No silent success** (operator-pinned).
- **`GET /v1/status/{fn}`** poll endpoint — deferred as an auxiliary recovery/status API, NOT the primary client contract (operator §3.1). Revisit only if the WebCheck shim's Initialization/GetCurrentStatus needs it on pilot day-1 (§10).
- The **WebCheck COM-shim** itself (separate Windows .NET deliverable, see `docs/architecture/2026-05-30-webcheck-shim-ingress-spec.md`) and **re-pointing `maria304_driver` at `prro`** (the driver was built against the Python gateway, no live `prro` egress ever proven — separate wiring task).
- Multi-FN routing logic beyond what the inbox + per-FN seam already enforce.

---

## 2. Investigation findings (the production ingress mechanism today)

All anchors verified against `rust/prro/src` at branch base `rust-gateway`/`4f5a935`.

1. **`IngressServer::serve()` is a literal no-op `{}`** (`runtime/ingress/mod.rs:22`). The module doc names the deferral: **W5** = "wires the actual axum router + handler that deserialises `dto::CanonicalCommand`"; **W7** = "plumbs the per-FN supervisor channel". RS-2 = W5 + the conversion layer; the per-FN drive is RS-3.

2. **W3 (PR #99) already landed the wire DTOs + mapping** (`runtime/ingress/dto.rs`): `CanonicalCommand` (`dto.rs:67`), `to_canonical_fiscal_command` (`:241`), and the **listener-stamped** `to_canonical_fiscal_command_with_context` (`:360`) which validates wire `fiscal_number == listener_fn` (`MappingError::FnConfigMismatch`) and stamps `driver_id`. `SCHEMA_VERSION = "1.0"` (`:65`), validated at `:244`.

3. **The conversion gap (the real work)** — `dto.rs:11-56`: `to_canonical_fiscal_command` sets `CanonicalFiscalCommand.payload_json` to the **driver-wire-shape** JSON of `ReceiptPayload` (`goods[].price_kopecks` / `quantity_milli` / `tax_group_1`; `payments[].type` enum). But `services/write_path/stage_sign.rs::parse_payload` (`stage_sign.rs:941`) is `#[serde(deny_unknown_fields)]` and expects a **different internal shape**:
   - `CheckJson { items[]: { code, name, price_kop, quantity_thousandths, sum_kop }, payments[]: { name, sum_kop, type_code } }` (`stage_sign.rs:805`) — SELL / RETURN.
   - `ZReportJson { payments[]: { name, sum_in_kop, sum_out_kop, type_code }, sell_count, return_count }` (`:920`) — Z_REPORT / SHIFT_CLOSE.
   - `ShiftOpenJson { opening_sum_kop }` (`:799`) — SHIFT_OPEN.
   Fields align **neither by name nor by data**. Critically, `ZReportJson.sell_count` / `return_count` **do not exist in the wire DTO** — they are derived from ledger rows since shift open, so the conversion is **repository-touching** (reads), not a pure transform. Until this lands, the first real receipt fails with `SignError::PayloadSchema` at `parse_payload`. A `#[ignore]`'d parity test documents the gap (`tests/ingress_dto_parity.rs::mapped_payload_json_is_wire_shape_not_stage_sign_ready`).

4. **Inbox repository** — `db/repositories/ingress_inbox.rs`: `insert(pool, &NewInboxEntry) -> InboxInsertOutcome::{Created, Replay, Conflict}` (`:65`). Idempotency is **already built in**: it probes `(fiscal_number, idempotency_key)` inside a RESERVED-locked `with_immediate` tx, returns `Replay` on a matching hash, `Conflict { existing_hash, submitted_hash }` on a hash mismatch. `NewInboxEntry` (`:44`) needs `request_id: [u8;16]`, `fiscal_number`, `protocol: Protocol`, `operation_type`, `idempotency_key`, `payload_json`, `payload_sha256_canonical: [u8;32]`, `correlation_id`.

5. **The Serve seam** — `main.rs:359` `Cmd::Serve`: when `supervisor.enabled`, calls `supervisor::run(app, shutdown)` which **consumes `app` by value** (`supervisor.rs:45`), connects DPS, builds the registry, then `run_with_registry` (`:105`). `run_with_registry` creates the shutdown watch `watch::channel(false)` (`supervisor.rs:172`), `tokio::spawn`s the drain + probe loops (`:422`, `:516`), and hands `(app, shutdown, shutdown_tx, drain_handle, probe_handle)` to `supervise_until_shutdown` (`:198`) which flips `shutdown_tx.send(true)` (`:244`) on shutdown and joins both loops under one grace (RS-1 F1). **This is where the ingress server plugs in** — spawned after the watch, sharing `shutdown_rx.clone()`, its `JoinHandle` threaded into `supervise_until_shutdown` so RS-1's loop-death supervision covers it.

6. **No HTTP server exists in `src` today.** Repo-wide grep for `axum::serve` / `Router::new` / `TcpListener::bind` returns **nothing**. The `ingress/mod.rs:5` doc says W1 (PR #96) "established the module namespace + axum/tower deps" — the dependencies are present but **no server is wired**. `admin_ui` is config-only (`AdminUiCfg.enabled/listen`) and is **also unserved**. RS-2 builds the first real axum server in the crate; there is no in-repo template to mirror (the admin_ui server is itself unbuilt).

7. **`Protocol` enum has only `Maria304` + `CheckboxCompat`** (`db/models/enums.rs:88-89`) — **no `WebCheck` variant**. The shim source must map to one of these or add a variant (schema-adjacent — see §10).

8. **`stage_acquire::run` (`stage_acquire.rs:48`) has zero production callers** (per WL-1 §0.1 + [[project-runtime-spine-gap]]). The entire write-path front-half is RS-3's build. RS-2 must NOT try to drive it directly — it calls the seam.

---

## 3. Central design decision

### 3.1 Response model — **inline-synchronous** (operator-chosen, 2026-06-05)

The POST blocks until fiscalization (sign → DPS-send **or** offline-local-ack) and returns a full `CanonicalResponse` with `fiscal_id` / `document_state`. **Rationale (operator):** for 1С/WebCheck, `FiscalReceipt` is a blocking COM call and `StatusBarXML.CheckID` must come from that operation's result. `enqueue + poll` breaks the contract and pushes synchronous-emulation complexity into the shim.

**Boundary (operator-specified):**
- **RS-2:** raise the ingress server in `supervisor::run`, wire→canonical conversion, idempotent inbox insert, call the write-path **seam**, return a typed response.
- **Pre-RS-3:** the seam returns a typed `NotYetFiscalized` / `NotImplemented` — **no silent success**.
- **RS-3:** fills the seam with the real front-half (`stage_acquire → sign → dispatch/send` or offline-local-ack).
- **Response:** `CanonicalResponse` carries `request_id` always; `fiscal_id` once known; `document_state`; and replay/conflict/error status.

### 3.2 Invariant #1 is the design's spine (operator-pinned)

**Inline ≠ holding a SQLite write transaction across crypto/network.** The inbox insert stays a **short RESERVED-tx** (it already is — `ingress_inbox::insert`). The ZReport conversion's ledger lookups are **READs** issued before/outside the write-tx. The seam's sign/DPS work (RS-3) happens **outside any long write-tx**, exactly as `boot_phase`/`backlog_drain` already do. A piece-6 invariant test asserts the inbox tx is released before the seam is entered.

### 3.3 Router — **Option A: one parameterized `POST /v1/ingress/{source}`** (recommended; flag to flip)

Both front-ends POST the **identical** `CanonicalCommand` JSON; `maria304_driver` already targets `/v1/ingress/maria304`, the WebCheck shim posts `/v1/ingress` (→ `/v1/ingress/webcheck`). One handler reads `{source}` for audit/protocol stamping; minimal duplication. (Option B = two named handlers; more explicit, duplicates the body. Operator did not object to A; final confirm in §10.)

---

## 4. Design (inline-synchronous, router A)

**Handler pipeline (single request):**
```
POST /v1/ingress/{source}  (axum)
  → deserialize CanonicalCommand (JSON; 400 on malformed / schema_version mismatch)
  → to_canonical_fiscal_command_with_context(cmd, listener_driver_id, listener_fn)
        ├─ FN mismatch        → 400 typed (MappingError::FnConfigMismatch)
        ├─ unsupported type   → 422 (UnsupportedCommandType, e.g. PeriodicReport)
        └─ cashier_id ""      → 400 typed (InvalidCashierId)
  → CONVERT wire ReceiptPayload → CheckJson | ZReportJson | ShiftOpenJson      ← the gap-closer
        (ZReport: ledger READ for sell_count/return_count — outside the write-tx)
        → overwrite canonical.payload_json with the stage_sign-ready JSON
        → recompute payload_sha256_canonical over the converted shape (see §7)
  → mint request_id = uuid v7 → [u8;16]   (§0.4 H6 — v7, NOT v4)
  → ingress_inbox::insert(NewInboxEntry{..protocol = source→Protocol..})   ← SHORT RESERVED-tx
        ├─ Conflict → 409 typed (idem_key reused with different payload)
        ├─ Replay   → 200 idempotent (return prior outcome; do NOT re-fiscalize)
        └─ Created  → continue
  → WritePathEntry::fiscalize(inbox_row)   ← SEAM (inline; RS-3 fills it)
        └─ pre-RS-3: Err(FiscalError::NotImplemented) → 501/503 typed not_yet_fiscalized
  → build CanonicalResponse{ok, request_id, document_id, fiscal_id, fiscal_ts,
        document_state, sale_total_kopecks, return_total_kopecks}
```

**Server lifecycle (in `run_with_registry`):**
- After the watch is created (`supervisor.rs:172`), if `config.ingress.enabled`, bind a `TcpListener` on `ingress.listen` and `tokio::spawn` the axum server with `.with_graceful_shutdown(async move { let mut rx = shutdown_rx.clone(); let _ = rx.changed().await; })`.
- Thread the server's `JoinHandle` into `supervise_until_shutdown` (extend it from 2 → 3 supervised handles) so an ingress-task panic is caught by RS-1's F1 path (CRITICAL audit + `Err` → orchestrator restart), and a normal shutdown joins it under the same one-grace deadline (#9).
- Shared state handed to the handler (axum `State`): cloned `SqlitePool`s (`app.db()`, `app.db_secure()` — `SqlitePool` is `Arc`-cheap), the `Arc<BindingsRegistry>` (for the RS-3 seam's per-FN DPS/sign), and the per-listener `driver_id` + `listener_fn`.

**The write-path seam:**
```rust
#[async_trait] trait WritePathEntry {
    async fn fiscalize(&self, row: &InboxRow) -> Result<FiscalOutcome, FiscalError>;
}
struct UnimplementedWritePath;          // RS-2 default
// fiscalize → Err(FiscalError::NotImplemented)   // typed, audited, NOT 200
```
RS-3 swaps `UnimplementedWritePath` for the real entry. The seam is the single integration point so RS-2 is independently mergeable + testable.

---

## 5. Per-component changes

| File | Change |
|---|---|
| `config/mod.rs` | Add `IngressCfg { enabled: bool, listen: String }` (mirror `SupervisorCfg` shape + a `listen` parse/validate); slot under the supervisor or top-level; clamp/validate tests. |
| `runtime/ingress/mod.rs` | Replace `serve()` no-op with `serve(listener, state, shutdown_rx)`: axum `Router` with `POST /v1/ingress/{source}` + `.with_graceful_shutdown`. |
| `runtime/ingress/handler.rs` *(new)* | The pipeline of §4 (deserialize → map_with_context → convert → mint id → inbox insert → seam → response). Typed error → HTTP status mapping. |
| `runtime/ingress/convert.rs` *(new)* | `ReceiptPayload → CheckJson \| ZReportJson \| ShiftOpenJson`. Pure for SELL/RETURN/SHIFT_OPEN; ZReport takes a read-only pool handle for `sell_count`/`return_count` ledger derivation. Closes the `dto.rs:11-56` gap. |
| `runtime/ingress/seam.rs` *(new)* | `WritePathEntry` trait + `UnimplementedWritePath` default + `FiscalOutcome` / `FiscalError` types. |
| `runtime/supervisor.rs` | Spawn the ingress server in `run_with_registry` (gated `ingress.enabled`); extend `supervise_until_shutdown` to a 3rd supervised handle. |
| `db/models/enums.rs` | *(maybe)* add `Protocol::WebCheck` — see §10. |
| `tests/ingress_dto_parity.rs` | Un-`#[ignore]` the gap test once `convert.rs` lands; assert converted `payload_json` parses through `stage_sign::parse_payload`. |
| `Cargo.toml` | Confirm `axum` / `tower` deps present (W1) + add `uuid` if absent. |

---

## 6. Invariant preservation

- **#1 (no net/crypto in long write-tx):** inbox insert is the only write and it is a short RESERVED-tx already; ZReport conversion lookups are READs issued outside it; the seam's sign/DPS runs outside any write-tx (RS-3, mirroring `boot_phase`/`backlog_drain`). Piece-6 test asserts tx-released-before-seam.
- **#2 (one FN = single-writer):** RS-2's inbox insert is **not** the writer — the per-FN single-writer lease is enforced inside the seam (RS-3). The ingress server is a single shared task across all FNs; it never writes `fiscal_documents` directly.
- **#4 (idempotency mandatory):** inbox `(fn, idem_key)` probe gives `Replay`/`Conflict` for free; `Replay` returns the prior outcome WITHOUT re-fiscalizing.
- **#7 (schema_version):** validated at `to_canonical_fiscal_command` (`dto.rs:244`); mismatch → typed 400.
- **#9 (graceful shutdown > fast):** ingress server shares the supervisor watch; `with_graceful_shutdown` drains in-flight requests within the configured grace; its `JoinHandle` is joined under the same one-grace deadline as the loops.

---

## 7. Edge cases

- **`payload_sha256_canonical` after conversion:** the inbox hash must be over the **converted** (stage_sign-ready) payload, not the wire shape — otherwise the persisted hash won't match what the signer consumes. Decide: recompute over `CheckJson`/`ZReportJson` in `convert.rs` (recommended) vs keep wire-hash for idempotency + a second signer-hash. **This is a correctness fork — call it out in review.**
- **ZReport with no derivable counters** (no open shift / no rows since shift open) → typed conversion error, not a `0/0` silent default.
- **`Replay`** (same `idem_key`, same payload) → return the prior `CanonicalResponse` (idempotent 200); never double-fiscalize.
- **`Conflict`** (same `idem_key`, different payload hash) → 409 typed; audit.
- **Pre-RS-3 seam** → typed `NotImplemented` mapped to a non-2xx (501/503) — **never a 200 with empty fiscal_id** (operator-pinned no-silent-success).
- **Shutdown mid-request** → graceful drain; requests arriving after the watch flip get connection refused / 503.
- **Offline mode** → the seam (RS-3) returns an offline-local-ack outcome; RS-2's response shape must already carry a `document_state` that expresses "locally accepted, not yet DPS-confirmed" (mirrors WebCheck's `mmmaaaccc` placeholder semantics).
- **Malformed JSON / oversized body** → 400 with a typed body; bounded request size.
- **Slow DPS under inline** (RS-3 concern, noted here): the handler must not block longer than the client's COM timeout → that latency ceiling is what trips auto-offline; RS-3 owns the timeout, RS-2 just must not add its own unbounded wait.

---

## 8. Test plan

- **Unit (`convert.rs`):** each DocType with **non-identity fixtures** (per [[feedback_type_system_reaching_fn_not_using]] — wire→canonical must be proven field-by-field, not by type-checking): SELL/RETURN → `CheckJson` (price_kopecks→price_kop, quantity_milli→quantity_thousandths, payment kind→type_code); SHIFT_OPEN → `ShiftOpenJson`; ZReport → `ZReportJson` with ledger-derived `sell_count`/`return_count` (seeded rows). Mapping-error arms (schema, FN, cashier "").
- **Handler integration (axum test harness):** `Created` → inbox row present + seam invoked + response shape; `Replay` → no second seam call; `Conflict` → 409; schema/FN/cashier errors → typed statuses; **`NotImplemented` seam → non-2xx, asserts NOT a silent 200**.
- **Server lifecycle:** binds + serves; graceful shutdown on watch flip drains an in-flight request; **F1 supervision covers the ingress task** (inject a panicking server handle → CRITICAL `SUPERVISOR_LOOP_DIED`-class audit + `Err`).
- **Invariant:** assert the inbox write-tx is committed/released **before** the seam is entered (no tx held across the seam boundary).
- **Parity:** un-`#[ignore]` the `dto` parity gap test — converted `payload_json` round-trips through `stage_sign::parse_payload` without `deny_unknown_fields` rejection.

---

## 9. Piece decomposition (small, reviewable)

1. **`IngressCfg`** — config struct + `listen` validation + clamp/default tests (mirror RS-1's `SupervisorCfg` piece).
2. **`convert.rs`** — wire→`CheckJson`/`ZReportJson`/`ShiftOpenJson`; ZReport ledger reads; non-identity unit fixtures. **Closes the `dto.rs:11-56` gap.** (Largest piece; mid-review here.)
3. **`seam.rs`** — `WritePathEntry` trait + `UnimplementedWritePath` (typed `NotImplemented`) + `FiscalOutcome`/`FiscalError`.
4. **`IngressServer::serve` + `handler.rs`** — axum router (`POST /v1/ingress/{source}`) + the §4 pipeline + typed error→status mapping + `with_graceful_shutdown`.
5. **Supervisor wiring** — spawn in `run_with_registry` (gated) + extend `supervise_until_shutdown` to 3 handles; shared `shutdown_rx` + state.
6. **Integration + lifecycle + invariant tests** (§8) + un-`#[ignore]` parity.
7. **Self-review + external review rounds** (hot-zone: expect 3–5 rounds per [[feedback_multi_round_external_review_pattern]]; mid-review after piece 2 (conversion) + before final per [[feedback_audit_review_cadence]]).

---

## 10. Open questions (external review + operator)

1. **`Protocol::WebCheck` variant vs reuse `Maria304`?** The shim emits the same canonical contract; reusing `Maria304` is zero-migration but loses audit provenance (you can't tell shim from driver in the inbox). Adding `WebCheck` is a new allowed enum string in `ingress_inbox.protocol` (schema-adjacent — confirm any CHECK constraint / migration). **Recommend: add `WebCheck`** for audit clarity. (operator call)
2. **`payload_sha256_canonical` scope after conversion** (§7) — recompute over the converted shape (recommended) vs dual-hash. Correctness fork.
3. **`request_id` mint** — uuid **v7** (§0.4 H6 supersedes the v4 note; idempotency is carried by `idem_key`, not `request_id`, so no determinism requirement — v7's time-ordering is a free win for the DB-stored id). Confirm.
4. **`GET /v1/status/{fn}`** — the WebCheck shim's Initialization/GetCurrentStatus may need a read-only status endpoint on pilot day-1 (`webcheck-shim-ingress-spec.md` §6). In RS-2 scope or RS-later? (operator call)
5. **`IngressCfg` gating** — separate `ingress.enabled` flag vs always-on when `supervisor.enabled`. Recommend a distinct flag (lets supervisor run maintenance-only without opening a listener).
6. **Router A vs B** — defaulting to A (§3.3); confirm.
7. **Health/readiness (RS-Q6)** — should `/health/ready` gate on the ingress listener being bound? (the health host is itself unbuilt — possible RS-2 sub-scope or RS-4.)

---

## 11. Risks

- **First real HTTP server in the crate** (§2.6): axum version / middleware / extractor patterns are unproven in this codebase — no in-repo template. Mitigate: thin router, standard `axum::serve` + `with_graceful_shutdown`, lean on the W1 deps.
- **Inline coupling to RS-3:** RS-2 cannot be proven end-to-end to a real `fiscal_id` until RS-3 lands. Mitigated by the typed seam + `NotImplemented` (RS-2 is mergeable, the pipeline up to the seam is fully tested, and no path silently 200s).
- **ZReport conversion correctness:** `sell_count`/`return_count` derivation from the ledger is the subtle part — wrong counters = wrong Z report. Non-identity fixtures + a real seeded-ledger test (§8).
- **Multi-FN ([[project_multi_fn_deployment]]):** one ingress server serves tens of FNs; the per-FN single-writer guarantee lives in the seam (RS-3), not the server. RS-2 must not assume a single FN anywhere (the listener carries one `listener_fn` per `ops/config.yaml` entry — confirm the multi-listener vs single-listener-multi-FN deployment shape with the W4-Z0 listener model).
- **`with_graceful_shutdown` + F1 supervision interaction:** the ingress task must terminate cleanly on watch-flip so the one-grace join doesn't detach it; test the shutdown path explicitly.

---

## Appendix — anchors

- Server seam: `main.rs:359`, `runtime/supervisor.rs:45/105/172/198/244`.
- DTO + mapping: `runtime/ingress/dto.rs:65/67/241/360`; gap doc `dto.rs:11-56`.
- Signer target shapes: `services/write_path/stage_sign.rs:799/805/920/941`.
- Inbox: `db/repositories/ingress_inbox.rs:44/65`.
- Protocol enum: `db/models/enums.rs:88-89`.
- Stub: `runtime/ingress/mod.rs:22`.
- Reconcile basis: `docs/architecture/2026-05-30-runtime-spine-connection-blueprint.md` (RS-2) + `docs/architecture/2026-05-30-webcheck-shim-ingress-spec.md` (O1 pilot ingress).
