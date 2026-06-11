# RS-3 A2.1b-core — inline `fiscalize` orchestrator (SELL/RETURN only)

**Date:** 2026-06-09
**Status:** IMPL SPEC (operator-signed scope; TDD in progress)
**Predecessors (MERGED):** A1 · A1Z · A2.0 · A2.1a · A3 · A4 (FnWriteGate) · C1 · C2
**Successors:** A2.2 (shift-link + `run_with_shift_transition`) → A2.3 (offline/refused polish) → A2.4 (binding flip)

Authoritative design: `2026-06-09-rs3-a2-implementation.md` §3/§6/§7. This is the
current-code execution checklist for the operator-signed **A2.1b-core** slice.

---

## 0. Locked scope (operator-signed 2026-06-09)

- **SELL / RETURN only.** SHIFT_OPEN / Z_REPORT / SHIFT_CLOSE are **fail-closed**
  in core (no semi-live shift path).
- Chain: `build_canonical → acquire lease (stage_acquire) → stage_sign → dispatch
  → online send → advance_to_ack (A2.1a) → stage_finalize::run`.
- Offline-local-ack + refusal/error arms **included** (they are part of the
  dispatch/stage outcome-map).
- Production binding (`UnimplementedWritePath`→`InlineWritePath`) **NOT** touched
  — A2.1b-core lands **dormant** (zero production blast radius until A2.4).
- A2.2 (shift-link + finalize hook) is a **separate PR right after** core.

### A4 gate decision (#6 — resolved)

`inline::run` takes the A4 per-FN gate guard as an explicit proof parameter
`fn_gate: &tokio::sync::OwnedMutexGuard<()>`. A2.4's binding MUST
`App::acquire_fn_gate(&row.fiscal_number).await` and hold the guard across the
`inline::run` call — so the binding **cannot** invoke core without first
acquiring the per-FN gate (invariant #2 at the runtime level). The DB lease CAS
(`stage_acquire` NEW→PROCESSING) remains the durable single-writer backstop;
the gate is the liveness/contention refinement (per `runtime/fn_gate.rs` docs).
No change to merged A4. `inline::run` takes individual deps (pool, pool_secure,
dps, sign_ctx) — NOT `&App` — so it stays unit-testable.

---

## 1. `inline::run` signature

```rust
// services/write_path/inline.rs  (NEW)
pub async fn run(
    pool: &SqlitePool,
    pool_secure: &SqlitePool,
    dps: &dyn DpsChannel,
    sign_ctx: &SigningContext,
    fn_gate: &tokio::sync::OwnedMutexGuard<()>, // A4 proof (held by caller for this FN)
    row: &InboxRow,
) -> Result<FiscalOutcome, FiscalError>;
```

`request_id = row.request_id`. `driver_id = row.driver_id` (REQUIRED — `None` →
`Internal/INVALID_PAYLOAD`, the seam null-handling contract; A1's
`build_canonical` already enforces the field set, so route its `BuildReject`).

---

## 2. The ladder (every arm anchored to a mapper / acceptance)

```
run(row):
  if is_z_class(row.operation_type):              # Z_REPORT/SHIFT_CLOSE
      lease+terminalise inbox(REJECTED)+audit; NO fiscal_documents
      return Err(ZSurfaceNotReady{request_id})    # 501  (ensure_full_z_surface_ready() is Err today)
  if is_shift_open(row.operation_type):           # SHIFT_OPEN (out of core)
      lease+terminalise inbox(REJECTED)+audit; NO fiscal_documents
      return Err(ShiftGuardRefused{code=SHIFT_OPEN_NOT_IN_CORE?})   # see §4 open-Q
  cmd = build_canonical(row)                       # A1; BuildReject → map_build_reject (Internal/500) + terminalise
  acq = stage_acquire::run(pool, pool_secure, driver_id, request_id, cmd)   # anyhow → Internal/500
  match acq:
    Noop          → replay::resolve_replay(ledger)  # decision e: terminal→outcome / in-flight→202 / empty→Internal; audit Critical (unexpected under A4)
    Rejected{r}   → map_rejection(r)  (inbox ALREADY terminalised by stage_acquire)
    Proceed(ctx)|Resumed(ctx):
      sign = stage_sign::run(pool, sign_ctx, ctx)
        Err(e) → terminalise inbox + map_sign_error(e)            # SignFailure/500 | Internal/500
      route = dispatch_post_sign(pool, doc_id, fn)               # anyhow → Internal/500
      match route:
        Refused(r) → terminalise inbox + map_dispatcher_refusal(r)   # OfflineRefused/503
        Offline{outcome} → match outcome:
            Applied{..}  → Ok(FiscalOutcome{document_state: OfflineLocalAck, fiscal_id: None, ..})  # 200, NOT Err
            Refused(rr)  → terminalise inbox + map (offline-ack refusal → FiscalError)  # see §4 open-Q
        Online{..}:
          send = stage_send::run(pool, dps, doc_id, Some(sign_ctx))
            Err(e) → terminalise inbox + map_send_error(e)        # SignFailure | Internal/500
          match classify_send_outcome(send):
            Reject(fe)        → terminalise inbox + Err(fe)        # DpsRejected/422 | Internal/500
            InProgress        → Ok(FiscalOutcome{document_state: ErrorRetryable, ..})  # 202, NOT Err
            ResolveReplay{ob} → replay::resolve_replay(ledger)     # NOT a phantom 500
            Proceed{server_fiscal_no}:
              confirm = advance_to_ack(pool, doc_id, kvt1_raw_bytes?, &server_fiscal_no, doc.state=Sent, attempt_no)
                  ⚠ OPEN-Q (§4): advance_to_ack needs kvt1_raw_bytes (lastChk evidence). The ONLINE
                    ladder reached `Sent` but has NOT fetched lastChk. Either (a) the online ladder
                    runs an inline lastChk-confirm to get the evidence then advance_to_ack, or (b) the
                    inline window does NOT confirm and returns 202 (drain/B1 confirms). Plan §0(a)
                    says inline drives KVT2-confirm; A2.1a's advance_to_ack REQUIRES the evidence bytes.
                    → RESOLVE before writing the Proceed arm.
                Ok(())                  → Ok(FiscalOutcome{document_state: Ack, fiscal_id: Some(server_fiscal_no), ..})  # 200
                Err(StructuralDrift)    → terminalise inbox + Internal/500
                Err(Database|Infra)     → resolve against ledger (doc still Sent → 202), NOT blind 500
              # inline-window timeout (if no confirm) → Ok(FiscalOutcome{document_state: Sent}) → 202
```

Every `Err(FiscalError)` arm except the stage_acquire-Rejected/Noop ones MUST
terminalise the inbox itself (the stage did not). The §6 four-variant gate test
(A2.4) pins this; A2.1b-core adds the per-arm assertions.

---

## 3. tx-boundary (invariant #1) — gate-outer / tx-inner

- A4 gate held by caller across the whole `run` (MAY span network/crypto).
- `build_canonical` pure CPU. `stage_acquire` / `stage_sign` / `stage_send` /
  `advance_to_ack` / `stage_finalize` each own their short `with_immediate`
  envelopes; DPS send + crypto sign happen strictly BETWEEN envelopes. `run`
  itself opens NO `with_immediate` — it only chains stages + reads the ledger
  (`resolve_replay`) + terminalises the inbox (each its own short tx).

---

## 4. RESOLVED decisions (operator-signed 2026-06-09 + arch-planner design)

1. **KVT1 evidence — Q1 = (b), LOCKED.** After `StageSendOutcome::Sent`, the inline
   ladder does an inline lastChk by `server_fiscal_no` → `data_sign` → `advance_to_ack(
   kvt1_raw_bytes=data_sign, doc_state_at_entry=Sent, attempt_no)`. `stage_send`
   untouched. **NEW thin wrapper** (arch-planner): `online_confirm(dps, fn_sign,
   server_fiscal_no) -> InlineConfirmOutcome{Acked(Vec<u8>)|Hold|Drift}` in `inline.rs`,
   body reuses `classify_check_result(result, Kvt2ConfirmSource::SentFresh, None)` (no
   BootError, no classification dup; `SentNotFoundDowngrade` is unreachable for SentFresh
   → fail-loud). Mapping: `Acked(bytes)`→`advance_to_ack`→on `Ok`→`FiscalOutcome{Ack,
   fiscal_id:Some}`; `Hold`/timeout/no-evidence→`Ok(FiscalOutcome{Sent})`/202 (NEVER 500,
   NEVER fake-ACK); `Drift`→terminalise inbox + `Internal`/500. `advance_to_ack`
   `Err(StructuralDrift)`→terminalise+Internal/500; `Err(Database|Infrastructure)`→resolve
   against ledger (still `Sent`→202), not blind 500. **`inline::run` gains `fn_sign:
   &CheckSignBlob` dep.** **GOTCHA**: `classify_send_outcome` drops `attempt_no` on
   `Proceed` (`inline_map.rs:225-227`) — capture it from `StageSendOutcome::Sent`
   separately (advance_to_ack audit needs it).
2. **Inbox + fiscal_documents on failure — confirmed.** On a real hard-refusal the
   write-path OWNS the inbox row → it becomes terminal/audited. `fiscal_documents` gets
   NO "failed ingress" issued-ledger residue; NEVER mint a NEW fiscal doc just for a
   refusal. If the doc already reached PREPARED, proceed by the state machine: a terminal
   DPS reject MAY leave the existing `fiscal_documents.state=Rejected` (an existing
   write-path artifact, NOT an issued receipt) — replay MUST read it as failure, not
   success.
3. **SHIFT_OPEN out of core — confirmed.** `ShiftGuardRefused{code="SHIFT_OPEN_NOT_SUPPORTED"}`
   → 422 (do NOT leak internal slicing into the wire contract — no `SHIFT_OPEN_NOT_IN_CORE`;
   no Internal, no NotImplemented). Add the code to `inline_map::codes` (fenced) +
   round-trip-422 test. Temporary fail-closed until A2.2. (Z-class → `ZSurfaceNotReady`/501.)
4. **Offline-ack `Refused` — confirmed.** → `OfflineRefused{code}`/503 via an A2.0 mapper.
   If the precise code is missing, add it to `inline_map::codes` (fenced) + round-trip
   HTTP-class test — never an ad-hoc string.

**Extra acceptance pin (operator):** a `Sent` doc WITHOUT a successful inline lastChk
(`online_confirm` = Hold) does NOT become 500 and is NOT fictitiously ACK'd; ONLY
`Acked(data_sign)` leads into `advance_to_ack`.

---

## 5. Test list (TDD)

§6 pinning + minimal acceptance:
1. **online ACK** — Online node + open shift + DPS accepts + lastChk evidence →
   `FiscalOutcome{document_state: Ack, fiscal_id: Some}`.
2. **transient → 202** — stage_send `ErrorRetryable` → `Ok(document_state:
   ErrorRetryable)` (NOT terminal 500); and `ConfirmError::Database` after `Sent`
   → resolve-ledger → 202.
3. **offline-local-ack** — node Offline → `Ok(document_state: OfflineLocalAck)`
   (NOT Err).
4. **hard refusals terminalise inbox, no fiscal_documents** — for `DpsRejected`,
   `OfflineRefused`, `ShiftNotOpen` (acquire-Rejected), assert inbox non-NEW +
   audited (the four-variant gate, scoped to core's reachable refusals).
5. **Z fail-closed** — Z_REPORT row → `Err(ZSurfaceNotReady)` + inbox REJECTED +
   no fiscal_documents.
6. **replay-resolve** — `Noop` / `ResolveReplay` resolve against the ledger (no
   phantom 500).
7. **lease single-writer** — concurrent `run` for same FN/distinct receipts:
   exactly one `Proceed` (NON-IDENTITY fixtures).
8. **invariant #1** — assert no foreign IO inside any `with_immediate` (the
   static scanner already guards; add the §7 reasoning to the PR).

---

## 6. Decomposition within A2.1b-core (TDD increments)

1. Skeleton + signature + the online-ACK happy path (test 1) — **first**.
2. transient/202 + replay-resolve arms (tests 2, 6).
3. offline-ack arm (test 3).
4. refusal/error arms + inbox-terminalise (tests 4, 5) — resolves §4 Q2/Q3/Q4.
5. lease single-writer + #1 reasoning (tests 7, 8).
