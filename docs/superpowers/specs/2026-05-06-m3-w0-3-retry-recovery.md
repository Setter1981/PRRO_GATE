# M3 W0-3 findings — retry / recovery policy

> **Status update 2026-05-07:** ADR-M3-A1..A9 were approved and committed in `8c72a14` (`docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md`).  Any `PROPOSED — NOT COMMITTED` wording below is historical research-time wording; canonical ADR status is the committed ADR block.
>
> **M3b update 2026-05-17 (PR #63 merged at `e04031b`):** Several contracts in this doc are OVERRIDDEN by `docs/superpowers/specs/2026-05-17-m3b-shift-state-expansion.md` §16 (Round 8 operational reality alignment) + §16.21 (Round 9 offline transition strategy):
> - **Retry budgets (§1.1 below)**: Round 7 had proposed 20-attempts/30min cap для Transport class. **M3b §16.4 reverses this: Transport budget UNBOUNDED** per operator-pinned decision ("ждём хоть год пока DPS не вернётся"). Hard rejects retain bounded budgets (0-3 attempts per error class).
> - **Recovery routing (§2 DpsError → retry policy table)**: M3b §16.3 introduces 5 new recovery classes (`AutoOfflineFallback` / `TechSupportEscalation` / `KeyRotationPending` / `MacReseedRecovery` / `TechSupportRepair`) — `EscalateManual` no longer the default for unknown DPS errors. Unknown / ambiguous **non-lifecycle** DPS errors (e.g. `SELL` / `RETURN` / `SERVICE_*`) → `AutoOfflineFallback` (auto-switch to OFFLINE + tech support notification, NOT Manual recon). **Ambiguous wire timeout after online `SHIFT_OPEN` / online `Z_REPORT`** (lifecycle docs that drive shift state edges 4 + 12) stays on the Manual / tech-support path per §16.7 — these are explicitly enumerated as real Manual recon triggers because the shift state machine cannot determine whether DPS accepted or rejected the lifecycle transition.
> - **DB-vs-log persistence (§16.1)**: failed DPS rejections + invalid ingress payloads → audit_log only, NOT `fiscal_documents`. Transport-class failures persist as `Sending` / `ErrorRetryable` for crash-recovery purposes only.
>
> Tables and per-status classifications below remain valid as M3a research baseline; for M3b runtime contracts, consult `2026-05-17-m3b-shift-state-expansion.md` §16 first.

**Status:** research findings, not yet ratified.  Closes nothing —
PRRO_GATE-6bj and PRRO_GATE-ah8 remain open until M3a implementation
lands the chosen contracts in code.

**Inputs:**

- `docs/M2-handoff.md` (§4.1 invariants #1, #4, #8; §1 W3 DPS substrate; §3 deferred bd issues)
- `docs/superpowers/plans/2026-05-06-m3-w0-research.md` (Task 3 acceptance)
- `docs/superpowers/specs/2026-05-06-m3-w0-1-state-sequence.md` (§2.1 DocState whitelist + failure-class × source-state matrix; landed at 18e2247)
- `docs/superpowers/specs/2026-05-06-m3-w0-2-lock-discipline.md` (§1 Python BEGIN IMMEDIATE audit; §1.3 aggregate verdict)
- `CLAUDE.md` (frozen invariants)
- `docs/Multi-Protocol_PRRO_Gateway.md` (technical spec)
- `rust/prro/src/transports/dps/error.rs` — 8-variant `DpsError` enum (lines 14–77)
- `rust/prro/src/transports/dps/channel.rs` — `DpsChannel` trait + `by_server_fiscal_no` default
- `rust/prro/src/transports/dps/grpc.rs` — `map_tonic_status` (lines 107–117) gRPC code routing
- `rust/prro/src/transports/dps/dto.rs` — `try_decode_check_response` / `try_decode_status_response` / `try_decode_rro_info_response` server-status routing (lines 161–289)
- `rust/prro/src/db/repositories/fiscal_documents.rs` — `allowed_transition` (lines 81–103); `list_pending_for_fn` (lines 172–221)
- `rust/prro/src/db/repositories/node_state.rs` — `upsert_initial` doc-comment (lines 9–28); `NodeStateRow` shape; `seed_prevhash`
- `rust/prro/src/db/models/enums.rs` — DocState (29–42), ShiftState (44–51), NodeMode (53–61)
- `rust/prro/src/app.rs:28` — M1 `App::boot` (pool + migrations only; deliberately skips `node_state` per the doc-comment at `:4-10` citing PRRO_GATE-ah8); M3a adds reconciliation phase **after** this existing boot boundary (§4.1)
- `rust/prro/src/runtime/` — current files: `singleton.rs` (PID-lock helper) + `mod.rs`; reconciliation worker site to be added by M3a
- `rust/prro/proto/fiscal_server.proto` — `CheckResponse.Status` enum values (lines 38–57)
- `src/prro_gateway/services/reconciliation.py` — Python recovery loop (`_apply_poll_result`, `reconcile_pending`, rate-limit cooldown)
- `src/prro_gateway/services/offline_sync.py` — six OFFLINE_LOCAL_ACK outcomes + retry self-loop (docstring lines 9–14; impl through line 605)
- `src/prro_gateway/services/write_path.py` — sign-retry resume path; rate-limited audit emission
- `src/prro_gateway/transports/dps_fiscal_server.py` — `_raise_classified_dps_error` (lines 507–550); status→canonical-error mapping (lines 560–586)
- `src/prro_gateway/runtime/container.py` — boot site (`runtime_initialized`, `_ops_tick` reconcile dispatch lines 282–294)
- `src/prro_gateway/runtime/supervisor.py` — `reconcile_on_startup` flag (line 34)
- `docs/webcheck_reverse/WEBCHECK_ANALYSIS.md`
- `docs/webcheck_reverse/WebCheckMain/WebCheck/All.cs` — retry constants (`Retries=2/3/9`, `Timing=3000/9000`, `RetriesBlock=108`, `SleepBlock=500`)
- `docs/webcheck_reverse/WebCheckMain/WebCheck/ClassFiscal.cs` — full/free version retry budgets (lines 188–199)
- `docs/webcheck_reverse/WebCheckMain/WebCheck/SubmitPtr.cs` — retry classification loop in `SubmitCheck` (lines 50–143)
- `docs/webcheck_reverse/WebCheckMain/WebCheck/StringXML.cs` — Z-report close branch wiring (line 2509)
- `docs/webcheck_reverse/WebCheckMain/WebCheck/FormTimer.cs` — Robot Server "9 раз" exhaustion log (line 662)
- bd issues: PRRO_GATE-6bj (retry/recovery policy), PRRO_GATE-ah8 (App::boot upsert behaviour)

**Out of scope:** state-machine enumeration (W0-1, landed at 18e2247),
lock discipline (W0-2), M3a implementation, ADR commits, offline
session lifecycle implementation (M3b), operator recovery UI/flows
(M3b), retry policy for non-DPS transports (Checkbox / sidecar /
Maria) — only DPS gRPC is in M3a scope.

---

## 1. WebCheck retry-class audit

This section enumerates the retry classes visible in the WebCheck
decompilation, then states which classes M3 Rust inherits verbatim,
which it extends, and which it rejects.  Every class cites the
exact source-line evidence so a reviewer can re-verify.

### 1.1 Global retry budgets

| Constant | Value | Source |
|---|---|---|
| `All.Retries` (default) | `2` | `WebCheckMain/WebCheck/All.cs:38` |
| `All.Retries` (full / paid version) | `3` | `WebCheckMain/WebCheck/ClassFiscal.cs:190` |
| `All.Retries` (free version) | `9` | `WebCheckMain/WebCheck/ClassFiscal.cs:196` |
| `All.Timing` (full) | `9000` ms (per-call gRPC deadline) | `WebCheckMain/WebCheck/All.cs:46`; `ClassFiscal.cs:191` |
| `All.Timing` (free) | `3000` ms | `WebCheckMain/WebCheck/All.cs:44`; `ClassFiscal.cs:197` |
| `RetriesBlock` (DB-blocked busy-wait) | `108` iterations | `WebCheckMain/WebCheck/All.cs:54` |
| `SleepBlock` | `500` ms | `WebCheckMain/WebCheck/All.cs:56` |
| Robot-Server failure ceiling ("9 раз") | `9` consecutive errors | `WebCheckMain/WebCheck/FormTimer.cs:662` |

**Reading:** WebCheck operates with a small bounded retry budget
(2/3/9) per submit attempt and a **constant per-call gRPC deadline**
(`All.Timing` is set once at boot, not adapted per call).  The
"free / 9 retries" budget compensates for the shorter 3-second
deadline; the "full / 3 retries" budget pairs with the longer
9-second deadline.  Total worst-case wall-clock is ~27 s in both
variants.

### 1.2 Per-status retry classification — `SubmitPtr.SubmitCheck` (lines 50–143)

The retry loop is a single `for (int i = 1; i <= retries; i++)`
block (`SubmitPtr.cs:51`) that branches per `answer.Status`:

| WebCheck status | Source line(s) | Class | WebCheck response | Notes |
|---|---|---|---|---|
| `Status == 1` (OK) | `SubmitPtr.cs:62-65` | terminal-success | `break` out of retry loop | success path |
| `Status == -3` (ERROR_SAVE) | `SubmitPtr.cs:66-76` | **transient retry, decremented separately** | `Thread.Sleep(333)`; decrement an inner counter `num` (default 7); audit "Сервер вернул ошибку -3 / Попытка повторной отправки"; `continue` | Note: `-3` retries up to `7` times **inside** the outer retry loop (so up to 7 × `Retries` total). Semantically: "DB-write transient on server side". |
| `Status == -15` (ERROR_NOT_OPEN_SHIFT) AND `OpenCloseShift` | `SubmitPtr.cs:77-90` | reconciliation via lastChk | `Thread.Sleep(333)`; call `LastCheckAllInfa()`; if local `MaxID + 1 == returnDI`, treat as success (server already accepted); else `continue` | DPS sometimes accepts a shift-open then forgets state; reconcile via `lastChk`. |
| `Status == -16` (ERROR_OFFLINE_ID) | `SubmitPtr.cs:91-100` | offline-technical recovery | If local `CHECKHEAD` rows present OR `OfflineOnTechno()` succeeds → `continue` (server will eventually accept); else surface `errCode=97` "Аварійне онулення даних ПРРО. Виконано технічне включення оффлайн режиму." | Effectively: "we hold offline IDs DPS doesn't recognise; switch to offline pool reconciliation." |
| `Status == -2` (ERROR_CHECK) AND `OpenCloseShift` | `SubmitPtr.cs:103-141` | reconciliation via lastChk + technical reopen | If error message indicates "open shift" + no local SHIFT row + open-shift attempt fails → `Thread.Sleep(333)` + reconcile via `LastCheckAllInfa`; if no match → surface `errCode=95` "Аварійне онулення даних ПРРО. Виконано технічне відкриття зміни. Підсумки за зміну онулені." | Edge case in close-shift path. |
| `Status == 0` (UNKNOWN / proto-default) | `SubmitPtr.cs:105-117` | reconciliation via lastChk | `Thread.Sleep(333)`; reconcile via `LastCheckAllInfa`; if local `MaxID + 1 == returnDI` → treat as success | Proto-default `0` means the field was missing on the wire; treated as "DPS may have accepted, ask `lastChk`". |
| `Status < -1` (any other negative status) | `SubmitPtr.cs:118-121` | terminal-rejection | `break` out of retry loop without retry | Most ERROR_* statuses are terminal in WebCheck. |
| Generic exception | `SubmitPtr.cs:145-153` | terminal-error (no retry) | `errCode=25 "Помилка надсилання чека для фіскалізації"`; surface to caller | Includes gRPC transport / TLS / DNS — note: WebCheck does **not** retry on raw transport exception; only on typed `Status == -3`. |
| Post-loop `Status ∈ {0, -1}` AND offline allowed | `SubmitPtr.cs:155-194` | offline failover | `OfflineOn()` triggered; `errCode=32 "Включен офлайн режим"` | After retry budget exhausted on transient classes, switch the whole node to offline pool. |

**Pre-call DB-blocked busy-wait** (`ClassFiscal.cs:408-420`,
`:1027-1037`): before any submit, WebCheck spin-waits on
`bl.ini Block != 0` for up to `108 × 500ms = 54s`, then surrenders
with `errCode=1015 "База заблокированна, ведется отправка офлайн
чеков."` This is **not** a DPS retry; it is a local-DB lock
contention against the offline-batch sender.  Out of scope for M3a
(no concurrent offline batch in the ONLINE-only pipeline) but
documented for completeness.

**Recovery via `LastCheck` itself** (`SubmitPtr.cs:225-264`): the
`lastChk` call also has its own `for (i = 1; i <= retries; i++)`
loop with the same `All.Retries` budget; it only `break`s on
`Status == 1` or `Status < -1` (line 249).  Same retry budget,
same per-call deadline.

### 1.3 Robot-Server retry exhaustion (`FormTimer.cs:653-666`)

The async background submitter ("Robot Server") tracks `EndT`
counter — decrements on each negative `num2`; logs "Ошибка
повторилась более 9 раз" (line 662) and disposes the timer.
Distinct-from-submit semantics: it is the **outer-cycle** ceiling
for repeated failed reconciliation attempts of the same document,
**not** the per-submit retry budget.  Approximate analogue: M3
`max_recovery_attempts` ceiling that escalates a doc to
REQUIRES_MANUAL_RECONCILIATION — Python today already implements
this with the same numeric ceiling default `5` at
`reconciliation.py:44` (see §3 below).

### 1.4 Aggregate verdict — what M3 inherits, extends, rejects

**Inherited verbatim (binding for M3a):**

1. **Status `-3` (ERROR_SAVE) is retry-class.**  This is the only
   negative DPS status that WebCheck retries inline.  M3 `Server { code: -3, .. }` MUST route to ERROR_RETRYABLE with bounded
   attempts + backoff, NOT to terminal Rejected.
   Cite `SubmitPtr.cs:66-76`.
2. **Status `0` (UNKNOWN / proto-default) is reconciliation-class
   via `lastChk`.**  M3 `DpsError::Decode` for the proto-default
   case (already routed to `Decode` at
   `dto.rs:175,215,275`) MUST trigger a `last_chk` reconciliation
   probe before deciding terminal vs retry.  Cite
   `SubmitPtr.cs:105-117`.
3. **Status `-15` (ERROR_NOT_OPEN_SHIFT) on close-shift /
   z-report path is reconciliation-class via `lastChk`.**  M3
   `Server { code: -15, .. }` on a doc whose `doc_type ∈ {SHIFT_CLOSE, Z_REPORT}` MUST trigger `last_chk` to
   distinguish "DPS forgot the open" (recover) from "we genuinely
   never opened" (escalate).  Cite `SubmitPtr.cs:77-90`.
4. **Bounded retry budget per submit attempt is small (2–9).**  M3
   default 5 (matches Python `max_recovery_attempts=5` at
   `reconciliation.py:44`) is within this band; do NOT inflate to
   double-digit values without an ADR.
5. **Per-call gRPC deadline is constant + short.** WebCheck uses
   3000 / 9000 ms.  M3 `GrpcDpsChannel.request_timeout`
   (`grpc.rs:51`) MUST be configured per-FN at `connect`-time, not
   recomputed per call — matches the WebCheck idiom and the W3
   `request<T>` helper contract that calls
   `req.set_timeout(self.request_timeout)`.

**Extended (M3 goes beyond WebCheck):**

6. **Generic transport exception is retry-class with a bounded
   budget**, NOT terminal.  WebCheck's `catch (Exception)` at
   `SubmitPtr.cs:145-153` immediately surfaces `errCode=25` and
   does not retry; M3 `DpsError::Transport` will retry with
   bounded attempts + exponential backoff.  Rationale: WebCheck is
   a desktop GUI app where the operator can press "retry" on the
   form; M3 is a server-side daemon where automated retry is the
   correct response to transient TCP / TLS / gRPC `Unavailable`.
   Cite Python precedent at `dps_fiscal_server.py:241-242`
   (`raise TransportRetryableError`) and W3 review finding D3 in
   `dto.rs:184-188` (`ErrorUnknown` → `Transport` / retry-class).
7. **Rate-limit cooldown is explicit, persisted, and longer than
   submit retries.**  Python today implements
   `_DEFAULT_RATE_LIMIT_COOLDOWN_SECONDS = 300` (5 min) at
   `reconciliation.py:335`, with override via
   `response_json['retry_after_seconds']`.  WebCheck has no
   equivalent.  M3 inherits the Python contract; see §2 row
   "rate-limited" mapping.
8. **App::boot reconciliation is automatic, not operator-driven.**
   WebCheck reconciles on user click ("Robot Server" GUI control);
   M3 reconciles every pending doc on process start before
   accepting new ingress.  See §4.

**Rejected (M3 does NOT inherit):**

9. **DB-busy-wait of `108 × 500ms`** (`ClassFiscal.cs:408-420`).
   M3 has a single-writer-per-FN lease + `with_immediate` lock;
   contention is in-process and bounded by tx-commit time.  Not
   applicable.
10. **Status `-15` on non-close-shift paths reconciles silently.**
    WebCheck only triggers `lastChk` reconciliation on `-15` when
    `OpenCloseShift==true`.  M3 will treat `-15` on any
    non-shift-mgmt op as a real "no open shift" — terminal
    REJECTED + audit, because the M3 ingress guard
    (`write_path` stage 1) is supposed to reject pre-submit if
    there is no OPEN shift; reaching DPS with `-15` means our
    state is wrong, not theirs.

---

## 2. DpsError → retry policy table

**Pattern B source-state note (per ADR-M3-A5 / ADR-M3-A9).**  Under
Pattern B, the immediate stage-4-4b CAS source state is
`Sending` (NOT `Sent`).  The "Source-state implications" column
below preserves the original `Sent → ...` references because
they are also reachable via the **reconciliation path** —
post-Sent docs whose `last_chk` returns a terminal status
transition `Sent → ErrorRetryable / Rejected` per the existing
M2 whitelist.  For the immediate stage-4-4b path the equivalent
transitions are `Sending → ErrorRetryable / Rejected / Sent / Kvt1`
per ADR-M3-A9 step 3.  Both source states are valid for the
same DpsError variant — the difference is *when* the error is
observed (stage 4b live vs reconciliation poll).  M3a impl maps
the variant to either source state based on the live-vs-poll
context.

**8 REAL DpsError variants from `rust/prro/src/transports/dps/error.rs:14`**:

| Variant | Retry policy | Max attempts | Backoff curve | Dead-letter destination | Source-state implications (per W0-1 §2.1 whitelist) |
|---|---|---|---|---|---|
| `Transport(String)` | retry transient | `max_recovery_attempts=5` (default; mirror Python `reconciliation.py:44`) | exponential capped: `min(30, 1 * 2^(attempt-1))` seconds (1, 2, 4, 8, 16, 30, 30, …) — proposed default; tunable | After `recovery_attempts >= 5`: transition to `REQUIRES_MANUAL_RECONCILIATION` + audit `OFFLINE_SYNC_FAILED_TERMINAL`-style event | Live stage-4-4b: `Sending → ErrorRetryable` (per ADR-M3-A9 step 3) on transient transport failure with known wire reply.  Recovery loop under Pattern B drives `ErrorRetryable → Sending → wire → Sending → Sent / Kvt1 / Rejected / ErrorRetryable` (per ADR-M3-A9 retry-path policy).  The legacy `ErrorRetryable → Sent` / `→ Kvt1` whitelist entries (`:99` / `:100`) remain in the whitelist for non-DPS backward compat but **MUST NOT be invoked by M3a DPS code** — using them for wire send re-introduces the duplicate-send hazard Pattern B exists to prevent.  `ErrorRetryable → Kvt1` is permitted only for direct `last_chk` re-query paths that bypass wire send |
| `Authorization { code, kind, message }` (after the M2/W3 amendment below — see **NB**) | NO retry (terminal-class) | 0 (never auto-retry) | n/a | Routing differs by `kind`: per-doc reject (kind=DocumentReject for `ERROR_VEREFY` -1) → `Rejected`; FN-config error (kind=FiscalNumberNotRegistered for -13 / -14) → `RequiresManualReconciliation` via ErrorRetryable→RequiresManualReconciliation chain (whitelist `:101`), because the FN itself is in a bad state, not the document.  Both paths emit ERROR severity audit; operator must rotate creds (-13/-14) or inspect the rejected doc (-1) | Live stage-4-4b: `Sending → Rejected` (per ADR-M3-A9 step 3) for kind=DocumentReject; `Sending → ErrorRetryable → RequiresManualReconciliation` for kind=FiscalNumberNotRegistered.  Reconciliation poll: `Sent → Rejected` (whitelist `:94`) for kind=DocumentReject; same chain for kind=FiscalNumberNotRegistered |

**NB — DpsError::Authorization variant amendment is a M2/W3
prerequisite for this row's routing.**  As shipped in M2 the
variant is `Authorization(String)` (`error.rs:27`) and the
decoder at `dto.rs:178-184` lumps `-1`, `-13`, `-14` into a
single `Authorization(format!(…))` instance.  The string
message is operator-readable but NOT a stable routing key —
W3 deliberately abstracted away the wire status code at the
`Authorization` boundary.

The differentiation `-1 → per-doc reject` vs `-13 / -14 →
config error` is real (operator response is different — rotate
the doc vs rotate creds) but it cannot be implemented from the
existing `Authorization(String)` shape without string-pattern
parsing of the message body, which is brittle (server message
text is not a contract).

**Required additive M2/W3 amendment (PRE-REQUISITE for M3a
impl of this row):**

```rust
// rust/prro/src/transports/dps/error.rs
#[derive(Debug, Error)]
pub enum DpsError {
    // …
    #[error("DPS authorization {kind:?} (code={code}): {message}")]
    Authorization {
        code: i32,
        kind: AuthorizationKind,
        message: String,
    },
    // …
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationKind {
    /// `-1` ERROR_VEREFY: signature/crypto rejected at server.
    /// Per-document failure; the doc is rejected, the FN is fine.
    DocumentReject,
    /// `-13` ERROR_NOT_REGISTERED_RRO / `-14`
    /// ERROR_NOT_REGISTERED_SIGNER: FN config issue; rotating
    /// creds or re-registering with DPS resolves it.  Per-FN
    /// failure; the doc is held for operator inspection, not
    /// rejected.
    FiscalNumberNotRegistered,
}
```

Decoder at `dto.rs:178-184` updates to populate the new
fields based on the raw status code already in scope.

This is an **additive amendment** — no existing public
contract is removed; the variant gets richer fields.  All
M2 W3 callers using string-pattern routing (none today) keep
working.  The amendment lands as part of M3a impl prep,
**before** the §3 SIGNED / SENDING recovery rules are
exercised end-to-end.

If the amendment is rejected: M3a falls back to the
"single safe destination" policy — route ALL Authorization
to `RequiresManualReconciliation` (the conservative bucket).
This loses the per-doc-reject vs FN-config-error
distinction; operator inspects every Authorization
manually, increasing operational load.  Document the choice
in the implementation PR.
| `Decode(String)` | one `last_chk` reconciliation probe; no retry loop | 1 reconciliation probe (single shot, not retried) | n/a | If `last_chk` confirms doc reached server (server `id` matches expected) → drive forward to `Kvt1` / `Ack`; if `last_chk` returns `NotFound` OR a non-matching id → `REQUIRES_MANUAL_RECONCILIATION` + audit `DPS_DECODE_FAILED`. **No bounded retry on Decode** — a server-contract drift is operator-visible, not auto-recoverable.  Decode failures are logged at WARN with the raw decoder error message for protocol-drift forensics | WebCheck inheritance: status `0` (proto-default UNKNOWN) decoded at `dto.rs:175-177` is reconciliation-class per `SubmitPtr.cs:105-117`.  Source state at probe time depends on context: live stage-4-4b → `Sending`; reconciliation poll → `Sent` |
| `Server { code, message }` | **see §2.1 sub-table** — varies by raw status code | varies | varies | varies | varies — see sub-table |
| `NotFound` | NO retry (semantic, not failure) | n/a | n/a | n/a — this is a typed Result for `by_server_fiscal_no` (`channel.rs:60`); caller decides routing | Not an error path during normal submit. Used by reconciliation and App::boot recovery (see §4); no DocState transition implied directly |
| `ServerFiscalIdMismatch { expected_id, actual_id }` | NO retry (terminal, operator escalation) | 0 | n/a | `REQUIRES_MANUAL_RECONCILIATION` + CRITICAL audit `DPS_FISCAL_ID_MISMATCH` carrying both ids | Source `ErrorRetryable` → `RequiresManualReconciliation` (whitelist `:101`). This is a fiscal-chain integrity failure (PRRO_GATE-5js): the FN we expected is not what DPS holds. NEVER auto-retry; NEVER auto-recover |
| `QueryNotSupported(&'static str)` | NO retry (caller bug) | 0 | n/a | Wrapper-side error, not a DocState transition; surface as INTERNAL log + reject the calling code path | Should never reach the write-path on a submit. `query_by_local_identity` is the only documented path that emits this (`channel.rs:86`); it is reserved for reconciliation experiments and is a typed "do not call" signal |
| `Internal(String)` | NO retry (wrapper bug) | 0 | n/a | CRITICAL audit `DPS_WRAPPER_INTERNAL` + escalate to `REQUIRES_MANUAL_RECONCILIATION` (defensive: don't lose the document) | Per `error.rs:71-76` "production callers should never see this." If observed, the channel is misconfigured; the document SHOULD route to RequiresManualReconciliation rather than Rejected, because the failure is on our side |

**Backoff jitter:** all exponential entries above SHOULD apply
±20% jitter on the computed delay to avoid thundering-herd on
recovery resumes.  Proposed for M3a; mirrors standard RPC-client
practice.

**Source-state matrix anchor:** the "source-state implications"
column is grounded in the W0-1 §2.1 failure-class × source-state
table.  Whitelist gaps documented there are intentional design
constraints (e.g. "no Signed→Rejected"), and §3 below preserves
them.

### 2.1 `Server { code, .. }` sub-table by raw status code

Raw `CheckResponse.Status` enum values from
`rust/prro/proto/fiscal_server.proto:38-57`, cross-referenced with
the W3 decoder routing in
`rust/prro/src/transports/dps/dto.rs:161-289`:

> **Decode-routing observation:** the dto decoder already pre-classifies
> three of these codes BEFORE they reach `Server { code, .. }`:
> `-1` / `-13` / `-14` are routed to `DpsError::Authorization` (lines
> 178–184), and `-4` is routed to `DpsError::Transport` (lines 185–188).
> The remaining codes (`-2`, `-3`, `-5` … `-12`, `-15`, `-16`) are the
> ones that surface to the write-path as `Server { code, .. }`.  The
> sub-table below covers ALL negative codes for completeness, but
> M3 write-path code only needs to match-on the `Server`-routed subset
> directly; the others arrive as their pre-classified variant.

| Status | Wire name | Pre-routed by dto? | Caller-visible variant | Class | M3 retry policy | Source-state route | Rationale |
|---|---|---|---|---|---|---|---|
| `+1` | OK | n/a (success path) | `Ok(CheckAck)` | success | success (no retry path) | — | per `dto.rs:170-174` |
| `0` | UNKNOWN (proto-default) | yes — `Decode` (lines 175–177) | `DpsError::Decode("…Unknown=0")` | reconciliation-class | 1 `last_chk` probe (single shot, no retry loop); if match → drive forward; if NotFound or non-matching id → `RequiresManualReconciliation` directly | Sending/Sent → RequiresManualReconciliation (via §2 main-table Decode rule; pre-Pattern-B drafts said "ErrorRetryable then escalate" — that two-step was withdrawn for consistency with §2 main table) | proto-default field-missing; WebCheck reconciles via `lastChk` (`SubmitPtr.cs:105-117`).  Python today maps to `TransportRetryableError` at `dps_fiscal_server.py:536-539` and the M3 spec deviates: contract drift is operator-visible (no bounded retry), preserving M3's "fail-loudly on protocol drift" stance over Python's "retry-and-hope" |
| `-1` | ERROR_VEREFY | yes — `Authorization { kind: DocumentReject, .. }` after amendment (lines 178–184) | `DpsError::Authorization { code: -1, kind: DocumentReject, .. }` | terminal-authorization (per-doc) | NO retry | Live stage-4-4b: Sending → Rejected.  Reconciliation poll: Sent → Rejected (whitelist `:94`) | signature/crypto rejected at server. Python rejects terminal via `_raise_classified_dps_error` `dps_fiscal_server.py:511,550`.  Requires M2/W3 amendment to extend `DpsError::Authorization` with `kind` field — see §2 main table NB |
| `-2` | ERROR_CHECK | no — surfaces as `Server { code: -2, .. }` | `DpsError::Server { code: -2, .. }` | terminal-business | NO retry under M3 default. **Exception:** if doc_type ∈ {SHIFT_CLOSE, Z_REPORT} → 1 `last_chk` reconciliation probe (WebCheck §1.2 row 5).  **Amendment 2026-05-10 (R-W10-F3):** the original draft also required `error_message indicates "open shift"` as a substring gate; this was DROPPED at W10.1 review because DPS message text is not a stable contract — wording differs across server versions and translations, so a substring-routed gate could silently mis-classify close-shift races as terminal Rejects.  All `-2` for close-shift doc_types now route to `ProbeRequired` regardless of message text; the `last_chk` probe (W9) is the durable source-of-truth.  WebCheck loop still uses substring check (Python upstream behaviour, §1.2 row 5) — M3 deviates here. | Sent → Rejected | RRO verification failed. WebCheck retries this only on close-shift edge (`SubmitPtr.cs:103-141`). Python: rejects terminal `dps_fiscal_server.py:512` |
| `-3` | ERROR_SAVE | no — `Server { code: -3, .. }` | `DpsError::Server { code: -3, .. }` | **transient-retry** | retry up to `max_recovery_attempts=5`; exponential backoff capped at 30s; dead-letter to `REQUIRES_MANUAL_RECONCILIATION` after exhaustion | Live stage-4-4b: Sending → ErrorRetryable.  Re-drive under Pattern B: ErrorRetryable → Sending → wire (NOT direct ErrorRetryable → Sent — see ADR-M3-A9 step 3 retry-path policy) | server-side persist transient. WebCheck §1.2 row 2 retries inline up to 7× (`SubmitPtr.cs:66-76`). Python today rejects terminal — **M3 deviates from Python here** to match WebCheck behaviour and reduce false REJECTEDs |
| `-4` | ERROR_UNKNOWN | yes — `Transport` (lines 185–188) | `DpsError::Transport("ERROR_UNKNOWN (-4)…")` | transient-retry | retry per `Transport` policy in §2 main table | Sent → ErrorRetryable | per W0-1 D3 review finding cited in `dto.rs:185-188`: "the wire error is not stable enough to mark the FN broken — back off + retry" |
| `-5` | ERROR_TYPE | no — `Server { code: -5, .. }` | `DpsError::Server { code: -5, .. }` | terminal-business | NO retry | Sent → Rejected | wrong `check_type`. M3 adapter bug; surface CRITICAL audit so it gets fixed in code, not retried |
| `-6` | ERROR_NOT_PREV_ZREPORT | no — `Server { code: -6, .. }` | `DpsError::Server { code: -6, .. }` | terminal-business with audit, possibly operator-recoverable | NO auto-retry; route to `REQUIRES_MANUAL_RECONCILIATION` (operator may need to file Z-report manually for a prior shift) | Sent → ErrorRetryable → RequiresManualReconciliation chain | DPS expected a prior Z-report we never sent. Operator decision territory |
| `-7` | ERROR_XML | no — `Server { code: -7, .. }` | `DpsError::Server { code: -7, .. }` | terminal-business | NO retry | Sent → Rejected | invalid XML. M3 builder bug; CRITICAL audit |
| `-8` | ERROR_XML_DATE | no — `Server { code: -8, .. }` | `DpsError::Server { code: -8, .. }` | terminal-business | NO retry | Sent → Rejected | date in XML mismatches DPS expectation. Often clock skew on the gateway host; CRITICAL audit + operator notification |
| `-9` | ERROR_XML_CHK | no — `Server { code: -9, .. }` | `DpsError::Server { code: -9, .. }` | terminal-business | NO retry | Sent → Rejected | invalid check format |
| `-10` | ERROR_XML_ZREPORT | no — `Server { code: -10, .. }` | `DpsError::Server { code: -10, .. }` | terminal-business | NO retry | Sent → Rejected | invalid Z-report format |
| `-11` | ERROR_OFFLINE_168 | no — `Server { code: -11, .. }` | `DpsError::Server { code: -11, .. }` | terminal-business | NO retry | Sent → Rejected | 168-hour cumulative-offline limit exceeded. Node MUST go to `BLOCKED` (per W0-1 §2.4 NodeMode); operator must reconcile time budget. CRITICAL audit + node-state flip |
| `-12` | ERROR_BAD_HASH_PREV | no — `Server { code: -12, .. }` | `DpsError::Server { code: -12, .. }` | **MAC-recovery-class** (one bounded auto-retry) | parse `error_message` for `store {64hex}` (regex per Python `dps_fiscal_server.py:494`); if extractable → ONE auto-recovery: re-derive MAC + re-sign + re-send; if recovery fails OR hash not extractable → terminal Rejected | Sent → ErrorRetryable for the bounded recovery; on success → forward to Sent → Kvt1; on failure → Rejected | DPS expects a different prev-hash. Python implements bounded MAC recovery at `write_path.py:903-994`; M3 inherits the contract |
| `-13` | ERROR_NOT_REGISTERED_RRO | yes — `Authorization { kind: FiscalNumberNotRegistered, .. }` after amendment (lines 178–184) | `DpsError::Authorization { code: -13, kind: FiscalNumberNotRegistered, .. }` | terminal-authorization, operator-escalation (per-FN) | NO retry. Route to `REQUIRES_MANUAL_RECONCILIATION` (FN registration is operator-resolvable, not a per-doc reject) | Live stage-4-4b: Sending → ErrorRetryable → RequiresManualReconciliation chain.  Reconciliation poll: Sent → ErrorRetryable → RequiresManualReconciliation chain | FN not registered. Different from `-1`: not the document, the FN itself.  Requires M2/W3 amendment — see §2 main table NB |
| `-14` | ERROR_NOT_REGISTERED_SIGNER | yes — `Authorization { kind: FiscalNumberNotRegistered, .. }` after amendment (lines 178–184) | `DpsError::Authorization { code: -14, kind: FiscalNumberNotRegistered, .. }` | terminal-authorization, operator-escalation (per-FN) | NO retry. Route to `REQUIRES_MANUAL_RECONCILIATION` | Live stage-4-4b: Sending → ErrorRetryable → RequiresManualReconciliation chain.  Reconciliation poll: Sent → ErrorRetryable → RequiresManualReconciliation chain | signer cert not registered. Operator must rotate / reconcile creds.  Requires M2/W3 amendment — see §2 main table NB |
| `-15` | ERROR_NOT_OPEN_SHIFT | no — `Server { code: -15, .. }` | `DpsError::Server { code: -15, .. }` | **conditional reconciliation** | If doc_type ∈ {SHIFT_CLOSE, Z_REPORT}: `last_chk` reconciliation probe (WebCheck §1.2 row 3, `SubmitPtr.cs:77-90`); if match → drive forward; if not → terminal Rejected. **For all other doc_types: terminal Rejected** (M3 §1.4 row 10 rationale) | Sent → Rejected; OR Sent → ErrorRetryable (then drive forward) on the reconciliation path | DPS reports no open shift. Often a transient race for close-shift; for SELL/RETURN it means our stage-1 guard let through a doc without an OPEN shift, which is an internal bug |
| `-16` | ERROR_OFFLINE_ID | no — `Server { code: -16, .. }` | `DpsError::Server { code: -16, .. }` | offline-failover-class | **M3a: terminal Rejected with operator alert** (M3a is ONLINE-only; offline-pool reconciliation lives in M3b). M3b: route to offline-pool reconciliation per WebCheck §1.2 row 4 | M3a: Sent → Rejected; M3b: Sent → ErrorRetryable then route to offline-id reconciliation worker | invalid offline-id. WebCheck switches the whole node to offline-technical mode (`SubmitPtr.cs:91-100`); for M3a we fail-fast since offline isn't implemented |

**Notes:**

- The "Pre-routed by dto?" column reflects W3 decoder behaviour
  shipped in M2; it is the contract M3 inherits.  Changing
  pre-routing requires a W3 amendment.
- The phrase "1 `last_chk` reconciliation probe" means: call
  `DpsChannel::last_chk(fn_sign)` once, OUTSIDE the lock
  (per invariant #1); compare `response.id` to the doc's expected
  `transport_request_id`; route based on match / no-match; do NOT
  loop the probe.  If `last_chk` itself fails with `Transport` /
  `Decode`, fall through to the underlying error's policy.

---

## 3. Pending-state recovery rules

For EACH of the pending states define an exact App::boot
reconciliation action.  M2 ships **7 pending states** at
`rust/prro/src/db/repositories/fiscal_documents.rs:176` —
`PREPARED`, `SIGNED`, `ENCRYPTED`, `SENT`, `KVT1`, `KVT2`,
`ERROR_RETRYABLE`.  M3a adds an **8th: `SENDING`** per ADR-M3-A9
(W0-3 §8.4) as the Pattern B intermediate that protects against
duplicate-send across crash boundaries.  All 8 are tabulated
below; `SENDING` is the row whose recovery rule is the
critical Pattern B safety contract.

| Source state | Reconciliation action | Whitelist transitions invoked (per W0-1 §2.1) | Design constraint preserved |
|---|---|---|---|
| `PREPARED` | **Re-drive forward via stage 3 (sign).**  No DPS query needed — the document never left the gateway. The signing artefact is missing or stale; treat as fresh sign attempt against the existing `lnd`. Sign-retry resume in Python (`write_path.py:351-363`) does this same thing | `Prepared → Signed` (whitelist `:85`) on success; `Prepared → Rejected` (whitelist `:86`) only on pre-sign business validation failure; **NOT** `Prepared → ErrorRetryable` (gap is intentional per W0-1 §2.1 design constraint "Prepared→ErrorRetryable absence is intentional — fresh PREPARED has nothing to retry") | Preserves W0-1 design constraint that PREPARED has no transient-retry exit; only success or pre-sign reject |
| `SIGNED` | **Re-drive forward via stage 4 entry (CAS Signed→Sending, then wire send).** The CMS bytes are persisted in `document_files.SIGNED_XML`; recovery skips re-sign. **Safe to re-drive** because under Pattern B (W0-2 §5.2 + ADR-M3-A5) state=SIGNED means the wire request has NOT yet been initiated — stage 4 always flips to Sending BEFORE calling `DpsChannel::send_chk`.  The dangerous "wire-might-have-fired" state is `Sending`, not `Signed`. | `Signed → Sending` (Pattern B entry; proposed addition to W0-1 §2.1 whitelist per ADR-M3-A5); `Signed → Encrypted` (Checkbox-flow only; out of M3a scope); `Signed → OfflineLocalAck` (whitelist `:89`) only when M3b offline routing is active. **NOT** `Signed → Rejected` — the W0-1 §2.1 design constraint "SIGNED→REJECTED is INTENTIONALLY ABSENT" applies: a DPS business reject can only be observed after the request lands at the server, which means persistence MUST first reach Sent | Preserves W0-1 design constraint; bypassing it would let recovery silently violate state transitions (M2 invariant #8). Pattern B + Sending intermediate is the structural guarantee that SIGNED is safe to re-drive |
| `SENDING` (new pending state per ADR-M3-A5 / ADR-M3-A9) | **DO NOT auto-re-send.** Recovery CAS Sending→ErrorRetryable + audit `crash_resume_sending_to_error_retryable`.  The wire request was either (a) sent to DPS but the reply was lost, or (b) never actually transmitted — recovery cannot distinguish.  Mirror of Python `write_path.py:144-165` — DPS does not deduplicate (`write_path.py:148`), so re-sending is the duplicate-document hazard.  Operator inspects the ErrorRetryable doc and either requeues (after manual `last_chk` confirms DPS has no record) or marks RequiresManualReconciliation (DPS already has it, can't recover automatically).  This is the W0-2 §5.2 / ADR-M3-A5 binding contract. | App::boot recovery uses `Sending → ErrorRetryable` only (Pattern B crash-resume; proposed addition per ADR-M3-A9).  Live stage-4-4b transitions out of Sending — `Sending → Sent`, `Sending → Kvt1` (OK paths), `Sending → Rejected` (immediate authz / business reject), `Sending → ErrorRetryable` (transient transport failure with known wire reply) — are NOT recovery actions; recovery NEVER drives Sending forward to Sent / Kvt1 / Rejected without an authoritative DPS query and operator decision | Inherits Python's safety contract verbatim; preserves M2 invariant #4 (idempotency at the wire) by preventing duplicate-send |
| `ENCRYPTED` | **Re-drive forward via stage 4 (send).** Out of M3a ONLINE-only scope; ENCRYPTED is Checkbox-flow specific. For M3a, the only legitimate way to land in ENCRYPTED is a misconfigured backend; route to `REQUIRES_MANUAL_RECONCILIATION` via the ErrorRetryable chain | `Encrypted → Sent` (whitelist `:90`); `Encrypted → ErrorRetryable` (whitelist `:91`); `ErrorRetryable → RequiresManualReconciliation` (whitelist `:101`) | Preserves W0-1 design constraint that no Encrypted→Rejected transition exists; recovery either drives forward or escalates |
| `SENT` | **Re-query DPS via `last_chk(fn_sign)`.** This is the ambiguous case: did DPS receive the request? Three possible outcomes: (a) `last_chk` returns OK with `id == transport_request_id` → drive forward to KVT1 (then KVT2, Ack); (b) `last_chk` returns OK with non-matching id → mark stuck (doc lost en route) → `RequiresManualReconciliation`; (c) `last_chk` returns `NotFound` → DPS has no record → drop to ErrorRetryable, then **M3a DPS re-drive uses Pattern B**: `Sent → ErrorRetryable → Sending → wire`.  The intermediate `Sent → ErrorRetryable` step is required because the whitelist has no direct `Sent → Sending` path (and adding one would conflate "post-Sent reconciliation" with "direct stage-4 entry").  Python equivalent: `dps_fiscal_server.py:262-349` | `Sent → Kvt1` (whitelist `:95`) on confirmed receipt; `Sent → ErrorRetryable` (whitelist `:93`) on transient lookup failure OR on case (c) NotFound (transition to ErrorRetryable then re-drive via `ErrorRetryable → Sending`); `Sent → Rejected` (whitelist `:94`) ONLY for pre-classified terminal business reject from the `last_chk` reply itself.  **NOT** direct `Sent → Sending` (path goes via ErrorRetryable per ADR-M3-A9 step 3) | Preserves W0-1 invariant that source state SENT is ambiguous and must be resolved by an authoritative server query, never by local guessing.  Pattern B re-send via the ErrorRetryable→Sending two-step preserves the SENDING-marker safety contract |
| `KVT1` | **Re-drive forward to KVT2.** KVT1 means the first protocol receipt is persisted; recovery re-queries `last_chk` to attempt to fetch KVT2 (or polls for it via `poll_status` per Python `reconciliation.py:60-74`). DO NOT resend — the document is acknowledged at the protocol level; only the second receipt is missing | `Kvt1 → Kvt2` (whitelist `:95`) on KVT2 arrival; `Kvt1 → ErrorRetryable` (whitelist `:96`) on transient transport failure during re-query; **NOT** `Kvt1 → Rejected` (gap is intentional per W0-1 §2.1: "reject after KVT1 is a server-protocol violation") | Preserves W0-1 design constraint that KVT1→Rejected is impossible; preserves invariant #8 |
| `KVT2` | **Re-drive forward to ACK only.** KVT2 means the document is fully accepted at the protocol level; the only missing thing is the local ACK transition + finalize bookkeeping (e.g. `node_state.last_known_unsigned_xml_sha256` for the next-doc MAC chain, per W0-1 §3.5). Recovery executes the same finalize logic that stage 5 would have executed pre-crash. **No DPS query needed** — KVT2 is terminal at the server | `Kvt2 → Ack` (whitelist `:97`) is the **only** legal transition out of KVT2; cited W0-1 §2.1 design constraint "Kvt2→ErrorRetryable absence is intentional — Kvt2 recovery re-drives forward to Ack only" | Preserves W0-1 design constraint; the explicit recovery example for §6 deterministic-replay invariant |
| `ERROR_RETRYABLE` | **Re-drive forward via stage 3 (re-sign) or stage 4 entry (re-send via SENDING marker) depending on whether SIGNED_XML exists.** Bounded re-attempts per §2 policy table (max 5). On the Nth failure, escalate to `RequiresManualReconciliation`. Mirror of Python `reconciliation.py:200-256` (the `poll.retryable` branch with `attempts >= self.max_recovery_attempts`) | **M3a DPS path:** `ErrorRetryable → Sending` (per ADR-M3-A9 step 3 — Pattern B; the only DPS re-send path), then `Sending → Sent / Kvt1 / Rejected / ErrorRetryable` per §2 mapping.  `ErrorRetryable → Kvt1` (whitelist `:100`) is reserved for direct `last_chk` re-query paths that bypass wire send.  `ErrorRetryable → Sent` (whitelist `:99`) stays in the whitelist for backward compat / non-DPS but **MUST NOT be invoked by M3a DPS code** (re-introduces the duplicate-send hazard).  `ErrorRetryable → RequiresManualReconciliation` (whitelist `:101`) on retry-budget exhaustion.  The retryable→retryable self-loop is implicit (state stays the same; only `recovery_attempts` advances) — implemented at the row-update level, not as a DocState transition | Preserves W0-1 inclusion of ERROR_RETRYABLE in the pending set and its legal exits; Pattern B propagates to retry path so re-send goes through SENDING marker |

### 3.1 Explicit exclusion rules (non-pending states)

Per `rust/prro/src/db/repositories/fiscal_documents.rs:182-185`:

| State | App::boot action | Reason |
|---|---|---|
| `OFFLINE_LOCAL_ACK` | **Hand off to `offline_sync_service` worker** (separate worker; M3b scope) | Cited `fiscal_documents.rs:184`. The offline-sync worker has its own state machine (six external targets + retry self-loop per W0-1 §2.1). M3a App::boot MUST NOT touch these documents; the offline worker is responsible for them |
| `REQUIRES_MANUAL_RECONCILIATION` | **No-op: operator-driven flow** | Cited `fiscal_documents.rs:185`. App::boot MUST NOT auto-re-drive these; doing so would lose the operator-escalation signal. Optional: emit an INFO audit `MANUAL_RECON_PENDING` per occurrence so the operator's dashboard reflects the count |
| `ACK` | **No-op: terminal-success** | Cited `fiscal_documents.rs:183`. The document is fully accepted; nothing to recover |
| `REJECTED` | **No-op: terminal** | Cited `fiscal_documents.rs:183`. The document was rejected by the backend; recovery would violate "reconciliation must not silently violate state transitions" (CLAUDE.md invariant #8) |
| `CANCELLED` | **No-op: terminal** | Cited `fiscal_documents.rs:183`. Operator/system cancellation; same logic as REJECTED |

### 3.2 Concurrency note for App::boot

`list_pending_for_fn`
(`rust/prro/src/db/repositories/fiscal_documents.rs:192`) returns
docs in `(lnd, created_at, document_id)` order — the
fiscal-chain order.  Recovery MUST process docs in this order on a
per-FN basis; concurrency across FNs is allowed (matches Python
reconciliation phase 2 ThreadPoolExecutor pattern at
`reconciliation.py:296-316`), but a single FN must not have two
recovery workers racing on the chain.  This is the **same
single-writer-per-FN invariant as the live ingress lease** (M2
invariant #2); App::boot MUST acquire the FN lease before
re-driving any doc.

---

## 4. App::boot reconciliation contract (Research-addresses PRRO_GATE-ah8)

### 4.1 Status quo and the bd-issue surface

**Today (M2):** `App::boot` exists at
`rust/prro/src/app.rs:28` but is **pool + migrations only** —
it (1) creates the parent dir for the SQLite DB if missing,
(2) opens the pool via `crate::db::open_pool`, (3) returns the
`App` handle.  It deliberately does NOT touch `node_state`;
the doc-comment at `rust/prro/src/app.rs:4-10` explicitly cites
PRRO_GATE-ah8 as the reason: "M1's boot only opens the pool +
applies migrations; bootstrap of `node_state` rows is deferred
to a later task with explicit reconciliation against `shifts` /
`fiscal_documents`."

The contract below is what M3a implementation MUST land as a
**reconciliation/bootstrap phase that runs after the existing
M1 boot boundary**.  Concretely: `App::boot` keeps its current
shape (pool + migrations); a new method (working name
`App::reconcile_pending` or similar — final naming is M3a impl
detail) runs the per-FN decision tree below before the runtime
accepts ingress traffic.  The phase ordering mirrors Python
`runtime/supervisor.py:34-58` (PHASE1_STARTING → migrate →
PHASE1_COMPLETE → optional reconciliation → ready=True).

**The hazard PRRO_GATE-ah8 names:** `node_state.upsert_initial`
(`rust/prro/src/db/repositories/node_state.rs:67-87`) refreshes
`mode` AND `shift_state` on conflict via
`ON CONFLICT(fiscal_number) DO UPDATE SET mode = excluded.mode,
shift_state = excluded.shift_state`.  If App::boot iterates
configured FNs and unconditionally calls
`upsert_initial(fn, Online, Closed, 1)` for all of them, it will
**overwrite** `shift_state=Opened` from a crashed in-flight shift,
**masking the recovery requirement**.  The doc-comment at
`node_state.rs:14-23` already names the contract; M3a must land the
caller-side discipline.

### 4.2 Pre-conditions

Before App::boot touches **any** FN row, it MUST:

1. **Acquire the singleton process lock** via
   `runtime::singleton::acquire(db_path)` (existing M1 helper at
   `rust/prro/src/runtime/singleton.rs:25`). Two concurrent
   `serve` processes against the same DB are forbidden.
2. **Run schema migrations to head** (re-uses existing M1
   migration runner; runs **before** any read of `node_state`).
3. **DB integrity probe** (mirror of Python `_ensure_persistent_pragmas`
   at `runtime/container.py:138-156`): `PRAGMA quick_check` must
   return `ok`.  On failure, **fail-closed before any FN-row write**:
   refuse startup with an explicit error (mirror of Python which
   raises `RuntimeError` and aborts process bootstrap).  Do NOT
   write `node_state.mode = STOP_MODE` to a DB the integrity
   probe just declared corrupt — writing into a corrupt DB is
   itself a footgun (the write may succeed, fail silently, or
   compound the corruption).  Operator-visible status MUST be
   surfaced via stderr / structured log / health endpoint
   (`/health/startup` returns 503 with the failure reason),
   NOT via a DB row.  M3a impl: emit a CRITICAL log
   `DB_INTEGRITY_CHECK_FAILED` with the quick_check output, set
   the in-memory health flag to refuse readiness, return a
   non-zero exit code from `App::boot`.  Earlier W0-3 drafts
   suggested transitioning to STOP_MODE — that recommendation
   was withdrawn after senior review (Python reference
   `container.py:144` does not write to a corrupt DB; raising
   is the correct behaviour).
4. **Per-FN-row exclusive lease** acquired before any per-FN
   recovery action (matches §3.2 single-writer-per-FN concurrency
   note).

### 4.3 Per-FN-row decision tree

For each configured FN at boot, App::boot MUST:

1. Call `node_state::get(pool, fn_id)` (already exists at
   `rust/prro/src/db/repositories/node_state.rs:103-125`).

2. **Branch on outcome:**

   **(a) FN row absent** (`get` returns `Ok(None)`):
   - Call `upsert_initial(fn, Online, Closed, 1)` (existing safe
     bootstrap). This is the only permitted use of
     `upsert_initial` from App::boot.
   - Audit: `NODE_STATE_INITIALISED` INFO.
   - No reconciliation work; the FN has no history.

   **(b) FN row present + `node_state.mode == ONLINE` + no pending
   docs** (`list_pending_for_fn(fn_id)` returns empty):
   - **Idempotent no-op.** Do NOT call `upsert_initial`. Do NOT
     touch `shift_state` or `next_lnd`. Audit:
     `NODE_STATE_BOOT_IDEMPOTENT` INFO with the observed shape.

   **(c) FN row present + pending docs found** (any non-empty
   list from `list_pending_for_fn`):
   - For each pending doc, in order, apply the §3 reconciliation
     rule for its source state.
   - Do NOT touch `node_state.mode` / `shift_state` /
     `next_lnd` directly; they are derived from the doc/shift
     repos and only updated as side-effects of legal whitelist
     transitions during reconciliation.
   - After the per-doc loop: audit `NODE_STATE_BOOT_RECONCILED`
     INFO carrying the histogram of outcomes.

   **(d) FN row present + `node_state.mode == OFFLINE`** (or
   `GOING_OFFLINE` / `GOING_ONLINE`):
   - **Preserve offline session pointer.** Do NOT reset.
   - M3a is ONLINE-only; if the persisted mode is OFFLINE on
     boot, the node MUST NOT silently flip to ONLINE. Three
     options: (i) refuse to boot with an explicit error
     "FN $fn is in OFFLINE mode — start with --recover-offline
     M3b CLI" (M3a recommendation); (ii) leave mode OFFLINE and
     reject ingress until the M3b offline worker drains;
     (iii) auto-flip after a `ping(fn_sign)` confirms ONLINE
     (M3b territory).
   - **M3a binding decision: option (i)** — refuse to boot. M3a
     is ONLINE-only by construction; an OFFLINE row at boot is a
     hard signal that M3b is required and isn't ready.
   - Audit: `NODE_STATE_BOOT_OFFLINE_REFUSAL` ERROR.

   **(e) FN row present + `shift_state ∈ {OPENING, CLOSING}`**
   (mid-transition):
   - This is the case PRRO_GATE-ah8 specifically calls out.
   - Do NOT mask `shift_state` to `Closed`. Do NOT call
     `upsert_initial`.
   - The corresponding SHIFT_OPEN / SHIFT_CLOSE / Z_REPORT
     document(s) are in the pending set (PREPARED/SIGNED/
     ENCRYPTED/SENT/KVT1/KVT2/ERROR_RETRYABLE) — branch (c)
     handles them per §3. After per-doc reconciliation
     completes, the shift state will be implicitly correct via
     the side-effects in `_apply_shift_side_effects_locked`
     (Python: `reconciliation.py:370-420`).
   - If a shift is in OPENING / CLOSING but **no corresponding
     pending doc** is found: this is a corruption signal
     (operator deleted the doc row, or DB was hand-edited).
     Transition shift to `ERROR` (per W0-1 §2.2 `any → ERROR`
     whitelist) + audit `SHIFT_BOOT_ORPHAN_ERROR` CRITICAL.
   - **Never** auto-transition OPENING→OPENED or CLOSING→CLOSED
     at boot without doc evidence; that violates invariant #8.

   **(f) FN row present + `node_state.mode == BLOCKED` /
   `STOP_MODE` / `CRYPTO_DEGRADED`:**
   - Preserve mode. Do NOT touch.
   - `BLOCKED` → log INFO; ingress on this FN remains gated by
     month-rollover logic (out of App::boot scope).
   - `STOP_MODE` → already terminal-soft; no recovery from boot.
     Operator must clear via separate CLI.
   - `CRYPTO_DEGRADED` → leave breaker open; first ingress
     attempt will trigger the half-open probe per
     `runtime/container.py:265-281`.

### 4.4 Post-conditions

After App::boot completes for **all** configured FNs:

1. **No FN row has had its `shift_state` silently masked.** This
   is the PRRO_GATE-ah8 acceptance test verbatim: create a
   `node_state` row with `shift_state = Opened`, run App::boot,
   assert the row still has `shift_state = Opened` (no
   overwrite).
2. **Every pending doc has either advanced state per the §3
   rules, or had `recovery_attempts` incremented + a structured
   audit entry** describing why it stayed in the same state.
3. **No FN's `next_lnd` was decremented or reset.** Cited W0-1
   §1.5 decision: `next_lnd` is the single source of truth for
   the chain key; App::boot must NOT touch it.
4. **`node_state.last_known_unsigned_xml_sha256` is unchanged**
   (it's only ever updated during stage-5 finalize on a real Ack
   transition, per W0-1 §3.5; recovery driving forward from KVT2
   to ACK preserves this contract because it executes the same
   finalize side-effects).
5. **Health gates flip** in this order: `live = true` (set
   during process bootstrap) → `startup_complete = true` (after
   App::boot per-FN loop terminates) → `ready = true` (only
   after reconciliation completes; mirrors Python
   `runtime/supervisor.py:34,56`).

### 4.5 Idempotency invariant

App::boot run twice in a row (process restart immediately after a
clean App::boot completes) MUST produce **the same final state**.
Concretely:

- The second run observes branch (b) for every previously-OK FN
  (no pending docs; `mode == ONLINE`; idempotent no-op).
- For FNs that completed reconciliation in the first run, the
  second run also observes (b).
- For FNs that escalated to `REQUIRES_MANUAL_RECONCILIATION` in
  the first run, the second run observes branch (c) → §3.1
  exclusion rule "operator-driven; no-op" (audits the count, does
  NOT re-drive).
- For FNs that refused boot (branch d, OFFLINE mode), the second
  run also refuses boot. Repeat-refusal is idempotent.

### 4.6 Citations to Python boot call sites

For parity audit:

- `src/prro_gateway/runtime/container.py:114-126` — `RuntimeContainer.__init__` invokes `StartupSupervisor.run()` after wiring services.
- `src/prro_gateway/runtime/supervisor.py:56` — `if self.reconcile_on_startup and self.reconciliation_service is not None: …` — Python equivalent of the per-FN reconciliation pass.
- `src/prro_gateway/runtime/container.py:282-294` — `_ops_tick` ONLINE branch (post-boot loop): `reconciliation_service.reconcile_pending(conn, fiscal_number=fiscal_number)` + offline-sync drain. M3 will likely fold the boot-time sweep into the same code path with a "first-tick" flag, but that's M3a implementation detail.
- `src/prro_gateway/runtime/container.py:138-156` — `_ensure_persistent_pragmas` integrity probe; M3 inherits.

**Drift between Python current behaviour and proposed Rust contract:**

| Aspect | Python today | Rust M3 proposal | Reason for drift |
|---|---|---|---|
| Per-FN boot pre-check | does NOT call `upsert_initial` blindly (because Python has no equivalent helper that masks shift_state — Python uses repository-specific updates) | explicit `get(fn) → branch on outcome` | Rust `upsert_initial` exists and is dangerous; Python has no equivalent footgun |
| OFFLINE-on-boot handling | reconciles, attempts ping, may auto-flip via `_maybe_ping_and_go_online` (`container.py:262`) | M3a refuses boot ("OFFLINE-only mode requires M3b") | M3a is ONLINE-only by scope; no ping-and-flip path is implemented |
| Mid-transition shift orphan | not explicitly handled | transitions to `ERROR` + CRITICAL audit | Rust adds an explicit fail-closed for corruption signal |

---

## 5. Offline failover trigger map

M3a does NOT implement offline writes (§2 row `Server { code: -16, .. }`
M3a column states "M3a: terminal Rejected with operator alert").
The carve-out boundary MUST be unambiguous so M3a doesn't silently
catch states it shouldn't.

| Retry class / DpsError variant | Triggers offline failover? | M3a behaviour | M3b behaviour (future) |
|---|---|---|---|
| `Transport(String)` | **No** in M3a (offline writes not implemented). In M3b: yes IF `node_state.last_online_seen` exceeds threshold (e.g. 3 consecutive `Transport` failures) | Retry per §2 main table; on exhaustion → `RequiresManualReconciliation`; emit ALERT for operator | Open offline session (`OPENING → OPEN`); route subsequent docs to offline pool; node mode → `OFFLINE` |
| `Authorization(String)` | **No (ever)** — terminal-class | Terminal `Rejected` / `RequiresManualReconciliation`; operator must rotate creds | Same as M3a; auth failures are not network failures |
| `Decode(String)` | **No** — reconciliation, not failover | One `last_chk` probe; route per §2 table | Same as M3a |
| `Server { code: -3, .. }` (ERROR_SAVE) | **No** — server-side transient | Bounded retry per §2.1 | Same as M3a |
| `Server { code: -11, .. }` (ERROR_OFFLINE_168) | **No (inverse direction)** — terminal | Terminal `Rejected` + `node_state.mode → BLOCKED` (per W0-1 §2.4); refuse new ingress until month rollover | Same as M3a — node BLOCKED is a hard cap; cannot be worked around by going offline |
| `Server { code: -16, .. }` (ERROR_OFFLINE_ID) | **Yes** in M3b; **No** in M3a (M3a fails fast) | Terminal `Rejected` + CRITICAL audit `OFFLINE_ID_INVALID_M3A_NO_FAILOVER` + ALERT | Per WebCheck §1.2 row 4 (`SubmitPtr.cs:91-100`): switch the node to offline-technical mode + reconcile offline-id pool |
| `Server { code, .. }` other terminal (`-2`, `-5`–`-10`, `-12` after bounded MAC recovery, `-15` for non-shift-mgmt ops) | **No** — business reject is terminal | Terminal `Rejected` per §2.1 | Same as M3a — business rejects are about the document content, not the network |
| `NotFound` | **No** — semantic Result | n/a (not a submit failure) | n/a |
| `ServerFiscalIdMismatch` | **No (ever)** — fiscal-chain integrity failure | `RequiresManualReconciliation` + CRITICAL audit | Same as M3a — fiscal-id mismatch is a hard signal that local FN config is wrong; offline cannot help |
| `QueryNotSupported` / `Internal` | **No** — wrapper bug | INTERNAL log + escalate to `RequiresManualReconciliation` | Same as M3a |

**M3a binding contract:**
- The only retry class that "would trigger offline" but does not
  in M3a is `Server { code: -16, .. }`.
- All other classes have the same routing in M3a and M3b; offline
  failover (M3b) only adds new paths, it does not change existing
  classifications.
- M3a code MUST emit an explicit ALERT (operator-visible audit at
  ERROR severity) when a class would have failed over in M3b but
  does not in M3a.  Audit event types to reserve:
  `OFFLINE_FAILOVER_M3A_NOT_IMPLEMENTED` (general),
  `OFFLINE_ID_INVALID_M3A_NO_FAILOVER` (specific to `-16`).

**Pre-conditions to gate offline failover (for the future M3b
implementation, recorded here so M3a doesn't accidentally violate
them):**
- `node_state.mode == ONLINE` (cannot fail over to offline if
  already OFFLINE — the partial UNIQUE on
  `offline_sessions(fiscal_number) WHERE status IN ('OPENING','OPEN','CLOSING')` proposed in W0-1 §6.3 enforces
  this at the DB level).
- The active shift's state allows new offline-pool docs (i.e. not
  CLOSED/ERROR).
- `OfflineRepository.get_open_session(fn)` returns None (no live
  session yet).
- Per CLAUDE.md invariant #5 ("Offline must respect time and code
  limits"): cumulative-month offline budget is not exhausted.

---

## 6. Deterministic-replay invariant

Recovery must produce the same final state regardless of
crash-and-resume vs uninterrupted run. Per pending state, a
concrete example:

### 6.1 PREPARED — crash between INSERT and sign

- **Uninterrupted run:** stage-1 commits `state=PREPARED`; stage-3
  builds canonical XML, calls `CryptoProvider.sign_cms_detached`,
  commits `state=SIGNED`. Final state: ACK after stage-5.
- **Crash AT the moment of `state=PREPARED` commit:** process
  restarts; App::boot finds the doc in PREPARED;
  `list_pending_for_fn` returns it; recovery branch (c) per §4.3
  invokes the §3 PREPARED rule "re-drive forward via stage 3"; the
  same canonical XML is re-built (deterministic by
  `CanonicalDoc` + frozen XML builder per M2 W4); the same CMS is
  re-signed; recovery commits `state=SIGNED`.
- **Final state in both cases: ACK.** Determinism preserved
  because canonical XML construction is referentially transparent
  on `(CanonicalDoc, business_ts, lnd, prevhash)` — all of which
  are persisted at PREPARED commit time.

### 6.2 SIGNED — crash between sign-commit and stage-4 entry

- **Uninterrupted run:** stage-3 commits `state=SIGNED`; stage-4
  enters with `with_immediate` → CAS `Signed→Sending` → commit →
  release lock → call `DpsChannel::send_chk(envelope)` outside
  the lock → on reply, `with_immediate` → CAS
  `Sending→Sent/Kvt1`.  Final state: ACK after stage-5.
- **Crash AT `state=SIGNED` commit, BEFORE stage-4's
  `Signed→Sending` CAS executes:** the wire request has NOT
  been transmitted (Pattern B's invariant — wire send happens
  ONLY after the Sending CAS commit).  App::boot recovery
  invokes §3 SIGNED rule "re-drive forward via stage 4 entry"
  — re-execute the `Signed→Sending` CAS, then the wire send.
  This is a **fresh first send** from DPS's perspective; no
  duplicate hazard.  Determinism is preserved because:
  - The CMS bytes are re-read from `document_files.SIGNED_XML`
    (persisted at stage-3 commit per W0-1 §3.3);
  - The canonical envelope is referentially transparent on
    `(SignedCmsBytes, lnd, rro_fn)` (M2 W4 byte-equivalence
    contract);
  - The `local_number=lnd` is stable on the existing row, so
    the wire payload is bit-identical to what stage-4 would
    have produced live.

### 6.3 SENDING — crash AFTER `Signed→Sending` CAS but BEFORE wire reply

This is the critical case Pattern B exists to handle.

- **Uninterrupted run:** the `Sending→Sent/Kvt1` CAS commits
  after the wire reply.
- **Crash WHILE state=SENDING:** the wire request was either
  (a) transmitted to DPS but the reply was lost (network /
  client crash post-send), (b) transmitted to DPS and accepted
  there, but the local commit of `Sending→Sent/Kvt1` did not
  happen, or (c) NOT transmitted (e.g. crash in the brief
  window between `Sending` commit and the actual TCP write).
  Recovery cannot distinguish these cases without consulting
  DPS.
- **Recovery action (per §3 SENDING row):** CAS
  `Sending→ErrorRetryable` + audit
  `crash_resume_sending_to_error_retryable`; do NOT
  auto-re-send.  Operator inspects the ErrorRetryable doc and
  manually consults `last_chk(fn_sign)`:
  - If `last_chk` returns the doc with matching id → DPS has it;
    operator transitions ErrorRetryable→Kvt1 (or further) via the
    whitelist `:100` path, drawing the local state forward to
    match DPS reality.
  - If `last_chk` returns a different id or NotFound → DPS does
    not have it; operator can re-queue the request via the
    Pattern B path `ErrorRetryable → Sending → wire send →
    4b CAS` (per ADR-M3-A9 step 3 retry-path policy — direct
    `ErrorRetryable → Sent` is forbidden for M3a DPS even on
    operator-initiated re-queue, because the duplicate-send
    hazard is identical regardless of who triggered the
    re-send), OR escalate to RequiresManualReconciliation if
    the operator wants to inspect the wire trace first.
- **Final state:** ACK if DPS had it OR operator re-queues
  successfully; RequiresManualReconciliation if operator can't
  reconcile.  Determinism guarantee: the final state is a
  function of `(observable DPS state at recovery time,
  operator decision)` — never a function of "did the wire fire
  twice".  Pattern B prevents the duplicate-send branch by
  construction.

**Why the operator-in-the-loop here:**  M3a recovery cannot
auto-reconcile this case safely.  WebCheck's reconciliation
pattern (`SubmitPtr.cs:77-90` for `-15`, `:105-117` for `0`)
uses `lastChk` to decide retry-vs-success after WebCheck's own
inline retry budget — that mechanism is for *transient* DPS
errors during a live submit.  Crash-mid-send is a different
class: the local process state is uncertain, and only the
operator (or, in M3b, a structured automated reconciliation
worker that calls `last_chk` with cooldown / rate-limiting) can
decide safely.  M3a punts to the operator; M3b may automate
this via a dedicated reconciler.

### 6.4 SENT — crash between transport return and KVT1-commit

- **Uninterrupted run:** stage-4 receives KVT1 from DPS, commits
  `state=KVT1`. Final state: ACK.
- **Crash AT `state=SENT` commit, BEFORE the in-memory KVT1
  response is persisted:** App::boot recovery invokes §3 SENT
  rule "re-query DPS via `last_chk`". Three sub-cases:
  - (a) `last_chk(fn_sign)` returns OK with `id == transport_request_id`: the doc reached DPS; recovery transitions
    `Sent → Kvt1` (whitelist `:95`) and continues forward.
  - (b) `last_chk` returns OK with non-matching id: the doc was
    lost; recovery escalates to `RequiresManualReconciliation`.
  - (c) `last_chk` returns `NotFound`: DPS has no record;
    recovery re-drives via the Pattern B path
    `Sent → ErrorRetryable → Sending → wire send → 4b CAS`
    (per §3 SENT rule + ADR-M3-A9 step 3).  The
    intermediate ErrorRetryable hop is required because the
    whitelist has no direct `Sent → Sending`; the two-step
    transition keeps Pattern B's SENDING-marker contract on
    the recovery path identical to the live path.
- **Final state: ACK in cases (a) and (c); `RequiresManualReconciliation` in case (b).** Determinism is **not** about a single
  final state in this case — it's about the same
  *deterministic mapping* from (DPS state at recovery time) to
  (local final state). A crash and an uninterrupted run that
  observe the same DPS reality at recovery time MUST converge to
  the same local final state.

### 6.5 KVT1 — crash between KVT1-commit and KVT2 reception

- **Uninterrupted run:** stage-4 commits `state=KVT1`; KVT2
  arrives synchronously (some endpoints) or via reconciliation
  poll; commits `state=KVT2`; stage-5 commits `state=ACK`.
- **Crash AT `state=KVT1` commit:** recovery invokes §3 KVT1 rule
  "re-drive forward to KVT2 via re-query". Same `last_chk` /
  reconciliation poll path as live operation. KVT2 either has
  arrived at DPS (forward) or has not (stay in KVT1; retry next
  cycle). Final state: ACK after KVT2 + stage-5 finalize.

### 6.6 KVT2 — crash between KVT2-commit and ACK transition

This is the canonical example the W0-1 design constraint
"Kvt2 recovery re-drives forward only" exists for.

- **Uninterrupted run:** stage-4 (or reconciliation) commits
  `state=KVT2`; stage-5 transitions `Kvt2 → Ack` (whitelist `:97`)
  + persists `node_state.last_known_unsigned_xml_sha256`. Final
  state: ACK.
- **Crash AT `state=KVT2` commit, BEFORE `Kvt2 → Ack` CAS
  succeeds:** App::boot recovery invokes §3 KVT2 rule "re-drive
  forward to ACK only". Note: there is **no DPS query** in this
  branch, because KVT2 is the protocol-level commit point; the
  document is fully accepted at the server. Recovery executes the
  stage-5 finalize logic — `transition_state(doc_id, Kvt2, Ack)`
  CAS UPDATE + `node_state.last_known_unsigned_xml_sha256` update
  + `audit_log` append + `inbox.status=DONE`. The CAS is
  idempotent: if it succeeds, the row flips to ACK; if the CAS
  has already happened (e.g. partial commit somehow reached ACK
  but the audit entry was missing) the CAS misses and recovery
  observes `TransitionOutcome::Conflict` (per
  `fiscal_documents.rs:74-79`) → reload + observe ACK already →
  no-op.
- **Final state: ACK regardless of crash point.** This is the
  **inviolable invariant** that justifies KVT2 being in the
  pending set in the first place (cited
  `fiscal_documents.rs:178-180` "KVT2 IS pending: a crash between
  persisting KVT2 and transitioning to ACK would otherwise strand
  the document"). The W0-1 §2.1 design constraint
  "Kvt2 → ErrorRetryable absence is intentional" is the
  whitelist-level expression of this invariant.

### 6.7 ERROR_RETRYABLE — crash mid-retry

- **Uninterrupted run:** retry budget allows N attempts; each
  attempt either succeeds (forward to Sent → Kvt1) or fails again
  (recovery_attempts++; stay in ERROR_RETRYABLE); on attempt N+1,
  escalate to RequiresManualReconciliation.
- **Crash mid-retry:** App::boot recovery invokes §3
  ERROR_RETRYABLE rule. The persisted `recovery_attempts` counter
  is monotonic and is NOT incremented unless a re-drive attempt
  actually executed and failed. So a crash between attempt-start
  and the failure-commit leaves `recovery_attempts` at its
  pre-attempt value; recovery's "is this attempt N or N+1?"
  decision is based on the persisted counter, not on in-memory
  state.
- **Determinism guarantee:** the final state (forward to Ack OR
  escalation to RequiresManualReconciliation) is a function of
  `(persisted recovery_attempts, observable DPS state)`. A crash
  cannot inflate `recovery_attempts` (because the increment is
  inside the same `with_immediate` as the attempt-failure persist
  per W0-2 §1.1 row 16) and cannot deflate it (counters are never
  decremented). Therefore the final state under crash-resume
  equals the final state under uninterrupted run for any given
  sequence of DPS responses.

---

## 7. Reviewer checklist

A future reviewer must re-verify the following if any of the
above changes:

- **If a `DpsError` variant is added or removed:**
  - `rust/prro/src/transports/dps/error.rs:14-77` updated.
  - W3 status-routing tests in
    `rust/prro/tests/dps_channel_smoke.rs` cover the new variant.
  - §2 main table extended with the new variant + retry policy.
  - §5 offline-failover trigger map extended.
  - W0-1 §2.1 failure-class × source-state matrix re-verified for
    impact.

- **If a new `CheckResponse.Status` raw code lands in the proto:**
  - `rust/prro/proto/fiscal_server.proto:38-57` updated.
  - dto decoder routing in
    `rust/prro/src/transports/dps/dto.rs:161-289` updated.
  - §2.1 sub-table extended with the new code + class +
    rationale.
  - WebCheck audit (§1) re-checked — does WebCheck's
    `SubmitPtr.SubmitCheck` need a new branch?
  - Python parity: `dps_fiscal_server.py:507-550` updated.

- **If the pending set in `list_pending_for_fn` changes:**
  - `rust/prro/src/db/repositories/fiscal_documents.rs:175-185`
    doc-comment updated.
  - `:203` SQL `state IN (...)` clause updated.
  - §3 pending-state recovery rules table extended with the new
    state.
  - §3.1 exclusion rules table updated if a state moves
    pending↔terminal.
  - §6 deterministic-replay invariant extended with a new
    crash-point example for the new state.
  - W0-1 §2.1 state table re-verified.

- **If the `allowed_transition` whitelist changes:**
  - `rust/prro/src/db/repositories/fiscal_documents.rs:81-103`
    updated.
  - W0-1 §2.1 transition matrix re-verified.
  - §3 pending-state recovery rules table re-checked: every
    "transitions invoked" cell still references valid whitelist
    entries.
  - The W0-1 §2.1 "design constraint" set re-verified — none of
    the intentional gaps (Signed→Rejected, Prepared→ErrorRetryable,
    Kvt2→ErrorRetryable, Kvt1→Rejected) have been silently filled.

- **If `node_state.upsert_initial` semantics change:**
  - `rust/prro/src/db/repositories/node_state.rs:67-87` updated.
  - Doc-comment at `:9-28` (CALLER CONTRACT) updated.
  - §4.3 App::boot decision tree re-verified — particularly
    branches (c) and (e).
  - §4.4 post-conditions re-verified — particularly the
    no-overwrite-of-shift_state guarantee that is the
    PRRO_GATE-ah8 acceptance test.

- **If `App::boot` behaviour changes:**
  - §4 decision tree + post-conditions re-verified.
  - §6 deterministic-replay invariant re-verified.
  - PRRO_GATE-ah8 acceptance test re-run.
  - Python parity citations (§4.6) refreshed.

- **If WebCheck retry constants drift:**
  - WebCheckMain decompilation re-pulled (per current
    `docs/webcheck_reverse/`).
  - §1.1 retry-budget table updated.
  - §1.2 per-status classification re-verified — particularly
    `-3`, `-15`, `-16`, `0` (the four classes with bespoke logic).

- **If M3b lands offline lifecycle:**
  - §5 trigger map re-verified — M3b column becomes M3 column;
    M3a column becomes "historical pre-offline".
  - §2.1 sub-table `-16` row updated to the M3b path.
  - Audit event types `OFFLINE_FAILOVER_M3A_NOT_IMPLEMENTED` /
    `OFFLINE_ID_INVALID_M3A_NO_FAILOVER` removed.

---

## 8. Proposed ADR amendments (if any)

The following amendments to
`docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md` are
**PROPOSED — NOT COMMITTED**. Coordinator to surface to user for
approval before any edit. Do NOT edit the ADR file from inside this
research spec.

### 8.1 PROPOSED — NOT COMMITTED — ADR-M3-A6: DpsError → retry policy

> **Numbering note:** W0-1 reserved ADR-M3-A1/A2; W0-2 reserved ADR-M3-A3/A4/A5; W0-3 continues from A6.  Final ADR numbering is committed at ADR-amendment time; this sub-section uses the continuation numbering to avoid collision in research drafts that cite the same milestone series.


```
Decision: M3 adopts the §2 main table + §2.1 sub-table as the
binding routing contract for `DpsError` variants in
`services::write_path`.  The contract has three pillars:

1. WebCheck-derived retry classes for negative `CheckResponse.Status`
   codes -3, -15 (close-shift only), -16, and 0 (proto-default
   UNKNOWN) carry distinct semantics from "all other negatives are
   terminal" — see §2.1.
2. Per-call gRPC deadline is constant + short (3–9 s band per
   WebCheck §1.1); set at `GrpcDpsChannel::connect` time, not
   per-call.
3. Recovery attempts are bounded (default 5, mirror Python
   `reconciliation.py:44`); on exhaustion, escalate to
   `REQUIRES_MANUAL_RECONCILIATION` via the
   ErrorRetryable→RequiresManualReconciliation chain.

All retry / reconciliation work happens OUTSIDE `with_immediate`
locks per CLAUDE.md invariant #1 + W0-2 §1 audit.

Research-addresses: PRRO_GATE-6bj (bd-issue closure deferred to
M3a implementation time).
```

### 8.2 PROPOSED — NOT COMMITTED — ADR-M3-A7: App::boot reconciliation contract

```
Decision: M3 App::boot follows the §4.3 per-FN decision tree.
Specifically:
- `node_state::upsert_initial` is permitted ONLY for branch (a)
  (FN row absent).
- For all other branches, App::boot MUST `get(fn)` first and
  reconcile via `list_pending_for_fn` + the §3 per-state recovery
  rules.
- OFFLINE-on-boot in M3a is a hard refusal (branch d, option (i)).
  Audit `NODE_STATE_BOOT_OFFLINE_REFUSAL` ERROR.
- Mid-transition shift orphan (branch e, no corresponding pending
  doc) transitions the shift to ERROR + CRITICAL audit.

Acceptance test (mandatory in M3a): create a `node_state` row
with `shift_state = Opened`, run App::boot, assert the row still
has `shift_state = Opened` (no overwrite). PRRO_GATE-ah8 acceptance
verbatim.

Research-addresses: PRRO_GATE-ah8 (bd-issue closure deferred to
M3a implementation time).
```

### 8.3 PROPOSED — NOT COMMITTED — ADR-M3-A8: pending-set documentation alignment

```
Decision: the §3 pending-state recovery rules table becomes the
binding M3a recovery contract.  The 7 M2-shipped pending
states from `rust/prro/src/db/repositories/fiscal_documents.rs:176`,
PLUS the M3a-introduced `SENDING` state per ADR-M3-A9 (8
pending states total in M3a), plus the explicit exclusions for
OFFLINE_LOCAL_ACK / REQUIRES_MANUAL_RECONCILIATION and the 3
terminal states (ACK / REJECTED / CANCELLED), are the M3a
recovery surface.

For each pending state, the §3 table specifies:
- the exact recovery action (re-drive forward / re-query DPS /
  mark recoverable / mark stuck for operator),
- the whitelist transitions invoked,
- the W0-1 design constraint preserved.

The §6 deterministic-replay invariant — particularly §6.6 KVT2 —
is the proof obligation that justifies including KVT2 in the
pending set.  Removing KVT2 from the pending set would re-introduce
the KVT2-strand bug cited at `fiscal_documents.rs:178-180`.

Research-addresses: parts of PRRO_GATE-6bj relating to
recovery rules (closure deferred to M3a).
```

### 8.4 PROPOSED — NOT COMMITTED — ADR-M3-A9: DocState::Sending + Pattern B for stage 4

```
Decision: M3a adopts Pattern B for the stage-4 send boundary
(per W0-2 §5.2 + ADR-M3-A5).  This requires a new DocState
value `Sending` that joins the pending set and gates wire send
through a CAS Signed→Sending → wire → CAS Sending→Sent/Kvt1
sequence, mirroring Python `write_path.py:786-803`.

Rationale: DPS does NOT deduplicate at the wire — Python
`write_path.py:148` explicitly states so.  Without a SENDING
intermediate, a process crash between state=SIGNED commit and
the wire reply lets recovery re-drive forward to send,
producing a duplicate document at DPS.  The SENDING marker
makes the dangerous state structurally distinct from the safe
SIGNED state: SIGNED means "stage 4 has not yet started";
SENDING means "wire send was initiated, outcome unknown".
Recovery rules (§3 + §6.3) treat the two cases differently:
SIGNED is safe to re-drive forward; SENDING is routed to
ErrorRetryable for operator inspection and never auto-re-sent.

Required code changes (M3a impl):

1. rust/prro/src/db/models/enums.rs:29-42 — add
   `Sending => "SENDING"` to the DocState enum (12 → 13
   values).

2. rust/prro/migrations/008_doc_state_sending.sql (new) —
   extend the fiscal_documents.state CHECK constraint to
   include 'SENDING'.  Migration is additive (existing rows
   keep their states); no backfill needed.

3. rust/prro/src/db/repositories/fiscal_documents.rs:81-103 —
   extend `allowed_transition` whitelist with:
   - (Signed, Sending)         — Pattern B entry (DPS profile)
   - (Encrypted, Sending)      — Pattern B entry (Checkbox/encrypted)
   - (Sending, Sent)           — wire OK, no inline KVT1
   - (Sending, Kvt1)           — wire OK with inline KVT1
   - (Sending, ErrorRetryable) — transient transport failure with
                                 known state (not crash-resume),
                                 OR crash-resume per App::boot rule
   - (Sending, Rejected)       — immediate-stage-4 terminal reject
                                 (Authorization -1, Server -2, -5
                                 .. -11, -16; see W0-3 §2.1
                                 sub-table for the full code map).
                                 Distinct from `Sent → Rejected`
                                 (whitelist `:94`) which is the
                                 reconciliation-path verdict —
                                 both stay in the whitelist
                                 because both are reachable: the
                                 `Sending → Rejected` path runs at
                                 4b commit; the `Sent → Rejected`
                                 path runs during reconciliation
                                 of an already-Sent doc whose
                                 `last_chk` returns a terminal
                                 reject status.
   - (ErrorRetryable, Sending) — **retry/requeue path under
                                 Pattern B**.  Any wire send
                                 from M3a DPS code MUST go through
                                 the SENDING marker first; that
                                 includes re-send attempts driven
                                 by the recovery loop after
                                 ErrorRetryable.  Without this
                                 transition, ErrorRetryable→Sent
                                 (the existing `:99` entry) would
                                 require either skipping the
                                 SENDING marker (re-introducing
                                 the duplicate-send hazard
                                 Pattern B exists to prevent) OR
                                 a two-step CAS via Sending —
                                 the latter is what M3a uses.

   The direct (Signed, Sent) and (Encrypted, Sent) transitions
   stay in the whitelist for backward compat (no production
   callers today; M3a impl simply does not invoke them).

   **Retry-path policy for M3a DPS:** the existing
   `(ErrorRetryable, Sent)` entry (`:99`) and `(ErrorRetryable,
   Kvt1)` entry (`:100`) are kept in the whitelist for
   backward compat with non-DPS / legacy code paths, but
   **M3a DPS code MUST NOT use them for wire send**.  The
   only DPS retry path is `ErrorRetryable → Sending → wire →
   Sending → Sent / Kvt1 / Rejected / ErrorRetryable`.
   `(ErrorRetryable, Kvt1)` remains usable for direct
   `last_chk` re-query paths that bypass wire send (e.g. when
   we already know KVT1 is at the server and just need to
   re-poll); document call-site usage with a comment.

4. rust/prro/src/db/repositories/fiscal_documents.rs:172-205 —
   extend `list_pending_for_fn`:
   - doc-comment: 7 → 8 pending states; add SENDING with the
     note "wire send initiated but outcome not yet persisted;
     recovery routes to ErrorRetryable, never re-sends" (mirror
     of Python write_path.py:148);
   - SQL `state IN (...)` clause: include `'SENDING'`.

5. M3a stage-4 implementation: open `with_immediate` → CAS
   Signed→Sending (or Encrypted→Sending for Checkbox flow) →
   commit → release lock → call `DpsChannel::send_chk` outside
   the lock → on reply: open `with_immediate` → CAS
   Sending→Sent (or Sending→Kvt1 if KVT1 inline; or
   Sending→ErrorRetryable on wire failure with known state) →
   commit + audit + transport_trace.

6. App::boot recovery worker: a doc found in SENDING after
   restart is unconditionally CAS'd Sending→ErrorRetryable
   with audit `crash_resume_sending_to_error_retryable`.  No
   wire calls made.  Operator (or M3b automated reconciler)
   resolves via `last_chk` + manual re-queue or escalation.

7. recovery_attempts column policy: SENDING does NOT count
   toward the per-doc recovery attempt budget on its own; the
   SENDING→ErrorRetryable transition is bookkeeping, not a
   retry.  The subsequent operator-driven re-queue MAY
   increment the counter depending on the operator action; M3a
   default is "do not auto-increment on this path".

8. W0-1 §2.1 transition matrix amendment: SENDING joins the
   pending source-states column; the failure-class table gets
   a new row for SENDING with: Transport→ErrorRetryable,
   Authorization→Rejected (post-send authz failures are rare
   but possible), DecodeError→ErrorRetryable, RetryBudget /
   Crash→ErrorRetryable (no auto-re-send).

9. W0-1 §6.3 schema clarifications: add the SENDING / migration
   008 amendment as a third bullet alongside the existing
   ix_offline_active UNIQUE proposal and OFFLINE_LOCAL_ACK
   whitelist extension.

Acceptance test: simulate a crash by manually inserting a doc
with state=SENDING, run App::boot, assert the doc is
transitioned to ErrorRetryable + audit row
`crash_resume_sending_to_error_retryable` exists + no wire
call was made (transport mock counts zero invocations for that
doc id).

Research-addresses: PRRO_GATE-6bj (retry/recovery policy
contract for the crash-mid-send class).  Bd-issue closure
deferred to M3a implementation time.
```

### 8.5 No-op note

If the user rejects 8.1, 8.2, 8.3, or 8.4, the bd issues
PRRO_GATE-6bj and PRRO_GATE-ah8 remain open and M3a
implementation must re-litigate.  This document remains the
research artefact regardless.

---

## 9. Test acceptance contract (M3a impl gate)

The contracts in §3 (pending-state recovery), §4 (App::boot
decision tree), §2 (DpsError → retry policy table + §2.1
sub-table) are not credible without explicit table-driven
proof obligations.  This section names the per-branch and
per-variant test fixtures M3a impl MUST land before any of
A6 / A7 / A8 / A9 are considered enforced in code.  Sized as
M3a impl gates, not research-only acceptance.

### 9.1 App::boot per-FN decision-tree branch matrix (ADR-M3-A7)

Each branch of §4.3 (a)–(f) MUST have an explicit test
fixture.  The PRRO_GATE-ah8 acceptance test (preserve
`shift_state=Opened`) is a subset of branch (e); the other
branches need their own coverage.

| # | §4.3 branch | Pre-condition fixture | Action | Acceptance assertions |
|---|-------------|-----------------------|--------|----------------------|
| 1 | **(a) FN row absent** | DB with no `node_state` row for `fn=X` | `App::boot` → reconciliation phase for X | `node_state` row inserted with `(mode=Online, shift_state=Closed, next_lnd=1)`; audit `NODE_STATE_INITIALISED` INFO present |
| 2 | **(b) FN row present + ONLINE + no pending docs** | `node_state(fn=X, mode=Online, shift_state=Opened)`; zero pending docs | `App::boot` → reconciliation phase for X | row UNCHANGED (same `mode`, same `shift_state`, same `next_lnd`); audit `NODE_STATE_BOOT_IDEMPOTENT` INFO present; **no `upsert_initial` call observed** (provider spy) |
| 3 | **(c) FN row present + pending docs** | `node_state(fn=X, mode=Online)`; one pending doc per state in {PREPARED, SIGNED, SENDING, SENT, KVT1, KVT2, ERROR_RETRYABLE} | `App::boot` → reconciliation phase for X | each doc transitions per §3 rules (PREPARED → re-driven; SENDING → ErrorRetryable; SENT → last_chk-queried; etc.); audit `NODE_STATE_BOOT_RECONCILED` INFO with histogram |
| 4 | **(d) FN row present + OFFLINE mode (M3a refusal)** | `node_state(fn=X, mode=Offline)` | `App::boot` → reconciliation phase for X | `App::boot` returns Err with explicit message «FN $X is in OFFLINE mode — start with --recover-offline M3b CLI»; audit `NODE_STATE_BOOT_OFFLINE_REFUSAL` ERROR; **node_state row UNCHANGED** (offline session pointer preserved); process exits non-zero |
| 5 | **(e) Mid-transition shift orphan (PRRO_GATE-ah8 acceptance verbatim)** | `node_state(fn=X, mode=Online, shift_state=Opened)`; one pending SHIFT_OPEN doc in SENT state | `App::boot` → reconciliation phase for X | `node_state.shift_state` STILL `Opened` after boot (no overwrite); doc transitions per §3 SENT rule (last_chk re-query); **no `upsert_initial` invocation** (provider spy verifies) |
| 6 | **(e) Orphan with NO pending doc** (corruption signal) | `node_state(fn=X, mode=Online, shift_state=Opening)`; ZERO pending docs (operator deleted the SHIFT_OPEN doc, or DB was hand-edited) | `App::boot` → reconciliation phase for X | `shifts.state` for the orphan shift transitions to `Error` (per W0-1 §2.2 `any → ERROR` whitelist); audit `SHIFT_BOOT_ORPHAN_ERROR` CRITICAL |
| 7 | **(f) BLOCKED / STOP_MODE / CRYPTO_DEGRADED preserve** | `node_state(fn=X, mode=Blocked)` (and parametrised over `StopMode`, `CryptoDegraded`) | `App::boot` → reconciliation phase for X | row UNCHANGED for each mode; appropriate audit (INFO for BLOCKED; no recovery action for STOP_MODE; CRYPTO_DEGRADED leaves breaker open per `runtime/container.py:265-281` parity) |
| 8 | **PRAGMA quick_check fails (per §4.2 step 3)** | DB file with deliberate corruption (e.g. truncated mid-page) | `App::boot` boot phase | `App::boot` returns Err before any reconciliation phase runs; CRITICAL log `DB_INTEGRITY_CHECK_FAILED` with quick_check output emitted; **NO writes to `node_state` / `shifts` / `audit_log` after the failed probe** (DB corruption is not compounded); `/health/startup` returns 503 with the failure reason |
| 9 | **Idempotency (App::boot run twice)** | Result of any of branches (a)-(f) above as starting state | Run `App::boot` twice in immediate succession | Second run is observationally equivalent to branch (b) for previously-completed FNs (no-op); for branch (d) refusal, second run also refuses; counter-state preservation per §4.5 |

These 9 fixtures are the structural proof of ADR-M3-A7 +
PRRO_GATE-ah8 closure.  Without them, the per-branch decision
tree is documentation, not enforced behaviour.

### 9.2 DpsError → state-machine routing table-driven tests (ADR-M3-A6 + ADR-M3-A8)

Per §2 main table (8 DpsError variants) + §2.1 sub-table
(18 CheckResponse.Status values).  Use a parametrised test
(`#[rstest]` or hand-rolled `for` loop over a fixture-vector)
that drives EACH variant / status code through stage 4 with a
DpsChannel mock returning the wire shape that produces that
variant, and asserts the DocState transition + audit + retry
counter outcome.

**§2 main table coverage (8 variants × 1 fixture each minimum):**

| # | DpsError variant | Mock setup | Acceptance |
|---|------------------|------------|------------|
| 1 | `Transport(String)` | DpsChannel mock returns `tonic::Status::with_code(GRPC_UNAVAILABLE)` | doc transitions Sending → ErrorRetryable; recovery_attempts incremented; backoff schedule honoured per §2 main row 1 |
| 2 | `Authorization { code: -1, kind: DocumentReject, message }` (post M2/W3 amendment) | DpsChannel mock returns CheckResponse with `status = -1` | doc transitions Sending → Rejected; ERROR audit; recovery_attempts NOT incremented |
| 3 | `Authorization { code: -13, kind: FiscalNumberNotRegistered, message }` | DpsChannel mock returns CheckResponse with `status = -13` | doc transitions Sending → ErrorRetryable; on next retry tick, ErrorRetryable → RequiresManualReconciliation (terminal); ERROR audit |
| 4 | `Authorization { code: -14, kind: FiscalNumberNotRegistered, message }` | DpsChannel mock returns CheckResponse with `status = -14` | same as #3 (per row 14 sub-table) |
| 5 | `Decode(String)` | DpsChannel mock returns CheckResponse with `status = 0` (UNKNOWN) | one `last_chk` reconciliation probe issued; mock returns NotFound; doc transitions Sending → RequiresManualReconciliation **directly** (no bounded retry — per §2 main Decode row); WARN log with raw decoder error |
| 6 | `Server { code, message }` | dispatched per §2.1 sub-table — see below |  |
| 7 | `NotFound` | DpsChannel mock for `by_server_fiscal_no` returns "no record" | typed result returned to caller; no DocState transition (this is a query-result shape, not a submit failure) |
| 8 | `ServerFiscalIdMismatch { expected_id, actual_id }` | DpsChannel mock for `last_chk` returns id different from expected | doc transitions Sending → ErrorRetryable → RequiresManualReconciliation; CRITICAL audit `DPS_FISCAL_ID_MISMATCH` carrying both ids; FN-chain integrity audit emitted |
| 9 | `QueryNotSupported(&'static str)` | M3a write-path triggers `query_by_local_identity` (out-of-band call) | INTERNAL log with the static reason; reject the calling code path (this is a wrapper-side surface, not a DocState transition) |
| 10 | `Internal(String)` | Force a wrapper-side internal error (e.g. mock channel returns a malformed proto that the dto decoder cannot route) | doc transitions Sending → ErrorRetryable → RequiresManualReconciliation; CRITICAL audit `DPS_WRAPPER_INTERNAL` |

**§2.1 Server-status sub-table coverage (12 codes that surface
to `Server { code, .. }` directly; 11 fixtures with `-2` and
`-15` each having two variants and `-7..-10` collapsed into
one parametrised XML-class fixture):**

The 12 codes NOT pre-routed by dto are `-2`, `-3`, `-5`, `-6`,
`-7`, `-8`, `-9`, `-10`, `-11`, `-12`, `-15`, `-16`.  The 4
dto-pre-routed codes (`-1`, `-4`, `-13`, `-14`) are covered
in the §2 main-table fixtures #2-4 + #1 (Authorization /
Transport routing).  Each Server-routed code MUST have its
own fixture proving the §2.1 row's "M3 retry policy" +
"Source-state route" cells:

| # | Status code | Mock | Acceptance per §2.1 row |
|---|-------------|------|-------------------------|
| 11 | `-2` ERROR_CHECK | non-shift-mgmt doc_type | doc Sending → Rejected; CRITICAL audit |
| 12 | `-2` ERROR_CHECK | doc_type ∈ {SHIFT_CLOSE, Z_REPORT}; error_message ARBITRARY (R-W10-F3 amendment dropped substring gate) | one `last_chk` probe; route per outcome |
| 13 | `-3` ERROR_SAVE | DpsChannel mock returns `status = -3` | Sending → ErrorRetryable; under Pattern B re-drive: ErrorRetryable → Sending → wire (NOT direct → Sent) |
| 14 | `-5` ERROR_TYPE | mock returns `status = -5` | Sending → Rejected; CRITICAL audit |
| 15 | `-6` ERROR_NOT_PREV_ZREPORT | mock returns `status = -6` | Sending → ErrorRetryable → RequiresManualReconciliation chain |
| 16 | `-7`/`-8`/`-9`/`-10` (XML-class errors) | parametrised over the four codes | each: Sending → Rejected; CRITICAL audit (M3 builder bug indicator) |
| 17 | `-11` ERROR_OFFLINE_168 | mock returns `status = -11` | Sending → Rejected; node_state.mode → BLOCKED (per W0-1 §2.4); CRITICAL audit |
| 18 | `-12` ERROR_BAD_HASH_PREV | mock returns `status = -12` + error_message with `store {64hex}` regex | one MAC-recovery attempt: re-derive MAC, re-sign, re-send via Pattern B; on success → Sent → Kvt1; on failure → Rejected |
| 19 | `-15` ERROR_NOT_OPEN_SHIFT, doc_type ∈ {SHIFT_CLOSE, Z_REPORT} | mock returns `status = -15` | one `last_chk` probe; if match → drive forward; if not → Rejected |
| 20 | `-15` ERROR_NOT_OPEN_SHIFT, doc_type non-shift | mock returns `status = -15` | Sending → Rejected directly (M3 §1.4 row 10 rationale: ingress guard bug) |
| 21 | `-16` ERROR_OFFLINE_ID, M3a (ONLINE-only) | mock returns `status = -16` | Sending → Rejected; ALERT audit; **no offline failover invoked** (M3a is ONLINE-only carve-out per §5) |

These 21 fixtures are the structural proof of ADR-M3-A6 +
ADR-M3-A8 + the ADR-M3-A9 retry-path policy.  Without them
the §2 / §2.1 routing tables are documentation, not enforced
behaviour.

### 9.3 Deterministic-replay invariant tests (ADR-M3-A8 + §6)

Per §6.1-§6.7, each pending-state crash-point MUST have a
deterministic-replay test:

| # | Crash point | Pre-condition | Action | Acceptance |
|---|-------------|---------------|--------|------------|
| 1 | §6.1 PREPARED | doc in PREPARED, no signed artefact | App::boot → re-drive | final state = ACK; canonical XML byte-equal to uninterrupted run (M2 W4 byte-equiv contract) |
| 2 | §6.2 SIGNED | doc in SIGNED, signed artefact persisted | App::boot → re-drive via stage 4 entry | final state = ACK; wire send is fresh first send (no duplicate hazard); Pattern B Sending CAS executes |
| 3 | §6.3 SENDING | doc in SENDING, wire-state ambiguous | App::boot → CAS Sending → ErrorRetryable | doc in ErrorRetryable; **DpsChannel mock records ZERO send_chk invocations**; audit `crash_resume_sending_to_error_retryable` present; operator-decision-required signal surfaced |
| 4 | §6.4 SENT case (a) | doc in SENT; mock `last_chk` returns matching id | App::boot → CAS Sent → Kvt1 | final state = ACK |
| 5 | §6.4 SENT case (b) | doc in SENT; mock `last_chk` returns non-matching id | App::boot → CAS Sent → ErrorRetryable → RequiresManualReconciliation | final state = RequiresManualReconciliation |
| 6 | §6.4 SENT case (c) | doc in SENT; mock `last_chk` returns NotFound | App::boot → CAS Sent → ErrorRetryable; on retry tick → ErrorRetryable → Sending → wire | final state = ACK (after re-send completes); two-step transition observed |
| 7 | §6.5 KVT1 | doc in KVT1; mock returns KVT2 on poll | App::boot → CAS Kvt1 → Kvt2 → Ack | final state = ACK |
| 8 | §6.6 KVT2 (canonical example justifying KVT2-in-pending) | doc in KVT2 | App::boot → CAS Kvt2 → Ack | final state = ACK; `node_state.last_known_unsigned_xml_sha256` updated; **no DPS query made** (KVT2 is protocol-final) |
| 9 | §6.7 ERROR_RETRYABLE | doc in ErrorRetryable; recovery_attempts < max | App::boot → re-drive (re-sign or re-send via Pattern B path) | terminal state determined by retry budget + DPS responses; deterministic per `(persisted recovery_attempts, observable DPS state)` mapping |

These 9 fixtures prove the §6 invariant — recovery converges
to the same final state as an uninterrupted run for any given
DPS reality at recovery time.
