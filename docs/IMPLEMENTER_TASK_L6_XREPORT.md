# Implementer Task — L6 X-report (поточний звіт / mid-shift snapshot)

**Base:** branch from `origin/main` (post-#260/#261 merge — has EPZ/L5/RAGE-W1). New worktree.
**Method:** strict RED-first TDD. **LOCAL only, no push** until operator says (batch model).

## What X-report is (the design, from research)
A **local-only, side-effect-FREE read** of the current open shift's turnover, returned on
demand. It is NOT a fiscal document: it is **never sent to DPS**, consumes no fiscal number,
advances no chain, writes no row, closes no shift. It is the read-only sibling of the Z-report,
reusing the SAME aggregation but as a pure snapshot.

Today `CommandType::XReport → CommandClass::ReadOnly → handler.rs:447` returns a hard 422
`READ_ONLY_COMMAND`. L6 replaces that refusal with a real read path.

## Decided design (locked — do not re-litigate)
1. **Reuse `aggregate_z_payload(pool, fiscal_number)`** (`convert.rs:1278`, already `pub`) — the
   SSOT for shift turnover (SELL/RETURN payforms + `<IO>` service-io + `<EPZ>` + `<TXS>` tax +
   `<NC>` counts). Resolves the open shift itself (no shift_id from caller). NO duplicate
   aggregation.
2. **+ cash-on-hand** — include the running cash balance (`cash_ledger::derive_cash_on_hand` over
   the shift's durable ledger). A mid-shift check exists to show the drawer; include it.
3. **Response = JSON** (`XReportPayload`) — turnover-by-payform + IO + EPZ + tax + counts +
   cash-on-hand. **NO DPS-wire XML** (X is local-only; no `<MAC>`, no chain fields, no DPS
   round-trip). If a print-body is needed later that's a separate follow-up.
4. **Handled in the POST path** — replace the `CommandClass::ReadOnly` 422 at `handler.rs:447`
   with a dispatch to a new `handle_x_report(...)` returning **200 + XReportPayload**.
5. **Bimodal for free** — the aggregation reads the durable ledger (`ACK` + `OFFLINE_LOCAL_ACK`);
   works identically in `Opened` and `OpenedLocalPendingDrain`. No offline special-casing.
6. **No open shift → 422 `NO_OPEN_SHIFT`** (typed error, not a panic). Valid only when a current
   open shift exists.

## The hard invariant — X is SIDE-EFFECT-FREE (this is the whole point)
X-report MUST NOT: enter `ingress_inbox`, create a `fiscal_documents` row, consume an lnd,
advance the MAC seed, transition shift state, sign anything (no sidecar/crypto), call DPS
(no network), or consume an offline code. It is a pure SELECT.

## V0 — RED-first pins (write + watch fail FIRST)
1. **Positive:** POST `CommandType::XReport` on an open shift with prior SELL/RETURN/service-io/
   EPZ → **200** with an `XReportPayload` whose turnover matches the seeded docs (payforms, IO,
   EPZ, tax, counts, cash-on-hand). NOT 422.
2. **Side-effect-free (the teeth):** after the X-report call, assert **0 new `ingress_inbox`
   rows, 0 new `fiscal_documents` rows, `last_local_number` unchanged, MAC seed unchanged,
   shift state unchanged**. (This pin is the core invariant.)
3. **No open shift → 422 `NO_OPEN_SHIFT`**, row-less.
4. **Bimodal:** X-report on an `OpenedLocalPendingDrain` (offline) shift returns the same
   turnover from the durable ledger (incl `OFFLINE_LOCAL_ACK` docs).

## V1 — implement (minimal diff)
`handler.rs:447` ReadOnly arm → `handle_x_report(fn, pool, ...)`; new `handle_x_report` reads the
current open shift, `aggregate_z_payload` + cash-on-hand → `XReportPayload` (new response DTO,
mirror `ZReportPayload` MINUS chain fields `local_number`/z-number/MAC, PLUS cash-on-hand) → 200.
No write-path, no stage_*, no inbox. `NO_OPEN_SHIFT` error variant + 422 mapping.

## V2 — fuzzer (MANDATORY — the side-effect-free probe the alphabet lacks)
`Op::XReport` (read-only, insertable at ANY point, carries no DpsScript). Model: `apply_x_report`
predicts `ExpectedOutcome` = **no mutation** (nothing changes: no lnd, no seed, no docs, no shift
state) AND the returned turnover matches the model's tracked totals. Oracle (6-point + more):
(1) no lnd consumed, (2) no seed advance, (3) no `fiscal_documents` row, (4) no `ingress_inbox`
row, (5) no shift-state transition, (6) turnover snapshot == model totals; plus **idempotent**
(two X in a row identical) and **commutative** (X between two ops doesn't change the next op's
result). **Teeth (prove empirically):** revert the side-effect-free property (e.g. make X consume
an lnd / write a row) → a seeded harness with `Op::XReport` goes RED. Include the seeded harness.

## Invariants — HOLD (state each)
- **No-network/crypto-in-txn** — vacuously held (X has NO write txn; pure SELECT).
- **z-quiescence / advance-at-SEND / D2 / idempotency** — untouched (X mutates nothing).
- **SSOT** — turnover from `aggregate_z_payload`, not a re-implementation.

## Verification gate (run YOURSELF, final commit, actual numbers)
`cargo nextest run -p prro --features test-support` (0 failed) + `cargo fmt -p prro -- --check`
+ `cargo clippy -p prro --all-targets --features test-support -- -D warnings` (0). Plus the
side-effect-free teeth-proof (revert → RED). ⚠ TMPDIR note: if local tests flake on temp-DB
setup, check `/dev/shm` isn't full (killed runs leak `.tmp*` dirs).

## Delivery (required 7). Commit per slice (V0→V1→V2). Do NOT push — architect verifies +
2-lens reviews; batch pushes on operator command.

## Known GAP (verify during impl, don't guess)
WebCheck X-report wire not confirmed local-only vs a `<X>` element — but since X does NOT go to
DPS in our design, no DPS XML is built; if reality shows X must be sent, STOP and report.
