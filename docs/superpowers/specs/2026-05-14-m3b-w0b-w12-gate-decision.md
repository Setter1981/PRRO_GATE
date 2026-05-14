# M3b W0b Gate Decision - Authoritative Per-Doc KVT2 Evidence

**Status:** ACCEPTED
**Date:** 2026-05-14
**Scope anchor:** `docs/superpowers/plans/2026-05-14-m3b-implementation.md` Task 0b / W12
**bd anchors:** `PRRO_GATE-9qd.2`, `PRRO_GATE-5js`

---

## Verdict

**YES - with explicit scope restriction.**

Authoritative per-doc KVT2 evidence is available through the `lastChk(fn_sign)` +
`response.id` match pattern, but only for the latest document on a fiscal number
and only under the ADR-M3-A10 single-writer preconditions.

This is enough for M3b's offline backlog drain:

1. W9 drains one fiscal number sequentially in strict `lnd` ASC order.
2. W2 makes the drain path acquire the same App reconciliation mutex as boot reconciliation.
3. For a pure offline doc, W9 calls `stage_send::run` for `doc_i` and does not start `doc_i+1` until W12 confirms `doc_i`.
4. Immediately after `stage_send::run` records `doc_i.server_fiscal_no`, W12 calls `lastChk(fn_sign)`.
5. Because no same-FN send can interleave in that window, `lastChk` is expected to report `doc_i` as the latest DPS document.
6. `response.status == OK`, `response.id == doc_i.server_fiscal_no`, and non-empty `response.data_sign` together provide the per-doc KVT2 evidence needed to advance the local state to `Ack`.

This is not enough for arbitrary boot-time polling of stale `Kvt1` documents.
If a historical/pre-existing `Kvt1` doc is no longer the latest document on the
fiscal number, `lastChk(fn_sign)` can return a later document and cannot prove
KVT2 for the stale one. Those docs remain handled by `passive_hold_kvt1` and are
deferred to M3c/M4 or a later full WebCheck-parity recovery surface.

---

## Evidence Sources

- `rust/prro/proto/fiscal_server.proto:8-13` defines the DPS surface: `sendChkV2`, `lastChk`, `ping`, `statusRro`, `infoRro`.
- `rust/prro/proto/fiscal_server.proto:32-34` shows `lastChk` takes only `rro_fn_sign`.
- `rust/prro/proto/fiscal_server.proto:36-61` shows `CheckResponse` carries `id`, `status`, `id_sign`, `data_sign`, and `error_message`.
- `docs/superpowers/specs/2026-05-04-m2-w0-1-dps-wire.md:123-126` maps `id` to `DpsAck.server_fiscal_no`, `id_sign` to DPS signature of `id`, and `data_sign` to DPS signature of the full payload.
- `docs/superpowers/specs/2026-05-04-m2-w0-1-dps-wire.md:141-145` records the recovery match rule: `response.id == transport_request_id`.
- `PRRO_GATE-5js` records the same WebCheck pattern: ByServerFiscalNo is `lastChk(fn_sign)` plus `response.id` match, not a direct server-id lookup.
- `docs/superpowers/specs/2026-05-12-adr-m3-a10-global-single-writer.md` records the current global-single-writer invariant and the carry-forward that W2 closes for direct boot-phase callers.

---

## Three-Condition Table

| RPC | Per-doc identifier match | Signed payload | No-side-effect poll | Gate result |
|---|---|---|---|---|
| `sendChkV2` | YES: response carries `id` | YES: `id_sign`, `data_sign` | NO: fiscalises/sends the document | NO |
| `lastChk` | YES, conditionally: `response.id == doc.server_fiscal_no`, latest-doc only | YES: `id_sign`, `data_sign` | YES: recovery/status read | YES for W9 in-drain latest-doc confirmation only |
| `ping` | NO: liveness only | n/a | YES | NO |
| `statusRro` | NO: RRO-wide status | NO | YES | NO |
| `infoRro` | NO: RRO-wide status/info | NO | YES | NO |

---

## Scope Contract

### In Scope

- W12 implements **in-drain KVT2 confirmation via `lastChk`**.
- W12 is called by W9 immediately after `stage_send::run` records server-side evidence for the current backlog document and before the drain attempts any later document on the same fiscal number.
- W12 may advance only the document currently being drained.
- W12 success requires all of:
  - `lastChk.status == OK`;
  - `lastChk.id == doc.server_fiscal_no`;
  - `lastChk.data_sign` is present and non-empty;
  - the drain still holds the W2/ADR-M3-A10 single-writer discipline, so no same-FN send interleaved between `stage_send(doc_i)` and `lastChk(fn_sign)`.
- On success, W12 persists/audits the evidence and advances the local state through the existing whitelisted `Kvt1 -> Kvt2 -> Ack` ladder, reusing M3a `stage_finalize` for the final `Kvt2 -> Ack` step.

### Out Of Scope

- Boot-time arbitrary `Kvt1` polling.
- Historical/pre-existing stale `Kvt1` documents whose `server_fiscal_no` may no longer be the latest DPS document for the FN.
- Full WebCheck parity.
- `ByLocalIdentity` recovery; M2 W0-1 explicitly records that the gRPC contour has no by-content lookup over canonical hash + LND.
- Any W12 path that synthesizes KVT2 evidence from `statusRro`, `infoRro`, `ping`, counters, or RRO-wide aggregates.

---

## Required Plan Amendments

The M3b implementation plan is amended in the same PR as this spec:

- W12 is renamed/scoped from general "active KVT2 polling" to **in-drain KVT2 confirmation via `lastChk`**.
- W12 no longer deprecates or retires `passive_hold_kvt1`.
- `passive_hold_kvt1` remains the primary boot-time handler for arbitrary/stale `Kvt1` documents.
- W9 records the hard precondition that no same-FN send may interleave between `stage_send(doc_i)` and `lastChk(fn_sign)`.
- M3b exit criteria say final `Ack` applies to the M3b offline-drain backlog only, not all historical/pre-existing `Kvt1` documents.

---

## Failure Semantics

- `lastChk.status != OK` -> typed W12 failure; doc state unchanged from the last durable local state.
- `lastChk.id != doc.server_fiscal_no` -> typed mismatch/ambiguous recovery error; no `Ack`.
- missing/empty `data_sign` -> typed missing-evidence error; no `Ack`.
- any lost CAS while advancing `Kvt1 -> Kvt2 -> Ack` -> typed replay/concurrency error; no synthesized success.

Every failure emits an audit row with the DPS error/mismatch class and leaves the document replayable.

---

## Verification Target

W12 implementation must add focused tests for:

- happy path: `stage_send(doc_i)` then `lastChk` match advances `doc_i` to `Ack`;
- mismatch path: `lastChk.id` differs from `doc.server_fiscal_no`, no `Ack`;
- missing evidence path: empty `data_sign`, no `Ack`;
- stale boot-time `Kvt1` path: boot reconciliation keeps using `passive_hold_kvt1`;
- interleave guard: the drain cannot send `doc_i+1` before W12 completes `doc_i`.
